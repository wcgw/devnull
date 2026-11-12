//! Profile buffers: who a person or an organization is, what they pinned, then their commits; and the lists that
//! lead from them.

use devnull_api::model::{AccountKind, AccountPage};

use super::feed::Source;
use super::{App, Effect, FetchError, Kind};
use crate::doc::{AccountRef, Content, Cursor, Target};
use crate::render::{organization_header, pinned_block, profile_header};

/// The head of a profile: who it is, then the commit pinned, if any.
fn head(page: &AccountPage, now: u64) -> Vec<(String, Content)> {
    match page {
        AccountPage::Person(profile) => {
            let mut blocks = vec![("~profile".to_owned(), profile_header(profile))];
            if let Some(pinned) = &profile.pinned {
                blocks.push(("~pinned".to_owned(), pinned_block(pinned, now)));
            }
            blocks
        }
        AccountPage::Organization(page) => vec![("~profile".to_owned(), organization_header(page))],
        // A kind of account this version does not know: nothing to show above its commits.
        _ => Vec::new(),
    }
}

impl App {
    /// Shows the profile of `account` in the current window, loading it if needed.
    pub(super) fn open_profile(&mut self, account: AccountRef) -> Vec<Effect> {
        let source = Source::Profile {
            handle: account.handle,
            kind: account.kind,
        };
        self.open_feed(source, Cursor::new())
    }

    /// `:profile`: the profile of `handle` (`@` or not), or without one, of the account under the cursor or of the
    /// author of the commit under it.
    pub(super) fn profile(&mut self, handle: Option<String>) -> Result<Vec<Effect>, String> {
        let account = match handle {
            Some(handle) => AccountRef::new(handle.trim_start_matches('@'), None),
            None => match self.target_under_cursor() {
                Some(Target::Account(account)) => account,
                Some(Target::Commit {
                    author: Some(author),
                    ..
                }) => author,
                _ => return Err("No account under the cursor".to_owned()),
            },
        };
        Ok(self.open_profile(account))
    }

    /// The connected account's own profile, as `:e profile` names it.
    pub(super) fn own_profile(&self) -> Option<Source> {
        self.connection.handle().map(|handle| Source::Profile {
            handle: handle.to_owned(),
            kind: Some(AccountKind::User),
        })
    }

    /// A profile arrived, or failed to. Once it says whether it is a person's or an organization's, its commits are
    /// asked for if they were waiting for that.
    pub(super) fn profile_arrived(
        &mut self,
        handle: String,
        result: Result<AccountPage, FetchError>,
        now: u64,
    ) -> Vec<Effect> {
        let kind = match &result {
            Ok(AccountPage::Person(_)) => Some(AccountKind::User),
            Ok(AccountPage::Organization(_)) => Some(AccountKind::Organization),
            _ => None,
        };
        let source = Source::Profile { handle, kind: None };
        let mut effects = self.head_arrived(&source, result.map(|page| head(&page, now)));
        // Only once it arrived: after a failure, loading would ask for the profile again straight away.
        if let Some(kind) = kind
            && let Some(index) = self.feed_buffer(&source)
        {
            if let Kind::Feed(feed) = &mut self.buffers[index].kind
                && let Source::Profile { kind: known, .. } = &mut feed.source
            {
                *known = Some(kind);
            }
            effects.extend(self.load(index));
        }
        effects
    }
}
