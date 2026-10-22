//! The computer player: a port of VMS-Empire's `compmove.c` and the path-finding it uses
//! from `map.c` (Chuck Simmons' "fourth rewrite of the algorithm").
//!
//! The computer plays from its own view of the world (`Game::comp_map`), which, like the
//! original's `comp_map`, only learns what the computer's units and cities can see. Each
//! square holds a map character from the computer's side:
//!
//! - `' '` unexplored, `'+'` land, `'.'` sea
//! - `'*'` an unowned city, `'O'` a player city, `'X'` a computer city
//! - a unit letter: upper case for the player's (`'A'`, `'T'`...), lower case for its own.
//!
//! Each turn the computer looks around, sets city production (`do_cities`), then moves
//! its units in the original order: transports, carriers, battleships, patrol boats,
//! submarines, destroyers, armies, fighters. Every unit heads for the cheapest
//! "objective" on a weighted breadth-first search of the view map.
use crate::game::{geo_dist, geo_offset, retool_work, Game, FIGHTER_FUEL};
use crate::models::*;

const INF: i32 = 10_000_000;

// Path-map terrain kinds (the original's T_* values).
const T_UNKNOWN: u8 = 0;
const T_PATH: u8 = 1;
const T_LAND: u8 = 2;
const T_WATER: u8 = 4;
const T_AIR: u8 = T_LAND | T_WATER;

/// Objective weight meaning "a city building a transport": cost depends on how long the
/// transport will take (W_TT_BUILD).
const W_TT_BUILD: i32 = -1;

const UNEXPLORED: u8 = b' ';
const LAND: u8 = b'+';
const SEA: u8 = b'.';

/// What a unit is looking for: the objective characters and their weights (`move_info_t`).
struct MoveInfo {
    city_owner: Owner,
    objectives: &'static [u8],
    weights: &'static [i32],
}

const TT_EXPLORE: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b" ", weights: &[1] };
const TT_LOAD: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b"$", weights: &[1] };
/// Unloading transports prefer cities on continents with many cities (the digits are set
/// by `make_unload_map`), and will explore if nothing better is near.
const TT_UNLOAD: MoveInfo = MoveInfo {
    city_owner: Owner::Comp,
    objectives: b"9876543210 ",
    weights: &[1, 1, 1, 1, 1, 1, 11, 21, 41, 101, 61],
};
const ARMY_FIGHT: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b"O*TA ", weights: &[1, 1, 1, 1, 11] };
/// '$' a loading transport, 'x' a city building one.
const ARMY_LOAD: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b"$x", weights: &[1, W_TT_BUILD] };
const FIGHTER_FIGHT: MoveInfo = MoveInfo {
    city_owner: Owner::Comp,
    objectives: b"TCFBSDPA ",
    weights: &[1, 1, 5, 5, 5, 5, 5, 5, 9],
};
const SHIP_FIGHT: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b"TCBSDP ", weights: &[1, 1, 3, 3, 3, 3, 21] };
const SHIP_REPAIR: MoveInfo = MoveInfo { city_owner: Owner::Comp, objectives: b"X", weights: &[1] };

/// What each kind of unit attacks when it is next to it, most valuable first.
const TT_ATTACK: &[u8] = b"T";
const ARMY_ATTACK: &[u8] = b"O*TACFBSDP";
const FIGHTER_ATTACK: &[u8] = b"TCFBSDPA";
const SHIP_ATTACK: &[u8] = b"TCBSDP";

/// The piece types in the original game's order (empire.h), which the production tables
/// are indexed by.
const ORDER: [PieceType; 9] = [
    PieceType::Army, PieceType::Fighter, PieceType::PatrolBoat, PieceType::Destroyer,
    PieceType::Submarine, PieceType::Transport, PieceType::Carrier, PieceType::Battleship,
    PieceType::Satellite,
];
const ARMY: usize = 0;
const FIGHTER: usize = 1;
const TRANSPORT: usize = 5;
const SATELLITE: usize = 8;

fn oidx(p: PieceType) -> usize {
    ORDER.iter().position(|&t| t == p).unwrap()
}

/// How many cities we want producing each type (A F P D S T C B Z): early on armies and
/// transports for rapid expansion, later ships of all kinds.
const RATIO1: [i64; 9] = [60, 0, 10, 0, 0, 20, 0, 0, 0];
const RATIO2: [i64; 9] = [90, 10, 10, 10, 10, 40, 0, 0, 0];
const RATIO3: [i64; 9] = [120, 20, 20, 10, 10, 60, 10, 10, 0];
const RATIO4: [i64; 9] = [150, 30, 30, 20, 20, 70, 10, 10, 0];

/// The eight directions in the original's `dir_offset` order: N, NE, E, SE, S, SW, W, NW.
const DIRS: [(i32, i32); 8] = [(-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1)];
/// The order `vmap_find_dir` tries them in: diagonals first.
const FIND_DIR_ORDER: [usize; 8] = [7, 1, 5, 3, 6, 2, 0, 4];

/// The map character for a unit (upper case: the player's).
pub fn unit_char(ptype: PieceType, owner: Owner) -> u8 {
    Game::unit_char(ptype, owner)
}

/// Things counted on a continent (`scan_counts_t`, only the fields the AI uses).
#[derive(Default, Debug)]
struct Counts {
    user_cities: i32,
    user_armies: i32,
    comp_cities: i32,
    unowned_cities: i32,
    unexplored: i32,
}

#[derive(Clone, Copy)]
struct Cell {
    cost: i32,
    inc_cost: i32,
    terrain: u8,
}

/// A path map plus the best objective found while building it.
struct PathMap {
    cells: Vec<Cell>,
    best_cost: i32,
    best_loc: usize,
}

/// Everything the computer works from during one move.
struct Ctx {
    rows: usize,
    cols: usize,
    torus: bool,
    /// The real map: '+', '.', or '*' for any city.
    rmap: Vec<u8>,
    /// The view map with unexplored squares predicted as land or sea where that is a safe
    /// bet (`emap`, from `vmap_prune_explore_locs`).
    emap: Vec<u8>,
    /// The transport unload map, built on first use each move.
    unload_map: Option<Vec<u8>>,
    /// Index of the city on each square, if any.
    city_at: Vec<Option<usize>>,
    /// Each square's on-board neighbours in `DIRS` order (`NONE` where there is none).
    nbr: Vec<[usize; 8]>,
}

const NONE: usize = usize::MAX;

fn neighbour_table(rows: usize, cols: usize, torus: bool) -> Vec<[usize; 8]> {
    let on_board = |l: Loc| l.r >= 1 && l.c >= 1 && l.r + 1 < rows && l.c + 1 < cols;
    (0..rows * cols)
        .map(|i| {
            let loc = Loc { r: i / cols, c: i % cols };
            let mut n = [NONE; 8];
            for (d, &(dr, dc)) in DIRS.iter().enumerate() {
                if let Some(l) = geo_offset(torus, rows, cols, loc, dr, dc) {
                    if on_board(l) {
                        n[d] = l.r * cols + l.c;
                    }
                }
            }
            n
        })
        .collect()
}

impl Ctx {
    fn on_board(&self, i: usize) -> bool {
        let (r, c) = (i / self.cols, i % self.cols);
        r >= 1 && c >= 1 && r + 1 < self.rows && c + 1 < self.cols
    }

    fn loc(&self, i: usize) -> Loc {
        Loc { r: i / self.cols, c: i % self.cols }
    }

