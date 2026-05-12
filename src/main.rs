use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use color_eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend, TestBackend},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Line,
    widgets::StatefulWidget,
    Terminal,
};
use ratatui_diffs::{
    add_pierre_highlights, parse_unified_diff, row_count_for_mode, DiffDocument, DiffMode,
    DiffViewState, DiffWidget,
};

mod app;
mod bounded_map;
mod commands;
mod components;
mod design_system;
mod github;
mod persistence;
mod server_query;
mod text;
mod ui;

use app::App;
pub(crate) use app::CommandResult;
pub(crate) use design_system::{FinderPalette, HomePalette};
pub(crate) use github::{GitHubComment, GitHubQueue};
use persistence::{ReviewItemKind, ReviewItemState, ReviewStore, ReviewThread};
pub(crate) use text::relative_unix_age;
pub(crate) use ui::{draw_box, fill_rect, right_aligned_text, truncate};

fn main() -> Result<()> {
    color_eyre::install()?;
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/nodejs-node-63115.diff"
    );
    let mut args: Vec<String> = env::args().skip(1).collect();
    let bench_scroll = args.first().is_some_and(|arg| arg == "--bench-scroll");
    if bench_scroll {
        args.remove(0);
    }
    if args.first().is_some_and(|arg| arg == "agent") {
        return run_agent_cli(&args[1..]);
    }
    let launch = LaunchInput::parse(args)?;
    let path = if bench_scroll {
        launch.label().unwrap_or_else(|| fixture.to_string())
    } else {
        launch.label().unwrap_or_else(|| "worktree".to_string())
    };
    let patch = if bench_scroll {
        fs::read_to_string(&path)?
    } else {
        launch.load_patch()?
    };
    let mut document = parse_unified_diff(&patch);
    let highlight_start = Instant::now();
    let highlight_stats = add_pierre_highlights(&mut document);
    eprintln!(
        "[lazydiff] pierre highlighted files={} sides={} spans={} in {:.3}ms",
        highlight_stats.files_highlighted,
        highlight_stats.sides_highlighted,
        highlight_stats.spans,
        highlight_start.elapsed().as_secs_f64() * 1000.0,
    );
    if bench_scroll {
        return bench_scroll_render(path, patch.len(), document);
    }
    let mut terminal = init_terminal()?;
    let app = match launch {
        LaunchInput::Home => App::new(path, patch.len(), document),
        LaunchInput::LocalDiff {
            label, base_ref, ..
        } => {
            let metadata = GitMetadata::detect()?;
            App::new_local_diff(
                label,
                patch.len(),
                document,
                metadata.repo_path,
                metadata.branch,
                base_ref,
            )
        }
        LaunchInput::Commit { ref_name } => {
            let metadata = GitMetadata::detect()?;
            App::new_commit_diff(path, patch.len(), document, metadata.repo_path, ref_name)
        }
        LaunchInput::Patch { .. } | LaunchInput::Difftool { .. } => App::new_local_diff(
            path,
            patch.len(),
            document,
            "patch".to_string(),
            "patch".to_string(),
            "file".to_string(),
        ),
    };
    let result = app.run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn run_agent_cli(args: &[String]) -> Result<()> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{}", agent_help_text());
        return Ok(());
    };
    if matches!(command, "--help" | "-h" | "help") {
        println!("{}", agent_help_text());
        return Ok(());
    }

    let store = ReviewStore::open_default()?;
    match command {
        "list" => agent_list(&store, &args[1..]),
        "thread" | "show" => {
            let id = required_arg(args, 1, "thread id")?;
            agent_thread(&store, id)
        }
        "reply" | "comment" => {
            let id = required_arg(args, 1, "thread id")?;
            let body = read_body_arg(&args[2..])?;
            agent_reply(&store, id, body)
        }
        "resolve" => {
            let id = required_arg(args, 1, "thread id")?;
            agent_set_state(&store, id, ReviewItemState::Resolved)
        }
        "state" => {
            let id = required_arg(args, 1, "thread id")?;
            let state = ReviewItemState::from_label(required_arg(args, 2, "state")?);
            agent_set_state(&store, id, state)
        }
        other => Err(color_eyre::eyre::eyre!(
            "unknown agent command `{other}`\n\n{}",
            agent_help_text()
        )),
    }
}

