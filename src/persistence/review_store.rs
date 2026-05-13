use std::{env, fs, path::PathBuf, process::Command as ProcessCommand};

use color_eyre::Result;
use lazydiff_diffs::{DiffDocument, DiffLineKind, DiffLineRangeTarget, DiffLineTarget, DiffSide};
use ratatui::style::Color;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::app::{now_stamp, stable_id, SemanticDiff, WorkItemKind};
use crate::design_system::ThemeVariant;
use crate::github::{GitHubComment, GitHubQueue};

const GITHUB_QUERY_CACHE_KEY: &str = "github:query-client";
const GITHUB_QUERY_CACHE_BUSTER: &str = "github-query-cache-v1";
const THEME_PREFERENCE_KEY: &str = "ui:theme-preference";

#[derive(Clone)]
pub(crate) struct ReviewSession {
    pub(crate) id: String,
    pub(crate) kind: WorkItemKind,
    pub(crate) repo_path: String,
    pub(crate) branch: String,
    pub(crate) base_ref: String,
    pub(crate) current_attempt: ReviewAttempt,
    pub(crate) notes: Vec<ReviewNote>,
    pub(crate) next_note_id: u64,
}

impl ReviewSession {
    pub(crate) fn load_or_create(
        store: &ReviewStore,
        path: &str,
        _bytes: usize,
        document: &DiffDocument,
    ) -> Self {
        let repo_path = env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "local".to_string());
        let branch = env::var("LAZYDIFF_BRANCH")
            .ok()
            .filter(|branch| !branch.trim().is_empty())
            .unwrap_or_else(current_git_branch);
        let base_ref = env::var("LAZYDIFF_BASE_REF").unwrap_or_else(|_| "HEAD".to_string());
        let id = stable_id(&(repo_path.clone(), branch.clone(), base_ref.clone()));
        let patch_hash = stable_id(&(
            path,
            document.files.len(),
            document.additions(),
            document.deletions(),
        ));
        if let Some(mut session) = store.load_session(&id) {
            if session.current_attempt.patch_hash != patch_hash {
                session.current_attempt = ReviewAttempt {
                    id: stable_id(&(id.as_str(), patch_hash.as_str())),
                    ordinal: session.current_attempt.ordinal.saturating_add(1),
                    parent_attempt_id: Some(session.current_attempt.id.clone()),
                    patch_hash,
                    summary: "working tree changed".to_string(),
                    created_at: now_stamp(),
                };
                store.upsert_session(&session);
            }
            return session;
        }

