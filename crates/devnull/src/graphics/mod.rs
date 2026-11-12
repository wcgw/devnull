//! Pictures as the terminal shows them (D7): fitted to cells, then sent with the kitty graphics protocol and shown
//! through Unicode placeholder cells, or drawn with half blocks where the protocol is missing. Both are text, so a
//! picture scrolls, clips and folds with the rest of a buffer.

#[cfg(test)]
mod tests;

use std::io::Cursor;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, Frames, ImageFormat, ImageReader, RgbaImage};

/// The most cells a picture takes across: no more than the placeholder diacritics below can number.
pub const MAX_COLS: u16 = 96;

/// The most cells a picture takes down.
pub const MAX_ROWS: u16 = 48;

/// The most pixels a picture may have to be decoded, as the Go client allows.
const MAX_PIXELS: u64 = 8_000_000;

/// Pixels sent per cell, across and down, for a sharp picture on dense screens without sending more than needed.
const CELL_PIXELS: (u32, u32) = (16, 32);

/// Base64 bytes per chunk of a transmission, as the kitty protocol requires at most.
const CHUNK: usize = 4096;

/// The character of a placeholder cell.
const PLACEHOLDER: char = '\u{10EEEE}';

/// The first entries of kitty's `rowcolumn-diacritics.txt`, the protocol's table: entry `n` stands for row or column
/// `n`. Enough for [`MAX_COLS`] and [`MAX_ROWS`].
const DIACRITICS: [char; 100] = [
    '\u{305}', '\u{30D}', '\u{30E}', '\u{310}', '\u{312}', '\u{33D}', '\u{33E}', '\u{33F}',
    '\u{346}', '\u{34A}', '\u{34B}', '\u{34C}', '\u{350}', '\u{351}', '\u{352}', '\u{357}',
    '\u{35B}', '\u{363}', '\u{364}', '\u{365}', '\u{366}', '\u{367}', '\u{368}', '\u{369}',
    '\u{36A}', '\u{36B}', '\u{36C}', '\u{36D}', '\u{36E}', '\u{36F}', '\u{483}', '\u{484}',
    '\u{485}', '\u{486}', '\u{487}', '\u{592}', '\u{593}', '\u{594}', '\u{595}', '\u{597}',
    '\u{598}', '\u{599}', '\u{59C}', '\u{59D}', '\u{59E}', '\u{59F}', '\u{5A0}', '\u{5A1}',
    '\u{5A8}', '\u{5A9}', '\u{5AB}', '\u{5AC}', '\u{5AF}', '\u{5C4}', '\u{610}', '\u{611}',
    '\u{612}', '\u{613}', '\u{614}', '\u{615}', '\u{616}', '\u{617}', '\u{657}', '\u{658}',
    '\u{659}', '\u{65A}', '\u{65B}', '\u{65D}', '\u{65E}', '\u{6D6}', '\u{6D7}', '\u{6D8}',
    '\u{6D9}', '\u{6DA}', '\u{6DB}', '\u{6DC}', '\u{6DF}', '\u{6E0}', '\u{6E1}', '\u{6E2}',
    '\u{6E4}', '\u{6E7}', '\u{6E8}', '\u{6EB}', '\u{6EC}', '\u{730}', '\u{732}', '\u{733}',
    '\u{735}', '\u{736}', '\u{73A}', '\u{73D}', '\u{73F}', '\u{740}', '\u{741}', '\u{743}',
    '\u{745}', '\u{747}', '\u{749}', '\u{74A}',
];

/// How pictures are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graphics {
    /// Sent with the kitty graphics protocol, shown through placeholder cells.
    Kitty,
    /// Drawn with half blocks in true colour.
    HalfBlocks,
}

impl Graphics {
    /// Every way pictures are drawn.
    pub const ALL: [Self; 2] = [Self::Kitty, Self::HalfBlocks];

    /// What the terminal likely speaks, from `$TERM` and `$TERM_PROGRAM`: the kitty protocol in kitty and Ghostty,
    /// but not inside tmux (`tmux`), where placeholders would need passing through; half blocks elsewhere.
    #[must_use]
    pub fn guess(term: Option<&str>, program: Option<&str>, tmux: bool) -> Self {
        let named = |name: &str| {
            [term, program]
                .into_iter()
                .flatten()
                .any(|value| value.to_lowercase().contains(name))
        };
        if !tmux && (named("kitty") || named("ghostty")) {
            Self::Kitty
        } else {
            Self::HalfBlocks
        }
    }

