//! Thread buffers: a commit whole, under the commits it replies to and above its replies, which come a page at a
//! time.

use devnull_api::model::{Commit, Page, Thread};

use super::fetch::{NOTICE, Paging, Request, append};
use super::{App, Buffer, Effect, FetchError, Kind};
use crate::doc::{Cursor, Document};
use crate::render::{commit_block, focused_block};

/// How many characters of a commit id its short hash is.
const SHORT_HASH: usize = 7;

/// What a thread's buffer knows beyond its document.
#[derive(Debug)]
pub(super) struct ThreadState {
    /// The id of the commit the thread is about.
    pub(super) id: String,
    /// The keys of the blocks of the commits it replies to, then of the commit itself; empty until they arrive.
    head: Vec<String>,
    /// Whether the commit and what it replies to are on their way.
    head_loading: bool,
    /// The replies, oldest first.
    pub(super) replies: Paging,
}

impl ThreadState {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            head: Vec::new(),
            head_loading: false,
            replies: Paging::default(),
        }
    }

    /// The requests for the commit, what it replies to and the first page of replies, noted as asked for: those that
    /// did not arrive and are not on their way.
    pub(super) fn start(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        if self.head.is_empty() && !self.head_loading {
            self.head_loading = true;
            requests.push(Request::Thread {
                id: self.id.clone(),
            });
        }
        if self.replies.start() {
            requests.push(self.first_replies());
        }
        requests
    }

    /// As [`ThreadState::start`], also for what arrived already.
    pub(super) fn refresh(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        if !self.head_loading {
            self.head_loading = true;
            requests.push(Request::Thread {
                id: self.id.clone(),
            });
        }
        if self.replies.refresh() {
            requests.push(self.first_replies());
        }
        requests
    }

    fn first_replies(&self) -> Request {
        Request::Replies {
            id: self.id.clone(),
            after: None,
        }
    }

    /// Whether any of the thread arrived.
    pub(super) fn has_content(&self) -> bool {
        !self.head.is_empty() || self.replies.loaded
    }
}

/// A thread's buffer name: `thread/` and the commit's short hash. dev-null.sh makes short hashes of the last seven
/// characters of commit ids, so the name is known before the commit arrives.
pub(super) fn buffer_name(id: &str) -> String {
    let start = id
        .char_indices()
        .rev()
        .nth(SHORT_HASH - 1)
        .map_or(0, |(at, _)| at);
    format!("thread/{}", &id[start..])
}

/// The first page of replies arrived: replies already shown are updated where they are, others go at the end.
pub(super) fn merge(doc: &mut Document, thread: &mut ThreadState, page: Page<Commit>, now: u64) {
    for commit in &page.items {
        let _ = doc.replace(commit.id.as_str(), commit_block(commit, now));
    }
    append(doc, &page, now);
    // Once further pages arrived, the next one is where they left off. Asking for one of them again is harmless:
    // replies shown already are not added twice.
    thread.replies.next = thread.replies.next.take().or(page.next);
}

impl App {
    /// The buffer of the thread about commit `id`, if there is one.
    pub(super) fn thread_buffer(&self, id: &str) -> Option<usize> {
        self.buffers
            .iter()
            .position(|buffer| matches!(&buffer.kind, Kind::Thread(thread) if thread.id == id))
    }

    /// Shows the thread about commit `id` in the current window, at the commit, loading it if needed.
    pub(super) fn open_thread(&mut self, id: &str) -> Vec<Effect> {
        let index = self.thread_buffer(id).unwrap_or_else(|| {
            self.buffers.push(Buffer {
                name: buffer_name(id),
                doc: Document::new(),
                kind: Kind::Thread(ThreadState::new(id)),
            });
            self.buffers.len() - 1
        });
        self.show_buffer_at(index, Cursor::on_block(id))
    }

    /// The commit `id` and what it replies to arrived, or failed to; they go above the replies.
    pub(super) fn thread_arrived(
        &mut self,
        id: &str,
        result: Result<Box<Thread>, FetchError>,
        now: u64,
    ) -> Vec<Effect> {
        let Some(index) = self.thread_buffer(id) else {
            return Vec::new();
        };
        let buffer = &mut self.buffers[index];
        let Kind::Thread(thread) = &mut buffer.kind else {
            return Vec::new();
        };
        thread.head_loading = false;
        let fetched = match result {
            Ok(fetched) => fetched,
            Err(error) => return self.failed(index, error),
        };
        let blocks: Vec<_> = fetched
            .ancestors
            .iter()
            .map(|commit| (commit.id.clone(), commit_block(commit, now)))
            .chain([(
                fetched.commit.id.clone(),
                focused_block(&fetched.commit, now),
            )])
            .collect();
        let keys: Vec<String> = blocks.iter().map(|(key, _)| key.clone()).collect();
        let doc = &mut buffer.doc;
        doc.remove(NOTICE);
        if keys == thread.head {
            // The same commits: updated in place, so folds keep the state they were given.
            for (key, content) in blocks {
                let _ = doc.replace(key.as_str(), content);
            }
        } else {
            for key in &thread.head {
                doc.remove(key.as_str());
            }
            for (key, content) in blocks.into_iter().rev() {
                if !doc.contains(key.as_str()) {
                    doc.push_front(key.as_str(), content);
                }
            }
        }
        thread.head = keys;
        Vec::new()
    }
}
