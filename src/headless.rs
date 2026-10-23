//! `--headless`: plays AI-vs-AI games across CPU threads as a balance/regression rig.
//!
//! Both sides are played by the computer AI (the port of the original's `compmove.c`),
//! each from its own view of the world, so the games are mirror matches that test the AI
//! against itself. Every game is reproducible from its seed.
use crate::game::{Game, MAP_COLS, MAP_ROWS};
use crate::models::*;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What the command line asked of the rig.
#[derive(Clone, Debug)]
pub struct Options {
    /// `--games N`: how many games to play.
    pub games: usize,
    /// `--seed N`: game `i` (from 1) uses seed `N + i - 1`; random if not given.
    pub seed: Option<u64>,
    /// `--small`: a 30 x 30 world (10 cities) for quick runs instead of the real 60 x 100.
    pub small: bool,
    /// `--level N`: the difficulty level; by default the fairest level that puts the two
    /// sides on different continents (see `fair_level`).
    pub level: Option<usize>,
    /// `--turns N`: give up on a game after this many turns.
    pub max_turns: i32,
    /// `--stalemate N`: end a game after this many turns in which neither side made
    /// progress, i.e. held more cities than it ever had before.
    pub stalemate_turns: i32,
}

impl Default for Options {
    fn default() -> Self {
        Options { games: 100, seed: None, small: false, level: None, max_turns: 3000, stalemate_turns: 500 }
    }
}

/// How one game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// One side has nothing left (the original's game-over rule).
    Wipeout(Owner),
    /// One side is down to under a third of the other's cities and armies (the original's
    /// rule for the computer acknowledging defeat, applied to both sides).
    Resigned(Owner),
    /// Neither side made progress for `stalemate_turns` turns. Cities trading hands
    /// back and forth is not progress: an even war of attrition can go on for thousands
    /// of turns while the unit count (and the time per turn) keeps growing.
    Stalemate,
    /// Reached `max_turns`.
    TurnLimit,
}

impl Outcome {
    /// The winning side, if any.
    pub fn winner(&self) -> Option<Owner> {
        let other = |o: Owner| if o == Owner::Player { Owner::Comp } else { Owner::Player };
        match *self {
            Outcome::Wipeout(loser) | Outcome::Resigned(loser) => Some(other(loser)),
            _ => None,
        }
    }

    fn describe(&self) -> String {
        let side = |o: Owner| if o == Owner::Player { "Player" } else { "Comp" };
        match *self {
            Outcome::Wipeout(loser) => format!("{} (wipeout)", side(self.winner().unwrap_or(loser))),
            Outcome::Resigned(_) => format!("{} (resign)", side(self.winner().unwrap())),
            Outcome::Stalemate => String::from("Stalemate"),
            Outcome::TurnLimit => String::from("Turn limit"),
        }
    }
}

/// The result of one game.
pub struct GameResult {
    pub outcome: Outcome,
    pub turns: i32,
    pub cities: [usize; 2],
    pub fighters: [FighterStats; 2],
}

fn cities_of(game: &Game, o: Owner) -> usize {
    game.cities.iter().filter(|c| c.owner == o).count()
}

fn armies_of(game: &Game, o: Owner) -> usize {
    fn count(pieces: &[Piece], o: Owner) -> usize {
        pieces.iter().map(|p| (p.owner == o && p.hits > 0 && p.ptype == PieceType::Army) as usize + count(&p.cargo, o)).sum()
    }
    count(&game.pieces, o)
}

/// True if side `o` is down to under a third of the other side's cities and armies.
fn outclassed(game: &Game, o: Owner) -> bool {
    let other = if o == Owner::Player { Owner::Comp } else { Owner::Player };
    cities_of(game, o) < cities_of(game, other) / 3 && armies_of(game, o) < armies_of(game, other) / 3
}

