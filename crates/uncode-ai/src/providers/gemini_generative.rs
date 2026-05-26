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

fn build_gemini_body(model: &Model, context: &Context, options: &StreamOptions) -> Value {
    let mut contents = Vec::new();

    if let Some(system) = context.system_prompt.as_deref() {
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
            "maxOutputTokens": options.max_tokens.unwrap_or(model.max_output_tokens),
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
        let mut body = build_gemini_body(model, context, options);
        crate::notify_request_payload(options, &mut body);
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
            return Err(crate::providers::http_error_from_response(response).await);
        }

        let line_buf = std::sync::Arc::new(std::sync::Mutex::new(
            crate::providers::NdjsonLineBuffer::new(),
        ));
        let stream = response
            .bytes_stream()
            .scan(line_buf, move |line_buf, chunk| {
                let events: Vec<StreamEvent> = match chunk {
                    Ok(c) => {
                        let mut all_events = Vec::new();
                        let mut guard = line_buf.lock().expect("gemini sse buffer lock");
                        for line in guard.push_chunk_and_drain_lines(&c) {
                            all_events.extend(parse_gemini_chunk(&line));
                        }
                        all_events
                    }
                    Err(e) => vec![StreamEvent::Error {
                        reason: crate::api_types::StopReason::Error,
                        message: e.to_string(),
                    }],
                };
                std::future::ready(Some(stream::iter(events)))
            })
            .flatten()
            .chain(stream::once(async {
                StreamEvent::Done {
                    reason: StopReason::Stop,
                }
            }));

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod stream_buffer_tests {
    use super::parse_gemini_chunk;
    use crate::api::StreamEvent;
    use crate::providers::NdjsonLineBuffer;

    #[test]
    fn ndjson_buffer_lines_drive_gemini_parser() {
        let mut buf = NdjsonLineBuffer::new();
        let chunk = br#"data: {"candidates":[{"content":{"parts":[{"text":"yo"}]}}]}
"#;
        let mut events = Vec::new();
        for line in buf.push_chunk_and_drain_lines(chunk) {
            events.extend(parse_gemini_chunk(&line));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TextDelta(s) if s == "yo"))
        );
    }

    #[test]
    fn buffer_splits_chunk_before_data_prefix() {
        let mut buf = NdjsonLineBuffer::new();
        buf.push_chunk(b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ab");
        assert!(buf.drain_complete_lines().is_empty());
        let lines = buf.push_chunk_and_drain_lines(b"\"}]}}]}\n");
        assert_eq!(lines.len(), 1);
        let events = parse_gemini_chunk(&lines[0]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::TextDelta(s) if s == "ab"))
        );
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;
    use crate::message::Message;
    use crate::model::Model;
    use crate::tool_def::ToolDefinition;

    #[test]
    fn build_body_with_user_message() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.messages = vec![Message::user("hello")];
        let options = StreamOptions::default();
        let body = build_gemini_body(&model, &ctx, &options);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "hello");
    }

    #[test]
    fn build_body_with_system_prompt() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.system_prompt = Some("Be helpful".into());
        let options = StreamOptions::default();
        let body = build_gemini_body(&model, &ctx, &options);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "System: Be helpful");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "Understood.");
    }

    #[test]
    fn build_body_role_mapping() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.messages = vec![Message::user("hi"), Message::assistant("hello")];
        let options = StreamOptions::default();
        let body = build_gemini_body(&model, &ctx, &options);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model");
    }

    #[test]
    fn build_body_with_temperature() {
        let model = Model::default();
        let ctx = Context::default();
        let options = StreamOptions {
            temperature: Some(0.5),
            ..Default::default()
        };
        let body = build_gemini_body(&model, &ctx, &options);
        let t = body["generationConfig"]["temperature"].as_f64().unwrap();
        assert!((t - 0.5).abs() < 0.01);
    }

    #[test]
    fn build_body_max_output_tokens() {
        let model = Model::default();
        let ctx = Context::default();
        let options = StreamOptions {
            max_tokens: Some(1024),
            ..Default::default()
        };
        let body = build_gemini_body(&model, &ctx, &options);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 1024);
    }

    #[test]
    fn build_body_with_tools() {
        let model = Model::default();
        let mut ctx = Context::default();
        ctx.tools = vec![ToolDefinition {
            name: "get_weather".into(),
            description: "Gets weather".into(),
            parameters: serde_json::json!({"type": "object", "properties": {"city": {"type": "string"}}}),
            label: None,
            execution_mode: Default::default(),
        }];
        let options = StreamOptions::default();
        let body = build_gemini_body(&model, &ctx, &options);
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        let fd = &tools[0]["functionDeclarations"][0];
        assert_eq!(fd["name"], "get_weather");
        assert_eq!(fd["description"], "Gets weather");
        assert!(fd["parameters"].is_object());
    }
}
