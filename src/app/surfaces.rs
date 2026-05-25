use super::*;

pub(super) const DETAIL_DESCRIPTION_ROW_LIMIT: usize = 2000;

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

fn file_tree_color(path: &str, palette: HomePalette, selected: bool) -> Color {
    if selected {
        return palette.selected_text;
    }
    match path.rsplit('.').next().unwrap_or_default() {
        "rs" => Color::Rgb(250, 179, 135),
        "ts" | "tsx" | "js" | "jsx" => Color::Rgb(137, 180, 250),
        "py" => Color::Rgb(249, 226, 175),
        "go" => Color::Rgb(137, 220, 235),
        "md" | "markdown" => Color::Rgb(166, 227, 161),
        "toml" | "yaml" | "yml" | "json" => Color::Rgb(203, 166, 247),
        "css" | "scss" | "sass" => Color::Rgb(245, 194, 231),
        _ => palette.fg,
    }
}

fn entity_tree_color(entity_type: &str, palette: HomePalette, selected: bool) -> Color {
    if selected {
        return palette.selected_text;
    }
    match entity_type.to_ascii_lowercase().as_str() {
        "function" | "fn" | "method" => Color::Rgb(137, 180, 250),
        "class" | "struct" | "trait" | "interface" | "type" => Color::Rgb(203, 166, 247),
        "module" | "namespace" | "package" => Color::Rgb(249, 226, 175),
        "const" | "constant" | "static" => Color::Rgb(250, 179, 135),
        _ => palette.orange,
    }
}

fn file_status_marker(status: FileDiffKind) -> (&'static str, fn(HomePalette) -> Color) {
    match status {
        FileDiffKind::New => ("A", |palette| palette.success),
        FileDiffKind::Deleted => ("D", |palette| palette.danger),
        FileDiffKind::RenamePure | FileDiffKind::RenameChanged => ("R", |palette| palette.accent),
        FileDiffKind::Change => ("M", |palette| palette.orange),
    }
}

fn compact_entity_type(entity_type: &str) -> &'static str {
    match entity_type.to_ascii_lowercase().as_str() {
        "function" | "method" => "fn",
        "property" => "prop",
        "constant" => "const",
        "module" => "mod",
        "section" => "sec",
        "orphan" => "orphan",
        "class" => "class",
        "struct" => "struct",
        "trait" => "trait",
        "interface" => "iface",
        "type" => "type",
        _ => "sym",
    }
}

fn semantic_map_node_label(row: &SemanticTreeRow) -> String {
    match row {
        SemanticTreeRow::Directory { name, .. } => name.clone(),
        SemanticTreeRow::File {
            name, change_count, ..
        } => format!("{name} · {change_count}"),
        SemanticTreeRow::Entity {
            entity_type,
            entity_name,
            line,
            ..
        } => {
            let line = line.map(|line| format!(" :{line}")).unwrap_or_default();
            format!("{} {entity_name}{line}", compact_entity_type(entity_type))
        }
        SemanticTreeRow::Status(status) => status.clone(),
    }
}

fn semantic_map_node_style(row: &SemanticTreeRow, palette: HomePalette, bg: Color) -> Style {
    match row {
        SemanticTreeRow::Directory { .. } => Style::new().fg(palette.accent).bg(bg),
        SemanticTreeRow::File { path, .. } => Style::new()
            .fg(file_tree_color(path, palette, false))
            .bg(bg),
        SemanticTreeRow::Entity {
            entity_type,
            change_type,
            ..
        } => match semantic_change_marker(change_type) {
            "+" => Style::new().fg(palette.success).bg(bg),
            "-" => Style::new().fg(palette.danger).bg(bg),
            _ => Style::new()
                .fg(entity_tree_color(entity_type, palette, false))
                .bg(bg),
        },
        SemanticTreeRow::Status(_) => palette.text(TextRole::Muted).bg(bg),
    }
}

fn semantic_map_node_symbol(row: &SemanticTreeRow, selected: bool) -> &'static str {
    if selected {
        return "▣";
    }
    match row {
        SemanticTreeRow::Directory { .. } => "▢",
        SemanticTreeRow::File { .. } => "▢",
        SemanticTreeRow::Entity { .. } => "▢",
        SemanticTreeRow::Status(_) => "·",
    }
}

