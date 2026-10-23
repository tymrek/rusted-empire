//! The interactive game: startup screens, then the main input/turn loop.
//!
//! This is the terminal front end. It turns keys into calls on `Game` (moves and the
//! order commands such as `order_unit` and `set_standing_order`) and keeps everything
//! about the screen itself in a `View`. The game never reads the view.
use crate::disp;
use crate::game::{Game, OrderScope, MAP_COLS, MAP_ROWS};
use crate::keys::{self, OrderKey, PIECE_KEYS};
use crate::models::{Loc, MoveResult, Owner, Piece, PieceType, UnitActivity};
use crate::terminal::TerminalGuard;
use crate::view::{self, GameMode, View};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// How long to wait for a key before redrawing (for blinking and animation).
const POLL: Duration = Duration::from_millis(50);
/// How long the turn prompt waits for a key before redrawing.
const IDLE_POLL: Duration = Duration::from_millis(100);
/// Pause after each turn in automove mode, so it can be watched.
const AUTO_TURN_PAUSE: Duration = Duration::from_millis(150);
/// Half a blink cycle for the unit or city being asked about.
const BLINK_MS: u128 = 300;

/// What the command line asked for.
pub struct Options {
    /// `--load FILE`: resume a saved game instead of starting a new one.
    pub load_file: Option<String>,
    /// `--seed N`: make a new game reproducible.
    pub seed: Option<u64>,
    /// `-t` / `--torus`: the playable map wraps around at its edges. Only applies to a new
    /// game; a loaded game keeps the topology it was saved with, as in the original.
    pub torus: bool,
}

/// The game being played and this player's view of it.
struct Session {
    game: Game,
    view: View,
}

/// True for Ctrl-C.
fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

/// Throws away keys typed ahead, so they don't act on the next screen.
fn drain_input() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

/// Waits for the next key press.
fn read_key() -> KeyEvent {
    loop {
        if let Ok(Event::Key(k)) = event::read() {
            return k;
        }
    }
}

/// Whether the blinking square is showing at the moment.
fn blink_on(since: Instant) -> bool {
    (since.elapsed().as_millis() / BLINK_MS) & 1 == 0
}

impl Session {
    fn draw(&mut self, prompt: &str) {
        disp::refresh_display_with_input(&self.game, &mut self.view, prompt);
    }

    fn draw_blinking(&mut self, prompt: &str, blink: (Loc, bool)) {
        disp::refresh_display_with_input_blinking(&self.game, &mut self.view, prompt, Some(blink));
    }

    /// Shows `prompt` and waits for Y or N (other keys are ignored).
    fn ask_yes_no(&mut self, prompt: &str) -> bool {
        self.draw(prompt);
        loop {
            if let KeyCode::Char(c) = read_key().code {
                match c.to_ascii_lowercase() {
                    'y' => return true,
                    'n' => return false,
                    _ => {}
                }
            }
        }
    }

    /// Asks to confirm quitting; logs it if the player changes their mind.
    fn confirm_quit(&mut self) -> bool {
        let quit = self.ask_yes_no("Really quit game? [Y/N]: ");
        if !quit {
            self.game.log(String::from("Quit cancelled."));
        }
        quit
    }

    /// Shows `prompt` and waits for a key that `choose` accepts. Esc gives `None`.
    fn pick<T>(&mut self, prompt: &str, choose: impl Fn(char) -> Option<T>) -> Option<T> {
        self.draw(prompt);
        loop {
            let key = read_key();
            if key.code == KeyCode::Esc {
                return None;
            }
            if let KeyCode::Char(c) = key.code {
                if let Some(t) = choose(c) {
                    return Some(t);
                }
            }
        }
    }

    /// Asks for a direction (arrows or keypad). Esc gives `None`.
    fn pick_direction(&mut self, ptype: PieceType) -> Option<(i32, i32)> {
        self.draw(&format!("Set march direction for {:?} [Arrows/NumPad] or [Esc] Cancel: ", ptype));
        loop {
            let key = read_key();
            if key.code == KeyCode::Esc {
                return None;
            }
            if let Some(dir) = keys::dir_for_key(key.code) {
                return Some(dir);
            }
        }
    }