        let session = Self {
            id: id.clone(),
            kind: WorkItemKind::LocalAgentBranch,
            repo_path,
            branch,
            base_ref,
            current_attempt: ReviewAttempt {
                id: stable_id(&(id.as_str(), patch_hash.as_str())),
                ordinal: 1,
                parent_attempt_id: None,
                patch_hash,
                summary: "initial agent output".to_string(),
                created_at: now_stamp(),
            },
            notes: Vec::new(),
            next_note_id: 1,
        };
        store.upsert_session(&session);
        session
    }

    pub(crate) fn load_or_create_scoped(
        store: &ReviewStore,
        id: String,
        kind: WorkItemKind,
        repo_path: String,
        branch: String,
        base_ref: String,
        patch_label: &str,
        document: &DiffDocument,
    ) -> Self {
        let patch_hash = stable_id(&(
            patch_label,
            document.files.len(),
            document.additions(),
            document.deletions(),
        ));
        if let Some(mut session) = store.load_session(&id) {
            session.kind = kind;
            session.repo_path = repo_path;
            session.branch = branch;
            session.base_ref = base_ref;
            if session.current_attempt.patch_hash != patch_hash {
                session.current_attempt = ReviewAttempt {
                    id: stable_id(&(id.as_str(), patch_hash.as_str())),
                    ordinal: session.current_attempt.ordinal.saturating_add(1),
                    parent_attempt_id: Some(session.current_attempt.id.clone()),
                    patch_hash,
                    summary: "diff changed".to_string(),
                    created_at: now_stamp(),
                };
            }
            store.upsert_session(&session);
            return session;
        }

        let session = Self {
            id: id.clone(),
            kind,
            repo_path,
            branch,
            base_ref,
            current_attempt: ReviewAttempt {
                id: stable_id(&(id.as_str(), patch_hash.as_str())),
                ordinal: 1,
                parent_attempt_id: None,
                patch_hash,
                summary: "initial diff".to_string(),
                created_at: now_stamp(),
            },
            notes: Vec::new(),
            next_note_id: 1,
        };
        store.upsert_session(&session);
        session
    }

    pub(crate) fn add_note(
        &mut self,
        store: &ReviewStore,
        target: DiffLineRangeTarget,
        kind: ReviewItemKind,
        parent_id: Option<u64>,
        body: String,
    ) {
        let id = self.next_note_id;
        self.next_note_id += 1;
        let note = ReviewNote {
            id,
            session_id: self.id.clone(),
            attempt_id: self.current_attempt.id.clone(),
            kind,
            state: kind.default_state(),
            target,
            body: body.trim().to_string(),
            author: kind.default_author().to_string(),
            parent_id,
            created_at: now_stamp(),
        };
        store.insert_note(&note);
        self.notes.push(note);
    }

    pub(crate) fn notes_for_target(&self, target: &DiffLineTarget) -> Vec<&ReviewNote> {
        self.notes
            .iter()
            .filter(|note| note.target.contains(target))
            .collect()
    }

    pub(crate) fn open_count(&self) -> usize {
        self.notes
            .iter()
            .filter(|note| note.state.is_open())
            .count()
    }

    pub(crate) fn resolved_count(&self) -> usize {
        self.notes
            .iter()
            .filter(|note| note.state == ReviewItemState::Resolved)
            .count()
    }
}

#[derive(Clone)]
pub(crate) struct ReviewAttempt {
    pub(crate) id: String,
    pub(crate) ordinal: u32,
    pub(crate) parent_attempt_id: Option<String>,
    pub(crate) patch_hash: String,
    pub(crate) summary: String,
    pub(crate) created_at: u64,
}

#[derive(Clone)]
pub(crate) struct ReviewNote {
    pub(crate) id: u64,
    pub(crate) session_id: String,
    pub(crate) attempt_id: String,
    pub(crate) kind: ReviewItemKind,
    pub(crate) state: ReviewItemState,
    pub(crate) target: DiffLineRangeTarget,
    pub(crate) body: String,
    pub(crate) author: String,
    pub(crate) parent_id: Option<u64>,
    pub(crate) created_at: u64,
}

#[derive(Clone)]
pub(crate) struct ReviewThread {
    pub(crate) session: ReviewSessionSummary,
    pub(crate) note: ReviewNote,
}

#[derive(Clone)]
pub(crate) struct ReviewSessionSummary {
    pub(crate) id: String,
    pub(crate) kind: WorkItemKind,
    pub(crate) repo_path: String,
    pub(crate) branch: String,
    pub(crate) base_ref: String,
}

#[derive(Clone, Copy)]
pub(crate) struct ReviewUiState {
    pub(crate) selected_row: usize,
    pub(crate) scroll_y: usize,
    pub(crate) selected_side: DiffSide,
    pub(crate) diff_mode: lazydiff_diffs::DiffMode,
}

