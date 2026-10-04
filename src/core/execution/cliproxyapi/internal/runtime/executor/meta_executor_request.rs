// ref: internal/runtime/executor/meta_executor_execute.go:33-82,271-299 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — injected translation, thinking and selected auth
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{helps::*, meta_executor_auth::meta_credentials};
use crate::internal::translator::common::{delete_raw_path, set_json_string};
use crate::internal::{
    thinking::parse_suffix,
    util::{apply_custom_headers_from_attrs, HeaderRequest},
};
use crate::sdk::cliproxy::auth::{Auth, AuthError};
use crate::sdk::pluginapi::{ExecutorRequest, HttpRequest, PluginExecutionError};
use crate::sdk::translator::Format;
use std::sync::Arc;

pub const META_USER_AGENT: &str =
    "muse-build/1.3.0 (interactive; macos-aarch64; build ac7280f2aca67769d1455a8847bb502b617d50f6)";

pub struct MetaRequestOwner {
    pub processor: Arc<dyn CodexMultiAgentV2Processor + Send + Sync>,
    pub thinking: Arc<dyn RequestThinkingEngine + Send + Sync>,
    pub config: Arc<PayloadApplyConfig>,
}
pub(crate) struct MetaPreparedRequest {
    pub original: Vec<u8>,
    pub body: Vec<u8>,
    pub base_model: String,
    pub from: Format,
    pub target: Format,
    pub patch: ApplyPatchResponsesState,
}

impl MetaRequestOwner {
    pub(crate) fn prepare(
        &self,
        request: &ExecutorRequest,
        stream: bool,
    ) -> Result<MetaPreparedRequest, PluginExecutionError> {
        let base_model = parse_suffix(&request.model).model_name;
        let from = Format::from(request.source_format.as_str());
        let to = Format::from("codex");
        let target = Format::from(if request.format.is_empty() {
            request.source_format.as_str()
        } else {
            request.format.as_str()
        });
        let original = if request.original_request.is_empty() {
            request.payload.clone()
        } else {
            request.original_request.clone()
        };
        let compat = request
            .resolved_home_model_options
            .as_ref()
            .map(|o| o.is_compat)
            .unwrap_or_else(|| {
                request
                    .resolved_model_info
                    .as_ref()
                    .is_some_and(|i| i.is_compat)
            });
        let translate = |payload: &[u8]| {
            translate_request_with_api_key_model_compatibility_for_executor(
                self.processor.as_ref(),
                &request.headers,
                "meta",
                &from,
                &to,
                &base_model,
                payload,
                stream,
                compat,
            )
        };
        let declarations = translate(&original);
        let translated = translate(&request.payload);
        let body = self
            .thinking
            .apply_request_thinking(RequestThinkingInput {
                body: &translated,
                current_source_payload: &request.payload,
                original_source_payload: &original,
                model: &request.model,
                from_format: from.as_str(),
                to_format: to.as_str(),
                provider: "meta",
                resolved_model_info: None,
                resolved_config_model_info: request.resolved_model_info.as_deref(),
            })
            .map_err(|error| Arc::new(error) as PluginExecutionError)?;
        let requested = request
            .metadata
            .get("requested_model")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(&request.model);
        let path = request
            .metadata
            .get("request_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        let body = apply_payload_config_with_request(
            &self.config,
            &base_model,
            "meta",
            from.as_str(),
            "",
            &body,
            Some(&declarations),
            requested,
            path,
            &request.headers,
        );
        let mut body = set_string_if_different(body, "model", &base_model);
        body = set_bool_if_different(body, "stream", stream);
        for key in [
            "generate",
            "prompt_cache_retention",
            "safety_identifier",
            "stream_options",
            "client_metadata",
        ] {
            body = delete_raw_path(&body, key);
        }
        let patch = ApplyPatchResponsesState::new(&from, &original, &declarations);
        body = normalize_apply_patch_responses_request(&body, Some(&original))
            .map_err(|_| meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE))?;
        let instructions = std::str::from_utf8(&body)
            .ok()
            .map(|v| gjson::get(v, "instructions"));
        if instructions
            .as_ref()
            .is_none_or(|v| !v.exists() || v.kind() == gjson::Kind::Null)
        {
            body = set_json_string(&body, "instructions", "");
        }
        body = super::sanitize_openai_responses_reasoning_encrypted_content("meta executor", &body)
            .into_owned();
        body = sanitize_meta_web_search_tools(&body);
        body = normalize_codex_tool_integer_types_for_executor(&body, &request.headers, "meta");
        Ok(MetaPreparedRequest {
            original,
            body,
            base_model,
            from,
            target,
            patch,
        })
    }
}

pub(crate) fn meta_error(status: u16, message: &str) -> PluginExecutionError {
    Arc::new(AuthError {
        code: String::new(),
        message: message.into(),
        http_status: status,
        retryable: status == 408 || status == 429 || status >= 500,
    })
}

/// Credentials are copied only from the selected manager snapshot. A DCA key
/// must be minted and accepted by its registered preparer before inference.
pub(crate) fn meta_http_request(
    request: &ExecutorRequest,
    body: Vec<u8>,
    stream: bool,
) -> Result<HttpRequest, PluginExecutionError> {
    let mut auth = Auth::default();
    auth.id = request.auth_id.clone();
    auth.provider = request.auth_provider.clone();
    auth.attributes = request.auth_attributes.clone();
    auth.metadata = request.auth_metadata.clone();
    let credential = meta_credentials(&auth);
    if credential.api_key().is_empty() {
        return Err(meta_error(401, "meta executor: missing inference API key"));
    }
    let url = format!("{}/responses", credential.base_url().trim_end_matches('/'));
    let mut outgoing = HttpRequest {
        method: "POST".into(),
        url,
        body,
        ..HttpRequest::default()
    };
    apply_meta_headers(
        &mut outgoing,
        &request.auth_attributes,
        credential.api_key(),
        stream,
    );
    Ok(outgoing)
}
pub(crate) fn apply_meta_headers(
    request: &mut HttpRequest,
    attributes: &std::collections::BTreeMap<String, String>,
    token: &str,
    stream: bool,
) {
    fn set(request: &mut HttpRequest, key: &str, value: String) {
        request.headers.retain(|k, _| !k.eq_ignore_ascii_case(key));
        request.headers.insert(key.into(), vec![value]);
    }
    request
        .headers
        .retain(|k, _| !k.eq_ignore_ascii_case("Authorization"));
    if !token.trim().is_empty() {
        set(request, "Authorization", format!("Bearer {token}"));
    }
    set(request, "Content-Type", "application/json".into());
    set(request, "User-Agent", META_USER_AGENT.into());
    set(request, "X-Client-Id", "tbh:tui".into());
    set(
        request,
        "Accept",
        if stream {
            "text/event-stream"
        } else {
            "application/json"
        }
        .into(),
    );
    if stream {
        set(request, "Cache-Control", "no-cache".into());
    }
    let mut custom = HeaderRequest {
        headers: std::mem::take(&mut request.headers),
        ..HeaderRequest::default()
    };
    apply_custom_headers_from_attrs(&mut custom, attributes);
    request.headers = custom.headers;
}
