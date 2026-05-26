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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use uncode_core::tool::{ExecutionMode, ToolContext};
    use uncode_extensions::tool::ExtensionToolMetadata;

    struct MockExtTool {
        meta: ExtensionToolMetadata,
        output: String,
    }

    #[async_trait::async_trait]
    impl ExtensionTool for MockExtTool {
        fn metadata(&self) -> ExtensionToolMetadata {
            self.meta.clone()
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<String> {
            Ok(self.output.clone())
        }
    }

    fn make_meta(name: &str, desc: &str, sequential: bool) -> ExtensionToolMetadata {
        ExtensionToolMetadata {
            name: name.into(),
            description: desc.into(),
            parameters: serde_json::json!({"type": "object"}),
            label: Some("Test Label".into()),
            sequential,
        }
    }

    #[test]
    fn test_definition_name_and_description() {
        let meta = make_meta("my_tool", "does things", false);
        let tool = Arc::new(MockExtTool {
            meta,
            output: String::new(),
        });
        let executor = ExtensionToolExecutor::new(tool);
        let def = executor.definition();
        assert_eq!(def.name, "my_tool");
        assert_eq!(def.description, "does things");
        assert_eq!(def.parameters, serde_json::json!({"type": "object"}));
        assert_eq!(def.label, Some("Test Label".into()));
    }

    #[test]
    fn test_definition_sequential() {
        let meta = make_meta("t", "d", true);
        let tool = Arc::new(MockExtTool {
            meta,
            output: String::new(),
        });
        let executor = ExtensionToolExecutor::new(tool);
        assert_eq!(
            executor.definition().execution_mode,
            ExecutionMode::Sequential
        );
    }

    #[test]
    fn test_definition_parallel() {
        let meta = make_meta("t", "d", false);
        let tool = Arc::new(MockExtTool {
            meta,
            output: String::new(),
        });
        let executor = ExtensionToolExecutor::new(tool);
        assert_eq!(
            executor.definition().execution_mode,
            ExecutionMode::Parallel
        );
    }

    #[tokio::test]
    async fn test_execute_returns_tool_result() {
        let meta = make_meta("t", "d", false);
        let tool = Arc::new(MockExtTool {
            meta,
            output: "output".into(),
        });
        let executor = ExtensionToolExecutor::new(tool);
        let result = executor.execute(serde_json::json!({"x": 1})).await.unwrap();
        assert_eq!(result, "output");
    }

    #[tokio::test]
    async fn test_execute_with_context_delegates() {
        let meta = make_meta("t", "d", false);
        let tool = Arc::new(MockExtTool {
            meta,
            output: "output".into(),
        });
        let executor = ExtensionToolExecutor::new(tool);
        let ctx = ToolContext {
            cancel_token: tokio_util::sync::CancellationToken::new(),
            on_progress: None,
            tool_call_id: "t1".into(),
            execution_env: None,
            allowed_paths: vec![],
        };
        let result = executor
            .execute_with_context(serde_json::json!({"x": 1}), ctx)
            .await
            .unwrap();
        assert_eq!(result.text_content(), "output");
        assert!(!result.is_error);
    }
}