fn agent_list(store: &ReviewStore, args: &[String]) -> Result<()> {
    let filter = AgentListFilter::parse(args)?;
    let threads = store
        .list_review_threads()
        .into_iter()
        .filter(|thread| filter.matches(thread))
        .collect::<Vec<_>>();
    print_threads_json(&threads)
}

struct AgentListFilter {
    include_all: bool,
    repo_path: Option<String>,
    branch: Option<String>,
    base_ref: Option<String>,
}

impl AgentListFilter {
    fn parse(args: &[String]) -> Result<Self> {
        let mut include_all = false;
        let mut repo_path = None;
        let mut branch = None;
        let mut base_ref = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--all" => include_all = true,
                "--repo" => {
                    repo_path = Some(required_arg(args, index + 1, "repo path")?.to_string());
                    index += 1;
                }
                "--branch" => {
                    branch = Some(required_arg(args, index + 1, "branch")?.to_string());
                    index += 1;
                }
                "--base" => {
                    base_ref = Some(required_arg(args, index + 1, "base ref")?.to_string());
                    index += 1;
                }
                other => {
                    return Err(color_eyre::eyre::eyre!(
                        "unknown agent list option `{other}`"
                    ))
                }
            }
            index += 1;
        }

        if !include_all {
            if repo_path.is_none() || branch.is_none() {
                if let Ok(metadata) = GitMetadata::detect() {
                    repo_path.get_or_insert(metadata.repo_path);
                    branch.get_or_insert(metadata.branch);
                }
            }
            if base_ref.is_none() {
                base_ref = detect_base_ref().ok();
            }
        }

        Ok(Self {
            include_all,
            repo_path,
            branch,
            base_ref,
        })
    }

    fn matches(&self, thread: &ReviewThread) -> bool {
        if !self.include_all && !thread.note.state.is_open() {
            return false;
        }
        if let Some(repo_path) = &self.repo_path {
            if &thread.session.repo_path != repo_path {
                return false;
            }
        }
        if let Some(branch) = &self.branch {
            if &thread.session.branch != branch {
                return false;
            }
        }
        if let Some(base_ref) = &self.base_ref {
            if &thread.session.base_ref != base_ref {
                return false;
            }
        }
        true
    }
}

fn agent_thread(store: &ReviewStore, id: &str) -> Result<()> {
    let (session_id, note_id) = parse_thread_id(id)?;
    let threads = store
        .list_review_threads()
        .into_iter()
        .filter(|thread| {
            thread.note.session_id == session_id
                && (thread.note.id == note_id || thread.note.parent_id == Some(note_id))
        })
        .collect::<Vec<_>>();
    print_threads_json(&threads)
}

fn agent_reply(store: &ReviewStore, id: &str, body: String) -> Result<()> {
    let (session_id, note_id) = parse_thread_id(id)?;
    let mut session = store
        .load_session(&session_id)
        .ok_or_else(|| color_eyre::eyre::eyre!("unknown session `{session_id}`"))?;
    let parent = session
        .notes
        .iter()
        .find(|note| note.id == note_id)
        .cloned()
        .ok_or_else(|| color_eyre::eyre::eyre!("unknown thread `{id}`"))?;
    session.add_note(
        store,
        parent.target.clone(),
        ReviewItemKind::AgentCheck,
        Some(parent.id),
        body,
    );
    if parent.kind == ReviewItemKind::Question {
        store.update_note_state(&session_id, note_id, ReviewItemState::Answered);
    }
    println!(
        "{}",
        serde_json::json!({ "ok": true, "thread_id": id }).to_string()
    );
    Ok(())
}

fn agent_set_state(store: &ReviewStore, id: &str, state: ReviewItemState) -> Result<()> {
    let (session_id, note_id) = parse_thread_id(id)?;
    if !store.update_note_state(&session_id, note_id, state) {
        return Err(color_eyre::eyre::eyre!("unknown thread `{id}`"));
    }
    println!(
        "{}",
        serde_json::json!({ "ok": true, "thread_id": id, "state": state.label() }).to_string()
    );
    Ok(())
}

