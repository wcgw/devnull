//! The options `:set` changes.

use crate::doc::CaseMode;
use crate::ex::{SetArg, SetOp};
use crate::graphics::Graphics;
use crate::key::{Key, parse_keys};
use crate::keymap::Keymap;

/// The options `dn` has, with Vim's defaults.
#[derive(Debug, Clone)]
pub(super) struct Options {
    pub(super) ignorecase: bool,
    pub(super) smartcase: bool,
    /// Rows of context kept above and below the cursor.
    pub(super) scrolloff: usize,
    /// How long, in milliseconds, an ambiguous key sequence waits for more.
    pub(super) timeoutlen: u64,
    /// How pictures are drawn.
    pub(super) graphics: Graphics,
    /// Whether the last search's matches are shown, as Neovim does by default.
    pub(super) hlsearch: bool,
    /// Whether a search being typed shows where it would go, as Neovim does by default.
    pub(super) incsearch: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ignorecase: false,
            smartcase: false,
            scrolloff: 0,
            timeoutlen: 1000,
            graphics: Graphics::HalfBlocks,
            hlsearch: true,
            incsearch: true,
        }
    }
}

/// What a `no` or `inv` prefix does to a flag.
#[derive(Clone, Copy)]
enum Flip {
    Off,
    Toggle,
}

impl Flip {
    /// The equivalent operation. Turning off is written as assigning nothing, which only flags accept here.
    fn op(self) -> SetOp {
        match self {
            Self::Off => SetOp::Assign(String::new()),
            Self::Toggle => SetOp::Toggle,
        }
    }
}

/// An option's name, its short name, and what it holds.
#[derive(Clone, Copy)]
enum Opt {
    Flag(fn(&mut Options) -> &mut bool),
    Number(fn(&mut Options) -> &mut u64),
    Count(fn(&mut Options) -> &mut usize),
    /// `leader`: the key `<leader>` stands for, kept by the keymap.
    Leader,
    /// `graphics`: how pictures are drawn.
    Graphics,
}

const OPTIONS: &[(&str, &str, Opt)] = &[
    ("ignorecase", "ic", Opt::Flag(|o| &mut o.ignorecase)),
    ("smartcase", "scs", Opt::Flag(|o| &mut o.smartcase)),
    ("hlsearch", "hls", Opt::Flag(|o| &mut o.hlsearch)),
    ("incsearch", "is", Opt::Flag(|o| &mut o.incsearch)),
    ("scrolloff", "so", Opt::Count(|o| &mut o.scrolloff)),
    ("timeoutlen", "tm", Opt::Number(|o| &mut o.timeoutlen)),
    ("leader", "", Opt::Leader),
    ("graphics", "", Opt::Graphics),
];

impl Options {
    /// Vim's E474 for `arg`.
    fn invalid(&self, arg: &SetArg) -> String {
        let value = match &arg.op {
            SetOp::Assign(value) => format!("={value}"),
            _ => String::new(),
        };
        format!("E474: Invalid argument: {}{value}", arg.name)
    }

    /// How searches treat case, from `'ignorecase'` and `'smartcase'`.
    pub(super) fn case_mode(&self) -> CaseMode {
        match (self.ignorecase, self.smartcase) {
            (false, _) => CaseMode::Sensitive,
            (true, false) => CaseMode::Insensitive,
            (true, true) => CaseMode::Smart,
        }
    }

    /// Applies one `:set` argument. Returns what to show (for `name?`, or `name` on a non-flag), or Vim's error.
    pub(super) fn set(
        &mut self,
        arg: &SetArg,
        keymap: &mut Keymap,
    ) -> Result<Option<String>, String> {
        let find = |typed: &str| {
            OPTIONS
                .iter()
                .copied()
                .find(|(name, short, _)| typed == *name || (!short.is_empty() && typed == *short))
        };
        let unknown = || format!("E518: Unknown option: {}", arg.name);
        // `noname` and `invname` turn a flag off or flip it, unless an option has that name itself.
        let (found, op) = match (find(&arg.name), &arg.op) {
            (Some(found), op) => (found, op.clone()),
            (None, SetOp::On) => {
                let off = arg
                    .name
                    .strip_prefix("no")
                    .and_then(find)
                    .map(|found| (found, Flip::Off));
                let inv = arg
                    .name
                    .strip_prefix("inv")
                    .and_then(find)
                    .map(|found| (found, Flip::Toggle));
                match off.or(inv) {
                    Some((found @ (_, _, Opt::Flag(_)), flip)) => (found, flip.op()),
                    Some(_) => return Err(self.invalid(arg)),
                    None => return Err(unknown()),
                }
            }
            (None, _) => return Err(unknown()),
        };
        let (name, _, opt) = found;
        let invalid = || self.invalid(arg);
        match (opt, &op) {
            (Opt::Flag(field), SetOp::Query) => {
                let on = *field(self);
                Ok(Some(format!("{}{name}", if on { "" } else { "no" })))
            }
            (Opt::Flag(field), SetOp::On) => {
                *field(self) = true;
                Ok(None)
            }
            (Opt::Flag(field), SetOp::Assign(off)) if off.is_empty() => {
                *field(self) = false;
                Ok(None)
            }
            (Opt::Flag(field), SetOp::Toggle) => {
                let flag = field(self);
                *flag = !*flag;
                Ok(None)
            }
            (Opt::Number(field), SetOp::Assign(value)) => {
                *field(self) = number(&arg.name, value)?;
                Ok(None)
            }
            (Opt::Count(field), SetOp::Assign(value)) => {
                *field(self) = usize::try_from(number(&arg.name, value)?).map_err(|_| invalid())?;
                Ok(None)
            }
            (Opt::Number(field), SetOp::Query | SetOp::On) => {
                Ok(Some(format!("{name}={}", field(self))))
            }
            (Opt::Count(field), SetOp::Query | SetOp::On) => {
                Ok(Some(format!("{name}={}", field(self))))
            }
            (Opt::Leader, SetOp::Assign(value)) => {
                match parse_keys(value, Key::char(' ')).as_slice() {
                    [key] => {
                        keymap.set_leader(*key);
                        Ok(None)
                    }
                    _ => Err(invalid()),
                }
            }
            (Opt::Leader, SetOp::Query | SetOp::On) => {
                Ok(Some(format!("{name}={}", keymap.leader())))
            }
            (Opt::Graphics, SetOp::Assign(value)) => {
                self.graphics = Graphics::named(value).ok_or_else(invalid)?;
                Ok(None)
            }
            (Opt::Graphics, SetOp::Query | SetOp::On) => {
                Ok(Some(format!("{name}={}", self.graphics.name())))
            }
            _ => Err(invalid()),
        }
    }
}

fn number(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("E521: Number required after =: {name}={value}"))
}
