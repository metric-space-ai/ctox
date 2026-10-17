// ref: sdk/cliproxy/session/identity_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::sdk::cliproxy::executor::{ExecutionMetadata, Headers, Options, Request};
use crate::sdk::translator::{claude, gemini, interactions, openai, openai_response};

fn request(payload: &[u8], metadata: ExecutionMetadata) -> Request {
    Request {
        payload: payload.to_vec(),
        metadata,
        ..Request::default()
    }
}

fn options(payload: &[u8], source_format: crate::sdk::translator::Format) -> Options {
    Options {
        original_request: payload.to_vec(),
        source_format,
        ..Options::default()
    }
}

#[test]
fn derive_id_is_stable_across_conversation_growth_for_all_protocols() {
    let cases = [
        (
            "openai chat",
            openai(),
            r#"{"messages":[{"role":"system","content":"system prompt"},{"role":"developer","content":"developer prompt"},{"role":"user","content":"complete first user prompt"}]}"#,
            r#"{"messages":[{"role":"system","content":"system prompt"},{"role":"developer","content":"developer prompt"},{"role":"user","content":"complete first user prompt"},{"role":"assistant","content":"answer"},{"role":"developer","content":"later instruction"},{"role":"user","content":"next"}]}"#,
        ),
        (
            "claude messages",
            claude(),
            r#"{"system":[{"type":"text","text":"system prompt"}],"messages":[{"role":"user","content":[{"type":"text","text":"complete first user prompt"}]}]}"#,
            r#"{"system":[{"type":"text","text":"system prompt"}],"messages":[{"role":"user","content":[{"type":"text","text":"complete first user prompt"}]},{"role":"assistant","content":"answer"},{"role":"user","content":"next"}]}"#,
        ),
        (
            "openai responses",
            openai_response(),
            r#"{"instructions":"system prompt","input":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"developer prompt"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"complete first user prompt"}]}]}"#,
            r#"{"instructions":"system prompt","input":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"developer prompt"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"complete first user prompt"}]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"answer"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]}]}"#,
        ),
        (
            "gemini",
            gemini(),
            r#"{"systemInstruction":{"parts":[{"text":"system prompt"}]},"contents":[{"role":"user","parts":[{"text":"complete first user prompt"}]}]}"#,
            r#"{"systemInstruction":{"parts":[{"text":"system prompt"}]},"contents":[{"role":"user","parts":[{"text":"complete first user prompt"}]},{"role":"model","parts":[{"text":"answer"}]},{"role":"user","parts":[{"text":"next"}]}]}"#,
        ),
        (
            "interactions",
            interactions(),
            r#"{"system_instruction":"system prompt","input":[{"type":"developer_instruction","text":"developer prompt"},{"type":"user_input","content":[{"type":"text","text":"complete first user prompt"}]}]}"#,
            r#"{"system_instruction":"system prompt","input":[{"type":"developer_instruction","text":"developer prompt"},{"type":"user_input","content":[{"type":"text","text":"complete first user prompt"}]},{"type":"model_output","content":[{"type":"text","text":"answer"}]},{"type":"user_input","content":[{"type":"text","text":"next"}]}]}"#,
        ),
    ];
    for (name, format, first, later) in cases {
        let first_id = derive_id(&format, first.as_bytes(), "caller-a");
        let later_id = derive_id(&format, later.as_bytes(), "caller-a");
        assert!(!first_id.is_empty(), "{name}");
        assert_eq!(first_id, later_id, "{name}");
    }
}

#[test]
fn derive_id_uses_instruction_prefix_and_full_user_input() {
    let prefix = "界".repeat(50);
    let first = format!(
        r#"{{"messages":[{{"role":"system","content":"{prefix}timestamp-a"}},{{"role":"user","content":"{}a"}}]}}"#,
        "u".repeat(120)
    );
    let same_root = format!(
        r#"{{"messages":[{{"role":"system","content":"{prefix}timestamp-b"}},{{"role":"user","content":"{}a"}}]}}"#,
        "u".repeat(120)
    );
    let different_user = format!(
        r#"{{"messages":[{{"role":"system","content":"{prefix}timestamp-b"}},{{"role":"user","content":"{}b"}}]}}"#,
        "u".repeat(120)
    );
    let first_id = derive_id(&openai(), first.as_bytes(), "caller-a");
    assert!(!first_id.is_empty());
    assert_eq!(
        derive_id(&openai(), same_root.as_bytes(), "caller-a"),
        first_id
    );
    assert_ne!(
        derive_id(&openai(), different_user.as_bytes(), "caller-a"),
        first_id
    );
}

