use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use uncode_core::error::UncodeResult;
use uncode_core::tool::{
    ExecutionMode, ToolContent, ToolContext, ToolDefinition, ToolExecutor, ToolProgress, ToolResult,
};

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
            label: Some("Shell Command".into()),
            execution_mode: ExecutionMode::default(),
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

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let command = arguments["command"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("command required".into()))?;

        let workdir = arguments["workdir"].as_str().unwrap_or(".").to_string();
        let timeout = arguments["timeout"]
            .as_u64()
            .unwrap_or(self.default_timeout_secs);

        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&workdir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("spawn: {e}")))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let mut output = String::with_capacity(4096);
        let mut errors = String::new();

        // Read stdout lines with cancellation
        let mut stdout_lines = stdout_reader.lines();
        loop {
            if ctx.cancel_token.is_cancelled() {
                child.kill().await.ok();
                return Ok(ToolResult::err("cancelled"));
            }
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    child.kill().await.ok();
                    return Ok(ToolResult::err("cancelled"));
                }
                line = stdout_lines.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if let Some(ref cb) = ctx.on_progress {
                                cb(ToolProgress::LogLine(l.clone()));
                            }
                            output.push_str(&l);
                            output.push('\n');
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }

        // Read remaining stderr
        let mut stderr_lines = stderr_reader.lines();
        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    child.kill().await.ok();
                    return Ok(ToolResult::err("cancelled"));
                }
                line = stderr_lines.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            errors.push_str(&l);
                            errors.push('\n');
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }

        let status =
            match tokio::time::timeout(std::time::Duration::from_secs(timeout), child.wait()).await
            {
                Ok(Ok(s)) => s,
                _ => {
                    child.kill().await.ok();
                    return Ok(ToolResult::err("timeout"));
                }
            };

        if !errors.is_empty() {
            output.push_str("stderr:\n");
            output.push_str(&errors);
        }
        if !status.success() {
            output.push_str(&format!("exit code: {}\n", status.code().unwrap_or(-1)));
        }

        let is_error = !status.success();
        Ok(ToolResult {
            content: vec![ToolContent::Text(output)],
            is_error,
            details: None,
            terminate: false,
        })
    }
}
