use std::path::PathBuf;

use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

const MAX_FIND_RESULTS: usize = 200;

#[derive(Default)]
pub struct FindTool;

#[async_trait]
impl ToolExecutor for FindTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "find".into(),
            description: "按文件名模式查找文件；默认遵守 .gitignore".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "pattern": {"type": "string", "description": "glob 模式，如 **/*.rs（相对搜索根）"},
                    "path": {"type": "string", "description": "搜索根目录，默认当前目录"}
                },
                "required": ["pattern"]
            }),
            label: Some("Find Files".into()),
            execution_mode: ExecutionMode::default(),
        }
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
        _ctx: ToolContext,
    ) -> UncodeResult<ToolResult> {
        let pattern = arguments["pattern"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("pattern required".into()))?
            .to_string();
        let root_raw = arguments["path"].as_str().unwrap_or(".").to_string();
        let root = super::resolve_path(&root_raw).map_err(uncode_core::error::UncodeError::Tool)?;

        let job = FindJob { root, pattern };

        let output = tokio::task::spawn_blocking(move || find_files(job))
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("find task failed: {e}")))?;

        match output {
            Ok(text) => Ok(ToolResult::ok(text)),
            Err(e) => Err(uncode_core::error::UncodeError::Tool(e)),
        }
    }
}

struct FindJob {
    root: PathBuf,
    pattern: String,
}

fn find_files(job: FindJob) -> Result<String, String> {
    let glob_pat =
        glob::Pattern::new(&job.pattern).map_err(|e| format!("invalid glob pattern: {e}"))?;

    let mut results: Vec<String> = Vec::with_capacity(MAX_FIND_RESULTS);

    let walker = ignore::WalkBuilder::new(&job.root)
        .standard_filters(true)
        .max_depth(Some(20))
        .build();

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if results.len() >= MAX_FIND_RESULTS {
            results.push("... (truncated)".into());
            break;
        }

        let rel = path
            .strip_prefix(&job.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        if !glob_pat.matches(&rel) && !glob_pat.matches(&file_name) {
            continue;
        }

        results.push(path.display().to_string());
    }

    if results.is_empty() {
        Ok("no files found".into())
    } else {
        Ok(results.join("\n"))
    }
}
