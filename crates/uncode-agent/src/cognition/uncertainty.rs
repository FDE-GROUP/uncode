//! 不确定性三分类显式建模
//!
//! ## 背景
//!
//! 当前 `ErrorCategory`（`Llm | Tool | Network | Config`）按错误**来源**分类。
//! `UncertaintyClass` 按不确定性的**性质**分类——这不是替代，是补充维度。
//!
//! ## 三分类
//!
//! | 不确定性类型 | 来源 | 适配策略 |
//! |:---|:---|:---|
//! | **生成不确定性** | LLM 采样机制 | 约束+验证（Schema、规则、类型） |
//! | **认知不完全性** | 上下文不足、信息缺失 | 记忆与检索建模 |
//! | **执行不确定性** | 外部系统、工具调用 | 补偿事务/事件溯源 |
//!
//! 参见：
//! - `docs/ai-agent-archi/ddd-ai-agent.md` §1.1 不确定性的三层解构
//! - `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 认知层

/// 不确定性的领域分类——按**性质**而非来源
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum UncertaintyClass {
    /// LLM 采样导致的多候选差异
    Generative(GenerativeConfig),
    /// 上下文不足导致的信息缺口
    Cognitive(CognitiveGap),
    /// 外部系统/工具调用导致的失败
    Executional(ExecutionContext),
}

// ── 生成不确定性 ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenerativeConfig {
    pub candidates: Vec<String>,
    pub temperature: f32,
    pub strategy: GenerativeStrategy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GenerativeStrategy {
    /// 多样本 + rerank，选最优
    Rerank,
    /// 多数投票
    MajorityVote,
    /// 取 N 个中最优
    BestOfN(usize),
    /// 单次采样（当前默认行为）
    SingleSample,
}

// ── 认知不完全性 ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CognitiveGap {
    pub missing_context: Vec<ContextRequirement>,
    pub suggested_remediation: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ContextRequirement {
    /// 需要读取特定文件
    FileContent(String),
    /// 需要查阅文档
    Documentation(String),
    /// 需要工作区结构信息
    WorkspaceStructure,
    /// 需要上一步决策的结果
    PreviousDecision,
    /// 需要用户澄清
    UserClarification(String),
}

// ── 执行不确定性 ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionContext {
    pub error: String,
    pub retry_count: u32,
    pub max_retries: u32,
    pub strategy: ExecutionStrategy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExecutionStrategy {
    /// 指数退避重试
    Retry,
    /// 降级到备选工具
    FallbackTool { tool_name: String },
    /// 补偿动作
    Compensate,
    /// 升级到人工处理
    Escalate,
}

// ── 不确定性处理结果 ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum UncertaintyResolution {
    /// 已通过指定策略解决
    Resolved { strategy_used: String },
    /// 已升级
    Escalated { reason: String },
    /// 多次尝试后仍未解决
    Unresolved { attempts: u32 },
}

// ── 辅助构造 ──

impl UncertaintyClass {
    /// 从 ErrorCategory 推断 UncertaintyClass
    ///
    /// `ErrorCategory::Llm` → 可能是生成或认知不确定性的信号
    /// `ErrorCategory::Tool` → 执行不确定性
    /// `ErrorCategory::Network` → 执行不确定性（重试）
    /// `ErrorCategory::Config` → 非不确定性——直接配置错误
    pub fn from_error_category(category: &str, message: &str) -> Self {
        match category {
            "llm" => {
                if message.contains("context") || message.contains("token") {
                    UncertaintyClass::Cognitive(CognitiveGap {
                        missing_context: vec![ContextRequirement::WorkspaceStructure],
                        suggested_remediation: "compact context or reduce input size".into(),
                    })
                } else {
                    UncertaintyClass::Generative(GenerativeConfig {
                        candidates: vec![],
                        temperature: 0.0,
                        strategy: GenerativeStrategy::SingleSample,
                    })
                }
            }
            "tool" | "network" => UncertaintyClass::Executional(ExecutionContext {
                error: message.to_string(),
                retry_count: 0,
                max_retries: 3,
                strategy: ExecutionStrategy::Retry,
            }),
            _ => UncertaintyClass::Executional(ExecutionContext {
                error: message.to_string(),
                retry_count: 0,
                max_retries: 1,
                strategy: ExecutionStrategy::Escalate,
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generative_uncertainty_from_llm_error() {
        let class = UncertaintyClass::from_error_category("llm", "model returned error");
        assert!(matches!(class, UncertaintyClass::Generative(_)));
    }

    #[test]
    fn test_cognitive_uncertainty_from_context_overflow() {
        let class =
            UncertaintyClass::from_error_category("llm", "context length exceeded");
        assert!(matches!(class, UncertaintyClass::Cognitive(_)));
    }

    #[test]
    fn test_executional_from_tool_error() {
        let class = UncertaintyClass::from_error_category("tool", "bash: command not found");
        assert!(matches!(class, UncertaintyClass::Executional(_)));
    }

    #[test]
    fn test_executional_from_network_error() {
        let class = UncertaintyClass::from_error_category("network", "connection reset");
        let exec = match class {
            UncertaintyClass::Executional(e) => e,
            _ => panic!("expected Executional"),
        };
        assert_eq!(exec.max_retries, 3);
        assert!(matches!(exec.strategy, ExecutionStrategy::Retry));
    }

    #[test]
    fn test_unknown_error_escalates() {
        let class = UncertaintyClass::from_error_category("unknown", "something went wrong");
        let exec = match class {
            UncertaintyClass::Executional(e) => e,
            _ => panic!("expected Executional"),
        };
        assert!(matches!(exec.strategy, ExecutionStrategy::Escalate));
    }
}
