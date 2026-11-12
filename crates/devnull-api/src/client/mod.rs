//! The signed HTTP client.

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use ssh_key::rand_core::{OsRng, RngCore};
use url::Url;

use crate::model::{
    Account, AccountKind, AccountPage, Commit, Cursor, Inbox, Me, Notification, OrganizationPage,
    Page, Preview, Profile, ProfileSearch, Thread,
};
use crate::signing::{Namespace, SignError, Signer, canonical_message};

/// The server everyone uses.
pub const ORIGIN: &str = "https://dev-null.sh";

/// `value` when it can go into a URL path as one segment: a handle or an id, made of letters, digits, `-` and `_`.
fn segment(value: &str) -> Result<&str, ApiError> {
    let fits = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if fits {
        Ok(value)
    } else {
        Err(ApiError::Invalid(format!("not a handle or id: {value:?}")))
    }
}

/// A commit to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Draft {
    /// Its text.
    pub body: String,
    /// The commit it answers, if any.
    pub answering: Answering,
    /// The organization it is posted as, by handle (one the account is a member of); `None` for the account itself.
    pub author: Option<String>,
}

impl Draft {
    /// `body`, answering what `answering` says, posted as the account itself.
    #[must_use]
    pub fn new(answering: Answering, body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            answering,
            author: None,
        }
    }
}

/// What a commit answers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Answering {
    /// Nothing: it is a commit of its own.
    #[default]
    Nothing,
    /// The commit with this id, as a reply.
    Reply(String),
    /// The commit with this id, as a quote.
    Quote(String),
}

/// A category of notifications, as the inbox filters them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Category {
    /// Replies to the account's commits.
    Replies,
    /// Quotes of its commits.
    Quotes,
    /// Stars on its commits.
    Stars,
    /// Mentions of it.
    Mentions,
    /// New followers.
    Follows,
    /// Moderation decisions on its commits.
    Moderation,
}

impl Category {
    /// Every category, in the order the web lists them.
    pub const ALL: [Self; 6] = [
        Self::Replies,
        Self::Quotes,
        Self::Stars,
        Self::Mentions,
        Self::Follows,
        Self::Moderation,
    ];

    /// Its name in the inbox's filter, and in buffer names: `replies`, `quotes`, …
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Replies => "replies",
            Self::Quotes => "quotes",
            Self::Stars => "stars",
            Self::Mentions => "mentions",
            Self::Follows => "follows",
            Self::Moderation => "moderation",
        }
    }

    /// The category of that name.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }
}

/// Where following `handle` happens: a person's or an organization's.
fn follow_path(handle: &str, kind: AccountKind) -> Result<String, ApiError> {
    let handle = segment(handle)?;
    Ok(match kind {
        AccountKind::Organization => format!("/api/v1/organizations/{handle}/follow"),
        _ => format!("/api/v1/users/{handle}/follow"),
    })
}

/// The longest search term the server takes, in characters.
pub const MAX_SEARCH: usize = 100;

/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The most a response may hold, so a broken server cannot exhaust memory.
const MAX_RESPONSE: usize = 4 << 20;

/// A connection to dev-null.sh as one device, signing every request with its key.
#[derive(Debug, Clone)]
pub struct Client {
    origin: Url,
    http: reqwest::Client,
    signer: Arc<Signer>,
}

/// One of the three logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Log {
    /// People and organizations the account follows.
    Following,
    /// Everyone, newest first.
    Latest,
    /// Ranked by the server.
    Trending,
}

impl Log {
    /// Its name in paths and buffer names: `following`, `latest`, `trending`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Following => "following",
            Self::Latest => "latest",
            Self::Trending => "trending",
        }
    }
}

/// A profile's lists of people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum People {
    /// Who follows them.
    Followers,
    /// Whom they follow.
    Following,
}

impl People {
    /// Its name in paths and buffer names: `followers`, `following`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Followers => "followers",
            Self::Following => "following",
        }
    }
}

/// An organization page's lists of commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrganizationTab {
    /// Commits made as the organization.
    Commits,
    /// Commits mentioning it.
    Mentions,
}

impl OrganizationTab {
    /// Its name in paths and buffer names: `commits`, `mentions`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Commits => "commits",
            Self::Mentions => "mentions",
        }
    }
}

/// An origin that is not `https://host` (or `http://` on loopback, for tests).
#[derive(Debug, thiserror::Error)]
#[error("the server must be an https origin with no path, or http on loopback: {0}")]
pub struct InvalidOrigin(pub(crate) String);

