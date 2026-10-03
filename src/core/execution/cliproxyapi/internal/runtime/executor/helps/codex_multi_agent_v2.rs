// ref: internal/runtime/executor/helps/codex_multi_agent_v2.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::codex_tool_schema::{
    is_codex_target_executor, is_codex_user_agent, normalize_codex_tool_integer_types,
};
use crate::internal::client::codex::optimize_multi_agent_v2 as client;
use crate::internal::thinking::{
    apply_summary_config_for_model, extract_translated_summary_config,
};
use crate::sdk::cliproxy::executor::Headers;
use crate::sdk::translator::{Format, Registry, RequestEnvelope, TranslationContext};

/// Host/client owner for canonical client configuration, model catalogs and
/// registry translation. Byte-transparent wrappers delegate; executor helpers
/// compose compatibility facades with this owner's plugin normalizer.
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

    /// Existing host processors retain their byte translator. The portable
    /// registry implementation below preserves the complete envelope.
    fn translate_request_envelope(
        &self,
        headers: &Headers,
        from: &Format,
        to: &Format,
        mut request: RequestEnvelope,
    ) -> RequestEnvelope {
        request.body = self.translate_request(
            headers,
            from,
            to,
            &request.model,
            &request.body,
            request.stream,
        );
        request.format = to.clone();
        request
    }

    fn rewrite_orphan_input(&self, _: &Headers, payload: &[u8]) -> Vec<u8> {
        payload.to_vec()
    }

    fn normalize_compatible_request(
        &self,
        _: &Format,
        _: &Format,
        _: &str,
        payload: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        payload
    }
}

/// Portable client/registry owner used by executor adapters. Configuration and
/// model metadata are injected; provider secrets and process globals stay out.
pub struct RegistryCodexMultiAgentV2Processor<'a> {
    pub registry: &'a Registry,
    pub context: &'a TranslationContext,
    pub client: &'a client::MultiAgentV2Context,
    pub model_metadata: &'a dyn client::SpawnAgentModelMetadataSource,
    pub orphan_delegation_compatibility: bool,
}

impl RegistryCodexMultiAgentV2Processor<'_> {
    fn client_context(&self, headers: &Headers) -> client::MultiAgentV2Context {
        let mut context = (*self.client).clone();
        context.user_agent = header_value(headers, "User-Agent").to_owned();
        context
    }
}

impl CodexMultiAgentV2Processor for RegistryCodexMultiAgentV2Processor<'_> {
    fn rewrite_spawn_agent_description(&self, headers: &Headers, payload: &[u8]) -> Vec<u8> {
        client::rewrite_spawn_agent_description(
            &self.client_context(headers),
            payload,
            self.model_metadata,
        )
    }

    fn rewrite_input(&self, headers: &Headers, payload: &[u8]) -> Vec<u8> {
        client::rewrite_multi_agent_input(&self.client_context(headers), payload)
    }

    fn translate_request(
        &self,
        headers: &Headers,
        from: &Format,
        to: &Format,
        model: &str,
        payload: &[u8],
        stream: bool,
    ) -> Vec<u8> {
        self.translate_request_envelope(
            headers,
            from,
            to,
            RequestEnvelope {
                format: from.clone(),
                model: model.to_owned(),
                stream,
                body: payload.to_vec(),
                configuration_updates_changed: false,
            },
        )
        .body
    }

    fn translate_request_envelope(
        &self,
        headers: &Headers,
        from: &Format,
        to: &Format,
        mut request: RequestEnvelope,
    ) -> RequestEnvelope {
        if from.as_str() == "openai-response" {
            request.body = self.rewrite_orphan_input(headers, &request.body);
            if !matches!(to.as_str(), "codex" | "openai-response") {
                request.body = self.rewrite_input(headers, &request.body);
            }
        }
        self.registry
            .translate_request_envelope(self.context, from, to, request)
    }

    fn rewrite_orphan_input(&self, headers: &Headers, payload: &[u8]) -> Vec<u8> {
        client::rewrite_orphan_delegation_input(
            payload,
            header_value(headers, "X-Openai-Subagent"),
            self.orphan_delegation_compatibility,
        )
    }

    fn normalize_compatible_request(
        &self,
        from: &Format,
        to: &Format,
        model: &str,
        payload: Vec<u8>,
        stream: bool,
    ) -> Vec<u8> {
        self.registry
            .normalize_request(self.context, from, to, model, payload, stream)
    }

    fn optimize_request(&self, headers: &Headers, payload: &[u8]) -> (Vec<u8>, bool) {
        let result =
            client::optimize_request(&self.client_context(headers), payload, self.model_metadata);
        (result.payload, result.namespace_optimized)
    }

    fn restore_response(&self, payload: &[u8], optimized: bool) -> Vec<u8> {
        client::restore_response(payload, optimized)
    }
}

