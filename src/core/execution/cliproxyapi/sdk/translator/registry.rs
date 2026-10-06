// ref: sdk/translator/registry.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    Format, PluginHooks, RequestEnvelope, RequestEnvelopeTransform, RequestTransform,
    ResponseTransform, TranslationContext, TranslationState,
};
use crate::internal::thinking::{
    apply_summary_config_for_model, extract_translated_summary_config,
};
use crate::internal::translator::common::set_json_string;
use crate::internal::util::valid_json_bytes;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default)]
struct RegistryState {
    requests: HashMap<(Format, Format), RequestEnvelopeTransform>,
    responses: HashMap<(Format, Format), ResponseTransform>,
    hooks: Option<Arc<dyn PluginHooks>>,
}

#[derive(Default)]
pub struct Registry {
    state: RwLock<RegistryState>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        from: Format,
        to: Format,
        request: Option<RequestTransform>,
        response: ResponseTransform,
    ) {
        self.register_request_envelope(from, to, request.map(wrap_request_transform), response);
    }

    /// ref: sdk/translator/registry.go:118-195 @ e2bff010
    /// Registers a transform that can preserve request-local update intent.
    pub fn register_request_envelope(
        &self,
        from: Format,
        to: Format,
        request: Option<RequestEnvelopeTransform>,
        response: ResponseTransform,
    ) {
        let mut state = self.state.write().expect("translator registry poisoned");
        if let Some(request) = request {
            state.requests.insert((from.clone(), to.clone()), request);
        }
        state.responses.insert((from, to), response);
    }

    /// Registers one client→provider protocol pair.
    ///
    /// As in upstream, request and response transforms share the same stored
    /// `(client, provider)` key. Response translation receives
    /// `(provider, client)` and performs the reverse lookup at dispatch time.
    pub fn register_pair(
        &self,
        client: Format,
        provider: Format,
        request: RequestTransform,
        response: ResponseTransform,
    ) {
        let mut state = self.state.write().expect("translator registry poisoned");
        state.requests.insert(
            (client.clone(), provider.clone()),
            wrap_request_transform(request),
        );
        state.responses.insert((client, provider), response);
    }

    pub fn set_plugin_hooks(&self, hooks: Option<Arc<dyn PluginHooks>>) {
        self.state
            .write()
            .expect("translator registry poisoned")
            .hooks = hooks;
    }

    pub fn has_request_transformer(&self, from: &Format, to: &Format) -> bool {
        self.state
            .read()
            .expect("translator registry poisoned")
            .requests
            .contains_key(&(from.clone(), to.clone()))
    }

    pub fn has_response_transformer(&self, from: &Format, to: &Format) -> bool {
        self.response_transform(from, to)
            .is_some_and(|transform| transform.has_any())
    }

    pub fn has_stream_response_transformer(&self, from: &Format, to: &Format) -> bool {
        self.response_transform(from, to)
            .is_some_and(|transform| transform.stream.is_some())
    }

    pub fn has_non_stream_response_transformer(&self, from: &Format, to: &Format) -> bool {
        self.response_transform(from, to)
            .is_some_and(|transform| transform.non_stream.is_some())
    }

    fn response_transform(&self, from: &Format, to: &Format) -> Option<ResponseTransform> {
        self.state
            .read()
            .expect("translator registry poisoned")
            .responses
            .get(&(from.clone(), to.clone()))
            .cloned()
    }

    pub fn translate_request(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        model: &str,
        raw_json: &[u8],
        stream: bool,
    ) -> Vec<u8> {
        self.translate_request_envelope(
            context,
            from,
            to,
            RequestEnvelope {
                format: from.clone(),
                model: model.to_owned(),
                stream,
                body: raw_json.to_vec(),
                configuration_updates_changed: false,
            },
        )
        .body
    }

    /// ref: sdk/translator/registry.go:118-195 @ e2bff010
    /// Only plugin normalization and explicit envelope transforms report update
    /// intent. A normal cross-protocol conversion dropping updates is not a
    /// plugin configuration edit.
    pub fn translate_request_envelope(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        mut request: RequestEnvelope,
    ) -> RequestEnvelope {
        let (transform, hooks) = {
            let state = self.state.read().expect("translator registry poisoned");
            (
                state.requests.get(&(from.clone(), to.clone())).cloned(),
                state.hooks.clone(),
            )
        };

        if let Some(transform) = transform {
            let summary =
                extract_translated_summary_config(&request.body, from.as_str(), to.as_str());
            request = transform(context, request);
            request.body = apply_summary_config_for_model(
                &request.body,
                to.as_str(),
                &request.model,
                &summary,
            );
            if let Some(hooks) = hooks {
                let before = configuration_updates(&request.body);
                request.body = hooks.normalize_request(
                    context,
                    from,
                    to,
                    &request.model,
                    request.body,
                    request.stream,
                );
                request.configuration_updates_changed |=
                    before != configuration_updates(&request.body);
            }
            request.format = to.clone();
            return request;
        }

        request.body = normalize_model(&request.body, &request.model);
        if let Some(hooks) = hooks {
            let before = configuration_updates(&request.body);
            request.body = hooks.normalize_request(
                context,
                from,
                to,
                &request.model,
                request.body,
                request.stream,
            );
            request.configuration_updates_changed |= before != configuration_updates(&request.body);
            let summary =
                extract_translated_summary_config(&request.body, from.as_str(), to.as_str());
            if let Some(translated) = hooks.translate_request(
                context,
                from,
                to,
                &request.model,
                &request.body,
                request.stream,
            ) {
                request.body = apply_summary_config_for_model(
                    &translated,
                    to.as_str(),
                    &request.model,
                    &summary,
                );
            }
        }
        request.format = to.clone();
        request
    }

    /// Runs only the selected plugin normalizer, as compatibility translators
    /// already own protocol and thinking-summary conversion.
    pub fn normalize_request(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        model: &str,
        body: Vec<u8>,
        stream: bool,
    ) -> Vec<u8> {
        let hooks = self
            .state
            .read()
            .expect("translator registry poisoned")
            .hooks
            .clone();
        let Some(hooks) = hooks else { return body };
        hooks.normalize_request(context, from, to, model, body, stream)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn translate_stream(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        model: &str,
        original_request: &[u8],
        translated_request: &[u8],
        raw_json: &[u8],
        state: &mut TranslationState,
    ) -> Vec<Vec<u8>> {
        if context.is_cancelled() {
            return Vec::new();
        }
        // Response registrations are provider -> client. This preserves the
        // direction used by upstream TranslateStream.
        let (transform, hooks) = {
            let guard = self.state.read().expect("translator registry poisoned");
            (
                guard.responses.get(&(to.clone(), from.clone())).cloned(),
                guard.hooks.clone(),
            )
        };
        let body = hooks.as_ref().map_or_else(
            || raw_json.to_vec(),
            |hooks| {
                hooks.normalize_response_before(
                    context,
                    from,
                    to,
                    model,
                    original_request,
                    translated_request,
                    raw_json.to_vec(),
                    true,
                )
            },
        );

        let used_native = transform.as_ref().is_some_and(|item| item.stream.is_some());
        let mut outputs = if let Some(native) = transform.and_then(|item| item.stream) {
            native(
                context,
                model,
                original_request,
                translated_request,
                &body,
                state,
            )
        } else if let Some(translated) = hooks.as_ref().and_then(|hooks| {
            hooks.translate_response(
                context,
                from,
                to,
                model,
                original_request,
                translated_request,
                &body,
                true,
            )
        }) {
            vec![translated]
        } else if used_native {
            Vec::new()
        } else {
            vec![body]
        };

        if let Some(hooks) = hooks {
            for output in &mut outputs {
                *output = hooks.normalize_response_after(
                    context,
                    from,
                    to,
                    model,
                    original_request,
                    translated_request,
                    std::mem::take(output),
                    true,
                );
            }
        }
        outputs
    }

    #[allow(clippy::too_many_arguments)]
    pub fn translate_non_stream(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        model: &str,
        original_request: &[u8],
        translated_request: &[u8],
        raw_json: &[u8],
        state: &mut TranslationState,
    ) -> Vec<u8> {
        let (transform, hooks) = {
            let guard = self.state.read().expect("translator registry poisoned");
            (
                guard.responses.get(&(to.clone(), from.clone())).cloned(),
                guard.hooks.clone(),
            )
        };
        let mut body = hooks.as_ref().map_or_else(
            || raw_json.to_vec(),
            |hooks| {
                hooks.normalize_response_before(
                    context,
                    from,
                    to,
                    model,
                    original_request,
                    translated_request,
                    raw_json.to_vec(),
                    false,
                )
            },
        );
        if let Some(native) = transform.and_then(|item| item.non_stream) {
            body = native(
                context,
                model,
                original_request,
                translated_request,
                &body,
                state,
            );
        } else if let Some(translated) = hooks.as_ref().and_then(|hooks| {
            hooks.translate_response(
                context,
                from,
                to,
                model,
                original_request,
                translated_request,
                &body,
                false,
            )
        }) {
            body = translated;
        }
        hooks.map_or(body.clone(), |hooks| {
            hooks.normalize_response_after(
                context,
                from,
                to,
                model,
                original_request,
                translated_request,
                body,
                false,
            )
        })
    }

    pub fn translate_token_count(
        &self,
        context: &TranslationContext,
        from: &Format,
        to: &Format,
        count: i64,
        raw_json: &[u8],
    ) -> Vec<u8> {
        self.response_transform(to, from)
            .and_then(|transform| transform.token_count)
            .map_or_else(|| raw_json.to_vec(), |transform| transform(context, count))
    }
}