    /// Lets the player move a cursor from `start` and pick a square with Enter (Esc gives
    /// `None`). The view is in `mode` meanwhile, and goes back as it was afterwards.
    fn pick_location(&mut self, prompt: &str, start: Loc, mode: GameMode) -> Option<Loc> {
        let (old_mode, old_cursor) = (self.view.mode, self.view.cursor);
        self.view.mode = mode;
        self.view.cursor = start;
        let picked = loop {
            self.draw(prompt);
            if !event::poll(POLL).unwrap_or(false) {
                continue;
            }
            let Ok(Event::Key(key)) = event::read() else { continue };
            match key.code {
                KeyCode::Esc => break None,
                KeyCode::Enter => break Some(self.view.cursor),
                code => {
                    if let Some((dr, dc)) = keys::dir_for_key(code) {
                        self.view.cursor = self.game.shift_cursor(self.view.cursor, dr, dc);
                    }
                }
            }
        };
        self.view.mode = old_mode;
        self.view.cursor = old_cursor;
        picked
    }

    /// Asks which order to give a unit of type `ptype` (`title` names what is being
    /// ordered), including a direction or destination if the order needs one. Esc gives
    /// `None`; backing out of a direction or destination goes back to the order menu.
    fn pick_order(&mut self, ptype: PieceType, title: &str, dest_start: Loc, dest_mode: GameMode) -> Option<UnitActivity> {
        let menu = format!("{} - {}: ", title, keys::order_menu(ptype).join(","));
        loop {
            match self.pick(&menu, |c| keys::order_for_key(c, ptype))? {
                OrderKey::Order(act) => return Some(act),
                OrderKey::PickDirection => {
                    if let Some((dr, dc)) = self.pick_direction(ptype) {
                        return Some(UnitActivity::Dir(dr, dc));
                    }
                }
                OrderKey::PickDestination => {
                    let prompt = format!("Pick destination for {:?} [Arrows/NumPad] Move | [Enter] Confirm | [Esc] Cancel: ", ptype);
                    if let Some(dest) = self.pick_location(&prompt, dest_start, dest_mode) {
                        return Some(UnitActivity::Dest(dest));
                    }
                }
            }
        }
    }

    /// Mouse wheel scrolls the message log; dragging with the left button pans the map
    /// (from `fallback_center` if it hasn't been panned yet).
    fn handle_mouse(&mut self, mouse: MouseEvent, fallback_center: Loc) {
        let v = &mut self.view;
        match mouse.kind {
            MouseEventKind::ScrollUp => v.msg_scroll = v.msg_scroll.saturating_add(1),
            MouseEventKind::ScrollDown => v.msg_scroll = v.msg_scroll.saturating_sub(1),
            MouseEventKind::Down(MouseButton::Left) => v.drag_from = Some((mouse.column, mouse.row)),
            MouseEventKind::Up(MouseButton::Left) => v.drag_from = None,
            MouseEventKind::Drag(MouseButton::Left) => {
                let current = v.center.unwrap_or(fallback_center);
                if let Some((last_c, last_r)) = v.drag_from {
                    let dc = mouse.column as i32 - last_c as i32;
                    let dr = mouse.row as i32 - last_r as i32;
                    v.center = Some(self.game.shift_cursor(current, -dr, -dc));
                }
                v.drag_from = Some((mouse.column, mouse.row));
            }
            _ => {}
        }
    }

    /// Asks what city `city_idx` should build. `new_city` is true for a city just won (or
    /// the starting city): it starts from scratch, so it pays the retooling cost whatever
    /// is chosen.
    fn prompt_city_production(&mut self, city_idx: usize, new_city: bool) {
        self.view.center = None;
        let city_loc = self.game.cities[city_idx].loc;
        let prompt = format!(
            "New City at ({},{}) Build? [A]rmy, [T]rns, [P]tBoat, [D]est, [S]ub, [C]arr, [B]ttle, [F]ght, [Z]Sat: ",
            city_loc.r, city_loc.c
        );
        let since = Instant::now();
        loop {
            self.draw_blinking(&prompt, (city_loc, blink_on(since)));
            if !event::poll(POLL).unwrap_or(false) {
                continue;
            }
            let Ok(Event::Key(key)) = event::read() else { continue };
            if key.code == KeyCode::Esc {
                if new_city {
                    let prod = self.game.cities[city_idx].prod;
                    self.game.set_production(city_idx, prod, true);
                }
                self.game.log(format!("Production unchanged for City at ({},{}).", city_loc.r, city_loc.c));
                return;
            }
            let KeyCode::Char(c) = key.code else { continue };
            match keys::piece_for_key(c) {
                Some(prod) if prod.is_naval() && !self.game.is_coastal(city_loc) => {
                    self.game.log(format!("City at ({},{}) is landlocked! Cannot build {:?}. Try again.", city_loc.r, city_loc.c, prod));
                }
                Some(prod) => {
                    self.game.set_production(city_idx, prod, new_city);
                    self.game.log(format!("City at ({},{}) assigned to build: {:?}", city_loc.r, city_loc.c, prod));
                    return;
                }
                None => self.game.log(format!("Invalid choice '{}'. Press a valid unit key.", c)),
            }
        }
    }
}

