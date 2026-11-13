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

use std::cmp::Reverse;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use cursor::{Cursor, Motion};
pub use layout::{Layout, PictureRow, ScreenPos};
pub use search::{CaseMode, Direction, Pattern, PatternError, SearchOutcome};
pub use visual::{Selection, SelectionKind};

use devnull_api::model::AccountKind;

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
    /// Space for a picture: `rows` empty screen rows whatever the width, `cols` cells wide. The cursor moves through
    /// it like empty lines; the window draws the picture over it (D7).
    Image {
        /// Height in screen rows; 0 counts as 1.
        rows: u16,
        /// Width in cells.
        cols: u16,
        /// Where the picture is, such as its URL.
        source: String,
    },
}

impl Line {
    /// A line of plain text in [`Group::NORMAL`], with no target.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(vec![Span::new(text, Group::NORMAL)])
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
    /// What the block as a whole refers to, such as the commit it shows: the target of its text that has none of its
    /// own, empty lines included.
    pub target: Option<Target>,
}

impl From<Vec<Line>> for Content {
    fn from(lines: Vec<Line>) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }
}

/// A foldable range of a block's lines. The folds of a block nest, as in Vim: two are apart or one holds the other
/// (of two over the same lines, the first holds the second); one that overlaps another otherwise is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    /// The folded lines, as indices into the block's lines.
    pub lines: Range<usize>,
    /// The one row shown in their place while closed; it is cut, not wrapped, at the window's width.
    pub summary: Line,
    /// Whether the fold starts closed; see [`Folds`] for how fold commands change that.
    pub closed: bool,
}

/// What a fold command does to the folds holding the cursor's line, as in Vim, from the outermost in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldOp {
    /// `zo`: opens the first closed fold.
    Open,
    /// `zO`: opens the first closed fold and every fold inside it.
    OpenRecursively,
    /// `zc`: closes the last open fold before the first closed one.
    Close,
    /// `zC`: closes them all.
    CloseRecursively,
    /// `za`: `zo` when one is closed, else `zc`.
    Toggle,
    /// `zA`: `zO` when one is closed, else `zC`.
    ToggleRecursively,
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
    /// Where the picture this span makes space for is, if it does (see [`Span::picture`]).
    pub picture: Option<String>,
}

impl Span {
    /// Text in `group`, with no target.
    pub fn new(text: impl Into<String>, group: Group) -> Self {
        Self {
            text: text.into(),
            group,
            target: None,
            picture: None,
        }
    }

    /// Space for a picture a row tall and `cols` cells wide in a line of text, such as an avatar: blank cells,
    /// over which the window draws the picture at `source`. They are braille blanks, not spaces, so that wrapping
    /// moves them as a word rather than hanging them past the edge as it does spaces.
    pub fn picture(source: impl Into<String>, cols: u16) -> Self {
        Self {
            picture: Some(source.into()),
            ..Self::new("\u{2800}".repeat(usize::from(cols)), Group::NORMAL)
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
    /// A person or an organization.
    Account(AccountRef),
    /// A commit.
    Commit {
        /// Its id.
        id: String,
        /// Who wrote it; `None` for a deleted commit.
        author: Option<AccountRef>,
        /// Its page on the web; `None` for a deleted commit.
        url: Option<String>,
        /// Whether the connected account starred it.
        starred: bool,
    },
    /// The card the server made for a link, opened in the browser like the link.
    LinkCard(LinkCard),
    /// A buffer, by name, such as a profile's list of followers.
    Buffer(String),
}

/// What the server says about a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCard {
    /// The link.
    pub url: String,
    /// The page's title; may be empty.
    pub title: String,
    /// The site's name; may be empty.
    pub site: String,
    /// What the page says it is about; may be empty.
    pub description: String,
}

/// A person or an organization, as text refers to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRef {
    /// The handle, without the `@`.
    pub handle: String,
    /// Whether it is a person or an organization; `None` when the text does not say.
    pub kind: Option<AccountKind>,
}

