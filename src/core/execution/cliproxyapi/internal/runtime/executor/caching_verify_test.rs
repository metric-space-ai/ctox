// ref: internal/runtime/executor/caching_verify_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::claude_executor_cloaking::ensure_claude_cache_control;

fn cached(value: &Value) -> bool {
    value
        .get("cache_control")
        .and_then(|cache| cache.get("type"))
        .and_then(Value::as_str)
        == Some("ephemeral")
}

#[test]
fn cache_control_covers_string_array_tools_and_independent_sections() {
    let output = ensure_claude_cache_control(
        br#"{"model":"claude-3-5-sonnet","tools":[{"name":"first"},{"name":"last"}],"system":"long prompt","messages":[]}"#,
    );
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(!cached(&value["tools"][0]));
    assert!(cached(&value["tools"][1]));
    assert_eq!(value["system"][0]["text"], "long prompt");
    assert!(cached(&value["system"][0]));

    let output = ensure_claude_cache_control(
        br#"{"tools":[{"name":"tool","cache_control":{"type":"ephemeral"}}],"system":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"messages":[]}"#,
    );
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(cached(&value["tools"][0]));
    assert!(!cached(&value["system"][0]));
    assert!(cached(&value["system"][1]));
}

#[test]
fn cache_control_handles_empty_and_many_tool_lists() {
    let tools = (0..50)
        .map(|index| json!({"name": format!("tool{index}")}))
        .collect::<Vec<_>>();
    let input = serde_json::to_vec(&json!({
        "tools": tools,
        "system": [{"type":"text", "text":"Claude Code"}],
        "messages": [{"role":"user", "content":"hello"}]
    }))
    .unwrap();
    let value: Value = serde_json::from_slice(&ensure_claude_cache_control(&input)).unwrap();
    assert!(value["tools"]
        .as_array()
        .unwrap()
        .iter()
        .take(49)
        .all(|tool| !cached(tool)));
    assert!(cached(&value["tools"][49]));
    assert!(cached(&value["system"][0]));

    let empty = ensure_claude_cache_control(
        br#"{"tools":[],"system":"test","messages":[{"role":"user","content":"hi"}]}"#,
    );
    let empty: Value = serde_json::from_slice(&empty).unwrap();
    assert!(cached(&empty["system"][0]));
}

#[test]
fn message_cache_uses_second_last_user_and_preserves_existing_breakpoint() {
    let output = ensure_claude_cache_control(
        br#"{"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"reply"},{"role":"user","content":"second"},{"role":"assistant","content":"reply 2"},{"role":"user","content":"third"}]}"#,
    );
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(cached(&value["messages"][2]["content"][0]));
    assert!(!cached(&value["messages"][4]["content"][0]));

    let output = ensure_claude_cache_control(
        br#"{"messages":[{"role":"user","content":[{"type":"text","text":"first"}]},{"role":"assistant","content":[{"type":"text","text":"reply","cache_control":{"type":"ephemeral"}}]},{"role":"user","content":[{"type":"text","text":"second"}]}]}"#,
    );
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert!(!cached(&value["messages"][0]["content"][0]));
    assert!(cached(&value["messages"][1]["content"][0]));
}

#[test]
fn deferred_tools_are_never_selected_as_cache_breakpoints() {
    for (tools, expected) in [
        (
            json!([{"name":"resident"},{"name":"deferred","defer_loading":true}]),
            Some(0),
        ),
        (
            json!([{"name":"resident"},{"name":"d1","defer_loading":true},{"name":"d2","defer_loading":true}]),
            Some(0),
        ),
        (
            json!([{"name":"r1"},{"name":"deferred","defer_loading":true},{"name":"r2"}]),
            Some(2),
        ),
        (
            json!([{"name":"d1","defer_loading":true},{"name":"d2","defer_loading":true}]),
            None,
        ),
    ] {
        let input = serde_json::to_vec(&json!({"tools": tools})).unwrap();
        let value: Value = serde_json::from_slice(&ensure_claude_cache_control(&input)).unwrap();
        for (index, tool) in value["tools"].as_array().unwrap().iter().enumerate() {
            assert_eq!(cached(tool), expected == Some(index));
        }
    }

    let existing = ensure_claude_cache_control(
        br#"{"tools":[{"name":"resident","cache_control":{"type":"ephemeral","ttl":"1h"}},{"name":"other"}]}"#,
    );
    let existing: Value = serde_json::from_slice(&existing).unwrap();
    assert_eq!(existing["tools"][0]["cache_control"]["ttl"], "1h");
    assert!(!cached(&existing["tools"][1]));
}

// ref: internal/runtime/executor/claude_prompt_cache_options_test.go @ eb6a768d103da08c039c20dfad28aad2c936fbbc
fn v16_cache_policy() -> super::claude_executor_cloaking::ClaudeCloakPolicy {
    let mut policy = super::claude_executor_cloaking::ClaudeCloakPolicy::oauth_default();
    policy.current_date = Some("2026-10-06".to_owned());
    policy
}

