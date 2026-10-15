// ref: examples/plugin/usage/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration("example-usage-go", &["usage_plugin"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "usage.handle" => reply(json!({})),
        _ => unknown(method),
    }
}
