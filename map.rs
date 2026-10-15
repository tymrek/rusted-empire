use crate::game::{Game, City, Owner, PieceType, Piece, Loc};
use rand::Rng;

pub fn generate_map(game: &mut Game) {
    let mut rng = rand::thread_rng();

    // 1. Generate map land masses (Archipelago)
    let num_clusters = (game.rows * game.cols) / 80;
    for _ in 0..num_clusters {
        let center_r = rng.gen_range(2..game.rows.saturating_sub(2));
        let center_c = rng.gen_range(2..game.cols.saturating_sub(2));
        let radius = rng.gen_range(2..5);

        for r in (center_r.saturating_sub(radius))..(center_r + radius).min(game.rows) {
            for c in (center_c.saturating_sub(radius))..(center_c + radius).min(game.cols) {
                let dist_sq = (r as isize - center_r as isize).pow(2) + (c as isize - center_c as isize).pow(2);
                if dist_sq <= (radius as isize).pow(2) && rng.gen_bool(0.75) {
                    game.map[r][c] = b'#';
                }
            }
        }
    }

    // Collect all land tiles suitable for cities
    let mut land_tiles = Vec::new();
    for r in 1..game.rows - 1 {
        for c in 1..game.cols - 1 {
            if game.map[r][c] == b'#' {
                land_tiles.push(Loc { r, c });
            }
        }
    }

    if land_tiles.is_empty() {
        // Fallback if map generation missed land
        game.map[game.rows / 2][game.cols / 2] = b'#';
        land_tiles.push(Loc { r: game.rows / 2, c: game.cols / 2 });
    }

    // Shuffle land tiles for randomized distribution
    for i in 0..land_tiles.len() {
        let j = rng.gen_range(0..land_tiles.len());
        land_tiles.swap(i, j);
    }

    // Total cities to place (scaled to map size)
    let total_cities = (land_tiles.len() / 6).max(6);
    
    // Difficulty (1 to 100) determines player vs computer starting city advantage.
    // Diff 1: Player gets 80% of initial cities (large island), Comp gets 2 cities.
    // Diff 100: Player gets 2 cities on a tiny island, Comp gets the rest.
    let diff_ratio = game.difficulty as f32 / 100.0;
    let player_city_count = ((1.0 - diff_ratio * 0.85) * (total_cities as f32)).max(2.0) as usize;

    let mut placed = 0;
    for loc in land_tiles {
        if placed >= total_cities {
            break;
        }

        let owner = if placed < player_city_count {
            Owner::Player
        } else if placed < player_city_count + 2 {
            Owner::Comp
        } else {
            Owner::Neutral
        };

        game.cities.push(City {
            loc,
            owner,
            prod: PieceType::Army,
            work: 0,
        });
        placed += 1;
    }

    // Spawn starting units at player & comp cities
    for city in &game.cities {
        if city.owner == Owner::Player {
            let unit_loc = Loc { r: city.loc.r.min(game.rows - 1), c: city.loc.c };
            game.pieces.push(Piece {
                id: game.pieces.len(),
                ptype: PieceType::Army,
                owner: Owner::Player,
                loc: unit_loc,
                hits: PieceType::Army.max_hits(),
                moves_left: PieceType::Army.max_moves(),
                cargo: Vec::new(),
            });
        } else if city.owner == Owner::Comp {
            let unit_loc = Loc { r: city.loc.r.min(game.rows - 1), c: city.loc.c };
            game.pieces.push(Piece {
                id: game.pieces.len(),
                ptype: PieceType::Army,
                owner: Owner::Comp,
                loc: unit_loc,
                hits: PieceType::Army.max_hits(),
                moves_left: PieceType::Army.max_moves(),
                cargo: Vec::new(),
            });
        }
    }

    game.log(format!("Map generated. Difficulty set to {}.", game.difficulty));
}
