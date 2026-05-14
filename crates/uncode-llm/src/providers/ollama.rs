use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};
use uncode_core::error::UncodeError;

pub struct OllamaDriver {
    client: Client,
    host: String,
}

impl OllamaDriver {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            host: "http://localhost:11434".into(),
        }
    }

    pub fn with_host(host: String) -> Self {
        Self {
            client: Client::new(),
            host,
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
            "stream": false,
        });

        if let Some(t) = request.temperature {
            body["options"] = serde_json::json!({"temperature": t});
        }

        body
    }
}

impl Default for OllamaDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmDriver for OllamaDriver {
    fn provider_name(&self) -> &'static str {
        "ollama"
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = self.build_body(&request);

        let response = self
            .client
            .post(format!("{}/api/chat", self.host))
            .json(&body)
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(UncodeError::Llm(text));
        }

        let full: Value = response
            .json()
            .await
            .map_err(|e| UncodeError::Llm(e.to_string()))?;

        let content = full["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let stream = stream::iter(vec![StreamEvent::TextDelta(content), StreamEvent::Done]);

        Ok(Box::pin(stream))
    }
}
