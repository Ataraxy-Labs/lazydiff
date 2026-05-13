use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    io,
    path::Path,
    process::Command as ProcessCommand,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use color_eyre::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lazydiff_diffs::{
    add_pierre_highlights, parse_unified_diff, row_count_for_mode, DiffDocument, DiffLine,
    DiffLineKind, DiffLineRangeTarget, DiffLineTarget, DiffMode, DiffSide, DiffTheme,
    DiffViewState, DiffWidget, FileDiff, InlineDiffSpan, SliderState, SyntaxHighlightKind,
    SyntaxSpan, TextSelection, TextViewport,
};
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, ListState, StatefulWidget},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};

const SPLIT_TEXT_COLUMN: u16 = 8;
const BODY_TOP: u16 = 2;
const APP_TOP_PADDING: u16 = 1;
const STICKY_FILE_OVERLAY_ROWS: usize = 2;
const GITHUB_QUERY_CACHE_BUSTER: &str = "github-query-cache-v1";
const QUERY_CACHE_GC_INTERVAL: Duration = Duration::from_secs(60);
const QUERY_CACHE_MAX_AGE_SECS: i64 = 60 * 60;
const SPINNER_REDRAW_INTERVAL: Duration = Duration::from_millis(80);
const ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(16);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);
use crate::commands::{command_for_layer, Command, Layer};
use crate::components::{app_chrome::AppHeader, command_palette::CommandPalette};
use crate::design_system::{FinderPalette, HomePalette, SurfaceLayer, TextRole};
use crate::github::{
    fetch_commit_patch, fetch_pull_request_comments, fetch_pull_request_commits,
    fetch_pull_request_patch, github_auth_status, link_worktree_pr, list_branch_commits,
    list_worktrees, login_with_device_flow, GitCommit, GitHubAuthStatus, GitHubComment,
    GitHubPullRequest, GitHubQueue, GitHubQueueStatus, PrId, Worktree, WorktreeId,
};
use crate::persistence::{
    CommentModal, GitHubQueryClientState, PersistedGitHubQueryClient, PersistedPullRequestComments,
    PersistedPullRequestDiff, PersistedSemanticDiff, PersistedViewedState, ReviewItemKind,
    ReviewNote, ReviewSession, ReviewStore, ReviewUiState,
};
use crate::server_query::{
    LocalDiffResult, PullRequestDiffResult, QueryClient, QueryEvent, QueryKey, QueryResult,
    QueryStatus,
};
use crate::text::{
    body_preview_lines, markdown_preview_lines, relative_age, relative_unix_age, wrap_plain_text,
};
use crate::ui::{
    centered_rect, contains_point, draw_box, draw_horizontal_rule, draw_vertical_rule, fill_rect,
    render_home_rule, right_aligned_text, set_symbol, short_path, truncate, truncate_middle,
};

mod finder;
pub(crate) use finder::CommandResult;
use finder::*;
mod input;
mod modals;
mod queries;
mod semantic;
pub(crate) use semantic::{
    semantic_tree_body_area, SemanticChange, SemanticDiff, SemanticNodeKey, SemanticTreeRow,
    SemanticViewport,
};
mod surfaces;

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

fn app_content_area(area: Rect) -> Rect {
    let top = APP_TOP_PADDING.min(area.height);
    Rect::new(
        area.x,
        area.y.saturating_add(top),
        area.width,
        area.height.saturating_sub(top),
    )
}

pub(crate) struct App {
    path: String,
    project_label: Option<String>,
    document: DiffDocument,
    local_document: DiffDocument,
    state: DiffViewState,
    surface: AppSurface,
    history: NavHistory,
    diff_source: DiffSource,
    local_route: LocalWorktreeRoute,
    should_quit: bool,
    draw_count: u64,
    draw_total: Duration,
    draw_max: Duration,
    viewport_height: usize,
    surface_scroll_y: usize,
    detail_tab: DetailTab,
    /// Index of the currently selected comment in the Comments reader.
    /// j/k step between comments (not lines); selected comment renders
    /// with elevated bg + amber rail.
    comments_selection: usize,
    dragging_scrollbar: bool,
    selecting_text: bool,
    text_selection_dragged: bool,
    file_picker_open: bool,
    finder_kind: FinderKind,
    file_picker_selection: usize,
    file_picker_query: String,
    file_picker_preview_scroll: usize,
    home_selection: usize,
    home_selection_changed_at: Instant,
    theme_variant: crate::design_system::ThemeVariant,
    attempt_modal_open: bool,
    last_selection_mouse: Option<(usize, usize)>,
    scrollbar_drag_offset_virtual: usize,
    session: ReviewSession,
    store: ReviewStore,
    github: GitHubQueue,
    github_auth: GitHubAuthStatus,
    pending_terminal_flow: Option<TerminalFlow>,
    worktrees: Vec<Worktree>,
    branch_operation_status: Option<String>,
    commits: Vec<GitCommit>,
    commit_selection: usize,
    commit_route: Option<LocalWorktreeRoute>,
    commit_pr_route: Option<(String, u32)>,
    commit_status: Option<String>,
    pr_diff_cache: crate::bounded_map::BoundedMap<(String, u32), DiffDocument>,
    pr_patch_cache: crate::bounded_map::BoundedMap<(String, u32), String>,
    pr_comments_cache: crate::bounded_map::BoundedMap<(String, u32), Vec<GitHubComment>>,
    semantic_diff_cache: crate::bounded_map::BoundedMap<DiffSource, SemanticDiff>,
    persisted_semantic_diff_cache: crate::bounded_map::BoundedMap<String, SemanticDiff>,
    semantic_expanded: HashSet<SemanticNodeKey>,
    semantic_expansion_seeded: HashSet<String>,
    semantic_selection: usize,
    semantic_scroll_y: usize,
    semantic_visible_rows: usize,
    semantic_dragging_scrollbar: bool,
    semantic_scrollbar_drag_offset_virtual: usize,
    pending_semantic_focus: Option<SemanticFocusTarget>,
    review_sidebar_visible: bool,
    review_sidebar_focus: bool,
    review_sidebar_selection: usize,
    review_sidebar_scroll_y: usize,
    review_sidebar_expanded: HashSet<ReviewTreeKey>,
    review_sidebar_seeded_routes: HashSet<String>,
    viewed_files: HashSet<String>,
    viewed_entities: HashSet<String>,
    viewed_session_id: String,
    body_preview_cache: crate::bounded_map::BoundedMap<BodyPreviewCacheKey, Vec<Line<'static>>>,
    query_tx: Sender<QueryEvent>,
    query_rx: Receiver<QueryEvent>,
    query_client: QueryClient,
    last_query_gc_at: Instant,
    comment_modal: Option<CommentModal>,
    thread_modal: Option<DiffLineTarget>,
    thread_selection: usize,
    thread_scroll_y: usize,
    transient_focus: Option<TransientFocus>,
}