    fn idx(&self, l: Loc) -> usize {
        l.r * self.cols + l.c
    }

    /// The neighbour of `i` in direction `d` (an index into `DIRS`), if on the board.
    fn adj(&self, i: usize, d: usize) -> Option<usize> {
        let n = self.nbr[i][d];
        if n == NONE { None } else { Some(n) }
    }

    /// The on-board neighbours of `i` (`FOR_ADJ_ON`).
    fn adj_on(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        self.nbr[i].iter().copied().filter(|&n| n != NONE)
    }

    fn dist(&self, a: usize, b: usize) -> i32 {
        geo_dist(self.torus, self.rows, self.cols, self.loc(a), self.loc(b))
    }
}

impl Game {
    // ------------------------------------------------------------------------------
    // The computer's view of the world
    // ------------------------------------------------------------------------------

    /// What the computer's view map shows at `l` (0 in an old save counts as unexplored).
    fn cview(&self, l: Loc) -> u8 {
        match self.comp_map[l.r][l.c] {
            0 => UNEXPLORED,
            ch => ch,
        }
    }

    /// The computer looks at every square within `range` of `loc`.
    #[allow(dead_code)] // used by the tests
    pub fn comp_scan(&mut self, loc: Loc, range: i32) {
        self.comp_scan_many(&[(loc, range)]);
    }

    /// The computer looks at the squares around each (centre, range) (the original's
    /// `scan`/`update`): a city shows as its owner's city character; otherwise the most
    /// important unit there (players' submarines only if detected); otherwise the terrain.
    fn comp_scan_many(&mut self, centers: &[(Loc, i32)]) {
        // In view: 0 = no, 1 = yes; then the best (rank + 1, char) of a unit seen there.
        let cols = self.cols;
        let mut seen = vec![false; self.rows * cols];
        let mut cells = Vec::new();
        for &(loc, range) in centers {
            for dr in -range..=range {
                for dc in -range..=range {
                    if let Some(l) = self.offset(loc, dr, dc) {
                        if !seen[l.r * cols + l.c] {
                            seen[l.r * cols + l.c] = true;
                            cells.push(l);
                        }
                    }
                }
            }
        }
        // find_obj_at_loc: the highest type wins; a satellite only if alone.
        let mut top: Vec<(u8, u8)> = vec![(0, 0); self.rows * cols];
        for (i, p) in self.pieces.iter().enumerate() {
            let k = p.loc.r * cols + p.loc.c;
            if p.hits <= 0 || !seen[k] {
                continue;
            }
            if p.owner != Owner::Comp && p.ptype == PieceType::Submarine && !self.is_visible_to(i, Owner::Comp) {
                continue;
            }
            let rank = if p.ptype == PieceType::Satellite { 1 } else { 2 + oidx(p.ptype) as u8 };
            if rank > top[k].0 {
                top[k] = (rank, unit_char(p.ptype, p.owner));
            }
        }
        for l in cells {
            let k = l.r * cols + l.c;
            self.comp_map[l.r][l.c] = match top[k] {
                (0, _) if self.map[l.r][l.c] == Terrain::Land => LAND,
                (0, _) => SEA,
                (_, ch) => ch,
            };
        }
        for city in &self.cities {
            if seen[city.loc.r * cols + city.loc.c] {
                self.comp_map[city.loc.r][city.loc.c] = match city.owner {
                    Owner::Neutral => b'*',
                    Owner::Player => b'O',
                    Owner::Comp => b'X',
                };
            }
        }
    }

    /// Refreshes the computer's view around all its units and cities.
    pub fn comp_scan_all(&mut self) {
        let centers: Vec<(Loc, i32)> = self.pieces.iter()
            .filter(|p| p.owner == Owner::Comp && p.hits > 0)
            .map(|p| (p.loc, p.ptype.sight_range()))
            .chain(self.cities.iter().filter(|c| c.owner == Owner::Comp).map(|c| (c.loc, Game::CITY_SIGHT)))
            .collect();
        self.comp_scan_many(&centers);
    }

