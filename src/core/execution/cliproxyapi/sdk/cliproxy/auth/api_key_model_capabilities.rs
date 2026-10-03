// ref: sdk/cliproxy/auth/api_key_model_capabilities.go @ a88197f845c979132c8978ea223c6af05cc81536
// ref: sdk/cliproxy/auth/conductor_models.go:15-35,258-380,520-681 @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::internal::config::{
    CodexKey, CodexModel, OpenAiCompatibility, ProviderCompatConfig, VertexCompatKey,
};
use crate::internal::modelconfig::{self, ModelInfo, ThinkingSupport};
use crate::internal::registry::RegistryThinkingSupport;
use crate::internal::thinking::parse_suffix;
use crate::sdk::cliproxy::executor::Request;

use super::{
    model_alias_lookup_candidates, preserve_resolved_model_suffix, Auth, AuthKind, AuthManager,
    AuthSourceKind, OAuthModelAliasResult,
};

#[derive(Clone)]
struct ConfiguredRoute {
    upstream_model: String,
    force_mapping: bool,
    original_alias: String,
    model_info: Arc<ModelInfo>,
}

/// One immutable, manager-published routing generation. It contains only
/// derived model data; API keys, headers and storage payloads never enter it.
#[derive(Clone, Default)]
pub struct ApiKeyModelRoutingSnapshot {
    routes: BTreeMap<String, BTreeMap<String, Vec<ConfiguredRoute>>>,
}

impl fmt::Debug for ApiKeyModelRoutingSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiKeyModelRoutingSnapshot")
            .field("credential_count", &self.routes.len())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub fn resolved_api_key_model_info(request: &Request) -> Option<Arc<ModelInfo>> {
    request.metadata.resolved_api_key_model_info.clone()
}

impl AuthManager {
    /// Atomically publishes a routing generation derived from a cloned typed
    /// config and the manager-owned credential generation.
    pub fn set_provider_config(&self, config: &ProviderCompatConfig) {
        let _guard = self.lock_mutation();
        self.publish_api_key_model_routing(config);
    }

    pub(super) fn rebuild_api_key_model_routing(&self) {
        let config = self
            .api_key_config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        self.publish_api_key_model_routing(config.as_ref());
    }

    fn publish_api_key_model_routing(&self, config: &ProviderCompatConfig) {
        let owned_config = Arc::new(config.clone());
        let snapshot = compile_snapshot(owned_config.as_ref(), &self.lifecycle.snapshot_cached());
        *self
            .api_key_config
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = owned_config;
        *self
            .api_key_model_routing
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(snapshot);
    }

    #[must_use]
    pub fn api_key_model_routing_snapshot(&self) -> Arc<ApiKeyModelRoutingSnapshot> {
        self.api_key_model_routing
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    #[must_use]
    pub fn lookup_api_key_upstream_model(&self, auth_id: &str, requested_model: &str) -> String {
        lookup_upstream(
            &self.api_key_model_routing_snapshot(),
            auth_id,
            requested_model,
        )
    }

    #[must_use]
    pub fn resolve_api_key_model_alias_with_result(
        &self,
        auth: &Auth,
        requested_model: &str,
    ) -> OAuthModelAliasResult {
        resolve_alias(
            &self.api_key_model_routing_snapshot(),
            auth,
            requested_model,
        )
    }

    #[must_use]
    pub fn execution_model_candidates_with_alias(
        &self,
        auth: &Auth,
        route_model: &str,
    ) -> (
        Vec<String>,
        OAuthModelAliasResult,
        Arc<ApiKeyModelRoutingSnapshot>,
    ) {
        let snapshot = self.api_key_model_routing_snapshot();
        let requested_model = rewrite_model_for_auth(route_model, auth);
        let alias = resolve_alias(&snapshot, auth, &requested_model);
        let routes = matching_routes(&snapshot, &auth.id, &requested_model);
        let mut models = Vec::new();
        for route in routes {
            let model = preserve_resolved_model_suffix(
                &route.upstream_model,
                &parse_suffix(&requested_model),
            );
            if !models
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&model))
            {
                models.push(model);
            }
        }
        if models.is_empty() {
            models.push(if alias.upstream_model.trim().is_empty() {
                requested_model
            } else {
                alias.upstream_model.clone()
            });
        }
        (models, alias, snapshot)
    }

    #[must_use]
    pub fn attach_resolved_api_key_model_info(
        &self,
        request: Request,
        auth: &Auth,
        route_model: &str,
        upstream_model: &str,
    ) -> Request {
        attach_resolved_api_key_model_info(
            &self.api_key_model_routing_snapshot(),
            request,
            auth,
            route_model,
            upstream_model,
        )
    }
}

