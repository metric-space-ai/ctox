// ref: examples/plugin/executor/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration(
        "example-executor-go",
        &["executor", "chat-completions:chat-completions"],
    )
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "executor.identifier" => reply(json!({"identifier":example().id})),
        "executor.execute" => reply(
            json!({"Payload": br#"{"id":"example-executor-go","object":"chat.completion"}"#.to_vec(), "Headers":{"content-type":["application/json"]}}),
        ),
        "executor.execute_stream" => reply(
            json!({"headers":{"content-type":["text/event-stream"]},"chunks":[{"Payload":b"data: example-executor-go\n\n"}]}),
        ),
        "executor.count_tokens" => reply(json!({"Payload": br#"{"total_tokens":0}"#.to_vec()})),
        "executor.http_request" => reply(
            json!({"StatusCode":200,"Headers":{"content-type":["application/json"]},"Body":br#"{"plugin":"example-executor-go"}"#.to_vec()}),
        ),
        _ => unknown(method),
    }
}
