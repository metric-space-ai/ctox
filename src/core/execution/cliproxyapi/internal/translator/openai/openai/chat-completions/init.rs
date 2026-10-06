// ref: internal/translator/openai/openai/chat-completions/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{openai, Registry, ResponseTransform};

use super::{
    convert_openai_request_to_openai, convert_openai_response_to_openai_non_stream,
    convert_openai_response_to_openai_with_state,
};

pub fn register_openai_chat_passthrough(registry: &Registry) {
    registry.register_pair(
        openai(),
        openai(),
        Arc::new(convert_openai_request_to_openai),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, raw, state| {
                convert_openai_response_to_openai_with_state(raw, state)
            })),
            non_stream: Some(Arc::new(|_, _, _, _, raw, _| {
                convert_openai_response_to_openai_non_stream(raw)
            })),
            token_count: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_activates_same_format_pair() {
        let registry = Registry::new();
        register_openai_chat_passthrough(&registry);
        assert!(registry.has_request_transformer(&openai(), &openai()));
        assert!(registry.has_stream_response_transformer(&openai(), &openai()));
        assert!(registry.has_non_stream_response_transformer(&openai(), &openai()));
    }
}
