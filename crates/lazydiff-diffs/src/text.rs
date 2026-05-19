use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::pierre::{selection_style, split_line_cell};
use crate::{
    DiffOverlayKind, DiffPaneTextLayout, DiffTheme, DiffVisualOverlay, InlineDiffSpan, RowKind,
    SyntaxSpan,
};

pub(crate) fn concealed_text(text: &str, conceal_first: bool) -> String {
    let _ = conceal_first;
    text.to_string()
}

pub(crate) fn render_full_line(area: Rect, y: u16, buf: &mut Buffer, text: &str, style: Style) {
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }
    buf.set_stringn(
        area.x,
        y,
        fit(text, area.width as usize),
        area.width as usize,
        style,
    );
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
    layout: DiffPaneTextLayout,
    overlays: &[DiffVisualOverlay],
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

    let text_x = area
        .x
        .saturating_add(layout.visual_code_start as u16)
        .min(right);

    render_styled_text(
        buf,
        text_x,
        y,
        right,
        text,
        syntax_spans,
        inline_spans,
        row_kind,
        theme,
        base_style,
        layout,
        overlays,
    );
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
    layout: DiffPaneTextLayout,
    overlays: &[DiffVisualOverlay],
) -> u16 {
    let selection_style = selection_style();
    let cursor_style = selection_style.add_modifier(ratatui::style::Modifier::BOLD);
    let search_style = Style::new().fg(theme.bg).bg(theme.add_fg);
    let mut display_column = 0usize;
    let cell = split_line_cell(
        row_kind,
        None,
        text,
        syntax_spans,
        inline_spans,
        theme,
        base_style,
    );

    for span in cell.spans {
        for ch in span.text.chars() {
            if x >= right {
                return x;
            }

            let char_width = ch.width().unwrap_or(0).max(1);
            let char_end = display_column + char_width;
            let doc_column = layout.document_code_start.saturating_add(display_column);
            let doc_char_end = layout.document_code_start.saturating_add(char_end);
            if char_end <= layout.scroll_x {
                display_column = char_end;
                continue;
            }
            let pane_column = layout.document_col_to_pane_col(doc_column);
            let pane_char_end = layout.document_col_to_pane_col(doc_char_end);
            let style = overlay_style_for(
                overlays,
                pane_column,
                pane_char_end,
                span.style,
                selection_style,
                search_style,
                cursor_style,
            );

            let mut buf_text = [0; 4];
            let text = ch.encode_utf8(&mut buf_text);
            let width = right.saturating_sub(x) as usize;
            buf.set_stringn(x, y, text, width, style);
            x = x.saturating_add(char_width as u16);
            display_column = char_end;
        }
    }

    for overlay in overlays
        .iter()
        .filter(|overlay| matches!(overlay.kind, DiffOverlayKind::Selection | DiffOverlayKind::Yank))
    {
        let doc_column = layout.document_code_start.saturating_add(display_column);
        let pane_column = layout.document_col_to_pane_col(doc_column);
        if overlay.range.end == usize::MAX && pane_column < overlay.range.end {
            let start = overlay.range.start.saturating_sub(pane_column) as u16;
            let selection_start_x = x.saturating_add(start).min(right);
            for selected_x in selection_start_x..right {
                buf[(selected_x, y)].set_style(selection_style);
            }
        }
    }

    x
}

fn overlay_style_for(
    overlays: &[DiffVisualOverlay],
    pane_column: usize,
    pane_char_end: usize,
    base: Style,
    selection: Style,
    search: Style,
    cursor: Style,
) -> Style {
    overlays
        .iter()
        .filter(|overlay| pane_column < overlay.range.end && pane_char_end > overlay.range.start)
        .fold(base, |_style, overlay| match overlay.kind {
            DiffOverlayKind::Selection | DiffOverlayKind::Yank => selection,
            DiffOverlayKind::Search => search,
            DiffOverlayKind::Cursor => cursor,
        })
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
