//! 决策反馈桥 — 事件流上行通道
//!
//! ## 认知显化与决策驱动设计中的定位
//!
//! 原则 5：**事件流是双向通道**。
//! 决策层的执行结果必须以结构化事件形式回流到认知层，
//! 形成"行动 → 观察 → 反馈 → 下次行动"的闭环。

use uncode_core::agent_step::{
    ActionObservation, AgentStateSnapshot, AgentStep, ExecutedAction, Feedback,
};

use super::evaluator::{BasicEvaluator, EvaluationContext, Evaluator, TurnEvaluation};
use super::types::ExecutionResult;

fn to_observation(result: &ExecutionResult) -> ActionObservation {
    ActionObservation {
        success: result.success,
        output_summary: result.output.clone().unwrap_or_default(),
        files_changed: vec![],
        duration_ms: result.duration_ms,
        terminate: result.terminate,
    }
}

fn to_agent_step(
    turn_id: impl Into<String>,
    turn_number: u32,
    active_tools: &[String],
    context_size_tokens: usize,
    result: &ExecutionResult,
    feedback: Option<Feedback>,
) -> AgentStep {
    AgentStep {
        step_id: uuid::Uuid::new_v4().to_string(),
        turn_id: turn_id.into(),
        state_before: AgentStateSnapshot {
            phase: "turn".into(),
            turn_number,
            active_tools: active_tools.to_vec(),
            context_size_tokens,
        },
        action: ExecutedAction {
            tool_name: result.tool_name.clone(),
            arguments_summary: String::new(),
            duration_ms: result.duration_ms,
        },
        observation: to_observation(result),
        feedback,
        timestamp: chrono::Utc::now(),
    }
}

fn infer_feedback(result: &ExecutionResult) -> Option<Feedback> {
    if !result.success {
        if let Some(ref err) = result.error {
            if err.contains("test") {
                return Some(Feedback::TestFailed {
                    test_name: "execution".into(),
                    error: err.clone(),
                });
            }
        }
        return Some(Feedback::AutoRevert {
            reason: result
                .error
                .clone()
                .unwrap_or_else(|| "unknown error".into()),
        });
    }

    if let Some(ref output) = result.output {
        if output.contains("test result: ok") {
            return Some(Feedback::TestPassed {
                test_name: "execution".into(),
            });
        }
    }

    None
}

/// 决策摘要 — 记录本 turn 内的裁决结果供认知层消费
#[derive(Debug, Clone, Default)]
pub struct DecisionSummary {
    pub tools_approved: Vec<String>,
    pub tools_denied: Vec<String>,
    pub denial_reasons: Vec<String>,
    pub firewall_violations: Vec<String>,
}

/// 认知层反馈上下文——供 WorkingMemory 消费
#[derive(Debug, Clone)]
pub struct TurnFeedback {
    pub turn_number: u32,
    pub observations: Vec<String>,
    pub agent_steps: Vec<AgentStep>,
    pub evaluation: Option<TurnEvaluation>,
    pub decision_summary: Option<DecisionSummary>,
}

impl TurnFeedback {
    pub fn new(turn_number: u32) -> Self {
        Self {
            turn_number,
            observations: Vec::new(),
            agent_steps: Vec::new(),
            evaluation: None,
            decision_summary: None,
        }
    }

    /// 添加一个执行结果（含评估）
    pub fn record(
        &mut self,
        result: &ExecutionResult,
        active_tools: &[String],
        context_tokens: usize,
        test_output: Option<&str>,
    ) {
        let observation = to_observation(result);
        let feedback = infer_feedback(result);
        let step = to_agent_step(
            format!("turn-{}", self.turn_number),
            self.turn_number,
            active_tools,
            context_tokens,
            result,
            feedback,
        );

        // Observation → WorkingMemory：成功/失败摘要注入认知层
        if !observation.success {
            self.observations.push(format!(
                "[observation] tool {} failed: {}",
                result.tool_name,
                observation
                    .output_summary
                    .chars()
                    .take(200)
                    .collect::<String>()
            ));
        }

        let eval_ctx = EvaluationContext {
            turn_number: self.turn_number,
            tool_name: result.tool_name.clone(),
            test_output: test_output.map(|s| s.to_string()),
            lint_output: None,
        };
        let evaluator: &dyn Evaluator = if test_output.is_some() {
            &super::evaluator::VerifiedEvaluator
        } else {
            &BasicEvaluator
        };
        let score = evaluator.evaluate(result, &eval_ctx);
        let mut scores = self
            .evaluation
            .as_ref()
            .map(|e| e.scores.clone())
            .unwrap_or_default();
        scores.push(score);
        self.evaluation = Some(TurnEvaluation::new(self.turn_number, scores));

        let status = if result.success { "✅" } else { "❌" };
        let summary = format!(
            "{status} [{tool}] {output}",
            tool = result.tool_name,
            output = result.output.as_deref().unwrap_or("(no output)"),
        );
        self.observations.push(summary);

        self.agent_steps.push(step);
    }

