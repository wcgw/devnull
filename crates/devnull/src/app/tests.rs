//! The app's behaviour, observed the way a user does: keys in, screen out.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::doc::{Content, Document, Fold, Line};
use crate::hl::Group;
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

/// The command line: the last row.
fn cmdline(app: &App) -> String {
    screen(app).rows.last().cloned().unwrap_or_default()
}

/// The current window's statusline: the row above the command line.
fn statusline(app: &App) -> String {
    let rows = screen(app).rows;
    rows[rows.len() - 2].clone()
}

#[test]
fn the_screen_has_a_bufferline_a_statusline_and_a_command_line() {
    let mut app = app_with(&["one two", "three"], 30, 6);
    let _ = press(&mut app, "jw");

    insta::assert_snapshot!(screen(&app).rows.join("\n"), @r"
 test
one two
three
~
 NORMAL  test        2,5  All
");
}

#[test]
fn colon_opens_the_command_line_and_escape_leaves_it() {
    let mut app = app_with(&["one"], 30, 6);

    let _ = press(&mut app, ":se");
    assert_eq!(cmdline(&app), ":se");
    assert_eq!(screen(&app).cursor, (3, 5));
    assert!(statusline(&app).contains("COMMAND"));
    let _ = press(&mut app, "<Esc>");
    assert_eq!(cmdline(&app), "");
    assert!(statusline(&app).contains("NORMAL"));
}

#[test]
fn ex_commands_run_on_enter() {
    let mut app = app_with(&["one"], 30, 6);

    assert_eq!(press(&mut app, ":q<CR>"), [Effect::Quit]);
    assert_eq!(press(&mut app, ":qa!<CR>"), [Effect::Quit]);
}

#[test]
fn errors_show_until_the_next_key_and_messages_keeps_them() {
    let mut app = app_with(&["one", "two"], 40, 8);

    let _ = press(&mut app, ":frob<CR>");
    assert_eq!(cmdline(&app), "E492: Not an editor command: frob");
    let _ = press(&mut app, "j");
    assert_eq!(cmdline(&app), "");

    let _ = press(&mut app, ":messages<CR>");
    let rows = screen(&app).rows;
    assert_eq!(rows[rows.len() - 2], "E492: Not an editor command: frob");
    assert_eq!(
        rows[rows.len() - 1],
        "Press ENTER or type command to continue"
    );
    let _ = press(&mut app, "<CR>");
    assert_eq!(cmdline(&app), "");
}

#[test]
fn the_command_line_edits_like_vim() {
    let mut app = app_with(&["one"], 40, 6);

    let _ = press(&mut app, ":set foo bar<BS><C-w>baz");
    assert_eq!(cmdline(&app), ":set foo baz");
    let _ = press(&mut app, "<Home><Right><Right><Del>x");
    assert_eq!(cmdline(&app), ":sex foo baz");
    let _ = press(&mut app, "<C-u>abc<Left>");
    assert_eq!(screen(&app).cursor, (3, 5));
    let _ = app.update(Msg::Paste("de\nignored".into()));
    assert_eq!(cmdline(&app), ":abdec foo baz");
    let _ = press(&mut app, "<End><C-u><BS>");
    assert!(statusline(&app).contains("NORMAL"));
}

#[test]
fn the_command_line_remembers_what_ran() {
    let mut app = app_with(&["one"], 40, 6);
    let _ = press(&mut app, ":set ic<CR>:set so=2<CR>");

    let _ = press(&mut app, ":<Up>");
    assert_eq!(cmdline(&app), ":set so=2");
    let _ = press(&mut app, "<Up>");
    assert_eq!(cmdline(&app), ":set ic");
    let _ = press(&mut app, "<Down><Down>");
    assert_eq!(cmdline(&app), ":");
}

#[test]
fn set_changes_and_shows_options() {
    let mut app = app_with(&["one"], 40, 6);

    let _ = press(&mut app, ":set ic?<CR>");
    assert_eq!(cmdline(&app), "noignorecase");
    let _ = press(&mut app, ":set ignorecase<CR>:set ic?<CR>");
    assert_eq!(cmdline(&app), "ignorecase");
    let _ = press(&mut app, ":set timeoutlen=500<CR>:set tm?<CR>");
    assert_eq!(cmdline(&app), "timeoutlen=500");
    let _ = press(&mut app, ":set foo<CR>");
    assert_eq!(cmdline(&app), "E518: Unknown option: foo");
    let _ = press(&mut app, ":set so=x<CR>");
    assert_eq!(cmdline(&app), "E521: Number required after =: so=x");
}

#[test]
fn scrolloff_keeps_rows_of_context_around_the_cursor() {
    let mut app = lines_app(20);
    let _ = press(&mut app, ":set so=1<CR>");

    let _ = press(&mut app, "3j");

    assert_eq!(window_rows(&app)[0], "line 2");
    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn maps_defined_on_the_command_line_apply_and_unmap_removes_them() {
    let mut app = app_with(&["one", "two", "three"], 40, 7);

    let _ = press(&mut app, ":nnoremap x jj<CR>x");
    assert_eq!(screen(&app).cursor, (0, 3));
    let _ = press(&mut app, ":nunmap x<CR>gg");
    let _ = press(&mut app, "x");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, ":nunmap x<CR>");
    assert_eq!(cmdline(&app), "E31: No such mapping");
}

#[test]
fn a_map_can_run_an_ex_command() {
    let mut app = app_with(&["one"], 40, 6);
    let _ = press(&mut app, ":nnoremap <lt>leader>x :frob<lt>CR><CR>");

    let _ = press(&mut app, "<Space>x");

    assert_eq!(cmdline(&app), "E492: Not an editor command: frob");
}

#[test]
fn slash_searches_and_n_and_big_n_repeat() {
    let mut app = app_with(&["one fish", "two fish", "red"], 40, 7);

    let _ = press(&mut app, "/fish<CR>");
    assert_eq!(screen(&app).cursor, (4, 1));
    let _ = press(&mut app, "n");
    assert_eq!(screen(&app).cursor, (4, 2));
    let _ = press(&mut app, "n");
    assert_eq!(screen(&app).cursor, (4, 1));
    assert_eq!(cmdline(&app), "search hit BOTTOM, continuing at TOP");
    let _ = press(&mut app, "N");
    assert_eq!(screen(&app).cursor, (4, 2));
    assert_eq!(cmdline(&app), "search hit TOP, continuing at BOTTOM");
    let _ = press(&mut app, "?one<CR>");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "/nope<CR>");
    assert_eq!(cmdline(&app), "E486: Pattern not found: nope");
    let _ = press(&mut app, "/(<CR>");
    assert!(cmdline(&app).starts_with("invalid pattern"));
}

