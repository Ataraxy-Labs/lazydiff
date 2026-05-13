use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinderKind {
    Root,
    Files,
    Text,
    Inbox,
    Themes,
}

pub(crate) struct FinderResult {
    pub(crate) index: usize,
    pub(crate) score: u32,
    pub(crate) matched: Vec<usize>,
}

pub(crate) struct TextSearchResult {
    pub(crate) file_index: usize,
    pub(crate) hunk_index: usize,
    pub(crate) line_index: usize,
    pub(crate) old_line: Option<u32>,
    pub(crate) new_line: Option<u32>,
    pub(crate) kind: &'static str,
    pub(crate) text: String,
    pub(crate) score: u32,
}

#[derive(Clone, Copy)]
pub(crate) enum TextSearchRow {
    FileHeader { file_index: usize },
    Match { result_index: usize },
}

pub(crate) fn text_search_rows(results: &[TextSearchResult]) -> Vec<TextSearchRow> {
    let mut rows = Vec::with_capacity(results.len() + results.len().min(32));
    let mut previous_file = None;
    for (result_index, result) in results.iter().enumerate() {
        if previous_file != Some(result.file_index) {
            rows.push(TextSearchRow::FileHeader {
                file_index: result.file_index,
            });
            previous_file = Some(result.file_index);
        }
        rows.push(TextSearchRow::Match { result_index });
    }
    rows
}

pub(crate) fn text_search_selected_row(
    rows: &[TextSearchRow],
    selected_result_index: usize,
) -> Option<usize> {
    rows.iter().position(|row| matches!(row, TextSearchRow::Match { result_index } if *result_index == selected_result_index))
}

pub(crate) fn text_search_list_start(
    list_height: usize,
    row_count: usize,
    selected_row: usize,
) -> usize {
    if list_height == 0 || row_count <= list_height {
        return 0;
    }
    selected_row
        .saturating_sub(list_height / 2)
        .min(row_count.saturating_sub(list_height))
}

#[derive(Clone)]
pub(crate) struct CommandResult {
    pub(crate) category: &'static str,
    pub(crate) label: &'static str,
    pub(crate) shortcut: &'static str,
    pub(crate) command: Command,
    pub(crate) score: u32,
}

fn command_result(
    category: &'static str,
    label: &'static str,
    shortcut: &'static str,
    command: Command,
    order: usize,
) -> CommandResult {
    CommandResult {
        category,
        label,
        shortcut,
        command,
        score: u32::MAX.saturating_sub(order as u32),
    }
}

