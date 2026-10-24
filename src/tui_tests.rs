//! Automated tests of the terminal interface.
//!
//! Three layers:
//! 1. **Screen tests** render game states into ratatui's in-memory `TestBackend` and
//!    check what is on screen.
//! 2. **Scripted tests** drive the real front end (`app.rs`: startup questions, the
//!    main loop, every prompt) with a `ScriptUi` that feeds a list of key and mouse
//!    events, then check the game, the view and the screen.
//! 3. **Fuzz tests** feed long random key and mouse sequences into seeded games and
//!    fail on any panic, hang, or game state that no longer saves and loads.
//!
//! When a script runs out, every prompt gets a `SCRIPT_END` error, so a test simply
//! ends wherever the game is waiting for input. A front-end loop that keeps redrawing
//! without ever asking for input is reported as a hang.
use crate::app::{self, Options, Session};
use crate::disp;
use crate::game::Game;
use crate::models::{Loc, Owner, Piece, PieceType, Terrain, UnitActivity};
use crate::ui::Ui;
use crate::view::{GameMode, View};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use rand::{Rng, SeedableRng};
use ratatui::{backend::TestBackend, Terminal};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------
// The scripted UI
// ---------------------------------------------------------------------------------

/// The error every input call returns once the script has run out.
const SCRIPT_END: &str = "test script finished";
/// Redraws allowed without any input being asked for before it counts as a hang.
const HANG_DRAWS: usize = 5_000;

/// One scripted input.
#[derive(Clone, Debug)]
enum Step {
    /// A key or mouse event.
    Ev(Event),
    /// The player does nothing for this many polls (for automove and blinking).
    Wait(u32),
}

/// What the scripted UI has seen, shared with the test so it can look after the run.
struct Screen {
    steps: VecDeque<Step>,
    term: Terminal<TestBackend>,
    /// The prompt of the last game screen drawn.
    prompt: String,
    /// Every startup banner drawn: (title, question, typed input).
    banners: Vec<(String, String, String)>,
    /// Game screens drawn in all.
    draws: usize,
    /// Game screens drawn since input was last asked for.
    draws_since_input: usize,
    /// Pauses requested (automove).
    pauses: usize,
    /// The kinds of prompt drawn (their first words), to show what a fuzz run reached.
    prompts_seen: std::collections::BTreeSet<String>,
}

impl Screen {
    /// The screen as lines of text.
    fn lines(&self) -> Vec<String> {
        let buf = self.term.backend().buffer();
        let w = buf.area.width as usize;
        buf.content().chunks(w).map(|row| row.iter().map(|c| c.symbol()).collect::<String>()).collect()
    }

    fn text(&self) -> String {
        self.lines().join("\n")
    }

    /// The character at (`x`, `y`) on screen.
    fn at(&self, x: u16, y: u16) -> String {
        self.term.backend().buffer().get(x, y).symbol().to_string()
    }
}

#[derive(Clone)]
struct ScriptUi(Rc<RefCell<Screen>>);

impl ScriptUi {
    fn new(width: u16, height: u16, steps: Vec<Step>) -> Self {
        ScriptUi(Rc::new(RefCell::new(Screen {
            steps: steps.into(),
            term: Terminal::new(TestBackend::new(width, height)).unwrap(),
            prompt: String::new(),
            banners: Vec::new(),
            draws: 0,
            draws_since_input: 0,
            pauses: 0,
            prompts_seen: Default::default(),
        })))
    }

    fn screen(&self) -> std::cell::Ref<'_, Screen> {
        self.0.borrow()
    }

    fn remaining(&self) -> usize {
        self.0.borrow().steps.iter().filter(|s| matches!(s, Step::Ev(_))).count()
    }
}

fn script_end() -> io::Error {
    io::Error::other(SCRIPT_END)
}

fn is_script_end(e: &io::Error) -> bool {
    e.to_string() == SCRIPT_END
}

impl Ui for ScriptUi {
    fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
        let mut s = self.0.borrow_mut();
        s.draws_since_input = 0;
        loop {
            match s.steps.front_mut() {
                None => return Err(script_end()),
                Some(Step::Ev(_)) => return Ok(true),
                Some(Step::Wait(0)) => {
                    s.steps.pop_front();
                }
                Some(Step::Wait(n)) => {
                    *n -= 1;
                    return Ok(false);
                }
            }
        }
    }

    fn read(&mut self) -> io::Result<Event> {
        let mut s = self.0.borrow_mut();
        s.draws_since_input = 0;
        loop {
            match s.steps.pop_front() {
                None => return Err(script_end()),
                Some(Step::Ev(ev)) => return Ok(ev),
                Some(Step::Wait(_)) => {}
            }
        }
    }

    /// Typed-ahead keys are the script itself, so nothing is thrown away.
    fn drain(&mut self) {}

    fn draw(&mut self, game: &Game, view: &mut View, prompt: &str, blink: Option<(Loc, bool)>) -> io::Result<()> {
        view.follow_messages(game);
        let mut s = self.0.borrow_mut();
        s.draws += 1;
        s.draws_since_input += 1;
        assert!(s.draws_since_input < HANG_DRAWS, "the front end redrew {} times without asking for input (hang); prompt: {}", HANG_DRAWS, prompt);
        s.prompt = prompt.to_string();
        let kind: String = prompt.split(|c: char| c == '(' || c == ':' || c.is_ascii_digit()).next().unwrap_or("").trim().to_string();
        s.prompts_seen.insert(kind);
        let view = &*view;
        s.term.draw(|f| disp::render(f, game, view, prompt, blink))?;
        Ok(())
    }

    fn banner(&mut self, title: &str, question: &str, input: &str) -> io::Result<()> {
        self.0.borrow_mut().banners.push((title.into(), question.into(), input.into()));
        Ok(())
    }

    fn pause(&mut self, _duration: Duration) {
        self.0.borrow_mut().pauses += 1;
    }

    /// Blinking squares are always shown, so screens are deterministic.
    fn blink_on(&self, _since: Instant) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------
