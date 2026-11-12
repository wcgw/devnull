//! Normal- and Visual-mode key bindings (D5): built-ins, user maps, counts and pending prefixes.
//!
//! Application commands are bound by name, as ex commands (`s` runs `:star`), so every key has an ex form and
//! `:map` can reach anything a key does.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::doc::{Direction, FoldOp, Motion, SelectionKind};
use crate::key::{Code, Key, Mods, parse_keys};

/// Which key table applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Normal mode.
    Normal,
    /// Visual mode, charwise or rowwise.
    Visual,
}

/// What a key sequence does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Moves the cursor.
    Motion(Motion),
    /// Opens the command line: `:` for ex commands, `/` and `?` for search.
    Cmdline(CmdlineKind),
    /// `n`, or `N` (`reverse`): the last search again.
    SearchNext {
        /// Whether to go the other way than the last search.
        reverse: bool,
    },
    /// `*` and `#`: searches for the word under the cursor.
    SearchWord(Direction),
    /// `v` and `V`: starts Visual mode, or switches or leaves it.
    Visual(SelectionKind),
    /// `y` in Visual mode, `yy` in Normal mode (the cursor's row).
    Yank,
    /// `<Esc>` in Visual mode.
    Escape,
    /// `zo`, `zc`, `za`.
    Fold(FoldOp),
    /// `zR` (`open`) and `zM`.
    FoldAll {
        /// Whether every fold opens rather than closes.
        open: bool,
    },
    /// Scrolls the window, moving the cursor only as far as it must to stay in view.
    Scroll(Scroll),
    /// `zt`, `zz`, `zb`: scrolls so the cursor's row is at the top, middle or bottom of the window.
    Place(Place),
    /// `H`, `M`, `L`: the cursor to the top, middle or bottom row of the window (`count` rows in for `H` and `L`).
    ViewRow(Place),
    /// `<C-o>`: back in the jumplist.
    JumpOlder,
    /// `<C-i>`: forward in the jumplist.
    JumpNewer,
    /// An ex command line, as typed after `:`, such as `star`.
    Ex(String),
    /// The rest of a map's keys after one that changed mode (`:`, `v`, `<Esc>`, …), to be fed to the new mode
    /// with [`Keymap::replay`].
    Keys {
        /// The keys, in order.
        keys: Vec<Key>,
        /// Whether user maps apply to them.
        remap: bool,
    },
    /// A key sequence that cannot run, such as a recursive mapping; the message says why.
    Error(String),
}

/// How far a scroll goes, and which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    /// `<C-e>`: one row down the buffer.
    LineDown,
    /// `<C-y>`: one row up.
    LineUp,
    /// `<C-d>`: half a window down, cursor along.
    HalfDown,
    /// `<C-u>`: half a window up, cursor along.
    HalfUp,
    /// `<C-f>`: a window down, keeping two rows.
    PageDown,
    /// `<C-b>`: a window up, keeping two rows.
    PageUp,
}

/// A place in the window: its top, middle or bottom row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The top row.
    Top,
    /// The middle row.
    Middle,
    /// The bottom row.
    Bottom,
}

/// What the command line is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdlineKind {
    /// `:`.
    Ex,
    /// `/` and `?`.
    Search(Direction),
}

/// One action to run, with the count typed before it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// What to do.
    pub action: Action,
    /// The count typed before the keys; `None` when none was.
    pub count: Option<usize>,
}

/// `f t F T`: builds the motion once the character is typed.
type CharMotion = fn(char) -> Motion;

/// What a bound sequence leads to.
#[derive(Debug, Clone)]
enum Target {
    Action(Action),
    /// `f t F T`: a motion completed by the next key.
    Char(CharMotion),
    /// `gg` and `G` (`last`): the row given by the count, or the first or last one without.
    Row {
        last: bool,
    },
    /// `;` and `,` (`reverse`): the last `f t F T` again.
    RepeatFind {
        reverse: bool,
    },
    /// A user map: keys fed again, through user maps only when `remap`.
    Keys {
        rhs: Vec<Key>,
        remap: bool,
    },
}

