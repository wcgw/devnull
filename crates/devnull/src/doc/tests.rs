//! Behaviour of the document model through its public interface: `Document`, `Layout` and `Cursor`.

use super::*;

/// A document of `(key, lines)` blocks of plain text.
fn doc(blocks: &[(&str, &[&str])]) -> Document {
    let mut doc = Document::new();
    for (key, lines) in blocks {
        doc.push_back(*key, lines.iter().map(|l| Line::text(*l)).collect());
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
            Span::new("@ada", Group::Normal).with_target(Target::Mention("ada".into())),
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
        Some(Target::Mention("ada".into()))
    );
    assert_eq!(
        target_after(Motion::Right, 3),
        Some(Target::Mention("ada".into()))
    );
    assert_eq!(target_after(Motion::Right, 2), None);
    assert_eq!(
        target_after(Motion::Right, 2),
        Some(Target::Url("https://dev-null.sh".into()))
    );
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
