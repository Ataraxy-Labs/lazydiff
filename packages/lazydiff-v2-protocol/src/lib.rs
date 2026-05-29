use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Viewport {
    pub first_row: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum WorkspaceKind {
    PatchFile,
    LocalDiff,
    PullRequest,
    CommitDiff,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum SurfaceId {
    Diff,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    pub surface: Option<SurfaceId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct KeymapContribution {
    pub key: String,
    pub command: String,
    pub surface: Option<SurfaceId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AppFrame {
    pub active_surface: SurfaceId,
    pub workspace_kind: WorkspaceKind,
    pub diff: DiffFrame,
    pub commands: Vec<CommandContribution>,
    pub keymaps: Vec<KeymapContribution>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DiffFrame {
    pub total_rows: usize,
    pub rows: Vec<DiffRow>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DiffRow {
    pub visual_index: usize,
    pub kind: DiffRowKind,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum DiffRowKind {
    FileHeader,
    HunkHeader,
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum ClientEvent {
    Frame { viewport: Viewport },
}
