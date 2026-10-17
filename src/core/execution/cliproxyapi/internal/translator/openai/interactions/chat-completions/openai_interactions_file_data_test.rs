// ref: internal/translator/openai/interactions/chat-completions/openai_interactions_file_data_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use gjson;
use serde_json::Value;

use super::convert_openai_request_to_interactions;

fn parse(raw: &[u8]) -> Value {
    serde_json::from_slice(raw).unwrap()
}