/// Why a request did not succeed, by what a caller can do about it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApiError {
    /// The server does not know this device, or it was revoked: connect again.
    #[error("this device is not approved; connect it again")]
    NotApproved,
    /// The server refused the signature, most often because the clock is more than 5 minutes off.
    #[error("{message}")]
    BadSignature {
        /// The server's explanation.
        message: String,
    },
    /// What was asked for does not exist.
    #[error("not found")]
    NotFound,
    /// Too many requests; try again after the server's delay, when it gave one.
    #[error("rate limited")]
    RateLimited {
        /// The `Retry-After` header: seconds, or an HTTP date.
        retry_after: Option<String>,
    },
    /// Any other refusal.
    #[error("{message}")]
    Refused {
        /// HTTP status.
        status: u16,
        /// The server's error code, when it gave one.
        code: Option<String>,
        /// The server's message.
        message: String,
    },
    /// The server could not be reached, or the connection broke.
    #[error("cannot reach the server; check your connection and try again")]
    Network(#[source] reqwest::Error),
    /// The response was not what the API returns.
    #[error("the server returned an invalid response: {0}")]
    Invalid(String),
    /// The request could not be signed.
    #[error(transparent)]
    Sign(#[from] SignError),
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
    #[serde(default)]
    unread_notifications_count: u64,
}

/// A page of the inbox as sent.
#[derive(Deserialize)]
struct InboxEnvelope {
    data: Vec<Notification>,
    #[serde(default)]
    read_at: String,
    #[serde(default)]
    meta: ListMeta,
}

#[derive(Deserialize)]
struct List<T> {
    data: Vec<T>,
    #[serde(default)]
    meta: ListMeta,
}

#[derive(Default, Deserialize)]
struct ListMeta {
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Default, Deserialize)]
struct Failure {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

impl Client {
    /// A client for `origin` (normally [`ORIGIN`]) signing as `signer`.
    ///
    /// # Errors
    ///
    /// When `origin` is not an https origin with no path; plain http is accepted only on loopback, for tests.
    pub fn new(origin: &str, signer: Signer) -> Result<Self, InvalidOrigin> {
        let invalid = || InvalidOrigin(origin.to_owned());
        let url = Url::parse(origin).map_err(|_| invalid())?;
        let loopback = matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
            || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            || url.host_str() == Some("localhost");
        let plain = url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty();
        if !plain || !(url.scheme() == "https" || (url.scheme() == "http" && loopback)) {
            return Err(invalid());
        }
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("dn/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| invalid())?;
        Ok(Self {
            origin: url,
            http,
            signer: Arc::new(signer),
        })
    }

    /// The device key's fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        self.signer.fingerprint()
    }

    /// The connected account.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::NotApproved`] means the device must connect again.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn me(&self) -> Result<Me, ApiError> {
        let envelope: Envelope<Me> = self.get("/api/v1/me", &[], None).await?;
        Ok(Me {
            unread_notifications: envelope.unread_notifications_count,
            ..envelope.data
        })
    }