#[must_use]
pub(crate) fn attach_resolved_api_key_model_info(
    snapshot: &ApiKeyModelRoutingSnapshot,
    mut request: Request,
    auth: &Auth,
    route_model: &str,
    upstream_model: &str,
) -> Request {
    let requested = rewrite_model_for_auth(route_model, auth);
    let selected = upstream_model.trim();
    let routes = matching_routes(snapshot, &auth.id, &requested);
    let exact = routes
        .iter()
        .find(|route| route.upstream_model.trim().eq_ignore_ascii_case(selected));
    let fallback = exact.or_else(|| {
        routes.iter().find(|route| {
            let configured = parse_suffix(route.upstream_model.trim());
            !configured.has_suffix
                && configured
                    .model_name
                    .trim()
                    .eq_ignore_ascii_case(parse_suffix(selected).model_name.trim())
        })
    });
    if let Some(route) = fallback {
        request.metadata.resolved_api_key_model_info = Some(route.model_info.clone());
    }
    request
}

fn compile_snapshot(config: &ProviderCompatConfig, auths: &[Auth]) -> ApiKeyModelRoutingSnapshot {
    let mut snapshot = ApiKeyModelRoutingSnapshot::default();
    for auth in auths {
        if !is_configured_model_routing_auth(auth) {
            continue;
        }
        if let Some(models) = configured_models(config, auth) {
            let mut by_route = BTreeMap::new();
            for model in models {
                add_route(&mut by_route, model);
            }
            if !by_route.is_empty() {
                snapshot.routes.insert(auth.id.trim().to_owned(), by_route);
            }
        }
    }
    snapshot
}

#[cfg(test)]
#[path = "api_key_model_compat_test.rs"]
mod candidate_resolved_model_capability_tests;

#[derive(Clone)]
struct ModelView {
    name: String,
    alias: String,
    model_type: &'static str,
    force_mapping: bool,
    is_compat: bool,
    thinking: Option<ThinkingSupport>,
}

fn configured_models(config: &ProviderCompatConfig, auth: &Auth) -> Option<Vec<ModelView>> {
    let provider = auth.provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "gemini" => resolve_key(&config.gemini_api_key, auth).map(|key| model_views(key, "gemini")),
        "gemini-interactions" => resolve_key(&config.interactions_api_key, auth)
            .map(|key| model_views(key, "interactions")),
        "claude" => resolve_key(&config.claude_api_key, auth).map(|key| model_views(key, "claude")),
        "codex" => resolve_key(&config.codex_api_key, auth).map(|key| model_views(key, "codex")),
        "xai" => resolve_key(&config.xai_api_key, auth).map(|key| model_views(key, "xai")),
        "vertex" => resolve_vertex_key(&config.vertex_api_key, auth).map(vertex_model_views),
        _ => resolve_openai_compat(&config.openai_compatibility, auth).map(openai_model_views),
    }
}

