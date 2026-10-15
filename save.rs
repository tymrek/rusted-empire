use crate::game::Game;
use std::fs::File;
use std::io::{Read, Write};

/// Saves the current game state to a JSON file.
pub fn save_game(game: &Game, filename: &str) {
    match serde_json::to_string_pretty(game) {
        Ok(json) => {
            if let Ok(mut file) = File::create(filename) {
                if file.write_all(json.as_bytes()).is_ok() {
                    println!("Game successfully saved to '{}'.", filename);
                } else {
                    println!("Failed to write save data to file.");
                }
            } else {
                println!("Failed to create save file '{}'.", filename);
            }
        }
        Err(e) => println!("Failed to serialize game state: {}", e),
    }
}

/// Loads a game state from a JSON file.
pub fn load_game(filename: &str) -> Result<Game, String> {
    let mut file = match File::open(filename) {
        Ok(f) => f,
        Err(e) => return Err(format!("Could not open file: {}", e)),
    };

    let mut contents = String::new();
    if let Err(e) = file.read_to_string(&mut contents) {
        return Err(format!("Could not read file: {}", e));
    }

    match serde_json::from_str::<Game>(&contents) {
        Ok(game) => Ok(game),
        Err(e) => Err(format!("Failed to parse save JSON: {}", e)),
    }
}
