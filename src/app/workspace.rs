//! Diff Workspace owner seam.
//!
//! This module starts as a private-state scaffold. Follow-up slices move diff
//! interaction state behind this `update(intent) -> effects` door.

use std::collections::HashMap;

use lazydiff_diffs::{
    DiffDocument, DiffInlineBlock, DiffMode, DiffSide, DiffViewerState, DiffVisualRow,
    row_count_for_mode,
};

#[derive(Debug, Default)]
pub(crate) struct DiffWorkspace {
    no_op_updates: u64,
    rows: Vec<WorkspaceVisualRow>,
    diff_rows: Vec<DiffVisualRow>,
    rows_dirty: bool,
    row_height_overrides: HashMap<RowId, u16>,
    row_cache_key: Option<RowCacheKey>,
    row_rebuilds: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffWorkspaceContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffWorkspaceIntent {
    NoOp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffWorkspaceEffect {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RowId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceVisualRow {
    DiffText {
        row: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    InlineReview {
        after_row: usize,
        index: usize,
        line: usize,
    },
    Spacer {
        side: Option<DiffSide>,
    },
    FoldSummary {
        fold_id: u64,
        hidden_count: usize,
        label: String,
        side: Option<DiffSide>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RowCacheKey {
    mode: DiffMode,
    document_rows: usize,
    inline_blocks: Vec<InlineBlockKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InlineBlockKey {
    after_row: usize,
    side: DiffSide,
    height: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffWorkspaceFrame<'a> {
    workspace: &'a DiffWorkspace,
    pub(crate) rows: &'a [WorkspaceVisualRow],
    pub(crate) diff_rows: &'a [DiffVisualRow],
    pub(crate) row_rebuilds: u64,
}

impl DiffWorkspace {
    pub(crate) fn update(
        &mut self,
        intent: DiffWorkspaceIntent,
        _ctx: DiffWorkspaceContext,
    ) -> Vec<DiffWorkspaceEffect> {
        match intent {
            DiffWorkspaceIntent::NoOp => {
                self.no_op_updates = self.no_op_updates.saturating_add(1);
                Vec::new()
            }
        }
    }

    pub(crate) fn frame(
        &mut self,
        document: &DiffDocument,
        viewer: &DiffViewerState,
        inline_blocks: &[DiffInlineBlock],
    ) -> DiffWorkspaceFrame<'_> {
        let row_cache_key = RowCacheKey::new(document, viewer.viewport.mode, inline_blocks);
        if self.rows_dirty || self.row_cache_key.as_ref() != Some(&row_cache_key) {
            self.rebuild_rows(document, viewer, inline_blocks, row_cache_key);
        }
        DiffWorkspaceFrame {
            workspace: self,
            rows: &self.rows,
            diff_rows: &self.diff_rows,
            row_rebuilds: self.row_rebuilds,
        }
    }

    fn rebuild_rows(
        &mut self,
        document: &DiffDocument,
        viewer: &DiffViewerState,
        inline_blocks: &[DiffInlineBlock],
        row_cache_key: RowCacheKey,
    ) {
        self.diff_rows = viewer.visual_rows_with_inline_blocks(document, inline_blocks);
        self.rows = self
            .diff_rows
            .iter()
            .copied()
            .map(WorkspaceVisualRow::from)
            .collect();
        self.row_cache_key = Some(row_cache_key);
        self.rows_dirty = false;
        self.row_rebuilds = self.row_rebuilds.saturating_add(1);
    }
}

impl DiffWorkspaceFrame<'_> {
    fn no_op_updates(&self) -> u64 {
        self.workspace.no_op_updates
    }
}

impl Default for DiffWorkspaceFrame<'_> {
    fn default() -> Self {
        unreachable!("DiffWorkspaceFrame borrows a live DiffWorkspace")
    }
}

impl From<DiffVisualRow> for WorkspaceVisualRow {
    fn from(row: DiffVisualRow) -> Self {
        match row {
            DiffVisualRow::Document { row, left, right } => {
                WorkspaceVisualRow::DiffText { row, left, right }
            }
            DiffVisualRow::InlineBlock {
                after_row,
                index,
                line,
            } => WorkspaceVisualRow::InlineReview {
                after_row,
                index,
                line,
            },
        }
    }
}

impl RowCacheKey {
    fn new(document: &DiffDocument, mode: DiffMode, inline_blocks: &[DiffInlineBlock]) -> Self {
        Self {
            mode,
            document_rows: row_count_for_mode(document, mode),
            inline_blocks: inline_blocks
                .iter()
                .map(|block| InlineBlockKey {
                    after_row: block.after_row,
                    side: block.side,
                    height: block.height,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod skeleton {
    use super::*;
    use lazydiff_diffs::parse_unified_diff;

    #[test]
    fn skeleton_no_op_update_round_trips_to_frame() {
        let mut workspace = DiffWorkspace::default();
        let document = parse_unified_diff("@@ -1 +1 @@\n-old\n+new\n");
        let viewer = DiffViewerState::default();

        let effects = workspace.update(DiffWorkspaceIntent::NoOp, DiffWorkspaceContext);
        let frame = workspace.frame(&document, &viewer, &[]);

        assert!(effects.is_empty());
        assert_eq!(frame.no_op_updates(), 1);
    }
}

#[cfg(test)]
mod rows {
    use super::*;
    use lazydiff_diffs::parse_unified_diff;

    #[test]
    fn consecutive_frames_without_mutation_reuse_cached_rows() {
        let document = parse_unified_diff("@@ -1 +1 @@\n-old\n+new\n");
        let viewer = DiffViewerState::default();
        let mut workspace = DiffWorkspace::default();

        let first = workspace.frame(&document, &viewer, &[]);
        let first_rebuilds = first.row_rebuilds;
        let first_rows = first.rows.to_vec();
        let second = workspace.frame(&document, &viewer, &[]);

        assert_eq!(first_rebuilds, 1);
        assert_eq!(second.row_rebuilds, 1);
        assert_eq!(second.rows, first_rows.as_slice());
    }
}
