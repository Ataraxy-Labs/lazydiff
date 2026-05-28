//! Diff Workspace owner seam.
//!
//! This module starts as a private-state scaffold. Follow-up slices move diff
//! interaction state behind this `update(intent) -> effects` door.

#[derive(Debug, Default)]
pub(crate) struct DiffWorkspace {
    no_op_updates: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffWorkspaceContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffWorkspaceIntent {
    NoOp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffWorkspaceEffect {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffWorkspaceFrame<'a> {
    workspace: &'a DiffWorkspace,
}

impl DiffWorkspace {
    pub(crate) fn update(
        &mut self,
        intent: DiffWorkspaceIntent,
        _ctx: DiffWorkspaceContext,
    ) -> Vec<DiffWorkspaceEffect> {
        match intent {
            DiffWorkspaceIntent::NoOp => {
                self.no_op_updates = self.no_op_updates.saturating_add(1);
                Vec::new()
            }
        }
    }

    pub(crate) fn frame(&mut self) -> DiffWorkspaceFrame<'_> {
        DiffWorkspaceFrame { workspace: self }
    }
}

impl DiffWorkspaceFrame<'_> {
    fn no_op_updates(&self) -> u64 {
        self.workspace.no_op_updates
    }
}

#[cfg(test)]
mod skeleton {
    use super::*;

    #[test]
    fn skeleton_no_op_update_round_trips_to_frame() {
        let mut workspace = DiffWorkspace::default();

        let effects = workspace.update(DiffWorkspaceIntent::NoOp, DiffWorkspaceContext);
        let frame = workspace.frame();

        assert!(effects.is_empty());
        assert_eq!(frame.no_op_updates(), 1);
    }
}
