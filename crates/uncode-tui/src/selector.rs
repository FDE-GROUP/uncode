use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState};

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

    pub fn selected_item(&self) -> Option<&str> {
        self.state
            .selected()
            .and_then(|i| self.items.get(i).map(|s| s.as_str()))
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
                    Style::default().yellow().on_dark_gray()
                } else {
                    Style::default()
                };
                ListItem::new(item.as_str()).style(style)
            })
            .collect();

        let list = List::new(items).block(Block::bordered().title(self.title.as_str()));

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_visible_shows_items() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut selector = OverlaySelector::new();
        selector.show("title", vec!["item1".into(), "item2".into()]);
        terminal
            .draw(|f| {
                selector.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("title"));
        assert!(text.contains("item1"));
    }

    #[test]
    fn test_render_hidden_is_empty() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut selector = OverlaySelector::new();
        terminal
            .draw(|f| {
                selector.render(f, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.chars().all(|c| c == ' '));
    }

    #[test]
    fn test_new_not_visible() {
        let s = OverlaySelector::new();
        assert!(!s.is_visible());
        assert!(s.selected_item().is_none());
    }

    #[test]
    fn test_show_makes_visible() {
        let mut s = OverlaySelector::new();
        s.show("Choose", vec!["a".into(), "b".into()]);
        assert!(s.is_visible());
        assert_eq!(s.selected_item(), Some("a"));
    }

    #[test]
    fn test_next_cycles_forward() {
        let mut s = OverlaySelector::new();
        s.show("Test", vec!["x".into(), "y".into(), "z".into()]);
        assert_eq!(s.selected_item(), Some("x"));
        s.next();
        assert_eq!(s.selected_item(), Some("y"));
        s.next();
        assert_eq!(s.selected_item(), Some("z"));
        s.next();
        assert_eq!(s.selected_item(), Some("x")); // wraps around
    }

    #[test]
    fn test_prev_cycles_backward() {
        let mut s = OverlaySelector::new();
        s.show("Test", vec!["a".into(), "b".into(), "c".into()]);
        s.prev();
        assert_eq!(s.selected_item(), Some("c")); // wraps to last
        s.prev();
        assert_eq!(s.selected_item(), Some("b"));
    }

    #[test]
    fn test_next_prev_empty_items() {
        let mut s = OverlaySelector::new();
        s.show("Empty", vec![]);
        s.next(); // should not panic
        s.prev(); // should not panic
        assert!(s.selected_item().is_none());
    }

    #[test]
    fn test_hide() {
        let mut s = OverlaySelector::new();
        s.show("Test", vec!["a".into()]);
        s.hide();
        assert!(!s.is_visible());
    }

    #[test]
    fn test_centered_rect() {
        let r = Rect::new(0, 0, 100, 100);
        let cr = centered_rect(50, 30, r);
        assert_eq!(cr.width, 50);
        assert_eq!(cr.height, 30);
        assert_eq!(cr.x, 25);
        assert_eq!(cr.y, 35);
    }

    #[test]
    fn test_centered_rect_zero_area() {
        let r = Rect::new(0, 0, 0, 0);
        let cr = centered_rect(50, 30, r);
        assert_eq!(cr.width, 0);
        assert_eq!(cr.height, 0);
    }
}
