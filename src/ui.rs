//! The front end's connection to the outside world: where input events come from and
//! where the screen is drawn.
//!
//! `app.rs` does all its input and output through the `Ui` trait, never through the
//! terminal directly. `TermUi` is the real terminal. Tests use a scripted `Ui` that
//! feeds key sequences and draws into an in-memory screen (see `tui_tests.rs`), and a
//! network client for the planned client/server split would be one more implementation.
use crate::disp;
use crate::game::Game;
use crate::models::Loc;
use crate::view::View;
use crossterm::{
    cursor,
    event::{self, Event},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// Half a blink cycle for the unit or city being asked about.
const BLINK_MS: u128 = 300;

pub trait Ui {
    /// Waits up to `timeout` for an input event; true if one is ready.
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    /// The next input event (waits for one).
    fn read(&mut self) -> io::Result<Event>;
    /// Throws away events typed ahead, so they don't act on the next screen.
    fn drain(&mut self);
    /// Draws the game screen with `prompt` on the bottom line, and the square in `blink`
    /// highlighted and shown or hidden (the bool).
    fn draw(&mut self, game: &Game, view: &mut View, prompt: &str, blink: Option<(Loc, bool)>) -> io::Result<()>;
    /// Draws a startup question: a title banner, the question, and what has been typed.
    fn banner(&mut self, title: &str, question: &str, input: &str) -> io::Result<()>;
    /// Pauses so that something (automove) can be watched.
    fn pause(&mut self, duration: Duration);
    /// Whether a square that has been blinking since `since` is showing right now.
    fn blink_on(&self, since: Instant) -> bool {
        (since.elapsed().as_millis() / BLINK_MS) & 1 == 0
    }
}

/// The real terminal (crossterm input, ratatui output).
pub struct TermUi {
    /// True while a startup banner is on screen: the next game draw clears it first.
    in_banner: bool,
}

impl TermUi {
    pub fn new() -> Self {
        TermUi { in_banner: false }
    }
}

impl Ui for TermUi {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }

    fn drain(&mut self) {
        while event::poll(Duration::from_millis(0)).unwrap_or(false) {
            let _ = event::read();
        }
    }

    fn draw(&mut self, game: &Game, view: &mut View, prompt: &str, blink: Option<(Loc, bool)>) -> io::Result<()> {
        if self.in_banner {
            io::stdout().execute(terminal::Clear(ClearType::All))?;
            self.in_banner = false;
        }
        disp::refresh_display_with_input_blinking(game, view, prompt, blink);
        Ok(())
    }

    fn banner(&mut self, title: &str, question: &str, input: &str) -> io::Result<()> {
        self.in_banner = true;
        let mut stdout = io::stdout();
        stdout.execute(terminal::Clear(ClearType::All))?;
        stdout.execute(cursor::MoveTo(10, 5))?;
        print!("==================================================");
        stdout.execute(cursor::MoveTo(10, 6))?;
        print!(" {:<62}", title);
        stdout.execute(cursor::MoveTo(10, 7))?;
        print!("==================================================");
        stdout.execute(cursor::MoveTo(10, 9))?;
        print!("{}{}", question, input);
        stdout.execute(cursor::Show)?;
        stdout.flush()
    }

    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
