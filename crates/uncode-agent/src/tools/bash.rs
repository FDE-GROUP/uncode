use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use uncode_core::error::UncodeResult;
use uncode_core::tool::{
    ExecutionMode, ToolContent, ToolContext, ToolDefinition, ToolExecutor, ToolProgress, ToolResult,
};

use super::local_env::{clean_binary_output, truncate_output};

const DEFAULT_MAX_OUTPUT_BYTES: usize = 50 * 1024; // 50KB
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
    workdir: String,
    timeout_secs: u64,
}

fn parse_args(
    arguments: &serde_json::Value,
) -> Result<ParsedArgs, uncode_core::error::UncodeError> {
    let command = arguments["command"]
        .as_str()
        .ok_or_else(|| uncode_core::error::UncodeError::Tool("command required".into()))?
        .to_string();
    let workdir = arguments["workdir"].as_str().unwrap_or(".").to_string();
    let timeout_secs = arguments["timeout"]
        .as_u64()
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Ok(ParsedArgs {
        command,
        workdir,
        timeout_secs,
    })
}

/// Kill an entire process group by sending SIGKILL to `-pgid`.
/// Requires the child to have been spawned with `process_group(0)`.
#[cfg(unix)]
#[allow(unsafe_code)]
fn kill_process_group(pgid: u32) {
    unsafe {
        libc::kill(-(pgid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pgid: u32) {}

fn build_command(command: &str, workdir: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c").arg(command).current_dir(workdir);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

fn build_result(
    stdout: &str,
    stderr: &str,
    exit_ok: bool,
    exit_code: Option<i32>,
    max_output_bytes: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !stdout.is_empty() {
        parts.push(truncate_output(stdout, max_output_bytes));
    }
    if !stderr.is_empty() {
        parts.push(format!(
            "stderr:\n{}",
            truncate_output(stderr, max_output_bytes)
        ));
    }
    if !exit_ok {
        parts.push(format!("exit code: {}", exit_code.unwrap_or(-1)));
    }
    parts.join("\n")
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".into(),
            description: "在沙箱中执行 bash 命令，支持描述、工作目录、超时和实时取消".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "要执行的 bash 命令"},
                    "description": {"type": "string", "description": "5-10 个词的清晰简洁描述"},
                    "workdir": {"type": "string", "description": "工作目录（相对于项目根目录）"},
                    "timeout": {"type": "integer", "description": "超时秒数，默认 120"}
                },
                "required": ["command"]
            }),
            label: Some("Shell Command".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let args = parse_args(&arguments)?;
        let mut cmd = build_command(&args.command, &args.workdir);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(args.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| uncode_core::error::UncodeError::Tool("timeout".into()))?
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("bash: {e}")))?;

        let stdout = clean_binary_output(&output.stdout);
        let stderr = clean_binary_output(&output.stderr);
        Ok(build_result(
            &stdout,
            &stderr,
            output.status.success(),
            output.status.code(),
            self.max_output_bytes,
        ))
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let args = parse_args(&arguments)?;
        let mut cmd = build_command(&args.command, &args.workdir);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("spawn: {e}")))?;

        let pgid = child.id().unwrap_or(0);
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let mut output = String::with_capacity(4096);
        let mut errors = String::new();

        // Read stdout with cancellation
        let mut stdout_lines = BufReader::new(stdout).lines();
        loop {
            if ctx.cancel_token.is_cancelled() {
                kill_process_group(pgid);
                return Ok(ToolResult::err("cancelled"));
            }
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    kill_process_group(pgid);
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

        // Read stderr
        let mut stderr_lines = BufReader::new(stderr).lines();
        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    kill_process_group(pgid);
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

        let status = match tokio::time::timeout(
            std::time::Duration::from_secs(args.timeout_secs),
            child.wait(),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                kill_process_group(pgid);
                return Ok(ToolResult::err("timeout"));
            }
        };

        if !errors.is_empty() {
            output.push_str("stderr:\n");
            output.push_str(&errors);
        }
        let exit_ok = status.success();
        let exit_code = status.code();
        if !exit_ok {
            use std::fmt::Write;
            let _ = write!(output, "exit code: {}\n", exit_code.unwrap_or(-1));
        }

        let output = truncate_output(&output, self.max_output_bytes);
        Ok(ToolResult {
            content: vec![ToolContent::Text(output)],
            is_error: !exit_ok,
            details: None,
            terminate: false,
        })
    }
}
