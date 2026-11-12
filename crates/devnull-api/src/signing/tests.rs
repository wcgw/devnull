//! Signing, checked against the signatures the original Go client produced (`tests/fixtures/signing`).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use super::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signing")
        .join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("fixture")
}

fn nonce(hex: &str) -> [u8; 16] {
    let bytes: Vec<u8> = (0..32)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect();
    bytes.try_into().expect("16 bytes")
}

#[test]
fn the_canonical_message_joins_the_request_as_the_go_client_does() {
    let get = canonical_message(
        1_762_944_000,
        &nonce("00112233445566778899aabbccddeeff"),
        "GET",
        "/api/v1/me",
        &[""],
    );
    let body = r#"{"body":"hello from dn ✓"}"#;
    let post = canonical_message(
        1_762_944_000,
        &nonce("ffeeddccbbaa99887766554433221100"),
        "POST",
        "/api/v1/commits",
        &[body],
    );

    assert_eq!(get, read("msg-get"));
    assert_eq!(post, read("msg-post"));
}

#[test]
fn a_key_file_signs_exactly_like_the_go_client() {
    let signer = Signer::open(&fixture("device_ed25519"), None).expect("key");

    for (message, namespace, expected) in [
        ("msg-get", Namespace::Api, "get.go.b64"),
        ("msg-post", Namespace::Api, "post.go.b64"),
        ("msg-upload", Namespace::Images, "upload.go.b64"),
    ] {
        let signature = signer
            .sign(namespace, read(message).as_bytes())
            .expect("signature");
        assert_eq!(signature, read(expected).trim(), "{message}");
    }
}

#[test]
fn a_missing_key_says_so() {
    let error = Signer::open(Path::new("/nonexistent/device_ed25519"), None).err();

    assert!(matches!(error, Some(KeyError::Missing(_))), "{error:?}");
}

#[test]
fn an_encrypted_key_needs_its_passphrase() {
    let path = fixture("enc_ed25519");

    assert!(matches!(
        Signer::open(&path, None),
        Err(KeyError::Encrypted)
    ));
    assert!(matches!(
        Signer::unlock(&path, "wrong"),
        Err(KeyError::WrongPassphrase)
    ));
    let signer = Signer::unlock(&path, "correct horse").expect("passphrase");
    assert_eq!(
        signer
            .sign(Namespace::Api, read("msg-get").as_bytes())
            .expect("signature"),
        read("get.go.b64").trim()
    );
}

#[test]
fn a_generated_key_is_private_to_its_owner_and_reopens_as_the_same_key() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("device_ed25519");

    let generated = Signer::generate(&path).expect("generate");

    assert_eq!(
        std::fs::metadata(&path).expect("file").permissions().mode() & 0o777,
        0o600
    );
    assert!(generated.fingerprint().starts_with("SHA256:"));
    assert_eq!(
        Signer::open(&path, None).expect("reopen").fingerprint(),
        generated.fingerprint()
    );
    assert!(matches!(Signer::generate(&path), Err(KeyError::Exists(_))));
}

/// A private `ssh-agent` on its own socket, killed on drop. Never the user's agent: the socket is passed explicitly,
/// and the environment is not read.
struct PrivateAgent {
    child: Child,
    socket: PathBuf,
    dir: tempfile::TempDir,
}

impl PrivateAgent {
    /// `None` when `ssh-agent` is not installed.
    fn start() -> Option<Self> {
        let dir = tempfile::tempdir().ok()?;
        let socket = dir.path().join("agent.sock");
        let child = Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .spawn()
            .ok()?;
        for _ in 0..50 {
            if socket.exists() {
                return Some(Self { child, socket, dir });
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        None
    }

    /// Adds the private key `key` to the agent. `ssh-add` refuses a key others may read, and a checkout leaves the
    /// fixtures readable (git keeps no mode bits but the executable one), so the agent gets a copy only the user may
    /// read.
    fn add(&self, key: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let copy = self.dir.path().join("key");
        std::fs::copy(key, &copy).expect("copy of the key");
        std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o600))
            .expect("the copy made private");
        let status = Command::new("ssh-add")
            .arg("-q")
            .arg(&copy)
            .env("SSH_AUTH_SOCK", &self.socket)
            .status()
            .expect("ssh-add");
        assert!(status.success());
    }
}

impl Drop for PrivateAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn ssh_agent_signs_when_it_holds_the_key_even_an_encrypted_one() {
    let Some(agent) = PrivateAgent::start() else {
        eprintln!("skipped: no ssh-agent");
        return;
    };
    let path = fixture("enc_ed25519");
    assert!(matches!(
        Signer::open(&path, Some(&agent.socket)),
        Err(KeyError::Encrypted)
    ));
    agent.add(&fixture("device_ed25519"));

    let signer = Signer::open(&path, Some(&agent.socket)).expect("agent holds the key");

    assert!(signer.uses_agent());
    assert_eq!(
        signer
            .sign(Namespace::Api, read("msg-get").as_bytes())
            .expect("signature"),
        read("get.go.b64").trim()
    );
}
