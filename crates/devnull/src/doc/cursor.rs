use super::word::{self, Kind};
use super::{Anchor, Document, Layout, Pos, ScreenPos, Target};

/// The Normal-mode cursor of one window over a [`super::Document`].
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    /// Where the cursor is; `None` until it first moves, meaning the start of the document.
    anchor: Option<Anchor>,
    /// Where vertical moves aim for (Vim's `curswant`), kept across rows too short to reach it. `None` means the
    /// cursor's current column.
    want: Option<Want>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    /// A display column.
    Column(usize),
    /// The last character of each row, after `$`.
    RowEnd,
}

/// A Vim motion, in screen-row terms (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Motion {
    /// `j`: down one screen row.
    Down,
    /// `k`: up one screen row.
    Up,
    /// `l`: right one character, onto the next row of a wrapped line but not past the line's end.
    Right,
    /// `h`: left one character, not past the line's start.
    Left,
    /// `0`: the first character of the screen row.
    RowStart,
    /// `^`: the first non-blank character of the screen row.
    FirstNonBlank,
    /// `$`: the last character of the screen row (`count - 1` rows down); later vertical moves stay at row ends.
    RowEnd,
    /// `w`: the start of the next word; words are runs of letters, digits and `_`, or of other symbols.
    WordForward,
    /// `b`: the start of this word, or of the previous one.
    WordBackward,
    /// `e`: the end of this word, or of the next one.
    WordEnd,
    /// `W`: the start of the next run of non-blanks.
    BigWordForward,
    /// `B`: the start of this run of non-blanks, or of the previous one.
    BigWordBackward,
    /// `E`: the end of this run of non-blanks, or of the next one.
    BigWordEnd,
    /// `f{char}`: the `count`th next occurrence of the character in the line (the whole line, across its rows).
    FindForward(char),
    /// `t{char}`: just before the `count`th next occurrence of the character in the line.
    TillForward(char),
    /// `F{char}`: the `count`th previous occurrence of the character in the line.
    FindBackward(char),
    /// `T{char}`: just after the `count`th previous occurrence of the character in the line.
    TillBackward(char),
    /// `]]`: the start of the next block (commit); stays put on the last one.
    NextBlock,
    /// `[[`: the start of this block, or of the previous one when already there.
    PrevBlock,
    /// `}`: the next empty line after a paragraph, or the end of the document.
    ParagraphForward,
    /// `{`: the previous empty line before a paragraph, or the start of the document.
    ParagraphBackward,
    /// `{count}G`, `{count}gg`: the first non-blank of screen row `count` (from 1), clamped to the last row.
    GotoRow(usize),
    /// `G` without a count: the first non-blank of the last screen row.
    LastRow,
}

impl Motion {
    fn word_kind(self) -> Kind {
        match self {
            Self::BigWordForward | Self::BigWordBackward | Self::BigWordEnd => Kind::Big,
            _ => Kind::Small,
        }
    }
}

impl Cursor {
    /// A cursor at the start of the document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where the cursor is drawn.
    #[must_use]
    pub fn screen(&self, layout: &Layout<'_>) -> ScreenPos {
        if layout.row_count() == 0 {
            return ScreenPos { row: 0, col: 0 };
        }
        let pos = self.pos(layout.doc());
        let row = layout.row_of(pos);
        ScreenPos {
            row,
            col: layout.cell_of(row, pos.col),
        }
    }

