//! `K`: a card about what is under the cursor, floating next to it until the next key.

use devnull_api::model::AccountPage;

use super::feed::{Source, notice};
use super::{Dn, Effects, FetchError, Kit, NetEvent, Request, fetch};
use crate::doc::{Document, Line, LinkCard, Span, Target};
use crate::render::{organization_header, profile_header};

/// A card of `lines`.
fn card(lines: Vec<Line>) -> Document {
    let mut doc = Document::new();
    doc.push_back("card", lines);
    doc
}

/// A link card: title, site, what the page is about, and the link.
fn link_card(card: &LinkCard) -> Vec<Line> {
    let mut lines = Vec::new();
    if !card.title.is_empty() {
        lines.push(Line::Text(vec![Span::new(
            card.title.clone(),
            crate::hl::AUTHOR,
        )]));
    }
    if !card.site.is_empty() {
        lines.push(Line::Text(vec![Span::new(
            card.site.clone(),
            crate::hl::TIME,
        )]));
    }
    lines.extend(
        card.description
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(Line::text),
    );
    lines.push(link(&card.url));
    lines
}

fn link(url: &str) -> Line {
    Line::Text(vec![
        Span::new(url, crate::hl::LINK).with_target(Target::Url(url.to_owned())),
    ])
}

/// An account's card: the head of its profile, without the blank line closing it.
fn account_card(page: &AccountPage) -> Option<Vec<Line>> {
    let mut content = match page {
        AccountPage::Person(profile) => profile_header(profile),
        AccountPage::Organization(page) => organization_header(page),
        _ => return None,
    };
    content.lines.pop();
    Some(content.lines)
}

impl Dn {
    /// `:details` (`K`), as muttrs' and slackers': the card of the link under the cursor, else of the account, else of the author of the commit; an
    /// account's card waits for its profile to arrive.
    pub(super) fn hover(&mut self, kit: &mut Kit) -> Result<Effects, String> {
        self.hover_waiting = None;
        let account = match kit.target_under_cursor() {
            Some(Target::Url(url)) => {
                kit.show_hover(card(vec![link(&url)]));
                return Ok(Vec::new());
            }
            Some(Target::LinkCard(link_card_of)) => {
                kit.show_hover(card(link_card(&link_card_of)));
                return Ok(Vec::new());
            }
            Some(Target::Account(account)) => account,
            Some(Target::Commit {
                author: Some(author),
                ..
            }) => author,
            _ => return Err("E349: No identifier under cursor".to_owned()),
        };
        kit.show_hover(notice(
            &format!("Loading @{}…", account.handle),
            crate::hl::TIME,
        ));
        self.hover_waiting = Some(account.handle.clone());
        Ok(vec![fetch(Request::Profile {
            handle: account.handle,
            kind: account.kind,
        })])
    }

    /// A profile arrived, or failed to: the card waiting for it shows it, or goes. Says why it failed when no
    /// profile buffer will.
    pub(super) fn hover_arrived(
        &mut self,
        kit: &mut Kit,
        handle: &str,
        result: &Result<AccountPage, FetchError>,
    ) -> Effects {
        if self.hover_waiting.as_deref() != Some(handle) {
            return Vec::new();
        }
        self.hover_waiting = None;
        let Some(hover) = kit.hover_mut() else {
            return Vec::new();
        };
        if let Ok(page) = result
            && let Some(lines) = account_card(page)
        {
            *hover = card(lines);
            return Vec::new();
        }
        kit.close_hover();
        let source = Source::Profile {
            handle: handle.to_owned(),
            kind: None,
        };
        if self.feed_buffer(&source).is_some() {
            return Vec::new();
        }
        match result {
            Err(FetchError::NotApproved) => self.net(kit, NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                kit.error(format!("Cannot load @{handle}: {reason}"));
                Vec::new()
            }
            Ok(_) => Vec::new(),
        }
    }
}
