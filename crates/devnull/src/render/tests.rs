//! Commits as blocks: what each part of a commit becomes on screen.

use devnull_api::model::Commit;

use super::*;
use crate::doc::{AccountRef, Content, Cursor, Document, Layout, Motion, Target};

/// 2026-11-12T15:13:40Z, a little after the fixtures' commits.
const NOW: u64 = 1_794_496_420;

fn commit(json: serde_json::Value) -> Commit {
    let mut base = serde_json::json!({
        "id": "c1", "short_hash": "6cd1130", "body": "", "body_segments": [],
        "author": { "id": "u1", "handle": "grace", "name": "Grace", "type": "user" },
        "created_at": "2026-11-12T15:10:40.000000Z", "status": "approved",
        "stars_count": 2, "replies_count": 1, "images": []
    });
    if let (Some(base), Some(extra)) = (base.as_object_mut(), json.as_object()) {
        base.extend(extra.clone());
    }
    serde_json::from_value(base).expect("commit")
}

fn account(handle: &str, kind: AccountKind) -> Target {
    Target::Account(AccountRef::new(handle, Some(kind)))
}

fn text(segment: &str) -> serde_json::Value {
    serde_json::json!({ "type": "text", "text": segment })
}

/// The block's rows at a wide width, as text.
fn rows(content: Content) -> Vec<String> {
    let mut doc = Document::new();
    doc.push_back("c", content);
    let layout = Layout::new(&doc, 200);
    (0..layout.row_count())
        .map(|row| layout.segments(row).map(|(text, _)| text).collect())
        .collect()
}

/// The targets under each character of row `row`, in order, deduplicated; text referring to the commit as a whole
/// is left out.
fn targets(content: Content, row: usize) -> Vec<Target> {
    let whole = content.target.clone();
    let mut doc = Document::new();
    doc.push_back("c", content);
    let layout = Layout::new(&doc, 200);
    let mut cursor = Cursor::new();
    cursor.apply(&layout, Motion::GotoRow(row + 1), 1);
    cursor.apply(&layout, Motion::RowStart, 1);
    let mut found: Vec<Target> = Vec::new();
    for _ in 0..200 {
        if let Some(target) = cursor.target(&layout)
            && found.last() != Some(target)
            && whole.as_ref() != Some(target)
        {
            found.push(target.clone());
        }
        cursor.apply(&layout, Motion::Right, 1);
    }
    found
}

#[test]
fn a_commit_is_a_header_its_text_its_counts_and_a_blank_line() {
    let block = commit_block(
        &commit(serde_json::json!({ "body_segments": [text("hello\nworld")] })),
        NOW,
    );

    assert_eq!(
        rows(block),
        [
            "Grace @grace · 3m · 6cd1130",
            "hello",
            "world",
            "☆ 2  ↩ 1",
            ""
        ]
    );
}

#[test]
fn mentions_and_links_in_the_text_are_targets() {
    let block = commit_block(
        &commit(serde_json::json!({
            "body_segments": [text("hi @ada and @nobody, see https://example.org/x.")],
            "mentions": [{ "handle": "ada", "kind": "user", "offset": 3, "length": 4 }]
        })),
        NOW,
    );

    assert_eq!(
        targets(block, 1),
        [
            account("ada", AccountKind::User),
            Target::Url("https://example.org/x".into())
        ]
    );
}

#[test]
fn the_header_points_at_the_author() {
    let block = commit_block(&commit(serde_json::json!({})), NOW);

    assert_eq!(targets(block, 0), [account("grace", AccountKind::User)]);
}

#[test]
fn inline_code_stays_in_the_line_and_blocks_get_a_header_and_plain_text() {
    let block = commit_block(
        &commit(serde_json::json!({ "body_segments": [
            text("run "), { "type": "code", "code": "cargo test" }, text(" now"),
            { "type": "block", "language": "rust", "filename": "main.rs",
              "html": "<span class=\"k\">fn</span> main() {\n    println!(&quot;&lt;hi&gt; &amp; bye&quot;);\n}" }
        ] })),
        NOW,
    );

    let rows = rows(block);
    assert_eq!(rows[1], "run cargo test now");
    assert_eq!(
        rows[2..6],
        [
            "── main.rs (rust) ──",
            "fn main() {",
            "    println!(\"<hi> & bye\");",
            "}"
        ]
    );
}

#[test]
fn a_quote_is_indented_under_a_bar_and_points_at_the_quoted_commit() {
    let block = commit_block(
        &commit(serde_json::json!({
            "quoted_commit": {
                "id": "q1", "short_hash": "82e820a", "body_segments": [text("quoted text")],
                "author": { "id": "o1", "handle": "acme", "name": "Acme", "type": "organization" },
                "created_at": "2026-11-12T14:13:40.000000Z", "images": []
            }
        })),
        NOW,
    );

    let rows = rows(block.clone());
    assert_eq!(
        rows[1..3],
        ["┃ Acme (org) @acme · 1h · 82e820a", "┃ quoted text"]
    );
    let quoted = Target::Commit {
        id: "q1".into(),
        author: Some(AccountRef::new("acme", Some(AccountKind::Organization))),
    };
    assert_eq!(targets(block.clone(), 1)[0], quoted);
    assert_eq!(targets(block, 2), [quoted], "the quote's text too");
}