fn print_threads_json(threads: &[ReviewThread]) -> Result<()> {
    let value = threads.iter().map(thread_json).collect::<Vec<_>>();
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn thread_json(thread: &ReviewThread) -> serde_json::Value {
    let note = &thread.note;
    serde_json::json!({
        "id": format!("{}:{}", note.session_id, note.id),
        "session_id": note.session_id,
        "note_id": note.id,
        "parent_id": note.parent_id.map(|parent| format!("{}:{}", note.session_id, parent)),
        "kind": note.kind.label(),
        "state": note.state.label(),
        "author": note.author,
        "body": note.body,
        "created_at": note.created_at,
        "session": {
            "kind": thread.session.kind.label(),
            "repo_path": thread.session.repo_path,
            "branch": thread.session.branch,
            "base_ref": thread.session.base_ref,
        },
        "target": {
            "path": note.target.path(),
            "side": format!("{:?}", note.target.side()).to_ascii_lowercase(),
            "start_line": note.target.start.line,
            "end_line": note.target.end.line,
            "old_line": note.target.start.old_line,
            "new_line": note.target.start.new_line,
        }
    })
}

fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| color_eyre::eyre::eyre!("missing {label}"))
}

fn parse_thread_id(id: &str) -> Result<(String, u64)> {
    let (session_id, note_id) = id.rsplit_once(':').ok_or_else(|| {
        color_eyre::eyre::eyre!("thread id must look like <session-id>:<note-id>")
    })?;
    Ok((session_id.to_string(), note_id.parse()?))
}

fn read_body_arg(args: &[String]) -> Result<String> {
    if let Some(first) = args.first() {
        match first.as_str() {
            "--body" | "-m" => return Ok(required_arg(args, 1, "body")?.to_string()),
            "--body-file" | "-F" => {
                return Ok(fs::read_to_string(required_arg(args, 1, "body file")?)?)
            }
            value => return Ok(value.to_string()),
        }
    }
    let mut body = String::new();
    io::Read::read_to_string(&mut io::stdin(), &mut body)?;
    Ok(body)
}

fn agent_help_text() -> &'static str {
    "Usage: lazydiff agent <command>\n\nCommands:\n  lazydiff agent list [--all] [--repo <path>] [--branch <name>] [--base <ref>]\n                                      list open current repo/branch threads as JSON\n  lazydiff agent thread <id>          show one thread and replies as JSON\n  lazydiff agent reply <id> --body <text>\n                                      add an agent reply/check to a thread\n  lazydiff agent resolve <id>         mark a thread resolved\n  lazydiff agent state <id> <state>   set state: open, answered, requested, changed, resolved, carried, stale\n\nThread ids come from `agent list` and look like <session-id>:<note-id>.\n"
}

#[derive(Clone, Debug)]
enum LaunchInput {
    Home,
    LocalDiff {
        label: String,
        base_ref: String,
        args: Vec<String>,
    },
    Commit {
        ref_name: String,
    },
    Patch {
        label: String,
        file: Option<PathBuf>,
        stdin: bool,
    },
    Difftool {
        left: PathBuf,
        right: PathBuf,
        label: String,
    },
}

impl LaunchInput {
    fn parse(args: Vec<String>) -> Result<Self> {
        if args.is_empty() {
            return Ok(Self::Home);
        }

        match args[0].as_str() {
            "--help" | "-h" | "help" => {
                println!("{}", help_text());
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--branch" => Self::branch_diff(None),
            "--worktree" => Self::worktree_diff(false, Vec::new()),
            "diff" => Self::parse_diff(args[1..].to_vec()),
            "show" => Ok(Self::Commit {
                ref_name: args.get(1).cloned().unwrap_or_else(|| "HEAD".to_string()),
            }),
            "patch" => Ok(Self::parse_patch(args.get(1).cloned())),
            "pager" => Ok(Self::Patch {
                label: "stdin".to_string(),
                file: None,
                stdin: true,
            }),
            "difftool" if args.len() >= 3 => Ok(Self::Difftool {
                left: PathBuf::from(&args[1]),
                right: PathBuf::from(&args[2]),
                label: args
                    .get(3)
                    .cloned()
                    .unwrap_or_else(|| "difftool".to_string()),
            }),
            first if first.starts_with('-') => Err(color_eyre::eyre::eyre!(
                "unknown lazydiff option `{first}`\n\n{}",
                help_text()
            )),
            patch_file => Ok(Self::Patch {
                label: patch_file.to_string(),
                file: Some(PathBuf::from(patch_file)),
                stdin: false,
            }),
        }
    }

    fn parse_diff(args: Vec<String>) -> Result<Self> {
        let mut staged = false;
        let mut target: Option<String> = None;
        let mut pathspecs = Vec::new();
        let mut after_separator = false;

        for arg in args {
            if after_separator {
                pathspecs.push(arg);
                continue;
            }
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{}", help_text());
                    std::process::exit(0);
                }
                "--" => after_separator = true,
                "--staged" | "--cached" => staged = true,
                value if value.starts_with('-') => {
                    return Err(color_eyre::eyre::eyre!("unknown diff option `{value}`"));
                }
                value => {
                    if target.is_none() {
                        target = Some(value.to_string());
                    } else {
                        pathspecs.push(value.to_string());
                    }
                }
            }
        }

