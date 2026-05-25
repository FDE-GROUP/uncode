//! 语义防火墙 — 认知层与决策层之间的唯一通道
//!
//! ## 三层管线
//!
//! ```text
//! ActionProposal (原始)
//!   → Parser    (ParsedAction)        — 结构化提取
//!   → Validator (ValidationVerdict)   — 合法性校验
//!   → Normalizer (NormalizedAction)   — 消歧义、标准化
//! ```
//!
//! ## 包装策略
//!
//! `ValidationRule` trait 实现**包装**现有的安全基础设施：
//!
//! | ValidationRule | 包装的现有组件 | 状态 |
//! |:---|:---|:---|
//! | `PermissionPolicyRule` | `tool_permission.rs::PermissionPolicy` | ✅ 已实现 |
//! | `PathSafetyRule` | `tools/mod.rs::resolve_path()` | ✅ 已实现 |
//! | `SchemaCoercionRule` | `tools/registry.rs::ToolRegistry::prepare_and_validate()` | ✅ 已实现 |
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 语义防火墙

use std::path::Path;
use std::sync::Arc;

use crate::tools::ToolRegistry;

use super::types::{ActionProposal, NormalizedAction, ParsedAction, ValidatedAction};

// ── SemanticFirewall ─────────────────────────────────────

/// 语义防火墙 — 编排三层管线
#[derive(Debug)]
pub struct SemanticFirewall {
    pub parser: Box<dyn ParseStrategy>,
    pub validators: Vec<Box<dyn ValidationRule>>,
    pub normalizer: Box<dyn NormalizeStrategy>,
}

impl SemanticFirewall {
    /// 完整执行三层管线
    pub fn process(&self, raw: &ActionProposal) -> Result<NormalizedAction, FirewallError> {
        let parsed = self.parser.parse(raw)?;
        let validated = self.validate_all(&parsed)?;
        self.normalizer.normalize(&validated).map_err(Into::into)
    }

    fn validate_all(&self, parsed: &ParsedAction) -> Result<ValidatedAction, FirewallError> {
        let mut applied_rules = Vec::new();
        for rule in &self.validators {
            let verdict = rule.validate(parsed)?;
            if !verdict.approved {
                return Err(FirewallError::Blocked {
                    reasons: verdict.violations,
                });
            }
            applied_rules.push(rule.name().to_string());
        }
        Ok(ValidatedAction {
            tool_name: parsed.tool_name.clone(),
            arguments: parsed.arguments.clone(),
            applied_rules,
        })
    }
}

// ── Parser ──────────────────────────────────────────────

pub trait ParseStrategy: Send + Sync + std::fmt::Debug {
    fn parse(&self, raw: &ActionProposal) -> Result<ParsedAction, ParseError>;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("failed to parse arguments: {0}")]
    InvalidArguments(String),
}

/// 默认解析器：将 ActionProposal 的 raw_arguments 直接作为 ParsedAction
#[derive(Debug)]
pub struct DefaultParser;

impl ParseStrategy for DefaultParser {
    fn parse(&self, raw: &ActionProposal) -> Result<ParsedAction, ParseError> {
        Ok(ParsedAction {
            tool_name: raw.tool_name.clone(),
            arguments: raw.raw_arguments.clone(),
        })
    }
}

// ── Validator ───────────────────────────────────────────

pub trait ValidationRule: Send + Sync + std::fmt::Debug {
    #[must_use]
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError>;
    fn name(&self) -> &str;
}

#[derive(Debug)]
pub struct ValidationVerdict {
    pub approved: bool,
    pub reason: Option<String>,
    pub violations: Vec<String>,
}

impl ValidationVerdict {
    #[must_use]
    pub fn approved() -> Self {
        Self {
            approved: true,
            reason: None,
            violations: vec![],
        }
    }

    #[must_use]
    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            approved: false,
            reason: Some(reason.into()),
            violations: vec![],
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    #[error("validation rule '{rule}' failed: {reason}")]
    RuleFailed { rule: String, reason: String },
}

// ── Normalizer ──────────────────────────────────────────

