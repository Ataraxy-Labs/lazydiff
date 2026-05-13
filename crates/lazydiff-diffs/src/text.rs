use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::pierre::{selection_style, split_line_cell};
use crate::{DiffTheme, InlineDiffSpan, RowKind, SyntaxSpan, TextSelectionRange};

pub(crate) fn concealed_text(text: &str, conceal_first: bool) -> String {
    let _ = conceal_first;
    text.to_string()
}

pub(crate) fn render_full_line(area: Rect, y: u16, buf: &mut Buffer, text: &str, style: Style) {
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }
    buf.set_stringn(area.x, y, fit(text, area.width as usize), area.width as usize, style);
}

pub(crate) fn render_segments(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    prefix_segments: &[(&str, Style)],
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    row_kind: RowKind,
    theme: DiffTheme,
    base_style: Style,
    selection_range: Option<TextSelectionRange>,
) {
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(base_style);
    }

    let mut x = area.x;
    let right = area.right();
    for (text, style) in prefix_segments.iter() {
        if x >= right {
            break;
        }
        let width = right.saturating_sub(x) as usize;
        let fitted = fit(text, width);
        buf.set_stringn(x, y, &fitted, width, *style);
        x = x.saturating_add(UnicodeWidthStr::width(fitted.as_str()) as u16);
    }

    render_styled_text(buf, x, y, right, text, syntax_spans, inline_spans, row_kind, theme, base_style, selection_range);
}

fn render_styled_text(
    buf: &mut Buffer,
    mut x: u16,
    y: u16,
    right: u16,
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    row_kind: RowKind,
    theme: DiffTheme,
    base_style: Style,
    selection_range: Option<TextSelectionRange>,
) -> u16 {
    let selection_style = selection_style();
    let mut display_column = 0usize;
    let cell = split_line_cell(row_kind, None, text, syntax_spans, inline_spans, theme, base_style);

    for span in cell.spans {
        for ch in span.text.chars() {
            if x >= right {
                return x;
            }

            let char_width = ch.width().unwrap_or(0).max(1);
            let char_end = display_column + char_width;
            let selected = selection_range.is_some_and(|range| display_column < range.end && char_end > range.start);
            let style = if selected { selection_style } else { span.style };

            let mut buf_text = [0; 4];
            let text = ch.encode_utf8(&mut buf_text);
            let width = right.saturating_sub(x) as usize;
            buf.set_stringn(x, y, text, width, style);
            x = x.saturating_add(char_width as u16);
            display_column = char_end;
        }
    }

    if let Some(selection_range) = selection_range {
        if selection_range.end == usize::MAX && display_column < selection_range.end {
            let start = selection_range.start.saturating_sub(display_column) as u16;
            let selection_start_x = x.saturating_add(start).min(right);
            for selected_x in selection_start_x..right {
                buf[(selected_x, y)].set_style(selection_style);
            }
        }
    }

    x
}

fn fit(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        used += ch_width;
        out.push(ch);
    }
    out
}
