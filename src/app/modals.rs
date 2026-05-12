use super::surfaces::comment_row_span;
use super::*;

impl App {
    pub(super) fn open_review_composer(&mut self, kind: ReviewItemKind) {
        if !matches!(self.diff_source, DiffSource::LocalWorktree(_)) {
            return;
        }
        let Some(target) = self.active_review_target() else {
            return;
        };
        self.state.clear_mouse_selection();
        self.file_picker_open = false;
        self.thread_modal = None;
        self.comment_modal = Some(CommentModal::new(target, kind, None));
    }

    pub(super) fn open_thread_modal(&mut self) {
        let Some(target) = self.focus_comment_target() else {
            return;
        };
        if self.session.notes_for_target(&target).is_empty() {
            self.open_review_composer(ReviewItemKind::Note);
            return;
        }
        self.state.clear_mouse_selection();
        self.file_picker_open = false;
        self.comment_modal = None;
        self.thread_modal = Some(target);
        self.thread_selection = 0;
        self.thread_scroll_y = 0;
    }

    pub(super) fn handle_modal_key(&mut self, code: KeyCode) -> bool {
        if let Some(target) = self.thread_modal.clone() {
            match code {
                KeyCode::Esc | KeyCode::Enter => self.thread_modal = None,
                KeyCode::Char('j') | KeyCode::Down => {
                    let max = self
                        .session
                        .notes_for_target(&target)
                        .len()
                        .saturating_sub(1);
                    self.thread_selection = self.thread_selection.saturating_add(1).min(max);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.thread_selection = self.thread_selection.saturating_sub(1);
                }
                KeyCode::Char('q')
                | KeyCode::Char('i')
                | KeyCode::Char('n')
                | KeyCode::Char('c') => {
                    if let Some(target) = self.thread_modal.take() {
                        let kind = match code {
                            KeyCode::Char('q') => ReviewItemKind::Question,
                            KeyCode::Char('i') => ReviewItemKind::Instruction,
                            _ => ReviewItemKind::Note,
                        };
                        let parent_id = self
                            .session
                            .notes_for_target(&target)
                            .last()
                            .map(|note| note.id);
                        self.comment_modal = Some(CommentModal::new(
                            DiffLineRangeTarget::single(target),
                            kind,
                            parent_id,
                        ));
                    }
                }
                _ => {}
            }
            return true;
        }

        let Some(mut modal) = self.comment_modal.take() else {
            return false;
        };
        match code {
            KeyCode::Esc => {}
            KeyCode::Backspace => {
                modal.body.pop();
                self.comment_modal = Some(modal);
            }
            KeyCode::Enter => {
                if !modal.body.trim().is_empty() {
                    self.session.add_note(
                        &self.store,
                        modal.target,
                        modal.kind,
                        modal.parent_id,
                        modal.body,
                    );
                }
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                modal.body.push(ch);
                self.comment_modal = Some(modal);
            }
            _ => self.comment_modal = Some(modal),
        }
        true
    }

