//! What buffers ask dev-null.sh for, and how the answers reach them. Lists, such as logs and replies, come a page at
//! a time.

use devnull_api::Log;
use devnull_api::model::{Commit, Cursor, Page, Thread};

use super::{App, Effect, Kind, NetEvent, logs, threads};
use crate::doc::{Document, Layout, Line, Span};
use crate::hl::Group;

/// The key of the "Loading more…" block at the end of a list while its next page loads.
const MORE: &str = "~more";

/// The key of the block of a buffer that says one thing, while it has nothing else to show.
pub(super) const NOTICE: &str = "notice";

/// Something to ask dev-null.sh for, on the app's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Request {
    /// A page of a log: the first, or the one after `after`.
    Log {
        /// Which log.
        log: Log,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// A commit and the commits it replies to.
    Thread {
        /// The commit's id.
        id: String,
    },
    /// A page of a commit's replies: the first, or the one after `after`.
    Replies {
        /// The commit's id.
        id: String,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fetched {
    /// The answer to [`Request::Log`].
    Log {
        /// Which log.
        log: Log,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Commit>, FetchError>,
    },
    /// The answer to [`Request::Thread`].
    Thread {
        /// The commit's id.
        id: String,
        /// The commit and what it replies to, or why there are none. Boxed: commits are large.
        result: Result<Box<Thread>, FetchError>,
    },
    /// The answer to [`Request::Replies`].
    Replies {
        /// The commit's id.
        id: String,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Commit>, FetchError>,
    },
}

/// Why a request failed, by what the app does about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchError {
    /// The device is no longer approved: back to connecting.
    NotApproved,
    /// Anything else, with the message to show.
    Failed(String),
}

/// Where loading a list a page at a time stands.
#[derive(Debug, Clone, Default)]
pub(super) struct Paging {
    /// Where the next page starts, once a page said there is one.
    pub(super) next: Option<Cursor>,
    /// Whether the first page or the next page is on its way.
    loading: bool,
    /// Whether a fresh first page is on its way for a list that has one already.
    refreshing: bool,
    /// Whether a page ever arrived.
    pub(super) loaded: bool,
}

impl Paging {
    /// Notes the first page as asked for, unless it arrived or is on its way: whether to ask for it.
    pub(super) fn start(&mut self) -> bool {
        let ask = !self.loaded && !self.loading;
        self.loading |= ask;
        ask
    }

    /// Notes the first page as asked for again, unless it is on its way: whether to ask for it.
    pub(super) fn refresh(&mut self) -> bool {
        if !self.loaded {
            return self.start();
        }
        let ask = !self.refreshing;
        self.refreshing = true;
        ask
    }

    /// Whether there is a next page and it is not on its way yet.
    fn has_more(&self) -> bool {
        !self.loading && self.next.is_some()
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

impl Kind {
    /// The list the buffer shows a page at a time, if any.
    fn paging(&self) -> Option<&Paging> {
        match self {
            Self::Log { paging, .. } => Some(paging),
            Self::Thread(thread) => Some(&thread.replies),
            Self::Status | Self::Fixed => None,
        }
    }

    fn paging_mut(&mut self) -> Option<&mut Paging> {
        match self {
            Self::Log { paging, .. } => Some(paging),
            Self::Thread(thread) => Some(&mut thread.replies),
            Self::Status | Self::Fixed => None,
        }
    }

    /// The requests for what the buffer shows, noted as asked for: none when it has it or is getting it.
    fn start(&mut self) -> Vec<Request> {
        match self {
            Self::Log { log, paging } => paging
                .start()
                .then_some(Request::Log {
                    log: *log,
                    after: None,
                })
                .into_iter()
                .collect(),
            Self::Thread(thread) => thread.start(),
            Self::Status | Self::Fixed => Vec::new(),
        }
    }

    /// The requests for what the buffer shows, again; `None` when it shows nothing from the server.
    fn refresh(&mut self) -> Option<Vec<Request>> {
        Some(match self {
            Self::Log { log, paging } => paging
                .refresh()
                .then_some(Request::Log {
                    log: *log,
                    after: None,
                })
                .into_iter()
                .collect(),
            Self::Thread(thread) => thread.refresh(),
            Self::Status | Self::Fixed => return None,
        })
    }

    /// The request for the next page, noted as asked for, unless there is none or it is on its way.
    fn more(&mut self) -> Option<Request> {
        match self {
            Self::Log { log, paging } => paging.more().map(|after| Request::Log {
                log: *log,
                after: Some(after),
            }),
            Self::Thread(thread) => thread.replies.more().map(|after| Request::Replies {
                id: thread.id.clone(),
                after: Some(after),
            }),
            Self::Status | Self::Fixed => None,
        }
    }

    /// Whether the buffer shows anything from the server yet.
    fn has_content(&self) -> bool {
        match self {
            Self::Log { paging, .. } => paging.loaded,
            Self::Thread(thread) => thread.has_content(),
            Self::Status | Self::Fixed => true,
        }
    }
}

/// A document saying one thing, while there is nothing else to show.
pub(super) fn notice(text: &str, group: Group) -> Document {
    let mut doc = Document::new();
    doc.push_back(NOTICE, notice_lines(text, group));
    doc
}

fn notice_lines(text: &str, group: Group) -> Vec<Line> {
    vec![Line::Text(vec![Span::new(text, group)])]
}

/// Appends the commits of `page` that `doc` does not show yet.
pub(super) fn append(doc: &mut Document, page: &Page<Commit>, now: u64) {
    for commit in &page.items {
        if !doc.contains(commit.id.as_str()) {
            doc.push_back(commit.id.as_str(), crate::render::commit_block(commit, now));
        }
    }
}

impl App {
    /// Asks for what buffer `index` shows, unless it has it or is getting it.
    pub(super) fn load(&mut self, index: usize) -> Vec<Effect> {
        let requests = self.buffers[index].kind.start();
        self.ask(index, requests)
    }

    /// `:e!`: asks for what buffer `index` shows again, keeping what is shown until it arrives. `None` when the
    /// buffer shows nothing from the server.
    pub(super) fn refresh(&mut self, index: usize) -> Option<Vec<Effect>> {
        let requests = self.buffers[index].kind.refresh()?;
        Some(self.ask(index, requests))
    }

    /// Sends `requests` for buffer `index`, saying it is loading while it has nothing else to show.
    fn ask(&mut self, index: usize, requests: Vec<Request>) -> Vec<Effect> {
        let buffer = &mut self.buffers[index];
        if !buffer.kind.has_content() {
            buffer.doc = notice(&format!("Loading {}…", buffer.name), Group::Time);
        }
        requests.into_iter().map(Effect::Fetch).collect()
    }

    /// Asks for the next page of the current window's list when its cursor is within a window's height of the end.
    pub(super) fn load_more(&mut self) -> Vec<Effect> {
        let area = self.window_area();
        let window = self.windows.current();
        let index = window.buffer;
        let buffer = &self.buffers[index];
        if !buffer.kind.paging().is_some_and(Paging::has_more) {
            return Vec::new();
        }
        let layout = Layout::new(&buffer.doc, area.width);
        if window.cursor.screen(&layout).row + usize::from(area.height) < layout.row_count() {
            return Vec::new();
        }
        let buffer = &mut self.buffers[index];
        let Some(request) = buffer.kind.more() else {
            return Vec::new();
        };
        buffer.doc.push_back(
            MORE,
            vec![Line::Text(vec![Span::new("Loading more…", Group::Time)])],
        );
        vec![Effect::Fetch(request)]
    }

    /// An answer from the server; `now` (Unix seconds) is when it arrived, for ages.
    pub(super) fn fetched(&mut self, fetched: Fetched, now: u64) -> Vec<Effect> {
        match fetched {
            Fetched::Log { log, after, result } => {
                let index = self.log_buffer(log);
                self.page_arrived(index, after.is_some(), result, now)
            }
            Fetched::Replies { id, after, result } => match self.thread_buffer(&id) {
                Some(index) => self.page_arrived(index, after.is_some(), result, now),
                None => Vec::new(),
            },
            Fetched::Thread { id, result } => self.thread_arrived(&id, result, now),
        }
    }

    /// A page of the list of buffer `index` arrived, or failed to: the next page when `next_page`, else the first.
    fn page_arrived(
        &mut self,
        index: usize,
        next_page: bool,
        result: Result<Page<Commit>, FetchError>,
        now: u64,
    ) -> Vec<Effect> {
        let top_before = self.top_row(index);
        let buffer = &mut self.buffers[index];
        let Some(paging) = buffer.kind.paging_mut() else {
            return Vec::new();
        };
        paging.answered(next_page);
        if next_page {
            buffer.doc.remove(MORE);
        }
        let page = match result {
            Ok(page) => page,
            Err(error) => return self.failed(index, error),
        };
        let mut arrived = 0;
        if next_page {
            append(&mut buffer.doc, &page, now);
            if let Some(paging) = buffer.kind.paging_mut() {
                paging.next = page.next;
            }
        } else {
            match &mut buffer.kind {
                Kind::Log { log, paging } => {
                    arrived = logs::merge(&mut buffer.doc, *log, paging, page, now);
                }
                Kind::Thread(thread) => threads::merge(&mut buffer.doc, thread, page, now),
                Kind::Status | Kind::Fixed => {}
            }
        }
        if buffer.doc.is_empty() {
            buffer.doc = notice("Nothing here yet.", Group::Time);
        }
        if let Some(paging) = buffer.kind.paging_mut() {
            paging.loaded = true;
        }
        if arrived > 0 && top_before.is_some_and(|top| top > 0) {
            self.info(format!(
                "{arrived} new commit{}",
                if arrived == 1 { "" } else { "s" }
            ));
        }
        Vec::new()
    }

    /// A request for buffer `index` failed: says why, also in the buffer while it has nothing else to show.
    pub(super) fn failed(&mut self, index: usize, error: FetchError) -> Vec<Effect> {
        match error {
            FetchError::NotApproved => self.net(NetEvent::NotApproved),
            FetchError::Failed(reason) => {
                let buffer = &mut self.buffers[index];
                let text = format!("Cannot load {}: {reason}", buffer.name);
                if !buffer.kind.has_content() {
                    buffer.doc = notice(&text, Group::ErrorMsg);
                } else if buffer.doc.contains(NOTICE) {
                    // Such as "Loading…" above replies that arrived when their commit did not.
                    let _ = buffer
                        .doc
                        .replace(NOTICE, notice_lines(&text, Group::ErrorMsg));
                }
                self.error(text);
                Vec::new()
            }
        }
    }

    /// The first row shown by the current window when it shows buffer `index`.
    fn top_row(&self, index: usize) -> Option<usize> {
        let window = self.windows.current();
        (window.buffer == index).then(|| {
            let layout = Layout::new(&self.buffers[index].doc, self.window_area().width);
            window.top(&layout, self.viewport())
        })
    }
}
