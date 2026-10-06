// ref: internal/runtime/executor/helps/claude_code_session_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::claude_code_session::*;
use crate::sdk::api::handlers::header_filter::HeaderMap;

fn headers(entries: &[(&str, &str)]) -> HeaderMap {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), vec![(*value).to_owned()]))
        .collect()
}

#[test]
fn extract_session_id_from_payload_json() {
    let payload =
        br#"{"metadata":{"user_id":"{\"device_id\":\"d\",\"session_id\":\"cache-session-1\"}"}}"#;
    assert_eq!(
        extract_claude_code_session_id(payload, None),
        "cache-session-1"
    );
}

#[test]
fn extract_session_id_from_explicit_header_context() {
    let headers = headers(&[(CLAUDE_CODE_SESSION_HEADER, "header-session-1")]);
    assert_eq!(
        extract_claude_code_session_id(br#"{"model":"gpt-5.4"}"#, Some(&headers)),
        "header-session-1"
    );
}

#[test]
fn prompt_cache_is_stable_across_requests() {
    let payload = br#"{"metadata":{"user_id":"{\"session_id\":\"cache-session-2\"}"}}"#;
    let first = claude_code_prompt_cache("grok-composer-2.5-fast", payload, None).unwrap();
    let second = claude_code_prompt_cache("grok-composer-2.5-fast", payload, None).unwrap();
    assert!(!first.id.is_empty());
    assert_eq!(second.id, first.id);
}

#[test]
fn session_id_prefers_header_over_payload() {
    let payload = br#"{"metadata":{"user_id":"{\"session_id\":\"payload-session\"}"}}"#;
    let headers = headers(&[(CLAUDE_CODE_SESSION_HEADER, "header-session")]);
    assert_eq!(
        extract_claude_code_session_id(payload, Some(&headers)),
        "header-session"
    );
}

#[test]
fn execution_scope_accepts_lowercase_header_map_keys() {
    let headers = headers(&[
        ("x-claude-code-session-id", "lower-session"),
        ("x-claude-code-agent-id", "lower-agent"),
    ]);
    assert_eq!(
        claude_code_execution_scope(&[], Some(&headers)).as_deref(),
        Some("claude:lower-session:agent:lower-agent")
    );
}

#[test]
fn execution_scope_isolates_agents() {
    let root = headers(&[(CLAUDE_CODE_SESSION_HEADER, "session-agents")]);
    let child_a = headers(&[
        (CLAUDE_CODE_SESSION_HEADER, "session-agents"),
        (CLAUDE_CODE_AGENT_HEADER, "agent-a"),
    ]);
    let child_b = headers(&[
        (CLAUDE_CODE_SESSION_HEADER, "session-agents"),
        (CLAUDE_CODE_AGENT_HEADER, "agent-b"),
    ]);
    let root = claude_code_execution_scope(&[], Some(&root)).unwrap();
    let child_a = claude_code_execution_scope(&[], Some(&child_a)).unwrap();
    let child_b = claude_code_execution_scope(&[], Some(&child_b)).unwrap();
    assert_eq!(root, "claude:session-agents:agent:main");
    assert_eq!(child_a, "claude:session-agents:agent:agent-a");
    assert_eq!(child_b, "claude:session-agents:agent:agent-b");
    assert_ne!(root, child_a);
    assert_ne!(root, child_b);
    assert_ne!(child_a, child_b);
}

#[test]
fn prompt_cache_is_deterministic_and_agent_scoped() {
    let root = headers(&[(CLAUDE_CODE_SESSION_HEADER, "session-cache-agents")]);
    let child = headers(&[
        (CLAUDE_CODE_SESSION_HEADER, "session-cache-agents"),
        (CLAUDE_CODE_AGENT_HEADER, "agent-a"),
    ]);
    let root_first = claude_code_prompt_cache("gpt-5.4", &[], Some(&root)).unwrap();
    let root_second = claude_code_prompt_cache("gpt-5.4", &[], Some(&root)).unwrap();
    let child = claude_code_prompt_cache("gpt-5.4", &[], Some(&child)).unwrap();
    let other_model = claude_code_prompt_cache("gpt-5.5", &[], Some(&root)).unwrap();
    assert_eq!(root_first.id, root_second.id);
    assert_ne!(root_first.id, child.id);
    assert_ne!(root_first.id, other_model.id);
}

#[test]
fn header_lookup_is_case_insensitive_and_keeps_every_non_empty_value() {
    let headers = HeaderMap::from([(
        "X-Trace".to_owned(),
        vec![" ".to_owned(), "first".to_owned(), " second ".to_owned()],
    )]);
    assert_eq!(
        header_value_case_insensitive(Some(&headers), "x-trace"),
        "first"
    );
    assert_eq!(
        header_values_case_insensitive(Some(&headers), "x-trace"),
        vec!["first".to_owned(), "second".to_owned()]
    );
    assert!(header_values_case_insensitive(None, "x-trace").is_empty());
}
