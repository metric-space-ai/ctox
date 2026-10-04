// ref: internal/runtime/executor/devin_executor.go:95-124,399-460,2483-2519,2540-2567
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — injected catalog/session/translation owners
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_executor_history::parse_devin_interactions_payload;
use super::helps::cloak_obfuscate::SensitiveWordMatcher;
use super::helps::devin_models::resolve_devin_chat_model_uid;
use super::helps::devin_request::{
    build_devin_get_chat_message_request, generate_devin_sentry_trace, DevinChatRequest,
    DevinSessionTurns, DEVIN_CHAT_PATH, DEVIN_DEFAULT_BASE_URL,
};
use super::helps::devin_wire::{wrap_connect_envelope, ConnectFrameError};
use crate::internal::registry::{DevinModelsStore, StaticModelsCatalog};
use crate::internal::thinking::parse_suffix;
use crate::internal::util::{apply_custom_headers_from_attrs, HeaderRequest};
use crate::sdk::pluginapi::{ExecutorRequest, HttpRequest};
use crate::sdk::translator::{Format, Registry, TranslationContext};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use url::Url;
use uuid::Uuid;

/// Host-owned runtime inputs. The caller supplies its already-resolved canonical
/// session identity; this helper does not guess one from an ambient environment.
pub struct DevinRequestOwner<'a> {
    pub registry: &'a Registry,
    pub models: &'a DevinModelsStore,
    pub catalog: &'a StaticModelsCatalog,
    pub turns: &'a DevinSessionTurns,
    pub canonical_session_id: &'a str,
    pub matcher: Option<&'a SensitiveWordMatcher>,
}

/// Borrowed from the selected credential only. No Debug/serialization path can
/// disclose tokens, device identity or a configured private server URL.
pub struct DevinCredentials<'a> {
    pub session_token: &'a str,
    pub base_url: &'a str,
    pub device_seed: &'a str,
}
impl fmt::Debug for DevinCredentials<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinCredentials")
            .field("has_token", &!self.session_token.is_empty())
            .field("has_device_seed", &!self.device_seed.is_empty())
            .finish_non_exhaustive()
    }
}

pub struct DevinPreparedRequest {
    pub http_request: HttpRequest,
    pub translated_payload: Vec<u8>,
    pub chat_model_uid: String,
    pub session_id: String,
    pub cascade_id: String,
    pub max_tokens: i64,
}
impl fmt::Debug for DevinPreparedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinPreparedRequest")
            .field("body_bytes", &self.http_request.body.len())
            .field("max_tokens", &self.max_tokens)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum DevinRequestError {
    MissingCredentials,
    InvalidUrl(url::ParseError),
    Wire(ConnectFrameError),
}
impl fmt::Display for DevinRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCredentials => {
                f.write_str("devin credentials missing: api_key or session_token required")
            }
            Self::InvalidUrl(error) => write!(f, "devin request URL is invalid: {error}"),
            Self::Wire(error) => write!(f, "devin request framing failed: {error}"),
        }
    }
}
impl std::error::Error for DevinRequestError {}

fn attribute<'a>(values: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    values.get(key).map_or("", String::as_str).trim()
}
fn metadata<'a>(values: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
    values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
}

pub fn devin_auth_credentials<'a>(
    attributes: &'a BTreeMap<String, String>,
    values: &'a BTreeMap<String, Value>,
) -> DevinCredentials<'a> {
    let mut token = ["api_key", "session_token", "token"]
        .into_iter()
        .map(|key| attribute(attributes, key))
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    let mut base_url = attribute(attributes, "base_url");
    if base_url.is_empty() {
        base_url = DEVIN_DEFAULT_BASE_URL;
    }
    let mut seed = attribute(attributes, "device_seed");
    for key in ["api_key", "session_token"] {
        if token.is_empty() {
            token = metadata(values, key);
        }
    }
    // Upstream intentionally ignores a metadata-only "token" key.
    let configured_base = metadata(values, "base_url");
    if base_url == DEVIN_DEFAULT_BASE_URL && !configured_base.is_empty() {
        base_url = configured_base;
    }
    if seed.is_empty() {
        seed = metadata(values, "device_seed");
    }
    DevinCredentials {
        session_token: token,
        base_url,
        device_seed: seed,
    }
}

fn set_header(request: &mut HttpRequest, name: &str, value: String) {
    request
        .headers
        .retain(|key, _| !key.eq_ignore_ascii_case(name));
    request.headers.insert(name.into(), vec![value]);
}

