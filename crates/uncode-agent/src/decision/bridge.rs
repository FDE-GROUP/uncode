//! ModelBridge — 将 uncode-ai::Model 的语义属性映射到本体字段
//!
//! ## 定位
//!
//! ModelBridge 是 `uncode-ontology`（本体 schema）与 `uncode-ai::Model`（运行时模型实例）
//! 之间的桥接层。它不修改 Model struct，而是在查询时构建本体视角的模型快照。
//!
//! ## 设计决策
//!
//! - `uncode-ontology` 是叶子 crate，不依赖 `uncode-ai`（避免循环依赖）
//! - 桥接代码放在 `uncode-agent`（同时依赖两者）
//! - 本体定义 schema（LLM 实体类型的字段/约束），Model 是实例数据
//!
//! 参见 `docs/technologies/UNCODE_LLM_ONTOLOGY_INTEGRATION.md`

use std::collections::HashMap;
use std::sync::Arc;

use uncode_ai::api_types::InputModality;
use uncode_ai::model::Model;

use super::firewall::{ValidationError, ValidationRule, ValidationVerdict};
use super::types::{DecisionVerdict, ParsedAction};

/// 模型桥接 — Model ↔ 本体字段的映射工具
///
/// 使用 unit struct 而非 free functions 以便将来添加 trait 实现
/// （如 `Default`、测试 mock trait），同时将相关函数组织在单一命名空间下。
pub struct ModelBridge;

impl ModelBridge {
    /// 从 Model 构建本体视角的字段 map
    ///
    /// 返回的 key 与 `uncode-ontology` LLM EntityDef 的 FieldDef.name 一一对应：
    /// model_id, provider, context_window, max_output_tokens,
    /// supports_vision, supports_reasoning, supports_tools, api_protocol,
    /// pricing_input_per_million, pricing_output_per_million
    pub fn model_to_fields(model: &Model) -> HashMap<String, serde_json::Value> {
        let mut fields = HashMap::with_capacity(10);
        fields.insert("model_id".into(), serde_json::json!(model.id));
        fields.insert("provider".into(), serde_json::json!(model.provider));
        fields.insert(
            "context_window".into(),
            serde_json::json!(model.context_window),
        );
        fields.insert(
            "max_output_tokens".into(),
            serde_json::json!(model.max_output_tokens),
        );
        fields.insert(
            "supports_vision".into(),
            serde_json::json!(model.input_modalities.contains(&InputModality::Image)),
        );
        fields.insert(
            "supports_reasoning".into(),
            serde_json::json!(model.reasoning),
        );
        fields.insert("supports_tools".into(), serde_json::json!(true));
        fields.insert("api_protocol".into(), serde_json::json!(model.api));
        fields.insert(
            "pricing_input_per_million".into(),
            serde_json::json!(model.pricing.input),
        );
        fields.insert(
            "pricing_output_per_million".into(),
            serde_json::json!(model.pricing.output),
        );
        fields
    }

    /// 用本体 LLM EntityDef 的字段约束校验 Model 实例
    ///
    /// 返回校验结果列表。对于本体中没有 preconditions 的字段（当前设计），
    /// 仅检查 RequiredField 和 TypeCheck。
    pub fn validate_model(
        registry: &uncode_ontology::TypeRegistry,
        model: &Model,
    ) -> Vec<uncode_ontology::ConstraintResult> {
        let entity = registry.get_entity(&uncode_ontology::TypeId("LLM".into()));
        let Some(entity) = entity else {
            return vec![uncode_ontology::ConstraintResult::Fail {
                constraint: "LLM entity not found".into(),
                field: "LLM".into(),
                detail: "system resource ontology not loaded".into(),
            }];
        };

        let fields = Self::model_to_fields(model);
        entity
            .fields
            .iter()
            .filter(|field_def| field_def.required)
            .map(|field_def| {
                uncode_ontology::evaluate_constraint(
                    &uncode_ontology::Constraint::RequiredField {
                        field: field_def.name.clone(),
                    },
                    &fields,
                )
            })
            .collect()
    }

