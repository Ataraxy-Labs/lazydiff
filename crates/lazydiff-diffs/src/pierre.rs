use giallo::{FontStyle, HighlightOptions, Registry, ThemeVariant};
use ratatui::style::{Color, Modifier, Style};

use crate::{DiffTheme, InlineDiffSpan, RowKind, SyntaxHighlightKind, SyntaxSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCellKind {
    Context,
    Addition,
    Deletion,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSpan {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitLineCell {
    pub kind: RenderCellKind,
    pub sign: char,
    pub line_number: Option<u32>,
    pub spans: Vec<RenderSpan>,
}

pub fn split_line_cell(
    kind: RowKind,
    line_number: Option<u32>,
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    theme: DiffTheme,
    base_style: Style,
) -> SplitLineCell {
    SplitLineCell {
        kind: render_cell_kind(kind),
        sign: match kind {
            RowKind::Add => '+',
            RowKind::Delete => '-',
            RowKind::Context | RowKind::Empty => ' ',
        },
        line_number,
        spans: line_render_spans(text, syntax_spans, inline_spans, kind, theme, base_style),
    }
}

fn render_cell_kind(kind: RowKind) -> RenderCellKind {
    match kind {
        RowKind::Context => RenderCellKind::Context,
        RowKind::Add => RenderCellKind::Addition,
        RowKind::Delete => RenderCellKind::Deletion,
        RowKind::Empty => RenderCellKind::Empty,
    }
}

pub fn line_render_spans(
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    row_kind: RowKind,
    theme: DiffTheme,
    base_style: Style,
) -> Vec<RenderSpan> {
    let mut spans = Vec::new();
    let mut inline_index = 0usize;

    for (byte_index, ch) in text.char_indices() {
        while inline_index < inline_spans.len() && inline_spans[inline_index].end <= byte_index {
            inline_index += 1;
        }

        let mut style = if let Some(span) = active_syntax_span(syntax_spans, byte_index) {
            syntax_span_style(base_style, span, theme)
        } else {
            base_style
        };
        if inline_index < inline_spans.len()
            && inline_spans[inline_index].start <= byte_index
            && byte_index < inline_spans[inline_index].end
        {
            style = inline_diff_style(style, row_kind, theme);
        }

        push_span(&mut spans, ch, style);
    }

    spans
}

fn active_syntax_span(spans: &[SyntaxSpan], byte_index: usize) -> Option<&SyntaxSpan> {
    let end = spans.partition_point(|span| span.start <= byte_index);
    spans[..end].iter().rev().find(|span| byte_index < span.end)
}

fn push_span(spans: &mut Vec<RenderSpan>, ch: char, style: Style) {
    if let Some(previous) = spans.last_mut() {
        if previous.style == style {
            previous.text.push(ch);
            return;
        }
    }

    spans.push(RenderSpan {
        text: ch.to_string(),
        style,
    });
}

fn inline_diff_style(base_style: Style, row_kind: RowKind, theme: DiffTheme) -> Style {
    match row_kind {
        RowKind::Add => base_style
            .bg(theme.add_content_bg)
            .add_modifier(Modifier::BOLD),
        RowKind::Delete => base_style
            .bg(theme.del_content_bg)
            .add_modifier(Modifier::BOLD),
        RowKind::Context | RowKind::Empty => base_style,
    }
}

fn syntax_style(base_style: Style, kind: SyntaxHighlightKind, theme: DiffTheme) -> Style {
    match kind {
        SyntaxHighlightKind::Comment => base_style
            .fg(theme.syntax.comment)
            .add_modifier(Modifier::ITALIC),
        SyntaxHighlightKind::Keyword => base_style
            .fg(theme.syntax.keyword)
            .add_modifier(Modifier::BOLD),
        SyntaxHighlightKind::Punctuation => base_style.fg(theme.syntax.punctuation),
        SyntaxHighlightKind::String | SyntaxHighlightKind::Markup => {
            base_style.fg(theme.syntax.string)
        }
        SyntaxHighlightKind::Number | SyntaxHighlightKind::Boolean => base_style
            .fg(theme.syntax.number)
            .add_modifier(Modifier::BOLD),
        SyntaxHighlightKind::Function => base_style.fg(theme.syntax.function),
        SyntaxHighlightKind::Type => base_style.fg(theme.syntax.r#type),
        SyntaxHighlightKind::Property => base_style.fg(theme.syntax.property),
    }
}

fn syntax_span_style(base_style: Style, span: &SyntaxSpan, theme: DiffTheme) -> Style {
    if let Some(style) = span.style {
        let mut merged = base_style;
        if let Some(fg) = style.fg {
            merged = merged.fg(fg);
        }
        merged.add_modifier(style.add_modifier)
    } else {
        syntax_style(base_style, span.kind, theme)
    }
}

pub fn selection_style() -> Style {
    Style::new().fg(Color::White).bg(Color::Rgb(31, 75, 153))
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct MarkdownOverlayState {
    in_html_comment: bool,
}

pub(crate) fn apply_markdown_overlays(
    text: &str,
    spans: &mut Vec<SyntaxSpan>,
    state: &mut MarkdownOverlayState,
) {
    if text.is_empty() {
        return;
    }

    let trimmed = text.trim_start();
    let leading = text.len().saturating_sub(trimmed.len());
    if state.in_html_comment || trimmed.starts_with("<!--") {
        spans.push(styled_span(
            leading,
            text.len(),
            Style::new().fg(Color::Rgb(132, 132, 138)),
        ));
        state.in_html_comment = !trimmed.contains("-->");
        return;
    }

    if trimmed.starts_with('#') {
        let marker_len = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        let marker_end =
            leading + marker_len + usize::from(trimmed.as_bytes().get(marker_len) == Some(&b' '));
        spans.push(styled_span(
            leading,
            marker_end.min(text.len()),
            Style::new().fg(Color::Rgb(196, 208, 218)),
        ));
    } else if trimmed.starts_with('>') {
        spans.push(styled_span(
            leading,
            leading + 1,
            Style::new().fg(Color::Rgb(121, 121, 127)),
        ));
        if leading + 1 < text.len() {
            spans.push(styled_span(
                leading + 1,
                text.len(),
                Style::new().fg(Color::Rgb(132, 132, 138)),
            ));
        }
    } else if let Some(marker_len) = markdown_list_marker_len(trimmed) {
        spans.push(styled_span(
            leading,
            leading + marker_len,
            Style::new().fg(Color::Rgb(196, 208, 218)),
        ));
    }

    if push_markdown_reference_definition_overlay(text, spans) {
        return;
    }

    push_markdown_link_overlays(text, spans);
    push_markdown_code_overlays(text, spans);
}

pub(crate) fn sort_render_spans(spans: &mut [SyntaxSpan]) {
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| right.end.cmp(&left.end))
    });
}

pub(crate) struct PierreHighlighter {
    registry: Registry,
}

impl PierreHighlighter {
    pub(crate) fn new() -> Option<Self> {
        let mut registry = Registry::builtin().ok()?;
        let _ = registry.add_theme_from_path(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/themes/pierre-dark.json"
        ));
        registry.link_grammars();
        Some(Self { registry })
    }

    pub(crate) fn highlight_lines(
        &mut self,
        language: &str,
        source: &str,
    ) -> Option<Vec<Vec<SyntaxSpan>>> {
        if source.is_empty() {
            return Some(Vec::new());
        }

        let options = HighlightOptions::new(language, ThemeVariant::Single("pierre dark"))
            .fallback_to_plain(true)
            .merge_whitespace(false)
            .merge_same_style_tokens(true);
        let highlighted = self.registry.highlight(source, &options).ok()?;
        let mut lines = Vec::with_capacity(highlighted.tokens.len());
        let source_lines = source.split('\n');

        for (source_line, line) in source_lines.zip(highlighted.tokens) {
            let mut cursor = 0usize;
            let mut spans = Vec::with_capacity(line.len());
            for token in line {
                let ThemeVariant::Single(style) = token.style else {
                    continue;
                };
                let Some(style) = giallo_style_to_ratatui(style) else {
                    continue;
                };
                push_aligned_token_span(&mut spans, source_line, &mut cursor, &token.text, style);
            }
            lines.push(spans);
        }

        Some(lines)
    }
}

fn push_aligned_token_span(
    spans: &mut Vec<SyntaxSpan>,
    line: &str,
    cursor: &mut usize,
    token_text: &str,
    style: Style,
) {
    if token_text.is_empty() || *cursor > line.len() {
        return;
    }

    let Some(relative_start) = line[*cursor..].find(token_text) else {
        return;
    };
    let start = *cursor + relative_start;
    let end = start + token_text.len();
    if start >= end || end > line.len() {
        return;
    }

    spans.push(SyntaxSpan {
        start,
        end,
        kind: SyntaxHighlightKind::Property,
        style: Some(style),
    });
    *cursor = end;
}

pub(crate) fn language_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if matches_extension(&lower, &["c", "h"]) {
        "c"
    } else if matches_extension(&lower, &["cc", "cpp", "cxx", "hh", "hpp", "hxx"]) {
        "cpp"
    } else if matches_extension(&lower, &["js", "cjs", "mjs"]) {
        "javascript"
    } else if matches_extension(&lower, &["jsx"]) {
        "jsx"
    } else if matches_extension(&lower, &["md", "markdown"]) {
        "markdown"
    } else if matches_extension(&lower, &["ts", "cts", "mts"]) {
        "typescript"
    } else if matches_extension(&lower, &["tsx"]) {
        "tsx"
    } else if matches_extension(&lower, &["json", "jsonc", "json5"]) {
        "json"
    } else if matches_extension(&lower, &["yml", "yaml"]) {
        "yaml"
    } else if matches_extension(&lower, &["py", "pyw"]) {
        "python"
    } else if matches_extension(&lower, &["rs"]) {
        "rust"
    } else if matches_extension(&lower, &["toml"]) {
        "toml"
    } else {
        "plain"
    }
}