// Building scripts
// ---------------------------------------------------------------------------------

fn key_event(code: KeyCode, modifiers: KeyModifiers) -> Step {
    Step::Ev(Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }))
}

fn key(code: KeyCode) -> Step {
    key_event(code, KeyModifiers::NONE)
}

fn ctrl_c() -> Step {
    key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Step {
    Step::Ev(Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }))
}

/// Keys typed as a string: `\r` is Enter, `\x1b` is Esc, `\x08` is Backspace.
fn typed(s: &str) -> Vec<Step> {
    s.chars()
        .map(|c| match c {
            '\r' => key(KeyCode::Enter),
            '\x1b' => key(KeyCode::Esc),
            '\x08' => key(KeyCode::Backspace),
            c => key(KeyCode::Char(c)),
        })
        .collect()
}

// ---------------------------------------------------------------------------------
// Worlds and sessions
// ---------------------------------------------------------------------------------

const SEED: u64 = 1792784026;

/// A full-size world with both sides placed, at the fairest level.
fn world(seed: u64) -> Game {
    let mut g = Game::with_seed(crate::game::MAP_ROWS, crate::game::MAP_COLS, 0, seed);
    g.generate_world();
    let level = crate::headless::fair_level(&g);
    g.choose_start(level);
    g
}

fn player_city(g: &Game) -> usize {
    g.cities.iter().position(|c| c.owner == Owner::Player).unwrap()
}

fn comp_city(g: &Game) -> usize {
    g.cities.iter().position(|c| c.owner == Owner::Comp).unwrap()
}

/// Puts a fresh unit of the player's on `loc` and returns its index.
fn add_unit(g: &mut Game, ptype: PieceType, owner: Owner, loc: Loc) -> usize {
    let id = g.next_free_id();
    g.next_id = id + 1;
    g.pieces.push(Piece {
        id,
        ptype,
        owner,
        loc,
        hits: ptype.max_hits(),
        moves_left: ptype.max_moves(),
        activity: UnitActivity::Idle,
        cargo: Vec::new(),
        fuel: if ptype == PieceType::Fighter { crate::game::FIGHTER_FUEL } else { 0 },
    });
    g.pieces.len() - 1
}

/// A direction from `loc` whose square has terrain `t` and no city, if any.
fn neighbour(g: &Game, loc: Loc, t: Terrain) -> Option<((i32, i32), char)> {
    let dirs = [((-1, -1), '7'), ((-1, 0), '8'), ((-1, 1), '9'), ((0, -1), '4'), ((0, 1), '6'), ((1, -1), '1'), ((1, 0), '2'), ((1, 1), '3')];
    dirs.into_iter().find(|&(d, _)| {
        g.step(loc, d).is_some_and(|l| g.map[l.r][l.c] == t && !g.cities.iter().any(|c| c.loc == l))
    })
}