    /// 查询满足条件的模型列表
    ///
    /// criteria 中的 key 应为 LLM EntityDef 的 FieldDef.name，
    /// value 为期望值（精确匹配）。
    pub fn query_models<'a>(
        models: &'a [Model],
        criteria: &HashMap<String, serde_json::Value>,
    ) -> Vec<&'a Model> {
        models
            .iter()
            .filter(|m| {
                let fields = Self::model_to_fields(m);
                criteria.iter().all(|(k, v)| fields.get(k) == Some(v))
            })
            .collect()
    }

    /// 预估单次 turn 的成本（USD）
    ///
    /// 输入端用 `context_tokens`，输出端用 `max_output_tokens` 作为上界。
    pub fn estimate_turn_cost(model: &Model, context_tokens: u64) -> f64 {
        let input_cost = (context_tokens as f64 / 1_000_000.0) * model.pricing.input;
        let output_cost = (model.max_output_tokens as f64 / 1_000_000.0) * model.pricing.output;
        input_cost + output_cost
    }
}

// ═══════════════════════════════════════════════════════════
// CostBudgetPolicy — 裁决器中的成本约束
// ═══════════════════════════════════════════════════════════

/// 成本预算策略 — 检查预估成本是否在预算内
///
/// Phase 1 先做 warn 不做 deny（通过 violations 报告），
/// 后续可切换为 deny 模式。
pub struct CostBudgetPolicy {
    budget_per_turn_usd: f64,
    deny_mode: bool,
}

impl CostBudgetPolicy {
    pub fn new(budget_per_turn_usd: f64) -> Self {
        Self {
            budget_per_turn_usd,
            deny_mode: false,
        }
    }

    pub fn with_deny_mode(mut self, deny: bool) -> Self {
        self.deny_mode = deny;
        self
    }