    /// The computer's view as a flat array (row-major), for path-finding.
    fn comp_view_flat(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.rows * self.cols);
        for r in 0..self.rows {
            for c in 0..self.cols {
                v.push(self.cview(Loc { r, c }));
            }
        }
        v
    }

    fn make_ctx(&self) -> Ctx {
        let mut rmap = vec![SEA; self.rows * self.cols];
        for r in 0..self.rows {
            for c in 0..self.cols {
                if self.map[r][c] == Terrain::Land {
                    rmap[r * self.cols + c] = LAND;
                }
            }
        }
        for city in &self.cities {
            rmap[city.loc.r * self.cols + city.loc.c] = b'*';
        }
        let mut ctx = Ctx { rows: self.rows, cols: self.cols, torus: self.torus, rmap, emap: Vec::new(), unload_map: None, city_at: self.city_grid(), nbr: neighbour_table(self.rows, self.cols, self.torus) };
        let mut emap = self.comp_view_flat();
        prune_explore_locs(&ctx, &mut emap);
        ctx.emap = emap;
        ctx
    }

    // ------------------------------------------------------------------------------
    // The computer's move
    // ------------------------------------------------------------------------------

    /// The computer's whole move (`comp_move`): look around, set production, move units.
    pub fn comp_move(&mut self) {
        // Armies riding in a transport's hold at sea come up on deck, so they can act
        // (attack, or walk ashore) like the original's embarked armies.
        for t in 0..self.pieces.len() {
            let p = &self.pieces[t];
            if p.owner == Owner::Comp && p.hits > 0 && p.ptype == PieceType::Transport
                && !p.cargo.is_empty() && !self.cities.iter().any(|c| c.loc == p.loc)
            {
                let loc = p.loc;
                let mut cargo = std::mem::take(&mut self.pieces[t].cargo);
                for a in cargo.iter_mut() {
                    a.loc = loc;
                    a.moves_left = a.ptype.moves_for_hits(a.hits);
                }
                self.pieces.extend(cargo);
            }
        }

        self.comp_scan_all();
        let mut ctx = self.make_ctx();
        self.do_cities(&ctx);
        // Production may have changed what is building; the unload map is built lazily.
        ctx.unload_map = None;

        const MOVE_ORDER: [PieceType; 8] = [
            PieceType::Transport, PieceType::Carrier, PieceType::Battleship, PieceType::PatrolBoat,
            PieceType::Submarine, PieceType::Destroyer, PieceType::Army, PieceType::Fighter,
        ];
        for ptype in MOVE_ORDER {
            let ids: Vec<usize> = self.pieces.iter()
                .filter(|p| p.owner == Owner::Comp && p.hits > 0 && p.ptype == ptype)
                .map(|p| p.id)
                .collect();
            for id in ids {
                if let Some(i) = self.pieces.iter().position(|p| p.id == id && p.hits > 0) {
                    self.cpiece_move(&mut ctx, i);
                }
            }
        }
        self.pieces.retain(|p| p.hits > 0);
    }

    /// City production (`do_cities`): choose production for newly won cities, build what
    /// is ready (then reconsider production), and move ship building off lakes.
    fn do_cities(&mut self, ctx: &Ctx) {
        for c in self.comp_new_cities.clone() {
            if self.cities.get(c).map_or(false, |city| city.owner == Owner::Comp) {
                let is_lake = self.lake(ctx, c);
                self.comp_prod(ctx, c, is_lake, true);
            }
            self.comp_new_cities.retain(|&x| x != c);
        }
        for c in 0..self.cities.len() {
            if self.cities[c].owner != Owner::Comp {
                continue;
            }
            let is_lake = self.lake(ctx, c);
            if self.advance_production(c) {
                self.produce_unit(c);
                self.comp_prod(ctx, c, is_lake, false);
            } else {
                let p = oidx(self.cities[c].prod);
                if p > FIGHTER && p != SATELLITE && is_lake {
                    // don't produce ships in lakes
                    self.comp_prod(ctx, c, is_lake, false);
                }
            }
        }
    }

    /// A city is on a lake if its body of water touches no attackable city and no
    /// unexplored territory (judged on the pruned view map).
    fn lake(&self, ctx: &Ctx, c: usize) -> bool {
        let start = ctx.idx(self.cities[c].loc);
        let cont = vmap_cont(ctx, &ctx.emap, start, LAND);
        let counts = cont_scan(ctx, &cont, &ctx.emap, self);
        !(counts.unowned_cities > 0 || counts.user_cities > 0 || counts.unexplored > 0)
    }

    /// Sets production for a computer city (`comp_prod`):
    /// 1. have enough army producers on each continent to deal with any threat there,
    /// 2. always have a transport producer,
    /// 3. otherwise keep a good ratio of producers of each type.
    /// `new_city` means the city has no production yet (just won).
    fn comp_prod(&mut self, ctx: &Ctx, c: usize, is_lake: bool, new_city: bool) {
        let ctx_view = self.comp_view_flat();
        let geo = ctx;
        let start = geo.idx(self.cities[c].loc);
        let cont = vmap_cont(&geo, &ctx_view, start, SEA);
        let counts = cont_scan(&geo, &cont, &ctx_view, self);

        // army producers on this continent
        let mut comp_ac = 0;
        for (i, &on) in cont.iter().enumerate() {
            if on && ctx_view[i] == b'X' {
                let l = geo.loc(i);
                if let Some(city) = self.cities.iter().position(|ct| ct.loc == l) {
                    if self.cities[city].owner == Owner::Comp && self.cities[city].prod == PieceType::Army
                        && !(city == c && new_city) && !self.comp_new_cities.contains(&city)
                    {
                        comp_ac += 1;
                    }
                }
            }
        }
        let mut interest = counts.unexplored > 0 || counts.user_cities > 0 || counts.user_armies > 0 || counts.unowned_cities > 0;

        // one more army producer than the enemy has cities, and one more if anything of
        // interest is on the continent
        let mut need_count = counts.user_cities - comp_ac + interest as i32;
        if counts.user_cities > 0 {
            need_count += 1;
        }
        if need_count > 0 {
            self.comp_set_prod(c, PieceType::Army, new_city);
            return;
        }
        // Produce armies in new cities if there is a city to attack.
        if counts.user_cities > 0 && new_city {
            self.comp_set_prod(c, PieceType::Army, new_city);
            return;
        }

        // number of cities producing each piece
        let mut city_count = [0i64; 9];
        let mut total_cities = 0;
        for (i, city) in self.cities.iter().enumerate() {
            if city.owner == Owner::Comp && !(i == c && new_city) && !self.comp_new_cities.contains(&i) {
                city_count[oidx(city.prod)] += 1;
                total_cities += 1;
            }
        }
        let ratio = if total_cities <= 10 { &RATIO1 } else if total_cities <= 20 { &RATIO2 } else if total_cities <= 30 { &RATIO3 } else { &RATIO4 };
        let prod = if new_city { None } else { Some(oidx(self.cities[c].prod)) };

        // if we have one army producer, and this is it, keep it
        if city_count[ARMY] == 1 && prod == Some(ARMY) {
            return;
        }
        // first available non-lake city becomes a transport producer
        if city_count[TRANSPORT] == 0 {
            if !is_lake {
                self.comp_set_prod(c, PieceType::Transport, new_city);
                return;
            }
            // if our one army producer is not on a lake, produce armies here instead
            if city_count[ARMY] == 1 {
                if let Some(a) = (0..self.cities.len()).find(|&i| {
                    self.cities[i].owner == Owner::Comp && self.cities[i].prod == PieceType::Army && !(i == c && new_city)
                }) {
                    if !self.lake(ctx, a) {
                        self.comp_set_prod(c, PieceType::Army, new_city);
                        return;
                    }
                }
            }
        }
        // don't change production from armies if something is on the continent
        if prod == Some(ARMY) && interest {
            return;
        }
        // Produce armies in new cities if there is a city to attack.
        if counts.unowned_cities > 0 && new_city {
            self.comp_set_prod(c, PieceType::Army, new_city);
            return;
        }

        // Set production to the item most needed. Continents with one city and nothing
        // interesting may not produce armies.
        interest = counts.comp_cities != 1 || interest;
        let change = match prod {
            None => true,
            Some(p) => {
                (p == ARMY && counts.comp_cities == 1) || overproduced(p, &city_count, ratio) || (p > FIGHTER && is_lake)
            }
        };
        if change {
            let best = comp_set_needed(&mut city_count, ratio, interest, is_lake);
            self.comp_set_prod(c, ORDER[best], new_city);
        }
    }

    /// Changes a computer city's production, paying the retooling cost; a no-op if it is
    /// already building that (unless the city is new and has no production yet).
    fn comp_set_prod(&mut self, c: usize, ptype: PieceType, new_city: bool) {
        if !new_city && self.cities[c].prod == ptype {
            return;
        }
        self.cities[c].prod = ptype;
        self.cities[c].work = retool_work(ptype);
    }

    fn city_grid(&self) -> Vec<Option<usize>> {
        let mut g = vec![None; self.rows * self.cols];
        for (i, c) in self.cities.iter().enumerate() {
            g[c.loc.r * self.cols + c.loc.c] = Some(i);
        }
        g
    }

    // ------------------------------------------------------------------------------
    // Moving units
    // ------------------------------------------------------------------------------

    /// Moves one computer unit until its moves are used up (`cpiece_move`).
    fn cpiece_move(&mut self, ctx: &mut Ctx, i: usize) {
        let ptype = self.pieces[i].ptype;
        if ptype == PieceType::Satellite {
            return; // satellites fly themselves
        }
        self.pieces[i].moves_left = self.pieces[i].ptype.moves_for_hits(self.pieces[i].hits);
        if ptype == PieceType::Fighter && self.own_city_at(self.pieces[i].loc, Owner::Comp) {
            self.pieces[i].fuel = FIGHTER_FUEL;
        }
        let mut guard = 0;
        while self.pieces[i].hits > 0 && self.pieces[i].moves_left > 0 {
            let before = (self.pieces[i].loc, self.pieces[i].moves_left);
            match ptype {
                PieceType::Army => self.army_move(ctx, i),
                PieceType::Transport => self.transport_move(ctx, i),
                PieceType::Fighter => self.fighter_move(ctx, i),
                _ => self.ship_move(ctx, i),
            }
            if self.pieces[i].hits > 0 && (self.pieces[i].loc, self.pieces[i].moves_left) == before {
                // nothing happened (a move that was refused): don't loop forever
                self.pieces[i].moves_left = 0;
            }
            if ptype == PieceType::Fighter && self.pieces[i].hits > 0 {
                if self.own_city_at(self.pieces[i].loc, Owner::Comp) {
                    self.pieces[i].moves_left = 0; // landed
                } else if self.pieces[i].fuel <= 0 {
                    self.pieces[i].hits = 0; // crash & burn
                }
            }
            guard += 1;
            if guard > 64 {
                self.pieces[i].moves_left = 0;
            }
        }
    }

    fn own_city_at(&self, l: Loc, owner: Owner) -> bool {
        self.cities.iter().any(|c| c.loc == l && c.owner == owner)
    }

    /// Moves or attacks into `to`, then lets the computer see the result.
    fn comp_step(&mut self, i: usize, to: usize, ctx: &Ctx) {
        let to_loc = ctx.loc(to);
        let from = self.pieces[i].loc;
        let (result, _) = self.attempt_move(i, to_loc);
        if result == MoveResult::InvalidMove {
            self.pieces[i].moves_left = 0;
        }
        let mut centers = vec![(to_loc, 1), (from, 1)];
        if self.pieces[i].hits > 0 {
            centers.push((self.pieces[i].loc, self.pieces[i].ptype.sight_range()));
        }
        self.comp_scan_many(&centers);
    }

    /// The transport (by index) an army is riding on: at sea on its square.
    fn army_ship(&self, i: usize) -> Option<usize> {
        let l = self.pieces[i].loc;
        if self.map[l.r][l.c] != Terrain::Sea {
            return None;
        }
        self.pieces.iter().position(|t| t.loc == l && t.owner == Owner::Comp && t.hits > 0 && t.ptype == PieceType::Transport)
    }

    fn tt_loading(&self, t: usize) -> bool {
        self.pieces[t].activity != UnitActivity::Attack
    }

    fn set_tt_loading(&mut self, t: usize, loading: bool) {
        self.pieces[t].activity = if loading { UnitActivity::Fill } else { UnitActivity::Attack };
    }

    /// An army that wants a transport (the original's army `func == 1`).
    fn army_wants_tt(&self, i: usize) -> bool {
        self.pieces[i].activity == UnitActivity::TransportWait
    }

    fn set_army_wants_tt(&mut self, i: usize, wants: bool) {
        self.pieces[i].activity = if wants { UnitActivity::TransportWait } else { UnitActivity::Idle };
    }

    /// Number of armies aboard transport `t`.
    fn tt_count(&self, t: usize) -> usize {
        let p = &self.pieces[t];
        p.ptype.capacity_for_hits(p.hits) - self.free_berths(p, None).min(p.ptype.capacity_for_hits(p.hits))
    }

    /// Moves an army (`army_move`):
    /// 1. attack anything next to it;
    /// 2. on a transport: wait while it loads, else head ashore toward the best target;
    /// 3. otherwise find the nearest land objective, and compare it with the nearest
    ///    loading transport (or city building one); head for whichever is better.
    fn army_move(&mut self, ctx: &mut Ctx, i: usize) {
        let vmap = self.comp_view_flat();
        let loc = ctx.idx(self.pieces[i].loc);
        self.set_army_wants_tt(i, false);
        let ship = self.army_ship(i);

        if at_sea(ctx, &vmap, loc) {
            // nowhere to go but the transport it is on
            self.load_army(ctx, i);
            if self.pieces[i].hits > 0 {
                self.pieces[i].moves_left = 0;
                if self.army_ship(i).is_none() && self.pieces[i].hits > 0 {
                    self.set_army_wants_tt(i, true);
                }
            }
            return;
        }
        let new_loc = find_attack(ctx, &vmap, loc, ARMY_ATTACK, if ship.is_some() { b"+*" } else { b".+*" });
        if new_loc != loc {
            self.comp_step(i, new_loc, ctx);
            return;
        }
        if let Some(t) = ship {
            if self.tt_loading(t) {
                self.load_army(ctx, i);
                if self.pieces[i].hits > 0 {
                    self.pieces[i].moves_left = 0; // armies stay on a loading ship
                }
                return;
            }
            let amap = self.unload_map(ctx);
            let mut pm = find_wlobj(ctx, &amap, loc, &TT_UNLOAD, self);
            let dest = pm.best_loc;
            self.move_objective(ctx, i, &mut pm, dest, b" ");
            return;
        }

        let mut pm = find_xobj(ctx, &vmap, loc, &ARMY_FIGHT, T_LAND, T_LAND, self);
        let new_loc = pm.best_loc;
        let cross_cost = if new_loc != loc {
            let c = match vmap[new_loc] {
                b'A' | b'O' => 60, // high cost if enemy present
                b' ' => 14,        // low cost for exploring
                _ => 30,           // medium cost for an attackable city
            };
            pm.cells[new_loc].cost * 2 - c
        } else {
            INF
        };
        if new_loc == loc || cross_cost > 0 {
            // see if there is something interesting to load
            let amap = self.make_army_load_map(ctx, i, &vmap);
            let mut pm2 = find_lwobj(ctx, &amap, loc, &ARMY_LOAD, cross_cost, self);
            let new_loc2 = pm2.best_loc;
            if new_loc2 != loc {
                // board_ship
                if !self.load_army(ctx, i) {
                    self.set_army_wants_tt(i, true);
                    self.move_objective(ctx, i, &mut pm2, new_loc2, b"t.");
                }
                return;
            }
        }
        self.move_objective(ctx, i, &mut pm, new_loc, b" ");
    }

    /// Puts an army on the fullest non-full transport here or next door (`load_army`).
    /// False if there is none.
    fn load_army(&mut self, ctx: &Ctx, i: usize) -> bool {
        let loc = ctx.idx(self.pieces[i].loc);
        let id = self.pieces[i].id;
        let mut best: Option<(usize, usize)> = None; // (transport index, count)
        let consider = |game: &Game, at: usize, best: &mut Option<(usize, usize)>| {
            let l = ctx.loc(at);
            for (t, p) in game.pieces.iter().enumerate() {
                if p.loc == l && p.owner == Owner::Comp && p.hits > 0 && p.ptype == PieceType::Transport
                    && game.has_room_for(p, Some(id))
                {
                    let count = game.tt_count(t);
                    if best.map_or(true, |(_, bc)| count >= bc) {
                        *best = Some((t, count));
                    }
                }
            }
        };
        consider(self, loc, &mut best);
        for n in ctx.adj_on(loc).collect::<Vec<_>>() {
            consider(self, n, &mut best);
        }
        let Some((t, _)) = best else { return false };
        let t_loc = self.pieces[t].loc;
        if t_loc == self.pieces[i].loc {
            if self.army_ship(i) != Some(t) {
                // in port with the transport: go aboard
                let _ = self.attempt_move(i, t_loc);
            }
            if self.pieces[i].hits > 0 {
                self.pieces[i].moves_left = 0;
            }
        } else {
            self.comp_step(i, ctx.idx(t_loc), ctx);
        }
        true
    }

    /// The view map with loading transports marked '$' and cities building transports
    /// that this army could use marked 'x' (`make_army_load_map`).
    fn make_army_load_map(&self, geo: &Ctx, i: usize, vmap: &[u8]) -> Vec<u8> {
        let mut xmap = vmap.to_vec();
        for (t, p) in self.pieces.iter().enumerate() {
            if p.owner == Owner::Comp && p.hits > 0 && p.ptype == PieceType::Transport && self.tt_loading(t) {
                xmap[geo.idx(p.loc)] = b'$';
            }
        }
        for city in &self.cities {
            if city.owner == Owner::Comp && city.prod == PieceType::Transport {
                if self.nearby_load(i, city.loc) || self.nearby_count(city.loc) < PieceType::Transport.capacity_for_hits(1) {
                    xmap[geo.idx(city.loc)] = b'x';
                }
            }
        }
        xmap
    }

    fn nearby_load(&self, i: usize, l: Loc) -> bool {
        self.army_wants_tt(i) && self.dist(self.pieces[i].loc, l) <= 2
    }

    fn nearby_count(&self, l: Loc) -> usize {
        (0..self.pieces.len())
            .filter(|&i| {
                let p = &self.pieces[i];
                p.owner == Owner::Comp && p.hits > 0 && p.ptype == PieceType::Army && self.nearby_load(i, l)
            })
            .count()
    }

    /// The view map with each attackable city given a digit for how good a target it is
    /// for an unloading transport (`make_unload_map`): cities on continents with many
    /// cities score high if the continent is contested or mostly unowned; a continent's
    /// sole enemy city scores 2; anything else (likely to fall to armies already there)
    /// scores 0. Unexplored squares are filled in with the pruned predictions.
    fn unload_map(&mut self, ctx: &mut Ctx) -> Vec<u8> {
        if ctx.unload_map.is_none() {
            let vmap = self.comp_view_flat();
            let mut xmap = vmap.clone();
            unmark_explore_locs(ctx, &mut xmap);
            let base = xmap.clone();
            let mut done: std::collections::HashMap<usize, u8> = std::collections::HashMap::new();
            for i in 0..vmap.len() {
                if vmap[i] == b'O' || vmap[i] == b'*' {
                    let cont = vmap_cont(ctx, &base, i, SEA);
                    // cities on the same continent get the same digit
                    let key = cont.iter().position(|&b| b).unwrap_or(i);
                    let digit = *done.entry(key).or_insert_with(|| {
                        let counts = cont_scan(ctx, &cont, &base, self);
                        let mut total = counts.unowned_cities + counts.user_cities + counts.comp_cities;
                        if total > 9 {
                            total = 0;
                        }
                        if (counts.user_cities > 0 && counts.comp_cities > 0)
                            || (counts.unowned_cities > counts.user_cities && counts.comp_cities == 0)
                        {
                            b'0' + total as u8
                        } else if counts.user_cities == 1 && counts.comp_cities == 0 {
                            b'2'
                        } else {
                            b'0'
                        }
                    });
                    xmap[i] = digit;
                }
            }
            ctx.unload_map = Some(xmap);
        }
        ctx.unload_map.clone().unwrap()
    }

    /// Moves a transport (`transport_move`). Empty transports load: they head for armies
    /// waiting for a ride (or explore), and attack enemy transports next to them. Full
    /// transports unload: they head for the best city to drop their armies at.
    fn transport_move(&mut self, ctx: &mut Ctx, i: usize) {
        let vmap = self.comp_view_flat();
        let loc = ctx.idx(self.pieces[i].loc);
        let count = self.tt_count(i);
        if count == 0 {
            self.set_tt_loading(i, true);
            let new_loc = find_attack(ctx, &vmap, loc, TT_ATTACK, b".");
            if new_loc != loc {
                self.comp_step(i, new_loc, ctx);
                return;
            }
        }
        if count == self.pieces[i].ptype.capacity_for_hits(self.pieces[i].hits) {
            self.set_tt_loading(i, false);
        }
        if self.tt_loading(i) {
            let mut amap = vmap.clone();
            for p in self.pieces.iter().enumerate() {
                let (a, p) = p;
                if p.owner == Owner::Comp && p.hits > 0 && p.ptype == PieceType::Army && self.army_wants_tt(a) {
                    amap[ctx.idx(p.loc)] = b'$';
                }
            }
            let mut pm = find_wlobj(ctx, &amap, loc, &TT_LOAD, self);
            if pm.best_loc == loc {
                // nothing to load: explore
                let mut amap = vmap.clone();
                unmark_explore_locs(ctx, &mut amap);
                pm = find_xobj(ctx, &amap, loc, &TT_EXPLORE, T_WATER, T_WATER, self);
            }
            let dest = pm.best_loc;
            self.move_objective(ctx, i, &mut pm, dest, b"a ");
        } else {
            let amap = self.unload_map(ctx);
            let mut pm = find_wlobj(ctx, &amap, loc, &TT_UNLOAD, self);
            let dest = pm.best_loc;
            self.move_objective(ctx, i, &mut pm, dest, b" ");
        }
    }

    /// Moves a fighter (`fighter_move`): attack anything next to it; head home if low on
    /// fuel; otherwise look for something to attack or explore.
    fn fighter_move(&mut self, ctx: &mut Ctx, i: usize) {
        let vmap = self.comp_view_flat();
        let loc = ctx.idx(self.pieces[i].loc);
        let new_loc = find_attack(ctx, &vmap, loc, FIGHTER_ATTACK, b".+");
        if new_loc != loc {
            self.comp_step(i, new_loc, ctx);
            return;
        }
        // return to base if low on fuel
        let (dist, city) = self.cities.iter()
            .filter(|c| c.owner == Owner::Comp)
            .map(|c| (ctx.dist(loc, ctx.idx(c.loc)), ctx.idx(c.loc)))
            .min_by_key(|&(d, _)| d)
            .unwrap_or((INF, loc));
        let mut pm;
        let mut dest = loc;
        if self.pieces[i].fuel <= dist + 2 && city != loc {
            pm = find_dest(ctx, &vmap, loc, city, Owner::Comp, T_AIR, self);
            dest = pm.best_loc;
        } else {
            pm = new_pathmap(ctx, loc, T_LAND);
        }
        if dest == loc {
            pm = find_xobj(ctx, &vmap, loc, &FIGHTER_FIGHT, T_LAND, T_AIR, self);
            dest = pm.best_loc;
        }
        self.move_objective(ctx, i, &mut pm, dest, b" ");
    }

    /// Moves a ship (`ship_move`): a damaged ship goes to port to be repaired; otherwise
    /// it attacks anything next to it, or hunts or explores.
    fn ship_move(&mut self, ctx: &mut Ctx, i: usize) {
        let vmap = self.comp_view_flat();
        let loc = ctx.idx(self.pieces[i].loc);
        let p = &self.pieces[i];
        if p.hits < p.ptype.max_hits() {
            if vmap[loc] == b'X' {
                self.pieces[i].moves_left = 0; // stay in port
                return;
            }
            let mut pm = find_xobj(ctx, &vmap, loc, &SHIP_REPAIR, T_WATER, T_WATER, self);
            let dest = pm.best_loc;
            self.move_objective(ctx, i, &mut pm, dest, b".");
        } else {
            let new_loc = find_attack(ctx, &vmap, loc, SHIP_ATTACK, b".");
            if new_loc != loc {
                self.comp_step(i, new_loc, ctx);
                return;
            }
            let mut amap = vmap.clone();
            unmark_explore_locs(ctx, &mut amap);
            let mut pm = find_xobj(ctx, &amap, loc, &SHIP_FIGHT, T_WATER, T_WATER, self);
            let dest = pm.best_loc;
            self.move_objective(ctx, i, &mut pm, dest, SHIP_FIGHT.objectives);
        }
    }

    /// Takes one step toward `dest` along the path map (`move_objective`).
    fn move_objective(&mut self, ctx: &mut Ctx, i: usize, pm: &mut PathMap, dest: usize, adj_list: &[u8]) {
        let loc = ctx.idx(self.pieces[i].loc);
        let ptype = self.pieces[i].ptype;
        if dest == loc {
            // no destination: sit (a fighter still burns fuel)
            self.pieces[i].moves_left = 0;
            if ptype == PieceType::Fighter {
                self.pieces[i].fuel -= 1;
            }
            return;
        }
        let vmap = self.comp_view_flat();
        if vmap[dest] == UNEXPLORED && ctx.dist(dest, loc) == 2 {
            // exploring: any step next door will do
            for n in ctx.adj_on(loc).collect::<Vec<_>>() {
                pm.cells[n].terrain = T_PATH;
            }
        } else {
            mark_path(ctx, pm, dest);
        }
        let terrain: &[u8] = match ptype {
            PieceType::Army => b"+",
            PieceType::Fighter => b"+.X",
            _ => b".X",
        };
        let mut step = find_dir(ctx, pm, &vmap, loc, terrain, adj_list);
        let on_ship = ptype == PieceType::Army && self.army_ship(i).is_some();
        if step == loc && !on_ship {
            // path blocked: try squares next to the path
            mark_near_path(ctx, pm, loc);
            step = find_dir(ctx, pm, &vmap, loc, terrain, adj_list);
        }
        if step == loc && ptype == PieceType::Army && ctx.rmap[loc] == b'*' {
            // encourage an army to leave its city
            if let Some(n) = ctx.adj_on(loc).find(|&n| vmap[n] == LAND) {
                step = n;
            }
        }
        if step == loc {
            self.pieces[i].moves_left = 0;
        } else {
            self.comp_step(i, step, ctx);
        }
    }
}

