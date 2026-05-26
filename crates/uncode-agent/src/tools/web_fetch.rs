use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolContext, ToolDefinition, ToolExecutor, ToolResult};

const DEFAULT_MAX_LENGTH: usize = 50 * 1024; // 50KB
const MAX_RESPONSE_BYTES: usize = 1024 * 1024; // 1MB
const TIMEOUT_SECS: u64 = 30;
const HTML_TEXT_WIDTH: usize = 80;

/// Convert an HTML response body to plain text; fall back to lossy UTF-8 source on failure.
fn html_body_to_text(bytes: &[u8]) -> String {
    html_body_to_text_with_width(bytes, HTML_TEXT_WIDTH)
}

fn html_body_to_text_with_width(bytes: &[u8], width: usize) -> String {
    let html = String::from_utf8_lossy(bytes);
    match html2text::from_read(html.as_bytes(), width) {
        Ok(text) => text,
        Err(_) => html.into_owned(),
    }
}

struct FetchMeta {
    requested_url: String,
    final_url: String,
    content_type: String,
    status: u16,
    body_bytes: usize,
}

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for WebFetchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_fetch".into(),
            description: "获取 URL 内容并转换为纯文本".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "url": {"type": "string", "description": "要获取的 URL (http/https)"},
                    "max_length": {"type": "integer", "description": "返回文本最大字节数 (默认 50KB)"}
                },
                "required": ["url"]
            }),
            label: Some("Web Fetch".into()),
            execution_mode: uncode_core::tool::ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let (text, _) = self.fetch_and_convert(arguments).await?;
        Ok(text)
    }

    async fn execute_with_context(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, uncode_core::error::UncodeError> {
        let (text, meta) = self.fetch_and_convert(arguments).await?;
        Ok(ToolResult::ok(text).with_details(serde_json::json!({
            "requested_url": meta.requested_url,
            "final_url": meta.final_url,
            "content_type": meta.content_type,
            "status": meta.status,
            "body_bytes": meta.body_bytes,
        })))
    }
}

impl WebFetchTool {
    async fn fetch_and_convert(
        &self,
        arguments: serde_json::Value,
    ) -> Result<(String, FetchMeta), uncode_core::error::UncodeError> {
        let requested_url = arguments["url"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("url required".into()))?
            .to_string();

        super::url_safety::ensure_public_http_url(&requested_url)
            .map_err(uncode_core::error::UncodeError::Tool)?;

        let max_length = arguments["max_length"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_LENGTH as u64) as usize;

        let response =
            self.client.get(&requested_url).send().await.map_err(|e| {
                uncode_core::error::UncodeError::Tool(format!("request failed: {e}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "HTTP {status}"
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let final_url = response.url().to_string();

        let bytes = response
            .bytes()
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("read body: {e}")))?;

        let body_bytes = bytes.len();
        if body_bytes > MAX_RESPONSE_BYTES {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "response too large: {body_bytes} bytes (max {MAX_RESPONSE_BYTES})"
            )));
        }

