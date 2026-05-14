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
        let messages = crate::providers::common::build_chat_messages(request);
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
