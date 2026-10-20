mod models;
mod game;
mod disp;
mod game_tests;

use models::{PieceType, GameMode, Owner, Loc, UnitActivity, MoveResult};
use game::Game;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers, EnableMouseCapture, DisableMouseCapture, MouseEventKind},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{self, Write};
use std::time::{Instant, Duration};

fn prompt_quit_confirmation(game: &Game) -> bool {
    let prompt = "Really quit game? [Y/N]: ";
    disp::refresh_display_with_input(game, prompt);
    loop {
        if let Ok(Event::Key(k)) = event::read() {
            if let KeyCode::Char(c) = k.code {
                match c.to_ascii_lowercase() {
                    'y' => return true,
                    'n' => return false,
                    _ => {}
                }
            }
        }
    }
}

fn prompt_city_production(game: &mut Game, city_idx: usize) {
    game.viewport_center = None;
    let city_loc = game.cities[city_idx].loc;
    let prompt_msg = format!(
        "New City at ({},{}) Build? [A]rmy, [T]rns, [P]tBoat, [D]est, [S]ub, [C]arr, [B]ttle, [F]ght, [Z]Sat: ",
        city_loc.r, city_loc.c
    );

    let blink_start = Instant::now();

    loop {
        let elapsed = blink_start.elapsed().as_millis();
        let show_city = (elapsed / 300) % 2 == 0;
        
        disp::refresh_display_with_input_blinking(game, &prompt_msg, Some((city_loc, show_city)));

        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Esc {
                    game.log(format!("Production unchanged for City at ({},{}).", city_loc.r, city_loc.c));
                    break;
                }
                if let KeyCode::Char(c) = key.code {
                    let choice = c.to_ascii_lowercase();
                    let requested_prod = match choice {
                        'a' => Some(PieceType::Army),
                        't' => Some(PieceType::Transport),
                        'p' => Some(PieceType::PatrolBoat),
                        'd' => Some(PieceType::Destroyer),
                        's' => Some(PieceType::Submarine),
                        'c' => Some(PieceType::Carrier),
                        'b' => Some(PieceType::Battleship),
                        'f' => Some(PieceType::Fighter),
                        'z' | 'l' => Some(PieceType::Satellite),
                        _ => None,
                    };

                    if let Some(prod) = requested_prod {
                        let coastal = game.is_coastal(city_loc);
                        if prod.is_naval() && !coastal {
                            game.log(format!("City at ({},{}) is landlocked! Cannot build {:?}. Try again.", city_loc.r, city_loc.c, prod));
                        } else {
                            game.cities[city_idx].prod = prod;
                            game.log(format!("City at ({},{}) assigned to build: {:?}", city_loc.r, city_loc.c, prod));
                            break;
                        }
                    } else {
                        game.log(format!("Invalid choice '{}'. Press a valid unit key.", c));
                    }
                }
            }
        }
    }
}

