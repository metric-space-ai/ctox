// ref: examples/plugin/host-callback-auth-files/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleHost, ExampleRegistration, ExampleResult};
use serde_json::{json, Value};
pub fn example() -> ExampleRegistration {
    registration(
        "example-host-callback-auth-files-go",
        &["management_api", "host_auth_files"],
    )
}
pub fn list(host: &dyn ExampleHost) -> ExampleResult {
    super::core::reply(host.call("host.auth.list", json!({}))?)
}
pub fn get(host: &dyn ExampleHost, id: &str) -> Result<Value, super::core::ExampleError> {
    host.call("host.auth.get", json!({"id":id}))
}
