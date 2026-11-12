//! The window layout: a tree of horizontal and vertical splits, and which window has the cursor.

use std::collections::HashMap;

use ratatui::layout::Rect;

use super::window::Window;

/// Identifies a window for as long as it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct WindowId(u64);

/// Which way a split divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    /// `:split`: windows stacked, one above the other.
    Rows,
    /// `:vsplit`: windows side by side, with a separator column between them.
    Columns,
}

/// A direction to move between windows in, as `<C-w>h`, `j`, `k` and `l`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Toward {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Debug, Clone)]
enum Split {
    Leaf(WindowId),
    Node(Axis, Vec<Split>),
}

/// Where everything goes on screen.
#[derive(Debug, Default)]
pub(super) struct Arrangement {
    /// Each window with its area, statusline row included, in tree order.
    pub(super) windows: Vec<(WindowId, Rect)>,
    /// The columns separating side-by-side windows.
    pub(super) separators: Vec<Rect>,
}

/// All open windows and their layout. There is always at least one, and one of them is current.
#[derive(Debug)]
pub(super) struct Windows {
    tree: Split,
    windows: HashMap<WindowId, Window>,
    current: WindowId,
    next_id: u64,
}

impl Windows {
    /// A layout of `window` alone.
    pub(super) fn new(window: Window) -> Self {
        let id = WindowId(0);
        Self {
            tree: Split::Leaf(id),
            windows: HashMap::from([(id, window)]),
            current: id,
            next_id: 1,
        }
    }

    pub(super) fn current_id(&self) -> WindowId {
        self.current
    }

    pub(super) fn current(&self) -> &Window {
        &self.windows[&self.current]
    }

    pub(super) fn current_mut(&mut self) -> &mut Window {
        self.windows
            .get_mut(&self.current)
            .expect("the current window is open")
    }

    pub(super) fn get(&self, id: WindowId) -> &Window {
        &self.windows[&id]
    }

    pub(super) fn count(&self) -> usize {
        self.windows.len()
    }

    /// `:split` and `:vsplit`: a copy of the current window's view goes above or to the left of it and becomes
    /// current.
    pub(super) fn split(&mut self, axis: Axis) {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        // Cloned so the new window starts where the current one is, then moves on its own.
        self.windows.insert(id, self.current().clone());
        self.tree.split(self.current, id, axis);
        self.current = id;
    }

    /// `:close`: closes the current window, giving the cursor to the one before it (or after it, for the first).
    /// Returns `false`, closing nothing, when it is the last window.
    pub(super) fn close(&mut self) -> bool {
        if self.windows.len() == 1 {
            return false;
        }
        let order = self.order();
        let at = order.iter().position(|&id| id == self.current).unwrap_or(0);
        let next = if at > 0 { order[at - 1] } else { order[at + 1] };
        self.tree.remove(self.current);
        self.windows.remove(&self.current);
        self.current = next;
        true
    }

    /// `:only`: closes every window but the current one.
    pub(super) fn only(&mut self) {
        self.tree = Split::Leaf(self.current);
        self.windows.retain(|&id, _| id == self.current);
    }

    /// `<C-w>w` (`forward`) and `<C-w>W`: the next or previous window, wrapping around.
    pub(super) fn cycle(&mut self, forward: bool) {
        let order = self.order();
        let at = order.iter().position(|&id| id == self.current).unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        self.current = order[next];
    }

