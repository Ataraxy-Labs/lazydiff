use std::{
    env, fs,
    fs::OpenOptions,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use lazydiff_diffs::{SourceSyntaxHighlighter, SyntaxHighlightKind, SyntaxSpan};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HIGHLIGHTD_PROTOCOL_VERSION: u32 = 3;
const MAX_CACHE_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHE_DIR_BYTES: u64 = 256 * 1024 * 1024;
const HIGHLIGHT_CACHE_FORMAT_VERSION: u32 = 1;

#[derive(Serialize)]
struct HighlightCacheKey<'a> {
    protocol_version: u32,
    cache_format_version: u32,
    kind: HighlightCacheKind,
    path: &'a str,
    source_digest: String,
    window: Option<HighlightLineWindow>,
}

#[derive(Serialize)]
enum HighlightCacheKind {
    Full,
    Window,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HighlightRequest {
    pub(crate) request_id: u64,
    pub(crate) files: Vec<HighlightFileRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HighlightFileRequest {
    pub(crate) file_index: usize,
    pub(crate) old_path: Option<String>,
    pub(crate) path: String,
    pub(crate) old_source: Option<String>,
    pub(crate) new_source: Option<String>,
    pub(crate) old_line_window: Option<HighlightLineWindow>,
    pub(crate) new_line_window: Option<HighlightLineWindow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HighlightLineWindow {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HighlightResponse {
    pub(crate) request_id: u64,
    pub(crate) files: Vec<HighlightFileResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HighlightFileResponse {
    pub(crate) file_index: usize,
    pub(crate) old_path: Option<String>,
    pub(crate) path: String,
    pub(crate) old_source_lines: Option<Vec<String>>,
    pub(crate) new_source_lines: Option<Vec<String>>,
    pub(crate) old_line_window: Option<HighlightLineWindow>,
    pub(crate) new_line_window: Option<HighlightLineWindow>,
    pub(crate) old_spans: Option<Vec<Vec<WireSyntaxSpan>>>,
    pub(crate) new_spans: Option<Vec<Vec<WireSyntaxSpan>>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct WireSyntaxSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) kind: WireSyntaxKind,
    pub(crate) style: Option<WireStyle>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct WireStyle {
    pub(crate) fg: Option<WireColor>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underlined: bool,
    pub(crate) crossed_out: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) enum WireColor {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) enum WireSyntaxKind {
    Comment,
    Keyword,
    String,
    Number,
    Boolean,
    Function,
    Type,
    Property,
    Punctuation,
    Markup,
}

impl From<SyntaxSpan> for WireSyntaxSpan {
    fn from(span: SyntaxSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
            kind: span.kind.into(),
            style: span.style.map(Into::into),
        }
    }
}

impl From<WireSyntaxSpan> for SyntaxSpan {
    fn from(span: WireSyntaxSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
            kind: span.kind.into(),
            style: span.style.map(Into::into),
        }
    }
}

impl From<Style> for WireStyle {
    fn from(style: Style) -> Self {
        Self {
            fg: style.fg.map(Into::into),
            bold: style.add_modifier.contains(Modifier::BOLD),
            italic: style.add_modifier.contains(Modifier::ITALIC),
            underlined: style.add_modifier.contains(Modifier::UNDERLINED),
            crossed_out: style.add_modifier.contains(Modifier::CROSSED_OUT),
        }
    }
}

impl From<WireStyle> for Style {
    fn from(style: WireStyle) -> Self {
        let mut modifiers = Modifier::empty();
        if style.bold {
            modifiers |= Modifier::BOLD;
        }
        if style.italic {
            modifiers |= Modifier::ITALIC;
        }
        if style.underlined {
            modifiers |= Modifier::UNDERLINED;
        }
        if style.crossed_out {
            modifiers |= Modifier::CROSSED_OUT;
        }
        let mut out = Style::new().add_modifier(modifiers);
        if let Some(fg) = style.fg {
            out = out.fg(fg.into());
        }
        out
    }
}

impl From<Color> for WireColor {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => Self::Reset,
            Color::Black => Self::Black,
            Color::Red => Self::Red,
            Color::Green => Self::Green,
            Color::Yellow => Self::Yellow,
            Color::Blue => Self::Blue,
            Color::Magenta => Self::Magenta,
            Color::Cyan => Self::Cyan,
            Color::Gray => Self::Gray,
            Color::DarkGray => Self::DarkGray,
            Color::LightRed => Self::LightRed,
            Color::LightGreen => Self::LightGreen,
            Color::LightYellow => Self::LightYellow,
            Color::LightBlue => Self::LightBlue,
            Color::LightMagenta => Self::LightMagenta,
            Color::LightCyan => Self::LightCyan,
            Color::White => Self::White,
            Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
            Color::Indexed(index) => Self::Indexed(index),
        }
    }
}

impl From<WireColor> for Color {
    fn from(color: WireColor) -> Self {
        match color {
            WireColor::Reset => Self::Reset,
            WireColor::Black => Self::Black,
            WireColor::Red => Self::Red,
            WireColor::Green => Self::Green,
            WireColor::Yellow => Self::Yellow,
            WireColor::Blue => Self::Blue,
            WireColor::Magenta => Self::Magenta,
            WireColor::Cyan => Self::Cyan,
            WireColor::Gray => Self::Gray,
            WireColor::DarkGray => Self::DarkGray,
            WireColor::LightRed => Self::LightRed,
            WireColor::LightGreen => Self::LightGreen,
            WireColor::LightYellow => Self::LightYellow,
            WireColor::LightBlue => Self::LightBlue,
            WireColor::LightMagenta => Self::LightMagenta,
            WireColor::LightCyan => Self::LightCyan,
            WireColor::White => Self::White,
            WireColor::Rgb(r, g, b) => Self::Rgb(r, g, b),
            WireColor::Indexed(index) => Self::Indexed(index),
        }
    }
}

impl From<SyntaxHighlightKind> for WireSyntaxKind {
    fn from(kind: SyntaxHighlightKind) -> Self {
        match kind {
            SyntaxHighlightKind::Comment => Self::Comment,
            SyntaxHighlightKind::Keyword => Self::Keyword,
            SyntaxHighlightKind::String => Self::String,
            SyntaxHighlightKind::Number => Self::Number,
            SyntaxHighlightKind::Boolean => Self::Boolean,
            SyntaxHighlightKind::Function => Self::Function,
            SyntaxHighlightKind::Type => Self::Type,
            SyntaxHighlightKind::Property => Self::Property,
            SyntaxHighlightKind::Punctuation => Self::Punctuation,
            SyntaxHighlightKind::Markup => Self::Markup,
        }
    }
}

impl From<WireSyntaxKind> for SyntaxHighlightKind {
    fn from(kind: WireSyntaxKind) -> Self {
        match kind {
            WireSyntaxKind::Comment => Self::Comment,
            WireSyntaxKind::Keyword => Self::Keyword,
            WireSyntaxKind::String => Self::String,
            WireSyntaxKind::Number => Self::Number,
            WireSyntaxKind::Boolean => Self::Boolean,
            WireSyntaxKind::Function => Self::Function,
            WireSyntaxKind::Type => Self::Type,
            WireSyntaxKind::Property => Self::Property,
            WireSyntaxKind::Punctuation => Self::Punctuation,
            WireSyntaxKind::Markup => Self::Markup,
        }
    }
}

#[cfg(unix)]
pub(crate) fn run_highlight_daemon() -> color_eyre::Result<()> {
    let socket = socket_path();
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)?;
    }
    if socket.exists() {
        if UnixStream::connect(&socket).is_ok() {
            return Ok(());
        }
        let _ = fs::remove_file(&socket);
    }
    let listener = UnixListener::bind(&socket)?;
    let mut highlighter = SourceSyntaxHighlighter::new()
        .ok_or_else(|| color_eyre::eyre::eyre!("failed to initialize Pierre highlighter"))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = handle_stream(stream, &mut highlighter);
            }
            Err(error) => eprintln!("[lazydiff-highlightd] accept failed: {error}"),
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn run_highlight_daemon() -> color_eyre::Result<()> {
    Err(color_eyre::eyre::eyre!(
        "highlight daemon requires Unix sockets and is not available on this platform"
    ))
}

