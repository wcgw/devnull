//! The runtime's network side: the device key, the API client and device approval, each reporting back to the app
//! as [`Msg::Net`].

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use devnull_api::enroll::{EnrollError, Enrollment};
use devnull_api::model::{AccountKind, AccountPage};
use devnull_api::signing::{KeyError, Signer};
use devnull_api::{ApiError, Client, ORIGIN};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use devnull_api::media::Media;
use tokio::sync::Semaphore;

use crate::app::{FetchError, Fetched, Msg, NetEvent, Prepared, Request};
use crate::graphics::Picture;

/// What the device is called on dev-null.sh's device list.
const DEVICE_NAME: &str = "dn";

/// The connection to dev-null.sh, as far as the runtime holds it.
#[derive(Debug)]
pub(crate) struct Net {
    /// The device key file; `None` without a home directory to keep it in.
    key: Option<PathBuf>,
    /// ssh-agent's socket, when there is one.
    agent: Option<PathBuf>,
    /// The client, once connected. Shared with the tasks that connect; locked only briefly, never across `.await`.
    client: Arc<Mutex<Option<Client>>>,
    /// While approving: where pasted callback links go, and the task waiting for the browser.
    approval: Option<(mpsc::UnboundedSender<String>, JoinHandle<()>)>,
    /// Where results go.
    later: mpsc::UnboundedSender<Msg>,
    /// Downloads pictures.
    media: Option<Media>,
    /// Pictures downloading at once: two, as the Go client.
    picture_slots: Arc<Semaphore>,
}

impl Net {
    pub(crate) fn new(
        key: Option<PathBuf>,
        agent: Option<PathBuf>,
        later: mpsc::UnboundedSender<Msg>,
    ) -> Self {
        Self {
            key,
            agent,
            client: Arc::new(Mutex::new(None)),
            approval: None,
            later,
            media: Media::new(ORIGIN).ok(),
            picture_slots: Arc::new(Semaphore::new(2)),
        }
    }

    /// Connects with the device key, through ssh-agent when it holds the key.
    pub(crate) fn connect(&self) {
        let Some(key) = self.key_file() else { return };
        let (agent, slot, later) = (
            self.agent.clone(),
            Arc::clone(&self.client),
            self.later.clone(),
        );
        tokio::spawn(async move {
            match Signer::open(&key, agent.as_deref()) {
                Ok(signer) => finish(signer, &slot, &later).await,
                Err(KeyError::Missing(_)) => report(&later, NetEvent::NoKey),
                Err(KeyError::Encrypted) => {
                    report(&later, NetEvent::NeedsPassphrase { wrong: false })
                }
                Err(error) => report(&later, NetEvent::Failed(error.to_string())),
            }
        });
    }

    /// Decrypts the device key with `passphrase`, then connects.
    pub(crate) fn unlock(&self, passphrase: String) {
        let Some(key) = self.key_file() else { return };
        let (slot, later) = (Arc::clone(&self.client), self.later.clone());
        tokio::spawn(async move {
            // Key derivation is deliberately slow: keep it off the runtime's thread.
            let unlocked =
                tokio::task::spawn_blocking(move || Signer::unlock(&key, &passphrase)).await;
            match unlocked {
                Ok(Ok(signer)) => finish(signer, &slot, &later).await,
                Ok(Err(KeyError::WrongPassphrase)) => {
                    report(&later, NetEvent::NeedsPassphrase { wrong: true })
                }
                Ok(Err(error)) => report(&later, NetEvent::Failed(error.to_string())),
                Err(panic) => std::panic::resume_unwind(panic.into_panic()),
            }
        });
    }

    /// Approves this device in the browser, with the existing key or a new one; any earlier attempt is abandoned.
    pub(crate) fn enroll(&mut self) {
        if let Some((_, task)) = self.approval.take() {
            task.abort();
        }
        let Some(key) = self.key_file() else { return };
        let (paste, pasted) = mpsc::unbounded_channel();
        let (agent, slot, later) = (
            self.agent.clone(),
            Arc::clone(&self.client),
            self.later.clone(),
        );
        let task = tokio::spawn(approve(key, agent, slot, later, pasted));
        self.approval = Some((paste, task));
    }

    /// Finishes approval with the callback link the browser ended on.
    pub(crate) fn paste(&self, link: String) {
        match &self.approval {
            Some((paste, _)) if paste.send(link).is_ok() => {}
            _ => report(
                &self.later,
                NetEvent::Error("No approval in progress: :connect first".to_owned()),
            ),
        }
    }

    /// Accepts the terms of `version`.
    pub(crate) fn accept_terms(&self, version: String) {
        let Some(client) = self.client() else {
            return report(&self.later, NetEvent::Error("Not connected".to_owned()));
        };
        let later = self.later.clone();
        tokio::spawn(async move {
            match client.accept_terms(&version).await {
                Ok(me) => report(&later, NetEvent::Connected(me)),
                Err(error) => report(&later, NetEvent::Error(error.to_string())),
            }
        });
    }

