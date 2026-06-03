pub(crate) mod credentials;
pub(crate) mod detect;
pub(crate) mod gitea;
pub(crate) mod gitlab;

use lazydiff_diffs::DiffLineRangeTarget;

use crate::github::models::{GitHubComment, GitHubQueue};
use crate::github::service::GitHubAuthStatus;
use crate::github::worktree::GitCommit;

/// Forge-agnostic aliases so the rest of the codebase can migrate
/// incrementally away from GitHub-specific names.
pub(crate) type ForgeQueue = GitHubQueue;
pub(crate) type ForgeComment = GitHubComment;
pub(crate) type ForgeAuthStatus = GitHubAuthStatus;

#[derive(Clone, Debug, Default)]
pub(crate) struct PullRequestFileSources {
    pub(crate) old: Option<String>,
    pub(crate) new: Option<String>,
}

/// Abstraction over a code forge (GitHub, GitLab, Gitea/Forgejo, …).
pub(crate) trait Forge: Send + Sync {
    /// Human-readable name shown in UI messages (e.g. "GitHub", "GitLab").
    fn name(&self) -> &'static str;

    /// Check whether the user is currently authenticated.
    fn auth_status(&self) -> ForgeAuthStatus;

    /// Interactive login flow. Returns the username on success.
    fn login(&self) -> Result<String, String>;

    /// Reuse already-persisted credentials without starting an interactive login flow.
    fn connect_existing_login(&self) -> Result<String, String> {
        Err(format!("no existing {} credentials", self.name()))
    }

    /// Remove persisted credentials. Returns `true` if something was removed.
    fn logout(&self) -> Result<bool, String>;

    /// Fetch the review queue (open PRs/MRs assigned to or authored by the user).
    fn fetch_queue(&self) -> Result<ForgeQueue, String>;

    /// Fetch comments on a pull request / merge request.
    fn fetch_pull_request_comments(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<ForgeComment>, String>;

    /// Fetch the unified diff patch for a pull request / merge request.
    fn fetch_pull_request_patch(&self, repo: &str, number: u32) -> Result<String, String>;

    /// Fetch full old/new file contents for the changed files in a pull request / merge request.
    fn fetch_pull_request_file_sources(
        &self,
        _repo: &str,
        _number: u32,
        _paths: &[String],
    ) -> Result<std::collections::HashMap<String, PullRequestFileSources>, String> {
        Ok(std::collections::HashMap::new())
    }

    /// Fetch commits belonging to a pull request / merge request.
    fn fetch_pull_request_commits(&self, repo: &str, number: u32)
    -> Result<Vec<GitCommit>, String>;

    /// Fetch the unified diff patch for a single commit.
    fn fetch_commit_patch(&self, repo: &str, sha: &str) -> Result<String, String>;

    /// Post an inline comment on a pull request / merge request.
    fn post_comment(
        &self,
        repo: &str,
        number: u32,
        target: &DiffLineRangeTarget,
        body: &str,
    ) -> Result<ForgeComment, String>;

    /// URL to open a pull request / merge request in the browser.
    fn pull_request_url(&self, repo: &str, number: u32) -> String;

    /// URL to open a repository branch in the browser.
    fn branch_url(&self, repo: &str, branch: &str) -> String;
}
