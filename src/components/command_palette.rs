use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Clear,
};

use crate::{CommandResult, FinderPalette, draw_box, fill_rect, truncate};

pub(crate) struct CommandPalette<'a> {
    pub(crate) title: &'a str,
    pub(crate) count: &'a str,
    pub(crate) verb: &'a str,
    pub(crate) query: &'a str,
    pub(crate) results: &'a [CommandResult],
    pub(crate) selected: usize,
    pub(crate) palette: FinderPalette,
}

impl CommandPalette<'_> {
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        self.render_shell(frame, area);
        if area.width < 4 || area.height < 7 {
            return;
        }

        let list_area = Rect::new(
            area.x + 2,
            area.y + 4,
            area.width.saturating_sub(4),
            area.height.saturating_sub(6),
        );
        let list_height = list_area.height as usize;
        let start = list_start(list_height, self.results.len(), self.selected);
        for (visual_index, command) in self
            .results
            .iter()
            .skip(start)
            .take(list_height)
            .enumerate()
        {
            let y = list_area.y + visual_index as u16;
            let selected = start + visual_index == self.selected;
            frame.render_widget(
                render_command_row(command, list_area.width as usize, selected, self.palette),
                Rect::new(list_area.x, y, list_area.width, 1),
            );
        }
    }

    pub(crate) fn render_shell(&self, frame: &mut Frame, area: Rect) {
        let bg = Style::new().fg(self.palette.fg).bg(self.palette.bg);
        let border = Style::new().fg(self.palette.border).bg(self.palette.bg);
        let muted = Style::new().fg(self.palette.muted).bg(self.palette.bg);
        frame.render_widget(Clear, area);
        fill_rect(frame.buffer_mut(), area, " ", bg);
        draw_box(frame.buffer_mut(), area, border);
        if area.width < 4 || area.height < 7 {
            return;
        }

        frame.buffer_mut().set_stringn(
            area.x + 2,
            area.y,
            format!(" {} ", self.title),
            area.width.saturating_sub(4) as usize,
            Style::new()
                .fg(self.palette.accent)
                .bg(self.palette.bg)
                .add_modifier(Modifier::BOLD),
        );
        let input = format!("> {}", self.query);
        frame.render_widget(
            Line::from(vec![
                Span::styled(" > ", Style::new().fg(self.palette.fg).bg(self.palette.bg)),
                Span::styled(
                    self.query.to_string(),
                    Style::new().fg(self.palette.fg).bg(self.palette.bg),
                ),
                Span::styled(
                    format!(
                        "{:>width$}",
                        self.count,
                        width =
                            area.width.saturating_sub(input.chars().count() as u16 + 6) as usize
                    ),
                    muted,
                ),
            ]),
            Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1),
        );

        let footer = Line::from(vec![
            Span::styled(
                " type",
                Style::new()
                    .fg(self.palette.key)
                    .bg(self.palette.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}  ", self.verb), muted),
            Span::styled(
                "↑↓",
                Style::new()
                    .fg(self.palette.key)
                    .bg(self.palette.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" move  ", muted),
            Span::styled(
                "wheel",
                Style::new()
                    .fg(self.palette.key)
                    .bg(self.palette.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" scroll  ", muted),
            Span::styled(
                "enter",
                Style::new()
                    .fg(self.palette.key)
                    .bg(self.palette.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" jump  ", muted),
            Span::styled(
                "esc",
                Style::new()
                    .fg(self.palette.key)
                    .bg(self.palette.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" close ", muted),
        ]);
        frame.render_widget(
            footer,
            Rect::new(
                area.x + 2,
                area.bottom().saturating_sub(1),
                area.width.saturating_sub(4),
                1,
            ),
        );
    }
}

fn list_start(list_height: usize, filtered_len: usize, selected: usize) -> usize {
    if list_height == 0 || filtered_len <= list_height {
        return 0;
    }
    selected
        .saturating_sub(list_height / 2)
        .min(filtered_len.saturating_sub(list_height))
}

fn render_command_row(
    command: &CommandResult,
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
    let category_width = 20usize;
    let shortcut_width = 8usize;
    let label_width = width.saturating_sub(category_width + shortcut_width + 4);
    let label = truncate(command.label, label_width);
    let category = format!("{:>category_width$}  ", command.category);
    let gap = width.saturating_sub(
        category.chars().count() + label.chars().count() + command.shortcut.chars().count(),
    );
    Line::from(vec![
        Span::styled(
            category,
            Style::new().fg(muted).bg(bg).add_modifier(if selected {
                Modifier::empty()
            } else {
                Modifier::DIM
            }),
        ),
        Span::styled(
            label,
            Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(gap), Style::new().bg(bg)),
        Span::styled(
            command.shortcut.to_string(),
            Style::new()
                .fg(if selected {
                    palette.selected_fg
                } else {
                    palette.key
                })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .style(Style::new().bg(bg))
}
