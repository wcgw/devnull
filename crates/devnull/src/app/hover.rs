//! `K`: a card about what is under the cursor, floating next to it until the next key.

use devnull_api::model::AccountPage;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line as TextLine, Span as TextSpan};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::feed::{Source, notice};
use super::{App, Effect, FetchError, NetEvent, Request};
use crate::doc::{Document, Layout, Line, LinkCard, Span, Target};
use crate::hl::Group;
use crate::render::{organization_header, profile_header};

/// The widest a card's text is, in cells, before it wraps.
const MAX_WIDTH: u16 = 60;

/// A card shown by `K`.
#[derive(Debug)]
pub(super) struct Hover {
    /// What the card says.
    doc: Document,
    /// The handle whose profile the card waits for, if any.
    waiting: Option<String>,
}

impl Hover {
    /// What the card says.
    pub(super) fn doc(&self) -> &Document {
        &self.doc
    }

    fn new(lines: Vec<Line>) -> Self {
        let mut doc = Document::new();
        doc.push_back("card", lines);
        Self { doc, waiting: None }
    }
}

/// A link card: title, site, what the page is about, and the link.
fn link_card(card: &LinkCard) -> Vec<Line> {
    let mut lines = Vec::new();
    if !card.title.is_empty() {
        lines.push(Line::Text(vec![Span::new(
            card.title.clone(),
            Group::Author,
        )]));
    }
    if !card.site.is_empty() {
        lines.push(Line::Text(vec![Span::new(card.site.clone(), Group::Time)]));
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
        Span::new(url, Group::Link).with_target(Target::Url(url.to_owned())),
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

impl App {
    /// `:hover`: the card of the link under the cursor, else of the account, else of the author of the commit; an
    /// account's card waits for its profile to arrive.
    pub(super) fn hover(&mut self) -> Result<Vec<Effect>, String> {
        let account = match self.target_under_cursor() {
            Some(Target::Url(url)) => {
                self.hover = Some(Hover::new(vec![link(&url)]));
                return Ok(Vec::new());
            }
            Some(Target::LinkCard(card)) => {
                self.hover = Some(Hover::new(link_card(&card)));
                return Ok(Vec::new());
            }
            Some(Target::Account(account)) => account,
            Some(Target::Commit {
                author: Some(author),
                ..
            }) => author,
            _ => return Err("E349: No identifier under cursor".to_owned()),
        };
        self.hover = Some(Hover {
            doc: notice(&format!("Loading @{}…", account.handle), Group::Time),
            waiting: Some(account.handle.clone()),
        });
        Ok(vec![Effect::Fetch(Request::Profile {
            handle: account.handle,
            kind: account.kind,
        })])
    }

    /// A profile arrived, or failed to: the card waiting for it shows it, or goes. Says why it failed when no
    /// profile buffer will.
    pub(super) fn hover_arrived(
        &mut self,
        handle: &str,
        result: &Result<AccountPage, FetchError>,
    ) -> Vec<Effect> {
        let Some(hover) = self
            .hover
            .as_mut()
            .filter(|hover| hover.waiting.as_deref() == Some(handle))
        else {
            return Vec::new();
        };
        if let Ok(page) = result
            && let Some(lines) = account_card(page)
        {
            *hover = Hover::new(lines);
            return Vec::new();
        }
        self.hover = None;
        let source = Source::Profile {
            handle: handle.to_owned(),
            kind: None,
        };
        if self.feed_buffer(&source).is_some() {
            return Vec::new();
        }
        match result {
            Err(FetchError::NotApproved) => self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                self.error(format!("Cannot load @{handle}: {reason}"));
                Vec::new()
            }
            Ok(_) => Vec::new(),
        }
    }

    /// Draws the card, if any, just below the cursor at `cursor`, or above it when there is more room there.
    pub(super) fn render_hover(&self, frame: &mut Frame<'_>, cursor: Position) {
        let Some(hover) = &self.hover else { return };
        let (width, height) = self.size;
        let max_inner = MAX_WIDTH.min(width.saturating_sub(2));
        // Rows between the bufferline and the command line.
        let (first, last) = (1, height.saturating_sub(2));
        if max_inner < 4 || last < first + 2 {
            return;
        }
        let natural = Layout::new(&hover.doc, max_inner);
        let inner_width = (0..natural.row_count())
            .map(|row| {
                natural
                    .segments(row)
                    .map(|(text, _)| text.width())
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .clamp(1, usize::from(max_inner));
        let inner_width = u16::try_from(inner_width).unwrap_or(max_inner);
        let layout = Layout::new(&hover.doc, inner_width);
        let needed = u16::try_from(layout.row_count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let below = (last + 1).saturating_sub(cursor.y + 1);
        let above = cursor.y.saturating_sub(first);
        let (y, card_height) = if needed <= below || below >= above {
            (cursor.y + 1, needed.min(below))
        } else {
            let card_height = needed.min(above);
            (cursor.y - card_height, card_height)
        };
        if card_height < 3 {
            return;
        }
        let card_width = inner_width + 2;
        let x = cursor.x.min(width.saturating_sub(card_width));
        let area = Rect::new(x, y, card_width, card_height);
        let block = Block::bordered()
            .border_style(self.theme.style(Group::FloatBorder))
            .style(self.theme.style(Group::NormalFloat));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let rows: Vec<TextLine<'_>> = (0..layout.row_count().min(usize::from(inner.height)))
            .map(|row| {
                TextLine::from(
                    layout
                        .segments(row)
                        .map(|(text, group)| {
                            // Plain text takes the card's colours, not the window's.
                            let group = if group == Group::Normal {
                                Group::NormalFloat
                            } else {
                                group
                            };
                            TextSpan::styled(text, self.theme.style(group))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(rows), inner);
        self.draw_pictures(frame.buffer_mut(), &layout, 0, inner);
    }
}