    /// 检查模型在给定 context tokens 下的预估成本
    pub fn check(&self, model: &Model, context_tokens: u64) -> DecisionVerdict {
        let cost = ModelBridge::estimate_turn_cost(model, context_tokens);
        if cost <= self.budget_per_turn_usd {
            return DecisionVerdict::approved();
        }

        let msg = format!(
            "estimated cost ${cost:.6} exceeds budget ${:.6} per turn (model: {})",
            self.budget_per_turn_usd, model.id
        );

        if self.deny_mode {
            DecisionVerdict::denied(msg)
        } else {
            DecisionVerdict::warn(
                format!("cost_warning: ${cost:.6} per turn (model: {})", model.id),
                vec![format!("cost_warning: ${cost:.6}")],
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════
// CostBudgetPolicyAdapter — DecisionPolicy trait adapter
// ═══════════════════════════════════════════════════════════

pub struct CostBudgetPolicyAdapter {
    inner: CostBudgetPolicy,
    model: Arc<Model>,
    context_tokens: u64,
}

impl CostBudgetPolicyAdapter {
    pub fn new(
        budget_per_turn_usd: f64,
        deny_mode: bool,
        model: Arc<Model>,
        context_tokens: u64,
    ) -> Self {
        Self {
            inner: CostBudgetPolicy::new(budget_per_turn_usd).with_deny_mode(deny_mode),
            model,
            context_tokens,
        }
    }
}

impl super::adjudication::DecisionPolicy for CostBudgetPolicyAdapter {
    fn evaluate(
        &self,
        context: &super::types::DecisionContext,
        _action: &super::types::NormalizedAction,
    ) -> Result<DecisionVerdict, super::adjudication::AdjudicationError> {
        let effective_tokens = if context.total_input_tokens > 0 {
            context.total_input_tokens
        } else {
            self.context_tokens
        };
        let verdict = self.inner.check(&self.model, effective_tokens);
        if !verdict.allowed {
            return Err(super::adjudication::AdjudicationError::Denied {
                policy: self.name().to_owned(),
                reason: verdict.reason.unwrap_or_default(),
            });
        }
        Ok(verdict)
    }

    fn name(&self) -> &str {
        "cost_budget"
    }
}

// ═══════════════════════════════════════════════════════════
// ModelCapabilityRule — 防火墙中的模型能力校验
// ═══════════════════════════════════════════════════════════

/// 模型能力校验 — 检查当前模型是否满足工具调用的能力要求
///
/// Phase 1 先做 warn 不做 deny。当检测到能力不匹配时，
/// 在 violations 中报告建议切换的模型列表。
#[derive(Debug)]
pub struct ModelCapabilityRule {
    ontology: uncode_ontology::TypeRegistry,
    models: Arc<Vec<Model>>,
    current_model: Arc<Model>,
}

impl ModelCapabilityRule {
    pub fn new(
        ontology: uncode_ontology::TypeRegistry,
        models: Arc<Vec<Model>>,
        current_model: Arc<Model>,
    ) -> Self {
        Self {
            ontology,
            models,
            current_model,
        }
    }

    fn required_capabilities(&self, action: &ParsedAction) -> Vec<String> {
        let mut caps = Vec::new();

        if action.tool_name == "read"
            && action
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .map(Self::needs_vision)
                .unwrap_or(false)
        {
            caps.push("supports_vision".into());
        }

        if let Some(action_def) = self.ontology.get_action(&action.tool_name) {
            if matches!(
                action_def.execution_category,
                uncode_ontology::types::ExecutionCategory::Shell
            ) {
                caps.push("supports_tools".into());
            }
        }

        caps
    }

    pub fn check_capability(&self, model: &Model, capability: &str) -> CapabilityCheckResult {
        let fields = ModelBridge::model_to_fields(model);
        let supported = fields
            .get(capability)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if supported {
            return CapabilityCheckResult::Supported;
        }

        let alternatives: Vec<String> = self
            .models
            .iter()
            .filter(|m| {
                let fields = ModelBridge::model_to_fields(m);
                fields
                    .get(capability)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .map(|m| m.id.clone())
            .collect();

        CapabilityCheckResult::Unsupported { alternatives }
    }

    pub fn needs_vision(path: &str) -> bool {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
        )
    }
}

/// 能力检查结果
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CapabilityCheckResult {
    Supported,
    Unsupported { alternatives: Vec<String> },
}

impl ValidationRule for ModelCapabilityRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        let required_caps = self.required_capabilities(action);

        if required_caps.is_empty() {
            return Ok(ValidationVerdict::approved());
        }

        let mut violations = Vec::new();
        for cap in &required_caps {
            if let CapabilityCheckResult::Unsupported { alternatives } =
                self.check_capability(&self.current_model, cap)
            {
                let msg = format!(
                    "model '{}' lacks '{}' for action '{}'. Alternatives: {}",
                    self.current_model.id,
                    cap,
                    action.tool_name,
                    alternatives.join(", ")
                );
                violations.push(msg);
            }
        }

        if violations.is_empty() {
            Ok(ValidationVerdict::approved())
        } else {
            let reason = violations.join("; ");
            Ok(ValidationVerdict {
                approved: true,
                reason: Some(reason),
                violations,
            })
        }
    }

    fn name(&self) -> &str {
        "model_capability"
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_ai::model::ModelPricingPerMillion;

    fn test_model(id: &str, reasoning: bool, vision: bool) -> Model {
        Model {
            id: id.into(),
            name: id.into(),
            api: "openai-completions".into(),
            provider: "test".into(),
            base_url: "https://api.test.com".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            reasoning,
            input_modalities: if vision {
                vec![InputModality::Text, InputModality::Image]
            } else {
                vec![InputModality::Text]
            },
            pricing: ModelPricingPerMillion {
                input: 1.0,
                output: 2.0,
                ..Default::default()
            },
            ..Model::default()
        }
    }

    #[test]
    fn test_model_to_fields_mapping() {
        let model = test_model("test-model", true, true);
        let fields = ModelBridge::model_to_fields(&model);
        assert_eq!(fields["model_id"], "test-model");
        assert_eq!(fields["provider"], "test");
        assert_eq!(fields["context_window"], 128_000);
        assert_eq!(fields["max_output_tokens"], 8192);
        assert_eq!(fields["supports_vision"], true);
        assert_eq!(fields["supports_reasoning"], true);
        assert_eq!(fields["supports_tools"], true);
        assert_eq!(fields["api_protocol"], "openai-completions");
        assert_eq!(fields["pricing_input_per_million"], 1.0);
        assert_eq!(fields["pricing_output_per_million"], 2.0);
    }

    #[test]
    fn test_model_to_fields_no_vision() {
        let model = test_model("text-only", false, false);
        let fields = ModelBridge::model_to_fields(&model);
        assert_eq!(fields["supports_vision"], false);
        assert_eq!(fields["supports_reasoning"], false);
    }

    #[test]
    fn test_validate_model_passes() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let model = test_model("valid-model", false, false);
        let results = ModelBridge::validate_model(&ontology, &model);
        assert!(
            results
                .iter()
                .all(|r| matches!(r, uncode_ontology::ConstraintResult::Pass)),
            "valid model should pass all required field checks"
        );
    }

    #[test]
    fn test_validate_model_missing_entity() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let model = test_model("any", false, false);
        let results = ModelBridge::validate_model(&ontology, &model);
        assert!(
            results
                .iter()
                .any(|r| matches!(r, uncode_ontology::ConstraintResult::Fail { .. })),
            "should fail when LLM entity not in domain-only ontology"
        );
    }

    #[test]
    fn test_query_models_by_capability() {
        let models = vec![
            test_model("text-only", false, false),
            test_model("vision-model", false, true),
            test_model("reasoning-model", true, false),
        ];
        let criteria = [("supports_vision".into(), serde_json::json!(true))]
            .into_iter()
            .collect();
        let result = ModelBridge::query_models(&models, &criteria);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "vision-model");
    }

    #[test]
    fn test_query_models_by_provider() {
        let models = vec![
            test_model("model-a", false, false),
            test_model("model-b", false, false),
        ];
        let criteria = [("provider".into(), serde_json::json!("test"))]
            .into_iter()
            .collect();
        let result = ModelBridge::query_models(&models, &criteria);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_query_models_no_match() {
        let models = vec![test_model("text-only", false, false)];
        let criteria = [("supports_vision".into(), serde_json::json!(true))]
            .into_iter()
            .collect();
        let result = ModelBridge::query_models(&models, &criteria);
        assert!(result.is_empty());
    }

    #[test]
    fn test_estimate_turn_cost() {
        let model = test_model("costly", false, false);
        let cost = ModelBridge::estimate_turn_cost(&model, 100_000);
        let expected_input = (100_000.0 / 1_000_000.0) * 1.0;
        let expected_output = (8192.0 / 1_000_000.0) * 2.0;
        let expected = expected_input + expected_output;
        assert!((cost - expected).abs() < 1e-10);
    }

    #[test]
    fn test_cost_budget_policy_under_budget() {
        let model = test_model("cheap", false, false);
        let policy = CostBudgetPolicy::new(1.0);
        let verdict = policy.check(&model, 100_000);
        assert!(verdict.allowed);
    }

    #[test]
    fn test_cost_budget_policy_over_budget_warn() {
        let model = Model {
            pricing: ModelPricingPerMillion {
                input: 100.0,
                output: 200.0,
                ..Default::default()
            },
            ..test_model("expensive", false, false)
        };
        let policy = CostBudgetPolicy::new(0.01);
        let verdict = policy.check(&model, 100_000);
        assert!(verdict.allowed, "warn mode should still allow");
        assert!(verdict.reason.is_some());
        assert!(!verdict.violations.is_empty());
    }

    #[test]
    fn test_cost_budget_policy_over_budget_deny() {
        let model = Model {
            pricing: ModelPricingPerMillion {
                input: 100.0,
                output: 200.0,
                ..Default::default()
            },
            ..test_model("expensive", false, false)
        };
        let policy = CostBudgetPolicy::new(0.01).with_deny_mode(true);
        let verdict = policy.check(&model, 100_000);
        assert!(!verdict.allowed, "deny mode should block");
    }

    #[test]
    fn test_capability_check_supported() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let current = test_model("v", false, true);
        let models = Arc::new(vec![current.clone()]);
        let rule = ModelCapabilityRule::new(ontology, models, Arc::new(current));
        let result = rule.check_capability(&rule.current_model, "supports_vision");
        assert!(matches!(result, CapabilityCheckResult::Supported));
    }

    #[test]
    fn test_capability_check_unsupported_with_alternatives() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let current = test_model("text", false, false);
        let models = Arc::new(vec![current.clone(), test_model("vision", false, true)]);
        let rule = ModelCapabilityRule::new(ontology, models, Arc::new(current));
        let result = rule.check_capability(&rule.current_model, "supports_vision");
        match result {
            CapabilityCheckResult::Unsupported { alternatives } => {
                assert_eq!(alternatives, vec!["vision"]);
            }
            _ => panic!("expected unsupported"),
        }
    }

