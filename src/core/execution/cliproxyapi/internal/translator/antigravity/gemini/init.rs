// ref: internal/translator/antigravity/gemini/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{antigravity, gemini, Registry, ResponseTransform};

use super::{
    convert_antigravity_response_to_gemini_non_stream, convert_gemini_request_to_antigravity,
    gemini_token_count,
};

/// The upstream stream converter depends on a context `alt` value not present
/// in CTOX's synchronous registry contract; direct callers use the explicit-alt
/// function while registry streaming keeps the normal empty-alt route.
pub fn register_gemini_antigravity(registry: &Registry) {
    registry.register_pair(
        gemini(),
        antigravity(),
        Arc::new(convert_gemini_request_to_antigravity),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, original, _, raw, _| {
                super::convert_antigravity_response_to_gemini(original, raw, Some(""))
            })),
            non_stream: Some(Arc::new(|_, _, original, _, raw, _| {
                convert_antigravity_response_to_gemini_non_stream(original, raw)
            })),
            token_count: Some(Arc::new(|_, count| gemini_token_count(count))),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

}
