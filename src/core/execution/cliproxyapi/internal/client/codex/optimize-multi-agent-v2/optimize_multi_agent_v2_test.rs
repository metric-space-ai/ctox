// ref: internal/client/codex/optimize-multi-agent-v2/optimize_multi_agent_v2_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::*;

fn model(value: Value) -> ModelMap {
    value.as_object().unwrap().clone()
}

fn source(_: &str) -> Option<SpawnAgentModelMetadata> {
    None
}

fn context() -> MultiAgentV2Context {
    MultiAgentV2Context {
        enabled: true,
        user_agent: "Codex Desktop/1.2.3".to_owned(),
        available_models: vec![model(json!({"id":"gpt-5.5"})), model(json!({"id":"custom","display_name":"Custom"}))],
        catalog_json: br#"{"models":[{"slug":"gpt-5.5","description":"Official model","display_name":"GPT","priority":10,"supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}],"default_reasoning_level":"high","service_tiers":[{"id":"priority"}]}]}"#.to_vec(),
    }
}

fn tool_payload() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "tools":[{"type":"namespace","name":"collaboration","tools":[{
            "type":"function","name":"spawn_agent","description":"Spawns an agent to work.",
            "parameters":{"properties":{"message":{"type":"string","encrypted":true}}}
        },{
            "type":"function","name":"send_message","parameters":{"properties":{"message":{"encrypted":true}}}
        }]}]
    })).unwrap()
}

#[test]
fn model_profiles_include_reasoning_and_tiers() {
    let models = spawn_agent_models_from_sources(
        &context().available_models,
        &context().catalog_json,
        &source,
    );
    assert_eq!(models[0].id, "gpt-5.5");
    assert_eq!(models[0].reasoning_efforts, vec!["low", "high"]);
    assert_eq!(models[0].default_reasoning_effort, "high");
    assert_eq!(models[0].service_tiers, vec!["priority"]);
    assert_eq!(models[1].id, "custom");
    assert!(models[1].service_tiers.is_empty());
}

#[test]
fn synthesized_profile_uses_injected_metadata() {
    let metadata = |id: &str| {
        (id == "custom").then(|| SpawnAgentModelMetadata {
            description: "Injected".into(),
            thinking_levels: vec!["none".into(), "medium".into(), "ultra".into()],
        })
    };
    let models = spawn_agent_models_from_sources(
        &context().available_models,
        &context().catalog_json,
        &metadata,
    );
    assert_eq!(models[1].description, "Injected");
    assert_eq!(models[1].default_reasoning_effort, "medium");
}

#[test]
fn rewrites_description_and_normalizes_existing_section() {
    let old =
        format!("Intro\n{CODEX_SPAWN_AGENT_MODELS_HEADING}\n- `old`: Old.\nSpawns an agent now.");
    let rewritten = replace_spawn_agent_models(&old, "- `new`: New.");
    assert_eq!(
        rewritten.matches(CODEX_SPAWN_AGENT_MODELS_HEADING).count(),
        1
    );
    assert!(!rewritten.contains("`old`"));
    assert!(rewritten.find("`new`").unwrap() < rewritten.find("Spawns an agent").unwrap());
}

#[test]
fn top_level_description_without_marker_gets_appended_section() {
    let rewritten = replace_spawn_agent_models("Delegate work.", "- `m`: Model.");
    assert!(rewritten.starts_with("Delegate work.\n\n"));
    assert!(rewritten.ends_with("- `m`: Model."));
}

#[test]
fn optimization_renames_namespace_and_removes_encryption() {
    let result = optimize_request(&context(), &tool_payload(), &source);
    assert!(result.namespace_optimized);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert_eq!(
        value["tools"][0]["name"],
        CODEX_OPTIMIZED_COLLABORATION_NAMESPACE
    );
    assert!(value
        .pointer("/tools/0/tools/0/parameters/properties/message/encrypted")
        .is_none());
    assert!(value
        .pointer("/tools/0/tools/1/parameters/properties/message/encrypted")
        .is_none());
    assert!(value["tools"][0]["tools"][0]["description"]
        .as_str()
        .unwrap()
        .contains("`gpt-5.5`"));
}

#[test]
fn optimization_skips_namespace_conflict_but_still_normalizes_messages() {
    let payload = serde_json::to_vec(&json!({
        "tools":[{"type":"function","name":"collaboration-optimize__existing"},{"type":"function","name":"send_message","parameters":{"properties":{"message":{"encrypted":true}}}}]
    })).unwrap();
    let result = optimize_request(&context(), &payload, &source);
    assert!(!result.namespace_optimized);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert!(value
        .pointer("/tools/1/parameters/properties/message/encrypted")
        .is_none());
}

