//! The API's data as `dn` reads it (see "API notes" in `docs/plan.md`).
//!
//! Fields the server may leave out default to empty, and fields `dn` does not use are ignored, so the server can add
//! to its responses without breaking the client.

use serde::{Deserialize, Deserializer};

/// The connected account, from `GET /api/v1/me`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Me {
    /// Account id.
    pub id: String,
    /// Handle, without the `@`.
    pub handle: String,
    /// Display name.
    #[serde(default, deserialize_with = "or_default")]
    pub name: String,
    /// Whether the current terms are accepted; until they are, nothing else works.
    #[serde(default, deserialize_with = "or_default")]
    pub terms_accepted: bool,
    /// The version of the current terms, to accept by.
    #[serde(default)]
    pub terms_version: Option<String>,
    /// Unread notifications, from the response envelope.
    #[serde(skip)]
    pub unread_notifications: u64,
}

/// A commit (a post).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Commit {
    /// UUIDv7.
    pub id: String,
    /// The last 7 hex digits of the id. Empty for a deleted commit, which can come as little more than its id.
    #[serde(default, deserialize_with = "or_default")]
    pub short_hash: String,
    /// The raw text; [`Commit::mentions`] index into it.
    #[serde(default, deserialize_with = "or_default")]
    pub body: String,
    /// The text to show, in segments.
    #[serde(default, deserialize_with = "or_default")]
    pub body_segments: Vec<Segment>,
    /// For long commits, the excerpt the web shows first.
    #[serde(default)]
    pub preview_segments: Option<Vec<Segment>>,
    /// Mentions in [`Commit::body`].
    #[serde(default, deserialize_with = "or_default")]
    pub mentions: Vec<Mention>,
    /// Absent once deleted.
    #[serde(default)]
    pub author: Option<Author>,
    /// RFC 3339; empty for a deleted commit.
    #[serde(default, deserialize_with = "or_default")]
    pub created_at: String,
    /// RFC 3339, when deleted.
    #[serde(default)]
    pub deleted_at: Option<String>,
    /// The commit on the web.
    #[serde(default, deserialize_with = "or_default")]
    pub html_url: String,
    /// Stars.
    #[serde(default, deserialize_with = "or_default")]
    pub stars_count: u64,
    /// Replies.
    #[serde(default, deserialize_with = "or_default")]
    pub replies_count: u64,
    /// Whether the connected account starred it.
    #[serde(default, deserialize_with = "or_default")]
    pub starred: bool,
    /// `approved`, or a moderation state.
    #[serde(default, deserialize_with = "or_default")]
    pub status: String,
    /// The commit this replies to.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The commit this quotes, one level deep.
    #[serde(default)]
    pub quoted_commit: Option<Box<Commit>>,
    /// The card for the first link, while or once the server fetched it.
    #[serde(default)]
    pub link_preview: Option<LinkPreview>,
    /// Attached images.
    #[serde(default, deserialize_with = "or_default")]
    pub images: Vec<Image>,
    /// The first replies, as the log shows them.
    #[serde(default)]
    pub reply_preview: Option<ReplyPreview>,
}

impl Commit {
    /// Whether the commit was deleted; it then has no author or text.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A piece of a commit's text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
#[non_exhaustive]
pub enum Segment {
    /// Plain text, which may contain mentions and links.
    Text {
        /// The text.
        text: String,
    },
    /// Inline code.
    Code {
        /// The code.
        code: String,
    },
    /// A code block, highlighted by the server.
    Block {
        /// Its language, when given.
        #[serde(default)]
        language: Option<String>,
        /// Its file name, when given.
        #[serde(default)]
        filename: Option<String>,
        /// The code as server-highlighted HTML.
        #[serde(default, deserialize_with = "or_default")]
        html: String,
    },
    /// A kind of segment this client does not know yet.
    #[serde(other)]
    Unknown,
}

/// A mention in a commit's body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Mention {
    /// The handle, without the `@`.
    pub handle: String,
    /// Whether it names a user or an organization.
    #[serde(default, deserialize_with = "or_default")]
    pub kind: AccountKind,
    /// Where it starts in the body, in characters (Unicode code points), at the `@`.
    pub offset: usize,
    /// Its length in characters, `@` included.
    pub length: usize,
}

/// A commit's author.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Author {
    /// Account id.
    pub id: String,
    /// Handle, without the `@`.
    pub handle: String,
    /// Display name.
    #[serde(default, deserialize_with = "or_default")]
    pub name: String,
    /// Avatar image.
    #[serde(default, deserialize_with = "or_default")]
    pub avatar_url: String,
    /// User or organization.
    #[serde(rename = "type", default, deserialize_with = "or_default")]
    pub kind: AccountKind,
    /// The author's lane colour.
    #[serde(default, deserialize_with = "lane")]
    pub lane: Lane,
}