// ----------------------------------------------------------------------------------
// Production helpers
// ----------------------------------------------------------------------------------

/// True if changing a city's production away from `prod` would improve the balance.
fn overproduced(prod: usize, city_count: &[i64; 9], ratio: &[i64; 9]) -> bool {
    (0..9).any(|i| i != prod && (city_count[prod] - 1) * ratio[i] > (city_count[i] + 1) * ratio[prod])
}

/// Whichever of two types is needed more.
fn need_more(city_count: &[i64; 9], ratio: &[i64; 9], p1: usize, p2: usize) -> usize {
    if city_count[p1] * ratio[p2] <= city_count[p2] * ratio[p1] { p1 } else { p2 }
}

/// The most needed type of production (`comp_set_needed`). Lake cities build armies or
/// fighters; others never fighters.
fn comp_set_needed(city_count: &mut [i64; 9], ratio: &[i64; 9], army_ok: bool, is_lake: bool) -> usize {
    if !army_ok {
        city_count[ARMY] = INF as i64;
    }
    if is_lake {
        return need_more(city_count, ratio, ARMY, FIGHTER);
    }
    city_count[FIGHTER] = INF as i64;
    let mut best = ARMY;
    for prod in 0..9 {
        best = need_more(city_count, ratio, best, prod);
    }
    best
}

