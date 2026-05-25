use async_trait::async_trait;
use std::sync::Arc;
use uncode_ai::model::Model;
use uncode_core::error::UncodeError;
use uncode_core::tool::{ExecutionMode, ToolDefinition, ToolExecutor};

use crate::decision::bridge::ModelBridge;

pub struct LLMQueryTool {
    models: Arc<Vec<Model>>,
}

impl LLMQueryTool {
    pub fn new(models: Arc<Vec<Model>>) -> Self {
        Self { models }
    }

    fn handle_query(&self, query: &str, arguments: &serde_json::Value) -> Result<String, String> {
        match query {
            "capabilities" | "capability" => self.query_capability(arguments),
            "cheapest" => self.query_cheapest(arguments),
            "list" | "all" => self.query_all(arguments),
            "recommend" => self.query_recommend(arguments),
            _ => Err(format!(
                "unknown query type: '{query}'. Available: capabilities, cheapest, list, recommend"
            )),
        }
    }

    fn query_capability(&self, arguments: &serde_json::Value) -> Result<String, String> {
        let model_id = arguments["model_id"]
            .as_str()
            .ok_or("model_id required for capability query")?;

        let model = self
            .models
            .iter()
            .find(|m| m.id == model_id)
            .ok_or(format!("model not found: {model_id}"))?;

        let fields = ModelBridge::model_to_fields(model);
        let mut lines = vec![format!("Model: {} ({})", model.id, model.provider)];
        lines.push(format!(
            "  context_window: {}",
            fields
                .get("context_window")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        ));
        lines.push(format!(
            "  max_output_tokens: {}",
            fields
                .get("max_output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        ));
        lines.push(format!(
            "  supports_vision: {}",
            fields
                .get("supports_vision")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        ));
        lines.push(format!(
            "  supports_reasoning: {}",
            fields
                .get("supports_reasoning")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        ));
        lines.push(format!(
            "  supports_tools: {}",
            fields
                .get("supports_tools")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        ));
        lines.push(format!(
            "  api_protocol: {}",
            fields
                .get("api_protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        ));
        lines.push(format!(
            "  pricing_input_per_million: ${:.2}",
            fields
                .get("pricing_input_per_million")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        ));
        lines.push(format!(
            "  pricing_output_per_million: ${:.2}",
            fields
                .get("pricing_output_per_million")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        ));
        Ok(lines.join("\n"))
    }

    fn query_cheapest(&self, arguments: &serde_json::Value) -> Result<String, String> {
        let mut candidates: Vec<&Model> = self.models.iter().collect();

        if let Some(min_ctx) = arguments["min_context_window"].as_u64() {
            candidates.retain(|m| m.context_window as u64 >= min_ctx);
        }

        if let Some(true) = arguments["supports_vision"].as_bool() {
            candidates.retain(|m| {
                m.input_modalities
                    .contains(&uncode_ai::api_types::InputModality::Image)
            });
        }

        if let Some(true) = arguments["supports_reasoning"].as_bool() {
            candidates.retain(|m| m.reasoning);
        }

        candidates.sort_by(|a, b| {
            let cost_a = a.pricing.input + a.pricing.output;
            let cost_b = b.pricing.input + b.pricing.output;
            cost_a
                .partial_cmp(&cost_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if candidates.is_empty() {
            return Ok("no models match the criteria".into());
        }

        let max_results = arguments["max_results"].as_u64().unwrap_or(5) as usize;
        let results: Vec<String> = candidates
            .iter()
            .take(max_results)
            .map(|m| {
                let fields = ModelBridge::model_to_fields(m);
                format!(
                    "{} ({}): ctx={}, in=${:.2}/M, out=${:.2}/M",
                    m.id,
                    m.provider,
                    m.context_window,
                    fields
                        .get("pricing_input_per_million")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    fields
                        .get("pricing_output_per_million")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                )
            })
            .collect();

        Ok(format!(
            "Cheapest models ({} found):\n{}",
            candidates.len(),
            results.join("\n")
        ))
    }

    fn query_all(&self, arguments: &serde_json::Value) -> Result<String, String> {
        let mut criteria = std::collections::HashMap::new();

        if let Some(provider) = arguments["provider"].as_str() {
            criteria.insert("provider".into(), serde_json::json!(provider));
        }
        if let Some(protocol) = arguments["api_protocol"].as_str() {
            criteria.insert("api_protocol".into(), serde_json::json!(protocol));
        }

        let results = if criteria.is_empty() {
            self.models.iter().collect()
        } else {
            ModelBridge::query_models(&self.models, &criteria)
        };

        if results.is_empty() {
            return Ok("no models found".into());
        }

        let lines: Vec<String> = results
            .iter()
            .map(|m| format!("{} ({}, {})", m.id, m.provider, m.api))
            .collect();
        Ok(format!("Models ({}):\n{}", results.len(), lines.join("\n")))
    }

    fn query_recommend(&self, arguments: &serde_json::Value) -> Result<String, String> {
        let context_tokens = arguments["context_tokens"]
            .as_u64()
            .unwrap_or(0) as u32;

        let mut candidates: Vec<&Model> = self.models.iter().collect();

        candidates.retain(|m| m.context_window >= context_tokens);

        if let Some(true) = arguments["supports_vision"].as_bool() {
            candidates.retain(|m| {
                m.input_modalities
                    .contains(&uncode_ai::api_types::InputModality::Image)
            });
        }

        if let Some(true) = arguments["supports_reasoning"].as_bool() {
            candidates.retain(|m| m.reasoning);
        }

        candidates.sort_by(|a, b| {
            let cost_a = a.pricing.input + a.pricing.output;
            let cost_b = b.pricing.input + b.pricing.output;
            cost_a
                .partial_cmp(&cost_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if candidates.is_empty() {
            return Ok(format!(
                "no models with context_window >= {context_tokens}"
            ));
        }

        let max_results = arguments["max_results"].as_u64().unwrap_or(3) as usize;
        let results: Vec<String> = candidates
            .iter()
            .take(max_results)
            .map(|m| {
                let mut suffix = String::new();
                if m
                    .input_modalities
                    .contains(&uncode_ai::api_types::InputModality::Image)
                {
                    suffix.push_str(" +vision");
                }
                if m.reasoning {
                    suffix.push_str(" +reasoning");
                }
                format!(
                    "{} ({}): ctx={}{}, in=${:.2}/M, out=${:.2}/M",
                    m.id,
                    m.provider,
                    m.context_window,
                    suffix,
                    m.pricing.input,
                    m.pricing.output,
                )
            })
            .collect();

        Ok(format!(
            "Recommended models:\n{}",
            results.join("\n")
        ))
    }
}

#[async_trait]
impl ToolExecutor for LLMQueryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "llm_query".into(),
            description: "查询 LLM 模型能力、成本和推荐。支持四种查询类型：capabilities（单个模型详情）、cheapest（最便宜模型）、list（列出模型）、recommend（按需求推荐）".into(),
            parameters: serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "enum": ["capabilities", "cheapest", "list", "recommend"],
                        "description": "查询类型"
                    },
                    "model_id": {
                        "type": "string",
                        "description": "模型 ID（capabilities 查询时必填）"
                    },
                    "min_context_window": {
                        "type": "integer",
                        "description": "最小上下文窗口（cheapest/recommend 时可选）"
                    },
                    "context_tokens": {
                        "type": "integer",
                        "description": "当前 context token 数（recommend 时使用）"
                    },
                    "supports_vision": {
                        "type": "boolean",
                        "description": "是否需要 vision 能力"
                    },
                    "supports_reasoning": {
                        "type": "boolean",
                        "description": "是否需要 reasoning 能力"
                    },
                    "provider": {
                        "type": "string",
                        "description": "按供应商过滤（list 查询时可选）"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "最大返回数量（默认 5）"
                    }
                },
                "required": ["query"]
            }),
            label: Some("LLM Query".into()),
            execution_mode: ExecutionMode::default(),
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<String, UncodeError> {
        let query = arguments["query"]
            .as_str()
            .ok_or_else(|| UncodeError::Tool("query field required".into()))?;

        self.handle_query(query, &arguments)
            .map_err(UncodeError::Tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_ai::api_types::InputModality;
    use uncode_ai::model::ModelPricingPerMillion;

    fn test_models() -> Vec<Model> {
        vec![
            Model {
                id: "cheap-text".into(),
                name: "Cheap Text".into(),
                api: "openai-completions".into(),
                provider: "test".into(),
                base_url: "https://api.test.com".into(),
                context_window: 32_000,
                max_output_tokens: 4096,
                reasoning: false,
                input_modalities: vec![InputModality::Text],
                pricing: ModelPricingPerMillion {
                    input: 0.1,
                    output: 0.2,
                    ..Default::default()
                },
                ..Model::default()
            },
            Model {
                id: "vision-pro".into(),
                name: "Vision Pro".into(),
                api: "anthropic-messages".into(),
                provider: "anthropic".into(),
                base_url: "https://api.anthropic.com".into(),
                context_window: 200_000,
                max_output_tokens: 8192,
                reasoning: true,
                input_modalities: vec![InputModality::Text, InputModality::Image],
                pricing: ModelPricingPerMillion {
                    input: 3.0,
                    output: 15.0,
                    ..Default::default()
                },
                ..Model::default()
            },
            Model {
                id: "reasoning-model".into(),
                name: "Reasoning".into(),
                api: "openai-completions".into(),
                provider: "deepseek".into(),
                base_url: "https://api.deepseek.com".into(),
                context_window: 128_000,
                max_output_tokens: 8192,
                reasoning: true,
                input_modalities: vec![InputModality::Text],
                pricing: ModelPricingPerMillion {
                    input: 1.0,
                    output: 2.0,
                    ..Default::default()
                },
                ..Model::default()
            },
        ]
    }

    fn make_tool() -> LLMQueryTool {
        LLMQueryTool::new(Arc::new(test_models()))
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = make_tool();
        let def = tool.definition();
        assert_eq!(def.name, "llm_query");
        assert!(def.parameters["properties"]["query"].is_object());
    }

    #[tokio::test]
    async fn test_capability_query() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "capabilities",
                "model_id": "vision-pro"
            }))
            .await
            .unwrap();
        assert!(result.contains("vision-pro"));
        assert!(result.contains("supports_vision: true"));
        assert!(result.contains("supports_reasoning: true"));
        assert!(result.contains("anthropic-messages"));
    }

    #[tokio::test]
    async fn test_capability_query_unknown_model() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "capabilities",
                "model_id": "nonexistent"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cheapest_no_filter() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({"query": "cheapest"}))
            .await
            .unwrap();
        assert!(result.contains("cheap-text"));
        assert!(result.contains("3 found"));
    }

    #[tokio::test]
    async fn test_cheapest_with_vision() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "cheapest",
                "supports_vision": true
            }))
            .await
            .unwrap();
        assert!(result.contains("vision-pro"));
        assert!(!result.contains("cheap-text"));
    }

    #[tokio::test]
    async fn test_cheapest_with_min_context() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "cheapest",
                "min_context_window": 100_000
            }))
            .await
            .unwrap();
        assert!(result.contains("reasoning-model"));
        assert!(result.contains("vision-pro"));
        assert!(!result.contains("cheap-text"));
    }

    #[tokio::test]
    async fn test_list_all() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({"query": "list"}))
            .await
            .unwrap();
        assert!(result.contains("cheap-text"));
        assert!(result.contains("vision-pro"));
        assert!(result.contains("reasoning-model"));
    }

    #[tokio::test]
    async fn test_list_by_provider() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "list",
                "provider": "deepseek"
            }))
            .await
            .unwrap();
        assert!(result.contains("reasoning-model"));
        assert!(!result.contains("vision-pro"));
    }

    #[tokio::test]
    async fn test_recommend_with_context_tokens() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "recommend",
                "context_tokens": 50_000,
                "supports_reasoning": true
            }))
            .await
            .unwrap();
        assert!(result.contains("reasoning-model"));
        assert!(result.contains("vision-pro"));
        assert!(!result.contains("cheap-text"));
    }

    #[tokio::test]
    async fn test_recommend_no_match() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({
                "query": "recommend",
                "context_tokens": 500_000
            }))
            .await
            .unwrap();
        assert!(result.contains("no models"));
    }

    #[tokio::test]
    async fn test_unknown_query_type() {
        let tool = make_tool();
        let result = tool
            .execute(serde_json::json!({"query": "invalid"}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown query"));
    }

    #[tokio::test]
    async fn test_missing_query_field() {
        let tool = make_tool();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
