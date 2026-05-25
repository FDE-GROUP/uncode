//! Extension tool interface — how extensions define LLM-callable custom tools.
//!
//! Extension authors implement [`ExtensionTool`] to register tools that appear
//! in the LLM's available tool list and can be invoked during agent execution.

/// Tool metadata provided by extensions at registration time.
///
/// Converted to `uncode_ai::ToolDefinition` by the adapter in `uncode-agent`.
#[derive(Clone, serde::Deserialize)]
pub struct ExtensionToolMetadata {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub parameters: serde_json::Value,
    /// Human-readable label for UI display.
    pub label: Option<String>,
    /// If true, the tool must run sequentially (not in parallel with others).
    pub sequential: bool,
}

impl ExtensionToolMetadata {
    /// Validate metadata before registration.
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("tool name cannot be empty".into());
        }
        if !self
            .name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Err(format!(
                "tool name must start with a letter or underscore: {}",
                self.name
            ));
        }
        if self.name.chars().any(|c| c.is_whitespace() || c == '\0') {
            return Err(format!(
                "tool name cannot contain whitespace: {}",
                self.name
            ));
        }
        if self.description.is_empty() {
            return Err(format!("tool description cannot be empty: {}", self.name));
        }
        Ok(())
    }
}

/// Trait for extension-defined tools.
///
/// Extensions implement this trait and register instances via
/// [`crate::api::ExtensionApi::register_tool`].
#[async_trait::async_trait]
pub trait ExtensionTool: Send + Sync {
    /// Metadata describing this tool to the LLM.
    fn metadata(&self) -> ExtensionToolMetadata;

    /// Execute the tool with validated arguments.
    async fn execute(&self, arguments: serde_json::Value) -> anyhow::Result<String>;
}
