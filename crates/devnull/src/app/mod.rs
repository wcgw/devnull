//! `dn` in the kit (D10): what dev-null.sh shows and what `dn` does with it. The editor around it (buffers,
//! windows, keys, the command line, highlight groups, pictures) is wcgw-kit's; `Dn` is the application it runs.

mod actions;
mod commands;
mod composer;
mod connection;
mod feed;
mod fetch;
mod hover;
mod logs;
mod picker;
mod profiles;
mod threads;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use wcgw_kit::BufferId;

use crate::doc::{Cursor, Document, LinkCard, Target};

use connection::Connection;
pub use connection::NetEvent;
use feed::Source;
pub use fetch::{FetchError, Fetched, Request};

/// The kit `dn` runs in.
pub type Kit = wcgw_kit::Kit<Dn>;

/// What `dn`'s code returns for the runtime to do.
pub type Effects = wcgw_kit::Effects<Dn>;

/// One thing for the runtime to do.
pub type Effect = wcgw_kit::Effect<Msg, Io>;

/// What `dn`'s own work tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// News about the connection.
    Net(NetEvent),
    /// The answer to an [`Io::Fetch`], and when it arrived (Unix seconds), for ages.
    Fetched {
        /// The answer.
        fetched: Fetched,
        /// When it arrived.
        now: u64,
    },
    /// Time to check the logs for new commits, unless polling restarted since this was asked for.
    Poll {
        /// The polling it belongs to.
        generation: u64,
    },
}

/// What `dn` asks its network side to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Io {
    /// Connect with the device key, reporting back as [`Msg::Net`].
    Connect,
    /// Decrypt the device key with this passphrase, then connect.
    Unlock(String),
    /// Approve this device in the browser: use the device key, creating it if there is none.
    Enroll,
    /// Finish approval with the callback link the browser ended on.
    EnrollPasted(String),
    /// Accept the terms of this version.
    AcceptTerms(String),
    /// Ask the server, sending the answer back as [`Msg::Fetched`].
    Fetch(Request),
}

/// `request`, as an effect.
fn fetch(request: Request) -> Effect {
    Effect::App(Io::Fetch(request))
}

/// What `dn` knows that the editor around it does not.
#[derive(Debug)]
pub struct Dn {
    connection: Connection,
    /// The buffer saying where connecting stands, while there is one.
    status: Option<BufferId>,
    /// What each buffer of dev-null.sh's shows, and how far it loaded.
    feeds: HashMap<BufferId, feed::Feed>,
    /// Whether the logs are being polled.
    polling: bool,
    /// The generation of polls: those of an older one are stale.
    poll_generation: u64,
    /// Notifications unread, as the server last counted them.
    unread: u64,
    /// The draft being written, from opening the editor until it is published or kept.
    composing: Option<composer::Composing>,
    /// Drafts not published, to open again.
    drafts: composer::Drafts,
    /// The handle whose profile the hover card waits for, if any.
    hover_waiting: Option<String>,
}

impl Default for Dn {
    fn default() -> Self {
        Self::new()
    }
}

