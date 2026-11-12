//! The app's behaviour, observed the way a user does: keys in, screen out.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use super::*;
use devnull_api::model::AccountKind;
use devnull_api::model::{Commit, Cursor, Page};
use devnull_api::{Category, Log, OrganizationTab, People};

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
        style.patch(app.style(Group::Visual)) == style
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
            ..Content::default()
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
fn each_window_has_its_own_folds_and_a_split_starts_with_its_parent_s() {
    let mut app = folded_app();
    let row = |app: &App| screen(app).rows[2].clone();

    let _ = press(&mut app, "jzo:vs<CR>");
    assert!(
        row(&app).starts_with("one ") && row(&app).ends_with("│one"),
        "{:?}",
        row(&app)
    );
    let _ = press(&mut app, "zc");
    assert!(
        row(&app).starts_with("+ 3 more") && row(&app).ends_with("│one"),
        "only the current window's: {:?}",
        row(&app)
    );
    let _ = press(&mut app, "<C-w>lzM<C-w>hzR");
    assert!(
        row(&app).starts_with("one ") && row(&app).ends_with("│+ 3 more"),
        "zR and zM too: {:?}",
        row(&app)
    );
}

#[test]
fn a_window_keeps_its_folds_in_a_buffer_it_leaves_and_comes_back_to() {
    let body = (1..=8)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = with_log(Log::Following, one_commit(&body, serde_json::json!({})));
    assert!(shows(&app, "… 2 more lines"), "{:#?}", screen(&app).rows);

    let _ = press(&mut app, "zR:e log/latest<CR>zM<C-^>");

    assert!(shows(&app, "line 8"), "{:#?}", screen(&app).rows);
}

/// An app on a block of `head`, `one`, `fish`, `three`, `tail`: an outer fold over the middle three, and an inner
/// one over `fish`, both closed.
fn nested_folds_app() -> App {
    let mut app = app_with(&[], 30, 9);
    let lines = ["head", "one", "fish", "three", "tail"]
        .map(Line::text)
        .to_vec();
    let fold = |lines, summary| Fold {
        lines,
        summary: Line::text(summary),
        closed: true,
    };
    app.doc_mut().push_back(
        "folded",
        Content {
            lines,
            folds: vec![fold(1..4, "+ outer"), fold(2..3, "+ inner")],
            ..Content::default()
        },
    );
    app
}

#[test]
fn nested_folds_open_and_close_a_level_or_all_of_them() {
    let mut app = nested_folds_app();

    let _ = press(&mut app, "jzo");
    assert_eq!(
        screen(&app).rows[1..6],
        ["head", "one", "+ inner", "three", "tail"]
    );
    let _ = press(&mut app, "jzO");
    assert_eq!(screen(&app).rows[3], "fish");
    let _ = press(&mut app, "zC");
    assert_eq!(screen(&app).rows[1..4], ["head", "+ outer", "tail"]);
    let _ = press(&mut app, "zA");
    assert_eq!(screen(&app).rows[3], "fish");
}

#[test]
fn a_search_landing_in_nested_closed_folds_opens_them_all() {
    let mut app = nested_folds_app();

    let _ = press(&mut app, "/fish<CR>");

    assert_eq!(screen(&app).rows[3], "fish");
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
    let mut app = app_with(&["one"], 60, 20);

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

#[test]
fn split_stacks_two_windows_on_the_same_buffer_with_their_own_cursors() {
    let mut app = app_with(&["one", "two", "three"], 30, 12);

    let _ = press(&mut app, ":sp<CR>");
    let rows = screen(&app).rows;
    assert_eq!(rows[1..4], ["one", "two", "three"]);
    assert_eq!(rows[6..9], ["one", "two", "three"]);
    assert!(
        rows[5].contains("NORMAL") && rows[5].contains("test"),
        "{rows:#?}"
    );
    assert!(
        !rows[10].contains("NORMAL") && rows[10].contains("test"),
        "{rows:#?}"
    );
    assert_eq!(screen(&app).cursor, (0, 1));

    let _ = press(&mut app, "jj<C-w>j");
    assert_eq!(screen(&app).cursor, (0, 6));
    let _ = press(&mut app, "<C-w>k");
    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn vsplit_puts_windows_side_by_side_with_a_separator() {
    let mut app = app_with(&["one", "two"], 21, 8);

    let _ = press(&mut app, "<C-w>v");
    let rows = screen(&app).rows;
    assert_eq!(rows[1], "one       │one");
    assert_eq!(rows[2], "two       │two");
    let _ = press(&mut app, "<C-w>l");
    assert_eq!(screen(&app).cursor, (11, 1));
    let _ = press(&mut app, "<C-w>h");
    assert_eq!(screen(&app).cursor, (0, 1));
}

#[test]
fn side_by_side_statuslines_meet_under_the_separator_as_in_vim() {
    let mut app = app_with(&["one", "two"], 21, 8);
    let status = |app: &App, x: u16| (style_at(app, x, 6).fg, style_at(app, x, 6).bg);

    let _ = press(&mut app, "<C-w>v");
    assert_eq!(screen(&app).rows[6].chars().nth(10), Some(' '));
    assert_eq!(
        status(&app, 10),
        status(&app, 9),
        "the current window's statusline, on the left, goes on"
    );
    let _ = press(&mut app, "<C-w>l");
    assert_eq!(status(&app, 10), status(&app, 9), "now another's");
    assert_ne!(status(&app, 10), status(&app, 11));

    let _ = press(&mut app, "<C-w>h:sp<CR>");
    let rows = screen(&app).rows;
    assert_eq!(
        rows[3].chars().nth(10),
        Some('│'),
        "the bar crosses a statusline it does not meet"
    );
    assert_eq!(rows[6].chars().nth(10), Some(' '));
    assert_eq!(
        status(&app, 10),
        status(&app, 9),
        "the window at the bottom left, not the current one above it"
    );
}

/// The text of the row the cursor is on.
fn cursor_line(app: &App) -> String {
    let screen = screen(app);
    screen.rows[usize::from(screen.cursor.1)].clone()
}

#[test]
fn help_lists_the_key_bindings_in_a_window_above() {
    let mut app = app_with(&["one"], 80, 30);

    let _ = press(&mut app, ":help<CR>");

    assert_eq!(statuslines(&app), 2);
    let rows = screen(&app).rows;
    assert!(rows[1].contains("Key bindings"), "{rows:#?}");
    assert!(rows[2].contains("<leader> is <Space>"), "{rows:#?}");
    assert!(
        !rows[0].contains("help"),
        "unlisted, as Vim's help: {:?}",
        rows[0]
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("NORMAL") && row.contains("help")),
        "the current window: {rows:#?}"
    );
    let _ = press(&mut app, ":help<CR>");
    assert_eq!(statuslines(&app), 2, "the help window again");
    let _ = press(&mut app, "q");
    assert_eq!(statuslines(&app), 1);
}

#[test]
fn help_with_keys_goes_to_their_binding_with_its_command() {
    let mut app = app_with(&["one"], 80, 30);

    let _ = press(&mut app, ":help zo<CR>");
    let line = cursor_line(&app);
    assert!(
        line.contains("zo") && line.contains("open fold"),
        "{line:?}"
    );
    let _ = press(&mut app, ":help s<CR>");
    let line = cursor_line(&app);
    assert!(
        line.contains("toggle star") && line.contains(":star"),
        "{line:?}"
    );

    let _ = press(&mut app, ":help nothing<CR>");
    assert_eq!(cmdline(&app), "E149: Sorry, no help for nothing");
}

#[test]
fn help_lists_the_user_maps_too() {
    let mut app = app_with(&["one"], 80, 30);
    let _ = press(&mut app, ":nmap <lt>leader>c :commit @acme<lt>CR><CR>");

    let _ = press(&mut app, ":help <lt>leader>c<CR>");

    let line = cursor_line(&app);
    assert!(
        line.contains("<leader>c") && line.contains(":commit @acme<CR>"),
        "{line:?}"
    );
}

#[test]
fn ctrl_w_w_cycles_through_the_windows() {
    let mut app = app_with(&["one"], 30, 16);
    let _ = press(&mut app, ":sp<CR>:sp<CR>");
    let first = screen(&app).cursor;

    let _ = press(&mut app, "<C-w>w<C-w>w");
    assert_ne!(screen(&app).cursor, first);
    let _ = press(&mut app, "<C-w>w");
    assert_eq!(screen(&app).cursor, first);
}

#[test]
fn close_and_only_remove_windows_but_never_the_last() {
    let mut app = app_with(&["one"], 60, 12);
    let _ = press(&mut app, ":sp<CR>:vs<CR>");

    let _ = press(&mut app, "<C-w>o");
    assert_eq!(statuslines(&app), 1);
    let _ = press(&mut app, ":sp<CR><C-w>c");
    assert_eq!(statuslines(&app), 1);
    let _ = press(&mut app, ":close<CR>");
    assert_eq!(cmdline(&app), "E444: Cannot close last window");
}

#[test]
fn q_closes_a_window_and_quits_only_with_the_last() {
    let mut app = app_with(&["one"], 60, 12);
    let _ = press(&mut app, ":vs<CR>");
    assert_eq!(statuslines(&app), 2);

    assert_eq!(press(&mut app, "q"), []);
    assert_eq!(statuslines(&app), 1);
    assert_eq!(press(&mut app, ":q<CR>"), [Effect::Quit]);
}

/// How many statuslines the screen shows, counted by their rulers (every window's cursor is at 1,1 here).
fn statuslines(app: &App) -> usize {
    screen(app)
        .rows
        .iter()
        .map(|row| row.matches("1,1").count())
        .sum()
}

#[test]
fn ctrl_o_and_ctrl_i_walk_back_and_forth_through_jumps() {
    let mut app = app_with(&["one", "two", "three", "four", "five"], 30, 10);

    let _ = press(&mut app, "/thr<CR>G");
    assert_eq!(screen(&app).cursor, (0, 5));
    let _ = press(&mut app, "<C-o>");
    assert_eq!(screen(&app).cursor, (0, 3));
    let _ = press(&mut app, "<C-o>");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-o>");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-i>");
    assert_eq!(screen(&app).cursor, (0, 3));
    let _ = press(&mut app, "<C-i>");
    assert_eq!(screen(&app).cursor, (0, 5));
}

