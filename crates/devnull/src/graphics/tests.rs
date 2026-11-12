//! Pictures as the terminal shows them.

use std::time::Duration;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageFormat, Rgba, RgbaImage};

use super::*;

/// `width` × `height` pixels, red above half its height and blue below, as PNG.
fn png(width: u32, height: u32) -> Vec<u8> {
    let picture = RgbaImage::from_fn(width, height, |_, y| {
        if y < height / 2 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut bytes, ImageFormat::Png).expect("png");
    bytes.into_inner()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// An animated GIF of `width` × `height` pixels: each of `frames` one colour, shown for its delay in milliseconds.
fn gif(width: u32, height: u32, frames: &[([u8; 4], u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut bytes, 30);
        encoder.set_repeat(Repeat::Infinite).expect("repeat");
        for &(colour, delay) in frames {
            let pixels = RgbaImage::from_pixel(width, height, Rgba(colour));
            let delay = Delay::from_numer_denom_ms(delay, 1);
            encoder
                .encode_frame(Frame::from_parts(pixels, 0, 0, delay))
                .expect("frame");
        }
    }
    bytes
}

/// The bytes of `name` in `tests/fixtures`.
fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(path).expect("fixture")
}

/// Whether each frame is red or blue (in its first cell's upper pixel), and how long it shows.
fn frames_of(picture: &Picture) -> Vec<(&'static str, Duration)> {
    picture
        .half_blocks(1, 1)
        .iter()
        .map(|frame| {
            let colour = match frame.cells[0][0].upper {
                Some((r, _, b)) if r > 200 && b < 60 => "red",
                Some((r, _, b)) if b > 200 && r < 60 => "blue",
                other => panic!("neither red nor blue: {other:?}"),
            };
            (colour, frame.delay)
        })
        .collect()
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

/// `width` × `height` pixels of noise, as PNG: too much to send in one chunk.
fn noisy_png(width: u32, height: u32) -> Vec<u8> {
    let picture = RgbaImage::from_fn(width, height, |x, y| {
        let n = x
            .wrapping_mul(2_654_435_761)
            .wrapping_add(y.wrapping_mul(40_503))
            .to_le_bytes();
        Rgba([n[0], n[1], n[2], 255])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut bytes, ImageFormat::Png).expect("png");
    bytes.into_inner()
}

#[test]
fn a_picture_fits_the_cells_as_wide_as_allowed_keeping_its_shape() {
    // Cells are twice as tall as they are wide.
    assert_eq!(fit(400, 200, 40, 30), (40, 10));
    assert_eq!(
        fit(200, 400, 40, 10),
        (10, 10),
        "too tall: as tall as allowed"
    );
    assert_eq!(fit(4000, 10, 40, 30), (40, 1), "at least a row");
    assert_eq!(
        fit(400, 200, 500, 500),
        (MAX_COLS, MAX_COLS / 4),
        "never more than the protocol's cells"
    );
    assert_eq!(fit(0, 10, 40, 30), (0, 0));
}

#[test]
fn at_its_own_size_a_picture_takes_the_cells_its_pixels_cover() {
    assert_eq!(natural(400, 200, (10, 20)), (40, 10));
    assert_eq!(
        natural(405, 201, (10, 20)),
        (41, 10),
        "a cell started counts, its shape kept"
    );
    assert_eq!(
        natural(4000, 2000, (10, 20)),
        (96, 24),
        "within the protocol's limits"
    );
    assert_eq!(natural(0, 200, (10, 20)), (0, 0));
}

#[test]
fn pictures_decode_from_png_and_anything_else_is_refused() {
    let picture = Picture::decode(&png(8, 4)).expect("picture");

    assert_eq!(picture.size(), (8, 4));
    assert!(Picture::decode(b"not a picture").is_none());
}

#[test]
fn pictures_too_large_to_decode_are_refused() {
    assert!(
        Picture::decode(&png(4000, 2001)).is_none(),
        "over 8 megapixels"
    );
}

#[test]
fn half_blocks_put_the_upper_pixel_in_front_and_the_lower_behind() {
    let picture = Picture::decode(&png(4, 4)).expect("picture");

    let frames = picture.half_blocks(2, 1);

    assert_eq!(frames.len(), 1, "a still picture is one frame");
    let cells = &frames[0].cells;
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].len(), 2);
    let HalfBlock {
        upper: Some(upper),
        lower: Some(lower),
    } = cells[0][0]
    else {
        panic!("opaque: {:?}", cells[0][0]);
    };
    assert!(upper.0 > 200 && upper.2 < 60, "red above: {upper:?}");
    assert!(lower.2 > 200 && lower.0 < 60, "blue below: {lower:?}");
}

#[test]
fn the_kitty_transmission_sends_a_png_in_chunks_as_a_virtual_placement() {
    let picture = Picture::decode(&noisy_png(400, 200)).expect("picture");

    let sent = picture.kitty(0x0a_0b_0c, 40, 10);

    let chunks: Vec<&str> = sent
        .split("\x1b\\")
        .filter(|chunk| !chunk.is_empty())
        .collect();
    assert!(
        chunks[0].starts_with("\x1b_Ga=T,f=100,t=d,i=658188,c=40,r=10,U=1,q=2,m="),
        "{}",
        &chunks[0][..60]
    );
    assert!(chunks.iter().all(|chunk| chunk.starts_with("\x1b_G")));
    assert!(chunks.last().expect("a chunk").starts_with("\x1b_Gm=0"));
    assert!(chunks.iter().all(|chunk| chunk.len() <= CHUNK + 64));
}

#[test]
fn a_placeholder_cell_names_its_row_and_column_and_its_colour_the_image() {
    assert_eq!(placeholder(0, 0), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(placeholder(2, 1), "\u{10EEEE}\u{30E}\u{30D}");
    assert_eq!(image_colour(0x0a_0b_0c), (10, 11, 12));
}

#[test]
fn deleting_an_image_frees_it_quietly() {
    assert_eq!(kitty_delete(7), "\x1b_Ga=d,d=I,i=7,q=2\x1b\\");
}

#[test]
fn kitty_graphics_are_guessed_from_the_terminal_and_never_through_tmux() {
    let guess =
        |term: &str, program: &str, tmux: bool| Graphics::guess(Some(term), Some(program), tmux);

    assert_eq!(guess("xterm-ghostty", "", false), Graphics::Kitty);
    assert_eq!(guess("xterm-kitty", "", false), Graphics::Kitty);
    assert_eq!(guess("xterm-256color", "ghostty", false), Graphics::Kitty);
    assert_eq!(
        guess("xterm-256color", "Apple_Terminal", false),
        Graphics::HalfBlocks
    );
    assert_eq!(
        guess("xterm-ghostty", "", true),
        Graphics::HalfBlocks,
        "placeholders need passthrough there"
    );
    assert_eq!(Graphics::guess(None, None, false), Graphics::HalfBlocks);
}

#[test]
fn graphics_have_the_names_set_takes() {
    for graphics in [Graphics::Kitty, Graphics::HalfBlocks] {
        assert_eq!(Graphics::named(graphics.name()), Some(graphics));
    }
    assert_eq!(Graphics::named("sixel"), None);
}

#[test]
fn transparent_pixels_show_the_background() {
    let clear = RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 0]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    clear.write_to(&mut bytes, ImageFormat::Png).expect("png");
    let picture = Picture::decode(&bytes.into_inner()).expect("picture");

    assert_eq!(
        picture.half_blocks(1, 1)[0].cells,
        [[HalfBlock {
            upper: None,
            lower: None
        }]]
    );
}

#[test]
fn a_picture_keeps_its_shape_in_a_box_of_another_centred_on_nothing() {
    let square = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    square.write_to(&mut bytes, ImageFormat::Png).expect("png");
    let picture = Picture::decode(&bytes.into_inner()).expect("picture");

    // Four cells across, one down: a box twice as wide as tall, so the square takes the middle half.
    let cells = &picture.half_blocks(4, 1)[0].cells;

    let filled: Vec<bool> = cells[0].iter().map(|cell| cell.upper.is_some()).collect();
    assert_eq!(filled, [false, true, true, false]);
}

/// The picture a kitty transmission sends, decoded from its chunks.
fn sent_picture(sent: &str) -> image::DynamicImage {
    let payload: String = sent
        .split("\x1b\\")
        .filter_map(|chunk| chunk.split_once(';').map(|(_, data)| data))
        .collect();
    let png = STANDARD.decode(payload).expect("base64");
    image::load_from_memory(&png).expect("png")
}

#[test]
fn a_small_picture_is_sent_at_its_size_for_the_terminal_to_stretch() {
    let picture = Picture::decode(&png(4, 2)).expect("picture");

    let sent = sent_picture(&picture.kitty(1, 48, 12));

    // The box's shape (48 × 16 by 12 × 32 pixels: 2:1) around the picture's own 4 × 2.
    assert_eq!((sent.width(), sent.height()), (4, 2));
}

#[test]
fn an_animated_gif_has_every_frame_with_its_delay() {
    let picture = Picture::decode(&gif(4, 2, &[(RED, 50), (BLUE, 120)])).expect("picture");

    assert_eq!(frames_of(&picture), [("red", ms(50)), ("blue", ms(120))]);
}

#[test]
fn animated_webp_and_png_move_too() {
    // Made with Pillow, which writes both; `image` writes neither.
    for name in ["two-frames.webp", "two-frames.png"] {
        let picture = Picture::decode(&fixture(name)).expect(name);

        assert_eq!(
            frames_of(&picture),
            [("red", ms(60)), ("blue", ms(120))],
            "{name}"
        );
    }
}

#[test]
fn a_frame_too_brief_shows_for_a_tenth_of_a_second_as_in_browsers() {
    let picture = Picture::decode(&gif(1, 1, &[(RED, 0), (BLUE, 10), (RED, 20)])).expect("picture");

    let delays: Vec<Duration> = frames_of(&picture).into_iter().map(|(_, d)| d).collect();
    assert_eq!(delays, [ms(100), ms(100), ms(20)]);
}

#[test]
fn an_animation_too_long_or_too_large_stays_on_its_first_frame() {
    let long: Vec<([u8; 4], u32)> = (0..=MAX_FRAMES)
        .map(|n| (if n == 0 { RED } else { BLUE }, 50))
        .collect();
    let picture = Picture::decode(&gif(1, 1, &long)).expect("picture");
    assert!(matches!(frames_of(&picture)[..], [("red", _)]));

    // Three frames of 3 megapixels, over 8 in all (made with Pillow: encoding it here takes seconds).
    let large = fixture("large-animation.gif");
    let picture = Picture::decode(&large).expect("picture");
    assert!(matches!(frames_of(&picture)[..], [("red", _)]));
}

#[test]
fn the_kitty_transmission_of_an_animation_sends_its_frames_then_runs_it() {
    let picture = Picture::decode(&gif(4, 2, &[(RED, 50), (BLUE, 120)])).expect("picture");

    let sent = picture.kitty(7, 4, 1);

    let escapes: Vec<&str> = sent
        .split("\x1b\\")
        .filter(|escape| !escape.is_empty())
        .collect();
    assert!(escapes[0].starts_with("\x1b_Ga=T,f=100,t=d,i=7,c=4,r=1,U=1,q=2,m=0;"));
    assert!(escapes[1].starts_with("\x1b_Ga=f,f=100,i=7,z=120,q=2,m=0;"));
    assert_eq!(
        escapes[2..],
        ["\x1b_Ga=a,i=7,r=1,z=50,q=2", "\x1b_Ga=a,i=7,s=3,v=1,q=2"]
    );
    let blue = sent_picture(&format!("{}\x1b\\", escapes[1]));
    assert_eq!(blue.to_rgba8().get_pixel(0, 0).0, BLUE);
}

#[test]
fn every_chunk_of_a_frame_says_it_is_one() {
    let noise = |seed: u32| {
        RgbaImage::from_fn(200, 100, |x, y| {
            let n = (x ^ seed)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(y.wrapping_mul(40_503))
                .to_le_bytes();
            Rgba([n[0], n[1], n[2], 255])
        })
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut bytes, 30);
        for seed in [1, 2] {
            let frame = Frame::from_parts(noise(seed), 0, 0, Delay::from_numer_denom_ms(50, 1));
            encoder.encode_frame(frame).expect("frame");
        }
    }
    let picture = Picture::decode(&bytes).expect("picture");

    let sent = picture.kitty(7, 20, 5);

    let start = sent.find("\x1b_Ga=f,").expect("a frame");
    let end = sent.find("\x1b_Ga=a,").expect("then the animation");
    let frame: Vec<&str> = sent[start..end]
        .split("\x1b\\")
        .filter(|escape| !escape.is_empty())
        .collect();
    assert!(frame.len() > 1, "in chunks");
    assert!(
        frame.iter().all(|escape| escape.starts_with("\x1b_Ga=f,")),
        "{:?}",
        frame.iter().map(|escape| &escape[..12]).collect::<Vec<_>>()
    );
}

#[test]
fn a_still_picture_is_sent_without_frames() {
    let sent = Picture::decode(&png(4, 2)).expect("picture").kitty(7, 4, 1);

    assert!(!sent.contains("a=f") && !sent.contains("a=a"), "{sent:?}");
}
