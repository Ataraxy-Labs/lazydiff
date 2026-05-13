use ratatui::style::{Color, Style};

use crate::RowKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffThemeName {
    Graphite,
    Midnight,
    Paper,
    Ember,
}

#[derive(Debug, Clone, Copy)]
pub struct SyntaxTheme {
    pub default: Color,
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
    pub function: Color,
    pub property: Color,
    pub r#type: Color,
    pub punctuation: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct DiffTheme {
    pub name: DiffThemeName,
    pub bg: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub file_header: Color,
    pub hunk: Color,
    pub text: Color,
    pub muted: Color,
    pub line_number_bg: Color,
    pub line_number_fg: Color,
    pub context_content_bg: Color,
    pub add_bg: Color,
    pub del_bg: Color,
    pub add_content_bg: Color,
    pub del_content_bg: Color,
    pub add_gutter_bg: Color,
    pub del_gutter_bg: Color,
    pub add_fg: Color,
    pub del_fg: Color,
    pub selected: Color,
    pub syntax: SyntaxTheme,
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::graphite()
    }
}

impl DiffTheme {
    pub fn named(name: DiffThemeName) -> Self {
        match name {
            DiffThemeName::Graphite => Self::graphite(),
            DiffThemeName::Midnight => Self::midnight(),
            DiffThemeName::Paper => Self::paper(),
            DiffThemeName::Ember => Self::ember(),
        }
    }

    pub fn graphite() -> Self {
        Self {
            name: DiffThemeName::Graphite,
            bg: Color::Rgb(17, 19, 21),
            panel: Color::Rgb(23, 26, 29),
            panel_alt: Color::Rgb(29, 33, 38),
            file_header: Color::Rgb(23, 26, 29),
            hunk: Color::Rgb(29, 33, 38),
            text: Color::Rgb(242, 244, 246),
            muted: Color::Rgb(154, 164, 175),
            line_number_bg: Color::Rgb(20, 24, 27),
            line_number_fg: Color::Rgb(121, 133, 146),
            context_content_bg: Color::Rgb(24, 28, 32),
            add_bg: Color::Rgb(31, 48, 37),
            del_bg: Color::Rgb(55, 37, 38),
            add_content_bg: Color::Rgb(36, 54, 42),
            del_content_bg: Color::Rgb(67, 43, 45),
            add_gutter_bg: Color::Rgb(31, 48, 37),
            del_gutter_bg: Color::Rgb(67, 43, 45),
            add_fg: Color::Rgb(136, 211, 155),
            del_fg: Color::Rgb(240, 160, 160),
            selected: Color::Rgb(59, 67, 75),
            syntax: SyntaxTheme {
                default: Color::Rgb(242, 244, 246),
                keyword: Color::Rgb(196, 208, 218),
                string: Color::Rgb(216, 198, 239),
                comment: Color::Rgb(127, 139, 151),
                number: Color::Rgb(230, 207, 152),
                function: Color::Rgb(223, 230, 237),
                property: Color::Rgb(186, 200, 212),
                r#type: Color::Rgb(211, 217, 226),
                punctuation: Color::Rgb(127, 139, 151),
            },
        }
    }

    pub fn midnight() -> Self {
        Self {
            name: DiffThemeName::Midnight,
            bg: Color::Rgb(8, 17, 31),
            panel: Color::Rgb(14, 27, 46),
            panel_alt: Color::Rgb(19, 36, 58),
            file_header: Color::Rgb(14, 27, 46),
            hunk: Color::Rgb(19, 36, 58),
            text: Color::Rgb(238, 244, 255),
            muted: Color::Rgb(141, 165, 199),
            line_number_bg: Color::Rgb(11, 22, 39),
            line_number_fg: Color::Rgb(86, 115, 154),
            context_content_bg: Color::Rgb(19, 34, 56),
            add_bg: Color::Rgb(16, 42, 31),
            del_bg: Color::Rgb(55, 27, 30),
            add_content_bg: Color::Rgb(16, 42, 31),
            del_content_bg: Color::Rgb(55, 27, 30),
            add_gutter_bg: Color::Rgb(21, 53, 38),
            del_gutter_bg: Color::Rgb(71, 38, 42),
            add_fg: Color::Rgb(105, 214, 154),
            del_fg: Color::Rgb(255, 142, 142),
            selected: Color::Rgb(32, 70, 106),
            syntax: SyntaxTheme {
                default: Color::Rgb(232, 241, 255),
                keyword: Color::Rgb(142, 212, 255),
                string: Color::Rgb(199, 180, 255),
                comment: Color::Rgb(110, 133, 167),
                number: Color::Rgb(255, 216, 131),
                function: Color::Rgb(182, 201, 255),
                property: Color::Rgb(168, 214, 255),
                r#type: Color::Rgb(164, 183, 255),
                punctuation: Color::Rgb(110, 133, 167),
            },
        }
    }

