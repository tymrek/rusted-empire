//! `--headless`: plays AI-vs-AI games across CPU threads as a balance/regression rig.
use crate::game::Game;
use crate::models::*;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};

/// Number of AI-vs-AI games the `--headless` playtest rig plays.
const PLAYTEST_GAMES: usize = 100;

pub fn run() {
    println!("Starting headless playtest rig for {} games across CPU threads...", PLAYTEST_GAMES);
    let p_wins = Arc::new(AtomicUsize::new(0));
    let c_wins = Arc::new(AtomicUsize::new(0));
    let draws = Arc::new(AtomicUsize::new(0));
    let current_game = Arc::new(AtomicUsize::new(1));
    let completed_games = Arc::new(AtomicUsize::new(0));

    let file = std::fs::File::create("playtest_metrics.csv").expect("Failed to create metrics file");
    let file_mutex = Arc::new(Mutex::new(file));
    // Fighter statistics summed over all games: [player, computer].
    let fighter_totals = Arc::new(Mutex::new([FighterStats::default(); 2]));
    
    {
        let mut f = file_mutex.lock().unwrap();
        let _ = writeln!(f, "Game,Winner,Turns,ElapsedSec,{},{}", stats_csv_header("P"), stats_csv_header("C"));
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
        let fighter_totals = Arc::clone(&fighter_totals);

        handles.push(thread::spawn(move || {
            loop {
                let i = current_game.fetch_add(1, Ordering::Relaxed);
                if i > PLAYTEST_GAMES {
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
                    let _ = writeln!(f, "{},{},{},{:.2},{},{}", i, winner, game.turn, elapsed_sec,
                        stats_csv(&game.fighter_stats[0]), stats_csv(&game.fighter_stats[1]));
                }
                {
                    let mut t = fighter_totals.lock().unwrap();
                    t[0] += game.fighter_stats[0];
                    t[1] += game.fighter_stats[1];
                }

                let mut stdout = io::stdout();
                let _ = write!(stdout, "\x1B[2K\r");
                if is_draw {
                    let _ = writeln!(stdout, "Game {}/{} finished in a Draw (Turn limit). [{:.2}s]", i, PLAYTEST_GAMES, elapsed_sec);
                } else {
                    let _ = writeln!(stdout, "Game {}/{} finished in {} turns. Winner: {} [{:.2}s]", i, PLAYTEST_GAMES, game.turn, winner, elapsed_sec);
                }
                let _ = writeln!(stdout, "    computer fighters: {}", stats_line(&game.fighter_stats[1]));
                
                completed_games.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let mut stdout = io::stdout();
    loop {
        let done = completed_games.load(Ordering::Relaxed);
        if done >= PLAYTEST_GAMES {
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
    let totals = fighter_totals.lock().unwrap();
    println!("Fighters, all games (details per game in playtest_metrics.csv):");
    println!("  Computer: {}", stats_line(&totals[1]));
    println!("  Player:   {}", stats_line(&totals[0]));
}

/// One line summing up a side's fighters.
fn stats_line(s: &FighterStats) -> String {
    format!(
        "built {} | landed {} ({} with <=2 fuel) | ran out of fuel {} | shot down {} | lost over enemy cities {} | lost with carriers {}",
        s.built, s.landings, s.low_fuel_landings, s.out_of_fuel, s.shot_down, s.lost_over_city, s.lost_with_carrier
    )
}

fn stats_csv_header(side: &str) -> String {
    ["FBuilt", "FLanded", "FLowFuelLandings", "FOutOfFuel", "FShotDown", "FLostOverCity", "FLostWithCarrier"]
        .iter()
        .map(|h| format!("{}{}", side, h))
        .collect::<Vec<_>>()
        .join(",")
}

fn stats_csv(s: &FighterStats) -> String {
    format!("{},{},{},{},{},{},{}", s.built, s.landings, s.low_fuel_landings, s.out_of_fuel, s.shot_down, s.lost_over_city, s.lost_with_carrier)
}
