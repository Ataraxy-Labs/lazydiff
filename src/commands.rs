use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::design_system::ThemeVariant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layer {
    FilePicker,
    CommitList,
    Comments,
    Diff,
    DetailFull,
    Queue,
    Global,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Quit,
    Back,
    MoveDown,
    MoveUp,
    PageDown,
    PageUp,
    Refresh,
    _LoginForge,
    PullBranch,
    PushBranch,
    FetchBranch,
    ForcePushBranch,
    OpenCommitList,
    OpenSelectedCommit,
    OpenDetail,
    OpenComments,
    OpenDiff,
    OpenInBrowser,
    OpenInEditor,
    OpenCommandPalette,
    OpenFileSearch,
    OpenTextSearch,
    OpenInbox,
    OpenThread,
    NewQuestion,
    NewInstruction,
    NewNote,
    ToggleDiffMode,
    ToggleFileTree,
    JumpFirst,
    JumpLast,
    PreviousFile,
    NextFile,
    PreviousHunk,
    NextHunk,
    ShowAttempts,
    SelectLeft,
    SelectRight,
    ScrollLeft,
    ScrollRight,
    OpenThemePicker,
    SelectTheme(ThemeVariant),
}

pub(crate) fn command_for_layer(layer: Layer, key: KeyEvent) -> Option<Command> {
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('p') {
        return Some(Command::OpenCommandPalette);
    }
    // Reserve ctrl-u / ctrl-d for half-page scroll inside scrollable surfaces.
    // Fall through here so the per-surface scroll handler in `App::handle_key`
    // can claim them without competing with `d`/`u` layer bindings.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('u') | KeyCode::Char('d'))
    {
        return None;
    }
    // Global theme picker: capital T opens the Lumen-compatible theme list.
    if key.code == KeyCode::Char('T') {
        return Some(Command::OpenThemePicker);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('h') {
        return Some(Command::ScrollLeft);
    }
    if (key.modifiers.is_empty() || key.modifiers.contains(KeyModifiers::CONTROL))
        && key.code == KeyCode::Backspace
    {
        return Some(Command::ScrollLeft);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        return Some(Command::ScrollRight);
    }
    // Global quit: `q` quits from any non-modal surface. Modal layers
    // (composer, file picker, attempt modal) intercept keys earlier in
    // `App::handle_key` so this can't close them by accident.
    if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
        return Some(Command::Quit);
    }
    match layer {
        Layer::FilePicker => None,
        Layer::CommitList if is_tab_key(key) => Some(Command::OpenSelectedCommit),
        Layer::CommitList => match key.code {
            KeyCode::Esc => Some(Command::Back),
            KeyCode::Enter | KeyCode::Char('d') => Some(Command::OpenSelectedCommit),
            KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
            KeyCode::Char('g') => Some(Command::JumpFirst),
            KeyCode::Char('G') => Some(Command::JumpLast),
            _ => None,
        },
        Layer::Queue if is_tab_key(key) => Some(Command::OpenCommitList),
        Layer::Queue => match key.code {
            KeyCode::Esc => Some(Command::Quit),
            KeyCode::Enter => Some(Command::OpenDetail),
            KeyCode::Char('C') => Some(Command::OpenCommitList),
            KeyCode::Char('d') => Some(Command::OpenDiff),
            KeyCode::Char('o') => Some(Command::OpenInBrowser),
            KeyCode::Char('c') => Some(Command::OpenComments),
            KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
            KeyCode::Char('g') => Some(Command::JumpFirst),
            KeyCode::Char('G') => Some(Command::JumpLast),
            KeyCode::Char('l') => Some(Command::_LoginForge),
            KeyCode::Char('r') => Some(Command::Refresh),
            KeyCode::Char('p') => Some(Command::PullBranch),
            KeyCode::Char('P') => Some(Command::PushBranch),
            KeyCode::Char('f') => Some(Command::FetchBranch),
            KeyCode::Char('F') => Some(Command::ForcePushBranch),
            KeyCode::Char('/') => Some(Command::OpenCommandPalette),
            KeyCode::Char(':') => Some(Command::OpenInbox),
            _ => None,
        },
        Layer::DetailFull => match key.code {
            KeyCode::Esc => Some(Command::Back),
            KeyCode::Char('d') => Some(Command::OpenDiff),
            KeyCode::Char('o') => Some(Command::OpenInBrowser),
            KeyCode::Char('c') => Some(Command::OpenComments),
            KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
            KeyCode::Char('g') => Some(Command::JumpFirst),
            KeyCode::Char('G') => Some(Command::JumpLast),
            KeyCode::PageDown => Some(Command::PageDown),
            KeyCode::PageUp => Some(Command::PageUp),
            _ => None,
        },
        Layer::Comments => match key.code {
            KeyCode::Esc => Some(Command::Back),
            KeyCode::Char('d') => Some(Command::OpenDiff),
            KeyCode::Char('o') => Some(Command::OpenInBrowser),
            KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
            KeyCode::Char('g') => Some(Command::JumpFirst),
            KeyCode::Char('G') => Some(Command::JumpLast),
            KeyCode::PageDown => Some(Command::PageDown),
            KeyCode::PageUp => Some(Command::PageUp),
            _ => None,
        },
        Layer::Diff => match key.code {
            KeyCode::Esc => Some(Command::Back),
            // `q` is reserved for global quit (above). Diff "ask question"
            // lives on `a` (Ask). `i` instruct + `n` note keep their spots.
            KeyCode::Char('a') => Some(Command::NewQuestion),
            KeyCode::Char('i') => Some(Command::NewInstruction),
            KeyCode::Char('n') | KeyCode::Char('c') => Some(Command::NewNote),
            KeyCode::Char('e') => Some(Command::OpenInEditor),
            KeyCode::Enter => Some(Command::OpenThread),
            KeyCode::Char('f') => Some(Command::OpenFileSearch),
            KeyCode::Char(':') => Some(Command::OpenInbox),
            KeyCode::Char('/') => Some(Command::OpenTextSearch),
            KeyCode::Char('j') | KeyCode::Down => Some(Command::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Command::MoveUp),
            KeyCode::PageDown => Some(Command::PageDown),
            KeyCode::PageUp => Some(Command::PageUp),
            KeyCode::Char('g') => Some(Command::JumpFirst),
            KeyCode::Char('G') => Some(Command::JumpLast),
            KeyCode::Char('m') => Some(Command::ToggleDiffMode),
            KeyCode::Char('t') => Some(Command::ToggleFileTree),
            KeyCode::Char(']') => Some(Command::NextFile),
            KeyCode::Char('[') => Some(Command::PreviousFile),
            KeyCode::Char('N') => Some(Command::NextHunk),
            KeyCode::Char('p') => Some(Command::PreviousHunk),
            KeyCode::Char('A') => Some(Command::ShowAttempts),
            KeyCode::Char('H') => Some(Command::ScrollLeft),
            KeyCode::Char('L') => Some(Command::ScrollRight),
            KeyCode::Left => Some(Command::SelectLeft),
            KeyCode::Right => Some(Command::SelectRight),
            _ => None,
        },
        Layer::Global => None,
    }
}

fn is_tab_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Tab | KeyCode::Char('\t'))
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('i'))
}
