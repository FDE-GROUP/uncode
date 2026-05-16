use async_trait::async_trait;
use regex::Regex;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

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
                    "include": {"type": "string", "description": "文件匹配模式，如 *.rs"}
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?;

        let re = Regex::new(pattern)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("invalid regex: {e}")))?;

        let search_path = crate::resolve_path(arguments["path"].as_str().unwrap_or("."));
        let include = arguments["include"].as_str();

        let mut results = Vec::new();
        let mut count = 0;
        let max_results = 50;

        for entry in walkdir::WalkDir::new(&search_path)
            .max_depth(20)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if count >= max_results {
                results.push("... (truncated)".into());
                break;
            }

            if let Some(inc) = include {
                if let Some(ext) = entry.path().extension() {
                    let Some(pattern) = inc.strip_prefix("*.") else {
                        continue;
                    };
                    if ext != pattern {
                        continue;
                    }
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
                    if count >= max_results {
                        break;
                    }
                }
            }
        }

        if results.is_empty() {
            Ok("no matches".into())
        } else {
            Ok(results.join("\n"))
        }
    }
}
