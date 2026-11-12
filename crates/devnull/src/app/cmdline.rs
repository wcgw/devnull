//! The command line being typed after `:`, `/` or `?`.

use unicode_width::UnicodeWidthStr;

use crate::key::{Code, Key, Mods};
use crate::keymap::CmdlineKind;

/// A command line being edited.
#[derive(Debug)]
pub(super) struct Cmdline {
    pub(super) kind: CmdlineKind,
    /// The count typed before `/` or `?`, for the search.
    pub(super) count: Option<usize>,
    text: Vec<char>,
    /// Insertion point, in chars.
    cursor: usize,
    /// While browsing history: the entry shown, counted back from the newest (0 is the newest), and what was typed
    /// before browsing started.
    browsing: Option<(usize, Vec<char>)>,
}

/// What a key did to the command line.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Edit {
    /// Still editing.
    Editing,
    /// `<CR>`: run this text.
    Submit(String),
    /// `<Esc>`, `<C-c>`, or `<BS>` on an empty line: leave without running anything.
    Cancel,
}

impl Cmdline {
    pub(super) fn new(kind: CmdlineKind, count: Option<usize>) -> Self {
        Self {
            kind,
            count,
            text: Vec::new(),
            cursor: 0,
            browsing: None,
        }
    }

    /// The prompt character: `:`, `/` or `?`.
    pub(super) fn prompt(&self) -> char {
        match self.kind {
            CmdlineKind::Ex => ':',
            CmdlineKind::Search(crate::doc::Direction::Forward) => '/',
            CmdlineKind::Search(crate::doc::Direction::Backward) => '?',
        }
    }

    pub(super) fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// Where the cursor is drawn, in cells after the prompt.
    pub(super) fn cursor_cells(&self) -> usize {
        self.text[..self.cursor].iter().collect::<String>().width()
    }

    /// Applies one key, with `history` (oldest first) for `<Up>` and `<Down>`.
    pub(super) fn key(&mut self, key: Key, history: &[String]) -> Edit {
        let ctrl = |ch| Key {
            code: Code::Char(ch),
            mods: Mods::CTRL,
        };
        match key {
            Key {
                code: Code::Enter, ..
            } => return Edit::Submit(self.text()),
            Key {
                code: Code::Esc, ..
            } => return Edit::Cancel,
            k if k == ctrl('c') => return Edit::Cancel,
            Key {
                code: Code::Backspace,
                ..
            } => {
                if self.text.is_empty() {
                    return Edit::Cancel;
                }
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.text.remove(self.cursor);
                }
            }
            Key {
                code: Code::Delete, ..
            } => {
                if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                }
            }
            Key {
                code: Code::Left, ..
            } => self.cursor = self.cursor.saturating_sub(1),
            Key {
                code: Code::Right, ..
            } => self.cursor = (self.cursor + 1).min(self.text.len()),
            Key {
                code: Code::Home, ..
            } => self.cursor = 0,
            k if k == ctrl('b') => self.cursor = 0,
            Key {
                code: Code::End, ..
            } => self.cursor = self.text.len(),
            k if k == ctrl('e') => self.cursor = self.text.len(),
            k if k == ctrl('u') => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            k if k == ctrl('w') => {
                let end = self.cursor;
                let mut start = end;
                while start > 0 && self.text[start - 1].is_whitespace() {
                    start -= 1;
                }
                let word = |c: char| c.is_alphanumeric() || c == '_';
                let in_word = start > 0 && word(self.text[start - 1]);
                while start > 0
                    && !self.text[start - 1].is_whitespace()
                    && word(self.text[start - 1]) == in_word
                {
                    start -= 1;
                }
                self.text.drain(start..end);
                self.cursor = start;
            }
            Key { code: Code::Up, .. } => self.browse(history, true),
            Key {
                code: Code::Down, ..
            } => self.browse(history, false),
            Key {
                code: Code::Char(ch),
                mods,
            } if !mods.contains(Mods::CTRL) && !mods.contains(Mods::ALT) => {
                self.insert(&[ch]);
            }
            _ => {}
        }
        Edit::Editing
    }

    /// Inserts pasted text at the cursor, up to its first line break: a command line is one line.
    pub(super) fn paste(&mut self, text: &str) {
        let line: Vec<char> = text
            .chars()
            .take_while(|&c| c != '\n' && c != '\r')
            .collect();
        self.insert(&line);
    }

    fn insert(&mut self, chars: &[char]) {
        self.text
            .splice(self.cursor..self.cursor, chars.iter().copied());
        self.cursor += chars.len();
    }

    /// `<Up>` (`older`) and `<Down>`: steps through `history`, back to what was typed past the newest entry.
    fn browse(&mut self, history: &[String], older: bool) {
        let typed = || self.text.clone();
        let next = match (&self.browsing, older) {
            (None, true) if !history.is_empty() => Some(0),
            (None, _) => return,
            (Some((back, _)), true) => Some((back + 1).min(history.len() - 1)),
            (Some((0, _)), false) => None,
            (Some((back, _)), false) => Some(back - 1),
        };
        let original = self
            .browsing
            .take()
            .map_or_else(typed, |(_, original)| original);
        self.text = match next {
            Some(back) => {
                self.browsing = Some((back, original));
                history[history.len() - 1 - back].chars().collect()
            }
            None => original,
        };
        self.cursor = self.text.len();
    }
}