#[test]
fn ordinary_motions_are_not_jumps() {
    let mut app = app_with(&["one", "two", "three"], 30, 10);

    let _ = press(&mut app, "jj<C-o>");

    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn a_jump_after_going_back_starts_the_way_forward_again() {
    let mut app = app_with(&["one", "two", "three", "four", "five"], 30, 10);
    let _ = press(&mut app, "G<C-o>");
    assert_eq!(screen(&app).cursor, (0, 1));

    let _ = press(&mut app, "3G<C-o>");
    assert_eq!(screen(&app).cursor, (0, 1));
    let _ = press(&mut app, "<C-i>");
    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn the_jumplist_keeps_one_entry_per_row() {
    let mut app = app_with(&["one", "two", "three", "four", "five"], 30, 10);

    let _ = press(&mut app, "GggGgg<C-o><C-o>");

    assert_eq!(screen(&app).cursor, (0, 5));
}

#[test]
fn colorscheme_shows_and_switches_the_colorscheme() {
    let mut app = app_with(&["one"], 40, 6);

    let _ = press(&mut app, ":colorscheme<CR>");
    assert_eq!(cmdline(&app), "tokyonight-night");
    let night = style_at(&app, 0, 1).bg;
    let _ = press(&mut app, ":colo tokyonight-day<CR>");
    assert_ne!(style_at(&app, 0, 1).bg, night);
    let _ = press(&mut app, ":colo nope<CR>");
    assert_eq!(cmdline(&app), "E185: Cannot find color scheme 'nope'");
}

#[test]
fn hi_restyles_a_group_on_screen_and_hi_clear_restores_it() {
    let mut app = app_with(&["one"], 60, 6);
    let original = style_at(&app, 10, 1).bg;

    let _ = press(&mut app, ":hi Normal guibg=#000000<CR>");
    assert_eq!(style_at(&app, 10, 1).bg, Some(Color::Rgb(0, 0, 0)));
    let _ = press(&mut app, ":hi Normal<CR>");
    assert!(
        cmdline(&app).starts_with("Normal         xxx guifg=#c0caf5 guibg=#000000"),
        "{:?}",
        cmdline(&app)
    );
    let _ = press(&mut app, ":hi clear<CR>");
    assert_eq!(style_at(&app, 10, 1).bg, original);
}

#[test]
fn the_statusline_mode_has_its_own_colours() {
    let mut app = app_with(&["one"], 40, 6);

    let normal = style_at(&app, 1, 4);
    let _ = press(&mut app, "v");
    let visual = style_at(&app, 1, 4);

    assert_eq!(normal.bg, app.style(Group::StatusLineModeNormal).bg);
    assert_eq!(visual.bg, app.style(Group::StatusLineModeVisual).bg);
}

#[test]
fn source_asks_for_the_file_and_runs_its_lines() {
    let mut app = app_with(&["one", "two", "three"], 40, 8);

    assert_eq!(
        press(&mut app, ":so ~/x.vim<CR>"),
        [Effect::Source("~/x.vim".into())]
    );
    let contents = "\" settings\nset scrolloff=1\nnnoremap x jj\n";
    let _ = app.update(Msg::Sourced {
        path: "~/x.vim".into(),
        contents: Ok(contents.into()),
    });

    assert_eq!(cmdline(&app), "");
    let _ = press(&mut app, "x");
    assert_eq!(screen(&app).cursor, (0, 3));
}

#[test]
fn errors_while_sourcing_name_the_file_and_line() {
    let mut app = app_with(&["one"], 50, 10);

    let contents = "set ic\nfrob\nset nope\n";
    let _ = app.update(Msg::Sourced {
        path: "init.vim".into(),
        contents: Ok(contents.into()),
    });

    let rows = screen(&app).rows;
    assert_eq!(
        rows[rows.len() - 6..],
        [
            "Error detected while processing init.vim:",
            "line 2:",
            "E492: Not an editor command: frob",
            "line 3:",
            "E518: Unknown option: nope",
            "Press ENTER or type command to continue",
        ]
    );
}

#[test]
fn a_file_that_cannot_be_read_is_reported() {
    let mut app = app_with(&["one"], 50, 6);

    let _ = app.update(Msg::Sourced {
        path: "nope.vim".into(),
        contents: Err("No such file".into()),
    });

    assert_eq!(cmdline(&app), "E484: Can't open file nope.vim");
}

#[test]
fn no_and_inv_prefixes_turn_flags_off_and_flip_them() {
    let mut app = app_with(&["one"], 50, 6);

    let _ = press(&mut app, ":set ic<CR>:set noic<CR>:set ic?<CR>");
    assert_eq!(cmdline(&app), "noignorecase");
    let _ = press(&mut app, ":set invignorecase<CR>:set ic?<CR>");
    assert_eq!(cmdline(&app), "ignorecase");
    let _ = press(&mut app, ":set noscrolloff<CR>");
    assert_eq!(cmdline(&app), "E474: Invalid argument: noscrolloff");
    let _ = press(&mut app, ":set nope<CR>");
    assert_eq!(cmdline(&app), "E518: Unknown option: nope");
}

#[test]
fn long_messages_wrap_in_the_message_view() {
    let mut app = app_with(&["one"], 20, 8);

    let _ = app.update(Msg::Sourced {
        path: "a/long/path/to/init.vim".into(),
        contents: Ok("frob".into()),
    });

    let rows = screen(&app).rows;
    assert_eq!(
        rows[rows.len() - 7..rows.len() - 4],
        [
            "Error detected while",
            " processing a/long/p",
            "ath/to/init.vim:"
        ]
    );
}

/// When the following fixture was "fetched": 2026-11-12T15:13:40Z.
const FIXTURE_NOW: u64 = 1_794_496_420;

/// The invented following log (`tests/fixtures/following.json`), as the API sends it.
fn following_page() -> Page<Commit> {
    let page: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/following.json")).expect("fixture");
    Page::new(
        serde_json::from_value(page["data"].clone()).expect("commits"),
        None,
    )
}

/// The app as `dn` starts it, in an 80 × 24 terminal, connected as `@ada` and with the following log loaded.
fn started() -> App {
    let mut app = App::new();
    let _ = app.update(Msg::Resize {
        width: 80,
        height: 24,
    });
    let _ = app.start();
    let _ = app.update(Msg::Net(NetEvent::Connected(me("ada", true))));
    let fetched = Fetched::Log {
        log: Log::Following,
        after: None,
        result: Ok(following_page()),
    };
    let _ = app.update(Msg::Fetched {
        fetched,
        now: FIXTURE_NOW,
    });
    app
}

#[test]
fn dn_starts_on_the_following_log() {
    let app = started();

    insta::assert_snapshot!(screen(&app).rows.join("\n"));
}

#[test]
fn double_brackets_go_from_commit_to_commit_and_zo_shows_the_rest_of_a_long_one() {
    let mut app = started();

    let _ = press(&mut app, "2]]");
    assert!(screen(&app).rows[screen(&app).cursor.1 as usize].starts_with("Ines Varga @ivarga"));
    // The header, six lines of text (one wraps onto a second row at 80 columns), then the fold.
    let _ = press(&mut app, "8jzo");
    let rows = screen(&app).rows;
    assert!(
        rows.iter()
            .any(|row| row.starts_with("Moral: never let a test read the clock")),
        "{rows:#?}"
    );
}

/// A fresh `dn` as the runtime starts it, before the network answered.
fn starting() -> (App, Vec<Effect>) {
    let mut app = App::new();
    let _ = app.update(Msg::Resize {
        width: 100,
        height: 20,
    });
    let effects = app.start();
    (app, effects)
}

fn net(app: &mut App, event: NetEvent) -> Vec<Effect> {
    app.update(Msg::Net(event))
}

fn me(handle: &str, terms_accepted: bool) -> devnull_api::model::Me {
    serde_json::from_value(serde_json::json!({
        "id": "u1", "handle": handle, "name": "Ada", "terms_accepted": terms_accepted, "terms_version": "2026-11-01"
    }))
    .expect("me")
}

fn shows(app: &App, text: &str) -> bool {
    screen(app).rows.iter().any(|row| row.contains(text))
}

#[test]
fn dn_starts_by_checking_its_connection() {
    let (app, effects) = starting();

    assert_eq!(effects, [Effect::Connect]);
    assert!(shows(&app, "Connecting to dev-null.sh"));
}

#[test]
fn without_a_key_connect_enrolls_and_opens_the_approval_page() {
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::NoKey);
    assert!(shows(&app, "This device is not connected"));

    assert_eq!(press(&mut app, ":connect<CR>"), [Effect::Enroll]);
    let url = "https://dev-null.sh/mobile/authorize?state=s";
    let effects = net(
        &mut app,
        NetEvent::Enrolling {
            url: url.into(),
            fingerprint: "SHA256:abc".into(),
        },
    );

    assert_eq!(effects, [Effect::OpenUrl(url.into())]);
    assert!(shows(&app, url) && shows(&app, "SHA256:abc"));
    let pasted = "http://127.0.0.1:4242/callback?code=c&state=s";
    assert_eq!(
        press(&mut app, &format!(":connect {pasted}<CR>")),
        [Effect::EnrollPasted(pasted.into())]
    );
}

#[test]
fn once_connected_the_log_shows_and_the_statusline_names_the_account() {
    let (mut app, _) = starting();

    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));

    assert!(shows(&app, "log/following"));
    assert!(statusline(&app).contains("@ada"), "{:?}", statusline(&app));
}

#[test]
fn an_unapproved_or_unreachable_device_says_why_and_connect_tries_again() {
    let (mut app, _) = starting();

    let _ = net(&mut app, NetEvent::NotApproved);
    assert!(shows(&app, "not approved"));
    assert_eq!(press(&mut app, ":connect<CR>"), [Effect::Enroll]);

    let _ = net(&mut app, NetEvent::Failed("cannot reach the server".into()));
    assert!(shows(&app, "cannot reach the server"));
    assert_eq!(press(&mut app, ":connect<CR>"), [Effect::Connect]);
}

#[test]
fn an_encrypted_key_asks_for_its_passphrase_without_showing_it() {
    let (mut app, _) = starting();

    let _ = net(&mut app, NetEvent::NeedsPassphrase { wrong: false });
    let effects = press(&mut app, "s3cret");
    assert_eq!(cmdline(&app), "Passphrase: ******");
    assert_eq!(effects, []);
    assert_eq!(press(&mut app, "<CR>"), [Effect::Unlock("s3cret".into())]);

    let _ = net(&mut app, NetEvent::NeedsPassphrase { wrong: true });
    assert!(shows(&app, "Wrong passphrase"));
    // The screen helper trims trailing blanks: this is the bare prompt.
    assert_eq!(cmdline(&app), "Passphrase:");
}

#[test]
fn terms_must_be_agreed_to_before_anything_else() {
    let (mut app, _) = starting();

    let _ = net(&mut app, NetEvent::Connected(me("ada", false)));
    assert!(shows(&app, "terms"));
    assert!(!shows(&app, "log/following"));

    assert_eq!(
        press(&mut app, ":agree<CR>"),
        [Effect::AcceptTerms("2026-11-01".into())]
    );
}

#[test]
fn once_connected_the_following_log_is_fetched_and_shows_while_loading() {
    let (mut app, _) = starting();

    let effects = net(&mut app, NetEvent::Connected(me("ada", true)));

    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Following,
            after: None
        }]
    );
    assert!(shows(&app, "Loading log/following…"));
}

#[test]
fn a_fetched_page_shows_its_commits() {
    let app = started();

    assert!(shows(&app, "Ada Quill @adaq · 2m · 7f3a9c1"));
    assert!(shows(
        &app,
        "Shipped the new parser today. Thanks @mrivers for the review!"
    ));
}

#[test]
fn a_log_that_cannot_load_says_why() {
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));

    let result = Err(FetchError::Failed("cannot reach the server".into()));
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Following,
            after: None,
            result,
        },
        now: 0,
    });

    assert_eq!(
        cmdline(&app),
        "Cannot load log/following: cannot reach the server"
    );
    assert!(shows(&app, "Cannot load log/following"));
}

#[test]
fn a_device_refused_while_fetching_goes_back_to_connecting() {
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));

    let result = Err(FetchError::NotApproved);
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Following,
            after: None,
            result,
        },
        now: 0,
    });

    assert!(shows(&app, "not approved"));
}

/// A page of `count` commits named `c{first}` … with `next` after it.
fn page_of(first: usize, count: usize, next: Option<&str>) -> Page<Commit> {
    let commits = (first..first + count)
        .map(|n| {
            serde_json::json!({
                "id": format!("c{n}"), "short_hash": format!("{n:07}"), "body": "", "created_at": "2026-11-12T15:13:40Z",
                "body_segments": [{ "type": "text", "text": format!("commit {n}") }],
                "author": { "id": "u", "handle": "ada", "name": "Ada" }, "stars_count": 0, "images": []
            })
        })
        .collect::<Vec<_>>();
    Page::new(
        serde_json::from_value(serde_json::Value::Array(commits)).expect("commits"),
        next.map(Cursor::new),
    )
}

/// A connected app whose `log` buffer (shown in the window) got `page` as its first page.
fn with_log(log: Log, page: Page<Commit>) -> App {
    let mut app = App::new();
    let _ = app.update(Msg::Resize {
        width: 60,
        height: 12,
    });
    let _ = app.start();
    let _ = app.update(Msg::Net(NetEvent::Connected(me("ada", true))));
    if log != Log::Following {
        let _ = press(&mut app, &format!(":e log/{}<CR>", log.name()));
    }
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log,
            after: None,
            result: Ok(page),
        },
        now: FIXTURE_NOW,
    });
    app
}

fn fetches(effects: &[Effect]) -> Vec<Request> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Fetch(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn nearing_the_end_of_a_log_loads_the_next_page_once() {
    let mut app = with_log(Log::Following, page_of(1, 20, Some("p2")));

    assert_eq!(fetches(&press(&mut app, "j")), []);
    let effects = press(&mut app, "G");
    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Following,
            after: Some(Cursor::new("p2"))
        }]
    );
    // The notice goes after the last commit, one row past it.
    assert_eq!(fetches(&press(&mut app, "j")), []);
    assert!(shows(&app, "Loading more…"));

    let next = Fetched::Log {
        log: Log::Following,
        after: Some(Cursor::new("p2")),
        result: Ok(page_of(21, 5, None)),
    };
    let _ = app.update(Msg::Fetched {
        fetched: next,
        now: FIXTURE_NOW,
    });
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 25"));
    assert!(!shows(&app, "Loading more…"));
    assert_eq!(fetches(&press(&mut app, "k")), []);
}

#[test]
fn a_failed_next_page_is_reported_and_tried_again_on_the_next_move() {
    let mut app = with_log(Log::Following, page_of(1, 20, Some("p2")));
    let _ = press(&mut app, "G");

    let failed = Fetched::Log {
        log: Log::Following,
        after: Some(Cursor::new("p2")),
        result: Err(FetchError::Failed("timeout".into())),
    };
    let _ = app.update(Msg::Fetched {
        fetched: failed,
        now: FIXTURE_NOW,
    });

    assert_eq!(cmdline(&app), "Cannot load log/following: timeout");
    assert_eq!(fetches(&press(&mut app, "k")).len(), 1);
}

#[test]
fn refreshing_puts_new_commits_above_updates_the_rest_and_keeps_the_cursor_on_its_commit() {
    let mut app = with_log(Log::Following, page_of(5, 5, None));
    let _ = press(&mut app, "]]");
    assert!(screen(&app).rows[screen(&app).cursor.1 as usize].contains("0000006"));

    let effects = press(&mut app, ":e!<CR>");
    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Following,
            after: None
        }]
    );
    let mut fresh = page_of(3, 4, None);
    fresh.items[3].stars_count = 9;
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Following,
            after: None,
            result: Ok(fresh),
        },
        now: FIXTURE_NOW,
    });

    assert!(screen(&app).rows[screen(&app).cursor.1 as usize].contains("0000006"));
    let _ = press(&mut app, "gg");
    assert!(
        screen(&app).rows[1].contains("0000003"),
        "{:#?}",
        screen(&app).rows
    );
    // Further down, out of this small window: the updated count of commit 6, and the older commits still there.
    let _ = press(&mut app, "3]]zt");
    assert!(shows(&app, "☆ 9"));
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 9"), "{:#?}", screen(&app).rows);
}

#[test]
fn ctrl_l_refreshes_too() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));

    assert_eq!(
        fetches(&press(&mut app, "<C-l>")),
        [Request::Log {
            log: Log::Following,
            after: None
        }]
    );
}

#[test]
fn a_refresh_with_nothing_in_common_replaces_the_log() {
    let mut app = with_log(Log::Following, page_of(1, 3, Some("old")));
    let _ = press(&mut app, ":e!<CR>");

    let fresh = Fetched::Log {
        log: Log::Following,
        after: None,
        result: Ok(page_of(100, 2, Some("new"))),
    };
    let _ = app.update(Msg::Fetched {
        fetched: fresh,
        now: FIXTURE_NOW,
    });

    assert!(shows(&app, "commit 100") && !shows(&app, "0000001"));
    let effects = press(&mut app, "G");
    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Following,
            after: Some(Cursor::new("new"))
        }]
    );
}

#[test]
fn trending_is_ranked_so_a_refresh_replaces_it_in_order() {
    let mut app = with_log(Log::Trending, page_of(1, 3, None));
    let _ = press(&mut app, ":e!<CR>");

    let mut ranked = page_of(1, 3, None);
    ranked.items.reverse();
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Trending,
            after: None,
            result: Ok(ranked),
        },
        now: FIXTURE_NOW,
    });

    let _ = press(&mut app, "gg");
    assert!(screen(&app).rows[1].contains("0000003"));
}

/// The poll wake-up among `effects`, if any.
fn poll(effects: &[Effect]) -> Option<(std::time::Duration, Wake)> {
    wakes(effects)
        .into_iter()
        .find(|(after, _)| after.as_secs() >= 15)
}

#[test]
fn following_is_polled_every_15_seconds_and_every_30_without_focus() {
    let (mut app, _) = starting();
    let effects = net(&mut app, NetEvent::Connected(me("ada", true)));
    let (after, token) = poll(&effects).expect("polling starts once connected");
    assert_eq!(after.as_secs(), 15);
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Following,
            after: None,
            result: Ok(page_of(1, 3, None)),
        },
        now: 0,
    });

    let _ = press(&mut app, "jk");
    let _ = app.update(Msg::FocusLost);
    let effects = app.update(Msg::Wake(token));

    assert_eq!(
        fetches(&effects),
        [
            Request::Log {
                log: Log::Following,
                after: None
            },
            Request::UnreadCount
        ]
    );
    assert_eq!(poll(&effects).map(|(after, _)| after.as_secs()), Some(30));
}

