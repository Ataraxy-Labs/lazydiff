use ratatui::style::{Color, Modifier, Style};
use lazydiff_diffs::{DiffTheme, DiffThemeName, SyntaxTheme};
use serde::{Deserialize, Serialize};

/// Quiver's design-system tokens for the ratatui demo.
///
/// Inspired by GPUI/Zed's split between theme colors, elevation indices, status
/// colors, and typography helpers. The terminal cannot express font families or
/// shadows, so the seam is intentionally semantic: renderers ask for layers,
/// status, and text roles instead of scattering RGB literals.
///
/// Register: Glow-inspired terminal structure with Tokyo Night colors: deep
/// indigo surfaces, calm blue foregrounds, cyan accents, purple focus, and
/// readable green/red state colors for long review sessions.
///
/// A second variant — `graphite` — restores the cool-graphite GitHub-clone
/// register the TUI shipped with before the redesign. The two are
/// runtime-toggleable so we can A/B compare without rebuilding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ThemeVariant {
    /// Tokyo Night register with Glow-inspired spacing/selection behavior.
    Warm,
    /// Cool-graphite blue-grey, full-width selection bar, warning-yellow
    /// headings — the pre-redesign register.
    Graphite,
}

impl ThemeVariant {
    pub(crate) fn toggled(self) -> Self {
        match self {
            ThemeVariant::Warm => ThemeVariant::Graphite,
            ThemeVariant::Graphite => ThemeVariant::Warm,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ThemeVariant::Warm => "tokyo",
            ThemeVariant::Graphite => "graphite",
        }
    }

