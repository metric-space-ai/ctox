// ref: internal/runtime/executor/helps/usage_helpers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: access-token identity is handed to the typed SDK usage record owner.
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value};

use crate::internal::logging::get_response_headers;
use crate::internal::runtime::executor::claude_executor::{ClaudeUsage, ClaudeUsageSink};
use crate::internal::thinking::extract_translated_reasoning_effort;
use crate::sdk::cliproxy::auth::{access_token_sha256, Auth};
use crate::sdk::cliproxy::usage::{
    ensure_token_breakdown_for_provider, generate_flag, new_independent_token_breakdown,
    new_partial_subset_token_breakdown, new_separate_reasoning_token_breakdown,
    new_subset_token_breakdown, new_unclassified_token_breakdown, Detail, Failure, Manager, Record,
    TokenAccountingQuality, TokenBreakdown, UsageContext, TOKEN_ACCOUNTING_SCHEMA_VERSION,
};

pub const MAX_USAGE_STREAM_CHUNK_BYTES: usize = 1024 * 1024;
pub const DEFAULT_STOP_TRACE_CAPACITY: usize = 4_096;
pub const DEFAULT_STOP_TRACE_TTL: Duration = Duration::from_secs(10 * 60);

/// Instance-owned usage publisher. The manager and request context are
/// explicit authorities; there is no package-global usage dispatcher.
pub struct UsageReporter {
    manager: Arc<Manager>,
    context: UsageContext,
    provider: String,
    executor_type: String,
    model: String,
    alias: String,
    auth_id: String,
    auth_index: String,
    access_token_sha256: Mutex<String>,
    auth_type: String,
    api_key: String,
    source: String,
    reasoning: Mutex<String>,
    service_tier: String,
    generate: bool,
    requested_at: SystemTime,
    requested_instant: Instant,
    timing: Mutex<UsageTiming>,
    published: AtomicBool,
}

#[derive(Debug, Default)]
struct UsageTiming {
    ttft: Duration,
    ttft_start: Option<Instant>,
    ttft_set: bool,
}

impl UsageReporter {
    pub fn new(
        manager: Arc<Manager>,
        context: UsageContext,
        provider: impl Into<String>,
        executor_type: impl Into<String>,
        model: impl Into<String>,
        auth: Option<&Auth>,
        api_key: impl Into<String>,
    ) -> Self {
        let provider = provider.into();
        let executor_type = executor_type.into();
        let model = model.into();
        let api_key = api_key.into();
        let alias = if context.requested_model_alias().trim().is_empty() {
            model.clone()
        } else {
            context.requested_model_alias().trim().to_owned()
        };
        let (auth_id, auth_index, access_token_fingerprint, auth_type) = auth.map_or_else(
            || (String::new(), String::new(), String::new(), String::new()),
            |auth| {
                let mut auth = auth.clone();
                (
                    auth.id.clone(),
                    auth.ensure_index(),
                    access_token_sha256(&auth),
                    auth.auth_kind()
                        .map(|kind| kind.as_str().to_owned())
                        .unwrap_or_default(),
                )
            },
        );
        let source = resolve_usage_source(auth, &api_key);
        let reasoning = context.reasoning_effort().to_owned();
        let service_tier = context.service_tier().to_owned();
        let generate = context.generate();
        Self {
            manager,
            context,
            provider,
            executor_type,
            model,
            alias,
            auth_id,
            auth_index,
            access_token_sha256: Mutex::new(access_token_fingerprint),
            auth_type,
            api_key,
            source,
            reasoning: Mutex::new(reasoning),
            service_tier,
            generate,
            requested_at: SystemTime::now(),
            requested_instant: Instant::now(),
            timing: Mutex::new(UsageTiming::default()),
            published: AtomicBool::new(false),
        }
    }

    pub fn update_access_token_fingerprint(&self, auth: &Auth) {
        *lock_unpoisoned(&self.access_token_sha256) = access_token_sha256(auth);
    }