#[test]
fn new_commits_arriving_while_reading_further_down_are_announced() {
    let mut app = with_log(Log::Following, page_of(1, 10, None));
    let _ = press(&mut app, "5]]<C-e><C-e>");

    let mut newer = page_of(90, 2, None);
    newer.items.extend(page_of(1, 10, None).items);
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Log {
            log: Log::Following,
            after: None,
            result: Ok(newer),
        },
        now: FIXTURE_NOW,
    });

    assert_eq!(cmdline(&app), "2 new commits");
}

/// The bufferline: the first row.
fn bufferline(app: &App) -> String {
    screen(app).rows[0].clone()
}

#[test]
fn leader_keys_open_the_other_logs_and_the_bufferline_lists_them() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    assert_eq!(bufferline(&app), " log/following");

    let effects = press(&mut app, "<Space>2");

    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Latest,
            after: None
        }]
    );
    assert_eq!(bufferline(&app), " log/following  log/latest");
    assert!(statusline(&app).contains("log/latest"));
    assert_eq!(
        fetches(&press(&mut app, "<Space>3")),
        [Request::Log {
            log: Log::Trending,
            after: None
        }]
    );
}

#[test]
fn going_back_to_a_buffer_returns_to_where_the_cursor_was() {
    let mut app = with_log(Log::Following, page_of(1, 5, None));
    let _ = press(&mut app, "2]]");
    let there = screen(&app).cursor;

    let _ = press(&mut app, "<Space>2<Space>1");

    assert_eq!(screen(&app).cursor, there);
    assert_eq!(fetches(&press(&mut app, "<Space>2")), []);
}

#[test]
fn ls_lists_the_buffers_and_marks_the_current_and_the_alternate_ones() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "<Space>2");

    let _ = press(&mut app, ":ls<CR>");

    let rows = screen(&app).rows;
    assert_eq!(
        rows[rows.len() - 3..],
        [
            "  1 #  \"log/following\"",
            "  2 %a \"log/latest\"",
            "Press ENTER or type command to continue"
        ]
    );
}

#[test]
fn b_bn_and_bp_switch_buffers_by_number_name_or_order() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "<Space>2<Space>3");

    let _ = press(&mut app, ":b 1<CR>");
    assert!(statusline(&app).contains("log/following"));
    let _ = press(&mut app, ":bn<CR>");
    assert!(statusline(&app).contains("log/latest"));
    let _ = press(&mut app, ":bp<CR>:bp<CR>");
    assert!(statusline(&app).contains("log/trending"));
    let _ = press(&mut app, ":b lat<CR>");
    assert!(statusline(&app).contains("log/latest"));
    let _ = press(&mut app, ":b log<CR>");
    assert_eq!(cmdline(&app), "E93: More than one match for log");
    let _ = press(&mut app, ":b nope<CR>");
    assert_eq!(cmdline(&app), "E94: No matching buffer for nope");
}

/// A commit `id` with `text` as its body, `replies` and an optional quote.
fn commit_with(id: &str, text: &str, quoted: Option<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "id": id, "short_hash": format!("{:0>7}", id.trim_start_matches('c')), "body": text, "created_at": "2026-11-12T15:13:40Z",
        "body_segments": [{ "type": "text", "text": text }], "author": { "id": "u", "handle": "ada", "name": "Ada" },
        "replies_count": 2, "images": [], "quoted_commit": quoted,
        "html_url": format!("https://dev-null.sh/@ada/commits/{id}")
    })
}

fn thread_of(ancestors: &[&str], focus: serde_json::Value) -> Box<devnull_api::model::Thread> {
    let ancestors: Vec<serde_json::Value> = ancestors
        .iter()
        .map(|id| commit_with(id, &format!("ancestor {id}"), None))
        .collect();
    serde_json::from_value(serde_json::json!({ "data": focus, "ancestors": ancestors }))
        .expect("thread")
}

#[test]
fn enter_on_a_commit_opens_its_thread() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "]]j");

    let effects = press(&mut app, "<CR>");

    assert_eq!(
        fetches(&effects),
        [
            Request::Thread { id: "c2".into() },
            Request::Replies {
                id: "c2".into(),
                after: None
            }
        ]
    );
    assert!(statusline(&app).contains("thread/c2"));
    assert!(shows(&app, "Loading thread/c2…"));
}

#[test]
fn a_thread_shows_ancestors_the_whole_commit_then_replies_and_starts_on_the_commit() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "]]<CR>");
    let long: String = (1..=8)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");

    let replies = Fetched::Replies {
        id: "c2".into(),
        after: None,
        result: Ok(page_of(7, 2, None)),
    };
    let _ = app.update(Msg::Fetched {
        fetched: replies,
        now: FIXTURE_NOW,
    });
    let thread = Fetched::Thread {
        id: "c2".into(),
        result: Ok(thread_of(&["c1"], commit_with("c2", &long, None))),
    };
    let _ = app.update(Msg::Fetched {
        fetched: thread,
        now: FIXTURE_NOW,
    });

    let cursor_row = |app: &App| screen(app).rows[screen(app).cursor.1 as usize].clone();
    assert!(
        cursor_row(&app).contains("0000002"),
        "{:#?}",
        screen(&app).rows
    );
    assert!(shows(&app, "ancestor c1"));
    let _ = press(&mut app, "]]");
    assert!(shows(&app, "line 8"), "the commit is never folded");
    assert!(
        cursor_row(&app).contains("0000007"),
        "its replies follow it"
    );
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 8"));
}

#[test]
fn enter_on_a_quote_opens_the_quoted_commit() {
    let quote = commit_with("c9", "the quoted one", None);
    let page: Page<Commit> = Page::new(
        serde_json::from_value(serde_json::json!([commit_with("c1", "look", Some(quote))]))
            .expect("page"),
        None,
    );
    let mut app = with_log(Log::Following, page);
    let _ = press(&mut app, "2j");
    assert!(screen(&app).rows[screen(&app).cursor.1 as usize].starts_with("┃ Ada @ada"));

    let effects = press(&mut app, "$<CR>");

    assert_eq!(fetches(&effects)[0], Request::Thread { id: "c9".into() });
}

#[test]
fn enter_on_a_link_opens_it_in_the_browser() {
    let commit = commit_with("c1", "see https://example.org/x", None);
    let page: Page<Commit> = Page::new(
        serde_json::from_value(serde_json::json!([commit])).expect("page"),
        None,
    );
    let mut app = with_log(Log::Following, page);

    let effects = press(&mut app, "j$<CR>");

    assert_eq!(effects, [Effect::OpenUrl("https://example.org/x".into())]);
}

#[test]
fn a_thread_that_cannot_load_says_why() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "<CR>");

    let failed = Fetched::Thread {
        id: "c1".into(),
        result: Err(FetchError::Failed("not found".into())),
    };
    let _ = app.update(Msg::Fetched {
        fetched: failed,
        now: FIXTURE_NOW,
    });

    assert_eq!(cmdline(&app), "Cannot load thread/c1: not found");
}

#[test]
fn a_thread_is_named_by_the_short_hash_dev_null_makes_of_the_id() {
    let id = "01a1d052-5c65-72dc-8937-b8409a82e820";
    let page: Page<Commit> = Page::new(
        serde_json::from_value(serde_json::json!([commit_with(id, "hi", None)])).expect("page"),
        None,
    );
    let mut app = with_log(Log::Following, page);

    let _ = press(&mut app, "<CR>");

    assert!(
        statusline(&app).contains("thread/a82e820"),
        "{}",
        statusline(&app)
    );
}

/// A connected app showing the thread of `c2` with ancestor `c1`, and the first page of its replies, `replies`.
fn in_thread(replies: Page<Commit>) -> App {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "]]<CR>");
    let thread = Fetched::Thread {
        id: "c2".into(),
        result: Ok(thread_of(&["c1"], commit_with("c2", "the commit", None))),
    };
    let _ = app.update(Msg::Fetched {
        fetched: thread,
        now: FIXTURE_NOW,
    });
    let replies = Fetched::Replies {
        id: "c2".into(),
        after: None,
        result: Ok(replies),
    };
    let _ = app.update(Msg::Fetched {
        fetched: replies,
        now: FIXTURE_NOW,
    });
    app
}

#[test]
fn nearing_the_end_of_a_thread_loads_more_replies() {
    let mut app = in_thread(page_of(10, 5, Some("r2")));

    let effects = press(&mut app, "G");

    assert_eq!(
        fetches(&effects),
        [Request::Replies {
            id: "c2".into(),
            after: Some(Cursor::new("r2"))
        }]
    );
    let more = Fetched::Replies {
        id: "c2".into(),
        after: Some(Cursor::new("r2")),
        result: Ok(page_of(15, 1, None)),
    };
    let _ = app.update(Msg::Fetched {
        fetched: more,
        now: FIXTURE_NOW,
    });
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 15"));
}

#[test]
fn refreshing_a_thread_asks_for_the_commit_and_its_replies_again() {
    let mut app = in_thread(page_of(10, 2, None));

    let effects = press(&mut app, ":e!<CR>");

    assert_eq!(
        fetches(&effects),
        [
            Request::Thread { id: "c2".into() },
            Request::Replies {
                id: "c2".into(),
                after: None
            }
        ]
    );
    assert_eq!(
        fetches(&press(&mut app, ":e!<CR>")),
        [],
        "not again while on their way"
    );
    let edited = Fetched::Thread {
        id: "c2".into(),
        result: Ok(thread_of(&["c1"], commit_with("c2", "edited", None))),
    };
    let _ = app.update(Msg::Fetched {
        fetched: edited,
        now: FIXTURE_NOW,
    });
    assert!(shows(&app, "edited") && !shows(&app, "the commit"));
    assert!(shows(&app, "ancestor c1"));
    let replies: Page<Commit> = Page::new(
        serde_json::from_value(serde_json::json!([commit_with(
            "c10",
            "edited reply",
            None
        )]))
        .expect("replies"),
        None,
    );
    let replies = Fetched::Replies {
        id: "c2".into(),
        after: None,
        result: Ok(replies),
    };
    let _ = app.update(Msg::Fetched {
        fetched: replies,
        now: FIXTURE_NOW,
    });
    let _ = press(&mut app, "G");
    assert!(shows(&app, "edited reply") && !shows(&app, "commit 10"));
}

#[test]
fn a_refreshed_thread_shows_what_the_commit_replies_to_now() {
    let mut app = in_thread(page_of(10, 2, None));
    let _ = press(&mut app, ":e!<CR>");

    let edited = Fetched::Thread {
        id: "c2".into(),
        result: Ok(thread_of(&["c0"], commit_with("c2", "the commit", None))),
    };
    let _ = app.update(Msg::Fetched {
        fetched: edited,
        now: FIXTURE_NOW,
    });

    let _ = press(&mut app, "gg");
    assert!(shows(&app, "ancestor c0") && !shows(&app, "ancestor c1"));
    // Row 0 is the bufferline.
    assert!(screen(&app).rows[1].contains("0000000"));
}

#[test]
fn replies_without_their_commit_say_why_it_is_missing() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "<CR>");
    let replies = Fetched::Replies {
        id: "c1".into(),
        after: None,
        result: Ok(page_of(10, 1, None)),
    };
    let _ = app.update(Msg::Fetched {
        fetched: replies,
        now: FIXTURE_NOW,
    });
    assert!(shows(&app, "Loading thread/c1…") && shows(&app, "commit 10"));

    let failed = Fetched::Thread {
        id: "c1".into(),
        result: Err(FetchError::Failed("not found".into())),
    };
    let _ = app.update(Msg::Fetched {
        fetched: failed,
        now: FIXTURE_NOW,
    });

    assert!(!shows(&app, "Loading thread/c1…"));
    assert!(screen(&app).rows[1].starts_with("Cannot load thread/c1: not found"));
    assert!(shows(&app, "commit 10"));
}

#[test]
fn a_thread_opened_again_is_not_asked_for_again() {
    let mut app = in_thread(page_of(10, 2, None));
    let _ = press(&mut app, ":b log/following<CR>");

    // Back where the window left the log: on c2.
    let effects = press(&mut app, "<CR>");

    assert_eq!(fetches(&effects), []);
    assert!(statusline(&app).contains("thread/c2"));
    assert_eq!(
        press(&mut app, "<CR>"),
        [],
        "the thread's own commit is where the window is already"
    );
}

#[test]
fn a_thread_on_its_way_is_not_asked_for_twice() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "<CR>:b log/following<CR>");

    let effects = press(&mut app, "<CR>");

    assert_eq!(fetches(&effects), []);
}

#[test]
fn refreshing_a_thread_keeps_the_folds_opened_in_it() {
    let long: String = (1..=8)
        .map(|n| format!("old {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let thread = || {
        let ancestor = commit_with("c1", &long, None);
        serde_json::from_value(serde_json::json!({ "data": commit_with("c2", "the commit", None), "ancestors": [ancestor] }))
            .map(Box::new)
            .expect("thread")
    };
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "]]<CR>");
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Thread {
            id: "c2".into(),
            result: Ok(thread()),
        },
        now: FIXTURE_NOW,
    });
    // The header, six lines, then the fold.
    let _ = press(&mut app, "gg7jzo");
    assert!(shows(&app, "old 8"));

    let _ = press(&mut app, ":e!<CR>");
    let _ = app.update(Msg::Fetched {
        fetched: Fetched::Thread {
            id: "c2".into(),
            result: Ok(thread()),
        },
        now: FIXTURE_NOW,
    });

    assert!(shows(&app, "old 8"));
}

/// A log page of one commit by `@ada` saying "thanks @lin", mentioning the person `@lin`.
fn mentioning_lin() -> Page<Commit> {
    let mut commit = commit_with("c1", "thanks @lin", None);
    commit["mentions"] =
        serde_json::json!([{ "handle": "lin", "kind": "user", "offset": 7, "length": 4 }]);
    Page::new(
        serde_json::from_value(serde_json::json!([commit])).expect("page"),
        None,
    )
}

