use crate::game::{Game, Loc, PieceType};

/// Simple Manhattan distance heuristic between two locations on the map
pub fn distance(a: Loc, b: Loc) -> usize {
    ((a.r as isize - b.r as isize).abs() + (a.c as isize - b.c as isize).abs()) as usize
}

/// Checks if a piece type can traverse a specific terrain byte
pub fn can_traverse(game: &Game, ptype: PieceType, r: usize, c: usize) -> bool {
    if r >= game.rows || c >= game.cols {
        return false;
    }
    
    let terrain = game.map[r][c];

    match ptype {
        PieceType::Army | PieceType::Transport => {
            // Armies and Transports prefer land/ports (customize based on your map terrain bytes)
            terrain != b'~' && terrain != b'#' // Assuming '~' or '#' is deep water
        }
        PieceType::Fighter | PieceType::Satellite => {
            // Air units can go over almost anything
            true
        }
        _ => {
            // Naval units require water
            terrain == b'~' || terrain == b'#'
        }
    }
}

/// Finds the next best directional step toward a target destination
pub fn find_next_step(game: &Game, current: Loc, target: Loc, ptype: PieceType) -> Option<Loc> {
    let mut best_loc = None;
    let mut min_dist = distance(current, target);

    // Check all 8 surrounding neighbors (cardinal + diagonal)
    let dr = [-1, -1, -1, 0, 0, 1, 1, 1];
    let dc = [-1, 0, 1, -1, 1, -1, 0, 1];

    for i in 0..8 {
        let nr = current.r as isize + dr[i];
        let nc = current.c as isize + dc[i];

        if nr >= 0 && nr < game.rows as isize && nc >= 0 && nc < game.cols as isize {
            let next_loc = Loc { r: nr as usize, c: nc as usize };

            if can_traverse(game, ptype, next_loc.r, next_loc.c) {
                let dist = distance(next_loc, target);
                if dist < min_dist {
                    min_dist = dist;
                    best_loc = Some(next_loc);
                }
            }
        }
    }

    best_loc
}
