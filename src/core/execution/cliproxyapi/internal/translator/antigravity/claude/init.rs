// ref: internal/translator/antigravity/claude/init.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::sdk::translator::{antigravity, claude, Registry, ResponseTransform, TranslationState};

use super::{
    claude_token_count, convert_antigravity_response_to_claude_non_stream,
    convert_antigravity_response_to_claude_stream,
    convert_claude_request_to_antigravity_with_capabilities, AntigravityClaudeRequestCapabilities,
    AntigravityClaudeStreamState,
};

/// Resolves provider-discovered capabilities for the exact model selected by
/// the runtime. The translator deliberately does not infer capabilities from a
/// model-name pattern: unknown models remain fail-closed.
pub type AntigravityClaudeCapabilityResolver =
    Arc<dyn Fn(&str) -> AntigravityClaudeRequestCapabilities + Send + Sync + 'static>;

/// Registers the Claude Messages ↔ Antigravity pair without claiming dynamic
/// provider capabilities. Runtime assembly can use
/// [`register_claude_antigravity_with_capability_resolver`] once its model
/// catalog is available.
pub fn register_claude_antigravity(registry: &Registry) {
    register_claude_antigravity_with_capability_resolver(
        registry,
        Arc::new(|_| AntigravityClaudeRequestCapabilities::default()),
    );
}

/// Registers the complete request, stream, aggregate, and token-count surface
/// with an explicit capability source owned by the caller.
pub fn register_claude_antigravity_with_capability_resolver(
    registry: &Registry,
    capabilities: AntigravityClaudeCapabilityResolver,
) {
    registry.register_pair(
        claude(),
        antigravity(),
        Arc::new(move |model, raw, stream| {
            convert_claude_request_to_antigravity_with_capabilities(
                model,
                raw,
                stream,
                capabilities(model),
            )
        }),
        ResponseTransform {
            stream: Some(Arc::new(|_, _, original, translated, raw, state| {
                let state = registered_stream_state(state);
                convert_antigravity_response_to_claude_stream(
                    original,
                    translated,
                    raw,
                    &mut state.converter,
                    &state.web_search_tool_use_id,
                )
            })),
            non_stream: Some(Arc::new(|_, _, original, translated, raw, _| {
                convert_antigravity_response_to_claude_non_stream(
                    original,
                    translated,
                    raw,
                    &new_web_search_tool_use_id(),
                )
            })),
            token_count: Some(Arc::new(|_, count| claude_token_count(count))),
        },
    );
}

#[derive(Default)]
struct RegisteredClaudeStreamState {
    converter: AntigravityClaudeStreamState,
    web_search_tool_use_id: String,
}

fn registered_stream_state(state: &mut TranslationState) -> &mut RegisteredClaudeStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<RegisteredClaudeStreamState>());
    if replace {
        *state = Some(Box::new(RegisteredClaudeStreamState {
            converter: AntigravityClaudeStreamState::default(),
            web_search_tool_use_id: new_web_search_tool_use_id(),
        }));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<RegisteredClaudeStreamState>())
        .expect("Claude Antigravity state was initialized with the expected type")
}

fn new_web_search_tool_use_id() -> String {
    format!("srvtoolu_{}", uuid::Uuid::new_v4().as_simple())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::sdk::translator::TranslationContext;

    const WEB_SEARCH_REQUEST: &[u8] = br#"{"messages":[{"role":"user","content":"weather"}],"tools":[{"type":"web_search_20250305","name":"web_search"}]}"#;

#[test]
    fn default_registration_is_capability_fail_closed() {
        let registry = Registry::new();
        register_claude_antigravity(&registry);
        let output = registry.translate_request(
            &TranslationContext::default(),
            &claude(),
            &antigravity(),
            "gemini-3.1-flash-lite",
            WEB_SEARCH_REQUEST,
            false,
        );
        let output: Value = serde_json::from_slice(&output).unwrap();
        assert!(output.get("requestType").is_none());
        assert!(output.pointer("/request/tools/0/googleSearch").is_none());
    }

#[test]
    fn injected_runtime_capability_enables_only_the_selected_model() {
        let registry = Registry::new();
        register_claude_antigravity_with_capability_resolver(
            &registry,
            Arc::new(|model| AntigravityClaudeRequestCapabilities {
                native_google_search: model == "gemini-3.1-flash-lite",
            }),
        );
        let capable = registry.translate_request(
            &TranslationContext::default(),
            &claude(),
            &antigravity(),
            "gemini-3.1-flash-lite",
            WEB_SEARCH_REQUEST,
            false,
        );
        let incapable = registry.translate_request(
            &TranslationContext::default(),
            &claude(),
            &antigravity(),
            "gemini-unknown",
            WEB_SEARCH_REQUEST,
            false,
        );
        let capable: Value = serde_json::from_slice(&capable).unwrap();
        let incapable: Value = serde_json::from_slice(&incapable).unwrap();
        assert_eq!(capable["requestType"], "web_search");
        assert!(capable.pointer("/request/tools/0/googleSearch").is_some());
        assert!(incapable.get("requestType").is_none());
    }
}
