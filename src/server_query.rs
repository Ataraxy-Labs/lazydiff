use std::{collections::HashMap, time::SystemTime};

use lazydiff_diffs::DiffDocument;

use crate::{
    GitHubComment, GitHubQueue,
    app::{DiffSource, SemanticDiff},
    design_system::ThemeVariant,
    github::{GitCommit, Worktree},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum QueryKey {
    ProjectLabel,
    ThemePreference,
    LocalDiff,
    Worktrees,
    GitHubQueue,
    PullRequestComments { repository: String, number: u32 },
    PullRequestDiff { repository: String, number: u32 },
    SemanticDiff { route_id: String },
}

impl QueryKey {
    pub(crate) fn pull_request_comments(repository: &str, number: u32) -> Self {
        Self::PullRequestComments {
            repository: repository.to_string(),
            number,
        }
    }

    pub(crate) fn pull_request_diff(repository: &str, number: u32) -> Self {
        Self::PullRequestDiff {
            repository: repository.to_string(),
            number,
        }
    }

    pub(crate) fn semantic_diff(route_id: impl Into<String>) -> Self {
        Self::SemanticDiff {
            route_id: route_id.into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct QueryClient {
    cache: QueryCache,
}

impl QueryClient {
    pub(crate) fn get(&self, key: &QueryKey) -> QueryResult {
        self.cache
            .get(key)
            .cloned()
            .unwrap_or_else(QueryState::pending_idle)
            .into_result()
    }

    pub(crate) fn is_fetching(&self) -> bool {
        self.cache
            .queries
            .values()
            .any(|query| query.fetch_status == FetchStatus::Fetching)
    }

    pub(crate) fn hydrate_success(&mut self, key: QueryKey, updated_at: i64) {
        self.cache.insert(key, QueryState::success_idle(updated_at));
    }

    pub(crate) fn start_fetch(&mut self, key: QueryKey) -> bool {
        let current = self
            .cache
            .get(&key)
            .cloned()
            .unwrap_or_else(QueryState::pending_idle);
        if current.fetch_status == FetchStatus::Fetching {
            return false;
        }
        self.cache.insert(key, current.fetching());
        true
    }

    pub(crate) fn finish_success(&mut self, key: QueryKey, updated_at: i64) {
        self.cache.insert(key, QueryState::success_idle(updated_at));
    }

    pub(crate) fn finish_error(&mut self, key: QueryKey, error: String) {
        let current = self
            .cache
            .get(&key)
            .cloned()
            .unwrap_or_else(QueryState::pending_idle);
        self.cache.insert(key, current.error_idle(error));
    }

    pub(crate) fn gc_stale(
        &mut self,
        now: i64,
        max_age_secs: i64,
        protected: &[QueryKey],
    ) -> Vec<QueryKey> {
        self.cache.gc_stale(now, max_age_secs, protected)
    }
}

#[derive(Clone, Debug, Default)]
struct QueryCache {
    queries: HashMap<QueryKey, QueryState>,
}

impl QueryCache {
    fn get(&self, key: &QueryKey) -> Option<&QueryState> {
        self.queries.get(key)
    }

    fn insert(&mut self, key: QueryKey, state: QueryState) {
        self.queries.insert(key, state);
    }

    fn gc_stale(&mut self, now: i64, max_age_secs: i64, protected: &[QueryKey]) -> Vec<QueryKey> {
        let mut removed = Vec::new();
        self.queries.retain(|key, state| {
            if key.is_static()
                || protected.contains(key)
                || state.fetch_status == FetchStatus::Fetching
            {
                return true;
            }
            let age = now.saturating_sub(state.touched_at);
            let keep = age <= max_age_secs;
            if !keep {
                removed.push(key.clone());
            }
            keep
        });
        removed
    }
}

#[derive(Clone, Debug)]
struct QueryState {
    status: QueryStatus,
    fetch_status: FetchStatus,
    updated_at: Option<i64>,
    touched_at: i64,
    error: Option<String>,
}

impl QueryState {
    fn pending_idle() -> Self {
        Self {
            status: QueryStatus::Pending,
            fetch_status: FetchStatus::Idle,
            updated_at: None,
            touched_at: query_now_stamp(),
            error: None,
        }
    }

    fn success_idle(updated_at: i64) -> Self {
        Self {
            status: QueryStatus::Success,
            fetch_status: FetchStatus::Idle,
            updated_at: Some(updated_at),
            touched_at: updated_at,
            error: None,
        }
    }

    fn fetching(mut self) -> Self {
        self.fetch_status = FetchStatus::Fetching;
        self.touched_at = query_now_stamp();
        if self.updated_at.is_none() {
            self.status = QueryStatus::Pending;
            self.error = None;
        }
        self
    }

    fn error_idle(mut self, error: String) -> Self {
        self.fetch_status = FetchStatus::Idle;
        self.status = QueryStatus::Error;
        self.touched_at = query_now_stamp();
        self.error = Some(error);
        self
    }

    fn into_result(self) -> QueryResult {
        QueryResult {
            status: self.status,
            fetch_status: self.fetch_status,
            updated_at: self.updated_at,
            error: self.error,
        }
    }
}

impl QueryKey {
    fn is_static(&self) -> bool {
        matches!(
            self,
            Self::ProjectLabel
                | Self::ThemePreference
                | Self::LocalDiff
                | Self::Worktrees
                | Self::GitHubQueue
        )
    }
}

fn query_now_stamp() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryStatus {
    Pending,
    Success,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FetchStatus {
    Idle,
    Fetching,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryResult {
    pub(crate) status: QueryStatus,
    pub(crate) fetch_status: FetchStatus,
    pub(crate) updated_at: Option<i64>,
    pub(crate) error: Option<String>,
}

impl QueryResult {
    pub(crate) fn is_fetching(&self) -> bool {
        self.fetch_status == FetchStatus::Fetching
    }

    pub(crate) fn is_initial_loading(&self) -> bool {
        self.status == QueryStatus::Pending && self.is_fetching()
    }

    pub(crate) fn is_refetching(&self) -> bool {
        self.status != QueryStatus::Pending && self.is_fetching()
    }

    pub(crate) fn label(&self) -> String {
        if self.is_initial_loading() {
            return "loading…".to_string();
        }
        if self.is_refetching() {
            return self
                .updated_at
                .map(|updated_at| {
                    format!(
                        "refreshing · cached {}",
                        crate::relative_unix_age(updated_at)
                    )
                })
                .unwrap_or_else(|| "refreshing…".to_string());
        }
        match self.status {
            QueryStatus::Pending => "not loaded".to_string(),
            QueryStatus::Success => self
                .updated_at
                .map(|updated_at| format!("cached {}", crate::relative_unix_age(updated_at)))
                .unwrap_or_else(|| "cached".to_string()),
            QueryStatus::Error => self
                .error
                .as_ref()
                .map(|error| format!("error: {error}"))
                .unwrap_or_else(|| "error".to_string()),
        }
    }
}

pub(crate) enum QueryEvent {
    ProjectLabel(std::result::Result<Option<String>, String>),
    ThemePreference(std::result::Result<Option<ThemeVariant>, String>),
    LocalDiff(std::result::Result<LocalDiffResult, String>),
    Worktrees(std::result::Result<Vec<Worktree>, String>),
    BranchCommits(std::result::Result<Vec<GitCommit>, String>),
    CommitDiff {
        repo_path: String,
        sha: String,
        result: std::result::Result<DiffDocument, String>,
    },
    BranchOperation(std::result::Result<String, String>),
    Queue(std::result::Result<GitHubQueue, String>),
    Comments {
        repository: String,
        number: u32,
        result: std::result::Result<Vec<GitHubComment>, String>,
    },
    PostedComment {
        repository: String,
        number: u32,
        result: std::result::Result<GitHubComment, String>,
    },
    Diff {
        repository: String,
        number: u32,
        result: std::result::Result<PullRequestDiffResult, String>,
    },
    CachedDiff {
        repository: String,
        number: u32,
        patch: String,
        result: std::result::Result<DiffDocument, String>,
    },
    SemanticDiff {
        route: DiffSource,
        result: std::result::Result<SemanticDiff, String>,
    },
}

pub(crate) struct LocalDiffResult {
    pub(crate) repo_path: String,
    pub(crate) branch: String,
    pub(crate) base_ref: String,
    pub(crate) document: DiffDocument,
}

pub(crate) struct PullRequestDiffResult {
    pub(crate) patch: String,
    pub(crate) document: DiffDocument,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_stale_removes_only_unprotected_idle_dynamic_queries() {
        let mut client = QueryClient::default();
        let stale_comments = QueryKey::pull_request_comments("owner/repo", 1);
        let protected_diff = QueryKey::pull_request_diff("owner/repo", 2);
        let fetching_diff = QueryKey::pull_request_diff("owner/repo", 3);
        client.hydrate_success(stale_comments.clone(), 10);
        client.hydrate_success(protected_diff.clone(), 10);
        client.hydrate_success(QueryKey::GitHubQueue, 10);
        client.hydrate_success(QueryKey::Worktrees, 10);
        assert!(client.start_fetch(fetching_diff.clone()));

        let removed = client.gc_stale(10_000, 60, std::slice::from_ref(&protected_diff));

        assert_eq!(removed, vec![stale_comments.clone()]);
        assert_eq!(client.get(&stale_comments).status, QueryStatus::Pending);
        assert_eq!(client.get(&protected_diff).status, QueryStatus::Success);
        assert!(client.get(&fetching_diff).is_fetching());
        assert_eq!(
            client.get(&QueryKey::GitHubQueue).status,
            QueryStatus::Success
        );
        assert_eq!(
            client.get(&QueryKey::Worktrees).status,
            QueryStatus::Success
        );
    }
}