// ----------------------------------------------------------------------------------
// View-map helpers (map.c)
// ----------------------------------------------------------------------------------

/// Marks out a continent (or, with `bad` = LAND, a body of water) containing `start`,
/// plus the unexplored squares bordering it (`vmap_cont`).
fn vmap_cont(ctx: &Ctx, vmap: &[u8], start: usize, bad: u8) -> Vec<bool> {
    let mut cont = vec![false; vmap.len()];
    cont[start] = true;
    let mut from = vec![start];
    while !from.is_empty() {
        let mut to = Vec::new();
        for &l in &from {
            for n in ctx.adj_on(l) {
                if cont[n] {
                    continue;
                }
                if vmap[n] == UNEXPLORED {
                    cont[n] = true; // mark, but don't expand, unexplored territory
                    continue;
                }
                let terrain = match vmap[n] {
                    LAND => LAND,
                    SEA => SEA,
                    _ => ctx_real(ctx, n),
                };
                if terrain != bad {
                    cont[n] = true;
                    to.push(n);
                }
            }
        }
        from = to;
    }
    cont
}

/// The real map at `i` ('+', '.', or '*'), whether or not the ctx has it cached.
fn ctx_real(ctx: &Ctx, i: usize) -> u8 {
    ctx.rmap.get(i).copied().unwrap_or(LAND)
}

