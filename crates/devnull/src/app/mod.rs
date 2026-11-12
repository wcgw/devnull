//! The application core (D10): state and a pure `update`; the runtime does the I/O.

mod actions;
mod cmdline;
mod composer;
mod connection;
mod feed;
mod fetch;
mod hover;
mod logs;
mod options;
mod picker;
mod pictures;
mod profiles;
mod threads;
mod view;
mod window;
mod windows;

#[cfg(test)]
mod tests;

use std::time::Duration;

use ratatui::layout::Rect;

use crate::doc::{
    Cursor, Direction, Document, FoldOp, Layout, LinkCard, Motion, Pattern, SearchOutcome,
    Selection, SelectionKind, Target,
};
use crate::ex::{self, Command};
use crate::hl::Theme;
use crate::key::Key;
use crate::keymap::{Action, Invocation, Keymap, Mode};

use actions::Confirm;
use cmdline::{Cmdline, Edit, Purpose};
use connection::Connection;
pub use connection::NetEvent;
use feed::{Feed, Source};
pub use fetch::{FetchError, Fetched, Request};
use hover::Hover;
use options::Options;
use picker::Picker;
pub use pictures::Prepared;
use window::{Viewport, Window};
use windows::{Axis, Toward, Windows};

/// Everything `dn` knows, and the only place that decides what happens next.
#[derive(Debug)]
pub struct App {
    /// The terminal's size in cells, as (width, height).
    size: (u16, u16),
    focused: bool,
    keymap: Keymap,
    options: Options,
    theme: Theme,
    buffers: Vec<Buffer>,
    windows: Windows,
    mode: AppMode,
    /// The message shown in the command line until the next key, if any.
    message: Option<Message>,
    /// Every message shown, oldest first, for `:messages`.
    messages: Vec<Message>,
    /// Command lines run, oldest first.
    ex_history: Vec<String>,
    /// Patterns searched for, oldest first.
    search_history: Vec<String>,
    last_search: Option<LastSearch>,
    connection: Connection,
    /// Counts key presses, so a [`Wake`] asked for before the latest key is recognised as stale.
    generation: u64,
    /// Whether the which-key popup is showing.
    which_key: bool,
    /// The card `K` shows, until the next key.
    hover: Option<Hover>,
    /// Whether the logs are being polled.
    polling: bool,
    /// The generation of poll wake-ups, apart from keys so that typing does not stop polling.
    poll_generation: u64,
    /// The generation of search wake-ups: the user picker's query changed this many times.
    search_generation: u64,
    /// Buffers in the order they were last shown, the latest first.
    recent: Vec<usize>,
    /// Notifications unread, as the server last counted them.
    unread: u64,
    /// The draft being written, from opening the editor until it is published or kept.
    composing: Option<composer::Composing>,
    /// Drafts not published, to open again.
    drafts: composer::Drafts,
    /// The pictures asked for.
    pictures: pictures::Pictures,
}

/// How long pending keys wait before the which-key popup shows what can follow them.
const WHICH_KEY_DELAY: Duration = Duration::from_millis(200);

/// A token the runtime hands back as [`Msg::Wake`] once the delay of an [`Effect::Wake`] has passed. It is only
/// meaningful to the app that asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wake {
    purpose: WakePurpose,
    /// The key press it belongs to; any later key makes it stale.
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WakePurpose {
    /// `'timeoutlen'` passed on an ambiguous key sequence.
    KeyTimeout,
    /// Time to show the which-key popup.
    WhichKey,
    /// Time to check the logs for new commits.
    Poll,
    /// Typing paused in the user picker: time to search.
    Search,
}

/// Something that happened, for [`App::update`] to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// A key was pressed.
    Key(Key),
    /// Text was pasted (bracketed paste). It never runs commands; on the command line it is inserted.
    Paste(String),
    /// The editor opened by [`Effect::Edit`] closed: what it saved, or why there is nothing (it failed, or quit
    /// with an error as Vim's `:cq` does).
    Edited(Result<String, String>),
    /// A picture asked for by [`Effect::Picture`], prepared as asked, or why it could not be.
    Picture {
        /// Where it is.
        source: String,
        /// Its width in cells.
        cols: u16,
        /// Its height in cells.
        rows: u16,
        /// The picture, or why there is none.
        result: Result<Prepared, String>,
    },
    /// The terminal gained focus.
    FocusGained,
    /// The terminal lost focus.
    FocusLost,
    /// The delay of an [`Effect::Wake`] has passed.
    Wake(Wake),
    /// News about the connection.
    Net(NetEvent),
    /// The answer to an [`Effect::Fetch`], and when it arrived (Unix seconds), for ages.
    Fetched {
        /// The answer.
        fetched: Fetched,
        /// When it arrived.
        now: u64,
    },
    /// A file to run as ex commands, such as `init.vim` at startup or what `:source` asked for, or why it could not
    /// be read.
    Sourced {
        /// The file, as named to the user.
        path: String,
        /// Its text, or the reason it could not be read.
        contents: Result<String, String>,
    },
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
    /// Put this text on the system clipboard (OSC 52).
    Copy(String),
    /// Read this file (`~` is the home directory) and send its contents back as [`Msg::Sourced`].
    Source(String),
    /// Connect with the device key, reporting back as [`Msg::Net`].
    Connect,
    /// Decrypt the device key with this passphrase, then connect.
    Unlock(String),
    /// Approve this device in the browser: use the device key, creating it if there is none.
    Enroll,
    /// Finish approval with the callback link the browser ended on.
    EnrollPasted(String),
    /// Accept the terms of this version.
    AcceptTerms(String),
    /// Download the picture at `source` and prepare it for `cols` × `rows` cells, then send [`Msg::Picture`]: for the
    /// kitty protocol as image `kitty` when given, as half blocks otherwise.
    Picture {
        /// Where it is.
        source: String,
        /// Its width in cells.
        cols: u16,
        /// Its height in cells.
        rows: u16,
        /// Its id in the terminal, for the kitty protocol.
        kitty: Option<u32>,
    },
    /// Write these graphics protocol escapes to the terminal, as they are.
    Graphics(String),
    /// Hand the terminal to the user's editor on `text`, then send [`Msg::Edited`].
    Edit {
        /// What the editor opens.
        text: String,
    },
    /// Open this link in the browser.
    OpenUrl(String),
    /// Ask the server, sending the answer back as [`Msg::Fetched`].
    Fetch(Request),
    /// Send [`Msg::Wake`] with `token` once `after` has passed.
    Wake {
        /// How long to wait.
        after: Duration,
        /// What to send back.
        token: Wake,
    },
}