fn resolve_key<'a>(entries: &'a [CodexKey], auth: &Auth) -> Option<&'a CodexKey> {
    if let Some(index) = config_index(auth) {
        return entries.get(index);
    }
    let api_key = auth
        .attributes
        .get("api_key")
        .map_or("", String::as_str)
        .trim();
    let base_url = auth
        .attributes
        .get("base_url")
        .map_or("", String::as_str)
        .trim();
    entries.iter().find(|entry| {
        !api_key.is_empty()
            && entry.api_key.trim() == api_key
            && (base_url.is_empty() || entry.base_url.trim() == base_url)
    })
}

fn resolve_vertex_key<'a>(
    entries: &'a [VertexCompatKey],
    auth: &Auth,
) -> Option<&'a VertexCompatKey> {
    if let Some(index) = config_index(auth) {
        return entries.get(index);
    }
    let api_key = auth
        .attributes
        .get("api_key")
        .map_or("", String::as_str)
        .trim();
    entries
        .iter()
        .find(|entry| !api_key.is_empty() && entry.api_key.trim() == api_key)
}

fn resolve_openai_compat<'a>(
    entries: &'a [OpenAiCompatibility],
    auth: &Auth,
) -> Option<&'a OpenAiCompatibility> {
    if let Some(index) = config_index(auth) {
        return entries.get(index);
    }
    let compat_name = auth
        .attributes
        .get("compat_name")
        .map_or_else(
            || auth.provider.rsplit(':').next().unwrap_or(""),
            String::as_str,
        )
        .trim();
    entries.iter().find(|entry| {
        !compat_name.is_empty() && entry.name.trim().eq_ignore_ascii_case(compat_name)
    })
}

fn config_index(auth: &Auth) -> Option<usize> {
    auth.attributes
        .get("config_index")
        .and_then(|value| value.trim().parse().ok())
}

fn model_views(key: &CodexKey, model_type: &'static str) -> Vec<ModelView> {
    key.models
        .iter()
        .map(|model| codex_model_view(model, model_type))
        .collect()
}

fn codex_model_view(model: &CodexModel, model_type: &'static str) -> ModelView {
    ModelView {
        name: model.name.clone(),
        alias: model.alias.clone(),
        model_type,
        force_mapping: model.force_mapping,
        is_compat: model.is_compat,
        thinking: model.thinking.as_ref().map(thinking_support),
    }
}

fn vertex_model_views(key: &VertexCompatKey) -> Vec<ModelView> {
    key.models
        .iter()
        .map(|model| ModelView {
            name: model.name.clone(),
            alias: model.alias.clone(),
            model_type: "gemini",
            force_mapping: model.force_mapping,
            is_compat: false,
            thinking: model.thinking.as_ref().map(thinking_support),
        })
        .collect()
}

fn openai_model_views(entry: &OpenAiCompatibility) -> Vec<ModelView> {
    entry
        .models
        .iter()
        .map(|model| {
            let thinking = model.thinking.as_ref().map(thinking_support).or_else(|| {
                (!model.image).then(|| ThinkingSupport {
                    levels: vec!["low".into(), "medium".into(), "high".into()],
                    ..ThinkingSupport::default()
                })
            });
            ModelView {
                name: model.name.clone(),
                alias: model.alias.clone(),
                model_type: "openai-compatibility",
                force_mapping: model.force_mapping,
                is_compat: model.is_compat,
                thinking,
            }
        })
        .collect()
}

fn thinking_support(raw: &RegistryThinkingSupport) -> ThinkingSupport {
    ThinkingSupport {
        min: raw.min,
        max: raw.max,
        zero_allowed: raw.zero_allowed,
        dynamic_allowed: raw.dynamic_allowed,
        levels: raw.levels.clone(),
    }
}

