// ref: examples/plugin/thinking/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, tagged_body, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration("example-thinking-go", &["thinking_applier"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "thinking.identifier" => super::core::reply(json!({"identifier":example().id})),
        "thinking.apply" => super::core::reply(tagged_body("thinking_applied_by", example().id)),
        _ => unknown(method),
    }
}