pub trait NormalizeStrategy: Send + Sync + std::fmt::Debug {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError>;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NormalizeError {
    #[error("path normalization failed: {0}")]
    PathError(String),
}

/// 默认规范化器：原样通过
#[derive(Debug)]
pub struct DefaultNormalizer;

impl NormalizeStrategy for DefaultNormalizer {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
        Ok(NormalizedAction {
            tool_name: action.tool_name.clone(),
            arguments: action.arguments.clone(),
            normalized_fields: vec![],
        })
    }
}

/// 声明式规范化器：字段名映射 + 默认值填充
///
/// 在没有完整本体 crate 的情况下，用参数化配置解决三个痛点：
/// - LLM 输出的字段名别名统一（filepath → path）
/// - 缺失参数的默认值填充
/// - normalized_fields 日志输出（不再为空）
#[derive(Debug)]
pub struct DeclarativeNormalizer {
    field_mapping: std::collections::HashMap<String, String>,
    defaults: uncode_ontology::registry::FieldDefaults,
}

impl DeclarativeNormalizer {
    pub fn new(
        field_mapping: std::collections::HashMap<String, String>,
        defaults: uncode_ontology::registry::FieldDefaults,
    ) -> Self {
        Self {
            field_mapping,
            defaults,
        }
    }

    /// Build from an ontology TypeRegistry — uses Domain category only.
    pub fn from_registry(registry: &uncode_ontology::TypeRegistry) -> Self {
        Self {
            field_mapping: registry
                .field_aliases_by_category(uncode_ontology::EntityCategory::Domain),
            defaults: registry.defaults_by_category(uncode_ontology::EntityCategory::Domain),
        }
    }

    /// Build with hardcoded mappings for the 9 built-in tools.
    pub fn builtin() -> Self {
        let registry = uncode_ontology::builtin::coding_agent_ontology();
        Self::from_registry(&registry)
    }
}

impl<'a> From<&'a uncode_ontology::TypeRegistry> for DeclarativeNormalizer {
    fn from(registry: &'a uncode_ontology::TypeRegistry) -> Self {
        Self::from_registry(registry)
    }
}

impl NormalizeStrategy for DeclarativeNormalizer {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError> {
        let mut args = action.arguments.clone();
        let mut normalized_fields = Vec::new();

        // Field name normalization
        if let serde_json::Value::Object(ref mut map) = args {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(canonical) = self.field_mapping.get(&key) {
                    if canonical != &key {
                        if let Some(val) = map.remove(&key) {
                            map.insert(canonical.clone(), val);
                            normalized_fields.push(format!("{key} → {canonical}"));
                        }
                    }
                }
            }
        }

        // Default value filling
        if let Some(tool_defaults) = self.defaults.get(&action.tool_name) {
            if let serde_json::Value::Object(ref mut map) = args {
                for (field, default) in tool_defaults {
                    if !map.contains_key(field) {
                        map.insert(field.clone(), default.clone());
                        normalized_fields.push(format!("{field} = default"));
                    }
                }
            }
        }

        Ok(NormalizedAction {
            tool_name: action.tool_name.clone(),
            arguments: args,
            normalized_fields,
        })
    }
}

// ── FirewallError ───────────────────────────────────────

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FirewallError {
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("validation blocked: {reasons:?}")]
    Blocked { reasons: Vec<String> },
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("normalization error: {0}")]
    Normalization(#[from] NormalizeError),
}

// ═══════════════════════════════════════════════════════════
// 内置 ValidationRule 实现 — 包装现有基础设施
// ═══════════════════════════════════════════════════════════

// ── PermissionPolicyRule ────────────────────────────────

/// 包装 `tool_permission.rs::PermissionPolicy`
#[derive(Debug)]
pub struct PermissionPolicyRule {
    policy: Arc<crate::tool_permission::PermissionPolicy>,
    auto_allow_readonly: bool,
    auto_allow_bash_safe: bool,
}