fn wrap_request_transform(transform: RequestTransform) -> RequestEnvelopeTransform {
    Arc::new(move |_, mut request| {
        request.body = transform(&request.model, &request.body, request.stream);
        request
    })
}

/// ref: sdk/translator/registry.go:177-195 @ e2bff010
/// Preserve exact update item bytes and their order, matching gjson item.Raw;
/// changes to ordinary messages or surrounding array whitespace do not count.
fn configuration_updates(body: &[u8]) -> Vec<String> {
    let input = crate::internal::util::get_gjson_bytes_no_copy(body, "input");
    if input.kind() != gjson::Kind::Array {
        return Vec::new();
    }
    let mut updates = Vec::new();
    input.each(|_, item| {
        if item.get("type").str() == "configuration_update" {
            updates.push(item.json().to_owned());
        }
        true
    });
    updates
}

fn normalize_model(raw_json: &[u8], model: &str) -> Vec<u8> {
    // ref: sdk/translator/registry.go:140-146 — sjson changes only the model.
    // Re-encoding siblings changes raw configuration-update comparisons and
    // can lose duplicate keys, large numbers or deeply nested schemas.
    if model.is_empty() || !valid_json_bytes(raw_json) {
        return raw_json.to_vec();
    }
    let Ok(document) = std::str::from_utf8(raw_json) else {
        return raw_json.to_vec();
    };
    let root = gjson::parse(document);
    if root.kind() != gjson::Kind::Object {
        return raw_json.to_vec();
    }
    let current = root.get("model");
    if current.kind() == gjson::Kind::String && current.str() == model {
        return raw_json.to_vec();
    }
    set_json_string(raw_json, "model", model)
}