/// A session on `game` whose input is `steps`, drawn on a `width` x `height` screen.
fn session(game: Game, width: u16, height: u16, steps: Vec<Step>) -> (Session, ScriptUi) {
    let ui = ScriptUi::new(width, height, steps);
    let mut s = Session::new(game, Box::new(ui.clone()));
    let dir = std::env::temp_dir().join(format!("rs_empire_tui_{}_{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).unwrap();
    s.save_file = dir.join("empire.sav").to_string_lossy().into_owned();
    s.autosave_file = dir.join("empire_auto.sav").to_string_lossy().into_owned();
    (s, ui)
}

/// Plays `s` until the script runs out (the usual end of a test) or the game ends.
/// Returns true if the game ended by itself (quit or game over).
fn play(s: &mut Session) -> bool {
    match app::play(s, 0) {
        Ok(()) => true,
        Err(e) if is_script_end(&e) => false,
        Err(e) => panic!("the front end failed: {}", e),
    }
}

/// The screen square showing map square `loc` when the map is centred on `center`, on
/// a `width` x `height` terminal (the layout of `disp::render`).
fn map_square(g: &Game, center: Loc, width: u16, height: u16, loc: Loc) -> Option<(u16, u16)> {
    let map_w = width.saturating_sub(30);
    let map_h = height.saturating_sub(9);
    let (rows, cols) = disp::view_axes(g, center, map_h.saturating_sub(2) as usize, map_w.saturating_sub(2) as usize);
    let y = rows.iter().position(|&r| r == loc.r)? as u16 + 1;
    let x = cols.iter().position(|&c| c == loc.c)? as u16 + 1;
    Some((x, y))
}

// ---------------------------------------------------------------------------------
// 1. Screen tests
// ---------------------------------------------------------------------------------

/// Renders `game` once with `prompt` and returns the screen.
fn render(game: &Game, view: &mut View, width: u16, height: u16, prompt: &str, blink: Option<(Loc, bool)>) -> ScriptUi {
    let mut ui = ScriptUi::new(width, height, Vec::new());
    ui.draw(game, view, prompt, blink).unwrap();
    ui
}

#[test]
fn screen_renders_at_any_size_in_every_mode() {
    let mut g = world(SEED);
    for _ in 0..30 {
        g.process_turn();
    }
    let home = g.cities[player_city(&g)].loc;
    let modes = [GameMode::Manual, GameMode::Auto, GameMode::Edit, GameMode::PickDest(0)];
    for &(w, h) in &[(1, 1), (10, 5), (30, 10), (40, 12), (60, 20), (80, 24), (120, 40), (200, 70), (300, 100)] {
        for mode in modes {
            for comp_view in [false, true] {
                let mut view = View { mode, cursor: home, show_comp_view: comp_view, ..View::default() };
                render(&g, &mut view, w, h, "[N]ext, [A]utomove, [M]anual, [S]ave, [L]oad, [Q]uit: ", Some((home, true)));
            }
        }
    }
}

#[test]
fn screen_title_and_status_match_the_game() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    for _ in 0..3 {
        add_unit(&mut g, PieceType::Army, Owner::Player, home);
    }
    g.turn = 42;
    g.player_score = 17;
    let ui = render(&g, &mut View::default(), 120, 40, "prompt", None);
    let text = ui.screen().text();
    assert!(text.contains("Turn: 42"), "{}", text);
    assert!(text.contains("Mode: Manual"));
    assert!(text.contains("My Score: 17"));
    assert!(text.contains("Armies: 3"));
    assert!(text.contains("My Cities: 1"));
    assert!(text.contains("--- LEGEND ---"));
}

#[test]
fn screen_shows_the_whole_unit_prompt_at_80_columns() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
    let prompt = app::unit_prompt(&g.pieces[a]);
    let ui = render(&g, &mut View::default(), 80, 24, &prompt, Some((home, true)));
    let lines = ui.screen().lines();
    let bottom = format!("{}{}", lines[22], lines[23]).replace(' ', "");
    for k in ["[S]entry", "[D]est", "[V]ctr", "[W]ake", "[T]rnsWait"] {
        assert!(bottom.contains(k), "{} missing from the prompt lines:\n{}\n{}", k, lines[22], lines[23]);
    }
}

#[test]
fn screen_fog_hides_what_the_player_has_not_seen() {
    let mut g = world(SEED);
    let enemy = g.cities[comp_city(&g)].loc;
    let mut view = View { center: Some(enemy), ..View::default() };
    let (x, y) = map_square(&g, enemy, 120, 40, enemy).unwrap();
    let ui = render(&g, &mut view, 120, 40, "", None);
    assert_eq!(ui.screen().at(x, y), " ", "the enemy's city is unexplored at the start");

    g.reveal_vision(enemy, 1);
    let ui = render(&g, &mut view, 120, 40, "", None);
    assert_eq!(ui.screen().at(x, y), "X", "once seen, it shows as an enemy city");

    // Lost from sight, it stays as last seen even if it changes hands.
    let c = comp_city(&g);
    g.cities[c].owner = Owner::Neutral;
    let ui = render(&g, &mut view, 120, 40, "", None);
    assert_eq!(ui.screen().at(x, y), "X", "out of sight, the map shows the owner last seen");
}

#[test]
fn screen_blinking_unit_is_hidden_on_the_off_beat() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    add_unit(&mut g, PieceType::Fighter, Owner::Player, home);
    let (x, y) = map_square(&g, home, 120, 40, home).unwrap();
    let mut view = View::default();
    assert_eq!(render(&g, &mut view, 120, 40, "", Some((home, true))).screen().at(x, y), "F");
    assert_eq!(render(&g, &mut view, 120, 40, "", Some((home, false))).screen().at(x, y), "O", "the city shows through");
}

#[test]
fn screen_edit_mode_describes_the_square_under_the_cursor() {
    let g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let mut view = View { mode: GameMode::Edit, cursor: home, ..View::default() };
    let text = render(&g, &mut view, 140, 45, "", None).screen().text();
    assert!(text.contains("CURSOR INFO"), "{}", text);
    assert!(text.contains("City: Player"));
    assert!(text.contains("Prod: Army"));
}