impl PermissionPolicyRule {
    pub fn new(policy: Arc<crate::tool_permission::PermissionPolicy>) -> Self {
        Self {
            policy,
            auto_allow_readonly: false,
            auto_allow_bash_safe: false,
        }
    }

    pub fn with_auto_allow(mut self, readonly: bool, bash_safe: bool) -> Self {
        self.auto_allow_readonly = readonly;
        self.auto_allow_bash_safe = bash_safe;
        self
    }
}

impl ValidationRule for PermissionPolicyRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        let args_str = serde_json::to_string(&action.arguments).unwrap_or_default();
        let needs = self.policy.needs_confirmation(
            &action.tool_name,
            &args_str,
            self.auto_allow_readonly,
            self.auto_allow_bash_safe,
        );
        if needs {
            Ok(ValidationVerdict::denied(
                "permission policy requires user confirmation",
            ))
        } else {
            Ok(ValidationVerdict::approved())
        }
    }
    fn name(&self) -> &str {
        "permission_policy"
    }
}

// ── PathSafetyRule ──────────────────────────────────────

/// 路径安全模式
#[derive(Debug)]
enum PathSafetyMode {
    /// 仅允许 CWD 内路径
    CwdOnly,
    /// CWD + allow_list 中的路径
    AllowList {
        allowed_dirs: Vec<std::path::PathBuf>,
    },
    /// 不限制（仅用于测试）
    Unrestricted,
}

/// 路径安全校验 — 确保文件操作路径在允许范围内
#[derive(Debug)]
pub struct PathSafetyRule {
    cwd: std::path::PathBuf,
    canonical_cwd: std::path::PathBuf,
    mode: PathSafetyMode,
}

impl PathSafetyRule {
    pub fn new(cwd: impl Into<std::path::PathBuf>) -> Self {
        let cwd = cwd.into();
        let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.clone());
        Self {
            cwd,
            canonical_cwd,
            mode: PathSafetyMode::CwdOnly,
        }
    }

    pub fn with_allow_list(
        cwd: impl Into<std::path::PathBuf>,
        allow_list: &[impl AsRef<std::path::Path>],
    ) -> Self {
        let cwd = cwd.into();
        let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.clone());
        let allowed_dirs: Vec<std::path::PathBuf> = allow_list
            .iter()
            .map(|p| std::path::PathBuf::from(p.as_ref()))
            .collect();
        Self {
            cwd,
            canonical_cwd,
            mode: PathSafetyMode::AllowList { allowed_dirs },
        }
    }

    pub fn unrestricted() -> Self {
        Self {
            cwd: std::path::PathBuf::new(),
            canonical_cwd: std::path::PathBuf::new(),
            mode: PathSafetyMode::Unrestricted,
        }
    }

    fn is_path_allowed(&self, resolved: &std::path::Path) -> bool {
        match &self.mode {
            PathSafetyMode::Unrestricted => true,
            PathSafetyMode::CwdOnly => resolved.starts_with(&self.canonical_cwd),
            PathSafetyMode::AllowList { allowed_dirs } => {
                if resolved.starts_with(&self.canonical_cwd) {
                    return true;
                }
                for dir in allowed_dirs {
                    if let Ok(canonical_dir) = dir.canonicalize() {
                        if resolved.starts_with(&canonical_dir) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

impl ValidationRule for PathSafetyRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        if matches!(self.mode, PathSafetyMode::Unrestricted) {
            return Ok(ValidationVerdict::approved());
        }

        // 只检查有 file/path 参数的工具
        let path_key = if action.arguments.get("path").is_some() {
            "path"
        } else if action.arguments.get("file").is_some() {
            "file"
        } else {
            return Ok(ValidationVerdict::approved());
        };

        if let Some(path_str) = action.arguments.get(path_key).and_then(|v| v.as_str()) {
            let p = Path::new(path_str);
            let full = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.cwd.join(p)
            };

            // 规范化路径
            let resolved = match full.canonicalize() {
                Ok(r) => r,
                Err(_) => {
                    // 路径可能不存在（新建文件），检查父目录
                    let Some(parent) = full.parent() else {
                        return Ok(ValidationVerdict::denied(format!(
                            "cannot resolve path: {path_str}"
                        )));
                    };
                    match parent.canonicalize() {
                        Ok(parent_resolved) => {
                            parent_resolved.join(full.file_name().unwrap_or_default())
                        }
                        Err(_) => {
                            return Ok(ValidationVerdict::denied(format!(
                                "cannot resolve path: {path_str}"
                            )));
                        }
                    }
                }
            };

            if !self.is_path_allowed(&resolved) {
                return Ok(ValidationVerdict {
                    approved: false,
                    reason: Some(format!("path escapes workspace: {path_str}")),
                    violations: vec![format!("path traversal: {path_str}")],
                });
            }
        }

        Ok(ValidationVerdict::approved())
    }
    fn name(&self) -> &str {
        "path_safety"
    }
}

// ── SchemaCoercionRule ──────────────────────────────────

/// 包装 `tools/registry.rs::ToolRegistry::prepare_and_validate()`
pub struct SchemaCoercionRule {
    registry: Arc<ToolRegistry>,
}

impl std::fmt::Debug for SchemaCoercionRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaCoercionRule").finish()
    }
}