    pub(super) fn render_comment_modal(&self, frame: &mut Frame, modal: &CommentModal) {
        let area = centered_rect(frame.area(), 78, 9);
        let palette = self.finder_palette();
        frame.render_widget(Clear, area);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(palette.bg));
        if area.width < 4 || area.height < 4 {
            return;
        }
        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let accent = Style::new()
            .fg(palette.accent)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let key = Style::new()
            .fg(palette.key)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let composer_title = modal.kind.composer_title();
        let (symbol, _) = modal.kind.gutter_marker();
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(symbol, accent),
                Span::raw(" "),
                Span::styled(composer_title, title),
                Span::styled("  ", muted),
                Span::styled(short_path(modal.target.path()).to_string(), muted),
                Span::styled(" ", muted),
                Span::styled(target_range_label(&modal.target), title),
            ]),
            Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1),
        );
        frame.render_widget(
            Line::from(Span::styled(
                truncate(
                    modal.kind.composer_help(),
                    area.width.saturating_sub(6) as usize,
                ),
                muted,
            )),
            Rect::new(area.x + 3, area.y + 2, area.width.saturating_sub(6), 1),
        );
        draw_horizontal_rule(
            frame.buffer_mut(),
            area.y + 3,
            area.x + 3,
            area.right().saturating_sub(3),
            palette.border,
            palette.bg,
        );
        let input_area = Rect::new(area.x + 3, area.y + 4, area.width.saturating_sub(6), 1);
        let body = if modal.body.is_empty() {
            modal.kind.placeholder().to_string()
        } else {
            modal.body.clone()
        };
        let body_style = if modal.body.is_empty() {
            muted
        } else {
            Style::new().fg(palette.fg).bg(palette.bg)
        };
        frame.render_widget(
            Line::from(vec![
                Span::styled("› ", Style::new().fg(palette.accent).bg(palette.bg)),
                Span::styled(
                    truncate(&body, input_area.width.saturating_sub(2) as usize),
                    body_style,
                ),
            ]),
            input_area,
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled("enter", key),
                Span::styled(format!(" {}   ", modal.kind.submit_label()), muted),
                Span::styled("esc", key),
                Span::styled(" cancel", muted),
            ]),
            Rect::new(
                area.x + 2,
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(4),
                1,
            ),
        );
    }

    pub(super) fn render_thread_modal(&mut self, frame: &mut Frame, target: &DiffLineTarget) {
        let notes = self.session.notes_for_target(target);
        let area = centered_rect(frame.area(), 86, 16);
        let palette = self.home_palette();
        frame.render_widget(Clear, area);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(palette.bg));
        draw_box(
            frame.buffer_mut(),
            area,
            Style::new().fg(palette.rule).bg(palette.bg),
        );
        if area.width < 4 || area.height < 5 {
            return;
        }
        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let key = Style::new()
            .fg(palette.action)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Discussion ", title),
                Span::styled("· ", muted),
                Span::styled(short_path(&target.path).to_string(), muted),
                Span::styled(" ", muted),
                Span::styled(target_line_label(target), title),
            ]),
            Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1),
        );
        let list_area = Rect::new(
            area.x + 2,
            area.y + 3,
            area.width.saturating_sub(4),
            area.height.saturating_sub(6),
        );
        let comments = notes
            .iter()
            .map(|note| CommentView::from_thread_note(note))
            .collect::<Vec<_>>();
        let rows = comment_surface_rows(
            &comments,
            list_area.width.saturating_sub(3) as usize,
            &palette,
        );
        let selection = self.thread_selection.min(notes.len().saturating_sub(1));
        self.thread_selection = selection;
        let (first_idx, last_idx) = comment_row_span(&rows, selection);
        let height = list_area.height as usize;
        if first_idx < self.thread_scroll_y {
            self.thread_scroll_y = first_idx;
        } else if last_idx >= self.thread_scroll_y.saturating_add(height) {
            self.thread_scroll_y = last_idx.saturating_sub(height.saturating_sub(1));
        }
        self.thread_scroll_y = self
            .thread_scroll_y
            .min(rows.len().saturating_sub(height.max(1)));

        let selected_bg = palette.layer_bg(SurfaceLayer::ElevatedSurface);
        let rail_style = Style::new().fg(palette.action).bg(selected_bg);
        for (visual_index, row) in rows
            .iter()
            .skip(self.thread_scroll_y)
            .take(height)
            .enumerate()
        {
            let y = list_area.y + visual_index as u16;
            let is_selected = row.comment_index() == selection;
            let row_rect = Rect::new(list_area.x, y, list_area.width, 1);
            if is_selected {
                fill_rect(
                    frame.buffer_mut(),
                    row_rect,
                    " ",
                    Style::new().bg(selected_bg),
                );
            }
            match row {
                CommentSurfaceRow::Header { author, age, .. } => {
                    let bg = if is_selected { selected_bg } else { palette.bg };
                    let prefix = if is_selected { "┃● " } else { " ● " };
                    frame.render_widget(
                        Line::from(vec![
                            Span::styled(prefix, Style::new().fg(palette.accent).bg(bg)),
                            Span::styled(author.clone(), title.bg(bg)),
                            Span::styled(format!(" · {age}"), muted.bg(bg)),
                        ]),
                        row_rect,
                    );
                }
                CommentSurfaceRow::Body { line, .. } => {
                    if is_selected {
                        let mut line = line.clone();
                        if let Some(first) = line.spans.first_mut() {
                            let trimmed_len = first
                                .content
                                .chars()
                                .skip_while(|c| c.is_whitespace())
                                .count();
                            if trimmed_len == 0 {
                                // Replace one gutter column with the rail so
                                // selecting a row doesn't shift markdown text.
                                let leading = first.content.chars().count();
                                let after_rail = leading.saturating_sub(1);
                                first.content = " ".repeat(after_rail).into();
                            }
                            first.style = first.style.bg(selected_bg);
                        }
                        line.spans.insert(0, Span::styled("┃", rail_style));
                        for span in line.spans.iter_mut().skip(1) {
                            span.style = span.style.bg(selected_bg);
                        }
                        frame.render_widget(line, row_rect);
                    } else {
                        frame.render_widget(line.clone(), row_rect);
                    }
                }
                CommentSurfaceRow::Blank { .. } => {
                    if is_selected {
                        frame.render_widget(
                            Line::from(vec![Span::styled("┃", rail_style)]),
                            row_rect,
                        );
                    }
                }
            }
        }
        frame.render_widget(
            Line::from(vec![
                Span::styled("j/k", key),
                Span::styled(" reply  ", muted),
                Span::styled("q", key),
                Span::styled(" ask follow-up  ", muted),
                Span::styled("i", key),
                Span::styled(" request change  ", muted),
                Span::styled("n", key),
                Span::styled(" note  ", muted),
                Span::styled("enter/esc", key),
                Span::styled(" close", muted),
            ]),
            Rect::new(
                area.x + 2,
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(4),
                1,
            ),
        );
    }

    pub(super) fn render_attempts_modal(&self, frame: &mut Frame) {
        let area = centered_rect(frame.area(), 72, 14);
        let palette = self.finder_palette();
        frame.render_widget(Clear, area);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(palette.bg));
        draw_box(
            frame.buffer_mut(),
            area,
            Style::new().fg(palette.border).bg(palette.bg),
        );
        if area.width < 4 || area.height < 5 {
            return;
        }
        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let key = Style::new()
            .fg(palette.key)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Line::from(Span::styled(" Current attempt ", title)),
            Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1),
        );
        let attempt = &self.session.current_attempt;
        frame.render_widget(
            Line::from(vec![
                Span::styled("● ", key),
                Span::styled(format!("Attempt {}   ", attempt.ordinal), title),
                Span::styled(attempt.summary.clone(), muted),
            ]),
            Rect::new(area.x + 2, area.y + 3, area.width.saturating_sub(4), 1),
        );
        frame.render_widget(
            Line::from(Span::styled(
                format!(
                    "{} items raised, {} open, {} resolved",
                    self.session.notes.len(),
                    self.session.open_count(),
                    self.session.resolved_count()
                ),
                muted,
            )),
            Rect::new(area.x + 4, area.y + 4, area.width.saturating_sub(6), 1),
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled("R", key),
                Span::styled(" compare with previous  ", muted),
                Span::styled("esc", key),
                Span::styled(" close", muted),
            ]),
            Rect::new(
                area.x + 2,
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(4),
                1,
            ),
        );
    }
}
