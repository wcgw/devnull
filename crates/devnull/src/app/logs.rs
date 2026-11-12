//! The log buffers: their names, polling chronological logs for new commits, and how a fresh first page joins what
//! is shown.

use std::time::Duration;

use devnull_api::Log;
use devnull_api::model::{Commit, Page};

use super::fetch::{Paging, append};
use super::{App, Buffer, Effect, Kind, Wake, WakePurpose};
use crate::doc::Document;
use crate::render::commit_block;

/// How often chronological logs are checked for new commits while the terminal has focus, and without it.
const POLL_FOCUSED: Duration = Duration::from_secs(15);
const POLL_UNFOCUSED: Duration = Duration::from_secs(30);

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
            kind: Kind::Log {
                log,
                paging: Paging::default(),
            },
        });
        self.buffers.len() - 1
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
            .filter(|&index| {
                matches!(&self.buffers[index].kind, Kind::Log { log, paging } if *log != Log::Trending && paging.loaded)
            })
            .collect();
        let mut effects: Vec<Effect> = chronological
            .into_iter()
            .filter_map(|index| self.refresh(index))
            .flatten()
            .collect();
        effects.push(self.next_poll());
        effects
    }

    /// `:e {name}`: shows the log of that name in the current window, loading it if needed.
    pub(super) fn open_log(&mut self, log: Log) -> Vec<Effect> {
        let index = self.log_buffer(log);
        self.show_buffer(index)
    }
}

/// The first page of `log` arrived: new commits of a chronological log go above the ones shown, which are updated
/// where they are; otherwise the page replaces what is shown. Returns how many new commits went above.
pub(super) fn merge(
    doc: &mut Document,
    log: Log,
    paging: &mut Paging,
    page: Page<Commit>,
    now: u64,
) -> usize {
    let known = page
        .items
        .iter()
        .position(|commit| doc.contains(commit.id.as_str()));
    match known.filter(|_| paging.loaded && log != Log::Trending) {
        Some(first_known) => {
            for commit in page.items[..first_known].iter().rev() {
                doc.push_front(commit.id.as_str(), commit_block(commit, now));
            }
            for commit in &page.items[first_known..] {
                let _ = doc.replace(commit.id.as_str(), commit_block(commit, now));
            }
            first_known
        }
        // A first load, a ranked log, or nothing in common with what is shown: the page replaces it all. Cursors stay
        // on commits that are still there, since they are anchored by key.
        None => {
            *doc = Document::new();
            append(doc, &page, now);
            paging.next = page.next;
            0
        }
    }
}
