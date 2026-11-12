//! What feeds ask dev-null.sh for, and how the answers reach their buffers.

use devnull_api::Log;
use devnull_api::model::{Commit, Cursor, Page, Thread};

use super::feed::{Feed, NOTICE, Source, notice, notice_lines};
use super::{App, Buffer, Effect, Kind, NetEvent, threads};
use crate::doc::{Content, Cursor as DocCursor, Document, Layout};
use crate::hl::Group;
use crate::render::commit_block;

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

impl App {
    /// The buffer showing `source`, if there is one.
    pub(super) fn feed_buffer(&self, source: &Source) -> Option<usize> {
        self.buffers
            .iter()
            .position(|buffer| matches!(&buffer.kind, Kind::Feed(feed) if feed.source == *source))
    }

    /// The buffer showing `source`, created empty if there is none yet.
    pub(super) fn feed_buffer_or_new(&mut self, source: Source) -> usize {
        self.feed_buffer(&source).unwrap_or_else(|| {
            self.buffers.push(Buffer {
                name: source.name(),
                doc: Document::new(),
                kind: Kind::Feed(Feed::new(source)),
            });
            self.buffers.len() - 1
        })
    }

    /// Shows `source` in the current window, loading it if needed; the cursor starts at `start` when the window
    /// never showed it.
    pub(super) fn open_feed(&mut self, source: Source, start: DocCursor) -> Vec<Effect> {
        let index = self.feed_buffer_or_new(source);
        self.show_buffer_at(index, start)
    }

    /// Asks for what buffer `index` shows, unless it has it or is getting it.
    pub(super) fn load(&mut self, index: usize) -> Vec<Effect> {
        let requests = match &mut self.buffers[index].kind {
            Kind::Feed(feed) => feed.start(),
            Kind::Status | Kind::Fixed => Vec::new(),
        };
        self.ask(index, requests)
    }

    /// `:e!`: asks for what buffer `index` shows again, keeping what is shown until it arrives. `None` when the
    /// buffer shows nothing from the server.
    pub(super) fn refresh(&mut self, index: usize) -> Option<Vec<Effect>> {
        let requests = match &mut self.buffers[index].kind {
            Kind::Feed(feed) => feed.refresh(),
            Kind::Status | Kind::Fixed => return None,
        };
        Some(self.ask(index, requests))
    }

    /// Sends `requests` for buffer `index`, saying it is loading while it has nothing else to show.
    fn ask(&mut self, index: usize, requests: Vec<Request>) -> Vec<Effect> {
        let buffer = &mut self.buffers[index];
        if let Kind::Feed(feed) = &buffer.kind
            && !feed.has_content()
        {
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
        if !matches!(&buffer.kind, Kind::Feed(feed) if feed.has_more()) {
            return Vec::new();
        }
        let layout = Layout::new(&buffer.doc, area.width);
        if window.cursor.screen(&layout).row + usize::from(area.height) < layout.row_count() {
            return Vec::new();
        }
        let buffer = &mut self.buffers[index];
        let Kind::Feed(feed) = &mut buffer.kind else {
            return Vec::new();
        };
        feed.more(&mut buffer.doc)
            .map(Effect::Fetch)
            .into_iter()
            .collect()
    }

    /// An answer from the server; `now` (Unix seconds) is when it arrived, for ages.
    pub(super) fn fetched(&mut self, fetched: Fetched, now: u64) -> Vec<Effect> {
        let commits = |page: Page<Commit>| {
            let items = page
                .items
                .iter()
                .map(|commit| (commit.id.clone(), commit_block(commit, now)))
                .collect();
            Page::new(items, page.next)
        };
        match fetched {
            Fetched::Log { log, after, result } => {
                self.page_arrived(&Source::Log(log), after.is_some(), result.map(commits))
            }
            Fetched::Replies { id, after, result } => {
                self.page_arrived(&Source::Thread(id), after.is_some(), result.map(commits))
            }
            Fetched::Thread { id, result } => self.head_arrived(
                &Source::Thread(id),
                result.map(|thread| threads::head(&thread, now)),
            ),
        }
    }

    /// The head of the feed of `source` arrived as blocks, or failed to.
    fn head_arrived(
        &mut self,
        source: &Source,
        result: Result<Vec<(String, Content)>, FetchError>,
    ) -> Vec<Effect> {
        let Some(index) = self.feed_buffer(source) else {
            return Vec::new();
        };
        let buffer = &mut self.buffers[index];
        let Kind::Feed(feed) = &mut buffer.kind else {
            return Vec::new();
        };
        feed.head_answered();
        match result {
            Ok(blocks) => {
                feed.head_arrived(&mut buffer.doc, blocks);
                Vec::new()
            }
            Err(error) => self.failed(index, error),
        }
    }

    /// A page of the list of the feed of `source` arrived as blocks, or failed to: the next page when `next_page`,
    /// else the first.
    fn page_arrived(
        &mut self,
        source: &Source,
        next_page: bool,
        result: Result<Page<(String, Content)>, FetchError>,
    ) -> Vec<Effect> {
        let Some(index) = self.feed_buffer(source) else {
            return Vec::new();
        };
        let top_before = self.top_row(index);
        let buffer = &mut self.buffers[index];
        let Kind::Feed(feed) = &mut buffer.kind else {
            return Vec::new();
        };
        feed.page_answered(&mut buffer.doc, next_page);
        let page = match result {
            Ok(page) => page,
            Err(error) => return self.failed(index, error),
        };
        let arrived = feed.page_arrived(&mut buffer.doc, next_page, page);
        if arrived > 0 && top_before.is_some_and(|top| top > 0) {
            self.info(format!(
                "{arrived} new commit{}",
                if arrived == 1 { "" } else { "s" }
            ));
        }
        Vec::new()
    }

    /// A request for buffer `index` failed: says why, also in the buffer while it has nothing else to show.
    fn failed(&mut self, index: usize, error: FetchError) -> Vec<Effect> {
        match error {
            FetchError::NotApproved => self.net(NetEvent::NotApproved),
            FetchError::Failed(reason) => {
                let buffer = &mut self.buffers[index];
                let text = format!("Cannot load {}: {reason}", buffer.name);
                if matches!(&buffer.kind, Kind::Feed(feed) if !feed.has_content()) {
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
