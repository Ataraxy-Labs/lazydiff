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
    let rail_style = Style::new()
        .fg(rail_fg)
        .bg(if selected { theme.selected } else { theme.panel });
    let number_fg = match kind {
        RowKind::Add => theme.add_fg,
        RowKind::Delete => theme.del_fg,
        RowKind::Context | RowKind::Empty => theme.line_number_fg,
    };
    let line_number_style = Style::new()
        .fg(if selected { Color::White } else { number_fg })
        .bg(selected_bg);
    let sign_style = line_number_style;
    let sign = match cell.sign {
        Some(LineSign::Add) => " +",
        Some(LineSign::Delete) => " -",
        None if cell.reserve_sign => "  ",
        None => "",
    };

    SplitGutterSegments {
        rail: "▌",
        rail_style,
        line_number: format!("{:>4}", line_num(cell.line)),
        sign,
        trailing: " ",
        line_number_style,
        sign_style,
    }
}

pub(crate) fn line_num(value: Option<u32>) -> String {
    value.map(|line| line.to_string()).unwrap_or_default()
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
    let Color::Rgb(r, g, b) = color else { return color };
    let Color::Rgb(tr, tg, tb) = toward else { return color };
    let mix = |front: u8, back: u8| (back as f32 + (front as f32 - back as f32) * amount).round() as u8;
    Color::Rgb(mix(r, tr), mix(g, tg), mix(b, tb))
}