    /// What the text under the cursor refers to, if anything.
    #[must_use]
    pub fn target<'d>(&self, layout: &Layout<'d>) -> Option<&'d Target> {
        if layout.row_count() == 0 {
            return None;
        }
        let doc = layout.doc();
        let pos = self.pos(doc);
        doc.line(pos.line).target_at(pos.col)
    }

    /// Moves `count` times by `motion` (a count of 0 counts as 1, as in Vim). A motion that cannot move, such as
    /// `f` for a character that isn't there, leaves the cursor where it is.
    pub fn apply(&mut self, layout: &Layout<'_>, motion: Motion, count: usize) {
        if layout.row_count() == 0 {
            return;
        }
        let count = count.max(1);
        let doc = layout.doc();
        let here = self.screen(layout);
        let want = match motion {
            Motion::Down | Motion::Up => self.want.unwrap_or(Want::Column(here.col)),
            Motion::RowEnd => Want::RowEnd,
            _ => {
                if let Some(pos) = self.destination(layout, motion, count) {
                    self.set(doc, pos);
                }
                return;
            }
        };
        let row = match motion {
            Motion::Down => here.row + count,
            Motion::Up => here.row.saturating_sub(count),
            _ => here.row + count - 1,
        }
        .min(layout.row_count() - 1);
        let pos = match want {
            Want::Column(col) => layout.pos_at(row, col),
            Want::RowEnd => layout.row_end(row),
        };
        self.set(doc, pos);
        self.want = Some(want);
    }

    /// Where a motion that forgets the wanted column goes, or `None` if it cannot move.
    fn destination(&self, layout: &Layout<'_>, motion: Motion, count: usize) -> Option<Pos> {
        let doc = layout.doc();
        let pos = self.pos(doc);
        let repeat = |step: &dyn Fn(Pos) -> Pos| (0..count).fold(pos, |pos, _| step(pos));
        let block_steps = |steps: usize| isize::try_from(steps).unwrap_or(isize::MAX);
        Some(match motion {
            Motion::Down | Motion::Up | Motion::RowEnd => {
                unreachable!("vertical motions keep the wanted column")
            }
            Motion::Right => Pos {
                col: (pos.col + count).min(doc.line(pos.line).len().saturating_sub(1)),
                ..pos
            },
            Motion::Left => Pos {
                col: pos.col.saturating_sub(count),
                ..pos
            },
            Motion::RowStart => {
                let row = layout.row(layout.row_of(pos));
                Pos {
                    line: row.line,
                    col: row.start,
                }
            }
            Motion::FirstNonBlank => layout.first_non_blank(layout.row_of(pos)),
            Motion::GotoRow(row) => {
                layout.first_non_blank(row.saturating_sub(1).min(layout.row_count() - 1))
            }
            Motion::LastRow => layout.first_non_blank(layout.row_count() - 1),
            Motion::WordForward | Motion::BigWordForward => {
                repeat(&|p| word::forward(doc, p, motion.word_kind()))
            }
            Motion::WordBackward | Motion::BigWordBackward => {
                repeat(&|p| word::backward(doc, p, motion.word_kind()))
            }
            Motion::WordEnd | Motion::BigWordEnd => {
                repeat(&|p| word::end(doc, p, motion.word_kind()))
            }
            Motion::FindForward(ch) => Pos {
                col: find(doc, pos, ch, count, true)?,
                ..pos
            },
            Motion::TillForward(ch) => Pos {
                col: find(doc, pos, ch, count, true)? - 1,
                ..pos
            },
            Motion::FindBackward(ch) => Pos {
                col: find(doc, pos, ch, count, false)?,
                ..pos
            },
            Motion::TillBackward(ch) => Pos {
                col: find(doc, pos, ch, count, false)? + 1,
                ..pos
            },
            Motion::NextBlock => {
                let line = doc.block_start(pos.line, block_steps(count));
                (line != doc.block_start(pos.line, 0)).then_some(Pos { line, col: 0 })?
            }
            Motion::PrevBlock => {
                let at_start = pos.col == 0 && doc.block_start(pos.line, 0) == pos.line;
                let steps = if at_start { count } else { count - 1 };
                Pos {
                    line: doc.block_start(pos.line, -block_steps(steps)),
                    col: 0,
                }
            }
            Motion::ParagraphForward => repeat(&|p| paragraph(doc, p, true)),
            Motion::ParagraphBackward => repeat(&|p| paragraph(doc, p, false)),
        })
    }

    fn pos(&self, doc: &Document) -> Pos {
        self.anchor
            .as_ref()
            .map_or_else(Pos::default, |anchor| doc.resolve(anchor))
    }

    /// Moves to `pos`, forgetting the column vertical moves aimed for.
    fn set(&mut self, doc: &Document, pos: Pos) {
        self.anchor = Some(doc.anchor(pos));
        self.want = None;
    }
}

/// The column of the `count`th occurrence of `ch` after (`forward`) or before `pos` in its line.
fn find(doc: &Document, pos: Pos, ch: char, count: usize, forward: bool) -> Option<usize> {
    let line = doc.line(pos.line);
    let is_ch = |&i: &usize| is_char(line.grapheme(i), ch);
    if forward {
        (pos.col + 1..line.len()).filter(is_ch).nth(count - 1)
    } else {
        (0..pos.col).rev().filter(is_ch).nth(count - 1)
    }
}

/// Whether `grapheme` is exactly the character `ch`.
fn is_char(grapheme: &str, ch: char) -> bool {
    let mut chars = grapheme.chars();
    chars.next() == Some(ch) && chars.next().is_none()
}

/// One `}` (`forward`) or `{` from `pos`, as Vim's `findpar`: skip any empty lines, then the paragraph, and stop on
/// the empty line after it. Without one, `}` ends on the last character and `{` on the first.
fn paragraph(doc: &Document, pos: Pos, forward: bool) -> Pos {
    let mut line = pos.line;
    let mut in_paragraph = false;
    loop {
        let next = if forward {
            line + 1
        } else {
            line.wrapping_sub(1)
        };
        if next >= doc.line_count() {
            return if forward {
                doc.last_pos()
            } else {
                Pos::default()
            };
        }
        in_paragraph |= doc.line(line).len() > 0;
        line = next;
        if in_paragraph && doc.line(line).len() == 0 {
            return Pos { line, col: 0 };
        }
    }
}