#[test]
fn derive_id_isolates_callers_and_tracks_gemini_cached_content() {
    let payload = br#"{"messages":[{"role":"user","content":"same prompt"}]}"#;
    let caller_a = derive_id(&openai(), payload, &caller_scope("api-key-a"));
    let caller_b = derive_id(&openai(), payload, &caller_scope("api-key-b"));
    assert!(!caller_a.is_empty());
    assert_ne!(caller_a, caller_b);

    let first = br#"{"cachedContent":"cachedContents/abc","contents":[{"role":"user","parts":[{"text":"first"}]}]}"#;
    let grown = br#"{"cachedContent":"cachedContents/abc","contents":[{"role":"user","parts":[{"text":"first"}]},{"role":"model","parts":[{"text":"answer"}]},{"role":"user","parts":[{"text":"next"}]}]}"#;
    let different = br#"{"cachedContent":"cachedContents/abc","contents":[{"role":"user","parts":[{"text":"different"}]}]}"#;
    let first_id = derive_id(&gemini(), first, "caller-a");
    assert!(!first_id.is_empty());
    assert_eq!(derive_id(&gemini(), grown, "caller-a"), first_id);
    assert_ne!(derive_id(&gemini(), different, "caller-a"), first_id);
}

#[test]
fn derive_id_requires_first_user_input() {
    assert!(derive_id(
        &openai(),
        br#"{"messages":[{"role":"system","content":"shared system"}]}"#,
        "caller-a"
    )
    .is_empty());
    assert!(derive_id(&openai(), b"not-json", "caller-a").is_empty());
}

