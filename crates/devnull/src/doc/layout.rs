use super::{Document, Pos, Shaped};
use crate::hl::Group;

/// A [`Document`] wrapped into screen rows for one width.
///
/// It borrows the document, so it can never describe a stale version of it: rebuild it after an edit or a resize.
#[derive(Debug)]
pub struct Layout<'d> {
    doc: &'d Document,
    width: usize,
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
    /// Whether this row is the summary of a closed fold starting at `line` (with an empty range at 0).
    pub(super) folded: bool,
}

/// A screen row of a picture: which picture, and which of its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureRow<'d> {
    /// Where the picture is.
    pub source: &'d str,
    /// Its width in cells.
    pub cols: u16,
    /// Its height in rows.
    pub rows: u16,
    /// Which of its rows this is, from 0.
    pub row: u16,
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
        let mut line = 0;
        while line < doc.line_count() {
            if let Some(fold) = doc.fold_at(line, true) {
                first_row.extend(fold.lines.clone().map(|_| rows.len()));
                rows.push(Row {
                    line,
                    start: 0,
                    end: 0,
                    folded: true,
                });
                line = fold.lines.end;
                continue;
            }
            first_row.push(rows.len());
            wrap(doc.line(line), width, |start, end| {
                rows.push(Row {
                    line,
                    start,
                    end,
                    folded: false,
                });
            });
            line += 1;
        }
        Self {
            doc,
            width,
            rows,
            first_row,
        }
    }

    pub(super) fn doc(&self) -> &'d Document {
        self.doc
    }

    /// Whether `row` is the summary of a closed fold.
    #[must_use]
    pub fn is_folded(&self, row: usize) -> bool {
        self.rows[row].folded
    }

    /// The picture row `row` shows, if it shows one (not when it is the summary of a fold over it).
    #[must_use]
    pub fn picture(&self, row: usize) -> Option<PictureRow<'d>> {
        let Row {
            line,
            start,
            folded,
            ..
        } = *self.rows.get(row)?;
        if folded {
            return None;
        }
        let image = self.doc.line(line).image.as_ref()?;
        Some(PictureRow {
            source: &image.source,
            cols: image.cols,
            rows: image.rows,
            row: u16::try_from(start).unwrap_or(u16::MAX),
        })
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
        let Row {
            line, start, end, ..
        } = self.rows[row];
        // Empty for an empty line and for every row of an image, whose column is the row itself.
        let graphemes = self
            .doc
            .line(line)
            .graphemes
            .get(start..end)
            .unwrap_or_default();
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
        let Row {
            line,
            start,
            end,
            folded,
        } = self.rows[row];
        let (shaped, from, to) = if let Some(fold) = self.doc.fold_at(line, true).filter(|_| folded)
        {
            let summary = fold.summary;
            (summary, 0, summary.byte_offset(summary.fitting(self.width)))
        } else {
            let shaped = self.doc.line(line);
            (shaped, shaped.byte_offset(start), shaped.byte_offset(end))
        };
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
        let Row {
            line, start, end, ..
        } = self.rows[row];
        let shaped = self.doc.line(line);
        &shaped.text[shaped.byte_offset(start)..shaped.byte_offset(end)]
    }

    /// The first non-blank grapheme of `row`, or its last one when it is all blank.
    pub(super) fn first_non_blank(&self, row: usize) -> Pos {
        let Row {
            line, start, end, ..
        } = self.rows[row];
        let shaped = self.doc.line(line);
        match (start..end).find(|&i| !shaped.is_blank(i)) {
            Some(col) => Pos { line, col },
            None => self.row_end(row),
        }
    }

    /// The last grapheme of `row`, or its start when the row is empty.
    pub(super) fn row_end(&self, row: usize) -> Pos {
        let Row {
            line, start, end, ..
        } = self.rows[row];
        Pos {
            line,
            col: end.saturating_sub(1).max(start),
        }
    }

    /// The display column where grapheme `col` of `row`'s line starts.
    pub(super) fn cell_of(&self, row: usize, col: usize) -> usize {
        let Row {
            line,
            start,
            folded,
            ..
        } = self.rows[row];
        if folded {
            return 0;
        }
        let graphemes = self
            .doc
            .line(line)
            .graphemes
            .get(start..col)
            .unwrap_or_default();
        graphemes.iter().map(|g| g.width).sum()
    }
}

/// Splits a line into rows of at most `width` cells, emitting each row's grapheme range `[start, end)`.
///
/// Rows break after the last whitespace that fits, or mid-word when a word alone is wider than the row. Whitespace
/// that reaches the edge stays on its row, hanging past it, rather than starting the next one. An empty line is one
/// empty row; an image is as many empty rows as it is tall.
fn wrap(line: &Shaped, width: usize, mut emit: impl FnMut(usize, usize)) {
    if let Some(image) = &line.image {
        // Row `i` is the empty range at `i`, so that column `i` lands on it (see `Shaped::image`).
        (0..usize::from(image.rows)).for_each(|i| emit(i, i));
        return;
    }
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
