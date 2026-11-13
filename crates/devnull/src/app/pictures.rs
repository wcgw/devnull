//! Pictures in buffers (D7): asked for once they come into view, then drawn in the space their lines take, by the
//! terminal through placeholders (kitty) or as half blocks. The terminal runs animations it is sent; those drawn with
//! half blocks move on frame by frame while in view, as wake-ups say.

use std::collections::HashMap;
use std::time::Duration;

use ratatui::buffer::Buffer as Cells;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{App, Effect, Wake, WakePurpose};
use crate::doc::{Layout, PictureRow};
use crate::graphics::{self, Graphics, HalfBlock, HalfBlockFrame};

/// The narrowest picture that says, while it is not drawn, that it is loading or unavailable.
const STATUS_COLS: u16 = 12;

/// A picture ready to draw, as the runtime prepares it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Prepared {
    /// The escapes that send it to the terminal, to write as they are; then placeholders show it.
    Kitty(String),
    /// Its frames: one for a still picture. Those of an animation show for some time each.
    HalfBlocks(Vec<HalfBlockFrame>),
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
    /// Drawn with half blocks.
    HalfBlocks(Playing),
    /// It could not be had.
    Failed,
}

/// A picture drawn with half blocks: its frames, and where it is in them.
#[derive(Debug)]
struct Playing {
    frames: Vec<HalfBlockFrame>,
    /// The frame shown.
    shown: usize,
    /// How much longer it shows, for an animation.
    left: Duration,
}

impl Playing {
    fn new(frames: Vec<HalfBlockFrame>) -> Self {
        let left = frames.first().map_or(Duration::ZERO, |frame| frame.delay);
        Self {
            frames,
            shown: 0,
            left,
        }
    }

    /// Whether it moves: it has frames to move on to, each showing for some time.
    fn is_animated(&self) -> bool {
        self.frames.len() > 1 && !self.left.is_zero()
    }

    /// The cells of the frame shown, by rows.
    fn cells(&self) -> &[Vec<HalfBlock>] {
        self.frames
            .get(self.shown)
            .map_or(&[], |frame| &frame.cells[..])
    }

    /// Moves on by `elapsed`, to the frame showing then.
    fn advance(&mut self, mut elapsed: Duration) {
        while self.is_animated() && elapsed >= self.left {
            elapsed -= self.left;
            self.shown = (self.shown + 1) % self.frames.len();
            self.left = self.frames[self.shown].delay;
        }
        self.left = self.left.saturating_sub(elapsed);
    }
}

/// The pictures asked for.
#[derive(Debug)]
pub(super) struct Pictures {
    states: HashMap<Key, State>,
    /// The id the next picture sent to the terminal gets.
    next_id: u32,
    /// How long after it was asked for the wake-up for the next frames comes, while one is to come.
    frame_wake: Option<Duration>,
    /// That wake-up's generation: those of another are stale, and none comes while there is no `frame_wake`.
    frame_generation: u64,
}

impl Default for Pictures {
    fn default() -> Self {
        Self {
            states: HashMap::new(),
            next_id: 1,
            frame_wake: None,
            frame_generation: 0,
        }
    }
}

impl Pictures {
    /// The generation of the wake-up for the next frames.
    pub(super) fn frame_generation(&self) -> u64 {
        self.frame_generation
    }

