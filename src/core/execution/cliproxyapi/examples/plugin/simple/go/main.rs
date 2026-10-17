// ref: examples/plugin/simple/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleRegistration};
pub fn example() -> ExampleRegistration {
    registration(
        "example-simple",
        &[
            "model_registrar",
            "model_provider",
            "auth_provider",
            "frontend_auth_provider",
            "executor",
            "request_translator",
            "request_normalizer",
            "response_translator",
            "thinking_applier",
            "usage_plugin",
            "command_line_plugin",
            "management_api",
        ],
    )
}
#[derive(Debug, Default)]
pub struct SimpleExample {
    usage_count: u64,
}
impl SimpleExample {
    pub fn echo(&self, body: &[u8]) -> Vec<u8> {
        body.to_vec()
    }
    pub fn record_usage(&mut self) {
        self.usage_count += 1;
    }
    pub fn usage_count(&self) -> u64 {
        self.usage_count
    }
}
