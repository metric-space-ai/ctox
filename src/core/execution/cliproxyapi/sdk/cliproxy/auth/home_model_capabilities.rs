// ref: sdk/cliproxy/auth/conductor_home.go:358-388 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// ref: sdk/cliproxy/auth/api_key_model_capabilities.go:126-190,284-330 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// Port-Note: private typed capability bound to the instance-owned dispatch selection
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde::Deserialize;

use crate::internal::modelconfig::{
    HomeModelOptions, ModelInfo, NativeCapabilities, ThinkingSupport,
};
use crate::internal::thinking::parse_suffix;
use crate::sdk::pluginapi::ExecutorRequest;

use super::{model_alias_lookup_candidates, Auth, AuthManager, HomeDispatchSelection};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct HomeDispatchModelInfo {
    id: String,
    #[serde(rename = "type")]
    provider_type: String,
    #[serde(rename = "inputTokenLimit")]
    input_token_limit: usize,
    #[serde(rename = "outputTokenLimit")]
    output_token_limit: usize,
    context_length: usize,
    max_completion_tokens: usize,
    thinking: Option<ThinkingSupport>,
    native_capabilities: Option<NativeCapabilities>,
    support_configuration_update: Option<bool>,
    user_defined: bool,
}

impl HomeDispatchModelInfo {
    fn model_info(&self) -> Option<ModelInfo> {
        (!self.id.trim().is_empty()).then(|| ModelInfo {
            id: self.id.trim().to_owned(),
            provider_type: self.provider_type.trim().to_owned(),
            user_defined: self.user_defined,
            is_compat: false,
            input_token_limit: self.input_token_limit,
            output_token_limit: self.output_token_limit,
            context_length: self.context_length,
            max_completion_tokens: self.max_completion_tokens,
            thinking: self.thinking.clone(),
            native_capabilities: self.native_capabilities.clone(),
            support_configuration_update: self.support_configuration_update.unwrap_or(false),
        })
    }
}

pub(super) fn prepare_home_executor_request(
    manager: &AuthManager,
    request: &ExecutorRequest,
    auth: &Auth,
    selection: &HomeDispatchSelection,
    route_model: &str,
) -> ExecutorRequest {
    let mut execution = super::conductor_home_execution::prepare_executor_request(
        request,
        auth,
        selection.provider(),
    );
    let selected = manager.attach_resolved_api_key_model_info(
        crate::sdk::cliproxy::executor::Request::default(),
        auth,
        route_model,
        &execution.model,
    );
    execution.resolved_model_info =
        super::api_key_model_capabilities::resolved_model_info(&selected);
    attach_home_model_info(execution, auth, route_model, selection.model_info.as_ref())
}

fn attach_home_model_info(
    mut request: ExecutorRequest,
    auth: &Auth,
    route_model: &str,
    wire: Option<&HomeDispatchModelInfo>,
) -> ExecutorRequest {
    let upstream = auth
        .attributes
        .get("home_upstream_model")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| wire.map(|info| info.id.as_str()))
        .unwrap_or(&request.model);
    let options = home_model_options(auth, upstream, route_model);
    // Home owns explicit options and their defaults. Never inherit a local
    // account's compatibility flag when central options are absent or invalid.
    request.resolved_home_model_options = Some(options.unwrap_or_default());
    let Some(wire) = wire else {
        return request;
    };
    let Some(mut info) = wire.model_info() else {
        return request;
    };
    let same_model = request.resolved_model_info.as_ref().filter(|local| {
        parse_suffix(local.id.trim())
            .model_name
            .trim()
            .eq_ignore_ascii_case(parse_suffix(info.id.trim()).model_name.trim())
    });
    info.support_configuration_update = wire
        .support_configuration_update
        .unwrap_or_else(|| same_model.is_some_and(|local| local.support_configuration_update));
    info.is_compat = request
        .resolved_home_model_options
        .as_ref()
        .is_some_and(|options| options.is_compat);
    request.resolved_model_info = Some(Arc::new(info));
    request
}

fn home_model_options(auth: &Auth, model: &str, route_model: &str) -> Option<HomeModelOptions> {
    let options = auth.metadata.get("credential_options")?;
    let raw_models = options.as_object()?.get("models")?;
    let models: Vec<HomeModelOptions> = if raw_models.is_null() {
        Vec::new()
    } else {
        serde_json::from_value(raw_models.clone()).ok()?
    };
    let requested = model.trim();
    if requested.is_empty() {
        return Some(HomeModelOptions::default());
    }
    let parsed = parse_suffix(requested);
    let base = parsed.model_name.trim();
    let prefix = auth.prefix.trim().trim_matches('/');
    let route_model = route_model.trim();
    let route = if prefix.is_empty() {
        route_model
    } else {
        route_model
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(route_model)
            .trim()
    };
    let (_, routes) = model_alias_lookup_candidates(route);
    for route in &routes {
        for candidate in [requested, base] {
            for configured in &models {
                let name = if configured.name.trim().is_empty() {
                    configured.alias.trim()
                } else {
                    configured.name.trim()
                };
                if name.eq_ignore_ascii_case(candidate)
                    && (configured.alias.trim().eq_ignore_ascii_case(route)
                        || name.eq_ignore_ascii_case(route))
                {
                    return Some(configured.clone());
                }
            }
        }
    }
    for use_alias in [false, true] {
        for candidate in [requested, base] {
            if candidate.is_empty() {
                continue;
            }
            for configured in &models {
                let name = if use_alias || configured.name.trim().is_empty() {
                    configured.alias.trim()
                } else {
                    configured.name.trim()
                };
                if name.eq_ignore_ascii_case(candidate) {
                    return Some(configured.clone());
                }
            }
        }
    }
    Some(HomeModelOptions::default())
}

#[cfg(test)]
#[path = "home_model_capabilities_test.rs"]
mod tests;
