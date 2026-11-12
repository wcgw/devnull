//! `/` and `?`: regex search over the logical text, so a match can span a wrap point but not a line break.

use std::fmt;

use regex::{Regex, RegexBuilder};

use super::{Document, Pos, Shaped};

/// How a pattern treats letter case, as Vim's `'ignorecase'` and `'smartcase'`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaseMode {
    /// Case matters (Vim's default).
    #[default]
    Sensitive,
    /// Case never matters (`'ignorecase'`).
    Insensitive,
    /// Case matters only when the pattern has an uppercase letter (`'ignorecase'` and `'smartcase'`).
    Smart,
}

/// A compiled search pattern, in Rust [`regex`](https://docs.rs/regex) syntax.
#[derive(Debug, Clone)]
pub struct Pattern {
    regex: Regex,
}

/// Why a pattern does not compile.
#[derive(Debug)]
pub struct PatternError(regex::Error);

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid pattern: {}", self.0)
    }
}

impl std::error::Error for PatternError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl Pattern {
    /// Compiles `pattern`, matching case as `case` says.
    ///
    /// # Errors
    ///
    /// If `pattern` is not a valid regex.
    pub fn new(pattern: &str, case: CaseMode) -> Result<Self, PatternError> {
        let insensitive = match case {
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
            CaseMode::Smart => !pattern.chars().any(char::is_uppercase),
        };
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .build()
            .map_err(PatternError)?;
        Ok(Self { regex })
    }

    /// Matches `word` literally and only as a whole word, as `*` does. Like Vim, it ignores `'smartcase'`, so
    /// [`CaseMode::Smart`] matches case-insensitively.
    ///
    /// # Panics
    ///
    /// Never for any `word`: it is escaped, so the pattern is always valid. A panic here is a bug.
    #[must_use]
    pub fn whole_word(word: &str, case: CaseMode) -> Self {
        let case = if case == CaseMode::Sensitive {
            CaseMode::Sensitive
        } else {
            CaseMode::Insensitive
        };
        let pattern = format!(r"\b{}\b", regex::escape(word));
        Self::new(&pattern, case).expect("an escaped word between word boundaries is a valid regex")
    }

    /// The grapheme columns where matches start in `line`, in order.
    fn starts(&self, line: &Shaped) -> Vec<usize> {
        self.regex
            .find_iter(&line.text)
            .map(|m| {
                line.graphemes
                    .partition_point(|g| g.start <= m.start())
                    .saturating_sub(1)
            })
            .collect()
    }
}

/// Which way a search goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `/` and `n` after `/`: toward the end.
    Forward,
    /// `?` and `n` after `?`: toward the start.
    Backward,
}

/// What a search did, for the message Vim would show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum SearchOutcome {
    /// Moved to a match.
    Found,
    /// Moved to a match after wrapping around an end of the document ("search hit BOTTOM, continuing at TOP").
    Wrapped,
    /// No match anywhere; the cursor did not move ("Pattern not found").
    NotFound,
}

/// The next match from `pos` going `direction`, and whether reaching it wrapped around.
pub(super) fn next_match(
    doc: &Document,
    pattern: &Pattern,
    pos: Pos,
    direction: Direction,
) -> Option<(Pos, bool)> {
    let first = |line| {
        Some(Pos {
            line,
            col: *pattern.starts(doc.line(line)).first()?,
        })
    };
    let last = |line| {
        Some(Pos {
            line,
            col: *pattern.starts(doc.line(line)).last()?,
        })
    };
    let in_line = |col| Pos {
        line: pos.line,
        col,
    };
    let lines = doc.line_count();
    let (unwrapped, wrapped) = match direction {
        Direction::Forward => (
            pattern
                .starts(doc.line(pos.line))
                .into_iter()
                .find(|&c| c > pos.col)
                .map(in_line)
                .or_else(|| (pos.line + 1..lines).find_map(first)),
            (0..=pos.line).find_map(first),
        ),
        Direction::Backward => (
            pattern
                .starts(doc.line(pos.line))
                .into_iter()
                .rev()
                .find(|&c| c < pos.col)
                .map(in_line)
                .or_else(|| (0..pos.line).rev().find_map(last)),
            (pos.line..lines).rev().find_map(last),
        ),
    };
    unwrapped
        .map(|p| (p, false))
        .or_else(|| wrapped.map(|p| (p, true)))
}
