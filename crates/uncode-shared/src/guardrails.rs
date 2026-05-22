//! GuardrailConfig — 认知显化与决策驱动设计的声明式护栏配置
//!
//! 对应 `.uncode/guardrails.yaml`（或 `.uncode/guardrails.json`）的 Rust 类型定义。
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §4.2 治理铁三角

use serde::{Deserialize, Serialize};

/// 完整护栏配置（顶层结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub decision: DecisionConfig,
    #[serde(default)]
    pub firewall: FirewallConfig,
    #[serde(default)]
    pub adjudication: AdjudicationConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

fn default_version() -> u32 {
    1
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            version: 1,
            decision: DecisionConfig::default(),
            firewall: FirewallConfig::default(),
            adjudication: AdjudicationConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

// ── Decision ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionConfig {
    #[serde(default = "default_turn_limit")]
    pub turn_limit: u32,
    #[serde(default = "default_max_concurrent_tools")]
    pub max_concurrent_tools: u32,
    #[serde(default = "default_tool_timeout")]
    pub tool_timeout_seconds: u64,
}

fn default_turn_limit() -> u32 {
    50
}
fn default_max_concurrent_tools() -> u32 {
    8
}
fn default_tool_timeout() -> u64 {
    120
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            turn_limit: 50,
            max_concurrent_tools: 8,
            tool_timeout_seconds: 120,
        }
    }
}

// ── Firewall ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallConfig {
    #[serde(default)]
    pub path_safety: PathSafetyConfig,
    #[serde(default)]
    pub tool_whitelist: ToolWhitelistConfig,
    #[serde(default)]
    pub resource_limits: ResourceLimitConfig,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            path_safety: PathSafetyConfig::default(),
            tool_whitelist: ToolWhitelistConfig::default(),
            resource_limits: ResourceLimitConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSafetyMode {
    CwdOnly,
    AllowList,
    Unrestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSafetyConfig {
    #[serde(default = "default_path_mode")]
    pub mode: PathSafetyMode,
    #[serde(default)]
    pub allow_list: Vec<String>,
}

fn default_path_mode() -> PathSafetyMode {
    PathSafetyMode::CwdOnly
}

impl Default for PathSafetyConfig {
    fn default() -> Self {
        Self {
            mode: PathSafetyMode::CwdOnly,
            allow_list: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolWhitelistConfig {
    #[serde(default = "default_whitelist_mode")]
    pub mode: ToolWhitelistMode,
    #[serde(default)]
    pub blocked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolWhitelistMode {
    Builtin,
    Custom,
    All,
}

fn default_whitelist_mode() -> ToolWhitelistMode {
    ToolWhitelistMode::Builtin
}

impl Default for ToolWhitelistConfig {
    fn default() -> Self {
        Self {
            mode: ToolWhitelistMode::Builtin,
            blocked: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitConfig {
    #[serde(default = "default_max_file_mb")]
    pub max_file_size_mb: u32,
    #[serde(default = "default_max_bash_lines")]
    pub max_bash_output_lines: u32,
}

fn default_max_file_mb() -> u32 {
    10
}
fn default_max_bash_lines() -> u32 {
    1000
}

impl Default for ResourceLimitConfig {
    fn default() -> Self {
        Self {
            max_file_size_mb: 10,
            max_bash_output_lines: 1000,
        }
    }
}

// ── Adjudication ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjudicationConfig {
    #[serde(default = "default_policies")]
    pub policies: Vec<AdjudicationPolicyConfig>,
}

fn default_policies() -> Vec<AdjudicationPolicyConfig> {
    vec![
        AdjudicationPolicyConfig {
            name: "no_destructive_commands".into(),
            enabled: true,
            rules: vec![
                PolicyRule {
                    pattern: "rm -rf".into(),
                    action: PolicyAction::Block,
                },
                PolicyRule {
                    pattern: "DROP TABLE".into(),
                    action: PolicyAction::BlockAndWarn,
                },
            ],
        },
        AdjudicationPolicyConfig {
            name: "require_approval_for_write".into(),
            enabled: false,
            rules: vec![
                PolicyRule {
                    pattern: "write".into(),
                    action: PolicyAction::AskUser,
                },
                PolicyRule {
                    pattern: "edit".into(),
                    action: PolicyAction::AskUser,
                },
            ],
        },
    ]
}

impl Default for AdjudicationConfig {
    fn default() -> Self {
        Self {
            policies: default_policies(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjudicationPolicyConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub pattern: String,
    pub action: PolicyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Block,
    BlockAndWarn,
    AskUser,
    Allow,
}

// ── Audit ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    #[serde(default)]
    pub event_levels: EventLevelConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            event_levels: EventLevelConfig::default(),
            retention: RetentionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLevelConfig {
    #[serde(default = "default_critical_events")]
    pub critical: Vec<String>,
    #[serde(default = "default_standard_events")]
    pub standard: Vec<String>,
    #[serde(default = "default_verbose_events")]
    pub verbose: Vec<String>,
}

fn default_critical_events() -> Vec<String> {
    vec![
        "TurnStart".into(),
        "TurnEnd".into(),
        "ToolCallEnd".into(),
        "DecisionMade".into(),
        "Error".into(),
        "SessionStart".into(),
        "SessionEnd".into(),
        "CompactionComplete".into(),
    ]
}

fn default_standard_events() -> Vec<String> {
    vec![
        "ContentDelta".into(),
        "ToolCallStart".into(),
        "CompactionStart".into(),
        "ModelChanged".into(),
        "MessageQueued".into(),
        "MessageDelivered".into(),
    ]
}

fn default_verbose_events() -> Vec<String> {
    vec!["ToolCallProgress".into(), "ToolCallAwaitingApproval".into()]
}

impl Default for EventLevelConfig {
    fn default() -> Self {
        Self {
            critical: default_critical_events(),
            standard: default_standard_events(),
            verbose: default_verbose_events(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_permanent")]
    pub critical_events: String,
    #[serde(default = "default_90d")]
    pub standard_events: String,
    #[serde(default = "default_7d")]
    pub verbose_events: String,
}

fn default_permanent() -> String {
    "permanent".into()
}
fn default_90d() -> String {
    "90_days".into()
}
fn default_7d() -> String {
    "7_days".into()
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            critical_events: "permanent".into(),
            standard_events: "90_days".into(),
            verbose_events: "7_days".into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_roundtrip_json() {
        let config = GuardrailConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: GuardrailConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.decision.turn_limit, 50);
        assert_eq!(parsed.decision.max_concurrent_tools, 8);
    }

    #[test]
    fn test_default_policies() {
        let config = GuardrailConfig::default();
        assert_eq!(config.adjudication.policies.len(), 2);
        assert!(config.adjudication.policies[0].enabled);
        assert!(!config.adjudication.policies[1].enabled);
    }

    #[test]
    fn test_event_levels() {
        let config = GuardrailConfig::default();
        assert!(config.audit.event_levels.critical.contains(&"Error".into()));
        assert!(
            config
                .audit
                .event_levels
                .verbose
                .contains(&"ToolCallProgress".into())
        );
    }
}
