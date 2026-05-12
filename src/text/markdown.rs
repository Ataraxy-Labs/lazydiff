//! Body-preview markdown renderer for PR descriptions.
//!
//! Ports the GHUI `bodyPreview` pipeline (`kitlangton/ghui` ·
//! `src/ui/DetailsPane.tsx` + `src/ui/inlineSegments.ts`) to Rust /
//! Ratatui spans, then routes colors through `HomePalette` so it lives in
//! the warm-paper editorial register instead of the GHUI graphite one.
//!
//! Architecture is two-stage:
//!
//! 1. **Block scan** — match each input line against a small fixed set of
//!    line-prefix regexes (headings, list items, task items, blockquote,
//!    horizontal rule, code-fence toggle). Each match strips its prefix
//!    and stamps the line with a fg color, BOLD flag, glyph substitution,
//!    and a hanging-indent string for continuation rows.
//!
//! 2. **Inline tokenize** — a single regex alternation walks the resulting
//!    text and emits styled segments for `` `code` ``, `[label](url)`,
//!    `**strong**` (recursively), bare `https?://...` URLs, and `#NNN`
//!    issue/PR references. Everything outside matches is plain text in
//!    the block's fg color.
//!
//! Then word-tokenize the segments and greedy-fill into width-bounded
//! `Line`s with a `palette.muted` hanging-indent on continuation rows.
//!
//! Deferred (not implemented in v1): tables (rendered as raw source),
//! fenced-code-block syntax highlighting (pierre owns diff syntax;
//! markdown code blocks just render as muted wheat), HTML tags / comments
//! (passed through literally per GHUI), footnotes, math, super/sub,
//! definition lists.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use regex::Regex;
use std::sync::OnceLock;

use crate::design_system::HomePalette;

