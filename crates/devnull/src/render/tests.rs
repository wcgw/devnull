//! Commits as blocks: what each part of a commit becomes on screen.

use devnull_api::model::{Account, Commit, OrganizationPage, Profile};

use super::*;
use crate::doc::{AccountRef, Content, Cursor, Document, Layout, LinkCard, Motion, Target};

/// 2026-11-12T15:13:40Z, a little after the fixtures' commits.
const NOW: u64 = 1_794_496_420;

fn commit(json: serde_json::Value) -> Commit {
    let mut base = serde_json::json!({
        "id": "c1", "short_hash": "6cd1130", "body": "", "body_segments": [],
        "author": { "id": "u1", "handle": "grace", "name": "Grace", "type": "user" },
        "created_at": "2026-11-12T15:10:40.000000Z", "status": "approved",
        "html_url": "https://dev-null.sh/@grace/commits/c1",
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
        url: None,
        starred: false,
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
            author: Some(AccountRef::new("grace", Some(AccountKind::User))),
            url: Some("https://dev-null.sh/@grace/commits/c1".into()),
            starred: false
        })
    );
    assert_eq!(
        commit_block(&deleted, NOW).target,
        Some(Target::Commit {
            id: "d1".into(),
            author: None,
            url: None,
            starred: false
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

fn targets_on(content: Content, row: usize) -> Vec<Target> {
    let whole = content.target.clone();
    let mut found = targets(content, row);
    if let Some(whole) = whole {
        found.retain(|target| *target != whole);
    }
    found
}

#[test]
fn a_person_s_header_says_who_they_are_and_its_counts_lead_to_their_lists() {
    let profile: Profile = serde_json::from_value(serde_json::json!({
        "data": { "id": "u1", "handle": "ada", "name": "Ada", "bio": "Counts things\nand more",
                  "company": "Acme", "location": "London", "website": "https://ada.dev",
                  "commits_count": 42, "followers_count": 10, "following_count": 7 },
        "is_following": true, "follows_you": true, "streaks": { "current": 3, "longest": 12 }
    }))
    .expect("profile");

    let header = profile_header(&profile);

    assert_eq!(
        header.target,
        Some(Target::Account(AccountRef::new(
            "ada",
            Some(AccountKind::User)
        )))
    );
    assert_eq!(
        rows(header.clone()),
        [
            "Ada @ada",
            "Counts things",
            "and more",
            "Acme · London · https://ada.dev",
            "42 commits · 10 followers · 7 following",
            "Follows you · Following",
            "Streak 3 days · longest 12",
            ""
        ]
    );
    assert_eq!(
        targets_on(header.clone(), 3),
        [Target::Url("https://ada.dev".into())]
    );
    assert_eq!(
        targets_on(header, 4),
        [
            Target::Buffer("@ada/followers".into()),
            Target::Buffer("@ada/following".into())
        ]
    );
}

#[test]
fn an_empty_profile_shows_only_who_it_is() {
    let profile: Profile =
        serde_json::from_value(serde_json::json!({ "data": { "handle": "ada" } }))
            .expect("profile");

    assert_eq!(rows(profile_header(&profile)), ["@ada", ""]);
}

#[test]
fn an_organization_s_header_names_its_admins_and_leads_to_its_mentions() {
    let page: OrganizationPage = serde_json::from_value(serde_json::json!({
        "data": { "handle": "acme", "name": "Acme", "bio": "We make things", "location": "Paris",
                  "followers_count": 12, "admins": [{ "id": "u1", "handle": "ada", "name": "Ada" }] },
        "is_following": true
    }))
    .expect("organization");

    let header = organization_header(&page);

    assert_eq!(
        rows(header.clone()),
        [
            "Acme (org) @acme",
            "We make things",
            "Paris",
            "12 followers · mentions",
            "Admins: @ada",
            "Following",
            ""
        ]
    );
    assert_eq!(
        targets_on(header.clone(), 3),
        [Target::Buffer("@acme/mentions".into())]
    );
    assert_eq!(
        targets_on(header, 4),
        [Target::Account(AccountRef::new(
            "ada",
            Some(AccountKind::User)
        ))]
    );
}

#[test]
fn a_pinned_commit_says_so_above_it() {
    let pinned = pinned_block(
        &commit(serde_json::json!({ "body_segments": [text("hello")] })),
        NOW,
    );

    assert_eq!(
        rows(pinned)[..3],
        ["📌 Pinned", "Grace @grace · 3m · 6cd1130", "hello"]
    );
    let body: String = (1..=10)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let long = pinned_block(
        &commit(serde_json::json!({ "body_segments": [text(&body)] })),
        NOW,
    );
    assert_eq!(
        rows(long)[7..9],
        ["line 6", "… 4 more lines"],
        "folded as in the log"
    );
}

#[test]
fn an_account_in_a_list_is_its_name_relationship_and_bio() {
    let account: Account = serde_json::from_value(serde_json::json!({
        "id": "u2", "handle": "lin", "name": "Lin", "bio": "Writes\ncode", "follows_you": true, "is_followed": false
    }))
    .expect("account");

    let block = account_block(&account);

    assert_eq!(
        rows(block.clone()),
        ["Lin @lin · follows you", "Writes", ""]
    );
    assert_eq!(
        block.target,
        Some(Target::Account(AccountRef::new("lin", None)))
    );
}

#[test]
fn a_link_card_carries_what_the_server_says_about_the_link() {
    let block = commit_block(
        &commit(serde_json::json!({
            "body_segments": [text("look")],
            "link_preview": { "url": "https://example.org", "title": "Example", "site_name": "example.org",
                              "description": "An example", "is_pending": false }
        })),
        NOW,
    );

    assert_eq!(
        targets(block, 2),
        [Target::LinkCard(LinkCard {
            url: "https://example.org".into(),
            title: "Example".into(),
            site: "example.org".into(),
            description: "An example".into(),
        })]
    );
}

fn notification(json: serde_json::Value) -> devnull_api::model::Notification {
    let mut base = serde_json::json!({
        "id": "n1", "group_id": "g1", "type": "starred", "created_at": "2026-11-12T15:00:40Z",
        "actor": { "id": "u2", "handle": "lin", "name": "Lin" }, "event_count": 1, "actors_count": 1,
        "commit": { "id": "c1", "short_hash": "6cd1130", "body_segments": [text("my words")],
                    "author": { "id": "u1", "handle": "ada", "name": "Ada" }, "created_at": "2026-11-12T14:00:00Z" }
    });
    if let (Some(base), Some(extra)) = (base.as_object_mut(), json.as_object()) {
        base.extend(extra.clone());
    }
    serde_json::from_value(base).expect("notification")
}

#[test]
fn a_star_says_who_starred_which_commit_of_yours() {
    let block = notification_block(&notification(serde_json::json!({})), false, NOW);

    assert_eq!(
        rows(block.clone()),
        [
            "  ★ Lin @lin starred your commit · 13m",
            "    ┃ my words",
            ""
        ]
    );
    assert_eq!(targets(block, 0), [account("lin", AccountKind::User)]);
}

#[test]
fn a_new_notification_is_marked_in_its_gutter() {
    let block = notification_block(&notification(serde_json::json!({})), true, NOW);

    assert_eq!(rows(block)[0], "● ★ Lin @lin starred your commit · 13m");
}

#[test]
fn a_reply_shows_what_was_replied_and_leads_to_it() {
    let reply = notification(serde_json::json!({
        "type": "replied",
        "commit": { "id": "r1", "short_hash": "aaaaaaa", "body_segments": [text("one\ntwo\nthree\nfour")],
                    "author": { "id": "u2", "handle": "lin", "name": "Lin" }, "created_at": "2026-11-12T15:00:40Z" }
    }));

    let block = notification_block(&reply, false, NOW);

    assert_eq!(
        rows(block.clone()),
        [
            "  ↩ Lin @lin replied to your commit · 13m",
            "    one",
            "    two",
            "    three",
            "    …",
            ""
        ]
    );
    assert!(matches!(block.target, Some(Target::Commit { ref id, .. }) if id == "r1"));
}

#[test]
fn a_follow_leads_to_the_follower() {
    let follow = notification(serde_json::json!({ "type": "followed", "commit": null }));

    let block = notification_block(&follow, false, NOW);

    assert_eq!(rows(block.clone()), ["  + Lin @lin followed you · 13m", ""]);
    assert_eq!(
        block.target,
        Some(Target::Account(AccountRef::new(
            "lin",
            Some(AccountKind::User)
        )))
    );
}

#[test]
fn a_group_names_the_latest_and_folds_the_others() {
    let group = notification(serde_json::json!({
        "event_count": 3, "actors_count": 3,
        "actors": [{ "id": "u2", "handle": "lin", "name": "Lin" }, { "id": "u3", "handle": "kim", "name": "Kim" },
                   { "id": "u4", "handle": "bo", "name": "Bo" }]
    }));

    let block = notification_block(&group, false, NOW);

    assert_eq!(
        rows(block.clone()),
        [
            "  ★ Lin @lin and 2 others starred your commit · 13m",
            "    ┃ my words",
            "    ▸ 3 stars",
            ""
        ]
    );
    let mut doc = Document::new();
    doc.push_back("n", block);
    doc.fold_all(true);
    let layout = Layout::new(&doc, 200);
    let open: Vec<String> = (0..layout.row_count())
        .map(|row| layout.segments(row).map(|(text, _)| text).collect())
        .collect();
    assert_eq!(open[2..5], ["    Lin @lin", "    Kim @kim", "    Bo @bo"]);
}

#[test]
fn a_moderation_decision_says_what_and_why() {
    let rejected =
        notification(serde_json::json!({ "type": "rejected", "actor": null, "reason": "spam" }));

    assert_eq!(
        rows(notification_block(&rejected, false, NOW)),
        [
            "  × Your commit was rejected · 13m",
            "    ┃ my words",
            "    reason: spam",
            ""
        ]
    );
}
