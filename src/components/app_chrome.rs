use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

use crate::HomePalette;

pub(crate) struct AppHeader<'a> {
    pub(crate) brand: &'a str,
    /// Project + branch scope, e.g. "quiver/quiver · main". Empty when
    /// the TUI is launched outside a known project.
    pub(crate) scope: &'a str,
    pub(crate) viewer: &'a str,
    pub(crate) summary: &'a str,
    pub(crate) is_fetching: bool,
    pub(crate) spinner: &'a str,
    pub(crate) palette: HomePalette,
}

impl AppHeader<'_> {
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let bg = self.palette.bg;
        let brand = Style::new()
            .fg(self.palette.accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let viewer = Style::new()
            .fg(self.palette.fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let scope = Style::new().fg(self.palette.fg).bg(bg);
        let muted = Style::new().fg(self.palette.muted).bg(bg);
        let spinner = Style::new().fg(self.palette.action).bg(bg);

        // Build left side (brand · scope).
        let mut left: Vec<Span<'_>> = vec![Span::styled(format!(" {}", self.brand), brand)];
        let mut left_width = 1 + self.brand.chars().count();
        if !self.scope.is_empty() {
            left.push(Span::styled(" · ".to_string(), muted));
            left.push(Span::styled(self.scope.to_string(), scope));
            left_width += 3 + self.scope.chars().count();
        }

        // Build right side (spinner · summary). Do not render the
        // GitHub viewer name here; the top bar can be screenshotted or
        // streamed during reviews.
        let mut right: Vec<Span<'_>> = Vec::new();
        let mut right_width = 0usize;
        let _ = viewer;
        let _ = self.viewer;
        if self.is_fetching {
            right.push(Span::styled(format!("{} ", self.spinner), spinner));
            right_width += self.spinner.chars().count() + 1;
        }
        right.push(Span::styled(self.summary.to_string(), muted));
        right_width += self.summary.chars().count();
        right.push(Span::styled(" ".to_string(), muted));
        right_width += 1;

        let total = area.width as usize;
        let pad = total.saturating_sub(left_width).saturating_sub(right_width);
        let mut spans = left;
        spans.push(Span::styled(" ".repeat(pad), muted));
        spans.extend(right);

        frame.render_widget(Line::from(spans), area);
    }
}
