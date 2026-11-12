//! The client against a mock dev-null.sh that verifies every signature like the real server.

use std::path::Path;

use ssh_encoding::Decode;
use ssh_key::{PublicKey, SshSig};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::model::{AccountKind, AccountPage, Lane, NotificationKind, Segment};
use crate::signing::{Signer, canonical_message};
use crate::{Answering, Category, Draft, OrganizationTab, People};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Matches only requests whose `X-SSH-Signature` verifies, with the test key, over the canonical message rebuilt from
/// the request as received.
#[derive(Clone)]
struct Signed(PublicKey);

impl Match for Signed {
    fn matches(&self, request: &Request) -> bool {
        let header = |name| {
            request
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let (Some(timestamp), Some(nonce), Some(signature)) = (
            header("x-devnull-timestamp"),
            header("x-devnull-nonce"),
            header("x-ssh-signature"),
        ) else {
            return false;
        };
        let Ok(timestamp) = timestamp.parse() else {
            return false;
        };
        let Ok(nonce) = (0..32)
            .step_by(2)
            .map(|i| u8::from_str_radix(nonce.get(i..i + 2).unwrap_or("zz"), 16))
            .collect::<Result<Vec<u8>, _>>()
        else {
            return false;
        };
        let Ok(nonce) = <[u8; 16]>::try_from(nonce) else {
            return false;
        };
        let uri = match request.url.query() {
            Some(query) => format!("{}?{query}", request.url.path()),
            None => request.url.path().to_owned(),
        };
        let body = String::from_utf8_lossy(&request.body);
        let message = canonical_message(timestamp, &nonce, request.method.as_str(), &uri, &[&body]);
        let Ok(blob) =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature)
        else {
            return false;
        };
        let Ok(sig) = SshSig::decode(&mut blob.as_slice()) else {
            return false;
        };
        self.0
            .verify("devnull-api", message.as_bytes(), &sig)
            .is_ok()
    }
}

async fn server_and_client() -> (MockServer, Client, Signed) {
    let server = MockServer::start().await;
    let signer = Signer::open(&fixture("signing/device_ed25519"), None).expect("test key");
    let public =
        PublicKey::read_openssh_file(&fixture("signing/device_ed25519.pub")).expect("public key");
    let client = Client::new(&server.uri(), signer).expect("loopback origin");
    (server, client, Signed(public))
}

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/json")
}

