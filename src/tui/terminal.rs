//! Terminal setup, teardown, and RAII guard for safe restoration.
//!
//! [`TerminalGuard`] ensures the terminal is restored to its original state
//! on both normal exit and panic. A panic hook is registered as a secondary
//! safety net.

use std::io::{self, Stdout};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Type alias for the terminal backend used throughout the TUI.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// RAII guard that restores the terminal on drop.
///
/// The primary mechanism for terminal restoration. A panic hook is also
/// registered as a belt-and-suspenders backup (see [`install_panic_hook`]).
#[derive(Debug)]
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

/// Enter the alternate screen, enable raw mode, and create the terminal.
///
/// Returns the terminal instance and a [`TerminalGuard`] whose `Drop`
/// implementation will restore the terminal. The caller must keep the guard
/// alive for the duration of the TUI session.
pub fn setup_terminal() -> io::Result<(Tui, TerminalGuard)> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok((terminal, TerminalGuard))
}

/// Restore the terminal to its original state.
///
/// Called by [`TerminalGuard::drop`] and the panic hook. Errors are
/// intentionally swallowed — there's nothing useful to do if restoration
/// fails during teardown.
fn restore_terminal() -> io::Result<()> {
    terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}

/// Register a panic hook that restores the terminal before printing the
/// panic message. This is a secondary safety net — [`TerminalGuard`] is
/// the primary mechanism.
pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
}
