//! The game's keyboard vocabulary: which key means which unit type, direction or order.
//!
//! Pure functions with no terminal or game state, so every screen agrees on the keys
//! (and a future client can reuse them).
use crate::models::{PieceType, UnitActivity};
use crossterm::event::KeyCode;

/// The unit type a key names (`a`rmy, `t`ransport, ... `z` or `l` for satellite).
pub fn piece_for_key(c: char) -> Option<PieceType> {
    match c.to_ascii_lowercase() {
        'a' => Some(PieceType::Army),
        't' => Some(PieceType::Transport),
        'p' => Some(PieceType::PatrolBoat),
        'd' => Some(PieceType::Destroyer),
        's' => Some(PieceType::Submarine),
        'c' => Some(PieceType::Carrier),
        'b' => Some(PieceType::Battleship),
        'f' => Some(PieceType::Fighter),
        'z' | 'l' => Some(PieceType::Satellite),
        _ => None,
    }
}

/// The key legend for choosing a unit type.
pub const PIECE_KEYS: &str = "[A]rmy,[T]rns,[P]tBoat,[D]est,[S]ub,[C]arr,[B]ttle,[F]ght,[Z]Sat";

/// The direction (row, column step) a key means: the numeric keypad layout
/// (7 8 9 / 4 6 / 1 2 3) or the arrow keys.
pub fn dir_for_key(code: KeyCode) -> Option<(i32, i32)> {
    match code {
        KeyCode::Char('8') | KeyCode::Up => Some((-1, 0)),
        KeyCode::Char('2') | KeyCode::Down => Some((1, 0)),
        KeyCode::Char('4') | KeyCode::Left => Some((0, -1)),
        KeyCode::Char('6') | KeyCode::Right => Some((0, 1)),
        KeyCode::Char('7') => Some((-1, -1)),
        KeyCode::Char('9') => Some((-1, 1)),
        KeyCode::Char('1') => Some((1, -1)),
        KeyCode::Char('3') => Some((1, 1)),
        _ => None,
    }
}

/// The direction a digit key on the numeric keypad means (digits only, no arrows).
pub fn dir_for_digit(c: char) -> Option<(i32, i32)> {
    if c.is_ascii_digit() { dir_for_key(KeyCode::Char(c)) } else { None }
}

/// What a key in an order menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKey {
    /// An order that needs nothing more.
    Order(UnitActivity),
    /// March in a direction: ask which.
    PickDirection,
    /// Go to a square: ask where.
    PickDestination,
}

/// The order a key gives a unit of type `ptype`, if that key means anything for it.
pub fn order_for_key(c: char, ptype: PieceType) -> Option<OrderKey> {
    let transport_or_carrier = ptype == PieceType::Transport || ptype == PieceType::Carrier;
    let order = match c.to_ascii_lowercase() {
        's' => UnitActivity::Sentry,
        'y' => UnitActivity::Attack,
        'g' => UnitActivity::Explore,
        'r' => UnitActivity::Random,
        'w' => UnitActivity::Idle,
        't' if ptype == PieceType::Army => UnitActivity::TransportWait,
        'l' if ptype == PieceType::Fighter => UnitActivity::Land,
        'f' if transport_or_carrier => UnitActivity::Fill,
        'u' if ptype.is_naval() => UnitActivity::Upgrade,
        'v' => return Some(OrderKey::PickDirection),
        'd' => return Some(OrderKey::PickDestination),
        _ => return None,
    };
    Some(OrderKey::Order(order))
}

/// The order menu for a unit type, in the order it is shown.
pub fn order_menu(ptype: PieceType) -> Vec<&'static str> {
    let mut opts = vec!["[S]entry", "[Y]attack", "[G]rope", "[D]est", "[R]nd", "[V]ctr", "[W]ake"];
    if ptype == PieceType::Army { opts.push("[T]rnsWait"); }
    if ptype == PieceType::Fighter { opts.push("[L]and"); }
    if ptype == PieceType::Transport || ptype == PieceType::Carrier { opts.push("[F]ill"); }
    if ptype.is_naval() { opts.push("[U]pgrade"); }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_unit_type_has_a_key() {
        for c in "atpdscbfz".chars() {
            assert!(piece_for_key(c).is_some(), "{}", c);
        }
        assert_eq!(piece_for_key('L'), Some(PieceType::Satellite));
        assert_eq!(piece_for_key('x'), None);
    }

    #[test]
    fn test_keypad_and_arrows_agree() {
        assert_eq!(dir_for_key(KeyCode::Up), dir_for_key(KeyCode::Char('8')));
        assert_eq!(dir_for_key(KeyCode::Char('7')), Some((-1, -1)));
        assert_eq!(dir_for_digit('5'), None);
        assert_eq!(crate::models::dir_name((1, 1)), "SE");
    }

    #[test]
    fn test_orders_only_for_units_that_can_take_them() {
        assert_eq!(order_for_key('t', PieceType::Army), Some(OrderKey::Order(UnitActivity::TransportWait)));
        assert_eq!(order_for_key('t', PieceType::Fighter), None);
        assert_eq!(order_for_key('L', PieceType::Fighter), Some(OrderKey::Order(UnitActivity::Land)));
        assert_eq!(order_for_key('f', PieceType::Army), None);
        assert_eq!(order_for_key('u', PieceType::Destroyer), Some(OrderKey::Order(UnitActivity::Upgrade)));
        assert_eq!(order_for_key('v', PieceType::Army), Some(OrderKey::PickDirection));
        assert_eq!(order_for_key('d', PieceType::Army), Some(OrderKey::PickDestination));
        assert!(order_menu(PieceType::Carrier).contains(&"[F]ill"));
        assert!(!order_menu(PieceType::Army).contains(&"[U]pgrade"));
    }
}