/// `@lin`'s profile: three commits, two followers, one following, and `c9` pinned.
fn lin() -> devnull_api::model::AccountPage {
    let profile = serde_json::from_value(serde_json::json!({
        "data": { "id": "u2", "handle": "lin", "name": "Lin", "commits_count": 3, "followers_count": 2, "following_count": 1 },
        "pinned_commit": commit_with("c9", "pinned words", None)
    }))
    .expect("profile");
    devnull_api::model::AccountPage::Person(Box::new(profile))
}

fn arrive(app: &mut App, fetched: Fetched) -> Vec<Effect> {
    app.update(Msg::Fetched {
        fetched,
        now: FIXTURE_NOW,
    })
}

#[test]
fn enter_on_a_mention_opens_the_profile_with_its_commits() {
    let mut app = with_log(Log::Following, mentioning_lin());

    let effects = press(&mut app, "j$<CR>");

    assert_eq!(
        fetches(&effects),
        [
            Request::Profile {
                handle: "lin".into(),
                kind: Some(AccountKind::User)
            },
            Request::Commits {
                handle: "lin".into(),
                after: None
            }
        ]
    );
    assert!(statusline(&app).contains("@lin"));
    assert!(shows(&app, "Loading @lin…"));
}

#[test]
fn a_profile_shows_who_it_is_its_pinned_commit_then_its_commits() {
    let mut app = with_log(Log::Following, mentioning_lin());
    let _ = press(&mut app, "j$<CR>");

    let _ = arrive(
        &mut app,
        Fetched::Commits {
            handle: "lin".into(),
            after: None,
            result: Ok(page_of(10, 2, None)),
        },
    );
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );

    let rows = screen(&app).rows;
    assert!(rows[1].starts_with("Lin @lin"), "{rows:#?}");
    // The pin is two cells wide: the screen has a blank cell after it.
    assert!(
        shows(&app, "📌  Pinned") && shows(&app, "pinned words"),
        "{rows:#?}"
    );
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 11"));
}

#[test]
fn a_handle_typed_in_is_asked_for_first_and_its_commits_once_its_kind_is_known() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, ":e @lin<CR>");
    assert_eq!(
        fetches(&effects),
        [Request::Profile {
            handle: "lin".into(),
            kind: None
        }]
    );

    let effects = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );

    assert_eq!(
        fetches(&effects),
        [Request::Commits {
            handle: "lin".into(),
            after: None
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Commits {
            handle: "lin".into(),
            after: None,
            result: Ok(page_of(10, 1, None)),
        },
    );
    assert!(
        screen(&app).rows[1].starts_with("Lin @lin"),
        "the profile stays above its commits"
    );
    let _ = press(&mut app, "G");
    assert!(shows(&app, "commit 10"));
    let _ = press(&mut app, ":b log/following<CR>");
    assert_eq!(
        fetches(&press(&mut app, ":profile @lin<CR>")),
        [],
        "the same buffer"
    );
    assert!(statusline(&app).contains("@lin"));
}

#[test]
fn a_profile_s_counts_lead_to_its_lists_of_people_and_they_to_their_profiles() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, ":e @lin<CR>");
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );

    let effects = press(&mut app, "/followers<CR><CR>");

    assert_eq!(
        fetches(&effects),
        [Request::People {
            handle: "lin".into(),
            which: People::Followers,
            after: None
        }]
    );
    assert!(statusline(&app).contains("@lin/followers"));
    let kim = serde_json::json!([{ "id": "u3", "handle": "kim", "name": "Kim", "bio": "hi", "follows_you": true }]);
    let page = Page::new(serde_json::from_value(kim).expect("accounts"), None);
    let _ = arrive(
        &mut app,
        Fetched::People {
            handle: "lin".into(),
            which: People::Followers,
            after: None,
            result: Ok(page),
        },
    );
    assert!(shows(&app, "Kim @kim · follows you"));
    let effects = press(&mut app, "<CR>");
    assert_eq!(
        fetches(&effects),
        [Request::Profile {
            handle: "kim".into(),
            kind: None
        }]
    );
}

#[test]
fn gd_opens_the_profile_of_an_organization_and_it_leads_to_its_mentions() {
    let mut commit = commit_with("c1", "we shipped", None);
    commit["author"] =
        serde_json::json!({ "id": "o1", "handle": "acme", "name": "Acme", "type": "organization" });
    let page = Page::new(
        serde_json::from_value(serde_json::json!([commit])).expect("page"),
        None,
    );
    let mut app = with_log(Log::Following, page);

    let effects = press(&mut app, "jgd");

    assert_eq!(
        fetches(&effects),
        [
            Request::Profile {
                handle: "acme".into(),
                kind: Some(AccountKind::Organization)
            },
            Request::OrganizationCommits {
                handle: "acme".into(),
                tab: OrganizationTab::Commits,
                after: None
            }
        ]
    );
    let org = serde_json::from_value(
        serde_json::json!({ "data": { "handle": "acme", "name": "Acme", "followers_count": 5 } }),
    )
    .expect("organization");
    let org = devnull_api::model::AccountPage::Organization(Box::new(org));
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "acme".into(),
            result: Ok(org),
        },
    );
    assert!(shows(&app, "Acme (org) @acme"));
    let effects = press(&mut app, "/mentions<CR><CR>");
    assert_eq!(
        fetches(&effects),
        [Request::OrganizationCommits {
            handle: "acme".into(),
            tab: OrganizationTab::Mentions,
            after: None
        }]
    );
    assert!(statusline(&app).contains("@acme/mentions"));
    let _ = arrive(
        &mut app,
        Fetched::OrganizationCommits {
            handle: "acme".into(),
            tab: OrganizationTab::Mentions,
            after: None,
            result: Ok(page_of(20, 1, None)),
        },
    );
    assert!(shows(&app, "commit 20"));
}

#[test]
fn gd_on_a_mention_opens_the_mentioned_and_elsewhere_the_author() {
    let mut app = with_log(Log::Following, mentioning_lin());

    assert_eq!(
        fetches(&press(&mut app, "j$gd"))[0],
        Request::Profile {
            handle: "lin".into(),
            kind: Some(AccountKind::User)
        }
    );
    let _ = press(&mut app, ":b log/following<CR>");
    assert_eq!(
        fetches(&press(&mut app, "0gd"))[0],
        Request::Profile {
            handle: "ada".into(),
            kind: Some(AccountKind::User)
        }
    );
}

#[test]
fn profile_without_an_account_under_the_cursor_says_so() {
    let mut app = with_log(Log::Following, page_of(1, 0, None));
    assert!(shows(&app, "Nothing here yet."));

    let _ = press(&mut app, "gd");

    assert_eq!(cmdline(&app), "No account under the cursor");
}

#[test]
fn e_profile_opens_your_own() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, "<Space>p");

    assert_eq!(
        fetches(&effects),
        [
            Request::Profile {
                handle: "ada".into(),
                kind: Some(AccountKind::User)
            },
            Request::Commits {
                handle: "ada".into(),
                after: None
            }
        ]
    );
    assert!(statusline(&app).contains("@ada"));
}

#[test]
fn an_account_that_does_not_exist_says_so() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, ":e @nobody<CR>");

    let effects = arrive(
        &mut app,
        Fetched::Profile {
            handle: "nobody".into(),
            result: Err(FetchError::Failed("not found".into())),
        },
    );

    assert_eq!(cmdline(&app), "Cannot load @nobody: not found");
    assert_eq!(fetches(&effects), [], "not asked for again until :e!");
}

#[test]
fn new_people_in_a_list_are_not_announced_as_commits() {
    let accounts = |handles: std::ops::Range<usize>| {
        let accounts: Vec<serde_json::Value> = handles
            .map(|n| serde_json::json!({ "id": format!("u{n}"), "handle": format!("p{n}"), "name": "P" }))
            .collect();
        Page::new(
            serde_json::from_value(serde_json::Value::Array(accounts)).expect("accounts"),
            None,
        )
    };
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, ":e @lin/followers<CR>");
    let _ = arrive(
        &mut app,
        Fetched::People {
            handle: "lin".into(),
            which: People::Followers,
            after: None,
            result: Ok(accounts(1..20)),
        },
    );
    let _ = press(&mut app, "G:e!<CR>");

    let _ = arrive(
        &mut app,
        Fetched::People {
            handle: "lin".into(),
            which: People::Followers,
            after: None,
            result: Ok(accounts(0..20)),
        },
    );

    assert_eq!(cmdline(&app), "");
    let _ = press(&mut app, "gg");
    assert!(shows(&app, "P @p0"), "new followers go above");
}

/// The effects but the wake-ups that prefix keys such as `g` ask for (which-key).
fn without_wakes(effects: Vec<Effect>) -> Vec<Effect> {
    effects
        .into_iter()
        .filter(|effect| !matches!(effect, Effect::Wake { .. }))
        .collect()
}

/// A log page of one commit, `c1`, with `text` and `extra` fields.
fn one_commit(text: &str, extra: serde_json::Value) -> Page<Commit> {
    let mut commit = commit_with("c1", text, None);
    if let (Some(commit), Some(extra)) = (commit.as_object_mut(), extra.as_object()) {
        commit.extend(extra.clone());
    }
    Page::new(
        serde_json::from_value(serde_json::json!([commit])).expect("page"),
        None,
    )
}

#[test]
fn gx_opens_the_link_under_the_cursor_and_nothing_else() {
    let mut app = with_log(
        Log::Following,
        one_commit("see https://example.org/x", serde_json::json!({})),
    );

    assert_eq!(
        without_wakes(press(&mut app, "j$gx")),
        [Effect::OpenUrl("https://example.org/x".into())]
    );
    assert_eq!(without_wakes(press(&mut app, "0gx")), []);
    assert_eq!(cmdline(&app), "No link under the cursor");
}

#[test]
fn big_gx_opens_the_commit_under_the_cursor_on_the_web_even_from_a_link_in_it() {
    let quote = commit_with("c9", "quoted", None);
    let mut app = with_log(
        Log::Following,
        one_commit(
            "see https://example.org/x",
            serde_json::json!({ "quoted_commit": quote }),
        ),
    );

    assert_eq!(
        without_wakes(press(&mut app, "j$gX")),
        [Effect::OpenUrl(
            "https://dev-null.sh/@ada/commits/c1".into()
        )]
    );
    assert_eq!(
        without_wakes(press(&mut app, "jgX")),
        [Effect::OpenUrl(
            "https://dev-null.sh/@ada/commits/c9".into()
        )],
        "a quote's own"
    );
}

#[test]
fn gy_yanks_the_commit_s_link() {
    let mut app = with_log(Log::Following, one_commit("hello", serde_json::json!({})));

    let effects = without_wakes(press(&mut app, "jgy"));

    assert_eq!(
        effects,
        [Effect::Copy("https://dev-null.sh/@ada/commits/c1".into())]
    );
    assert_eq!(cmdline(&app), "Yanked https://dev-null.sh/@ada/commits/c1");
}

#[test]
fn big_gx_in_a_profile_opens_the_account_on_the_web() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, ":e @lin<CR>");
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );

    assert_eq!(
        without_wakes(press(&mut app, "ggjgX")),
        [Effect::OpenUrl("https://dev-null.sh/@lin".into())]
    );
}

#[test]
fn k_on_a_mention_shows_the_account_s_card_until_the_next_key() {
    let mut app = with_log(Log::Following, mentioning_lin());

    let effects = press(&mut app, "j$K");

    assert_eq!(
        fetches(&effects),
        [Request::Profile {
            handle: "lin".into(),
            kind: Some(AccountKind::User)
        }]
    );
    assert!(shows(&app, "Loading @lin…"));
    assert!(
        statusline(&app).contains("log/following"),
        "the window stays where it is"
    );
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );
    let rows = screen(&app).rows;
    let counts = rows
        .iter()
        .position(|row| row.contains("│3 commits · 2 followers · 1 following"))
        .expect("counts");
    assert!(rows[counts - 1].contains("│Lin @lin"), "{rows:#?}");
    assert!(
        rows[counts + 1].contains('└'),
        "no blank row closing it: {rows:#?}"
    );
    let _ = press(&mut app, "k");
    assert!(!shows(&app, "Lin @lin"));
}

#[test]
fn k_elsewhere_in_a_commit_shows_its_author_s_card() {
    let mut app = with_log(Log::Following, one_commit("hello", serde_json::json!({})));

    let effects = press(&mut app, "jK");

    assert_eq!(
        fetches(&effects),
        [Request::Profile {
            handle: "ada".into(),
            kind: Some(AccountKind::User)
        }]
    );
}

#[test]
fn k_on_a_link_card_shows_it_whole() {
    let card = serde_json::json!({ "url": "https://example.org/post", "title": "A post", "site_name": "Example",
                                   "description": "What the post says", "is_pending": false });
    let mut app = with_log(
        Log::Following,
        one_commit("look", serde_json::json!({ "link_preview": card })),
    );
    let _ = press(&mut app, "2j");

    let effects = press(&mut app, "K");

    assert_eq!(fetches(&effects), []);
    for text in [
        "A post",
        "Example",
        "What the post says",
        "https://example.org/post",
    ] {
        assert!(shows(&app, text), "{text}: {:#?}", screen(&app).rows);
    }
    assert_eq!(
        without_wakes(press(&mut app, "gx")),
        [Effect::OpenUrl("https://example.org/post".into())],
        "a link card is a link"
    );
    assert_eq!(
        press(&mut app, "<CR>"),
        [Effect::OpenUrl("https://example.org/post".into())],
        "the card still opens"
    );
}

#[test]
fn k_with_nothing_to_tell_says_so() {
    let mut app = with_log(Log::Following, page_of(1, 0, None));

    let _ = press(&mut app, "K");

    assert_eq!(cmdline(&app), "E349: No identifier under cursor");
}

#[test]
fn a_card_with_no_room_below_the_cursor_goes_above_it() {
    let mut app = with_log(Log::Following, page_of(1, 3, None));
    let _ = press(&mut app, "G");
    let cursor_y = screen(&app).cursor.1 as usize;

    let _ = press(&mut app, "K");
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "ada".into(),
            result: Ok(lin()),
        },
    );

    let rows = screen(&app).rows;
    let card = rows
        .iter()
        .position(|row| row.contains("│Lin @lin"))
        .expect("card");
    assert!(card < cursor_y, "{rows:#?}");
}