#[derive(Debug, Clone)]
struct Binding {
    target: Target,
    /// What which-key shows for it.
    description: Cow<'static, str>,
}

/// One mode's bindings, with every proper prefix of their key sequences, so a prefix is known to be worth waiting
/// on.
#[derive(Debug, Default)]
struct Table {
    bindings: HashMap<Vec<Key>, Binding>,
    prefixes: HashSet<Vec<Key>>,
}

impl Table {
    fn bind(&mut self, keys: Vec<Key>, target: Target, description: impl Into<Cow<'static, str>>) {
        for end in 1..keys.len() {
            self.prefixes.insert(keys[..end].to_vec());
        }
        self.bindings.insert(
            keys,
            Binding {
                target,
                description: description.into(),
            },
        );
    }
}

/// Keys typed so far that have not run yet.
#[derive(Debug, Default)]
struct Typed {
    keys: Vec<Key>,
    count: Option<usize>,
    /// A find motion waiting for its character.
    awaiting_char: Option<CharMotion>,
    /// Whether `keys` are a complete binding that is also the prefix of a longer one, so a timeout runs them.
    ambiguous: bool,
}

impl Typed {
    fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.count.is_none() && self.awaiting_char.is_none()
    }
}

/// A key that may follow the pending ones, for which-key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    /// The next key, in Vim notation.
    pub keys: String,
    /// What it does, or `+prefix` when it only leads to more bindings.
    pub description: String,
}

/// How deep user maps may expand into each other before Vim's "E223: recursive mapping".
const MAX_MAP_DEPTH: usize = 100;

/// The key bindings of Normal and Visual mode, and the keys typed toward one.
#[derive(Debug)]
pub struct Keymap {
    leader: Key,
    builtin: HashMap<Mode, Table>,
    user: HashMap<Mode, Table>,
    typed: Typed,
    last_find: Option<Motion>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new()
    }
}

impl Keymap {
    /// The default bindings, with `<Space>` as `<leader>`.
    #[must_use]
    pub fn new() -> Self {
        let leader = Key::char(' ');
        Self {
            leader,
            builtin: builtins(leader),
            user: HashMap::new(),
            typed: Typed::default(),
            last_find: None,
        }
    }

    /// Feeds one typed key in `mode`, returning what is ready to run: nothing while a sequence is still pending or
    /// when the keys are bound to nothing, several actions when a user map expands to them.
    pub fn feed(&mut self, mode: Mode, key: Key) -> Vec<Invocation> {
        let mut typed = std::mem::take(&mut self.typed);
        let ran = self.step(&mut typed, mode, key, true, 0);
        self.typed = typed;
        ran
    }

    /// Feeds the keys of an [`Action::Keys`] in `mode`, through user maps only when `remap`.
    pub fn replay(&mut self, mode: Mode, keys: &[Key], remap: bool) -> Vec<Invocation> {
        let mut typed = Typed::default();
        self.expand(&mut typed, mode, keys, remap, 0)
    }

