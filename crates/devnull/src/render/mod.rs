//! What dev-null.sh sends, as document blocks: commits, the people and organizations who write them, and
//! notifications.

mod accounts;
mod notifications;
#[cfg(test)]
mod tests;

use devnull_api::model::{AccountKind, Author, Commit, Mention, Preview, Segment};

pub use accounts::{account_block, organization_header, pinned_block, profile_header};
pub use notifications::notification_block;

use crate::doc::{AccountRef, Content, Fold, Line, LinkCard, Span, Target};
use crate::hl::Group;
use wcgw_kit::gfx::{fit, natural};
use wcgw_kit::jiff::Timestamp;

/// Lines of text shown before the rest of a long commit folds away.
const SHOWN_LINES: usize = 6;

/// The most cells a commit's picture takes across, and down: a glance; the caption opens it whole.
const PICTURE_COLS: u16 = 48;
const PICTURE_ROWS: u16 = 14;

/// The cells a link card's picture takes; cards are about twice as wide as tall.
const CARD_COLS: u16 = 32;
const CARD_ROWS: u16 = 8;

/// The cells an avatar takes beside a name: a square, as cells are twice as tall as wide.
const AVATAR_COLS: u16 = 2;

/// The width, in pixels, avatars beside names are asked for at.
const AVATAR_PIXELS: u32 = 40;

/// Replies shown under a commit, from those the server sends along.
const REPLY_PREVIEWS: usize = 3;

/// `commit` as a block: header, text (long text folded), quote, link card, images, counts, a few replies, and a
/// blank line. The block refers to the commit. Ages are relative to `now` (Unix seconds).
#[must_use]
pub fn commit_block(commit: &Commit, now: u64) -> Content {
    block(commit, now, Shown::Summary)
}

/// `commit` as the block of the thread about it: as [`commit_block`], but all of its text, its pictures at their own
/// size (a cell being `cell` pixels, across and down), and without the replies that the thread lists below it.
#[must_use]
pub fn focused_block(commit: &Commit, now: u64, cell: (u16, u16)) -> Content {
    block(commit, now, Shown::Whole { cell })
}

/// What is left of a deleted commit.
#[must_use]
pub fn deleted_block() -> Content {
    vec![
        Line::Text(vec![Span::new("Commit deleted", crate::hl::TIME)]),
        Line::text(""),
    ]
    .into()
}

/// How much of a commit its block shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// Long text folded, and a few replies.
    Summary,
    /// All the text, pictures at their own size in cells of `cell` pixels, and no replies.
    Whole {
        /// A cell's size in pixels, across and down.
        cell: (u16, u16),
    },
}

