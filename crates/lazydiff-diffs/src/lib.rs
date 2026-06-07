use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod gutter;
mod inline_diff;
mod metadata;
mod pierre;
mod scrollbar;
mod selection;
mod text;
mod theme;
mod viewer;
pub use gutter::{GutterCell, LineSign};
pub use inline_diff::InlineDiffSpan;
use inline_diff::compute_inline_diff_spans;
pub use metadata::{FileDiffKind, FileDiffMetadata, HunkContent, HunkMetadata};
pub use pierre::{RenderCellKind, RenderSpan, SplitLineCell, line_render_spans};
pub use scrollbar::{SliderGeometry, SliderState, VerticalScrollbar, render_scrollbar};
pub use selection::{
    DiffSearchMatch, DiffSelectionMode, DiffTextPoint, DiffTextSelection, TextPoint, TextSelection,
    TextSelectionRange, TextViewport,
};
use text::{concealed_text, render_full_line, render_segments};
use theme::row_style;
pub use theme::{DiffTheme, DiffThemeName, SyntaxTheme};
pub use viewer::{
    DiffCursor, DiffInlineBlock, DiffInlineBlockAccent, DiffInlineBlockKind, DiffRenderModel,
    DiffScreenPosition, DiffSearchState, DiffSideBySideRow, DiffViewerState, DiffViewport,
    DiffVisualRow, DiffWordMotion,
};

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "carriage-return",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "constructor.builtin",
    "embedded",
    "error",
    "escape",
    "function",
    "function.builtin",
    "keyword",
    "markup",
    "markup.bold",
    "markup.heading",
    "markup.italic",
    "markup.link",
    "markup.link.url",
    "markup.list",
    "markup.list.checked",
    "markup.list.numbered",
    "markup.list.unchecked",
    "markup.list.unnumbered",
    "markup.quote",
    "markup.raw",
    "markup.raw.block",
    "markup.raw.inline",
    "markup.strikethrough",
    "module",
    "number",
    "operator",
    "property",
    "property.builtin",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "string.regexp",
    "string.special",
    "string.special.symbol",
    "tag",
    "text.emphasis",
    "text.literal",
    "text.reference",
    "text.strong",
    "text.title",
    "text.uri",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
];
#[derive(Debug, Clone, Default)]
pub struct DiffDocument {
    pub files: Vec<FileDiff>,
    unified_rows: Vec<RowRef>,
    split_rows: Vec<RowRef>,
}

