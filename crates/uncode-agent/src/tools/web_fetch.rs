use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

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
        let url = arguments["url"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("url required".into()))?;

        super::url_safety::ensure_public_http_url(url)
            .map_err(uncode_core::error::UncodeError::Tool)?;

        let max_length = arguments["max_length"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_LENGTH as u64) as usize;

        let response =
            self.client.get(url).send().await.map_err(|e| {
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

        let bytes = response
            .bytes()
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("read body: {e}")))?;

        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "response too large: {} bytes (max {})",
                bytes.len(),
                MAX_RESPONSE_BYTES
            )));
        }

        let text = if content_type.contains("text/html") {
            html_body_to_text(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        let truncated = if text.len() > max_length {
            let mut end = max_length;
            // Try to break at a newline
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

        Ok(truncated)
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
