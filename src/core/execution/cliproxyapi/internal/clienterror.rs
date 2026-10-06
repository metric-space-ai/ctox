// ref: internal/clienterror/client_error.go::IsClaudeThreadNotFound @ 16d98881d4bb37adaa827599e4be8f5154e81646
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::util::valid_json_bytes;

/// A stale Claude continuation is a caller fault, not a credential failure.
pub fn is_claude_thread_not_found(status: u16, body: &[u8]) -> bool {
    claude_thread_not_found_detail(status, body).is_some()
}

pub(crate) fn claude_thread_not_found_detail(status: u16, body: &[u8]) -> Option<(String, String)> {
    if status != 404 || !valid_json_bytes(body) {
        return None;
    }
    let document = String::from_utf8_lossy(body);
    let error_type = gjson::get(&document, "error.type");
    let message = gjson::get(&document, "error.message");
    let error_type = error_type.str().trim();
    let message = message.str().trim();
    let lower = message.to_lowercase();
    (error_type.eq_ignore_ascii_case("not_found_error")
        && lower.contains("thread state")
        && lower.contains("previous_message_id"))
    .then(|| (error_type.to_owned(), message.to_owned()))
}

// ref: sdk/api/handlers/claude/code_handlers.go::toClaudeError @ 16d98881d4bb37adaa827599e4be8f5154e81646
pub(crate) fn claude_thread_replay_body(status: u16, body: &[u8]) -> Option<Vec<u8>> {
    let (error_type, message) = claude_thread_not_found_detail(status, body)?;
    serde_json::to_vec(&serde_json::json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message,
            "details": {"error_code": "thread_not_found"}
        }
    }))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: &[u8] = br#"{"type":"error","error":{"type":"not_found_error","message":"No thread state was found for the requested previous_message_id."}}"#;

    #[test]
    fn candidate_claude_thread_classifier_requires_status_type_and_both_message_parts() {
        assert!(is_claude_thread_not_found(404, MISSING));
        for status in [200, 401, 402, 429, 500] {
            assert!(!is_claude_thread_not_found(status, MISSING));
        }
        for body in [
            b"model not found".as_slice(),
            br#"{"error":{"type":"not_found_error","message":"Not Found"}}"#,
            br#"{"error":{"type":"not_found_error","message":"No thread state"}}"#,
            br#"{"error":{"type":"not_found_error","message":"previous_message_id missing"}}"#,
            br#"{"error":{"type":"authentication_error","message":"thread state previous_message_id"}}"#,
            br#"{"error":{"type":"not_found_error","message":"thread state previous_message_id"}} trailing"#,
        ] {
            assert!(!is_claude_thread_not_found(404, body));
        }
        assert!(is_claude_thread_not_found(404, br#"{"error":{"type":" NOT_FOUND_ERROR ","message":"THREAD STATE PREVIOUS_MESSAGE_ID"}}"#));
    }

    #[test]
    fn candidate_claude_thread_classifier_keeps_first_duplicate_and_raw_numeric_semantics() {
        let good = r#"{"type":"not_found_error","message":"thread state previous_message_id"}"#;
        let bad = r#"{"type":"not_found_error","message":"model missing"}"#;
        let document = format!(r#"{{"n":1e1000,"error":{good},"error":{bad}}}"#);
        assert!(is_claude_thread_not_found(404, document.as_bytes()));
        let document = format!(r#"{{"n":1e1000,"error":{bad},"error":{good}}}"#);
        assert!(!is_claude_thread_not_found(404, document.as_bytes()));
    }

    #[test]
    fn candidate_claude_thread_classifier_marks_only_replayable_errors() {
        let marked = claude_thread_replay_body(404, MISSING).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&marked).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["error"]["type"], "not_found_error");
        assert_eq!(value["error"]["details"]["error_code"], "thread_not_found");
        assert!(claude_thread_replay_body(500, MISSING).is_none());
        assert!(claude_thread_replay_body(404, b"model not found").is_none());
    }
}