impl App {
    pub(super) fn handle_file_picker_key(&mut self, code: KeyCode, rows: usize) {
        let filtered_len = self.filtered_results_len();
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.file_picker_open = false,
            KeyCode::Char('j') | KeyCode::Down => {
                self.file_picker_selection = self
                    .file_picker_selection
                    .saturating_add(1)
                    .min(filtered_len.saturating_sub(1));
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.file_picker_selection = self.file_picker_selection.saturating_sub(1);
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Backspace => {
                self.file_picker_query.pop();
                let filtered_len = self.filtered_results_len();
                self.file_picker_selection = self
                    .file_picker_selection
                    .min(filtered_len.saturating_sub(1));
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Enter => {
                match self.finder_kind {
                    FinderKind::Root => {
                        if let Some(command) = self
                            .filtered_command_results()
                            .get(self.file_picker_selection)
                        {
                            let command = command.command;
                            self.file_picker_open = false;
                            self.execute_command(command, rows);
                            return;
                        }
                    }
                    FinderKind::Files => {
                        if let Some(file_index) = self
                            .filtered_file_indices()
                            .get(self.file_picker_selection)
                            .copied()
                        {
                            self.jump_to_file(file_index, rows);
                        }
                    }
                    FinderKind::Text => {
                        if let Some(result) =
                            self.filtered_text_results().get(self.file_picker_selection)
                        {
                            self.jump_to_text_result(result, rows);
                        }
                    }
                    FinderKind::Inbox => {
                        if let Some(note) = self
                            .filtered_inbox_notes()
                            .get(self.file_picker_selection)
                            .cloned()
                            .cloned()
                        {
                            self.jump_to_review_item(&note, rows);
                        }
                    }
                    FinderKind::Themes => {
                        if let Some(command) = self
                            .filtered_theme_results()
                            .get(self.file_picker_selection)
                        {
                            let command = command.command;
                            self.file_picker_open = false;
                            self.execute_command(command, rows);
                            return;
                        }
                    }
                }
                self.file_picker_open = false;
            }
            KeyCode::Char('/') => {
                if self.finder_kind == FinderKind::Root
                    && self.context_has_command(Command::OpenTextSearch)
                {
                    self.open_command_palette_mode(FinderKind::Text);
                } else {
                    self.file_picker_query.clear();
                    self.file_picker_selection = 0;
                    self.file_picker_preview_scroll = 0;
                }
            }
            KeyCode::Char('f')
                if self.finder_kind == FinderKind::Root
                    && self.context_has_command(Command::OpenFileSearch) =>
            {
                self.open_command_palette_mode(FinderKind::Files);
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                self.file_picker_query.push(ch);
                self.file_picker_selection = 0;
                self.file_picker_preview_scroll = 0;
            }
            _ => {}
        }
    }

    pub(super) fn open_command_palette_mode(&mut self, kind: FinderKind) {
        self.file_picker_open = true;
        self.finder_kind = kind;
        self.file_picker_query.clear();
        self.file_picker_selection = match kind {
            FinderKind::Files => self.current_file_index().unwrap_or(0),
            FinderKind::Root | FinderKind::Text | FinderKind::Inbox | FinderKind::Themes => 0,
        };
        self.file_picker_preview_scroll = 0;
    }

    pub(super) fn open_root_palette(&mut self) {
        self.open_command_palette_mode(FinderKind::Root);
    }

    pub(super) fn open_file_search(&mut self) {
        if self.surface != AppSurface::Diff {
            self.open_selected_diff();
        }
        self.open_command_palette_mode(FinderKind::Files);
    }

    pub(super) fn open_text_search(&mut self) {
        if self.surface != AppSurface::Diff {
            self.open_selected_diff();
        }
        self.open_command_palette_mode(FinderKind::Text);
    }

    pub(super) fn open_inbox(&mut self, _rows: usize) {
        if self.surface != AppSurface::Diff {
            self.open_local_diff(None);
        }
        self.open_command_palette_mode(FinderKind::Inbox);
    }

    pub(super) fn open_theme_picker(&mut self) {
        self.open_command_palette_mode(FinderKind::Themes);
        self.file_picker_selection = crate::design_system::ThemeVariant::all()
            .iter()
            .position(|theme| *theme == self.theme_variant)
            .unwrap_or(0);
    }

    pub(super) fn filtered_results_len(&self) -> usize {
        match self.finder_kind {
            FinderKind::Root => self.filtered_command_results().len(),
            FinderKind::Files => self.filtered_file_indices().len(),
            FinderKind::Text => self.filtered_text_results().len(),
            FinderKind::Inbox => self.filtered_inbox_notes().len(),
            FinderKind::Themes => self.filtered_theme_results().len(),
        }
    }

    pub(super) fn filtered_theme_results(&self) -> Vec<CommandResult> {
        let query = self.file_picker_query.trim();
        let mut themes = crate::design_system::ThemeVariant::all()
            .iter()
            .enumerate()
            .map(|(index, theme)| {
                let shortcut = if *theme == self.theme_variant {
                    "current"
                } else {
                    "enter"
                };
                command_result(
                    "theme",
                    theme.label(),
                    shortcut,
                    Command::SelectTheme(*theme),
                    index,
                )
            })
            .collect::<Vec<_>>();
        if query.is_empty() {
            return themes;
        }
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut chars = Vec::new();
        themes
            .drain(..)
            .filter_map(|mut theme| {
                let haystack = format!("{} {} {}", theme.category, theme.label, theme.shortcut);
                let score = nucleo_score_with(&pattern, &mut matcher, &mut chars, &haystack)?;
                theme.score = score;
                Some(theme)
            })
            .collect()
    }

    pub(super) fn filtered_file_indices(&self) -> Vec<usize> {
        self.filtered_file_results()
            .into_iter()
            .map(|result| result.index)
            .collect()
    }

    pub(super) fn filtered_command_results(&self) -> Vec<CommandResult> {
        let query = self.file_picker_query.trim();
        let commands = self.contextual_command_results();
        if query.is_empty() {
            return commands;
        }
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut chars = Vec::new();
        commands
            .into_iter()
            .filter_map(|mut command| {
                let haystack = format!(
                    "{} {} {}",
                    command.category, command.label, command.shortcut
                );
                let score = nucleo_score_with(&pattern, &mut matcher, &mut chars, &haystack)?;
                command.score = score;
                Some(command)
            })
            .collect()
    }

    pub(super) fn contextual_command_results(&self) -> Vec<CommandResult> {
        let mut commands = match self.surface {
            AppSurface::CommitList => vec![
                command_result(
                    "navigation",
                    "open commit diff",
                    "tab",
                    Command::OpenSelectedCommit,
                    0,
                ),
                command_result("navigation", "back to queue", "esc", Command::Back, 1),
                command_result("navigation", "next commit", "j", Command::MoveDown, 2),
                command_result("navigation", "previous commit", "k", Command::MoveUp, 3),
            ],
            AppSurface::Queue => vec![
                command_result("navigation", "open detail", "enter", Command::OpenDetail, 0),
                command_result("navigation", "open diff", "d", Command::OpenDiff, 1),
                command_result(
                    "navigation",
                    "open commit list",
                    "tab",
                    Command::OpenCommitList,
                    2,
                ),
                command_result(
                    "navigation",
                    "open in GitHub",
                    "o",
                    Command::OpenInBrowser,
                    3,
                ),
                command_result("navigation", "open comments", "c", Command::OpenComments, 4),
                command_result("review", "review items", ":", Command::OpenInbox, 5),
                command_result("branch", "pull branch", "p", Command::PullBranch, 6),
                command_result("branch", "push branch", "P", Command::PushBranch, 7),
                command_result("branch", "fetch", "f", Command::FetchBranch, 8),
                command_result(
                    "branch",
                    "force push with lease",
                    "F",
                    Command::ForcePushBranch,
                    9,
                ),
                command_result("data", "refresh", "r", Command::Refresh, 10),
            ],
            AppSurface::DetailFull => vec![
                command_result("navigation", "back to queue", "esc", Command::Back, 0),
                command_result("navigation", "open diff", "d", Command::OpenDiff, 1),
                command_result(
                    "navigation",
                    "open in GitHub",
                    "o",
                    Command::OpenInBrowser,
                    2,
                ),
                command_result("navigation", "open comments", "c", Command::OpenComments, 3),
                command_result("scroll", "page down", "pagedown", Command::PageDown, 4),
                command_result("scroll", "page up", "pageup", Command::PageUp, 5),
            ],
            AppSurface::Comments => vec![
                command_result("navigation", "back to queue", "esc", Command::Back, 0),
                command_result("navigation", "open diff", "d", Command::OpenDiff, 1),
                command_result(
                    "navigation",
                    "open in GitHub",
                    "o",
                    Command::OpenInBrowser,
                    2,
                ),
                command_result("reader", "next comment", "j", Command::MoveDown, 3),
                command_result("reader", "previous comment", "k", Command::MoveUp, 4),
                command_result("reader", "page down", "pagedown", Command::PageDown, 5),
                command_result("reader", "page up", "pageup", Command::PageUp, 6),
            ],
            AppSurface::Diff => {
                let mut commands = vec![
                    command_result("search", "file search", "f", Command::OpenFileSearch, 0),
                    command_result("search", "diff search", "/", Command::OpenTextSearch, 1),
                    command_result("review", "review items", ":", Command::OpenInbox, 2),
                    command_result("navigation", "back to queue", "esc", Command::Back, 3),
                    command_result("navigation", "open thread", "enter", Command::OpenThread, 4),
                    command_result("view", "toggle diff mode", "m", Command::ToggleDiffMode, 5),
                    command_result("navigation", "next file", "]", Command::NextFile, 6),
                    command_result("navigation", "previous file", "[", Command::PreviousFile, 7),
                    command_result("navigation", "next hunk", "N", Command::NextHunk, 8),
                    command_result("navigation", "previous hunk", "p", Command::PreviousHunk, 9),
                    command_result("navigation", "first row", "g", Command::JumpFirst, 10),
                    command_result("navigation", "last row", "G", Command::JumpLast, 11),
                    command_result("history", "show attempts", "A", Command::ShowAttempts, 12),
                ];
                if matches!(self.diff_source, DiffSource::LocalWorktree(_)) {
                    commands.splice(
                        3..3,
                        [
                            command_result("compose", "ask question", "a", Command::NewQuestion, 3),
                            command_result(
                                "compose",
                                "new instruction",
                                "i",
                                Command::NewInstruction,
                                4,
                            ),
                            command_result("compose", "new note", "n", Command::NewNote, 5),
                        ],
                    );
                }
                commands
            }
        };
        let offset = commands.len();
        commands.extend([
            command_result(
                "global",
                "theme picker",
                "T",
                Command::OpenThemePicker,
                offset,
            ),
            command_result("global", "quit", "q", Command::Quit, offset + 1),
        ]);
        commands
    }

    fn context_has_command(&self, command: Command) -> bool {
        self.contextual_command_results()
            .iter()
            .any(|result| result.command == command)
    }

    pub(super) fn filtered_file_results(&self) -> Vec<FinderResult> {
        let query = self.file_picker_query.trim();
        if query.is_empty() {
            return self
                .document
                .files
                .iter()
                .enumerate()
                .map(|(index, _)| FinderResult {
                    index,
                    score: 0,
                    matched: Vec::new(),
                })
                .collect();
        }
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut chars = Vec::new();
        let mut indices = Vec::new();
        let mut results: Vec<_> = self
            .document
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                nucleo_match_with(
                    &pattern,
                    &mut matcher,
                    &mut chars,
                    &mut indices,
                    &file.new_path,
                )
                .map(|(score, matched)| FinderResult {
                    index,
                    score,
                    matched,
                })
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| {
                    self.document.files[a.index]
                        .new_path
                        .len()
                        .cmp(&self.document.files[b.index].new_path.len())
                })
                .then_with(|| {
                    self.document.files[a.index]
                        .new_path
                        .cmp(&self.document.files[b.index].new_path)
                })
        });
        results
    }

    pub(super) fn filtered_text_results(&self) -> Vec<TextSearchResult> {
        let query = self.file_picker_query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut chars = Vec::new();
        let mut results = Vec::new();
        for (file_index, file) in self.document.files.iter().enumerate() {
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                for (line_index, line) in hunk.lines.iter().enumerate() {
                    let (old_line, new_line, text, kind) = match line {
                        DiffLine::Context {
                            old_line,
                            new_line,
                            text,
                            ..
                        } => (Some(*old_line), Some(*new_line), text.as_str(), " "),
                        DiffLine::Add { new_line, text, .. } => {
                            (None, Some(*new_line), text.as_str(), "+")
                        }
                        DiffLine::Delete { old_line, text, .. } => {
                            (Some(*old_line), None, text.as_str(), "-")
                        }
                    };
                    let text_score = nucleo_score_with(&pattern, &mut matcher, &mut chars, text);
                    let path_score =
                        nucleo_score_with(&pattern, &mut matcher, &mut chars, &file.new_path)
                            .map(|score| score / 2);
                    if let Some(score) = text_score.or(path_score) {
                        results.push(TextSearchResult {
                            file_index,
                            hunk_index,
                            line_index,
                            old_line,
                            new_line,
                            kind,
                            text: text.to_string(),
                            score,
                        });
                    }
                }
            }
        }
        results.sort_by(|a, b| {
            a.file_index
                .cmp(&b.file_index)
                .then_with(|| b.score.cmp(&a.score))
                .then_with(|| a.hunk_index.cmp(&b.hunk_index))
                .then_with(|| a.line_index.cmp(&b.line_index))
        });
        results.truncate(300);
        results
    }

    pub(super) fn filtered_inbox_notes(&self) -> Vec<&ReviewNote> {
        let query = self.file_picker_query.trim().to_ascii_lowercase();
        let mut notes: Vec<_> = self.session.notes.iter().collect();
        if !query.is_empty() {
            notes.retain(|note| {
                note.body.to_ascii_lowercase().contains(&query)
                    || note.target.path().to_ascii_lowercase().contains(&query)
                    || note.kind.label().contains(&query)
            });
        }
        notes.sort_by_key(|note| (note.state.sort_key(), note.id));
        notes
    }

    pub(super) fn jump_to_review_item(&mut self, note: &ReviewNote, rows: usize) {
        let Some(row) = self.document.line_row(
            self.state.mode,
            note.target.start.file_index,
            note.target.start.hunk_index,
            note.target.start.line_index,
        ) else {
            return;
        };
        self.state.clear_mouse_selection();
        self.state.selected_side = note.target.side();
        self.state.selected_row = row.min(rows.saturating_sub(1));
        self.state.scroll_y = self
            .state
            .selected_row
            .saturating_sub(STICKY_FILE_OVERLAY_ROWS + 2)
            .min(rows.saturating_sub(self.viewport_height));
    }
}

