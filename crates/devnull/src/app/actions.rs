//! Actions on what is under the cursor: starring a commit, following an account, deleting one's own commit after
//! confirming it.

use devnull_api::model::Commit;

use super::feed::Source;
use super::threads::short_hash;
use super::{Dn, Effects, FetchError, Kit, NetEvent, Request, fetch};
use crate::doc::Target;
use crate::render::{commit_block, deleted_block, focused_block};

/// The commit under the cursor, as `:browse` finds it: a quote's, else the one holding the cursor.
pub(super) struct UnderCursor {
    pub(super) id: String,
    pub(super) author: Option<String>,
    starred: bool,
}

impl Dn {
    pub(super) fn commit_under_cursor(kit: &Kit) -> Option<UnderCursor> {
        kit.targets_under_cursor()
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
    pub(super) fn star(&mut self, kit: &mut Kit) -> Result<Effects, String> {
        let commit = Self::commit_under_cursor(kit)
            .ok_or_else(|| "No commit under the cursor".to_owned())?;
        Ok(vec![fetch(Request::Star {
            id: commit.id,
            on: !commit.starred,
        })])
    }

    /// `:delete` (`dd`): asks to delete the account's own commit under the cursor.
    pub(super) fn delete(&mut self, kit: &mut Kit) -> Result<Effects, String> {
        let commit = Self::commit_under_cursor(kit)
            .ok_or_else(|| "No commit under the cursor".to_owned())?;
        if commit.author.is_none() || commit.author.as_deref() != self.connection.handle() {
            return Err("You can only delete your own commits".to_owned());
        }
        let id = commit.id;
        kit.confirm(
            format!("Delete commit {}? (y/n)", short_hash(&id)),
            None,
            move |_, _, answer| {
                if answer == 'y' {
                    vec![fetch(Request::DeleteCommit { id })]
                } else {
                    Vec::new()
                }
            },
        );
        Ok(Vec::new())
    }

    /// `:follow` and `:unfollow` (`on` false): the account `handle`, else the one under the cursor, else the author
    /// of the commit under it.
    pub(super) fn follow(
        &mut self,
        kit: &mut Kit,
        handle: Option<String>,
        on: bool,
    ) -> Result<Effects, String> {
        let (handle, kind) = match handle {
            Some(handle) => (handle, None),
            None => match kit.target_under_cursor() {
                Some(Target::Account(account)) => (account.handle, account.kind),
                Some(Target::Commit {
                    author: Some(author),
                    ..
                }) => (author.handle, author.kind),
                _ => return Err("No account under the cursor".to_owned()),
            },
        };
        Ok(vec![fetch(Request::Follow { handle, kind, on })])
    }

    /// A star given or taken back, or why not: the commit shows its new count wherever it is shown.
    pub(super) fn starred(
        &mut self,
        kit: &mut Kit,
        id: &str,
        on: bool,
        result: Result<Box<Commit>, FetchError>,
        now: u64,
    ) -> Effects {
        match result {
            Ok(commit) => {
                let cell = kit.cell_size();
                for (&buffer, feed) in &self.feeds {
                    // A thread shows its commit whole.
                    let content = if feed.source == Source::Thread(id.to_owned()) {
                        focused_block(&commit, now, cell)
                    } else {
                        commit_block(&commit, now)
                    };
                    let _ = kit.buffer_mut(buffer).doc_mut().replace(id, content);
                }
                Vec::new()
            }
            Err(FetchError::NotApproved) => self.net(kit, NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let verb = if on { "star" } else { "unstar" };
                kit.error(format!("Cannot {verb} {}: {reason}", short_hash(id)));
                Vec::new()
            }
        }
    }

    /// An account followed or no longer, or why not; a profile shown of it is asked for again.
    pub(super) fn followed(
        &mut self,
        kit: &mut Kit,
        handle: &str,
        on: bool,
        result: Result<(), FetchError>,
    ) -> Effects {
        match result {
            Ok(()) => {
                kit.info(if on {
                    format!("Following @{handle}")
                } else {
                    format!("No longer following @{handle}")
                });
                let profile = Source::Profile {
                    handle: handle.to_owned(),
                    kind: None,
                };
                self.feed_buffer(&profile)
                    .and_then(|id| self.refresh(kit, id))
                    .unwrap_or_default()
            }
            Err(FetchError::NotApproved) => self.net(kit, NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                let verb = if on { "follow" } else { "unfollow" };
                kit.error(format!("Cannot {verb} @{handle}: {reason}"));
                Vec::new()
            }
        }
    }

    /// A commit deleted, or why not: wherever it was, it says it was deleted.
    pub(super) fn deleted(
        &mut self,
        kit: &mut Kit,
        id: &str,
        result: Result<(), FetchError>,
    ) -> Effects {
        match result {
            Ok(()) => {
                for &buffer in self.feeds.keys() {
                    let _ = kit
                        .buffer_mut(buffer)
                        .doc_mut()
                        .replace(id, deleted_block());
                }
                kit.info(format!("Deleted {}", short_hash(id)));
                Vec::new()
            }
            Err(FetchError::NotApproved) => self.net(kit, NetEvent::NotApproved),
            Err(FetchError::Failed(reason)) => {
                kit.error(format!("Cannot delete {}: {reason}", short_hash(id)));
                Vec::new()
            }
        }
    }
}