        if let Some(target) = target {
            Self::branch_diff(Some((target, pathspecs)))
        } else {
            Self::worktree_diff(staged, pathspecs)
        }
    }

    fn parse_patch(file: Option<String>) -> Self {
        match file.as_deref() {
            Some("-") => Self::Patch {
                label: "stdin".to_string(),
                file: None,
                stdin: true,
            },
            Some(file) => Self::Patch {
                label: file.to_string(),
                file: Some(PathBuf::from(file)),
                stdin: false,
            },
            None => Self::Patch {
                label: "stdin".to_string(),
                file: None,
                stdin: true,
            },
        }
    }

    fn worktree_diff(staged: bool, pathspecs: Vec<String>) -> Result<Self> {
        let base_ref = if staged { "--cached" } else { "HEAD" }.to_string();
        let mut diff_args = vec![
            "diff".to_string(),
            "--no-ext-diff".to_string(),
            "--binary".to_string(),
        ];
        if staged {
            diff_args.push("--cached".to_string());
        } else {
            diff_args.push("HEAD".to_string());
        }
        append_pathspecs(&mut diff_args, pathspecs);
        Ok(Self::LocalDiff {
            label: if staged { "staged" } else { "worktree" }.to_string(),
            base_ref,
            args: diff_args,
        })
    }

    fn branch_diff(target: Option<(String, Vec<String>)>) -> Result<Self> {
        let (base_ref, pathspecs) = match target {
            Some((target, pathspecs)) => (target, pathspecs),
            None => (detect_base_ref()?, Vec::new()),
        };
        let mut diff_args = vec![
            "diff".to_string(),
            "--no-ext-diff".to_string(),
            "--binary".to_string(),
            base_ref.clone(),
        ];
        append_pathspecs(&mut diff_args, pathspecs);
        Ok(Self::LocalDiff {
            label: format!("branch vs {base_ref}"),
            base_ref,
            args: diff_args,
        })
    }

    fn label(&self) -> Option<String> {
        match self {
            Self::Home => None,
            Self::LocalDiff { label, .. } => Some(label.clone()),
            Self::Commit { ref_name } => Some(format!("show {ref_name}")),
            Self::Patch { label, .. } => Some(label.clone()),
            Self::Difftool { label, .. } => Some(label.clone()),
        }
    }

    fn load_patch(&self) -> Result<String> {
        match self {
            Self::Home => Ok(String::new()),
            Self::LocalDiff { args, .. } => run_git_dynamic(Path::new("."), args),
            Self::Commit { ref_name } => run_git_dynamic(
                Path::new("."),
                &[
                    "show".to_string(),
                    "--format=".to_string(),
                    "--patch".to_string(),
                    "--no-ext-diff".to_string(),
                    "--binary".to_string(),
                    ref_name.clone(),
                ],
            ),
            Self::Patch { file, stdin, .. } => {
                if *stdin {
                    let mut patch = String::new();
                    io::Read::read_to_string(&mut io::stdin(), &mut patch)?;
                    Ok(patch)
                } else {
                    Ok(fs::read_to_string(
                        file.as_ref().expect("patch file is set"),
                    )?)
                }
            }
            Self::Difftool { left, right, .. } => run_git_dynamic(
                Path::new("."),
                &[
                    "diff".to_string(),
                    "--no-index".to_string(),
                    "--no-ext-diff".to_string(),
                    "--binary".to_string(),
                    left.display().to_string(),
                    right.display().to_string(),
                ],
            ),
        }
    }
}

