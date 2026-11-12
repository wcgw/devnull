//! The log buffers: what they ask the server for and how answers become their documents.

use devnull_api::Log;
use devnull_api::model::{Commit, Cursor, Page};

use super::{App, Buffer, Effect};
use crate::doc::{Document, Line, Span};
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
    pub(super) next: Option<Cursor>,
    pub(super) loading: bool,
    /// Whether a page ever arrived.
    pub(super) loaded: bool,
}

impl LogState {
    pub(super) fn new(log: Log) -> Self {
        Self {
            log,
            next: None,
            loading: false,
            loaded: false,
        }
    }
}

/// A log's buffer name: `log/following` and so on.
pub(super) fn buffer_name(log: Log) -> String {
    format!("log/{}", log.name())
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

    /// An answer from the server; `now` (Unix seconds) is when it arrived, for ages.
    pub(super) fn fetched(&mut self, fetched: Fetched, now: u64) -> Vec<Effect> {
        let Fetched::Log { log, after, result } = fetched;
        let index = self.log_buffer(log);
        let buffer = &mut self.buffers[index];
        let Some(state) = buffer.log.as_mut() else {
            return Vec::new();
        };
        state.loading = false;
        match result {
            Ok(page) => {
                if after.is_none() {
                    buffer.doc = Document::new();
                }
                for commit in &page.items {
                    if !buffer.doc.contains(commit.id.as_str()) {
                        buffer
                            .doc
                            .push_back(commit.id.as_str(), commit_block(commit, now));
                    }
                }
                if buffer.doc.is_empty() {
                    buffer.doc = notice("Nothing here yet.", Group::Time);
                }
                state.next = page.next;
                state.loaded = true;
                Vec::new()
            }
            Err(FetchError::NotApproved) => self.net(super::NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let text = format!("Cannot load {}: {reason}", buffer_name(log));
                if !state.loaded {
                    buffer.doc = notice(&text, Group::ErrorMsg);
                }
                self.error(text);
                Vec::new()
            }
        }
    }
}
