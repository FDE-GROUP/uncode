use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

const MAX_DIR_ENTRIES: usize = 500;

pub struct ReadTool {
    max_size: usize,
}

impl ReadTool {
    pub fn new() -> Self {
        Self {
            max_size: 1024 * 1024,
        }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

struct ReadParams {
    resolved: PathBuf,
    max_size: usize,
    offset: usize,
    limit: Option<usize>,
    hashline: bool,
}

fn read_blocking(params: ReadParams) -> UncodeResult<String> {
    let ReadParams {
        resolved,
        max_size,
        offset,
        limit,
        hashline,
    } = params;

    let meta = fs::metadata(&resolved).map_err(|e| {
        uncode_core::error::UncodeError::Tool(format!("read {}: {e}", resolved.display()))
    })?;

    if meta.is_dir() {
        let mut entries: Vec<String> = fs::read_dir(&resolved)
            .map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!(
                    "read dir {}: {e}",
                    resolved.display()
                ))
            })?
            .filter_map(Result::ok)
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if e.path().is_dir() {
                    format!("{name}/")
                } else {
                    name
                }
            })
            .take(MAX_DIR_ENTRIES + 1)
            .collect();
        entries.sort();
        let truncated = entries.len() > MAX_DIR_ENTRIES;
        if truncated {
            entries.truncate(MAX_DIR_ENTRIES);
            entries.push("... (truncated)".into());
        }
        return Ok(format!(
            "Directory listing for {}:\n{}",
            resolved.display(),
            entries.join("\n")
        ));
    }

    if meta.len() > max_size as u64 {
        return Err(uncode_core::error::UncodeError::Tool(format!(
            "file too large ({} bytes, max {})",
            meta.len(),
            max_size
        )));
    }

    let content = fs::read_to_string(&resolved).map_err(|e| {
        uncode_core::error::UncodeError::Tool(format!("read {}: {e}", resolved.display()))
    })?;

    let result = if hashline {
        content
            .lines()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .enumerate()
            .fold(
                String::with_capacity(limit.unwrap_or(0).min(content.len()).saturating_mul(100)),
                |mut acc, (i, line)| {
                    use std::fmt::Write;
                    let hash = super::hashline::compute_line_hash(line);
                    let hash_str = std::str::from_utf8(&hash).unwrap_or("??");
                    let _ = writeln!(acc, "{:>6}#{hash_str} {line}", offset + i + 1);
                    acc
                },
            )
    } else {
        content
            .lines()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .enumerate()
            .fold(
                String::with_capacity(limit.unwrap_or(0).min(content.len()).saturating_mul(80)),
                |mut acc, (i, line)| {
                    use std::fmt::Write;
                    let _ = writeln!(acc, "{:>6}: {line}", offset + i + 1);
                    acc
                },
            )
    };

    Ok(result)
}

#[async_trait]
impl ToolExecutor for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".into(),
            description: "读取文件内容或列出目录。文件时支持 offset/limit，目录时自动列出内容。"
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "offset": {
                        "type": "integer",
                        "description": "跳过的行数（0 表示从第一行开始）；显示行号 = offset + 行序号"
                    },
                    "limit": {"type": "integer", "description": "最多读取的行数"},
                    "hashline": {"type": "boolean", "description": "If true, prepend LINE#HASH anchor to each line for use with edit tool"}
                },
                "required": ["path"]
            }),
            label: Some("Read File".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let resolved = super::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;

        let offset = arguments["offset"].as_u64().unwrap_or(0) as usize;
        let limit = arguments["limit"].as_u64().map(|l| l as usize);
        let hashline = arguments["hashline"].as_bool().unwrap_or(false);
        let max_size = self.max_size;

        tokio::task::spawn_blocking(move || {
            read_blocking(ReadParams {
                resolved,
                max_size,
                offset,
                limit,
                hashline,
            })
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("read task failed: {e}")))?
    }
}
