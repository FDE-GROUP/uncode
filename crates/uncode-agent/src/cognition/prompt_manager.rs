//! 提示词管理器 — 认知层的领域语言
//!
//! ## 认知显化与决策驱动设计中的定位
//!
//! 提示词是认知层的"领域语言"——它编码了"系统期望 LLM 理解什么"。
//! 决策层不接触提示词；语义防火墙保证自然语言止于认知层。
//!
//! 参见 `docs/ai-agent-archi/cognition-decision-driven-design.md` §3.3 认知层
//!
//! ## 与现有代码的关系
//!
//! `PromptManager` 包装 `crate::system_prompt::SystemPromptBuilder`，
//! 增加范式对齐的命名和文档，不重写逻辑。

use uncode_core::tool::ToolDefinition;

use crate::system_prompt::SystemPromptBuilder;

/// 提示词管理器 — 认知层的提示词编排
///
/// 职责：
/// - 构建系统提示词（base + tool guide + project context）
/// - 管理工具描述生成
/// - 角色配置
pub struct PromptManager {
    builder: SystemPromptBuilder,
}

impl PromptManager {
    pub fn new() -> Self {
        Self {
            builder: SystemPromptBuilder::new(),
        }
    }

    /// 设置基础系统提示词
    ///
    /// 通常来自 AGENTS.md / UNCODE.md 或内置模板。
    pub fn with_base(mut self, text: impl Into<String>) -> Self {
        self.builder = self.builder.base(text);
        self
    }

    /// 添加工具使用指南
    ///
    /// 基于当前 active_tools 生成 "## 可用工具" 章节。
    pub fn with_tool_guide(mut self, tools: &[ToolDefinition]) -> Self {
        self.builder = self.builder.add_tool_guide(tools);
        self
    }

    /// 添加项目上下文
    pub fn with_context(mut self, text: impl Into<String>) -> Self {
        self.builder = self.builder.base(text);
        self
    }

    /// 构建最终的系统提示词字符串
    pub fn build(self) -> String {
        self.builder.build()
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_prompt_manager() {
        let prompt = PromptManager::new().build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_prompt_with_base() {
        let prompt = PromptManager::new()
            .with_base("You are a coding assistant.")
            .build();
        assert!(prompt.contains("You are a coding assistant"));
    }

    #[test]
    fn test_prompt_with_tool_guide() {
        let tools = vec![
            ToolDefinition {
                name: "read".into(),
                description: "Read a file".into(),
                parameters: serde_json::json!({}),
                label: None,
                execution_mode: uncode_core::tool::ExecutionMode::Parallel,
            },
            ToolDefinition {
                name: "write".into(),
                description: "Write a file".into(),
                parameters: serde_json::json!({}),
                label: None,
                execution_mode: uncode_core::tool::ExecutionMode::Sequential,
            },
        ];
        let prompt = PromptManager::new().with_tool_guide(&tools).build();
        assert!(prompt.contains("## 可用工具"));
        assert!(prompt.contains("read"));
        assert!(prompt.contains("write"));
    }
}
