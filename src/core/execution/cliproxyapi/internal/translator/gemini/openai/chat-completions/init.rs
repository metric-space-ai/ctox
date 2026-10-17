// ref: internal/translator/gemini/openai/chat-completions/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{gemini, openai, Registry, ResponseTransform, TranslationState};

use super::{
    convert_gemini_response_to_openai_chat_non_stream,
    convert_gemini_response_to_openai_chat_stream, convert_openai_chat_request_to_gemini,
    GeminiToChatStreamState,
};

pub fn register_openai_chat_gemini(registry: &Registry) {
    registry.register_pair(
        openai(),
        gemini(),
        Arc::new(convert_openai_chat_request_to_gemini),
        ResponseTransform {
            stream: Some(Arc::new(|_, model, original, request, raw, state| {
                convert_gemini_response_to_openai_chat_stream(
                    model,
                    original,
                    request,
                    raw,
                    gemini_chat_state(state),
                )
            })),
            non_stream: Some(Arc::new(|_, _, original, request, raw, _| {
                convert_gemini_response_to_openai_chat_non_stream(original, request, raw)
            })),
            token_count: None,
        },
    );
}

fn gemini_chat_state(state: &mut TranslationState) -> &mut GeminiToChatStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<GeminiToChatStreamState>());
    if replace {
        *state = Some(Box::new(GeminiToChatStreamState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<GeminiToChatStreamState>())
        .expect("Gemini Chat state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;
}
