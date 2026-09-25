//! Tell a refused stream from audio.
//!
//! A server refuses `rest/stream` with an HTTP error status or with a
//! Subsonic error document (often at HTTP 200). mpv can only say it could not
//! play the bytes, so the reason comes from here.

use reqwest::header::CONTENT_TYPE;
use tracing::{debug, warn};

/// Most bytes read from a reply that is not audio. A Subsonic error document
/// is a few hundred bytes; the cap stops an endless body holding memory.
const MAX_ERROR_BODY: usize = 64 * 1024;

/// Subsonic error codes that fail every request of this account:
/// credentials (40-44) and permission (50).
const ACCOUNT_WIDE_CODES: [i64; 6] = [40, 41, 42, 43, 44, 50];

/// Why a stream cannot play, worded for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamProblem {
    /// Clause that follows `Cannot play "<title>": `.
    pub summary: String,
    /// The fault blocks every song on this server (credentials, permission,
    /// rate limit, no connection), so moving to the next song cannot help.
    pub every_song: bool,
}

/// `true` when a reply with this status and content type carries audio.
/// A missing content type counts as audio: servers that send raw files
/// sometimes omit it.
///
/// ```
/// use ferrosonic::subsonic::stream_check::is_audio_reply;
/// assert!(is_audio_reply(200, Some("audio/flac")));
/// assert!(is_audio_reply(206, None));
/// assert!(!is_audio_reply(200, Some("text/xml; charset=utf-8")));
/// assert!(!is_audio_reply(404, Some("audio/flac")));
/// ```
#[must_use]
pub fn is_audio_reply(status: u16, content_type: Option<&str>) -> bool {
    (200..300).contains(&status) && !content_type.is_some_and(is_document_type)
}

fn is_document_type(content_type: &str) -> bool {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.starts_with("text/")
        || mime == "application/json"
        || mime == "application/xml"
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
}

/// The problem a stream reply reports, or `None` for audio. `body` is the
/// start of the reply body; it matters only when the reply is not audio.
///
/// ```
/// use ferrosonic::subsonic::stream_check::classify_stream_reply;
/// assert_eq!(classify_stream_reply(200, Some("audio/mpeg"), b""), None);
/// let p = classify_stream_reply(429, None, b"").unwrap();
/// assert_eq!(p.summary, "the server refused it: HTTP 429 Too Many Requests");
/// assert!(p.every_song);
/// ```
#[must_use]
pub fn classify_stream_reply(
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
) -> Option<StreamProblem> {
    if is_audio_reply(status, content_type) {
        return None;
    }
    Some(refusal(status, content_type, body))
}

/// The problem of a reply already known not to be audio.
fn refusal(status: u16, content_type: Option<&str>, body: &[u8]) -> StreamProblem {
    let api = parse_api_error(body);
    let summary = match &api {
        Some((code, message)) if message.is_empty() => {
            format!("the server refused it: Subsonic error {code}")
        }
        Some((code, message)) => {
            format!("the server refused it: {message} (Subsonic error {code})")
        }
        None if (200..300).contains(&status) => format!(
            "the server sent {} instead of audio",
            content_type.unwrap_or("a document")
        ),
        None => format!("the server refused it: {}", http_status_text(status)),
    };
    let every_song = api
        .as_ref()
        .is_some_and(|(code, _)| ACCOUNT_WIDE_CODES.contains(code))
        || matches!(status, 401 | 403 | 429);
    StreamProblem {
        summary,
        every_song,
    }
}

fn http_status_text(status: u16) -> String {
    let reason = match reqwest::StatusCode::from_u16(status) {
        Ok(code) => code.canonical_reason(),
        Err(e) => {
            debug!("refused stream: status {status} is outside the HTTP range: {e}");
            None
        }
    };
    reason.map_or_else(
        || format!("HTTP {status}"),
        |reason| format!("HTTP {status} {reason}"),
    )
}