/// Counts what is on a continent (`vmap_cont_scan`).
fn cont_scan(_ctx: &Ctx, cont: &[bool], vmap: &[u8], _game: &Game) -> Counts {
    let mut counts = Counts::default();
    for (i, &on) in cont.iter().enumerate() {
        if !on {
            continue;
        }
        match vmap[i] {
            UNEXPLORED => counts.unexplored += 1,
            b'O' => counts.user_cities += 1,
            b'A' => counts.user_armies += 1,
            b'X' => counts.comp_cities += 1,
            b'*' => counts.unowned_cities += 1,
            _ => {}
        }
    }
    counts
}

/// True if `loc` is sea and every square around it is known sea (`vmap_at_sea`).
fn at_sea(ctx: &Ctx, vmap: &[u8], loc: usize) -> bool {
    if ctx.rmap[loc] != SEA {
        return false;
    }
    ctx.adj_on(loc).all(|n| vmap[n] != UNEXPLORED && vmap[n] != LAND && ctx.rmap[n] == SEA)
}

/// The best adjacent thing to attack, or `loc` if none (`find_attack`). `terrain` is
/// matched against the real map.
fn find_attack(ctx: &Ctx, vmap: &[u8], loc: usize, list: &[u8], terrain: &[u8]) -> usize {
    let mut best = loc;
    let mut best_val = usize::MAX;
    for n in ctx.adj_on(loc) {
        if terrain.contains(&ctx.rmap[n]) {
            if let Some(p) = list.iter().position(|&ch| ch == vmap[n]) {
                if p < best_val {
                    best_val = p;
                    best = n;
                }
            }
        }
    }
    best
}

/// Replaces unexplored squares with the pruned predictions (`unmark_explore_locs`).
fn unmark_explore_locs(ctx: &Ctx, xmap: &mut [u8]) {
    for i in 0..xmap.len() {
        if ctx.on_board(i) && xmap[i] == UNEXPLORED {
            xmap[i] = ctx.emap[i];
        }
    }
}

