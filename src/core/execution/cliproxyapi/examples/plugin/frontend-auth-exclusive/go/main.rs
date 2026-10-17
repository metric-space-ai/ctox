// ref: examples/plugin/frontend-auth-exclusive/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleRegistration};
pub fn example() -> ExampleRegistration {
    registration(
        "example-frontend-auth-exclusive-go",
        &["frontend_auth_provider", "exclusive"],
    )
}
pub fn authenticate(header: Option<&str>) -> bool {
    header == Some("exclusive")
}