#[tokio::test]
async fn me_is_a_signed_get_and_carries_the_unread_count() {
    let (server, client, signed) = server_and_client().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .and(signed)
        .respond_with(json(
            r#"{"data":{"id":"u1","handle":"ada","name":"Ada","terms_accepted":true,"terms_version":"v1"},
                "unread_notifications_count":4}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let me = client.me().await.expect("me");

    assert_eq!(
        (
            me.handle.as_str(),
            me.terms_accepted,
            me.unread_notifications
        ),
        ("ada", true, 4)
    );
}

#[tokio::test]
async fn a_log_page_parses_commits_and_its_next_cursor() {
    let (server, client, signed) = server_and_client().await;
    let page = std::fs::read_to_string(fixture("api/log-page.json")).expect("fixture");
    Mock::given(method("GET"))
        .and(path("/api/v1/log/latest"))
        .and(signed)
        .respond_with(json(&page))
        .mount(&server)
        .await;

    let page = client.log(Log::Latest, None).await.expect("page");

    assert_eq!(page.items.len(), 2);
    let commit = &page.items[0];
    let author = commit.author.as_ref().expect("author");
    assert_eq!(
        (author.handle.as_str(), author.kind, &author.lane),
        ("grace", AccountKind::User, &Lane::Color("#ff0000".into()))
    );
    assert_eq!(commit.mentions[0].offset, 6);
    assert!(matches!(&commit.body_segments[1], Segment::Code { code } if code == "x"));
    assert!(
        matches!(&commit.body_segments[3], Segment::Block { language: Some(l), .. } if l == "rust")
    );
    assert!(matches!(commit.body_segments[4], Segment::Unknown));
    let quoted = commit.quoted_commit.as_ref().expect("quote");
    assert_eq!(
        (
            quoted.author.as_ref().map(|a| a.kind),
            quoted.author.as_ref().map(|a| &a.lane)
        ),
        (Some(AccountKind::Organization), Some(&Lane::Index(3)))
    );
    // Real responses send null for some strings (avatar, card title and description): they read as empty.
    assert_eq!(
        commit.link_preview.as_ref().map(|p| p.title.as_str()),
        Some("")
    );
    assert_eq!(
        quoted.author.as_ref().map(|a| a.avatar_url.as_str()),
        Some("")
    );
    // A deleted reply comes as little more than its id.
    let replies = &commit.reply_preview.as_ref().expect("replies").data;
    assert_eq!(replies.len(), 2);
    assert!(replies[1].is_deleted() && replies[1].short_hash.is_empty());
    assert!(page.items[1].is_deleted() && page.items[1].author.is_none());
    assert!(page.next.is_some());
}

#[tokio::test]
async fn the_next_page_is_asked_for_with_the_cursor_on_the_same_log() {
    let (server, client, signed) = server_and_client().await;
    let page = std::fs::read_to_string(fixture("api/log-page.json")).expect("fixture");
    Mock::given(method("GET"))
        .and(path("/api/v1/log/latest"))
        .and(query_param("cursor", "eyJpZCI6IngifQ"))
        .and(signed)
        .respond_with(json(
            r#"{"data":[],"links":{"next":null},"meta":{"next_cursor":null}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/log/latest"))
        .respond_with(json(&page))
        .mount(&server)
        .await;

    let first = client.log(Log::Latest, None).await.expect("first page");
    let second = client
        .log(Log::Latest, first.next.as_ref())
        .await
        .expect("second page");

    assert!(second.items.is_empty() && second.next.is_none());
}

async fn error_for(status: u16, body: &str, header: Option<(&str, &str)>) -> ApiError {
    let (server, client, _) = server_and_client().await;
    let mut response =
        ResponseTemplate::new(status).set_body_raw(body.to_owned(), "application/json");
    if let Some((name, value)) = header {
        response = response.insert_header(name, value);
    }
    Mock::given(method("GET"))
        .respond_with(response)
        .mount(&server)
        .await;
    client.me().await.expect_err("an error")
}

#[tokio::test]
async fn refusals_become_errors_a_caller_can_act_on() {
    let not_approved = r#"{"message":"This key isn't approved","code":"device_not_approved"}"#;
    assert!(matches!(
        error_for(401, not_approved, None).await,
        ApiError::NotApproved
    ));
    let expired = r#"{"message":"Signature expired. Check that your clock is right.","code":"invalid_signature"}"#;
    assert!(
        matches!(error_for(401, expired, None).await, ApiError::BadSignature { message } if message.contains("clock"))
    );
    assert!(matches!(
        error_for(404, r#"{"message":"No query results"}"#, None).await,
        ApiError::NotFound
    ));
    let limited = error_for(
        429,
        r#"{"message":"Too many"}"#,
        Some(("retry-after", "30")),
    )
    .await;
    assert!(
        matches!(limited, ApiError::RateLimited { retry_after: Some(ref after) } if after == "30"),
        "{limited:?}"
    );
    let refused = error_for(422, r#"{"message":"No.","code":"nope"}"#, None).await;
    assert!(
        matches!(refused, ApiError::Refused { status: 422, ref message, .. } if message == "No."),
        "{refused:?}"
    );
    assert!(matches!(
        error_for(200, "not json", None).await,
        ApiError::Invalid(_)
    ));
}

#[test]
fn only_https_origins_are_accepted_except_on_loopback() {
    let signer = || Signer::open(&fixture("signing/device_ed25519"), None).expect("test key");

    assert!(Client::new("https://dev-null.sh", signer()).is_ok());
    assert!(Client::new("http://127.0.0.1:8080", signer()).is_ok());
    assert!(Client::new("http://dev-null.sh", signer()).is_err());
    assert!(Client::new("https://dev-null.sh/some/path", signer()).is_err());
}

#[tokio::test]
async fn accepting_terms_is_a_signed_put_that_must_be_confirmed() {
    let (server, client, signed) = server_and_client().await;
    let accepted =
        r#"{"data":{"id":"u1","handle":"ada","terms_accepted":true,"terms_version":"v2"}}"#;
    Mock::given(method("PUT"))
        .and(path("/api/v1/me/terms"))
        .and(wiremock::matchers::body_json(
            serde_json::json!({ "version": "v2" }),
        ))
        .and(signed)
        .respond_with(json(accepted))
        .expect(1)
        .mount(&server)
        .await;
    // Any other acceptance gets the same answer, confirming v2 rather than what was asked.
    Mock::given(method("PUT"))
        .and(path("/api/v1/me/terms"))
        .respond_with(json(accepted))
        .mount(&server)
        .await;

    assert!(
        client
            .accept_terms("v2")
            .await
            .expect("accepted")
            .terms_accepted
    );
    assert!(matches!(
        client.accept_terms("v3").await,
        Err(ApiError::Invalid(_))
    ));
}

fn commit_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "short_hash": "0000000", "body": "", "body_segments": [], "created_at": "2026-11-12T15:13:40Z",
        "author": { "id": "u1", "handle": "ada", "name": "Ada", "type": "user" }, "images": []
    })
}

#[tokio::test]
async fn a_thread_is_the_commit_and_its_ancestors_and_replies_come_in_pages() {
    let (server, client, signed) = server_and_client().await;
    let thread = serde_json::json!({ "data": commit_json("c-2"), "ancestors": [commit_json("c-1")], "reply_participants": [] });
    Mock::given(method("GET"))
        .and(path("/api/v1/commits/c-2"))
        .and(signed)
        .respond_with(json(&thread.to_string()))
        .mount(&server)
        .await;
    let replies =
        serde_json::json!({ "data": [commit_json("c-3")], "meta": { "next_cursor": "r2" } });
    Mock::given(method("GET"))
        .and(path("/api/v1/commits/c-2/replies"))
        .respond_with(json(&replies.to_string()))
        .mount(&server)
        .await;

    let thread = client.thread("c-2").await.expect("thread");
    let replies = client.replies("c-2", None).await.expect("replies");

    assert_eq!(
        (thread.commit.id.as_str(), thread.ancestors.len()),
        ("c-2", 1)
    );
    assert_eq!(replies.items[0].id, "c-3");
    assert!(replies.next.is_some());
}

#[tokio::test]
async fn a_profile_has_the_account_its_relationship_and_pinned_commit() {
    let (server, client, signed) = server_and_client().await;
    let profile = serde_json::json!({
        "data": { "id": "u1", "handle": "ada", "name": "Ada", "bio": null, "company": "Acme", "location": "Lisbon",
                  "website": "https://ada.dev", "commits_count": 77, "followers_count": 197, "following_count": 48 },
        "is_following": true, "follows_you": false, "pinned_commit": commit_json("p1"),
        "streaks": { "current": 3, "longest": 9 }
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/users/ada"))
        .and(signed)
        .respond_with(json(&profile.to_string()))
        .mount(&server)
        .await;
    let commits =
        serde_json::json!({ "data": [commit_json("c1")], "meta": { "next_cursor": null } });
    Mock::given(method("GET"))
        .and(path("/api/v1/users/ada/commits"))
        .and(query_param("replies", "1"))
        .respond_with(json(&commits.to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let profile = client.profile("ada").await.expect("profile");
    let replies = client
        .commits_of("ada", true, None)
        .await
        .expect("replies tab");

    assert_eq!(
        (
            profile.account.followers_count,
            profile.is_following,
            profile.follows_you
        ),
        (Some(197), true, false)
    );
    assert_eq!(profile.account.bio, "");
    assert_eq!(
        profile.pinned.map(|commit| commit.id),
        Some("p1".to_owned())
    );
    assert_eq!(replies.items.len(), 1);
}

#[tokio::test]
async fn followers_and_following_are_pages_of_accounts() {
    let (server, client, _) = server_and_client().await;
    let people = serde_json::json!({ "data": [{ "id": "u2", "handle": "lin", "name": "Lin", "follows_you": true,
        "is_followed": false, "avatar_url": "https://example.org/lin.png" }], "meta": {} });
    Mock::given(method("GET"))
        .and(path("/api/v1/users/ada/followers"))
        .respond_with(json(&people.to_string()))
        .mount(&server)
        .await;

    let followers = client
        .people("ada", People::Followers, None)
        .await
        .expect("followers");

    assert_eq!(
        (
            followers.items[0].handle.as_str(),
            followers.items[0].follows_you
        ),
        ("lin", Some(true))
    );
    assert_eq!(followers.items[0].avatar_url, "https://example.org/lin.png");
}

#[tokio::test]
async fn an_organization_has_its_admins_and_tabs() {
    let (server, client, _) = server_and_client().await;
    let org = serde_json::json!({
        "data": { "handle": "acme", "name": "Acme", "bio": "We make things", "followers_count": 12,
                  "avatar_url": "https://example.org/acme.png",
                  "admins": [{ "id": "u1", "handle": "ada", "name": "Ada" }] },
        "is_following": false, "can_claim": false, "settings_url": null
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme"))
        .respond_with(json(&org.to_string()))
        .mount(&server)
        .await;
    let page = serde_json::json!({ "data": [commit_json("m1")], "meta": {} });
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/mentions"))
        .respond_with(json(&page.to_string()))
        .mount(&server)
        .await;

    let org = client.organization("acme").await.expect("organization");
    let mentions = client
        .organization_commits("acme", OrganizationTab::Mentions, None)
        .await
        .expect("mentions");

    assert_eq!(
        (
            org.organization.name.as_str(),
            org.organization.admins.len(),
            org.is_following
        ),
        ("Acme", 1, false)
    );
    assert_eq!(mentions.items[0].id, "m1");
    assert_eq!(org.organization.avatar_url, "https://example.org/acme.png");
}

#[tokio::test]
async fn an_account_is_a_person_or_else_an_organization() {
    let (server, client, _) = server_and_client().await;
    let person = serde_json::json!({ "data": { "id": "u1", "handle": "ada", "name": "Ada" } });
    Mock::given(method("GET"))
        .and(path("/api/v1/users/ada"))
        .respond_with(json(&person.to_string()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/acme"))
        .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"message":"Not found."}"#))
        .mount(&server)
        .await;
    let org = serde_json::json!({ "data": { "handle": "acme", "name": "Acme" } });
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme"))
        .respond_with(json(&org.to_string()))
        .mount(&server)
        .await;

    let ada = client.account("ada").await.expect("ada");
    let acme = client.account("acme").await.expect("acme");
    let nobody = client.account("nobody").await;

    assert!(matches!(ada, AccountPage::Person(profile) if profile.account.name == "Ada"));
    assert!(matches!(acme, AccountPage::Organization(page) if page.organization.name == "Acme"));
    assert!(matches!(nobody, Err(ApiError::NotFound)), "{nobody:?}");
}

#[tokio::test]
async fn a_profile_search_finds_people_then_organizations() {
    let (server, client, signed) = server_and_client().await;
    let found = serde_json::json!({
        "users": [{ "id": "u1", "handle": "ada", "name": "Ada", "is_followed": true, "follows_you": false }],
        "organizations": [{ "handle": "adacorp", "name": "Ada Corp", "is_followed": false }]
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/search/profiles"))
        .and(query_param("q", "ada lo"))
        .and(signed)
        .respond_with(json(&found.to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let found = client.search_profiles("  ada lo ").await.expect("search");

    assert_eq!(
        (found.people[0].handle.as_str(), found.people[0].is_followed),
        ("ada", Some(true))
    );
    assert_eq!(found.organizations[0].handle, "adacorp");
}

#[tokio::test]
async fn a_profile_search_longer_than_the_server_takes_is_refused_without_a_request() {
    let (server, client, _) = server_and_client().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let long = "é".repeat(101);

    assert!(matches!(
        client.search_profiles(&long).await,
        Err(ApiError::Invalid(_))
    ));
}

#[tokio::test]
async fn the_inbox_is_a_page_of_notifications_filtered_by_category_with_when_it_was_read() {
    let (server, client, signed) = server_and_client().await;
    let inbox = serde_json::json!({
        "data": [{
            "id": "n1", "group_id": "g1", "type": "starred", "reason": null, "read_at": null,
            "created_at": "2026-11-12T15:00:00.000000Z", "actor": { "id": "u2", "handle": "lin", "name": "Lin" },
            "actors": [{ "id": "u2", "handle": "lin", "name": "Lin" }, { "id": "u3", "handle": "kim", "name": "Kim" }],
            "event_count": 2, "actors_count": 2, "commit": commit_json("c1")
        }, {
            "id": "n2", "group_id": "g2", "type": "a-kind-from-later", "created_at": "2026-11-12T14:00:00Z",
            "actor": { "id": "u2", "handle": "lin" }
        }],
        "read_at": "2026-11-12T15:13:40.000000Z",
        "meta": { "next_cursor": "n-next" }
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/notifications"))
        .and(query_param("type", "replies,mentions"))
        .and(signed.clone())
        .respond_with(json(&inbox.to_string()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/notifications"))
        .and(query_param("type", "all"))
        .and(query_param("cursor", "n-next"))
        .and(signed)
        .respond_with(json(
            r#"{"data":[],"read_at":"2026-11-12T15:13:40Z","meta":{}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let first = client
        .notifications(&[Category::Replies, Category::Mentions], None)
        .await
        .expect("inbox");
    let next = client
        .notifications(&[], first.page.next.as_ref())
        .await
        .expect("next page");

    let starred = &first.page.items[0];
    assert_eq!(
        (
            starred.kind,
            starred.actors.len(),
            starred.event_count,
            starred.read_at.is_none()
        ),
        (NotificationKind::Starred, 2, 2, true)
    );
    assert_eq!(starred.commit.as_ref().map(|c| c.id.as_str()), Some("c1"));
    assert_eq!(first.page.items[1].kind, NotificationKind::Other);
    assert_eq!(first.read_at, "2026-11-12T15:13:40.000000Z");
    assert!(next.page.items.is_empty() && next.page.next.is_none());
}

#[test]
fn categories_have_the_names_the_inbox_filters_by() {
    for category in Category::ALL {
        assert_eq!(Category::named(category.name()), Some(category));
    }
    assert_eq!(Category::named("bogus"), None);
}

#[tokio::test]
async fn starred_commits_come_in_pages() {
    let (server, client, signed) = server_and_client().await;
    let page = serde_json::json!({ "data": [commit_json("s1")], "meta": { "next_cursor": null } });
    Mock::given(method("GET"))
        .and(path("/api/v1/starred"))
        .and(signed)
        .respond_with(json(&page.to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let starred = client.starred(None).await.expect("starred");

    assert_eq!(starred.items[0].id, "s1");
    assert!(starred.next.is_none());
}

#[tokio::test]
async fn a_preview_is_the_text_as_the_server_would_show_it() {
    let (server, client, signed) = server_and_client().await;
    let preview = serde_json::json!({ "data": {
        "body": "hi @ada", "body_segments": [{ "type": "text", "text": "hi @ada" }], "mentions": []
    } });
    Mock::given(method("POST"))
        .and(path("/api/v1/commits/preview"))
        .and(body_json(serde_json::json!({ "body": "hi @ada" })))
        .and(signed)
        .respond_with(json(&preview.to_string()))
        .expect(1)
        .mount(&server)
        .await;

    let preview = client.preview("hi @ada").await.expect("preview");

    assert_eq!(preview.body, "hi @ada");
    assert_eq!(preview.segments.len(), 1);
}

#[tokio::test]
async fn a_draft_is_published_as_a_commit_a_reply_or_a_quote() {
    let (server, client, signed) = server_and_client().await;
    let created = |id: &str| {
        let mut commit = commit_json(id);
        commit["status"] = "approved".into();
        ResponseTemplate::new(201)
            .set_body_string(serde_json::json!({ "data": commit }).to_string())
    };
    for (payload, id) in [
        (serde_json::json!({ "body": "hello" }), "n1"),
        (
            serde_json::json!({ "body": "agreed", "parent_id": "p1" }),
            "n2",
        ),
        (
            serde_json::json!({ "body": "look", "quoted_commit_id": "q1" }),
            "n3",
        ),
    ] {
        Mock::given(method("POST"))
            .and(path("/api/v1/commits"))
            .and(body_json(payload))
            .and(signed.clone())
            .respond_with(created(id))
            .expect(1)
            .mount(&server)
            .await;
    }

    let commit = client
        .publish(&Draft::new(Answering::Nothing, "hello"))
        .await
        .expect("commit");
    let reply = client
        .publish(&Draft::new(Answering::Reply("p1".into()), "agreed"))
        .await
        .expect("reply");
    let quote = client
        .publish(&Draft::new(Answering::Quote("q1".into()), "look"))
        .await
        .expect("quote");

    assert_eq!([commit.id, reply.id, quote.id], ["n1", "n2", "n3"]);
}

#[tokio::test]
async fn a_draft_posted_as_an_organization_names_it_by_handle() {
    let (server, client, signed) = server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/commits"))
        .and(body_json(
            serde_json::json!({ "body": "news", "parent_id": "p1", "author": "acme" }),
        ))
        .and(signed)
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_string(serde_json::json!({ "data": commit_json("n1") }).to_string()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let mut draft = Draft::new(Answering::Reply("p1".into()), "news");
    draft.author = Some("acme".into());

    let commit = client.publish(&draft).await.expect("published");

    assert_eq!(commit.id, "n1");
    draft.author = Some("../me".into());
    assert!(
        matches!(client.publish(&draft).await, Err(ApiError::Invalid(_))),
        "not a handle"
    );
}

#[tokio::test]
async fn stars_follows_and_deletions_are_signed_writes() {
    let (server, client, signed) = server_and_client().await;
    let starred = |on: bool| {
        let mut commit = commit_json("c1");
        commit["starred"] = on.into();
        json(&serde_json::json!({ "data": commit }).to_string())
    };
    for (verb, route, response) in [
        ("POST", "/api/v1/commits/c1/star", starred(true)),
        ("DELETE", "/api/v1/commits/c1/star", starred(false)),
        (
            "POST",
            "/api/v1/users/ada/follow",
            ResponseTemplate::new(204),
        ),
        (
            "DELETE",
            "/api/v1/users/ada/follow",
            ResponseTemplate::new(204),
        ),
        (
            "POST",
            "/api/v1/organizations/acme/follow",
            ResponseTemplate::new(204),
        ),
        ("DELETE", "/api/v1/commits/c1", ResponseTemplate::new(204)),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .and(signed.clone())
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }

    assert!(client.star("c1").await.expect("star").starred);
    assert!(!client.unstar("c1").await.expect("unstar").starred);
    client
        .follow("ada", AccountKind::User)
        .await
        .expect("follow");
    client
        .unfollow("ada", AccountKind::User)
        .await
        .expect("unfollow");
    client
        .follow("acme", AccountKind::Organization)
        .await
        .expect("follow an organization");
    client.delete_commit("c1").await.expect("delete");
}

#[tokio::test]
async fn a_star_the_server_does_not_confirm_is_an_error() {
    let (server, client, _) = server_and_client().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/commits/c1/star"))
        .respond_with(json(
            &serde_json::json!({ "data": commit_json("c1") }).to_string(),
        ))
        .mount(&server)
        .await;

    assert!(matches!(client.star("c1").await, Err(ApiError::Invalid(_))));
}

#[tokio::test]
async fn handles_and_ids_that_could_leave_their_path_are_refused_without_a_request() {
    let (server, client, _) = server_and_client().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    for bad in ["../me", "ada/followers", "a?b", "", "a b"] {
        assert!(
            matches!(client.profile(bad).await, Err(ApiError::Invalid(_))),
            "{bad:?}"
        );
        assert!(
            matches!(client.thread(bad).await, Err(ApiError::Invalid(_))),
            "{bad:?}"
        );
    }
}
