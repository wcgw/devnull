//! Pictures in buffers (D7): asked for once they come into view, then drawn in the space their lines take, by the
//! terminal through placeholders (kitty) or as half blocks.

use std::collections::HashMap;

use ratatui::buffer::Buffer as Cells;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{App, Effect};
use crate::doc::{Layout, PictureRow};
use crate::graphics::{self, Graphics, HalfBlock};
use crate::hl::Group;

/// The narrowest picture that says, while it is not drawn, that it is loading or unavailable.
const STATUS_COLS: u16 = 12;

/// A picture ready to draw, as the runtime prepares it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Prepared {
    /// The escapes that send it to the terminal, to write as they are; then placeholders show it.
    Kitty(String),
    /// Its cells, by rows.
    HalfBlocks(Vec<Vec<HalfBlock>>),
}

/// A picture at one size.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    source: String,
    cols: u16,
    rows: u16,
}

/// Where a picture stands.
#[derive(Debug)]
enum State {
    /// Asked for; with the id it will have in the terminal, for the kitty protocol.
    Loading(Option<u32>),
    /// Sent to the terminal as image `id`.
    Kitty(u32),
    /// Its cells, by rows.
    HalfBlocks(Vec<Vec<HalfBlock>>),
    /// It could not be had.
    Failed,
}

/// The pictures asked for.
#[derive(Debug)]
pub(super) struct Pictures {
    states: HashMap<Key, State>,
    /// The id the next picture sent to the terminal gets.
    next_id: u32,
}

impl Default for Pictures {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            next_id: 1,
        }
    }
}

impl Pictures {
    /// Frees what was sent to the terminal and forgets every picture: the escapes to write.
    pub(super) fn clear(&mut self) -> Vec<Effect> {
        self.states
            .drain()
            .filter_map(|(_, state)| match state {
                State::Kitty(id) => Some(Effect::Graphics(graphics::kitty_delete(id))),
                _ => None,
            })
            .collect()
    }

    /// Draws row `picture` of a picture in `area`, a row of the screen, if it is ready; otherwise says how it
    /// stands in its first row.
    pub(super) fn draw(
        &self,
        cells: &mut Cells,
        area: Rect,
        picture: PictureRow<'_>,
        theme: &crate::hl::Theme,
    ) {
        let key = Key {
            source: picture.source.to_owned(),
            cols: picture.cols,
            rows: picture.rows,
        };
        let width = picture.cols.min(area.width);
        match self.states.get(&key) {
            Some(State::Kitty(id)) => {
                let (r, g, b) = graphics::image_colour(*id);
                for col in 0..width {
                    if let Some(cell) = cells.cell_mut((area.x + col, area.y)) {
                        cell.set_symbol(&graphics::placeholder(picture.row, col))
                            .set_fg(Color::Rgb(r, g, b));
                    }
                }
            }
            Some(State::HalfBlocks(grid)) => {
                let Some(row) = grid.get(usize::from(picture.row)) else {
                    return;
                };
                for (col, half) in (0..width).zip(row) {
                    let Some(cell) = cells.cell_mut((area.x + col, area.y)) else {
                        continue;
                    };
                    let rgb = |(r, g, b)| Color::Rgb(r, g, b);
                    let mut style = Style::new();
                    if let Some(upper) = half.upper {
                        style = style.fg(rgb(upper));
                    }
                    if let Some(lower) = half.lower {
                        style = style.bg(rgb(lower));
                    }
                    let symbol = if half.upper.is_some() { "▀" } else { " " };
                    cell.set_symbol(symbol).set_style(style);
                }
            }
            // Saying how it stands needs room: a picture in a line of text, such as an avatar, says nothing.
            state if picture.row == 0 && picture.cols >= STATUS_COLS => {
                let text = match state {
                    Some(State::Failed) => "unavailable",
                    _ => "loading…",
                };
                cells.set_stringn(
                    area.x,
                    area.y,
                    text,
                    usize::from(area.width),
                    theme.style(Group::Time),
                );
            }
            _ => {}
        }
    }
}

impl App {
    /// Draws pictures as `graphics` says, from now on (as `:set graphics=` does): what was drawn otherwise is freed
    /// (the effects say how) and asked for again.
    pub fn set_graphics(&mut self, graphics: Graphics) -> Vec<Effect> {
        if graphics == self.options.graphics {
            return Vec::new();
        }
        self.options.graphics = graphics;
        self.pictures.clear()
    }

    /// Numbers the pictures sent to the terminal from `id` on, so they do not take the ids of another program's.
    pub fn number_pictures_from(&mut self, id: u32) {
        self.pictures.next_id = id.clamp(1, 0x00ff_ffff);
    }

    /// The pictures in view in any window that were never asked for, asked for now.
    pub(super) fn wanted_pictures(&mut self) -> Vec<Effect> {
        let arrangement = self.windows.arrange(self.windows_area());
        let mut wanted = Vec::new();
        for &(id, rect) in &arrangement.windows {
            let window = self.windows.get(id);
            let area = super::text_area(rect);
            let layout = Layout::new(&self.buffers[window.buffer].doc, area.width);
            let viewport = super::Viewport {
                height: usize::from(area.height),
                scrolloff: self.options.scrolloff,
            };
            let top = window.top(&layout, viewport);
            for row in top..(top + usize::from(area.height)).min(layout.row_count()) {
                let inline = layout
                    .inline_pictures(row)
                    .into_iter()
                    .map(|(_, picture)| picture);
                for picture in layout.picture(row).into_iter().chain(inline) {
                    wanted.push(Key {
                        source: picture.source.to_owned(),
                        cols: picture.cols,
                        rows: picture.rows,
                    });
                }
            }
        }
        let mut effects = Vec::new();
        for key in wanted {
            if self.pictures.states.contains_key(&key) {
                continue;
            }
            let kitty = (self.options.graphics == Graphics::Kitty).then(|| {
                let id = self.pictures.next_id;
                // Ids are 24 bits, the colour of the placeholder cells; past the last, they start over.
                self.pictures.next_id = if id >= 0x00ff_ffff { 1 } else { id + 1 };
                id
            });
            effects.push(Effect::Picture {
                source: key.source.clone(),
                cols: key.cols,
                rows: key.rows,
                kitty,
            });
            self.pictures.states.insert(key, State::Loading(kitty));
        }
        effects
    }

    /// A picture prepared, or why it could not be; ignored when no longer asked for as such.
    pub(super) fn picture_arrived(
        &mut self,
        source: String,
        cols: u16,
        rows: u16,
        result: Result<Prepared, String>,
    ) -> Vec<Effect> {
        let key = Key { source, cols, rows };
        let Some(State::Loading(kitty)) = self.pictures.states.get(&key) else {
            return Vec::new();
        };
        let kitty = *kitty;
        let (state, effects) = match (result, kitty) {
            (Ok(Prepared::Kitty(sent)), Some(id)) => {
                (State::Kitty(id), vec![Effect::Graphics(sent)])
            }
            (Ok(Prepared::HalfBlocks(grid)), None) => (State::HalfBlocks(grid), Vec::new()),
            // Prepared for other graphics than asked for, or not at all.
            _ => (State::Failed, Vec::new()),
        };
        self.pictures.states.insert(key, state);
        effects
    }

    /// Frees the pictures sent to the terminal, before quitting.
    pub(super) fn free_pictures(&mut self) -> Vec<Effect> {
        self.pictures.clear()
    }
}
