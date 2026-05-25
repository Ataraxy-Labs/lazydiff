use lazydiff_diffs::{DiffTheme, DiffThemeName, SyntaxTheme};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// Lazydiff's Lumen-compatible theme presets.
///
/// The public names intentionally match Lumen so `LAZYDIFF_THEME` and
/// `LUMEN_THEME` can use the same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ThemeVariant {
    DefaultDark,
    DefaultLight,
    CatppuccinMocha,
    CatppuccinLatte,
    Dracula,
    Nord,
    GruvboxDark,
    GruvboxLight,
    OneDark,
    SolarizedDark,
    SolarizedLight,
}

impl ThemeVariant {
    pub(crate) const fn all() -> &'static [Self] {
        &[
            Self::DefaultDark,
            Self::DefaultLight,
            Self::CatppuccinMocha,
            Self::CatppuccinLatte,
            Self::Dracula,
            Self::Nord,
            Self::GruvboxDark,
            Self::GruvboxLight,
            Self::OneDark,
            Self::SolarizedDark,
            Self::SolarizedLight,
        ]
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DefaultDark => "default-dark",
            Self::DefaultLight => "default-light",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::GruvboxDark => "gruvbox-dark",
            Self::GruvboxLight => "gruvbox-light",
            Self::OneDark => "one-dark",
            Self::SolarizedDark => "solarized-dark",
            Self::SolarizedLight => "solarized-light",
        }
    }

    pub(crate) fn from_label(label: &str) -> Option<Self> {
        match label.to_lowercase().replace('_', "-").as_str() {
            "default-dark" | "dark" | "tokyo" | "warm" | "graphite" => Some(Self::DefaultDark),
            "default-light" | "light" => Some(Self::DefaultLight),
            "catppuccin-mocha" | "mocha" => Some(Self::CatppuccinMocha),
            "catppuccin-latte" | "latte" => Some(Self::CatppuccinLatte),
            "dracula" => Some(Self::Dracula),
            "nord" => Some(Self::Nord),
            "gruvbox-dark" => Some(Self::GruvboxDark),
            "gruvbox-light" => Some(Self::GruvboxLight),
            "one-dark" | "onedark" => Some(Self::OneDark),
            "solarized-dark" => Some(Self::SolarizedDark),
            "solarized-light" => Some(Self::SolarizedLight),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct QuiverTheme {
    pub(crate) variant: ThemeVariant,
    pub(crate) colors: ThemeColors,
    pub(crate) status: StatusColors,
    pub(crate) typography: Typography,
}

#[derive(Clone, Copy)]
struct LumenPreset {
    syntax: SyntaxTheme,
    added_bg: Color,
    added_gutter_bg: Color,
    added_gutter_fg: Color,
    deleted_bg: Color,
    deleted_gutter_bg: Color,
    deleted_gutter_fg: Color,
    context_bg: Color,
    empty_placeholder_fg: Color,
    added_word_bg: Color,
    deleted_word_bg: Color,
    border_focused: Color,
    border_unfocused: Color,
    text_primary: Color,
    text_secondary: Color,
    text_muted: Color,
    line_number: Color,
    bg: Color,
    footer_branch_bg: Color,
    footer_branch_fg: Color,
    status_added: Color,
    status_modified: Color,
    status_deleted: Color,
    selection_bg: Color,
    selection_fg: Color,
    highlight: Color,
}

impl QuiverTheme {
    pub(crate) fn for_variant(variant: ThemeVariant) -> Self {
        Self::from_lumen_preset(variant, lumen_preset(variant))
    }

    fn from_lumen_preset(variant: ThemeVariant, p: LumenPreset) -> Self {
        let transparent = matches!(p.bg, Color::Reset);
        Self {
            variant,
            colors: ThemeColors {
                background: p.bg,
                surface: p.bg,
                editor_surface: p.context_bg,
                elevated_surface: p.selection_bg,
                modal_surface: if transparent {
                    Color::Reset
                } else {
                    p.footer_branch_bg
                },
                text: p.text_primary,
                text_muted: p.text_secondary,
                text_dim: p.text_muted,
                text_selected: p.selection_fg,
                border: p.border_unfocused,
                border_focused: p.border_focused,
                rule_dim: p.border_unfocused,
                accent: p.border_focused,
                count: p.highlight,
                action: p.footer_branch_fg,
                code_fg: p.syntax.r#type,
            },
            status: StatusColors {
                success: p.status_added,
                warning: p.status_modified,
                danger: p.status_deleted,
                modified: p.status_modified,
                added: p.status_added,
                deleted: p.status_deleted,
            },
            typography: Typography,
        }
    }

    pub(crate) fn layer_bg(self, layer: SurfaceLayer) -> Color {
        match layer {
            SurfaceLayer::Background => self.colors.background,
            SurfaceLayer::Surface => self.colors.surface,
            SurfaceLayer::EditorSurface => self.colors.editor_surface,
            SurfaceLayer::ElevatedSurface => self.colors.elevated_surface,
            SurfaceLayer::ModalSurface => self.colors.modal_surface,
        }
    }

    /// Lumen-compatible `DiffTheme` derived from the selected preset. Pierre
    /// token spans remain theme-independent; this covers structural diff colors.
    pub(crate) fn diff_theme(self) -> DiffTheme {
        let c = self.colors;
        let p = lumen_preset(self.variant);
        DiffTheme {
            name: DiffThemeName::Paper,
            bg: c.background,
            panel: c.background,
            panel_alt: c.modal_surface,
            file_header: c.background,
            hunk: c.modal_surface,
            text: c.text,
            muted: c.text_muted,
            line_number_bg: c.background,
            line_number_fg: p.line_number,
            context_content_bg: c.background,
            add_bg: p.added_bg,
            del_bg: p.deleted_bg,
            add_content_bg: p.added_word_bg,
            del_content_bg: p.deleted_word_bg,
            add_gutter_bg: p.added_gutter_bg,
            del_gutter_bg: p.deleted_gutter_bg,
            add_fg: p.added_gutter_fg,
            del_fg: p.deleted_gutter_fg,
            empty_placeholder_fg: p.empty_placeholder_fg,
            selected: p.selection_bg,
            syntax: p.syntax,
        }
    }
}

fn lumen_preset(variant: ThemeVariant) -> LumenPreset {
    match variant {
        ThemeVariant::DefaultDark => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(106, 115, 125),
                keyword: rgb(255, 123, 114),
                string: rgb(165, 214, 255),
                number: rgb(121, 192, 255),
                function: rgb(210, 168, 255),
                property: rgb(121, 192, 255),
                r#type: rgb(255, 203, 107),
                punctuation: rgb(200, 200, 200),
                default: rgb(230, 230, 230),
            },
            added_bg: rgb(35, 50, 40),
            added_gutter_bg: rgb(40, 80, 50),
            added_gutter_fg: rgb(140, 200, 160),
            deleted_bg: rgb(50, 35, 35),
            deleted_gutter_bg: rgb(80, 40, 40),
            deleted_gutter_fg: rgb(200, 140, 140),
            context_bg: rgb(40, 40, 50),
            empty_placeholder_fg: rgb(55, 60, 70),
            added_word_bg: rgb(40, 85, 55),
            deleted_word_bg: rgb(100, 50, 50),
            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,
            text_primary: rgb(230, 230, 230),
            text_secondary: rgb(200, 200, 200),
            text_muted: rgb(140, 140, 160),
            line_number: Color::DarkGray,
            bg: Color::Reset,
            footer_branch_bg: rgb(50, 50, 70),
            footer_branch_fg: rgb(180, 180, 220),
            status_added: Color::Green,
            status_modified: Color::Yellow,
            status_deleted: Color::Red,
            selection_bg: Color::Cyan,
            selection_fg: Color::Black,
            highlight: Color::Yellow,
        },
        ThemeVariant::DefaultLight => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(106, 115, 125),
                keyword: rgb(207, 34, 46),
                string: rgb(10, 48, 105),
                number: rgb(5, 80, 174),
                function: rgb(130, 80, 223),
                property: rgb(5, 80, 174),
                r#type: rgb(149, 56, 0),
                punctuation: rgb(87, 96, 106),
                default: rgb(36, 41, 47),
            },
            added_bg: rgb(230, 255, 237),
            added_gutter_bg: rgb(180, 240, 200),
            added_gutter_fg: rgb(36, 100, 60),
            deleted_bg: rgb(255, 245, 243),
            deleted_gutter_bg: rgb(255, 210, 205),
            deleted_gutter_fg: rgb(140, 60, 60),
            context_bg: rgb(246, 248, 250),
            empty_placeholder_fg: rgb(200, 205, 212),
            added_word_bg: rgb(171, 242, 188),
            deleted_word_bg: rgb(255, 184, 174),
            border_focused: rgb(9, 105, 218),
            border_unfocused: rgb(208, 215, 222),
            text_primary: rgb(36, 41, 47),
            text_secondary: rgb(87, 96, 106),
            text_muted: rgb(140, 149, 159),
            line_number: rgb(140, 149, 159),
            bg: Color::Reset,
            footer_branch_bg: rgb(221, 244, 255),
            footer_branch_fg: rgb(9, 105, 218),
            status_added: rgb(26, 127, 55),
            status_modified: rgb(154, 103, 0),
            status_deleted: rgb(207, 34, 46),
            selection_bg: rgb(9, 105, 218),
            selection_fg: Color::White,
            highlight: rgb(154, 103, 0),
        },
        ThemeVariant::CatppuccinMocha => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(108, 112, 134),
                keyword: rgb(203, 166, 247),
                string: rgb(166, 227, 161),
                number: rgb(250, 179, 135),
                function: rgb(137, 180, 250),
                property: rgb(116, 199, 236),
                r#type: rgb(249, 226, 175),
                punctuation: rgb(166, 173, 200),
                default: rgb(205, 214, 244),
            },
            added_bg: rgb(30, 40, 35),
            added_gutter_bg: rgb(40, 70, 50),
            added_gutter_fg: rgb(166, 227, 161),
            deleted_bg: rgb(45, 30, 35),
            deleted_gutter_bg: rgb(70, 40, 50),
            deleted_gutter_fg: rgb(243, 139, 168),
            context_bg: rgb(30, 30, 46),
            empty_placeholder_fg: rgb(69, 71, 90),
            added_word_bg: rgb(50, 90, 60),
            deleted_word_bg: rgb(100, 50, 60),
            border_focused: rgb(137, 180, 250),
            border_unfocused: rgb(69, 71, 90),
            text_primary: rgb(205, 214, 244),
            text_secondary: rgb(166, 173, 200),
            text_muted: rgb(108, 112, 134),
            line_number: rgb(88, 91, 112),
            bg: rgb(24, 24, 37),
            footer_branch_bg: rgb(49, 50, 68),
            footer_branch_fg: rgb(137, 180, 250),
            status_added: rgb(166, 227, 161),
            status_modified: rgb(249, 226, 175),
            status_deleted: rgb(243, 139, 168),
            selection_bg: rgb(137, 180, 250),
            selection_fg: rgb(30, 30, 46),
            highlight: rgb(249, 226, 175),
        },
        ThemeVariant::CatppuccinLatte => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(140, 143, 161),
                keyword: rgb(136, 57, 239),
                string: rgb(64, 160, 43),
                number: rgb(254, 100, 11),
                function: rgb(30, 102, 245),
                property: rgb(32, 159, 181),
                r#type: rgb(223, 142, 29),
                punctuation: rgb(92, 95, 119),
                default: rgb(76, 79, 105),
            },
            added_bg: rgb(230, 250, 235),
            added_gutter_bg: rgb(190, 235, 200),
            added_gutter_fg: rgb(64, 160, 43),
            deleted_bg: rgb(255, 235, 235),
            deleted_gutter_bg: rgb(250, 200, 200),
            deleted_gutter_fg: rgb(210, 15, 57),
            context_bg: rgb(239, 241, 245),
            empty_placeholder_fg: rgb(188, 192, 204),
            added_word_bg: rgb(160, 230, 180),
            deleted_word_bg: rgb(255, 180, 180),
            border_focused: rgb(30, 102, 245),
            border_unfocused: rgb(188, 192, 204),
            text_primary: rgb(76, 79, 105),
            text_secondary: rgb(92, 95, 119),
            text_muted: rgb(140, 143, 161),
            line_number: rgb(140, 143, 161),
            bg: rgb(230, 233, 239),
            footer_branch_bg: rgb(204, 208, 218),
            footer_branch_fg: rgb(30, 102, 245),
            status_added: rgb(64, 160, 43),
            status_modified: rgb(223, 142, 29),
            status_deleted: rgb(210, 15, 57),
            selection_bg: rgb(30, 102, 245),
            selection_fg: Color::White,
            highlight: rgb(223, 142, 29),
        },
        ThemeVariant::Dracula => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(98, 114, 164),
                keyword: rgb(255, 121, 198),
                string: rgb(241, 250, 140),
                number: rgb(189, 147, 249),
                function: rgb(80, 250, 123),
                property: rgb(248, 248, 242),
                r#type: rgb(139, 233, 253),
                punctuation: rgb(248, 248, 242),
                default: rgb(248, 248, 242),
            },
            added_bg: rgb(35, 50, 40),
            added_gutter_bg: rgb(40, 80, 50),
            added_gutter_fg: rgb(80, 250, 123),
            deleted_bg: rgb(50, 35, 40),
            deleted_gutter_bg: rgb(80, 40, 50),
            deleted_gutter_fg: rgb(255, 85, 85),
            context_bg: rgb(40, 42, 54),
            empty_placeholder_fg: rgb(68, 71, 90),
            added_word_bg: rgb(50, 100, 60),
            deleted_word_bg: rgb(100, 50, 60),
            border_focused: rgb(189, 147, 249),
            border_unfocused: rgb(68, 71, 90),
            text_primary: rgb(248, 248, 242),
            text_secondary: rgb(189, 147, 249),
            text_muted: rgb(98, 114, 164),
            line_number: rgb(98, 114, 164),
            bg: rgb(33, 34, 44),
            footer_branch_bg: rgb(68, 71, 90),
            footer_branch_fg: rgb(189, 147, 249),
            status_added: rgb(80, 250, 123),
            status_modified: rgb(255, 184, 108),
            status_deleted: rgb(255, 85, 85),
            selection_bg: rgb(189, 147, 249),
            selection_fg: rgb(40, 42, 54),
            highlight: rgb(241, 250, 140),
        },
        ThemeVariant::Nord => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(76, 86, 106),
                keyword: rgb(129, 161, 193),
                string: rgb(163, 190, 140),
                number: rgb(180, 142, 173),
                function: rgb(136, 192, 208),
                property: rgb(216, 222, 233),
                r#type: rgb(235, 203, 139),
                punctuation: rgb(216, 222, 233),
                default: rgb(236, 239, 244),
            },
            added_bg: rgb(40, 55, 50),
            added_gutter_bg: rgb(50, 75, 60),
            added_gutter_fg: rgb(163, 190, 140),
            deleted_bg: rgb(55, 45, 50),
            deleted_gutter_bg: rgb(75, 55, 60),
            deleted_gutter_fg: rgb(191, 97, 106),
            context_bg: rgb(46, 52, 64),
            empty_placeholder_fg: rgb(59, 66, 82),
            added_word_bg: rgb(60, 100, 75),
            deleted_word_bg: rgb(110, 65, 70),
            border_focused: rgb(136, 192, 208),
            border_unfocused: rgb(59, 66, 82),
            text_primary: rgb(236, 239, 244),
            text_secondary: rgb(216, 222, 233),
            text_muted: rgb(76, 86, 106),
            line_number: rgb(76, 86, 106),
            bg: rgb(59, 66, 82),
            footer_branch_bg: rgb(67, 76, 94),
            footer_branch_fg: rgb(136, 192, 208),
            status_added: rgb(163, 190, 140),
            status_modified: rgb(235, 203, 139),
            status_deleted: rgb(191, 97, 106),
            selection_bg: rgb(136, 192, 208),
            selection_fg: rgb(46, 52, 64),
            highlight: rgb(235, 203, 139),
        },
        ThemeVariant::GruvboxDark => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(146, 131, 116),
                keyword: rgb(251, 73, 52),
                string: rgb(184, 187, 38),
                number: rgb(211, 134, 155),
                function: rgb(184, 187, 38),
                property: rgb(235, 219, 178),
                r#type: rgb(250, 189, 47),
                punctuation: rgb(235, 219, 178),
                default: rgb(235, 219, 178),
            },
            added_bg: rgb(40, 50, 35),
            added_gutter_bg: rgb(60, 80, 50),
            added_gutter_fg: rgb(184, 187, 38),
            deleted_bg: rgb(55, 35, 35),
            deleted_gutter_bg: rgb(85, 50, 50),
            deleted_gutter_fg: rgb(251, 73, 52),
            context_bg: rgb(40, 40, 40),
            empty_placeholder_fg: rgb(60, 56, 54),
            added_word_bg: rgb(70, 100, 55),
            deleted_word_bg: rgb(115, 55, 50),
            border_focused: rgb(250, 189, 47),
            border_unfocused: rgb(80, 73, 69),
            text_primary: rgb(235, 219, 178),
            text_secondary: rgb(213, 196, 161),
            text_muted: rgb(146, 131, 116),
            line_number: rgb(124, 111, 100),
            bg: rgb(50, 48, 47),
            footer_branch_bg: rgb(80, 73, 69),
            footer_branch_fg: rgb(250, 189, 47),
            status_added: rgb(184, 187, 38),
            status_modified: rgb(250, 189, 47),
            status_deleted: rgb(251, 73, 52),
            selection_bg: rgb(250, 189, 47),
            selection_fg: rgb(40, 40, 40),
            highlight: rgb(250, 189, 47),
        },
        ThemeVariant::GruvboxLight => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(146, 131, 116),
                keyword: rgb(204, 36, 29),
                string: rgb(152, 151, 26),
                number: rgb(177, 98, 134),
                function: rgb(152, 151, 26),
                property: rgb(60, 56, 54),
                r#type: rgb(215, 153, 33),
                punctuation: rgb(60, 56, 54),
                default: rgb(60, 56, 54),
            },
            added_bg: rgb(235, 250, 230),
            added_gutter_bg: rgb(200, 230, 190),
            added_gutter_fg: rgb(152, 151, 26),
            deleted_bg: rgb(255, 240, 235),
            deleted_gutter_bg: rgb(250, 210, 200),
            deleted_gutter_fg: rgb(204, 36, 29),
            context_bg: rgb(251, 241, 199),
            empty_placeholder_fg: rgb(213, 196, 161),
            added_word_bg: rgb(180, 235, 165),
            deleted_word_bg: rgb(255, 195, 180),
            border_focused: rgb(69, 133, 136),
            border_unfocused: rgb(213, 196, 161),
            text_primary: rgb(60, 56, 54),
            text_secondary: rgb(80, 73, 69),
            text_muted: rgb(146, 131, 116),
            line_number: rgb(146, 131, 116),
            bg: rgb(235, 219, 178),
            footer_branch_bg: rgb(213, 196, 161),
            footer_branch_fg: rgb(69, 133, 136),
            status_added: rgb(152, 151, 26),
            status_modified: rgb(215, 153, 33),
            status_deleted: rgb(204, 36, 29),
            selection_bg: rgb(69, 133, 136),
            selection_fg: Color::White,
            highlight: rgb(215, 153, 33),
        },
        ThemeVariant::OneDark => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(92, 99, 112),
                keyword: rgb(198, 120, 221),
                string: rgb(152, 195, 121),
                number: rgb(209, 154, 102),
                function: rgb(97, 175, 239),
                property: rgb(171, 178, 191),
                r#type: rgb(229, 192, 123),
                punctuation: rgb(171, 178, 191),
                default: rgb(171, 178, 191),
            },
            added_bg: rgb(35, 50, 40),
            added_gutter_bg: rgb(50, 80, 55),
            added_gutter_fg: rgb(152, 195, 121),
            deleted_bg: rgb(50, 35, 38),
            deleted_gutter_bg: rgb(80, 50, 55),
            deleted_gutter_fg: rgb(224, 108, 117),
            context_bg: rgb(40, 44, 52),
            empty_placeholder_fg: rgb(62, 68, 81),
            added_word_bg: rgb(55, 100, 65),
            deleted_word_bg: rgb(110, 55, 60),
            border_focused: rgb(97, 175, 239),
            border_unfocused: rgb(62, 68, 81),
            text_primary: rgb(171, 178, 191),
            text_secondary: rgb(152, 159, 172),
            text_muted: rgb(92, 99, 112),
            line_number: rgb(76, 82, 99),
            bg: rgb(33, 37, 43),
            footer_branch_bg: rgb(62, 68, 81),
            footer_branch_fg: rgb(97, 175, 239),
            status_added: rgb(152, 195, 121),
            status_modified: rgb(229, 192, 123),
            status_deleted: rgb(224, 108, 117),
            selection_bg: rgb(97, 175, 239),
            selection_fg: rgb(40, 44, 52),
            highlight: rgb(229, 192, 123),
        },
        ThemeVariant::SolarizedDark => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(88, 110, 117),
                keyword: rgb(133, 153, 0),
                string: rgb(42, 161, 152),
                number: rgb(108, 113, 196),
                function: rgb(38, 139, 210),
                property: rgb(147, 161, 161),
                r#type: rgb(181, 137, 0),
                punctuation: rgb(131, 148, 150),
                default: rgb(131, 148, 150),
            },
            added_bg: rgb(0, 50, 50),
            added_gutter_bg: rgb(0, 70, 60),
            added_gutter_fg: rgb(133, 153, 0),
            deleted_bg: rgb(50, 30, 30),
            deleted_gutter_bg: rgb(70, 40, 40),
            deleted_gutter_fg: rgb(220, 50, 47),
            context_bg: rgb(0, 43, 54),
            empty_placeholder_fg: rgb(7, 54, 66),
            added_word_bg: rgb(20, 85, 75),
            deleted_word_bg: rgb(100, 50, 45),
            border_focused: rgb(38, 139, 210),
            border_unfocused: rgb(88, 110, 117),
            text_primary: rgb(131, 148, 150),
            text_secondary: rgb(147, 161, 161),
            text_muted: rgb(88, 110, 117),
            line_number: rgb(88, 110, 117),
            bg: rgb(7, 54, 66),
            footer_branch_bg: rgb(88, 110, 117),
            footer_branch_fg: rgb(38, 139, 210),
            status_added: rgb(133, 153, 0),
            status_modified: rgb(181, 137, 0),
            status_deleted: rgb(220, 50, 47),
            selection_bg: rgb(38, 139, 210),
            selection_fg: rgb(0, 43, 54),
            highlight: rgb(181, 137, 0),
        },
        ThemeVariant::SolarizedLight => LumenPreset {
            syntax: SyntaxTheme {
                comment: rgb(147, 161, 161),
                keyword: rgb(133, 153, 0),
                string: rgb(42, 161, 152),
                number: rgb(108, 113, 196),
                function: rgb(38, 139, 210),
                property: rgb(88, 110, 117),
                r#type: rgb(181, 137, 0),
                punctuation: rgb(101, 123, 131),
                default: rgb(101, 123, 131),
            },
            added_bg: rgb(230, 250, 235),
            added_gutter_bg: rgb(200, 235, 210),
            added_gutter_fg: rgb(133, 153, 0),
            deleted_bg: rgb(255, 240, 238),
            deleted_gutter_bg: rgb(250, 210, 205),
            deleted_gutter_fg: rgb(220, 50, 47),
            context_bg: rgb(253, 246, 227),
            empty_placeholder_fg: rgb(238, 232, 213),
            added_word_bg: rgb(175, 235, 190),
            deleted_word_bg: rgb(255, 190, 185),
            border_focused: rgb(38, 139, 210),
            border_unfocused: rgb(147, 161, 161),
            text_primary: rgb(101, 123, 131),
            text_secondary: rgb(88, 110, 117),
            text_muted: rgb(147, 161, 161),
            line_number: rgb(147, 161, 161),
            bg: rgb(238, 232, 213),
            footer_branch_bg: rgb(147, 161, 161),
            footer_branch_fg: rgb(38, 139, 210),
            status_added: rgb(133, 153, 0),
            status_modified: rgb(181, 137, 0),
            status_deleted: rgb(220, 50, 47),
            selection_bg: rgb(38, 139, 210),
            selection_fg: Color::White,
            highlight: rgb(181, 137, 0),
        },
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[derive(Clone, Copy)]
pub(crate) struct ThemeColors {
    pub(crate) background: Color,
    pub(crate) surface: Color,
    pub(crate) editor_surface: Color,
    pub(crate) elevated_surface: Color,
    pub(crate) modal_surface: Color,
    pub(crate) text: Color,
    pub(crate) text_muted: Color,
    pub(crate) text_dim: Color,
    pub(crate) text_selected: Color,
    pub(crate) border: Color,
    pub(crate) border_focused: Color,
    pub(crate) rule_dim: Color,
    pub(crate) accent: Color,
    pub(crate) count: Color,
    pub(crate) action: Color,
    pub(crate) code_fg: Color,
}