fn v16_cache_wire(
    input: &Value,
    policy: &super::claude_executor_cloaking::ClaudeCloakPolicy,
    oauth: bool,
) -> Value {
    let input = serde_json::to_vec(input).unwrap();
    let cloaked = super::claude_executor_cloaking::try_apply_claude_cloaking(
        &input,
        "claude-fable-5-1",
        policy,
        None,
    )
    .unwrap();
    let (body, _, _) = super::claude_executor_request::prepare_claude_upstream_body_with_identity(
        &cloaked, None, "", oauth,
    );
    let wire: Value = serde_json::from_slice(&body).unwrap();
    assert!(wire.get("prompt_cache_options").is_none());
    wire
}

fn v16_cache_controls(value: &Value) -> Vec<Value> {
    fn visit(value: &Value, controls: &mut Vec<Value>) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == "cache_control" {
                        controls.push(value.clone());
                    } else {
                        visit(value, controls);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, controls);
                }
            }
            _ => {}
        }
    }
    let mut controls = Vec::new();
    visit(value, &mut controls);
    controls
}

#[test]
fn candidate_v16_claude_cache_explicit_adds_no_breakpoints() {
    for oauth in [false, true] {
        for stream in [false, true] {
            for cloak in [false, true] {
                let mut policy = v16_cache_policy();
                if !cloak {
                    policy.mode = "never".to_owned();
                }
                let wire = v16_cache_wire(
                    &json!({
                        "model":"claude-fable-5-1", "stream":stream,
                        "prompt_cache_options":{"mode":"explicit"},
                        "system":"retained-caller", "tools":[{"name":"read","input_schema":{"type":"object"}}],
                        "messages":[{"role":"user","content":"hello"}]
                    }),
                    &policy,
                    oauth,
                );
                assert!(v16_cache_controls(&wire).is_empty(), "{wire}");
                assert!(wire.to_string().contains("retained-caller"));
                assert_eq!(wire["stream"], stream);
            }
        }
    }
}

#[test]
fn candidate_v16_claude_cache_forwarded_system_survives_all_destinations() {
    let control = json!({"type":"ephemeral","ttl":"1h","scope":"workspace","future_field":true});
    for (model, messages) in [
        (
            "claude-3-7-sonnet-20250219",
            json!([{"role":"user","content":"hello"},{"role":"assistant","content":"reply"}]),
        ),
        (
            "claude-fable-5-1",
            json!([{"role":"user","content":"hello"},{"role":"assistant","content":"reply"}]),
        ),
        (
            "claude-fable-5-1",
            json!([{"role":"user","content":"hello"},{"role":"user","content":"more"}]),
        ),
    ] {
        let wire = v16_cache_wire(
            &json!({
                "model":model,"prompt_cache_options":{"mode":"explicit"},
                "system":[{"type":"text","text":"retained-caller","cache_control":control},
                          {"type":"text","text":"also-retained"}],
                "messages":messages
            }),
            &v16_cache_policy(),
            true,
        );
        assert_eq!(
            v16_cache_controls(&wire),
            vec![control.clone()],
            "{model}: {wire}"
        );
        assert!(wire.to_string().contains("retained-caller"));
        assert!(wire.to_string().contains("also-retained"));
        if model.starts_with("claude-3-7") {
            assert!(wire["messages"][0]["content"]
                .to_string()
                .contains("retained-caller"));
        } else if wire["messages"].as_array().unwrap().len() == 2 {
            assert!(wire["system"].to_string().contains("retained-caller"));
        } else {
            assert_eq!(wire["messages"][1]["role"], "system");
        }
    }
}

#[test]
fn candidate_v16_claude_cache_invalid_forwarded_controls_are_not_promoted() {
    let valid = json!({"type":"ephemeral","ttl":"1h","scope":"workspace","future":123});
    let mut system = vec![json!({"type":"text","text":"valid","cache_control":valid})];
    for invalid in [
        Value::Null,
        json!("ephemeral"),
        json!({"type":"persistent"}),
        json!({"type":true}),
        json!({}),
    ] {
        system.push(json!({"type":"text","text":"plain-client-block","cache_control":invalid}));
    }
    let wire = v16_cache_wire(
        &json!({
            "model":"claude-fable-5-1","prompt_cache_options":{"mode":"explicit"},
            "system":system, "messages":[{"role":"user","content":"hello"}]
        }),
        &v16_cache_policy(),
        false,
    );
    assert_eq!(v16_cache_controls(&wire), vec![valid]);
    assert!(wire.to_string().contains("plain-client-block"));
}