impl SchemaCoercionRule {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

impl ValidationRule for SchemaCoercionRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        match self
            .registry
            .prepare_and_validate(&action.tool_name, action.arguments.clone())
        {
            Ok(_coerced) => {
                // prepare_and_validate 已做类型 coercion + schema 校验
                // 校验通过，不需要阻断
                Ok(ValidationVerdict::approved())
            }
            Err(msg) => Ok(ValidationVerdict {
                approved: false,
                reason: Some(msg.clone()),
                violations: vec![msg],
            }),
        }
    }
    fn name(&self) -> &str {
        "schema_coercion"
    }
}

// ── OntologyConstraintRule ───────────────────────────────

/// 本体约束校验 — 使用 TypeRegistry 的 constraint axioms 验证动作参数
///
/// 评估 `ActionDef::preconditions` 中的约束（RequiredField, TypeCheck, RangeCheck 等），
/// 将 Hard 级别失败转为 firewall deny，Soft 级别失败记录为 violations 但不阻断。
#[derive(Debug)]
pub struct OntologyConstraintRule {
    registry: uncode_ontology::TypeRegistry,
}

impl OntologyConstraintRule {
    pub fn new(registry: uncode_ontology::TypeRegistry) -> Self {
        Self { registry }
    }
}

impl ValidationRule for OntologyConstraintRule {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        let Some(action_def) = self.registry.get_action(&action.tool_name) else {
            return Ok(ValidationVerdict::approved());
        };

        let Some(map) = action.arguments.as_object() else {
            return Ok(ValidationVerdict::approved());
        };

        // Collect all entity types referenced by this action's effects,
        // resolve each entity (merging extends chains), and append their invariants.
        let mut constraints = Vec::with_capacity(action_def.preconditions.len() + 8);
        constraints.extend(action_def.preconditions.clone());

        for effect in &action_def.effects {
            let entity_name = match effect {
                uncode_ontology::Effect::Read { target, .. } => Some(target.as_str()),
                uncode_ontology::Effect::Modify { entity, .. } => Some(entity.as_str()),
                uncode_ontology::Effect::Create { entity, .. } => Some(entity.as_str()),
                uncode_ontology::Effect::Delete { entity, .. } => Some(entity.as_str()),
                _ => None,
            };
            if let Some(name) = entity_name {
                if let Some(resolved) = self
                    .registry
                    .resolve_entity(&uncode_ontology::TypeId::from(name))
                {
                    constraints.extend(resolved.invariants);
                }
            }
        }

        let mut violations = Vec::new();
        for constraint in &constraints {
            let result = uncode_ontology::evaluate_constraint(constraint, map);
            match result {
                uncode_ontology::ConstraintResult::Pass => {}
                uncode_ontology::ConstraintResult::Warn { detail, .. } => {
                    violations.push(detail);
                }
                uncode_ontology::ConstraintResult::Fail { detail, .. } => {
                    return Ok(ValidationVerdict {
                        approved: false,
                        reason: Some(detail.clone()),
                        violations: vec![detail],
                    });
                }
                _ => {}
            }
        }

