// ref: examples/plugin/response-normalizer/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, tagged_body, unknown, ExampleRegistration, ExampleResult};
pub fn example() -> ExampleRegistration {
    registration(
        "example-response-normalizer-go",
        &["response_before_translator", "response_after_translator"],
    )
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "response.normalize_before" => {
            super::core::reply(tagged_body("response_normalized_before_by", example().id))
        }
        "response.normalize_after" => {
            super::core::reply(tagged_body("response_normalized_after_by", example().id))
        }
        _ => unknown(method),
    }
}
