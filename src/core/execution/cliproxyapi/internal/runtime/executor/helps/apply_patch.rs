// ref: internal/runtime/executor/helps/apply_patch.go:11-12,73-85,93-99
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — original winning declaration classification
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::util::responses_tool_reverse_identity_map;
use crate::sdk::pluginapi::ExecutorRequest;

pub const APPLY_PATCH_UPSTREAM_ERROR_MESSAGE: &str =
    "Invalid apply_patch tool arguments received from upstream.";

pub fn apply_patch_requested(original: &[u8]) -> bool {
    responses_tool_reverse_identity_map(original)
        .values()
        .any(|identity| identity.apply_patch)
}

pub fn is_apply_patch_upstream_tool(original: &[u8], name: &str) -> bool {
    responses_tool_reverse_identity_map(original)
        .get(name)
        .is_some_and(|identity| identity.apply_patch)
}

pub fn apply_patch_original_request(request: &ExecutorRequest) -> &[u8] {
    if request.original_request.is_empty() {
        &request.payload
    } else {
        &request.original_request
    }
}
