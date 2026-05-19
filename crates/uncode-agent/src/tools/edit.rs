use async_trait::async_trait;
use serde::Deserialize;
use std::fs;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

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
            description: "Edit a file using hashline anchors or string replacement.\n\
                Hashline mode: Provide 'path' and 'edits' array. Each edit has:\n\
                - op: 'replace' | 'prepend' | 'append'\n\
                - pos: line anchor like '5#KJ' (from read with hashline=true)\n\
                - end: optional end anchor for range replace\n\
                - lines: content to insert\n\
                Legacy mode: Provide 'path', 'old_string', 'new_string' for exact string replacement.\n\
                Returns unified diff of changes."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件路径（相对或绝对）"},
                    "edits": {
                        "type": "array",
                        "description": "Array of hashline edit operations",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": {"type": "string", "enum": ["replace", "prepend", "append"]},
                                "pos": {"type": "string", "description": "Start anchor (e.g. '5#KJ')"},
                                "end": {"type": "string", "description": "End anchor for range (optional)"},
                                "lines": {"type": "string", "description": "Content to insert"}
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

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let raw = arguments["path"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("path required".into()))?;

        let resolved = super::resolve_path(raw).map_err(uncode_core::error::UncodeError::Tool)?;
        let display = resolved.display().to_string();

        let old_content = fs::read_to_string(&resolved)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}")))?;

        let new_content = if let Some(edits_val) = arguments.get("edits") {
            apply_hashline_edits(&old_content, edits_val)?
        } else {
            apply_legacy_edit(&old_content, &arguments)?
        };

        if old_content == new_content {
            return Ok(format!("no changes: {display}"));
        }

        // Atomic write
        let tmp_path = resolved.with_extension("tmp");
        fs::write(&tmp_path, &new_content)
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}")))?;
        fs::rename(&tmp_path, &resolved).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            uncode_core::error::UncodeError::Tool(format!("edit {display}: {e}"))
        })?;

        Ok(unified_diff(&old_content, &new_content, &display))
    }
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

        let new_content_lines: Vec<&str> = edit.new_lines.lines().collect();

        match edit.op.as_str() {
            "replace" => {
                result_lines.drain(start_idx..=end_idx);
                for (i, new_line) in new_content_lines.iter().enumerate() {
                    result_lines.insert(start_idx + i, new_line.to_string());
                }
            }
            "prepend" => {
                for (i, new_line) in new_content_lines.iter().enumerate() {
                    result_lines.insert(start_idx + i, new_line.to_string());
                }
            }
            "append" => {
                let insert_at = end_idx + 1;
                for (i, new_line) in new_content_lines.iter().enumerate() {
                    result_lines.insert(insert_at + i, new_line.to_string());
                }
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
        return Err(UncodeError::Tool(
            "old_string not found in file".to_string(),
        ));
    }
    if count > 1 {
        return Err(UncodeError::Tool(format!(
            "old_string found {count} times, must be unique"
        )));
    }

    Ok(old_content.replacen(old_string, new_string, 1))
}