impl ReviewNote {
    pub(crate) fn summary(&self) -> String {
        self.body
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

#[derive(Clone)]
pub(crate) struct CommentModal {
    pub(crate) target: DiffLineRangeTarget,
    pub(crate) kind: ReviewItemKind,
    pub(crate) parent_id: Option<u64>,
    pub(crate) body: String,
}

impl CommentModal {
    pub(crate) fn new(
        target: DiffLineRangeTarget,
        kind: ReviewItemKind,
        parent_id: Option<u64>,
    ) -> Self {
        Self {
            target,
            kind,
            parent_id,
            body: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReviewItemKind {
    Question,
    Instruction,
    Note,
    AgentCheck,
}

impl ReviewItemKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Instruction => "instruction",
            Self::Note => "note",
            Self::AgentCheck => "agent",
        }
    }

    pub(crate) fn composer_title(self) -> &'static str {
        match self {
            Self::Question => "Ask agent",
            Self::Instruction => "Request change",
            Self::Note => "Add note",
            Self::AgentCheck => "Agent check",
        }
    }

    pub(crate) fn composer_help(self) -> &'static str {
        match self {
            Self::Question => "Question for the agent. It should answer without editing code.",
            Self::Instruction => "Instruction for the agent. It may edit the working tree.",
            Self::Note => "Private human note. It will not be sent to the agent.",
            Self::AgentCheck => "Agent-authored observation for this attempt.",
        }
    }

