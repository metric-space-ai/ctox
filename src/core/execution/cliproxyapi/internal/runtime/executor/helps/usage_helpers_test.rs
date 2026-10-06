// ref: internal/runtime/executor/helps/usage_helpers_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::usage_helpers::*;
use crate::sdk::cliproxy::auth::{access_token_sha256, Auth};
use crate::sdk::cliproxy::usage::{
    Detail, Manager, Plugin, Record, TokenAccountingQuality, UsageContext,
};

#[test]
fn parses_openai_chat_completions_usage() {
    let detail = parse_openai_usage(br#"{"usage":{"prompt_tokens":10,"completion_tokens":6,"total_tokens":16,"prompt_tokens_details":{"cached_tokens":4},"completion_tokens_details":{"reasoning_tokens":5}}}"#);
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.total_tokens,
            detail.cached_tokens,
            detail.cache_read_tokens,
            detail.reasoning_tokens,
        ),
        (10, 6, 16, 4, 4, 5)
    );
    assert!(detail.token_breakdown.valid());
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Complete
    );
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 6);
    assert_eq!(detail.token_breakdown.output.non_reasoning_tokens, 1);
}

#[test]
fn parses_openai_responses_usage_and_service_tier() {
    let detail = parse_openai_usage(br#"{"service_tier":"default","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30,"input_tokens_details":{"cached_tokens":7},"output_tokens_details":{"reasoning_tokens":9}}}"#);
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.total_tokens,
            detail.cached_tokens,
            detail.cache_read_tokens,
            detail.reasoning_tokens,
        ),
        (10, 20, 30, 7, 7, 9)
    );
    assert_eq!(detail.response_service_tier, "default");
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 3);
    assert_eq!(detail.token_breakdown.output.non_reasoning_tokens, 11);
}

#[test]
fn openai_total_only_is_unclassified() {
    let detail = parse_openai_usage(br#"{"usage":{"total_tokens":42}}"#);
    assert!(detail.token_breakdown.valid());
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Unclassified
    );
    assert_eq!(detail.total_tokens, 42);
    assert_eq!(detail.token_breakdown.unclassified_tokens, 42);
}

#[test]
fn openai_partial_buckets_preserve_known_tokens() {
    let detail = parse_openai_usage(br#"{"usage":{"input_tokens":10,"total_tokens":15}}"#);
    assert!(detail.token_breakdown.valid());
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Unclassified
    );
    assert_eq!(detail.token_breakdown.input.total_tokens, 10);
    assert_eq!(detail.token_breakdown.unclassified_tokens, 5);
}

#[test]
fn openai_explicit_zero_buckets_remain_inconsistent() {
    let detail =
        parse_openai_usage(br#"{"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":42}}"#);
    assert!(detail.token_breakdown.valid());
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Inconsistent
    );
}

#[test]
fn codex_usage_includes_cache_write_tokens() {
    let detail = parse_codex_usage(br#"{"response":{"service_tier":"priority","usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120,"input_tokens_details":{"cached_tokens":30,"cache_write_tokens":40}}}}"#).unwrap();
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.cached_tokens,
            detail.cache_read_tokens,
            detail.cache_creation_tokens,
            detail.total_tokens,
        ),
        (100, 20, 30, 30, 40, 120)
    );
    assert_eq!(detail.response_service_tier, "priority");
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 30);
    assert_eq!(detail.token_breakdown.input.cache_write_tokens, 40);
}

