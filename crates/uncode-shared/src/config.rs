use serde::{Deserialize, Serialize};

/// 应用级配置（CLI `config.json` / TOML 解析目标）。
///
/// **Pi:** 对照 Pi 模型与 provider 配置；路径与键名不复制 Pi/opencode 专名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub providers: ProviderConfigs,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub user_models: Vec<UserModelConfig>,
    #[serde(default)]
    pub workspace_graph: WorkspaceGraphConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
    #[serde(default)]
    pub compaction: CompactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    #[serde(default = "default_model_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigs {
    pub glm: Option<ProviderConfig>,
    pub deepseek: Option<ProviderConfig>,
    pub ollama: Option<OllamaConfig>,
    pub openrouter: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub anthropic: Option<ProviderConfig>,
    pub gemini: Option<ProviderConfig>,
    pub tavily: Option<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub host: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model: "deepseek-v3".into(),
            max_tokens: 8192,
            temperature: 0.7,
            providers: ProviderConfigs::default(),
            models: default_models(),
            user_models: vec![],
            workspace_graph: WorkspaceGraphConfig::default(),
            tools: ToolsConfig::default(),
            permissions: PermissionConfig::default(),
            compaction: CompactionConfig::default(),
        }
    }
}

/// 内置 coding 工具限额（`read` / `grep` 等）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// 单文件最大读取/搜索字节数（`read`、`grep` 跳过更大文件）。
    #[serde(default = "default_tools_max_file_bytes")]
    pub max_file_bytes: usize,
    /// `grep` 全局最多匹配条数。
    #[serde(default = "default_tools_max_grep_results")]
    pub max_grep_results: usize,
    /// Bash tool configuration (sandbox, etc.).
    #[serde(default)]
    pub bash: BashToolConfig,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: default_tools_max_file_bytes(),
            max_grep_results: default_tools_max_grep_results(),
            bash: BashToolConfig::default(),
        }
    }
}

/// Bash tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashToolConfig {
    /// Enable OS-level sandbox for bash command execution.
    #[serde(default)]
    pub sandbox: bool,
    /// Sandbox profile: "strict" (read-only FS + no network) or "permissive" (writable /tmp + network).
    #[serde(default = "default_sandbox_profile")]
    pub sandbox_profile: SandboxProfile,
}

impl Default for BashToolConfig {
    fn default() -> Self {
        Self {
            sandbox: false,
            sandbox_profile: default_sandbox_profile(),
        }
    }
}

/// Sandbox isolation profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxProfile {
    /// Read-only system directories, writable CWD only, no network.
    #[serde(rename = "strict")]
    Strict,
    /// Read-only system + writable CWD and /tmp, network allowed.
    #[serde(rename = "permissive")]
    Permissive,
}

impl Default for SandboxProfile {
    fn default() -> Self {
        Self::Strict
    }
}

fn default_sandbox_profile() -> SandboxProfile {
    SandboxProfile::Strict
}

fn default_tools_max_file_bytes() -> usize {
    1024 * 1024
}

fn default_tools_max_grep_results() -> usize {
    50
}

fn default_max_tokens() -> u32 {
    8192
}

fn default_temperature() -> f32 {
    0.7
}

fn default_base_url() -> Option<String> {
    None
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

fn default_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            id: "deepseek-v3".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V3".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            id: "deepseek-v4-pro".into(),
            provider: "deepseek".into(),
            display_name: "DeepSeek V4 Pro".into(),
            max_tokens: 128_000,
            supports_vision: false,
            supports_tools: true,
        },
        ModelConfig {
            id: "glm-5.1".into(),
            provider: "glm".into(),
            display_name: "GLM 5.1".into(),
            max_tokens: 128_000,
            supports_vision: true,
            supports_tools: true,
        },
    ]
}

fn default_model_max_tokens() -> u32 {
    128_000
}

// ── 用户自定义模型配置（Stage 6） ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelConfig {
    pub id: String,
    #[serde(default = "default_user_api")]
    pub api: String,
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub compat: Option<UserCompatConfig>,
}

