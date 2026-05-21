use async_trait::async_trait;
use regex::Regex;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

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
            description: "使用正则表达式搜索文件内容；默认遵守 .gitignore，跳过超过 1MB 的文件"
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

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?
            .to_string();

        let re = Regex::new(&pattern)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("invalid regex: {e}")))?;

        let search_path = super::resolve_path(arguments["path"].as_str().unwrap_or("."))
            .map_err(uncode_core::error::UncodeError::Tool)?;

        let glob_pattern = arguments["include"]
            .as_str()
            .map(glob::Pattern::new)
            .transpose()
            .map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("invalid include pattern: {e}"))
            })?;

        let max_results = self.max_results;
        let max_file_bytes = self.max_file_bytes;

        // Run blocking file I/O on a dedicated thread to avoid stalling the tokio runtime
        let result = tokio::task::spawn_blocking(move || {
            grep_files(
                &re,
                &search_path,
                glob_pattern.as_ref(),
                max_results,
                max_file_bytes,
            )
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("grep task failed: {e}")))?;

        Ok(result)
    }
}

fn grep_files(
    re: &Regex,
    search_path: &std::path::Path,
    glob_pattern: Option<&glob::Pattern>,
    max_results: usize,
    max_file_bytes: u64,
) -> String {
    let mut results = Vec::with_capacity(max_results.min(64));
    let mut count = 0;

    let walker = ignore::WalkBuilder::new(search_path)
        .standard_filters(true)
        .max_depth(Some(20))
        .build();

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if count >= max_results {
            results.push("... (truncated)".into());
            break;
        }

        if let Some(pat) = glob_pattern {
            let rel = path
                .strip_prefix(search_path)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            if !pat.matches(&rel) && !pat.matches(&file_name) {
                continue;
            }
        }

        if let Ok(meta) = std::fs::metadata(path)
            && meta.len() > max_file_bytes
        {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                results.push(format!("{}:{}: {}", path.display(), i + 1, line));
                count += 1;
                if count >= max_results {
                    break;
                }
            }
        }
    }

    if results.is_empty() {
        "no matches".into()
    } else {
        results.join("\n")
    }
}
