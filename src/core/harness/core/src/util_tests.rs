use super::*;

#[test]
fn test_try_parse_error_message() {
    let text = r#"{
  "error": {
    "message": "Your refresh token has already been used to generate a new access token. Please try signing in again.",
    "type": "invalid_request_error",
    "param": null,
    "code": "refresh_token_reused"
  }
}"#;
    let message = try_parse_error_message(text);
    assert_eq!(
        message,
        "Your refresh token has already been used to generate a new access token. Please try signing in again."
    );
}

#[test]
fn test_try_parse_error_message_no_error() {
    let text = r#"{"message": "test"}"#;
    let message = try_parse_error_message(text);
    assert_eq!(message, r#"{"message": "test"}"#);
}













#[test]
fn normalize_thread_name_trims_and_rejects_empty() {
    assert_eq!(normalize_thread_name("   "), None);
    assert_eq!(
        normalize_thread_name("  my thread  "),
        Some("my thread".to_string())
    );
}