    /// `<C-w>h` and friends: the nearest window on that side of the current one, laid out in `area`, if any.
    pub(super) fn focus(&mut self, area: Rect, toward: Toward) {
        let arrangement = self.arrange(area);
        let Some(&(_, here)) = arrangement
            .windows
            .iter()
            .find(|(id, _)| *id == self.current)
        else {
            return;
        };
        let rows_overlap = |r: &Rect| r.top() < here.bottom() && here.top() < r.bottom();
        let columns_overlap = |r: &Rect| r.left() < here.right() && here.left() < r.right();
        let distance = |r: &Rect| match toward {
            Toward::Left if rows_overlap(r) && r.right() <= here.left() => {
                Some(here.left() - r.right())
            }
            Toward::Right if rows_overlap(r) && r.left() >= here.right() => {
                Some(r.left() - here.right())
            }
            Toward::Up if columns_overlap(r) && r.bottom() <= here.top() => {
                Some(here.top() - r.bottom())
            }
            Toward::Down if columns_overlap(r) && r.top() >= here.bottom() => {
                Some(r.top() - here.bottom())
            }
            _ => None,
        };
        if let Some((id, _)) = arrangement
            .windows
            .iter()
            .filter_map(|(id, rect)| distance(rect).map(|d| (*id, d)))
            .min_by_key(|&(_, d)| d)
        {
            self.current = id;
        }
    }

    /// Lays the windows out in `area`, sharing space evenly; side-by-side windows lose a column to a separator.
    pub(super) fn arrange(&self, area: Rect) -> Arrangement {
        let mut arrangement = Arrangement::default();
        arrange(&self.tree, area, &mut arrangement);
        arrangement
    }

    /// Window ids in tree order: top to bottom, left to right.
    fn order(&self) -> Vec<WindowId> {
        let mut order = Vec::with_capacity(self.windows.len());
        self.tree.leaves(&mut order);
        order
    }
}

impl Split {
    /// Splits the leaf `target`, putting `new` before it.
    fn split(&mut self, target: WindowId, new: WindowId, axis: Axis) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Node(axis, vec![Self::Leaf(new), Self::Leaf(target)]);
                true
            }
            Self::Leaf(_) => false,
            Self::Node(node_axis, children) => {
                let at = children
                    .iter()
                    .position(|child| matches!(child, Self::Leaf(id) if *id == target));
                if let Some(at) = at
                    && *node_axis == axis
                {
                    children.insert(at, Self::Leaf(new));
                    return true;
                }
                children
                    .iter_mut()
                    .any(|child| child.split(target, new, axis))
            }
        }
    }

    /// Removes the leaf `target`, collapsing splits left with one child.
    fn remove(&mut self, target: WindowId) -> bool {
        let Self::Node(_, children) = self else {
            return false;
        };
        let at = children
            .iter()
            .position(|child| matches!(child, Self::Leaf(id) if *id == target));
        match at {
            Some(at) => {
                children.remove(at);
            }
            None if children.iter_mut().any(|child| child.remove(target)) => {}
            None => return false,
        }
        if children.len() == 1 {
            *self = children.pop().expect("one child left");
        }
        true
    }

    fn leaves(&self, out: &mut Vec<WindowId>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Node(_, children) => children.iter().for_each(|child| child.leaves(out)),
        }
    }
}

fn arrange(split: &Split, area: Rect, out: &mut Arrangement) {
    let (axis, children) = match split {
        Split::Leaf(id) => {
            out.windows.push((*id, area));
            return;
        }
        Split::Node(axis, children) => (*axis, children),
    };
    let count = u16::try_from(children.len()).unwrap_or(u16::MAX).max(1);
    let total = match axis {
        Axis::Rows => area.height,
        Axis::Columns => area.width.saturating_sub(count - 1),
    };
    let (base, extra) = (total / count, total % count);
    let mut offset = 0;
    for (i, child) in (0..count).zip(children) {
        let size = base + u16::from(i < extra);
        let child_area = match axis {
            Axis::Rows => Rect::new(area.x, area.y + offset, area.width, size),
            Axis::Columns => Rect::new(area.x + offset, area.y, size, area.height),
        };
        arrange(child, child_area, out);
        offset += size;
        if axis == Axis::Columns && i + 1 < count {
            out.separators
                .push(Rect::new(area.x + offset, area.y, 1, area.height));
            offset += 1;
        }
    }
}
