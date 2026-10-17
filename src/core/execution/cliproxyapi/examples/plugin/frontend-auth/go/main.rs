// ref: examples/plugin/frontend-auth/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration("example-frontend-auth-go", &["frontend_auth_provider"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "frontend_auth.identifier" => reply(json!({"identifier":example().id})),
        "frontend_auth.authenticate" => reply(
            json!({"Authenticated":true,"Principal":example().id,"Metadata":{"provider":example().id}}),
        ),
        _ => unknown(method),
    }
}
