//! The app's behaviour, observed the way a user does: keys in, screen out.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::doc::{Document, Line};
use crate::key::{Key, parse_keys};

/// An app showing one buffer of `lines`, in a `width` × `height` terminal.
fn app_with(lines: &[&str], width: u16, height: u16) -> App {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        lines
            .iter()
            .map(|line| Line::text(*line))
            .collect::<Vec<_>>(),
    );
    let mut app = App::with_buffer("test", doc);
    let _ = app.update(Msg::Resize { width, height });
    app
}

/// Types `notation` and collects the effects.
fn press(app: &mut App, notation: &str) -> Vec<Effect> {
    parse_keys(notation, Key::char(' '))
        .into_iter()
        .flat_map(|key| app.update(Msg::Key(key)))
        .collect()
}

/// What the terminal shows: each row's text, trailing blanks trimmed, and the cursor's (column, row).
struct Screen {
    rows: Vec<String>,
    cursor: (u16, u16),
}

fn screen(app: &App) -> Screen {
    let (width, height) = app.size();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    let cursor = terminal.get_cursor_position().expect("cursor");
    Screen {
        rows,
        cursor: (cursor.x, cursor.y),
    }
}

#[test]
fn the_window_shows_the_buffer_with_tildes_past_its_end() {
    let app = app_with(&["one", "two"], 20, 7);

    let shown = screen(&app);

    assert_eq!(shown.rows[1..5], ["one", "two", "~", "~"]);
    assert_eq!(shown.cursor, (0, 1));
}

#[test]
fn normal_mode_keys_move_the_cursor() {
    let mut app = app_with(&["one", "two three"], 20, 7);

    let _ = press(&mut app, "jw");

    assert_eq!(screen(&app).cursor, (4, 2));
}

#[test]
fn q_closes_the_last_window_and_so_quits() {
    let mut app = app_with(&["one"], 20, 7);

    assert_eq!(press(&mut app, "q"), [Effect::Quit]);
}

#[test]
fn a_paste_never_runs_commands() {
    let mut app = app_with(&["one"], 20, 7);

    assert_eq!(app.update(Msg::Paste("q".into())), []);
}

#[test]
fn the_app_tracks_terminal_focus() {
    let mut app = app_with(&["one"], 20, 7);
    assert!(app.is_focused());

    let _ = app.update(Msg::FocusLost);
    assert!(!app.is_focused());
    let _ = app.update(Msg::FocusGained);
    assert!(app.is_focused());
}

/// An app on a buffer of `line 1` … `line {count}`, with a window four rows tall.
fn lines_app(count: usize) -> App {
    let lines: Vec<String> = (1..=count).map(|n| format!("line {n}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    app_with(&lines, 20, 7)
}

/// The window's rows as shown.
fn window_rows(app: &App) -> Vec<String> {
    screen(app).rows[1..5].to_vec()
}

#[test]
fn moving_past_the_bottom_scrolls_just_enough() {
    let mut app = lines_app(20);

    let _ = press(&mut app, "4j");

    assert_eq!(window_rows(&app), ["line 2", "line 3", "line 4", "line 5"]);
    assert_eq!(screen(&app).cursor, (0, 4));
    let _ = press(&mut app, "k3k");
    assert_eq!(window_rows(&app), ["line 1", "line 2", "line 3", "line 4"]);
}

#[test]
fn ctrl_e_and_ctrl_y_scroll_the_view_and_push_the_cursor_along_only_when_it_would_leave() {
    let mut app = lines_app(20);
    let _ = press(&mut app, "j");

    let _ = press(&mut app, "<C-e>");
    assert_eq!(window_rows(&app)[0], "line 2");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-e>");
    assert_eq!(window_rows(&app)[0], "line 3");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "3<C-y>");
    assert_eq!(window_rows(&app)[0], "line 1");
    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn ctrl_d_and_ctrl_u_move_half_a_window_and_ctrl_f_and_ctrl_b_a_page() {
    let mut app = lines_app(20);

    let _ = press(&mut app, "<C-d>");
    assert_eq!(window_rows(&app)[0], "line 3");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-u>");
    assert_eq!(window_rows(&app)[0], "line 1");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-f>");
    assert_eq!(window_rows(&app)[0], "line 3");
    let _ = press(&mut app, "<C-b>");
    assert_eq!(window_rows(&app)[0], "line 1");
}

#[test]
fn zt_zz_and_zb_place_the_cursor_row_in_the_window() {
    let mut app = lines_app(20);
    let _ = press(&mut app, "10G");

    let _ = press(&mut app, "zt");
    assert_eq!(window_rows(&app)[0], "line 10");
    let _ = press(&mut app, "zb");
    assert_eq!(window_rows(&app)[0], "line 7");
    let _ = press(&mut app, "zz");
    assert_eq!(window_rows(&app)[0], "line 9");
    assert_eq!(screen(&app).cursor, (0, 2));
}

#[test]
fn big_h_m_and_l_go_to_the_top_middle_and_bottom_of_the_window() {
    let mut app = lines_app(20);
    let _ = press(&mut app, "10G");

    let _ = press(&mut app, "H");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "L");
    assert_eq!(screen(&app).cursor, (0, 4));
    let _ = press(&mut app, "M");
    assert_eq!(screen(&app).cursor, (0, 2));
    let _ = press(&mut app, "2H");
    assert_eq!(screen(&app).cursor, (0, 2));
    assert_eq!(window_rows(&app)[0], "line 7");
}

#[test]
fn the_view_stays_on_its_text_when_blocks_arrive_above() {
    let mut app = lines_app(20);
    let _ = press(&mut app, "<C-e>");

    app.doc_mut()
        .push_front("new", vec![Line::text("brand new")]);

    assert_eq!(window_rows(&app)[0], "line 2");
}

#[test]
fn an_untouched_view_shows_blocks_arriving_above() {
    let mut app = lines_app(20);

    app.doc_mut()
        .push_front("new", vec![Line::text("brand new")]);

    assert_eq!(window_rows(&app)[0], "brand new");
}
