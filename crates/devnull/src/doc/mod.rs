//! The text model behind every buffer (D4).
//!
//! A [`Document`] is a read-only sequence of keyed blocks (one per commit, header, …), each a list of [`Line`]s.
//! A [`Layout`] wraps it into screen rows for a width, and a [`Cursor`] moves over those rows with Vim motions
//! while staying on the same text when blocks are inserted or replaced and when the width changes.

mod cursor;
mod layout;
mod search;
#[cfg(test)]
mod tests;
mod visual;
mod word;

use std::collections::HashMap;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use cursor::{Cursor, Motion};
pub use layout::{Layout, ScreenPos};
pub use search::{CaseMode, Direction, Pattern, PatternError, SearchOutcome};
pub use visual::{Selection, SelectionKind};

use crate::hl::Group;

/// Identifies a block across document edits, such as a commit id or `"header"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey(String);

impl BlockKey {
    /// A key from any string; keys are compared exactly.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl From<&str> for BlockKey {
    fn from(key: &str) -> Self {
        Self::new(key)
    }
}

/// One line of a block before wrapping, such as a commit's header or one paragraph of its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Styled text, wrapped to the layout width.
    Text(Vec<Span>),
    /// Space for an image: `rows` empty screen rows whatever the width, `cols` cells wide. The cursor moves through
    /// it like empty lines; the window draws the image over it (D7).
    Image {
        /// Height in screen rows; 0 counts as 1.
        rows: u16,
        /// Width in cells.
        cols: u16,
    },
}

impl Line {
    /// A line of plain text in [`Group::Normal`], with no target.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(vec![Span::new(text, Group::Normal)])
    }
}

/// What a block holds: its lines, and the folds over them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    /// The lines, top to bottom.
    pub lines: Vec<Line>,
    /// Ranges of lines that `zc` can collapse into a summary row, such as the rest of a long commit. Folds do not
    /// nest: a line belongs to the first fold covering it. Folds with an empty range or reaching past the lines are
    /// ignored.
    pub folds: Vec<Fold>,
}

impl From<Vec<Line>> for Content {
    fn from(lines: Vec<Line>) -> Self {
        Self {
            lines,
            folds: Vec::new(),
        }
    }
}

/// A foldable range of a block's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    /// The folded lines, as indices into the block's lines.
    pub lines: Range<usize>,
    /// The one row shown in their place while closed; it is cut, not wrapped, at the window's width.
    pub summary: Line,
    /// Whether the fold starts closed. Once opened or closed with a fold command, it keeps that state, also when its
    /// block is replaced.
    pub closed: bool,
}

/// What `zo`, `zc` and `za` do to the fold under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldOp {
    /// `zo`.
    Open,
    /// `zc`.
    Close,
    /// `za`.
    Toggle,
}

/// A run of text sharing one highlight group and one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text; it should not contain line breaks, which start a new [`Line`] instead.
    pub text: String,
    /// How the text is drawn.
    pub group: Group,
    /// What `<CR>`, `gd` and `gx` act on when the cursor is on this text, if anything.
    pub target: Option<Target>,
}

impl Span {
    /// Text in `group`, with no target.
    pub fn new(text: impl Into<String>, group: Group) -> Self {
        Self {
            text: text.into(),
            group,
            target: None,
        }
    }

    /// The same span, referring to `target`.
    #[must_use]
    pub fn with_target(self, target: Target) -> Self {
        Self {
            target: Some(target),
            ..self
        }
    }
}

/// Something the text refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A web link, opened in the browser.
    Url(String),
    /// A user or organization handle, without the `@`.
    Mention(String),
    /// A commit id.
    Commit(String),
}

/// A read-only document of keyed blocks of lines.
///
/// Positions inside it are logical (line, grapheme) pairs, independent of any width; see [`Layout`] for screen rows.
#[derive(Debug, Default)]
pub struct Document {
    blocks: Vec<Block>,
    /// Block index by key.
    index: HashMap<BlockKey, usize>,
    /// The first logical line of each block, plus the total line count at the end.
    starts: Vec<usize>,
    /// Folds opened (`true`) or closed (`false`) by a fold command, by block and fold index, overriding
    /// [`Fold::closed`].
    fold_states: HashMap<(BlockKey, usize), bool>,
}

