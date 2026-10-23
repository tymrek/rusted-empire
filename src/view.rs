//! What one player's screen is showing: input mode, cursor, scroll position and so on.
//!
//! This is client-side state. None of it is part of the game itself, none of it is saved,
//! and the game never reads it: `Game` holds only the state of the world, `View` only
//! how it is being looked at. (Keeping the two apart is what will let the game run as a
//! server, with each client keeping its own view.)
use crate::game::Game;
use crate::models::{Loc, Owner};

/// What the main loop is doing with the player's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    /// Asking for orders unit by unit.
    Manual,
    /// Playing turns on its own until a unit needs orders.
    Auto,
    /// The map editor (`e`): a cursor to inspect squares and give orders.
    Edit,
    /// Picking a destination for the unit at this index.
    PickDest(usize),
}

pub struct View {
    pub mode: GameMode,
    /// The cursor in edit mode and when picking a destination.
    pub cursor: Loc,
    /// Where the player has dragged the map to, if anywhere; any key snaps back.
    pub center: Option<Loc>,
    /// The mouse position at the last drag event.
    pub drag_from: Option<(u16, u16)>,
    /// How many messages the log is scrolled back.
    pub msg_scroll: usize,
    /// `Game::messages_logged` when the log was last drawn: new messages scroll it back
    /// to the bottom.
    pub messages_seen: u64,
    /// Show the computer's map instead of the player's (offered once it has resigned).
    pub show_comp_view: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            mode: GameMode::Manual,
            cursor: Loc { r: 0, c: 0 },
            center: None,
            drag_from: None,
            msg_scroll: 0,
            messages_seen: 0,
            show_comp_view: false,
        }
    }
}

impl View {
    /// True if the map should show the cursor (edit mode or picking a destination).
    pub fn shows_cursor(&self) -> bool {
        matches!(self.mode, GameMode::Edit | GameMode::PickDest(_))
    }

    /// Scrolls the message log back to the newest message whenever the game has logged
    /// something since the last look.
    pub fn follow_messages(&mut self, game: &Game) {
        if game.messages_logged != self.messages_seen {
            self.messages_seen = game.messages_logged;
            self.msg_scroll = 0;
        }
    }
}

/// Where to look when nothing else says: the player's first city, else the middle of the
/// map.
pub fn home(game: &Game) -> Loc {
    game.cities.iter().find(|c| c.owner == Owner::Player).map(|c| c.loc)
        .unwrap_or(Loc { r: game.rows / 2, c: game.cols / 2 })
}
