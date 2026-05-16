use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};
use crate::providers::common::{build_chat_messages, build_tools_json};
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
        let messages = build_chat_messages(request);
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true,
        });

        if let Some(t) = request.temperature {
            body["options"] = serde_json::json!({"temperature": t});
        }
        if let Some(tools) = build_tools_json(&request.tools) {
            body["tools"] = tools;
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

        let state = OllamaToolState::new();
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_ollama_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}

struct OllamaToolState {
    /// tool call index → (id, name)
    active_tools: std::collections::HashMap<usize, (String, String)>,
}

impl OllamaToolState {
    fn new() -> Self {
        Self {
            active_tools: std::collections::HashMap::new(),
        }
    }
}

fn parse_ollama_chunk(text: &str, state: &mut OllamaToolState) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            // Text content
            if let Some(content) = event["message"]["content"].as_str() {
                if !content.is_empty() {
                    events.push(StreamEvent::TextDelta(content.to_string()));
                }
            }

            // Tool calls — Ollama sends them as complete objects per line
            if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                for (i, tc) in tool_calls.iter().enumerate() {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    if let Some(func) = tc.get("function") {
                        let name = func["name"].as_str().unwrap_or_default().to_string();
                        let args = func["arguments"].clone();

                        // Emit Start + Delta + End for complete tool calls
                        if !name.is_empty() {
                            state.active_tools.insert(i, (id.clone(), name.clone()));
                            events.push(StreamEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            });
                            events.push(StreamEvent::ToolCallDelta {
                                id: id.clone(),
                                arguments: args.to_string(),
                            });
                            events.push(StreamEvent::ToolCallEnd {
                                id,
                                name,
                                arguments: args,
                            });
                        }
                    }
                }
            }

            // Done flag — final response may contain complete tool_calls
            if event["done"].as_bool() == Some(true) {
                if let Some(tool_calls) = event["message"]["tool_calls"].as_array() {
                    for (i, tc) in tool_calls.iter().enumerate() {
                        // Only emit if not already emitted in streaming
                        if state.active_tools.contains_key(&i) {
                            continue;
                        }
                        let id = tc["id"].as_str().unwrap_or("").to_string();
                        if let Some(func) = tc.get("function") {
                            let name = func["name"].as_str().unwrap_or_default().to_string();
                            let args = func["arguments"].clone();
                            if !name.is_empty() {
                                events.push(StreamEvent::ToolCallStart {
                                    id: id.clone(),
                                    name: name.clone(),
                                });
                                events.push(StreamEvent::ToolCallDelta {
                                    id: id.clone(),
                                    arguments: args.to_string(),
                                });
                                events.push(StreamEvent::ToolCallEnd {
                                    id,
                                    name,
                                    arguments: args,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    events
}
