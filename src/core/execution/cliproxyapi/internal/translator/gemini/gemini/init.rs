// ref: internal/translator/gemini/gemini/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{gemini, Registry, ResponseTransform};

use super::{
    convert_gemini_request_to_gemini, gemini_token_count, passthrough_gemini_response_non_stream,
    passthrough_gemini_response_stream,
};

pub fn register_gemini_passthrough(registry: &Registry) {
    registry.register_pair(
        gemini(),
        gemini(),
        Arc::new(convert_gemini_request_to_gemini),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, raw, _| {
                passthrough_gemini_response_stream(raw)
            })),
            non_stream: Some(Arc::new(|_, _, _, _, raw, _| {
                passthrough_gemini_response_non_stream(raw)
            })),
            token_count: Some(Arc::new(|_, count| gemini_token_count(count))),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
}
