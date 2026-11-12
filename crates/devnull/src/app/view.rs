//! Drawing the app: bufferline, windows with their statuslines, and the command line.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::window::{Viewport, Window};
use super::{App, AppMode, MessageKind, text_area};
use crate::doc::{Layout, SelectionKind};
use crate::hl::Group;

/// The width of the area showing pending keys, at the right of the command line, as Vim's `'showcmd'`.
const SHOWCMD_WIDTH: u16 = 10;

impl App {
    /// Draws the whole screen.
    pub fn render(&self, frame: &mut Frame<'_>) {
        let (width, height) = self.size;
        let screen = frame.area();
        frame
            .buffer_mut()
            .set_style(screen, self.theme.style(Group::Normal));
        let current = self.windows.current();
        let bufferline = Line::from(Span::styled(
            format!(" {} ", self.buffers[current.buffer].name),
            self.theme.style(Group::TabLineSel),
        ))
        .style(self.theme.style(Group::TabLineFill));
        frame.render_widget(Paragraph::new(bufferline), Rect::new(0, 0, width, 1));

        let arrangement = self.windows.arrange(self.windows_area());
        for separator in &arrangement.separators {
            let bar: Vec<Line<'_>> = (0..separator.height).map(|_| Line::from("│")).collect();
            frame.render_widget(
                Paragraph::new(bar).style(self.theme.style(Group::WinSeparator)),
                *separator,
            );
        }
        let mut cursor = None;
        for &(id, rect) in &arrangement.windows {
            let is_current = id == self.windows.current_id();
            let shown = self.render_window(frame, self.windows.get(id), rect, is_current);
            if is_current {
                cursor = Some(shown);
            }
        }

        if self.which_key {
            self.render_which_key(frame);
        }

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
            AppMode::MorePrompt { from } => {
                self.render_messages(frame, *from);
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
                Paragraph::new(Span::styled(message.text.as_str(), self.theme.style(group))),
                cmdline_area,
            );
        }
        self.render_showcmd(frame, cmdline_area);
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
    }

    /// Draws `window` in `rect`, its statusline on the last row, and returns where its cursor is.
    fn render_window(
        &self,
        frame: &mut Frame<'_>,
        window: &Window,
        rect: Rect,
        is_current: bool,
    ) -> Position {
        let area = text_area(rect);
        let viewport = Viewport {
            height: usize::from(area.height),
            scrolloff: self.options.scrolloff,
        };
        let buffer = &self.buffers[window.buffer];
        let layout = Layout::new(&buffer.doc, area.width);
        let top = window.top(&layout, viewport);
        let rows: Vec<Line<'_>> = (top..top + usize::from(area.height))
            .map(|row| {
                if row < layout.row_count() {
                    Line::from(
                        layout
                            .segments(row)
                            .map(|(text, group)| Span::styled(text, self.theme.style(group)))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    Line::from(Span::styled("~", self.theme.style(Group::EndOfBuffer)))
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(rows), area);
        for (y, row) in (area.y..area.bottom()).zip(top..layout.row_count()) {
            if layout.is_folded(row) {
                frame.buffer_mut().set_style(
                    Rect::new(area.x, y, area.width, 1),
                    self.theme.style(Group::Folded),
                );
            }
        }
        if let (true, AppMode::Visual(selection)) = (is_current, &self.mode) {
            for (y, row) in (area.y..area.bottom()).zip(top..layout.row_count()) {
                if let Some(cells) = selection.cells(&window.cursor, &layout, row) {
                    let x = area.x.saturating_add(to_u16(cells.start));
                    let width = to_u16(cells.len()).min(area.right().saturating_sub(x));
                    frame
                        .buffer_mut()
                        .set_style(Rect::new(x, y, width, 1), self.theme.style(Group::Visual));
                }
            }
        }

        let cursor = window.cursor.screen(&layout);
        let (mode, mode_group) = self.mode_label();
        let status = statusline(
            is_current.then_some(mode),
            &buffer.name,
            &format!("{},{}", cursor.row + 1, cursor.col + 1),
            &position(top, viewport.height, layout.row_count()),
            rect.width,
        );
        let group = if is_current {
            Group::StatusLine
        } else {
            Group::StatusLineNC
        };
        let status_area = Rect::new(rect.x, area.bottom(), rect.width, 1);
        frame.render_widget(
            Paragraph::new(status).style(self.theme.style(group)),
            status_area,
        );
        if is_current {
            // " NORMAL ": the mode and the space on either side of it.
            let label = Rect {
                width: to_u16(mode.width() + 2).min(rect.width),
                ..status_area
            };
            frame
                .buffer_mut()
                .set_style(label, self.theme.style(mode_group));
        }

        let x = area.x.saturating_add(to_u16(cursor.col));
        let y = area
            .y
            .saturating_add(to_u16(cursor.row.saturating_sub(top)));
        Position::new(x.min(area.right().saturating_sub(1)), y)
    }

    /// The mode as the statusline names and colours it.
    fn mode_label(&self) -> (&'static str, Group) {
        match &self.mode {
            AppMode::Normal | AppMode::MorePrompt { .. } => ("NORMAL", Group::StatusLineModeNormal),
            AppMode::Visual(selection) => match selection.kind() {
                SelectionKind::Charwise => ("VISUAL", Group::StatusLineModeVisual),
                SelectionKind::Rowwise => ("V-LINE", Group::StatusLineModeVisual),
            },
            AppMode::Cmdline(_) => ("COMMAND", Group::StatusLineModeCommand),
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

    /// The which-key popup: what may follow the pending keys, in columns, just above the command line.
    fn render_which_key(&self, frame: &mut Frame<'_>) {
        let (width, height) = self.size;
        let entries: Vec<(String, String)> = self
            .keymap
            .continuations(self.keymap_mode())
            .into_iter()
            .map(|c| (c.keys, c.description))
            .collect();
        if entries.is_empty() || width < 8 || height < 5 {
            return;
        }
        let entry_width = entries
            .iter()
            .map(|(k, d)| k.width() + d.width() + 3)
            .max()
            .unwrap_or(1)
            + 3;
        let inner_width = usize::from(width - 2);
        let columns = (inner_width / entry_width).max(1);
        let rows = entries.len().div_ceil(columns).min(usize::from(height) / 2);
        let popup_height = to_u16(rows) + 2;
        let area = Rect::new(0, height - 1 - popup_height, width, popup_height);
        let title: String = self
            .keymap
            .pending_keys()
            .iter()
            .map(ToString::to_string)
            .collect();
        let block = Block::bordered()
            .title(format!(" {title} "))
            .border_style(self.theme.style(Group::FloatBorder))
            .style(self.theme.style(Group::NormalFloat));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        for (i, (key, description)) in entries.iter().take(rows * columns).enumerate() {
            let (row, column) = (i / columns, i % columns);
            let group = if description == "+prefix" {
                Group::WhichKeyGroup
            } else {
                Group::WhichKeyDesc
            };
            let line = Line::from(vec![
                Span::styled(key.as_str(), self.theme.style(Group::WhichKey)),
                Span::styled(" → ", self.theme.style(Group::WhichKeySeparator)),
                Span::styled(description.as_str(), self.theme.style(group)),
            ]);
            let x = inner.x + to_u16(column * entry_width);
            let cell = Rect::new(
                x,
                inner.y + to_u16(row),
                to_u16(entry_width).min(inner.right() - x),
                1,
            );
            frame.render_widget(Paragraph::new(line), cell);
        }
    }

    /// Messages from index `from` of the history over the bottom of the screen, as many as fit, then Vim's prompt
    /// to continue.
    fn render_messages(&self, frame: &mut Frame<'_>, from: usize) {
        let (width, height) = self.size;
        // Long messages wrap onto further rows, as in Vim; the newest that fit are shown.
        let mut rows: Vec<Line<'_>> = self.messages[from.min(self.messages.len())..]
            .iter()
            .flat_map(|message| {
                let group = if message.kind == MessageKind::Error {
                    Group::ErrorMsg
                } else {
                    Group::Normal
                };
                let style = self.theme.style(group);
                wrapped(&message.text, usize::from(width.max(1)))
                    .into_iter()
                    .map(move |row| Line::from(Span::styled(row, style)))
            })
            .collect();
        let fit = usize::from(height.saturating_sub(1));
        rows.drain(..rows.len().saturating_sub(fit));
        let top = height.saturating_sub(1).saturating_sub(to_u16(rows.len()));
        let area = Rect::new(0, top, width, height - top);
        frame.render_widget(Clear, area);
        let prompt = "Press ENTER or type command to continue";
        rows.push(Line::from(Span::styled(
            prompt,
            self.theme.style(Group::MoreMsg),
        )));
        frame.render_widget(Paragraph::new(rows), area);
        let x = to_u16(prompt.width()).min(width.saturating_sub(1));
        frame.set_cursor_position(Position::new(x, height.saturating_sub(1)));
    }
}

/// `text` cut into rows of at most `width` cells.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    let mut used = 0;
    for ch in text.chars() {
        let cells = ch.to_string().width();
        if used + cells > width && used > 0 {
            rows.push(String::new());
            used = 0;
        }
        rows.last_mut().expect("rows starts non-empty").push(ch);
        used += cells;
    }
    rows
}

/// A statusline: the mode (on the current window only) and the buffer's name on the left, the ruler on the right.
fn statusline(
    mode: Option<&str>,
    name: &str,
    ruler: &str,
    position: &str,
    width: u16,
) -> Line<'static> {
    let left = match mode {
        Some(mode) => format!(" {mode}  {name}"),
        None => format!(" {name}"),
    };
    let right = format!("{ruler}  {position} ");
    let gap = usize::from(width)
        .saturating_sub(left.width() + right.width())
        .max(1);
    Line::from(format!("{left}{}{right}", " ".repeat(gap)))
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
