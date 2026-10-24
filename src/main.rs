//! rs-empire: a Rust port of VMS-Empire.
//!
//! The game itself (`game`, `ai`, `models`) knows nothing about the screen: the terminal
//! front end (`app`, `disp`, `keys`, `view`, `ui`, `terminal`) turns keys into calls on `Game`
//! and keeps what it is showing in a `View`. That line is where a client/server split
//! would go.
mod ai;
mod app;
mod disp;
mod game;
mod game_tests;
mod headless;
mod keys;
mod models;
mod terminal;
#[cfg(test)]
mod tui_tests;
mod ui;
mod view;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    let value_of = |flags: &[&str]| {
        args.iter().position(|a| flags.contains(&a.as_str())).and_then(|i| args.get(i + 1)).cloned()
    };
    // A whole-number option: Ok(None) if absent, Err if not a number.
    let number = |flag: &str| -> Result<Option<u64>, ExitCode> {
        match value_of(&[flag]) {
            None => Ok(None),
            Some(s) => s.parse::<u64>().map(Some).map_err(|_| {
                eprintln!("{} needs a whole number, got '{}'", flag, s);
                ExitCode::from(2)
            }),
        }
    };
    let seed = match number("--seed") {
        Ok(n) => n,
        Err(code) => return code,
    };

    if args.iter().any(|a| a == "--headless") {
        let mut opts = headless::Options { seed, small: args.iter().any(|a| a == "--small"), ..Default::default() };
        // rule experiments (see models::ORIGINAL_SIGHT)
        if args.iter().any(|a| a == "--original-sight") {
            models::ORIGINAL_SIGHT.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if args.iter().any(|a| a == "--adjacent-subs") {
            models::ADJACENT_SUB_DETECTION.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let numbers = (number("--games"), number("--level"), number("--turns"), number("--stalemate"));
        match numbers {
            (Ok(games), Ok(level), Ok(turns), Ok(stalemate)) => {
                if let Some(n) = games { opts.games = (n as usize).max(1); }
                opts.level = level.map(|n| n as usize);
                if let Some(n) = turns { opts.max_turns = n as i32; }
                if let Some(n) = stalemate { opts.stalemate_turns = n as i32; }
            }
            (Err(code), ..) | (_, Err(code), ..) | (_, _, Err(code), _) | (.., Err(code)) => return code,
        }
        headless::run(opts);
        return ExitCode::SUCCESS;
    }

    let opts = app::Options {
        load_file: value_of(&["--load", "-l"]),
        seed,
        torus: args.iter().any(|a| a == "-t" || a == "--torus"),
    };

    match app::run(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rs-empire: {}", e);
            ExitCode::FAILURE
        }
    }
}
