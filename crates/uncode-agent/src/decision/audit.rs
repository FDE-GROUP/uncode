//! 审计器 — 决策轨迹的记录与回放
//!
//! ## 职责
//!
//! - 记录每次裁决和执行结果为 `DecisionRecord`
//! - 生成面向离线训练的 `AgentStep`
//! - 提供历史决策的查询和回放接口
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 决策层

use super::types::{ActionObservation, AgentStateSnapshot, AgentStep, DecisionRecord, ExecutedAction, Feedback};

/// 审计器 — 决策轨迹的管理者
pub struct Auditor {
    pub trail: Vec<DecisionRecord>,
}

impl Auditor {
    pub fn new() -> Self {
        Self { trail: Vec::new() }
    }

    /// 记录一次裁决 + 执行结果
    pub fn record(
        &mut self,
        record: DecisionRecord,
    ) {
        self.trail.push(record);
    }

    /// 从最近一次记录生成 AgentStep（面向离线训练）
    pub fn generate_step(
        &self,
        turn_id: impl Into<String>,
        state_before: AgentStateSnapshot,
        observation: ActionObservation,
        feedback: Option<Feedback>,
    ) -> Option<AgentStep> {
        self.trail.last().map(|record| {
            let action = record.approved_action.as_ref();
            AgentStep {
                step_id: uuid::Uuid::new_v4().to_string(),
                turn_id: turn_id.into(),
                state_before,
                action: ExecutedAction {
                    tool_name: action.map(|a| a.action.tool_name.clone()).unwrap_or_default(),
                    arguments_summary: String::new(),
                    duration_ms: 0,
                },
                observation,
                feedback,
                timestamp: chrono::Utc::now(),
            }
        })
    }

    /// 返回完整决策轨迹
    pub fn trail(&self) -> &[DecisionRecord] {
        &self.trail
    }
}
