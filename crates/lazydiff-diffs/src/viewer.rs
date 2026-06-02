use crate::{
    DiffDocument, DiffLine, DiffMode, DiffOverlayKind, DiffPaneTextLayout, DiffSearchMatch,
    DiffSide, DiffTextPoint, DiffTextSelection, DiffVisualOverlay, TextSelectionRange,
};
use ratatui::layout::Rect;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const YANK_FLASH_DURATION: Duration = Duration::from_millis(550);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffCursor {
    pub row: usize,
    pub side: DiffSide,
    pub col: usize,
    pub goal_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffScreenPosition {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone)]
pub struct DiffRenderModel {
    pub content_area: Rect,
    pub scrollbar_area: Rect,
    pub viewport_height: usize,
    pub visual_row_count: usize,
    pub visual_rows: Vec<DiffVisualRow>,
}

impl Default for DiffCursor {
    fn default() -> Self {
        Self {
            row: 0,
            side: DiffSide::Right,
            col: 0,
            goal_col: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffWordMotion {
    NextStart { big: bool },
    NextEnd { big: bool },
    PreviousStart { big: bool },
    PreviousEnd { big: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffViewport {
    pub mode: DiffMode,
    pub scroll_x: usize,
    pub scroll_x_left: usize,
    pub scroll_x_right: usize,
    pub scroll_y: usize,
    pub width: u16,
    pub height: u16,
    pub top_margin: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffSideBySideRow {
    pub row: usize,
    pub left: Option<usize>,
    pub right: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffVisualRow {
    Document {
        row: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    InlineBlock {
        after_row: usize,
        index: usize,
        line: usize,
    },
}

impl DiffVisualRow {
    pub fn document_row(self) -> Option<usize> {
        match self {
            DiffVisualRow::Document { row, .. } => Some(row),
            DiffVisualRow::InlineBlock { .. } => None,
        }
    }

    pub fn row_for_side(self, side: DiffSide) -> Option<usize> {
        match (self, side) {
            (DiffVisualRow::Document { left, .. }, DiffSide::Left) => left,
            (DiffVisualRow::Document { right, .. }, DiffSide::Right) => right,
            (DiffVisualRow::InlineBlock { .. }, _) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffInlineBlockKind {
    Comment,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffInlineBlockAccent {
    Note,
    Question,
    Instruction,
    Agent,
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffInlineBlock {
    pub id: String,
    pub after_row: usize,
    pub side: DiffSide,
    pub height: usize,
    pub kind: DiffInlineBlockKind,
    pub accent: DiffInlineBlockAccent,
    pub title: String,
    pub body: String,
}

impl Default for DiffViewport {
    fn default() -> Self {
        Self {
            mode: DiffMode::Split,
            scroll_x: 0,
            scroll_x_left: 0,
            scroll_x_right: 0,
            scroll_y: 0,
            width: 0,
            height: 0,
            top_margin: 0,
        }
    }
}

impl DiffViewport {
    pub fn pane_rect(&self, area: Rect, side: DiffSide) -> Rect {
        match self.mode {
            DiffMode::Unified => area,
            DiffMode::Split => {
                let half = area.width / 2;
                match side {
                    DiffSide::Left => Rect::new(area.x, area.y, half, area.height),
                    DiffSide::Right => Rect::new(
                        area.x.saturating_add(half),
                        area.y,
                        area.width.saturating_sub(half),
                        area.height,
                    ),
                }
            }
        }
    }

    pub fn scroll_x_for_side(&self, side: DiffSide) -> usize {
        match self.mode {
            DiffMode::Unified => self.scroll_x,
            DiffMode::Split => match side {
                DiffSide::Left => self.scroll_x_left,
                DiffSide::Right => self.scroll_x_right,
            },
        }
    }

    pub fn scroll_side_horizontally(&mut self, side: DiffSide, delta: isize) {
        match self.mode {
            DiffMode::Unified => self.scroll_x = self.scroll_x.saturating_add_signed(delta),
            DiffMode::Split => match side {
                DiffSide::Left => {
                    self.scroll_x_left = self.scroll_x_left.saturating_add_signed(delta)
                }
                DiffSide::Right => {
                    self.scroll_x_right = self.scroll_x_right.saturating_add_signed(delta)
                }
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiffViewerState {
    pub viewport: DiffViewport,
    pub cursor: DiffCursor,
    pub selection: Option<DiffTextSelection>,
    pub yank_selection: Option<DiffTextSelection>,
    pub yank_until: Option<Instant>,
    pub search: DiffSearchState,
}

#[derive(Debug, Clone, Default)]
pub struct DiffSearchState {
    pub query: String,
    pub matches: Vec<DiffSearchMatch>,
    pub index: Option<usize>,
}

impl DiffViewerState {
    pub fn horizontal_scroll_for_side(&self, side: DiffSide) -> usize {
        self.viewport.scroll_x_for_side(side)
    }

    pub fn active_horizontal_scroll(&self) -> usize {
        self.horizontal_scroll_for_side(self.cursor.side)
    }

    pub fn scroll_active_side_horizontally(&mut self, delta: isize) {
        self.viewport
            .scroll_side_horizontally(self.cursor.side, delta);
    }

    pub fn render_model(
        &mut self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
    ) -> DiffRenderModel {
        let content_area = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        let scrollbar_area = Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
        let viewport_height = area.height as usize;
        let (visual_row_count, mut visual_rows) = self.visible_rows_with_inline_blocks(
            document,
            inline_blocks,
            self.viewport.scroll_y,
            viewport_height,
        );
        let max_scroll = visual_row_count.saturating_sub(viewport_height);
        self.viewport.scroll_y = self.viewport.scroll_y.min(max_scroll);
        if self.viewport.scroll_y == max_scroll && visual_rows.len() < viewport_height {
            let (_, refreshed_rows) = self.visible_rows_with_inline_blocks(
                document,
                inline_blocks,
                self.viewport.scroll_y,
                viewport_height,
            );
            visual_rows = refreshed_rows;
        }
        self.cursor.row = self
            .cursor
            .row
            .min(document.rows(self.viewport.mode).len().saturating_sub(1));
        DiffRenderModel {
            content_area,
            scrollbar_area,
            viewport_height,
            visual_row_count,
            visual_rows,
        }
    }

    pub fn overlays_for_row_side(
        &self,
        document: &DiffDocument,
        area: Rect,
        row: usize,
        side: DiffSide,
        now: Instant,
        search_matches: &[DiffSearchMatch],
    ) -> Vec<DiffVisualOverlay> {
        let Some(layout) = document.pane_text_layout(
            self.viewport.mode,
            row,
            side,
            self.viewport.pane_rect(area, side),
            self.viewport.scroll_x_for_side(side),
        ) else {
            return Vec::new();
        };
        let mut overlays = Vec::new();
        let active_selection = self
            .selection
            .map(|selection| (selection, DiffOverlayKind::Selection))
            .or_else(|| {
                self.yank_until
                    .filter(|until| now < *until)
                    .and(self.yank_selection)
                    .map(|selection| (selection, DiffOverlayKind::Yank))
            });
        if let Some((selection, kind)) = active_selection {
            if let Some(range) = selection.document_column_range_on_side(row, side) {
                overlays.push(DiffVisualOverlay {
                    row,
                    side,
                    range: layout.selection_range_to_visual(range),
                    kind,
                });
            }
        }
        if self.cursor.row == row && self.cursor.side == side {
            overlays.push(DiffVisualOverlay {
                row,
                side,
                range: layout.selection_range_to_visual(TextSelectionRange {
                    start: self.cursor.col,
                    end: self.cursor.col.saturating_add(1),
                }),
                kind: DiffOverlayKind::Cursor,
            });
        }
        overlays.extend(
            search_matches
                .iter()
                .filter(|search_match| search_match.row == row && search_match.side == side)
                .map(|search_match| DiffVisualOverlay {
                    row,
                    side,
                    range: layout.selection_range_to_visual(
                        layout.text_local_range_to_document(search_match.range),
                    ),
                    kind: DiffOverlayKind::Search,
                }),
        );
        overlays
    }

    pub fn side_by_side_rows(&self, document: &DiffDocument) -> Vec<DiffSideBySideRow> {
        let row_count = document.rows(DiffMode::Split).len();
        (0..row_count)
            .map(|row| DiffSideBySideRow {
                row,
                left: document
                    .line_target(DiffMode::Split, row, DiffSide::Left)
                    .map(|_| row),
                right: document
                    .line_target(DiffMode::Split, row, DiffSide::Right)
                    .map(|_| row),
            })
            .collect()
    }

    pub fn visual_rows(&self, document: &DiffDocument) -> Vec<DiffVisualRow> {
        self.visual_rows_with_inline_blocks(document, &[])
    }

    pub fn visual_rows_with_inline_blocks(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
    ) -> Vec<DiffVisualRow> {
        let mut inline_blocks = inline_blocks.iter().enumerate().collect::<Vec<_>>();
        inline_blocks.sort_unstable_by_key(|(_, block)| block.after_row);
        let document_rows = self.document_visual_rows(document);
        let mut visual_rows = Vec::with_capacity(
            document_rows.len()
                + inline_blocks
                    .iter()
                    .map(|(_, block)| block.height)
                    .sum::<usize>(),
        );
        for document_row in document_rows {
            let row = document_row.document_row().unwrap_or(0);
            visual_rows.push(document_row);
            for (index, block) in inline_blocks
                .iter()
                .copied()
                .filter(|(_, block)| block.after_row == row)
            {
                for line in 0..block.height {
                    visual_rows.push(DiffVisualRow::InlineBlock {
                        after_row: block.after_row,
                        index,
                        line,
                    });
                }
            }
        }
        visual_rows
    }

    pub fn visual_row_count_with_inline_blocks(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
    ) -> usize {
        document.rows(self.viewport.mode).len().saturating_add(
            inline_blocks
                .iter()
                .map(|block| block.height)
                .sum::<usize>(),
        )
    }

    pub fn visible_rows_with_inline_blocks(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        start: usize,
        limit: usize,
    ) -> (usize, Vec<DiffVisualRow>) {
        let row_count = document.rows(self.viewport.mode).len();
        let mut inline_blocks = inline_blocks.iter().enumerate().collect::<Vec<_>>();
        inline_blocks.sort_unstable_by_key(|(_, block)| block.after_row);
        let total = row_count.saturating_add(
            inline_blocks
                .iter()
                .map(|(_, block)| block.height)
                .sum::<usize>(),
        );
        if limit == 0 || start >= total {
            return (total, Vec::new());
        }

        let inline_height_before_row = |row: usize| {
            inline_blocks
                .iter()
                .take_while(|(_, block)| block.after_row < row)
                .map(|(_, block)| block.height)
                .sum::<usize>()
        };
        let document_visual_index = |row: usize| row.saturating_add(inline_height_before_row(row));
        let mut low = 0usize;
        let mut high = row_count;
        while low < high {
            let mid = low + (high - low) / 2;
            if document_visual_index(mid) <= start {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let mut row = low.saturating_sub(1);
        let mut visible_rows = Vec::with_capacity(limit.min(total.saturating_sub(start)));

        while row < row_count && visible_rows.len() < limit {
            let mut visual_index = document_visual_index(row);
            if visual_index >= start {
                visible_rows.push(self.document_visual_row(document, row));
                if visible_rows.len() >= limit {
                    break;
                }
            }
            visual_index = visual_index.saturating_add(1);
            for (index, block) in inline_blocks
                .iter()
                .copied()
                .filter(|(_, block)| block.after_row == row)
            {
                for line in 0..block.height {
                    if visual_index >= start {
                        visible_rows.push(DiffVisualRow::InlineBlock {
                            after_row: block.after_row,
                            index,
                            line,
                        });
                        if visible_rows.len() >= limit {
                            break;
                        }
                    }
                    visual_index = visual_index.saturating_add(1);
                }
                if visible_rows.len() >= limit {
                    break;
                }
            }
            row = row.saturating_add(1);
        }

        (total, visible_rows)
    }

    fn document_visual_row(&self, document: &DiffDocument, row: usize) -> DiffVisualRow {
        DiffVisualRow::Document {
            row,
            left: document
                .line_target(self.viewport.mode, row, DiffSide::Left)
                .map(|_| row),
            right: document
                .line_target(self.viewport.mode, row, DiffSide::Right)
                .map(|_| row),
        }
    }

    fn document_visual_rows(&self, document: &DiffDocument) -> Vec<DiffVisualRow> {
        match self.viewport.mode {
            DiffMode::Unified => (0..document.rows(DiffMode::Unified).len())
                .map(|row| DiffVisualRow::Document {
                    row,
                    left: document
                        .line_target(DiffMode::Unified, row, DiffSide::Left)
                        .map(|_| row),
                    right: document
                        .line_target(DiffMode::Unified, row, DiffSide::Right)
                        .map(|_| row),
                })
                .collect(),
            DiffMode::Split => self
                .side_by_side_rows(document)
                .into_iter()
                .map(|row| DiffVisualRow::Document {
                    row: row.row,
                    left: row.left,
                    right: row.right,
                })
                .collect(),
        }
    }

    pub fn visual_index_for_document_row(
        &self,
        document: &DiffDocument,
        row: usize,
    ) -> Option<usize> {
        self.visual_rows(document)
            .iter()
            .position(|visual_row| visual_row.document_row() == Some(row))
    }

    pub fn document_row_for_visual_index(
        &self,
        document: &DiffDocument,
        visual_index: usize,
        side: DiffSide,
    ) -> Option<usize> {
        self.visual_rows(document)
            .get(visual_index)
            .copied()
            .and_then(|visual_row| {
                visual_row
                    .row_for_side(side)
                    .or_else(|| visual_row.document_row())
            })
    }

    pub fn cursor_screen_position(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
    ) -> Option<DiffScreenPosition> {
        if area.is_empty() {
            return None;
        }
        let content_area = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        let visual_rows = self.visual_rows_with_inline_blocks(document, inline_blocks);
        let visual_index = visual_rows
            .iter()
            .position(|visual_row| {
                visual_row.row_for_side(self.cursor.side) == Some(self.cursor.row)
            })
            .or_else(|| {
                visual_rows
                    .iter()
                    .position(|visual_row| visual_row.document_row() == Some(self.cursor.row))
            })?;
        if visual_index < self.viewport.scroll_y {
            return None;
        }
        let local_y = visual_index - self.viewport.scroll_y;
        if local_y >= content_area.height as usize {
            return None;
        }
        if document.is_collapsed_row(self.viewport.mode, self.cursor.row) {
            return Some(DiffScreenPosition {
                x: content_area.x.saturating_add(content_area.width / 2),
                y: content_area.y.saturating_add(local_y as u16),
            });
        }
        let layout = self.pane_text_layout_for_side(document, content_area, self.cursor.side)?;
        if layout.pane.is_empty() {
            return None;
        }
        let x = layout.document_col_to_screen_x(self.cursor.col);
        Some(DiffScreenPosition {
            x,
            y: content_area.y.saturating_add(local_y as u16),
        })
    }

    pub fn focus_row(&mut self, document: &DiffDocument, row: usize) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        self.cursor.row = row.min(row_count.saturating_sub(1));
        self.normalize_cursor_side(document);
        self.clamp_cursor_col(document);
        self.cursor.goal_col = self.cursor.col;
        self.center_cursor_row(document);
        self.update_visual_selection(document);
    }

    pub fn focus_row_ensure_visible(&mut self, document: &DiffDocument, row: usize) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        self.cursor.row = row.min(row_count.saturating_sub(1));
        self.normalize_cursor_side(document);
        self.clamp_cursor_col(document);
        self.cursor.goal_col = self.cursor.col;
        self.ensure_cursor_visible(document);
        self.update_visual_selection(document);
    }

    pub fn focus_row_preserving_view(&mut self, document: &DiffDocument, row: usize) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        self.cursor.row = row.min(row_count.saturating_sub(1));
        self.normalize_cursor_side(document);
        self.clamp_cursor_col(document);
        self.cursor.goal_col = self.cursor.col;
        self.update_visual_selection(document);
    }

    pub fn move_cursor_rows(&mut self, document: &DiffDocument, delta: isize) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        self.prepare_cursor_for_movement(document);
        let next_row = if self.viewport.mode == DiffMode::Split {
            self.next_split_cursor_row(document, delta)
                .unwrap_or(self.cursor.row)
        } else {
            self.cursor
                .row
                .saturating_add_signed(delta)
                .min(row_count.saturating_sub(1))
        };
        self.cursor.row = next_row;
        self.normalize_cursor_side(document);
        let row_end = self
            .cursor_code_range(document)
            .map(|(_, end)| end)
            .unwrap_or(0);
        self.cursor.col = self.cursor.goal_col.min(row_end);
        self.clamp_cursor_col(document);
        self.ensure_cursor_visible(document);
        self.update_visual_selection(document);
    }

    pub fn move_cursor_cols(&mut self, document: &DiffDocument, delta: isize) {
        self.prepare_cursor_for_movement(document);
        let Some((start, end)) = self.cursor_code_range(document) else {
            return;
        };

        let max_col = if end > start { end - 1 } else { start };
        let old = self.cursor.col.max(start).min(max_col);
        let next = old.saturating_add_signed(delta).max(start).min(max_col);
        self.cursor.col = next;
        self.cursor.goal_col = next;
        self.ensure_cursor_col_visible(document);
        self.update_visual_selection(document);
    }

    pub fn switch_side(&mut self, document: &DiffDocument) -> bool {
        if self.viewport.mode != DiffMode::Split {
            return false;
        }
        let next_side = match self.cursor.side {
            DiffSide::Left => DiffSide::Right,
            DiffSide::Right => DiffSide::Left,
        };
        if document
            .line_target(self.viewport.mode, self.cursor.row, next_side)
            .is_none()
        {
            return false;
        }
        let old_code_start = self.cursor_code_start(document);
        let local_col = self.cursor.col.saturating_sub(old_code_start);
        self.cursor.side = next_side;
        let new_code_start = self.cursor_code_start(document);
        self.cursor.col = new_code_start.saturating_add(local_col);
        self.clamp_cursor_col(document);
        self.cursor.goal_col = self.cursor.col;
        self.ensure_cursor_col_visible(document);
        self.update_visual_selection(document);
        true
    }

    pub fn half_page(&mut self, document: &DiffDocument, direction: isize) {
        let half_page = (usize::from(self.viewport.height).max(2) / 2).max(1) as isize;
        self.move_cursor_rows(document, direction.saturating_mul(half_page));
        self.center_cursor_row(document);
    }

    pub fn page(&mut self, document: &DiffDocument, direction: isize) {
        let page = usize::from(self.viewport.height).max(1) as isize;
        self.move_cursor_rows(document, direction.saturating_mul(page));
        self.center_cursor_row(document);
    }

    pub fn top(&mut self, document: &DiffDocument) {
        self.focus_row(document, 0);
    }

    pub fn bottom(&mut self, document: &DiffDocument) {
        let row_count = self.row_count(document);
        self.focus_row(document, row_count.saturating_sub(1));
    }

    pub fn cursor_line_start(&mut self, document: &DiffDocument) {
        if let Some((start, _)) = self.cursor_code_range(document) {
            self.cursor.col = start;
        } else {
            self.cursor.col = 0;
        }
        self.cursor.goal_col = self.cursor.col;
        self.ensure_cursor_visible(document);
        self.update_visual_selection(document);
    }

    pub fn cursor_line_end(&mut self, document: &DiffDocument) {
        if let Some((_, end)) = self.cursor_code_range(document) {
            self.cursor.col = end.saturating_sub(1);
        } else {
            self.cursor.col = 0;
        }
        self.cursor.goal_col = self.cursor.col;
        self.ensure_cursor_visible(document);
        self.update_visual_selection(document);
    }

    pub fn move_word(&mut self, document: &DiffDocument, motion: DiffWordMotion) -> bool {
        self.prepare_cursor_for_movement(document);
        let Some(point) = self.word_motion_target(document, motion) else {
            return false;
        };
        self.cursor.row = point.row;
        self.cursor.side = point.side;
        self.cursor.col = point.column;
        self.cursor.goal_col = point.column;
        self.ensure_cursor_visible(document);
        self.update_visual_selection(document);
        true
    }

    pub fn toggle_mode(&mut self, document: &DiffDocument) {
        self.viewport.mode = self.viewport.mode.toggle();
        self.viewport.scroll_x = 0;
        self.viewport.scroll_x_left = 0;
        self.viewport.scroll_x_right = 0;
        self.viewport.scroll_y = 0;
        self.cursor.row = 0;
        self.cursor.side = DiffSide::Right;
        self.cursor.col = 0;
        self.cursor.goal_col = 0;
        self.selection = None;
        self.clamp_cursor_col(document);
        if self.search.query.trim().is_empty() {
            self.clear_search_matches();
        } else {
            self.recompute_search(document);
        }
    }

    pub fn start_visual_selection(&mut self, document: &DiffDocument) {
        self.clamp_cursor_col(document);
        self.cursor.goal_col = self.cursor.col;
        self.selection = Some(DiffTextSelection::character(self.cursor_point()));
    }

    pub fn start_visual_line_selection(&mut self, document: &DiffDocument) {
        let Some((start, end)) = self.cursor_code_range(document) else {
            return;
        };
        let anchor = DiffTextPoint {
            row: self.cursor.row,
            side: self.cursor.side,
            column: start,
        };
        let cursor = DiffTextPoint {
            row: self.cursor.row,
            side: self.cursor.side,
            column: end.saturating_sub(1),
        };
        let mut selection = DiffTextSelection::line(anchor);
        selection.set_cursor(cursor);
        self.selection = Some(selection);
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn text_point_for_screen_cell(
        &self,
        document: &DiffDocument,
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> Option<DiffTextPoint> {
        self.text_point_for_screen_cell_with_inline_blocks(document, &[], area, column, screen_row)
    }

    pub fn text_point_for_screen_cell_with_inline_blocks(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> Option<DiffTextPoint> {
        if column < area.left()
            || column >= area.right()
            || screen_row < area.top()
            || screen_row >= area.bottom()
        {
            return None;
        }
        let visual_index = self
            .viewport
            .scroll_y
            .saturating_add(screen_row.saturating_sub(area.y) as usize);
        let visual_rows = self.visual_rows_with_inline_blocks(document, inline_blocks);
        let row = visual_rows.get(visual_index)?.document_row()?;

        let side = match self.viewport.mode {
            DiffMode::Unified => {
                if document
                    .line_target(DiffMode::Unified, row, DiffSide::Right)
                    .is_some()
                {
                    DiffSide::Right
                } else {
                    DiffSide::Left
                }
            }
            DiffMode::Split => {
                let half = area.width / 2;
                if column < area.x.saturating_add(half) {
                    DiffSide::Left
                } else {
                    DiffSide::Right
                }
            }
        };
        let pane = self.viewport.pane_rect(area, side);
        let layout = document.pane_text_layout(
            self.viewport.mode,
            row,
            side,
            pane,
            self.viewport.scroll_x_for_side(side),
        )?;
        let text_len = document
            .row_text_for_selection(self.viewport.mode, row, side)?
            .chars()
            .count();
        let code_end = layout.document_code_start.saturating_add(text_len);
        let document_column = layout.screen_x_to_document_col(column, code_end);
        Some(DiffTextPoint {
            row,
            side,
            column: document_column,
        })
    }

    pub fn document_row_for_screen_cell_with_inline_blocks(
        &self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> Option<usize> {
        if column < area.left()
            || column >= area.right()
            || screen_row < area.top()
            || screen_row >= area.bottom()
        {
            return None;
        }
        let visual_index = self
            .viewport
            .scroll_y
            .saturating_add(screen_row.saturating_sub(area.y) as usize);
        self.visual_rows_with_inline_blocks(document, inline_blocks)
            .get(visual_index)?
            .document_row()
    }

    pub fn start_mouse_selection(
        &mut self,
        document: &DiffDocument,
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> bool {
        self.start_mouse_selection_with_inline_blocks(document, &[], area, column, screen_row)
    }

    pub fn start_mouse_selection_with_inline_blocks(
        &mut self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> bool {
        let Some(point) = self.text_point_for_screen_cell_with_inline_blocks(
            document,
            inline_blocks,
            area,
            column,
            screen_row,
        ) else {
            return false;
        };
        self.cursor.row = point.row;
        self.cursor.side = point.side;
        self.cursor.col = point.column;
        self.cursor.goal_col = point.column;
        let mut selection = DiffTextSelection::character(point);
        if self.viewport.mode == DiffMode::Split {
            selection.side_filtered = true;
            selection.side = point.side;
        }
        self.selection = Some(selection);
        true
    }

    pub fn extend_mouse_selection(
        &mut self,
        document: &DiffDocument,
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> bool {
        self.extend_mouse_selection_with_inline_blocks(document, &[], area, column, screen_row)
    }

    pub fn extend_mouse_selection_with_inline_blocks(
        &mut self,
        document: &DiffDocument,
        inline_blocks: &[DiffInlineBlock],
        area: Rect,
        column: u16,
        screen_row: u16,
    ) -> bool {
        let Some(point) = self.text_point_for_screen_cell_with_inline_blocks(
            document,
            inline_blocks,
            area,
            column,
            screen_row,
        ) else {
            return false;
        };
        if self.selection.is_none() {
            return self.start_mouse_selection_with_inline_blocks(
                document,
                inline_blocks,
                area,
                column,
                screen_row,
            );
        }
        self.cursor.row = point.row;
        self.cursor.side = point.side;
        self.cursor.col = point.column;
        self.cursor.goal_col = point.column;
        self.update_visual_selection(document);
        true
    }

    pub fn finish_mouse_selection(&mut self) {
        if self
            .selection
            .is_some_and(|selection| selection.anchor == selection.cursor)
        {
            self.selection = None;
        }
    }

    pub fn selection_for_paint(&self, now: Instant) -> Option<DiffTextSelection> {
        self.selection.or_else(|| {
            self.yank_until
                .filter(|until| now < *until)
                .and(self.yank_selection)
        })
    }

    pub fn flash_yank_selection(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        self.yank_selection = Some(selection);
        self.yank_until = Some(Instant::now() + YANK_FLASH_DURATION);
        self.selection = None;
        true
    }

    pub fn select_text_object(
        &mut self,
        document: &DiffDocument,
        around: bool,
        object: char,
    ) -> bool {
        if object == 'w' {
            return self.select_word_text_object(document, around);
        }
        let Some((open, close)) = text_object_delimiters(object) else {
            return false;
        };
        self.select_delimited_text_object(document, around, open, close)
    }

    pub fn replace_search_matches(&mut self, matches: Vec<DiffSearchMatch>) {
        self.search.matches = matches;
        self.search.index = None;
    }

    pub fn clear_search_matches(&mut self) {
        self.search.matches.clear();
        self.search.index = None;
    }

    pub fn recompute_search(&mut self, document: &DiffDocument) {
        self.search.matches.clear();
        self.search.index = None;
        let query = self.search.query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return;
        }
        for (file_index, file) in document.files.iter().enumerate() {
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                for (line_index, line) in hunk.lines.iter().enumerate() {
                    let (text, side) = match line {
                        DiffLine::Context { text, .. } | DiffLine::Add { text, .. } => {
                            (text, DiffSide::Right)
                        }
                        DiffLine::Delete { text, .. } => (text, DiffSide::Left),
                    };
                    let text_lower = text.to_ascii_lowercase();
                    let Some(row) =
                        document.line_row(self.viewport.mode, file_index, hunk_index, line_index)
                    else {
                        continue;
                    };
                    let mut start = 0;
                    while let Some(offset) = text_lower[start..].find(&query) {
                        let match_start = start + offset;
                        let match_end = match_start + query.len();
                        self.search.matches.push(DiffSearchMatch {
                            row,
                            side,
                            range: TextSelectionRange {
                                start: match_start,
                                end: match_end,
                            },
                        });
                        start = match_end;
                    }
                }
            }
        }
    }

    pub fn move_search_match(&mut self, document: &DiffDocument, delta: isize) -> bool {
        if self.search.matches.is_empty() {
            self.recompute_search(document);
        }
        if self.search.matches.is_empty() {
            return false;
        }
        let len = self.search.matches.len();
        let index = match self.search.index {
            Some(index) if delta < 0 => index.checked_sub(1).unwrap_or(len - 1),
            Some(index) => (index + 1) % len,
            None if delta < 0 => self
                .search
                .matches
                .iter()
                .rposition(|search_match| search_match.row < self.cursor.row)
                .unwrap_or(len - 1),
            None => self
                .search
                .matches
                .iter()
                .position(|search_match| search_match.row >= self.cursor.row)
                .unwrap_or(0),
        };
        self.move_to_search_match(index, document);
        true
    }

    pub fn move_to_search_match(&mut self, index: usize, document: &DiffDocument) {
        let Some(search_match) = self.search.matches.get(index).copied() else {
            return;
        };
        self.search.index = Some(index);
        self.cursor.row = search_match.row;
        self.cursor.side = search_match.side;
        self.normalize_cursor_side(document);
        let match_col = self
            .cursor_code_start(document)
            .saturating_add(search_match.range.start);
        self.cursor.col = match_col;
        self.cursor.goal_col = match_col;
        self.clamp_cursor_col(document);
        self.center_cursor_row(document);
        self.update_visual_selection(document);
    }

    pub fn cursor_code_start(&self, document: &DiffDocument) -> usize {
        document
            .row_code_start(self.viewport.mode, self.cursor.row, self.cursor.side)
            .unwrap_or(0)
    }

    fn row_count(&self, document: &DiffDocument) -> usize {
        document.rows(self.viewport.mode).len()
    }

    fn next_split_cursor_row(&self, document: &DiffDocument, delta: isize) -> Option<usize> {
        if delta == 0 {
            return Some(self.cursor.row);
        }
        let visual_rows = self.visual_rows(document);
        let mut visual_index = self.visual_index_for_document_row(document, self.cursor.row)?;
        loop {
            visual_index = visual_index.saturating_add_signed(delta);
            if visual_index >= visual_rows.len() {
                return None;
            }
            let Some(row) = visual_rows[visual_index].document_row() else {
                continue;
            };
            if document.is_focusable_row(self.viewport.mode, row, self.cursor.side) {
                return Some(row);
            }
            if (delta < 0 && visual_index == 0)
                || (delta > 0 && visual_index == visual_rows.len().saturating_sub(1))
            {
                return None;
            }
        }
    }

    fn reset_position(&mut self) {
        self.viewport.scroll_y = 0;
        self.viewport.scroll_x = 0;
        self.viewport.scroll_x_left = 0;
        self.viewport.scroll_x_right = 0;
        self.cursor = DiffCursor::default();
        self.selection = None;
    }

    fn normalize_cursor_side(&mut self, document: &DiffDocument) {
        if document
            .line_target(self.viewport.mode, self.cursor.row, self.cursor.side)
            .is_some()
        {
            return;
        }
        for side in [DiffSide::Right, DiffSide::Left] {
            if document
                .line_target(self.viewport.mode, self.cursor.row, side)
                .is_some()
            {
                self.cursor.side = side;
                return;
            }
        }
    }

    fn cursor_code_range(&self, document: &DiffDocument) -> Option<(usize, usize)> {
        let text_len = document
            .row_text_for_selection(self.viewport.mode, self.cursor.row, self.cursor.side)?
            .chars()
            .count();
        let start = self.cursor_code_start(document);
        Some((start, start.saturating_add(text_len)))
    }

    fn select_word_text_object(&mut self, document: &DiffDocument, around: bool) -> bool {
        let Some(text) = self.cursor_row_text(document) else {
            return false;
        };
        let Some((code_start, code_end)) = self.cursor_code_range(document) else {
            return false;
        };
        let local_col = self.cursor.col.saturating_sub(code_start);
        let Some((mut start, mut end)) = token_range_at(text, local_col) else {
            return false;
        };
        start = start.min(code_end.saturating_sub(code_start));
        end = end.min(code_end.saturating_sub(code_start));
        if around {
            end = extend_around_word(text, start, end);
        }
        if start >= end {
            return false;
        }
        self.apply_text_object_selection(
            DiffTextPoint {
                row: self.cursor.row,
                side: self.cursor.side,
                column: code_start.saturating_add(start),
            },
            DiffTextPoint {
                row: self.cursor.row,
                side: self.cursor.side,
                column: code_start.saturating_add(end.saturating_sub(1)),
            },
            false,
            false,
            document,
        )
    }

    fn select_delimited_text_object(
        &mut self,
        document: &DiffDocument,
        around: bool,
        open: char,
        close: char,
    ) -> bool {
        let Some(text) = self.cursor_row_text(document) else {
            return false;
        };
        let Some((code_start, code_end)) = self.cursor_code_range(document) else {
            return false;
        };
        let local_col = self
            .cursor
            .col
            .saturating_sub(code_start)
            .min(code_end.saturating_sub(code_start).saturating_sub(1));
        let Some((open_col, close_col)) =
            find_delimited_text_object_on_row(text, local_col, open, close)
        else {
            return false;
        };
        let (start, end) = if around {
            (open_col, close_col)
        } else {
            (
                next_cell_after(text, open_col),
                previous_cell_before(text, close_col),
            )
        };
        if end < start {
            return false;
        }
        self.apply_text_object_selection(
            DiffTextPoint {
                row: self.cursor.row,
                side: self.cursor.side,
                column: code_start.saturating_add(start),
            },
            DiffTextPoint {
                row: self.cursor.row,
                side: self.cursor.side,
                column: code_start.saturating_add(end),
            },
            false,
            false,
            document,
        )
    }

    fn apply_text_object_selection(
        &mut self,
        anchor: DiffTextPoint,
        cursor: DiffTextPoint,
        include_initial_newline: bool,
        include_final_newline: bool,
        document: &DiffDocument,
    ) -> bool {
        let mut selection = DiffTextSelection::character(anchor);
        selection.set_cursor(cursor);
        selection.include_initial_newline = include_initial_newline;
        selection.include_final_newline = include_final_newline;
        if anchor.row != cursor.row {
            selection.side_filtered = true;
            selection.side = self.cursor.side;
        }
        self.selection = Some(selection);
        self.cursor.row = cursor.row;
        self.cursor.side = cursor.side;
        self.cursor.col = cursor.column;
        self.cursor.goal_col = cursor.column;
        self.ensure_cursor_visible(document);
        true
    }

    fn cursor_row_text<'a>(&self, document: &'a DiffDocument) -> Option<&'a str> {
        document.row_text_for_selection(self.viewport.mode, self.cursor.row, self.cursor.side)
    }

    fn word_motion_target(
        &self,
        document: &DiffDocument,
        motion: DiffWordMotion,
    ) -> Option<DiffTextPoint> {
        let row_count = self.row_count(document);
        if row_count == 0 {
            return None;
        }
        match motion {
            DiffWordMotion::NextStart { big } => {
                for row in self.cursor.row..row_count {
                    let Some((code_start, _)) =
                        self.code_range_for_row_side(document, row, self.cursor.side)
                    else {
                        continue;
                    };
                    let text = document.row_text_for_selection(
                        self.viewport.mode,
                        row,
                        self.cursor.side,
                    )?;
                    let local_col = if row == self.cursor.row {
                        self.cursor.col.saturating_sub(code_start)
                    } else {
                        0
                    };
                    if let Some(col) = next_word_start(text, local_col, row == self.cursor.row, big)
                    {
                        return Some(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column: code_start.saturating_add(col),
                        });
                    }
                }
            }
            DiffWordMotion::NextEnd { big } => {
                for row in self.cursor.row..row_count {
                    let Some((code_start, _)) =
                        self.code_range_for_row_side(document, row, self.cursor.side)
                    else {
                        continue;
                    };
                    let text = document.row_text_for_selection(
                        self.viewport.mode,
                        row,
                        self.cursor.side,
                    )?;
                    let local_col = if row == self.cursor.row {
                        self.cursor.col.saturating_sub(code_start)
                    } else {
                        0
                    };
                    if let Some(col) = next_word_end(text, local_col, row == self.cursor.row, big) {
                        return Some(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column: code_start.saturating_add(col),
                        });
                    }
                }
            }
            DiffWordMotion::PreviousStart { big } => {
                for row in (0..=self.cursor.row).rev() {
                    let Some((code_start, code_end)) =
                        self.code_range_for_row_side(document, row, self.cursor.side)
                    else {
                        continue;
                    };
                    let text = document.row_text_for_selection(
                        self.viewport.mode,
                        row,
                        self.cursor.side,
                    )?;
                    let local_col = if row == self.cursor.row {
                        self.cursor.col.saturating_sub(code_start)
                    } else {
                        code_end.saturating_sub(code_start)
                    };
                    if let Some(col) = previous_word_start(text, local_col, big) {
                        return Some(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column: code_start.saturating_add(col),
                        });
                    }
                }
            }
            DiffWordMotion::PreviousEnd { big } => {
                for row in (0..=self.cursor.row).rev() {
                    let Some((code_start, code_end)) =
                        self.code_range_for_row_side(document, row, self.cursor.side)
                    else {
                        continue;
                    };
                    let text = document.row_text_for_selection(
                        self.viewport.mode,
                        row,
                        self.cursor.side,
                    )?;
                    let local_col = if row == self.cursor.row {
                        self.cursor.col.saturating_sub(code_start)
                    } else {
                        code_end.saturating_sub(code_start)
                    };
                    if let Some(col) = previous_word_end(text, local_col, big) {
                        return Some(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column: code_start.saturating_add(col),
                        });
                    }
                }
            }
        }
        None
    }

    fn code_range_for_row_side(
        &self,
        document: &DiffDocument,
        row: usize,
        side: DiffSide,
    ) -> Option<(usize, usize)> {
        document.line_target(self.viewport.mode, row, side)?;
        let text = document.row_text_for_selection(self.viewport.mode, row, side)?;
        let start = document.row_code_start(self.viewport.mode, row, side)?;
        Some((start, start.saturating_add(text_cell_width(text))))
    }

    fn clamp_cursor_col(&mut self, document: &DiffDocument) {
        if let Some((start, end)) = self.cursor_code_range(document) {
            self.cursor.col = self.cursor.col.max(start).min(end);
        } else {
            self.cursor.col = 0;
        }
    }

    fn ensure_cursor_visible(&mut self, document: &DiffDocument) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        let height = usize::from(self.viewport.height).max(1);
        let top_margin = self.viewport.top_margin.min(height.saturating_sub(1));
        self.cursor.row = self.cursor.row.min(row_count.saturating_sub(1));
        if self.cursor.row < self.viewport.scroll_y.saturating_add(top_margin) {
            self.viewport.scroll_y = self.cursor.row.saturating_sub(top_margin);
        } else if self.cursor.row >= self.viewport.scroll_y.saturating_add(height) {
            self.viewport.scroll_y = self.cursor.row.saturating_sub(height.saturating_sub(1));
        }
        self.viewport.scroll_y = self.viewport.scroll_y.min(row_count.saturating_sub(height));
        self.ensure_cursor_col_visible(document);
    }

    fn prepare_cursor_for_movement(&mut self, document: &DiffDocument) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        self.cursor.row = self.cursor.row.min(row_count.saturating_sub(1));
        self.clamp_cursor_col(document);
        let visible = usize::from(self.viewport.height).max(1);
        if self.cursor.row < self.viewport.scroll_y
            || self.cursor.row >= self.viewport.scroll_y.saturating_add(visible)
        {
            self.cursor.row = self.viewport.scroll_y.min(row_count.saturating_sub(1));
            self.normalize_cursor_side(document);
            self.clamp_cursor_col(document);
            self.cursor.goal_col = self.cursor.col;
        }
    }

    fn center_cursor_row(&mut self, document: &DiffDocument) {
        let row_count = self.row_count(document);
        if row_count == 0 {
            self.reset_position();
            return;
        }
        let height = usize::from(self.viewport.height).max(1);
        let center = height / 2;
        self.viewport.scroll_y = self
            .cursor
            .row
            .saturating_sub(center)
            .min(row_count.saturating_sub(height));
        self.ensure_cursor_col_visible(document);
    }

    fn ensure_cursor_col_visible(&mut self, document: &DiffDocument) {
        let side = self.cursor.side;
        let scroll_x = self.viewport.scroll_x_for_side(side);
        let area = Rect::new(0, 0, self.viewport.width, self.viewport.height.max(1));
        let Some(layout) = self.pane_text_layout_for_side(document, area, side) else {
            return;
        };
        let next_scroll_x = layout.scroll_to_reveal(self.cursor.col);
        if next_scroll_x != scroll_x {
            self.set_scroll_x_for_side(side, next_scroll_x);
        }
    }

    fn set_scroll_x_for_side(&mut self, side: DiffSide, scroll_x: usize) {
        match self.viewport.mode {
            DiffMode::Unified => self.viewport.scroll_x = scroll_x,
            DiffMode::Split => match side {
                DiffSide::Left => self.viewport.scroll_x_left = scroll_x,
                DiffSide::Right => self.viewport.scroll_x_right = scroll_x,
            },
        }
    }

    fn pane_text_layout_for_side(
        &self,
        document: &DiffDocument,
        area: Rect,
        side: DiffSide,
    ) -> Option<DiffPaneTextLayout> {
        document.pane_text_layout(
            self.viewport.mode,
            self.cursor.row,
            side,
            self.viewport.pane_rect(area, side),
            self.viewport.scroll_x_for_side(side),
        )
    }

    pub fn update_visual_selection(&mut self, document: &DiffDocument) {
        if let Some(mut selection) = self.selection {
            match selection.mode {
                crate::DiffSelectionMode::Character => {
                    self.clamp_cursor_col(document);
                    self.cursor.goal_col = self.cursor.col;
                    selection.set_cursor(self.cursor_point());
                }
                crate::DiffSelectionMode::Line => {
                    let row = self.cursor.row;
                    if let Some((start, end)) = self.cursor_code_range(document) {
                        let column = if row < selection.anchor.row {
                            start
                        } else {
                            end.saturating_sub(1)
                        };
                        selection.set_cursor(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column,
                        });
                    } else {
                        selection.set_cursor(DiffTextPoint {
                            row,
                            side: self.cursor.side,
                            column: 0,
                        });
                    }
                }
            }
            self.selection = Some(selection);
        }
    }

    fn cursor_point(&self) -> DiffTextPoint {
        DiffTextPoint {
            row: self.cursor.row,
            side: self.cursor.side,
            column: self.cursor.col,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextCell {
    start: usize,
    end: usize,
    ch: char,
    kind: TextCellKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextCellKind {
    Space,
    Word,
    Punctuation,
    Symbol,
}

fn token_range_at(text: &str, col: usize) -> Option<(usize, usize)> {
    let cells = text_cells(text);
    if cells.is_empty() {
        return None;
    }
    let index = cells
        .iter()
        .position(|cell| col < cell.end)
        .unwrap_or_else(|| cells.len().saturating_sub(1));
    let kind = cells[index].kind;
    let mut start = index;
    while start > 0 && cells[start - 1].kind == kind {
        start -= 1;
    }
    let mut end = index + 1;
    while end < cells.len() && cells[end].kind == kind {
        end += 1;
    }
    Some((cells[start].start, cells[end - 1].end))
}

fn next_word_start(text: &str, col: usize, same_row: bool, big: bool) -> Option<usize> {
    let tokens = word_tokens(text, big);
    tokens
        .iter()
        .find(|token| token.start > col || (!same_row && token.start >= col))
        .map(|token| token.start)
}

fn next_word_end(text: &str, col: usize, same_row: bool, big: bool) -> Option<usize> {
    let tokens = word_tokens(text, big);
    tokens
        .iter()
        .find(|token| token.end.saturating_sub(1) > col || (!same_row && token.end > col))
        .map(|token| token.end.saturating_sub(1))
}

fn previous_word_start(text: &str, col: usize, big: bool) -> Option<usize> {
    let tokens = word_tokens(text, big);
    tokens
        .iter()
        .rev()
        .find(|token| token.start < col)
        .map(|token| token.start)
}

fn previous_word_end(text: &str, col: usize, big: bool) -> Option<usize> {
    let tokens = word_tokens(text, big);
    tokens
        .iter()
        .rev()
        .find(|token| token.end.saturating_sub(1) < col)
        .map(|token| token.end.saturating_sub(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WordToken {
    start: usize,
    end: usize,
}

fn word_tokens(text: &str, big: bool) -> Vec<WordToken> {
    let cells = text_cells(text);
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < cells.len() {
        if cells[index].kind == TextCellKind::Space {
            index += 1;
            continue;
        }
        let start = cells[index].start;
        let mut end = cells[index].end;
        let kind = cells[index].kind;
        index += 1;
        while index < cells.len()
            && cells[index].kind != TextCellKind::Space
            && (big || cells[index].kind == kind)
        {
            end = cells[index].end;
            index += 1;
        }
        tokens.push(WordToken { start, end });
    }
    tokens
}

fn extend_around_word(text: &str, mut start: usize, mut end: usize) -> usize {
    let cells = text_cells(text);
    while let Some(cell) = cells.iter().find(|cell| cell.start == end) {
        if cell.kind != TextCellKind::Space {
            break;
        }
        end = cell.end;
    }
    if end == text_cell_width(text) {
        while let Some(cell) = cells.iter().rev().find(|cell| cell.end == start) {
            if cell.kind != TextCellKind::Space {
                break;
            }
            start = cell.start;
        }
    }
    let _ = start;
    end
}

fn text_object_delimiters(object: char) -> Option<(char, char)> {
    Some(match object {
        '(' | ')' => ('(', ')'),
        '[' | ']' => ('[', ']'),
        '{' | '}' => ('{', '}'),
        '<' | '>' => ('<', '>'),
        '"' => ('"', '"'),
        '\'' => ('\'', '\''),
        '`' => ('`', '`'),
        _ => return None,
    })
}

fn find_delimited_text_object_on_row(
    text: &str,
    cursor_col: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    if open == close {
        return find_quote_text_object_on_row(text, cursor_col, open);
    }
    let cells = text_cells(text);
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    for cell in cells {
        if cell.ch == open {
            stack.push(cell.start);
        } else if cell.ch == close {
            if let Some(open_col) = stack.pop() {
                pairs.push((open_col, cell.start));
            }
        }
    }
    pairs
        .iter()
        .copied()
        .filter(|(open_col, close_col)| *open_col <= cursor_col && cursor_col <= *close_col)
        .max_by_key(|(open_col, _)| *open_col)
        .or_else(|| {
            pairs
                .into_iter()
                .filter(|(open_col, _)| cursor_col < *open_col)
                .min_by_key(|(open_col, _)| *open_col)
        })
}

fn find_quote_text_object_on_row(
    text: &str,
    cursor_col: usize,
    quote: char,
) -> Option<(usize, usize)> {
    let quote_cols = text_cells(text)
        .into_iter()
        .filter_map(|cell| (cell.ch == quote).then_some(cell.start))
        .collect::<Vec<_>>();
    for pair in quote_cols.windows(2) {
        let open = pair[0];
        let close = pair[1];
        if open <= cursor_col && cursor_col <= close {
            return Some((open, close));
        }
    }
    quote_cols
        .windows(2)
        .find(|pair| cursor_col < pair[0])
        .map(|pair| (pair[0], pair[1]))
}

fn next_cell_after(text: &str, col: usize) -> usize {
    text_cells(text)
        .into_iter()
        .find(|cell| cell.start == col)
        .map(|cell| cell.end)
        .unwrap_or(col.saturating_add(1))
}

fn previous_cell_before(text: &str, col: usize) -> usize {
    text_cells(text)
        .into_iter()
        .rev()
        .find(|cell| cell.end <= col)
        .map(|cell| cell.start)
        .unwrap_or(col.saturating_sub(1))
}

fn text_cells(text: &str) -> Vec<TextCell> {
    let mut cells = Vec::new();
    let mut col = 0usize;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0).max(1);
        let start = col;
        let end = start + width;
        col = end;
        cells.push(TextCell {
            start,
            end,
            ch,
            kind: text_cell_kind(ch),
        });
    }
    cells
}

fn text_cell_width(text: &str) -> usize {
    text.chars().map(|ch| ch.width().unwrap_or(0).max(1)).sum()
}

fn text_cell_kind(ch: char) -> TextCellKind {
    if ch.is_whitespace() {
        TextCellKind::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        TextCellKind::Word
    } else if ch.is_ascii_punctuation() {
        TextCellKind::Punctuation
    } else {
        TextCellKind::Symbol
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_unified_diff;

    #[test]
    fn visual_g_keeps_selection_side_filtered() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n context\n-old alpha\n+new alpha\n tail\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();

        viewer.start_visual_selection(&document);
        viewer.bottom(&document);

        let selection = viewer.selection.expect("visual selection");
        assert!(selection.side_filtered);
        assert_eq!(selection.side, DiffSide::Right);
        assert!(selection.contains_row_on_side(row, DiffSide::Right));
        assert!(!selection.contains_row_on_side(row, DiffSide::Left));
    }

    #[test]
    fn visual_gg_extends_selection_to_top() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-old alpha\n+new alpha\n three\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = document
            .row_code_start(DiffMode::Split, start_row, DiffSide::Right)
            .unwrap();

        viewer.start_visual_selection(&document);
        viewer.top(&document);

        let selection = viewer.selection.expect("visual selection");
        assert!(selection.side_filtered);
        assert_eq!(selection.side, DiffSide::Right);
        assert_eq!(selection.cursor.row, 0);
    }

    #[test]
    fn visual_line_motion_selects_whole_lines_on_one_side() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-old alpha\n+new alpha\n three\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = document
            .row_code_start(DiffMode::Split, start_row, DiffSide::Right)
            .unwrap();

        viewer.start_visual_line_selection(&document);
        viewer.move_cursor_rows(&document, 1);

        let selection = viewer.selection.expect("visual line selection");
        assert_eq!(selection.side, DiffSide::Right);
        assert_eq!(selection.mode, crate::DiffSelectionMode::Line);
        assert!(selection.contains_row_on_side(start_row, DiffSide::Right));
        assert!(!selection.contains_row_on_side(start_row, DiffSide::Left));
    }

    #[test]
    fn visual_line_jk_keeps_whole_line_selection() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-old alpha\n+new alpha\n three\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let next_row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;

        viewer.start_visual_line_selection(&document);
        viewer.move_cursor_rows(&document, 1);
        viewer.move_cursor_rows(&document, -1);

        let selection = viewer.selection.expect("visual line selection");
        assert_eq!(selection.mode, crate::DiffSelectionMode::Line);
        assert!(selection.contains_row_on_side(start_row, DiffSide::Right));
        assert!(selection.contains_row_on_side(next_row, DiffSide::Right));
        assert!(!selection.contains_row_on_side(start_row, DiffSide::Left));
    }

    #[test]
    fn visual_gg_extends_from_later_rows() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-old alpha\n+new alpha\n three\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 3).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, start_row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start;

        viewer.start_visual_selection(&document);
        viewer.top(&document);

        let selection = viewer.selection.expect("visual selection");
        assert_eq!(viewer.cursor.row, 0);
        assert_eq!(selection.side, DiffSide::Right);
        assert!(selection.contains_row_on_side(start_row, DiffSide::Right));
        assert!(!selection.contains_row_on_side(start_row, DiffSide::Left));
    }

    #[test]
    fn visual_l_extends_forward_one_character() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 4;

        viewer.start_visual_selection(&document);
        viewer.move_cursor_cols(&document, 1);

        let selection = viewer.selection.expect("visual selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "al");
    }

    #[test]
    fn visual_h_extends_backward_one_character() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 5;

        viewer.start_visual_selection(&document);
        viewer.move_cursor_cols(&document, -1);

        let selection = viewer.selection.expect("visual selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "al");
    }

    #[test]
    fn visual_zero_extends_to_line_start() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 4;

        viewer.start_visual_selection(&document);
        viewer.cursor_line_start(&document);

        let selection = viewer.selection.expect("visual selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "new a");
    }

    #[test]
    fn visual_dollar_extends_to_line_end() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 4;

        viewer.start_visual_selection(&document);
        viewer.cursor_line_end(&document);

        let selection = viewer.selection.expect("visual selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha");
    }

    #[test]
    fn visual_yank_flashes_and_clears_active_selection() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 4;
        viewer.start_visual_selection(&document);
        viewer.move_cursor_cols(&document, 1);

        assert!(viewer.flash_yank_selection());
        assert!(viewer.selection.is_none());
        let flash = viewer
            .selection_for_paint(Instant::now())
            .expect("yank flash selection");
        assert_eq!(document.selection_text(DiffMode::Split, flash), "al");
    }

    #[test]
    fn split_l_stays_in_left_pane_at_line_end() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Left;
        viewer.cursor_line_end(&document);
        let col = viewer.cursor.col;

        viewer.move_cursor_cols(&document, 1);

        assert_eq!(viewer.cursor.side, DiffSide::Left);
        assert_eq!(viewer.cursor.col, col);
    }

    #[test]
    fn split_h_stays_in_right_pane_at_line_start() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor_line_start(&document);
        let col = viewer.cursor.col;

        viewer.move_cursor_cols(&document, -1);

        assert_eq!(viewer.cursor.side, DiffSide::Right);
        assert_eq!(viewer.cursor.col, col);
    }

    #[test]
    fn split_horizontal_scroll_is_per_side() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha beta gamma delta epsilon\n+new alpha beta gamma delta epsilon\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.width = 20;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Left;

        viewer.scroll_active_side_horizontally(8);

        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Left), 8);
        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Right), 0);

        assert!(viewer.switch_side(&document));
        viewer.scroll_active_side_horizontally(3);

        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Left), 8);
        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Right), 3);
    }

    #[test]
    fn split_cursor_visibility_scrolls_only_active_side() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha beta gamma delta epsilon\n+new alpha beta gamma delta epsilon\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.width = 20;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + "new alpha beta gamma".len();
        viewer.cursor.goal_col = viewer.cursor.col;

        viewer.move_cursor_cols(&document, 1);

        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Left), 0);
        assert!(viewer.horizontal_scroll_for_side(DiffSide::Right) > 0);
    }

    #[test]
    fn split_line_end_scrolls_until_cursor_character_is_visible() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new alpha beta gamma delta epsilon zeta eta theta\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.width = 30;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor_line_start(&document);

        viewer.cursor_line_end(&document);

        let pane_width = usize::from(viewer.viewport.width / 2);
        let local_code_col = viewer.cursor.col.saturating_sub(code_start);
        let visible_code_col =
            local_code_col.saturating_sub(viewer.horizontal_scroll_for_side(DiffSide::Right));
        assert!(visible_code_col < pane_width.saturating_sub(code_start));
        assert_eq!(viewer.horizontal_scroll_for_side(DiffSide::Left), 0);
    }

    #[test]
    fn split_left_line_end_reaches_last_character() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Left)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.width = 40;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Left;

        viewer.cursor_line_end(&document);

        assert_eq!(viewer.cursor.col, code_start + "old alpha".len() - 1);
    }

    #[test]
    fn tab_switches_split_side_on_same_row() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let left_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Left)
            .unwrap();
        let right_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Left;
        viewer.cursor.col = left_start + 4;

        assert!(viewer.switch_side(&document));

        assert_eq!(viewer.cursor.row, row);
        assert_eq!(viewer.cursor.side, DiffSide::Right);
        assert_eq!(viewer.cursor.col, right_start + 4);
    }

    #[test]
    fn split_vertical_motion_preserves_side_across_delete_only_row() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-old alpha\n three\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let end_row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;

        viewer.move_cursor_rows(&document, 1);

        assert_eq!(viewer.cursor.row, end_row);
        assert_eq!(viewer.cursor.side, DiffSide::Right);
    }

    #[test]
    fn side_by_side_visual_rows_record_side_specific_document_rows() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-old alpha\n three\n",
        );
        let delete_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let viewer = DiffViewerState::default();

        let visual_rows = viewer.visual_rows(&document);
        let visual_index = viewer
            .visual_index_for_document_row(&document, delete_row)
            .expect("visual index");
        let visual_row = visual_rows[visual_index];

        assert_eq!(visual_row.document_row(), Some(delete_row));
        assert_eq!(visual_row.row_for_side(DiffSide::Left), Some(delete_row));
        assert_eq!(visual_row.row_for_side(DiffSide::Right), None);
        assert_eq!(
            viewer.document_row_for_visual_index(&document, visual_index, DiffSide::Left),
            Some(delete_row)
        );
    }

    #[test]
    fn inline_blocks_are_visual_rows_after_target_document_row() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+three\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let viewer = DiffViewerState::default();
        let inline_blocks = [DiffInlineBlock {
            id: "note:1".to_string(),
            after_row: row,
            side: DiffSide::Right,
            height: 2,
            kind: DiffInlineBlockKind::Comment,
            accent: DiffInlineBlockAccent::Note,
            title: "note · pi".to_string(),
            body: "hello".to_string(),
        }];

        let visual_rows = viewer.visual_rows_with_inline_blocks(&document, &inline_blocks);
        let index = visual_rows
            .iter()
            .position(|visual_row| visual_row.document_row() == Some(row))
            .expect("document visual row");

        assert_eq!(
            visual_rows.get(index + 1),
            Some(&DiffVisualRow::InlineBlock {
                after_row: row,
                index: 0,
                line: 0,
            })
        );
        assert_eq!(
            visual_rows.get(index + 2),
            Some(&DiffVisualRow::InlineBlock {
                after_row: row,
                index: 0,
                line: 1,
            })
        );
    }

    #[test]
    fn mouse_mapping_accounts_for_inline_visual_rows() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+three\n four\n",
        );
        let anchor_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let shifted_row = document.line_row(DiffMode::Split, 0, 0, 3).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, shifted_row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 8;
        let inline_blocks = [DiffInlineBlock {
            id: "note:1".to_string(),
            after_row: anchor_row,
            side: DiffSide::Right,
            height: 2,
            kind: DiffInlineBlockKind::Comment,
            accent: DiffInlineBlockAccent::Note,
            title: "note · pi".to_string(),
            body: "hello".to_string(),
        }];
        let area = Rect::new(0, 0, 40, 8);
        let visual_index = viewer
            .visual_rows_with_inline_blocks(&document, &inline_blocks)
            .iter()
            .position(|visual_row| visual_row.document_row() == Some(shifted_row))
            .expect("shifted row visual index");

        let point = viewer
            .text_point_for_screen_cell_with_inline_blocks(
                &document,
                &inline_blocks,
                area,
                20 + code_start as u16,
                visual_index as u16,
            )
            .expect("mouse point");

        assert_eq!(point.row, shifted_row);
        assert_eq!(point.side, DiffSide::Right);
        assert_eq!(point.column, code_start);
    }

    #[test]
    fn cursor_screen_position_accounts_for_inline_visual_rows() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+three\n four\n",
        );
        let anchor_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let cursor_row = document.line_row(DiffMode::Split, 0, 0, 3).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, cursor_row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 8;
        viewer.cursor.row = cursor_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start;
        let inline_blocks = [DiffInlineBlock {
            id: "note:1".to_string(),
            after_row: anchor_row,
            side: DiffSide::Right,
            height: 2,
            kind: DiffInlineBlockKind::Comment,
            accent: DiffInlineBlockAccent::Note,
            title: "note · pi".to_string(),
            body: "hello".to_string(),
        }];
        let area = Rect::new(0, 0, 40, 8);
        let visual_index = viewer
            .visual_rows_with_inline_blocks(&document, &inline_blocks)
            .iter()
            .position(|visual_row| visual_row.document_row() == Some(cursor_row))
            .expect("cursor row visual index");

        let position = viewer
            .cursor_screen_position(&document, &inline_blocks, area)
            .expect("cursor screen position");

        assert_eq!(position.y, visual_index as u16);
        let content_width = area.width.saturating_sub(1);
        assert_eq!(position.x, content_width / 2 + code_start as u16 + 1);
    }

    #[test]
    fn split_cursor_screen_position_is_clipped_to_active_pane() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha beta gamma delta epsilon zeta\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Left)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.width = 40;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Left;
        viewer.cursor.col = code_start + "old alpha beta gamma delta".len();
        let area = Rect::new(0, 0, 40, 10);

        let position = viewer
            .cursor_screen_position(&document, &[], area)
            .expect("cursor position");

        let content_width = area.width.saturating_sub(1);
        assert!(position.x < area.x + content_width / 2);
        assert_eq!(position.x, area.x + content_width / 2 - 1);
    }

    #[test]
    fn mouse_cell_maps_to_split_side_document_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 5;
        let area = Rect::new(0, 0, 40, 5);

        let point = viewer
            .text_point_for_screen_cell(&document, area, 20 + code_start as u16 + 5, row as u16)
            .expect("mouse point");

        assert_eq!(point.row, row);
        assert_eq!(point.side, DiffSide::Right);
        assert_eq!(point.column, code_start + 4);
    }

    #[test]
    fn mouse_drag_selection_uses_viewer_selection_model() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 5;
        let area = Rect::new(0, 0, 40, 5);

        assert!(viewer.start_mouse_selection(
            &document,
            area,
            20 + code_start as u16 + 5,
            row as u16,
        ));
        assert!(viewer.extend_mouse_selection(
            &document,
            area,
            20 + code_start as u16 + 9,
            row as u16,
        ));

        let selection = viewer.selection.expect("mouse selection");
        assert_eq!(selection.side, DiffSide::Right);
        assert!(selection.side_filtered);
        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha");
    }

    #[test]
    fn half_page_down_and_up_center_cursor() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,8 +1,8 @@\n one\n two\n three\n four\n five\n six\n seven\n eight\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 4;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;

        viewer.half_page(&document, 1);
        let down_row = viewer.cursor.row;
        assert!(down_row > start_row);
        assert_eq!(viewer.viewport.scroll_y, down_row.saturating_sub(2));

        viewer.half_page(&document, -1);
        assert_eq!(viewer.cursor.row, start_row);
        assert_eq!(
            viewer.viewport.scroll_y,
            viewer.cursor.row.saturating_sub(2)
        );
    }

    #[test]
    fn visual_word_object_selects_word_at_cursor() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap()
            + 4;

        assert!(viewer.select_text_object(&document, false, 'w'));
        let selection = viewer.selection.expect("word selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "alpha");
    }

    #[test]
    fn around_word_object_includes_trailing_space() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+foo bar\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 1;

        assert!(viewer.select_text_object(&document, true, 'w'));
        let selection = viewer.selection.expect("around word selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "foo ");
    }

    #[test]
    fn bracket_text_objects_select_inner_and_around_ranges() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+call(foo, [bar], baz)\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + "call(f".len();

        assert!(viewer.select_text_object(&document, false, '('));
        let selection = viewer.selection.expect("inner paren selection");
        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "foo, [bar], baz"
        );

        assert!(viewer.select_text_object(&document, true, '('));
        let selection = viewer.selection.expect("around paren selection");
        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "(foo, [bar], baz)"
        );

        viewer.cursor.col = code_start + "call(foo, [b".len();
        assert!(viewer.select_text_object(&document, false, '['));
        let selection = viewer.selection.expect("inner bracket selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "bar");

        assert!(viewer.select_text_object(&document, true, '['));
        let selection = viewer.selection.expect("around bracket selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "[bar]");
    }

    #[test]
    fn brace_and_angle_text_objects_select_inner_and_around_ranges() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+items = {foo: <bar>}\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + "items = {f".len();

        assert!(viewer.select_text_object(&document, false, '{'));
        let selection = viewer.selection.expect("inner brace selection");
        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "foo: <bar>"
        );

        assert!(viewer.select_text_object(&document, true, '{'));
        let selection = viewer.selection.expect("around brace selection");
        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "{foo: <bar>}"
        );

        viewer.cursor.col = code_start + "items = {foo: <b".len();
        assert!(viewer.select_text_object(&document, false, '<'));
        let selection = viewer.selection.expect("inner angle selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "bar");

        assert!(viewer.select_text_object(&document, true, '<'));
        let selection = viewer.selection.expect("around angle selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "<bar>");
    }

    #[test]
    fn quote_text_objects_select_inner_ranges() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+name = \"lazy diff\" + 'pi' + `code`\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + "name = \"lazy".len();

        assert!(viewer.select_text_object(&document, false, '"'));
        let selection = viewer.selection.expect("inner quote selection");
        assert_eq!(
            document.selection_text(DiffMode::Split, selection),
            "lazy diff"
        );

        viewer.cursor.col = code_start + "name = \"lazy diff\" + 'p".len();
        assert!(viewer.select_text_object(&document, false, '\''));
        let selection = viewer.selection.expect("inner single quote selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "pi");

        viewer.cursor.col = code_start + "name = \"lazy diff\" + 'pi' + `c".len();
        assert!(viewer.select_text_object(&document, false, '`'));
        let selection = viewer.selection.expect("inner backtick selection");
        assert_eq!(document.selection_text(DiffMode::Split, selection), "code");
    }

    #[test]
    fn word_motions_follow_vim_word_boundaries() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+alpha.beta gamma\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start;

        assert!(viewer.move_word(&document, DiffWordMotion::NextStart { big: false }));
        assert_eq!(viewer.cursor.col, code_start + "alpha".len());

        assert!(viewer.move_word(&document, DiffWordMotion::NextStart { big: false }));
        assert_eq!(viewer.cursor.col, code_start + "alpha.".len());

        assert!(viewer.move_word(&document, DiffWordMotion::NextEnd { big: false }));
        assert_eq!(viewer.cursor.col, code_start + "alpha.beta".len() - 1);

        assert!(viewer.move_word(&document, DiffWordMotion::PreviousStart { big: false }));
        assert_eq!(viewer.cursor.col, code_start + "alpha.".len());
    }

    #[test]
    fn big_word_motions_treat_non_whitespace_as_one_word() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+alpha.beta gamma\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start;

        assert!(viewer.move_word(&document, DiffWordMotion::NextStart { big: true }));
        assert_eq!(viewer.cursor.col, code_start + "alpha.beta ".len());

        assert!(viewer.move_word(&document, DiffWordMotion::PreviousEnd { big: true }));
        assert_eq!(viewer.cursor.col, code_start + "alpha.beta".len() - 1);
    }

    #[test]
    fn word_motion_crosses_rows_and_extends_visual_selection() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old\n+alpha\n+beta gamma\n",
        );
        let first_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let first_code = document
            .row_code_start(DiffMode::Split, first_row, DiffSide::Right)
            .unwrap();
        let second_row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let second_code = document
            .row_code_start(DiffMode::Split, second_row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = first_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = first_code + "alpha".len() - 1;
        viewer.start_visual_selection(&document);

        assert!(viewer.move_word(&document, DiffWordMotion::NextStart { big: false }));
        assert_eq!(viewer.cursor.row, second_row);
        assert_eq!(viewer.cursor.col, second_code);
        let selection = viewer.selection.expect("visual selection");
        assert!(selection.contains_row_on_side(first_row, DiffSide::Right));
        assert!(selection.contains_row_on_side(second_row, DiffSide::Right));
    }

    #[test]
    fn search_next_lands_on_exact_match_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n+new alpha\n beta gamma\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.search.query = "new".to_string();

        assert!(viewer.move_search_match(&document, 1));
        assert_eq!(viewer.cursor.row, row);
        assert_eq!(viewer.cursor.side, DiffSide::Right);
        assert_eq!(viewer.cursor.col, code_start);
    }

    #[test]
    fn search_previous_lands_on_exact_match_column() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n+new alpha\n beta gamma\n",
        );
        let row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let code_start = document
            .row_code_start(DiffMode::Split, row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = code_start + 8;
        viewer.search.query = "beta".to_string();

        assert!(viewer.move_search_match(&document, -1));
        assert_eq!(viewer.cursor.row, row);
        assert_eq!(viewer.cursor.side, DiffSide::Right);
        assert_eq!(viewer.cursor.col, code_start);
    }

    #[test]
    fn toggle_mode_recomputes_search_matches_for_new_layout() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old alpha\n+new alpha\n",
        );
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.search.query = "new".to_string();
        viewer.recompute_search(&document);
        assert!(viewer.search.matches.iter().all(|search_match| {
            document
                .line_target(DiffMode::Split, search_match.row, search_match.side)
                .is_some()
        }));

        viewer.toggle_mode(&document);

        assert_eq!(viewer.viewport.mode, DiffMode::Unified);
        assert!(!viewer.search.matches.is_empty());
        assert!(viewer.search.matches.iter().all(|search_match| {
            document
                .line_target(DiffMode::Unified, search_match.row, search_match.side)
                .is_some()
        }));
    }

    #[test]
    fn search_in_visual_mode_extends_selection_to_match() {
        let document = parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old alpha\n+new alpha\n beta gamma\n",
        );
        let start_row = document.line_row(DiffMode::Split, 0, 0, 1).unwrap();
        let start_code = document
            .row_code_start(DiffMode::Split, start_row, DiffSide::Right)
            .unwrap();
        let match_row = document.line_row(DiffMode::Split, 0, 0, 2).unwrap();
        let match_code = document
            .row_code_start(DiffMode::Split, match_row, DiffSide::Right)
            .unwrap();
        let mut viewer = DiffViewerState::default();
        viewer.viewport.mode = DiffMode::Split;
        viewer.viewport.height = 10;
        viewer.cursor.row = start_row;
        viewer.cursor.side = DiffSide::Right;
        viewer.cursor.col = start_code;
        viewer.start_visual_selection(&document);
        viewer.search.query = "gamma".to_string();

        assert!(viewer.move_search_match(&document, 1));
        let selection = viewer.selection.expect("visual selection");
        assert_eq!(viewer.cursor.row, match_row);
        assert_eq!(viewer.cursor.col, match_code + 5);
        assert_eq!(selection.cursor.row, match_row);
        assert_eq!(selection.cursor.column, match_code + 5);
        assert_eq!(selection.side, DiffSide::Right);
    }
}
