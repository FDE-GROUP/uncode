//! Bash 命令执行 — `LocalShell` / `BashTool` 共享实现。
//!
//! Agent 主路径支持流式 stdout、`on_progress` 与 `CancellationToken`；
//! 简单路径用于 `execute()` 与 `LocalShell::exec`。

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uncode_core::error::UncodeError;
use uncode_core::tool::{ToolContent, ToolProgress, ToolResult};

use super::local_env::{clean_binary_output, truncate_output};

fn bash_timeout_result() -> ToolResult {
    ToolResult::err_with_details("timeout", serde_json::json!({ "reason": "timeout" }))
}

fn bash_cancelled_result() -> ToolResult {
    ToolResult::err_with_details("cancelled", serde_json::json!({ "reason": "cancelled" }))
}

fn remaining_until(deadline: tokio::time::Instant) -> std::time::Duration {
    deadline.saturating_duration_since(tokio::time::Instant::now())
}

/// Kill an entire process group (`process_group(0)` on spawn).
#[cfg(unix)]
#[allow(unsafe_code)]
pub fn kill_process_group(pgid: u32) {
    unsafe {
        libc::kill(-(pgid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
pub fn kill_process_group(_pgid: u32) {}

pub fn build_bash_command(command: &str, workdir: &Path) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(command).current_dir(workdir);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

pub fn format_bash_output(
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

pub struct BashExecArgs {
    pub command: String,
    pub workdir: PathBuf,
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
}

pub struct BashStreamContext {
    pub cancel_token: CancellationToken,
    pub on_progress: Option<Box<dyn Fn(ToolProgress) + Send + Sync>>,
}

/// 一次性等待输出（`BashTool::execute` / 测试路径）。
pub async fn exec_bash_simple(args: BashExecArgs) -> Result<String, UncodeError> {
    let mut cmd = build_bash_command(&args.command, &args.workdir);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| UncodeError::Tool(format!("bash: {e}")))?;
    let pgid = child.id().unwrap_or(0);

    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(args.timeout_secs),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(UncodeError::Tool(format!("bash: {e}"))),
        Err(_) => {
            kill_process_group(pgid);
            return Err(UncodeError::Tool("timeout".into()));
        }
    };

    let stdout = clean_binary_output(&output.stdout);
    let stderr = clean_binary_output(&output.stderr);
    Ok(format_bash_output(
        &stdout,
        &stderr,
        output.status.success(),
        output.status.code(),
        args.max_output_bytes,
    ))
}

/// Agent 主路径：流式 stdout、取消、输出上限。
pub async fn exec_bash_streaming(args: BashExecArgs, ctx: BashStreamContext) -> ToolResult {
    let mut cmd = build_bash_command(&args.command, &args.workdir);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("spawn: {e}")),
    };

    let pgid = child.id().unwrap_or(0);
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut output = String::with_capacity(4096);
    let mut errors = String::new();
    let mut truncated = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(args.timeout_secs);

    let mut stdout_lines = BufReader::new(stdout).lines();
    loop {
        if ctx.cancel_token.is_cancelled() {
            kill_process_group(pgid);
            return bash_cancelled_result();
        }
        if remaining_until(deadline).is_zero() {
            kill_process_group(pgid);
            return bash_timeout_result();
        }
        tokio::select! {
            _ = ctx.cancel_token.cancelled() => {
                kill_process_group(pgid);
                return bash_cancelled_result();
            }
            line = tokio::time::timeout(remaining_until(deadline), stdout_lines.next_line()) => {
                match line {
                    Err(_) => {
                        kill_process_group(pgid);
                        return bash_timeout_result();
                    }
                    Ok(Ok(Some(l))) => {
                        if output.len() >= args.max_output_bytes {
                            kill_process_group(pgid);
                            truncated = true;
                            output.push_str("\n[truncated]");
                            break;
                        }
                        if let Some(ref cb) = ctx.on_progress {
                            cb(ToolProgress::LogLine(l.clone()));
                        }
                        output.push_str(&l);
                        output.push('\n');
                        if output.len() >= args.max_output_bytes {
                            kill_process_group(pgid);
                            truncated = true;
                            output.push_str("\n[truncated]");
                            break;
                        }
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(_)) => break,
                }
            }
        }
    }

    let mut stderr_lines = BufReader::new(stderr).lines();
    loop {
        if ctx.cancel_token.is_cancelled() {
            kill_process_group(pgid);
            return bash_cancelled_result();
        }
        if remaining_until(deadline).is_zero() {
            kill_process_group(pgid);
            return bash_timeout_result();
        }
        tokio::select! {
            _ = ctx.cancel_token.cancelled() => {
                kill_process_group(pgid);
                return bash_cancelled_result();
            }
            line = tokio::time::timeout(remaining_until(deadline), stderr_lines.next_line()) => {
                match line {
                    Err(_) => {
                        kill_process_group(pgid);
                        return bash_timeout_result();
                    }
                    Ok(Ok(Some(l))) => {
                        errors.push_str(&l);
                        errors.push('\n');
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(_)) => break,
                }
            }
        }
    }

    let wait_budget = remaining_until(deadline);
    if wait_budget.is_zero() {
        kill_process_group(pgid);
        return bash_timeout_result();
    }
    let status = match tokio::time::timeout(wait_budget, child.wait()).await {
        Ok(Ok(s)) => s,
        _ => {
            kill_process_group(pgid);
            return bash_timeout_result();
        }
    };

    if !errors.is_empty() {
        output.push_str("stderr:\n");
        output.push_str(&errors);
    }
    let exit_ok = status.success();
    let exit_code = status.code().unwrap_or(-1);
    if !exit_ok {
        use std::fmt::Write;
        let _ = writeln!(output, "exit code: {exit_code}");
    }

    let output = truncate_output(&output, args.max_output_bytes);
    if output.contains("[truncated]") {
        truncated = true;
    }
    ToolResult {
        content: vec![ToolContent::Text(output)],
        is_error: !exit_ok,
        details: Some(serde_json::json!({
            "exit_code": exit_code,
            "truncated": truncated,
        })),
        terminate: false,
    }
}

/// `LocalShell::exec` — 合并 stdout/stderr，超时杀进程组。
pub async fn shell_exec_bash(
    cmd: &str,
    workdir: Option<PathBuf>,
    timeout_ms: u64,
) -> Result<(String, String, i32), UncodeError> {
    let workdir = workdir.unwrap_or_else(|| PathBuf::from("."));
    let mut command = build_bash_command(cmd, &workdir);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child = command.spawn().map_err(|e| {
        UncodeError::Execution(uncode_core::error::ExecutionError::Other {
            message: format!("spawn: {e}"),
            code: 2099,
        })
    })?;
    let pgid = child.id().unwrap_or(0);

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| {
        kill_process_group(pgid);
        UncodeError::Execution(uncode_core::error::ExecutionError::Timeout {
            command: cmd.to_string(),
            timeout_ms,
            code: 2002,
        })
    })?
    .map_err(|e| {
        UncodeError::Execution(uncode_core::error::ExecutionError::Other {
            message: format!("spawn: {e}"),
            code: 2099,
        })
    })?;

    Ok((
        clean_binary_output(&output.stdout),
        clean_binary_output(&output.stderr),
        output.status.code().unwrap_or(-1),
    ))
}
