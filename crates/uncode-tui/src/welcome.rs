use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Stylize;
use ratatui::widgets::{Block, Clear, Paragraph};

pub struct WelcomeScreen {
    pub visible: bool,
}

impl WelcomeScreen {
    pub fn new() -> Self {
        Self { visible: true }
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_area = centered_rect(65, 55, area);
        f.render_widget(Clear, popup_area);

        let text = concat!(
            "\n",
            "\n",
            "      UnCode Agent Coding System         \n",
            "  [认知显化与决策驱动开发]新范式的最佳实践  \n",
            "\n",
            "\n",
            "  我们的哲学主张：人机协同创作是一个愿景从模糊认知显化、再到 Agent 工程化实现的过程。\n",
            "  决策的本质是模糊认知的显化，Agent Coding 不是 AI 替代，而是人与大模型的有机联动。\n",
            "\n",
            "\n",
            "  面向 前线部署工程师 (FDE) 开发            项目发起人：Abel TAN\n",
            "   \n",
            "\n",
            "\n",
            "  ───────────────────────────────\n",
            "  斜杠命令 / 查看所有可用命令\n",
            "  Ctrl+P 切换模型\n",
            "  Ctrl+C 退出\n",
            "\n",
            "  按 Enter 开始",
        );

        let p = Paragraph::new(text)
            .block(Block::bordered().title(" Wellcome to UnCodeNow ").cyan())
            .alignment(Alignment::Left);

        f.render_widget(p, popup_area);
    }
}

impl Default for WelcomeScreen {
    fn default() -> Self {
        Self::new()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let pw = r.width * percent_x / 100;
    let ph = r.height * percent_y / 100;
    let x = r.x + (r.width - pw) / 2;
    let y = r.y + (r.height - ph) / 2;
    Rect::new(x, y, pw, ph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_visible_shows_title() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let screen = WelcomeScreen::new();
        terminal
            .draw(|f| {
                screen.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("Wellcome to UnCodeNow"));
    }

    #[test]
    fn test_render_hidden_is_empty() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let screen = WelcomeScreen { visible: false };
        terminal
            .draw(|f| {
                screen.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.chars().all(|c| c == ' '));
    }

    #[test]
    fn test_new_is_visible_by_default() {
        let w = WelcomeScreen::new();
        assert!(w.is_visible());
    }

    #[test]
    fn test_hide() {
        let mut w = WelcomeScreen::new();
        w.hide();
        assert!(!w.is_visible());
    }

    #[test]
    fn test_centered_rect() {
        let r = Rect::new(10, 20, 200, 100);
        let cr = centered_rect(50, 40, r);
        assert_eq!(cr.width, 100);
        assert_eq!(cr.height, 40);
        assert_eq!(cr.x, 60);
        assert_eq!(cr.y, 50);
    }

    #[test]
    fn test_centered_rect_full() {
        let r = Rect::new(0, 0, 100, 50);
        let cr = centered_rect(100, 100, r);
        assert_eq!(cr.width, 100);
        assert_eq!(cr.height, 50);
        assert_eq!(cr.x, 0);
        assert_eq!(cr.y, 0);
    }
}
