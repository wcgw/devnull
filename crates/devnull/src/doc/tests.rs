//! Behaviour of the document model through its public interface: `Document`, `Layout` and `Cursor`.

use super::*;

/// A document of `(key, lines)` blocks of plain text.
fn doc(blocks: &[(&str, &[&str])]) -> Document {
    let mut doc = Document::new();
    for (key, lines) in blocks {
        doc.push_back(
            *key,
            lines.iter().map(|l| Line::text(*l)).collect::<Vec<_>>(),
        );
    }
    doc
}

fn at(row: usize, col: usize) -> ScreenPos {
    ScreenPos { row, col }
}

#[test]
fn j_moves_down_one_row() {
    let doc = doc(&[("a", &["first", "second"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::Down, 1);

    assert_eq!(cursor.screen(&layout), at(1, 0));
}

#[test]
fn cursor_stays_on_its_line_when_blocks_arrive_above() {
    let mut doc = doc(&[("b", &["bravo one", "bravo two"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);

    doc.push_front("a", vec![Line::text("alpha one"), Line::text("alpha two")]);

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(3, 0));
}

#[test]
fn long_lines_wrap_at_word_boundaries() {
    let doc = doc(&[("a", &["one two three four", ""])]);
    let layout = Layout::new(&doc, 9);

    let rows: Vec<&str> = (0..layout.row_count())
        .map(|row| layout.row_text(row))
        .collect();

    assert_eq!(rows, ["one two ", "three ", "four", ""]);
}

#[test]
fn a_word_longer_than_the_width_is_cut() {
    let doc = doc(&[("a", &["abcdefghij xy"])]);
    let layout = Layout::new(&doc, 4);

    let rows: Vec<&str> = (0..layout.row_count())
        .map(|row| layout.row_text(row))
        .collect();

    assert_eq!(rows, ["abcd", "efgh", "ij ", "xy"]);
}

#[test]
fn spaces_that_reach_the_edge_hang_off_it_instead_of_starting_a_row() {
    let doc = doc(&[("a", &["one two three"])]);
    let layout = Layout::new(&doc, 7);

    let rows: Vec<&str> = (0..layout.row_count())
        .map(|row| layout.row_text(row))
        .collect();

    assert_eq!(rows, ["one two ", "three"]);
}

#[test]
fn j_moves_through_the_rows_of_a_wrapped_line() {
    let doc = doc(&[("a", &["one two three four"])]);
    let layout = Layout::new(&doc, 9);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::Down, 2);

    assert_eq!(cursor.screen(&layout), at(2, 0));
}

#[test]
fn l_follows_the_text_onto_the_next_row_but_stops_at_the_end_of_the_line() {
    let doc = doc(&[("a", &["one two", "next"])]);
    let layout = Layout::new(&doc, 4);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::Right, 5);
    assert_eq!(cursor.screen(&layout), at(1, 1));

    cursor.apply(&layout, Motion::Right, 10);
    assert_eq!(cursor.screen(&layout), at(1, 2));
}

#[test]
fn h_stops_at_the_start_of_the_line() {
    let doc = doc(&[("a", &["one", "two three"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Down, 1);
    cursor.apply(&layout, Motion::Right, 4);

    cursor.apply(&layout, Motion::Left, 3);
    assert_eq!(cursor.screen(&layout), at(1, 1));

    cursor.apply(&layout, Motion::Left, 3);
    assert_eq!(cursor.screen(&layout), at(1, 0));
}

#[test]
fn the_cursor_stays_on_its_character_when_the_width_changes() {
    let doc = doc(&[("a", &["one two three four"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 9), Motion::Right, 15);
    assert_eq!(cursor.screen(&Layout::new(&doc, 9)), at(2, 1));

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(0, 15));
    assert_eq!(cursor.screen(&Layout::new(&doc, 5)), at(3, 1));
}

#[test]
fn k_moves_up_and_stops_at_the_top() {
    let doc = doc(&[("a", &["one", "two", "three"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Down, 2);

    cursor.apply(&layout, Motion::Up, 1);
    assert_eq!(cursor.screen(&layout), at(1, 0));

    cursor.apply(&layout, Motion::Up, 5);
    assert_eq!(cursor.screen(&layout), at(0, 0));
}

#[test]
fn vertical_moves_keep_aiming_for_the_same_column_across_short_rows() {
    let doc = doc(&[("a", &["hello world", "hi", "", "hello again"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Right, 8);

    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(1, 1));
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(2, 0));
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(3, 8));

    cursor.apply(&layout, Motion::Left, 1);
    cursor.apply(&layout, Motion::Up, 3);
    assert_eq!(cursor.screen(&layout), at(0, 7));
}

#[test]
fn wide_characters_take_two_cells_and_the_cursor_lands_on_the_one_covering_its_column() {
    let doc = doc(&[("a", &["abcdef", "日本語です", "x🦀y"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Right, 3);

    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(1, 2));
    cursor.apply(&layout, Motion::Right, 1);
    assert_eq!(cursor.screen(&layout), at(1, 4));
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(2, 3));
}

#[test]
fn zero_caret_and_dollar_work_on_the_screen_row() {
    let doc = doc(&[("a", &["  indented text wraps here"])]);
    let layout = Layout::new(&doc, 16);
    assert_eq!(
        (0..layout.row_count())
            .map(|r| layout.row_text(r))
            .collect::<Vec<_>>(),
        ["  indented text ", "wraps here"]
    );
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::FirstNonBlank, 1);
    assert_eq!(cursor.screen(&layout), at(0, 2));
    cursor.apply(&layout, Motion::RowEnd, 1);
    assert_eq!(cursor.screen(&layout), at(0, 15));
    cursor.apply(&layout, Motion::Down, 1);
    cursor.apply(&layout, Motion::Left, 2);
    cursor.apply(&layout, Motion::RowStart, 1);
    assert_eq!(cursor.screen(&layout), at(1, 0));
}

#[test]
fn after_dollar_vertical_moves_stick_to_the_end_of_each_row() {
    let doc = doc(&[("a", &["short", "a longer line", "mid", ""])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::RowEnd, 1);

    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(1, 12));
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(2, 2));
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(cursor.screen(&layout), at(3, 0));
    cursor.apply(&layout, Motion::Up, 2);
    assert_eq!(cursor.screen(&layout), at(1, 12));
}

/// Applies `motion` once per step and records where the cursor lands each time.
fn walk(layout: &Layout<'_>, cursor: &mut Cursor, motion: Motion, steps: usize) -> Vec<ScreenPos> {
    (0..steps)
        .map(|_| {
            cursor.apply(layout, motion, 1);
            cursor.screen(layout)
        })
        .collect()
}

#[test]
fn w_stops_at_each_word_and_punctuation_run() {
    let doc = doc(&[("a", &["foo.bar baz"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    let stops = walk(&layout, &mut cursor, Motion::WordForward, 4);

    assert_eq!(stops, [at(0, 3), at(0, 4), at(0, 8), at(0, 10)]);
}

#[test]
fn w_crosses_lines_and_stops_on_empty_ones() {
    let doc = doc(&[("a", &["one", ""]), ("b", &["  two"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    let stops = walk(&layout, &mut cursor, Motion::WordForward, 2);

    assert_eq!(stops, [at(1, 0), at(2, 2)]);
}

#[test]
fn big_w_only_stops_after_blanks() {
    let doc = doc(&[("a", &["foo.bar baz"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::BigWordForward, 1);

    assert_eq!(cursor.screen(&layout), at(0, 8));
}

#[test]
fn e_goes_to_the_end_of_each_word() {
    let doc = doc(&[("a", &["foo.bar", "", "baz"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    let stops = walk(&layout, &mut cursor, Motion::WordEnd, 4);

    assert_eq!(stops, [at(0, 2), at(0, 3), at(0, 6), at(2, 2)]);
}

#[test]
fn b_goes_back_to_the_start_of_each_word() {
    let doc = doc(&[("a", &["foo.bar", "", "baz qux"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Down, 2);
    cursor.apply(&layout, Motion::RowEnd, 1);

    let stops = walk(&layout, &mut cursor, Motion::WordBackward, 6);

    assert_eq!(
        stops,
        [at(2, 4), at(2, 0), at(1, 0), at(0, 4), at(0, 3), at(0, 0)]
    );
}

#[test]
fn big_e_and_big_b_treat_punctuation_as_part_of_the_word() {
    let doc = doc(&[("a", &["foo.bar baz"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::BigWordEnd, 1);
    assert_eq!(cursor.screen(&layout), at(0, 6));
    cursor.apply(&layout, Motion::BigWordForward, 1);
    cursor.apply(&layout, Motion::BigWordBackward, 1);
    assert_eq!(cursor.screen(&layout), at(0, 0));
}

#[test]
fn word_motions_stop_at_the_edges_of_the_document() {
    let doc = doc(&[("a", &["one two"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::WordForward, 5);
    assert_eq!(cursor.screen(&layout), at(0, 6));
    cursor.apply(&layout, Motion::WordBackward, 5);
    assert_eq!(cursor.screen(&layout), at(0, 0));
}

#[test]
fn f_and_t_find_a_character_forward_in_the_line_and_stay_put_when_it_is_missing() {
    let doc = doc(&[("a", &["a.b.c.d", "x.y"])]);
    let layout = Layout::new(&doc, 4);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::FindForward('.'), 2);
    assert_eq!(cursor.screen(&layout), at(0, 3));
    cursor.apply(&layout, Motion::TillForward('.'), 1);
    assert_eq!(cursor.screen(&layout), at(1, 0));
    cursor.apply(&layout, Motion::FindForward('y'), 1);
    assert_eq!(cursor.screen(&layout), at(1, 0));
}

#[test]
fn big_f_and_big_t_find_backward() {
    let doc = doc(&[("a", &["a.b.c.d"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::RowEnd, 1);

    cursor.apply(&layout, Motion::FindBackward('.'), 2);
    assert_eq!(cursor.screen(&layout), at(0, 3));
    cursor.apply(&layout, Motion::TillBackward('a'), 1);
    assert_eq!(cursor.screen(&layout), at(0, 1));
}

#[test]
fn double_brackets_jump_between_blocks() {
    let doc = doc(&[
        ("a", &["alpha", "more alpha"]),
        ("b", &["  bravo"]),
        ("c", &["charlie"]),
    ]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    assert_eq!(
        walk(&layout, &mut cursor, Motion::NextBlock, 3),
        [at(2, 0), at(3, 0), at(3, 0)]
    );

    cursor.apply(&layout, Motion::Up, 1);
    cursor.apply(&layout, Motion::RowEnd, 1);
    cursor.apply(&layout, Motion::PrevBlock, 1);
    assert_eq!(cursor.screen(&layout), at(2, 0));
    cursor.apply(&layout, Motion::PrevBlock, 5);
    assert_eq!(cursor.screen(&layout), at(0, 0));
}

#[test]
fn braces_jump_to_the_empty_lines_between_paragraphs() {
    let doc = doc(&[
        ("a", &["one", "two", "", "", "three", ""]),
        ("b", &["four"]),
    ]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    assert_eq!(
        walk(&layout, &mut cursor, Motion::ParagraphForward, 3),
        [at(2, 0), at(5, 0), at(6, 3)]
    );
    assert_eq!(
        walk(&layout, &mut cursor, Motion::ParagraphBackward, 3),
        [at(5, 0), at(3, 0), at(0, 0)]
    );
}

#[test]
fn gg_and_big_g_go_to_a_screen_row_by_number_or_to_the_last_one() {
    let doc = doc(&[("a", &["first", "  wrapped second row", "last"])]);
    let layout = Layout::new(&doc, 10);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::LastRow, 1);
    assert_eq!(cursor.screen(&layout), at(layout.row_count() - 1, 0));
    cursor.apply(&layout, Motion::GotoRow(2), 1);
    assert_eq!(cursor.screen(&layout), at(1, 2));
    cursor.apply(&layout, Motion::GotoRow(99), 1);
    assert_eq!(cursor.screen(&layout), at(layout.row_count() - 1, 0));
}

#[test]
fn a_count_past_the_last_block_goes_as_far_as_it_can() {
    let doc = doc(&[("a", &["alpha"]), ("b", &["bravo"]), ("c", &["charlie"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::NextBlock, 9);

    assert_eq!(cursor.screen(&layout), at(2, 0));
}

#[test]
fn the_target_under_the_cursor_is_the_one_of_the_span_it_sits_in() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![Line::Text(vec![
            Span::new("hi ", Group::Normal),
            Span::new("@ada", Group::Normal).with_target(Target::Buffer("@ada".into())),
            Span::new(" 🦀 ", Group::Normal),
            Span::new("dev-null.sh", Group::Normal)
                .with_target(Target::Url("https://dev-null.sh".into())),
        ])],
    );
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    let mut target_after = |motion, count| {
        cursor.apply(&layout, motion, count);
        cursor.target(&layout).cloned()
    };

    assert_eq!(target_after(Motion::Left, 1), None);
    assert_eq!(
        target_after(Motion::Right, 3),
        Some(Target::Buffer("@ada".into()))
    );
    assert_eq!(
        target_after(Motion::Right, 3),
        Some(Target::Buffer("@ada".into()))
    );
    assert_eq!(target_after(Motion::Right, 2), None);
    assert_eq!(
        target_after(Motion::Right, 2),
        Some(Target::Url("https://dev-null.sh".into()))
    );
}

#[test]
fn text_without_a_target_of_its_own_refers_to_its_block_s() {
    let mut doc = Document::new();
    let lines = vec![
        Line::Text(vec![
            Span::new("by ", Group::Normal),
            Span::new("@ada", Group::Normal).with_target(Target::Buffer("@ada".into())),
        ]),
        Line::text(""),
    ];
    doc.push_back(
        "a",
        Content {
            lines,
            target: Some(Target::Buffer("c1".into())),
            ..Content::default()
        },
    );
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    let mut target_after = |motion, count| {
        cursor.apply(&layout, motion, count);
        cursor.target(&layout).cloned()
    };

    assert_eq!(
        target_after(Motion::Left, 1),
        Some(Target::Buffer("c1".into()))
    );
    assert_eq!(
        target_after(Motion::Right, 3),
        Some(Target::Buffer("@ada".into()))
    );
    assert_eq!(
        target_after(Motion::Down, 1),
        Some(Target::Buffer("c1".into())),
        "an empty line too"
    );
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Right, 3);
    assert_eq!(cursor.target(&layout), Some(&Target::Buffer("@ada".into())));
    assert_eq!(
        cursor.block_target(&layout),
        Some(&Target::Buffer("c1".into())),
        "the block's own"
    );
}

#[test]
fn a_cursor_can_wait_at_a_block_that_has_not_arrived_yet() {
    let mut doc = doc(&[("a", &["one", "two"])]);
    let cursor = Cursor::on_block("b");
    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(0, 0));

    doc.push_back("b", vec![Line::text("three")]);

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(2, 0));
}

#[test]
fn an_empty_document_has_no_target_and_ignores_motions() {
    let doc = Document::new();
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    cursor.apply(&layout, Motion::WordForward, 3);

    assert_eq!(cursor.screen(&layout), at(0, 0));
    assert_eq!(cursor.target(&layout), None);
}

#[test]
fn replacing_the_cursor_block_keeps_the_cursor_on_the_same_line_clamped_to_the_new_text() {
    let mut doc = doc(&[("a", &["header", "★ 12 stars"]), ("b", &["next"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);
    cursor.apply(&Layout::new(&doc, 80), Motion::RowEnd, 1);

    assert!(doc.replace("a", vec![Line::text("header"), Line::text("★ 3")]));

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(1, 2));
    assert!(!doc.replace("missing", vec![]));
}

#[test]
fn removing_the_cursor_block_leaves_the_cursor_on_what_took_its_place() {
    let mut doc = doc(&[("a", &["alpha"]), ("b", &["bravo"]), ("c", &["charlie"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);

    doc.remove("b");

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(1, 0));
    assert_eq!(Layout::new(&doc, 80).row_text(1), "charlie");
}

fn row_texts(doc: &Document) -> Vec<String> {
    let layout = Layout::new(doc, 80);
    (0..layout.row_count())
        .map(|row| layout.row_text(row).to_owned())
        .collect()
}

#[test]
fn a_block_can_go_just_before_another_or_at_the_end_without_it() {
    let mut doc = doc(&[("a", &["alpha"]), ("c", &["charlie"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);

    doc.insert_before("c", "b", vec![Line::text("bravo")]);
    doc.insert_before("nothing", "d", vec![Line::text("delta")]);

    assert_eq!(row_texts(&doc), ["alpha", "bravo", "charlie", "delta"]);
    assert_eq!(
        cursor.screen(&Layout::new(&doc, 80)),
        at(2, 0),
        "still on charlie"
    );
}

#[test]
fn retain_keeps_only_the_blocks_asked_for() {
    let mut doc = doc(&[("a", &["alpha"]), ("b", &["bravo"]), ("c", &["charlie"])]);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 2);

    doc.retain(|key| key != "b");

    assert_eq!(row_texts(&doc), ["alpha", "charlie"]);
    assert_eq!(
        cursor.screen(&Layout::new(&doc, 80)),
        at(1, 0),
        "still on charlie"
    );
}

#[test]
fn rows_come_out_as_styled_segments_split_at_span_and_row_boundaries() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![Line::Text(vec![
            Span::new("plain ", Group::Normal),
            Span::new("@handle", Group::Handle),
        ])],
    );
    let layout = Layout::new(&doc, 9);

    let rows: Vec<Vec<(&str, Group)>> = (0..layout.row_count())
        .map(|row| layout.segments(row).collect())
        .collect();

    assert_eq!(
        rows,
        [
            vec![("plain ", Group::Normal)],
            vec![("@handle", Group::Handle)]
        ]
    );

    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![Line::Text(vec![
            Span::new("ab", Group::Normal),
            Span::new("cdefgh", Group::Handle),
        ])],
    );
    let layout = Layout::new(&doc, 4);
    let rows: Vec<Vec<(&str, Group)>> = (0..layout.row_count())
        .map(|row| layout.segments(row).collect())
        .collect();
    assert_eq!(
        rows,
        [
            vec![("ab", Group::Normal), ("cd", Group::Handle)],
            vec![("efgh", Group::Handle)]
        ]
    );
}

#[test]
fn an_image_line_takes_its_rows_at_any_width_it_fits_and_the_cursor_walks_through_them() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![
            Line::text("above"),
            Line::Image {
                rows: 3,
                cols: 16,
                source: "p.png".into(),
            },
            Line::text("below"),
        ],
    );

    for width in [16, 80] {
        let layout = Layout::new(&doc, width);
        assert_eq!(layout.row_count(), 5);
        let mut cursor = Cursor::new();
        cursor.apply(&layout, Motion::Right, 2);
        assert_eq!(
            walk(&layout, &mut cursor, Motion::Down, 4),
            [at(1, 0), at(2, 0), at(3, 0), at(4, 2)]
        );
        assert_eq!(layout.segments(2).count(), 0);
    }
}

#[test]
fn on_an_image_h_and_l_stay_put_and_the_cursor_keeps_its_image_row_across_resizes() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![
            Line::text("above the picture, long"),
            Line::Image {
                rows: 3,
                cols: 16,
                source: "p.png".into(),
            },
        ],
    );
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 2);

    cursor.apply(&Layout::new(&doc, 80), Motion::Right, 1);
    cursor.apply(&Layout::new(&doc, 80), Motion::Left, 1);
    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(2, 0));
    assert_eq!(
        cursor.screen(&Layout::new(&doc, 16)),
        at(3, 0),
        "below the text, now two rows"
    );
}

#[test]
fn slash_search_moves_to_the_next_match_wrapping_around_the_end() {
    use SearchOutcome::{Found, Wrapped};

    let doc = doc(&[
        ("a", &["one fish two fish"]),
        ("b", &["red fish blue fish"]),
    ]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    let fish = Pattern::new("fi.h", CaseMode::Sensitive).expect("valid pattern");

    let mut hits = Vec::new();
    for _ in 0..5 {
        let outcome = cursor.search(&layout, &fish, Direction::Forward, 1);
        hits.push((outcome, cursor.screen(&layout)));
    }

    assert_eq!(
        hits,
        [
            (Found, at(0, 4)),
            (Found, at(0, 13)),
            (Found, at(1, 4)),
            (Found, at(1, 14)),
            (Wrapped, at(0, 4))
        ]
    );
}

#[test]
fn backward_search_and_counts() {
    let doc = doc(&[
        ("a", &["one fish two fish"]),
        ("b", &["red fish blue fish"]),
    ]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    let fish = Pattern::new("fish", CaseMode::Sensitive).expect("valid pattern");

    assert_eq!(
        cursor.search(&layout, &fish, Direction::Backward, 1),
        SearchOutcome::Wrapped
    );
    assert_eq!(cursor.screen(&layout), at(1, 14));
    assert_eq!(
        cursor.search(&layout, &fish, Direction::Backward, 2),
        SearchOutcome::Found
    );
    assert_eq!(cursor.screen(&layout), at(0, 13));
}

#[test]
fn search_matches_across_wrap_points_and_reports_a_missing_pattern() {
    let doc = doc(&[("a", &["the quick brown fox"])]);
    let layout = Layout::new(&doc, 10);
    let mut cursor = Cursor::new();
    let phrase = Pattern::new("quick brown", CaseMode::Sensitive).expect("valid pattern");
    let missing = Pattern::new("QUICK", CaseMode::Sensitive).expect("valid pattern");

    assert_eq!(
        cursor.search(&layout, &phrase, Direction::Forward, 1),
        SearchOutcome::Found
    );
    assert_eq!(cursor.screen(&layout), at(0, 4));
    assert_eq!(
        cursor.search(&layout, &missing, Direction::Forward, 1),
        SearchOutcome::NotFound
    );
    assert_eq!(cursor.screen(&layout), at(0, 4));
}

#[test]
fn case_modes_follow_ignorecase_and_smartcase() {
    let doc = doc(&[("a", &["Rust rust"])]);
    let layout = Layout::new(&doc, 80);
    let first_match = |pattern: &str, case| {
        let mut cursor = Cursor::new();
        cursor.apply(&layout, Motion::RowEnd, 1);
        let pattern = Pattern::new(pattern, case).expect("valid pattern");
        let _ = cursor.search(&layout, &pattern, Direction::Forward, 1);
        cursor.screen(&layout).col
    };

    assert_eq!(first_match("rust", CaseMode::Sensitive), 5);
    assert_eq!(first_match("rust", CaseMode::Insensitive), 0);
    assert_eq!(first_match("rust", CaseMode::Smart), 0);
    assert_eq!(first_match("Rust", CaseMode::Smart), 0);
    assert!(Pattern::new("(", CaseMode::Sensitive).is_err());
}

#[test]
fn star_searches_the_word_under_or_after_the_cursor_as_a_whole_word() {
    let doc = doc(&[("a", &["  foo foobar", "foo. bar"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    let (pattern, outcome) = cursor
        .search_word(&layout, Direction::Forward, 1, CaseMode::Smart)
        .expect("a word after the cursor");
    assert_eq!(
        (outcome, cursor.screen(&layout)),
        (SearchOutcome::Found, at(1, 0))
    );
    assert_eq!(
        cursor.search(&layout, &pattern, Direction::Forward, 1),
        SearchOutcome::Wrapped
    );
    assert_eq!(cursor.screen(&layout), at(0, 2));

    cursor.apply(&layout, Motion::Down, 1);
    cursor.apply(&layout, Motion::FindForward(' '), 1);
    let (_, outcome) = cursor
        .search_word(&layout, Direction::Backward, 1, CaseMode::Smart)
        .expect("bar");
    assert_eq!(
        (outcome, cursor.screen(&layout)),
        (SearchOutcome::Wrapped, at(1, 5))
    );
}

#[test]
fn star_with_only_blanks_on_the_rest_of_the_line_does_nothing() {
    let doc = doc(&[("a", &["word   ", "next"])]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::RowEnd, 1);

    assert!(
        cursor
            .search_word(&layout, Direction::Forward, 1, CaseMode::Smart)
            .is_none()
    );
    assert_eq!(cursor.screen(&layout), at(0, 6));
}

#[test]
fn v_yanks_characters_inclusively_joining_wrapped_rows_and_breaking_lines() {
    let doc = doc(&[("a", &["one two", "three"])]);
    let layout = Layout::new(&doc, 4);
    let mut cursor = Cursor::new();
    let selection = Selection::start(&cursor, &layout, SelectionKind::Charwise);
    cursor.apply(&layout, Motion::WordEnd, 1);
    assert_eq!(selection.text(&cursor, &layout), "one");

    cursor.apply(&layout, Motion::WordForward, 1);
    let selection = Selection::start(&cursor, &layout, SelectionKind::Charwise);
    cursor.apply(&layout, Motion::Down, 1);
    cursor.apply(&layout, Motion::Right, 1);
    assert_eq!(selection.text(&cursor, &layout), "two\nth");

    cursor.apply(&layout, Motion::GotoRow(1), 1);
    assert_eq!(selection.text(&cursor, &layout), "one t");
}

#[test]
fn big_v_yanks_whole_screen_rows_as_lines() {
    let doc = doc(&[("a", &["one two three", "four"])]);
    let layout = Layout::new(&doc, 8);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Down, 1);
    let selection = Selection::start(&cursor, &layout, SelectionKind::Rowwise);

    assert_eq!(selection.text(&cursor, &layout), "three\n");
    cursor.apply(&layout, Motion::Down, 1);
    assert_eq!(selection.text(&cursor, &layout), "three\nfour\n");
    cursor.apply(&layout, Motion::Up, 2);
    assert_eq!(selection.text(&cursor, &layout), "one two three\n");
}

#[test]
fn a_selection_survives_blocks_arriving_above() {
    let mut doc = doc(&[("b", &["bravo"])]);
    let mut cursor = Cursor::new();
    let selection = Selection::start(&cursor, &Layout::new(&doc, 80), SelectionKind::Charwise);
    cursor.apply(&Layout::new(&doc, 80), Motion::RowEnd, 1);

    doc.push_front("a", vec![Line::text("alpha")]);

    assert_eq!(selection.text(&cursor, &Layout::new(&doc, 80)), "bravo");
}

#[test]
fn the_selection_highlights_cells_row_by_row() {
    let doc = doc(&[("a", &["one two", "日本"])]);
    let layout = Layout::new(&doc, 4);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Right, 2);
    let selection = Selection::start(&cursor, &layout, SelectionKind::Charwise);
    cursor.apply(&layout, Motion::Down, 2);

    let cells: Vec<_> = (0..layout.row_count())
        .map(|row| selection.cells(&cursor, &layout, row))
        .collect();

    assert_eq!(cells, [Some(2..4), Some(0..3), Some(0..4)]);
    let rows = Selection::start(&cursor, &layout, SelectionKind::Rowwise);
    assert_eq!(rows.cells(&cursor, &layout, 1), None);
    assert_eq!(rows.cells(&cursor, &layout, 2), Some(0..4));
}

#[test]
fn a_cursor_that_never_moved_stays_at_the_top_as_blocks_arrive() {
    let mut doc = doc(&[("b", &["bravo"])]);
    let cursor = Cursor::new();

    doc.push_front("a", vec![Line::text("alpha")]);

    assert_eq!(cursor.screen(&Layout::new(&doc, 80)), at(0, 0));
}

/// A block "a" of `head`, three folded lines and `tail`, then a block "b".
fn folded(closed: bool) -> Document {
    let mut doc = Document::new();
    let lines = ["head", "one", "two three", "four", "tail"]
        .map(Line::text)
        .to_vec();
    let fold = Fold {
        lines: 1..4,
        summary: Line::text("+ 3 more lines"),
        closed,
    };
    doc.push_back(
        "a",
        Content {
            lines,
            folds: vec![fold],
            ..Content::default()
        },
    );
    doc.push_back("b", vec![Line::text("next")]);
    doc
}

fn texts(layout: &Layout<'_>) -> Vec<String> {
    (0..layout.row_count())
        .map(|row| layout.segments(row).map(|(text, _)| text).collect())
        .collect()
}

#[test]
fn a_closed_fold_is_one_summary_row_that_j_and_k_step_over() {
    let doc = folded(true);
    let layout = Layout::new(&doc, 80);
    assert_eq!(texts(&layout), ["head", "+ 3 more lines", "tail", "next"]);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::Right, 3);

    assert_eq!(
        walk(&layout, &mut cursor, Motion::Down, 2),
        [at(1, 0), at(2, 3)]
    );
    assert_eq!(walk(&layout, &mut cursor, Motion::Up, 1), [at(1, 0)]);
}

#[test]
fn zo_zc_and_za_open_close_and_toggle_the_fold_under_the_cursor() {
    let doc = folded(true);
    let mut folds = Folds::default();
    let laid_out = |folds: &Folds| Layout::with_folds(&doc, folds.clone(), 80);
    let mut cursor = Cursor::new();
    cursor.apply(&laid_out(&folds), Motion::Down, 1);

    folds.fold(&doc, &cursor, FoldOp::Open);
    assert_eq!(
        texts(&laid_out(&folds)),
        ["head", "one", "two three", "four", "tail", "next"]
    );
    assert_eq!(cursor.screen(&laid_out(&folds)), at(1, 0));
    cursor.apply(&laid_out(&folds), Motion::Down, 1);
    cursor.apply(&laid_out(&folds), Motion::RowEnd, 1);

    folds.fold(&doc, &cursor, FoldOp::Close);
    assert_eq!(cursor.screen(&laid_out(&folds)), at(1, 0));
    folds.fold(&doc, &cursor, FoldOp::Toggle);
    assert_eq!(cursor.screen(&laid_out(&folds)), at(2, 8));
    folds.fold(&doc, &cursor, FoldOp::Toggle);
    assert_eq!(laid_out(&folds).row_count(), 4);
    assert_eq!(
        Layout::new(&doc, 80).row_count(),
        4,
        "the document's folds are as given"
    );
}

#[test]
fn zm_closes_and_zr_opens_every_fold() {
    let doc = folded(false);
    let mut folds = Folds::default();
    assert_eq!(Layout::with_folds(&doc, folds.clone(), 80).row_count(), 6);

    folds.fold_all(&doc, false);
    assert_eq!(Layout::with_folds(&doc, folds.clone(), 80).row_count(), 4);
    folds.fold_all(&doc, true);
    assert_eq!(Layout::with_folds(&doc, folds.clone(), 80).row_count(), 6);
}

#[test]
fn a_clone_of_folds_keeps_its_own_state() {
    let doc = folded(true);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);
    let mut folds = Folds::default();
    folds.fold(&doc, &cursor, FoldOp::Open);

    let mut other = folds.clone();
    other.fold(&doc, &cursor, FoldOp::Close);

    assert_eq!(Layout::with_folds(&doc, folds, 80).row_count(), 6);
    assert_eq!(Layout::with_folds(&doc, other, 80).row_count(), 4);
}

#[test]
fn a_fold_keeps_its_state_when_its_block_is_replaced() {
    let mut doc = folded(true);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);
    let mut folds = Folds::default();
    folds.fold(&doc, &cursor, FoldOp::Open);

    let lines = ["head", "one", "two", "four", "tail ★ 4"]
        .map(Line::text)
        .to_vec();
    let fold = Fold {
        lines: 1..4,
        summary: Line::text("+ 3 more lines"),
        closed: true,
    };
    doc.replace(
        "a",
        Content {
            lines,
            folds: vec![fold],
            ..Content::default()
        },
    );

    assert_eq!(Layout::with_folds(&doc, folds, 80).row_count(), 6);
}

#[test]
fn a_block_dropped_and_added_again_starts_with_its_folds_as_given() {
    let mut doc = folded(true);
    let mut cursor = Cursor::new();
    cursor.apply(&Layout::new(&doc, 80), Motion::Down, 1);
    let mut folds = Folds::default();
    folds.fold(&doc, &cursor, FoldOp::Open);

    doc.retain(|key| key != "a");
    let lines = ["head", "one", "two", "four", "tail"]
        .map(Line::text)
        .to_vec();
    let fold = Fold {
        lines: 1..4,
        summary: Line::text("+ 3 more lines"),
        closed: true,
    };
    doc.push_back(
        "a",
        Content {
            lines,
            folds: vec![fold],
            ..Content::default()
        },
    );

    // "next", then the block's head, summary and tail.
    assert_eq!(Layout::with_folds(&doc, folds, 80).row_count(), 4);
}

#[test]
fn word_motions_treat_a_closed_fold_as_one_stop() {
    let doc = folded(true);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    assert_eq!(
        walk(&layout, &mut cursor, Motion::WordForward, 2),
        [at(1, 0), at(2, 0)]
    );
    assert_eq!(
        walk(&layout, &mut cursor, Motion::WordBackward, 2),
        [at(1, 0), at(0, 0)]
    );
}

#[test]
fn word_motions_go_through_an_open_fold_word_by_word() {
    let doc = folded(false);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    assert_eq!(
        walk(&layout, &mut cursor, Motion::WordForward, 3),
        [at(1, 0), at(2, 0), at(2, 4)]
    );
}

/// A block of `head`, `a`, `b`, `c`, `d`, `tail` with `folds` as (lines, summary, closed), then a block `next`.
fn nested(folds: &[(Range<usize>, &str, bool)]) -> Document {
    let mut doc = Document::new();
    let lines = ["head", "a", "b", "c", "d", "tail"]
        .map(Line::text)
        .to_vec();
    let folds = folds
        .iter()
        .map(|(lines, summary, closed)| Fold {
            lines: lines.clone(),
            summary: Line::text(*summary),
            closed: *closed,
        })
        .collect();
    doc.push_back(
        "a",
        Content {
            lines,
            folds,
            ..Content::default()
        },
    );
    doc.push_back("b", vec![Line::text("next")]);
    doc
}

/// What `doc` shows with `folds`, row by row.
fn shown(doc: &Document, folds: &Folds) -> Vec<String> {
    texts(&Layout::with_folds(doc, folds.clone(), 80))
}

/// A cursor on line `line` of the first block of `doc`, all folds open.
fn on_line(doc: &Document, line: usize) -> Cursor {
    let mut open = Folds::default();
    open.fold_all(doc, true);
    Cursor::at_row(&Layout::with_folds(doc, open, 80), line)
}

const OUTER: (Range<usize>, &str, bool) = (1..5, "+ outer", true);
const INNER: (Range<usize>, &str, bool) = (2..4, "+ inner", true);

#[test]
fn a_closed_fold_inside_an_open_one_shows_its_own_summary() {
    let doc = nested(&[(1..5, "+ outer", false), INNER]);
    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );

    let doc = nested(&[OUTER, (2..4, "+ inner", false)]);
    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "+ outer", "tail", "next"]
    );
}

#[test]
fn zo_opens_the_outermost_closed_fold_and_zc_the_innermost_open_one_outside_it() {
    let doc = nested(&[OUTER, INNER]);
    let mut folds = Folds::default();
    let b = on_line(&doc, 2);

    folds.fold(&doc, &b, FoldOp::Open);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::Open);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "b", "c", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::Close);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::Close);
    assert_eq!(shown(&doc, &folds), ["head", "+ outer", "tail", "next"]);
    folds.fold(&doc, &b, FoldOp::Close);
    folds.fold(&doc, &b, FoldOp::Open);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"],
        "nothing open to close outside the closed fold"
    );
}

#[test]
fn zo_and_zc_in_capitals_go_all_the_way() {
    let doc = nested(&[OUTER, INNER, (4..5, "+ d", true)]);
    let mut folds = Folds::default();
    let b = on_line(&doc, 2);

    folds.fold(&doc, &b, FoldOp::OpenRecursively);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "b", "c", "d", "tail", "next"],
        "and the folds inside, holding the cursor or not"
    );
    folds.fold(&doc, &b, FoldOp::CloseRecursively);
    assert_eq!(shown(&doc, &folds), ["head", "+ outer", "tail", "next"]);
    folds.fold(&doc, &b, FoldOp::Open);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"],
        "only the folds holding the cursor were closed"
    );
}

#[test]
fn za_opens_a_closed_fold_and_closes_an_open_one_and_za_in_capitals_all_the_way() {
    let doc = nested(&[OUTER, INNER]);
    let mut folds = Folds::default();
    let b = on_line(&doc, 2);

    folds.fold(&doc, &b, FoldOp::Toggle);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::ToggleRecursively);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "b", "c", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::Toggle);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::ToggleRecursively);
    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "b", "c", "d", "tail", "next"]
    );
    folds.fold(&doc, &b, FoldOp::ToggleRecursively);
    assert_eq!(shown(&doc, &folds), ["head", "+ outer", "tail", "next"]);
}

#[test]
fn revealing_a_line_opens_every_fold_hiding_it_and_no_other() {
    let doc = nested(&[OUTER, INNER, (4..5, "+ d", true)]);
    let mut folds = Folds::default();

    folds.reveal(&doc, &on_line(&doc, 3));

    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "b", "c", "+ d", "tail", "next"]
    );
}

#[test]
fn folds_nest_whatever_order_they_come_in() {
    let doc = nested(&[INNER, OUTER]);
    let mut folds = Folds::default();
    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "+ outer", "tail", "next"]
    );

    folds.fold(&doc, &on_line(&doc, 2), FoldOp::Open);

    assert_eq!(
        shown(&doc, &folds),
        ["head", "a", "+ inner", "d", "tail", "next"]
    );
    let doc = nested(&[(4..5, "+ d", true), (1..3, "+ ab", true)]);
    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "+ ab", "c", "+ d", "tail", "next"]
    );
}

#[test]
fn zo_in_capitals_leaves_the_folds_around_alone() {
    let doc = nested(&[OUTER, INNER, (0..1, "+ head", true), (5..6, "+ tail", true)]);
    let mut folds = Folds::default();

    folds.fold(&doc, &on_line(&doc, 2), FoldOp::OpenRecursively);

    assert_eq!(
        shown(&doc, &folds),
        ["+ head", "a", "b", "c", "d", "+ tail", "next"]
    );
}

#[test]
fn a_fold_overlapping_another_without_holding_it_is_dropped() {
    let doc = nested(&[(1..4, "+ first", true), (2..5, "+ second", true)]);

    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "+ first", "d", "tail", "next"]
    );
}

#[test]
fn a_fold_over_the_same_lines_as_another_is_inside_it() {
    let doc = nested(&[(1..3, "+ outer", false), (1..3, "+ inner", true)]);

    assert_eq!(
        texts(&Layout::new(&doc, 80)),
        ["head", "+ inner", "c", "d", "tail", "next"]
    );
}

#[test]
fn word_motions_step_over_a_closed_fold_inside_an_open_one() {
    let doc = nested(&[(1..5, "+ outer", false), INNER]);
    let layout = Layout::new(&doc, 80);
    let mut cursor = Cursor::new();

    assert_eq!(
        walk(&layout, &mut cursor, Motion::WordForward, 3),
        [at(1, 0), at(2, 0), at(3, 0)]
    );
}

/// A block of a `cols` × `rows` picture between two lines of text.
fn with_picture(cols: u16, rows: u16) -> Document {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![
            Line::text("above"),
            Line::Image {
                rows,
                cols,
                source: "p".into(),
            },
            Line::text("below"),
        ],
    );
    doc
}

#[test]
fn a_picture_larger_than_the_room_shrinks_to_fit_keeping_its_shape() {
    let doc = with_picture(40, 10);
    let size = |layout: &Layout<'_>| {
        layout
            .picture(1)
            .map(|picture| (picture.cols, picture.rows))
    };

    assert_eq!(
        size(&Layout::new(&doc, 80)),
        Some((40, 10)),
        "whole when it fits"
    );
    let narrow = Layout::new(&doc, 20);
    assert_eq!(size(&narrow), Some((20, 5)));
    assert_eq!(narrow.row_count(), 1 + 5 + 1);
    let low = Layout::within(&doc, Folds::default(), 80, 3);
    assert_eq!(size(&low), Some((12, 3)));
    assert_eq!(low.row_count(), 1 + 3 + 1);
    assert_eq!(texts(&low)[4], "below");
}

#[test]
fn a_block_s_lines_can_be_read_back() {
    let doc = doc(&[("a", &["header", "body"])]);

    assert_eq!(doc.line_text("a", 1), Some("body"));
    assert_eq!(doc.line_text("a", 2), None);
    assert_eq!(doc.line_text("b", 0), None);
}

#[test]
fn a_picture_row_says_which_picture_and_which_of_its_rows() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![
            Line::text("above"),
            Line::Image {
                rows: 2,
                cols: 16,
                source: "p.png".into(),
            },
        ],
    );
    let layout = Layout::new(&doc, 80);

    assert_eq!(layout.picture(0), None);
    assert_eq!(
        layout.picture(2),
        Some(PictureRow {
            source: "p.png",
            cols: 16,
            rows: 2,
            row: 1
        })
    );
}

#[test]
fn a_picture_folded_away_shows_no_picture_rows() {
    let mut doc = Document::new();
    let picture = Line::Image {
        rows: 2,
        cols: 16,
        source: "p.png".into(),
    };
    let fold = Fold {
        lines: 1..2,
        summary: Line::text("+ a picture"),
        closed: true,
    };
    doc.push_back(
        "a",
        Content {
            lines: vec![Line::text("above"), picture],
            folds: vec![fold],
            ..Content::default()
        },
    );
    let layout = Layout::new(&doc, 80);

    assert_eq!(layout.row_count(), 2);
    assert_eq!(layout.picture(1), None, "the fold's summary row");
}

#[test]
fn a_picture_in_a_line_of_text_takes_its_cells_and_says_where_it_starts() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![Line::Text(vec![
            Span::new("ab", Group::Normal),
            Span::picture("face.png", 2),
            Span::new("cd", Group::Normal),
        ])],
    );
    let layout = Layout::new(&doc, 80);

    assert_eq!(layout.row_text(0), "ab\u{2800}\u{2800}cd");
    assert_eq!(
        layout.inline_pictures(0),
        [(
            2,
            PictureRow {
                source: "face.png",
                cols: 2,
                rows: 1,
                row: 0
            }
        )]
    );
}

#[test]
fn a_picture_wrapped_onto_the_next_row_belongs_to_that_row() {
    let mut doc = Document::new();
    doc.push_back(
        "a",
        vec![Line::Text(vec![
            Span::new("abcd efgh ", Group::Normal),
            Span::picture("face.png", 2),
        ])],
    );
    let layout = Layout::new(&doc, 10);

    assert_eq!(layout.inline_pictures(0), []);
    assert_eq!(
        layout.inline_pictures(1),
        [(
            0,
            PictureRow {
                source: "face.png",
                cols: 2,
                rows: 1,
                row: 0
            }
        )]
    );
}
