use super::*;

enum NoteEditorOutcome {
    Keep(CommentModal),
    ExitAbove(CommentModal),
    ExitBelow(CommentModal),
    Submit(CommentModal),
}

impl App {
    pub(super) fn open_review_composer(&mut self, kind: ReviewItemKind) {
        if !matches!(
            self.diff_source,
            DiffSource::LocalWorktree(_) | DiffSource::PullRequest { .. }
        ) {
            self.branch_operation_status =
                Some("notes are only available for worktree or PR diffs".to_string());
            return;
        }
        if let Some(note) = self.focused_inline_note() {
            self.open_existing_note_editor(note, KeyCode::Char('i'));
            return;
        }
        let Some(target) = self.active_review_target() else {
            self.branch_operation_status =
                Some("move to an added/deleted line before adding a note".to_string());
            return;
        };
        self.diff_buffer.viewer_mut().clear_selection();
        self.inline_focus = None;
        self.file_picker_open = false;
        self.thread_modal = None;
        let kind = if matches!(self.diff_source, DiffSource::PullRequest { .. }) {
            ReviewItemKind::Note
        } else {
            kind
        };
        self.diff_buffer.viewer_mut().yank_selection = None;
        self.diff_buffer.viewer_mut().yank_until = None;
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
        self.file_picker_open = false;
        self.comment_modal = None;
        self.thread_modal = None;
        self.branch_operation_status = Some("thread is shown inline".to_string());
    }

    pub(super) fn handle_modal_key(&mut self, code: KeyCode) -> bool {
        let Some(mut modal) = self.comment_modal.take() else {
            return false;
        };
        let pending_delete = modal.pending_delete;
        modal.pending_delete = false;
        match Self::handle_note_editor_key(modal, code, pending_delete) {
            NoteEditorOutcome::Keep(modal) => {
                self.comment_modal = Some(modal);
                self.ensure_inline_comment_editor_cursor_visible();
            }
            NoteEditorOutcome::ExitAbove(modal) => self.exit_note_editor(modal, -1),
            NoteEditorOutcome::ExitBelow(modal) => self.exit_note_editor(modal, 1),
            NoteEditorOutcome::Submit(modal) => self.submit_comment_modal(modal),
        }
        true
    }

