//! Notifications as blocks: what happened, by whom, to which commit; a group's accounts folded under it.

use devnull_api::model::{Commit, Notification, NotificationKind};

use super::{age, body_lines, commit_target, parse_time, who};
use crate::doc::{AccountRef, Content, Fold, Line, Span, Target};
use crate::hl::Group;

/// Lines of a commit shown under a notification.
const EXCERPT_LINES: usize = 3;

/// Where what follows a notification's first line starts: under the account that did it.
const INDENT: &str = "    ";

/// `notification` as a block: a gutter marking it `new` (since the inbox was last read), what happened, by whom
/// and when (ages relative to `now`, in Unix seconds); then the commit it is about, the reason of a moderation
/// decision, a group's accounts folded, and a blank line. The block refers to the commit, else to who did it.
#[must_use]
pub fn notification_block(notification: &Notification, new: bool, now: u64) -> Content {
    let kind = notification.kind;
    let (icon, verb) = match kind {
        NotificationKind::Followed => ("+", "followed you"),
        NotificationKind::Starred => ("★", "starred your commit"),
        NotificationKind::Replied => ("↩", "replied to your commit"),
        NotificationKind::Quoted => ("❝", "quoted your commit"),
        NotificationKind::Mentioned => ("@", "mentioned you"),
        NotificationKind::Approved => ("✓", "Your commit was approved"),
        NotificationKind::Rejected => ("×", "Your commit was rejected"),
        _ => ("•", "did something"),
    };
    let mut first = vec![
        Span::new(if new { "● " } else { "  " }, crate::hl::HANDLE),
        Span::new(format!("{icon} "), crate::hl::STATS),
    ];
    let moderation = matches!(
        kind,
        NotificationKind::Approved | NotificationKind::Rejected
    );
    if moderation {
        first.push(Span::new(verb, Group::NORMAL));
    } else {
        if let Some(actor) = &notification.actor {
            first.extend(who(
                &actor.name,
                &actor.handle,
                actor.kind,
                &actor.avatar_url,
            ));
        }
        let others = notification.actors_count.saturating_sub(1);
        if others > 0 {
            let s = if others == 1 { "" } else { "s" };
            first.push(Span::new(format!(" and {others} other{s}"), Group::NORMAL));
        }
        first.push(Span::new(format!(" {verb}"), Group::NORMAL));
    }
    let age = parse_time(&notification.created_at)
        .map(|then| age(then, now))
        .unwrap_or_default();
    first.push(Span::new(format!(" · {age}"), crate::hl::TIME));
    let mut lines = vec![Line::Text(first)];
    if let Some(commit) = &notification.commit {
        // The account's own commit is quoted; a reply, quote or mention is shown as written.
        let own = moderation || kind == NotificationKind::Starred;
        lines.extend(excerpt(commit, own));
    }
    if !notification.reason.is_empty() {
        lines.push(Line::Text(vec![Span::new(
            format!("{INDENT}reason: {}", notification.reason),
            crate::hl::TIME,
        )]));
    }
    let mut folds = Vec::new();
    if notification.actors.len() > 1 {
        let start = lines.len();
        for actor in &notification.actors {
            let mut spans = vec![Span::new(INDENT, Group::NORMAL)];
            spans.extend(who(
                &actor.name,
                &actor.handle,
                actor.kind,
                &actor.avatar_url,
            ));
            lines.push(Line::Text(spans));
        }
        let noun = match kind {
            NotificationKind::Starred => "stars",
            NotificationKind::Followed => "followers",
            _ => "updates",
        };
        folds.push(Fold {
            lines: start..lines.len(),
            summary: Line::Text(vec![Span::new(
                format!("{INDENT}▸ {} {noun}", notification.event_count),
                Group::FOLDED,
            )]),
            closed: true,
        });
    }
    lines.push(Line::text(""));
    let target = match (&notification.commit, &notification.actor) {
        (Some(commit), _) => Some(commit_target(commit)),
        (None, Some(actor)) => Some(Target::Account(AccountRef::new(
            actor.handle.clone(),
            Some(actor.kind),
        ))),
        (None, None) => None,
    };
    Content {
        lines,
        folds,
        target,
    }
}

/// The first lines of `commit`, indented, under a bar when it is the account's `own`.
fn excerpt(commit: &Commit, own: bool) -> Vec<Line> {
    let lead = if own {
        format!("{INDENT}┃ ")
    } else {
        INDENT.to_owned()
    };
    let indent = |spans: Vec<Span>| {
        Line::Text(
            std::iter::once(Span::new(lead.clone(), crate::hl::TIME))
                .chain(spans)
                .collect(),
        )
    };
    if commit.is_deleted() {
        return vec![indent(vec![Span::new("Commit deleted", crate::hl::TIME)])];
    }
    let body = body_lines(commit);
    let more = body.len() > EXCERPT_LINES;
    let mut lines: Vec<Line> = body
        .into_iter()
        .take(EXCERPT_LINES)
        .map(|line| match line {
            Line::Text(spans) => indent(spans),
            image @ Line::Image { .. } => image,
        })
        .collect();
    if more {
        lines.push(indent(vec![Span::new("…", crate::hl::TIME)]));
    }
    lines
}
