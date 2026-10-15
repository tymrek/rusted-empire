use serde::{Serialize, Deserialize};
use rand::Rng;

#[derive(Clone, Copy, PartialEq, Eq)]
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
            PieceType::Army => 2,
            PieceType::Fighter => 12,
            PieceType::PatrolBoat => 4,
            PieceType::Destroyer => 3,
            PieceType::Submarine => 3,
            PieceType::Carrier => 2,
            PieceType::Battleship => 2,
            PieceType::Transport => 2,
            PieceType::Satellite => 20,
        }
    }

    pub fn max_hits(&self) -> i32 {
        match self {
            PieceType::Army => 10,
            PieceType::Fighter => 8,
            PieceType::PatrolBoat => 10,
            PieceType::Destroyer => 20,
            PieceType::Submarine => 10,
            PieceType::Carrier => 30,
            PieceType::Battleship => 40,
            PieceType::Transport => 20,
            PieceType::Satellite => 1,
        }
    }

    pub fn build_cost(&self) -> i32 {
        match self {
            PieceType::Army => 6,
            PieceType::Fighter => 8,
            PieceType::PatrolBoat => 10,
            PieceType::Destroyer => 24,
            PieceType::Submarine => 24,
            PieceType::Carrier => 40,
            PieceType::Battleship => 48,
            PieceType::Transport => 30,
            PieceType::Satellite => 60,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City {
    pub loc: Loc,
    pub owner: Owner,
    pub prod: PieceType,
    pub work: i32,
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
        let water_percent = 55; 
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

        // Classic VMS-Empire places exactly 70 cities, but spread out!
        let target_cities = 70;
        let mut chosen_city_locs: Vec<Loc> = Vec::new();

        for loc in land_tiles {
            if chosen_city_locs.len() >= target_cities {
                break;
            }

            // Ensure cities are at least a few tiles apart (distance squared < 9 means a radius of ~3 tiles)
            let mut too_close = false;
            for &c_loc in &chosen_city_locs {
                let dist_sq = (c_loc.r as isize - loc.r as isize).pow(2) + (c_loc.c as isize - loc.c as isize).pow(2);
                if dist_sq < 9 {
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
            });
        }

        // 2. Group cities into continents using Flood Fill (BFS)

        // 2. Group cities into continents using Flood Fill (BFS)
        let mut visited_land = vec![vec![false; self.cols]; self.rows];
        let mut continents: Vec<Vec<usize>> = Vec::new();

        for (c_idx, city) in self.cities.iter().enumerate() {
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
        
        // 1. Terrain Check & Friendly Port Docking
        let is_friendly_city = self.cities.iter().any(|c| c.loc == target_loc && c.owner == Owner::Player);
        let can_enter = match ptype {
            PieceType::Army => is_land,
            _ => !is_land || is_friendly_city, // Naval units can enter land if it is a friendly city
        };

        if !can_enter && ptype != PieceType::Fighter {
            return (MoveResult::InvalidMove, String::from("Terrain impassable for this unit type."));
        }

        // 2. Unit Collision, Combat, and Transport Boarding
        let mut enemy_idx = None;
        let mut boardable_carrier_idx = None;

        for (i, other) in self.pieces.iter().enumerate() {
            // Ignore self when checking for tile collisions
            if i != piece_idx && other.loc == target_loc && other.hits > 0 {
                if other.owner == Owner::Player {
                    // Check if the target is a friendly transport/carrier with cargo space
                    let can_board_transport = ptype == PieceType::Army && other.ptype == PieceType::Transport && other.cargo.len() < 6;
                    let can_board_carrier = ptype == PieceType::Fighter && other.ptype == PieceType::Carrier && other.cargo.len() < 8;

                    if can_board_transport || can_board_carrier {
                        boardable_carrier_idx = Some(i);
                    } else {
                        return (MoveResult::InvalidMove, String::from("Sector is occupied by a friendly unit."));
                    }
                } else {
                    enemy_idx = Some(i);
                }
            }
        }

        // Handle Boarding a Transport/Carrier first
        if let Some(carrier_idx) = boardable_carrier_idx {
            let mut boarded_piece = self.pieces[piece_idx].clone();
            boarded_piece.loc = target_loc;
            boarded_piece.moves_left = 0;
            self.pieces[carrier_idx].cargo.push(boarded_piece);
            
            // "Destroy" the original map piece so it gets cleaned up by the engine array
            self.pieces[piece_idx].hits = 0;
            
            return (MoveResult::Moved, format!("Your {:?} successfully boarded the {:?}.", ptype, self.pieces[carrier_idx].ptype));
        }

        if let Some(e_idx) = enemy_idx {
            let mut rng = rand::thread_rng();
            let e_type = self.pieces[e_idx].ptype;
            
            while self.pieces[piece_idx].hits > 0 && self.pieces[e_idx].hits > 0 {
                if rng.gen_bool(0.5) { self.pieces[e_idx].hits -= 1; }
                if self.pieces[e_idx].hits > 0 && rng.gen_bool(0.5) { self.pieces[piece_idx].hits -= 1; }
            }

            if self.pieces[piece_idx].hits == 0 {
                return (MoveResult::CombatLost, format!(">>> DEFEAT! Your {:?} was DESTROYED by enemy {:?}! <<<", ptype, e_type));
            } else {
                self.pieces[piece_idx].loc = target_loc;
                self.pieces[piece_idx].moves_left -= 1;
                self.reveal_vision(target_loc, ptype.sight_range());
                
                // If we won combat and landed on an enemy city, capture it instantly!
                if ptype == PieceType::Army {
                    for (c_idx, city) in self.cities.iter_mut().enumerate() {
                        if city.loc == target_loc && city.owner != Owner::Player {
                            city.owner = Owner::Player;
                            self.pieces[piece_idx].hits = 0; // Army is consumed occupying the city!
                            return (MoveResult::CityCaptured(c_idx), format!(">>> VICTORY & CITY CAPTURED! You destroyed the defender and took ({},{})! <<<", target_loc.r, target_loc.c));
                        }
                    }
                }
                
                return (MoveResult::CombatWon, format!(">>> VICTORY! Your {:?} DESTROYED enemy {:?}! <<<", ptype, e_type));
            }
        }

        // 3. City Assault Check
        if ptype == PieceType::Army {
            for (c_idx, city) in self.cities.iter_mut().enumerate() {
                // Removed the restriction preventing attacks on Player-owned cities
                if city.loc == target_loc { 
                    let mut rng = rand::thread_rng();
                    let was_friendly = city.owner == Owner::Player;
                    
                    // Core Rule: Army is ALWAYS consumed when assaulting a city
                    self.pieces[piece_idx].hits = 0; 

                    if rng.gen_bool(0.5) {
                        city.owner = Owner::Player;
                        self.reveal_vision(target_loc, ptype.sight_range());
                        
                        if was_friendly {
                            return (MoveResult::CityCaptured(c_idx), format!(">>> YOU ATTACKED YOUR OWN CITY! Army destroyed, but the city survived at ({},{})! <<<", target_loc.r, target_loc.c));
                        } else {
                            return (MoveResult::CityCaptured(c_idx), format!(">>> CITY CAPTURED! You took ({},{})! <<<", target_loc.r, target_loc.c));
                        }
                    } else {
                        return (MoveResult::CityAssaultFailed, format!(">>> ASSAULT FAILED! Army DESTROYED at ({},{})! <<<", target_loc.r, target_loc.c));
                    }
                }
            }
        }

        // 4. Standard Move
        self.pieces[piece_idx].loc = target_loc;
        self.pieces[piece_idx].moves_left -= 1;
        self.reveal_vision(target_loc, ptype.sight_range());
        (MoveResult::Moved, String::new())
    }

    pub fn explore_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut explore_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            // Extract values early to keep the Rust borrow checker happy
            let ptype = self.pieces[piece_idx].ptype;
            let current_loc = self.pieces[piece_idx].loc;
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
                        let occupied_by_friend = self.pieces.iter().any(|other| other.loc == target_loc && other.owner == Owner::Player && other.hits > 0);
                        
                        if !friendly_city && !occupied_by_friend {
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
                    break; // Do not drain moves so the player is immediately reprompted!
                }

                // Only officially put the unit to sleep if it was explicitly told to Grope/Explore
                if self.pieces[piece_idx].activity == UnitActivity::Explore {
                    self.pieces[piece_idx].activity = UnitActivity::Sentry; // Actually put it to sleep
                    explore_logs.push(format!("{:?} mapped its entire accessible region and went to sleep.", ptype));
                }
                
                // If it is an Attacking unit that fell back to explore, it stays awake on silent overwatch.
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
                    self.default_orders.iter().find(|(p, _)| *p == city.prod).map(|(_, a)| *a).unwrap_or(UnitActivity::Idle)
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
        let mut wakeup_logs = Vec::new();

        let enemy_locs: Vec<Loc> = self.pieces.iter()
            .filter(|p| p.owner != Owner::Player && p.hits > 0)
            .map(|p| p.loc)
            .collect();

        for (idx, p) in self.pieces.iter_mut().enumerate() {
            if p.owner == Owner::Player {
                if p.moves_left <= 0 {
                    p.moves_left = p.ptype.max_moves();
                }

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
                        p.activity = UnitActivity::Idle;
                        wakeup_logs.push(format!("{:?} on sentry duty at ({},{}) spotted an enemy and woke up!", p.ptype, p.loc.r, p.loc.c));
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

        self.pieces.retain(|p| p.hits > 0);
    }
    pub fn attack_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut attack_logs = Vec::new();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            // ONLY target enemies that the player has actually discovered in the fog
            let enemy_locs: Vec<Loc> = self.pieces.iter()
                .filter(|p| p.owner != Owner::Player && p.hits > 0 && self.player_map[p.loc.r][p.loc.c] != 0)
                .map(|p| p.loc)
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
            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];

            // 1. Gather all physically valid moves
            let mut valid_steps = Vec::new();
            for &dir in &dirs {
                let target_r = (current_loc.r as i32 + dir.0) as usize;
                let target_c = (current_loc.c as i32 + dir.1) as usize;

                if target_r < self.rows && target_c < self.cols {
                    let is_land = self.map[target_r][target_c] == b'#';
                    let can_enter = match p.ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || p.ptype == PieceType::Fighter {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        let occupied_by_friend = self.pieces.iter().any(|other| other.loc == target_loc && other.owner == Owner::Player && other.hits > 0);
                        
                        if !friendly_city && !occupied_by_friend {
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
                // No known targets exist anywhere. Fall back to Explore logic to hunt through the fog, but STAY in Attack mode!
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                self.explore_piece(piece_idx);
                return; // Exit the attack loop entirely so explore takes over
            }

            // 2. Flood-Fill (BFS) to find the shortest actual path to any target
            // Create a fast-lookup map of friendly units to prevent BFS traffic jams
            let mut friendly_locs = vec![vec![false; self.cols]; self.rows];
            for other in &self.pieces {
                if other.owner == Owner::Player && other.hits > 0 {
                    friendly_locs[other.loc.r][other.loc.c] = true;
                }
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
                            let can_enter = match p.ptype {
                                PieceType::Army => is_land,
                                PieceType::Fighter | PieceType::Satellite => true,
                                _ => !is_land,
                            };

                            // Flow through valid terrain, BUT do NOT pathfind through friendly units
                            if can_enter && !friendly_locs[target_r][target_c] {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
                            }
                        }
                    }
                }
            }

            // If we couldn't find a path to any target, fall back to Explore logic but STAY in Attack mode
            if !found_target {
                // We deliberately DO NOT change the activity state here. 
                // It remains 'Attack' so it can instantly snap back to combat if an enemy appears next turn.
                
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                
                // Delegate the rest of this unit's moves to the explore logic
                self.explore_piece(piece_idx);
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
                        let occupied_by_friend = self.pieces.iter().any(|other| other.loc == target_loc && other.owner == Owner::Player && other.hits > 0);
                        
                        // Prevent random wanderers from suiciding into friendly cities or piling up on allies
                        if !friendly_city && !occupied_by_friend {
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
                    let is_land = self.map[r][c] == b'#';
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || ptype == PieceType::Fighter {
                        let step_loc = Loc { r, c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == step_loc && ct.owner == Owner::Player);
                        let occupied_by_friend = self.pieces.iter().any(|other| other.loc == step_loc && other.owner == Owner::Player && other.hits > 0);
                        
                        // Treat the exact destination as valid even if occupied, so we can march up to it
                        if (!friendly_city && !occupied_by_friend) || step_loc == target_loc {
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

            let mut friendly_locs = vec![vec![false; self.cols]; self.rows];
            for other in &self.pieces {
                if other.owner == Owner::Player && other.hits > 0 {
                    friendly_locs[other.loc.r][other.loc.c] = true;
                }
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
                        let can_enter = match ptype {
                            PieceType::Army => is_land,
                            PieceType::Fighter | PieceType::Satellite => true,
                            _ => !is_land,
                        };

                        if can_enter {
                            let loc_rc = Loc { r, c };
                            if !friendly_locs[r][c] || loc_rc == target_loc {
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

            let target_r = current_loc.r as i32 + dr;
            let target_c = current_loc.c as i32 + dc;

            // Halt if it marches off the edge of the world
            if target_r < 0 || target_r >= self.rows as i32 || target_c < 0 || target_c >= self.cols as i32 {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                dir_logs.push(format!("{:?} reached the edge of the map. March halted.", ptype));
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let target_loc = Loc { r: target_r as usize, c: target_c as usize };
            let is_land = self.map[target_loc.r][target_loc.c] == b'#';
            
            let can_enter = match ptype {
                PieceType::Army => is_land,
                _ => !is_land,
            };

            let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
            let occupied_by_friend = self.pieces.iter().any(|other| other.loc == target_loc && other.owner == Owner::Player && other.hits > 0);

            // Directional march halts immediately if blocked by terrain or friends
            if (!can_enter && ptype != PieceType::Fighter) || friendly_city || occupied_by_friend {
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
        let ptype = self.pieces[piece_idx].ptype;
        let cargo_count = self.pieces[piece_idx].cargo.len();
        
        let is_full = match ptype {
            PieceType::Transport => cargo_count >= 6,
            PieceType::Carrier => cargo_count >= 8,
            _ => false,
        };

        if is_full {
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            self.log(format!("{:?} is fully loaded with {} units and awaiting orders.", ptype, cargo_count));
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
}