fn block(commit: &Commit, now: u64, shown: Shown) -> Content {
    let target = Some(commit_target(commit));
    if commit.is_deleted() {
        return Content {
            target,
            ..deleted_block()
        };
    }
    let mut lines = vec![header(commit, now, None)];
    let body = body_lines(commit);
    let body_len = body.len();
    lines.extend(body);
    if let Some(quoted) = &commit.quoted_commit {
        lines.extend(quote(quoted, now));
    }
    if let Some(card) = &commit.link_preview {
        let text = if card.is_pending {
            "🔗 Loading preview…".to_owned()
        } else {
            let title = if card.title.is_empty() {
                card.url.as_str()
            } else {
                card.title.as_str()
            };
            match &card.site_name {
                Some(site) if !site.is_empty() => format!("🔗 {title} · {site}"),
                _ => format!("🔗 {title}"),
            }
        };
        let target = Target::LinkCard(LinkCard {
            url: card.url.clone(),
            title: card.title.clone(),
            site: card.site_name.clone().unwrap_or_default(),
            description: card.description.clone(),
        });
        lines.push(Line::Text(vec![
            Span::new(text, crate::hl::LINK).with_target(target),
        ]));
        if let Some(picture) = card.image_url.as_deref().filter(|url| !url.is_empty()) {
            // Its size is not known before it arrives: a box of the usual card shape.
            lines.push(Line::Image {
                rows: CARD_ROWS,
                cols: CARD_COLS,
                source: picture.to_owned(),
            });
        }
    }
    for image in &commit.images {
        let (cols, rows) = match shown {
            Shown::Summary => fit(image.width, image.height, PICTURE_COLS, PICTURE_ROWS),
            Shown::Whole { cell } => natural(image.width, image.height, cell),
        };
        if cols > 0 {
            lines.push(Line::Image {
                rows,
                cols,
                source: image.url.clone(),
            });
        }
        let mut text = format!("🖼 image {}×{}", image.width, image.height);
        if let Some(description) = image.description.as_deref().filter(|d| !d.is_empty()) {
            text.push_str(&format!(": {description}"));
        }
        lines.push(Line::Text(vec![
            Span::new(text, crate::hl::STATS).with_target(Target::Url(image.url.clone())),
        ]));
    }
    lines.push(counts(commit));
    let replies = commit
        .reply_preview
        .iter()
        .filter(|_| shown == Shown::Summary)
        .flat_map(|preview| &preview.data);
    for reply in replies
        .filter(|reply| !reply.is_deleted())
        .take(REPLY_PREVIEWS)
    {
        lines.push(reply_line(reply));
    }
    lines.push(Line::text(""));
    let mut folds = Vec::new();
    if shown == Shown::Summary && body_len > SHOWN_LINES {
        let hidden = body_len - SHOWN_LINES;
        folds.push(Fold {
            lines: 1 + SHOWN_LINES..1 + body_len,
            summary: Line::Text(vec![Span::new(
                format!(
                    "… {hidden} more {}",
                    if hidden == 1 { "line" } else { "lines" }
                ),
                Group::FOLDED,
            )]),
            closed: true,
        });
    }
    Content {
        lines,
        folds,
        target,
    }
}

/// `Name @handle · age · hash`, after `prefix` when given (for quotes).
fn header(commit: &Commit, now: u64, prefix: Option<&str>) -> Line {
    let mut spans = Vec::new();
    if let Some(prefix) = prefix {
        spans.push(Span::new(prefix, crate::hl::TIME));
    }
    if let Some(author) = &commit.author {
        spans.extend(author_spans(author));
    }
    let age = parse_time(&commit.created_at)
        .map(|then| age(then, now))
        .unwrap_or_default();
    spans.push(Span::new(format!(" · {age} · "), crate::hl::TIME));
    spans.push(Span::new(commit.short_hash.clone(), crate::hl::HASH));
    Line::Text(spans)
}

/// What a commit's text refers to.
fn commit_target(commit: &Commit) -> Target {
    Target::Commit {
        id: commit.id.clone(),
        author: commit
            .author
            .as_ref()
            .map(|author| AccountRef::new(author.handle.clone(), Some(author.kind))),
        url: Some(commit.html_url.clone()).filter(|url| !url.is_empty()),
        starred: commit.starred,
    }
}

/// `Name @handle`, `Name (org) @handle` for an organization, or `@handle` without a name, after the `avatar` when
/// there is one. The handle refers to the account, of `kind` unless it is [`AccountKind::Other`].
fn who(name: &str, handle: &str, kind: AccountKind, avatar: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    if !avatar.is_empty() {
        spans.push(Span::picture(sized(avatar, AVATAR_PIXELS), AVATAR_COLS));
        spans.push(Span::new(" ", Group::NORMAL));
    }
    if !name.is_empty() {
        spans.push(Span::new(name, crate::hl::AUTHOR));
        if kind == AccountKind::Organization {
            spans.push(Span::new(" (org)", crate::hl::TIME));
        }
        spans.push(Span::new(" ", Group::NORMAL));
    }
    spans.push(mention(handle, Some(kind)));
    spans
}

fn author_spans(author: &Author) -> Vec<Span> {
    who(
        &author.name,
        &author.handle,
        author.kind,
        &author.avatar_url,
    )
}

