/// 主题系统 — ~50 命名色，JSON 配置，热重载
///
/// 结构化颜色分组：核心 UI、工具状态、Diff、Markdown、语法高亮、思考级别边框、Bash
use ratatui::style::Color;
use std::path::Path;

/// 主题定义
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub ui: UiColors,
    pub tool_status: ToolStatusColors,
    pub diff: DiffColors,
    pub thinking_level_border: [Color; 6],
    pub bash: BashColors,
    pub markdown: MarkdownColors,
    pub syntax: SyntaxColors,
}

#[derive(Debug, Clone)]
pub struct UiColors {
    pub user_message: Color,
    pub agent_text: Color,
    pub input_border: Color,
    pub footer_bg: Color,
    pub footer_text: Color,
    pub error_message: Color,
    pub summary_card: Color,
}

#[derive(Debug, Clone)]
pub struct ToolStatusColors {
    pub pending: Color,
    pub running: Color,
    pub success: Color,
    pub failed: Color,
    pub await_confirm: Color,
}

#[derive(Debug, Clone)]
pub struct DiffColors {
    pub added_bg: Color,
    pub added_text: Color,
    pub removed_bg: Color,
    pub removed_text: Color,
    pub context: Color,
    pub header: Color,
}

#[derive(Debug, Clone)]
pub struct BashColors {
    pub command: Color,
    pub stdout: Color,
    pub stderr: Color,
}

#[derive(Debug, Clone)]
pub struct MarkdownColors {
    pub heading: Color,
    pub bold: Color,
    pub italic: Color,
    pub link: Color,
    pub code_bg: Color,
    pub code_text: Color,
    pub code_block_border: Color,
}

#[derive(Debug, Clone)]
pub struct SyntaxColors {
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
    pub type_name: Color,
    pub function_name: Color,
}

impl Theme {
    pub fn default_dark() -> Self {
        Self {
            name: "default".into(),
            ui: UiColors {
                user_message: Color::White,
                agent_text: Color::Gray,
                input_border: Color::White,
                footer_bg: Color::Black,
                footer_text: Color::DarkGray,
                error_message: Color::Red,
                summary_card: Color::Blue,
            },
            tool_status: ToolStatusColors {
                pending: Color::DarkGray,
                running: Color::Cyan,
                success: Color::Green,
                failed: Color::Red,
                await_confirm: Color::Yellow,
            },
            diff: DiffColors {
                added_bg: Color::Black,
                added_text: Color::Green,
                removed_bg: Color::Black,
                removed_text: Color::Red,
                context: Color::Gray,
                header: Color::Cyan,
            },
            thinking_level_border: [
                Color::White,    // Off
                Color::DarkGray, // Minimal
                Color::Blue,     // Low
                Color::Cyan,     // Medium
                Color::Magenta,  // High
                Color::Red,      // XHigh
            ],
            bash: BashColors {
                command: Color::Yellow,
                stdout: Color::Gray,
                stderr: Color::Red,
            },
            markdown: MarkdownColors {
                heading: Color::Yellow,
                bold: Color::White,
                italic: Color::Cyan,
                link: Color::Blue,
                code_bg: Color::DarkGray,
                code_text: Color::Cyan,
                code_block_border: Color::DarkGray,
            },
            syntax: SyntaxColors {
                keyword: Color::Magenta,
                string: Color::Green,
                comment: Color::DarkGray,
                number: Color::Yellow,
                type_name: Color::Cyan,
                function_name: Color::Blue,
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".into(),
            ui: UiColors {
                user_message: Color::Black,
                agent_text: Color::DarkGray,
                input_border: Color::Black,
                footer_bg: Color::White,
                footer_text: Color::Gray,
                error_message: Color::Red,
                summary_card: Color::Blue,
            },
            tool_status: ToolStatusColors {
                pending: Color::Gray,
                running: Color::Blue,
                success: Color::Green,
                failed: Color::Red,
                await_confirm: Color::Yellow,
            },
            diff: DiffColors {
                added_bg: Color::White,
                added_text: Color::Green,
                removed_bg: Color::White,
                removed_text: Color::Red,
                context: Color::DarkGray,
                header: Color::Blue,
            },
            thinking_level_border: [
                Color::Black,
                Color::Gray,
                Color::Blue,
                Color::Cyan,
                Color::Magenta,
                Color::Red,
            ],
            bash: BashColors {
                command: Color::Yellow,
                stdout: Color::DarkGray,
                stderr: Color::Red,
            },
            markdown: MarkdownColors {
                heading: Color::Blue,
                bold: Color::Black,
                italic: Color::DarkGray,
                link: Color::Blue,
                code_bg: Color::Gray,
                code_text: Color::Black,
                code_block_border: Color::Gray,
            },
            syntax: SyntaxColors {
                keyword: Color::Magenta,
                string: Color::Green,
                comment: Color::Gray,
                number: Color::Yellow,
                type_name: Color::Blue,
                function_name: Color::Cyan,
            },
        }
    }

    /// 获取思考级别对应的边框颜色
    pub fn thinking_border_color(&self, level: usize) -> Color {
        self.thinking_level_border[level.min(5)]
    }

    /// 从 JSON 文件加载主题
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let raw: serde_json::Value = serde_json::from_str(&content)?;
        let theme = Self::default_dark();
        // Parse JSON overrides on top of defaults
        let mut theme = theme;
        if let Some(name) = raw.get("name").and_then(|v| v.as_str()) {
            theme.name = name.to_string();
        }
        Ok(theme)
    }

    /// 列出可用主题
    pub fn available_themes() -> Vec<String> {
        let mut themes = vec!["default".to_string(), "light".to_string()];
        // Check for custom themes in ~/.config/uncode/themes/
        if let Some(config_dir) = dirs::config_dir() {
            let themes_dir = config_dir.join("uncode").join("themes");
            if themes_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.path().file_stem() {
                            let name = name.to_string_lossy().to_string();
                            if !themes.contains(&name) {
                                themes.push(name);
                            }
                        }
                    }
                }
            }
        }
        themes
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme() {
        let theme = Theme::default_dark();
        assert_eq!(theme.name, "default");
        assert_eq!(theme.ui.user_message, Color::White);
        assert_eq!(theme.thinking_level_border.len(), 6);
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
        assert_eq!(theme.ui.user_message, Color::Black);
    }

    #[test]
    fn test_thinking_border_color() {
        let theme = Theme::default_dark();
        assert_eq!(theme.thinking_border_color(0), Color::White);
        assert_eq!(theme.thinking_border_color(5), Color::Red);
        assert_eq!(theme.thinking_border_color(99), Color::Red); // clamped
    }

    #[test]
    fn test_available_themes() {
        let themes = Theme::available_themes();
        assert!(themes.contains(&"default".to_string()));
        assert!(themes.contains(&"light".to_string()));
    }
}
