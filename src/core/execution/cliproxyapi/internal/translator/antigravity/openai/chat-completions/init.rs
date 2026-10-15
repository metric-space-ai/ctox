// ref: internal/translator/antigravity/openai/chat-completions/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{antigravity, openai, Registry, ResponseTransform, TranslationState};

use super::{
    convert_antigravity_response_to_openai_chat_non_stream,
    convert_antigravity_response_to_openai_chat_stream, convert_openai_chat_request_to_antigravity,
    AntigravityToChatStreamState,
};

pub fn register_openai_chat_antigravity(registry: &Registry) {
    registry.register_pair(
        openai(),
        antigravity(),
        Arc::new(convert_openai_chat_request_to_antigravity),
        ResponseTransform {
            stream: Some(Arc::new(|_, model, original, request, raw, state| {
                convert_antigravity_response_to_openai_chat_stream(
                    model,
                    original,
                    request,
                    raw,
                    antigravity_chat_state(state),
                )
            })),
            non_stream: Some(Arc::new(|_, _, original, request, raw, _| {
                convert_antigravity_response_to_openai_chat_non_stream(original, request, raw)
            })),
            token_count: None,
        },
    );
}

fn antigravity_chat_state(state: &mut TranslationState) -> &mut AntigravityToChatStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<AntigravityToChatStreamState>());
    if replace {
        *state = Some(Box::new(AntigravityToChatStreamState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<AntigravityToChatStreamState>())
        .expect("Antigravity Chat state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;
}