    /// Accepts only the normalized one-way token identity emitted by an
    /// executor. Rejecting every other shape prevents a credential (or an
    /// arbitrary diagnostic value) from being persisted as usage metadata.
    pub(crate) fn update_access_token_fingerprint_sha256(&self, sha256: &str) -> bool {
        let sha256 = sha256.trim();
        let mut current = lock_unpoisoned(&self.access_token_sha256);
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            current.clear();
            return false;
        }
        *current = sha256.to_owned();
        true
    }

    pub fn access_token_fingerprint(&self) -> String {
        lock_unpoisoned(&self.access_token_sha256).clone()
    }

    pub fn publish(&self, detail: Detail) -> bool {
        self.publish_with_outcome(detail, false, Failure::default())
    }

    pub fn publish_failure(
        &self,
        status_code: Option<i32>,
        error: &(dyn fmt::Display + Sync),
    ) -> bool {
        self.publish_with_outcome(
            Detail::default(),
            true,
            Failure {
                status_code: status_code.unwrap_or_default(),
                body: error.to_string().trim().to_owned(),
            },
        )
    }

    pub fn ensure_published(&self) -> bool {
        self.publish_with_outcome(Detail::default(), false, Failure::default())
    }

    pub fn publish_additional_model(&self, model: &str, detail: Detail) -> bool {
        let Some(record) = self.build_additional_model_record(model, detail) else {
            return false;
        };
        self.manager.publish(self.context.clone(), record)
    }

    pub fn set_translated_reasoning_effort(&self, payload: &[u8], provider: &str) {
        *lock_unpoisoned(&self.reasoning) = extract_translated_reasoning_effort(payload, provider);
    }

    pub fn start_response_ttft(&self) {
        let mut timing = lock_unpoisoned(&self.timing);
        if !timing.ttft_set && timing.ttft_start.is_none() {
            timing.ttft_start = Some(Instant::now());
        }
    }

    /// Marks TTFT only for the first non-empty response fragment.
    pub fn observe_response_chunk(&self, chunk: &[u8]) {
        if !chunk.is_empty() {
            self.mark_first_response_byte();
        }
    }

    pub fn mark_first_response_byte(&self) {
        let mut timing = lock_unpoisoned(&self.timing);
        if timing.ttft_set {
            return;
        }
        let Some(start) = timing.ttft_start.take() else {
            return;
        };
        timing.ttft = start.elapsed();
        timing.ttft_set = true;
    }

    pub fn build_record(&self, detail: Detail, failed: bool, fail: Failure) -> Record {
        self.build_record_for_model(&self.model, detail, failed, fail)
    }

    fn publish_with_outcome(&self, detail: Detail, failed: bool, fail: Failure) -> bool {
        if self
            .published
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let detail = normalize_usage_detail_total(detail, &self.provider, &self.executor_type);
        let record = self.build_record(detail, failed, fail);
        self.manager.publish(self.context.clone(), record)
    }

    fn build_additional_model_record(&self, model: &str, detail: Detail) -> Option<Record> {
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        let detail = normalize_usage_detail_total(detail, &self.provider, &self.executor_type);
        has_nonzero_token_usage(&detail)
            .then(|| self.build_record_for_model(model, detail, false, Failure::default()))
    }

    fn build_record_for_model(
        &self,
        model: &str,
        detail: Detail,
        failed: bool,
        fail: Failure,
    ) -> Record {
        Record {
            provider: self.provider.clone(),
            executor_type: self.executor_type.clone(),
            model: model.to_owned(),
            alias: self.alias.clone(),
            source: self.source.clone(),
            api_key: self.api_key.clone(),
            auth_id: self.auth_id.clone(),
            auth_index: self.auth_index.clone(),
            access_token_sha256: self.access_token_fingerprint(),
            auth_type: self.auth_type.clone(),
            reasoning_effort: lock_unpoisoned(&self.reasoning).clone(),
            service_tier: self.service_tier.clone(),
            request_service_tier: self.service_tier.clone(),
            response_service_tier: detail.response_service_tier.trim().to_owned(),
            generate: generate_flag(self.generate),
            requested_at: Some(self.requested_at),
            latency: self.requested_instant.elapsed(),
            ttft: lock_unpoisoned(&self.timing).ttft,
            failed,
            fail,
            detail,
            response_headers: get_response_headers(Some(&self.context.request)).unwrap_or_default(),
        }
    }
}