    /// Frees what was sent to the terminal and forgets every picture: the escapes to write.
    pub(super) fn clear(&mut self) -> Vec<Effect> {
        self.frame_wake = None;
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
            Some(State::HalfBlocks(playing)) => {
                let Some(row) = playing.cells().get(usize::from(picture.row)) else {
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
                    theme.style(crate::hl::TIME),
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

    /// What the pictures in view in any window need: those never asked for, asked for now; a wake-up for the next
    /// frame of an animation drawn with half blocks, unless one is to come.
    pub(super) fn pictures_in_view(&mut self) -> Vec<Effect> {
        let in_view = self.in_view();
        let mut effects = Vec::new();
        for key in &in_view {
            if self.pictures.states.contains_key(key) {
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
            self.pictures
                .states
                .insert(key.clone(), State::Loading(kitty));
        }
        let next = in_view
            .iter()
            .filter_map(|key| match self.pictures.states.get(key) {
                Some(State::HalfBlocks(playing)) if playing.is_animated() => Some(playing.left),
                _ => None,
            })
            .min();
        if let (Some(after), None) = (next, self.pictures.frame_wake) {
            self.pictures.frame_wake = Some(after);
            self.pictures.frame_generation += 1;
            effects.push(Effect::Wake {
                after,
                token: Wake {
                    purpose: WakePurpose::Frame,
                    generation: self.pictures.frame_generation,
                },
            });
        }
        effects
    }

    /// The wake-up for the next frames came: the animations in view move on by the time it waited.
    pub(super) fn next_frames(&mut self) {
        let Some(elapsed) = self.pictures.frame_wake.take() else {
            return;
        };
        for key in self.in_view() {
            if let Some(State::HalfBlocks(playing)) = self.pictures.states.get_mut(&key) {
                playing.advance(elapsed);
            }
        }
    }

    /// Draws the pictures of `layout`'s rows from `top` on, a row of `area` each.
    pub(super) fn draw_pictures(
        &self,
        cells: &mut Cells,
        layout: &Layout<'_>,
        top: usize,
        area: Rect,
    ) {
        for (y, row) in (area.y..area.bottom()).zip(top..layout.row_count()) {
            if let Some(picture) = layout.picture(row) {
                let line = Rect::new(area.x, y, area.width, 1);
                self.pictures.draw(cells, line, picture, &self.theme);
            }
            for (col, picture) in layout.inline_pictures(row) {
                let col = u16::try_from(col).unwrap_or(u16::MAX);
                if col < area.width {
                    let rest = Rect::new(area.x + col, y, area.width - col, 1);
                    self.pictures.draw(cells, rest, picture, &self.theme);
                }
            }
        }
    }

    /// The pictures in view, each once: in any window, and on the hover card.
    fn in_view(&self) -> Vec<Key> {
        let arrangement = self.windows.arrange(self.windows_area());
        let mut wanted: Vec<Key> = Vec::new();
        for &(id, rect) in &arrangement.windows {
            let window = self.windows.get(id);
            let area = super::text_area(rect);
            let layout = window.layout(&self.buffers, area);
            let viewport = super::Viewport {
                height: usize::from(area.height),
                scrolloff: self.options.scrolloff,
            };
            let top = window.top(&layout, viewport);
            let bottom = (top + usize::from(area.height)).min(layout.row_count());
            add_pictures(&layout, top..bottom, &mut wanted);
        }
        if let Some(hover) = &self.hover {
            // Which pictures it has does not depend on the width it is laid out to.
            let layout = Layout::new(hover.doc(), u16::MAX);
            add_pictures(&layout, 0..layout.row_count(), &mut wanted);
        }
        wanted
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
            (Ok(Prepared::HalfBlocks(frames)), None) => {
                (State::HalfBlocks(Playing::new(frames)), Vec::new())
            }
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

/// Adds to `wanted` the pictures on `rows` of `layout` it does not have yet.
fn add_pictures(layout: &Layout<'_>, rows: std::ops::Range<usize>, wanted: &mut Vec<Key>) {
    for row in rows {
        let inline = layout
            .inline_pictures(row)
            .into_iter()
            .map(|(_, picture)| picture);
        for picture in layout.picture(row).into_iter().chain(inline) {
            let key = Key {
                source: picture.source.to_owned(),
                cols: picture.cols,
                rows: picture.rows,
            };
            if !wanted.contains(&key) {
                wanted.push(key);
            }
        }
    }
}
