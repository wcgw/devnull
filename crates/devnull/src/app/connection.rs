//! Whether `dn` is connected to dev-null.sh, and the `connect` buffer explaining it until it is (D8).

use devnull_api::model::Me;

use crate::doc::{Document, Line, Span, Target};

/// What the network reports about the connection, sent back by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetEvent {
    /// The device key works and the account answered.
    Connected(Me),
    /// There is no device key yet.
    NoKey,
    /// The server does not know this device, or it was revoked.
    NotApproved,
    /// The key is passphrase-protected and no agent holds it; `wrong` after a failed attempt.
    NeedsPassphrase {
        /// Whether the last passphrase was wrong.
        wrong: bool,
    },
    /// Approval started: the page to open and the fingerprint it will show.
    Enrolling {
        /// The approval page.
        url: String,
        /// The device key's fingerprint.
        fingerprint: String,
    },
    /// Connecting failed, for the reason given.
    Failed(String),
    /// Something to report that does not change the connection, such as a pasted link that is not this attempt's.
    Error(String),
}

/// Where connecting stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Connection {
    Checking,
    NoKey,
    NotApproved,
    Failed(String),
    NeedsPassphrase {
        wrong: bool,
    },
    Enrolling {
        url: String,
        fingerprint: String,
    },
    /// Connected, but the terms must be accepted first.
    Terms(Me),
    Connected(Me),
}

impl Connection {
    /// The account's handle, once connected.
    pub(super) fn handle(&self) -> Option<&str> {
        match self {
            Self::Connected(me) => Some(&me.handle),
            _ => None,
        }
    }

    /// What the `connect` buffer says; `None` once connected.
    pub(super) fn document(&self) -> Option<Document> {
        let plain = |text: &str| Line::text(text);
        let lines = match self {
            Self::Connected(_) => return None,
            Self::Checking => vec![plain("Connecting to dev-null.sh…")],
            Self::NoKey => vec![
                plain("This device is not connected to dev-null.sh."),
                plain(""),
                plain("Type :connect to approve it in your browser."),
            ],
            Self::NotApproved => vec![
                plain("This device is not approved on dev-null.sh, or its approval was revoked."),
                plain(""),
                plain("Type :connect to approve it again."),
            ],
            Self::Failed(reason) => vec![
                plain(&format!("Cannot connect to dev-null.sh: {reason}.")),
                plain(""),
                plain("Type :connect to try again."),
            ],
            Self::NeedsPassphrase { wrong } => {
                let mut lines = vec![plain(
                    "The device key is encrypted. Type its passphrase below.",
                )];
                if *wrong {
                    lines.push(plain("Wrong passphrase, try again."));
                }
                lines.extend([plain(""), plain("Escape, then :connect, to start over.")]);
                lines
            }
            Self::Enrolling { url, fingerprint } => vec![
                plain("Approve this device in your browser:"),
                plain(""),
                Line::Text(vec![
                    Span::new(url.clone(), crate::hl::LINK).with_target(Target::Url(url.clone())),
                ]),
                plain(""),
                plain("Check that the page shows this fingerprint:"),
                Line::Text(vec![Span::new(fingerprint.clone(), crate::hl::HASH)]),
                plain(""),
                plain("Waiting for the browser to come back…"),
                plain("Over SSH, paste the link the browser ends on with :connect <link>."),
            ],
            Self::Terms(me) => vec![
                plain(&format!(
                    "dev-null.sh's terms changed (version {}).",
                    me.terms_version.as_deref().unwrap_or("unknown")
                )),
                plain("Read them at https://dev-null.sh/terms and https://dev-null.sh/privacy."),
                plain(""),
                plain("Type :agree to accept them."),
            ],
        };
        let mut doc = Document::new();
        doc.push_back("connect", lines);
        Some(doc)
    }
}
