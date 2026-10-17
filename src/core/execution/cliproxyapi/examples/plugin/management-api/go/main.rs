// ref: examples/plugin/management-api/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleRegistration};
pub fn example() -> ExampleRegistration {
    registration("example-management-api-go", &["management_api"])
}
pub fn status_page() -> &'static [u8] {
    b"<!doctype html><title>Management API</title><main>Management API resource</main>"
}
