//! 可选 ripgrep（`rg`）后端：与自研 walk+regex 输出格式一致，不可用时由 `grep` 回退。

use std::path::Path;
use std::process::Command;

pub fn rg_available() -> bool {
    Command::new("rg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `Ok(None)` — 未安装 `rg`；`Ok(Some(...))` — 已用 ripgrep 完成搜索。
pub fn try_search(
    pattern: &str,
    search_path: &Path,
    include: Option<&str>,
    max_results: usize,
    max_file_bytes: u64,
) -> Result<Option<(String, usize, bool)>, String> {
    if !rg_available() {
        return Ok(None);
    }

    let max_filesize = max_file_bytes.to_string();
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number")
        .arg("--no-heading")
        .arg("--color")
        .arg("never")
        .arg("--max-filesize")
        .arg(max_filesize)
        .arg("-e")
        .arg(pattern)
        .arg(search_path);

    if let Some(glob) = include {
        cmd.arg("--glob").arg(glob);
    }

    let output = cmd.output().map_err(|e| format!("rg spawn: {e}"))?;

    match output.status.code() {
        Some(0) => {}
        Some(1) => {
            return Ok(Some(("no matches".into(), 0, false)));
        }
        code => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "rg failed (exit {}): {}",
                code.unwrap_or(-1),
                stderr.trim()
            ));
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines: Vec<String> = Vec::new();
    let mut truncated = false;

    for line in stdout.lines() {
        if lines.len() >= max_results {
            truncated = true;
            break;
        }
        lines.push(format_match_line(line));
    }

    let match_count = lines.len();
    let text = if lines.is_empty() {
        "no matches".into()
    } else {
        if truncated {
            lines.push("... (truncated)".into());
        }
        lines.join("\n")
    };

    Ok(Some((text, match_count, truncated)))
}

fn format_match_line(line: &str) -> String {
    let Some((path, rest)) = line.split_once(':') else {
        return line.to_string();
    };
    let display = super::display_path_within_project(Path::new(path));
    if let Some((line_no, content)) = rest.split_once(':') {
        format!("{display}:{line_no}:{content}")
    } else {
        format!("{display}:{rest}")
    }
}
