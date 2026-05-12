use super::*;

const DETAIL_DESCRIPTION_ROW_LIMIT: usize = 2000;

/// Find the first and last row indices in `rows` belonging to the
/// comment at `selection`. Returns (0, 0) when no rows match (empty list
/// or out-of-range selection).
pub(super) fn comment_row_span(rows: &[CommentSurfaceRow], selection: usize) -> (usize, usize) {
    let first = rows.iter().position(|r| r.comment_index() == selection);
    let last = rows.iter().rposition(|r| r.comment_index() == selection);
    match (first, last) {
        (Some(a), Some(b)) => (a, b),
        _ => (0, 0),
    }
}

fn semantic_change_marker(change_type: &str) -> &'static str {
    match change_type {
        "added" => "+",
        "deleted" => "-",
        "renamed" | "moved" | "reordered" => "→",
        _ => "~",
    }
}

fn centered_line_rect(area: Rect, y: u16, width: usize) -> Rect {
    let width = (width as u16).min(area.width);
    let x = area.x + area.width.saturating_sub(width) / 2;
    Rect::new(x, y, width, 1)
}

impl App {
    pub(super) fn render_commit_list(&mut self, frame: &mut Frame) {
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let branch = self
            .commit_route
            .as_ref()
            .map(|route| route.branch.clone())
            .or_else(|| {
                self.commit_pr_route
                    .as_ref()
                    .map(|(_, number)| format!("PR #{number}"))
            })
            .unwrap_or_else(|| "branch".to_string());
        let summary = self
            .commit_status
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "{} commit{}",
                    self.commits.len(),
                    plural_s(self.commits.len())
                )
            });
        AppHeader {
            brand: "QUIVER",
            scope: &format!("commits · {branch}"),
            viewer: self.github.viewer.as_deref().unwrap_or("local"),
            summary: &summary,
            is_fetching: self.query_client.is_fetching(),
            spinner: self.spinner(),
            palette,
        }
        .render(frame, header);
        draw_horizontal_rule(
            frame.buffer_mut(),
            header.y + 1,
            area.x,
            area.right(),
            palette.rule,
            palette.bg,
        );

        let heading = palette.text(TextRole::Heading);
        let text = palette.text(TextRole::Body);
        let muted = palette.text(TextRole::Muted);
        let key = palette.text(TextRole::Key);
        let selected_style = Style::new()
            .fg(palette.selected_text)
            .bg(palette.selected_bg)
            .add_modifier(Modifier::BOLD);
        let (list_area, meta_area) = if body.width >= 96 {
            let [list, _gap, meta] = Layout::horizontal([
                Constraint::Percentage(56),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .areas(body);
            (list, Some(meta))
        } else {
            (body, None)
        };
        let mut y = list_area.y;
        frame.render_widget(
            Line::from(vec![Span::raw(" "), Span::styled("commit list", heading)]),
            Rect::new(list_area.x, y, list_area.width, 1),
        );
        y += 2;
        if self.commits.is_empty() {
            let label = self.commit_status.as_deref().unwrap_or("no branch commits");
            frame.render_widget(
                Line::from(vec![Span::raw(" "), Span::styled(label, muted)]),
                Rect::new(list_area.x, y, list_area.width, 1),
            );
        } else {
            self.commit_selection = self
                .commit_selection
                .min(self.commits.len().saturating_sub(1));
            for (index, commit) in self.commits.iter().enumerate() {
                if y >= list_area.bottom() {
                    break;
                }
                let selected = index == self.commit_selection;
                let style = if selected { selected_style } else { text };
                let sha_style = if selected { selected_style } else { heading };
                let bg = if selected {
                    palette.selected_bg
                } else {
                    palette.bg
                };
                let line = Line::from(vec![
                    Span::styled(" ", Style::new().bg(bg)),
                    Span::styled(format!("{} ", commit.short_sha), sha_style),
                    Span::styled(
                        truncate(&commit.subject, list_area.width.saturating_sub(22) as usize),
                        style,
                    ),
                    Span::styled(
                        right_aligned_text(
                            list_area.width,
                            commit.short_sha.chars().count() + 2,
                            &format!("{} files", commit.files.len()),
                        ),
                        if selected { selected_style } else { muted },
                    ),
                ]);
                frame.render_widget(
                    line.style(Style::new().bg(bg)),
                    Rect::new(list_area.x, y, list_area.width, 1),
                );
                y += 1;
            }
        }
        if let Some(meta_area) = meta_area {
            self.render_commit_metadata(frame, meta_area, palette);
        }
        frame.render_widget(
            Line::from(vec![
                Span::styled("tab", key),
                Span::styled(" diff  ", muted),
                Span::styled("enter", key),
                Span::styled(" diff  ", muted),
                Span::styled("esc", key),
                Span::styled(" back", muted),
            ]),
            footer,
        );
    }

    fn render_commit_metadata(&mut self, frame: &mut Frame, area: Rect, palette: HomePalette) {
        let Some(commit) = self.commits.get(self.commit_selection).cloned() else {
            return;
        };
        let heading = palette.text(TextRole::Heading);
        let text = palette.text(TextRole::Body);
        let muted = palette.text(TextRole::Muted);
        let amber = Style::new().fg(palette.orange).bg(palette.bg);
        let mut y = area.y;
        frame.render_widget(
            Line::from(vec![
                Span::styled(commit.short_sha.clone(), heading),
                Span::styled(format!(" · {}", commit.author), muted),
            ]),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
        frame.render_widget(
            Line::from(Span::styled(
                truncate(&commit.subject, area.width as usize),
                text.add_modifier(Modifier::BOLD),
            )),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 2;

        if let Some(route) = &self.commit_route {
            let source = DiffSource::Commit {
                repo_path: route.repo_path.clone(),
                sha: commit.sha.clone(),
            };
            y = self.render_semantic_tree(
                frame,
                Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y)),
                &source,
                palette,
            );
            if y < area.bottom() {
                y += 1;
            }
        }

        frame.render_widget(
            Line::from(vec![
                Span::styled("Diff files", heading),
                Span::styled(format!(" · {}", commit.files.len()), muted),
            ]),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
        for file in commit
            .files
            .iter()
            .take(area.height.saturating_sub(4) as usize)
        {
            if y >= area.bottom() {
                break;
            }
            frame.render_widget(
                Line::from(vec![
                    Span::styled(format!("{} ", file.status), amber),
                    Span::styled(
                        truncate(&file.path, area.width.saturating_sub(3) as usize),
                        text,
                    ),
                ]),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
    }

    pub(super) fn render_home(&mut self, frame: &mut Frame) {
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        if area.width < 32 || area.height < 8 {
            frame.render_widget(Line::from("Quiver"), area);
            return;
        }

        let [header, body, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        self.home_selection = self.home_selection.min(items.len().saturating_sub(1));
        let selected = items.get(self.home_selection).unwrap_or(&items[0]);

        let title = palette.text(TextRole::Selected);
        let heading = palette.text(TextRole::Heading);
        let text = palette.text(TextRole::Body);
        let muted = palette.text(TextRole::Muted);
        let rule = Style::new().fg(palette.rule).bg(palette.bg);
        let key = palette.text(TextRole::Key);

        let viewer = self.github.viewer.as_deref().unwrap_or("local");
        let scope = self.scope_label();
        AppHeader {
            brand: "QUIVER",
            scope: &scope,
            viewer,
            summary: &self.github_summary(),
            is_fetching: self.query_client.is_fetching(),
            spinner: self.spinner(),
            palette,
        }
        .render(frame, header);
        draw_horizontal_rule(
            frame.buffer_mut(),
            header.y + 1,
            area.x,
            area.right(),
            palette.rule,
            palette.bg,
        );

        if area.width >= 118 {
            self.render_home_wide(frame, body, footer, &items, selected, palette);
            return;
        }

        let content = body;
        let mut y = content.y;
        let machine = selected.machine_name(self);
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("#{}", selected.id), heading),
                Span::styled(format!(" {machine}"), muted),
                Span::styled(
                    right_aligned_text(content.width, machine.chars().count() + 5, &selected.age),
                    muted,
                ),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(selected.title.to_string(), title),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        let stats_left = content.width.saturating_sub(22) as usize;
        frame.render_widget(
            Line::from(vec![
                Span::styled(" ".repeat(stats_left), muted),
                Span::styled(
                    format!("+{}", self.document.additions()),
                    Style::new().fg(palette.success).bg(palette.bg),
                ),
                Span::styled(
                    format!(" -{}", self.document.deletions()),
                    Style::new().fg(palette.danger).bg(palette.bg),
                ),
                Span::styled(format!(" · {} files", self.document.files.len()), muted),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        // Blank-line gap between sections — Amp uses whitespace, not rules.
        y += 1;

        let is_pr = selected.pull_request(self).is_some();
        let (label, summary) = if is_pr {
            let count = selected
                .pull_request(self)
                .map(|pr| pr.comments.len())
                .unwrap_or(0);
            (
                "Comments",
                format!("{} comment{} · press c to view all", count, plural_s(count)),
            )
        } else {
            let count = self.local_review_session().notes.len();
            (
                "Review items",
                format!("{} item{} · press : to view all", count, plural_s(count)),
            )
        };
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(label, heading),
                Span::styled(
                    right_aligned_text(content.width, label.chars().count() + 1, &summary),
                    muted,
                ),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        y += 1;

        if let Some(pr) = selected.pull_request(self) {
            let body = pr.body.clone();
            let limit = content.bottom().saturating_sub(y) as usize;
            for line in body_preview_lines(&body, content.width, limit, &palette) {
                if y >= content.bottom() {
                    break;
                }
                frame.render_widget(line, Rect::new(content.x, y, content.width, 1));
                y += 1;
            }
        } else {
            for line in selected.description(self) {
                if y >= content.bottom() {
                    break;
                }
                frame.render_widget(
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            truncate(&line, content.width.saturating_sub(2) as usize),
                            text,
                        ),
                    ]),
                    Rect::new(content.x, y, content.width, 1),
                );
                y += 1;
            }
        }
        y += 1;

        if y < content.bottom() {
            frame.render_widget(
                Line::from(vec![Span::raw(" "), Span::styled("inbox", heading)]),
                Rect::new(content.x, y, content.width, 1),
            );
            y += 1;
        }

        let mut previous_group: Option<&str> = None;
        for (index, item) in items.iter().enumerate() {
            if y >= content.bottom() {
                break;
            }
            if previous_group != Some(item.group.as_str()) {
                if previous_group.is_some() && y + 1 < content.bottom() {
                    // One blank row between groups for vertical rhythm.
                    y += 1;
                }
                if y >= content.bottom() {
                    break;
                }
                self.render_queue_group_header(
                    frame,
                    Rect::new(content.x, y, content.width, 1),
                    &item.group,
                    palette,
                );
                y += 1;
                previous_group = Some(item.group.as_str());
            }
            if y >= content.bottom() {
                break;
            }
            self.render_quiver_work_item(
                frame,
                Rect::new(content.x, y, content.width, 1),
                item,
                index == self.home_selection,
                palette,
            );
            y += 1;
        }

        if let Some(notice) = self.github_notice() {
            if y + 1 < content.bottom() {
                y += 1;
                frame.render_widget(
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            truncate(&notice, content.width.saturating_sub(2) as usize),
                            muted,
                        ),
                    ]),
                    Rect::new(content.x, y, content.width, 1),
                );
            }
        }

        render_home_rule(frame, content, content.bottom().saturating_sub(1), rule);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" /", key),
                Span::styled(" filter  ", muted),
                Span::styled("tab", key),
                Span::styled(" commits  ", muted),
                Span::styled("enter", key),
                Span::styled(" details  ", muted),
                Span::styled("d", key),
                Span::styled(" diff  ", muted),
                Span::styled("o", key),
                Span::styled(" github  ", muted),
                Span::styled("p", key),
                Span::styled(" pull  ", muted),
                Span::styled("P", key),
                Span::styled(" push  ", muted),
                Span::styled("c", key),
                Span::styled(" comments  ", muted),
                Span::styled(":", key),
                Span::styled(" review items  ", muted),
                Span::styled("ctrl-p", key),
                Span::styled(" commands  ", muted),
                Span::styled("T", key),
                Span::styled(format!(" theme:{}", self.theme_variant.label()), muted),
            ]),
            footer,
        );
    }

    pub(super) fn render_home_wide(
        &mut self,
        frame: &mut Frame,
        body: Rect,
        footer: Rect,
        items: &[WorkItem],
        selected: &WorkItem,
        palette: HomePalette,
    ) {
        let [queue, _gap, details] = Layout::horizontal([
            Constraint::Percentage(58),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(body);
        let heading = palette.text(TextRole::Heading);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let key = palette.text(TextRole::Key);

        let mut y = queue.y;
        frame.render_widget(
            Line::from(vec![Span::raw(" "), Span::styled("inbox", heading)]),
            Rect::new(queue.x, y, queue.width.saturating_sub(1), 1),
        );
        y += 1;
        // Breathing room between the inbox heading and the first group.
        if y + 1 < queue.bottom() {
            y += 1;
        }
        let mut previous_group: Option<&str> = None;
        for (index, item) in items.iter().enumerate() {
            if y >= queue.bottom() {
                break;
            }
            if previous_group != Some(item.group.as_str()) {
                if previous_group.is_some() && y + 1 < queue.bottom() {
                    // One blank row between groups for vertical rhythm.
                    y += 1;
                }
                if y >= queue.bottom() {
                    break;
                }
                self.render_queue_group_header(
                    frame,
                    Rect::new(queue.x, y, queue.width.saturating_sub(1), 1),
                    &item.group,
                    palette,
                );
                y += 1;
                previous_group = Some(item.group.as_str());
            }
            if y >= queue.bottom() {
                break;
            }
            self.render_quiver_work_item(
                frame,
                Rect::new(queue.x, y, queue.width.saturating_sub(1), 1),
                item,
                index == self.home_selection,
                palette,
            );
            y += 1;
        }

        if let Some(notice) = self.github_notice() {
            if y + 1 < queue.bottom() {
                y += 1;
                frame.render_widget(
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            truncate(&notice, queue.width.saturating_sub(3) as usize),
                            muted,
                        ),
                    ]),
                    Rect::new(queue.x, y, queue.width.saturating_sub(1), 1),
                );
            }
        }

        self.render_home_detail_pane(frame, details, selected, palette);
        // No vertical divider, no connector glyphs, no bottom rule. Amp-style:
        // panes are separated by whitespace alone, footer carries its own
        // emphasis without a horizontal line above it.
        frame.render_widget(
            Line::from(vec![
                Span::styled(" /", key),
                Span::styled(" filter  ", muted),
                Span::styled("tab", key),
                Span::styled(" commits  ", muted),
                Span::styled("enter", key),
                Span::styled(" details  ", muted),
                Span::styled("d", key),
                Span::styled(" diff  ", muted),
                Span::styled("o", key),
                Span::styled(" github  ", muted),
                Span::styled("p", key),
                Span::styled(" pull  ", muted),
                Span::styled("P", key),
                Span::styled(" push  ", muted),
                Span::styled("c", key),
                Span::styled(" comments  ", muted),
                Span::styled(":", key),
                Span::styled(" review items  ", muted),
                Span::styled("ctrl-p", key),
                Span::styled(" commands  ", muted),
                Span::styled("T", key),
                Span::styled(format!(" theme:{}", self.theme_variant.label()), muted),
            ]),
            footer,
        );
    }

    pub(super) fn render_home_detail_pane(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        selected: &WorkItem,
        palette: HomePalette,
    ) {
        let surface_bg = palette.layer_bg(SurfaceLayer::Surface);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(surface_bg));
        let title = palette.text(TextRole::Selected).bg(surface_bg);
        let heading = palette.text(TextRole::Heading).bg(surface_bg);
        let text = palette.text(TextRole::Body).bg(surface_bg);
        let muted = palette.text(TextRole::Muted).bg(surface_bg);
        let mut y = area.y;
        let pull_request = selected.pull_request(self);
        let machine = selected.machine_name(self);
        let content = Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(2),
            area.height,
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled(format!("#{}", selected.id), heading),
                Span::styled(format!(" {machine}"), muted),
                Span::styled(
                    right_aligned_text(content.width, machine.chars().count() + 4, &selected.age),
                    muted,
                ),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        frame.render_widget(
            Line::from(vec![Span::styled(selected.title.to_string(), title)]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        let (additions, deletions, changed_files) = pull_request.map_or(
            (
                self.document.additions(),
                self.document.deletions(),
                self.document.files.len(),
            ),
            |pull_request| {
                (
                    pull_request.additions,
                    pull_request.deletions,
                    pull_request.changed_files,
                )
            },
        );
        let add = format!("+{additions}");
        let del = format!(" -{deletions}");
        let files = format!(" · {changed_files} files");
        let stats_gap = (content.width as usize)
            .saturating_sub(add.chars().count() + del.chars().count() + files.chars().count());
        frame.render_widget(
            Line::from(vec![
                Span::styled(" ".repeat(stats_gap), muted),
                Span::styled(add, Style::new().fg(palette.success).bg(surface_bg)),
                Span::styled(del, Style::new().fg(palette.danger).bg(surface_bg)),
                Span::styled(files, muted),
            ]),
            Rect::new(content.x, y, content.width, 1),
        );
        y += 1;
        y += 1;

        // For non-PR rows (local worktrees) keep the "Review items"
        // heading as a useful at-a-glance hook. PR rows skip straight to
        // Checks → Description (GHUI parity; the Comments reader is one
        // keystroke away via `c`).
        if pull_request.is_none() {
            let count = self.local_review_session().notes.len();
            let summary = format!("{} item{} · press : to view all", count, plural_s(count));
            frame.render_widget(
                Line::from(vec![
                    Span::styled("Review items", heading),
                    Span::styled(
                        right_aligned_text(
                            content.width,
                            "Review items".chars().count() + 1,
                            &summary,
                        ),
                        muted,
                    ),
                ]),
                Rect::new(content.x, y, content.width, 1),
            );
            y += 1;
            y += 1;
        }

        if let Some(pull_request) = pull_request {
            // Checks section: only render heading + grid when there's
            // something to show. An empty-checks pane reads as honest;
            // an "0 / no checks" heading just adds vertical noise.
            if !pull_request.checks.is_empty() {
                if y < area.bottom() {
                    let check_summary = pull_request
                        .check_summary
                        .as_deref()
                        .unwrap_or_else(|| pull_request.check_status.label());
                    frame.render_widget(
                        Line::from(vec![
                            Span::styled("Checks", heading),
                            Span::styled(
                                right_aligned_text(content.width, 6, check_summary),
                                muted,
                            ),
                        ]),
                        Rect::new(content.x, y, content.width, 1),
                    );
                    y += 1;
                }
                // 2-column checks grid (GHUI parity). Each cell is half
                // the pane width minus the 1-col leading gutter.
                let checks: Vec<_> = pull_request.checks.iter().take(8).collect();
                let cell_width = (content.width.saturating_sub(1) / 2) as usize;
                for chunk in checks.chunks(2) {
                    if y >= area.bottom() {
                        break;
                    }
                    let mut spans: Vec<Span> = Vec::with_capacity(6);
                    for (col_idx, check) in chunk.iter().enumerate() {
                        let (symbol, color) = check.status_symbol();
                        if col_idx > 0 {
                            spans.push(Span::styled(" ", muted));
                        }
                        spans.push(Span::styled(
                            format!("{symbol} "),
                            Style::new().fg(color).bg(surface_bg),
                        ));
                        spans.push(Span::styled(
                            format!(
                                "{:<width$}",
                                truncate(&check.name, cell_width.saturating_sub(2)),
                                width = cell_width.saturating_sub(2)
                            ),
                            text,
                        ));
                    }
                    frame.render_widget(
                        Line::from(spans),
                        Rect::new(content.x, y, content.width, 1),
                    );
                    y += 1;
                }
                if y < area.bottom() {
                    y += 1;
                }
            }
        }

        if y >= area.bottom() {
            return;
        }
        self.render_detail_tabs(frame, Rect::new(content.x, y, content.width, 1), palette);
        y += 1;
        if y + 1 < area.bottom() {
            y += 1;
        }
        let tab_area = Rect::new(content.x, y, content.width, area.bottom().saturating_sub(y));
        match self.detail_tab {
            DetailTab::Semantic => {
                let semantic_route = selected.route(self);
                self.render_semantic_tree(frame, tab_area, &semantic_route, palette);
            }
            DetailTab::Description => {
                self.render_detail_description(frame, tab_area, selected, palette);
            }
        }
    }

    pub(super) fn render_detail_full(&mut self, frame: &mut Frame) {
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        let [header, divider, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        let Some(selected) = items.get(self.home_selection.min(items.len().saturating_sub(1)))
        else {
            return;
        };
        self.render_quiver_header(frame, header, palette, "details");
        draw_horizontal_rule(
            frame.buffer_mut(),
            divider.y,
            divider.x,
            divider.right(),
            palette.rule,
            palette.bg,
        );
        self.render_home_detail_pane(frame, body, selected, palette);
        self.render_surface_footer(
            frame,
            footer,
            &[
                ("esc", "back"),
                ("c", "comments"),
                ("d", "diff"),
                ("ctrl-p", "commands"),
            ],
            palette,
        );
    }

    pub(super) fn render_comments_surface(&mut self, frame: &mut Frame) {
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        let [header, top_rule, title_area, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let items = self.home_work_items();
        let Some(selected) = items.get(self.home_selection.min(items.len().saturating_sub(1)))
        else {
            return;
        };
        self.render_quiver_header(frame, header, palette, "comments");
        draw_horizontal_rule(
            frame.buffer_mut(),
            top_rule.y,
            top_rule.x,
            top_rule.right(),
            palette.rule,
            palette.bg,
        );

        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let heading = palette.text(TextRole::Heading);
        let comments = self.selected_comments(selected);
        let comment_status = selected.pull_request(self).map(|pull_request| {
            let query = self.query_client.get(&QueryKey::pull_request_comments(
                &pull_request.repository,
                pull_request.number,
            ));
            self.query_status_label(
                query,
                &QueryKey::pull_request_comments(&pull_request.repository, pull_request.number),
            )
        });
        let is_pr = selected.pull_request(self).is_some();
        let (label, item_word) = if is_pr {
            ("Comments", "comment")
        } else {
            ("Review items", "item")
        };
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(label, heading),
                Span::styled(format!(" #{}  {}", selected.id, selected.group), muted),
                Span::styled(
                    right_aligned_text(
                        title_area.width,
                        selected.group.chars().count() + label.chars().count() + 7,
                        &format!(
                            "{} {}{}",
                            comments.len(),
                            item_word,
                            plural_s(comments.len())
                        ),
                    ),
                    muted,
                ),
            ]),
            Rect::new(title_area.x, title_area.y, title_area.width, 1),
        );
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(&selected.title, title),
                Span::styled(
                    comment_status
                        .map(|status| format!("  {status}"))
                        .unwrap_or_default(),
                    muted,
                ),
            ]),
            Rect::new(title_area.x, title_area.y + 1, title_area.width, 1),
        );

        let rows = comment_surface_rows(&comments, body.width.saturating_sub(3) as usize, &palette);

        // Auto-scroll: keep the first and last row of the selected
        // comment within the body viewport. The selection drives scroll
        // (not the other way around) so j/k feels list-like.
        let selection = self.comments_selection;
        let (first_idx, last_idx) = comment_row_span(&rows, selection);
        let height = body.height as usize;
        if first_idx < self.surface_scroll_y {
            self.surface_scroll_y = first_idx;
        } else if last_idx >= self.surface_scroll_y.saturating_add(height) {
            self.surface_scroll_y = last_idx.saturating_sub(height.saturating_sub(1));
        }
        self.surface_scroll_y = self
            .surface_scroll_y
            .min(rows.len().saturating_sub(height.max(1)));

        let selected_bg = palette.layer_bg(SurfaceLayer::ElevatedSurface);
        let rail_style = Style::new().fg(palette.action).bg(selected_bg);
        let bullet = Style::new().fg(palette.accent).bg(palette.bg);
        for (visual_index, row) in rows
            .iter()
            .skip(self.surface_scroll_y)
            .take(height)
            .enumerate()
        {
            let y = body.y + visual_index as u16;
            let is_selected = row.comment_index() == selection;
            let row_rect = Rect::new(body.x, y, body.width, 1);

            // For selected rows, paint the full-width row bg first so any
            // trailing whitespace also picks up the elevation.
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
                    let mut spans: Vec<Span> = Vec::new();
                    if is_selected {
                        spans.push(Span::styled("┃", rail_style));
                        // Keep the gutter width identical to the unselected
                        // " ● " (3 cols): rail in col 1, `●` in col 2,
                        // single trailing space in col 3 so the author
                        // name lands at the same column either way.
                        spans.push(Span::styled(
                            "● ",
                            Style::new().fg(palette.accent).bg(selected_bg),
                        ));
                        spans.push(Span::styled(
                            author.clone(),
                            title.bg(selected_bg).add_modifier(Modifier::BOLD),
                        ));
                        spans.push(Span::styled(
                            format!(" · {age}"),
                            Style::new().fg(palette.muted).bg(selected_bg),
                        ));
                    } else {
                        spans.push(Span::styled(" ● ", bullet));
                        spans.push(Span::styled(author.clone(), title));
                        spans.push(Span::styled(format!(" · {age}"), muted));
                    }
                    frame.render_widget(Line::from(spans), row_rect);
                }
                CommentSurfaceRow::Body { line, .. } => {
                    if is_selected {
                        // Swap the 3-col leading gutter for `┃  ` so the
                        // amber rail sits in col 1 and the indent rhythm
                        // stays the same. Patch every other span's bg.
                        let mut line = line.clone();
                        if let Some(first) = line.spans.first_mut() {
                            let trimmed_len = first
                                .content
                                .chars()
                                .skip_while(|c| c.is_whitespace())
                                .count();
                            if trimmed_len == 0 {
                                // Whole first span was whitespace gutter.
                                let leading = first.content.chars().count();
                                let after_rail = leading.saturating_sub(1);
                                first.content = " ".repeat(after_rail).into();
                                first.style = first.style.bg(selected_bg);
                            } else {
                                first.style = first.style.bg(selected_bg);
                            }
                        }
                        // Prepend the rail span.
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
        self.render_surface_footer(
            frame,
            footer,
            &[
                ("↑↓", "comment"),
                ("enter", "open"),
                ("d", "diff"),
                ("esc", "back"),
                ("ctrl-p", "commands"),
            ],
            palette,
        );
    }

    pub(super) fn should_render_diff_placeholder(&self) -> bool {
        self.document.files.is_empty()
            && (matches!(self.diff_source, DiffSource::PullRequest { .. })
                || matches!(self.diff_source, DiffSource::Commit { .. })
                || self.query_client.get(&QueryKey::LocalDiff).is_fetching())
    }

    pub(super) fn github_summary(&self) -> String {
        if !self.github_auth.can_load_github() {
            return self.github_auth.summary().to_string();
        }
        let queue = self.query_client.get(&QueryKey::GitHubQueue);
        if queue.is_initial_loading() {
            return "loading GitHub PRs…".to_string();
        }
        if queue.is_refetching() {
            return queue
                .updated_at
                .map(|updated_at| format!("refreshing · updated {}", relative_unix_age(updated_at)))
                .unwrap_or_else(|| "loading GitHub PRs…".to_string());
        }
        match queue.status {
            QueryStatus::Error => self.github.summary().to_string(),
            QueryStatus::Success => self
                .github
                .cached_at
                .map(|updated_at| format!("updated {}", relative_unix_age(updated_at)))
                .unwrap_or_else(|| self.github.summary().to_string()),
            QueryStatus::Pending => self.github.summary().to_string(),
        }
    }

    pub(super) fn github_notice(&self) -> Option<String> {
        if !self.github_auth.can_load_github() {
            return Some(self.github_auth.notice().to_string());
        }
        self.github.notice()
    }

    pub(super) fn query_status_label(&self, query: QueryResult, _key: &QueryKey) -> String {
        if query.is_initial_loading() {
            return "loading…".to_string();
        }
        if query.is_refetching() {
            return query
                .updated_at
                .map(|updated_at| format!("cached {}", relative_unix_age(updated_at)))
                .unwrap_or_else(|| "loading…".to_string());
        }
        query.label()
    }

    pub(super) fn spinner(&self) -> &'static str {
        const FRAMES: [&str; 8] = ["⠆", "⠖", "⠲", "⠴", "⠰", "⠤", "⠆", "⠒"];
        FRAMES[(self.draw_count as usize / 2) % FRAMES.len()]
    }

    pub(super) fn render_diff_placeholder(&self, frame: &mut Frame) {
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        let [header, divider, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        self.render_quiver_header(frame, header, palette, "diff");
        draw_horizontal_rule(
            frame.buffer_mut(),
            divider.y,
            divider.x,
            divider.right(),
            palette.rule,
            palette.bg,
        );

        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let accent = Style::new()
            .fg(palette.accent)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let (route, query) = match &self.diff_source {
            DiffSource::PullRequest { repository, number } => {
                let key = QueryKey::pull_request_diff(repository, *number);
                (
                    format!("{repository} #{number}"),
                    self.query_client.get(&key),
                )
            }
            DiffSource::LocalWorktree(route) => (
                format!("{} · {}", short_path(&route.repo_path), route.branch),
                self.query_client.get(&QueryKey::LocalDiff),
            ),
            DiffSource::Commit { repo_path, sha } => (
                format!("{} · {}", short_path(repo_path), &sha[..sha.len().min(7)]),
                QueryResult {
                    status: QueryStatus::Pending,
                    fetch_status: crate::server_query::FetchStatus::Fetching,
                    updated_at: None,
                    error: None,
                },
            ),
        };
        let status = if query.is_fetching() {
            "fetching latest changes"
        } else if query.status == QueryStatus::Error {
            "unable to load diff"
        } else {
            "waiting for diff"
        };
        let center_y = body.y + body.height.saturating_div(2).saturating_sub(2);
        let spinner = self.spinner();
        let headline = "preparing review";
        let headline_width = spinner.chars().count() + 1 + headline.chars().count();
        frame.render_widget(
            Line::from(vec![
                Span::styled(spinner, accent),
                Span::styled(" ", title),
                Span::styled(headline, title),
            ]),
            centered_line_rect(body, center_y, headline_width),
        );
        let route = truncate_middle(&route, body.width.saturating_sub(8) as usize);
        frame.render_widget(
            Line::from(Span::styled(route.clone(), muted)),
            centered_line_rect(body, center_y.saturating_add(2), route.chars().count()),
        );
        let status = truncate(status, body.width.saturating_sub(8) as usize).to_string();
        if !status.is_empty() {
            frame.render_widget(
                Line::from(Span::styled(status.clone(), muted)),
                centered_line_rect(body, center_y.saturating_add(3), status.chars().count()),
            );
        }
        self.render_surface_footer(
            frame,
            footer,
            &[("esc", "back"), ("ctrl-p", "commands")],
            palette,
        );
    }

    pub(super) fn render_quiver_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        palette: HomePalette,
        _section: &str,
    ) {
        let viewer = self.github.viewer.as_deref().unwrap_or("local");
        let scope = self.scope_label();
        AppHeader {
            brand: "QUIVER",
            scope: &scope,
            viewer,
            summary: &self.github_summary(),
            is_fetching: self.query_client.is_fetching(),
            spinner: self.spinner(),
            palette,
        }
        .render(frame, area);
    }

    pub(super) fn render_surface_footer(
        &self,
        frame: &mut Frame,
        area: Rect,
        pairs: &[(&str, &str)],
        palette: HomePalette,
    ) {
        let key = palette.text(TextRole::Key);
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let mut spans = Vec::new();
        for (shortcut, label) in pairs {
            spans.push(Span::styled(format!(" {shortcut}"), key));
            spans.push(Span::styled(format!(" {label} "), muted));
        }
        frame.render_widget(Line::from(spans), area);
    }

    fn detail_markdown_line(&self, mut line: Line<'static>, bg: Color) -> Line<'static> {
        if let Some(first) = line.spans.first_mut() {
            if let Some(stripped) = first.content.strip_prefix(' ') {
                first.content = stripped.to_string().into();
            }
        }
        if line
            .spans
            .first()
            .is_some_and(|span| span.content.is_empty())
        {
            line.spans.remove(0);
        }
        line.style = line.style.bg(bg);
        for span in &mut line.spans {
            span.style = span.style.bg(bg);
        }
        line
    }

    fn render_detail_tabs(&self, frame: &mut Frame, area: Rect, palette: HomePalette) {
        if area.width == 0 {
            return;
        }
        let bg = palette.layer_bg(SurfaceLayer::Surface);
        let active = palette
            .text(TextRole::Heading)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let inactive = palette.text(TextRole::Muted).bg(bg);
        let key = palette.text(TextRole::Key).bg(bg);
        let semantic = if self.detail_tab == DetailTab::Semantic {
            active
        } else {
            inactive
        };
        let description = if self.detail_tab == DetailTab::Description {
            active
        } else {
            inactive
        };
        frame.render_widget(
            Line::from(vec![
                Span::styled("1 ", key),
                Span::styled("Semantic", semantic),
                Span::styled("  ", inactive),
                Span::styled("2 ", key),
                Span::styled("Description", description),
                Span::styled(
                    right_aligned_text(area.width, 25, "←/→ switch · [ fold · ] unfold"),
                    inactive,
                ),
            ]),
            area,
        );
    }

    fn render_detail_description(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        selected: &WorkItem,
        palette: HomePalette,
    ) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let bg = palette.layer_bg(SurfaceLayer::Surface);
        let text = palette.text(TextRole::Body).bg(bg);
        let visible_rows = area.height as usize;
        if let Some((repository, number, body)) = selected
            .pull_request(self)
            .map(|pr| (pr.repository.clone(), pr.number, pr.body.clone()))
        {
            let preview_width = area.width.saturating_sub(1).max(16);
            let lines = self.cached_pull_request_body_preview(
                &repository,
                number,
                &body,
                preview_width,
                DETAIL_DESCRIPTION_ROW_LIMIT,
                &palette,
                true,
            );
            let Some(lines) = lines else { return };
            let total_rows = lines.len();
            let scroll_y = self
                .surface_scroll_y
                .min(total_rows.saturating_sub(visible_rows));
            self.surface_scroll_y = scroll_y;
            for (index, line) in lines
                .into_iter()
                .skip(scroll_y)
                .take(visible_rows)
                .enumerate()
            {
                frame.render_widget(
                    self.detail_markdown_line(line, bg),
                    Rect::new(area.x, area.y + index as u16, preview_width, 1),
                );
            }
            render_modal_diff_scrollbar(
                frame.buffer_mut(),
                area,
                total_rows,
                visible_rows,
                scroll_y,
            );
        } else {
            let lines = selected.description(self);
            let total_rows = lines.len();
            let scroll_y = self
                .surface_scroll_y
                .min(total_rows.saturating_sub(visible_rows));
            self.surface_scroll_y = scroll_y;
            for (index, line) in lines.iter().skip(scroll_y).take(visible_rows).enumerate() {
                frame.render_widget(
                    Line::from(vec![Span::styled(
                        truncate(line, area.width.saturating_sub(1) as usize),
                        text,
                    )]),
                    Rect::new(area.x, area.y + index as u16, area.width, 1),
                );
            }
            render_modal_diff_scrollbar(
                frame.buffer_mut(),
                area,
                total_rows,
                visible_rows,
                scroll_y,
            );
        }
    }

    pub(super) fn render_semantic_tree(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        route: &DiffSource,
        palette: HomePalette,
    ) -> u16 {
        if area.height == 0 || area.width == 0 {
            return area.y;
        }
        self.seed_semantic_expansion(route);
        let rows = self.semantic_tree_rows(route);
        if rows.is_empty() {
            self.set_semantic_viewport(SemanticViewport {
                total_rows: 0,
                visible_rows: 1,
                selected: 0,
                scroll_y: 0,
            });
            return area.y;
        }
        let total_rows = rows.len();
        let body_area = semantic_tree_body_area(area);
        let viewport = self.semantic_viewport_for(total_rows, body_area.height as usize);
        self.set_semantic_viewport(viewport);
        let bg = palette.layer_bg(SurfaceLayer::Surface);
        let heading = palette.text(TextRole::Heading).bg(bg);
        let muted = palette.text(TextRole::Muted).bg(bg);
        let text = palette.text(TextRole::Body).bg(bg);
        let key = palette.text(TextRole::Key).bg(bg);
        let add = Style::new().fg(palette.success).bg(bg);
        let del = Style::new().fg(palette.danger).bg(bg);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        let y = area.y;
        frame.render_widget(
            Line::from(vec![
                Span::styled("Changes", heading),
                Span::styled(
                    right_aligned_text(
                        area.width,
                        "Changes".chars().count() + 1,
                        "enter/click opens file",
                    ),
                    muted,
                ),
            ]),
            Rect::new(area.x, y, area.width, 1),
        );
        let items: Vec<ListItem> = rows
            .into_iter()
            .map(|row| {
                let line = match row {
                    SemanticTreeRow::Directory {
                        name,
                        depth,
                        collapsed,
                        ..
                    } => {
                        let indent = "  ".repeat(depth);
                        Line::from(vec![
                            Span::styled(indent, muted),
                            Span::styled(if collapsed { "▸ " } else { "▾ " }, key),
                            Span::styled(
                                truncate(&name, area.width.saturating_sub(6) as usize),
                                text.add_modifier(Modifier::BOLD),
                            ),
                        ])
                    }
                    SemanticTreeRow::File {
                        name,
                        depth,
                        change_count,
                        collapsed,
                        ..
                    } => {
                        let indent = "  ".repeat(depth);
                        Line::from(vec![
                            Span::styled(indent, muted),
                            Span::styled(if collapsed { "▸ " } else { "▾ " }, key),
                            Span::styled(
                                truncate(&name, area.width.saturating_sub(18) as usize),
                                text.add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                right_aligned_text(
                                    area.width,
                                    depth.saturating_mul(2)
                                        + name.chars().count().min(area.width as usize),
                                    &format!("{change_count}"),
                                ),
                                muted,
                            ),
                        ])
                    }
                    SemanticTreeRow::Entity {
                        depth,
                        entity_type,
                        entity_name,
                        change_type,
                        line,
                        ..
                    } => {
                        let marker = semantic_change_marker(&change_type);
                        let marker_style = match marker {
                            "+" => add,
                            "-" => del,
                            _ => muted,
                        };
                        let prefix = format!(
                            "{}{} {:<10} ",
                            "  ".repeat(depth),
                            marker,
                            entity_type.to_ascii_uppercase()
                        );
                        Line::from(vec![
                            Span::styled(prefix, marker_style),
                            Span::styled(
                                truncate(&entity_name, area.width.saturating_sub(16) as usize),
                                text,
                            ),
                            Span::styled(
                                line.map(|line| format!(" :{line}")).unwrap_or_default(),
                                muted,
                            ),
                        ])
                    }
                    SemanticTreeRow::Status(status) => Line::from(vec![
                        Span::styled("  ", muted),
                        Span::styled(
                            truncate(&status, area.width.saturating_sub(2) as usize),
                            muted,
                        ),
                    ]),
                };
                ListItem::new(line).style(Style::new().bg(bg))
            })
            .collect();
        let mut list_state = ListState::default()
            .with_offset(viewport.scroll_y)
            .with_selected(Some(viewport.selected));
        let list = List::new(items).style(Style::new().bg(bg)).highlight_style(
            Style::new()
                .bg(palette.layer_bg(SurfaceLayer::ElevatedSurface))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_stateful_widget(
            list,
            Rect::new(
                body_area.x,
                body_area.y,
                body_area.width.saturating_sub(1),
                body_area.height,
            ),
            &mut list_state,
        );
        render_modal_diff_scrollbar(
            frame.buffer_mut(),
            body_area,
            total_rows,
            viewport.visible_rows,
            viewport.scroll_y,
        );
        body_area.bottom()
    }

    pub(super) fn render_queue_group_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        label: &str,
        palette: HomePalette,
    ) {
        let bg = palette.layer_bg(SurfaceLayer::Surface);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        let bullet = Style::new().fg(palette.dim).bg(bg);
        let text = Style::new()
            .fg(palette.muted)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" ", text),
                Span::styled("◆ ", bullet),
                Span::styled(truncate(label, area.width.saturating_sub(3) as usize), text),
            ]),
            area,
        );
    }

    pub(super) fn render_quiver_work_item(
        &self,
        frame: &mut Frame,
        area: Rect,
        item: &WorkItem,
        selected: bool,
        palette: HomePalette,
    ) {
        let bg = if selected {
            palette.selected_bg
        } else {
            palette.layer_bg(SurfaceLayer::Surface)
        };
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));

        let title_fg = if selected {
            palette.selected_text
        } else {
            palette.fg
        };
        let mut title = Style::new().fg(title_fg).bg(bg);
        if selected {
            title = title.add_modifier(Modifier::BOLD);
        }
        // PR numbers / ids are strong but not action-colored; keep amber
        // reserved for actual risk/action state.
        let number = Style::new()
            .fg(palette.fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let muted = Style::new().fg(palette.muted).bg(bg);

        // Kind colors stay subtle; checks/review state carry meaning.
        let glyph_style = match item.kind {
            WorkItemKind::LocalAgentBranch => Style::new().fg(palette.fg).bg(bg),
            WorkItemKind::RequestedPrReview => Style::new().fg(palette.fg).bg(bg),
            WorkItemKind::OwnedPrFeedback => Style::new().fg(palette.orange).bg(bg),
            WorkItemKind::Update => Style::new().fg(palette.danger).bg(bg),
        };
        let glyph = item.pull_request(self).map_or_else(
            || item.kind.glyph(),
            |pull_request| pull_request.review_status.glyph(),
        );
        let status = item.status_symbol(self);
        let status_style = match status {
            "✓" => Style::new().fg(palette.success).bg(bg),
            "×" => Style::new().fg(palette.danger).bg(bg),
            _ => muted.add_modifier(Modifier::DIM),
        };
        // Push metadata visually back with DIM, like Amp's faded "Ran tool"
        // subtitles and bottom-border cwd label.
        let metadata = muted.add_modifier(Modifier::DIM);
        let id_str = format!("#{}", item.id);
        let suffix_label = if selected && item.pr_index.is_none() {
            self.branch_operation_status
                .as_deref()
                .or(item.branch_status.as_deref())
                .unwrap_or(&item.age)
        } else {
            &item.age
        };
        let suffix = format!(" · {suffix_label} ");

        // M4 cluster child: indent under parent worktree with `└─ `.
        let (indent_text, indent_width) = if item.child { ("└─ ", 3) } else { ("", 0) };

        // Row geometry:
        //   1 gutter + indent + 1 glyph + 1 space + id + 1 space + title +
        //   pad + suffix + 1 status
        let prefix_width = 4 + indent_width + id_str.chars().count();
        let suffix_width = suffix.chars().count() + 1; // +1 for status glyph
        let title_width = (area.width as usize)
            .saturating_sub(prefix_width)
            .saturating_sub(suffix_width);
        let truncated_title = truncate(&item.title, title_width);
        let pad = title_width.saturating_sub(truncated_title.chars().count());

        let mut spans = vec![Span::styled(" ", title)];
        if item.child {
            // Quiet relation glyph in muted color so the eye reads it as
            // structure, not a separate row.
            spans.push(Span::styled(indent_text.to_string(), muted));
        }
        spans.extend(vec![
            Span::styled(glyph.to_string(), glyph_style),
            Span::styled(" ", title),
            Span::styled(id_str, number),
            Span::styled(" ", title),
            Span::styled(truncated_title, title),
            Span::styled(" ".repeat(pad), metadata),
            Span::styled(suffix, metadata),
            Span::styled(status.to_string(), status_style),
        ]);

        frame.render_widget(Line::from(spans), area);
    }

    pub(super) fn render_diff_header(&self, frame: &mut Frame, area: Rect, palette: HomePalette) {
        let summary = match &self.diff_source {
            DiffSource::LocalWorktree(_) => format!(
                "+{} -{} · {} open {} resolved",
                self.document.additions(),
                self.document.deletions(),
                self.session.open_count(),
                self.session.resolved_count()
            ),
            DiffSource::PullRequest { .. } | DiffSource::Commit { .. } => format!(
                "+{} -{} · {} files",
                self.document.additions(),
                self.document.deletions(),
                self.document.files.len()
            ),
        };
        let scope = match &self.diff_source {
            DiffSource::LocalWorktree(_) => format!(
                "inspect {} · attempt {}",
                self.session.branch, self.session.current_attempt.ordinal
            ),
            DiffSource::PullRequest { repository, number } => {
                format!("review {repository}#{number}")
            }
            DiffSource::Commit { sha, .. } => format!("commit {}", &sha[..sha.len().min(7)]),
        };
        let viewer = self.github.viewer.as_deref().unwrap_or("local");
        AppHeader {
            brand: "QUIVER",
            scope: &scope,
            viewer,
            summary: &summary,
            is_fetching: self.query_client.is_fetching(),
            spinner: self.spinner(),
            palette,
        }
        .render(frame, area);
    }

    pub(super) fn render_sticky_file_overlay(&self, frame: &mut Frame, body: Rect) {
        if body.height < 2 {
            return;
        }
        let overlay_width = body.width.saturating_sub(1);
        if overlay_width == 0 {
            return;
        }
        let area = Rect::new(body.x, body.y, overlay_width, 1);
        let divider = Rect::new(body.x, body.y + 1, overlay_width, 1);
        let palette = self.home_palette();
        let bg = palette.bg;
        let style = Style::new().fg(palette.fg).bg(bg);
        let muted = Style::new().fg(palette.muted).bg(bg);
        let add = Style::new().fg(palette.success).bg(bg);
        let del = Style::new().fg(palette.danger).bg(bg);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        fill_rect(frame.buffer_mut(), divider, " ", Style::new().bg(bg));
        let Some(index) = self.current_file_index() else {
            frame.render_widget(Line::from(""), area);
            return;
        };
        let file = &self.document.files[index];
        let additions = file.additions();
        let deletions = file.deletions();
        let mut spans = vec![
            Span::styled(
                format!(" {}/{} ", index + 1, self.document.files.len()),
                muted,
            ),
            Span::styled(
                truncate(&file.new_path, area.width.saturating_sub(24) as usize),
                style,
            ),
            Span::styled("  ", muted),
        ];
        if additions > 0 {
            spans.push(Span::styled(format!("+{additions}"), add));
        }
        if additions > 0 && deletions > 0 {
            spans.push(Span::styled(" ", muted));
        }
        if deletions > 0 {
            spans.push(Span::styled(format!("-{deletions}"), del));
        }
        frame.render_widget(Line::from(spans).style(Style::new().bg(bg)), area);
        frame.render_widget(
            Line::from("─".repeat(divider.width as usize))
                .style(Style::new().fg(palette.rule).bg(palette.bg)),
            divider,
        );
    }

    pub(super) fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let palette = self.home_palette();
        let bg = palette.layer_bg(SurfaceLayer::EditorSurface);
        let key = palette.text(TextRole::Key).bg(bg);
        let label = palette.text(TextRole::Metadata).bg(bg);
        let current = self.current_file_index().map_or(String::new(), |index| {
            format!("  {}/{}", index + 1, self.document.files.len())
        });
        let mode = match self.state.mode {
            DiffMode::Split => "unified",
            DiffMode::Unified => "split",
        };
        let line = if self.file_picker_open {
            Line::from(vec![
                Span::styled(" esc", key),
                Span::styled(" close  ", label),
                Span::styled("↑↓", key),
                Span::styled(" move  ", label),
                Span::styled("enter", key),
                Span::styled(" jump", label),
            ])
        } else {
            Line::from(vec![
                Span::styled(" esc", key),
                Span::styled(" clear  ", label),
                Span::styled("↑↓", key),
                Span::styled(" line  ", label),
                Span::styled("a", key),
                Span::styled(" ask  ", label),
                Span::styled("i", key),
                Span::styled(" instruct  ", label),
                Span::styled("n", key),
                Span::styled(" note  ", label),
                Span::styled("enter", key),
                Span::styled(" discuss  ", label),
                Span::styled(":", key),
                Span::styled(" inbox  ", label),
                Span::styled("/", key),
                Span::styled(" search  ", label),
                Span::styled("f", key),
                Span::styled(" files  ", label),
                Span::styled("v", key),
                Span::styled(" select  ", label),
                Span::styled("m", key),
                Span::styled(format!(" {mode}  "), label),
                Span::styled("A", key),
                Span::styled(" attempts", label),
                Span::styled(current, label),
            ])
        };
        frame.render_widget(line.style(Style::new().bg(bg)), area);
    }

    pub(super) fn render_comment_preview(&self, frame: &mut Frame, area: Rect) {
        let palette = self.home_palette();
        let bg = palette.bg;
        let border = Style::new().fg(palette.rule).bg(bg);
        let text = Style::new().fg(palette.fg).bg(bg);
        let muted = Style::new().fg(palette.muted).bg(bg);
        let key = Style::new()
            .fg(palette.accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        if area.height == 0 {
            return;
        }
        frame.render_widget(
            Line::from("─".repeat(area.width as usize)).style(border),
            Rect::new(area.x, area.y, area.width, 1),
        );
        if area.height < 2 {
            return;
        }
        let line_area = Rect::new(area.x, area.y + 1, area.width, 1);
        if !matches!(self.diff_source, DiffSource::LocalWorktree(_)) {
            frame.render_widget(
                Line::from(vec![
                    Span::styled("  external diff  ", muted),
                    Span::styled("esc", key),
                    Span::styled(" back  ", muted),
                    Span::styled("/", key),
                    Span::styled(" search  ", muted),
                    Span::styled("f", key),
                    Span::styled(" files", muted),
                ]),
                line_area,
            );
            return;
        }
        let Some(target) = self.active_line_target() else {
            frame.render_widget(
                Line::from(vec![
                    Span::styled("  focus a changed line ", muted),
                    Span::styled("j/k", key),
                    Span::styled(" or click gutter/line to comment", muted),
                ]),
                line_area,
            );
            return;
        };
        let notes = self.session.notes_for_target(&target);
        if let Some(note) = notes.last() {
            let (symbol, color) = note.kind.gutter_marker();
            frame.render_widget(
                Line::from(vec![
                    Span::styled(format!("  {symbol} "), Style::new().fg(color).bg(bg)),
                    Span::styled(
                        target_line_label(&target),
                        text.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            " {} · {} item{} · ",
                            note.kind.label(),
                            notes.len(),
                            plural_s(notes.len())
                        ),
                        muted,
                    ),
                    Span::styled("enter thread", key),
                    Span::styled("  ", muted),
                    Span::styled(
                        truncate(&note.summary(), area.width.saturating_sub(42) as usize),
                        text,
                    ),
                ]),
                line_area,
            );
        } else {
            frame.render_widget(
                Line::from(vec![
                    Span::styled("  ", muted),
                    Span::styled("a", key),
                    Span::styled(" ask  ", muted),
                    Span::styled("i", key),
                    Span::styled(" instruct  ", muted),
                    Span::styled("n", key),
                    Span::styled(" note · ", muted),
                    Span::styled(
                        target_line_label(&target),
                        text.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" · ", muted),
                    Span::styled(short_path(&target.path).to_string(), muted),
                ]),
                line_area,
            );
        }
    }

    pub(super) fn render_note_gutter_markers(&self, frame: &mut Frame, body: Rect) {
        if !matches!(self.diff_source, DiffSource::LocalWorktree(_))
            || body.width == 0
            || body.height == 0
            || self.session.notes.is_empty()
        {
            return;
        }

        let rows = row_count_for_mode(&self.document, self.state.mode);
        let viewport_top = self.state.scroll_y.min(rows.saturating_sub(1));
        let viewport_bottom = self
            .state
            .scroll_y
            .saturating_add(self.viewport_height.saturating_sub(1))
            .min(rows.saturating_sub(1));
        let content_width = body.width.saturating_sub(1).max(1);
        let half = content_width / 2;
        let overlay_cutoff = body.y.saturating_add(STICKY_FILE_OVERLAY_ROWS as u16);

        for note in &self.session.notes {
            let Some(start_row) = self.document.line_row(
                self.state.mode,
                note.target.start.file_index,
                note.target.start.hunk_index,
                note.target.start.line_index,
            ) else {
                continue;
            };
            let Some(end_row) = self.document.line_row(
                self.state.mode,
                note.target.end.file_index,
                note.target.end.hunk_index,
                note.target.end.line_index,
            ) else {
                continue;
            };
            let range_start = start_row.min(end_row);
            let range_end = start_row.max(end_row);
            let x = match self.state.mode {
                DiffMode::Unified => body.x,
                DiffMode::Split => match note.target.side() {
                    DiffSide::Left => body.x,
                    DiffSide::Right => body.x.saturating_add(half),
                },
            };
            if x >= body.right().saturating_sub(1) {
                continue;
            }
            let (symbol, color) = note.kind.gutter_marker();
            for row in range_start.max(viewport_top)..=range_end.min(viewport_bottom) {
                let y = body
                    .y
                    .saturating_add(row.saturating_sub(self.state.scroll_y) as u16);
                if y < overlay_cutoff || y >= body.bottom() {
                    continue;
                }
                let marker = if note.target.is_single_line() {
                    symbol
                } else if row == range_start {
                    "╭"
                } else if row == range_end {
                    "╰"
                } else {
                    "│"
                };
                frame.buffer_mut()[(x, y)]
                    .set_symbol(marker)
                    .set_style(Style::new().fg(color).bg(Color::Rgb(61, 54, 32)));
            }
        }
    }

    pub(super) fn render_file_picker(&self, frame: &mut Frame) {
        if self.finder_kind == FinderKind::Inbox {
            self.render_agent_review_inbox(frame);
            return;
        }
        if self.finder_kind == FinderKind::Root {
            self.render_root_command_palette(frame);
            return;
        }
        if self.finder_kind == FinderKind::Text {
            self.render_text_search(frame);
            return;
        }
        let Some((area, list_area, preview_area)) = self.file_picker_areas(frame.area()) else {
            return;
        };
        let palette = self.finder_palette();
        let filtered = self.filtered_file_results();
        self.render_command_palette_shell(
            frame,
            area,
            "File Search",
            &format!("{}/{}", filtered.len(), self.document.files.len()),
            "filter",
            palette,
        );

        if preview_area.is_some() {
            let separator_x = list_area.right();
            for y in list_area.top()..list_area.bottom() {
                frame.buffer_mut()[(separator_x, y)]
                    .set_symbol("│")
                    .set_style(Style::new().fg(palette.border).bg(palette.bg));
            }
        }

        let list_height = list_area.height as usize;
        let start = self.file_picker_list_start(list_height, filtered.len());
        let row_width = list_area.width.saturating_sub(1) as usize;
        for (visual_index, result) in filtered.iter().skip(start).take(list_height).enumerate() {
            let y = list_area.y + visual_index as u16;
            let selected = start + visual_index == self.file_picker_selection;
            let file = &self.document.files[result.index];
            let row = render_finder_row(file, result, row_width, selected, palette);
            frame.render_widget(
                row,
                Rect::new(list_area.x, y, list_area.width.saturating_sub(1), 1),
            );
        }
        if let Some(preview_area) = preview_area {
            if let Some(result) = filtered.get(self.file_picker_selection) {
                self.render_file_preview(frame, preview_area, result.index, palette);
            }
        }
    }

    pub(super) fn render_command_palette_shell(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        count: &str,
        verb: &str,
        palette: FinderPalette,
    ) {
        CommandPalette {
            title,
            count,
            verb,
            query: &self.file_picker_query,
            results: &[],
            selected: self.file_picker_selection,
            palette,
        }
        .render_shell(frame, area);
    }

    pub(super) fn render_file_preview(
        &self,
        frame: &mut Frame,
        area: Rect,
        file_index: usize,
        palette: FinderPalette,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let file = &self.document.files[file_index];
        let theme = crate::design_system::QuiverTheme::for_variant(self.theme_variant).diff_theme();
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        let add = Style::new().fg(palette.add).bg(palette.bg);
        frame.render_widget(
            Line::from(vec![
                Span::styled("preview", muted),
                Span::raw(" "),
                Span::styled(file_stats(file.additions(), file.deletions()), add),
            ]),
            Rect::new(area.x, area.y, area.width, 1),
        );
        frame.render_widget(
            Line::from(Span::styled(
                truncate(&file.new_path, area.width as usize),
                title,
            )),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );

        let mut y = area.y + 3;
        let mut skipped = 0usize;
        let content_height = area.height.saturating_sub(3) as usize;
        let preview_rows = file_preview_row_count(file);
        let target_scroll = self
            .file_picker_preview_scroll
            .min(preview_rows.saturating_sub(content_height));
        let content_width = area.width.saturating_sub(1);
        'outer: for hunk in &file.hunks {
            if skipped < target_scroll {
                skipped += 1;
            } else {
                if y >= area.bottom() {
                    break 'outer;
                }
                frame.render_widget(
                    Line::from(Span::styled(
                        truncate(&hunk.header, area.width as usize),
                        Style::new().fg(theme.muted).bg(palette.bg),
                    )),
                    Rect::new(area.x, y, content_width, 1),
                );
                y += 1;
            }
            for line in &hunk.lines {
                if skipped < target_scroll {
                    skipped += 1;
                    continue;
                }
                if y >= area.bottom() {
                    break 'outer;
                }
                frame.render_widget(
                    preview_unified_diff_line(line, content_width as usize, theme, palette),
                    Rect::new(area.x, y, content_width, 1),
                );
                y += 1;
            }
        }
        render_modal_diff_scrollbar(
            frame.buffer_mut(),
            area,
            preview_rows,
            content_height,
            target_scroll,
        );
    }

    pub(super) fn render_text_search(&self, frame: &mut Frame) {
        let Some((area, list_area, preview_area)) = self.file_picker_areas(frame.area()) else {
            return;
        };
        let palette = self.finder_palette();
        let results = self.filtered_text_results();
        self.render_command_palette_shell(
            frame,
            area,
            "Diff Search",
            &format!("{} matches", results.len()),
            "search",
            palette,
        );
        if area.width < 4 || area.height < 7 {
            return;
        }

        if preview_area.is_some() {
            let separator_x = list_area.right();
            for y in list_area.top()..list_area.bottom() {
                frame.buffer_mut()[(separator_x, y)]
                    .set_symbol("│")
                    .set_style(Style::new().fg(palette.border).bg(palette.bg));
            }
        }

        let list_height = list_area.height as usize;
        let rows = text_search_rows(&results);
        let selected_row = text_search_selected_row(&rows, self.file_picker_selection).unwrap_or(0);
        let start = text_search_list_start(list_height, rows.len(), selected_row);
        let row_width = list_area.width.saturating_sub(1) as usize;
        for (visual_index, row) in rows.iter().skip(start).take(list_height).enumerate() {
            let y = list_area.y + visual_index as u16;
            let line = match *row {
                TextSearchRow::FileHeader { file_index } => render_text_search_file_header(
                    &self.document.files[file_index],
                    row_width,
                    palette,
                ),
                TextSearchRow::Match { result_index } => {
                    let result = &results[result_index];
                    let selected = result_index == self.file_picker_selection;
                    let line = &self.document.files[result.file_index].hunks[result.hunk_index]
                        .lines[result.line_index];
                    render_text_search_row(
                        result,
                        line,
                        &self.file_picker_query,
                        row_width,
                        selected,
                        palette,
                    )
                }
            };
            frame.render_widget(
                line,
                Rect::new(list_area.x, y, list_area.width.saturating_sub(1), 1),
            );
        }
        if let Some(preview_area) = preview_area {
            if let Some(result) = results.get(self.file_picker_selection) {
                self.render_text_search_preview(frame, preview_area, result, palette);
            }
        }
    }

    pub(super) fn render_root_command_palette(&self, frame: &mut Frame) {
        let area = centered_rect(frame.area(), 82, 22);
        let palette = self.finder_palette();
        let results = self.filtered_command_results();
        CommandPalette {
            title: "Command Palette",
            count: "",
            verb: "filter",
            query: &self.file_picker_query,
            results: &results,
            selected: self.file_picker_selection,
            palette,
        }
        .render(frame, area);
    }

    pub(super) fn render_agent_review_inbox(&self, frame: &mut Frame) {
        let area = centered_rect(frame.area(), 86, 24);
        let palette = self.finder_palette();
        let results = self.filtered_inbox_notes();
        self.render_command_palette_shell(
            frame,
            area,
            "Review items",
            &format!(
                "Attempt {} · {} open",
                self.session.current_attempt.ordinal,
                self.session.open_count()
            ),
            "search questions, instructions, notes",
            palette,
        );
        if area.width < 4 || area.height < 7 {
            return;
        }
        let list_area = Rect::new(
            area.x + 2,
            area.y + 4,
            area.width.saturating_sub(4),
            area.height.saturating_sub(6),
        );
        let start = self.file_picker_list_start(list_area.height as usize, results.len());
        let mut previous_bucket = "";
        let mut y = list_area.y;
        for (visual_index, note) in results.iter().skip(start).enumerate() {
            if y >= list_area.bottom() {
                break;
            }
            let bucket = note.state.bucket_label();
            if bucket != previous_bucket {
                frame.render_widget(
                    Line::from(Span::styled(
                        bucket.to_string(),
                        Style::new().fg(palette.muted).bg(palette.bg),
                    )),
                    Rect::new(list_area.x, y, list_area.width, 1),
                );
                y += 1;
                previous_bucket = bucket;
                if y >= list_area.bottom() {
                    break;
                }
            }
            let selected = start + visual_index == self.file_picker_selection;
            frame.render_widget(
                render_inbox_row(note, list_area.width as usize, selected, palette),
                Rect::new(list_area.x, y, list_area.width, 1),
            );
            y += 1;
        }
    }

    pub(super) fn render_text_search_preview(
        &self,
        frame: &mut Frame,
        area: Rect,
        result: &TextSearchResult,
        palette: FinderPalette,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let file = &self.document.files[result.file_index];
        let hunk = &file.hunks[result.hunk_index];
        let theme = crate::design_system::QuiverTheme::for_variant(self.theme_variant).diff_theme();
        let muted = Style::new().fg(palette.muted).bg(palette.bg);
        let title = Style::new()
            .fg(palette.fg)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
        frame.render_widget(
            Line::from(vec![
                Span::styled("preview", muted),
                Span::raw(" "),
                Span::styled(
                    format!(
                        "{}:{}",
                        short_path(&file.new_path),
                        result.new_line.or(result.old_line).unwrap_or_default()
                    ),
                    title,
                ),
            ]),
            Rect::new(area.x, area.y, area.width, 1),
        );
        frame.render_widget(
            Line::from(Span::styled(
                truncate(&file.new_path, area.width as usize),
                muted,
            )),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
        if area.height < 4 {
            return;
        }

        let content_width = area.width.saturating_sub(1);
        let content_height = area.height.saturating_sub(3) as usize;
        let start = result.line_index.saturating_sub(content_height / 2);
        let mut y = area.y + 3;
        frame.render_widget(
            Line::from(Span::styled(
                truncate(&hunk.header, content_width as usize),
                Style::new().fg(theme.muted).bg(palette.bg),
            )),
            Rect::new(area.x, y.saturating_sub(1), content_width, 1),
        );
        for (line_index, line) in hunk
            .lines
            .iter()
            .enumerate()
            .skip(start)
            .take(content_height)
        {
            if y >= area.bottom() {
                break;
            }
            let selected_line = line_index == result.line_index;
            frame.render_widget(
                preview_unified_search_line(
                    line,
                    content_width as usize,
                    theme,
                    palette,
                    &self.file_picker_query,
                    selected_line,
                ),
                Rect::new(area.x, y, content_width, 1),
            );
            y += 1;
        }
    }
}
