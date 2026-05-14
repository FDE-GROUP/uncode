use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent, UsageInfo};
use uncode_core::error::UncodeError;

pub struct DeepSeekDriver {
    client: Client,
    api_key: String,
    base_url: String,
}

impl DeepSeekDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.deepseek.com/v1".into(),
        }
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
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

        if !request.tools.is_empty() {
            body["tools"] = serde_json::to_value(&request.tools).unwrap_or_default();
        }

        body
    }
}

#[async_trait]
impl LlmDriver for DeepSeekDriver {
    fn provider_name(&self) -> &'static str {
        "deepseek"
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = self.build_body(&request);

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
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
            .map(|chunk| {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => return StreamEvent::Error(e.to_string()),
                };
                let text = String::from_utf8_lossy(&chunk);

                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line == "data: [DONE]" {
                        continue;
                    }
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if let Ok(event) = serde_json::from_str::<Value>(json_str) {
                            if let Some(choice) = event["choices"][0].as_object() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta["content"].as_str() {
                                        if !content.is_empty() {
                                            return StreamEvent::TextDelta(content.to_string());
                                        }
                                    }
                                }
                                if let Some(finish_reason) = choice.get("finish_reason") {
                                    if finish_reason.as_str() == Some("stop") {
                                        if let Some(usage) = event.get("usage") {
                                            return StreamEvent::Usage(UsageInfo {
                                                input_tokens: usage["prompt_tokens"]
                                                    .as_u64()
                                                    .unwrap_or(0),
                                                output_tokens: usage["completion_tokens"]
                                                    .as_u64()
                                                    .unwrap_or(0),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                StreamEvent::TextDelta(String::new())
            })
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}