fn add_route(by_route: &mut BTreeMap<String, Vec<ConfiguredRoute>>, model: ModelView) {
    let mut name = model.name.trim().to_owned();
    let mut alias = model.alias.trim().to_owned();
    if name.is_empty() {
        name.clone_from(&alias);
    }
    if alias.is_empty() {
        alias.clone_from(&name);
    }
    if name.is_empty() {
        return;
    }
    let support = model.thinking.as_ref();
    let mut model_info = modelconfig::resolve_model_info(&name, model.model_type, support);
    model_info.is_compat = model.is_compat;
    let route = ConfiguredRoute {
        upstream_model: name.clone(),
        force_mapping: model.force_mapping,
        original_alias: alias.clone(),
        model_info: Arc::new(model_info),
    };
    let mut seen = Vec::new();
    for route_model in [&alias, &name] {
        let (_, candidates) = model_alias_lookup_candidates(route_model);
        for candidate in candidates {
            let key = candidate.trim().to_ascii_lowercase();
            if key.is_empty() || seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());
            let routes = by_route.entry(key).or_default();
            if !routes.iter().any(|existing| {
                existing
                    .upstream_model
                    .eq_ignore_ascii_case(&route.upstream_model)
            }) {
                routes.push(route.clone());
            }
        }
    }
}

fn lookup_upstream(
    snapshot: &ApiKeyModelRoutingSnapshot,
    auth_id: &str,
    requested: &str,
) -> String {
    matching_routes(snapshot, auth_id, requested)
        .first()
        .map(|route| {
            preserve_resolved_model_suffix(&route.upstream_model, &parse_suffix(requested.trim()))
        })
        .unwrap_or_default()
}

fn resolve_alias(
    snapshot: &ApiKeyModelRoutingSnapshot,
    auth: &Auth,
    requested: &str,
) -> OAuthModelAliasResult {
    let requested = requested.trim();
    if requested.is_empty() {
        return OAuthModelAliasResult::default();
    }
    let routes = matching_routes(snapshot, &auth.id, requested);
    let Some(route) = routes.first() else {
        return OAuthModelAliasResult {
            upstream_model: requested.to_owned(),
            ..OAuthModelAliasResult::default()
        };
    };
    let requested_base = parse_suffix(requested).model_name;
    let upstream_base = parse_suffix(&route.upstream_model).model_name;
    if requested_base
        .trim()
        .eq_ignore_ascii_case(upstream_base.trim())
    {
        return OAuthModelAliasResult {
            upstream_model: requested.to_owned(),
            ..OAuthModelAliasResult::default()
        };
    }
    OAuthModelAliasResult {
        upstream_model: preserve_resolved_model_suffix(
            &route.upstream_model,
            &parse_suffix(requested),
        ),
        force_mapping: route.force_mapping,
        original_alias: if route.force_mapping {
            route.original_alias.clone()
        } else {
            String::new()
        },
    }
}

fn matching_routes<'a>(
    snapshot: &'a ApiKeyModelRoutingSnapshot,
    auth_id: &str,
    requested: &str,
) -> Vec<&'a ConfiguredRoute> {
    let Some(by_route) = snapshot.routes.get(auth_id.trim()) else {
        return Vec::new();
    };
    let (_, candidates) = model_alias_lookup_candidates(requested.trim());
    let mut routes = Vec::new();
    for candidate in candidates {
        if let Some(found) = by_route.get(&candidate.trim().to_ascii_lowercase()) {
            for route in found {
                if !routes.iter().any(|existing: &&ConfiguredRoute| {
                    existing
                        .upstream_model
                        .eq_ignore_ascii_case(&route.upstream_model)
                }) {
                    routes.push(route);
                }
            }
        }
    }
    routes
}

fn rewrite_model_for_auth(route_model: &str, auth: &Auth) -> String {
    let model = route_model.trim();
    let prefix = auth.prefix.trim().trim_matches('/');
    if prefix.is_empty() {
        return model.to_owned();
    }
    model
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(model)
        .trim()
        .to_owned()
}

fn is_configured_model_routing_auth(auth: &Auth) -> bool {
    auth.auth_kind() == Some(AuthKind::ApiKey)
        || (auth.auth_source_kind() == Some(AuthSourceKind::Config)
            && auth
                .attributes
                .get("compat_name")
                .is_some_and(|value| !value.trim().is_empty()))
}
