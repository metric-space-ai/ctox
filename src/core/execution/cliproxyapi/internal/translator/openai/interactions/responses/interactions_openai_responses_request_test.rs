// ref: internal/translator/openai/interactions/responses/interactions_openai_responses_request_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_interactions_request_to_openai_responses,
    convert_openai_responses_request_to_interactions,
};
use serde_json::{json, Value};

fn value(raw: Vec<u8>) -> Value {
    serde_json::from_slice(&raw).unwrap()
}
