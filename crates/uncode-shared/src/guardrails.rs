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
    #[serde(default)]
    pub cost: CostConfig,
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
            cost: CostConfig::default(),
        }
    }
}

impl GuardrailConfig {
    /// Load from `.uncode/guardrails.json` in the given directory.
    /// Returns `Self::default()` if the file does not exist or cannot be parsed.
    pub fn load_from_dir(dir: &std::path::Path) -> Self {
        let path = dir.join(".uncode").join("guardrails.json");
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "warn: failed to parse {}: {e}, using defaults",
                    path.display()
                );
                Self::default()
            }),
            Err(e) => {
                eprintln!(
                    "warn: failed to read {}: {e}, using defaults",
                    path.display()
                );
                Self::default()
            }
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
        "turn_start".into(),
        "turn_end".into(),
        "tool_call_end".into(),
        "decision_made".into(),
        "error".into(),
        "session_start".into(),
        "session_end".into(),
        "compaction_complete".into(),
    ]
}

fn default_standard_events() -> Vec<String> {
    vec![
        "content_delta".into(),
        "tool_call_start".into(),
        "compaction_start".into(),
        "model_changed".into(),
        "message_queued".into(),
        "message_delivered".into(),
    ]
}

fn default_verbose_events() -> Vec<String> {
    vec![
        "tool_call_progress".into(),
        "tool_call_awaiting_approval".into(),
    ]
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

// ── Cost ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    #[serde(default = "default_budget_per_turn")]
    pub budget_per_turn_usd: f64,
    #[serde(default = "default_deny_mode")]
    pub deny_mode: bool,
}

