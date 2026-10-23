use crate::models::*;
use serde::{Serialize, Deserialize};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Percent of the playable map that is sea (the original game's default, `-w 70`).
const WATER_RATIO: usize = 70;
/// Number of smoothing passes over the height map (the original's default, `-s 5`).
const SMOOTH_PASSES: usize = 5;
const MAX_HEIGHT: usize = 999;

/// Maps a coordinate onto the playable interior (`1..=len-2`) of a wrapping axis.
fn wrap_axis(v: i64, len: usize) -> i64 {
    let n = len as i64 - 2;
    if v >= 1 && v <= n { v } else { (v - 1).rem_euclid(n) + 1 }
}

/// `loc` moved by (`dr`, `dc`), or `None` if that is off the map. On a torus map (`-t`),
/// stepping into the border ring lands on the opposite playable edge instead.
pub(crate) fn geo_offset(torus: bool, rows: usize, cols: usize, loc: Loc, dr: i32, dc: i32) -> Option<Loc> {
    let (mut r, mut c) = (loc.r as i64 + dr as i64, loc.c as i64 + dc as i64);
    if torus && rows > 2 && cols > 2 {
        r = wrap_axis(r, rows);
        c = wrap_axis(c, cols);
    }
    if r < 0 || c < 0 || r >= rows as i64 || c >= cols as i64 {
        None
    } else {
        Some(Loc { r: r as usize, c: c as usize })
    }
}

/// The original game's `dist()`: the larger of the row and column distances, each taking
/// the shorter way round on a torus map.
pub(crate) fn geo_dist(torus: bool, rows: usize, cols: usize, a: Loc, b: Loc) -> i32 {
    let mut dr = (a.r as i32 - b.r as i32).abs();
    let mut dc = (a.c as i32 - b.c as i32).abs();
    if torus {
        dr = dr.min(rows as i32 - 2 - dr);
        dc = dc.min(cols as i32 - 2 - dc);
    }
    dr.max(dc)
}

/// The size of the world in a normal game (the original's 60 x 100 map).
pub const MAP_ROWS: usize = 60;
pub const MAP_COLS: usize = 100;
/// Squares of map per city: the original's 70 cities on a 60 x 100 map.
const SQUARES_PER_CITY: usize = 85;
/// How far the player can see around the starting city at the start.
const START_SIGHT: i32 = 6;
/// Messages kept in the log.
const MAX_MESSAGES: usize = 100;

/// A fighter's fuel when full, in moves (the original's fighter range).
pub const FIGHTER_FUEL: i32 = 32;
/// A satellite's life in turns: the original's range of 500 moves at 10 moves a turn.
pub const SATELLITE_LIFE: i32 = 50;
/// The diagonals a satellite can travel (as `Dir` activities): NE, NW, SE, SW.
pub const SAT_DIRS: [UnitActivity; 4] = [
    UnitActivity::Dir(-1, 1), UnitActivity::Dir(-1, -1), UnitActivity::Dir(1, 1), UnitActivity::Dir(1, -1),
];

/// The work a city starts from when it switches to building `prod`: the original charges a
/// fifth of the build time to retool (`work = -(build_time / 5)`), and progress on the
/// old item is lost.
pub fn retool_work(prod: PieceType) -> i32 {
    -(prod.build_cost() / 5)
}

fn fresh_rng() -> StdRng {
    StdRng::from_entropy()
}

/// Version of the save-file format written by this build. Bump it whenever a change to
/// `Game` can't be handled by a `#[serde(default)]` on the new field, and add a
/// migration step to `Game::migrate`. Saves from before versioning count as version 0.
pub const SAVE_VERSION: u32 = 3;

/// Where a standing order for new units applies (`Game::set_standing_order`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderScope {
    /// Every player city without its own order for that unit type.
    Global,
    /// One city (an index into `Game::cities`).
    City(usize),
}

/// A continent a side may start on (the original's `cont_t`): its value and its cities
/// (indices into `Game::cities`).
#[derive(Clone, Debug)]
pub struct StartContinent {
    pub value: i64,
    pub cities: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    /// Save-file format version (see `SAVE_VERSION`).
    #[serde(default)]
    pub save_version: u32,
    pub rows: usize,
    pub cols: usize,
    pub turn: i32,
    pub difficulty: u32,
    pub map: Vec<Vec<Terrain>>,
    /// The computer's view of the world, as in the original: what its units and cities
    /// have seen, as map characters (see `ai.rs`); 0 or `' '` is unexplored.
    pub comp_map: Vec<Vec<u8>>,
    pub player_map: Vec<Vec<u8>>,
    /// What the player last saw of enemy units on each square, as the unit's map letter
    /// (lower case: the computer's); 0 means none seen there. Like the original's user view
    /// map, it only changes when the square is in sight, so it can be out of date.
    #[serde(default)]
    pub player_view: Vec<Vec<u8>>,
    /// Cities the computer has won since its last move, whose production it still has to
    /// choose.
    #[serde(default)]
    pub comp_new_cities: Vec<usize>,
    /// Fighter statistics for the player (index 0) and the computer (index 1), for the
    /// `--headless` playtest rig. Not saved.
    #[serde(skip)]
    pub fighter_stats: [FighterStats; 2],
    /// The computer has acknowledged defeat and the player chose to fight on (the
    /// original's `resigned`). Its map can then be examined (`View::show_comp_view`).
    #[serde(default)]
    pub comp_resigned: bool,
    /// The player's side is played by the AI too (`--headless`): it moves after the
    /// player's automatic orders would, and before the computer. Not saved.
    #[serde(skip)]
    pub player_ai: bool,
    /// The AI's view of the world when it plays the player's side (as `comp_map` is for
    /// the computer). Not saved.
    #[serde(skip)]
    pub player_ai_map: Vec<Vec<u8>>,
    /// Debug observer mode (`~`): the player sees everything, live.
    #[serde(skip)]
    pub omniscient: bool,
    pub cities: Vec<City>,
    pub pieces: Vec<Piece>,
    pub messages: Vec<String>,
    /// How many messages have ever been logged (the log itself keeps only the last
    /// `MAX_MESSAGES`): lets a view notice new ones. Not saved.
    #[serde(skip)]
    pub messages_logged: u64,
    pub pending_cities: Vec<usize>,
    pub player_score: u32,
    pub comp_score: u32,
    pub default_orders: Vec<(PieceType, UnitActivity)>,
    /// Next piece id to hand out. Ids are never reused, even after a piece dies
    /// or while it is riding in a transport's cargo hold.
    #[serde(default)]
    pub next_id: usize,
    /// Source of all randomness (map generation, combat, AI wiggle). Not saved:
    /// a loaded game gets a fresh entropy-seeded generator. Use `with_seed` /
    /// `seed_rng` to make a game reproducible.
    #[serde(skip, default = "fresh_rng")]
    pub rng: StdRng,
    /// Ids of pieces that changed location since the last turn boundary. A damaged boat
    /// in its own port that did not move this turn repairs one hit, as in the original.
    #[serde(default)]
    pub moved_this_turn: std::collections::HashSet<usize>,
    /// `-t`: the playable map wraps around at its top, bottom, left and right edges.
    #[serde(default)]
    pub torus: bool,
}

