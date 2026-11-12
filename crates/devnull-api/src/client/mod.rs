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

use crate::model::{Commit, Cursor, Me, Page};
use crate::signing::{Namespace, SignError, Signer, canonical_message};

/// The server everyone uses.
pub const ORIGIN: &str = "https://dev-null.sh";

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
        let envelope: Envelope<Me> = self.get("/api/v1/me", None).await?;
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
        let path = format!("/api/v1/log/{}", log.name());
        let list: List<Commit> = self.get(&path, next).await?;
        Ok(Page {
            items: list.data,
            next: list
                .meta
                .next_cursor
                .filter(|cursor| !cursor.is_empty())
                .map(Cursor),
        })
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

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        cursor: Option<&Cursor>,
    ) -> Result<T, ApiError> {
        let mut url = self
            .origin
            .join(path)
            .map_err(|error| ApiError::Invalid(error.to_string()))?;
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
