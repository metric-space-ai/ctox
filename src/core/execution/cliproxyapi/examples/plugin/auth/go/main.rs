// ref: examples/plugin/auth/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;

pub fn example() -> ExampleRegistration {
    registration("example-auth-go", &["auth_provider"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "auth.identifier" => reply(json!({"identifier": example().id})),
        "auth.parse" => reply(
            json!({"Handled": true, "Auth": {"Provider": example().id, "ID": example().id, "FileName": "example-auth-go.json", "Label": "Auth Example", "StorageJSON": br#"{"type":"example-auth-go","token":"example-token"}"#.to_vec(), "Metadata": {"type": example().id}}}),
        ),
        "auth.login.start" => reply(
            json!({"Provider": example().id, "URL": "https://example.invalid/login", "State": "example-state", "ExpiresAt": "2030-01-01T00:00:00Z"}),
        ),
        "auth.login.poll" => {
            reply(json!({"Status": "success", "Message": "example login complete"}))
        }
        "auth.refresh" => reply(
            json!({"Auth": {"Provider": example().id, "ID": example().id}, "NextRefreshAfter": "2030-01-01T00:00:00Z"}),
        ),
        _ => unknown(method),
    }
}
