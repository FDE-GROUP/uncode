use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};
use crate::providers::common::map_http_error;
use uncode_core::error::UncodeError;

pub struct GeminiDriver {
    client: Client,
    api_key: String,
}

impl GeminiDriver {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

fn build_body(request: &CompletionRequest) -> Value {
    let mut contents = Vec::new();
    if let Some(ref system) = request.system {
        contents.push(
            serde_json::json!({"role": "user", "parts": [{"text": format!("System: {system}")}]}),
        );
        contents.push(serde_json::json!({"role": "model", "parts": [{"text": "Understood."}]}));
    }
    for msg in &request.messages {
        let role = match msg.role {
            uncode_core::message::Role::Assistant => "model",
            _ => "user",
        };
        let text = msg
            .content
            .iter()
            .filter_map(|b| match b {
                uncode_core::message::ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        contents.push(serde_json::json!({"role": role, "parts": [{"text": text}]}));
    }
    serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": request.max_tokens.unwrap_or(8192),
            "temperature": request.temperature.unwrap_or(0.7),
        }
    })
}

#[async_trait]
impl LlmDriver for GeminiDriver {
    fn provider_name(&self) -> &'static str {
        "gemini"
    }
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let model = request.model.clone();
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"
        );
        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&build_body(&request))
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;
        if !response.status().is_success() {
            return Err(map_http_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }
        let stream = response
            .bytes_stream()
            .flat_map(|chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let text = String::from_utf8_lossy(&c);
                        let mut v = Vec::new();
                        for line in text.lines() {
                            if let Some(json) = line.strip_prefix("data: ") {
                                if let Ok(event) = serde_json::from_str::<Value>(json) {
                                    if let Some(text) = event["candidates"][0]["content"]["parts"]
                                        [0]["text"]
                                        .as_str()
                                    {
                                        v.push(StreamEvent::TextDelta(text.to_string()));
                                    }
                                }
                            }
                        }
                        v
                    }
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                stream::iter(events)
            })
            .chain(stream::once(async { StreamEvent::Done }));
        Ok(Box::pin(stream))
    }
}