impl AccountRef {
    /// `handle`, of `kind`; a kind this version does not know counts as not saying.
    pub fn new(handle: impl Into<String>, kind: Option<AccountKind>) -> Self {
        Self {
            handle: handle.into(),
            kind: kind.filter(|kind| *kind != AccountKind::Other),
        }
    }
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

/// Which folds are open, as one window shows them (folds are per window, as in Vim), in whichever [`Document`]s it
/// shows: a block's folds are its own. Those a fold command opened or closed stay so, also when their block is
/// replaced; the others are as [`Fold::closed`] says, as are the folds of a block dropped and added again.
///
/// Clones are cheap: they share the state until one of them changes.
#[derive(Debug, Clone, Default)]
pub struct Folds {
    /// Whether a fold is open, by [`FoldAt::id`].
    open: Rc<HashMap<(u64, usize), bool>>,
}

impl Folds {
    /// Opens or closes folds holding the cursor's line in `doc` as `op` says. The cursor keeps its place in the
    /// text; while a fold hides it, it is drawn on the fold's summary row.
    pub fn fold(&mut self, doc: &Document, cursor: &Cursor, op: FoldOp) {
        let holding = self.holding(doc, cursor);
        let first_closed = holding.iter().position(|fold| self.is_closed(fold));
        let opening = match op {
            FoldOp::Open | FoldOp::OpenRecursively => true,
            FoldOp::Close | FoldOp::CloseRecursively => false,
            FoldOp::Toggle | FoldOp::ToggleRecursively => first_closed.is_some(),
        };
        let recursively = matches!(
            op,
            FoldOp::OpenRecursively | FoldOp::CloseRecursively | FoldOp::ToggleRecursively
        );
        if opening {
            let Some(at) = first_closed else {
                return;
            };
            let inside = recursively
                .then(|| doc.folds_inside(&holding[at]))
                .into_iter()
                .flatten();
            self.set(
                std::iter::once(holding[at].id).chain(inside.map(|fold| fold.id)),
                true,
            );
        } else {
            let open = &holding[..first_closed.unwrap_or(holding.len())];
            let closing = if recursively {
                &holding[..]
            } else {
                open.last().map(std::slice::from_ref).unwrap_or_default()
            };
            self.set(closing.iter().map(|fold| fold.id), false);
        }
    }

    /// Opens every fold hiding the cursor's line in `doc`, as a search landing there does.
    pub fn reveal(&mut self, doc: &Document, cursor: &Cursor) {
        let holding = self.holding(doc, cursor);
        self.set(holding.iter().map(|fold| fold.id), true);
    }

    /// The folds holding the cursor's line in `doc`, the outermost first.
    fn holding<'d>(&self, doc: &'d Document, cursor: &Cursor) -> Vec<FoldAt<'d>> {
        if doc.line_count() == 0 {
            return Vec::new();
        }
        let mut holding: Vec<FoldAt<'d>> = doc.folds_holding(cursor.pos(doc).line).collect();
        holding.sort_by_key(|fold| (Reverse(fold.lines.len()), fold.id.1));
        holding
    }

    /// Leaves the folds `ids` open, or closed.
    fn set(&mut self, ids: impl Iterator<Item = (u64, usize)>, open: bool) {
        let states = Rc::make_mut(&mut self.open);
        states.extend(ids.map(|id| (id, open)));
    }

    /// `zR` (`open`) and `zM`: opens or closes every fold of `doc`.
    pub fn fold_all(&mut self, doc: &Document, open: bool) {
        let states = Rc::make_mut(&mut self.open);
        for block in &doc.blocks {
            states.extend((0..block.folds.len()).map(|index| ((block.id, index), open)));
        }
    }

    fn is_closed(&self, fold: &FoldAt<'_>) -> bool {
        self.open.get(&fold.id).map_or(fold.closed, |open| !open)
    }
}

/// The id of the next block made, in any document.
static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Block {
    /// Unique among the blocks of every document, and kept when the block is replaced: what [`Folds`] knows a block
    /// by, so a block dropped and added again under its key, or one of another document, is another.
    id: u64,
    key: BlockKey,
    lines: Vec<Shaped>,
    folds: Vec<ShapedFold>,
    target: Option<Target>,
}

#[derive(Debug)]
struct ShapedFold {
    lines: Range<usize>,
    summary: Shaped,
    closed: bool,
}

impl Block {
    /// A block with a new id.
    fn new(key: BlockKey, content: Content) -> Self {
        // Only uniqueness matters: no other memory is ordered by the counter.
        let id = NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed);
        Self::with_id(id, key, content)
    }

    fn with_id(id: u64, key: BlockKey, content: Content) -> Self {
        let lines: Vec<Shaped> = content.lines.into_iter().map(Shaped::new).collect();
        let mut folds: Vec<ShapedFold> = Vec::new();
        for fold in content.folds {
            let nests = |kept: &ShapedFold| {
                let (a, b) = (&kept.lines, &fold.lines);
                a.end <= b.start
                    || b.end <= a.start
                    || (a.start <= b.start && b.end <= a.end)
                    || (b.start <= a.start && a.end <= b.end)
            };
            if !fold.lines.is_empty() && fold.lines.end <= lines.len() && folds.iter().all(nests) {
                folds.push(ShapedFold {
                    lines: fold.lines,
                    summary: Shaped::new(fold.summary),
                    closed: fold.closed,
                });
            }
        }
        Self {
            id,
            key,
            lines,
            folds,
            target: content.target,
        }
    }
}

