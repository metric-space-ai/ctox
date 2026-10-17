// Origin: CTOX
// License: AGPL-3.0-only

use std::time::Duration;

use super::{async_prompt, generate_random_state, parse_oauth_callback, OAuthCallback};

#[test]
fn random_state_is_16_bytes_of_lowercase_hex() {
    let state = generate_random_state().expect("OS randomness should be available");
    assert_eq!(state.len(), 32);
    assert!(state.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(state, state.to_ascii_lowercase());
}

#[test]
fn parses_query_fragment_and_bare_query_forms() {
    assert_eq!(
        parse_oauth_callback(" https://localhost/callback?code=query&state=s#code=fragment ")
            .expect("valid callback"),
        Some(OAuthCallback {
            code: "query".into(),
            state: "s".into(),
            ..OAuthCallback::default()
        })
    );
    assert_eq!(
        parse_oauth_callback("code=abc&state=xyz").expect("valid bare query"),
        Some(OAuthCallback {
            code: "abc".into(),
            state: "xyz".into(),
            ..OAuthCallback::default()
        })
    );
    assert_eq!(parse_oauth_callback(" \t").expect("blank callback"), None);
}

#[test]
fn fragment_fills_only_missing_query_values() {
    assert_eq!(
        parse_oauth_callback("http://localhost/#code=fragment&state=fragment-state")
            .expect("valid fragment callback"),
        Some(OAuthCallback {
            code: "fragment".into(),
            state: "fragment-state".into(),
            ..OAuthCallback::default()
        })
    );
}

#[test]
fn error_description_is_promoted_when_error_code_is_missing() {
    assert_eq!(
        parse_oauth_callback("?error_description=access%20denied").expect("valid error callback"),
        Some(OAuthCallback {
            error: "access denied".into(),
            ..OAuthCallback::default()
        })
    );
}

#[test]
fn rejects_non_url_and_callbacks_without_code_or_error() {
    assert!(parse_oauth_callback("not-a-callback").is_err());
    assert!(parse_oauth_callback("http://localhost/?state=orphan").is_err());
}

#[test]
fn async_prompt_reports_exactly_one_lane_without_blocking() {
    let (input, error) = async_prompt(
        |message| Ok::<_, &'static str>(message.to_uppercase()),
        "answer".into(),
    );
    assert_eq!(
        input.recv_timeout(Duration::from_secs(1)).as_deref(),
        Ok("ANSWER")
    );
    assert!(error.recv_timeout(Duration::from_millis(10)).is_err());

    let (input, error) = async_prompt(|_message| Err::<String, _>("cancelled"), "answer".into());
    assert_eq!(error.recv_timeout(Duration::from_secs(1)), Ok("cancelled"));
    assert!(input.recv_timeout(Duration::from_millis(10)).is_err());
}
