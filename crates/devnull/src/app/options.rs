//! The options `:set` changes.

use crate::doc::CaseMode;
use crate::ex::{SetArg, SetOp};
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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ignorecase: false,
            smartcase: false,
            scrolloff: 0,
            timeoutlen: 1000,
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
}

const OPTIONS: &[(&str, &str, Opt)] = &[
    ("ignorecase", "ic", Opt::Flag(|o| &mut o.ignorecase)),
    ("smartcase", "scs", Opt::Flag(|o| &mut o.smartcase)),
    ("scrolloff", "so", Opt::Count(|o| &mut o.scrolloff)),
    ("timeoutlen", "tm", Opt::Number(|o| &mut o.timeoutlen)),
    ("leader", "", Opt::Leader),
];

impl Options {
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
        let &(name, _, opt) = OPTIONS
            .iter()
            .find(|(name, short, _)| arg.name == *name || (!short.is_empty() && arg.name == *short))
            .ok_or_else(|| format!("E518: Unknown option: {}", arg.name))?;
        let invalid = || {
            let value = match &arg.op {
                SetOp::Assign(value) => format!("={value}"),
                _ => String::new(),
            };
            format!("E474: Invalid argument: {}{value}", arg.name)
        };
        match (opt, &arg.op) {
            (Opt::Flag(field), SetOp::Query) => {
                let on = *field(self);
                Ok(Some(format!("{}{name}", if on { "" } else { "no" })))
            }
            (Opt::Flag(field), SetOp::On) => set_flag(field(self), true),
            (Opt::Flag(field), SetOp::Off) => set_flag(field(self), false),
            (Opt::Flag(field), SetOp::Toggle) => {
                let flag = field(self);
                set_flag(flag, !*flag)
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
            _ => Err(invalid()),
        }
    }
}

fn set_flag(flag: &mut bool, on: bool) -> Result<Option<String>, String> {
    *flag = on;
    Ok(None)
}

fn number(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("E521: Number required after =: {name}={value}"))
}
