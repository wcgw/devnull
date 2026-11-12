//! Thread buffers: a commit whole, under the commits it replies to and above its replies.

use devnull_api::model::Thread;

use super::feed::Source;
use super::{App, Effect};
use crate::doc::{Content, Cursor};
use crate::render::{commit_block, focused_block};

/// How many characters of a commit id its short hash is.
const SHORT_HASH: usize = 7;

/// A thread's buffer name: `thread/` and the commit's short hash, known before the commit arrives.
pub(super) fn buffer_name(id: &str) -> String {
    format!("thread/{}", short_hash(id))
}

/// The short hash of commit `id`: dev-null.sh makes it of the id's last seven characters.
pub(super) fn short_hash(id: &str) -> &str {
    let start = id
        .char_indices()
        .rev()
        .nth(SHORT_HASH - 1)
        .map_or(0, |(at, _)| at);
    &id[start..]
}

/// The head of a thread: the commits it replies to, root first, then the commit whole, its pictures at their own size
/// in cells of `cell` pixels.
pub(super) fn head(thread: &Thread, now: u64, cell: (u16, u16)) -> Vec<(String, Content)> {
    thread
        .ancestors
        .iter()
        .map(|commit| (commit.id.clone(), commit_block(commit, now)))
        .chain([(
            thread.commit.id.clone(),
            focused_block(&thread.commit, now, cell),
        )])
        .collect()
}

impl App {
    /// Shows the thread about commit `id` in the current window, at the commit, loading it if needed.
    pub(super) fn open_thread(&mut self, id: &str) -> Vec<Effect> {
        self.open_feed(Source::Thread(id.to_owned()), Cursor::on_block(id))
    }
}
