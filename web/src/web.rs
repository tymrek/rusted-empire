//! The browser front end: a `Ui` for the game loop running in a Web Worker.
//!
//! The page (`www/main.js`) owns the screen, the keyboard and the mouse. It puts input
//! events into a ring buffer in shared memory and wakes the worker; the worker blocks on
//! that memory (`Atomics.wait`) exactly where the terminal version blocks on the
//! keyboard, so `app.rs` runs unchanged. Each frame is drawn with the game's own
//! renderer (`disp::render`) into an in-memory ratatui buffer and posted to the page,
//! which paints it on a canvas. Saves live in the browser's local storage.
//!
//! The JavaScript side of these calls is `www/bridge.js`.
use crate::app::{self, Options};
use crate::disp;
use crate::game::Game;
use crate::models::Loc;
use crate::ui::Ui;
use crate::view::View;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    backend::TestBackend,
    buffer::Cell,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Clear, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/www/bridge.js")]
extern "C" {
    /// Blocks until an input event is waiting or `timeout_ms` passes; true if one is.
    fn wait_input(timeout_ms: f64) -> bool;
    /// Takes the next input event into `out` (six numbers, see `decode`); false if none.
    fn pop_input(out: &mut [i32]) -> bool;
    /// The screen size in character cells, as `columns << 16 | rows`.
    fn screen_size() -> u32;
    /// Shows a frame: one entry per cell, row by row.
    fn present(cols: u32, rows: u32, chars: &[u32], fg: &[u32], bg: &[u32], mods: &[u32]);
    /// Blocks for `ms` milliseconds.
    fn sleep_ms(ms: f64);
    /// Milliseconds since the page started.
    fn now_ms() -> f64;
    /// Stores a save in local storage.
    fn save_put(name: &str, data: &str) -> bool;
    /// Reads a save from local storage.
    fn save_get(name: &str) -> Option<String>;
    /// The game has ended (or crashed): `message` says why.
    fn finished(message: &str);
}

/// A point in time, from the browser's clock (the standard library's `Instant` doesn't
/// work in the browser).
#[derive(Clone, Copy, Debug)]
pub struct Instant(f64);

impl Instant {
    pub fn now() -> Instant {
        Instant(now_ms())
    }
    pub fn elapsed(&self) -> Duration {
        Duration::from_secs_f64(((now_ms() - self.0) / 1000.0).max(0.0))
    }
}

/// Runs the game in the worker until the player quits or the game ends. `seed` is an
/// optional number (empty for a random world); `torus` makes the map wrap around.
#[wasm_bindgen]
pub fn run(seed: String, torus: bool) {
    std::panic::set_hook(Box::new(|info| {
        finished(&format!("The game crashed: {}", info));
    }));
    let opts = Options { load_file: None, seed: seed.trim().parse().ok(), torus };
    let ui = WebUi::new();
    let ending = match app::start_session(opts, Box::new(ui)) {
        Ok(Some((mut session, autosave))) => match app::play(&mut session, autosave) {
            Ok(()) => "Game over. Reload the page to play again.".to_string(),
            Err(e) => format!("The game stopped: {}", e),
        },
        Ok(None) => "Game cancelled. Reload the page to start again.".to_string(),
        Err(e) => format!("The game could not start: {}", e),
    };
    finished(&ending);
}

/// The browser `Ui`: input from the page, frames to the page, saves in local storage.
pub struct WebUi {
    term: Terminal<TestBackend>,
    /// The last frame sent to the page, to send only frames that changed.
    last: Vec<u32>,
}

impl WebUi {
    pub fn new() -> Self {
        let (cols, rows) = Self::size();
        WebUi { term: Terminal::new(TestBackend::new(cols, rows)).expect("in-memory terminal"), last: Vec::new() }
    }

    fn size() -> (u16, u16) {
        let s = screen_size();
        (((s >> 16) as u16).max(20), ((s & 0xffff) as u16).max(10))
    }

    /// Matches the in-memory screen to the page's size.
    fn fit(&mut self) -> io::Result<()> {
        let (cols, rows) = Self::size();
        let area = self.term.backend().buffer().area;
        if area.width != cols || area.height != rows {
            self.term.backend_mut().resize(cols, rows);
            self.term.resize(Rect::new(0, 0, cols, rows))?;
            self.last.clear();
        }
        Ok(())
    }

    /// Sends the in-memory screen to the page, if it changed since last time.
    fn present(&mut self) {
        let buf = self.term.backend().buffer();
        let n = buf.content().len();
        let (mut chars, mut fg, mut bg, mut mods) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for cell in buf.content() {
            chars.push(cell_char(cell));
            fg.push(color_code(cell.fg));
            bg.push(color_code(cell.bg));
            mods.push(cell.modifier.bits() as u32);
        }
        let mut key = chars.clone();
        key.extend(&fg);
        key.extend(&bg);
        key.extend(&mods);
        if key == self.last {
            return;
        }
        self.last = key;
        present(buf.area.width as u32, buf.area.height as u32, &chars, &fg, &bg, &mods);
    }
}

