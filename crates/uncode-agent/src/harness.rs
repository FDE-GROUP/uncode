//! AgentHarness — 决策层编排器
//!
//! ## 认知显化与决策驱动设计中的定位
//!
//! `AgentHarness` 是决策层的最高编排点，对应范式中的
//! "Harness = 决策层编排器"（参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §附注）。
//!
//! ## 与 decision/ 模块的映射
//!
//! | AgentHarness 职责 | 映射到的 decision/ 模块 |
//! |:---|:---|
//! | Phase 守卫 | `decision::adjudication::PhaseGuardPolicy` |
//! | MAX_TURNS 检查 | `decision::adjudication::TurnLimitPolicy` |
//! | CancellationToken 检查 | `decision::adjudication::CancellationPolicy` |
//! | active_run CAS | `decision::adjudication::ConcurrencyPolicy` |
//! | 权限策略 | `decision::firewall::PermissionPolicyRule` |
//! | 路径安全 | `decision::firewall::PathSafetyRule` |
//! | Schema 验证 | `decision::firewall::SchemaCoercionRule` |
//!
//! ## 演进方向
//!
//! 当前 AgentHarness 内联了部分决策逻辑。在后续重构中，
//! 这些逻辑将逐步委托给 `decision/` 模块的 Adjudicator 和 SemanticFirewall。
//!
//! 职责：
//! - Phase 守卫（Idle/Turn/Compaction/BranchSummary/Retry）
//! - Session 持久化（turn 边界 flush pending writes）
//! - Compaction 触发决策
//! - 运行时配置（model/thinkingLevel/tools 动态切换）
//! - 事件管理（SessionStart/End/CompactionComplete）

use std::sync::Arc;

use tracing::{debug, info};

use crate::session::store::SessionStore;
use uncode_core::api_types::{PayloadCallback, ResponseCallback, ThinkingLevel};
use uncode_core::error::{HarnessError, UncodeError};
use uncode_core::event::AgentEvent;
use uncode_core::message::Message;
use uncode_core::session::SessionEntry;
use uncode_core::skill::SkillRegistry;
use uncode_core::template::TemplateStore;

use crate::loop_engine::AgentLoop;
use crate::model_switch;

/// Harness 运行阶段
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentHarnessPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}

impl std::fmt::Display for AgentHarnessPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Turn => write!(f, "turn"),
            Self::Compaction => write!(f, "compaction"),
            Self::BranchSummary => write!(f, "branch_summary"),
            Self::Retry => write!(f, "retry"),
        }
    }
}

/// 缓存的 session 写入（在 turn 边界 flush）
pub enum PendingSessionWrite {
    ModelChange {
        model_id: String,
    },
    ThinkingLevelChange {
        level: ThinkingLevel,
    },
    Custom {
        entry: SessionEntry,
    },
    Label {
        target_id: String,
        label: Option<String>,
    },
}

/// Skills + Templates 容器
pub struct HarnessResources {
    pub skills: SkillRegistry,
    pub templates: TemplateStore,
}

/// AgentHarness — Pi 三层架构的最高层。
///
/// 包装 [`AgentLoop`]，负责 session 持久化、compaction 触发、
/// Phase 守卫和运行时配置。
///
/// **Pi:** 对应 `AgentHarness`（`before_agent_start`、`session_before_compact` 等 Hook 的子集由
/// `ToolHooks` / compaction 路径实现）。
pub struct AgentHarness {
    agent: AgentLoop,
    phase: AgentHarnessPhase,
    pending_writes: Vec<PendingSessionWrite>,
    resources: HarnessResources,
    session_store: Arc<SessionStore>,
    /// Phase 守卫策略 — 与 Adjudicator 共享，动态同步 phase (#385)
    phase_guard: std::sync::Arc<crate::decision::adjudication::PhaseGuardPolicy>,
}

impl AgentHarness {
    pub fn new(mut agent: AgentLoop, session_store: Arc<SessionStore>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();

        // 构建 PhaseGuardPolicy 并注入 Adjudicator (#385)
        let phase_guard = std::sync::Arc::new(
            crate::decision::adjudication::PhaseGuardPolicy::new(AgentHarnessPhase::Idle),
        );
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let adjudicator = crate::decision::adjudication::build_default_adjudicator(
            (*phase_guard).clone(),
            cancel_token,
            crate::loop_engine::MAX_TURNS as u32,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        );
        agent.set_adjudicator(adjudicator);

        Self {
            agent,
            phase: AgentHarnessPhase::Idle,
            pending_writes: Vec::new(),
            resources: HarnessResources {
                skills: SkillRegistry::load_with_project(&cwd),
                templates: TemplateStore::load(),
            },
            session_store,
            phase_guard,
        }
    }