fn semantic_entity_preview_lines(
    document: &DiffDocument,
    path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    width: usize,
    limit: usize,
    palette: HomePalette,
    bg: Color,
) -> Vec<Line<'static>> {
    let Some(start_line) = start_line else {
        return Vec::new();
    };
    let end_line = end_line.unwrap_or(start_line).max(start_line);
    let Some(file) = document.files.iter().find(|file| file.new_path == path) else {
        return Vec::new();
    };
    let render_diff_line = |diff_line: &DiffLine| -> (usize, Line<'static>) {
        let (line_number, marker, text, syntax_spans, inline_spans, row_kind, base_style) =
            match diff_line {
                DiffLine::Context {
                    new_line,
                    text,
                    syntax_spans,
                    ..
                } => (
                    *new_line as usize,
                    " ",
                    text.as_str(),
                    syntax_spans.as_slice(),
                    &[][..],
                    lazydiff_diffs::RowKind::Context,
                    palette.text(TextRole::Body).bg(bg),
                ),
                DiffLine::Add {
                    new_line,
                    text,
                    syntax_spans,
                    inline_spans,
                } => (
                    *new_line as usize,
                    "+",
                    text.as_str(),
                    syntax_spans.as_slice(),
                    inline_spans.as_slice(),
                    lazydiff_diffs::RowKind::Add,
                    Style::new().fg(palette.success).bg(bg),
                ),
                DiffLine::Delete {
                    old_line,
                    text,
                    syntax_spans,
                    inline_spans,
                } => (
                    *old_line as usize,
                    "-",
                    text.as_str(),
                    syntax_spans.as_slice(),
                    inline_spans.as_slice(),
                    lazydiff_diffs::RowKind::Delete,
                    Style::new().fg(palette.danger).bg(bg),
                ),
            };
        let theme = DiffTheme::default();
        let mut spans = vec![
            Span::styled(marker.to_string(), base_style.add_modifier(Modifier::BOLD)),
            Span::styled(" ", base_style),
        ];
        for render_span in lazydiff_diffs::line_render_spans(
            text,
            syntax_spans,
            inline_spans,
            row_kind,
            theme,
            base_style,
        ) {
            let style = if render_span.style.bg.is_some() {
                render_span.style
            } else {
                render_span.style.bg(bg)
            };
            spans.push(Span::styled(render_span.text, style));
        }
        let mut line = Line::from(spans);
        if width > 0 {
            line = Line::from(
                line.spans
                    .into_iter()
                    .scan(0usize, |used, span| {
                        if *used >= width {
                            return None;
                        }
                        let remaining = width.saturating_sub(*used);
                        let text = truncate(&span.content, remaining);
                        *used = used.saturating_add(text.chars().count());
                        Some(Span::styled(text, span.style))
                    })
                    .collect::<Vec<_>>(),
            );
        };
        (line_number, line)
    };

    let mut lines = Vec::new();
    for hunk in &file.hunks {
        for diff_line in &hunk.lines {
            let (line_number, line) = render_diff_line(diff_line);
            if line_number < start_line || line_number > end_line {
                continue;
            }
            lines.push(line);
            if lines.len() >= limit {
                return lines;
            }
        }
    }

    if !lines.is_empty() {
        return lines;
    }

    let Some(nearest_hunk) = file.hunks.iter().min_by_key(|hunk| {
        let hunk_start = hunk.new_start as usize;
        let hunk_end = hunk
            .lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Context { .. } | DiffLine::Add { .. }))
            .count()
            .saturating_sub(1)
            .saturating_add(hunk_start);
        if start_line < hunk_start {
            hunk_start.saturating_sub(start_line)
        } else if start_line > hunk_end {
            start_line.saturating_sub(hunk_end)
        } else {
            0
        }
    }) else {
        return Vec::new();
    };
    for diff_line in &nearest_hunk.lines {
        let (_, line) = render_diff_line(diff_line);
        lines.push(line);
        if lines.len() >= limit {
            return lines;
        }
    }
    lines
}

fn semantic_map_point_visible(area: Rect, x: i32, y: i32) -> bool {
    x >= area.left() as i32
        && x < area.right() as i32
        && y >= area.top() as i32
        && y < area.bottom() as i32
}

fn draw_semantic_map_symbol(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    x: i32,
    y: i32,
    symbol: &str,
    style: Style,
) {
    if !semantic_map_point_visible(area, x, y) {
        return;
    }
    buf[(x as u16, y as u16)]
        .set_symbol(symbol)
        .set_style(style);
}

fn draw_semantic_map_line(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    style: Style,
) {
    if !semantic_map_point_visible(area, from_x, from_y)
        || !semantic_map_point_visible(area, to_x, to_y)
    {
        return;
    }
    if from_x == to_x {
        for y in from_y.min(to_y)..=from_y.max(to_y) {
            draw_semantic_map_symbol(buf, area, from_x, y, "│", style);
        }
        return;
    }
    for y in from_y.min(to_y)..=from_y.max(to_y) {
        draw_semantic_map_symbol(buf, area, from_x, y, "│", style);
    }
    for x in from_x.min(to_x)..=from_x.max(to_x) {
        draw_semantic_map_symbol(buf, area, x, to_y, "─", style);
    }
    draw_semantic_map_symbol(buf, area, from_x, to_y, "└", style);
}

fn centered_line_rect(area: Rect, y: u16, width: usize) -> Rect {
    let width = (width as u16).min(area.width);
    let x = area.x + area.width.saturating_sub(width) / 2;
    Rect::new(x, y, width, 1)
}

impl App {
    pub(super) fn diff_sidebar_layout(&self, body: Rect) -> (Option<Rect>, Option<Rect>, Rect) {
        if !self.review_sidebar_visible || body.width < 96 {
            return (None, None, body);
        }
        let sidebar_width = (body.width / 3).clamp(28, 42);
        let sidebar = Rect::new(body.x, body.y, sidebar_width, body.height);
        let diff_body = Rect::new(
            body.x.saturating_add(sidebar_width),
            body.y,
            body.width.saturating_sub(sidebar_width),
            body.height,
        );
        (Some(sidebar), None, diff_body)
    }

