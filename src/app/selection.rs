use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenPoint {
    pub(crate) x: u16,
    pub(crate) y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenTextSelection {
    pub(crate) anchor: ScreenPoint,
    pub(crate) focus: ScreenPoint,
}

impl ScreenTextSelection {
    pub(crate) fn new(anchor: ScreenPoint) -> Self {
        Self {
            anchor,
            focus: anchor,
        }
    }

    pub(crate) fn set_focus(&mut self, focus: ScreenPoint) {
        self.focus = focus;
    }

    pub(crate) fn normalized(self) -> (ScreenPoint, ScreenPoint) {
        if (self.anchor.y, self.anchor.x) <= (self.focus.y, self.focus.x) {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

pub(crate) fn is_selectable_text_char(ch: char) -> bool {
    !ch.is_whitespace()
        && !matches!(
            ch,
            '│' | '─'
                | '╭'
                | '╮'
                | '╰'
                | '╯'
                | '├'
                | '┤'
                | '┬'
                | '┴'
                | '┼'
                | '▕'
                | '▏'
                | '▎'
                | '▌'
                | '▐'
                | '▝'
                | '▗'
                | '█'
                | '▀'
                | '▄'
                | '╱'
                | '╲'
        )
}

pub(crate) fn selectable_row_range(line: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start > end {
        return None;
    }
    let chars = line.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return None;
    }
    let end = end.min(chars.len().saturating_sub(1));
    let first = (start..=end).find(|index| is_selectable_text_char(chars[*index]))?;
    let last = (first..=end)
        .rev()
        .find(|index| is_selectable_text_char(chars[*index]))?;
    Some((first, last))
}

pub(crate) fn selected_screen_text(
    lines: &[String],
    selection: ScreenTextSelection,
    bounds: Option<Rect>,
) -> String {
    let (start, end) = selection.normalized();
    let bounds = bounds.unwrap_or_else(|| Rect::new(0, 0, u16::MAX, lines.len() as u16));
    let mut out = Vec::new();
    for y in start.y.max(bounds.y)..=end.y.min(bounds.bottom().saturating_sub(1)) {
        let Some(line) = lines.get(y as usize) else {
            continue;
        };
        let start_x = if y == start.y { start.x } else { bounds.x };
        let end_x = if y == end.y {
            end.x
        } else {
            bounds.right().saturating_sub(1)
        };
        let start_x = start_x.max(bounds.x) as usize;
        let end_x = end_x.min(bounds.right().saturating_sub(1)) as usize;
        let Some((start_x, end_x)) = selectable_row_range(line, start_x, end_x) else {
            continue;
        };
        out.push(
            line.chars()
                .skip(start_x)
                .take(end_x.saturating_sub(start_x).saturating_add(1))
                .collect::<String>()
                .trim_end()
                .to_string(),
        );
    }
    out.join("\n").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_screen_text_extracts_forward_and_backward_ranges() {
        let lines = vec!["alpha beta".to_string(), "gamma delta".to_string()];
        let forward = ScreenTextSelection {
            anchor: ScreenPoint { x: 6, y: 0 },
            focus: ScreenPoint { x: 4, y: 1 },
        };
        let backward = ScreenTextSelection {
            anchor: forward.focus,
            focus: forward.anchor,
        };

        assert_eq!(selected_screen_text(&lines, forward, None), "beta\ngamma");
        assert_eq!(selected_screen_text(&lines, backward, None), "beta\ngamma");
    }

    #[test]
    fn selected_screen_text_stays_inside_pane_bounds() {
        let lines = vec![
            "left pane text      right pane text".to_string(),
            "left two            right two".to_string(),
        ];
        let selection = ScreenTextSelection {
            anchor: ScreenPoint { x: 5, y: 0 },
            focus: ScreenPoint { x: 30, y: 1 },
        };
        let left_pane = Rect::new(0, 0, 18, 2);

        assert_eq!(
            selected_screen_text(&lines, selection, Some(left_pane)),
            "pane text\nleft two"
        );
    }

    #[test]
    fn selected_screen_text_ignores_box_chrome_and_padding() {
        let lines = vec![
            "│   Description text       │".to_string(),
            "│   more text              │".to_string(),
        ];
        let selection = ScreenTextSelection {
            anchor: ScreenPoint { x: 0, y: 0 },
            focus: ScreenPoint { x: 27, y: 1 },
        };

        assert_eq!(
            selected_screen_text(&lines, selection, None),
            "Description text\nmore text"
        );
    }

    #[test]
    fn selected_screen_text_ignores_scrollbar_chrome() {
        let lines = vec![
            "Component alignment:      ▐".to_string(),
            "Guard patterns.          ▝".to_string(),
            "fetchEntityConnections   ▗".to_string(),
        ];
        let selection = ScreenTextSelection {
            anchor: ScreenPoint { x: 0, y: 0 },
            focus: ScreenPoint { x: 25, y: 2 },
        };

        assert_eq!(
            selected_screen_text(&lines, selection, None),
            "Component alignment:\nGuard patterns.\nfetchEntityConnections"
        );
    }
}