struct GitMetadata {
    repo_path: String,
    branch: String,
}

impl GitMetadata {
    fn detect() -> Result<Self> {
        let repo_path = git_stdout(["rev-parse", "--show-toplevel"])?;
        let branch = git_stdout(["branch", "--show-current"])
            .or_else(|_| git_stdout(["rev-parse", "--abbrev-ref", "HEAD"]))
            .unwrap_or_else(|_| "detached-head".to_string());
        Ok(Self { repo_path, branch })
    }
}

fn append_pathspecs(args: &mut Vec<String>, pathspecs: Vec<String>) {
    if pathspecs.is_empty() {
        return;
    }
    args.push("--".to_string());
    args.extend(pathspecs);
}

fn detect_base_ref() -> Result<String> {
    if let Ok(upstream) = git_stdout([
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]) {
        return Ok(upstream);
    }
    for candidate in ["origin/main", "origin/master", "main", "master"] {
        if git_success(["rev-parse", "--verify", candidate]) {
            return Ok(candidate.to_string());
        }
    }
    Ok("HEAD".to_string())
}

fn git_success<const N: usize>(args: [&str; N]) -> bool {
    Command::new("git")
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_stdout<const N: usize>(args: [&str; N]) -> Result<String> {
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    run_git_dynamic(Path::new("."), &args).map(|value| value.trim().to_string())
}

fn run_git_dynamic(cwd: &Path, args: &[String]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    let no_index_with_differences =
        args.get(1).is_some_and(|arg| arg == "--no-index") && output.status.code() == Some(1);
    if output.status.success() || no_index_with_differences {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(color_eyre::eyre::eyre!(
        "git {} failed{}{}",
        args.join(" "),
        if stderr.is_empty() { "" } else { ": " },
        stderr
    ))
}

fn help_text() -> &'static str {
    "Usage: lazydiff [command] [options]\n\nCommands:\n  lazydiff                         open the default review queue/home\n  lazydiff diff [target] [-- paths] review working tree or branch diff\n  lazydiff diff --staged           review staged changes\n  lazydiff show [ref]              review a commit (default HEAD)\n  lazydiff patch [file|-]          review a patch file or stdin\n  lazydiff pager                   read a patch from stdin\n  lazydiff difftool <left> <right> review two files\n\nShortcuts:\n  lazydiff --branch                review current branch vs upstream/base\n  lazydiff --worktree              review worktree vs HEAD\n"
}

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

fn init_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn bench_scroll_render(path: String, bytes: usize, document: DiffDocument) -> Result<()> {
    let rows = row_count_for_mode(&document, DiffMode::Split);
    let mut state = DiffViewState::default();
    let backend = TestBackend::new(180, 50);
    let mut terminal = Terminal::new(backend)?;
    let mut total = Duration::ZERO;
    let mut max = Duration::ZERO;
    let iterations = 1_000usize;

    let start_all = Instant::now();
    for _ in 0..iterations {
        state.scroll_y = state
            .scroll_y
            .saturating_add(1)
            .min(rows.saturating_sub(49));
        let start = Instant::now();
        terminal.draw(|frame| {
            let [header, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(frame.area());
            frame.render_widget(
                Line::from(" Ratatui diff benchmark ")
                    .style(Style::new().fg(Color::White).bg(Color::Rgb(17, 24, 39))),
                header,
            );
            StatefulWidget::render(
                DiffWidget::new(&document),
                body,
                frame.buffer_mut(),
                &mut state,
            );
        })?;
        let elapsed = start.elapsed();
        total += elapsed;
        max = max.max(elapsed);
    }
    let elapsed_all = start_all.elapsed();
    println!(
        "ratatui scroll bench: fixture={path} bytes={bytes} files={} rows={rows} iterations={iterations} avg_draw_ms={:.3} max_draw_ms={:.3} total_ms={:.3} final_selected={} final_scroll={}",
        document.files.len(),
        (total / iterations as u32).as_secs_f64() * 1000.0,
        max.as_secs_f64() * 1000.0,
        elapsed_all.as_secs_f64() * 1000.0,
        state.selected_row,
        state.scroll_y,
    );
    Ok(())
}