#[test]
fn a_card_whose_profile_cannot_load_goes_and_says_why() {
    let mut app = with_log(Log::Following, mentioning_lin());
    let _ = press(&mut app, "j$K");

    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Err(FetchError::Failed("not found".into())),
        },
    );

    assert!(!shows(&app, "Loading @lin…"));
    assert_eq!(cmdline(&app), "Cannot load @lin: not found");
}

#[test]
fn a_card_waits_for_its_own_account() {
    let mut app = with_log(Log::Following, mentioning_lin());
    let _ = press(&mut app, "j$K");

    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "kim".into(),
            result: Ok(lin()),
        },
    );

    assert!(shows(&app, "Loading @lin…"));
}

#[test]
fn a_profile_that_cannot_load_is_reported_once_with_a_card_waiting_for_it() {
    let mut app = with_log(Log::Following, mentioning_lin());
    // Back on the log, the cursor is where it was: on @lin.
    let _ = press(&mut app, "j$<CR>:b log/following<CR>K");
    assert!(shows(&app, "Loading @lin…"));

    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Err(FetchError::Failed("not found".into())),
        },
    );
    let _ = press(&mut app, ":messages<CR>");

    let reported = screen(&app)
        .rows
        .iter()
        .filter(|row| row.contains("Cannot load @lin"))
        .count();
    assert_eq!(reported, 1, "{:#?}", screen(&app).rows);
}

fn cursor_row(app: &App) -> String {
    let screen = screen(app);
    screen.rows[screen.cursor.1 as usize].clone()
}

#[test]
fn ctrl_o_from_a_thread_returns_to_where_enter_was_pressed_and_ctrl_i_goes_back() {
    let mut app = with_log(Log::Following, page_of(1, 5, None));
    let _ = press(&mut app, "2]]j<CR>");
    assert!(statusline(&app).contains("thread/c3"));

    let effects = press(&mut app, "<C-o>");

    assert_eq!(fetches(&effects), []);
    assert!(statusline(&app).contains("log/following"));
    assert_eq!(cursor_row(&app), "commit 3");
    let _ = press(&mut app, "<C-i>");
    assert!(statusline(&app).contains("thread/c3"));
}

#[test]
fn the_jumplist_crosses_buffers_and_jumps_within_them() {
    let mut app = with_log(Log::Following, page_of(1, 5, None));
    let _ = press(&mut app, "G<Space>2");
    let latest = Fetched::Log {
        log: Log::Latest,
        after: None,
        result: Ok(page_of(10, 5, None)),
    };
    let _ = arrive(&mut app, latest);
    let _ = press(&mut app, "G");

    let _ = press(&mut app, "<C-o>");
    assert!(
        statusline(&app).contains("log/latest") && cursor_row(&app).starts_with("Ada @ada"),
        "back to the top of latest"
    );
    let _ = press(&mut app, "<C-o>");
    assert!(
        statusline(&app).contains("log/following") && shows(&app, "commit 5"),
        "back to the end of following, where <Space>2 left it"
    );
    let _ = press(&mut app, "<C-o>");
    assert!(
        statusline(&app).contains("log/following") && cursor_row(&app).contains("0000001"),
        "the top of following"
    );
    let _ = press(&mut app, "3<C-i>");
    assert!(statusline(&app).contains("log/latest"));
}

#[test]
fn jumps_never_lead_back_to_the_connect_page() {
    let mut app = started();

    let _ = press(&mut app, "<C-o>");

    assert!(statusline(&app).contains("log/following"));
}

#[test]
fn ctrl_t_returns_from_what_enter_opened_however_far_one_went_there() {
    let mut app = with_log(Log::Following, page_of(1, 5, None));
    let _ = press(&mut app, "2]]j<CR>");
    let replies = Fetched::Replies {
        id: "c3".into(),
        after: None,
        result: Ok(page_of(10, 5, None)),
    };
    let _ = arrive(&mut app, replies);
    let thread = Fetched::Thread {
        id: "c3".into(),
        result: Ok(thread_of(&[], commit_with("c3", "the commit", None))),
    };
    let _ = arrive(&mut app, thread);
    // <CR> on the thread's own commit stays in the thread: nothing to come back from.
    let _ = press(&mut app, "<CR>Ggg");

    let _ = press(&mut app, "<C-t>");

    assert!(statusline(&app).contains("log/following"));
    assert_eq!(cursor_row(&app), "commit 3");
    let _ = press(&mut app, "<C-o>");
    assert!(
        statusline(&app).contains("thread/c3") && cursor_row(&app).contains("0000003"),
        "<C-t> is a jump too: back to the top of the thread, where it was pressed"
    );
    let _ = press(&mut app, "<C-t>");
    assert_eq!(cmdline(&app), "E73: Tag stack empty");
}

#[test]
fn a_jump_back_to_a_buffer_that_could_not_load_asks_for_it_again() {
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));
    let failed = Fetched::Log {
        log: Log::Following,
        after: None,
        result: Err(FetchError::Failed("cannot reach the server".into())),
    };
    let _ = arrive(&mut app, failed);
    let _ = press(&mut app, "<Space>2");

    let effects = press(&mut app, "<C-o>");

    assert!(statusline(&app).contains("log/following"));
    assert_eq!(
        fetches(&effects),
        [Request::Log {
            log: Log::Following,
            after: None
        }]
    );
}

#[test]
fn ctrl_t_returns_from_a_profile_gd_opened() {
    let mut app = with_log(Log::Following, mentioning_lin());
    let _ = press(&mut app, "j$gd");
    assert!(statusline(&app).contains("@lin"));

    let _ = press(&mut app, ":pop<CR>");

    assert!(statusline(&app).contains("log/following"));
    assert_eq!(screen(&app).cursor.1, 2);
}

#[test]
fn after_a_jump_the_view_stays_put_until_the_cursor_leaves_it() {
    let mut app = with_log(Log::Following, page_of(1, 5, None));

    let _ = press(&mut app, "G<C-o>j");

    // Row 0 is the bufferline: the cursor is on the window's second row, the view still at the top.
    assert_eq!(screen(&app).cursor.1, 2);
}

#[test]
fn tab_goes_forward_like_ctrl_i_as_terminals_without_the_kitty_protocol_send_it() {
    let mut app = app_with(&["one", "two", "three", "four", "five"], 30, 10);

    let _ = press(&mut app, "G<C-o><Tab>");

    assert_eq!(screen(&app).cursor, (0, 5));
}

/// Connected, with following, latest and trending opened in that order: trending is shown.
fn three_logs() -> App {
    let mut app = with_log(Log::Following, page_of(1, 2, None));
    let _ = press(&mut app, "<Space>2<Space>3");
    app
}

#[test]
fn the_buffer_picker_lists_the_buffers_last_shown_first_and_starts_on_the_one_before() {
    let mut app = three_logs();

    let _ = press(&mut app, "<Space><Space>");

    let rows = screen(&app).rows;
    // Inside the picker's border: the bufferline names them all too.
    let at = |name: &str| {
        rows.iter()
            .position(|row| row.contains('│') && row.contains(name))
            .expect(name)
    };
    assert!(
        at("log/trending") < at("log/latest") && at("log/latest") < at("log/following"),
        "{rows:#?}"
    );
    let _ = press(&mut app, "<CR>");
    assert!(statusline(&app).contains("log/latest"));
    assert!(!shows(&app, "> "), "the picker is gone");
}

#[test]
fn typing_in_the_buffer_picker_narrows_it_fuzzily() {
    let mut app = three_logs();

    let _ = press(&mut app, "<Space><Space>lgfl");

    assert!(shows(&app, "> lgfl"));
    let _ = press(&mut app, "<CR>");
    assert!(statusline(&app).contains("log/following"));
}

#[test]
fn the_picker_moves_with_ctrl_n_and_ctrl_p_and_escape_leaves_it() {
    let mut app = three_logs();

    let _ = press(&mut app, "<Space><Space><C-n><C-p><C-n><CR>");
    assert!(statusline(&app).contains("log/following"));

    let _ = press(&mut app, "<Space><Space><Esc>");
    assert!(statusline(&app).contains("log/following"));
    assert!(!shows(&app, "> "));
}

#[test]
fn the_user_picker_searches_once_typing_pauses_and_opens_who_is_picked() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, "<Space>/");
    assert!(shows(&app, "Type to find people and organizations"));

    let first = press(&mut app, "a");
    let second = press(&mut app, "d");
    assert_eq!(fetches(&second), []);
    let stale = wakes(&first).last().expect("a wake-up").1;
    let (after, token) = *wakes(&second).last().expect("a wake-up");
    assert_eq!(after, std::time::Duration::from_millis(120));
    assert_eq!(app.update(Msg::Wake(stale)), []);
    let effects = app.update(Msg::Wake(token));
    assert_eq!(
        fetches(&effects),
        [Request::SearchProfiles { term: "ad".into() }]
    );

    let found = serde_json::from_value(serde_json::json!({
        "users": [{ "id": "u1", "handle": "ada", "name": "Ada" }],
        "organizations": [{ "handle": "adacorp", "name": "Ada Corp" }]
    }))
    .expect("found");
    let _ = arrive(
        &mut app,
        Fetched::SearchProfiles {
            term: "ad".into(),
            result: Ok(found),
        },
    );
    assert!(shows(&app, "Ada @ada") && shows(&app, "Ada Corp (org) @adacorp"));
    let effects = press(&mut app, "<C-n><CR>");
    assert_eq!(
        fetches(&effects)[0],
        Request::Profile {
            handle: "adacorp".into(),
            kind: Some(AccountKind::Organization)
        }
    );
}

#[test]
fn answers_to_an_older_search_are_ignored() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, "<Space>/ad");

    let found = serde_json::from_value(
        serde_json::json!({ "users": [{ "id": "u1", "handle": "a", "name": "Old" }] }),
    )
    .expect("found");
    let _ = arrive(
        &mut app,
        Fetched::SearchProfiles {
            term: "a".into(),
            result: Ok(found),
        },
    );

    assert!(!shows(&app, "Old @a"));
}

#[test]
fn the_best_match_comes_first_whatever_was_shown_last() {
    let mut app = three_logs();
    let _ = press(&mut app, ":e @lin<CR>:b log/following<CR>");

    // log/following, shown last, matches `li` too, but scattered.
    let _ = press(&mut app, "<Space><Space>li<CR>");

    assert!(statusline(&app).contains("@lin"));
}

#[test]
fn backspace_in_an_empty_picker_leaves_it_open() {
    let mut app = three_logs();

    let _ = press(&mut app, "<Space><Space><BS>");

    assert!(shows(&app, "│> "));
}

#[test]
fn buffers_jumped_to_count_as_shown_last() {
    let mut app = three_logs();
    let _ = press(&mut app, "<C-o><C-o>");
    assert!(statusline(&app).contains("log/following"));

    let _ = press(&mut app, "<Space><Space><CR>");

    assert!(statusline(&app).contains("log/latest"));
}

#[test]
fn leader_s_opens_the_starred_commits() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, "<Space>s");

    assert_eq!(fetches(&effects), [Request::Starred { after: None }]);
    assert!(statusline(&app).contains("starred"));
    let _ = arrive(
        &mut app,
        Fetched::Starred {
            after: None,
            result: Ok(page_of(5, 2, None)),
        },
    );
    assert!(shows(&app, "commit 5") && shows(&app, "commit 6"));
}

#[test]
fn the_statusline_counts_unread_notifications_and_polling_keeps_it_current() {
    let (mut app, _) = starting();
    let mut ada = me("ada", true);
    ada.unread_notifications = 3;

    let effects = net(&mut app, NetEvent::Connected(ada));

    assert!(
        statusline(&app).contains("󰂚 3  @ada"),
        "{}",
        statusline(&app)
    );
    let (_, token) = poll(&effects).expect("polling");
    let _ = app.update(Msg::Wake(token));
    let _ = arrive(&mut app, Fetched::UnreadCount { result: Ok(0) });
    assert!(
        !statusline(&app).contains("󰂚"),
        "nothing unread, nothing shown"
    );
}

/// An inbox page: Lin starred c1 (unread), Kim replied with r1 (read before this visit), Bo followed (unread).
fn inbox() -> devnull_api::model::Inbox {
    let commit = |id: &str, text: &str| commit_with(id, text, None);
    let notifications: Vec<devnull_api::model::Notification> = serde_json::from_value(serde_json::json!([
        { "id": "n1", "group_id": "g1", "type": "starred", "created_at": "2026-11-12T15:13:40Z", "read_at": null,
          "actor": { "id": "u2", "handle": "lin", "name": "Lin" }, "actors_count": 1, "commit": commit("c1", "my words") },
        { "id": "n2", "group_id": "g2", "type": "replied", "created_at": "2026-11-12T15:13:40Z",
          "read_at": "2026-11-11T10:00:00Z", "actor": { "id": "u3", "handle": "kim", "name": "Kim" }, "actors_count": 1,
          "commit": commit("r1", "nice one") },
        { "id": "n3", "group_id": "g3", "type": "followed", "created_at": "2026-11-12T15:13:40Z",
          "read_at": "2026-11-12T15:13:40Z", "actor": { "id": "u4", "handle": "bo", "name": "Bo" }, "actors_count": 1 }
    ]))
    .expect("notifications");
    devnull_api::model::Inbox::new(Page::new(notifications, None), "2026-11-12T15:13:40Z")
}

fn with_notifications() -> App {
    let (mut app, _) = starting();
    let mut ada = me("ada", true);
    ada.unread_notifications = 2;
    let _ = net(&mut app, NetEvent::Connected(ada));
    let _ = press(&mut app, "<Space>n");
    let _ = arrive(
        &mut app,
        Fetched::Notifications {
            only: Vec::new(),
            after: None,
            result: Ok(inbox()),
        },
    );
    app
}

#[test]
fn leader_n_opens_the_notifications_marking_those_new_since_the_last_visit() {
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));

    let effects = press(&mut app, "<Space>n");
    assert_eq!(
        fetches(&effects),
        [Request::Notifications {
            only: Vec::new(),
            after: None
        }]
    );
    assert!(statusline(&app).contains("notifications"));

    let app = with_notifications();
    assert!(shows(&app, "● ★ Lin @lin starred your commit"));
    assert!(
        shows(&app, "  ↩ Kim @kim replied to your commit"),
        "read before: no mark"
    );
    assert!(
        shows(&app, "● + Bo @bo followed you"),
        "read by this visit: new"
    );
}

#[test]
fn reading_the_notifications_clears_the_unread_count() {
    let app = with_notifications();

    assert!(!statusline(&app).contains("󰂚"), "{}", statusline(&app));
}

