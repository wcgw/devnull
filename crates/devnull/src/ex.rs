//! Ex command lines (D5, D6): what `:` runs and what `init.vim` holds.
//!
//! Only whole commands, one per line: no ranges and no `|` chaining. Application commands (`:star`, `:reply`, …)
//! join the table as they are implemented; until then they are "Not an editor command".

use std::fmt;

use crate::keymap::Mode;

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// `:q`, `:qa`, with `!` to force.
    Quit {
        /// `:qa`: every window rather than the current one.
        all: bool,
        /// `!`: even when something would be lost.
        force: bool,
    },
    /// `:map` and its mode and `noremap` variants.
    Map {
        /// The modes the map applies to.
        modes: Vec<Mode>,
        /// The keys typed, in Vim notation.
        lhs: String,
        /// The keys they stand for, in Vim notation.
        rhs: String,
        /// Whether `rhs` goes through user maps too (`:map`) rather than only built-ins (`:noremap`).
        remap: bool,
    },
    /// `:unmap` and its mode variants.
    Unmap {
        /// The modes to remove the map from.
        modes: Vec<Mode>,
        /// The keys of the map, in Vim notation.
        lhs: String,
    },
    /// `:set` with its arguments, in order.
    Set(Vec<SetArg>),
    /// `:hi {group} {key}={value} ...`.
    Highlight {
        /// The highlight group's name, as typed.
        group: String,
        /// Attribute keys and values, in order, such as `guifg` and `#c0caf5`.
        attrs: Vec<(String, String)>,
    },
    /// `:hi clear`: every group back to the colorscheme's defaults.
    HighlightClear,
    /// `:colorscheme {name}`, or without a name to show the current one.
    Colorscheme(Option<String>),
    /// `:source {file}`.
    Source(String),
    /// `:messages`.
    Messages,
    /// `:split` and `:vsplit` (`vertical`).
    Split {
        /// Side by side rather than stacked.
        vertical: bool,
    },
    /// `:close`: the current window, unless it is the last.
    Close,
    /// `:only`: every window but the current one.
    Only,
    /// `:wincmd {char}`: what `<C-w>{char}` does.
    Wincmd(char),
    /// `:connect`: connect this device, or with a link, finish connecting with the callback link the browser ended
    /// on.
    Connect(Option<String>),
    /// `:agree`: accept dev-null.sh's current terms.
    Agree,
    /// `:edit {name}` opens a buffer by name; `:edit` alone (or `:e!`) reloads the current one.
    Edit(Option<String>),
    /// `:ls`, `:buffers`: lists the buffers.
    Buffers,
    /// `:buffer {N|name}`: shows a buffer, by number or (part of) its name.
    Buffer(String),
    /// `:bnext` (`forward`) and `:bprevious`.
    NextBuffer {
        /// Whether to go to the next buffer rather than the previous one.
        forward: bool,
    },
}

/// One `:set` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetArg {
    /// The option's name as typed, including any `no` or `inv` prefix: only the options know whether `nowrap` is
    /// `wrap` turned off or an option of its own.
    pub name: String,
    /// What to do with it.
    pub op: SetOp,
}

/// What a `:set` argument does to its option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOp {
    /// `name`: turn a flag on (or off, or flip it, through a `no` or `inv` prefix), or show another option.
    On,
    /// `name!`: flip a flag.
    Toggle,
    /// `name?`: show the value.
    Query,
    /// `name=value` or `name:value`.
    Assign(String),
}

/// Why a command line cannot run, worded as Vim words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExError {
    /// E492: the command name is unknown.
    NotACommand(String),
    /// E477: the command takes no `!`.
    NoBang,
    /// E471: an argument is missing.
    ArgumentRequired,
    /// E475: an argument is malformed.
    InvalidArgument(String),
}

impl fmt::Display for ExError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotACommand(line) => write!(f, "E492: Not an editor command: {line}"),
            Self::NoBang => f.write_str("E477: No ! allowed"),
            Self::ArgumentRequired => f.write_str("E471: Argument required"),
            Self::InvalidArgument(arg) => write!(f, "E475: Invalid argument: {arg}"),
        }
    }
}

impl std::error::Error for ExError {}

