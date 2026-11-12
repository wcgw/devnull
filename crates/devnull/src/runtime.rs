//! The I/O shell around [`App`]: owns the terminal, turns terminal events into [`Msg`]s and carries out
//! [`Effect`]s.

use std::io::{self, Stdout, stdout};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    EventStream, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use futures_util::StreamExt;
use ratatui::DefaultTerminal;

use crate::app::{App, Effect, Msg};
use crate::key::Key;

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
    loop {
        terminal.draw(|frame| app.render(frame))?;
        let Some(event) = events.next().await else {
            return Ok(());
        };
        let Some(msg) = to_msg(event?) else {
            continue;
        };
        // `Quit` is the only effect so far; requests, timers and the rest are dispatched here as they arrive.
        if app.update(msg).contains(&Effect::Quit) {
            return Ok(());
        }
    }
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
