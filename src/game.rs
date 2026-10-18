use crate::models::*;
use serde::{Serialize, Deserialize};
use rand::Rng;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub rows: usize,
    pub cols: usize,
    pub turn: i32,
    pub difficulty: u32,
    pub mode: GameMode,
    pub map: Vec<Vec<u8>>,
    pub comp_map: Vec<Vec<u8>>,
    pub player_map: Vec<Vec<u8>>,
    pub cities: Vec<City>,
    pub pieces: Vec<Piece>,
    pub messages: Vec<String>,
    pub msg_scroll: usize,          
    pub pending_cities: Vec<usize>, 
    pub edit_cursor: Loc,
    pub default_orders: Vec<(PieceType, UnitActivity)>,
    pub viewport_center: Option<Loc>,
    pub last_mouse_coord: Option<(u16, u16)>,
}

impl Game {
    pub fn new(rows: usize, cols: usize, difficulty: u32) -> Self {
        let map = vec![vec![b'.'; cols]; rows];
        let comp_map = vec![vec![0; cols]; rows];
        let player_map = vec![vec![0; cols]; rows];
        Self {
            rows,
            cols,
            turn: 1,
            difficulty,
            mode: GameMode::Manual,
            map,
            comp_map,
            player_map,
            cities: Vec::new(),
            pieces: Vec::new(),
            messages: Vec::new(),
            msg_scroll: 0,
            pending_cities: Vec::new(),
            edit_cursor: Loc { r: 0, c: 0 },
            viewport_center: None,
            last_mouse_coord: None,
            default_orders: vec![
                (PieceType::Army, UnitActivity::Idle),
                (PieceType::Fighter, UnitActivity::Idle),
                (PieceType::PatrolBoat, UnitActivity::Idle),
                (PieceType::Destroyer, UnitActivity::Idle),
                (PieceType::Submarine, UnitActivity::Idle),
                (PieceType::Carrier, UnitActivity::Idle),
                (PieceType::Battleship, UnitActivity::Idle),
                (PieceType::Transport, UnitActivity::Idle),
                (PieceType::Satellite, UnitActivity::Idle),
            ],
        }
    }

    pub fn generate_map(&mut self) {
        let mut rng = rand::thread_rng();
        let water_percent = 62; 
        let smooth_passes = 5;

        for r in 0..self.rows {
            for c in 0..self.cols {
                if rng.gen_range(0..100) < water_percent {
                    self.map[r][c] = b'.';
                } else {
                    self.map[r][c] = b'#';
                }
            }
        }

        for _ in 0..smooth_passes {
            let mut next_map = self.map.clone();
            for r in 1..self.rows - 1 {
                for c in 1..self.cols - 1 {
                    let mut land_count = 0;
                    for dr in -1..=1 {
                        for dc in -1..=1 {
                            if self.map[(r as i32 + dr) as usize][(c as i32 + dc) as usize] == b'#' {
                                land_count += 1;
                            }
                        }
                    }
                    
                    let is_land = self.map[r][c] == b'#';
                    if land_count >= 5 || (is_land && land_count >= 4) {
                        next_map[r][c] = b'#';
                    } else {
                        next_map[r][c] = b'.';
                    }
                }
            }
            self.map = next_map;
        }

        for r in 0..self.rows {
            self.map[r][0] = b'.';
            self.map[r][self.cols - 1] = b'.';
        }
        for c in 0..self.cols {
            self.map[0][c] = b'.';
            self.map[self.rows - 1][c] = b'.';
        }
    }

    pub fn dist_to_nearest_base(&self, loc: Loc) -> Option<i32> {
        let mut min_dist: Option<i32> = None;
        for city in &self.cities {
            if city.owner == Owner::Player {
                let dist = (loc.r as i32 - city.loc.r as i32).abs().max((loc.c as i32 - city.loc.c as i32).abs());
                min_dist = Some(min_dist.unwrap_or(dist).min(dist));
            }
        }
        for piece in &self.pieces {
            if piece.owner == Owner::Player && piece.ptype == PieceType::Carrier && piece.hits > 0 {
                let dist = (loc.r as i32 - piece.loc.r as i32).abs().max((loc.c as i32 - piece.loc.c as i32).abs());
                min_dist = Some(min_dist.unwrap_or(dist).min(dist));
            }
        }
        min_dist
    }

    pub fn is_visible_to(&self, target_idx: usize, viewer: Owner) -> bool {
        let target = &self.pieces[target_idx];
        if target.owner == viewer { return true; }
        
        if target.ptype != PieceType::Submarine {
            if viewer == Owner::Player {
                return self.player_map[target.loc.r][target.loc.c] != 0;
            }
            return true;
        }

        for p in &self.pieces {
            if p.owner == viewer && p.hits > 0 {
                let dr = (p.loc.r as isize - target.loc.r as isize).abs();
                let dc = (p.loc.c as isize - target.loc.c as isize).abs();
                let dist = std::cmp::max(dr, dc);

                if p.ptype == PieceType::Destroyer {
                    if dist <= p.ptype.sight_range() as isize { return true; }
                } else {
                    if dist <= 1 { return true; }
                }
            }
        }
        for c in &self.cities {
            if c.owner == viewer {
                let dr = (c.loc.r as isize - target.loc.r as isize).abs();
                let dc = (c.loc.c as isize - target.loc.c as isize).abs();
                let dist = std::cmp::max(dr, dc);
                if dist <= 1 { return true; }
            }
        }
        false
    }