/// Whether an account is a person or an organization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AccountKind {
    /// A person.
    #[default]
    User,
    /// An organization.
    Organization,
    /// A kind this client does not know yet.
    #[serde(other)]
    Other,
}

/// An author's lane colour: one of the web's palette entries, a colour of their own, or none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lane {
    /// An index into the web's palette.
    Index(i64),
    /// A `#rrggbb` colour.
    Color(String),
    /// No lane.
    #[default]
    None,
}

fn lane<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Lane, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Index(i64),
        Color(String),
    }
    Ok(match Option::<Raw>::deserialize(deserializer)? {
        Some(Raw::Index(index)) => Lane::Index(index),
        Some(Raw::Color(color)) => Lane::Color(color),
        None => Lane::None,
    })
}

/// A field's default when it is null as well as when it is absent: real responses send null for some strings, such
/// as an author's avatar or a link card's title.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// The card the server makes for a commit's first link.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct LinkPreview {
    /// The link.
    pub url: String,
    /// The page's title.
    #[serde(default, deserialize_with = "or_default")]
    pub title: String,
    /// The page's description.
    #[serde(default, deserialize_with = "or_default")]
    pub description: String,
    /// The site's name, when it gives one.
    #[serde(default)]
    pub site_name: Option<String>,
    /// The page's picture.
    #[serde(default)]
    pub image_url: Option<String>,
    /// Whether the server is still fetching it.
    #[serde(default, deserialize_with = "or_default")]
    pub is_pending: bool,
}

/// An image attached to a commit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Image {
    /// Image id.
    pub id: String,
    /// Where to fetch it.
    pub url: String,
    /// Width in pixels.
    #[serde(default, deserialize_with = "or_default")]
    pub width: u32,
    /// Height in pixels.
    #[serde(default, deserialize_with = "or_default")]
    pub height: u32,
    /// Its description, when the author gave one.
    #[serde(default)]
    pub description: Option<String>,
}

/// The first replies of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ReplyPreview {
    /// The replies, oldest first.
    #[serde(default, deserialize_with = "or_default")]
    pub data: Vec<Commit>,
}

/// One page of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Page<T> {
    /// The items, in the list's order.
    pub items: Vec<T>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<Cursor>,
}

impl<T> Page<T> {
    /// A page of `items`, followed by the page `next` points at, if any.
    #[must_use]
    pub fn new(items: Vec<T>, next: Option<Cursor>) -> Self {
        Self { items, next }
    }
}

/// Where a list's next page starts, as the server gave it. Opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(pub(crate) String);

impl Cursor {
    /// A cursor as the server gave it, such as one kept from an earlier page.
    #[must_use]
    pub fn new(cursor: impl Into<String>) -> Self {
        Self(cursor.into())
    }
}

/// A commit with the commits it replies to, from `GET /api/v1/commits/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Thread {
    /// The commit.
    #[serde(rename = "data")]
    pub commit: Commit,
    /// What it replies to, the root first.
    #[serde(default, deserialize_with = "or_default")]
    pub ancestors: Vec<Commit>,
}

/// An account as profiles and people lists show it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Account {
    /// Account id.
    #[serde(default, deserialize_with = "or_default")]
    pub id: String,
    /// Handle, without the `@`.
    pub handle: String,
    /// Display name.
    #[serde(default, deserialize_with = "or_default")]
    pub name: String,
    /// Bio.
    #[serde(default, deserialize_with = "or_default")]
    pub bio: String,
    /// Company.
    #[serde(default, deserialize_with = "or_default")]
    pub company: String,
    /// Location.
    #[serde(default, deserialize_with = "or_default")]
    pub location: String,
    /// Website.
    #[serde(default, deserialize_with = "or_default")]
    pub website: String,
    /// The profile on the web.
    #[serde(default, deserialize_with = "or_default")]
    pub html_url: String,
    /// Commits, when the server counts them here.
    #[serde(default)]
    pub commits_count: Option<u64>,
    /// Followers, when the server counts them here.
    #[serde(default)]
    pub followers_count: Option<u64>,
    /// Accounts followed, when the server counts them here.
    #[serde(default)]
    pub following_count: Option<u64>,
    /// Whether it follows the connected account, in people lists.
    #[serde(default)]
    pub follows_you: Option<bool>,
    /// Whether the connected account follows it, in people lists.
    #[serde(default)]
    pub is_followed: Option<bool>,
}

/// A person's profile, from `GET /api/v1/users/{handle}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Profile {
    /// The account.
    #[serde(rename = "data")]
    pub account: Account,
    /// Whether the connected account follows them.
    #[serde(default, deserialize_with = "or_default")]
    pub is_following: bool,
    /// Whether they follow the connected account.
    #[serde(default, deserialize_with = "or_default")]
    pub follows_you: bool,
    /// The commit they pinned.
    #[serde(default, rename = "pinned_commit")]
    pub pinned: Option<Commit>,
    /// Their commit streaks, in days.
    #[serde(default)]
    pub streaks: Option<Streaks>,
}

