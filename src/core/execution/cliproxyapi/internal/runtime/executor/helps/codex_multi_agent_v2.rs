// ref: internal/runtime/executor/helps/codex_multi_agent_v2.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::codex_tool_schema::{
    is_codex_target_executor, is_codex_user_agent, normalize_codex_tool_integer_types,
};
use crate::sdk::cliproxy::executor::Headers;
use crate::sdk::translator::Format;

/// Canonical Codex multi-agent-v2 payload processor supplied by the host/client
/// layer. Eligibility, model catalogs, translation, and JSON mutation stay in
/// that single owner; these executor helpers are deliberately byte-transparent.
pub trait CodexMultiAgentV2Processor {
    fn rewrite_spawn_agent_description(&self, headers: &Headers, payload: &[u8]) -> Vec<u8>;

    fn rewrite_input(&self, headers: &Headers, payload: &[u8]) -> Vec<u8>;

    fn translate_request(
        &self,
        headers: &Headers,
        from: &Format,
        to: &Format,
        model: &str,
        payload: &[u8],
        stream: bool,
    ) -> Vec<u8>;

    fn optimize_request(&self, headers: &Headers, payload: &[u8]) -> (Vec<u8>, bool);

    fn restore_response(&self, payload: &[u8], optimized: bool) -> Vec<u8>;
}

#[must_use]
pub fn rewrite_codex_spawn_agent_description<Processor>(
    processor: &Processor,
    headers: &Headers,
    payload: &[u8],
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    processor.rewrite_spawn_agent_description(headers, payload)
}

#[must_use]
pub fn rewrite_codex_multi_agent_v2_input<Processor>(
    processor: &Processor,
    headers: &Headers,
    payload: &[u8],
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    processor.rewrite_input(headers, payload)
}

#[must_use]
pub fn translate_request_with_codex_multi_agent_v2<Processor>(
    processor: &Processor,
    headers: &Headers,
    from: &Format,
    to: &Format,
    model: &str,
    payload: &[u8],
    stream: bool,
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    processor.translate_request(headers, from, to, model, payload, stream)
}

#[must_use]
pub fn optimize_codex_multi_agent_v2_request<Processor>(
    processor: &Processor,
    headers: &Headers,
    payload: &[u8],
) -> (Vec<u8>, bool)
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    processor.optimize_request(headers, payload)
}

#[must_use]
pub fn restore_codex_multi_agent_v2_response<Processor>(
    processor: &Processor,
    payload: &[u8],
    optimized: bool,
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    processor.restore_response(payload, optimized)
}

/// Applies reserved-field integer normalization for a non-Codex target, then
/// hands the bytes to the host processor. The byte-transparent wrapper does
/// not normalize.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn translate_request_with_codex_multi_agent_v2_for_executor<Processor>(
    processor: &Processor,
    headers: &Headers,
    target_executor: &str,
    from: &Format,
    to: &Format,
    model: &str,
    payload: &[u8],
    stream: bool,
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    let normalized = codex_client_integer_payload(headers, target_executor, payload);
    processor.translate_request(headers, from, to, model, &normalized, stream)
}

/// Integer-normalizes once, then uses the host processor.
///
/// Upstream's compatibility mode switches six `Convert*WithCompat` translators,
/// rewrites orphan delegation input, and applies a thinking summary. Those
/// translators are not in this crate. When `is_compat` is true, the host
/// processor's `translate_request` remains that compatibility translator.
/// Integer normalization still runs first, matching the executor wrapper.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn translate_request_with_api_key_model_compatibility_for_executor<Processor>(
    processor: &Processor,
    headers: &Headers,
    target_executor: &str,
    from: &Format,
    to: &Format,
    model: &str,
    payload: &[u8],
    stream: bool,
    is_compat: bool,
) -> Vec<u8>
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    let _ = is_compat;
    translate_request_with_codex_multi_agent_v2_for_executor(
        processor,
        headers,
        target_executor,
        from,
        to,
        model,
        payload,
        stream,
    )
}

/// Returns the translated body and whether request-scoped configuration
/// updates changed.
///
/// The Go envelope translator reports `ConfigurationUpdatesChanged`. This
/// processor has no such flag, so the second value stays false.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn translate_request_with_api_key_model_compatibility_and_update_intent_for_executor<
    Processor,
>(
    processor: &Processor,
    headers: &Headers,
    target_executor: &str,
    from: &Format,
    to: &Format,
    model: &str,
    payload: &[u8],
    stream: bool,
    is_compat: bool,
) -> (Vec<u8>, bool)
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    (
        translate_request_with_api_key_model_compatibility_for_executor(
            processor,
            headers,
            target_executor,
            from,
            to,
            model,
            payload,
            stream,
            is_compat,
        ),
        false,
    )
}

/// Translates the baseline and working payloads. Identical backing slices are
/// translated once; the working buffer is a separate copy.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn translate_request_pair_with_api_key_model_compatibility_and_update_intent<Processor>(
    processor: &Processor,
    headers: &Headers,
    target_executor: &str,
    from: &Format,
    to: &Format,
    model: &str,
    original_payload: &[u8],
    request_payload: &[u8],
    stream: bool,
    is_compat: bool,
) -> (Vec<u8>, Vec<u8>, bool)
where
    Processor: CodexMultiAgentV2Processor + ?Sized,
{
    let (original, updates_changed) =
        translate_request_with_api_key_model_compatibility_and_update_intent_for_executor(
            processor,
            headers,
            target_executor,
            from,
            to,
            model,
            original_payload,
            stream,
            is_compat,
        );
    if same_byte_slice(original_payload, request_payload) {
        let working = original.clone();
        return (original, working, updates_changed);
    }
    let (working, updates_changed) =
        translate_request_with_api_key_model_compatibility_and_update_intent_for_executor(
            processor,
            headers,
            target_executor,
            from,
            to,
            model,
            request_payload,
            stream,
            is_compat,
        );
    (original, working, updates_changed)
}

fn codex_client_integer_payload(
    headers: &Headers,
    target_executor: &str,
    payload: &[u8],
) -> Vec<u8> {
    if is_codex_user_agent(headers) && !is_codex_target_executor(target_executor) {
        normalize_codex_tool_integer_types(payload, headers)
    } else {
        payload.to_vec()
    }
}

/// Pointer identity, matching Go `sameByteSlice`. Empty slices match.
fn same_byte_slice(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    if left.is_empty() {
        return true;
    }
    std::ptr::eq(left.as_ptr(), right.as_ptr())
}