#[test]
fn screen_edit_cursor_is_visible_everywhere() {
    // Regression: the cursor was drawn only on explored squares, so moving it into the
    // fog (or viewing the enemy's map) made it vanish.
    let mut g = world(SEED);
    let enemy = g.cities[comp_city(&g)].loc;
    assert_eq!(g.player_map[enemy.r][enemy.c], 0, "the test needs an unexplored square");
    for comp_view in [false, true] {
        let mut view = View { mode: GameMode::Edit, cursor: enemy, show_comp_view: comp_view, ..View::default() };
        let (x, y) = map_square(&g, enemy, 120, 40, enemy).unwrap();
        let ui = render(&g, &mut view, 120, 40, "", None);
        assert_eq!(ui.screen().at(x, y), "█", "cursor hidden (enemy map: {})", comp_view);
        assert!(ui.screen().text().contains(&format!("At: ({},{})", enemy.r, enemy.c)), "the panel says where the cursor is");
    }
}

#[test]
fn screen_message_log_shows_the_newest_and_scrolls_back() {
    let mut g = world(SEED);
    for i in 0..20 {
        g.log(format!("message number {}", i));
    }
    let mut view = View::default();
    let text = render(&g, &mut view, 120, 40, "", None).screen().text();
    assert!(text.contains("message number 19") && !text.contains("message number 10"));
    view.msg_scroll = 5;
    let text = render(&g, &mut view, 120, 40, "", None).screen().text();
    assert!(text.contains("Scroll: 5") && text.contains("message number 14") && !text.contains("message number 19"));
}

// ---------------------------------------------------------------------------------
// 2. Scripted tests
// ---------------------------------------------------------------------------------

fn new_game_opts(seed: u64) -> Options {
    Options { load_file: None, seed: Some(seed), torus: false }
}

#[test]
fn startup_asks_level_autosave_and_first_production() {
    let ui = ScriptUi::new(120, 40, typed("0\r5\rf"));
    let (s, interval) = app::start_session(new_game_opts(SEED), Box::new(ui.clone())).unwrap().unwrap();
    assert_eq!(interval, 5);
    let c = player_city(&s.game);
    assert_eq!(s.game.cities[c].prod, PieceType::Fighter);
    let screen = ui.screen();
    assert!(screen.banners.iter().any(|(_, q, _)| q.contains("difficulty level")));
    assert!(screen.banners.iter().any(|(t, _, _)| t.contains("Autosave")));
    assert!(screen.prompt.contains("Build?"), "the first city is asked about: {}", screen.prompt);
}

#[test]
fn startup_rejects_an_out_of_range_level_and_echoes_typing() {
    let ui = ScriptUi::new(120, 40, typed("9999\r12\x08\x080\r0\ra"));
    let (s, _) = app::start_session(new_game_opts(SEED), Box::new(ui.clone())).unwrap().unwrap();
    assert_eq!(s.game.cities[player_city(&s.game)].prod, PieceType::Army);
    let screen = ui.screen();
    assert!(screen.banners.iter().any(|(t, _, _)| t.starts_with("Please enter an integer")), "a bad level is refused");
    assert!(screen.banners.iter().any(|(_, _, input)| input == "12"), "typing is echoed");
    assert!(screen.banners.iter().any(|(_, _, input)| input == "1"), "backspace removes a digit");
}

#[test]
fn startup_ctrl_c_backs_out_cleanly() {
    let ui = ScriptUi::new(120, 40, vec![ctrl_c()]);
    assert!(app::start_session(new_game_opts(SEED), Box::new(ui)).unwrap().is_none());
    let mut steps = typed("0\r");
    steps.push(ctrl_c());
    let ui = ScriptUi::new(120, 40, steps);
    assert!(app::start_session(new_game_opts(SEED), Box::new(ui)).unwrap().is_none(), "also at the autosave question");
}

#[test]
fn startup_reports_a_save_that_cannot_be_loaded() {
    let opts = Options { load_file: Some("/nonexistent/empire.sav".into()), seed: None, torus: false };
    let err = app::start_session(opts, Box::new(ScriptUi::new(80, 24, Vec::new()))).err().expect("an error");
    assert!(err.to_string().contains("failed to load save file"), "{}", err);
}

#[test]
fn startup_resumes_a_saved_game() {
    let mut g = world(SEED);
    g.turn = 77;
    let path = std::env::temp_dir().join(format!("rs_empire_tui_resume_{}.sav", std::process::id()));
    g.save_game(path.to_str().unwrap()).unwrap();
    let opts = Options { load_file: Some(path.to_string_lossy().into_owned()), seed: None, torus: false };
    let ui = ScriptUi::new(80, 24, typed("0\r"));
    let (s, _) = app::start_session(opts, Box::new(ui.clone())).unwrap().unwrap();
    assert_eq!(s.game.turn, 77);
    assert!(!ui.screen().banners.iter().any(|(_, q, _)| q.contains("difficulty")), "no new-game questions");
    let _ = std::fs::remove_file(path);
}

#[test]
fn army_moves_with_the_keypad() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
    let id = g.pieces[a].id;
    let ((dr, dc), k) = neighbour(&g, home, Terrain::Land).expect("land next to the capital");
    let (mut s, ui) = session(g, 120, 40, typed(&k.to_string()));
    assert!(!play(&mut s));
    let army = s.game.pieces.iter().find(|p| p.id == id).unwrap();
    assert_eq!(army.loc, s.game.step(home, (dr, dc)).unwrap());
    assert!(ui.screen().prompt.starts_with("[N]ext"), "with nothing left to move, the turn prompt shows");
}