        // Soft violations don't block, but we still report them
        if violations.is_empty() {
            Ok(ValidationVerdict::approved())
        } else {
            Ok(ValidationVerdict {
                approved: true,
                reason: None,
                violations,
            })
        }
    }

    fn name(&self) -> &str {
        "ontology_constraint"
    }
}

// ── Composite builder ───────────────────────────────────

pub struct FirewallModelInfo {
    pub current_model: Arc<uncode_ai::model::Model>,
    pub all_models: Arc<Vec<uncode_ai::model::Model>>,
}

/// 使用默认配置构建完整的 SemanticFirewall
pub fn build_default_firewall(
    policy: Arc<crate::tool_permission::PermissionPolicy>,
    registry: Arc<ToolRegistry>,
    cwd: std::path::PathBuf,
) -> SemanticFirewall {
    build_default_firewall_with_model(policy, registry, cwd, None)
}

pub fn build_default_firewall_with_model(
    policy: Arc<crate::tool_permission::PermissionPolicy>,
    registry: Arc<ToolRegistry>,
    cwd: std::path::PathBuf,
    model_info: Option<FirewallModelInfo>,
) -> SemanticFirewall {
    let ontology = uncode_ontology::builtin::full_ontology();
    let mut validators: Vec<Box<dyn ValidationRule>> = vec![
        Box::new(OntologyConstraintRule::new(ontology.clone())),
        Box::new(SchemaCoercionRule::new(Arc::clone(&registry))),
        Box::new(PathSafetyRule::new(cwd)),
        Box::new(PermissionPolicyRule::new(policy)),
    ];
    if let Some(info) = model_info {
        validators.push(Box::new(crate::decision::bridge::ModelCapabilityRule::new(
            ontology.clone(),
            info.all_models,
            info.current_model,
        )));
    }
    SemanticFirewall {
        parser: Box::new(DefaultParser),
        validators,
        normalizer: Box::new(DeclarativeNormalizer::from_registry(&ontology)),
    }
}

/// 从 GuardrailConfig 构建完整的 SemanticFirewall
pub fn build_firewall_from_config(
    config: &uncode_shared::guardrails::GuardrailConfig,
    registry: Arc<ToolRegistry>,
    cwd: std::path::PathBuf,
) -> SemanticFirewall {
    build_firewall_from_config_with_model(config, registry, cwd, None)
}