    /// The device key file, or a report that there is nowhere to keep one.
    fn key_file(&self) -> Option<PathBuf> {
        if self.key.is_none() {
            report(
                &self.later,
                NetEvent::Failed("there is no home directory to keep the device key in".to_owned()),
            );
        }
        self.key.clone()
    }

    /// Asks the server for `request`, answering with [`Msg::Fetched`].
    pub(crate) fn fetch(&self, request: Request) {
        let Some(client) = self.client() else {
            return report(&self.later, NetEvent::Error("Not connected".to_owned()));
        };
        let later = self.later.clone();
        tokio::spawn(async move {
            let fetched = match request {
                Request::Log { log, after } => {
                    let result = client
                        .log(log, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Log { log, after, result }
                }
                Request::Thread { id } => {
                    let result = client
                        .thread(&id)
                        .await
                        .map(Box::new)
                        .map_err(|error| fetch_error(&error));
                    Fetched::Thread { id, result }
                }
                Request::Replies { id, after } => {
                    let result = client
                        .replies(&id, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Replies { id, after, result }
                }
                Request::Profile { handle, kind } => {
                    let result = match kind {
                        Some(AccountKind::User) => client
                            .profile(&handle)
                            .await
                            .map(|profile| AccountPage::Person(Box::new(profile))),
                        Some(AccountKind::Organization) => client
                            .organization(&handle)
                            .await
                            .map(|page| AccountPage::Organization(Box::new(page))),
                        _ => client.account(&handle).await,
                    }
                    .map_err(|error| fetch_error(&error));
                    Fetched::Profile { handle, result }
                }
                Request::Commits { handle, after } => {
                    let result = client
                        .commits_of(&handle, false, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Commits {
                        handle,
                        after,
                        result,
                    }
                }
                Request::OrganizationCommits { handle, tab, after } => {
                    let result = client
                        .organization_commits(&handle, tab, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::OrganizationCommits {
                        handle,
                        tab,
                        after,
                        result,
                    }
                }
                Request::Notifications { only, after } => {
                    let result = client
                        .notifications(&only, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Notifications {
                        only,
                        after,
                        result,
                    }
                }
                Request::Star { id, on } => {
                    let result = if on {
                        client.star(&id).await
                    } else {
                        client.unstar(&id).await
                    }
                    .map(Box::new)
                    .map_err(|error| fetch_error(&error));
                    Fetched::Star { id, on, result }
                }
                Request::Follow { handle, kind, on } => {
                    let result = follow(&client, &handle, kind, on)
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Follow { handle, on, result }
                }
                Request::Preview { body } => {
                    let result = client
                        .preview(&body)
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Preview { body, result }
                }
                Request::Publish { draft } => {
                    let result = client
                        .publish(&draft)
                        .await
                        .map(Box::new)
                        .map_err(|error| fetch_error(&error));
                    Fetched::Published { draft, result }
                }
                Request::DeleteCommit { id } => {
                    let result = client
                        .delete_commit(&id)
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::DeleteCommit { id, result }
                }
                Request::Starred { after } => {
                    let result = client
                        .starred(after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::Starred { after, result }
                }
                Request::UnreadCount => {
                    let result = client
                        .me()
                        .await
                        .map(|me| me.unread_notifications)
                        .map_err(|error| fetch_error(&error));
                    Fetched::UnreadCount { result }
                }
                Request::SearchProfiles { term } => {
                    let result = client
                        .search_profiles(&term)
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::SearchProfiles { term, result }
                }
                Request::People {
                    handle,
                    which,
                    after,
                } => {
                    let result = client
                        .people(&handle, which, after.as_ref())
                        .await
                        .map_err(|error| fetch_error(&error));
                    Fetched::People {
                        handle,
                        which,
                        after,
                        result,
                    }
                }
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs());
            // Fails only once the runtime's loop has ended, when nobody needs the answer.
            let _ = later.send(Msg::Fetched { fetched, now });
        });
    }

    /// Downloads the picture at `source` and prepares it for `cols` × `rows` cells (for the kitty protocol as image
    /// `kitty`, else as half blocks), answering with [`Msg::Picture`].
    pub(crate) fn picture(&self, source: String, cols: u16, rows: u16, kitty: Option<u32>) {
        let (media, slots, later) = (
            self.media.clone(),
            Arc::clone(&self.picture_slots),
            self.later.clone(),
        );
        tokio::spawn(async move {
            // Never closed: a permit always comes.
            let _slot = slots.acquire_owned().await;
            let result = match media {
                None => Err("pictures cannot be downloaded".to_owned()),
                Some(media) => match media.get(&source).await {
                    Ok(bytes) => {
                        // Decoding and scaling take a while: off the runtime's thread.
                        tokio::task::spawn_blocking(move || prepare(&bytes, cols, rows, kitty))
                            .await
                            .unwrap_or_else(|_| Err("cannot prepare it".to_owned()))
                    }
                    Err(error) => Err(error.to_string()),
                },
            };
            // Fails only once the runtime's loop has ended, when nobody needs the picture.
            let _ = later.send(Msg::Picture {
                source,
                cols,
                rows,
                result,
            });
        });
    }

    fn client(&self) -> Option<Client> {
        // Cloned out so the lock is never held across `.await`; a poisoned lock still holds a usable client.
        self.client
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Connects with `signer`: the account answers, or the reason it did not.
async fn finish(signer: Signer, slot: &Mutex<Option<Client>>, later: &mpsc::UnboundedSender<Msg>) {
    let client = match Client::new(ORIGIN, signer) {
        Ok(client) => client,
        Err(error) => return report(later, NetEvent::Failed(error.to_string())),
    };
    match client.me().await {
        Ok(me) => {
            *slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(client);
            report(later, NetEvent::Connected(me));
        }
        Err(error) => report(later, failure(&error)),
    }
}

/// The approval task: start, wait for the browser or a pasted link, redeem.
async fn approve(
    key: PathBuf,
    agent: Option<PathBuf>,
    slot: Arc<Mutex<Option<Client>>>,
    later: mpsc::UnboundedSender<Msg>,
    mut pasted: mpsc::UnboundedReceiver<String>,
) {
    let signer = match Signer::open(&key, agent.as_deref()) {
        Ok(signer) => signer,
        Err(KeyError::Missing(_)) => match Signer::generate(&key) {
            Ok(signer) => signer,
            Err(error) => return report(&later, NetEvent::Failed(error.to_string())),
        },
        Err(KeyError::Encrypted) => {
            let reason =
                "the device key is encrypted; add it to ssh-agent to approve it".to_owned();
            return report(&later, NetEvent::Failed(reason));
        }
        Err(error) => return report(&later, NetEvent::Failed(error.to_string())),
    };
    let enrollment = match Enrollment::start(ORIGIN, signer, DEVICE_NAME).await {
        Ok(enrollment) => enrollment,
        Err(error) => return report(&later, NetEvent::Failed(error.to_string())),
    };
    let url = enrollment.authorize_url().to_owned();
    report(
        &later,
        NetEvent::Enrolling {
            url,
            fingerprint: enrollment.fingerprint(),
        },
    );
    let code = loop {
        tokio::select! {
            code = enrollment.wait() => break code,
            Some(link) = pasted.recv() => match enrollment.code_from_url(&link) {
                Err(EnrollError::BadCallback) => report(&later, NetEvent::Error(EnrollError::BadCallback.to_string())),
                outcome => break outcome,
            },
        }
    };
    let outcome = match code {
        Ok(code) => enrollment.redeem(&code).await,
        Err(error) => Err(error),
    };
    match outcome {
        Ok((client, me)) => {
            *slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(client);
            report(&later, NetEvent::Connected(me));
        }
        Err(EnrollError::Api(error)) => report(&later, failure(&error)),
        Err(error) => report(&later, NetEvent::Failed(error.to_string())),
    }
}

/// Follows `handle` (`on`) or stops following it; of unknown kind, as a person, else as an organization.
async fn follow(
    client: &Client,
    handle: &str,
    kind: Option<AccountKind>,
    on: bool,
) -> Result<(), ApiError> {
    let set = |kind| async move {
        if on {
            client.follow(handle, kind).await
        } else {
            client.unfollow(handle, kind).await
        }
    };
    match kind {
        Some(kind) => set(kind).await,
        None => match set(AccountKind::User).await {
            Err(ApiError::NotFound) => set(AccountKind::Organization).await,
            other => other,
        },
    }
}

/// The picture in `bytes`, prepared for `cols` × `rows` cells: for the kitty protocol as image `kitty`, else as
/// half blocks.
fn prepare(bytes: &[u8], cols: u16, rows: u16, kitty: Option<u32>) -> Result<Prepared, String> {
    let picture = Picture::decode(bytes).ok_or_else(|| "not a picture dn shows".to_owned())?;
    Ok(match kitty {
        Some(id) => Prepared::Kitty(picture.kitty(id, cols, rows)),
        None => Prepared::HalfBlocks(picture.half_blocks(cols, rows)),
    })
}

fn fetch_error(error: &ApiError) -> FetchError {
    match error {
        ApiError::NotApproved => FetchError::NotApproved,
        other => FetchError::Failed(other.to_string()),
    }
}

fn failure(error: &ApiError) -> NetEvent {
    match error {
        ApiError::NotApproved => NetEvent::NotApproved,
        other => NetEvent::Failed(other.to_string()),
    }
}

fn report(later: &mpsc::UnboundedSender<Msg>, event: NetEvent) {
    // Fails only once the runtime's loop has ended, when nobody needs the news.
    let _ = later.send(Msg::Net(event));
}

/// Opens `url` in the browser, without waiting for it.
pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = std::process::Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Reaped on a thread of its own so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}