#[test]
fn walking_into_the_sea_asks_first() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
    let id = g.pieces[a].id;
    let (_, k) = neighbour(&g, home, Terrain::Sea).expect("the capital is a port");

    // "n": the move is called off and the army waits for orders again.
    let (mut s, ui) = session(g.clone(), 120, 40, typed(&format!("{}n", k)));
    assert!(!play(&mut s));
    assert!(s.game.pieces.iter().any(|p| p.id == id && p.loc == home && p.hits > 0));
    assert!(s.game.messages.iter().any(|m| m == "Move cancelled."));
    assert!(ui.screen().prompt.contains("[Army at"), "{}", ui.screen().prompt);

    // "y": the army drowns.
    let (mut s, _) = session(g, 120, 40, typed(&format!("{}y", k)));
    assert!(!play(&mut s));
    assert!(!s.game.pieces.iter().any(|p| p.id == id && p.hits > 0));
}

#[test]
fn orders_from_the_unit_prompt() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    for _ in 0..4 {
        add_unit(&mut g, PieceType::Army, Owner::Player, home);
    }
    // sentry, explore, march east, and a destination three squares right of home
    let mut steps = typed("sgv6d");
    steps.extend([key(KeyCode::Right), key(KeyCode::Right), key(KeyCode::Right), key(KeyCode::Enter)]);
    let (mut s, _) = session(g, 120, 40, steps);
    assert!(!play(&mut s));
    let acts: Vec<UnitActivity> = s.game.pieces.iter().filter(|p| p.owner == Owner::Player).map(|p| p.activity).collect();
    assert!(acts.contains(&UnitActivity::Sentry), "{:?}", acts);
    assert!(acts.contains(&UnitActivity::Explore) || s.game.messages.iter().any(|m| m.contains("grope")), "{:?}", acts);
    assert!(acts.contains(&UnitActivity::Dir(0, 1)) || s.game.messages.iter().any(|m| m.contains("march")), "{:?}", acts);
    let dest = s.game.shift_cursor(s.game.shift_cursor(s.game.shift_cursor(home, 0, 1), 0, 1), 0, 1);
    assert!(acts.contains(&UnitActivity::Dest(dest)), "{:?}", acts);
}

#[test]
fn escape_backs_out_of_pickers() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
    let (mut s, ui) = session(g, 120, 40, typed("v\x1bd\x1b"));
    assert!(!play(&mut s));
    assert_eq!(s.game.pieces[a].activity, UnitActivity::Idle);
    assert!(s.game.messages.iter().any(|m| m == "March order cancelled."));
    assert!(s.game.messages.iter().any(|m| m == "Destination selection cancelled."));
    assert_eq!(s.view.mode, GameMode::Manual, "the picker's mode is put back");
    assert!(ui.screen().prompt.contains("[Army at"));
}

#[test]
fn skipping_units_and_ending_turns() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    add_unit(&mut g, PieceType::Army, Owner::Player, home);
    let turn = g.turn;
    // space skips the army; at the turn prompt n ends the turn; at the army's prompt n
    // skips it; then n ends the turn again
    let (mut s, _) = session(g, 120, 40, typed(" nnn"));
    assert!(!play(&mut s));
    assert_eq!(s.game.turn, turn + 2);
}

#[test]
fn quitting_asks_to_confirm() {
    let g = world(SEED);
    let (mut s, _) = session(g.clone(), 120, 40, typed("qn"));
    assert!(!play(&mut s), "N keeps playing");
    assert!(s.game.messages.iter().any(|m| m == "Quit cancelled."));
    let (mut s, _) = session(g.clone(), 120, 40, typed("qy"));
    assert!(play(&mut s), "Y quits");
    let mut steps = vec![ctrl_c()];
    steps.extend(typed("y"));
    let (mut s, _) = session(g, 120, 40, steps);
    assert!(play(&mut s), "so does Ctrl-C");
}

#[test]
fn edit_mode_production_and_standing_orders() {
    let g = world(SEED);
    let c = player_city(&g);
    // e: edit mode (cursor starts on the capital); p f: build fighters;
    // i l f s: fighters built here go on sentry; i l a g: armies explore; e: leave.
    let (mut s, ui) = session(g, 120, 40, typed("epfilfsilage"));
    assert!(!play(&mut s));
    let city = &s.game.cities[c];
    assert_eq!(city.prod, PieceType::Fighter);
    assert!(city.default_orders.contains(&(PieceType::Fighter, UnitActivity::Sentry)), "{:?}", city.default_orders);
    assert!(city.default_orders.contains(&(PieceType::Army, UnitActivity::Explore)), "{:?}", city.default_orders);
    assert_eq!(s.view.mode, GameMode::Manual);
    assert!(ui.screen().prompt.starts_with("[N]ext"));
}

