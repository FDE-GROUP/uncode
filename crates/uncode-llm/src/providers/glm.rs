use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
use uncode_core::error::UncodeError;

pub struct GlmDriver {
    client: Client,
    api_key: String,
}

impl GlmDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn build_body(&self, request: &CompletionRequest) -> Value {
        let mut messages = Vec::new();

        if let Some(ref system) = request.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system
            }));
        }

        for msg in &request.messages {
            let role = match msg.role {
                uncode_core::message::Role::User => "user",
                uncode_core::message::Role::Assistant => "assistant",
                uncode_core::message::Role::System => "system",
                uncode_core::message::Role::Tool => "tool",
            };
            let content = msg
                .content
                .iter()
                .filter_map(|block| match block {
                    uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
                    uncode_core::message::ContentBlock::Thinking { .. } => None,
                    uncode_core::message::ContentBlock::ToolCall(tc) => {
                        Some(format!("[tool_call: {}]", tc.name))
                    }
                    uncode_core::message::ContentBlock::ToolResult(tr) => Some(tr.content.clone()),
                })
                .collect::<Vec<_>>()
                .join("\n");
            messages.push(serde_json::json!({
                "role": role,
                "content": content
            }));
        }

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true,
        });

        if let Some(mt) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(mt);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }

        body
    }
}

#[async_trait]
impl LlmDriver for GlmDriver {
    fn provider_name(&self) -> &'static str {
        "glm"
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = self.build_body(&request);

        let response = self
            .client
            .post("https://open.bigmodel.cn/api/paas/v4/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(UncodeError::LlmAuth(text));
            }
            if status.as_u16() == 429 {
                return Err(UncodeError::LlmRateLimit(text));
            }
            return Err(UncodeError::Llm(format!("HTTP {status}: {text}")));
        }

        let stream = response
            .bytes_stream()
            .flat_map(|chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let text = String::from_utf8_lossy(&c);
                        parse_sse(&text)
                    }
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                stream::iter(events)
            })
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}

fn parse_sse(text: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "data: [DONE]" {
            break;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                if let Some(delta) = &event["choices"][0]["delta"].as_object() {
                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            events.push(StreamEvent::TextDelta(content.to_string()));
                        }
                    }
                }
                if let Some(usage) = event.get("usage") {
                    events.push(StreamEvent::Usage(UsageInfo {
                        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                    }));
                }
            }
        }
    }
    events
}
