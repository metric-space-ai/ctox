// ref: internal/thinking/apply_codex_usage_test.go:335-443 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    ModelInfoView, ResolvedCapabilityThinkingRequest, SummaryConfig, ThinkingEngine,
    ThinkingRequest,
};
use crate::internal::{
    logging::global_logger::{LogLevel, LogOutputController, LogSink},
    modelconfig,
};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Default)]
struct RecordingSink {
    bytes: Mutex<Vec<u8>>,
    attempts: AtomicUsize,
    fail: bool,
}

impl RecordingSink {
    fn lines(&self) -> Vec<String> {
        String::from_utf8(self.bytes.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl LogSink for RecordingSink {
    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(io::Error::other("fixture sink unavailable"));
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
}

fn engine(level: LogLevel) -> (ThinkingEngine, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let output = Arc::new(LogOutputController::new(sink.clone()));
    (
        ThinkingEngine::default().with_log_output(output, level),
        sink,
    )
}

fn selected(id: &str, thinking: bool, user_defined: bool) -> modelconfig::ModelInfo {
    modelconfig::ModelInfo {
        id: id.into(),
        provider_type: "codex".into(),
        support_configuration_update: true,
        user_defined,
        thinking: thinking.then(|| modelconfig::ThinkingSupport {
            levels: ["low", "high", "xhigh"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn native(
    engine: &ThinkingEngine,
    body: &[u8],
    provider: &str,
    selected: Option<&modelconfig::ModelInfo>,
) -> Vec<u8> {
    let from = if provider == "xai" { "codex" } else { provider };
    if let Some(info) = selected {
        let view = ModelInfoView::from(info);
        engine
            .apply_thinking_with_capability_info_and_summary(
                ResolvedCapabilityThinkingRequest {
                    body,
                    source_body: body,
                    model: "request-alias",
                    from_format: from,
                    to_format: provider,
                    provider_key: "codex",
                    model_info: Some(&view),
                    model_info_resolved: true,
                    normalized_updates_changed: false,
                },
                &SummaryConfig::default(),
            )
            .unwrap()
    } else {
        engine
            .apply_thinking(ThinkingRequest {
                body,
                model: "gpt-6-astra",
                from_format: from,
                to_format: provider,
                provider_key: "codex",
            })
            .unwrap()
    }
}

#[test]
fn candidate_thinking_native_logs_respect_level_capabilities_and_absent_effort() {
    let body = br#"{"reasoning":{"effort":"high"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#;
    for level in [LogLevel::Info, LogLevel::Warn, LogLevel::Error] {
        let (engine, sink) = engine(level);
        assert_eq!(native(&engine, body, "codex", None), body);
        assert!(sink.lines().is_empty());
        assert_eq!(sink.attempts.load(Ordering::SeqCst), 0);
    }
    let (engine, sink) = engine(LogLevel::Debug);
    let no_thinking = selected("registered-no-thinking", false, false);
    assert_eq!(native(&engine, body, "codex", Some(&no_thinking)), body);
    assert!(sink.lines().is_empty());
    for absent in [
        b"{}".as_slice(),
        br#"{"input":[{"role":"user","content":"PROMPT_CANARY"}]}"#.as_slice(),
        br#"{"input":[{"type":"configuration_update","reasoning":{"effort":" "}}]}"#.as_slice(),
        b"malformed native bytes".as_slice(),
    ] {
        assert_eq!(native(&engine, absent, "codex", None), absent);
        assert!(sink.lines().is_empty());
    }
}

#[test]
fn candidate_thinking_native_logs_do_not_cross_gateway_output_owners() {
    let (engine_a, sink_a) = engine(LogLevel::Debug);
    let (engine_b, sink_b) = engine(LogLevel::Debug);
    // User-defined models are logged even without declared thinking metadata.
    let info_a = selected("owner-a-private", false, true);
    let info_b = selected("owner-b-private", false, true);
    let body_a = br#"{"input":[{"type":"configuration_update","reasoning":{"effort":"xhigh"}}],"private":"BODY_A"}"#;
    let body_b = br#"{"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}],"private":"BODY_B"}"#;
    assert_eq!(native(&engine_a, body_a, "codex", Some(&info_a)), body_a);
    assert_eq!(native(&engine_b, body_b, "codex", Some(&info_b)), body_b);
    for (sink, own, foreign, level) in [
        (&sink_a, "owner-a-private", "owner-b-private", "xhigh"),
        (&sink_b, "owner-b-private", "owner-a-private", "low"),
    ] {
        let lines = sink.lines();
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert!(line.contains(&format!("model={own}")));
            assert!(line.contains(&format!("level={level}")));
            assert!(!line.contains(foreign));
            assert!(!line.contains("BODY_A"));
            assert!(!line.contains("BODY_B"));
        }
    }
}

#[test]
fn candidate_thinking_native_logs_sink_failure_preserves_success_and_cache_bytes() {
    let sink = Arc::new(RecordingSink {
        fail: true,
        ..Default::default()
    });
    let output = Arc::new(LogOutputController::new(sink.clone()));
    let engine = ThinkingEngine::default().with_log_output(output, LogLevel::Debug);
    let body = br#" {"reasoning":{"effort":"high"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}],"number":1.2300,"number":1e400} "#;
    assert_eq!(native(&engine, body, "codex", None), body);
    assert_eq!(sink.attempts.load(Ordering::SeqCst), 2);
    assert!(sink.lines().is_empty());
}
