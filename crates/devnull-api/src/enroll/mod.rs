//! Connecting a device (D8): browser approval with PKCE, coming back to a loopback listener.

#[cfg(test)]
mod tests;

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ssh_key::rand_core::{OsRng, RngCore};
use ssh_key::sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

use crate::client::{ApiError, Client, InvalidOrigin};
use crate::model::Me;
use crate::signing::Signer;

/// How long the browser has to send its request once connected.
const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// The most of the browser's request that is read: its request line and headers.
const MAX_REQUEST: usize = 4096;

/// Why connecting failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EnrollError {
    /// The approval was canceled in the browser.
    #[error("browser approval was canceled")]
    Denied,
    /// A pasted callback link is not this attempt's.
    #[error("that is not the callback link of this connection attempt")]
    BadCallback,
    /// The loopback listener could not be set up or failed.
    #[error("cannot listen for the browser's return")]
    Listener(#[source] std::io::Error),
    /// The server is not a valid origin.
    #[error(transparent)]
    Origin(#[from] InvalidOrigin),
    /// The server refused the code, or could not be reached.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// The device was approved for one account and `/me` answered for another.
    #[error("the account changed while connecting; connect again")]
    IdentityChanged,
}

/// One attempt at connecting a device key: open [`Enrollment::authorize_url`] in the browser, then
/// [`Enrollment::wait`] for it to come back (or take the callback link from [`Enrollment::code_from_url`]) and
/// [`Enrollment::redeem`] the code.
#[derive(Debug)]
pub struct Enrollment {
    origin: String,
    signer: Signer,
    /// PKCE: the secret whose hash the authorize URL carries.
    verifier: String,
    /// Ties the browser's return to this attempt.
    state: String,
    listener: TcpListener,
    /// Where the browser comes back: `http://127.0.0.1:<port>/callback`.
    redirect: String,
    authorize_url: String,
}

impl Enrollment {
    /// Starts connecting `signer`'s key to `origin` as a device called `device_name`, listening on a free loopback
    /// port for the browser's return.
    ///
    /// # Errors
    ///
    /// When `origin` is not valid or the listener cannot be bound.
    pub async fn start(
        origin: &str,
        signer: Signer,
        device_name: &str,
    ) -> Result<Self, EnrollError> {
        let mut authorize = Url::parse(origin)
            .map_err(|_| EnrollError::Origin(InvalidOrigin(origin.to_owned())))?
            .join("/mobile/authorize")
            .map_err(|_| EnrollError::Origin(InvalidOrigin(origin.to_owned())))?;
        let verifier = URL_SAFE_NO_PAD.encode(random::<32>());
        let state: String = random::<16>()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(EnrollError::Listener)?;
        let port = listener.local_addr().map_err(EnrollError::Listener)?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        authorize
            .query_pairs_mut()
            .append_pair("fingerprint", &signer.fingerprint())
            .append_pair("device_name", device_name)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("redirect_uri", &redirect);
        Ok(Self {
            origin: origin.to_owned(),
            signer,
            verifier,
            state,
            listener,
            redirect,
            authorize_url: authorize.into(),
        })
    }

    /// The page to open in the browser.
    #[must_use]
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// The key's fingerprint, to compare with the one the approval page shows.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        self.signer.fingerprint()
    }

    /// Waits for the browser to come back with this attempt's code. Requests for anything else are answered with
    /// an error and the wait goes on.
    ///
    /// # Errors
    ///
    /// [`EnrollError::Denied`] when approval was canceled, [`EnrollError::Listener`] when the listener fails.
    ///
    /// # Cancel Safety
    ///
    /// Cancel safe: a connection being read is dropped, and the browser can retry.
    pub async fn wait(&self) -> Result<String, EnrollError> {
        loop {
            let (stream, _) = self
                .listener
                .accept()
                .await
                .map_err(EnrollError::Listener)?;
            if let Some(outcome) = self.answer(stream).await {
                return outcome;
            }
        }
    }

    /// The code in a callback link pasted from the browser's address bar, for when the browser cannot reach this
    /// machine's loopback (over SSH, for one).
    ///
    /// # Errors
    ///
    /// [`EnrollError::BadCallback`] when the link is not this attempt's, [`EnrollError::Denied`] when it says
    /// approval was canceled.
    pub fn code_from_url(&self, pasted: &str) -> Result<String, EnrollError> {
        let url = Url::parse(pasted.trim()).map_err(|_| EnrollError::BadCallback)?;
        // Scheme, host, port and path, exactly as the redirect.
        if url[..url::Position::AfterPath] != self.redirect {
            return Err(EnrollError::BadCallback);
        }
        self.code_from_query(&url).ok_or(EnrollError::BadCallback)?
    }

    /// Exchanges `code` for the device's approval, then checks `/me` answers for the same account.
    ///
    /// # Errors
    ///
    /// When the server refuses the code or cannot be reached, or the account changed in between.
    pub async fn redeem(self, code: &str) -> Result<(Client, Me), EnrollError> {
        let client = Client::new(&self.origin, self.signer)?;
        let approved = client.enroll(code, &self.verifier).await?;
        let me = client.me().await?;
        if me.id != approved.id {
            return Err(EnrollError::IdentityChanged);
        }
        Ok((client, me))
    }

    /// Reads one request from the browser and answers it. `Some` with the outcome when it was this attempt's
    /// callback.
    async fn answer(&self, mut stream: TcpStream) -> Option<Result<String, EnrollError>> {
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        let read = async {
            while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < MAX_REQUEST {
                let n = stream.read(&mut chunk).await.ok()?;
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }
            Some(())
        };
        tokio::time::timeout(READ_TIMEOUT, read).await.ok()??;
        let request = String::from_utf8_lossy(&request);
        let mut lines = request.lines();
        let target = lines
            .next()?
            .strip_prefix("GET ")?
            .split(' ')
            .next()?
            .to_owned();
        let host = lines
            .take_while(|line| !line.is_empty())
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
            })
            .map(|(_, value)| value.trim().to_owned())
            .unwrap_or_default();
        let url = Url::parse(&format!("http://{host}{target}")).ok();
        let expected_host = self
            .redirect
            .trim_start_matches("http://")
            .trim_end_matches("/callback");
        let outcome = url
            .filter(|url| host == expected_host && url.path() == "/callback")
            .and_then(|url| self.code_from_query(&url));
        let (status, text) = match &outcome {
            None if host != expected_host => ("404 Not Found", ""),
            None => ("400 Bad Request", ""),
            Some(Ok(_)) => (
                "200 OK",
                "Done. You can close this tab and return to your terminal.\n",
            ),
            Some(Err(_)) => ("200 OK", "Approval was canceled. You can close this tab.\n"),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\n\
             Referrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        // The browser may already have gone; the outcome stands either way.
        let _ = stream.write_all(response.as_bytes()).await;
        outcome
    }

    /// This attempt's code, or its denial, from a callback's query; `None` when the state is not this attempt's or
    /// the query carries neither.
    fn code_from_query(&self, url: &Url) -> Option<Result<String, EnrollError>> {
        let params: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let value = |key| {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        if value("state")? != self.state {
            return None;
        }
        match (value("code"), value("error")) {
            (Some(code), None) if !code.is_empty() && code.len() <= 255 => Some(Ok(code)),
            (None, Some(_)) => Some(Err(EnrollError::Denied)),
            _ => None,
        }
    }
}

/// The PKCE challenge for `verifier`: base64url, unpadded, of its SHA-256.
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}
