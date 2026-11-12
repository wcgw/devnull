//! Actions on what is under the cursor: starring a commit, following an account, deleting one's own commit after
//! confirming it.

use devnull_api::model::Commit;

use super::feed::Source;
use super::threads::short_hash;
use super::{App, AppMode, Effect, FetchError, Kind, NetEvent, Request};
use crate::doc::{Layout, Target};
use crate::key::{Code, Key};
use crate::render::{commit_block, deleted_block, focused_block};

/// A question in the command line, answered by the next key: `y` does it, anything else does not.
#[derive(Debug)]
pub(super) struct Confirm {
    question: String,
    then: Pending,
}

/// What a confirmed question does.
#[derive(Debug)]
enum Pending {
    /// Delete the account's commit of this id.
    Delete(String),
}

impl Confirm {
    /// The question, with its answers.
    pub(super) fn question(&self) -> &str {
        &self.question
    }
}

/// The commit under the cursor, as `:browse` finds it: a quote's, else the one holding the cursor.
struct UnderCursor {
    id: String,
    author: Option<String>,
    starred: bool,
}

impl App {
    fn commit_under_cursor(&self) -> Option<UnderCursor> {
        let window = self.windows.current();
        let layout = Layout::new(&self.buffers[window.buffer].doc, self.window_area().width);
        [
            window.cursor.target(&layout),
            window.cursor.block_target(&layout),
        ]
        .into_iter()
        .flatten()
        .find_map(|target| match target {
            Target::Commit {
                id,
                author,
                starred,
                ..
            } => Some(UnderCursor {
                id: id.clone(),
                author: author.as_ref().map(|author| author.handle.clone()),
                starred: *starred,
            }),
            _ => None,
        })
    }

    /// `:star` (`s`): stars the commit under the cursor, or takes its star back.
    pub(super) fn star(&mut self) -> Result<Vec<Effect>, String> {
        let commit = self
            .commit_under_cursor()
            .ok_or_else(|| "No commit under the cursor".to_owned())?;
        Ok(vec![Effect::Fetch(Request::Star {
            id: commit.id,
            on: !commit.starred,
        })])
    }

    /// `:delete` (`dd`): asks to delete the account's own commit under the cursor.
    pub(super) fn delete(&mut self) -> Result<Vec<Effect>, String> {
        let commit = self
            .commit_under_cursor()
            .ok_or_else(|| "No commit under the cursor".to_owned())?;
        if commit.author.is_none() || commit.author.as_deref() != self.connection.handle() {
            return Err("You can only delete your own commits".to_owned());
        }
        self.mode = AppMode::Confirm(Confirm {
            question: format!("Delete commit {}? (y/n)", short_hash(&commit.id)),
            then: Pending::Delete(commit.id),
        });
        Ok(Vec::new())
    }

    /// `:follow` and `:unfollow` (`on` false): the account `handle`, else the one under the cursor, else the author
    /// of the commit under it.
    pub(super) fn follow(
        &mut self,
        handle: Option<String>,
        on: bool,
    ) -> Result<Vec<Effect>, String> {
        let (handle, kind) = match handle {
            Some(handle) => (handle, None),
            None => match self.target_under_cursor() {
                Some(Target::Account(account)) => (account.handle, account.kind),
                Some(Target::Commit {
                    author: Some(author),
                    ..
                }) => (author.handle, author.kind),
                _ => return Err("No account under the cursor".to_owned()),
            },
        };
        Ok(vec![Effect::Fetch(Request::Follow { handle, kind, on })])
    }

    /// The answer to a question in the command line.
    pub(super) fn confirm_key(&mut self, key: Key) -> Vec<Effect> {
        let AppMode::Confirm(confirm) = std::mem::replace(&mut self.mode, AppMode::Normal) else {
            return Vec::new();
        };
        if key != Key::new(Code::Char('y')) {
            return Vec::new();
        }
        match confirm.then {
            Pending::Delete(id) => vec![Effect::Fetch(Request::DeleteCommit { id })],
        }
    }

    /// A star given or taken back, or why not: the commit shows its new count wherever it is shown.
    pub(super) fn starred(
        &mut self,
        id: &str,
        on: bool,
        result: Result<Box<Commit>, FetchError>,
        now: u64,
    ) -> Vec<Effect> {
        match result {
            Ok(commit) => {
                for buffer in &mut self.buffers {
                    let Kind::Feed(feed) = &buffer.kind else {
                        continue;
                    };
                    // A thread shows its commit whole.
                    let content = if feed.source == Source::Thread(id.to_owned()) {
                        focused_block(&commit, now)
                    } else {
                        commit_block(&commit, now)
                    };
                    let _ = buffer.doc.replace(id, content);
                }
                Vec::new()
            }
            Err(FetchError::NotApproved) => self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let verb = if on { "star" } else { "unstar" };
                self.error(format!("Cannot {verb} {}: {reason}", short_hash(id)));
                Vec::new()
            }
        }
    }

    /// An account followed or no longer, or why not; a profile shown of it is asked for again.
    pub(super) fn followed(
        &mut self,
        handle: &str,
        on: bool,
        result: Result<(), FetchError>,
    ) -> Vec<Effect> {
        match result {
            Ok(()) => {
                self.info(if on {
                    format!("Following @{handle}")
                } else {
                    format!("No longer following @{handle}")
                });
                let profile = Source::Profile {
                    handle: handle.to_owned(),
                    kind: None,
                };
                self.feed_buffer(&profile)
                    .and_then(|index| self.refresh(index))
                    .unwrap_or_default()
            }
            Err(FetchError::NotApproved) => self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let verb = if on { "follow" } else { "unfollow" };
                self.error(format!("Cannot {verb} @{handle}: {reason}"));
                Vec::new()
            }
        }
    }

    /// A commit deleted, or why not: wherever it was, it says it was deleted.
    pub(super) fn deleted(&mut self, id: &str, result: Result<(), FetchError>) -> Vec<Effect> {
        match result {
            Ok(()) => {
                for buffer in &mut self.buffers {
                    if matches!(buffer.kind, Kind::Feed(_)) {
                        let _ = buffer.doc.replace(id, deleted_block());
                    }
                }
                self.info(format!("Deleted {}", short_hash(id)));
                Vec::new()
            }
            Err(FetchError::NotApproved) => self.net(NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                self.error(format!("Cannot delete {}: {reason}", short_hash(id)));
                Vec::new()
            }
        }
    }
}