#[cfg(unix)]
pub(crate) fn request_highlights(request: &HighlightRequest) -> io::Result<HighlightResponse> {
    let mut stream = connect_or_spawn()?;
    serde_json::to_writer(&mut stream, request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(io::Error::other)
}

#[cfg(not(unix))]
pub(crate) fn request_highlights(_request: &HighlightRequest) -> io::Result<HighlightResponse> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "highlight daemon requires Unix sockets",
    ))
}

pub(crate) fn cached_highlights(request: &HighlightRequest) -> HighlightResponse {
    HighlightResponse {
        request_id: request.request_id,
        files: request
            .files
            .iter()
            .filter_map(cached_highlight_file)
            .collect(),
    }
}

#[cfg(unix)]
fn connect_or_spawn() -> io::Result<UnixStream> {
    let socket = socket_path();
    if let Ok(stream) = UnixStream::connect(&socket) {
        return Ok(stream);
    }

    let lock = acquire_spawn_lock()?;
    if let Ok(stream) = UnixStream::connect(&socket) {
        drop(lock);
        release_spawn_lock();
        return Ok(stream);
    }

    let exe = env::current_exe()?;
    let _child = Command::new(exe)
        .arg("highlightd")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let start = Instant::now();
    loop {
        match UnixStream::connect(&socket) {
            Ok(stream) => {
                drop(lock);
                release_spawn_lock();
                return Ok(stream);
            }
            Err(error) if start.elapsed() < Duration::from_millis(750) => {
                let _ = error;
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                drop(lock);
                release_spawn_lock();
                return Err(error);
            }
        }
    }
}