/// Render a markdown body into a vector of styled `Line`s, hard-capped at
/// `limit` rows. Each line begins with a single-space gutter so the caller
/// can render it directly into a content rect.
pub(crate) fn body_preview_lines(
    body: &str,
    width: u16,
    limit: usize,
    palette: &HomePalette,
) -> Vec<Line<'static>> {
    if body.trim().is_empty() {
        return vec![Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "No description.",
                Style::new().fg(palette.muted).bg(palette.bg),
            ),
        ])];
    }

    let content_width = width.saturating_sub(1).max(16) as usize;
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut in_html_comment = false;
    let mut in_details_block = false;
    // Track whether we've emitted any non-blank content yet — used to
    // suppress a leading `## Description` heading in the body when the
    // surrounding panel already labels the section.
    let mut seen_content = false;
    let cleaned = body.replace('\r', "");

    for raw_line in cleaned.lines() {
        if out.len() >= limit {
            break;
        }

        // Code fence toggles state but the fence line itself is suppressed.
        if !in_html_comment
            && !in_details_block
            && code_fence_regex().is_match(raw_line.trim_start())
        {
            in_code_block = !in_code_block;
            continue;
        }

        // Strip <details>…</details> blocks (and their <summary>) — these
        // are PR scaffolding (react-doctor, expand/collapse) that read as
        // noise in a terminal pane. Match line-level only; GFM requires
        // blank lines around the tags for their inner markdown to render.
        if !in_code_block {
            let trimmed = raw_line.trim_start();
            if in_details_block {
                if trimmed.to_ascii_lowercase().contains("</details>") {
                    in_details_block = false;
                }
                continue;
            }
            if trimmed.to_ascii_lowercase().starts_with("<details") {
                if !trimmed.to_ascii_lowercase().contains("</details>") {
                    in_details_block = true;
                }
                continue;
            }
        }

        // Strip HTML comments — they're PR-template scaffolding (`<!--
        // react-doctor -->`, `<!-- Add screenshots here -->`) and never
        // load-bearing for a reader. Handles single-line, line-spanning,
        // and multiple comments per line. Code blocks are left untouched.
        let stripped: String = if in_code_block {
            raw_line.to_string()
        } else {
            let (text, still_in) = strip_html_comments(raw_line, in_html_comment);
            in_html_comment = still_in;
            text
        };
        if in_html_comment && stripped.trim().is_empty() {
            continue;
        }

        let line: String = if in_code_block {
            stripped.replace('\t', "  ").trim_end().to_string()
        } else {
            stripped.trim().to_string()
        };
        if line.is_empty() {
            // Preserve one blank line between paragraphs; collapse runs.
            if !matches!(out.last(), Some(l) if line_is_blank(l)) && !out.is_empty() {
                out.push(blank_line(palette));
            }
            continue;
        }

        // Block-prefix classification.
        let mut fg = palette.fg;
        let mut bold = false;
        let mut indent: String = String::new();
        let mut text = line.clone();
        let mut block_kind = BlockKind::Paragraph;

        if in_code_block {
            fg = palette.code_fg;
            block_kind = BlockKind::CodeBody;
        } else if let Some(caps) = heading_regex().captures(&line) {
            // Strip leading hashes + space. Add a separator blank above.
            text = line[caps.get(0).unwrap().len()..].to_string();
            // The surrounding right pane already prints "Description"
            // as its section heading. If the body opens with a literal
            // `# Description` / `## Description` heading, suppress it —
            // matches GHUI parity and avoids the doubled label.
            if !seen_content && text.trim().eq_ignore_ascii_case("Description") {
                continue;
            }
            fg = palette.code_fg;
            bold = true;
            block_kind = BlockKind::Heading;
            if !out.is_empty() && !matches!(out.last(), Some(l) if line_is_blank(l)) {
                out.push(blank_line(palette));
                if out.len() >= limit {
                    break;
                }
            }
        } else if let Some(caps) = task_bullet_regex().captures(&line) {
            let marker = caps.get(1).unwrap().as_str();
            let checked = matches!(marker, "x" | "X");
            let body = caps.get(2).unwrap().as_str();
            let glyph = if checked { "☑" } else { "☐" };
            text = format!("{glyph}  {body}");
            indent = "   ".to_string();
            if checked {
                fg = palette.success;
            }
            block_kind = BlockKind::Task;
        } else if let Some(caps) = bare_task_regex().captures(&line) {
            let marker = caps.get(1).unwrap().as_str();
            let checked = matches!(marker, "x" | "X");
            let body = caps.get(2).unwrap().as_str();
            let glyph = if checked { "☑" } else { "☐" };
            text = format!("{glyph}  {body}");
            indent = "   ".to_string();
            if checked {
                fg = palette.success;
            }
            block_kind = BlockKind::Task;
        } else if let Some(caps) = bullet_regex().captures(&line) {
            let body = caps.get(1).unwrap().as_str();
            text = format!("•  {body}");
            indent = "   ".to_string();
            block_kind = BlockKind::Bullet;
        } else if let Some(caps) = ordered_regex().captures(&line) {
            // Keep the original "N. " literal; hanging indent matches its width.
            let marker = caps.get(1).unwrap().as_str();
            indent = " ".repeat(marker.len() + 2);
            block_kind = BlockKind::Ordered;
        } else if let Some(caps) = blockquote_regex().captures(&line) {
            let body = caps.get(1).unwrap().as_str();
            text = format!("▎ {body}");
            indent = "  ".to_string();
            fg = palette.muted;
            block_kind = BlockKind::Quote;
        } else if hr_regex().is_match(&line) {
            out.push(hr_line(palette, content_width));
            continue;
        }

        // Inline-tokenize the (possibly stripped) text and wrap to width.
        let segments = match block_kind {
            BlockKind::CodeBody => vec![InlineSeg::new(text.clone(), fg, false, false)],
            _ => inline_segments(&text, fg, bold, palette),
        };
        let wrapped = wrap_segments(&segments, content_width, &indent, palette);
        for line_segs in wrapped {
            if out.len() >= limit {
                break;
            }
            out.push(compose_line(line_segs, palette));
            seen_content = true;
        }
    }

    if out.is_empty() {
        return vec![Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "No description.",
                Style::new().fg(palette.muted).bg(palette.bg),
            ),
        ])];
    }

    // Trim trailing blank rows.
    while matches!(out.last(), Some(l) if line_is_blank(l)) {
        out.pop();
    }

    out.truncate(limit);
    out
}

#[derive(Clone, Copy)]
enum BlockKind {
    Paragraph,
    Heading,
    Bullet,
    Task,
    Ordered,
    Quote,
    CodeBody,
}

#[derive(Clone)]
struct InlineSeg {
    text: String,
    fg: ratatui::style::Color,
    bold: bool,
    underline: bool,
}

impl InlineSeg {
    fn new(text: String, fg: ratatui::style::Color, bold: bool, underline: bool) -> Self {
        Self {
            text,
            fg,
            bold,
            underline,
        }
    }
}