/// A fold located in the document.
struct FoldAt<'d> {
    /// Its block's id and its index there.
    id: (u64, usize),
    /// The folded logical lines, document-wide.
    lines: Range<usize>,
    summary: &'d Shaped,
    /// Whether it is closed until a fold command says otherwise.
    closed: bool,
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
    /// For a picture, the space it takes; text lines wrap instead. On a picture's line a position's column is not a
    /// grapheme (there are none) but which of its rows the cursor is on.
    image: Option<Space>,
}

/// The space a picture takes.
#[derive(Debug)]
struct Space {
    source: String,
    cols: u16,
    /// At least 1.
    rows: u16,
}

/// A span without its text: what styles and targets `text[previous run's end..end]`.
#[derive(Debug)]
struct Run {
    /// Byte offset just past the span's text.
    end: usize,
    group: Group,
    target: Option<Target>,
    /// Where the picture drawn over the span's text is, for a span made by [`Span::picture`].
    picture: Option<String>,
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
            Line::Image { rows, cols, source } => {
                return Self {
                    text: String::new(),
                    graphemes: Vec::new(),
                    runs: Vec::new(),
                    image: Some(Space {
                        source,
                        cols,
                        rows: rows.max(1),
                    }),
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
                picture: span.picture,
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
            image: None,
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
        self.image
            .as_ref()
            .map_or(self.len(), |image| usize::from(image.rows))
            .saturating_sub(1)
    }

    fn is_image(&self) -> bool {
        self.image.is_some()
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

    /// Inserts a block just before the block `before`, or at the end when there is no such block. Cursors stay on
    /// their text.
    ///
    /// # Panics
    ///
    /// If a block with the same key is already present.
    pub fn insert_before(
        &mut self,
        before: impl Into<BlockKey>,
        key: impl Into<BlockKey>,
        content: impl Into<Content>,
    ) {
        let at = self
            .index
            .get(&before.into())
            .copied()
            .unwrap_or(self.blocks.len());
        self.insert(at, key.into(), content.into());
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
        self.blocks[block] = Block::with_id(self.blocks[block].id, key, content.into());
        self.reindex();
        true
    }

    /// Removes the block `key`, if present. Cursors in it move to whatever takes its place.
    pub fn remove(&mut self, key: impl Into<BlockKey>) {
        let key = key.into();
        if let Some(block) = self.index.get(&key).copied() {
            self.blocks.remove(block);
            self.reindex();
        }
    }

    /// Removes every block whose key `keep` says not to keep. Cursors in them move to whatever takes their place.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.blocks.retain(|block| keep(&block.key.0));
        self.reindex();
    }

    /// The folds holding `line`, in no particular order.
    fn folds_holding(&self, line: usize) -> impl Iterator<Item = FoldAt<'_>> {
        self.folds_of(self.block_of(line))
            .filter(move |fold| fold.lines.contains(&line))
    }

    /// The folds inside `outer`, but for itself.
    fn folds_inside<'d>(&'d self, outer: &FoldAt<'_>) -> impl Iterator<Item = FoldAt<'d>> {
        let (id, lines) = (outer.id, outer.lines.clone());
        self.folds_of(self.block_of(lines.start))
            .filter(move |fold| {
                fold.id != id && lines.start <= fold.lines.start && fold.lines.end <= lines.end
            })
    }

    /// The folds of block `block`.
    fn folds_of(&self, block: usize) -> impl Iterator<Item = FoldAt<'_>> {
        let start = self.starts[block];
        let id = self.blocks[block].id;
        self.blocks[block]
            .folds
            .iter()
            .enumerate()
            .map(move |(index, fold)| FoldAt {
                id: (id, index),
                lines: start + fold.lines.start..start + fold.lines.end,
                summary: &fold.summary,
                closed: fold.closed,
            })
    }

    /// The text of line `line` (from 0) of block `key`, if both are there.
    #[must_use]
    pub fn line_text(&self, key: impl Into<BlockKey>, line: usize) -> Option<&str> {
        let block = &self.blocks[*self.index.get(&key.into())?];
        block.lines.get(line).map(|shaped| shaped.text.as_str())
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

    /// What the text at `pos` refers to: its span's target, else its block's.
    fn target_at(&self, pos: Pos) -> Option<&Target> {
        self.line(pos.line)
            .target_at(pos.col)
            .or_else(|| self.block_target_at(pos))
    }

    /// What the block holding `pos` refers to as a whole.
    fn block_target_at(&self, pos: Pos) -> Option<&Target> {
        self.blocks[self.block_of(pos.line)].target.as_ref()
    }
}
