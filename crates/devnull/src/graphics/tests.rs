//! Pictures as the terminal shows them.

use image::{ImageFormat, Rgba, RgbaImage};

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

    let cells = picture.half_blocks(2, 1);

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
        picture.half_blocks(1, 1),
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
    let cells = picture.half_blocks(4, 1);

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
