use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs::OpenOptions,
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use color_eyre::Result;
use crossterm::{
    cursor::{MoveTo, SetCursorStyle, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{
        Clear as TerminalClear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
        disable_raw_mode, enable_raw_mode,
    },
};
use lazydiff_diffs::{
    DiffDocument, DiffInlineBlock, DiffInlineBlockAccent, DiffInlineBlockKind, DiffLine,
    DiffLineKind, DiffLineRangeTarget, DiffLineTarget, DiffMode, DiffSide, DiffTheme,
    DiffVisualRow, DiffWidget, DiffWordMotion, FileDiff, FileDiffKind, InlineDiffSpan, SliderState,
    SyntaxHighlightKind, SyntaxSpan, VerticalScrollbar, add_pierre_highlights,
    add_pierre_highlights_with_limit, add_pierre_highlights_with_sources,
    add_pierre_highlights_with_sources_and_limit, parse_unified_diff, render_scrollbar,
    row_count_for_mode,
};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, StatefulWidget},
};
use serde::{Deserialize, Serialize};

const BODY_TOP: u16 = 2;
const APP_TOP_PADDING: u16 = 1;
const STICKY_FILE_OVERLAY_ROWS: usize = 2;
const GITHUB_QUERY_CACHE_BUSTER: &str = "github-query-cache-v1";
const QUERY_CACHE_GC_INTERVAL: Duration = Duration::from_secs(60);
const QUERY_CACHE_MAX_AGE_SECS: i64 = 60 * 60;
const SPINNER_REDRAW_INTERVAL: Duration = Duration::from_millis(80);
const ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(16);
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const INLINE_COMMENT_TEXT_WIDTH: usize = 34;
const SOURCE_AWARE_HIGHLIGHT_FILE_LIMIT: usize = 512;
const HIGHLIGHT_VIEWPORT_OVERSCAN_ROWS: usize = 24;
const HIGHLIGHT_PREFETCH_DOCUMENT_ROWS: usize = 800;
const HIGHLIGHT_PREFETCH_FILE_LIMIT: usize = 24;

fn inline_block_text_width(pane: Rect) -> usize {
    pane.width
        .saturating_sub(4)
        .min(76)
        .saturating_sub(5)
        .max(1) as usize
}

fn inline_comment_visual_line_count(body: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut count = 0usize;
    for line in body.lines() {
        count = count.saturating_add(line.chars().count().div_ceil(width).max(1));
    }
    count.max(1)
}

fn diff_content_area(area: Rect) -> Rect {
    Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height)
}

fn diff_viewport_area(area: Rect) -> Rect {
    let sticky_rows = (STICKY_FILE_OVERLAY_ROWS as u16).min(area.height.saturating_sub(1));
    Rect::new(
        area.x,
        area.y.saturating_add(sticky_rows),
        area.width,
        area.height.saturating_sub(sticky_rows),
    )
}
use crate::commands::{Command, Layer, command_for_layer};
use crate::components::{app_chrome::AppHeader, command_palette::CommandPalette};
use crate::design_system::{FinderPalette, HomePalette, SurfaceLayer, TextRole};
use crate::forge::{Forge, PullRequestFileSources};
use crate::github::{
    GitCommit, GitHubAuthStatus, GitHubComment, GitHubPullRequest, GitHubQueue, GitHubQueueStatus,
    PrId, Worktree, WorktreeId, link_worktree_pr, list_branch_commits, list_worktrees,
};
use crate::highlight_daemon::{
    self, HighlightFileRequest, HighlightLineWindow, HighlightRequest, HighlightResponse,
    WireSyntaxSpan,
};
use crate::persistence::{
    CommentEditorMode, CommentModal, GitHubQueryClientState, PersistedGitHubQueryClient,
    PersistedPullRequestComments, PersistedPullRequestDiff, PersistedSemanticDiff,
    PersistedViewedState, ReviewItemKind, ReviewNote, ReviewSession, ReviewStore, ReviewUiState,
};
use crate::server_query::{
    CommitListContext, LocalDiffResult, PullRequestDiffResult, QueryClient, QueryEvent, QueryKey,
    QueryResult, QueryStatus,
};
use crate::text::{body_preview_lines, markdown_preview_lines, relative_age};
use crate::ui::{
    ListGeometryBuilder, ListRowGeometry, ListRowKind, centered_rect, contains_point, draw_box,
    draw_horizontal_rule, draw_vertical_rule, fill_rect, list_item_rows, list_row_at,
    render_home_rule, right_aligned_text, short_path, truncate, truncate_middle,
};