/// Runs the interactive game until the player quits.
pub fn run(opts: Options) -> io::Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        TerminalGuard::restore();
        original_hook(panic_info);
    }));

    // Restores the terminal on every exit path from here on, including `?` errors.
    let _terminal_guard = TerminalGuard::enter()?;

    match start_session(opts)? {
        Some((session, autosave_interval)) => play(session, autosave_interval),
        None => Ok(()),
    }
}

/// Shows a banner and reads a number. `None` means the player pressed Ctrl-C.
fn prompt_number(title: &str, question: &str) -> io::Result<Option<String>> {
    let mut stdout = io::stdout();
    stdout.execute(terminal::Clear(ClearType::All))?;
    stdout.execute(cursor::MoveTo(10, 5))?;
    print!("==================================================");
    stdout.execute(cursor::MoveTo(10, 6))?;
    print!(" {:<62}", title);
    stdout.execute(cursor::MoveTo(10, 7))?;
    print!("==================================================");
    stdout.execute(cursor::MoveTo(10, 9))?;
    print!("{}", question);
    stdout.flush()?;

    let mut input = String::new();
    stdout.execute(cursor::Show)?;
    loop {
        if let Event::Key(key) = event::read()? {
            if is_ctrl_c(&key) {
                return Ok(None);
            }
            match key.code {
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    input.push(c);
                    print!("{}", c);
                    stdout.flush()?;
                }
                KeyCode::Enter => break,
                KeyCode::Backspace if !input.is_empty() => {
                    input.pop();
                    print!("\x08 \x08");
                    stdout.flush()?;
                }
                _ => {}
            }
        }
    }
    stdout.execute(cursor::Hide)?;
    Ok(Some(input))
}

/// Loads or creates the game and asks the startup questions.
/// `None` means the player backed out with Ctrl-C (or the save could not be loaded).
fn start_session(opts: Options) -> io::Result<Option<(Session, i32)>> {
    let mut preloaded_game = None;
    if let Some(filename) = opts.load_file {
        match Game::load_game(&filename) {
            Ok(loaded_game) => preloaded_game = Some(loaded_game),
            Err(reason) => {
                TerminalGuard::restore(); // leave the alternate screen so the message stays visible
                eprintln!("Failed to load save file: {}", reason);
                return Ok(None);
            }
        }
    }

    // A new world is made first: how many difficulty levels there are depends on its
    // continents, as in the original.
    let mut new_game = None;
    if preloaded_game.is_none() {
        let mut g = match opts.seed {
            Some(seed) => Game::with_seed(MAP_ROWS, MAP_COLS, 0, seed),
            None => Game::new(MAP_ROWS, MAP_COLS, 0),
        };
        g.torus = opts.torus;
        let levels = g.generate_world();
        let question = format!("Choose a difficulty level where 0 is easy and {} is hard: ", levels - 1);
        let mut title = String::from("Welcome to rs empire (Rust Port of VMS-empire)");
        let level = loop {
            match prompt_number(&title, &question)? {
                Some(s) => match s.trim().parse::<usize>() {
                    Ok(n) if n < levels => break n,
                    _ => title = format!("Please enter an integer in the range 0..{}.", levels - 1),
                },
                None => return Ok(None),
            }
        };
        g.choose_start(level);
        new_game = Some(g);
    }

    let autosave_interval: i32 = match prompt_number("Autosave Configuration", "Enter autosave interval in turns (0 = Disabled): ")? {
        Some(s) => s.trim().parse().unwrap_or(0),
        None => return Ok(None),
    };

    io::stdout().execute(terminal::Clear(ClearType::All))?;

    let session = if let Some(mut game) = preloaded_game {
        game.log(String::from("Game loaded from command line."));
        Session { game, view: View::default() }
    } else {
        let game = new_game.expect("a new game was made above");
        let mut session = Session { game, view: View::default() };
        let starting_cities: Vec<usize> = session.game.cities.iter().enumerate()
            .filter(|(_, ct)| ct.owner == Owner::Player)
            .map(|(i, _)| i)
            .collect();
        for idx in starting_cities {
            session.prompt_city_production(idx, true);
        }
        session
    };
    Ok(Some((session, autosave_interval)))
}

