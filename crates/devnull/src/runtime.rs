//! The I/O shell around [`App`]: owns the terminal, turns terminal events into [`Msg`]s and carries out
//! [`Effect`]s.

use std::io::{self, Stdout, stdout};
use std::ops::ControlFlow;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::clipboard::CopyToClipboard;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    EventStream, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::{App, Effect, Msg, NetEvent};
use crate::key::Key;
use crate::net::{Net, open_url};

/// Runs `dn` until it quits, restoring the terminal however it ends.
///
/// # Errors
///
/// If the terminal cannot be set up, read from or drawn to.
pub async fn run() -> io::Result<()> {
    let mut terminal = Session::start()?;
    let mut app = App::new();
    let size = terminal.size()?;
    let _ = app.update(Msg::Resize {
        width: size.width,
        height: size.height,
    });
    let mut events = EventStream::new();
    // Messages that effects send back later, such as wake-ups and files read.
    let (later, mut arrived) = mpsc::unbounded_channel();
    let env = |name| std::env::var_os(name).map(PathBuf::from);
    let config = config_dir(env("XDG_CONFIG_HOME"), env("HOME"));
    let mut net = Net::new(
        config.as_ref().map(|dir| dir.join("device_ed25519")),
        env("SSH_AUTH_SOCK"),
        later.clone(),
    );
    if let Some(init) = config.map(|dir| dir.join("init.vim")) {
        match tokio::fs::read_to_string(&init).await {
            Ok(contents) => {
                let path = shown_path(&init, env("HOME"));
                let _ = later.send(Msg::Sourced {
                    path,
                    contents: Ok(contents),
                });
            }
            // No init.vim is the usual case, not an error.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                let path = shown_path(&init, env("HOME"));
                let _ = later.send(Msg::Sourced {
                    path,
                    contents: Err(error.to_string()),
                });
            }
        }
    }
    for effect in app.start() {
        if perform(effect, &later, &mut net)?.is_break() {
            return Ok(());
        }
    }
    loop {
        terminal.draw(|frame| app.render(frame))?;
        // Both branches are cancel-safe: an event or message not taken now stays queued.
        let msg = tokio::select! {
            event = events.next() => {
                let Some(event) = event else {
                    return Ok(());
                };
                match to_msg(event?) {
                    Some(msg) => msg,
                    None => continue,
                }
            }
            Some(msg) = arrived.recv() => msg,
        };
        for effect in app.update(msg) {
            if perform(effect, &later, &mut net)?.is_break() {
                return Ok(());
            }
        }
    }
}

/// Carries out one effect; `Break` means quit. Effects that finish later send their result through `later`.
fn perform(
    effect: Effect,
    later: &mpsc::UnboundedSender<Msg>,
    net: &mut Net,
) -> io::Result<ControlFlow<()>> {
    match effect {
        Effect::Quit => return Ok(ControlFlow::Break(())),
        Effect::Copy(text) => execute!(stdout(), CopyToClipboard::to_clipboard_from(text))?,
        // Sending below only fails once the loop has ended, when nobody needs the result any more.
        Effect::Wake { after, token } => {
            let later = later.clone();
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                let _ = later.send(Msg::Wake(token));
            });
        }
        Effect::Connect => net.connect(),
        Effect::Unlock(passphrase) => net.unlock(passphrase),
        Effect::Enroll => net.enroll(),
        Effect::EnrollPasted(link) => net.paste(link),
        Effect::AcceptTerms(version) => net.accept_terms(version),
        Effect::OpenUrl(url) => {
            if let Err(error) = open_url(&url) {
                let _ = later.send(Msg::Net(NetEvent::Error(format!(
                    "Cannot open the browser: {error}"
                ))));
            }
        }
        Effect::Source(path) => {
            let later = later.clone();
            let file = expand_home(&path, std::env::var_os("HOME").map(PathBuf::from));
            tokio::spawn(async move {
                let contents = tokio::fs::read_to_string(file)
                    .await
                    .map_err(|error| error.to_string());
                let _ = later.send(Msg::Sourced { path, contents });
            });
        }
    }
    Ok(ControlFlow::Continue(()))
}

/// `path` with a leading `~` standing for `home`, when there is one.
fn expand_home(path: &str, home: Option<PathBuf>) -> PathBuf {
    match (path.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

/// `path` as messages show it: under `home`, as `~/…`.
fn shown_path(path: &Path, home: Option<PathBuf>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `dn`'s configuration directory (D6), holding `init.vim` and the device key: `devnull` under
/// `$XDG_CONFIG_HOME`, else under `~/.config`.
fn config_dir(config_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let config = config_home
        .filter(|dir| dir.is_absolute())
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(config.join("devnull"))
}

/// The message for a terminal event, if `App` cares.
fn to_msg(event: Event) -> Option<Msg> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            Key::try_from(key).ok().map(Msg::Key)
        }
        Event::Paste(text) => Some(Msg::Paste(text)),
        Event::FocusGained => Some(Msg::FocusGained),
        Event::FocusLost => Some(Msg::FocusLost),
        Event::Resize(width, height) => Some(Msg::Resize { width, height }),
        Event::Key(_) | Event::Mouse(_) => None,
    }
}

