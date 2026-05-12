use crate::DiffSide;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPoint {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: TextPoint,
    pub focus: TextPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelectionRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextViewport {
    pub scroll_x: usize,
    pub scroll_y: usize,
}

impl TextSelection {
    pub fn new(anchor: TextPoint) -> Self {
        Self { anchor, focus: anchor }
    }

    pub fn set_focus(&mut self, focus: TextPoint) {
        self.focus = focus;
    }

    pub fn normalized(&self) -> (TextPoint, TextPoint) {
        if (self.anchor.row, self.anchor.column) <= (self.focus.row, self.focus.column) {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    pub fn contains_row(&self, row: usize) -> bool {
        let (start, end) = self.normalized();
        row >= start.row && row <= end.row
    }

    pub fn column_range_on_row(&self, row: usize) -> Option<TextSelectionRange> {
        if !self.contains_row(row) {
            return None;
        }

        let (start, end) = self.normalized();
        let range = if start.row == end.row {
            TextSelectionRange { start: start.column.min(end.column), end: end.column.max(start.column) }
        } else if row == start.row {
            TextSelectionRange { start: start.column, end: usize::MAX }
        } else if row == end.row {
            TextSelectionRange { start: 0, end: end.column }
        } else {
            TextSelectionRange { start: 0, end: usize::MAX }
        };

        (range.start < range.end).then_some(range)
    }

    pub fn document_point_from_local(local_column: usize, local_row: usize, viewport: TextViewport) -> TextPoint {
        TextPoint {
            row: viewport.scroll_y.saturating_add(local_row),
            column: viewport.scroll_x.saturating_add(local_column),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffSelection {
    pub side: DiffSide,
    pub text: TextSelection,
}

impl DiffSelection {
    pub fn new(row: usize, side: DiffSide, column: usize) -> Self {
        Self { side, text: TextSelection::new(TextPoint { row, column }) }
    }

    pub fn focus(&self) -> TextPoint {
        self.text.focus
    }

    pub fn set_focus(&mut self, row: usize, column: usize) {
        self.text.set_focus(TextPoint { row, column });
    }

    pub fn contains_row_on_side(&self, row: usize, side: DiffSide) -> bool {
        self.side == side && self.text.contains_row(row)
    }

    pub fn column_range_on_side(&self, row: usize, side: DiffSide) -> Option<TextSelectionRange> {
        (self.side == side).then(|| self.text.column_range_on_row(row)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_row_selection_is_character_range() {
        let mut selection = TextSelection::new(TextPoint { row: 3, column: 4 });
        selection.set_focus(TextPoint { row: 3, column: 9 });

        assert_eq!(selection.column_range_on_row(3), Some(TextSelectionRange { start: 4, end: 9 }));
        assert_eq!(selection.column_range_on_row(2), None);
    }

    #[test]
    fn reverse_same_row_selection_normalizes() {
        let mut selection = TextSelection::new(TextPoint { row: 3, column: 9 });
        selection.set_focus(TextPoint { row: 3, column: 4 });

        assert_eq!(selection.column_range_on_row(3), Some(TextSelectionRange { start: 4, end: 9 }));
    }

    #[test]
    fn multi_row_selection_uses_open_ended_middle_rows() {
        let mut selection = TextSelection::new(TextPoint { row: 1, column: 3 });
        selection.set_focus(TextPoint { row: 4, column: 7 });

        assert_eq!(selection.column_range_on_row(1), Some(TextSelectionRange { start: 3, end: usize::MAX }));
        assert_eq!(selection.column_range_on_row(2), Some(TextSelectionRange { start: 0, end: usize::MAX }));
        assert_eq!(selection.column_range_on_row(4), Some(TextSelectionRange { start: 0, end: 7 }));
        assert_eq!(selection.column_range_on_row(5), None);
    }

    #[test]
    fn local_points_include_viewport_offset_like_opentui() {
        let point = TextSelection::document_point_from_local(5, 2, TextViewport { scroll_x: 10, scroll_y: 20 });

        assert_eq!(point, TextPoint { row: 22, column: 15 });
    }
}
