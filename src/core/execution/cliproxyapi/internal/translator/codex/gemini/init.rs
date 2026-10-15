// ref: internal/translator/codex/gemini/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{codex, gemini, Registry, ResponseTransform, TranslationState};

use super::{
    convert_codex_response_to_gemini_non_stream, convert_codex_response_to_gemini_stream,
    convert_gemini_request_to_codex, gemini_token_count, CodexToGeminiStreamState,
};

pub fn register_gemini_codex(registry: &Registry) {
    registry.register_pair(
        gemini(),
        codex(),
        Arc::new(convert_gemini_request_to_codex),
        ResponseTransform {
            stream: Some(Arc::new(|_, model, original, _, raw, state| {
                convert_codex_response_to_gemini_stream(model, original, raw, stream_state(state))
            })),
            non_stream: Some(Arc::new(|_, model, original, _, raw, _| {
                convert_codex_response_to_gemini_non_stream(model, original, raw)
            })),
            token_count: Some(Arc::new(|_, count| gemini_token_count(count))),
        },
    );
}

fn stream_state(state: &mut TranslationState) -> &mut CodexToGeminiStreamState {
    if state
        .as_ref()
        .is_none_or(|value| !value.is::<CodexToGeminiStreamState>())
    {
        *state = Some(Box::new(CodexToGeminiStreamState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<CodexToGeminiStreamState>())
        .expect("Codex/Gemini state was initialized with the expected type")
}

#[cfg(test)]
mod tests {
    use super::*;
}
