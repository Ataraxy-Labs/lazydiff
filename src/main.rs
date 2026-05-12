use std::{
    env, fs, io,
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
    let path = args.first().cloned().unwrap_or_else(|| fixture.to_string());
    let patch = if bench_scroll {
        fs::read_to_string(&path)?
    } else {
        String::new()
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
    let result = App::new("worktree".to_string(), patch.len(), document).run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
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
