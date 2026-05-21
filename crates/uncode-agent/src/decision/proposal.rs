//! 提案接收 — 从 LLM 流式输出中累积工具调用提案
//!
//! 对应决策层四阶段中的"提案接收"阶段。
//! 负责将 LLM 的 `StreamEvent::ToolCall*` 流累积为 `ActionProposal`。
//!
//! ## 提取来源
//!
//! 从 `loop_engine.rs::run_inner()` 的 stream 处理循环中提取
//! （ToolCallStart → ToolCallDelta → ToolCallEnd 的累积逻辑）。
//!
//! 参见 `docs/ai-agent-archi/uncodenow-refactoring-roadmap.md` §1.2

use std::collections::HashMap;

use uncode_ai::StreamEvent;

use super::types::ActionProposal;
use crate::decision::firewall::SemanticFirewall;

/// 流式提案累积器 — 从 LLM StreamEvent 流中提取工具调用
///
/// 用法：
/// ```text
/// for event in llm_stream {
///     accumulator.feed(event);
/// }
/// let proposals = accumulator.finalize();
/// ```
pub struct ProposalAccumulator {
    /// (tool_call_id, tool_name, accumulated_arguments)
    pending: Vec<(String, String, String)>,
    /// 已推送过 early progress 的 tool_call_id
    args_pushed: HashMap<String, bool>,
    /// 最终完成的提案
    completed: Vec<ActionProposal>,
}

impl ProposalAccumulator {
    pub fn new() -> Self {
        Self {
            pending: Vec::with_capacity(8),
            args_pushed: HashMap::new(),
            completed: Vec::with_capacity(8),
        }
    }

    /// 喂入一个 StreamEvent
    ///
    /// 返回 `Some(ActionProposal)` 当 ToolCallEnd 完成时；
    /// 其他事件返回 `None`。
    pub fn feed(&mut self, event: &StreamEvent) -> Option<ActionProposal> {
        match event {
            StreamEvent::ToolCallStart { id, name } => {
                self.pending.push((id.clone(), name.clone(), String::new()));
                None
            }
            StreamEvent::ToolCallDelta { id, arguments } => {
                if let Some(tc) = self.pending.iter_mut().find(|(tid, ..)| tid == id) {
                    tc.2.push_str(arguments);
                }
                None
            }
            StreamEvent::ToolCallEnd(data) => {
                let proposal = ActionProposal {
                    tool_name: data.name.clone(),
                    raw_arguments: data.arguments.clone(),
                    rationale: self.extract_rationale(&data.name, &data.arguments),
                    confidence: None,
                };
                self.completed.push(proposal.clone());
                // 从 pending 中移除以保持内存
                self.pending.retain(|(tid, ..)| tid != &data.id);
                Some(proposal)
            }
            _ => None,
        }
    }

    /// 返回所有已完成的提案
    pub fn completed(&self) -> &[ActionProposal] {
        &self.completed
    }

    /// 重置累积器（用于新的 LLM 调用）
    pub fn reset(&mut self) {
        self.pending.clear();
        self.args_pushed.clear();
        self.completed.clear();
    }

    fn extract_rationale(&self, name: &str, args: &serde_json::Value) -> Option<String> {
        if name == "bash" {
            args.get("description").and_then(|v| v.as_str()).map(|s| s.to_string())
        } else {
            None
        }
    }
}

/// 将累积的 ActionProposal 通过防火墙处理为可裁决的提案
pub async fn process_proposals(
    proposals: Vec<ActionProposal>,
    firewall: &SemanticFirewall,
) -> Result<
    Vec<crate::decision::types::NormalizedAction>,
    crate::decision::firewall::FirewallError,
> {
    let mut results = Vec::with_capacity(proposals.len());
    for proposal in &proposals {
        let normalized = firewall.process(proposal)?;
        results.push(normalized);
    }
    Ok(results)
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_ai::ToolCallEndData;

    fn make_end(id: &str, name: &str, args: serde_json::Value) -> StreamEvent {
        StreamEvent::ToolCallEnd(Box::new(ToolCallEndData {
            id: id.into(),
            name: name.into(),
            arguments: args,
        }))
    }

    #[test]
    fn test_accumulator_single_tool_call() {
        let mut acc = ProposalAccumulator::new();

        acc.feed(&StreamEvent::ToolCallStart { id: "1".into(), name: "read".into() });
        acc.feed(&StreamEvent::ToolCallDelta { id: "1".into(), arguments: r#"{"path":"src/main.rs"}"#.into() });

        let proposal = acc.feed(&make_end("1", "read", serde_json::json!({"path": "src/main.rs"})));
        assert!(proposal.is_some());
        let p = proposal.unwrap();
        assert_eq!(p.tool_name, "read");
        assert_eq!(p.raw_arguments["path"], "src/main.rs");
    }

    #[test]
    fn test_accumulator_multiple_tool_calls() {
        let mut acc = ProposalAccumulator::new();

        acc.feed(&StreamEvent::ToolCallStart { id: "1".into(), name: "read".into() });
        acc.feed(&StreamEvent::ToolCallDelta { id: "1".into(), arguments: r#"{"path":"a.rs"}"#.into() });
        acc.feed(&make_end("1", "read", serde_json::json!({"path": "a.rs"})));

        acc.feed(&StreamEvent::ToolCallStart { id: "2".into(), name: "write".into() });
        acc.feed(&StreamEvent::ToolCallDelta { id: "2".into(), arguments: r#"{"path":"b.rs"}"#.into() });
        acc.feed(&make_end("2", "write", serde_json::json!({"path": "b.rs"})));

        assert_eq!(acc.completed().len(), 2);
    }

    #[test]
    fn test_accumulator_reset() {
        let mut acc = ProposalAccumulator::new();

        acc.feed(&StreamEvent::ToolCallStart { id: "1".into(), name: "read".into() });
        acc.feed(&make_end("1", "read", serde_json::json!({"path": "a.rs"})));

        assert_eq!(acc.completed().len(), 1);
        acc.reset();
        assert!(acc.completed().is_empty());
    }
}
