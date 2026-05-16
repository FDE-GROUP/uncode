use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

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
        tracing::debug!("input messages: {}", request.messages.len());
        for (i, msg) in request.messages.iter().enumerate() {
            tracing::debug!("msg[{i}]: role={:?} blocks={}", msg.role, msg.content.len());
        }
        let messages = crate::providers::common::build_chat_messages(request);
        tracing::debug!(
            "API messages: {}",
            serde_json::to_string_pretty(&messages).unwrap_or_default()
        );
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
            let tools: Vec<serde_json::Value> = request
                .tools
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
            body["tools"] = serde_json::json!(tools);
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
            return Err(crate::providers::common::map_http_error(
                response.status(),
                response.text().await.unwrap_or_default(),
            ));
        }

        // Use scan to carry accumulated tool-call state across chunks.
        // flat_map would lose state between chunks, causing ToolCallEnd to never fire.
        let state = DeepSeekStreamState {
            pending_args: HashMap::new(),
            tool_names: HashMap::new(),
            index_to_id: HashMap::new(),
        };
        let stream = response
            .bytes_stream()
            .scan(state, |state, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_deepseek_sse_chunk(&String::from_utf8_lossy(&c), state),
                    Err(e) => vec![StreamEvent::Error(e.to_string())],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async { StreamEvent::Done }));

        Ok(Box::pin(stream))
    }
}

/// Mutable state carried across SSE chunks for proper tool-call assembly.
struct DeepSeekStreamState {
    /// Accumulated arguments per tool call ID.
    pending_args: HashMap<String, String>,
    /// Tool name recorded at ToolCallStart, keyed by tool call ID.
    tool_names: HashMap<String, String>,
    /// Maps SSE tool_call index → tool call ID (OpenAI-compat APIs only
    /// include `id` in the first chunk; subsequent arg-only chunks omit it).
    index_to_id: HashMap<usize, String>,
}

/// Parse a single SSE chunk. `pending_args` and `tool_names` accumulate
/// across chunks so that `ToolCallEnd` can be emitted when `finish_reason`
/// arrives — even if it's in a different chunk than the tool-call start/args.
fn parse_deepseek_sse_chunk(text: &str, state: &mut DeepSeekStreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(choice) = event["choices"][0].as_object() {
                    // Tool calls (streaming fragments)
                    if let Some(tool_calls) = choice["delta"]["tool_calls"].as_array() {
                        for tc in tool_calls {
                            let index = tc["index"].as_u64().unwrap_or(0) as usize;
                            let raw_id = tc["id"].as_str().unwrap_or("");
                            // Resolve the actual tool call ID: prefer the id
                            // field, fall back to index-based lookup for
                            // arg-only chunks that omit the id.
                            let id = if !raw_id.is_empty() {
                                state.index_to_id.insert(index, raw_id.to_string());
                                raw_id.to_string()
                            } else {
                                state.index_to_id.get(&index).cloned().unwrap_or_default()
                            };

                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func["name"].as_str() {
                                    state.tool_names.insert(id.clone(), name.to_string());
                                    events.push(StreamEvent::ToolCallStart {
                                        id: id.clone(),
                                        name: name.to_string(),
                                    });
                                }
                                if let Some(args) = func["arguments"].as_str() {
                                    events.push(StreamEvent::ToolCallDelta {
                                        id: id.clone(),
                                        arguments: args.to_string(),
                                    });
                                    state.pending_args.entry(id).or_default().push_str(args);
                                }
                            }
                        }
                    }
                    // Reasoning/thinking content (DeepSeek reasoning models)
                    if let Some(reasoning) = choice["delta"]["reasoning_content"].as_str() {
                        if !reasoning.is_empty() {
                            events.push(StreamEvent::ThinkingDelta(reasoning.to_string()));
                        }
                    }
                    // Text delta
                    if let Some(content) = choice["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            events.push(StreamEvent::TextDelta(content.to_string()));
                        }
                    }
                    // Finish reason — flush accumulated args as ToolCallEnd
                    if let Some(reason) = choice.get("finish_reason") {
                        if !reason.is_null() {
                            for (id, args) in state.pending_args.drain() {
                                let parsed = match serde_json::from_str::<serde_json::Value>(&args)
                                {
                                    Ok(v) => v,
                                    Err(e) => {
                                        events.push(StreamEvent::Error(format!(
                                            "tool args JSON parse failed: {e}"
                                        )));
                                        serde_json::Value::Object(Default::default())
                                    }
                                };
                                let name = state.tool_names.remove(&id).unwrap_or_default();
                                events.push(StreamEvent::ToolCallEnd {
                                    id,
                                    name,
                                    arguments: parsed,
                                });
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
            }
        }
    }
    events
}