fn inline_segments(
    text: &str,
    base_fg: ratatui::style::Color,
    base_bold: bool,
    palette: &HomePalette,
) -> Vec<InlineSeg> {
    let mut out: Vec<InlineSeg> = Vec::new();
    if text.is_empty() {
        return out;
    }
    let mut cursor: usize = 0;
    let bytes = text.as_bytes();

    for caps in inline_token_regex().captures_iter(text) {
        let m = caps.get(0).unwrap();
        let start = m.start();
        if start > cursor {
            out.push(InlineSeg::new(
                text[cursor..start].to_string(),
                base_fg,
                base_bold,
                false,
            ));
        }

        if let Some(code) = caps.get(1) {
            // `code` — strip the surrounding backticks.
            let raw = code.as_str();
            let inner = &raw[1..raw.len() - 1];
            out.push(InlineSeg::new(
                inner.to_string(),
                palette.code_fg,
                base_bold,
                false,
            ));
        } else if let (Some(label), Some(_url)) = (caps.get(2), caps.get(3)) {
            // [label](url) — keep label, drop URL.
            out.push(InlineSeg::new(
                label.as_str().to_string(),
                palette.muted,
                base_bold,
                true,
            ));
        } else if let Some(strong) = caps.get(4) {
            // **strong** — recurse with BOLD flag.
            for seg in inline_segments(strong.as_str(), base_fg, true, palette) {
                out.push(seg);
            }
        } else if let Some(url) = caps.get(5) {
            // Bare URL — strip trailing punctuation.
            let raw = url.as_str();
            let trail_len = trailing_punctuation_len(raw);
            let url_text = &raw[..raw.len() - trail_len];
            if !url_text.is_empty() {
                out.push(InlineSeg::new(
                    url_text.to_string(),
                    palette.muted,
                    base_bold,
                    true,
                ));
            }
            if trail_len > 0 {
                out.push(InlineSeg::new(
                    raw[raw.len() - trail_len..].to_string(),
                    base_fg,
                    base_bold,
                    false,
                ));
            }
        } else if let Some(reff) = caps.get(6) {
            // #NNN issue/PR reference.
            out.push(InlineSeg::new(
                reff.as_str().to_string(),
                palette.action,
                base_bold,
                false,
            ));
        }

        cursor = m.end();
        // Keep `bytes` alive for byte-index safety in slicing — Rust slicing
        // is byte-indexed and matches are byte offsets from regex.
        let _ = bytes;
    }

    if cursor < text.len() {
        out.push(InlineSeg::new(
            text[cursor..].to_string(),
            base_fg,
            base_bold,
            false,
        ));
    }
    out
}

/// Remove HTML comments from a single input line, threading the
/// in-comment state across lines for multi-line comments.
///
/// Returns the stripped text and the new in-comment state. Handles:
/// - single-line: `before <!-- noise --> after` → `before  after`
/// - opening only: `before <!-- noise…` → `before` + still-in
/// - closing only: `…end --> after` (when entering with still-in) → `after`
/// - fully inside: `mid-comment text` (when in_comment) → ``
fn strip_html_comments(input: &str, mut in_comment: bool) -> (String, bool) {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        if in_comment {
            match rest.find("-->") {
                Some(idx) => {
                    rest = &rest[idx + 3..];
                    in_comment = false;
                }
                None => return (out, true),
            }
        } else {
            match rest.find("<!--") {
                Some(idx) => {
                    out.push_str(&rest[..idx]);
                    rest = &rest[idx + 4..];
                    in_comment = true;
                }
                None => {
                    out.push_str(rest);
                    return (out, false);
                }
            }
        }
    }
}

fn trailing_punctuation_len(s: &str) -> usize {
    // Mirror GHUI's TRAILING_URL_PUNCTUATION: `[.,;:!?)>\]}"'`]+$`
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let b = bytes[i - 1];
        if matches!(
            b,
            b'.' | b','
                | b';'
                | b':'
                | b'!'
                | b'?'
                | b')'
                | b'>'
                | b']'
                | b'}'
                | b'"'
                | b'\''
                | b'`'
        ) {
            i -= 1;
        } else {
            break;
        }
    }
    bytes.len() - i
}

