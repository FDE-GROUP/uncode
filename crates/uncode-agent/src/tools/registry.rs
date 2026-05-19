use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn ToolExecutor>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::with_capacity(8)),
        }
    }

    pub fn register(&self, name: impl Into<String>, tool: Arc<dyn ToolExecutor>) {
        self.tools.write().insert(name.into(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.read().values().map(|t| t.definition()).collect()
    }

    pub fn list(&self) -> Vec<String> {
        self.tools.read().keys().cloned().collect()
    }

    /// Get the execution mode for a named tool (defaults to Parallel)
    pub fn execution_mode(&self, name: &str) -> ExecutionMode {
        self.tools
            .read()
            .get(name)
            .map(|t| t.definition().execution_mode)
            .unwrap_or_default()
    }

    /// Get a tool's display label (falls back to name)
    pub fn label_for(&self, name: &str) -> String {
        self.tools
            .read()
            .get(name)
            .map(|t| {
                t.definition()
                    .label
                    .clone()
                    .unwrap_or_else(|| t.definition().name.clone())
            })
            .unwrap_or_else(|| name.to_string())
    }

    /// Check if all named tools can run in parallel
    pub fn can_run_parallel(&self, tool_names: &[String]) -> bool {
        tool_names
            .iter()
            .all(|name| self.execution_mode(name) == ExecutionMode::Parallel)
    }

    /// Validate tool arguments against the tool's JSON Schema parameters.
    /// Returns Ok(()) if valid, Err with details if not.
    pub fn validate(&self, name: &str, args: &serde_json::Value) -> Result<(), String> {
        let tool = self.tools.read().get(name).cloned();
        let Some(tool) = tool else {
            return Err(format!("unknown tool: {name}"));
        };
        let schema = &tool.definition().parameters;
        // Check required fields
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            if let Some(props) = args.as_object() {
                for req in required {
                    if let Some(key) = req.as_str()
                        && !props.contains_key(key)
                    {
                        return Err(format!("missing required parameter: {key}"));
                    }
                }
            } else if !required.is_empty() {
                return Err("arguments must be an object".into());
            }
        }
        Ok(())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use uncode_core::error::UncodeError;
    use uncode_core::tool::{ToolDefinition, ToolExecutor};

    struct FakeTool {
        def: ToolDefinition,
    }

    #[async_trait]
    impl ToolExecutor for FakeTool {
        fn definition(&self) -> ToolDefinition {
            self.def.clone()
        }
        async fn execute(&self, _arguments: serde_json::Value) -> Result<String, UncodeError> {
            Ok("ok".into())
        }
    }

    #[test]
    fn test_validate_missing_required() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read file".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        let err = reg.validate("read", &serde_json::json!({})).unwrap_err();
        assert!(err.contains("missing required parameter: path"));
    }

    #[test]
    fn test_validate_ok() {
        let reg = ToolRegistry::new();
        reg.register(
            "read",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "read".into(),
                    description: "Read file".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": {"type": "string"} }
                    }),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );

        assert!(
            reg.validate("read", &serde_json::json!({"path": "/foo"}))
                .is_ok()
        );
    }

    #[test]
    fn test_validate_unknown_tool() {
        let reg = ToolRegistry::new();
        let err = reg.validate("nope", &serde_json::json!({})).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[test]
    fn test_validate_non_object_with_required() {
        let reg = ToolRegistry::new();
        reg.register(
            "tool",
            Arc::new(FakeTool {
                def: ToolDefinition {
                    name: "tool".into(),
                    description: "".into(),
                    parameters: serde_json::json!({"required": ["x"]}),
                    label: None,
                    execution_mode: Default::default(),
                },
            }),
        );
        let err = reg
            .validate("tool", &serde_json::json!("string"))
            .unwrap_err();
        assert!(err.contains("arguments must be an object"));
    }
}