#[derive(Debug)]
struct Block {
    key: BlockKey,
    lines: Vec<Shaped>,
    folds: Vec<ShapedFold>,
}

#[derive(Debug)]
struct ShapedFold {
    lines: Range<usize>,
    summary: Shaped,
    closed: bool,
}

impl Block {
    fn new(key: BlockKey, content: Content) -> Self {
        let lines: Vec<Shaped> = content.lines.into_iter().map(Shaped::new).collect();
        let folds = content
            .folds
            .into_iter()
            .filter(|fold| !fold.lines.is_empty() && fold.lines.end <= lines.len())
            .map(|fold| ShapedFold {
                lines: fold.lines,
                summary: Shaped::new(fold.summary),
                closed: fold.closed,
            })
            .collect();
        Self { key, lines, folds }
    }
}

/// A fold located in the document.
struct FoldAt<'d> {
    /// Its block and index there, as `Document::fold_states` keys it.
    block: usize,
    index: usize,
    /// The folded logical lines, document-wide.
    lines: Range<usize>,
    summary: &'d Shaped,
}

/// A line with its graphemes pre-segmented, so layout and motions don't segment the same text repeatedly.
#[derive(Debug)]
struct Shaped {
    /// The text of all spans, concatenated.
    text: String,
    /// Byte offset and display width of each grapheme, in order.
    graphemes: Vec<Grapheme>,
    /// The spans' styling and targets, by the byte range of `text` they cover, in order.
    runs: Vec<Run>,
    /// For an image, its height in rows; text lines wrap instead. On an image line a position's column is not a
    /// grapheme (there are none) but which of its rows the cursor is on.
    fixed_rows: Option<usize>,
}

/// A span without its text: what styles and targets `text[previous run's end..end]`.
#[derive(Debug)]
struct Run {
    /// Byte offset just past the span's text.
    end: usize,
    group: Group,
    target: Option<Target>,
}

#[derive(Debug, Clone, Copy)]
struct Grapheme {
    start: usize,
    width: usize,
}

impl Shaped {
    fn new(line: Line) -> Self {
        let spans = match line {
            Line::Text(spans) => spans,
            Line::Image { rows, .. } => {
                return Self {
                    text: String::new(),
                    graphemes: Vec::new(),
                    runs: Vec::new(),
                    fixed_rows: Some(usize::from(rows.max(1))),
                };
            }
        };
        let mut text = String::new();
        let mut runs = Vec::with_capacity(spans.len());
        for span in spans {
            text.push_str(&span.text);
            runs.push(Run {
                end: text.len(),
                group: span.group,
                target: span.target,
            });
        }
        let graphemes = text
            .grapheme_indices(true)
            .map(|(start, g)| Grapheme {
                start,
                width: g.width(),
            })
            .collect();
        Self {
            text,
            graphemes,
            runs,
            fixed_rows: None,
        }
    }

    /// The target of the span holding grapheme `col`.
    fn target_at(&self, col: usize) -> Option<&Target> {
        let byte = self.graphemes.get(col)?.start;
        self.runs.iter().find(|run| run.end > byte)?.target.as_ref()
    }

    fn len(&self) -> usize {
        self.graphemes.len()
    }

    /// How many leading graphemes fit in `width` cells.
    fn fitting(&self, width: usize) -> usize {
        let mut used = 0;
        self.graphemes
            .iter()
            .take_while(|g| {
                used += g.width;
                used <= width
            })
            .count()
    }

    /// The highest column a position on this line can have: its last grapheme, or an image's last row.
    fn last_col(&self) -> usize {
        self.fixed_rows.unwrap_or(self.len()).saturating_sub(1)
    }

    fn is_image(&self) -> bool {
        self.fixed_rows.is_some()
    }

