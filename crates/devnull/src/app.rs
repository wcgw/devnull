//! The application core (D10): state and a pure `update`; the runtime does the I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::key::Key;

/// Everything `dn` knows, and the only place that decides what happens next.
#[derive(Debug)]
pub struct App {
    focused: bool,
}

/// Something that happened, for [`App::update`] to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// A key was pressed.
    Key(Key),
    /// Text was pasted (bracketed paste). It never runs commands.
    Paste(String),
    /// The terminal gained focus.
    FocusGained,
    /// The terminal lost focus.
    FocusLost,
}

/// Something [`App::update`] asks the runtime to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Restore the terminal and exit.
    Quit,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// A fresh session, assuming the terminal has focus until told otherwise.
    #[must_use]
    pub fn new() -> Self {
        Self { focused: true }
    }

    /// Whether the terminal has focus, which slows polling down when it doesn't.
    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Reacts to `msg`, returning what the runtime must do.
    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(key) if key == Key::char('q') => vec![Effect::Quit],
            Msg::Key(_) | Msg::Paste(_) => Vec::new(),
            Msg::FocusGained => {
                self.focused = true;
                Vec::new()
            }
            Msg::FocusLost => {
                self.focused = false;
                Vec::new()
            }
        }
    }

    /// Draws the whole screen.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let intro = [
            Line::from(format!("dn v{}", env!("CARGO_PKG_VERSION"))),
            Line::from("a Neovim-flavoured client for dev-null.sh"),
            Line::from(""),
            Line::from("type  q  to exit"),
        ];
        let height = u16::try_from(intro.len()).unwrap_or(u16::MAX);
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(frame.area());
        frame.render_widget(Paragraph::new(intro.to_vec()).centered(), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::key::Key;

    #[test]
    fn q_quits() {
        let mut app = App::new();

        assert_eq!(app.update(Msg::Key(Key::char('q'))), [Effect::Quit]);
    }

    #[test]
    fn a_paste_never_runs_commands() {
        let mut app = App::new();

        assert_eq!(app.update(Msg::Paste("q".into())), []);
    }

    #[test]
    fn the_app_tracks_terminal_focus() {
        let mut app = App::new();
        assert!(app.is_focused());

        assert_eq!(app.update(Msg::FocusLost), []);
        assert!(!app.is_focused());
        let _ = app.update(Msg::FocusGained);
        assert!(app.is_focused());
    }

    #[test]
    fn the_intro_screen_is_centered() {
        let app = App::new();
        let mut terminal = Terminal::new(TestBackend::new(60, 9)).expect("test backend");

        terminal.draw(|frame| app.render(frame)).expect("draw");

        insta::assert_snapshot!(terminal.backend());
    }
}