/// One iteration of the map editor: draw, then handle at most one mouse/key event.
/// Returns `Ok(true)` if the player chose to quit the whole program.
fn edit_mode_step(s: &mut Session) -> io::Result<bool> {
    s.draw("[Arrows/NumPad] Move | [P]rod | [O]rders | [W]ake | e[X]ject | [I]nitial Orders | [E]xit Edit: ");
    if !event::poll(POLL).unwrap_or(false) {
        return Ok(false);
    }
    let Ok(ev) = event::read() else { return Ok(false) };
    if let Event::Mouse(mouse) = ev {
        let fallback = s.view.cursor;
        s.handle_mouse(mouse, fallback);
    }
    let Event::Key(key) = ev else { return Ok(false) };
    s.view.center = None;
    if is_ctrl_c(&key) {
        return Ok(s.confirm_quit());
    }
    let cursor = s.view.cursor;
    match key.code {
        KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Esc => {
            s.view.mode = GameMode::Manual;
            s.game.log(String::from("Exited Edit Mode."));
            drain_input();
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            match s.game.cities.iter().position(|c| c.loc == cursor && c.owner == Owner::Player) {
                Some(c_idx) => s.prompt_city_production(c_idx, false),
                None => s.game.log(String::from("No player-owned city at cursor location.")),
            }
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            // Some(None) = every type ('*').
            let prompt = format!("Wake units at cursor: {} or [*]All: ", PIECE_KEYS);
            match s.pick(&prompt, |c| if c == '*' { Some(None) } else { keys::piece_for_key(c).map(Some) }) {
                Some(only) => {
                    let woke = s.game.wake_units_at(cursor, only);
                    if woke > 0 {
                        s.game.log(format!("Awakened {} unit(s) at cursor.", woke));
                    } else {
                        s.game.log(String::from("No resting player units of that type at cursor."));
                    }
                }
                None => s.game.log(String::from("Wake cancelled.")),
            }
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            if !s.game.launch_cargo_at(cursor) {
                s.game.log(String::from("No transports/carriers with cargo at cursor."));
            }
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            let unit = s.game.pieces.iter()
                .position(|p| p.loc == cursor && p.owner == Owner::Player && p.hits > 0 && p.ptype != PieceType::Satellite);
            match unit {
                Some(p_idx) => {
                    let ptype = s.game.pieces[p_idx].ptype;
                    match s.pick_order(ptype, &format!("Order {:?}", ptype), cursor, GameMode::Edit) {
                        Some(act) => {
                            s.game.pieces[p_idx].activity = act;
                            s.game.log(format!("Order applied manually to {:?} at cursor.", ptype));
                        }
                        None => s.game.log(String::from("Order update cancelled.")),
                    }
                }
                None => s.game.log(String::from("No player unit at cursor location.")),
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => edit_standing_orders(s, cursor),
        code => {
            if let Some((dr, dc)) = keys::dir_for_key(code) {
                s.view.cursor = s.game.shift_cursor(cursor, dr, dc);
            }
        }
    }
    Ok(false)
}

/// Edit mode `I`: set the standing order for new units, in the city at the cursor or
/// globally.
fn edit_standing_orders(s: &mut Session, cursor: Loc) {
    let city = s.game.cities.iter().position(|c| c.loc == cursor && c.owner == Owner::Player);
    // Some(None) = the player said no.
    let scope = match city {
        Some(c_idx) => s.pick("Edit [L]ocal City Order or [G]lobal Fallback? (Esc to cancel): ", |c| match c.to_ascii_lowercase() {
            'l' => Some(Some(OrderScope::City(c_idx))),
            'g' => Some(Some(OrderScope::Global)),
            _ => None,
        }),
        None => s.pick("No city here. Edit [G]lobal Fallback? (Y/N): ", |c| match c.to_ascii_lowercase() {
            'y' | 'g' => Some(Some(OrderScope::Global)),
            'n' => Some(None),
            _ => None,
        }),
    };
    let Some(Some(scope)) = scope else {
        s.game.log(String::from("Order update cancelled."));
        return;
    };
    let scope_name = if scope == OrderScope::Global { "GLOBAL" } else { "LOCAL" };
    let Some(ptype) = s.pick(&format!("{} Order? {}: ", scope_name, PIECE_KEYS), keys::piece_for_key) else {
        s.game.log(String::from("Order update cancelled."));
        return;
    };
    match s.pick_order(ptype, &format!("{} Order for {:?}", scope_name, ptype), cursor, GameMode::Edit) {
        Some(act) => {
            s.game.set_standing_order(scope, ptype, act);
        }
        None => s.game.log(String::from("Order update cancelled.")),
    }
}

const ACTION_PROMPT: &str = "[N]ext, [A]utomove, [M]anual, [S]ave, [L]oad, [Q]uit: ";
/// The prompt once the computer has resigned: its map can be viewed.
const ACTION_PROMPT_RESIGNED: &str = "[N]ext, [A]utomove, [M]anual, [V]iew enemy map, [S]ave, [L]oad, [Q]uit: ";

fn action_prompt(game: &Game) -> &'static str {
    if game.comp_resigned { ACTION_PROMPT_RESIGNED } else { ACTION_PROMPT }
}

/// What the main loop should do after a step that may end the program or the iteration.
enum Flow {
    /// The player quit.
    Quit,
    /// Start the next loop iteration.
    Continue,
    /// Carry on with the rest of this iteration.
    Proceed,
}

/// A unit that needs manual input: idle, or on standing orders but stuck with moves left.
fn awaiting_manual(p: &Piece) -> bool {
    if p.owner != Owner::Player || p.moves_left <= 0 || p.hits <= 0 { return false; }
    if p.ptype == PieceType::Satellite { return false; } // satellites fly themselves
    matches!(p.activity, UnitActivity::Idle | UnitActivity::Explore | UnitActivity::Attack | UnitActivity::Random | UnitActivity::Dir(_, _) | UnitActivity::Dest(_) | UnitActivity::Land)
}

/// Saves to `empire_auto.sav` once per configured interval of turns.
fn autosave_if_due(game: &mut Game, interval: i32, last_autosaved_turn: &mut i32) {
    if interval > 0 && game.turn > 0 && game.turn % interval == 0 && game.turn != *last_autosaved_turn {
        match game.save_game("empire_auto.sav") {
            Ok(()) => game.log(format!("Autosaved game at turn {}.", game.turn)),
            Err(e) => game.log(format!("AUTOSAVE FAILED at turn {}: {}", game.turn, e)),
        }
        *last_autosaved_turn = game.turn;
    }
}

/// Dev observer mode: the player is skipped and the computer plays on until the player cancels.
fn observer_step(s: &mut Session, observer_mode: &mut bool) {
    for p in s.game.pieces.iter_mut() {
        if p.owner == Owner::Player {
            p.moves_left = 0;
        }
    }
    s.game.process_turn();
    s.draw("[DEV OBSERVER MODE] Computer playing. Press 'O' or 'ESC' to cancel...");
    if event::poll(POLL).unwrap_or(false) {
        if let Ok(Event::Key(key)) = event::read() {
            if matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Esc) {
                *observer_mode = false;
                s.view.mode = GameMode::Manual;
                s.game.log(String::from("Exited Dev Observer Mode."));
                drain_input();
            }
        }
    }
}