    pub(super) fn render_review_sidebar(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        palette: HomePalette,
    ) {
        if area.width < 8 || area.height == 0 {
            return;
        }
        self.seed_review_sidebar_expansion();
        self.keep_review_sidebar_selection_visible();
        let bg = palette.bg;
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        let border = if self.review_sidebar_focus {
            palette.theme.colors.border_focused
        } else {
            palette.rule
        };
        let border_style = Style::new().fg(border).bg(bg);
        draw_box(frame.buffer_mut(), area, border_style);
        let heading = if self.review_sidebar_focus {
            palette.text(TextRole::Heading)
        } else {
            palette.text(TextRole::Muted)
        };
        let muted = palette.text(TextRole::Muted);
        let key = palette.text(TextRole::Key);
        let viewed = self.viewed_file_count();
        let total = self.document.files.len();
        let title = format!(" Changes {viewed}/{total} ");
        frame.render_widget(
            Line::from(vec![Span::styled(title, heading)]).style(Style::new().bg(bg)),
            Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
        );
        if area.height < 3 {
            return;
        }
        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        let rows = self.review_tree_rows();
        let list_area = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        );
        let start = self
            .review_sidebar_scroll_y
            .min(rows.len().saturating_sub(1));
        for (visual_index, row) in rows
            .iter()
            .skip(start)
            .take(list_area.height as usize)
            .enumerate()
        {
            let y = list_area.y + visual_index as u16;
            let selected =
                self.review_sidebar_focus && start + visual_index == self.review_sidebar_selection;
            let line = self.render_review_tree_row(row, inner.width as usize, selected, palette);
            frame.render_widget(line, Rect::new(inner.x, y, inner.width, 1));
        }
        let footer_y = inner.bottom().saturating_sub(1);
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled("tab", key),
                Span::styled(" focus  ", muted),
                Span::styled("space", key),
                Span::styled(" viewed  ", muted),
                Span::styled("enter", key),
                Span::styled(" open", muted),
            ])
            .style(Style::new().bg(bg)),
            Rect::new(inner.x, footer_y, inner.width, 1),
        );
    }

    fn render_review_tree_row(
        &self,
        row: &ReviewTreeRow,
        width: usize,
        selected: bool,
        palette: HomePalette,
    ) -> Line<'static> {
        let bg = if selected {
            palette.selected_bg
        } else {
            palette.bg
        };
        let muted = if selected {
            palette.selected_text
        } else {
            palette.muted
        };
        let tick = Style::new()
            .fg(if selected {
                palette.selected_text
            } else {
                palette.success
            })
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let muted_style = Style::new().fg(muted).bg(bg);
        let add = Style::new()
            .fg(if selected {
                palette.selected_text
            } else {
                palette.success
            })
            .bg(bg);
        let del = Style::new()
            .fg(if selected {
                palette.selected_text
            } else {
                palette.danger
            })
            .bg(bg);
        let entity_style = Style::new()
            .fg(if selected {
                palette.selected_text
            } else {
                palette.orange
            })
            .bg(bg);
        let mut spans = vec![Span::styled(" ", Style::new().bg(bg))];
        match row {
            ReviewTreeRow::Directory {
                name,
                depth,
                collapsed,
                ..
            } => {
                let indent = " ".repeat(*depth);
                let marker = if *collapsed { "▶" } else { "▼" };
                let label = format!("{indent}  {marker} {name}");
                spans.push(Span::styled(
                    truncate(&label, width.saturating_sub(1)),
                    muted_style,
                ));
            }
            ReviewTreeRow::File {
                path,
                name,
                status,
                depth,
                collapsed,
                semantic_count,
                ..
            } => {
                let checked = if self.is_file_viewed(path) {
                    "✓ "
                } else {
                    "  "
                };
                let marker = if *semantic_count > 0 {
                    if *collapsed { "▶" } else { "▼" }
                } else {
                    " "
                };
                let (status_marker, status_color) = file_status_marker(*status);
                let status_style = Style::new()
                    .fg(if selected {
                        palette.selected_text
                    } else {
                        status_color(palette)
                    })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD);
                let indent = " ".repeat(*depth);
                let suffix = format!(" {status_marker}");
                let label = format!("{indent}{checked}{marker} {name}");
                let label_width = width.saturating_sub(1 + suffix.chars().count());
                spans.push(Span::styled(indent, muted_style));
                spans.push(Span::styled(checked, tick));
                spans.push(Span::styled(format!("{marker} "), muted_style));
                spans.push(Span::styled(
                    truncate(name, label_width.saturating_sub(depth.saturating_add(3))),
                    Style::new()
                        .fg(file_tree_color(path, palette, selected))
                        .bg(bg),
                ));
                spans.push(Span::styled(
                    right_aligned_text(
                        width as u16,
                        label.chars().count().min(label_width) + 1,
                        &suffix,
                    ),
                    status_style,
                ));
            }
            ReviewTreeRow::Entity {
                key,
                depth,
                entity_type,
                entity_name,
                change_type,
                ..
            } => {
                let checked = if self.is_entity_viewed(key) {
                    "✓ "
                } else {
                    "  "
                };
                let marker = semantic_change_marker(change_type);
                let marker_style = match marker {
                    "+" => add,
                    "-" => del,
                    _ => entity_style,
                };
                let indent = " ".repeat(*depth);
                let compact_type = compact_entity_type(entity_type);
                let suffix = format!(" {marker} {compact_type}");
                let label = format!("{indent}{checked}{entity_name}");
                let label_width = width.saturating_sub(1 + suffix.chars().count());
                spans.push(Span::styled(indent, muted_style));
                spans.push(Span::styled(checked, tick));
                spans.push(Span::styled(
                    truncate(
                        entity_name,
                        label_width.saturating_sub(depth.saturating_add(2)),
                    ),
                    Style::new()
                        .fg(entity_tree_color(entity_type, palette, selected))
                        .bg(bg),
                ));
                spans.push(Span::styled(
                    right_aligned_text(
                        width as u16,
                        label.chars().count().min(label_width) + 1,
                        &suffix,
                    ),
                    marker_style,
                ));
            }
        }
        Line::from(spans).style(Style::new().bg(bg))
    }

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

        let list_active = self.commit_focus == CommitPane::List;
        let focus_line = Style::new()
            .fg(palette.theme.colors.border_focused)
            .bg(palette.bg)
            .add_modifier(Modifier::BOLD);
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
            self.commit_focus = CommitPane::List;
            (body, None)
        };
        let active_rule = if list_active {
            list_area
        } else {
            meta_area.unwrap_or(list_area)
        };
        frame.render_widget(
            Line::from("━".repeat(active_rule.width as usize)).style(focus_line),
            Rect::new(active_rule.x, header.y + 1, active_rule.width, 1),
        );
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
                Span::styled(" pane  ", muted),
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
            let [queue, _gap, details] = Layout::horizontal([
                Constraint::Percentage(58),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .areas(body);
            let active = if self.queue_focus == QueuePane::List {
                Rect::new(queue.x, header.y + 1, queue.width.saturating_sub(1), 1)
            } else {
                Rect::new(details.x, header.y + 1, details.width, 1)
            };
            let focus_line = Style::new()
                .fg(palette.theme.colors.border_focused)
                .bg(palette.bg)
                .add_modifier(Modifier::BOLD);
            frame.render_widget(
                Line::from("━".repeat(active.width as usize)).style(focus_line),
                active,
            );
            self.render_home_wide(frame, body, footer, &items, selected, palette);
            return;
        }
        self.queue_focus = QueuePane::List;

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

        let rows = self.grouped_work_item_rows(&items, content, y);
        for row in &rows {
            match row {
                GroupedWorkItemRow::Header { label, geometry } => {
                    self.render_queue_group_header(frame, geometry.area, label, palette);
                }
                GroupedWorkItemRow::Item { index, geometry } => {
                    if let Some(item) = items.get(*index) {
                        self.render_quiver_work_item(
                            frame,
                            geometry.area,
                            item,
                            *index == self.home_selection,
                            palette,
                        );
                    }
                }
            }
        }
        if let Some(last) = rows.last() {
            y = last.area().bottom();
        }

        if let Some(notice) = self.github_notice()
            && y + 1 < content.bottom()
        {
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

        render_home_rule(frame, content, content.bottom().saturating_sub(1), rule);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" /", key),
                Span::styled(" filter  ", muted),
                Span::styled("C", key),
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
        let list_area = Rect::new(
            queue.x,
            queue.y,
            queue.width.saturating_sub(1),
            queue.height,
        );
        let rows = self.grouped_work_item_rows(items, list_area, y);
        for row in &rows {
            match row {
                GroupedWorkItemRow::Header { label, geometry } => {
                    self.render_queue_group_header(frame, geometry.area, label, palette);
                }
                GroupedWorkItemRow::Item { index, geometry } => {
                    if let Some(item) = items.get(*index) {
                        self.render_quiver_work_item(
                            frame,
                            geometry.area,
                            item,
                            *index == self.home_selection,
                            palette,
                        );
                    }
                }
            }
        }
        if let Some(last) = rows.last() {
            y = last.area().bottom();
        }

        if let Some(notice) = self.github_notice()
            && y + 1 < queue.bottom()
        {
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

        self.render_home_detail_pane(frame, details, selected, palette);
        // No vertical divider, no connector glyphs, no bottom rule. Amp-style:
        // panes are separated by whitespace alone, footer carries its own
        // emphasis without a horizontal line above it.
        frame.render_widget(
            Line::from(vec![
                Span::styled(" /", key),
                Span::styled(" filter  ", muted),
                Span::styled("tab", key),
                Span::styled(" pane  ", muted),
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
            DetailTab::Graph => {
                let semantic_route = selected.route(self);
                self.render_semantic_graph(frame, tab_area, &semantic_route, palette);
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
            let row_rect = Rect::new(body.x, y, body.width.saturating_sub(1), 1);

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
        render_scrollbar(
            body,
            frame.buffer_mut(),
            rows.len(),
            height,
            self.surface_scroll_y,
        );
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
        if let Some(first) = line.spans.first_mut()
            && let Some(stripped) = first.content.strip_prefix(' ')
        {
            first.content = stripped.to_string().into();
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
        let graph = if self.detail_tab == DetailTab::Graph {
            active
        } else {
            inactive
        };
        frame.render_widget(
            Line::from(vec![
                Span::styled("Semantic", semantic),
                Span::styled("   ", inactive),
                Span::styled("Description", description),
                Span::styled("   ", inactive),
                Span::styled("Graph", graph),
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
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        draw_box(
            frame.buffer_mut(),
            area,
            Style::new().fg(palette.theme.colors.border).bg(bg),
        );
        let viewed = self.viewed_file_count();
        let total = self.document.files.len();
        let heading = palette.text(TextRole::Heading).bg(bg);
        let muted = palette.text(TextRole::Muted).bg(bg);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Description ", heading),
                Span::styled(
                    right_aligned_text(
                        area.width.saturating_sub(2),
                        " Description ".chars().count(),
                        &format!(
                            "{} viewed {viewed}/{total}",
                            if viewed == total { "✓" } else { " " }
                        ),
                    ),
                    muted,
                ),
            ]),
            Rect::new(
                area.x.saturating_add(1),
                area.y,
                area.width.saturating_sub(2),
                1,
            ),
        );
        let content = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if content.height == 0 || content.width == 0 {
            return;
        }
        let text = palette.text(TextRole::Body).bg(bg);
        let visible_rows = content.height as usize;
        if let Some((repository, number, body)) = selected
            .pull_request(self)
            .map(|pr| (pr.repository.clone(), pr.number, pr.body.clone()))
        {
            let preview_width = content.width.saturating_sub(1).max(16);
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
                    Rect::new(content.x, content.y + index as u16, preview_width, 1),
                );
            }
            render_modal_diff_scrollbar(
                frame.buffer_mut(),
                content,
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
                        truncate(line, content.width.saturating_sub(1) as usize),
                        text,
                    )]),
                    Rect::new(content.x, content.y + index as u16, content.width, 1),
                );
            }
            render_modal_diff_scrollbar(
                frame.buffer_mut(),
                content,
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
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        draw_box(
            frame.buffer_mut(),
            area,
            Style::new().fg(palette.theme.colors.border).bg(bg),
        );
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Semantic ", heading),
                Span::styled(
                    right_aligned_text(
                        area.width.saturating_sub(2),
                        " Semantic ".chars().count(),
                        "↑↓ focus · enter/click opens · space ticks",
                    ),
                    muted,
                ),
            ]),
            Rect::new(
                area.x.saturating_add(1),
                area.y,
                area.width.saturating_sub(2),
                1,
            ),
        );
        for (screen_row, row_index) in (viewport.scroll_y..total_rows)
            .take(body_area.height as usize)
            .enumerate()
        {
            let Some(row) = rows.get(row_index) else {
                continue;
            };
            let selected = row_index == viewport.selected.min(total_rows.saturating_sub(1));
            let line =
                self.render_semantic_tree_row(row, body_area.width as usize, selected, palette, bg);
            frame.render_widget(
                line,
                Rect::new(
                    body_area.x,
                    body_area.y.saturating_add(screen_row as u16),
                    body_area.width,
                    1,
                ),
            );
        }
        render_modal_diff_scrollbar(
            frame.buffer_mut(),
            body_area,
            total_rows,
            viewport.visible_rows,
            viewport.scroll_y,
        );
        body_area.bottom()
    }

    fn render_semantic_tree_row(
        &self,
        row: &SemanticTreeRow,
        width: usize,
        selected: bool,
        palette: HomePalette,
        bg: Color,
    ) -> Line<'static> {
        let row_bg = if selected { palette.selected_bg } else { bg };
        let selected_fg = if selected {
            palette.selected_text
        } else {
            palette.fg
        };
        let muted = if selected {
            palette.selected_text
        } else {
            palette.muted
        };
        let muted_style = Style::new().fg(muted).bg(row_bg);
        let tick = Style::new()
            .fg(if selected {
                palette.selected_text
            } else {
                palette.success
            })
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);
        let mut spans = vec![Span::styled(" ", Style::new().bg(row_bg))];
        match row {
            SemanticTreeRow::Directory {
                name,
                depth,
                collapsed,
                ..
            } => {
                let indent = " ".repeat(*depth);
                let marker = if *collapsed { "▶" } else { "▼" };
                spans.push(Span::styled(indent, muted_style));
                spans.push(Span::styled(format!("  {marker} "), muted_style));
                spans.push(Span::styled(
                    truncate(name, width.saturating_sub(depth.saturating_add(5))),
                    Style::new()
                        .fg(if selected {
                            selected_fg
                        } else {
                            palette.accent
                        })
                        .bg(row_bg),
                ));
            }
            SemanticTreeRow::File {
                path,
                name,
                depth,
                change_count,
                collapsed,
                ..
            } => {
                let checked = if self.is_file_viewed(path) {
                    "✓ "
                } else {
                    "  "
                };
                let marker = if *collapsed { "▶" } else { "▼" };
                let indent = " ".repeat(*depth);
                let suffix = format!(" {change_count} symbols");
                let label_width = width.saturating_sub(1 + suffix.chars().count());
                spans.push(Span::styled(indent.clone(), muted_style));
                spans.push(Span::styled(checked, tick));
                spans.push(Span::styled(format!("{marker} "), muted_style));
                spans.push(Span::styled(
                    truncate(name, label_width.saturating_sub(depth.saturating_add(4))),
                    Style::new()
                        .fg(file_tree_color(path, palette, selected))
                        .bg(row_bg),
                ));
                spans.push(Span::styled(
                    right_aligned_text(
                        width as u16,
                        (indent.chars().count()
                            + checked.chars().count()
                            + 2
                            + name.chars().count())
                        .min(label_width)
                            + 1,
                        &suffix,
                    ),
                    muted_style,
                ));
            }
            SemanticTreeRow::Entity {
                path,
                depth,
                entity_type,
                entity_name,
                change_type,
                line,
                ..
            } => {
                let entity_key = Self::semantic_entity_key_parts(
                    path,
                    entity_type,
                    entity_name,
                    change_type,
                    *line,
                );
                let checked = if self.is_entity_viewed(&entity_key) {
                    "✓ "
                } else {
                    "  "
                };
                let marker = semantic_change_marker(change_type);
                let marker_style = match marker {
                    "+" => Style::new()
                        .fg(if selected {
                            selected_fg
                        } else {
                            palette.success
                        })
                        .bg(row_bg),
                    "-" => Style::new()
                        .fg(if selected {
                            selected_fg
                        } else {
                            palette.danger
                        })
                        .bg(row_bg),
                    _ => Style::new()
                        .fg(if selected {
                            selected_fg
                        } else {
                            palette.orange
                        })
                        .bg(row_bg),
                };
                let indent = " ".repeat(*depth);
                let suffix = format!(" {marker} {}", compact_entity_type(entity_type));
                let label_width = width.saturating_sub(1 + suffix.chars().count());
                spans.push(Span::styled(indent.clone(), muted_style));
                spans.push(Span::styled(checked, tick));
                spans.push(Span::styled(
                    truncate(
                        entity_name,
                        label_width.saturating_sub(depth.saturating_add(2)),
                    ),
                    Style::new()
                        .fg(entity_tree_color(entity_type, palette, selected))
                        .bg(row_bg),
                ));
                spans.push(Span::styled(
                    right_aligned_text(
                        width as u16,
                        (indent.chars().count()
                            + checked.chars().count()
                            + entity_name.chars().count())
                        .min(label_width)
                            + 1,
                        &suffix,
                    ),
                    marker_style,
                ));
            }
            SemanticTreeRow::Status(status) => {
                spans.push(Span::styled(
                    truncate(status, width.saturating_sub(1)),
                    muted_style,
                ));
            }
        }
        Line::from(spans).style(Style::new().bg(row_bg))
    }

    pub(super) fn render_semantic_graph(
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
        let tick = Style::new()
            .fg(palette.success)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        fill_rect(frame.buffer_mut(), area, " ", Style::new().bg(bg));
        let border = palette.theme.colors.border;
        draw_box(frame.buffer_mut(), area, Style::new().fg(border).bg(bg));
        let y = area.y;
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Semantic Map ", heading),
                Span::styled(
                    right_aligned_text(
                        area.width.saturating_sub(2),
                        " Semantic Map ".chars().count(),
                        "↑↓ focus · enter/click opens · space ticks",
                    ),
                    muted,
                ),
            ]),
            Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1),
        );
        let nodes = build_semantic_map_nodes(&rows);
        if nodes.is_empty() {
            if let Some(SemanticTreeRow::Status(status)) = rows.first() {
                frame.render_widget(
                    Line::from(Span::styled(
                        truncate(status, body_area.width.saturating_sub(1) as usize),
                        muted,
                    )),
                    body_area,
                );
            }
            return body_area.bottom();
        }

        let selected_row = viewport.selected.min(total_rows.saturating_sub(1));
        let selected_node_index = nodes
            .iter()
            .position(|node| node.row_index == Some(selected_row))
            .unwrap_or(1.min(nodes.len().saturating_sub(1)));
        let selected_node = &nodes[selected_node_index];
        let positions = semantic_map_screen_positions(
            &nodes,
            body_area,
            self.semantic_map_zoom,
            self.semantic_map_pan_x,
            self.semantic_map_pan_y,
        );
        let connector_style = Style::new().fg(palette.dim).bg(bg);
        let tick_style = tick;

        for (node_index, node) in nodes.iter().enumerate() {
            let Some(parent_index) = node.parent else {
                continue;
            };
            let Some((from_x, from_y)) = positions.get(parent_index).copied() else {
                continue;
            };
            let Some((to_x, to_y)) = positions.get(node_index).copied() else {
                continue;
            };
            draw_semantic_map_line(
                frame.buffer_mut(),
                body_area,
                from_x,
                from_y.saturating_add(1),
                to_x,
                to_y.saturating_sub(1),
                connector_style,
            );
        }

        let root_style = Style::new()
            .fg(palette.accent)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        if let Some((root_x, root_y)) = positions.first().copied() {
            draw_semantic_map_symbol(
                frame.buffer_mut(),
                body_area,
                root_x,
                root_y,
                "□",
                root_style,
            );
        }

        for (node_index, node) in nodes.iter().enumerate().skip(1) {
            let Some(row_index) = node.row_index else {
                continue;
            };
            let Some(row) = rows.get(row_index) else {
                continue;
            };
            let selected = node_index == selected_node_index;
            let Some((x, y)) = positions.get(node_index).copied() else {
                continue;
            };
            let style = if selected {
                Style::new()
                    .fg(palette.selected_text)
                    .bg(palette.layer_bg(SurfaceLayer::ElevatedSurface))
                    .add_modifier(Modifier::BOLD)
            } else {
                semantic_map_node_style(row, palette, bg)
            };
            draw_semantic_map_symbol(
                frame.buffer_mut(),
                body_area,
                x,
                y,
                semantic_map_node_symbol(row, selected),
                style,
            );

            let viewed = match row {
                SemanticTreeRow::File { path, .. } => self.is_file_viewed(path),
                SemanticTreeRow::Entity {
                    path,
                    entity_type,
                    entity_name,
                    change_type,
                    line,
                    ..
                } => {
                    let entity_key = Self::semantic_entity_key_parts(
                        path,
                        entity_type,
                        entity_name,
                        change_type,
                        *line,
                    );
                    self.is_entity_viewed(&entity_key)
                }
                SemanticTreeRow::Directory { .. } | SemanticTreeRow::Status(_) => false,
            };
            if viewed {
                draw_semantic_map_symbol(
                    frame.buffer_mut(),
                    body_area,
                    x.saturating_add(2),
                    y,
                    "✓",
                    tick_style,
                );
            }
        }

        if let Some(row) = rows.get(selected_row) {
            let label = semantic_map_node_label(row);
            let detail = match row {
                SemanticTreeRow::Directory { collapsed, .. }
                | SemanticTreeRow::File { collapsed, .. } => {
                    if *collapsed {
                        "enter expands branch"
                    } else {
                        "enter collapses branch"
                    }
                }
                SemanticTreeRow::Entity { path, .. } => path,
                SemanticTreeRow::Status(_) => "",
            };
            let preview_lines = match row {
                SemanticTreeRow::Entity {
                    path,
                    line,
                    end_line,
                    ..
                } => {
                    let preview_document = self.document_for_route(route);
                    semantic_entity_preview_lines(
                        &preview_document,
                        path,
                        *line,
                        *end_line,
                        60,
                        7,
                        palette,
                        bg,
                    )
                }
                _ => Vec::new(),
            };
            let card_width = body_area
                .width
                .saturating_sub(4)
                .min(if preview_lines.is_empty() { 42 } else { 68 });
            if card_width > 8 && body_area.height > 4 {
                let card_height = (3 + preview_lines.len() as u16)
                    .min(body_area.height.saturating_sub(2))
                    .max(3);
                let (selected_x, selected_y) = positions
                    .get(selected_node_index)
                    .copied()
                    .unwrap_or((selected_node.x, selected_node.y));
                let card_x = if selected_x.saturating_add(card_width as i32 + 4)
                    < body_area.right() as i32
                {
                    selected_x.saturating_add(4)
                } else {
                    selected_x
                        .saturating_sub(card_width as i32)
                        .saturating_sub(4)
                }
                .clamp(
                    body_area.x.saturating_add(1) as i32,
                    body_area
                        .right()
                        .saturating_sub(card_width)
                        .saturating_sub(1) as i32,
                ) as u16;
                let card_y = selected_y.saturating_add(1).clamp(
                    body_area.y.saturating_add(1) as i32,
                    body_area.bottom().saturating_sub(card_height) as i32,
                ) as u16;
                let card = Rect::new(card_x, card_y, card_width, card_height);
                let card_bg = bg;
                fill_rect(frame.buffer_mut(), card, " ", Style::new().bg(card_bg));
                draw_box(
                    frame.buffer_mut(),
                    card,
                    Style::new().fg(palette.accent).bg(card_bg),
                );
                frame.render_widget(
                    Line::from(Span::styled(
                        truncate(&label, card.width.saturating_sub(4) as usize),
                        heading.bg(card_bg),
                    )),
                    Rect::new(
                        card.x.saturating_add(2),
                        card.y,
                        card.width.saturating_sub(4),
                        1,
                    ),
                );
                frame.render_widget(
                    Line::from(Span::styled(
                        truncate(detail, card.width.saturating_sub(4) as usize),
                        muted.bg(card_bg),
                    )),
                    Rect::new(
                        card.x.saturating_add(2),
                        card.y.saturating_add(1),
                        card.width.saturating_sub(4),
                        1,
                    ),
                );
                for (index, preview_line) in preview_lines.into_iter().enumerate() {
                    let y = card.y.saturating_add(2 + index as u16);
                    if y >= card.bottom().saturating_sub(1) {
                        break;
                    }
                    frame.render_widget(
                        preview_line,
                        Rect::new(card.x.saturating_add(2), y, card.width.saturating_sub(4), 1),
                    );
                }
            }
        }
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

    pub(super) fn render_diff_pane_slider(
        &self,
        frame: &mut Frame,
        rule: Rect,
        diff_body: Rect,
        palette: HomePalette,
    ) {
        if rule.height == 0 || diff_body.width < 8 {
            return;
        }
        let bg = palette.bg;
        let active = Style::new()
            .fg(palette.theme.colors.border_focused)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let inactive = Style::new().fg(palette.rule_dim).bg(bg);
        let hint = Style::new().fg(palette.muted).bg(bg);
        if self.diff_buffer.viewer().viewport.mode == DiffMode::Split {
            let half = diff_body.width / 2;
            let left = Rect::new(diff_body.x, rule.y, half, 1);
            let right = Rect::new(diff_body.x + half, rule.y, diff_body.width - half, 1);
            let left_active = self.diff_buffer.viewer().cursor.side == DiffSide::Left;
            frame.render_widget(
                Line::from("━".repeat(left.width as usize)).style(if left_active {
                    active
                } else {
                    inactive
                }),
                left,
            );
            frame.render_widget(
                Line::from("━".repeat(right.width as usize)).style(if left_active {
                    inactive
                } else {
                    active
                }),
                right,
            );
        }
        let scroll_x = self.diff_buffer.viewer().active_horizontal_scroll();
        let label = if scroll_x > 0 {
            format!(" tab pane · H/L scroll · x{} ", scroll_x)
        } else {
            " tab pane · H/L scroll ".to_string()
        };
        let label_width = label.chars().count() as u16;
        if label_width < diff_body.width {
            frame.render_widget(
                Line::from(label).style(hint),
                Rect::new(
                    diff_body.right().saturating_sub(label_width + 1),
                    rule.y,
                    label_width,
                    1,
                ),
            );
        }
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
        let viewed = if self.is_file_viewed(&file.new_path) {
            "✓ viewed"
        } else {
            "  viewed"
        };
        let mut spans = vec![
            Span::styled(
                format!(" {}/{} ", index + 1, self.document.files.len()),
                muted,
            ),
            Span::styled(format!("{viewed}  "), muted),
            Span::styled(
                truncate(&file.new_path, area.width.saturating_sub(34) as usize),
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
        let mode = match self.diff_buffer.viewer().viewport.mode {
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
        } else if self.surface == AppSurface::Diff
            && self.diff_buffer.mode() == DiffBufferMode::Search
        {
            Line::from(vec![
                Span::styled("/", key),
                Span::styled(self.diff_buffer.search_query().to_string(), label),
                Span::styled("  enter", key),
                Span::styled(" accept  ", label),
                Span::styled("esc", key),
                Span::styled(" cancel", label),
            ])
        } else if self.surface == AppSurface::Diff
            && self.diff_buffer.mode() == DiffBufferMode::Command
        {
            Line::from(vec![
                Span::styled(":", key),
                Span::styled(self.diff_buffer.command_line().to_string(), label),
                Span::styled("  enter", key),
                Span::styled(" run  ", label),
                Span::styled("esc", key),
                Span::styled(" cancel", label),
            ])
        } else if let Some(status) = self.branch_operation_status.as_deref() {
            Line::from(vec![
                Span::styled(" notice ", key),
                Span::styled(status, label),
            ])
        } else {
            let mut spans = vec![
                Span::styled(" esc", key),
                Span::styled(" clear  ", label),
                Span::styled("↑↓", key),
                Span::styled(" line  ", label),
                Span::styled("i", key),
                Span::styled(" comment  ", label),
            ];
            if matches!(self.diff_source, DiffSource::LocalWorktree(_)) {
                spans.push(Span::styled("<space>e", key));
                spans.push(Span::styled(" files  ", label));
            }
            spans.extend([
                Span::styled("enter", key),
                Span::styled(" discuss  ", label),
                Span::styled(":", key),
                Span::styled(" command  ", label),
                Span::styled("/", key),
                Span::styled(" search  ", label),
                Span::styled("s", key),
                Span::styled(format!(" {mode}  "), label),
                Span::styled("space", key),
                Span::styled(" viewed  ", label),
                Span::styled("v", key),
                Span::styled(" select  ", label),
                Span::styled("A", key),
                Span::styled(" attempts", label),
                Span::styled(current, label),
            ]);
            Line::from(spans)
        };
        frame.render_widget(line.style(Style::new().bg(bg)), area);
    }

    pub(super) fn render_note_gutter_markers(&self, frame: &mut Frame, body: Rect) {
        if !matches!(self.diff_source, DiffSource::LocalWorktree(_))
            || body.width == 0
            || body.height == 0
            || self.session.notes.is_empty()
        {
            return;
        }

        let mode = self.diff_buffer.viewer().viewport.mode;
        let scroll_y = self.diff_buffer.viewer().viewport.scroll_y;
        let rows = row_count_for_mode(&self.document, mode);
        let viewport_top = scroll_y.min(rows.saturating_sub(1));
        let viewport_bottom = self
            .diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .saturating_add(self.viewport_height.saturating_sub(1))
            .min(rows.saturating_sub(1));
        let content_width = body.width.saturating_sub(1).max(1);
        let half = content_width / 2;

        for row in viewport_top..=viewport_bottom {
            let y = body.y.saturating_add(row.saturating_sub(scroll_y) as u16);
            if y >= body.bottom() {
                continue;
            }

            for side in [DiffSide::Left, DiffSide::Right] {
                let Some(target) = self.document.line_target(mode, row, side) else {
                    continue;
                };
                let x = match mode {
                    DiffMode::Unified => body.x,
                    DiffMode::Split => match side {
                        DiffSide::Left => body.x,
                        DiffSide::Right => body.x.saturating_add(half),
                    },
                };
                if x >= body.right().saturating_sub(1) {
                    continue;
                }

                for note in &self.session.notes {
                    if !note.target.contains(&target) {
                        continue;
                    }
                    let (symbol, color) = note.kind.gutter_marker();
                    let range_start = note.target.start.line.min(note.target.end.line);
                    let range_end = note.target.start.line.max(note.target.end.line);
                    let marker = if note.target.is_single_line() {
                        symbol
                    } else if target.line == range_start {
                        "╭"
                    } else if target.line == range_end {
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
        if self.finder_kind == FinderKind::Themes {
            self.render_theme_picker(frame);
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
        let rows = list_item_rows(
            Rect::new(
                list_area.x,
                list_area.y,
                list_area.width.saturating_sub(1),
                list_area.height,
            ),
            start,
            filtered.len(),
        );
        for geometry in rows {
            let ListRowKind::Item(index) = geometry.kind else {
                continue;
            };
            let Some(result) = filtered.get(index) else {
                continue;
            };
            let selected = index == self.file_picker_selection;
            let file = &self.document.files[result.index];
            let line = render_finder_row(file, result, row_width, selected, palette);
            frame.render_widget(line, geometry.area);
        }
        if let Some(preview_area) = preview_area
            && let Some(result) = filtered.get(self.file_picker_selection)
        {
            self.render_file_preview(frame, preview_area, result.index, palette);
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
        if let Some(preview_area) = preview_area
            && let Some(result) = results.get(self.file_picker_selection)
        {
            self.render_text_search_preview(frame, preview_area, result, palette);
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

    pub(super) fn render_theme_picker(&self, frame: &mut Frame) {
        let area = centered_rect(frame.area(), 82, 22);
        let palette = self.finder_palette();
        let results = self.filtered_theme_results();
        CommandPalette {
            title: "Theme Picker",
            count: self.theme_variant.label(),
            verb: "filter themes",
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
