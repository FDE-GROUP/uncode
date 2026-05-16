/// 主题系统 — ~50 命名色，JSON 配置，热重载
use ratatui::style::Color;
use std::path::Path;

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
                Color::White,
                Color::DarkGray,
                Color::Blue,
                Color::Cyan,
                Color::Magenta,
                Color::Red,
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

    pub fn monokai() -> Self {
        Self {
            name: "monokai".into(),
            ui: UiColors {
                user_message: Color::Rgb(248, 248, 242),
                agent_text: Color::Rgb(230, 230, 220),
                input_border: Color::Rgb(248, 248, 242),
                footer_bg: Color::Rgb(39, 40, 34),
                footer_text: Color::Rgb(117, 113, 94),
                error_message: Color::Rgb(249, 38, 114),
                summary_card: Color::Rgb(102, 217, 239),
            },
            tool_status: ToolStatusColors {
                pending: Color::Rgb(117, 113, 94),
                running: Color::Rgb(102, 217, 239),
                success: Color::Rgb(166, 226, 46),
                failed: Color::Rgb(249, 38, 114),
                await_confirm: Color::Rgb(230, 219, 116),
            },
            diff: DiffColors {
                added_bg: Color::Rgb(39, 40, 34),
                added_text: Color::Rgb(166, 226, 46),
                removed_bg: Color::Rgb(39, 40, 34),
                removed_text: Color::Rgb(249, 38, 114),
                context: Color::Rgb(150, 148, 140),
                header: Color::Rgb(102, 217, 239),
            },
            thinking_level_border: [
                Color::Rgb(248, 248, 242),
                Color::Rgb(117, 113, 94),
                Color::Rgb(102, 217, 239),
                Color::Rgb(166, 226, 46),
                Color::Rgb(249, 38, 114),
                Color::Rgb(174, 129, 255),
            ],
            bash: BashColors {
                command: Color::Rgb(230, 219, 116),
                stdout: Color::Rgb(230, 230, 220),
                stderr: Color::Rgb(249, 38, 114),
            },
            markdown: MarkdownColors {
                heading: Color::Rgb(230, 219, 116),
                bold: Color::Rgb(248, 248, 242),
                italic: Color::Rgb(102, 217, 239),
                link: Color::Rgb(102, 217, 239),
                code_bg: Color::Rgb(55, 56, 48),
                code_text: Color::Rgb(248, 248, 242),
                code_block_border: Color::Rgb(73, 74, 66),
            },
            syntax: SyntaxColors {
                keyword: Color::Rgb(249, 38, 114),
                string: Color::Rgb(230, 219, 116),
                comment: Color::Rgb(117, 113, 94),
                number: Color::Rgb(174, 129, 255),
                type_name: Color::Rgb(102, 217, 239),
                function_name: Color::Rgb(166, 226, 46),
            },
        }
    }

    pub fn solarized() -> Self {
        Self {
            name: "solarized".into(),
            ui: UiColors {
                user_message: Color::Rgb(131, 148, 150),
                agent_text: Color::Rgb(147, 161, 161),
                input_border: Color::Rgb(147, 161, 161),
                footer_bg: Color::Rgb(0, 43, 54),
                footer_text: Color::Rgb(88, 110, 117),
                error_message: Color::Rgb(220, 50, 47),
                summary_card: Color::Rgb(38, 139, 210),
            },
            tool_status: ToolStatusColors {
                pending: Color::Rgb(88, 110, 117),
                running: Color::Rgb(38, 139, 210),
                success: Color::Rgb(133, 153, 0),
                failed: Color::Rgb(220, 50, 47),
                await_confirm: Color::Rgb(181, 137, 0),
            },
            diff: DiffColors {
                added_bg: Color::Rgb(0, 43, 54),
                added_text: Color::Rgb(133, 153, 0),
                removed_bg: Color::Rgb(0, 43, 54),
                removed_text: Color::Rgb(220, 50, 47),
                context: Color::Rgb(131, 148, 150),
                header: Color::Rgb(38, 139, 210),
            },
            thinking_level_border: [
                Color::Rgb(147, 161, 161),
                Color::Rgb(88, 110, 117),
                Color::Rgb(38, 139, 210),
                Color::Rgb(42, 161, 152),
                Color::Rgb(108, 113, 196),
                Color::Rgb(211, 54, 130),
            ],
            bash: BashColors {
                command: Color::Rgb(181, 137, 0),
                stdout: Color::Rgb(131, 148, 150),
                stderr: Color::Rgb(220, 50, 47),
            },
            markdown: MarkdownColors {
                heading: Color::Rgb(181, 137, 0),
                bold: Color::Rgb(147, 161, 161),
                italic: Color::Rgb(42, 161, 152),
                link: Color::Rgb(38, 139, 210),
                code_bg: Color::Rgb(7, 54, 66),
                code_text: Color::Rgb(147, 161, 161),
                code_block_border: Color::Rgb(88, 110, 117),
            },
            syntax: SyntaxColors {
                keyword: Color::Rgb(108, 113, 196),
                string: Color::Rgb(42, 161, 152),
                comment: Color::Rgb(88, 110, 117),
                number: Color::Rgb(211, 54, 130),
                type_name: Color::Rgb(181, 137, 0),
                function_name: Color::Rgb(133, 153, 0),
            },
        }
    }

    /// 按名称获取内置主题
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "default" | "dark" => Some(Self::default_dark()),
            "light" => Some(Self::light()),
            "monokai" => Some(Self::monokai()),
            "solarized" => Some(Self::solarized()),
            _ => None,
        }
    }

    /// 从 JSON 文件加载自定义主题（基于 default_dark 覆盖）
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let raw: serde_json::Value = serde_json::from_str(&content)?;
        let mut theme = Self::default_dark();

        if let Some(name) = raw.get("name").and_then(|v| v.as_str()) {
            theme.name = name.to_string();
        }
        theme.ui = parse_ui(&raw, &theme.ui);
        theme.tool_status = parse_tool_status(&raw, &theme.tool_status);
        theme.markdown = parse_markdown(&raw, &theme.markdown);
        theme.syntax = parse_syntax(&raw, &theme.syntax);
        theme.diff = parse_diff(&raw, &theme.diff);
        theme.bash = parse_bash(&raw, &theme.bash);

        if let Some(borders) = raw.get("thinking_level_border").and_then(|v| v.as_array()) {
            for (i, c) in borders.iter().enumerate().take(6) {
                if let Some(color) = parse_color(c) {
                    theme.thinking_level_border[i] = color;
                }
            }
        }

        Ok(theme)
    }

    /// 列出所有可用主题（内置 + 自定义）
    pub fn available_themes() -> Vec<String> {
        let mut themes = vec![
            "default".to_string(),
            "light".to_string(),
            "monokai".to_string(),
            "solarized".to_string(),
        ];
        if let Some(config_dir) = dirs::config_dir() {
            let themes_dir = config_dir.join("uncode").join("themes");
            if themes_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                    for entry in entries.flatten() {
                        if entry.path().extension().is_some_and(|e| e == "json") {
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
        }
        themes
    }

    /// 按名称加载主题（先查内置，再查文件）
    pub fn load_by_name(name: &str) -> Option<Self> {
        if let Some(t) = Self::builtin(name) {
            return Some(t);
        }
        // Try custom theme file
        let path = dirs::config_dir()?
            .join("uncode")
            .join("themes")
            .join(format!("{name}.json"));
        if path.exists() {
            Self::load_from_file(&path).ok()
        } else {
            None
        }
    }

    pub fn thinking_border_color(&self, level: usize) -> Color {
        self.thinking_level_border[level.min(5)]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}

// --- JSON parsing helpers ---

fn parse_color(val: &serde_json::Value) -> Option<Color> {
    match val {
        serde_json::Value::String(s) => parse_color_str(s),
        serde_json::Value::Array(arr) if arr.len() == 3 => {
            let r = arr.first()?.as_u64()? as u8;
            let g = arr.get(1)?.as_u64()? as u8;
            let b = arr.get(2)?.as_u64()? as u8;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

fn parse_color_str(s: &str) -> Option<Color> {
    // Named colors
    match s {
        "black" => return Some(Color::Black),
        "white" => return Some(Color::White),
        "red" => return Some(Color::Red),
        "green" => return Some(Color::Green),
        "yellow" => return Some(Color::Yellow),
        "blue" => return Some(Color::Blue),
        "magenta" => return Some(Color::Magenta),
        "cyan" => return Some(Color::Cyan),
        "gray" | "grey" => return Some(Color::Gray),
        "dark_gray" | "dark_grey" => return Some(Color::DarkGray),
        "light_red" => return Some(Color::LightRed),
        "light_green" => return Some(Color::LightGreen),
        "light_yellow" => return Some(Color::LightYellow),
        "light_blue" => return Some(Color::LightBlue),
        "light_magenta" => return Some(Color::LightMagenta),
        "light_cyan" => return Some(Color::LightCyan),
        _ => {}
    }
    // Hex colors: "#RRGGBB"
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    None
}

fn get_color(obj: &serde_json::Value, key: &str, default: Color) -> Color {
    obj.get(key).and_then(parse_color).unwrap_or(default)
}

fn parse_ui(raw: &serde_json::Value, defaults: &UiColors) -> UiColors {
    let Some(obj) = raw.get("ui") else {
        return defaults.clone();
    };
    UiColors {
        user_message: get_color(obj, "user_message", defaults.user_message),
        agent_text: get_color(obj, "agent_text", defaults.agent_text),
        input_border: get_color(obj, "input_border", defaults.input_border),
        footer_bg: get_color(obj, "footer_bg", defaults.footer_bg),
        footer_text: get_color(obj, "footer_text", defaults.footer_text),
        error_message: get_color(obj, "error_message", defaults.error_message),
        summary_card: get_color(obj, "summary_card", defaults.summary_card),
    }
}

fn parse_tool_status(raw: &serde_json::Value, defaults: &ToolStatusColors) -> ToolStatusColors {
    let Some(obj) = raw.get("tool_status") else {
        return defaults.clone();
    };
    ToolStatusColors {
        pending: get_color(obj, "pending", defaults.pending),
        running: get_color(obj, "running", defaults.running),
        success: get_color(obj, "success", defaults.success),
        failed: get_color(obj, "failed", defaults.failed),
        await_confirm: get_color(obj, "await_confirm", defaults.await_confirm),
    }
}

fn parse_markdown(raw: &serde_json::Value, defaults: &MarkdownColors) -> MarkdownColors {
    let Some(obj) = raw.get("markdown") else {
        return defaults.clone();
    };
    MarkdownColors {
        heading: get_color(obj, "heading", defaults.heading),
        bold: get_color(obj, "bold", defaults.bold),
        italic: get_color(obj, "italic", defaults.italic),
        link: get_color(obj, "link", defaults.link),
        code_bg: get_color(obj, "code_bg", defaults.code_bg),
        code_text: get_color(obj, "code_text", defaults.code_text),
        code_block_border: get_color(obj, "code_block_border", defaults.code_block_border),
    }
}

fn parse_syntax(raw: &serde_json::Value, defaults: &SyntaxColors) -> SyntaxColors {
    let Some(obj) = raw.get("syntax") else {
        return defaults.clone();
    };
    SyntaxColors {
        keyword: get_color(obj, "keyword", defaults.keyword),
        string: get_color(obj, "string", defaults.string),
        comment: get_color(obj, "comment", defaults.comment),
        number: get_color(obj, "number", defaults.number),
        type_name: get_color(obj, "type_name", defaults.type_name),
        function_name: get_color(obj, "function_name", defaults.function_name),
    }
}

fn parse_diff(raw: &serde_json::Value, defaults: &DiffColors) -> DiffColors {
    let Some(obj) = raw.get("diff") else {
        return defaults.clone();
    };
    DiffColors {
        added_bg: get_color(obj, "added_bg", defaults.added_bg),
        added_text: get_color(obj, "added_text", defaults.added_text),
        removed_bg: get_color(obj, "removed_bg", defaults.removed_bg),
        removed_text: get_color(obj, "removed_text", defaults.removed_text),
        context: get_color(obj, "context", defaults.context),
        header: get_color(obj, "header", defaults.header),
    }
}

fn parse_bash(raw: &serde_json::Value, defaults: &BashColors) -> BashColors {
    let Some(obj) = raw.get("bash") else {
        return defaults.clone();
    };
    BashColors {
        command: get_color(obj, "command", defaults.command),
        stdout: get_color(obj, "stdout", defaults.stdout),
        stderr: get_color(obj, "stderr", defaults.stderr),
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
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
        assert_eq!(theme.ui.user_message, Color::Black);
    }

    #[test]
    fn test_monokai_theme() {
        let theme = Theme::monokai();
        assert_eq!(theme.name, "monokai");
        assert!(matches!(theme.syntax.keyword, Color::Rgb(249, 38, 114)));
    }

    #[test]
    fn test_solarized_theme() {
        let theme = Theme::solarized();
        assert_eq!(theme.name, "solarized");
        assert!(matches!(theme.ui.footer_bg, Color::Rgb(0, 43, 54)));
    }

    #[test]
    fn test_builtin_lookup() {
        assert!(Theme::builtin("default").is_some());
        assert!(Theme::builtin("monokai").is_some());
        assert!(Theme::builtin("solarized").is_some());
        assert!(Theme::builtin("nonexistent").is_none());
    }

    #[test]
    fn test_available_themes() {
        let themes = Theme::available_themes();
        assert!(themes.contains(&"default".to_string()));
        assert!(themes.contains(&"light".to_string()));
        assert!(themes.contains(&"monokai".to_string()));
        assert!(themes.contains(&"solarized".to_string()));
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color_str("red"), Some(Color::Red));
        assert_eq!(parse_color_str("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color_str("dark_gray"), Some(Color::DarkGray));
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color_str("#ff6600"), Some(Color::Rgb(255, 102, 0)));
        assert_eq!(parse_color_str("#00ffcc"), Some(Color::Rgb(0, 255, 204)));
    }

    #[test]
    fn test_parse_color_array() {
        let val = serde_json::json!([255, 102, 0]);
        assert_eq!(parse_color(&val), Some(Color::Rgb(255, 102, 0)));
    }

    #[test]
    fn test_load_from_json() {
        let json = r##"{
            "name": "test",
            "ui": { "footer_text": "#ff0000" },
            "syntax": { "keyword": "yellow" }
        }"##;
        let dir = std::env::temp_dir().join("uncode_test_theme.json");
        std::fs::write(&dir, json).unwrap();
        let theme = Theme::load_from_file(&dir).unwrap();
        assert_eq!(theme.name, "test");
        assert!(matches!(theme.ui.footer_text, Color::Rgb(255, 0, 0)));
        assert_eq!(theme.syntax.keyword, Color::Yellow);
        // Unchanged defaults
        assert_eq!(theme.ui.user_message, Color::White);
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn test_thinking_border_color() {
        let theme = Theme::default_dark();
        assert_eq!(theme.thinking_border_color(0), Color::White);
        assert_eq!(theme.thinking_border_color(5), Color::Red);
        assert_eq!(theme.thinking_border_color(99), Color::Red);
    }
}