/// Advances units that are filling a transport or waiting for one.
fn run_standing_orders(game: &mut Game) {
    let mut cleanup_needed = false;
    for i in 0..game.pieces.len() {
        if game.pieces[i].hits > 0 && game.pieces[i].owner == Owner::Player {
            if game.pieces[i].activity == UnitActivity::Fill {
                game.process_fill_mode(i);
            } else if game.pieces[i].activity == UnitActivity::TransportWait && game.process_transport_wait(i) {
                cleanup_needed = true;
            }
        }
    }
    if cleanup_needed {
        game.pieces.retain(|p| p.hits > 0);
    }
}

/// One iteration of automove mode: hands control back to the player if a unit needs orders,
/// otherwise plays the turn and watches for a key that interrupts.
fn auto_step(s: &mut Session) -> io::Result<Flow> {
    if s.game.pieces.iter().any(awaiting_manual) {
        s.view.mode = GameMode::Manual;
        return Ok(Flow::Proceed);
    }
    s.game.process_turn();
    let prompt = action_prompt(&s.game);
    s.draw(prompt);
    std::thread::sleep(AUTO_TURN_PAUSE);

    if event::poll(POLL).unwrap_or(false) {
        let ev = event::read()?;
        if let Event::Mouse(mouse) = ev {
            let home = view::home(&s.game);
            s.handle_mouse(mouse, home);
        }
        if let Event::Key(key) = ev {
            s.view.center = None;
            if is_ctrl_c(&key) {
                return Ok(if s.confirm_quit() { Flow::Quit } else { Flow::Continue });
            }
            if let KeyCode::Char(ch) = key.code {
                match ch.to_ascii_lowercase() {
                    'q' => return Ok(if s.confirm_quit() { Flow::Quit } else { Flow::Continue }),
                    'o' | 'm' => {
                        s.view.mode = GameMode::Manual;
                        s.game.log(String::from("Manual move mode engaged (Auto interrupted)."));
                    }
                    'n' => s.game.process_turn(),
                    _ => {}
                }
            }
        }
    }
    Ok(Flow::Continue)
}

