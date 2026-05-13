use super::*;

impl App {
    pub(super) fn hydrate_persisted_query_client(&mut self, persisted: PersistedGitHubQueryClient) {
        for comments in persisted.client_state.comments {
            let key = (comments.repository.clone(), comments.number);
            self.pr_comments_cache.insert(key, comments.comments);
            self.query_client.hydrate_success(
                QueryKey::pull_request_comments(&comments.repository, comments.number),
                persisted.timestamp,
            );
        }
        for diff in persisted.client_state.diffs {
            let key = (diff.repository.clone(), diff.number);
            self.pr_patch_cache.insert(key, diff.patch);
            self.query_client.hydrate_success(
                QueryKey::pull_request_diff(&diff.repository, diff.number),
                persisted.timestamp,
            );
        }
        for semantic in persisted.client_state.semantic_diffs {
            let route_id = semantic.route_id.clone();
            self.persisted_semantic_diff_cache
                .insert(route_id.clone(), semantic.diff.clone());
            self.query_client
                .hydrate_success(QueryKey::semantic_diff(route_id), persisted.timestamp);
            if let Some(route) = self.route_for_persisted_semantic_diff(&semantic.route_id) {
                self.semantic_diff_cache.insert(route, semantic.diff);
            }
        }
    }

    pub(super) fn persist_github_query_client(&self) {
        let store = self.store.clone();
        let client = self.dehydrate_github_query_client();
        thread::spawn(move || {
            store.persist_github_query_client(client);
        });
    }

    pub(super) fn dehydrate_github_query_client(&self) -> PersistedGitHubQueryClient {
        PersistedGitHubQueryClient {
            timestamp: now_stamp() as i64,
            buster: GITHUB_QUERY_CACHE_BUSTER.to_string(),
            client_state: GitHubQueryClientState {
                queue: Some(self.github.clone()),
                comments: self
                    .pr_comments_cache
                    .iter()
                    .map(
                        |((repository, number), comments)| PersistedPullRequestComments {
                            repository: repository.clone(),
                            number: *number,
                            comments: comments.clone(),
                        },
                    )
                    .collect(),
                diffs: self
                    .pr_patch_cache
                    .iter()
                    .map(|((repository, number), patch)| PersistedPullRequestDiff {
                        repository: repository.clone(),
                        number: *number,
                        patch: patch.clone(),
                    })
                    .collect(),
                semantic_diffs: self
                    .persisted_semantic_diff_cache
                    .iter()
                    .map(|(route_id, diff)| PersistedSemanticDiff {
                        route_id: route_id.clone(),
                        diff: diff.clone(),
                    })
                    .chain(self.semantic_diff_cache.iter().map(|(route, diff)| {
                        PersistedSemanticDiff {
                            route_id: route.session_id(),
                            diff: diff.clone(),
                        }
                    }))
                    .collect(),
            },
        }
    }

    fn route_for_persisted_semantic_diff(&self, route_id: &str) -> Option<DiffSource> {
        if self.local_route().session_id() == route_id {
            return Some(self.local_route());
        }
        for item in self.home_work_items() {
            let route = item.route(self);
            if route.session_id() == route_id {
                return Some(route);
            }
        }
        if let (Some(route), Some(commit)) =
            (&self.commit_route, self.commits.get(self.commit_selection))
        {
            let route = DiffSource::Commit {
                repo_path: route.repo_path.clone(),
                sha: commit.sha.clone(),
            };
            if route.session_id() == route_id {
                return Some(route);
            }
        }
        None
    }