/// A bright row-flash that fades out shortly after a semantic
/// navigation lands in the diff view. Helps the reader spot exactly
/// where they jumped to when the cursor highlight alone is too subtle.
#[derive(Clone, Debug)]
pub(crate) struct TransientFocus {
    pub(crate) path: String,
    pub(crate) row: usize,
    pub(crate) started_at: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticFocusTarget {
    pub(crate) route: DiffSource,
    pub(crate) path: String,
    pub(crate) line: Option<usize>,
    pub(crate) change_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ReviewTreeKey(String);

impl ReviewTreeKey {
    fn directory(path: &str) -> Self {
        Self(format!("dir:{path}"))
    }

    fn file(path: &str) -> Self {
        Self(format!("file:{path}"))
    }
}

#[derive(Clone, Debug)]
enum ReviewTreeRow {
    Directory {
        key: ReviewTreeKey,
        path: String,
        name: String,
        depth: usize,
        collapsed: bool,
        file_count: usize,
    },
    File {
        key: ReviewTreeKey,
        file_index: usize,
        path: String,
        name: String,
        depth: usize,
        collapsed: bool,
        semantic_count: usize,
    },
    Entity {
        key: String,
        file_index: usize,
        path: String,
        depth: usize,
        entity_type: String,
        entity_name: String,
        change_type: String,
        line: Option<usize>,
    },
}

const TRANSIENT_FOCUS_DURATION: Duration = Duration::from_millis(900);
const TRANSIENT_FOCUS_TICK: Duration = Duration::from_millis(60);

impl App {
    pub(crate) fn new(path: String, bytes: usize, document: DiffDocument) -> Self {
        Self::new_with_initial_route(path, bytes, document, None, true)
    }

    pub(crate) fn new_local_diff(
        path: String,
        bytes: usize,
        document: DiffDocument,
        repo_path: String,
        branch: String,
        base_ref: String,
    ) -> Self {
        let route = LocalWorktreeRoute {
            repo_path,
            branch,
            base_ref,
        };
        Self::new_with_initial_route(
            path,
            bytes,
            document,
            Some(AppRoute::Diff(DiffSource::LocalWorktree(route))),
            false,
        )
    }

    pub(crate) fn new_commit_diff(
        path: String,
        bytes: usize,
        document: DiffDocument,
        repo_path: String,
        sha: String,
    ) -> Self {
        Self::new_with_initial_route(
            path,
            bytes,
            document,
            Some(AppRoute::Diff(DiffSource::Commit { repo_path, sha })),
            false,
        )
    }

    fn new_with_initial_route(
        path: String,
        bytes: usize,
        document: DiffDocument,
        initial_route: Option<AppRoute>,
        refresh_local_diff: bool,
    ) -> Self {
        eprintln!(
            "[lazydiff] initial-diff={path} bytes={bytes} files={}",
            document.files.len()
        );
        let store = ReviewStore::open_default().unwrap_or_else(|error| {
            eprintln!("[lazydiff] sqlite disabled: {error}");
            ReviewStore::memory_only()
        });
        let local_route = match &initial_route {
            Some(AppRoute::Diff(DiffSource::LocalWorktree(route))) => route.clone(),
            _ => Self::initial_local_route(),
        };
        let initial_route = initial_route.unwrap_or(AppRoute::Queue);
        let initial_source = match &initial_route {
            AppRoute::Detail(source) | AppRoute::Comments(source) | AppRoute::Diff(source) => {
                source.clone()
            }
            AppRoute::Queue | AppRoute::CommitList => {
                DiffSource::LocalWorktree(local_route.clone())
            }
        };
        let session = Self::load_session_for_route(&store, &initial_source, &document);
        let (query_tx, query_rx) = mpsc::channel();
        let persisted_queries = store.restore_github_query_client();
        let github = persisted_queries
            .as_ref()
            .and_then(|client| client.client_state.queue.clone())
            .unwrap_or_else(GitHubQueue::empty_loading);
        let github_auth = github_auth_status();
        let mut query_client = QueryClient::default();
        if let Some(cached_at) = github.cached_at {
            query_client.hydrate_success(QueryKey::GitHubQueue, cached_at);
        }
        let project_label = Self::project_label_from_env();
        // Read the persisted theme synchronously so the first paint
        // already uses the user's preferred variant — no warm→cool
        // flicker once the async revalidate finishes.
        let theme_variant = store
            .restore_theme_variant()
            .unwrap_or(crate::design_system::ThemeVariant::Warm);
        let mut app = Self {
            path,
            project_label,
            local_document: document.clone(),
            document,
            state: DiffViewState::default(),
            surface: AppSurface::Queue,
            history: NavHistory::new(initial_route.clone()),
            diff_source: initial_source,
            local_route,
            should_quit: false,
            draw_count: 0,
            draw_total: Duration::ZERO,
            draw_max: Duration::ZERO,
            viewport_height: 1,
            surface_scroll_y: 0,
            detail_tab: DetailTab::Semantic,
            comments_selection: 0,
            dragging_scrollbar: false,
            selecting_text: false,
            text_selection_dragged: false,
            file_picker_open: false,
            finder_kind: FinderKind::Files,
            file_picker_selection: 0,
            file_picker_query: String::new(),
            file_picker_preview_scroll: 0,
            home_selection: 0,
            home_selection_changed_at: Instant::now(),
            theme_variant,
            attempt_modal_open: false,
            last_selection_mouse: None,
            scrollbar_drag_offset_virtual: 0,
            session,
            store,
            github,
            github_auth,
            pending_terminal_flow: None,
            worktrees: Vec::new(),
            branch_operation_status: None,
            commits: Vec::new(),
            commit_selection: 0,
            commit_route: None,
            commit_pr_route: None,
            commit_status: None,
            pr_diff_cache: crate::bounded_map::BoundedMap::new(16),
            pr_patch_cache: crate::bounded_map::BoundedMap::new(16),
            pr_comments_cache: crate::bounded_map::BoundedMap::new(32),
            semantic_diff_cache: crate::bounded_map::BoundedMap::new(32),
            persisted_semantic_diff_cache: crate::bounded_map::BoundedMap::new(32),
            semantic_expanded: HashSet::new(),
            semantic_expansion_seeded: HashSet::new(),
            semantic_selection: 0,
            semantic_scroll_y: 0,
            semantic_visible_rows: 1,
            semantic_dragging_scrollbar: false,
            semantic_scrollbar_drag_offset_virtual: 0,
            pending_semantic_focus: None,
            review_sidebar_visible: true,
            review_sidebar_focus: false,
            review_sidebar_selection: 0,
            review_sidebar_scroll_y: 0,
            review_sidebar_expanded: HashSet::new(),
            review_sidebar_seeded_routes: HashSet::new(),
            viewed_files: HashSet::new(),
            viewed_entities: HashSet::new(),
            viewed_session_id: String::new(),
            body_preview_cache: crate::bounded_map::BoundedMap::new(128),
            query_tx,
            query_rx,
            query_client,
            last_query_gc_at: Instant::now(),
            comment_modal: None,
            thread_modal: None,
            thread_selection: 0,
            thread_scroll_y: 0,
            transient_focus: None,
        };
        if let Some(persisted_queries) = persisted_queries {
            app.hydrate_persisted_query_client(persisted_queries);
        }
        app.sync_viewed_state_for_session();
        app.apply_route(initial_route);
        app.restore_view_state_for_current_route();
        app.revalidate_project_label();
        if refresh_local_diff {
            app.revalidate_local_diff();
        }
        app.revalidate_worktrees();
        app.revalidate_semantic_diff(app.local_route());
        app.revalidate_queue();
        app
    }

    pub(crate) fn run(mut self, terminal: &mut Tui) -> Result<()> {
        let mut needs_redraw = true;
        let mut last_spinner_redraw = Instant::now();
        while !self.should_quit {
            if self.drain_query_events() {
                needs_redraw = true;
            }
            if self.query_client.is_fetching()
                && last_spinner_redraw.elapsed() >= SPINNER_REDRAW_INTERVAL
            {
                needs_redraw = true;
                last_spinner_redraw = Instant::now();
            }
            if self.tick_transient_focus() {
                needs_redraw = true;
            }
            if needs_redraw {
                let start = Instant::now();
                terminal.draw(|frame| self.render(frame))?;
                let elapsed = start.elapsed();
                self.record_draw(elapsed);
                needs_redraw = false;
            }

            let poll_interval = if self.query_client.is_fetching()
                || self.dragging_scrollbar
                || self.selecting_text
            {
                ACTIVE_POLL_INTERVAL
            } else if self.transient_focus.is_some() {
                TRANSIENT_FOCUS_TICK
            } else {
                IDLE_POLL_INTERVAL
            };
            if event::poll(poll_interval)? {
                // Drain every event currently queued so a burst of
                // scroll/mouse events collapses into a single redraw.
                // This is the key to making fast wheel/trackpad
                // gestures feel responsive: without it, each event
                // takes one full draw cycle to be reflected and the
                // queue keeps moving the view after the user stops.
                let mut processed = 0usize;
                loop {
                    match event::read()? {
                        Event::Key(key) => {
                            self.handle_key(key);
                            if let Some(flow) = self.pending_terminal_flow.take() {
                                self.run_terminal_flow(terminal, flow)?;
                            }
                            needs_redraw = true;
                        }
                        Event::Mouse(mouse) => {
                            // Plain cursor movement (no button held)
                            // would otherwise force a redraw on every
                            // pixel of motion — the app has no handler
                            // for it, so swallow these cheaply.
                            if !matches!(mouse.kind, MouseEventKind::Moved) {
                                let size = terminal.size()?;
                                self.handle_mouse(mouse, size.width, size.height);
                                needs_redraw = true;
                            }
                        }
                        Event::Resize(_, _) => needs_redraw = true,
                        _ => {}
                    }
                    processed += 1;
                    // Cap to avoid starving the renderer if a flood
                    // of events arrives faster than we can handle.
                    // Keep this high enough that held page-scroll keys
                    // collapse into the newest scroll position instead
                    // of painting dozens of stale intermediate frames.
                    if processed >= 256 || !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
        }
        self.persist_view_state_for_current_route();
        Ok(())
    }

    fn run_terminal_flow(&mut self, terminal: &mut Tui, flow: TerminalFlow) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;

        let result = match flow {
            TerminalFlow::GitHubLogin => login_with_device_flow().map(|user| user.login),
        };

        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        enable_raw_mode()?;
        terminal.clear()?;

        self.github_auth = github_auth_status();
        match result {
            Ok(login) if self.github_auth.can_load_github() => {
                self.github.viewer = Some(login);
                self.github.status = GitHubQueueStatus::Loading;
                self.revalidate_queue();
            }
            Ok(_) => {
                self.github.status = GitHubQueueStatus::MissingToken;
            }
            Err(error) => {
                self.github.status = GitHubQueueStatus::Error(error);
            }
        }
        Ok(())
    }

    fn restore_view_state_for_current_route(&mut self) {
        let Some(saved) = self.store.restore_ui_state(&self.diff_source.session_id()) else {
            return;
        };
        let rows = row_count_for_mode(&self.document, saved.diff_mode);
        self.state.mode = saved.diff_mode;
        self.state.selected_row = saved.selected_row.min(rows.saturating_sub(1));
        self.state.scroll_y = saved
            .scroll_y
            .min(rows.saturating_sub(self.viewport_height));
        self.state.selected_side = saved.selected_side;
    }

    fn persist_view_state_for_current_route(&self) {
        self.store.persist_ui_state(
            &self.diff_source.session_id(),
            ReviewUiState {
                selected_row: self.state.selected_row,
                scroll_y: self.state.scroll_y,
                selected_side: self.state.selected_side,
                diff_mode: self.state.mode,
            },
        );
    }

    fn render(&mut self, frame: &mut Frame) {
        match self.surface {
            AppSurface::Queue => {
                self.render_home(frame);
                self.render_global_overlays(frame);
                return;
            }
            AppSurface::CommitList => {
                self.render_commit_list(frame);
                self.render_global_overlays(frame);
                return;
            }
            AppSurface::DetailFull => {
                self.render_detail_full(frame);
                self.render_global_overlays(frame);
                return;
            }
            AppSurface::Comments => {
                self.render_comments_surface(frame);
                self.render_global_overlays(frame);
                return;
            }
            AppSurface::Diff if self.should_render_diff_placeholder() => {
                self.render_diff_placeholder(frame);
                self.render_global_overlays(frame);
                return;
            }
            AppSurface::Diff => {}
        }
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let [header, divider, body, comment_preview, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(area);
        let (sidebar, sidebar_divider, diff_body) = self.diff_sidebar_layout(body);
        self.viewport_height = diff_body.height as usize;
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        self.render_diff_header(frame, header, palette);
        draw_horizontal_rule(
            frame.buffer_mut(),
            divider.y,
            divider.x,
            divider.right(),
            palette.rule,
            palette.bg,
        );
        StatefulWidget::render(
            DiffWidget::new(&self.document).theme(palette.theme.diff_theme()),
            diff_body,
            frame.buffer_mut(),
            &mut self.state,
        );
        if let Some(sidebar) = sidebar {
            self.render_review_sidebar(frame, sidebar, palette);
        }
        if let Some(sidebar_divider) = sidebar_divider {
            draw_vertical_rule(
                frame.buffer_mut(),
                sidebar_divider.x,
                sidebar_divider.y,
                sidebar_divider.bottom(),
                palette.rule,
                palette.bg,
            );
        }
        self.render_sticky_file_overlay(frame, diff_body);
        self.render_note_gutter_markers(frame, diff_body);
        // Drawn last so it stays visible on top of the sticky file
        // overlay and gutter markers.
        self.render_transient_focus(frame, diff_body, palette);
        self.render_comment_preview(frame, comment_preview);
        self.render_footer(frame, footer);
        self.render_global_overlays(frame);
    }

    fn render_global_overlays(&mut self, frame: &mut Frame) {
        if let Some(target) = self.thread_modal.clone() {
            self.render_thread_modal(frame, &target);
        }
        if let Some(modal) = &self.comment_modal {
            self.render_comment_modal(frame, modal);
        }
        if self.attempt_modal_open {
            self.render_attempts_modal(frame);
        }
        if self.file_picker_open {
            self.render_file_picker(frame);
        }
    }

    fn file_picker_list_start(&self, list_height: usize, filtered_len: usize) -> usize {
        if list_height == 0 || filtered_len <= list_height {
            return 0;
        }
        self.file_picker_selection
            .saturating_sub(list_height / 2)
            .min(filtered_len.saturating_sub(list_height))
    }

    fn record_draw(&mut self, elapsed: Duration) {
        self.draw_count += 1;
        self.draw_total += elapsed;
        self.draw_max = self.draw_max.max(elapsed);
    }

    fn handle_key(&mut self, key: KeyEvent) {
        let rows = row_count_for_mode(&self.document, self.state.mode);
        if self.handle_modal_key(key.code) {
            return;
        }
        if self.attempt_modal_open {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.attempt_modal_open = false
                }
                _ => {}
            }
            return;
        }
        if self.file_picker_open {
            self.handle_file_picker_key(key.code, rows);
            return;
        }
        if self.surface == AppSurface::Diff {
            match key.code {
                KeyCode::Char('s') => {
                    self.review_sidebar_visible = !self.review_sidebar_visible;
                    self.review_sidebar_focus = self.review_sidebar_visible;
                    return;
                }
                KeyCode::Tab if self.review_sidebar_visible => {
                    self.review_sidebar_focus = !self.review_sidebar_focus;
                    self.sync_review_sidebar_selection_to_current_file();
                    return;
                }
                _ => {}
            }
            if self.review_sidebar_focus && self.review_sidebar_visible {
                self.handle_review_sidebar_key(key.code, rows);
                return;
            }
        }
        if matches!(self.surface, AppSurface::Queue | AppSurface::DetailFull) {
            match key.code {
                KeyCode::Enter
                    if self.surface == AppSurface::DetailFull
                        && self.detail_tab == DetailTab::Semantic =>
                {
                    if self.open_selected_semantic_row() {
                        return;
                    }
                }
                KeyCode::Left => {
                    self.set_detail_tab(DetailTab::Semantic);
                    return;
                }
                KeyCode::Right => {
                    self.set_detail_tab(DetailTab::Description);
                    return;
                }
                KeyCode::Char('1') => {
                    self.set_detail_tab(DetailTab::Semantic);
                    return;
                }
                KeyCode::Char('2') => {
                    self.set_detail_tab(DetailTab::Description);
                    return;
                }
                KeyCode::Char('[') if self.detail_tab == DetailTab::Semantic => {
                    self.collapse_focused_semantic_branch();
                    return;
                }
                KeyCode::Char(']') if self.detail_tab == DetailTab::Semantic => {
                    self.expand_focused_semantic_branch();
                    return;
                }
                _ => {}
            }
        }
        // Ctrl+d / Ctrl+u: half-page scroll across every scrollable surface.
        // Reserved upstream in `command_for_layer` so per-layer `d`/`u`
        // bindings can't shadow them.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => {
                    self.page_surface_half(1, rows);
                    return;
                }
                KeyCode::Char('u') => {
                    self.page_surface_half(-1, rows);
                    return;
                }
                _ => {}
            }
        }
        if let Some(command) = self.command_for_key(key) {
            self.execute_command(command, rows);
            return;
        }
        if self.surface == AppSurface::Diff {
            self.handle_plain_key(key.code, rows);
        }
    }

    /// Half-page scroll dispatcher used by ctrl-d / ctrl-u from any surface.
    fn page_surface_half(&mut self, direction: isize, rows: usize) {
        match self.surface {
            AppSurface::Queue => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_home_selection(direction.saturating_mul(half));
            }
            AppSurface::CommitList => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_commit_selection(direction.saturating_mul(half));
            }
            AppSurface::Comments => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_comments_selection(direction.saturating_mul(half));
            }
            AppSurface::DetailFull => {
                if self.detail_tab == DetailTab::Semantic {
                    let half = (self.semantic_visible_rows.max(2) / 2).max(1) as isize;
                    self.move_semantic_selection(direction.saturating_mul(half));
                } else {
                    let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                    self.surface_scroll_y = self
                        .surface_scroll_y
                        .saturating_add_signed(direction.saturating_mul(half));
                }
            }
            AppSurface::Diff => self.move_active_half_page(direction, rows),
        }
    }

    fn handle_plain_key(&mut self, code: KeyCode, rows: usize) {
        match code {
            KeyCode::Esc => {
                if self.state.selection.is_some() {
                    self.state.clear_mouse_selection();
                } else {
                    self.go_back();
                }
            }
            KeyCode::Char('a') => self.open_review_composer(ReviewItemKind::Question),
            KeyCode::Char('i') => self.open_review_composer(ReviewItemKind::Instruction),
            KeyCode::Char('n') | KeyCode::Char('c') => {
                self.open_review_composer(ReviewItemKind::Note)
            }
            KeyCode::Enter => self.open_thread_modal(),
            KeyCode::Char('f') => {
                self.file_picker_open = true;
                self.finder_kind = FinderKind::Files;
                self.file_picker_selection = self.current_file_index().unwrap_or(0);
                self.file_picker_query.clear();
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Char(':') => {
                self.file_picker_open = true;
                self.finder_kind = FinderKind::Inbox;
                self.file_picker_selection = 0;
                self.file_picker_query.clear();
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Char('/') => {
                self.file_picker_open = true;
                self.finder_kind = FinderKind::Text;
                self.file_picker_query.clear();
                self.file_picker_selection = 0;
                self.file_picker_preview_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_active_relative(1, rows),
            KeyCode::Char('k') | KeyCode::Up => self.move_active_relative(-1, rows),
            KeyCode::PageDown => self.scroll_relative(self.viewport_height as isize, rows),
            KeyCode::PageUp => self.scroll_relative(-(self.viewport_height as isize), rows),
            KeyCode::Char('g') => {
                self.state.scroll_y = 0;
            }
            KeyCode::Char('G') => {
                self.state.scroll_y = rows.saturating_sub(self.viewport_height);
            }
            KeyCode::Char('v') => self.toggle_visual_selection(rows),
            KeyCode::Char(' ') => self.toggle_current_file_viewed(),
            KeyCode::Char('m') => {
                self.state.mode = self.state.mode.toggle();
                self.state.scroll_y = 0;
                self.state.selected_row = 0;
                self.state.clear_mouse_selection();
            }
            KeyCode::Char(']') => self.jump_relative_file(1, rows),
            KeyCode::Char('[') => self.jump_relative_file(-1, rows),
            KeyCode::Char('N') => self.jump_relative_hunk(1, rows),
            KeyCode::Char('p') => self.jump_relative_hunk(-1, rows),
            KeyCode::Char('A') => self.attempt_modal_open = true,
            KeyCode::Left => self.state.selected_side = DiffSide::Left,
            KeyCode::Right => self.state.selected_side = DiffSide::Right,
            _ => {}
        }
    }

    fn command_for_key(&self, key: KeyEvent) -> Option<Command> {
        self.active_layers()
            .into_iter()
            .find_map(|layer| command_for_layer(layer, key))
    }

    fn active_layers(&self) -> Vec<Layer> {
        if self.file_picker_open {
            return vec![Layer::FilePicker, Layer::Global];
        }
        let surface_layer = match self.surface {
            AppSurface::Queue => Layer::Queue,
            AppSurface::CommitList => Layer::CommitList,
            AppSurface::DetailFull => Layer::DetailFull,
            AppSurface::Comments => Layer::Comments,
            AppSurface::Diff => Layer::Diff,
        };
        vec![surface_layer, Layer::Global]
    }

    fn execute_command(&mut self, command: Command, rows: usize) {
        match command {
            Command::Quit => self.should_quit = true,
            Command::Back => self.go_back(),
            Command::MoveDown => self.move_surface_down(rows),
            Command::MoveUp => self.move_surface_up(rows),
            Command::PageDown => self.page_surface(1, rows),
            Command::PageUp => self.page_surface(-1, rows),
            Command::Refresh => {
                self.revalidate_local_diff();
                self.revalidate_worktrees();
                self.revalidate_selected_semantic_diff();
                self.revalidate_queue();
            }
            Command::LoginGitHub => self.pending_terminal_flow = Some(TerminalFlow::GitHubLogin),
            Command::PullBranch => self.run_selected_branch_operation(BranchOperation::Pull),
            Command::PushBranch => self.run_selected_branch_operation(BranchOperation::Push),
            Command::FetchBranch => self.run_selected_branch_operation(BranchOperation::Fetch),
            Command::ForcePushBranch => {
                self.run_selected_branch_operation(BranchOperation::ForcePush)
            }
            Command::OpenCommitList => self.open_commit_list(),
            Command::OpenSelectedCommit => self.open_selected_commit_diff(),
            Command::OpenDetail => {
                if let Some(item) = self.selected_work_item() {
                    self.push_route(AppRoute::Detail(item.route(self)));
                }
                self.surface_scroll_y = 0;
            }
            Command::OpenComments => {
                if let Some(item) = self.selected_work_item() {
                    self.push_route(AppRoute::Comments(item.route(self)));
                }
                self.surface_scroll_y = 0;
                self.comments_selection = 0;
                self.revalidate_selected_comments();
            }
            Command::OpenDiff => self.open_selected_diff(),
            Command::OpenInBrowser => self.open_selected_in_browser(),
            Command::OpenCommandPalette => self.open_root_palette(),
            Command::OpenFileSearch => self.open_file_search(),
            Command::OpenTextSearch => self.open_text_search(),
            Command::OpenInbox => self.open_inbox(rows),
            Command::OpenThread => self.open_thread_modal(),
            Command::NewQuestion => self.open_review_composer(ReviewItemKind::Question),
            Command::NewInstruction => self.open_review_composer(ReviewItemKind::Instruction),
            Command::NewNote => self.open_review_composer(ReviewItemKind::Note),
            Command::ToggleDiffMode => {
                self.state.mode = self.state.mode.toggle();
                self.state.scroll_y = 0;
                self.state.selected_row = 0;
                self.state.clear_mouse_selection();
            }
            Command::JumpFirst => self.state.scroll_y = 0,
            Command::JumpLast => self.state.scroll_y = rows.saturating_sub(self.viewport_height),
            Command::PreviousFile => self.jump_relative_file(-1, rows),
            Command::NextFile => self.jump_relative_file(1, rows),
            Command::PreviousHunk => self.jump_relative_hunk(-1, rows),
            Command::NextHunk => self.jump_relative_hunk(1, rows),
            Command::ShowAttempts => self.attempt_modal_open = true,
            Command::SelectLeft => self.state.selected_side = DiffSide::Left,
            Command::SelectRight => self.state.selected_side = DiffSide::Right,
            Command::ToggleTheme => self.toggle_theme_variant(),
        }
    }

    fn go_back(&mut self) {
        if self.state.selection.is_some() && self.surface == AppSurface::Diff {
            self.state.clear_mouse_selection();
            return;
        }
        if !self.history.can_go_back() {
            self.should_quit = true;
            return;
        }
        let route = self.history.go(-1).clone();
        self.apply_route(route);
    }

    fn push_route(&mut self, route: AppRoute) {
        self.history.push(route.clone());
        self.apply_route(route);
    }

    fn replace_route(&mut self, route: AppRoute) {
        self.history.replace(route.clone());
        self.apply_route(route);
    }

    fn apply_route(&mut self, route: AppRoute) {
        self.surface_scroll_y = 0;
        match route {
            AppRoute::Queue => self.surface = AppSurface::Queue,
            AppRoute::CommitList => self.surface = AppSurface::CommitList,
            AppRoute::Detail(source) => {
                if source.requires_github_auth() && !self.ensure_github_auth() {
                    self.surface = AppSurface::Queue;
                    return;
                }
                self.activate_route(source);
                self.surface = AppSurface::DetailFull;
            }
            AppRoute::Comments(source) => {
                if source.requires_github_auth() && !self.ensure_github_auth() {
                    self.surface = AppSurface::Queue;
                    return;
                }
                self.activate_route(source);
                self.surface = AppSurface::Comments;
            }
            AppRoute::Diff(source) => {
                if source.requires_github_auth() && !self.ensure_github_auth() {
                    self.surface = AppSurface::Queue;
                    return;
                }
                self.activate_route(source);
                self.surface = AppSurface::Diff;
            }
        }
    }

    fn move_surface_down(&mut self, rows: usize) {
        match self.surface {
            AppSurface::Queue => self.move_home_selection(1),
            AppSurface::CommitList => self.move_commit_selection(1),
            AppSurface::Comments => self.move_comments_selection(1),
            AppSurface::DetailFull => {
                if self.detail_tab == DetailTab::Semantic {
                    self.move_semantic_selection(1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_add(1)
                }
            }
            AppSurface::Diff => self.move_active_relative(1, rows),
        }
    }

    fn move_surface_up(&mut self, rows: usize) {
        match self.surface {
            AppSurface::Queue => self.move_home_selection(-1),
            AppSurface::CommitList => self.move_commit_selection(-1),
            AppSurface::Comments => self.move_comments_selection(-1),
            AppSurface::DetailFull => {
                if self.detail_tab == DetailTab::Semantic {
                    self.move_semantic_selection(-1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_sub(1)
                }
            }
            AppSurface::Diff => self.move_active_relative(-1, rows),
        }
    }

    /// Step the comment selection by `delta`. Clamped to [0, total-1].
    /// The renderer is responsible for auto-scrolling so the selected
    /// comment stays on-screen, since it owns the row-flattening map.
    fn move_comments_selection(&mut self, delta: isize) {
        let total = self.current_comment_count();
        if total == 0 {
            return;
        }
        let max = total.saturating_sub(1);
        let next = (self.comments_selection as isize)
            .saturating_add(delta)
            .clamp(0, max as isize) as usize;
        self.comments_selection = next;
    }

    /// Count of comments on the currently selected work item, accounting
    /// for whether it's a PR (GitHub comments) or local (review notes).
    fn current_comment_count(&self) -> usize {
        let items = self.home_work_items();
        let selected = items.get(self.home_selection);
        match selected {
            Some(item) => {
                if let Some(pr) = item.pull_request(self) {
                    let key = (pr.repository.clone(), pr.number);
                    self.pr_comments_cache
                        .get(&key)
                        .map_or(pr.comments.len(), Vec::len)
                } else {
                    self.session.notes.len()
                }
            }
            None => 0,
        }
    }

    fn page_surface(&mut self, direction: isize, rows: usize) {
        match self.surface {
            AppSurface::CommitList => {
                self.move_commit_selection(direction * self.viewport_height.max(1) as isize)
            }
            AppSurface::Comments => {
                self.move_comments_selection(direction * self.viewport_height.max(1) as isize)
            }
            AppSurface::DetailFull => {
                if self.detail_tab == DetailTab::Semantic {
                    self.move_semantic_selection(
                        direction * self.semantic_visible_rows.max(1) as isize,
                    );
                } else {
                    self.surface_scroll_y = self
                        .surface_scroll_y
                        .saturating_add_signed(direction * self.viewport_height.max(1) as isize)
                }
            }
            AppSurface::Diff => {
                self.scroll_relative(direction * self.viewport_height as isize, rows)
            }
            AppSurface::Queue => {
                self.surface_scroll_y = self
                    .surface_scroll_y
                    .saturating_add_signed(direction * self.viewport_height.max(1) as isize)
            }
        }
    }

    fn move_home_selection(&mut self, delta: isize) {
        let items_len = self.home_work_items().len();
        let next = self
            .home_selection
            .saturating_add_signed(delta)
            .min(items_len.saturating_sub(1));
        if next != self.home_selection {
            self.home_selection = next;
            self.home_selection_changed_at = Instant::now();
            self.surface_scroll_y = 0;
            self.semantic_scroll_y = 0;
            self.semantic_selection = 0;
            self.revalidate_selected_semantic_diff();
        }
    }

    fn move_commit_selection(&mut self, delta: isize) {
        let next = self
            .commit_selection
            .saturating_add_signed(delta)
            .min(self.commits.len().saturating_sub(1));
        if next != self.commit_selection {
            self.commit_selection = next;
            self.surface_scroll_y = 0;
            self.semantic_scroll_y = 0;
            self.semantic_selection = 0;
            self.revalidate_selected_semantic_diff();
        }
    }

    fn set_detail_tab(&mut self, tab: DetailTab) {
        if self.detail_tab != tab {
            self.detail_tab = tab;
            self.surface_scroll_y = 0;
            self.semantic_scroll_y = 0;
            self.semantic_selection = 0;
        }
    }

    fn home_detail_area(&self, terminal_width: u16, terminal_height: u16) -> Option<Rect> {
        let area = app_content_area(Rect::new(0, 0, terminal_width, terminal_height));
        if area.width < 118 || area.height < 8 {
            return None;
        }
        let [_header, body, _footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let [_queue, _gap, details] = Layout::horizontal([
            Constraint::Percentage(58),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(body);
        Some(details)
    }

    pub(crate) fn theme_variant(&self) -> crate::design_system::ThemeVariant {
        self.theme_variant
    }

    pub(crate) fn home_palette(&self) -> HomePalette {
        HomePalette::for_variant(self.theme_variant)
    }

    pub(crate) fn finder_palette(&self) -> FinderPalette {
        FinderPalette::for_variant(self.theme_variant)
    }

    /// Flip between the warm and graphite registers. The diff body's syntax
    /// spans are theme-independent (Pierre's IDE palette), so toggling is a
    /// single field write — no pierre re-highlight, no document mutation.
    /// Only the diff_theme's bg/text/structural colors change.
    pub(crate) fn toggle_theme_variant(&mut self) {
        self.theme_variant = self.theme_variant.toggled();
        self.store.persist_theme_variant(self.theme_variant);
        self.query_client
            .finish_success(QueryKey::ThemePreference, now_stamp() as i64);
    }

    pub(crate) fn cached_pull_request_body_preview(
        &mut self,
        repository: &str,
        number: u32,
        body: &str,
        width: u16,
        limit: usize,
        palette: &HomePalette,
        parse_if_missing: bool,
    ) -> Option<Vec<Line<'static>>> {
        let key = BodyPreviewCacheKey {
            repository: repository.to_string(),
            number,
            width,
            limit,
            theme_variant: self.theme_variant,
        };
        if let Some(lines) = self.body_preview_cache.get(&key) {
            return Some(lines.clone());
        }
        if !parse_if_missing {
            return None;
        }
        let lines = body_preview_lines(body, width, limit, palette);
        self.body_preview_cache.insert(key, lines.clone());
        Some(lines)
    }

    pub(crate) fn home_selection_is_settled(&self) -> bool {
        self.home_selection_changed_at.elapsed() >= Duration::from_millis(120)
    }

    /// Project + branch label shown in the top header.
    /// Until M3 (real project detection) this derives from the launching repo
    /// folder name and the current session branch.
    pub(crate) fn scope_label(&self) -> String {
        let project = self.project_label();
        match project {
            Some(project) => format!("{project} · {}", self.local_route.branch),
            None => "no project scope".to_string(),
        }
    }

    fn project_label(&self) -> Option<String> {
        self.project_label.clone()
    }

    fn project_label_from_env() -> Option<String> {
        if let Ok(value) = std::env::var("LAZYDIFF_PROJECT") {
            if !value.trim().is_empty() {
                return Some(normalize_project_label(&value));
            }
        }
        None
    }

    fn detect_project_label_from_git() -> Option<String> {
        let cwd = std::env::current_dir().ok()?;
        let url = std::process::Command::new("git")
            .args(["config", "--get", "remote.origin.url"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())?;
        let url = url.trim();
        (!url.is_empty()).then(|| normalize_project_label(url))
    }

    fn initial_local_route() -> LocalWorktreeRoute {
        LocalWorktreeRoute {
            repo_path: std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "local".to_string()),
            branch: std::env::var("LAZYDIFF_BRANCH")
                .ok()
                .filter(|branch| !branch.trim().is_empty())
                .unwrap_or_else(|| "loading".to_string()),
            base_ref: std::env::var("LAZYDIFF_BASE_REF").unwrap_or_else(|_| "HEAD".to_string()),
        }
    }

    fn load_local_worktree_diff() -> std::result::Result<LocalDiffResult, String> {
        let cwd =
            std::env::current_dir().map_err(|error| format!("failed to read cwd: {error}"))?;
        let repo_path = git_stdout_in(&cwd, ["rev-parse", "--show-toplevel"])
            .unwrap_or_else(|| cwd.display().to_string());
        let branch = std::env::var("LAZYDIFF_BRANCH")
            .ok()
            .filter(|branch| !branch.trim().is_empty())
            .or_else(|| git_stdout_in(&cwd, ["branch", "--show-current"]))
            .or_else(|| git_stdout_in(&cwd, ["rev-parse", "--abbrev-ref", "HEAD"]))
            .filter(|branch| branch != "HEAD")
            .unwrap_or_else(|| "detached-head".to_string());
        let base_ref = std::env::var("LAZYDIFF_BASE_REF").unwrap_or_else(|_| "HEAD".to_string());
        let patch = git_stdout_result_in(
            &cwd,
            ["diff", "--no-ext-diff", "--binary", base_ref.as_str()],
        )?;
        let document = Self::materialize_diff_document(&patch);
        Ok(LocalDiffResult {
            repo_path,
            branch,
            base_ref,
            document,
        })
    }

    fn load_worktrees() -> std::result::Result<Vec<Worktree>, String> {
        let cwd =
            std::env::current_dir().map_err(|error| format!("failed to read cwd: {error}"))?;
        let repo_root = git_stdout_result_in(&cwd, ["rev-parse", "--show-toplevel"])?;
        list_worktrees(Path::new(repo_root.trim()))
    }

    fn load_commit_diff(repo_path: &str, sha: &str) -> std::result::Result<DiffDocument, String> {
        if let Some(repository) = repo_path.strip_prefix("github:") {
            let patch = fetch_commit_patch(repository, sha)?;
            return Ok(Self::materialize_diff_document(&patch));
        }
        let patch = git_stdout_result_in(
            Path::new(repo_path),
            [
                "show",
                "--format=",
                "--patch",
                "--no-ext-diff",
                "--binary",
                sha,
            ],
        )?;
        Ok(Self::materialize_diff_document(&patch))
    }

    fn load_pull_request_diff(
        repository: &str,
        number: u32,
    ) -> std::result::Result<PullRequestDiffResult, String> {
        let patch = fetch_pull_request_patch(repository, number)?;
        let document = Self::materialize_diff_document(&patch);
        Ok(PullRequestDiffResult { patch, document })
    }

    fn materialize_diff_document(patch: &str) -> DiffDocument {
        let mut document = parse_unified_diff(patch);
        add_pierre_highlights(&mut document);
        document
    }

    fn replace_document_preserving_view(&mut self, document: DiffDocument) {
        self.document = document;
        let rows = row_count_for_mode(&self.document, self.state.mode);
        self.state.selected_row = self.state.selected_row.min(rows.saturating_sub(1));
        self.state.scroll_y = self
            .state
            .scroll_y
            .min(rows.saturating_sub(self.viewport_height));
        self.state.clear_mouse_selection();
    }

    fn home_work_items(&self) -> Vec<WorkItem> {
        let project = self.project_label();
        let your_work_label = match project.as_deref() {
            Some(project) => format!("your work · {project}"),
            None => "your work".to_string(),
        };
        let worktrees = self.worktrees_for_queue();
        let github_items = if self.github_auth.can_load_github() {
            self.github.items.as_slice()
        } else {
            &[]
        };
        let linked_pr_ids = project
            .as_ref()
            .map(|project| link_worktree_pr(&worktrees, github_items, project))
            .unwrap_or_default();
        let pr_index_by_id: HashMap<PrId, usize> = self
            .github_auth
            .can_load_github()
            .then_some(github_items)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(index, pr)| {
                (
                    PrId {
                        repository: pr.repository.clone(),
                        number: pr.number,
                    },
                    index,
                )
            })
            .collect();
        let linked_pr_indices: HashSet<usize> = linked_pr_ids
            .values()
            .filter_map(|id| pr_index_by_id.get(id).copied())
            .collect();

        let mut items: Vec<WorkItem> = Vec::new();
        for (worktree_index, worktree) in worktrees.iter().enumerate() {
            items.push(WorkItem {
                id: (worktree_index + 1) as u64,
                kind: WorkItemKind::LocalAgentBranch,
                group: your_work_label.clone(),
                title: worktree.branch.clone(),
                age: if worktree.is_current { "now" } else { "local" }.to_string(),
                pr_index: None,
                linked_pr_index: linked_pr_ids
                    .get(&worktree.id)
                    .and_then(|id| pr_index_by_id.get(id).copied()),
                branch_status: Some(branch_status_label(worktree)),
                upstream: worktree.upstream.clone(),
                local_route: Some(self.route_for_worktree(worktree)),
                child: false,
            });

            let Some(pr_index) = linked_pr_ids
                .get(&worktree.id)
                .and_then(|id| pr_index_by_id.get(id).copied())
            else {
                continue;
            };
            let pr = &self.github.items[pr_index];
            items.push(WorkItem {
                id: pr.number as u64,
                kind: pr.kind,
                group: your_work_label.clone(),
                title: pr.title.clone(),
                age: relative_age(&pr.created_at),
                pr_index: Some(pr_index),
                linked_pr_index: None,
                branch_status: None,
                upstream: None,
                local_route: None,
                child: true,
            });
        }

        // Bucket every remaining PR by concrete repository. This keeps
        // current-project PRs first while avoiding a vague "other repos"
        // catch-all that hides ownership context.
        for (index, pr) in github_items.iter().enumerate() {
            if linked_pr_indices.contains(&index) {
                continue;
            }
            if Some(pr.repository.as_str()) != project.as_deref() {
                continue;
            }
            items.push(WorkItem {
                id: pr.number as u64,
                kind: pr.kind,
                group: format!("pull requests · {}", pr.repository),
                title: pr.title.clone(),
                age: relative_age(&pr.created_at),
                pr_index: Some(index),
                linked_pr_index: None,
                branch_status: None,
                upstream: None,
                local_route: None,
                child: false,
            });
        }

        let mut repo_order: Vec<String> = Vec::new();
        let mut repo_indices: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, pr) in github_items.iter().enumerate() {
            if linked_pr_indices.contains(&index) {
                continue;
            }
            if Some(pr.repository.as_str()) == project.as_deref() {
                continue;
            }
            if !repo_indices.contains_key(&pr.repository) {
                repo_order.push(pr.repository.clone());
            }
            repo_indices
                .entry(pr.repository.clone())
                .or_default()
                .push(index);
        }
        for repository in repo_order {
            let Some(indices) = repo_indices.remove(&repository) else {
                continue;
            };
            let group = format!("pull requests · {repository}");
            for index in indices {
                let pr = &self.github.items[index];
                items.push(WorkItem {
                    id: pr.number as u64,
                    kind: pr.kind,
                    group: group.clone(),
                    title: pr.title.clone(),
                    age: relative_age(&pr.created_at),
                    pr_index: Some(index),
                    linked_pr_index: None,
                    branch_status: None,
                    upstream: None,
                    local_route: None,
                    child: false,
                });
            }
        }

        items
    }

    fn worktrees_for_queue(&self) -> Vec<Worktree> {
        if !self.worktrees.is_empty() {
            return self.worktrees.clone();
        }
        vec![Worktree {
            id: WorktreeId(self.local_route.repo_path.clone()),
            path: self.local_route.repo_path.clone(),
            branch: self.local_route.branch.clone(),
            head_sha: String::new(),
            upstream: None,
            ahead: 0,
            behind: 0,
            additions: self.local_document.additions(),
            deletions: self.local_document.deletions(),
            files_changed: self.local_document.files.len(),
            is_current: true,
        }]
    }

    fn route_for_worktree(&self, worktree: &Worktree) -> LocalWorktreeRoute {
        LocalWorktreeRoute {
            repo_path: worktree.path.clone(),
            branch: worktree.branch.clone(),
            base_ref: self.local_route.base_ref.clone(),
        }
    }

    fn selected_work_item(&self) -> Option<WorkItem> {
        let items = self.home_work_items();
        items
            .get(self.home_selection.min(items.len().saturating_sub(1)))
            .cloned()
    }

    fn activate_route(&mut self, route: DiffSource) {
        if self.diff_source != route {
            self.diff_source = route.clone();
        }
        self.session =
            Self::load_session_for_route(&self.store, &route, &self.document_for_route(&route));
        self.sync_viewed_state_for_session();
        self.restore_view_state_for_current_route();
    }

    fn document_for_route(&self, route: &DiffSource) -> DiffDocument {
        match route {
            DiffSource::LocalWorktree(_) => self.local_document.clone(),
            DiffSource::PullRequest { repository, number } => self
                .pr_diff_cache
                .get(&(repository.clone(), *number))
                .cloned()
                .unwrap_or_else(|| parse_unified_diff("")),
            DiffSource::Commit { .. } => self.document.clone(),
        }
    }

    fn load_session_for_route(
        store: &ReviewStore,
        route: &DiffSource,
        document: &DiffDocument,
    ) -> ReviewSession {
        let id = route.session_id();
        let (kind, repo_path, branch, base_ref, patch_label) = match route {
            DiffSource::LocalWorktree(local) => (
                WorkItemKind::LocalAgentBranch,
                local.repo_path.clone(),
                local.branch.clone(),
                local.base_ref.clone(),
                route.patch_label(),
            ),
            DiffSource::PullRequest { repository, number } => (
                WorkItemKind::RequestedPrReview,
                repository.clone(),
                format!("PR #{number}"),
                format!("pull/{number}"),
                route.patch_label(),
            ),
            DiffSource::Commit { repo_path, sha } => (
                WorkItemKind::LocalAgentBranch,
                repo_path.clone(),
                format!("commit {}", &sha[..sha.len().min(7)]),
                sha.clone(),
                route.patch_label(),
            ),
        };
        ReviewSession::load_or_create_scoped(
            store,
            id,
            kind,
            repo_path,
            branch,
            base_ref,
            &patch_label,
            document,
        )
    }

    fn local_review_session(&self) -> ReviewSession {
        let route = self.local_route();
        self.store
            .load_session(&route.session_id())
            .unwrap_or_else(|| {
                Self::load_session_for_route(&self.store, &route, &self.local_document)
            })
    }

    fn open_selected_diff(&mut self) {
        let Some(item) = self.selected_work_item() else {
            return;
        };
        if let Some(pull_request) = item
            .pr_index
            .and_then(|index| self.github.items.get(index).cloned())
        {
            self.open_pull_request_diff(&pull_request);
        } else {
            self.open_local_diff(item.local_route.clone());
        }
    }

    fn refresh_github_auth_gate(&mut self) -> GitHubAuthStatus {
        self.github_auth = github_auth_status();
        if !self.github_auth.can_load_github() {
            self.github.status = GitHubQueueStatus::MissingToken;
            self.github.viewer = None;
            self.github.items.clear();
            self.body_preview_cache.clear();
        }
        self.github_auth.clone()
    }

    fn ensure_github_auth(&mut self) -> bool {
        let auth = self.refresh_github_auth_gate();
        if auth.can_load_github() {
            return true;
        }
        if let Some(error) = auth.error() {
            self.query_client
                .finish_error(QueryKey::GitHubQueue, error.clone());
            self.commit_status = Some(error);
        }
        false
    }

    fn open_local_diff(&mut self, route: Option<LocalWorktreeRoute>) {
        let source = DiffSource::LocalWorktree(route.unwrap_or_else(|| self.local_route.clone()));
        self.document = self.local_document.clone();
        self.push_route(AppRoute::Diff(source));
        self.state = DiffViewState::default();
        self.surface_scroll_y = 0;
        self.revalidate_local_diff();
        self.revalidate_selected_semantic_diff();
    }

    fn open_pull_request_diff(&mut self, pull_request: &GitHubPullRequest) {
        if !self.ensure_github_auth() {
            return;
        }
        let key = (pull_request.repository.clone(), pull_request.number);
        let route = DiffSource::PullRequest {
            repository: pull_request.repository.clone(),
            number: pull_request.number,
        };
        self.push_route(AppRoute::Diff(route.clone()));
        if let Some(document) = self.pr_diff_cache.get(&key).cloned() {
            self.document = document;
            self.state = DiffViewState::default();
            self.session = Self::load_session_for_route(&self.store, &route, &self.document);
        } else {
            self.document = parse_unified_diff("");
            self.state = DiffViewState::default();
            if let Some(patch) = self.pr_patch_cache.get(&key).cloned() {
                self.materialize_cached_pull_request_diff(
                    pull_request.repository.clone(),
                    pull_request.number,
                    patch,
                );
            }
        }
        self.surface_scroll_y = 0;
        self.revalidate_pull_request_diff(pull_request.repository.clone(), pull_request.number);
        self.revalidate_semantic_diff(route);
    }

    fn open_selected_in_browser(&mut self) {
        let Some(item) = self.selected_work_item() else {
            return;
        };
        let Some(url) = item.browser_url(self) else {
            return;
        };
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        if let Err(error) = ProcessCommand::new(opener).arg(&url).spawn() {
            self.branch_operation_status = Some(format!("failed to open browser: {error}"));
        }
    }

    fn run_selected_branch_operation(&mut self, operation: BranchOperation) {
        if self.surface != AppSurface::Queue {
            return;
        }
        let Some(item) = self.selected_work_item() else {
            return;
        };
        if item.pr_index.is_some() {
            return;
        }
        let Some(route) = item.local_route.clone() else {
            return;
        };
        self.branch_operation_status = Some(operation.running_label().to_string());
        let upstream = item.upstream.clone();
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = run_branch_operation(&route, upstream.as_deref(), operation);
            let _ = sender.send(QueryEvent::BranchOperation(result));
        });
    }

    fn open_commit_list(&mut self) {
        if self.surface != AppSurface::Queue {
            return;
        }
        let Some(item) = self.selected_work_item() else {
            return;
        };
        self.commit_selection = 0;
        self.commits.clear();
        if let Some(pull_request) = item
            .pr_index
            .and_then(|index| self.github.items.get(index).cloned())
        {
            if !self.ensure_github_auth() {
                return;
            }
            self.commit_route = None;
            self.commit_pr_route = Some((pull_request.repository.clone(), pull_request.number));
            self.commit_status = Some("loading PR commits…".to_string());
            self.push_route(AppRoute::CommitList);
            let sender = self.query_tx.clone();
            thread::spawn(move || {
                let result =
                    fetch_pull_request_commits(&pull_request.repository, pull_request.number);
                let _ = sender.send(QueryEvent::BranchCommits(result));
            });
        } else if let Some(route) = item.local_route.clone() {
            self.commit_route = Some(route.clone());
            self.commit_pr_route = None;
            self.commit_status = Some("loading commits…".to_string());
            self.push_route(AppRoute::CommitList);
            let upstream = item.upstream.clone();
            let sender = self.query_tx.clone();
            thread::spawn(move || {
                let result = list_branch_commits(Path::new(&route.repo_path), upstream.as_deref());
                let _ = sender.send(QueryEvent::BranchCommits(result));
            });
        }
    }

    fn open_selected_commit_diff(&mut self) {
        let Some(commit) = self.commits.get(self.commit_selection).cloned() else {
            return;
        };
        self.commit_status = Some(format!("loading {}…", commit.short_sha));
        let repo_path = if let Some(route) = self.commit_route.clone() {
            route.repo_path
        } else if let Some((repository, _number)) = self.commit_pr_route.clone() {
            if !self.ensure_github_auth() {
                return;
            }
            format!("github:{repository}")
        } else {
            return;
        };
        let source = DiffSource::Commit {
            repo_path: repo_path.clone(),
            sha: commit.sha.clone(),
        };
        self.document = parse_unified_diff("");
        self.state = DiffViewState::default();
        self.push_route(AppRoute::Diff(source));
        self.revalidate_semantic_diff(self.diff_source.clone());
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Self::load_commit_diff(&repo_path, &commit.sha);
            let _ = sender.send(QueryEvent::CommitDiff {
                repo_path,
                sha: commit.sha,
                result,
            });
        });
    }

    fn materialize_cached_pull_request_diff(
        &mut self,
        repository: String,
        number: u32,
        patch: String,
    ) {
        let sender = self.query_tx.clone();
        thread::spawn(move || {
            let result = Ok(Self::materialize_diff_document(&patch));
            let _ = sender.send(QueryEvent::CachedDiff {
                repository,
                number,
                patch,
                result,
            });
        });
    }

    fn revalidate_selected_comments(&mut self) {
        let Some(pull_request) = self
            .selected_work_item()
            .and_then(|item| item.pr_index)
            .and_then(|index| self.github.items.get(index).cloned())
        else {
            return;
        };
        if !self.ensure_github_auth() {
            return;
        }
        self.revalidate_pull_request_comments(pull_request.repository, pull_request.number);
    }

    fn selected_comments(&self, selected: &WorkItem) -> Vec<CommentView> {
        if let Some(pull_request) = selected.pull_request(self) {
            let key = (pull_request.repository.clone(), pull_request.number);
            return self
                .pr_comments_cache
                .get(&key)
                .unwrap_or(&pull_request.comments)
                .iter()
                .map(CommentView::from_github)
                .collect();
        }
        self.session
            .notes
            .iter()
            .map(CommentView::from_note)
            .collect()
    }

    fn local_route(&self) -> DiffSource {
        DiffSource::LocalWorktree(self.local_route.clone())
    }

    fn current_file_index(&self) -> Option<usize> {
        let rows = row_count_for_mode(&self.document, self.state.mode);
        if self
            .document
            .is_file_header_row(self.state.mode, self.state.scroll_y)
        {
            return self
                .document
                .row_file_index(self.state.mode, self.state.scroll_y);
        }
        self.document
            .row_file_index(self.state.mode, self.first_unobscured_visible_row(rows))
    }

    fn focus_row(&mut self, row: usize, rows: usize) {
        self.state.clear_mouse_selection();
        self.state.selected_row = row.min(rows.saturating_sub(1));
        // Pull scroll back by the sticky-file overlay height so the
        // landed row isn't hidden under it, and so the user can see a
        // few lines of context above the entity.
        let top_margin = self.active_top_margin();
        let context_margin = top_margin.saturating_add(2);
        let target_scroll = self.state.selected_row.saturating_sub(context_margin);
        self.state.scroll_y = target_scroll.min(rows.saturating_sub(self.viewport_height.max(1)));
    }

    pub(super) fn trigger_transient_focus(&mut self, path: String, row: usize) {
        self.transient_focus = Some(TransientFocus {
            path,
            row,
            started_at: Instant::now(),
        });
    }

    /// Drop an expired transient focus. Returns `true` when state was
    /// mutated so the caller can request a redraw.
    fn tick_transient_focus(&mut self) -> bool {
        let Some(focus) = self.transient_focus.as_ref() else {
            return false;
        };
        if focus.started_at.elapsed() >= TRANSIENT_FOCUS_DURATION {
            self.transient_focus = None;
            true
        } else {
            // While the highlight is active, we still want continuous
            // redraws so the fade phases through.
            true
        }
    }

    fn render_transient_focus(&self, frame: &mut Frame, body: Rect, _palette: HomePalette) {
        let Some(focus) = self.transient_focus.as_ref() else {
            return;
        };
        let elapsed = focus.started_at.elapsed();
        if elapsed >= TRANSIENT_FOCUS_DURATION {
            return;
        }
        if focus.row < self.state.scroll_y {
            return;
        }
        let offset = focus.row - self.state.scroll_y;
        if offset >= body.height as usize {
            return;
        }
        let y = body.y + offset as u16;
        // Two-phase pulse: an intense flash at first, then a calm amber
        // wash that lingers so the eye can settle on the target line.
        // We only mutate the background of existing cells so we don't
        // stomp double-width glyphs or the gutter's line numbers.
        let progress = elapsed.as_secs_f32() / TRANSIENT_FOCUS_DURATION.as_secs_f32();
        let highlight_bg = if progress < 0.35 {
            Color::Rgb(150, 100, 30)
        } else if progress < 0.7 {
            Color::Rgb(90, 60, 18)
        } else {
            Color::Rgb(58, 40, 12)
        };
        let buffer = frame.buffer_mut();
        for x in body.x..body.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_bg(highlight_bg);
            }
        }
    }

    fn scroll_relative(&mut self, delta: isize, rows: usize) {
        self.state.scroll_y = self
            .state
            .scroll_y
            .saturating_add_signed(delta)
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn move_active_relative(&mut self, delta: isize, rows: usize) {
        if rows == 0 {
            self.state.selected_row = 0;
            self.state.scroll_y = 0;
            return;
        }

        let selecting = self.state.selection.is_some();
        if !self.is_active_visible(rows) {
            self.state.selected_row = self.first_unobscured_visible_row(rows);
            self.update_keyboard_selection(rows);
            return;
        }

        self.state.selected_row = self
            .state
            .selected_row
            .saturating_add_signed(delta)
            .min(rows.saturating_sub(1));
        self.keep_active_visible(rows);
        if selecting {
            self.update_keyboard_selection(rows);
        } else {
            self.state.clear_mouse_selection();
        }
    }

    fn move_active_half_page(&mut self, direction: isize, rows: usize) {
        if rows == 0 {
            self.state.selected_row = 0;
            self.state.scroll_y = 0;
            return;
        }

        let half_page = (self.viewport_height.max(2) / 2).max(1) as isize;
        let viewport_height = self.viewport_height.max(1);
        let target = self
            .state
            .scroll_y
            .saturating_add_signed(direction.saturating_mul(half_page))
            .min(rows.saturating_sub(1));
        let max_scroll = rows.saturating_sub(viewport_height);
        let target_scroll = target.min(max_scroll);

        self.state.clear_mouse_selection();
        self.state.scroll_y = target_scroll;
        self.state.selected_row = self.first_unobscured_visible_row(rows);
    }

    fn toggle_visual_selection(&mut self, rows: usize) {
        if self.state.selection.is_some() {
            self.state.clear_mouse_selection();
            return;
        }
        if rows == 0 {
            return;
        }
        let Some(target) = self.focus_comment_target() else {
            return;
        };
        let row = self.state.selected_row;
        self.state
            .start_mouse_selection(row, target.side, 0, rows, self.viewport_height);
        self.state
            .update_mouse_selection(row, usize::MAX, rows, self.viewport_height);
    }

    fn update_keyboard_selection(&mut self, rows: usize) {
        if self.state.selection.is_none() {
            return;
        }
        self.state.update_mouse_selection(
            self.state.selected_row,
            usize::MAX,
            rows,
            self.viewport_height,
        );
    }

    fn is_active_visible(&self, rows: usize) -> bool {
        if rows == 0 || self.viewport_height == 0 {
            return false;
        }
        let first_visible = self.first_unobscured_visible_row(rows);
        let last_visible = first_visible
            .saturating_add(
                self.viewport_height
                    .saturating_sub(self.active_top_margin())
                    .saturating_sub(1),
            )
            .min(rows.saturating_sub(1));
        self.state.selected_row >= first_visible && self.state.selected_row <= last_visible
    }

    fn first_unobscured_visible_row(&self, rows: usize) -> usize {
        self.state
            .scroll_y
            .saturating_add(self.active_top_margin())
            .min(rows.saturating_sub(1))
    }

    fn active_top_margin(&self) -> usize {
        STICKY_FILE_OVERLAY_ROWS.min(self.viewport_height.saturating_sub(1))
    }

    fn keep_active_visible(&mut self, rows: usize) {
        if rows == 0 {
            self.state.scroll_y = 0;
            self.state.selected_row = 0;
            return;
        }
        let top_margin = self.active_top_margin();
        let viewport_height = self.viewport_height.max(1);
        self.state.selected_row = self.state.selected_row.min(rows.saturating_sub(1));
        if self.state.selected_row < self.state.scroll_y.saturating_add(top_margin) {
            self.state.scroll_y = self.state.selected_row.saturating_sub(top_margin);
        } else if self.state.selected_row >= self.state.scroll_y.saturating_add(viewport_height) {
            self.state.scroll_y = self
                .state
                .selected_row
                .saturating_sub(viewport_height.saturating_sub(1));
        }
        self.state.scroll_y = self
            .state
            .scroll_y
            .min(rows.saturating_sub(viewport_height));
    }

    fn navigation_origin_row(&self) -> usize {
        if self.state.selected_row >= self.state.scroll_y
            && self.state.selected_row < self.state.scroll_y.saturating_add(self.viewport_height)
        {
            self.state.selected_row
        } else {
            self.state.scroll_y
        }
    }

    fn jump_relative_file(&mut self, delta: isize, rows: usize) {
        let Some(current) = self
            .document
            .row_file_index(self.state.mode, self.navigation_origin_row())
        else {
            return;
        };
        let next = current
            .saturating_add_signed(delta)
            .min(self.document.files.len().saturating_sub(1));
        self.jump_to_file(next, rows);
    }

    fn jump_to_file(&mut self, file_index: usize, rows: usize) {
        let Some(row) = self.document.file_row(self.state.mode, file_index) else {
            return;
        };
        self.focus_row(row, rows);
    }

    fn jump_to_text_result(&mut self, result: &TextSearchResult, rows: usize) {
        let Some(row) = self.document.line_row(
            self.state.mode,
            result.file_index,
            result.hunk_index,
            result.line_index,
        ) else {
            return;
        };
        self.state.clear_mouse_selection();
        self.state.selected_side = if result.kind == "-" {
            DiffSide::Left
        } else {
            DiffSide::Right
        };
        self.state.selected_row = row.min(rows.saturating_sub(1));
        let sticky_header_rows = 2usize;
        let context_rows = sticky_header_rows + 3;
        self.state.scroll_y = self
            .state
            .selected_row
            .saturating_sub(context_rows)
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn jump_relative_hunk(&mut self, delta: isize, rows: usize) {
        self.jump_relative_hunk_from(self.navigation_origin_row(), delta, rows);
    }

    fn jump_relative_hunk_from(&mut self, origin_row: usize, delta: isize, rows: usize) {
        let target = if delta > 0 {
            self.document.next_hunk_row(self.state.mode, origin_row)
        } else {
            self.document.previous_hunk_row(self.state.mode, origin_row)
        };
        let Some(row) = target else { return };
        self.state.clear_mouse_selection();
        self.state.selected_row = row.min(rows.saturating_sub(1));
        self.state.scroll_y = self
            .state
            .selected_row
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn active_line_target(&self) -> Option<DiffLineTarget> {
        self.document.line_target(
            self.state.mode,
            self.state.selected_row,
            self.state.selected_side,
        )
    }

    fn handle_review_sidebar_key(&mut self, code: KeyCode, rows: usize) {
        let visible_rows = self.review_tree_rows();
        match code {
            KeyCode::Esc => self.review_sidebar_focus = false,
            KeyCode::Char('j') | KeyCode::Down => {
                self.review_sidebar_selection = self
                    .review_sidebar_selection
                    .saturating_add(1)
                    .min(visible_rows.len().saturating_sub(1));
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.review_sidebar_selection = self.review_sidebar_selection.saturating_sub(1);
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::PageDown | KeyCode::Char('d') if visible_rows.len() > 1 => {
                let step = self.viewport_height.max(1) / 2;
                self.review_sidebar_selection = self
                    .review_sidebar_selection
                    .saturating_add(step.max(1))
                    .min(visible_rows.len().saturating_sub(1));
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::PageUp | KeyCode::Char('u') if visible_rows.len() > 1 => {
                let step = self.viewport_height.max(1) / 2;
                self.review_sidebar_selection =
                    self.review_sidebar_selection.saturating_sub(step.max(1));
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::Char('h') | KeyCode::Left => self.collapse_selected_review_tree_row(),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.open_selected_review_tree_row(rows)
            }
            KeyCode::Char(' ') => self.toggle_selected_review_tree_viewed(),
            _ => {}
        }
    }

    fn sync_viewed_state_for_session(&mut self) {
        if self.viewed_session_id == self.session.id {
            return;
        }
        let persisted = self.store.restore_viewed_state(&self.session.id);
        self.viewed_files = persisted.files.into_iter().collect();
        self.viewed_entities = persisted.entities.into_iter().collect();
        self.viewed_session_id = self.session.id.clone();
        self.review_sidebar_selection = 0;
        self.review_sidebar_scroll_y = 0;
    }

    fn persist_viewed_state(&self) {
        let mut files: Vec<_> = self.viewed_files.iter().cloned().collect();
        files.sort();
        let mut entities: Vec<_> = self.viewed_entities.iter().cloned().collect();
        entities.sort();
        self.store
            .persist_viewed_state(&self.session.id, &PersistedViewedState { files, entities });
    }

    fn review_tree_rows(&self) -> Vec<ReviewTreeRow> {
        let semantic_by_path = self.semantic_changes_by_path();
        let mut rows = Vec::new();
        let mut emitted_dirs = HashSet::new();
        let mut file_counts: HashMap<String, usize> = HashMap::new();
        for file in &self.document.files {
            let parts: Vec<_> = file
                .new_path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            let mut prefix = String::new();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                *file_counts.entry(prefix.clone()).or_default() += 1;
            }
        }

        for (file_index, file) in self.document.files.iter().enumerate() {
            let parts: Vec<_> = file
                .new_path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            let file_name = parts
                .last()
                .copied()
                .unwrap_or(file.new_path.as_str())
                .to_string();
            let mut collapsed_ancestor = false;
            if parts.len() > 1 {
                let mut prefix = String::new();
                for (depth, part) in parts[..parts.len() - 1].iter().enumerate() {
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(part);
                    let key = ReviewTreeKey::directory(&prefix);
                    let collapsed = !self.review_sidebar_expanded.contains(&key);
                    if emitted_dirs.insert(prefix.clone()) {
                        rows.push(ReviewTreeRow::Directory {
                            key,
                            path: prefix.clone(),
                            name: (*part).to_string(),
                            depth,
                            collapsed,
                            file_count: file_counts.get(&prefix).copied().unwrap_or(0),
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
            let key = ReviewTreeKey::file(&file.new_path);
            let collapsed = !self.review_sidebar_expanded.contains(&key);
            let changes = semantic_by_path.get(file.new_path.as_str());
            rows.push(ReviewTreeRow::File {
                key,
                file_index,
                path: file.new_path.clone(),
                name: file_name,
                depth: parts.len().saturating_sub(1),
                collapsed,
                semantic_count: changes.map_or(0, |changes| changes.len()),
            });
            if collapsed {
                continue;
            }
            if let Some(changes) = changes {
                rows.extend(changes.iter().map(|change| ReviewTreeRow::Entity {
                    key: Self::review_entity_key(&file.new_path, change),
                    file_index,
                    path: file.new_path.clone(),
                    depth: parts.len(),
                    entity_type: change.entity_type.clone(),
                    entity_name: change.entity_name.clone(),
                    change_type: change.change_type.clone(),
                    line: change.line,
                }));
            }
        }
        rows
    }

    fn seed_review_sidebar_expansion(&mut self) {
        let route_id = self.diff_source.session_id();
        if !self.review_sidebar_seeded_routes.insert(route_id) {
            return;
        }
        for file in &self.document.files {
            let parts: Vec<_> = file
                .new_path
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() <= 1 {
                continue;
            }
            let mut prefix = String::new();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                self.review_sidebar_expanded
                    .insert(ReviewTreeKey::directory(&prefix));
            }
        }
    }

    fn semantic_changes_by_path(&self) -> HashMap<&str, &[SemanticChange]> {
        self.semantic_diff_for_route(&self.diff_source)
            .map(|diff| {
                diff.files
                    .iter()
                    .map(|file| (file.path.as_str(), file.changes.as_slice()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn review_entity_key(path: &str, change: &SemanticChange) -> String {
        format!(
            "{path}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            change.entity_type,
            change.entity_name,
            change.change_type,
            change.line.unwrap_or(0)
        )
    }

    fn current_file_path(&self) -> Option<&str> {
        let index = self.current_file_index()?;
        self.document
            .files
            .get(index)
            .map(|file| file.new_path.as_str())
    }

    fn is_file_viewed(&self, path: &str) -> bool {
        self.viewed_files.contains(path)
    }

    fn is_entity_viewed(&self, key: &str) -> bool {
        self.viewed_entities.contains(key)
    }

    fn viewed_file_count(&self) -> usize {
        self.document
            .files
            .iter()
            .filter(|file| self.is_file_viewed(&file.new_path))
            .count()
    }

    fn toggle_current_file_viewed(&mut self) {
        let Some(path) = self.current_file_path().map(str::to_string) else {
            return;
        };
        self.toggle_file_viewed(&path);
    }

    fn toggle_file_viewed(&mut self, path: &str) {
        if self.viewed_files.contains(path) {
            self.viewed_files.remove(path);
            self.set_file_entities_viewed(path, false);
        } else {
            self.viewed_files.insert(path.to_string());
            self.set_file_entities_viewed(path, true);
        }
        self.persist_viewed_state();
    }

    fn set_file_entities_viewed(&mut self, path: &str, viewed: bool) {
        let keys: Vec<_> = self
            .semantic_diff_for_route(&self.diff_source)
            .and_then(|diff| diff.files.iter().find(|file| file.path == path))
            .map(|file| {
                file.changes
                    .iter()
                    .map(|change| Self::review_entity_key(path, change))
                    .collect()
            })
            .unwrap_or_default();
        for key in keys {
            if viewed {
                self.viewed_entities.insert(key);
            } else {
                self.viewed_entities.remove(&key);
            }
        }
    }

    fn toggle_selected_review_tree_viewed(&mut self) {
        let rows = self.review_tree_rows();
        let Some(row) = rows.get(
            self.review_sidebar_selection
                .min(rows.len().saturating_sub(1)),
        ) else {
            return;
        };
        match row.clone() {
            ReviewTreeRow::Directory { path, .. } => self.toggle_directory_viewed(&path),
            ReviewTreeRow::File { path, .. } => self.toggle_file_viewed(&path),
            ReviewTreeRow::Entity { key, path, .. } => {
                if !self.viewed_entities.insert(key.clone()) {
                    self.viewed_entities.remove(&key);
                    self.viewed_files.remove(&path);
                }
                self.persist_viewed_state();
            }
        }
    }

    fn toggle_directory_viewed(&mut self, directory: &str) {
        let paths: Vec<_> = self
            .document
            .files
            .iter()
            .filter(|file| file.new_path.starts_with(&format!("{directory}/")))
            .map(|file| file.new_path.clone())
            .collect();
        let should_mark = paths.iter().any(|path| !self.viewed_files.contains(path));
        for path in paths {
            if should_mark {
                self.viewed_files.insert(path.clone());
            } else {
                self.viewed_files.remove(&path);
            }
            self.set_file_entities_viewed(&path, should_mark);
        }
        self.persist_viewed_state();
    }

    fn open_selected_review_tree_row(&mut self, rows: usize) {
        let visible_rows = self.review_tree_rows();
        let Some(row) = visible_rows
            .get(
                self.review_sidebar_selection
                    .min(visible_rows.len().saturating_sub(1)),
            )
            .cloned()
        else {
            return;
        };
        match row {
            ReviewTreeRow::Directory { key, .. } => {
                if !self.review_sidebar_expanded.insert(key.clone()) {
                    self.review_sidebar_expanded.remove(&key);
                }
            }
            ReviewTreeRow::File {
                key, file_index, ..
            } => {
                if !self.review_sidebar_expanded.insert(key.clone()) {
                    self.review_sidebar_expanded.remove(&key);
                }
                self.jump_to_file(file_index, rows);
            }
            ReviewTreeRow::Entity {
                path,
                line,
                change_type,
                ..
            } => {
                self.focus_semantic_path(&path, line, Some(&change_type));
            }
        }
        self.keep_review_sidebar_selection_visible();
    }

    fn collapse_selected_review_tree_row(&mut self) {
        let rows = self.review_tree_rows();
        let Some(row) = rows
            .get(
                self.review_sidebar_selection
                    .min(rows.len().saturating_sub(1)),
            )
            .cloned()
        else {
            return;
        };
        match row {
            ReviewTreeRow::Directory { key, .. } | ReviewTreeRow::File { key, .. } => {
                self.review_sidebar_expanded.remove(&key);
            }
            ReviewTreeRow::Entity { path, .. } => {
                self.review_sidebar_expanded
                    .remove(&ReviewTreeKey::file(&path));
            }
        }
        self.review_sidebar_selection = self
            .review_sidebar_selection
            .min(self.review_tree_rows().len().saturating_sub(1));
        self.keep_review_sidebar_selection_visible();
    }

    fn keep_review_sidebar_selection_visible(&mut self) {
        let total = self.review_tree_rows().len();
        let visible = self.viewport_height.max(1);
        if total == 0 {
            self.review_sidebar_selection = 0;
            self.review_sidebar_scroll_y = 0;
            return;
        }
        self.review_sidebar_selection = self.review_sidebar_selection.min(total.saturating_sub(1));
        if self.review_sidebar_selection < self.review_sidebar_scroll_y {
            self.review_sidebar_scroll_y = self.review_sidebar_selection;
        } else if self.review_sidebar_selection
            >= self.review_sidebar_scroll_y.saturating_add(visible)
        {
            self.review_sidebar_scroll_y = self
                .review_sidebar_selection
                .saturating_sub(visible.saturating_sub(1));
        }
        self.review_sidebar_scroll_y = self
            .review_sidebar_scroll_y
            .min(total.saturating_sub(visible));
    }

    fn sync_review_sidebar_selection_to_current_file(&mut self) {
        let Some(path) = self.current_file_path() else {
            return;
        };
        let rows = self.review_tree_rows();
        if let Some(index) = rows.iter().position(
            |row| matches!(row, ReviewTreeRow::File { path: row_path, .. } if row_path == path),
        ) {
            self.review_sidebar_selection = index;
            self.keep_review_sidebar_selection_visible();
        }
    }

    fn active_review_target(&mut self) -> Option<DiffLineRangeTarget> {
        if let Some(selection) = self.state.selection {
            if let Some(target) = self.document.selection_target(self.state.mode, selection) {
                return Some(target);
            }
        }
        self.focus_comment_target().map(DiffLineRangeTarget::single)
    }

    fn focus_comment_target(&mut self) -> Option<DiffLineTarget> {
        let rows = row_count_for_mode(&self.document, self.state.mode);
        if rows == 0 {
            return None;
        }

        if let Some(target) = self.line_target_at(self.state.selected_row) {
            return Some(target);
        }

        let visible_top = self.state.scroll_y.min(rows.saturating_sub(1));
        let visible_bottom = self
            .state
            .scroll_y
            .saturating_add(self.viewport_height.saturating_sub(1))
            .min(rows.saturating_sub(1));
        let start = self.state.selected_row.clamp(visible_top, visible_bottom);

        for row in start..=visible_bottom {
            if let Some(target) = self.line_target_at(row) {
                return Some(target);
            }
        }
        for row in (visible_top..start).rev() {
            if let Some(target) = self.line_target_at(row) {
                return Some(target);
            }
        }
        None
    }

    fn line_target_at(&mut self, row: usize) -> Option<DiffLineTarget> {
        if let Some(target) =
            self.document
                .line_target(self.state.mode, row, self.state.selected_side)
        {
            self.state.selected_row = row;
            self.state.selected_side = target.side;
            return Some(target);
        }

        let other_side = match self.state.selected_side {
            DiffSide::Left => DiffSide::Right,
            DiffSide::Right => DiffSide::Left,
        };
        let target = self
            .document
            .line_target(self.state.mode, row, other_side)?;
        self.state.selected_row = row;
        self.state.selected_side = target.side;
        Some(target)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppSurface {
    Queue,
    CommitList,
    DetailFull,
    Comments,
    Diff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalFlow {
    GitHubLogin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailTab {
    Semantic,
    Description,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AppRoute {
    Queue,
    CommitList,
    Detail(DiffSource),
    Comments(DiffSource),
    Diff(DiffSource),
}

#[derive(Clone, Debug)]
struct NavHistory {
    entries: Vec<AppRoute>,
    index: usize,
    last_action: NavAction,
}

impl NavHistory {
    fn new(initial: AppRoute) -> Self {
        Self {
            entries: vec![initial],
            index: 0,
            last_action: NavAction::Replace,
        }
    }

    fn push(&mut self, route: AppRoute) {
        self.index = self.index.saturating_add(1);
        self.entries.truncate(self.index);
        self.entries.push(route);
        self.last_action = NavAction::Push;
    }

    fn replace(&mut self, route: AppRoute) {
        if let Some(entry) = self.entries.get_mut(self.index) {
            *entry = route;
        } else {
            self.entries.push(route);
            self.index = self.entries.len().saturating_sub(1);
        }
        self.last_action = NavAction::Replace;
    }

    fn go(&mut self, delta: isize) -> &AppRoute {
        let max = self.entries.len().saturating_sub(1) as isize;
        self.index = (self.index as isize).saturating_add(delta).clamp(0, max) as usize;
        self.last_action = NavAction::Pop;
        &self.entries[self.index]
    }

    fn can_go_back(&self) -> bool {
        self.index > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavAction {
    Pop,
    Push,
    Replace,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LocalWorktreeRoute {
    repo_path: String,
    branch: String,
    base_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DiffSource {
    LocalWorktree(LocalWorktreeRoute),
    PullRequest { repository: String, number: u32 },
    Commit { repo_path: String, sha: String },
}

impl DiffSource {
    fn requires_github_auth(&self) -> bool {
        matches!(self, Self::PullRequest { .. })
            || matches!(self, Self::Commit { repo_path, .. } if repo_path.starts_with("github:"))
    }

    fn session_id(&self) -> String {
        match self {
            Self::LocalWorktree(route) => stable_id(&(
                "local-worktree",
                route.repo_path.as_str(),
                route.branch.as_str(),
                route.base_ref.as_str(),
            )),
            Self::PullRequest { repository, number } => {
                stable_id(&("pull-request", repository.as_str(), *number))
            }
            Self::Commit { repo_path, sha } => {
                stable_id(&("commit", repo_path.as_str(), sha.as_str()))
            }
        }
    }

    fn patch_label(&self) -> String {
        match self {
            Self::LocalWorktree(route) => format!(
                "local:{}:{}:{}",
                route.repo_path, route.branch, route.base_ref
            ),
            Self::PullRequest { repository, number } => format!("pr:{repository}#{number}"),
            Self::Commit { repo_path, sha } => format!("commit:{repo_path}:{sha}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BodyPreviewCacheKey {
    repository: String,
    number: u32,
    width: u16,
    limit: usize,
    theme_variant: crate::design_system::ThemeVariant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WorkItemKind {
    LocalAgentBranch,
    RequestedPrReview,
    OwnedPrFeedback,
    Update,
}

impl WorkItemKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LocalAgentBranch => "local_agent_branch",
            Self::RequestedPrReview => "requested_pr_review",
            Self::OwnedPrFeedback => "owned_pr_feedback",
            Self::Update => "update",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::LocalAgentBranch => "◐",
            Self::RequestedPrReview => "◐",
            Self::OwnedPrFeedback => "◌",
            Self::Update => "×",
        }
    }
}

#[derive(Clone)]
struct WorkItem {
    id: u64,
    kind: WorkItemKind,
    group: String,
    title: String,
    age: String,
    pr_index: Option<usize>,
    linked_pr_index: Option<usize>,
    branch_status: Option<String>,
    upstream: Option<String>,
    local_route: Option<LocalWorktreeRoute>,
    /// True for cluster child rows (linked PR rendered indented under its
    /// worktree). Drives the `└─` indent in the queue body.
    child: bool,
}

#[derive(Clone, Copy, Debug)]
enum BranchOperation {
    Pull,
    Push,
    Fetch,
    ForcePush,
}

impl BranchOperation {
    fn running_label(self) -> &'static str {
        match self {
            Self::Pull => "pulling…",
            Self::Push => "pushing…",
            Self::Fetch => "fetching…",
            Self::ForcePush => "force pushing…",
        }
    }

    fn done_label(self) -> &'static str {
        match self {
            Self::Pull => "pulled",
            Self::Push => "pushed",
            Self::Fetch => "fetched",
            Self::ForcePush => "force pushed",
        }
    }
}

fn run_branch_operation(
    route: &LocalWorktreeRoute,
    upstream: Option<&str>,
    operation: BranchOperation,
) -> std::result::Result<String, String> {
    let mut command = ProcessCommand::new("git");
    command.current_dir(&route.repo_path);
    match operation {
        BranchOperation::Pull => {
            command.args(["pull", "--ff-only"]);
        }
        BranchOperation::Push => {
            if upstream.is_some() {
                command.args(["push"]);
            } else {
                command.args(["push", "--set-upstream", "origin", &route.branch]);
            }
        }
        BranchOperation::Fetch => {
            command.args(["fetch", "--prune"]);
        }
        BranchOperation::ForcePush => {
            if upstream.is_some() {
                command.args(["push", "--force-with-lease"]);
            } else {
                command.args([
                    "push",
                    "--force-with-lease",
                    "--set-upstream",
                    "origin",
                    &route.branch,
                ]);
            }
        }
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            stderr
        });
    }
    Ok(operation.done_label().to_string())
}

fn branch_status_label(worktree: &Worktree) -> String {
    if worktree.upstream.is_none() {
        return "no upstream ◌".to_string();
    }
    match (worktree.ahead, worktree.behind) {
        (0, 0) => "up to date".to_string(),
        (ahead, 0) => format!("↑{ahead}"),
        (0, behind) => format!("↓{behind}"),
        (ahead, behind) => format!("↑{ahead} ↓{behind}"),
    }
}

impl WorkItem {
    fn pull_request<'a>(&self, app: &'a App) -> Option<&'a GitHubPullRequest> {
        self.pr_index.and_then(|index| app.github.items.get(index))
    }

    fn browser_pull_request<'a>(&self, app: &'a App) -> Option<&'a GitHubPullRequest> {
        self.pr_index
            .or(self.linked_pr_index)
            .and_then(|index| app.github.items.get(index))
    }

    fn browser_url(&self, app: &App) -> Option<String> {
        if let Some(pull_request) = self.browser_pull_request(app) {
            return Some(format!(
                "https://github.com/{}/pull/{}",
                pull_request.repository, pull_request.number
            ));
        }
        let route = self.local_route.as_ref().unwrap_or(&app.local_route);
        let project = app.project_label()?;
        Some(format!(
            "https://github.com/{}/tree/{}",
            project, route.branch
        ))
    }

    fn route(&self, app: &App) -> DiffSource {
        if let Some(pull_request) = self.pull_request(app) {
            return DiffSource::PullRequest {
                repository: pull_request.repository.clone(),
                number: pull_request.number,
            };
        }
        DiffSource::LocalWorktree(
            self.local_route
                .clone()
                .unwrap_or_else(|| app.local_route.clone()),
        )
    }

    fn machine_name(&self, app: &App) -> String {
        if let Some(pull_request) = self.pull_request(app) {
            return pull_request.head_ref_name.clone();
        }
        match self.kind {
            WorkItemKind::LocalAgentBranch => self.session_slug(),
            WorkItemKind::RequestedPrReview => "review-requested".to_string(),
            WorkItemKind::OwnedPrFeedback => "authored".to_string(),
            WorkItemKind::Update => "stale".to_string(),
        }
    }

    fn session_slug(&self) -> String {
        self.title.replace(' ', "-")
    }

    fn status_symbol(&self, app: &App) -> &'static str {
        if let Some(pull_request) = self.pull_request(app) {
            return pull_request.check_status.symbol();
        }
        match self.kind {
            WorkItemKind::Update => "×",
            WorkItemKind::OwnedPrFeedback => "",
            WorkItemKind::LocalAgentBranch | WorkItemKind::RequestedPrReview => "✓",
        }
    }

    fn description(&self, app: &App) -> Vec<String> {
        if let Some(pull_request) = self.pull_request(app) {
            let lines = markdown_preview_lines(&pull_request.body, 12);
            if lines.is_empty() {
                return vec![format!(
                    "{} opened this pull request from {}.",
                    pull_request.author, pull_request.head_ref_name
                )];
            }
            return lines;
        }

        match self.kind {
            WorkItemKind::LocalAgentBranch => {
                let session = app.local_review_session();
                let mut lines = vec![
                    "The agent wrote local code. Inspect this attempt, ask questions, request fixes, or keep notes.".to_string(),
                    format!(
                        "Attempt {} has {} open review item{}.",
                        session.current_attempt.ordinal,
                        session.open_count(),
                        plural_s(session.open_count())
                    ),
                ];
                if let Some(note) = session.notes.last() {
                    lines.push(note.summary());
                }
                lines
            }
            WorkItemKind::RequestedPrReview => vec![
                "Review someone else's PR. Draft comments stay private until submit review.".to_string(),
                "Agent help is for explanation, risk checks, and draft wording.".to_string(),
            ],
            WorkItemKind::OwnedPrFeedback => vec![
                "Someone reviewed your PR. Respond to threads, ask the agent to explain feedback, or request fixes.".to_string(),
                "Reply drafts and local fix attempts will be tracked here.".to_string(),
            ],
            WorkItemKind::Update => vec![
                "This work is stale. Refresh the branch before spending review attention.".to_string(),
                "Update will eventually rebase, rerun the agent, or reload provider state.".to_string(),
            ],
        }
    }
}

struct CommentView {
    author: String,
    body: String,
    created_at: String,
}

impl CommentView {
    fn from_github(comment: &GitHubComment) -> Self {
        Self {
            author: comment.author.clone(),
            body: comment.body.clone(),
            created_at: comment.created_at.clone(),
        }
    }

    fn from_note(note: &ReviewNote) -> Self {
        Self {
            author: note.author.clone(),
            body: note.body.clone(),
            created_at: "local".to_string(),
        }
    }

    fn from_thread_note(note: &ReviewNote) -> Self {
        let reply = note.parent_id.map(|_| " follow-up").unwrap_or_default();
        Self {
            author: format!("{} {}{}", note.author, note.kind.label(), reply),
            body: note.body.clone(),
            created_at: "local".to_string(),
        }
    }
}

enum CommentSurfaceRow {
    Header {
        author: String,
        age: String,
        comment_index: usize,
    },
    Body {
        line: Line<'static>,
        comment_index: usize,
    },
    Blank {
        comment_index: usize,
    },
}

impl CommentSurfaceRow {
    fn comment_index(&self) -> usize {
        match self {
            CommentSurfaceRow::Header { comment_index, .. }
            | CommentSurfaceRow::Body { comment_index, .. }
            | CommentSurfaceRow::Blank { comment_index } => *comment_index,
        }
    }
}

fn comment_surface_rows(
    comments: &[CommentView],
    width: usize,
    palette: &crate::design_system::HomePalette,
) -> Vec<CommentSurfaceRow> {
    if comments.is_empty() {
        return vec![CommentSurfaceRow::Body {
            line: Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    "No comments yet",
                    Style::new().fg(palette.muted).bg(palette.bg),
                ),
            ]),
            comment_index: 0,
        }];
    }
    // Each comment body flows through the same markdown pipeline used by
    // PR descriptions. The 3-space leading gutter (vs the body preview's
    // 1-space) keeps comment text visually anchored under the author
    // header bullet `●` and indented from the thread frame.
    let mut rows = Vec::new();
    for (idx, comment) in comments.iter().enumerate() {
        rows.push(CommentSurfaceRow::Header {
            author: comment.author.to_string(),
            age: relative_age(&comment.created_at),
            comment_index: idx,
        });
        // Width minus the 3-col indent we'll prepend.
        let content_width = width.saturating_sub(2).max(16) as u16;
        let mut lines = body_preview_lines(&comment.body, content_width, 200, palette);
        if lines.is_empty() {
            lines = vec![Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "(empty comment)",
                    Style::new().fg(palette.muted).bg(palette.bg),
                ),
            ])];
        }
        for mut line in lines {
            // body_preview_lines starts each line with a 1-col gutter; we
            // grow it to 3 cols so the comment text sits under the header
            // bullet's leading whitespace.
            if let Some(first) = line.spans.first_mut() {
                if first.content.as_ref() == " " {
                    first.content = "   ".to_string().into();
                }
            }
            rows.push(CommentSurfaceRow::Body {
                line,
                comment_index: idx,
            });
        }
        rows.push(CommentSurfaceRow::Blank { comment_index: idx });
    }
    rows
}

fn render_inbox_row(
    note: &ReviewNote,
    width: usize,
    selected: bool,
    palette: FinderPalette,
) -> Line<'static> {
    let bg = if selected {
        palette.selected_bg
    } else {
        palette.bg
    };
    let fg = if selected {
        palette.selected_fg
    } else {
        palette.fg
    };
    let muted = if selected {
        palette.selected_muted
    } else {
        palette.muted
    };
    let (symbol, color) = note.kind.gutter_marker();
    let path = format!(
        "{} {}",
        short_path(note.target.path()),
        target_range_label(&note.target)
    );
    let prefix = format!(" {symbol} {:<18} ", truncate(&path, 18));
    let body_width = width.saturating_sub(prefix.chars().count() + 1);
    Line::from(vec![
        Span::styled(
            format!(" {symbol}"),
            Style::new()
                .fg(if selected { fg } else { color })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<18} ", truncate(&path, 18)),
            Style::new().fg(muted).bg(bg),
        ),
        Span::styled(
            truncate(&note.summary(), body_width),
            Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ),
    ])
    .style(Style::new().bg(bg))
}

/// Reduce a remote URL to `owner/repo`. Falls back to the trimmed input on
/// shapes we don't recognize so the header always shows *something*.
fn normalize_project_label(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed.trim_end_matches(".git");
    let segments: Vec<&str> = if let Some(rest) = stripped.strip_prefix("git@") {
        // git@github.com:owner/repo
        rest.split([':', '/']).filter(|s| !s.is_empty()).collect()
    } else if let Some(rest) = stripped
        .strip_prefix("https://")
        .or_else(|| stripped.strip_prefix("http://"))
        .or_else(|| stripped.strip_prefix("ssh://"))
    {
        rest.split('/').filter(|s| !s.is_empty()).collect()
    } else {
        stripped.split('/').filter(|s| !s.is_empty()).collect()
    };
    if segments.len() >= 2 {
        let owner = segments[segments.len() - 2];
        let repo = segments[segments.len() - 1];
        format!("{owner}/{repo}")
    } else {
        stripped.to_string()
    }
}

pub(crate) fn stable_id<T: Hash>(value: &T) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub(crate) fn now_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn git_stdout_in<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    git_stdout_result_in(cwd, args)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn git_stdout_result_in<const N: usize>(
    cwd: &Path,
    args: [&str; N],
) -> std::result::Result<String, String> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            stderr
        });
    }
    String::from_utf8(output.stdout).map_err(|error| format!("git output was not utf-8: {error}"))
}

fn file_preview_row_count(file: &FileDiff) -> usize {
    file.hunks.iter().map(|hunk| 1 + hunk.lines.len()).sum()
}
