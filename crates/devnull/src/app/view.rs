//! Drawing the app: bufferline, windows with their statuslines, and the command line.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::App;
use crate::doc::Layout;
use crate::hl::Group;

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
        let layout = Layout::new(&buffer.doc, area.width);
        let top = self.window.top(&layout, usize::from(area.height));
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

        let status = Line::from(format!(" {}", buffer.name)).style(Group::StatusLine.style());
        frame.render_widget(
            Paragraph::new(status),
            Rect::new(0, height.saturating_sub(2), width, 1),
        );

        let cursor = self.window.cursor.screen(&layout);
        let x = area
            .x
            .saturating_add(u16::try_from(cursor.col).unwrap_or(u16::MAX));
        let y = area
            .y
            .saturating_add(u16::try_from(cursor.row - top).unwrap_or(u16::MAX));
        frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), y));
    }
}