    pub fn is_coastal(&self, loc: Loc) -> bool {
        for dr in -1..=1 {
            for dc in -1..=1 {
                let nr = loc.r as i32 + dr;
                let nc = loc.c as i32 + dc;
                if nr >= 0 && nr < self.rows as i32 && nc >= 0 && nc < self.cols as i32 {
                    if self.map[nr as usize][nc as usize] == b'.' {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn is_tile_blocked_by_friend(&self, loc: Loc, moving_ptype: PieceType, owner: Owner) -> bool {
        if self.cities.iter().any(|c| c.loc == loc && c.owner == owner) {
            return false;
        }
        let mut can_board = false;
        let mut has_blocker = false;
        for other in &self.pieces {
            if other.loc != loc || other.owner != owner || other.hits <= 0 { continue; }
            let is_fighter_flyover = (moving_ptype == PieceType::Fighter && other.ptype != PieceType::Fighter) || 
                                     (moving_ptype != PieceType::Fighter && other.ptype == PieceType::Fighter);
            let can_board_transport = moving_ptype == PieceType::Army && other.ptype == PieceType::Transport && other.cargo.len() < 6;
            let can_board_carrier = moving_ptype == PieceType::Fighter && other.ptype == PieceType::Carrier && other.cargo.len() < other.hits as usize;
            
            if can_board_transport || can_board_carrier {
                can_board = true;
            } else if !is_fighter_flyover {
                has_blocker = true;
            }
        }
        has_blocker && !can_board
    }

    pub fn setup_initial_states(&mut self) {
        let mut best_score = -1;
        let mut best_map = Vec::new();
        let mut best_cities = Vec::new();
        let mut best_continents = Vec::new();

        // Build 10 candidate worlds and evaluate them to find the best topology
        for _ in 0..10 {
            self.cities.clear();
            self.generate_map();

            let mut rng = rand::thread_rng();
            let mut land_tiles = Vec::new();

            for r in 1..self.rows - 1 {
                for c in 1..self.cols - 1 {
                    if self.map[r][c] == b'#' {
                        land_tiles.push(Loc { r, c });
                    }
                }
            }

            if land_tiles.is_empty() {
                self.map[self.rows / 2][self.cols / 2] = b'#';
                land_tiles.push(Loc { r: self.rows / 2, c: self.cols / 2 });
            }

            for i in 0..land_tiles.len() {
                let j = rng.gen_range(0..land_tiles.len());
                land_tiles.swap(i, j);
            }

            let target_cities = (self.rows * self.cols) / 85;
            let mut chosen_city_locs: Vec<Loc> = Vec::new();

            for loc in land_tiles {
                if chosen_city_locs.len() >= target_cities { break; }
                let mut too_close = false;
                for &c_loc in &chosen_city_locs {
                    let dr = (c_loc.r as isize - loc.r as isize).abs();
                    let dc = (c_loc.c as isize - loc.c as isize).abs();
                    if dr < 6 && dc < 6 { too_close = true; break; }
                }
                if !too_close { chosen_city_locs.push(loc); }
            }

            for loc in chosen_city_locs {
                self.cities.push(City {
                    loc, owner: Owner::Neutral, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
                });
            }

            let mut visited_land = vec![vec![false; self.cols]; self.rows];
            let mut continents: Vec<Vec<usize>> = Vec::new();

            for (_c_idx, city) in self.cities.iter().enumerate() {
                if visited_land[city.loc.r][city.loc.c] { continue; }
                
                let mut continent_cities = Vec::new();
                let mut queue = std::collections::VecDeque::new();
                
                queue.push_back(city.loc);
                visited_land[city.loc.r][city.loc.c] = true;

                while let Some(loc) = queue.pop_front() {
                    if let Some(idx) = self.cities.iter().position(|c| c.loc == loc) {
                        if !continent_cities.contains(&idx) { continent_cities.push(idx); }
                    }

                    for dr in -1..=1 {
                        for dc in -1..=1 {
                            let nr = (loc.r as i32 + dr) as usize;
                            let nc = (loc.c as i32 + dc) as usize;
                            if nr < self.rows && nc < self.cols && !visited_land[nr][nc] && self.map[nr][nc] == b'#' {
                                visited_land[nr][nc] = true;
                                queue.push_back(Loc { r: nr, c: nc });
                            }
                        }
                    }
                }
                if !continent_cities.is_empty() { continents.push(continent_cities); }
            }

            continents.sort_by(|a, b| b.len().cmp(&a.len()));
            let valid_continents: Vec<Vec<usize>> = continents.iter().cloned().filter(|cities| cities.len() >= 2).collect();

            // Fitness score: prioritize maps with a large 2nd and 3rd continent to guarantee good starts for both sides
            let score = if valid_continents.len() >= 2 {
                let c1 = valid_continents[0].len() as i32;
                let c2 = valid_continents[1].len() as i32;
                let c3 = if valid_continents.len() > 2 { valid_continents[2].len() as i32 } else { 0 };
                (c2 * 10) + (c3 * 5) + c1
            } else {
                -1
            };

            if score > best_score {
                best_score = score;
                best_map = self.map.clone();
                best_cities = self.cities.clone();
                best_continents = valid_continents;
            }
        }

        // Apply the best map candidate found
        self.map = best_map;
        self.cities = best_cities;
        let mut valid_continents = best_continents;

        if valid_continents.is_empty() {
            valid_continents.push((0..self.cities.len()).collect());
        }

        // Perfect 1 to 100 Linear Scale
        let diff_ratio = (self.difficulty.clamp(1, 100) - 1) as f32 / 99.0;
        
        let p_cont_idx = (diff_ratio * (valid_continents.len() - 1) as f32).round() as usize;
        let p_continent = &valid_continents[p_cont_idx];

        let p_cap_idx = *p_continent.iter()
            .find(|&&idx| self.is_coastal(self.cities[idx].loc))
            .unwrap_or(&p_continent[0]);
            
        self.cities[p_cap_idx].owner = Owner::Player;

        let mut c_cont_idx = ((1.0 - diff_ratio) * (valid_continents.len() - 1) as f32).round() as usize;
        
        // Force the competitors apart if they roll the exact same continent at difficulty 50
        if c_cont_idx == p_cont_idx && valid_continents.len() > 1 {
            if diff_ratio < 0.5 {
                c_cont_idx = (c_cont_idx + 1).min(valid_continents.len() - 1);
            } else {
                c_cont_idx = c_cont_idx.saturating_sub(1);
            }
        }
        
        let c_continent = &valid_continents[c_cont_idx];
        let mut rng = rand::thread_rng();
        let c_cap_idx = c_continent[rng.gen_range(0..c_continent.len())];
        self.cities[c_cap_idx].owner = Owner::Comp;

        let player_capitals: Vec<Loc> = self.cities.iter()
            .filter(|c| c.owner == Owner::Player)
            .map(|c| c.loc)
            .collect();

        for loc in player_capitals {
            self.reveal_vision(loc, 6);
        }

        self.log(format!("Classic VMS-Empire map generated. Difficulty set to {}.", self.difficulty));
        self.log(String::from("Game initialized. 70 cities scattered across the world."));
    }

    pub fn attempt_move(&mut self, piece_idx: usize, target_loc: Loc) -> (MoveResult, String) {
        let ptype = self.pieces[piece_idx].ptype;
        let is_land = self.map[target_loc.r][target_loc.c] == b'#';
        
        let unit_owner = self.pieces[piece_idx].owner;
        let is_player = unit_owner == Owner::Player;

        let is_friendly_city = self.cities.iter().any(|c| c.loc == target_loc && c.owner == unit_owner);
        let can_enter = match ptype {
            PieceType::Army => is_land || self.pieces.iter().any(|p| p.loc == target_loc && p.owner == unit_owner && p.ptype == PieceType::Transport && p.hits > 0),
            _ => !is_land || is_friendly_city, 
        };

        let is_bombardment = !can_enter 
            && (ptype == PieceType::Battleship || ptype == PieceType::Destroyer) 
            && (self.pieces.iter().any(|p| p.loc == target_loc && p.owner != unit_owner && p.hits > 0) 
                || self.cities.iter().any(|c| c.loc == target_loc && c.owner != unit_owner));

        if !can_enter && ptype != PieceType::Fighter && !is_bombardment {
            return (MoveResult::InvalidMove, if is_player { String::from("Terrain impassable for this unit type.") } else { String::new() });
        }

        let mut enemy_idx = None;
        let mut boardable_carrier_idx = None;
        let mut blocked_by_friend = false;

        if ptype != PieceType::Satellite {
            for (i, other) in self.pieces.iter().enumerate() {
                if i != piece_idx && other.loc == target_loc && other.hits > 0 {
                    if other.owner == unit_owner {
                        let can_board_transport = ptype == PieceType::Army && other.ptype == PieceType::Transport && other.cargo.len() < 6;
                        let can_board_carrier = ptype == PieceType::Fighter && other.ptype == PieceType::Carrier && other.cargo.len() < other.hits as usize;
                        let is_fighter_flyover = (ptype == PieceType::Fighter && other.ptype != PieceType::Fighter) || 
                                                 (ptype != PieceType::Fighter && other.ptype == PieceType::Fighter);

                        if can_board_transport || can_board_carrier {
                            boardable_carrier_idx = Some(i);
                        } else if !is_friendly_city && !is_fighter_flyover {
                            blocked_by_friend = true;
                        }
                    } else {
                        enemy_idx = Some(i);
                    }
                }
            }
        }

        // Always prioritize boarding if an available transport exists, overriding deck collision
        if let Some(carrier_idx) = boardable_carrier_idx {
            let mut boarded_piece = self.pieces[piece_idx].clone();
            boarded_piece.loc = target_loc;
            boarded_piece.moves_left = 0;
            self.pieces[carrier_idx].cargo.push(boarded_piece);
            self.pieces[piece_idx].hits = 0;
            
            let msg = if is_player { format!("Your {:?} successfully boarded the {:?}.", ptype, self.pieces[carrier_idx].ptype) } else { String::new() };
            return (MoveResult::Moved, msg);
        }

        if blocked_by_friend {
            return (MoveResult::InvalidMove, if is_player { String::from("Sector is occupied by a friendly unit.") } else { String::new() });
        }

        if let Some(e_idx) = enemy_idx {
            let mut rng = rand::thread_rng();
            let e_type = self.pieces[e_idx].ptype;
            let p_damage = self.pieces[piece_idx].ptype.strike_damage();
            let e_damage = e_type.strike_damage();
            
            while self.pieces[piece_idx].hits > 0 && self.pieces[e_idx].hits > 0 {
                if rng.gen_bool(0.5) { self.pieces[e_idx].hits -= p_damage; }
                if self.pieces[e_idx].hits > 0 && rng.gen_bool(0.5) { self.pieces[piece_idx].hits -= e_damage; }
            }

            let attacked_player = self.pieces[e_idx].owner == Owner::Player;

            if self.pieces[piece_idx].hits <= 0 {
                let msg = if is_player { format!(">>> DEFEAT! Your {:?} was DESTROYED by enemy {:?}! <<<", ptype, e_type) } 
                          else if attacked_player { format!(">>> DEFENSE! Your {:?} DESTROYED attacking enemy {:?}! <<<", e_type, ptype) } 
                          else { String::new() };
                return (MoveResult::CombatLost, msg);
            } else {
                if !is_bombardment {
                    let old_loc = self.pieces[piece_idx].loc;
                    self.pieces[piece_idx].loc = target_loc;
                    
                    // Combat Drag Logic: Ensure deck units ride along if a transport attacks and occupies a tile
                    if ptype == PieceType::Transport || ptype == PieceType::Carrier {
                        for i in 0..self.pieces.len() {
                            if i != piece_idx && self.pieces[i].loc == old_loc && self.pieces[i].owner == unit_owner && self.pieces[i].hits > 0 {
                                let optype = self.pieces[i].ptype;
                                if (ptype == PieceType::Transport && optype == PieceType::Army) ||
                                   (ptype == PieceType::Carrier && optype == PieceType::Fighter) {
                                    self.pieces[i].loc = target_loc;
                                }
                            }
                        }
                    }
                }
                self.pieces[piece_idx].moves_left -= 1;
                
                if is_player {
                    self.reveal_vision(target_loc, ptype.sight_range());
                }

                if ptype == PieceType::Army {
                    for (c_idx, city) in self.cities.iter_mut().enumerate() {
                        if city.loc == target_loc && city.owner != unit_owner {
                            city.owner = unit_owner;
                            self.pieces[piece_idx].hits = 0; 
                            let cap_msg = if is_player {
                                format!(">>> VICTORY & CITY CAPTURED! You took ({},{})! <<<", target_loc.r, target_loc.c)
                            } else if attacked_player {
                                format!(">>> ALERT! The enemy took your city at ({},{})! <<<", target_loc.r, target_loc.c)
                            } else { String::new() };
                            return (MoveResult::CityCaptured(c_idx), cap_msg);
                        }
                    }
                }
                
                let win_msg = if is_player && is_bombardment { format!(">>> BOMBARDMENT VICTORY! Your {:?} DESTROYED enemy {:?}! <<<", ptype, e_type) }
                              else if is_player { format!(">>> VICTORY! Your {:?} DESTROYED enemy {:?}! <<<", ptype, e_type) }
                              else if attacked_player { format!(">>> ALERT! Your {:?} was DESTROYED by enemy {:?}! <<<", e_type, ptype) }
                              else { String::new() };
                return (MoveResult::CombatWon, win_msg);
            }
        }

        if is_bombardment {
            for city in self.cities.iter_mut() {
                if city.loc == target_loc && city.owner != unit_owner {
                    city.work = 0;
                    self.pieces[piece_idx].moves_left -= 1;
                    if is_player {
                        self.reveal_vision(target_loc, ptype.sight_range());
                        return (MoveResult::Moved, format!(">>> BOMBARDMENT! Your {:?} shelled the city at ({},{}), resetting its production! <<<", ptype, target_loc.r, target_loc.c));
                    } else if city.owner == Owner::Player {
                        return (MoveResult::Moved, format!(">>> ALERT! Enemy {:?} shelled your city at ({},{}), resetting production! <<<", ptype, target_loc.r, target_loc.c));
                    }
                    return (MoveResult::Moved, String::new());
                }
            }
        }

        if ptype == PieceType::Army {
            for (c_idx, city) in self.cities.iter_mut().enumerate() {
                if city.loc == target_loc && city.owner != unit_owner { 
                    let mut rng = rand::thread_rng();
                    let was_friendly = city.owner == Owner::Player;
                    
                    self.pieces[piece_idx].hits = 0; 

                    if rng.gen_bool(0.5) {
                        city.owner = unit_owner;
                        
                        if is_player {
                            self.reveal_vision(target_loc, ptype.sight_range());
                            if was_friendly {
                                return (MoveResult::CityCaptured(c_idx), format!(">>> YOU ATTACKED YOUR OWN CITY! Army destroyed, but the city survived at ({},{})! <<<", target_loc.r, target_loc.c));
                            } else {
                                return (MoveResult::CityCaptured(c_idx), format!(">>> CITY CAPTURED! You took ({},{})! <<<", target_loc.r, target_loc.c));
                            }
                        } else {
                            if was_friendly {
                                return (MoveResult::CityCaptured(c_idx), format!(">>> ALERT! The enemy captured your city at ({},{})! <<<", target_loc.r, target_loc.c));
                            }
                            return (MoveResult::CityCaptured(c_idx), String::new());
                        }
                    } else {
                        if is_player {
                            return (MoveResult::CityAssaultFailed, format!(">>> ASSAULT FAILED! Army DESTROYED at ({},{})! <<<", target_loc.r, target_loc.c));
                        } else if was_friendly {
                            return (MoveResult::CityAssaultFailed, format!(">>> DEFENSE! Enemy army DESTROYED attacking your city at ({},{})! <<<", target_loc.r, target_loc.c));
                        }
                        return (MoveResult::CityAssaultFailed, String::new());
                    }
                }
            }
        }

        let old_loc = self.pieces[piece_idx].loc;
        self.pieces[piece_idx].loc = target_loc;
        self.pieces[piece_idx].moves_left -= 1;
        let mut move_msg = String::new();

        // Mobile Sector Drag Logic: Pull any awake units sharing the deck to the new tile
        if ptype == PieceType::Transport || ptype == PieceType::Carrier {
            for i in 0..self.pieces.len() {
                if i != piece_idx && self.pieces[i].loc == old_loc && self.pieces[i].owner == unit_owner && self.pieces[i].hits > 0 {
                    let optype = self.pieces[i].ptype;
                    if (ptype == PieceType::Transport && optype == PieceType::Army) ||
                       (ptype == PieceType::Carrier && optype == PieceType::Fighter) {
                        self.pieces[i].loc = target_loc;
                    }
                }
            }
        }

        if ptype == PieceType::Fighter {
            self.pieces[piece_idx].fuel -= 1;
            let at_base = self.cities.iter().any(|c| c.loc == target_loc && c.owner == unit_owner);
            let at_carrier = self.pieces.iter().any(|p| p.loc == target_loc && p.owner == unit_owner && p.ptype == PieceType::Carrier && p.hits > 0);
            
            if at_base || at_carrier {
                self.pieces[piece_idx].fuel = 32;
            } else if self.pieces[piece_idx].fuel <= 0 {
                self.pieces[piece_idx].hits = 0;
                let msg = if is_player {
                    format!(">>> MAYDAY! Your Fighter at ({},{}) ran out of fuel and crashed! <<<", target_loc.r, target_loc.c)
                } else {
                    String::new()
                };
                return (MoveResult::CombatLost, msg);
            }
        }

        if is_friendly_city && ptype != PieceType::Satellite {
            if let Some(city) = self.cities.iter().find(|c| c.loc == target_loc && c.owner == unit_owner) {
                let assigned_order = city.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a)
                    .unwrap_or_else(|| self.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a).unwrap_or(UnitActivity::Idle));
                
                self.pieces[piece_idx].activity = assigned_order;
                
                if is_player {
                    move_msg = format!("{:?} entered base at ({},{}) and acquired orders: {:?}", ptype, target_loc.r, target_loc.c, assigned_order);
                }
            }
        }
        
        if is_player {
            self.reveal_vision(target_loc, ptype.sight_range());
        }
        
        (MoveResult::Moved, move_msg)
    }

    pub fn explore_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut explore_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let ptype = self.pieces[piece_idx].ptype;
            let current_loc = self.pieces[piece_idx].loc;
            
            if ptype == PieceType::Fighter {
                if let Some(dist) = self.dist_to_nearest_base(current_loc) {
                    if self.pieces[piece_idx].fuel <= dist + 2 {
                        self.pieces[piece_idx].activity = UnitActivity::Land;
                        self.pieces[piece_idx].moves_left = 0;
                        explore_logs.push(format!("Fighter at ({},{}) reached bingo fuel ({} left). Aborting grope and returning to base.", current_loc.r, current_loc.c, self.pieces[piece_idx].fuel));
                        break; 
                    }
                }
            }

            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];

            let mut valid_steps = Vec::new();
            for &dir in &dirs {
                let target_r = (current_loc.r as i32 + dir.0) as usize;
                let target_c = (current_loc.c as i32 + dir.1) as usize;

                if target_r < self.rows && target_c < self.cols {
                    let is_land = self.map[target_r][target_c] == b'#';
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || ptype == PieceType::Fighter {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player) {
                            valid_steps.push(target_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let mut chosen_step = valid_steps[0];
            let mut found_fog = false;

            let mut queue = std::collections::VecDeque::new();
            let mut visited = vec![vec![false; self.cols]; self.rows];

            visited[current_loc.r][current_loc.c] = true;

            for &step in &valid_steps {
                visited[step.r][step.c] = true;
                queue.push_back((step, step)); 
            }

            while let Some((loc, initial_step)) = queue.pop_front() {
                if self.player_map[loc.r][loc.c] == 0 {
                    chosen_step = initial_step;
                    found_fog = true;
                    break;
                }

                for &dir in &dirs {
                    let target_r = (loc.r as i32 + dir.0) as usize;
                    let target_c = (loc.c as i32 + dir.1) as usize;

                    if target_r < self.rows && target_c < self.cols {
                        if !visited[target_r][target_c] {
                            let is_land = self.map[target_r][target_c] == b'#';
                            let can_enter = match ptype {
                                PieceType::Army => is_land,
                                PieceType::Fighter | PieceType::Satellite => true,
                                _ => !is_land,
                            };

                            if can_enter {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
                            } else if self.player_map[target_r][target_c] == 0 {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
                            }
                        }
                    }
                }
            }
            if !found_fog {
                let on_friendly_city = self.cities.iter().any(|c| c.loc == current_loc && c.owner == Owner::Player);
                
                if ptype == PieceType::Army && on_friendly_city {
                    self.pieces[piece_idx].activity = UnitActivity::Idle;
                    explore_logs.push(format!("{:?} cannot automate inside a city. Orders rejected!", ptype));
                    break; 
                }

                self.pieces[piece_idx].activity = UnitActivity::Idle;
                explore_logs.push(format!("{:?} has nothing left to explore or attack and woke up.", ptype));
                
                self.pieces[piece_idx].moves_left = 0;
                break;
            }
            
            let (result, msg) = self.attempt_move(piece_idx, chosen_step);
            if !msg.is_empty() {
                explore_logs.push(msg);
            }
            
            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed | MoveResult::InvalidMove => break,
                MoveResult::CityCaptured(c_idx) => {
                    self.pending_cities.push(c_idx);
                    break;
                }
                MoveResult::Moved | MoveResult::CombatWon => {}
            }
        }

        for msg in explore_logs {
            self.log(msg);
        }
    }

    pub fn process_comp_ai(&mut self) {
        use std::collections::VecDeque;

        // 1. Initialize Dual Flow Fields
        let mut attack_map = vec![vec![i32::MAX; self.cols]; self.rows];
        let mut attack_queue = VecDeque::new();
        
        let mut ferry_map = vec![vec![i32::MAX; self.cols]; self.rows];
        let mut ferry_queue = VecDeque::new();

        // 2. Seed the Targets
        for city in &self.cities {
            if city.owner != Owner::Comp {
                attack_map[city.loc.r][city.loc.c] = 0;
                attack_queue.push_back((city.loc.r, city.loc.c, 0));
            } else {
                ferry_map[city.loc.r][city.loc.c] = 0;
                ferry_queue.push_back((city.loc.r, city.loc.c, 0));
            }
        }
        for p in &self.pieces {
            if p.hits > 0 {
                if p.owner == Owner::Player {
                    attack_map[p.loc.r][p.loc.c] = 0;
                    attack_queue.push_back((p.loc.r, p.loc.c, 0));
                } else if p.owner == Owner::Comp && p.ptype == PieceType::Army {
                    // Seed ferries to target Armies that are NOT already on a boat
                    let on_transport = self.pieces.iter().any(|t| t.ptype == PieceType::Transport && t.loc == p.loc && t.id != p.id);
                    if !on_transport {
                        ferry_map[p.loc.r][p.loc.c] = 0;
                        ferry_queue.push_back((p.loc.r, p.loc.c, 0));
                    }
                }
            }
        }

        let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];
        
        // 3. Flood Fill the Distances (The Wave)
        while let Some((r, c, d)) = attack_queue.pop_front() {
            for &(dr, dc) in &dirs {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nr < self.rows as i32 && nc >= 0 && nc < self.cols as i32 {
                    let nr = nr as usize;
                    let nc = nc as usize;
                    if attack_map[nr][nc] > d + 1 {
                        attack_map[nr][nc] = d + 1;
                        attack_queue.push_back((nr, nc, d + 1));
                    }
                }
            }
        }

        while let Some((r, c, d)) = ferry_queue.pop_front() {
            for &(dr, dc) in &dirs {
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nr < self.rows as i32 && nc >= 0 && nc < self.cols as i32 {
                    let nr = nr as usize;
                    let nc = nc as usize;
                    if ferry_map[nr][nc] > d + 1 {
                        ferry_map[nr][nc] = d + 1;
                        ferry_queue.push_back((nr, nc, d + 1));
                    }
                }
            }
        }

        // 4. Units Roll Downhill
        let comp_indices: Vec<usize> = self.pieces.iter().enumerate()
            .filter(|(_, p)| p.owner == Owner::Comp && p.hits > 0)
            .map(|(i, _)| i)
            .collect();

        for idx in comp_indices {
            let mut visited = vec![self.pieces[idx].loc];

            while self.pieces[idx].moves_left > 0 && self.pieces[idx].hits > 0 {
                let p_loc = self.pieces[idx].loc;
                let ptype = self.pieces[idx].ptype;

                // Empty transports go home to ferry. Everyone else attacks.
                let map_to_use = if ptype == PieceType::Transport && self.pieces[idx].cargo.is_empty() {
                    &ferry_map
                } else {
                    &attack_map
                };

                let mut best_step = None;
                let mut best_dist = map_to_use[p_loc.r][p_loc.c];

                for &dir in &dirs {
                    let tr = p_loc.r as i32 + dir.0;
                    let tc = p_loc.c as i32 + dir.1;
                    if tr >= 0 && tr < self.rows as i32 && tc >= 0 && tc < self.cols as i32 {
                        let next_loc = Loc { r: tr as usize, c: tc as usize };
                        if visited.contains(&next_loc) { continue; }

                        let is_land = self.map[next_loc.r][next_loc.c] == b'#';
                        let can_enter = match ptype {
                            PieceType::Army => is_land || self.pieces.iter().any(|p| p.loc == next_loc && p.owner == Owner::Comp && p.ptype == PieceType::Transport && p.hits > 0 && p.cargo.len() < 6),
                            PieceType::Fighter | PieceType::Satellite => true,
                            _ => !is_land || self.cities.iter().any(|c| c.loc == next_loc),
                        };

                        if can_enter && !self.is_tile_blocked_by_friend(next_loc, ptype, Owner::Comp) {
                            let d = map_to_use[next_loc.r][next_loc.c];
                            // Unit must strictly step downhill
                            if d < best_dist {
                                best_dist = d;
                                best_step = Some(next_loc);
                            }
                        }
                    }
                }

                if let Some(step) = best_step {
                    visited.push(step);
                    let old_loc = self.pieces[idx].loc;
                    let _ = self.attempt_move(idx, step);
                    if self.pieces[idx].hits == 0 { break; }
                    if self.pieces[idx].loc == old_loc { 
                        // Unit failed to move (bounced off a blocker or invalid state), consume moves to prevent infinite loops
                        self.pieces[idx].moves_left = 0;
                        break; 
                    }
                } else {
                    self.pieces[idx].moves_left = 0;
                    break;
                }
            }
        }
    }

    pub fn check_adjacent_unloads(&mut self) {
        let mut to_wake = Vec::new();
        for (i, p) in self.pieces.iter().enumerate() {
            // Only automatically wake cargo for Computer transports.
            // Player transports hold cargo until the player manually wakes [W] them.
            if p.ptype == PieceType::Transport && !p.cargo.is_empty() && p.hits > 0 && p.owner == Owner::Comp {
                let mut should_unload = false;
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        let r = p.loc.r as i32 + dr;
                        let c = p.loc.c as i32 + dc;
                        if r >= 0 && r < self.rows as i32 && c >= 0 && c < self.cols as i32 {
                            let loc_rc = Loc { r: r as usize, c: c as usize };
                            if self.map[loc_rc.r][loc_rc.c] == b'#' {
                                if p.owner == Owner::Player {
                                    should_unload = true;
                                } else {
                                    let near_enemy_city = self.cities.iter().any(|ct| ct.loc == loc_rc && ct.owner != Owner::Comp);
                                    let near_friendly_city = self.cities.iter().any(|ct| ct.loc == loc_rc && ct.owner == Owner::Comp);
                                    if near_enemy_city || !near_friendly_city {
                                        should_unload = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if should_unload {
                    to_wake.push(i);
                }
            }
        }
        for i in to_wake {
            self.wake_cargo(i);
        }
    }

    pub fn process_turn(&mut self) {
        self.turn += 1;
        self.log(format!("Advanced to Turn {}.", self.turn));

        let mut new_pieces = Vec::new();
        let mut logs_to_add = Vec::new();

        for (idx, city) in self.cities.iter_mut().enumerate() {
            if city.owner == Owner::Neutral {
                continue;
            }

            city.work += 1;
            let cost = city.prod.build_cost();

            if city.work >= cost {
                city.work = 0;
                
                if city.owner == Owner::Player {
                    logs_to_add.push(format!(
                        "City [C{}] ({},{}) finished building {:?}!",
                        idx, city.loc.r, city.loc.c, city.prod
                    ));
                }

                let max_id = self.pieces.iter().map(|p| p.id).max().unwrap_or(0);
                let new_id = max_id + 1 + new_pieces.len();
                
                let starting_activity = if city.owner == Owner::Player {
                    city.default_orders.iter().find(|(p, _)| *p == city.prod).map(|(_, a)| *a)
                        .unwrap_or_else(|| self.default_orders.iter().find(|(p, _)| *p == city.prod).map(|(_, a)| *a).unwrap_or(UnitActivity::Idle))
                } else {
                    UnitActivity::Explore
                };

                new_pieces.push(Piece {
                    id: new_id,
                    ptype: city.prod,
                    owner: city.owner,
                    loc: city.loc,
                    hits: city.prod.max_hits(),
                    moves_left: city.prod.max_moves(),
                    activity: starting_activity,
                    cargo: Vec::new(),
                    fuel: match city.prod {
                        PieceType::Fighter => 32,
                        PieceType::Satellite => 20, 
                        _ => 0,
                    },
                });

                if city.owner == Owner::Comp {
                    let mut is_coastal = false;
                    for dr in -1..=1 {
                        for dc in -1..=1 {
                            let nr = city.loc.r as i32 + dr;
                            let nc = city.loc.c as i32 + dc;
                            if nr >= 0 && nr < self.rows as i32 && nc >= 0 && nc < self.cols as i32 {
                                if self.map[nr as usize][nc as usize] == b'.' { is_coastal = true; }
                            }
                        }
                    }
                    let mut rng = rand::thread_rng();
                    if is_coastal {
                        let roll = rng.gen_range(0..100);
                        city.prod = if roll < 30 { PieceType::Army }
                                    else if roll < 60 { PieceType::Transport }
                                    else if roll < 80 { PieceType::Battleship }
                                    else { PieceType::Fighter };
                    } else {
                        city.prod = if rng.gen_bool(0.7) { PieceType::Army } else { PieceType::Fighter };
                    }
                }
            }
        }

        for msg in logs_to_add {
            self.log(msg);
        }
        self.pieces.extend(new_pieces);

        let mut auto_explore_indices = Vec::new();
        let mut auto_attack_indices = Vec::new();
        let mut auto_random_indices = Vec::new();
        let mut auto_dest_indices = Vec::new();
        let mut auto_dir_indices = Vec::new();
        let mut auto_fill_indices = Vec::new();
        let mut auto_upgrade_indices = Vec::new();
        let mut auto_land_indices = Vec::new();
        let mut wakeup_logs = Vec::new();

        let enemy_locs: Vec<Loc> = self.pieces.iter().enumerate()
            .filter(|(i, p)| p.owner != Owner::Player && p.hits > 0 && self.is_visible_to(*i, Owner::Player))
            .map(|(_, p)| p.loc)
            .collect();

        for (idx, p) in self.pieces.iter_mut().enumerate() {
            if p.owner == Owner::Player || p.owner == Owner::Comp {
                if p.moves_left <= 0 {
                    p.moves_left = p.ptype.max_moves();
                }

                let at_friendly_city = self.cities.iter().any(|c| c.loc == p.loc && c.owner == p.owner);
                if at_friendly_city && matches!(p.activity, UnitActivity::Sentry | UnitActivity::Fill | UnitActivity::TransportWait | UnitActivity::Upgrade) {
                    if p.hits < p.ptype.max_hits() {
                        p.hits += 1;
                    }
                }
            }

            if p.owner == Owner::Player {

                if p.activity == UnitActivity::Sentry {
                    let sight = p.ptype.sight_range() as f64;
                    let mut should_wake = false;
                    
                    for &eloc in &enemy_locs {
                        let dist = ((eloc.r as f64 - p.loc.r as f64).powi(2) + (eloc.c as f64 - p.loc.c as f64).powi(2)).sqrt();
                        if dist <= sight {
                            should_wake = true;
                            break;
                        }
                    }
                    
                    if should_wake {
                        if p.ptype == PieceType::Fighter {
                            p.activity = UnitActivity::Attack;
                            wakeup_logs.push(format!("CAP INTERCEPT: Fighter on sentry at ({},{}) scrambled to attack!", p.loc.r, p.loc.c));
                        } else {
                            p.activity = UnitActivity::Idle;
                            wakeup_logs.push(format!("{:?} on sentry duty at ({},{}) spotted an enemy and woke up!", p.ptype, p.loc.r, p.loc.c));
                        }
                    }
                }

                if p.activity == UnitActivity::Explore {
                    auto_explore_indices.push(idx);
                } else if p.activity == UnitActivity::Attack {
                    auto_attack_indices.push(idx);
                } else if p.activity == UnitActivity::Random {
                    auto_random_indices.push(idx);
                } else if matches!(p.activity, UnitActivity::Dest(_)) {
                    auto_dest_indices.push(idx);
                } else if matches!(p.activity, UnitActivity::Dir(_, _)) {
                    auto_dir_indices.push(idx);
                } else if p.activity == UnitActivity::Fill {
                    auto_fill_indices.push(idx);
                } else if p.activity == UnitActivity::Upgrade {
                    auto_upgrade_indices.push(idx);
                } else if p.activity == UnitActivity::Land {
                    auto_land_indices.push(idx);
                }
            }
        }

        for msg in wakeup_logs {
            self.log(msg);
        }

        for idx in auto_explore_indices {
            self.explore_piece(idx);
        }
        for idx in auto_attack_indices {
            self.attack_piece(idx);
        }
        for idx in auto_random_indices {
            self.random_piece(idx);
        }
        for idx in auto_dest_indices {
            self.dest_piece(idx);
        }
        for idx in auto_dir_indices {
            self.dir_piece(idx);
        }
        for idx in auto_fill_indices {
            self.process_fill_mode(idx);
        }
        for idx in auto_upgrade_indices {
            self.process_upgrade_mode(idx);
        }
        for idx in auto_land_indices {
            self.process_land_mode(idx);
        }

        self.process_comp_ai();
        self.check_adjacent_unloads();

        let mut dead_sats = Vec::new();
        for p in self.pieces.iter_mut() {
            if p.ptype == PieceType::Satellite && p.hits > 0 {
                p.fuel -= 1; 
                if p.fuel <= 0 {
                    p.hits = 0;
                    dead_sats.push(format!("Satellite at ({},{}) burned up in the atmosphere.", p.loc.r, p.loc.c));
                }
            }
        }
        for msg in dead_sats {
            self.log(msg);
        }

        self.pieces.retain(|p| p.hits > 0);
    }
    pub fn attack_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut attack_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let enemy_locs: Vec<Loc> = self.pieces.iter().enumerate()
                .filter(|(i, p)| p.owner != Owner::Player && p.hits > 0 && self.is_visible_to(*i, Owner::Player))
                .map(|(_, p)| p.loc)
                .collect();

            let target_city_locs: Vec<Loc> = self.cities.iter()
                .filter(|c| c.owner != Owner::Player && self.player_map[c.loc.r][c.loc.c] != 0)
                .map(|c| c.loc)
                .collect();

            let mut all_targets = enemy_locs;
            all_targets.extend(target_city_locs);

            let p = &self.pieces[piece_idx];
            let current_loc = p.loc;
            let ptype = p.ptype;

            if ptype == PieceType::Fighter {
                if let Some(dist) = self.dist_to_nearest_base(current_loc) {
                    if self.pieces[piece_idx].fuel <= dist + 2 {
                        self.pieces[piece_idx].activity = UnitActivity::Land;
                        attack_logs.push(format!("Fighter at ({},{}) reached bingo fuel. Aborting attack and returning to base.", current_loc.r, current_loc.c));
                        self.process_land_mode(piece_idx);
                        break; 
                    }
                }
            }

            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];

            let mut valid_steps = Vec::new();
            for &dir in &dirs {
                let target_r = (current_loc.r as i32 + dir.0) as usize;
                let target_c = (current_loc.c as i32 + dir.1) as usize;

                if target_r < self.rows && target_c < self.cols {
                    let is_land = self.map[target_r][target_c] == b'#';
                    let target_rc = Loc { r: target_r, c: target_c };
                    let can_enter = match p.ptype {
                        PieceType::Army => is_land,
                        PieceType::Battleship | PieceType::Destroyer => !is_land || all_targets.contains(&target_rc),
                        _ => !is_land,
                    };

                    if can_enter || p.ptype == PieceType::Fighter {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_tile_blocked_by_friend(target_loc, p.ptype, Owner::Player) {
                            valid_steps.push(target_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                self.pieces[piece_idx].moves_left = 0; 
                break;
            }

            if all_targets.is_empty() {
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                self.pieces[piece_idx].activity = UnitActivity::Explore;
                return;
            }

            let mut chosen_step = valid_steps[0];
            let mut found_target = false;

            let mut queue = std::collections::VecDeque::new();
            let mut visited = vec![vec![false; self.cols]; self.rows];

            visited[current_loc.r][current_loc.c] = true;

            for &step in &valid_steps {
                visited[step.r][step.c] = true;
                queue.push_back((step, step)); 
            }

            while let Some((loc, initial_step)) = queue.pop_front() {
                if all_targets.contains(&loc) {
                    chosen_step = initial_step;
                    found_target = true;
                    break;
                }

                for &dir in &dirs {
                    let target_r = (loc.r as i32 + dir.0) as usize;
                    let target_c = (loc.c as i32 + dir.1) as usize;

                    if target_r < self.rows && target_c < self.cols {
                        if !visited[target_r][target_c] {
                            let is_land = self.map[target_r][target_c] == b'#';
                            let target_rc = Loc { r: target_r, c: target_c };
                            let can_enter = match p.ptype {
                                PieceType::Army => is_land,
                                PieceType::Fighter | PieceType::Satellite => true,
                                PieceType::Battleship | PieceType::Destroyer => !is_land || all_targets.contains(&target_rc),
                                _ => !is_land,
                            };

                            // Look through friendly traffic to prevent pathfinding failure
                            if can_enter {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
                            }
                        }
                    }
                }
            }

            if !found_target {
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                self.pieces[piece_idx].activity = UnitActivity::Explore;
                return; 
            }

            let (result, msg) = self.attempt_move(piece_idx, chosen_step);
            if !msg.is_empty() {
                attack_logs.push(msg);
            }

            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed | MoveResult::InvalidMove => break,
                MoveResult::CityCaptured(c_idx) => {
                    self.pending_cities.push(c_idx);
                    break; 
                }
                MoveResult::Moved | MoveResult::CombatWon => {}
            }
        }

        for msg in attack_logs {self.log(msg);
        }
    }

    pub fn random_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut random_logs = Vec::new();
        let mut rng = rand::thread_rng();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let ptype = self.pieces[piece_idx].ptype;
            let current_loc = self.pieces[piece_idx].loc;
            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];

            let mut valid_steps = Vec::new();
            for &dir in &dirs {
                let target_r = (current_loc.r as i32 + dir.0) as usize;
                let target_c = (current_loc.c as i32 + dir.1) as usize;

                if target_r < self.rows && target_c < self.cols {
                    let is_land = self.map[target_r][target_c] == b'#';
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || ptype == PieceType::Fighter {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player) {
                            valid_steps.push(target_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let chosen_step = valid_steps[rng.gen_range(0..valid_steps.len())];

            let (result, msg) = self.attempt_move(piece_idx, chosen_step);
            if !msg.is_empty() {
                random_logs.push(msg);
            }
            
            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed | MoveResult::InvalidMove => break,
                MoveResult::CityCaptured(c_idx) => {
                    self.pending_cities.push(c_idx);
                    break;
                }
                MoveResult::Moved | MoveResult::CombatWon => {}
            }
        }

        for msg in random_logs {
        	self.log(msg);
        }
    }

    pub fn dest_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut dest_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let target_loc = if let UnitActivity::Dest(loc) = self.pieces[piece_idx].activity { loc } else { break; };
            let current_loc = self.pieces[piece_idx].loc;
            let ptype = self.pieces[piece_idx].ptype;

            if ptype == PieceType::Fighter {
                if let Some(dist) = self.dist_to_nearest_base(current_loc) {
                    if self.pieces[piece_idx].fuel <= dist + 2 {
                        self.pieces[piece_idx].activity = UnitActivity::Land;
                        self.pieces[piece_idx].moves_left = 0;
                        dest_logs.push(format!("Fighter at ({},{}) reached bingo fuel. Route cancelled, returning to base.", current_loc.r, current_loc.c));
                        break;
                    }
                }
            }

            if current_loc == target_loc {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                dest_logs.push(format!("{:?} arrived at destination ({},{}). Awaiting orders.", ptype, target_loc.r, target_loc.c));
                break;
            }

            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];
            let mut valid_steps = Vec::new();

            for &dir in &dirs {
                let r = (current_loc.r as i32 + dir.0) as usize;
                let c = (current_loc.c as i32 + dir.1) as usize;

                if r < self.rows && c < self.cols {
                    let step_loc = Loc { r, c };
                    let is_land = self.map[r][c] == b'#';
                    let friendly_city = self.cities.iter().any(|ct| ct.loc == step_loc && ct.owner == Owner::Player);
                    
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land || friendly_city,
                    };

                    if can_enter || ptype == PieceType::Fighter {
                        if (!friendly_city && !self.is_tile_blocked_by_friend(step_loc, ptype, Owner::Player)) || step_loc == target_loc {
                            valid_steps.push(step_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                dest_logs.push(format!("{:?} at ({},{}) is blocked! Halting for manual assist.", ptype, current_loc.r, current_loc.c));
                break; // Leave moves_left > 0 so the TUI can pick it up
            }

            let mut chosen_step = valid_steps[0];
            let mut found_path = false;
            let mut queue = std::collections::VecDeque::new();
            let mut visited = vec![vec![false; self.cols]; self.rows];

            visited[current_loc.r][current_loc.c] = true;

            for &step in &valid_steps {
                visited[step.r][step.c] = true;
                queue.push_back((step, step)); 
            }

            while let Some((loc, initial_step)) = queue.pop_front() {
                if loc == target_loc {
                    chosen_step = initial_step;
                    found_path = true;
                    break;
                }

                for &dir in &dirs {
                    let r = (loc.r as i32 + dir.0) as usize;
                    let c = (loc.c as i32 + dir.1) as usize;

                    if r < self.rows && c < self.cols && !visited[r][c] {
                        let is_land = self.map[r][c] == b'#';
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == Loc { r, c } && ct.owner == Owner::Player);
                        let can_enter = match ptype {
                            PieceType::Army => is_land,
                            PieceType::Fighter | PieceType::Satellite => true,
                            _ => !is_land || friendly_city,
                        };

                        // Look through friendly traffic to prevent pathfinding failure
                        if can_enter {
                            let loc_rc = Loc { r, c };
                            visited[r][c] = true;
                            queue.push_back((loc_rc, initial_step));
                        }
                    }
                }
            }

            if !found_path {
                dest_logs.push(format!("{:?} cannot find a route to ({},{}). Halting for manual assist.", ptype, target_loc.r, target_loc.c));
                break; // Leave moves_left > 0 so the TUI can pick it up
            }

            let (result, msg) = self.attempt_move(piece_idx, chosen_step);
            if !msg.is_empty() {
                dest_logs.push(msg);
            }
            
            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed => break,
                MoveResult::InvalidMove => {
                    dest_logs.push(format!("{:?} encountered an invalid move. Halting for manual assist.", ptype));
                    break; // Leave moves_left > 0 so the TUI can pick it up
                },
                MoveResult::CityCaptured(c_idx) => {
                    self.pending_cities.push(c_idx);
                    break;
                }
                MoveResult::Moved | MoveResult::CombatWon => {}
            }
        }

        for msg in dest_logs {
            self.log(msg);
        }
    }
    pub fn dir_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut dir_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let (dr, dc) = if let UnitActivity::Dir(r, c) = self.pieces[piece_idx].activity { (r, c) } else { break; };
            let current_loc = self.pieces[piece_idx].loc;
            let ptype = self.pieces[piece_idx].ptype;

            if ptype == PieceType::Fighter {
                if let Some(dist) = self.dist_to_nearest_base(current_loc) {
                    if self.pieces[piece_idx].fuel <= dist + 2 {
                        self.pieces[piece_idx].activity = UnitActivity::Land;
                        self.pieces[piece_idx].moves_left = 0;
                        dir_logs.push(format!("Fighter at ({},{}) reached bingo fuel. March halted, returning to base.", current_loc.r, current_loc.c));
                        break;
                    }
                }
            }

            let target_r = current_loc.r as i32 + dr;
            let target_c = current_loc.c as i32 + dc;

            if target_r < 0 || target_r >= self.rows as i32 || target_c < 0 || target_c >= self.cols as i32 {
                if ptype == PieceType::Satellite {
                    self.pieces[piece_idx].hits = 0;
                    self.pieces[piece_idx].moves_left = 0;
                    dir_logs.push(String::from("Satellite flew out of orbit and was lost."));
                } else {
                    dir_logs.push(format!("{:?} reached the edge of the map. Halting for manual assist.", ptype));
                }
                break; // Leave moves_left > 0 so the TUI can pick it up
            }

            let target_loc = Loc { r: target_r as usize, c: target_c as usize };
            let is_land = self.map[target_loc.r][target_loc.c] == b'#';
            let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
            
            let can_enter = match ptype {
                PieceType::Army => is_land,
                _ => !is_land || friendly_city,
            };

            if ptype != PieceType::Satellite && ((!can_enter && ptype != PieceType::Fighter) || friendly_city || self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player)) {
                dir_logs.push(format!("{:?} hit an obstacle at ({},{}) and halted its march.", ptype, target_loc.r, target_loc.c));
                break; // Leave moves_left > 0 so the TUI can pick it up
            }

            let (result, msg) = self.attempt_move(piece_idx, target_loc);
            if !msg.is_empty() {
                dir_logs.push(msg);
            }

            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed => break,
                MoveResult::InvalidMove => {
                    dir_logs.push(format!("{:?} encountered an invalid move. Halting for manual assist.", ptype));
                    break; // Leave moves_left > 0 so the TUI can pick it up
                },
                MoveResult::CityCaptured(c_idx) => {
                    self.pending_cities.push(c_idx);
                    break;
                }
                MoveResult::Moved | MoveResult::CombatWon => {}
            }
        }

        for msg in dir_logs {
            self.log(msg);
        }
    }
    pub fn reveal_entire_map(&mut self) {
        for r in 0..self.rows {
            for c in 0..self.cols {
                self.player_map[r][c] = 1;
            }
        }
        self.log(String::from("Observer mode active. Map completely revealed."));
    }