/// The difficulty level whose two starting continents are closest in value while being
/// different continents (sharing one makes for a quick knife fight, not a test of the
/// AI). If there is only one continent to start on, level 0.
pub fn fair_level(game: &Game) -> usize {
    let conts = game.start_continents();
    game.start_pairs().iter().enumerate()
        .filter(|(_, (u, c))| u != c)
        .min_by_key(|(_, (u, c))| (conts[*u].value - conts[*c].value).abs())
        .map_or(0, |(level, _)| level)
}

/// Plays one AI-vs-AI game from `seed` to the end.
pub fn play_one(opts: &Options, seed: u64) -> GameResult {
    let (rows, cols) = if opts.small { (30, 30) } else { (MAP_ROWS, MAP_COLS) };
    let mut game = Game::with_seed(rows, cols, 0, seed);
    let levels = game.generate_world();
    let level = opts.level.unwrap_or_else(|| fair_level(&game)).min(levels - 1);
    game.choose_start(level);
    game.player_ai = true;
    // The player's starting city builds armies from a standing start, exactly as the
    // computer's does (no retooling penalty), so neither side gets a head start.
    if let Some(c) = game.cities.iter().position(|c| c.owner == Owner::Player) {
        game.cities[c].prod = PieceType::Army;
        game.cities[c].work = 0;
    }

    let held = |g: &Game, o: Owner| g.cities.iter().filter(|c| c.owner == o).count();
    let mut best = [held(&game, Owner::Player), held(&game, Owner::Comp)];
    let mut last_progress = game.turn;
    let outcome = loop {
        game.process_turn();
        if let Some(msg) = game.check_game_over() {
            break Outcome::Wipeout(if msg.contains("DEFEAT") { Owner::Player } else { Owner::Comp });
        }
        if outclassed(&game, Owner::Comp) {
            break Outcome::Resigned(Owner::Comp);
        }
        if outclassed(&game, Owner::Player) {
            break Outcome::Resigned(Owner::Player);
        }
        let now = [held(&game, Owner::Player), held(&game, Owner::Comp)];
        if now[0] > best[0] || now[1] > best[1] {
            best = [best[0].max(now[0]), best[1].max(now[1])];
            last_progress = game.turn;
        } else if game.turn - last_progress >= opts.stalemate_turns {
            break Outcome::Stalemate;
        }
        if game.turn >= opts.max_turns {
            break Outcome::TurnLimit;
        }
    };
    GameResult {
        outcome,
        turns: game.turn,
        cities: [cities_of(&game, Owner::Player), cities_of(&game, Owner::Comp)],
        fighters: game.fighter_stats,
    }
}

