// ref: examples/plugin/protocol-format/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration(
        "example-protocol-format-go",
        &["executor", "chat-completions:responses"],
    )
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "executor.identifier" => reply(json!({"identifier":example().id})),
        "executor.execute" => reply(
            json!({"Payload":br#"{"id":"example-protocol-format-go","object":"chat.completion"}"#.to_vec(),"Headers":{"content-type":["application/json"]}}),
        ),
        _ => unknown(method),
    }
}
