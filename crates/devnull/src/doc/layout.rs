use super::{Document, Pos, Shaped};
use crate::hl::Group;

/// A [`Document`] wrapped into screen rows for one width.
///
/// It borrows the document, so it can never describe a stale version of it: rebuild it after an edit or a resize.
#[derive(Debug)]
pub struct Layout<'d> {
    doc: &'d Document,
    rows: Vec<Row>,
    /// The first row of each logical line.
    first_row: Vec<usize>,
}

/// A screen row: a grapheme range of one logical line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Row {
    pub(super) line: usize,
    /// First grapheme, inclusive.
    pub(super) start: usize,
    /// Last grapheme, exclusive. Equal to `start` only for an empty line.
    pub(super) end: usize,
}

/// A cell on screen, relative to the top-left of the whole laid-out document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPos {
    /// Row, from 0.
    pub row: usize,
    /// Display column in terminal cells, from 0.
    pub col: usize,
}

impl<'d> Layout<'d> {
    /// Wraps `doc` to `width` cells. A width of 0 is treated as 1.
    #[must_use]
    pub fn new(doc: &'d Document, width: u16) -> Self {
        let width = usize::from(width.max(1));
        let mut rows = Vec::new();
        let mut first_row = Vec::with_capacity(doc.line_count());
        for line in 0..doc.line_count() {
            first_row.push(rows.len());
            wrap(doc.line(line), width, |start, end| {
                rows.push(Row { line, start, end });
            });
        }
        Self {
            doc,
            rows,
            first_row,
        }
    }

    pub(super) fn doc(&self) -> &'d Document {
        self.doc
    }

    /// The number of screen rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn row(&self, row: usize) -> Row {
        self.rows[row]
    }

    /// The row showing `pos`.
    pub(super) fn row_of(&self, pos: Pos) -> usize {
        let first = self.first_row[pos.line];
        let mut row = first;
        while row + 1 < self.rows.len()
            && self.rows[row + 1].line == pos.line
            && self.rows[row + 1].start <= pos.col
        {
            row += 1;
        }
        row
    }

    /// The grapheme of `row` covering display column `cell`, or its last one when the row is narrower.
    pub(super) fn pos_at(&self, row: usize, cell: usize) -> Pos {
        let Row { line, start, end } = self.rows[row];
        let graphemes = &self.doc.line(line).graphemes[start..end];
        let mut left = 0;
        for (i, g) in graphemes.iter().enumerate() {
            if cell < left + g.width.max(1) {
                return Pos {
                    line,
                    col: start + i,
                };
            }
            left += g.width.max(1);
        }
        self.row_end(row)
    }

    /// The text shown on `row`, left to right, as runs of one highlight group. Spaces hanging off the row's edge
    /// are included; an empty row yields nothing.
    pub fn segments(&self, row: usize) -> impl Iterator<Item = (&'d str, Group)> + use<'d> {
        let Row { line, start, end } = self.rows[row];
        let shaped = self.doc.line(line);
        let (from, to) = (shaped.byte_offset(start), shaped.byte_offset(end));
        let mut run_start = 0;
        shaped.runs.iter().filter_map(move |run| {
            let (lo, hi) = (run_start.max(from), run.end.min(to));
            run_start = run.end;
            (lo < hi).then(|| (&shaped.text[lo..hi], run.group))
        })
    }

    /// The text shown on `row`, including spaces hanging off its edge.
    #[cfg(test)]
    pub(crate) fn row_text(&self, row: usize) -> &str {
        let Row { line, start, end } = self.rows[row];
        let shaped = self.doc.line(line);
        &shaped.text[shaped.byte_offset(start)..shaped.byte_offset(end)]
    }

    /// The first non-blank grapheme of `row`, or its last one when it is all blank.
    pub(super) fn first_non_blank(&self, row: usize) -> Pos {
        let Row { line, start, end } = self.rows[row];
        let shaped = self.doc.line(line);
        match (start..end).find(|&i| !shaped.is_blank(i)) {
            Some(col) => Pos { line, col },
            None => self.row_end(row),
        }
    }

    /// The last grapheme of `row`, or its start when the row is empty.
    pub(super) fn row_end(&self, row: usize) -> Pos {
        let Row { line, start, end } = self.rows[row];
        Pos {
            line,
            col: end.saturating_sub(1).max(start),
        }
    }

    /// The display column where grapheme `col` of `row`'s line starts.
    pub(super) fn cell_of(&self, row: usize, col: usize) -> usize {
        let Row { line, start, .. } = self.rows[row];
        self.doc.line(line).graphemes[start..col]
            .iter()
            .map(|g| g.width)
            .sum()
    }
}

/// Splits a line into rows of at most `width` cells, emitting each row's grapheme range `[start, end)`.
///
/// Rows break after the last whitespace that fits, or mid-word when a word alone is wider than the row. Whitespace
/// that reaches the edge stays on its row, hanging past it, rather than starting the next one. An empty line is one
/// empty row.
fn wrap(line: &Shaped, width: usize, mut emit: impl FnMut(usize, usize)) {
    let mut start = 0;
    let mut used = 0;
    // Where a row may end: just after the most recent whitespace of the current row.
    let mut breakable = None;
    for (i, g) in line.graphemes.iter().enumerate() {
        let blank = line.is_blank(i);
        if used + g.width > width && i > start && !blank {
            let end = breakable.filter(|&b| b > start).unwrap_or(i);
            emit(start, end);
            used = line.graphemes[end..i].iter().map(|g| g.width).sum();
            start = end;
            breakable = None;
        }
        used += g.width;
        if blank {
            breakable = Some(i + 1);
        }
    }
    emit(start, line.len());
}
