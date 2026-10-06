use std::io::{self, Stdout};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Takes over the real terminal on construction (raw mode, alternate
/// screen, mouse capture so the scroll wheel gets delivered as events at
/// all) and restores it on drop — including on panic, since `Drop` still
/// runs while unwinding. Without this, a crash would leave the user's shell
/// in raw mode with the alternate screen still active.
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

pub fn init() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    Terminal::new(CrosstermBackend::new(io::stdout()))
}
