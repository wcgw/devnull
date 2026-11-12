//! Highlight groups: the names text is styled by, resolved to colours by the active colorscheme (D6).

use ratatui::style::{Color, Modifier, Style};

/// A named highlight group. `:hi` overrides a group by name; the colorscheme gives every group its default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Group {
    /// Plain text.
    #[default]
    Normal,
    /// A user or organization handle.
    Handle,
    /// The `~` filling rows past the end of a buffer.
    NonText,
    /// The statusline of the current window.
    StatusLine,
    /// The current buffer in the bufferline.
    TabLineSel,
    /// The rest of the bufferline.
    TabLineFill,
    /// Error messages.
    ErrorMsg,
    /// Warnings, such as a search wrapping around.
    WarningMsg,
    /// "Press ENTER" prompts.
    MoreMsg,
}

impl Group {
    /// The style this group draws with.
    // A stand-in until colorschemes and `:hi` (M1.7): terminal colours only.
    #[must_use]
    pub fn style(self) -> Style {
        match self {
            Self::Normal => Style::new(),
            Self::Handle => Style::new().fg(Color::Cyan),
            Self::NonText => Style::new().fg(Color::Blue),
            Self::StatusLine => Style::new().add_modifier(Modifier::REVERSED),
            Self::TabLineSel => Style::new().add_modifier(Modifier::BOLD),
            Self::TabLineFill => Style::new().fg(Color::DarkGray),
            Self::ErrorMsg => Style::new().fg(Color::Red),
            Self::WarningMsg => Style::new().fg(Color::Yellow),
            Self::MoreMsg => Style::new().fg(Color::Green),
        }
    }
}