#[test]
fn edit_mode_global_orders_and_cursor() {
    let g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let mut steps = typed("e");
    steps.extend([key(KeyCode::Down), key(KeyCode::Down)]);
    steps.extend(typed("ig"));
    steps.extend(typed("ty")); // transports attack
    let (mut s, _) = session(g, 120, 40, steps);
    assert!(!play(&mut s));
    assert_eq!(s.view.mode, GameMode::Edit);
    assert_eq!(s.view.cursor, s.game.shift_cursor(s.game.shift_cursor(home, 1, 0), 1, 0));
    assert!(s.game.default_orders.contains(&(PieceType::Transport, UnitActivity::Attack)), "{:?}", s.game.default_orders);
}

#[test]
fn save_and_load_keys() {
    let mut g = world(SEED);
    g.turn = 9;
    let (mut s, _) = session(g, 120, 40, typed("s"));
    assert!(!play(&mut s));
    assert!(std::path::Path::new(&s.save_file).exists());
    assert!(s.game.messages.last().unwrap().starts_with("Game saved"));
    s.game.turn = 50;
    s.ui = Box::new(ScriptUi::new(120, 40, typed("l")));
    assert!(!play(&mut s));
    assert_eq!(s.game.turn, 9, "L brings back the saved game");
    assert!(s.game.messages.last().unwrap().starts_with("Game loaded"));
}

#[test]
fn a_failed_load_is_reported_not_fatal() {
    let g = world(SEED);
    let (mut s, _) = session(g, 120, 40, typed("l"));
    s.save_file = "/nonexistent/dir/empire.sav".into();
    assert!(!play(&mut s));
    assert!(s.game.messages.last().unwrap().starts_with("LOAD FAILED"));
}

#[test]
fn automove_plays_turns_until_stopped() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
    g.pieces[a].activity = UnitActivity::Sentry;
    let turn = g.turn;
    let mut steps = typed("a");
    steps.push(Step::Wait(3));
    steps.extend(typed("m"));
    let (mut s, ui) = session(g, 120, 40, steps);
    assert!(!play(&mut s));
    assert!(s.game.turn >= turn + 3, "turn {} -> {}", turn, s.game.turn);
    assert!(ui.screen().pauses >= 3, "automove pauses so it can be watched");
}

#[test]
fn mouse_scrolls_the_log_and_pans_the_map() {
    let mut g = world(SEED);
    for i in 0..30 {
        g.log(format!("note {}", i));
    }
    let steps = vec![
        mouse(MouseEventKind::ScrollUp, 10, 30),
        mouse(MouseEventKind::ScrollUp, 10, 30),
        mouse(MouseEventKind::ScrollUp, 10, 30),
        mouse(MouseEventKind::ScrollDown, 10, 30),
        mouse(MouseEventKind::Down(MouseButton::Left), 40, 10),
        mouse(MouseEventKind::Drag(MouseButton::Left), 35, 8),
    ];
    let (mut s, ui) = session(g, 120, 40, steps);
    assert!(!play(&mut s));
    assert_eq!(s.view.msg_scroll, 2);
    assert!(ui.screen().text().contains("Scroll: 2"));
    let home = s.game.cities[player_city(&s.game)].loc;
    assert_eq!(s.view.center, Some(s.game.shift_cursor(home, 2, 5)), "dragging up-left looks down-right");
}

#[test]
fn a_captured_city_asks_what_to_build() {
    let mut g = world(SEED);
    let neutral = g.cities.iter().position(|c| c.owner == Owner::Neutral && g.is_coastal(c.loc)).unwrap();
    g.cities[neutral].owner = Owner::Player;
    g.pending_cities.push(neutral);
    let (mut s, _) = session(g, 120, 40, typed("xt"));
    assert!(!play(&mut s));
    assert_eq!(s.game.cities[neutral].prod, PieceType::Transport);
    assert!(s.game.messages.iter().any(|m| m.contains("Invalid choice 'x'")), "a wrong key is explained");
}

#[test]
fn a_landlocked_city_refuses_ships() {
    let mut g = world(SEED);
    let Some(inland) = g.cities.iter().position(|c| c.owner == Owner::Neutral && !g.is_coastal(c.loc)) else { return };
    g.cities[inland].owner = Owner::Player;
    g.pending_cities.push(inland);
    let (mut s, _) = session(g, 120, 40, typed("ba"));
    assert!(!play(&mut s));
    assert_eq!(s.game.cities[inland].prod, PieceType::Army);
    assert!(s.game.messages.iter().any(|m| m.contains("landlocked")));
}

/// A world where the computer is down to one city and no units.
fn computer_beaten() -> Game {
    let mut g = world(SEED);
    let keep = comp_city(&g);
    let neutrals: Vec<usize> = (0..g.cities.len()).filter(|&i| g.cities[i].owner == Owner::Neutral).collect();
    for i in neutrals.into_iter().take(6) {
        g.cities[i].owner = Owner::Player;
    }
    g.pieces.retain(|p| p.owner != Owner::Comp);
    let home = g.cities[player_city(&g)].loc;
    for _ in 0..4 {
        let a = add_unit(&mut g, PieceType::Army, Owner::Player, home);
        g.pieces[a].activity = UnitActivity::Sentry;
    }
    assert_eq!(g.cities.iter().filter(|c| c.owner == Owner::Comp).count(), 1);
    assert_eq!(g.cities[keep].owner, Owner::Comp);
    g
}

