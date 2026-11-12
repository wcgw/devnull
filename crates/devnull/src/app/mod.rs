//! The application core (D10): state and a pure `update`; the runtime does the I/O.

mod cmdline;
mod options;
mod view;
mod window;
mod windows;

#[cfg(test)]
mod tests;

use std::time::Duration;

use ratatui::layout::Rect;

use crate::doc::{
    Direction, Document, FoldOp, Layout, Motion, Pattern, SearchOutcome, Selection, SelectionKind,
};
use crate::ex::{self, Command};
use crate::hl::Theme;
use crate::key::Key;
use crate::keymap::{Action, CmdlineKind, Invocation, Keymap, Mode};

use cmdline::{Cmdline, Edit};
use options::Options;
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
    /// Counts key presses, so a [`Wake`] asked for before the latest key is recognised as stale.
    generation: u64,
    /// Whether the which-key popup is showing.
    which_key: bool,
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
}

/// Something that happened, for [`App::update`] to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// A key was pressed.
    Key(Key),
    /// Text was pasted (bracketed paste). It never runs commands; on the command line it is inserted.
    Paste(String),
    /// The terminal gained focus.
    FocusGained,
    /// The terminal lost focus.
    FocusLost,
    /// The delay of an [`Effect::Wake`] has passed.
    Wake(Wake),
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
}

/// What keys are for right now.
#[derive(Debug)]
enum AppMode {
    Normal,
    /// Selecting, from the selection's fixed end to the cursor.
    Visual(Selection),
    /// Typing after `:`, `/` or `?`.
    Cmdline(Cmdline),
    /// Messages shown over the screen (from index `from` of the history) until a key continues.
    MorePrompt {
        from: usize,
    },
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
    /// A fresh session on the following log; made up until M2 (see [`crate::fake`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_buffer("log/following", crate::fake::following())
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
            }],
            windows: Windows::new(Window::new(0)),
            mode: AppMode::Normal,
            message: None,
            messages: Vec::new(),
            ex_history: Vec::new(),
            search_history: Vec::new(),
            last_search: None,
            generation: 0,
            which_key: false,
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
                self.generation += 1;
                self.which_key = false;
                let mut effects = self.key(key);
                effects.extend(self.wake_ups());
                effects
            }
            Msg::Wake(token) if token.generation != self.generation => Vec::new(),
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
            },
            Msg::Paste(text) => {
                if let AppMode::Cmdline(cmdline) = &mut self.mode {
                    cmdline.paste(&text);
                }
                Vec::new()
            }
            Msg::Sourced { path, contents } => self.source(&path, contents),
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
            AppMode::Cmdline(cmdline) => {
                let history = match cmdline.kind {
                    CmdlineKind::Ex => &self.ex_history,
                    CmdlineKind::Search(_) => &self.search_history,
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
                        match cmdline.kind {
                            CmdlineKind::Ex => {
                                remember(&mut self.ex_history, &text);
                                self.ex(&text)
                            }
                            CmdlineKind::Search(direction) => {
                                remember(&mut self.search_history, &text);
                                self.search(&text, direction, cmdline.count.unwrap_or(1));
                                Vec::new()
                            }
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
                window.jump_older(&layout, count);
                window.follow_cursor(&layout, viewport);
            }
            Action::JumpNewer => {
                window.jump_newer(count);
                window.follow_cursor(&layout, viewport);
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
                self.mode = AppMode::Cmdline(Cmdline::new(kind, invocation.count))
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
            AppMode::Cmdline(_) | AppMode::MorePrompt { .. } => self.key(key),
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
            Command::Quit { .. } => return Ok(vec![Effect::Quit]),
            Command::Split { vertical } => {
                self.windows
                    .split(if vertical { Axis::Columns } else { Axis::Rows })
            }
            Command::Close => self.close_window()?,
            Command::Only => self.windows.only(),
            Command::Wincmd(ch) => return self.wincmd(ch),
            Command::Set(args) => {
                for arg in &args {
                    if let Some(shown) = self.options.set(arg, &mut self.keymap)? {
                        self.info(shown);
                    }
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
            Command::Messages => self.mode = AppMode::MorePrompt { from: 0 },
            Command::Highlight { group, attrs } => {
                if let Some(shown) = self.theme.highlight(&group, &attrs)? {
                    self.info(shown);
                }
            }
            Command::HighlightClear => self.theme.clear(),
            Command::Colorscheme(None) => self.info(self.theme.name().to_owned()),
            Command::Colorscheme(Some(name)) => self.theme = Theme::named(&name)?,
            Command::Source(path) => return Ok(vec![Effect::Source(path)]),
        }
        Ok(Vec::new())
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
            self.mode = AppMode::MorePrompt { from };
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
