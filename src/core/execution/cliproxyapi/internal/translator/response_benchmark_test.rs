// ref: internal/translator/response_benchmark_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::hint::black_box;

use serde_json::Value;

use super::facade::Translator;

#[test]
fn response_benchmark_routes_keep_valid_json_and_payload() {
    verify_response_matrix(1_024);
}

fn verify_response_matrix(payload_size: usize) {
    visit_response_matrix(payload_size, |_, _, _, _| {});
}

fn exercise_response_matrix(payload_size: usize, iterations: usize) {
    visit_response_matrix(payload_size, |translator, source, target, raw| {
        for _ in 0..iterations {
            black_box(translator.response_non_stream(
                source,
                target,
                "benchmark-model",
                &[],
                &[],
                black_box(raw),
                None,
            ));
        }
    });
}

fn visit_response_matrix(
    payload_size: usize,
    mut visit: impl FnMut(&Translator, &str, &str, &[u8]),
) {
    let payload = "x".repeat(payload_size);
    let cases = [
        (
            "gemini",
            "openai",
            format!(r#"{{"modelVersion":"gemini-test","candidates":[{{"index":0,"content":{{"parts":[{{"text":"{payload}"}}]}},"finishReason":"STOP"}}]}}"#).into_bytes(),
        ),
        (
            "codex",
            "openai",
            format!(r#"{{"type":"response.completed","response":{{"id":"resp_1","created_at":1700000000,"model":"gpt-test","status":"completed","output":[{{"type":"message","content":[{{"type":"output_text","text":"{payload}"}}]}}]}}}}"#).into_bytes(),
        ),
        ("claude", "openai", claude_large_text_response(&payload)),
        (
            "claude",
            "openai-response",
            claude_large_text_response(&payload),
        ),
    ];
    let translator = Translator::registered();
    for (source, target, raw) in cases {
        let output =
            translator.response_non_stream(source, target, "benchmark-model", &[], &[], &raw, None);
        assert!(
            serde_json::from_slice::<Value>(&output).is_ok(),
            "{source}_to_{target} generated invalid JSON"
        );
        assert!(
            output
                .windows(payload.len())
                .any(|window| window == payload.as_bytes()),
            "{source}_to_{target} dropped the benchmark payload: {}",
            String::from_utf8_lossy(&output)
        );

        visit(&translator, source, target, &raw);
    }
}

fn claude_large_text_response(payload: &str) -> Vec<u8> {
    format!(
        "data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_1\",\"model\":\"claude-test\"}}}}\n\
         data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\"}}}}\n\
         data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{payload}\"}}}}\n\
         data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}}}}\n"
    )
    .into_bytes()
}
