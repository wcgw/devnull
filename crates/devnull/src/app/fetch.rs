//! What feeds ask dev-null.sh for, and how the answers reach their buffers.

use devnull_api::model::{
    Account, AccountKind, AccountPage, Commit, Cursor, Inbox, Page, ProfileSearch, Thread,
};
use devnull_api::{Category, Log, OrganizationTab, People};

use super::feed::{Feed, NOTICE, Source, notice, notice_lines};
use super::{App, Buffer, Effect, Kind, NetEvent, threads};
use crate::doc::{Content, Cursor as DocCursor, Document, Layout};
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
            Fetched::Profile { handle, result } => self.profile_arrived(handle, result, now),
            Fetched::Commits {
                handle,
                after,
                result,
            } => {
                let source = Source::Profile { handle, kind: None };
                self.page_arrived(&source, after.is_some(), result.map(commits))
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
                self.page_arrived(&source, after.is_some(), result.map(commits))
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
                self.page_arrived(&source, after.is_some(), result.map(accounts))
            }
            Fetched::SearchProfiles { term, result } => self.found(&term, result),
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
                    &source,
                    after.is_some(),
                    result.map(|inbox| inbox_page(&inbox, now)),
                )
            }
            Fetched::Star { id, on, result } => self.starred(&id, on, result, now),
            Fetched::Follow { handle, on, result } => self.followed(&handle, on, result),
            Fetched::DeleteCommit { id, result } => self.deleted(&id, result),
            Fetched::Starred { after, result } => {
                self.page_arrived(&Source::Starred, after.is_some(), result.map(commits))
            }
            Fetched::UnreadCount { result } => match result {
                Ok(count) => {
                    self.unread = count;
                    Vec::new()
                }
                Err(FetchError::NotApproved) => self.net(NetEvent::NotApproved),
                // Asked for every poll: the next one tries again, without an error each time.
                Err(FetchError::Failed(_)) => Vec::new(),
            },
        }
    }

    /// The head of the feed of `source` arrived as blocks, or failed to.
    pub(super) fn head_arrived(
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
        let of_commits = !matches!(source, Source::People(..));
        if of_commits && arrived > 0 && top_before.is_some_and(|top| top > 0) {
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
