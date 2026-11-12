//! The server's rules for a commit's text, so a client can count it as the server will before sending it: the
//! budget of [`MAX_LENGTH`] characters, links counting at most [`LINK_COST`], code counting whole.
//!
//! Ported from the Go client (`internal/api/body.go` at 76c3413, MIT), which mirrors the server. The server has the
//! last word: its refusals come back as [`crate::ApiError::Refused`].

#[cfg(test)]
mod tests;

use std::sync::LazyLock;

use regex::Regex;

/// The most characters a commit may count.
pub const MAX_LENGTH: usize = 4000;

/// The most characters a commit's text may hold, links whole.
pub const MAX_RAW_LENGTH: usize = 40_000;

/// The most a link counts, however long.
pub const LINK_COST: usize = 23;

/// The longest link, in characters (`https://` added to one written without).
const MAX_LINK: usize = 2048;

/// Code blocks (fenced by at least three backticks on their own lines) and inline code.
static CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^`{3,}[^`\r\n]*\n(?:[\s\S]*?\n)?`{3,}[ \t]*$|`[^`\r\n]+`")
        .expect("valid pattern")
});

/// Three blank lines or more, in prose.
static BLANK_LINES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:[ \t]*\n){3,}").expect("valid pattern"));

/// What may be a link: with a scheme, or a bare domain and path.
static LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[Hh][Tt][Tt][Pp][Ss]?://[^\s<>]+|(?:[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?\.)+[A-Za-z]+(?:[/?#][^\s<>]*)?",
    )
    .expect("valid pattern")
});

/// Laravel's `Str::INVISIBLE_CHARACTERS`, trimmed from text as the server trims it.
const INVISIBLE: &[char] = &[
    '\u{ad}',
    '\u{34f}',
    '\u{61c}',
    '\u{115f}',
    '\u{1160}',
    '\u{17b4}',
    '\u{17b5}',
    '\u{180e}',
    '\u{200b}',
    '\u{200c}',
    '\u{200d}',
    '\u{200e}',
    '\u{200f}',
    '\u{2060}',
    '\u{2061}',
    '\u{2062}',
    '\u{2063}',
    '\u{2064}',
    '\u{2065}',
    '\u{206a}',
    '\u{206b}',
    '\u{206c}',
    '\u{206d}',
    '\u{206e}',
    '\u{206f}',
    '\u{2800}',
    '\u{3164}',
    '\u{feff}',
    '\u{ffa0}',
    '\u{1d159}',
    '\u{1d173}',
    '\u{1d174}',
    '\u{1d175}',
    '\u{1d176}',
    '\u{1d177}',
    '\u{1d178}',
    '\u{1d179}',
    '\u{1d17a}',
    '\u{e0020}',
];

/// The top-level domains a link without a scheme may end in.
const BARE_TLDS: &str = "com org net edu gov mil info biz io dev app co ai me xyz tech gg tv fm ly to is blog cloud \
    site online design studio tools software codes wiki news website club ninja rocks academy guide social digital \
    agency fyi lol live uk de fr nl es it pt br ca au jp ch se no dk fi be at ie nz us eu cz sk hu ro bg gr hr si ee \
    lv lt ru ua tr il za mx ar kr cn tw hk sg vn th";

/// Why the server would refuse a commit's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BodyError {
    /// Nothing but blanks.
    #[error("write a commit first")]
    Empty,
    /// More than [`MAX_RAW_LENGTH`] characters, links whole.
    #[error("the text exceeds {MAX_RAW_LENGTH} characters")]
    RawTooLong,
    /// A link longer than 2048 characters.
    #[error("each link must fit within {MAX_LINK} characters")]
    LinkTooLong,
    /// More than [`MAX_LENGTH`] characters counted.
    #[error("{count}/{MAX_LENGTH} characters, over the budget")]
    OverBudget {
        /// How many it counts.
        count: usize,
    },
}

/// The text as the server keeps it: NULs dropped, blanks and invisibles trimmed from both ends, line breaks as
/// `\n`, and runs of blank lines outside code down to one.
#[must_use]
pub fn normalize(body: &str) -> String {
    let body = trim(body).replace("\r\n", "\n").replace('\r', "\n");
    map_prose(&body, |prose| {
        BLANK_LINES.replace_all(prose, "\n\n").into_owned()
    })
}

/// How many characters `body` counts against [`MAX_LENGTH`]: prose by character, each link as at most
/// [`LINK_COST`], code whole.
#[must_use]
pub fn count(body: &str) -> usize {
    budget(body).0
}

