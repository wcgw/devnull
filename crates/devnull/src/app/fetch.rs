//! What feeds ask dev-null.sh for, and how the answers reach their buffers.

use devnull_api::model::{
    Account, AccountKind, AccountPage, Commit, Cursor, Inbox, Page, Preview, ProfileSearch, Thread,
};
use devnull_api::{Category, Draft, Log, OrganizationTab, People};

use wcgw_kit::BufferId;

use super::feed::{Feed, NOTICE, Source, notice, notice_lines};
use super::{Dn, Effects, Kit, NetEvent, fetch, threads};
use crate::doc::{Content, Cursor as DocCursor, Document};
use crate::hl::Group;
use crate::render::{account_block, commit_block, notification_block, parse_time};

/// Something to ask dev-null.sh for, or to do, on the app's behalf.
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
    /// A person's profile or an organization's page.
    Profile {
        /// The handle, without the `@`.
        handle: String,
        /// Which it is, when known; otherwise a person's, else an organization's.
        kind: Option<AccountKind>,
    },
    /// A page of a person's commits.
    Commits {
        /// The handle, without the `@`.
        handle: String,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// A page of an organization's commits, or of the commits mentioning it.
    OrganizationCommits {
        /// The handle, without the `@`.
        handle: String,
        /// Which commits.
        tab: OrganizationTab,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// A page of who follows a person, or whom they follow.
    People {
        /// The handle, without the `@`.
        handle: String,
        /// Which list.
        which: People,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// The people and organizations matching a search term.
    SearchProfiles {
        /// The term.
        term: String,
    },
    /// A page of the commits the account starred.
    Starred {
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// How many notifications are unread, without marking any read.
    UnreadCount,
    /// A page of the inbox, which marks every notification read.
    Notifications {
        /// The categories; all when empty.
        only: Vec<Category>,
        /// Where the page starts; `None` for the first.
        after: Option<Cursor>,
    },
    /// Star a commit, or take the star back.
    Star {
        /// The commit's id.
        id: String,
        /// Whether to star rather than take the star back.
        on: bool,
    },
    /// Follow an account, or stop following it.
    Follow {
        /// The handle, without the `@`.
        handle: String,
        /// A person or an organization; when not known, a person's, else an organization's.
        kind: Option<AccountKind>,
        /// Whether to follow rather than stop following.
        on: bool,
    },
    /// Delete one of the account's commits.
    DeleteCommit {
        /// The commit's id.
        id: String,
    },
    /// A draft's text as the server would keep and show it; publishes nothing.
    Preview {
        /// The draft's text.
        body: String,
    },
    /// Publish a draft.
    Publish {
        /// The draft.
        draft: Draft,
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
    /// The answer to [`Request::Profile`].
    Profile {
        /// The handle asked for.
        handle: String,
        /// The profile or page, or why there is none.
        result: Result<AccountPage, FetchError>,
    },
    /// The answer to [`Request::Commits`].
    Commits {
        /// The handle asked for.
        handle: String,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Commit>, FetchError>,
    },
    /// The answer to [`Request::OrganizationCommits`].
    OrganizationCommits {
        /// The handle asked for.
        handle: String,
        /// Which commits.
        tab: OrganizationTab,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Commit>, FetchError>,
    },
    /// The answer to [`Request::People`].
    People {
        /// The handle asked for.
        handle: String,
        /// Which list.
        which: People,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Account>, FetchError>,
    },
    /// The answer to [`Request::SearchProfiles`].
    SearchProfiles {
        /// The term searched for.
        term: String,
        /// What was found, or why nothing could be.
        result: Result<ProfileSearch, FetchError>,
    },
    /// The answer to [`Request::Starred`].
    Starred {
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Page<Commit>, FetchError>,
    },
    /// The answer to [`Request::UnreadCount`].
    UnreadCount {
        /// The count, or why there is none.
        result: Result<u64, FetchError>,
    },
    /// The answer to [`Request::Notifications`].
    Notifications {
        /// The categories asked for.
        only: Vec<Category>,
        /// Where the page was asked to start.
        after: Option<Cursor>,
        /// The page, or why there is none.
        result: Result<Inbox, FetchError>,
    },
    /// The answer to [`Request::Star`].
    Star {
        /// The commit's id.
        id: String,
        /// Whether it was to star.
        on: bool,
        /// The commit as the server now counts it, or why not. Boxed: commits are large.
        result: Result<Box<Commit>, FetchError>,
    },
    /// The answer to [`Request::Follow`].
    Follow {
        /// The handle.
        handle: String,
        /// Whether it was to follow.
        on: bool,
        /// Done, or why not.
        result: Result<(), FetchError>,
    },
    /// The answer to [`Request::DeleteCommit`].
    DeleteCommit {
        /// The commit's id.
        id: String,
        /// Done, or why not.
        result: Result<(), FetchError>,
    },
    /// The answer to [`Request::Preview`].
    Preview {
        /// The draft's text.
        body: String,
        /// The preview, or why the server refuses the text.
        result: Result<Preview, FetchError>,
    },
    /// The answer to [`Request::Publish`].
    Published {
        /// The draft.
        draft: Draft,
        /// The commit as published, or why not. Boxed: commits are large.
        result: Result<Box<Commit>, FetchError>,
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

/// A page of the inbox as blocks, each marked new when it was unread, or read by the visit that read this page.
fn inbox_page(inbox: &Inbox, now: u64) -> Page<(String, Content)> {
    let boundary = parse_time(&inbox.read_at);
    let items = inbox
        .page
        .items
        .iter()
        .map(|notification| {
            let read = notification.read_at.as_deref().map(parse_time);
            let new = match (read, boundary) {
                (None, _) => true,
                (Some(Some(read)), Some(boundary)) => read >= boundary,
                (Some(_), _) => false,
            };
            (
                notification.id.clone(),
                notification_block(notification, new, now),
            )
        })
        .collect();
    Page::new(items, inbox.page.next.clone())
}

impl Dn {
    /// The buffer showing `source`, created empty if there is none yet.
    pub(super) fn feed_buffer_or_new(&mut self, kit: &mut Kit, source: Source) -> BufferId {
        self.feed_buffer(&source).unwrap_or_else(|| {
            let id = kit.add_buffer(source.name(), Document::new(), true);
            // A feed is prose: laid out up to the kit's 'textwidth' (120), as every app's.
            kit.set_prose(id, true);
            self.feeds.insert(id, Feed::new(source));
            id
        })
    }

    /// Shows `source` in the current window, loading it if needed; the cursor starts at `start` when the window
    /// never showed it.
    pub(super) fn open_feed(&mut self, kit: &mut Kit, source: Source, start: DocCursor) -> Effects {
        let id = self.feed_buffer_or_new(kit, source);
        kit.show_buffer_at(id, start);
        Vec::new()
    }

    /// Asks for what buffer `id` shows, unless it has it or is getting it.
    pub(super) fn load(&mut self, kit: &mut Kit, id: BufferId) -> Effects {
        let requests = match self.feeds.get_mut(&id) {
            Some(feed) => feed.start(),
            None => Vec::new(),
        };
        self.ask(kit, id, requests)
    }

    /// `:e!`: asks for what buffer `id` shows again, keeping what is shown until it arrives. `None` when the
    /// buffer shows nothing from the server.
    pub(super) fn refresh(&mut self, kit: &mut Kit, id: BufferId) -> Option<Effects> {
        let requests = self.feeds.get_mut(&id)?.refresh();
        Some(self.ask(kit, id, requests))
    }

    /// Sends `requests` for buffer `id`, saying it is loading while it has nothing else to show.
    fn ask(&mut self, kit: &mut Kit, id: BufferId, requests: Vec<Request>) -> Effects {
        if let Some(feed) = self.feeds.get(&id)
            && !feed.has_content()
        {
            let buffer = kit.buffer_mut(id);
            let doc = notice(&format!("Loading {}…", buffer.name()), crate::hl::TIME);
            *buffer.doc_mut() = doc;
        }
        requests.into_iter().map(fetch).collect()
    }

    /// Asks for the next page of the current window's list when its cursor is within a window's height of the end.
    pub(super) fn load_more(&mut self, kit: &mut Kit) -> Effects {
        let id = kit.current_buffer();
        if !self.feeds.get(&id).is_some_and(Feed::has_more) || !kit.cursor_near_end() {
            return Vec::new();
        }
        let Some(feed) = self.feeds.get_mut(&id) else {
            return Vec::new();
        };
        feed.more(kit.buffer_mut(id).doc_mut())
            .map(fetch)
            .into_iter()
            .collect()
    }

    /// An answer from the server; `now` (Unix seconds) is when it arrived, for ages.
    pub(super) fn fetched(&mut self, kit: &mut Kit, fetched: Fetched, now: u64) -> Effects {
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
                self.page_arrived(kit, &Source::Log(log), after.is_some(), result.map(commits))
            }
            Fetched::Replies { id, after, result } => self.page_arrived(
                kit,
                &Source::Thread(id),
                after.is_some(),
                result.map(commits),
            ),
            Fetched::Thread { id, result } => {
                let cell = kit.cell_size();
                self.head_arrived(
                    kit,
                    &Source::Thread(id),
                    result.map(|thread| threads::head(&thread, now, cell)),
                )
            }
            Fetched::Profile { handle, result } => self.profile_arrived(kit, handle, result, now),
            Fetched::Commits {
                handle,
                after,
                result,
            } => {
                let source = Source::Profile { handle, kind: None };
                self.page_arrived(kit, &source, after.is_some(), result.map(commits))
            }
            Fetched::OrganizationCommits {
                handle,
                tab,
                after,
                result,
            } => {
                let source = match tab {
                    OrganizationTab::Mentions => Source::Mentions(handle),
                    _ => Source::Profile { handle, kind: None },
                };
                self.page_arrived(kit, &source, after.is_some(), result.map(commits))
            }
            Fetched::People {
                handle,
                which,
                after,
                result,
            } => {
                let accounts = |page: Page<Account>| {
                    let items = page
                        .items
                        .iter()
                        .map(|account| (format!("@{}", account.handle), account_block(account)))
                        .collect();
                    Page::new(items, page.next)
                };
                let source = Source::People(handle, which);
                self.page_arrived(kit, &source, after.is_some(), result.map(accounts))
            }
            Fetched::SearchProfiles { term, result } => self.found(kit, &term, result),
            Fetched::Notifications {
                only,
                after,
                result,
            } => {
                if result.is_ok() {
                    // Reading the inbox marked it all read.
                    self.unread = 0;
                }
                let source = Source::Notifications(only);
                self.page_arrived(
                    kit,
                    &source,
                    after.is_some(),
                    result.map(|inbox| inbox_page(&inbox, now)),
                )
            }
            Fetched::Star { id, on, result } => self.starred(kit, &id, on, result, now),
            Fetched::Follow { handle, on, result } => self.followed(kit, &handle, on, result),
            Fetched::DeleteCommit { id, result } => self.deleted(kit, &id, result),
            Fetched::Preview { body, result } => self.previewed(kit, body, result),
            Fetched::Published { draft, result } => self.published(kit, &draft, result, now),
            Fetched::Starred { after, result } => {
                self.page_arrived(kit, &Source::Starred, after.is_some(), result.map(commits))
            }
            Fetched::UnreadCount { result } => match result {
                Ok(count) => {
                    self.unread = count;
                    Vec::new()
                }
                Err(FetchError::NotApproved) => self.net(kit, NetEvent::NotApproved),
                // Asked for every poll: the next one tries again, without an error each time.
                Err(FetchError::Failed(_)) => Vec::new(),
            },
        }
    }

    /// The head of the feed of `source` arrived as blocks, or failed to.
    pub(super) fn head_arrived(
        &mut self,
        kit: &mut Kit,
        source: &Source,
        result: Result<Vec<(String, Content)>, FetchError>,
    ) -> Effects {
        let Some(id) = self.feed_buffer(source) else {
            return Vec::new();
        };
        let Some(feed) = self.feeds.get_mut(&id) else {
            return Vec::new();
        };
        feed.head_answered();
        match result {
            Ok(blocks) => {
                feed.head_arrived(kit.buffer_mut(id).doc_mut(), blocks);
                Vec::new()
            }
            Err(error) => self.failed(kit, id, error),
        }
    }

    /// A page of the list of the feed of `source` arrived as blocks, or failed to: the next page when `next_page`,
    /// else the first.
    fn page_arrived(
        &mut self,
        kit: &mut Kit,
        source: &Source,
        next_page: bool,
        result: Result<Page<(String, Content)>, FetchError>,
    ) -> Effects {
        let Some(id) = self.feed_buffer(source) else {
            return Vec::new();
        };
        let top_before = kit.top_row(id);
        let Some(feed) = self.feeds.get_mut(&id) else {
            return Vec::new();
        };
        feed.page_answered(kit.buffer_mut(id).doc_mut(), next_page);
        let page = match result {
            Ok(page) => page,
            Err(error) => return self.failed(kit, id, error),
        };
        let arrived = feed.page_arrived(kit.buffer_mut(id).doc_mut(), next_page, page);
        let of_commits = !matches!(source, Source::People(..));
        if of_commits && arrived > 0 && top_before.is_some_and(|top| top > 0) {
            kit.info(format!(
                "{arrived} new commit{}",
                if arrived == 1 { "" } else { "s" }
            ));
        }
        Vec::new()
    }

    /// A request for buffer `id` failed: says why, also in the buffer while it has nothing else to show.
    fn failed(&mut self, kit: &mut Kit, id: BufferId, error: FetchError) -> Effects {
        match error {
            FetchError::NotApproved => self.net(kit, NetEvent::NotApproved),
            FetchError::Failed(reason) => {
                let has_content = self.feeds.get(&id).is_none_or(Feed::has_content);
                let buffer = kit.buffer_mut(id);
                let text = format!("Cannot load {}: {reason}", buffer.name());
                if !has_content {
                    *buffer.doc_mut() = notice(&text, Group::ERROR_MSG);
                } else if buffer.doc().contains(NOTICE) {
                    // Such as "Loading…" above replies that arrived when their commit did not.
                    let _ = buffer
                        .doc_mut()
                        .replace(NOTICE, notice_lines(&text, Group::ERROR_MSG));
                }
                kit.error(text);
                Vec::new()
            }
        }
    }
}
