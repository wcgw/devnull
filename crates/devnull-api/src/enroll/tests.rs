//! Enrollment against a mock dev-null.sh, with the browser played by plain HTTP requests.

use std::path::Path;

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn test_key() -> Signer {
    let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/signing/device_ed25519");
    Signer::open(&key, None).expect("test key")
}

fn query(url: &str) -> std::collections::HashMap<String, String> {
    Url::parse(url)
        .expect("url")
        .query_pairs()
        .into_owned()
        .collect()
}

/// What the browser does after approval: follows the redirect to the loopback listener.
async fn browser_returns(to: &str) -> (u16, String) {
    let response = reqwest::get(to).await.expect("loopback reachable");
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn the_authorize_url_asks_for_this_key_with_pkce_and_a_loopback_return() {
    let enrollment = Enrollment::start("https://dev-null.sh", test_key(), "dn")
        .await
        .expect("start");

    let url = enrollment.authorize_url().to_owned();
    assert!(url.starts_with("https://dev-null.sh/mobile/authorize?"));
    let params = query(&url);
    assert_eq!(params["fingerprint"], enrollment.fingerprint());
    assert_eq!(params["device_name"], "dn");
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["code_challenge"], challenge(&enrollment.verifier));
    assert_eq!(params["state"].len(), 32);
    assert!(
        params["redirect_uri"].starts_with("http://127.0.0.1:")
            && params["redirect_uri"].ends_with("/callback")
    );
}

#[tokio::test]
async fn the_browser_returning_with_a_code_completes_enrollment() {
    let server = MockServer::start().await;
    let enrollment = Enrollment::start(&server.uri(), test_key(), "dn")
        .await
        .expect("start");
    let params = query(enrollment.authorize_url());
    let me = r#"{"data":{"id":"u1","handle":"ada","terms_accepted":true},"unread_notifications_count":0}"#;
    Mock::given(method("POST"))
        .and(path("/api/v1/devices"))
        .and(body_json(
            serde_json::json!({ "code": "the-code", "code_verifier": enrollment.verifier.clone() }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(me, "application/json"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(me, "application/json"))
        .mount(&server)
        .await;

    let callback = format!(
        "{}?code=the-code&state={}",
        params["redirect_uri"], params["state"]
    );
    let (waited, (status, page)) = tokio::join!(enrollment.wait(), browser_returns(&callback));
    assert_eq!(
        (status, page.contains("return to your terminal")),
        (200, true)
    );
    let (client, me) = enrollment
        .redeem(&waited.expect("code"))
        .await
        .expect("enrolled");

    assert_eq!(me.handle, "ada");
    assert_eq!(client.fingerprint(), enrollment_fingerprint());
}

fn enrollment_fingerprint() -> String {
    test_key().fingerprint()
}

#[tokio::test]
async fn a_return_with_the_wrong_state_or_host_is_turned_away_and_the_wait_goes_on() {
    let enrollment = Enrollment::start("https://dev-null.sh", test_key(), "dn")
        .await
        .expect("start");
    let params = query(enrollment.authorize_url());
    let redirect = params["redirect_uri"].clone();
    let wrong_state = format!("{redirect}?code=x&state=00000000000000000000000000000000");
    let wrong_host = redirect.replace("127.0.0.1", "localhost");
    let right = format!("{redirect}?code=good&state={}", params["state"]);

    let browser = async {
        let refused = browser_returns(&wrong_state).await.0;
        let elsewhere = reqwest::Client::new()
            .get(&wrong_host)
            .send()
            .await
            .map(|response| response.status().as_u16())
            .unwrap_or(0);
        let accepted = browser_returns(&right).await.0;
        (refused, elsewhere, accepted)
    };
    let (code, (refused, elsewhere, accepted)) = tokio::join!(enrollment.wait(), browser);

    assert_eq!((refused, accepted), (400, 200));
    assert_ne!(elsewhere, 200);
    assert_eq!(code.expect("code"), "good");
}

#[tokio::test]
async fn cancelling_in_the_browser_is_reported() {
    let enrollment = Enrollment::start("https://dev-null.sh", test_key(), "dn")
        .await
        .expect("start");
    let params = query(enrollment.authorize_url());
    let denied = format!(
        "{}?error=access_denied&state={}",
        params["redirect_uri"], params["state"]
    );

    let (code, _) = tokio::join!(enrollment.wait(), browser_returns(&denied));

    assert!(matches!(code, Err(EnrollError::Denied)), "{code:?}");
}

#[tokio::test]
async fn a_pasted_callback_url_gives_the_code_when_it_belongs_to_this_attempt() {
    let enrollment = Enrollment::start("https://dev-null.sh", test_key(), "dn")
        .await
        .expect("start");
    let params = query(enrollment.authorize_url());
    let redirect = &params["redirect_uri"];

    let pasted = format!("  {redirect}?code=abc&state={}  ", params["state"]);
    assert_eq!(enrollment.code_from_url(&pasted).expect("code"), "abc");
    let other = format!("{redirect}?code=abc&state=ffffffffffffffffffffffffffffffff");
    assert!(matches!(
        enrollment.code_from_url(&other),
        Err(EnrollError::BadCallback)
    ));
    assert!(matches!(
        enrollment.code_from_url("https://evil.example/callback?code=abc"),
        Err(EnrollError::BadCallback)
    ));
}

#[tokio::test]
async fn a_pasted_link_must_name_the_loopback_address_exactly() {
    let enrollment = Enrollment::start("https://dev-null.sh", test_key(), "dn")
        .await
        .expect("start");
    let params = query(enrollment.authorize_url());
    let elsewhere = params["redirect_uri"].replace("127.0.0.1", "localhost");

    let pasted = format!("{elsewhere}?code=abc&state={}", params["state"]);

    assert!(matches!(
        enrollment.code_from_url(&pasted),
        Err(EnrollError::BadCallback)
    ));
}
