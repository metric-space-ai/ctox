// ref: internal/runtime/executor/devin_executor_test.go:144-320,508-580,2600-2702
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_executor_history::{
    parse_devin_interactions_payload, parse_devin_signature_bytes,
};
use super::helps::devin_request::DEVIN_DEFAULT_MAX_TOKENS;
use base64::{engine::general_purpose, Engine as _};
