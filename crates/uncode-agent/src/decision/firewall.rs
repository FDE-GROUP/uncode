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
//! ## 与现有代码的关系
//!
//! `ValidationRule` trait 实现**包装**现有的安全基础设施，不重写：
//!
//! | ValidationRule 实现 | 包装的现有组件 |
//! |:---|:---|
//! | `PermissionPolicyRule` | `tool_permission.rs::PermissionPolicy` |
//! | `PathSafetyRule` | `tools/mod.rs::resolve_path()` |
//! | `UrlSafetyRule` | `uncode-core/src/context.rs::fetch_url()` |
//! | `SchemaCoercionRule` | `tools/registry.rs::prepare_and_validate()` |
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 语义防火墙

use super::types::{ActionProposal, NormalizedAction, ParsedAction, ValidatedAction};

/// 语义防火墙 — 编排三层管线
pub struct SemanticFirewall {
    pub parser: Box<dyn ParseStrategy>,
    pub validators: Vec<Box<dyn ValidationRule>>,
    pub normalizer: Box<dyn NormalizeStrategy>,
}

// ── Parser ──────────────────────────────────────────────

/// 将 LLM 原始输出解析为结构化动作
pub trait ParseStrategy: Send + Sync {
    fn parse(&self, raw: &ActionProposal) -> Result<ParsedAction, ParseError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("failed to parse arguments: {0}")]
    InvalidArguments(String),
}

// ── Validator ───────────────────────────────────────────

/// 单条验证规则 — 包装现有安全基础设施
pub trait ValidationRule: Send + Sync {
    fn validate(&self, action: &ParsedAction) -> Result<ValidationVerdict, ValidationError>;
    fn name(&self) -> &'static str;
}

#[derive(Debug)]
pub struct ValidationVerdict {
    pub approved: bool,
    pub reason: Option<String>,
    pub violations: Vec<String>,
}

impl ValidationVerdict {
    pub fn approved() -> Self {
        Self { approved: true, reason: None, violations: vec![] }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("validation rule '{rule}' failed: {reason}")]
    RuleFailed { rule: String, reason: String },
}

// ── Normalizer ──────────────────────────────────────────

/// 将已验证的动作标准化为确定性命令
pub trait NormalizeStrategy: Send + Sync {
    fn normalize(&self, action: &ValidatedAction) -> Result<NormalizedAction, NormalizeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum NormalizeError {
    #[error("path normalization failed: {0}")]
    PathError(String),
}

// ── FirewallError ───────────────────────────────────────

#[derive(Debug, thiserror::Error)]
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

impl SemanticFirewall {
    /// 完整执行三层管线
    pub async fn process(&self, raw: ActionProposal) -> Result<NormalizedAction, FirewallError> {
        let parsed = self.parser.parse(&raw)?;
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

// ── 内置规则（后续 commit 中实现，包装现有逻辑）─────────

/// 包装 `tool_permission.rs::PermissionPolicy`
///
/// 在后续 commit 中实现。当前为占位符，默认允许所有动作。
pub struct PermissionPolicyRule;

impl ValidationRule for PermissionPolicyRule {
    fn validate(&self, _action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        // TODO(decision-refactor): 包装 PermissionPolicy::needs_confirmation()
        Ok(ValidationVerdict::approved())
    }
    fn name(&self) -> &'static str { "permission_policy" }
}

/// 包装 `tools/mod.rs::resolve_path()` — CWD 范围校验
pub struct PathSafetyRule;

impl ValidationRule for PathSafetyRule {
    fn validate(&self, _action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        // TODO(decision-refactor): 包装 resolve_path() 逻辑
        Ok(ValidationVerdict::approved())
    }
    fn name(&self) -> &'static str { "path_safety" }
}

/// 包装 `tools/registry.rs::prepare_and_validate()` — JSON Schema 验证
pub struct SchemaCoercionRule;

impl ValidationRule for SchemaCoercionRule {
    fn validate(&self, _action: &ParsedAction) -> Result<ValidationVerdict, ValidationError> {
        // TODO(decision-refactor): 包装 ToolRegistry::prepare_and_validate()
        Ok(ValidationVerdict::approved())
    }
    fn name(&self) -> &'static str { "schema_coercion" }
}