#[test]
fn star_searches_the_word_and_n_repeats_it() {
    let mut app = app_with(&["fish one", "fishy", "a fish"], 40, 7);

    let _ = press(&mut app, "*");
    assert_eq!(screen(&app).cursor, (2, 3));
    let _ = press(&mut app, "n");
    assert_eq!(screen(&app).cursor, (0, 1));
}

#[test]
fn ignorecase_and_smartcase_apply_to_searches() {
    let mut app = app_with(&["Fish fish"], 40, 6);

    let _ = press(&mut app, ":set ic scs<CR>/fish<CR>");
    assert_eq!(screen(&app).cursor, (5, 1));
    let _ = press(&mut app, "/Fish<CR>");
    assert_eq!(screen(&app).cursor, (0, 1));
}

#[test]
fn pending_keys_show_at_the_right_of_the_command_line() {
    let mut app = app_with(&["one"], 30, 6);

    let _ = press(&mut app, "3g");

    assert!(cmdline(&app).ends_with("3g"), "{:?}", cmdline(&app));
}

/// The style of the cell at (`x`, `y`).
fn style_at(app: &App, x: u16, y: u16) -> ratatui::style::Style {
    let (width, height) = app.size();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal.backend().buffer()[(x, y)].style()
}

#[test]
fn v_selects_what_motions_cover_and_y_yanks_it_to_the_clipboard() {
    let mut app = app_with(&["one two", "three"], 30, 7);

    let _ = press(&mut app, "wv");
    assert!(statusline(&app).contains("VISUAL"));
    let effects = press(&mut app, "jy");

    assert_eq!(effects, [Effect::Copy("two\nthree".into())]);
    assert!(statusline(&app).contains("NORMAL"));
    assert_eq!(screen(&app).cursor, (4, 1));
}

#[test]
fn the_selection_is_highlighted() {
    let mut app = app_with(&["one two"], 30, 7);
    let _ = press(&mut app, "vl");

    // A cell is highlighted when patching it with the Visual style changes nothing.
    let highlighted = |x| {
        let style = style_at(&app, x, 1);
        style.patch(Group::Visual.style()) == style
    };
    assert!(highlighted(0));
    assert!(highlighted(1));
    assert!(!highlighted(2));
}

#[test]
fn big_v_yanks_rows_and_reports_more_than_two_lines() {
    let mut app = app_with(&["one", "two", "three", "four"], 30, 8);

    assert_eq!(press(&mut app, "Vjy"), [Effect::Copy("one\ntwo\n".into())]);
    assert_eq!(cmdline(&app), "");
    assert_eq!(
        press(&mut app, "V2jy"),
        [Effect::Copy("one\ntwo\nthree\n".into())]
    );
    assert_eq!(cmdline(&app), "3 lines yanked");
    assert!(statusline(&app).contains("NORMAL"));
}

#[test]
fn yy_yanks_the_cursor_row() {
    let mut app = app_with(&["one two", "three"], 30, 7);

    let copied: Vec<Effect> = press(&mut app, "jyy")
        .into_iter()
        .filter(|e| matches!(e, Effect::Copy(_)))
        .collect();

    assert_eq!(copied, [Effect::Copy("three\n".into())]);
}

