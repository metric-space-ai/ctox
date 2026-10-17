// ref: examples/plugin/request-translator/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, tagged_body, unknown, ExampleRegistration, ExampleResult};
pub fn example() -> ExampleRegistration {
    registration("example-request-translator-go", &["request_translator"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "request.translate" => super::core::reply(tagged_body("translated_by", example().id)),
        _ => unknown(method),
    }
}