#[test]
fn openai_cache_creation_aliases_have_upstream_precedence() {
    for (details, expected) in [
        (r#"{"cache_creation_tokens":12}"#, 12),
        (r#"{"cache_write_tokens":13}"#, 13),
    ] {
        let payload = format!(
            r#"{{"usage":{{"input_tokens":20,"output_tokens":1,"total_tokens":21,"input_tokens_details":{details}}}}}"#
        );
        assert_eq!(
            parse_openai_usage(payload.as_bytes()).cache_creation_tokens,
            expected
        );
    }
}

#[test]
fn openai_null_usage_is_ignored_but_tier_is_preserved() {
    assert_eq!(parse_openai_usage(br#"{"usage":null}"#), Detail::default());
    assert_eq!(
        parse_openai_usage(br#"{"service_tier":"default"}"#).response_service_tier,
        "default"
    );
    assert_eq!(
        parse_codex_usage(br#"{"response":{"service_tier":"default"}}"#)
            .unwrap()
            .response_service_tier,
        "default"
    );
}

#[test]
fn openai_stream_null_usage_is_ignored() {
    assert!(parse_openai_stream_usage(br#"data: {"choices":[],"usage":null}"#).is_none());
}

#[test]
fn parses_openai_stream_responses_fields() {
    let detail = parse_openai_stream_usage(br#"data: {"service_tier":"flex","choices":[],"usage":{"input_tokens":8,"output_tokens":5,"total_tokens":13,"input_tokens_details":{"cached_tokens":3},"output_tokens_details":{"reasoning_tokens":2}}}"#).unwrap();
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.total_tokens,
            detail.cached_tokens,
            detail.cache_read_tokens,
            detail.reasoning_tokens,
        ),
        (8, 5, 13, 3, 3, 2)
    );
    assert_eq!(detail.response_service_tier, "flex");
}

#[test]
fn stream_usage_buffer_keeps_last_usage() {
    let mut buffer = StreamUsageBuffer::default();
    buffer.observe(Detail::default(), true);
    buffer.observe(
        Detail {
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
            ..Detail::default()
        },
        false,
    );
    buffer.observe(
        Detail {
            input_tokens: 39_320,
            output_tokens: 26,
            total_tokens: 39_346,
            cached_tokens: 33_280,
            ..Detail::default()
        },
        true,
    );
    let detail = buffer.detail().unwrap();
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.total_tokens,
            detail.cached_tokens,
        ),
        (39_320, 26, 39_346, 33_280)
    );
}

#[test]
fn stream_usage_buffer_preserves_and_overrides_tier() {
    let mut buffer = StreamUsageBuffer::default();
    buffer.observe_openai_stream(br#"data: {"service_tier":"default"}"#);
    buffer.observe_openai_stream(
        br#"data: {"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}"#,
    );
    let detail = buffer.detail().unwrap();
    assert_eq!(
        (detail.input_tokens, detail.response_service_tier.as_str()),
        (1, "default")
    );

    buffer.observe_openai_stream(br#"data: {"service_tier":"priority","usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}"#);
    assert_eq!(buffer.detail().unwrap().response_service_tier, "priority");
}

#[test]
fn stream_usage_buffer_state_transitions_match_upstream() {
    let mut same = StreamUsageBuffer::default();
    same.observe_openai_stream(br#"data: {"service_tier":"flex","usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}"#);
    assert_eq!(same.detail().unwrap().response_service_tier, "flex");

    let mut after = StreamUsageBuffer::default();
    after.observe_openai_stream(
        br#"data: {"usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}"#,
    );
    after.observe_openai_stream(br#"data: {"service_tier":"default"}"#);
    assert_eq!(after.detail().unwrap().response_service_tier, "default");

    let mut ignored = StreamUsageBuffer::default();
    ignored.observe_openai_stream(br#"data: {"content":"the word \"usage\" appears"}"#);
    ignored.observe_openai_stream(br#"data: {"usage":"#);
    ignored.observe_openai_stream(br#"data: {"usage":null}"#);
    assert!(ignored.detail().is_none());

    let mut zero = StreamUsageBuffer::default();
    zero.observe_openai_stream(
        br#"data: {"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}"#,
    );
    assert!(zero.detail().is_some());
}

#[test]
fn stream_usage_buffer_preserves_explicit_zero_observation() {
    let mut buffer = StreamUsageBuffer::default();
    buffer.observe(Detail::default(), true);
    assert_eq!(buffer.detail(), Some(&Detail::default()));
}

#[test]
fn claude_usage_includes_cache_tokens_in_total() {
    let detail = parse_claude_usage(br#"{"usage":{"input_tokens":3085,"output_tokens":253,"cache_read_input_tokens":7,"cache_creation_input_tokens":19514}}"#);
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.cache_read_tokens,
            detail.cache_creation_tokens,
            detail.cached_tokens,
            detail.total_tokens,
        ),
        (3_085, 253, 7, 19_514, 7, 22_859)
    );
    assert_eq!(detail.token_breakdown.input.total_tokens, 22_606);
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 3_085);
}

#[test]
fn claude_cached_tokens_fall_back_to_cache_creation() {
    let detail = parse_claude_usage(br#"{"usage":{"input_tokens":3085,"output_tokens":253,"cache_creation_input_tokens":19514}}"#);
    assert_eq!(detail.cached_tokens, 19_514);
    assert_eq!(detail.total_tokens, 22_852);
}

#[test]
fn claude_stream_parser_and_null_contract() {
    let detail =
        parse_claude_stream_usage(br#"data: {"usage":{"input_tokens":2,"output_tokens":3}}"#)
            .unwrap();
    assert_eq!(detail.total_tokens, 5);
    assert!(parse_claude_stream_usage(br#"data: {"usage":null}"#).is_none());
}

#[test]
fn gemini_usage_normalizes_cached_content() {
    let detail = parse_gemini_usage(br#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":2,"cachedContentTokenCount":4,"totalTokenCount":12}}"#);
    assert_eq!((detail.cached_tokens, detail.cache_read_tokens), (4, 4));
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 6);
    assert_eq!(detail.token_breakdown.total_tokens, 12);
}

#[test]
fn gemini_usage_includes_tool_use_and_reasoning_tokens() {
    let detail = parse_gemini_usage(br#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":2,"thoughtsTokenCount":3,"toolUsePromptTokenCount":5,"totalTokenCount":20}}"#);
    assert_eq!((detail.input_tokens, detail.total_tokens), (15, 20));
    assert!(detail.token_breakdown.valid());
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Complete
    );
    assert_eq!(detail.token_breakdown.output.reasoning_tokens, 3);
}

#[test]
fn gemini_usage_rejects_negative_and_overflowed_tool_sums() {
    for payload in [
        br#"{"usageMetadata":{"promptTokenCount":10,"toolUsePromptTokenCount":-1,"totalTokenCount":10}}"#.as_slice(),
        br#"{"usageMetadata":{"promptTokenCount":9223372036854775807,"toolUsePromptTokenCount":1,"totalTokenCount":9223372036854775807}}"#.as_slice(),
    ] {
        let detail = parse_gemini_usage(payload);
        assert!(detail.input_tokens >= 0);
        assert!(detail.token_breakdown.valid());
        assert_eq!(
            detail.token_breakdown.quality,
            TokenAccountingQuality::Inconsistent
        );
    }
}

#[test]
fn parses_interactions_usage_and_cache_aliases() {
    let detail = parse_interactions_usage(
        br#"{"usage":{"input_tokens":3,"output_tokens":4,"reasoning_tokens":5,"cached_tokens":2}}"#,
    );
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.reasoning_tokens,
            detail.total_tokens,
            detail.cached_tokens,
            detail.cache_read_tokens,
        ),
        (3, 4, 5, 12, 2, 2)
    );
    assert_eq!(detail.token_breakdown.input.uncached_tokens, 1);
    assert_eq!(detail.token_breakdown.output.total_tokens, 9);
    assert_eq!(
        parse_interactions_usage(br#"{"usage":{"input_tokens":3,"cache_write_tokens":2}}"#)
            .cache_creation_tokens,
        2
    );
}

#[test]
fn interactions_include_tool_use_tokens() {
    let detail = parse_interactions_usage(br#"{"usage":{"total_input_tokens":2,"total_output_tokens":6,"total_thought_tokens":3,"total_tool_use_tokens":4,"total_tokens":15}}"#);
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.reasoning_tokens,
            detail.total_tokens,
        ),
        (6, 6, 3, 15)
    );
    assert!(detail.token_breakdown.valid());
    assert_eq!(detail.token_breakdown.output.total_tokens, 9);
}

#[test]
fn interactions_stream_accepts_raw_and_sse_official_metadata() {
    let raw = parse_interactions_stream_usage(br#"{"type":"interaction.completed","interaction":{"usage":{"input_tokens":2,"output_tokens":6,"total_tokens":8}}}"#).unwrap();
    assert_eq!(raw.total_tokens, 8);
    let official = parse_interactions_stream_usage(br#"data: {"event_type":"finish","metadata":{"total_usage":{"total_input_tokens":2,"total_output_tokens":6,"total_thought_tokens":3,"total_cached_tokens":1,"total_tokens":11}}}"#).unwrap();
    assert_eq!(
        (
            official.input_tokens,
            official.output_tokens,
            official.reasoning_tokens,
            official.cached_tokens,
            official.cache_read_tokens,
            official.total_tokens,
        ),
        (2, 6, 3, 1, 1, 11)
    );
}

#[test]
fn normalization_does_not_double_count_reasoning() {
    let detail = normalize_usage_detail_total(
        Detail {
            input_tokens: 100,
            output_tokens: 30,
            reasoning_tokens: 12,
            ..Detail::default()
        },
        "openai",
        "",
    );
    assert_eq!(detail.total_tokens, 130);
    assert_eq!(
        detail.token_breakdown.quality,
        TokenAccountingQuality::Complete
    );
    assert_eq!(detail.token_breakdown.output.reasoning_tokens, 12);
}

#[test]
fn all_provider_stream_parsers_preserve_usage_contracts() {
    let gemini = parse_gemini_stream_usage(br#"data: {"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}"#).unwrap();
    assert_eq!(gemini.total_tokens, 5);
    let anti = parse_antigravity_stream_usage(br#"data: {"response":{"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}}"#).unwrap();
    assert_eq!(anti, gemini);
    assert_eq!(
        parse_antigravity_usage(br#"{"response":{"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}}"#),
        gemini
    );
    assert!(parse_codex_image_tool_usage(br#"{"response":{"tool_usage":{"image_gen":{"input_tokens":4,"output_tokens":1,"total_tokens":5}}}}"#).is_some());
}

#[derive(Default)]
struct RecordingPlugin(Mutex<Vec<Record>>);

impl Plugin for RecordingPlugin {
    fn handle_usage(&self, _context: &UsageContext, record: &Record) {
        self.0.lock().unwrap().push(record.clone());
    }
}

fn reporter() -> (Arc<Manager>, Arc<RecordingPlugin>, Arc<UsageReporter>) {
    let manager = Arc::new(Manager::new(16));
    let plugin = Arc::new(RecordingPlugin::default());
    manager.register(plugin.clone());
    let context = UsageContext::default()
        .with_requested_model_alias("client-gpt")
        .with_reasoning_effort("medium")
        .with_service_tier("auto");
    let reporter = Arc::new(UsageReporter::new(
        manager.clone(),
        context,
        "openai",
        "OpenAIExecutor",
        "gpt-5.4",
        None,
        "secret-api-key",
    ));
    (manager, plugin, reporter)
}

#[test]
fn reporter_builds_record_with_alias_reasoning_tier_generate_and_latency() {
    let (manager, _plugin, reporter) = reporter();
    std::thread::sleep(Duration::from_millis(2));
    let record = reporter.build_record(
        Detail {
            total_tokens: 3,
            response_service_tier: "default".to_owned(),
            ..Detail::default()
        },
        false,
        Default::default(),
    );
    assert_eq!(record.model, "gpt-5.4");
    assert_eq!(record.alias, "client-gpt");
    assert_eq!(record.reasoning_effort, "medium");
    assert_eq!(record.service_tier, "auto");
    assert_eq!(record.response_service_tier, "default");
    assert_eq!(record.generate, Some(true));
    assert!(record.latency >= Duration::from_millis(1));
    assert!(!format!("{reporter:?}").contains("secret-api-key"));
    manager.stop();
}

#[test]
fn reporter_records_initial_and_refreshed_access_token_fingerprint_without_secret() {
    let manager = Arc::new(Manager::new(1));
    let mut auth = Auth::default();
    auth.metadata.insert(
        "access_token".to_owned(),
        serde_json::json!("initial-access-secret"),
    );
    let reporter = UsageReporter::new(
        manager.clone(),
        UsageContext::default(),
        "antigravity",
        "AntigravityExecutor",
        "gemini-3-pro",
        Some(&auth),
        "",
    );

    let initial = reporter.build_record(Detail::default(), false, Default::default());
    assert_eq!(initial.access_token_sha256, access_token_sha256(&auth));
    assert!(!initial
        .access_token_sha256
        .contains("initial-access-secret"));
    assert!(!reporter.update_access_token_fingerprint_sha256("refreshed-access-secret"));
    assert!(reporter
        .build_record(Detail::default(), false, Default::default())
        .access_token_sha256
        .is_empty());

    auth.metadata.insert(
        "access_token".to_owned(),
        serde_json::json!("refreshed-access-secret"),
    );
    reporter.update_access_token_fingerprint(&auth);
    let refreshed = reporter.build_record(Detail::default(), false, Default::default());
    assert_eq!(refreshed.access_token_sha256, access_token_sha256(&auth));
    assert_ne!(refreshed.access_token_sha256, initial.access_token_sha256);
    let diagnostics = format!("{reporter:?}");
    assert!(!diagnostics.contains("initial-access-secret"));
    assert!(!diagnostics.contains("refreshed-access-secret"));
    manager.stop();
}

#[test]
fn reporter_ttft_starts_before_first_nonempty_chunk_and_sets_once() {
    let (manager, _plugin, reporter) = reporter();
    reporter.start_response_ttft();
    reporter.observe_response_chunk(b"");
    std::thread::sleep(Duration::from_millis(3));
    reporter.observe_response_chunk(b"first");
    let first = reporter
        .build_record(Detail::default(), false, Default::default())
        .ttft;
    reporter.observe_response_chunk(b"second");
    let second = reporter
        .build_record(Detail::default(), false, Default::default())
        .ttft;
    assert!(first >= Duration::from_millis(2));
    assert_eq!(first, second);
    manager.stop();
}

#[test]
fn reporter_publishes_exactly_once_under_concurrency() {
    let (manager, plugin, reporter) = reporter();
    let workers = (0..32)
        .map(|_| {
            let reporter = reporter.clone();
            std::thread::spawn(move || reporter.publish(Detail::default()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|published| *published)
            .count(),
        1
    );
    manager.stop();
    assert_eq!(plugin.0.lock().unwrap().len(), 1);
}

#[test]
fn reporter_skips_zero_additional_model_but_records_token_usage() {
    let (manager, plugin, reporter) = reporter();
    assert!(!reporter.publish_additional_model("gpt-image-2", Detail::default()));
    assert!(reporter.publish_additional_model(
        "gpt-image-2",
        Detail {
            input_tokens: 2,
            ..Detail::default()
        }
    ));
    manager.stop();
    let records = plugin.0.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "gpt-image-2");
}

#[test]
fn reporter_generate_false_and_translated_reasoning_do_not_clobber_tier() {
    let manager = Arc::new(Manager::new(1));
    let reporter = UsageReporter::new(
        manager.clone(),
        UsageContext::default()
            .with_service_tier("auto")
            .with_generate(false),
        "openai",
        "OpenAIExecutor",
        "gpt-5.4",
        None,
        "",
    );
    reporter.set_translated_reasoning_effort(
        br#"{"reasoning":{"effort":"high"},"service_tier":"priority"}"#,
        "openai",
    );
    let record = reporter.build_record(Detail::default(), false, Default::default());
    assert_eq!(record.service_tier, "auto");
    assert_eq!(record.generate, Some(false));
    assert_eq!(record.reasoning_effort, "high");
    manager.stop();
}

#[test]
fn reporter_resolves_vertex_source_auth_kind_and_stable_index() {
    let manager = Arc::new(Manager::new(1));
    let mut auth = Auth::default();
    auth.id = "vertex-auth".to_owned();
    auth.provider = "vertex".to_owned();
    auth.attributes
        .insert("auth_kind".to_owned(), "oauth".to_owned());
    auth.metadata.insert(
        "project_id".to_owned(),
        serde_json::Value::String(" project-1 ".to_owned()),
    );
    let reporter = UsageReporter::new(
        manager.clone(),
        UsageContext::default(),
        "vertex",
        "GeminiVertexExecutor",
        "gemini-3-pro",
        Some(&auth),
        "context-key",
    );
    let record = reporter.build_record(Detail::default(), false, Default::default());
    assert_eq!(record.source, "project-1");
    assert_eq!(record.auth_type, "oauth");
    assert_eq!(record.auth_id, "vertex-auth");
    assert!(!record.auth_index.is_empty());
    manager.stop();
}

#[test]
fn sse_metadata_filter_moves_nonterminal_usage_and_preserves_terminal() {
    let filter = SseUsageMetadataFilter::default();
    let cleaned = filter
        .filter(br#"data: {"candidates":[{"content":{}}],"usageMetadata":{"promptTokenCount":2}}"#);
    let text = String::from_utf8(cleaned).unwrap();
    assert!(!text.contains("\"usageMetadata\""));
    assert!(text.contains("\"cpaUsageMetadata\""));

    let terminal =
        br#"data: {"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"totalTokenCount":5}}"#;
    assert_eq!(filter.filter(terminal), terminal);
}

#[test]
fn split_stop_and_usage_chunks_are_correlated_per_instance_and_bounded() {
    let filter = SseUsageMetadataFilter::new(2, Duration::from_secs(60));
    for trace in ["one", "two", "three"] {
        let stop =
            format!(r#"data: {{"traceId":"{trace}","candidates":[{{"finishReason":"STOP"}}]}}"#);
        assert_eq!(filter.filter(stop.as_bytes()), stop.as_bytes());
    }
    assert_eq!(filter.remembered_len_for_test(), 2);
    let delayed = br#"data: {"traceId":"three","usageMetadata":{"totalTokenCount":5}}"#;
    assert_eq!(filter.filter(delayed), delayed);
    assert_eq!(filter.remembered_len_for_test(), 1);

    let independent = SseUsageMetadataFilter::default();
    let changed = independent.filter(delayed);
    assert!(String::from_utf8(changed)
        .unwrap()
        .contains("cpaUsageMetadata"));
}

#[test]
fn oversized_stream_chunks_are_rejected_without_state_or_rewrite() {
    let oversized = vec![b' '; MAX_USAGE_STREAM_CHUNK_BYTES + 1];
    let mut buffer = StreamUsageBuffer::default();
    buffer.observe_openai_stream(&oversized);
    assert!(buffer.detail().is_none());
    let filter = SseUsageMetadataFilter::default();
    assert_eq!(filter.filter(&oversized), oversized);
    assert_eq!(filter.remembered_len_for_test(), 0);
}

#[test]
fn json_payload_matches_sse_framing_contract() {
    assert_eq!(
        json_payload(br#" data: {"ok":true} "#),
        Some(br#"{"ok":true}"#.as_slice())
    );
    assert!(json_payload(b"event: message").is_none());
    assert!(json_payload(b"[DONE]").is_none());
    assert!(json_payload(b"plain text").is_none());
}

#[test]
fn plugin_responses_usage_keeps_top_level_tokens_beside_service_tier() {
    let payload = br#"{"id":"resp_1","object":"response","service_tier":"default","usage":{"input_tokens":34,"output_tokens":499,"total_tokens":533}}"#;
    let detail = parse_plugin_executor_response_usage("openai-response", payload);
    assert_eq!(
        (
            detail.input_tokens,
            detail.output_tokens,
            detail.total_tokens
        ),
        (34, 499, 533)
    );
    assert_eq!(detail.response_service_tier, "default");

    let mut buffer = StreamUsageBuffer::default();
    observe_plugin_executor_stream(
        "openai-response",
        b"data: {\"type\":\"response.completed\",\"service_tier\":\"default\",\"usage\":{\"input_tokens\":34,\"output_tokens\":499,\"total_tokens\":533}}\n\n",
        &mut buffer,
    );
    let observed = buffer.detail().unwrap();
    assert_eq!(
        (
            observed.input_tokens,
            observed.output_tokens,
            observed.total_tokens,
            observed.response_service_tier.as_str(),
        ),
        (34, 499, 533, "default")
    );
}

#[test]
fn plugin_responses_usage_shapes_match_upstream() {
    let cases = [
        (
            "openai-response",
            br#"{"usage":{"input_tokens":34,"output_tokens":499,"total_tokens":533}}"#.as_slice(),
            34,
            499,
            533,
            "",
        ),
        (
            "codex",
            br#"{"service_tier":"default","usage":{"input_tokens":34,"output_tokens":499,"total_tokens":533}}"#.as_slice(),
            34,
            499,
            533,
            "default",
        ),
        (
            "openai-response",
            br#"{"service_tier":"priority","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2},"response":{"usage":{"input_tokens":18,"output_tokens":22,"total_tokens":40}}}"#.as_slice(),
            18,
            22,
            40,
            "priority",
        ),
        (
            "openai-response",
            br#"{"service_tier":"default"}"#.as_slice(),
            0,
            0,
            0,
            "default",
        ),
    ];
    for (protocol, payload, input, output, total, tier) in cases {
        let detail = parse_plugin_executor_response_usage(protocol, payload);
        assert_eq!(
            (
                detail.input_tokens,
                detail.output_tokens,
                detail.total_tokens,
                detail.response_service_tier.as_str(),
            ),
            (input, output, total, tier),
            "{protocol}"
        );
    }
}