/// What `body` counts, or why the server would refuse it.
///
/// # Errors
///
/// When the server would refuse it, as [`BodyError`] says.
pub fn check(body: &str) -> Result<usize, BodyError> {
    let normal = normalize(body);
    if normal.is_empty() {
        return Err(BodyError::Empty);
    }
    if normal.chars().count() > MAX_RAW_LENGTH {
        return Err(BodyError::RawTooLong);
    }
    match budget(body) {
        (_, true) => Err(BodyError::LinkTooLong),
        (count, false) if count > MAX_LENGTH => Err(BodyError::OverBudget { count }),
        (count, false) => Ok(count),
    }
}

/// What `body` counts, and whether a link in it is too long.
fn budget(body: &str) -> (usize, bool) {
    let normal = normalize(body);
    let mut count = 0;
    let mut too_long = false;
    for (piece, code) in pieces(&normal) {
        count += piece.chars().count();
        if code {
            continue;
        }
        for link in links(piece) {
            let length = link.chars().count();
            let schemed = has_scheme(link);
            if length + if schemed { 0 } else { 8 } > MAX_LINK {
                too_long = true;
                continue;
            }
            count -= length.saturating_sub(LINK_COST);
        }
    }
    (count, too_long)
}

/// Drops NULs, then trims blanks and invisibles from both ends, as the server does to every field.
fn trim(value: &str) -> String {
    value
        .replace('\0', "")
        .trim_matches(|c: char| c.is_whitespace() || INVISIBLE.contains(&c))
        .to_owned()
}

/// `body` cut into prose and code (`true`), in order.
fn pieces(body: &str) -> Vec<(&str, bool)> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for code in CODE.find_iter(body) {
        pieces.push((&body[start..code.start()], false));
        pieces.push((code.as_str(), true));
        start = code.end();
    }
    pieces.push((&body[start..], false));
    pieces
}

/// `body` with `prose` applied to what is not code.
fn map_prose(body: &str, prose: impl Fn(&str) -> String) -> String {
    pieces(body)
        .into_iter()
        .map(|(piece, code)| if code { piece.to_owned() } else { prose(piece) })
        .collect()
}

fn has_scheme(link: &str) -> bool {
    let lower = link.to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The links in `prose`, as the server finds them.
fn links(prose: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for candidate in LINK.find_iter(prose) {
        let mut link = candidate.as_str();
        let schemed = has_scheme(link);
        if let Some(previous) = prose[..candidate.start()].chars().next_back() {
            let mut blocked = previous.is_alphabetic() || previous.is_numeric();
            if schemed {
                // Marks and connector punctuation (`_`, `‿`) glue a link to the word before it.
                blocked |= previous == '_'
                    || ('\u{203f}'..='\u{2040}').contains(&previous)
                    || is_mark(previous);
            } else {
                blocked |= "_./@-".contains(previous);
            }
            if blocked {
                continue;
            }
        }
        if !schemed && !bare_link(prose, link, candidate.end()) {
            continue;
        }
        loop {
            let before = link;
            link = link.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'', '"', '…', '”', '’']);
            for (open, close) in [('(', ')'), ('[', ']')] {
                if link.ends_with(close) && link.matches(open).count() < link.matches(close).count()
                {
                    link = &link[..link.len() - close.len_utf8()];
                }
            }
            if link == before {
                break;
            }
        }
        if !link.ends_with("://") {
            found.push(link);
        }
    }
    found
}

/// Whether `link`, written without a scheme and ending at byte `end` of `prose`, is one: a known top-level domain
/// in one case, not running on into a word.
fn bare_link(prose: &str, link: &str, end: usize) -> bool {
    let host = link.split(['/', '?', '#']).next().unwrap_or(link);
    let tld = host.rsplit('.').next().unwrap_or(host);
    if tld != tld.to_lowercase() && tld != tld.to_uppercase() {
        return false;
    }
    if !BARE_TLDS
        .split_whitespace()
        .any(|known| known == tld.to_lowercase())
    {
        return false;
    }
    let mut after = prose[end..].chars();
    match after.next() {
        Some(next) if next.is_alphanumeric() || "_(-".contains(next) => false,
        Some('.') => !after.next().is_some_and(|c| c.is_ascii_alphanumeric()),
        _ => true,
    }
}

/// Whether `c` is a combining mark, for the common blocks (the Go client asks Unicode's `M` category).
fn is_mark(c: char) -> bool {
    matches!(c,
        '\u{300}'..='\u{36f}' | '\u{1ab0}'..='\u{1aff}' | '\u{1dc0}'..='\u{1dff}' | '\u{20d0}'..='\u{20ff}'
        | '\u{fe20}'..='\u{fe2f}')
}