/// A named document, such as a log or a thread.
#[derive(Debug)]
struct Buffer {
    name: String,
    doc: Document,
    kind: Kind,
}

/// What a buffer shows, and so where its document comes from.
#[derive(Debug)]
enum Kind {
    /// A page saying where something stands, such as `connect`; unlike the others, `:ls`, `:b` and the bufferline
    /// leave it out.
    Status,
    /// A document given whole, as to [`App::with_buffer`].
    Fixed,
    /// What dev-null.sh sends, such as a log or a thread.
    Feed(Feed),
}

impl Buffer {
    /// Whether `:ls`, `:b` and the bufferline show it.
    fn is_listed(&self) -> bool {
        !matches!(self.kind, Kind::Status)
    }
}

/// What keys are for right now.
#[derive(Debug)]
enum AppMode {
    Normal,
    /// Selecting, from the selection's fixed end to the cursor.
    Visual(Selection),
    /// Typing after `:`, `/` or `?`.
    Cmdline(Cmdline),
    /// Lines shown over the screen (messages, or a listing such as `:ls`) until a key continues.
    MorePrompt {
        lines: Vec<Message>,
    },
    /// A picker over the windows, taking keys until something is picked or it is left.
    Picker(Picker),
    /// A question in the command line, answered by the next key.
    Confirm(Confirm),
}

/// A line for the command line area.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Message {
    text: String,
    kind: MessageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageKind {
    Info,
    Warning,
    Error,
}

/// The last search, for `n` and `N`.
#[derive(Debug, Clone)]
struct LastSearch {
    pattern: Pattern,
    direction: Direction,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// A fresh session, connecting: [`App::start`] says how.
    #[must_use]
    pub fn new() -> Self {
        let doc = Connection::Checking.document().unwrap_or_default();
        let mut app = Self::with_buffer("connect", doc);
        app.buffers[0].kind = Kind::Status;
        app
    }