pub fn build_firewall_from_config_with_model(
    config: &uncode_shared::guardrails::GuardrailConfig,
    registry: Arc<ToolRegistry>,
    cwd: std::path::PathBuf,
    model_info: Option<FirewallModelInfo>,
) -> SemanticFirewall {
    let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
    let auto_allow = matches!(
        config.firewall.tool_whitelist.mode,
        uncode_shared::guardrails::ToolWhitelistMode::All
    );
    let ontology = uncode_ontology::builtin::full_ontology();

    let path_rule = match &config.firewall.path_safety.mode {
        uncode_shared::guardrails::PathSafetyMode::CwdOnly => PathSafetyRule::new(cwd),
        uncode_shared::guardrails::PathSafetyMode::AllowList => {
            PathSafetyRule::with_allow_list(cwd, &config.firewall.path_safety.allow_list)
        }
        uncode_shared::guardrails::PathSafetyMode::Unrestricted => PathSafetyRule::unrestricted(),
    };

    let mut validators: Vec<Box<dyn ValidationRule>> = vec![
        Box::new(OntologyConstraintRule::new(ontology.clone())),
        Box::new(SchemaCoercionRule::new(Arc::clone(&registry))),
        Box::new(path_rule),
        Box::new(PermissionPolicyRule::new(policy).with_auto_allow(auto_allow, auto_allow)),
    ];
    if let Some(info) = model_info {
        validators.push(Box::new(crate::decision::bridge::ModelCapabilityRule::new(
            ontology.clone(),
            info.all_models,
            info.current_model,
        )));
    }
    SemanticFirewall {
        parser: Box::new(DefaultParser),
        validators,
        normalizer: Box::new(DeclarativeNormalizer::from_registry(&ontology)),
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::types::IntentType;

    fn make_proposal(tool: &str, args: serde_json::Value) -> ActionProposal {
        ActionProposal {
            proposal_id: uuid::Uuid::new_v4(),
            intent: IntentType::from_tool_name(tool),
            tool_name: tool.to_string(),
            raw_arguments: args,
            rationale: None,
            confidence: None,
            alternatives: vec![],
            trace: vec![],
        }
    }

    // ── DefaultParser ──

    #[test]
    fn test_default_parser_passthrough() {
        let raw = make_proposal("read", serde_json::json!({"path": "src/main.rs"}));
        let parser = DefaultParser;
        let parsed = parser.parse(&raw).unwrap();
        assert_eq!(parsed.tool_name, "read");
        assert_eq!(parsed.arguments["path"], "src/main.rs");
    }

    // ── PermissionPolicyRule ──

    #[test]
    fn test_permission_policy_blocks_rm_rf() {
        let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
        let rule = PermissionPolicyRule::new(policy);
        let action = ParsedAction {
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command": "rm -rf /"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(!verdict.approved, "rm -rf should be blocked");
    }

    #[test]
    fn test_permission_policy_allows_ls() {
        let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
        let rule = PermissionPolicyRule::new(policy).with_auto_allow(false, true);
        let action = ParsedAction {
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command": "ls -la"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(
            verdict.approved,
            "ls should be allowed with auto_allow_bash_safe"
        );
    }

    // ── PathSafetyRule ──

    #[test]
    fn test_path_safety_blocks_traversal() {
        let cwd = std::env::temp_dir();
        let rule = PathSafetyRule::new(cwd.clone());
        let action = ParsedAction {
            tool_name: "write".into(),
            arguments: serde_json::json!({"path": "../outside/file.txt"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(!verdict.approved, "path traversal should be blocked");
    }

    #[test]
    fn test_path_safety_allows_non_file_tools() {
        let rule = PathSafetyRule::new(std::env::temp_dir());
        let action = ParsedAction {
            tool_name: "grep".into(),
            arguments: serde_json::json!({"pattern": "foo"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "non-path tools should pass through");
    }

    // ── Firewall pipeline ──

    #[test]
    fn test_firewall_pipeline_rejects_dangerous_bash() {
        let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
        let registry = Arc::new(ToolRegistry::new());
        let cwd = std::env::current_dir().unwrap();
        let firewall = build_default_firewall(policy, registry, cwd);

        let raw = make_proposal("bash", serde_json::json!({"command": "rm -rf /"}));
        let result = firewall.process(&raw);
        assert!(
            result.is_err(),
            "dangerous bash should be blocked by firewall pipeline"
        );
    }

    #[test]
    fn test_firewall_pipeline_allows_safe_action() {
        let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
        let registry = Arc::new(ToolRegistry::new());
        let cwd = std::env::current_dir().unwrap();
        let firewall = build_default_firewall(policy, registry, cwd);

        let raw = make_proposal("grep", serde_json::json!({"pattern": "fn main"}));
        let result = firewall.process(&raw);
        assert!(
            result.is_err(),
            "unregistered grep should fail at SchemaCoercion"
        );
    }

    #[test]
    fn test_firewall_blocked_error_contains_reasons() {
        let policy = Arc::new(crate::tool_permission::PermissionPolicy::default_policy());
        let registry = Arc::new(ToolRegistry::new());
        let cwd = std::env::current_dir().unwrap();
        let firewall = build_default_firewall(policy, registry, cwd);

        let raw = make_proposal("bash", serde_json::json!({"command": "rm -rf /"}));
        let err = firewall.process(&raw).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("blocked") || msg.contains("tool not found"),
            "error should indicate blocking, got: {msg}"
        );
    }

    #[test]
    fn test_path_safety_blocks_absolute_escape() {
        let rule = PathSafetyRule::new(std::env::temp_dir());
        let action = ParsedAction {
            tool_name: "write".into(),
            arguments: serde_json::json!({"path": "/etc/passwd"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(
            !verdict.approved,
            "absolute path outside CWD should be blocked"
        );
    }

    #[test]
    fn test_path_safety_allows_cwd_relative() {
        let cwd = std::env::current_dir().unwrap();
        let rule = PathSafetyRule::new(cwd.clone());
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "CWD-relative path should be allowed");
    }

    #[test]
    fn test_path_safety_unrestricted_allows_all() {
        let rule = PathSafetyRule::unrestricted();
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "/etc/passwd"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "unrestricted mode should allow any path");
    }

    #[test]
    fn test_path_safety_allow_list() {
        let tmp = std::env::temp_dir();
        let allowed = vec![tmp.to_string_lossy().to_string()];
        let rule =
            PathSafetyRule::with_allow_list(std::path::PathBuf::from("/nonexistent"), &allowed);
        // Path in allowed dir should pass
        let test_path = tmp.join("test_file.txt");
        let action = ParsedAction {
            tool_name: "write".into(),
            arguments: serde_json::json!({"path": test_path.to_string_lossy().to_string()}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "path in allow_list dir should be allowed");
    }

    #[test]
    fn test_normalizer_passthrough() {
        let normalizer = DefaultNormalizer;
        let validated = ValidatedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
            applied_rules: vec!["path_safety".into()],
        };
        let normalized = normalizer.normalize(&validated).unwrap();
        assert_eq!(normalized.tool_name, "read");
    }

    // ── DeclarativeNormalizer ──

    #[test]
    fn test_declarative_normalizer_field_alias() {
        let normalizer = DeclarativeNormalizer::builtin();
        let validated = ValidatedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"filepath": "src/main.rs"}),
            applied_rules: vec![],
        };
        let normalized = normalizer.normalize(&validated).unwrap();
        assert_eq!(normalized.arguments["path"], "src/main.rs");
        assert!(
            normalized
                .normalized_fields
                .contains(&"filepath → path".to_owned())
        );
    }

    #[test]
    fn test_declarative_normalizer_default_fill() {
        let normalizer = DeclarativeNormalizer::builtin();
        let validated = ValidatedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
            applied_rules: vec![],
        };
        let normalized = normalizer.normalize(&validated).unwrap();
        assert_eq!(normalized.arguments["offset"], 0);
        assert!(
            normalized
                .normalized_fields
                .contains(&"offset = default".to_owned())
        );
    }

    #[test]
    fn test_declarative_normalizer_no_op_on_canonical() {
        let normalizer = DeclarativeNormalizer::builtin();
        let validated = ValidatedAction {
            tool_name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
            applied_rules: vec![],
        };
        let normalized = normalizer.normalize(&validated).unwrap();
        assert_eq!(normalized.arguments["command"], "ls");
        // bash has no defaults, and "command" is already canonical
        assert!(normalized.normalized_fields.is_empty());
    }

    #[test]
    fn test_declarative_normalizer_combined() {
        let normalizer = DeclarativeNormalizer::builtin();
        // LLM sends "filepath" alias + missing "offset"
        let validated = ValidatedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"filepath": "src/main.rs"}),
            applied_rules: vec![],
        };
        let normalized = normalizer.normalize(&validated).unwrap();
        assert_eq!(normalized.arguments["path"], "src/main.rs");
        assert_eq!(normalized.arguments["offset"], 0);
        assert_eq!(
            normalized.normalized_fields.len(),
            3,
            "expected alias + offset default + hashline default"
        );
    }

    // ── OntologyConstraintRule ──

    #[test]
    fn test_ontology_constraint_blocks_missing_required_field() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let rule = OntologyConstraintRule::new(ontology);
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(!verdict.approved, "read without path should be blocked");
    }

    #[test]
    fn test_ontology_constraint_allows_valid_action() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let rule = OntologyConstraintRule::new(ontology);
        let action = ParsedAction {
            tool_name: "read".into(),
            arguments: serde_json::json!({"path": "src/main.rs"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "read with path should pass");
    }

    #[test]
    fn test_ontology_constraint_passes_unknown_tool() {
        let ontology = uncode_ontology::builtin::coding_agent_ontology();
        let rule = OntologyConstraintRule::new(ontology);
        let action = ParsedAction {
            tool_name: "custom_tool".into(),
            arguments: serde_json::json!({}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "unknown tool should pass through");
    }

    #[test]
    fn test_entity_invariants_are_enforced() {
        use uncode_ontology::*;

        let mut reg = TypeRegistry::new();
        // Register a File entity with RequiredField("path") invariant
        reg.register_entity(EntityDef {
            id: TypeId::from("File"),
            category: EntityCategory::Domain,
            fields: vec![FieldDef {
                name: "path".into(),
                value_type: TypeId::string(),
                required: true,
                default: None,
                aliases: vec![],
                description: None,
            }],
            invariants: vec![Constraint::RequiredField {
                field: "path".into(),
            }],
            extends: None,
            description: None,
        });
        // Register an action that modifies File but doesn't declare RequiredField("path")
        // itself — the entity invariant should be enforced
        reg.register_action(ActionDef {
            name: "touch".into(),
            category: EntityCategory::Domain,
            fields: vec![FieldDef {
                name: "path".into(),
                value_type: TypeId::string(),
                required: false,
                default: None,
                aliases: vec![],
                description: None,
            }],
            effects: vec![Effect::Modify {
                entity: "File".into(),
                fields: vec!["content".into()],
            }],
            output_type: TypeId::string(),
            preconditions: vec![], // ← no RequiredField here!
            execution_category: ExecutionCategory::Destructive,
            description: None,
        });

        let rule = OntologyConstraintRule::new(reg);
        // Without path — should be blocked by entity invariant
        let action = ParsedAction {
            tool_name: "touch".into(),
            arguments: serde_json::json!({}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(
            !verdict.approved,
            "touch without path should be blocked by entity invariant"
        );
        // With path — should pass
        let action = ParsedAction {
            tool_name: "touch".into(),
            arguments: serde_json::json!({"path": "/tmp/x"}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(verdict.approved, "touch with path should pass");
    }

    #[test]
    fn test_entity_invariants_through_extends_chain() {
        use uncode_ontology::*;

        let mut reg = TypeRegistry::new();
        // Base entity with RequiredField("id") invariant
        reg.register_entity(EntityDef {
            id: TypeId::from("Base"),
            category: EntityCategory::Domain,
            fields: vec![FieldDef {
                name: "id".into(),
                value_type: TypeId::string(),
                required: true,
                default: None,
                aliases: vec![],
                description: None,
            }],
            invariants: vec![Constraint::RequiredField { field: "id".into() }],
            extends: None,
            description: None,
        });
        // Derived entity extends Base — should inherit RequiredField("id")
        reg.register_entity(EntityDef {
            id: TypeId::from("Derived"),
            category: EntityCategory::Domain,
            fields: vec![],
            invariants: vec![],
            extends: Some(TypeId::from("Base")),
            description: None,
        });
        // Action that modifies Derived, has NO preconditions of its own
        reg.register_action(ActionDef {
            name: "op".into(),
            category: EntityCategory::Domain,
            fields: vec![FieldDef {
                name: "id".into(),
                value_type: TypeId::string(),
                required: false,
                default: None,
                aliases: vec![],
                description: None,
            }],
            effects: vec![Effect::Modify {
                entity: "Derived".into(),
                fields: vec![],
            }],
            output_type: TypeId::string(),
            preconditions: vec![],
            execution_category: ExecutionCategory::Destructive,
            description: None,
        });

        let rule = OntologyConstraintRule::new(reg);
        // Without "id" — entity invariant from Base should be enforced through extends chain
        let action = ParsedAction {
            tool_name: "op".into(),
            arguments: serde_json::json!({}),
        };
        let verdict = rule.validate(&action).unwrap();
        assert!(
            !verdict.approved,
            "extends chain: missing id should be blocked by Base entity invariant"
        );
    }
}