    fn handle_note_editor_key(
        mut modal: CommentModal,
        code: KeyCode,
        pending_delete: bool,
    ) -> NoteEditorOutcome {
        match modal.mode {
            CommentEditorMode::Insert => match code {
                KeyCode::Esc => {
                    modal.mode = CommentEditorMode::Normal;
                    modal.clear_selection();
                }
                KeyCode::Enter => modal.insert_line_below_split(),
                KeyCode::Backspace => modal.backspace(),
                KeyCode::Delete => modal.delete_forward(),
                KeyCode::Left => modal.move_col(-1),
                KeyCode::Right => modal.move_col(1),
                KeyCode::Up => modal.move_row(-1),
                KeyCode::Down => modal.move_row(1),
                KeyCode::Char(ch) if !ch.is_control() => modal.insert_char(ch),
                _ => {}
            },
            CommentEditorMode::Visual | CommentEditorMode::VisualLine => match code {
                KeyCode::Esc => {
                    modal.mode = CommentEditorMode::Normal;
                    modal.clear_selection();
                }
                KeyCode::Char('h') | KeyCode::Left => modal.move_col(-1),
                KeyCode::Char('l') | KeyCode::Right => modal.move_col(1),
                KeyCode::Char('j') | KeyCode::Down => modal.move_row(1),
                KeyCode::Char('k') | KeyCode::Up => modal.move_row(-1),
                KeyCode::Char('0') | KeyCode::Home => modal.col = 0,
                KeyCode::Char('$') | KeyCode::End => modal.col = modal.line_len(),
                KeyCode::Char('w') => modal.move_word_forward(),
                KeyCode::Char('b') => modal.move_word_backward(),
                KeyCode::Char('v') if modal.mode == CommentEditorMode::Visual => {
                    modal.mode = CommentEditorMode::Normal;
                    modal.clear_selection();
                }
                KeyCode::Char('V') if modal.mode == CommentEditorMode::VisualLine => {
                    modal.mode = CommentEditorMode::Normal;
                    modal.clear_selection();
                }
                KeyCode::Char('y') => {
                    modal.mode = CommentEditorMode::Normal;
                    modal.clear_selection();
                }
                KeyCode::Char('x') | KeyCode::Delete => {
                    modal.delete_selection();
                }
                _ => {}
            },
            CommentEditorMode::Normal => match code {
                KeyCode::Esc => return NoteEditorOutcome::ExitAbove(modal),
                KeyCode::Enter => return NoteEditorOutcome::Submit(modal),
                KeyCode::Char('i') => {
                    modal.clear_selection();
                    modal.mode = CommentEditorMode::Insert;
                }
                KeyCode::Char('a') => {
                    modal.clear_selection();
                    modal.move_col(1);
                    modal.mode = CommentEditorMode::Insert;
                }
                KeyCode::Char('A') => {
                    modal.clear_selection();
                    modal.col = modal.line_len();
                    modal.mode = CommentEditorMode::Insert;
                }
                KeyCode::Char('o') => {
                    modal.clear_selection();
                    modal.open_line_below();
                    modal.mode = CommentEditorMode::Insert;
                }
                KeyCode::Char('O') => {
                    modal.clear_selection();
                    modal.open_line_above();
                    modal.mode = CommentEditorMode::Insert;
                }
                KeyCode::Char('v') => modal.start_visual(false),
                KeyCode::Char('V') => modal.start_visual(true),
                KeyCode::Char('h') | KeyCode::Left => modal.move_col(-1),
                KeyCode::Char('l') | KeyCode::Right => modal.move_col(1),
                KeyCode::Char('0') | KeyCode::Home => modal.col = 0,
                KeyCode::Char('$') | KeyCode::End => modal.col = modal.line_len(),
                KeyCode::Char('w') => modal.move_word_forward(),
                KeyCode::Char('b') => modal.move_word_backward(),
                KeyCode::Char('x') | KeyCode::Delete => modal.delete_forward(),
                KeyCode::Backspace => modal.backspace(),
                KeyCode::Char('d') if pending_delete => modal.delete_line(),
                KeyCode::Char('d') => modal.pending_delete = true,
                KeyCode::Char('j') | KeyCode::Down => {
                    if modal.row + 1 >= modal.lines.len() {
                        return NoteEditorOutcome::ExitBelow(modal);
                    }
                    modal.move_row(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if modal.row == 0 {
                        return NoteEditorOutcome::ExitAbove(modal);
                    }
                    modal.move_row(-1);
                }
                _ => {}
            },
        }
        NoteEditorOutcome::Keep(modal)
    }

    fn exit_note_editor(&mut self, modal: CommentModal, row_delta: isize) {
        let target = modal.target.end.clone();
        let mode = self.diff_buffer.viewer().viewport.mode;
        let target_row = self.document.line_row(
            mode,
            target.file_index,
            target.hunk_index,
            target.line_index,
        );
        let rows = row_count_for_mode(&self.document, mode);
        let next_row = target_row.map(|row| {
            if row_delta > 0 {
                row.saturating_add(1).min(rows.saturating_sub(1))
            } else {
                row.min(rows.saturating_sub(1))
            }
        });
        self.comment_modal = Some(modal.clone());
        let pre_exit_screen_row =
            next_row.and_then(|row| self.diff_document_row_screen_offset(row));
        self.comment_modal = None;
        self.submit_comment_modal(modal);
        if let Some(next_row) = next_row {
            self.inline_focus = None;
            self.focus_document_row_preserving_view(next_row);
            if let Some(screen_row) = pre_exit_screen_row {
                self.keep_diff_document_row_at_screen_offset(next_row, screen_row);
            } else {
                self.ensure_focused_diff_visual_row_visible();
            }
        }
    }

    fn submit_comment_modal(&mut self, mut modal: CommentModal) {
        modal.sync_body();
        if let Some(note_id) = modal.edit_note_id {
            if modal.body.trim().is_empty() {
                self.session.notes.retain(|note| note.id != note_id);
                self.store.delete_note(&self.session.id, note_id);
            } else {
                self.session
                    .update_note_body(&self.store, note_id, modal.body);
            }
            return;
        }
        if modal.body.trim().is_empty() {
            return;
        }
        if let DiffSource::PullRequest { repository, number } = &self.diff_source {
            let repository = repository.clone();
            let number = *number;
            let target = modal.target;
            let body = modal.body;
            self.branch_operation_status = Some("posting PR comment…".to_string());
            let sender = self.query_tx.clone();
            let forge = Arc::clone(&self.forge);
            thread::spawn(move || {
                let result = forge.post_comment(&repository, number, &target, &body);
                let _ = sender.send(QueryEvent::PostedComment {
                    repository,
                    number,
                    result,
                });
            });
        } else {
            self.session.add_note(
                &self.store,
                modal.target,
                modal.kind,
                modal.parent_id,
                modal.body,
            );
        }
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
