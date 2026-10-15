// ref: examples/plugin/model/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration("example-model-go", &["model_provider"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "model.static" | "model.for_auth" => reply(
            json!({"Provider":example().id,"Models":[{"ID":"example-model-go-model","Object":"model","OwnedBy":example().id,"DisplayName":"Model Example Model","SupportedGenerationMethods":["chat"],"ContextLength":8192,"MaxCompletionTokens":1024,"UserDefined":true}]}),
        ),
        _ => unknown(method),
    }
}
