// ref: internal/runtime/executor/xai_executor_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only
// Candidate: xai_executor_request.go:58-91 @ 0d18d3b365be000eeaee7de4615f9da596b2059d

use super::helps::{
    CodexMultiAgentV2Processor, RegistryCodexMultiAgentV2Processor, RequestThinkingEngine,
    RequestThinkingInput, RequestThinkingPipeline,
};
use super::xai_executor_request::{shape_xai_responses_body, XaiPreparedRequest, XaiRequestPolicy};
use crate::internal::thinking::ThinkingEngine;
use crate::sdk::cliproxy::executor::{Options, Request};
use crate::sdk::translator::{Format, Registry, TranslationContext};
use std::sync::Arc;

/// Request-scoped model decisions use the host's registries and selected private
/// capability. No process-global plugin, configuration or model owner is added.
pub struct XaiRequestOwner {
    registry: Arc<Registry>,
    thinking: RequestThinkingPipeline,
    processor: Option<Arc<dyn CodexMultiAgentV2Processor + Send + Sync>>,
    context: TranslationContext,
}
impl Default for XaiRequestOwner {
    fn default() -> Self {
        Self::new(
            Arc::new(Registry::new()),
            Arc::new(ThinkingEngine::default()),
        )
    }
}
impl XaiRequestOwner {
    pub fn new(registry: Arc<Registry>, engine: Arc<ThinkingEngine>) -> Self {
        Self {
            thinking: RequestThinkingPipeline::new(engine, registry.clone()),
            registry,
            processor: None,
            context: TranslationContext::default(),
        }
    }
    #[must_use]
    pub fn with_processor(
        mut self,
        processor: Arc<dyn CodexMultiAgentV2Processor + Send + Sync>,
    ) -> Self {
        self.processor = Some(processor);
        self
    }
    pub fn prepare(
        &self,
        request: &Request,
        options: &Options,
        stream: bool,
        inject_x_search: bool,
    ) -> Result<XaiPreparedRequest, serde_json::Error> {
        let from = if !options.source_format.as_str().is_empty() {
            options.source_format.clone()
        } else if !request.format.as_str().is_empty() {
            request.format.clone()
        } else {
            Format::from("openai-response")
        };
        let to = Format::from("codex");
        let selected = options
            .metadata
            .resolved_api_key_model_info
            .as_deref()
            .or(request.metadata.resolved_api_key_model_info.as_deref());
        let original = if options.original_request.is_empty() {
            &request.payload
        } else {
            &options.original_request
        };
        let mut current = request.payload.clone();
        if let Some(effort) = options
            .metadata
            .reasoning_effort
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            let mut body: serde_json::Value = serde_json::from_slice(&current)?;
            if let Some(object) = body.as_object_mut() {
                let reasoning = object
                    .entry("reasoning")
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(reasoning) = reasoning.as_object_mut() {
                    reasoning.insert("effort".into(), effort.trim().into());
                }
            }
            current = serde_json::to_vec(&body)?;
        }
        let translate = |processor: &dyn CodexMultiAgentV2Processor| {
            super::helps::translate_request_with_api_key_model_compatibility_and_update_intent_for_executor(
                processor, &options.headers, "xai", &from, &to,
                request.model.split_once('(').map_or(request.model.as_str(), |(m,_)| m).trim(),
                &current, stream, selected.is_some_and(|m| m.is_compat))
        };
        let (body, normalized_updates_changed) = if let Some(processor) = &self.processor {
            translate(processor.as_ref())
        } else {
            let client = crate::internal::client::codex::optimize_multi_agent_v2::MultiAgentV2Context::default();
            let metadata = |_: &str| None;
            translate(&RegistryCodexMultiAgentV2Processor {
                registry: &self.registry,
                context: &self.context,
                client: &client,
                model_metadata: &metadata,
                orphan_delegation_compatibility: false,
            })
        };
        let body = self
            .thinking
            .apply_request_thinking(RequestThinkingInput {
                body: &body,
                current_source_payload: &current,
                original_source_payload: original,
                model: &request.model,
                from_format: from.as_str(),
                to_format: to.as_str(),
                provider: "xai",
                normalized_updates_changed,
                resolved_model_info: None,
                resolved_config_model_info: selected,
            })
            .map_err(|error| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                ))
            })?;
        shape_xai_responses_body(
            &body,
            XaiRequestPolicy {
                model: &request.model,
                stream,
                inject_x_search,
                session_id: options.metadata.execution_session_id.as_deref(),
                reasoning_effort: None,
            },
        )
    }
}