#[test]
fn the_computer_concedes_and_its_map_can_be_viewed() {
    let g = computer_beaten();
    assert!(g.comp_should_resign());
    let (mut s, ui) = session(g, 120, 40, typed("yv"));
    assert!(!play(&mut s));
    assert!(s.game.comp_resigned);
    assert!(s.view.show_comp_view);
    assert!(ui.screen().text().contains("THE ENEMY'S MAP"));
    assert!(ui.screen().prompt.contains("[V]iew enemy map"));
}

#[test]
fn accepting_the_concession_ends_the_game() {
    let (mut s, ui) = session(computer_beaten(), 120, 40, typed("nx"));
    assert!(play(&mut s), "any key leaves the game-over screen");
    assert!(s.game.messages.iter().any(|m| m.contains("VICTORY")));
    assert!(ui.screen().prompt.contains("GAME OVER"));
}

#[test]
fn wiping_out_the_enemy_wins() {
    let mut g = world(SEED);
    let c = comp_city(&g);
    g.cities[c].owner = Owner::Neutral;
    g.pieces.retain(|p| p.owner != Owner::Comp);
    let (mut s, ui) = session(g, 120, 40, typed("x"));
    assert!(play(&mut s));
    assert!(s.game.messages.iter().any(|m| m.contains("eradicated")));
    assert!(ui.screen().prompt.contains("GAME OVER"));
}

#[test]
fn autosave_writes_on_schedule() {
    let g = world(SEED);
    let (mut s, _) = session(g, 120, 40, typed("nn"));
    let _ = std::fs::remove_file(&s.autosave_file);
    match app::play(&mut s, 2) {
        Err(e) if is_script_end(&e) => {}
        other => panic!("{:?}", other.err()),
    }
    assert!(std::path::Path::new(&s.autosave_file).exists());
    assert!(s.game.messages.iter().any(|m| m.contains("Autosaved game at turn 2")));
}

#[test]
fn w_on_a_transport_unloads_its_armies() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let ((dr, dc), _) = neighbour(&g, home, Terrain::Sea).expect("the capital is a port");
    let sea = g.step(home, (dr, dc)).unwrap();
    let t = add_unit(&mut g, PieceType::Transport, Owner::Player, sea);
    let a = add_unit(&mut g, PieceType::Army, Owner::Player, sea);
    let mut army = g.pieces.remove(a);
    army.activity = UnitActivity::Sentry;
    g.pieces[t].cargo.push(army);
    let prompt = app::unit_prompt(&g.pieces[t]);
    assert!(!prompt.contains("ject"), "no separate eject key: {}", prompt);
    assert!(prompt.contains("[W]ake"));

    let (mut s, ui) = session(g, 120, 40, typed("w"));
    assert!(!play(&mut s));
    assert!(s.game.pieces[t].cargo.is_empty());
    assert!(s.game.messages.iter().any(|m| m.contains("1 army is up on deck")), "{:?}", s.game.messages.last());
    assert!(ui.screen().prompt.contains("[Transport at") || ui.screen().prompt.contains("[Army at"), "{}", ui.screen().prompt);
}

#[test]
fn fighter_fuel_warning_can_call_off_a_move() {
    let mut g = world(SEED);
    let home = g.cities[player_city(&g)].loc;
    let f = add_unit(&mut g, PieceType::Fighter, Owner::Player, home);
    let id = g.pieces[f].id;
    // one square of fuel left: any step away from base is a crash
    g.pieces[f].fuel = 1;
    let (_, k) = neighbour(&g, home, Terrain::Land).or_else(|| neighbour(&g, home, Terrain::Sea)).unwrap();
    let (mut s, ui) = session(g, 120, 40, typed(&format!("{}n", k)));
    assert!(!play(&mut s));
    assert!(s.game.messages.iter().any(|m| m.contains("cancelled due to fuel warning")));
    assert!(s.game.pieces.iter().any(|p| p.id == id && p.loc == home));
    assert!(ui.screen().prompt.contains("[Fighter at"));
}

// ---------------------------------------------------------------------------------
// 3. Fuzz tests
// ---------------------------------------------------------------------------------

/// A random input, weighted towards what players actually press.
fn random_step(rng: &mut rand::rngs::StdRng) -> Step {
    // `q` is left out so games run long enough to reach late-game prompts; Ctrl-C (below)
    // still exercises quitting.
    const KEYS: &str = "123456789123456789123456789 nnnnsgdvrwtlfuxaemoyiplwy*`~zZNSGDVRWT";
    match rng.gen_range(0..100) {
        0..=69 => {
            let c = KEYS.chars().nth(rng.gen_range(0..KEYS.chars().count())).unwrap();
            key(KeyCode::Char(c))
        }
        70..=79 => key([KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right][rng.gen_range(0..4)]),
        80..=84 => key(KeyCode::Enter),
        85..=88 => key(KeyCode::Esc),
        89 => key(KeyCode::Backspace),
        90 => ctrl_c(),
        91..=95 => {
            let kind = [
                MouseEventKind::ScrollUp,
                MouseEventKind::ScrollDown,
                MouseEventKind::Down(MouseButton::Left),
                MouseEventKind::Drag(MouseButton::Left),
                MouseEventKind::Up(MouseButton::Left),
            ][rng.gen_range(0..5)];
            mouse(kind, rng.gen_range(0..200), rng.gen_range(0..70))
        }
        _ => Step::Wait(rng.gen_range(1..4)),
    }
}