/// Predicts unexplored territory that is surrounded by lots of land or lots of water
/// as being land or water (`vmap_prune_explore_locs`), so that exploring units don't
/// chase every unknown square. Stops when a pass would remove all unexplored territory
/// or nothing more can be predicted.
fn prune_explore_locs(ctx: &Ctx, vmap: &mut [u8]) {
    let size = vmap.len();
    let (rows, cols) = (ctx.rows, ctx.cols);
    let mut land = vec![0i32; size];
    let mut water = vec![0i32; size];
    let mut explored = 0usize;
    let mut from: Vec<usize> = Vec::new();
    let neighbours = |i: usize| {
        let (r, c) = ((i / cols) as i64, (i % cols) as i64);
        DIRS.iter().filter_map(move |&(dr, dc)| {
            let (nr, nc) = (r + dr as i64, c + dc as i64);
            if nr < 0 || nc < 0 || nr >= rows as i64 || nc >= cols as i64 { None } else { Some(nr as usize * cols + nc as usize) }
        })
    };
    for i in 0..size {
        if vmap[i] != UNEXPLORED {
            explored += 1;
        } else {
            for n in neighbours(i) {
                if vmap[n] == UNEXPLORED {
                    continue;
                }
                if ctx.rmap[n] != SEA { land[i] += 1 } else { water[i] += 1 }
            }
            if land[i] > 0 || water[i] > 0 {
                from.push(i);
            }
        }
    }
    let edge_row = |i: usize| i < cols || i >= size - cols;
    let corner = |i: usize| i == 0 || i == size - 1;
    let expand = |vmap: &mut [u8], land: &mut Vec<i32>, water: &mut Vec<i32>, i: usize, is_land: bool, to: &mut Vec<usize>, explored: &mut usize| {
        *explored += 1;
        vmap[i] = if is_land { LAND } else { SEA };
        for n in neighbours(i) {
            if vmap[n] == UNEXPLORED {
                if land[n] == 0 && water[n] == 0 {
                    to.push(n);
                }
                if is_land { land[n] += 1 } else { water[n] += 1 }
            }
        }
    };

    // high-probability predictions
    loop {
        if from.len() + explored == size {
            return;
        }
        let mut to = Vec::new();
        let mut copied = 0;
        for &i in &from {
            if vmap[i] != UNEXPLORED {
                continue;
            }
            let (l, w) = (land[i], water[i]);
            let as_land = if l >= 5 {
                Some(true)
            } else if w >= 5 {
                Some(false)
            } else if edge_row(i) && l >= 3 {
                Some(true)
            } else if edge_row(i) && w >= 3 {
                Some(false)
            } else if corner(i) && l >= 2 {
                Some(true)
            } else if corner(i) && w >= 2 {
                Some(false)
            } else {
                None
            };
            match as_land {
                Some(is_land) => expand(vmap, &mut land, &mut water, i, is_land, &mut to, &mut explored),
                None => {
                    to.push(i);
                    copied += 1;
                }
            }
        }
        let n = from.len();
        from = to;
        if copied == n {
            break;
        }
    }

    // one pass of medium-probability predictions
    if from.len() + explored == size {
        return;
    }
    let mut to = Vec::new();
    for &i in &from {
        if vmap[i] != UNEXPLORED {
            continue;
        }
        if land[i] > water[i] {
            expand(vmap, &mut land, &mut water, i, true, &mut to, &mut explored);
        } else if land[i] < water[i] {
            expand(vmap, &mut land, &mut water, i, false, &mut to, &mut explored);
        } else {
            to.push(i);
        }
    }
    from = to;

    // low-probability passes
    loop {
        if from.len() + explored >= size.saturating_sub(rows) {
            return;
        }
        let mut to = Vec::new();
        let mut copied = 0;
        for &i in &from {
            if vmap[i] != UNEXPLORED {
                continue;
            }
            let (l, w) = (land[i], water[i]);
            if l >= 4 && w < 4 {
                expand(vmap, &mut land, &mut water, i, true, &mut to, &mut explored);
            } else if w >= 4 && l < 4 {
                expand(vmap, &mut land, &mut water, i, false, &mut to, &mut explored);
            } else if edge_row(i) && l > w {
                expand(vmap, &mut land, &mut water, i, true, &mut to, &mut explored);
            } else if edge_row(i) && w > l {
                expand(vmap, &mut land, &mut water, i, false, &mut to, &mut explored);
            } else {
                to.push(i);
                copied += 1;
            }
        }
        let n = from.len();
        from = to;
        if copied == n {
            break;
        }
    }
}

// ----------------------------------------------------------------------------------
// Path-finding (map.c)
// ----------------------------------------------------------------------------------

/// True if any square of `vmap` is one of `info`'s objectives. If none is, a search can
/// only come back empty-handed, so it is skipped.
fn has_objective(vmap: &[u8], info: &MoveInfo) -> bool {
    let mut wanted = [false; 256];
    for &ch in info.objectives {
        wanted[ch as usize] = true;
    }
    vmap.iter().any(|&ch| wanted[ch as usize])
}

fn new_pathmap(ctx: &Ctx, start: usize, terrain: u8) -> PathMap {
    let mut cells = vec![Cell { cost: INF, inc_cost: 0, terrain: T_UNKNOWN }; ctx.rows * ctx.cols];
    cells[start] = Cell { cost: 0, inc_cost: 0, terrain };
    PathMap { cells, best_cost: INF, best_loc: start }
}

/// The kind of terrain at `to` for a search coming from `from` (`terrain_type`).
fn terrain_type(ctx: &Ctx, pm: &PathMap, vmap: &[u8], info: &MoveInfo, from: usize, to: usize, game: &Game) -> u8 {
    match vmap[to] {
        LAND => return T_LAND,
        SEA => return T_WATER,
        b'%' => return T_UNKNOWN, // magic objective
        UNEXPLORED => return pm.cells[from].terrain,
        _ => {}
    }
    match ctx.rmap[to] {
        SEA => T_WATER,
        LAND => T_LAND,
        _ => {
            let owner = ctx.city_at[to].map(|c| game.cities[c].owner);
            if owner == Some(info.city_owner) { T_WATER } else { T_UNKNOWN }
        }
    }
}

/// The cost of reaching objective `loc` from a perimeter `base_cost` away (`objective_cost`).
fn objective_cost(vmap: &[u8], info: &MoveInfo, loc: usize, base_cost: i32, ctx: &Ctx, game: &Game) -> i32 {
    let Some(p) = info.objectives.iter().position(|&ch| ch == vmap[loc]) else { return INF };
    let w = info.weights[p];
    if w >= 0 {
        return w + base_cost;
    }
    // W_TT_BUILD: a city building a transport; allow for the wait
    let Some(city) = ctx.city_at[loc].map(|c| &game.cities[c]) else { return base_cost + 2 };
    if city.prod != PieceType::Transport {
        return base_cost + 2;
    }
    let w = (PieceType::Transport.build_cost() - city.work) * 2;
    w.max(base_cost + 2)
}

/// Expands a perimeter by one ring (`expand_perimeter`). New cells are returned in order
/// as (cell, is_land); cells of a kind not in `kind` are not expanded.
#[allow(clippy::too_many_arguments)]
fn expand_perimeter(
    ctx: &Ctx, pm: &mut PathMap, vmap: &[u8], info: &MoveInfo, cur: &[usize], kind: u8,
    cur_cost: i32, inc_wcost: i32, inc_lcost: i32, game: &Game,
) -> Vec<(usize, bool)> {
    let mut out = Vec::with_capacity(cur.len() * 3 + 8);
    for &from in cur {
        for n in ctx.adj_on(from) {
            if pm.cells[n].cost != INF {
                continue;
            }
            let new_type = terrain_type(ctx, pm, vmap, info, from, n, game);
            if new_type == T_LAND && (kind & T_LAND) != 0 {
                pm.cells[n] = Cell { cost: cur_cost + inc_lcost, inc_cost: inc_lcost, terrain: T_LAND };
                out.push((n, true));
            } else if new_type == T_WATER && (kind & T_WATER) != 0 {
                pm.cells[n] = Cell { cost: cur_cost + inc_wcost, inc_cost: inc_wcost, terrain: T_WATER };
                out.push((n, false));
            } else if new_type == T_UNKNOWN {
                // unreachable cell
                pm.cells[n] = Cell { cost: cur_cost + INF / 2, inc_cost: INF / 2, terrain: T_UNKNOWN };
            }
            if pm.cells[n].cost != INF {
                let obj_cost = objective_cost(vmap, info, n, cur_cost, ctx, game);
                if obj_cost < pm.best_cost {
                    pm.best_cost = obj_cost;
                    pm.best_loc = n;
                    if new_type == T_UNKNOWN {
                        pm.cells[n].cost = cur_cost + 2;
                        pm.cells[n].inc_cost = 2;
                    }
                }
            }
        }
    }
    out
}