fn default_user_api() -> String {
    "openai-completions".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserCompatConfig {
    #[serde(default)]
    pub supports_developer_role: Option<bool>,
    #[serde(default)]
    pub supports_usage_in_streaming: Option<bool>,
    #[serde(default)]
    pub done_breaks_stream: Option<bool>,
    #[serde(default)]
    pub thinking_format: Option<String>,
    #[serde(default)]
    pub max_tokens_field: Option<String>,
    #[serde(default)]
    pub send_session_affinity_headers: Option<bool>,
    #[serde(default)]
    pub supports_long_cache_retention: Option<bool>,
    #[serde(default)]
    pub supports_store: Option<bool>,
    #[serde(default)]
    pub requires_reasoning_content_on_assistant_messages: Option<bool>,
    #[serde(default)]
    pub supports_eager_tool_input_streaming: Option<bool>,
    #[serde(default)]
    pub supports_cache_control_on_tools: Option<bool>,
}

// ── Workspace Graph 配置 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGraphConfig {
    #[serde(default = "default_wg_enabled")]
    pub enabled: bool,
    #[serde(default = "default_wg_ttl_secs")]
    pub ttl_secs: u64,
    #[serde(default = "default_wg_max_items")]
    pub max_items: usize,
    #[serde(default = "default_wg_max_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_wg_max_file_bytes")]
    pub max_file_bytes: usize,
}

impl Default for WorkspaceGraphConfig {
    fn default() -> Self {
        Self {
            enabled: default_wg_enabled(),
            ttl_secs: default_wg_ttl_secs(),
            max_items: default_wg_max_items(),
            max_bytes: default_wg_max_bytes(),
            max_file_bytes: default_wg_max_file_bytes(),
        }
    }
}

fn default_wg_enabled() -> bool {
    true
}
fn default_wg_ttl_secs() -> u64 {
    21600
}
fn default_wg_max_items() -> usize {
    16
}
fn default_wg_max_bytes() -> usize {
    16384
}
fn default_wg_max_file_bytes() -> usize {
    100_000
}

// ── Permission 配置 ──

/// 工具权限策略配置（protected paths、dangerous bash detection、custom safe commands）。
///
/// **Pi:** 对照 `confirm-destructive` / `protected-paths` / `permission-gate` 扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Glob patterns for paths that always require confirmation for write/edit.
    #[serde(default = "default_protected_paths")]
    pub protected_paths: Vec<String>,
    /// Regex patterns for dangerous bash commands (always require confirmation).
    #[serde(default = "default_dangerous_patterns")]
    pub dangerous_bash_patterns: Vec<String>,
    /// Extra safe bash commands beyond the built-in whitelist.
    #[serde(default)]
    pub extra_safe_commands: Vec<String>,
    /// Whether dangerous bash detection is enabled. Default: true.
    #[serde(default = "default_true")]
    pub dangerous_bash_detection: bool,
    /// Whether protected path blocking is enabled. Default: true.
    #[serde(default = "default_true")]
    pub protected_paths_enabled: bool,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            protected_paths: default_protected_paths(),
            dangerous_bash_patterns: default_dangerous_patterns(),
            extra_safe_commands: vec![],
            dangerous_bash_detection: true,
            protected_paths_enabled: true,
        }
    }
}

fn default_protected_paths() -> Vec<String> {
    vec![
        ".env".into(),
        ".env.*".into(),
        ".git/".into(),
        ".ssh/".into(),
        ".aws/".into(),
        "**/credentials*".into(),
        "**/id_rsa*".into(),
        "**/id_ed25519*".into(),
    ]
}

fn default_dangerous_patterns() -> Vec<String> {
    vec![
        r"rm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+|--recursive\s+|--force\s+)".into(),
        r"\bsudo\b".into(),
        r"chmod\s+(777|000|a\+rwx)".into(),
        r"\bchown\b".into(),
        r"\bmkfs\b".into(),
        r"dd\s+if=".into(),
    ]
}

fn default_true() -> bool {
    true
}

// ── Compaction 配置 ──

/// 上下文压缩策略配置。
///
/// **Pi:** 对照 `CompactionSettings`（enabled, reserveTokens, keepRecentTokens）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// 是否启用自动压缩。Default: true。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 压缩触发阈值（context window 百分比）。Default: 80。
    #[serde(default = "default_threshold_percent")]
    pub threshold_percent: u64,
    /// 保留最近对话的 token 数。Default: 20000。
    #[serde(default = "default_keep_recent_tokens")]
    pub keep_recent_tokens: u64,
    /// 为模型回复预留的 token 数。Default: 16384。
    #[serde(default = "default_reserve_tokens")]
    pub reserve_tokens: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_percent: 80,
            keep_recent_tokens: 20000,
            reserve_tokens: 16384,
        }
    }
}