pub(crate) fn render_finder_row(
    file: &FileDiff,
    result: &FinderResult,
    width: usize,
    selected: bool,
    palette: FinderPalette,
) -> Line<'static> {
    let bg = if selected {
        palette.selected_bg
    } else {
        palette.bg
    };
    let fg = if selected {
        palette.selected_fg
    } else {
        palette.fg
    };
    let muted = if selected {
        palette.selected_muted
    } else {
        palette.muted
    };
    let accent = if selected {
        palette.selected_fg
    } else {
        palette.accent
    };
    let status_color = if selected {
        palette.selected_fg
    } else {
        match file_status(file) {
            "+" => palette.add,
            "-" => palette.del,
            "↻" => palette.accent,
            _ => palette.muted,
        }
    };
    let base = Style::new().fg(fg).bg(bg);
    let muted_style = Style::new().fg(muted).bg(bg);
    let stats = file_stats(file.additions(), file.deletions());
    let path_width = width.saturating_sub(4 + stats.chars().count());
    let path = truncate_middle(&file.new_path, path_width);
    let path_char_len = path.chars().count();
    let gap = width.saturating_sub(2 + path_char_len + stats.chars().count());
    let mut spans = vec![
        Span::styled(
            file_status(file).to_string(),
            Style::new()
                .fg(status_color)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::new().bg(bg)),
    ];
    spans.extend(styled_path_spans(
        &path,
        &result.matched,
        base,
        muted_style,
        accent,
        bg,
    ));
    spans.push(Span::styled(" ".repeat(gap), Style::new().bg(bg)));
    spans.push(Span::styled(
        stats,
        Style::new()
            .fg(status_color)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans).style(Style::new().bg(bg))
}

