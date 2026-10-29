//! A stand-in for the parts of `crossterm` that rusted-empire's shared code uses: the
//! keyboard and mouse event types (and nothing that talks to a terminal). crossterm
//! itself doesn't build for the browser; the browser front end (`web/src/web.rs`)
//! fills these in from DOM events. The names and shapes match crossterm 0.27, so the
//! shared code compiles unchanged against either.

pub mod event {
    /// An input event.
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum Event {
        Key(KeyEvent),
        Mouse(MouseEvent),
        /// The screen changed size (columns, rows).
        Resize(u16, u16),
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct KeyEvent {
        pub code: KeyCode,
        pub modifiers: KeyModifiers,
        pub kind: KeyEventKind,
        pub state: KeyEventState,
    }

    impl KeyEvent {
        pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
            KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }
        }
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum KeyCode {
        Backspace,
        Enter,
        Left,
        Right,
        Up,
        Down,
        Home,
        End,
        PageUp,
        PageDown,
        Tab,
        BackTab,
        Delete,
        Insert,
        F(u8),
        Char(char),
        Null,
        Esc,
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum KeyEventKind {
        Press,
        Repeat,
        Release,
    }

    /// Modifier keys held down (a small set of bit flags, as in crossterm).
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Default)]
    pub struct KeyModifiers(u8);

    impl KeyModifiers {
        pub const NONE: KeyModifiers = KeyModifiers(0);
        pub const SHIFT: KeyModifiers = KeyModifiers(1);
        pub const CONTROL: KeyModifiers = KeyModifiers(2);
        pub const ALT: KeyModifiers = KeyModifiers(4);
        pub const fn empty() -> KeyModifiers {
            KeyModifiers(0)
        }
        pub const fn contains(&self, other: KeyModifiers) -> bool {
            self.0 & other.0 == other.0
        }
        pub const fn bits(&self) -> u8 {
            self.0
        }
        pub const fn from_bits_truncate(bits: u8) -> KeyModifiers {
            KeyModifiers(bits & 7)
        }
    }

    impl std::ops::BitOr for KeyModifiers {
        type Output = KeyModifiers;
        fn bitor(self, rhs: KeyModifiers) -> KeyModifiers {
            KeyModifiers(self.0 | rhs.0)
        }
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct KeyEventState(u8);

    impl KeyEventState {
        pub const NONE: KeyEventState = KeyEventState(0);
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct MouseEvent {
        pub kind: MouseEventKind,
        pub column: u16,
        pub row: u16,
        pub modifiers: KeyModifiers,
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum MouseEventKind {
        Down(MouseButton),
        Up(MouseButton),
        Drag(MouseButton),
        Moved,
        ScrollDown,
        ScrollUp,
    }

    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
    }
}
