mod ai;
mod app;
mod disp;
mod game;
mod game_tests;
mod headless;
mod models;
mod terminal;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--headless") {
        headless::run();
        return ExitCode::SUCCESS;
    }

    let value_of = |flags: &[&str]| {
        args.iter().position(|a| flags.contains(&a.as_str())).and_then(|i| args.get(i + 1)).cloned()
    };
    let seed = match value_of(&["--seed"]) {
        None => None,
        Some(s) => match s.parse::<u64>() {
            Ok(n) => Some(n),
            Err(_) => {
                eprintln!("--seed needs a whole number, got '{}'", s);
                return ExitCode::from(2);
            }
        },
    };
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