    pub(crate) fn from_label(label: &str) -> Option<Self> {
        match label {
            "tokyo" | "warm" => Some(Self::Warm),
            "graphite" => Some(Self::Graphite),
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

impl QuiverTheme {
    pub(crate) fn for_variant(variant: ThemeVariant) -> Self {
        match variant {
            ThemeVariant::Warm => Self::dark(),
            ThemeVariant::Graphite => Self::graphite(),
        }
    }

    pub(crate) fn dark() -> Self {
        Self {
            variant: ThemeVariant::Warm,
            colors: ThemeColors {
                background: Color::Rgb(26, 27, 38),
                surface: Color::Rgb(36, 40, 59),
                editor_surface: Color::Rgb(26, 27, 38),
                elevated_surface: Color::Rgb(41, 46, 66),
                modal_surface: Color::Rgb(31, 35, 53),
                text: Color::Rgb(169, 177, 214),
                text_muted: Color::Rgb(154, 165, 206),
                text_dim: Color::Rgb(86, 95, 137),
                text_selected: Color::Rgb(187, 154, 247),
                border: Color::Rgb(59, 66, 97),
                border_focused: Color::Rgb(187, 154, 247),
                rule_dim: Color::Rgb(41, 46, 66),
                accent: Color::Rgb(125, 207, 255),
                count: Color::Rgb(154, 165, 206),
                action: Color::Rgb(187, 154, 247),
                code_fg: Color::Rgb(224, 175, 104),
            },
            status: StatusColors {
                success: Color::Rgb(158, 206, 106),
                warning: Color::Rgb(224, 175, 104),
                danger: Color::Rgb(247, 118, 142),
                modified: Color::Rgb(224, 175, 104),
                added: Color::Rgb(158, 206, 106),
                deleted: Color::Rgb(247, 118, 142),
            },
            typography: Typography,
        }
    }

    /// Pre-redesign register: cool-graphite GitHub-clone palette. Selection is
    /// a saturated blue strip, headings are warning-yellow, accent is cool
    /// blue. Kept as a comparison toggle for the redesign.
    pub(crate) fn graphite() -> Self {
        Self {
            variant: ThemeVariant::Graphite,
            colors: ThemeColors {
                background: Color::Rgb(17, 16, 24),
                surface: Color::Rgb(20, 22, 30),
                editor_surface: Color::Rgb(17, 19, 21),
                elevated_surface: Color::Rgb(28, 33, 51),
                modal_surface: Color::Rgb(29, 36, 48),
                text: Color::Rgb(237, 231, 218),
                text_muted: Color::Rgb(159, 151, 136),
                text_dim: Color::Rgb(111, 104, 93),
                text_selected: Color::Rgb(248, 250, 252),
                border: Color::Rgb(111, 104, 93),
                border_focused: Color::Rgb(96, 165, 250),
                rule_dim: Color::Rgb(48, 51, 64),
                accent: Color::Rgb(96, 165, 250),
                count: Color::Rgb(244, 165, 28),
                action: Color::Rgb(244, 165, 28),
                // Graphite-era inline-code wheat from the GHUI reference.
                code_fg: Color::Rgb(215, 197, 161),
            },
            status: StatusColors {
                success: Color::Rgb(136, 211, 155),
                warning: Color::Rgb(244, 165, 28),
                danger: Color::Rgb(240, 160, 160),
                modified: Color::Rgb(244, 165, 28),
                added: Color::Rgb(136, 211, 155),
                deleted: Color::Rgb(240, 160, 160),
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

    /// Quiver-flavored `DiffTheme` derived from this theme. Keeps the Tokyo
    /// Night register and avoids the cool-graphite defaults shipped by the
    /// diff package.
    ///
    /// Approach: open background, visible but not fluorescent add/del row
    /// tints, and syntax colors from the Tokyo Night palette.
    pub(crate) fn diff_theme(self) -> DiffTheme {
        if self.variant == ThemeVariant::Graphite {
            return DiffTheme::graphite();
        }
        let c = self.colors;
        let s = self.status;
        DiffTheme {
            // Re-using the Paper variant tag is fine; it just labels the kind.
            name: DiffThemeName::Paper,
            bg: c.background,
            panel: c.surface,
            panel_alt: c.elevated_surface,
            file_header: c.surface,
            hunk: c.elevated_surface,
            text: c.text,
            muted: c.text_muted,
            line_number_bg: c.background,
            line_number_fg: c.text_dim,
            context_content_bg: c.background,
            add_bg: Color::Rgb(39, 71, 54),
            del_bg: Color::Rgb(79, 42, 55),
            add_content_bg: Color::Rgb(49, 89, 66),
            del_content_bg: Color::Rgb(96, 49, 65),
            add_gutter_bg: Color::Rgb(49, 89, 66),
            del_gutter_bg: Color::Rgb(96, 49, 65),
            add_fg: s.added,
            del_fg: s.deleted,
            selected: c.elevated_surface,
            syntax: SyntaxTheme {
                default: c.text,
                keyword: Color::Rgb(187, 154, 247),
                string: Color::Rgb(158, 206, 106),
                comment: c.text_dim,
                number: Color::Rgb(255, 158, 100),
                function: Color::Rgb(122, 162, 247),
                property: c.text_muted,
                r#type: Color::Rgb(125, 207, 255),
                punctuation: c.text_muted,
            },
        }
    }
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
        let status = theme.status;
        match role {
            TextRole::Body => Style::new().fg(colors.text).bg(bg),
            TextRole::Muted => Style::new().fg(colors.text_muted).bg(bg),
            TextRole::Metadata => Style::new()
                .fg(colors.text_muted)
                .bg(bg)
                .add_modifier(Modifier::DIM),
            TextRole::SemanticHook => Style::new()
                .fg(colors.text_muted)
                .bg(bg)
                .add_modifier(Modifier::ITALIC),
            // Graphite restores the pre-redesign warning-yellow heading; warm
            // promotes headings to plain text BOLD per the redesign.
            TextRole::Heading => match theme.variant {
                ThemeVariant::Graphite => Style::new()
                    .fg(status.warning)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
                ThemeVariant::Warm => Style::new()
                    .fg(colors.text)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            },
            TextRole::Brand => Style::new()
                .fg(colors.text)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            TextRole::Key => Style::new()
                .fg(match theme.variant {
                    ThemeVariant::Warm => colors.accent,
                    ThemeVariant::Graphite => colors.text,
                })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            TextRole::Action => Style::new().fg(colors.action).bg(bg),
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
    SemanticHook,
    Heading,
    Brand,
    Key,
    Action,
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
    pub(crate) warning: Color,
    pub(crate) danger: Color,
    pub(crate) orange: Color,
    pub(crate) accent: Color,
    pub(crate) count: Color,
    pub(crate) action: Color,
    pub(crate) code_fg: Color,
    pub(crate) theme: QuiverTheme,
}

impl HomePalette {
    pub(crate) fn quiver() -> Self {
        Self::for_variant(ThemeVariant::Warm)
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
            warning: theme.status.warning,
            danger: theme.status.danger,
            orange: theme.status.modified,
            accent: theme.colors.accent,
            count: theme.colors.count,
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
        Self::for_variant(ThemeVariant::Warm)
    }
}