    pub(super) fn drain_query_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.query_rx.try_recv() {
            changed = true;
            match result {
                QueryEvent::ProjectLabel(result) => match result {
                    Ok(project_label) => {
                        self.project_label = project_label;
                        self.query_client
                            .finish_success(QueryKey::ProjectLabel, now_stamp() as i64);
                    }
                    Err(error) => self
                        .query_client
                        .finish_error(QueryKey::ProjectLabel, error),
                },
                QueryEvent::ThemePreference(result) => match result {
                    Ok(theme_variant) => {
                        if let Some(theme_variant) = theme_variant {
                            self.theme_variant = theme_variant;
                        }
                        self.query_client
                            .finish_success(QueryKey::ThemePreference, now_stamp() as i64);
                    }
                    Err(error) => self
                        .query_client
                        .finish_error(QueryKey::ThemePreference, error),
                },
                QueryEvent::LocalDiff(result) => match result {
                    Ok(local_diff) => {
                        self.local_document = local_diff.document.clone();
                        self.local_route = LocalWorktreeRoute {
                            repo_path: local_diff.repo_path,
                            branch: local_diff.branch,
                            base_ref: local_diff.base_ref,
                        };
                        let route = DiffSource::LocalWorktree(self.local_route.clone());
                        self.query_client
                            .finish_success(QueryKey::LocalDiff, now_stamp() as i64);
                        self.revalidate_semantic_diff(route.clone());
                        if matches!(self.diff_source, DiffSource::LocalWorktree(_)) {
                            self.session = App::load_session_for_route(
                                &self.store,
                                &route,
                                &local_diff.document,
                            );
                            self.sync_viewed_state_for_session();
                            self.diff_source = route;
                            self.replace_document_preserving_view(local_diff.document);
                        }
                    }
                    Err(error) => self.query_client.finish_error(QueryKey::LocalDiff, error),
                },
                QueryEvent::Worktrees(result) => match result {
                    Ok(worktrees) => {
                        self.worktrees = worktrees;
                        self.query_client
                            .finish_success(QueryKey::Worktrees, now_stamp() as i64);
                    }
                    Err(error) => self.query_client.finish_error(QueryKey::Worktrees, error),
                },
                QueryEvent::BranchOperation(result) => {
                    self.branch_operation_status = Some(match result {
                        Ok(message) => message,
                        Err(error) => format!("git failed: {error}"),
                    });
                    self.revalidate_local_diff();
                    self.revalidate_worktrees();
                }
                QueryEvent::BranchCommits(result) => match result {
                    Ok(commits) => {
                        self.commits = commits;
                        self.commit_selection = self
                            .commit_selection
                            .min(self.commits.len().saturating_sub(1));
                        self.commit_status = None;
                        self.revalidate_selected_semantic_diff();
                    }
                    Err(error) => self.commit_status = Some(format!("commit log failed: {error}")),
                },
                QueryEvent::CommitDiff {
                    repo_path,
                    sha,
                    result,
                } => match result {
                    Ok(document) => {
                        let route = DiffSource::Commit { repo_path, sha };
                        self.replace_route(AppRoute::Diff(route.clone()));
                        self.diff_source = route.clone();
                        self.session = App::load_session_for_route(&self.store, &route, &document);
                        self.sync_viewed_state_for_session();
                        self.replace_document_preserving_view(document);
                        self.state = DiffViewState::default();
                        self.surface_scroll_y = 0;
                    }
                    Err(error) => self.commit_status = Some(format!("commit diff failed: {error}")),
                },
                QueryEvent::Queue(result) => match result {
                    Ok(mut queue) => {
                        queue.cached_at = Some(now_stamp() as i64);
                        self.github = queue;
                        self.body_preview_cache.clear();
                        self.query_client.finish_success(
                            QueryKey::GitHubQueue,
                            self.github.cached_at.unwrap_or_else(|| now_stamp() as i64),
                        );
                        self.revalidate_selected_semantic_diff();
                        self.persist_github_query_client();
                    }
                    Err(error) => {
                        self.query_client
                            .finish_error(QueryKey::GitHubQueue, error.clone());
                        self.github.status = GitHubQueueStatus::Error(error);
                    }
                },
                QueryEvent::Comments {
                    repository,
                    number,
                    result,
                } => {
                    let key = (repository.clone(), number);
                    let query_key = QueryKey::pull_request_comments(&repository, number);
                    match result {
                        Ok(comments) => {
                            self.pr_comments_cache.insert(key, comments);
                            self.query_client
                                .finish_success(query_key, now_stamp() as i64);
                            self.persist_github_query_client();
                        }
                        Err(error) => {
                            self.query_client.finish_error(query_key, error);
                        }
                    }
                }
                QueryEvent::Diff {
                    repository,
                    number,
                    result,
                } => {
                    let key = (repository.clone(), number);
                    match result {
                        Ok(diff) => self.apply_pull_request_diff(
                            key,
                            diff.patch,
                            diff.document,
                            Some(now_stamp() as i64),
                        ),
                        Err(error) => {
                            self.query_client.finish_error(
                                QueryKey::pull_request_diff(&repository, number),
                                error,
                            );
                        }
                    }
                }
                QueryEvent::CachedDiff {
                    repository,
                    number,
                    patch,
                    result,
                } => {
                    let key = (repository.clone(), number);
                    match result {
                        Ok(document) => {
                            self.pr_patch_cache.insert(key.clone(), patch);
                            self.pr_diff_cache.insert(key, document.clone());
                            let route = DiffSource::PullRequest { repository, number };
                            if self.diff_source == route && self.document.files.is_empty() {
                                self.session =
                                    App::load_session_for_route(&self.store, &route, &document);
                                self.sync_viewed_state_for_session();
                                self.replace_document_preserving_view(document);
                                self.apply_pending_semantic_focus(&route);
                            }
                        }
                        Err(_error) => {}
                    }
                }
                QueryEvent::SemanticDiff { route, result } => {
                    let query_key = QueryKey::semantic_diff(route.session_id());
                    match result {
                        Ok(diff) => {
                            let route_id = route.session_id();
                            self.persisted_semantic_diff_cache
                                .insert(route_id, diff.clone());
                            self.semantic_diff_cache.insert(route, diff);
                            self.query_client
                                .finish_success(query_key, now_stamp() as i64);
                            self.persist_github_query_client();
                        }
                        Err(error) => self.query_client.finish_error(query_key, error),
                    }
                }
            }
        }
        if changed || self.last_query_gc_at.elapsed() >= QUERY_CACHE_GC_INTERVAL {
            self.gc_stale_queries();
        }
        changed
    }

    pub(super) fn gc_stale_queries(&mut self) {
        self.last_query_gc_at = Instant::now();
        let now = now_stamp() as i64;
        let protected = self.protected_query_keys();
        let removed = self
            .query_client
            .gc_stale(now, QUERY_CACHE_MAX_AGE_SECS, &protected);
        if removed.is_empty() {
            return;
        }
        let mut pr_cache_changed = false;
        for key in removed {
            match key {
                QueryKey::PullRequestComments { repository, number } => {
                    pr_cache_changed |= self
                        .pr_comments_cache
                        .remove(&(repository, number))
                        .is_some();
                }
                QueryKey::PullRequestDiff { repository, number } => {
                    pr_cache_changed |= self
                        .pr_patch_cache
                        .remove(&(repository.clone(), number))
                        .is_some();
                    self.pr_diff_cache.remove(&(repository, number));
                }
                QueryKey::SemanticDiff { route_id } => {
                    self.semantic_diff_cache
                        .retain(|route, _| route.session_id() != route_id);
                    self.persisted_semantic_diff_cache.remove(&route_id);
                    self.semantic_expanded
                        .retain(|key| key.route_id != route_id);
                    self.semantic_expansion_seeded.remove(&route_id);
                }
                QueryKey::ProjectLabel
                | QueryKey::ThemePreference
                | QueryKey::LocalDiff
                | QueryKey::Worktrees
                | QueryKey::GitHubQueue => {}
            }
        }
        if pr_cache_changed {
            self.persist_github_query_client();
        }
    }

    fn protected_query_keys(&self) -> Vec<QueryKey> {
        let mut keys = Vec::new();
        if let DiffSource::PullRequest { repository, number } = &self.diff_source {
            keys.push(QueryKey::pull_request_diff(repository, *number));
            keys.push(QueryKey::pull_request_comments(repository, *number));
        }
        keys.push(QueryKey::semantic_diff(self.diff_source.session_id()));
        if let Some(pull_request) = self
            .selected_work_item()
            .and_then(|item| item.pr_index)
            .and_then(|index| self.github.items.get(index))
        {
            keys.push(QueryKey::pull_request_diff(
                &pull_request.repository,
                pull_request.number,
            ));
            keys.push(QueryKey::pull_request_comments(
                &pull_request.repository,
                pull_request.number,
            ));
        }
        if let Some(item) = self.selected_work_item() {
            keys.push(QueryKey::semantic_diff(item.route(self).session_id()));
        }
        if let (Some(route), Some(commit)) =
            (&self.commit_route, self.commits.get(self.commit_selection))
        {
            keys.push(QueryKey::semantic_diff(
                DiffSource::Commit {
                    repo_path: route.repo_path.clone(),
                    sha: commit.sha.clone(),
                }
                .session_id(),
            ));
        }
        keys
    }

    pub(super) fn revalidate_project_label(&mut self) {
        if std::env::var("LAZYDIFF_PROJECT").is_ok() {
            self.query_client
                .hydrate_success(QueryKey::ProjectLabel, now_stamp() as i64);
            return;
        }
        if !self.query_client.start_fetch(QueryKey::ProjectLabel) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Ok(Self::detect_project_label_from_git());
            let _ = sender.send(QueryEvent::ProjectLabel(result));
        });
    }

    pub(super) fn revalidate_theme_preference(&mut self) {
        if !self.query_client.start_fetch(QueryKey::ThemePreference) {
            return;
        }
        let sender = self.query_tx.clone();
        let store = self.store.clone();
        thread::spawn(move || {
            let result = Ok(store.restore_theme_variant());
            let _ = sender.send(QueryEvent::ThemePreference(result));
        });
    }

    pub(super) fn revalidate_local_diff(&mut self) {
        if !self.query_client.start_fetch(QueryKey::LocalDiff) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Self::load_local_worktree_diff();
            let _ = sender.send(QueryEvent::LocalDiff(result));
        });
    }

    pub(super) fn revalidate_worktrees(&mut self) {
        if !self.query_client.start_fetch(QueryKey::Worktrees) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Self::load_worktrees();
            let _ = sender.send(QueryEvent::Worktrees(result));
        });
    }

    pub(super) fn revalidate_queue(&mut self) {
        let auth = self.refresh_github_auth_gate();
        if !auth.can_load_github() {
            if let Some(error) = auth.error() {
                self.query_client.finish_error(QueryKey::GitHubQueue, error);
            }
            return;
        }
        if !self.query_client.start_fetch(QueryKey::GitHubQueue) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let queue = GitHubQueue::load_fresh();
            let result = match &queue.status {
                GitHubQueueStatus::Ready => Ok(queue),
                GitHubQueueStatus::MissingToken => {
                    Err("set GITHUB_TOKEN or GH_TOKEN to load PRs".to_string())
                }
                GitHubQueueStatus::Error(error) => Err(error.clone()),
                GitHubQueueStatus::Loading => Ok(queue),
            };
            let _ = sender.send(QueryEvent::Queue(result));
        });
    }

    pub(super) fn revalidate_pull_request_comments(&mut self, repository: String, number: u32) {
        if !self.ensure_github_auth() {
            return;
        }
        let query_key = QueryKey::pull_request_comments(&repository, number);
        if !self.query_client.start_fetch(query_key) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = fetch_pull_request_comments(&repository, number);
            let _ = sender.send(QueryEvent::Comments {
                repository,
                number,
                result,
            });
        });
    }

    pub(super) fn revalidate_pull_request_diff(&mut self, repository: String, number: u32) {
        if !self.ensure_github_auth() {
            return;
        }
        let query_key = QueryKey::pull_request_diff(&repository, number);
        if !self.query_client.start_fetch(query_key) {
            return;
        }
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Self::load_pull_request_diff(&repository, number);
            let _ = sender.send(QueryEvent::Diff {
                repository,
                number,
                result,
            });
        });
    }

    pub(super) fn apply_pull_request_diff(
        &mut self,
        key: (String, u32),
        patch: String,
        document: DiffDocument,
        fetched_at: Option<i64>,
    ) {
        let updated_at = fetched_at.or_else(|| {
            self.query_client
                .get(&QueryKey::pull_request_diff(&key.0, key.1))
                .updated_at
        });
        self.pr_patch_cache.insert(key.clone(), patch);
        self.pr_diff_cache.insert(key.clone(), document.clone());
        self.query_client.finish_success(
            QueryKey::pull_request_diff(&key.0, key.1),
            updated_at.unwrap_or_else(|| now_stamp() as i64),
        );
        self.persist_github_query_client();
        let route = DiffSource::PullRequest {
            repository: key.0,
            number: key.1,
        };
        if self.diff_source == route {
            self.session = App::load_session_for_route(&self.store, &route, &document);
            self.sync_viewed_state_for_session();
            self.replace_document_preserving_view(document);
            self.apply_pending_semantic_focus(&route);
        }
    }
}
