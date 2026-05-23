//! 扩展侧工具渲染描述符 — JSON 序列化，桥接 WASM → TUI。
//!
//! 扩展通过 `ToolRenderConfig` 声明式描述工具渲染方式，
//! Host 端将其转为 `TemplateToolRenderer`（实现 `ToolRenderer` trait）。

use serde::{Deserialize, Serialize};

/// 扩展注册的工具渲染配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRenderConfig {
    /// 目标工具名（如 "my_custom_tool"）。
    pub tool_name: String,
    /// 调用摘要模板，支持 `{field}` 占位符替换。
    /// 例：`"→ {path}"` 或 `"Run {command} in {workdir}"`。
    #[serde(default)]
    pub call_template: String,
    /// 需要从工具 args JSON 中提取的字段列表（用于模板替换）。
    #[serde(default)]
    pub call_template_fields: Vec<String>,
    /// 结果渲染样式。
    #[serde(default)]
    pub result_style: ResultStyle,
    /// 结果最大显示行数。
    #[serde(default = "default_result_max_lines")]
    pub result_max_lines: usize,
}

fn default_result_max_lines() -> usize {
    20
}

/// 结果渲染样式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ResultStyle {
    /// 纯文本，使用 theme 代码色。
    #[default]
    Plain,
    /// 等宽字体 + 语法高亮（自动检测语言）。
    Code,
    /// Unified diff 着色（+/-/@@）。
    Diff,
    /// Bash stdout 样式。
    Bash,
}

impl ToolRenderConfig {
    /// 验证配置合法性。
    pub fn validate(&self) -> Result<(), String> {
        if self.tool_name.is_empty() {
            return Err("tool_name must not be empty".into());
        }
        if self.result_max_lines == 0 {
            return Err("result_max_lines must be > 0".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validate_ok() {
        let config = ToolRenderConfig {
            tool_name: "my_tool".into(),
            call_template: "→ {path}".into(),
            call_template_fields: vec!["path".into()],
            result_style: ResultStyle::Code,
            result_max_lines: 20,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_validate_empty_name() {
        let config = ToolRenderConfig {
            tool_name: String::new(),
            call_template: String::new(),
            call_template_fields: vec![],
            result_style: ResultStyle::Plain,
            result_max_lines: 20,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validate_zero_max_lines() {
        let config = ToolRenderConfig {
            tool_name: "tool".into(),
            call_template: String::new(),
            call_template_fields: vec![],
            result_style: ResultStyle::Plain,
            result_max_lines: 0,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_roundtrip() {
        let config = ToolRenderConfig {
            tool_name: "my_tool".into(),
            call_template: "→ {path}".into(),
            call_template_fields: vec!["path".into(), "command".into()],
            result_style: ResultStyle::Diff,
            result_max_lines: 30,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ToolRenderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn result_style_default_is_plain() {
        assert_eq!(ResultStyle::default(), ResultStyle::Plain);
    }
}