#[cfg(unix)]
fn acquire_spawn_lock() -> io::Result<fs::File> {
    let lock = lock_path();
    if let Some(parent) = lock.parent() {
        fs::create_dir_all(parent)?;
    }
    let start = Instant::now();
    loop {
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(file) => return Ok(file),
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists
                    && start.elapsed() < Duration::from_millis(750) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&lock);
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
fn release_spawn_lock() {
    let _ = fs::remove_file(lock_path());
}

#[cfg(unix)]
fn handle_stream(
    mut stream: UnixStream,
    highlighter: &mut SourceSyntaxHighlighter,
) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    let request: HighlightRequest = serde_json::from_str(&line).map_err(io::Error::other)?;
    let response = HighlightResponse {
        request_id: request.request_id,
        files: request
            .files
            .into_iter()
            .map(|file| highlight_file(file, highlighter))
            .collect(),
    };
    serde_json::to_writer(&mut stream, &response)?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn highlight_file(
    file: HighlightFileRequest,
    highlighter: &mut SourceSyntaxHighlighter,
) -> HighlightFileResponse {
    let old_highlight_path = file.old_path.as_deref().unwrap_or(&file.path);
    let old_spans = file.old_source.as_deref().and_then(|source| {
        highlight_source_for_request(
            old_highlight_path,
            source,
            file.old_line_window,
            highlighter,
        )
    });
    let new_spans = file.new_source.as_deref().and_then(|source| {
        highlight_source_for_request(&file.path, source, file.new_line_window, highlighter)
    });
    HighlightFileResponse {
        file_index: file.file_index,
        old_path: file.old_path,
        path: file.path,
        old_source_lines: file.old_source.as_deref().map(source_lines),
        new_source_lines: file.new_source.as_deref().map(source_lines),
        old_line_window: file.old_line_window,
        new_line_window: file.new_line_window,
        old_spans,
        new_spans,
    }
}

fn cached_highlight_file(file: &HighlightFileRequest) -> Option<HighlightFileResponse> {
    let old_highlight_path = file.old_path.as_deref().unwrap_or(&file.path);
    let old_spans = file.old_source.as_deref().and_then(|source| {
        cached_spans_for_request(old_highlight_path, source, file.old_line_window)
    });
    let new_spans = file
        .new_source
        .as_deref()
        .and_then(|source| cached_spans_for_request(&file.path, source, file.new_line_window));
    if file.old_source.is_some() != old_spans.is_some()
        || file.new_source.is_some() != new_spans.is_some()
        || (old_spans.is_none() && new_spans.is_none())
    {
        return None;
    }
    Some(HighlightFileResponse {
        file_index: file.file_index,
        old_path: file.old_path.clone(),
        path: file.path.clone(),
        old_source_lines: file.old_source.as_deref().map(source_lines),
        new_source_lines: file.new_source.as_deref().map(source_lines),
        old_line_window: file.old_line_window,
        new_line_window: file.new_line_window,
        old_spans,
        new_spans,
    })
}

fn highlight_source_for_request(
    path: &str,
    source: &str,
    window: Option<HighlightLineWindow>,
    highlighter: &mut SourceSyntaxHighlighter,
) -> Option<Vec<Vec<WireSyntaxSpan>>> {
    cached_spans_for_request(path, source, window).or_else(|| match window {
        Some(window) => {
            highlight_source_window(path, source, window, highlighter).inspect(|spans| {
                write_cached_window_spans_for_source(
                    path,
                    source,
                    window,
                    compact_window_spans(spans, window),
                )
            })
        }
        None => highlighter
            .highlight_source_lines_for_path(path, source)
            .map(wire_lines)
            .inspect(|spans| write_cached_spans_for_source(path, source, spans)),
    })
}

fn highlight_source_window(
    path: &str,
    source: &str,
    window: HighlightLineWindow,
    highlighter: &mut SourceSyntaxHighlighter,
) -> Option<Vec<Vec<WireSyntaxSpan>>> {
    let lines = source_lines(source);
    let (start_index, end_index) = window_indices(window, lines.len())?;
    let window_source = lines[start_index..end_index].join("\n");
    let highlighted = highlighter
        .highlight_source_lines_for_path(path, &window_source)
        .map(wire_lines)?;
    Some(expand_window_spans(highlighted, window, lines.len()))
}

fn cached_spans_for_request(
    path: &str,
    source: &str,
    window: Option<HighlightLineWindow>,
) -> Option<Vec<Vec<WireSyntaxSpan>>> {
    match window {
        Some(window) => cached_window_spans_for_source(path, source, window)
            .map(|spans| expand_window_spans(spans, window, source_lines(source).len()))
            .or_else(|| cached_spans_for_source(path, source)),
        None => cached_spans_for_source(path, source),
    }
}

fn compact_window_spans(
    spans: &[Vec<WireSyntaxSpan>],
    window: HighlightLineWindow,
) -> &[Vec<WireSyntaxSpan>] {
    let Some((start, end)) = window_indices(window, spans.len()) else {
        return &[];
    };
    &spans[start..end]
}

fn expand_window_spans(
    window_spans: Vec<Vec<WireSyntaxSpan>>,
    window: HighlightLineWindow,
    total_lines: usize,
) -> Vec<Vec<WireSyntaxSpan>> {
    let mut spans = vec![Vec::new(); total_lines];
    let Some((start, end)) = window_indices(window, total_lines) else {
        return spans;
    };
    for (index, line_spans) in (start..end).zip(window_spans) {
        spans[index] = line_spans;
    }
    spans
}

fn window_indices(window: HighlightLineWindow, total_lines: usize) -> Option<(usize, usize)> {
    if total_lines == 0 || window.start == 0 || window.end < window.start {
        return None;
    }
    let start = window.start.saturating_sub(1) as usize;
    if start >= total_lines {
        return None;
    }
    let end = (window.end as usize).min(total_lines);
    Some((start, end.max(start + 1)))
}

fn cached_spans_for_source(path: &str, source: &str) -> Option<Vec<Vec<WireSyntaxSpan>>> {
    let cache_path = cache_path_for_source(path, source);
    if fs::metadata(&cache_path).ok()?.len() > MAX_CACHE_ENTRY_BYTES {
        return None;
    }
    let bytes = fs::read(cache_path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn cached_window_spans_for_source(
    path: &str,
    source: &str,
    window: HighlightLineWindow,
) -> Option<Vec<Vec<WireSyntaxSpan>>> {
    let cache_path = cache_path_for_source_window(path, source, window);
    if fs::metadata(&cache_path).ok()?.len() > MAX_CACHE_ENTRY_BYTES {
        return None;
    }
    let bytes = fs::read(cache_path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_cached_spans_for_source(path: &str, source: &str, spans: &[Vec<WireSyntaxSpan>]) {
    let cache_path = cache_path_for_source(path, source);
    write_cache_file(cache_path, spans);
}

fn write_cached_window_spans_for_source(
    path: &str,
    source: &str,
    window: HighlightLineWindow,
    spans: &[Vec<WireSyntaxSpan>],
) {
    let cache_path = cache_path_for_source_window(path, source, window);
    write_cache_file(cache_path, spans);
}

fn write_cache_file(cache_path: PathBuf, spans: &[Vec<WireSyntaxSpan>]) {
    let Some(parent) = cache_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temp_path = cache_path.with_extension(format!("{}.tmp", std::process::id()));
    let Ok(bytes) = serde_json::to_vec(spans) else {
        return;
    };
    if bytes.len() as u64 > MAX_CACHE_ENTRY_BYTES {
        return;
    }
    if fs::write(&temp_path, bytes).is_ok() {
        if fs::rename(temp_path, cache_path).is_ok() {
            prune_cache_dir(MAX_CACHE_DIR_BYTES);
        }
    }
}

fn prune_cache_dir(max_bytes: u64) {
    prune_cache_dir_at(cache_dir(), max_bytes);
}

fn prune_cache_dir_at(dir: PathBuf, max_bytes: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ))
        })
        .collect::<Vec<_>>();
    let mut total = files.iter().map(|(_, len, _)| *len).sum::<u64>();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, len, _) in files {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn cache_path_for_source(path: &str, source: &str) -> PathBuf {
    cache_path_for_key(HighlightCacheKey {
        protocol_version: HIGHLIGHTD_PROTOCOL_VERSION,
        cache_format_version: HIGHLIGHT_CACHE_FORMAT_VERSION,
        kind: HighlightCacheKind::Full,
        path,
        source_digest: source_digest(source),
        window: None,
    })
}

fn cache_path_for_source_window(path: &str, source: &str, window: HighlightLineWindow) -> PathBuf {
    cache_path_for_key(HighlightCacheKey {
        protocol_version: HIGHLIGHTD_PROTOCOL_VERSION,
        cache_format_version: HIGHLIGHT_CACHE_FORMAT_VERSION,
        kind: HighlightCacheKind::Window,
        path,
        source_digest: source_digest(source),
        window: Some(window),
    })
}

fn cache_path_for_key(key: HighlightCacheKey<'_>) -> PathBuf {
    let encoded = serde_json::to_vec(&key).expect("highlight cache key serializes");
    let digest = Sha256::digest(&encoded);
    let mut cache_path = cache_dir();
    cache_path.push(format!("{digest:x}.json"));
    cache_path
}

fn source_digest(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    format!("{digest:x}")
}

fn cache_dir() -> PathBuf {
    if let Some(path) = env::var_os("LAZYDIFF_HIGHLIGHT_CACHE_DIR") {
        return PathBuf::from(path);
    }
    let mut dir = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir);
    dir.push("lazydiff");
    dir.push(format!("highlightd-v{HIGHLIGHTD_PROTOCOL_VERSION}"));
    dir
}

fn wire_lines(lines: Vec<Vec<SyntaxSpan>>) -> Vec<Vec<WireSyntaxSpan>> {
    lines
        .into_iter()
        .map(|line| line.into_iter().map(Into::into).collect())
        .collect()
}

fn source_lines(source: &str) -> Vec<String> {
    source.split('\n').map(ToString::to_string).collect()
}

#[cfg(unix)]
fn socket_path() -> PathBuf {
    let mut dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let user = env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    dir.push(format!("lazydiff-{user}"));
    dir.push(format!("highlightd-v{HIGHLIGHTD_PROTOCOL_VERSION}.sock"));
    dir
}

#[cfg(unix)]
fn lock_path() -> PathBuf {
    let mut path = socket_path();
    path.set_extension("lock");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::sync::mpsc;

    #[cfg(unix)]
    #[test]
    fn spawn_lock_allows_only_one_owner_at_a_time() {
        release_spawn_lock();
        let first = acquire_spawn_lock().expect("first lock");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let second = acquire_spawn_lock().expect("second lock after release");
            let _ = tx.send(second.metadata().is_ok());
            drop(second);
            release_spawn_lock();
        });

        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        release_spawn_lock();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(true));
    }

    #[cfg(unix)]
    #[test]
    fn active_socket_is_not_removed_as_stale() {
        let socket = socket_path();
        if let Some(parent) = socket.parent() {
            fs::create_dir_all(parent).expect("socket parent");
        }
        let _ = fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("bind active socket");

        assert!(UnixStream::connect(&socket).is_ok());
        assert!(socket.exists());

        drop(listener);
        let _ = fs::remove_file(&socket);
    }

    #[test]
    fn wire_syntax_span_preserves_pierre_style() {
        let original = SyntaxSpan {
            start: 1,
            end: 4,
            kind: SyntaxHighlightKind::Property,
            style: Some(
                Style::new()
                    .fg(Color::Rgb(12, 34, 56))
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC),
            ),
        };

        let wire = WireSyntaxSpan::from(original);
        let round_trip = SyntaxSpan::from(wire);

        assert_eq!(round_trip.start, original.start);
        assert_eq!(round_trip.end, original.end);
        assert_eq!(round_trip.kind, original.kind);
        let style = round_trip.style.expect("style preserved");
        assert_eq!(style.fg, Some(Color::Rgb(12, 34, 56)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn cached_highlights_return_exact_style_without_highlighter() {
        let request = HighlightRequest {
            request_id: 7,
            files: vec![HighlightFileRequest {
                file_index: 3,
                old_path: Some("src/lib.rs".to_string()),
                path: "src/lib.rs".to_string(),
                old_source: None,
                new_source: Some("fn main() {}\n".to_string()),
                old_line_window: None,
                new_line_window: None,
            }],
        };
        let span = WireSyntaxSpan {
            start: 0,
            end: 2,
            kind: WireSyntaxKind::Keyword,
            style: Some(WireStyle {
                fg: Some(WireColor::Rgb(1, 2, 3)),
                bold: true,
                italic: false,
                underlined: false,
                crossed_out: false,
            }),
        };
        write_cached_spans_for_source(
            "src/lib.rs",
            request.files[0].new_source.as_deref().expect("source"),
            &[vec![span]],
        );

        let response = cached_highlights(&request);

        assert_eq!(response.request_id, 7);
        assert_eq!(response.files.len(), 1);
        assert_eq!(response.files[0].file_index, 3);
        assert_eq!(
            response.files[0].new_source_lines.as_deref(),
            Some(&["fn main() {}".to_string(), String::new()][..])
        );
        let cached_span = response.files[0].new_spans.as_ref().expect("spans")[0][0];
        assert_eq!(cached_span.start, span.start);
        assert_eq!(cached_span.end, span.end);
        assert!(cached_span.style.expect("style").bold);
    }

    #[test]
    fn cached_highlights_use_old_path_for_renamed_old_source() {
        let request = HighlightRequest {
            request_id: 8,
            files: vec![HighlightFileRequest {
                file_index: 4,
                old_path: Some("src/lib.rs".to_string()),
                path: "src/lib.txt".to_string(),
                old_source: Some("fn old() {}\n".to_string()),
                new_source: None,
                old_line_window: None,
                new_line_window: None,
            }],
        };
        let span = WireSyntaxSpan {
            start: 0,
            end: 2,
            kind: WireSyntaxKind::Keyword,
            style: Some(WireStyle {
                fg: Some(WireColor::Rgb(4, 5, 6)),
                bold: false,
                italic: true,
                underlined: false,
                crossed_out: false,
            }),
        };
        write_cached_spans_for_source(
            "src/lib.rs",
            request.files[0].old_source.as_deref().expect("source"),
            &[vec![span]],
        );

        let response = cached_highlights(&request);

        assert_eq!(response.files.len(), 1);
        let cached_span = response.files[0].old_spans.as_ref().expect("old spans")[0][0];
        assert!(cached_span.style.expect("style").italic);
    }

    #[test]
    fn oversized_cache_entry_is_treated_as_miss() {
        let path = "src/too-large.rs";
        let source = "fn too_large() {}\n";
        let cache_path = cache_path_for_source(path, source);
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).expect("cache parent");
        }
        fs::write(
            &cache_path,
            vec![b'['; (MAX_CACHE_ENTRY_BYTES + 1) as usize],
        )
        .expect("oversized cache write");

        assert!(cached_spans_for_source(path, source).is_none());
        let _ = fs::remove_file(cache_path);
    }

    #[test]
    fn cached_window_highlights_expand_to_source_line_count() {
        let request = HighlightRequest {
            request_id: 9,
            files: vec![HighlightFileRequest {
                file_index: 5,
                old_path: None,
                path: "src/window.rs".to_string(),
                old_source: None,
                new_source: Some("fn one() {}\nfn two() {}\nfn three() {}".to_string()),
                old_line_window: None,
                new_line_window: Some(HighlightLineWindow { start: 2, end: 2 }),
            }],
        };
        let span = WireSyntaxSpan {
            start: 0,
            end: 2,
            kind: WireSyntaxKind::Keyword,
            style: None,
        };
        write_cached_window_spans_for_source(
            "src/window.rs",
            request.files[0].new_source.as_deref().expect("source"),
            HighlightLineWindow { start: 2, end: 2 },
            &[vec![span]],
        );

        let response = cached_highlights(&request);

        let spans = response.files[0].new_spans.as_ref().expect("window spans");
        assert_eq!(spans.len(), 3);
        assert!(spans[0].is_empty());
        assert_eq!(spans[1][0].start, span.start);
        assert!(spans[2].is_empty());
        assert_eq!(
            response.files[0].new_line_window,
            Some(HighlightLineWindow { start: 2, end: 2 })
        );
    }

    #[test]
    fn cache_pruning_removes_old_files_until_under_limit() {
        let dir = env::temp_dir().join(format!("lazydiff-cache-prune-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("cache dir");
        fs::write(dir.join("a.json"), vec![1; 10]).expect("a");
        thread::sleep(Duration::from_millis(2));
        fs::write(dir.join("b.json"), vec![1; 10]).expect("b");

        prune_cache_dir_at(dir.clone(), 10);

        let remaining = fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec!["b.json".to_string()]);
        let _ = fs::remove_dir_all(dir);
    }
}