#[test]
fn visual_kinds_switch_and_leave_like_vim() {
    let mut app = app_with(&["one two"], 30, 7);

    let _ = press(&mut app, "vV");
    assert!(statusline(&app).contains("V-LINE"));
    let _ = press(&mut app, "V");
    assert!(statusline(&app).contains("NORMAL"));
    let _ = press(&mut app, "vw<Esc>");
    assert!(statusline(&app).contains("NORMAL"));
    assert_eq!(screen(&app).cursor, (4, 1));
}

/// An app whose buffer has `head`, a closed fold over three lines summarised as `+ 3 more`, and `tail`.
fn folded_app() -> App {
    let mut app = app_with(&[], 30, 9);
    let lines = ["head", "one", "fish", "three", "tail"]
        .map(Line::text)
        .to_vec();
    let fold = Fold {
        lines: 1..4,
        summary: Line::text("+ 3 more"),
        closed: true,
    };
    app.doc_mut().push_back(
        "folded",
        Content {
            lines,
            folds: vec![fold],
        },
    );
    app
}

#[test]
fn fold_keys_open_and_close_folds() {
    let mut app = folded_app();
    assert_eq!(screen(&app).rows[1..5], ["head", "+ 3 more", "tail", "~"]);

    let _ = press(&mut app, "jzo");
    assert_eq!(screen(&app).rows[1..5], ["head", "one", "fish", "three"]);
    let _ = press(&mut app, "zc");
    assert_eq!(screen(&app).rows[2], "+ 3 more");
    let _ = press(&mut app, "zR");
    assert_eq!(screen(&app).rows[2], "one");
    let _ = press(&mut app, "zM");
    assert_eq!(screen(&app).rows[2], "+ 3 more");
}

#[test]
fn a_search_landing_in_a_closed_fold_opens_it() {
    let mut app = folded_app();

    let _ = press(&mut app, "/fish<CR>");

    assert_eq!(screen(&app).rows[1..5], ["head", "one", "fish", "three"]);
    assert_eq!(screen(&app).cursor, (0, 3));
}

/// The wake-up an effect list asks for, if any, with its delay.
fn wakes(effects: &[Effect]) -> Vec<(std::time::Duration, Wake)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Wake { after, token } => Some((*after, *token)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_ambiguous_map_runs_when_timeoutlen_passes_without_another_key() {
    let mut app = app_with(&["one", "two", "three"], 30, 7);
    let _ = press(&mut app, ":nnoremap d j<CR>:set tm=300<CR>");

    let effects = press(&mut app, "d");
    let timeout = wakes(&effects)
        .into_iter()
        .find(|(after, _)| after.as_millis() == 300)
        .expect("a timeout");
    assert_eq!(screen(&app).cursor, (0, 1));

    let _ = app.update(Msg::Wake(timeout.1));
    assert_eq!(screen(&app).cursor, (0, 2));
}

#[test]
fn a_wake_up_from_before_the_last_key_is_ignored() {
    let mut app = app_with(&["one", "two", "three"], 30, 7);
    let _ = press(&mut app, ":nnoremap d j<CR>");
    let stale = wakes(&press(&mut app, "d"))
        .into_iter()
        .map(|(_, token)| token)
        .collect::<Vec<_>>();

    let _ = press(&mut app, "<Esc>");
    for token in stale {
        let _ = app.update(Msg::Wake(token));
    }

    assert_eq!(screen(&app).cursor, (0, 1));
}

#[test]
fn which_key_shows_what_can_follow_a_prefix_after_a_moment() {
    let mut app = app_with(&["one"], 60, 14);

    let effects = press(&mut app, "z");
    let (after, token) = wakes(&effects)[0];
    assert_eq!(after.as_millis(), 200);
    assert!(
        !screen(&app)
            .rows
            .iter()
            .any(|row| row.contains("open fold"))
    );

    let _ = app.update(Msg::Wake(token));
    let rows = screen(&app).rows;
    assert!(
        rows.iter().any(|row| row.contains("o → open fold")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("t → row to top")),
        "{rows:#?}"
    );

    let _ = press(&mut app, "o");
    assert!(
        !screen(&app)
            .rows
            .iter()
            .any(|row| row.contains("open fold"))
    );
}

#[test]
fn which_key_marks_groups_of_further_keys() {
    let mut app = app_with(&["one"], 60, 14);
    let _ = press(&mut app, ":nnoremap <lt>leader>ab j<CR>");

    let effects = press(&mut app, "<Space>");
    let _ = app.update(Msg::Wake(wakes(&effects)[0].1));

    let rows = screen(&app).rows;
    assert!(
        rows.iter().any(|row| row.contains("1 → following log")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("a → +prefix")),
        "{rows:#?}"
    );
}