fn cell_char(cell: &Cell) -> u32 {
    cell.symbol().chars().next().map_or(' ' as u32, |c| c as u32)
}

/// A ratatui colour for the page: 0 is the default, 1-16 the sixteen named colours
/// (in ratatui's order), then indexed and RGB colours with a tag in the top byte.
fn color_code(c: Color) -> u32 {
    match c {
        Color::Reset => 0,
        Color::Black => 1,
        Color::Red => 2,
        Color::Green => 3,
        Color::Yellow => 4,
        Color::Blue => 5,
        Color::Magenta => 6,
        Color::Cyan => 7,
        Color::Gray => 8,
        Color::DarkGray => 9,
        Color::LightRed => 10,
        Color::LightGreen => 11,
        Color::LightYellow => 12,
        Color::LightBlue => 13,
        Color::LightMagenta => 14,
        Color::LightCyan => 15,
        Color::White => 16,
        Color::Indexed(i) => 0x0100_0000 | i as u32,
        Color::Rgb(r, g, b) => 0x0200_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

/// An input event from the six numbers the page sends:
/// key: `[1, key, char, modifiers, 0, 0]`; mouse: `[2, kind, button, column, row, modifiers]`.
fn decode(e: &[i32; 6]) -> Option<Event> {
    match e[0] {
        1 => {
            let code = match e[1] {
                0 => KeyCode::Char(char::from_u32(e[2] as u32)?),
                1 => KeyCode::Enter,
                2 => KeyCode::Esc,
                3 => KeyCode::Backspace,
                4 => KeyCode::Up,
                5 => KeyCode::Down,
                6 => KeyCode::Left,
                7 => KeyCode::Right,
                8 => KeyCode::Tab,
                9 => KeyCode::Delete,
                10 => KeyCode::Home,
                11 => KeyCode::End,
                12 => KeyCode::PageUp,
                13 => KeyCode::PageDown,
                _ => return None,
            };
            let modifiers = KeyModifiers::from_bits_truncate(e[3] as u8);
            Some(Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }))
        }
        2 => {
            let button = match e[2] {
                1 => MouseButton::Right,
                2 => MouseButton::Middle,
                _ => MouseButton::Left,
            };
            let kind = match e[1] {
                0 => MouseEventKind::Down(button),
                1 => MouseEventKind::Up(button),
                2 => MouseEventKind::Drag(button),
                3 => MouseEventKind::Moved,
                4 => MouseEventKind::ScrollDown,
                5 => MouseEventKind::ScrollUp,
                _ => return None,
            };
            let modifiers = KeyModifiers::from_bits_truncate(e[5] as u8);
            Some(Event::Mouse(MouseEvent { kind, column: e[3].max(0) as u16, row: e[4].max(0) as u16, modifiers }))
        }
        _ => None,
    }
}

impl Ui for WebUi {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        Ok(wait_input(timeout.as_secs_f64() * 1000.0))
    }

    fn read(&mut self) -> io::Result<Event> {
        let mut raw = [0i32; 6];
        loop {
            if pop_input(&mut raw) {
                if let Some(ev) = decode(&raw) {
                    return Ok(ev);
                }
            } else {
                wait_input(60_000.0);
            }
        }
    }

    fn drain(&mut self) {
        let mut raw = [0i32; 6];
        while pop_input(&mut raw) {}
    }

    fn draw(&mut self, game: &Game, view: &mut View, prompt: &str, blink: Option<(Loc, bool)>) -> io::Result<()> {
        view.follow_messages(game);
        self.fit()?;
        let view = &*view;
        self.term.draw(|f| disp::render(f, game, view, prompt, blink))?;
        self.present();
        Ok(())
    }

    fn banner(&mut self, title: &str, question: &str, input: &str) -> io::Result<()> {
        self.fit()?;
        let rule = "=".repeat(50);
        let pad = " ".repeat(10);
        let mut lines: Vec<Line> = vec![Line::from(""); 5];
        lines.push(Line::from(format!("{}{}", pad, rule)));
        lines.push(Line::from(format!("{} {}", pad, title)));
        lines.push(Line::from(format!("{}{}", pad, rule)));
        lines.push(Line::from(""));
        lines.push(Line::from(format!("{}{}{}█", pad, question, input)));
        self.term.draw(|f| {
            let area = f.size();
            f.render_widget(Clear, area);
            f.render_widget(Paragraph::new(lines).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)), area);
        })?;
        self.present();
        Ok(())
    }

    fn pause(&mut self, duration: Duration) {
        sleep_ms(duration.as_secs_f64() * 1000.0);
    }

    fn blink_on(&self, since: Instant) -> bool {
        (since.elapsed().as_millis() / 300) & 1 == 0
    }

    fn write_save(&mut self, name: &str, game: &Game) -> io::Result<()> {
        let data = game.to_save_string()?;
        if save_put(name, &data) {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "the browser refused to store the save (storage full or disabled)"))
        }
    }

    fn read_save(&mut self, name: &str) -> Result<Game, String> {
        match save_get(name) {
            Some(data) => Game::from_save_string(&data, name),
            None => Err(format!("there is no saved game called {} in this browser", name)),
        }
    }
}
