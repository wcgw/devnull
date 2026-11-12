//! The server's rules for a commit's text, with the Go client's vectors (`internal/api/commit_test.go`,
//! `body_test.go` at 76c3413), which mirror the server's.

use super::*;

#[test]
fn embedded_nuls_are_dropped_as_the_server_does() {
    for body in [" before\0after ", "before\0\0after", "\0beforeafter\0"] {
        assert_eq!(normalize(body), "beforeafter");
    }
    assert_eq!(
        normalize("`a\0b`\n\n```go\nx\0y\n```"),
        "`ab`\n\n```go\nxy\n```"
    );
    assert_eq!(count("a\0b"), 2);
    assert_eq!(check("\0\0"), Err(BodyError::Empty));
}

#[test]
fn the_budget_counts_prose_links_and_code_as_the_server_does() {
    let a80 = "a".repeat(80);
    for (body, expected) in [
        ("  界👩🏽‍💻\r\n\r\n\r\nshipped  ".to_owned(), 14),
        (format!("https://example.com/{a80}"), 23),
        (format!("example.com/{a80}."), 24),
        (format!("`https://example.com/{a80}`"), 102),
        ("a\n\n\n\nb".to_owned(), 4),
        ("```go\na\n\n\n\nb\n```".to_owned(), 16),
    ] {
        assert_eq!(count(&body), expected, "{body:?}");
    }
}

#[test]
fn bodies_the_server_refuses_are_refused() {
    assert_eq!(
        check(&"a".repeat(4001)),
        Err(BodyError::OverBudget { count: 4001 })
    );
    assert_eq!(check(" \n "), Err(BodyError::Empty));
    assert_eq!(
        check(&format!("https://example.com/{}", "a".repeat(2048))),
        Err(BodyError::LinkTooLong)
    );
}

#[test]
fn text_that_only_looks_like_a_link_counts_as_written() {
    for body in [
        "nativephp.com.123",
        "socket.io.js",
        "array.at(-1)",
        "example.Com",
        "café.com",
        "foo_https://example.com/long/path",
        "mail@nativephp.com/a/long/path",
    ] {
        assert_eq!(count(body), body.chars().count(), "{body:?}");
    }
}

#[test]
fn long_links_fit_the_budget_but_not_past_the_raw_limit() {
    let body = format!("https://example.com/{} ", "a".repeat(1380)).repeat(3);
    assert_eq!(check(&body), Ok(count(&body)));
    assert_eq!(check(&body.repeat(20)), Err(BodyError::RawTooLong));
}

#[test]
fn unicode_spaces_end_links_and_invisibles_are_trimmed() {
    for space in ['\u{a0}', '\u{2003}', '\u{2028}', '\u{2029}', '\u{85}'] {
        let body = format!(
            "https://example.com/long-path-here{space}{}",
            "x".repeat(100)
        );
        assert_eq!(count(&body), 124, "{space:?}");
    }
    for space in [
        '\u{a0}',
        '\u{200b}',
        '\u{34f}',
        '\u{2060}',
        '\u{feff}',
        '\u{1d159}',
    ] {
        let body = format!("{space}{}{space}", "x".repeat(4000));
        assert_eq!(check(&body), Ok(4000), "{space:?}");
    }
}
