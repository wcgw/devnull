//! The log buffers: what they ask the server for and how answers become their documents.

use std::time::Duration;

use devnull_api::Log;
use devnull_api::model::{Commit, Cursor, Page};

use super::{App, Buffer, Effect, Wake, WakePurpose};
use crate::doc::{Document, Layout, Line, Span};
use crate::hl::Group;
use crate::render::commit_block;

/// How often chronological logs are checked for new commits while the terminal has focus, and without it.
const POLL_FOCUSED: Duration = Duration::from_secs(15);
const POLL_UNFOCUSED: Duration = Duration::from_secs(30);

/// The key of the "Loading more…" block at the end of a log while its next page loads.
const MORE: &str = "~more";

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

/// What a log buffer knows beyond its document.
#[derive(Debug, Clone)]
pub(super) struct LogState {
    pub(super) log: Log,
    /// Where the next page starts, once a page said there is one.
    next: Option<Cursor>,
    /// Whether the first page or the next page is on its way.
    loading: bool,
    /// Whether a fresh first page is on its way for a log that has one already.
    refreshing: bool,
    /// Whether a page ever arrived.
    loaded: bool,
}

impl LogState {
    pub(super) fn new(log: Log) -> Self {
        Self {
            log,
            next: None,
            loading: false,
            refreshing: false,
            loaded: false,
        }
    }
}

/// A log's buffer name: `log/following` and so on.
pub(super) fn buffer_name(log: Log) -> String {
    format!("log/{}", log.name())
}

/// The log a buffer name names.
pub(super) fn named(name: &str) -> Option<Log> {
    [Log::Following, Log::Latest, Log::Trending]
        .into_iter()
        .find(|log| buffer_name(*log) == name)
}

/// A buffer saying one thing, while there is nothing else to show.
pub(super) fn notice(text: &str, group: Group) -> Document {
    let mut doc = Document::new();
    doc.push_back("notice", vec![Line::Text(vec![Span::new(text, group)])]);
    doc
}

impl App {
    /// The buffer of `log`, created empty if it does not exist yet.
    pub(super) fn log_buffer(&mut self, log: Log) -> usize {
        let name = buffer_name(log);
        if let Some(index) = self.buffers.iter().position(|buffer| buffer.name == name) {
            return index;
        }
        self.buffers.push(Buffer {
            name,
            doc: Document::new(),
            log: Some(LogState::new(log)),
            listed: true,
        });
        self.buffers.len() - 1
    }

    /// Asks for the first page of the log shown in buffer `index`, unless it has one or is getting one.
    pub(super) fn load_log(&mut self, index: usize) -> Vec<Effect> {
        let buffer = &mut self.buffers[index];
        let Some(state) = buffer
            .log
            .as_mut()
            .filter(|state| !state.loaded && !state.loading)
        else {
            return Vec::new();
        };
        state.loading = true;
        let log = state.log;
        buffer.doc = notice(&format!("Loading {}…", buffer_name(log)), Group::Time);
        vec![Effect::Fetch(Request::Log { log, after: None })]
    }

    /// `:e!`: asks for the first page again, keeping what is shown until it arrives. `None` when `index` is not a
    /// log's buffer.
    pub(super) fn refresh(&mut self, index: usize) -> Option<Vec<Effect>> {
        let state = self.buffers[index].log.as_mut()?;
        if !state.loaded {
            return Some(self.load_log(index));
        }
        if state.refreshing {
            return Some(Vec::new());
        }
        state.refreshing = true;
        Some(vec![Effect::Fetch(Request::Log {
            log: state.log,
            after: None,
        })])
    }

    /// Asks for the next page of the current window's log when its cursor is within a window's height of the end.
    pub(super) fn load_more(&mut self) -> Vec<Effect> {
        let area = self.window_area();
        let window = self.windows.current();
        let index = window.buffer;
        let buffer = &self.buffers[index];
        let Some(state) = buffer.log.as_ref().filter(|state| !state.loading) else {
            return Vec::new();
        };
        let Some(next) = state.next.clone() else {
            return Vec::new();
        };
        let layout = Layout::new(&buffer.doc, area.width);
        if window.cursor.screen(&layout).row + usize::from(area.height) < layout.row_count() {
            return Vec::new();
        }
        let log = state.log;
        let buffer = &mut self.buffers[index];
        if let Some(state) = buffer.log.as_mut() {
            state.loading = true;
        }
        buffer.doc.push_back(
            MORE,
            vec![Line::Text(vec![Span::new("Loading more…", Group::Time)])],
        );
        vec![Effect::Fetch(Request::Log {
            log,
            after: Some(next),
        })]
    }