mod finder;
pub(crate) use finder::CommandResult;
use finder::*;
mod highlight_coordinator;
use highlight_coordinator::{
    HighlightCoordinator, HighlightFrameWindow, HighlightRequestEnvelope, inline_layout_hash,
};
pub(crate) use highlight_coordinator::{HighlightFileJob, HighlightToken};
mod diff_buffer;
use diff_buffer::{DiffBufferAction, DiffBufferMode, DiffBufferState, TextObjectKind};
mod input;
mod modals;
mod queries;
mod semantic;
pub(crate) use semantic::{
    SemanticChange, SemanticDiff, SemanticNodeKey, SemanticTreeRow, SemanticViewport,
    build_semantic_map_nodes, semantic_map_screen_positions, semantic_tree_body_area,
};
mod selection;
use selection::{ScreenPoint, ScreenTextSelection};
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
    forge: Arc<dyn Forge>,
    _path: String,
    project_label: Option<String>,
    document: DiffDocument,
    local_document: DiffDocument,
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
    queue_focus: QueuePane,
    commit_focus: CommitPane,
    diff_buffer: DiffBufferState,
    /// Index of the currently selected comment in the Comments reader.
    /// j/k step between comments (not lines); selected comment renders
    /// with elevated bg + amber rail.
    comments_selection: usize,
    dragging_scrollbar: bool,
    active_scrollbar_drag: Option<ScrollbarDrag>,
    selecting_text: bool,
    text_selection_dragged: bool,
    pending_screen_selection: Option<(ScreenPoint, Option<Rect>)>,
    screen_selection: Option<ScreenTextSelection>,
    screen_selection_bounds: Option<Rect>,
    screen_text: Vec<String>,
    file_picker_open: bool,
    finder_kind: FinderKind,
    file_picker_selection: usize,
    file_picker_query: String,
    file_picker_preview_scroll: usize,
    home_selection: usize,
    home_selection_changed_at: Instant,
    theme_variant: crate::design_system::ThemeVariant,
    attempt_modal_open: bool,
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
    semantic_map_zoom: f32,
    semantic_map_pan_x: i32,
    semantic_map_pan_y: i32,
    pending_semantic_focus: Option<SemanticFocusTarget>,
    review_sidebar_visible: bool,
    diff_focus: DiffFocusPane,
    review_sidebar_focus: bool,
    review_sidebar_selection: usize,
    review_sidebar_scroll_y: usize,
    review_sidebar_unreviewed_only: bool,
    review_sidebar_expanded: HashSet<ReviewTreeKey>,
    review_sidebar_seeded_routes: HashSet<String>,
    expanded_review_threads: HashSet<u64>,
    viewed_files: HashSet<String>,
    viewed_entities: HashSet<String>,
    viewed_session_id: String,
    body_preview_cache: crate::bounded_map::BoundedMap<BodyPreviewCacheKey, Vec<Line<'static>>>,
    query_tx: Sender<QueryEvent>,
    query_rx: Receiver<QueryEvent>,
    query_client: QueryClient,
    last_query_gc_at: Instant,
    comment_modal: Option<CommentModal>,
    inline_focus: Option<InlineFocus>,
    thread_modal: Option<DiffLineTarget>,
    transient_focus: Option<TransientFocus>,
    pending_highlight_window: Option<HighlightFrameWindow>,
    highlight_coordinator: HighlightCoordinator,
    highlight_worker_tx: Sender<HighlightRequestEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InlineFocus {
    block_id: String,
    line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffScrollPolicy {
    EnsureVisible,
    Center,
}

/// A bright row-flash that fades out shortly after a semantic
/// navigation lands in the diff view. Helps the reader spot exactly
/// where they jumped to when the cursor highlight alone is too subtle.
#[derive(Clone, Debug)]
pub(crate) struct TransientFocus {
    pub(crate) _path: String,
    pub(crate) row: usize,
    pub(crate) started_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollbarTarget {
    DetailDescription,
    Comments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollbarDrag {
    pub(crate) target: ScrollbarTarget,
    pub(crate) offset_virtual: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticFocusTarget {
    pub(crate) route: DiffSource,
    pub(crate) path: String,
    pub(crate) line: Option<usize>,
    pub(crate) end_line: Option<usize>,
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
    },
    File {
        key: ReviewTreeKey,
        file_index: usize,
        path: String,
        name: String,
        status: FileDiffKind,
        depth: usize,
        collapsed: bool,
        semantic_count: usize,
    },
    Entity {
        key: String,
        _file_index: usize,
        path: String,
        depth: usize,
        entity_type: String,
        entity_name: String,
        change_type: String,
        line: Option<usize>,
    },
}

#[derive(Debug, Clone)]
enum GroupedWorkItemRow {
    Header {
        label: String,
        geometry: ListRowGeometry,
    },
    Item {
        index: usize,
        geometry: ListRowGeometry,
    },
}

impl GroupedWorkItemRow {
    fn area(&self) -> Rect {
        match self {
            Self::Header { geometry, .. } | Self::Item { geometry, .. } => geometry.area,
        }
    }
}

const TRANSIENT_FOCUS_DURATION: Duration = Duration::from_millis(900);
const TRANSIENT_FOCUS_TICK: Duration = Duration::from_millis(60);

impl App {
    pub(crate) fn new(
        path: String,
        bytes: usize,
        document: DiffDocument,
        forge: Arc<dyn Forge>,
    ) -> Self {
        Self::new_with_initial_route(path, bytes, document, None, true, forge)
    }

    pub(crate) fn new_local_diff(
        path: String,
        bytes: usize,
        document: DiffDocument,
        repo_path: String,
        branch: String,
        base_ref: String,
        forge: Arc<dyn Forge>,
    ) -> Self {
        Self::new_local_diff_with_refresh(
            path, bytes, document, repo_path, branch, base_ref, false, forge,
        )
    }

    pub(crate) fn new_local_diff_with_refresh(
        path: String,
        bytes: usize,
        document: DiffDocument,
        repo_path: String,
        branch: String,
        base_ref: String,
        refresh_local_diff: bool,
        forge: Arc<dyn Forge>,
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
            refresh_local_diff,
            forge,
        )
    }

    pub(crate) fn new_commit_diff(
        path: String,
        bytes: usize,
        document: DiffDocument,
        repo_path: String,
        sha: String,
        forge: Arc<dyn Forge>,
    ) -> Self {
        Self::new_with_initial_route(
            path,
            bytes,
            document,
            Some(AppRoute::Diff(DiffSource::Commit { repo_path, sha })),
            false,
            forge,
        )
    }

    fn new_with_initial_route(
        path: String,
        bytes: usize,
        document: DiffDocument,
        initial_route: Option<AppRoute>,
        refresh_local_diff: bool,
        forge: Arc<dyn Forge>,
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
        let (highlight_worker_tx, highlight_worker_rx) = mpsc::channel();
        spawn_highlight_worker(highlight_worker_rx, query_tx.clone());
        spawn_highlight_daemon_warmup();
        let persisted_queries = store.restore_github_query_client();
        let (github, persisted_queue_at) = persisted_queries
            .as_ref()
            .and_then(|client| {
                let mut queue = client.client_state.queue.clone()?;
                let has_displayable_queue = queue.cached_at.is_some()
                    || matches!(queue.status, GitHubQueueStatus::Ready)
                    || !queue.items.is_empty();
                has_displayable_queue.then(|| {
                    let cached_at = queue.cached_at.unwrap_or(client.timestamp);
                    queue.cached_at = Some(cached_at);
                    queue.status = GitHubQueueStatus::Ready;
                    (queue, cached_at)
                })
            })
            .unwrap_or_else(|| (GitHubQueue::empty_loading(), 0));
        let github_auth = GitHubAuthStatus::Checking;
        let mut query_client = QueryClient::default();
        if persisted_queue_at > 0 {
            query_client.hydrate_success(QueryKey::GitHubQueue, persisted_queue_at);
        }
        let project_label = Self::project_label_from_env();
        // Read the persisted theme synchronously so the first paint
        // already uses the user's preferred variant — no warm→cool
        // flicker once the async revalidate finishes.
        let theme_variant = std::env::var("LAZYDIFF_THEME")
            .ok()
            .or_else(|| std::env::var("LUMEN_THEME").ok())
            .and_then(|label| crate::design_system::ThemeVariant::from_label(&label))
            .or_else(|| store.restore_theme_variant())
            .unwrap_or(crate::design_system::ThemeVariant::DefaultDark);
        let mut app = Self {
            forge,
            _path: path,
            project_label,
            local_document: document.clone(),
            document,
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
            queue_focus: QueuePane::List,
            commit_focus: CommitPane::List,
            diff_buffer: DiffBufferState::default(),
            comments_selection: 0,
            dragging_scrollbar: false,
            active_scrollbar_drag: None,
            selecting_text: false,
            text_selection_dragged: false,
            pending_screen_selection: None,
            screen_selection: None,
            screen_selection_bounds: None,
            screen_text: Vec::new(),
            file_picker_open: false,
            finder_kind: FinderKind::Files,
            file_picker_selection: 0,
            file_picker_query: String::new(),
            file_picker_preview_scroll: 0,
            home_selection: 0,
            home_selection_changed_at: Instant::now(),
            theme_variant,
            attempt_modal_open: false,
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
            semantic_map_zoom: 1.0,
            semantic_map_pan_x: 0,
            semantic_map_pan_y: 0,
            pending_semantic_focus: None,
            review_sidebar_visible: true,
            diff_focus: DiffFocusPane::Right,
            review_sidebar_focus: false,
            review_sidebar_selection: 0,
            review_sidebar_scroll_y: 0,
            review_sidebar_unreviewed_only: false,
            review_sidebar_expanded: HashSet::new(),
            review_sidebar_seeded_routes: HashSet::new(),
            expanded_review_threads: HashSet::new(),
            viewed_files: HashSet::new(),
            viewed_entities: HashSet::new(),
            viewed_session_id: String::new(),
            body_preview_cache: crate::bounded_map::BoundedMap::new(128),
            query_tx,
            query_rx,
            query_client,
            last_query_gc_at: Instant::now(),
            comment_modal: None,
            inline_focus: None,
            thread_modal: None,
            transient_focus: None,
            pending_highlight_window: None,
            highlight_coordinator: HighlightCoordinator::default(),
            highlight_worker_tx,
        };
        if let Some(persisted_queries) = persisted_queries {
            app.hydrate_persisted_query_client(persisted_queries);
        }
        app.sync_viewed_state_for_session();
        app.apply_route(initial_route);
        app.restore_view_state_for_current_route();
        app.schedule_full_diff_highlight(app.diff_source.clone(), app.document.clone());
        app.revalidate_project_label();
        if refresh_local_diff {
            app.revalidate_local_diff();
        }
        app.revalidate_worktrees();
        app.revalidate_semantic_diff(app.local_route());
        app.revalidate_existing_forge_login(false);
        app
    }

    pub(crate) fn run(mut self, terminal: &mut Tui) -> Result<()> {
        let mut needs_redraw = true;
        let mut last_spinner_redraw = Instant::now();
        let mut traced_first_draw = false;
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
                self.apply_cursor_style(terminal)?;
                let elapsed = start.elapsed();
                self.record_draw(elapsed);
                if !traced_first_draw
                    && std::env::var_os("LAZYDIFF_STARTUP_TRACE").is_some()
                    && let Some(startup) = crate::STARTUP_INSTANT.get()
                {
                    eprintln!(
                        "[lazydiff-startup] first_draw_ms={:.3} draw_ms={:.3}",
                        startup.elapsed().as_secs_f64() * 1000.0,
                        elapsed.as_secs_f64() * 1000.0,
                    );
                    traced_first_draw = true;
                }
                self.dispatch_pending_highlight_window();
                needs_redraw = false;
            }

            let poll_interval = if self.query_client.is_fetching()
                || self.dragging_scrollbar
                || self.active_scrollbar_drag.is_some()
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
                let mut coalesced_diff_scroll_rows = 0isize;
                let mut coalesced_diff_scroll_cols = 0isize;
                loop {
                    match event::read()? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => {
                            Self::debug_key_event(key);
                            self.handle_key(key);
                            if let Some(flow) = self.pending_terminal_flow.take() {
                                self.run_terminal_flow(terminal, flow)?;
                            }
                            needs_redraw = true;
                        }
                        Event::Mouse(mouse) => {
                            let size = terminal.size()?;
                            if self.surface == AppSurface::Diff
                                && !self.file_picker_open
                                && self.thread_modal.is_none()
                            {
                                match mouse.kind {
                                    MouseEventKind::ScrollDown => {
                                        coalesced_diff_scroll_rows =
                                            coalesced_diff_scroll_rows.saturating_add(1);
                                        needs_redraw = true;
                                        processed += 1;
                                        if processed >= 256 || !event::poll(Duration::ZERO)? {
                                            break;
                                        }
                                        continue;
                                    }
                                    MouseEventKind::ScrollUp => {
                                        coalesced_diff_scroll_rows =
                                            coalesced_diff_scroll_rows.saturating_sub(1);
                                        needs_redraw = true;
                                        processed += 1;
                                        if processed >= 256 || !event::poll(Duration::ZERO)? {
                                            break;
                                        }
                                        continue;
                                    }
                                    MouseEventKind::ScrollRight => {
                                        coalesced_diff_scroll_cols =
                                            coalesced_diff_scroll_cols.saturating_add(1);
                                        needs_redraw = true;
                                        processed += 1;
                                        if processed >= 256 || !event::poll(Duration::ZERO)? {
                                            break;
                                        }
                                        continue;
                                    }
                                    MouseEventKind::ScrollLeft => {
                                        coalesced_diff_scroll_cols =
                                            coalesced_diff_scroll_cols.saturating_sub(1);
                                        needs_redraw = true;
                                        processed += 1;
                                        if processed >= 256 || !event::poll(Duration::ZERO)? {
                                            break;
                                        }
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            // Plain cursor movement (no button held)
                            // would otherwise force a redraw on every
                            // pixel of motion — the app has no handler
                            // for it outside the semantic map, so swallow
                            // non-semantic motion cheaply.
                            if !matches!(mouse.kind, MouseEventKind::Moved)
                                || self
                                    .semantic_mouse_target_area(size.width, size.height)
                                    .is_some_and(|(_, area)| {
                                        contains_point(area, mouse.column, mouse.row)
                                    })
                            {
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
                if coalesced_diff_scroll_cols != 0 {
                    self.scroll_active_pane_horizontally(coalesced_diff_scroll_cols * 8);
                }
                if coalesced_diff_scroll_rows != 0 {
                    let rows =
                        row_count_for_mode(&self.document, self.diff_buffer.viewer().viewport.mode);
                    self.scroll_relative(coalesced_diff_scroll_rows, rows);
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
            DisableMouseCapture,
            Show,
            LeaveAlternateScreen,
            TerminalClear(ClearType::All),
            MoveTo(0, 0)
        )?;
        terminal.backend_mut().flush()?;

        let result = match flow {
            TerminalFlow::ForgeLogin => TerminalFlowResult::ForgeLogin(self.forge.login()),
            TerminalFlow::OpenEditor { command, cwd } => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                TerminalFlowResult::Editor(
                    match ProcessCommand::new(shell)
                        .arg("-c")
                        .arg(command)
                        .current_dir(cwd)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .status()
                    {
                        Ok(status) if status.success() => Ok(()),
                        Ok(status) => Err(format!("editor exited with {status}")),
                        Err(error) => Err(format!("failed to open editor: {error}")),
                    },
                )
            }
        };

        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            TerminalClear(ClearType::All),
            MoveTo(0, 0)
        )?;
        enable_raw_mode()?;
        terminal.clear()?;
        while event::poll(Duration::ZERO)? {
            let _ = event::read()?;
        }

        self.github_auth = self.forge.auth_status();
        match result {
            TerminalFlowResult::ForgeLogin(Ok(login)) if self.github_auth.can_load_github() => {
                self.github.viewer = Some(login);
                self.github.status = GitHubQueueStatus::Ready;
            }
            TerminalFlowResult::ForgeLogin(Ok(_)) => {
                self.github.status = GitHubQueueStatus::MissingToken;
            }
            TerminalFlowResult::ForgeLogin(Err(error)) => {
                self.github.status = GitHubQueueStatus::Error(error);
            }
            TerminalFlowResult::Editor(Ok(())) => self.revalidate_local_diff(),
            TerminalFlowResult::Editor(Err(error)) => {
                self.branch_operation_status = Some(error);
            }
        }
        Ok(())
    }

    fn restore_view_state_for_current_route(&mut self) {
        let Some(saved) = self.store.restore_ui_state(&self.diff_source.session_id()) else {
            return;
        };
        let rows = row_count_for_mode(&self.document, saved.diff_mode);
        let viewer = self.diff_buffer.viewer_mut();
        viewer.viewport.mode = saved.diff_mode;
        viewer.viewport.scroll_y = saved
            .scroll_y
            .min(rows.saturating_sub(self.viewport_height));
        viewer.cursor.row = saved.selected_row.min(rows.saturating_sub(1));
        viewer.cursor.side = saved.selected_side;
    }

    fn persist_view_state_for_current_route(&self) {
        let viewer = self.diff_buffer.viewer();
        self.store.persist_ui_state(
            &self.diff_source.session_id(),
            ReviewUiState {
                selected_row: viewer.cursor.row,
                scroll_y: viewer.viewport.scroll_y,
                selected_side: viewer.cursor.side,
                diff_mode: viewer.viewport.mode,
            },
        );
    }

    fn render(&mut self, frame: &mut Frame) {
        match self.surface {
            AppSurface::Queue => {
                self.render_home(frame);
                self.render_global_overlays(frame);
                self.finish_render(frame);
                return;
            }
            AppSurface::CommitList => {
                self.render_commit_list(frame);
                self.render_global_overlays(frame);
                self.finish_render(frame);
                return;
            }
            AppSurface::DetailFull => {
                self.render_detail_full(frame);
                self.render_global_overlays(frame);
                self.finish_render(frame);
                return;
            }
            AppSurface::Comments => {
                self.render_comments_surface(frame);
                self.render_global_overlays(frame);
                self.finish_render(frame);
                return;
            }
            AppSurface::Diff if self.should_render_diff_placeholder() => {
                self.render_diff_placeholder(frame);
                self.render_global_overlays(frame);
                self.finish_render(frame);
                return;
            }
            AppSurface::Diff => {}
        }
        let frame_area = frame.area();
        let area = app_content_area(frame_area);
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let (sidebar, sidebar_divider, diff_shell) = self.diff_sidebar_layout(body);
        let diff_body = Rect::new(
            diff_shell.x.saturating_add(1),
            diff_shell.y.saturating_add(1),
            diff_shell.width.saturating_sub(2),
            diff_shell.height.saturating_sub(2),
        );
        let diff_viewport = diff_viewport_area(diff_body);
        self.viewport_height = diff_viewport.height as usize;
        let palette = self.home_palette();
        fill_rect(
            frame.buffer_mut(),
            frame_area,
            " ",
            Style::new().fg(palette.fg).bg(palette.bg),
        );
        self.render_diff_header(frame, header, palette);
        self.render_diff_shell(frame, diff_shell, palette);
        self.render_diff_pane_slider(
            frame,
            Rect::new(diff_body.x, diff_shell.y, diff_body.width, 1),
            diff_body,
            palette,
        );
        self.diff_buffer.sync_viewport(
            diff_viewport.width,
            diff_viewport.height,
            self.active_top_margin(),
        );
        let search_matches = self.diff_buffer.search_matches().to_vec();
        let content_area = diff_content_area(diff_viewport);
        let inline_blocks = self.diff_inline_blocks_for_area(Some(content_area));
        let visible_highlight_window = self.diff_buffer.visible_document_rows(
            &self.document,
            &inline_blocks,
            diff_viewport,
            HIGHLIGHT_VIEWPORT_OVERSCAN_ROWS,
        );
        let visible_file_indices = self.document.file_indices_in_document_rows(
            self.diff_buffer.viewer().viewport.mode,
            &visible_highlight_window.document_rows,
        );
        let highlight_file_indices = self.highlight_file_indices_for_window(
            &visible_file_indices,
            &visible_highlight_window.document_rows,
        );
        let highlight_windows = self.document.file_line_windows_for_document_rows(
            self.diff_buffer.viewer().viewport.mode,
            &visible_highlight_window.document_rows,
        );
        let mut highlight_jobs =
            self.highlight_file_jobs(&highlight_file_indices, &highlight_windows);
        if self.hydrate_cached_expansion_highlights_for_jobs(&highlight_jobs) {
            highlight_jobs = self.highlight_file_jobs(&highlight_file_indices, &highlight_windows);
        }
        let visible_job_count = highlight_jobs
            .iter()
            .take_while(|job| visible_file_indices.contains(&job.file_index))
            .count();
        let inline_hash = inline_layout_hash(
            inline_blocks
                .iter()
                .map(|block| (block.after_row, block.side, block.height)),
        );
        self.pending_highlight_window = Some(HighlightFrameWindow {
            route: self.diff_source.clone(),
            mode: self.diff_buffer.viewer().viewport.mode,
            viewport: diff_viewport,
            visual_start: visible_highlight_window.visual_start,
            visual_end: visible_highlight_window.visual_end,
            overscan: HIGHLIGHT_VIEWPORT_OVERSCAN_ROWS,
            inline_layout_hash: inline_hash,
            file_indices: highlight_file_indices,
            jobs: highlight_jobs,
            visible_job_count,
        });
        StatefulWidget::render(
            DiffWidget::new(&self.document)
                .theme(palette.theme.diff_theme())
                .search_matches(&search_matches)
                .inline_blocks(&inline_blocks)
                .reviewed_paths(&self.viewed_files)
                .show_diff_cursor(self.comment_modal.is_none() && self.inline_focus.is_none()),
            diff_viewport,
            frame.buffer_mut(),
            self.diff_buffer.viewer_mut(),
        );
        if let Some(sidebar) = sidebar {
            self.render_review_map_panel(frame, sidebar, palette);
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
        self.render_note_gutter_markers(frame, diff_viewport);
        // Drawn last so it stays visible on top of gutter markers.
        self.render_transient_focus(frame, diff_viewport, palette);
        self.render_footer(frame, footer);
        self.render_global_overlays(frame);
        self.place_terminal_diff_cursor(frame, diff_viewport, content_area, &inline_blocks);
        self.finish_render(frame);
    }

    fn place_terminal_diff_cursor(
        &self,
        frame: &mut Frame,
        diff_body: Rect,
        content_area: Rect,
        inline_blocks: &[DiffInlineBlock],
    ) {
        if self.surface != AppSurface::Diff
            || self.file_picker_open
            || self.attempt_modal_open
            || diff_body.is_empty()
        {
            return;
        }

        if let Some((x, y)) =
            self.inline_comment_editor_cursor_position(content_area, inline_blocks)
        {
            frame.set_cursor_position((x, y));
            return;
        }
        if let Some((x, y)) = self.inline_focus_cursor_position(content_area, inline_blocks) {
            frame.set_cursor_position((x, y));
            return;
        }
        let Some(position) = self.diff_buffer.viewer().cursor_screen_position(
            &self.document,
            inline_blocks,
            diff_body,
        ) else {
            return;
        };
        frame.set_cursor_position((position.x, position.y));
    }

    fn inline_comment_editor_cursor_position(
        &self,
        diff_body: Rect,
        inline_blocks: &[DiffInlineBlock],
    ) -> Option<(u16, u16)> {
        let modal = self.comment_modal.as_ref()?;
        let viewer = self.diff_buffer.viewer();
        let editor_block_index = inline_blocks
            .iter()
            .position(|block| block.kind == DiffInlineBlockKind::Editor)?;
        let block = inline_blocks.get(editor_block_index)?;
        let pane = viewer.viewport.pane_rect(diff_body, block.side);
        let text_width = inline_block_text_width(pane);
        let editor_line = modal.visual_cursor_row(text_width).saturating_add(1);
        let visual_index = viewer.visual_index_for_inline_block_line(
            &self.document,
            inline_blocks,
            editor_block_index,
            editor_line,
        )?;
        if visual_index < viewer.viewport.scroll_y {
            return None;
        }
        let local_y = visual_index - viewer.viewport.scroll_y;
        if local_y >= diff_body.height as usize {
            return None;
        }
        let cursor_col = modal.visual_cursor_col(text_width);
        let x = pane
            .x
            .saturating_add(4)
            .saturating_add(cursor_col as u16)
            .min(pane.right().saturating_sub(1));
        let y = diff_body.y.saturating_add(local_y as u16);
        Some((x, y))
    }

    fn inline_focus_cursor_position(
        &self,
        diff_body: Rect,
        inline_blocks: &[DiffInlineBlock],
    ) -> Option<(u16, u16)> {
        let focus = self.inline_focus.as_ref()?;
        let viewer = self.diff_buffer.viewer();
        let block_index = inline_blocks
            .iter()
            .position(|block| block.id == focus.block_id)?;
        let visual_index = viewer.visual_index_for_inline_block_line(
            &self.document,
            inline_blocks,
            block_index,
            focus.line,
        )?;
        if visual_index < viewer.viewport.scroll_y {
            return None;
        }
        let local_y = visual_index - viewer.viewport.scroll_y;
        if local_y >= diff_body.height as usize {
            return None;
        }
        let block = inline_blocks.get(block_index)?;
        let pane = viewer.viewport.pane_rect(diff_body, block.side);
        Some((
            pane.x.saturating_add(4).min(pane.right().saturating_sub(1)),
            diff_body.y.saturating_add(local_y as u16),
        ))
    }

    fn finish_render(&mut self, frame: &mut Frame) {
        self.capture_screen_text(frame);
        self.render_screen_selection(frame);
    }

    fn render_screen_selection(&self, frame: &mut Frame) {
        let Some(selection) = self.screen_selection else {
            return;
        };
        let area = frame.area();
        let bounds = self.screen_selection_bounds.unwrap_or(area);
        let (start, end) = selection.normalized();
        if start.y >= bounds.bottom() || end.y < bounds.y {
            return;
        }
        let style = crate::design_system::QuiverTheme::for_variant(self.theme_variant)
            .typography
            .style(
                TextRole::Selected,
                self.home_palette().theme,
                self.home_palette().bg,
            );
        for y in start.y.max(bounds.y)..=end.y.min(bounds.bottom().saturating_sub(1)) {
            let x_start = if y == start.y { start.x } else { bounds.x };
            let x_end = if y == end.y {
                end.x
            } else {
                bounds.right().saturating_sub(1)
            };
            if x_start > x_end {
                continue;
            }
            let Some((trimmed_start, trimmed_end)) = selection::selectable_row_range(
                self.screen_text
                    .get(y as usize)
                    .map(String::as_str)
                    .unwrap_or(""),
                x_start.max(bounds.x) as usize,
                x_end.min(bounds.right().saturating_sub(1)) as usize,
            ) else {
                continue;
            };
            for x in trimmed_start as u16..=trimmed_end as u16 {
                frame.buffer_mut()[(x, y)].set_style(style);
            }
        }
    }

    fn capture_screen_text(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let mut lines = Vec::with_capacity(area.height as usize);
        for y in area.top()..area.bottom() {
            let mut line = String::new();
            for x in area.left()..area.right() {
                line.push_str(frame.buffer_mut()[(x, y)].symbol());
            }
            lines.push(line);
        }
        self.screen_text = lines;
    }

    fn screen_point_is_text(&self, point: ScreenPoint) -> bool {
        self.screen_text
            .get(point.y as usize)
            .and_then(|line| line.chars().nth(point.x as usize))
            .is_some_and(selection::is_selectable_text_char)
    }

    fn render_global_overlays(&mut self, frame: &mut Frame) {
        if self.attempt_modal_open {
            self.render_attempts_modal(frame);
        }
        if self.file_picker_open {
            self.render_file_picker(frame);
        }
        if self.surface == AppSurface::Diff && self.diff_buffer.help_visible() {
            self.render_diff_help_overlay(frame);
        }
    }

    fn diff_inline_blocks(&self) -> Vec<DiffInlineBlock> {
        self.diff_inline_blocks_for_area(None)
    }

    fn diff_inline_blocks_for_area(&self, diff_body: Option<Rect>) -> Vec<DiffInlineBlock> {
        let mode = self.diff_buffer.viewer().viewport.mode;
        let mut blocks = self
            .session
            .notes
            .iter()
            .filter(|note| {
                self.comment_modal
                    .as_ref()
                    .and_then(|modal| modal.edit_note_id)
                    != Some(note.id)
            })
            .filter(|note| note.parent_id.is_none())
            .filter_map(|note| self.note_inline_block(note, mode, diff_body))
            .collect::<Vec<_>>();
        if let Some(block) = self.editor_inline_block(mode, diff_body) {
            blocks.push(block);
        }
        blocks
    }

    fn note_inline_block(
        &self,
        note: &ReviewNote,
        mode: DiffMode,
        diff_body: Option<Rect>,
    ) -> Option<DiffInlineBlock> {
        let after_row = self.document.line_row(
            mode,
            note.target.end.file_index,
            note.target.end.hunk_index,
            note.target.end.line_index,
        )?;
        let body = self.note_inline_body(note);
        let text_width = self.inline_text_width(diff_body, note.target.side());
        let expanded = self.expanded_review_threads.contains(&note.id);
        Some(DiffInlineBlock {
            id: format!("note:{}", note.id),
            after_row,
            side: note.target.side(),
            height: inline_comment_visual_line_count(&body, text_width)
                .saturating_add(2)
                .min(if expanded { 14 } else { 8 }),
            kind: DiffInlineBlockKind::Comment,
            accent: inline_block_accent_for_review_kind(note.kind),
            title: format!("{} · {}", note.kind.label(), note.author),
            body,
        })
    }

    fn note_inline_body(&self, note: &ReviewNote) -> String {
        let replies = self
            .session
            .notes
            .iter()
            .filter(|reply| reply.parent_id == Some(note.id))
            .collect::<Vec<_>>();
        if replies.is_empty() {
            return note.body.clone();
        }
        if !self.expanded_review_threads.contains(&note.id) {
            return format!(
                "{}\n{} {} · enter expand · r reply",
                note.body,
                replies.len(),
                if replies.len() == 1 {
                    "reply"
                } else {
                    "replies"
                }
            );
        }

        let mut lines = vec![note.body.clone()];
        for reply in replies {
            lines.push(format!("── reply · {}", reply.author));
            lines.push(reply.body.clone());
        }
        lines.join("\n")
    }

    fn editor_inline_block(
        &self,
        mode: DiffMode,
        diff_body: Option<Rect>,
    ) -> Option<DiffInlineBlock> {
        let modal = self.comment_modal.as_ref()?;
        let after_row = self.document.line_row(
            mode,
            modal.target.end.file_index,
            modal.target.end.hunk_index,
            modal.target.end.line_index,
        )?;
        let body = modal.lines.join("\n");
        let text_width = self.inline_text_width(diff_body, modal.target.side());
        Some(DiffInlineBlock {
            id: "draft".to_string(),
            after_row,
            side: modal.target.side(),
            height: inline_comment_visual_line_count(&body, text_width).saturating_add(2),
            kind: DiffInlineBlockKind::Editor,
            accent: DiffInlineBlockAccent::Draft,
            title: if modal.edit_note_id.is_some() {
                format!("{} · {}", modal.kind.label(), modal.kind.default_author())
            } else {
                format!("{} draft", modal.kind.label())
            },
            body,
        })
    }

    fn inline_text_width(&self, diff_body: Option<Rect>, side: DiffSide) -> usize {
        diff_body
            .map(|area| {
                inline_block_text_width(self.diff_buffer.viewer().viewport.pane_rect(area, side))
            })
            .unwrap_or(INLINE_COMMENT_TEXT_WIDTH)
    }

    fn render_diff_help_overlay(&self, frame: &mut Frame) {
        let area = frame.area();
        let width = 52u16.min(area.width.saturating_sub(4)).max(20);
        let height = 20u16.min(area.height.saturating_sub(4)).max(8);
        if width < 20 || height < 8 {
            return;
        }
        let x = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        let rect = Rect::new(x, y, width, height);
        let palette = self.home_palette();
        let style = Style::new().fg(palette.fg).bg(palette.bg);
        fill_rect(frame.buffer_mut(), rect, " ", style);
        draw_box(
            frame.buffer_mut(),
            rect,
            Style::new().fg(palette.rule).bg(palette.bg),
        );
        let lines = [
            "lazydiff diff keys",
            "",
            "j/k, arrows    move cursor",
            "h/l            move horizontally within active side",
            "Tab            switch split side on same row",
            "gg / G         top / bottom",
            "Home / $       line start / end",
            "Ctrl-d/u       half-page",
            "Ctrl-p         command palette",
            "[ / ]          previous / next file",
            "v / V          visual / visual-line",
            "i/a + object   text objects (iw, aw, brackets)",
            "/, n, N        search",
            "s              toggle split/unified",
            "space t        toggle file tree",
            "enter          open thread",
            "i              comment",
            "x / dd         delete note",
            ":w, :q, :q!    save / quit",
            "? / esc        close help",
        ];
        for (index, line) in lines.iter().enumerate() {
            let row = rect.y + 1 + index as u16;
            if row >= rect.bottom().saturating_sub(1) {
                break;
            }
            let line_style = if index == 0 {
                Style::new().fg(palette.accent).bg(palette.bg)
            } else {
                style
            };
            frame
                .buffer_mut()
                .set_string(rect.x + 2, row, *line, line_style);
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
        let rows = row_count_for_mode(&self.document, self.diff_buffer.viewer().viewport.mode);
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
            self.handle_file_picker_key(key, rows);
            return;
        }
        if self.handle_panel_jump_key(key.code, rows) {
            return;
        }
        if self.handle_pane_navigation_key(key) {
            return;
        }
        if self.surface == AppSurface::Diff {
            match key.code {
                KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if self.diff_buffer.mode() == DiffBufferMode::Search {
                        // In / search, Ctrl-H is Backspace. Let the diff buffer edit the query
                        // instead of treating it as horizontal pane scroll.
                    } else {
                        self.scroll_active_pane_horizontally(-8);
                        return;
                    }
                }
                // Most terminals encode Ctrl-H as ASCII backspace, so crossterm
                // may surface it as Backspace instead of Char('h') + CONTROL.
                KeyCode::Backspace
                    if key.modifiers.is_empty()
                        || key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    if self.diff_buffer.mode() == DiffBufferMode::Search {
                        // In / search, Backspace edits the query.
                    } else {
                        self.scroll_active_pane_horizontally(-8);
                        return;
                    }
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroll_active_pane_horizontally(8);
                    return;
                }
                _ => {}
            }
            if self.review_sidebar_focus && self.review_sidebar_visible {
                self.handle_review_sidebar_key(key.code, rows);
                return;
            }
            if self.handle_inline_note_edit_key(key.code) {
                return;
            }
            if self.handle_inline_focus_key(key.code, rows) {
                return;
            }
            if self.handle_diff_buffer_key(key, rows) {
                return;
            }
        }
        if matches!(self.surface, AppSurface::Queue | AppSurface::DetailFull) {
            match key.code {
                KeyCode::Enter
                    if self.surface == AppSurface::Queue
                        && self.queue_focus == QueuePane::Detail
                        && self.semantic_panel_active() =>
                {
                    if self.open_selected_semantic_row() {
                        return;
                    }
                }
                KeyCode::Char(' ') if self.semantic_panel_active() => {
                    if self.toggle_selected_semantic_viewed() {
                        return;
                    }
                }
                KeyCode::Enter
                    if self.surface == AppSurface::DetailFull && self.semantic_panel_active() =>
                {
                    if self.open_selected_semantic_row() {
                        return;
                    }
                }
                KeyCode::Char('1') if self.detail_shortcuts_active() => {
                    self.set_detail_tab(DetailTab::Semantic);
                    return;
                }
                KeyCode::Char('2') if self.detail_shortcuts_active() => {
                    self.set_detail_tab(DetailTab::Description);
                    return;
                }
                KeyCode::Char('3') if self.detail_shortcuts_active() => {
                    self.set_detail_tab(DetailTab::Graph);
                    return;
                }
                KeyCode::Char('0')
                    if self.detail_shortcuts_active() && self.detail_tab == DetailTab::Graph =>
                {
                    self.reset_semantic_map_view();
                    return;
                }
                KeyCode::Char('[')
                    if self.detail_shortcuts_active() && self.semantic_panel_active() =>
                {
                    self.collapse_focused_semantic_branch();
                    return;
                }
                KeyCode::Char(']')
                    if self.detail_shortcuts_active() && self.semantic_panel_active() =>
                {
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
        if key.modifiers.is_empty()
            && self.semantic_map_keyboard_active()
            && matches!(
                key.code,
                KeyCode::Char('h')
                    | KeyCode::Char('j')
                    | KeyCode::Char('k')
                    | KeyCode::Char('l')
                    | KeyCode::Left
                    | KeyCode::Down
                    | KeyCode::Up
                    | KeyCode::Right
            )
            && self.move_semantic_selection_structural(key.code)
        {
            return;
        }
        if let Some(command) = self.command_for_key(key) {
            self.execute_command(command, rows);
            return;
        }
        if self.surface == AppSurface::Diff {
            self.handle_plain_key(key.code, rows);
        }
    }

    fn apply_cursor_style(&self, terminal: &mut Tui) -> Result<()> {
        let style = match self.comment_modal.as_ref().map(|modal| modal.mode) {
            Some(CommentEditorMode::Insert) => SetCursorStyle::SteadyBar,
            Some(CommentEditorMode::Normal) => SetCursorStyle::SteadyBlock,
            Some(CommentEditorMode::Visual | CommentEditorMode::VisualLine) => {
                SetCursorStyle::SteadyUnderScore
            }
            None => SetCursorStyle::DefaultUserShape,
        };
        execute!(terminal.backend_mut(), style)?;
        Ok(())
    }

    fn debug_key_event(key: KeyEvent) {
        Self::debug_input_event(format_args!(
            "key code={:?} modifiers={:?} kind={:?} state={:?}",
            key.code, key.modifiers, key.kind, key.state
        ));
    }

    fn debug_input_event(args: std::fmt::Arguments<'_>) {
        if std::env::var_os("LAZYDIFF_KEY_DEBUG").is_none() {
            return;
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/lazydiff-key-debug.log")
        {
            let _ = writeln!(file, "{args}");
        }
    }

    fn handle_pane_navigation_key(&mut self, key: KeyEvent) -> bool {
        let shift_tab = matches!(key.code, KeyCode::BackTab)
            || (matches!(key.code, KeyCode::Tab | KeyCode::Char('\t'))
                && key.modifiers.contains(KeyModifiers::SHIFT));
        let plain_tab = matches!(key.code, KeyCode::Tab | KeyCode::Char('\t'))
            && !key.modifiers.contains(KeyModifiers::SHIFT);
        if !shift_tab && !plain_tab {
            return false;
        }
        if shift_tab {
            self.move_pane_focus(-1)
        } else {
            self.move_pane_focus(1)
        }
    }

    fn handle_panel_jump_key(&mut self, code: KeyCode, rows: usize) -> bool {
        if self.surface == AppSurface::Diff
            && matches!(
                self.diff_buffer.mode(),
                DiffBufferMode::Search
                    | DiffBufferMode::Command
                    | DiffBufferMode::PendingTextObject(_)
            )
        {
            return false;
        }
        match code {
            KeyCode::Char('1') if self.surface == AppSurface::Diff => {
                self.set_diff_focus(DiffFocusPane::Sidebar);
                true
            }
            KeyCode::Char('1') => {
                self.replace_route(AppRoute::Queue);
                true
            }
            KeyCode::Char('2') if self.surface == AppSurface::Diff => {
                self.set_diff_focus(DiffFocusPane::Sidebar);
                true
            }
            KeyCode::Char('2') => {
                self.open_selected_diff();
                self.set_diff_focus(DiffFocusPane::Sidebar);
                true
            }
            KeyCode::Char('3') => {
                self.open_inbox(rows);
                true
            }
            KeyCode::Char('4') => {
                self.open_commit_list_for_current_context();
                true
            }
            KeyCode::Char('0') if self.surface == AppSurface::Diff => {
                self.set_diff_focus(self.current_diff_focus().non_sidebar());
                true
            }
            KeyCode::Char('0') => {
                self.open_selected_diff();
                self.set_diff_focus(self.current_diff_focus().non_sidebar());
                true
            }
            _ => false,
        }
    }

    fn handle_diff_buffer_key(&mut self, key: KeyEvent, rows: usize) -> bool {
        let action = self.diff_buffer.handle_key(key, Instant::now());
        self.apply_diff_buffer_action(action, rows);
        true
    }

    fn apply_diff_buffer_action(&mut self, action: DiffBufferAction, rows: usize) {
        match action {
            DiffBufferAction::None => {}
            DiffBufferAction::Cancel => {
                if self.diff_buffer.viewer().selection.is_some() {
                    self.diff_buffer.viewer_mut().clear_selection();
                } else if !self.diff_buffer.search_query().is_empty() {
                    self.diff_buffer.clear_search_matches();
                    self.diff_buffer.clear_transient();
                } else {
                    self.go_back();
                }
            }
            DiffBufferAction::MoveRows(delta) => self.move_diff_cursor_rows(delta, rows),
            DiffBufferAction::MoveCols(delta) => self.move_diff_cursor_cols(delta, rows),
            DiffBufferAction::WordForward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::NextStart { big: false });
            }
            DiffBufferAction::BigWordForward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::NextStart { big: true });
            }
            DiffBufferAction::WordEndForward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::NextEnd { big: false });
            }
            DiffBufferAction::BigWordEndForward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::NextEnd { big: true });
            }
            DiffBufferAction::WordBackward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::PreviousStart { big: false });
            }
            DiffBufferAction::BigWordBackward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::PreviousStart { big: true });
            }
            DiffBufferAction::WordEndBackward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::PreviousEnd { big: false });
            }
            DiffBufferAction::BigWordEndBackward => {
                self.diff_buffer
                    .viewer_mut()
                    .move_word(&self.document, DiffWordMotion::PreviousEnd { big: true });
            }
            DiffBufferAction::Page(direction) => {
                self.diff_buffer
                    .viewer_mut()
                    .page(&self.document, direction);
            }
            DiffBufferAction::HalfPage(direction) => self.move_active_half_page(direction, rows),
            DiffBufferAction::Top => {
                self.diff_buffer.viewer_mut().top(&self.document);
            }
            DiffBufferAction::Bottom => {
                self.diff_buffer.viewer_mut().bottom(&self.document);
            }
            DiffBufferAction::LineStart => {
                self.diff_buffer
                    .viewer_mut()
                    .cursor_line_start(&self.document);
            }
            DiffBufferAction::LineEnd => {
                self.diff_buffer
                    .viewer_mut()
                    .cursor_line_end(&self.document);
            }
            DiffBufferAction::PreviousFile => self.jump_relative_file(-1, rows),
            DiffBufferAction::NextFile => self.jump_relative_file(1, rows),
            DiffBufferAction::NextChange => self.jump_relative_hunk(1, rows),
            DiffBufferAction::PreviousChange => self.jump_relative_hunk(-1, rows),
            DiffBufferAction::NextNote => self.jump_relative_note(1, rows),
            DiffBufferAction::PreviousNote => self.jump_relative_note(-1, rows),
            DiffBufferAction::ToggleSideBySide => {
                self.diff_buffer.viewer_mut().toggle_mode(&self.document);
            }
            DiffBufferAction::ToggleFileTree => self.toggle_review_sidebar(),
            DiffBufferAction::SwitchSide => {
                if !self.diff_buffer.viewer_mut().switch_side(&self.document) {
                    self.branch_operation_status =
                        Some("no matching line on other side".to_string());
                }
            }
            DiffBufferAction::OpenCommandPalette => self.open_root_palette(),
            DiffBufferAction::OpenFileFinder => self.open_file_search(),
            DiffBufferAction::SearchChanged => self.recompute_diff_buffer_search(),
            DiffBufferAction::SearchAccept => self.accept_diff_buffer_search(rows),
            DiffBufferAction::SearchNext => self.move_diff_buffer_search(1, rows),
            DiffBufferAction::SearchPrevious => self.move_diff_buffer_search(-1, rows),
            DiffBufferAction::OpenThread => {
                if !self.expand_collapsed_diff_row_under_cursor() {
                    self.open_thread_modal();
                }
            }
            DiffBufferAction::OpenEditor => self.open_current_file_in_editor(),
            DiffBufferAction::OpenQuestion => self.open_review_composer(ReviewItemKind::Question),
            DiffBufferAction::ToggleVisual => self.toggle_visual_selection(rows, false),
            DiffBufferAction::ToggleVisualLine => self.toggle_visual_selection(rows, true),
            DiffBufferAction::SelectTextObject(kind, object) => {
                let selected = self.diff_buffer.viewer_mut().select_text_object(
                    &self.document,
                    matches!(kind, TextObjectKind::Around),
                    object,
                );
                if !selected {
                    self.branch_operation_status = Some(match kind {
                        TextObjectKind::_Inner => format!("inner {object} text object not found"),
                        TextObjectKind::Around => format!("around {object} text object not found"),
                    });
                }
            }
            DiffBufferAction::YankSelection => self.yank_diff_selection(),
            DiffBufferAction::OpenComment => self.open_review_composer(ReviewItemKind::Note),
            DiffBufferAction::ToggleReviewed => self.toggle_current_file_viewed(),
            DiffBufferAction::DeleteNote => self.delete_note_under_cursor(),
            DiffBufferAction::SaveComments => self.persist_review_session(),
            DiffBufferAction::Quit { force } => {
                self.quit_or_go_back(force);
            }
            DiffBufferAction::ShowHelp => self.diff_buffer.toggle_help(),
        }
    }

    fn expand_collapsed_diff_row_under_cursor(&mut self) -> bool {
        let viewer = self.diff_buffer.viewer();
        let mode = viewer.viewport.mode;
        let row = viewer.cursor.row;
        if !self.document.is_collapsed_row(mode, row) {
            return false;
        }
        if self.document.expand_collapsed_row(mode, row) {
            self.diff_buffer
                .viewer_mut()
                .focus_row_ensure_visible(&self.document, row);
        } else {
            self.request_sources_for_collapsed_row(mode, row);
            self.branch_operation_status =
                Some("loading unchanged lines for this file… press expand again".into());
        }
        true
    }

    fn request_sources_for_collapsed_row(&mut self, mode: DiffMode, row: usize) {
        let Some(file_index) = self.document.row_file_index(mode, row) else {
            return;
        };
        if self.document.file_has_expansion_sources(file_index) {
            return;
        }
        let Some(file) = self.document.files.get(file_index) else {
            return;
        };
        let old_path = file.old_path.clone();
        let new_path = file.new_path.clone();
        let window = HighlightFrameWindow {
            route: self.diff_source.clone(),
            mode,
            viewport: Rect::default(),
            visual_start: row,
            visual_end: row.saturating_add(1),
            overscan: 0,
            inline_layout_hash: 0,
            file_indices: vec![file_index],
            jobs: vec![HighlightFileJob {
                file_index,
                old_path,
                new_path,
                old_line_window: None,
                new_line_window: None,
            }],
            visible_job_count: 1,
        };
        if let Some(envelope) = self.highlight_coordinator.visible_window_changed(window) {
            let _ = self.highlight_worker_tx.send(envelope);
        }
    }

    fn update_current_route_document_cache(&mut self) {
        match &self.diff_source {
            DiffSource::LocalWorktree(_) => {
                self.local_document = self.document.clone();
            }
            DiffSource::PullRequest { repository, number } => {
                self.pr_diff_cache
                    .insert((repository.clone(), *number), self.document.clone());
            }
            DiffSource::Commit { .. } => {}
        }
    }

    fn schedule_full_diff_highlight(&self, route: DiffSource, document: DiffDocument) {
        let _ = (route, document);
    }

    fn dispatch_pending_highlight_window(&mut self) {
        let Some(window) = self.pending_highlight_window.take() else {
            return;
        };
        let Some(envelope) = self.highlight_coordinator.visible_window_changed(window) else {
            return;
        };
        highlight_trace(&format!(
            "request {} generation {:?} jobs={}",
            envelope.token.request_id,
            envelope.token.generation,
            envelope.jobs.len()
        ));
        let _ = self.highlight_worker_tx.send(envelope);
    }

    fn highlight_file_jobs(
        &self,
        file_indices: &[usize],
        windows: &[lazydiff_diffs::DiffFileLineWindow],
    ) -> Vec<HighlightFileJob> {
        file_indices
            .iter()
            .copied()
            .filter_map(|file_index| {
                if self.document.file_has_source_syntax(file_index) {
                    return None;
                }
                let file = self.document.files.get(file_index)?;
                Some(HighlightFileJob {
                    file_index,
                    old_path: file.old_path.clone(),
                    new_path: file.new_path.clone(),
                    old_line_window: windows
                        .iter()
                        .find(|window| window.file_index == file_index)
                        .and_then(|window| line_window(window.old_start, window.old_end)),
                    new_line_window: windows
                        .iter()
                        .find(|window| window.file_index == file_index)
                        .and_then(|window| line_window(window.new_start, window.new_end)),
                })
            })
            .collect()
    }

    fn highlight_file_indices_for_window(
        &self,
        visible_file_indices: &[usize],
        document_rows: &[usize],
    ) -> Vec<usize> {
        let mut file_indices = Vec::new();
        let mut seen = HashSet::new();
        for file_index in visible_file_indices.iter().copied() {
            if seen.insert(file_index) {
                file_indices.push(file_index);
            }
        }
        let Some(first_row) = document_rows.iter().min().copied() else {
            return file_indices;
        };
        let Some(last_row) = document_rows.iter().max().copied() else {
            return file_indices;
        };
        let start = first_row.saturating_sub(HIGHLIGHT_PREFETCH_DOCUMENT_ROWS);
        let end = last_row.saturating_add(HIGHLIGHT_PREFETCH_DOCUMENT_ROWS);
        let limit = end.saturating_sub(start).saturating_add(1);
        let nearby_file_indices = self.document.file_indices_in_row_window(
            self.diff_buffer.viewer().viewport.mode,
            start,
            limit,
        );
        for file_index in nearby_file_indices {
            if seen.insert(file_index) {
                file_indices.push(file_index);
                if file_indices.len() >= HIGHLIGHT_PREFETCH_FILE_LIMIT {
                    break;
                }
            }
        }
        file_indices
    }

    fn hydrate_cached_expansion_highlights_for_jobs(&mut self, jobs: &[HighlightFileJob]) -> bool {
        let files = jobs
            .iter()
            .filter_map(|job| {
                if !self.document.file_has_expansion_sources(job.file_index)
                    || self.document.file_has_source_syntax(job.file_index)
                {
                    return None;
                }
                let (old_source, new_source) =
                    self.document.file_expansion_sources(job.file_index)?;
                Some(HighlightFileRequest {
                    file_index: job.file_index,
                    old_path: job.old_path.clone(),
                    path: job.new_path.clone(),
                    old_source,
                    new_source,
                    old_line_window: None,
                    new_line_window: None,
                })
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            return false;
        }
        let response = highlight_daemon::cached_highlights(&HighlightRequest {
            request_id: 0,
            files,
        });
        if response.files.is_empty() {
            return false;
        }
        highlight_trace(&format!(
            "prepaint cache hydrated {} expanded files",
            response.files.len()
        ));
        self.apply_highlight_response_files(response.files);
        true
    }

    fn apply_highlighted_files(&mut self, token: HighlightToken, response: HighlightResponse) {
        if !self
            .highlight_coordinator
            .should_apply(&token, &self.diff_source)
        {
            return;
        }
        self.apply_highlight_response_files(response.files);
        self.update_current_route_document_cache();
    }

    fn apply_highlight_response_files(
        &mut self,
        files: Vec<highlight_daemon::HighlightFileResponse>,
    ) {
        for file in files {
            let Some(current) = self.document.files.get(file.file_index) else {
                continue;
            };
            if !highlight_response_matches_file(current, &file) {
                continue;
            }
            self.document.apply_file_source_syntax_window(
                file.file_index,
                file.old_source_lines,
                file.new_source_lines,
                file.old_line_window
                    .map(|window| (window.start, window.end)),
                file.new_line_window
                    .map(|window| (window.start, window.end)),
                file.old_spans.map(wire_span_lines_to_syntax),
                file.new_spans.map(wire_span_lines_to_syntax),
            );
        }
    }

    fn toggle_review_sidebar(&mut self) {
        self.review_sidebar_visible = !self.review_sidebar_visible;
        if !self.review_sidebar_visible && self.review_sidebar_focus {
            self.set_diff_focus(self.current_diff_focus().non_sidebar());
        }
    }

    fn recompute_diff_buffer_search(&mut self) {
        self.diff_buffer
            .viewer_mut()
            .recompute_search(&self.document);
    }

    fn accept_diff_buffer_search(&mut self, rows: usize) {
        self.recompute_diff_buffer_search();
        if self.diff_buffer.search_matches().is_empty() {
            self.branch_operation_status = Some("pattern not found".to_string());
            return;
        }
        self.move_diff_buffer_search(1, rows);
    }

    fn move_diff_buffer_search(&mut self, delta: isize, rows: usize) {
        let _ = rows;
        self.diff_buffer
            .viewer_mut()
            .move_search_match(&self.document, delta);
    }

    fn jump_relative_note(&mut self, delta: isize, rows: usize) {
        let mut note_rows = self
            .session
            .notes
            .iter()
            .filter_map(|note| {
                self.document
                    .line_row(
                        self.diff_buffer.viewer().viewport.mode,
                        note.target.start.file_index,
                        note.target.start.hunk_index,
                        note.target.start.line_index,
                    )
                    .map(|row| (row, note.target.side()))
            })
            .collect::<Vec<_>>();
        note_rows.sort_unstable_by_key(|(row, side)| (*row, side_sort_key(*side)));
        note_rows.dedup();
        if note_rows.is_empty() {
            return;
        }
        let index = if delta < 0 {
            note_rows
                .iter()
                .rposition(|(row, _)| *row < self.diff_buffer.viewer().cursor.row)
                .unwrap_or(note_rows.len() - 1)
        } else {
            note_rows
                .iter()
                .position(|(row, _)| *row > self.diff_buffer.viewer().cursor.row)
                .unwrap_or(0)
        };
        let (row, side) = note_rows[index];
        self.diff_buffer.viewer_mut().cursor.side = side;
        self.focus_row(row, rows);
    }

    fn yank_diff_selection(&mut self) {
        let Some(selection) = self.diff_buffer.viewer().selection else {
            self.branch_operation_status = Some("no visual selection".to_string());
            return;
        };
        let text = self
            .document
            .selection_text(self.diff_buffer.viewer().viewport.mode, selection);
        if text.is_empty() {
            self.branch_operation_status = Some("no visual selection".to_string());
        } else {
            self.diff_buffer.viewer_mut().flash_yank_selection();
            self.diff_buffer.clear_transient();
            self.branch_operation_status = Some("selection ready to copy".to_string());
        }
    }

    fn delete_note_under_cursor(&mut self) {
        let Some(target) = self.active_line_target() else {
            return;
        };
        let Some(index) = self
            .session
            .notes
            .iter()
            .position(|note| note.target.contains(&target))
        else {
            self.branch_operation_status = Some("no note".to_string());
            return;
        };
        let note = self.session.notes.remove(index);
        self.store.delete_note(&self.session.id, note.id);
        self.branch_operation_status = Some("note deleted".to_string());
    }

    fn persist_review_session(&mut self) {
        self.store.upsert_session(&self.session);
        self.branch_operation_status = Some("comments saved".to_string());
    }

    fn detail_shortcuts_active(&self) -> bool {
        self.surface == AppSurface::DetailFull
            || (self.surface == AppSurface::Queue && self.queue_focus == QueuePane::Detail)
    }

    fn semantic_panel_active(&self) -> bool {
        matches!(self.detail_tab, DetailTab::Semantic | DetailTab::Graph)
    }

    fn semantic_map_keyboard_active(&self) -> bool {
        self.detail_tab == DetailTab::Graph
            && (self.surface == AppSurface::DetailFull
                || (self.surface == AppSurface::Queue && self.queue_focus == QueuePane::Detail)
                || (self.surface == AppSurface::CommitList
                    && self.commit_focus == CommitPane::Detail))
    }

    fn move_pane_focus(&mut self, direction: isize) -> bool {
        match self.surface {
            AppSurface::Queue => {
                self.queue_focus = match self.queue_focus {
                    QueuePane::List => QueuePane::Detail,
                    QueuePane::Detail => QueuePane::List,
                };
                true
            }
            AppSurface::Diff => {
                self.move_diff_pane_focus(direction);
                true
            }
            AppSurface::CommitList => {
                self.commit_focus = match self.commit_focus {
                    CommitPane::List => CommitPane::Detail,
                    CommitPane::Detail => CommitPane::List,
                };
                true
            }
            AppSurface::Comments | AppSurface::DetailFull => false,
        }
    }

    fn move_diff_pane_focus(&mut self, direction: isize) {
        if self.diff_buffer.viewer().viewport.mode != DiffMode::Split {
            if self.review_sidebar_visible {
                let next = if self.current_diff_focus() == DiffFocusPane::Sidebar {
                    DiffFocusPane::Right
                } else {
                    DiffFocusPane::Sidebar
                };
                self.set_diff_focus(next);
            }
            return;
        }

        let pane_count = if self.review_sidebar_visible { 3 } else { 2 };
        let current = match self.current_diff_focus() {
            DiffFocusPane::Sidebar if self.review_sidebar_visible => 0,
            DiffFocusPane::Left => usize::from(self.review_sidebar_visible),
            DiffFocusPane::Right => usize::from(self.review_sidebar_visible).saturating_add(1),
            DiffFocusPane::Sidebar => 0,
        };
        let next = (current as isize + direction).rem_euclid(pane_count as isize) as usize;

        let focus = if self.review_sidebar_visible && next == 0 {
            DiffFocusPane::Sidebar
        } else {
            match next.saturating_sub(usize::from(self.review_sidebar_visible)) {
                0 => DiffFocusPane::Left,
                _ => DiffFocusPane::Right,
            }
        };
        self.set_diff_focus(focus);
    }

    fn current_diff_focus(&self) -> DiffFocusPane {
        self.diff_focus
    }

    fn set_diff_focus(&mut self, focus: DiffFocusPane) {
        match focus {
            DiffFocusPane::Sidebar if self.review_sidebar_visible => {
                self.diff_focus = DiffFocusPane::Sidebar;
                self.review_sidebar_focus = true;
                self.sync_review_sidebar_selection_to_current_file();
            }
            DiffFocusPane::Sidebar => {}
            DiffFocusPane::Left => {
                self.diff_focus = DiffFocusPane::Left;
                self.review_sidebar_focus = false;
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Left;
            }
            DiffFocusPane::Right => {
                self.diff_focus = DiffFocusPane::Right;
                self.review_sidebar_focus = false;
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Right;
            }
        }
    }

    /// Half-page scroll dispatcher used by ctrl-d / ctrl-u from any surface.
    fn page_surface_half(&mut self, direction: isize, rows: usize) {
        match self.surface {
            AppSurface::Queue => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_queue_focused(direction.saturating_mul(half));
            }
            AppSurface::CommitList => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_commit_focused(direction.saturating_mul(half));
            }
            AppSurface::Comments => {
                let half = (self.viewport_height.max(2) / 2).max(1) as isize;
                self.move_comments_selection(direction.saturating_mul(half));
            }
            AppSurface::DetailFull => {
                if self.semantic_panel_active() {
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

    fn diff_mode(&self) -> DiffMode {
        self.diff_buffer.viewer().viewport.mode
    }

    fn _focus_first_inline_block_after_row(&mut self, row: usize) -> bool {
        let mut blocks = self.diff_inline_blocks();
        blocks.sort_unstable_by_key(|block| (block.after_row, block.id.clone()));
        let Some(block) = blocks
            .into_iter()
            .find(|block| block.after_row == row && inline_block_body_line_range(block).is_some())
        else {
            return false;
        };
        let (first_body_line, _) = inline_block_body_line_range(&block).unwrap_or((0, 0));
        self.inline_focus = Some(InlineFocus {
            block_id: block.id,
            line: first_body_line,
        });
        true
    }

    fn focused_inline_block(&self) -> Option<DiffInlineBlock> {
        let focus = self.inline_focus.as_ref()?;
        self.diff_inline_blocks()
            .into_iter()
            .find(|block| block.id == focus.block_id)
    }

    fn handle_inline_note_edit_key(&mut self, code: KeyCode) -> bool {
        if self.comment_modal.is_some() {
            return false;
        }
        if !matches!(
            code,
            KeyCode::Char('i')
                | KeyCode::Char('a')
                | KeyCode::Char('A')
                | KeyCode::Char('o')
                | KeyCode::Char('O')
        ) {
            return false;
        }
        if let Some(note) = self.focused_inline_note() {
            self.open_existing_note_editor(note, code);
            return true;
        }
        false
    }

    fn focused_inline_note(&self) -> Option<ReviewNote> {
        let focus = self.inline_focus.as_ref()?;
        let note_id = focus
            .block_id
            .strip_prefix("note:")
            .and_then(|id| id.parse::<u64>().ok())?;
        self.session
            .notes
            .iter()
            .find(|note| note.id == note_id)
            .cloned()
    }

    fn open_existing_note_editor(&mut self, note: ReviewNote, code: KeyCode) {
        let mut modal = CommentModal::existing(&note);
        modal.row = modal.lines.len().saturating_sub(1);
        modal.col = modal.line_len();
        match code {
            KeyCode::Char('a') => modal.move_col(1),
            KeyCode::Char('A') => modal.col = modal.line_len(),
            KeyCode::Char('o') => modal.open_line_below(),
            KeyCode::Char('O') => modal.open_line_above(),
            _ => {}
        }
        modal.mode = CommentEditorMode::Insert;
        self.inline_focus = None;
        self.diff_buffer.viewer_mut().clear_selection();
        self.diff_buffer.viewer_mut().yank_selection = None;
        self.diff_buffer.viewer_mut().yank_until = None;
        self.comment_modal = Some(modal);
    }

    fn handle_inline_focus_key(&mut self, code: KeyCode, _rows: usize) -> bool {
        let Some(mut focus) = self.inline_focus.clone() else {
            return false;
        };
        let Some(block) = self.focused_inline_block() else {
            self.inline_focus = None;
            return false;
        };
        let Some((first_body_line, last_body_line)) = inline_block_body_line_range(&block) else {
            self.inline_focus = None;
            return false;
        };
        focus.line = focus.line.clamp(first_body_line, last_body_line);
        match code {
            KeyCode::Esc => {
                self.inline_focus = None;
                true
            }
            KeyCode::Enter => {
                if let Some(thread_id) = focus
                    .block_id
                    .strip_prefix("note:")
                    .and_then(|id| id.parse::<u64>().ok())
                {
                    if !self.expanded_review_threads.insert(thread_id) {
                        self.expanded_review_threads.remove(&thread_id);
                    }
                    let (first_body_line, last_body_line) = self
                        .focused_inline_block()
                        .as_ref()
                        .and_then(inline_block_body_line_range)
                        .unwrap_or((1, 1));
                    focus.line = focus.line.clamp(first_body_line, last_body_line);
                    self.inline_focus = Some(focus);
                } else {
                    self.inline_focus = None;
                    self.open_thread_modal();
                }
                true
            }
            KeyCode::Char('r') => {
                self.inline_focus = None;
                self.open_review_composer(ReviewItemKind::Note);
                true
            }
            KeyCode::Char('i')
            | KeyCode::Char('a')
            | KeyCode::Char('A')
            | KeyCode::Char('o')
            | KeyCode::Char('O') => {
                if let Some(note_id) = focus
                    .block_id
                    .strip_prefix("note:")
                    .and_then(|id| id.parse::<u64>().ok())
                    && let Some(note) = self
                        .session
                        .notes
                        .iter()
                        .find(|note| note.id == note_id)
                        .cloned()
                {
                    self.open_existing_note_editor(note, code);
                }
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if focus.line < last_body_line {
                    focus.line += 1;
                    self.inline_focus = Some(focus);
                    self.ensure_focused_diff_visual_row_visible();
                } else {
                    let block_id = focus.block_id.clone();
                    if !self.focus_adjacent_inline_block_row(&block_id, 1) {
                        self.inline_focus = None;
                    }
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if focus.line > first_body_line {
                    focus.line -= 1;
                    self.inline_focus = Some(focus);
                    self.ensure_focused_diff_visual_row_visible();
                } else {
                    let block_id = focus.block_id.clone();
                    if !self.focus_adjacent_inline_block_row(&block_id, -1) {
                        self.inline_focus = None;
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn diff_cursor_row(&self) -> usize {
        self.diff_buffer.viewer().cursor.row
    }

    fn diff_scroll_y(&self) -> usize {
        self.diff_buffer.viewer().viewport.scroll_y
    }

    fn handle_plain_key(&mut self, code: KeyCode, rows: usize) {
        match code {
            KeyCode::Esc => {
                if self.diff_buffer.viewer().selection.is_some() && self.surface == AppSurface::Diff
                {
                    self.diff_buffer.viewer_mut().clear_selection();
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
                if self.surface == AppSurface::Diff {
                    self.diff_buffer.viewer_mut().viewport.scroll_y = 0;
                }
            }
            KeyCode::Char('G') => {
                if self.surface == AppSurface::Diff {
                    let inline_blocks = self.diff_inline_blocks();
                    let visual_row_count = self
                        .diff_buffer
                        .viewer()
                        .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
                    self.diff_buffer.viewer_mut().viewport.scroll_y =
                        visual_row_count.saturating_sub(self.viewport_height);
                }
            }
            KeyCode::Char('v') => self.toggle_visual_selection(rows, false),
            KeyCode::Char(' ') => self.toggle_current_file_viewed(),
            KeyCode::Char('m') => {
                self.diff_buffer.viewer_mut().toggle_mode(&self.document);
            }
            KeyCode::Char(']') => self.jump_relative_file(1, rows),
            KeyCode::Char('[') => self.jump_relative_file(-1, rows),
            KeyCode::Char('N') => self.jump_relative_hunk(1, rows),
            KeyCode::Char('p') => self.jump_relative_hunk(-1, rows),
            KeyCode::Char('A') => self.attempt_modal_open = true,
            KeyCode::Left => {
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Left;
            }
            KeyCode::Right => {
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Right;
            }
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
            Command::Quit => self.quit_or_go_back(false),
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
            Command::_LoginForge => self.revalidate_existing_forge_login(true),
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
            Command::OpenInEditor => self.open_current_file_in_editor(),
            Command::OpenCommandPalette => self.open_root_palette(),
            Command::OpenFileSearch => self.open_file_search(),
            Command::OpenTextSearch => self.open_text_search(),
            Command::OpenInbox => self.open_inbox(rows),
            Command::OpenThread => self.open_thread_modal(),
            Command::NewQuestion => self.open_review_composer(ReviewItemKind::Question),
            Command::NewInstruction => self.open_review_composer(ReviewItemKind::Instruction),
            Command::NewNote => self.open_review_composer(ReviewItemKind::Note),
            Command::ToggleDiffMode => {
                self.diff_buffer.viewer_mut().toggle_mode(&self.document);
            }
            Command::ToggleFileTree => self.toggle_review_sidebar(),
            Command::JumpFirst => self.jump_focused_boundary(false, rows),
            Command::JumpLast => self.jump_focused_boundary(true, rows),
            Command::PreviousFile => self.jump_relative_file(-1, rows),
            Command::NextFile => self.jump_relative_file(1, rows),
            Command::PreviousHunk => self.jump_relative_hunk(-1, rows),
            Command::NextHunk => self.jump_relative_hunk(1, rows),
            Command::ShowAttempts => self.attempt_modal_open = true,
            Command::SelectLeft => {
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Left;
            }
            Command::SelectRight => {
                self.diff_buffer.viewer_mut().cursor.side = DiffSide::Right;
            }
            Command::ScrollLeft => self.scroll_active_pane_horizontally(-8),
            Command::ScrollRight => self.scroll_active_pane_horizontally(8),
            Command::OpenThemePicker => self.open_theme_picker(),
            Command::SelectTheme(theme) => self.select_theme_variant(theme),
        }
    }

    fn scroll_active_pane_horizontally(&mut self, delta: isize) {
        let before = self.diff_buffer.viewer().active_horizontal_scroll();
        match self.surface {
            AppSurface::Diff => {
                self.diff_buffer
                    .viewer_mut()
                    .scroll_active_side_horizontally(delta);
            }
            AppSurface::Queue
            | AppSurface::DetailFull
            | AppSurface::CommitList
            | AppSurface::Comments => {}
        }
        Self::debug_input_event(format_args!(
            "scroll_x delta={delta} before={before} after={} surface={:?} sidebar_focus={}",
            self.diff_buffer.viewer().active_horizontal_scroll(),
            self.surface,
            self.review_sidebar_focus
        ));
    }

    fn jump_focused_boundary(&mut self, last: bool, _rows: usize) {
        match self.surface {
            AppSurface::Queue => match self.queue_focus {
                QueuePane::List => {
                    self.home_selection = if last {
                        self.home_work_items().len().saturating_sub(1)
                    } else {
                        0
                    };
                    self.home_selection_changed_at = Instant::now();
                    self.surface_scroll_y = 0;
                    self.semantic_scroll_y = 0;
                    self.semantic_selection = 0;
                    self.revalidate_selected_semantic_diff();
                }
                QueuePane::Detail => {
                    if self.semantic_panel_active() {
                        self.semantic_selection = if last {
                            self.semantic_tree_rows(&self.current_semantic_route().unwrap_or_else(
                                || {
                                    self.selected_work_item()
                                        .map(|item| item.route(self))
                                        .unwrap_or_else(|| self.diff_source.clone())
                                },
                            ))
                            .len()
                            .saturating_sub(1)
                        } else {
                            0
                        };
                        self.semantic_scroll_y = self.semantic_selection;
                    } else {
                        self.surface_scroll_y = if last { usize::MAX / 2 } else { 0 };
                    }
                }
            },
            AppSurface::CommitList => match self.commit_focus {
                CommitPane::List => {
                    self.commit_selection = if last {
                        self.commits.len().saturating_sub(1)
                    } else {
                        0
                    };
                }
                CommitPane::Detail => {
                    self.semantic_selection = if last { usize::MAX / 2 } else { 0 };
                    self.semantic_scroll_y = self.semantic_selection;
                }
            },
            AppSurface::Comments => {
                self.comments_selection = if last {
                    self.current_comment_count().saturating_sub(1)
                } else {
                    0
                };
            }
            AppSurface::DetailFull => {
                if self.semantic_panel_active() {
                    self.semantic_selection = if last { usize::MAX / 2 } else { 0 };
                    self.semantic_scroll_y = self.semantic_selection;
                } else {
                    self.surface_scroll_y = if last { usize::MAX / 2 } else { 0 };
                }
            }
            AppSurface::Diff => {
                let inline_blocks = self.diff_inline_blocks();
                let visual_row_count = self
                    .diff_buffer
                    .viewer()
                    .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
                self.diff_buffer.viewer_mut().viewport.scroll_y = if last {
                    visual_row_count.saturating_sub(self.viewport_height)
                } else {
                    0
                };
            }
        }
    }

    fn go_back(&mut self) {
        if self.diff_buffer.viewer().selection.is_some() && self.surface == AppSurface::Diff {
            self.diff_buffer.viewer_mut().clear_selection();
            return;
        }
        if !self.history.can_go_back() {
            return;
        }
        let route = self.history.go(-1).clone();
        self.apply_route(route);
    }

    fn quit_or_go_back(&mut self, force: bool) {
        let _ = force;
        self.should_quit = true;
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
            AppSurface::Queue => self.move_queue_focused(1),
            AppSurface::CommitList => self.move_commit_focused(1),
            AppSurface::Comments => self.move_comments_selection(1),
            AppSurface::DetailFull => {
                if self.semantic_panel_active() {
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
            AppSurface::Queue => self.move_queue_focused(-1),
            AppSurface::CommitList => self.move_commit_focused(-1),
            AppSurface::Comments => self.move_comments_selection(-1),
            AppSurface::DetailFull => {
                if self.semantic_panel_active() {
                    self.move_semantic_selection(-1);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_sub(1)
                }
            }
            AppSurface::Diff => self.move_active_relative(-1, rows),
        }
    }

    fn move_queue_focused(&mut self, delta: isize) {
        match self.queue_focus {
            QueuePane::List => self.move_home_selection(delta),
            QueuePane::Detail => {
                if self.semantic_panel_active() {
                    self.move_semantic_selection(delta);
                } else {
                    self.surface_scroll_y = self.surface_scroll_y.saturating_add_signed(delta);
                }
            }
        }
    }

    fn move_commit_focused(&mut self, delta: isize) {
        match self.commit_focus {
            CommitPane::List => self.move_commit_selection(delta),
            CommitPane::Detail => self.move_semantic_selection(delta),
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
                self.move_commit_focused(direction * self.viewport_height.max(1) as isize)
            }
            AppSurface::Comments => {
                self.move_comments_selection(direction * self.viewport_height.max(1) as isize)
            }
            AppSurface::DetailFull => {
                if self.semantic_panel_active() {
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
                self.move_queue_focused(direction * self.viewport_height.max(1) as isize)
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

    fn home_wide_queue_area(&self, terminal_width: u16, terminal_height: u16) -> Option<Rect> {
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
        let [queue, _gap, _details] = Layout::horizontal([
            Constraint::Percentage(58),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(body);
        Some(queue)
    }

    fn grouped_work_item_rows(
        &self,
        items: &[WorkItem],
        area: Rect,
        start_y: u16,
    ) -> Vec<GroupedWorkItemRow> {
        let mut builder = ListGeometryBuilder::new(area, start_y);
        let mut rows = Vec::new();
        let mut previous_group: Option<&str> = None;
        for (index, item) in items.iter().enumerate() {
            if previous_group != Some(item.group.as_str()) {
                if previous_group.is_some() {
                    builder.gap();
                }
                if let Some(geometry) = builder.header() {
                    rows.push(GroupedWorkItemRow::Header {
                        label: item.group.clone(),
                        geometry,
                    });
                }
                previous_group = Some(item.group.as_str());
            }
            if let Some(geometry) = builder.item(index) {
                rows.push(GroupedWorkItemRow::Item { index, geometry });
            }
            if rows.last().is_some_and(|row| row.area().y >= area.bottom()) {
                break;
            }
        }
        rows.into_iter()
            .filter(|row| row.area().y < area.bottom())
            .collect()
    }

    fn home_wide_queue_rows(
        &self,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<Vec<GroupedWorkItemRow>> {
        let queue = self.home_wide_queue_area(terminal_width, terminal_height)?;
        let items = self.home_work_items();
        let start_y = queue.y.saturating_add(2);
        Some(self.grouped_work_item_rows(
            &items,
            Rect::new(
                queue.x,
                queue.y,
                queue.width.saturating_sub(1),
                queue.height,
            ),
            start_y,
        ))
    }

    pub(crate) fn _theme_variant(&self) -> crate::design_system::ThemeVariant {
        self.theme_variant
    }

    pub(crate) fn home_palette(&self) -> HomePalette {
        HomePalette::for_variant(self.theme_variant)
    }

    pub(crate) fn finder_palette(&self) -> FinderPalette {
        FinderPalette::for_variant(self.theme_variant)
    }

    /// Select a theme without mutating the parsed/highlighted document. The
    /// diff body's Pierre spans are theme-independent; this only changes
    /// structural UI/diff colors and persists the chosen Lumen preset.
    pub(crate) fn select_theme_variant(&mut self, theme: crate::design_system::ThemeVariant) {
        self.theme_variant = theme;
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

    pub(crate) fn _home_selection_is_settled(&self) -> bool {
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
        if let Ok(value) = std::env::var("LAZYDIFF_PROJECT")
            && !value.trim().is_empty()
        {
            return Some(normalize_project_label(&value));
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

    fn load_local_worktree_diff(
        route: LocalWorktreeRoute,
    ) -> std::result::Result<LocalDiffResult, String> {
        let cwd = PathBuf::from(&route.repo_path);
        let repo_path = git_stdout_in(&cwd, ["rev-parse", "--show-toplevel"])
            .unwrap_or_else(|| route.repo_path.clone());
        let branch = std::env::var("LAZYDIFF_BRANCH")
            .ok()
            .filter(|branch| !branch.trim().is_empty())
            .or_else(|| git_stdout_in(&cwd, ["branch", "--show-current"]))
            .or_else(|| git_stdout_in(&cwd, ["rev-parse", "--abbrev-ref", "HEAD"]))
            .filter(|branch| branch != "HEAD")
            .unwrap_or(route.branch);
        let base_ref = (!route.base_ref.trim().is_empty())
            .then_some(route.base_ref)
            .or_else(|| std::env::var("LAZYDIFF_BASE_REF").ok())
            .unwrap_or_else(|| "HEAD".to_string());
        let patch = git_stdout_result_in(
            &cwd,
            ["diff", "--no-ext-diff", "--binary", base_ref.as_str()],
        )?;
        let document =
            Self::materialize_local_diff_document(&patch, Path::new(&repo_path), &base_ref);
        Ok(LocalDiffResult {
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

    fn load_commit_diff(
        repo_path: &str,
        sha: &str,
        forge: &dyn Forge,
    ) -> std::result::Result<DiffDocument, String> {
        if let Some(repository) = repo_path.strip_prefix("forge:") {
            let patch = forge.fetch_commit_patch(repository, sha)?;
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
        Ok(Self::materialize_commit_diff_document(
            &patch,
            Path::new(repo_path),
            sha,
        ))
    }

    fn load_pull_request_diff_via_forge(
        forge: &dyn Forge,
        repository: &str,
        number: u32,
    ) -> std::result::Result<PullRequestDiffResult, String> {
        let patch = forge.fetch_pull_request_patch(repository, number)?;
        let document =
            Self::materialize_pull_request_diff_document(forge, repository, number, &patch);
        Ok(PullRequestDiffResult { patch, document })
    }

    fn materialize_pull_request_diff_document(
        forge: &dyn Forge,
        repository: &str,
        number: u32,
        patch: &str,
    ) -> DiffDocument {
        let mut document = parse_unified_diff(patch);
        let paths = document
            .files
            .iter()
            .flat_map(|file| [file.old_path.clone(), Some(file.new_path.clone())])
            .flatten()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let sources = forge
            .fetch_pull_request_file_sources(repository, number, &paths)
            .unwrap_or_default();
        let file_limit = (document.files.len() > SOURCE_AWARE_HIGHLIGHT_FILE_LIMIT)
            .then_some(SOURCE_AWARE_HIGHLIGHT_FILE_LIMIT);
        add_pierre_highlights_with_sources_and_limit(&mut document, file_limit, |file, side| {
            let source = pull_request_file_source(&sources, file, side)?;
            match side {
                DiffSide::Left => source.old.clone(),
                DiffSide::Right => source.new.clone(),
            }
        });
        document
    }

    fn materialize_diff_document(patch: &str) -> DiffDocument {
        let mut document = parse_unified_diff(patch);
        add_pierre_highlights(&mut document);
        document
    }

    fn materialize_local_diff_document(
        patch: &str,
        repo_path: &Path,
        base_ref: &str,
    ) -> DiffDocument {
        let mut document = parse_unified_diff(patch);
        if document.files.len() > SOURCE_AWARE_HIGHLIGHT_FILE_LIMIT {
            add_pierre_highlights_with_limit(&mut document, SOURCE_AWARE_HIGHLIGHT_FILE_LIMIT);
            return document;
        }
        add_pierre_highlights_with_sources(&mut document, |file, side| match side {
            DiffSide::Left => file
                .old_path
                .as_deref()
                .and_then(|path| git_blob_at(repo_path, local_diff_old_ref(base_ref), path)),
            DiffSide::Right if base_ref == "--cached" => {
                git_index_blob_at(repo_path, &file.new_path)
            }
            DiffSide::Right => std::fs::read_to_string(repo_path.join(&file.new_path)).ok(),
        });
        document
    }

    fn materialize_commit_diff_document(patch: &str, repo_path: &Path, sha: &str) -> DiffDocument {
        let mut document = parse_unified_diff(patch);
        let parent = format!("{sha}^");
        add_pierre_highlights_with_sources(&mut document, |file, side| match side {
            DiffSide::Left => file
                .old_path
                .as_deref()
                .and_then(|path| git_blob_at(repo_path, &parent, path)),
            DiffSide::Right => git_blob_at(repo_path, sha, &file.new_path),
        });
        document
    }

    fn replace_document_preserving_view(&mut self, document: DiffDocument) {
        self.document = document;
        self.highlight_coordinator.document_replaced();
        let mode = self.diff_buffer.viewer().viewport.mode;
        let rows = row_count_for_mode(&self.document, mode);
        let viewer = self.diff_buffer.viewer_mut();
        viewer.cursor.row = viewer.cursor.row.min(rows.saturating_sub(1));
        viewer.viewport.scroll_y = viewer
            .viewport
            .scroll_y
            .min(rows.saturating_sub(self.viewport_height));
        viewer.clear_selection();
    }

    fn home_work_items(&self) -> Vec<WorkItem> {
        let project = self.project_label();
        let your_work_label = match project.as_deref() {
            Some(project) => format!("your work · {project}"),
            None => "your work".to_string(),
        };
        let worktrees = self.worktrees_for_queue();
        let github_indices = self.github_indices_for_queue();
        let github_items = github_indices
            .iter()
            .map(|index| self.github.items[*index].clone())
            .collect::<Vec<_>>();
        let github_items = github_items.as_slice();
        let linked_pr_ids = project
            .as_ref()
            .map(|project| link_worktree_pr(&worktrees, github_items, project))
            .unwrap_or_default();
        let pr_index_by_id: HashMap<PrId, usize> = github_items
            .iter()
            .zip(github_indices.iter().copied())
            .map(|(pr, index)| {
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
        for (filtered_index, pr) in github_items.iter().enumerate() {
            let index = github_indices[filtered_index];
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
        for (filtered_index, pr) in github_items.iter().enumerate() {
            let index = github_indices[filtered_index];
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

    fn github_indices_for_queue(&self) -> Vec<usize> {
        if !self.github_auth.can_load_github() {
            return Vec::new();
        }
        self.github
            .items
            .iter()
            .enumerate()
            .map(|(index, _)| index)
            .collect()
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
        self.github_auth = self.forge.auth_status();
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
        if let Some(route) = route {
            if route != self.local_route {
                self.local_route = route;
                self.local_document = parse_unified_diff("");
            }
        }
        let source = DiffSource::LocalWorktree(self.local_route.clone());
        self.document = self.local_document.clone();
        self.highlight_coordinator.document_replaced();
        self.push_route(AppRoute::Diff(source));
        *self.diff_buffer.viewer_mut() = Default::default();
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
            self.highlight_coordinator.document_replaced();
            *self.diff_buffer.viewer_mut() = Default::default();
            self.session = Self::load_session_for_route(&self.store, &route, &self.document);
        } else {
            self.document = parse_unified_diff("");
            self.highlight_coordinator.document_replaced();
            *self.diff_buffer.viewer_mut() = Default::default();
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

    fn open_current_file_in_editor(&mut self) {
        if self.surface != AppSurface::Diff {
            return;
        }
        let DiffSource::LocalWorktree(route) = &self.diff_source else {
            return;
        };
        let repo_path = PathBuf::from(&route.repo_path);
        if !repo_path.is_dir() {
            return;
        }

        let line_target = self.line_target_at(self.diff_cursor_row());
        let file_index = line_target
            .as_ref()
            .map(|target| target.file_index)
            .or_else(|| self.current_file_index());
        let Some(file) = file_index.and_then(|index| self.document.files.get(index)) else {
            return;
        };
        let path = if file.new_path == "/dev/null" {
            file.old_path.as_deref().unwrap_or(file.new_path.as_str())
        } else {
            file.new_path.as_str()
        };
        if path == "/dev/null" {
            return;
        }

        let file_path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            repo_path.join(path)
        };
        let line = line_target.and_then(|target| target.new_line.or(target.old_line));
        self.pending_terminal_flow = Some(TerminalFlow::OpenEditor {
            command: self.editor_command(&file_path, line, &repo_path),
            cwd: repo_path,
        });
    }

    fn editor_command(&self, path: &Path, line: Option<u32>, repo_path: &Path) -> String {
        let editor = self.guess_default_editor(repo_path);
        let preset = editor_preset_name(&editor);
        let filename = shell_quote(&path.display().to_string());
        let Some(line) = line.filter(|line| *line > 0) else {
            return match preset.as_str() {
                "code" => format!("{editor} --reuse-window -- {filename}"),
                _ => format!("{editor} -- {filename}"),
            };
        };

        match preset.as_str() {
            "code" => format!("{editor} --reuse-window --goto -- {filename}:{line}"),
            "subl" | "zed" | "hx" | "helix" => format!("{editor} -- {filename}:{line}"),
            "bbedit" => format!("{editor} +{line} -- {filename}"),
            "xed" => format!("{editor} --line {line} -- {filename}"),
            _ => format!("{editor} +{line} -- {filename}"),
        }
    }

    fn guess_default_editor(&self, repo_path: &Path) -> String {
        if let Ok(output) = ProcessCommand::new("git")
            .args(["config", "--get", "core.editor"])
            .current_dir(repo_path)
            .output()
            && output.status.success()
            && let Ok(editor) = String::from_utf8(output.stdout)
        {
            let editor = editor.trim();
            if !editor.is_empty() {
                return editor.to_string();
            }
        }

        ["GIT_EDITOR", "VISUAL", "EDITOR"]
            .into_iter()
            .find_map(|key| {
                std::env::var(key)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| "vim".to_string())
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
            let forge = Arc::clone(&self.forge);
            thread::spawn(move || {
                let result =
                    forge.fetch_pull_request_commits(&pull_request.repository, pull_request.number);
                let _ = sender.send(QueryEvent::BranchCommits {
                    context: CommitListContext::PullRequest {
                        repository: pull_request.repository,
                        number: pull_request.number,
                    },
                    result,
                });
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
                let _ = sender.send(QueryEvent::BranchCommits {
                    context: CommitListContext::Local(route),
                    result,
                });
            });
        }
    }

    fn open_commit_list_for_current_context(&mut self) {
        if self.surface == AppSurface::Queue {
            self.open_commit_list();
            return;
        }
        match self.diff_source.clone() {
            DiffSource::LocalWorktree(route) => {
                self.commit_selection = 0;
                self.commits.clear();
                self.commit_route = Some(route.clone());
                self.commit_pr_route = None;
                self.commit_status = Some("loading commits…".to_string());
                self.push_route(AppRoute::CommitList);
                let sender = self.query_tx.clone();
                thread::spawn(move || {
                    let result = list_branch_commits(Path::new(&route.repo_path), None);
                    let _ = sender.send(QueryEvent::BranchCommits {
                        context: CommitListContext::Local(route),
                        result,
                    });
                });
            }
            DiffSource::PullRequest { repository, number } => {
                if !self.ensure_github_auth() {
                    return;
                }
                self.commit_selection = 0;
                self.commits.clear();
                self.commit_route = None;
                self.commit_pr_route = Some((repository.clone(), number));
                self.commit_status = Some("loading PR commits…".to_string());
                self.push_route(AppRoute::CommitList);
                let sender = self.query_tx.clone();
                let forge = Arc::clone(&self.forge);
                thread::spawn(move || {
                    let result = forge.fetch_pull_request_commits(&repository, number);
                    let _ = sender.send(QueryEvent::BranchCommits {
                        context: CommitListContext::PullRequest { repository, number },
                        result,
                    });
                });
            }
            DiffSource::Commit { .. } => {
                self.replace_route(AppRoute::CommitList);
            }
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
            format!("forge:{repository}")
        } else {
            return;
        };
        let source = DiffSource::Commit {
            repo_path: repo_path.clone(),
            sha: commit.sha.clone(),
        };
        self.document = parse_unified_diff("");
        self.highlight_coordinator.document_replaced();
        *self.diff_buffer.viewer_mut() = Default::default();
        self.push_route(AppRoute::Diff(source));
        self.revalidate_semantic_diff(self.diff_source.clone());
        let sender = self.query_tx.clone();
        let forge = Arc::clone(&self.forge);
        thread::spawn(move || {
            let result = Self::load_commit_diff(&repo_path, &commit.sha, forge.as_ref());
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
        let forge = Arc::clone(&self.forge);
        thread::spawn(move || {
            let result = Ok(Self::materialize_pull_request_diff_document(
                forge.as_ref(),
                &repository,
                number,
                &patch,
            ));
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
        let mode = self.diff_buffer.viewer().viewport.mode;
        let scroll_y = self.diff_buffer.viewer().viewport.scroll_y;
        let rows = row_count_for_mode(&self.document, mode);
        if self.document.is_file_header_row(mode, scroll_y) {
            return self.document.row_file_index(mode, scroll_y);
        }
        self.document
            .row_file_index(mode, self.first_unobscured_visible_row(rows))
    }

    fn focus_row(&mut self, row: usize, rows: usize) {
        let _ = rows;
        self.diff_buffer.viewer_mut().focus_row(&self.document, row);
        self.center_focused_diff_visual_row();
    }

    fn focus_semantic_document_row(&mut self, row: usize) {
        self.diff_buffer
            .viewer_mut()
            .focus_row_ensure_visible(&self.document, row);
        self.ensure_focused_diff_visual_row_visible();
    }

    pub(super) fn trigger_transient_focus(&mut self, path: String, row: usize) {
        self.transient_focus = Some(TransientFocus {
            _path: path,
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

    fn render_transient_focus(&self, frame: &mut Frame, body: Rect, palette: HomePalette) {
        let Some(focus) = self.transient_focus.as_ref() else {
            return;
        };
        let elapsed = focus.started_at.elapsed();
        if elapsed >= TRANSIENT_FOCUS_DURATION {
            return;
        }
        let content_area = diff_content_area(body);
        let inline_blocks = self.diff_inline_blocks_for_area(Some(content_area));
        let viewer = self.diff_buffer.viewer();
        let side = viewer.cursor.side;
        let Some(visual_index) = viewer.visual_index_for_document_row_with_inline_blocks(
            &self.document,
            &inline_blocks,
            focus.row,
        ) else {
            return;
        };
        let Some(visual_row) =
            viewer.visual_row_at_with_inline_blocks(&self.document, &inline_blocks, visual_index)
        else {
            return;
        };
        if visual_row.row_for_side(side) != Some(focus.row)
            && visual_row.document_row() != Some(focus.row)
        {
            return;
        }
        let scroll_y = viewer.viewport.scroll_y;
        if visual_index < scroll_y {
            return;
        }
        let local_y = visual_index - scroll_y;
        if local_y >= body.height as usize {
            return;
        }
        let y = body.y + local_y as u16;
        // Use the same selected-line highlight family as the diff
        // viewer overlays instead of a separate semantic-only amber.
        // We only mutate the background of existing cells so we don't
        // stomp double-width glyphs or the gutter's line numbers.
        let progress = elapsed.as_secs_f32() / TRANSIENT_FOCUS_DURATION.as_secs_f32();
        let diff_theme = palette.theme.diff_theme();
        let highlight_bg = if progress < 0.7 {
            diff_theme.selected
        } else {
            diff_theme.panel_alt
        };
        let cursor_x = if self.comment_modal.is_none() && self.inline_focus.is_none() {
            viewer
                .cursor_screen_position(&self.document, &inline_blocks, body)
                .filter(|position| position.y == y)
                .map(|position| position.x)
        } else {
            None
        };
        let buffer = frame.buffer_mut();
        for x in body.x..body.right() {
            if cursor_x == Some(x) {
                continue;
            }
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_bg(highlight_bg);
            }
        }
    }

    fn scroll_relative(&mut self, delta: isize, rows: usize) {
        if self.surface == AppSurface::Diff {
            let inline_blocks = self.diff_inline_blocks();
            let visual_row_count = self
                .diff_buffer
                .viewer()
                .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
            let max_scroll = visual_row_count.saturating_sub(self.viewport_height);
            self.diff_buffer.viewer_mut().viewport.scroll_y = self
                .diff_buffer
                .viewer()
                .viewport
                .scroll_y
                .saturating_add_signed(delta)
                .min(max_scroll);
            return;
        }
        self.diff_buffer.viewer_mut().viewport.scroll_y = self
            .diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .saturating_add_signed(delta)
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn move_diff_cursor_rows(&mut self, delta: isize, rows: usize) {
        let _ = rows;
        if self.comment_modal.is_none() && self.move_diff_visual_row(delta) {
            return;
        }
        self.diff_buffer
            .viewer_mut()
            .move_cursor_rows(&self.document, delta);
        self.inline_focus = None;
        self.ensure_focused_diff_visual_row_visible();
    }

    fn move_diff_visual_row(&mut self, delta: isize) -> bool {
        let inline_blocks = self.diff_inline_blocks();
        let visual_row_count = self
            .diff_buffer
            .viewer()
            .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        let Some(current_index) = self.current_diff_visual_index(&inline_blocks) else {
            return false;
        };
        let next_index = current_index.saturating_add_signed(delta);
        if next_index >= visual_row_count {
            return false;
        };
        self.focus_diff_visual_index(&inline_blocks, next_index, DiffScrollPolicy::EnsureVisible)
    }

    fn focus_adjacent_inline_block_row(&mut self, block_id: &str, delta: isize) -> bool {
        let inline_blocks = self.diff_inline_blocks();
        let Some(block_index) = inline_blocks.iter().position(|block| block.id == block_id) else {
            return false;
        };
        let viewer = self.diff_buffer.viewer();
        let visual_row_count =
            viewer.visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        let Some(block_height) = inline_blocks.get(block_index).map(|block| block.height) else {
            return false;
        };
        let target_index = if delta > 0 {
            let Some(index) = viewer.visual_index_for_inline_block_line(
                &self.document,
                &inline_blocks,
                block_index,
                block_height.saturating_sub(1),
            ) else {
                return false;
            };
            index.saturating_add(1)
        } else {
            let Some(index) = viewer.visual_index_for_inline_block_line(
                &self.document,
                &inline_blocks,
                block_index,
                0,
            ) else {
                return false;
            };
            let Some(target_index) = index.checked_sub(1) else {
                return false;
            };
            target_index
        };
        if target_index >= visual_row_count {
            return false;
        }
        self.focus_diff_visual_index(
            &inline_blocks,
            target_index,
            DiffScrollPolicy::EnsureVisible,
        )
    }

    fn current_diff_visual_index(&self, inline_blocks: &[DiffInlineBlock]) -> Option<usize> {
        let viewer = self.diff_buffer.viewer();
        if let Some(focus) = &self.inline_focus
            && let Some(block_index) = inline_blocks
                .iter()
                .position(|block| block.id == focus.block_id)
        {
            return viewer.visual_index_for_inline_block_line(
                &self.document,
                inline_blocks,
                block_index,
                focus.line,
            );
        }

        viewer.cursor_visual_index(&self.document, inline_blocks)
    }

    fn focus_diff_visual_index(
        &mut self,
        inline_blocks: &[DiffInlineBlock],
        visual_index: usize,
        scroll_policy: DiffScrollPolicy,
    ) -> bool {
        let viewer = self.diff_buffer.viewer();
        let visual_row_count =
            viewer.visual_row_count_with_inline_blocks(&self.document, inline_blocks);
        let Some(next_visual_row) =
            viewer.visual_row_at_with_inline_blocks(&self.document, inline_blocks, visual_index)
        else {
            return false;
        };
        match next_visual_row {
            DiffVisualRow::Document { row, .. } => {
                self.inline_focus = None;
                self.focus_document_row_preserving_view(row);
                self.apply_diff_scroll_policy(scroll_policy, visual_index, visual_row_count);
                true
            }
            DiffVisualRow::InlineBlock { index, line, .. } => {
                let Some(block) = inline_blocks.get(index) else {
                    return false;
                };
                let Some((first_body_line, last_body_line)) = inline_block_body_line_range(block)
                else {
                    return false;
                };
                self.focus_document_row_preserving_view(block.after_row);
                self.inline_focus = Some(InlineFocus {
                    block_id: block.id.clone(),
                    line: line.clamp(first_body_line, last_body_line),
                });
                self.apply_diff_scroll_policy(scroll_policy, visual_index, visual_row_count);
                true
            }
        }
    }

    fn apply_diff_scroll_policy(
        &mut self,
        scroll_policy: DiffScrollPolicy,
        visual_index: usize,
        visual_row_count: usize,
    ) {
        match scroll_policy {
            DiffScrollPolicy::EnsureVisible => {
                self.ensure_diff_visual_index_visible(visual_index, visual_row_count)
            }
            DiffScrollPolicy::Center => {
                self.center_diff_visual_index(visual_index, visual_row_count)
            }
        }
    }

    fn focus_document_row_preserving_view(&mut self, row: usize) {
        self.diff_buffer
            .viewer_mut()
            .focus_row_preserving_view(&self.document, row);
    }

    fn diff_document_row_screen_offset(&self, row: usize) -> Option<usize> {
        let inline_blocks = self.diff_inline_blocks();
        let viewer = self.diff_buffer.viewer();
        let visual_index = viewer.visual_index_for_document_row_with_inline_blocks(
            &self.document,
            &inline_blocks,
            row,
        )?;
        visual_index.checked_sub(self.diff_buffer.viewer().viewport.scroll_y)
    }

    fn keep_diff_document_row_at_screen_offset(&mut self, row: usize, screen_offset: usize) {
        let inline_blocks = self.diff_inline_blocks();
        let viewer = self.diff_buffer.viewer();
        let Some(visual_index) = viewer.visual_index_for_document_row_with_inline_blocks(
            &self.document,
            &inline_blocks,
            row,
        ) else {
            return;
        };
        let visual_row_count =
            viewer.visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        let height = self.viewport_height.max(1);
        let max_scroll = visual_row_count.saturating_sub(height);
        self.diff_buffer.viewer_mut().viewport.scroll_y =
            visual_index.saturating_sub(screen_offset).min(max_scroll);
        self.ensure_diff_visual_index_visible(visual_index, visual_row_count);
    }

    fn ensure_diff_visual_index_visible(&mut self, visual_index: usize, visual_row_count: usize) {
        let height = self.viewport_height.max(1);
        let top_margin = self.active_top_margin().min(height.saturating_sub(1));
        let scroll_y = self.diff_buffer.viewer().viewport.scroll_y;
        let max_scroll = visual_row_count.saturating_sub(height);
        let first_unobscured = scroll_y.saturating_add(top_margin);
        let next_scroll_y = if visual_index < first_unobscured {
            visual_index.saturating_sub(top_margin)
        } else if visual_index >= scroll_y.saturating_add(height) {
            visual_index.saturating_sub(height.saturating_sub(1))
        } else {
            scroll_y
        };
        self.diff_buffer.viewer_mut().viewport.scroll_y = next_scroll_y.min(max_scroll);
    }

    fn center_focused_diff_visual_row(&mut self) {
        let inline_blocks = self.diff_inline_blocks();
        let visual_row_count = self
            .diff_buffer
            .viewer()
            .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        if let Some(index) = self.current_diff_visual_index(&inline_blocks) {
            self.center_diff_visual_index(index, visual_row_count);
        }
    }

    fn ensure_focused_diff_visual_row_visible(&mut self) {
        let inline_blocks = self.diff_inline_blocks();
        let visual_row_count = self
            .diff_buffer
            .viewer()
            .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        if let Some(index) = self.current_diff_visual_index(&inline_blocks) {
            self.ensure_diff_visual_index_visible(index, visual_row_count);
        }
    }

    fn ensure_inline_comment_editor_cursor_visible(&mut self) {
        let Some(modal) = self.comment_modal.as_ref() else {
            return;
        };
        let inline_blocks = self.diff_inline_blocks();
        let Some(editor_block_index) = inline_blocks
            .iter()
            .position(|block| block.kind == DiffInlineBlockKind::Editor)
        else {
            return;
        };
        let viewer = self.diff_buffer.viewer();
        let visual_row_count =
            viewer.visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
        let pane = viewer.viewport.pane_rect(
            Rect::new(
                0,
                0,
                viewer.viewport.width.saturating_sub(1),
                viewer.viewport.height.max(1),
            ),
            inline_blocks[editor_block_index].side,
        );
        let text_width = inline_block_text_width(pane);
        let editor_line = modal.visual_cursor_row(text_width).saturating_add(1);
        let Some(visual_index) = viewer.visual_index_for_inline_block_line(
            &self.document,
            &inline_blocks,
            editor_block_index,
            editor_line,
        ) else {
            return;
        };
        self.ensure_diff_visual_index_visible(visual_index, visual_row_count);
    }

    fn center_diff_visual_index(&mut self, visual_index: usize, visual_row_count: usize) {
        let height = self.viewport_height.max(1);
        let max_scroll = visual_row_count.saturating_sub(height);
        let center = height / 2;
        self.diff_buffer.viewer_mut().viewport.scroll_y =
            visual_index.saturating_sub(center).min(max_scroll);
    }

    fn move_diff_cursor_cols(&mut self, delta: isize, rows: usize) {
        let _ = rows;
        self.diff_buffer
            .viewer_mut()
            .move_cursor_cols(&self.document, delta);
    }

    fn move_active_relative(&mut self, delta: isize, rows: usize) {
        if self.surface == AppSurface::Diff {
            self.move_diff_cursor_rows(delta, rows);
            return;
        }
        if rows == 0 {
            self.diff_buffer.viewer_mut().cursor.row = 0;
            self.diff_buffer.viewer_mut().viewport.scroll_y = 0;
            return;
        }

        let selecting = self.diff_buffer.viewer().selection.is_some();
        if !self.is_active_visible(rows) {
            self.diff_buffer.viewer_mut().cursor.row = self.first_unobscured_visible_row(rows);
            self.update_keyboard_selection(rows);
            return;
        }

        self.diff_buffer.viewer_mut().cursor.row = self
            .diff_buffer
            .viewer()
            .cursor
            .row
            .saturating_add_signed(delta)
            .min(rows.saturating_sub(1));
        self.keep_active_visible(rows);
        if selecting {
            self.update_keyboard_selection(rows);
        } else {
            self.diff_buffer.viewer_mut().clear_selection();
        }
    }

    fn move_active_half_page(&mut self, direction: isize, rows: usize) {
        let _ = rows;
        if self.surface == AppSurface::Diff {
            let inline_blocks = self.diff_inline_blocks();
            let visual_row_count = self
                .diff_buffer
                .viewer()
                .visual_row_count_with_inline_blocks(&self.document, &inline_blocks);
            let Some(current_index) = self.current_diff_visual_index(&inline_blocks) else {
                return;
            };
            let half = (self.viewport_height.max(2) / 2).max(1) as isize;
            let target_index = current_index
                .saturating_add_signed(direction.saturating_mul(half))
                .min(visual_row_count.saturating_sub(1));
            self.focus_diff_visual_index(&inline_blocks, target_index, DiffScrollPolicy::Center);
            return;
        }
        self.diff_buffer
            .viewer_mut()
            .half_page(&self.document, direction);
    }

    fn toggle_visual_selection(&mut self, rows: usize, linewise: bool) {
        let _ = rows;
        if self.diff_buffer.viewer().selection.is_some() {
            self.diff_buffer.viewer_mut().clear_selection();
        } else if linewise {
            self.diff_buffer
                .viewer_mut()
                .start_visual_line_selection(&self.document);
        } else {
            self.diff_buffer
                .viewer_mut()
                .start_visual_selection(&self.document);
        }
    }

    fn update_keyboard_selection(&mut self, rows: usize) {
        if self.diff_buffer.viewer().selection.is_none() {
            return;
        }
        self.diff_buffer
            .viewer_mut()
            .update_visual_selection(&self.document);
        let _ = rows;
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
        let row = self.diff_buffer.viewer().cursor.row;
        row >= first_visible && row <= last_visible
    }

    fn first_unobscured_visible_row(&self, rows: usize) -> usize {
        self.diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .saturating_add(self.active_top_margin())
            .min(rows.saturating_sub(1))
    }

    fn active_top_margin(&self) -> usize {
        0
    }

    fn keep_active_visible(&mut self, rows: usize) {
        if rows == 0 {
            self.diff_buffer.viewer_mut().viewport.scroll_y = 0;
            self.diff_buffer.viewer_mut().cursor.row = 0;
            return;
        }
        let top_margin = self.active_top_margin();
        let viewport_height = self.viewport_height.max(1);
        self.diff_buffer.viewer_mut().cursor.row = self
            .diff_buffer
            .viewer()
            .cursor
            .row
            .min(rows.saturating_sub(1));
        let row = self.diff_buffer.viewer().cursor.row;
        let scroll_y = self.diff_buffer.viewer().viewport.scroll_y;
        if row < scroll_y.saturating_add(top_margin) {
            self.diff_buffer.viewer_mut().viewport.scroll_y = row.saturating_sub(top_margin);
        } else if row >= scroll_y.saturating_add(viewport_height) {
            self.diff_buffer.viewer_mut().viewport.scroll_y =
                row.saturating_sub(viewport_height.saturating_sub(1));
        }
        self.diff_buffer.viewer_mut().viewport.scroll_y = self
            .diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .min(rows.saturating_sub(viewport_height));
    }

    fn navigation_origin_row(&self) -> usize {
        let cursor_row = self.diff_cursor_row();
        let scroll_y = self.diff_scroll_y();
        if cursor_row >= scroll_y && cursor_row < scroll_y.saturating_add(self.viewport_height) {
            cursor_row
        } else {
            scroll_y
        }
    }

    fn jump_relative_file(&mut self, delta: isize, rows: usize) {
        let Some(current) = self
            .document
            .row_file_index(self.diff_mode(), self.navigation_origin_row())
        else {
            return;
        };
        let next = current
            .saturating_add_signed(delta)
            .min(self.document.files.len().saturating_sub(1));
        self.jump_to_file(next, rows);
    }

    fn jump_to_file(&mut self, file_index: usize, rows: usize) {
        let Some(row) = self.document.file_row(self.diff_mode(), file_index) else {
            return;
        };
        self.focus_row(row, rows);
    }

    fn jump_to_text_result(&mut self, result: &TextSearchResult, rows: usize) {
        let Some(row) = self.document.line_row(
            self.diff_mode(),
            result.file_index,
            result.hunk_index,
            result.line_index,
        ) else {
            return;
        };
        self.diff_buffer.viewer_mut().clear_selection();
        self.diff_buffer.viewer_mut().cursor.side = if result.kind == "-" {
            DiffSide::Left
        } else {
            DiffSide::Right
        };
        self.diff_buffer.viewer_mut().cursor.row = row.min(rows.saturating_sub(1));
        let sticky_header_rows = 2usize;
        let context_rows = sticky_header_rows + 3;
        self.diff_buffer.viewer_mut().viewport.scroll_y = self
            .diff_buffer
            .viewer()
            .cursor
            .row
            .saturating_sub(context_rows)
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn jump_relative_hunk(&mut self, delta: isize, rows: usize) {
        self.jump_relative_hunk_from(self.navigation_origin_row(), delta, rows);
    }

    fn jump_relative_hunk_from(&mut self, origin_row: usize, delta: isize, rows: usize) {
        let target = if delta > 0 {
            self.document.next_hunk_row(self.diff_mode(), origin_row)
        } else {
            self.document
                .previous_hunk_row(self.diff_mode(), origin_row)
        };
        let Some(row) = target else { return };
        self.diff_buffer.viewer_mut().clear_selection();
        self.diff_buffer.viewer_mut().cursor.row = row.min(rows.saturating_sub(1));
        self.diff_buffer.viewer_mut().viewport.scroll_y = self
            .diff_buffer
            .viewer()
            .cursor
            .row
            .min(rows.saturating_sub(self.viewport_height));
    }

    fn active_line_target(&self) -> Option<DiffLineTarget> {
        self.document.line_target(
            self.diff_mode(),
            self.diff_buffer.viewer().cursor.row,
            self.diff_buffer.viewer().cursor.side,
        )
    }

    fn handle_review_sidebar_key(&mut self, code: KeyCode, rows: usize) {
        let visible_rows = self.review_tree_rows();
        match code {
            KeyCode::Esc => self.set_diff_focus(DiffFocusPane::Right),
            KeyCode::Char('q') => self.quit_or_go_back(false),
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
            KeyCode::PageUp if visible_rows.len() > 1 => {
                let step = self.viewport_height.max(1) / 2;
                self.review_sidebar_selection =
                    self.review_sidebar_selection.saturating_sub(step.max(1));
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::Char('h') | KeyCode::Left => self.collapse_selected_review_tree_row(),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.open_selected_review_tree_row(rows)
            }
            KeyCode::Char(' ') | KeyCode::Char('r') => self.toggle_selected_review_tree_viewed(),
            KeyCode::Char('u') => {
                self.review_sidebar_unreviewed_only = true;
                self.review_sidebar_selection = 0;
                self.review_sidebar_scroll_y = 0;
                self.keep_review_sidebar_selection_visible();
            }
            KeyCode::Char('U') => {
                self.review_sidebar_unreviewed_only = false;
                self.review_sidebar_selection = 0;
                self.review_sidebar_scroll_y = 0;
                self.keep_review_sidebar_selection_visible();
            }
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
                status: file.metadata().kind,
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
                    _file_index: file_index,
                    path: file.new_path.clone(),
                    depth: parts.len(),
                    entity_type: change.entity_type.clone(),
                    entity_name: change.entity_name.clone(),
                    change_type: change.change_type.clone(),
                    line: change.line,
                }));
            }
        }
        if self.review_sidebar_unreviewed_only {
            rows.into_iter()
                .filter(|row| self.review_tree_row_has_unreviewed_target(row))
                .collect()
        } else {
            rows
        }
    }

    fn review_tree_row_has_unreviewed_target(&self, row: &ReviewTreeRow) -> bool {
        match row {
            ReviewTreeRow::Directory { path, .. } => self.document.files.iter().any(|file| {
                file.new_path.starts_with(&format!("{path}/"))
                    && !self.is_file_viewed(&file.new_path)
            }),
            ReviewTreeRow::File { path, .. } => !self.is_file_viewed(path),
            ReviewTreeRow::Entity { key, .. } => !self.is_entity_viewed(key),
        }
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
        Self::semantic_entity_key_parts(
            path,
            &change.entity_type,
            &change.entity_name,
            &change.change_type,
            change.line,
        )
    }

    fn semantic_entity_key_parts(
        path: &str,
        entity_type: &str,
        entity_name: &str,
        change_type: &str,
        line: Option<usize>,
    ) -> String {
        format!(
            "{path}\u{1f}{entity_type}\u{1f}{entity_name}\u{1f}{change_type}\u{1f}{}",
            line.unwrap_or(0)
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

    fn review_sidebar_progress_counts(&self) -> (usize, usize) {
        let Some(semantic_diff) = self.semantic_diff_for_route(&self.diff_source) else {
            return (self.viewed_file_count(), self.document.files.len());
        };
        let document_paths: HashSet<&str> = self
            .document
            .files
            .iter()
            .map(|file| file.new_path.as_str())
            .collect();
        let entity_keys = semantic_diff
            .files
            .iter()
            .filter(|file| document_paths.contains(file.path.as_str()))
            .flat_map(|file| {
                file.changes
                    .iter()
                    .map(|change| Self::review_entity_key(&file.path, change))
            })
            .collect::<Vec<_>>();
        if entity_keys.is_empty() {
            return (self.viewed_file_count(), self.document.files.len());
        }
        let viewed = entity_keys
            .iter()
            .filter(|key| self.viewed_entities.contains(*key))
            .count();
        (viewed, entity_keys.len())
    }

    fn toggle_current_file_viewed(&mut self) {
        let Some(path) = self.current_file_path().map(str::to_string) else {
            return;
        };
        self.toggle_file_viewed(&path);
    }

    fn toggle_file_viewed(&mut self, path: &str) {
        let route = self.diff_source.clone();
        self.toggle_file_viewed_for_route(&route, path);
    }

    fn toggle_file_viewed_for_route(&mut self, route: &DiffSource, path: &str) {
        if self.viewed_files.contains(path) {
            self.viewed_files.remove(path);
            self.set_file_entities_viewed_for_route(route, path, false);
        } else {
            self.viewed_files.insert(path.to_string());
            self.set_file_entities_viewed_for_route(route, path, true);
        }
        self.persist_viewed_state();
    }

    fn set_file_entities_viewed(&mut self, path: &str, viewed: bool) {
        let route = self.diff_source.clone();
        self.set_file_entities_viewed_for_route(&route, path, viewed);
    }

    fn set_file_entities_viewed_for_route(&mut self, route: &DiffSource, path: &str, viewed: bool) {
        let keys: Vec<_> = self
            .semantic_diff_for_route(route)
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

    fn backfill_viewed_entities_for_route(&mut self, route: &DiffSource) {
        if &self.diff_source != route {
            return;
        }
        let keys = self
            .semantic_diff_for_route(route)
            .map(|diff| {
                diff.files
                    .iter()
                    .filter(|file| self.viewed_files.contains(&file.path))
                    .flat_map(|file| {
                        file.changes
                            .iter()
                            .map(|change| Self::review_entity_key(&file.path, change))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut changed = false;
        for key in keys {
            changed |= self.viewed_entities.insert(key);
        }
        if changed {
            self.persist_viewed_state();
        }
    }

    fn toggle_selected_semantic_viewed(&mut self) -> bool {
        let Some(route) = self.current_semantic_route() else {
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
            SemanticTreeRow::Directory { key, .. } => {
                let Some(directory) = key.path.strip_prefix("dir:") else {
                    return false;
                };
                self.toggle_semantic_directory_viewed(&route, directory);
            }
            SemanticTreeRow::File { path, .. } => self.toggle_file_viewed_for_route(&route, &path),
            SemanticTreeRow::Entity {
                path,
                entity_type,
                entity_name,
                change_type,
                line,
                ..
            } => {
                let key = Self::semantic_entity_key_parts(
                    &path,
                    &entity_type,
                    &entity_name,
                    &change_type,
                    line,
                );
                if !self.viewed_entities.insert(key) {
                    self.viewed_entities
                        .remove(&Self::semantic_entity_key_parts(
                            &path,
                            &entity_type,
                            &entity_name,
                            &change_type,
                            line,
                        ));
                    self.viewed_files.remove(&path);
                }
                self.persist_viewed_state();
            }
            SemanticTreeRow::Status(_) => return false,
        }
        true
    }

    fn toggle_semantic_directory_viewed(&mut self, route: &DiffSource, directory: &str) {
        let paths: Vec<String> = self
            .semantic_diff_for_route(route)
            .map(|diff| {
                diff.files
                    .iter()
                    .filter(|file| file.path.starts_with(&format!("{directory}/")))
                    .map(|file| file.path.clone())
                    .collect()
            })
            .unwrap_or_default();
        let should_mark = paths.iter().any(|path| !self.viewed_files.contains(path));
        for path in paths {
            if should_mark {
                self.viewed_files.insert(path.clone());
            } else {
                self.viewed_files.remove(&path);
            }
            self.set_file_entities_viewed_for_route(route, &path, should_mark);
        }
        self.persist_viewed_state();
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
        if let Some(selection) = self.diff_buffer.viewer().selection
            && let Some(target) = self
                .document
                .selection_target(self.diff_buffer.viewer().viewport.mode, selection)
        {
            return Some(target);
        }
        self.focus_comment_target().map(DiffLineRangeTarget::single)
    }

    fn focus_comment_target(&mut self) -> Option<DiffLineTarget> {
        let mode = self.diff_buffer.viewer().viewport.mode;
        let cursor = self.diff_buffer.viewer().cursor;
        let rows = row_count_for_mode(&self.document, mode);
        if rows == 0 {
            return None;
        }

        if let Some(target) = self.line_target_at(cursor.row) {
            return Some(target);
        }

        let visible_top = self
            .diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .min(rows.saturating_sub(1));
        let visible_bottom = self
            .diff_buffer
            .viewer()
            .viewport
            .scroll_y
            .saturating_add(self.viewport_height.saturating_sub(1))
            .min(rows.saturating_sub(1));
        let start = cursor.row.clamp(visible_top, visible_bottom);

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
        let mode = self.diff_buffer.viewer().viewport.mode;
        let side = self.diff_buffer.viewer().cursor.side;
        if let Some(target) = self.document.line_target(mode, row, side) {
            self.diff_buffer.viewer_mut().cursor.row = row;
            self.diff_buffer.viewer_mut().cursor.side = target.side;
            return Some(target);
        }

        let other_side = match side {
            DiffSide::Left => DiffSide::Right,
            DiffSide::Right => DiffSide::Left,
        };
        let target = self.document.line_target(mode, row, other_side)?;
        self.diff_buffer.viewer_mut().cursor.row = row;
        self.diff_buffer.viewer_mut().cursor.side = target.side;
        Some(target)
    }
}

fn editor_preset_name(editor: &str) -> String {
    let first_word = editor
        .split_whitespace()
        .next()
        .unwrap_or(editor)
        .trim_matches(['\'', '"']);
    let name = Path::new(first_word)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(first_word);
    match name {
        "codium" | "code-insiders" => "code".to_string(),
        "vim" | "nvim" | "vi" | "lvim" | "nano" | "micro" | "kak" | "hx" | "helix" | "code"
        | "subl" | "bbedit" | "xed" | "zed" => name.to_string(),
        _ => name.to_string(),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppSurface {
    Queue,
    CommitList,
    DetailFull,
    Comments,
    Diff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalFlow {
    ForgeLogin,
    OpenEditor { command: String, cwd: PathBuf },
}

#[derive(Debug, PartialEq, Eq)]
enum TerminalFlowResult {
    ForgeLogin(std::result::Result<String, String>),
    Editor(std::result::Result<(), String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailTab {
    Semantic,
    Description,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueuePane {
    List,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitPane {
    List,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffFocusPane {
    Sidebar,
    Left,
    Right,
}

impl DiffFocusPane {
    fn non_sidebar(self) -> Self {
        match self {
            DiffFocusPane::Sidebar => DiffFocusPane::Right,
            DiffFocusPane::Left | DiffFocusPane::Right => self,
        }
    }
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
            || matches!(self, Self::Commit { repo_path, .. } if repo_path.starts_with("forge:"))
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

fn inline_block_accent_for_review_kind(kind: ReviewItemKind) -> DiffInlineBlockAccent {
    match kind {
        ReviewItemKind::Question => DiffInlineBlockAccent::Question,
        ReviewItemKind::Instruction => DiffInlineBlockAccent::Instruction,
        ReviewItemKind::Note => DiffInlineBlockAccent::Note,
        ReviewItemKind::AgentCheck => DiffInlineBlockAccent::Agent,
    }
}

fn inline_block_body_line_range(block: &DiffInlineBlock) -> Option<(usize, usize)> {
    if block.height < 3 {
        return None;
    }
    Some((1, block.height.saturating_sub(2)))
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
            return Some(
                app.forge
                    .pull_request_url(&pull_request.repository, pull_request.number),
            );
        }
        let route = self.local_route.as_ref().unwrap_or(&app.local_route);
        let project = app.project_label()?;
        Some(app.forge.branch_url(&project, &route.branch))
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
            if let Some(first) = line.spans.first_mut()
                && first.content.as_ref() == " "
            {
                first.content = "   ".to_string().into();
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

fn side_sort_key(side: DiffSide) -> u8 {
    match side {
        DiffSide::Left => 0,
        DiffSide::Right => 1,
    }
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

fn pull_request_file_source<'a>(
    sources: &'a HashMap<String, PullRequestFileSources>,
    file: &FileDiff,
    side: DiffSide,
) -> Option<&'a PullRequestFileSources> {
    match side {
        DiffSide::Left => file
            .old_path
            .as_ref()
            .and_then(|path| sources.get(path))
            .or_else(|| sources.get(&file.new_path)),
        DiffSide::Right => sources
            .get(&file.new_path)
            .or_else(|| file.old_path.as_ref().and_then(|path| sources.get(path))),
    }
}

fn git_blob_at(repo_path: &Path, rev: &str, path: &str) -> Option<String> {
    let spec = format!("{rev}:{path}");
    git_stdout_result_in(repo_path, ["show", spec.as_str()]).ok()
}

fn git_index_blob_at(repo_path: &Path, path: &str) -> Option<String> {
    let spec = format!(":{path}");
    git_stdout_result_in(repo_path, ["show", spec.as_str()]).ok()
}

fn local_diff_old_ref(base_ref: &str) -> &str {
    if base_ref == "--cached" {
        "HEAD"
    } else {
        base_ref
    }
}

fn wire_span_lines_to_syntax(lines: Vec<Vec<WireSyntaxSpan>>) -> Vec<Vec<SyntaxSpan>> {
    lines
        .into_iter()
        .map(|line| line.into_iter().map(Into::into).collect())
        .collect()
}

fn line_window(start: Option<u32>, end: Option<u32>) -> Option<HighlightLineWindow> {
    Some(HighlightLineWindow {
        start: start?,
        end: end?,
    })
}

fn highlight_response_matches_file(
    current: &FileDiff,
    response: &highlight_daemon::HighlightFileResponse,
) -> bool {
    current.new_path == response.path && current.old_path == response.old_path
}

fn highlight_response_matches_request(
    request: &HighlightFileRequest,
    response: &highlight_daemon::HighlightFileResponse,
) -> bool {
    request.file_index == response.file_index
        && request.path == response.path
        && request.old_path == response.old_path
}

fn spawn_highlight_worker(rx: Receiver<HighlightRequestEnvelope>, query_tx: Sender<QueryEvent>) {
    thread::spawn(move || {
        let mut pending: Option<HighlightRequestEnvelope> = None;
        'outer: loop {
            let Some(mut envelope) = pending.take().or_else(|| rx.recv().ok()) else {
                break;
            };
            while let Ok(newer) = rx.try_recv() {
                envelope = newer;
            }
            let visible_count = envelope.visible_job_count.min(envelope.jobs.len());
            let (visible_jobs, prefetch_jobs) = envelope.jobs.split_at(visible_count);
            request_highlight_jobs(visible_jobs, &envelope, &query_tx, "visible");
            for job in prefetch_jobs {
                if let Ok(newer) = rx.try_recv() {
                    let mut newest = newer;
                    while let Ok(newer) = rx.try_recv() {
                        newest = newer;
                    }
                    pending = Some(newest);
                    continue 'outer;
                }
                request_highlight_jobs(std::slice::from_ref(job), &envelope, &query_tx, "prefetch");
            }
        }
    });
}

fn spawn_highlight_daemon_warmup() {
    thread::spawn(|| {
        let request = HighlightRequest {
            request_id: 0,
            files: Vec::new(),
        };
        let started = Instant::now();
        if highlight_daemon::request_highlights(&request).is_ok() {
            highlight_trace(&format!(
                "daemon warmup completed in {:.3}ms",
                started.elapsed().as_secs_f64() * 1000.0
            ));
        }
    });
}

fn request_highlight_jobs(
    jobs: &[HighlightFileJob],
    envelope: &HighlightRequestEnvelope,
    query_tx: &Sender<QueryEvent>,
    label: &str,
) {
    let files = jobs
        .iter()
        .filter_map(|job| highlight_request_for_job(&envelope.token.route, job))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return;
    }
    let request = HighlightRequest {
        request_id: envelope.token.request_id,
        files,
    };
    let cached = highlight_daemon::cached_highlights(&request);
    let is_prefetch = label == "prefetch";
    if !cached.files.is_empty() {
        highlight_trace(&format!(
            "{label} cache returned {} files before daemon request",
            cached.files.len()
        ));
        let cached_files = cached.files.clone();
        let _ = query_tx.send(QueryEvent::HighlightedFiles {
            token: envelope.token.clone(),
            response: cached,
        });
        let missing_files = request
            .files
            .into_iter()
            .filter(|request_file| {
                !cached_files.iter().any(|cached_file| {
                    highlight_response_matches_request(request_file, cached_file)
                })
            })
            .collect::<Vec<_>>();
        if missing_files.is_empty() {
            return;
        }
        if is_prefetch {
            highlight_trace(&format!(
                "skipping prefetch daemon miss files={}",
                missing_files.len()
            ));
            return;
        }
        let request = HighlightRequest {
            request_id: envelope.token.request_id,
            files: missing_files,
        };
        request_daemon_highlights(&request, envelope, query_tx, label);
        return;
    }
    if is_prefetch {
        highlight_trace(&format!(
            "skipping prefetch daemon miss files={}",
            request.files.len()
        ));
        return;
    }
    request_daemon_highlights(&request, envelope, query_tx, label);
}

fn request_daemon_highlights(
    request: &HighlightRequest,
    envelope: &HighlightRequestEnvelope,
    query_tx: &Sender<QueryEvent>,
    label: &str,
) {
    let started = Instant::now();
    highlight_trace(&format!(
        "requesting {label} daemon highlights request={} files={}",
        request.request_id,
        request.files.len()
    ));
    if let Ok(response) = highlight_daemon::request_highlights(request) {
        highlight_trace(&format!(
            "{label} daemon returned {} files in {:.3}ms",
            response.files.len(),
            started.elapsed().as_secs_f64() * 1000.0
        ));
        let _ = query_tx.send(QueryEvent::HighlightedFiles {
            token: envelope.token.clone(),
            response,
        });
    }
}

fn highlight_request_for_job(
    route: &DiffSource,
    job: &HighlightFileJob,
) -> Option<HighlightFileRequest> {
    let (old_source, new_source) = match route {
        DiffSource::LocalWorktree(route) => {
            let repo_path = Path::new(&route.repo_path);
            let old_source = job
                .old_path
                .as_deref()
                .and_then(|path| git_blob_at(repo_path, local_diff_old_ref(&route.base_ref), path));
            let new_source = if route.base_ref == "--cached" {
                git_index_blob_at(repo_path, &job.new_path)
            } else {
                std::fs::read_to_string(repo_path.join(&job.new_path)).ok()
            };
            (old_source, new_source)
        }
        DiffSource::Commit { repo_path, sha } if !repo_path.starts_with("forge:") => {
            let repo_path = Path::new(repo_path);
            let parent = format!("{sha}^");
            let old_source = job
                .old_path
                .as_deref()
                .and_then(|path| git_blob_at(repo_path, &parent, path));
            let new_source = git_blob_at(repo_path, sha, &job.new_path);
            (old_source, new_source)
        }
        DiffSource::PullRequest { .. } | DiffSource::Commit { .. } => return None,
    };
    if old_source.is_none() && new_source.is_none() {
        return None;
    }
    Some(HighlightFileRequest {
        file_index: job.file_index,
        old_path: job.old_path.clone(),
        path: job.new_path.clone(),
        old_source,
        new_source,
        old_line_window: job.old_line_window,
        new_line_window: job.new_line_window,
    })
}

fn highlight_trace(message: &str) {
    if std::env::var_os("LAZYDIFF_HIGHLIGHT_TRACE").is_some() {
        eprintln!("[lazydiff-highlight] {message}");
    }
}

fn file_preview_row_count(file: &FileDiff) -> usize {
    file.hunks.iter().map(|hunk| 1 + hunk.lines.len()).sum()
}

#[cfg(test)]
mod highlight_identity_tests {
    use super::*;

    fn response(
        file_index: usize,
        old_path: Option<&str>,
        path: &str,
    ) -> highlight_daemon::HighlightFileResponse {
        highlight_daemon::HighlightFileResponse {
            file_index,
            old_path: old_path.map(ToString::to_string),
            path: path.to_string(),
            old_source_lines: None,
            new_source_lines: None,
            old_line_window: None,
            new_line_window: None,
            old_spans: None,
            new_spans: None,
        }
    }

    #[test]
    fn highlight_response_identity_matches_current_file_paths() {
        let document = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );

        assert!(highlight_response_matches_file(
            &document.files[0],
            &response(0, Some("a.rs"), "a.rs")
        ));
    }

    #[test]
    fn highlight_response_identity_rejects_file_index_reuse_with_different_path() {
        let document = parse_unified_diff(
            "diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );

        assert!(!highlight_response_matches_file(
            &document.files[0],
            &response(0, Some("a.rs"), "a.rs")
        ));
    }
}

#[cfg(test)]
mod review_sidebar_tests {
    use std::sync::Arc;

    use lazydiff_diffs::DiffLineRangeTarget;

    use super::*;
    use crate::app::semantic::SemanticFile;
    use crate::forge::{Forge, ForgeComment, ForgeQueue, PullRequestFileSources};
    use crate::github::worktree::GitCommit;

    struct TestForge;

    impl Forge for TestForge {
        fn name(&self) -> &'static str {
            "Test"
        }

        fn auth_status(&self) -> GitHubAuthStatus {
            GitHubAuthStatus::MissingLogin
        }

        fn login(&self) -> std::result::Result<String, String> {
            Err("not implemented".into())
        }

        fn logout(&self) -> std::result::Result<bool, String> {
            Ok(false)
        }

        fn fetch_queue(&self) -> std::result::Result<ForgeQueue, String> {
            Err("not implemented".into())
        }

        fn fetch_pull_request_comments(
            &self,
            _repo: &str,
            _number: u32,
        ) -> std::result::Result<Vec<ForgeComment>, String> {
            Err("not implemented".into())
        }

        fn fetch_pull_request_patch(
            &self,
            _repo: &str,
            _number: u32,
        ) -> std::result::Result<String, String> {
            Err("not implemented".into())
        }

        fn fetch_pull_request_file_sources(
            &self,
            _repo: &str,
            _number: u32,
            _paths: &[String],
        ) -> std::result::Result<HashMap<String, PullRequestFileSources>, String> {
            Ok(HashMap::new())
        }

        fn fetch_pull_request_commits(
            &self,
            _repo: &str,
            _number: u32,
        ) -> std::result::Result<Vec<GitCommit>, String> {
            Err("not implemented".into())
        }

        fn fetch_commit_patch(
            &self,
            _repo: &str,
            _sha: &str,
        ) -> std::result::Result<String, String> {
            Err("not implemented".into())
        }

        fn post_comment(
            &self,
            _repo: &str,
            _number: u32,
            _target: &DiffLineRangeTarget,
            _body: &str,
        ) -> std::result::Result<ForgeComment, String> {
            Err("not implemented".into())
        }

        fn pull_request_url(&self, repo: &str, number: u32) -> String {
            format!("https://example.test/{repo}/pull/{number}")
        }

        fn branch_url(&self, repo: &str, branch: &str) -> String {
            format!("https://example.test/{repo}/tree/{branch}")
        }
    }

    fn test_app() -> App {
        let patch = "diff --git a/src/app/queries.rs b/src/app/queries.rs\n--- a/src/app/queries.rs\n+++ b/src/app/queries.rs\n@@ -1,3 +1,3 @@\n fn drain_query_events() {}\n-fn revalidate_local_diff() {}\n+fn revalidate_local_diff_now() {}\n fn apply_pull_request_diff() {}\n";
        App::new_local_diff(
            "test.diff".into(),
            patch.len(),
            parse_unified_diff(patch),
            "/repo".into(),
            "fix/perf".into(),
            "main".into(),
            Arc::new(TestForge),
        )
    }

    #[test]
    fn changes_progress_counts_semantic_nodes_when_semantic_diff_exists() {
        let mut app = test_app();
        app.viewed_files.clear();
        app.viewed_entities.clear();
        let route = app.diff_source.clone();
        app.semantic_diff_cache.insert(
            route,
            SemanticDiff {
                truncated: false,
                files: vec![SemanticFile {
                    path: "src/app/queries.rs".into(),
                    changes: vec![
                        SemanticChange {
                            entity_type: "function".into(),
                            entity_name: "drain_query_events".into(),
                            change_type: "modified".into(),
                            line: Some(1),
                            end_line: Some(1),
                        },
                        SemanticChange {
                            entity_type: "function".into(),
                            entity_name: "revalidate_local_diff".into(),
                            change_type: "modified".into(),
                            line: Some(2),
                            end_line: Some(2),
                        },
                        SemanticChange {
                            entity_type: "function".into(),
                            entity_name: "apply_pull_request_diff".into(),
                            change_type: "modified".into(),
                            line: Some(3),
                            end_line: Some(3),
                        },
                    ],
                }],
            },
        );

        assert_eq!(app.review_sidebar_progress_counts(), (0, 3));
    }

    #[test]
    fn numeric_panel_jumps_focus_changes_and_diff() {
        let mut app = test_app();
        app.surface = AppSurface::Diff;
        app.review_sidebar_visible = true;
        app.set_diff_focus(DiffFocusPane::Right);

        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.surface, AppSurface::Diff);
        assert_eq!(app.current_diff_focus(), DiffFocusPane::Sidebar);
        assert!(app.review_sidebar_focus);

        app.set_diff_focus(DiffFocusPane::Right);

        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(app.current_diff_focus(), DiffFocusPane::Sidebar);
        assert!(app.review_sidebar_focus);

        app.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE));
        assert_eq!(app.current_diff_focus(), DiffFocusPane::Right);
        assert!(!app.review_sidebar_focus);
    }

    #[test]
    fn numeric_panel_jumps_do_not_intercept_search_text() {
        let mut app = test_app();
        app.surface = AppSurface::Diff;

        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));

        assert_eq!(app.diff_buffer.search_query(), "2");
        assert_eq!(app.surface, AppSurface::Diff);
        assert!(!app.review_sidebar_focus);
    }

    #[test]
    fn numeric_panel_jumps_open_status_review_and_commits() {
        let mut app = test_app();
        app.surface = AppSurface::Diff;

        app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert!(app.file_picker_open);
        assert_eq!(app.finder_kind, FinderKind::Inbox);

        app.file_picker_open = false;
        app.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE));
        assert_eq!(app.surface, AppSurface::CommitList);
        assert!(app.commit_route.is_some());

        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.surface, AppSurface::Queue);
    }

    #[test]
    fn r_marks_focused_changes_row_viewed() {
        let mut app = test_app();
        app.surface = AppSurface::Diff;
        app.review_sidebar_visible = true;
        app.set_diff_focus(DiffFocusPane::Sidebar);
        app.seed_review_sidebar_expansion();
        let selected_file_row = app
            .review_tree_rows()
            .iter()
            .position(|row| matches!(row, ReviewTreeRow::File { path, .. } if path == "src/app/queries.rs"))
            .expect("file row should exist");
        app.review_sidebar_selection = selected_file_row;
        assert!(matches!(
            app.review_tree_rows().get(app.review_sidebar_selection),
            Some(ReviewTreeRow::File { path, .. }) if path == "src/app/queries.rs"
        ));
        app.viewed_files.remove("src/app/queries.rs");

        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        assert!(app.is_file_viewed("src/app/queries.rs"));
    }

    #[test]
    fn u_filters_changes_to_unreviewed_rows_and_upper_u_clears_filter() {
        let mut app = test_app();
        app.surface = AppSurface::Diff;
        app.review_sidebar_visible = true;
        app.set_diff_focus(DiffFocusPane::Sidebar);
        app.seed_review_sidebar_expansion();
        app.viewed_files.insert("src/app/queries.rs".to_string());

        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));

        assert!(app.review_sidebar_unreviewed_only);
        assert!(app.review_tree_rows().iter().all(|row| {
            !matches!(row, ReviewTreeRow::File { path, .. } if path == "src/app/queries.rs")
        }));

        app.handle_key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::NONE));

        assert!(!app.review_sidebar_unreviewed_only);
        assert!(app.review_tree_rows().iter().any(|row| {
            matches!(row, ReviewTreeRow::File { path, .. } if path == "src/app/queries.rs")
        }));
    }

    #[test]
    fn opening_another_local_worktree_uses_that_route_not_previous_document() {
        let mut app = test_app();
        let route = LocalWorktreeRoute {
            repo_path: "/repo/other".into(),
            branch: "agent/other".into(),
            base_ref: "origin/main".into(),
        };

        app.open_local_diff(Some(route.clone()));

        assert_eq!(app.local_route, route);
        assert_eq!(app.diff_source, DiffSource::LocalWorktree(route));
        assert!(app.document.files.is_empty());
        assert!(app.local_document.files.is_empty());
    }

    #[test]
    fn stale_local_diff_result_does_not_replace_current_worktree_document() {
        let mut app = test_app();
        let stale_route = app.local_route.clone();
        let current_route = LocalWorktreeRoute {
            repo_path: "/repo/current".into(),
            branch: "agent/current".into(),
            base_ref: "main".into(),
        };
        app.local_route = current_route.clone();
        app.diff_source = DiffSource::LocalWorktree(current_route);
        app.local_document = parse_unified_diff("");
        app.document = parse_unified_diff("");

        let stale_patch = "diff --git a/stale.txt b/stale.txt\n--- a/stale.txt\n+++ b/stale.txt\n@@ -1 +1 @@\n-old\n+new\n";
        app.query_tx
            .send(QueryEvent::LocalDiff {
                route: stale_route,
                result: Ok(LocalDiffResult {
                    branch: "fix/perf".into(),
                    base_ref: "main".into(),
                    document: parse_unified_diff(stale_patch),
                }),
            })
            .expect("send local diff event");

        assert!(app.drain_query_events());

        assert!(app.local_document.files.is_empty());
        assert!(app.document.files.is_empty());
    }

    #[test]
    fn local_diff_result_that_normalizes_branch_still_updates_active_document() {
        let mut app = test_app();
        let requested_route = LocalWorktreeRoute {
            repo_path: "/repo".into(),
            branch: "loading".into(),
            base_ref: "main".into(),
        };
        app.local_route = requested_route.clone();
        app.diff_source = DiffSource::LocalWorktree(requested_route.clone());
        app.document = parse_unified_diff("");
        app.local_document = parse_unified_diff("");
        let patch = "diff --git a/fresh.txt b/fresh.txt\n--- a/fresh.txt\n+++ b/fresh.txt\n@@ -1 +1 @@\n-old\n+new\n";

        app.query_tx
            .send(QueryEvent::LocalDiff {
                route: requested_route,
                result: Ok(LocalDiffResult {
                    branch: "agent/fresh".into(),
                    base_ref: "main".into(),
                    document: parse_unified_diff(patch),
                }),
            })
            .expect("send local diff event");

        assert!(app.drain_query_events());

        assert_eq!(app.local_route.branch, "agent/fresh");
        assert!(matches!(
            &app.diff_source,
            DiffSource::LocalWorktree(route) if route.branch == "agent/fresh"
        ));
        assert_eq!(app.document.files[0].new_path, "fresh.txt");
    }

    #[test]
    fn stale_branch_commits_do_not_replace_current_commit_list() {
        let mut app = test_app();
        let stale_route = app.local_route.clone();
        app.commit_route = Some(LocalWorktreeRoute {
            repo_path: "/repo/current".into(),
            branch: "agent/current".into(),
            base_ref: "main".into(),
        });
        app.commit_pr_route = None;
        app.commits.clear();

        app.query_tx
            .send(QueryEvent::BranchCommits {
                context: CommitListContext::Local(stale_route),
                result: Ok(vec![GitCommit {
                    sha: "abcdef".into(),
                    short_sha: "abcdef".into(),
                    author: "test".into(),
                    authored_at: "0".into(),
                    subject: "stale".into(),
                    files: Vec::new(),
                }]),
            })
            .expect("send branch commits event");

        assert!(app.drain_query_events());

        assert!(app.commits.is_empty());
    }

    #[test]
    fn stale_commit_diff_does_not_replace_current_diff_route() {
        let mut app = test_app();
        let current_route = app.diff_source.clone();
        app.surface = AppSurface::Diff;
        let stale_patch = "diff --git a/stale.txt b/stale.txt\n--- a/stale.txt\n+++ b/stale.txt\n@@ -1 +1 @@\n-old\n+new\n";

        app.query_tx
            .send(QueryEvent::CommitDiff {
                repo_path: "/repo".into(),
                sha: "abcdef".into(),
                result: Ok(parse_unified_diff(stale_patch)),
            })
            .expect("send commit diff event");

        assert!(app.drain_query_events());

        assert_eq!(app.diff_source, current_route);
        assert!(
            app.document
                .files
                .iter()
                .all(|file| file.new_path != "stale.txt")
        );
    }

    #[test]
    fn semantic_diff_arrival_backfills_entities_for_viewed_files() {
        let mut app = test_app();
        let route = app.diff_source.clone();
        app.viewed_files.insert("src/app/queries.rs".into());
        app.viewed_entities.clear();

        app.query_tx
            .send(QueryEvent::SemanticDiff {
                route: route.clone(),
                result: Ok(SemanticDiff {
                    truncated: false,
                    files: vec![SemanticFile {
                        path: "src/app/queries.rs".into(),
                        changes: vec![SemanticChange {
                            entity_type: "function".into(),
                            entity_name: "drain_query_events".into(),
                            change_type: "modified".into(),
                            line: Some(1),
                            end_line: Some(1),
                        }],
                    }],
                }),
            })
            .expect("send semantic diff event");

        assert!(app.drain_query_events());

        assert_eq!(app.review_sidebar_progress_counts(), (1, 1));
    }
}
