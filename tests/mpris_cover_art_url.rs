//! build_cover_art_url: signed URL construction for the MPRIS metadata field.

use ferrosonic::config::Config;
use ferrosonic::mpris::server::build_cover_art_url;

fn cfg(base_url: &str) -> Config {
    let mut c = Config::new();
    c.base_url = base_url.to_string();
    c.username = "user".into();
    c.password = "pw".into();
    c
}

#[test]
fn empty_base_url_returns_none() {
    let url = build_cover_art_url(&cfg(""), "art-1");
    assert!(url.is_none());
}

#[test]
fn empty_cover_art_id_returns_none() {
    let url = build_cover_art_url(&cfg("https://example.com"), "");
    assert!(url.is_none());
}

#[test]
fn typical_url_contains_id_and_auth_params() {
    let url = build_cover_art_url(&cfg("https://example.com"), "art-42").unwrap();
    assert!(url.contains("/rest/getCoverArt"));
    assert!(url.contains("id=art-42"));
    assert!(url.contains("u=user"));
    assert!(url.contains("t="));
    assert!(url.contains("s="));
    assert!(url.contains("c="));
}

#[test]
fn invalid_base_url_returns_none() {
    let url = build_cover_art_url(&cfg("not a url"), "art-1");
    assert!(url.is_none());
}

#[test]
fn cover_art_id_with_special_chars_is_url_encoded() {
    let url = build_cover_art_url(&cfg("https://x"), "id with spaces").unwrap();
    assert!(
        url.contains("id+with+spaces") || url.contains("id%20with%20spaces"),
        "spaces must be encoded; got {}",
        url
    );
}

#[test]
fn https_base_preserved_in_output() {
    let url = build_cover_art_url(&cfg("https://srv.example.com:8443"), "x").unwrap();
    assert!(url.starts_with("https://srv.example.com:8443"));
}

#[test]
fn http_base_preserved_in_output() {
    let url = build_cover_art_url(&cfg("http://localhost:4040"), "x").unwrap();
    assert!(url.starts_with("http://localhost:4040"));
}

#[test]
fn distinct_passwords_yield_distinct_tokens() {
    let mut c1 = cfg("https://x");
    c1.password = "secret1".into();
    let mut c2 = cfg("https://x");
    c2.password = "secret2".into();
    let u1 = build_cover_art_url(&c1, "art").unwrap();
    let u2 = build_cover_art_url(&c2, "art").unwrap();
    let t1 = u1.split("t=").nth(1).unwrap().split('&').next().unwrap();
    let t2 = u2.split("t=").nth(1).unwrap().split('&').next().unwrap();
    assert_ne!(t1, t2, "auth tokens must differ when passwords differ");
}
