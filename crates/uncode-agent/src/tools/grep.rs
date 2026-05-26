use std::path::{Path, PathBuf};

use async_trait::async_trait;
use regex::Regex;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

pub struct GrepTool {
    max_results: usize,
    max_file_bytes: u64,
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new(50, 1024 * 1024)
    }
}

impl GrepTool {
    pub fn new(max_results: usize, max_file_bytes: usize) -> Self {
        Self {
            max_results,
            max_file_bytes: max_file_bytes as u64,
        }
    }
}

#[async_trait]
impl ToolExecutor for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".into(),
            description: "使用正则表达式搜索文件内容；已安装 ripgrep (`rg`) 时优先使用，否则回退内置实现；默认遵守 .gitignore，跳过超过 1MB 的文件"
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "pattern": {"type": "string", "description": "正则表达式"},
                    "path": {"type": "string", "description": "搜索目录路径（相对或绝对），默认当前目录"},
                    "include": {"type": "string", "description": "文件名或相对路径 glob，如 *.rs、src/*.rs（相对搜索根目录）"}
                },
                "required": ["pattern"]
            }),
            label: Some("Search".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, uncode_core::error::UncodeError> {
        super::prepare_arguments_path(arguments, "path", Some("."), &[])
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
                    allowed_paths: Vec::new(),
                    subagent_runner: None,
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
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?
            .to_string();

        let search_path = super::resolve_path(
            arguments["path"].as_str().unwrap_or("."),
            &ctx.allowed_paths,
        )
        .map_err(uncode_core::error::UncodeError::Tool)?;

        let include = arguments["include"].as_str().map(str::to_string);
        let max_results = self.max_results;
        let max_file_bytes = self.max_file_bytes;
        let pattern_for_rg = pattern.clone();
        let search_path_for_rg = search_path.clone();

        let rg_result = tokio::task::spawn_blocking(move || {
            super::grep_rg::try_search(
                &pattern_for_rg,
                &search_path_for_rg,
                include.as_deref(),
                max_results,
                max_file_bytes,
            )
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("grep task failed: {e}")))?;
        if let Some(rg_out) = rg_result.map_err(uncode_core::error::UncodeError::Tool)? {
            let (output, match_count, truncated) = rg_out;
            return Ok(ToolResult::ok(output).with_details(serde_json::json!({
                "match_count": match_count,
                "truncated": truncated,
                "backend": "ripgrep",
            })));
        }

        let re = Regex::new(&pattern)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("invalid regex: {e}")))?;

        let glob_pattern = arguments["include"]
            .as_str()
            .map(glob::Pattern::new)
            .transpose()
            .map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("invalid include pattern: {e}"))
            })?;

        let paths = tokio::task::spawn_blocking(move || {
            collect_grep_file_paths(&search_path, glob_pattern.as_ref())
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("grep task failed: {e}")))?;

        let env = super::ctx_execution_env(&ctx);
        let (output, match_count, truncated) = grep_paths(
            env.as_ref(),
            &re,
            paths,
            self.max_results,
            self.max_file_bytes,
        )
        .await;

        Ok(ToolResult::ok(output).with_details(serde_json::json!({
            "match_count": match_count,
            "truncated": truncated,
            "backend": "native",
        })))
    }
}

fn collect_grep_file_paths(
    search_path: &Path,
    glob_pattern: Option<&glob::Pattern>,
) -> Vec<PathBuf> {
    let walker = ignore::WalkBuilder::new(search_path)
        .standard_filters(true)
        .max_depth(Some(20))
        .build();

    walker
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            if let Some(pat) = glob_pattern {
                let rel = path
                    .strip_prefix(search_path)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                if !pat.matches(&rel) && !pat.matches(&file_name) {
                    return None;
                }
            }
            Some(path.to_path_buf())
        })
        .collect()
}

async fn grep_paths(
    env: &dyn uncode_core::tool::ExecutionEnv,
    re: &Regex,
    paths: Vec<PathBuf>,
    max_results: usize,
    max_file_bytes: u64,
) -> (String, usize, bool) {
    let mut results = Vec::with_capacity(max_results.min(64));
    let mut count = 0;
    let mut truncated = false;

    for path in paths {
        if count >= max_results {
            results.push("... (truncated)".into());
            truncated = true;
            break;
        }

        let info = match env.fs().file_info(&path).await {
            Ok(i) => i,
            Err(_) => continue,
        };
        if info.size > max_file_bytes {
            continue;
        }

        let content = match env.fs().read_text_file(&path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                results.push(format!("{}:{}: {}", path.display(), i + 1, line));
                count += 1;
                if count >= max_results {
                    truncated = true;
                    break;
                }
            }
        }
    }

    let text = if results.is_empty() {
        "no matches".into()
    } else {
        results.join("\n")
    };
    (text, count, truncated)
}