    /// A page of `log`: the first, or the one `next` points at.
    ///
    /// # Errors
    ///
    /// See [`ApiError`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn log(&self, log: Log, next: Option<&Cursor>) -> Result<Page<Commit>, ApiError> {
        self.list(&format!("/api/v1/log/{}", log.name()), &[], next)
            .await
    }

    /// `POST /api/v1/devices`: exchanges an approval `code` and its PKCE `verifier` for this device's approval.
    pub(crate) async fn enroll(&self, code: &str, verifier: &str) -> Result<Me, ApiError> {
        let url = self
            .origin
            .join("/api/v1/devices")
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
        let body = serde_json::json!({ "code": code, "code_verifier": verifier }).to_string();
        let bytes = self.send(Method::POST, url, body.into_bytes()).await?;
        let envelope: Envelope<Me> =
            serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))?;
        Ok(envelope.data)
    }

    /// `PUT /api/v1/me/terms`: accepts the terms of `version`, as shown to the user.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::Invalid`] when the server does not confirm that version as accepted.
    ///
    /// # Cancel Safety
    ///
    /// Not cancel safe: the server may record the acceptance even when the response is lost.
    pub async fn accept_terms(&self, version: &str) -> Result<Me, ApiError> {
        let url = self
            .origin
            .join("/api/v1/me/terms")
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
        let body = serde_json::json!({ "version": version }).to_string();
        let bytes = self.send(Method::PUT, url, body.into_bytes()).await?;
        let envelope: Envelope<Me> =
            serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))?;
        if !envelope.data.terms_accepted || envelope.data.terms_version.as_deref() != Some(version)
        {
            return Err(ApiError::Invalid(
                "the server did not confirm the terms as accepted".to_owned(),
            ));
        }
        Ok(envelope.data)
    }

    /// A commit and what it replies to.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::NotFound`] for a commit that does not exist, [`ApiError::Invalid`] without a
    /// request for an id that is not one.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn thread(&self, id: &str) -> Result<Thread, ApiError> {
        self.get(&format!("/api/v1/commits/{}", segment(id)?), &[], None)
            .await
    }

    /// A page of a commit's replies, oldest first.
    ///
    /// # Errors
    ///
    /// As [`Client::thread`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn replies(
        &self,
        id: &str,
        after: Option<&Cursor>,
    ) -> Result<Page<Commit>, ApiError> {
        self.list(
            &format!("/api/v1/commits/{}/replies", segment(id)?),
            &[],
            after,
        )
        .await
    }

    /// A person's profile.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::NotFound`] for no such person, [`ApiError::Invalid`] without a request for a
    /// handle that is not one.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn profile(&self, handle: &str) -> Result<Profile, ApiError> {
        self.get(&format!("/api/v1/users/{}", segment(handle)?), &[], None)
            .await
    }

    /// What `handle` names: a person's profile, or else an organization's page. For when the kind of account is not
    /// known, as for a handle typed in; it costs a second request for an organization.
    ///
    /// # Errors
    ///
    /// As [`Client::profile`]; [`ApiError::NotFound`] when neither a person nor an organization has the handle.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn account(&self, handle: &str) -> Result<AccountPage, ApiError> {
        match self.profile(handle).await {
            Err(ApiError::NotFound) => self
                .organization(handle)
                .await
                .map(|page| AccountPage::Organization(Box::new(page))),
            other => other.map(|profile| AccountPage::Person(Box::new(profile))),
        }
    }

    /// People and organizations matching `term`, as the web's search finds them. Blanks around `term` are ignored.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::Invalid`] without a request for a term longer than [`MAX_SEARCH`] characters.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn search_profiles(&self, term: &str) -> Result<ProfileSearch, ApiError> {
        let term = term.trim();
        if term.chars().count() > MAX_SEARCH {
            return Err(ApiError::Invalid(format!(
                "searches are limited to {MAX_SEARCH} characters"
            )));
        }
        self.get("/api/v1/search/profiles", &[("q", term)], None)
            .await
    }

    /// A page of the inbox, of the categories `only` (every category when empty), newest first.
    ///
    /// Reading the inbox marks **every** notification read on the server, whatever the categories asked for; the
    /// unread count in [`Client::me`] does not.
    ///
    /// # Errors
    ///
    /// See [`ApiError`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe, though the server may have marked the inbox read by the time it is canceled.
    pub async fn notifications(
        &self,
        only: &[Category],
        after: Option<&Cursor>,
    ) -> Result<Inbox, ApiError> {
        let filter = if only.is_empty() {
            "all".to_owned()
        } else {
            only.iter()
                .map(|category| category.name())
                .collect::<Vec<_>>()
                .join(",")
        };
        let inbox: InboxEnvelope = self
            .get("/api/v1/notifications", &[("type", &filter)], after)
            .await?;
        Ok(Inbox {
            page: Page {
                items: inbox.data,
                next: inbox
                    .meta
                    .next_cursor
                    .filter(|cursor| !cursor.is_empty())
                    .map(Cursor),
            },
            read_at: inbox.read_at,
        })
    }

    /// A page of the commits the account starred, as the server orders them.
    ///
    /// # Errors
    ///
    /// See [`ApiError`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn starred(&self, after: Option<&Cursor>) -> Result<Page<Commit>, ApiError> {
        self.list("/api/v1/starred", &[], after).await
    }

    /// `body` as the server would keep and show it, without publishing anything.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::Refused`] with the server's message for a text it would refuse.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it changes nothing.
    pub async fn preview(&self, body: &str) -> Result<Preview, ApiError> {
        let bytes = self
            .write(
                Method::POST,
                "/api/v1/commits/preview",
                Some(serde_json::json!({ "body": body })),
            )
            .await?;
        let envelope: Envelope<Preview> =
            serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))?;
        Ok(envelope.data)
    }

    /// Publishes `draft`; the commit as published, which moderation may hold (see [`Commit::status`]).
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::Refused`] with the server's message for a text it refuses.
    ///
    /// # Cancel Safety
    ///
    /// Not cancel safe: the commit may be published even when the response is lost.
    pub async fn publish(&self, draft: &Draft) -> Result<Commit, ApiError> {
        let mut payload = serde_json::json!({ "body": draft.body });
        match &draft.answering {
            Answering::Nothing => {}
            Answering::Reply(to) => payload["parent_id"] = segment(to)?.into(),
            Answering::Quote(of) => payload["quoted_commit_id"] = segment(of)?.into(),
        }
        if let Some(author) = &draft.author {
            payload["author"] = segment(author)?.into();
        }
        let bytes = self
            .write(Method::POST, "/api/v1/commits", Some(payload))
            .await?;
        let envelope: Envelope<Commit> =
            serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))?;
        Ok(envelope.data)
    }

    /// Deletes the account's commit `id`.
    ///
    /// # Errors
    ///
    /// See [`ApiError`].
    ///
    /// # Cancel Safety
    ///
    /// Not cancel safe: the commit may be deleted even when the response is lost.
    pub async fn delete_commit(&self, id: &str) -> Result<(), ApiError> {
        let path = format!("/api/v1/commits/{}", segment(id)?);
        self.write(Method::DELETE, &path, None).await.map(drop)
    }

    /// Stars commit `id`; the commit as the server now counts it.
    ///
    /// # Errors
    ///
    /// See [`ApiError`]; [`ApiError::Invalid`] when the server does not confirm the star.
    ///
    /// # Cancel Safety
    ///
    /// Not cancel safe: the star may be given even when the response is lost.
    pub async fn star(&self, id: &str) -> Result<Commit, ApiError> {
        self.set_star(id, true).await
    }

    /// Takes the star off commit `id`; the commit as the server now counts it.
    ///
    /// # Errors
    ///
    /// As [`Client::star`].
    ///
    /// # Cancel Safety
    ///
    /// As [`Client::star`].
    pub async fn unstar(&self, id: &str) -> Result<Commit, ApiError> {
        self.set_star(id, false).await
    }

    async fn set_star(&self, id: &str, on: bool) -> Result<Commit, ApiError> {
        let path = format!("/api/v1/commits/{}/star", segment(id)?);
        let method = if on { Method::POST } else { Method::DELETE };
        let bytes = self.write(method, &path, None).await?;
        let envelope: Envelope<Commit> =
            serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))?;
        let commit = envelope.data;
        if commit.id != id || commit.starred != on {
            return Err(ApiError::Invalid(
                "the server did not confirm the star".to_owned(),
            ));
        }
        Ok(commit)
    }

    /// Follows the person or organization `handle`.
    ///
    /// # Errors
    ///
    /// See [`ApiError`].
    ///
    /// # Cancel Safety
    ///
    /// Not cancel safe: the follow may happen even when the response is lost.
    pub async fn follow(&self, handle: &str, kind: AccountKind) -> Result<(), ApiError> {
        self.write(Method::POST, &follow_path(handle, kind)?, None)
            .await
            .map(drop)
    }

    /// Stops following the person or organization `handle`.
    ///
    /// # Errors
    ///
    /// As [`Client::follow`].
    ///
    /// # Cancel Safety
    ///
    /// As [`Client::follow`].
    pub async fn unfollow(&self, handle: &str, kind: AccountKind) -> Result<(), ApiError> {
        self.write(Method::DELETE, &follow_path(handle, kind)?, None)
            .await
            .map(drop)
    }

    /// Sends a signed `method` request to `path` with `payload` as its JSON body, if any; the response's body.
    async fn write(
        &self,
        method: Method,
        path: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<Vec<u8>, ApiError> {
        let url = self
            .origin
            .join(path)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
        let body = payload.map_or_else(Vec::new, |payload| payload.to_string().into_bytes());
        self.send(method, url, body).await
    }

    /// A page of a person's commits, or of their replies.
    ///
    /// # Errors
    ///
    /// As [`Client::profile`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn commits_of(
        &self,
        handle: &str,
        replies: bool,
        after: Option<&Cursor>,
    ) -> Result<Page<Commit>, ApiError> {
        let query: &[(&str, &str)] = if replies { &[("replies", "1")] } else { &[] };
        self.list(
            &format!("/api/v1/users/{}/commits", segment(handle)?),
            query,
            after,
        )
        .await
    }

    /// A page of who follows a person, or whom they follow.
    ///
    /// # Errors
    ///
    /// As [`Client::profile`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn people(
        &self,
        handle: &str,
        which: People,
        after: Option<&Cursor>,
    ) -> Result<Page<Account>, ApiError> {
        self.list(
            &format!("/api/v1/users/{}/{}", segment(handle)?, which.name()),
            &[],
            after,
        )
        .await
    }

    /// An organization's page.
    ///
    /// # Errors
    ///
    /// As [`Client::profile`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn organization(&self, handle: &str) -> Result<OrganizationPage, ApiError> {
        self.get(
            &format!("/api/v1/organizations/{}", segment(handle)?),
            &[],
            None,
        )
        .await
    }

    /// A page of an organization's commits, or of commits mentioning it.
    ///
    /// # Errors
    ///
    /// As [`Client::profile`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn organization_commits(
        &self,
        handle: &str,
        tab: OrganizationTab,
        after: Option<&Cursor>,
    ) -> Result<Page<Commit>, ApiError> {
        self.list(
            &format!("/api/v1/organizations/{}/{}", segment(handle)?, tab.name()),
            &[],
            after,
        )
        .await
    }

    /// A page of a list at `path`.
    async fn list<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        after: Option<&Cursor>,
    ) -> Result<Page<T>, ApiError> {
        let list: List<T> = self.get(path, query, after).await?;
        Ok(Page {
            items: list.data,
            next: list
                .meta
                .next_cursor
                .filter(|cursor| !cursor.is_empty())
                .map(Cursor),
        })
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        cursor: Option<&Cursor>,
    ) -> Result<T, ApiError> {
        let mut url = self
            .origin
            .join(path)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
        for (key, value) in query {
            url.query_pairs_mut().append_pair(key, value);
        }
        if let Some(Cursor(cursor)) = cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        let bytes = self.send(Method::GET, url, Vec::new()).await?;
        serde_json::from_slice(&bytes).map_err(|error| ApiError::Invalid(error.to_string()))
    }

    /// Sends one signed request and returns the body of a successful response.
    async fn send(&self, method: Method, url: Url, body: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let mut nonce = [0u8; 16];
        OsRng.fill_bytes(&mut nonce);
        let uri = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        let message = canonical_message(
            timestamp,
            &nonce,
            method.as_str(),
            &uri,
            &[&String::from_utf8_lossy(&body)],
        );
        let signature = self.sign(message).await?;
        let nonce: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let response = self
            .http
            .request(method, url)
            .header("X-Devnull-Timestamp", timestamp.to_string())
            .header("X-Devnull-Nonce", nonce)
            .header("X-SSH-Signature", signature)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(ApiError::Network)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = response.bytes().await.map_err(ApiError::Network)?;
        if bytes.len() > MAX_RESPONSE {
            return Err(ApiError::Invalid("response too large".to_owned()));
        }
        if status.is_success() {
            return Ok(bytes.to_vec());
        }
        let failure: Failure = serde_json::from_slice(&bytes).unwrap_or_default();
        let message = failure.message.unwrap_or_else(|| status.to_string());
        Err(match (status, failure.code.as_deref()) {
            (StatusCode::UNAUTHORIZED, Some("device_not_approved")) => ApiError::NotApproved,
            (StatusCode::UNAUTHORIZED, Some("invalid_signature")) => {
                ApiError::BadSignature { message }
            }
            (StatusCode::NOT_FOUND, _) => ApiError::NotFound,
            (StatusCode::TOO_MANY_REQUESTS, _) => ApiError::RateLimited { retry_after },
            (status, code) => ApiError::Refused {
                status: status.as_u16(),
                code: code.map(str::to_owned),
                message,
            },
        })
    }

    /// Signs off the async runtime when ssh-agent signs, since that blocks on its socket.
    async fn sign(&self, message: String) -> Result<String, SignError> {
        if !self.signer.uses_agent() {
            return self.signer.sign(Namespace::Api, message.as_bytes());
        }
        let signer = Arc::clone(&self.signer);
        match tokio::task::spawn_blocking(move || signer.sign(Namespace::Api, message.as_bytes()))
            .await
        {
            Ok(signed) => signed,
            // Signing does not panic; if it ever does, that is a bug to surface, not an error to report.
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
}
