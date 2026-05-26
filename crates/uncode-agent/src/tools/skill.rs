use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use uncode_core::error::{UncodeError, UncodeResult};
use uncode_core::skill::SkillRegistry;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

pub struct SkillTool {
    registry: std::sync::Arc<parking_lot::RwLock<SkillRegistry>>,
}

impl SkillTool {
    pub fn new(registry: SkillRegistry) -> Self {
        Self {
            registry: std::sync::Arc::new(parking_lot::RwLock::new(registry)),
        }
    }
}

impl Default for SkillTool {
    fn default() -> Self {
        Self::new(SkillRegistry::load_with_project(
            &std::env::current_dir().unwrap_or_default(),
        ))
    }
}

#[async_trait]
impl ToolExecutor for SkillTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "skill".to_string(),
            description: "Load a specialized skill that provides domain-specific instructions and workflows. Use this when a task matches a specific skill's description.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of the skill to load"
                    }
                },
                "required": ["name"]
            }),
            label: Some("load_skill".to_string()),
            execution_mode: ExecutionMode::Parallel,
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, UncodeError> {
        let name = arguments["name"]
            .as_str()
            .ok_or_else(|| UncodeError::Tool("skill name required".into()))?;

        let registry = self.registry.read();
        let available = registry.list();
        let names: Vec<&str> = available.iter().map(|s| s.name.as_str()).collect();

        if !names.contains(&name) {
            return Ok(format!(
                "Skill '{name}' not found. Available skills: {}",
                names.join(", ")
            ));
        }

        match registry.render(name, &HashMap::new()) {
            Some(content) => Ok(content),
            None => Ok(format!("Skill '{name}' has no content")),
        }
    }

    async fn execute_with_context(
        &self,
        arguments: Value,
        _ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let name = arguments["name"]
            .as_str()
            .ok_or_else(|| UncodeError::Tool("skill name required".into()))?;

        let registry = self.registry.read();
        let available = registry.list();
        let names: Vec<&str> = available.iter().map(|s| s.name.as_str()).collect();

        if !names.contains(&name) {
            return Ok(ToolResult::ok(format!(
                "Skill '{name}' not found. Available skills: {}",
                names.join(", ")
            )));
        }

        match registry.render(name, &HashMap::new()) {
            Some(content) => Ok(ToolResult::ok(content).with_details(serde_json::json!({
                "title": format!("Skill: {name}"),
                "skill_name": name
            }))),
            None => Ok(ToolResult::ok(format!("Skill '{name}' loaded (no body)"))),
        }
    }
}
