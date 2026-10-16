// ref: examples/plugin/request-normalizer/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, tagged_body, unknown, ExampleRegistration, ExampleResult};
pub fn example() -> ExampleRegistration {
    registration("example-request-normalizer-go", &["request_normalizer"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "request.normalize" => super::core::reply(tagged_body("normalized_by", example().id)),
        _ => unknown(method),
    }
}
