//! 决策反馈桥 — 事件流上行通道
//!
//! ## 认知与决策驱动设计中的定位
//!
//! 原则 5：**事件流是双向通道**。
//! 决策层的执行结果必须以结构化事件形式回流到认知层，
//! 形成"行动 → 观察 → 反馈 → 下次行动"的闭环。
//!
//! ## 数据流
//!
//! ```text
//! ExecutionResult (决策层产出)
//!   → ActionObservation (结构化观察)
//!   → AgentStep (训练模型, uncode-core)
//!   → WorkingMemory.observe() (认知层反馈)
//!   → AgentEvent (可观测性)
//! ```

use uncode_core::agent_step::{ActionObservation, AgentStep, AgentStateSnapshot, ExecutedAction, Feedback};

use super::evaluator::{BasicEvaluator, EvaluationContext, Evaluator, TurnEvaluation};
use super::execution::ExecutionResult;

/// 决策反馈桥 — 连接决策层产出到认知层
pub struct FeedbackBridge;

impl FeedbackBridge {
    /// 从执行结果构建 ActionObservation
    pub fn to_observation(result: &ExecutionResult) -> ActionObservation {
        ActionObservation {
            success: result.success,
            output_summary: result.output.clone().unwrap_or_default(),
            files_changed: vec![], // 由调用方从工具细节中提取
            duration_ms: result.duration_ms,
            terminate: result.terminate,
        }
    }

    /// 构建 AgentStep（面向离线训练）
    pub fn to_agent_step(
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
            observation: Self::to_observation(result),
            feedback,
            timestamp: chrono::Utc::now(),
        }
    }

    /// 分析执行结果，推断 feedback 信号
    pub fn infer_feedback(result: &ExecutionResult) -> Option<Feedback> {
        if !result.success {
            // 失败 → 潜在的回滚信号
            if let Some(ref err) = result.error {
                if err.contains("test") {
                    return Some(Feedback::TestFailed {
                        test_name: "execution".into(),
                        error: err.clone(),
                    });
                }
            }
            return Some(Feedback::AutoRevert {
                reason: result.error.clone().unwrap_or_else(|| "unknown error".into()),
            });
        }

        // 成功场景：检查是否有显式的测试通过标记
        if let Some(ref output) = result.output {
            if output.contains("test result: ok") {
                return Some(Feedback::TestPassed {
                    test_name: "execution".into(),
                });
            }
        }

        None
    }
}

/// 认知层反馈上下文——供 WorkingMemory 消费
#[derive(Debug, Clone)]
pub struct TurnFeedback {
    pub turn_number: u32,
    pub observations: Vec<String>,
    pub agent_steps: Vec<AgentStep>,
    pub evaluation: Option<TurnEvaluation>,
}

impl TurnFeedback {
    pub fn new(turn_number: u32) -> Self {
        Self { turn_number, observations: Vec::new(), agent_steps: Vec::new(), evaluation: None }
    }

    /// 添加一个执行结果（含评估）
    pub fn record(&mut self, result: &ExecutionResult, active_tools: &[String], context_tokens: usize, test_output: Option<&str>) {
        let observation = FeedbackBridge::to_observation(result);
        let feedback = FeedbackBridge::infer_feedback(result);
        let step = FeedbackBridge::to_agent_step(
            format!("turn-{}", self.turn_number),
            self.turn_number,
            active_tools,
            context_tokens,
            result,
            feedback,
        );

        // ── 评估 (H0-H3 阶梯) ──
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
        // 累积评估分数
        let mut scores = self.evaluation.as_ref()
            .map(|e| e.scores.clone())
            .unwrap_or_default();
        scores.push(score);
        self.evaluation = Some(TurnEvaluation::new(self.turn_number, scores));

        // 构造人类可读的观察
        let status = if result.success { "✅" } else { "❌" };
        let summary = format!(
            "{status} [{tool}] {output}",
            tool = result.tool_name,
            output = result.output.as_deref().unwrap_or("(no output)"),
        );
        self.observations.push(summary);

        self.agent_steps.push(step);
    }

    /// 生成给 WorkingMemory 的观察条目
    pub fn to_working_memory_entries(&self) -> Vec<(&str, bool)> {
        self.observations
            .iter()
            .enumerate()
            .map(|(i, obs)| {
                let is_important = i < 3 || obs.starts_with('❌');
                (obs.as_str(), is_important)
            })
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

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
            error: if success { None } else { Some("test failed: assertion error".into()) },
            terminate: false,
        }
    }

    #[test]
    fn test_observation_from_success() {
        let result = make_result(true, "read", "file contents");
        let obs = FeedbackBridge::to_observation(&result);
        assert!(obs.success);
        assert_eq!(obs.output_summary, "file contents");
    }

    #[test]
    fn test_observation_from_failure() {
        let result = make_result(false, "write", "permission denied");
        let obs = FeedbackBridge::to_observation(&result);
        assert!(!obs.success);
    }

    #[test]
    fn test_infer_feedback_test_failed() {
        let result = make_result(false, "bash", "test failed: assertion error");
        let fb = FeedbackBridge::infer_feedback(&result);
        assert!(matches!(fb, Some(Feedback::TestFailed { .. })));
    }

    #[test]
    fn test_infer_feedback_test_passed() {
        let result = make_result(true, "bash", "running tests... test result: ok. 10 passed");
        let fb = FeedbackBridge::infer_feedback(&result);
        assert!(matches!(fb, Some(Feedback::TestPassed { .. })));
    }

    #[test]
    fn test_agent_step_generation() {
        let result = make_result(true, "read", "content");
        let step = FeedbackBridge::to_agent_step(
            "turn-1", 1, &["read".into()], 5000, &result, None,
        );
        assert_eq!(step.action.tool_name, "read");
        assert!(step.observation.success);
    }

    #[test]
    fn test_turn_feedback_records_multiple() {
        let mut tf = TurnFeedback::new(1);
        tf.record(&make_result(true, "read", "ok"), &[], 1000, None);
        tf.record(&make_result(false, "write", "fail"), &[], 1000, None);
        assert_eq!(tf.observations.len(), 2);
        assert_eq!(tf.agent_steps.len(), 2);
        // 失败条目应标记为重要
        let entries = tf.to_working_memory_entries();
        assert!(entries[1].1, "failure should be marked important");
        // 评估应存在
        assert!(tf.evaluation.is_some());
    }
}
