use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

pub struct OverlaySelector {
    pub visible: bool,
    title: String,
    items: Vec<String>,
    state: ListState,
}

impl OverlaySelector {
    pub fn new() -> Self {
        Self {
            visible: false,
            title: String::new(),
            items: Vec::new(),
            state: ListState::default(),
        }
    }

    pub fn show(&mut self, title: &str, items: Vec<String>) {
        self.title = title.to_string();
        self.items = items;
        self.state.select(Some(0));
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            self.state.select(Some((i + 1) % self.items.len()));
        }
    }

    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            let n = if i == 0 { self.items.len() - 1 } else { i - 1 };
            self.state.select(Some(n));
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_area = centered_rect(60, 40, area);
        f.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if self.state.selected() == Some(i) {
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                ListItem::new(item.as_str()).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(self.title.as_str()),
        );

        f.render_stateful_widget(list, popup_area, &mut self.state);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let pw = r.width * percent_x / 100;
    let ph = r.height * percent_y / 100;
    let x = r.x + (r.width - pw) / 2;
    let y = r.y + (r.height - ph) / 2;
    Rect::new(x, y, pw, ph)
}

impl Default for OverlaySelector {
    fn default() -> Self {
        Self::new()
    }
}