/// The commands `dn` knows, with the shortest abbreviation Vim accepts for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Name {
    Quit,
    QuitAll,
    Map(&'static [Mode], bool),
    Unmap(&'static [Mode]),
    Set,
    Highlight,
    Colorscheme,
    Source,
    Messages,
    Split(bool),
    Close,
    Only,
    Wincmd,
    Connect,
    Agree,
    Edit,
    Buffer,
    Buffers,
    NextBuffer(bool),
}

const NORMAL: &[Mode] = &[Mode::Normal];
const VISUAL: &[Mode] = &[Mode::Visual];
const NORMAL_VISUAL: &[Mode] = &[Mode::Normal, Mode::Visual];

/// Full name, shortest abbreviation length, and command, in the order Vim resolves ambiguous abbreviations.
const COMMANDS: &[(&str, usize, Name)] = &[
    ("edit", 1, Name::Edit),
    ("buffer", 1, Name::Buffer),
    ("buffers", 7, Name::Buffers),
    ("bnext", 2, Name::NextBuffer(true)),
    ("bprevious", 2, Name::NextBuffer(false)),
    ("ls", 2, Name::Buffers),
    ("quit", 1, Name::Quit),
    ("qall", 2, Name::QuitAll),
    ("quitall", 5, Name::QuitAll),
    ("map", 3, Name::Map(NORMAL_VISUAL, true)),
    ("nmap", 2, Name::Map(NORMAL, true)),
    ("vmap", 2, Name::Map(VISUAL, true)),
    ("xmap", 2, Name::Map(VISUAL, true)),
    ("noremap", 2, Name::Map(NORMAL_VISUAL, false)),
    ("nnoremap", 2, Name::Map(NORMAL, false)),
    ("vnoremap", 2, Name::Map(VISUAL, false)),
    ("xnoremap", 2, Name::Map(VISUAL, false)),
    ("unmap", 3, Name::Unmap(NORMAL_VISUAL)),
    ("nunmap", 3, Name::Unmap(NORMAL)),
    ("vunmap", 2, Name::Unmap(VISUAL)),
    ("xunmap", 2, Name::Unmap(VISUAL)),
    ("set", 2, Name::Set),
    ("highlight", 2, Name::Highlight),
    ("colorscheme", 4, Name::Colorscheme),
    ("source", 2, Name::Source),
    ("messages", 3, Name::Messages),
    ("split", 2, Name::Split(false)),
    ("vsplit", 2, Name::Split(true)),
    ("close", 3, Name::Close),
    ("only", 2, Name::Only),
    ("wincmd", 4, Name::Wincmd),
    ("connect", 4, Name::Connect),
    ("agree", 5, Name::Agree),
];

/// Parses one command line, as typed after `:` or written in `init.vim`. Blank lines and `"` comments are
/// `Ok(None)`; leading `:` and whitespace are ignored.
///
/// # Errors
///
/// When the command is unknown or its arguments don't fit it, with Vim's message.
pub fn parse(line: &str) -> Result<Option<Command>, ExError> {
    let trimmed = line.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    if trimmed.is_empty() || trimmed.starts_with('"') {
        return Ok(None);
    }
    let name_len = trimmed
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(trimmed.len());
    let (typed, rest) = trimmed.split_at(name_len);
    let not_a_command = || ExError::NotACommand(trimmed.trim_end().to_owned());
    let name = COMMANDS
        .iter()
        .find(|(full, min, _)| typed.len() >= *min && full.starts_with(typed))
        .map(|&(_, _, name)| name)
        .ok_or_else(not_a_command)?;
    let (bang, args) = match rest.strip_prefix('!') {
        Some(args) => (true, args.trim()),
        None => (false, rest.trim()),
    };
    if bang && !matches!(name, Name::Quit | Name::QuitAll | Name::Edit) {
        return Err(ExError::NoBang);
    }
    let command = match name {
        Name::Quit | Name::QuitAll => Command::Quit {
            all: name == Name::QuitAll,
            force: bang,
        },
        Name::Map(modes, remap) => {
            let (lhs, rhs) = map_args(args).ok_or(ExError::ArgumentRequired)?;
            Command::Map {
                modes: modes.to_vec(),
                lhs: lhs.to_owned(),
                rhs: rhs.to_owned(),
                remap,
            }
        }
        Name::Unmap(modes) => Command::Unmap {
            modes: modes.to_vec(),
            lhs: required(args)?.to_owned(),
        },
        Name::Set => Command::Set(args.split_whitespace().map(set_arg).collect()),
        Name::Highlight => highlight(args)?,
        Name::Colorscheme => Command::Colorscheme((!args.is_empty()).then(|| args.to_owned())),
        Name::Source => Command::Source(required(args)?.to_owned()),
        Name::Messages => Command::Messages,
        Name::Split(vertical) => Command::Split { vertical },
        Name::Close => Command::Close,
        Name::Only => Command::Only,
        Name::Connect => Command::Connect((!args.is_empty()).then(|| args.to_owned())),
        Name::Agree => Command::Agree,
        Name::Edit => Command::Edit((!args.is_empty()).then(|| args.to_owned())),
        Name::Buffer => Command::Buffer(required(args)?.to_owned()),
        Name::Buffers => Command::Buffers,
        Name::NextBuffer(forward) => Command::NextBuffer { forward },
        Name::Wincmd => {
            let mut chars = required(args)?.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Command::Wincmd(ch),
                _ => return Err(ExError::InvalidArgument(args.to_owned())),
            }
        }
    };
    Ok(Some(command))
}