fn giallo_style_to_ratatui(style: giallo::Style) -> Option<Style> {
    let mut modifiers = Modifier::empty();
    if style.font_style.contains(FontStyle::BOLD) {
        modifiers |= Modifier::BOLD;
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        modifiers |= Modifier::ITALIC;
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        modifiers |= Modifier::UNDERLINED;
    }
    if style.font_style.contains(FontStyle::STRIKETHROUGH) {
        modifiers |= Modifier::CROSSED_OUT;
    }
    let foreground = normalize_pierre_token_color(style.foreground);
    if foreground == PIERRE_DEFAULT_FOREGROUND && modifiers.is_empty() {
        return None;
    }
    Some(Style::new().fg(foreground).add_modifier(modifiers))
}

const PIERRE_DEFAULT_FOREGROUND: Color = Color::Rgb(251, 251, 251);

fn giallo_color_to_ratatui(color: giallo::Color) -> Color {
    let hex = color.as_hex();
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::Rgb(r, g, b)
    } else {
        Color::White
    }
}

/// Tiny passthrough remap that preserves Pierre's original IDE palette but
/// nudges two specific token colors to read better against a dark diff body.
/// Theme-independent on purpose: the host app's theme toggle changes the
/// background and structural colors only — syntax tokens stay vibrant.
fn normalize_pierre_token_color(color: giallo::Color) -> Color {
    match color.as_hex().to_ascii_lowercase().as_str() {
        "#ff6762" => Color::Rgb(196, 208, 218),
        "#5ecc71" => Color::Rgb(216, 198, 239),
        _ => giallo_color_to_ratatui(color),
    }
}

