pub mod anthropic_messages;
pub mod gemini_generative;
pub mod ollama_native;
pub mod openai_completions;

use crate::tool_def::ToolDefinition;
use reqwest::StatusCode;
use serde_json::Value;
use uncode_shared::error::UncodeError;

/// 读取 HTTP 错误响应体；`text()` 失败时返回说明性占位串（避免静默空 body）。
pub(crate) async fn read_response_body(response: reqwest::Response) -> String {
    match response.text().await {
        Ok(body) => body,
        Err(e) => format!("<failed to read response body: {e}>"),
    }
}

/// 将状态码与 body 映射为 [`UncodeError`]（各 provider 共用）。
pub(crate) fn map_http_error(status: StatusCode, body: String) -> UncodeError {
    match status.as_u16() {
        401 | 403 => UncodeError::LlmAuth(body),
        429 => UncodeError::LlmRateLimit(body),
        _ => UncodeError::Llm(format!("HTTP {status}: {body}")),
    }
}

/// 非 2xx 响应：只读取一次 body，再映射错误类型。
pub(crate) async fn http_error_from_response(response: reqwest::Response) -> UncodeError {
    let status = response.status();
    let body = read_response_body(response).await;
    map_http_error(status, body)
}

/// NDJSON 行缓冲：保留末尾可能不完整的 UTF-8，仅对已闭合 `\n` 行做解码。
pub(crate) struct NdjsonLineBuffer {
    bytes: Vec<u8>,
}

impl NdjsonLineBuffer {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn push_chunk(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
    }

    /// 追加 chunk 并返回本批新完成的行（SSE / NDJSON 流共用）。
    pub fn push_chunk_and_drain_lines(&mut self, chunk: &[u8]) -> Vec<String> {
        self.push_chunk(chunk);
        self.drain_complete_lines()
    }

    /// 取出所有完整行（已 trim；空行跳过）。
    pub fn drain_complete_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(pos) = self.bytes.iter().position(|b| *b == b'\n') {
            let mut raw: Vec<u8> = self.bytes.drain(..=pos).collect();
            if raw.last() == Some(&b'\n') {
                raw.pop();
            }
            let line_bytes = raw.as_slice();
            if line_bytes.is_empty() {
                continue;
            }
            let text = match std::str::from_utf8(line_bytes) {
                Ok(s) => s.trim().to_string(),
                Err(_) => String::from_utf8_lossy(line_bytes).trim().to_string(),
            };
            if !text.is_empty() {
                lines.push(text);
            }
        }
        lines
    }

    /// 流结束时若仍有非空尾部字节，返回错误说明（含不完整 UTF-8）。
    pub fn trailing_error_message(&self, provider: &str) -> Option<String> {
        if self.bytes.is_empty() {
            return None;
        }
        match std::str::from_utf8(&self.bytes) {
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(format!("{provider}: incomplete chunk: {t:.100}"))
                }
            }
            Err(_) => Some(format!("{provider}: incomplete UTF-8 at end of stream")),
        }
    }
}

impl Default for NdjsonLineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod ndjson_buffer_tests {
    use super::NdjsonLineBuffer;

    #[test]
    fn drains_utf8_lines_across_chunks() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(br#"{"a":1}"#.as_slice());
        assert!(buf.drain_complete_lines().is_empty());
        buf.push_chunk(b"\n{\"b\":2}\n");
        let lines = buf.drain_complete_lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"a\""));
        assert!(lines[1].contains("\"b\""));
    }

    #[test]
    fn trailing_error_on_incomplete_utf8() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(&[0xff, 0xfe]);
        assert!(buf.trailing_error_message("test").is_some());
    }

    #[test]
    fn skips_blank_lines() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(b"\n  \n{\"x\":1}\n\n");
        let lines = buf.drain_complete_lines();
        assert_eq!(lines, vec!["{\"x\":1}"]);
    }

    #[test]
    fn trailing_valid_json_without_newline_is_incomplete() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(br#"{"done":true}"#);
        assert!(buf.drain_complete_lines().is_empty());
        let msg = buf.trailing_error_message("sse").expect("incomplete");
        assert!(msg.contains("incomplete chunk"));
        assert!(msg.contains("done"));
    }

    #[test]
    fn trailing_whitespace_only_is_ok() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(b"  \n ");
        assert!(buf.drain_complete_lines().is_empty());
        assert!(buf.trailing_error_message("sse").is_none());
    }

    #[test]
    fn push_chunk_and_drain_matches_two_step() {
        let mut manual = NdjsonLineBuffer::new();
        let chunk = b"line1\nline2\n";
        let from_helper = {
            let mut helper = NdjsonLineBuffer::new();
            helper.push_chunk_and_drain_lines(chunk)
        };
        manual.push_chunk(chunk);
        let from_manual = manual.drain_complete_lines();
        assert_eq!(from_helper, from_manual);
        assert_eq!(from_manual, vec!["line1", "line2"]);
    }

    #[test]
    fn preserves_sse_data_prefix_on_line() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(b"data: {\"k\":1}\n");
        let lines = buf.drain_complete_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("data: "));
    }
}

#[cfg(test)]
mod http_error_tests {
    use super::{http_error_from_response, map_http_error, read_response_body};
    use http::Response;
    use reqwest::StatusCode;
    use uncode_shared::error::UncodeError;

