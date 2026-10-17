// ref: internal/translator/codex/interactions/interactions_codex_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::sdk::translator::TranslationContext;

use super::{
    convert_codex_response_to_interactions_non_stream,
    convert_codex_response_to_interactions_stream, convert_interactions_request_to_codex,
    CodexToInteractionsState,
};
