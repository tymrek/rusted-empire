use serde::{Serialize, Deserialize};

/// One map tile. Serialized as the original ASCII byte (`b'#'` land, `b'.'` sea)
/// so existing save files keep loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum Terrain {
    Sea,
    Land,
}

impl From<Terrain> for u8 {
    fn from(t: Terrain) -> u8 {
        match t {
            Terrain::Sea => b'.',
            Terrain::Land => b'#',
        }
    }
}

impl TryFrom<u8> for Terrain {
    type Error = String;
    fn try_from(b: u8) -> Result<Self, String> {
        match b {
            b'.' => Ok(Terrain::Sea),
            b'#' => Ok(Terrain::Land),
            other => Err(format!("unknown terrain byte {}", other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveResult {
    Moved,
    CombatWon,
    CombatLost,
    CityCaptured(usize),
    CityAssaultFailed,
    InvalidMove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum Owner {
    Player,
    Comp,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

    /// Moves per turn for a unit with `hits` hits left, as in the original game:
    /// speed scales with remaining health, rounded up, and a live unit always gets
    /// at least one move. (A battleship at half health moves at half speed.)
    pub fn moves_for_hits(&self, hits: i32) -> i32 {
        if hits <= 0 {
            return 0;
        }
        (self.max_moves() * hits + self.max_hits() - 1) / self.max_hits()
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

    /// The kind of piece this one carries, if any (transports carry armies, carriers
    /// carry fighters).
    pub fn cargo_type(&self) -> Option<PieceType> {
        match self {
            PieceType::Transport => Some(PieceType::Army),
            PieceType::Carrier => Some(PieceType::Fighter),
            _ => None,
        }
    }

    /// How many pieces a carrier of this type holds with `hits` hits left, as in the
    /// original's `obj_capacity`: full capacity scaled by remaining health, rounded up.
    pub fn capacity_for_hits(&self, hits: i32) -> usize {
        let full = match self {
            PieceType::Transport => 6,
            PieceType::Carrier => 8,
            _ => return 0,
        };
        if hits <= 0 {
            return 0;
        }
        ((full * hits + self.max_hits() - 1) / self.max_hits()) as usize
    }

    /// Pieces that fly: they cross land and sea alike.
    pub fn flies(&self) -> bool {
        matches!(self, PieceType::Fighter | PieceType::Satellite)
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct Loc {
    pub r: usize,
    pub c: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnitActivity {
    Idle,          
    Explore,       
    Sentry,        
    Attack,        
    Fill,          
    Land,          
    Random,        
    TransportWait, 
    Upgrade,       
    Dir(i32, i32), 
    Dest(Loc),     
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
    /// Who the player last saw holding this city (`None`: never seen). See
    /// `Game::known_city_owner`.
    #[serde(default)]
    pub seen_owner: Option<Owner>,
}

/// What happened to one side's fighters during a game, for the `--headless` playtest rig.
/// Not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FighterStats {
    pub built: u32,
    /// Landings in a city or on a carrier (each refuels the fighter).
    pub landings: u32,
    /// Landings with 2 or less fuel to spare: near misses.
    pub low_fuel_landings: u32,
    /// Ran out of fuel and crashed.
    pub out_of_fuel: u32,
    /// Lost in combat, attacking or defending.
    pub shot_down: u32,
    /// Flew into a neutral or enemy city and was shot down.
    pub lost_over_city: u32,
    /// Went down aboard a carrier that was sunk.
    pub lost_with_carrier: u32,
}

impl std::ops::AddAssign for FighterStats {
    fn add_assign(&mut self, o: Self) {
        self.built += o.built;
        self.landings += o.landings;
        self.low_fuel_landings += o.low_fuel_landings;
        self.out_of_fuel += o.out_of_fuel;
        self.shot_down += o.shot_down;
        self.lost_over_city += o.lost_over_city;
        self.lost_with_carrier += o.lost_with_carrier;
    }
}

/// A direction's name for messages (a (row, column) step).
pub fn dir_name(dir: (i32, i32)) -> &'static str {
    match dir {
        (-1, 0) => "North",
        (1, 0) => "South",
        (0, -1) => "West",
        (0, 1) => "East",
        (-1, -1) => "NW",
        (-1, 1) => "NE",
        (1, -1) => "SW",
        (1, 1) => "SE",
        _ => "Unknown",
    }
}