/// Moves the unit at `piece_idx` one square in direction `dir`, after the original's
/// "are you sure?" for a move that would destroy it, and a warning if a fighter would be
/// left without fuel to get home.
fn move_unit(s: &mut Session, piece_idx: usize, dir: (i32, i32)) {
    let (ptype, loc) = (s.game.pieces[piece_idx].ptype, s.game.pieces[piece_idx].loc);
    let Some(target) = s.game.step(loc, dir) else {
        s.game.log(String::from("Cannot move off the edge of the world."));
        return;
    };
    if let Some(prompt) = s.game.suicidal_move_prompt(ptype, Owner::Player, target) {
        if !s.ask_yes_no(prompt) {
            s.game.log(String::from("Move cancelled."));
            drain_input();
            return;
        }
    }
    if ptype == PieceType::Fighter {
        if let Some(dist) = s.game.dist_to_nearest_base(target) {
            let remaining_fuel = s.game.pieces[piece_idx].fuel - 1;
            if remaining_fuel < dist {
                let prompt = format!("WARNING: Move leaves {} fuel, but base is {} away. Crash imminent! Proceed? [Y/N]: ", remaining_fuel, dist);
                if !s.ask_yes_no(&prompt) {
                    s.game.log(String::from("Fighter move cancelled due to fuel warning."));
                    drain_input();
                    return;
                }
            }
        }
    }
    let (result, msg) = s.game.attempt_move(piece_idx, target);
    if !msg.is_empty() {
        s.game.log(msg);
    }
    if let MoveResult::CityCaptured(c_idx) = result {
        s.prompt_city_production(c_idx, true);
    }
}

