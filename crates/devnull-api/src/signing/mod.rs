//! Device keys and SSHSIG request signing (D9).
//!
//! Every request carries an SSHSIG signature, made with the device's Ed25519 key, over a canonical message: the
//! timestamp, a nonce, the method, the request URI and the signed lines (see [`canonical_message`]).

#[cfg(test)]
mod tests;

use std::fmt;
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ssh_agent_client_rs::{Client as AgentClient, Identity};
use ssh_encoding::Encode;
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey, SshSig};

/// What a signature covers, as the server tells the message formats apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    /// JSON requests (`devnull-api`).
    Api,
    /// Multipart uploads, which also sign the files' hashes (`devnull-api-images`).
    Images,
}

impl Namespace {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "devnull-api",
            Self::Images => "devnull-api-images",
        }
    }
}

/// Why a device key cannot be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KeyError {
    /// There is no key file: the device has not been connected yet.
    #[error("no device key at {}", .0.display())]
    Missing(PathBuf),
    /// A key file is already there, so generating one would replace an enrolled device.
    #[error("a device key already exists at {}", .0.display())]
    Exists(PathBuf),
    /// The key file is passphrase-protected and no agent holds it: [`Signer::unlock`] it.
    #[error("the device key is encrypted")]
    Encrypted,
    /// The passphrase does not decrypt the key.
    #[error("wrong passphrase for the device key")]
    WrongPassphrase,
    /// Devices are Ed25519 keys.
    #[error("the device key is not an Ed25519 key")]
    NotEd25519,
    /// The file cannot be read or written as an OpenSSH key.
    #[error("cannot use the device key at {}", path.display())]
    Unusable {
        /// The key file.
        path: PathBuf,
        /// What went wrong.
        #[source]
        source: ssh_key::Error,
    },
}

/// Why a message could not be signed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SignError {
    /// Signing with the key file failed.
    #[error("cannot sign with the device key")]
    Key(#[source] ssh_key::Error),
    /// The agent that held the key at startup could not sign, for example because it was stopped or the key was
    /// removed from it.
    #[error("ssh-agent could not sign with the device key")]
    Agent(#[source] ssh_agent_client_rs::Error),
}

/// A device key able to sign requests: the key file itself, or the same key held by ssh-agent (D9).
pub struct Signer {
    public: PublicKey,
    how: How,
}

enum How {
    File(Box<PrivateKey>),
    /// The agent's socket.
    Agent(PathBuf),
}

impl fmt::Debug for Signer {
    // Hand-written so the private key never reaches a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer")
            .field("fingerprint", &self.fingerprint())
            .field("uses_agent", &self.uses_agent())
            .finish()
    }
}

impl Signer {
    /// The device key at `path`. When `agent` (an ssh-agent socket) holds the same key, signing goes through the
    /// agent and the private key is not loaded at all; otherwise the file must be unencrypted.
    ///
    /// # Errors
    ///
    /// [`KeyError::Missing`] when there is no file, [`KeyError::Encrypted`] when it needs a passphrase and no agent
    /// holds it, and the other [`KeyError`]s when it is not a usable Ed25519 key.
    pub fn open(path: &Path, agent: Option<&Path>) -> Result<Self, KeyError> {
        let key = read(path)?;
        let public = key.public_key().clone();
        if let Some(socket) = agent
            && agent_holds(socket, &public)
        {
            return Ok(Self {
                public,
                how: How::Agent(socket.to_owned()),
            });
        }
        if key.is_encrypted() {
            return Err(KeyError::Encrypted);
        }
        Ok(Self {
            public,
            how: How::File(Box::new(key)),
        })
    }

    /// The passphrase-protected device key at `path`, decrypted with `passphrase`.
    ///
    /// # Errors
    ///
    /// [`KeyError::WrongPassphrase`] when it does not decrypt, and the [`Signer::open`] errors.
    pub fn unlock(path: &Path, passphrase: &str) -> Result<Self, KeyError> {
        let key = read(path)?;
        let key = if key.is_encrypted() {
            key.decrypt(passphrase)
                .map_err(|_| KeyError::WrongPassphrase)?
        } else {
            key
        };
        Ok(Self {
            public: key.public_key().clone(),
            how: How::File(Box::new(key)),
        })
    }

