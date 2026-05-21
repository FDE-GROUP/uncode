use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

const MAX_DIR_ENTRIES: usize = 500;

pub struct ReadTool {
    max_size: usize,
}

impl ReadTool {
    pub fn new() -> Self {
        Self::with_max_file_bytes(1024 * 1024)
    }

    pub fn with_max_file_bytes(max_size: usize) -> Self {
        Self { max_size }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

fn format_directory_listing(
    display: &str,
    entries: Vec<uncode_core::tool::DirEntry>,
) -> (String, bool) {
    let mut names: Vec<String> = entries
        .into_iter()
        .map(|e| {
            if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name
            }
        })
        .take(MAX_DIR_ENTRIES + 1)
        .collect();
    names.sort();
    let truncated = names.len() > MAX_DIR_ENTRIES;
    if truncated {
        names.truncate(MAX_DIR_ENTRIES);
        names.push("... (truncated)".into());
    }
    (
        format!("Directory listing for {display}:\n{}", names.join("\n")),
        truncated,
    )
}

fn format_file_content(
    content: &str,
    offset: usize,
    limit: Option<usize>,
    hashline: bool,
) -> String {
    if hashline {
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
    }
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

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, uncode_core::error::UncodeError> {
        super::prepare_arguments_path(arguments, "path", None)
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let tr = self
            .execute_with_context(
                arguments,
                ToolContext {
                    cancel_token: tokio_util::sync::CancellationToken::new(),
                    on_progress: None,
                    tool_call_id: String::new(),
                    execution_env: None,
                },
            )
            .await?;
        Ok(tr.text_content())
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let resolved = super::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let offset = arguments["offset"].as_u64().unwrap_or(0) as usize;
        let limit = arguments["limit"].as_u64().map(|l| l as usize);
        let hashline = arguments["hashline"].as_bool().unwrap_or(false);
        let max_size = self.max_size;

        let env = super::ctx_execution_env(&ctx);
        let info =
            env.fs().file_info(&resolved).await.map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("read {display}: {e}"))
            })?;

        if info.is_dir {
            let entries = env.fs().list_dir(&resolved).await.map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("read dir {display}: {e}"))
            })?;
            let (listing, truncated) = format_directory_listing(&display, entries);
            let mut result = ToolResult::ok(listing);
            if truncated {
                result = result.with_details(serde_json::json!({
                    "truncated": true,
                    "entry_limit": MAX_DIR_ENTRIES,
                }));
            }
            return Ok(result);
        }

        if info.size > max_size as u64 {
            return Ok(ToolResult::err_with_details(
                format!("file too large ({} bytes, max {max_size})", info.size),
                serde_json::json!({
                    "reason": "file_too_large",
                    "size_bytes": info.size,
                    "max_bytes": max_size,
                }),
            ));
        }

        let content =
            env.fs().read_text_file(&resolved).await.map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("read {display}: {e}"))
            })?;

        let formatted = tokio::task::spawn_blocking(move || {
            format_file_content(&content, offset, limit, hashline)
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("read task failed: {e}")))?;

        Ok(ToolResult::ok(formatted))
    }
}
