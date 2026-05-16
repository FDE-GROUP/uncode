use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

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

        let popup_area = centered_rect(55, 45, area);
        f.render_widget(Clear, popup_area);

        let text = concat!(
            "\n",
            "  ╔══════════════════════════════════╗\n",
            "  ║        UnCode                    ║\n",
            "  ║   Agent Coding 系统              ║\n",
            "  ╚══════════════════════════════════╝\n",
            "\n",
            "  面向 前线部署工程师 (FDE) 开发\n",
            "\n",
            "  ───────────────────────────────\n",
            "  斜杠命令 / 查看所有可用命令\n",
            "  Ctrl+P 切换模型\n",
            "  Ctrl+C 退出\n",
            "\n",
            "  按 Enter 或 Esc 开始",
        );

        let p = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 欢迎使用 UnCode ")
                    .style(Style::default().fg(Color::Cyan)),
            )
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
