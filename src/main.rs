mod game;
mod disp;

use game::{Game, PieceType, GameMode, Owner, Loc, UnitActivity, MoveResult};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers, EnableMouseCapture, DisableMouseCapture, MouseEventKind},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{self, Write};
use std::time::{Instant, Duration};

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("empire_debug.log") 
            {
                use std::io::Write;
                let _ = writeln!(file, $($arg)*);
            }
        }
    };
}

fn prompt_city_production(game: &mut Game, city_idx: usize) {
    game.map_offset_r = 0;
    game.map_offset_c = 0;
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

fn main() -> io::Result<()> {
    // Clear previous runtime instrumentation log (Debug builds only)
    #[cfg(debug_assertions)]
    {
        let _ = std::fs::File::create("empire_debug.log");
        if let Ok(mut file) = std::fs::File::create("ai_metrics.csv") {
            use std::io::Write;
            let _ = writeln!(file, "Turn,Comp_Cities,Comp_Units");
        }
    }

    // Setup global panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableMouseCapture); // Release the mouse safely
        let _ = terminal::disable_raw_mode();
        let _ = stdout.execute(terminal::LeaveAlternateScreen);
        let _ = stdout.execute(cursor::Show);
        original_hook(panic_info);
    }));

    let mut stdout = io::stdout();
    stdout.execute(terminal::EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    stdout.execute(EnableMouseCapture)?; // Start listening to the mouse!
    stdout.execute(cursor::Hide)?;
    stdout.execute(terminal::Clear(ClearType::All))?;

    // 1. Difficulty prompt
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
                stdout.execute(cursor::Show)?;
                terminal::disable_raw_mode()?;
                stdout.execute(terminal::LeaveAlternateScreen)?;
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

    let difficulty: u32 = difficulty_input.trim().parse().unwrap_or(50).clamp(1, 100);

    // 2. Initial state setup
    stdout.execute(terminal::Clear(ClearType::All))?;
    let mut game = Game::new(60, 100, difficulty);
    game.setup_initial_states();

    // 3. Initial Production Prompt for Starting Player Cities
    let starting_player_cities: Vec<usize> = game.cities.iter().enumerate()
        .filter(|(_, ct)| ct.owner == Owner::Player)
        .map(|(i, _)| i)
        .collect();

    for idx in starting_player_cities {
        prompt_city_production(&mut game, idx);
    }

    // 4. Main Game Loop with Blinking Active Unit Prompting
    let action_prompt = "[N]ext, [A]utomove, [M]anual, [S]ave, [L]oad, [Q]uit: ";
    disp::refresh_display_with_input(&game, action_prompt);

    loop {
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

        // Event Queue Hook: If the server captured a city in the background, prompt the client immediately
        if let Some(c_idx) = game.pending_cities.pop() {
            prompt_city_production(&mut game, c_idx);
            continue; 
        }

        // --- EDIT MODE LOOP ---
        if game.mode == GameMode::Edit {
            let edit_prompt = "[Arrows/NumPad] Move | [P]rod | [O]rders | [W]ake | [I]nitial Orders | [E]xit Edit: ";
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
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    game.map_offset_c -= mouse.column as i32 - last_c as i32;
                                    game.map_offset_r -= mouse.row as i32 - last_r as i32;
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }

                    if let Event::Key(key) = ev {
                        game.map_offset_r = 0;
                        game.map_offset_c = 0;
                        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                            let mut stdout = io::stdout();
                            let _ = stdout.execute(crossterm::event::DisableMouseCapture);
                            let _ = stdout.execute(cursor::Show);
                            let _ = terminal::disable_raw_mode();
                            let _ = stdout.execute(terminal::LeaveAlternateScreen);
                            std::process::exit(0);
                        }

                        match key.code {
                            KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Esc => {
                                game.mode = GameMode::Manual;
                                game.log(String::from("Exited Edit Mode."));
                                
                                // Drain terminal buffer to prevent instant double-trigger re-entry
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
                                let mut woke = 0;
                                for p in game.pieces.iter_mut() {
                                    if p.loc == game.edit_cursor && p.owner == Owner::Player && p.hits > 0 {
                                        p.activity = UnitActivity::Idle;
                                        woke += 1;
                                    }
                                }
                                if woke > 0 {
                                    game.log(format!("Awakened {} unit(s) at cursor.", woke));
                                } else {
                                    game.log(String::from("No resting player units at cursor."));
                                }
                            }
                            KeyCode::Char('o') | KeyCode::Char('O') => {
                                if let Some(p_idx) = game.pieces.iter().position(|p| p.loc == game.edit_cursor && p.owner == Owner::Player && p.hits > 0) {
                                    let ptype = game.pieces[p_idx].ptype;
                                    let prompt = format!("Set Orders for {:?} - [S]entry, [Y]attack, [G]rope, [W]ake/Idle: ", ptype);
                                    disp::refresh_display_with_input(&game, &prompt);
                                    loop {
                                        if let Ok(Event::Key(k)) = event::read() {
                                            if k.code == KeyCode::Esc {
                                                game.log(String::from("Order update cancelled."));
                                                break;
                                            }
                                            if let KeyCode::Char(c) = k.code {
                                                match c.to_ascii_lowercase() {
                                                    's' => { game.pieces[p_idx].activity = UnitActivity::Sentry; game.log(String::from("Unit set to Sentry.")); break; }
                                                    'y' => { game.pieces[p_idx].activity = UnitActivity::Attack; game.log(String::from("Unit set to Attack.")); break; }
                                                    'g' => { game.pieces[p_idx].activity = UnitActivity::Explore; game.log(String::from("Unit set to Grope/Explore.")); break; }
                                                    'w' => { game.pieces[p_idx].activity = UnitActivity::Idle; game.log(String::from("Unit woken up (Idle).")); break; }
                                                    'f' => { game.pieces[p_idx].activity = UnitActivity::Fill; game.log(String::from("Unit set to Fill.")); break; }
                                                    'l' => { game.pieces[p_idx].activity = UnitActivity::Land; game.log(String::from("Unit set to Land.")); break; }
                                                    'r' => { game.pieces[p_idx].activity = UnitActivity::Random; game.log(String::from("Unit set to Random.")); break; }
                                                    't' => { game.pieces[p_idx].activity = UnitActivity::TransportWait; game.log(String::from("Unit set to Transport Wait.")); break; }
                                                    'u' => { game.pieces[p_idx].activity = UnitActivity::Upgrade; game.log(String::from("Unit set to Upgrade.")); break; }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    game.log(String::from("No player unit at cursor location."));
                                }
                            }
                            KeyCode::Char('i') | KeyCode::Char('I') => {
                                if let Some(c_idx) = game.cities.iter().position(|c| c.loc == game.edit_cursor && c.owner == Owner::Player) {
                                    let prompt = "City Order? [A]rmy,[T]rns,[P]tBoat,[D]est,[S]ub,[C]arr,[B]ttle,[F]ght,[Z]Sat (Esc to cancel): ";
                                    disp::refresh_display_with_input(&game, prompt);
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
                                        let prompt2 = format!("City Order for {:?} - [S]entry, [Y]attack, [G]rope, [F]ill, [L]and, [T]rnsWait, [U]pgrade, [R]andom, [V]ector, [W]ake/Idle: ", ptype);
                                        disp::refresh_display_with_input(&game, &prompt2);
                                        let mut new_act = None;
                                        loop {
                                            if let Ok(Event::Key(k)) = event::read() {
                                                if k.code == KeyCode::Esc {
                                                    game.log(String::from("City order update cancelled."));
                                                    break;
                                                }
                                                if let KeyCode::Char(c) = k.code {
                                                    let choice = c.to_ascii_lowercase();
                                                    if choice == 'v' {
                                                        let dir_prompt = format!("Set city march direction for {:?} [Arrows/NumPad] or [Esc] Cancel: ", ptype);
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
                                                        if new_act.is_some() {
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(act) = new_act {
                                            if let Some(idx) = game.cities[c_idx].default_orders.iter().position(|(p, _)| *p == ptype) {
                                                game.cities[c_idx].default_orders[idx].1 = act;
                                            } else {
                                                game.cities[c_idx].default_orders.push((ptype, act));
                                            }
                                            game.log(format!("City at ({},{}) order for {:?} set to {:?}.", game.cities[c_idx].loc.r, game.cities[c_idx].loc.c, ptype, act));
                                        }
                                    } else {
                                        game.log(String::from("City order update cancelled."));
                                    }
                                } else {
                                    game.log(String::from("No player-owned city at cursor location."));
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

        
        // --- LOGISTICS AUTOMATION SWEEP ---
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

        let mut was_auto = game.mode == GameMode::Auto;

        if was_auto {
            let idle_units = game.pieces.iter()
                .filter(|p| p.owner == Owner::Player && p.moves_left > 0 && p.activity == UnitActivity::Idle)
                .count();

            if idle_units > 0 {
                game.mode = GameMode::Manual;
            } else {
                let player_moves_left = game.pieces.iter()
                    .filter(|p| p.owner == Owner::Player && p.moves_left > 0 && p.activity == UnitActivity::Idle)
                    .count();
                
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
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    game.map_offset_c -= mouse.column as i32 - last_c as i32;
                                    game.map_offset_r -= mouse.row as i32 - last_r as i32;
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }
                    if let Event::Key(key) = ev {
                        game.map_offset_r = 0;
                        game.map_offset_c = 0;
                        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                            let mut stdout = io::stdout();
                            let _ = stdout.execute(DisableMouseCapture); // Turn mouse off
                            let _ = stdout.execute(cursor::Show);
                            let _ = terminal::disable_raw_mode();
                            let _ = stdout.execute(terminal::LeaveAlternateScreen);
                            return Ok(());
                        }
                        if let KeyCode::Char(ch) = key.code {
                            match ch.to_ascii_lowercase() {
                                'q' => {
                                    let mut stdout = io::stdout();
                                    let _ = stdout.execute(DisableMouseCapture); // Turn mouse off
                                    let _ = stdout.execute(cursor::Show);
                                    let _ = terminal::disable_raw_mode();
                                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                    return Ok(());
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

        // Available active player piece requiring input
        let active_piece_id = game.pieces.iter()
            .find(|p| p.owner == Owner::Player && p.moves_left > 0 && p.activity == UnitActivity::Idle)
            .map(|p| p.id);

        if let Some(pid) = active_piece_id {
            if let Some(piece_idx) = game.pieces.iter().position(|p| p.id == pid) {
                let ptype = game.pieces[piece_idx].ptype;
                let loc = game.pieces[piece_idx].loc;
                let moves = game.pieces[piece_idx].moves_left;

                let move_prompt = format!(
                    "[{:?} at ({},{}) M:{}]: [789/46/123] move, [S]entry, [Y]attack, [G]rope, [D]est, [V]ector, [T]rnsWait, [W]ake: ",
                    ptype, loc.r, loc.c, moves
                );

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
                                if let Some((last_c, last_r)) = game.last_mouse_coord {
                                    game.map_offset_c -= mouse.column as i32 - last_c as i32;
                                    game.map_offset_r -= mouse.row as i32 - last_r as i32;
                                }
                                game.last_mouse_coord = Some((mouse.column, mouse.row));
                            }
                            _ => {}
                        }
                    }
                    if let Event::Key(key) = ev {
                            game.map_offset_r = 0;
                            game.map_offset_c = 0;
                            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                                let mut stdout = io::stdout();
                                let _ = stdout.execute(DisableMouseCapture); // Turn mouse off
                                let _ = stdout.execute(cursor::Show);
                                let _ = terminal::disable_raw_mode();
                                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                return Ok(());
                            }

                            if let KeyCode::Char(ch) = key.code {
                                match ch {
                                    ' ' | 'n' | 'N' | 's' | 'S' | 't' | 'T' | 'w' | 'W' => {
                                        let is_sentry = ch == 's' || ch == 'S';
                                        let is_twait = ch == 't' || ch == 'T';
                                        let is_wake = ch == 'w' || ch == 'W';
                                        let mut proceed = true;
                                        let mut attack_own_city = false;

                                        // Prevent Armies from sleeping/skipping inside a player city
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

                                        game.pieces[piece_idx].moves_left = 0;
                                        if is_sentry {
                                            game.pieces[piece_idx].activity = UnitActivity::Sentry;
                                            game.log(format!("{:?}({}:{}) put on sentry duty.", ptype, loc.r, loc.c));
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
                                                        game.pieces[piece_idx].moves_left = 0; 
                                                        game.mode = GameMode::Manual;
                                                        game.log(format!("{:?} ordered to march toward ({},{}).", ptype, game.edit_cursor.r, game.edit_cursor.c));
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
                                            game.pieces[piece_idx].moves_left = 0; 
                                            let dir_name = match (dr, dc) {
                                                (-1, 0) => "North", (1, 0) => "South", (0, -1) => "West", (0, 1) => "East",
                                                (-1, -1) => "NW", (-1, 1) => "NE", (1, -1) => "SW", (1, 1) => "SE",
                                                _ => "Unknown",
                                            };
                                            game.log(format!("{:?} ordered to march continuously {}.", ptype, dir_name));
                                        } else {
                                            game.log(String::from("March order cancelled."));
                                        }
                                        while event::poll(Duration::from_millis(0)).unwrap_or(false) { let _ = event::read(); }
                                        break;
                                    }
                                    'f' | 'F' => { game.pieces[piece_idx].activity = UnitActivity::Fill; game.log(String::from("Unit set to Fill.")); break; }
                                    'l' | 'L' => { game.pieces[piece_idx].activity = UnitActivity::Land; game.log(String::from("Unit set to Land.")); break; }
                                    'r' | 'R' => { game.pieces[piece_idx].activity = UnitActivity::Random; game.log(String::from("Unit set to Random.")); break; }
                                    'u' | 'U' => { game.pieces[piece_idx].activity = UnitActivity::Upgrade; game.log(String::from("Unit set to Upgrade.")); break; }
                                    _ => {}
                                }
                                // Handle directional movement cleanly
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
                                                let mut stdout = io::stdout();
                                                let _ = stdout.execute(DisableMouseCapture); // Turn mouse off
                                                let _ = stdout.execute(cursor::Show);
                                                let _ = terminal::disable_raw_mode();
                                                let _ = stdout.execute(terminal::LeaveAlternateScreen);
                                                return Ok(());
                                            }
                                            'a' => {
                                                game.mode = GameMode::Auto;
                                                was_auto = true; // Ensure Auto resumes
                                                game.log(String::from("Automove mode engaged. Press 'o' or 'm' to cancel."));
                                            }
                                            'o' | 'm' => {
                                                game.mode = GameMode::Manual;
                                                was_auto = false; // Cancel the Auto-resume
                                                game.log(String::from("Manual move mode engaged."));
                                            }
                                            'e' => {
                                                game.mode = GameMode::Edit;
                                                game.edit_cursor = loc; // Start cursor at current unit
                                                was_auto = false; // Prevent Auto from instantly overriding Edit Mode!
                                                game.log(String::from("Edit Mode engaged."));
                                                
                                                // Drain terminal buffer to prevent instant double-trigger exit
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
                                        
                                        // NEW: Friendly City Assault Safety Catch
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

                                        // If the unified engine says a city was captured, we pause and prompt the user!
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

        // Standard Command Mode Input / Polling when no units have moves left
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
                        if let Some((last_c, last_r)) = game.last_mouse_coord {
                            game.map_offset_c -= mouse.column as i32 - last_c as i32;
                            game.map_offset_r -= mouse.row as i32 - last_r as i32;
                        }
                        game.last_mouse_coord = Some((mouse.column, mouse.row));
                    }
                    _ => {}
                }
            }

            if let Event::Key(key) = ev {
                game.map_offset_r = 0;
                game.map_offset_c = 0;
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    let mut stdout = io::stdout();
                    let _ = stdout.execute(DisableMouseCapture);
                    let _ = stdout.execute(cursor::Show);
                    let _ = terminal::disable_raw_mode();
                    let _ = stdout.execute(terminal::LeaveAlternateScreen);
                    return Ok(());
                }

                if let KeyCode::Char(ch) = key.code {
                    match ch.to_ascii_lowercase() {
                        'q' => {
                            let mut stdout = io::stdout();
                            let _ = stdout.execute(DisableMouseCapture);
                            let _ = stdout.execute(cursor::Show);
                            let _ = terminal::disable_raw_mode();
                            let _ = stdout.execute(terminal::LeaveAlternateScreen);
                            return Ok(());
                        }
                        'n' => {
                            game.process_turn();
                        }
                        '`' | '~' => {
                            game.reveal_entire_map();
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
                            // Start cursor at the player's first valid city
                            game.edit_cursor = game.cities.iter().find(|c| c.owner == Owner::Player).map(|c| c.loc).unwrap_or(Loc { r: game.rows/2, c: game.cols/2 });
                            game.log(String::from("Edit Mode engaged."));
                            
                            // Drain terminal buffer to prevent instant double-trigger exit
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
