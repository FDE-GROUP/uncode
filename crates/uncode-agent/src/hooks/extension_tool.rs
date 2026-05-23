use std::sync::Arc;

use async_trait::async_trait;
use uncode_core::error::UncodeError;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};
use uncode_extensions::tool::{ExtensionTool, ExtensionToolMetadata};

/// Adapter: wraps an [`ExtensionTool`] as a [`ToolExecutor`].
///
/// This bridge lives in `uncode-agent` because it needs access to both
/// `ToolExecutor` (uncode-core) and `ExtensionTool` (uncode-extensions).
pub struct ExtensionToolExecutor {
    tool: Arc<dyn ExtensionTool>,
    metadata: ExtensionToolMetadata,
}

impl ExtensionToolExecutor {
    pub fn new(tool: Arc<dyn ExtensionTool>) -> Self {
        let metadata = tool.metadata();
        Self { tool, metadata }
    }
}

#[async_trait]
impl ToolExecutor for ExtensionToolExecutor {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.metadata.name.clone(),
            description: self.metadata.description.clone(),
            parameters: self.metadata.parameters.clone(),
            label: self.metadata.label.clone(),
            execution_mode: if self.metadata.sequential {
                ExecutionMode::Sequential
            } else {
                ExecutionMode::Parallel
            },
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError> {
        self.tool
            .execute(arguments)
            .await
            .map_err(|e| UncodeError::Tool(e.to_string()))
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, UncodeError> {
        let output = self.execute(arguments).await?;
        Ok(ToolResult::ok(output))
    }
}
