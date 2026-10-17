// ref: examples/plugin/cli/go/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::core::{registration, reply, unknown, ExampleRegistration, ExampleResult};
use serde_json::json;
pub fn example() -> ExampleRegistration {
    registration("example-cli-go", &["command_line_plugin"])
}
pub fn handle(method: &str) -> ExampleResult {
    match method {
        "command_line.register" => reply(
            json!({"Flags": [{"Name":"example-cli-go-command","Usage":"Run the example plugin command","Type":"bool"}]}),
        ),
        "command_line.execute" => {
            reply(json!({"Stdout": b"example-cli-go command executed\n", "ExitCode": 0}))
        }
        _ => unknown(method),
    }
}