    /// The byte offset of grapheme `i`, or the text's length for `i == len()`.
    fn byte_offset(&self, i: usize) -> usize {
        self.graphemes.get(i).map_or(self.text.len(), |g| g.start)
    }

    /// The text of grapheme `i`.
    fn grapheme(&self, i: usize) -> &str {
        &self.text[self.byte_offset(i)..self.byte_offset(i + 1)]
    }

    /// Whether grapheme `i` is whitespace.
    fn is_blank(&self, i: usize) -> bool {
        self.grapheme(i).chars().all(char::is_whitespace)
    }
}

/// A position expressed relative to its block, so it survives edits elsewhere in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    block: BlockKey,
    /// Logical line within the block.
    line: usize,
    /// Grapheme within the line.
    col: usize,
    /// The document-wide logical line when anchored, used once the block is gone.
    fallback: usize,
}

/// A logical position: a line of the whole document and a grapheme index within it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    line: usize,
    col: usize,
}

impl Document {
    /// An empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a block at the end.
    ///
    /// # Panics
    ///
    /// If a block with the same key is already present.
    pub fn push_back(&mut self, key: impl Into<BlockKey>, content: impl Into<Content>) {
        self.insert(self.blocks.len(), key.into(), content.into());
    }

    /// Inserts a block at the start, as when newer commits arrive. Cursors stay on their text.
    ///
    /// # Panics
    ///
    /// If a block with the same key is already present.
    pub fn push_front(&mut self, key: impl Into<BlockKey>, content: impl Into<Content>) {
        self.insert(0, key.into(), content.into());
    }

    fn insert(&mut self, at: usize, key: BlockKey, content: Content) {
        assert!(
            !self.index.contains_key(&key),
            "duplicate block key {key:?}"
        );
        self.blocks.insert(at, Block::new(key, content));
        self.reindex();
    }

    /// Replaces the content of the block `key`, keeping its place. Cursors in it stay on the same line and
    /// character, clamped to the new text, and its folds keep the state fold commands gave them. Returns whether the
    /// block was there; nothing changes when it wasn't.
    pub fn replace(&mut self, key: impl Into<BlockKey>, content: impl Into<Content>) -> bool {
        let key = key.into();
        let Some(&block) = self.index.get(&key) else {
            return false;
        };
        self.blocks[block] = Block::new(key, content.into());
        self.reindex();
        true
    }

    /// Removes the block `key`, if present. Cursors in it move to whatever takes its place.
    pub fn remove(&mut self, key: impl Into<BlockKey>) {
        let key = key.into();
        if let Some(block) = self.index.get(&key).copied() {
            self.blocks.remove(block);
            self.fold_states
                .retain(|(block_key, _), _| *block_key != key);
            self.reindex();
        }
    }

    /// `zo`, `zc`, `za`: opens, closes or toggles the fold holding the cursor's line, if any. The cursor keeps its
    /// place in the text; while the fold is closed it is drawn on the fold's summary row.
    pub fn fold(&mut self, cursor: &Cursor, op: FoldOp) {
        if self.line_count() == 0 {
            return;
        }
        let line = cursor.pos(self).line;
        let Some(fold) = self.fold_at(line, false) else {
            return;
        };
        let closed = match op {
            FoldOp::Open => false,
            FoldOp::Close => true,
            FoldOp::Toggle => !self.is_closed(fold.block, fold.index),
        };
        let key = (self.blocks[fold.block].key.clone(), fold.index);
        self.fold_states.insert(key, !closed);
    }

    /// `zR` (`open`) and `zM`: opens or closes every fold.
    pub fn fold_all(&mut self, open: bool) {
        self.fold_states = self
            .blocks
            .iter()
            .flat_map(|block| {
                (0..block.folds.len()).map(|index| ((block.key.clone(), index), open))
            })
            .collect();
    }