impl Dn {
    /// A fresh session, before connecting.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connection: Connection::Checking,
            status: None,
            feeds: HashMap::new(),
            polling: false,
            poll_generation: 0,
            unread: 0,
            composing: None,
            drafts: composer::Drafts::new(),
            hover_waiting: None,
        }
    }

    /// `dn` and the editor around it, on the `connect` buffer saying where connecting stands.
    #[must_use]
    pub fn session() -> (Self, Kit) {
        let doc = Connection::Checking.document().unwrap_or_default();
        let mut dn = Self::new();
        let kit = Self::kit("connect", doc, false);
        dn.status = kit.find_buffer("connect");
        (dn, kit)
    }

    /// The editor `dn` runs in, on one buffer called `name` showing `doc`, listed or not: with `dn`'s groups,
    /// keys, commands and statusline, and its bufferline always shown.
    #[must_use]
    pub fn kit(name: &str, doc: Document, listed: bool) -> Kit {
        let mut kit = Kit::new(name, doc, listed);
        *kit.theme_mut() = crate::hl::theme();
        *kit.keymap_mut() = crate::keymap::keymap();
        commands::add(&mut kit);
        kit.add_segment(wcgw_kit::shell::Segment::new(
            wcgw_kit::shell::Side::Right,
            0,
            |dn: &Dn, _| dn.account_segment(),
        ));
        let mut dn = Self::new();
        // dn has always shown its buffers at the top, and left the mouse to the terminal (its D1).
        let _ = kit.run_ex(&mut dn, "set showtabline=2 mouse=");
        kit
    }

    /// The buffer showing `source`, if there is one.
    fn feed_buffer(&self, source: &Source) -> Option<BufferId> {
        self.feeds
            .iter()
            .find(|(_, feed)| feed.source == *source)
            .map(|(&id, _)| id)
    }

    fn net(&mut self, kit: &mut Kit, event: NetEvent) -> Effects {
        let mut effects = Vec::new();
        self.connection = match event {
            NetEvent::Error(text) => {
                kit.error(text);
                return effects;
            }
            NetEvent::Connected(me) if !me.terms_accepted => Connection::Terms(me),
            NetEvent::Connected(me) => {
                self.unread = me.unread_notifications;
                Connection::Connected(me)
            }
            NetEvent::NoKey => Connection::NoKey,
            NetEvent::NotApproved => Connection::NotApproved,
            NetEvent::Failed(reason) => Connection::Failed(reason),
            NetEvent::NeedsPassphrase { wrong } => {
                kit.prompt("Passphrase: ", true, |_, _, passphrase| {
                    vec![Effect::App(Io::Unlock(passphrase))]
                });
                Connection::NeedsPassphrase { wrong }
            }
            NetEvent::Enrolling { url, fingerprint } => {
                effects.push(Effect::OpenUrl(url.clone()));
                Connection::Enrolling { url, fingerprint }
            }
        };
        effects.extend(self.show_connection(kit));
        effects
    }

    /// Shows the `connect` buffer while not connected, and the following log once connected, asking for it if it
    /// was never loaded.
    fn show_connection(&mut self, kit: &mut Kit) -> Effects {
        let (id, effects) = match self.connection.document() {
            Some(doc) => {
                let id = if let Some(id) = self.status {
                    id
                } else {
                    let id = kit.add_buffer("connect", Document::new(), false);
                    self.status = Some(id);
                    id
                };
                *kit.buffer_mut(id).doc_mut() = doc;
                (id, Vec::new())
            }
            None => (
                self.feed_buffer_or_new(kit, Source::Log(devnull_api::Log::Following)),
                self.start_polling(kit),
            ),
        };
        kit.show_buffer(id);
        effects
    }

    /// `:connect`, whose meaning depends on where connecting stands.
    fn connect(&mut self, kit: &mut Kit, link: Option<String>) -> Result<Effects, String> {
        let io = |io| Ok(vec![Effect::App(io)]);
        match (&self.connection, link) {
            (Connection::Enrolling { .. }, Some(link)) => io(Io::EnrollPasted(link)),
            (_, Some(_)) => Err("No approval in progress: :connect first".to_owned()),
            (Connection::NoKey | Connection::NotApproved | Connection::Enrolling { .. }, None) => {
                io(Io::Enroll)
            }
            (Connection::Connected(me), None) => {
                kit.info(format!("Connected as @{}", me.handle));
                Ok(Vec::new())
            }
            (_, None) => io(Io::Connect),
        }
    }

    /// `:agree`: accepts dev-null.sh's current terms.
    fn agree(&self) -> Result<Effects, String> {
        match &self.connection {
            Connection::Terms(me) => Ok(vec![Effect::App(Io::AcceptTerms(
                me.terms_version.clone().unwrap_or_default(),
            ))]),
            _ => Err("No terms to agree to".to_owned()),
        }
    }

    /// The web page of what the cursor is on: of the commit (a quote's, else the one holding the cursor), else of
    /// the account.
    fn web_page(kit: &Kit) -> Option<String> {
        let targets = kit.targets_under_cursor();
        let commit = targets.iter().flatten().find_map(|target| match target {
            Target::Commit { url: Some(url), .. } => Some(url.clone()),
            _ => None,
        });
        commit.or_else(|| {
            targets.iter().flatten().find_map(|target| match target {
                Target::Account(account) => {
                    Some(format!("{}/@{}", devnull_api::ORIGIN, account.handle))
                }
                _ => None,
            })
        })
    }

    /// The link under the cursor, a link card's too: what `:browse link` opens.
    fn link_under_cursor(kit: &Kit) -> Option<String> {
        match kit.target_under_cursor() {
            Some(Target::Url(url) | Target::LinkCard(LinkCard { url, .. })) => Some(url),
            _ => None,
        }
    }

    /// `:browse`: the commit or account under the cursor on the web; `:browse link`, the link under it.
    fn browse(kit: &Kit, link: bool) -> Result<Effects, String> {
        let url = if link {
            Self::link_under_cursor(kit).ok_or_else(|| "No link under the cursor".to_owned())?
        } else {
            Self::web_page(kit).ok_or_else(|| "No commit or account under the cursor".to_owned())?
        };
        Ok(vec![Effect::OpenUrl(url)])
    }

    /// `:yanklink`: the link under the cursor, else the one `:browse` opens, to the clipboard, as in every app.
    fn yank_link(kit: &mut Kit) -> Result<Effects, String> {
        let url = Self::link_under_cursor(kit)
            .or_else(|| Self::web_page(kit))
            .ok_or_else(|| "No link, commit or account under the cursor".to_owned())?;
        kit.info(format!("Yanked {url}"));
        Ok(vec![Effect::Copy(url)])
    }

    /// Notifications unread, as the server last counted them, and the account connected, for the statusline.
    fn account_segment(&self) -> Option<String> {
        let unread = (self.unread > 0).then(|| format!("󰂚 {}", self.unread));
        let handle = self.connection.handle().map(|handle| format!("@{handle}"));
        let parts: Vec<String> = unread.into_iter().chain(handle).collect();
        (!parts.is_empty()).then(|| parts.join("  "))
    }
}

