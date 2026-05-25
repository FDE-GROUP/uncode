use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolContext, ToolDefinition, ToolExecutor, ToolResult};

use super::diff::unified_diff;
use super::hashline::{parse_anchor, validate_anchors};

#[derive(Default)]
pub struct EditTool;

#[derive(Debug, Deserialize)]
struct HashlineEdit {
    op: String,
    pos: String,
    end: Option<String>,
    lines: String,
}

struct ParsedEdit {
    op: String,
    start_line: usize,
    end_line: Option<usize>,
    new_lines: String,
}

#[async_trait]
impl ToolExecutor for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".into(),
            description: "编辑文件：hashline 锚点模式或 legacy 字符串替换；返回 unified diff。\n\
                hashline 模式：提供 path 与 edits 数组，每项含 op（replace/prepend/append）、\
                pos（如 5#KJ，来自 read 且 hashline=true）、可选 end（区间替换）、lines（插入内容）。\n\
                legacy 模式：提供 path、old_string、new_string 做精确替换。"
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "edits": {
                        "type": "array",
                        "description": "hashline 编辑操作列表",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "op": {"type": "string", "enum": ["replace", "prepend", "append"]},
                                "pos": {"type": "string", "description": "起始锚点（如 5#KJ）"},
                                "end": {"type": "string", "description": "结束锚点（区间替换时可选）"},
                                "lines": {"type": "string", "description": "插入或替换的文本"}
                            },
                            "required": ["op", "pos", "lines"]
                        }
                    },
                    "old_string": {"type": "string", "description": "要替换的原字符串 (legacy mode)"},
                    "new_string": {"type": "string", "description": "替换后的新字符串 (legacy mode)"}
                },
                "required": ["path"]
            }),
            label: Some("Edit File".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    fn prepare_arguments(
        &self,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, uncode_core::error::UncodeError> {
        super::prepare_arguments_path(arguments, "path", None, &[])
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

        let resolved = super::resolve_path(raw, &ctx.allowed_paths)
            .map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let env = super::ctx_execution_env(&ctx);
        let old_content =
            env.fs().read_text_file(&resolved).await.map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}"))
            })?;

        let job = EditJob {
            path: resolved,
            display,
            old_content,
            arguments,
        };

        let output = tokio::task::spawn_blocking(move || edit_blocking(job))
            .await
            .map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("edit task failed: {e}"))
            })??;

        Ok(ToolResult::ok(output))
    }
}

struct EditJob {
    path: PathBuf,
    display: String,
    old_content: String,
    arguments: serde_json::Value,
}

fn edit_blocking(job: EditJob) -> Result<String, uncode_core::error::UncodeError> {
    let EditJob {
        path,
        display,
        old_content,
        arguments,
    } = job;

    let new_content = if let Some(edits_val) = arguments.get("edits") {
        apply_hashline_edits(&old_content, edits_val)?
    } else {
        apply_legacy_edit(&old_content, &arguments)?
    };

    if old_content == new_content {
        return Ok(format!("no changes: {display}"));
    }

    super::atomic_write(&path, &new_content)
        .map_err(|e| uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}")))?;

    Ok(unified_diff(&old_content, &new_content, &display))
}