impl fmt::Debug for UsageReporter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UsageReporter")
            .field("provider", &self.provider)
            .field("executor_type", &self.executor_type)
            .field("model", &self.model)
            .field("alias", &self.alias)
            .field("auth_id", &self.auth_id)
            .field("auth_index", &self.auth_index)
            .field("api_key", &"[REDACTED]")
            .field("source", &"[REDACTED]")
            .field("published", &self.published.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl ClaudeUsageSink for UsageReporter {
    fn publish(&self, _model: Option<&str>, usage: ClaudeUsage) {
        let detail = Detail {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_tokens: usage.cached_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            total_tokens: usage.total_tokens,
            token_breakdown: new_independent_token_breakdown(
                usage.input_tokens,
                usage.cache_read_tokens,
                usage.cache_creation_tokens,
                usage.output_tokens,
                0,
                usage.total_tokens,
            ),
            ..Detail::default()
        };
        let _ = UsageReporter::publish(self, detail);
    }
}

pub fn normalize_usage_detail_total(detail: Detail, provider: &str, executor_type: &str) -> Detail {
    ensure_token_breakdown_for_provider(detail, provider, executor_type)
}

pub fn has_nonzero_token_usage(detail: &Detail) -> bool {
    detail.input_tokens != 0
        || detail.output_tokens != 0
        || detail.reasoning_tokens != 0
        || detail.cached_tokens != 0
        || detail.cache_read_tokens != 0
        || detail.cache_creation_tokens != 0
        || detail.total_tokens != 0
        || detail.token_breakdown.total_tokens != 0
}

fn resolve_usage_source(auth: Option<&Auth>, api_key: &str) -> String {
    if let Some(auth) = auth {
        if auth.provider.trim().eq_ignore_ascii_case("vertex") {
            for key in ["project_id", "project"] {
                if let Some(value) = auth.metadata.get(key).and_then(Value::as_str) {
                    if !value.trim().is_empty() {
                        return value.trim().to_owned();
                    }
                }
            }
        }
        let (_, account) = auth.account_info();
        if !account.trim().is_empty() {
            return account.trim().to_owned();
        }
        if let Some(email) = auth.metadata.get("email").and_then(Value::as_str) {
            if !email.trim().is_empty() {
                return email.trim().to_owned();
            }
        }
        if let Some(key) = auth.attributes.get("api_key") {
            if !key.trim().is_empty() {
                return key.trim().to_owned();
            }
        }
    }
    api_key.trim().to_owned()
}

/// Constant-space holder for the latest stream usage observation.
#[derive(Clone, Debug, Default)]
pub struct StreamUsageBuffer {
    detail: Detail,
    observed: bool,
}

impl StreamUsageBuffer {
    pub fn observe(&mut self, detail: Detail, ok: bool) {
        if !ok {
            return;
        }
        let tier = detail.response_service_tier.trim().to_owned();
        if tier.is_empty() || has_nonzero_token_usage(&detail) {
            let preserved_tier = self.detail.response_service_tier.clone();
            self.detail = detail;
            if self.detail.response_service_tier.is_empty() {
                self.detail.response_service_tier = preserved_tier;
            }
        } else {
            self.detail.response_service_tier = tier;
        }
        self.observed = true;
    }

    pub fn observe_openai_stream(&mut self, line: &[u8]) {
        if line.len() > MAX_USAGE_STREAM_CHUNK_BYTES {
            return;
        }
        let Some(payload) = json_payload(line) else {
            return;
        };
        let has_usage = find_bytes(payload, b"\"usage\"");
        let need_tier = self.detail.response_service_tier.is_empty() || has_usage;
        let has_tier = need_tier && find_bytes(payload, b"\"service_tier\"");
        if !has_usage && !has_tier {
            return;
        }
        let Ok(root) = serde_json::from_slice::<Value>(payload) else {
            return;
        };
        let mut detail = Detail::default();
        let usage_ok = root
            .get("usage")
            .filter(|usage| has_openai_style_usage_token_fields(usage))
            .map(|usage| {
                detail = parse_openai_style_usage_node(usage);
            })
            .is_some();
        if has_tier {
            detail.response_service_tier = extract_response_service_tier_value(&root);
        }
        let observed = usage_ok || !detail.response_service_tier.is_empty();
        self.observe(detail, observed);
    }

    pub fn publish(&self, reporter: &UsageReporter) -> bool {
        if !self.observed {
            return false;
        }
        let _ = reporter.publish(self.detail.clone());
        true
    }

    pub fn detail(&self) -> Option<&Detail> {
        self.observed.then_some(&self.detail)
    }
}

/// Plugin executor usage. Codex and OpenAI Responses keep a service tier when
/// token counts live on the completed object instead of `response.usage`.
pub fn parse_plugin_executor_response_usage(protocol: &str, payload: &[u8]) -> Detail {
    if payload.is_empty() {
        return Detail::default();
    }
    match protocol.trim().to_ascii_lowercase().as_str() {
        "claude" => parse_claude_plugin_payload_usage(payload),
        "gemini" => parse_gemini_usage(payload),
        "interactions" | "interactions-response" => parse_interactions_usage(payload),
        "antigravity" => parse_antigravity_usage(payload),
        "codex" | "openai-response" => parse_responses_plugin_executor_usage(payload),
        _ => parse_openai_usage(payload),
    }
}

fn parse_responses_plugin_executor_usage(payload: &[u8]) -> Detail {
    let parsed = parse_codex_usage(payload);
    if parsed
        .as_ref()
        .is_some_and(|detail| has_nonzero_token_usage(detail))
    {
        return parsed.unwrap_or_default();
    }
    let mut openai = parse_openai_usage(payload);
    if has_nonzero_token_usage(&openai) {
        if openai.response_service_tier.is_empty() {
            if let Some(codex) = parsed.as_ref() {
                openai
                    .response_service_tier
                    .clone_from(&codex.response_service_tier);
            }
        }
        return openai;
    }
    parsed.unwrap_or(openai)
}

/// Streaming counterpart of `parse_plugin_executor_response_usage`.
///
/// No production executor calls this yet. The Responses branch falls through
/// to top-level usage when a service tier arrives without `response.usage`.
pub fn observe_plugin_executor_stream(
    protocol: &str,
    payload: &[u8],
    buffer: &mut StreamUsageBuffer,
) {
    if payload.is_empty() {
        return;
    }
    match protocol.trim().to_ascii_lowercase().as_str() {
        "claude" => for_each_stream_line(payload, |line| {
            if let Some(detail) = parse_claude_plugin_stream_line(line) {
                observe_merged_stream_usage(buffer, detail);
            }
        }),
        "gemini" => for_each_stream_line(payload, |line| {
            if let Some(detail) = parse_gemini_stream_usage(line) {
                buffer.observe(detail, true);
            }
        }),
        "interactions" | "interactions-response" => for_each_stream_line(payload, |line| {
            if let Some(detail) = parse_interactions_stream_usage(line) {
                observe_merged_stream_usage(buffer, detail);
            }
        }),
        "antigravity" => for_each_stream_line(payload, |line| {
            if let Some(detail) = parse_antigravity_stream_usage(line) {
                buffer.observe(detail, true);
            }
        }),
        "codex" | "openai-response" => for_each_stream_line(payload, |line| {
            if let Some(json_bytes) = json_payload(line) {
                if let Some(detail) = parse_codex_usage(json_bytes) {
                    if has_nonzero_token_usage(&detail) {
                        buffer.observe(detail, true);
                        return;
                    }
                }
            }
            buffer.observe_openai_stream(line);
        }),
        _ => for_each_stream_line(payload, |line| buffer.observe_openai_stream(line)),
    }
}

pub fn observe_merged_stream_usage(buffer: &mut StreamUsageBuffer, update: Detail) {
    if let Some(existing) = buffer.detail().cloned() {
        buffer.observe(merge_stream_usage_detail(existing, update), true);
        return;
    }
    buffer.observe(update, true);
}

pub fn merge_stream_usage_detail(existing: Detail, update: Detail) -> Detail {
    let mut merged = update;
    if merged.input_tokens == 0 && existing.input_tokens > 0 {
        merged.input_tokens = existing.input_tokens;
    }
    if merged.cached_tokens == 0 && existing.cached_tokens > 0 {
        merged.cached_tokens = existing.cached_tokens;
    }
    if merged.cache_read_tokens == 0 && existing.cache_read_tokens > 0 {
        merged.cache_read_tokens = existing.cache_read_tokens;
    }
    if merged.cache_creation_tokens == 0 && existing.cache_creation_tokens > 0 {
        merged.cache_creation_tokens = existing.cache_creation_tokens;
    }
    if merged.output_tokens == 0 && existing.output_tokens > 0 {
        merged.output_tokens = existing.output_tokens;
    }
    if merged.reasoning_tokens == 0 && existing.reasoning_tokens > 0 {
        merged.reasoning_tokens = existing.reasoning_tokens;
    }
    if merged.response_service_tier.is_empty() {
        merged.response_service_tier = existing.response_service_tier;
    }
    let mut cached = merged
        .cache_read_tokens
        .saturating_add(merged.cache_creation_tokens);
    if cached == 0 {
        cached = merged.cached_tokens;
    }
    let calculated_total = merged
        .input_tokens
        .saturating_add(merged.output_tokens)
        .saturating_add(cached);
    if merged.total_tokens == 0 || merged.total_tokens < calculated_total {
        merged.total_tokens = calculated_total;
    }
    merged.token_breakdown = new_independent_token_breakdown(
        merged.input_tokens,
        merged.cache_read_tokens,
        merged.cache_creation_tokens,
        merged.output_tokens,
        merged.reasoning_tokens,
        merged.total_tokens,
    );
    merged
}

fn parse_claude_plugin_payload_usage(payload: &[u8]) -> Detail {
    let Some(root) = parse_json(payload) else {
        return Detail::default();
    };
    let usage = root
        .get("usage")
        .or_else(|| root.pointer("/message/usage"))
        .filter(|usage| usage.is_object());
    let Some(usage) = usage else {
        return Detail::default();
    };
    let wrapped = format!(r#"{{"usage":{}}}"#, usage);
    parse_claude_usage(wrapped.as_bytes())
}

fn parse_claude_plugin_stream_line(line: &[u8]) -> Option<Detail> {
    if line.len() > MAX_USAGE_STREAM_CHUNK_BYTES {
        return None;
    }
    let payload = json_payload(line)?;
    let root = parse_json(payload)?;
    let usage = root
        .get("usage")
        .or_else(|| root.pointer("/message/usage"))
        .filter(|usage| usage.is_object())?;
    let wrapped = format!(r#"{{"usage":{}}}"#, usage);
    Some(parse_claude_usage(wrapped.as_bytes()))
}

fn for_each_stream_line(payload: &[u8], mut visit: impl FnMut(&[u8])) {
    let mut rest = payload;
    while !rest.is_empty() {
        if let Some(index) = rest.iter().position(|byte| *byte == b'\n') {
            let (line, next) = rest.split_at(index + 1);
            if !trim_ascii(line).is_empty() {
                visit(line);
            }
            rest = next;
            continue;
        }
        if !trim_ascii(rest).is_empty() {
            visit(rest);
        }
        break;
    }
}

pub fn parse_codex_usage(data: &[u8]) -> Option<Detail> {
    let root = parse_json(data)?;
    let tier = extract_response_service_tier_value(&root);
    let usage = get_path(&root, &["response", "usage"]);
    if !usage.is_some_and(has_openai_style_usage_token_fields) {
        return (!tier.is_empty()).then(|| Detail {
            response_service_tier: tier,
            ..Detail::default()
        });
    }
    let mut detail = parse_openai_style_usage_node(usage.unwrap());
    detail.response_service_tier = tier;
    Some(detail)
}

pub fn parse_codex_image_tool_usage(data: &[u8]) -> Option<Detail> {
    let root = parse_json(data)?;
    let usage = get_path(&root, &["response", "tool_usage", "image_gen"])?;
    has_openai_style_usage_token_fields(usage).then(|| parse_openai_style_usage_node(usage))
}

pub fn parse_openai_usage(data: &[u8]) -> Detail {
    let Some(root) = parse_json(data) else {
        return Detail::default();
    };
    let tier = extract_response_service_tier_value(&root);
    let Some(usage) = root
        .get("usage")
        .filter(|usage| has_openai_style_usage_token_fields(usage))
    else {
        return Detail {
            response_service_tier: tier,
            ..Detail::default()
        };
    };
    let mut detail = parse_openai_style_usage_node(usage);
    detail.response_service_tier = tier;
    detail
}

fn has_openai_style_usage_token_fields(usage: &Value) -> bool {
    usage.as_object().is_some_and(|object| {
        object.contains_key("total_tokens") || has_openai_style_usage_bucket_fields(object)
    })
}

fn has_openai_style_usage_bucket_fields(object: &Map<String, Value>) -> bool {
    [
        "prompt_tokens",
        "input_tokens",
        "completion_tokens",
        "output_tokens",
    ]
    .into_iter()
    .any(|key| object.contains_key(key))
        || [
            ["prompt_tokens_details", "cached_tokens"],
            ["input_tokens_details", "cached_tokens"],
            ["prompt_tokens_details", "cache_write_tokens"],
            ["prompt_tokens_details", "cache_creation_tokens"],
            ["input_tokens_details", "cache_write_tokens"],
            ["input_tokens_details", "cache_creation_tokens"],
            ["completion_tokens_details", "reasoning_tokens"],
            ["output_tokens_details", "reasoning_tokens"],
        ]
        .into_iter()
        .any(|path| get_object_path(object, &path).is_some())
}

fn parse_openai_style_usage_node(usage: &Value) -> Detail {
    let object = usage.as_object().expect("validated usage object");
    let input = first_object_value(object, &["prompt_tokens", "input_tokens"]);
    let output = first_object_value(object, &["completion_tokens", "output_tokens"]);
    let mut detail = Detail {
        input_tokens: input.map_or(0, value_i64),
        output_tokens: output.map_or(0, value_i64),
        total_tokens: object.get("total_tokens").map_or(0, value_i64),
        ..Detail::default()
    };
    if let Some(cached) = first_nested_value(
        object,
        &[
            ["prompt_tokens_details", "cached_tokens"],
            ["input_tokens_details", "cached_tokens"],
        ],
    ) {
        detail.cached_tokens = value_i64(cached);
        detail.cache_read_tokens = detail.cached_tokens;
    }
    if let Some(cache_creation) = first_nested_value(
        object,
        &[
            ["input_tokens_details", "cache_creation_tokens"],
            ["input_tokens_details", "cache_write_tokens"],
            ["prompt_tokens_details", "cache_creation_tokens"],
            ["prompt_tokens_details", "cache_write_tokens"],
        ],
    ) {
        detail.cache_creation_tokens = value_i64(cache_creation);
    }
    if let Some(reasoning) = first_nested_value(
        object,
        &[
            ["completion_tokens_details", "reasoning_tokens"],
            ["output_tokens_details", "reasoning_tokens"],
        ],
    ) {
        detail.reasoning_tokens = value_i64(reasoning);
    }
    if has_openai_style_usage_bucket_fields(object) {
        detail.token_breakdown = if input.is_some() && output.is_some() {
            new_subset_token_breakdown(
                detail.input_tokens,
                detail.cache_read_tokens,
                detail.cache_creation_tokens,
                detail.output_tokens,
                detail.reasoning_tokens,
                detail.total_tokens,
            )
        } else {
            new_partial_subset_token_breakdown(
                detail.input_tokens,
                if input.is_some() {
                    detail.cache_read_tokens
                } else {
                    0
                },
                if input.is_some() {
                    detail.cache_creation_tokens
                } else {
                    0
                },
                detail.output_tokens,
                if output.is_some() {
                    detail.reasoning_tokens
                } else {
                    0
                },
                detail.total_tokens,
            )
        };
    } else {
        detail.token_breakdown = new_unclassified_token_breakdown(detail.total_tokens);
    }
    if detail.total_tokens == 0 {
        detail.total_tokens = detail.token_breakdown.total_tokens;
    }
    detail
}

pub fn parse_openai_stream_usage(line: &[u8]) -> Option<Detail> {
    let payload = bounded_json_payload(line)?;
    let root = parse_json(payload)?;
    let tier = extract_response_service_tier_value(&root);
    let usage = root.get("usage");
    if !usage.is_some_and(has_openai_style_usage_token_fields) {
        return (!tier.is_empty()).then(|| Detail {
            response_service_tier: tier,
            ..Detail::default()
        });
    }
    let mut detail = parse_openai_style_usage_node(usage.unwrap());
    detail.response_service_tier = tier;
    Some(detail)
}

pub fn parse_claude_usage(data: &[u8]) -> Detail {
    parse_json(data)
        .and_then(|root| root.get("usage").filter(|usage| usage.is_object()).cloned())
        .map_or_else(Detail::default, |usage| parse_claude_usage_node(&usage))
}

pub fn parse_claude_stream_usage(line: &[u8]) -> Option<Detail> {
    let root = parse_json(bounded_json_payload(line)?)?;
    root.get("usage")
        .filter(|usage| usage.is_object())
        .map(parse_claude_usage_node)
}

fn parse_claude_usage_node(usage: &Value) -> Detail {
    let cache_read = field_i64(usage, "cache_read_input_tokens");
    let cache_creation = field_i64(usage, "cache_creation_input_tokens");
    let mut detail = Detail {
        input_tokens: field_i64(usage, "input_tokens"),
        output_tokens: field_i64(usage, "output_tokens"),
        cached_tokens: cache_read,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        ..Detail::default()
    };
    if detail.cached_tokens == 0 {
        detail.cached_tokens = detail.cache_creation_tokens;
    }
    detail.total_tokens = [
        detail.input_tokens,
        detail.output_tokens,
        detail.cache_read_tokens,
        detail.cache_creation_tokens,
    ]
    .into_iter()
    .fold(0_i64, i64::saturating_add);
    detail.token_breakdown = new_independent_token_breakdown(
        detail.input_tokens,
        detail.cache_read_tokens,
        detail.cache_creation_tokens,
        detail.output_tokens,
        detail.reasoning_tokens,
        detail.total_tokens,
    );
    detail
}

fn parse_gemini_family_usage_detail(node: &Value) -> Detail {
    let cached = field_i64(node, "cachedContentTokenCount");
    let tool = first_field_i64(
        node,
        &["toolUsePromptTokenCount", "tool_use_prompt_token_count"],
    );
    let prompt = field_i64(node, "promptTokenCount");
    let input = if prompt < 0 || tool < 0 {
        None
    } else {
        prompt.checked_add(tool)
    };
    let mut detail = Detail {
        input_tokens: input.unwrap_or_default(),
        output_tokens: field_i64(node, "candidatesTokenCount"),
        reasoning_tokens: field_i64(node, "thoughtsTokenCount"),
        total_tokens: field_i64(node, "totalTokenCount"),
        cached_tokens: cached,
        cache_read_tokens: cached,
        ..Detail::default()
    };
    if input.is_none() {
        detail.token_breakdown = invalid_usage_token_breakdown(detail.total_tokens);
        return detail;
    }
    if detail.total_tokens == 0 {
        let total = detail
            .input_tokens
            .checked_add(detail.output_tokens)
            .and_then(|value| value.checked_add(detail.reasoning_tokens));
        let Some(total) = total.filter(|total| *total >= 0) else {
            detail.token_breakdown = invalid_usage_token_breakdown(0);
            return detail;
        };
        detail.total_tokens = total;
    }
    detail.token_breakdown = new_separate_reasoning_token_breakdown(
        detail.input_tokens,
        detail.cache_read_tokens,
        detail.cache_creation_tokens,
        detail.output_tokens,
        detail.reasoning_tokens,
        detail.total_tokens,
    );
    detail
}

fn parse_interactions_usage_detail(node: &Value) -> Detail {
    let cache_read_value = first_field(node, &["cache_read_tokens", "cacheReadTokens"]);
    let tool = first_field_i64(
        node,
        &[
            "tool_use_tokens",
            "total_tool_use_tokens",
            "toolUseTokens",
            "totalToolUseTokens",
        ],
    );
    let base_input = first_field_i64(
        node,
        &["input_tokens", "prompt_tokens", "total_input_tokens"],
    );
    let input = if base_input < 0 || tool < 0 {
        None
    } else {
        base_input.checked_add(tool)
    };
    let mut detail = Detail {
        input_tokens: input.unwrap_or_default(),
        output_tokens: first_field_i64(
            node,
            &["output_tokens", "completion_tokens", "total_output_tokens"],
        ),
        reasoning_tokens: first_field_i64(
            node,
            &[
                "reasoning_tokens",
                "thoughtsTokenCount",
                "total_thought_tokens",
            ],
        ),
        total_tokens: first_field_i64(node, &["total_tokens", "totalTokenCount"]),
        cached_tokens: first_field_i64(
            node,
            &[
                "cached_tokens",
                "cachedContentTokenCount",
                "total_cached_tokens",
            ],
        ),
        cache_read_tokens: cache_read_value.map_or(0, value_i64),
        cache_creation_tokens: first_field_i64(
            node,
            &[
                "cache_creation_tokens",
                "cacheCreationTokens",
                "cache_write_tokens",
                "cacheWriteTokens",
            ],
        ),
        ..Detail::default()
    };
    if input.is_none() {
        detail.token_breakdown = invalid_usage_token_breakdown(detail.total_tokens);
        return detail;
    }
    if cache_read_value.is_none() && detail.cached_tokens > 0 {
        detail.cache_read_tokens = detail.cached_tokens;
    }
    if detail.total_tokens == 0 {
        let total = detail
            .input_tokens
            .checked_add(detail.output_tokens)
            .and_then(|value| value.checked_add(detail.reasoning_tokens));
        let Some(total) = total.filter(|total| *total >= 0) else {
            detail.token_breakdown = invalid_usage_token_breakdown(0);
            return detail;
        };
        detail.total_tokens = total;
    }
    detail.token_breakdown = new_separate_reasoning_token_breakdown(
        detail.input_tokens,
        detail.cache_read_tokens,
        detail.cache_creation_tokens,
        detail.output_tokens,
        detail.reasoning_tokens,
        detail.total_tokens,
    );
    detail
}

pub fn parse_interactions_usage(data: &[u8]) -> Detail {
    let Some(root) = parse_json(data) else {
        return Detail::default();
    };
    let paths: &[&[&str]] = &[
        &["usage"],
        &["total_usage"],
        &["metadata", "total_usage"],
        &["metadata", "usage"],
        &["usageMetadata"],
        &["usage_metadata"],
        &["interaction", "usage"],
        &["interaction", "total_usage"],
        &["interaction", "metadata", "total_usage"],
    ];
    let Some(node) = paths.iter().find_map(|path| get_path(&root, path)) else {
        return Detail::default();
    };
    let mut detail =
        if node.get("promptTokenCount").is_some() || node.get("candidatesTokenCount").is_some() {
            parse_gemini_family_usage_detail(node)
        } else {
            parse_interactions_usage_detail(node)
        };
    detail.response_service_tier = extract_response_service_tier_value(&root);
    detail
}

pub fn parse_interactions_stream_usage(line: &[u8]) -> Option<Detail> {
    if line.len() > MAX_USAGE_STREAM_CHUNK_BYTES {
        return None;
    }
    let payload = json_payload(line).unwrap_or(line);
    let detail = parse_interactions_usage(payload);
    has_nonzero_token_usage(&detail).then_some(detail)
}

pub fn parse_gemini_usage(data: &[u8]) -> Detail {
    let Some(root) = parse_json(data) else {
        return Detail::default();
    };
    root.get("usageMetadata")
        .or_else(|| root.get("usage_metadata"))
        .map_or_else(Detail::default, parse_gemini_family_usage_detail)
}

pub fn parse_gemini_stream_usage(line: &[u8]) -> Option<Detail> {
    let root = parse_json(bounded_json_payload(line)?)?;
    root.get("usageMetadata")
        .or_else(|| root.get("usage_metadata"))
        .map(parse_gemini_family_usage_detail)
}

pub fn parse_antigravity_usage(data: &[u8]) -> Detail {
    let Some(root) = parse_json(data) else {
        return Detail::default();
    };
    get_path(&root, &["response", "usageMetadata"])
        .or_else(|| root.get("usageMetadata"))
        .or_else(|| root.get("usage_metadata"))
        .map_or_else(Detail::default, parse_gemini_family_usage_detail)
}

pub fn parse_antigravity_stream_usage(line: &[u8]) -> Option<Detail> {
    let root = parse_json(bounded_json_payload(line)?)?;
    get_path(&root, &["response", "usageMetadata"])
        .or_else(|| root.get("usageMetadata"))
        .or_else(|| root.get("usage_metadata"))
        .map(parse_gemini_family_usage_detail)
}

fn invalid_usage_token_breakdown(total: i64) -> TokenBreakdown {
    let total = total.max(0);
    TokenBreakdown {
        schema_version: TOKEN_ACCOUNTING_SCHEMA_VERSION,
        quality: TokenAccountingQuality::Inconsistent,
        total_tokens: total,
        unclassified_tokens: total,
        ..TokenBreakdown::default()
    }
}

/// Per-runtime correlation state for split terminal/usage Gemini SSE chunks.
pub struct SseUsageMetadataFilter {
    stop_without_usage: Mutex<HashMap<String, Instant>>,
    capacity: usize,
    ttl: Duration,
}

impl SseUsageMetadataFilter {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            stop_without_usage: Mutex::new(HashMap::new()),
            capacity: capacity.max(1),
            ttl,
        }
    }

    pub fn filter(&self, payload: &[u8]) -> Vec<u8> {
        if payload.is_empty() || payload.len() > MAX_USAGE_STREAM_CHUNK_BYTES {
            return payload.to_vec();
        }
        let mut found_data = false;
        let mut modified = false;
        let mut lines = payload
            .split(|byte| *byte == b'\n')
            .map(Vec::from)
            .collect::<Vec<_>>();
        for line in &mut lines {
            let trimmed = trim_ascii(line);
            if !trimmed.starts_with(b"data:") {
                continue;
            }
            found_data = true;
            let Some(data_index) = find_subslice(line, b"data:") else {
                continue;
            };
            let raw = trim_ascii(&line[data_index + 5..]);
            let trace = parse_json(raw)
                .and_then(|value| value.get("traceId").and_then(value_string_owned))
                .unwrap_or_default();
            if is_stop_chunk_without_usage(raw) && !trace.is_empty() {
                self.remember_stop_without_usage(trace);
                continue;
            }
            if !trace.is_empty() && self.consume_remembered_stop_if_usage(&trace, raw) {
                continue;
            }
            let Some(cleaned) = strip_usage_metadata_from_json(raw) else {
                continue;
            };
            let mut rebuilt = line[..data_index + 5].to_vec();
            if !cleaned.is_empty() {
                rebuilt.push(b' ');
                rebuilt.extend_from_slice(&cleaned);
            }
            *line = rebuilt;
            modified = true;
        }
        if modified {
            return join_lines(&lines);
        }
        if !found_data {
            let trimmed = trim_ascii(payload);
            return strip_usage_metadata_from_json(trimmed).unwrap_or_else(|| payload.to_vec());
        }
        payload.to_vec()
    }

    fn remember_stop_without_usage(&self, trace: String) {
        let now = Instant::now();
        let mut entries = lock_unpoisoned(&self.stop_without_usage);
        entries.retain(|_, observed| now.saturating_duration_since(*observed) <= self.ttl);
        while entries.len() >= self.capacity {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, observed)| **observed)
                .map(|(trace, _)| trace.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
        entries.insert(trace, now);
    }

    fn consume_remembered_stop_if_usage(&self, trace: &str, raw: &[u8]) -> bool {
        let now = Instant::now();
        let mut entries = lock_unpoisoned(&self.stop_without_usage);
        entries.retain(|_, observed| now.saturating_duration_since(*observed) <= self.ttl);
        if entries.contains_key(trace) && has_usage_metadata(raw) {
            entries.remove(trace);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub(crate) fn remembered_len_for_test(&self) -> usize {
        lock_unpoisoned(&self.stop_without_usage).len()
    }
}

impl Default for SseUsageMetadataFilter {
    fn default() -> Self {
        Self::new(DEFAULT_STOP_TRACE_CAPACITY, DEFAULT_STOP_TRACE_TTL)
    }
}

impl fmt::Debug for SseUsageMetadataFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SseUsageMetadataFilter")
            .field("capacity", &self.capacity)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

/// Returns `Some(cleaned)` only when non-terminal usage metadata was moved to
/// `cpaUsageMetadata`; terminal or irrelevant JSON returns `None` unchanged.
pub fn strip_usage_metadata_from_json(raw: &[u8]) -> Option<Vec<u8>> {
    let mut root = parse_json(raw)?;
    let terminal = get_path(&root, &["candidates", "0", "finishReason"])
        .or_else(|| get_path(&root, &["response", "candidates", "0", "finishReason"]))
        .and_then(value_string_owned)
        .is_some_and(|reason| !reason.trim().is_empty());
    if terminal {
        return None;
    }
    let mut changed = false;
    if let Some(object) = root.as_object_mut() {
        if let Some(usage) = object.remove("usageMetadata") {
            object.insert("cpaUsageMetadata".to_owned(), usage);
            changed = true;
        }
        if let Some(response) = object.get_mut("response").and_then(Value::as_object_mut) {
            if let Some(usage) = response.remove("usageMetadata") {
                response.insert("cpaUsageMetadata".to_owned(), usage);
                changed = true;
            }
        }
    }
    changed.then(|| serde_json::to_vec(&root).unwrap_or_else(|_| raw.to_vec()))
}

pub fn json_payload(line: &[u8]) -> Option<&[u8]> {
    let mut trimmed = trim_ascii(line);
    if trimmed.is_empty() || trimmed == b"[DONE]" || trimmed.starts_with(b"event:") {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix(b"data:") {
        trimmed = trim_ascii(rest);
    }
    (!trimmed.is_empty() && trimmed[0] == b'{').then_some(trimmed)
}

fn bounded_json_payload(line: &[u8]) -> Option<&[u8]> {
    (line.len() <= MAX_USAGE_STREAM_CHUNK_BYTES)
        .then(|| json_payload(line))
        .flatten()
}

fn extract_response_service_tier_value(root: &Value) -> String {
    [
        &["response", "service_tier"][..],
        &["service_tier"][..],
        &["interaction", "service_tier"][..],
    ]
    .into_iter()
    .find_map(|path| get_path(root, path).and_then(value_string_owned))
    .map(|tier| tier.trim().to_owned())
    .filter(|tier| !tier.is_empty())
    .unwrap_or_default()
}

fn parse_json(data: &[u8]) -> Option<Value> {
    serde_json::from_slice(data).ok()
}

fn get_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(root, |node, segment| {
        if let Ok(index) = segment.parse::<usize>() {
            node.as_array()?.get(index)
        } else {
            node.as_object()?.get(*segment)
        }
    })
}

fn get_object_path<'a>(object: &'a Map<String, Value>, path: &[&str; 2]) -> Option<&'a Value> {
    object.get(path[0])?.as_object()?.get(path[1])
}

fn first_nested_value<'a>(
    object: &'a Map<String, Value>,
    paths: &[[&str; 2]],
) -> Option<&'a Value> {
    paths.iter().find_map(|path| get_object_path(object, path))
}

