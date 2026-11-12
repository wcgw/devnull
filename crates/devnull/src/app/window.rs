//! A window: a view of a buffer with its own cursor and scroll position.

use crate::doc::{Cursor, Layout, Motion};
use crate::keymap::{Place, Scroll};

/// A view of a buffer. Both the cursor and the top of the view are pinned to text, so neither moves when blocks
/// arrive above them; a view that has never scrolled stays at the top, showing new blocks as they come.
#[derive(Debug)]
pub(super) struct Window {
    pub(super) buffer: usize,
    pub(super) cursor: Cursor,
    /// The text at the first row shown.
    top: Cursor,
}

impl Window {
    pub(super) fn new(buffer: usize) -> Self {
        Self {
            buffer,
            cursor: Cursor::new(),
            top: Cursor::new(),
        }
    }

    /// The first row to show in a window `height` rows tall: where the view was, moved as little as possible to
    /// keep the cursor in view.
    pub(super) fn top(&self, layout: &Layout<'_>, height: usize) -> usize {
        let top = self.top.screen(layout).row;
        let cursor = self.cursor.screen(layout).row;
        let height = height.max(1);
        if cursor < top {
            cursor
        } else if cursor >= top + height {
            cursor + 1 - height
        } else {
            top
        }
    }

    /// Keeps the cursor in view after it moved, pinning the view to new text only when it had to scroll.
    pub(super) fn follow_cursor(&mut self, layout: &Layout<'_>, height: usize) {
        let top = self.top(layout, height);
        self.set_top(layout, top);
    }

    fn set_top(&mut self, layout: &Layout<'_>, row: usize) {
        if row != self.top.screen(layout).row {
            self.top = Cursor::at_row(layout, row);
        }
    }

    /// `<C-e>` and friends.
    pub(super) fn scroll(
        &mut self,
        layout: &Layout<'_>,
        height: usize,
        scroll: Scroll,
        count: usize,
    ) {
        let height = height.max(1);
        let top = self.top(layout, height);
        let last = layout.row_count().saturating_sub(1);
        let last_page = layout.row_count().saturating_sub(height);
        let half = (height / 2).max(1);
        let page = height.saturating_sub(2).max(1);
        let (new_top, cursor_rows) = match scroll {
            Scroll::LineDown => ((top + count).min(last), 0),
            Scroll::LineUp => (top.saturating_sub(count), 0),
            Scroll::HalfDown => ((top + half).min(last_page.max(top)), half),
            Scroll::HalfUp => (top.saturating_sub(half), half),
            Scroll::PageDown => ((top + page * count).min(last), 0),
            Scroll::PageUp => (top.saturating_sub(page * count), 0),
        };
        self.set_top(layout, new_top);
        match scroll {
            Scroll::HalfDown => self.cursor.apply(layout, Motion::Down, cursor_rows),
            Scroll::HalfUp => self.cursor.apply(layout, Motion::Up, cursor_rows),
            Scroll::PageDown => self.cursor.apply(layout, Motion::GotoRow(new_top + 1), 1),
            Scroll::PageUp => {
                let bottom = (new_top + height - 1).min(last);
                self.cursor.apply(layout, Motion::GotoRow(bottom + 1), 1);
            }
            Scroll::LineDown | Scroll::LineUp => {}
        }
        // Push the cursor back into view, keeping its column, when the view left it behind.
        let row = self.cursor.screen(layout).row;
        let new_top = self.top.screen(layout).row;
        if row < new_top {
            self.cursor.apply(layout, Motion::Down, new_top - row);
        } else if row >= new_top + height {
            self.cursor
                .apply(layout, Motion::Up, row + 1 - (new_top + height));
        }
    }

    /// `zt`, `zz`, `zb`.
    pub(super) fn place(&mut self, layout: &Layout<'_>, height: usize, place: Place) {
        let height = height.max(1);
        let row = self.cursor.screen(layout).row;
        let top = match place {
            Place::Top => row,
            Place::Middle => row.saturating_sub((height - 1) / 2),
            Place::Bottom => (row + 1).saturating_sub(height),
        };
        self.set_top(layout, top);
    }

    /// `H`, `M`, `L`: `count` rows in from the top or bottom (1 is the edge itself).
    pub(super) fn view_row(
        &mut self,
        layout: &Layout<'_>,
        height: usize,
        place: Place,
        count: usize,
    ) {
        let top = self.top(layout, height);
        let shown = height
            .max(1)
            .min(layout.row_count().saturating_sub(top))
            .max(1);
        let offset = count.max(1) - 1;
        let row = match place {
            Place::Top => top + offset.min(shown - 1),
            Place::Middle => top + (shown - 1) / 2,
            Place::Bottom => top + (shown - 1).saturating_sub(offset),
        };
        self.cursor.apply(layout, Motion::GotoRow(row + 1), 1);
    }
}