    /// A new device key, written to `path` (mode 0600, in a directory created 0700 if needed), for enrollment.
    ///
    /// # Errors
    ///
    /// [`KeyError::Exists`] rather than replacing a key, and [`KeyError::Unusable`] when it cannot be written.
    pub fn generate(path: &Path) -> Result<Self, KeyError> {
        if path.exists() {
            return Err(KeyError::Exists(path.to_owned()));
        }
        let unusable = |source| KeyError::Unusable {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(|error| unusable(ssh_key::Error::Io(error.kind())))?;
        }
        let mut key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(unusable)?;
        key.set_comment("devnull device");
        // Written owner-only by `ssh-key`, as OpenSSH does.
        key.write_openssh_file(path, LineEnding::LF)
            .map_err(unusable)?;
        Ok(Self {
            public: key.public_key().clone(),
            how: How::File(Box::new(key)),
        })
    }

    /// The key's OpenSSH fingerprint, `SHA256:…`, as the approval page shows it.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        self.public.fingerprint(HashAlg::Sha256).to_string()
    }

    /// Whether signing goes through ssh-agent.
    #[must_use]
    pub fn uses_agent(&self) -> bool {
        matches!(self.how, How::Agent(_))
    }

    /// The `X-SSH-Signature` for `message`: the standard base64 of the raw SSHSIG blob.
    ///
    /// Blocks while ssh-agent signs, when it is the agent's key.
    ///
    /// # Errors
    ///
    /// When the key or the agent cannot sign.
    pub fn sign(&self, namespace: Namespace, message: &[u8]) -> Result<String, SignError> {
        let namespace = namespace.as_str();
        let signature = match &self.how {
            How::File(key) => key
                .sign(namespace, HashAlg::Sha512, message)
                .map_err(SignError::Key)?,
            How::Agent(socket) => {
                let data = SshSig::signed_data(namespace, HashAlg::Sha512, message)
                    .map_err(SignError::Key)?;
                let mut agent = AgentClient::connect(socket).map_err(SignError::Agent)?;
                let raw = agent
                    .sign(self.public.clone(), &data)
                    .map_err(SignError::Agent)?;
                SshSig::new(
                    self.public.key_data().clone(),
                    namespace,
                    HashAlg::Sha512,
                    raw,
                )
                .map_err(SignError::Key)?
            }
        };
        let mut blob = Vec::new();
        signature
            .encode(&mut blob)
            .map_err(|error| SignError::Key(ssh_key::Error::Encoding(error)))?;
        Ok(STANDARD.encode(blob))
    }
}

fn read(path: &Path) -> Result<PrivateKey, KeyError> {
    if !path.exists() {
        return Err(KeyError::Missing(path.to_owned()));
    }
    let key = PrivateKey::read_openssh_file(path).map_err(|source| KeyError::Unusable {
        path: path.to_owned(),
        source,
    })?;
    if key.algorithm() != Algorithm::Ed25519 {
        return Err(KeyError::NotEd25519);
    }
    Ok(key)
}

/// Whether the agent at `socket` holds `public`. An agent that cannot be reached holds nothing: signing then
/// falls back to the key file, as D9 says.
fn agent_holds(socket: &Path, public: &PublicKey) -> bool {
    let Ok(mut agent) = AgentClient::connect(socket) else {
        return false;
    };
    agent.list_all_identities().is_ok_and(|identities| {
        identities
            .iter()
            .any(|identity| matches!(identity, Identity::PublicKey(key) if key.key_data() == public.key_data()))
    })
}

/// The message a request is signed over: `timestamp` (Unix seconds), the lowercase hex `nonce`, `method`, the
/// request `uri` (path and query, exactly as sent), then `lines` (the body for JSON requests; the files' hashes and
/// the payload for uploads), joined by `\n` with no trailing newline.
pub(crate) fn canonical_message(
    timestamp: u64,
    nonce: &[u8; 16],
    method: &str,
    uri: &str,
    lines: &[&str],
) -> String {
    let nonce: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let head = [
        timestamp.to_string(),
        nonce,
        method.to_owned(),
        uri.to_owned(),
    ];
    head.iter()
        .map(String::as_str)
        .chain(lines.iter().copied())
        .collect::<Vec<_>>()
        .join("\n")
}
