use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use std::collections::HashMap;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

mod gutter;
mod inline_diff;
mod metadata;
mod pierre;
mod scrollbar;
mod selection;
mod text;
mod theme;
pub use gutter::{GutterCell, LineSign};
use inline_diff::compute_inline_diff_spans;
pub use inline_diff::InlineDiffSpan;
pub use metadata::{FileDiffKind, FileDiffMetadata, HunkContent, HunkMetadata};
pub use pierre::{line_render_spans, RenderCellKind, RenderSpan, SplitLineCell};
pub use scrollbar::{render_scrollbar, SliderGeometry, SliderState, VerticalScrollbar};
pub use selection::{DiffSelection, TextPoint, TextSelection, TextSelectionRange, TextViewport};
use text::{concealed_text, render_full_line, render_segments};
use theme::row_style;
pub use theme::{DiffTheme, DiffThemeName, SyntaxTheme};

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

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
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
        count: u32,
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
            let mut previous_old_end = 0;

            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                let collapsed_before = hunk.old_start.saturating_sub(previous_old_end + 1);
                if collapsed_before > 0 {
                    self.unified_rows.push(RowRef::Collapsed {
                        file_index,
                        count: collapsed_before,
                    });
                    self.split_rows.push(RowRef::Collapsed {
                        file_index,
                        count: collapsed_before,
                    });
                }

                self.unified_rows.push(RowRef::HunkHeader {
                    file_index,
                    hunk_index,
                });
                self.split_rows.push(RowRef::HunkHeader {
                    file_index,
                    hunk_index,
                });

                for line_index in 0..hunk.lines.len() {
                    let line = LineRef {
                        file_index,
                        hunk_index,
                        line_index,
                    };
                    self.unified_rows.push(RowRef::Unified { line });
                }

                let mut line_index = 0;
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
            }
        }
    }

    fn line(&self, line_ref: LineRef) -> &DiffLine {
        &self.files[line_ref.file_index].hunks[line_ref.hunk_index].lines[line_ref.line_index]
    }
}

