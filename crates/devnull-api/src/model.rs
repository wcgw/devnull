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