    pub fn reveal_vision(&mut self, loc: Loc, range: i32) {
        for dr in -range..=range {
            for dc in -range..=range {
                let nr = (loc.r as i32 + dr) as usize;
                let nc = (loc.c as i32 + dc) as usize;
                if nr < self.rows && nc < self.cols {
                    self.player_map[nr][nc] = 1;
                }
            }
        }
    }

    pub fn log(&mut self, msg: String) {
        self.messages.push(msg);
        self.msg_scroll = 0; 
        if self.messages.len() > 100 { 
            self.messages.remove(0);
        }
    }

    pub fn process_fill_mode(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }
        let ptype = self.pieces[piece_idx].ptype;
        let cargo_count = self.pieces[piece_idx].cargo.len();
        
        let max_cargo = match ptype {
            PieceType::Transport => 6,
            PieceType::Carrier => self.pieces[piece_idx].hits as usize,
            _ => 0,
        };

        if cargo_count >= max_cargo {
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            self.pieces[piece_idx].moves_left = self.pieces[piece_idx].ptype.max_moves();
            self.log(format!("{:?} is fully loaded with {} units and woke up.", ptype, cargo_count));
        } else {
            self.pieces[piece_idx].moves_left = 0;
        }
    }
    pub fn process_upgrade_mode(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }
        let ptype = self.pieces[piece_idx].ptype;
        let p_loc = self.pieces[piece_idx].loc;
        let p_hits = self.pieces[piece_idx].hits;
        
        let at_owned_city = self.cities.iter().any(|c| c.loc == p_loc && c.owner == Owner::Player);
        let max_h = ptype.max_hits();

        if at_owned_city {
            if p_hits < max_h {
                let new_hits = (p_hits + 1).min(max_h);
                self.pieces[piece_idx].hits = new_hits;
                self.pieces[piece_idx].moves_left = 0;
                self.log(format!("{:?} docked at ({},{}) repaired to {}/{} hits.", ptype, p_loc.r, p_loc.c, new_hits, max_h));
            } else {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                self.pieces[piece_idx].moves_left = ptype.max_moves();
                self.log(format!("{:?} fully repaired at ({},{}) and woke up.", ptype, p_loc.r, p_loc.c));
            }
            return;
        }

        let owned_city_locs: Vec<Loc> = self.cities.iter()
            .filter(|c| c.owner == Owner::Player)
            .map(|c| c.loc)
            .collect();

        if owned_city_locs.is_empty() {
            self.pieces[piece_idx].moves_left = 0;
            return;
        }

        let mut closest_loc = owned_city_locs[0];
        let mut min_dist = (p_loc.r as isize - closest_loc.r as isize).pow(2) + (p_loc.c as isize - closest_loc.c as isize).pow(2);

        for &cloc in &owned_city_locs {
            let dist = (p_loc.r as isize - cloc.r as isize).pow(2) + (p_loc.c as isize - cloc.c as isize).pow(2);
            if dist < min_dist {
                min_dist = dist;
                closest_loc = cloc;
            }
        }

        self.pieces[piece_idx].activity = UnitActivity::Dest(closest_loc);
        self.dest_piece(piece_idx);
        
        if self.pieces[piece_idx].hits > 0 {
            self.pieces[piece_idx].activity = UnitActivity::Upgrade;
        }
    }

    pub fn process_transport_wait(&mut self, piece_idx: usize) -> bool {
        if self.pieces[piece_idx].ptype != PieceType::Army { return false; }
        let loc = self.pieces[piece_idx].loc;
        
        let mut target_transport = None;
        for (i, other) in self.pieces.iter().enumerate() {
            if other.owner == Owner::Player && other.ptype == PieceType::Transport && other.cargo.len() < 6 {
                let r_diff = (loc.r as isize - other.loc.r as isize).abs();
                let c_diff = (loc.c as isize - other.loc.c as isize).abs();
                if r_diff <= 1 && c_diff <= 1 {
                    target_transport = Some(i);
                    break;
                }
            }
        }

        if let Some(t_idx) = target_transport {
            let mut boarded_piece = self.pieces[piece_idx].clone();
            boarded_piece.moves_left = 0;
            boarded_piece.activity = UnitActivity::Sentry; 
            self.pieces[t_idx].cargo.push(boarded_piece);
            
            self.pieces[piece_idx].hits = 0; 
            self.log(String::from("Army woke up and boarded an available Transport."));
            return true;
        }
        false
    }

    pub fn wake_cargo(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].cargo.is_empty() {
            return;
        }
        
        let t_loc = self.pieces[piece_idx].loc;
        let mut awakened_units = Vec::new();
        
        // Mobile Sector Design: Waking cargo brings them out of the hold and onto the deck
        // (the main pieces array) where they can receive normal sequential orders.
        for mut unit in self.pieces[piece_idx].cargo.drain(..) {
            unit.loc = t_loc;
            unit.activity = UnitActivity::Idle; 
            unit.moves_left = unit.ptype.max_moves(); 
            awakened_units.push(unit);
        }
        
        self.pieces.extend(awakened_units);
        self.log(format!("{:?} at ({},{}) awakened its cargo onto the deck.", self.pieces[piece_idx].ptype, t_loc.r, t_loc.c));
    }

    pub fn process_land_mode(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }
        
        let p_loc = self.pieces[piece_idx].loc;
        let ptype = self.pieces[piece_idx].ptype;
        
        if let Some(city_idx) = self.cities.iter().position(|c| c.loc == p_loc && c.owner == Owner::Player) {
            let city = &self.cities[city_idx];
            let assigned_order = city.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a)
                .unwrap_or_else(|| self.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a).unwrap_or(UnitActivity::Idle));

            self.pieces[piece_idx].activity = assigned_order;
            self.pieces[piece_idx].moves_left = ptype.max_moves();
            if ptype == PieceType::Fighter {
                self.pieces[piece_idx].fuel = 32;
                self.log(format!("Fighter landed at ({},{}) refueled and assigned {:?}.", p_loc.r, p_loc.c, assigned_order));
            } else {
                self.log(format!("{:?} docked at ({},{}) and assigned {:?}.", ptype, p_loc.r, p_loc.c, assigned_order));
            }
            return;
        }

        if ptype == PieceType::Fighter {
            if let Some(carrier_idx) = self.pieces.iter().position(|p| p.owner == Owner::Player && p.ptype == PieceType::Carrier && p.loc == p_loc && p.hits > 0 && p.cargo.len() < p.hits as usize) {
                let mut boarded_piece = self.pieces[piece_idx].clone();
                boarded_piece.moves_left = 0;
                boarded_piece.activity = UnitActivity::Sentry; 
                boarded_piece.fuel = 32; // Refuel upon docking
                self.pieces[carrier_idx].cargo.push(boarded_piece);
                self.pieces[piece_idx].hits = 0; // Mark original hovering fighter for engine cleanup
                self.log(format!("Fighter at ({},{}) immediately landed on the carrier below it.", p_loc.r, p_loc.c));
                return;
            }
        }

        if ptype != PieceType::Fighter { 
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            return;
        }
        
        let mut valid_targets = Vec::new();
        for city in &self.cities {
            if city.owner == Owner::Player {
                valid_targets.push(city.loc);
            }
        }
        for (i, piece) in self.pieces.iter().enumerate() {
            if i != piece_idx && piece.owner == Owner::Player && piece.ptype == PieceType::Carrier && piece.hits > 0 && piece.cargo.len() < piece.hits as usize {
                valid_targets.push(piece.loc);
            }
        }
        
        if valid_targets.is_empty() {
            self.log(String::from("Fighter has no available bases or carriers to land on!"));
            self.pieces[piece_idx].moves_left = 0;
            return;
        }
        
        let mut closest_loc = valid_targets[0];
        let mut min_dist = (p_loc.r as isize - closest_loc.r as isize).pow(2) + (p_loc.c as isize - closest_loc.c as isize).pow(2);
        
        for &loc in &valid_targets[1..] {
            let dist = (p_loc.r as isize - loc.r as isize).pow(2) + (p_loc.c as isize - loc.c as isize).pow(2);
            if dist < min_dist {
                min_dist = dist;
                closest_loc = loc;
            }
        }
        
        self.pieces[piece_idx].activity = UnitActivity::Dest(closest_loc);
        self.pieces[piece_idx].moves_left = 0;
    }

    pub fn check_game_over(&self) -> Option<String> {
        let player_cities = self.cities.iter().filter(|c| c.owner == Owner::Player).count();
        let comp_cities = self.cities.iter().filter(|c| c.owner == Owner::Comp).count();
        
        let player_units = self.pieces.iter().filter(|p| p.owner == Owner::Player && p.hits > 0).count();
        let comp_units = self.pieces.iter().filter(|p| p.owner == Owner::Comp && p.hits > 0).count();

        if comp_cities == 0 && comp_units == 0 {
            return Some(String::from(">>> VICTORY! The enemy has been completely eradicated! <<<"));
        }

        if player_cities == 0 && player_units == 0 {
            return Some(String::from(">>> DEFEAT! Your forces are destroyed and your empire has fallen! <<<"));
        }

        if player_cities == self.cities.len() {
            return Some(String::from(">>> TOTAL DOMINATION! You have captured every city on the globe! <<<"));
        }

        None
    }

    pub fn save_game(&self, filename: &str) {
        if let Ok(data) = serde_json::to_string(self) {
            let _ = std::fs::write(filename, data);
        }
    }

    pub fn load_game(filename: &str) -> Option<Self> {
        if let Ok(data) = std::fs::read_to_string(filename) {
            serde_json::from_str(&data).ok()
        } else {
            None
        }
    }
}


