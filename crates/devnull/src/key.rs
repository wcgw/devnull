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

impl fmt::Display for Key {
    /// Vim notation: a plain printable character as itself, anything else in angle brackets (`<C-w>`, `<Space>`,
    /// `<S-Tab>`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            Code::Char(' ') => "Space".to_owned(),
            Code::Char('<') => "lt".to_owned(),
            Code::Char('|') => "Bar".to_owned(),
            Code::Char('\\') => "Bslash".to_owned(),
            Code::Char(ch) if self.mods == Mods::NONE => return write!(f, "{ch}"),
            Code::Char(ch) => ch.to_string(),
            Code::Enter => "CR".to_owned(),
            Code::Esc => "Esc".to_owned(),
            Code::Tab => "Tab".to_owned(),
            Code::Backspace => "BS".to_owned(),
            Code::Delete => "Del".to_owned(),
            Code::Insert => "Insert".to_owned(),
            Code::Up => "Up".to_owned(),
            Code::Down => "Down".to_owned(),
            Code::Left => "Left".to_owned(),
            Code::Right => "Right".to_owned(),
            Code::Home => "Home".to_owned(),
            Code::End => "End".to_owned(),
            Code::PageUp => "PageUp".to_owned(),
            Code::PageDown => "PageDown".to_owned(),
            Code::F(n) => format!("F{n}"),
        };
        f.write_str("<")?;
        for (modifier, prefix) in [(Mods::CTRL, "C-"), (Mods::ALT, "M-"), (Mods::SHIFT, "S-")] {
            if self.mods.contains(modifier) {
                f.write_str(prefix)?;
            }
        }
        write!(f, "{name}>")
    }
}

/// Parses Vim key notation (`:h key-notation`) such as `gg`, `<C-w>j` or `<leader>n`, with `<leader>` standing for
/// `leader`. Names are case-insensitive; anything that is not a key name, like `<Foo>` or an unclosed `<C-`, is
/// taken literally, character by character, as Vim does.
#[must_use]
pub fn parse_keys(notation: &str, leader: Key) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut rest = notation;
    while let Some(ch) = rest.chars().next() {
        if ch == '<'
            && let Some(end) = rest.find('>')
            && let Some(key) = named_key(&rest[1..end], leader)
        {
            keys.push(key);
            rest = &rest[end + 1..];
            continue;
        }
        keys.push(Key::char(ch));
        rest = &rest[ch.len_utf8()..];
    }
    keys
}

/// The key a `<...>` name stands for, such as `C-w`, `CR` or `leader`.
fn named_key(name: &str, leader: Key) -> Option<Key> {
    let mut mods = Mods::NONE;
    let mut base = name;
    while let [prefix, b'-', _, ..] = base.as_bytes() {
        mods = mods
            | match prefix.to_ascii_lowercase() {
                b'c' => Mods::CTRL,
                b'm' | b'a' => Mods::ALT,
                b's' => Mods::SHIFT,
                _ => break,
            };
        base = &base[2..];
    }
    let code = match base.to_ascii_lowercase().as_str() {
        "leader" if mods == Mods::NONE => return Some(leader),
        "cr" | "enter" | "return" => Code::Enter,
        "esc" => Code::Esc,
        "tab" => Code::Tab,
        "bs" | "backspace" => Code::Backspace,
        "del" | "delete" => Code::Delete,
        "insert" | "ins" => Code::Insert,
        "up" => Code::Up,
        "down" => Code::Down,
        "left" => Code::Left,
        "right" => Code::Right,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "space" => Code::Char(' '),
        "lt" => Code::Char('<'),
        "bar" => Code::Char('|'),
        "bslash" => Code::Char('\\'),
        lower => match lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
            Some(n) if (1..=37).contains(&n) => Code::F(n),
            _ => {
                let mut chars = base.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) if mods != Mods::NONE => Code::Char(ch),
                    _ => return None,
                }
            }
        },
    };
    Some(normalized(code, mods))
}

/// `code` with `mods`, in the one form [`Key`] keeps: shift folded into printable characters, control letters
/// lowercase.
fn normalized(code: Code, mods: Mods) -> Key {
    match code {
        Code::Char(ch) if mods.contains(Mods::CTRL) => Key {
            code: Code::Char(ch.to_ascii_lowercase()),
            mods: mods.without(Mods::SHIFT),
        },
        Code::Char(ch) if mods.contains(Mods::SHIFT) => Key {
            code: Code::Char(ch.to_ascii_uppercase()),
            mods: mods.without(Mods::SHIFT),
        },
        code => Key { code, mods },
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
            // The terminal already applied shift to the character (`?`, `Q`); only control needs normalizing.
            KeyCode::Char(ch) => return Ok(normalized(Code::Char(ch), mods.without(Mods::SHIFT))),
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

    fn keys(notation: &str) -> Vec<Key> {
        parse_keys(notation, Key::char(' '))
    }

    #[test]
    fn plain_characters_are_keys_of_their_own() {
        assert_eq!(keys("gg"), [Key::char('g'), Key::char('g')]);
    }

    #[test]
    fn special_key_names_are_case_insensitive() {
        assert_eq!(
            keys("<CR><esc><Tab><BS><Enter><Up><F2>"),
            [
                Key::new(Code::Enter),
                Key::new(Code::Esc),
                Key::new(Code::Tab),
                Key::new(Code::Backspace),
                Key::new(Code::Enter),
                Key::new(Code::Up),
                Key::new(Code::F(2)),
            ]
        );
    }

    #[test]
    fn modifiers_combine_and_normalize() {
        assert_eq!(keys("<C-w>j"), [Key::ctrl('w'), Key::char('j')]);
        assert_eq!(keys("<C-W>"), keys("<c-w>"));
        let alt_x = Key {
            code: Code::Char('x'),
            mods: Mods::ALT,
        };
        assert_eq!(keys("<M-x><A-x>"), [alt_x, alt_x]);
        assert_eq!(
            keys("<S-Tab>"),
            [Key {
                code: Code::Tab,
                mods: Mods::SHIFT
            }]
        );
        assert_eq!(
            keys("<C-S-Tab>"),
            [Key {
                code: Code::Tab,
                mods: Mods::CTRL | Mods::SHIFT
            }]
        );
        assert_eq!(keys("<S-a>"), [Key::char('A')]);
    }

    #[test]
    fn named_characters_and_the_leader() {
        assert_eq!(
            keys("<Space><lt><Bar><Bslash>"),
            [
                Key::char(' '),
                Key::char('<'),
                Key::char('|'),
                Key::char('\\')
            ]
        );
        assert_eq!(
            parse_keys("<leader>n", Key::char(',')),
            [Key::char(','), Key::char('n')]
        );
        assert_eq!(keys("<Leader><leader>"), [Key::char(' '), Key::char(' ')]);
    }

    #[test]
    fn anything_that_is_not_a_key_name_is_taken_literally() {
        assert_eq!(
            keys("<Foo>"),
            "<Foo>".chars().map(Key::char).collect::<Vec<_>>()
        );
        assert_eq!(
            keys("<C-"),
            "<C-".chars().map(Key::char).collect::<Vec<_>>()
        );
        assert_eq!(keys("a<"), [Key::char('a'), Key::char('<')]);
    }

    #[test]
    fn keys_print_back_in_vim_notation() {
        let printed: Vec<String> = keys("<C-w>j<Space><lt><S-Tab><F2><CR>Q<M-x>")
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            printed,
            [
                "<C-w>", "j", "<Space>", "<lt>", "<S-Tab>", "<F2>", "<CR>", "Q", "<M-x>"
            ]
        );
    }
}
