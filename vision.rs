use crate::game::{Game, Owner};

/// Initializes or updates the player's explored/visible map based on unit and city positions.
pub fn update_player_vision(game: &mut Game) {
    if game.player_map.is_empty() || game.player_map.len() != game.rows {
        game.player_map = vec![vec![0; game.cols]; game.rows];
    } else {
        for r in 0..game.rows {
            for c in 0..game.cols {
                if game.player_map[r][c] == 2 {
                    game.player_map[r][c] = 1;
                }
            }
        }
    }

    let mut reveal = |center_r: usize, center_c: usize, range: i32| {
        let r_min = (center_r as isize - range as isize).max(0) as usize;
        let r_max = (center_r as isize + range as isize).min(game.rows as isize - 1) as usize;
        let c_min = (center_c as isize - range as isize).max(0) as usize;
        let c_max = (center_c as isize + range as isize).min(game.cols as isize - 1) as usize;

        for r in r_min..=r_max {
            for c in c_min..=c_max {
                let dr = (r as isize - center_r as isize).abs();
                let dc = (c as isize - center_c as isize).abs();
                if dr * dr + dc * dc <= (range as isize * range as isize) {
                    game.player_map[r][c] = 2; // Currently visible
                }
            }
        }
    };

    for city in &game.cities {
        if city.owner == Owner::Player {
            reveal(city.loc.r, city.loc.c, 3);
        }
    }

    for piece in &game.pieces {
        if piece.owner == Owner::Player {
            reveal(piece.loc.r, piece.loc.c, piece.ptype.sight_range());
        }
    }
}