/// Commit streaks, in days.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Streaks {
    /// The streak going on now.
    #[serde(default, deserialize_with = "or_default")]
    pub current: u64,
    /// The longest one.
    #[serde(default, deserialize_with = "or_default")]
    pub longest: u64,
}

/// An organization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Organization {
    /// Handle, without the `@`.
    pub handle: String,
    /// Display name.
    #[serde(default, deserialize_with = "or_default")]
    pub name: String,
    /// Description.
    #[serde(default, deserialize_with = "or_default")]
    pub bio: String,
    /// Location.
    #[serde(default, deserialize_with = "or_default")]
    pub location: String,
    /// Website.
    #[serde(default, deserialize_with = "or_default")]
    pub website: String,
    /// The organization on the web.
    #[serde(default, deserialize_with = "or_default")]
    pub html_url: String,
    /// Followers.
    #[serde(default)]
    pub followers_count: Option<u64>,
    /// Its admins.
    #[serde(default, deserialize_with = "or_default")]
    pub admins: Vec<Account>,
}

/// A commit's text as the server would show it, from `POST /api/v1/commits/preview`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Preview {
    /// The text as it would be kept.
    #[serde(default, deserialize_with = "or_default")]
    pub body: String,
    /// The text as it would be shown.
    #[serde(rename = "body_segments", default, deserialize_with = "or_default")]
    pub segments: Vec<Segment>,
}

/// A notification, or a group of them about the same thing (several stars on one commit, say), from
/// `GET /api/v1/notifications`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Notification {
    /// Its id.
    pub id: String,
    /// What happened.
    #[serde(rename = "type", default, deserialize_with = "or_default")]
    pub kind: NotificationKind,
    /// Why, for a moderation decision.
    #[serde(default, deserialize_with = "or_default")]
    pub reason: String,
    /// When it was read (RFC 3339); `None` while it is unread.
    #[serde(default)]
    pub read_at: Option<String>,
    /// When it happened (RFC 3339).
    #[serde(default, deserialize_with = "or_default")]
    pub created_at: String,
    /// Who did it, or the latest of a group.
    #[serde(default)]
    pub actor: Option<Author>,
    /// Who did it, for a group; not always all of them (see `actors_count`).
    #[serde(default, deserialize_with = "or_default")]
    pub actors: Vec<Author>,
    /// The group it stands for.
    #[serde(default, deserialize_with = "or_default")]
    pub group_id: String,
    /// How many notifications the group holds.
    #[serde(default, deserialize_with = "or_default")]
    pub event_count: u64,
    /// How many accounts did them.
    #[serde(default, deserialize_with = "or_default")]
    pub actors_count: u64,
    /// The commit it is about: the reply, quote or mention for those; the connected account's own commit for stars
    /// and moderation.
    #[serde(default)]
    pub commit: Option<Box<Commit>>,
}

/// What a notification says happened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NotificationKind {
    /// Someone followed the account.
    Followed,
    /// Someone starred one of its commits.
    Starred,
    /// Someone replied to one of its commits.
    Replied,
    /// Someone quoted one of its commits.
    Quoted,
    /// Someone mentioned it.
    Mentioned,
    /// A commit of its was approved.
    Approved,
    /// A commit of its was rejected.
    Rejected,
    /// A kind this client does not know yet.
    #[default]
    #[serde(other)]
    Other,
}

/// A page of the inbox, with when the inbox was read: notifications read since then are new to this visit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Inbox {
    /// The notifications, newest first.
    pub page: Page<Notification>,
    /// When reading the inbox marked it read (RFC 3339).
    pub read_at: String,
}

impl Inbox {
    /// A page of the inbox read at `read_at`, such as one kept from an earlier visit.
    #[must_use]
    pub fn new(page: Page<Notification>, read_at: impl Into<String>) -> Self {
        Self {
            page,
            read_at: read_at.into(),
        }
    }
}

/// What a search for people and organizations finds, from `GET /api/v1/search/profiles`: one list each, best match
/// first, not paged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ProfileSearch {
    /// People.
    #[serde(rename = "users", default, deserialize_with = "or_default")]
    pub people: Vec<Account>,
    /// Organizations.
    #[serde(default, deserialize_with = "or_default")]
    pub organizations: Vec<Organization>,
}

/// What a handle names: a person's profile or an organization's page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AccountPage {
    /// A person's profile; boxed, as it carries a whole pinned commit.
    Person(Box<Profile>),
    /// An organization's page.
    Organization(OrganizationPage),
}

/// An organization's page, from `GET /api/v1/organizations/{handle}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct OrganizationPage {
    /// The organization.
    #[serde(rename = "data")]
    pub organization: Organization,
    /// Whether the connected account follows it.
    #[serde(default, deserialize_with = "or_default")]
    pub is_following: bool,
}
