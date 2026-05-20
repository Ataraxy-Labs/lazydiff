use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
};

pub(crate) fn short_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn right_aligned_text(width: u16, used: usize, text: &str) -> String {
    let gap = (width as usize).saturating_sub(used + text.chars().count());
    format!("{}{}", " ".repeat(gap), text)
}

pub(crate) fn render_home_rule(frame: &mut Frame, area: Rect, y: u16, style: Style) {
    if y < area.bottom() {
        frame.render_widget(
            Line::from("─".repeat(area.width as usize)).style(style),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

pub(crate) fn draw_horizontal_rule(
    buf: &mut ratatui::buffer::Buffer,
    y: u16,
    left: u16,
    right: u16,
    fg: Color,
    bg: Color,
) {
    for x in left..right {
        buf[(x, y)]
            .set_symbol("─")
            .set_style(Style::new().fg(fg).bg(bg));
    }
}

pub(crate) fn draw_vertical_rule(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    top: u16,
    bottom: u16,
    fg: Color,
    bg: Color,
) {
    for y in top..bottom {
        buf[(x, y)]
            .set_symbol("│")
            .set_style(Style::new().fg(fg).bg(bg));
    }
}

pub(crate) fn set_symbol(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    symbol: &str,
    style: Style,
) {
    if x < buf.area.right() && y < buf.area.bottom() {
        buf[(x, y)].set_symbol(symbol).set_style(style);
    }
}

pub(crate) fn contains_point(area: Rect, column: u16, row: u16) -> bool {
    column >= area.left() && column < area.right() && row >= area.top() && row < area.bottom()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListRowKind {
    Header,
    Item(usize),
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListRowGeometry {
    pub(crate) kind: ListRowKind,
    pub(crate) area: Rect,
}

#[derive(Debug, Clone)]
pub(crate) struct ListGeometryBuilder {
    area: Rect,
    y: u16,
    rows: Vec<ListRowGeometry>,
}

impl ListGeometryBuilder {
    pub(crate) fn new(area: Rect, start_y: u16) -> Self {
        Self {
            area,
            y: start_y.max(area.y),
            rows: Vec::new(),
        }
    }

    pub(crate) fn gap(&mut self) -> Option<ListRowGeometry> {
        self.push(ListRowKind::Gap)
    }

    pub(crate) fn header(&mut self) -> Option<ListRowGeometry> {
        self.push(ListRowKind::Header)
    }

    pub(crate) fn item(&mut self, index: usize) -> Option<ListRowGeometry> {
        self.push(ListRowKind::Item(index))
    }

    fn push(&mut self, kind: ListRowKind) -> Option<ListRowGeometry> {
        if self.y >= self.area.bottom() {
            return None;
        }
        let row = ListRowGeometry {
            kind,
            area: Rect::new(self.area.x, self.y, self.area.width, 1),
        };
        self.rows.push(row);
        self.y = self.y.saturating_add(1);
        Some(row)
    }
}

pub(crate) fn list_row_at(
    rows: &[ListRowGeometry],
    column: u16,
    row: u16,
) -> Option<ListRowGeometry> {
    rows.iter()
        .copied()
        .find(|entry| contains_point(entry.area, column, row))
}

pub(crate) fn list_item_rows(
    area: Rect,
    start_index: usize,
    total_len: usize,
) -> Vec<ListRowGeometry> {
    let mut rows = Vec::new();
    let visible = area.height as usize;
    for visual_index in 0..visible {
        let index = start_index.saturating_add(visual_index);
        if index >= total_len {
            break;
        }
        rows.push(ListRowGeometry {
            kind: ListRowKind::Item(index),
            area: Rect::new(area.x, area.y + visual_index as u16, area.width, 1),
        });
    }
    rows
}

pub(crate) fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() + 1 >= width {
            if text.chars().count() > width {
                out.push('…');
            }
            return out;
        }
        out.push(ch);
    }
    out
}

pub(crate) fn truncate_middle(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".repeat(width);
    }
    let head = width / 2;
    let tail = width.saturating_sub(head + 1);
    let prefix: String = text.chars().take(head).collect();
    let suffix: String = text.chars().skip(len.saturating_sub(tail)).collect();
    format!("{prefix}…{suffix}")
}

pub(crate) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4)).max(1);
    let height = height.min(area.height.saturating_sub(4)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub(crate) fn fill_rect(buf: &mut ratatui::buffer::Buffer, area: Rect, symbol: &str, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_symbol(symbol).set_style(style);
        }
    }
}

pub(crate) fn draw_box(buf: &mut ratatui::buffer::Buffer, area: Rect, style: Style) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let left = area.left();
    let right = area.right().saturating_sub(1);
    let top = area.top();
    let bottom = area.bottom().saturating_sub(1);
    buf[(left, top)].set_symbol("╭").set_style(style);
    buf[(right, top)].set_symbol("╮").set_style(style);
    buf[(left, bottom)].set_symbol("╰").set_style(style);
    buf[(right, bottom)].set_symbol("╯").set_style(style);
    for x in left.saturating_add(1)..right {
        buf[(x, top)].set_symbol("─").set_style(style);
        buf[(x, bottom)].set_symbol("─").set_style(style);
    }
    for y in top.saturating_add(1)..bottom {
        buf[(left, y)].set_symbol("│").set_style(style);
        buf[(right, y)].set_symbol("│").set_style(style);
    }
}