/// `url` asking for an avatar `pixels` wide, where the host takes the size: GitHub's.
fn sized(url: &str, pixels: u32) -> String {
    let Some(rest) = url.strip_prefix("https://avatars.githubusercontent.com/") else {
        return url.to_owned();
    };
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if path.contains('#')
        || query.contains('#')
        || query.split('&').any(|pair| pair.starts_with("s="))
    {
        return url.to_owned();
    }
    let joiner = if query.is_empty() { '?' } else { '&' };
    let url = url.trim_end_matches('?');
    format!("{url}{joiner}s={pixels}")
}

/// `@handle`, referring to the account.
fn mention(handle: &str, kind: Option<AccountKind>) -> Span {
    Span::new(format!("@{handle}"), crate::hl::HANDLE)
        .with_target(Target::Account(AccountRef::new(handle, kind)))
}

/// The quoted commit, indented under a bar. Its text refers to it, unless it refers to something of its own.
fn quote(quoted: &Commit, now: u64) -> Vec<Line> {
    const BAR: &str = "┃ ";
    let mut lines = if quoted.is_deleted() {
        vec![Line::Text(vec![Span::new(
            format!("{BAR}Commit deleted"),
            crate::hl::TIME,
        )])]
    } else {
        let mut lines = vec![header(quoted, now, Some(BAR))];
        lines.extend(body_lines(quoted).into_iter().map(|line| {
            let Line::Text(spans) = line else { return line };
            Line::Text(
                std::iter::once(Span::new(BAR, crate::hl::TIME))
                    .chain(spans)
                    .collect(),
            )
        }));
        lines
    };
    for line in &mut lines {
        if let Line::Text(spans) = line {
            for span in spans.iter_mut().filter(|span| span.target.is_none()) {
                span.target = Some(commit_target(quoted));
            }
        }
    }
    lines
}

/// `★ stars  ↩ replies`, the star filled when the account starred it, and the status unless approved.
fn counts(commit: &Commit) -> Line {
    let star = if commit.starred { '★' } else { '☆' };
    let mut text = format!("{star} {}  ↩ {}", commit.stars_count, commit.replies_count);
    if !commit.status.is_empty() && commit.status != "approved" {
        text.push_str(&format!(" · {}", commit.status));
    }
    Line::Text(vec![Span::new(text, crate::hl::STATS)])
}

/// `  ↳ Name @handle: first line`.
fn reply_line(reply: &Commit) -> Line {
    let mut spans = vec![Span::new("  ↳ ", crate::hl::TIME)];
    if let Some(author) = &reply.author {
        spans.extend(author_spans(author));
    }
    let first = body_lines(reply)
        .into_iter()
        .next()
        .map_or_else(String::new, |line| match line {
            Line::Text(spans) => spans.into_iter().map(|span| span.text).collect(),
            Line::Image { .. } => String::new(),
        });
    spans.push(Span::new(format!(": {first}"), Group::NORMAL));
    Line::Text(spans)
}

/// The commit's text as lines: text segments break at newlines, inline code stays in its line, code blocks get
/// lines of their own.
fn body_lines(commit: &Commit) -> Vec<Line> {
    segment_lines(&commit.body_segments, &commit.mentions)
}

/// What the server would show of a draft, from its preview: the text alone, its mentions not yet resolved.
#[must_use]
pub fn preview_block(preview: &Preview) -> Content {
    segment_lines(&preview.segments, &[]).into()
}

