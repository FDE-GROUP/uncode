use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use reqwest::Client;
use serde_json::Value;

use crate::api::{Api, StreamEvent, ToolCallEndData};
use crate::api_types::{Context, StopReason, StreamOptions};
use crate::message::{ContentBlock, Role};
use crate::model::Model;
use uncode_shared::error::UncodeError;

pub struct GeminiGenerativeAiApi {
    client: Client,
}

impl GeminiGenerativeAiApi {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for GeminiGenerativeAiApi {
    fn default() -> Self {
        Self::new()
    }
}

fn build_gemini_body(_model: &Model, context: &Context, options: &StreamOptions) -> Value {
    let mut contents = Vec::new();

    if let Some(ref system) = context.system_prompt {
        contents.push(
            serde_json::json!({"role": "user", "parts": [{"text": format!("System: {system}")}]}),
        );
        contents.push(serde_json::json!({"role": "model", "parts": [{"text": "Understood."}]}));
    }

    for msg in &context.messages {
        let role = match msg.role {
            Role::Assistant => "model",
            _ => "user",
        };
        let text = msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        contents.push(serde_json::json!({"role": role, "parts": [{"text": text}]}));
    }

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": options.max_tokens.unwrap_or(8192),
        }
    });

    if let Some(t) = options.temperature {
        body["generationConfig"]["temperature"] = serde_json::json!(t);
    }

    if !context.tools.is_empty() {
        let tools: Vec<Value> = context
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "functionDeclarations": [{
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }]
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    body
}

pub(crate) fn map_gemini_finish_reason(reason: &str) -> StopReason {
    match reason {
        "STOP" | "FINISH_REASON_STOP" => StopReason::Stop,
        "MAX_TOKENS" | "FINISH_REASON_MAX_TOKENS" => StopReason::Length,
        "SAFETY" | "FINISH_REASON_SAFETY" | "RECITATION" | "FINISH_REASON_RECITATION" => {
            StopReason::Error
        }
        _ => StopReason::Stop,
    }
}

fn parse_gemini_chunk(text: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        if let Some(json) = line.trim().strip_prefix("data: ")
            && let Ok(event) = serde_json::from_str::<Value>(json)
        {
            if let Some(parts) = event["candidates"][0]["content"]["parts"].as_array() {
                for part in parts {
                    if let Some(text) = part["text"].as_str() {
                        events.push(StreamEvent::TextDelta(text.to_string()));
                    }
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc["name"].as_str().unwrap_or_default().to_string();
                        let id = format!("gemini_{}", &name);
                        let args = fc["args"].clone();
                        events.push(StreamEvent::ToolCallStart {
                            id: id.clone(),
                            name: name.clone(),
                        });
                        events.push(StreamEvent::ToolCallDelta {
                            id: id.clone(),
                            arguments: args.to_string(),
                        });
                        events.push(StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
                            id,
                            name,
                            arguments: args,
                        })));
                    }
                }
            }
            if let Some(reason) = event["candidates"][0]["finishReason"].as_str() {
                events.push(StreamEvent::Done {
                    reason: map_gemini_finish_reason(reason),
                });
            }
        }
    }
    events
}

#[async_trait]
impl Api for GeminiGenerativeAiApi {
    fn api_name(&self) -> &'static str {
        "google-generative-ai"
    }

    async fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> Result<BoxStream<'static, StreamEvent>, UncodeError> {
        let body = build_gemini_body(model, context, options);
        crate::notify_request_payload(options, &body);
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            model.base_url, model.id
        );

        let mut req = self.client.post(&url).json(&body);
        req = crate::apply_option_headers(req, options);

        if let Some(ref key) = options.api_key {
            req = req.header("x-goog-api-key", key);
        }

        for (k, v) in &model.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let send_future = req.send();
        let response = match options.timeout_ms {
            Some(ms) => tokio::time::timeout(std::time::Duration::from_millis(ms), send_future)
                .await
                .map_err(|_| UncodeError::Llm("request timed out".into()))?
                .map_err(|e| UncodeError::Network(e.to_string()))?,
            None => send_future
                .await
                .map_err(|e| UncodeError::Network(e.to_string()))?,
        };

        crate::notify_http_response(options, response.status().as_u16(), response.headers());

        if !response.status().is_success() {
            let status = response.status();
            return Err(match status.as_u16() {
                401 | 403 => UncodeError::LlmAuth(response.text().await.unwrap_or_default()),
                429 => UncodeError::LlmRateLimit(response.text().await.unwrap_or_default()),
                _ => UncodeError::Llm(format!(
                    "HTTP {status}: {}",
                    response.text().await.unwrap_or_default()
                )),
            });
        }

        let stream = response
            .bytes_stream()
            .flat_map(|chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => parse_gemini_chunk(&String::from_utf8_lossy(&c)),
                    Err(e) => vec![StreamEvent::Error {
                        reason: crate::api_types::StopReason::Error,
                        message: e.to_string(),
                    }],
                };
                stream::iter(events)
            })
            .chain(stream::once(async {
                StreamEvent::Done {
                    reason: StopReason::Stop,
                }
            }));

        Ok(Box::pin(stream))
    }
}
