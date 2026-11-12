//! Pictures dev-null.sh links to (commit images, avatars, link-card pictures), downloaded unsigned and only from
//! the public internet over HTTPS, or from the API's own origin, as the Go client does: a picture's address is
//! chosen by whoever posted it, and must not reach into the user's network.

#[cfg(test)]
mod tests;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{StatusCode, redirect};
use url::{Host, Url};

use crate::client::InvalidOrigin;

/// The largest picture downloaded, in bytes: the server's own limit for uploads.
pub const MAX_BYTES: usize = 10 << 20;

/// How long a download may take.
const TIMEOUT: Duration = Duration::from_secs(12);

/// How many redirects a download follows.
const MAX_REDIRECTS: usize = 3;

/// Why a picture could not be downloaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MediaError {
    /// Its address is not one pictures are downloaded from: not HTTPS, or off the public internet.
    #[error("not an address pictures are downloaded from")]
    Refused,
    /// It is larger than [`MAX_BYTES`].
    #[error("larger than {MAX_BYTES} bytes")]
    TooLarge,
    /// The server answered something else than the picture.
    #[error("the server answered {0}")]
    Status(u16),
    /// It could not be reached, or the download failed.
    #[error("cannot download it")]
    Network(#[source] reqwest::Error),
}

/// Downloads pictures.
#[derive(Debug, Clone)]
pub struct Media {
    client: reqwest::Client,
    origin: Url,
}

impl Media {
    /// Downloads pictures, trusting `origin` (the API's) besides the public internet.
    ///
    /// # Errors
    ///
    /// When `origin` is not a valid one (see [`crate::Client::new`]).
    pub fn new(origin: &str) -> Result<Self, InvalidOrigin> {
        let invalid = || InvalidOrigin(origin.to_owned());
        let origin = Url::parse(origin).map_err(|_| invalid())?;
        if origin.host_str().is_none() {
            return Err(invalid());
        }
        let checked = origin.clone();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(TIMEOUT)
            .dns_resolver(PublicOnly {
                trusted: origin.host_str().unwrap_or_default().to_owned(),
            })
            .redirect(redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("too many redirects")
                } else if allowed(attempt.url(), &checked) {
                    attempt.follow()
                } else {
                    attempt.error(Refused)
                }
            }))
            .build()
            .map_err(|_| invalid())?;
        Ok(Self { client, origin })
    }

    /// The picture at `url`.
    ///
    /// # Errors
    ///
    /// See [`MediaError`].
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: it only reads.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>, MediaError> {
        let url = Url::parse(url).map_err(|_| MediaError::Refused)?;
        if !allowed(&url, &self.origin) {
            return Err(MediaError::Refused);
        }
        let mut response = self
            .client
            .get(url)
            .header("Accept", "image/png,image/jpeg,image/webp,image/gif")
            .send()
            .await
            .map_err(failure)?;
        if response.status() != StatusCode::OK {
            return Err(MediaError::Status(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BYTES as u64)
        {
            return Err(MediaError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failure)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_BYTES {
                return Err(MediaError::TooLarge);
            }
        }
        Ok(bytes)
    }
}

/// A redirect refused by [`allowed`].
#[derive(Debug, thiserror::Error)]
#[error("refused")]
struct Refused;

/// What a failed download means: refused when a redirect or the resolver refused the address.
fn failure(error: reqwest::Error) -> MediaError {
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        if cause.is::<Refused>() || cause.is::<NotPublic>() {
            return MediaError::Refused;
        }
        source = cause.source();
    }
    MediaError::Network(error)
}

/// Whether pictures are downloaded from `url`: HTTPS without credentials to a name, or to an address on the public
/// internet; or anything on `origin`'s host and port, which is the API's (over plain HTTP too when it is local).
fn allowed(url: &Url, origin: &Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let same_origin = url.scheme() == origin.scheme()
        && url.host_str() == origin.host_str()
        && url.port_or_known_default() == origin.port_or_known_default();
    if same_origin {
        return true;
    }
    if url.scheme() != "https" {
        return false;
    }
    match url.host() {
        Some(Host::Domain(_)) => true,
        Some(Host::Ipv4(ip)) => is_public(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => is_public(IpAddr::V6(ip)),
        None => false,
    }
}

/// Whether `ip` is on the public internet: not private, loopback, link-local, shared, reserved for documentation,
/// multicast or unspecified.
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(mapped) => is_public_v4(mapped),
            None => is_public_v6(ip),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    let shared = a == 100 && (64..128).contains(&b);
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || shared
        || a == 0
        || a >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    let unique_local = first & 0xfe00 == 0xfc00;
    let link_local = first & 0xffc0 == 0xfe80;
    let documentation = first == 0x2001 && ip.segments()[1] == 0x0db8;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || unique_local
        || link_local
        || documentation)
}

/// A name resolved to no address on the public internet.
#[derive(Debug, thiserror::Error)]
#[error("{0} is not on the public internet")]
struct NotPublic(String);

/// Resolves names to their addresses on the public internet only, but for the API's own host.
struct PublicOnly {
    trusted: String,
}

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let trusted = host.eq_ignore_ascii_case(&self.trusted);
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| trusted || is_public(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(Box::new(NotPublic(host)) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}