#[test]
fn enter_on_a_notification_opens_its_commit_s_thread_or_who_followed() {
    let mut app = with_notifications();

    let effects = press(&mut app, "/nice<CR><CR>");
    assert_eq!(fetches(&effects)[0], Request::Thread { id: "r1".into() });
    let _ = press(&mut app, "<C-o>");
    let effects = press(&mut app, "/followed<CR><CR>");
    assert_eq!(
        fetches(&effects)[0],
        Request::Profile {
            handle: "bo".into(),
            kind: Some(AccountKind::User)
        }
    );
}

#[test]
fn notifications_are_filtered_by_their_buffer_s_name() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, ":e notifications/mentions,replies<CR>");

    assert_eq!(
        fetches(&effects),
        [Request::Notifications {
            only: vec![Category::Replies, Category::Mentions],
            after: None
        }]
    );
    assert!(statusline(&app).contains("notifications/replies,mentions"));
    let _ = press(&mut app, ":e notifications/gossip<CR>");
    assert_eq!(
        cmdline(&app),
        "E94: No matching buffer for notifications/gossip"
    );
}

#[test]
fn notifications_are_not_polled_as_reading_them_marks_them_read() {
    let (mut app, _) = starting();
    let effects = net(&mut app, NetEvent::Connected(me("ada", true)));
    let (_, token) = poll(&effects).expect("polling");
    let _ = press(&mut app, "<Space>n");
    let _ = arrive(
        &mut app,
        Fetched::Notifications {
            only: Vec::new(),
            after: None,
            result: Ok(inbox()),
        },
    );

    let effects = app.update(Msg::Wake(token));

    assert!(
        !fetches(&effects)
            .iter()
            .any(|request| matches!(request, Request::Notifications { .. })),
        "{effects:?}"
    );
}

#[test]
fn a_refreshed_inbox_shows_groups_that_moved_up_and_what_came_after_them() {
    let mut app = with_notifications();
    let _ = press(&mut app, ":e!<CR>");
    let mut refreshed = inbox();
    // Kim's reply group grew, so it is first now; Zed's follow came after it.
    let zed: devnull_api::model::Notification = serde_json::from_value(serde_json::json!({
        "id": "n4", "group_id": "g4", "type": "followed", "created_at": "2026-11-12T15:13:40Z",
        "actor": { "id": "u5", "handle": "zed", "name": "Zed" }, "actors_count": 1
    }))
    .expect("notification");
    let items = &mut refreshed.page.items;
    items.swap(0, 1);
    items.insert(1, zed);

    let _ = arrive(
        &mut app,
        Fetched::Notifications {
            only: Vec::new(),
            after: None,
            result: Ok(refreshed),
        },
    );

    let _ = press(&mut app, "gg");
    assert!(
        shows(&app, "Zed @zed followed you"),
        "{:#?}",
        screen(&app).rows
    );
    let rows = screen(&app).rows;
    let at = |text: &str| rows.iter().position(|row| row.contains(text)).expect(text);
    assert!(
        at("Kim @kim") < at("Zed @zed") && at("Zed @zed") < at("Lin @lin"),
        "in the server's order"
    );
}

/// `commit` as JSON, starred or not, with `count` stars.
fn starred_as(id: &str, starred: bool, count: u64) -> devnull_api::model::Commit {
    let mut commit = commit_with(id, "hello", None);
    commit["starred"] = starred.into();
    commit["stars_count"] = count.into();
    serde_json::from_value(commit).expect("commit")
}

#[test]
fn s_stars_the_commit_under_the_cursor_and_shows_the_server_s_count() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));

    let effects = press(&mut app, "js");

    assert_eq!(
        fetches(&effects),
        [Request::Star {
            id: "c1".into(),
            on: true
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Star {
            id: "c1".into(),
            on: true,
            result: Ok(Box::new(starred_as("c1", true, 1))),
        },
    );
    assert!(shows(&app, "★ 1"), "{:#?}", screen(&app).rows);
    let effects = press(&mut app, "s");
    assert_eq!(
        fetches(&effects),
        [Request::Star {
            id: "c1".into(),
            on: false
        }],
        "a starred commit is unstarred"
    );
}

#[test]
fn a_star_shows_wherever_the_commit_is() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));
    let _ = press(&mut app, "<CR>");
    let thread = Fetched::Thread {
        id: "c1".into(),
        result: Ok(thread_of(&[], commit_with("c1", "hello", None))),
    };
    let _ = arrive(&mut app, thread);

    let _ = press(&mut app, "s");
    let _ = arrive(
        &mut app,
        Fetched::Star {
            id: "c1".into(),
            on: true,
            result: Ok(Box::new(starred_as("c1", true, 4))),
        },
    );

    assert!(shows(&app, "★ 4"));
    let _ = press(&mut app, ":b log/following<CR>");
    assert!(shows(&app, "★ 4"));
}

#[test]
fn a_star_that_fails_says_why() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, "s");

    let _ = arrive(
        &mut app,
        Fetched::Star {
            id: "c1".into(),
            on: true,
            result: Err(FetchError::Failed("too many requests".into())),
        },
    );

    assert_eq!(cmdline(&app), "Cannot star c1: too many requests");
}

#[test]
fn follow_and_unfollow_the_account_under_the_cursor_or_named() {
    let mut app = with_log(Log::Following, mentioning_lin());

    let effects = press(&mut app, "j$:follow<CR>");
    assert_eq!(
        fetches(&effects),
        [Request::Follow {
            handle: "lin".into(),
            kind: Some(AccountKind::User),
            on: true
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Follow {
            handle: "lin".into(),
            on: true,
            result: Ok(()),
        },
    );
    assert_eq!(cmdline(&app), "Following @lin");

    let effects = press(&mut app, ":unfollow @kim<CR>");
    assert_eq!(
        fetches(&effects),
        [Request::Follow {
            handle: "kim".into(),
            kind: None,
            on: false
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Follow {
            handle: "kim".into(),
            on: false,
            result: Ok(()),
        },
    );
    assert_eq!(cmdline(&app), "No longer following @kim");
}

#[test]
fn following_someone_whose_profile_is_open_refreshes_it() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, ":e @lin<CR>");
    let _ = arrive(
        &mut app,
        Fetched::Profile {
            handle: "lin".into(),
            result: Ok(lin()),
        },
    );

    let _ = press(&mut app, ":follow<CR>");
    let effects = arrive(
        &mut app,
        Fetched::Follow {
            handle: "lin".into(),
            on: true,
            result: Ok(()),
        },
    );

    assert!(fetches(&effects).contains(&Request::Profile {
        handle: "lin".into(),
        kind: Some(AccountKind::User)
    }));
}

#[test]
fn dd_deletes_your_own_commit_once_confirmed() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));

    let effects = press(&mut app, "dd");
    assert_eq!(fetches(&effects), []);
    assert_eq!(cmdline(&app), "Delete commit c1? (y/n)");

    let effects = press(&mut app, "y");
    assert_eq!(
        fetches(&effects),
        [Request::DeleteCommit { id: "c1".into() }]
    );
    let _ = arrive(
        &mut app,
        Fetched::DeleteCommit {
            id: "c1".into(),
            result: Ok(()),
        },
    );
    assert!(shows(&app, "Commit deleted") && !shows(&app, "commit 1"));
}

#[test]
fn anything_but_y_keeps_the_commit() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));

    let effects = press(&mut app, "ddn");

    assert_eq!(fetches(&effects), []);
    assert!(shows(&app, "commit 1"));
}

#[test]
fn only_your_own_commits_can_be_deleted() {
    let mut theirs = commit_with("c7", "theirs", None);
    theirs["author"] = serde_json::json!({ "id": "u2", "handle": "lin", "name": "Lin" });
    let page = Page::new(
        serde_json::from_value(serde_json::json!([theirs])).expect("page"),
        None,
    );
    let mut app = with_log(Log::Following, page);

    let effects = press(&mut app, "dd");

    assert_eq!(fetches(&effects), []);
    assert_eq!(cmdline(&app), "You can only delete your own commits");
}

#[test]
fn a_starred_thread_commit_stays_whole() {
    let long: String = (1..=8)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, "<CR>");
    let thread = Fetched::Thread {
        id: "c1".into(),
        result: Ok(thread_of(&[], commit_with("c1", &long, None))),
    };
    let _ = arrive(&mut app, thread);
    let mut starred = commit_with("c1", &long, None);
    starred["starred"] = true.into();
    let starred = serde_json::from_value(starred).expect("commit");

    let _ = press(&mut app, "s");
    let _ = arrive(
        &mut app,
        Fetched::Star {
            id: "c1".into(),
            on: true,
            result: Ok(Box::new(starred)),
        },
    );

    assert!(shows(&app, "line 8"), "{:#?}", screen(&app).rows);
}

#[test]
fn s_on_a_mention_stars_the_commit_holding_it() {
    let mut app = with_log(Log::Following, mentioning_lin());

    let effects = press(&mut app, "j$s");

    assert_eq!(
        fetches(&effects),
        [Request::Star {
            id: "c1".into(),
            on: true
        }]
    );
}

/// The text the editor was asked to open, if any.
fn edited(effects: &[Effect]) -> Option<String> {
    effects.iter().find_map(|effect| match effect {
        Effect::Edit { text } => Some(text.clone()),
        _ => None,
    })
}

/// Saves `body` in the editor that `effects` opened, keeping what the template has below the scissors.
fn save(app: &mut App, effects: &[Effect], body: &str) -> Vec<Effect> {
    let text = edited(effects).expect("the editor opened");
    let scissors = text.find("# ---").expect("a scissors line");
    app.update(Msg::Edited(Ok(format!("{body}\n\n{}", &text[scissors..]))))
}

fn preview_of(body: &str) -> devnull_api::model::Preview {
    serde_json::from_value(
        serde_json::json!({ "body": body, "body_segments": [{ "type": "text", "text": body }] }),
    )
    .expect("preview")
}

#[test]
fn c_opens_the_editor_on_a_new_commit_with_its_context_below_the_scissors() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, "c");

    let text = edited(&effects).expect("the editor opened");
    assert!(text.starts_with('\n'), "an empty body first: {text:?}");
    assert!(text.contains("# ------------------------ >8 ------------------------"));
    assert!(text.contains("# A new commit as @ada."));
}

#[test]
fn a_saved_draft_is_previewed_then_published_once_confirmed() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");

    let effects = save(&mut app, &effects, "hello :wave:\n# not a comment");
    assert_eq!(
        fetches(&effects),
        [Request::Preview {
            body: "hello :wave:\n# not a comment".into()
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "hello :wave:\n# not a comment".into(),
            result: Ok(preview_of("hello 👋")),
        },
    );
    assert_eq!(cmdline(&app), "commit 7/4000? [y]es/[e]dit/[n]o");
    assert!(
        shows(&app, "hello 👋"),
        "the preview shows: {:#?}",
        screen(&app).rows
    );

    let effects = press(&mut app, "y");
    let draft = devnull_api::Draft::new(Answering::Nothing, "hello :wave:\n# not a comment");
    assert_eq!(
        fetches(&effects),
        [Request::Publish {
            draft: draft.clone()
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Published {
            draft,
            result: Ok(Box::new(starred_as("c9", false, 0))),
        },
    );
    assert_eq!(cmdline(&app), "Published c9");
    let effects = press(&mut app, "c");
    assert!(
        edited(&effects).expect("editor").starts_with('\n'),
        "nothing kept once published"
    );
}

#[test]
fn n_keeps_the_draft_for_next_time_and_e_edits_it_again() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");
    let _ = save(&mut app, &effects, "first try");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "first try".into(),
            result: Ok(preview_of("first try")),
        },
    );

    let effects = press(&mut app, "e");
    assert!(edited(&effects).expect("editor").starts_with("first try\n"));
    let _ = save(&mut app, &effects, "second try");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "second try".into(),
            result: Ok(preview_of("second try")),
        },
    );
    let effects = press(&mut app, "n");
    assert_eq!(fetches(&effects), []);
    assert_eq!(cmdline(&app), "Kept as a draft");

    let effects = press(&mut app, "c");
    assert!(
        edited(&effects)
            .expect("editor")
            .starts_with("second try\n")
    );
}

#[test]
fn r_replies_and_capital_r_quotes_the_commit_under_the_cursor() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));

    let effects = press(&mut app, "]]r");
    assert!(
        edited(&effects)
            .expect("editor")
            .contains("# Replying to @ada: commit 2")
    );
    let _ = save(&mut app, &effects, "agreed");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "agreed".into(),
            result: Ok(preview_of("agreed")),
        },
    );
    assert_eq!(cmdline(&app), "reply 6/4000 → @ada? [y]es/[e]dit/[n]o");
    let effects = press(&mut app, "y");
    assert_eq!(
        fetches(&effects),
        [Request::Publish {
            draft: devnull_api::Draft::new(Answering::Reply("c2".into()), "agreed")
        }]
    );

    let effects = press(&mut app, "R");
    assert!(
        edited(&effects)
            .expect("editor")
            .contains("# Quoting @ada: commit 2")
    );
}

/// `draft`, posted as the organization `handle`.
fn posted_as(mut draft: devnull_api::Draft, handle: &str) -> devnull_api::Draft {
    draft.author = Some(handle.into());
    draft
}

/// Commit `id`, published by the account `handle`.
fn published_by(id: &str, handle: &str) -> Box<devnull_api::model::Commit> {
    let mut commit = commit_with(id, "news", None);
    commit["author"] = serde_json::json!({ "id": "o", "handle": handle, "type": "organization" });
    Box::new(serde_json::from_value(commit).expect("commit"))
}

#[test]
fn commit_with_a_handle_posts_as_that_organization() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let effects = press(&mut app, ":commit @acme<CR>");
    assert!(
        edited(&effects)
            .expect("the editor opened")
            .contains("# A new commit as @acme.")
    );
    let _ = save(&mut app, &effects, "news");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "news".into(),
            result: Ok(preview_of("news")),
        },
    );
    assert_eq!(cmdline(&app), "commit 4/4000 as @acme? [y]es/[e]dit/[n]o");

    let effects = press(&mut app, "e");
    assert!(
        edited(&effects)
            .expect("editing again")
            .contains("# A new commit as @acme."),
        "still as the organization"
    );
    let _ = save(&mut app, &effects, "news");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "news".into(),
            result: Ok(preview_of("news")),
        },
    );
    let effects = press(&mut app, "y");
    let draft = posted_as(devnull_api::Draft::new(Answering::Nothing, "news"), "acme");
    assert_eq!(
        fetches(&effects),
        [Request::Publish {
            draft: draft.clone()
        }]
    );
    let _ = arrive(
        &mut app,
        Fetched::Published {
            draft,
            result: Ok(published_by("c9", "acme")),
        },
    );
    assert_eq!(cmdline(&app), "Published c9");
}