/// Shared by chat and native unary status/catalog calls. Explicit account
/// headers are applied last, matching upstream's provider-auth contract.
pub fn prepare_devin_http_headers(
    request: &mut HttpRequest,
    attributes: &BTreeMap<String, String>,
    values: &BTreeMap<String, Value>,
) {
    let credential = devin_auth_credentials(attributes, values);
    if !credential.session_token.is_empty() {
        set_header(
            request,
            "Authorization",
            format!("Basic {0}-{0}", credential.session_token),
        );
    }
    set_header(request, "Content-Type", "application/connect+proto".into());
    set_header(request, "Connect-Protocol-Version", "1".into());
    set_header(request, "Accept", "*/*".into());
    let path = Url::parse(&request.url)
        .map(|url| url.path().to_owned())
        .unwrap_or_else(|_| {
            request
                .url
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .into()
        });
    let unary = [
        "GetUserStatus",
        "GetCliModelConfigs",
        "SeatManagementService",
    ]
    .into_iter()
    .any(|name| path.contains(name));
    let has_trace = request.headers.iter().any(|(name, values)| {
        name.eq_ignore_ascii_case("Sentry-Trace")
            && values.first().is_some_and(|value| !value.is_empty())
    });
    if !unary && !has_trace {
        set_header(request, "Sentry-Trace", generate_devin_sentry_trace());
    }
    // Empty value suppresses transport/default user-agent injection.
    set_header(request, "User-Agent", String::new());
    let mut custom = HeaderRequest {
        headers: std::mem::take(&mut request.headers),
        ..HeaderRequest::default()
    };
    apply_custom_headers_from_attrs(&mut custom, attributes);
    request.headers = custom.headers;
}

pub fn normalize_devin_uuid(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return Uuid::new_v4().to_string();
    }
    // google/uuid also accepts a case-insensitive URN prefix. Preserve every
    // already-valid wire UUID's original spelling instead of canonicalizing it.
    let urn = raw
        .get(..9)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("urn:uuid:"));
    let parsed = if urn {
        Uuid::parse_str(&raw[9..])
    } else {
        Uuid::parse_str(raw)
    };
    if parsed.is_ok() {
        raw.into()
    } else {
        Uuid::new_v5(&Uuid::NAMESPACE_OID, raw.as_bytes()).to_string()
    }
}

pub fn resolve_devin_session_and_cascade_ids(
    session: &str,
    cascade: &str,
    canonical_session: &str,
) -> (String, String) {
    let session = if session.is_empty() {
        canonical_session
    } else {
        session
    };
    let session = normalize_devin_uuid(session);
    let cascade = if cascade.is_empty() {
        session.clone()
    } else {
        normalize_devin_uuid(cascade)
    };
    (session, cascade)
}

pub fn prepare_devin_http_request(
    owner: &DevinRequestOwner<'_>,
    request: &ExecutorRequest,
) -> Result<DevinPreparedRequest, DevinRequestError> {
    let credentials = devin_auth_credentials(&request.auth_attributes, &request.auth_metadata);
    if credentials.session_token.is_empty() {
        return Err(DevinRequestError::MissingCredentials);
    }
    let url = format!(
        "{}{}",
        credentials.base_url.trim_end_matches('/'),
        DEVIN_CHAT_PATH
    );
    let payload = if request.source_format.is_empty() || request.source_format == "interactions" {
        request.payload.clone()
    } else {
        owner.registry.translate_request(
            &TranslationContext::default(),
            &Format::from(request.source_format.as_str()),
            &Format::from("interactions"),
            &request.model,
            &request.payload,
            request.stream,
        )
    };
    let mut history = parse_devin_interactions_payload(&payload, &request.original_request);
    let (session_id, cascade_id) = resolve_devin_session_and_cascade_ids(
        &history.session_id,
        &history.cascade_id,
        owner.canonical_session_id,
    );
    let base_model = parse_suffix(&request.model).model_name;
    if let Some(info) = owner.models.lookup(&base_model, owner.catalog) {
        if info.max_completion_tokens > 0 {
            let maximum = i64::try_from(info.max_completion_tokens).unwrap_or(i64::MAX);
            if history.max_tokens > maximum || history.max_tokens <= 0 {
                history.max_tokens = maximum;
            }
        }
    }
    let chat_model_uid = resolve_devin_chat_model_uid(
        &request.model,
        &history.thinking_level,
        history.budget_tokens,
        owner.models,
        owner.catalog,
    );
    let protobuf = build_devin_get_chat_message_request(
        &DevinChatRequest {
            session_token: credentials.session_token,
            device_seed: credentials.device_seed,
            chat_model_uid: &chat_model_uid,
            system_prompt: &history.system_prompt,
            prompts: &history.prompts,
            tools: &history.tools,
            temperature: history.temperature,
            max_tokens: history.max_tokens,
            session_id: &session_id,
            cascade_id: &cascade_id,
            matcher: owner.matcher,
        },
        owner.turns,
    );
    let mut http_request = HttpRequest {
        method: "POST".into(),
        url,
        body: wrap_connect_envelope(&protobuf).map_err(DevinRequestError::Wire)?,
        ..HttpRequest::default()
    };
    Url::parse(&http_request.url).map_err(DevinRequestError::InvalidUrl)?;
    prepare_devin_http_headers(
        &mut http_request,
        &request.auth_attributes,
        &request.auth_metadata,
    );
    Ok(DevinPreparedRequest {
        http_request,
        translated_payload: payload,
        chat_model_uid,
        session_id,
        cascade_id,
        max_tokens: history.max_tokens,
    })
}

#[cfg(test)]
#[path = "devin_executor_request_test.rs"]
mod tests;