        let text = if content_type.contains("text/html") {
            html_body_to_text(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        let truncated = if text.len() > max_length {
            let mut end = max_length;
            if let Some(pos) = text[..max_length].rfind('\n') {
                end = pos;
            }
            format!(
                "{}\n\n[truncated at {} bytes, total {} bytes]",
                &text[..end],
                end,
                text.len()
            )
        } else {
            text
        };

        Ok((
            truncated,
            FetchMeta {
                requested_url,
                final_url,
                content_type,
                status: status.as_u16(),
                body_bytes,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_definition() {
        let tool = WebFetchTool::new();
        let def = tool.definition();
        assert_eq!(def.name, "web_fetch");
    }

    #[test]
    fn test_reject_non_http() {
        let tool = WebFetchTool::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({"url": "ftp://example.com"})));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("only http/https") || msg.contains("invalid URL"));
    }

    #[test]
    fn test_reject_loopback() {
        crate::tools::url_safety::set_allow_loopback_for_tests(false);
        let tool = WebFetchTool::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({"url": "http://127.0.0.1/"})));
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("blocked"));
    }

    #[test]
    fn test_reject_missing_url() {
        let tool = WebFetchTool::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(tool.execute(serde_json::json!({})));
        assert!(result.is_err());
    }

    #[test]
    fn test_html_body_to_text_extracts_plain_text() {
        let html = b"<html><body><p>Hello</p></body></html>";
        let out = html_body_to_text(html);
        assert!(out.contains("Hello"));
        assert!(!out.contains("<p>"));
    }

    #[test]
    fn test_html_body_to_text_falls_back_to_raw_html() {
        let html = b"<html><body><p>Hello</p></body></html>";
        // width 0 triggers html2text::Error::TooNarrow
        let out = html_body_to_text_with_width(html, 0);
        assert!(out.contains("<p>Hello</p>"));
    }

    struct AllowLoopbackGuard;

    impl AllowLoopbackGuard {
        fn set() -> Self {
            crate::tools::url_safety::set_allow_loopback_for_tests(true);
            Self
        }
    }

    impl Drop for AllowLoopbackGuard {
        fn drop(&mut self) {
            crate::tools::url_safety::set_allow_loopback_for_tests(false);
        }
    }

    fn test_tool_context() -> uncode_core::tool::ToolContext {
        uncode_core::tool::ToolContext {
            cancel_token: tokio_util::sync::CancellationToken::new(),
            on_progress: None,
            tool_call_id: "test".into(),
            execution_env: None,
            allowed_paths: Vec::new(),
            subagent_runner: None,
            current_model: None,
        }
    }

    #[tokio::test]
    async fn test_fetch_details_include_final_url_and_content_type() {
        let _guard = AllowLoopbackGuard::set();
        let mock_server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("body")
                    .insert_header("content-type", "text/plain; charset=utf-8"),
            )
            .mount(&mock_server)
            .await;

        let tool = WebFetchTool::new();
        let tr = tool
            .execute_with_context(
                serde_json::json!({ "url": mock_server.uri() }),
                test_tool_context(),
            )
            .await
            .unwrap();
        let details = tr.details.expect("details");
        assert!(
            details["content_type"]
                .as_str()
                .is_some_and(|ct| ct.starts_with("text/plain"))
        );
        let expected = mock_server.uri().trim_end_matches('/').to_string();
        for key in ["final_url", "requested_url"] {
            let u = details[key].as_str().expect(key);
            assert_eq!(u.trim_end_matches('/'), expected);
        }
        assert_eq!(details["status"], 200);
        assert_eq!(details["body_bytes"], 4);
    }

    #[tokio::test]
    async fn test_fetch_plain_text_via_mock_server() {
        let _guard = AllowLoopbackGuard::set();
        let mock_server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("plain body from mock")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&mock_server)
            .await;

        let tool = WebFetchTool::new();
        let out = tool
            .execute(serde_json::json!({ "url": mock_server.uri() }))
            .await
            .unwrap();
        assert!(out.contains("plain body from mock"));
    }

    #[tokio::test]
    async fn test_fetch_html_via_mock_server() {
        let _guard = AllowLoopbackGuard::set();
        let mock_server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("<html><body><p>Hello mock</p></body></html>")
                    .insert_header("content-type", "text/html; charset=utf-8"),
            )
            .mount(&mock_server)
            .await;

        let tool = WebFetchTool::new();
        let out = tool
            .execute(serde_json::json!({ "url": mock_server.uri() }))
            .await
            .unwrap();
        assert!(out.contains("Hello mock"));
    }

    #[tokio::test]
    async fn test_fetch_http_error_from_mock_server() {
        let _guard = AllowLoopbackGuard::set();
        let mock_server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let tool = WebFetchTool::new();
        let err = tool
            .execute(serde_json::json!({ "url": mock_server.uri() }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("503"));
    }
}