#[test]
fn reply_and_quote_with_a_handle_answer_as_that_organization() {
    let mut app = with_log(Log::Following, page_of(1, 2, None));

    let effects = press(&mut app, "]]:reply acme<CR>");
    assert!(
        edited(&effects)
            .expect("editor")
            .contains("# Replying to @ada as @acme: commit 2")
    );
    let _ = save(&mut app, &effects, "agreed");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "agreed".into(),
            result: Ok(preview_of("agreed")),
        },
    );
    assert_eq!(
        cmdline(&app),
        "reply 6/4000 → @ada as @acme? [y]es/[e]dit/[n]o"
    );
    let effects = press(&mut app, "y");
    assert_eq!(
        fetches(&effects),
        [Request::Publish {
            draft: posted_as(
                devnull_api::Draft::new(Answering::Reply("c2".into()), "agreed"),
                "acme"
            )
        }]
    );

    let effects = press(&mut app, ":quote @acme<CR>");
    assert!(
        edited(&effects)
            .expect("editor")
            .contains("# Quoting @ada as @acme: commit 2")
    );
}

#[test]
fn a_commit_published_as_someone_else_than_asked_says_so() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let draft = posted_as(devnull_api::Draft::new(Answering::Nothing, "news"), "acme");

    let _ = arrive(
        &mut app,
        Fetched::Published {
            draft,
            result: Ok(published_by("c9", "ada")),
        },
    );

    assert_eq!(cmdline(&app), "Published c9 as @ada, not @acme");
    let (_, height) = app.size();
    assert_eq!(
        style_at(&app, 0, height - 1).fg,
        app.style(Group::ErrorMsg).fg,
        "said as an error"
    );
}

#[test]
fn a_reply_published_shows_in_its_thread() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let _ = press(&mut app, "<CR>");
    let _ = arrive(
        &mut app,
        Fetched::Replies {
            id: "c1".into(),
            after: None,
            result: Ok(page_of(10, 1, None)),
        },
    );
    let thread = Fetched::Thread {
        id: "c1".into(),
        result: Ok(thread_of(&[], commit_with("c1", "hello", None))),
    };
    let _ = arrive(&mut app, thread);
    let effects = press(&mut app, "r");
    let _ = save(&mut app, &effects, "agreed");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "agreed".into(),
            result: Ok(preview_of("agreed")),
        },
    );
    let _ = press(&mut app, "y");

    let draft = devnull_api::Draft::new(Answering::Reply("c1".into()), "agreed");
    let _ = arrive(
        &mut app,
        Fetched::Published {
            draft,
            result: Ok(Box::new(starred_as("c99", false, 0))),
        },
    );

    let _ = press(&mut app, "G");
    assert!(shows(&app, "0000099"), "{:#?}", screen(&app).rows);
}

#[test]
fn an_empty_draft_or_a_failed_editor_commits_nothing() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");

    let effects = save(&mut app, &effects, "  \n ");
    assert_eq!(fetches(&effects), []);
    assert_eq!(
        cmdline(&app),
        "Aborting commit due to empty commit message."
    );

    let _ = press(&mut app, "c");
    let effects = app.update(Msg::Edited(Err("the editor exited with 1".into())));
    assert_eq!(fetches(&effects), []);
    assert_eq!(cmdline(&app), "Not committed: the editor exited with 1");
}

#[test]
fn a_draft_over_budget_is_kept_and_not_sent() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");
    let long = "x".repeat(4001);

    let effects = save(&mut app, &effects, &long);

    assert_eq!(fetches(&effects), []);
    assert_eq!(
        cmdline(&app),
        "Not committed: 4001/4000 characters, over the budget"
    );
    let effects = press(&mut app, "c");
    assert!(edited(&effects).expect("editor").starts_with(&long));
}

#[test]
fn a_draft_the_server_refuses_is_kept_with_its_reason() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");
    let _ = save(&mut app, &effects, "hi");

    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "hi".into(),
            result: Err(FetchError::Failed("The body field is required.".into())),
        },
    );

    assert_eq!(cmdline(&app), "Not committed: The body field is required.");
    let effects = press(&mut app, "c");
    assert!(edited(&effects).expect("editor").starts_with("hi\n"));
}

#[test]
fn emptying_a_kept_draft_lets_it_go() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));
    let effects = press(&mut app, "c");
    let _ = save(&mut app, &effects, "maybe later");
    let _ = arrive(
        &mut app,
        Fetched::Preview {
            body: "maybe later".into(),
            result: Ok(preview_of("maybe later")),
        },
    );
    let _ = press(&mut app, "n");

    let effects = press(&mut app, "c");
    let _ = save(&mut app, &effects, "");

    let effects = press(&mut app, "c");
    assert!(
        edited(&effects).expect("editor").starts_with('\n'),
        "an empty draft is not kept"
    );
}

/// One commit, `c1`, with a 400 × 200 picture: 48 × 12 cells.
fn with_picture() -> Page<Commit> {
    one_commit(
        "look",
        serde_json::json!({ "images": [{ "id": "i1", "url": "https://example.org/p.png", "width": 400, "height": 200 }] }),
    )
}

/// A connected app on following, drawing pictures as `graphics` does, and the effects of `page` arriving.
fn pictured(graphics: crate::graphics::Graphics, page: Page<Commit>) -> (App, Vec<Effect>) {
    let mut app = with_log(Log::Following, page_of(1, 0, None));
    app.set_graphics(graphics);
    let _ = press(&mut app, ":e!<CR>");
    let effects = arrive(
        &mut app,
        Fetched::Log {
            log: Log::Following,
            after: None,
            result: Ok(page),
        },
    );
    (app, effects)
}

fn pictures(effects: &[Effect]) -> Vec<(String, u16, u16, Option<u32>)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Picture {
                source,
                cols,
                rows,
                kitty,
            } => Some((source.clone(), *cols, *rows, *kitty)),
            _ => None,
        })
        .collect()
}

/// A still picture of `cells`, drawn with half blocks.
fn still(cells: Vec<Vec<crate::graphics::HalfBlock>>) -> Prepared {
    Prepared::HalfBlocks(vec![crate::graphics::HalfBlockFrame {
        cells,
        delay: Duration::ZERO,
    }])
}

#[test]
fn a_picture_in_view_is_asked_for_once_and_drawn_with_half_blocks() {
    let (mut app, effects) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());

    assert_eq!(
        pictures(&effects),
        [("https://example.org/p.png".to_owned(), 48, 12, None)]
    );
    let loading = screen(&app)
        .rows
        .iter()
        .filter(|row| row.contains("loading…"))
        .count();
    assert_eq!(loading, 1, "said once, in its first row");
    assert_eq!(pictures(&press(&mut app, "j")), [], "asked for once");

    let red = crate::graphics::HalfBlock {
        upper: Some((255, 0, 0)),
        lower: Some((0, 0, 255)),
    };
    let prepared = still(vec![vec![red; 48]; 12]);
    let _ = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols: 48,
        rows: 12,
        result: Ok(prepared),
    });

    // Row 0 is the bufferline, then the header and "look".
    let rows = screen(&app).rows;
    assert!(rows[3].starts_with("▀▀▀"), "{rows:#?}");
    let style = style_at(&app, 0, 3);
    assert_eq!(
        (style.fg, style.bg),
        (Some(Color::Rgb(255, 0, 0)), Some(Color::Rgb(0, 0, 255)))
    );
}

/// An animation drawn with half blocks for `with_picture`'s 48 × 12 cells: each frame one colour, shown for its
/// delay in milliseconds.
fn animated(frames: &[((u8, u8, u8), u64)]) -> Prepared {
    Prepared::HalfBlocks(
        frames
            .iter()
            .map(|&(colour, delay)| crate::graphics::HalfBlockFrame {
                cells: vec![
                    vec![
                        crate::graphics::HalfBlock {
                            upper: Some(colour),
                            lower: Some(colour),
                        };
                        48
                    ];
                    12
                ],
                delay: Duration::from_millis(delay),
            })
            .collect(),
    )
}

/// The wake-ups for the next frames of animations that `effects` ask for: how long after, and the token.
fn frame_wakes(effects: &[Effect]) -> Vec<(Duration, Wake)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Wake { after, token } if token.purpose == WakePurpose::Frame => {
                Some((*after, *token))
            }
            _ => None,
        })
        .collect()
}

/// The colour the picture of `with_picture` shows, in its first cell.
fn picture_colour(app: &App) -> Option<Color> {
    style_at(app, 0, 3).fg
}

/// The picture of `with_picture`, arrived.
fn picture_arrives(app: &mut App, prepared: Prepared) -> Vec<Effect> {
    app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols: 48,
        rows: 12,
        result: Ok(prepared),
    })
}

#[test]
fn an_animation_drawn_with_half_blocks_moves_on_after_each_frame_s_delay() {
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());
    let red = Some(Color::Rgb(255, 0, 0));
    let blue = Some(Color::Rgb(0, 0, 255));

    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50), ((0, 0, 255), 120)]));
    assert_eq!(picture_colour(&app), red);
    let [(after, token)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    assert_eq!(after, Duration::from_millis(50));
    assert_eq!(frame_wakes(&press(&mut app, "j")), [], "one at a time");

    let effects = app.update(Msg::Wake(token));
    assert_eq!(picture_colour(&app), blue);
    let [(after, token)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    assert_eq!(after, Duration::from_millis(120));

    let effects = app.update(Msg::Wake(token));
    assert_eq!(picture_colour(&app), red, "and round again");
    assert_eq!(frame_wakes(&effects).len(), 1);
}

#[test]
fn animations_in_view_move_on_each_after_its_own_delay() {
    let page = one_commit(
        "look",
        serde_json::json!({
            "images": [{ "id": "i1", "url": "https://example.org/p.png", "width": 400, "height": 200 }],
            "author": { "id": "u", "handle": "ada", "name": "Ada", "avatar_url": "https://example.org/ada.png" },
        }),
    );
    let (mut app, effects) = pictured(crate::graphics::Graphics::HalfBlocks, page);
    let asked = pictures(&effects);
    let Some((avatar, cols, rows, _)) = asked
        .iter()
        .find(|(source, ..)| source.ends_with("ada.png"))
    else {
        panic!("an avatar: {asked:?}");
    };
    let frames = |colour, delay| crate::graphics::HalfBlockFrame {
        cells: vec![
            vec![
                crate::graphics::HalfBlock {
                    upper: Some(colour),
                    lower: None
                };
                usize::from(*cols)
            ];
            usize::from(*rows)
        ],
        delay: Duration::from_millis(delay),
    };
    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50), ((0, 0, 255), 120)]));
    let effects_of_avatar = app.update(Msg::Picture {
        source: avatar.clone(),
        cols: *cols,
        rows: *rows,
        result: Ok(Prepared::HalfBlocks(vec![
            frames((0, 255, 0), 120),
            frames((0, 0, 0), 120),
        ])),
    });
    assert_eq!(frame_wakes(&effects_of_avatar), [], "one is to come");
    let [(after, token)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    assert_eq!(after, Duration::from_millis(50));

    let effects = app.update(Msg::Wake(token));

    let [(after, _)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    assert_eq!(
        after,
        Duration::from_millis(70),
        "the avatar's frame, 50 ms in"
    );
}

#[test]
fn an_animation_out_of_view_waits_until_it_is_back() {
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());
    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50), ((0, 0, 255), 120)]));
    let [(_, token)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };

    let _ = press(&mut app, ":e log/latest<CR>");
    assert_eq!(frame_wakes(&app.update(Msg::Wake(token))), []);
    let effects = press(&mut app, "<C-^>");

    assert_eq!(
        picture_colour(&app),
        Some(Color::Rgb(255, 0, 0)),
        "where it was left"
    );
    let [(after, _)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    assert_eq!(after, Duration::from_millis(50));
}

#[test]
fn still_pictures_and_animations_the_terminal_runs_need_no_wake_ups() {
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());
    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50)]));
    assert_eq!(frame_wakes(&effects), [], "one frame");
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());
    let still = animated(&[((255, 0, 0), 0), ((0, 0, 255), 0)]);
    assert_eq!(
        frame_wakes(&picture_arrives(&mut app, still)),
        [],
        "no time to move"
    );

    let (mut app, effects) = pictured(crate::graphics::Graphics::Kitty, with_picture());
    let [(_, cols, rows, Some(id))] = pictures(&effects)[..] else {
        panic!("one picture with an id: {effects:?}");
    };
    let effects = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols,
        rows,
        result: Ok(Prepared::Kitty(format!("frames of {id}"))),
    });
    assert_eq!(frame_wakes(&effects), []);
}

#[test]
fn a_wake_up_for_pictures_since_forgotten_is_ignored() {
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());
    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50), ((0, 0, 255), 120)]));
    let [(_, stale)] = frame_wakes(&effects)[..] else {
        panic!("one wake-up: {effects:?}");
    };
    let _ = press(
        &mut app,
        ":set graphics=kitty<CR>:set graphics=halfblocks<CR>",
    );
    let effects = picture_arrives(&mut app, animated(&[((255, 0, 0), 50), ((0, 0, 255), 120)]));
    assert_eq!(frame_wakes(&effects).len(), 1);

    let _ = app.update(Msg::Wake(stale));

    assert_eq!(picture_colour(&app), Some(Color::Rgb(255, 0, 0)));
}

#[test]
fn with_kitty_graphics_a_picture_is_sent_once_and_shown_by_placeholders() {
    let (mut app, effects) = pictured(crate::graphics::Graphics::Kitty, with_picture());
    let [(_, cols, rows, Some(id))] = pictures(&effects)[..] else {
        panic!("one picture with an id: {effects:?}");
    };

    let effects = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols,
        rows,
        result: Ok(Prepared::Kitty("<sent>".into())),
    });

    assert_eq!(effects, [Effect::Graphics("<sent>".into())]);
    let rows = screen(&app).rows;
    assert!(rows[3].starts_with('\u{10EEEE}'), "{rows:#?}");
    let (r, g, b) = crate::graphics::image_colour(id);
    assert_eq!(style_at(&app, 0, 3).fg, Some(Color::Rgb(r, g, b)));
}

