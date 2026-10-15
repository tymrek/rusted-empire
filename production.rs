use crate::game::{Game, Piece};

pub fn process_city_production(game: &mut Game) {
    let initial_pieces_len = game.pieces.len();
    let mut new_pieces = Vec::new();
    let mut log_messages = Vec::new();

    for city in &mut game.cities {
        city.work += 1;

        let cost = city.prod.build_cost();
        if city.work >= cost {
            city.work = 0;

            let new_piece = Piece {
                id: initial_pieces_len + new_pieces.len(),
                ptype: city.prod,
                owner: city.owner,
                loc: city.loc,
                hits: city.prod.max_hits(),
                moves_left: city.prod.max_moves(),
                cargo: Vec::new(),
            };

            log_messages.push(format!(
                "City at ({}, {}) built {:?} for {:?}!",
                city.loc.r, city.loc.c, city.prod, city.owner
            ));
            new_pieces.push(new_piece);
        }
    }

    game.pieces.extend(new_pieces);
    for msg in log_messages {
        game.log(msg);
    }
}