    #[test]
    fn test_needs_vision() {
        assert!(ModelCapabilityRule::needs_vision("image.png"));
        assert!(ModelCapabilityRule::needs_vision("photo.jpg"));
        assert!(ModelCapabilityRule::needs_vision("pic.JPEG"));
        assert!(ModelCapabilityRule::needs_vision("anim.gif"));
        assert!(!ModelCapabilityRule::needs_vision("icon.svg"));
        assert!(!ModelCapabilityRule::needs_vision("main.rs"));
        assert!(!ModelCapabilityRule::needs_vision("data.json"));
        assert!(!ModelCapabilityRule::needs_vision("readme.md"));
    }

    #[test]
    fn test_validation_rule_blocks_image_read_for_non_vision_model() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let current = test_model("text-only", false, false);
        let models = Arc::new(vec![
            current.clone(),
            test_model("vision-model", false, true),
        ]);
        let rule = ModelCapabilityRule::new(ontology, models, Arc::new(current));
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "/tmp/image.png"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "Phase 1 warn mode should still allow");
        assert!(!verdict.violations.is_empty());
    }

    #[test]
    fn test_validation_rule_allows_text_read() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let current = test_model("text-only", false, false);
        let models = Arc::new(vec![current.clone()]);
        let rule = ModelCapabilityRule::new(ontology, models, Arc::new(current));
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "/tmp/main.rs"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved);
        assert!(verdict.violations.is_empty());
    }

    #[test]
    fn test_required_capabilities_vision_for_image() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let model = test_model("m", false, false);
        let rule =
            ModelCapabilityRule::new(ontology, Arc::new(vec![model.clone()]), Arc::new(model));
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "/tmp/photo.jpg"}),
        };
        let caps = rule.required_capabilities(&action);
        assert!(caps.contains(&"supports_vision".to_owned()));
    }

    #[test]
    fn test_required_capabilities_empty_for_text() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let model = test_model("m", false, false);
        let rule =
            ModelCapabilityRule::new(ontology, Arc::new(vec![model.clone()]), Arc::new(model));
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "/tmp/main.rs"}),
        };
        let caps = rule.required_capabilities(&action);
        assert!(caps.is_empty());
    }

    #[test]
    fn test_required_capabilities_tools_for_bash() {
        let ontology = uncode_ontology::builtin::full_ontology();
        let model = test_model("m", false, false);
        let rule =
            ModelCapabilityRule::new(ontology, Arc::new(vec![model.clone()]), Arc::new(model));
        let action = ParsedAction {
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
        };
        let caps = rule.required_capabilities(&action);
        assert!(
            caps.contains(&"supports_tools".to_owned()),
            "Shell actions should require supports_tools"
        );
    }
}