/// `(code, message)` from a Subsonic error document in JSON or XML.
fn parse_api_error(body: &[u8]) -> Option<(i64, String)> {
    let text = String::from_utf8_lossy(body);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') {
        let doc: serde_json::Value = match serde_json::from_str(text) {
            Ok(doc) => doc,
            Err(e) => {
                debug!("refused stream: reply body is not valid JSON: {e}");
                return None;
            }
        };
        let inner = doc.get("subsonic-response")?;
        if inner.get("status").and_then(serde_json::Value::as_str) == Some("ok") {
            return None;
        }
        let err = inner.get("error")?;
        let code = err.get("code")?.as_i64()?;
        let message = err
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        return Some((code, message));
    }
    if !text.contains("<subsonic-response") {
        return None;
    }
    let start = text.find("<error")?;
    let tag = &text[start..];
    let tag = &tag[..tag.find('>')?];
    let raw_code = xml_attr(tag, "code")?;
    let code = match raw_code.trim().parse() {
        Ok(code) => code,
        Err(e) => {
            debug!("refused stream: error code {raw_code:?} is not a number: {e}");
            return None;
        }
    };
    let message = xml_attr(tag, "message").unwrap_or_default();
    Some((code, message))
}

/// Value of attribute `name` inside one XML start tag, entities decoded.
fn xml_attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let preceded_by_space = rest[..at].ends_with(char::is_whitespace);
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        let Some(after_eq) = after.strip_prefix('=') else {
            continue;
        };
        if !preceded_by_space {
            continue;
        }
        let after_eq = after_eq.trim_start();
        let quote = after_eq
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')?;
        let value = &after_eq[1..];
        let end = value.find(quote)?;
        return Some(decode_xml_entities(&value[..end]));
    }
}

fn decode_xml_entities(raw: &str) -> String {
    raw.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Pass an audio reply through; read a bounded part of any other reply and
/// return its problem.
///
/// # Errors
/// Returns the `StreamProblem` when the reply is not audio.
pub async fn check_stream_response(
    resp: reqwest::Response,
) -> Result<reqwest::Response, StreamProblem> {
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| match v.to_str() {
            Ok(s) => Some(s.to_owned()),
            Err(e) => {
                debug!("stream reply: content type is not visible ASCII: {e}");
                None
            }
        });
    if is_audio_reply(status, content_type.as_deref()) {
        return Ok(resp);
    }
    let body = read_capped(resp, MAX_ERROR_BODY).await;
    Err(refusal(status, content_type.as_deref(), &body))
}

async fn read_capped(mut resp: reqwest::Response, cap: usize) -> Vec<u8> {
    let mut body = Vec::new();
    while body.len() < cap {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                let take = chunk.len().min(cap - body.len());
                body.extend_from_slice(&chunk[..take]);
            }
            Ok(None) => break,
            Err(e) => {
                debug!("refused stream: body read stopped: {}", e.without_url());
                break;
            }
        }
    }
    body
}

/// Ask the server for `url` once and report why it cannot play. `Ok(())`
/// means the server sends audio, so the fault is not on the server side.
///
/// # Errors
/// Returns the `StreamProblem` for a refusal or for no answer at all.
pub async fn probe_stream(url: &str) -> Result<(), StreamProblem> {
    let client = match reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("stream probe: client build failed ({e}); using defaults");
            reqwest::Client::new()
        }
    };
    let resp = client.get(url).send().await.map_err(no_answer)?;
    check_stream_response(resp).await.map(drop)
}