fn required(args: &str) -> Result<&str, ExError> {
    if args.is_empty() {
        Err(ExError::ArgumentRequired)
    } else {
        Ok(args)
    }
}

/// The left side and the rest of the line of a `:map`, after map modifiers like `<silent>`, which `dn` accepts but
/// has no use for. `None` when either side is missing.
fn map_args(args: &str) -> Option<(&str, &str)> {
    let mut rest = args;
    while let Some(after) = ["<silent>", "<nowait>", "<unique>"]
        .iter()
        .find_map(|m| rest.strip_prefix(m))
    {
        rest = after.trim_start();
    }
    let (lhs, rhs) = rest.split_once(char::is_whitespace)?;
    let rhs = rhs.trim_start();
    (!rhs.is_empty()).then_some((lhs, rhs))
}

fn set_arg(arg: &str) -> SetArg {
    let named = |name: &str, op| SetArg {
        name: name.to_owned(),
        op,
    };
    if let Some((name, value)) = arg.split_once(['=', ':']) {
        return named(name, SetOp::Assign(value.to_owned()));
    }
    if let Some(name) = arg.strip_suffix('?') {
        return named(name, SetOp::Query);
    }
    if let Some(name) = arg.strip_suffix('!') {
        return named(name, SetOp::Toggle);
    }
    named(arg, SetOp::On)
}