impl FileDiff {
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
            RowRef::Split { left, right, .. } => [*left, *right].into_iter().flatten().any(|line| {
                line.file_index == file_index
                    && line.hunk_index == hunk_index
                    && line.line_index == line_index
            }),
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
            | RowRef::HunkHeader { .. } => None,
        }
    }

    pub fn selection_target(
        &self,
        mode: DiffMode,
        selection: DiffSelection,
    ) -> Option<DiffLineRangeTarget> {
        let (start, end) = selection.text.normalized();
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

    pub fn selection_text(&self, mode: DiffMode, selection: DiffSelection) -> String {
        let (start, end) = selection.text.normalized();
        let mut lines = Vec::new();

        for row_index in start.row..=end.row {
            let Some(text) = self.row_text_for_selection(mode, row_index, selection.side) else {
                continue;
            };
            let range = selection.text.column_range_on_row(row_index).unwrap_or(TextSelectionRange {
                start: 0,
                end: usize::MAX,
            });
            lines.push(slice_text_columns(text, range.start, range.end));
        }

        lines.join("\n")
    }

    fn row_text_for_selection(
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
            RowRef::FileSeparator
            | RowRef::FileHeader { .. }
            | RowRef::Collapsed { .. }
            | RowRef::HunkHeader { .. } => None,
        }
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

#[derive(Debug, Clone)]
pub struct DiffViewState {
    pub mode: DiffMode,
    pub scroll_x: usize,
    pub scroll_y: usize,
    pub selected_row: usize,
    pub selected_side: DiffSide,
    pub selection: Option<DiffSelection>,
}

impl Default for DiffViewState {
    fn default() -> Self {
        Self {
            mode: DiffMode::Split,
            scroll_x: 0,
            scroll_y: 0,
            selected_row: 0,
            selected_side: DiffSide::Right,
            selection: None,
        }
    }
}

impl DiffViewState {
    pub fn move_selection(&mut self, delta: isize, row_count: usize, viewport_height: usize) {
        if row_count == 0 {
            self.selected_row = 0;
            self.scroll_y = 0;
            return;
        }

        let selected = self
            .selected_row
            .saturating_add_signed(delta)
            .min(row_count.saturating_sub(1));
        self.selected_row = selected;

        if selected < self.scroll_y {
            self.scroll_y = selected;
        } else if selected >= self.scroll_y.saturating_add(viewport_height) {
            self.scroll_y = selected.saturating_sub(viewport_height.saturating_sub(1));
        }
    }

    pub fn start_mouse_selection(
        &mut self,
        row: usize,
        side: DiffSide,
        column: usize,
        row_count: usize,
        viewport_height: usize,
    ) {
        let row = row.min(row_count.saturating_sub(1));
        self.selection = Some(DiffSelection::new(row, side, column));
        self.selected_side = side;
        self.selected_row = row;
        self.keep_selected_visible(row_count, viewport_height);
    }

    pub fn update_mouse_selection(
        &mut self,
        row: usize,
        column: usize,
        row_count: usize,
        viewport_height: usize,
    ) {
        let Some(mut selection) = self.selection else {
            return;
        };
        selection.set_focus(row.min(row_count.saturating_sub(1)), column);
        self.selection = Some(selection);
        self.selected_side = selection.side;
        self.selected_row = selection.focus().row;
        self.keep_selected_visible(row_count, viewport_height);
    }

    pub fn clear_mouse_selection(&mut self) {
        self.selection = None;
    }

    pub fn scroll_horizontal(&mut self, delta: isize) {
        self.scroll_x = self.scroll_x.saturating_add_signed(delta).min(400);
    }

    fn keep_selected_visible(&mut self, row_count: usize, viewport_height: usize) {
        if row_count == 0 {
            self.scroll_y = 0;
            self.selected_row = 0;
            return;
        }
        self.selected_row = self.selected_row.min(row_count.saturating_sub(1));
        if self.selected_row < self.scroll_y {
            self.scroll_y = self.selected_row;
        } else if self.selected_row >= self.scroll_y.saturating_add(viewport_height) {
            self.scroll_y = self
                .selected_row
                .saturating_sub(viewport_height.saturating_sub(1));
        }
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
}

impl<'a> DiffWidget<'a> {
    pub fn new(document: &'a DiffDocument) -> Self {
        Self {
            document,
            theme: DiffTheme::default(),
        }
    }

    pub fn theme(mut self, theme: DiffTheme) -> Self {
        self.theme = theme;
        self
    }
}

impl StatefulWidget for DiffWidget<'_> {
    type State = DiffViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        fill(area, buf, " ", Style::new().bg(self.theme.bg));
        if area.is_empty() {
            return;
        }

        let rows = self.document.rows(state.mode);
        let content_area = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        let scrollbar_area = Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
        let viewport_height = area.height as usize;
        let max_scroll = rows.len().saturating_sub(viewport_height);
        state.scroll_y = state.scroll_y.min(max_scroll);
        state.selected_row = state.selected_row.min(rows.len().saturating_sub(1));

        for (screen_y, row_index) in (state.scroll_y..rows.len())
            .take(viewport_height)
            .enumerate()
        {
            let y = content_area.y + screen_y as u16;
            let selected = state.selection.is_none() && row_index == state.selected_row;
            render_row_ref(
                self.document,
                rows[row_index],
                row_index,
                content_area,
                y,
                buf,
                selected,
                state.selected_side,
                state.selection,
                self.theme,
                state.mode,
                state.scroll_x,
            );
        }

        scrollbar::render_scrollbar(
            scrollbar_area,
            buf,
            rows.len(),
            viewport_height,
            state.scroll_y,
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
            });
            continue;
        }

        if let Some(rest) = raw.strip_prefix("--- ") {
            if let Some(file) = current.as_mut() {
                file.old_path = Some(clean_diff_path(rest));
            }
            continue;
        }

        if let Some(rest) = raw.strip_prefix("+++ ") {
            if let Some(file) = current.as_mut() {
                file.new_path = clean_diff_path(rest);
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
    let Some(mut highlighter) = pierre::PierreHighlighter::new() else {
        return HighlightStats::default();
    };
    let mut stats = HighlightStats::default();

    for file in &mut document.files {
        let language = pierre::language_for_path(&file.new_path);
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

impl SideSource {
    fn line_to_index(&self, line: u32) -> Option<usize> {
        self.line_indices.get(&line).copied()
    }
}

fn collect_side_source(file: &FileDiff, side: DiffSide) -> SideSource {
    let mut text = String::new();
    let mut line_indices = HashMap::new();

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
                }
                text.push_str(line_text);
                line_indices.insert(line_number, line_indices.len());
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
    document: &DiffDocument,
    row: RowRef,
    row_index: usize,
    area: Rect,
    y: u16,
    buf: &mut Buffer,
    selected: bool,
    selected_side: DiffSide,
    selection: Option<DiffSelection>,
    theme: DiffTheme,
    mode: DiffMode,
    scroll_x: usize,
) {
    match row {
        RowRef::FileSeparator => {
            let style = Style::new().fg(Color::Rgb(52, 60, 69)).bg(theme.panel);
            render_full_line(area, y, buf, "", style);
        }
        RowRef::FileHeader { file_index } => {
            let file = &document.files[file_index];
            render_file_header(area, y, buf, file, file_index, document.files.len(), theme);
        }
        RowRef::Collapsed { file_index, count } => {
            render_collapsed_row(
                area,
                y,
                buf,
                document.files[file_index].old_path.as_deref(),
                count,
                theme,
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
            let (old, new, kind, text, syntax_spans, inline_spans) = match diff_line {
                DiffLine::Context {
                    old_line,
                    new_line,
                    text,
                    syntax_spans,
                } => (
                    Some(*old_line),
                    Some(*new_line),
                    RowKind::Context,
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
                    text.as_str(),
                    syntax_spans.as_slice(),
                    inline_spans.as_slice(),
                ),
            };
            let style = row_style(kind, selected, theme);
            let gutter_style = if selected {
                Style::new().fg(Color::White).bg(theme.selected)
            } else {
                Style::new()
                    .fg(theme.muted)
                    .bg(style.bg.unwrap_or(theme.bg))
            };
            let gutter = format!(
                "{} {} ",
                gutter::exact_line_num(old),
                gutter::exact_line_num(new)
            );
            let conceal_first = is_markdown_path(&document.files[line.file_index].new_path);
            let visible_text = concealed_text(text, conceal_first);
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
                None,
                scroll_x,
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
            render_split_cell(
                left_area,
                buf,
                left_cell.as_ref(),
                selected && mode == DiffMode::Split && selected_side == DiffSide::Left,
                selection.and_then(|selection| {
                    selection.column_range_on_side(row_index, DiffSide::Left)
                }),
                theme,
                scroll_x,
            );
            render_split_cell(
                right_area,
                buf,
                right_cell.as_ref(),
                selected && mode == DiffMode::Split && selected_side == DiffSide::Right,
                selection.and_then(|selection| {
                    selection.column_range_on_side(row_index, DiffSide::Right)
                }),
                theme,
                scroll_x,
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
    theme: DiffTheme,
) {
    let style = Style::new().fg(theme.text).bg(theme.panel);
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }

    let name = format!("{}/{} {}", file_index + 1, file_count, file.new_path);
    buf.set_stringn(area.x, y, name, area.width as usize, style);

    let additions = file.additions();
    let deletions = file.deletions();
    let suffix = match (additions, deletions) {
        (0, 0) => "0 ".to_string(),
        (additions, 0) => format!("+{additions} "),
        (0, deletions) => format!("-{deletions} "),
        (additions, deletions) => format!("+{additions} -{deletions} "),
    };
    let suffix_width = suffix.len() as u16;
    if suffix_width < area.width {
        let suffix_x = area.right().saturating_sub(suffix_width);
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
) {
    let _ = old_path;
    let style = Style::new().fg(theme.muted).bg(theme.panel_alt);
    let half = area.width / 2;
    let _ = count;
    let text = "⋯";

    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol(" ").set_style(style);
    }

    if half > 0 {
        buf.set_stringn(area.x, y, &text, half as usize, style);
    }
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
    area: Rect,
    buf: &mut Buffer,
    cell: Option<&Cell<'_>>,
    selected: bool,
    selection_range: Option<TextSelectionRange>,
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
    let style = row_style(cell.kind, selected, theme);
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
        selected,
    );
    let prefix_segments = [
        (gutter.rail, gutter.rail_style),
        (&gutter.line_number, gutter.line_number_style),
        (gutter.sign, gutter.sign_style),
        (gutter.trailing, gutter.line_number_style),
    ];
    let visible_text = concealed_text(cell.text, cell.conceal_first);
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
        selection_range,
        scroll_x,
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
    fn selection_text_extracts_split_side_columns() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n-old beta\n+new alpha\n+new beta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut selection = DiffSelection::new(row, DiffSide::Right, 4);
        selection.set_focus(row + 1, 8);

        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha\nnew beta");
    }

    #[test]
    fn selection_text_normalizes_backward_selection_like_opentui() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n-old beta\n+new alpha\n+new beta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut selection = DiffSelection::new(row + 1, DiffSide::Right, 8);
        selection.set_focus(row, 4);

        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha\nnew beta");
    }

    #[test]
    fn selection_text_extracts_same_line_slice() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut selection = DiffSelection::new(row, DiffSide::Right, 4);
        selection.set_focus(row, 9);

        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha");
    }
}
