//! `dn`'s highlight groups (D6): the kit's (its D7), and those of what dev-null.sh shows, each linked to a standard
//! group so any colorscheme colours it, with `dn`'s own colours for the built-in colorschemes.

pub use wcgw_kit::hl::*;

/// A user or organization handle.
pub const HANDLE: Group = Group::new("DevnullHandle");
/// A commit author's name.
pub const AUTHOR: Group = Group::new("DevnullAuthor");
/// When a commit was made.
pub const TIME: Group = Group::new("DevnullTime");
/// A commit's short hash.
pub const HASH: Group = Group::new("DevnullHash");
/// A commit's counts: stars, replies.
pub const STATS: Group = Group::new("DevnullStats");
/// A link in a commit.
pub const LINK: Group = Group::new("DevnullLink");
/// Inline code in a commit.
pub const CODE: Group = Group::new("DevnullCode");

/// `dn`'s groups and the standard ones they are drawn as when a colorscheme says nothing of them.
const GROUPS: [(Group, Group); 7] = [
    (HANDLE, Group::IDENTIFIER),
    (AUTHOR, Group::BOLD),
    (TIME, Group::COMMENT),
    (HASH, Group::COMMENT),
    (STATS, Group::NORMAL),
    (LINK, Group::UNDERLINED),
    (CODE, Group::STRING),
];

/// `dn`'s colours for the built-in colorschemes, as tokyonight.nvim's palettes give them.
const OVERRIDES: [(&str, &str); 6] = [
    (
        "tokyonight-night",
        "\
         hi DevnullHandle guifg=#7dcfff\n\
         hi DevnullAuthor gui=bold guifg=#c0caf5\n\
         hi DevnullTime guifg=#565f89\n\
         hi DevnullHash guifg=#737aa2\n\
         hi DevnullStats guifg=#a9b1d6\n\
         hi DevnullLink gui=underline guifg=#7aa2f7\n\
         hi DevnullCode guifg=#9ece6a\n\
",
    ),
    (
        "tokyonight",
        "\
         hi DevnullHandle guifg=#7dcfff\n\
         hi DevnullAuthor gui=bold guifg=#c0caf5\n\
         hi DevnullTime guifg=#565f89\n\
         hi DevnullHash guifg=#737aa2\n\
         hi DevnullStats guifg=#a9b1d6\n\
         hi DevnullLink gui=underline guifg=#7aa2f7\n\
         hi DevnullCode guifg=#9ece6a\n\
",
    ),
    (
        "tokyonight-storm",
        "\
         hi DevnullHandle guifg=#7dcfff\n\
         hi DevnullAuthor gui=bold guifg=#c0caf5\n\
         hi DevnullTime guifg=#565f89\n\
         hi DevnullHash guifg=#737aa2\n\
         hi DevnullStats guifg=#a9b1d6\n\
         hi DevnullLink gui=underline guifg=#7aa2f7\n\
         hi DevnullCode guifg=#9ece6a\n\
",
    ),
    (
        "tokyonight-moon",
        "\
         hi DevnullHandle guifg=#86e1fc\n\
         hi DevnullAuthor gui=bold guifg=#c8d3f5\n\
         hi DevnullTime guifg=#636da6\n\
         hi DevnullHash guifg=#737aa2\n\
         hi DevnullStats guifg=#828bb8\n\
         hi DevnullLink gui=underline guifg=#82aaff\n\
         hi DevnullCode guifg=#c3e88d\n\
",
    ),
    (
        "tokyonight-day",
        "\
         hi DevnullHandle guifg=#007197\n\
         hi DevnullAuthor gui=bold guifg=#3760bf\n\
         hi DevnullTime guifg=#848cb5\n\
         hi DevnullHash guifg=#68709a\n\
         hi DevnullStats guifg=#6172b0\n\
         hi DevnullLink gui=underline guifg=#2e7de9\n\
         hi DevnullCode guifg=#587539\n\
",
    ),
    (
        "default",
        "\
         hi DevnullHandle guifg=cyan\n\
         hi DevnullAuthor gui=bold\n\
         hi DevnullTime guifg=darkgray\n\
         hi DevnullHash guifg=darkgray\n\
         hi DevnullStats NONE\n\
         hi DevnullLink gui=underline guifg=blue\n\
         hi DevnullCode guifg=green\n\
",
    ),
];

/// The kit's theme with `dn`'s groups and colours.
#[must_use]
pub fn theme() -> Theme {
    let mut theme = Theme::new();
    for (group, link) in GROUPS {
        theme.define(group, Some(link));
    }
    for (scheme, text) in OVERRIDES {
        theme
            .add_overrides(scheme, text)
            .expect("dn's colours name dn's groups");
    }
    theme
}
