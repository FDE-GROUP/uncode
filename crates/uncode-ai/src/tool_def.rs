use serde::{Deserialize, Serialize};

/// 工具定义，传递给 LLM 的 JSON Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    /// Human-readable label for UI display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Execution mode: Parallel or Sequential
    #[serde(default)]
    pub execution_mode: ExecutionMode,
}

/// 工具执行模式：并行或顺序
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Parallel,
    Sequential,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_construction() {
        let def = ToolDefinition {
            name: "get_weather".into(),
            description: "Get current weather".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
            label: Some("Weather Tool".into()),
            execution_mode: ExecutionMode::Sequential,
        };
        assert_eq!(def.name, "get_weather");
        assert_eq!(def.description, "Get current weather");
        assert_eq!(def.label.unwrap(), "Weather Tool");
        assert_eq!(def.execution_mode, ExecutionMode::Sequential);
    }

    #[test]
    fn test_tool_definition_serde_roundtrip() {
        let def = ToolDefinition {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({"type": "object"}),
            label: Some("Search".into()),
            execution_mode: ExecutionMode::Parallel,
        };
        let json = serde_json::to_string(&def).unwrap();
        let de: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(de.name, "search");
        assert_eq!(de.label.unwrap(), "Search");
        assert_eq!(de.execution_mode, ExecutionMode::Parallel);
    }

    #[test]
    fn test_execution_mode_variants() {
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Parallel).unwrap(),
            r#""parallel""#
        );
        assert_eq!(
            serde_json::to_string(&ExecutionMode::Sequential).unwrap(),
            r#""sequential""#
        );
    }

    #[test]
    fn test_execution_mode_default_is_parallel() {
        let def = ToolDefinition {
            name: "x".into(),
            description: "y".into(),
            parameters: serde_json::json!({}),
            label: None,
            execution_mode: ExecutionMode::default(),
        };
        assert_eq!(def.execution_mode, ExecutionMode::Parallel);
    }

    #[test]
    fn test_serialization_skips_label_when_none() {
        let def = ToolDefinition {
            name: "x".into(),
            description: "y".into(),
            parameters: serde_json::json!({}),
            label: None,
            execution_mode: ExecutionMode::Parallel,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("label"));
    }
}