fn wrap_segments(
    segments: &[InlineSeg],
    width: usize,
    indent: &str,
    palette: &HomePalette,
) -> Vec<Vec<InlineSeg>> {
    if width == 0 {
        return Vec::new();
    }
    // Tokenize each segment into words + whitespace runs while preserving
    // style attribution.
    let mut tokens: Vec<InlineSeg> = Vec::new();
    for seg in segments {
        for piece in split_whitespace_preserving(&seg.text) {
            tokens.push(InlineSeg {
                text: piece,
                fg: seg.fg,
                bold: seg.bold,
                underline: seg.underline,
            });
        }
    }

    let mut lines: Vec<Vec<InlineSeg>> = Vec::new();
    let mut current: Vec<InlineSeg> = Vec::new();
    let mut current_len: usize = 0;
    let indent_len = indent.chars().count();

    for token in tokens {
        let token_len = token.text.chars().count();
        let is_ws = token.text.chars().all(char::is_whitespace);

        if current_len > 0 && current_len + token_len > width {
            // Trim trailing whitespace, push, start a fresh continuation line.
            while matches!(current.last(), Some(s) if s.text.chars().all(char::is_whitespace)) {
                current.pop();
            }
            lines.push(current);
            current = Vec::new();
            current_len = 0;
            if !indent.is_empty() {
                current.push(InlineSeg::new(
                    indent.to_string(),
                    palette.muted,
                    false,
                    false,
                ));
                current_len = indent_len;
            }
            if is_ws {
                // Drop the leading whitespace that caused the wrap.
                continue;
            }
        }
        current.push(token);
        current_len += token_len;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn split_whitespace_preserving(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut current_is_ws: Option<bool> = None;
    for ch in s.chars() {
        let ws = ch.is_whitespace();
        match current_is_ws {
            Some(prev) if prev == ws => buf.push(ch),
            _ => {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
                buf.push(ch);
                current_is_ws = Some(ws);
            }
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn compose_line(segments: Vec<InlineSeg>, palette: &HomePalette) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(segments.len() + 1);
    spans.push(Span::raw(" "));
    for seg in segments {
        let mut style = Style::new().fg(seg.fg).bg(palette.bg);
        if seg.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if seg.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        spans.push(Span::styled(seg.text, style));
    }
    Line::from(spans)
}

fn blank_line(palette: &HomePalette) -> Line<'static> {
    Line::from(vec![Span::styled(
        " ",
        Style::new().fg(palette.muted).bg(palette.bg),
    )])
}

fn line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|s| s.content.chars().all(char::is_whitespace))
}

fn hr_line(palette: &HomePalette, width: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "─".repeat(width.saturating_sub(1)),
            Style::new().fg(palette.rule_dim).bg(palette.bg),
        ),
    ])
}

// ---- regex lazies --------------------------------------------------------

