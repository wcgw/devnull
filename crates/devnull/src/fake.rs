//! A made-up `log/following`, so the interface can be tried before `dn` talks to dev-null.sh (M1.8).
//!
//! Everyone and everything here is invented. M2 replaces this with the real log, rendered from the API's segments
//! and mention offsets rather than the tiny markup below.

use crate::doc::{Content, Document, Fold, Line, Span, Target};
use crate::hl::Group;

/// One invented commit.
struct Commit {
    name: &'static str,
    handle: &'static str,
    ago: &'static str,
    hash: &'static str,
    /// Paragraphs, in a tiny markup: `@handle`, `https://…` and `` `code` ``.
    body: &'static [&'static str],
    stars: u32,
    replies: u32,
}

/// Paragraphs shown before the rest of a long commit folds away.
const SHOWN_PARAGRAPHS: usize = 2;

const COMMITS: &[Commit] = &[
    Commit {
        name: "Ada Quill",
        handle: "adaq",
        ago: "2m",
        hash: "7f3a9c1",
        body: &["Shipped the new parser today. Thanks @mrivers for the review!"],
        stars: 12,
        replies: 3,
    },
    Commit {
        name: "Mo Rivers",
        handle: "mrivers",
        ago: "14m",
        hash: "b21e0d4",
        body: &["Reminder that `cargo test -- --nocapture` exists and will save your afternoon."],
        stars: 4,
        replies: 0,
    },
    Commit {
        name: "Ines Varga",
        handle: "ivarga",
        ago: "41m",
        hash: "0c9d77e",
        body: &[
            "Spent the morning chasing a flaky test. Long story, so here goes.",
            "It only failed on CI, only on Tuesdays, and only when the cache was warm.",
            "Turned out the fixture used the weekday to pick a timezone. Nobody remembers why.",
            "Moral: never let a test read the clock. Write-up at https://example.org/flaky-tuesdays",
        ],
        stars: 31,
        replies: 7,
    },
    Commit {
        name: "Tom Okafor",
        handle: "tokafor",
        ago: "1h",
        hash: "5e11f02",
        body: &["Does anyone else use `jj` for everything now? Asking for @adaq."],
        stars: 2,
        replies: 5,
    },
    Commit {
        name: "Lin Hara",
        handle: "lhara",
        ago: "2h",
        hash: "a8c40b9",
        body: &["Released v0.4 of my little TUI toolkit 🦀 https://example.org/tui-kit"],
        stars: 18,
        replies: 2,
    },
    Commit {
        name: "Ada Quill",
        handle: "adaq",
        ago: "3h",
        hash: "e40d5a3",
        body: &[
            "Thread on error handling in Rust, since @tokafor asked.",
            "1. Libraries: `thiserror`, one enum per failure a caller can act on.",
            "2. Applications: `anyhow` with context on every `?` that crosses a boundary.",
            "3. Never `unwrap()` on input you didn't produce yourself.",
            "4. Panics are for bugs. If you can name the invariant, write it in the `expect`.",
        ],
        stars: 54,
        replies: 11,
    },
    Commit {
        name: "Sam Ortiz",
        handle: "sortiz",
        ago: "5h",
        hash: "19bf6e8",
        body: &["Coffee, then code review. In that order, always."],
        stars: 6,
        replies: 1,
    },
    Commit {
        name: "Ines Varga",
        handle: "ivarga",
        ago: "8h",
        hash: "c3a2e51",
        body: &["Pairing with @lhara on the release pipeline tomorrow. Bring snacks."],
        stars: 3,
        replies: 0,
    },
];

/// The made-up following log.
#[must_use]
pub fn following() -> Document {
    let mut doc = Document::new();
    for commit in COMMITS {
        doc.push_back(commit.hash, block(commit));
    }
    doc
}

/// A commit as a block: header, body, counts, and a blank line before the next one.
fn block(commit: &Commit) -> Content {
    let mut lines = vec![Line::Text(vec![
        Span::new(commit.name, Group::Author),
        Span::new(" ", Group::Normal),
        mention(commit.handle),
        Span::new(format!(" · {}", commit.ago), Group::Time),
        Span::new(" · ", Group::Time),
        Span::new(commit.hash, Group::Hash).with_target(Target::Commit(commit.hash.to_owned())),
    ])];
    lines.extend(
        commit
            .body
            .iter()
            .map(|paragraph| Line::Text(markup(paragraph))),
    );
    let stats = format!("★ {}  ↩ {}", commit.stars, commit.replies);
    lines.push(Line::Text(vec![Span::new(stats, Group::Stats)]));
    lines.push(Line::text(""));
    let mut folds = Vec::new();
    let hidden = commit.body.len().saturating_sub(SHOWN_PARAGRAPHS);
    if hidden > 0 {
        let first = 1 + SHOWN_PARAGRAPHS;
        folds.push(Fold {
            lines: first..first + hidden,
            summary: Line::Text(vec![Span::new(
                format!("… {hidden} more lines"),
                Group::Folded,
            )]),
            closed: true,
        });
    }
    Content { lines, folds }
}

fn mention(handle: &str) -> Span {
    Span::new(format!("@{handle}"), Group::Handle).with_target(Target::Mention(handle.to_owned()))
}

/// Styles `@handle`, `https://…` and `` `code` `` in `text`.
fn markup(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        let word_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        // After `@`, which is one byte, the handle runs to the first character that cannot be in one.
        let handle_end = || {
            rest[1..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(word_end, |e| e + 1)
        };
        let special = if ch == '@' && handle_end() > 1 {
            Some((mention(&rest[1..handle_end()]), handle_end()))
        } else if rest.starts_with("https://") {
            let url = &rest[..word_end];
            Some((
                Span::new(url, Group::Link).with_target(Target::Url(url.to_owned())),
                word_end,
            ))
        } else if ch == '`' {
            rest[1..]
                .find('`')
                .map(|end| (Span::new(&rest[1..=end], Group::Code), end + 2))
        } else {
            None
        };
        match special {
            Some((span, len)) => {
                if !plain.is_empty() {
                    spans.push(Span::new(std::mem::take(&mut plain), Group::Normal));
                }
                spans.push(span);
                rest = &rest[len..];
            }
            None => {
                plain.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        spans.push(Span::new(plain, Group::Normal));
    }
    spans
}