fn header_value<'a>(headers: &'a Headers, name: &str) -> &'a str {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
        .unwrap_or("")
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

/// Integer-normalizes before compatibility dispatch, applies the translated
/// thinking summary, then runs exactly one plugin normalizer.
/// ref: internal/runtime/executor/helps/codex_multi_agent_v2.go:152-190 @ e2bff010
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
    let mut payload = codex_client_integer_payload(headers, target_executor, payload);
    if !is_compat {
        return processor.translate_request(headers, from, to, model, &payload, stream);
    }
    if from.as_str() == "openai-response" {
        payload = processor.rewrite_orphan_input(headers, &payload);
        if !matches!(to.as_str(), "codex" | "openai-response") {
            payload = client::rewrite_multi_agent_input_with_compat(
                &client::MultiAgentV2Context::default(),
                &payload,
                true,
            );
        }
    }

    use crate::internal::translator::{
        claude::openai::{chat_completions, responses},
        codex::claude as codex_claude,
        gemini::claude as gemini_claude,
        interactions::claude as interactions_claude,
        openai::claude as openai_claude,
    };
    let translated = match (from.as_str(), to.as_str()) {
        ("claude", "codex") => {
            codex_claude::convert_claude_request_to_codex_with_compat(model, &payload, stream)
        }
        ("claude", "gemini") => {
            gemini_claude::convert_claude_request_to_gemini_with_compat(model, &payload, stream)
        }
        ("claude", "interactions") => {
            interactions_claude::convert_claude_request_to_interactions_with_compat(
                model, &payload, stream,
            )
        }
        ("claude", "openai") => {
            openai_claude::convert_claude_request_to_openai_with_compat(model, &payload, stream)
        }
        ("openai", "claude") => {
            chat_completions::convert_openai_chat_request_to_claude_with_compat(
                model, &payload, stream,
            )
        }
        ("openai-response", "claude") => {
            responses::convert_openai_responses_request_to_claude_with_compat(
                model, &payload, stream,
            )
        }
        _ => return processor.translate_request(headers, from, to, model, &payload, stream),
    };
    let summary = extract_translated_summary_config(&payload, from.as_str(), to.as_str());
    let translated = apply_summary_config_for_model(&translated, to.as_str(), model, &summary);
    processor.normalize_compatible_request(from, to, model, translated, stream)
}

/// Returns the translated body and whether request-scoped configuration
/// updates changed.
///
/// Normal/envelope routes preserve the plugin normalizer's update decision.
/// Explicit compatibility facade routes return false, matching upstream.
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
    // ref: internal/runtime/executor/helps/codex_multi_agent_v2.go:128-139 @ e2bff010
    if !is_compat || (to.as_str() == "codex" && from.as_str() != "claude") {
        let result = processor.translate_request_envelope(
            headers,
            from,
            to,
            RequestEnvelope {
                format: from.clone(),
                model: model.to_owned(),
                stream,
                body: codex_client_integer_payload(headers, target_executor, payload),
                configuration_updates_changed: false,
            },
        );
        return (result.body, result.configuration_updates_changed);
    }
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
