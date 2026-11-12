//! Drawing the app: bufferline, windows with their statuslines, and the command line.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{App, AppMode, MessageKind};
use crate::doc::{Layout, SelectionKind};
use crate::hl::Group;

/// The width of the area showing pending keys, at the right of the command line, as Vim's `'showcmd'`.
const SHOWCMD_WIDTH: u16 = 10;

impl App {
    /// Draws the whole screen.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let (width, height) = self.size;
        let buffer = &self.buffers[self.window.buffer];
        let bufferline = Line::from(Span::styled(
            format!(" {} ", buffer.name),
            Group::TabLineSel.style(),
        ))
        .style(Group::TabLineFill.style());
        frame.render_widget(Paragraph::new(bufferline), Rect::new(0, 0, width, 1));

        let area = self.window_area();
        let viewport = self.viewport();
        let layout = Layout::new(&buffer.doc, area.width);
        let top = self.window.top(&layout, viewport);
        let rows: Vec<Line<'_>> = (top..top + usize::from(area.height))
            .map(|row| {
                if row < layout.row_count() {
                    Line::from(
                        layout
                            .segments(row)
                            .map(|(text, group)| Span::styled(text, group.style()))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    Line::from(Span::styled("~", Group::NonText.style()))
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(rows), area);
        if let AppMode::Visual(selection) = &self.mode {
            for (y, row) in (area.y..area.bottom()).zip(top..layout.row_count()) {
                if let Some(cells) = selection.cells(&self.window.cursor, &layout, row) {
                    let x = area.x.saturating_add(to_u16(cells.start));
                    let width = to_u16(cells.len()).min(area.right().saturating_sub(x));
                    frame
                        .buffer_mut()
                        .set_style(Rect::new(x, y, width, 1), Group::Visual.style());
                }
            }
        }

        let cursor = self.window.cursor.screen(&layout);
        let status = statusline(
            self.mode_name(),
            &buffer.name,
            &format!("{},{}", cursor.row + 1, cursor.col + 1),
            &position(top, viewport.height, layout.row_count()),
            width,
        );
        frame.render_widget(
            Paragraph::new(status),
            Rect::new(0, area.bottom(), width, 1),
        );

        let last = height.saturating_sub(1);
        let cmdline_area = Rect::new(0, last, width, 1);
        match &self.mode {
            AppMode::Cmdline(cmdline) => {
                let text = format!("{}{}", cmdline.prompt(), cmdline.text());
                frame.render_widget(Paragraph::new(text), cmdline_area);
                let x = 1 + cmdline.cursor_cells();
                frame.set_cursor_position(Position::new(
                    to_u16(x).min(width.saturating_sub(1)),
                    last,
                ));
                return;
            }
            AppMode::MorePrompt => {
                self.render_messages(frame);
                return;
            }
            AppMode::Normal | AppMode::Visual(_) => {}
        }
        if let Some(message) = &self.message {
            let group = match message.kind {
                MessageKind::Info => Group::Normal,
                MessageKind::Warning => Group::WarningMsg,
                MessageKind::Error => Group::ErrorMsg,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(message.text.as_str(), group.style())),
                cmdline_area,
            );
        }
        self.render_showcmd(frame, cmdline_area);

        let x = area.x.saturating_add(to_u16(cursor.col));
        let y = area.y.saturating_add(to_u16(cursor.row - top));
        frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), y));
    }

    /// The mode as the statusline names it.
    fn mode_name(&self) -> &'static str {
        match &self.mode {
            AppMode::Normal | AppMode::MorePrompt => "NORMAL",
            AppMode::Visual(selection) => match selection.kind() {
                SelectionKind::Charwise => "VISUAL",
                SelectionKind::Rowwise => "V-LINE",
            },
            AppMode::Cmdline(_) => "COMMAND",
        }
    }

    /// Keys typed toward a binding, with their count, as Vim's `'showcmd'` shows them.
    fn render_showcmd(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut pending: String = self
            .keymap
            .pending_count()
            .map(|count| count.to_string())
            .unwrap_or_default();
        pending.extend(self.keymap.pending_keys().iter().map(ToString::to_string));
        if pending.is_empty() || area.width <= SHOWCMD_WIDTH + 1 {
            return;
        }
        let shown: String = pending
            .chars()
            .rev()
            .take(usize::from(SHOWCMD_WIDTH))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let x = area.right() - SHOWCMD_WIDTH - 1;
        frame.render_widget(
            Paragraph::new(shown),
            Rect::new(x, area.y, SHOWCMD_WIDTH, 1),
        );
    }

    /// `:messages`: the history over the bottom of the screen, then Vim's prompt to continue.
    fn render_messages(&self, frame: &mut Frame<'_>) {
        let (width, height) = self.size;
        let shown = self
            .messages
            .len()
            .min(usize::from(height.saturating_sub(1)));
        let first = self.messages.len() - shown;
        let top = height.saturating_sub(1).saturating_sub(to_u16(shown));
        let area = Rect::new(0, top, width, height - top);
        frame.render_widget(Clear, area);
        let mut lines: Vec<Line<'_>> = self.messages[first..]
            .iter()
            .map(|message| {
                let group = if message.kind == MessageKind::Error {
                    Group::ErrorMsg
                } else {
                    Group::Normal
                };
                Line::from(Span::styled(message.text.as_str(), group.style()))
            })
            .collect();
        let prompt = "Press ENTER or type command to continue";
        lines.push(Line::from(Span::styled(prompt, Group::MoreMsg.style())));
        frame.render_widget(Paragraph::new(lines), area);
        let x = to_u16(prompt.width()).min(width.saturating_sub(1));
        frame.set_cursor_position(Position::new(x, height.saturating_sub(1)));
    }
}

/// A statusline: the mode and the buffer's name on the left, the ruler on the right.
fn statusline<'a>(
    mode: &'a str,
    name: &'a str,
    ruler: &str,
    position: &str,
    width: u16,
) -> Line<'a> {
    let left = format!(" {mode}  {name}");
    let right = format!("{ruler}  {position} ");
    let gap = usize::from(width)
        .saturating_sub(left.width() + right.width())
        .max(1);
    Line::from(format!("{left}{}{right}", " ".repeat(gap))).style(Group::StatusLine.style())
}

/// Where the view is in its buffer, as Vim's ruler says it: `All`, `Top`, `Bot`, or the share of rows above it.
fn position(top: usize, height: usize, rows: usize) -> String {
    let below = rows.saturating_sub(top + height);
    match (top, below) {
        (0, 0) => "All".to_owned(),
        (0, _) => "Top".to_owned(),
        (_, 0) => "Bot".to_owned(),
        (above, below) => format!("{}%", above * 100 / (above + below)),
    }
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}