fn default_budget_per_turn() -> f64 {
    1.0
}
fn default_deny_mode() -> bool {
    false
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            budget_per_turn_usd: 1.0,
            deny_mode: false,
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
    fn test_cost_config_defaults() {
        let config = GuardrailConfig::default();
        assert!((config.cost.budget_per_turn_usd - 1.0).abs() < f64::EPSILON);
        assert!(!config.cost.deny_mode);
    }

    #[test]
    fn test_cost_config_from_json() {
        let json = r#"{"cost": {"budget_per_turn_usd": 0.5, "deny_mode": true}}"#;
        let config: GuardrailConfig = serde_json::from_str(json).unwrap();
        assert!((config.cost.budget_per_turn_usd - 0.5).abs() < f64::EPSILON);
        assert!(config.cost.deny_mode);
    }

    #[test]
    fn test_load_from_dir_missing_returns_default() {
        let dir = std::path::Path::new("/nonexistent/path");
        let config = GuardrailConfig::load_from_dir(dir);
        assert_eq!(config.version, 1);
        assert_eq!(config.decision.turn_limit, 50);
    }

    #[test]
    fn test_load_from_dir_valid_json() {
        let dir = std::env::temp_dir().join("uncode_test_guardrails");
        let uncode_dir = dir.join(".uncode");
        std::fs::create_dir_all(&uncode_dir).unwrap();
        let file_path = uncode_dir.join("guardrails.json");
        let json = r#"{"version": 2, "cost": {"budget_per_turn_usd": 0.1, "deny_mode": true}}"#;
        std::fs::write(&file_path, json).unwrap();

        let config = GuardrailConfig::load_from_dir(&dir);
        assert_eq!(config.version, 2);
        assert!((config.cost.budget_per_turn_usd - 0.1).abs() < f64::EPSILON);
        assert!(config.cost.deny_mode);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_from_dir_invalid_json_returns_default() {
        let dir = std::env::temp_dir().join("uncode_test_guardrails_invalid");
        let uncode_dir = dir.join(".uncode");
        std::fs::create_dir_all(&uncode_dir).unwrap();
        let file_path = uncode_dir.join("guardrails.json");
        std::fs::write(&file_path, "not valid json").unwrap();

        let config = GuardrailConfig::load_from_dir(&dir);
        assert_eq!(config.version, 1); // default

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_path_safety_config_default() {
        let config = PathSafetyConfig::default();
        assert!(matches!(config.mode, PathSafetyMode::CwdOnly));
        assert!(config.allow_list.is_empty());
    }

    #[test]
    fn test_path_safety_config_serde() {
        let json = r#"{"mode": "allow_list", "allow_list": ["/tmp"]}"#;
        let config: PathSafetyConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config.mode, PathSafetyMode::AllowList));
        assert_eq!(config.allow_list, vec!["/tmp"]);
    }

    #[test]
    fn test_path_safety_unrestricted_serde() {
        let json = r#"{"mode": "unrestricted"}"#;
        let config: PathSafetyConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config.mode, PathSafetyMode::Unrestricted));
    }

    #[test]
    fn test_tool_whitelist_mode_all_serde() {
        let json = r#"{"mode": "all"}"#;
        let config: ToolWhitelistConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config.mode, ToolWhitelistMode::All));
    }

    #[test]
    fn test_tool_whitelist_mode_custom_serde() {
        let json = r#"{"mode": "custom", "blocked": ["bash", "write"]}"#;
        let config: ToolWhitelistConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config.mode, ToolWhitelistMode::Custom));
        assert_eq!(config.blocked, vec!["bash", "write"]);
    }

    #[test]
    fn test_policy_action_allow_serde() {
        let json = r#"{"pattern": "ls", "action": "allow"}"#;
        let rule: PolicyRule = serde_json::from_str(json).unwrap();
        assert!(matches!(rule.action, PolicyAction::Allow));
        assert_eq!(rule.pattern, "ls");
    }

    #[test]
    fn test_policy_action_block_and_warn_serde() {
        let json = r#"{"pattern": "DROP TABLE", "action": "block_and_warn"}"#;
        let rule: PolicyRule = serde_json::from_str(json).unwrap();
        assert!(matches!(rule.action, PolicyAction::BlockAndWarn));
    }

    #[test]
    fn test_policy_action_ask_user_serde() {
        let json = r#"{"pattern": "write", "action": "ask_user"}"#;
        let rule: PolicyRule = serde_json::from_str(json).unwrap();
        assert!(matches!(rule.action, PolicyAction::AskUser));
    }

    #[test]
    fn test_resource_limit_defaults() {
        let config = ResourceLimitConfig::default();
        assert_eq!(config.max_file_size_mb, 10);
        assert_eq!(config.max_bash_output_lines, 1000);
    }

    #[test]
    fn test_full_config_from_json() {
        let json = r#"{
            "version": 3,
            "decision": {"turn_limit": 100, "max_concurrent_tools": 4, "tool_timeout_seconds": 300},
            "firewall": {
                "path_safety": {"mode": "unrestricted"},
                "tool_whitelist": {"mode": "custom", "blocked": ["bash"]}
            },
            "cost": {"budget_per_turn_usd": 0.01, "deny_mode": true}
        }"#;
        let config: GuardrailConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.version, 3);
        assert_eq!(config.decision.turn_limit, 100);
        assert_eq!(config.decision.max_concurrent_tools, 4);
        assert_eq!(config.decision.tool_timeout_seconds, 300);
        assert!(matches!(
            config.firewall.path_safety.mode,
            PathSafetyMode::Unrestricted
        ));
        assert!(matches!(
            config.firewall.tool_whitelist.mode,
            ToolWhitelistMode::Custom
        ));
        assert_eq!(config.firewall.tool_whitelist.blocked, vec!["bash"]);
        assert!((config.cost.budget_per_turn_usd - 0.01).abs() < f64::EPSILON);
        assert!(config.cost.deny_mode);
    }

    #[test]
    fn test_audit_config_defaults() {
        let config = AuditConfig::default();
        assert!(
            config
                .event_levels
                .critical
                .contains(&"session_start".into())
        );
        assert!(
            config
                .event_levels
                .standard
                .contains(&"content_delta".into())
        );
        assert!(
            config
                .event_levels
                .verbose
                .contains(&"tool_call_progress".into())
        );
        assert_eq!(config.retention.critical_events, "permanent");
        assert_eq!(config.retention.standard_events, "90_days");
        assert_eq!(config.retention.verbose_events, "7_days");
    }

    #[test]
    fn test_event_level_config_serde() {
        let json = r#"{"critical": ["Error"], "standard": ["ContentDelta"], "verbose": ["ToolCallProgress"]}"#;
        let config: EventLevelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.critical, vec!["Error"]);
        assert_eq!(config.standard, vec!["ContentDelta"]);
        assert_eq!(config.verbose, vec!["ToolCallProgress"]);
    }

    #[test]
    fn test_default_version_fn() {
        assert_eq!(default_version(), 1);
    }
}
