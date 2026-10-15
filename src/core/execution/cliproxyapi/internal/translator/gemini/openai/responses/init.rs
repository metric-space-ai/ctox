// ref: internal/translator/gemini/openai/responses/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use super::{
    convert_gemini_response_to_openai_responses_non_stream,
    convert_gemini_response_to_openai_responses_stream, convert_openai_responses_request_to_gemini,
    GeminiToResponsesState,
};
use crate::sdk::translator::{
    gemini, openai_response, Registry, ResponseTransform, TranslationState,
};

/// Activates the verified request plus non-stream and stream response paths.
pub fn register_openai_responses_gemini_request(registry: &Registry) {
    registry.register_pair(
        openai_response(),
        gemini(),
        Arc::new(convert_openai_responses_request_to_gemini),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, original_request, request, raw, state| {
                let state = gemini_state(state);
                convert_gemini_response_to_openai_responses_stream(
                    original_request,
                    request,
                    raw,
                    state,
                )
            })),
            non_stream: Some(Arc::new(|_, _, original_request, request, raw, _| {
                convert_gemini_response_to_openai_responses_non_stream(
                    original_request,
                    request,
                    raw,
                )
            })),
            token_count: None,
        },
    );
}

fn gemini_state(state: &mut TranslationState) -> &mut GeminiToResponsesState {
    let needs_state = match state.as_ref() {
        Some(value) => !value.is::<GeminiToResponsesState>(),
        None => true,
    };
    if needs_state {
        *state = Some(Box::new(GeminiToResponsesState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<GeminiToResponsesState>())
        .expect("Gemini state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;

}
