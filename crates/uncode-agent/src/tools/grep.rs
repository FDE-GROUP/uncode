use async_trait::async_trait;
use regex::Regex;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

#[derive(Default)]
pub struct GrepTool;

#[async_trait]
impl ToolExecutor for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".into(),
            description: "使用正则表达式搜索文件内容".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "正则表达式"},
                    "path": {"type": "string", "description": "搜索目录路径（相对或绝对），默认当前目录"},
                    "include": {"type": "string", "description": "文件匹配模式，如 *.rs、**/*.toml、src/*.rs"}
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

        // Run blocking file I/O on a dedicated thread to avoid stalling the tokio runtime
        let result = tokio::task::spawn_blocking(move || {
            grep_files(&re, &search_path, glob_pattern.as_ref())
        })
        .await
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("grep task failed: {e}")))?;

        Ok(result)
    }
}

const MAX_RESULTS: usize = 50;

fn grep_files(
    re: &Regex,
    search_path: &std::path::Path,
    glob_pattern: Option<&glob::Pattern>,
) -> String {
    let mut results = Vec::with_capacity(MAX_RESULTS);
    let mut count = 0;

    for entry in walkdir::WalkDir::new(search_path)
        .max_depth(20)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if count >= MAX_RESULTS {
            results.push("... (truncated)".into());
            break;
        }

        if let Some(pat) = glob_pattern {
            let file_name = entry.file_name().to_string_lossy();
            if !pat.matches(&file_name) {
                continue;
            }
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                results.push(format!("{}:{}: {}", entry.path().display(), i + 1, line));
                count += 1;
                if count >= MAX_RESULTS {
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