/// Runs the rig: plays `opts.games` games across CPU threads, prints a line per game and a
/// summary, and writes `playtest_metrics.csv`.
pub fn run(opts: Options) {
    let base_seed = opts.seed.unwrap_or_else(|| {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1)
    });
    let map = if opts.small { "30x30" } else { "60x100" };
    println!(
        "Starting headless playtest rig: {} AI-vs-AI games on the {} map, seeds {}..{}",
        opts.games, map, base_seed, base_seed + opts.games as u64 - 1
    );

    let file = std::fs::File::create("playtest_metrics.csv").expect("Failed to create metrics file");
    let file = Arc::new(Mutex::new(file));
    {
        let mut f = file.lock().unwrap();
        let _ = writeln!(f, "Game,Seed,Outcome,Turns,ElapsedSec,PCities,CCities,{},{}", stats_csv_header("P"), stats_csv_header("C"));
    }
    let results: Arc<Mutex<Vec<GameResult>>> = Arc::new(Mutex::new(Vec::new()));
    let next_game = Arc::new(AtomicUsize::new(1));
    let completed = Arc::new(AtomicUsize::new(0));
    let num_threads = thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4)).unwrap_or(4);
    let thread_starts = Arc::new((0..num_threads).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());

    let mut handles = vec![];
    for thread_id in 0..num_threads {
        let (opts, file, results) = (opts.clone(), Arc::clone(&file), Arc::clone(&results));
        let (next_game, completed, thread_starts) = (Arc::clone(&next_game), Arc::clone(&completed), Arc::clone(&thread_starts));
        handles.push(thread::spawn(move || loop {
            let i = next_game.fetch_add(1, Ordering::Relaxed);
            if i > opts.games {
                thread_starts[thread_id].store(0, Ordering::Relaxed);
                break;
            }
            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
            thread_starts[thread_id].store(now_ms, Ordering::Relaxed);
            let started = Instant::now();
            let seed = base_seed + i as u64 - 1;
            let r = play_one(&opts, seed);
            let secs = started.elapsed().as_secs_f32();

            {
                let mut f = file.lock().unwrap();
                let _ = writeln!(f, "{},{},{},{},{:.2},{},{},{},{}", i, seed, r.outcome.describe(), r.turns, secs,
                    r.cities[0], r.cities[1], stats_csv(&r.fighters[0]), stats_csv(&r.fighters[1]));
            }
            let mut out = io::stdout();
            let _ = write!(out, "\x1B[2K\r");
            let _ = writeln!(out, "Game {}/{} (seed {}): {} at turn {}, cities player {} / computer {} [{:.2}s]",
                i, opts.games, seed, r.outcome.describe(), r.turns, r.cities[0], r.cities[1], secs);
            let _ = writeln!(out, "    computer fighters: {}", stats_line(&r.fighters[1]));
            results.lock().unwrap().push(r);
            completed.fetch_add(1, Ordering::Relaxed);
        }));
    }

    // A status line with each thread's current game time.
    let mut out = io::stdout();
    while completed.load(Ordering::Relaxed) < opts.games {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
        let status: String = thread_starts.iter().enumerate().map(|(id, st)| {
            let st = st.load(Ordering::Relaxed);
            if st > 0 && now_ms >= st { format!("[T{}: {:.1}s] ", id, (now_ms - st) as f32 / 1000.0) } else { format!("[T{}: IDLE] ", id) }
        }).collect();
        let _ = write!(out, "\x1B[2K\r{}", status);
        let _ = out.flush();
        thread::sleep(Duration::from_millis(100));
    }
    for h in handles {
        let _ = h.join();
    }
    let _ = write!(out, "\x1B[2K\r");

    let results = results.lock().unwrap();
    let count = |f: &dyn Fn(&Outcome) -> bool| results.iter().filter(|r| f(&r.outcome)).count();
    println!(
        "Rig complete. Player (AI) wins: {} | Computer wins: {} | Stalemates: {} | Turn limit: {}",
        count(&|o| o.winner() == Some(Owner::Player)),
        count(&|o| o.winner() == Some(Owner::Comp)),
        count(&|o| *o == Outcome::Stalemate),
        count(&|o| *o == Outcome::TurnLimit),
    );
    let mut totals = [FighterStats::default(); 2];
    for r in results.iter() {
        totals[0] += r.fighters[0];
        totals[1] += r.fighters[1];
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_mirror_game_is_reproducible_and_ends() {
        let opts = Options { small: true, max_turns: 400, stalemate_turns: 200, ..Options::default() };
        let a = play_one(&opts, 5);
        let b = play_one(&opts, 5);
        assert_eq!((a.outcome, a.turns, a.cities), (b.outcome, b.turns, b.cities));
        assert!(a.turns <= 400);
    }

    #[test]
    fn test_fair_level_starts_the_sides_on_different_continents() {
        for seed in 1..=5 {
            let mut game = Game::with_seed(60, 100, 0, seed);
            game.generate_world();
            let conts = game.start_continents();
            if conts.len() < 2 {
                continue;
            }
            let (u, c) = game.start_pairs()[fair_level(&game)];
            assert_ne!(u, c);
        }
    }

    #[test]
    fn test_ai_plays_the_player_side() {
        let mut game = Game::with_seed(30, 30, 0, 7);
        game.setup_initial_states();
        game.player_ai = true;
        for _ in 0..40 {
            game.process_turn();
        }
        let player_units = game.pieces.iter().filter(|p| p.owner == Owner::Player).count();
        assert!(player_units > 0, "the player's side built and kept units");
        assert!(game.player_ai_map.iter().flatten().any(|&c| c == b'X'), "it sees its own city as its own");
    }
}