    /// Its name, as `:set graphics=` takes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Kitty => "kitty",
            Self::HalfBlocks => "halfblocks",
        }
    }

    /// The graphics of that name.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        [Self::Kitty, Self::HalfBlocks]
            .into_iter()
            .find(|graphics| graphics.name() == name)
    }
}

/// The cells a `width` × `height` pixel picture takes when as wide as `max_cols` and no taller than `max_rows`
/// (both within the protocol's limits), as (columns, rows); cells are twice as tall as they are wide. `(0, 0)` for
/// an empty picture.
#[must_use]
pub fn fit(width: u32, height: u32, max_cols: u16, max_rows: u16) -> (u16, u16) {
    if width == 0 || height == 0 || max_cols == 0 || max_rows == 0 {
        return (0, 0);
    }
    let (width, height) = (f64::from(width), f64::from(height));
    let max_rows = max_rows.min(MAX_ROWS);
    let mut cols = max_cols.min(MAX_COLS);
    let mut rows = rounded(f64::from(cols) * height / width / 2.0).max(1);
    if rows > max_rows {
        rows = max_rows;
        cols = cols
            .min(rounded(f64::from(rows) * 2.0 * width / height))
            .max(1);
    }
    (cols, rows)
}

/// The cells a `width` × `height` pixel picture takes at its own size, a cell being `cell` pixels (across, down):
/// those its pixels cover, keeping its shape within the protocol's limits, as (columns, rows). `(0, 0)` for an empty
/// picture.
#[must_use]
pub fn natural(width: u32, height: u32, cell: (u16, u16)) -> (u16, u16) {
    let covered = |pixels: u32, per_cell: u16| {
        u16::try_from(pixels.div_ceil(u32::from(per_cell.max(1)))).unwrap_or(u16::MAX)
    };
    fit(
        width,
        height,
        covered(width, cell.0),
        covered(height, cell.1),
    )
}

/// `value`, which is positive, rounded to cells.
fn rounded(value: f64) -> u16 {
    // Cells are counted in the thousands at most: the cast neither truncates nor loses a sign.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = value.round().min(f64::from(u16::MAX)) as u16;
    cells
}

/// The most frames an animation keeps; a longer one stays on its first.
const MAX_FRAMES: usize = 200;

/// A frame said to show this briefly or less shows for [`BRIEF_FRAME`] instead, as browsers do.
const BRIEFEST_FRAME: Duration = Duration::from_millis(10);

/// How long a frame said to show too briefly shows.
const BRIEF_FRAME: Duration = Duration::from_millis(100);

/// A decoded picture: a still one, or an animation.
#[derive(Debug, Clone)]
pub struct Picture {
    /// Never empty: a still picture is one frame.
    frames: Vec<Frame>,
}

/// A frame of a picture, all of it.
#[derive(Debug, Clone)]
struct Frame {
    pixels: RgbaImage,
    /// How long it shows before the next, in an animation.
    delay: Duration,
}

/// A frame of a picture drawn with half blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HalfBlockFrame {
    /// Its cells, by rows.
    pub cells: Vec<Vec<HalfBlock>>,
    /// How long it shows before the next, in an animation.
    pub delay: Duration,
}

/// One cell of a picture drawn with half blocks: `▀` in the upper pixel's colour over the lower's. `None` is a
/// transparent pixel, which shows the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfBlock {
    /// The upper pixel, as RGB.
    pub upper: Option<(u8, u8, u8)>,
    /// The lower pixel, as RGB.
    pub lower: Option<(u8, u8, u8)>,
}