    pub fn paper() -> Self {
        Self {
            name: DiffThemeName::Paper,
            bg: Color::Rgb(244, 239, 230),
            panel: Color::Rgb(255, 250, 243),
            panel_alt: Color::Rgb(248, 241, 231),
            file_header: Color::Rgb(255, 250, 243),
            hunk: Color::Rgb(248, 241, 231),
            text: Color::Rgb(47, 36, 23),
            muted: Color::Rgb(120, 103, 83),
            line_number_bg: Color::Rgb(242, 233, 220),
            line_number_fg: Color::Rgb(155, 131, 103),
            context_content_bg: Color::Rgb(255, 250, 243),
            add_bg: Color::Rgb(234, 248, 236),
            del_bg: Color::Rgb(251, 235, 235),
            add_content_bg: Color::Rgb(234, 248, 236),
            del_content_bg: Color::Rgb(251, 235, 235),
            add_gutter_bg: Color::Rgb(223, 240, 225),
            del_gutter_bg: Color::Rgb(246, 221, 222),
            add_fg: Color::Rgb(63, 141, 88),
            del_fg: Color::Rgb(180, 84, 91),
            selected: Color::Rgb(234, 220, 197),
            syntax: SyntaxTheme {
                default: Color::Rgb(47, 36, 23),
                keyword: Color::Rgb(123, 90, 53),
                string: Color::Rgb(74, 104, 144),
                comment: Color::Rgb(143, 122, 101),
                number: Color::Rgb(159, 108, 31),
                function: Color::Rgb(90, 74, 142),
                property: Color::Rgb(53, 107, 127),
                r#type: Color::Rgb(95, 95, 154),
                punctuation: Color::Rgb(143, 122, 101),
            },
        }
    }

    pub fn ember() -> Self {
        Self {
            name: DiffThemeName::Ember,
            bg: Color::Rgb(20, 11, 8),
            panel: Color::Rgb(34, 18, 13),
            panel_alt: Color::Rgb(44, 23, 16),
            file_header: Color::Rgb(34, 18, 13),
            hunk: Color::Rgb(44, 23, 16),
            text: Color::Rgb(255, 240, 230),
            muted: Color::Rgb(199, 161, 141),
            line_number_bg: Color::Rgb(28, 16, 12),
            line_number_fg: Color::Rgb(154, 115, 95),
            context_content_bg: Color::Rgb(43, 23, 17),
            add_bg: Color::Rgb(33, 67, 44),
            del_bg: Color::Rgb(90, 39, 39),
            add_content_bg: Color::Rgb(33, 67, 44),
            del_content_bg: Color::Rgb(90, 39, 39),
            add_gutter_bg: Color::Rgb(24, 52, 36),
            del_gutter_bg: Color::Rgb(74, 31, 31),
            add_fg: Color::Rgb(131, 217, 157),
            del_fg: Color::Rgb(255, 157, 143),
            selected: Color::Rgb(106, 56, 41),
            syntax: SyntaxTheme {
                default: Color::Rgb(255, 240, 230),
                keyword: Color::Rgb(255, 180, 127),
                string: Color::Rgb(255, 211, 168),
                comment: Color::Rgb(161, 125, 105),
                number: Color::Rgb(255, 208, 143),
                function: Color::Rgb(255, 217, 179),
                property: Color::Rgb(255, 200, 159),
                r#type: Color::Rgb(247, 197, 176),
                punctuation: Color::Rgb(161, 125, 105),
            },
        }
    }
}

pub(crate) fn row_style(kind: RowKind, selected: bool, theme: DiffTheme) -> Style {
    if selected {
        return Style::new().fg(Color::White).bg(theme.selected);
    }
    match kind {
        RowKind::Add => Style::new().fg(theme.text).bg(theme.add_bg),
        RowKind::Delete => Style::new().fg(theme.text).bg(theme.del_bg),
        RowKind::Context => Style::new().fg(theme.text).bg(theme.context_content_bg),
        RowKind::Empty => Style::new().fg(theme.muted).bg(theme.panel_alt),
    }
}

pub(crate) fn gutter_bg(kind: RowKind, theme: DiffTheme) -> Color {
    match kind {
        RowKind::Add => theme.add_gutter_bg,
        RowKind::Delete => theme.del_gutter_bg,
        RowKind::Context | RowKind::Empty => theme.line_number_bg,
    }
}
