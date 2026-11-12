//! Keys as Vim names them: `q`, `<CR>`, `<C-w>`, `<S-Tab>`.

use std::fmt;
use std::ops::BitOr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One key press, with the modifiers Vim distinguishes.
///
/// Shift is folded into printable characters (`Q`, `?`), and control letters are lowercase (`<C-W>` is `<C-w>`),
/// so each key has one value. The kitty keyboard protocol, when the terminal speaks it, keeps `<C-i>` apart from
/// `<Tab>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// Which key.
    pub code: Code,
    /// The modifiers held with it.
    pub mods: Mods,
}

/// The key itself, without modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A printable character.
    Char(char),
    /// `<CR>`.
    Enter,
    /// `<Esc>`.
    Esc,
    /// `<Tab>`; `<S-Tab>` with [`Mods::SHIFT`].
    Tab,
    /// `<BS>`.
    Backspace,
    /// `<Del>`.
    Delete,
    /// `<Insert>`.
    Insert,
    /// `<Up>`.
    Up,
    /// `<Down>`.
    Down,
    /// `<Left>`.
    Left,
    /// `<Right>`.
    Right,
    /// `<Home>`.
    Home,
    /// `<End>`.
    End,
    /// `<PageUp>`.
    PageUp,
    /// `<PageDown>`.
    PageDown,
    /// `<F1>` to `<F12>` and beyond.
    F(u8),
}

/// A set of modifier keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Shift, kept only on keys that are not printable characters.
    pub const SHIFT: Self = Self(1);
    /// Control.
    pub const CTRL: Self = Self(2);
    /// Alt, which Vim calls Meta (`<M-x>`).
    pub const ALT: Self = Self(4);

    /// Whether every modifier in `other` is held.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for Mods {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Key {
    /// `code` with no modifier.
    #[must_use]
    pub fn new(code: Code) -> Self {
        Self {
            code,
            mods: Mods::NONE,
        }
    }

    /// A printable character with no modifier.
    #[must_use]
    pub fn char(ch: char) -> Self {
        Self::new(Code::Char(ch))
    }

    /// `<C-{ch}>`.
    #[must_use]
    pub fn ctrl(ch: char) -> Self {
        Self {
            code: Code::Char(ch.to_ascii_lowercase()),
            mods: Mods::CTRL,
        }
    }
}

/// A key press that Vim has no name for, such as a media key or a bare modifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnnamedKey(KeyCode);

impl fmt::Display for UnnamedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no Vim name for key {:?}", self.0)
    }
}

impl std::error::Error for UnnamedKey {}

impl TryFrom<KeyEvent> for Key {
    type Error = UnnamedKey;

    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        let mut mods = Mods::NONE;
        for (held, modifier) in [
            (KeyModifiers::SHIFT, Mods::SHIFT),
            (KeyModifiers::CONTROL, Mods::CTRL),
            (KeyModifiers::ALT, Mods::ALT),
        ] {
            if event.modifiers.contains(held) {
                mods = mods | modifier;
            }
        }
        let code = match event.code {
            KeyCode::Char(ch) => {
                mods = mods.without(Mods::SHIFT);
                Code::Char(if mods.contains(Mods::CTRL) {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                })
            }
            KeyCode::BackTab => {
                mods = mods | Mods::SHIFT;
                Code::Tab
            }
            KeyCode::Tab => Code::Tab,
            KeyCode::Enter => Code::Enter,
            KeyCode::Esc => Code::Esc,
            KeyCode::Backspace => Code::Backspace,
            KeyCode::Delete => Code::Delete,
            KeyCode::Insert => Code::Insert,
            KeyCode::Up => Code::Up,
            KeyCode::Down => Code::Down,
            KeyCode::Left => Code::Left,
            KeyCode::Right => Code::Right,
            KeyCode::Home => Code::Home,
            KeyCode::End => Code::End,
            KeyCode::PageUp => Code::PageUp,
            KeyCode::PageDown => Code::PageDown,
            KeyCode::F(n) => Code::F(n),
            other => return Err(UnnamedKey(other)),
        };
        Ok(Self { code, mods })
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> Key {
        Key::try_from(KeyEvent::new(code, modifiers)).expect("a key Vim has a name for")
    }

    #[test]
    fn printable_characters_keep_their_case_and_drop_shift() {
        assert_eq!(
            press(KeyCode::Char('q'), KeyModifiers::NONE),
            Key::char('q')
        );
        assert_eq!(
            press(KeyCode::Char('Q'), KeyModifiers::SHIFT),
            Key::char('Q')
        );
        assert_eq!(
            press(KeyCode::Char('?'), KeyModifiers::SHIFT),
            Key::char('?')
        );
    }

    #[test]
    fn control_and_alt_are_kept_and_letters_with_control_are_lowercase() {
        assert_eq!(
            press(KeyCode::Char('w'), KeyModifiers::CONTROL),
            Key::ctrl('w')
        );
        assert_eq!(
            press(
                KeyCode::Char('W'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            Key::ctrl('w')
        );
        assert_eq!(
            press(KeyCode::Char('x'), KeyModifiers::ALT),
            Key {
                code: Code::Char('x'),
                mods: Mods::ALT
            }
        );
    }

    #[test]
    fn special_keys_map_by_name_and_back_tab_is_shift_tab() {
        assert_eq!(
            press(KeyCode::Enter, KeyModifiers::NONE),
            Key::new(Code::Enter)
        );
        assert_eq!(press(KeyCode::Esc, KeyModifiers::NONE), Key::new(Code::Esc));
        assert_eq!(
            press(KeyCode::BackTab, KeyModifiers::SHIFT),
            Key {
                code: Code::Tab,
                mods: Mods::SHIFT
            }
        );
        assert_eq!(press(KeyCode::Tab, KeyModifiers::CONTROL).code, Code::Tab);
    }

    #[test]
    fn keys_vim_has_no_name_for_are_rejected() {
        assert!(Key::try_from(KeyEvent::new(KeyCode::CapsLock, KeyModifiers::NONE)).is_err());
    }
}
