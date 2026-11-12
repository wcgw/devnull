//! The application core (D10): state and a pure `update`; the runtime does the I/O.

mod view;
mod window;

#[cfg(test)]
mod tests;

use ratatui::layout::Rect;

use crate::doc::{Document, Layout, Line};
use crate::ex::{self, Command};
use crate::key::Key;
use crate::keymap::{Action, Invocation, Keymap, Mode};

use window::Window;

/// Everything `dn` knows, and the only place that decides what happens next.
#[derive(Debug)]
pub struct App {
    /// The terminal's size in cells, as (width, height).
    size: (u16, u16),
    focused: bool,
    keymap: Keymap,
    buffers: Vec<Buffer>,
    window: Window,
}

/// Something that happened, for [`App::update`] to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// A key was pressed.
    Key(Key),
    /// Text was pasted (bracketed paste). It never runs commands.
    Paste(String),
    /// The terminal gained focus.
    FocusGained,
    /// The terminal lost focus.
    FocusLost,
    /// The terminal has this size, in cells; sent at startup and on every resize.
    Resize {
        /// Columns.
        width: u16,
        /// Rows.
        height: u16,
    },
}

/// Something [`App::update`] asks the runtime to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Restore the terminal and exit.
    Quit,
}

/// A named document, such as a log or a thread.
#[derive(Debug)]
struct Buffer {
    name: String,
    doc: Document,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// A fresh session showing the intro.
    #[must_use]
    pub fn new() -> Self {
        Self::with_buffer("intro", intro())
    }

    /// A fresh session showing `doc` as a buffer called `name`.
    #[must_use]
    pub fn with_buffer(name: impl Into<String>, doc: Document) -> Self {
        Self {
            size: (80, 24),
            focused: true,
            keymap: Keymap::new(),
            buffers: vec![Buffer {
                name: name.into(),
                doc,
            }],
            window: Window::new(0),
        }
    }

    /// The terminal's size in cells, as (width, height), as last told by [`Msg::Resize`].
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.size
    }

    /// Whether the terminal has focus, which slows polling down when it doesn't.
    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Reacts to `msg`, returning what the runtime must do.
    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(key) => {
                let ran = self.keymap.feed(Mode::Normal, key);
                ran.into_iter()
                    .flat_map(|invocation| self.run(invocation))
                    .collect()
            }
            Msg::Paste(_) => Vec::new(),
            Msg::FocusGained => {
                self.focused = true;
                Vec::new()
            }
            Msg::FocusLost => {
                self.focused = false;
                Vec::new()
            }
            Msg::Resize { width, height } => {
                self.size = (width, height);
                Vec::new()
            }
        }
    }

    fn run(&mut self, invocation: Invocation) -> Vec<Effect> {
        let count = invocation.count.unwrap_or(1);
        let area = self.window_area();
        let height = usize::from(area.height);
        let window = &mut self.window;
        let layout = Layout::new(&self.buffers[window.buffer].doc, area.width);
        match invocation.action {
            Action::Motion(motion) => {
                window.cursor.apply(&layout, motion, count);
                window.follow_cursor(&layout, height);
            }
            Action::Scroll(scroll) => window.scroll(&layout, height, scroll, count),
            Action::Place(place) => window.place(&layout, height, place),
            Action::ViewRow(place) => window.view_row(&layout, height, place, count),
            Action::Ex(line) => return self.ex(&line),
            _ => {}
        }
        Vec::new()
    }

    /// The document of the current window, for tests to change as the network would.
    #[cfg(test)]
    pub(crate) fn doc_mut(&mut self) -> &mut Document {
        &mut self.buffers[self.window.buffer].doc
    }

    fn ex(&mut self, line: &str) -> Vec<Effect> {
        match ex::parse(line) {
            Ok(Some(Command::Quit { .. })) => vec![Effect::Quit],
            _ => Vec::new(),
        }
    }

    /// Where the window's text goes: below the bufferline, above its statusline and the command line.
    fn window_area(&self) -> Rect {
        let (width, height) = self.size;
        Rect::new(0, 1, width, height.saturating_sub(3))
    }
}

/// What `dn` shows before anything is loaded, like Vim's intro screen.
fn intro() -> Document {
    let mut doc = Document::new();
    let lines = [
        format!("dn v{}", env!("CARGO_PKG_VERSION")),
        "a Neovim-flavoured client for dev-null.sh".to_owned(),
        String::new(),
        "type  q  to exit".to_owned(),
    ];
    doc.push_back(
        "intro",
        lines.into_iter().map(Line::text).collect::<Vec<_>>(),
    );
    doc
}
