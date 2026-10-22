use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io;

/// Puts the terminal into raw/alternate-screen/mouse mode and puts it back when
/// dropped, so any early return, `?` error or panic leaves the shell usable.
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        let mut stdout = io::stdout();
        stdout.execute(terminal::EnterAlternateScreen)?;
        terminal::enable_raw_mode()?;
        stdout.execute(EnableMouseCapture)?;
        stdout.execute(cursor::Hide)?;
        stdout.execute(terminal::Clear(ClearType::All))?;
        Ok(TerminalGuard)
    }

    /// Safe to call more than once (also used by the panic hook).
    pub fn restore() {
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableMouseCapture);
        let _ = stdout.execute(cursor::Show);
        let _ = terminal::disable_raw_mode();
        let _ = stdout.execute(terminal::LeaveAlternateScreen);
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        Self::restore();
    }
}