#[test]
fn a_picture_out_of_view_is_asked_for_once_scrolled_to() {
    let mut page = page_of(1, 6, None);
    let mut last = with_picture().items.remove(0);
    // Not c1: that one is in the page already.
    last.id = "c9".into();
    page.items.push(last);
    let (mut app, effects) = pictured(crate::graphics::Graphics::HalfBlocks, page);
    assert_eq!(pictures(&effects), []);

    let effects = press(&mut app, "G");

    assert_eq!(pictures(&effects).len(), 1);
}

#[test]
fn a_picture_that_cannot_be_had_says_so_in_its_place() {
    let (mut app, _) = pictured(crate::graphics::Graphics::HalfBlocks, with_picture());

    let _ = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols: 48,
        rows: 12,
        result: Err("gone".into()),
    });

    assert!(shows(&app, "unavailable"));
}

#[test]
fn switching_graphics_frees_what_was_sent_and_draws_pictures_again() {
    let (mut app, effects) = pictured(crate::graphics::Graphics::Kitty, with_picture());
    let [(_, cols, rows, Some(id))] = pictures(&effects)[..] else {
        panic!("one picture: {effects:?}");
    };
    let _ = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols,
        rows,
        result: Ok(Prepared::Kitty(String::new())),
    });

    let effects = press(&mut app, ":set graphics=halfblocks<CR>");

    assert!(effects.contains(&Effect::Graphics(crate::graphics::kitty_delete(id))));
    assert_eq!(
        pictures(&effects),
        [("https://example.org/p.png".to_owned(), 48, 12, None)]
    );
    assert_eq!(press(&mut app, ":set graphics?<CR>"), []);
    assert_eq!(cmdline(&app), "graphics=halfblocks");
}

#[test]
fn quitting_frees_the_pictures_sent() {
    let (mut app, effects) = pictured(crate::graphics::Graphics::Kitty, with_picture());
    let [(_, cols, rows, Some(id))] = pictures(&effects)[..] else {
        panic!("one picture: {effects:?}");
    };
    let _ = app.update(Msg::Picture {
        source: "https://example.org/p.png".into(),
        cols,
        rows,
        result: Ok(Prepared::Kitty(String::new())),
    });

    let effects = press(&mut app, ":qa<CR>");

    assert_eq!(
        effects,
        [
            Effect::Graphics(crate::graphics::kitty_delete(id)),
            Effect::Quit
        ]
    );
}

#[test]
fn an_avatar_beside_a_name_is_asked_for_and_drawn_in_its_cells() {
    let mut commit = commit_with("c1", "hi", None);
    commit["author"]["avatar_url"] = "https://example.org/ada.png".into();
    let page = Page::new(
        serde_json::from_value(serde_json::json!([commit])).expect("page"),
        None,
    );

    let (mut app, effects) = pictured(crate::graphics::Graphics::HalfBlocks, page);

    assert_eq!(
        pictures(&effects),
        [("https://example.org/ada.png".to_owned(), 2, 1, None)]
    );
    assert!(
        screen(&app).rows[1].contains(" Ada @ada"),
        "the name shows while the avatar loads"
    );
    let face = crate::graphics::HalfBlock {
        upper: Some((1, 2, 3)),
        lower: Some((4, 5, 6)),
    };
    let prepared = still(vec![vec![face; 2]]);
    let _ = app.update(Msg::Picture {
        source: "https://example.org/ada.png".into(),
        cols: 2,
        rows: 1,
        result: Ok(prepared),
    });
    let rows = screen(&app).rows;
    assert!(rows[1].starts_with("▀▀ Ada @ada"), "{rows:#?}");
    assert_eq!(style_at(&app, 1, 1).bg, Some(Color::Rgb(4, 5, 6)));
}

#[test]
fn an_avatar_further_along_a_row_is_drawn_where_it_is() {
    let follow: devnull_api::model::Notification = serde_json::from_value(serde_json::json!({
        "id": "n1", "group_id": "g1", "type": "followed", "created_at": "2026-11-12T15:13:40Z",
        "actor": { "id": "u2", "handle": "lin", "name": "Lin", "avatar_url": "https://example.org/lin.png" }, "actors_count": 1
    }))
    .expect("notification");
    let (mut app, _) = starting();
    let _ = net(&mut app, NetEvent::Connected(me("ada", true)));
    app.set_graphics(crate::graphics::Graphics::HalfBlocks);
    let _ = press(&mut app, "<Space>n");
    let inbox =
        devnull_api::model::Inbox::new(Page::new(vec![follow], None), "2026-11-12T15:13:40Z");
    let _ = arrive(
        &mut app,
        Fetched::Notifications {
            only: Vec::new(),
            after: None,
            result: Ok(inbox),
        },
    );

    let face = crate::graphics::HalfBlock {
        upper: Some((1, 2, 3)),
        lower: Some((4, 5, 6)),
    };
    let prepared = still(vec![vec![face; 2]]);
    let _ = app.update(Msg::Picture {
        source: "https://example.org/lin.png".into(),
        cols: 2,
        rows: 1,
        result: Ok(prepared),
    });

    // The gutter and the icon come first: "● + ".
    let rows = screen(&app).rows;
    assert!(rows[1].starts_with("● + ▀▀ Lin @lin"), "{rows:#?}");
}

#[test]
fn ctrl_caret_and_b_hash_go_to_the_alternate_buffer() {
    let mut app = three_logs();

    let _ = press(&mut app, ":b#<CR>");
    assert!(statusline(&app).contains("log/latest"));
    let _ = press(&mut app, "<C-^>");
    assert!(statusline(&app).contains("log/trending"));
    let _ = press(&mut app, "<C-6>");
    assert!(statusline(&app).contains("log/latest"));
}

#[test]
fn without_an_alternate_buffer_b_hash_says_so() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let _ = press(&mut app, ":b#<CR>");

    assert_eq!(cmdline(&app), "E23: No alternate file");
}

#[test]
fn jumps_lists_the_jumplist_as_vim_does() {
    let mut app = app_with(&["one", "two", "three", "four", "five"], 50, 12);
    let _ = press(&mut app, "/thr<CR>G");

    let _ = press(&mut app, ":jumps<CR>");

    let rows = screen(&app).rows;
    assert_eq!(
        rows[rows.len() - 5..],
        [
            " jump line  col file/text",
            "   2     1    0 one",
            "   1     3    0 three",
            ">",
            "Press ENTER or type command to continue"
        ]
    );
}

#[test]
fn jumps_into_other_buffers_show_their_names_and_where_one_went_back_to() {
    let mut app = three_logs();
    let _ = press(&mut app, "<C-o>");

    let _ = press(&mut app, ":ju<CR>");

    // Back on latest (never loaded here), between following and trending.
    let rows = screen(&app).rows;
    assert_eq!(
        rows[rows.len() - 5..],
        [
            " jump line  col file/text",
            "   1     1    0 log/following",
            ">  0     1    0 Loading log/latest…",
            "   1     1    0 log/trending",
            "Press ENTER or type command to continue"
        ]
    );
}

#[test]
fn jumps_into_another_buffer_count_rows_with_the_folds_left_there() {
    let body = (1..=8)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = with_log(Log::Following, one_commit(&body, serde_json::json!({})));
    let _ = press(&mut app, "zRG");
    let ruler = statusline(&app);
    let row = ruler
        .split_whitespace()
        .find_map(|word| word.split_once(',').map(|(row, _)| row.to_owned()))
        .expect("a ruler");

    let _ = press(&mut app, ":e log/latest<CR>:ju<CR>");

    let rows = screen(&app).rows;
    // The newest entry there: where the window left it (the one before is where `G` jumped from).
    let entry = rows
        .iter()
        .rfind(|row| row.ends_with("log/following"))
        .expect("an entry");
    assert_eq!(
        entry.split_whitespace().nth(1),
        Some(row.as_str()),
        "{entry:?}, as the ruler said: {ruler:?}"
    );
}

#[test]
fn a_narrow_statusline_keeps_the_ruler_and_shortens_the_name_from_the_left() {
    let mut app = with_log(Log::Following, page_of(1, 1, None));

    let _ = app.update(Msg::Resize {
        width: 30,
        height: 12,
    });

    // " NORMAL  log/following  @ada  1,1  All " is 39 cells: the handle goes, then the name's start.
    assert_eq!(statusline(&app), " NORMAL  </following 1,1  All");
}

#[test]
fn star_without_a_keyword_ahead_searches_the_non_blank_word_under_the_cursor() {
    let mut app = app_with(&["fish :=", "and :="], 40, 7);

    let _ = press(&mut app, "5l*");

    assert_eq!(screen(&app).cursor, (4, 2));
}

#[test]
fn hi_link_draws_a_group_as_another_until_it_is_unlinked_or_restyled() {
    use crate::hl::Group;
    let mut app = app_with(&["one"], 40, 6);
    let own = app.style(Group::Handle);

    let _ = press(&mut app, ":hi link DevnullHandle ErrorMsg<CR>");
    assert_eq!(app.style(Group::Handle), app.style(Group::ErrorMsg));
    let _ = press(&mut app, ":hi DevnullHandle<CR>");
    assert_eq!(cmdline(&app), "DevnullHandle  xxx links to ErrorMsg");

    let _ = press(&mut app, ":hi link DevnullHandle NONE<CR>");
    assert_eq!(app.style(Group::Handle), own);

    let _ = press(
        &mut app,
        ":hi link DevnullHandle ErrorMsg<CR>:hi DevnullHandle gui=bold<CR>",
    );
    assert_ne!(
        app.style(Group::Handle),
        app.style(Group::ErrorMsg),
        "restyling a linked group unlinks it"
    );

    let _ = press(&mut app, ":hi link DevnullHash ErrorMsg<CR>:hi clear<CR>");
    assert_ne!(app.style(Group::Hash), app.style(Group::ErrorMsg));
    let _ = press(&mut app, ":hi link Nope ErrorMsg<CR>");
    assert_eq!(cmdline(&app), "E411: Highlight group not found: Nope");
}

#[test]
fn groups_linked_in_a_loop_still_draw() {
    use crate::hl::Group;
    let mut app = app_with(&["one"], 40, 6);

    let _ = press(
        &mut app,
        ":hi link DevnullHash DevnullHandle<CR>:hi link DevnullHandle DevnullHash<CR>",
    );

    let _ = app.style(Group::Hash);
    let _ = screen(&app);
}

#[test]
fn hlsearch_shows_the_last_search_s_matches_and_noh_hides_them_until_the_next() {
    use crate::hl::Group;
    let mut app = app_with(&["one fish", "two fish", "red"], 40, 8);

    let _ = press(&mut app, "/fish<CR>");

    assert_eq!(
        style_at(&app, 4, 1).bg,
        app.style(Group::CurSearch).bg,
        "under the cursor"
    );
    assert_eq!(style_at(&app, 4, 2).bg, app.style(Group::Search).bg);
    assert_eq!(style_at(&app, 0, 2).bg, app.style(Group::Normal).bg);
    let _ = press(&mut app, ":noh<CR>");
    assert_eq!(style_at(&app, 4, 2).bg, app.style(Group::Normal).bg);
    let _ = press(&mut app, "n");
    assert_eq!(
        style_at(&app, 4, 1).bg,
        app.style(Group::Search).bg,
        "back, the cursor on the other match"
    );
    let _ = press(&mut app, ":set nohls<CR>");
    assert_eq!(style_at(&app, 4, 1).bg, app.style(Group::Normal).bg);
}

#[test]
fn incsearch_shows_where_a_search_being_typed_goes_and_escape_goes_back() {
    use crate::hl::Group;
    let mut lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    lines[24] = "a needle here".into();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut app = app_with(&lines, 40, 10);

    let _ = press(&mut app, "/need");

    let rows = screen(&app).rows;
    let at = rows
        .iter()
        .position(|row| row.contains("a needle here"))
        .expect("scrolled to the match");
    assert_eq!(
        style_at(&app, 2, at as u16).bg,
        app.style(Group::IncSearch).bg
    );
    let _ = press(&mut app, "<Esc>");
    assert!(shows(&app, "line 1"), "back where it was");
    assert!(!shows(&app, "a needle here"));
}

#[test]
fn a_match_is_highlighted_whole_and_on_the_rows_it_wraps_to_only() {
    use crate::hl::Group;
    let mut app = app_with(&["aaaa fish bbbb"], 7, 8);

    let _ = press(&mut app, ":set nohls<CR>/fish<CR>");

    // Rows: "aaaa ", "fish ", "bbbb"; the first ends two cells short of the window.
    assert_eq!(style_at(&app, 5, 1).bg, app.style(Group::Normal).bg);
    assert_eq!(
        style_at(&app, 3, 2).bg,
        app.style(Group::CurSearch).bg,
        "the whole match"
    );
}

#[test]
fn star_shows_the_matches_again_after_noh_and_noincsearch_keeps_the_view() {
    use crate::hl::Group;
    let mut lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    lines[24] = "a needle here".into();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut app = app_with(&lines, 40, 10);

    let _ = press(&mut app, ":set nois<CR>/need");
    assert!(shows(&app, "line 1") && !shows(&app, "a needle here"));

    let _ = press(&mut app, "<Esc>/line<CR>:noh<CR>*");
    assert_eq!(style_at(&app, 0, 1).bg, app.style(Group::Search).bg);
}

#[test]
fn the_buffer_picker_shows_which_letters_matched() {
    use crate::hl::Group;
    let mut app = three_logs();

    let _ = press(&mut app, "<Space><Space>lgfl");

    let rows = screen(&app).rows;
    let y = rows
        .iter()
        .position(|row| row.contains("│  log/following"))
        .expect("listed");
    let x = rows[y].chars().position(|c| c == 'l').expect("in the row");
    let style = |dx: usize| {
        style_at(
            &app,
            u16::try_from(x + dx).expect("x"),
            u16::try_from(y).expect("y"),
        )
    };
    let matched = app.style(Group::PmenuMatch).fg;
    // l, g, the f of "following", then its l.
    assert_eq!([0, 2, 4, 6].map(|dx| style(dx).fg), [matched; 4]);
    assert_ne!(style(1).fg, matched);
}
