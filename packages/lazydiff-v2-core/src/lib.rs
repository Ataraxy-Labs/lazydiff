use lazydiff_v2_diff::{DiffDocument, DiffParseError};
use lazydiff_v2_protocol::{
    AppFrame, CommandContribution, KeymapContribution, SurfaceId, Viewport, WorkspaceKind,
};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ContributionRegistry {
    pub commands: Vec<CommandContribution>,
    pub keymaps: Vec<KeymapContribution>,
}

impl ContributionRegistry {
    pub fn built_ins() -> Self {
        Self {
            commands: vec![
                CommandContribution {
                    id: "app.quit".to_string(),
                    title: "Quit".to_string(),
                    surface: None,
                },
                CommandContribution {
                    id: "app.open_context_help".to_string(),
                    title: "Open context help".to_string(),
                    surface: None,
                },
                CommandContribution {
                    id: "diff.scroll_down".to_string(),
                    title: "Scroll diff down".to_string(),
                    surface: Some(SurfaceId::Diff),
                },
                CommandContribution {
                    id: "diff.scroll_up".to_string(),
                    title: "Scroll diff up".to_string(),
                    surface: Some(SurfaceId::Diff),
                },
            ],
            keymaps: vec![
                KeymapContribution {
                    key: "q".to_string(),
                    command: "app.quit".to_string(),
                    surface: None,
                },
                KeymapContribution {
                    key: "?".to_string(),
                    command: "app.open_context_help".to_string(),
                    surface: None,
                },
                KeymapContribution {
                    key: "j".to_string(),
                    command: "diff.scroll_down".to_string(),
                    surface: Some(SurfaceId::Diff),
                },
                KeymapContribution {
                    key: "k".to_string(),
                    command: "diff.scroll_up".to_string(),
                    surface: Some(SurfaceId::Diff),
                },
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppCore {
    workspace_kind: WorkspaceKind,
    registry: ContributionRegistry,
    diff: DiffDocument,
}

impl AppCore {
    pub fn from_patch_text(
        patch_text: &str,
        workspace_kind: WorkspaceKind,
    ) -> Result<Self, DiffParseError> {
        Ok(Self {
            workspace_kind,
            registry: ContributionRegistry::built_ins(),
            diff: DiffDocument::parse(patch_text)?,
        })
    }

    pub fn frame(&self, viewport: Viewport) -> AppFrame {
        AppFrame {
            active_surface: SurfaceId::Diff,
            workspace_kind: self.workspace_kind.clone(),
            diff: self.diff.frame(viewport),
            commands: self.registry.commands.clone(),
            keymaps: self.registry.keymaps.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_core_frame_contains_shared_contributions_and_diff_rows() {
        let patch =
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let app = AppCore::from_patch_text(patch, WorkspaceKind::PatchFile).unwrap();
        let frame = app.frame(Viewport {
            first_row: 0,
            height: 8,
        });

        assert_eq!(frame.active_surface, SurfaceId::Diff);
        assert!(
            frame
                .commands
                .iter()
                .any(|command| command.id == "app.open_context_help")
        );
        assert!(frame.diff.rows.iter().any(|row| row.text.contains("a.txt")));
        assert!(frame.diff.rows.iter().any(|row| row.text == "-old"));
        assert!(frame.diff.rows.iter().any(|row| row.text == "+new"));
    }
}