#[test]
fn enrich_skips_derivation_for_explicit_sessions() {
    struct Case {
        name: &'static str,
        payload: Vec<u8>,
        headers: Headers,
        request_metadata: ExecutionMetadata,
        option_metadata: ExecutionMetadata,
        execution: bool,
    }
    let header = |name: &str, values: &[&str]| {
        Headers::from([(
            name.to_owned(),
            values.iter().map(ToString::to_string).collect(),
        )])
    };
    let message = br#"{"messages":[{"role":"user","content":"hello"}]}"#.to_vec();
    let cases = vec![
        Case { name: "session header avoids malformed body parsing", payload: b"not-json".to_vec(), headers: header("X-Session-ID", &["header-session"]), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "Claude Code session header", payload: message.clone(), headers: header("X-Claude-Code-Session-Id", &["claude-session"]), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "later valid multi-value header", payload: message.clone(), headers: header("X-Session-Affinity", &["", "later-valid-session"]), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "conversation object", payload: br#"{"conversation":{"id":"conversation-session"},"messages":[{"role":"user","content":"hello"}]}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "conversation string", payload: br#"{"conversation":"conversation-session","messages":[{"role":"user","content":"hello"}]}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "metadata user id", payload: br#"{"metadata":{"user_id":"explicit-user"},"messages":[{"role":"user","content":"hello"}]}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "long legacy Claude metadata", payload: format!(r#"{{"metadata":{{"user_id":"{}_session_ac980658-63bd-4fb3-97ba-8da64cb1e344"}},"messages":[{{"role":"user","content":"hello"}}]}}"#, "x".repeat(300)).into_bytes(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "JSON metadata without session", payload: br#"{"metadata":{"user_id":"{\"device_id\":\"abc123\"}"},"messages":[{"role":"user","content":"hello"}]}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "body session", payload: br#"{"session_id":"body-session","messages":[{"role":"user","content":"hello"}]}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "prompt cache key", payload: br#"{"prompt_cache_key":"cache-session","input":"hello"}"#.to_vec(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata::default(), execution: false },
        Case { name: "execution option metadata", payload: message.clone(), headers: Headers::new(), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata { execution_session_id: Some("execution-session".into()), ..ExecutionMetadata::default() }, execution: true },
        Case { name: "execution request metadata", payload: message.clone(), headers: Headers::new(), request_metadata: ExecutionMetadata { execution_session_id: Some("execution-session".into()), ..ExecutionMetadata::default() }, option_metadata: ExecutionMetadata::default(), execution: true },
        Case { name: "header clears stale derived", payload: message, headers: header("x-session-id", &["header-session"]), request_metadata: ExecutionMetadata::default(), option_metadata: ExecutionMetadata { derived_session_id: Some("ctx:v1:stale".into()), ..ExecutionMetadata::default() }, execution: false },
    ];
    for case in cases {
        let req = request(&case.payload, case.request_metadata);
        let mut opts = options(&case.payload, openai());
        opts.headers = case.headers;
        opts.metadata = case.option_metadata;
        let (req, opts) = enrich(req, opts);
        assert!(derived_id(&req.metadata).is_empty(), "{}", case.name);
        assert!(derived_id(&opts.metadata).is_empty(), "{}", case.name);
        if case.execution {
            assert_eq!(
                req.metadata.execution_session_id.as_deref(),
                Some("execution-session"),
                "{}",
                case.name
            );
            assert_eq!(
                opts.metadata.execution_session_id.as_deref(),
                Some("execution-session"),
                "{}",
                case.name
            );
        }
    }
}

#[test]
fn enrich_derives_after_invalid_session_identity() {
    let cases: [(String, Option<&str>); 4] = [
        (
            format!(
                r#"{{"prompt_cache_key":"{}","input":"hello"}}"#,
                "x".repeat(257)
            ),
            None,
        ),
        (
            r#"{"prompt_cache_key":"tenant\n","input":"hello"}"#.to_owned(),
            None,
        ),
        (
            r#"{"prompt_cache_key":"\ttenant","input":"hello"}"#.to_owned(),
            None,
        ),
        (r#"{"input":"hello"}"#.to_owned(), Some("bad\nsession")),
    ];
    for (payload, header) in cases {
        let mut opts = options(payload.as_bytes(), openai_response());
        if let Some(header) = header {
            opts.headers
                .insert("X-Session-Affinity".into(), vec![header.into()]);
        }
        let expected = derive_id(&openai_response(), payload.as_bytes(), "");
        let (req, opts) = enrich(
            request(payload.as_bytes(), ExecutionMetadata::default()),
            opts,
        );
        assert_eq!(derived_id(&req.metadata), expected);
        assert_eq!(derived_id(&opts.metadata), expected);
        assert!(req.metadata.execution_session_id.is_none());
        assert!(opts.metadata.execution_session_id.is_none());
    }

    for (on_options, execution) in [(true, "x".repeat(257)), (false, "bad\nsession".into())] {
        let payload = br#"{"input":"hello"}"#;
        let mut req_meta = ExecutionMetadata::default();
        let mut opts = options(payload, openai_response());
        if on_options {
            opts.metadata.execution_session_id = Some(execution);
        } else {
            req_meta.execution_session_id = Some(execution);
        }
        let expected = derive_id(&openai_response(), payload, "");
        let (req, opts) = enrich(request(payload, req_meta), opts);
        assert_eq!(derived_id(&req.metadata), expected);
        assert_eq!(derived_id(&opts.metadata), expected);
    }

    for (on_options, retained) in [(true, "x".repeat(257)), (false, "bad\nsession".to_owned())] {
        let payload = br#"{"input":"hello"}"#;
        let mut request_metadata = ExecutionMetadata::default();
        let mut opts = options(payload, openai_response());
        if on_options {
            opts.metadata.derived_session_id = Some(retained);
        } else {
            request_metadata.derived_session_id = Some(retained);
        }
        let expected = derive_id(&openai_response(), payload, "");
        let (req, opts) = enrich(request(payload, request_metadata), opts);
        assert_eq!(derived_id(&req.metadata), expected);
        assert_eq!(derived_id(&opts.metadata), expected);
    }
}

#[test]
fn enrich_copies_derived_identity_without_mutating_input_clones() {
    let payload = br#"{"messages":[{"role":"user","content":"hello"}]}"#;
    let original = request(payload, ExecutionMetadata::default());
    let mut opts = options(payload, openai());
    opts.metadata.caller_scope = Some("caller-a".into());
    let (enriched_request, enriched_options) = enrich(original.clone(), opts);
    assert!(!derived_id(&enriched_request.metadata).is_empty());
    assert_eq!(
        derived_id(&enriched_request.metadata),
        derived_id(&enriched_options.metadata)
    );
    assert!(original.metadata.derived_session_id.is_none());
}

#[test]
fn enrich_carries_a_non_aliasing_request_payload_snapshot() {
    let payload = br#"{"conversation":{"id":"request-only-conversation"},"input":"hello"}"#;
    let original = request(payload, ExecutionMetadata::default());
    let (_, enriched) = enrich(
        original,
        Options {
            source_format: openai_response(),
            ..Options::default()
        },
    );
    assert_eq!(enriched.original_request, payload);
    assert!(derived_id(&enriched.metadata).is_empty());
}

#[test]
fn explicit_id_claude_metadata_and_caller_scope_match_upstream_contract() {
    assert_eq!(normalize_explicit_id("  opaque value  "), "opaque value");
    assert!(normalize_explicit_id("bad\nvalue").is_empty());
    assert!(normalize_explicit_id(&"界".repeat(86)).is_empty());
    assert_eq!(
        claude_metadata_session_id(
            br#"{"metadata":{"user_id":"{\"session_id\":\" current-session \"}"}}"#
        ),
        "current-session"
    );
    assert_eq!(
        claude_metadata_session_id(
            br#"{"metadata":{"user_id":"prefix_session_ac980658-63bd-4fb3-97ba-8da64cb1e344"}}"#
        ),
        "ac980658-63bd-4fb3-97ba-8da64cb1e344"
    );
    assert!(
        claude_metadata_session_id(br#"{"metadata":{"user_id":"prefix_session_UPPER"}}"#)
            .is_empty()
    );
    assert_eq!(caller_scope(" api-key-a ").len(), 64);
    assert_eq!(caller_scope("api-key-a"), caller_scope(" api-key-a "));
    assert_ne!(caller_scope("api-key-a"), caller_scope("api-key-b"));
}

#[test]
fn canonical_media_cache_control_and_nested_interactions_are_stable() {
    let media_a = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hello"},{"type":"image_url","image_url":{"url":"https://example/a.png"}},{"custom":{"cache_control":{"type":"ephemeral"},"b":2,"a":1}}]}]}"#;
    let media_b = br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hello"},{"type":"image_url","image_url":{"url":"https://example/a.png"}},{"custom":{"a":1,"b":2}}]}]}"#;
    assert_eq!(
        derive_id(&openai(), media_a, "caller"),
        derive_id(&openai(), media_b, "caller")
    );

    let flat =
        br#"{"input":[{"role":"developer","text":"rules"},{"role":"user","content":"hello"}]}"#;
    let nested = br#"{"input":{"steps":[{"role":"developer","text":"rules"},{"role":"user","content":"hello"}]}}"#;
    assert_eq!(
        derive_id(&interactions(), flat, "caller"),
        derive_id(&interactions(), nested, "caller")
    );
}

#[test]
fn execution_and_retained_derived_precedence_match_upstream() {
    let payload = br#"{"messages":[{"role":"user","content":"hello"}]}"#;
    let request_metadata = ExecutionMetadata {
        execution_session_id: Some("request-execution".into()),
        derived_session_id: Some("request-derived".into()),
        caller_scope: Some("request-caller".into()),
        ..ExecutionMetadata::default()
    };
    let option_metadata = ExecutionMetadata {
        execution_session_id: Some(" option-execution ".into()),
        derived_session_id: Some("option-derived".into()),
        caller_scope: Some("option-caller".into()),
        ..ExecutionMetadata::default()
    };
    let mut opts = options(payload, openai());
    opts.metadata = option_metadata;
    let (req, opts) = enrich(request(payload, request_metadata), opts);
    assert_eq!(
        req.metadata.execution_session_id.as_deref(),
        Some("option-execution")
    );
    assert_eq!(
        opts.metadata.execution_session_id.as_deref(),
        Some("option-execution")
    );
    assert!(req.metadata.derived_session_id.is_none());

    let mut opts = options(payload, openai());
    opts.metadata.derived_session_id = Some(" option-derived ".into());
    let req = request(
        payload,
        ExecutionMetadata {
            derived_session_id: Some("request-derived".into()),
            ..ExecutionMetadata::default()
        },
    );
    let (req, opts) = enrich(req, opts);
    assert_eq!(derived_id(&req.metadata), "option-derived");
    assert_eq!(derived_id(&opts.metadata), "option-derived");
}