fn styled_span(start: usize, end: usize, style: Style) -> SyntaxSpan {
    SyntaxSpan {
        start,
        end,
        kind: SyntaxHighlightKind::Property,
        style: Some(style),
    }
}

fn push_markdown_code_overlays(text: &str, spans: &mut Vec<SyntaxSpan>) {
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch != '`' {
            continue;
        }
        if let Some(open) = start.take() {
            spans.push(styled_span(
                open,
                open + 1,
                Style::new().fg(Color::Rgb(121, 121, 127)),
            ));
            if open + 1 < index {
                spans.push(styled_span(
                    open + 1,
                    index,
                    Style::new().fg(Color::Rgb(216, 198, 239)),
                ));
            }
            spans.push(styled_span(
                index,
                index + 1,
                Style::new().fg(Color::Rgb(121, 121, 127)),
            ));
        } else {
            start = Some(index);
        }
    }
}

fn push_markdown_link_overlays(text: &str, spans: &mut Vec<SyntaxSpan>) {
    let mut cursor = 0;
    while let Some(open_rel) = text[cursor..].find('[') {
        let open = cursor + open_rel;
        let Some(close_rel) = text[open..].find(']') else {
            break;
        };
        let close = open + close_rel;

        if open + 1 == close {
            spans.push(styled_span(
                open,
                close + 1,
                Style::new().fg(Color::Rgb(255, 212, 82)),
            ));
            cursor = close + 1;
            continue;
        }

        spans.push(styled_span(
            open,
            open + 1,
            Style::new().fg(Color::Rgb(121, 121, 127)),
        ));
        if !text[open + 1..close].contains('`') {
            spans.push(styled_span(
                open + 1,
                close,
                Style::new().fg(Color::Rgb(157, 106, 251)),
            ));
        }
        spans.push(styled_span(
            close,
            close + 1,
            Style::new().fg(Color::Rgb(121, 121, 127)),
        ));
        cursor = close + 1;
    }
}

