use super::*;
use sem_core::{
    git::{
        bridge::GitBridge,
        types::{DiffScope, FileChange, FileStatus},
    },
    model::change::ChangeType,
    parser::{differ::compute_semantic_diff, plugins::create_default_registry},
};

const SEMANTIC_CHANGE_LIMIT: usize = 240;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct SemanticDiff {
    pub(crate) files: Vec<SemanticFile>,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SemanticFile {
    pub(crate) path: String,
    pub(crate) changes: Vec<SemanticChange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SemanticChange {
    pub(crate) entity_type: String,
    pub(crate) entity_name: String,
    pub(crate) change_type: String,
    pub(crate) line: Option<usize>,
    #[serde(default)]
    pub(crate) end_line: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SemanticNodeKey {
    pub(crate) route_id: String,
    pub(crate) path: String,
}

impl SemanticNodeKey {
    fn file(route: &DiffSource, path: &str) -> Self {
        Self::scoped(route, &format!("file:{path}"))
    }

    fn directory(route: &DiffSource, path: &str) -> Self {
        Self::scoped(route, &format!("dir:{path}"))
    }

    fn scoped(route: &DiffSource, path: &str) -> Self {
        Self {
            route_id: route.session_id(),
            path: path.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SemanticTreeRow {
    Directory {
        key: SemanticNodeKey,
        name: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        key: SemanticNodeKey,
        path: String,
        name: String,
        depth: usize,
        change_count: usize,
        collapsed: bool,
    },
    Entity {
        path: String,
        depth: usize,
        entity_type: String,
        entity_name: String,
        change_type: String,
        line: Option<usize>,
        end_line: Option<usize>,
    },
    Status(String),
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticMapNode {
    pub(crate) row_index: Option<usize>,
    pub(crate) parent: Option<usize>,
    pub(crate) x: i32,
    pub(crate) y: i32,
}

pub(crate) fn build_semantic_map_nodes(rows: &[SemanticTreeRow]) -> Vec<SemanticMapNode> {
    let mut nodes = vec![SemanticMapNode {
        row_index: None,
        parent: None,
        x: 0,
        y: 0,
    }];
    let mut children: Vec<Vec<usize>> = vec![Vec::new()];
    let mut depth_stack: Vec<Option<usize>> = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        let depth = match row {
            SemanticTreeRow::Directory { depth, .. }
            | SemanticTreeRow::File { depth, .. }
            | SemanticTreeRow::Entity { depth, .. } => *depth,
            SemanticTreeRow::Status(_) => continue,
        };
        if depth_stack.len() <= depth {
            depth_stack.resize(depth.saturating_add(1), None);
        }
        depth_stack.truncate(depth.saturating_add(1));
        let parent = depth
            .checked_sub(1)
            .and_then(|parent_depth| depth_stack.get(parent_depth).copied().flatten());
        let node_index = nodes.len();
        let parent = parent.or(Some(0));
        nodes.push(SemanticMapNode {
            row_index: Some(row_index),
            parent,
            x: 0,
            y: ((depth as i32).saturating_add(1)).saturating_mul(4),
        });
        children.push(Vec::new());
        if let Some(parent) = parent {
            children[parent].push(node_index);
        }
        depth_stack[depth] = Some(node_index);
    }
    if nodes.len() == 1 {
        return Vec::new();
    }

    fn assign_tree_x(
        node_index: usize,
        nodes: &mut [SemanticMapNode],
        children: &[Vec<usize>],
        next_leaf: &mut i32,
    ) -> i32 {
        if children[node_index].is_empty() {
            let x = *next_leaf;
            *next_leaf = next_leaf.saturating_add(8);
            nodes[node_index].x = x;
            return x;
        }
        let mut first = None;
        let mut last = None;
        for child_index in &children[node_index] {
            let child_x = assign_tree_x(*child_index, nodes, children, next_leaf);
            first.get_or_insert(child_x);
            last = Some(child_x);
        }
        let x = (first.unwrap_or(0) + last.unwrap_or(0)) / 2;
        nodes[node_index].x = x;
        x
    }

    let mut next_leaf = 0;
    assign_tree_x(0, &mut nodes, &children, &mut next_leaf);
    nodes
}

pub(crate) fn semantic_map_screen_positions(
    nodes: &[SemanticMapNode],
    area: Rect,
    zoom: f32,
    pan_x: i32,
    pan_y: i32,
) -> Vec<(i32, i32)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let min_x = nodes.iter().map(|node| node.x).min().unwrap_or(0);
    let max_x = nodes.iter().map(|node| node.x).max().unwrap_or(min_x);
    let min_y = nodes.iter().map(|node| node.y).min().unwrap_or(0);
    let max_y = nodes.iter().map(|node| node.y).max().unwrap_or(min_y);
    let x_range = (max_x - min_x).max(1) as f32;
    let y_range = (max_y - min_y).max(1) as f32;
    let left = area.x.saturating_add(2) as i32;
    let top = area.y.saturating_add(1) as i32;
    let width = area.width.saturating_sub(5).max(1) as f32;
    let height = area.height.saturating_sub(5).max(1) as f32;
    let center_x = area.x as f32 + area.width as f32 / 2.0;
    let center_y = area.y as f32 + area.height as f32 / 2.0;
    let zoom = zoom.clamp(0.5, 4.0);

    nodes
        .iter()
        .map(|node| {
            let base_x = left as f32 + ((node.x - min_x) as f32 / x_range) * width;
            let base_y = top as f32 + ((node.y - min_y) as f32 / y_range) * height;
            let x = center_x + (base_x - center_x) * zoom + pan_x as f32;
            let y = center_y + (base_y - center_y) * zoom + pan_y as f32;
            (x.round() as i32, y.round() as i32)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SemanticViewport {
    pub(crate) total_rows: usize,
    pub(crate) visible_rows: usize,
    pub(crate) selected: usize,
    pub(crate) scroll_y: usize,
}

impl SemanticViewport {
    fn new(total_rows: usize, visible_rows: usize, selected: usize, scroll_y: usize) -> Self {
        let visible_rows = visible_rows.max(1);
        if total_rows == 0 {
            return Self {
                total_rows,
                visible_rows,
                selected: 0,
                scroll_y: 0,
            };
        }
        let selected = selected.min(total_rows.saturating_sub(1));
        let max_scroll = total_rows.saturating_sub(visible_rows);
        let scroll_y = scroll_y.min(max_scroll);
        Self {
            total_rows,
            visible_rows,
            selected,
            scroll_y,
        }
    }

    fn centered(total_rows: usize, visible_rows: usize, selected: usize) -> Self {
        let visible_rows = visible_rows.max(1);
        let selected = selected.min(total_rows.saturating_sub(1));
        let scroll_y = selected
            .saturating_sub(visible_rows / 2)
            .min(total_rows.saturating_sub(visible_rows));
        Self::new(total_rows, visible_rows, selected, scroll_y)
    }

    fn clamped(self) -> Self {
        let mut viewport = Self::new(
            self.total_rows,
            self.visible_rows,
            self.selected,
            self.scroll_y,
        );
        if viewport.selected < viewport.scroll_y {
            viewport.scroll_y = viewport.selected;
        } else if viewport.selected >= viewport.scroll_y.saturating_add(viewport.visible_rows) {
            viewport.scroll_y = viewport
                .selected
                .saturating_sub(viewport.visible_rows.saturating_sub(1));
        }
        viewport.scroll_y = viewport
            .scroll_y
            .min(viewport.total_rows.saturating_sub(viewport.visible_rows));
        viewport
    }

    fn row_at(&self, visible_row: usize) -> Option<usize> {
        if visible_row >= self.visible_rows {
            return None;
        }
        let row = self.scroll_y.saturating_add(visible_row);
        (row < self.total_rows).then_some(row)
    }
}

pub(crate) fn semantic_tree_body_area(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

impl App {
    pub(super) fn revalidate_semantic_diff(&mut self, route: DiffSource) {
        if route.requires_github_auth() && !self.ensure_github_auth() {
            return;
        }
        let query_key = QueryKey::semantic_diff(route.session_id());
        if !self.query_client.start_fetch(query_key) {
            return;
        }
        let sender = self.query_tx.clone();
        let forge = Arc::clone(&self.forge);
        thread::spawn(move || {
            let result = Self::load_semantic_diff(&route, forge.as_ref());
            let _ = sender.send(QueryEvent::SemanticDiff { route, result });
        });
    }

    pub(super) fn revalidate_selected_semantic_diff(&mut self) {
        match self.surface {
            AppSurface::Queue | AppSurface::DetailFull | AppSurface::Comments => {
                if let Some(item) = self.selected_work_item() {
                    self.revalidate_semantic_diff(item.route(self));
                }
            }
            AppSurface::CommitList => {
                if let (Some(route), Some(commit)) = (
                    self.commit_route.clone(),
                    self.commits.get(self.commit_selection).cloned(),
                ) {
                    self.revalidate_semantic_diff(DiffSource::Commit {
                        repo_path: route.repo_path,
                        sha: commit.sha,
                    });
                }
            }
            AppSurface::Diff => self.revalidate_semantic_diff(self.diff_source.clone()),
        }
    }

    fn load_semantic_diff(
        route: &DiffSource,
        forge: &dyn crate::forge::Forge,
    ) -> std::result::Result<SemanticDiff, String> {
        let (repo_path, file_changes) = match route {
            DiffSource::LocalWorktree(route) => {
                let git = GitBridge::open(Path::new(&route.repo_path))
                    .map_err(|error| format!("sem git open failed: {error}"))?;
                let scope = DiffScope::RefToWorking {
                    refspec: route.base_ref.clone(),
                };
                let files = git
                    .get_changed_files(&scope, &[])
                    .map_err(|error| format!("sem git diff failed: {error}"))?;
                (route.repo_path.clone(), files)
            }
            DiffSource::Commit { repo_path, sha } => {
                if let Some(repository) = repo_path.strip_prefix("forge:") {
                    let patch = forge.fetch_commit_patch(repository, sha)?;
                    return Ok(SemanticDiff::from_sem_changes(
                        compute_semantic_diff(
                            &file_changes_from_unified_patch(&patch),
                            &create_default_registry(),
                            None,
                            None,
                        )
                        .changes,
                    ));
                }
                let git = GitBridge::open(Path::new(repo_path))
                    .map_err(|error| format!("sem git open failed: {error}"))?;
                let scope = DiffScope::Commit { sha: sha.clone() };
                let files = git
                    .get_changed_files(&scope, &[])
                    .map_err(|error| format!("sem git diff failed: {error}"))?;
                (repo_path.clone(), files)
            }
            DiffSource::PullRequest { repository, number } => {
                let patch = forge.fetch_pull_request_patch(repository, *number)?;
                (String::new(), file_changes_from_unified_patch(&patch))
            }
        };
        if file_changes.is_empty() {
            return Ok(SemanticDiff::default());
        }
        let file_changes: Vec<FileChange> = file_changes
            .into_iter()
            .filter(should_semantic_parse_file_change)
            .take(80)
            .collect();
        if file_changes.is_empty() {
            return Ok(SemanticDiff::default());
        }
        let mut registry = create_default_registry();
        if !repo_path.is_empty() {
            let root = Path::new(&repo_path);
            registry.load_semrc(root);
            registry.load_gitattributes(root);
        }
        let result = compute_semantic_diff(&file_changes, &registry, None, None);
        Ok(SemanticDiff::from_sem_changes(result.changes))
    }

    pub(super) fn semantic_tree_rows(&self, route: &DiffSource) -> Vec<SemanticTreeRow> {
        let query = self
            .query_client
            .get(&QueryKey::semantic_diff(route.session_id()));
        let Some(diff) = self
            .semantic_diff_cache
            .get(route)
            .or_else(|| self.persisted_semantic_diff_cache.get(&route.session_id()))
        else {
            if query.is_fetching() {
                return vec![SemanticTreeRow::Status(
                    "loading semantic changes…".to_string(),
                )];
            }
            if query.status == QueryStatus::Error {
                return vec![SemanticTreeRow::Status(
                    query
                        .error
                        .unwrap_or_else(|| "semantic diff unavailable".to_string()),
                )];
            }
            return Vec::new();
        };
        let mut rows = Vec::new();
        let mut emitted_dirs = HashSet::new();
        for file in &diff.files {
            let parts: Vec<&str> = file
                .path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            let file_name = parts
                .last()
                .copied()
                .unwrap_or(file.path.as_str())
                .to_string();
            let mut collapsed_ancestor = false;
            if parts.len() > 1 {
                let mut prefix = String::new();
                for (depth, part) in parts[..parts.len() - 1].iter().enumerate() {
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(part);
                    let key = SemanticNodeKey::directory(route, &prefix);
                    let collapsed = !self.semantic_expanded.contains(&key);
                    if emitted_dirs.insert(prefix.clone()) {
                        rows.push(SemanticTreeRow::Directory {
                            key,
                            name: (*part).to_string(),
                            depth,
                            collapsed,
                        });
                    }
                    if collapsed {
                        collapsed_ancestor = true;
                        break;
                    }
                }
            }
            if collapsed_ancestor {
                continue;
            }
            let key = SemanticNodeKey::file(route, &file.path);
            let collapsed = !self.semantic_expanded.contains(&key);
            rows.push(SemanticTreeRow::File {
                key,
                path: file.path.clone(),
                name: file_name,
                depth: parts.len().saturating_sub(1),
                change_count: file.changes.len(),
                collapsed,
            });
            if collapsed {
                continue;
            }
            rows.extend(file.changes.iter().map(|change| SemanticTreeRow::Entity {
                path: file.path.clone(),
                depth: parts.len(),
                entity_type: change.entity_type.clone(),
                entity_name: change.entity_name.clone(),
                change_type: change.change_type.clone(),
                line: change.line,
                end_line: change.end_line,
            }));
        }
        if diff.truncated {
            rows.push(SemanticTreeRow::Status(
                "semantic list truncated".to_string(),
            ));
        }
        rows
    }

    pub(super) fn toggle_semantic_file(&mut self, key: SemanticNodeKey) {
        if !self.semantic_expanded.insert(key.clone()) {
            self.semantic_expanded.remove(&key);
        }
    }

    pub(super) fn seed_semantic_expansion(&mut self, route: &DiffSource) {
        let route_id = route.session_id();
        if !self.semantic_expansion_seeded.insert(route_id) {
            return;
        }
        let Some(diff) = self.semantic_diff_for_route(route).cloned() else {
            return;
        };
        let keys = self.semantic_default_expanded_keys(route, &diff);
        self.semantic_expanded.extend(keys);
    }

    pub(super) fn collapse_focused_semantic_branch(&mut self) {
        let Some(route) = self.current_semantic_route() else {
            return;
        };
        let route_id = route.session_id();
        self.semantic_expansion_seeded.insert(route_id);
        let Some(keys) = self.focused_semantic_branch_keys(&route) else {
            return;
        };
        for key in keys {
            self.semantic_expanded.remove(&key);
        }
        self.semantic_selection = self
            .semantic_selection
            .min(self.semantic_tree_rows(&route).len().saturating_sub(1));
    }

    pub(super) fn expand_focused_semantic_branch(&mut self) {
        let Some(route) = self.current_semantic_route() else {
            return;
        };
        let route_id = route.session_id();
        self.semantic_expansion_seeded.insert(route_id);
        let Some(keys) = self.focused_semantic_branch_keys(&route) else {
            return;
        };
        self.semantic_expanded.extend(keys);
        self.semantic_selection = self
            .semantic_selection
            .min(self.semantic_tree_rows(&route).len().saturating_sub(1));
    }

    fn focused_semantic_branch_keys(&self, route: &DiffSource) -> Option<Vec<SemanticNodeKey>> {
        let rows = self.semantic_tree_rows(route);
        let row = rows
            .get(self.semantic_selection.min(rows.len().saturating_sub(1)))?
            .clone();
        match row {
            SemanticTreeRow::Directory { key, .. } => {
                let prefix = key.path.strip_prefix("dir:")?;
                let diff = self.semantic_diff_for_route(route)?;
                Some(self.semantic_expandable_keys_under_directory(route, diff, prefix))
            }
            SemanticTreeRow::File { key, .. } => Some(vec![key]),
            SemanticTreeRow::Entity { path, .. } => Some(vec![SemanticNodeKey::file(route, &path)]),
            SemanticTreeRow::Status(_) => None,
        }
    }

    fn semantic_expandable_keys_under_directory(
        &self,
        route: &DiffSource,
        diff: &SemanticDiff,
        prefix: &str,
    ) -> Vec<SemanticNodeKey> {
        let mut keys = vec![SemanticNodeKey::directory(route, prefix)];
        let mut seen_dirs = HashSet::from([prefix.to_string()]);
        for file in &diff.files {
            if file.path != prefix && !file.path.starts_with(&format!("{prefix}/")) {
                continue;
            }
            let parts: Vec<&str> = file
                .path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() > 1 {
                let mut directory = String::new();
                for part in &parts[..parts.len() - 1] {
                    if !directory.is_empty() {
                        directory.push('/');
                    }
                    directory.push_str(part);
                    if directory.starts_with(prefix) && seen_dirs.insert(directory.clone()) {
                        keys.push(SemanticNodeKey::directory(route, &directory));
                    }
                }
            }
            keys.push(SemanticNodeKey::file(route, &file.path));
        }
        keys
    }

    pub(super) fn semantic_diff_for_route(&self, route: &DiffSource) -> Option<&SemanticDiff> {
        self.semantic_diff_cache
            .get(route)
            .or_else(|| self.persisted_semantic_diff_cache.get(&route.session_id()))
    }

    fn semantic_default_expanded_keys(
        &self,
        route: &DiffSource,
        diff: &SemanticDiff,
    ) -> Vec<SemanticNodeKey> {
        // Keep the map useful as a semantic graph: path scaffolding and all
        // changed files are open by default so entity nodes are immediately
        // reachable by mouse/keyboard.
        let mut keys = Self::semantic_directory_keys(route, diff);
        keys.extend(
            diff.files
                .iter()
                .map(|file| SemanticNodeKey::file(route, &file.path)),
        );
        keys
    }

    fn semantic_directory_keys(route: &DiffSource, diff: &SemanticDiff) -> Vec<SemanticNodeKey> {
        let mut seen = HashSet::new();
        let mut keys = Vec::new();
        for file in &diff.files {
            let parts: Vec<&str> = file
                .path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() <= 1 {
                continue;
            }
            let mut prefix = String::new();
            for part in &parts[..parts.len() - 1] {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                if seen.insert(prefix.clone()) {
                    keys.push(SemanticNodeKey::directory(route, &prefix));
                }
            }
        }
        keys
    }

    pub(super) fn current_semantic_route(&self) -> Option<DiffSource> {
        match self.surface {
            AppSurface::Queue | AppSurface::DetailFull => {
                self.selected_work_item().map(|item| item.route(self))
            }
            AppSurface::CommitList => {
                if let (Some((repository, _number)), Some(commit)) = (
                    &self.commit_pr_route,
                    self.commits.get(self.commit_selection),
                ) {
                    Some(DiffSource::Commit {
                        repo_path: format!("github:{repository}"),
                        sha: commit.sha.clone(),
                    })
                } else {
                    self.commit_route
                        .as_ref()
                        .zip(self.commits.get(self.commit_selection))
                        .map(|(route, commit)| DiffSource::Commit {
                            repo_path: route.repo_path.clone(),
                            sha: commit.sha.clone(),
                        })
                }
            }
            AppSurface::Comments | AppSurface::Diff => None,
        }
    }

    pub(super) fn semantic_viewport_for(
        &self,
        total_rows: usize,
        visible_rows: usize,
    ) -> SemanticViewport {
        SemanticViewport::new(
            total_rows,
            visible_rows,
            self.semantic_selection,
            self.semantic_scroll_y,
        )
        .clamped()
    }

    pub(super) fn set_semantic_viewport(&mut self, viewport: SemanticViewport) {
        self.semantic_visible_rows = viewport.visible_rows;
        self.semantic_selection = viewport.selected;
        self.semantic_scroll_y = viewport.scroll_y;
    }

    pub(super) fn open_semantic_path(
        &mut self,
        route: DiffSource,
        path: String,
        line: Option<usize>,
        end_line: Option<usize>,
        change_type: Option<String>,
    ) {
        let semantic_focus = SemanticFocusTarget {
            route: route.clone(),
            path: path.clone(),
            line,
            end_line,
            change_type: change_type.clone(),
        };
        match &route {
            DiffSource::LocalWorktree(_) => {
                self.document = self.document_for_route(&route);
                self.push_route(AppRoute::Diff(route.clone()));
                *self.diff_buffer.viewer_mut() = Default::default();
                self.pending_semantic_focus = Some(semantic_focus);
                self.apply_pending_semantic_focus(&route);
                self.revalidate_local_diff();
            }
            DiffSource::PullRequest { repository, number } => {
                self.pending_semantic_focus = Some(semantic_focus);
                self.push_route(AppRoute::Diff(route.clone()));
                self.document = self.document_for_route(&route);
                *self.diff_buffer.viewer_mut() = Default::default();
                self.apply_pending_semantic_focus(&route);
                self.revalidate_pull_request_diff(repository.clone(), *number);
            }
            DiffSource::Commit { repo_path, sha } => {
                self.pending_semantic_focus = Some(semantic_focus);
                self.push_route(AppRoute::Diff(route.clone()));
                self.document = parse_unified_diff("");
                *self.diff_buffer.viewer_mut() = Default::default();
                let sender = self.query_tx.clone();
                let repo_path = repo_path.clone();
                let sha = sha.clone();
                let forge = Arc::clone(&self.forge);
                thread::spawn(move || {
                    let result = Self::load_commit_diff(&repo_path, &sha, forge.as_ref());
                    let _ = sender.send(QueryEvent::CommitDiff {
                        repo_path,
                        sha,
                        result,
                    });
                });
            }
        }
        self.surface_scroll_y = 0;
    }

    pub(super) fn handle_semantic_tree_click(
        &mut self,
        route: DiffSource,
        area: Rect,
        column: u16,
        row: u16,
    ) -> bool {
        let body = semantic_tree_body_area(area);
        if area.width == 0
            || area.height == 0
            || column < area.x
            || column >= area.right()
            || !contains_point(body, column, row)
        {
            return false;
        }
        let rows = self.semantic_tree_rows(&route);
        let viewport = self.semantic_viewport_for(rows.len(), body.height as usize);
        self.set_semantic_viewport(viewport);
        if self.detail_tab == DetailTab::Semantic {
            let row_index = viewport
                .scroll_y
                .saturating_add(row.saturating_sub(body.y) as usize);
            let Some(tree_row) = rows.get(row_index).cloned() else {
                return false;
            };
            self.semantic_selection = row_index;
            match tree_row {
                SemanticTreeRow::Directory { key, .. } => self.toggle_semantic_file(key),
                SemanticTreeRow::File { key, .. } => self.toggle_semantic_file(key),
                SemanticTreeRow::Entity {
                    path,
                    line,
                    end_line,
                    change_type,
                    ..
                } => self.open_semantic_path(route, path, line, end_line, Some(change_type)),
                SemanticTreeRow::Status(_) => return false,
            }
            return true;
        }
        let nodes = build_semantic_map_nodes(&rows);
        let positions = semantic_map_screen_positions(
            &nodes,
            body,
            self.semantic_map_zoom,
            self.semantic_map_pan_x,
            self.semantic_map_pan_y,
        );
        let selected_node_index = nodes
            .iter()
            .position(|node| node.row_index == Some(viewport.selected))
            .unwrap_or(1.min(nodes.len().saturating_sub(1)));
        if nodes.get(selected_node_index).is_none() {
            return false;
        }
        let click_x = column as i32;
        let click_y = row as i32;
        let Some(row_index) = nodes
            .iter()
            .zip(positions.iter())
            .filter_map(|(node, (x, y))| {
                if *x < body.left() as i32
                    || *x >= body.right() as i32
                    || *y < body.top() as i32
                    || *y >= body.bottom() as i32
                {
                    return None;
                }
                let row_index = node.row_index?;
                let distance = x.abs_diff(click_x).saturating_add(y.abs_diff(click_y));
                (distance <= 3).then_some((distance, row_index))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, row_index)| row_index)
        else {
            return false;
        };
        let Some(tree_row) = rows.get(row_index).cloned() else {
            return false;
        };
        self.semantic_selection = row_index;
        match tree_row {
            SemanticTreeRow::Directory { key, .. } => self.toggle_semantic_file(key),
            SemanticTreeRow::File { key, .. } => self.toggle_semantic_file(key),
            SemanticTreeRow::Entity {
                path,
                line,
                end_line,
                change_type,
                ..
            } => self.open_semantic_path(route, path, line, end_line, Some(change_type)),
            SemanticTreeRow::Status(_) => return false,
        }
        true
    }

    pub(super) fn select_semantic_node_at(
        &mut self,
        route: &DiffSource,
        area: Rect,
        column: u16,
        row: u16,
    ) -> bool {
        let body = semantic_tree_body_area(area);
        if area.width == 0
            || area.height == 0
            || column < area.x
            || column >= area.right()
            || !contains_point(body, column, row)
        {
            return false;
        }
        let rows = self.semantic_tree_rows(route);
        let viewport = self.semantic_viewport_for(rows.len(), body.height as usize);
        self.set_semantic_viewport(viewport);
        if self.detail_tab == DetailTab::Semantic {
            let row_index = viewport
                .scroll_y
                .saturating_add(row.saturating_sub(body.y) as usize);
            if row_index < rows.len() {
                self.semantic_selection = row_index;
                return true;
            }
            return false;
        }
        let nodes = build_semantic_map_nodes(&rows);
        let positions = semantic_map_screen_positions(
            &nodes,
            body,
            self.semantic_map_zoom,
            self.semantic_map_pan_x,
            self.semantic_map_pan_y,
        );
        let click_x = column as i32;
        let click_y = row as i32;
        let Some(row_index) = nodes
            .iter()
            .zip(positions.iter())
            .filter_map(|(node, (x, y))| {
                let row_index = node.row_index?;
                let distance = x.abs_diff(click_x).saturating_add(y.abs_diff(click_y));
                (distance <= 3).then_some((distance, row_index))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, row_index)| row_index)
        else {
            return false;
        };
        self.semantic_selection = row_index.min(rows.len().saturating_sub(1));
        true
    }

    pub(super) fn move_semantic_selection(&mut self, delta: isize) {
        let route = self.current_semantic_route();
        let Some(route) = route else { return };
        let rows = self.semantic_tree_rows(&route);
        if rows.is_empty() {
            self.semantic_selection = 0;
            self.semantic_scroll_y = 0;
            return;
        }
        self.semantic_selection = self
            .semantic_selection
            .saturating_add_signed(delta)
            .min(rows.len().saturating_sub(1));
        let viewport = SemanticViewport::centered(
            rows.len(),
            self.semantic_visible_rows.max(1),
            self.semantic_selection,
        );
        self.set_semantic_viewport(viewport);
    }

    pub(super) fn move_semantic_selection_structural(&mut self, code: KeyCode) -> bool {
        let Some(route) = self.current_semantic_route() else {
            return false;
        };
        let rows = self.semantic_tree_rows(&route);
        if rows.is_empty() {
            return false;
        }
        let nodes = build_semantic_map_nodes(&rows);
        let Some(current_index) = nodes
            .iter()
            .position(|node| node.row_index == Some(self.semantic_selection.min(rows.len() - 1)))
        else {
            return false;
        };
        let current_x = nodes[current_index].x;
        let current_y = nodes[current_index].y;
        let nearest_on_level = |target_y: i32, direction: isize| {
            nodes
                .iter()
                .enumerate()
                .filter(|(index, node)| {
                    *index != current_index
                        && node.row_index.is_some()
                        && node.y == target_y
                        && if direction.is_negative() {
                            node.x < current_x
                        } else if direction.is_positive() {
                            node.x > current_x
                        } else {
                            true
                        }
                })
                .min_by_key(|(_, node)| node.x.abs_diff(current_x))
                .map(|(index, _)| index)
        };
        let nearest_vertical = |target_y: i32| {
            nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.row_index.is_some() && node.y == target_y)
                .min_by_key(|(_, node)| node.x.abs_diff(current_x))
                .map(|(index, _)| index)
        };
        let target_index = match code {
            KeyCode::Char('h') | KeyCode::Left => nearest_on_level(current_y, -1),
            KeyCode::Char('l') | KeyCode::Right => nearest_on_level(current_y, 1),
            KeyCode::Char('k') | KeyCode::Up => nearest_vertical(current_y.saturating_sub(4)),
            KeyCode::Char('j') | KeyCode::Down => nearest_vertical(current_y.saturating_add(4)),
            _ => None,
        };
        let Some(target_row) = target_index
            .and_then(|index| nodes.get(index))
            .and_then(|node| node.row_index)
        else {
            return false;
        };
        self.semantic_selection = target_row.min(rows.len().saturating_sub(1));
        let viewport = SemanticViewport::centered(
            rows.len(),
            self.semantic_visible_rows.max(1),
            self.semantic_selection,
        );
        self.set_semantic_viewport(viewport);
        true
    }

    pub(super) fn scroll_semantic_tree(&mut self, delta: isize) {
        self.move_semantic_selection(delta);
    }

    pub(super) fn zoom_semantic_map_at(&mut self, area: Rect, column: u16, row: u16, delta: isize) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let old_zoom = self.semantic_map_zoom.clamp(0.5, 4.0);
        let factor = if delta.is_positive() {
            1.18
        } else {
            1.0 / 1.18
        };
        let new_zoom = (old_zoom * factor).clamp(0.5, 4.0);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            self.semantic_map_zoom = new_zoom;
            return;
        }
        let center_x = area.x as f32 + area.width as f32 / 2.0;
        let center_y = area.y as f32 + area.height as f32 / 2.0;
        let cursor_x = column as f32;
        let cursor_y = row as f32;
        let ratio = new_zoom / old_zoom;
        let new_pan_x =
            cursor_x - center_x - (cursor_x - center_x - self.semantic_map_pan_x as f32) * ratio;
        let new_pan_y =
            cursor_y - center_y - (cursor_y - center_y - self.semantic_map_pan_y as f32) * ratio;
        self.semantic_map_zoom = new_zoom;
        self.semantic_map_pan_x = new_pan_x.round() as i32;
        self.semantic_map_pan_y = new_pan_y.round() as i32;
    }

    pub(super) fn pan_semantic_map(&mut self, dx: i32, dy: i32) {
        self.semantic_map_pan_x = self.semantic_map_pan_x.saturating_add(dx);
        self.semantic_map_pan_y = self.semantic_map_pan_y.saturating_add(dy);
    }

    pub(super) fn reset_semantic_map_view(&mut self) {
        self.semantic_map_zoom = 1.0;
        self.semantic_map_pan_x = 0;
        self.semantic_map_pan_y = 0;
    }

    pub(super) fn scroll_semantic_viewport_to(&mut self, row: u16, area: Rect) -> bool {
        let Some(route) = self.current_semantic_route() else {
            return false;
        };
        let body = semantic_tree_body_area(area);
        if body.height == 0 || row < body.y || row >= body.bottom() {
            return false;
        }
        let rows = self.semantic_tree_rows(&route);
        let viewport = self.semantic_viewport_for(rows.len(), body.height as usize);
        let scrollbar = VerticalScrollbar::new(
            body,
            viewport.total_rows,
            viewport.visible_rows,
            viewport.scroll_y,
        );
        if scrollbar.slider().max == 0 {
            self.set_semantic_viewport(viewport);
            return true;
        }
        let scroll_y = scrollbar.value_from_drag(row, self.semantic_scrollbar_drag_offset_virtual);
        let selected = scroll_y
            .saturating_add(viewport.selected.saturating_sub(viewport.scroll_y))
            .min(viewport.total_rows.saturating_sub(1));
        self.set_semantic_viewport(SemanticViewport::new(
            viewport.total_rows,
            viewport.visible_rows,
            selected,
            scroll_y,
        ));
        true
    }

    pub(super) fn semantic_scrollbar_drag_offset(&self, row: u16, area: Rect) -> usize {
        let Some(route) = self.current_semantic_route() else {
            return 0;
        };
        let body = semantic_tree_body_area(area);
        let rows = self.semantic_tree_rows(&route);
        let viewport = self.semantic_viewport_for(rows.len(), body.height as usize);
        VerticalScrollbar::new(
            body,
            viewport.total_rows,
            viewport.visible_rows,
            viewport.scroll_y,
        )
        .drag_offset_virtual(row)
    }

    pub(super) fn apply_pending_semantic_focus(&mut self, route: &DiffSource) {
        let Some(target) = self.pending_semantic_focus.clone() else {
            return;
        };
        if &target.route != route || self.document.files.is_empty() {
            return;
        }
        self.pending_semantic_focus = None;
        *self.diff_buffer.viewer_mut() = Default::default();
        self.focus_path_if_present(
            &target.path,
            target.line,
            target.end_line,
            target.change_type.as_deref(),
        );
    }

    pub(super) fn focus_semantic_path(
        &mut self,
        path: &str,
        line: Option<usize>,
        change_type: Option<&str>,
    ) {
        self.focus_path_if_present(path, line, None, change_type);
    }

    pub(super) fn open_selected_semantic_row(&mut self) -> bool {
        if !self.semantic_panel_active() {
            return false;
        }
        let Some(route) = self.selected_work_item().map(|item| item.route(self)) else {
            return false;
        };
        let rows = self.semantic_tree_rows(&route);
        let Some(row) = rows
            .get(self.semantic_selection.min(rows.len().saturating_sub(1)))
            .cloned()
        else {
            return false;
        };
        match row {
            SemanticTreeRow::Directory { key, .. } | SemanticTreeRow::File { key, .. } => {
                self.semantic_expansion_seeded.insert(route.session_id());
                match self.focused_semantic_branch_keys(&route) {
                    Some(keys) if keys.iter().all(|key| self.semantic_expanded.contains(key)) => {
                        for key in keys {
                            self.semantic_expanded.remove(&key);
                        }
                    }
                    Some(keys) => self.semantic_expanded.extend(keys),
                    None => {
                        self.semantic_expanded.insert(key);
                    }
                }
                true
            }
            SemanticTreeRow::Entity {
                path,
                line,
                end_line,
                change_type,
                ..
            } => {
                self.open_semantic_path(route, path, line, end_line, Some(change_type));
                true
            }
            SemanticTreeRow::Status(_) => false,
        }
    }

    pub(super) fn home_semantic_tree_start_y(&self, area: Rect, selected: &WorkItem) -> u16 {
        let content_y = area.y;
        let mut y = content_y.saturating_add(4);
        let pull_request = selected.pull_request(self);
        if pull_request.is_none() {
            y = y.saturating_add(2);
        }
        if let Some(pull_request) = pull_request {
            if !pull_request.checks.is_empty() {
                y = y.saturating_add(1);
                y = y.saturating_add(pull_request.checks.iter().take(8).count().div_ceil(2) as u16);
                y = y.saturating_add(1);
            }
        }
        y
    }

    fn focus_path_if_present(
        &mut self,
        path: &str,
        line: Option<usize>,
        end_line: Option<usize>,
        change_type: Option<&str>,
    ) {
        let Some(index) = self.document.files.iter().position(|file| {
            file.new_path == path
                || file.old_path.as_deref() == Some(path)
                || file.new_path.ends_with(path)
                || path.ends_with(file.new_path.as_str())
        }) else {
            return;
        };
        let mode = self.diff_buffer.viewer().viewport.mode;
        let rows = row_count_for_mode(&self.document, mode);
        // sem-core reports primary entity spans on the after side for added/
        // modified/renamed/moved/reordered changes and on the before side for
        // deleted changes.
        let use_old_side = matches!(change_type, Some("deleted"));
        if let Some(line) = line.and_then(|line| u32::try_from(line).ok()) {
            let end_line = end_line
                .and_then(|line| u32::try_from(line).ok())
                .unwrap_or(line)
                .max(line);
            let target = best_line_match(&self.document.files[index], line, end_line, use_old_side)
                .and_then(|(hunk_index, line_index)| {
                    self.document.line_row(mode, index, hunk_index, line_index)
                });
            if let Some(row) = target {
                self.focus_semantic_document_row(row);
                self.trigger_transient_focus(path.to_string(), row);
                return;
            }
        }
        self.jump_to_file(index, rows);
        self.trigger_transient_focus(path.to_string(), self.diff_buffer.viewer().cursor.row);
    }
}

/// Pick the hunk line that best represents a semantic entity span. Prefer a
/// changed line whose target-side line number intersects the semantic span;
/// otherwise use the entity start line and nearest target-side hunk line.
/// Returns `(hunk_index, line_index)`.
fn best_line_match(
    file: &FileDiff,
    entity_start_line: u32,
    entity_end_line: u32,
    use_old_side: bool,
) -> Option<(usize, usize)> {
    let entity_end_line = entity_end_line.max(entity_start_line);
    let mut changed_in_span: Option<(usize, usize)> = None;
    let mut exact: Option<(usize, usize)> = None;
    let mut after: Option<(u32, (usize, usize))> = None;
    let mut before: Option<(u32, (usize, usize))> = None;
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        for (line_index, diff_line) in hunk.lines.iter().enumerate() {
            let Some((line_no, is_changed)) = diff_line_side_line(diff_line, use_old_side) else {
                continue;
            };
            if is_changed && (entity_start_line..=entity_end_line).contains(&line_no) {
                changed_in_span = Some((hunk_index, line_index));
                break;
            }
            if line_no == entity_start_line {
                if exact.is_none() {
                    exact = Some((hunk_index, line_index));
                }
            } else if line_no > entity_start_line {
                let delta = line_no - entity_start_line;
                if after.is_none_or(|(d, _)| delta < d) {
                    after = Some((delta, (hunk_index, line_index)));
                }
            } else {
                let delta = entity_start_line - line_no;
                if before.is_none_or(|(d, _)| delta < d) {
                    before = Some((delta, (hunk_index, line_index)));
                }
            }
        }
        if changed_in_span.is_some() {
            return changed_in_span;
        }
        if exact.is_some() {
            return exact;
        }
    }
    changed_in_span
        .or(exact)
        .or_else(|| after.map(|(_, pos)| pos))
        .or_else(|| before.map(|(_, pos)| pos))
}

fn diff_line_side_line(diff_line: &DiffLine, use_old_side: bool) -> Option<(u32, bool)> {
    match diff_line {
        DiffLine::Context {
            old_line, new_line, ..
        } => Some((if use_old_side { *old_line } else { *new_line }, false)),
        DiffLine::Add { new_line, .. } if !use_old_side => Some((*new_line, true)),
        DiffLine::Delete { old_line, .. } if use_old_side => Some((*old_line, true)),
        _ => None,
    }
}

impl SemanticDiff {
    fn from_sem_changes(changes: Vec<sem_core::model::change::SemanticChange>) -> Self {
        let truncated = changes.len() > SEMANTIC_CHANGE_LIMIT;
        let mut file_order: Vec<String> = Vec::new();
        let mut files: HashMap<String, Vec<SemanticChange>> = HashMap::new();
        for change in changes.into_iter().take(SEMANTIC_CHANGE_LIMIT) {
            if change.entity_type.eq_ignore_ascii_case("chunk") {
                continue;
            }
            let (line, end_line) = semantic_line_span(&change);
            let path = if change.file_path.trim().is_empty() {
                "unknown".to_string()
            } else {
                change.file_path
            };
            if !files.contains_key(&path) {
                file_order.push(path.clone());
            }
            files.entry(path).or_default().push(SemanticChange {
                entity_type: normalize_semantic_label(&change.entity_type, "ENTITY"),
                entity_name: normalize_semantic_label(&change.entity_name, "module"),
                change_type: semantic_change_label(change.change_type).to_string(),
                line,
                end_line,
            });
        }
        Self {
            files: file_order
                .into_iter()
                .filter_map(|path| {
                    let changes = files.remove(&path)?;
                    Some(SemanticFile { path, changes })
                })
                .collect(),
            truncated,
        }
    }
}

fn semantic_line_span(
    change: &sem_core::model::change::SemanticChange,
) -> (Option<usize>, Option<usize>) {
    let start = (change.entity_line > 0).then_some(change.entity_line);
    let content = if matches!(change.change_type, ChangeType::Deleted) {
        change
            .before_content
            .as_deref()
            .or(change.after_content.as_deref())
    } else {
        change
            .after_content
            .as_deref()
            .or(change.before_content.as_deref())
    };
    let end = start.map(|line| {
        let line_count = content.map_or(1, |content| content.lines().count().max(1));
        line.saturating_add(line_count.saturating_sub(1))
    });
    (start, end)
}

fn semantic_change_label(change_type: ChangeType) -> &'static str {
    match change_type {
        ChangeType::Added => "added",
        ChangeType::Modified => "modified",
        ChangeType::Deleted => "deleted",
        ChangeType::Moved => "moved",
        ChangeType::Renamed => "renamed",
        ChangeType::Reordered => "reordered",
    }
}

fn file_changes_from_unified_patch(patch: &str) -> Vec<FileChange> {
    parse_unified_diff(patch)
        .files
        .into_iter()
        .filter_map(|file| {
            let before_content = collect_patch_side(&file, DiffSide::Left);
            let after_content = collect_patch_side(&file, DiffSide::Right);
            if before_content.is_none() && after_content.is_none() {
                return None;
            }
            let status = match (&before_content, &after_content, file.old_path.as_ref()) {
                (None, Some(_), _) => FileStatus::Added,
                (Some(_), None, _) => FileStatus::Deleted,
                (Some(_), Some(_), Some(old_path)) if old_path != &file.new_path => {
                    FileStatus::Renamed
                }
                _ => FileStatus::Modified,
            };
            Some(FileChange {
                file_path: file.new_path,
                status,
                old_file_path: file.old_path,
                before_content,
                after_content,
            })
        })
        .collect()
}

fn collect_patch_side(file: &FileDiff, side: DiffSide) -> Option<String> {
    let mut lines = Vec::new();
    for hunk in &file.hunks {
        for line in &hunk.lines {
            match (side, line) {
                (DiffSide::Left, DiffLine::Context { text, .. })
                | (DiffSide::Left, DiffLine::Delete { text, .. })
                | (DiffSide::Right, DiffLine::Context { text, .. })
                | (DiffSide::Right, DiffLine::Add { text, .. }) => lines.push(text.clone()),
                _ => {}
            }
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn should_semantic_parse_file_change(change: &FileChange) -> bool {
    let path = change.file_path.as_str();
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".lock")
        || lower.ends_with("lockfile")
        || lower.ends_with("package-lock.json")
        || lower.ends_with("pnpm-lock.yaml")
        || lower.ends_with("yarn.lock")
        || lower.ends_with("bun.lockb")
        || lower.ends_with("cargo.lock")
        || lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
        || lower.ends_with(".min.js")
        || lower.ends_with(".map")
    {
        return false;
    }
    let content_len = change
        .before_content
        .as_ref()
        .map_or(0, String::len)
        .max(change.after_content.as_ref().map_or(0, String::len));
    content_len <= 500_000
}

fn normalize_semantic_label(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazydiff_diffs::{DiffLine, FileDiff, Hunk};

    fn ctx(old_line: u32, new_line: u32) -> DiffLine {
        DiffLine::Context {
            old_line,
            new_line,
            text: String::new(),
            syntax_spans: Vec::new(),
        }
    }
    fn add(new_line: u32) -> DiffLine {
        DiffLine::Add {
            new_line,
            text: String::new(),
            syntax_spans: Vec::new(),
            inline_spans: Vec::new(),
        }
    }
    fn del(old_line: u32) -> DiffLine {
        DiffLine::Delete {
            old_line,
            text: String::new(),
            syntax_spans: Vec::new(),
            inline_spans: Vec::new(),
        }
    }

    fn file(lines: Vec<DiffLine>) -> FileDiff {
        FileDiff {
            old_path: Some("a".into()),
            new_path: "a".into(),
            hunks: vec![Hunk {
                old_start: 1,
                new_start: 1,
                header: String::new(),
                lines,
            }],
        }
    }

    #[test]
    fn semantic_viewport_centers_selection_when_possible() {
        let viewport = SemanticViewport::centered(100, 20, 50);
        assert_eq!(viewport.selected, 50);
        assert_eq!(viewport.scroll_y, 40);
        assert_eq!(viewport.row_at(10), Some(50));
    }

    #[test]
    fn semantic_viewport_clamps_selection_and_scroll_to_content() {
        let viewport = SemanticViewport::new(5, 20, 99, 99).clamped();
        assert_eq!(viewport.selected, 4);
        assert_eq!(viewport.scroll_y, 0);
        assert_eq!(viewport.row_at(4), Some(4));
        assert_eq!(viewport.row_at(5), None);
    }

    #[test]
    fn semantic_viewport_keeps_selected_row_visible_after_external_scroll() {
        let viewport = SemanticViewport::new(100, 10, 25, 0).clamped();
        assert_eq!(viewport.selected, 25);
        assert_eq!(viewport.scroll_y, 16);
        assert_eq!(viewport.row_at(9), Some(25));
    }

    #[test]
    fn best_line_match_prefers_exact_new_line_for_added_entity() {
        // Lines 1..3 context, line 4 added entity, line 5 context.
        let f = file(vec![ctx(1, 1), ctx(2, 2), ctx(3, 3), add(4), ctx(4, 5)]);
        let pos = best_line_match(&f, 4, 4, false).unwrap();
        // Index 3 is the Add at new_line=4.
        assert_eq!(pos, (0, 3));
    }

    #[test]
    fn best_line_match_uses_old_side_for_deleted_entity() {
        // Lines 1..2 context, line 3 deleted entity, then a context line
        // whose old side advanced to 4 because the delete shifted it.
        let f = file(vec![ctx(1, 1), ctx(2, 2), del(3), ctx(4, 3)]);
        let pos = best_line_match(&f, 3, 3, true).unwrap();
        // Index 2 is the Delete at old_line=3.
        assert_eq!(pos, (0, 2));
    }

    #[test]
    fn best_line_match_does_not_match_old_side_when_looking_at_new() {
        // The old-side line numbers happen to match `line`, but the entity
        // is on the after side; we must not anchor on them.
        let f = file(vec![ctx(10, 1), ctx(11, 2), ctx(12, 3)]);
        let pos = best_line_match(&f, 10, 10, false);
        // None of new_line values reach 10; nearest below is new_line=3
        // at index 2.
        assert_eq!(pos, Some((0, 2)));
    }

    #[test]
    fn best_line_match_returns_none_for_empty_file() {
        let f = file(Vec::new());
        assert_eq!(best_line_match(&f, 1, 1, false), None);
        assert_eq!(best_line_match(&f, 1, 1, true), None);
    }

    #[test]
    fn best_line_match_prefers_changed_line_inside_entity_span_over_declaration() {
        let f = file(vec![
            ctx(10, 10),
            ctx(11, 11),
            ctx(12, 12),
            add(13),
            ctx(13, 14),
        ]);

        assert_eq!(best_line_match(&f, 10, 14, false), Some((0, 3)));
    }
}