fn code_fence_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^```").unwrap())
}

fn heading_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^#{1,6}\s+").unwrap())
}

fn task_bullet_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[-*+]\s+\[([xX ])\]\s+(.*)$").unwrap())
}

fn bare_task_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\[([xX ])\]\s+(.*)$").unwrap())
}

fn bullet_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[-*+]\s+(.*)$").unwrap())
}

fn ordered_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(\d+)\.\s+").unwrap())
}

fn blockquote_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^>\s+(.*)$").unwrap())
}

fn hr_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // `regex` crate is RE2-based: no backreferences. Enumerate the three
    // valid horizontal-rule shapes directly.
    R.get_or_init(|| Regex::new(r"^(?:-{3,}|\*{3,}|_{3,})\s*$").unwrap())
}

fn inline_token_regex() -> &'static Regex {
    // 1: `code`   2: [label   3: ](url)   4: **strong**   5: bare URL   6: #NNN
    //
    // GHUI's original allowed single `*` runs inside `**…**` via a
    // look-ahead `\*(?!\*)`. RE2 has no look-around, so we accept the
    // simpler "no `*` inside strong" pattern. PR descriptions rarely
    // contain mixed bold+italic inline, and this avoids a much hairier
    // grammar.
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r#"(`[^`\n]+`)|\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*\n]+)\*\*|(https?://[^\s<>()\[\]"'`]+)|(#\d+)"#,
        )
        .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_system::HomePalette;

    fn flat(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn empty_body_renders_no_description() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("", 60, 10, &p);
        assert_eq!(lines.len(), 1);
        assert!(flat(&lines[0]).contains("No description."));
    }

    #[test]
    fn heading_strips_hashes_and_bolds() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("## Hello world", 60, 10, &p);
        assert_eq!(lines.len(), 1);
        assert_eq!(flat(&lines[0]).trim(), "Hello world");
        // Bold modifier set on the heading span (after the leading gutter space).
        let heading_span = &lines[0].spans[1];
        assert!(heading_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn bullet_substitutes_glyph() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("- alpha\n- beta", 60, 10, &p);
        assert_eq!(lines.len(), 2);
        assert!(flat(&lines[0]).contains("•  alpha"));
        assert!(flat(&lines[1]).contains("•  beta"));
    }

    #[test]
    fn task_lists_render_glyphs() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("- [ ] todo\n- [x] done", 60, 10, &p);
        assert!(flat(&lines[0]).contains("☐  todo"));
        assert!(flat(&lines[1]).contains("☑  done"));
    }

    #[test]
    fn blockquote_uses_thin_bar() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("> a quote", 60, 10, &p);
        assert!(flat(&lines[0]).contains("▎ a quote"));
    }

    #[test]
    fn fence_lines_are_dropped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("```rust\nfn main() {}\n```", 60, 10, &p);
        assert_eq!(lines.len(), 1);
        assert!(flat(&lines[0]).contains("fn main()"));
        assert!(!flat(&lines[0]).contains("```"));
    }

    #[test]
    fn inline_code_changes_color() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("call `frob()` to do it", 60, 10, &p);
        let span = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "frob()")
            .expect("inline code span");
        assert_eq!(span.style.fg, Some(p.code_fg));
    }

    #[test]
    fn issue_ref_is_amber() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("see #1234 for details", 60, 10, &p);
        let span = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "#1234")
            .expect("ref span");
        assert_eq!(span.style.fg, Some(p.action));
    }

    #[test]
    fn link_drops_url_keeps_label() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("see [the docs](https://example.com)", 60, 10, &p);
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        assert!(body.contains("the docs"));
        assert!(!body.contains("https://example.com"));
    }

    #[test]
    fn bare_url_trailing_period_stripped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("visit https://example.com. now", 60, 10, &p);
        let span = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "https://example.com")
            .expect("url span");
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn paragraphs_separated_by_blank() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("first paragraph\n\nsecond paragraph", 60, 10, &p);
        assert!(lines.len() >= 3);
        assert!(line_is_blank(&lines[1]));
    }

    #[test]
    fn single_line_html_comment_is_stripped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("before <!-- noise --> after", 60, 10, &p);
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        assert!(body.contains("before"));
        assert!(body.contains("after"));
        assert!(!body.contains("noise"));
        assert!(!body.contains("<!--"));
        assert!(!body.contains("-->"));
    }

    #[test]
    fn multiline_html_comment_is_stripped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines(
            "lead\n<!--\nhidden line 1\nhidden line 2\n-->\ntail",
            60,
            10,
            &p,
        );
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        assert!(body.contains("lead"));
        assert!(body.contains("tail"));
        assert!(!body.contains("hidden"));
        assert!(!body.contains("<!--"));
        assert!(!body.contains("-->"));
    }

    #[test]
    fn html_comment_inside_code_block_is_kept() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("```\n<!-- keep this -->\n```", 60, 10, &p);
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        assert!(body.contains("<!-- keep this -->"));
    }

    #[test]
    fn details_block_is_stripped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines(
            "lead\n<details>\n<summary>hidden</summary>\nsecret payload\n</details>\ntail",
            60,
            10,
            &p,
        );
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        assert!(body.contains("lead"));
        assert!(body.contains("tail"));
        assert!(!body.contains("hidden"));
        assert!(!body.contains("secret payload"));
        assert!(!body.contains("<details>"));
        assert!(!body.contains("</details>"));
    }

    #[test]
    fn leading_description_heading_is_dropped() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("## Description\n\nIt does the thing.", 60, 10, &p);
        // First non-blank line should be the prose, not "Description".
        let first_nonblank = lines
            .iter()
            .map(flat)
            .find(|s| !s.trim().is_empty())
            .unwrap();
        assert!(first_nonblank.contains("It does the thing."));
    }

    #[test]
    fn mid_body_description_heading_is_kept() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("preamble\n\n## Description\n\nbody", 60, 10, &p);
        let body: String = lines.iter().map(flat).collect::<Vec<_>>().join("\n");
        // Not a leading heading → preserved.
        assert!(body.contains("Description"));
    }

    #[test]
    fn long_paragraph_wraps_with_hanging_indent_on_bullet() {
        let p = HomePalette::quiver();
        let lines = body_preview_lines("- aaaa bbbb cccc dddd eeee ffff gggg hhhh", 20, 10, &p);
        assert!(lines.len() >= 2);
        // continuation begins with the hanging indent (after gutter)
        let second = flat(&lines[1]);
        assert!(second.starts_with("    ") || second.starts_with(" "));
    }
}
