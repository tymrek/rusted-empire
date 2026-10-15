use crate::game::{Game, Owner, PieceType, Loc};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::disable_raw_mode;

pub fn handle_player_input(game: &mut Game) -> bool {
    let mut command_buffer = String::new();

    loop {
        crate::disp::refresh_display_with_input(game, &command_buffer);

        if let Ok(Event::Key(key_event)) = event::read() {
            // Check for Ctrl+C to exit gracefully
            if key_event.modifiers.contains(KeyModifiers::CONTROL) && key_event.code == KeyCode::Char('c') {
                let _ = disable_raw_mode();
                println!("\nExiting game via Ctrl+C.");
                std::process::exit(0);
            }

            match key_event.code {
                KeyCode::Enter => {
                    let line = command_buffer.trim().to_string();
                    command_buffer.clear();
                    
                    if line.is_empty() {
                        continue;
                    }

                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let cmd = parts[0].to_lowercase();

                    match cmd.as_str() {
                        "q" | "quit" => {
                            let _ = disable_raw_mode();
                            game.log("Exiting game.".to_string());
                            return false;
                        }
                        "n" | "next" => {
                            return true;
                        }
                        "s" | "save" => {
                            let filename = if parts.len() > 1 { parts[1] } else { "savegame.json" };
                            crate::save::save_game(game, filename);
                            game.log(format!("Game saved to '{}'.", filename));
                        }
                        "load" => {
                            let filename = if parts.len() > 1 { parts[1] } else { "savegame.json" };
                            match crate::save::load_game(filename) {
                                Ok(loaded) => {
                                    *game = loaded;
                                    game.log(format!("Game loaded from '{}'.", filename));
                                }
                                Err(e) => {
                                    game.log(format!("Load failed: {}", e));
                                }
                            }
                        }
                        "p" | "prod" => {
                            if parts.len() < 3 {
                                game.log("Usage: prod <city_idx> <type>".to_string());
                                continue;
                            }
                            let city_idx = match parts[1].parse::<usize>() {
                                Ok(idx) => idx,
                                Err(_) => {
                                    game.log("Invalid city index.".to_string());
                                    continue;
                                }
                            };

                            if city_idx >= game.cities.len() || game.cities[city_idx].owner != Owner::Player {
                                game.log("Invalid city selection.".to_string());
                                continue;
                            }

                            let ptype = match parts[2].to_lowercase().as_str() {
                                "army" => PieceType::Army,
                                "fighter" => PieceType::Fighter,
                                "patrol" | "patrolboat" => PieceType::PatrolBoat,
                                "destroyer" => PieceType::Destroyer,
                                "submarine" => PieceType::Submarine,
                                "carrier" => PieceType::Carrier,
                                "battleship" => PieceType::Battleship,
                                "transport" => PieceType::Transport,
                                "satellite" => PieceType::Satellite,
                                _ => {
                                    game.log("Unknown unit type option.".to_string());
                                    continue;
                                }
                            };

                            game.cities[city_idx].prod = ptype;
                            game.cities[city_idx].work = 0;
                            game.log(format!("City {} producing {:?}", city_idx, ptype));
                        }
                        "m" | "move" => {
                            if parts.len() < 3 {
                                game.log("Usage: move <unit_id> <dir (n/s/e/w/ne/nw/se/sw)>".to_string());
                                continue;
                            }
                            let id = match parts[1].parse::<usize>() {
                                Ok(val) => val,
                                Err(_) => {
                                    game.log("Invalid unit ID.".to_string());
                                    continue;
                                }
                            };

                            let dir = parts[2].to_lowercase();

                            if let Some(piece_idx) = game.pieces.iter().position(|p| p.id == id && p.owner == Owner::Player) {
                                if game.pieces[piece_idx].moves_left <= 0 {
                                    game.log(format!("Unit {} has no moves left.", id));
                                    continue;
                                }

                                let curr_loc = game.pieces[piece_idx].loc;
                                let (dr, dc) = match dir.as_str() {
                                    "n" | "up" => (-1, 0),
                                    "s" | "down" => (1, 0),
                                    "e" | "right" => (0, 1),
                                    "w" | "left" => (0, -1),
                                    "ne" => (-1, 1),
                                    "nw" => (-1, -1),
                                    "se" => (1, 1),
                                    "sw" => (1, -1),
                                    _ => {
                                        game.log("Invalid direction.".to_string());
                                        continue;
                                    }
                                };

                                let new_r = curr_loc.r as isize + dr;
                                let new_c = curr_loc.c as isize + dc;

                                if new_r < 0 || new_r >= game.rows as isize || new_c < 0 || new_c >= game.cols as isize {
                                    game.log("Move out of map bounds.".to_string());
                                    continue;
                                }

                                let target_loc = Loc { r: new_r as usize, c: new_c as usize };
                                let ptype = game.pieces[piece_idx].ptype;

                                let tile = game.map[target_loc.r][target_loc.c];
                                let is_city = game.cities.iter().any(|c| c.loc == target_loc);

                                if !ptype.can_pass(tile, is_city) {
                                    game.log(format!("Terrain impassable for {:?}.", ptype));
                                    continue;
                                }

                                if let Some(enemy_idx) = game.pieces.iter().position(|p| p.loc == target_loc && p.owner != game.pieces[piece_idx].owner) {
                                    let victory = crate::attack::resolve_combat(game, piece_idx, enemy_idx);
                                    if victory {
                                        game.pieces[piece_idx].loc = target_loc;
                                        game.pieces[piece_idx].moves_left -= 1;
                                        game.log(format!("Unit {} captured sector ({}, {}).", id, target_loc.r, target_loc.c));
                                    } else {
                                        game.pieces[piece_idx].moves_left = 0;
                                    }
                                    continue;
                                }

                                let mut boarded = false;
                                if ptype == PieceType::Army || ptype == PieceType::Fighter {
                                    if let Some(carrier_idx) = game.pieces.iter().position(|p| {
                                        p.id != game.pieces[piece_idx].id
                                            && p.owner == Owner::Player
                                            && p.loc == target_loc
                                            && p.ptype.can_carry(ptype)
                                    }) {
                                        let max_cargo = 4;
                                        if game.pieces[carrier_idx].cargo.len() < max_cargo {
                                            let mut loaded_piece = game.pieces.remove(piece_idx);
                                            loaded_piece.moves_left = 0;
                                            loaded_piece.loc = target_loc;
                                            game.pieces[carrier_idx].cargo.push(loaded_piece);
                                            game.log(format!("Unit {} boarded transport.", id));
                                            boarded = true;
                                        } else {
                                            game.log("Transport cargo hold is full!".to_string());
                                            continue;
                                        }
                                    }
                                }

                                if !boarded {
                                    game.pieces[piece_idx].loc = target_loc;
                                    game.pieces[piece_idx].moves_left -= 1;
                                    game.log(format!("Unit {} moved. Moves left: {}", id, game.pieces[piece_idx].moves_left));
                                }
                            } else {
                                game.log(format!("Player unit {} not found.", id));
                            }
                        }
                        "unload" => {
                            if parts.len() < 3 {
                                game.log("Usage: unload <transport_id> <dir>".to_string());
                                continue;
                            }
                            let id = match parts[1].parse::<usize>() {
                                Ok(val) => val,
                                Err(_) => {
                                    game.log("Invalid unit ID.".to_string());
                                    continue;
                                }
                            };
                            let dir = parts[2].to_lowercase();

                            if let Some(carrier_idx) = game.pieces.iter().position(|p| p.id == id && p.owner == Owner::Player) {
                                if game.pieces[carrier_idx].cargo.is_empty() {
                                    game.log("Transport has no cargo to unload.".to_string());
                                    continue;
                                }

                                let carrier_loc = game.pieces[carrier_idx].loc;
                                let (dr, dc) = match dir.as_str() {
                                    "n" | "up" => (-1, 0),
                                    "s" | "down" => (1, 0),
                                    "e" | "right" => (0, 1),
                                    "w" | "left" => (0, -1),
                                    _ => {
                                        game.log("Invalid unload direction.".to_string());
                                        continue;
                                    }
                                };

                                let unload_r = carrier_loc.r as isize + dr;
                                let unload_c = carrier_loc.c as isize + dc;
                                if unload_r < 0 || unload_r >= game.rows as isize || unload_c < 0 || unload_c >= game.cols as isize {
                                    game.log("Unload location out of bounds.".to_string());
                                    continue;
                                }

                                let target_loc = Loc { r: unload_r as usize, c: unload_c as usize };
                                let mut unlodable_cargo = game.pieces[carrier_idx].cargo.remove(0);
                                unlodable_cargo.loc = target_loc;
                                unlodable_cargo.moves_left = 1;
                                game.pieces.push(unlodable_cargo);
                                game.log(format!("Cargo unloaded to ({}, {}).", target_loc.r, target_loc.c));
                            } else {
                                game.log(format!("Transport unit {} not found.", id));
                            }
                        }
                        _ => {
                            game.log(format!("Unknown command: '{}'", cmd));
                        }
                    }
                }
                KeyCode::Backspace => {
                    command_buffer.pop();
                }
                KeyCode::Char(c) => {
                    command_buffer.push(c);
                }
                _ => {}
            }
        }
    }
}