/// Shared coordinate contract for diff text.
///
/// - Document columns are semantic text columns anchored at the start of the
///   line's selectable code text.
/// - Visual pane columns are terminal-cell columns inside the rendered pane and
///   include gutter/rail decoration plus horizontal scroll.
/// - `document_code_start` is the semantic code offset used by selection,
///   search, yank, cursor, and copy operations.
/// - `visual_code_start` is the rendered terminal-cell start of code text.
/// - Cursor, mouse, search, selection, yank, and renderer paint conversions must
///   go through this layout. Do not use `row_code_start` directly as a screen-x
///   coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffPaneTextLayout {
    pub mode: DiffMode,
    pub side: DiffSide,
    pub row: usize,
    pub pane: Rect,
    pub document_code_start: usize,
    pub visual_code_start: usize,
    pub scroll_x: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOverlayKind {
    Selection,
    Search,
    Yank,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffVisualOverlay {
    pub row: usize,
    pub side: DiffSide,
    pub range: TextSelectionRange,
    pub kind: DiffOverlayKind,
}

impl DiffPaneTextLayout {
    pub fn document_col_to_pane_col(&self, document_col: usize) -> usize {
        self.visual_code_start
            .saturating_add(document_col.saturating_sub(self.document_code_start))
            .saturating_sub(self.scroll_x)
    }

    pub fn document_col_to_screen_x(&self, document_col: usize) -> u16 {
        self.pane
            .x
            .saturating_add(self.document_col_to_pane_col(document_col) as u16)
            .min(self.pane.right().saturating_sub(1))
    }

    pub fn screen_x_to_document_col(&self, x: u16, document_code_end: usize) -> usize {
        let pane_col = x.saturating_sub(self.pane.x) as usize;
        let text_col = pane_col
            .saturating_sub(self.visual_code_start)
            .saturating_add(self.scroll_x);
        self.document_code_start
            .saturating_add(text_col)
            .max(self.document_code_start)
            .min(document_code_end.saturating_sub(1))
    }

    pub fn scroll_to_reveal(&self, document_col: usize) -> usize {
        let text_col = document_col.saturating_sub(self.document_code_start);
        if text_col < self.scroll_x {
            return text_col;
        }
        let visible_text_width = usize::from(self.pane.width)
            .saturating_sub(self.visual_code_start)
            .max(1);
        if text_col >= self.scroll_x.saturating_add(visible_text_width) {
            text_col.saturating_sub(visible_text_width.saturating_sub(1))
        } else {
            self.scroll_x
        }
    }

    pub fn selection_range_to_visual(self, range: TextSelectionRange) -> TextSelectionRange {
        TextSelectionRange {
            start: self.document_col_to_pane_col(range.start),
            end: if range.end == usize::MAX {
                usize::MAX
            } else {
                self.document_col_to_pane_col(range.end)
            },
        }
    }

    pub fn text_local_range_to_document(self, range: TextSelectionRange) -> TextSelectionRange {
        TextSelectionRange {
            start: self.document_code_start.saturating_add(range.start),
            end: self.document_code_start.saturating_add(range.end),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
    old_source_lines: Option<Vec<String>>,
    new_source_lines: Option<Vec<String>>,
    old_source_syntax_spans: Option<Vec<Vec<SyntaxSpan>>>,
    new_source_syntax_spans: Option<Vec<Vec<SyntaxSpan>>>,
    expanded_gaps: HashSet<usize>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxHighlightKind {
    Comment,
    Keyword,
    String,
    Number,
    Boolean,
    Function,
    Type,
    Property,
    Punctuation,
    Markup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub start: usize,
    pub end: usize,
    pub kind: SyntaxHighlightKind,
    pub style: Option<Style>,
}

#[derive(Debug, Clone, Default)]
pub struct HighlightStats {
    pub files_highlighted: usize,
    pub sides_highlighted: usize,
    pub spans: usize,
}

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context {
        old_line: u32,
        new_line: u32,
        text: String,
        syntax_spans: Vec<SyntaxSpan>,
    },
    Add {
        new_line: u32,
        text: String,
        syntax_spans: Vec<SyntaxSpan>,
        inline_spans: Vec<InlineDiffSpan>,
    },
    Delete {
        old_line: u32,
        text: String,
        syntax_spans: Vec<SyntaxSpan>,
        inline_spans: Vec<InlineDiffSpan>,
    },
}

#[derive(Debug, Clone, Copy)]
enum RowRef {
    FileSeparator,
    FileHeader {
        file_index: usize,
    },
    Collapsed {
        file_index: usize,
        hunk_index: usize,
        count: u32,
    },
    ExpandedContext {
        file_index: usize,
        old_line: u32,
        new_line: u32,
    },
    HunkHeader {
        file_index: usize,
        hunk_index: usize,
    },
    Unified {
        line: LineRef,
    },
    Split {
        left: Option<LineRef>,
        right: Option<LineRef>,
        left_reserve_sign: bool,
        right_reserve_sign: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct LineRef {
    file_index: usize,
    hunk_index: usize,
    line_index: usize,
}

impl DiffDocument {
    fn rows(&self, mode: DiffMode) -> &[RowRef] {
        match mode {
            DiffMode::Unified => &self.unified_rows,
            DiffMode::Split => &self.split_rows,
        }
    }

    fn rebuild_row_cache(&mut self) {
        self.unified_rows.clear();
        self.split_rows.clear();

        for file_index in 0..self.files.len() {
            if file_index > 0 {
                self.unified_rows.push(RowRef::FileSeparator);
                self.split_rows.push(RowRef::FileSeparator);
            }
            self.unified_rows.push(RowRef::FileHeader { file_index });
            self.split_rows.push(RowRef::FileHeader { file_index });

            let file = &self.files[file_index];
            let left_reserve_sign = true;
            let right_reserve_sign = true;
            let mut previous_old_end = 0u32;
            let mut previous_new_end = 0u32;

            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                let gap_expanded = file.expanded_gaps.contains(&hunk_index);
                let collapsed_before = hunk.old_start.saturating_sub(previous_old_end + 1);
                if collapsed_before > 0 {
                    let old_start = previous_old_end.saturating_add(1);
                    let new_start = previous_new_end.saturating_add(1);
                    if gap_expanded {
                        for offset in 0..collapsed_before {
                            let row = RowRef::ExpandedContext {
                                file_index,
                                old_line: old_start.saturating_add(offset),
                                new_line: new_start.saturating_add(offset),
                            };
                            self.unified_rows.push(row);
                            self.split_rows.push(row);
                        }
                    } else {
                        self.unified_rows.push(RowRef::Collapsed {
                            file_index,
                            hunk_index,
                            count: collapsed_before,
                        });
                        self.split_rows.push(RowRef::Collapsed {
                            file_index,
                            hunk_index,
                            count: collapsed_before,
                        });
                    }
                }

                let skip_leading_context = if gap_expanded {
                    hunk.leading_context_count()
                } else {
                    0
                };

                if !gap_expanded {
                    self.unified_rows.push(RowRef::HunkHeader {
                        file_index,
                        hunk_index,
                    });
                    self.split_rows.push(RowRef::HunkHeader {
                        file_index,
                        hunk_index,
                    });
                }

                for line_index in skip_leading_context..hunk.lines.len() {
                    let line = LineRef {
                        file_index,
                        hunk_index,
                        line_index,
                    };
                    self.unified_rows.push(RowRef::Unified { line });
                }

                let mut line_index = skip_leading_context;
                while line_index < hunk.lines.len() {
                    match &hunk.lines[line_index] {
                        DiffLine::Context { .. } => {
                            let line = LineRef {
                                file_index,
                                hunk_index,
                                line_index,
                            };
                            self.split_rows.push(RowRef::Split {
                                left: Some(line),
                                right: Some(line),
                                left_reserve_sign,
                                right_reserve_sign,
                            });
                            line_index += 1;
                        }
                        DiffLine::Delete { .. } | DiffLine::Add { .. } => {
                            let mut deletes = Vec::new();
                            let mut adds = Vec::new();
                            while line_index < hunk.lines.len() {
                                let line = LineRef {
                                    file_index,
                                    hunk_index,
                                    line_index,
                                };
                                match &hunk.lines[line_index] {
                                    DiffLine::Delete { .. } => deletes.push(line),
                                    DiffLine::Add { .. } => adds.push(line),
                                    DiffLine::Context { .. } => break,
                                }
                                line_index += 1;
                            }

                            let max_len = deletes.len().max(adds.len());
                            for index in 0..max_len {
                                self.split_rows.push(RowRef::Split {
                                    left: deletes.get(index).copied(),
                                    right: adds.get(index).copied(),
                                    left_reserve_sign,
                                    right_reserve_sign,
                                });
                            }
                        }
                    }
                }

                previous_old_end = hunk.old_start + hunk.old_line_count().saturating_sub(1);
                previous_new_end = hunk.new_start + hunk.new_line_count().saturating_sub(1);
            }
        }
    }

    fn line(&self, line_ref: LineRef) -> &DiffLine {
        &self.files[line_ref.file_index].hunks[line_ref.hunk_index].lines[line_ref.line_index]
    }
}

impl FileDiff {
    pub fn new(old_path: Option<String>, new_path: String, hunks: Vec<Hunk>) -> Self {
        Self {
            old_path,
            new_path,
            hunks,
            old_source_lines: None,
            new_source_lines: None,
            old_source_syntax_spans: None,
            new_source_syntax_spans: None,
            expanded_gaps: HashSet::new(),
        }
    }

    pub fn metadata(&self) -> FileDiffMetadata {
        metadata::build_file_metadata(self)
    }

    pub fn additions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| matches!(line, DiffLine::Add { .. }))
            .count()
    }

    pub fn deletions(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| matches!(line, DiffLine::Delete { .. }))
            .count()
    }
}

impl DiffDocument {
    pub fn metadata(&self) -> Vec<FileDiffMetadata> {
        self.files.iter().map(FileDiff::metadata).collect()
    }

    pub fn file_row(&self, mode: DiffMode, file_index: usize) -> Option<usize> {
        self.rows(mode).iter().position(
            |row| matches!(row, RowRef::FileHeader { file_index: index } if *index == file_index),
        )
    }

    pub fn is_file_header_row(&self, mode: DiffMode, row_index: usize) -> bool {
        matches!(
            self.rows(mode).get(row_index),
            Some(RowRef::FileHeader { .. })
        )
    }

    pub fn line_row(
        &self,
        mode: DiffMode,
        file_index: usize,
        hunk_index: usize,
        line_index: usize,
    ) -> Option<usize> {
        self.rows(mode).iter().position(|row| match row {
            RowRef::Unified { line } => {
                line.file_index == file_index
                    && line.hunk_index == hunk_index
                    && line.line_index == line_index
            }
            RowRef::Split { left, right, .. } => {
                [*left, *right].into_iter().flatten().any(|line| {
                    line.file_index == file_index
                        && line.hunk_index == hunk_index
                        && line.line_index == line_index
                })
            }
            _ => false,
        })
    }

    pub fn next_hunk_row(&self, mode: DiffMode, row_index: usize) -> Option<usize> {
        self.rows(mode)
            .iter()
            .enumerate()
            .skip(row_index.saturating_add(1))
            .find_map(|(index, row)| matches!(row, RowRef::HunkHeader { .. }).then_some(index))
    }

    pub fn previous_hunk_row(&self, mode: DiffMode, row_index: usize) -> Option<usize> {
        self.rows(mode)
            .iter()
            .take(row_index)
            .enumerate()
            .rev()
            .find_map(|(index, row)| matches!(row, RowRef::HunkHeader { .. }).then_some(index))
    }

    pub fn row_file_index(&self, mode: DiffMode, row_index: usize) -> Option<usize> {
        let rows = self.rows(mode);
        if rows.is_empty() {
            return None;
        }
        let row_index = row_index.min(rows.len().saturating_sub(1));
        rows[..=row_index].iter().rev().find_map(|row| match row {
            RowRef::FileHeader { file_index }
            | RowRef::Collapsed { file_index, .. }
            | RowRef::ExpandedContext { file_index, .. }
            | RowRef::HunkHeader { file_index, .. } => Some(*file_index),
            RowRef::Unified { line } => Some(line.file_index),
            RowRef::Split { left, right, .. } => (*left).or(*right).map(|line| line.file_index),
            RowRef::FileSeparator => None,
        })
    }

    pub fn line_target(
        &self,
        mode: DiffMode,
        row_index: usize,
        side: DiffSide,
    ) -> Option<DiffLineTarget> {
        let row = *self.rows(mode).get(row_index)?;
        match row {
            RowRef::Unified { line } => self.line_target_for_ref(line, side),
            RowRef::Split { left, right, .. } => match side {
                DiffSide::Left => left.and_then(|line| self.line_target_for_ref(line, side)),
                DiffSide::Right => right.and_then(|line| self.line_target_for_ref(line, side)),
            },
            RowRef::FileSeparator
            | RowRef::FileHeader { .. }
            | RowRef::Collapsed { .. }
            | RowRef::ExpandedContext { .. }
            | RowRef::HunkHeader { .. } => None,
        }
    }

    pub fn row_code_start(
        &self,
        mode: DiffMode,
        row_index: usize,
        side: DiffSide,
    ) -> Option<usize> {
        match *self.rows(mode).get(row_index)? {
            RowRef::Unified { line } => {
                let diff_line = self.line(line);
                let (old, new) = match diff_line {
                    DiffLine::Context {
                        old_line, new_line, ..
                    } => (Some(*old_line), Some(*new_line)),
                    DiffLine::Add { new_line, .. } => (None, Some(*new_line)),
                    DiffLine::Delete { old_line, .. } => (Some(*old_line), None),
                };
                Some(
                    UnicodeWidthStr::width(gutter::exact_line_num(old).as_str())
                        + UnicodeWidthStr::width(" ")
                        + UnicodeWidthStr::width(gutter::exact_line_num(new).as_str())
                        + UnicodeWidthStr::width(" "),
                )
            }
            RowRef::ExpandedContext {
                old_line, new_line, ..
            } => match mode {
                DiffMode::Unified => Some(
                    UnicodeWidthStr::width(gutter::exact_line_num(Some(old_line)).as_str())
                        + UnicodeWidthStr::width(" ")
                        + UnicodeWidthStr::width(gutter::exact_line_num(Some(new_line)).as_str())
                        + UnicodeWidthStr::width(" "),
                ),
                DiffMode::Split => {
                    let line = match side {
                        DiffSide::Left => old_line,
                        DiffSide::Right => new_line,
                    };
                    Some(
                        UnicodeWidthStr::width(gutter::exact_line_num(Some(line)).as_str())
                            + UnicodeWidthStr::width(" "),
                    )
                }
            },
            RowRef::Split { .. } => {
                let line = self
                    .line_target(mode, row_index, side)
                    .map(|target| target.line);
                Some(
                    UnicodeWidthStr::width(gutter::exact_line_num(line).as_str())
                        + UnicodeWidthStr::width("")
                        + UnicodeWidthStr::width(" "),
                )
            }
            RowRef::FileSeparator
            | RowRef::FileHeader { .. }
            | RowRef::Collapsed { .. }
            | RowRef::HunkHeader { .. } => None,
        }
    }

    pub fn pane_text_layout(
        &self,
        mode: DiffMode,
        row_index: usize,
        side: DiffSide,
        pane: Rect,
        scroll_x: usize,
    ) -> Option<DiffPaneTextLayout> {
        let document_code_start = self.row_code_start(mode, row_index, side)?;
        let visual_code_start = match mode {
            DiffMode::Unified => document_code_start,
            DiffMode::Split => document_code_start.saturating_add(1),
        };
        Some(DiffPaneTextLayout {
            mode,
            side,
            row: row_index,
            pane,
            document_code_start,
            visual_code_start,
            scroll_x,
        })
    }

    pub fn selection_target(
        &self,
        mode: DiffMode,
        selection: DiffTextSelection,
    ) -> Option<DiffLineRangeTarget> {
        let (start, end) = selection.normalized();
        let mut first = None;
        let mut last = None;

        for row_index in start.row..=end.row {
            let Some(target) = self.line_target(mode, row_index, selection.side) else {
                continue;
            };
            if let Some(first_target) = &first {
                if !target.is_same_range_context(first_target) {
                    return None;
                }
            }
            first.get_or_insert_with(|| target.clone());
            last = Some(target);
        }

        Some(DiffLineRangeTarget {
            start: first?,
            end: last?,
        })
    }

    pub fn selection_text(&self, mode: DiffMode, selection: DiffTextSelection) -> String {
        let (start, end) = selection.normalized();
        let mut lines = Vec::new();

        for row_index in start.row..=end.row {
            let Some(text) = self.row_text_for_selection(mode, row_index, selection.side) else {
                continue;
            };
            let code_start = self
                .row_code_start(mode, row_index, selection.side)
                .unwrap_or(0);
            let range = selection
                .column_range_on_side(row_index, selection.side, code_start)
                .unwrap_or(TextSelectionRange {
                    start: 0,
                    end: usize::MAX,
                });
            lines.push(slice_text_columns(text, range.start, range.end));
        }

        lines.join("\n")
    }

    pub(crate) fn row_text_for_selection(
        &self,
        mode: DiffMode,
        row_index: usize,
        side: DiffSide,
    ) -> Option<&str> {
        match *self.rows(mode).get(row_index)? {
            RowRef::Unified { line } => Some(self.line(line).text()),
            RowRef::Split { left, right, .. } => match side {
                DiffSide::Left => left.map(|line| self.line(line).text()),
                DiffSide::Right => right.map(|line| self.line(line).text()),
            },
            RowRef::ExpandedContext {
                file_index,
                old_line,
                new_line,
            } => self.expanded_context_text(file_index, side, old_line, new_line),
            RowRef::FileSeparator
            | RowRef::FileHeader { .. }
            | RowRef::Collapsed { .. }
            | RowRef::HunkHeader { .. } => None,
        }
    }

    pub fn is_collapsed_row(&self, mode: DiffMode, row_index: usize) -> bool {
        matches!(
            self.rows(mode).get(row_index),
            Some(RowRef::Collapsed { .. })
        )
    }

    pub fn expand_collapsed_row(&mut self, mode: DiffMode, row_index: usize) -> bool {
        let Some(RowRef::Collapsed {
            file_index,
            hunk_index,
            ..
        }) = self.rows(mode).get(row_index).copied()
        else {
            return false;
        };
        let Some(file) = self.files.get_mut(file_index) else {
            return false;
        };
        if file.old_source_lines.is_none() && file.new_source_lines.is_none() {
            return false;
        }
        if !file.expanded_gaps.insert(hunk_index) {
            return false;
        }
        self.rebuild_row_cache();
        true
    }

    pub fn is_focusable_row(&self, mode: DiffMode, row_index: usize, side: DiffSide) -> bool {
        let Some(row) = self.rows(mode).get(row_index) else {
            return false;
        };
        matches!(
            row,
            RowRef::Collapsed { .. } | RowRef::ExpandedContext { .. }
        ) || self.line_target(mode, row_index, side).is_some()
            || self.line_target(mode, row_index, side.opposite()).is_some()
    }

    fn expanded_context_text(
        &self,
        file_index: usize,
        side: DiffSide,
        old_line: u32,
        new_line: u32,
    ) -> Option<&str> {
        let file = self.files.get(file_index)?;
        let (lines, line) = match side {
            DiffSide::Left => (&file.old_source_lines, old_line),
            DiffSide::Right => (&file.new_source_lines, new_line),
        };
        lines
            .as_ref()
            .and_then(|lines| lines.get(line.saturating_sub(1) as usize))
            .map(String::as_str)
            .or_else(|| {
                let fallback = match side {
                    DiffSide::Left => (&file.new_source_lines, new_line),
                    DiffSide::Right => (&file.old_source_lines, old_line),
                };
                fallback
                    .0
                    .as_ref()
                    .and_then(|lines| lines.get(fallback.1.saturating_sub(1) as usize))
                    .map(String::as_str)
            })
    }

    fn expanded_context_syntax_spans(
        &self,
        file_index: usize,
        side: DiffSide,
        old_line: u32,
        new_line: u32,
    ) -> &[SyntaxSpan] {
        let Some(file) = self.files.get(file_index) else {
            return &[];
        };
        let (spans, line) = match side {
            DiffSide::Left => (&file.old_source_syntax_spans, old_line),
            DiffSide::Right => (&file.new_source_syntax_spans, new_line),
        };
        spans
            .as_ref()
            .and_then(|lines| lines.get(line.saturating_sub(1) as usize))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn line_target_for_ref(&self, line: LineRef, side: DiffSide) -> Option<DiffLineTarget> {
        let diff_line = self.line(line);
        let (old_line, new_line, kind) = match diff_line {
            DiffLine::Context {
                old_line, new_line, ..
            } => (Some(*old_line), Some(*new_line), DiffLineKind::Context),
            DiffLine::Add { new_line, .. } => (None, Some(*new_line), DiffLineKind::Add),
            DiffLine::Delete { old_line, .. } => (Some(*old_line), None, DiffLineKind::Delete),
        };
        let line_number = match side {
            DiffSide::Left => old_line,
            DiffSide::Right => new_line,
        }?;
        Some(DiffLineTarget {
            file_index: line.file_index,
            hunk_index: line.hunk_index,
            line_index: line.line_index,
            path: self.files[line.file_index].new_path.clone(),
            side,
            old_line,
            new_line,
            line: line_number,
            kind,
        })
    }

    pub fn additions(&self) -> usize {
        self.files.iter().map(FileDiff::additions).sum()
    }

    pub fn deletions(&self) -> usize {
        self.files.iter().map(FileDiff::deletions).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMode {
    Unified,
    Split,
}

impl DiffMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Unified => Self::Split,
            Self::Split => Self::Unified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffSide {
    Left,
    Right,
}

impl DiffSide {
    fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiffLineTarget {
    pub file_index: usize,
    pub hunk_index: usize,
    pub line_index: usize,
    pub path: String,
    pub side: DiffSide,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub line: u32,
    pub kind: DiffLineKind,
}

impl DiffLineTarget {
    fn is_same_range_context(&self, other: &Self) -> bool {
        self.file_index == other.file_index && self.path == other.path && self.side == other.side
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiffLineRangeTarget {
    pub start: DiffLineTarget,
    pub end: DiffLineTarget,
}

impl DiffLineRangeTarget {
    pub fn single(target: DiffLineTarget) -> Self {
        Self {
            start: target.clone(),
            end: target,
        }
    }

    pub fn path(&self) -> &str {
        &self.start.path
    }

    pub fn side(&self) -> DiffSide {
        self.start.side
    }

    pub fn contains(&self, target: &DiffLineTarget) -> bool {
        self.start.is_same_range_context(target)
            && target.line >= self.start.line.min(self.end.line)
            && target.line <= self.start.line.max(self.end.line)
    }

    pub fn is_single_line(&self) -> bool {
        self.start.line == self.end.line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Context,
    Add,
    Delete,
    Empty,
}

#[derive(Debug, Clone, Copy)]
struct Cell<'a> {
    line: Option<u32>,
    kind: RowKind,
    text: &'a str,
    syntax_spans: &'a [SyntaxSpan],
    inline_spans: &'a [InlineDiffSpan],
    conceal_first: bool,
    reserve_sign: bool,
}

pub struct DiffWidget<'a> {
    document: &'a DiffDocument,
    theme: DiffTheme,
    search_matches: &'a [DiffSearchMatch],
    inline_blocks: &'a [DiffInlineBlock],
    reviewed_paths: Option<&'a HashSet<String>>,
    show_diff_cursor: bool,
}

impl<'a> DiffWidget<'a> {
    pub fn new(document: &'a DiffDocument) -> Self {
        Self {
            document,
            theme: DiffTheme::default(),
            search_matches: &[],
            inline_blocks: &[],
            reviewed_paths: None,
            show_diff_cursor: true,
        }
    }

    pub fn theme(mut self, theme: DiffTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn search_matches(mut self, search_matches: &'a [DiffSearchMatch]) -> Self {
        self.search_matches = search_matches;
        self
    }

    pub fn inline_blocks(mut self, inline_blocks: &'a [DiffInlineBlock]) -> Self {
        self.inline_blocks = inline_blocks;
        self
    }

    pub fn reviewed_paths(mut self, reviewed_paths: &'a HashSet<String>) -> Self {
        self.reviewed_paths = Some(reviewed_paths);
        self
    }

    pub fn show_diff_cursor(mut self, show: bool) -> Self {
        self.show_diff_cursor = show;
        self
    }
}

impl StatefulWidget for DiffWidget<'_> {
    type State = DiffViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        fill(area, buf, " ", Style::new().bg(self.theme.bg));
        if area.is_empty() {
            return;
        }

        let rows = self.document.rows(state.viewport.mode);
        let now = Instant::now();
        let model = state.render_model(self.document, self.inline_blocks, area);

        for (screen_y, visual_row) in model.visual_rows.iter().enumerate() {
            let y = model.content_area.y + screen_y as u16;
            match *visual_row {
                DiffVisualRow::Document { row: row_index, .. } => render_row_ref(
                    state,
                    self.document,
                    rows[row_index],
                    row_index,
                    model.content_area,
                    y,
                    buf,
                    state.cursor.side,
                    now,
                    self.search_matches,
                    self.reviewed_paths,
                    self.theme,
                    state.viewport,
                    self.show_diff_cursor,
                ),
                DiffVisualRow::InlineBlock { index, line, .. } => render_inline_block_row(
                    self.inline_blocks
                        .get(index)
                        .map(|block| state.viewport.pane_rect(model.content_area, block.side))
                        .unwrap_or(model.content_area),
                    y,
                    buf,
                    self.inline_blocks.get(index),
                    line,
                    self.theme,
                ),
            }
        }

        scrollbar::render_scrollbar(
            model.scrollbar_area,
            buf,
            model.visual_row_count,
            model.viewport_height,
            state.viewport.scroll_y,
        );
    }
}

pub fn parse_unified_diff(input: &str) -> DiffDocument {
    let mut files = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut current_hunk: Option<Hunk> = None;
    let mut old_line = 0;
    let mut new_line = 0;

    for raw in input.lines() {
        if raw.starts_with("diff --git ") {
            flush_hunk(&mut current, &mut current_hunk);
            if let Some(file) = current.take() {
                files.push(file);
            }
            current = Some(FileDiff {
                old_path: None,
                new_path: "diff".into(),
                hunks: Vec::new(),
                old_source_lines: None,
                new_source_lines: None,
                old_source_syntax_spans: None,
                new_source_syntax_spans: None,
                expanded_gaps: HashSet::new(),
            });
            continue;
        }

        if let Some(rest) = raw.strip_prefix("--- ") {
            if let Some(file) = current.as_mut() {
                let path = clean_diff_path(rest);
                file.old_path = if path == "/dev/null" {
                    None
                } else {
                    Some(path)
                };
            }
            continue;
        }

        if let Some(rest) = raw.strip_prefix("+++ ") {
            if let Some(file) = current.as_mut() {
                let path = clean_diff_path(rest);
                if path == "/dev/null" {
                    // Deletion: keep identity from old_path
                    if let Some(old) = &file.old_path {
                        file.new_path = old.clone();
                    }
                } else {
                    file.new_path = path;
                }
            }
            continue;
        }

        if raw.starts_with("@@ ") {
            flush_hunk(&mut current, &mut current_hunk);
            let (old_start, new_start) = parse_hunk_header(raw).unwrap_or((0, 0));
            old_line = old_start;
            new_line = new_start;
            current_hunk = Some(Hunk {
                old_start,
                new_start,
                header: raw.to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        let Some(hunk) = current_hunk.as_mut() else {
            continue;
        };
        if raw.starts_with('\\') {
            continue;
        }

        let text = raw.get(1..).unwrap_or_default().to_string();
        match raw.as_bytes().first().copied() {
            Some(b' ') => {
                hunk.lines.push(DiffLine::Context {
                    old_line,
                    new_line,
                    text,
                    syntax_spans: Vec::new(),
                });
                old_line += 1;
                new_line += 1;
            }
            Some(b'+') => {
                hunk.lines.push(DiffLine::Add {
                    new_line,
                    text,
                    syntax_spans: Vec::new(),
                    inline_spans: Vec::new(),
                });
                new_line += 1;
            }
            Some(b'-') => {
                hunk.lines.push(DiffLine::Delete {
                    old_line,
                    text,
                    syntax_spans: Vec::new(),
                    inline_spans: Vec::new(),
                });
                old_line += 1;
            }
            _ => {}
        }
    }

    flush_hunk(&mut current, &mut current_hunk);
    if let Some(file) = current.take() {
        files.push(file);
    }

    let mut document = DiffDocument {
        files,
        unified_rows: Vec::new(),
        split_rows: Vec::new(),
    };
    add_inline_diff_spans(&mut document);
    document.rebuild_row_cache();
    document
}

pub fn row_count_for_mode(document: &DiffDocument, mode: DiffMode) -> usize {
    document.rows(mode).len()
}

pub fn add_tree_sitter_highlights(document: &mut DiffDocument) -> HighlightStats {
    let mut highlighter = TreeSitterDiffHighlighter::new();
    let mut stats = HighlightStats::default();

    for file in &mut document.files {
        let Some(language) = language_for_path(&file.new_path) else {
            continue;
        };

        let old_source = collect_side_source(file, DiffSide::Left);
        let new_source = collect_side_source(file, DiffSide::Right);
        let old_spans = highlighter.highlight_lines(language, &old_source.text);
        let new_spans = highlighter.highlight_lines(language, &new_source.text);

        if old_spans.is_some() || new_spans.is_some() {
            stats.files_highlighted += 1;
        }
        stats.sides_highlighted +=
            usize::from(old_spans.is_some()) + usize::from(new_spans.is_some());

        for hunk in &mut file.hunks {
            for line in &mut hunk.lines {
                let mut spans = match line {
                    DiffLine::Context { new_line, .. } => new_spans
                        .as_ref()
                        .and_then(|lines| {
                            new_source
                                .line_to_index(*new_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                    DiffLine::Add { new_line, .. } => new_spans
                        .as_ref()
                        .and_then(|lines| {
                            new_source
                                .line_to_index(*new_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                    DiffLine::Delete { old_line, .. } => old_spans
                        .as_ref()
                        .and_then(|lines| {
                            old_source
                                .line_to_index(*old_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                };
                if matches!(language, SourceLanguage::Markdown) {
                    spans.extend(pierre::markdown_decoration_spans(line.text()));
                    normalize_line_spans(&mut spans);
                }
                stats.spans += spans.len();
                *line.syntax_spans_mut() = spans;
            }
        }
    }

    stats
}

pub fn add_pierre_highlights(document: &mut DiffDocument) -> HighlightStats {
    add_pierre_highlights_with_sources(document, |_file, _side| None)
}

pub fn add_pierre_highlights_with_sources<F>(
    document: &mut DiffDocument,
    mut resolve_source: F,
) -> HighlightStats
where
    F: FnMut(&FileDiff, DiffSide) -> Option<String>,
{
    let Some(mut highlighter) = pierre::PierreHighlighter::new() else {
        return HighlightStats::default();
    };
    let mut stats = HighlightStats::default();

    for file in &mut document.files {
        let language = pierre::language_for_path(&file.new_path);
        let old_full_text = resolve_source(file, DiffSide::Left);
        let new_full_text = resolve_source(file, DiffSide::Right);
        file.old_source_lines = old_full_text.as_deref().map(source_lines);
        file.new_source_lines = new_full_text.as_deref().map(source_lines);
        let old_source = old_full_text
            .map(SideSource::from_full_text)
            .unwrap_or_else(|| collect_side_source(file, DiffSide::Left));
        let new_source = new_full_text
            .map(SideSource::from_full_text)
            .unwrap_or_else(|| collect_side_source(file, DiffSide::Right));
        let old_spans = highlighter.highlight_lines(language, &old_source.text);
        let new_spans = highlighter.highlight_lines(language, &new_source.text);
        file.old_source_syntax_spans = file.old_source_lines.as_ref().and(old_spans.clone());
        file.new_source_syntax_spans = file.new_source_lines.as_ref().and(new_spans.clone());

        if old_spans.is_some() || new_spans.is_some() {
            stats.files_highlighted += 1;
        }
        stats.sides_highlighted +=
            usize::from(old_spans.is_some()) + usize::from(new_spans.is_some());

        for hunk in &mut file.hunks {
            let mut old_markdown_state = pierre::MarkdownOverlayState::default();
            let mut new_markdown_state = pierre::MarkdownOverlayState::default();
            for line in &mut hunk.lines {
                let mut spans = match line {
                    DiffLine::Context { new_line, .. } => new_spans
                        .as_ref()
                        .and_then(|lines| {
                            new_source
                                .line_to_index(*new_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                    DiffLine::Add { new_line, .. } => new_spans
                        .as_ref()
                        .and_then(|lines| {
                            new_source
                                .line_to_index(*new_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                    DiffLine::Delete { old_line, .. } => old_spans
                        .as_ref()
                        .and_then(|lines| {
                            old_source
                                .line_to_index(*old_line)
                                .and_then(|index| lines.get(index))
                        })
                        .cloned()
                        .unwrap_or_default(),
                };
                if language == "markdown" {
                    let state = match line {
                        DiffLine::Delete { .. } => &mut old_markdown_state,
                        DiffLine::Context { .. } | DiffLine::Add { .. } => &mut new_markdown_state,
                    };
                    pierre::apply_markdown_overlays(line.text(), &mut spans, state);
                    pierre::sort_render_spans(&mut spans);
                }
                stats.spans += spans.len();
                *line.syntax_spans_mut() = spans;
            }
        }
    }

    stats
}

impl DiffLine {
    fn syntax_spans_mut(&mut self) -> &mut Vec<SyntaxSpan> {
        match self {
            DiffLine::Context { syntax_spans, .. }
            | DiffLine::Add { syntax_spans, .. }
            | DiffLine::Delete { syntax_spans, .. } => syntax_spans,
        }
    }
}

impl Hunk {
    fn old_line_count(&self) -> u32 {
        self.lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Context { .. } | DiffLine::Delete { .. }))
            .count() as u32
    }

    fn new_line_count(&self) -> u32 {
        self.lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Context { .. } | DiffLine::Add { .. }))
            .count() as u32
    }

    fn leading_context_count(&self) -> usize {
        self.lines
            .iter()
            .take_while(|line| matches!(line, DiffLine::Context { .. }))
            .count()
    }
}

fn add_inline_diff_spans(document: &mut DiffDocument) {
    for file in &mut document.files {
        for hunk in &mut file.hunks {
            let mut line_index = 0;
            while line_index < hunk.lines.len() {
                if !matches!(
                    hunk.lines[line_index],
                    DiffLine::Delete { .. } | DiffLine::Add { .. }
                ) {
                    line_index += 1;
                    continue;
                }

                let group_start = line_index;
                while line_index < hunk.lines.len()
                    && matches!(
                        hunk.lines[line_index],
                        DiffLine::Delete { .. } | DiffLine::Add { .. }
                    )
                {
                    line_index += 1;
                }

                let deletes: Vec<usize> = (group_start..line_index)
                    .filter(|index| matches!(hunk.lines[*index], DiffLine::Delete { .. }))
                    .collect();
                let adds: Vec<usize> = (group_start..line_index)
                    .filter(|index| matches!(hunk.lines[*index], DiffLine::Add { .. }))
                    .collect();

                for pair_index in 0..deletes.len().min(adds.len()) {
                    let delete_index = deletes[pair_index];
                    let add_index = adds[pair_index];
                    let delete_text = hunk.lines[delete_index].text().to_owned();
                    let add_text = hunk.lines[add_index].text().to_owned();
                    let Some((delete_spans, add_spans)) =
                        compute_inline_diff_spans(&delete_text, &add_text)
                    else {
                        continue;
                    };
                    if let DiffLine::Delete { inline_spans, .. } = &mut hunk.lines[delete_index] {
                        *inline_spans = delete_spans;
                    }
                    if let DiffLine::Add { inline_spans, .. } = &mut hunk.lines[add_index] {
                        *inline_spans = add_spans;
                    }
                }
            }
        }
    }
}

impl DiffLine {
    fn text(&self) -> &str {
        match self {
            DiffLine::Context { text, .. }
            | DiffLine::Add { text, .. }
            | DiffLine::Delete { text, .. } => text,
        }
    }
}

fn slice_text_columns(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

struct SideSource {
    text: String,
    line_indices: HashMap<u32, usize>,
}

fn source_lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_string).collect()
}

impl SideSource {
    fn from_full_text(text: String) -> Self {
        let mut line_indices = HashMap::new();
        for (index, _) in text.split('\n').enumerate() {
            line_indices.insert(index.saturating_add(1) as u32, index);
        }
        Self { text, line_indices }
    }

    fn line_to_index(&self, line: u32) -> Option<usize> {
        self.line_indices.get(&line).copied()
    }
}

fn collect_side_source(file: &FileDiff, side: DiffSide) -> SideSource {
    let mut text = String::new();
    let mut line_indices = HashMap::new();
    let mut source_line_index = 0usize;
    let mut previous_line_number = None::<u32>;

    for hunk in &file.hunks {
        for line in &hunk.lines {
            let next = match (side, line) {
                (DiffSide::Left, DiffLine::Context { old_line, text, .. })
                | (DiffSide::Left, DiffLine::Delete { old_line, text, .. }) => {
                    Some((*old_line, text.as_str()))
                }
                (DiffSide::Right, DiffLine::Context { new_line, text, .. })
                | (DiffSide::Right, DiffLine::Add { new_line, text, .. }) => {
                    Some((*new_line, text.as_str()))
                }
                _ => None,
            };

            if let Some((line_number, line_text)) = next {
                if !text.is_empty() {
                    text.push('\n');
                    source_line_index = source_line_index.saturating_add(1);
                }
                if let Some(previous) = previous_line_number {
                    for _ in previous.saturating_add(1)..line_number {
                        text.push('\n');
                        source_line_index = source_line_index.saturating_add(1);
                    }
                }
                text.push_str(line_text);
                line_indices.insert(line_number, source_line_index);
                previous_line_number = Some(line_number);
            }
        }
    }

    SideSource { text, line_indices }
}

#[derive(Clone, Copy)]
enum SourceLanguage {
    Cpp,
    JavaScript,
    Markdown,
    TypeScript,
    Tsx,
}

fn language_for_path(path: &str) -> Option<SourceLanguage> {
    let lower = path.to_ascii_lowercase();
    if matches_extension(&lower, &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"]) {
        Some(SourceLanguage::Cpp)
    } else if matches_extension(&lower, &["js", "cjs", "mjs", "jsx"]) {
        Some(SourceLanguage::JavaScript)
    } else if matches_extension(&lower, &["md", "markdown"]) {
        Some(SourceLanguage::Markdown)
    } else if matches_extension(&lower, &["ts", "cts", "mts"]) {
        Some(SourceLanguage::TypeScript)
    } else if matches_extension(&lower, &["tsx"]) {
        Some(SourceLanguage::Tsx)
    } else {
        None
    }
}

fn matches_extension(path: &str, extensions: &[&str]) -> bool {
    extensions
        .iter()
        .any(|extension| path.ends_with(&format!(".{extension}")))
}

struct TreeSitterDiffHighlighter {
    highlighter: Highlighter,
    cpp: Option<HighlightConfiguration>,
    javascript: Option<HighlightConfiguration>,
    markdown: Option<HighlightConfiguration>,
    markdown_inline: Option<HighlightConfiguration>,
    typescript: Option<HighlightConfiguration>,
    tsx: Option<HighlightConfiguration>,
}

impl TreeSitterDiffHighlighter {
    fn new() -> Self {
        Self {
            highlighter: Highlighter::new(),
            cpp: make_highlight_config(
                tree_sitter_cpp::LANGUAGE.into(),
                "cpp",
                tree_sitter_cpp::HIGHLIGHT_QUERY,
                "",
                "",
            ),
            javascript: make_highlight_config(
                tree_sitter_javascript::LANGUAGE.into(),
                "javascript",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::INJECTIONS_QUERY,
                tree_sitter_javascript::LOCALS_QUERY,
            ),
            markdown: make_highlight_config(
                tree_sitter_md::LANGUAGE.into(),
                "markdown",
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                tree_sitter_md::INJECTION_QUERY_BLOCK,
                "",
            ),
            markdown_inline: make_highlight_config(
                tree_sitter_md::INLINE_LANGUAGE.into(),
                "markdown_inline",
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                tree_sitter_md::INJECTION_QUERY_INLINE,
                "",
            ),
            typescript: make_highlight_config(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                "typescript",
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_typescript::LOCALS_QUERY,
            ),
            tsx: make_highlight_config(
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                "tsx",
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_typescript::LOCALS_QUERY,
            ),
        }
    }

    fn highlight_lines(
        &mut self,
        language: SourceLanguage,
        source: &str,
    ) -> Option<Vec<Vec<SyntaxSpan>>> {
        if matches!(language, SourceLanguage::Markdown) {
            return self.highlight_markdown_lines(source);
        }

        if source.is_empty() {
            return Some(Vec::new());
        }

        let config = match language {
            SourceLanguage::Cpp => self.cpp.as_ref(),
            SourceLanguage::JavaScript => self.javascript.as_ref(),
            SourceLanguage::Markdown => unreachable!(),
            SourceLanguage::TypeScript => self.typescript.as_ref(),
            SourceLanguage::Tsx => self.tsx.as_ref(),
        }?;
        highlight_with_config(&mut self.highlighter, config, source)
    }

    fn highlight_markdown_lines(&mut self, source: &str) -> Option<Vec<Vec<SyntaxSpan>>> {
        if source.is_empty() {
            return Some(Vec::new());
        }

        let mut line_spans =
            highlight_with_config(&mut self.highlighter, self.markdown.as_ref()?, source)?;
        if let Some(inline_config) = self.markdown_inline.as_ref() {
            if let Some(inline_spans) =
                highlight_with_config(&mut self.highlighter, inline_config, source)
            {
                for (target, inline) in line_spans.iter_mut().zip(inline_spans) {
                    target.extend(inline);
                    normalize_line_spans(target);
                }
            }
        }
        Some(line_spans)
    }
}

fn highlight_with_config(
    highlighter: &mut Highlighter,
    config: &HighlightConfiguration,
    source: &str,
) -> Option<Vec<Vec<SyntaxSpan>>> {
    let events = highlighter
        .highlight(config, source.as_bytes(), None, |_| None)
        .ok()?;
    let line_starts = line_starts(source);
    let mut line_spans = vec![Vec::new(); line_starts.len()];
    let mut highlight_stack = Vec::new();

    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(highlight) => {
                highlight_stack.push(highlight.0);
            }
            HighlightEvent::HighlightEnd => {
                highlight_stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let Some(kind) = highlight_stack
                    .last()
                    .and_then(|index| highlight_kind(HIGHLIGHT_NAMES.get(*index).copied()?))
                else {
                    continue;
                };
                push_source_span(source, &line_starts, &mut line_spans, start, end, kind);
            }
        }
    }

    for spans in &mut line_spans {
        normalize_line_spans(spans);
    }

    Some(line_spans)
}

fn make_highlight_config(
    language: tree_sitter::Language,
    name: &str,
    highlights_query: &str,
    injection_query: &str,
    locals_query: &str,
) -> Option<HighlightConfiguration> {
    let mut config = HighlightConfiguration::new(
        language,
        name,
        highlights_query,
        injection_query,
        locals_query,
    )
    .ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(config)
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (offset, byte) in source.bytes().enumerate() {
        if byte == b'\n' && offset + 1 < source.len() {
            starts.push(offset + 1);
        }
    }
    starts
}

fn push_source_span(
    source: &str,
    line_starts: &[usize],
    line_spans: &mut [Vec<SyntaxSpan>],
    start: usize,
    end: usize,
    kind: SyntaxHighlightKind,
) {
    if start >= end || line_starts.is_empty() {
        return;
    }

    let mut line_index = byte_line_index(line_starts, start);
    while line_index < line_starts.len() {
        let line_start = line_starts[line_index];
        let line_end = line_starts
            .get(line_index + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(source.len());
        if start >= line_end && end > line_end {
            line_index += 1;
            continue;
        }
        let span_start = start.max(line_start);
        let span_end = end.min(line_end);
        if span_start < span_end {
            line_spans[line_index].push(SyntaxSpan {
                start: span_start - line_start,
                end: span_end - line_start,
                kind,
                style: None,
            });
        }
        if end <= line_end {
            break;
        }
        line_index += 1;
    }
}

fn byte_line_index(line_starts: &[usize], byte: usize) -> usize {
    line_starts
        .partition_point(|start| *start <= byte)
        .saturating_sub(1)
}

fn highlight_kind(name: &str) -> Option<SyntaxHighlightKind> {
    match name {
        "comment" | "comment.documentation" => Some(SyntaxHighlightKind::Comment),
        "keyword" | "operator" => Some(SyntaxHighlightKind::Keyword),
        "string"
        | "string.escape"
        | "string.regexp"
        | "string.special"
        | "string.special.symbol" => Some(SyntaxHighlightKind::String),
        "number" | "constant" | "constant.builtin" => Some(SyntaxHighlightKind::Number),
        "boolean" => Some(SyntaxHighlightKind::Boolean),
        "function" | "function.builtin" | "constructor" | "constructor.builtin" => {
            Some(SyntaxHighlightKind::Function)
        }
        "type" | "type.builtin" | "tag" => Some(SyntaxHighlightKind::Type),
        "property" | "property.builtin" | "variable.member" | "variable.parameter" => {
            Some(SyntaxHighlightKind::Property)
        }
        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special" => {
            Some(SyntaxHighlightKind::Punctuation)
        }
        "text.literal" | "text.uri" => Some(SyntaxHighlightKind::String),
        "text.reference" => Some(SyntaxHighlightKind::Function),
        "text.title" | "text.emphasis" | "text.strong" => Some(SyntaxHighlightKind::Markup),
        name if name.starts_with("markup") => Some(SyntaxHighlightKind::Markup),
        _ => None,
    }
}

fn normalize_line_spans(spans: &mut Vec<SyntaxSpan>) {
    spans.sort_by_key(|span| (span.start, span.end));

    let mut cursor = 0;
    spans.retain_mut(|span| {
        if span.end <= cursor || span.start >= span.end {
            return false;
        }
        if span.start < cursor {
            span.start = cursor;
        }
        cursor = span.end;
        true
    });
}

fn flush_hunk(current: &mut Option<FileDiff>, current_hunk: &mut Option<Hunk>) {
    if let (Some(file), Some(hunk)) = (current.as_mut(), current_hunk.take()) {
        file.hunks.push(hunk);
    }
}

fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split_whitespace();
    parts.next()?;
    let old = parts
        .next()?
        .trim_start_matches('-')
        .split(',')
        .next()?
        .parse()
        .ok()?;
    let new = parts
        .next()?
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse()
        .ok()?;
    Some((old, new))
}

fn clean_diff_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .strip_prefix("a/")
        .or_else(|| path.trim().trim_matches('"').strip_prefix("b/"))
        .unwrap_or_else(|| path.trim().trim_matches('"'))
        .to_string()
}

fn is_markdown_path(path: &str) -> bool {
    path.ends_with(".md") || path.ends_with(".markdown")
}

fn render_row_ref(
    state: &DiffViewerState,
    document: &DiffDocument,
    row: RowRef,
    row_index: usize,
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    _selected_side: DiffSide,
    now: Instant,
    search_matches: &[DiffSearchMatch],
    reviewed_paths: Option<&HashSet<String>>,
    theme: DiffTheme,
    viewport: DiffViewport,
    show_diff_cursor: bool,
) {
    match row {
        RowRef::FileSeparator => {
            let style = Style::new().fg(Color::Rgb(52, 60, 69)).bg(theme.panel);
            render_full_line(area, y, buf, "", style);
        }
        RowRef::FileHeader { file_index } => {
            let file = &document.files[file_index];
            render_file_header(
                area,
                y,
                buf,
                file,
                file_index,
                document.files.len(),
                reviewed_paths.is_some_and(|paths| paths.contains(&file.new_path)),
                theme,
            );
        }
        RowRef::Collapsed {
            file_index, count, ..
        } => {
            render_collapsed_row(
                area,
                y,
                buf,
                document.files[file_index].old_path.as_deref(),
                count,
                theme,
                state.cursor.row == row_index,
            );
        }
        RowRef::ExpandedContext {
            file_index,
            old_line,
            new_line,
        } => {
            render_expanded_context_row(
                state,
                document,
                area,
                y,
                buf,
                file_index,
                old_line,
                new_line,
                row_index,
                now,
                search_matches,
                theme,
                viewport,
                show_diff_cursor,
            );
        }
        RowRef::HunkHeader {
            file_index,
            hunk_index,
        } => {
            render_hunk_header_row(
                area,
                y,
                buf,
                &document.files[file_index].hunks[hunk_index].header,
                theme,
            );
        }
        RowRef::Unified { line } => {
            let diff_line = document.line(line);
            let (old, new, kind, side, text, syntax_spans, inline_spans) = match diff_line {
                DiffLine::Context {
                    old_line,
                    new_line,
                    text,
                    syntax_spans,
                } => (
                    Some(*old_line),
                    Some(*new_line),
                    RowKind::Context,
                    DiffSide::Right,
                    text.as_str(),
                    syntax_spans.as_slice(),
                    &[][..],
                ),
                DiffLine::Add {
                    new_line,
                    text,
                    syntax_spans,
                    inline_spans,
                } => (
                    None,
                    Some(*new_line),
                    RowKind::Add,
                    DiffSide::Right,
                    text.as_str(),
                    syntax_spans.as_slice(),
                    inline_spans.as_slice(),
                ),
                DiffLine::Delete {
                    old_line,
                    text,
                    syntax_spans,
                    inline_spans,
                } => (
                    Some(*old_line),
                    None,
                    RowKind::Delete,
                    DiffSide::Left,
                    text.as_str(),
                    syntax_spans.as_slice(),
                    inline_spans.as_slice(),
                ),
            };
            let style = row_style(kind, false, theme);
            let gutter_style = Style::new()
                .fg(theme.muted)
                .bg(style.bg.unwrap_or(theme.bg));
            let gutter = format!(
                "{} {} ",
                gutter::exact_line_num(old),
                gutter::exact_line_num(new)
            );
            let conceal_first = is_markdown_path(&document.files[line.file_index].new_path);
            let visible_text = concealed_text(text, conceal_first);
            let code_start = document
                .row_code_start(DiffMode::Unified, row_index, side)
                .unwrap_or(0);
            let layout = document
                .pane_text_layout(
                    DiffMode::Unified,
                    row_index,
                    side,
                    area,
                    viewport.scroll_x_for_side(side),
                )
                .unwrap_or(DiffPaneTextLayout {
                    mode: DiffMode::Unified,
                    side,
                    row: row_index,
                    pane: area,
                    document_code_start: code_start,
                    visual_code_start: code_start,
                    scroll_x: viewport.scroll_x_for_side(side),
                });
            let overlays = row_overlays_for_render(
                state,
                document,
                area,
                row_index,
                side,
                now,
                search_matches,
                show_diff_cursor,
            );
            render_segments(
                area,
                y,
                buf,
                &[(&gutter, gutter_style)],
                &visible_text,
                syntax_spans,
                inline_spans,
                kind,
                theme,
                style,
                layout,
                &overlays,
            );
        }
        RowRef::Split {
            left,
            right,
            left_reserve_sign,
            right_reserve_sign,
        } => {
            let half = area.width / 2;
            let left_area = Rect::new(area.x, y, half, 1);
            let right_area = Rect::new(area.x + half, y, area.width - half, 1);
            let left_cell = left
                .map(|line| cell_for_line_ref(document, line, DiffSide::Left, left_reserve_sign));
            let right_cell = right
                .map(|line| cell_for_line_ref(document, line, DiffSide::Right, right_reserve_sign));
            let left_overlays = row_overlays_for_render(
                state,
                document,
                area,
                row_index,
                DiffSide::Left,
                now,
                search_matches,
                show_diff_cursor,
            );
            let right_overlays = row_overlays_for_render(
                state,
                document,
                area,
                row_index,
                DiffSide::Right,
                now,
                search_matches,
                show_diff_cursor,
            );
            render_split_cell(
                document,
                row_index,
                DiffSide::Left,
                left_area,
                buf,
                left_cell.as_ref(),
                &left_overlays,
                theme,
                viewport.scroll_x_for_side(DiffSide::Left),
            );
            render_split_cell(
                document,
                row_index,
                DiffSide::Right,
                right_area,
                buf,
                right_cell.as_ref(),
                &right_overlays,
                theme,
                viewport.scroll_x_for_side(DiffSide::Right),
            );
        }
    }
}

fn render_file_header(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    file: &FileDiff,
    file_index: usize,
    file_count: usize,
    reviewed: bool,
    theme: DiffTheme,
) {
    let style = Style::new().fg(theme.text).bg(theme.panel);
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }

    let name = format!("{}/{} {}", file_index + 1, file_count, file.new_path);
    buf.set_stringn(area.x, y, name, area.width as usize, style);

    let reviewed_label = if reviewed {
        "☑ Reviewed "
    } else {
        "☐ Reviewed "
    };
    let reviewed_width = UnicodeWidthStr::width(reviewed_label) as u16;
    let reviewed_x = area.right().saturating_sub(reviewed_width);
    if reviewed_width < area.width {
        buf.set_stringn(
            reviewed_x,
            y,
            reviewed_label,
            reviewed_width as usize,
            Style::new()
                .fg(if reviewed { theme.add_fg } else { theme.muted })
                .bg(theme.panel),
        );
    }

    let additions = file.additions();
    let deletions = file.deletions();
    let suffix = match (additions, deletions) {
        (0, 0) => "0 ".to_string(),
        (additions, 0) => format!("+{additions} "),
        (0, deletions) => format!("-{deletions} "),
        (additions, deletions) => format!("+{additions} -{deletions} "),
    };
    let suffix_width = suffix.len() as u16;
    let suffix_right = reviewed_x.saturating_sub(1).max(area.x);
    if suffix_width < suffix_right.saturating_sub(area.x) {
        let suffix_x = suffix_right.saturating_sub(suffix_width);
        match (additions, deletions) {
            (0, 0) => {
                buf.set_stringn(
                    suffix_x,
                    y,
                    "0",
                    suffix_width as usize,
                    Style::new().fg(theme.muted).bg(theme.panel),
                );
            }
            (additions, 0) => {
                buf.set_stringn(
                    suffix_x,
                    y,
                    format!("+{additions}"),
                    suffix_width as usize,
                    Style::new().fg(theme.add_fg).bg(theme.panel),
                );
            }
            (0, deletions) => {
                buf.set_stringn(
                    suffix_x,
                    y,
                    format!("-{deletions}"),
                    suffix_width as usize,
                    Style::new().fg(theme.del_fg).bg(theme.panel),
                );
            }
            (additions, deletions) => {
                buf.set_stringn(
                    suffix_x,
                    y,
                    format!("+{additions}"),
                    suffix_width as usize,
                    Style::new().fg(theme.add_fg).bg(theme.panel),
                );
                let del_x = suffix_x.saturating_add(additions.to_string().len() as u16 + 2);
                buf.set_stringn(
                    del_x,
                    y,
                    format!("-{deletions}"),
                    suffix_width as usize,
                    Style::new().fg(theme.del_fg).bg(theme.panel),
                );
            }
        }
    }
}

fn render_collapsed_row(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    old_path: Option<&str>,
    count: u32,
    theme: DiffTheme,
    selected: bool,
) {
    let _ = old_path;
    let bg = if selected { theme.selected } else { theme.bg };
    let style = Style::new()
        .fg(if selected { theme.text } else { theme.muted })
        .bg(bg);
    let line_style = Style::new().fg(theme.panel_alt).bg(bg);
    let label = unchanged_lines_label(count);

    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }

    render_collapsed_boundary_segment(area, y, buf, &label, style, line_style);
}

fn render_expanded_context_row(
    state: &DiffViewerState,
    document: &DiffDocument,
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    file_index: usize,
    old_line: u32,
    new_line: u32,
    row_index: usize,
    now: Instant,
    search_matches: &[DiffSearchMatch],
    theme: DiffTheme,
    viewport: DiffViewport,
    show_diff_cursor: bool,
) {
    match viewport.mode {
        DiffMode::Unified => {
            let side = DiffSide::Right;
            let text = document
                .expanded_context_text(file_index, side, old_line, new_line)
                .unwrap_or_default();
            let syntax_spans =
                document.expanded_context_syntax_spans(file_index, side, old_line, new_line);
            let visible_text =
                concealed_text(text, is_markdown_path(&document.files[file_index].new_path));
            let style = row_style(RowKind::Context, false, theme);
            let gutter_style = Style::new()
                .fg(theme.muted)
                .bg(style.bg.unwrap_or(theme.bg));
            let gutter = format!(
                "{} {} ",
                gutter::exact_line_num(Some(old_line)),
                gutter::exact_line_num(Some(new_line))
            );
            let layout = document
                .pane_text_layout(DiffMode::Unified, row_index, side, area, viewport.scroll_x)
                .unwrap_or(DiffPaneTextLayout {
                    mode: DiffMode::Unified,
                    side,
                    row: row_index,
                    pane: area,
                    document_code_start: gutter.len(),
                    visual_code_start: gutter.len(),
                    scroll_x: viewport.scroll_x,
                });
            let overlays = row_overlays_for_render(
                state,
                document,
                area,
                row_index,
                side,
                now,
                search_matches,
                show_diff_cursor,
            );
            render_segments(
                area,
                y,
                buf,
                &[(&gutter, gutter_style)],
                &visible_text,
                syntax_spans,
                &[],
                RowKind::Context,
                theme,
                style,
                layout,
                &overlays,
            );
        }
        DiffMode::Split => {
            let half = area.width / 2;
            let left_area = Rect::new(area.x, y, half, 1);
            let right_area = Rect::new(area.x + half, y, area.width - half, 1);
            render_expanded_context_split_side(
                state,
                document,
                left_area,
                buf,
                file_index,
                old_line,
                new_line,
                row_index,
                DiffSide::Left,
                now,
                search_matches,
                theme,
                viewport,
                show_diff_cursor,
            );
            render_expanded_context_split_side(
                state,
                document,
                right_area,
                buf,
                file_index,
                old_line,
                new_line,
                row_index,
                DiffSide::Right,
                now,
                search_matches,
                theme,
                viewport,
                show_diff_cursor,
            );
        }
    }
}

fn render_expanded_context_split_side(
    state: &DiffViewerState,
    document: &DiffDocument,
    area: Rect,
    buf: &mut Buffer,
    file_index: usize,
    old_line: u32,
    new_line: u32,
    row_index: usize,
    side: DiffSide,
    now: Instant,
    search_matches: &[DiffSearchMatch],
    theme: DiffTheme,
    viewport: DiffViewport,
    show_diff_cursor: bool,
) {
    let text = document
        .expanded_context_text(file_index, side, old_line, new_line)
        .unwrap_or_default();
    let syntax_spans = document.expanded_context_syntax_spans(file_index, side, old_line, new_line);
    let visible_text = concealed_text(text, is_markdown_path(&document.files[file_index].new_path));
    let style = row_style(RowKind::Context, false, theme);
    let line = match side {
        DiffSide::Left => old_line,
        DiffSide::Right => new_line,
    };
    let gutter = gutter::split_gutter_segments(
        GutterCell {
            line: Some(line),
            sign: None,
            reserve_sign: true,
        },
        RowKind::Context,
        theme,
        false,
    );
    let prefix_segments = [
        (gutter.rail, gutter.rail_style),
        (&gutter.line_number, gutter.line_number_style),
        (gutter.sign, gutter.sign_style),
        (gutter.trailing, gutter.line_number_style),
    ];
    let layout = document
        .pane_text_layout(
            DiffMode::Split,
            row_index,
            side,
            area,
            viewport.scroll_x_for_side(side),
        )
        .unwrap_or(DiffPaneTextLayout {
            mode: DiffMode::Split,
            side,
            row: row_index,
            pane: area,
            document_code_start: 5,
            visual_code_start: 6,
            scroll_x: viewport.scroll_x_for_side(side),
        });
    let overlays = row_overlays_for_render(
        state,
        document,
        area,
        row_index,
        side,
        now,
        search_matches,
        show_diff_cursor,
    );
    render_segments(
        area,
        area.y,
        buf,
        &prefix_segments,
        &visible_text,
        syntax_spans,
        &[],
        RowKind::Context,
        theme,
        style,
        layout,
        &overlays,
    );
}

fn unchanged_lines_label(count: u32) -> String {
    match count {
        1 => "↕ 1 unchanged line".to_string(),
        count => format!("↕ {count} unchanged lines"),
    }
}

fn render_collapsed_boundary_segment(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    label: &str,
    label_style: Style,
    line_style: Style,
) {
    if area.width == 0 {
        return;
    }

    let label_width = UnicodeWidthStr::width(label) as u16;
    if label_width.saturating_add(2) >= area.width {
        buf.set_stringn(area.x, y, label, area.width as usize, label_style);
        return;
    }

    for x in area.x.saturating_add(1)..area.right().saturating_sub(1) {
        buf.set_stringn(x, y, "─", 1, line_style);
    }

    let label_x = area
        .x
        .saturating_add(area.width.saturating_sub(label_width) / 2)
        .min(area.right().saturating_sub(label_width));
    let padded = format!(" {label} ");
    let padded_x = label_x.saturating_sub(1);
    buf.set_stringn(
        padded_x,
        y,
        padded,
        area.right().saturating_sub(padded_x) as usize,
        label_style,
    );
}

fn render_hunk_header_row(area: Rect, y: u16, buf: &mut Buffer, header: &str, theme: DiffTheme) {
    let style = Style::new().fg(theme.muted).bg(theme.panel_alt);
    let rail_style = Style::new()
        .fg(gutter::rail_color(RowKind::Context, theme, false))
        .bg(theme.panel_alt);
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }
    if area.width == 0 {
        return;
    }
    let label = compact_hunk_header(header);
    buf.set_stringn(area.x, y, "▌", 1, rail_style);
    if area.width > 2 {
        buf.set_stringn(
            area.x.saturating_add(2),
            y,
            label,
            area.width.saturating_sub(2) as usize,
            style,
        );
    }
}

fn row_overlays_for_render(
    state: &DiffViewerState,
    document: &DiffDocument,
    area: Rect,
    row_index: usize,
    side: DiffSide,
    now: Instant,
    search_matches: &[DiffSearchMatch],
    show_diff_cursor: bool,
) -> Vec<DiffVisualOverlay> {
    let mut overlays =
        state.overlays_for_row_side(document, area, row_index, side, now, search_matches);
    if !show_diff_cursor {
        overlays.retain(|overlay| overlay.kind != DiffOverlayKind::Cursor);
    }
    overlays
}

fn render_inline_block_row(
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    block: Option<&DiffInlineBlock>,
    line: usize,
    theme: DiffTheme,
) {
    let style = Style::new().fg(theme.text).bg(theme.panel_alt);
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }
    let Some(block) = block else {
        return;
    };
    let accent = inline_block_accent_color(block.accent, theme);
    let box_x = area.x.saturating_add(2).min(area.right());
    let box_width = area.width.saturating_sub(4).min(76).max(0);
    if box_width < 4 || box_x >= area.right() {
        return;
    }
    let box_width = box_width.min(area.right().saturating_sub(box_x));
    let border_style = Style::new().fg(accent).bg(theme.panel_alt);
    let body_style = Style::new().fg(theme.text).bg(theme.panel_alt);

    if line == 0 {
        let title = if block.title.is_empty() {
            String::new()
        } else {
            format!(" {} ", block.title)
        };
        let available = box_width.saturating_sub(2) as usize;
        let title_width = UnicodeWidthStr::width(title.as_str()).min(available);
        buf.set_stringn(box_x, y, "╭", 1, border_style);
        buf.set_stringn(box_x.saturating_add(1), y, &title, available, border_style);
        let used = 1u16.saturating_add(title_width as u16);
        for x in box_x.saturating_add(used)..box_x.saturating_add(box_width).saturating_sub(1) {
            buf.set_stringn(x, y, "─", 1, border_style);
        }
        buf.set_stringn(
            box_x.saturating_add(box_width).saturating_sub(1),
            y,
            "╮",
            1,
            border_style,
        );
        return;
    }

    if line + 1 == block.height {
        buf.set_stringn(box_x, y, "╰", 1, border_style);
        for x in box_x.saturating_add(1)..box_x.saturating_add(box_width).saturating_sub(1) {
            buf.set_stringn(x, y, "─", 1, border_style);
        }
        buf.set_stringn(
            box_x.saturating_add(box_width).saturating_sub(1),
            y,
            "╯",
            1,
            border_style,
        );
        return;
    }

    buf.set_stringn(box_x, y, "│", 1, border_style);
    buf.set_stringn(
        box_x.saturating_add(box_width).saturating_sub(1),
        y,
        "│",
        1,
        border_style,
    );
    for x in box_x.saturating_add(1)..box_x.saturating_add(box_width).saturating_sub(1) {
        buf[(x, y)].set_symbol(" ").set_style(body_style);
    }
    let content_width = box_width.saturating_sub(5) as usize;
    let wrapped_body = wrap_inline_block_body(&block.body, content_width);
    let body_line_index = line.saturating_sub(1);
    let body_line = wrapped_body
        .get(body_line_index)
        .map(String::as_str)
        .unwrap_or_default();
    let placeholder =
        block.kind == DiffInlineBlockKind::Editor && block.body.is_empty() && body_line_index == 0;
    let text = if placeholder {
        "type a comment…"
    } else {
        body_line
    };
    let text_style = if placeholder {
        Style::new()
            .fg(theme.muted)
            .bg(body_style.bg.unwrap_or(theme.panel_alt))
    } else {
        body_style
    };
    if box_width > 5 {
        buf.set_stringn(box_x.saturating_add(2), y, text, content_width, text_style);
    }
}

fn wrap_inline_block_body(body: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for logical in body.lines() {
        if logical.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in logical.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if current_width > 0 && current_width.saturating_add(ch_width) > width {
                out.push(current);
                current = String::new();
                current_width = 0;
            }
            current.push(ch);
            current_width = current_width.saturating_add(ch_width);
        }
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn inline_block_accent_color(accent: DiffInlineBlockAccent, theme: DiffTheme) -> Color {
    match accent {
        DiffInlineBlockAccent::Instruction => theme.del_fg,
        DiffInlineBlockAccent::Question => theme.add_fg,
        DiffInlineBlockAccent::Agent => theme.muted,
        DiffInlineBlockAccent::Note | DiffInlineBlockAccent::Draft => theme.line_number_fg,
    }
}

fn compact_hunk_header(header: &str) -> String {
    let mut parts = header.split("@@");
    let _ = parts.next();
    let _ranges = parts.next().unwrap_or_default().trim();
    let context = parts.next().unwrap_or_default().trim();
    if context.is_empty() || context == "{" || context == "}" {
        "nearby changes".to_string()
    } else {
        context.to_string()
    }
}

fn cell_for_line_ref<'a>(
    document: &'a DiffDocument,
    line: LineRef,
    side: DiffSide,
    reserve_sign: bool,
) -> Cell<'a> {
    let conceal_first = is_markdown_path(&document.files[line.file_index].new_path);
    match document.line(line) {
        DiffLine::Context {
            old_line,
            new_line,
            text,
            syntax_spans,
        } => Cell {
            line: Some(match side {
                DiffSide::Left => *old_line,
                DiffSide::Right => *new_line,
            }),
            kind: RowKind::Context,
            text,
            syntax_spans,
            inline_spans: &[],
            conceal_first,
            reserve_sign,
        },
        DiffLine::Add {
            new_line,
            text,
            syntax_spans,
            inline_spans,
        } => Cell {
            line: Some(*new_line),
            kind: RowKind::Add,
            text,
            syntax_spans,
            inline_spans,
            conceal_first,
            reserve_sign,
        },
        DiffLine::Delete {
            old_line,
            text,
            syntax_spans,
            inline_spans,
        } => Cell {
            line: Some(*old_line),
            kind: RowKind::Delete,
            text,
            syntax_spans,
            inline_spans,
            conceal_first,
            reserve_sign,
        },
    }
}

fn render_split_cell(
    document: &DiffDocument,
    row_index: usize,
    side: DiffSide,
    area: Rect,
    buf: &mut Buffer,
    cell: Option<&Cell<'_>>,
    overlays: &[DiffVisualOverlay],
    theme: DiffTheme,
    scroll_x: usize,
) {
    let cell = cell.copied().unwrap_or(Cell {
        line: None,
        kind: RowKind::Empty,
        text: "",
        syntax_spans: &[],
        inline_spans: &[],
        conceal_first: false,
        reserve_sign: false,
    });
    let style = row_style(cell.kind, false, theme);
    let sign = match cell.kind {
        RowKind::Add => Some(LineSign::Add),
        RowKind::Delete => Some(LineSign::Delete),
        RowKind::Context | RowKind::Empty => None,
    };
    let gutter = gutter::split_gutter_segments(
        GutterCell {
            line: cell.line,
            sign,
            reserve_sign: cell.reserve_sign,
        },
        cell.kind,
        theme,
        false,
    );
    let prefix_segments = [
        (gutter.rail, gutter.rail_style),
        (&gutter.line_number, gutter.line_number_style),
        (gutter.sign, gutter.sign_style),
        (gutter.trailing, gutter.line_number_style),
    ];
    let visible_text = concealed_text(cell.text, cell.conceal_first);
    let layout = document
        .pane_text_layout(DiffMode::Split, row_index, side, area, scroll_x)
        .unwrap_or(DiffPaneTextLayout {
            mode: DiffMode::Split,
            side,
            row: row_index,
            pane: area,
            document_code_start: 0,
            visual_code_start: prefix_segments
                .iter()
                .map(|(text, _)| UnicodeWidthStr::width(*text))
                .sum(),
            scroll_x,
        });
    render_segments(
        area,
        area.y,
        buf,
        &prefix_segments,
        visible_text.as_str(),
        cell.syntax_spans,
        cell.inline_spans,
        cell.kind,
        theme,
        style,
        layout,
        overlays,
    );

    if cell.kind == RowKind::Empty {
        let gutter_width = prefix_segments
            .iter()
            .map(|(text, _)| text.chars().count() as u16)
            .sum::<u16>();
        let start = area.x.saturating_add(gutter_width).min(area.right());
        for x in start..area.right() {
            buf[(x, area.y)].set_symbol("╱").set_style(style);
        }
    }
}

fn fill(area: Rect, buf: &mut Buffer, symbol: &str, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_symbol(symbol).set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_highlight_source_preserves_sparse_line_gaps() {
        let document = parse_unified_diff(
            "diff --git a/app.rs b/app.rs\n--- a/app.rs\n+++ b/app.rs\n@@ -1 +1 @@\n fn first() {}\n@@ -10 +10 @@\n fn tenth() {}\n",
        );
        let file = document.files.first().expect("file diff");
        let source = collect_side_source(file, DiffSide::Right);

        assert_eq!(source.line_to_index(1), Some(0));
        assert_eq!(source.line_to_index(10), Some(9));
        assert_eq!(source.text.lines().nth(9), Some("fn tenth() {}"));
    }

    #[test]
    fn full_highlight_source_maps_real_file_line_numbers() {
        let source = SideSource::from_full_text("one\ntwo\nthree".to_string());

        assert_eq!(source.line_to_index(1), Some(0));
        assert_eq!(source.line_to_index(2), Some(1));
        assert_eq!(source.line_to_index(3), Some(2));
    }

    #[test]
    fn selection_text_extracts_split_side_columns() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n-old beta\n+new alpha\n+new beta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut selection = DiffTextSelection::character(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 4,
        });
        selection.set_cursor(DiffTextPoint {
            row: row + 1,
            side: DiffSide::Right,
            column: code_start + 7,
        });

        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "alpha\nnew beta"
        );
    }

    #[test]
    fn selection_text_normalizes_backward_selection_like_opentui() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n-old beta\n+new alpha\n+new beta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut selection = DiffTextSelection::character(DiffTextPoint {
            row: row + 1,
            side: DiffSide::Right,
            column: code_start + 7,
        });
        selection.set_cursor(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 4,
        });

        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "alpha\nnew beta"
        );
    }

    #[test]
    fn selection_text_extracts_same_line_slice() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut selection = DiffTextSelection::character(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 4,
        });
        selection.set_cursor(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 8,
        });

        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha");
    }

    #[test]
    fn pane_text_layout_round_trips_document_and_screen_columns() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let area = Rect::new(20, 0, 20, 1);
        let layout = document
            .pane_text_layout(DiffMode::Split, row, DiffSide::Right, area, 0)
            .expect("pane layout");
        let text_col = 4;
        let document_col = layout.document_code_start + text_col;
        let x = layout.document_col_to_screen_x(document_col);

        assert_eq!(
            layout.screen_x_to_document_col(x, layout.document_code_start + "new alpha".len()),
            document_col
        );
    }

    #[test]
    fn pane_text_layout_scroll_to_reveal_accounts_for_gutter_width() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new alpha beta gamma delta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let area = Rect::new(20, 0, 12, 1);
        let layout = document
            .pane_text_layout(DiffMode::Split, row, DiffSide::Right, area, 0)
            .expect("pane layout");
        let last_col = layout.document_code_start + "new alpha beta gamma delta".len() - 1;
        let scroll_x = layout.scroll_to_reveal(last_col);
        let scrolled = document
            .pane_text_layout(DiffMode::Split, row, DiffSide::Right, area, scroll_x)
            .expect("scrolled layout");

        assert!(scrolled.document_col_to_pane_col(last_col) < usize::from(area.width));
    }

    #[test]
    fn split_selection_paints_at_cursor_document_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut state = DiffViewerState::default();
        state.viewport.mode = DiffMode::Split;
        state.cursor.row = row;
        state.cursor.side = DiffSide::Right;
        state.selection = Some(DiffTextSelection::character(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 4,
        }));

        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        StatefulWidget::render(DiffWidget::new(&document), area, &mut buf, &mut state);

        let selected_bg = crate::pierre::selection_style().bg.unwrap();
        let y = row as u16;
        let content_width = area.width.saturating_sub(1);
        let right_pane_x = content_width / 2;
        let expected_x = right_pane_x + (code_start + 4) as u16 + 1;
        let previous_x = expected_x - 1;

        assert_eq!(buf[(expected_x, y)].bg, selected_bg);
        assert_ne!(buf[(previous_x, y)].bg, selected_bg);
    }

    #[test]
    fn split_left_selection_paints_at_cursor_document_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Left)
            .unwrap();
        let mut state = DiffViewerState::default();
        state.viewport.mode = DiffMode::Split;
        state.cursor.row = row;
        state.cursor.side = DiffSide::Left;
        state.selection = Some(DiffTextSelection::character(DiffTextPoint {
            row,
            side: DiffSide::Left,
            column: code_start,
        }));

        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        StatefulWidget::render(DiffWidget::new(&document), area, &mut buf, &mut state);

        let selected_bg = crate::pierre::selection_style().bg.unwrap();
        let y = row as u16;

        let expected_x = code_start as u16 + 1;
        assert_eq!(buf[(expected_x, y)].bg, selected_bg);
        assert_ne!(buf[(expected_x - 1, y)].bg, selected_bg);
    }

    #[test]
    fn split_selection_paints_only_selected_side() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut state = DiffViewerState::default();
        state.viewport.mode = DiffMode::Split;
        state.cursor.row = row;
        state.cursor.side = DiffSide::Right;
        state.selection = Some(DiffTextSelection::character(DiffTextPoint {
            row,
            side: DiffSide::Right,
            column: code_start + 4,
        }));

        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        StatefulWidget::render(DiffWidget::new(&document), area, &mut buf, &mut state);

        let selected_bg = crate::pierre::selection_style().bg.unwrap();
        let y = row as u16;
        let content_width = area.width.saturating_sub(1);
        let right_pane_x = content_width / 2;
        let right_x = right_pane_x + (code_start + 4) as u16 + 1;
        let left_x = (code_start + 4) as u16;

        assert_eq!(buf[(right_x, y)].bg, selected_bg);
        assert_ne!(buf[(left_x, y)].bg, selected_bg);
    }

    #[test]
    fn split_search_paints_at_match_document_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let search_matches = [DiffSearchMatch {
            row,
            side: DiffSide::Right,
            range: TextSelectionRange { start: 4, end: 9 },
        }];
        let mut state = DiffViewerState::default();
        state.viewport.mode = DiffMode::Split;

        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        StatefulWidget::render(
            DiffWidget::new(&document).search_matches(&search_matches),
            area,
            &mut buf,
            &mut state,
        );

        let search_bg = DiffTheme::default().add_fg;
        let y = row as u16;
        let content_width = area.width.saturating_sub(1);
        let right_pane_x = content_width / 2;
        let expected_x = right_pane_x + (code_start + 4) as u16 + 1;
        let previous_x = expected_x - 1;

        assert_eq!(buf[(expected_x, y)].bg, search_bg);
        assert_ne!(buf[(previous_x, y)].bg, search_bg);
    }

    #[test]
    fn renderer_consumes_inline_block_visual_rows() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let inline_blocks = [DiffInlineBlock {
            id: "note:1".to_string(),
            after_row: row,
            side: DiffSide::Right,
            height: 3,
            kind: DiffInlineBlockKind::Comment,
            accent: DiffInlineBlockAccent::Note,
            title: "note · pi".to_string(),
            body: "inline body".to_string(),
        }];
        let mut state = DiffViewerState::default();
        state.viewport.mode = DiffMode::Split;
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);

        StatefulWidget::render(
            DiffWidget::new(&document).inline_blocks(&inline_blocks),
            area,
            &mut buf,
            &mut state,
        );

        let inline_y = row as u16 + 1;
        let rendered = (0..area.width)
            .map(|x| buf[(x, inline_y)].symbol())
            .collect::<String>();
        assert!(rendered.contains("note · pi"));
        let body_y = inline_y + 1;
        let rendered_body = (0..area.width)
            .map(|x| buf[(x, body_y)].symbol())
            .collect::<String>();
        assert!(rendered_body.contains("inline body"));
    }

    #[test]
    fn collapsed_row_renders_unchanged_line_boundary_label() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        render_collapsed_row(area, 0, &mut buf, None, 309, DiffTheme::default(), false);

        let rendered = (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        assert!(rendered.contains("↕ 309 unchanged lines"));
        assert!(rendered.contains("─"));
    }

    #[test]
    fn split_collapsed_row_centers_one_label_across_full_width() {
        let area = Rect::new(0, 0, 100, 1);
        let mut buf = Buffer::empty(area);

        render_collapsed_row(area, 0, &mut buf, None, 12, DiffTheme::default(), false);

        let rendered = (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        assert_eq!(rendered.matches("↕ 12 unchanged lines").count(), 1);
        let label_start = (0..area.width)
            .find(|x| buf[(*x, 0)].symbol() == "↕")
            .expect("boundary label marker");
        assert!(label_start > 35 && label_start < 45);
    }

    #[test]
    fn collapsed_row_expands_into_full_source_context_rows() {
        let mut document = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-fn one() {}\n+fn ONE() {}\n@@ -4,2 +4,2 @@\n fn four() {}\n-fn five() {}\n+fn FIVE() {}\n",
        );
        add_pierre_highlights_with_sources(&mut document, |_file, _side| {
            Some("fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}".to_string())
        });
        let collapsed_row = document
            .rows(DiffMode::Unified)
            .iter()
            .position(|row| matches!(row, RowRef::Collapsed { count: 2, .. }))
            .expect("collapsed gap row");
        let before = row_count_for_mode(&document, DiffMode::Unified);

        assert!(document.expand_collapsed_row(DiffMode::Unified, collapsed_row));

        assert_eq!(
            row_count_for_mode(&document, DiffMode::Unified),
            before.saturating_sub(1)
        );
        assert_eq!(
            document.row_text_for_selection(DiffMode::Unified, collapsed_row, DiffSide::Right),
            Some("fn two() {}")
        );
        assert!(
            !document
                .expanded_context_syntax_spans(0, DiffSide::Right, 2, 2)
                .is_empty()
        );
        assert_eq!(
            document.row_text_for_selection(DiffMode::Unified, collapsed_row + 1, DiffSide::Right),
            Some("fn three() {}")
        );
        assert!(document.line_row(DiffMode::Unified, 0, 1, 0).is_none());
        assert_eq!(
            document.line_row(DiffMode::Unified, 0, 1, 1),
            Some(collapsed_row + 2)
        );
    }

    #[test]
    fn deleted_file_uses_real_path_not_dev_null() {
        let document = parse_unified_diff(
            "diff --git a/docs/foo.md b/docs/foo.md\n\
             deleted file mode 100644\n\
             --- a/docs/foo.md\n\
             +++ /dev/null\n\
             @@ -1,2 +0,0 @@\n\
             -line one\n\
             -line two\n",
        );
        let file = &document.files[0];
        assert_eq!(file.old_path, Some("docs/foo.md".to_string()));
        assert_eq!(file.new_path, "docs/foo.md");

        let meta = crate::metadata::build_file_metadata(file);
        assert_eq!(meta.kind, crate::metadata::FileDiffKind::Deleted);
        assert_eq!(meta.name, "docs/foo.md");
    }

    #[test]
    fn added_file_has_no_old_path() {
        let document = parse_unified_diff(
            "diff --git a/new_file.rs b/new_file.rs\n\
             new file mode 100644\n\
             --- /dev/null\n\
             +++ b/new_file.rs\n\
             @@ -0,0 +1,2 @@\n\
             +line one\n\
             +line two\n",
        );
        let file = &document.files[0];
        assert_eq!(file.old_path, None);
        assert_eq!(file.new_path, "new_file.rs");

        let meta = crate::metadata::build_file_metadata(file);
        assert_eq!(meta.kind, crate::metadata::FileDiffKind::New);
        assert_eq!(meta.name, "new_file.rs");
    }
}