impl Game {
    pub fn new(rows: usize, cols: usize, difficulty: u32) -> Self {
        let map = vec![vec![Terrain::Sea; cols]; rows];
        let comp_map = vec![vec![0; cols]; rows];
        let player_map = vec![vec![0; cols]; rows];
        let player_view = vec![vec![0; cols]; rows];
        Self {
            save_version: SAVE_VERSION,
            rows,
            cols,
            turn: 1,
            difficulty,
            map,
            comp_map,
            player_map,
            player_view,
            omniscient: false,
            player_ai: false,
            player_ai_map: Vec::new(),
            comp_resigned: false,
            fighter_stats: [FighterStats::default(); 2],
            comp_new_cities: Vec::new(),
            cities: Vec::new(),
            pieces: Vec::new(),
            messages: Vec::new(),
            messages_logged: 0,
            pending_cities: Vec::new(),
            player_score: 0,
            comp_score: 0,
            next_id: 1,
            rng: fresh_rng(),
            moved_this_turn: std::collections::HashSet::new(),
            torus: false,
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

    /// A new game whose map, combat and AI choices are fully determined by `seed`.
    pub fn with_seed(rows: usize, cols: usize, difficulty: u32, seed: u64) -> Self {
        let mut game = Self::new(rows, cols, difficulty);
        game.seed_rng(seed);
        game
    }

    pub fn seed_rng(&mut self, seed: u64) {
        self.rng = StdRng::seed_from_u64(seed);
    }

    /// Sets what city `c_idx` builds. Changing production costs the retooling penalty
    /// (`retool_work`), as does choosing production for a newly won city (`new_city`),
    /// which starts from scratch whatever it was building before.
    pub fn set_production(&mut self, c_idx: usize, prod: PieceType, new_city: bool) {
        let city = &mut self.cities[c_idx];
        if new_city || city.prod != prod {
            city.work = retool_work(prod);
        }
        city.prod = prod;
    }

    /// Builds the land/water map the way the original game does: random heights, smoothed
    /// by repeated 3x3 averaging (so land and water clump), then the water line is placed
    /// so that exactly `WATER_RATIO` percent of the playable (non-border) squares are sea.
    /// Squares tied at the water line are split at random to hit the ratio exactly. The
    /// outer ring of the map is always sea.
    pub fn generate_map(&mut self) {
        let (rows, cols) = (self.rows, self.cols);
        let size = rows * cols;
        let mut from: Vec<u32> = (0..size).map(|_| self.rng.gen_range(0..MAX_HEIGHT as u32)).collect();
        let mut to = vec![0u32; size];

        // The original works on a flat array, so "neighbours" of an edge cell wrap into the
        // adjacent row, and anything off either end of the array counts as the cell itself.
        let w = cols as isize;
        let offsets = [-w - 1, -w, -w + 1, -1, 1, w - 1, w, w + 1];
        for _ in 0..SMOOTH_PASSES {
            for j in 0..size {
                let mut sum = from[j];
                for off in offsets {
                    let loc = j as isize + off;
                    let loc = if loc < 0 || loc >= size as isize { j } else { loc as usize };
                    sum += from[loc];
                }
                to[j] = sum / 9;
            }
            std::mem::swap(&mut from, &mut to);
        }
        let height = from;

        let on_board = |i: usize| {
            let (r, c) = (i / cols, i % cols);
            !(r == 0 || r == rows - 1 || c == 0 || c == cols - 1)
        };
        let mut height_count = vec![0usize; MAX_HEIGHT + 1];
        let mut playable = 0usize;
        for i in 0..size {
            if on_board(i) {
                height_count[height[i] as usize] += 1;
                playable += 1;
            }
        }

        // Leave enough land for every city, then find the height at which we have enough water.
        let num_city = size / SQUARES_PER_CITY;
        let mut target_water = playable * WATER_RATIO / 100;
        if target_water > playable.saturating_sub(num_city) {
            target_water = playable.saturating_sub(num_city);
        }
        let mut water_line = MAX_HEIGHT;
        let mut sum = 0usize;
        for (h, &count) in height_count.iter().enumerate() {
            if sum + count >= target_water {
                water_line = h;
                break;
            }
            sum += count;
        }
        let water_at_line = target_water - sum;

        let mut tied = Vec::new();
        for i in 0..size {
            let (r, c) = (i / cols, i % cols);
            if !on_board(i) || (height[i] as usize) < water_line {
                self.map[r][c] = Terrain::Sea;
            } else {
                self.map[r][c] = Terrain::Land;
                if height[i] as usize == water_line {
                    tied.push(i);
                }
            }
        }
        // Randomly turn just enough of the squares tied at the line into water.
        for i in 0..water_at_line.min(tied.len()) {
            let choice = i + self.rng.gen_range(0..tied.len() - i);
            tied.swap(i, choice);
            self.map[tied[i] / cols][tied[i] % cols] = Terrain::Sea;
        }
    }

    /// First id that is safe to assign: above the stored counter and above every id
    /// currently in play, including pieces carried in cargo (which are not in `self.pieces`).
    pub(crate) fn next_free_id(&self) -> usize {
        fn max_id(pieces: &[Piece]) -> usize {
            pieces.iter().map(|p| p.id.max(max_id(&p.cargo))).max().unwrap_or(0)
        }
        self.next_id.max(max_id(&self.pieces) + 1)
    }

    pub fn dist_to_nearest_base(&self, loc: Loc) -> Option<i32> {
        let mut min_dist: Option<i32> = None;
        for city in &self.cities {
            if city.owner == Owner::Player {
                let dist = self.dist(loc, city.loc);
                min_dist = Some(min_dist.unwrap_or(dist).min(dist));
            }
        }
        for piece in &self.pieces {
            if piece.owner == Owner::Player && piece.ptype == PieceType::Carrier && piece.hits > 0 {
                let dist = self.dist(loc, piece.loc);
                min_dist = Some(min_dist.unwrap_or(dist).min(dist));
            }
        }
        min_dist
    }

    /// Room left aboard `carrier` (a transport or carrier) for one more piece, ignoring the
    /// piece with id `mover` (the one asking to board, which may already be on deck).
    ///
    /// A carrier's load is its hold (`cargo`) plus any awake pieces of the type it carries
    /// sharing its square at sea: those are standing on deck and sail with it. In a city
    /// such pieces are just in the city, so they don't count.
    pub fn free_berths(&self, carrier: &Piece, mover: Option<usize>) -> usize {
        let Some(cargo_type) = carrier.ptype.cargo_type() else { return 0 };
        if carrier.hits <= 0 {
            return 0;
        }
        let capacity = carrier.ptype.capacity_for_hits(carrier.hits);
        let in_city = self.cities.iter().any(|c| c.loc == carrier.loc);
        let on_deck = if in_city {
            0
        } else {
            self.pieces.iter()
                .filter(|p| p.loc == carrier.loc && p.owner == carrier.owner && p.hits > 0
                    && p.ptype == cargo_type && Some(p.id) != mover)
                .count()
        };
        capacity.saturating_sub(carrier.cargo.len() + on_deck)
    }

    /// True if `carrier` can take the piece with id `mover` aboard.
    pub fn has_room_for(&self, carrier: &Piece, mover: Option<usize>) -> bool {
        self.free_berths(carrier, mover) > 0
    }

    /// Drowns armies left standing on open sea with no transport of their own under them
    /// (their transport sank, or sailed off without them). Returns how many of the
    /// player's armies were lost.
    fn drown_stranded_armies(&mut self) -> usize {
        let transport_squares: std::collections::HashSet<(Loc, Owner)> = self.pieces.iter()
            .filter(|p| p.ptype == PieceType::Transport && p.hits > 0)
            .map(|p| (p.loc, p.owner))
            .collect();
        let mut player_lost = 0;
        for i in 0..self.pieces.len() {
            let p = &self.pieces[i];
            if p.ptype == PieceType::Army && p.hits > 0
                && self.map[p.loc.r][p.loc.c] == Terrain::Sea
                && !transport_squares.contains(&(p.loc, p.owner))
            {
                if p.owner == Owner::Player {
                    player_lost += 1;
                }
                self.pieces[i].hits = 0;
            }
        }
        player_lost
    }

    /// Moves along any awake pieces standing on the deck of the transport or carrier at
    /// `piece_idx`, which has just sailed from `old_loc`, as far as its capacity allows.
    fn drag_deck_cargo(&mut self, piece_idx: usize, old_loc: Loc) {
        let ptype = self.pieces[piece_idx].ptype;
        let Some(cargo_type) = ptype.cargo_type() else { return };
        let owner = self.pieces[piece_idx].owner;
        let new_loc = self.pieces[piece_idx].loc;
        let capacity = ptype.capacity_for_hits(self.pieces[piece_idx].hits);
        let mut deck_space = capacity.saturating_sub(self.pieces[piece_idx].cargo.len());
        for i in 0..self.pieces.len() {
            if deck_space == 0 { break; }
            let p = &self.pieces[i];
            if i != piece_idx && p.loc == old_loc && p.owner == owner && p.hits > 0 && p.ptype == cargo_type {
                self.pieces[i].loc = new_loc;
                deck_space -= 1;
            }
        }
    }

    /// Radius within which a player's city sees: its own square and the eight around it, as
    /// in the original's scan of each city every turn.
    pub const CITY_SIGHT: i32 = 1;

    /// Which squares the player can see right now: within sight range of one of the
    /// player's units, or next to one of the player's cities. (Everything, in debug
    /// observer mode.)
    pub fn player_sight(&self) -> Vec<Vec<bool>> {
        let mut grid = vec![vec![self.omniscient; self.cols]; self.rows];
        if self.omniscient {
            return grid;
        }
        let centers = self.pieces.iter()
            .filter(|p| p.owner == Owner::Player && p.hits > 0)
            .map(|p| (p.loc, p.ptype.sight_range()))
            .chain(self.cities.iter().filter(|c| c.owner == Owner::Player).map(|c| (c.loc, Self::CITY_SIGHT)));
        for (loc, range) in centers {
            for dr in -range..=range {
                for dc in -range..=range {
                    if let Some(l) = self.offset(loc, dr, dc) {
                        grid[l.r][l.c] = true;
                    }
                }
            }
        }
        grid
    }

    /// True if the player can see `loc` right now (see `player_sight`).
    pub fn player_sees(&self, loc: Loc) -> bool {
        self.omniscient
            || self.pieces.iter().any(|p| p.owner == Owner::Player && p.hits > 0 && self.dist(p.loc, loc) <= p.ptype.sight_range())
            || self.cities.iter().any(|c| c.owner == Owner::Player && self.dist(c.loc, loc) <= Self::CITY_SIGHT)
    }

    /// True if `viewer` can see the piece at `target_idx` right now. The player sees enemy
    /// units only on squares in sight (and submarines only from next door, or from a
    /// destroyer in range); the computer currently sees everything.
    pub fn is_visible_to(&self, target_idx: usize, viewer: Owner) -> bool {
        self.is_visible_with(target_idx, viewer, None)
    }

    /// `is_visible_to`, with the player's sight grid already worked out (for loops).
    pub fn is_visible_with(&self, target_idx: usize, viewer: Owner, sight: Option<&Vec<Vec<bool>>>) -> bool {
        let target = &self.pieces[target_idx];
        if target.owner == viewer { return true; }
        if viewer == Owner::Player && self.omniscient { return true; }
        
        if target.ptype != PieceType::Submarine {
            if viewer == Owner::Player {
                return match sight {
                    Some(grid) => grid[target.loc.r][target.loc.c],
                    None => self.player_sees(target.loc),
                };
            }
            return true;
        }

        for p in &self.pieces {
            if p.owner == viewer && p.hits > 0 {
                let dist = self.dist(p.loc, target.loc);

                if p.ptype == PieceType::Destroyer {
                    if dist <= p.ptype.sight_range() { return true; }
                } else {
                    if dist <= 1 { return true; }
                }
            }
        }
        for c in &self.cities {
            if c.owner == viewer
                && self.dist(c.loc, target.loc) <= 1 { return true; }
        }
        false
    }

    /// `loc` moved by (`dr`, `dc`), wrapping across the seams on a torus map.
    /// `None` if the result is off the map.
    pub fn offset(&self, loc: Loc, dr: i32, dc: i32) -> Option<Loc> {
        geo_offset(self.torus, self.rows, self.cols, loc, dr, dc)
    }

    /// One step in direction `dir` (a `(row, col)` delta), with wrapping.
    pub fn step(&self, loc: Loc, dir: (i32, i32)) -> Option<Loc> {
        self.offset(loc, dir.0, dir.1)
    }

    /// Distance between two squares as the original game measures it (see `geo_dist`).
    pub fn dist(&self, a: Loc, b: Loc) -> i32 {
        geo_dist(self.torus, self.rows, self.cols, a, b)
    }

    /// The up-to-8 squares surrounding `loc` (not including `loc` itself); across the
    /// seams on a torus map.
    pub fn neighbors(&self, loc: Loc) -> impl Iterator<Item = Loc> {
        let (torus, rows, cols) = (self.torus, self.rows, self.cols);
        (-1i32..=1).flat_map(move |dr| (-1i32..=1).map(move |dc| (dr, dc)))
            .filter(|&d| d != (0, 0))
            .filter_map(move |(dr, dc)| geo_offset(torus, rows, cols, loc, dr, dc))
            .filter(move |&n| n != loc)
    }

    /// Moves a cursor or view centre by (`dr`, `dc`): clamped to the map normally,
    /// wrapping on a torus map.
    pub fn shift_cursor(&self, loc: Loc, dr: i32, dc: i32) -> Loc {
        if self.torus {
            self.offset(loc, dr, dc).unwrap_or(loc)
        } else {
            Loc {
                r: (loc.r as i32 + dr).clamp(0, self.rows as i32 - 1) as usize,
                c: (loc.c as i32 + dc).clamp(0, self.cols as i32 - 1) as usize,
            }
        }
    }

    pub fn is_land(&self, loc: Loc) -> bool {
        self.map[loc.r][loc.c] == Terrain::Land
    }

    /// True if the tile, or any tile next to it, is sea.
    pub fn is_coastal(&self, loc: Loc) -> bool {
        !self.is_land(loc) || self.neighbors(loc).any(|n| !self.is_land(n))
    }

    pub fn is_tile_blocked_by_friend(&self, loc: Loc, moving_ptype: PieceType, owner: Owner) -> bool {
        if self.cities.iter().any(|c| c.loc == loc && c.owner == owner) {
            return false;
        }
        // Satellites orbit above everything and never share a square with anyone.
        if moving_ptype == PieceType::Satellite {
            return false;
        }
        let mut can_board = false;
        let mut has_blocker = false;
        for other in &self.pieces {
            if other.loc != loc || other.owner != owner || other.hits <= 0 { continue; }
            if other.ptype == PieceType::Satellite { continue; }
            let is_fighter_flyover = (moving_ptype == PieceType::Fighter && other.ptype != PieceType::Fighter) ||
                                     (moving_ptype != PieceType::Fighter && other.ptype == PieceType::Fighter);
            let boards = other.ptype.cargo_type() == Some(moving_ptype) && self.has_room_for(other, None);
            let can_board_transport = boards && moving_ptype == PieceType::Army;
            let can_board_carrier = boards && moving_ptype == PieceType::Fighter;
            
            if can_board_transport || can_board_carrier {
                can_board = true;
            } else if !is_fighter_flyover {
                has_blocker = true;
            }
        }
        has_blocker && !can_board
    }

    /// Picks city sites the way the original does: choose random land squares one at a
    /// time, discarding land closer than `min_dist` to a chosen city. The minimum distance
    /// starts at the square root of the expected land per city and is reduced by one each
    /// time the candidates run out, so the full number of cities is always placed.
    pub(crate) fn place_cities(&mut self, land: &[Loc], count: usize) -> Vec<Loc> {
        let (torus, rows, cols) = (self.torus, self.rows, self.cols);
        let playable = (self.rows - 2) * (self.cols - 2);
        let expected_land = playable - playable * WATER_RATIO / 100;
        let mut min_dist = ((expected_land / count.max(1)) as f64).sqrt() as i32;
        let mut placed: Vec<Loc> = Vec::new();
        let mut candidates: Vec<Loc> = land.to_vec();
        while placed.len() < count.min(land.len()) {
            if candidates.is_empty() {
                if !placed.is_empty() && min_dist > 0 {
                    min_dist -= 1;
                }
                candidates = land.iter().cloned()
                    .filter(|l| !placed.contains(l) && placed.iter().all(|p| geo_dist(torus, rows, cols, *p, *l) >= min_dist))
                    .collect();
                if candidates.is_empty() {
                    break; // no land left at all
                }
                continue;
            }
            let loc = candidates.swap_remove(self.rng.gen_range(0..candidates.len()));
            placed.push(loc);
            candidates.retain(|l| geo_dist(torus, rows, cols, loc, *l) >= min_dist);
        }
        placed
    }

    /// Generates the world and picks both starting cities at difficulty level
    /// `self.difficulty` (limited to the levels this world offers). The interactive game
    /// asks for the level in between instead (`generate_world`, then `choose_start`).
    #[allow(dead_code)] // used by the tests
    pub fn setup_initial_states(&mut self) {
        let levels = self.generate_world();
        let level = (self.difficulty as usize).min(levels - 1);
        self.choose_start(level);
    }

    /// Makes the land and sea and places the cities, as the original's `init_game`:
    /// cities are placed again (and, should that keep failing, the map remade) until at
    /// least one continent is fit to start on (see `start_continents`). Returns the
    /// number of difficulty levels this world offers (`0` easiest to `levels - 1`).
    pub fn generate_world(&mut self) -> usize {
        let mut attempts = 0;
        loop {
            if attempts % 20 == 0 {
                self.generate_map();
            }
            attempts += 1;
            self.cities.clear();
            let land_tiles: Vec<Loc> = (1..self.rows - 1)
                .flat_map(|r| (1..self.cols - 1).map(move |c| Loc { r, c }))
                .filter(|&l| self.map[l.r][l.c] == Terrain::Land)
                .collect();
            let target_cities = (self.rows * self.cols) / SQUARES_PER_CITY;
            for loc in self.place_cities(&land_tiles, target_cities) {
                self.cities.push(City {
                    loc, owner: Owner::Neutral, prod: PieceType::Army, work: 0, default_orders: Vec::new(), seen_owner: None,
                });
            }
            let n = self.start_continents().len();
            if n > 0 {
                return n * n;
            }
            if attempts >= 200 && self.cities.len() >= 2 {
                return 1; // a world too small for the rules: see `start_pairs`
            }
        }
    }

    /// The continents a side may start on, as the original's `find_cont`: up to the first
    /// ten (scanning the map row by row) that have at least two cities, one of them on the
    /// shore of the open ocean (`open_ocean`; the original accepted any shore, lakes too). Each gets a value: cities beyond the first two count most (a shore city
    /// 3, an inland city 2, times 1000), then land area.
    pub fn start_continents(&self) -> Vec<StartContinent> {
        const MAX_CONT: usize = 10;
        let on_board = |l: Loc| l.r >= 1 && l.c >= 1 && l.r + 1 < self.rows && l.c + 1 < self.cols;
        let ocean = self.open_ocean();
        let mut marked = vec![vec![false; self.cols]; self.rows];
        let mut found = Vec::new();
        for r in 0..self.rows {
            for c in 0..self.cols {
                if found.len() >= MAX_CONT {
                    return found;
                }
                let start = Loc { r, c };
                if !on_board(start) || marked[r][c] || !self.is_land(start) {
                    continue;
                }
                // mark_cont: count the land, cities and shore cities of this continent
                let (mut nland, mut cities, mut nshore) = (0i64, Vec::new(), 0i64);
                let mut stack = vec![start];
                marked[r][c] = true;
                while let Some(l) = stack.pop() {
                    nland += 1;
                    if let Some(ci) = self.cities.iter().position(|ct| ct.loc == l) {
                        cities.push(ci);
                        // A port on the open ocean (QOL change: the original also counted
                        // a city on a lake, which can leave a side with no way off its
                        // continent).
                        if self.neighbors(l).any(|n| on_board(n) && ocean[n.r][n.c]) {
                            nshore += 1;
                        }
                    }
                    for n in self.neighbors(l) {
                        if on_board(n) && !marked[n.r][n.c] && self.is_land(n) {
                            marked[n.r][n.c] = true;
                            stack.push(n);
                        }
                    }
                }
                let ncity = cities.len() as i64;
                if nshore < 1 || ncity < 2 {
                    continue;
                }
                // The first two cities, one of them a shore city, don't count.
                let mut value = if ncity == nshore {
                    (nshore - 2) * 3
                } else {
                    (nshore - 1) * 3 + (ncity - nshore - 1) * 2
                };
                value = value * 1000 + nland;
                found.push(StartContinent { value, cities });
            }
        }
        found
    }

    /// The open ocean: the largest connected body of water on the playable map. Every
    /// other body of water is a lake. (On a torus map, water connects across the seams.)
    pub fn open_ocean(&self) -> Vec<Vec<bool>> {
        let on_board = |l: Loc| l.r >= 1 && l.c >= 1 && l.r + 1 < self.rows && l.c + 1 < self.cols;
        let mut label = vec![vec![0usize; self.cols]; self.rows];
        let mut sizes = vec![0usize]; // body 0 = not water
        for r in 0..self.rows {
            for c in 0..self.cols {
                let start = Loc { r, c };
                if !on_board(start) || label[r][c] != 0 || self.is_land(start) {
                    continue;
                }
                let id = sizes.len();
                let mut size = 0;
                let mut stack = vec![start];
                label[r][c] = id;
                while let Some(l) = stack.pop() {
                    size += 1;
                    for n in self.neighbors(l) {
                        if on_board(n) && label[n.r][n.c] == 0 && !self.is_land(n) {
                            label[n.r][n.c] = id;
                            stack.push(n);
                        }
                    }
                }
                sizes.push(size);
            }
        }
        let biggest = (1..sizes.len()).max_by_key(|&i| (sizes[i], std::cmp::Reverse(i))).unwrap_or(0);
        label.iter().map(|row| row.iter().map(|&id| id != 0 && id == biggest).collect()).collect()
    }

    /// Every (player continent, computer continent) pair, easiest for the player first:
    /// ranked by how much better the player's continent is than the computer's (the
    /// original's `make_pair`). Both sides may get the same continent. The position in
    /// this list is the difficulty level.
    pub fn start_pairs(&self) -> Vec<(usize, usize)> {
        let conts = self.start_continents();
        let n = conts.len();
        let mut pairs: Vec<(i64, usize, usize)> = (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| (conts[i].value - conts[j].value, i, j))
            .collect();
        pairs.sort_by_key(|p| std::cmp::Reverse(p.0)); // stable: ties keep their order
        pairs.into_iter().map(|(_, i, j)| (i, j)).collect()
    }

    /// Gives each side its first city for difficulty `level` (`0` is easiest), as the
    /// original's `select_cities`: a random city on each side's continent, never the same
    /// one. The player's city gets a first look around; the computer starts building
    /// armies.
    pub fn choose_start(&mut self, level: usize) {
        let conts = self.start_continents();
        let (user_cities, comp_cities) = if conts.is_empty() {
            // A world too small for the rules: any two cities will do.
            let all: Vec<usize> = (0..self.cities.len()).collect();
            (all.clone(), all)
        } else {
            let pairs = self.start_pairs();
            let (u, c) = pairs[level.min(pairs.len() - 1)];
            (conts[u].cities.clone(), conts[c].cities.clone())
        };
        let comp_city = comp_cities[self.rng.gen_range(0..comp_cities.len())];
        let user_city = if user_cities.iter().any(|&c| c != comp_city) {
            loop {
                let pick = user_cities[self.rng.gen_range(0..user_cities.len())];
                if pick != comp_city {
                    break pick;
                }
            }
        } else {
            // only reachable on a degenerate world with a single eligible city
            (0..self.cities.len()).find(|&c| c != comp_city).unwrap_or(comp_city)
        };
        self.difficulty = level as u32;
        self.cities[comp_city].owner = Owner::Comp;
        self.cities[comp_city].prod = PieceType::Army;
        self.cities[comp_city].work = 0;
        self.cities[user_city].owner = Owner::Player;
        self.cities[user_city].work = 0;
        let loc = self.cities[user_city].loc;
        self.reveal_vision(loc, START_SIGHT);

        self.log(format!("Classic VMS-Empire map generated. Difficulty level {}.", level));
        self.log(format!("Game initialized. {} cities scattered across the world.", self.cities.len()));
        self.log(format!("Your city is at ({},{}).", loc.r, loc.c));
    }

    /// An army's assault on a neutral or enemy city, as in the original game's `attack_city`.
    ///
    /// It is a pure 50% coin flip and the army is spent either way. Pieces inside the city
    /// take no part in the fight. If the assault succeeds the city falls (`kill_city`):
    /// the previous owner's armies in it die, its other pieces (ships, fighters) change
    /// sides, a transport's cargo is lost, and the city's production and standing orders
    /// are reset. Satellites are unaffected.
    fn assault_city(&mut self, piece_idx: usize, c_idx: usize) -> (MoveResult, String) {
        let unit_owner = self.pieces[piece_idx].owner;
        let is_player = unit_owner == Owner::Player;
        let ptype = self.pieces[piece_idx].ptype;
        let loc = self.cities[c_idx].loc;
        let city_owner = self.cities[c_idx].owner;
        let was_friendly = city_owner == Owner::Player;

        self.pieces[piece_idx].hits = 0;

        if self.rng.gen_bool(0.5) {
            for p in self.pieces.iter_mut() {
                if p.loc != loc || p.hits <= 0 || p.ptype == PieceType::Satellite || p.owner != city_owner {
                    continue;
                }
                if p.ptype == PieceType::Army {
                    p.hits = 0;
                    continue;
                }
                if p.ptype == PieceType::Transport {
                    p.cargo.clear();
                } else {
                    for c in p.cargo.iter_mut() {
                        c.owner = unit_owner;
                        c.activity = UnitActivity::Idle;
                    }
                }
                p.owner = unit_owner;
                p.activity = UnitActivity::Idle;
            }

            let city = &mut self.cities[c_idx];
            city.owner = unit_owner;
            city.work = 0;
            city.default_orders.clear();
            if unit_owner == Owner::Comp || self.player_ai {
                // A captured city starts from scratch, so even armies pay the retooling cost.
                city.prod = PieceType::Army;
                city.work = retool_work(PieceType::Army);
                // The AI picks what the city really builds at the start of its next move
                // (`comp_prod`), as the original does for a city with no production.
                if unit_owner == Owner::Comp {
                    self.comp_new_cities.push(c_idx);
                } else {
                    self.pending_cities.push(c_idx);
                }
            }

            if is_player {
                self.reveal_vision(loc, ptype.sight_range());
                (MoveResult::CityCaptured(c_idx), format!(">>> CITY CAPTURED! You took ({},{})! <<<", loc.r, loc.c))
            } else if was_friendly {
                // The player is told, so the map shows the loss even out of sight.
                self.cities[c_idx].seen_owner = Some(unit_owner);
                (MoveResult::CityCaptured(c_idx), format!(">>> ALERT! The enemy captured your city at ({},{})! <<<", loc.r, loc.c))
            } else {
                (MoveResult::CityCaptured(c_idx), String::new())
            }
        } else {
            if was_friendly {
                self.player_score += ptype.build_cost() as u32;
            } else if city_owner == Owner::Comp {
                self.comp_score += ptype.build_cost() as u32;
            }

            if is_player {
                (MoveResult::CityAssaultFailed, format!(">>> ASSAULT FAILED! Army DESTROYED at ({},{})! <<<", loc.r, loc.c))
            } else if was_friendly {
                (MoveResult::CityAssaultFailed, format!(">>> DEFENSE! Enemy army DESTROYED attacking your city at ({},{})! <<<", loc.r, loc.c))
            } else {
                (MoveResult::CityAssaultFailed, String::new())
            }
        }
    }

    /// True if moving `ptype` onto `loc` is suicide, as in the original game: a fighter that
    /// flies into a neutral or enemy city is shot down, and a ship that tries to enter one
    /// breaks up on shore. (Only armies can take a city; see `assault_city`.)
    pub fn is_fatal_city_entry(&self, ptype: PieceType, owner: Owner, loc: Loc) -> bool {
        (ptype == PieceType::Fighter || ptype.is_naval())
            && self.cities.iter().any(|c| c.loc == loc && c.owner != owner)
    }

    /// True if moving onto `loc` is a terrain suicide as in the original game, where the unit
    /// is lost whatever happens (after winning any fight there):
    /// - a ship that goes ashore breaks up on shore;
    /// - an army that walks into the sea, or onto a transport that is full, drowns.
    ///
    /// Cities are handled separately (`is_fatal_city_entry`, `assault_city`).
    pub fn is_suicidal_terrain_entry(&self, ptype: PieceType, owner: Owner, loc: Loc) -> bool {
        self.is_suicidal_entry_for(ptype, owner, loc, None)
    }

    /// `is_suicidal_terrain_entry` for the piece with id `mover`, which is left out when
    /// counting who is already aboard a transport (it may be standing on its deck).
    fn is_suicidal_entry_for(&self, ptype: PieceType, owner: Owner, loc: Loc, mover: Option<usize>) -> bool {
        let is_land = self.map[loc.r][loc.c] == Terrain::Land;
        let own_piece_here = |pred: &dyn Fn(&Piece) -> bool| {
            self.pieces.iter().any(|p| p.loc == loc && p.owner == owner && p.hits > 0 && pred(p))
        };
        if ptype.is_naval() {
            is_land
                && !self.cities.iter().any(|c| c.loc == loc)
                && !own_piece_here(&|p| p.ptype != PieceType::Satellite && p.ptype != PieceType::Fighter)
        } else if ptype == PieceType::Army {
            !is_land
                && !own_piece_here(&|p| p.ptype == PieceType::Transport && self.has_room_for(p, mover))
                && !own_piece_here(&|p| p.ptype != PieceType::Satellite && p.ptype != PieceType::Fighter && p.ptype != PieceType::Transport)
        } else {
            false
        }
    }

    /// The question the original game asks before a move that destroys the unit, or `None`
    /// if the move is safe.
    pub fn suicidal_move_prompt(&self, ptype: PieceType, owner: Owner, loc: Loc) -> Option<&'static str> {
        if self.is_fatal_city_entry(ptype, owner, loc) {
            Some("That's never worked before, sir.  Do you really want to try? [Y/N]: ")
        } else if !self.is_suicidal_terrain_entry(ptype, owner, loc) {
            None
        } else if ptype == PieceType::Army {
            if self.pieces.iter().any(|p| p.loc == loc && p.owner == owner && p.hits > 0 && p.ptype == PieceType::Transport) {
                Some("Sorry, sir.  There is no more room on the transport.  Do you insist? [Y/N]: ")
            } else {
                Some("Troops can't walk on water, sir.  Do you really want to go to sea? [Y/N]: ")
            }
        } else {
            Some("Ships need sea to float, sir.  Do you really want to go ashore? [Y/N]: ")
        }
    }

    /// Chooses which enemy piece on a tile defends against `attacker`.
    ///
    /// Air fights air and ground fights ground: a fighter must beat the enemy fighter on the
    /// tile before it can fight the unit underneath, and every other attacker must beat the
    /// surface unit before it can fight an enemy fighter flying over it. Only when nothing
    /// on the attacker's plane is left does the other plane defend. Within a plane (a
    /// city can hold several units) the highest piece type in the original game's ordering
    /// defends, first listed winning ties, as in VMS-Empire's `find_obj_at_loc`.
    fn pick_defender(&self, candidates: &[usize], attacker: PieceType) -> Option<usize> {
        // Order of the piece types in the original game (empire.h).
        fn original_rank(p: PieceType) -> u8 {
            match p {
                PieceType::Army => 0,
                PieceType::Fighter => 1,
                PieceType::PatrolBoat => 2,
                PieceType::Destroyer => 3,
                PieceType::Submarine => 4,
                PieceType::Transport => 5,
                PieceType::Carrier => 6,
                PieceType::Battleship => 7,
                PieceType::Satellite => 8,
            }
        }
        let best = |same_plane_only: bool| -> Option<usize> {
            let mut best: Option<usize> = None;
            for &i in candidates {
                let ptype = self.pieces[i].ptype;
                if same_plane_only && (ptype == PieceType::Fighter) != (attacker == PieceType::Fighter) {
                    continue;
                }
                if !matches!(best, Some(b) if original_rank(ptype) <= original_rank(self.pieces[b].ptype)) {
                    best = Some(i);
                }
            }
            best
        };
        best(true).or_else(|| best(false))
    }

    pub fn attempt_move(&mut self, piece_idx: usize, target_loc: Loc) -> (MoveResult, String) {
        let (result, mut msg) = self.attempt_move_inner(piece_idx, target_loc);
        // A transport that sank, or sailed off with a full deck, leaves its armies in the sea.
        let drowned = self.drown_stranded_armies();
        if drowned > 0 {
            let note = format!(">>> {} of your armies drowned when their transport was lost! <<<", drowned);
            msg = if msg.is_empty() { note } else { format!("{} \n{}", msg, note) };
        }
        (result, msg)
    }

    fn attempt_move_inner(&mut self, piece_idx: usize, target_loc: Loc) -> (MoveResult, String) {
        let ptype = self.pieces[piece_idx].ptype;
        let is_land = self.map[target_loc.r][target_loc.c] == Terrain::Land;
        
        let unit_owner = self.pieces[piece_idx].owner;
        let is_player = unit_owner == Owner::Player;

        let is_friendly_city = self.cities.iter().any(|c| c.loc == target_loc && c.owner == unit_owner);
        let can_enter = match ptype {
            PieceType::Army => is_land || self.pieces.iter().any(|p| p.loc == target_loc && p.owner == unit_owner && p.ptype == PieceType::Transport && p.hits > 0),
            PieceType::Fighter | PieceType::Satellite => true,
            _ => !is_land || is_friendly_city, 
        };

        // True when this move kills the unit whatever happens (ship ashore, army at sea).
        let mover_id = Some(self.pieces[piece_idx].id);
        let suicidal = self.is_suicidal_entry_for(ptype, unit_owner, target_loc, mover_id);

        // Fighters and ships cannot take a city: the attempt destroys them (the original game
        // asks "That's never worked before, sir" first; the UI does the same).
        if self.is_fatal_city_entry(ptype, unit_owner, target_loc) {
            self.pieces[piece_idx].hits = 0;
            if ptype == PieceType::Fighter {
                self.fighter_stats_mut(unit_owner).lost_over_city += 1;
            }
            let msg = if !is_player {
                String::new()
            } else if ptype == PieceType::Fighter {
                format!(">>> Your Fighter was shot down over the city at ({},{})! <<<", target_loc.r, target_loc.c)
            } else {
                format!(">>> Your {:?} broke up on shore at ({},{})! <<<", ptype, target_loc.r, target_loc.c)
            };
            return (MoveResult::CombatLost, msg);
        }

        if !can_enter && !suicidal {
            return (MoveResult::InvalidMove, if is_player { String::from("Terrain impassable for this unit type.") } else { String::new() });
        }

        // An army moving onto a neutral or enemy city is a city assault, decided by a coin flip
        // that ignores whatever is inside (VMS-Empire's attack_city). It never fights the
        // city's occupants.
        if ptype == PieceType::Army {
            if let Some(c_idx) = self.cities.iter().position(|c| c.loc == target_loc && c.owner != unit_owner) {
                return self.assault_city(piece_idx, c_idx);
            }
        }

        let mut enemy_candidates: Vec<usize> = Vec::new();
        let mut boardable_carrier_idx = None;
        let mut blocked_by_friend = false;

        if ptype != PieceType::Satellite {
            for (i, other) in self.pieces.iter().enumerate() {
                if i != piece_idx && other.loc == target_loc && other.hits > 0 {
                    if other.ptype == PieceType::Satellite { continue; }
                    if other.owner == unit_owner {
                        let boards = other.ptype.cargo_type() == Some(ptype) && self.has_room_for(other, mover_id);
                        let can_board_transport = boards && ptype == PieceType::Army;
                        let can_board_carrier = boards && ptype == PieceType::Fighter;
                        let is_fighter_flyover = (ptype == PieceType::Fighter && other.ptype != PieceType::Fighter) || 
                                                 (ptype != PieceType::Fighter && other.ptype == PieceType::Fighter);

                        if can_board_transport || can_board_carrier {
                            boardable_carrier_idx = Some(i);
                        } else if !is_friendly_city && !is_fighter_flyover {
                            blocked_by_friend = true;
                        }
                    } else {
                        enemy_candidates.push(i);
                    }
                }
            }
        }

        let enemy_idx = self.pick_defender(&enemy_candidates, ptype);

        // Always prioritize boarding if an available transport exists, overriding deck collision
        if let Some(carrier_idx) = boardable_carrier_idx {
            let mut boarded_piece = self.pieces[piece_idx].clone();
            boarded_piece.loc = target_loc;
            boarded_piece.moves_left = 0;
            if ptype == PieceType::Fighter {
                let fuel_left = boarded_piece.fuel - 1;
                self.note_landing(unit_owner, fuel_left);
                boarded_piece.fuel = FIGHTER_FUEL;
            }
            self.pieces[carrier_idx].cargo.push(boarded_piece);
            self.pieces[piece_idx].hits = 0;
            
            let msg = if is_player { format!("Your {:?} successfully boarded the {:?}.", ptype, self.pieces[carrier_idx].ptype) } else { String::new() };
            return (MoveResult::Moved, msg);
        }

        if blocked_by_friend {
            // An army ordered onto a transport that is full goes into the water and drowns.
            if ptype == PieceType::Army && !is_land
                && self.pieces.iter().any(|p| p.loc == target_loc && p.owner == unit_owner && p.hits > 0 && p.ptype == PieceType::Transport)
            {
                self.pieces[piece_idx].hits = 0;
                let msg = if is_player { String::from(">>> Your Army jumped into the briny and drowned! <<<") } else { String::new() };
                return (MoveResult::CombatLost, msg);
            }
            return (MoveResult::InvalidMove, if is_player { String::from("Sector is occupied by a friendly unit.") } else { String::new() });
        }

        if let Some(e_idx) = enemy_idx {
            let e_type = self.pieces[e_idx].ptype;
            let hits_before_combat = self.pieces[piece_idx].hits;
            let p_damage = self.pieces[piece_idx].ptype.strike_damage();
            let e_damage = e_type.strike_damage();
            
            // Same model as the original game: each round exactly one side lands a blow,
            // 50/50. (Letting both sides roll every round favoured whoever rolled first.)
            while self.pieces[piece_idx].hits > 0 && self.pieces[e_idx].hits > 0 {
                if self.rng.gen_bool(0.5) {
                    self.pieces[e_idx].hits -= p_damage;
                } else {
                    self.pieces[piece_idx].hits -= e_damage;
                }
            }

            // Damage taken in the fight slows the rest of this turn's movement (the original
            // recomputes a unit's allowance from its current hits every step).
            if self.pieces[piece_idx].hits > 0 {
                let ptype_now = self.pieces[piece_idx].ptype;
                let lost = ptype_now.moves_for_hits(hits_before_combat) - ptype_now.moves_for_hits(self.pieces[piece_idx].hits);
                self.pieces[piece_idx].moves_left = (self.pieces[piece_idx].moves_left - lost).max(0);
            }

            let attacked_player = self.pieces[e_idx].owner == Owner::Player;

            // Fighter statistics: fighters lost in the fight, or aboard a sunk carrier.
            for (who, other_dead) in [(piece_idx, self.pieces[piece_idx].hits <= 0), (e_idx, self.pieces[e_idx].hits <= 0)] {
                if !other_dead {
                    continue;
                }
                let owner = self.pieces[who].owner;
                if self.pieces[who].ptype == PieceType::Fighter {
                    self.fighter_stats_mut(owner).shot_down += 1;
                }
                let aboard = self.pieces[who].cargo.iter().filter(|c| c.ptype == PieceType::Fighter).count() as u32;
                self.fighter_stats_mut(owner).lost_with_carrier += aboard;
            }

            if self.pieces[piece_idx].hits <= 0 {
                if self.pieces[e_idx].owner == Owner::Player { self.player_score += ptype.build_cost() as u32; }
                else if self.pieces[e_idx].owner == Owner::Comp { self.comp_score += ptype.build_cost() as u32; }
            }
            if self.pieces[e_idx].hits <= 0 {
                if unit_owner == Owner::Player { self.player_score += e_type.build_cost() as u32; }
                else if unit_owner == Owner::Comp { self.comp_score += e_type.build_cost() as u32; }
            }

            if self.pieces[piece_idx].hits <= 0 {
                let msg = if is_player { format!(">>> DEFEAT! Your {:?} was DESTROYED by enemy {:?}! <<<", ptype, e_type) } 
                          else if attacked_player { format!(">>> DEFENSE! Your {:?} DESTROYED attacking enemy {:?}! <<<", e_type, ptype) } 
                          else { String::new() };
                return (MoveResult::CombatLost, msg);
            } else {
                // If another enemy still holds the sector (an air unit over a ground unit, or the
                // reverse), the winner stays put: it must beat the rest of the stack before it
                // can occupy the tile or take a city there.
                let others_remain = enemy_candidates.iter().any(|&i| i != e_idx && self.pieces[i].hits > 0);
                let advance = !suicidal && !others_remain;

                if advance {
                    let old_loc = self.pieces[piece_idx].loc;
                    self.pieces[piece_idx].loc = target_loc;
                    self.moved_this_turn.insert(self.pieces[piece_idx].id);
                    
                    // Deck units ride along if a transport or carrier attacks and occupies a tile.
                    self.drag_deck_cargo(piece_idx, old_loc);
                }
                self.pieces[piece_idx].moves_left = (self.pieces[piece_idx].moves_left - 1).max(0);
                
                if is_player {
                    self.reveal_vision(target_loc, ptype.sight_range());
                }

                // A ship that attacks something ashore breaks up, and an army that attacks
                // something at sea drowns, even after a successful assault.
                if suicidal {
                    self.pieces[piece_idx].hits = 0;
                }

                let mut win_msg = if is_player && suicidal {
                    let fate = if ptype == PieceType::Army { "drowned" } else { "broke up on shore" };
                    format!(">>> VICTORY! Your {:?} DESTROYED enemy {:?}, then {} after its assault! <<<", ptype, e_type, fate)
                }
                              else if is_player { format!(">>> VICTORY! Your {:?} DESTROYED enemy {:?}! <<<", ptype, e_type) }
                              else if attacked_player { format!(">>> ALERT! Your {:?} was DESTROYED by enemy {:?}! <<<", e_type, ptype) }
                              else { String::new() };

                if others_remain && is_player {
                    win_msg = format!("{} Another enemy unit still holds the sector.", win_msg);
                }

                if ptype == PieceType::Fighter {
                    self.pieces[piece_idx].fuel -= 1;
                    let end_loc = self.pieces[piece_idx].loc;
                    let at_base = self.cities.iter().any(|c| c.loc == end_loc && c.owner == unit_owner);
                    let at_carrier = self.pieces.iter().any(|p| p.loc == end_loc && p.owner == unit_owner && p.ptype == PieceType::Carrier && p.hits > 0);

                    if at_base || at_carrier {
                        // Landing refuels the fighter and ends its turn, as in the original.
                        let fuel_left = self.pieces[piece_idx].fuel;
                        self.note_landing(unit_owner, fuel_left);
                        self.pieces[piece_idx].fuel = FIGHTER_FUEL;
                        self.pieces[piece_idx].moves_left = 0;
                    } else if self.pieces[piece_idx].fuel <= 0 {
                        self.pieces[piece_idx].hits = 0;
                        self.fighter_stats_mut(unit_owner).out_of_fuel += 1;
                        if is_player {
                            win_msg = format!("{} \n>>> MAYDAY! Your Fighter then ran out of fuel and crashed! <<<", win_msg);
                        }
                    }
                }

                return (MoveResult::CombatWon, win_msg);
            }
        }

        if suicidal {
            // Nothing there to fight: the unit simply goes ashore / into the sea and is lost.
            self.pieces[piece_idx].hits = 0;
            let msg = if !is_player {
                String::new()
            } else if ptype == PieceType::Army {
                String::from(">>> Your Army marched dutifully into the sea and drowned! <<<")
            } else {
                format!(">>> Your {:?} broke up on shore at ({},{})! <<<", ptype, target_loc.r, target_loc.c)
            };
            return (MoveResult::CombatLost, msg);
        }

        let old_loc = self.pieces[piece_idx].loc;
        self.pieces[piece_idx].loc = target_loc;
        self.moved_this_turn.insert(self.pieces[piece_idx].id);
        self.pieces[piece_idx].moves_left -= 1;
        let mut move_msg = String::new();

        // Pull any awake units standing on the deck along to the new tile.
        self.drag_deck_cargo(piece_idx, old_loc);

        if ptype == PieceType::Fighter {
            self.pieces[piece_idx].fuel -= 1;
            let at_base = self.cities.iter().any(|c| c.loc == target_loc && c.owner == unit_owner);
            let at_carrier = self.pieces.iter().any(|p| p.loc == target_loc && p.owner == unit_owner && p.ptype == PieceType::Carrier && p.hits > 0);
            
            if at_base || at_carrier {
                // Landing refuels the fighter and ends its turn, as in the original: no
                // touch-and-go through a city to stretch its range.
                let fuel_left = self.pieces[piece_idx].fuel;
                self.note_landing(unit_owner, fuel_left);
                self.pieces[piece_idx].fuel = FIGHTER_FUEL;
                self.pieces[piece_idx].moves_left = 0;
            } else if self.pieces[piece_idx].fuel <= 0 {
                self.pieces[piece_idx].hits = 0;
                self.fighter_stats_mut(unit_owner).out_of_fuel += 1;
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
                let (target_r, target_c, target_in_map) = match self.step(current_loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                if target_in_map {
                    let is_land = self.map[target_r][target_c] == Terrain::Land;
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || ptype.flies() {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_fatal_city_entry(ptype, Owner::Player, target_loc) && !self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player) {
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
                    let (target_r, target_c, target_in_map) = match self.step(loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                    if target_in_map
                        && !visited[target_r][target_c] {
                            let is_land = self.map[target_r][target_c] == Terrain::Land;
                            let can_enter = match ptype {
                                PieceType::Army => is_land,
                                PieceType::Fighter | PieceType::Satellite => true,
                                _ => !is_land,
                            };

                            // unexplored squares are searched too: they might be passable
                            if can_enter || self.player_map[target_r][target_c] == 0 {
                                visited[target_r][target_c] = true;
                                queue.push_back((Loc { r: target_r, c: target_c }, initial_step));
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

    /// The fighter statistics for `owner` (the neutral side has no fighters; it shares
    /// the player's slot harmlessly).
    pub(crate) fn fighter_stats_mut(&mut self, owner: Owner) -> &mut FighterStats {
        &mut self.fighter_stats[if owner == Owner::Comp { 1 } else { 0 }]
    }

    /// Records a fighter landing with `fuel_left` to spare.
    fn note_landing(&mut self, owner: Owner, fuel_left: i32) {
        let st = self.fighter_stats_mut(owner);
        st.landings += 1;
        if fuel_left <= 2 {
            st.low_fuel_landings += 1;
        }
    }

    /// One turn of work on city `c_idx`'s production. True if a unit is ready, as in the
    /// original's `work++ >= build_time`: the check uses the work done before this turn's
    /// increment, and any surplus carries over to the next unit.
    pub(crate) fn advance_production(&mut self, c_idx: usize) -> bool {
        let city = &mut self.cities[c_idx];
        let cost = city.prod.build_cost();
        let ready = city.work >= cost;
        city.work += 1;
        if ready {
            city.work -= cost;
        }
        ready
    }

    /// Builds city `c_idx`'s current production on its square and returns the new piece's
    /// index. The player's new units take the city's (or the global) standing orders.
    pub(crate) fn produce_unit(&mut self, c_idx: usize) -> usize {
        let id = self.next_free_id();
        self.next_id = id + 1;
        let city = &self.cities[c_idx];
        let ptype = city.prod;
        let mut activity = if city.owner == Owner::Player {
            city.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a)
                .unwrap_or_else(|| self.default_orders.iter().find(|(p, _)| *p == ptype).map(|(_, a)| *a).unwrap_or(UnitActivity::Idle))
        } else {
            UnitActivity::Idle
        };
        if ptype == PieceType::Satellite {
            // A new satellite heads off on a random diagonal (the original's sat_dir).
            activity = SAT_DIRS[self.rng.gen_range(0..SAT_DIRS.len())];
        }
        if ptype == PieceType::Fighter {
            let owner = city.owner;
            self.fighter_stats_mut(owner).built += 1;
        }
        let city = &self.cities[c_idx];
        self.pieces.push(Piece {
            id,
            ptype,
            owner: city.owner,
            loc: city.loc,
            hits: ptype.max_hits(),
            moves_left: ptype.max_moves(),
            activity,
            cargo: Vec::new(),
            fuel: match ptype {
                PieceType::Fighter => FIGHTER_FUEL,
                PieceType::Satellite => SATELLITE_LIFE,
                _ => 0,
            },
        });
        self.pieces.len() - 1
    }

    pub fn process_turn(&mut self) {
        self.turn += 1;
        self.log(format!("Advanced to Turn {}.", self.turn));

        // The player's cities produce now; the computer's produce at the start of its own
        // move (`comp_move`), as in the original. (An AI-run player side produces in its
        // own move too.)
        for idx in 0..self.cities.len() {
            if self.cities[idx].owner != Owner::Player || self.player_ai {
                continue;
            }
            if self.advance_production(idx) {
                let city = &self.cities[idx];
                let msg = format!("City [C{}] ({},{}) finished building {:?}!", idx, city.loc.r, city.loc.c, city.prod);
                self.log(msg);
                self.produce_unit(idx);
            }
        }

        let mut auto_explore_indices = Vec::new();
        let mut auto_attack_indices = Vec::new();
        let mut auto_random_indices = Vec::new();
        let mut auto_dest_indices = Vec::new();
        let mut auto_dir_indices = Vec::new();
        let mut auto_fill_indices = Vec::new();
        let mut auto_upgrade_indices = Vec::new();
        let mut auto_land_indices = Vec::new();
        let mut wakeup_logs = Vec::new();

        let sight = self.player_sight();
        let enemy_locs: Vec<Loc> = self.pieces.iter().enumerate()
            .filter(|(i, p)| p.owner != Owner::Player && p.hits > 0 && self.is_visible_with(*i, Owner::Player, Some(&sight)))
            .map(|(_, p)| p.loc)
            .collect();

        let (torus_flag, rows_n, cols_n) = (self.torus, self.rows, self.cols);
        let player_ai = self.player_ai;
        // End of the previous turn: a damaged boat that sat in its own port without ever
        // moving repairs one hit (original rule; applies to both sides).
        for p in self.pieces.iter_mut() {
            if (p.owner == Owner::Player || p.owner == Owner::Comp)
                && p.hits > 0
                && p.ptype.is_naval()
                && p.hits < p.ptype.max_hits()
                && !self.moved_this_turn.contains(&p.id)
                && self.cities.iter().any(|c| c.loc == p.loc && c.owner == p.owner)
            {
                p.hits += 1;
            }
        }
        self.moved_this_turn.clear();

        for (idx, p) in self.pieces.iter_mut().enumerate() {
            if p.owner == Owner::Player || p.owner == Owner::Comp {
                // Every unit starts a new turn with its full movement allowance (scaled down
                // for damage, as in the original); unspent moves do not carry over.
                p.moves_left = p.ptype.moves_for_hits(p.hits);
            }

            // Satellites take no orders: they fly themselves (`move_satellites`). An
            // AI-run player side gives its own orders.
            if p.owner == Owner::Player && p.ptype != PieceType::Satellite && !player_ai {

                if p.activity == UnitActivity::Sentry {
                    let sight = p.ptype.sight_range();
                    let mut should_wake = false;
                    
                    for &eloc in &enemy_locs {
                        // Same distance as everywhere else in the game (the original dist()).
                        if geo_dist(torus_flag, rows_n, cols_n, eloc, p.loc) <= sight {
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

        // As in the original, satellites move before everything else.
        self.move_satellites();

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

        if self.player_ai {
            self.ai_move(Owner::Player);
        }
        self.comp_move();

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

        // The start of the player's turn: look around with every unit and city.
        self.refresh_player_view();
    }
    pub fn attack_piece(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }

        let mut attack_logs = Vec::new();
        // What the player can see, worked out once for this unit's whole move (checking each
        // enemy against every player unit made attack orders crawl in big games). Anything
        // newly spotted on the way is still found through the remembered view below, which
        // every step updates.
        let sight = self.player_sight();

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            // Targets are enemy units in sight now, plus where the player last saw others
            // (the remembered view), as the original's attack orders work from the user's map.
            let mut enemy_locs: Vec<Loc> = self.pieces.iter().enumerate()
                .filter(|(i, p)| p.owner != Owner::Player && p.hits > 0 && p.ptype != PieceType::Satellite
                    && self.is_visible_with(*i, Owner::Player, Some(&sight)))
                .map(|(_, p)| p.loc)
                .collect();
            for (r, row) in self.player_view.iter().enumerate() {
                for (c, &ch) in row.iter().enumerate() {
                    if ch.is_ascii_lowercase() && ch != b'z' { // satellites can't be attacked
                        enemy_locs.push(Loc { r, c });
                    }
                }
            }

            let p = &self.pieces[piece_idx];
            let current_loc = p.loc;
            let ptype = p.ptype;

            // Only armies can take a city. Fighters and ships that enter a neutral or enemy
            // city are destroyed, so for them cities (and anything sheltering inside one)
            // are not targets; otherwise they would hover beside the city until out of fuel.
            let target_city_locs: Vec<Loc> = if ptype == PieceType::Army {
                self.cities.iter()
                    .filter(|c| matches!(self.known_city_owner(c, Some(&sight)), Some(o) if o != Owner::Player))
                    .map(|c| c.loc)
                    .collect()
            } else {
                Vec::new()
            };

            let mut all_targets: Vec<Loc> = enemy_locs.into_iter()
                .filter(|&l| !self.is_fatal_city_entry(ptype, Owner::Player, l))
                .collect();
            all_targets.extend(target_city_locs);

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
                let (target_r, target_c, target_in_map) = match self.step(current_loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                if target_in_map {
                    let is_land = self.map[target_r][target_c] == Terrain::Land;
                    let can_enter = match p.ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || p.ptype.flies() {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_fatal_city_entry(p.ptype, Owner::Player, target_loc) && !self.is_tile_blocked_by_friend(target_loc, p.ptype, Owner::Player) {
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
                self.explore_piece(piece_idx);
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
                    let (target_r, target_c, target_in_map) = match self.step(loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                    if target_in_map
                        && !visited[target_r][target_c] {
                            let is_land = self.map[target_r][target_c] == Terrain::Land;
                            let can_enter = match p.ptype {
                                PieceType::Army => is_land,
                                PieceType::Fighter | PieceType::Satellite => true,
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

            if !found_target {
                for msg in attack_logs.drain(..) {
                    self.log(msg);
                }
                self.explore_piece(piece_idx);
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

        while self.pieces[piece_idx].moves_left > 0 && self.pieces[piece_idx].hits > 0 {
            let ptype = self.pieces[piece_idx].ptype;
            let current_loc = self.pieces[piece_idx].loc;
            let dirs = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];

            let mut valid_steps = Vec::new();
            for &dir in &dirs {
                let (target_r, target_c, target_in_map) = match self.step(current_loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                if target_in_map {
                    let is_land = self.map[target_r][target_c] == Terrain::Land;
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land,
                    };

                    if can_enter || ptype.flies() {
                        let target_loc = Loc { r: target_r, c: target_c };
                        let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
                        
                        if !friendly_city && !self.is_fatal_city_entry(ptype, Owner::Player, target_loc) && !self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player) {
                            valid_steps.push(target_loc);
                        }
                    }
                }
            }

            if valid_steps.is_empty() {
                self.pieces[piece_idx].moves_left = 0;
                break;
            }

            let chosen_step = valid_steps[self.rng.gen_range(0..valid_steps.len())];

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
                let (r, c, in_map) = match self.step(current_loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                if in_map {
                    let step_loc = Loc { r, c };
                    let is_land = self.map[r][c] == Terrain::Land;
                    let friendly_city = self.cities.iter().any(|ct| ct.loc == step_loc && ct.owner == Owner::Player);
                    
                    let can_enter = match ptype {
                        PieceType::Army => is_land,
                        _ => !is_land || friendly_city,
                    };

                    if (can_enter || ptype.flies())
                        && ((!friendly_city && !self.is_tile_blocked_by_friend(step_loc, ptype, Owner::Player)) || step_loc == target_loc)
                            && !self.is_fatal_city_entry(ptype, Owner::Player, step_loc) {
                            valid_steps.push(step_loc);
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
                    let (r, c, in_map) = match self.step(loc, dir) { Some(l) => (l.r, l.c, true), None => (0, 0, false) };

                    if in_map && !visited[r][c] {
                        let is_land = self.map[r][c] == Terrain::Land;
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

            // (Satellites never come here: they fly themselves, see `move_satellites`.)
            let target_step = self.offset(current_loc, dr, dc);

            if target_step.is_none() {
                dir_logs.push(format!("{:?} reached the edge of the map. Halting for manual assist.", ptype));
                break; // Leave moves_left > 0 so the TUI can pick it up
            }

            let target_loc = target_step.unwrap();
            let is_land = self.map[target_loc.r][target_loc.c] == Terrain::Land;
            let friendly_city = self.cities.iter().any(|ct| ct.loc == target_loc && ct.owner == Owner::Player);
            
            let can_enter = match ptype {
                PieceType::Army => is_land,
                _ => !is_land || friendly_city,
            };

            if (!can_enter && ptype != PieceType::Fighter) || friendly_city || self.is_fatal_city_entry(ptype, Owner::Player, target_loc) || self.is_tile_blocked_by_friend(target_loc, ptype, Owner::Player) {
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
    /// Flies every satellite its full speed along its diagonal, as the original's
    /// `move_sat`: a satellite that would leave the playable map bounces off the edge.
    /// Satellites never fight and never take orders; one without a diagonal heading (from
    /// an older save) is given a random one.
    pub fn move_satellites(&mut self) {
        let (rows, cols) = (self.rows as i64, self.cols as i64);
        let on_board = |r: i64, c: i64| r >= 1 && c >= 1 && r <= rows - 2 && c <= cols - 2;
        for i in 0..self.pieces.len() {
            if self.pieces[i].ptype != PieceType::Satellite || self.pieces[i].hits <= 0 {
                continue;
            }
            if !SAT_DIRS.contains(&self.pieces[i].activity) {
                self.pieces[i].activity = SAT_DIRS[self.rng.gen_range(0..SAT_DIRS.len())];
            }
            for _ in 0..PieceType::Satellite.max_moves() {
                let UnitActivity::Dir(mut dr, mut dc) = self.pieces[i].activity else { break };
                let loc = self.pieces[i].loc;
                let (r, c) = (loc.r as i64, loc.c as i64);
                if !on_board(r + dr as i64, c + dc as i64) {
                    // The original's bounce(): flip east/west if that stays on the map,
                    // else flip north/south if that does, else turn straight back.
                    let (ndr, ndc) = [(dr, -dc), (-dr, dc)].into_iter()
                        .find(|&(a, b)| on_board(r + a as i64, c + b as i64))
                        .unwrap_or((-dr, -dc));
                    dr = ndr;
                    dc = ndc;
                    self.pieces[i].activity = UnitActivity::Dir(dr, dc);
                }
                let (nr, nc) = (r + dr as i64, c + dc as i64);
                if !on_board(nr, nc) {
                    break; // a map too small to fly in
                }
                let new_loc = Loc { r: nr as usize, c: nc as usize };
                self.pieces[i].loc = new_loc;
                self.moved_this_turn.insert(self.pieces[i].id);
                if self.pieces[i].owner == Owner::Player {
                    self.reveal_vision(new_loc, PieceType::Satellite.sight_range());
                }
            }
            self.pieces[i].moves_left = 0;
        }
    }

    pub fn reveal_entire_map(&mut self) {
        for r in 0..self.rows {
            for c in 0..self.cols {
                self.player_map[r][c] = 1;
            }
        }
        self.omniscient = true;
        self.log(String::from("Observer mode active. Map completely revealed."));
    }

    /// The player looks at every square within `range` of `loc`: the squares become
    /// explored, and what is seen there (enemy units, city owners) is remembered.
    pub fn reveal_vision(&mut self, loc: Loc, range: i32) {
        self.scan_for_player(&[(loc, range)]);
    }

    /// Re-scans everything the player can see right now (all units and cities). Called at
    /// the start of each player turn, like the original's scans of every user piece.
    pub fn refresh_player_view(&mut self) {
        let mut centers: Vec<(Loc, i32)> = self.pieces.iter()
            .filter(|p| p.owner == Owner::Player && p.hits > 0)
            .map(|p| (p.loc, p.ptype.sight_range()))
            .collect();
        centers.extend(self.cities.iter().filter(|c| c.owner == Owner::Player).map(|c| (c.loc, Self::CITY_SIGHT)));
        self.scan_for_player(&centers);
    }

    /// The map letter for a unit as the player sees it (upper case: the player's).
    pub fn unit_char(ptype: PieceType, owner: Owner) -> u8 {
        let ch = match ptype {
            PieceType::Army => b'A',
            PieceType::Fighter => b'F',
            PieceType::PatrolBoat => b'P',
            PieceType::Destroyer => b'D',
            PieceType::Submarine => b'S',
            PieceType::Carrier => b'C',
            PieceType::Battleship => b'B',
            PieceType::Transport => b'T',
            PieceType::Satellite => b'Z',
        };
        if owner == Owner::Player { ch } else { ch.to_ascii_lowercase() }
    }

    fn scan_for_player(&mut self, centers: &[(Loc, i32)]) {
        if self.player_view.len() != self.rows || self.player_view.iter().any(|r| r.len() != self.cols) {
            self.player_view = vec![vec![0; self.cols]; self.rows];
        }
        let mut squares = std::collections::HashSet::new();
        for &(loc, range) in centers {
            for dr in -range..=range {
                for dc in -range..=range {
                    if let Some(l) = self.offset(loc, dr, dc) {
                        squares.insert(l);
                    }
                }
            }
        }
        // The enemy unit the player would see on each square (the first one listed, as
        // the display shows). Submarines only show up if detected.
        let mut seen_units: std::collections::HashMap<Loc, u8> = std::collections::HashMap::new();
        for (i, p) in self.pieces.iter().enumerate() {
            if p.owner != Owner::Player && p.hits > 0 && squares.contains(&p.loc)
                && (p.ptype != PieceType::Submarine || self.is_visible_to(i, Owner::Player))
            {
                seen_units.entry(p.loc).or_insert(Self::unit_char(p.ptype, p.owner));
            }
        }
        for &l in &squares {
            self.player_map[l.r][l.c] = 1;
            self.player_view[l.r][l.c] = seen_units.get(&l).copied().unwrap_or(0);
        }
        for city in self.cities.iter_mut() {
            if squares.contains(&city.loc) {
                city.seen_owner = Some(city.owner);
            }
        }
    }

    /// The owner of `city` as the player knows it: the truth if it is the player's or in
    /// sight, otherwise what the player saw last (`None` if never seen).
    pub fn known_city_owner(&self, city: &City, sight: Option<&Vec<Vec<bool>>>) -> Option<Owner> {
        let in_sight = match sight {
            Some(grid) => grid[city.loc.r][city.loc.c],
            None => self.player_sees(city.loc),
        };
        if city.owner == Owner::Player || in_sight || self.omniscient {
            Some(city.owner)
        } else {
            city.seen_owner
        }
    }

    pub fn log(&mut self, msg: String) {
        self.messages.push(msg);
        self.messages_logged += 1;
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
    }

    pub fn process_fill_mode(&mut self, piece_idx: usize) {
        if self.pieces[piece_idx].owner != Owner::Player || self.pieces[piece_idx].hits <= 0 { return; }
        let ptype = self.pieces[piece_idx].ptype;
        let capacity = ptype.capacity_for_hits(self.pieces[piece_idx].hits);
        let cargo_count = capacity - self.free_berths(&self.pieces[piece_idx], None).min(capacity);

        if self.free_berths(&self.pieces[piece_idx], None) == 0 {
            self.pieces[piece_idx].activity = UnitActivity::Idle;
            self.pieces[piece_idx].moves_left = self.pieces[piece_idx].ptype.moves_for_hits(self.pieces[piece_idx].hits);
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
                // Waiting in port: the hit itself is restored at the turn boundary by the
                // general repair rule (a boat that stays put in its own port heals one hit).
                self.pieces[piece_idx].moves_left = 0;
                self.log(format!("{:?} docked at ({},{}) is under repair ({}/{} hits).", ptype, p_loc.r, p_loc.c, p_hits, max_h));
            } else {
                self.pieces[piece_idx].activity = UnitActivity::Idle;
                self.pieces[piece_idx].moves_left = ptype.moves_for_hits(p_hits);
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
        let mut min_dist = self.dist(p_loc, closest_loc);

        for &cloc in &owned_city_locs {
            let dist = self.dist(p_loc, cloc);
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
            if other.owner == Owner::Player && other.ptype == PieceType::Transport
                && self.has_room_for(other, Some(self.pieces[piece_idx].id))
                && self.dist(loc, other.loc) <= 1 {
                    target_transport = Some(i);
                    break;
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

    // ------------------------------------------------------------------------------
    // Orders the player gives. The interface turns keys into these calls; nothing here
    // knows about the screen, so the same calls can serve any front end.
    // ------------------------------------------------------------------------------

    /// Carries out the standing order the unit at `idx` has just been given, straight
    /// away (orders that only take effect at the next turn do nothing here).
    pub fn carry_out_order(&mut self, idx: usize) {
        match self.pieces[idx].activity {
            UnitActivity::Explore => self.explore_piece(idx),
            UnitActivity::Attack => self.attack_piece(idx),
            UnitActivity::Random => self.random_piece(idx),
            UnitActivity::Dest(_) => self.dest_piece(idx),
            UnitActivity::Dir(_, _) => self.dir_piece(idx),
            UnitActivity::Fill => self.process_fill_mode(idx),
            UnitActivity::Upgrade => self.process_upgrade_mode(idx),
            UnitActivity::Land => self.process_land_mode(idx),
            _ => {}
        }
    }

    /// Gives the player's unit at `idx` a standing order and carries it out at once (the
    /// unit-by-unit orders). Sentry and wake have their own calls (`sentry_unit`,
    /// `wake_unit`). Dead pieces are cleared out afterwards, so indices may change.
    pub fn order_unit(&mut self, idx: usize, act: UnitActivity) {
        let ptype = self.pieces[idx].ptype;
        self.pieces[idx].activity = act;
        let msg = match act {
            UnitActivity::Attack => format!("{:?} set to attack mode.", ptype),
            UnitActivity::Explore => format!("{:?} set to grope/explore mode.", ptype),
            UnitActivity::Dest(to) => format!("{:?} ordered to march toward ({},{}).", ptype, to.r, to.c),
            UnitActivity::Dir(dr, dc) => format!("{:?} ordered to march continuously {}.", ptype, dir_name((dr, dc))),
            UnitActivity::TransportWait => String::from("Unit set to Transport Wait."),
            other => format!("Unit set to {:?}.", other),
        };
        self.log(msg);
        self.carry_out_order(idx);
        self.pieces.retain(|p| p.hits > 0);
    }

    /// Ends the unit's move for this turn without orders.
    pub fn skip_unit(&mut self, idx: usize) {
        let p = &mut self.pieces[idx];
        p.moves_left = 0;
        let msg = format!("{:?} at ({},{}) skipped turn.", p.ptype, p.loc.r, p.loc.c);
        self.log(msg);
    }

    /// Puts the unit on sentry duty for the rest of this turn and after. An army on a
    /// transport (or a fighter over a carrier) with room goes below deck into the hold
    /// instead. Dead pieces may be cleared out afterwards, so indices may change.
    pub fn sentry_unit(&mut self, idx: usize) {
        let (ptype, loc, id) = (self.pieces[idx].ptype, self.pieces[idx].loc, self.pieces[idx].id);
        self.pieces[idx].moves_left = 0;
        let ship_with_room = self.pieces.iter().any(|p| {
            p.loc == loc && p.owner == Owner::Player && p.ptype.cargo_type() == Some(ptype) && self.has_room_for(p, Some(id))
        });
        if ship_with_room {
            let (res, msg) = self.attempt_move(idx, loc);
            if res == MoveResult::Moved && msg.contains("boarded") {
                self.log(msg);
                self.pieces.retain(|p| p.hits > 0);
                return;
            }
        }
        self.pieces[idx].activity = UnitActivity::Sentry;
        self.log(format!("{:?}({}:{}) put on sentry duty.", ptype, loc.r, loc.c));
    }

    /// Wakes the unit (clears its standing order); a transport or carrier also brings
    /// its cargo up on deck.
    pub fn wake_unit(&mut self, idx: usize) {
        let ptype = self.pieces[idx].ptype;
        self.pieces[idx].activity = UnitActivity::Idle;
        self.log(format!("{:?} woken up.", ptype));
        if ptype.cargo_type().is_some() {
            self.wake_cargo(idx);
        }
    }

    /// Wakes the player's units at `loc` (of type `only`, or all of them), and brings up
    /// the cargo of any transport or carrier among them. Returns how many were woken.
    pub fn wake_units_at(&mut self, loc: Loc, only: Option<PieceType>) -> usize {
        let mut woke = 0;
        let mut carriers = Vec::new();
        for (i, p) in self.pieces.iter_mut().enumerate() {
            if p.loc == loc && p.owner == Owner::Player && p.hits > 0 && p.ptype != PieceType::Satellite
                && !matches!(only, Some(t) if t != p.ptype)
            {
                if p.activity != UnitActivity::Idle {
                    p.activity = UnitActivity::Idle;
                    woke += 1;
                }
                if !p.cargo.is_empty() {
                    carriers.push(i);
                }
            }
        }
        for i in carriers {
            self.wake_cargo(i);
        }
        woke
    }

    /// Brings up the cargo of every player transport or carrier at `loc`. False if there
    /// was none.
    pub fn launch_cargo_at(&mut self, loc: Loc) -> bool {
        let carriers: Vec<usize> = (0..self.pieces.len())
            .filter(|&i| {
                let p = &self.pieces[i];
                p.loc == loc && p.owner == Owner::Player && p.hits > 0 && !p.cargo.is_empty()
            })
            .collect();
        for &i in &carriers {
            self.wake_cargo(i);
        }
        !carriers.is_empty()
    }

    /// Sets the standing order new units of type `ptype` get (globally, or in one city),
    /// and gives it to units of that type already waiting in the cities concerned, which
    /// carry it out at once. A city's own order overrides the global one, so a global
    /// change leaves cities with their own order for that type alone. Returns how many
    /// units took the new order.
    pub fn set_standing_order(&mut self, scope: OrderScope, ptype: PieceType, act: UnitActivity) -> usize {
        fn set(orders: &mut Vec<(PieceType, UnitActivity)>, ptype: PieceType, act: UnitActivity) {
            match orders.iter_mut().find(|(p, _)| *p == ptype) {
                Some(entry) => entry.1 = act,
                None => orders.push((ptype, act)),
            }
        }
        let city_locs: Vec<Loc> = match scope {
            OrderScope::Global => {
                set(&mut self.default_orders, ptype, act);
                self.cities.iter()
                    .filter(|c| c.owner == Owner::Player && !c.default_orders.iter().any(|(p, _)| *p == ptype))
                    .map(|c| c.loc)
                    .collect()
            }
            OrderScope::City(c) => {
                set(&mut self.cities[c].default_orders, ptype, act);
                vec![self.cities[c].loc]
            }
        };
        let mut updated = Vec::new();
        for (i, p) in self.pieces.iter_mut().enumerate() {
            let waiting = matches!(p.activity, UnitActivity::Idle | UnitActivity::Explore | UnitActivity::Attack
                | UnitActivity::Random | UnitActivity::Dir(_, _) | UnitActivity::Dest(_));
            if city_locs.contains(&p.loc) && p.owner == Owner::Player && p.ptype == ptype && p.hits > 0 && waiting {
                p.activity = act;
                updated.push(i);
            }
        }
        let msg = match scope {
            OrderScope::Global => format!("GLOBAL default order for {:?} set to {:?} (Applied to {} units).", ptype, act, updated.len()),
            OrderScope::City(c) => {
                let l = self.cities[c].loc;
                format!("LOCAL City({},{}) order for {:?} set to {:?} (Applied to {} stacked units).", l.r, l.c, ptype, act, updated.len())
            }
        };
        self.log(msg);
        for &i in &updated {
            self.carry_out_order(i);
        }
        self.pieces.retain(|p| p.hits > 0);
        updated.len()
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
            unit.moves_left = unit.ptype.moves_for_hits(unit.hits);
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
            self.pieces[piece_idx].moves_left = ptype.moves_for_hits(self.pieces[piece_idx].hits);
            if ptype == PieceType::Fighter {
                self.pieces[piece_idx].fuel = FIGHTER_FUEL;
                self.log(format!("Fighter landed at ({},{}) refueled and assigned {:?}.", p_loc.r, p_loc.c, assigned_order));
            } else {
                self.log(format!("{:?} docked at ({},{}) and assigned {:?}.", ptype, p_loc.r, p_loc.c, assigned_order));
            }
            return;
        }

        if ptype == PieceType::Fighter {
            if let Some(carrier_idx) = self.pieces.iter().position(|p| p.owner == Owner::Player && p.ptype == PieceType::Carrier && p.loc == p_loc && self.has_room_for(p, Some(self.pieces[piece_idx].id))) {
                let mut boarded_piece = self.pieces[piece_idx].clone();
                boarded_piece.moves_left = 0;
                boarded_piece.activity = UnitActivity::Sentry; 
                boarded_piece.fuel = FIGHTER_FUEL; // Refuel upon docking
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
            if i != piece_idx && piece.owner == Owner::Player && piece.ptype == PieceType::Carrier && self.has_room_for(piece, Some(self.pieces[piece_idx].id)) {
                valid_targets.push(piece.loc);
            }
        }
        
        if valid_targets.is_empty() {
            self.log(String::from("Fighter has no available bases or carriers to land on!"));
            self.pieces[piece_idx].moves_left = 0;
            return;
        }
        
        let mut closest_loc = valid_targets[0];
        let mut min_dist = self.dist(p_loc, closest_loc);
        
        for &loc in &valid_targets[1..] {
            let dist = self.dist(p_loc, loc);
            if dist < min_dist {
                min_dist = dist;
                closest_loc = loc;
            }
        }
        
        self.pieces[piece_idx].activity = UnitActivity::Dest(closest_loc);
        self.pieces[piece_idx].moves_left = 0;
    }

    /// True when the computer should acknowledge defeat, as in the original's
    /// `check_endgame`: it has fewer than a third as many cities as the player and fewer
    /// than a third as many armies (counting with whole-number division, as the
    /// original does). Asked once: after the computer has resigned, never again.
    pub fn comp_should_resign(&self) -> bool {
        if self.comp_resigned {
            return false;
        }
        let count_cities = |o: Owner| self.cities.iter().filter(|c| c.owner == o).count();
        let count_armies = |o: Owner| {
            fn armies(pieces: &[Piece], o: Owner) -> usize {
                pieces.iter()
                    .map(|p| (p.owner == o && p.hits > 0 && p.ptype == PieceType::Army) as usize + armies(&p.cargo, o))
                    .sum()
            }
            armies(&self.pieces, o)
        };
        count_cities(Owner::Comp) < count_cities(Owner::Player) / 3
            && count_armies(Owner::Comp) < count_armies(Owner::Player) / 3
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

    /// Writes the game to `filename`, reporting any failure to the caller.
    ///
    /// The data goes to `<filename>.tmp` first and is renamed into place, so a crash or a full
    /// disk part-way through never destroys an existing good save.
    pub fn save_game(&self, filename: &str) -> std::io::Result<()> {
        use std::io::Write;
        let data = serde_json::to_string(self).map_err(std::io::Error::other)?;
        let tmp = format!("{}.tmp", filename);
        let result = (|| {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(data.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&tmp, filename)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Loads a save, or says why it can't: unreadable, not a save, from a newer build, or
    /// internally inconsistent. Saves hold only the game, never what the screen was
    /// showing (see `View`); old saves that still carry view fields load fine.
    pub fn load_game(filename: &str) -> Result<Self, String> {
        let data = std::fs::read_to_string(filename)
            .map_err(|e| format!("could not read {}: {}", filename, e))?;
        let mut game: Game = serde_json::from_str(&data)
            .map_err(|e| format!("{} is not a valid save file ({})", filename, e))?;
        if game.save_version > SAVE_VERSION {
            return Err(format!(
                "{} was saved by a newer version of the game (save format {}, this build understands up to {})",
                filename, game.save_version, SAVE_VERSION
            ));
        }
        game.migrate();
        game.validate().map_err(|e| format!("{} is damaged: {}", filename, e))?;
        Ok(game)
    }

    /// Brings a save from an older format up to `SAVE_VERSION`. Version 0 (saves written
    /// before versioning) needs nothing beyond the field defaults.
    fn migrate(&mut self) {
        if self.save_version < 2 {
            // Version 2 added the player's remembered view (fog of war). Start it from what
            // the player can see now, and from the true owners of cities already explored.
            self.player_view = vec![vec![0; self.cols]; self.rows];
            for city in self.cities.iter_mut() {
                if self.player_map.get(city.loc.r).and_then(|r| r.get(city.loc.c)).copied().unwrap_or(0) != 0 {
                    city.seen_owner = Some(city.owner);
                }
            }
            self.refresh_player_view();
        }
        let grids_ok = self.comp_map.len() == self.rows && self.comp_map.iter().all(|r| r.len() == self.cols)
            && self.map.len() == self.rows && self.map.iter().all(|r| r.len() == self.cols);
        if self.save_version < 3 && grids_ok {
            // Version 3 gave the computer its own view of the world (`comp_map`). Older
            // computers saw everything, so start it off knowing the map and every city.
            for r in 0..self.rows {
                for c in 0..self.cols {
                    self.comp_map[r][c] = if self.map[r][c] == Terrain::Land { b'+' } else { b'.' };
                }
            }
            for city in &self.cities {
                self.comp_map[city.loc.r][city.loc.c] = match city.owner {
                    Owner::Neutral => b'*',
                    Owner::Player => b'O',
                    Owner::Comp => b'X',
                };
            }
        }
        self.save_version = SAVE_VERSION;
    }

    /// Cheap consistency checks so a damaged save fails to load instead of panicking later.
    fn validate(&self) -> Result<(), String> {
        let grid_ok = |g: &Vec<Vec<u8>>| g.len() == self.rows && g.iter().all(|r| r.len() == self.cols);
        if self.rows == 0 || self.cols == 0
            || self.map.len() != self.rows || self.map.iter().any(|r| r.len() != self.cols)
            || !grid_ok(&self.comp_map) || !grid_ok(&self.player_map) || !grid_ok(&self.player_view)
        {
            return Err(String::from("map sizes do not match the stated dimensions"));
        }
        let inside = |l: &Loc| l.r < self.rows && l.c < self.cols;
        if self.cities.iter().any(|c| !inside(&c.loc)) {
            return Err(String::from("a city is outside the map"));
        }
        fn pieces_inside(pieces: &[Piece], inside: &dyn Fn(&Loc) -> bool) -> bool {
            pieces.iter().all(|p| inside(&p.loc) && pieces_inside(&p.cargo, inside))
        }
        if !pieces_inside(&self.pieces, &inside) {
            return Err(String::from("a unit is outside the map"));
        }
        Ok(())
    }
}


