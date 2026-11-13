//! Key bindings (D5): the kit's (its D12), and `dn`'s defaults, bound by the ex commands they run.

pub use wcgw_kit::keymap::*;

/// What `dn`'s own commands for commits are about, in `:help`: between opening and buffers.
pub const COMMITTING: Topic = Topic::new(75, "Commits");

/// `dn`'s default bindings: keys, the ex command each runs, what which-key says of it, and its `:help` topic.
const DEFAULTS: [(&str, &str, &str, Topic); 19] = [
    ("<CR>", "open", "open under cursor", Topic::OPENING),
    ("<C-]>", "open", "open under cursor", Topic::OPENING),
    ("gd", "profile", "author's profile", Topic::OPENING),
    ("K", "hover", "hover card", Topic::OPENING),
    ("gx", "browse link", "open link in browser", Topic::OPENING),
    ("gX", "browse", "open commit on the web", Topic::OPENING),
    ("<leader>/", "pick users", "find user", Topic::OPENING),
    ("gy", "yanklink", "yank commit link", Topic::YANKING),
    ("c", "commit", "new commit", COMMITTING),
    ("r", "reply", "reply", COMMITTING),
    ("R", "quote", "quote", COMMITTING),
    ("s", "star", "toggle star", COMMITTING),
    ("dd", "delete", "delete own commit", COMMITTING),
    (
        "<leader>1",
        "edit log/following",
        "following log",
        Topic::BUFFERS,
    ),
    ("<leader>2", "edit log/latest", "latest log", Topic::BUFFERS),
    (
        "<leader>3",
        "edit log/trending",
        "trending log",
        Topic::BUFFERS,
    ),
    (
        "<leader>n",
        "edit notifications",
        "notifications",
        Topic::BUFFERS,
    ),
    ("<leader>s", "edit starred", "starred", Topic::BUFFERS),
    ("<leader>p", "edit profile", "your profile", Topic::BUFFERS),
];

/// The kit's bindings and `dn`'s.
#[must_use]
pub fn keymap() -> Keymap {
    let mut keymap = Keymap::new();
    for (notation, command, description, topic) in DEFAULTS {
        keymap.bind(&[Mode::Normal], notation, command, description, topic);
    }
    keymap
}