/// `segments` as lines (see [`body_lines`]); only handles in `mentions` refer to accounts.
fn segment_lines(segments: &[Segment], mentions: &[Mention]) -> Vec<Line> {
    let mentioned: Vec<(String, AccountKind)> = mentions
        .iter()
        .map(|m| (m.handle.to_lowercase(), m.kind))
        .collect();
    let mut lines = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    for segment in segments {
        match segment {
            Segment::Text { text } => {
                for (i, piece) in text.split('\n').enumerate() {
                    if i > 0 {
                        lines.push(Line::Text(std::mem::take(&mut current)));
                    }
                    current.extend(text_spans(piece, &mentioned));
                }
            }
            Segment::Code { code } => current.push(Span::new(code.clone(), crate::hl::CODE)),
            Segment::Block {
                language,
                filename,
                html,
            } => {
                if !current.is_empty() {
                    lines.push(Line::Text(std::mem::take(&mut current)));
                }
                let title = match (filename.as_deref(), language.as_deref()) {
                    (Some(file), Some(lang)) => format!("── {file} ({lang}) ──"),
                    (Some(name), None) | (None, Some(name)) => format!("── {name} ──"),
                    (None, None) => "──".to_owned(),
                };
                lines.push(Line::Text(vec![Span::new(title, crate::hl::TIME)]));
                let code = plain_text(html);
                lines.extend(
                    code.lines()
                        .map(|line| Line::Text(vec![Span::new(line, crate::hl::CODE)])),
                );
            }
            // A kind of segment this version does not know: leave it out rather than show it wrong.
            _ => {}
        }
    }
    if !current.is_empty() {
        lines.push(Line::Text(current));
    }
    lines
}

/// Plain text with mentions (only of handles the commit mentions) and links made into targets.
fn text_spans(text: &str, mentioned: &[(String, AccountKind)]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, spans: &mut Vec<Span>| {
        if !plain.is_empty() {
            spans.push(Span::new(std::mem::take(plain), Group::NORMAL));
        }
    };
    while let Some(ch) = rest.chars().next() {
        if ch == '@' {
            let end = rest[1..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .map_or(rest.len(), |e| e + 1);
            let handle = &rest[1..end];
            let lowercase = handle.to_lowercase();
            if let Some((_, kind)) = mentioned.iter().find(|(known, _)| *known == lowercase) {
                flush(&mut plain, &mut spans);
                spans.push(mention(handle, Some(*kind)));
                rest = &rest[end..];
                continue;
            }
        }
        if rest.starts_with("https://") || rest.starts_with("http://") {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            // Punctuation ending a sentence is not part of the link.
            let url =
                rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '\'', '"']);
            flush(&mut plain, &mut spans);
            spans.push(Span::new(url, crate::hl::LINK).with_target(Target::Url(url.to_owned())));
            rest = &rest[url.len()..];
            continue;
        }
        plain.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    flush(&mut plain, &mut spans);
    spans
}

/// The text of server-highlighted HTML: tags dropped, entities decoded.
fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '<' => rest = rest.find('>').map_or("", |end| &rest[end + 1..]),
            '&' => {
                let entity = rest
                    .find(';')
                    .filter(|&end| end <= 10)
                    .map(|end| &rest[1..end]);
                let decoded = entity.and_then(|entity| match entity {
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "amp" => Some('&'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => entity
                        .strip_prefix("#x")
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                        .and_then(char::from_u32),
                });
                match (decoded, entity) {
                    (Some(decoded), Some(entity)) => {
                        text.push(decoded);
                        rest = &rest[entity.len() + 2..];
                    }
                    _ => {
                        text.push('&');
                        rest = &rest[1..];
                    }
                }
            }
            _ => {
                text.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    text
}

/// How long ago `then` was at `now` (both Unix seconds), as every wcgw app says it: `now`, `5m`, `3h`, `2d`, `6w`,
/// `1y` (the kit's `when::ago`).
#[must_use]
pub fn age(then: u64, now: u64) -> String {
    let at = |seconds: u64| {
        i64::try_from(seconds)
            .ok()
            .and_then(|seconds| Timestamp::from_second(seconds).ok())
            .unwrap_or_default()
    };
    wcgw_kit::when::ago(at(then), at(now))
}

/// Unix seconds for an RFC 3339 UTC time such as `2026-11-12T15:13:40.000000Z`; `None` when it is not one.
#[must_use]
pub fn parse_time(text: &str) -> Option<u64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<u64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if separators
        .iter()
        .any(|&(at, byte)| text.as_bytes().get(at) != Some(&byte))
        || !(1..=12).contains(&month)
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a date in the proleptic Gregorian calendar (Howard Hinnant's algorithm).
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let year = if month <= 2 {
        year.checked_sub(1)?
    } else {
        year
    };
    let era = year / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era).checked_sub(719_468)
}
