use std::path::PathBuf;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

use super::bash_exec::{BashExecArgs, BashStreamContext, exec_bash_simple, exec_bash_streaming};

const DEFAULT_MAX_OUTPUT_BYTES: usize = 50 * 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 120;

pub struct BashTool {
    max_output_bytes: usize,
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

struct ParsedArgs {
    command: String,
    workdir: PathBuf,
    timeout_secs: u64,
}

fn parse_args(
    arguments: &serde_json::Value,
) -> Result<ParsedArgs, uncode_core::error::UncodeError> {
    let command = arguments["command"]
        .as_str()
        .ok_or_else(|| uncode_core::error::UncodeError::Tool("command required".into()))?
        .to_string();
    let workdir_raw = arguments["workdir"].as_str().unwrap_or(".");
    let workdir =
        super::resolve_path(workdir_raw).map_err(uncode_core::error::UncodeError::Tool)?;
    let timeout_secs = arguments["timeout"]
        .as_u64()
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Ok(ParsedArgs {
        command,
        workdir,
        timeout_secs,
    })
}

fn to_exec_args(parsed: ParsedArgs, max_output_bytes: usize) -> BashExecArgs {
    BashExecArgs {
        command: parsed.command,
        workdir: parsed.workdir,
        timeout_secs: parsed.timeout_secs,
        max_output_bytes,
    }
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".into(),
            description: "在项目目录内执行 bash 命令（workdir 受路径沙箱约束），支持超时和实时取消"
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "command": {"type": "string", "description": "要执行的 bash 命令"},
                    "description": {"type": "string", "description": "5-10 个词的清晰简洁描述"},
                    "workdir": {"type": "string", "description": "工作目录（相对于项目根目录）"},
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 86400,
                        "description": "超时秒数，默认 120"
                    }
                },
                "required": ["command"]
            }),
            label: Some("Shell Command".into()),
            execution_mode: ExecutionMode::Sequential,
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let parsed = parse_args(&arguments)?;
        exec_bash_simple(to_exec_args(parsed, self.max_output_bytes)).await
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let _env = super::ctx_execution_env(&ctx);
        let parsed = parse_args(&arguments)?;
        Ok(exec_bash_streaming(
            to_exec_args(parsed, self.max_output_bytes),
            BashStreamContext {
                cancel_token: ctx.cancel_token,
                on_progress: ctx.on_progress,
            },
        )
        .await)
    }
}