#[test]
fn candidate_v16_claude_cache_preserves_message_tool_scope_and_mixed_ttls() {
    let tool = json!({"type":"ephemeral","ttl":"5m","scope":"organization"});
    let user = json!({"type":"ephemeral","ttl":"1h","scope":"workspace"});
    let wire = v16_cache_wire(
        &json!({
            "model":"claude-fable-5-1","prompt_cache_options":{"mode":"explicit"},
            "tools":[{"name":"read","cache_control":tool}],
            "messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":user}]}]
        }),
        &v16_cache_policy(),
        true,
    );
    let controls = v16_cache_controls(&wire);
    assert_eq!(controls.len(), 2);
    assert!(controls.contains(&tool), "{wire}");
    assert!(controls.contains(&user), "{wire}");
}

#[test]
fn candidate_v16_claude_cache_strict_drop_keeps_existing_user_marker() {
    let user = json!({"type":"ephemeral","ttl":"1h","scope":"workspace"});
    let mut policy = v16_cache_policy();
    policy.strict_mode = true;
    let wire = v16_cache_wire(
        &json!({
            "model":"claude-fable-5-1","prompt_cache_options":{"mode":"explicit"},
            "system":[{"type":"text","text":"discarded-caller","cache_control":{"type":"ephemeral","ttl":"5m"}}],
            "messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":user}]}]
        }),
        &policy,
        true,
    );
    assert!(!wire.to_string().contains("discarded-caller"));
    assert_eq!(v16_cache_controls(&wire), vec![user]);
}

#[test]
fn candidate_v16_claude_cache_count_tokens_retains_client_cache_contract() {
    let system = json!({"type":"ephemeral","ttl":"1h","scope":"workspace"});
    let tool = json!({"type":"ephemeral","ttl":"5m","scope":"organization"});
    for (model, messages) in [
        (
            "claude-3-7-sonnet-20250219",
            json!([{"role":"user","content":"hello"},{"role":"assistant","content":"reply"}]),
        ),
        (
            "claude-fable-5-1",
            json!([{"role":"user","content":"hello"},{"role":"assistant","content":"reply"}]),
        ),
        (
            "claude-fable-5-1",
            json!([{"role":"user","content":"hello"},{"role":"user","content":"more"}]),
        ),
    ] {
        let input = json!({
            "model":model,"prompt_cache_options":{"mode":"explicit"},
            "system":[{"type":"text","text":"retained-caller","cache_control":system}],
            "tools":[{"name":"read","cache_control":tool}],
            "messages":messages
        });
        let body = super::claude_executor_tokens::prepare_claude_first_party_token_count_body(
            &serde_json::to_vec(&input).unwrap(),
            model,
            &v16_cache_policy(),
            "",
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&body.body).unwrap();
        assert!(body.cloaked);
        assert!(wire.get("prompt_cache_options").is_none());
        assert!(!wire.to_string().contains("official CLI"));
        assert!(wire.to_string().contains("retained-caller"));
        let controls = v16_cache_controls(&wire);
        assert_eq!(controls.len(), 2);
        assert!(controls.contains(&system));
        assert!(controls.contains(&tool));
        assert!(body
            .requested_betas
            .iter()
            .any(|beta| beta == "token-counting-2024-11-01"));
    }
}

#[test]
fn candidate_v16_claude_cache_automatic_mode_keeps_default_breakpoints() {
    for mode in [Value::Null, json!("automatic"), json!(""), json!(true)] {
        let wire = v16_cache_wire(
            &json!({
                "model":"claude-fable-5-1","prompt_cache_options":{"mode":mode},
                "system":"retained-caller", "messages":[{"role":"user","content":"hello"}]
            }),
            &v16_cache_policy(),
            false,
        );
        assert!(v16_cache_controls(&wire).len() >= 2, "{wire}");
    }
}

#[test]
fn candidate_v16_claude_cache_mode_detection_and_byte_identical_noop() {
    use super::claude_executor_cloaking::{
        is_explicit_claude_prompt_cache_mode, strip_claude_prompt_cache_options,
    };
    let explicit = br#"{ "prompt_cache_options": { "mode": " ExPlIcIt " } }"#;
    assert!(is_explicit_claude_prompt_cache_mode(&[
        b"invalid", b"{}", explicit
    ]));
    for body in [
        b"invalid".as_slice(),
        b"[]",
        b"{}",
        br#"{"prompt_cache_options":{"mode":true}}"#,
    ] {
        assert!(!is_explicit_claude_prompt_cache_mode(&[body]));
    }
    for body in [b"invalid".as_slice(), b"[]", b"{ \"model\" : \"claude\" }"] {
        assert_eq!(strip_claude_prompt_cache_options(body), body);
    }
    let wire: Value = serde_json::from_slice(&strip_claude_prompt_cache_options(
        br#"{"model":"claude","prompt_cache_options":null}"#,
    ))
    .unwrap();
    assert_eq!(wire, json!({"model":"claude"}));
}
