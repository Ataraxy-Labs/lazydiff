use std::io::Write as _;
use std::process::{Command, Stdio};

use super::*;

impl App {
    pub(super) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        let rows = row_count_for_mode(&self.document, self.diff_buffer.viewer().viewport.mode);
        if self.comment_modal.is_some() || self.thread_modal.is_some() {
            return;
        }
        if self.semantic_dragging_scrollbar {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((_route, area)) =
                        self.semantic_mouse_target_area(terminal_width, terminal_height)
                    {
                        self.scroll_semantic_viewport_to(mouse.row, area);
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.semantic_dragging_scrollbar = false;
                    return;
                }
                _ => {}
            }
        }
        if self.active_scrollbar_drag.is_some() {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    self.drag_active_scrollbar_to(mouse.row, terminal_width, terminal_height);
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.active_scrollbar_drag = None;
                    return;
                }
                _ => {}
            }
        }
        if self.file_picker_open
            && self.handle_file_picker_mouse(mouse, terminal_width, terminal_height)
        {
            return;
        }
        if self.surface == AppSurface::Diff {
            match mouse.kind {
                MouseEventKind::ScrollLeft => {
                    self.scroll_active_pane_horizontally(-8);
                    return;
                }
                MouseEventKind::ScrollRight => {
                    self.scroll_active_pane_horizontally(8);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_relative(1, rows);
                    return;
                }
                MouseEventKind::ScrollUp => {
                    self.scroll_relative(-1, rows);
                    return;
                }
                _ => {}
            }
        }
        let over_main_scrollbar =
            self.is_on_main_scrollbar(mouse.column, mouse.row, terminal_width, terminal_height);
        let scrollbar_target =
            self.scrollbar_target_at(mouse.column, mouse.row, terminal_width, terminal_height);
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            self.focus_pane_at_mouse(mouse.column, mouse.row, terminal_width, terminal_height);
        }
        if let Some(target) = scrollbar_target
            .filter(|_| matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)))
        {
            self.start_scrollbar_drag(target, mouse.row, terminal_width, terminal_height);
            return;
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.handle_semantic_mouse_down(mouse, terminal_width, terminal_height)
        {
            return;
        }
        if matches!(mouse.kind, MouseEventKind::Moved)
            && let Some((route, semantic_area)) =
                self.semantic_mouse_target_area(terminal_width, terminal_height)
            && self.select_semantic_node_at(&route, semantic_area, mouse.column, mouse.row)
        {
            return;
        }
        if let Some((_route, semantic_area)) =
            self.semantic_mouse_target_area(terminal_width, terminal_height)
            && contains_point(semantic_area, mouse.column, mouse.row)
        {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.zoom_semantic_map_at(semantic_area, mouse.column, mouse.row, 1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.zoom_semantic_map_at(semantic_area, mouse.column, mouse.row, -1);
                    return;
                }
                MouseEventKind::ScrollLeft => {
                    self.pan_semantic_map(8, 0);
                    return;
                }
                MouseEventKind::ScrollRight => {
                    self.pan_semantic_map(-8, 0);
                    return;
                }
                _ => {}
            }
        }
        match (self.surface, mouse.kind) {
            (AppSurface::Queue, MouseEventKind::Down(MouseButton::Left)) => {
                if self.handle_home_queue_click(mouse, terminal_width, terminal_height) {
                    return;
                }
            }
            (AppSurface::CommitList, MouseEventKind::ScrollDown) => {
                self.move_commit_selection(1);
                return;
            }
            (AppSurface::CommitList, MouseEventKind::ScrollUp) => {
                self.move_commit_selection(-1);
                return;
            }
            (AppSurface::Queue, MouseEventKind::ScrollDown) => {
                if self
                    .home_detail_area(terminal_width, terminal_height)
                    .is_some_and(|area| contains_point(area, mouse.column, mouse.row))
                {
                    if self.semantic_panel_active() {
                        self.scroll_semantic_tree(1);
                    } else {
                        self.surface_scroll_y = self.surface_scroll_y.saturating_add(1);
                    }
                } else {
                    self.move_home_selection(1);
                }
                return;
            }
            (AppSurface::Queue, MouseEventKind::ScrollUp) => {
                if self
                    .home_detail_area(terminal_width, terminal_height)
                    .is_some_and(|area| contains_point(area, mouse.column, mouse.row))
                {
                    if self.semantic_panel_active() {
                        self.scroll_semantic_tree(-1);
                    } else {
                        self.surface_scroll_y = self.surface_scroll_y.saturating_sub(1);
                    }
                } else {
                    self.move_home_selection(-1);
                }
                return;
            }
            (AppSurface::DetailFull, MouseEventKind::ScrollDown) => {
                if self.semantic_panel_active() {
                    self.scroll_semantic_tree(1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_add(1);
                }
                return;
            }
            (AppSurface::DetailFull, MouseEventKind::ScrollUp) => {
                if self.semantic_panel_active() {
                    self.scroll_semantic_tree(-1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_sub(1);
                }
                return;
            }
            (AppSurface::Comments, MouseEventKind::ScrollDown) => {
                self.move_comments_selection(1);
                return;
            }
            (AppSurface::Comments, MouseEventKind::ScrollUp) => {
                self.move_comments_selection(-1);
                return;
            }
            _ => {}
        }
        match mouse.kind {
            MouseEventKind::ScrollLeft => {
                self.scroll_active_pane_horizontally(-8);
            }
            MouseEventKind::ScrollRight => {
                self.scroll_active_pane_horizontally(8);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_relative(1, rows);
            }
            MouseEventKind::ScrollUp => {
                self.scroll_relative(-1, rows);
            }
            MouseEventKind::Down(MouseButton::Left) if over_main_scrollbar => {
                self.selecting_text = false;
                self.pending_screen_selection = None;
                self.screen_selection = None;
                self.dragging_scrollbar = true;
                let body_row = mouse.row.saturating_sub(main_body_top()) as usize;
                if self.is_in_scrollbar_thumb(body_row, rows) {
                    self.scrollbar_drag_offset_virtual =
                        self.scrollbar_drag_offset_virtual(body_row, rows);
                } else {
                    self.jump_scrollbar_to(mouse.row, rows);
                    self.scrollbar_drag_offset_virtual =
                        self.scrollbar_drag_offset_virtual(body_row, rows);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_scrollbar => {
                if self.is_in_main_body(mouse.row, terminal_height) {
                    self.drag_scrollbar_to(mouse.row, rows);
                }
            }
            MouseEventKind::Drag(MouseButton::Left)
                if self.surface == AppSurface::Diff
                    && self.diff_buffer.viewer().selection.is_some() =>
            {
                if self.extend_diff_mouse_selection(mouse, terminal_width, terminal_height) {}
            }
            MouseEventKind::Drag(MouseButton::Left)
                if self.selecting_text || self.pending_screen_selection.is_some() =>
            {
                if !self.selecting_text {
                    self.start_pending_screen_text_selection();
                }
                self.update_screen_text_selection(mouse);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.dragging_scrollbar = false;
                if self.start_diff_mouse_selection(mouse, terminal_width, terminal_height) {
                    return;
                }
                self.prepare_screen_text_selection(mouse, terminal_width, terminal_height);
            }
            MouseEventKind::Up(MouseButton::Left)
                if self.surface == AppSurface::Diff
                    && self.diff_buffer.viewer().selection.is_some() =>
            {
                self.finish_diff_mouse_selection();
                self.dragging_scrollbar = false;
                self.selecting_text = false;
                self.text_selection_dragged = false;
                self.pending_screen_selection = None;
            }
            MouseEventKind::Up(MouseButton::Left) if self.selecting_text => {
                if self.text_selection_dragged {
                    self.copy_screen_text_selection_to_clipboard();
                } else {
                    self.screen_selection = None;
                    self.screen_selection_bounds = None;
                }
                self.dragging_scrollbar = false;
                self.selecting_text = false;
                self.text_selection_dragged = false;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging_scrollbar = false;
                self.selecting_text = false;
                self.text_selection_dragged = false;
                self.pending_screen_selection = None;
            }
            _ => {}
        }
    }

    fn diff_body_area_for_terminal(&self, terminal_width: u16, terminal_height: u16) -> Rect {
        let full = Rect::new(0, 0, terminal_width, terminal_height);
        let area = app_content_area(full);
        let [_header, _divider, body, _footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let (_sidebar, _sidebar_divider, diff_body) = self.diff_sidebar_layout(body);
        diff_body
    }

    fn start_diff_mouse_selection(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        if self.surface != AppSurface::Diff || self.review_sidebar_focus {
            return false;
        }
        let area = self.diff_body_area_for_terminal(terminal_width, terminal_height);
        if !contains_point(area, mouse.column, mouse.row) {
            return false;
        }
        let inline_blocks = self.diff_inline_blocks();
        let started = self
            .diff_buffer
            .viewer_mut()
            .start_mouse_selection_with_inline_blocks(
                &self.document,
                &inline_blocks,
                area,
                mouse.column,
                mouse.row,
            );
        if started {
            self.selecting_text = false;
            self.pending_screen_selection = None;
            self.screen_selection = None;
            self.screen_selection_bounds = None;
            self.diff_buffer.clear_transient();
        }
        started
    }

    fn extend_diff_mouse_selection(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        let area = self.diff_body_area_for_terminal(terminal_width, terminal_height);
        if !contains_point(area, mouse.column, mouse.row) {
            return false;
        }
        let inline_blocks = self.diff_inline_blocks();

        self.diff_buffer
            .viewer_mut()
            .extend_mouse_selection_with_inline_blocks(
                &self.document,
                &inline_blocks,
                area,
                mouse.column,
                mouse.row,
            )
    }

    fn finish_diff_mouse_selection(&mut self) {
        self.diff_buffer.viewer_mut().finish_mouse_selection();
    }

    fn handle_home_queue_click(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        let Some(rows) = self.home_wide_queue_rows(terminal_width, terminal_height) else {
            return false;
        };
        let geometries: Vec<_> = rows
            .iter()
            .map(|row| match row {
                GroupedWorkItemRow::Header { geometry, .. }
                | GroupedWorkItemRow::Item { geometry, .. } => *geometry,
            })
            .collect();
        let Some(hit) = list_row_at(&geometries, mouse.column, mouse.row) else {
            return false;
        };
        match hit.kind {
            ListRowKind::Item(index) => {
                let items_len = self.home_work_items().len();
                self.home_selection = index.min(items_len.saturating_sub(1));
                self.home_selection_changed_at = Instant::now();
                self.surface_scroll_y = 0;
                self.semantic_scroll_y = 0;
                self.semantic_selection = 0;
                self.revalidate_selected_semantic_diff();
                true
            }
            ListRowKind::Header | ListRowKind::Gap => true,
        }
    }

    pub(super) fn handle_file_picker_mouse(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        if self.finder_kind == FinderKind::Text {
            let area = centered_rect(Rect::new(0, 0, terminal_width, terminal_height), 104, 24);
            let list_area = Rect::new(
                area.x + 2,
                area.y + 4,
                area.width.saturating_sub(4),
                area.height.saturating_sub(6),
            );
            if contains_point(list_area, mouse.column, mouse.row) {
                let results = self.filtered_text_results();
                let rows = text_search_rows(&results);
                let selected_row =
                    text_search_selected_row(&rows, self.file_picker_selection).unwrap_or(0);
                let start =
                    text_search_list_start(list_area.height as usize, rows.len(), selected_row);
                match mouse.kind {
                    MouseEventKind::ScrollDown => {
                        self.file_picker_selection = self
                            .file_picker_selection
                            .saturating_add(3)
                            .min(results.len().saturating_sub(1));
                    }
                    MouseEventKind::ScrollUp => {
                        self.file_picker_selection = self.file_picker_selection.saturating_sub(3);
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        let row = mouse.row.saturating_sub(list_area.y) as usize;
                        if let Some(TextSearchRow::Match { result_index }) =
                            rows.get(start.saturating_add(row)).copied()
                        {
                            self.file_picker_selection = result_index;
                        }
                    }
                    _ => {}
                }
            }
            return true;
        }
        let Some((_area, list_area, preview_area)) =
            self.file_picker_areas(Rect::new(0, 0, terminal_width, terminal_height))
        else {
            return false;
        };
        let over_list = contains_point(list_area, mouse.column, mouse.row);
        let over_preview =
            preview_area.is_some_and(|area| contains_point(area, mouse.column, mouse.row));

        match mouse.kind {
            MouseEventKind::ScrollDown if over_list => {
                let filtered_len = self.filtered_file_indices().len();
                self.file_picker_selection = self
                    .file_picker_selection
                    .saturating_add(3)
                    .min(filtered_len.saturating_sub(1));
                self.file_picker_preview_scroll = 0;
                true
            }
            MouseEventKind::ScrollUp if over_list => {
                self.file_picker_selection = self.file_picker_selection.saturating_sub(3);
                self.file_picker_preview_scroll = 0;
                true
            }
            MouseEventKind::ScrollDown if over_preview => {
                self.scroll_file_picker_preview(3);
                true
            }
            MouseEventKind::ScrollUp if over_preview => {
                self.scroll_file_picker_preview(-3);
                true
            }
            MouseEventKind::Down(MouseButton::Left) if over_list => {
                let filtered_len = self.filtered_file_indices().len();
                let start = self.file_picker_list_start(list_area.height as usize, filtered_len);
                let rows = list_item_rows(
                    Rect::new(
                        list_area.x,
                        list_area.y,
                        list_area.width.saturating_sub(1),
                        list_area.height,
                    ),
                    start,
                    filtered_len,
                );
                if let Some(hit) = list_row_at(&rows, mouse.column, mouse.row)
                    && let ListRowKind::Item(index) = hit.kind
                {
                    self.file_picker_selection = index.min(filtered_len.saturating_sub(1));
                }
                self.file_picker_preview_scroll = 0;
                true
            }
            _ => true,
        }
    }

    pub(super) fn handle_semantic_mouse_down(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        let scrollbar_hit = |area: Rect, column: u16, row: u16| {
            let body = semantic_tree_body_area(area);
            body.width > 0
                && body.height > 0
                && column == body.right().saturating_sub(1)
                && row >= body.y
                && row < body.bottom()
        };
        match self.surface {
            AppSurface::Queue => {
                let Some(details) = self.home_detail_area(terminal_width, terminal_height) else {
                    return false;
                };
                if !contains_point(details, mouse.column, mouse.row) {
                    return false;
                }
                let items = self.home_work_items();
                let Some(selected) =
                    items.get(self.home_selection.min(items.len().saturating_sub(1)))
                else {
                    return false;
                };
                let content = Rect::new(
                    details.x.saturating_add(1),
                    details.y,
                    details.width.saturating_sub(2),
                    details.height,
                );
                let tab_y = self.home_semantic_tree_start_y(content, selected);
                if mouse.row == tab_y {
                    if mouse.column >= content.x.saturating_add(25) {
                        self.set_detail_tab(DetailTab::Graph);
                    } else if mouse.column >= content.x.saturating_add(11) {
                        self.set_detail_tab(DetailTab::Description);
                    } else {
                        self.set_detail_tab(DetailTab::Semantic);
                    }
                    return true;
                }
                if !self.semantic_panel_active() {
                    return false;
                }
                let route = selected.route(self);
                let semantic_area = Rect::new(
                    content.x,
                    tab_y.saturating_add(1),
                    content.width,
                    content.bottom().saturating_sub(tab_y.saturating_add(1)),
                );
                if scrollbar_hit(semantic_area, mouse.column, mouse.row) {
                    self.semantic_dragging_scrollbar = true;
                    self.semantic_scrollbar_drag_offset_virtual =
                        self.semantic_scrollbar_drag_offset(mouse.row, semantic_area);
                    self.scroll_semantic_viewport_to(mouse.row, semantic_area);
                    return true;
                }
                self.handle_semantic_tree_click(route, semantic_area, mouse.column, mouse.row)
            }
            AppSurface::DetailFull => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, _divider, body, _footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                if !contains_point(body, mouse.column, mouse.row) {
                    return false;
                }
                let items = self.home_work_items();
                let Some(selected) =
                    items.get(self.home_selection.min(items.len().saturating_sub(1)))
                else {
                    return false;
                };
                let content = Rect::new(
                    body.x.saturating_add(1),
                    body.y,
                    body.width.saturating_sub(2),
                    body.height,
                );
                let tab_y = self.home_semantic_tree_start_y(content, selected);
                if mouse.row == tab_y {
                    if mouse.column >= content.x.saturating_add(25) {
                        self.set_detail_tab(DetailTab::Graph);
                    } else if mouse.column >= content.x.saturating_add(11) {
                        self.set_detail_tab(DetailTab::Description);
                    } else {
                        self.set_detail_tab(DetailTab::Semantic);
                    }
                    return true;
                }
                if !self.semantic_panel_active() {
                    return false;
                }
                let route = selected.route(self);
                let semantic_area = Rect::new(
                    content.x,
                    tab_y.saturating_add(1),
                    content.width,
                    content.bottom().saturating_sub(tab_y.saturating_add(1)),
                );
                if scrollbar_hit(semantic_area, mouse.column, mouse.row) {
                    self.semantic_dragging_scrollbar = true;
                    self.semantic_scrollbar_drag_offset_virtual =
                        self.semantic_scrollbar_drag_offset(mouse.row, semantic_area);
                    self.scroll_semantic_viewport_to(mouse.row, semantic_area);
                    return true;
                }
                self.handle_semantic_tree_click(route, semantic_area, mouse.column, mouse.row)
            }
            AppSurface::CommitList => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, body, _footer] = Layout::vertical([
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                if body.width < 96 {
                    return false;
                }
                let [_list, _gap, meta] = Layout::horizontal([
                    Constraint::Percentage(56),
                    Constraint::Length(2),
                    Constraint::Fill(1),
                ])
                .areas(body);
                if !contains_point(meta, mouse.column, mouse.row) {
                    return false;
                }
                let Some(source) = self.current_semantic_route() else {
                    return false;
                };
                let semantic_area = Rect::new(
                    meta.x,
                    meta.y.saturating_add(4),
                    meta.width,
                    meta.bottom().saturating_sub(meta.y.saturating_add(4)),
                );
                if scrollbar_hit(semantic_area, mouse.column, mouse.row) {
                    self.semantic_dragging_scrollbar = true;
                    self.semantic_scrollbar_drag_offset_virtual =
                        self.semantic_scrollbar_drag_offset(mouse.row, semantic_area);
                    self.scroll_semantic_viewport_to(mouse.row, semantic_area);
                    return true;
                }
                self.handle_semantic_tree_click(source, semantic_area, mouse.column, mouse.row)
            }
            AppSurface::Comments | AppSurface::Diff => false,
        }
    }

    pub(super) fn semantic_mouse_target_area(
        &self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<(DiffSource, Rect)> {
        match self.surface {
            AppSurface::Queue => {
                let details = self.home_detail_area(terminal_width, terminal_height)?;
                let items = self.home_work_items();
                let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
                if !self.semantic_panel_active() {
                    return None;
                }
                let content = Rect::new(
                    details.x.saturating_add(1),
                    details.y,
                    details.width.saturating_sub(2),
                    details.height,
                );
                let tab_y = self.home_semantic_tree_start_y(content, selected);
                Some((
                    selected.route(self),
                    Rect::new(
                        content.x,
                        tab_y.saturating_add(1),
                        content.width,
                        content.bottom().saturating_sub(tab_y.saturating_add(1)),
                    ),
                ))
            }
            AppSurface::DetailFull => {
                if !self.semantic_panel_active() {
                    return None;
                }
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, _divider, body, _footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                let items = self.home_work_items();
                let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
                let content = Rect::new(
                    body.x.saturating_add(1),
                    body.y,
                    body.width.saturating_sub(2),
                    body.height,
                );
                let tab_y = self.home_semantic_tree_start_y(content, selected);
                Some((
                    selected.route(self),
                    Rect::new(
                        content.x,
                        tab_y.saturating_add(1),
                        content.width,
                        content.bottom().saturating_sub(tab_y.saturating_add(1)),
                    ),
                ))
            }
            AppSurface::CommitList => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, body, _footer] = Layout::vertical([
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                if body.width < 96 {
                    return None;
                }
                let [_list, _gap, meta] = Layout::horizontal([
                    Constraint::Percentage(56),
                    Constraint::Length(2),
                    Constraint::Fill(1),
                ])
                .areas(body);
                self.current_semantic_route().map(|route| {
                    (
                        route,
                        Rect::new(
                            meta.x,
                            meta.y.saturating_add(4),
                            meta.width,
                            meta.bottom().saturating_sub(meta.y.saturating_add(4)),
                        ),
                    )
                })
            }
            AppSurface::Comments | AppSurface::Diff => None,
        }
    }

    pub(super) fn scroll_file_picker_preview(&mut self, delta: isize) {
        let filtered = self.filtered_file_indices();
        let Some(file_index) = filtered.get(self.file_picker_selection).copied() else {
            self.file_picker_preview_scroll = 0;
            return;
        };
        let max = file_preview_row_count(&self.document.files[file_index]);
        self.file_picker_preview_scroll = self
            .file_picker_preview_scroll
            .saturating_add_signed(delta)
            .min(max.saturating_sub(1));
    }

    pub(super) fn file_picker_areas(&self, frame_area: Rect) -> Option<(Rect, Rect, Option<Rect>)> {
        let area = centered_rect(frame_area, 104, 24);
        if area.width < 4 || area.height < 4 {
            return None;
        }
        let list_top = area.y + 4;
        let content_height = area.height.saturating_sub(6);
        let has_preview = area.width >= 84;
        let list_width = if has_preview {
            (area.width.saturating_mul(2) / 5).clamp(36, 44)
        } else {
            area.width.saturating_sub(4)
        };
        let list_area = Rect::new(area.x + 2, list_top, list_width, content_height);
        let preview_area = has_preview.then(|| {
            Rect::new(
                area.x + 2 + list_width + 2,
                list_top,
                area.width.saturating_sub(list_width + 6),
                content_height,
            )
        });
        Some((area, list_area, preview_area))
    }

    fn prepare_screen_text_selection(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        let anchor = ScreenPoint {
            x: mouse.column,
            y: mouse.row,
        };
        if !self.screen_point_is_text(anchor) {
            self.pending_screen_selection = None;
            return;
        }
        self.pending_screen_selection = Some((
            anchor,
            Some(self.selection_pane_bounds(mouse, terminal_width, terminal_height)),
        ));
    }

    fn focus_pane_at_mouse(
        &mut self,
        column: u16,
        row: u16,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        match self.surface {
            AppSurface::Queue => {
                if self
                    .home_detail_area(terminal_width, terminal_height)
                    .is_some_and(|area| contains_point(area, column, row))
                {
                    self.queue_focus = QueuePane::Detail;
                } else if self
                    .home_wide_queue_area(terminal_width, terminal_height)
                    .is_some_and(|area| contains_point(area, column, row))
                {
                    self.queue_focus = QueuePane::List;
                }
            }
            AppSurface::Diff => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, _divider, body, _footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                let (sidebar, _sidebar_divider, diff_body) = self.diff_sidebar_layout(body);
                if sidebar.is_some_and(|area| contains_point(area, column, row)) {
                    self.set_diff_focus(DiffFocusPane::Sidebar);
                } else if contains_point(diff_body, column, row) {
                    self.set_diff_focus(self.current_diff_focus().non_sidebar());
                }
            }
            AppSurface::CommitList => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, body, _footer] = Layout::vertical([
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                if body.width >= 96 {
                    let [list, _gap, meta] = Layout::horizontal([
                        Constraint::Percentage(56),
                        Constraint::Length(2),
                        Constraint::Fill(1),
                    ])
                    .areas(body);
                    if contains_point(meta, column, row) {
                        self.commit_focus = CommitPane::Detail;
                    } else if contains_point(list, column, row) {
                        self.commit_focus = CommitPane::List;
                    }
                }
            }
            AppSurface::Comments | AppSurface::DetailFull => {}
        }
    }

    fn start_pending_screen_text_selection(&mut self) {
        let Some((anchor, bounds)) = self.pending_screen_selection.take() else {
            return;
        };
        self.dragging_scrollbar = false;
        self.selecting_text = true;
        self.text_selection_dragged = false;
        self.diff_buffer.viewer_mut().clear_selection();
        self.screen_selection_bounds = bounds;
        self.screen_selection = Some(ScreenTextSelection::new(anchor));
    }

    fn update_screen_text_selection(&mut self, mouse: MouseEvent) {
        self.text_selection_dragged = true;
        if let Some(selection) = &mut self.screen_selection {
            selection.set_focus(ScreenPoint {
                x: mouse.column,
                y: mouse.row,
            });
        }
    }

    fn copy_screen_text_selection_to_clipboard(&mut self) {
        let Some(selection) = self.screen_selection else {
            return;
        };
        let text = selection::selected_screen_text(
            &self.screen_text,
            selection,
            self.screen_selection_bounds,
        );
        if text.is_empty() {
            self.screen_selection = None;
            self.screen_selection_bounds = None;
            return;
        }
        let copied = copy_to_system_clipboard(&text) || copy_to_terminal_clipboard(&text);
        if copied {
            self.screen_selection = None;
            self.screen_selection_bounds = None;
        }
    }

    fn selection_pane_bounds(
        &self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Rect {
        let full = Rect::new(0, 0, terminal_width, terminal_height);
        if self.file_picker_open {
            return full;
        }
        match self.surface {
            AppSurface::Queue => {
                if let Some(details) = self.home_detail_area(terminal_width, terminal_height)
                    && contains_point(details, mouse.column, mouse.row)
                {
                    return details;
                }
                if let Some(queue) = self.home_wide_queue_area(terminal_width, terminal_height)
                    && contains_point(queue, mouse.column, mouse.row)
                {
                    return queue;
                }
                full
            }
            AppSurface::Diff => {
                let area = app_content_area(full);
                let [_header, _divider, body, _footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                let (sidebar, _sidebar_divider, diff_body) = self.diff_sidebar_layout(body);
                if let Some(sidebar) = sidebar
                    && contains_point(sidebar, mouse.column, mouse.row)
                {
                    return sidebar;
                }
                if contains_point(diff_body, mouse.column, mouse.row) {
                    return diff_body;
                }
                full
            }
            AppSurface::DetailFull | AppSurface::Comments | AppSurface::CommitList => full,
        }
    }

    pub(super) fn is_on_main_scrollbar(
        &self,
        column: u16,
        row: u16,
        terminal_width: u16,
        terminal_height: u16,
    ) -> bool {
        terminal_width > 0
            && column >= terminal_width.saturating_sub(1)
            && self.is_in_main_body(row, terminal_height)
    }

    pub(super) fn is_in_main_body(&self, row: u16, _terminal_height: u16) -> bool {
        row >= main_body_top() && row < main_body_top().saturating_add(self.viewport_height as u16)
    }

    fn scrollbar_target_at(
        &mut self,
        column: u16,
        row: u16,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<ScrollbarTarget> {
        [
            ScrollbarTarget::DetailDescription,
            ScrollbarTarget::Comments,
        ]
        .into_iter()
        .find(|&target| {
            self.scrollbar_for_target(target, terminal_width, terminal_height)
                .is_some_and(|scrollbar| scrollbar.hit(column, row))
        })
    }

    fn start_scrollbar_drag(
        &mut self,
        target: ScrollbarTarget,
        row: u16,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        self.selecting_text = false;
        self.pending_screen_selection = None;
        self.screen_selection = None;
        let Some(mut scrollbar) =
            self.scrollbar_for_target(target, terminal_width, terminal_height)
        else {
            return;
        };
        if !scrollbar.thumb_hit(row) {
            let value = scrollbar.value_from_drag(row, 0);
            self.set_scrollbar_target_position(target, value, terminal_width, terminal_height);
            let Some(updated) = self.scrollbar_for_target(target, terminal_width, terminal_height)
            else {
                return;
            };
            scrollbar = updated;
        }
        let offset_virtual = scrollbar.drag_offset_virtual(row);
        self.active_scrollbar_drag = Some(ScrollbarDrag {
            target,
            offset_virtual,
        });
        self.drag_active_scrollbar_to(row, terminal_width, terminal_height);
    }

    fn drag_active_scrollbar_to(&mut self, row: u16, terminal_width: u16, terminal_height: u16) {
        let Some(drag) = self.active_scrollbar_drag else {
            return;
        };
        let Some(scrollbar) =
            self.scrollbar_for_target(drag.target, terminal_width, terminal_height)
        else {
            return;
        };
        let value = scrollbar.value_from_drag(row, drag.offset_virtual);
        self.set_scrollbar_target_position(drag.target, value, terminal_width, terminal_height);
    }

    fn set_scrollbar_target_position(
        &mut self,
        target: ScrollbarTarget,
        value: usize,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        match target {
            ScrollbarTarget::DetailDescription => self.surface_scroll_y = value,
            ScrollbarTarget::Comments => {
                self.set_comments_scrollbar_position(value, terminal_width, terminal_height)
            }
        }
    }

    fn scrollbar_for_target(
        &mut self,
        target: ScrollbarTarget,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<VerticalScrollbar> {
        match target {
            ScrollbarTarget::DetailDescription => {
                self.description_scrollbar(terminal_width, terminal_height)
            }
            ScrollbarTarget::Comments => self.comments_scrollbar(terminal_width, terminal_height),
        }
    }

    fn description_scrollbar(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<VerticalScrollbar> {
        let (content, total_rows, visible_rows) =
            self.description_scroll_context(terminal_width, terminal_height)?;
        Some(VerticalScrollbar::new(
            content,
            total_rows,
            visible_rows,
            self.surface_scroll_y,
        ))
    }

    fn comments_scrollbar(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<VerticalScrollbar> {
        if self.surface != AppSurface::Comments {
            return None;
        }
        let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
        let [_header, _top_rule, _title_area, body, _footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
        let comments = self.selected_comments(selected);
        let rows = comment_surface_rows(
            &comments,
            body.width.saturating_sub(3) as usize,
            &self.home_palette(),
        );
        Some(VerticalScrollbar::new(
            body,
            rows.len(),
            body.height as usize,
            self.surface_scroll_y,
        ))
    }

    fn set_comments_scrollbar_position(
        &mut self,
        value: usize,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        let Some((body, rows_len)) = self.comments_scroll_context(terminal_width, terminal_height)
        else {
            return;
        };
        self.surface_scroll_y = value.min(rows_len.saturating_sub(body.height as usize));
        if let Some(comment_index) = self.comment_index_at_scroll_position(
            self.surface_scroll_y,
            terminal_width,
            terminal_height,
        ) {
            self.comments_selection = comment_index;
        }
    }

    fn comment_index_at_scroll_position(
        &mut self,
        value: usize,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<usize> {
        let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
        let [_header, _top_rule, _title_area, body, _footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
        let comments = self.selected_comments(selected);
        let rows = comment_surface_rows(
            &comments,
            body.width.saturating_sub(3) as usize,
            &self.home_palette(),
        );
        rows.get(value)
            .map(CommentSurfaceRow::comment_index)
            .or_else(|| rows.get(value).map(CommentSurfaceRow::comment_index))
            .or_else(|| {
                rows.iter()
                    .next_back()
                    .map(CommentSurfaceRow::comment_index)
            })
    }

    fn comments_scroll_context(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<(Rect, usize)> {
        if self.surface != AppSurface::Comments {
            return None;
        }
        let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
        let [_header, _top_rule, _title_area, body, _footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
        let comments = self.selected_comments(selected);
        let rows = comment_surface_rows(
            &comments,
            body.width.saturating_sub(3) as usize,
            &self.home_palette(),
        );
        Some((body, rows.len()))
    }

    fn description_scroll_context(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<(Rect, usize, usize)> {
        if self.detail_tab != DetailTab::Description {
            return None;
        }
        let details = match self.surface {
            AppSurface::Queue => self.home_detail_area(terminal_width, terminal_height)?,
            AppSurface::DetailFull => {
                let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
                let [_header, _divider, body, _footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(area);
                body
            }
            AppSurface::CommitList | AppSurface::Comments | AppSurface::Diff => return None,
        };
        if details.width < 3 || details.height < 3 {
            return None;
        }
        let content = Rect::new(
            details.x.saturating_add(1),
            details.y.saturating_add(1),
            details.width.saturating_sub(2),
            details.height.saturating_sub(2),
        );
        let visible_rows = content.height as usize;
        if visible_rows == 0 || content.width == 0 {
            return None;
        }
        let items = self.home_work_items();
        let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
        let total_rows = if let Some((repository, number, body)) = selected
            .pull_request(self)
            .map(|pr| (pr.repository.clone(), pr.number, pr.body.clone()))
        {
            let preview_width = content.width.saturating_sub(1).max(16);
            self.cached_pull_request_body_preview(
                &repository,
                number,
                &body,
                preview_width,
                surfaces::DETAIL_DESCRIPTION_ROW_LIMIT,
                &self.home_palette(),
                true,
            )?
            .len()
        } else {
            selected.description(self).len()
        };
        Some((content, total_rows, visible_rows))
    }

    pub(super) fn jump_scrollbar_to(&mut self, row: u16, rows: usize) {
        let body_row = row.saturating_sub(main_body_top()) as usize;
        let height = self.viewport_height.max(1);
        let max_scroll = rows.saturating_sub(height);
        let scroll = if max_scroll == 0 {
            0
        } else {
            body_row.min(height.saturating_sub(1)) * max_scroll / height.saturating_sub(1).max(1)
        };
        self.diff_buffer.viewer_mut().viewport.scroll_y = scroll;
    }

    pub(super) fn drag_scrollbar_to(&mut self, row: u16, rows: usize) {
        let height = self.viewport_height.max(1);
        let slider = self.scrollbar_slider_state(rows);
        if slider.max == 0 {
            self.diff_buffer.viewer_mut().viewport.scroll_y = 0;
            return;
        }

        let geometry = slider.geometry(height);
        let max_thumb_start = geometry
            .virtual_track_size
            .saturating_sub(geometry.virtual_thumb_size);
        let body_row = row.saturating_sub(main_body_top()) as usize;
        let virtual_mouse = body_row.min(height) * 2;
        let desired_thumb_start = virtual_mouse
            .saturating_sub(self.scrollbar_drag_offset_virtual)
            .min(max_thumb_start);
        self.diff_buffer.viewer_mut().viewport.scroll_y = slider
            .value_from_virtual_thumb_start(height, desired_thumb_start)
            .min(slider.max);
    }

    pub(super) fn is_in_scrollbar_thumb(&self, body_row: usize, rows: usize) -> bool {
        let thumb_size = self.virtual_scrollbar_thumb_size(rows);
        let thumb_start = self.virtual_scrollbar_thumb_start(rows);
        let thumb_end = thumb_start + thumb_size;
        let virtual_row_start = body_row * 2;
        let virtual_row_end = virtual_row_start + 2;
        virtual_row_start < thumb_end && virtual_row_end > thumb_start
    }

    pub(super) fn scrollbar_drag_offset_virtual(&self, body_row: usize, rows: usize) -> usize {
        let thumb_start = self.virtual_scrollbar_thumb_start(rows);
        let virtual_mouse = body_row.min(self.viewport_height.max(1)) * 2;
        virtual_mouse
            .saturating_sub(thumb_start)
            .min(self.virtual_scrollbar_thumb_size(rows))
    }

    pub(super) fn virtual_scrollbar_thumb_size(&self, rows: usize) -> usize {
        self.scrollbar_slider_state(rows)
            .geometry(self.viewport_height.max(1))
            .virtual_thumb_size
    }

    pub(super) fn virtual_scrollbar_thumb_start(&self, rows: usize) -> usize {
        self.scrollbar_slider_state(rows)
            .geometry(self.viewport_height.max(1))
            .virtual_thumb_start
    }

    pub(super) fn scrollbar_slider_state(&self, rows: usize) -> SliderState {
        let height = self.viewport_height.max(1);
        let max_scroll = rows.saturating_sub(height);
        SliderState {
            value: self.diff_buffer.viewer().viewport.scroll_y.min(max_scroll),
            min: 0,
            max: max_scroll,
            viewport_size: height,
        }
    }
}

fn copy_to_system_clipboard(text: &str) -> bool {
    let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let Some(mut stdin) = child.stdin.take() else {
        return false;
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        return false;
    }
    drop(stdin);
    child.wait().is_ok_and(|status| status.success())
}

fn main_body_top() -> u16 {
    APP_TOP_PADDING.saturating_add(BODY_TOP)
}

fn copy_to_terminal_clipboard(text: &str) -> bool {
    let payload = base64_encode(text.as_bytes());
    print!("\x1b]52;c;{payload}\x07");
    std::io::stdout().flush().is_ok()
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_base64_payload_matches_standard_encoding() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode("alpha\nbeta".as_bytes()), "YWxwaGEKYmV0YQ==");
    }
}