    /// 生成给 WorkingMemory 的观察条目（含决策摘要）
    pub fn to_working_memory_entries(&self) -> Vec<(&str, bool)> {
        let entries: Vec<(&str, bool)> = self
            .observations
            .iter()
            .enumerate()
            .map(|(i, obs)| {
                let is_important = i < 3 || obs.starts_with('❌');
                (obs.as_str(), is_important)
            })
            .collect();

        // Decision summary observations are handled separately via decision_observations()
        entries
    }

    /// Return decision summary observations (important) for WorkingMemory injection
    pub fn decision_observations(&self) -> Vec<String> {
        let mut obs = Vec::new();
        if let Some(ref ds) = self.decision_summary {
            if !ds.tools_denied.is_empty() {
                obs.push(format!(
                    "denied tools: {} — reasons: {}",
                    ds.tools_denied.join(", "),
                    ds.denial_reasons.join("; ")
                ));
            }
            if !ds.firewall_violations.is_empty() {
                obs.push(format!(
                    "firewall violations: {}",
                    ds.firewall_violations.join("; ")
                ));
            }
        }
        obs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(success: bool, tool: &str, output: &str) -> ExecutionResult {
        ExecutionResult {
            tool_id: "test-1".into(),
            tool_name: tool.into(),
            success,
            duration_ms: 100,
            output: Some(output.into()),
            error: if success {
                None
            } else {
                Some("test failed: assertion error".into())
            },
            terminate: false,
        }
    }

    #[test]
    fn test_observation_from_success() {
        let result = make_result(true, "read", "file contents");
        let obs = to_observation(&result);
        assert!(obs.success);
        assert_eq!(obs.output_summary, "file contents");
    }

    #[test]
    fn test_observation_from_failure() {
        let result = make_result(false, "write", "permission denied");
        let obs = to_observation(&result);
        assert!(!obs.success);
    }

    #[test]
    fn test_infer_feedback_test_failed() {
        let result = make_result(false, "bash", "test failed: assertion error");
        let fb = infer_feedback(&result);
        assert!(matches!(fb, Some(Feedback::TestFailed { .. })));
    }

    #[test]
    fn test_infer_feedback_test_passed() {
        let result = make_result(true, "bash", "running tests... test result: ok. 10 passed");
        let fb = infer_feedback(&result);
        assert!(matches!(fb, Some(Feedback::TestPassed { .. })));
    }

    #[test]
    fn test_agent_step_generation() {
        let result = make_result(true, "read", "content");
        let step = to_agent_step("turn-1", 1, &["read".into()], 5000, &result, None);
        assert_eq!(step.action.tool_name, "read");
        assert!(step.observation.success);
    }

    #[test]
    fn test_turn_feedback_records_multiple() {
        let mut tf = TurnFeedback::new(1);
        tf.record(&make_result(true, "read", "ok"), &[], 1000, None);
        tf.record(&make_result(false, "write", "fail"), &[], 1000, None);
        // success → 1 observation, failure → 2 observations (observation + summary)
        assert_eq!(tf.observations.len(), 3);
        assert_eq!(tf.agent_steps.len(), 2);
        let entries = tf.to_working_memory_entries();
        assert!(entries[1].1, "failure observation should be important");
        assert!(tf.evaluation.is_some());
    }

    #[test]
    fn test_decision_summary_default() {
        let ds = DecisionSummary::default();
        assert!(ds.tools_approved.is_empty());
        assert!(ds.tools_denied.is_empty());
        assert!(ds.denial_reasons.is_empty());
        assert!(ds.firewall_violations.is_empty());
    }

    #[test]
    fn test_decision_observations_empty() {
        let tf = TurnFeedback::new(1);
        assert!(tf.decision_observations().is_empty());
    }

    #[test]
    fn test_decision_observations_with_denied() {
        let mut tf = TurnFeedback::new(1);
        tf.decision_summary = Some(DecisionSummary {
            tools_denied: vec!["bash".into(), "write".into()],
            denial_reasons: vec!["blocked by policy".into(), "path unsafe".into()],
            ..Default::default()
        });
        let obs = tf.decision_observations();
        assert_eq!(obs.len(), 1);
        assert!(obs[0].contains("denied tools: bash, write"));
        assert!(obs[0].contains("blocked by policy; path unsafe"));
    }

    #[test]
    fn test_decision_observations_with_violations() {
        let mut tf = TurnFeedback::new(1);
        tf.decision_summary = Some(DecisionSummary {
            firewall_violations: vec!["argument validation failed".into()],
            ..Default::default()
        });
        let obs = tf.decision_observations();
        assert_eq!(obs.len(), 1);
        assert!(obs[0].contains("firewall violations"));
    }
}