fn highlight(args: &str) -> Result<Command, ExError> {
    let mut words = args.split_whitespace();
    let group = words.next().ok_or(ExError::ArgumentRequired)?;
    if group == "clear" {
        return Ok(Command::HighlightClear);
    }
    let attrs = words
        .map(|word| {
            word.split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .ok_or_else(|| ExError::InvalidArgument(word.to_owned()))
        })
        .collect::<Result<_, _>>()?;
    Ok(Command::Highlight {
        group: group.to_owned(),
        attrs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mode;

    fn parsed(line: &str) -> Command {
        parse(line).expect("valid command").expect("not blank")
    }

    #[test]
    fn quit_and_its_abbreviations_and_bang() {
        assert_eq!(
            parsed("q"),
            Command::Quit {
                all: false,
                force: false
            }
        );
        assert_eq!(
            parsed("quit!"),
            Command::Quit {
                all: false,
                force: true
            }
        );
        assert_eq!(
            parsed("qa"),
            Command::Quit {
                all: true,
                force: false
            }
        );
        assert_eq!(
            parsed("  quita!"),
            Command::Quit {
                all: true,
                force: true
            }
        );
    }

    #[test]
    fn unknown_commands_and_misplaced_bangs_are_vim_errors() {
        assert_eq!(
            parse("star").unwrap_err().to_string(),
            "E492: Not an editor command: star"
        );
        assert_eq!(
            parse("frobnicate x").unwrap_err().to_string(),
            "E492: Not an editor command: frobnicate x"
        );
        assert_eq!(
            parse("messages!").unwrap_err().to_string(),
            "E477: No ! allowed"
        );
        assert_eq!(
            parse("colo").map(Option::unwrap),
            Ok(Command::Colorscheme(None))
        );
    }

    #[test]
    fn blank_lines_and_comments_are_nothing() {
        assert_eq!(parse("   "), Ok(None));
        assert_eq!(parse("\" a comment"), Ok(None));
        assert_eq!(parse(":"), Ok(None));
    }

    #[test]
    fn maps_take_a_left_side_and_the_rest_of_the_line() {
        assert_eq!(
            parsed("nnoremap <silent> <leader>x :star<CR>"),
            Command::Map {
                modes: vec![Mode::Normal],
                lhs: "<leader>x".into(),
                rhs: ":star<CR>".into(),
                remap: false
            }
        );
        assert_eq!(
            parsed("map Q gq ip"),
            Command::Map {
                modes: vec![Mode::Normal, Mode::Visual],
                lhs: "Q".into(),
                rhs: "gq ip".into(),
                remap: true
            }
        );
        assert!(
            matches!(parsed("xn x y"), Command::Map { remap: false, ref modes, .. } if *modes == [Mode::Visual])
        );
        assert!(
            matches!(parsed("vmap x y"), Command::Map { remap: true, ref modes, .. } if *modes == [Mode::Visual])
        );
        assert_eq!(
            parse("nnoremap x").unwrap_err().to_string(),
            "E471: Argument required"
        );
        assert_eq!(
            parsed("nun x"),
            Command::Unmap {
                modes: vec![Mode::Normal],
                lhs: "x".into()
            }
        );
    }

    #[test]
    fn set_takes_any_number_of_options() {
        assert_eq!(
            parsed("se ignorecase nosmartcase invwrap timeoutlen=500 leader=, scrolloff:3 wrap?"),
            Command::Set(vec![
                SetArg {
                    name: "ignorecase".into(),
                    op: SetOp::On
                },
                SetArg {
                    name: "nosmartcase".into(),
                    op: SetOp::On
                },
                SetArg {
                    name: "invwrap".into(),
                    op: SetOp::On
                },
                SetArg {
                    name: "timeoutlen".into(),
                    op: SetOp::Assign("500".into())
                },
                SetArg {
                    name: "leader".into(),
                    op: SetOp::Assign(",".into())
                },
                SetArg {
                    name: "scrolloff".into(),
                    op: SetOp::Assign("3".into())
                },
                SetArg {
                    name: "wrap".into(),
                    op: SetOp::Query
                },
            ])
        );
        assert_eq!(
            parsed("set nonumber!"),
            Command::Set(vec![SetArg {
                name: "nonumber".into(),
                op: SetOp::Toggle
            }])
        );
    }

    #[test]
    fn highlight_sets_attributes_by_key() {
        assert_eq!(
            parsed("hi StatusLine guifg=#c0caf5 bg=#1f2335 gui=bold,italic"),
            Command::Highlight {
                group: "StatusLine".into(),
                attrs: vec![
                    ("guifg".into(), "#c0caf5".into()),
                    ("bg".into(), "#1f2335".into()),
                    ("gui".into(), "bold,italic".into()),
                ],
            }
        );
        assert_eq!(parsed("highlight clear"), Command::HighlightClear);
        assert_eq!(
            parse("hi Normal guifg").unwrap_err().to_string(),
            "E475: Invalid argument: guifg"
        );
    }

    #[test]
    fn colorscheme_source_and_messages() {
        assert_eq!(
            parsed("colorscheme tokyonight-storm"),
            Command::Colorscheme(Some("tokyonight-storm".into()))
        );
        assert_eq!(
            parsed("so ~/.config/devnull/init.vim"),
            Command::Source("~/.config/devnull/init.vim".into())
        );
        assert_eq!(
            parse("source").unwrap_err().to_string(),
            "E471: Argument required"
        );
        assert_eq!(parsed("mes"), Command::Messages);
    }

    #[test]
    fn edit_reloads_or_opens_a_buffer() {
        assert_eq!(parsed("e!"), Command::Edit(None));
        assert_eq!(parsed("edit"), Command::Edit(None));
        assert_eq!(
            parsed("e log/latest"),
            Command::Edit(Some("log/latest".into()))
        );
    }

    #[test]
    fn buffer_commands() {
        assert_eq!(parsed("ls"), Command::Buffers);
        assert_eq!(parsed("buffers"), Command::Buffers);
        assert_eq!(parsed("b 2"), Command::Buffer("2".into()));
        assert_eq!(parsed("buffer lat"), Command::Buffer("lat".into()));
        assert_eq!(parsed("bn"), Command::NextBuffer { forward: true });
        assert_eq!(parsed("bprevious"), Command::NextBuffer { forward: false });
        assert_eq!(
            parse("b").unwrap_err().to_string(),
            "E471: Argument required"
        );
    }
}
