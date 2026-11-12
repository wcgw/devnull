//! Commits as document blocks: what the log (and later threads and profiles) show for each commit.

#[cfg(test)]
mod tests;

use devnull_api::model::{AccountKind, Author, Commit, Segment};

use crate::doc::{Content, Fold, Line, Span, Target};
use crate::hl::Group;

/// Lines of text shown before the rest of a long commit folds away.
const SHOWN_LINES: usize = 6;

/// Replies shown under a commit, from those the server sends along.
const REPLY_PREVIEWS: usize = 3;

/// `commit` as a block: header, text (long text folded), quote, link card, images, counts, a few replies, and a
/// blank line. The block refers to the commit. Ages are relative to `now` (Unix seconds).
#[must_use]
pub fn commit_block(commit: &Commit, now: u64) -> Content {
    block(commit, now, Shown::Summary)
}

/// `commit` as the block of the thread about it: as [`commit_block`], but all of its text, and without the replies
/// that the thread lists below it.
#[must_use]
pub fn focused_block(commit: &Commit, now: u64) -> Content {
    block(commit, now, Shown::Whole)
}

/// How much of a commit its block shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// Long text folded, and a few replies.
    Summary,
    /// All the text, and no replies.
    Whole,
}

fn block(commit: &Commit, now: u64, shown: Shown) -> Content {
    let target = Some(Target::Commit(commit.id.clone()));
    if commit.is_deleted() {
        let lines = vec![
            Line::Text(vec![Span::new("Commit deleted", Group::Time)]),
            Line::text(""),
        ];
        return Content {
            lines,
            folds: Vec::new(),
            target,
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
        lines.push(Line::Text(vec![
            Span::new(text, Group::Link).with_target(Target::Url(card.url.clone())),
        ]));
    }
    for image in &commit.images {
        let mut text = format!("🖼 image {}×{}", image.width, image.height);
        if let Some(description) = image.description.as_deref().filter(|d| !d.is_empty()) {
            text.push_str(&format!(": {description}"));
        }
        lines.push(Line::Text(vec![
            Span::new(text, Group::Stats).with_target(Target::Url(image.url.clone())),
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
                Group::Folded,
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
        spans.push(Span::new(prefix, Group::Time));
    }
    if let Some(author) = &commit.author {
        spans.extend(author_spans(author));
    }
    let age = parse_time(&commit.created_at)
        .map(|then| age(then, now))
        .unwrap_or_default();
    spans.push(Span::new(format!(" · {age} · "), Group::Time));
    spans.push(Span::new(commit.short_hash.clone(), Group::Hash));
    Line::Text(spans)
}

/// `Name @handle`, or `Name (org) @handle` for an organization.
fn author_spans(author: &Author) -> Vec<Span> {
    let mut spans = vec![Span::new(author.name.clone(), Group::Author)];
    if author.kind == AccountKind::Organization {
        spans.push(Span::new(" (org)", Group::Time));
    }
    spans.push(Span::new(" ", Group::Normal));
    spans.push(mention(&author.handle));
    spans
}

fn mention(handle: &str) -> Span {
    Span::new(format!("@{handle}"), Group::Handle).with_target(Target::Mention(handle.to_owned()))
}

/// The quoted commit, indented under a bar. Its text refers to it, unless it refers to something of its own.
fn quote(quoted: &Commit, now: u64) -> Vec<Line> {
    const BAR: &str = "┃ ";
    let mut lines = if quoted.is_deleted() {
        vec![Line::Text(vec![Span::new(
            format!("{BAR}Commit deleted"),
            Group::Time,
        )])]
    } else {
        let mut lines = vec![header(quoted, now, Some(BAR))];
        lines.extend(body_lines(quoted).into_iter().map(|line| {
            let Line::Text(spans) = line else { return line };
            Line::Text(
                std::iter::once(Span::new(BAR, Group::Time))
                    .chain(spans)
                    .collect(),
            )
        }));
        lines
    };
    for line in &mut lines {
        if let Line::Text(spans) = line {
            for span in spans.iter_mut().filter(|span| span.target.is_none()) {
                span.target = Some(Target::Commit(quoted.id.clone()));
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
    Line::Text(vec![Span::new(text, Group::Stats)])
}

/// `  ↳ Name @handle: first line`.
fn reply_line(reply: &Commit) -> Line {
    let mut spans = vec![Span::new("  ↳ ", Group::Time)];
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
    spans.push(Span::new(format!(": {first}"), Group::Normal));
    Line::Text(spans)
}

/// The commit's text as lines: text segments break at newlines, inline code stays in its line, code blocks get
/// lines of their own.
fn body_lines(commit: &Commit) -> Vec<Line> {
    let mentioned: Vec<String> = commit
        .mentions
        .iter()
        .map(|m| m.handle.to_lowercase())
        .collect();
    let mut lines = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    for segment in &commit.body_segments {
        match segment {
            Segment::Text { text } => {
                for (i, piece) in text.split('\n').enumerate() {
                    if i > 0 {
                        lines.push(Line::Text(std::mem::take(&mut current)));
                    }
                    current.extend(text_spans(piece, &mentioned));
                }
            }
            Segment::Code { code } => current.push(Span::new(code.clone(), Group::Code)),
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
                lines.push(Line::Text(vec![Span::new(title, Group::Time)]));
                let code = plain_text(html);
                lines.extend(
                    code.lines()
                        .map(|line| Line::Text(vec![Span::new(line, Group::Code)])),
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
fn text_spans(text: &str, mentioned: &[String]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, spans: &mut Vec<Span>| {
        if !plain.is_empty() {
            spans.push(Span::new(std::mem::take(plain), Group::Normal));
        }
    };
    while let Some(ch) = rest.chars().next() {
        if ch == '@' {
            let end = rest[1..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .map_or(rest.len(), |e| e + 1);
            let handle = &rest[1..end];
            if !handle.is_empty() && mentioned.contains(&handle.to_lowercase()) {
                flush(&mut plain, &mut spans);
                spans.push(mention(handle));
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
            spans.push(Span::new(url, Group::Link).with_target(Target::Url(url.to_owned())));
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

/// How long ago `then` was at `now` (both Unix seconds), as the web says it: `now`, `5m`, `3h`, `2d`, then the date
/// (`Oct 1`, with the year when it is not this year's).
#[must_use]
pub fn age(then: u64, now: u64) -> String {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86_400 => format!("{}h", seconds / 3600),
        86_400..604_800 => format!("{}d", seconds / 86_400),
        _ => {
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let (year, month, day) = civil(then / 86_400);
            let (this_year, ..) = civil(now / 86_400);
            let date = format!("{} {day}", MONTHS[usize::from(month - 1)]);
            if year == this_year {
                date
            } else {
                format!("{date}, {year}")
            }
        }
    }
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

/// The date of day `days` since 1970-01-01, as (year, month, day).
fn civil(days: u64) -> (u64, u8, u8) {
    let days = days + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u8::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u8::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}