fn default_threshold_percent() -> u64 {
    80
}
fn default_keep_recent_tokens() -> u64 {
    20000
}
fn default_reserve_tokens() -> u64 {
    16384
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_profile_default() {
        assert_eq!(SandboxProfile::default(), SandboxProfile::Strict);
    }

    #[test]
    fn test_bash_tool_config_default() {
        let config = BashToolConfig::default();
        assert!(!config.sandbox);
        assert_eq!(config.sandbox_profile, SandboxProfile::Strict);
    }

    #[test]
    fn test_permission_config_default() {
        let config = PermissionConfig::default();
        assert!(config.dangerous_bash_detection);
        assert!(config.protected_paths_enabled);
        assert!(!config.protected_paths.is_empty());
        assert!(!config.dangerous_bash_patterns.is_empty());
        assert!(config.extra_safe_commands.is_empty());
    }

    #[test]
    fn test_compaction_config_default() {
        let config = CompactionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.threshold_percent, 80);
        assert_eq!(config.keep_recent_tokens, 20000);
        assert_eq!(config.reserve_tokens, 16384);
    }

    #[test]
    fn test_tools_config_default() {
        let config = ToolsConfig::default();
        assert_eq!(config.max_file_bytes, 1024 * 1024);
        assert_eq!(config.max_grep_results, 50);
        assert!(!config.bash.sandbox);
    }

    #[test]
    fn test_provider_config_construction() {
        let config = ProviderConfig {
            api_key: "sk-test123".into(),
            base_url: Some("https://api.example.com".into()),
        };
        assert_eq!(config.api_key, "sk-test123");
        assert_eq!(config.base_url, Some("https://api.example.com".to_string()));
    }

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.model, "deepseek-v3");
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.temperature, 0.7);
        assert!(config.providers.deepseek.is_none());
        assert_eq!(config.models.len(), 3);
    }

    #[test]
    fn test_user_compat_config_construction() {
        let config = UserCompatConfig {
            supports_developer_role: Some(true),
            supports_usage_in_streaming: Some(true),
            done_breaks_stream: Some(false),
            thinking_format: Some("deepseek-r1".into()),
            max_tokens_field: Some("max_tokens".into()),
            send_session_affinity_headers: Some(false),
            supports_long_cache_retention: Some(true),
            supports_store: Some(false),
            requires_reasoning_content_on_assistant_messages: Some(true),
            supports_eager_tool_input_streaming: Some(true),
            supports_cache_control_on_tools: Some(false),
        };
        assert_eq!(config.supports_developer_role, Some(true));
        assert_eq!(config.thinking_format, Some("deepseek-r1".to_string()));
        assert_eq!(config.done_breaks_stream, Some(false));
    }

    #[test]
    fn test_model_config_fields() {
        let config = ModelConfig {
            id: "test-model".into(),
            provider: "test-provider".into(),
            display_name: "Test Model".into(),
            max_tokens: 32000,
            supports_vision: true,
            supports_tools: false,
        };
        assert_eq!(config.id, "test-model");
        assert_eq!(config.provider, "test-provider");
        assert_eq!(config.display_name, "Test Model");
        assert_eq!(config.max_tokens, 32000);
        assert!(config.supports_vision);
        assert!(!config.supports_tools);
    }

    #[test]
    fn test_user_model_config_construction() {
        let config = UserModelConfig {
            id: "my-model".into(),
            api: "anthropic-messages".into(),
            provider: "anthropic".into(),
            base_url: Some("https://api.anthropic.com".into()),
            api_key: Some("sk-ant-xxx".into()),
            context_window: Some(200000),
            max_output_tokens: Some(8192),
            compat: Some(UserCompatConfig {
                supports_developer_role: Some(false),
                ..Default::default()
            }),
        };
        assert_eq!(config.id, "my-model");
        assert_eq!(config.api, "anthropic-messages");
        assert_eq!(config.provider, "anthropic");
        assert_eq!(
            config.base_url,
            Some("https://api.anthropic.com".to_string())
        );
        assert_eq!(config.api_key, Some("sk-ant-xxx".to_string()));
        assert_eq!(config.context_window, Some(200000));
        assert_eq!(config.max_output_tokens, Some(8192));
        assert_eq!(config.compat.unwrap().supports_developer_role, Some(false));
    }

    #[test]
    fn test_permission_config_serde_roundtrip() {
        let config = PermissionConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PermissionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.dangerous_bash_detection,
            deserialized.dangerous_bash_detection
        );
        assert_eq!(
            config.protected_paths_enabled,
            deserialized.protected_paths_enabled
        );
        assert_eq!(config.protected_paths, deserialized.protected_paths);
    }

    #[test]
    fn test_compaction_config_serde_roundtrip() {
        let config = CompactionConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CompactionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.threshold_percent, deserialized.threshold_percent);
        assert_eq!(config.keep_recent_tokens, deserialized.keep_recent_tokens);
        assert_eq!(config.reserve_tokens, deserialized.reserve_tokens);
    }

    #[test]
    fn test_bash_tool_config_serde_roundtrip() {
        let config = BashToolConfig {
            sandbox: true,
            sandbox_profile: SandboxProfile::Permissive,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BashToolConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.sandbox);
        assert_eq!(deserialized.sandbox_profile, SandboxProfile::Permissive);
    }
}
