// ref: internal/runtime/executor/helps/model_capabilities.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::registry::ModelInfo;
use crate::internal::thinking::ThinkingError;
use crate::sdk::cliproxy::executor::{Options, Request};

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

/// Adapter implemented by the canonical top-level thinking pipeline once it is
/// available. Keeping this boundary injected avoids cloning its capability or
/// JSON-mutation rules into executor helpers.
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
