// ref: examples/plugin/host-callback/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleHost, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration(
        "example-host-callback-go",
        &["management_api", "host_callback"],
    )
}
pub fn handle(host: &dyn ExampleHost) -> ExampleResult {
    host.call(
        "host.log",
        json!({"level":"info","message":"example host callback"}),
    )?;
    let response = host.call(
        "host.http.do",
        json!({"method":"GET","url":"https://example.com"}),
    )?;
    super::core::reply(response)
}