/// Whether the kitty keyboard protocol was turned on, so that it is turned off again exactly once, also from the
/// panic hook, which cannot reach the [`Session`].
static KEYBOARD_ENHANCED: AtomicBool = AtomicBool::new(false);

/// The terminal in `dn`'s modes: raw, alternate screen, focus reporting, bracketed paste and, where supported, the
/// kitty keyboard protocol (so `<C-i>` and `<Tab>` differ). Dropping it, or panicking, restores the terminal.
struct Session {
    terminal: DefaultTerminal,
}

impl Session {
    fn start() -> io::Result<Self> {
        // Raw mode, the alternate screen, and a panic hook that leaves both.
        let terminal = ratatui::try_init()?;
        let session = Self { terminal };
        execute!(stdout(), EnableFocusChange, EnableBracketedPaste)?;
        if supports_keyboard_enhancement().unwrap_or(false) {
            execute!(
                stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
            KEYBOARD_ENHANCED.store(true, Ordering::SeqCst);
        }
        // Runs before ratatui's hook, which this one wraps.
        let ratatui_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            leave_modes();
            ratatui_hook(info);
        }));
        Ok(session)
    }
}

/// Turns off the modes `Session::start` added on top of ratatui's. Errors are ignored: this runs while exiting or
/// panicking, when there is nothing better to do.
fn leave_modes() {
    let mut out = stdout();
    if KEYBOARD_ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, DisableBracketedPaste, DisableFocusChange);
}

impl Drop for Session {
    fn drop(&mut self) {
        leave_modes();
        ratatui::restore();
    }
}

impl Deref for Session {
    type Target = ratatui::Terminal<ratatui::backend::CrosstermBackend<Stdout>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::*;
    use crate::key::Key;

    fn key(kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn a_leading_tilde_is_the_home_directory() {
        let home = Some(PathBuf::from("/home/u"));

        assert_eq!(
            expand_home("~/x.vim", home.clone()),
            PathBuf::from("/home/u/x.vim")
        );
        assert_eq!(expand_home("~", home.clone()), PathBuf::from("/home/u"));
        assert_eq!(
            expand_home("~bob/x.vim", home.clone()),
            PathBuf::from("~bob/x.vim")
        );
        assert_eq!(expand_home("x.vim", home), PathBuf::from("x.vim"));
        assert_eq!(expand_home("~/x.vim", None), PathBuf::from("~/x.vim"));
    }

    #[test]
    fn paths_under_home_are_shown_with_a_tilde() {
        let home = Some(PathBuf::from("/home/u"));

        assert_eq!(
            shown_path(Path::new("/home/u/.config/devnull/init.vim"), home.clone()),
            "~/.config/devnull/init.vim"
        );
        assert_eq!(
            shown_path(Path::new("/etc/init.vim"), home),
            "/etc/init.vim"
        );
    }

    #[test]
    fn the_config_dir_is_under_the_xdg_config_home_or_dot_config() {
        let home = Some(PathBuf::from("/home/u"));

        assert_eq!(
            config_dir(Some("/xdg".into()), home.clone()),
            Some(PathBuf::from("/xdg/devnull"))
        );
        assert_eq!(
            config_dir(None, home.clone()),
            Some(PathBuf::from("/home/u/.config/devnull"))
        );
        assert_eq!(
            config_dir(Some("relative".into()), home),
            Some(PathBuf::from("/home/u/.config/devnull"))
        );
        assert_eq!(config_dir(None, None), None);
    }

    #[test]
    fn presses_and_repeats_are_keys_but_releases_are_dropped() {
        assert_eq!(
            to_msg(key(KeyEventKind::Press)),
            Some(Msg::Key(Key::char('j')))
        );
        assert_eq!(
            to_msg(key(KeyEventKind::Repeat)),
            Some(Msg::Key(Key::char('j')))
        );
        assert_eq!(to_msg(key(KeyEventKind::Release)), None);
    }

    #[test]
    fn focus_paste_and_resize_become_messages() {
        assert_eq!(to_msg(Event::FocusLost), Some(Msg::FocusLost));
        assert_eq!(to_msg(Event::FocusGained), Some(Msg::FocusGained));
        assert_eq!(
            to_msg(Event::Paste("hi".into())),
            Some(Msg::Paste("hi".into()))
        );
        assert_eq!(
            to_msg(Event::Resize(80, 24)),
            Some(Msg::Resize {
                width: 80,
                height: 24
            })
        );
    }
}
