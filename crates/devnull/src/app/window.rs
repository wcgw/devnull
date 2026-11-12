//! A window: a view of a buffer with its own cursor and scroll position.

use crate::doc::{Cursor, Layout, Motion};
use crate::keymap::{Place, Scroll};

/// A view of a buffer. Both the cursor and the top of the view are pinned to text, so neither moves when blocks
/// arrive above them; a view that has never scrolled stays at the top, showing new blocks as they come.
#[derive(Debug, Clone)]
pub(super) struct Window {
    pub(super) buffer: usize,
    pub(super) cursor: Cursor,
    /// The text at the first row shown.
    top: Cursor,
}

/// How a window is shown: its height in rows, and the context (`'scrolloff'`) kept around the cursor.
#[derive(Debug, Clone, Copy)]
pub(super) struct Viewport {
    pub(super) height: usize,
    pub(super) scrolloff: usize,
}

impl Viewport {
    fn height(self) -> usize {
        self.height.max(1)
    }

    /// `'scrolloff'`, limited so it can always be honoured, as in Vim.
    fn scrolloff(self) -> usize {
        self.scrolloff.min((self.height() - 1) / 2)
    }
}

impl Window {
    pub(super) fn new(buffer: usize) -> Self {
        Self {
            buffer,
            cursor: Cursor::new(),
            top: Cursor::new(),
        }
    }

    /// The first row to show: where the view was, moved as little as possible to keep the cursor in view with
    /// `'scrolloff'` rows around it, without scrolling past the end for context alone.
    pub(super) fn top(&self, layout: &Layout<'_>, viewport: Viewport) -> usize {
        let (height, scrolloff) = (viewport.height(), viewport.scrolloff());
        let top = self.top.screen(layout).row;
        let cursor = self.cursor.screen(layout).row;
        if cursor < top + scrolloff {
            cursor.saturating_sub(scrolloff)
        } else if cursor + scrolloff >= top + height {
            let with_context = cursor + scrolloff + 1 - height;
            let last_page = layout.row_count().saturating_sub(height);
            with_context
                .min(last_page)
                .max((cursor + 1).saturating_sub(height))
        } else {
            top
        }
    }

    /// Keeps the cursor in view after it moved, pinning the view to new text only when it had to scroll.
    pub(super) fn follow_cursor(&mut self, layout: &Layout<'_>, viewport: Viewport) {
        let top = self.top(layout, viewport);
        self.set_top(layout, top);
    }

    fn set_top(&mut self, layout: &Layout<'_>, row: usize) {
        if row != self.top.screen(layout).row {
            self.top = Cursor::at_row(layout, row);
        }
    }

    /// `<C-e>` and friends: moves the view, then the cursor only as far as it must to stay in view.
    pub(super) fn scroll(
        &mut self,
        layout: &Layout<'_>,
        viewport: Viewport,
        scroll: Scroll,
        count: usize,
    ) {
        let height = viewport.height();
        let top = self.top(layout, viewport);
        let last = layout.row_count().saturating_sub(1);
        let last_page = layout.row_count().saturating_sub(height);
        let half = (height / 2).max(1);
        let page = height.saturating_sub(2).max(1);
        let new_top = match scroll {
            Scroll::LineDown => (top + count).min(last),
            Scroll::LineUp => top.saturating_sub(count),
            Scroll::HalfDown => (top + half).min(last_page.max(top)),
            Scroll::HalfUp => top.saturating_sub(half),
            Scroll::PageDown => (top + page * count).min(last),
            Scroll::PageUp => top.saturating_sub(page * count),
        };
        self.set_top(layout, new_top);
        match scroll {
            Scroll::HalfDown => self.cursor.apply(layout, Motion::Down, half),
            Scroll::HalfUp => self.cursor.apply(layout, Motion::Up, half),
            Scroll::PageDown => self.cursor.apply(layout, Motion::GotoRow(new_top + 1), 1),
            Scroll::PageUp => {
                let bottom = (new_top + height - 1).min(last);
                self.cursor.apply(layout, Motion::GotoRow(bottom + 1), 1);
            }
            Scroll::LineDown | Scroll::LineUp => {}
        }
        let scrolloff = viewport.scrolloff();
        let (row, top) = (self.cursor.screen(layout).row, self.top.screen(layout).row);
        let (first, last_shown) = (
            top + if top > 0 { scrolloff } else { 0 },
            top + height - 1 - scrolloff,
        );
        if row < first {
            self.cursor.apply(layout, Motion::Down, first - row);
        } else if row > last_shown {
            self.cursor.apply(layout, Motion::Up, row - last_shown);
        }
    }

    /// `zt`, `zz`, `zb`.
    pub(super) fn place(&mut self, layout: &Layout<'_>, viewport: Viewport, place: Place) {
        let (height, scrolloff) = (viewport.height(), viewport.scrolloff());
        let row = self.cursor.screen(layout).row;
        let top = match place {
            Place::Top => row.saturating_sub(scrolloff),
            Place::Middle => row.saturating_sub((height - 1) / 2),
            Place::Bottom => (row + 1 + scrolloff).saturating_sub(height),
        };
        self.set_top(layout, top);
    }

    /// `H`, `M`, `L`: `count` rows in from the top or bottom (1 is the edge itself), and at least `'scrolloff'`
    /// rows in unless that edge is the buffer's.
    pub(super) fn view_row(
        &mut self,
        layout: &Layout<'_>,
        viewport: Viewport,
        place: Place,
        count: usize,
    ) {
        let top = self.top(layout, viewport);
        let shown = viewport
            .height()
            .min(layout.row_count().saturating_sub(top))
            .max(1);
        let scrolloff = viewport.scrolloff();
        let offset = count.max(1) - 1;
        let row = match place {
            Place::Top => {
                top + offset
                    .max(if top > 0 { scrolloff } else { 0 })
                    .min(shown - 1)
            }
            Place::Middle => top + (shown - 1) / 2,
            Place::Bottom => {
                let at_end = top + shown >= layout.row_count();
                top + (shown - 1).saturating_sub(offset.max(if at_end { 0 } else { scrolloff }))
            }
        };
        self.cursor.apply(layout, Motion::GotoRow(row + 1), 1);
        self.follow_cursor(layout, viewport);
    }
}
