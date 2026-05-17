use ratatui::style::{Color, Style};

use crate::theme::gutter_bg;
use crate::{DiffTheme, RowKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSign {
    Add,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GutterCell {
    pub line: Option<u32>,
    pub sign: Option<LineSign>,
    pub reserve_sign: bool,
}

pub(crate) struct SplitGutterSegments {
    pub rail: &'static str,
    pub rail_style: Style,
    pub line_number: String,
    pub sign: &'static str,
    pub trailing: &'static str,
    pub line_number_style: Style,
    pub sign_style: Style,
}

pub(crate) fn split_gutter_segments(
    cell: GutterCell,
    kind: RowKind,
    theme: DiffTheme,
    selected: bool,
) -> SplitGutterSegments {
    let gutter_bg = gutter_bg(kind, theme);
    let selected_bg = if selected { theme.selected } else { gutter_bg };
    let rail_fg = rail_color(kind, theme, selected);
    let rail = match kind {
        RowKind::Add | RowKind::Delete => "▎",
        RowKind::Context | RowKind::Empty => " ",
    };
    let rail_style = Style::new()
        .fg(rail_fg)
        .bg(if selected { theme.selected } else { gutter_bg });
    let number_fg = match kind {
        RowKind::Add => theme.add_fg,
        RowKind::Delete => theme.del_fg,
        RowKind::Context | RowKind::Empty => theme.line_number_fg,
    };
    let line_number_style = Style::new()
        .fg(if selected { Color::White } else { number_fg })
        .bg(selected_bg);
    let sign_style = line_number_style;
    let sign = "";

    SplitGutterSegments {
        rail,
        rail_style,
        line_number: exact_line_num(cell.line),
        sign,
        trailing: " ",
        line_number_style,
        sign_style,
    }
}

pub(crate) fn exact_line_num(value: Option<u32>) -> String {
    match value {
        None => "    ".to_string(),
        Some(line) => format!("{line:>4}"),
    }
}

pub(crate) fn rail_color(kind: RowKind, theme: DiffTheme, selected: bool) -> Color {
    let active = match kind {
        RowKind::Add => theme.add_fg,
        RowKind::Delete => theme.del_fg,
        RowKind::Context | RowKind::Empty => theme.line_number_fg,
    };
    if selected {
        active
    } else {
        blend(active, theme.panel, 0.35)
    }
}

fn blend(color: Color, toward: Color, amount: f32) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let Color::Rgb(tr, tg, tb) = toward else {
        return color;
    };
    let mix =
        |front: u8, back: u8| (back as f32 + (front as f32 - back as f32) * amount).round() as u8;
    Color::Rgb(mix(r, tr), mix(g, tg), mix(b, tb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_line_num_does_not_abbreviate_thousands() {
        assert_eq!(exact_line_num(Some(999)), " 999");
        assert_eq!(exact_line_num(Some(1000)), "1000");
        assert_eq!(exact_line_num(Some(12_345)), "12345");
    }
}