fn push_markdown_reference_definition_overlay(text: &str, spans: &mut Vec<SyntaxSpan>) -> bool {
    let trimmed = text.trim_start();
    let leading = text.len().saturating_sub(trimmed.len());
    if !trimmed.starts_with('[') {
        return false;
    }

    let Some(close_rel) = trimmed.find("]:") else {
        return false;
    };
    let label_end = leading + close_rel + 1;
    let colon = label_end;
    spans.push(styled_span(
        leading,
        label_end,
        Style::new().fg(Color::Rgb(255, 212, 82)),
    ));
    spans.push(styled_span(
        colon,
        (colon + 1).min(text.len()),
        Style::new().fg(Color::Rgb(121, 121, 127)),
    ));

    let destination_start = (colon + 1).min(text.len());
    if destination_start < text.len() {
        spans.push(styled_span(
            destination_start,
            text.len(),
            Style::new().fg(Color::Rgb(255, 103, 141)),
        ));
    }
    true
}

fn matches_extension(path: &str, extensions: &[&str]) -> bool {
    extensions
        .iter()
        .any(|extension| path.ends_with(&format!(".{extension}")))
}

pub(crate) fn markdown_decoration_spans(text: &str) -> Vec<SyntaxSpan> {
    let mut spans = Vec::new();
    let trimmed = text.trim_start();
    let leading = text.len().saturating_sub(trimmed.len());

    if trimmed.starts_with('#') {
        spans.push(SyntaxSpan {
            start: leading,
            end: text.len(),
            kind: SyntaxHighlightKind::Markup,
            style: None,
        });
    } else if trimmed.starts_with("<!--") {
        spans.push(SyntaxSpan {
            start: leading,
            end: text.len(),
            kind: SyntaxHighlightKind::Comment,
            style: None,
        });
    } else if trimmed.starts_with('>') {
        spans.push(SyntaxSpan {
            start: leading,
            end: text.len(),
            kind: SyntaxHighlightKind::Comment,
            style: None,
        });
    } else if let Some(marker_len) = markdown_list_marker_len(trimmed) {
        spans.push(SyntaxSpan {
            start: leading,
            end: leading + marker_len,
            kind: SyntaxHighlightKind::Markup,
            style: None,
        });
    }

    push_delimited_spans(text, '`', SyntaxHighlightKind::String, &mut spans);
    push_markdown_link_spans(text, &mut spans);
    spans
}

fn markdown_list_marker_len(text: &str) -> Option<usize> {
    if text.starts_with("- ") || text.starts_with("* ") || text.starts_with("+ ") {
        return Some(1);
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && text[digits..].starts_with(". ") {
        Some(digits + 1)
    } else {
        None
    }
}

fn push_delimited_spans(
    text: &str,
    delimiter: char,
    kind: SyntaxHighlightKind,
    spans: &mut Vec<SyntaxSpan>,
) {
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch != delimiter {
            continue;
        }
        if let Some(open) = start.take() {
            spans.push(SyntaxSpan {
                start: open,
                end: index + delimiter.len_utf8(),
                kind,
                style: None,
            });
        } else {
            start = Some(index);
        }
    }
}

