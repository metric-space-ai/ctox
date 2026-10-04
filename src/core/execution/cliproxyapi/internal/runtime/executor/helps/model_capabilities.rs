// ref: internal/runtime/executor/helps/model_capabilities.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::registry::ModelInfo;
use std::sync::Arc;

use crate::internal::thinking::{
    ModelInfoView, ResolvedCapabilityThinkingRequest, ThinkingEngine, ThinkingError,
};
use crate::sdk::cliproxy::executor::{Options, Request};
use crate::sdk::translator::Registry;

/// Complete request view passed to the canonical thinking engine.
///
/// The executor helper only resolves source-payload precedence and forwards an
/// exact configured model capability. Extraction, validation, registry lookup,
/// and provider mutation remain owned by `internal::thinking`.
#[derive(Debug, Clone, Copy)]
pub struct RequestThinkingInput<'a> {
    pub body: &'a [u8],
    pub current_source_payload: &'a [u8],
    pub original_source_payload: &'a [u8],
    pub model: &'a str,
    pub from_format: &'a str,
    pub to_format: &'a str,
    pub provider: &'a str,
    pub resolved_model_info: Option<&'a ModelInfo>,
    /// Manager-selected owned capability. When present this takes precedence
    /// over the static registry view, without leaking dynamic strings.
    pub resolved_config_model_info: Option<&'a crate::internal::modelconfig::ModelInfo>,
}

/// Canonical top-level thinking boundary. It resolves summary intent with the
/// owning translator registry and shares capability validation/provider mutation
/// with the engine instead of duplicating those rules in executor helpers.
pub trait RequestThinkingEngine {
    fn apply_request_thinking(
        &self,
        input: RequestThinkingInput<'_>,
    ) -> Result<Vec<u8>, ThinkingError>;
}

#[derive(Debug, Clone, Copy)]
pub struct RequestThinkingRoute<'a> {
    pub from_format: &'a str,
    pub to_format: &'a str,
    pub provider: &'a str,
    pub resolved_model_info: Option<&'a ModelInfo>,
    /// Manager-selected owned capability. When present this takes precedence
    /// over the static registry view, without leaking dynamic strings.
    pub resolved_config_model_info: Option<&'a crate::internal::modelconfig::ModelInfo>,
}

/// Preserves the upstream executor routing rule: an explicitly selected API
/// key model definition wins; otherwise the thinking engine performs its own
/// canonical registry lookup.
pub fn apply_request_thinking<Engine>(
    engine: &Engine,
    body: &[u8],
    request: &Request,
    options: &Options,
    route: RequestThinkingRoute<'_>,
) -> Result<Vec<u8>, ThinkingError>
where
    Engine: RequestThinkingEngine + ?Sized,
{
    let original_source_payload = if options.original_request.is_empty() {
        request.payload.as_slice()
    } else {
        options.original_request.as_slice()
    };
    engine.apply_request_thinking(RequestThinkingInput {
        body,
        current_source_payload: &request.payload,
        original_source_payload,
        model: &request.model,
        from_format: route.from_format,
        to_format: route.to_format,
        provider: route.provider,
        resolved_model_info: route.resolved_model_info,
        resolved_config_model_info: route.resolved_config_model_info,
    })
}

/// Owner-scoped bridge from account-selected model records to the complete
/// canonical pipeline. Both registries belong to the same gateway instance.
#[derive(Clone)]
pub struct RequestThinkingPipeline {
    engine: Arc<ThinkingEngine>,
    translators: Arc<Registry>,
}

impl RequestThinkingPipeline {
    pub fn new(engine: Arc<ThinkingEngine>, translators: Arc<Registry>) -> Self {
        Self {
            engine,
            translators,
        }
    }
}

impl RequestThinkingEngine for RequestThinkingPipeline {
    fn apply_request_thinking(
        &self,
        input: RequestThinkingInput<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let summary = super::thinking::translated_request_summary_config(
            &self.translators,
            input.body,
            input.current_source_payload,
            input.original_source_payload,
            input.model,
            input.from_format,
            input.to_format,
        );
        // ref: internal/runtime/executor/helps/model_capabilities.go:22-31 @ d7914afd
        // Current/plugin-normalized source owns effort; original source participates
        // in summary preservation. Empty current input falls back to the original.
        let source_body = if input.current_source_payload.is_empty() {
            input.original_source_payload
        } else {
            input.current_source_payload
        };
        let view = input
            .resolved_config_model_info
            .map(ModelInfoView::from)
            .or_else(|| input.resolved_model_info.map(ModelInfoView::from));
        self.engine.apply_thinking_with_capability_info_and_summary(
            ResolvedCapabilityThinkingRequest {
                body: input.body,
                source_body,
                model: input.model,
                from_format: input.from_format,
                to_format: input.to_format,
                provider_key: input.provider,
                model_info: view.as_ref(),
                model_info_resolved: view.is_some(),
            },
            &summary,
        )
    }
}
