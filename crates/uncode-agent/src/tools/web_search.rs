use async_trait::async_trait;
use uncode_core::error::UncodeResult;
use uncode_core::tool::{ToolDefinition, ToolExecutor};

use super::local_env::truncate_output;

const TAVILY_API_URL: &str = "https://api.tavily.com/search";
const DEFAULT_MAX_RESULTS: usize = 5;
const TIMEOUT_SECS: u64 = 30;
/// Cap formatted search output (aligned with bash / web_fetch).
const MAX_OUTPUT_BYTES: usize = 50 * 1024;

pub struct WebSearchTool {
    api_key: String,
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self { api_key, client }
    }

    pub fn try_new(api_key: &str) -> Option<Self> {
        if api_key.trim().is_empty() {
            None
        } else {
            Some(Self::new(api_key.to_string()))
        }
    }
}

#[derive(serde::Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(serde::Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    #[allow(dead_code)]
    score: f64,
}

#[async_trait]
impl ToolExecutor for WebSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".into(),
            description: "搜索互联网获取信息".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词"},
                    "max_results": {"type": "integer", "description": "最大结果数 (默认 5)"}
                },
                "required": ["query"]
            }),
            label: Some("Web Search".into()),
            execution_mode: uncode_core::tool::ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> UncodeResult<String> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| uncode_core::error::UncodeError::Tool("query required".into()))?;

        let max_results = arguments["max_results"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_RESULTS as u64) as usize;

        let body = serde_json::json!({
            "api_key": self.api_key,
            "query": query,
            "max_results": max_results,
            "include_answer": true
        });

        let response = self
            .client
            .post(TAVILY_API_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(uncode_core::error::UncodeError::Tool(format!(
                "Tavily API error: HTTP {status} — {text}"
            )));
        }

        let tavily: TavilyResponse = response
            .json()
            .await
            .map_err(|e| uncode_core::error::UncodeError::Tool(format!("parse response: {e}")))?;

        use std::fmt::Write;

        let mut output = String::new();
        let results_len = tavily.results.len();
        let reserve_hint: usize = tavily
            .answer
            .as_ref()
            .map(|a| a.len())
            .unwrap_or(0)
            .saturating_add(results_len.saturating_mul(128));
        output.reserve(reserve_hint);

        if let Some(answer) = tavily.answer.as_deref()
            && !answer.is_empty()
        {
            output.push_str(answer);
            output.push_str("\n\n");
        }

        if results_len == 0 {
            output.push_str("No results found.");
            return Ok(output);
        }

        let _ = write!(output, "Found {results_len} results:\n\n");

        for (i, result) in tavily.results.iter().enumerate() {
            let _ = write!(
                output,
                "{}. {} ({})\n   {}\n\n",
                i + 1,
                result.title,
                result.url,
                result.content.trim()
            );
        }

        Ok(truncate_output(&output, MAX_OUTPUT_BYTES))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_new_empty_key() {
        assert!(WebSearchTool::try_new("").is_none());
        assert!(WebSearchTool::try_new("  ").is_none());
    }

    #[test]
    fn test_try_new_valid_key() {
        let tool = WebSearchTool::try_new("tvly-test-key");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().definition().name, "web_search");
    }

    #[test]
    fn test_definition() {
        let tool = WebSearchTool::new("test-key".into());
        let def = tool.definition();
        assert_eq!(def.name, "web_search");
    }

    #[test]
    fn test_truncate_output_constant() {
        let huge = "x".repeat(MAX_OUTPUT_BYTES + 1000);
        let out = truncate_output(&huge, MAX_OUTPUT_BYTES);
        assert!(out.contains("[truncated]"));
        assert!(out.len() < huge.len());
    }
}
