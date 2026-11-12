//! Feeds: buffers of what dev-null.sh sends. A feed has a head, such as a thread's commit and what it replies to,
//! then a list that comes a page at a time, such as the thread's replies. A log is a list alone.
//!
//! The head's blocks come first in the document, then the list's, then "Loading more…" while the next page loads.

use devnull_api::Log;
use devnull_api::model::{Cursor, Page};

use super::fetch::Request;
use super::{logs, threads};
use crate::doc::{Content, Document, Line, Span};
use crate::hl::Group;

/// The key of the block of a feed that says one thing, such as "Loading…", while it has nothing else to show, or
/// above a list whose head has not arrived.
pub(super) const NOTICE: &str = "notice";

/// The key of the "Loading more…" block at the end of a list while its next page loads.
const MORE: &str = "~more";

/// What a feed shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Source {
    /// A log; a list alone.
    Log(Log),
    /// The thread of a commit, by id: what the commit replies to and the commit, then its replies.
    Thread(String),
}

/// How a list is ordered, which decides where its items go when a fresh first page arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// Newest first: new items go above those shown.
    Newest,
    /// Oldest first: new items go after those shown.
    Oldest,
    /// Ranked: a fresh first page replaces the list.
    Ranked,
}

impl Source {
    /// The name of its buffer.
    pub(super) fn name(&self) -> String {
        match self {
            Self::Log(log) => logs::buffer_name(*log),
            Self::Thread(id) => threads::buffer_name(id),
        }
    }

    /// The request for the head, if the feed has one.
    fn head(&self) -> Option<Request> {
        match self {
            Self::Log(_) => None,
            Self::Thread(id) => Some(Request::Thread { id: id.clone() }),
        }
    }

    /// The request for the list's page starting `after`, or the first.
    fn page(&self, after: Option<Cursor>) -> Request {
        match self {
            Self::Log(log) => Request::Log { log: *log, after },
            Self::Thread(id) => Request::Replies {
                id: id.clone(),
                after,
            },
        }
    }

    fn order(&self) -> Order {
        match self {
            Self::Log(Log::Trending) => Order::Ranked,
            Self::Log(_) => Order::Newest,
            Self::Thread(_) => Order::Oldest,
        }
    }
}

/// Where loading a list a page at a time stands.
#[derive(Debug, Clone, Default)]
pub(super) struct Paging {
    /// Where the next page starts, once a page said there is one.
    next: Option<Cursor>,
    /// Whether the first page or the next page is on its way.
    loading: bool,
    /// Whether a fresh first page is on its way for a list that has one already.
    refreshing: bool,
    /// Whether a page ever arrived.
    loaded: bool,
}

impl Paging {
    /// Notes the first page as asked for, unless it arrived or is on its way: whether to ask for it.
    fn start(&mut self) -> bool {
        let ask = !self.loaded && !self.loading;
        self.loading |= ask;
        ask
    }

    /// Notes the first page as asked for again, unless it is on its way: whether to ask for it.
    fn refresh(&mut self) -> bool {
        if !self.loaded {
            return self.start();
        }
        let ask = !self.refreshing;
        self.refreshing = true;
        ask
    }

    /// Notes the next page as asked for: where it starts, unless there is none or it is on its way.
    fn more(&mut self) -> Option<Cursor> {
        let next = self.next.clone().filter(|_| !self.loading)?;
        self.loading = true;
        Some(next)
    }

    /// Notes a page as no longer on its way: the next one when `next_page`, else the first.
    fn answered(&mut self, next_page: bool) {
        if next_page || !self.loaded {
            self.loading = false;
        } else {
            self.refreshing = false;
        }
    }
}

/// What a feed's buffer knows beyond its document.
#[derive(Debug)]
pub(super) struct Feed {
    pub(super) source: Source,
    /// The keys of the head's blocks, top to bottom; empty until it arrives, and for a feed without a head.
    head: Vec<String>,
    /// Whether the head is on its way.
    head_loading: bool,
    list: Paging,
}

impl Feed {
    pub(super) fn new(source: Source) -> Self {
        Self {
            source,
            head: Vec::new(),
            head_loading: false,
            list: Paging::default(),
        }
    }

    /// The requests for what the feed shows, noted as asked for: those that did not arrive and are not on their
    /// way.
    pub(super) fn start(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        if let Some(head) = self.source.head()
            && self.head.is_empty()
            && !self.head_loading
        {
            self.head_loading = true;
            requests.push(head);
        }
        if self.list.start() {
            requests.push(self.source.page(None));
        }
        requests
    }

    /// As [`Feed::start`], also for what arrived already.
    pub(super) fn refresh(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        if let Some(head) = self.source.head()
            && !self.head_loading
        {
            self.head_loading = true;
            requests.push(head);
        }
        if self.list.refresh() {
            requests.push(self.source.page(None));
        }
        requests
    }