pub(crate) fn render_text_search_file_header(
    file: &FileDiff,
    width: usize,
    palette: FinderPalette,
) -> Line<'static> {
    let status_color = match file_status(file) {
        "+" => palette.add,
        "-" => palette.del,
        "↻" => palette.accent,
        _ => palette.muted,
    };
    let stats = file_stats(file.additions(), file.deletions());
    let path_width = width.saturating_sub(4 + stats.chars().count());
    let path = truncate_middle(&file.new_path, path_width);
    let used_width = 2 + path.chars().count() + 2 + stats.chars().count();
    let gap = width.saturating_sub(used_width);
    Line::from(vec![
        Span::styled(
            file_status(file).to_string(),
            Style::new()
                .fg(status_color)
                .bg(palette.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::new().bg(palette.bg)),
        Span::styled(
            path,
            Style::new()
                .fg(palette.fg)
                .bg(palette.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(gap + 2), Style::new().bg(palette.bg)),
        Span::styled(
            stats,
            Style::new()
                .fg(status_color)
                .bg(palette.bg)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .style(Style::new().bg(palette.bg))
}

pub(crate) fn render_text_search_row(
    result: &TextSearchResult,
    line: &DiffLine,
    query: &str,
    width: usize,
    selected: bool,
    palette: FinderPalette,
) -> Line<'static> {
    let bg = if selected {
        palette.selected_bg
    } else {
        palette.bg
    };
    let fg = if selected {
        palette.selected_fg
    } else {
        palette.fg
    };
    let theme = crate::design_system::QuiverTheme::for_variant(palette.variant).diff_theme();
    let kind_color = if selected {
        palette.selected_fg
    } else {
        match result.kind {
            "+" => palette.add,
            "-" => palette.del,
            _ => palette.muted,
        }
    };
    let line_number = result
        .new_line
        .or(result.old_line)
        .map_or(String::new(), |line| line.to_string());
    let prefix = format!("  {} {:>5} ", result.kind, line_number);
    let available = width.saturating_sub(prefix.chars().count());
    let text = truncate(result.text.trim(), available);
    let used_width = prefix.chars().count() + text.chars().count();
    let (syntax_spans, inline_spans) = match line {
        DiffLine::Context { syntax_spans, .. } => (syntax_spans.as_slice(), &[][..]),
        DiffLine::Add {
            syntax_spans,
            inline_spans,
            ..
        }
        | DiffLine::Delete {
            syntax_spans,
            inline_spans,
            ..
        } => (syntax_spans.as_slice(), inline_spans.as_slice()),
    };
    let mut match_indices = nucleo_match(&text, query)
        .map(|(_, indices)| indices)
        .unwrap_or_default();
    match_indices.sort_unstable();
    match_indices.dedup();
    let mut spans = vec![Span::styled(
        prefix,
        Style::new()
            .fg(kind_color)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )];
    let base = Style::new().fg(fg).bg(bg);
    spans.extend(preview_search_syntax_spans(
        &text,
        syntax_spans,
        inline_spans,
        line,
        theme,
        base,
        &match_indices,
        selected,
        palette,
    ));
    if used_width < width {
        spans.push(Span::styled(
            " ".repeat(width - used_width),
            Style::new().bg(bg),
        ));
    }
    Line::from(spans).style(Style::new().bg(bg))
}

pub(crate) fn preview_unified_diff_line(
    line: &DiffLine,
    width: usize,
    theme: DiffTheme,
    palette: FinderPalette,
) -> Line<'static> {
    let (old_line, new_line, mark, text, syntax_spans, inline_spans, line_bg, mark_fg) = match line
    {
        DiffLine::Context {
            old_line,
            new_line,
            text,
            syntax_spans,
        } => (
            Some(*old_line),
            Some(*new_line),
            " ",
            text.as_str(),
            syntax_spans.as_slice(),
            &[][..],
            theme.context_content_bg,
            theme.muted,
        ),
        DiffLine::Add {
            new_line,
            text,
            syntax_spans,
            inline_spans,
        } => (
            None,
            Some(*new_line),
            "+",
            text.as_str(),
            syntax_spans.as_slice(),
            inline_spans.as_slice(),
            theme.add_bg,
            theme.add_fg,
        ),
        DiffLine::Delete {
            old_line,
            text,
            syntax_spans,
            inline_spans,
        } => (
            Some(*old_line),
            None,
            "-",
            text.as_str(),
            syntax_spans.as_slice(),
            inline_spans.as_slice(),
            theme.del_bg,
            theme.del_fg,
        ),
    };
    let gutter = format!(
        "{:>4} {:>4} {} ",
        preview_line_num(old_line),
        preview_line_num(new_line),
        mark,
    );
    let text_width = width.saturating_sub(gutter.chars().count());
    let text = truncate(text, text_width);
    let base = Style::new().fg(theme.text).bg(line_bg);
    let gutter_numbers = gutter[..gutter.len().saturating_sub(2)].to_string();
    let mut spans = vec![
        Span::styled(gutter_numbers, Style::new().fg(theme.muted).bg(line_bg)),
        Span::styled(
            format!(" {mark} "),
            Style::new()
                .fg(mark_fg)
                .bg(line_bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    spans.extend(preview_syntax_spans(
        &text,
        syntax_spans,
        inline_spans,
        line,
        theme,
        base,
    ));
    if text.chars().count() < text_width {
        spans.push(Span::styled(
            " ".repeat(text_width - text.chars().count()),
            base,
        ));
    }
    Line::from(spans).style(Style::new().fg(palette.fg).bg(line_bg))
}

pub(crate) fn preview_unified_search_line(
    line: &DiffLine,
    width: usize,
    theme: DiffTheme,
    palette: FinderPalette,
    query: &str,
    selected: bool,
) -> Line<'static> {
    let (old_line, new_line, mark, text, syntax_spans, inline_spans, line_bg, mark_fg) = match line
    {
        DiffLine::Context {
            old_line,
            new_line,
            text,
            syntax_spans,
        } => (
            Some(*old_line),
            Some(*new_line),
            " ",
            text.as_str(),
            syntax_spans.as_slice(),
            &[][..],
            if selected {
                theme.selected
            } else {
                theme.context_content_bg
            },
            theme.muted,
        ),
        DiffLine::Add {
            new_line,
            text,
            syntax_spans,
            inline_spans,
        } => (
            None,
            Some(*new_line),
            "+",
            text.as_str(),
            syntax_spans.as_slice(),
            inline_spans.as_slice(),
            if selected {
                theme.add_content_bg
            } else {
                theme.add_bg
            },
            theme.add_fg,
        ),
        DiffLine::Delete {
            old_line,
            text,
            syntax_spans,
            inline_spans,
        } => (
            Some(*old_line),
            None,
            "-",
            text.as_str(),
            syntax_spans.as_slice(),
            inline_spans.as_slice(),
            if selected {
                theme.del_content_bg
            } else {
                theme.del_bg
            },
            theme.del_fg,
        ),
    };
    let gutter = format!(
        "{:>4} {:>4} {} ",
        preview_line_num(old_line),
        preview_line_num(new_line),
        mark,
    );
    let text_width = width.saturating_sub(gutter.chars().count());
    let text = truncate(text, text_width);
    let mut match_indices = nucleo_match(&text, query)
        .map(|(_, indices)| indices)
        .unwrap_or_default();
    match_indices.sort_unstable();
    match_indices.dedup();
    let base = Style::new().fg(theme.text).bg(line_bg);
    let gutter_numbers = gutter[..gutter.len().saturating_sub(2)].to_string();
    let mut spans = vec![
        Span::styled(gutter_numbers, Style::new().fg(theme.muted).bg(line_bg)),
        Span::styled(
            format!(" {mark} "),
            Style::new()
                .fg(mark_fg)
                .bg(line_bg)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    spans.extend(preview_search_syntax_spans(
        &text,
        syntax_spans,
        inline_spans,
        line,
        theme,
        base,
        &match_indices,
        selected,
        palette,
    ));
    if text.chars().count() < text_width {
        spans.push(Span::styled(
            " ".repeat(text_width - text.chars().count()),
            base,
        ));
    }
    Line::from(spans).style(Style::new().fg(palette.fg).bg(line_bg))
}

pub(crate) fn preview_line_num(line: Option<u32>) -> String {
    line.map_or_else(String::new, |line| line.to_string())
}

pub(crate) fn preview_syntax_spans(
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    line: &DiffLine,
    theme: DiffTheme,
    base: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut inline_index = 0usize;
    let is_add = matches!(line, DiffLine::Add { .. });
    let is_delete = matches!(line, DiffLine::Delete { .. });
    for (byte_index, ch) in text.char_indices() {
        while inline_index < inline_spans.len() && inline_spans[inline_index].end <= byte_index {
            inline_index += 1;
        }
        let mut style = if let Some(span) = active_preview_syntax_span(syntax_spans, byte_index) {
            preview_syntax_style(base, span, theme)
        } else {
            base
        };
        if inline_index < inline_spans.len()
            && inline_spans[inline_index].start <= byte_index
            && byte_index < inline_spans[inline_index].end
        {
            let bg = if is_add {
                theme.add_content_bg
            } else if is_delete {
                theme.del_content_bg
            } else {
                base.bg.unwrap_or(theme.bg)
            };
            style = style.bg(bg).add_modifier(Modifier::BOLD);
        }
        push_preview_span(&mut spans, ch, style);
    }
    spans
}

pub(crate) fn preview_search_syntax_spans(
    text: &str,
    syntax_spans: &[SyntaxSpan],
    inline_spans: &[InlineDiffSpan],
    line: &DiffLine,
    theme: DiffTheme,
    base: Style,
    match_indices: &[usize],
    selected: bool,
    palette: FinderPalette,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut inline_index = 0usize;
    let is_add = matches!(line, DiffLine::Add { .. });
    let is_delete = matches!(line, DiffLine::Delete { .. });
    for (byte_index, ch) in text.char_indices() {
        while inline_index < inline_spans.len() && inline_spans[inline_index].end <= byte_index {
            inline_index += 1;
        }
        let mut style = if let Some(span) = active_preview_syntax_span(syntax_spans, byte_index) {
            preview_syntax_style(base, span, theme)
        } else {
            base
        };
        if inline_index < inline_spans.len()
            && inline_spans[inline_index].start <= byte_index
            && byte_index < inline_spans[inline_index].end
        {
            let bg = if is_add {
                theme.add_content_bg
            } else if is_delete {
                theme.del_content_bg
            } else {
                base.bg.unwrap_or(theme.bg)
            };
            style = style.bg(bg).add_modifier(Modifier::BOLD);
        }
        if match_indices.binary_search(&byte_index).is_ok() {
            let fg = if selected { theme.bg } else { palette.bg };
            style = style.fg(fg).bg(palette.accent).add_modifier(Modifier::BOLD);
        }
        push_preview_span(&mut spans, ch, style);
    }
    spans
}

pub(crate) fn active_preview_syntax_span(
    spans: &[SyntaxSpan],
    byte_index: usize,
) -> Option<&SyntaxSpan> {
    let end = spans.partition_point(|span| span.start <= byte_index);
    spans[..end].iter().rev().find(|span| byte_index < span.end)
}

pub(crate) fn preview_syntax_style(base: Style, span: &SyntaxSpan, theme: DiffTheme) -> Style {
    if let Some(style) = span.style {
        let mut merged = base;
        if let Some(fg) = style.fg {
            merged = merged.fg(fg);
        }
        merged.add_modifier(style.add_modifier)
    } else {
        match span.kind {
            SyntaxHighlightKind::Comment => {
                base.fg(theme.syntax.comment).add_modifier(Modifier::ITALIC)
            }
            SyntaxHighlightKind::Keyword => {
                base.fg(theme.syntax.keyword).add_modifier(Modifier::BOLD)
            }
            SyntaxHighlightKind::String | SyntaxHighlightKind::Markup => {
                base.fg(theme.syntax.string)
            }
            SyntaxHighlightKind::Number | SyntaxHighlightKind::Boolean => {
                base.fg(theme.syntax.number).add_modifier(Modifier::BOLD)
            }
            SyntaxHighlightKind::Function => base.fg(theme.syntax.function),
            SyntaxHighlightKind::Type => base.fg(theme.syntax.r#type),
            SyntaxHighlightKind::Property => base.fg(theme.syntax.property),
            SyntaxHighlightKind::Punctuation => base.fg(theme.syntax.punctuation),
        }
    }
}

pub(crate) fn push_preview_span(spans: &mut Vec<Span<'static>>, ch: char, style: Style) {
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            last.content.to_mut().push(ch);
            return;
        }
    }
    spans.push(Span::styled(ch.to_string(), style));
}

pub(crate) fn render_modal_diff_scrollbar(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    content_len: usize,
    viewport_len: usize,
    position: usize,
) {
    render_scrollbar(area, buf, content_len, viewport_len, position);
}

pub(crate) fn styled_path_spans(
    path: &str,
    matched: &[usize],
    base: Style,
    muted: Style,
    accent: Color,
    bg: Color,
) -> Vec<Span<'static>> {
    let basename_start = path.rfind('/').map_or(0, |index| index + 1);
    let mut spans = Vec::new();
    let mut segment = String::new();
    let mut segment_style: Option<Style> = None;
    for (char_index, ch) in path.chars().enumerate() {
        let is_match = matched.contains(&char_index);
        let style = if is_match {
            Style::new().fg(accent).bg(bg).add_modifier(Modifier::BOLD)
        } else if char_index < basename_start {
            muted
        } else {
            base.add_modifier(Modifier::BOLD)
        };
        if segment_style == Some(style) {
            segment.push(ch);
        } else {
            if !segment.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut segment),
                    segment_style.unwrap_or(base),
                ));
            }
            segment.push(ch);
            segment_style = Some(style);
        }
    }
    if !segment.is_empty() {
        spans.push(Span::styled(segment, segment_style.unwrap_or(base)));
    }
    spans
}