fn push_markdown_link_spans(text: &str, spans: &mut Vec<SyntaxSpan>) {
    let mut cursor = 0;
    while let Some(open_rel) = text[cursor..].find('[') {
        let open = cursor + open_rel;
        let Some(close_rel) = text[open..].find(']') else {
            break;
        };
        let close = open + close_rel + 1;
        spans.push(SyntaxSpan {
            start: open,
            end: close,
            kind: SyntaxHighlightKind::Number,
            style: None,
        });
        if text[close..].starts_with('(') {
            if let Some(dest_close_rel) = text[close..].find(')') {
                spans.push(SyntaxSpan {
                    start: close,
                    end: close + dest_close_rel + 1,
                    kind: SyntaxHighlightKind::String,
                    style: None,
                });
                cursor = close + dest_close_rel + 1;
                continue;
            }
        }
        cursor = close;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_python_extensions_to_python_grammar() {
        assert_eq!(language_for_path("script.py"), "python");
        assert_eq!(language_for_path("tools/launcher.PYW"), "python");
    }

    #[test]
    fn pierre_highlights_python_files() {
        let mut highlighter = PierreHighlighter::new().expect("built-in giallo registry loads");
        let spans = highlighter
            .highlight_lines(
                language_for_path("example.py"),
                "def greet(name):\n    return f'hi {name}'",
            )
            .expect("python grammar highlights");

        assert!(spans.iter().flatten().next().is_some());
    }

    #[test]
    fn pierre_does_not_emit_default_foreground_as_syntax() {
        let mut highlighter = PierreHighlighter::new().expect("built-in giallo registry loads");
        let spans = highlighter
            .highlight_lines(language_for_path("example.txt"), "plain text")
            .expect("plain fallback highlights");

        assert_eq!(spans, vec![Vec::new()]);
    }

    #[test]
    fn aligned_token_spans_leave_unhighlighted_gaps_without_shifting_later_tokens() {
        let mut spans = Vec::new();
        let mut cursor = 0;

        push_aligned_token_span(
            &mut spans,
            "alpha skipped beta",
            &mut cursor,
            "alpha",
            Style::new().fg(Color::Red),
        );
        push_aligned_token_span(
            &mut spans,
            "alpha skipped beta",
            &mut cursor,
            "beta",
            Style::new().fg(Color::Blue),
        );

        assert_eq!(spans[0].start..spans[0].end, 0..5);
        assert_eq!(spans[1].start..spans[1].end, 14..18);
    }

    #[test]
    fn aligned_token_spans_skip_unmatched_tokens_without_poisoning_cursor() {
        let mut spans = Vec::new();
        let mut cursor = 0;

        push_aligned_token_span(
            &mut spans,
            "alpha beta",
            &mut cursor,
            "alpha",
            Style::new().fg(Color::Red),
        );
        push_aligned_token_span(
            &mut spans,
            "alpha beta",
            &mut cursor,
            "missing",
            Style::new().fg(Color::Green),
        );
        push_aligned_token_span(
            &mut spans,
            "alpha beta",
            &mut cursor,
            "beta",
            Style::new().fg(Color::Blue),
        );

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].start..spans[0].end, 0..5);
        assert_eq!(spans[1].start..spans[1].end, 6..10);
    }

    #[test]
    fn inline_diff_background_preserves_syntax_foreground() {
        let theme = DiffTheme::default();
        let spans = line_render_spans(
            "Style::new()",
            &[SyntaxSpan {
                start: 0,
                end: 5,
                kind: SyntaxHighlightKind::Property,
                style: Some(Style::new().fg(Color::Magenta)),
            }],
            &[InlineDiffSpan { start: 0, end: 5 }],
            RowKind::Add,
            theme,
            Style::new().fg(Color::White).bg(theme.add_bg),
        );

        assert_eq!(spans[0].text, "Style");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
        assert_eq!(spans[0].style.bg, Some(theme.add_content_bg));
    }
}