/// What a fuzz run reached.
struct FuzzReport {
    games: usize,
    max_turn: i32,
    prompts: std::collections::BTreeSet<String>,
}

/// Plays `events` random inputs into a new game from seed `seed` on a `width` x
/// `height` screen, restarting when the game ends. Checks after each game that the state
/// is consistent and survives a save and a load.
fn fuzz(seed: u64, events: usize, width: u16, height: u16) -> FuzzReport {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let steps: Vec<Step> = (0..events).map(|_| random_step(&mut rng)).collect();
    let ui = ScriptUi::new(width, height, typed("0\r0\r"));
    // startup: answer the questions, then the random inputs start at the first prompt
    ui.0.borrow_mut().steps.extend(steps);
    let mut report = FuzzReport { games: 1, max_turn: 0, prompts: Default::default() };
    let Some((mut s, _)) = app::start_session(new_game_opts(seed), Box::new(ui.clone())).unwrap() else { return report };
    let save = std::env::temp_dir().join(format!("rs_empire_fuzz_{}_{}.sav", std::process::id(), seed));
    s.save_file = save.to_string_lossy().into_owned();
    s.autosave_file = s.save_file.clone();
    loop {
        let ended = match app::play(&mut s, 7) {
            Ok(()) => true,
            Err(e) if is_script_end(&e) => false,
            Err(e) => panic!("seed {}: the front end failed: {}", seed, e),
        };
        s.game.validate().unwrap_or_else(|e| panic!("seed {}: inconsistent game: {}", seed, e));
        assert!(s.view.cursor.r < s.game.rows && s.view.cursor.c < s.game.cols, "seed {}: cursor off the map", seed);
        s.game.save_game(&s.save_file).unwrap();
        let back = Game::load_game(&s.save_file).unwrap_or_else(|e| panic!("seed {}: save does not load: {}", seed, e));
        assert_eq!(back.turn, s.game.turn);
        report.max_turn = report.max_turn.max(s.game.turn);
        if !ended || ui.remaining() == 0 {
            break;
        }
        // the player quit or the game ended: carry on in a fresh session
        s.game = world(seed.wrapping_add(s.game.turn as u64));
        s.view = View::default();
        report.games += 1;
    }
    let _ = std::fs::remove_file(save);
    report.prompts = ui.screen().prompts_seen.clone();
    report
}

/// Runs `f` on its own thread and fails if it takes longer than `limit` (a hang).
fn within(limit: Duration, name: &str, f: impl FnOnce() + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    match rx.recv_timeout(limit) {
        Ok(()) => handle.join().unwrap(),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            if let Err(panic) = handle.join() {
                std::panic::resume_unwind(panic);
            }
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{} did not finish within {:?}: probable hang", name, limit),
    }
}

/// A quick fuzz for every `cargo test`; `fuzz_long` is the thorough one.
#[test]
fn fuzz_random_input_never_crashes_or_hangs() {
    for seed in 1..=2u64 {
        within(Duration::from_secs(120), &format!("fuzz seed {}", seed), move || {
            fuzz(seed, 600, 120, 40);
        });
    }
}

#[test]
fn fuzz_random_input_on_small_and_odd_screens() {
    for (seed, (w, h)) in [(11u64, (80u16, 24u16)), (12, (40, 12)), (13, (200, 60))] {
        within(Duration::from_secs(120), &format!("fuzz seed {}", seed), move || {
            fuzz(seed, 300, w, h);
        });
    }
}

/// The long run: `cargo test --release -- --ignored fuzz_long`. Set RS_EMPIRE_FUZZ_SEEDS
/// and RS_EMPIRE_FUZZ_EVENTS to change its size.
#[test]
#[ignore]
fn fuzz_long() {
    let seeds: u64 = std::env::var("RS_EMPIRE_FUZZ_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
    let events: usize = std::env::var("RS_EMPIRE_FUZZ_EVENTS").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000);
    let all = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
    for seed in 100..100 + seeds {
        let all = all.clone();
        within(Duration::from_secs(1200), &format!("fuzz seed {}", seed), move || {
            let r = fuzz(seed, events, 120, 40);
            println!("seed {}: {} game(s), reached turn {}, {} kinds of prompt", seed, r.games, r.max_turn, r.prompts.len());
            all.lock().unwrap().extend(r.prompts);
        });
    }
    println!("prompts reached across all seeds:");
    for p in all.lock().unwrap().iter() {
        println!("  {}", p);
    }
}
