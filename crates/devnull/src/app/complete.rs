//! `<Tab>` on the command line: what the word before the cursor may be completed to, by what the command takes.

use super::feed::Source;
use super::{App, AppMode, options};
use crate::ex;
use crate::hl::{self, Group};
use crate::keymap::Mode;

/// What `<Tab>` may put in place of the end of a command line: from char `start` on, one of `candidates`, sorted.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Completions {
    /// Where the word they replace starts, in chars.
    pub(super) start: usize,
    pub(super) candidates: Vec<String>,
}

impl App {
    /// `<Tab>` (`forward`) and `<S-Tab>` on the ex command line: the next or previous completion.
    pub(super) fn complete_command_line(&mut self, forward: bool) {
        let AppMode::Cmdline(mut cmdline) = std::mem::replace(&mut self.mode, AppMode::Normal)
        else {
            return;
        };
        cmdline.complete(forward, |line| self.ex_completions(line));
        self.mode = AppMode::Cmdline(cmdline);
    }

    /// What the ex command line `line`, up to the cursor, may be completed to: a command's name while it is the
    /// first word, else an argument of the command, as the command takes them.
    pub(super) fn ex_completions(&self, line: &str) -> Completions {
        let command = line.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
        let skipped = line.len() - command.len();
        let name_len = command
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(command.len());
        if name_len == command.len() {
            let names = ex::command_names().map(str::to_owned);
            return completions(line[..skipped].chars().count(), command, names);
        }
        let Some(name) = ex::full_name(&command[..name_len]) else {
            return Completions::default();
        };
        // `:e! name` as `:e name`.
        let rest = &command[name_len..];
        let rest = rest.strip_prefix('!').unwrap_or(rest);
        if !rest.starts_with(char::is_whitespace) {
            return Completions::default();
        }
        let word_at = line
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(at, c)| at + c.len_utf8());
        let word = &line[word_at..];
        let start = line[..word_at].chars().count();
        let before: Vec<&str> = rest.split_whitespace().collect();
        let before = &before[..before.len() - usize::from(!word.is_empty())];
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        let candidates: Vec<String> = match (name, before) {
            ("set", _) => {
                let (offset, values) = options::completions(word);
                let value = word.chars().skip(offset).collect::<String>();
                return completions(start + offset, &value, values);
            }
            ("colorscheme", []) => hl::colorscheme_names().map(str::to_owned).collect(),
            ("edit", []) => self
                .buffers
                .iter()
                .map(|buffer| buffer.name.clone())
                .chain(Source::names())
                .chain(["profile".to_owned()])
                .collect(),
            ("buffer", []) => self
                .listed()
                .into_iter()
                .map(|index| self.buffers[index].name.clone())
                .collect(),
            ("highlight", []) => groups().chain(names(&["clear", "link"])).collect(),
            ("highlight", ["link"] | ["link", _]) => groups().collect(),
            ("help", []) => [Mode::Normal, Mode::Visual]
                .into_iter()
                .flat_map(|mode| self.keymap.listing(mode))
                .map(|listed| listed.keys)
                .collect(),
            ("pick", []) => names(&["buffers", "users"]),
            ("browse", []) => names(&["link"]),
            _ => Vec::new(),
        };
        completions(start, word, candidates)
    }
}

/// The names of the highlight groups, as `:hi` takes them.
fn groups() -> impl Iterator<Item = String> {
    Group::ALL.iter().map(|group| group.name().to_owned())
}

/// Those of `candidates` that complete `word`, from char `start` on: sorted, each once.
fn completions(
    start: usize,
    word: &str,
    candidates: impl IntoIterator<Item = String>,
) -> Completions {
    let mut candidates: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with(word))
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    Completions { start, candidates }
}