    /// Starts polling chronological logs for new commits, once.
    pub(super) fn start_polling(&mut self) -> Vec<Effect> {
        if self.polling {
            return Vec::new();
        }
        self.polling = true;
        vec![self.next_poll()]
    }

    /// The wake-up for the next poll, sooner while the terminal has focus.
    fn next_poll(&self) -> Effect {
        Effect::Wake {
            after: if self.focused {
                POLL_FOCUSED
            } else {
                POLL_UNFOCUSED
            },
            token: Wake {
                purpose: WakePurpose::Poll,
                generation: self.poll_generation,
            },
        }
    }

    /// Time to poll: refresh every loaded chronological log, and plan the next poll.
    pub(super) fn poll(&mut self) -> Vec<Effect> {
        let chronological: Vec<usize> = (0..self.buffers.len())
            .filter(|&index| matches!(&self.buffers[index].log, Some(state) if state.log != Log::Trending && state.loaded))
            .collect();
        let mut effects: Vec<Effect> = chronological
            .into_iter()
            .filter_map(|index| self.refresh(index))
            .flatten()
            .collect();
        effects.push(self.next_poll());
        effects
    }

    /// An answer from the server; `now` (Unix seconds) is when it arrived, for ages.
    pub(super) fn fetched(&mut self, fetched: Fetched, now: u64) -> Vec<Effect> {
        let Fetched::Log { log, after, result } = fetched;
        let index = self.log_buffer(log);
        let top_before = self.top_row(index);
        let buffer = &mut self.buffers[index];
        let Some(state) = buffer.log.as_mut() else {
            return Vec::new();
        };
        if after.is_some() {
            state.loading = false;
            buffer.doc.remove(MORE);
        } else if state.loaded {
            state.refreshing = false;
        } else {
            state.loading = false;
        }
        let page = match result {
            Ok(page) => page,
            Err(FetchError::NotApproved) => return self.net(super::NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let text = format!("Cannot load {}: {reason}", buffer_name(log));
                if !state.loaded {
                    buffer.doc = notice(&text, Group::ErrorMsg);
                }
                self.error(text);
                return Vec::new();
            }
        };
        let block = |commit: &Commit| commit_block(commit, now);
        let mut arrived = 0;
        if after.is_some() {
            for commit in &page.items {
                if !buffer.doc.contains(commit.id.as_str()) {
                    buffer.doc.push_back(commit.id.as_str(), block(commit));
                }
            }
            state.next = page.next;
        } else {
            let known = page
                .items
                .iter()
                .position(|commit| buffer.doc.contains(commit.id.as_str()));
            match known.filter(|_| state.loaded && log != Log::Trending) {
                // New commits go above the ones already loaded; those are updated where they are.
                Some(first_known) => {
                    for commit in page.items[..first_known].iter().rev() {
                        buffer.doc.push_front(commit.id.as_str(), block(commit));
                        arrived += 1;
                    }
                    for commit in &page.items[first_known..] {
                        let _ = buffer.doc.replace(commit.id.as_str(), block(commit));
                    }
                }
                // A first load, a ranked log, or nothing in common with what is loaded: the page replaces it all.
                // Cursors stay on commits that are still there, since they are anchored by key.
                _ => {
                    let mut doc = Document::new();
                    for commit in &page.items {
                        if !doc.contains(commit.id.as_str()) {
                            doc.push_back(commit.id.as_str(), block(commit));
                        }
                    }
                    buffer.doc = doc;
                    state.next = page.next;
                }
            }
        }
        if buffer.doc.is_empty() {
            buffer.doc = notice("Nothing here yet.", Group::Time);
        }
        state.loaded = true;
        if arrived > 0 && top_before.is_some_and(|top| top > 0) {
            self.info(format!(
                "{arrived} new commit{}",
                if arrived == 1 { "" } else { "s" }
            ));
        }
        Vec::new()
    }

    /// `:e {name}`: shows the log of that name in the current window, loading it if needed.
    pub(super) fn open_log(&mut self, log: Log) -> Vec<Effect> {
        let index = self.log_buffer(log);
        self.show_buffer(index)
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