impl Picture {
    /// The picture `bytes` hold: PNG, JPEG, WebP or GIF, of at most 8 megapixels. An animated GIF, WebP or PNG
    /// keeps its frames, up to [`MAX_FRAMES`] and 8 megapixels in all; a longer or larger one only its first. `None`
    /// for anything else.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let format = reader.format()?;
        if !matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif
        ) {
            return None;
        }
        let (width, height) = reader.into_dimensions().ok()?;
        let canvas = u64::from(width) * u64::from(height);
        if canvas == 0 || canvas > MAX_PIXELS {
            return None;
        }
        if let Some(frames) = animation(bytes, format, canvas) {
            return Some(Self { frames });
        }
        let picture = image::load_from_memory_with_format(bytes, format).ok()?;
        Some(Self {
            frames: vec![Frame {
                pixels: picture.to_rgba8(),
                delay: Duration::ZERO,
            }],
        })
    }

    /// Its size in pixels, as (width, height).
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.frames[0].pixels.dimensions()
    }

    /// The kitty protocol escapes that send it as image `id`, shown in `cols` × `rows` cells by placeholders (a
    /// virtual placement), quietly: a PNG of at most [`CELL_PIXELS`] per cell, base64 in chunks. An animation sends
    /// its other frames too, then has the terminal run it, looping (D7).
    #[must_use]
    pub fn kitty(&self, id: u32, cols: u16, rows: u16) -> String {
        let (width, height) = (
            u32::from(cols) * CELL_PIXELS.0,
            u32::from(rows) * CELL_PIXELS.1,
        );
        let mut sent = String::new();
        for (i, frame) in self.frames.iter().enumerate() {
            let Some(png) = png(&boxed(&frame.pixels, width, height, Fill::Shape)) else {
                return String::new();
            };
            if i == 0 {
                let keys = format!("a=T,f=100,t=d,i={id},c={cols},r={rows},U=1,q=2");
                transmit(&mut sent, &keys, "", &png);
            } else {
                let keys = format!("a=f,f=100,i={id},z={},q=2", frame.delay.as_millis());
                transmit(&mut sent, &keys, "a=f,", &png);
            }
        }
        if let [first, _, ..] = &self.frames[..] {
            // The root frame's gap can only be set once it is there.
            let gap = first.delay.as_millis();
            sent.push_str(&format!("\x1b_Ga=a,i={id},r=1,z={gap},q=2\x1b\\"));
            sent.push_str(&format!("\x1b_Ga=a,i={id},s=3,v=1,q=2\x1b\\"));
        }
        sent
    }

    /// The picture as `cols` × `rows` cells of half blocks, frame by frame.
    #[must_use]
    pub fn half_blocks(&self, cols: u16, rows: u16) -> Vec<HalfBlockFrame> {
        self.frames
            .iter()
            .map(|frame| HalfBlockFrame {
                cells: half_blocks(&frame.pixels, cols, rows),
                delay: frame.delay,
            })
            .collect()
    }
}

/// The frames of the GIF, or animated WebP or PNG, `bytes` hold as `format`, each of `canvas` pixels, how long each
/// shows made sensible; `None` for anything else, or when they are more than [`MAX_FRAMES`] or [`MAX_PIXELS`] in all.
fn animation(bytes: &[u8], format: ImageFormat, canvas: u64) -> Option<Vec<Frame>> {
    // A still WebP or PNG is not decoded as frames, then again as a picture.
    let frames: Frames<'_> = match format {
        ImageFormat::Gif => GifDecoder::new(Cursor::new(bytes)).ok()?.into_frames(),
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
            if !decoder.has_animation() {
                return None;
            }
            decoder.into_frames()
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::new(Cursor::new(bytes)).ok()?;
            if !decoder.is_apng().ok()? {
                return None;
            }
            decoder.apng().ok()?.into_frames()
        }
        _ => return None,
    };
    let most = usize::try_from(MAX_PIXELS / canvas)
        .unwrap_or(usize::MAX)
        .min(MAX_FRAMES);
    let mut kept = Vec::new();
    for frame in frames {
        if kept.len() == most {
            return None;
        }
        let frame = frame.ok()?;
        let delay = Duration::from(frame.delay());
        kept.push(Frame {
            delay: if delay <= BRIEFEST_FRAME {
                BRIEF_FRAME
            } else {
                delay
            },
            pixels: frame.into_buffer(),
        });
    }
    (!kept.is_empty()).then_some(kept)
}

/// `pixels` as a PNG, if it can be encoded.
fn png(pixels: &RgbaImage) -> Option<Vec<u8>> {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels.clone())
        .write_to(&mut png, ImageFormat::Png)
        .ok()?;
    Some(png.into_inner())
}

