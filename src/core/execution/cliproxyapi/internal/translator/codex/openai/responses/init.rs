// ref: internal/translator/codex/openai/responses/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{codex, openai_response, Registry, ResponseTransform};

use super::{
    convert_codex_response_to_openai_responses,
    convert_codex_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_codex,
};

pub fn register_openai_responses_codex(registry: &Registry) {
    registry.register_pair(
        openai_response(),
        codex(),
        Arc::new(convert_openai_responses_request_to_codex),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, raw, _| {
                convert_codex_response_to_openai_responses(raw)
            })),
            non_stream: Some(Arc::new(|_, _, _, _, raw, _| {
                convert_codex_response_to_openai_responses_non_stream(raw)
            })),
            token_count: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;

}
