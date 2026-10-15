// ref: internal/translator/codex/openai/chat-completions/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{codex, openai, Registry, ResponseTransform, TranslationState};

use super::{
    convert_codex_response_to_openai_chat_non_stream, convert_codex_response_to_openai_chat_stream,
    convert_openai_chat_request_to_codex, CodexToChatStreamState,
};

pub fn register_openai_chat_codex(registry: &Registry) {
    registry.register_pair(
        openai(),
        codex(),
        Arc::new(convert_openai_chat_request_to_codex),
        ResponseTransform {
            stream: Some(Arc::new(|_, model, original, request, raw, state| {
                convert_codex_response_to_openai_chat_stream(
                    model,
                    original,
                    request,
                    raw,
                    codex_chat_state(state),
                )
            })),
            non_stream: Some(Arc::new(|_, _, original, request, raw, _| {
                convert_codex_response_to_openai_chat_non_stream(original, request, raw)
            })),
            token_count: None,
        },
    );
}

fn codex_chat_state(state: &mut TranslationState) -> &mut CodexToChatStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<CodexToChatStreamState>());
    if replace {
        *state = Some(Box::new(CodexToChatStreamState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<CodexToChatStreamState>())
        .expect("Codex Chat state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;

}