#[test]
fn disabled_or_foreign_client_leaves_bytes_unchanged() {
    let payload = tool_payload();
    let mut disabled = context();
    disabled.enabled = false;
    assert_eq!(
        optimize_request(&disabled, &payload, &source).payload,
        payload
    );
    let mut foreign = context();
    foreign.user_agent = "curl".into();
    assert_eq!(
        optimize_request(&foreign, &payload, &source).payload,
        payload
    );
}

#[test]
fn agent_message_content_is_decrypted_for_proxy_without_changing_role() {
    let payload = br#"{"input":[{"type":"agent_message","role":"assistant","content":[{"type":"encrypted_content","encrypted_content":"cipher"}]}]}"#;
    let result = optimize_request(&context(), payload, &source);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert_eq!(value["input"][0]["type"], "agent_message");
    assert_eq!(
        value["input"][0]["content"][0],
        json!({"type":"input_text","text":"cipher"})
    );
}

#[test]
fn multi_agent_input_becomes_standard_responses_message() {
    let payload = br#"{"input":[{"type":"agent_message","content":[{"type":"encrypted_content","encrypted_content":"text"}]}]}"#;
    let rewritten = rewrite_multi_agent_input(&context(), payload);
    let value: Value = serde_json::from_slice(&rewritten).unwrap();
    assert_eq!(value["input"][0]["type"], "message");
    assert_eq!(value["input"][0]["role"], "user");
    assert_eq!(value["input"][0]["content"][0]["text"], "text");
}

#[test]
fn additional_tools_are_discovered_and_rewritten() {
    let payload = serde_json::to_vec(&json!({"input":[{"type":"additional_tools","tools":[{"type":"namespace","name":"collaboration","tools":[{"type":"function","name":"spawn_agent","description":"Spawns an agent","parameters":{"properties":{"message":{"encrypted":true}}}}]}]}]})).unwrap();
    let result = optimize_request(&context(), &payload, &source);
    assert!(result.namespace_optimized);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert!(value
        .pointer("/input/0/tools/0/tools/0/parameters/properties/message/encrypted")
        .is_none());
}

#[test]
fn unrelated_encrypted_fields_are_preserved() {
    let payload = serde_json::to_vec(&json!({"tools":[{"type":"function","name":"other","parameters":{"properties":{"message":{"encrypted":true}}}}],"encrypted":true})).unwrap();
    let result = optimize_request(&context(), &payload, &source);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert_eq!(value["encrypted"], true);
    assert_eq!(
        value["tools"][0]["parameters"]["properties"]["message"]["encrypted"],
        true
    );
}

#[test]
fn restore_response_restores_tool_names_but_not_opaque_arguments() {
    let payload = serde_json::to_vec(&json!({
        "type":"response","output":[
          {"type":"function_call","namespace":"collaboration-optimize","name":"collaboration-optimize__spawn_agent","arguments":"{\"name\":\"collaboration-optimize\"}"},
          {"type":"namespace","name":"collaboration-optimize"}
        ]
    })).unwrap();
    let restored: Value = serde_json::from_slice(&restore_response(&payload, true)).unwrap();
    assert_eq!(restored["output"][0]["namespace"], "collaboration");
    assert_eq!(restored["output"][0]["name"], "collaboration__spawn_agent");
    assert_eq!(restored["output"][1]["name"], "collaboration");
    assert!(restored["output"][0]["arguments"]
        .as_str()
        .unwrap()
        .contains("collaboration-optimize"));
}

#[test]
fn restore_disabled_or_invalid_is_exact_noop() {
    assert_eq!(restore_response(b"not-json", true), b"not-json");
    assert_eq!(restore_response(b"{ \"x\": 1 }", false), b"{ \"x\": 1 }");
}

#[test]
fn no_spawn_agent_still_removes_all_collaboration_encryption() {
    let payload = serde_json::to_vec(&json!({"tools": COLLABORATION_MESSAGE_TOOLS.iter().skip(1).map(|name| json!({"type":"function","name":name,"parameters":{"properties":{"message":{"encrypted":true}}}})).collect::<Vec<_>>() })).unwrap();
    let result = optimize_request(&context(), &payload, &source);
    let value: Value = serde_json::from_slice(&result.payload).unwrap();
    assert!(value
        .pointer("/tools/0/parameters/properties/message/encrypted")
        .is_none());
    assert!(value
        .pointer("/tools/1/parameters/properties/message/encrypted")
        .is_none());
}
