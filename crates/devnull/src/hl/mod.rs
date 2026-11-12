//! Highlight groups (D6): the names text is styled by, the colorschemes that colour them, and `:hi`.

mod tokyonight;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::BitOr;

use ratatui::style::{Color, Modifier, Style};

/// Declares [`Group`] with each variant's `:hi` name, keeping the two in one place.
macro_rules! groups {
    ($($(#[$doc:meta])* $variant:ident => $name:literal,)*) => {
        /// A named highlight group. The colorscheme gives each its default; `:hi` changes it by name.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Group {
            $($(#[$doc])* $variant,)*
        }

        impl Group {
            /// Every group, in declaration order.
            pub const ALL: &[Group] = &[$(Group::$variant,)*];

            /// The name `:hi` knows the group by.
            #[must_use]
            pub fn name(self) -> &'static str {
                match self {
                    $(Group::$variant => $name,)*
                }
            }
        }
    };
}

groups! {
    /// Plain text; also the background of the whole screen.
    #[default]
    Normal => "Normal",
    /// Text that is not part of the content, such as wrap markers.
    NonText => "NonText",
    /// The `~` filling rows past the end of a buffer.
    EndOfBuffer => "EndOfBuffer",
    /// The statusline of the current window.
    StatusLine => "StatusLine",
    /// The statuslines of the other windows.
    StatusLineNC => "StatusLineNC",
    /// The mode in the statusline, in Normal mode.
    StatusLineModeNormal => "StatusLineModeNormal",
    /// The mode in the statusline, in Visual mode.
    StatusLineModeVisual => "StatusLineModeVisual",
    /// The mode in the statusline, on the command line.
    StatusLineModeCommand => "StatusLineModeCommand",
    /// The current buffer in the bufferline.
    TabLineSel => "TabLineSel",
    /// The rest of the bufferline.
    TabLineFill => "TabLineFill",
    /// The column between side-by-side windows.
    WinSeparator => "WinSeparator",
    /// Error messages.
    ErrorMsg => "ErrorMsg",
    /// Warnings, such as a search wrapping around.
    WarningMsg => "WarningMsg",
    /// "Press ENTER" prompts.
    MoreMsg => "MoreMsg",
    /// The Visual mode selection.
    Visual => "Visual",
    /// The summary row of a closed fold.
    Folded => "Folded",
    /// The background of popups.
    NormalFloat => "NormalFloat",
    /// The border of popups.
    FloatBorder => "FloatBorder",
    /// A key in the which-key popup.
    WhichKey => "WhichKey",
    /// The arrow between a key and what it does.
    WhichKeySeparator => "WhichKeySeparator",
    /// What a key does, in the which-key popup.
    WhichKeyDesc => "WhichKeyDesc",
    /// A key that leads to more keys, in the which-key popup.
    WhichKeyGroup => "WhichKeyGroup",
    /// A user or organization handle.
    Handle => "DevnullHandle",
    /// A commit author's name.
    Author => "DevnullAuthor",
    /// When a commit was made.
    Time => "DevnullTime",
    /// A commit's short hash.
    Hash => "DevnullHash",
    /// A commit's counts: stars, replies.
    Stats => "DevnullStats",
    /// A link in a commit.
    Link => "DevnullLink",
    /// Inline code in a commit.
    Code => "DevnullCode",
}

impl Group {
    /// The group `:hi` calls `name`, ignoring case as Vim does.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|group| group.name().eq_ignore_ascii_case(name))
    }
}

/// Text attributes, as `:hi … gui=` lists them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attrs(u8);

impl Attrs {
    /// None.
    pub const NONE: Self = Self(0);
    /// `bold`.
    pub const BOLD: Self = Self(1);
    /// `italic`.
    pub const ITALIC: Self = Self(1 << 1);
    /// `underline`.
    pub const UNDERLINE: Self = Self(1 << 2);
    /// `undercurl`, drawn as an underline.
    pub const UNDERCURL: Self = Self(1 << 3);
    /// `strikethrough`.
    pub const STRIKETHROUGH: Self = Self(1 << 4);
    /// `reverse` (also `inverse`).
    pub const REVERSE: Self = Self(1 << 5);

    const NAMED: [(&str, Self); 6] = [
        ("bold", Self::BOLD),
        ("italic", Self::ITALIC),
        ("underline", Self::UNDERLINE),
        ("undercurl", Self::UNDERCURL),
        ("strikethrough", Self::STRIKETHROUGH),
        ("reverse", Self::REVERSE),
    ];

    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Attrs {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// How a group looks, as `:hi` sets it: `None` leaves a colour to the terminal (`NONE`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spec {
    /// `guifg`.
    pub fg: Option<Color>,
    /// `guibg`.
    pub bg: Option<Color>,
    /// `guisp`: the underline colour.
    pub sp: Option<Color>,
    /// `gui`.
    pub attrs: Attrs,
}

impl Spec {
    fn style(self) -> Style {
        let mut style = Style::new();
        style.fg = self.fg;
        style.bg = self.bg;
        style.underline_color = self.sp;
        let modifiers = [
            (Attrs::BOLD, Modifier::BOLD),
            (Attrs::ITALIC, Modifier::ITALIC),
            (Attrs::UNDERLINE, Modifier::UNDERLINED),
            (Attrs::UNDERCURL, Modifier::UNDERLINED),
            (Attrs::STRIKETHROUGH, Modifier::CROSSED_OUT),
            (Attrs::REVERSE, Modifier::REVERSED),
        ];
        for (attr, modifier) in modifiers {
            if self.attrs.contains(attr) {
                style = style.add_modifier(modifier);
            }
        }
        style
    }

    /// `:hi`'s description of it, in Vim's order: `gui=… guifg=… guibg=… guisp=…`.
    fn describe(self) -> String {
        let mut text = String::new();
        let attrs: Vec<&str> = Attrs::NAMED
            .iter()
            .filter(|(_, attr)| self.attrs.contains(*attr))
            .map(|(name, _)| *name)
            .collect();
        if !attrs.is_empty() {
            let _ = write!(text, " gui={}", attrs.join(","));
        }
        for (key, color) in [("guifg", self.fg), ("guibg", self.bg), ("guisp", self.sp)] {
            if let Some(color) = color {
                let _ = write!(text, " {key}={}", color_name(color));
            }
        }
        if text.is_empty() {
            " cleared".to_owned()
        } else {
            text
        }
    }
}

/// The colorschemes `:colorscheme` knows, by name.
const COLORSCHEMES: &[(&str, &[(Group, Spec)])] = &[
    ("tokyonight-night", tokyonight::NIGHT),
    ("tokyonight", tokyonight::NIGHT),
    ("tokyonight-storm", tokyonight::STORM),
    ("tokyonight-moon", tokyonight::MOON),
    ("tokyonight-day", tokyonight::DAY),
    ("default", DEFAULT),
];

/// `default`: terminal colours only, for those who want `dn` in their terminal's own theme.
const DEFAULT: &[(Group, Spec)] = &[
    (Group::NonText, fg(Color::Blue)),
    (Group::EndOfBuffer, fg(Color::Blue)),
    (Group::StatusLine, attrs(Attrs::REVERSE)),
    (
        Group::StatusLineNC,
        Spec {
            fg: Some(Color::DarkGray),
            attrs: Attrs::REVERSE,
            ..EMPTY
        },
    ),
    (Group::StatusLineModeNormal, attrs(Attrs::BOLD)),
    (Group::StatusLineModeVisual, attrs(Attrs::BOLD)),
    (Group::StatusLineModeCommand, attrs(Attrs::BOLD)),
    (Group::TabLineSel, attrs(Attrs::BOLD)),
    (Group::TabLineFill, fg(Color::DarkGray)),
    (Group::WinSeparator, fg(Color::DarkGray)),
    (Group::ErrorMsg, fg(Color::Red)),
    (Group::WarningMsg, fg(Color::Yellow)),
    (Group::MoreMsg, fg(Color::Green)),
    (Group::Visual, attrs(Attrs::REVERSE)),
    (Group::Folded, fg(Color::Cyan)),
    (Group::FloatBorder, fg(Color::DarkGray)),
    (Group::WhichKey, fg(Color::Cyan)),
    (Group::WhichKeySeparator, fg(Color::DarkGray)),
    (Group::WhichKeyGroup, fg(Color::Magenta)),
    (Group::Handle, fg(Color::Cyan)),
    (Group::Author, attrs(Attrs::BOLD)),
    (Group::Time, fg(Color::DarkGray)),
    (Group::Hash, fg(Color::DarkGray)),
    (
        Group::Link,
        Spec {
            fg: Some(Color::Blue),
            attrs: Attrs::UNDERLINE,
            ..EMPTY
        },
    ),
    (Group::Code, fg(Color::Green)),
];

const EMPTY: Spec = Spec {
    fg: None,
    bg: None,
    sp: None,
    attrs: Attrs::NONE,
};

const fn fg(color: Color) -> Spec {
    Spec {
        fg: Some(color),
        ..EMPTY
    }
}

const fn attrs(attrs: Attrs) -> Spec {
    Spec { attrs, ..EMPTY }
}

/// The active colorscheme, with whatever `:hi` changed since it was loaded.
#[derive(Debug, Clone)]
pub struct Theme {
    name: &'static str,
    specs: HashMap<Group, Spec>,
}

impl Default for Theme {
    /// tokyonight-night (D6).
    fn default() -> Self {
        Self::named("tokyonight-night").expect("tokyonight-night is built in")
    }
}

impl Theme {
    /// `:colorscheme {name}`.
    ///
    /// # Errors
    ///
    /// Vim's E185 when there is no colorscheme by that name.
    pub fn named(name: &str) -> Result<Self, String> {
        let &(name, specs) = COLORSCHEMES
            .iter()
            .find(|(known, _)| *known == name)
            .ok_or_else(|| format!("E185: Cannot find color scheme '{name}'"))?;
        Ok(Self {
            name,
            specs: specs.iter().copied().collect(),
        })
    }

    /// The colorscheme's name, as `:colorscheme` without arguments shows it.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name
    }

    /// How `group` is drawn.
    #[must_use]
    pub fn style(&self, group: Group) -> Style {
        self.specs.get(&group).copied().unwrap_or_default().style()
    }

    /// `:hi clear`: every group back to the colorscheme's definition.
    pub fn clear(&mut self) {
        if let Ok(theme) = Self::named(self.name) {
            *self = theme;
        }
    }

    /// `:hi {group} {key}={value} …`: changes only the attributes given; with none, describes the group.
    ///
    /// # Errors
    ///
    /// Vim's error for an unknown group (E411), key (E423), colour (E254) or attribute (E418). Nothing changes
    /// when any argument is wrong.
    pub fn highlight(
        &mut self,
        group: &str,
        attrs: &[(String, String)],
    ) -> Result<Option<String>, String> {
        let group = Group::named(group)
            .ok_or_else(|| format!("E411: Highlight group not found: {group}"))?;
        let mut spec = self.specs.get(&group).copied().unwrap_or_default();
        if attrs.is_empty() {
            return Ok(Some(format!("{:<15}xxx{}", group.name(), spec.describe())));
        }
        for (key, value) in attrs {
            match key.to_ascii_lowercase().as_str() {
                "guifg" | "fg" => spec.fg = color(value)?,
                "guibg" | "bg" => spec.bg = color(value)?,
                "guisp" | "sp" => spec.sp = color(value)?,
                "gui" => spec.attrs = gui(value)?,
                // Terminal-only keys from vimrcs written for 256 colours: `dn` draws in true colour.
                "term" | "cterm" | "ctermfg" | "ctermbg" | "ctermul" | "start" | "stop" => {}
                _ => return Err(format!("E423: Illegal argument: {key}={value}")),
            }
        }
        self.specs.insert(group, spec);
        Ok(None)
    }
}

/// A `:hi` colour: `#rrggbb`, one of the terminal's named colours, or `NONE`.
fn color(value: &str) -> Result<Option<Color>, String> {
    let named = [
        ("black", Color::Black),
        ("red", Color::Red),
        ("green", Color::Green),
        ("yellow", Color::Yellow),
        ("blue", Color::Blue),
        ("magenta", Color::Magenta),
        ("cyan", Color::Cyan),
        ("gray", Color::Gray),
        ("grey", Color::Gray),
        ("darkgray", Color::DarkGray),
        ("darkgrey", Color::DarkGray),
        ("white", Color::White),
    ];
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if let Some(&(_, color)) = named
        .iter()
        .find(|(name, _)| value.eq_ignore_ascii_case(name))
    {
        return Ok(Some(color));
    }
    let hex = value.strip_prefix('#').filter(|hex| hex.len() == 6);
    let rgb = hex.and_then(|hex| u32::from_str_radix(hex, 16).ok());
    match rgb {
        Some(rgb) => Ok(Some(Color::from_u32(rgb))),
        None => Err(format!("E254: Cannot allocate color {value}")),
    }
}

/// A `gui=` value: attribute names separated by commas, or `NONE`.
fn gui(value: &str) -> Result<Attrs, String> {
    value.split(',').try_fold(Attrs::NONE, |attrs, name| {
        let name = name.to_ascii_lowercase();
        if name == "none" {
            return Ok(attrs);
        }
        let name = if name == "inverse" {
            "reverse".to_owned()
        } else {
            name
        };
        Attrs::NAMED
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, attr)| attrs | *attr)
            .ok_or_else(|| format!("E418: Illegal value: {name}"))
    })
}

/// How `:hi` writes a colour back.
fn color_name(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        other => format!("{other:?}"),
    }
}