    // ── Phase 守卫 ──

    fn enter_phase(&mut self, phase: AgentHarnessPhase) -> Result<(), UncodeError> {
        if self.phase != AgentHarnessPhase::Idle {
            return Err(UncodeError::Harness(HarnessError::busy(
                self.phase.to_string(),
            )));
        }
        self.phase = phase.clone();
        self.phase_guard.set_phase(phase);
        Ok(())
    }

    fn exit_phase(&mut self) {
        self.phase = AgentHarnessPhase::Idle;
        self.phase_guard.set_phase(AgentHarnessPhase::Idle);
    }

    pub fn phase(&self) -> &AgentHarnessPhase {
        &self.phase
    }

    // ── 决策层集成（认知显化与决策驱动设计）──

    /// 构建完整的决策管线：防火墙 → 裁决器 → 执行编排器
    ///
    /// 组合 `decision/` 模块中的 firewall、adjudication、execution 组件。
    /// 当前用于验证新增决策层组件可与现有 AgentLoop 并行工作；
    /// 后续 refactor 中 AgentLoop 的工具执行将逐步委托给此管线。
    ///
    /// 参见 `docs/uncode-technologies/UNCODE_DECISION_LAYER.md`
    pub fn build_decision_adjudicator(
        &self,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> crate::decision::adjudication::Adjudicator {
        use crate::decision::adjudication::{PhaseGuardPolicy, build_default_adjudicator};
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let phase_policy = PhaseGuardPolicy::new(self.phase.clone());

        build_default_adjudicator(
            phase_policy,
            cancel_token,
            crate::loop_engine::MAX_TURNS as u32,
            Arc::new(AtomicBool::new(true)),
        )
    }

    /// 构建语义防火墙（包装现有 PermissionPolicy）
    pub fn build_firewall(
        &self,
        _cancel_token: tokio_util::sync::CancellationToken,
        tool_registry: std::sync::Arc<crate::tools::ToolRegistry>,
    ) -> crate::decision::firewall::SemanticFirewall {
        use crate::decision::firewall::build_default_firewall;
        use crate::tool_permission::PermissionPolicy;
        use std::sync::Arc;

        let policy = Arc::new(PermissionPolicy::default_policy());

        build_default_firewall(
            policy,
            tool_registry,
            std::env::current_dir().unwrap_or_default(),
        )
    }

    pub fn is_idle(&self) -> bool {
        self.phase == AgentHarnessPhase::Idle
    }

    // ── 决策反馈桥（认知显化与决策驱动设计 原则5）──

    /// 将工具执行结果通过反馈桥回流到认知层
    ///
    /// 原则 5：事件流是双向通道。
    /// 每次工具执行完成后调用此方法，将结果转化为
    /// ActionObservation → AgentStep → WorkingMemory 反馈。
    ///
    /// 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3
    pub fn bridge_execution_to_cognition(
        &self,
        _result: &crate::decision::execution::ExecutionResult,
        _turn_number: u32,
        _active_tools: &[String],
        _context_tokens: usize,
    ) {
        // 反馈闭环已通过 AgentLoop 内的 TurnFeedback → WorkingMemory → EpisodeMemory 实现 (#385)
        // 此方法保留以兼容外部调用方
    }

    // ── 核心方法 ──

    /// 开始新 turn（带 Phase 守卫）
    pub async fn prompt(&mut self, user_message: Message) -> Result<Vec<Message>, UncodeError> {
        self.enter_phase(AgentHarnessPhase::Turn)?;
        let result = self.agent.run(user_message).await;
        self.flush_pending_writes().await;
        self.exit_phase();
        result
    }

    /// 注入 steering 消息
    pub async fn steer(&self, msg: Message) {
        self.agent.steer(msg).await;
    }

    /// 排队 follow-up 消息
    pub async fn follow_up(&self, msg: Message) {
        self.agent.follow_up(msg).await;
    }

    /// 排队 nextTurn 消息
    pub async fn next_turn(&self, msg: Message) {
        self.agent.next_turn(msg).await;
    }

    /// 中断 + 清空队列
    pub async fn abort(&mut self) {
        self.agent.fire_session_shutdown("quit").await;
        self.agent.cancel();
        let _ = self.agent.cancel_and_clear().await;
        self.pending_writes.clear();
        self.exit_phase();
    }

    // ── 运行时配置 ──

    /// Restrict tools visible to the LLM (**Pi:** `setActiveTools`).
    pub fn set_active_tools(&self, names: &[impl AsRef<str>]) -> Result<(), String> {
        self.agent.set_active_tools(names)
    }

    /// 切换 LLM 模型（缓存在 pending_writes，turn 边界 flush）
    pub async fn set_model(&mut self, model_id: &str, provider: &str) {
        self.pending_writes.push(PendingSessionWrite::ModelChange {
            model_id: model_id.to_string(),
        });
        // 立即生效到 agent
        self.agent.set_model_id(model_id.to_string());
        // 持久化到 session
        if let Some(session_id) = self.agent.session_id()
            && let Err(e) = model_switch::switch_model(
                &mut model_id.to_string(),
                model_id,
                provider,
                &self.session_store,
                Some(session_id),
            )
            .await
        {
            debug!("model switch persist skipped: {e}");
        }
    }

    /// 设置 session ID
    pub fn set_session_id(&mut self, session_id: String) {
        self.agent.set_session_id(session_id);
    }

    /// 注册 LLM 请求体观测回调（经 `stream_simple` → provider 触发）。
    pub fn set_on_payload(&mut self, cb: PayloadCallback) {
        self.agent.set_on_payload(cb);
    }

    /// 注册 LLM HTTP 响应观测回调。
    pub fn set_on_response(&mut self, cb: ResponseCallback) {
        self.agent.set_on_response(cb);
    }

    /// 发送 LLM 前变换消息（与 Pi `transformContext` 同层）。
    pub fn set_transform_context(&mut self, cb: uncode_core::api_types::TransformContextCallback) {
        self.agent.set_transform_context(cb);
    }

    /// 获取当前 session ID
    pub fn session_id(&self) -> Option<&str> {
        self.agent.session_id()
    }

    /// 订阅事件
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent> {
        self.agent.subscribe()
    }

    /// 获取事件 sender
    pub fn event_sender(&self) -> tokio::sync::broadcast::Sender<AgentEvent> {
        self.agent.event_sender()
    }

    /// 获取 resources 引用
    pub fn resources(&self) -> &HarnessResources {
        &self.resources
    }

    /// 更新 skills + templates，发射 resources_update 事件
    pub fn set_resources(&mut self, skills: SkillRegistry, templates: TemplateStore) {
        self.resources.skills = skills;
        self.resources.templates = templates;
        tracing::info!("resources updated");
    }

    // ── 内部方法 ──

    async fn flush_pending_writes(&mut self) {
        if self.pending_writes.is_empty() {
            return;
        }
        if let Some(session_id) = self.agent.session_id() {
            for write in self.pending_writes.drain(..) {
                let entry = match write {
                    PendingSessionWrite::ModelChange { model_id } => {
                        info!("flushing model change: {model_id}");
                        continue; // model_switch 已在 set_model 中持久化
                    }
                    PendingSessionWrite::ThinkingLevelChange { level } => {
                        SessionEntry::ThinkingLevelChange(Box::new(
                            uncode_core::session::ThinkingLevelChangeEntry {
                                id: uncode_core::session::generate_entry_id(),
                                parent_id: None,
                                timestamp: chrono::Utc::now(),
                                thinking_level: level,
                            },
                        ))
                    }
                    PendingSessionWrite::Custom { entry } => entry,
                    PendingSessionWrite::Label { target_id, label } => {
                        SessionEntry::Label(Box::new(uncode_core::session::LabelEntry {
                            id: uncode_core::session::generate_entry_id(),
                            parent_id: None,
                            timestamp: chrono::Utc::now(),
                            target_id,
                            label,
                        }))
                    }
                };
                if let Err(e) = self.session_store.append_entry(session_id, &entry).await {
                    debug!("flush pending write skipped: {e}");
                }
            }
        } else {
            self.pending_writes.clear();
        }
    }
}