pub(crate) fn nucleo_match(haystack: &str, query: &str) -> Option<(u32, Vec<usize>)> {
    let query = query.trim();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut chars = Vec::new();
    let mut indices = Vec::new();
    nucleo_match_with(&pattern, &mut matcher, &mut chars, &mut indices, haystack)
}

pub(crate) fn nucleo_match_with(
    pattern: &Pattern,
    matcher: &mut Matcher,
    chars: &mut Vec<char>,
    indices: &mut Vec<u32>,
    haystack: &str,
) -> Option<(u32, Vec<usize>)> {
    chars.clear();
    indices.clear();
    let score = pattern.indices(Utf32Str::new(haystack, chars), matcher, indices)?;
    indices.sort_unstable();
    indices.dedup();
    Some((score, indices.iter().map(|index| *index as usize).collect()))
}

pub(crate) fn nucleo_score_with(
    pattern: &Pattern,
    matcher: &mut Matcher,
    chars: &mut Vec<char>,
    haystack: &str,
) -> Option<u32> {
    chars.clear();
    pattern.score(Utf32Str::new(haystack, chars), matcher)
}

pub(crate) fn file_status(file: &FileDiff) -> &'static str {
    match (file.old_path.as_deref(), file.additions(), file.deletions()) {
        (_, additions, 0) if additions > 0 => "+",
        (_, 0, deletions) if deletions > 0 => "-",
        (Some(old), _, _) if old != file.new_path => "↻",
        _ => "~",
    }
}

pub(crate) fn file_stats(additions: usize, deletions: usize) -> String {
    match (additions, deletions) {
        (0, 0) => "0".to_string(),
        (additions, 0) => format!("+{additions}"),
        (0, deletions) => format!("-{deletions}"),
        (additions, deletions) => format!("+{additions} -{deletions}"),
    }
}

pub(crate) fn target_line_label(target: &DiffLineTarget) -> String {
    let prefix = match target.kind {
        DiffLineKind::Add => "+",
        DiffLineKind::Delete => "-",
        DiffLineKind::Context => match target.side {
            DiffSide::Left => "-",
            DiffSide::Right => "+",
        },
    };
    format!("{prefix}{}", target.line)
}

pub(crate) fn target_range_label(target: &DiffLineRangeTarget) -> String {
    let start = target_line_label(&target.start);
    if target.is_single_line() {
        start
    } else {
        format!("{start}..{}", target_line_label(&target.end))
    }
}

pub(crate) fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