fn apply_hashline_edits(
    old_content: &str,
    edits_val: &serde_json::Value,
) -> Result<String, uncode_core::error::UncodeError> {
    use uncode_core::error::UncodeError;

    let edits: Vec<HashlineEdit> = serde_json::from_value(edits_val.clone())
        .map_err(|e| UncodeError::Tool(format!("invalid edits array: {e}")))?;

    if edits.is_empty() {
        return Err(UncodeError::Tool("edits array is empty".into()));
    }

    let lines: Vec<&str> = old_content.lines().collect();

    // Phase 1: Parse and validate all anchors
    let mut parsed_edits: Vec<ParsedEdit> = Vec::with_capacity(edits.len());
    for edit in &edits {
        let pos = parse_anchor(&edit.pos)
            .ok_or_else(|| UncodeError::Tool(format!("invalid pos anchor: {}", edit.pos)))?;

        validate_anchors(old_content, &[(pos.line, &pos.hash)]).map_err(UncodeError::Tool)?;

        let end = if let Some(end_str) = &edit.end {
            let e = parse_anchor(end_str)
                .ok_or_else(|| UncodeError::Tool(format!("invalid end anchor: {}", end_str)))?;
            validate_anchors(old_content, &[(e.line, &e.hash)]).map_err(UncodeError::Tool)?;
            Some(e.line)
        } else {
            None
        };

        if end.is_some_and(|e| e < pos.line) {
            return Err(UncodeError::Tool(format!(
                "end anchor line {} is before pos line {}",
                end.unwrap(),
                pos.line
            )));
        }

        parsed_edits.push(ParsedEdit {
            op: edit.op.clone(),
            start_line: pos.line,
            end_line: end,
            new_lines: edit.lines.clone(),
        });
    }

    // Phase 2: Check for overlapping edits
    let mut sorted_for_check: Vec<&ParsedEdit> = parsed_edits.iter().collect();
    sorted_for_check.sort_by_key(|e| e.start_line);
    for window in sorted_for_check.windows(2) {
        let a = &window[0];
        let b = &window[1];
        let a_end = a.end_line.unwrap_or(a.start_line);
        if b.start_line <= a_end {
            return Err(UncodeError::Tool(format!(
                "overlapping edits: edit at line {} overlaps with edit at line {}",
                a.start_line, b.start_line
            )));
        }
    }

    // Phase 3: Apply edits bottom-up (highest line first)
    parsed_edits.sort_by_key(|b| std::cmp::Reverse(b.start_line));

    let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    for edit in &parsed_edits {
        let start_idx = edit.start_line - 1; // 0-based
        let end_idx = edit.end_line.map(|l| l - 1).unwrap_or(start_idx);

        let new_content_lines: Vec<String> = edit.new_lines.lines().map(str::to_string).collect();

        match edit.op.as_str() {
            "replace" => {
                result_lines.splice(start_idx..=end_idx, new_content_lines);
            }
            "prepend" => {
                result_lines.splice(start_idx..start_idx, new_content_lines);
            }
            "append" => {
                let insert_at = end_idx + 1;
                result_lines.splice(insert_at..insert_at, new_content_lines);
            }
            other => {
                return Err(UncodeError::Tool(format!("unknown op: {other}")));
            }
        }
    }

    let mut result = result_lines.join("\n");
    if old_content.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

fn apply_legacy_edit(
    old_content: &str,
    arguments: &serde_json::Value,
) -> Result<String, uncode_core::error::UncodeError> {
    use uncode_core::error::UncodeError;

    let old_string = arguments["old_string"]
        .as_str()
        .ok_or_else(|| UncodeError::Tool("old_string required (legacy mode)".into()))?;
    let new_string = arguments["new_string"]
        .as_str()
        .ok_or_else(|| UncodeError::Tool("new_string required (legacy mode)".into()))?;

    let count = old_content.matches(old_string).count();
    if count == 0 {
        return Err(UncodeError::Tool("old_string not found in file".to_owned()));
    }
    if count > 1 {
        return Err(UncodeError::Tool(format!(
            "old_string found {count} times, must be unique"
        )));
    }

    Ok(old_content.replacen(old_string, new_string, 1))
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::tools::hashline::compute_line_hash;

    fn anchor(line_no: usize, line_text: &str) -> String {
        let h = compute_line_hash(line_text);
        format!("{line_no}#{}", String::from_utf8_lossy(&h))
    }

    #[test]
    fn legacy_replace_unique_match() {
        let content = "foo bar\n";
        let args = serde_json::json!({
            "old_string": "bar",
            "new_string": "baz"
        });
        let out = apply_legacy_edit(content, &args).unwrap();
        assert_eq!(out, "foo baz\n");
    }

    #[test]
    fn legacy_replace_ambiguous_returns_error() {
        let content = "x\nx\n";
        let args = serde_json::json!({
            "old_string": "x",
            "new_string": "y"
        });
        assert!(apply_legacy_edit(content, &args).is_err());
    }

    #[test]
    fn hashline_replace_via_splice() {
        let content = "alpha\nbeta\ngamma\n";
        let pos = anchor(2, "beta");
        let edits = serde_json::json!([{
            "op": "replace",
            "pos": pos,
            "lines": "BETA"
        }]);
        let out = apply_hashline_edits(content, &edits).unwrap();
        assert_eq!(out, "alpha\nBETA\ngamma\n");
    }

    #[test]
    fn hashline_append_inserts_after_range() {
        let content = "one\ntwo\n";
        let pos = anchor(1, "one");
        let edits = serde_json::json!([{
            "op": "append",
            "pos": pos,
            "lines": "extra"
        }]);
        let out = apply_hashline_edits(content, &edits).unwrap();
        assert_eq!(out, "one\nextra\ntwo\n");
    }

    #[test]
    fn hashline_prepend_inserts_before_line() {
        let content = "one\ntwo\n";
        let pos = anchor(2, "two");
        let edits = serde_json::json!([{
            "op": "prepend",
            "pos": pos,
            "lines": "mid"
        }]);
        let out = apply_hashline_edits(content, &edits).unwrap();
        assert_eq!(out, "one\nmid\ntwo\n");
    }
}