fn first_object_value<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn first_field<'a>(node: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let object = node.as_object()?;
    first_object_value(object, keys)
}

fn first_field_i64(node: &Value, keys: &[&str]) -> i64 {
    first_field(node, keys).map_or(0, value_i64)
}

fn field_i64(node: &Value, key: &str) -> i64 {
    node.get(key).map_or(0, value_i64)
}

fn value_i64(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_f64().map(|value| value as i64))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .unwrap_or_default()
}

fn value_string_owned(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn has_usage_metadata(raw: &[u8]) -> bool {
    parse_json(raw).is_some_and(|root| {
        root.get("usageMetadata").is_some()
            || get_path(&root, &["response", "usageMetadata"]).is_some()
    })
}

fn is_stop_chunk_without_usage(raw: &[u8]) -> bool {
    let Some(root) = parse_json(raw) else {
        return false;
    };
    let finished = get_path(&root, &["candidates", "0", "finishReason"])
        .or_else(|| get_path(&root, &["response", "candidates", "0", "finishReason"]))
        .and_then(value_string_owned)
        .is_some_and(|reason| !reason.trim().is_empty());
    finished && !has_usage_metadata(raw)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    find_subslice(haystack, needle).is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty())
        .then(|| {
            haystack
                .windows(needle.len())
                .position(|window| window == needle)
        })
        .flatten()
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn join_lines(lines: &[Vec<u8>]) -> Vec<u8> {
    let length = lines.iter().map(Vec::len).sum::<usize>() + lines.len().saturating_sub(1);
    let mut joined = Vec::with_capacity(length);
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            joined.push(b'\n');
        }
        joined.extend_from_slice(line);
    }
    joined
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