use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_headless_rig() {
    println!("Starting headless playtest rig for 1000 games across CPU threads...");
    let p_wins = Arc::new(AtomicUsize::new(0));
    let c_wins = Arc::new(AtomicUsize::new(0));
    let draws = Arc::new(AtomicUsize::new(0));
    let current_game = Arc::new(AtomicUsize::new(1));
    let completed_games = Arc::new(AtomicUsize::new(0));

    let file = std::fs::File::create("playtest_metrics.csv").expect("Failed to create metrics file");
    let file_mutex = Arc::new(Mutex::new(file));
    
    {
        let mut f = file_mutex.lock().unwrap();
        let _ = writeln!(f, "Game,Winner,Turns,ElapsedSec");
    }

    let num_threads = std::thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4)).unwrap_or(4);
    let thread_starts = Arc::new((0..num_threads).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let p_wins = Arc::clone(&p_wins);
        let c_wins = Arc::clone(&c_wins);
        let draws = Arc::clone(&draws);
        let current_game = Arc::clone(&current_game);
        let completed_games = Arc::clone(&completed_games);
        let file_mutex = Arc::clone(&file_mutex);
        let thread_starts = Arc::clone(&thread_starts);

        handles.push(thread::spawn(move || {
            loop {
                let i = current_game.fetch_add(1, Ordering::Relaxed);
                if i > 100 {
                    thread_starts[thread_id].store(0, Ordering::Relaxed);
                    break;
                }

                let start_sys = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
                thread_starts[thread_id].store(start_sys, Ordering::Relaxed);
                let start_inst = Instant::now();

                let mut game;
                loop {
                    game = Game::new(30, 30, 12);
                    game.setup_initial_states();
                    
                    if game.cities.iter().any(|c| c.owner == Owner::Player) {
                        break;
                    }
                }
                
                game.default_orders.clear();
                let types = [PieceType::Army, PieceType::Fighter, PieceType::PatrolBoat, PieceType::Destroyer, PieceType::Submarine, PieceType::Carrier, PieceType::Battleship, PieceType::Transport, PieceType::Satellite];
                for &ptype in &types {
                    game.default_orders.push((ptype, UnitActivity::Attack));
                }
                
                let mut is_draw = false;

                let winner = loop {
                    for p in game.pieces.iter_mut() {
                        if p.owner == Owner::Player && p.activity == UnitActivity::Idle {
                            p.activity = UnitActivity::Attack;
                        }
                    }

                    game.process_turn();

                    if let Some(msg) = game.check_game_over() {
                        break if msg.contains("VICTORY") || msg.contains("TOTAL DOMINATION") {
                            p_wins.fetch_add(1, Ordering::Relaxed);
                            "Player"
                        } else {
                            c_wins.fetch_add(1, Ordering::Relaxed);
                            "Comp"
                        };
                    }

                    if game.turn > 3000 {
                        if game.player_score > game.comp_score {
                            p_wins.fetch_add(1, Ordering::Relaxed);
                            break "Player (Score)";
                        } else if game.comp_score > game.player_score {
                            c_wins.fetch_add(1, Ordering::Relaxed);
                            break "Comp (Score)";
                        } else {
                            draws.fetch_add(1, Ordering::Relaxed);
                            is_draw = true;
                            break "Draw";
                        }
                    }
                };

                let elapsed_sec = start_inst.elapsed().as_secs_f32();

                {
                    let mut f = file_mutex.lock().unwrap();
                    let _ = writeln!(f, "{},{},{},{:.2}", i, winner, game.turn, elapsed_sec);
                }

                let mut stdout = io::stdout();
                let _ = write!(stdout, "\x1B[2K\r");
                if is_draw {
                    let _ = writeln!(stdout, "Game {}/100 finished in a Draw (Turn limit). [{:.2}s]", i, elapsed_sec);
                } else {
                    let _ = writeln!(stdout, "Game {}/100 finished in {} turns. Winner: {} [{:.2}s]", i, game.turn, winner, elapsed_sec);
                }
                
                completed_games.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let mut stdout = io::stdout();
    loop {
        let done = completed_games.load(Ordering::Relaxed);
        if done >= 100 {
            break;
        }

        let now_sys = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let mut status_line = String::new();
        
        for (id, start_time) in thread_starts.iter().enumerate() {
            let st = start_time.load(Ordering::Relaxed);
            if st > 0 && now_sys >= st {
                let elapsed = (now_sys - st) as f32 / 1000.0;
                status_line.push_str(&format!("[T{}: {:.1}s] ", id, elapsed));
            } else {
                status_line.push_str(&format!("[T{}: IDLE] ", id));
            }
        }

        let _ = write!(stdout, "\x1B[2K\r{}", status_line);
        let _ = stdout.flush();
        thread::sleep(Duration::from_millis(100));
    }

    for handle in handles {
        let _ = handle.join();
    }

    let _ = write!(io::stdout(), "\x1B[2K\r");
    println!("Rig complete. Player Wins: {} | Comp Wins: {} | Draws: {}", 
        p_wins.load(Ordering::Relaxed), 
        c_wins.load(Ordering::Relaxed), 
        draws.load(Ordering::Relaxed)
    );
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--headless") {
        run_headless_rig();
        return Ok(());
    }

    let mut load_file = None;
    if let Some(idx) = args.iter().position(|arg| arg == "--load" || arg == "-l") {
        if idx + 1 < args.len() {
            load_file = Some(args[idx + 1].clone());
        }
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableMouseCapture);
        let _ = terminal::disable_raw_mode();
        let _ = stdout.execute(terminal::LeaveAlternateScreen);
        let _ = stdout.execute(cursor::Show);
        original_hook(panic_info);
    }));

    let mut stdout = io::stdout();
    stdout.execute(terminal::EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    stdout.execute(EnableMouseCapture)?;
    stdout.execute(cursor::Hide)?;
    stdout.execute(terminal::Clear(ClearType::All))?;

    let mut preloaded_game = None;
    if let Some(filename) = load_file {
        if let Some(loaded_game) = Game::load_game(&filename) {
            preloaded_game = Some(loaded_game);
        } else {
            let _ = stdout.execute(DisableMouseCapture);
            let _ = terminal::disable_raw_mode();
            let _ = stdout.execute(terminal::LeaveAlternateScreen);
            let _ = stdout.execute(cursor::Show);
            eprintln!("Failed to load save file: {}", filename);
            return Ok(());
        }
    }

    let difficulty: u32 = if preloaded_game.is_none() {
        stdout.execute(cursor::MoveTo(10, 5))?;
        print!("==================================================");
        stdout.execute(cursor::MoveTo(10, 6))?;
        print!(" Welcome to rs empire (Rust Port of VMS-empire)                 ");
        stdout.execute(cursor::MoveTo(10, 7))?;
        print!("==================================================");
        stdout.execute(cursor::MoveTo(10, 9))?;
        print!("Enter difficulty level (1 - 100): ");
        stdout.flush()?;

        let mut difficulty_input = String::new();
        stdout.execute(cursor::Show)?;
        loop {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    let _ = stdout.execute(cursor::Show);
                    let _ = terminal::disable_raw_mode();
                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                    std::process::exit(0);
                }

                match key.code {
                    KeyCode::Char(c) => {
                        if c.is_ascii_digit() {
                            difficulty_input.push(c);
                            print!("{}", c);
                            stdout.flush()?;
                        }
                    }
                    KeyCode::Enter => break,
                    KeyCode::Backspace => {
                        if !difficulty_input.is_empty() {
                            difficulty_input.pop();
                            print!("\x08 \x08");
                            stdout.flush()?;
                        }
                    }
                    _ => {}
                }
            }
        }
        stdout.execute(cursor::Hide)?;
        difficulty_input.trim().parse().unwrap_or(50).clamp(1, 100)
    } else {
        50
    };

    // Prompt for Autosave Frequency
    stdout.execute(terminal::Clear(ClearType::All))?;
    stdout.execute(cursor::MoveTo(10, 5))?;
    print!("==================================================");
    stdout.execute(cursor::MoveTo(10, 6))?;
    print!(" Autosave Configuration                                         ");
    stdout.execute(cursor::MoveTo(10, 7))?;
    print!("==================================================");
    stdout.execute(cursor::MoveTo(10, 9))?;
    print!("Enter autosave interval in turns (0 = Disabled): ");
    stdout.flush()?;

    let mut autosave_input = String::new();
    stdout.execute(cursor::Show)?;
    loop {
        if let Event::Key(key) = event::read()? {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                let _ = stdout.execute(cursor::Show);
                let _ = terminal::disable_raw_mode();
                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                std::process::exit(0);
            }

            match key.code {
                KeyCode::Char(c) => {
                    if c.is_ascii_digit() {
                        autosave_input.push(c);
                        print!("{}", c);
                        stdout.flush()?;
                    }
                }
                KeyCode::Enter => break,
                KeyCode::Backspace => {
                    if !autosave_input.is_empty() {
                        autosave_input.pop();
                        print!("\x08 \x08");
                        stdout.flush()?;
                    }
                }
                _ => {}
            }
        }
    }
    stdout.execute(cursor::Hide)?;

    let autosave_interval: i32 = autosave_input.trim().parse().unwrap_or(0);

    stdout.execute(terminal::Clear(ClearType::All))?;

    let mut game = if let Some(mut loaded_game) = preloaded_game {
        loaded_game.log(String::from("Game loaded from command line."));
        loaded_game
    } else {
        let mut g = Game::new(60, 100, difficulty);
        g.setup_initial_states();

        let starting_player_cities: Vec<usize> = g.cities.iter().enumerate()
            .filter(|(_, ct)| ct.owner == Owner::Player)
            .map(|(i, _)| i)
            .collect();

        for idx in starting_player_cities {
            prompt_city_production(&mut g, idx);
        }
        g
    };

    let mut last_autosaved_turn = 0;
    let action_prompt = "[N]ext, [A]utomove, [M]anual, [S]ave, [L]oad, [Q]uit: ";
    disp::refresh_display_with_input(&game, action_prompt);
    let mut observer_mode = false;

    loop {
        // Handle Configurable Autosave check at turn boundary
        if autosave_interval > 0 && game.turn > 0 && game.turn % autosave_interval == 0 && game.turn != last_autosaved_turn {
            game.save_game("empire_auto.sav");
            game.log(format!("Autosaved game at turn {}.", game.turn));
            last_autosaved_turn = game.turn;
        }

        if let Some(game_over_msg) = game.check_game_over() {
            game.log(game_over_msg.clone());
            disp::refresh_display_with_input(&game, " GAME OVER! Press any key to exit... ");
            
            loop {
                if let Ok(Event::Key(_)) = event::read() {
                    let mut stdout = io::stdout();
                    let _ = stdout.execute(DisableMouseCapture);
                    let _ = stdout.execute(cursor::Show);
                    let _ = terminal::disable_raw_mode();
                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                    return Ok(());
                }
            }
        }

        if let Some(c_idx) = game.pending_cities.pop() {
            if observer_mode {
                game.cities[c_idx].prod = PieceType::Army;
                continue; 
            }
            prompt_city_production(&mut game, c_idx);
            continue; 
        }

        if observer_mode {
            for p in game.pieces.iter_mut() {
                if p.owner == Owner::Player {
                    p.moves_left = 0;
                }
            }
            game.process_turn();
            
            disp::refresh_display_with_input(&game, "[DEV OBSERVER MODE] Computer playing. Press 'O' or 'ESC' to cancel...");
            
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Esc => {
                            observer_mode = false;
                            game.mode = GameMode::Manual;
                            game.log(String::from("Exited Dev Observer Mode."));
                            while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                        }
                        _ => {}
                    }
                }
            }
            continue;
        }

        if game.mode == GameMode::Edit {
            let edit_prompt = "[Arrows/NumPad] Move | [P]rod | [O]rders | [W]ake | e[X]ject | [I]nitial Orders | [E]xit Edit: ";
            disp::refresh_display_with_input(&game, edit_prompt);

            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(ev) = event::read() {
                    if let Event::Mouse(mouse) = ev {
                        match mouse.kind {
                            crossterm::event::MouseEventKind::ScrollUp => game.msg_scroll = game.msg_scroll.saturating_add(1),
                            crossterm::event::MouseEventKind::ScrollDown => game.msg_scroll = game.msg_scroll.saturating_sub(1),
                            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = None;
                            }
                            crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                                let mut current = game.viewport_center.unwrap_or(game.edit_cursor);
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    let dc = mouse.column as i32 - last_c as i32;
                                    let dr = mouse.row as i32 - last_r as i32;
                                    current.c = (current.c as i32 - dc).clamp(0, game.cols as i32 - 1) as usize;
                                    current.r = (current.r as i32 - dr).clamp(0, game.rows as i32 - 1) as usize;
                                    game.viewport_center = Some(current);
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }

                    if let Event::Key(key) = ev {
                        game.viewport_center = None;
                        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                            if prompt_quit_confirmation(&game) {
                                let mut stdout = io::stdout();
                                let _ = stdout.execute(crossterm::event::DisableMouseCapture);
                                let _ = stdout.execute(cursor::Show);
                                let _ = terminal::disable_raw_mode();
                                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                std::process::exit(0);
                            } else {
                                game.log(String::from("Quit cancelled."));
                                continue;
                            }
                        }

                        match key.code {
                            KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Esc => {
                                game.mode = GameMode::Manual;
                                game.log(String::from("Exited Edit Mode."));
                                while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                                    let _ = event::read();
                                }
                            }
                            KeyCode::Char('p') | KeyCode::Char('P') => {
                                if let Some(c_idx) = game.cities.iter().position(|c| c.loc == game.edit_cursor && c.owner == Owner::Player) {
                                    prompt_city_production(&mut game, c_idx);
                                } else {
                                    game.log(String::from("No player-owned city at cursor location."));
                                }
                            }
                            KeyCode::Char('w') | KeyCode::Char('W') => {
                                let prompt = "Wake units at cursor: [A]rmy,[T]rns,[P]tBoat,[D]est,[S]ub,[C]arr,[B]ttle,[F]ght,[Z]Sat or [*]All: ";
                                disp::refresh_display_with_input(&game, prompt);
                                
                                let mut target_ptype = None;
                                let mut wake_all = false;
                                
                                loop {
                                    if let Ok(Event::Key(k)) = event::read() {
                                        if k.code == KeyCode::Esc { break; }
                                        if let KeyCode::Char(c) = k.code {
                                            if c == '*' { wake_all = true; break; }
                                            target_ptype = match c.to_ascii_lowercase() {
                                                'a' => Some(PieceType::Army),
                                                't' => Some(PieceType::Transport),
                                                'p' => Some(PieceType::PatrolBoat),
                                                'd' => Some(PieceType::Destroyer),
                                                's' => Some(PieceType::Submarine),
                                                'c' => Some(PieceType::Carrier),
                                                'b' => Some(PieceType::Battleship),
                                                'f' => Some(PieceType::Fighter),
                                                'z' | 'l' => Some(PieceType::Satellite),
                                                _ => None,
                                            };
                                            if target_ptype.is_some() { break; }
                                        }
                                    }
                                }

                                if wake_all || target_ptype.is_some() {
                                    let mut woke = 0;
                                    let mut cargo_carriers = Vec::new();
                                    for (i, p) in game.pieces.iter_mut().enumerate() {
                                        if p.loc == game.edit_cursor && p.owner == Owner::Player && p.hits > 0 {
                                            if wake_all || Some(p.ptype) == target_ptype {
                                                if p.activity != UnitActivity::Idle {
                                                    p.activity = UnitActivity::Idle;
                                                    woke += 1;
                                                }
                                                if !p.cargo.is_empty() {
                                                    cargo_carriers.push(i);
                                                }
                                            }
                                        }
                                    }
                                    for i in cargo_carriers {
                                        game.wake_cargo(i);
                                    }
                                    if woke > 0 {
                                        game.log(format!("Awakened {} unit(s) at cursor.", woke));
                                    } else {
                                        game.log(String::from("No resting player units of that type at cursor."));
                                    }
                                } else {
                                    game.log(String::from("Wake cancelled."));
                                }
                            }
                            KeyCode::Char('x') | KeyCode::Char('X') => {
                                let mut cargo_carriers = Vec::new();
                                for (i, p) in game.pieces.iter().enumerate() {
                                    if p.loc == game.edit_cursor && p.owner == Owner::Player && p.hits > 0 && !p.cargo.is_empty() {
                                        cargo_carriers.push(i);
                                    }
                                }
                                if cargo_carriers.is_empty() {
                                    game.log(String::from("No transports/carriers with cargo at cursor."));
                                } else {
                                    for i in cargo_carriers {
                                        game.wake_cargo(i);
                                    }
                                }
                            }
                            KeyCode::Char('o') | KeyCode::Char('O') => {
                                if let Some(p_idx) = game.pieces.iter().position(|p| p.loc == game.edit_cursor && p.owner == Owner::Player && p.hits > 0) {
                                    let ptype = game.pieces[p_idx].ptype;
                                    
                                    let mut opts = vec!["[S]entry", "[Y]attack", "[G]rope", "[D]est", "[R]nd", "[V]ctr", "[W]ake"];
                                    if ptype == PieceType::Army { opts.push("[T]rnsWait"); }
                                    if ptype == PieceType::Fighter { opts.push("[L]and"); }
                                    if ptype == PieceType::Transport || ptype == PieceType::Carrier { opts.push("[F]ill"); }
                                    if ptype.is_naval() { opts.push("[U]pgrade"); }
                                    
                                    let prompt = format!("Order {:?} - {}: ", ptype, opts.join(","));
                                    disp::refresh_display_with_input(&game, &prompt);
                                    let mut new_act = None;
                                    loop {
                                        if let Ok(Event::Key(k)) = event::read() {
                                            if k.code == KeyCode::Esc { break; }
                                            if let KeyCode::Char(c) = k.code {
                                                let choice = c.to_ascii_lowercase();
                                                
                                                let valid = match choice {
                                                    's' | 'y' | 'g' | 'd' | 'r' | 'v' | 'w' => true,
                                                    't' => ptype == PieceType::Army,
                                                    'l' => ptype == PieceType::Fighter,
                                                    'f' => ptype == PieceType::Transport || ptype == PieceType::Carrier,
                                                    'u' => ptype.is_naval(),
                                                    _ => false,
                                                };
                                                if !valid { continue; }
                                                if choice == 'v' {
                                                    let dir_prompt = format!("Set march direction for {:?} [Arrows/NumPad] or [Esc] Cancel: ", ptype);
                                                    disp::refresh_display_with_input(&game, &dir_prompt);
                                                    let mut chosen_dir = None;
                                                    loop {
                                                        if let Ok(Event::Key(dk)) = event::read() {
                                                            if dk.code == KeyCode::Esc { break; }
                                                            match dk.code {
                                                                KeyCode::Char('8') | KeyCode::Up => { chosen_dir = Some((-1, 0)); break; }
                                                                KeyCode::Char('2') | KeyCode::Down => { chosen_dir = Some((1, 0)); break; }
                                                                KeyCode::Char('4') | KeyCode::Left => { chosen_dir = Some((0, -1)); break; }
                                                                KeyCode::Char('6') | KeyCode::Right => { chosen_dir = Some((0, 1)); break; }
                                                                KeyCode::Char('7') => { chosen_dir = Some((-1, -1)); break; }
                                                                KeyCode::Char('9') => { chosen_dir = Some((-1, 1)); break; }
                                                                KeyCode::Char('1') => { chosen_dir = Some((1, -1)); break; }
                                                                KeyCode::Char('3') => { chosen_dir = Some((1, 1)); break; }
                                                                _ => {}
                                                            }
                                                        }
                                                    }
                                                    if let Some((dr, dc)) = chosen_dir {
                                                        new_act = Some(UnitActivity::Dir(dr, dc));
                                                        break;
                                                    }
                                                } else if choice == 'd' {
                                                    let dest_prompt = format!("Pick destination for {:?} [Arrows/NumPad] Move | [Enter] Confirm | [Esc] Cancel: ", ptype);
                                                    let mut chosen_dest = None;
                                                    let original_cursor = game.edit_cursor;
                                                    loop {
                                                        disp::refresh_display_with_input(&game, &dest_prompt);
                                                        if let Ok(Event::Key(dk)) = event::read() {
                                                            if dk.code == KeyCode::Esc {
                                                                game.edit_cursor = original_cursor;
                                                                break;
                                                            } else if dk.code == KeyCode::Enter {
                                                                chosen_dest = Some(game.edit_cursor);
                                                                game.edit_cursor = original_cursor;
                                                                break;
                                                            } else {
                                                                match dk.code {
                                                                    KeyCode::Char('8') | KeyCode::Up => game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1),
                                                                    KeyCode::Char('2') | KeyCode::Down => game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1),
                                                                    KeyCode::Char('4') | KeyCode::Left => game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1),
                                                                    KeyCode::Char('6') | KeyCode::Right => game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1),
                                                                    KeyCode::Char('7') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                                    KeyCode::Char('9') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                                    KeyCode::Char('1') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                                    KeyCode::Char('3') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                                    _ => {}
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if let Some(dest) = chosen_dest {
                                                        new_act = Some(UnitActivity::Dest(dest));
                                                        break;
                                                    }
                                                } else {
                                                    new_act = match choice {
                                                        's' => Some(UnitActivity::Sentry),
                                                        'y' => Some(UnitActivity::Attack),
                                                        'g' => Some(UnitActivity::Explore),
                                                        'w' => Some(UnitActivity::Idle),
                                                        'f' => Some(UnitActivity::Fill),
                                                        'l' => Some(UnitActivity::Land),
                                                        'r' => Some(UnitActivity::Random),
                                                        't' => Some(UnitActivity::TransportWait),
                                                        'u' => Some(UnitActivity::Upgrade),
                                                        _ => None,
                                                    };
                                                    if new_act.is_some() { break; }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(act) = new_act {
                                        game.pieces[p_idx].activity = act;
                                        game.log(format!("Order applied manually to {:?} at cursor.", ptype));
                                    } else {
                                        game.log(String::from("Order update cancelled."));
                                    }
                                } else {
                                    game.log(String::from("No player unit at cursor location."));
                                }
                            }
                            KeyCode::Char('i') | KeyCode::Char('I') => {
                                let mut edit_global = false;
                                let mut edit_c_idx = None;

                                if let Some(c_idx) = game.cities.iter().position(|c| c.loc == game.edit_cursor && c.owner == Owner::Player) {
                                    let prompt = "Edit [L]ocal City Order or [G]lobal Fallback? (Esc to cancel): ";
                                    disp::refresh_display_with_input(&game, prompt);
                                    loop {
                                        if let Ok(Event::Key(k)) = event::read() {
                                            if k.code == KeyCode::Esc { break; }
                                            if let KeyCode::Char(c) = k.code {
                                                match c.to_ascii_lowercase() {
                                                    'l' => { edit_c_idx = Some(c_idx); break; }
                                                    'g' => { edit_global = true; break; }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let prompt = "No city here. Edit [G]lobal Fallback? (Y/N): ";
                                    disp::refresh_display_with_input(&game, prompt);
                                    loop {
                                        if let Ok(Event::Key(k)) = event::read() {
                                            if k.code == KeyCode::Esc { break; }
                                            if let KeyCode::Char(c) = k.code {
                                                match c.to_ascii_lowercase() {
                                                    'y' | 'g' => { edit_global = true; break; }
                                                    'n' => { break; }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }

                                if edit_global || edit_c_idx.is_some() {
                                    let mode_str = if edit_global { "GLOBAL" } else { "LOCAL" };
                                    let prompt = format!("{} Order? [A]rmy,[T]rns,[P]tBoat,[D]est,[S]ub,[C]arr,[B]ttle,[F]ght,[Z]Sat: ", mode_str);
                                    disp::refresh_display_with_input(&game, &prompt);
                                    
                                    let mut selected_ptype = None;
                                    loop {
                                        if let Ok(Event::Key(k)) = event::read() {
                                            if k.code == KeyCode::Esc { break; }
                                            if let KeyCode::Char(c) = k.code {
                                                selected_ptype = match c.to_ascii_lowercase() {
                                                    'a' => Some(PieceType::Army),
                                                    't' => Some(PieceType::Transport),
                                                    'p' => Some(PieceType::PatrolBoat),
                                                    'd' => Some(PieceType::Destroyer),
                                                    's' => Some(PieceType::Submarine),
                                                    'c' => Some(PieceType::Carrier),
                                                    'b' => Some(PieceType::Battleship),
                                                    'f' => Some(PieceType::Fighter),
                                                    'z' | 'l' => Some(PieceType::Satellite),
                                                    _ => None,
                                                };
                                                if selected_ptype.is_some() { break; }
                                            }
                                        }
                                    }

                                    if let Some(ptype) = selected_ptype {
                                        let mut opts = vec!["[S]entry", "[Y]attack", "[G]rope", "[D]est", "[R]nd", "[V]ctr", "[W]ake"];
                                        if ptype == PieceType::Army { opts.push("[T]rnsWait"); }
                                        if ptype == PieceType::Fighter { opts.push("[L]and"); }
                                        if ptype == PieceType::Transport || ptype == PieceType::Carrier { opts.push("[F]ill"); }
                                        if ptype.is_naval() { opts.push("[U]pgrade"); }
                                        
                                        let prompt2 = format!("{} Order for {:?} - {}: ", mode_str, ptype, opts.join(","));
                                        disp::refresh_display_with_input(&game, &prompt2);
                                        let mut new_act = None;
                                        loop {
                                            if let Ok(Event::Key(k)) = event::read() {
                                                if k.code == KeyCode::Esc { break; }
                                                if let KeyCode::Char(c) = k.code {
                                                    let choice = c.to_ascii_lowercase();
                                                    
                                                    let valid = match choice {
                                                        's' | 'y' | 'g' | 'd' | 'r' | 'v' | 'w' => true,
                                                        't' => ptype == PieceType::Army,
                                                        'l' => ptype == PieceType::Fighter,
                                                        'f' => ptype == PieceType::Transport || ptype == PieceType::Carrier,
                                                        'u' => ptype.is_naval(),
                                                        _ => false,
                                                    };
                                                    if !valid { continue; }
                                                    if choice == 'v' {
                                                        let dir_prompt = format!("Set march direction for {:?} [Arrows/NumPad] or [Esc] Cancel: ", ptype);
                                                        disp::refresh_display_with_input(&game, &dir_prompt);
                                                        let mut chosen_dir = None;
                                                        loop {
                                                            if let Ok(Event::Key(dk)) = event::read() {
                                                                if dk.code == KeyCode::Esc { break; }
                                                                match dk.code {
                                                                    KeyCode::Char('8') | KeyCode::Up => { chosen_dir = Some((-1, 0)); break; }
                                                                    KeyCode::Char('2') | KeyCode::Down => { chosen_dir = Some((1, 0)); break; }
                                                                    KeyCode::Char('4') | KeyCode::Left => { chosen_dir = Some((0, -1)); break; }
                                                                    KeyCode::Char('6') | KeyCode::Right => { chosen_dir = Some((0, 1)); break; }
                                                                    KeyCode::Char('7') => { chosen_dir = Some((-1, -1)); break; }
                                                                    KeyCode::Char('9') => { chosen_dir = Some((-1, 1)); break; }
                                                                    KeyCode::Char('1') => { chosen_dir = Some((1, -1)); break; }
                                                                    KeyCode::Char('3') => { chosen_dir = Some((1, 1)); break; }
                                                                    _ => {}
                                                                }
                                                            }
                                                        }
                                                        if let Some((dr, dc)) = chosen_dir {
                                                            new_act = Some(UnitActivity::Dir(dr, dc));
                                                            break;
                                                        }
                                                    } else if choice == 'd' {
                                                        let dest_prompt = format!("Pick destination for {:?} [Arrows/NumPad] Move | [Enter] Confirm | [Esc] Cancel: ", ptype);
                                                        let mut chosen_dest = None;
                                                        let original_cursor = game.edit_cursor;
                                                        loop {
                                                            disp::refresh_display_with_input(&game, &dest_prompt);
                                                            if let Ok(Event::Key(dk)) = event::read() {
                                                                if dk.code == KeyCode::Esc {
                                                                    game.edit_cursor = original_cursor;
                                                                    break;
                                                                } else if dk.code == KeyCode::Enter {
                                                                    chosen_dest = Some(game.edit_cursor);
                                                                    game.edit_cursor = original_cursor;
                                                                    break;
                                                                } else {
                                                                    match dk.code {
                                                                        KeyCode::Char('8') | KeyCode::Up => game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1),
                                                                        KeyCode::Char('2') | KeyCode::Down => game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1),
                                                                        KeyCode::Char('4') | KeyCode::Left => game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1),
                                                                        KeyCode::Char('6') | KeyCode::Right => game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1),
                                                                        KeyCode::Char('7') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                                        KeyCode::Char('9') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                                        KeyCode::Char('1') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                                        KeyCode::Char('3') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                                        _ => {}
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if let Some(dest) = chosen_dest {
                                                            new_act = Some(UnitActivity::Dest(dest));
                                                            break;
                                                        }
                                                    } else {
                                                        new_act = match choice {
                                                            's' => Some(UnitActivity::Sentry),
                                                            'y' => Some(UnitActivity::Attack),
                                                            'g' => Some(UnitActivity::Explore),
                                                            'w' => Some(UnitActivity::Idle),
                                                            'f' => Some(UnitActivity::Fill),
                                                            'l' => Some(UnitActivity::Land),
                                                            'r' => Some(UnitActivity::Random),
                                                            't' => Some(UnitActivity::TransportWait),
                                                            'u' => Some(UnitActivity::Upgrade),
                                                            _ => None,
                                                        };
                                                        if new_act.is_some() { break; }
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(act) = new_act {
                                            if edit_global {
                                                if let Some(idx) = game.default_orders.iter().position(|(p, _)| *p == ptype) {
                                                    game.default_orders[idx].1 = act;
                                                } else {
                                                    game.default_orders.push((ptype, act));
                                                }
                                                
                                                let mut updated_indices = Vec::new();
                                                for city in &game.cities {
                                                    if city.owner == Owner::Player {
                                                        if !city.default_orders.iter().any(|(p, _)| *p == ptype) {
                                                            let c_loc = city.loc;
                                                            for (i, p) in game.pieces.iter_mut().enumerate() {
                                                                let is_awaiting = matches!(p.activity, UnitActivity::Idle | UnitActivity::Explore | UnitActivity::Attack | UnitActivity::Random | UnitActivity::Dir(_, _) | UnitActivity::Dest(_));
                                                                if p.loc == c_loc && p.owner == Owner::Player && p.ptype == ptype && p.hits > 0 && is_awaiting {
                                                                    p.activity = act;
                                                                    updated_indices.push(i);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                game.log(format!("GLOBAL default order for {:?} set to {:?} (Applied to {} units).", ptype, act, updated_indices.len()));
                                                
                                                for idx in updated_indices {
                                                    match act {
                                                        UnitActivity::Explore => game.explore_piece(idx),
                                                        UnitActivity::Attack => game.attack_piece(idx),
                                                        UnitActivity::Random => game.random_piece(idx),
                                                        UnitActivity::Dest(_) => game.dest_piece(idx),
                                                        UnitActivity::Dir(_, _) => game.dir_piece(idx),
                                                        UnitActivity::Fill => game.process_fill_mode(idx),
                                                        UnitActivity::Upgrade => game.process_upgrade_mode(idx),
                                                        UnitActivity::Land => game.process_land_mode(idx),
                                                        _ => {}
                                                    }
                                                }
                                                game.pieces.retain(|p| p.hits > 0);
                                            } else if let Some(c_idx) = edit_c_idx {
                                                if let Some(idx) = game.cities[c_idx].default_orders.iter().position(|(p, _)| *p == ptype) {
                                                    game.cities[c_idx].default_orders[idx].1 = act;
                                                } else {
                                                    game.cities[c_idx].default_orders.push((ptype, act));
                                                }
                                                let mut updated_indices = Vec::new();
                                                let city_loc = game.cities[c_idx].loc;
                                                for (i, p) in game.pieces.iter_mut().enumerate() {
                                                    let is_awaiting = matches!(p.activity, UnitActivity::Idle | UnitActivity::Explore | UnitActivity::Attack | UnitActivity::Random | UnitActivity::Dir(_, _) | UnitActivity::Dest(_));
                                                    if p.loc == city_loc && p.owner == Owner::Player && p.ptype == ptype && p.hits > 0 && is_awaiting {
                                                        p.activity = act;
                                                        updated_indices.push(i);
                                                    }
                                                }
                                                game.log(format!("LOCAL City({},{}) order for {:?} set to {:?} (Applied to {} stacked units).", game.cities[c_idx].loc.r, game.cities[c_idx].loc.c, ptype, act, updated_indices.len()));
                                                
                                                for idx in updated_indices {
                                                    match act {
                                                        UnitActivity::Explore => game.explore_piece(idx),
                                                        UnitActivity::Attack => game.attack_piece(idx),
                                                        UnitActivity::Random => game.random_piece(idx),
                                                        UnitActivity::Dest(_) => game.dest_piece(idx),
                                                        UnitActivity::Dir(_, _) => game.dir_piece(idx),
                                                        UnitActivity::Fill => game.process_fill_mode(idx),
                                                        UnitActivity::Upgrade => game.process_upgrade_mode(idx),
                                                        UnitActivity::Land => game.process_land_mode(idx),
                                                        _ => {}
                                                    }
                                                }
                                                game.pieces.retain(|p| p.hits > 0);
                                            }
                                        } else {
                                            game.log(String::from("Order update cancelled."));
                                        }
                                    } else {
                                        game.log(String::from("Order update cancelled."));
                                    }
                                } else {
                                    game.log(String::from("Order update cancelled."));
                                }
                            }
                            KeyCode::Char('8') | KeyCode::Up => game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1),
                            KeyCode::Char('2') | KeyCode::Down => game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1),
                            KeyCode::Char('4') | KeyCode::Left => game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1),
                            KeyCode::Char('6') | KeyCode::Right => game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1),
                            KeyCode::Char('7') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                            KeyCode::Char('9') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                            KeyCode::Char('1') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                            KeyCode::Char('3') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                            _ => {}
                        }
                    }
                }
            }
            continue; 
        }

        let mut cleanup_needed = false;
        for i in 0..game.pieces.len() {
            if game.pieces[i].hits > 0 && game.pieces[i].owner == Owner::Player {
                if game.pieces[i].activity == UnitActivity::Fill {
                    game.process_fill_mode(i);
                } else if game.pieces[i].activity == UnitActivity::TransportWait {
                    if game.process_transport_wait(i) {
                        cleanup_needed = true;
                    }
                }
            }
        }
        if cleanup_needed {
            game.pieces.retain(|p| p.hits > 0);
        }

        // Defines any unit that needs manual input (Idle, or automated but stuck with moves remaining)
        let is_awaiting_manual = |p: &models::Piece| {
            if p.owner != Owner::Player || p.moves_left <= 0 || p.hits <= 0 { return false; }
            matches!(p.activity, UnitActivity::Idle | UnitActivity::Explore | UnitActivity::Attack | UnitActivity::Random | UnitActivity::Dir(_, _) | UnitActivity::Dest(_) | UnitActivity::Land)
        };

        let mut was_auto = game.mode == GameMode::Auto;

        if was_auto {
            let idle_units = game.pieces.iter().filter(|&p| is_awaiting_manual(p)).count();

            if idle_units > 0 {
                game.mode = GameMode::Manual;
            } else {
                let player_moves_left = game.pieces.iter().filter(|&p| is_awaiting_manual(p)).count();
                
                if player_moves_left == 0 {
                    game.process_turn();
                    disp::refresh_display_with_input(&game, action_prompt);
                    std::thread::sleep(Duration::from_millis(150));
                }

                if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                    let ev = event::read().unwrap();
                    if let Event::Mouse(mouse) = ev {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => game.msg_scroll = game.msg_scroll.saturating_add(1),
                            MouseEventKind::ScrollDown => game.msg_scroll = game.msg_scroll.saturating_sub(1),
                            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = None;
                            }
                            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                                let loc = game.cities.iter().find(|c| c.owner == Owner::Player).map(|c| c.loc).unwrap_or(Loc { r: game.rows / 2, c: game.cols / 2 });
                                let mut current = game.viewport_center.unwrap_or(loc);
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    let dc = mouse.column as i32 - last_c as i32;
                                    let dr = mouse.row as i32 - last_r as i32;
                                    current.c = (current.c as i32 - dc).clamp(0, game.cols as i32 - 1) as usize;
                                    current.r = (current.r as i32 - dr).clamp(0, game.rows as i32 - 1) as usize;
                                    game.viewport_center = Some(current);
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }
                    if let Event::Key(key) = ev {
                        game.viewport_center = None;
                        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                            if prompt_quit_confirmation(&game) {
                                let mut stdout = io::stdout();
                                let _ = stdout.execute(DisableMouseCapture); 
                                let _ = stdout.execute(cursor::Show);
                                let _ = terminal::disable_raw_mode();
                                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                return Ok(());
                            } else {
                                game.log(String::from("Quit cancelled."));
                                continue;
                            }
                        }
                        if let KeyCode::Char(ch) = key.code {
                            match ch.to_ascii_lowercase() {
                                'q' => {
                                    if prompt_quit_confirmation(&game) {
                                        let mut stdout = io::stdout();
                                        let _ = stdout.execute(DisableMouseCapture);
                                        let _ = stdout.execute(cursor::Show);
                                        let _ = terminal::disable_raw_mode();
                                        let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                        return Ok(());
                                    } else {
                                        game.log(String::from("Quit cancelled."));
                                        continue;
                                    }
                                }
                                'o' | 'm' => {
                                    game.mode = GameMode::Manual;
                                    game.log(String::from("Manual move mode engaged (Auto interrupted)."));
                                }
                                'n' => game.process_turn(),
                                _ => {}
                            }
                        }
                    }
                }
                continue;
            }
        }

        let active_piece_id = game.pieces.iter()
            .find(|&p| is_awaiting_manual(p))
            .map(|p| p.id);

        if let Some(pid) = active_piece_id {
            if let Some(piece_idx) = game.pieces.iter().position(|p| p.id == pid) {
                let ptype = game.pieces[piece_idx].ptype;
                let loc = game.pieces[piece_idx].loc;
                let moves = game.pieces[piece_idx].moves_left;

                let mut opts = vec!["[789/46/123] move", "[S]entry", "[Y]attack", "[G]rope", "[D]est", "[R]nd", "[V]ctr", "[W]ake"];
                if ptype == PieceType::Army { opts.push("[T]rnsWait"); }
                if ptype == PieceType::Fighter { opts.push("[L]and"); }
                if ptype == PieceType::Transport || ptype == PieceType::Carrier { 
                    opts.push("[F]ill");
                    opts.push("e[X]ject");
                }
                if ptype.is_naval() { opts.push("[U]pgrade"); }
                
                let sticky_str = match game.pieces[piece_idx].activity {
                    UnitActivity::Idle => String::new(),
                    UnitActivity::Dir(_, _) => " [STUCK: Dir]".to_string(),
                    UnitActivity::Dest(_) => " [STUCK: Dest]".to_string(),
                    UnitActivity::Explore => " [STUCK: Grope]".to_string(),
                    UnitActivity::Attack => " [STUCK: Attack]".to_string(),
                    UnitActivity::Random => " [STUCK: Random]".to_string(),
                    UnitActivity::Land => " [STUCK: Land]".to_string(),
                    _ => String::new(),
                };
                
                let move_prompt = format!("[{:?} at ({},{}) M:{}{}]: {}: ", ptype, loc.r, loc.c, moves, sticky_str, opts.join(","));

                let blink_start = Instant::now();

                loop {
                    let elapsed = blink_start.elapsed().as_millis();
                    let show_unit = (elapsed / 300) % 2 == 0;

                    disp::refresh_display_with_input_blinking(&game, &move_prompt, Some((loc, show_unit)));

                    if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                    let ev = event::read().unwrap();
                    if let Event::Mouse(mouse) = ev {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => game.msg_scroll = game.msg_scroll.saturating_add(1),
                            MouseEventKind::ScrollDown => game.msg_scroll = game.msg_scroll.saturating_sub(1),
                            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                                game.last_mouse_coord = None;
                            }
                            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                                let mut current = game.viewport_center.unwrap_or(loc);
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    let dc = mouse.column as i32 - last_c as i32;
                                    let dr = mouse.row as i32 - last_r as i32;
                                    current.c = (current.c as i32 - dc).clamp(0, game.cols as i32 - 1) as usize;
                                    current.r = (current.r as i32 - dr).clamp(0, game.rows as i32 - 1) as usize;
                                    game.viewport_center = Some(current);
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }
                    if let Event::Key(key) = ev {
                            game.viewport_center = None;
                            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                                if prompt_quit_confirmation(&game) {
                                    let mut stdout = io::stdout();
                                    let _ = stdout.execute(DisableMouseCapture);
                                    let _ = stdout.execute(cursor::Show);
                                    let _ = terminal::disable_raw_mode();
                                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                    return Ok(());
                                } else {
                                    game.log(String::from("Quit cancelled."));
                                    continue;
                                }
                            }

                            if let KeyCode::Char(ch) = key.code {
                                match ch {
                                    ' ' | 'n' | 'N' | 's' | 'S' | 't' | 'T' | 'w' | 'W' => {
                                        let is_sentry = ch == 's' || ch == 'S';
                                        let is_twait = ch == 't' || ch == 'T';
                                        let is_wake = ch == 'w' || ch == 'W';
                                        let mut proceed = true;
                                        let mut attack_own_city = false;

                                        if ptype == PieceType::Army && game.cities.iter().any(|c| c.loc == loc && c.owner == Owner::Player) {
                                            let prompt = format!("Army must move out of city! Attack own city at ({},{})? [Y/N]: ", loc.r, loc.c);
                                            disp::refresh_display_with_input(&game, &prompt);
                                            loop {
                                                if let Ok(Event::Key(k)) = event::read() {
                                                    if let KeyCode::Char(c) = k.code {
                                                        match c.to_ascii_lowercase() {
                                                            'y' => { attack_own_city = true; break; }
                                                            'n' => { proceed = false; break; }
                                                            _ => {}
                                                        }
                                                    }
                                                }
                                            }
                                            while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                                        }

                                        if !proceed {
                                            continue;
                                        }

                                        if attack_own_city {
                                            let (_result, msg) = game.attempt_move(piece_idx, loc);
                                            game.log(msg);
                                            break;
                                        }

                                        if !is_wake {
                                            game.pieces[piece_idx].moves_left = 0;
                                        }
                                        
                                        if is_sentry {
                                            let mut boarded = false;
                                            // If sentried while stacked on a transport, go below deck into cargo
                                            if ptype == PieceType::Army || ptype == PieceType::Fighter {
                                                let (res, msg) = game.attempt_move(piece_idx, loc);
                                                if res == MoveResult::Moved && msg.contains("boarded") {
                                                    game.log(msg);
                                                    boarded = true;
                                                }
                                            }
                                            if !boarded {
                                                game.pieces[piece_idx].activity = UnitActivity::Sentry;
                                                game.log(format!("{:?}({}:{}) put on sentry duty.", ptype, loc.r, loc.c));
                                            }
                                        } else if is_twait {
                                            game.pieces[piece_idx].activity = UnitActivity::TransportWait;
                                            game.log(String::from("Unit set to Transport Wait."));
                                        } else if is_wake {
                                            game.pieces[piece_idx].activity = UnitActivity::Idle;
                                            game.log(format!("{:?} woken up.", ptype));
                                            if ptype == PieceType::Transport || ptype == PieceType::Carrier {
                                                game.wake_cargo(piece_idx);
                                            }
                                        } else {
                                            game.log(format!("{:?} at ({},{}) skipped turn.", ptype, loc.r, loc.c));
                                        }
                                        break;
                                    }
                                    'x' | 'X' => {
                                        if !game.pieces[piece_idx].cargo.is_empty() {
                                            game.wake_cargo(piece_idx);
                                            break;
                                        } else {
                                            game.log(String::from("No cargo to eject/launch."));
                                        }
                                    }
                                    'y' | 'Y' => {
                                        game.pieces[piece_idx].activity = UnitActivity::Attack;
                                        game.log(format!("{:?} set to attack mode.", ptype));
                                        game.attack_piece(piece_idx);
                                        game.pieces.retain(|p| p.hits > 0);
                                        break;
                                    }
                                    'g' | 'G' => {
                                        game.pieces[piece_idx].activity = UnitActivity::Explore;
                                        game.log(format!("{:?} set to grope/explore mode.", ptype));
                                        game.explore_piece(piece_idx);
                                        break;
                                    }
                                    'd' | 'D' => {
                                        game.mode = GameMode::PickDest(piece_idx);
                                        game.edit_cursor = loc; 
                                        let dest_prompt = format!("Pick target for {:?} - [Arrows/NumPad] Move | [Enter] Confirm | [Esc] Cancel: ", ptype);
                                        
                                        loop {
                                            disp::refresh_display_with_input(&game, &dest_prompt);
                                            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                                                if let Ok(Event::Key(k)) = event::read() {
                                                    if k.code == KeyCode::Esc {
                                                        game.mode = GameMode::Manual;
                                                        game.log(String::from("Destination selection cancelled."));
                                                        break;
                                                    } else if k.code == KeyCode::Enter {
                                                        game.pieces[piece_idx].activity = UnitActivity::Dest(game.edit_cursor);
                                                        game.mode = GameMode::Manual;
                                                        game.log(format!("{:?} ordered to march toward ({},{}).", ptype, game.edit_cursor.r, game.edit_cursor.c));
                                                        game.dest_piece(piece_idx);
                                                        game.pieces.retain(|p| p.hits > 0);
                                                        break;
                                                    } else {
                                                        match k.code {
                                                            KeyCode::Char('8') | KeyCode::Up => game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1),
                                                            KeyCode::Char('2') | KeyCode::Down => game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1),
                                                            KeyCode::Char('4') | KeyCode::Left => game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1),
                                                            KeyCode::Char('6') | KeyCode::Right => game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1),
                                                            KeyCode::Char('7') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                            KeyCode::Char('9') => { game.edit_cursor.r = game.edit_cursor.r.saturating_sub(1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                            KeyCode::Char('1') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_sub(1); }
                                                            KeyCode::Char('3') => { game.edit_cursor.r = game.edit_cursor.r.saturating_add(1).min(game.rows - 1); game.edit_cursor.c = game.edit_cursor.c.saturating_add(1).min(game.cols - 1); }
                                                            _ => {}
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                                        break;
                                    }
                                    'v' | 'V' => {
                                        let prompt = format!("Set march direction for {:?} [Arrows/NumPad] or [Esc] Cancel: ", ptype);
                                        disp::refresh_display_with_input(&game, &prompt);
                                        let mut chosen_dir = None;
                                        loop {
                                            if let Ok(Event::Key(k)) = event::read() {
                                                if k.code == KeyCode::Esc { break; }
                                                match k.code {
                                                    KeyCode::Char('8') | KeyCode::Up => { chosen_dir = Some((-1, 0)); break; }
                                                    KeyCode::Char('2') | KeyCode::Down => { chosen_dir = Some((1, 0)); break; }
                                                    KeyCode::Char('4') | KeyCode::Left => { chosen_dir = Some((0, -1)); break; }
                                                    KeyCode::Char('6') | KeyCode::Right => { chosen_dir = Some((0, 1)); break; }
                                                    KeyCode::Char('7') => { chosen_dir = Some((-1, -1)); break; }
                                                    KeyCode::Char('9') => { chosen_dir = Some((-1, 1)); break; }
                                                    KeyCode::Char('1') => { chosen_dir = Some((1, -1)); break; }
                                                    KeyCode::Char('3') => { chosen_dir = Some((1, 1)); break; }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        if let Some((dr, dc)) = chosen_dir {
                                            game.pieces[piece_idx].activity = UnitActivity::Dir(dr, dc);
                                            let dir_name = match (dr, dc) {
                                                (-1, 0) => "North", (1, 0) => "South", (0, -1) => "West", (0, 1) => "East",
                                                (-1, -1) => "NW", (-1, 1) => "NE", (1, -1) => "SW", (1, 1) => "SE",
                                                _ => "Unknown",
                                            };
                                            game.log(format!("{:?} ordered to march continuously {}.", ptype, dir_name));
                                            game.dir_piece(piece_idx);
                                            game.pieces.retain(|p| p.hits > 0);
                                        } else {
                                            game.log(String::from("March order cancelled."));
                                        }
                                        while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                                        break;
                                    }
                                    'f' | 'F' => { 
                                        if ptype == PieceType::Transport || ptype == PieceType::Carrier {
                                            game.pieces[piece_idx].activity = UnitActivity::Fill; 
                                            game.log(String::from("Unit set to Fill.")); 
                                            game.process_fill_mode(piece_idx);
                                            break; 
                                        }
                                    }
                                    'l' | 'L' => { 
                                        if ptype == PieceType::Fighter {
                                            game.pieces[piece_idx].activity = UnitActivity::Land; 
                                            game.log(String::from("Unit set to Land.")); 
                                            game.process_land_mode(piece_idx);
                                            break; 
                                        }
                                    }
                                    'r' | 'R' => { 
                                        game.pieces[piece_idx].activity = UnitActivity::Random; 
                                        game.log(String::from("Unit set to Random.")); 
                                        game.random_piece(piece_idx);
                                        game.pieces.retain(|p| p.hits > 0);
                                        break; 
                                    }
                                    'u' | 'U' => { 
                                        if ptype.is_naval() {
                                            game.pieces[piece_idx].activity = UnitActivity::Upgrade; 
                                            game.log(String::from("Unit set to Upgrade.")); 
                                            game.process_upgrade_mode(piece_idx);
                                            break; 
                                        }
                                    }
                                    _ => {}
                                }
                                
                                let (dr_val, dc_val, is_move) = match ch {
                                    '7' => (-1, -1, true),
                                    '8' => (-1, 0, true),
                                    '9' => (-1, 1, true),
                                    '4' => (0, -1, true),
                                    '6' => (0, 1, true),
                                    '1' => (1, -1, true),
                                    '2' => (1, 0, true),
                                    '3' => (1, 1, true),
                                    _ => {
                                        match ch.to_ascii_lowercase() {
                                            'q' => {
                                                if prompt_quit_confirmation(&game) {
                                                    let mut stdout = io::stdout();
                                                    let _ = stdout.execute(DisableMouseCapture);
                                                    let _ = stdout.execute(cursor::Show);
                                                    let _ = terminal::disable_raw_mode();
                                                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                                    return Ok(());
                                                } else {
                                                    game.log(String::from("Quit cancelled."));
                                                    break;
                                                }
                                            }
                                            'z' => {
                                                if cfg!(debug_assertions) {
                                                    observer_mode = true;
                                                    game.log(String::from("Dev Observer Mode engaged. Player turns skipped."));
                                                }
                                            }
                                            'a' => {
                                                game.mode = GameMode::Auto;
                                                was_auto = true;
                                                game.log(String::from("Automove mode engaged. Press 'o' or 'm' to cancel."));
                                            }
                                            'o' | 'm' => {
                                                game.mode = GameMode::Manual;
                                                was_auto = false;
                                                game.log(String::from("Manual move mode engaged."));
                                            }
                                            'e' => {
                                                game.mode = GameMode::Edit;
                                                game.edit_cursor = loc;
                                                was_auto = false;
                                                game.log(String::from("Edit Mode engaged."));
                                                
                                                while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                                                    let _ = event::read();
                                                }
                                            }
                                            _ => {}
                                        }
                                        break;
                                    }
                                };

                                if is_move {
                                    let target_r = (loc.r as i32 + dr_val) as usize;
                                    let target_c = (loc.c as i32 + dc_val) as usize;

                                    if target_r < game.rows && target_c < game.cols {
                                        let target_loc = Loc { r: target_r, c: target_c };
                                        
                                        let mut proceed = true;
                                        if ptype == PieceType::Army {
                                            if game.cities.iter().any(|c| c.loc == target_loc && c.owner == Owner::Player) {
                                                let prompt = format!("Really attack your own city at ({},{})? [Y/N]: ", target_loc.r, target_loc.c);
                                                disp::refresh_display_with_input(&game, &prompt);
                                                loop {
                                                    if let Ok(Event::Key(k)) = event::read() {
                                                        if let KeyCode::Char(c) = k.code {
                                                            match c.to_ascii_lowercase() {
                                                                'y' => { break; }
                                                                'n' => { proceed = false; break; }
                                                                _ => {}
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        if !proceed {
                                            game.log(String::from("Friendly city attack cancelled."));
                                            break;
                                        }

                                        if ptype == PieceType::Fighter {
                                            if let Some(dist) = game.dist_to_nearest_base(target_loc) {
                                                let remaining_fuel = game.pieces[piece_idx].fuel - 1;
                                                if remaining_fuel < dist {
                                                    let prompt = format!("WARNING: Move leaves {} fuel, but base is {} away. Crash imminent! Proceed? [Y/N]: ", remaining_fuel, dist);
                                                    disp::refresh_display_with_input(&game, &prompt);
                                                    let mut confirm = false;
                                                    loop {
                                                        if let Ok(Event::Key(k)) = event::read() {
                                                            if let KeyCode::Char(c) = k.code {
                                                                match c.to_ascii_lowercase() {
                                                                    'y' => { confirm = true; break; }
                                                                    'n' => { break; }
                                                                    _ => {}
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if !confirm {
                                                        game.log(String::from("Fighter move cancelled due to fuel warning."));
                                                        while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                                                        break;
                                                    }
                                                }
                                            }
                                        }

                                        let (result, msg) = game.attempt_move(piece_idx, target_loc);
                                        
                                        if !msg.is_empty() {
                                            game.log(msg);
                                        }

                                        if let MoveResult::CityCaptured(c_idx) = result {
                                            prompt_city_production(&mut game, c_idx);
                                        }
                                        
                                    } else {
                                        game.log(String::from("Cannot move off the edge of the world."));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                if was_auto {
                    game.mode = GameMode::Auto;
                }

                continue;
            }
        }

        disp::refresh_display_with_input(&game, action_prompt);

        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            let ev = event::read().unwrap();
            
            if let Event::Mouse(mouse) = ev {
                match mouse.kind {
                    MouseEventKind::ScrollUp => game.msg_scroll = game.msg_scroll.saturating_add(1),
                    MouseEventKind::ScrollDown => game.msg_scroll = game.msg_scroll.saturating_sub(1),
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        game.last_mouse_coord = Some((mouse.column, mouse.row));
                    }
                    MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                        game.last_mouse_coord = None;
                    }
                    MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                        let mut current = game.viewport_center.unwrap_or_else(|| game.cities.iter().find(|c| c.owner == Owner::Player).map(|c| c.loc).unwrap_or(Loc { r: game.rows / 2, c: game.cols / 2 }));
                        if let Some((last_c, last_r)) = game.last_mouse_coord {
                            let dc = mouse.column as i32 - last_c as i32;
                            let dr = mouse.row as i32 - last_r as i32;
                            current.c = (current.c as i32 - dc).clamp(0, game.cols as i32 - 1) as usize;
                            current.r = (current.r as i32 - dr).clamp(0, game.rows as i32 - 1) as usize;
                            game.viewport_center = Some(current);
                        }
                        game.last_mouse_coord = Some((mouse.column, mouse.row));
                    }
                    _ => {}
                }
            }

            if let Event::Key(key) = ev {
                game.viewport_center = None;
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    if prompt_quit_confirmation(&game) {
                        let mut stdout = io::stdout();
                        let _ = stdout.execute(DisableMouseCapture);
                        let _ = stdout.execute(cursor::Show);
                        let _ = terminal::disable_raw_mode();
                        let _ = stdout.execute(terminal::LeaveAlternateScreen);
                        return Ok(());
                    } else {
                        game.log(String::from("Quit cancelled."));
                        continue;
                    }
                }

                if let KeyCode::Char(ch) = key.code {
                    match ch.to_ascii_lowercase() {
                        'q' => {
                            if prompt_quit_confirmation(&game) {
                                let mut stdout = io::stdout();
                                let _ = stdout.execute(DisableMouseCapture);
                                let _ = stdout.execute(cursor::Show);
                                let _ = terminal::disable_raw_mode();
                                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                return Ok(());
                            } else {
                                game.log(String::from("Quit cancelled."));
                                continue;
                            }
                        }
                        'n' => {
                            game.process_turn();
                        }
                        '`' | '~' => {
                            if cfg!(debug_assertions) {
                                game.reveal_entire_map();
                            } else {
                                game.log(format!("Unknown command '{}'.", ch));
                            }
                        }
                        'z' => {
                            if cfg!(debug_assertions) {
                                observer_mode = true;
                                game.log(String::from("Dev Observer Mode engaged. Player turns skipped."));
                            } else {
                                game.log(format!("Unknown command '{}'.", ch));
                            }
                        }
                        'a' => {
                            game.mode = GameMode::Auto;
                            game.log(String::from("Automove mode engaged. Press 'o' or 'm' to cancel."));
                        }
                        'o' | 'm' => {
                            game.mode = GameMode::Manual;
                            game.log(String::from("Manual move mode engaged."));
                        }
                        'e' => {
                            game.mode = GameMode::Edit;
                            game.edit_cursor = game.cities.iter().find(|c| c.owner == Owner::Player).map(|c| c.loc).unwrap_or(Loc { r: game.rows/2, c: game.cols/2 });
                            game.log(String::from("Edit Mode engaged."));
                            
                            while event::poll(Duration::from_millis(0)).unwrap_or(false) {
                                let _ = event::read();
                            }
                        }
                        's' => {
                            game.save_game("empire.sav");
                            game.log(String::from("Game saved to empire.sav."));
                        }
                        'l' => {
                            if let Some(loaded_game) = Game::load_game("empire.sav") {
                                game = loaded_game;
                                game.log(String::from("Game loaded from empire.sav."));
                            } else {
                                game.log(String::from("Failed to load empire.sav."));
                            }
                        }
                        _ => {
                            game.log(format!("Unknown command '{}'.", ch));
                        }
                    }
                }
            }
        }
    }
}
