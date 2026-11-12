//! A window: a view of a buffer with its own cursor and scroll position, and its own folds.

use std::collections::HashMap;

use ratatui::layout::Rect;

use super::Buffer;
use crate::doc::{Cursor, Folds, Layout, Motion};
use crate::keymap::{Place, Scroll};

/// A view of a buffer. Both the cursor and the top of the view are pinned to text, so neither moves when blocks
/// arrive above them; a view that has never scrolled stays at the top, showing new blocks as they come.
#[derive(Debug, Clone)]
pub(super) struct Window {
    pub(super) buffer: usize,
    pub(super) cursor: Cursor,
    /// Which folds are open here, in whichever buffer: each block's folds are its own.
    pub(super) folds: Folds,
    /// The text at the first row shown.
    top: Cursor,
    /// Where jumps started from, oldest first, in whichever buffer (Vim's jumplist).
    jumps: Vec<Jump>,
    /// The entry `<C-o>` and `<C-i>` last went to; `jumps.len()` when they haven't since the last jump.
    jump: usize,
    /// Where `:open` and `:profile` left for another buffer from, newest last (Vim's tag stack).
    tags: Vec<Jump>,
    /// Where this window left the buffers it showed before, to return there.
    left: HashMap<usize, View>,
}

/// A place in a buffer to go back to.
#[derive(Debug, Clone)]
pub(super) struct Jump {
    pub(super) buffer: usize,
    pub(super) cursor: Cursor,
}

/// Where a window was in a buffer it left: cursor and scroll position.
#[derive(Debug, Clone)]
struct View {
    cursor: Cursor,
    top: Cursor,
}

/// How many positions a jumplist keeps, as in Vim.
const MAX_JUMPS: usize = 100;

/// How many entries the tag stack keeps, as in Vim.
const MAX_TAGS: usize = 20;

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
            folds: Folds::default(),
            top: Cursor::new(),
            jumps: Vec::new(),
            jump: 0,
            tags: Vec::new(),
            left: HashMap::new(),
        }
    }

    /// Shows buffer `buffer` instead, where this window last left it, or with the cursor at `start` when it never
    /// showed it. Not a jump by itself: see [`Window::record_jump`].
    pub(super) fn switch_to(&mut self, buffer: usize, start: Cursor) {
        if buffer == self.buffer {
            return;
        }
        let view = self.left.remove(&buffer).unwrap_or(View {
            cursor: start,
            top: Cursor::new(),
        });
        let leaving = View {
            cursor: std::mem::replace(&mut self.cursor, view.cursor),
            top: std::mem::replace(&mut self.top, view.top),
        };
        self.left.insert(self.buffer, leaving);
        self.buffer = buffer;
    }

    /// The buffer it shows, laid out to fit `area`, its text area, with its folds there.
    pub(super) fn layout<'d>(&self, buffers: &'d [Buffer], area: Rect) -> Layout<'d> {
        self.layout_of(buffers, self.buffer, area)
    }

    /// Buffer `buffer`, laid out to fit `area` as this window shows it, or would: with its folds there.
    pub(super) fn layout_of<'d>(
        &self,
        buffers: &'d [Buffer],
        buffer: usize,
        area: Rect,
    ) -> Layout<'d> {
        let folds = self.folds.clone();
        Layout::within(&buffers[buffer].doc, folds, area.width, area.height)
    }

    /// The jumplist, oldest first, and the entry `<C-o>` and `<C-i>` last went to: past the end when they have not
    /// since the last jump.
    pub(super) fn jumplist(&self) -> (&[Jump], usize) {
        (&self.jumps, self.jump)
    }

    /// Where the cursor is, to come back to.
    pub(super) fn here(&self) -> Jump {
        Jump {
            buffer: self.buffer,
            cursor: self.cursor.clone(),
        }
    }

    /// Remembers `from`, where a jump started in the buffer shown (laid out as `layout`), as the newest jumplist
    /// entry; an older entry on the same row of that buffer goes.
    pub(super) fn record_jump(&mut self, layout: &Layout<'_>, from: Cursor) {
        let row = from.screen(layout).row;
        let buffer = self.buffer;
        self.jumps
            .retain(|jump| jump.buffer != buffer || jump.cursor.screen(layout).row != row);
        self.jumps.push(Jump {
            buffer,
            cursor: from,
        });
        if self.jumps.len() > MAX_JUMPS {
            self.jumps.remove(0);
        }
        self.jump = self.jumps.len();
    }

    /// `<C-o>`: back `count` entries, in whichever buffer. Going back from the newest position first remembers it,
    /// so `<C-i>` can return.
    pub(super) fn jump_older(&mut self, layout: &Layout<'_>, count: usize) {
        if self.jump == self.jumps.len() {
            self.record_jump(layout, self.cursor.clone());
            self.jump = self.jumps.len() - 1;
        }
        let target = self.jump.saturating_sub(count);
        if target != self.jump {
            self.jump = target;
            self.go(self.jumps[target].clone());
        }
    }

    /// `<C-i>`: forward `count` entries, after `<C-o>` went back.
    pub(super) fn jump_newer(&mut self, count: usize) {
        let target = self.jump + count;
        if target < self.jumps.len() {
            self.jump = target;
            self.go(self.jumps[target].clone());
        }
    }

    /// Notes `from` as where the window left for another buffer to follow something, unless it is still there.
    pub(super) fn push_tag(&mut self, from: Jump) {
        if from.buffer == self.buffer {
            return;
        }
        self.tags.push(from);
        if self.tags.len() > MAX_TAGS {
            self.tags.remove(0);
        }
    }

    /// `<C-t>`: back to where the newest entry of the tag stack left from, as a jump (the buffer shown laid out as
    /// `layout`). `false` when the stack is empty.
    pub(super) fn pop_tag(&mut self, layout: &Layout<'_>) -> bool {
        let Some(tag) = self.tags.pop() else {
            return false;
        };
        self.record_jump(layout, self.cursor.clone());
        self.go(tag);
        true
    }

    /// Puts the cursor at `jump`, showing its buffer.
    fn go(&mut self, jump: Jump) {
        self.switch_to(jump.buffer, jump.cursor.clone());
        self.cursor = jump.cursor;
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
