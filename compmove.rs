use crate::game::{Game, Owner, PieceType, Loc};
use crate::attack::resolve_combat;

pub fn process_computer_turn(game: &mut Game) {
    game.log("--- Processing Computer Turn ---".to_string());

    let mut comp_new_pieces = Vec::new();
    let mut log_messages = Vec::new();

    for city in &mut game.cities {
        if city.owner == Owner::Comp {
            city.work += 1;
            let cost = city.prod.build_cost();
            if city.work >= cost {
                city.work = 0;
                let ptype = if city.prod == PieceType::Army { PieceType::Army } else { city.prod };

                let new_piece = crate::game::Piece {
                    id: game.pieces.len() + comp_new_pieces.len(),
                    ptype,
                    owner: Owner::Comp,
                    loc: city.loc,
                    hits: ptype.max_hits(),
                    moves_left: ptype.max_moves(),
                    cargo: Vec::new(),
                };

                log_messages.push(format!("Computer city ({}, {}) built {:?}!", city.loc.r, city.loc.c, ptype));
                comp_new_pieces.push(new_piece);
            }
        }
    }
    game.pieces.extend(comp_new_pieces);
    for msg in log_messages {
        game.log(msg);
    }

    for piece in &mut game.pieces {
        if piece.owner == Owner::Comp {
            piece.moves_left = piece.ptype.max_moves();
        }
    }

    let comp_piece_ids: Vec<usize> = game.pieces
        .iter()
        .filter(|p| p.owner == Owner::Comp)
        .map(|p| p.id)
        .collect();

    for id in comp_piece_ids {
        let Some(piece_idx) = game.pieces.iter().position(|p| p.id == id) else {
            continue;
        };

        while game.pieces[piece_idx].moves_left > 0 {
            let curr_loc = game.pieces[piece_idx].loc;
            let ptype = game.pieces[piece_idx].ptype;

            let directions = [
                (-1, 0), (1, 0), (0, 1), (0, -1),
                (-1, 1), (-1, -1), (1, 1), (1, -1)
            ];

            let mut chosen_move: Option<Loc> = None;
            let mut target_enemy_idx: Option<usize> = None;

            for (dr, dc) in directions {
                let nr = curr_loc.r as isize + dr;
                let nc = curr_loc.c as isize + dc;

                if nr >= 0 && nr < game.rows as isize && nc >= 0 && nc < game.cols as isize {
                    let target_loc = Loc { r: nr as usize, c: nc as usize };
                    let tile = game.map[target_loc.r][target_loc.c];
                    let is_city = game.cities.iter().any(|c| c.loc == target_loc);

                    if ptype.can_pass(tile, is_city) {
                        if let Some(e_idx) = game.pieces.iter().position(|p| p.loc == target_loc && p.owner == Owner::Player) {
                            target_enemy_idx = Some(e_idx);
                            chosen_move = Some(target_loc);
                            break;
                        } else if chosen_move.is_none() {
                            chosen_move = Some(target_loc);
                        }
                    }
                }
            }

            if let Some(target_loc) = chosen_move {
                if let Some(e_idx) = target_enemy_idx {
                    let victory = resolve_combat(game, piece_idx, e_idx);
                    if let Some(p_idx) = game.pieces.iter().position(|p| p.id == id) {
                        if victory {
                            game.pieces[p_idx].loc = target_loc;
                        }
                        game.pieces[p_idx].moves_left = 0;
                    }
                    break;
                } else {
                    if let Some(p_idx) = game.pieces.iter().position(|p| p.id == id) {
                        game.pieces[p_idx].loc = target_loc;
                        game.pieces[p_idx].moves_left -= 1;

                        if let Some(city) = game.cities.iter_mut().find(|c| c.loc == target_loc) {
                            if city.owner != Owner::Comp {
                                city.owner = Owner::Comp;
                                game.log(format!("Computer captured city at ({}, {})!", target_loc.r, target_loc.c));
                            }
                        }
                    }
                }
            } else {
                if let Some(p_idx) = game.pieces.iter().position(|p| p.id == id) {
                    game.pieces[p_idx].moves_left = 0;
                }
                break;
            }
        }
    }
}
