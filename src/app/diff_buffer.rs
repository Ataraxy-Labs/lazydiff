use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazydiff_diffs::{DiffSearchMatch, DiffViewerState};

const PENDING_KEY_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Clone, Debug, Default)]
pub(super) struct DiffBufferState {
    viewer: DiffViewerState,
    mode: DiffBufferMode,
    pending: PendingChord,
    command_line: String,
    help_visible: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DiffBufferMode {
    #[default]
    Normal,
    Visual,
    VisualLine,
    Search,
    Command,
    PendingTextObject(TextObjectKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextObjectKind {
    Inner,
    Around,
}

#[derive(Clone, Debug, Default)]
struct PendingChord {
    keys: String,
    at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiffBufferAction {
    None,
    Cancel,
    MoveRows(isize),
    MoveCols(isize),
    WordForward,
    BigWordForward,
    WordEndForward,
    BigWordEndForward,
    WordBackward,
    BigWordBackward,
    WordEndBackward,
    BigWordEndBackward,
    Page(isize),
    HalfPage(isize),
    Top,
    Bottom,
    LineStart,
    LineEnd,
    PreviousFile,
    NextFile,
    NextCommit,
    PreviousCommit,
    NextChange,
    PreviousChange,
    NextNote,
    PreviousNote,
    ToggleSideBySide,
    SwitchSide,
    OpenCommandPalette,
    OpenFileFinder,
    SearchChanged,
    SearchAccept,
    SearchNext,
    SearchPrevious,
    OpenThread,
    OpenEditor,
    ToggleVisual,
    ToggleVisualLine,
    SelectTextObject(TextObjectKind, char),
    YankSelection,
    OpenComment,
    DeleteNote,
    SaveComments,
    Quit { force: bool },
    ShowHelp,
}

impl DiffBufferState {
    pub(super) fn viewer(&self) -> &DiffViewerState {
        &self.viewer
    }

    pub(super) fn viewer_mut(&mut self) -> &mut DiffViewerState {
        &mut self.viewer
    }

    pub(super) fn sync_viewport(&mut self, width: u16, height: u16, top_margin: usize) {
        self.viewer.viewport.width = width;
        self.viewer.viewport.height = height;
        self.viewer.viewport.top_margin = top_margin;
    }

    pub(super) fn mode(&self) -> DiffBufferMode {
        self.mode
    }

    pub(super) fn search_matches(&self) -> &[DiffSearchMatch] {
        &self.viewer.search.matches
    }

    pub(super) fn clear_search_matches(&mut self) {
        self.viewer.clear_search_matches();
    }

    pub(super) fn search_query(&self) -> &str {
        &self.viewer.search.query
    }

    pub(super) fn command_line(&self) -> &str {
        &self.command_line
    }

    pub(super) fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub(super) fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub(super) fn clear_transient(&mut self) {
        self.mode = DiffBufferMode::Normal;
        self.pending.clear();
        self.command_line.clear();
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, now: Instant) -> DiffBufferAction {
        self.pending.clear_expired(now);

        if self.help_visible {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                    self.help_visible = false;
                    DiffBufferAction::None
                }
                _ => DiffBufferAction::None,
            };
        }

        match self.mode {
            DiffBufferMode::Search => return self.handle_search_key(key),
            DiffBufferMode::Command => return self.handle_command_key(key),
            DiffBufferMode::PendingTextObject(kind) => {
                return self.handle_text_object_key(key, kind);
            }
            DiffBufferMode::Normal | DiffBufferMode::Visual | DiffBufferMode::VisualLine => {}
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => return self.accept(DiffBufferAction::HalfPage(1)),
                KeyCode::Char('u') => return self.accept(DiffBufferAction::HalfPage(-1)),
                KeyCode::Char('p') => return self.accept(DiffBufferAction::OpenCommandPalette),
                KeyCode::Char('c') => return self.accept(DiffBufferAction::Cancel),
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => self.accept(DiffBufferAction::Cancel),
            KeyCode::Char('?') => self.accept(DiffBufferAction::ShowHelp),
            KeyCode::Char('[') if key.modifiers.is_empty() => {
                self.accept(DiffBufferAction::PreviousFile)
            }
            KeyCode::Char(']') if key.modifiers.is_empty() => {
                self.accept(DiffBufferAction::NextFile)
            }
            KeyCode::Char(' ') if key.modifiers.is_empty() => {
                self.pending.set(" ", now);
                DiffBufferAction::None
            }
            KeyCode::Char('c') if self.pending.keys == "]" => {
                self.accept(DiffBufferAction::NextChange)
            }
            KeyCode::Char('c') if self.pending.keys == "[" => {
                self.accept(DiffBufferAction::PreviousChange)
            }
            KeyCode::Char('n') if self.pending.keys == "]" => {
                self.accept(DiffBufferAction::NextNote)
            }
            KeyCode::Char('n') if self.pending.keys == "[" => {
                self.accept(DiffBufferAction::PreviousNote)
            }
            KeyCode::Char('e') if self.pending.keys == " " => {
                self.accept(DiffBufferAction::OpenFileFinder)
            }
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.pending.clear();
                self.viewer.search.query.clear();
                self.mode = DiffBufferMode::Search;
                DiffBufferAction::SearchChanged
            }
            KeyCode::Char(':') if key.modifiers.is_empty() => {
                self.pending.clear();
                self.command_line.clear();
                self.mode = DiffBufferMode::Command;
                DiffBufferAction::None
            }
            KeyCode::Char('n') if key.modifiers.is_empty() => {
                self.accept(DiffBufferAction::SearchNext)
            }
            KeyCode::Char('N') if key.modifiers.is_empty() => {
                self.accept(DiffBufferAction::SearchPrevious)
            }
            KeyCode::Char('j') | KeyCode::Down => self.accept(DiffBufferAction::MoveRows(1)),
            KeyCode::Char('k') | KeyCode::Up => self.accept(DiffBufferAction::MoveRows(-1)),
            KeyCode::Char('h') | KeyCode::Left => self.accept(DiffBufferAction::MoveCols(-1)),
            KeyCode::Char('l') | KeyCode::Right => self.accept(DiffBufferAction::MoveCols(1)),
            KeyCode::Char('w') => self.accept(DiffBufferAction::WordForward),
            KeyCode::Char('W') => self.accept(DiffBufferAction::BigWordForward),
            KeyCode::Char('e') if self.pending.keys == "g" => {
                self.accept(DiffBufferAction::WordEndBackward)
            }
            KeyCode::Char('E') if self.pending.keys == "g" => {
                self.accept(DiffBufferAction::BigWordEndBackward)
            }
            KeyCode::Char('e') => self.accept(DiffBufferAction::WordEndForward),
            KeyCode::Char('E') => self.accept(DiffBufferAction::BigWordEndForward),
            KeyCode::Char('b') => self.accept(DiffBufferAction::WordBackward),
            KeyCode::Char('B') => self.accept(DiffBufferAction::BigWordBackward),
            KeyCode::PageDown => self.accept(DiffBufferAction::Page(1)),
            KeyCode::PageUp => self.accept(DiffBufferAction::Page(-1)),
            KeyCode::Char('g') if self.pending.keys == "g" => self.accept(DiffBufferAction::Top),
            KeyCode::Char('g') if key.modifiers.is_empty() => {
                self.pending.set("g", now);
                DiffBufferAction::None
            }
            KeyCode::Char('G') | KeyCode::End => self.accept(DiffBufferAction::Bottom),
            KeyCode::Char('0') | KeyCode::Home => self.accept(DiffBufferAction::LineStart),
            KeyCode::Char('$') => self.accept(DiffBufferAction::LineEnd),
            KeyCode::Char('J') => self.accept(DiffBufferAction::NextCommit),
            KeyCode::Char('K') => self.accept(DiffBufferAction::PreviousCommit),
            KeyCode::Char('s') => self.accept(DiffBufferAction::ToggleSideBySide),
            KeyCode::Tab | KeyCode::BackTab => self.accept(DiffBufferAction::SwitchSide),
            KeyCode::Enter => self.accept(DiffBufferAction::OpenThread),
            KeyCode::Char('o') => self.accept(DiffBufferAction::OpenEditor),
            KeyCode::Char('v') => {
                self.mode = if self.mode == DiffBufferMode::Visual {
                    DiffBufferMode::Normal
                } else {
                    DiffBufferMode::Visual
                };
                self.accept(DiffBufferAction::ToggleVisual)
            }
            KeyCode::Char('V') => {
                self.mode = if self.mode == DiffBufferMode::VisualLine {
                    DiffBufferMode::Normal
                } else {
                    DiffBufferMode::VisualLine
                };
                self.accept(DiffBufferAction::ToggleVisualLine)
            }
            KeyCode::Char('a')
                if matches!(
                    self.mode,
                    DiffBufferMode::Visual | DiffBufferMode::VisualLine
                ) =>
            {
                self.pending.clear();
                self.mode = DiffBufferMode::PendingTextObject(TextObjectKind::Around);
                DiffBufferAction::None
            }
            KeyCode::Char('i') | KeyCode::Char('I') => self.accept(DiffBufferAction::OpenComment),
            KeyCode::Char('x') => self.accept(DiffBufferAction::DeleteNote),
            KeyCode::Char('d') if self.pending.keys == "d" => {
                self.accept(DiffBufferAction::DeleteNote)
            }
            KeyCode::Char('d') if key.modifiers.is_empty() => {
                self.pending.set("d", now);
                DiffBufferAction::None
            }
            KeyCode::Char('y') => self.accept(DiffBufferAction::YankSelection),
            _ => self.accept(DiffBufferAction::None),
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> DiffBufferAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = DiffBufferMode::Normal;
                DiffBufferAction::Cancel
            }
            KeyCode::Enter => {
                self.mode = DiffBufferMode::Normal;
                DiffBufferAction::SearchAccept
            }
            KeyCode::Backspace => {
                self.viewer.search.query.pop();
                DiffBufferAction::SearchChanged
            }
            KeyCode::Char(ch) if !ch.is_control() && key.modifiers.is_empty() => {
                self.viewer.search.query.push(ch);
                DiffBufferAction::SearchChanged
            }
            _ => DiffBufferAction::None,
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> DiffBufferAction {
        match key.code {
            KeyCode::Esc => {
                self.command_line.clear();
                self.mode = DiffBufferMode::Normal;
                DiffBufferAction::Cancel
            }
            KeyCode::Backspace => {
                self.command_line.pop();
                DiffBufferAction::None
            }
            KeyCode::Enter => self.execute_command_line(),
            KeyCode::Char(ch) if !ch.is_control() && key.modifiers.is_empty() => {
                self.command_line.push(ch);
                DiffBufferAction::None
            }
            _ => DiffBufferAction::None,
        }
    }

    fn handle_text_object_key(&mut self, key: KeyEvent, kind: TextObjectKind) -> DiffBufferAction {
        self.mode = DiffBufferMode::Visual;
        match key.code {
            KeyCode::Esc => DiffBufferAction::Cancel,
            KeyCode::Char(ch) => {
                DiffBufferAction::SelectTextObject(kind, shifted_text_object_char(ch))
            }
            _ => DiffBufferAction::None,
        }
    }

    fn execute_command_line(&mut self) -> DiffBufferAction {
        let command = self.command_line.trim().to_string();
        self.command_line.clear();
        self.mode = DiffBufferMode::Normal;
        match command.as_str() {
            "w" => DiffBufferAction::SaveComments,
            "q" => DiffBufferAction::Quit { force: false },
            "q!" => DiffBufferAction::Quit { force: true },
            "wq" | "x" => DiffBufferAction::SaveComments,
            _ => DiffBufferAction::None,
        }
    }

    fn accept(&mut self, action: DiffBufferAction) -> DiffBufferAction {
        self.pending.clear();
        action
    }
}

impl PendingChord {
    fn set(&mut self, keys: &str, now: Instant) {
        self.keys.clear();
        self.keys.push_str(keys);
        self.at = Some(now);
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.at = None;
    }

    fn clear_expired(&mut self, now: Instant) {
        if self
            .at
            .is_some_and(|at| !self.keys.is_empty() && now.duration_since(at) > PENDING_KEY_TIMEOUT)
        {
            self.clear();
        }
    }
}

fn shifted_text_object_char(ch: char) -> char {
    match ch {
        ')' => '(',
        '}' => '{',
        ']' => '[',
        '>' => '<',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn parses_vim_chords() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(key(KeyCode::Char('g')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('g')), now),
            DiffBufferAction::Top
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char(']')), now),
            DiffBufferAction::NextFile
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('[')), now),
            DiffBufferAction::PreviousFile
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char(' ')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('e')), now),
            DiffBufferAction::OpenFileFinder
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('g')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('e')), now),
            DiffBufferAction::WordEndBackward
        );
    }

    #[test]
    fn parses_word_and_reverse_search_keys() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(key(KeyCode::Char('w')), now),
            DiffBufferAction::WordForward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('W')), now),
            DiffBufferAction::BigWordForward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('e')), now),
            DiffBufferAction::WordEndForward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('E')), now),
            DiffBufferAction::BigWordEndForward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('b')), now),
            DiffBufferAction::WordBackward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('B')), now),
            DiffBufferAction::BigWordBackward
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('n')), now),
            DiffBufferAction::SearchNext
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('N')), now),
            DiffBufferAction::SearchPrevious
        );
    }

    #[test]
    fn pending_chords_expire() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(key(KeyCode::Char('g')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(
                key(KeyCode::Char('g')),
                now + PENDING_KEY_TIMEOUT + Duration::from_millis(1)
            ),
            DiffBufferAction::None
        );
    }

    #[test]
    fn parses_search_mode_without_touching_global_palette() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(key(KeyCode::Char('/')), now),
            DiffBufferAction::SearchChanged
        );
        assert_eq!(state.mode(), DiffBufferMode::Search);
        assert_eq!(
            state.handle_key(key(KeyCode::Char('f')), now),
            DiffBufferAction::SearchChanged
        );
        assert_eq!(state.search_query(), "f");
        assert_eq!(
            state.handle_key(key(KeyCode::Enter), now),
            DiffBufferAction::SearchAccept
        );
        assert_eq!(state.mode(), DiffBufferMode::Normal);
    }

    #[test]
    fn parses_command_mode() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(key(KeyCode::Char(':')), now),
            DiffBufferAction::None
        );
        assert_eq!(state.mode(), DiffBufferMode::Command);
        assert_eq!(
            state.handle_key(key(KeyCode::Char('q')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Char('!')), now),
            DiffBufferAction::None
        );
        assert_eq!(
            state.handle_key(key(KeyCode::Enter), now),
            DiffBufferAction::Quit { force: true }
        );
    }

    #[test]
    fn parses_control_paging() {
        let mut state = DiffBufferState::default();
        let now = Instant::now();

        assert_eq!(
            state.handle_key(ctrl('d'), now),
            DiffBufferAction::HalfPage(1)
        );
        assert_eq!(
            state.handle_key(ctrl('u'), now),
            DiffBufferAction::HalfPage(-1)
        );
        assert_eq!(
            state.handle_key(ctrl('p'), now),
            DiffBufferAction::OpenCommandPalette
        );
    }
}
