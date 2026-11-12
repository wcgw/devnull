//! People and organizations as blocks: the head of their profiles, and their entries in lists of people.

use devnull_api::model::{Account, AccountKind, Commit, OrganizationPage, Profile};

use super::{commit_block, mention, who};
use crate::doc::{AccountRef, Content, Line, Span, Target};
use crate::hl::Group;

/// The head of a person's profile: who they are, their bio and details, their counts (followers and following lead
/// to those lists), how they relate to the connected account, and their streak. The block refers to them.
#[must_use]
pub fn profile_header(profile: &Profile) -> Content {
    let account = &profile.account;
    let mut lines = vec![Line::Text(who(
        &account.name,
        &account.handle,
        AccountKind::User,
    ))];
    lines.extend(text_lines(&account.bio));
    lines.extend(details(
        &[&account.company, &account.location],
        &account.website,
    ));
    let lists = |count: Option<u64>, list: &str| {
        count.map(|count| {
            (
                format!("{count} {list}"),
                Some(format!("@{}/{list}", account.handle)),
            )
        })
    };
    let counts: Vec<(String, Option<String>)> = [
        account
            .commits_count
            .map(|count| (format!("{count} commits"), None)),
        lists(account.followers_count, "followers"),
        lists(account.following_count, "following"),
    ]
    .into_iter()
    .flatten()
    .collect();
    lines.extend(joined(counts, Group::Stats));
    let relationship: Vec<&str> = [
        profile.follows_you.then_some("Follows you"),
        profile.is_following.then_some("Following"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !relationship.is_empty() {
        lines.push(Line::Text(vec![Span::new(
            relationship.join(" · "),
            Group::Time,
        )]));
    }
    if let Some(streaks) = profile.streaks.filter(|streaks| streaks.longest > 0) {
        let days = if streaks.current == 1 { "day" } else { "days" };
        lines.push(Line::Text(vec![Span::new(
            format!(
                "Streak {} {days} · longest {}",
                streaks.current, streaks.longest
            ),
            Group::Time,
        )]));
    }
    lines.push(Line::text(""));
    Content {
        lines,
        folds: Vec::new(),
        target: Some(Target::Account(AccountRef::new(
            account.handle.clone(),
            Some(AccountKind::User),
        ))),
    }
}

/// The head of an organization's page: who it is, its description and details, its followers and a link to the
/// commits mentioning it, its admins, and whether the connected account follows it. The block refers to it.
#[must_use]
pub fn organization_header(page: &OrganizationPage) -> Content {
    let organization = &page.organization;
    let mut lines = vec![Line::Text(who(
        &organization.name,
        &organization.handle,
        AccountKind::Organization,
    ))];
    lines.extend(text_lines(&organization.bio));
    lines.extend(details(&[&organization.location], &organization.website));
    let counts: Vec<(String, Option<String>)> = organization
        .followers_count
        .map(|count| (format!("{count} followers"), None))
        .into_iter()
        .chain([(
            "mentions".to_owned(),
            Some(format!("@{}/mentions", organization.handle)),
        )])
        .collect();
    lines.extend(joined(counts, Group::Stats));
    if !organization.admins.is_empty() {
        let mut spans = vec![Span::new("Admins:", Group::Time)];
        for admin in &organization.admins {
            spans.push(Span::new(" ", Group::Normal));
            spans.push(mention(&admin.handle, Some(AccountKind::User)));
        }
        lines.push(Line::Text(spans));
    }
    if page.is_following {
        lines.push(Line::Text(vec![Span::new("Following", Group::Time)]));
    }
    lines.push(Line::text(""));
    Content {
        lines,
        folds: Vec::new(),
        target: Some(Target::Account(AccountRef::new(
            organization.handle.clone(),
            Some(AccountKind::Organization),
        ))),
    }
}

/// A pinned commit: its block under a line saying it is pinned.
#[must_use]
pub fn pinned_block(commit: &Commit, now: u64) -> Content {
    let mut content = commit_block(commit, now);
    content
        .lines
        .insert(0, Line::Text(vec![Span::new("📌 Pinned", Group::Time)]));
    for fold in &mut content.folds {
        fold.lines = fold.lines.start + 1..fold.lines.end + 1;
    }
    content
}

/// A person in a list such as someone's followers: who they are and how they relate to the connected account, the
/// first line of their bio, and a blank line. The block refers to them; lists do not say whether an account is a
/// person or an organization.
#[must_use]
pub fn account_block(account: &Account) -> Content {
    let mut spans = who(&account.name, &account.handle, AccountKind::Other);
    let relationship: Vec<&str> = [
        account
            .follows_you
            .unwrap_or(false)
            .then_some("follows you"),
        account.is_followed.unwrap_or(false).then_some("following"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !relationship.is_empty() {
        spans.push(Span::new(
            format!(" · {}", relationship.join(" · ")),
            Group::Time,
        ));
    }
    let mut lines = vec![Line::Text(spans)];
    lines.extend(text_lines(&account.bio).into_iter().take(1));
    lines.push(Line::text(""));
    Content {
        lines,
        folds: Vec::new(),
        target: Some(Target::Account(AccountRef::new(
            account.handle.clone(),
            None,
        ))),
    }
}

/// The lines of a bio or description; none when it is empty.
fn text_lines(text: &str) -> Vec<Line> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(Line::text)
        .collect()
}

/// `company · location · website`, leaving out what is empty; the website is a link. No line when all are empty.
fn details(plain: &[&str], website: &str) -> Option<Line> {
    let mut parts: Vec<(String, Option<Target>)> = plain
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| ((*part).to_owned(), None))
        .collect();
    if !website.is_empty() {
        parts.push((website.to_owned(), Some(Target::Url(website.to_owned()))));
    }
    if parts.is_empty() {
        return None;
    }
    let mut spans = Vec::new();
    for (i, (text, target)) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::new(" · ", Group::Time));
        }
        spans.push(match target {
            Some(target) => Span::new(text, Group::Link).with_target(target),
            None => Span::new(text, Group::Time),
        });
    }
    Some(Line::Text(spans))
}

/// Parts joined with ` · `, each leading to the buffer named with it, if any; no line without parts.
fn joined(parts: Vec<(String, Option<String>)>, group: Group) -> Option<Line> {
    if parts.is_empty() {
        return None;
    }
    let mut spans = Vec::new();
    for (i, (text, buffer)) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::new(" · ", group));
        }
        let span = Span::new(text, group);
        spans.push(match buffer {
            Some(name) => span.with_target(Target::Buffer(name)),
            None => span,
        });
    }
    Some(Line::Text(spans))
}
