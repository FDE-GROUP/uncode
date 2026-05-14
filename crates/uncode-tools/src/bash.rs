use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

pub struct BashTool {
    default_timeout_secs: u64,
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            default_timeout_secs: 120,
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".into(),
            description: "执行 shell 命令".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "要执行的命令"},
                    "workdir": {"type": "string", "description": "工作目录"},
                    "timeout": {"type": "integer", "description": "超时秒数"}
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let command = arguments["command"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("command required".into()))?;

        let workdir = arguments["workdir"].as_str().unwrap_or(".");
        let timeout = arguments["timeout"]
            .as_u64()
            .unwrap_or(self.default_timeout_secs);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(workdir)
                .output(),
        )
        .await
        .map_err(|_| uncode_core::error::UncodeError::Tool("timeout".into()))?
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("bash: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut parts: Vec<String> = Vec::new();
        if !stdout.is_empty() {
            parts.push(stdout.into_owned());
        }
        if !stderr.is_empty() {
            parts.push(format!("stderr:\n{stderr}"));
        }
        if !output.status.success() {
            parts.push(format!("exit code: {}", output.status.code().unwrap_or(-1)));
        }

        Ok(parts.join("\n"))
    }
}
