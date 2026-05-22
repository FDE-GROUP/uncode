//! 评估器 — H0-H3 评估阶梯
//!
//! ## Harness Engineering 中的定位
//!
//! 学术论文《AI Harness Engineering》提出 H0-H3 四级评估阶梯：
//!
//! | 级别 | 名称 | 描述 | uncode 实现 |
//! |:---|:---|:---|:---|
//! | **H0** | 仅输出结果 | 工具执行完成，返回 raw output | ✅ 当前状态 |
//! | **H1** | 输出+自评 | 包含 pass/fail 判定 + 简述 | ✅ `AssessmentLevel::Basic` |
//! | **H2** | 输出+自动化验证 | 集成 test/lint/typecheck 结果 | ✅ `AssessmentLevel::Verified` |
//! | **H3** | 输出+可复现验证报告 | 完整的 evaluation report + 证据 | ⚠️ 预留 |
//!
//! ## 与认知与决策驱动设计的关系
//!
//! 评估属于**决策层审计阶段**的职责——不是"Agent 做得好不好"的主观判断，
//! 而是"执行结果是否满足可验证的质量标准"的系统判定。

use super::execution::ExecutionResult;

/// H0-H3 评估级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssessmentLevel {
    /// H0: 仅输出结果，无评估
    RawOutput = 0,
    /// H1: 有基本的成功/失败判定
    Basic = 1,
    /// H2: 经过自动化验证（test/lint/typecheck）
    Verified = 2,
    /// H3: 有完整的可复现验证报告
    Reproducible = 3,
}

/// 评估分数
#[derive(Debug, Clone)]
pub struct EvaluationScore {
    /// H0-H3 级别
    pub level: AssessmentLevel,
    /// 0.0-1.0 的质量分数
    pub quality_score: f32,
    /// 通过的检查项
    pub passed: Vec<String>,
    /// 失败的检查项
    pub failed: Vec<String>,
    /// 改进建议
    pub recommendation: Option<String>,
}

impl EvaluationScore {
    pub fn h0(output_summary: &str) -> Self {
        Self {
            level: AssessmentLevel::RawOutput,
            quality_score: if output_summary.is_empty() { 0.0 } else { 0.5 },
            passed: vec!["output produced".into()],
            failed: vec![],
            recommendation: None,
        }
    }

    pub fn h1(success: bool, output_summary: &str, error: Option<&str>) -> Self {
        let quality = if success { 0.7 } else { 0.3 };
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        let recommendation = if success {
            passed.push("execution succeeded".into());
            None
        } else {
            failed.push(format!("execution failed: {}", error.unwrap_or("unknown")));
            Some("review error and retry".into())
        };
        Self {
            level: AssessmentLevel::Basic,
            quality_score: quality,
            passed,
            failed,
            recommendation,
        }
    }

    pub fn h2(success: bool, test_output: &str) -> Self {
        let tests_passed = test_output.contains("test result: ok");
        let quality = match (success, tests_passed) {
            (true, true) => 0.9,
            (true, false) => 0.6,
            (false, _) => 0.2,
        };
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        if tests_passed {
            passed.push("automated tests passed".into());
        } else if !test_output.is_empty() {
            failed.push("automated tests failed".into());
        }
        Self {
            level: AssessmentLevel::Verified,
            quality_score: quality,
            passed,
            failed,
            recommendation: if !tests_passed {
                Some("fix failing tests".into())
            } else {
                None
            },
        }
    }
}

/// 评估器 — 决策层审计阶段的组件
pub trait Evaluator: Send + Sync {
    /// 评估一次执行结果
    fn evaluate(&self, result: &ExecutionResult, context: &EvaluationContext) -> EvaluationScore;

    fn name(&self) -> &'static str;
}

/// 评估上下文
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub turn_number: u32,
    pub tool_name: String,
    /// 是否有可用的测试输出
    pub test_output: Option<String>,
    /// 是否有可用的 lint 输出
    pub lint_output: Option<String>,
}

/// 默认评估器 — H1 级别
///
/// 基于执行结果的 success/error 做基本判断。
pub struct BasicEvaluator;

impl Evaluator for BasicEvaluator {
    fn evaluate(&self, result: &ExecutionResult, _ctx: &EvaluationContext) -> EvaluationScore {
        EvaluationScore::h1(
            result.success,
            result.output.as_deref().unwrap_or(""),
            result.error.as_deref(),
        )
    }
    fn name(&self) -> &'static str {
        "basic"
    }
}

/// 验证级评估器 — H2 级别
///
/// 集成 test/lint 输出做自动化验证。
pub struct VerifiedEvaluator;

impl Evaluator for VerifiedEvaluator {
    fn evaluate(&self, result: &ExecutionResult, ctx: &EvaluationContext) -> EvaluationScore {
        // 如果有测试输出，使用 H2 级别
        if let Some(ref test_output) = ctx.test_output {
            return EvaluationScore::h2(result.success, test_output);
        }
        // 回退到 H1
        EvaluationScore::h1(
            result.success,
            result.output.as_deref().unwrap_or(""),
            result.error.as_deref(),
        )
    }
    fn name(&self) -> &'static str {
        "verified"
    }
}

