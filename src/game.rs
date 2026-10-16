use serde::{Serialize, Deserialize};
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveResult {
    Moved,
    CombatWon,
    CombatLost,
    CityCaptured(usize),
    CityAssaultFailed,
    InvalidMove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Player,
    Comp,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Manual,
    Auto,
    Edit,
    PickDest(usize), // NEW: Target selection mode, holds the unit ID
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceType {
    Army,
    Fighter,
    PatrolBoat,
    Destroyer,
    Submarine,
    Carrier,
    Battleship,
    Transport,
    Satellite,
}

impl PieceType {
    pub fn max_moves(&self) -> i32 {
        match self {
            PieceType::Army => 1,
            PieceType::Fighter => 8,
            PieceType::PatrolBoat => 4,
            PieceType::Destroyer => 2,
            PieceType::Submarine => 2,
            PieceType::Carrier => 2,
            PieceType::Battleship => 2,
            PieceType::Transport => 2,
            PieceType::Satellite => 10,
        }
    }

    pub fn max_hits(&self) -> i32 {
        match self {
            PieceType::Army => 1,
            PieceType::Fighter => 1,
            PieceType::PatrolBoat => 1,
            PieceType::Destroyer => 3,
            PieceType::Submarine => 2,
            PieceType::Carrier => 8,
            PieceType::Battleship => 10,
            PieceType::Transport => 1,
            PieceType::Satellite => 1,
        }
    }

    pub fn strike_damage(&self) -> i32 {
        match self {
            PieceType::Submarine => 3,
            PieceType::Battleship => 2,
            _ => 1,
        }
    }

    pub fn build_cost(&self) -> i32 {
        match self {
            PieceType::Army => 5,
            PieceType::Fighter => 10,
            PieceType::PatrolBoat => 15,
            PieceType::Destroyer => 20,
            PieceType::Submarine => 20,
            PieceType::Carrier => 30,
            PieceType::Battleship => 40,
            PieceType::Transport => 30,
            PieceType::Satellite => 50,
        }
    }

    pub fn sight_range(&self) -> i32 {
        match self {
            PieceType::Army => 2,
            PieceType::Fighter => 6,
            PieceType::PatrolBoat => 4,
            PieceType::Destroyer => 4,
            PieceType::Submarine => 3,
            PieceType::Carrier => 5,
            PieceType::Battleship => 5,
            PieceType::Transport => 3,
            PieceType::Satellite => 10,
        }
    }

    pub fn is_naval(&self) -> bool {
        matches!(
            self,
            PieceType::PatrolBoat
                | PieceType::Destroyer
                | PieceType::Submarine
                | PieceType::Carrier
                | PieceType::Battleship
                | PieceType::Transport
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Hash)]
pub struct Loc {
    pub r: usize,
    pub c: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitActivity {
    Idle,          // Awake / Manual
    Explore,       // Grope
    Sentry,        // Sleep
    Attack,        // Attack
    Fill,          // Transports/Carriers wait for cargo
    Land,          // Fighters seek nearest carrier/city
    Random,        // Move randomly
    TransportWait, // Armies wait for a transport to pass
    Upgrade,       // Ships seek nearest city to repair
    Dir(i32, i32), // Move in a specific direction (dr, dc)
    Dest(Loc),     // Move toward a specific destination
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub id: usize,
    pub ptype: PieceType,
    pub owner: Owner,
    pub loc: Loc,
    pub hits: i32,
    pub moves_left: i32,
    pub activity: UnitActivity,
    pub cargo: Vec<Piece>,
    pub fuel: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City {
    pub loc: Loc,
    pub owner: Owner,
    pub prod: PieceType,
    pub work: i32,
    #[serde(default)]
    pub default_orders: Vec<(PieceType, UnitActivity)>,
}

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
    pub edit_cursor: Loc,                                 // Tracks where the free-roaming cursor is
    pub default_orders: Vec<(PieceType, UnitActivity)>,   // Tracks global initial orders for new units
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

        // 55% water provides a better seed ratio for the smoothing algorithm to form continents
        let water_percent = 62; 
        let smooth_passes = 5;

        // 1. Initial random noise
        for r in 0..self.rows {
            for c in 0..self.cols {
                if rng.gen_range(0..100) < water_percent {
                    self.map[r][c] = b'.';
                } else {
                    self.map[r][c] = b'#';
                }
            }
        }

        // 2. Cellular Automata Smoothing
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
                    // Land survives if 4+ neighbors are land. Water becomes land if 5+ neighbors are land.
                    if land_count >= 5 || (is_land && land_count >= 4) {
                        next_map[r][c] = b'#';
                    } else {
                        next_map[r][c] = b'.';
                    }
                }
            }
            self.map = next_map;
        }

        // 3. Force borders to water
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

        // Submarine stealth logic
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
            return false; // Unlimited stacking in friendly cities
        }
        self.pieces.iter().any(|other| {
            if other.loc != loc || other.owner != owner || other.hits <= 0 { return false; }
            let is_fighter_flyover = (moving_ptype == PieceType::Fighter && other.ptype != PieceType::Fighter) || 
                                     (moving_ptype != PieceType::Fighter && other.ptype == PieceType::Fighter);
            let can_board_transport = moving_ptype == PieceType::Army && other.ptype == PieceType::Transport && other.cargo.len() < 6;
            let can_board_carrier = moving_ptype == PieceType::Fighter && other.ptype == PieceType::Carrier && other.cargo.len() < other.hits as usize;
            
            !(is_fighter_flyover || can_board_transport || can_board_carrier)
        })
    }

    pub fn setup_initial_states(&mut self) {
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

        // Fallback if map generation somehow missed land
        if land_tiles.is_empty() {
            self.map[self.rows / 2][self.cols / 2] = b'#';
            land_tiles.push(Loc { r: self.rows / 2, c: self.cols / 2 });
        }

        // Shuffle land tiles for randomized city distribution
        for i in 0..land_tiles.len() {
            let j = rng.gen_range(0..land_tiles.len());
            land_tiles.swap(i, j);
        }

        // Scale cities dynamically based on map area (Classic 60x100 had 70 cities, roughly 1 per 85 tiles)
        let target_cities = (self.rows * self.cols) / 85;
        let mut chosen_city_locs: Vec<Loc> = Vec::new();

        for loc in land_tiles {
            if chosen_city_locs.len() >= target_cities {
                break;
            }

            // Ensure cities are at least 6 tiles apart using Chebyshev distance
            let mut too_close = false;
            for &c_loc in &chosen_city_locs {
                let dr = (c_loc.r as isize - loc.r as isize).abs();
                let dc = (c_loc.c as isize - loc.c as isize).abs();
                if dr < 6 && dc < 6 {
                    too_close = true;
                    break;
                }
            }

            if !too_close {
                chosen_city_locs.push(loc);
            }
        }

        // 1. Place the safely spaced cities as Neutral first
        for loc in chosen_city_locs {
            self.cities.push(City {
                loc,
                owner: Owner::Neutral,
                prod: PieceType::Army,
                work: 0,
                default_orders: Vec::new(),
            });
        }

        // 2. Group cities into continents using Flood Fill (BFS)

        // 2. Group cities into continents using Flood Fill (BFS)
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
                    if !continent_cities.contains(&idx) {
                        continent_cities.push(idx);
                    }
                }

                // Flood fill through all 8 connected directions
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
            if !continent_cities.is_empty() {
                continents.push(continent_cities);
            }
        }

        // 3. Filter for valid player continents (>= 2 cities AND at least one coastal city)
        let mut valid_continents: Vec<Vec<usize>> = continents.into_iter().filter(|cities| {
            cities.len() >= 2 && cities.iter().any(|&idx| self.is_coastal(self.cities[idx].loc))
        }).collect();

        // Fallback if map generation created an overly fragmented archipelago
        if valid_continents.is_empty() {
            valid_continents.push((0..self.cities.len()).collect());
        }

        // Sort continents by city count (Descending: Largest to Smallest)
        valid_continents.sort_by(|a, b| b.len().cmp(&a.len()));

        // 4. Map difficulty (1-100) to continent index
        let diff_ratio = (self.difficulty.clamp(1, 100) - 1) as f32 / 99.0;
        
        // Lower difficulty = Lower index (Largest Continent). Higher diff = Higher index (Smallest Continent).
        let p_cont_idx = (diff_ratio * (valid_continents.len() - 1) as f32).round() as usize;
        let p_continent = &valid_continents[p_cont_idx];

        // Ensure Player Capital is coastal
        let p_cap_idx = *p_continent.iter()
            .find(|&&idx| self.is_coastal(self.cities[idx].loc))
            .unwrap_or(&p_continent[0]);
            
        self.cities[p_cap_idx].owner = Owner::Player;

        // 5. Assign Computer to the opposite end of the continent size spectrum
        let mut c_cont_idx = ((1.0 - diff_ratio) * (valid_continents.len() - 1) as f32).round() as usize;
        
        // Prevent Computer from starting on the exact same continent as Player if alternatives exist
        if c_cont_idx == p_cont_idx && valid_continents.len() > 1 {
            if c_cont_idx > 0 { c_cont_idx -= 1; } else { c_cont_idx += 1; }
        }
        let c_continent = &valid_continents[c_cont_idx];
        
        // Pick Computer Capital randomly from its designated continent
        let mut rng = rand::thread_rng();
        let c_cap_idx = c_continent[rng.gen_range(0..c_continent.len())];
        self.cities[c_cap_idx].owner = Owner::Comp;

        // Reveal map around player capitals (No starting armies!)
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

        // 1. Terrain Check & Friendly Port Docking
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

        // 2. Unit Collision, Combat, and Transport Boarding
        let mut enemy_idx = None;
        let mut boardable_carrier_idx = None;

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
                        } else if is_friendly_city || is_fighter_flyover {
                            continue;
                        } else {
                            return (MoveResult::InvalidMove, if is_player { String::from("Sector is occupied by a friendly unit.") } else { String::new() });
                        }
                    } else {
                        enemy_idx = Some(i);
                    }
                }
            }
        }

        if let Some(carrier_idx) = boardable_carrier_idx {
            let mut boarded_piece = self.pieces[piece_idx].clone();
            boarded_piece.loc = target_loc;
            boarded_piece.moves_left = 0;
            self.pieces[carrier_idx].cargo.push(boarded_piece);
            self.pieces[piece_idx].hits = 0;
            
            let msg = if is_player { format!("Your {:?} successfully boarded the {:?}.", ptype, self.pieces[carrier_idx].ptype) } else { String::new() };
            return (MoveResult::Moved, msg);
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
                    self.pieces[piece_idx].loc = target_loc;
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

        // 2.5 Empty City Bombardment Check
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

        // 3. City Assault Check
        if ptype == PieceType::Army {
            for (c_idx, city) in self.cities.iter_mut().enumerate() {
                if city.loc == target_loc { 
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

        // 4. Standard Move
        self.pieces[piece_idx].loc = target_loc;
        self.pieces[piece_idx].moves_left -= 1;
        
        if ptype == PieceType::Fighter {
            self.pieces[piece_idx].fuel -= 1;
            let at_base = self.cities.iter().any(|c| c.loc == target_loc && c.owner == unit_owner);
            if at_base {
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
        
        if is_player {
            self.reveal_vision(target_loc, ptype.sight_range());
        }
        (MoveResult::Moved, String::new())
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

            // 1. Gather all physically valid adjacent steps
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
                self.pieces[piece_idx].moves_left = 0; // Drain moves to prevent ghosts
                break;
            }

            // 2. Flood-Fill (BFS) to find the shortest path to reachable fog
            let mut chosen_step = valid_steps[0];
            let mut found_fog = false;

            let mut queue = std::collections::VecDeque::new();
            let mut visited = vec![vec![false; self.cols]; self.rows];

            visited[current_loc.r][current_loc.c] = true;

            // Seed the algorithm with our valid immediate steps
            for &step in &valid_steps {
                visited[step.r][step.c] = true;
                // Store both the current tile being searched, AND the initial step we took to get this branch started
                queue.push_back((step, step)); 
            }

            // Pour outwards until we touch fog
            while let Some((loc, initial_step)) = queue.pop_front() {
                if self.player_map[loc.r][loc.c] == 0 {
                    chosen_step = initial_step; // We found fog! Take the step that led us down this path.
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

                            // Only flow through terrain this specific unit type is allowed to traverse
                            if can_enter {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
                            } else if self.player_map[target_r][target_c] == 0 {
                                // NEW: If the tile is untraversable (like water for an army) but is still unrevealed Fog of War,
                                // we queue it anyway! This lets armies pathfind to the coastline to reveal the ocean, 
                                // and ships sail to the coast to reveal the land.
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
            // 3. Pass to Universal Engine
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
        let comp_indices: Vec<usize> = self.pieces.iter().enumerate()
            .filter(|(_, p)| p.owner == Owner::Comp && p.hits > 0)
            .map(|(i, _)| i)
            .collect();

        for idx in comp_indices {
            while self.pieces[idx].moves_left > 0 && self.pieces[idx].hits > 0 {
                let p_loc = self.pieces[idx].loc;
                let ptype = self.pieces[idx].ptype;

                let mut targets = Vec::new();
                for city in &self.cities {
                    if city.owner != Owner::Comp {
                        targets.push(city.loc);
                    }
                }
                
                if targets.is_empty() {
                    for p in &self.pieces {
                        if p.owner == Owner::Player && p.hits > 0 {
                            targets.push(p.loc);
                        }
                    }
                }
                
                if targets.is_empty() { break; }

                targets.sort_by_key(|t| {
                    (p_loc.r as isize - t.r as isize).abs().max((p_loc.c as isize - t.c as isize).abs())
                });
                let target = targets[0];

                let mut best_step = None;
                let mut best_dist = i32::MAX;

                let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];
                for &dir in &dirs {
                    let tr = p_loc.r as i32 + dir.0;
                    let tc = p_loc.c as i32 + dir.1;
                    if tr >= 0 && tr < self.rows as i32 && tc >= 0 && tc < self.cols as i32 {
                        let next_loc = Loc { r: tr as usize, c: tc as usize };
                        
                        let is_land = self.map[next_loc.r][next_loc.c] == b'#';
                        let can_enter = match ptype {
                            PieceType::Army => is_land,
                            PieceType::Fighter | PieceType::Satellite => true,
                            _ => !is_land || self.cities.iter().any(|c| c.loc == next_loc && c.owner == Owner::Comp) || next_loc == target,
                        };

                        if can_enter && (!self.is_tile_blocked_by_friend(next_loc, ptype, Owner::Comp) || next_loc == target) {
                            let dist = (next_loc.r as i32 - target.r as i32).abs().max((next_loc.c as i32 - target.c as i32).abs());
                            if dist < best_dist {
                                best_dist = dist;
                                best_step = Some(next_loc);
                            }
                        }
                    }
                }

                if let Some(step) = best_step {
                    let (res, msg) = self.attempt_move(idx, step);
                    if !msg.is_empty() {
                        self.log(msg);
                    }
                    match res {
                        MoveResult::InvalidMove | MoveResult::CombatLost | MoveResult::CityAssaultFailed => break,
                        MoveResult::CityCaptured(_) => break, // Engine handled ownership, consume army and end turn
                        _ => {}
                    }
                } else {
                    self.pieces[idx].moves_left = 0; 
                    break;
                }
            }
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
                        PieceType::Satellite => 20, // 20 turns of orbital lifespan
                        _ => 0,
                    },
                });
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

                // Docked port healing
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

        let mut dead_sats = Vec::new();
        for p in self.pieces.iter_mut() {
            if p.ptype == PieceType::Satellite && p.hits > 0 {
                p.fuel -= 1; // Orbit decays by 1 turn
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

        #[cfg(debug_assertions)]
        {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open("ai_metrics.csv") {
                let comp_cities = self.cities.iter().filter(|c| c.owner == Owner::Comp).count();
                let comp_units = self.pieces.iter().filter(|p| p.owner == Owner::Comp && p.hits > 0).count();
                let _ = writeln!(file, "{},{},{}", self.turn, comp_cities, comp_units);
            }
        }
    }
    pub fn attack_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut attack_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            // ONLY target enemies that the player has actually discovered in the fog (and are not hidden submarines)
            let enemy_locs: Vec<Loc> = self.pieces.iter().enumerate()
                .filter(|(i, p)| p.owner != Owner::Player && p.hits > 0 && self.is_visible_to(*i, Owner::Player))
                .map(|(_, p)| p.loc)
                .collect();

            // ONLY target non-player cities that have been revealed on the map
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

            // 1. Gather all physically valid moves
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
                self.pieces[piece_idx].moves_left = 0; // Drain moves to prevent ghost states
                break;
            }

            if all_targets.is_empty() {
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                self.pieces[piece_idx].activity = UnitActivity::Explore;
                return;
            }

            // 2. Flood-Fill (BFS) to find the shortest actual path to any target
            let mut chosen_step = valid_steps[0];
            let mut found_target = false;

            let mut queue = std::collections::VecDeque::new();
            let mut visited = vec![vec![false; self.cols]; self.rows];

            visited[current_loc.r][current_loc.c] = true;

            // Seed the algorithm with our valid immediate steps
            for &step in &valid_steps {
                visited[step.r][step.c] = true;
                // Store both the current tile being searched, AND the initial step we took to get this branch started
                queue.push_back((step, step)); 
            }

            // Pour outwards until we touch a target
            while let Some((loc, initial_step)) = queue.pop_front() {
                if all_targets.contains(&loc) {
                    chosen_step = initial_step; // We found a target! Take the step that led us down this path.
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

                            // Flow through valid terrain, BUT do NOT pathfind through blocking friendly units
                            if can_enter && !self.is_tile_blocked_by_friend(target_rc, p.ptype, Owner::Player) {
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

            // 3. Pass to Universal Engine
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
                        
                        // Prevent random wanderers from suiciding into friendly cities or piling up on allies
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
                        // Treat the exact destination as valid even if occupied, so we can march up to it
                        if (!friendly_city && !self.is_tile_blocked_by_friend(step_loc, ptype, Owner::Player)) || step_loc == target_loc {
                            valid_steps.push(step_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                dest_logs.push(format!("{:?} at ({},{}) is blocked! Orders cancelled.", ptype, current_loc.r, current_loc.c));
                self.pieces[piece_idx].moves_left = 0;
                break;
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

                        if can_enter {
                            let loc_rc = Loc { r, c };
                            if !self.is_tile_blocked_by_friend(loc_rc, ptype, Owner::Player) || loc_rc == target_loc {
                                visited[r][c] = true;
                                queue.push_back((loc_rc, initial_step));
                            }
                        }
                    }
                }
            }

            if !found_path {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                dest_logs.push(format!("{:?} cannot find a route to ({},{}). Orders cancelled.", ptype, target_loc.r, target_loc.c));
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let (result, msg) = self.attempt_move(piece_idx, chosen_step);
            if !msg.is_empty() {
                dest_logs.push(msg);
            }
            
            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed => break,
                MoveResult::InvalidMove => {
                    self.pieces[piece_idx].activity = UnitActivity::Idle;
                    self.pieces[piece_idx].moves_left = 0;
                    dest_logs.push(format!("{:?} encountered an invalid move. Orders cancelled.", ptype));
                    break;
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

            // Halt if it marches off the edge of the world
            if target_r < 0 || target_r >= self.rows as i32 || target_c < 0 || target_c >= self.cols as i32 {
                if ptype == PieceType::Satellite {
                    self.pieces[piece_idx].hits = 0;
                    dir_logs.push(String::from("Satellite flew out of orbit and was lost."));
                } else {
                    self.pieces[piece_idx].activity = UnitActivity::Idle;
                    dir_logs.push(format!("{:?} reached the edge of the map. March halted.", ptype));
                }
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let target_loc = Loc { r: target_r as usize, c: target_c as usize };
            let is_land = self.map[target_loc.r][target_loc.c] == b'#';
            let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
            
            let can_enter = match ptype {
                PieceType::Army => is_land,
                _ => !is_land || friendly_city,
            };

            // Directional march halts immediately if blocked by terrain or friends
            if ptype != PieceType::Satellite && ((!can_enter && ptype != PieceType::Fighter) || friendly_city || self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player)) {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                dir_logs.push(format!("{:?} hit an obstacle at ({},{}) and halted its march.", ptype, target_loc.r, target_loc.c));
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let (result, msg) = self.attempt_move(piece_idx, target_loc);
            if !msg.is_empty() {
                dir_logs.push(msg);
            }

            match result {
                MoveResult::CombatLost | MoveResult::CityAssaultFailed => break,
                MoveResult::InvalidMove => {
                    self.pieces[piece_idx].activity = UnitActivity::Idle;
                    self.pieces[piece_idx].moves_left = 0;
                    dir_logs.push(format!("{:?} encountered an invalid move. March halted.", ptype));
                    break;
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
        self.msg_scroll = 0; // Jump back to the bottom when a new message appears
        if self.messages.len() > 100 { // Keep a larger history to scroll through
            self.messages.remove(0);
        }
    }
    // --- NAVAL LOGISTICS & AUTOMATION ---

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
        
        // Scan 3x3 area for a friendly transport with space
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
            boarded_piece.activity = UnitActivity::Sentry; // Sleep inside the transport
            self.pieces[t_idx].cargo.push(boarded_piece);
            
            self.pieces[piece_idx].hits = 0; // Mark original for cleanup
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
        
        // Eject cargo onto the map on the transport's exact tile
        for mut unit in self.pieces[piece_idx].cargo.drain(..) {
            unit.loc = t_loc;
            unit.activity = UnitActivity::Idle; // Wake them up for manual control
            unit.moves_left = unit.ptype.max_moves(); // Give them movement points to walk ashore
            awakened_units.push(unit);
        }
        
        self.pieces.extend(awakened_units);
        self.log(format!("{:?} at ({},{}) awakened its cargo for deployment.", self.pieces[piece_idx].ptype, t_loc.r, t_loc.c));
    }

    pub fn process_land_mode(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }
        if self.pieces[piece_idx].ptype != PieceType::Fighter { 
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            return;
        }
        
        let p_loc = self.pieces[piece_idx].loc;
        
        if self.cities.iter().any(|c| c.loc == p_loc && c.owner == Owner::Player) {
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            self.pieces[piece_idx].moves_left = self.pieces[piece_idx].ptype.max_moves();
            self.pieces[piece_idx].fuel = 32;
            self.log(format!("Fighter landed at ({},{}) and refueled.", p_loc.r, p_loc.c));
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_generation_dimensions() {
        let game = Game::new(120, 240, 50);
        assert_eq!(game.map.len(), 120);
        assert_eq!(game.map[0].len(), 240);
    }

    #[test]
    fn test_dynamic_city_scaling() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();
        
        let expected_target = (120 * 240) / 85;
        assert!(game.cities.len() <= expected_target);
        assert!(game.cities.len() > expected_target - 40, "City count was lower than expected target constraints");
    }

    #[test]
    fn test_city_distance_enforcement() {
        let mut game = Game::new(120, 240, 50);
        game.setup_initial_states();

        for (i, city_a) in game.cities.iter().enumerate() {
            for (j, city_b) in game.cities.iter().enumerate() {
                if i != j {
                    let dr = (city_a.loc.r as isize - city_b.loc.r as isize).abs();
                    let dc = (city_a.loc.c as isize - city_b.loc.c as isize).abs();
                    let dist = dr.max(dc);
                    assert!(dist >= 6, "Cities spawned too close: {:?} and {:?}", city_a.loc, city_b.loc);
                }
            }
        }
    }

    #[test]
    fn test_fighter_fuel_depletion_and_crash() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; // All water
        
        let fighter = Piece {
            id: 1,
            ptype: PieceType::Fighter,
            owner: Owner::Player,
            loc: Loc { r: 5, c: 5 },
            hits: 1,
            moves_left: 8,
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: 1, // 1 fuel remaining
        };
        game.pieces.push(fighter);

        // Move the fighter to adjacent tile
        let target_loc = Loc { r: 5, c: 6 };
        let (result, _) = game.attempt_move(0, target_loc);
        
        assert_eq!(result, MoveResult::CombatLost, "Fighter should crash when fuel reaches 0 without a base");
        assert_eq!(game.pieces[0].hits, 0, "Fighter hits should be 0 after crashing");
    }

    #[test]
    fn test_fighter_refuel_at_city() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; // All land
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 },
            owner: Owner::Player,
            prod: PieceType::Army,
            work: 0,
            default_orders: Vec::new(),
        });

        let fighter = Piece {
            id: 1,
            ptype: PieceType::Fighter,
            owner: Owner::Player,
            loc: Loc { r: 5, c: 5 },
            hits: 1,
            moves_left: 8,
            activity: UnitActivity::Idle,
            cargo: Vec::new(),
            fuel: 5, 
        };
        game.pieces.push(fighter);

        let target_loc = Loc { r: 5, c: 6 };
        let (result, _) = game.attempt_move(0, target_loc);
        
        assert_eq!(result, MoveResult::Moved);
        assert_eq!(game.pieces[0].fuel, 32, "Fighter fuel should instantly reset to 32 upon entering friendly city");
    }

    #[test]
    fn test_cargo_boarding_limits() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10]; // All water

        // Setup Transport (Capacity: 6)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // Setup 7 Armies adjacent to the Transport
        for i in 0..7 {
            game.pieces.push(Piece {
                id: i + 2, ptype: PieceType::Army, owner: Owner::Player,
                loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
                cargo: Vec::new(), fuel: 0,
            });
        }

        let mut army_success = 0;
        let mut army_fail = 0;
        for i in 1..=7 {
            let (res, _) = game.attempt_move(i, Loc { r: 5, c: 5 });
            if res == MoveResult::Moved { army_success += 1; }
            else if res == MoveResult::InvalidMove { army_fail += 1; }
        }

        assert_eq!(army_success, 6, "Transport should accept exactly 6 armies");
        assert_eq!(army_fail, 1, "Transport should reject the 7th army");
        assert_eq!(game.pieces[0].cargo.len(), 6, "Transport cargo array should hold 6 items");

        // Setup Carrier (Damaged to 2 hits. Dynamic capacity should now be 2)
        let carrier_idx = game.pieces.len();
        game.pieces.push(Piece {
            id: 10, ptype: PieceType::Carrier, owner: Owner::Player,
            loc: Loc { r: 8, c: 8 }, hits: 2, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // Setup 3 Fighters adjacent to the Carrier
        for i in 0..3 {
            game.pieces.push(Piece {
                id: i + 11, ptype: PieceType::Fighter, owner: Owner::Player,
                loc: Loc { r: 8, c: 7 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle,
                cargo: Vec::new(), fuel: 32,
            });
        }

        let mut f_boarded = 0;
        let mut f_hovering = 0;
        for i in 0..3 {
            let fighter_idx = carrier_idx + 1 + i;
            let (res, _) = game.attempt_move(fighter_idx, Loc { r: 8, c: 8 });
            if res == MoveResult::Moved { 
                if game.pieces[fighter_idx].hits == 0 {
                    f_boarded += 1;
                } else {
                    f_hovering += 1;
                }
            }
        }

        assert_eq!(f_boarded, 2, "Damaged Carrier should accept exactly 2 fighters");
        assert_eq!(f_hovering, 1, "The 3rd fighter should perform a flyover on the full carrier");
        assert_eq!(game.pieces[carrier_idx].cargo.len(), 2, "Carrier cargo array should hold exactly 2 items");
    }

    #[test]
    fn test_logistics_wake_cargo() {
        let mut game = Game::new(10, 10, 50);
        let mut transport = Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        };
        
        let army1 = Piece { id: 2, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 };
        let army2 = Piece { id: 3, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 };
        
        transport.cargo.push(army1);
        transport.cargo.push(army2);
        game.pieces.push(transport);

        game.wake_cargo(0);

        assert_eq!(game.pieces[0].cargo.len(), 0, "Transport cargo should be emptied");
        assert_eq!(game.pieces.len(), 3, "Awakened armies should be appended to the global pieces array");
        assert_eq!(game.pieces[1].loc, Loc { r: 5, c: 5 }, "Army 1 should spawn on the transport's exact tile");
        assert_eq!(game.pieces[1].activity, UnitActivity::Idle, "Army 1 should be awake (Idle)");
        assert_eq!(game.pieces[1].moves_left, 1, "Army 1 should have its movement points restored to walk ashore");
    }

    #[test]
    fn test_logistics_process_fill_mode() {
        let mut game = Game::new(10, 10, 50);
        let mut transport = Piece {
            id: 1, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 2, activity: UnitActivity::Fill,
            cargo: Vec::new(), fuel: 0,
        };
        
        // Fill the transport to its max capacity of 6
        for i in 0..6 {
            transport.cargo.push(Piece { id: i+2, ptype: PieceType::Army, owner: Owner::Player, loc: Loc { r: 0, c: 0 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry, cargo: Vec::new(), fuel: 0 });
        }
        game.pieces.push(transport);

        game.process_fill_mode(0);

        assert_eq!(game.pieces[0].activity, UnitActivity::Idle, "Full transport should wake up to Idle");
        assert_eq!(game.pieces[0].moves_left, 2, "Full transport should regain its movement points");

        game.pieces[0].cargo.pop(); // Remove 1 army, dropping capacity to 5/6
        game.pieces[0].activity = UnitActivity::Fill;
        game.process_fill_mode(0);

        assert_eq!(game.pieces[0].activity, UnitActivity::Fill, "Partially filled transport should remain asleep in Fill mode");
        assert_eq!(game.pieces[0].moves_left, 0, "Partially filled transport should have its moves drained to wait for more cargo");
    }

    #[test]
    fn test_combat_city_assault_consumption() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; // All land
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 }, owner: Owner::Neutral, prod: PieceType::Army, work: 0, default_orders: Vec::new(),
        });

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 4 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 5 });
        
        assert!(matches!(result, MoveResult::CityCaptured(_) | MoveResult::CityAssaultFailed), "Result should be capture or fail");
        assert_eq!(game.pieces[0].hits, 0, "Attacking army must be consumed (hits = 0) regardless of RNG success or failure");
        
        if let MoveResult::CityCaptured(idx) = result {
            assert_eq!(game.cities[idx].owner, Owner::Player, "City owner should update to player on successful assault");
        } else {
            assert_eq!(game.cities[0].owner, Owner::Neutral, "City owner should remain neutral on failed assault");
        }
    }

    #[test]
    fn test_turn_production_cycle() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(City {
            loc: Loc { r: 5, c: 5 },
            owner: Owner::Player,
            prod: PieceType::Army, // Cost is 5
            work: 4,               // One turn away from completion
            default_orders: Vec::new(),
        });
        
        let initial_pieces = game.pieces.len();
        game.process_turn();
        
        assert_eq!(game.pieces.len(), initial_pieces + 1, "New piece should spawn after work threshold is met");
        assert_eq!(game.cities[0].work, 0, "City work should reset to 0 after production");
        
        let new_piece = game.pieces.last().unwrap();
        assert_eq!(new_piece.ptype, PieceType::Army, "Produced unit should match city production target");
        assert_eq!(new_piece.loc, Loc { r: 5, c: 5 }, "Unit should spawn on the city tile");
    }

    #[test]
    fn test_turn_port_healing() {
        let mut game = Game::new(10, 10, 50);
        game.cities.push(City { 
            loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() 
        });
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Battleship, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 5, moves_left: 0, activity: UnitActivity::Sentry,
            cargo: Vec::new(), fuel: 0,
        });

        game.process_turn();
        assert_eq!(game.pieces[0].hits, 6, "Battleship should heal 1 hit point while on Sentry in a friendly city");
        
        game.pieces[0].hits = 10;
        game.process_turn();
        assert_eq!(game.pieces[0].hits, 10, "Battleship should not heal past its maximum hits threshold");
    }

    #[test]
    fn test_turn_satellite_decay() {
        let mut game = Game::new(10, 10, 50);
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Satellite, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 0, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 2, 
        });

        game.process_turn();
        assert_eq!(game.pieces.len(), 1, "Satellite should remain in orbit");
        assert_eq!(game.pieces[0].fuel, 1, "Satellite fuel should decrease by 1 per turn");

        game.process_turn();
        assert_eq!(game.pieces.len(), 0, "Satellite should be destroyed and removed from pieces array when fuel reaches 0");
    }

    #[test]
    fn test_ai_sentry_wake_logic() {
        let mut game = Game::new(10, 10, 50);
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 0, activity: UnitActivity::Sentry,
            cargo: Vec::new(), fuel: 0,
        });

        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Comp,
            loc: Loc { r: 5, c: 6 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // Reveal the tile so the player's vision system registers the enemy
        game.player_map[5][6] = 1;

        game.process_turn();

        assert_eq!(game.pieces[0].activity, UnitActivity::Idle, "Player Army should wake up from Sentry to Idle when an enemy is spotted");
    }

    #[test]
    fn test_ai_explore_bfs_obstacle_avoidance() {
        let mut game = Game::new(5, 5, 50);
        
        // Map: 5x5 land grid
        for r in 0..5 {
            for c in 0..5 {
                game.map[r][c] = b'#';
                game.player_map[r][c] = 1; // Fully revealed
            }
        }
        
        // Create an impassable water wall in the middle column
        game.map[1][2] = b'.';
        game.map[2][2] = b'.';
        game.map[3][2] = b'.';

        // Set a single unrevealed Fog of War tile on the far side of the wall
        game.player_map[2][4] = 0;

        // Place Player Army on the left side of the wall
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 2, c: 1 }, hits: 1, moves_left: 1, activity: UnitActivity::Explore,
            cargo: Vec::new(), fuel: 0,
        });

        game.explore_piece(0);

        let new_loc = game.pieces[0].loc;
        // The BFS should determine that stepping into the wall at (2,2) is invalid, 
        // and route the army diagonally North-East or South-East, or directly North/South to go around it.
        assert_ne!(new_loc, Loc { r: 2, c: 2 }, "Army must not step into the water wall");
        assert_eq!(game.map[new_loc.r][new_loc.c], b'#', "Army must only step on land");
        assert!(new_loc.c >= 1, "Army should not move backward away from the fog");
    }

    #[test]
    fn test_combat_unit_engagement() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'#'; 10]; 10]; 
        
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Comp,
            loc: Loc { r: 5, c: 6 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        
        assert!(matches!(result, MoveResult::CombatWon | MoveResult::CombatLost), "Result must be CombatWon or CombatLost");
        
        if result == MoveResult::CombatWon {
            assert_eq!(game.pieces[1].hits, 0, "Enemy should be destroyed");
            assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 6 }, "Victor should occupy the tile");
        } else {
            assert_eq!(game.pieces[0].hits, 0, "Attacker should be destroyed");
        }
    }

    #[test]
    fn test_combat_naval_bombardment() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10];
        game.map[5][6] = b'#'; 

        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Battleship, owner: Owner::Player,
            loc: Loc { r: 5, c: 5 }, hits: 10, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });
        
        game.cities.push(City {
            loc: Loc { r: 5, c: 6 }, owner: Owner::Comp, prod: PieceType::Army, work: 5, default_orders: Vec::new(),
        });

        let (result, _) = game.attempt_move(0, Loc { r: 5, c: 6 });
        
        assert_eq!(result, MoveResult::Moved, "Bombardment evaluates as Moved for the shelling action");
        assert_eq!(game.cities[0].work, 0, "Bombardment should reset city production work to 0");
        assert_eq!(game.pieces[0].loc, Loc { r: 5, c: 5 }, "Battleship must not enter the land tile");
    }

    #[test]
    fn test_combat_submarine_stealth() {
        let mut game = Game::new(10, 15, 50);
        game.map = vec![vec![b'.'; 15]; 10];

        // 0: Enemy Submarine at (5, 5)
        game.pieces.push(Piece {
            id: 1, ptype: PieceType::Submarine, owner: Owner::Comp,
            loc: Loc { r: 5, c: 5 }, hits: 2, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        // 1: Player Army at (5, 7) - Distance 2 (Too far for non-destroyer)
        game.pieces.push(Piece {
            id: 2, ptype: PieceType::Army, owner: Owner::Player,
            loc: Loc { r: 5, c: 7 }, hits: 1, moves_left: 1, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        assert!(!game.is_visible_to(0, Owner::Player), "Submarine should be hidden from Army at distance 2");

        // Move Army to (5, 6) - Distance 1 (Adjacent)
        game.pieces[1].loc = Loc { r: 5, c: 6 };
        assert!(game.is_visible_to(0, Owner::Player), "Submarine should be visible to adjacent Army");

        // Destroy Army, spawn Destroyer at (5, 9) - Distance 4 (Max sight range)
        game.pieces[1].hits = 0;
        game.pieces.push(Piece {
            id: 3, ptype: PieceType::Destroyer, owner: Owner::Player,
            loc: Loc { r: 5, c: 9 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle,
            cargo: Vec::new(), fuel: 0,
        });

        assert!(game.is_visible_to(0, Owner::Player), "Submarine should be visible to Destroyer at distance 4");

        // Move Destroyer to (5, 10) - Distance 5 (Out of range)
        game.pieces[2].loc = Loc { r: 5, c: 10 };
        assert!(!game.is_visible_to(0, Owner::Player), "Submarine should be hidden from Destroyer at distance 5");
    }

    #[test]
    fn test_city_stacking_and_fighter_flyover() {
        let mut game = Game::new(10, 10, 50);
        game.map = vec![vec![b'.'; 10]; 10];
        
        // Setup City at (5,5)
        game.cities.push(City { loc: Loc { r: 5, c: 5 }, owner: Owner::Player, prod: PieceType::Army, work: 0, default_orders: Vec::new() });

        // Place 6 Destroyers in the city
        for i in 0..6 {
            game.pieces.push(Piece {
                id: i+1, ptype: PieceType::Destroyer, owner: Owner::Player,
                loc: Loc { r: 5, c: 5 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
            });
        }
        
        // Spawn a 7th Destroyer at (5,6) and try to move into the city
        game.pieces.push(Piece {
            id: 7, ptype: PieceType::Destroyer, owner: Owner::Player,
            loc: Loc { r: 5, c: 6 }, hits: 3, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
        });
        
        let (res_city, _) = game.attempt_move(6, Loc { r: 5, c: 5 });
        assert_eq!(res_city, MoveResult::Moved, "Friendly units should stack infinitely inside friendly cities");

        // Now test Fighter Flyover on open water
        // Place an Army at (8,8) in a Transport
        game.pieces.push(Piece {
            id: 8, ptype: PieceType::Transport, owner: Owner::Player,
            loc: Loc { r: 8, c: 8 }, hits: 1, moves_left: 2, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 0,
        });

        // Spawn a Fighter at (8,9)
        game.pieces.push(Piece {
            id: 9, ptype: PieceType::Fighter, owner: Owner::Player,
            loc: Loc { r: 8, c: 9 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 32,
        });

        let (res_flyover, _) = game.attempt_move(8, Loc { r: 8, c: 8 });
        assert_eq!(res_flyover, MoveResult::Moved, "Fighter should be able to fly over a friendly Transport");
        
        // Spawn a second Fighter at (8,7) and try to fly over the first fighter at (8,8)
        game.pieces.push(Piece {
            id: 10, ptype: PieceType::Fighter, owner: Owner::Player,
            loc: Loc { r: 8, c: 7 }, hits: 1, moves_left: 8, activity: UnitActivity::Idle, cargo: Vec::new(), fuel: 32,
        });
        let (res_fighter_crash, _) = game.attempt_move(9, Loc { r: 8, c: 8 });
        assert_eq!(res_fighter_crash, MoveResult::InvalidMove, "Fighters cannot stack with other Fighters outside of cities/carriers");
    }
}
