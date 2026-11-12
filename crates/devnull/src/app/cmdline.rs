//! The command line being typed after `:`, `/` or `?`.

use unicode_width::UnicodeWidthStr;

use super::complete::Completions;
use crate::doc::Direction;
use crate::key::{Code, Key, Mods};
use crate::keymap::CmdlineKind;

/// What a command line is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Purpose {
    /// `:`.
    Ex,
    /// `/` and `?`.
    Search(Direction),
    /// The device key's passphrase: shown masked, never remembered.
    Passphrase,
}

impl From<CmdlineKind> for Purpose {
    fn from(kind: CmdlineKind) -> Self {
        match kind {
            CmdlineKind::Ex => Self::Ex,
            CmdlineKind::Search(direction) => Self::Search(direction),
        }
    }
}

/// A command line being edited.
#[derive(Debug)]
pub(super) struct Cmdline {
    pub(super) purpose: Purpose,
    /// The count typed before `/` or `?`, for the search.
    pub(super) count: Option<usize>,
    text: Vec<char>,
    /// Insertion point, in chars.
    cursor: usize,
    /// While browsing history: the entry shown, counted back from the newest (0 is the newest), and what was typed
    /// before browsing started.
    browsing: Option<(usize, Vec<char>)>,
    /// While `<Tab>` cycles through completions.
    completing: Option<Completing>,
}

/// Completions `<Tab>` and `<S-Tab>` cycle through, as Vim's 'wildmenu' does.
#[derive(Debug)]
struct Completing {
    /// Where the completed word starts, in chars.
    start: usize,
    /// The word as typed, which cycling comes back to.
    typed: Vec<char>,
    /// More than one.
    candidates: Vec<String>,
    /// The one in place, if any; else what was typed is.
    chosen: Option<usize>,
}

/// The completions shown while `<Tab>` cycles through them.
#[derive(Debug)]
pub(super) struct Shown<'c> {
    /// Where the completed word starts, in cells after the prompt.
    pub(super) cells: usize,
    pub(super) candidates: &'c [String],
    /// The one in place, if any.
    pub(super) chosen: Option<usize>,
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
    pub(super) fn new(purpose: Purpose, count: Option<usize>) -> Self {
        Self {
            purpose,
            count,
            text: Vec::new(),
            cursor: 0,
            browsing: None,
            completing: None,
        }
    }

    /// The prompt: `:`, `/`, `?` or `Passphrase: `.
    pub(super) fn prompt(&self) -> &'static str {
        match self.purpose {
            Purpose::Ex => ":",
            Purpose::Search(Direction::Forward) => "/",
            Purpose::Search(Direction::Backward) => "?",
            Purpose::Passphrase => "Passphrase: ",
        }
    }

    /// The text as shown: masked for a passphrase.
    pub(super) fn shown(&self) -> String {
        match self.purpose {
            Purpose::Passphrase => "*".repeat(self.text.len()),
            _ => self.text(),
        }
    }

    pub(super) fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// Where the cursor is drawn, in cells after the prompt.
    pub(super) fn cursor_cells(&self) -> usize {
        if self.purpose == Purpose::Passphrase {
            return self.cursor;
        }
        self.text[..self.cursor].iter().collect::<String>().width()
    }

    /// `<Tab>` (`forward`) and `<S-Tab>`: puts the next or previous of the completions `complete` gives for the text
    /// before the cursor in place of the word they complete; past either end, the word as typed. A lone completion
    /// is put in place at once, with nothing to cycle through.
    pub(super) fn complete(&mut self, forward: bool, complete: impl FnOnce(&str) -> Completions) {
        if self.completing.is_none() {
            let before: String = self.text[..self.cursor].iter().collect();
            let Completions { start, candidates } = complete(&before);
            let start = start.min(self.cursor);
            match &candidates[..] {
                [] => return,
                [lone] => {
                    let lone: Vec<char> = lone.chars().collect();
                    self.replace(start, &lone);
                    return;
                }
                _ => {}
            }
            self.completing = Some(Completing {
                start,
                typed: self.text[start..self.cursor].to_vec(),
                candidates,
                chosen: None,
            });
        }
        let Some(completing) = &mut self.completing else {
            return;
        };
        let last = completing.candidates.len() - 1;
        completing.chosen = match (completing.chosen, forward) {
            (None, true) => Some(0),
            (None, false) => Some(last),
            (Some(at), true) if at < last => Some(at + 1),
            (Some(at), false) if at > 0 => Some(at - 1),
            _ => None,
        };
        let replacement: Vec<char> = match completing.chosen {
            Some(at) => completing.candidates[at].chars().collect(),
            None => completing.typed.clone(),
        };
        let start = completing.start;
        self.replace(start, &replacement);
    }

    /// The completions `<Tab>` cycles through, while it does.
    pub(super) fn completions(&self) -> Option<Shown<'_>> {
        let completing = self.completing.as_ref()?;
        Some(Shown {
            cells: self.text[..completing.start]
                .iter()
                .collect::<String>()
                .width(),
            candidates: &completing.candidates,
            chosen: completing.chosen,
        })
    }

    /// Puts `chars` in place of the text from `start` to the cursor, the cursor after them.
    fn replace(&mut self, start: usize, chars: &[char]) {
        self.text.splice(start..self.cursor, chars.iter().copied());
        self.cursor = start + chars.len();
    }

    /// Applies one key, with `history` (oldest first) for `<Up>` and `<Down>`. Any key keeps the completion in
    /// place.
    pub(super) fn key(&mut self, key: Key, history: &[String]) -> Edit {
        self.completing = None;
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
        self.completing = None;
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
