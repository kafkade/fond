//! Full-screen TUI cook mode.
//!
//! Provides step-by-step recipe guidance with live countdown timers,
//! backward-scheduled timeline rail, and cook log recording.

mod app;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub use app::{CookApp, CookResult};

/// Guard that restores the terminal on drop (including panics).
struct TerminalGuard;

impl TerminalGuard {
    fn setup() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Run the interactive TUI cook mode.
///
/// Returns a `CookResult` with session data for optional cook-log persistence.
pub fn run_cook_mode(
    recipe: fond_domain::Recipe,
    schedule: Option<fond_timeline::ScheduledTimeline>,
) -> Result<CookResult> {
    // Install panic hook to restore terminal before printing panic info
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    let _guard = TerminalGuard;
    let mut terminal = TerminalGuard::setup()?;
    let mut app = CookApp::new(recipe, schedule);

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match app.quit_confirm {
                true => match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        break;
                    }
                    _ => app.quit_confirm = false,
                },
                false => match key.code {
                    KeyCode::Char('q') => app.quit_confirm = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('n') => {
                        app.next_step();
                    }
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('p') => {
                        app.prev_step();
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        app.toggle_timer();
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        let n = c.to_digit(10).unwrap() as usize;
                        if n >= 1 && n <= app.total_steps {
                            app.jump_to_step(n - 1);
                        }
                    }
                    _ => {}
                },
            }
        }

        // Tick: check for timer alerts
        app.tick();
    }

    Ok(app.result())
}
