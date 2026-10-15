// ref: internal/translator/antigravity/openai/responses/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::{
    convert_antigravity_response_to_openai_responses_non_stream_with_state,
    convert_antigravity_response_to_openai_responses_stream,
    convert_openai_responses_request_to_antigravity, AntigravityToResponsesState,
};
use crate::sdk::translator::{
    antigravity, openai_response, Registry, ResponseTransform, TranslationState,
};

/// Activates the request and both response directions.
pub fn register_openai_responses_antigravity(registry: &Registry) {
    registry.register_pair(
        openai_response(),
        antigravity(),
        Arc::new(convert_openai_responses_request_to_antigravity),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, original_request, request, raw, state| {
                let state = antigravity_state(state);
                convert_antigravity_response_to_openai_responses_stream(
                    original_request,
                    request,
                    raw,
                    state,
                )
            })),
            non_stream: Some(Arc::new(|_, _, original_request, request, raw, state| {
                convert_antigravity_response_to_openai_responses_non_stream_with_state(
                    original_request,
                    request,
                    raw,
                    antigravity_state(state),
                )
            })),
            token_count: None,
        },
    );
}

fn antigravity_state(state: &mut TranslationState) -> &mut AntigravityToResponsesState {
    let needs_state = match state.as_ref() {
        Some(value) => !value.is::<AntigravityToResponsesState>(),
        None => true,
    };
    if needs_state {
        *state = Some(Box::new(AntigravityToResponsesState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<AntigravityToResponsesState>())
        .expect("Antigravity state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;

}
