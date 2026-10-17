// ref: examples/plugin/host-model-callback/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, ExampleHost, ExampleRegistration};
use serde_json::{json, Value};
pub fn example() -> ExampleRegistration {
    registration(
        "example-host-model-callback-go",
        &["management_api", "host_model_execute"],
    )
}
pub fn execute(
    host: &dyn ExampleHost,
    model: &str,
    body: Value,
) -> Result<Value, super::core::ExampleError> {
    host.call(
        "host.model.execute",
        json!({"model":model,"body":body,"stream":false}),
    )
}
