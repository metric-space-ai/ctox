// ref: internal/translator/openai/openai/responses/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::{
    convert_openai_chat_completions_response_to_openai_responses,
    convert_openai_chat_completions_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_openai_chat_completions,
};
use crate::sdk::translator::{openai, openai_response, Registry, ResponseTransform};

/// Registers the OpenAI Responses ↔ OpenAI Chat Completions request and
/// response paths. The pair is symmetric: the request path takes a
/// Responses request and emits a Chat Completions body, while the
/// response paths translate provider Chat Completions back into the
/// Responses wire format (SSE for streaming, single JSON otherwise).
pub fn register_openai_responses_chat_completions(registry: &Registry) {
    registry.register_pair(
        openai_response(),
        openai(),
        Arc::new(convert_openai_responses_request_to_openai_chat_completions),
        ResponseTransform {
            stream: Some(Arc::new(
                convert_openai_chat_completions_response_to_openai_responses,
            )),
            non_stream: Some(Arc::new(
                convert_openai_chat_completions_response_to_openai_responses_non_stream,
            )),
            token_count: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;
}
