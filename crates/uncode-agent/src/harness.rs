//! AgentHarness — 生产编排器（Pi 三层架构的最高层）
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
use uncode_core::api_types::ThinkingLevel;
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

/// AgentHarness — Pi 三层架构的最高层
///
/// 包装 AgentLoop，负责 session 持久化、compaction 触发、
/// Phase 守卫和运行时配置。
pub struct AgentHarness {
    agent: AgentLoop,
    phase: AgentHarnessPhase,
    pending_writes: Vec<PendingSessionWrite>,
    resources: HarnessResources,
    session_store: Arc<SessionStore>,
}

impl AgentHarness {
    pub fn new(agent: AgentLoop, session_store: Arc<SessionStore>) -> Self {
        Self {
            agent,
            phase: AgentHarnessPhase::Idle,
            pending_writes: Vec::new(),
            resources: HarnessResources {
                skills: SkillRegistry::load(),
                templates: TemplateStore::load(),
            },
            session_store,
        }
    }

    // ── Phase 守卫 ──

    fn enter_phase(&mut self, phase: AgentHarnessPhase) -> Result<(), UncodeError> {
        if self.phase != AgentHarnessPhase::Idle {
            return Err(UncodeError::Harness(HarnessError::busy(
                self.phase.to_string(),
            )));
        }
        self.phase = phase;
        Ok(())
    }

    fn exit_phase(&mut self) {
        self.phase = AgentHarnessPhase::Idle;
    }

    pub fn phase(&self) -> &AgentHarnessPhase {
        &self.phase
    }

    pub fn is_idle(&self) -> bool {
        self.phase == AgentHarnessPhase::Idle
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
        self.agent.cancel();
        let _ = self.agent.cancel_and_clear().await;
        self.pending_writes.clear();
        self.exit_phase();
    }

    // ── 运行时配置 ──

    /// 切换 LLM 模型（缓存在 pending_writes，turn 边界 flush）
    pub async fn set_model(&mut self, model_id: &str, provider: &str) {
        self.pending_writes.push(PendingSessionWrite::ModelChange {
            model_id: model_id.to_string(),
        });
        // 立即生效到 agent
        self.agent.set_model_id(model_id.to_string());
        // 持久化到 session
        if let Some(session_id) = self.agent.session_id() {
            if let Err(e) = model_switch::switch_model(
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
    }

    /// 设置 session ID
    pub fn set_session_id(&mut self, session_id: String) {
        self.agent.set_session_id(session_id);
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
