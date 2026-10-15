use rand::Rng;
use crate::game::Game;

/// Generates a random number up to max, mimicking the original C irand() function.
pub fn irand(_game: &Game, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    rand::thread_rng().gen_range(0..max)
}