/// Appends to `sent` the escapes that send `data` in base64 chunks: the first with `keys`, the others with
/// `continued` (`a=f,` for a frame, which every chunk must say) before their own.
fn transmit(sent: &mut String, keys: &str, continued: &str, data: &[u8]) {
    let encoded = STANDARD.encode(data);
    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(CHUNK).collect();
    sent.reserve(encoded.len() + chunks.len() * 64);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        // Chunks are base64, so ASCII.
        let chunk = std::str::from_utf8(chunk).unwrap_or_default();
        if i == 0 {
            sent.push_str(&format!("\x1b_G{keys},m={more};{chunk}\x1b\\"));
        } else {
            sent.push_str(&format!("\x1b_G{continued}m={more},q=2;{chunk}\x1b\\"));
        }
    }
}

/// `pixels` as `cols` × `rows` cells of half blocks, by rows.
fn half_blocks(pixels: &RgbaImage, cols: u16, rows: u16) -> Vec<Vec<HalfBlock>> {
    if cols == 0 || rows == 0 {
        return Vec::new();
    }
    let scaled = boxed(pixels, u32::from(cols), u32::from(rows) * 2, Fill::Exactly);
    let colour = |x: u32, y: u32| {
        let [r, g, b, a] = scaled.get_pixel(x, y).0;
        (a >= 128).then_some((r, g, b))
    };
    (0..u32::from(rows))
        .map(|row| {
            (0..u32::from(cols))
                .map(|col| HalfBlock {
                    upper: colour(col, row * 2),
                    lower: colour(col, row * 2 + 1),
                })
                .collect()
        })
        .collect()
}

/// How [`Picture::boxed`] fills its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// Exactly the box's pixels: the picture scaled up or down to fit.
    Exactly,
    /// The box's shape, at most its size: the picture scaled down to fit, never up, for a terminal that stretches
    /// it over the cells.
    Shape,
}

/// `source` centred in a transparent canvas filling a `width` × `height` pixel box as `fill` says, keeping its
/// own shape.
fn boxed(source: &RgbaImage, width: u32, height: u32, fill: Fill) -> RgbaImage {
    let (w, h) = source.dimensions();
    let (box_w, box_h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let mut scale = (box_w / f64::from(w)).min(box_h / f64::from(h));
    if fill == Fill::Shape {
        scale = scale.min(1.0);
    }
    let scaled_w = pixels(f64::from(w) * scale);
    let scaled_h = pixels(f64::from(h) * scale);
    let scaled = if (scaled_w, scaled_h) == (w, h) {
        source.clone()
    } else {
        image::imageops::resize(source, scaled_w, scaled_h, FilterType::CatmullRom)
    };
    let (canvas_w, canvas_h) = match fill {
        Fill::Exactly => (width.max(1), height.max(1)),
        Fill::Shape => (
            scaled_w.max(pixels(f64::from(scaled_h) * box_w / box_h)),
            scaled_h.max(pixels(f64::from(scaled_w) * box_h / box_w)),
        ),
    };
    let mut canvas = RgbaImage::new(canvas_w, canvas_h);
    image::imageops::overlay(
        &mut canvas,
        &scaled,
        i64::from((canvas_w - scaled_w.min(canvas_w)) / 2),
        i64::from((canvas_h - scaled_h.min(canvas_h)) / 2),
    );
    canvas
}

/// `value`, a positive size in pixels, rounded to at least one.
fn pixels(value: f64) -> u32 {
    // Pictures are 8 megapixels at most, and boxes a few thousand pixels: neither truncates nor loses a sign.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = value.round().clamp(1.0, f64::from(u32::MAX)) as u32;
    pixels
}

/// The text of the placeholder cell at `row` and `col` of a picture shown through placeholders; its foreground
/// colour names the picture (see [`image_colour`]).
#[must_use]
pub fn placeholder(row: u16, col: u16) -> String {
    let diacritic = |n: u16| DIACRITICS[usize::from(n).min(DIACRITICS.len() - 1)];
    [PLACEHOLDER, diacritic(row), diacritic(col)]
        .into_iter()
        .collect()
}

/// The foreground colour, as RGB, of the placeholder cells of image `id` (24 bits).
#[must_use]
pub fn image_colour(id: u32) -> (u8, u8, u8) {
    let [_, r, g, b] = id.to_be_bytes();
    (r, g, b)
}

/// The kitty protocol escape that frees image `id`, quietly.
#[must_use]
pub fn kitty_delete(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
}
