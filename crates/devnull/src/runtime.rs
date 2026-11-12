//! The I/O shell around [`App`]: owns the terminal, turns terminal events into [`Msg`]s and carries out
//! [`Effect`]s.

use std::io::{self, Stdout, stdout};

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        if let Step::Quit = perform(effect, &later, &mut net)? {
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
            match perform(effect, &later, &mut net)? {
                Step::Go => {}
                Step::Quit => return Ok(()),
                Step::Edit(text) => {
                    // The editor reads the keys until it exits: stop reading them meanwhile.
                    drop(events);
                    let saved = terminal.edit(&text)?;
                    events = EventStream::new();
                    // Fails only once the loop has ended.
                    let _ = later.send(Msg::Edited(saved));
                }
            }
        }
    }
}

/// What the loop does after an effect.
enum Step {
    Go,
    Quit,
    /// Hand the terminal to the editor on this text.
    Edit(String),
}

/// Carries out one effect, but for the editor, which needs the terminal. Effects that finish later send their result
/// through `later`.
fn perform(effect: Effect, later: &mpsc::UnboundedSender<Msg>, net: &mut Net) -> io::Result<Step> {
    match effect {
        Effect::Quit => return Ok(Step::Quit),
        Effect::Edit { text } => return Ok(Step::Edit(text)),
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
        Effect::Fetch(request) => net.fetch(request),
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
    Ok(Step::Go)
}

/// Counts drafts opened, so each gets a file of its own.
static EDITS: AtomicUsize = AtomicUsize::new(0);

/// The user's editor (D3): `$EDITOR`, else `$VISUAL`, else `nvim` when installed, else `vi`.
fn editor() -> String {
    let set = |name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    set("EDITOR").or_else(|| set("VISUAL")).unwrap_or_else(|| {
        let installed = std::env::var_os("PATH")
            .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("nvim").is_file()));
        if installed { "nvim" } else { "vi" }.to_owned()
    })
}

/// Where drafts are written (D3): `$XDG_RUNTIME_DIR` (memory only, the user's alone), else the temporary directory.
fn drafts_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// Runs `editor` (a shell command, as `$EDITOR` may hold arguments) on a file of `dir` holding `text`, readable by
/// the user alone and removed after; what it saved, or why there is nothing: it could not run, or exited with an
/// error (`:cq`).
fn run_editor(editor: &str, dir: &Path, text: &str) -> Result<String, String> {
    let path = dir.join(format!(
        "dn-draft-{}-{}.md",
        std::process::id(),
        EDITS.fetch_add(1, Ordering::SeqCst)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&path)
        .and_then(|mut file| io::Write::write_all(&mut file, text.as_bytes()));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write the draft: {error}"));
    }
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("dn")
        .arg(&path)
        .status();
    let saved = std::fs::read_to_string(&path);
    // The draft stays in the app's memory, not on disk.
    let _ = std::fs::remove_file(&path);
    match status {
        Err(error) => Err(format!("cannot run {editor}: {error}")),
        Ok(status) if !status.success() => Err(match status.code() {
            Some(code) => format!("the editor exited with {code}"),
            None => "the editor was stopped".to_owned(),
        }),
        Ok(_) => saved.map_err(|error| format!("cannot read the draft: {error}")),
    }
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
    /// Whether the terminal speaks the kitty keyboard protocol, asked once: asking takes a round trip.
    keyboard: bool,
}

impl Session {
    fn start() -> io::Result<Self> {
        // Raw mode, the alternate screen, and a panic hook that leaves both.
        let terminal = ratatui::try_init()?;
        let session = Self {
            terminal,
            keyboard: supports_keyboard_enhancement().unwrap_or(false),
        };
        enter_modes(session.keyboard)?;
        // Runs before ratatui's hook, which this one wraps.
        let ratatui_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            leave_modes();
            ratatui_hook(info);
        }));
        Ok(session)
    }
}

impl Session {
    /// Hands the terminal to the user's editor on `text` until it exits, then takes it back; what it saved, or why
    /// there is nothing.
    fn edit(&mut self, text: &str) -> io::Result<Result<String, String>> {
        leave_modes();
        ratatui::restore();
        let saved = run_editor(&editor(), &drafts_dir(), text);
        crossterm::terminal::enable_raw_mode()?;
        execute!(
            stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
        enter_modes(self.keyboard)?;
        // A blank screen, and a terminal that knows it, so the next draw paints it all. Not `Terminal::clear`: it
        // asks the terminal where the cursor is, and gives up with an error when the answer is slow.
        self.terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;
        Ok(saved)
    }
}

/// Turns on the modes `dn` adds on top of ratatui's: focus reporting, bracketed paste and, when the terminal speaks
/// it (`keyboard`), the kitty keyboard protocol.
fn enter_modes(keyboard: bool) -> io::Result<()> {
    execute!(stdout(), EnableFocusChange, EnableBracketedPaste)?;
    if keyboard {
        execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        KEYBOARD_ENHANCED.store(true, Ordering::SeqCst);
    }
    Ok(())
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

#[cfg(test)]
mod editor_tests {
    use super::*;

    fn files(dir: &Path) -> usize {
        std::fs::read_dir(dir).expect("dir").count()
    }

    #[test]
    fn what_the_editor_saved_comes_back_and_the_draft_file_goes() {
        let dir = tempfile::tempdir().expect("dir");

        let saved = run_editor("printf 'saved' >", dir.path(), "draft");

        assert_eq!(saved, Ok("saved".to_owned()));
        assert_eq!(files(dir.path()), 0);
    }

    #[test]
    fn an_editor_quitting_with_an_error_gives_nothing() {
        let dir = tempfile::tempdir().expect("dir");

        let saved = run_editor("false", dir.path(), "draft");

        assert_eq!(saved, Err("the editor exited with 1".to_owned()));
        assert_eq!(files(dir.path()), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_draft_is_the_user_s_alone_and_editors_may_take_arguments() {
        let dir = tempfile::tempdir().expect("dir");

        let mode = run_editor(
            "f() { stat -c %a \"$1\" > \"$1\"; }; f",
            dir.path(),
            "draft",
        );
        let edited = run_editor("sed -i s/draft/done/", dir.path(), "a draft");

        assert_eq!(mode, Ok("600\n".to_owned()));
        assert_eq!(edited, Ok("a done".to_owned()));
    }
}