    fn mock_response(status: u16, body: impl Into<String>) -> reqwest::Response {
        let http_resp = Response::builder()
            .status(status)
            .body(body.into())
            .expect("mock http response");
        reqwest::Response::from(http_resp)
    }

    #[tokio::test]
    async fn read_response_body_returns_upstream_text() {
        let body = read_response_body(mock_response(502, "upstream failed")).await;
        assert_eq!(body, "upstream failed");
    }

    #[tokio::test]
    async fn http_error_from_response_reads_body_once_for_auth() {
        let err = http_error_from_response(mock_response(401, "invalid key")).await;
        assert!(matches!(
            err,
            UncodeError::LlmAuth(msg) if msg == "invalid key"
        ));
    }

    #[tokio::test]
    async fn http_error_from_response_rate_limit() {
        let err = http_error_from_response(mock_response(429, "quota")).await;
        assert!(matches!(err, UncodeError::LlmRateLimit(msg) if msg == "quota"));
    }

    #[test]
    fn map_http_error_auth_and_rate_limit() {
        assert!(matches!(
            map_http_error(StatusCode::UNAUTHORIZED, "bad key".into()),
            UncodeError::LlmAuth(_)
        ));
        assert!(matches!(
            map_http_error(StatusCode::TOO_MANY_REQUESTS, "slow down".into()),
            UncodeError::LlmRateLimit(_)
        ));
    }

    #[test]
    fn map_http_error_generic_includes_status() {
        let err = map_http_error(StatusCode::BAD_GATEWAY, "upstream".into());
        match err {
            UncodeError::Llm(msg) => {
                assert!(msg.contains("502"));
                assert!(msg.contains("upstream"));
            }
            other => panic!("expected Llm, got {other:?}"),
        }
    }
}

/// Build OpenAI-compatible tool definitions JSON.
/// Shared by openai_completions and ollama_native providers.
pub(crate) fn build_tools_json(tools: &[ToolDefinition]) -> Option<Value> {
    if tools.is_empty() {
        return None;
    }
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect();
    Some(Value::Array(tools_json))
}

#[cfg(test)]
mod tool_schema_tests {
    use super::*;
    use crate::tool_def::ExecutionMode;

    /// Build a ToolDefinition that mirrors what the ontology produces via to_json_schema().
    fn onto_style_tool(
        name: &str,
        properties: serde_json::Value,
        required: serde_json::Value,
    ) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: format!("{name} tool").into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": properties,
                "required": required,
            }),
            label: None,
            execution_mode: ExecutionMode::Parallel,
        }
    }

    #[test]
    fn build_tools_json_from_ontology_style_schemas() {
        let tools = vec![
            onto_style_tool(
                "read",
                serde_json::json!({
                    "path": {"type": "string", "description": "File path"},
                    "offset": {"type": "integer", "description": "Start line"}
                }),
                serde_json::json!(["path"]),
            ),
            onto_style_tool(
                "bash",
                serde_json::json!({
                    "command": {"type": "string", "description": "Shell command"},
                    "workdir": {"type": "string", "description": "Working dir"}
                }),
                serde_json::json!(["command"]),
            ),
        ];

        let result = build_tools_json(&tools).expect("should produce tools JSON");
        let arr = result.as_array().expect("should be array");

        assert_eq!(arr.len(), 2, "two tools");

        // OpenAI function-calling format
        for tool in arr {
            assert_eq!(tool["type"], "function", "OpenAI requires type=function");
            let func = &tool["function"];
            assert!(
                !func["name"].as_str().unwrap_or("").is_empty(),
                "must have name"
            );
            assert_eq!(
                func["parameters"]["type"], "object",
                "parameters must be object type"
            );
            assert_eq!(func["parameters"]["additionalProperties"], false);
            assert!(
                func["parameters"]["properties"].is_object(),
                "must have properties"
            );
        }

        // Verify read tool has path as required
        let read = &arr[0]["function"];
        let req: Vec<_> = read["parameters"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(req.contains(&"path"), "read should require path");

        // Verify bash tool has command as required
        let bash = &arr[1]["function"];
        let req: Vec<_> = bash["parameters"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(req.contains(&"command"), "bash should require command");
    }

    #[test]
    fn build_tools_json_empty_tools_is_none() {
        assert!(build_tools_json(&[]).is_none());
    }

    #[test]
    fn build_tools_json_single_tool_no_required_fields() {
        let tools = vec![onto_style_tool(
            "ls",
            serde_json::json!({"path": {"type": "string"}}),
            serde_json::json!([]),
        )];
        let result = build_tools_json(&tools).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        // No required fields is valid
        assert!(
            arr[0]["function"]["parameters"]["required"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn build_tools_json_sequential_mode_not_in_schema() {
        // ExecutionMode affects AgentLoop routing, NOT the JSON schema sent to LLM
        // Verify Sequential tools produce valid schema just like Parallel ones
        let tool = ToolDefinition {
            name: "bash".into(),
            description: "run command".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
            label: None,
            execution_mode: ExecutionMode::Sequential,
        };
        let result = build_tools_json(&[tool]).unwrap();
        let func = &result.as_array().unwrap()[0]["function"];
        assert_eq!(func["name"], "bash");
        assert!(
            func["parameters"]["properties"]["command"]["type"]
                .as_str()
                .unwrap()
                == "string"
        );
    }
}