/// Finds the cheapest objective over one kind of terrain (`vmap_find_xobj`); `start` is
/// the terrain of the starting square, `kind` what may be crossed.
fn find_xobj(ctx: &Ctx, vmap: &[u8], loc: usize, info: &MoveInfo, start: u8, kind: u8, game: &Game) -> PathMap {
    let mut pm = new_pathmap(ctx, loc, start);
    if !has_objective(vmap, info) {
        return pm;
    }
    let mut from = vec![loc];
    let mut cur_cost = 0;
    loop {
        let to: Vec<usize> = expand_perimeter(ctx, &mut pm, vmap, info, &from, kind, cur_cost, 1, 1, game)
            .into_iter().map(|(n, _)| n).collect();
        cur_cost += 1;
        if to.is_empty() || pm.best_cost <= cur_cost {
            return pm;
        }
        from = to;
    }
}

/// Finds an objective moving from land onto water (`vmap_find_lwobj`): land costs 2,
/// water 1. Only an objective cheaper than `beat_cost` counts.
fn find_lwobj(ctx: &Ctx, vmap: &[u8], loc: usize, info: &MoveInfo, beat_cost: i32, game: &Game) -> PathMap {
    let mut pm = new_pathmap(ctx, loc, T_LAND);
    pm.best_cost = beat_cost;
    if !has_objective(vmap, info) {
        return pm;
    }
    let mut cur_land = vec![loc];
    let mut cur_water: Vec<usize> = Vec::new();
    let mut cur_cost = 0;
    loop {
        let mut new_water: Vec<usize> = expand_perimeter(ctx, &mut pm, vmap, info, &cur_water, T_WATER, cur_cost, 1, 1, game)
            .into_iter().map(|(n, _)| n).collect();
        let mut new_land = Vec::new();
        for (n, is_land) in expand_perimeter(ctx, &mut pm, vmap, info, &cur_land, T_AIR, cur_cost, 1, 2, game) {
            if is_land { new_land.push(n) } else { new_water.push(n) }
        }
        cur_water = expand_perimeter(ctx, &mut pm, vmap, info, &new_water, T_WATER, cur_cost + 1, 1, 1, game)
            .into_iter().map(|(n, _)| n).collect();
        cur_cost += 2;
        if (cur_water.is_empty() && new_land.is_empty()) || pm.best_cost <= cur_cost {
            return pm;
        }
        cur_land = new_land;
    }
}

/// Finds an objective moving from water onto land (`vmap_find_wlobj`).
fn find_wlobj(ctx: &Ctx, vmap: &[u8], loc: usize, info: &MoveInfo, game: &Game) -> PathMap {
    let mut pm = new_pathmap(ctx, loc, T_WATER);
    if !has_objective(vmap, info) {
        return pm;
    }
    let mut cur_water = vec![loc];
    let mut cur_land: Vec<usize> = Vec::new();
    let mut cur_cost = 0;
    loop {
        let mut new_water = Vec::new();
        let mut new_land = Vec::new();
        for (n, is_land) in expand_perimeter(ctx, &mut pm, vmap, info, &cur_water, T_AIR, cur_cost, 1, 2, game) {
            if is_land { new_land.push(n) } else { new_water.push(n) }
        }
        for (n, _) in expand_perimeter(ctx, &mut pm, vmap, info, &cur_land, T_LAND, cur_cost, 1, 2, game) {
            new_land.push(n);
        }
        cur_water = expand_perimeter(ctx, &mut pm, vmap, info, &new_water, T_WATER, cur_cost + 1, 1, 1, game)
            .into_iter().map(|(n, _)| n).collect();
        cur_cost += 2;
        if (cur_water.is_empty() && new_land.is_empty()) || pm.best_cost <= cur_cost {
            return pm;
        }
        cur_land = new_land;
    }
}

/// The shortest path to a known destination (`vmap_find_dest`); its `best_loc` is `dest`
/// if reachable, else `loc`.
fn find_dest(ctx: &Ctx, vmap: &[u8], loc: usize, dest: usize, owner: Owner, kind: u8, game: &Game) -> PathMap {
    let mut v = vmap.to_vec();
    v[dest] = b'%';
    let info = MoveInfo { city_owner: owner, objectives: b"%", weights: &[1] };
    let start = if kind == T_AIR { T_LAND } else { kind };
    let mut pm = new_pathmap(ctx, loc, start);
    let mut from = vec![loc];
    let mut cur_cost = 0;
    loop {
        let to: Vec<usize> = expand_perimeter(ctx, &mut pm, &v, &info, &from, kind, cur_cost, 1, 1, game)
            .into_iter().map(|(n, _)| n).collect();
        cur_cost += 1;
        if to.is_empty() || pm.best_cost <= cur_cost {
            return pm;
        }
        from = to;
    }
}

/// Marks every square on a shortest path from the start to `dest` (`vmap_mark_path`).
fn mark_path(ctx: &Ctx, pm: &mut PathMap, dest: usize) {
    let mut stack = vec![dest];
    while let Some(d) = stack.pop() {
        let cell = pm.cells[d];
        if cell.cost == 0 || cell.terrain == T_PATH {
            continue;
        }
        pm.cells[d].terrain = T_PATH;
        for n in ctx.adj_on(d) {
            if pm.cells[n].cost == cell.cost - cell.inc_cost {
                stack.push(n);
            }
        }
    }
}

/// Marks the squares next to `loc` that are next to the marked path (`vmap_mark_near_path`).
fn mark_near_path(ctx: &Ctx, pm: &mut PathMap, loc: usize) {
    let hits: Vec<usize> = ctx.adj_on(loc)
        .filter(|&n| ctx.adj_on(n).any(|x| x != loc && pm.cells[x].terrain == T_PATH))
        .collect();
    for n in hits {
        pm.cells[n].terrain = T_PATH;
    }
}

/// Picks the next step along the marked path (`vmap_find_dir`): a free square of the right
/// terrain, preferring diagonals, squares next to many `adj_char` squares, and squares in
/// the middle of the path.
fn find_dir(ctx: &Ctx, pm: &PathMap, vmap: &[u8], loc: usize, terrain: &[u8], adj_char: &[u8]) -> usize {
    let mut best = loc;
    let mut best_count = -INF;
    let mut best_path = -1;
    for d in FIND_DIR_ORDER {
        let Some(n) = ctx.adj(loc, d) else { continue };
        if pm.cells[n].terrain != T_PATH || !terrain.contains(&vmap[n]) {
            continue;
        }
        let len = adj_char.len() as i32;
        let count: i32 = ctx.adj_on(n)
            .filter_map(|x| adj_char.iter().position(|&ch| ch == vmap[x]))
            .map(|p| 8 * (len - p as i32))
            .sum();
        let path_count = ctx.adj_on(n).filter(|&x| pm.cells[x].terrain == T_PATH).count() as i32;
        if count > best_count || (count == best_count && path_count > best_path) {
            best_count = count;
            best_path = path_count;
            best = n;
        }
    }
    best
}
