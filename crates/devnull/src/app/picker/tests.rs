//! The fuzzy matching behind the buffer picker.

use super::best;

/// How well `query` matches `text`.
fn score(query: &str, text: &str) -> Option<i64> {
    best(query, text).map(|(score, _)| score)
}

/// Which characters of `text` matched `query`, by index.
fn matched(query: &str, text: &str) -> Option<Vec<usize>> {
    best(query, text).map(|(_, positions)| positions)
}

#[test]
fn the_query_s_characters_must_appear_in_order() {
    assert!(score("lgfl", "log/following").is_some());
    assert_eq!(score("lgfl", "log/latest"), None);
    // `g`, then `l`, but no `/` after them.
    assert_eq!(score("gl/", "log/following"), None);
}

#[test]
fn an_empty_query_matches_everything_alike() {
    assert_eq!(score("", "log/latest"), score("", "thread/a82e820"));
}

#[test]
fn case_is_ignored_unless_the_query_has_capitals() {
    assert!(score("ada", "@Ada").is_some());
    assert_eq!(score("Ada", "@ada"), None);
}

#[test]
fn word_starts_and_runs_beat_scattered_letters() {
    let starts = score("lf", "log/following").expect("match");
    let scattered = score("lf", "self-fulfilling").expect("match");
    assert!(starts > scattered);
    let run = score("foll", "log/following").expect("match");
    let spread = score("foll", "f/o/l/l").expect("match");
    assert!(run > spread);
}

#[test]
fn a_later_better_place_wins_over_the_first_one() {
    // The first `t` starts a scattered match; the `t` of `trending` starts a run.
    let best = score("tren", "log/trending");
    let only_scattered = score("tren", "log/t-r-e-n");
    assert!(best > only_scattered);
}

#[test]
fn a_match_says_which_characters_matched() {
    assert_eq!(matched("lgfl", "log/following"), Some(vec![0, 2, 4, 6]));
    assert_eq!(
        matched("tren", "log/trending"),
        Some(vec![4, 5, 6, 7]),
        "the best place, not the first"
    );
    assert_eq!(matched("x", "log"), None);
    assert_eq!(
        matched("a", "a/a"),
        Some(vec![0]),
        "of equal places, the first"
    );
}
