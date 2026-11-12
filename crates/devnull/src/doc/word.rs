//! Vim's word motions (`w b e` and `W B E`), ported from its `fwd_word`, `bck_word` and `end_word`.
//!
//! They walk the logical text, not screen rows: wrapping never splits a word. As in Vim, the end of each line is a
//! position of its own that counts as a blank, so a line break separates words, and an empty line is a word that
//! `w` and `b` stop on.

use super::{Document, Pos};

/// Character classes, as Vim's `cls()`: words are made of one class, blanks separate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Whitespace and line ends.
    Blank,
    /// Punctuation and other symbols.
    Punct,
    /// Letters, digits and `_`.
    Word,
    /// Wide symbols such as emoji, a class of their own as in Vim.
    Wide,
}

/// Which word definition a motion uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// `w b e`: runs of one class.
    Small,
    /// `W B E`: runs of non-blanks.
    Big,
}

/// A walk over the logical text, one grapheme or line end at a time.
struct Walk<'d> {
    doc: &'d Document,
    kind: Kind,
    pos: Pos,
}

impl Walk<'_> {
    fn class(&self) -> Class {
        let line = self.doc.line(self.pos.line);
        if self.pos.col >= line.len() || line.is_blank(self.pos.col) {
            return Class::Blank;
        }
        if self.kind == Kind::Big {
            return Class::Word;
        }
        let first = line.grapheme(self.pos.col).chars().next().unwrap_or(' ');
        if first.is_alphanumeric() || first == '_' {
            Class::Word
        } else if line.graphemes[self.pos.col].width > 1 {
            Class::Wide
        } else {
            Class::Punct
        }
    }

    /// Whether the walk is on an empty line.
    fn on_empty_line(&self) -> bool {
        self.doc.line(self.pos.line).len() == 0
    }

    /// Steps forward onto the next grapheme or line end; `false` at the end of the document.
    fn inc(&mut self) -> bool {
        if self.pos.col < self.doc.line(self.pos.line).len() {
            self.pos.col += 1;
        } else if self.pos.line + 1 < self.doc.line_count() {
            self.pos = Pos {
                line: self.pos.line + 1,
                col: 0,
            };
        } else {
            return false;
        }
        true
    }

    /// Steps backward onto the previous grapheme or line end; `false` at the start of the document.
    fn dec(&mut self) -> bool {
        if self.pos.col > 0 {
            self.pos.col -= 1;
        } else if self.pos.line > 0 {
            self.pos.line -= 1;
            self.pos.col = self.doc.line(self.pos.line).len();
        } else {
            return false;
        }
        true
    }

    /// Steps while on `class`; `false` if the document ended first.
    fn skip(&mut self, class: Class, forward: bool) -> bool {
        while self.class() == class {
            if !(if forward { self.inc() } else { self.dec() }) {
                return false;
            }
        }
        true
    }
}

/// `w`/`W`: the start of the next word, or the last character of the document when there is none.
pub(super) fn forward(doc: &Document, pos: Pos, kind: Kind) -> Pos {
    let mut walk = Walk { doc, kind, pos };
    let class = walk.class();
    let reached_next = walk.inc()
        && (class == Class::Blank || walk.skip(class, true))
        && loop {
            if walk.class() != Class::Blank || (walk.on_empty_line() && walk.pos.col == 0) {
                break true;
            }
            if !walk.inc() {
                break false;
            }
        };
    if reached_next {
        walk.pos
    } else {
        doc.last_pos()
    }
}

/// `e`/`E`: the end of this word, or of the next one when already at an end. Stays put when there is none.
pub(super) fn end(doc: &Document, pos: Pos, kind: Kind) -> Pos {
    let mut walk = Walk { doc, kind, pos };
    let class = walk.class();
    if !walk.inc() {
        return pos;
    }
    let found = if walk.class() == class && class != Class::Blank {
        walk.skip(class, true)
    } else {
        walk.skip(Class::Blank, true) && {
            let next = walk.class();
            walk.skip(next, true)
        }
    };
    if !found {
        return pos;
    }
    walk.dec();
    walk.pos
}

/// `b`/`B`: the start of this word, or of the previous one when already at a start. An empty line counts as a word.
pub(super) fn backward(doc: &Document, pos: Pos, kind: Kind) -> Pos {
    let mut walk = Walk { doc, kind, pos };
    if !walk.dec() {
        return pos;
    }
    while walk.class() == Class::Blank {
        if walk.on_empty_line() {
            return walk.pos;
        }
        if !walk.dec() {
            return walk.pos;
        }
    }
    let class = walk.class();
    if walk.skip(class, false) {
        walk.inc();
    }
    walk.pos
}