#[test]
fn the_rest_of_a_commit_points_at_it() {
    let block = commit_block(
        &commit(serde_json::json!({ "body_segments": [text("plain")] })),
        NOW,
    );
    let deleted: Commit = serde_json::from_value(
        serde_json::json!({ "id": "d1", "deleted_at": "2026-11-12T15:00:00Z" }),
    )
    .expect("commit");

    assert_eq!(
        block.target,
        Some(Target::Commit {
            id: "c1".into(),
            author: Some(AccountRef::new("grace", Some(AccountKind::User)))
        })
    );
    assert_eq!(
        commit_block(&deleted, NOW).target,
        Some(Target::Commit {
            id: "d1".into(),
            author: None
        })
    );
}

#[test]
fn the_commit_a_thread_is_about_shows_whole_and_leaves_its_replies_to_the_thread() {
    let body: String = (1..=10)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let commit = commit(serde_json::json!({
        "body_segments": [text(&body)],
        "reply_preview": { "data": [{
            "id": "r1", "short_hash": "aaaaaaa", "body_segments": [text("a reply")],
            "author": { "id": "u2", "handle": "lin", "name": "Lin" }, "created_at": "2026-11-12T15:12:40.000000Z", "images": []
        }] }
    }));

    let rows = rows(focused_block(&commit, NOW));

    assert_eq!(rows[10..], ["line 10", "☆ 2  ↩ 1", ""]);
}

#[test]
fn link_cards_images_status_and_replies_follow_the_text() {
    let block = commit_block(
        &commit(serde_json::json!({
            "body_segments": [text("look")],
            "starred": true,
            "status": "pending",
            "link_preview": { "url": "https://example.org", "title": "Example", "site_name": "example.org", "is_pending": false },
            "images": [{ "id": "i1", "url": "https://example.org/i.png", "width": 640, "height": 480 }],
            "reply_preview": { "data": [{
                "id": "r1", "short_hash": "aaaaaaa", "body_segments": [text("first line\nsecond")],
                "author": { "id": "u2", "handle": "lin", "name": "Lin" }, "created_at": "2026-11-12T15:12:40.000000Z", "images": []
            }] }
        })),
        NOW,
    );

    assert_eq!(
        rows(block),
        [
            "Grace @grace · 3m · 6cd1130",
            "look",
            "🔗 Example · example.org",
            "🖼 image 640×480",
            "★ 2  ↩ 1 · pending",
            "  ↳ Lin @lin: first line",
            "",
        ]
    );
}

#[test]
fn a_deleted_commit_shows_only_that() {
    let deleted: Commit = serde_json::from_value(
        serde_json::json!({ "id": "d1", "deleted_at": "2026-11-12T15:00:00Z" }),
    )
    .expect("commit");

    assert_eq!(rows(commit_block(&deleted, NOW)), ["Commit deleted", ""]);
}

#[test]
fn a_long_commit_folds_after_its_first_lines() {
    let body: String = (1..=10)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let block = commit_block(
        &commit(serde_json::json!({ "body_segments": [text(&body)] })),
        NOW,
    );

    let rows = rows(block);
    assert_eq!(
        rows[1..9],
        [
            "line 1",
            "line 2",
            "line 3",
            "line 4",
            "line 5",
            "line 6",
            "… 4 more lines",
            "☆ 2  ↩ 1"
        ]
    );
}

#[test]
fn ages_read_like_the_web() {
    let at = |seconds_ago: u64| age(NOW - seconds_ago, NOW);

    assert_eq!(
        [at(30), at(5 * 60), at(3 * 3600), at(2 * 86400)],
        ["now", "5m", "3h", "2d"]
    );
    assert_eq!(
        age(parse_time("2026-10-01T08:00:00Z").expect("time"), NOW),
        "Oct 1"
    );
    assert_eq!(
        age(
            parse_time("2025-12-24T08:00:00.123456Z").expect("time"),
            NOW
        ),
        "Dec 24, 2025"
    );
    assert_eq!(parse_time("not a time"), None);
}

#[test]
fn a_page_of_commits_is_one_block_each_keyed_by_id() {
    let commits = [
        commit(serde_json::json!({ "id": "a" })),
        commit(serde_json::json!({ "id": "b" })),
    ];
    let mut doc = Document::new();

    for commit in &commits {
        doc.push_back(commit.id.as_str(), commit_block(commit, NOW));
    }

    // Header, counts and the blank line: these commits have no text.
    assert_eq!(Layout::new(&doc, 80).row_count(), 2 * 3);
}
