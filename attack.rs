use crate::game::{Game, PieceType};

pub fn resolve_combat(game: &mut Game, attacker_idx: usize, defender_idx: usize) -> bool {
    let att_id = game.pieces[attacker_idx].id;
    let att_type = game.pieces[attacker_idx].ptype;
    let att_hits = game.pieces[attacker_idx].hits;

    let def_id = game.pieces[defender_idx].id;
    let def_type = game.pieces[defender_idx].ptype;
    let def_hits = game.pieces[defender_idx].hits;

    game.log(format!(
        "COMBAT: {:?}(#{}) engages {:?}(#{})!",
        att_type, att_id, def_type, def_id
    ));

    let att_power = get_combat_power(att_type) * att_hits;
    let def_power = get_combat_power(def_type) * def_hits;

    let attacker_damage = (def_power / 20).max(1);
    let defender_damage = (att_power / 20).max(1);

    let mut attacker_destroyed = false;
    if let Some(att_piece) = game.pieces.iter_mut().find(|p| p.id == att_id) {
        att_piece.hits -= attacker_damage;
        if att_piece.hits <= 0 {
            attacker_destroyed = true;
        }
    }

    let mut defender_destroyed = false;
    if let Some(def_piece) = game.pieces.iter_mut().find(|p| p.id == def_id) {
        def_piece.hits -= defender_damage;
        if def_piece.hits <= 0 {
            defender_destroyed = true;
        }
    }

    if attacker_destroyed {
        game.log(format!("-> Attacking {:?}(#{}) was destroyed!", att_type, att_id));
        game.pieces.retain(|p| p.id != att_id);
    } else {
        if let Some(att_piece) = game.pieces.iter().find(|p| p.id == att_id) {
            game.log(format!("-> Attacker survived with {} hits.", att_piece.hits));
        }
    }

    if defender_destroyed {
        game.log(format!("-> Defending {:?}(#{}) was destroyed!", def_type, def_id));
        game.pieces.retain(|p| p.id != def_id);
        true 
    } else {
        game.log(format!("-> Defender survived combat."));
        false
    }
}

fn ptype_strength(ptype: PieceType) -> i32 {
    match ptype {
        PieceType::Army => 4,
        PieceType::Fighter => 6,
        PieceType::PatrolBoat => 5,
        PieceType::Destroyer => 12,
        PieceType::Submarine => 10,
        PieceType::Carrier => 15,
        PieceType::Battleship => 25,
        PieceType::Transport => 8,
        PieceType::Satellite => 1,
    }
}

fn get_combat_power(ptype: PieceType) -> i32 {
    ptype_strength(ptype)
}