    /// Whether keys typed so far wait for more.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        !self.typed.is_empty()
    }

    /// The keys typed toward a binding so far, without the count.
    #[must_use]
    pub fn pending_keys(&self) -> &[Key] {
        &self.typed.keys
    }

    /// Whether the pending keys are a complete binding that is also the start of a longer one: `timeoutlen` after
    /// the last key, [`Keymap::timeout`] runs them.
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        self.typed.ambiguous
    }

    /// Runs the pending keys after `timeoutlen` when they are [ambiguous](Keymap::is_ambiguous). Anything else keeps
    /// waiting, as Vim does for its own multi-key commands.
    pub fn timeout(&mut self, mode: Mode) -> Vec<Invocation> {
        if !self.typed.ambiguous {
            return Vec::new();
        }
        let mut typed = std::mem::take(&mut self.typed);
        let target = self
            .lookup(mode, &typed.keys, true)
            .map(|binding| binding.target.clone());
        let ran = target.map_or_else(Vec::new, |target| self.run(&mut typed, mode, target, 0));
        self.typed = typed;
        ran
    }

    /// Forgets keys typed so far, as when the mode changes under them.
    pub fn reset(&mut self) {
        self.typed = Typed::default();
    }

    /// `:map` and friends: binds `lhs` to the keys `rhs` (both in Vim notation, `<leader>` read now) in `mode`.
    /// With `remap`, `rhs` goes through user maps too (`:nmap`), otherwise only through built-ins (`:nnoremap`).
    pub fn map(&mut self, mode: Mode, lhs: &str, rhs: &str, remap: bool) {
        let target = Target::Keys {
            rhs: parse_keys(rhs, self.leader),
            remap,
        };
        let lhs = parse_keys(lhs, self.leader);
        self.user
            .entry(mode)
            .or_default()
            .bind(lhs, target, rhs.to_owned());
    }

    /// Makes `key` the `<leader>` of built-in bindings and of maps defined from now on, as setting `mapleader`
    /// before them does in Vim. Existing user maps keep the leader they were defined with.
    pub fn set_leader(&mut self, key: Key) {
        self.leader = key;
        self.builtin = builtins(key);
    }

    /// What may follow the pending keys in `mode`, sorted by key: each binding's description, or `+prefix` for keys
    /// that only lead to more bindings. User maps take precedence over built-ins.
    #[must_use]
    pub fn continuations(&self, mode: Mode) -> Vec<Continuation> {
        let typed = &self.typed.keys;
        let mut next: HashMap<Key, Cow<'static, str>> = HashMap::new();
        // Built-ins first, so user maps overwrite them.
        for table in [self.builtin.get(&mode), self.user.get(&mode)]
            .into_iter()
            .flatten()
        {
            for (keys, binding) in &table.bindings {
                if keys.len() <= typed.len() || !keys.starts_with(typed) {
                    continue;
                }
                let key = keys[typed.len()];
                if keys.len() == typed.len() + 1 {
                    next.insert(key, binding.description.clone());
                } else {
                    next.entry(key).or_insert(Cow::Borrowed("+prefix"));
                }
            }
        }
        let mut next: Vec<Continuation> = next
            .into_iter()
            .map(|(key, description)| Continuation {
                keys: key.to_string(),
                description: description.into_owned(),
            })
            .collect();
        next.sort_by(|a, b| a.keys.cmp(&b.keys));
        next
    }

    /// The binding of exactly `keys` in `mode`, a user map before a built-in.
    fn lookup(&self, mode: Mode, keys: &[Key], allow_user: bool) -> Option<&Binding> {
        let user = self.user.get(&mode).filter(|_| allow_user);
        user.and_then(|table| table.bindings.get(keys))
            .or_else(|| self.builtin[&mode].bindings.get(keys))
    }

    /// Whether `keys` start a longer binding in `mode`.
    fn is_prefix(&self, mode: Mode, keys: &[Key], allow_user: bool) -> bool {
        let user = self.user.get(&mode).filter(|_| allow_user);
        user.is_some_and(|table| table.prefixes.contains(keys))
            || self.builtin[&mode].prefixes.contains(keys)
    }

    fn step(
        &mut self,
        typed: &mut Typed,
        mode: Mode,
        key: Key,
        allow_user: bool,
        depth: usize,
    ) -> Vec<Invocation> {
        if let Some(make) = typed.awaiting_char.take() {
            let count = typed.count.take();
            typed.keys.clear();
            let Key {
                code: Code::Char(ch),
                mods: Mods::NONE,
            } = key
            else {
                return Vec::new();
            };
            let motion = make(ch);
            self.last_find = Some(motion);
            return vec![Invocation {
                action: Action::Motion(motion),
                count,
            }];
        }
        if key == Key::new(Code::Esc) && !typed.is_empty() {
            *typed = Typed::default();
            return Vec::new();
        }
        if typed.keys.is_empty()
            && key.mods == Mods::NONE
            && let Code::Char(digit @ '0'..='9') = key.code
            && (digit != '0' || typed.count.is_some())
        {
            let digit = digit.to_digit(10).map_or(0, |d| d as usize);
            typed.count = Some(
                typed
                    .count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit),
            );
            return Vec::new();
        }
        typed.keys.push(key);
        let exact = self
            .lookup(mode, &typed.keys, allow_user)
            .map(|binding| binding.target.clone());
        let is_prefix = self.is_prefix(mode, &typed.keys, allow_user);
        typed.ambiguous = exact.is_some() && is_prefix;
        match (exact, is_prefix) {
            (Some(target), false) => self.run(typed, mode, target, depth),
            (_, true) => Vec::new(),
            (None, false) => {
                *typed = Typed::default();
                Vec::new()
            }
        }
    }

    fn run(
        &mut self,
        typed: &mut Typed,
        mode: Mode,
        target: Target,
        depth: usize,
    ) -> Vec<Invocation> {
        let count = typed.count.take();
        typed.keys.clear();
        typed.ambiguous = false;
        let one = |action| vec![Invocation { action, count }];
        match target {
            Target::Action(action) => one(action),
            Target::Char(make) => {
                typed.count = count;
                typed.awaiting_char = Some(make);
                Vec::new()
            }
            Target::Row { last } => {
                let motion = match count {
                    Some(row) => Motion::GotoRow(row),
                    None if last => Motion::LastRow,
                    None => Motion::GotoRow(1),
                };
                vec![Invocation {
                    action: Action::Motion(motion),
                    count: None,
                }]
            }
            Target::RepeatFind { reverse } => match self.last_find {
                Some(motion) => one(Action::Motion(if reverse {
                    reversed(motion)
                } else {
                    motion
                })),
                None => Vec::new(),
            },
            Target::Keys { rhs, remap } => {
                if depth >= MAX_MAP_DEPTH {
                    return one(Action::Error("E223: recursive mapping".to_owned()));
                }
                let mut ran = self.expand(&mut Typed::default(), mode, &rhs, remap, depth + 1);
                // As in Vim, the count typed before a map applies to the first thing it runs.
                if let Some(first) = ran.first_mut() {
                    first.count = first.count.or(count);
                }
                ran
            }
        }
    }

    /// Runs `keys` in `mode` until one of them changes mode; the rest go back as [`Action::Keys`] for the new mode.
    fn expand(
        &mut self,
        typed: &mut Typed,
        mode: Mode,
        keys: &[Key],
        remap: bool,
        depth: usize,
    ) -> Vec<Invocation> {
        let mut ran = Vec::new();
        for (i, &key) in keys.iter().enumerate() {
            ran.extend(self.step(typed, mode, key, remap, depth));
            if ran
                .last()
                .is_some_and(|last| changes_mode(&last.action, mode))
            {
                let rest = keys[i + 1..].to_vec();
                if !rest.is_empty() {
                    ran.push(Invocation {
                        action: Action::Keys { keys: rest, remap },
                        count: None,
                    });
                }
                break;
            }
        }
        ran
    }
}