/// A transport failure, worded without the request URL: the URL of a
/// Subsonic stream carries the auth token and salt.
#[must_use]
pub fn no_answer(err: reqwest::Error) -> StreamProblem {
    let err = err.without_url();
    let mut summary = format!("the server did not answer: {err}");
    let mut source = std::error::Error::source(&err);
    while let Some(cause) = source {
        summary.push_str(": ");
        summary.push_str(&cause.to_string());
        source = cause.source();
    }
    StreamProblem {
        summary,
        every_song: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const XML_40: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<subsonic-response xmlns="http://subsonic.org/restapi" status="failed" version="1.16.1"><error code="40" message="Wrong username or password"/></subsonic-response>"#;

    #[test]
    fn xml_error_at_http_200_is_a_refusal_with_its_message() {
        let p = classify_stream_reply(200, Some("text/xml; charset=utf-8"), XML_40.as_bytes());
        assert_eq!(
            p,
            Some(StreamProblem {
                summary: "the server refused it: Wrong username or password (Subsonic error 40)"
                    .into(),
                every_song: true,
            })
        );
    }

    #[test]
    fn xml_single_quotes_and_entities_decode() {
        let body = r"<subsonic-response status='failed'><error message='Can&apos;t find &quot;x&quot; &amp; y' code='70'/></subsonic-response>";
        let p = classify_stream_reply(200, Some("application/xml"), body.as_bytes()).unwrap();
        assert_eq!(
            p.summary,
            r#"the server refused it: Can't find "x" & y (Subsonic error 70)"#
        );
        assert!(!p.every_song, "a missing song blocks only that song");
    }

    #[test]
    fn xml_attribute_name_inside_another_name_is_not_taken() {
        let body = r#"<subsonic-response><error errorcode="1" code="70" xmessage="no" message="Song not found"/></subsonic-response>"#;
        assert_eq!(
            parse_api_error(body.as_bytes()),
            Some((70, "Song not found".into()))
        );
    }

    #[test]
    fn xml_code_that_is_not_a_number_is_no_api_error() {
        let body = r#"<subsonic-response><error code="x" message="m"/></subsonic-response>"#;
        assert_eq!(parse_api_error(body.as_bytes()), None);
    }

    #[test]
    fn json_error_is_a_refusal_and_json_ok_is_not_an_api_error() {
        let failed = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":50,"message":"User is not authorized"}}}"#;
        let p = classify_stream_reply(200, Some("application/json"), failed).unwrap();
        assert_eq!(
            p.summary,
            "the server refused it: User is not authorized (Subsonic error 50)"
        );
        assert!(p.every_song);
        let ok = br#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;
        assert_eq!(parse_api_error(ok), None);
        assert_eq!(parse_api_error(b"{not json"), None);
    }

    #[test]
    fn empty_message_falls_back_to_the_code() {
        let body = br#"{"subsonic-response":{"status":"failed","error":{"code":0}}}"#;
        assert_eq!(
            classify_stream_reply(200, Some("application/json"), body)
                .unwrap()
                .summary,
            "the server refused it: Subsonic error 0"
        );
    }

    #[test]
    fn http_status_without_a_document_names_the_status() {
        let p = classify_stream_reply(404, Some("text/plain"), b"404 page not found").unwrap();
        assert_eq!(p.summary, "the server refused it: HTTP 404 Not Found");
        assert!(!p.every_song);
        assert!(classify_stream_reply(401, None, b"").unwrap().every_song);
        assert!(classify_stream_reply(403, None, b"").unwrap().every_song);
        assert!(!classify_stream_reply(503, None, b"").unwrap().every_song);
        assert_eq!(
            classify_stream_reply(599, None, b"").unwrap().summary,
            "the server refused it: HTTP 599"
        );
    }

    #[test]
    fn html_page_at_http_200_is_not_audio() {
        let p = classify_stream_reply(200, Some("text/html"), b"<html>Sign in</html>").unwrap();
        assert_eq!(p.summary, "the server sent text/html instead of audio");
        assert!(!p.every_song);
    }

    #[test]
    fn audio_replies_pass() {
        for ct in [
            Some("audio/flac"),
            Some("audio/mpeg"),
            Some("application/octet-stream"),
            Some("application/ogg"),
            None,
        ] {
            assert_eq!(classify_stream_reply(200, ct, b"fLaC"), None, "{ct:?}");
        }
        assert_eq!(classify_stream_reply(206, Some("audio/flac"), b""), None);
    }

    #[test]
    fn document_types_are_recognised_with_parameters_and_case() {
        for ct in [
            "text/xml",
            "TEXT/XML; charset=UTF-8",
            "application/json;charset=utf-8",
            "application/problem+json",
            "application/atom+xml",
        ] {
            assert!(is_document_type(ct), "{ct}");
        }
        assert!(!is_document_type("audio/x-flac"));
    }

    proptest! {
        #[test]
        fn arbitrary_bodies_never_panic(status in 0u16..1000, body in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = classify_stream_reply(status, Some("text/xml"), &body);
        }

        #[test]
        fn arbitrary_xml_tags_never_panic(tag in "<subsonic-response><error[ a-z=\"'&;0-9]{0,40}/?>?") {
            let _ = parse_api_error(tag.as_bytes());
        }
    }
}