/// 评估聚合器 — 对 turn 内的多个评估结果汇总
#[derive(Debug, Clone)]
pub struct TurnEvaluation {
    pub turn_number: u32,
    pub scores: Vec<EvaluationScore>,
    pub overall_level: AssessmentLevel,
    pub overall_quality: f32,
}

impl TurnEvaluation {
    pub fn new(turn_number: u32, scores: Vec<EvaluationScore>) -> Self {
        let overall_level = scores
            .iter()
            .map(|s| s.level)
            .min()
            .unwrap_or(AssessmentLevel::RawOutput);

        let overall_quality = if scores.is_empty() {
            0.0
        } else {
            scores.iter().map(|s| s.quality_score).sum::<f32>() / scores.len() as f32
        };

        Self {
            turn_number,
            scores,
            overall_level,
            overall_quality,
        }
    }

    /// 生成评估摘要
    pub fn summary(&self) -> String {
        let level_name = match self.overall_level {
            AssessmentLevel::RawOutput => "H0",
            AssessmentLevel::Basic => "H1",
            AssessmentLevel::Verified => "H2",
            AssessmentLevel::Reproducible => "H3",
        };
        format!(
            "Turn {} evaluation: level={level_name} quality={:.0}% ({}/{} passed)",
            self.turn_number,
            self.overall_quality * 100.0,
            self.scores.iter().flat_map(|s| &s.passed).count(),
            self.scores.iter().flat_map(|s| &s.passed).count()
                + self.scores.iter().flat_map(|s| &s.failed).count(),
        )
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(success: bool, output: &str) -> ExecutionResult {
        ExecutionResult {
            tool_id: "t1".into(),
            tool_name: "test".into(),
            success,
            duration_ms: 50,
            output: Some(output.into()),
            error: if success {
                None
            } else {
                Some("command failed".into())
            },
            terminate: false,
        }
    }

    #[test]
    fn test_h0_raw_output() {
        let score = EvaluationScore::h0("some output");
        assert_eq!(score.level, AssessmentLevel::RawOutput);
        assert!(score.quality_score > 0.0);
    }

    #[test]
    fn test_h1_success() {
        let score = EvaluationScore::h1(true, "done", None);
        assert_eq!(score.level, AssessmentLevel::Basic);
        assert!(score.quality_score > 0.5);
        assert!(score.failed.is_empty());
    }

    #[test]
    fn test_h1_failure() {
        let score = EvaluationScore::h1(false, "", Some("error"));
        assert_eq!(score.level, AssessmentLevel::Basic);
        assert!(score.quality_score < 0.5);
        assert!(!score.failed.is_empty());
    }

    #[test]
    fn test_h2_with_passing_tests() {
        let score = EvaluationScore::h2(true, "test result: ok. 10 passed");
        assert_eq!(score.level, AssessmentLevel::Verified);
        assert!(score.quality_score > 0.8);
    }

    #[test]
    fn test_h2_with_failing_tests() {
        let score = EvaluationScore::h2(false, "test result: FAILED. 1 failed");
        assert_eq!(score.level, AssessmentLevel::Verified);
        assert!(score.quality_score < 0.5);
    }

    #[test]
    fn test_basic_evaluator() {
        let eval = BasicEvaluator;
        let result = make_result(true, "ok");
        let ctx = EvaluationContext {
            turn_number: 1,
            tool_name: "bash".into(),
            test_output: None,
            lint_output: None,
        };
        let score = eval.evaluate(&result, &ctx);
        assert_eq!(score.level, AssessmentLevel::Basic);
    }

    #[test]
    fn test_verified_evaluator_uses_tests() {
        let eval = VerifiedEvaluator;
        let result = make_result(true, "tests: test result: ok. 5 passed");
        let ctx = EvaluationContext {
            turn_number: 1,
            tool_name: "bash".into(),
            test_output: Some("test result: ok. 5 passed".into()),
            lint_output: None,
        };
        let score = eval.evaluate(&result, &ctx);
        assert_eq!(score.level, AssessmentLevel::Verified);
    }

    #[test]
    fn test_turn_evaluation_aggregation() {
        let scores = vec![
            EvaluationScore::h1(true, "done", None),
            EvaluationScore::h2(true, "test result: ok"),
        ];
        let turn_eval = TurnEvaluation::new(1, scores);
        assert!(turn_eval.overall_quality > 0.7);
        let summary = turn_eval.summary();
        // overall_level = min(H1, H2) = H1 (weakest link)
        assert!(
            summary.contains("H1"),
            "expected H1 (weakest link), got: {summary}"
        );
    }

    #[test]
    fn test_assessment_level_ordering() {
        assert!(AssessmentLevel::Verified > AssessmentLevel::Basic);
        assert!(AssessmentLevel::Reproducible > AssessmentLevel::Verified);
        assert!(AssessmentLevel::Basic > AssessmentLevel::RawOutput);
    }
}
