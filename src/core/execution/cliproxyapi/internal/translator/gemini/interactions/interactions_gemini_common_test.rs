// ref: internal/translator/gemini/interactions/interactions_gemini_common_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    convert_gemini_request_to_interactions, convert_gemini_response_to_interactions_non_stream,
    convert_gemini_response_to_interactions_stream, convert_interactions_request_to_gemini,
    convert_interactions_response_to_gemini_non_stream, GeminiToInteractionsState,
};
