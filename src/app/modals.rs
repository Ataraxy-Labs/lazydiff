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
    }

    pub(super) fn handle_modal_key(&mut self, code: KeyCode) -> bool {
        if self.thread_modal.is_some() {
            match code {
                KeyCode::Esc | KeyCode::Enter => self.thread_modal = None,
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
        let area = centered_rect(frame.area(), 82, 13);
        let palette = self.finder_palette();
        frame.render_widget(Clear, area);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(palette.bg));
        draw_box(
            frame.buffer_mut(),
            area,
            Style::new().fg(palette.border).bg(palette.bg),
        );
        if area.width < 4 || area.height < 4 {
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
        let composer_title = modal.kind.composer_title();
        frame.render_widget(
            Line::from(vec![
                Span::styled(format!(" {composer_title} "), title),
                Span::styled("· ", muted),
                Span::styled(short_path(modal.target.path()).to_string(), muted),
                Span::styled(" ", muted),
                Span::styled(target_range_label(&modal.target), title),
            ]),
            Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1),
        );
        frame.render_widget(
            Line::from(Span::styled(modal.kind.composer_help(), muted)),
            Rect::new(area.x + 2, area.y + 3, area.width.saturating_sub(4), 1),
        );
        let input_area = Rect::new(area.x + 2, area.y + 5, area.width.saturating_sub(4), 4);
        let input_bg = palette.selected_bg;
        fill_rect(
            frame.buffer_mut(),
            input_area,
            " ",
            Style::new().fg(palette.fg).bg(input_bg),
        );
        draw_box(
            frame.buffer_mut(),
            input_area,
            Style::new().fg(palette.border).bg(input_bg),
        );
        let body = if modal.body.is_empty() {
            modal.kind.placeholder().to_string()
        } else {
            modal.body.clone()
        };
        let body_style = if modal.body.is_empty() {
            muted.bg(input_bg)
        } else {
            Style::new().fg(palette.fg).bg(input_bg)
        };
        frame.render_widget(
            Line::from(Span::styled(
                truncate(&body, input_area.width.saturating_sub(4) as usize),
                body_style,
            )),
            Rect::new(
                input_area.x + 2,
                input_area.y + 1,
                input_area.width.saturating_sub(4),
                1,
            ),
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled("enter", key),
                Span::styled(format!(" {}  ", modal.kind.submit_label()), muted),
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

    pub(super) fn render_thread_modal(&self, frame: &mut Frame, target: &DiffLineTarget) {
        let notes = self.session.notes_for_target(target);
        let area = centered_rect(frame.area(), 86, 16);
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
        for (index, note) in notes.iter().take(list_area.height as usize).enumerate() {
            let y = list_area.y + index as u16;
            let source = note.kind.label();
            let reply = note
                .parent_id
                .map(|_| " follow-up".to_string())
                .unwrap_or_default();
            let (symbol, color) = note.kind.gutter_marker();
            frame.render_widget(
                Line::from(vec![
                    Span::styled(
                        format!("{symbol} {} {}{}  ", note.author, source, reply),
                        Style::new()
                            .fg(color)
                            .bg(palette.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        truncate(&note.body, list_area.width.saturating_sub(24) as usize),
                        Style::new().fg(palette.fg).bg(palette.bg),
                    ),
                ]),
                Rect::new(list_area.x, y, list_area.width, 1),
            );
        }
        frame.render_widget(
            Line::from(vec![
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