    pub(crate) fn placeholder(self) -> &'static str {
        match self {
            Self::Question => "Why did you make this change?",
            Self::Instruction => "Change this so it handles the edge case.",
            Self::Note => "remember what to verify here…",
            Self::AgentCheck => "agent observation…",
        }
    }

    pub(crate) fn submit_label(self) -> &'static str {
        match self {
            Self::Question => "send question",
            Self::Instruction => "send to agent",
            Self::Note => "save note",
            Self::AgentCheck => "save check",
        }
    }

    pub(crate) fn default_state(self) -> ReviewItemState {
        match self {
            Self::Instruction => ReviewItemState::Requested,
            Self::Question => ReviewItemState::Open,
            Self::Note => ReviewItemState::Open,
            Self::AgentCheck => ReviewItemState::Answered,
        }
    }

    pub(crate) fn default_author(self) -> &'static str {
        match self {
            Self::AgentCheck => "Agent",
            Self::Question | Self::Instruction | Self::Note => "You",
        }
    }

    pub(crate) fn gutter_marker(self) -> (&'static str, Color) {
        match self {
            Self::Instruction => ("!", Color::Rgb(255, 184, 122)),
            Self::Question => ("?", Color::Rgb(230, 207, 152)),
            Self::AgentCheck => ("✦", Color::Rgb(199, 180, 255)),
            Self::Note => ("●", Color::Rgb(154, 164, 175)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReviewItemState {
    Open,
    Answered,
    Requested,
    Changed,
    Resolved,
    Carried,
    Stale,
}

impl ReviewItemState {
    pub(crate) fn label(self) -> &'static str {
        review_item_state_name(self)
    }

    pub(crate) fn from_label(value: &str) -> Self {
        parse_review_item_state(value)
    }

    pub(crate) fn is_open(self) -> bool {
        matches!(
            self,
            Self::Open | Self::Requested | Self::Changed | Self::Stale
        )
    }

    pub(crate) fn bucket_label(self) -> &'static str {
        match self {
            Self::Open | Self::Requested | Self::Changed | Self::Stale => "open",
            Self::Answered => "answered",
            Self::Resolved => "resolved",
            Self::Carried => "carried",
        }
    }

    pub(crate) fn sort_key(self) -> u8 {
        match self {
            Self::Requested => 0,
            Self::Open => 1,
            Self::Changed => 2,
            Self::Answered => 3,
            Self::Stale => 4,
            Self::Carried => 5,
            Self::Resolved => 6,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ReviewStore {
    pub(crate) path: Option<PathBuf>,
}

impl ReviewStore {
    pub(crate) fn open_default() -> Result<Self> {
        let mut dir = xdg_data_home();
        dir.push("lazydiff");
        fs::create_dir_all(&dir)?;
        let path = dir.join("lazydiff.db");
        let store = Self { path: Some(path) };
        store.init()?;
        Ok(store)
    }

    pub(crate) fn memory_only() -> Self {
        Self { path: None }
    }

    fn connection(&self) -> Option<Connection> {
        self.path
            .as_ref()
            .and_then(|path| Connection::open(path).ok())
    }

    fn init(&self) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS review_sessions (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                repo_path TEXT NOT NULL,
                branch TEXT NOT NULL,
                base_ref TEXT NOT NULL,
                attempt_id TEXT NOT NULL,
                attempt_ordinal INTEGER NOT NULL,
                parent_attempt_id TEXT,
                patch_hash TEXT NOT NULL,
                attempt_summary TEXT NOT NULL,
                attempt_created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS review_items (
                id INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                attempt_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                state TEXT NOT NULL,
                file_index INTEGER NOT NULL,
                hunk_index INTEGER NOT NULL,
                start_line_index INTEGER NOT NULL,
                end_line_index INTEGER NOT NULL,
                path TEXT NOT NULL,
                side TEXT NOT NULL,
                old_line INTEGER,
                new_line INTEGER,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                line_kind TEXT NOT NULL,
                body TEXT NOT NULL,
                author TEXT NOT NULL,
                parent_id INTEGER,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (session_id, id)
            );
            CREATE INDEX IF NOT EXISTS review_items_session_attempt ON review_items(session_id, attempt_id);",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS review_ui_state (
                session_id TEXT PRIMARY KEY,
                selected_row INTEGER NOT NULL,
                scroll_y INTEGER NOT NULL,
                selected_side TEXT NOT NULL,
                diff_mode TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    fn load_json_cache<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let conn = self.connection()?;
        let value: String = conn
            .query_row(
                "SELECT value FROM app_kv WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .ok()??;
        serde_json::from_str(&value).ok()
    }

    fn save_json_cache<T>(&self, key: &str, value: &T)
    where
        T: Serialize,
    {
        let Some(conn) = self.connection() else {
            return;
        };
        let Ok(value) = serde_json::to_string(value) else {
            return;
        };
        let _ = conn.execute(
            "INSERT INTO app_kv (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![key, value, now_stamp() as i64],
        );
    }

    pub(crate) fn restore_github_query_client(&self) -> Option<PersistedGitHubQueryClient> {
        let persisted =
            self.load_json_cache::<PersistedGitHubQueryClient>(GITHUB_QUERY_CACHE_KEY)?;
        let expired = (now_stamp() as i64).saturating_sub(persisted.timestamp) > 86_400;
        let busted = !persisted.buster.starts_with("github-query-cache-v");
        if expired || busted {
            self.remove_json_cache(GITHUB_QUERY_CACHE_KEY);
            return None;
        }
        Some(persisted)
    }

    pub(crate) fn persist_github_query_client(&self, client: PersistedGitHubQueryClient) {
        self.save_json_cache(GITHUB_QUERY_CACHE_KEY, &client);
    }

    pub(crate) fn restore_theme_variant(&self) -> Option<ThemeVariant> {
        let persisted = self.load_json_cache::<PersistedThemePreference>(THEME_PREFERENCE_KEY)?;
        ThemeVariant::from_label(&persisted.variant)
    }

    pub(crate) fn persist_theme_variant(&self, variant: ThemeVariant) {
        self.save_json_cache(
            THEME_PREFERENCE_KEY,
            &PersistedThemePreference {
                variant: variant.label().to_string(),
            },
        );
    }

    fn remove_json_cache(&self, key: &str) {
        let Some(conn) = self.connection() else {
            return;
        };
        let _ = conn.execute("DELETE FROM app_kv WHERE key = ?1", params![key]);
    }

    pub(crate) fn load_session(&self, id: &str) -> Option<ReviewSession> {
        let conn = self.connection()?;
        let mut session = conn
            .query_row(
                "SELECT kind, repo_path, branch, base_ref, attempt_id, attempt_ordinal, parent_attempt_id, patch_hash, attempt_summary, attempt_created_at
                 FROM review_sessions WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ReviewSession {
                        id: id.to_string(),
                        kind: parse_work_item_kind(row.get::<_, String>(0)?.as_str()),
                        repo_path: row.get(1)?,
                        branch: row.get(2)?,
                        base_ref: row.get(3)?,
                        current_attempt: ReviewAttempt {
                            id: row.get(4)?,
                            ordinal: row.get::<_, u32>(5)?,
                            parent_attempt_id: row.get(6)?,
                            patch_hash: row.get(7)?,
                            summary: row.get(8)?,
                            created_at: row.get(9)?,
                        },
                        notes: Vec::new(),
                        next_note_id: 1,
                    })
                },
            )
            .optional()
            .ok()??;
        session.notes = self.load_notes(&conn, id);
        session.next_note_id = session.notes.iter().map(|note| note.id).max().unwrap_or(0) + 1;
        Some(session)
    }

    pub(crate) fn list_review_threads(&self) -> Vec<ReviewThread> {
        let Some(conn) = self.connection() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, kind, repo_path, branch, base_ref FROM review_sessions ORDER BY updated_at DESC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok(ReviewSessionSummary {
                id: row.get(0)?,
                kind: parse_work_item_kind(row.get::<_, String>(1)?.as_str()),
                repo_path: row.get(2)?,
                branch: row.get(3)?,
                base_ref: row.get(4)?,
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(Result::ok)
            .flat_map(|session| {
                self.load_notes(&conn, &session.id)
                    .into_iter()
                    .map(move |note| ReviewThread {
                        session: session.clone(),
                        note,
                    })
            })
            .collect()
    }

    pub(crate) fn update_note_state(
        &self,
        session_id: &str,
        note_id: u64,
        state: ReviewItemState,
    ) -> bool {
        let Some(conn) = self.connection() else {
            return false;
        };
        conn.execute(
            "UPDATE review_items SET state = ?1 WHERE session_id = ?2 AND id = ?3",
            params![review_item_state_name(state), session_id, note_id],
        )
        .map(|count| count > 0)
        .unwrap_or(false)
    }

    pub(crate) fn restore_ui_state(&self, session_id: &str) -> Option<ReviewUiState> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT selected_row, scroll_y, selected_side, diff_mode FROM review_ui_state WHERE session_id = ?1",
            params![session_id],
            |row| {
                Ok(ReviewUiState {
                    selected_row: row.get::<_, usize>(0)?,
                    scroll_y: row.get::<_, usize>(1)?,
                    selected_side: parse_diff_side(row.get::<_, String>(2)?.as_str()),
                    diff_mode: parse_diff_mode(row.get::<_, String>(3)?.as_str()),
                })
            },
        )
        .optional()
        .ok()?
    }

    pub(crate) fn persist_ui_state(&self, session_id: &str, state: ReviewUiState) {
        let Some(conn) = self.connection() else {
            return;
        };
        let _ = conn.execute(
            "INSERT INTO review_ui_state (session_id, selected_row, scroll_y, selected_side, diff_mode, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                selected_row=excluded.selected_row,
                scroll_y=excluded.scroll_y,
                selected_side=excluded.selected_side,
                diff_mode=excluded.diff_mode,
                updated_at=excluded.updated_at",
            params![
                session_id,
                state.selected_row,
                state.scroll_y,
                diff_side_name(state.selected_side),
                diff_mode_name(state.diff_mode),
                now_stamp() as i64,
            ],
        );
    }

    pub(crate) fn restore_viewed_state(&self, session_id: &str) -> PersistedViewedState {
        self.load_json_cache(&viewed_state_key(session_id))
            .unwrap_or_default()
    }

    pub(crate) fn persist_viewed_state(&self, session_id: &str, state: &PersistedViewedState) {
        self.save_json_cache(&viewed_state_key(session_id), state);
    }

    fn load_notes(&self, conn: &Connection, session_id: &str) -> Vec<ReviewNote> {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, attempt_id, kind, state, file_index, hunk_index, start_line_index, end_line_index, path, side, old_line, new_line, start_line, end_line, line_kind, body, author, parent_id, created_at
             FROM review_items WHERE session_id = ?1 ORDER BY id ASC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map(params![session_id], |row| {
            let side = parse_diff_side(row.get::<_, String>(9)?.as_str());
            let kind = parse_diff_line_kind(row.get::<_, String>(14)?.as_str());
            let start = DiffLineTarget {
                file_index: row.get::<_, usize>(4)?,
                hunk_index: row.get::<_, usize>(5)?,
                line_index: row.get::<_, usize>(6)?,
                path: row.get(8)?,
                side,
                old_line: row.get(10)?,
                new_line: row.get(11)?,
                line: row.get(12)?,
                kind,
            };
            let mut end = start.clone();
            end.line_index = row.get::<_, usize>(7)?;
            end.line = row.get(13)?;
            Ok(ReviewNote {
                id: row.get(0)?,
                session_id: session_id.to_string(),
                attempt_id: row.get(1)?,
                kind: parse_review_item_kind(row.get::<_, String>(2)?.as_str()),
                state: parse_review_item_state(row.get::<_, String>(3)?.as_str()),
                target: DiffLineRangeTarget { start, end },
                body: row.get(15)?,
                author: row.get(16)?,
                parent_id: row.get(17)?,
                created_at: row.get(18)?,
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(Result::ok).collect()
    }

    pub(crate) fn upsert_session(&self, session: &ReviewSession) {
        let Some(conn) = self.connection() else {
            return;
        };
        let _ = conn.execute(
            "INSERT INTO review_sessions (id, kind, repo_path, branch, base_ref, attempt_id, attempt_ordinal, parent_attempt_id, patch_hash, attempt_summary, attempt_created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, repo_path=excluded.repo_path, branch=excluded.branch, base_ref=excluded.base_ref,
             attempt_id=excluded.attempt_id, attempt_ordinal=excluded.attempt_ordinal, parent_attempt_id=excluded.parent_attempt_id,
             patch_hash=excluded.patch_hash, attempt_summary=excluded.attempt_summary, attempt_created_at=excluded.attempt_created_at, updated_at=excluded.updated_at",
            params![
                session.id,
                work_item_kind_name(session.kind),
                session.repo_path,
                session.branch,
                session.base_ref,
                session.current_attempt.id,
                session.current_attempt.ordinal,
                session.current_attempt.parent_attempt_id,
                session.current_attempt.patch_hash,
                session.current_attempt.summary,
                session.current_attempt.created_at,
                now_stamp(),
            ],
        );
    }

    pub(crate) fn insert_note(&self, note: &ReviewNote) {
        let Some(conn) = self.connection() else {
            return;
        };
        let _ = conn.execute(
            "INSERT OR REPLACE INTO review_items (id, session_id, attempt_id, kind, state, file_index, hunk_index, start_line_index, end_line_index, path, side, old_line, new_line, start_line, end_line, line_kind, body, author, parent_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                note.id,
                note.session_id,
                note.attempt_id,
                review_item_kind_name(note.kind),
                review_item_state_name(note.state),
                note.target.start.file_index,
                note.target.start.hunk_index,
                note.target.start.line_index,
                note.target.end.line_index,
                note.target.path(),
                diff_side_name(note.target.side()),
                note.target.start.old_line,
                note.target.start.new_line,
                note.target.start.line,
                note.target.end.line,
                diff_line_kind_name(note.target.start.kind),
                note.body,
                note.author,
                note.parent_id,
                note.created_at,
            ],
        );
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedGitHubQueryClient {
    pub(crate) timestamp: i64,
    pub(crate) buster: String,
    pub(crate) client_state: GitHubQueryClientState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistedViewedState {
    #[serde(default)]
    pub(crate) files: Vec<String>,
    #[serde(default)]
    pub(crate) entities: Vec<String>,
}

fn viewed_state_key(session_id: &str) -> String {
    format!("review:viewed:{session_id}")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GitHubQueryClientState {
    pub(crate) queue: Option<GitHubQueue>,
    pub(crate) comments: Vec<PersistedPullRequestComments>,
    pub(crate) diffs: Vec<PersistedPullRequestDiff>,
    #[serde(default)]
    pub(crate) semantic_diffs: Vec<PersistedSemanticDiff>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedPullRequestComments {
    pub(crate) repository: String,
    pub(crate) number: u32,
    pub(crate) comments: Vec<GitHubComment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedPullRequestDiff {
    pub(crate) repository: String,
    pub(crate) number: u32,
    pub(crate) patch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PersistedSemanticDiff {
    pub(crate) route_id: String,
    pub(crate) diff: SemanticDiff,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedThemePreference {
    variant: String,
}

fn current_git_branch() -> String {
    git_stdout(["branch", "--show-current"])
        .or_else(|| git_stdout(["rev-parse", "--abbrev-ref", "HEAD"]))
        .filter(|branch| branch != "HEAD")
        .unwrap_or_else(|| "detached-head".to_string())
}

fn git_stdout<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn work_item_kind_name(kind: WorkItemKind) -> &'static str {
    match kind {
        WorkItemKind::LocalAgentBranch => "local_agent_branch",
        WorkItemKind::RequestedPrReview => "requested_pr_review",
        WorkItemKind::OwnedPrFeedback => "owned_pr_feedback",
        WorkItemKind::Update => "update",
    }
}

fn parse_work_item_kind(value: &str) -> WorkItemKind {
    match value {
        "requested_pr_review" => WorkItemKind::RequestedPrReview,
        "owned_pr_feedback" => WorkItemKind::OwnedPrFeedback,
        "update" => WorkItemKind::Update,
        _ => WorkItemKind::LocalAgentBranch,
    }
}

fn review_item_kind_name(kind: ReviewItemKind) -> &'static str {
    match kind {
        ReviewItemKind::Question => "question",
        ReviewItemKind::Instruction => "instruction",
        ReviewItemKind::Note => "note",
        ReviewItemKind::AgentCheck => "agent_check",
    }
}

fn parse_review_item_kind(value: &str) -> ReviewItemKind {
    match value {
        "instruction" => ReviewItemKind::Instruction,
        "agent_check" => ReviewItemKind::AgentCheck,
        "question" => ReviewItemKind::Question,
        _ => ReviewItemKind::Note,
    }
}

fn review_item_state_name(state: ReviewItemState) -> &'static str {
    match state {
        ReviewItemState::Open => "open",
        ReviewItemState::Answered => "answered",
        ReviewItemState::Requested => "requested",
        ReviewItemState::Changed => "changed",
        ReviewItemState::Resolved => "resolved",
        ReviewItemState::Carried => "carried",
        ReviewItemState::Stale => "stale",
    }
}

fn parse_review_item_state(value: &str) -> ReviewItemState {
    match value {
        "answered" => ReviewItemState::Answered,
        "requested" => ReviewItemState::Requested,
        "changed" => ReviewItemState::Changed,
        "resolved" => ReviewItemState::Resolved,
        "carried" => ReviewItemState::Carried,
        "stale" => ReviewItemState::Stale,
        _ => ReviewItemState::Open,
    }
}

fn diff_side_name(side: DiffSide) -> &'static str {
    match side {
        DiffSide::Left => "left",
        DiffSide::Right => "right",
    }
}

fn diff_mode_name(mode: lazydiff_diffs::DiffMode) -> &'static str {
    match mode {
        lazydiff_diffs::DiffMode::Split => "split",
        lazydiff_diffs::DiffMode::Unified => "unified",
    }
}

fn parse_diff_mode(value: &str) -> lazydiff_diffs::DiffMode {
    match value {
        "unified" => lazydiff_diffs::DiffMode::Unified,
        _ => lazydiff_diffs::DiffMode::Split,
    }
}

fn parse_diff_side(value: &str) -> DiffSide {
    match value {
        "left" => DiffSide::Left,
        _ => DiffSide::Right,
    }
}

fn xdg_data_home() -> PathBuf {
    if let Ok(value) = env::var("XDG_DATA_HOME") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Ok(value) = env::var("HOME") {
        if !value.trim().is_empty() {
            return PathBuf::from(value).join(".local").join("share");
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn diff_line_kind_name(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Context => "context",
        DiffLineKind::Add => "add",
        DiffLineKind::Delete => "delete",
    }
}

fn parse_diff_line_kind(value: &str) -> DiffLineKind {
    match value {
        "add" => DiffLineKind::Add,
        "delete" => DiffLineKind::Delete,
        _ => DiffLineKind::Context,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_items_are_scoped_by_route_session_id() {
        let path = env::temp_dir().join(format!(
            "quiver-review-store-{}-{}.sqlite3",
            std::process::id(),
            now_stamp()
        ));
        let store = ReviewStore {
            path: Some(path.clone()),
        };
        store.init().expect("store initializes");
        let document = lazydiff_diffs::parse_unified_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -0,0 +1 @@\n+hello\n",
        );

        let mut local = ReviewSession::load_or_create_scoped(
            &store,
            "local-route".to_string(),
            WorkItemKind::LocalAgentBranch,
            "/repo".to_string(),
            "feature".to_string(),
            "HEAD".to_string(),
            "local:/repo:feature:HEAD",
            &document,
        );
        let pr = ReviewSession::load_or_create_scoped(
            &store,
            "pr-route".to_string(),
            WorkItemKind::RequestedPrReview,
            "owner/repo".to_string(),
            "PR #7".to_string(),
            "pull/7".to_string(),
            "pr:owner/repo#7",
            &document,
        );
        let target = DiffLineTarget {
            file_index: 0,
            hunk_index: 0,
            line_index: 0,
            path: "a.txt".to_string(),
            side: DiffSide::Right,
            old_line: None,
            new_line: Some(1),
            line: 1,
            kind: DiffLineKind::Add,
        };
        local.add_note(
            &store,
            DiffLineRangeTarget::single(target),
            ReviewItemKind::Note,
            None,
            "local-only feedback".to_string(),
        );

        let local = store
            .load_session(&local.id)
            .expect("local session reloads");
        let pr = store.load_session(&pr.id).expect("pr session reloads");
        assert_eq!(local.notes.len(), 1);
        assert!(pr.notes.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn theme_preference_round_trips_through_sqlite_cache() {
        let path = env::temp_dir().join(format!(
            "quiver-theme-store-{}-{}.sqlite3",
            std::process::id(),
            now_stamp()
        ));
        let store = ReviewStore {
            path: Some(path.clone()),
        };
        store.init().expect("store initializes");

        assert_eq!(store.restore_theme_variant(), None);
        store.persist_theme_variant(ThemeVariant::Graphite);
        assert_eq!(store.restore_theme_variant(), Some(ThemeVariant::Graphite));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn viewed_state_round_trips_through_sqlite_cache() {
        let path = env::temp_dir().join(format!(
            "quiver-viewed-store-{}-{}.sqlite3",
            std::process::id(),
            now_stamp()
        ));
        let store = ReviewStore {
            path: Some(path.clone()),
        };
        store.init().expect("store initializes");

        assert!(store.restore_viewed_state("session-a").files.is_empty());
        store.persist_viewed_state(
            "session-a",
            &PersistedViewedState {
                files: vec!["src/app.rs".to_string()],
                entities: vec!["src/app.rs\u{1f}fn\u{1f}run".to_string()],
            },
        );
        let restored = store.restore_viewed_state("session-a");
        assert_eq!(restored.files, vec!["src/app.rs"]);
        assert_eq!(restored.entities, vec!["src/app.rs\u{1f}fn\u{1f}run"]);
        assert!(store.restore_viewed_state("session-b").files.is_empty());

        let _ = fs::remove_file(path);
    }
}