    /// The fold covering `line`, if any; only a closed one when `closed_only`.
    fn fold_at(&self, line: usize, closed_only: bool) -> Option<FoldAt<'_>> {
        let block = self.block_of(line);
        let start = self.starts[block];
        let (index, fold) = self.blocks[block]
            .folds
            .iter()
            .enumerate()
            .find(|(_, fold)| fold.lines.contains(&(line - start)))?;
        if closed_only && !self.is_closed(block, index) {
            return None;
        }
        Some(FoldAt {
            block,
            index,
            lines: start + fold.lines.start..start + fold.lines.end,
            summary: &fold.summary,
        })
    }

    /// `pos`, or the start of the closed fold hiding it: where a motion that lands in a closed fold stops.
    fn outside_closed_fold(&self, pos: Pos) -> Pos {
        self.fold_at(pos.line, true).map_or(pos, |fold| Pos {
            line: fold.lines.start,
            col: 0,
        })
    }

    /// Where a motion leaving a closed fold that holds `pos` starts from: just past its last character going
    /// forward, its first line going backward. `pos` itself when no closed fold holds it.
    fn leaving_closed_fold(&self, pos: Pos, forward: bool) -> Pos {
        match self.fold_at(pos.line, true) {
            Some(fold) if forward => {
                let line = fold.lines.end - 1;
                Pos {
                    line,
                    col: self.line(line).len(),
                }
            }
            Some(fold) => Pos {
                line: fold.lines.start,
                col: 0,
            },
            None => pos,
        }
    }

    fn is_closed(&self, block: usize, index: usize) -> bool {
        let key = (self.blocks[block].key.clone(), index);
        self.fold_states
            .get(&key)
            .map_or(self.blocks[block].folds[index].closed, |open| !open)
    }

    /// Whether a block with this key is in the document.
    #[must_use]
    pub fn contains(&self, key: impl Into<BlockKey>) -> bool {
        self.index.contains_key(&key.into())
    }

    /// Whether the document has no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The anchor for `pos`, which must be a position in this document.
    fn anchor(&self, pos: Pos) -> Anchor {
        let block = self.block_of(pos.line);
        Anchor {
            block: self.blocks[block].key.clone(),
            line: pos.line - self.starts[block],
            col: pos.col,
            fallback: pos.line,
        }
    }

    /// Where `anchor` is now: the same text when its block still exists, clamped to the block's current lines;
    /// otherwise whatever took the block's place. An empty document resolves to the origin.
    fn resolve(&self, anchor: &Anchor) -> Pos {
        let line = match self.index.get(&anchor.block) {
            Some(&block) => {
                let len = self.blocks[block].lines.len();
                self.starts[block] + anchor.line.min(len.saturating_sub(1))
            }
            None => anchor.fallback.min(self.line_count().saturating_sub(1)),
        };
        if self.line_count() == 0 {
            return Pos::default();
        }
        Pos {
            line,
            col: anchor.col.min(self.line(line).last_col()),
        }
    }

    /// The last character of the document, or the origin when it is empty.
    fn last_pos(&self) -> Pos {
        let Some(line) = self.line_count().checked_sub(1) else {
            return Pos::default();
        };
        Pos {
            line,
            col: self.line(line).len().saturating_sub(1),
        }
    }

    /// The first line of the block `offset` blocks away from the one holding `line` (negative is backward), clamped
    /// to the first and last blocks.
    fn block_start(&self, line: usize, offset: isize) -> usize {
        let block = self.block_of(line).saturating_add_signed(offset);
        self.starts[block.min(self.blocks.len() - 1)]
    }

    fn block_of(&self, line: usize) -> usize {
        self.starts.partition_point(|&start| start <= line) - 1
    }

    fn reindex(&mut self) {
        self.index = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, block)| (block.key.clone(), i))
            .collect();
        self.starts.clear();
        let mut line = 0;
        for block in &self.blocks {
            self.starts.push(line);
            line += block.lines.len();
        }
        self.starts.push(line);
    }

    /// The number of logical lines.
    fn line_count(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    fn line(&self, line: usize) -> &Shaped {
        let block = self.block_of(line);
        &self.blocks[block].lines[line - self.starts[block]]
    }
}