/// Asks the player for orders for one unit and carries them out.
/// Returns `Ok(true)` if the player quit the program.
fn manual_move(s: &mut Session, piece_idx: usize, mut was_auto: bool, observer_mode: &mut bool) -> io::Result<bool> {
    let unit = &s.game.pieces[piece_idx];
    let (ptype, loc, moves) = (unit.ptype, unit.loc, unit.moves_left);

    let mut opts = vec!["[789/46/123] move"];
    opts.extend(keys::order_menu(ptype));
    if ptype.cargo_type().is_some() {
        // e[X]ject sits after [F]ill
        let at = opts.iter().position(|o| *o == "[F]ill").map_or(opts.len(), |i| i + 1);
        opts.insert(at, "e[X]ject");
    }
    let stuck = match unit.activity {
        UnitActivity::Dir(_, _) => " [STUCK: Dir]",
        UnitActivity::Dest(_) => " [STUCK: Dest]",
        UnitActivity::Explore => " [STUCK: Grope]",
        UnitActivity::Attack => " [STUCK: Attack]",
        UnitActivity::Random => " [STUCK: Random]",
        UnitActivity::Land => " [STUCK: Land]",
        _ => "",
    };
    let move_prompt = format!("[{:?} at ({},{}) M:{}{}]: {}: ", ptype, loc.r, loc.c, moves, stuck, opts.join(","));

    let since = Instant::now();
    loop {
        s.draw_blinking(&move_prompt, (loc, blink_on(since)));
        if !event::poll(POLL).unwrap_or(false) {
            continue;
        }
        let ev = event::read()?;
        if let Event::Mouse(mouse) = ev {
            s.handle_mouse(mouse, loc);
        }
        let Event::Key(key) = ev else { continue };
        s.view.center = None;
        if is_ctrl_c(&key) {
            if s.confirm_quit() {
                return Ok(true);
            }
            continue;
        }
        let KeyCode::Char(ch) = key.code else { continue };

        if let Some(dir) = keys::dir_for_digit(ch) {
            move_unit(s, piece_idx, dir);
            break;
        }
        match ch.to_ascii_lowercase() {
            ' ' | 'n' => s.game.skip_unit(piece_idx),
            's' => s.game.sentry_unit(piece_idx),
            't' if ptype == PieceType::Army => {
                s.game.pieces[piece_idx].moves_left = 0;
                s.game.order_unit(piece_idx, UnitActivity::TransportWait);
            }
            'w' => s.game.wake_unit(piece_idx),
            'x' => {
                if s.game.pieces[piece_idx].cargo.is_empty() {
                    s.game.log(String::from("No cargo to eject/launch."));
                    continue;
                }
                s.game.wake_cargo(piece_idx);
            }
            'd' => {
                let prompt = format!("Pick target for {:?} - [Arrows/NumPad] Move | [Enter] Confirm | [Esc] Cancel: ", ptype);
                match s.pick_location(&prompt, loc, GameMode::PickDest(piece_idx)) {
                    Some(dest) => s.game.order_unit(piece_idx, UnitActivity::Dest(dest)),
                    None => s.game.log(String::from("Destination selection cancelled.")),
                }
                drain_input();
            }
            'v' => {
                match s.pick_direction(ptype) {
                    Some((dr, dc)) => s.game.order_unit(piece_idx, UnitActivity::Dir(dr, dc)),
                    None => s.game.log(String::from("March order cancelled.")),
                }
                drain_input();
            }
            'q' => {
                if s.confirm_quit() {
                    return Ok(true);
                }
            }
            'z' => {
                if cfg!(debug_assertions) {
                    *observer_mode = true;
                    s.game.log(String::from("Dev Observer Mode engaged. Player turns skipped."));
                }
            }
            'a' => {
                s.view.mode = GameMode::Auto;
                was_auto = true;
                s.game.log(String::from("Automove mode engaged. Press 'o' or 'm' to cancel."));
            }
            'o' | 'm' => {
                s.view.mode = GameMode::Manual;
                was_auto = false;
                s.game.log(String::from("Manual move mode engaged."));
            }
            'e' => {
                s.view.mode = GameMode::Edit;
                s.view.cursor = loc;
                was_auto = false;
                s.game.log(String::from("Edit Mode engaged."));
                drain_input();
            }
            c => {
                // y/g/r/f/l/u: an order carried out at once (s, t, w, d and v are above)
                if let Some(OrderKey::Order(act)) = keys::order_for_key(c, ptype) {
                    s.game.order_unit(piece_idx, act);
                }
            }
        }
        break;
    }

    if was_auto {
        s.view.mode = GameMode::Auto;
    }
    Ok(false)
}

