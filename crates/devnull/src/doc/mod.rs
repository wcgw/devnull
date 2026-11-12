//! The text model behind every buffer (D4).
//!
//! A [`Document`] is a read-only sequence of keyed blocks (one per commit, header, …), each a list of [`Line`]s.
//! A [`Layout`] wraps it into screen rows for a width, and a [`Cursor`] moves over those rows with Vim motions
//! while staying on the same text when blocks are inserted or replaced and when the width changes.

mod cursor;
mod layout;
#[cfg(test)]
mod tests;
mod word;

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use cursor::{Cursor, Motion};
pub use layout::{Layout, ScreenPos};

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
}

impl Line {
    /// A line of plain text in [`Group::Normal`], with no target.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(vec![Span::new(text, Group::Normal)])
    }
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
}

#[derive(Debug)]
struct Block {
    key: BlockKey,
    lines: Vec<Shaped>,
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
        let Line::Text(spans) = line;
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
    pub fn push_back(&mut self, key: impl Into<BlockKey>, lines: Vec<Line>) {
        self.insert(self.blocks.len(), key.into(), lines);
    }

    /// Inserts a block at the start, as when newer commits arrive. Cursors stay on their text.
    ///
    /// # Panics
    ///
    /// If a block with the same key is already present.
    pub fn push_front(&mut self, key: impl Into<BlockKey>, lines: Vec<Line>) {
        self.insert(0, key.into(), lines);
    }

    fn insert(&mut self, at: usize, key: BlockKey, lines: Vec<Line>) {
        assert!(
            !self.index.contains_key(&key),
            "duplicate block key {key:?}"
        );
        let lines = lines.into_iter().map(Shaped::new).collect();
        self.blocks.insert(at, Block { key, lines });
        self.reindex();
    }

    /// Replaces the lines of the block `key`, keeping its place. Cursors in it stay on the same line and character,
    /// clamped to the new text. Returns whether the block was there; nothing changes when it wasn't.
    pub fn replace(&mut self, key: impl Into<BlockKey>, lines: Vec<Line>) -> bool {
        let Some(&block) = self.index.get(&key.into()) else {
            return false;
        };
        self.blocks[block].lines = lines.into_iter().map(Shaped::new).collect();
        self.reindex();
        true
    }

    /// Removes the block `key`, if present. Cursors in it move to whatever takes its place.
    pub fn remove(&mut self, key: impl Into<BlockKey>) {
        if let Some(block) = self.index.get(&key.into()).copied() {
            self.blocks.remove(block);
            self.reindex();
        }
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
        let len = self.line(line).len();
        Pos {
            line,
            col: anchor.col.min(len.saturating_sub(1)),
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
