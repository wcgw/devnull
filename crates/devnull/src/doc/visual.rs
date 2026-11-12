//! Visual mode: a selection from where `v` or `V` was pressed to the cursor, and the text `y` yanks from it.

use std::ops::Range;

use super::{Anchor, Cursor, Document, Layout, Pos};

/// One end of a visual selection, fixed where `v` or `V` was pressed; the cursor is the other end.
///
/// The fixed end is anchored like the cursor, so the selection keeps covering the same text when blocks are inserted
/// or replaced around it.
#[derive(Debug, Clone)]
pub struct Selection {
    /// The fixed end; `None` only in an empty document.
    anchor: Option<Anchor>,
    kind: SelectionKind,
}

/// What a selection covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    /// `v`: from one character to another, both included.
    Charwise,
    /// `V`: whole screen rows.
    Rowwise,
}

/// The selected text as `[start, end)` logical positions.
struct Span {
    start: Pos,
    end: Pos,
}

impl Selection {
    /// Starts a selection at `cursor`, pinned to the text there even if the cursor has never moved.
    #[must_use]
    pub fn start(cursor: &Cursor, layout: &Layout<'_>, kind: SelectionKind) -> Self {
        let doc = layout.doc();
        let anchor = (layout.row_count() > 0).then(|| doc.anchor(cursor.pos(doc)));
        Self { anchor, kind }
    }

    /// The text `y` yanks: rows of one wrapped line join without a break, lines are separated by `\n`, and a
    /// rowwise selection ends with `\n`, as a linewise register does in Vim.
    #[must_use]
    pub fn text(&self, cursor: &Cursor, layout: &Layout<'_>) -> String {
        if layout.row_count() == 0 {
            return String::new();
        }
        let doc = layout.doc();
        let Span { start, end } = self.span(cursor, layout);
        let mut text = String::new();
        for line in start.line..=end.line {
            let shaped = doc.line(line);
            let from = if line == start.line { start.col } else { 0 }.min(shaped.len());
            let to = if line == end.line {
                end.col
            } else {
                shaped.len()
            }
            .min(shaped.len());
            if line > start.line {
                text.push('\n');
            }
            text.push_str(&shaped.text[shaped.byte_offset(from)..shaped.byte_offset(to.max(from))]);
        }
        if self.kind == SelectionKind::Rowwise {
            text.push('\n');
        }
        text
    }

    /// The display cells to highlight on `row`, or `None` when the selection does not reach it. A selected empty
    /// row highlights one cell, as in Vim.
    #[must_use]
    pub fn cells(&self, cursor: &Cursor, layout: &Layout<'_>, row: usize) -> Option<Range<usize>> {
        if layout.row_count() == 0 {
            return None;
        }
        let doc = layout.doc();
        let (first, last) = self.ends(cursor, doc);
        let (first_row, last_row) = (layout.row_of(first), layout.row_of(last));
        if !(first_row..=last_row).contains(&row) {
            return None;
        }
        let whole = self.kind == SelectionKind::Rowwise;
        let left = if row == first_row && !whole {
            layout.cell_of(row, first.col)
        } else {
            0
        };
        let right = if row == last_row && !whole {
            let width = doc
                .line(last.line)
                .graphemes
                .get(last.col)
                .map_or(1, |g| g.width.max(1));
            layout.cell_of(row, last.col) + width
        } else {
            layout.cell_of(row, layout.row(row).end)
        };
        Some(left..right.max(left + 1))
    }

    /// The selected positions in document order: the first and last character for a charwise selection, any
    /// position on the first and last row for a rowwise one.
    fn ends(&self, cursor: &Cursor, doc: &Document) -> (Pos, Pos) {
        let fixed = self
            .anchor
            .as_ref()
            .map_or_else(Pos::default, |anchor| doc.resolve(anchor));
        let moving = cursor.pos(doc);
        (fixed.min(moving), fixed.max(moving))
    }

    fn span(&self, cursor: &Cursor, layout: &Layout<'_>) -> Span {
        let (first, last) = self.ends(cursor, layout.doc());
        match self.kind {
            SelectionKind::Charwise => Span {
                start: first,
                end: Pos {
                    col: last.col + 1,
                    ..last
                },
            },
            SelectionKind::Rowwise => {
                let (top, bottom) = (
                    layout.row(layout.row_of(first)),
                    layout.row(layout.row_of(last)),
                );
                Span {
                    start: Pos {
                        line: top.line,
                        col: top.start,
                    },
                    end: Pos {
                        line: bottom.line,
                        col: bottom.end,
                    },
                }
            }
        }
    }
}
