//! Turn 级阶段小结：可选 LLM 生成自然语言 bullet，失败时回退启发式 `tool(args)` 行。

use std::collections::HashMap;

use futures::StreamExt;
use uncode_ai::{ApiRegistry, StreamEvent};
use uncode_core::api_types::{Context, StreamOptions};
use uncode_core::event::PhaseSummaryData;
use uncode_core::message::{Message, UsageInfo};
use uncode_core::model::Model;

const PHASE_SUMMARY_SYSTEM: &str =
    "你是编码 Agent 的回合小结助手。根据本轮工具执行与助手说明，用简洁中文输出 JSON，不要 markdown 代码块。";

const PHASE_SUMMARY_PROMPT: &str = "\
请为本轮（Turn {turn}）生成阶段小结。输入为工具执行摘要；若助手有文字说明则一并参考。

要求：
- completed：2～5 条，每条一句自然语言，描述「做了什么、结果如何」，不要照抄 tool(args) 字面
- issues：失败或异常项，无则 []
- next_steps：若模型可能继续调用工具则 1～2 条提示；否则 []

仅输出一行 JSON，键名固定：
{{\"completed\":[\"...\"],\"issues\":[\"...\"],\"next_steps\":[\"...\"]}}

助手说明（可空）：
{assistant}

成功工具：
{completed_tools}

失败工具：
{failed_tools}

内层工具链未结束（可能继续调工具）：{continues}
";

/// LLM 阶段小结输入（避免 `try_llm_phase_summary` 参数过多）。
pub struct PhaseSummaryLlmInput<'a> {
    pub turn: u64,
    pub completed_labels: &'a [String],
    pub issue_labels: &'a [String],
    pub assistant_snippet: &'a str,
    pub has_more_tool_calls: bool,
    pub token_usage: UsageInfo,
    pub api_registry: &'a ApiRegistry,
    pub model: &'a Model,
    pub api_keys: &'a HashMap<String, String>,
}

/// 是否启用 LLM 阶段小结（默认开启；`UNCODE_PHASE_SUMMARY_LLM=0|false` 关闭）。
pub fn llm_phase_summary_enabled() -> bool {
    if cfg!(test) {
        return false;
    }
    match std::env::var("UNCODE_PHASE_SUMMARY_LLM") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v == "0" || v == "false" || v == "no" || v == "off")
        }
        Err(_) => true,
    }
}

/// 启发式小结（P4）：`completed` / `issues` 为 `tool(args)` 行。
pub fn build_phase_summary_heuristic(
    turn: u64,
    completed: Vec<String>,
    issues: Vec<String>,
    has_more_tool_calls: bool,
    token_usage: UsageInfo,
) -> PhaseSummaryData {
    let next_steps = if has_more_tool_calls {
        vec!["模型可能在下一轮继续调用工具。".to_string()]
    } else {
        Vec::new()
    };
    PhaseSummaryData {
        phase: turn,
        completed,
        issues,
        next_steps,
        token_usage,
    }
}

/// 尝试 LLM 自然语言小结；解析失败或 API 错误时返回 `None`。
pub async fn try_llm_phase_summary(input: PhaseSummaryLlmInput<'_>) -> Option<PhaseSummaryData> {
    if !llm_phase_summary_enabled() {
        return None;
    }

    let completed_tools = if input.completed_labels.is_empty() {
        "（无）".to_string()
    } else {
        input.completed_labels.join("\n")
    };
    let failed_tools = if input.issue_labels.is_empty() {
        "（无）".to_string()
    } else {
        input.issue_labels.join("\n")
    };
    let assistant_block = if input.assistant_snippet.is_empty() {
        "（无）".to_string()
    } else {
        input.assistant_snippet.to_string()
    };

    let prompt = PHASE_SUMMARY_PROMPT
        .replace("{turn}", &input.turn.to_string())
        .replace("{assistant}", &assistant_block)
        .replace("{completed_tools}", &completed_tools)
        .replace("{failed_tools}", &failed_tools)
        .replace(
            "{continues}",
            if input.has_more_tool_calls { "是" } else { "否" },
        );

    let raw = match llm_one_shot(
        PHASE_SUMMARY_SYSTEM,
        &prompt,
        input.api_registry,
        input.model,
        input.api_keys,
        512,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("phase summary LLM skipped: {e}");
            return None;
        }
    };

    let parsed = parse_phase_summary_json(&raw)?;
    if parsed.completed.is_empty() && parsed.issues.is_empty() && parsed.next_steps.is_empty() {
        return None;
    }

    Some(PhaseSummaryData {
        phase: input.turn,
        completed: parsed.completed,
        issues: parsed.issues,
        next_steps: parsed.next_steps,
        token_usage: input.token_usage,
    })
}

#[derive(Debug, Default)]
struct ParsedPhaseBullets {
    completed: Vec<String>,
    issues: Vec<String>,
    next_steps: Vec<String>,
}

fn parse_phase_summary_json(raw: &str) -> Option<ParsedPhaseBullets> {
    let trimmed = raw.trim();
    let json_str = extract_json_object(trimmed)?;
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    Some(ParsedPhaseBullets {
        completed: string_array(&value, "completed"),
        issues: string_array(&value, "issues"),
        next_steps: string_array(&value, "next_steps"),
    })
}

fn extract_json_object(s: &str) -> Option<&str> {
    if let Some(start) = s.find('{')
        && let Some(end) = s.rfind('}')
        && end > start
    {
        return Some(&s[start..=end]);
    }
    None
}

fn string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(str::trim).filter(|t| !t.is_empty()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn llm_one_shot(
    system_prompt: &str,
    user_prompt: &str,
    api_registry: &ApiRegistry,
    model: &Model,
    api_keys: &HashMap<String, String>,
    max_tokens: u32,
) -> anyhow::Result<String> {
    let api_key = api_keys.get(&model.provider).cloned();
    let context = Context {
        system_prompt: Some(system_prompt.into()),
        messages: vec![Message::user(user_prompt)],
        tools: vec![],
    };
    let options = StreamOptions {
        api_key,
        temperature: Some(0.2),
        max_tokens: Some(max_tokens),
        ..StreamOptions::default()
    };

    let mut stream = uncode_ai::stream_simple(model, &context, &options, api_registry).await?;
    let mut out = String::with_capacity(256);
    while let Some(event) = stream.next().await {
        if let StreamEvent::TextDelta(text) = event {
            out.push_str(&text);
        }
    }
    Ok(out)
}

/// Shorten tool arguments for phase summary context lines.
pub fn summarize_tool_args(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.chars().count() <= 48 {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(48).collect::<String>())
    }
}

pub fn format_tool_phase_label(tool_name: &str, args_short: &str) -> String {
    if args_short.is_empty() {
        tool_name.to_string()
    } else {
        format!("{tool_name}({args_short})")
    }
}

/// Truncate assistant text for the phase-summary prompt.
pub fn assistant_snippet_for_phase(text: &str, max_chars: usize) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.chars().count() <= max_chars {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_phase_summary_json_extracts_bullets() {
        let raw = r#"说明
{"completed":["已读取 main.rs 并确认入口"],"issues":[],"next_steps":["继续运行测试"]}"#;
        let p = parse_phase_summary_json(raw).unwrap();
        assert_eq!(p.completed.len(), 1);
        assert!(p.completed[0].contains("main.rs"));
        assert_eq!(p.next_steps.len(), 1);
    }

    #[test]
    fn assistant_snippet_truncates() {
        let long = "字".repeat(100);
        let s = assistant_snippet_for_phase(&long, 20);
        assert!(s.ends_with('…'));
    }
}