/// Whether `action`, run in `mode`, leaves it, so keys after it belong to another mode.
fn changes_mode(action: &Action, mode: Mode) -> bool {
    match action {
        Action::Cmdline(_) | Action::Visual(_) => true,
        Action::Escape | Action::Yank => mode == Mode::Visual,
        _ => false,
    }
}

/// The `f t F T` motion going the other way, for `,`.
fn reversed(motion: Motion) -> Motion {
    match motion {
        Motion::FindForward(ch) => Motion::FindBackward(ch),
        Motion::FindBackward(ch) => Motion::FindForward(ch),
        Motion::TillForward(ch) => Motion::TillBackward(ch),
        Motion::TillBackward(ch) => Motion::TillForward(ch),
        other => other,
    }
}

/// The default bindings of each mode (D5), with `leader` for `<leader>`.
fn builtins(leader: Key) -> HashMap<Mode, Table> {
    let keys = |notation: &str| parse_keys(notation, leader);
    let mut normal = Table::default();
    let mut visual = Table::default();
    let motions: [(&str, Motion, &str); 22] = [
        ("h", Motion::Left, "left"),
        ("<Left>", Motion::Left, "left"),
        ("j", Motion::Down, "down"),
        ("<Down>", Motion::Down, "down"),
        ("k", Motion::Up, "up"),
        ("<Up>", Motion::Up, "up"),
        ("l", Motion::Right, "right"),
        ("<Right>", Motion::Right, "right"),
        ("w", Motion::WordForward, "next word"),
        ("b", Motion::WordBackward, "previous word"),
        ("e", Motion::WordEnd, "end of word"),
        ("W", Motion::BigWordForward, "next WORD"),
        ("B", Motion::BigWordBackward, "previous WORD"),
        ("E", Motion::BigWordEnd, "end of WORD"),
        ("0", Motion::RowStart, "start of row"),
        ("^", Motion::FirstNonBlank, "first non-blank"),
        ("$", Motion::RowEnd, "end of row"),
        ("}", Motion::ParagraphForward, "next paragraph"),
        ("{", Motion::ParagraphBackward, "previous paragraph"),
        ("]]", Motion::NextBlock, "next commit"),
        ("[[", Motion::PrevBlock, "previous commit"),
        ("<Home>", Motion::RowStart, "start of row"),
    ];
    let char_motions: [(&str, CharMotion, &str); 4] = [
        ("f", Motion::FindForward, "find character"),
        ("t", Motion::TillForward, "till character"),
        ("F", Motion::FindBackward, "find character backward"),
        ("T", Motion::TillBackward, "till character backward"),
    ];
    for table in [&mut normal, &mut visual] {
        for (notation, motion, description) in motions {
            table.bind(
                keys(notation),
                Target::Action(Action::Motion(motion)),
                description,
            );
        }
        table.bind(
            keys("<End>"),
            Target::Action(Action::Motion(Motion::RowEnd)),
            "end of row",
        );
        for (notation, make, description) in char_motions {
            table.bind(keys(notation), Target::Char(make), description);
        }
        table.bind(
            keys(";"),
            Target::RepeatFind { reverse: false },
            "repeat find",
        );
        table.bind(
            keys(","),
            Target::RepeatFind { reverse: true },
            "repeat find backward",
        );
        table.bind(
            keys("gg"),
            Target::Row { last: false },
            "first row, or row [count]",
        );
        table.bind(
            keys("G"),
            Target::Row { last: true },
            "last row, or row [count]",
        );
        let view: [(&str, Action, &str); 12] = [
            ("<C-e>", Action::Scroll(Scroll::LineDown), "scroll down"),
            ("<C-y>", Action::Scroll(Scroll::LineUp), "scroll up"),
            ("<C-d>", Action::Scroll(Scroll::HalfDown), "half page down"),
            ("<C-u>", Action::Scroll(Scroll::HalfUp), "half page up"),
            ("<C-f>", Action::Scroll(Scroll::PageDown), "page down"),
            ("<C-b>", Action::Scroll(Scroll::PageUp), "page up"),
            ("zt", Action::Place(Place::Top), "row to top"),
            ("zz", Action::Place(Place::Middle), "row to middle"),
            ("zb", Action::Place(Place::Bottom), "row to bottom"),
            ("H", Action::ViewRow(Place::Top), "top of window"),
            ("M", Action::ViewRow(Place::Middle), "middle of window"),
            ("L", Action::ViewRow(Place::Bottom), "bottom of window"),
        ];
        for (notation, action, description) in view {
            table.bind(keys(notation), Target::Action(action), description);
        }
        table.bind(
            keys("v"),
            Target::Action(Action::Visual(SelectionKind::Charwise)),
            "visual",
        );
        table.bind(
            keys("V"),
            Target::Action(Action::Visual(SelectionKind::Rowwise)),
            "visual rows",
        );
    }
    visual.bind(keys("y"), Target::Action(Action::Yank), "yank");
    visual.bind(
        keys("<Esc>"),
        Target::Action(Action::Escape),
        "leave visual",
    );

    let actions: [(&str, Action, &str); 14] = [
        (":", Action::Cmdline(CmdlineKind::Ex), "command line"),
        (
            "/",
            Action::Cmdline(CmdlineKind::Search(Direction::Forward)),
            "search forward",
        ),
        (
            "?",
            Action::Cmdline(CmdlineKind::Search(Direction::Backward)),
            "search backward",
        ),
        ("n", Action::SearchNext { reverse: false }, "next match"),
        ("N", Action::SearchNext { reverse: true }, "previous match"),
        (
            "*",
            Action::SearchWord(Direction::Forward),
            "search word forward",
        ),
        (
            "#",
            Action::SearchWord(Direction::Backward),
            "search word backward",
        ),
        ("yy", Action::Yank, "yank row"),
        ("zo", Action::Fold(FoldOp::Open), "open fold"),
        ("zc", Action::Fold(FoldOp::Close), "close fold"),
        ("za", Action::Fold(FoldOp::Toggle), "toggle fold"),
        ("zR", Action::FoldAll { open: true }, "open all folds"),
        ("zM", Action::FoldAll { open: false }, "close all folds"),
        ("<C-o>", Action::JumpOlder, "jump back"),
    ];
    for (notation, action, description) in actions {
        normal.bind(keys(notation), Target::Action(action), description);
    }
    normal.bind(
        keys("<C-i>"),
        Target::Action(Action::JumpNewer),
        "jump forward",
    );
    let commands: [(&str, &str, &str); 30] = [
        ("q", "quit", "close window"),
        ("<CR>", "open", "open under cursor"),
        ("<C-]>", "open", "open under cursor"),
        ("gd", "profile", "author's profile"),
        ("K", "hover", "hover card"),
        ("gx", "browse link", "open link in browser"),
        ("gX", "browse", "open commit on the web"),
        ("gy", "yanklink", "yank commit link"),
        ("<C-l>", "edit!", "refresh"),
        ("c", "commit", "new commit"),
        ("r", "reply", "reply"),
        ("R", "quote", "quote"),
        ("s", "star", "toggle star"),
        ("dd", "delete", "delete own commit"),
        ("<C-w>h", "wincmd h", "window left"),
        ("<C-w>j", "wincmd j", "window below"),
        ("<C-w>k", "wincmd k", "window above"),
        ("<C-w>l", "wincmd l", "window right"),
        ("<C-w>w", "wincmd w", "next window"),
        ("<C-w>s", "split", "split"),
        ("<C-w>v", "vsplit", "split vertically"),
        ("<C-w>c", "close", "close window"),
        ("<C-w>o", "only", "only this window"),
        ("<leader>1", "edit log/following", "following log"),
        ("<leader>2", "edit log/latest", "latest log"),
        ("<leader>3", "edit log/trending", "trending log"),
        ("<leader>n", "edit notifications", "notifications"),
        ("<leader>s", "edit starred", "starred"),
        ("<leader>p", "edit profile", "your profile"),
        ("<leader><leader>", "buffers", "buffers"),
    ];
    for (notation, command, description) in commands {
        normal.bind(
            keys(notation),
            Target::Action(Action::Ex(command.to_owned())),
            description,
        );
    }
    normal.bind(
        keys("<leader>/"),
        Target::Action(Action::Ex("users".to_owned())),
        "find user",
    );
    HashMap::from([(Mode::Normal, normal), (Mode::Visual, visual)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Motion;
    use crate::key::{Key, parse_keys};

    /// Feeds `notation` in `mode` and collects every invocation it produced.
    fn feed(keymap: &mut Keymap, mode: Mode, notation: &str) -> Vec<Invocation> {
        parse_keys(notation, Key::char(' '))
            .into_iter()
            .flat_map(|key| keymap.feed(mode, key))
            .collect()
    }

    fn run(action: Action, count: Option<usize>) -> Invocation {
        Invocation { action, count }
    }

    fn motion(motion: Motion) -> Invocation {
        run(Action::Motion(motion), None)
    }

    #[test]
    fn a_single_key_runs_its_action() {
        let mut keymap = Keymap::new();

        assert_eq!(feed(&mut keymap, Mode::Normal, "j"), [motion(Motion::Down)]);
    }

    #[test]
    fn digits_before_a_command_are_its_count_and_a_lone_zero_is_a_motion() {
        let mut keymap = Keymap::new();

        assert_eq!(
            feed(&mut keymap, Mode::Normal, "3j"),
            [run(Action::Motion(Motion::Down), Some(3))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "10k"),
            [run(Action::Motion(Motion::Up), Some(10))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "0"),
            [motion(Motion::RowStart)]
        );
    }

    #[test]
    fn a_prefix_waits_for_the_rest_of_the_sequence() {
        let mut keymap = Keymap::new();

        assert_eq!(feed(&mut keymap, Mode::Normal, "g"), []);
        assert!(keymap.is_pending());
        assert_eq!(keymap.pending_keys(), [Key::char('g')]);
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "g"),
            [motion(Motion::GotoRow(1))]
        );
        assert!(!keymap.is_pending());
    }

    #[test]
    fn gg_and_big_g_turn_a_count_into_a_row_number() {
        let mut keymap = Keymap::new();

        assert_eq!(
            feed(&mut keymap, Mode::Normal, "5gg"),
            [motion(Motion::GotoRow(5))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "G"),
            [motion(Motion::LastRow)]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "7G"),
            [motion(Motion::GotoRow(7))]
        );
    }

    #[test]
    fn find_motions_take_the_next_key_and_semicolon_and_comma_repeat_them() {
        let mut keymap = Keymap::new();

        assert_eq!(feed(&mut keymap, Mode::Normal, "f"), []);
        assert!(keymap.is_pending());
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "x"),
            [motion(Motion::FindForward('x'))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "2T."),
            [run(Action::Motion(Motion::TillBackward('.')), Some(2))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, ";"),
            [motion(Motion::TillBackward('.'))]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, ","),
            [motion(Motion::TillForward('.'))]
        );
    }

    #[test]
    fn escape_cancels_a_pending_sequence_and_its_count() {
        let mut keymap = Keymap::new();

        assert_eq!(
            feed(&mut keymap, Mode::Normal, "3g<Esc>j"),
            [motion(Motion::Down)]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "f<Esc>j"),
            [motion(Motion::Down)]
        );
    }

    #[test]
    fn unbound_keys_do_nothing() {
        let mut keymap = Keymap::new();

        assert_eq!(feed(&mut keymap, Mode::Normal, "<F9>"), []);
        assert!(!keymap.is_pending());
    }

    fn ex(command: &str) -> Invocation {
        run(Action::Ex(command.to_owned()), None)
    }

    #[test]
    fn a_noremap_expands_to_built_ins_only_and_its_count_goes_to_the_first_action() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Normal, "x", "jw", false);
        keymap.map(Mode::Normal, "w", "k", false);

        assert_eq!(
            feed(&mut keymap, Mode::Normal, "x"),
            [motion(Motion::Down), motion(Motion::WordForward)]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "3x"),
            [
                run(Action::Motion(Motion::Down), Some(3)),
                motion(Motion::WordForward)
            ]
        );
    }

    #[test]
    fn a_remap_expands_through_other_user_maps_and_recursion_is_an_error() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Normal, "a", "b", true);
        keymap.map(Mode::Normal, "b", "j", true);
        keymap.map(Mode::Normal, "o", "o", true);

        assert_eq!(feed(&mut keymap, Mode::Normal, "a"), [motion(Motion::Down)]);
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "o"),
            [run(Action::Error("E223: recursive mapping".into()), None)]
        );
    }

    #[test]
    fn user_maps_shadow_built_ins_and_can_run_ex_commands() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Normal, "j", "k", false);
        keymap.map(Mode::Normal, "<leader>x", ":star<CR>", false);

        assert_eq!(feed(&mut keymap, Mode::Normal, "j"), [motion(Motion::Up)]);
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "<leader>x"),
            [
                run(Action::Cmdline(CmdlineKind::Ex), None),
                run(
                    Action::Keys {
                        keys: parse_keys("star<CR>", Key::char(' ')),
                        remap: false
                    },
                    None
                )
            ]
        );
    }

    #[test]
    fn a_map_that_is_also_a_prefix_waits_for_more_or_runs_on_timeout() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Normal, "d", "j", false);

        assert_eq!(feed(&mut keymap, Mode::Normal, "d"), []);
        assert_eq!(feed(&mut keymap, Mode::Normal, "d"), [ex("delete")]);
        assert_eq!(feed(&mut keymap, Mode::Normal, "2d"), []);
        assert!(keymap.is_ambiguous());
        assert_eq!(
            keymap.timeout(Mode::Normal),
            [run(Action::Motion(Motion::Down), Some(2))]
        );
        assert_eq!(keymap.timeout(Mode::Normal), []);
    }

    #[test]
    fn built_in_prefixes_wait_without_a_timeout() {
        let mut keymap = Keymap::new();

        assert_eq!(feed(&mut keymap, Mode::Normal, "z"), []);
        assert!(!keymap.is_ambiguous());
        assert_eq!(keymap.timeout(Mode::Normal), []);
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "a"),
            [run(Action::Fold(FoldOp::Toggle), None)]
        );
    }

    #[test]
    fn which_key_lists_what_can_follow_the_pending_keys() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Normal, "zz", ":center<CR>", false);
        let _ = feed(&mut keymap, Mode::Normal, "z");

        let next: Vec<(String, String)> = keymap
            .continuations(Mode::Normal)
            .into_iter()
            .map(|c| (c.keys, c.description))
            .collect();

        assert_eq!(
            next,
            [
                ("M", "close all folds"),
                ("R", "open all folds"),
                ("a", "toggle fold"),
                ("b", "row to bottom"),
                ("c", "close fold"),
                ("o", "open fold"),
                ("t", "row to top"),
                ("z", ":center<CR>"),
            ]
            .map(|(k, d)| (k.to_owned(), d.to_owned()))
        );
    }

    #[test]
    fn the_leader_can_change_and_built_ins_follow_it() {
        let mut keymap = Keymap::new();
        assert_eq!(
            feed(&mut keymap, Mode::Normal, "<Space>1"),
            [ex("edit log/following")]
        );

        keymap.set_leader(Key::char(','));

        assert_eq!(
            feed(&mut keymap, Mode::Normal, ",1"),
            [ex("edit log/following")]
        );
        assert_eq!(feed(&mut keymap, Mode::Normal, "<Space>1"), []);
    }

    #[test]
    fn visual_mode_has_its_own_table() {
        let mut keymap = Keymap::new();
        keymap.map(Mode::Visual, "x", "y", false);

        assert_eq!(
            feed(&mut keymap, Mode::Visual, "wy"),
            [motion(Motion::WordForward), run(Action::Yank, None)]
        );
        assert_eq!(
            feed(&mut keymap, Mode::Visual, "x"),
            [run(Action::Yank, None)]
        );
        assert_eq!(feed(&mut keymap, Mode::Normal, "x"), []);
        assert_eq!(
            feed(&mut keymap, Mode::Visual, "<Esc>"),
            [run(Action::Escape, None)]
        );
    }
}
