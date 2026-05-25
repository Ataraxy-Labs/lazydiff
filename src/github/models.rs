use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::app::WorkItemKind;
use crate::text::{_wrap_plain_text, markdown_preview_lines};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GitHubQueue {
    pub(crate) viewer: Option<String>,
    pub(crate) status: GitHubQueueStatus,
    pub(crate) items: Vec<GitHubPullRequest>,
    #[serde(default)]
    pub(crate) cached_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum GitHubQueueStatus {
    MissingToken,
    Loading,
    Ready,
    Error(String),
}

impl GitHubQueue {
    pub(crate) fn empty_loading() -> Self {
        Self {
            viewer: None,
            status: GitHubQueueStatus::Loading,
            items: Vec::new(),
            cached_at: None,
        }
    }

    pub(crate) fn summary(&self) -> &str {
        match &self.status {
            GitHubQueueStatus::MissingToken => "set GITHUB_TOKEN or GH_TOKEN to load PRs",
            GitHubQueueStatus::Loading => "loading GitHub PRs…",
            GitHubQueueStatus::Ready => "updated now",
            GitHubQueueStatus::Error(_) => "GitHub load failed · press r to retry",
        }
    }

    pub(crate) fn notice(&self) -> Option<String> {
        match &self.status {
            GitHubQueueStatus::MissingToken => {
                Some("GitHub PR queue disabled: set GITHUB_TOKEN or GH_TOKEN".to_string())
            }
            GitHubQueueStatus::Loading => Some("Loading GitHub PR queue…".to_string()),
            GitHubQueueStatus::Error(error) => Some(format!("GitHub PR queue failed: {error}")),
            GitHubQueueStatus::Ready => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GitHubPullRequest {
    pub(crate) kind: WorkItemKind,
    pub(crate) repository: String,
    pub(crate) author: String,
    pub(crate) head_ref_name: String,
    pub(crate) number: u32,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) changed_files: usize,
    pub(crate) review_status: ReviewStatus,
    pub(crate) check_status: CheckRollupStatus,
    pub(crate) check_summary: Option<String>,
    pub(crate) checks: Vec<GitHubCheck>,
    pub(crate) comments: Vec<GitHubComment>,
    pub(crate) created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ReviewStatus {
    Draft,
    Approved,
    Changes,
    Review,
    None,
}

impl ReviewStatus {
    pub(crate) fn from_raw(is_draft: bool, decision: Option<&str>) -> Self {
        if is_draft {
            return Self::Draft;
        }
        match decision {
            Some("APPROVED") => Self::Approved,
            Some("CHANGES_REQUESTED") => Self::Changes,
            Some("REVIEW_REQUIRED") => Self::Review,
            _ => Self::None,
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Self::Draft => "◌",
            Self::Approved => "✓",
            Self::Changes => "!",
            Self::Review => "◐",
            Self::None => "·",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CheckRollupStatus {
    Passing,
    Pending,
    Failing,
    None,
}

impl CheckRollupStatus {
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Self::Passing => "✓",
            Self::Failing => "×",
            Self::Pending => "◐",
            Self::None => "",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Passing => "checks passing",
            Self::Failing => "checks failing",
            Self::Pending => "checks pending",
            Self::None => "no checks",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GitHubCheck {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) conclusion: Option<String>,
}

impl GitHubCheck {
    pub(crate) fn status_symbol(&self) -> (&'static str, Color) {
        match self.conclusion.as_deref() {
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => ("✓", Color::Rgb(158, 206, 106)),
            Some(_) => ("×", Color::Rgb(247, 118, 142)),
            None if self.status == "COMPLETED" => ("·", Color::Rgb(86, 95, 137)),
            None => ("◐", Color::Rgb(224, 175, 104)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GitHubComment {
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: String,
}

impl GitHubComment {
    pub(crate) fn _preview_lines(&self, width: usize) -> Vec<String> {
        markdown_preview_lines(&self.body, 3)
            .into_iter()
            .flat_map(|line| _wrap_plain_text(&line, width))
            .collect()
    }
}