    /// A fresh session showing `doc` as a buffer called `name`.
    #[must_use]
    pub fn with_buffer(name: impl Into<String>, doc: Document) -> Self {
        Self {
            size: (80, 24),
            focused: true,
            keymap: Keymap::new(),
            options: Options::default(),
            theme: Theme::default(),
            buffers: vec![Buffer {
                name: name.into(),
                doc,
                kind: Kind::Fixed,
            }],
            windows: Windows::new(Window::new(0)),
            mode: AppMode::Normal,
            message: None,
            messages: Vec::new(),
            ex_history: Vec::new(),
            search_history: Vec::new(),
            last_search: None,
            connection: Connection::Checking,
            generation: 0,
            which_key: false,
            hover: None,
            polling: false,
            poll_generation: 0,
            search_generation: 0,
            recent: Vec::new(),
            unread: 0,
            composing: None,
            drafts: composer::Drafts::new(),
            pictures: pictures::Pictures::default(),
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
        let mut effects = self.react(msg);
        if !effects.contains(&Effect::Quit) {
            effects.extend(self.wanted_pictures());
        }
        effects
    }

    fn react(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(key) => {
                self.generation += 1;
                self.which_key = false;
                let mut effects = self.key(key);
                effects.extend(self.wake_ups());
                effects.extend(self.load_more());
                effects
            }
            Msg::Wake(token) if token.generation != self.generation_of(token.purpose) => Vec::new(),
            Msg::Wake(Wake { purpose, .. }) => match purpose {
                WakePurpose::KeyTimeout => {
                    self.which_key = false;
                    let ran = self.keymap.timeout(self.keymap_mode());
                    self.run_all(ran)
                }
                WakePurpose::WhichKey => {
                    self.which_key = !self.keymap.pending_keys().is_empty();
                    Vec::new()
                }
                WakePurpose::Poll => self.poll(),
                WakePurpose::Search => self.search_people(),
            },
            Msg::Paste(text) => match &mut self.mode {
                AppMode::Cmdline(cmdline) => {
                    cmdline.paste(&text);
                    Vec::new()
                }
                AppMode::Picker(_) => self.picker_paste(&text),
                _ => Vec::new(),
            },
            Msg::Sourced { path, contents } => self.source(&path, contents),
            Msg::Edited(saved) => self.edited(saved),
            Msg::Picture {
                source,
                cols,
                rows,
                result,
            } => self.picture_arrived(source, cols, rows, result),
            Msg::Net(event) => self.net(event),
            Msg::Fetched { fetched, now } => self.fetched(fetched, now),
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

    fn key(&mut self, key: Key) -> Vec<Effect> {
        match &mut self.mode {
            AppMode::Normal | AppMode::Visual(_) => {
                self.message = None;
                // As Neovim's hover window, the card goes with the next key, which also does its usual job.
                self.hover = None;
                let ran = self.keymap.feed(self.keymap_mode(), key);
                self.run_all(ran)
            }
            AppMode::MorePrompt { .. } => {
                self.mode = AppMode::Normal;
                // As in Vim, <CR>, <Space> and <Esc> only dismiss; any other key also does its usual job.
                let dismiss_only = [
                    Key::new(crate::key::Code::Enter),
                    Key::char(' '),
                    Key::new(crate::key::Code::Esc),
                ];
                if dismiss_only.contains(&key) {
                    Vec::new()
                } else {
                    self.key(key)
                }
            }
            AppMode::Picker(_) => self.picker_key(key),
            AppMode::Confirm(_) => self.confirm_key(key),
            AppMode::Cmdline(cmdline) => {
                let history: &[String] = match cmdline.purpose {
                    Purpose::Ex => &self.ex_history,
                    Purpose::Search(_) => &self.search_history,
                    Purpose::Passphrase => &[],
                };
                match cmdline.key(key, history) {
                    Edit::Editing => Vec::new(),
                    Edit::Cancel => {
                        self.mode = AppMode::Normal;
                        Vec::new()
                    }
                    Edit::Submit(text) => {
                        let AppMode::Cmdline(cmdline) =
                            std::mem::replace(&mut self.mode, AppMode::Normal)
                        else {
                            unreachable!("the mode was just matched as the command line");
                        };
                        match cmdline.purpose {
                            Purpose::Ex => {
                                remember(&mut self.ex_history, &text);
                                self.ex(&text)
                            }
                            Purpose::Search(direction) => {
                                remember(&mut self.search_history, &text);
                                self.search(&text, direction, cmdline.count.unwrap_or(1));
                                Vec::new()
                            }
                            Purpose::Passphrase => vec![Effect::Unlock(text)],
                        }
                    }
                }
            }
        }
    }

    fn run_all(&mut self, ran: Vec<Invocation>) -> Vec<Effect> {
        ran.into_iter()
            .flat_map(|invocation| self.run(invocation))
            .collect()
    }

    /// The wake-ups pending keys need: the which-key popup, and the timeout of an ambiguous sequence.
    fn wake_ups(&self) -> Vec<Effect> {
        let in_keymap_mode = matches!(self.mode, AppMode::Normal | AppMode::Visual(_));
        if !in_keymap_mode || self.keymap.pending_keys().is_empty() {
            return Vec::new();
        }
        let wake = |purpose| Wake {
            purpose,
            generation: self.generation,
        };
        let mut wakes = vec![Effect::Wake {
            after: WHICH_KEY_DELAY,
            token: wake(WakePurpose::WhichKey),
        }];
        if self.keymap.is_ambiguous() {
            wakes.push(Effect::Wake {
                after: Duration::from_millis(self.options.timeoutlen),
                token: wake(WakePurpose::KeyTimeout),
            });
        }
        wakes
    }

    /// The current generation of wake-ups for `purpose`: tokens of an older one are stale.
    fn generation_of(&self, purpose: WakePurpose) -> u64 {
        match purpose {
            WakePurpose::KeyTimeout | WakePurpose::WhichKey => self.generation,
            WakePurpose::Poll => self.poll_generation,
            WakePurpose::Search => self.search_generation,
        }
    }

    /// Shows buffer `index` in the current window, where the window last left it, and loads what it shows unless
    /// it has it or is getting it.
    fn show_buffer(&mut self, index: usize) -> Vec<Effect> {
        self.show_buffer_at(index, Cursor::new())
    }

    /// As [`App::show_buffer`], starting at `start` when the window never showed the buffer. Showing another buffer
    /// is a jump, as starting to edit another file is in Vim, unless it is from a status page such as `connect`.
    fn show_buffer_at(&mut self, index: usize, start: Cursor) -> Vec<Effect> {
        let area = self.window_area();
        let window = self.windows.current_mut();
        let shown = &self.buffers[window.buffer];
        if index != window.buffer && shown.is_listed() {
            let layout = Layout::new(&shown.doc, area.width);
            window.record_jump(&layout, window.cursor.clone());
        }
        window.switch_to(index, start);
        self.visited(index);
        self.load(index)
    }

    /// Notes buffer `index` as the last shown.
    fn visited(&mut self, index: usize) {
        self.recent.retain(|&recent| recent != index);
        self.recent.insert(0, index);
    }

    /// After the current window jumped from buffer `before`: keeps its cursor in view, and loads the buffer it
    /// jumped to, if it is another, unless it has what it shows or is getting it.
    fn jumped(&mut self, before: usize) -> Vec<Effect> {
        self.follow_cursor();
        let now = self.windows.current().buffer;
        if now == before {
            Vec::new()
        } else {
            self.visited(now);
            self.load(now)
        }
    }

    /// `:pop` (`<C-t>`): back to where the window was when `:open` or `:profile` last left for another buffer.
    fn pop_tag(&mut self) -> Result<Vec<Effect>, String> {
        let area = self.window_area();
        let window = self.windows.current_mut();
        let before = window.buffer;
        let layout = Layout::new(&self.buffers[before].doc, area.width);
        if !window.pop_tag(&layout) {
            return Err("E73: Tag stack empty".to_owned());
        }
        Ok(self.jumped(before))
    }

    /// The listed buffers' indices, in order: what `:ls`, `:b` and the bufferline count.
    fn listed(&self) -> Vec<usize> {
        (0..self.buffers.len())
            .filter(|&index| self.buffers[index].is_listed())
            .collect()
    }

    /// The alternate buffer, as Vim calls it: the listed buffer shown last before the current one.
    fn alternate(&self) -> Option<usize> {
        let current = self.windows.current().buffer;
        self.recent
            .iter()
            .copied()
            .find(|&index| index != current && self.buffers[index].is_listed())
    }

    /// `:ls`: the listed buffers, the current one marked `%a` and the alternate one `#`.
    fn list_buffers(&mut self) {
        let current = self.windows.current().buffer;
        let alternate = self.alternate();
        let lines = self
            .listed()
            .into_iter()
            .enumerate()
            .map(|(number, index)| Message {
                text: format!(
                    "{:>3} {:2} \"{}\"",
                    number + 1,
                    if index == current {
                        "%a"
                    } else if Some(index) == alternate {
                        "#"
                    } else {
                        ""
                    },
                    self.buffers[index].name
                ),
                kind: MessageKind::Info,
            })
            .collect();
        self.mode = AppMode::MorePrompt { lines };
    }

    /// `:b {arg}`: the alternate buffer for `#`, else the listed buffer numbered `arg`, else named exactly `arg`, else the only one whose name contains
    /// it.
    fn buffer_named(&self, arg: &str) -> Result<usize, String> {
        if arg == "#" {
            return self
                .alternate()
                .ok_or_else(|| "E23: No alternate file".to_owned());
        }
        let listed = self.listed();
        if let Ok(number) = arg.parse::<usize>() {
            return listed
                .get(number.wrapping_sub(1))
                .copied()
                .ok_or_else(|| format!("E86: Buffer {number} does not exist"));
        }
        if let Some(&index) = listed
            .iter()
            .find(|&&index| self.buffers[index].name == arg)
        {
            return Ok(index);
        }
        let matching: Vec<usize> = listed
            .into_iter()
            .filter(|&index| self.buffers[index].name.contains(arg))
            .collect();
        match matching.as_slice() {
            [index] => Ok(*index),
            [] => Err(format!("E94: No matching buffer for {arg}")),
            _ => Err(format!("E93: More than one match for {arg}")),
        }
    }

    /// `:bn` and `:bp`: the next or previous listed buffer, wrapping around.
    fn cycle_buffer(&mut self, forward: bool) -> Vec<Effect> {
        let listed = self.listed();
        if listed.is_empty() {
            return Vec::new();
        }
        let current = self.windows.current().buffer;
        let at = listed
            .iter()
            .position(|&index| index == current)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % listed.len()
        } else {
            (at + listed.len() - 1) % listed.len()
        };
        self.show_buffer(listed[next])
    }

    /// The keymap table for the current mode.
    fn keymap_mode(&self) -> Mode {
        match self.mode {
            AppMode::Visual(_) => Mode::Visual,
            _ => Mode::Normal,
        }
    }

    fn run(&mut self, invocation: Invocation) -> Vec<Effect> {
        match invocation.action {
            Action::Visual(kind) => {
                self.visual(kind);
                return Vec::new();
            }
            Action::Yank => return self.yank(),
            Action::Escape => {
                self.mode = AppMode::Normal;
                return Vec::new();
            }
            Action::Fold(op) => {
                let doc = &mut self.buffers[self.windows.current().buffer].doc;
                doc.fold(&self.windows.current().cursor, op);
            }
            Action::FoldAll { open } => self.buffers[self.windows.current().buffer]
                .doc
                .fold_all(open),
            _ => return self.run_in_window(invocation),
        }
        self.follow_cursor();
        Vec::new()
    }

    /// `v` and `V`: start Visual mode, switch its kind, or leave it when it already is that kind.
    fn visual(&mut self, kind: SelectionKind) {
        self.mode = match std::mem::replace(&mut self.mode, AppMode::Normal) {
            AppMode::Visual(selection) if selection.kind() == kind => AppMode::Normal,
            AppMode::Visual(selection) => AppMode::Visual(selection.with_kind(kind)),
            _ => {
                let area = self.window_area();
                let layout =
                    Layout::new(&self.buffers[self.windows.current().buffer].doc, area.width);
                AppMode::Visual(Selection::start(
                    &self.windows.current().cursor,
                    &layout,
                    kind,
                ))
            }
        };
    }

    /// `y` in Visual mode and `yy` in Normal mode: copies the text and, as Vim does, leaves the cursor at its start.
    fn yank(&mut self) -> Vec<Effect> {
        let area = self.window_area();
        let layout = Layout::new(&self.buffers[self.windows.current().buffer].doc, area.width);
        let cursor = &mut self.windows.current_mut().cursor;
        let selection = match std::mem::replace(&mut self.mode, AppMode::Normal) {
            AppMode::Visual(selection) => selection,
            _ => Selection::start(cursor, &layout, SelectionKind::Rowwise),
        };
        let text = selection.text(cursor, &layout);
        *cursor = selection.start_cursor(cursor, &layout);
        let lines = text.trim_end_matches('\n').matches('\n').count() + 1;
        // Vim reports yanks of more lines than 'report', which is 2 by default.
        if lines > 2 {
            self.info(format!("{lines} lines yanked"));
        }
        self.follow_cursor();
        vec![Effect::Copy(text)]
    }

    /// Keeps the cursor of the current window in view.
    fn follow_cursor(&mut self) {
        let viewport = self.viewport();
        let area = self.window_area();
        let layout = Layout::new(&self.buffers[self.windows.current().buffer].doc, area.width);
        self.windows.current_mut().follow_cursor(&layout, viewport);
    }

    /// Opens the fold hiding the cursor, as Vim's `'foldopen'` does after a search.
    fn open_fold_at_cursor(&mut self) {
        self.buffers[self.windows.current().buffer]
            .doc
            .fold(&self.windows.current().cursor, FoldOp::Open);
        self.follow_cursor();
    }

    fn run_in_window(&mut self, invocation: Invocation) -> Vec<Effect> {
        let count = invocation.count.unwrap_or(1);
        let viewport = self.viewport();
        let area = self.window_area();
        let window = self.windows.current_mut();
        let layout = Layout::new(&self.buffers[window.buffer].doc, area.width);
        // Searches and these motions are jumps: where they start goes into the jumplist, as in Vim.
        let from = window.cursor.clone();
        match invocation.action {
            Action::Motion(motion) => {
                window.cursor.apply(&layout, motion, count);
                if is_jump(motion) {
                    window.record_jump(&layout, from);
                }
                window.follow_cursor(&layout, viewport);
            }
            Action::JumpOlder => {
                let before = window.buffer;
                window.jump_older(&layout, count);
                return self.jumped(before);
            }
            Action::JumpNewer => {
                let before = window.buffer;
                window.jump_newer(count);
                return self.jumped(before);
            }
            Action::Scroll(scroll) => window.scroll(&layout, viewport, scroll, count),
            Action::Place(place) => window.place(&layout, viewport, place),
            Action::ViewRow(place) => {
                window.view_row(&layout, viewport, place, count);
                window.record_jump(&layout, from);
            }
            Action::SearchWord(direction) => {
                let case = self.options.case_mode();
                if let Some((pattern, outcome)) =
                    window.cursor.search_word(&layout, direction, count, case)
                {
                    window.record_jump(&layout, from);
                    self.last_search = Some(LastSearch { pattern, direction });
                    self.open_fold_at_cursor();
                    self.report_search(outcome, direction, "");
                } else {
                    self.error("E348: No string under cursor");
                }
            }
            Action::SearchNext { reverse } => match self.last_search.clone() {
                Some(LastSearch { pattern, direction }) => {
                    let direction = if reverse {
                        opposite(direction)
                    } else {
                        direction
                    };
                    let outcome = window.cursor.search(&layout, &pattern, direction, count);
                    if outcome != SearchOutcome::NotFound {
                        window.record_jump(&layout, from);
                    }
                    self.open_fold_at_cursor();
                    self.report_search(outcome, direction, "");
                }
                None => self.error("E35: No previous regular expression"),
            },
            Action::Cmdline(kind) => {
                self.mode = AppMode::Cmdline(Cmdline::new(kind.into(), invocation.count))
            }
            Action::Keys { keys, remap } => {
                return keys
                    .into_iter()
                    .flat_map(|key| self.replay(key, remap))
                    .collect();
            }
            Action::Ex(line) => return self.ex(&line),
            Action::Error(text) => self.error(text),
            _ => {}
        }
        Vec::new()
    }

    /// Feeds one key of an [`Action::Keys`] to whatever mode is current now.
    fn replay(&mut self, key: Key, remap: bool) -> Vec<Effect> {
        match self.mode {
            AppMode::Normal | AppMode::Visual(_) => {
                let ran = self.keymap.replay(self.keymap_mode(), &[key], remap);
                self.run_all(ran)
            }
            AppMode::Cmdline(_)
            | AppMode::MorePrompt { .. }
            | AppMode::Picker(_)
            | AppMode::Confirm(_) => self.key(key),
        }
    }

    /// Runs an ex command line, reporting its error in the message area.
    fn ex(&mut self, line: &str) -> Vec<Effect> {
        match self.run_line(line) {
            Ok(effects) => effects,
            Err(error) => {
                self.error(error);
                Vec::new()
            }
        }
    }

    /// Parses and runs one command line; blank lines and comments do nothing.
    fn run_line(&mut self, line: &str) -> Result<Vec<Effect>, String> {
        match ex::parse(line) {
            Ok(Some(command)) => self.execute(command),
            Ok(None) => Ok(Vec::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn execute(&mut self, command: Command) -> Result<Vec<Effect>, String> {
        match command {
            Command::Quit { all: false, .. } if self.windows.count() > 1 => {
                self.windows.close();
            }
            Command::Quit { .. } => {
                let mut effects = self.free_pictures();
                effects.push(Effect::Quit);
                return Ok(effects);
            }
            Command::Split { vertical } => {
                self.windows
                    .split(if vertical { Axis::Columns } else { Axis::Rows })
            }
            Command::Close => self.close_window()?,
            Command::Only => self.windows.only(),
            Command::Wincmd(ch) => return self.wincmd(ch),
            Command::Set(args) => {
                let graphics = self.options.graphics;
                for arg in &args {
                    if let Some(shown) = self.options.set(arg, &mut self.keymap)? {
                        self.info(shown);
                    }
                }
                if self.options.graphics != graphics {
                    // Drawn otherwise now: what was sent goes, and pictures are asked for again.
                    return Ok(self.pictures.clear());
                }
            }
            Command::Map {
                modes,
                lhs,
                rhs,
                remap,
            } => {
                for mode in modes {
                    self.keymap.map(mode, &lhs, &rhs, remap);
                }
            }
            Command::Unmap { modes, lhs } => {
                let mut removed = false;
                for mode in modes {
                    removed |= self.keymap.unmap(mode, &lhs);
                }
                if !removed {
                    return Err("E31: No such mapping".to_owned());
                }
            }
            // Like Vim, nothing to show means no prompt either.
            Command::Messages if self.messages.is_empty() => {}
            Command::Messages => {
                self.mode = AppMode::MorePrompt {
                    lines: self.messages.clone(),
                };
            }
            Command::Highlight { group, attrs } => {
                if let Some(shown) = self.theme.highlight(&group, &attrs)? {
                    self.info(shown);
                }
            }
            Command::HighlightClear => self.theme.clear(),
            Command::Colorscheme(None) => self.info(self.theme.name().to_owned()),
            Command::Colorscheme(Some(name)) => self.theme = Theme::named(&name)?,
            Command::Source(path) => return Ok(vec![Effect::Source(path)]),
            Command::Connect(link) => return self.connect(link),
            Command::Edit(None) => {
                return self
                    .refresh(self.windows.current().buffer)
                    .ok_or_else(|| "Nothing to reload here".to_owned());
            }
            Command::Buffers => self.list_buffers(),
            Command::Buffer(arg) => {
                let index = self.buffer_named(&arg)?;
                return Ok(self.show_buffer(index));
            }
            Command::NextBuffer { forward } => return Ok(self.cycle_buffer(forward)),
            Command::Edit(Some(name)) => return self.edit(&name),
            Command::Open => return self.following(Self::open),
            Command::Profile(handle) => return self.following(|app| app.profile(handle)),
            Command::Pop => return self.pop_tag(),
            Command::Pick { list, query } => return Ok(self.pick(list, &query)),
            Command::Star => return self.star(),
            Command::Commit => return self.compose(composer::Answering::Nothing),
            Command::Reply => return self.compose(composer::Answering::Reply(String::new())),
            Command::Quote => return self.compose(composer::Answering::Quote(String::new())),
            Command::Delete => return self.delete(),
            Command::Follow { handle, on } => return self.follow(handle, on),
            Command::Browse { link } => return self.browse(link),
            Command::YankLink => return self.yank_link(),
            Command::Hover => return self.hover(),
            Command::Agree => match &self.connection {
                Connection::Terms(me) => {
                    return Ok(vec![Effect::AcceptTerms(
                        me.terms_version.clone().unwrap_or_default(),
                    )]);
                }
                _ => return Err("No terms to agree to".to_owned()),
            },
        }
        Ok(Vec::new())
    }

    /// What the text under the current window's cursor refers to, if anything.
    fn target_under_cursor(&self) -> Option<Target> {
        let window = self.windows.current();
        let layout = Layout::new(&self.buffers[window.buffer].doc, self.window_area().width);
        window.cursor.target(&layout).cloned()
    }

    /// Runs `follow` (`:open`, `:profile`), noting where the window was on the tag stack if it shows another buffer
    /// after.
    fn following(
        &mut self,
        follow: impl FnOnce(&mut Self) -> Result<Vec<Effect>, String>,
    ) -> Result<Vec<Effect>, String> {
        let from = self.windows.current().here();
        let outcome = follow(self);
        self.windows.current_mut().push_tag(from);
        outcome
    }

    /// `:open`: what the text under the cursor refers to: a link opens in the browser, a commit's thread, a profile
    /// or another buffer in the window.
    fn open(&mut self) -> Result<Vec<Effect>, String> {
        match self.target_under_cursor() {
            Some(Target::Url(url) | Target::LinkCard(LinkCard { url, .. })) => {
                Ok(vec![Effect::OpenUrl(url)])
            }
            Some(Target::Commit { id, .. }) => Ok(self.open_thread(&id)),
            Some(Target::Account(account)) => Ok(self.open_profile(account)),
            Some(Target::Buffer(name)) => self.edit(&name),
            None => Ok(Vec::new()),
        }
    }

    /// The web page of what the cursor is on: of the commit (a quote's, else the one holding the cursor), else of
    /// the account.
    fn web_page(&self) -> Option<String> {
        let window = self.windows.current();
        let layout = Layout::new(&self.buffers[window.buffer].doc, self.window_area().width);
        let targets = [
            window.cursor.target(&layout),
            window.cursor.block_target(&layout),
        ];
        let commit = targets.iter().flatten().find_map(|target| match target {
            Target::Commit { url: Some(url), .. } => Some(url.clone()),
            _ => None,
        });
        commit.or_else(|| {
            targets.iter().flatten().find_map(|target| match target {
                Target::Account(account) => {
                    Some(format!("{}/@{}", devnull_api::ORIGIN, account.handle))
                }
                _ => None,
            })
        })
    }

    /// `:browse`: the commit or account under the cursor on the web; `:browse link`, the link under it.
    fn browse(&mut self, link: bool) -> Result<Vec<Effect>, String> {
        let url = if link {
            match self.target_under_cursor() {
                Some(Target::Url(url) | Target::LinkCard(LinkCard { url, .. })) => url,
                _ => return Err("No link under the cursor".to_owned()),
            }
        } else {
            self.web_page()
                .ok_or_else(|| "No commit or account under the cursor".to_owned())?
        };
        Ok(vec![Effect::OpenUrl(url)])
    }

    /// `:yanklink`: the link `:browse` opens, to the clipboard.
    fn yank_link(&mut self) -> Result<Vec<Effect>, String> {
        let url = self
            .web_page()
            .ok_or_else(|| "No commit or account under the cursor".to_owned())?;
        self.info(format!("Yanked {url}"));
        Ok(vec![Effect::Copy(url)])
    }

    /// `:e {name}`: shows the buffer of that name, or else the feed the name names (see [`Source::named`]), or
    /// with `profile`, the connected account's profile.
    fn edit(&mut self, name: &str) -> Result<Vec<Effect>, String> {
        if let Some(index) = self.buffers.iter().position(|buffer| buffer.name == name) {
            return Ok(self.show_buffer(index));
        }
        let source = if name == "profile" {
            self.own_profile()
        } else {
            Source::named(name)
        };
        match source {
            Some(source) => Ok(self.open_feed(source, Cursor::new())),
            None => Err(format!("E94: No matching buffer for {name}")),
        }
    }

    /// Runs the lines of a sourced file, carrying on after errors and then listing them as Vim does.
    fn source(&mut self, path: &str, contents: Result<String, String>) -> Vec<Effect> {
        let Ok(contents) = contents else {
            self.error(format!("E484: Can't open file {path}"));
            return Vec::new();
        };
        let mut effects = Vec::new();
        let mut errors = Vec::new();
        for (number, line) in (1..).zip(contents.lines()) {
            match self.run_line(line) {
                Ok(more) => effects.extend(more),
                Err(error) => errors.push((number, error)),
            }
        }
        if !errors.is_empty() {
            let from = self.messages.len();
            self.error(format!("Error detected while processing {path}:"));
            for (number, error) in errors {
                self.info(format!("line {number}:"));
                self.error(error);
            }
            self.mode = AppMode::MorePrompt {
                lines: self.messages[from..].to_vec(),
            };
        }
        effects
    }

    fn close_window(&mut self) -> Result<(), String> {
        if self.windows.close() {
            Ok(())
        } else {
            Err("E444: Cannot close last window".to_owned())
        }
    }

    /// `:wincmd {ch}`, which `<C-w>{ch}` runs.
    fn wincmd(&mut self, ch: char) -> Result<Vec<Effect>, String> {
        let area = self.windows_area();
        match ch {
            'h' => self.windows.focus(area, Toward::Left),
            'j' => self.windows.focus(area, Toward::Down),
            'k' => self.windows.focus(area, Toward::Up),
            'l' => self.windows.focus(area, Toward::Right),
            'w' => self.windows.cycle(true),
            'W' => self.windows.cycle(false),
            's' => self.windows.split(Axis::Rows),
            'v' => self.windows.split(Axis::Columns),
            'c' => self.close_window()?,
            'o' => self.windows.only(),
            'q' => return self.run_line("quit"),
            _ => {}
        }
        Ok(Vec::new())
    }

    /// `/` and `?` submitted with `text`; an empty pattern repeats the last one.
    fn search(&mut self, text: &str, direction: Direction, count: usize) {
        let pattern = if text.is_empty() {
            match &self.last_search {
                Some(last) => last.pattern.clone(),
                None => return self.error("E35: No previous regular expression"),
            }
        } else {
            match Pattern::new(text, self.options.case_mode()) {
                Ok(pattern) => pattern,
                Err(error) => return self.error(error.to_string()),
            }
        };
        let area = self.window_area();
        let window = self.windows.current_mut();
        let layout = Layout::new(&self.buffers[window.buffer].doc, area.width);
        let from = window.cursor.clone();
        let outcome = window.cursor.search(&layout, &pattern, direction, count);
        if outcome != SearchOutcome::NotFound {
            window.record_jump(&layout, from);
        }
        self.last_search = Some(LastSearch { pattern, direction });
        self.open_fold_at_cursor();
        self.report_search(outcome, direction, text);
    }

    fn report_search(&mut self, outcome: SearchOutcome, direction: Direction, text: &str) {
        match (outcome, direction) {
            (SearchOutcome::Found, _) => {}
            (SearchOutcome::Wrapped, Direction::Forward) => {
                self.warn("search hit BOTTOM, continuing at TOP")
            }
            (SearchOutcome::Wrapped, Direction::Backward) => {
                self.warn("search hit TOP, continuing at BOTTOM")
            }
            (SearchOutcome::NotFound, _) => self.error(format!("E486: Pattern not found: {text}")),
        }
    }

    fn info(&mut self, text: impl Into<String>) {
        self.say(text.into(), MessageKind::Info);
    }

    fn warn(&mut self, text: impl Into<String>) {
        self.say(text.into(), MessageKind::Warning);
    }

    fn error(&mut self, text: impl Into<String>) {
        self.say(text.into(), MessageKind::Error);
    }

    fn say(&mut self, text: String, kind: MessageKind) {
        let message = Message { text, kind };
        self.messages.push(message.clone());
        self.message = Some(message);
    }

    /// Where windows go: between the bufferline and the command line.
    fn windows_area(&self) -> Rect {
        let (width, height) = self.size;
        Rect::new(0, 1, width, height.saturating_sub(2))
    }

    /// Where the current window's text goes: its share of the windows area, above its statusline.
    fn window_area(&self) -> Rect {
        let current = self.windows.current_id();
        let arrangement = self.windows.arrange(self.windows_area());
        let rect = arrangement
            .windows
            .into_iter()
            .find(|(id, _)| *id == current)
            .map(|(_, rect)| rect);
        text_area(rect.unwrap_or_default())
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            height: usize::from(self.window_area().height),
            scrolloff: self.options.scrolloff,
        }
    }

    /// The effects to start with, once the runtime is ready: connecting.
    pub fn start(&mut self) -> Vec<Effect> {
        vec![Effect::Connect]
    }

    fn net(&mut self, event: NetEvent) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.connection = match event {
            NetEvent::Error(text) => {
                self.error(text);
                return effects;
            }
            NetEvent::Connected(me) if !me.terms_accepted => Connection::Terms(me),
            NetEvent::Connected(me) => {
                self.unread = me.unread_notifications;
                Connection::Connected(me)
            }
            NetEvent::NoKey => Connection::NoKey,
            NetEvent::NotApproved => Connection::NotApproved,
            NetEvent::Failed(reason) => Connection::Failed(reason),
            NetEvent::NeedsPassphrase { wrong } => {
                self.mode = AppMode::Cmdline(Cmdline::new(Purpose::Passphrase, None));
                Connection::NeedsPassphrase { wrong }
            }
            NetEvent::Enrolling { url, fingerprint } => {
                effects.push(Effect::OpenUrl(url.clone()));
                Connection::Enrolling { url, fingerprint }
            }
        };
        effects.extend(self.show_connection());
        effects
    }

    /// Shows the `connect` buffer while not connected, and the following log once connected, asking for it if it
    /// was never loaded.
    fn show_connection(&mut self) -> Vec<Effect> {
        let (index, mut effects) = match self.connection.document() {
            Some(doc) => {
                let index = match self
                    .buffers
                    .iter()
                    .position(|buffer| buffer.name == "connect")
                {
                    Some(index) => index,
                    None => {
                        self.buffers.push(Buffer {
                            name: "connect".to_owned(),
                            doc: Document::new(),
                            kind: Kind::Status,
                        });
                        self.buffers.len() - 1
                    }
                };
                self.buffers[index].doc = doc;
                (index, Vec::new())
            }
            None => (
                self.feed_buffer_or_new(Source::Log(devnull_api::Log::Following)),
                self.start_polling(),
            ),
        };
        effects.extend(self.show_buffer(index));
        effects
    }

    /// `:connect`, whose meaning depends on where connecting stands.
    fn connect(&mut self, link: Option<String>) -> Result<Vec<Effect>, String> {
        Ok(match (&self.connection, link) {
            (Connection::Enrolling { .. }, Some(link)) => vec![Effect::EnrollPasted(link)],
            (_, Some(_)) => return Err("No approval in progress: :connect first".to_owned()),
            (Connection::NoKey | Connection::NotApproved | Connection::Enrolling { .. }, None) => {
                vec![Effect::Enroll]
            }
            (Connection::Connected(me), None) => {
                self.info(format!("Connected as @{}", me.handle));
                Vec::new()
            }
            (_, None) => vec![Effect::Connect],
        })
    }

    /// How `group` is drawn now.
    #[cfg(test)]
    pub(crate) fn style(&self, group: crate::hl::Group) -> ratatui::style::Style {
        self.theme.style(group)
    }

    /// The document of the current window, for tests to change as the network would.
    #[cfg(test)]
    pub(crate) fn doc_mut(&mut self) -> &mut Document {
        &mut self.buffers[self.windows.current().buffer].doc
    }
}

/// Whether `motion` is a jump, remembered in the jumplist.
fn is_jump(motion: Motion) -> bool {
    matches!(
        motion,
        Motion::GotoRow(_)
            | Motion::LastRow
            | Motion::ParagraphForward
            | Motion::ParagraphBackward
            | Motion::NextBlock
            | Motion::PrevBlock
    )
}

/// A window's text area: all of `rect` but its last row, the statusline.
fn text_area(rect: Rect) -> Rect {
    Rect {
        height: rect.height.saturating_sub(1),
        ..rect
    }
}

fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::Forward => Direction::Backward,
        Direction::Backward => Direction::Forward,
    }
}

/// Adds `entry` to `history` as its newest, unless empty; an older identical entry moves up instead of repeating.
fn remember(history: &mut Vec<String>, entry: &str) {
    if entry.is_empty() {
        return;
    }
    history.retain(|old| old != entry);
    history.push(entry.to_owned());
}