impl wcgw_kit::App for Dn {
    type Target = Target;
    type Msg = Msg;
    type Effect = Io;

    fn update(&mut self, kit: &mut Kit, msg: Msg) -> Effects {
        match msg {
            Msg::Net(event) => self.net(kit, event),
            Msg::Fetched { fetched, now } => self.fetched(kit, fetched, now),
            Msg::Poll { generation } if generation == self.poll_generation => self.poll(kit),
            Msg::Poll { .. } => Vec::new(),
        }
    }

    fn start(&mut self, _kit: &mut Kit) -> Effects {
        vec![Effect::App(Io::Connect)]
    }

    /// What the text under the cursor refers to: a link opens in the browser, a commit's thread, a profile or
    /// another buffer in the window.
    fn open(&mut self, kit: &mut Kit, target: Target) -> Result<Effects, String> {
        match target {
            Target::Url(url) | Target::LinkCard(LinkCard { url, .. }) => {
                Ok(vec![Effect::OpenUrl(url)])
            }
            Target::Commit { id, .. } => Ok(self.open_thread(kit, &id)),
            Target::Account(account) => Ok(self.open_profile(kit, account)),
            Target::Buffer(name) => self.edit(kit, &name),
        }
    }

    /// `:e {name}`: the feed the name names (see [`Source::named`]), or with `profile`, the connected account's
    /// profile.
    fn edit(&mut self, kit: &mut Kit, name: &str) -> Result<Effects, String> {
        if let Some(id) = kit.find_buffer(name) {
            kit.show_buffer(id);
            return Ok(Vec::new());
        }
        let source = if name == "profile" {
            self.own_profile()
        } else {
            Source::named(name)
        };
        match source {
            Some(source) => Ok(self.open_feed(kit, source, Cursor::new())),
            None => Err(format!("E94: No matching buffer for {name}")),
        }
    }

    fn edit_names(&self, _kit: &Kit) -> Vec<String> {
        Source::names()
            .into_iter()
            .chain(["profile".to_owned()])
            .collect()
    }

    fn reload(&mut self, kit: &mut Kit, buffer: BufferId) -> Option<Effects> {
        self.refresh(kit, buffer)
    }

    fn shown(&mut self, kit: &mut Kit, buffer: BufferId) -> Effects {
        self.load(kit, buffer)
    }

    fn keyed(&mut self, kit: &mut Kit) -> Effects {
        self.load_more(kit)
    }

    fn edited(&mut self, kit: &mut Kit, saved: Result<String, String>) -> Effects {
        self.saved(kit, saved)
    }
}
