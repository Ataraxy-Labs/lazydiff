use super::*;

impl App {
    pub(super) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        terminal_height: u16,
    ) {
        let rows = row_count_for_mode(&self.document, self.state.mode);
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
        if self.file_picker_open
            && self.handle_file_picker_mouse(mouse, terminal_width, terminal_height)
        {
            return;
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.handle_semantic_mouse_down(mouse, terminal_width, terminal_height)
        {
            return;
        }
        match (self.surface, mouse.kind) {
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
                    if self.detail_tab == DetailTab::Semantic {
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
                    if self.detail_tab == DetailTab::Semantic {
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
                if self.detail_tab == DetailTab::Semantic {
                    self.scroll_semantic_tree(1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_add(1);
                }
                return;
            }
            (AppSurface::DetailFull, MouseEventKind::ScrollUp) => {
                if self.detail_tab == DetailTab::Semantic {
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
            MouseEventKind::ScrollDown => {
                self.scroll_relative(1, rows);
            }
            MouseEventKind::ScrollUp => {
                self.scroll_relative(-1, rows);
            }
            MouseEventKind::Down(MouseButton::Left)
                if self.is_on_main_scrollbar(
                    mouse.column,
                    mouse.row,
                    terminal_width,
                    terminal_height,
                ) =>
            {
                self.selecting_text = false;
                self.dragging_scrollbar = true;
                let body_row = mouse.row.saturating_sub(BODY_TOP) as usize;
                if self.is_in_scrollbar_thumb(body_row, rows) {
                    self.scrollbar_drag_offset_virtual =
                        self.scrollbar_drag_offset_virtual(body_row, rows);
                } else {
                    self.jump_scrollbar_to(mouse.row, rows);
                    self.scrollbar_drag_offset_virtual =
                        self.scrollbar_drag_offset_virtual(body_row, rows);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.dragging_scrollbar = false;
                self.selecting_text = true;
                self.text_selection_dragged = false;
                self.start_text_selection(mouse, terminal_width, rows);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_scrollbar => {
                if self.is_in_main_body(mouse.row, terminal_height) {
                    self.drag_scrollbar_to(mouse.row, rows);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.selecting_text => {
                self.text_selection_dragged = true;
                self.update_text_selection(mouse, terminal_width, rows);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.selecting_text && !self.text_selection_dragged {
                    self.state.clear_mouse_selection();
                }
                self.dragging_scrollbar = false;
                self.selecting_text = false;
                self.text_selection_dragged = false;
            }
            _ => {}
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
                let row = mouse.row.saturating_sub(list_area.y) as usize;
                self.file_picker_selection = start
                    .saturating_add(row)
                    .min(filtered_len.saturating_sub(1));
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
                    if mouse.column >= content.x.saturating_add(12) {
                        self.set_detail_tab(DetailTab::Description);
                    } else {
                        self.set_detail_tab(DetailTab::Semantic);
                    }
                    return true;
                }
                if self.detail_tab != DetailTab::Semantic {
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
                    if mouse.column >= content.x.saturating_add(12) {
                        self.set_detail_tab(DetailTab::Description);
                    } else {
                        self.set_detail_tab(DetailTab::Semantic);
                    }
                    return true;
                }
                if self.detail_tab != DetailTab::Semantic {
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

    fn semantic_mouse_target_area(
        &self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<(DiffSource, Rect)> {
        match self.surface {
            AppSurface::Queue => {
                let details = self.home_detail_area(terminal_width, terminal_height)?;
                let items = self.home_work_items();
                let selected = items.get(self.home_selection.min(items.len().saturating_sub(1)))?;
                if self.detail_tab != DetailTab::Semantic {
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
                if self.detail_tab != DetailTab::Semantic {
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

    pub(super) fn start_text_selection(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        rows: usize,
    ) {
        let Some((row, side, column, body_row)) =
            self.mouse_selection_point(mouse, terminal_width, rows)
        else {
            return;
        };
        self.last_selection_mouse = Some((body_row, column));
        self.state
            .start_mouse_selection(row, side, column, rows, self.viewport_height);
    }

    pub(super) fn update_text_selection(
        &mut self,
        mouse: MouseEvent,
        terminal_width: u16,
        rows: usize,
    ) {
        let Some((mut row, _side, column, body_row)) =
            self.mouse_selection_point(mouse, terminal_width, rows)
        else {
            return;
        };

        if body_row == 0 && self.state.scroll_y > 0 {
            self.state.scroll_y = self.state.scroll_y.saturating_sub(1);
            row = self.state.scroll_y;
        } else if body_row + 1 >= self.viewport_height
            && self.state.scroll_y + self.viewport_height < rows
        {
            self.state.scroll_y += 1;
            row = (self.state.scroll_y + self.viewport_height.saturating_sub(1))
                .min(rows.saturating_sub(1));
        }

        self.last_selection_mouse = Some((body_row, column));
        self.state
            .update_mouse_selection(row, column, rows, self.viewport_height);
    }

    pub(super) fn mouse_selection_point(
        &self,
        mouse: MouseEvent,
        terminal_width: u16,
        rows: usize,
    ) -> Option<(usize, DiffSide, usize, usize)> {
        if rows == 0
            || terminal_width <= 1
            || mouse.row < BODY_TOP
            || mouse.column >= terminal_width.saturating_sub(1)
        {
            return None;
        }
        let body_row = mouse.row.saturating_sub(BODY_TOP) as usize;
        if body_row >= self.viewport_height {
            return None;
        }
        let content_width = terminal_width.saturating_sub(1).max(1);
        let half = content_width / 2;
        let side = if mouse.column < half {
            DiffSide::Left
        } else {
            DiffSide::Right
        };
        let side_x = match side {
            DiffSide::Left => mouse.column,
            DiffSide::Right => mouse.column.saturating_sub(half),
        };
        let column = side_x.saturating_sub(SPLIT_TEXT_COLUMN) as usize;
        let point = TextSelection::document_point_from_local(
            column,
            body_row,
            TextViewport {
                scroll_x: 0,
                scroll_y: self.state.scroll_y,
            },
        );
        Some((
            point.row.min(rows.saturating_sub(1)),
            side,
            point.column,
            body_row,
        ))
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
        row >= BODY_TOP && row < BODY_TOP.saturating_add(self.viewport_height as u16)
    }

    pub(super) fn jump_scrollbar_to(&mut self, row: u16, rows: usize) {
        let body_row = row.saturating_sub(BODY_TOP) as usize;
        let height = self.viewport_height.max(1);
        let max_scroll = rows.saturating_sub(height);
        let scroll = if max_scroll == 0 {
            0
        } else {
            body_row.min(height.saturating_sub(1)) * max_scroll / height.saturating_sub(1).max(1)
        };
        self.state.scroll_y = scroll;
    }

    pub(super) fn drag_scrollbar_to(&mut self, row: u16, rows: usize) {
        let height = self.viewport_height.max(1);
        let slider = self.scrollbar_slider_state(rows);
        if slider.max == 0 {
            self.state.scroll_y = 0;
            return;
        }

        let geometry = slider.geometry(height);
        let max_thumb_start = geometry
            .virtual_track_size
            .saturating_sub(geometry.virtual_thumb_size);
        let body_row = row.saturating_sub(BODY_TOP) as usize;
        let virtual_mouse = body_row.min(height) * 2;
        let desired_thumb_start = virtual_mouse
            .saturating_sub(self.scrollbar_drag_offset_virtual)
            .min(max_thumb_start);
        self.state.scroll_y = slider
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
            value: self.state.scroll_y.min(max_scroll),
            min: 0,
            max: max_scroll,
            viewport_size: height,
        }
    }
}
