// ref: internal/translator/request_benchmark_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::hint::black_box;

use serde_json::{json, Value};

use super::facade::Translator;

const BENCHMARK_HISTORY_SENTINEL: &str = "benchmark-final-history-turn";
const ROUTES: &[(&str, &[&str])] = &[
    (
        "claude",
        &["openai", "gemini", "codex", "interactions", "antigravity"],
    ),
    (
        "gemini",
        &[
            "openai",
            "claude",
            "codex",
            "interactions",
            "antigravity",
            "gemini",
        ],
    ),
    (
        "openai",
        &[
            "claude",
            "gemini",
            "codex",
            "interactions",
            "antigravity",
            "openai",
        ],
    ),
    (
        "openai-response",
        &["claude", "gemini", "codex", "interactions", "openai"],
    ),
    (
        "interactions",
        &[
            "claude",
            "gemini",
            "codex",
            "openai",
            "openai-response",
            "antigravity",
        ],
    ),
];

#[test]
fn request_benchmark_routes_keep_valid_json_and_final_history_turn() {
    verify_request_matrix(1);
}

fn verify_request_matrix(turns: usize) {
    visit_request_matrix(turns, |_, _, _, _| {});
}

fn exercise_request_matrix(turns: usize, iterations: usize) {
    visit_request_matrix(turns, |translator, source, target, request| {
        for _ in 0..iterations {
            black_box(translator.request(
                source,
                target,
                "gemini-2.5-pro",
                black_box(request),
                true,
            ));
        }
    });
}

fn visit_request_matrix(turns: usize, mut visit: impl FnMut(&Translator, &str, &str, &[u8])) {
    let translator = Translator::registered();
    let requests = BTreeMap::from([
        ("claude", benchmark_claude_request(turns)),
        ("gemini", benchmark_gemini_request(turns)),
        ("openai", benchmark_openai_request(turns)),
        ("openai-response", benchmark_openai_responses_request(turns)),
        ("interactions", benchmark_interactions_request(turns)),
    ]);

    for (source, targets) in ROUTES {
        let request = &requests[source];
        for target in *targets {
            let output = translator.request(source, target, "gemini-2.5-pro", request, true);
            assert!(
                serde_json::from_slice::<Value>(&output).is_ok(),
                "{source}_to_{target} generated invalid JSON: {}",
                String::from_utf8_lossy(&output)
            );
            if turns > 0 {
                assert!(
                    output
                        .windows(BENCHMARK_HISTORY_SENTINEL.len())
                        .any(|window| window == BENCHMARK_HISTORY_SENTINEL.as_bytes()),
                    "{source}_to_{target} dropped the final history turn"
                );
            }

            visit(&translator, source, target, request);
        }
    }
}

fn benchmark_claude_request(turns: usize) -> Vec<u8> {
    let payload = "x".repeat(1_024);
    let mut messages = Vec::with_capacity(turns * 2 + usize::from(turns > 0));
    for index in 0..turns {
        let call_id = format!("call_{index}");
        messages.push(json!({"role":"assistant","content":[
            {"type":"text","text":payload},
            {"type":"tool_use","id":call_id,"name":"lookup","input":{"query":payload}}
        ]}));
        messages.push(json!({"role":"user","content":[
            {"type":"tool_result","tool_use_id":call_id,"content":[{"type":"text","text":payload}]}
        ]}));
    }
    if turns > 0 {
        messages.push(json!({"role":"user","content":BENCHMARK_HISTORY_SENTINEL}));
    }
    benchmark_json(json!({
        "system":[{"type":"text","text":payload}],
        "messages":messages,
        "tools":[{"name":"lookup","description":payload,"input_schema":benchmark_schema()}]
    }))
}

fn benchmark_gemini_request(turns: usize) -> Vec<u8> {
    let payload = "x".repeat(1_024);
    let mut contents = Vec::with_capacity(turns * 2 + usize::from(turns > 0));
    for index in 0..turns {
        let call_id = format!("call_{index}");
        contents.push(json!({"role":"model","parts":[
            {"text":payload},
            {"functionCall":{"id":call_id,"name":"lookup","args":{"query":payload}}}
        ]}));
        contents.push(json!({"role":"user","parts":[
            {"functionResponse":{"id":call_id,"name":"lookup","response":{"result":payload}}}
        ]}));
    }
    if turns > 0 {
        contents.push(json!({"role":"user","parts":[{"text":BENCHMARK_HISTORY_SENTINEL}]}));
    }
    benchmark_json(json!({
        "system_instruction":{"parts":[{"text":payload}]},
        "contents":contents,
        "tools":[{"functionDeclarations":[{"name":"lookup","description":payload,"parameters":benchmark_schema()}]}]
    }))
}

fn benchmark_openai_request(turns: usize) -> Vec<u8> {
    let payload = "x".repeat(1_024);
    let mut messages = vec![json!({"role":"system","content":payload})];
    for index in 0..turns {
        let call_id = format!("call_{index}");
        messages.push(json!({"role":"assistant","content":payload,"tool_calls":[{
            "id":call_id,"type":"function","function":{"name":"lookup","arguments":"{\"query\":\"value\"}"}
        }]}));
        messages.push(json!({"role":"tool","tool_call_id":call_id,"content":payload}));
    }
    if turns > 0 {
        messages.push(json!({"role":"user","content":BENCHMARK_HISTORY_SENTINEL}));
    }
    benchmark_json(json!({
        "model":"gemini-2.5-pro",
        "messages":messages,
        "tools":[{"type":"function","function":{"name":"lookup","description":payload,"parameters":benchmark_schema()}}]
    }))
}

fn benchmark_openai_responses_request(turns: usize) -> Vec<u8> {
    let payload = "x".repeat(1_024);
    let mut input = Vec::with_capacity(turns * 3 + usize::from(turns > 0));
    for index in 0..turns {
        let call_id = format!("call_{index}");
        input.push(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":payload}]}));
        input.push(json!({"type":"function_call","call_id":call_id,"name":"lookup","arguments":"{\"query\":\"value\"}"}));
        input.push(json!({"type":"function_call_output","call_id":call_id,"output":payload}));
    }
    if turns > 0 {
        input.push(json!({"type":"message","role":"user","content":[{"type":"input_text","text":BENCHMARK_HISTORY_SENTINEL}]}));
    }
    benchmark_json(json!({
        "instructions":payload,
        "input":input,
        "tools":[{"type":"function","name":"lookup","description":payload,"parameters":benchmark_schema()}]
    }))
}

fn benchmark_interactions_request(turns: usize) -> Vec<u8> {
    let payload = "x".repeat(1_024);
    let mut input = Vec::with_capacity(turns * 3 + usize::from(turns > 0));
    for index in 0..turns {
        let call_id = format!("call_{index}");
        input.push(json!({"type":"model_output","content":[{"type":"text","text":payload}]}));
        input.push(json!({"type":"function_call","call_id":call_id,"name":"lookup","arguments":{"query":payload}}));
        input.push(
            json!({"type":"function_result","call_id":call_id,"name":"lookup","result":payload}),
        );
    }
    if turns > 0 {
        input.push(json!({"type":"user_input","content":[{"type":"text","text":BENCHMARK_HISTORY_SENTINEL}]}));
    }
    benchmark_json(json!({
        "system_instruction":payload,
        "input":input,
        "tools":[{"function_declarations":[{"name":"lookup","description":payload,"parameters":benchmark_schema()}]}]
    }))
}

fn benchmark_schema() -> Value {
    json!({"type":"object","properties":{"query":{"type":"string"}}})
}

fn benchmark_json(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).expect("benchmark fixture must serialize")
}
