//! Downloading pictures safely.

use std::net::IpAddr;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

async fn server_and_media() -> (MockServer, Media) {
    let server = MockServer::start().await;
    let media = Media::new(&server.uri()).expect("loopback origin");
    (server, media)
}

#[tokio::test]
async fn a_picture_on_the_api_s_own_origin_downloads() {
    let (server, media) = server_and_media().await;
    Mock::given(method("GET"))
        .and(path("/a.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"png bytes".to_vec()))
        .expect(1)
        .mount(&server)
        .await;

    let bytes = media
        .get(&format!("{}/a.png", server.uri()))
        .await
        .expect("picture");

    assert_eq!(bytes, b"png bytes");
}

#[tokio::test]
async fn addresses_off_the_public_internet_or_not_https_are_refused_without_a_request() {
    let (server, media) = server_and_media().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    for url in [
        "http://example.com/a.png",
        "https://user@example.com/a.png",
        "https://10.0.0.1/a.png",
        "https://192.168.1.1/a.png",
        "https://127.0.0.1/a.png",
        "https://[::1]/a.png",
        "https://169.254.169.254/latest/meta-data",
        "file:///etc/passwd",
        "not a url",
    ] {
        assert!(
            matches!(media.get(url).await, Err(MediaError::Refused)),
            "{url}"
        );
    }
}

#[tokio::test]
async fn a_picture_larger_than_the_limit_is_refused() {
    let (server, media) = server_and_media().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0; MAX_BYTES + 1]))
        .mount(&server)
        .await;

    assert!(matches!(
        media.get(&format!("{}/big.png", server.uri())).await,
        Err(MediaError::TooLarge)
    ));
}

#[tokio::test]
async fn a_picture_that_is_not_there_says_so() {
    let (server, media) = server_and_media().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    assert!(matches!(
        media.get(&format!("{}/gone.png", server.uri())).await,
        Err(MediaError::Status(404))
    ));
}

#[tokio::test]
async fn a_redirect_to_an_address_off_limits_is_refused() {
    let (server, media) = server_and_media().await;
    Mock::given(method("GET"))
        .and(path("/moved.png"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", "http://169.254.169.254/x"),
        )
        .mount(&server)
        .await;

    assert!(matches!(
        media.get(&format!("{}/moved.png", server.uri())).await,
        Err(MediaError::Refused)
    ));
}

#[test]
fn only_addresses_on_the_public_internet_are_public() {
    let public = |ip: &str| is_public(ip.parse::<IpAddr>().expect("ip"));
    for ip in ["1.1.1.1", "140.82.112.3", "2606:4700::1111"] {
        assert!(public(ip), "{ip}");
    }
    for ip in [
        "10.1.2.3",
        "172.16.0.1",
        "192.168.0.1",
        "127.0.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "255.255.255.255",
        "192.0.2.1",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "ff02::1",
        "::ffff:10.0.0.1",
        "2001:db8::1",
    ] {
        assert!(!public(ip), "{ip}");
    }
}