    /// Whether the list was loaded and is polled for news: chronological logs.
    pub(super) fn is_polled(&self) -> bool {
        matches!(self.source, Source::Log(log) if log != Log::Trending) && self.list.loaded
    }

    /// Whether any of the feed arrived.
    pub(super) fn has_content(&self) -> bool {
        !self.head.is_empty() || self.list.loaded
    }

    /// Whether there is a next page that is not on its way yet.
    pub(super) fn has_more(&self) -> bool {
        !self.list.loading && self.list.next.is_some()
    }

    /// The request for the next page, noted as asked for and shown at the end of `doc`; `None` when there is no
    /// next page or it is on its way.
    pub(super) fn more(&mut self, doc: &mut Document) -> Option<Request> {
        let after = self.list.more()?;
        doc.push_back(
            MORE,
            vec![Line::Text(vec![Span::new("Loading more…", Group::Time)])],
        );
        Some(self.source.page(Some(after)))
    }

    /// Notes the head as no longer on its way.
    pub(super) fn head_answered(&mut self) {
        self.head_loading = false;
    }

    /// The head arrived as `blocks`, top to bottom: they take the place of the head shown, if any, above the list.
    pub(super) fn head_arrived(&mut self, doc: &mut Document, blocks: Vec<(String, Content)>) {
        let keys: Vec<String> = blocks.iter().map(|(key, _)| key.clone()).collect();
        doc.remove(NOTICE);
        if keys == self.head {
            // The same blocks: updated in place, so folds keep the state they were given.
            for (key, content) in blocks {
                let _ = doc.replace(key.as_str(), content);
            }
        } else {
            for key in &self.head {
                doc.remove(key.as_str());
            }
            for (key, content) in blocks.into_iter().rev() {
                if !doc.contains(key.as_str()) {
                    doc.push_front(key.as_str(), content);
                }
            }
        }
        self.head = keys;
    }

    /// Notes a page as no longer on its way: the next one when `next_page`, else the first.
    pub(super) fn page_answered(&mut self, doc: &mut Document, next_page: bool) {
        self.list.answered(next_page);
        if next_page {
            doc.remove(MORE);
        }
    }

    /// A page of the list arrived, as blocks: the next one when `next_page`, else the first. Returns how many new
    /// items went above those shown.
    pub(super) fn page_arrived(
        &mut self,
        doc: &mut Document,
        next_page: bool,
        page: Page<(String, Content)>,
    ) -> usize {
        // Once the head arrived, or for a feed without one, nothing is loading any more.
        if self.source.head().is_none() || !self.head.is_empty() {
            doc.remove(NOTICE);
        }
        let mut arrived = 0;
        let known = page
            .items
            .iter()
            .position(|(key, _)| doc.contains(key.as_str()));
        match (next_page, self.source.order(), known) {
            (true, ..) => {
                append(doc, page.items);
                self.list.next = page.next;
            }
            (false, Order::Oldest, _) => {
                for (key, content) in &page.items {
                    let _ = doc.replace(key.as_str(), content.clone());
                }
                append(doc, page.items);
                // Once further pages arrived, the next one is where they left off. Asking for one of them again is
                // harmless: items shown already are not added twice.
                self.list.next = self.list.next.take().or(page.next);
            }
            (false, Order::Newest, Some(first_known)) if self.list.loaded => {
                let mut new = page.items;
                let known = new.split_off(first_known);
                let first_known = known[0].0.clone();
                for (key, content) in new {
                    // Should a page hold an item twice, it is shown once.
                    if !doc.contains(key.as_str()) {
                        doc.insert_before(first_known.as_str(), key.as_str(), content);
                        arrived += 1;
                    }
                }
                for (key, content) in known {
                    let _ = doc.replace(key.as_str(), content);
                }
            }
            // A first load, a ranked list, or nothing in common with what is shown: the page replaces the list.
            // Cursors stay on items that are still there, since they are anchored by key.
            _ => {
                let head = &self.head;
                doc.retain(|key| key == NOTICE || head.iter().any(|kept| kept == key));
                append(doc, page.items);
                self.list.next = page.next;
            }
        }
        if doc.is_empty() {
            doc.push_back(NOTICE, notice_lines("Nothing here yet.", Group::Time));
        }
        self.list.loaded = true;
        arrived
    }
}

/// A document saying one thing, while there is nothing else to show.
pub(super) fn notice(text: &str, group: Group) -> Document {
    let mut doc = Document::new();
    doc.push_back(NOTICE, notice_lines(text, group));
    doc
}

/// The lines of a notice.
pub(super) fn notice_lines(text: &str, group: Group) -> Vec<Line> {
    vec![Line::Text(vec![Span::new(text, group)])]
}

/// Appends the blocks that `doc` does not show yet.
fn append(doc: &mut Document, blocks: Vec<(String, Content)>) {
    for (key, content) in blocks {
        if !doc.contains(key.as_str()) {
            doc.push_back(key.as_str(), content);
        }
    }
}
