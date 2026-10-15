use crate::game::{Game, Piece, PieceType};
use crate::util;

impl PieceType {
    /// Returns the base combat attack strength for a piece type
    pub fn attack_strength(&self) -> i32 {
        match self {
            PieceType::Army => 4,
            PieceType::Fighter => 6,
            PieceType::PatrolBoat => 3,
            PieceType::Destroyer => 8,
            PieceType::Submarine => 7,
            PieceType::Carrier => 10,
            PieceType::Battleship => 15,
            PieceType::Transport => 2,
            PieceType::Satellite => 1,
        }
    }

    /// Returns the base defense strength for a piece type
    pub fn defense_strength(&self) -> i32 {
        match self {
            PieceType::Army => 4,
            PieceType::Fighter => 5,
            PieceType::PatrolBoat => 4,
            PieceType::Destroyer => 7,
            PieceType::Submarine => 6,
            PieceType::Carrier => 12,
            PieceType::Battleship => 14,
            PieceType::Transport => 4,
            PieceType::Satellite => 1,
        }
    }
}

/// Resolves combat between an attacking piece and a defending piece
pub fn resolve_unit_combat(game: &Game, attacker: &Piece, defender: &Piece) -> bool {
    let att_str = attacker.ptype.attack_strength() * (attacker.hits.max(1));
    let def_str = defender.ptype.defense_strength() * (defender.hits.max(1));

    let total = att_str + def_str;
    if total <= 0 {
        return false;
    }

    // Randomized roll favoring stronger units
    let roll = util::irand(game, total as u32);
    roll < att_str as u32
}

pub fn update_combat_loop(game: &mut Game) {
    // Check for units occupying the same location and resolve combat
    let mut dead_indices = Vec::new();

    for i in 0..game.pieces.len() {
        for j in (i + 1)..game.pieces.len() {
            if game.pieces[i].loc == game.pieces[j].loc {
                if game.pieces[i].owner != game.pieces[j].owner {
                    // Conflict detected!
                    let attacker = &game.pieces[i];
                    let defender = &game.pieces[j];

                    if resolve_unit_combat(game, attacker, defender) {
                        // Attacker wins, defender dies
                        dead_indices.push(j);
                    } else {
                        // Defender wins, attacker dies
                        dead_indices.push(i);
                    }
                }
            }
        }
    }

    // Remove destroyed units safely (in reverse order to preserve indexing)
    dead_indices.sort_unstable();
    dead_indices.dedup();
    for &idx in dead_indices.iter().rev() {
        game.pieces.remove(idx);
    }
}

pub fn check_city_conquests(game: &mut Game) {
    for city in &mut game.cities {
        for piece in &game.pieces {
            if piece.loc == city.loc && piece.owner != city.owner {
                println!("City at ({}, {}) conquered by {:?}!", city.loc.r, city.loc.c, piece.owner);
                city.owner = piece.owner;
                city.work = 0; // Reset work progress on capture
            }
        }
    }
}