#[derive(Clone, Copy)]
pub(crate) struct StatusColors {
    pub(crate) success: Color,
    pub(crate) warning: Color,
    pub(crate) danger: Color,
    pub(crate) modified: Color,
    pub(crate) added: Color,
    pub(crate) deleted: Color,
}

/// Semantic z-order for terminal surfaces.
///
/// This mirrors GPUI's elevation concept. In ratatui the layer maps to a
/// background/border treatment rather than shadows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceLayer {
    Background,
    Surface,
    EditorSurface,
    ElevatedSurface,
    ModalSurface,
}

#[derive(Clone, Copy)]
pub(crate) struct Typography;

impl Typography {
    pub(crate) fn style(self, role: TextRole, theme: QuiverTheme, bg: Color) -> Style {
        let colors = theme.colors;
        match role {
            TextRole::Body => Style::new().fg(colors.text).bg(bg),
            TextRole::Muted => Style::new().fg(colors.text_muted).bg(bg),
            TextRole::Metadata => Style::new()
                .fg(colors.text_muted)
                .bg(bg)
                .add_modifier(Modifier::DIM),
            TextRole::_SemanticHook => Style::new()
                .fg(colors.text_muted)
                .bg(bg)
                .add_modifier(Modifier::ITALIC),
            TextRole::Heading => Style::new()
                .fg(colors.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            TextRole::_Brand => Style::new()
                .fg(colors.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            TextRole::Key => Style::new()
                .fg(colors.accent)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            TextRole::_Action => Style::new().fg(colors.action).bg(bg),
            TextRole::Selected => Style::new()
                .fg(colors.text_selected)
                .bg(colors.elevated_surface)
                .add_modifier(Modifier::BOLD),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextRole {
    Body,
    Muted,
    Metadata,
    _SemanticHook,
    Heading,
    _Brand,
    Key,
    _Action,
    Selected,
}

#[derive(Clone, Copy)]
pub(crate) struct HomePalette {
    pub(crate) bg: Color,
    pub(crate) fg: Color,
    pub(crate) muted: Color,
    pub(crate) dim: Color,
    pub(crate) rule: Color,
    pub(crate) rule_dim: Color,
    pub(crate) selected_bg: Color,
    pub(crate) selected_text: Color,
    pub(crate) success: Color,
    pub(crate) _warning: Color,
    pub(crate) danger: Color,
    pub(crate) orange: Color,
    pub(crate) accent: Color,
    pub(crate) _count: Color,
    pub(crate) action: Color,
    pub(crate) code_fg: Color,
    pub(crate) theme: QuiverTheme,
}

impl HomePalette {
    #[allow(dead_code)]
    pub(crate) fn quiver() -> Self {
        Self::for_variant(ThemeVariant::DefaultDark)
    }

    pub(crate) fn for_variant(variant: ThemeVariant) -> Self {
        let theme = QuiverTheme::for_variant(variant);
        Self {
            bg: theme.layer_bg(SurfaceLayer::Background),
            fg: theme.colors.text,
            muted: theme.colors.text_muted,
            dim: theme.colors.text_dim,
            rule: theme.colors.border,
            rule_dim: theme.colors.rule_dim,
            selected_bg: theme.layer_bg(SurfaceLayer::ElevatedSurface),
            selected_text: theme.colors.text_selected,
            success: theme.status.success,
            _warning: theme.status.warning,
            danger: theme.status.danger,
            orange: theme.status.modified,
            accent: theme.colors.accent,
            _count: theme.colors.count,
            action: theme.colors.action,
            code_fg: theme.colors.code_fg,
            theme,
        }
    }

    pub(crate) fn text(self, role: TextRole) -> Style {
        self.theme.typography.style(role, self.theme, self.bg)
    }

    pub(crate) fn layer_bg(self, layer: SurfaceLayer) -> Color {
        self.theme.layer_bg(layer)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FinderPalette {
    pub(crate) bg: Color,
    pub(crate) fg: Color,
    pub(crate) muted: Color,
    pub(crate) border: Color,
    pub(crate) accent: Color,
    pub(crate) key: Color,
    pub(crate) add: Color,
    pub(crate) del: Color,
    pub(crate) selected_bg: Color,
    pub(crate) selected_fg: Color,
    pub(crate) selected_muted: Color,
    pub(crate) variant: ThemeVariant,
}

impl FinderPalette {
    pub(crate) fn for_variant(variant: ThemeVariant) -> Self {
        let theme = QuiverTheme::for_variant(variant);
        Self {
            bg: theme.layer_bg(SurfaceLayer::ModalSurface),
            fg: theme.colors.text,
            muted: theme.colors.text_muted,
            border: theme.colors.border,
            accent: theme.colors.action,
            key: theme.colors.text,
            add: theme.status.added,
            del: theme.status.deleted,
            selected_bg: theme.layer_bg(SurfaceLayer::ElevatedSurface),
            selected_fg: theme.colors.text_selected,
            selected_muted: theme.colors.text_selected,
            variant,
        }
    }
}

impl Default for FinderPalette {
    fn default() -> Self {
        Self::for_variant(ThemeVariant::DefaultDark)
    }
}