/// Handles one key or mouse event while no unit needs orders (the "[N]ext, [A]utomove..." prompt).
/// Returns `Ok(true)` if the player quit the program.
fn idle_input(s: &mut Session, observer_mode: &mut bool) -> io::Result<bool> {
    let prompt = action_prompt(&s.game);
    s.draw(prompt);
    if !event::poll(IDLE_POLL).unwrap_or(false) {
        return Ok(false);
    }
    let ev = event::read()?;
    if let Event::Mouse(mouse) = ev {
        let home = view::home(&s.game);
        s.handle_mouse(mouse, home);
    }
    let Event::Key(key) = ev else { return Ok(false) };
    s.view.center = None;
    if is_ctrl_c(&key) {
        return Ok(s.confirm_quit());
    }
    let KeyCode::Char(ch) = key.code else { return Ok(false) };
    match ch.to_ascii_lowercase() {
        'q' => return Ok(s.confirm_quit()),
        'n' => s.game.process_turn(),
        '`' | '~' if cfg!(debug_assertions) => s.game.reveal_entire_map(),
        'z' if cfg!(debug_assertions) => {
            *observer_mode = true;
            s.game.log(String::from("Dev Observer Mode engaged. Player turns skipped."));
        }
        'a' => {
            s.view.mode = GameMode::Auto;
            s.game.log(String::from("Automove mode engaged. Press 'o' or 'm' to cancel."));
        }
        'o' | 'm' => {
            s.view.mode = GameMode::Manual;
            s.game.log(String::from("Manual move mode engaged."));
        }
        'e' => {
            s.view.mode = GameMode::Edit;
            s.view.cursor = view::home(&s.game);
            s.game.log(String::from("Edit Mode engaged."));
            drain_input();
        }
        's' => match s.game.save_game("empire.sav") {
            Ok(()) => s.game.log(String::from("Game saved to empire.sav.")),
            Err(e) => s.game.log(format!("SAVE FAILED: could not write empire.sav: {}", e)),
        },
        'v' if s.game.comp_resigned => {
            s.view.show_comp_view = !s.view.show_comp_view;
            s.game.log(String::from(if s.view.show_comp_view {
                "Showing what the enemy has learned. [V] again for your own map."
            } else {
                "Back to your own map."
            }));
        }
        'l' => match Game::load_game("empire.sav") {
            Ok(loaded) => {
                s.game = loaded;
                s.view = View::default();
                s.game.log(String::from("Game loaded from empire.sav."));
            }
            Err(reason) => s.game.log(format!("LOAD FAILED: {}", reason)),
        },
        _ => s.game.log(format!("Unknown command '{}'.", ch)),
    }
    Ok(false)
}

/// Shows the final message until a key is pressed.
fn game_over(s: &mut Session, msg: String) {
    s.game.log(msg);
    s.draw(" GAME OVER! Press any key to exit... ");
    read_key();
}

/// The main input/turn loop.
fn play(mut s: Session, autosave_interval: i32) -> io::Result<()> {
    let mut last_autosaved_turn = 0;
    let mut observer_mode = false;
    let prompt = action_prompt(&s.game);
    s.draw(prompt);

    loop {
        autosave_if_due(&mut s.game, autosave_interval, &mut last_autosaved_turn);

        if let Some(msg) = s.game.check_game_over() {
            game_over(&mut s, msg);
            return Ok(());
        }

        if s.game.comp_should_resign() && !observer_mode {
            if !s.ask_yes_no("The computer acknowledges defeat. Do you wish to smash the rest of the enemy? [Y/N]: ") {
                game_over(&mut s, String::from(">>> VICTORY! The computer acknowledges defeat. <<<"));
                return Ok(());
            }
            s.game.comp_resigned = true;
            s.game.log(String::from("The enemy inadvertently revealed its code used for receiving battle information."));
            s.game.log(String::from("You can display what they've learned with the [V]iew command."));
            continue;
        }

        if let Some(c_idx) = s.game.pending_cities.pop() {
            if observer_mode {
                s.game.set_production(c_idx, PieceType::Army, true);
            } else {
                s.prompt_city_production(c_idx, true);
            }
            continue;
        }

        if observer_mode {
            observer_step(&mut s, &mut observer_mode);
            continue;
        }

        if s.view.mode == GameMode::Edit {
            if edit_mode_step(&mut s)? {
                return Ok(());
            }
            continue;
        }

        run_standing_orders(&mut s.game);

        let was_auto = s.view.mode == GameMode::Auto;
        if was_auto {
            match auto_step(&mut s)? {
                Flow::Quit => return Ok(()),
                Flow::Continue => continue,
                Flow::Proceed => {}
            }
        }

        if let Some(piece_idx) = s.game.pieces.iter().position(awaiting_manual) {
            if manual_move(&mut s, piece_idx, was_auto, &mut observer_mode)? {
                return Ok(());
            }
            continue;
        }

        if idle_input(&mut s, &mut observer_mode)? {
            return Ok(());
        }
    }
}
