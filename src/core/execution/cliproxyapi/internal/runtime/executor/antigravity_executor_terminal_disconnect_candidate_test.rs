// ref: internal/runtime/executor/antigravity_executor_terminal_disconnect_test.go @ a4acc9f752bd46571f737a10c04bf413656ab06b
// Actual native stream owner, replay lane, usage manager and cooldown guards.

#[derive(Default)]
struct AntigravityTerminalUsageCapture(Mutex<Vec<crate::sdk::cliproxy::usage::Record>>);

impl crate::sdk::cliproxy::usage::Plugin for AntigravityTerminalUsageCapture {
    fn handle_usage(
        &self,
        _: &crate::sdk::cliproxy::usage::UsageContext,
        record: &crate::sdk::cliproxy::usage::Record,
    ) {
        self.0.lock().unwrap().push(record.clone());
    }
}

type AntigravityTerminalFixture = (
    AntigravityTrackedResponsesStream,
    tokio::sync::mpsc::Sender<Result<Vec<u8>, AntigravityGenerateTransportFailure>>,
    Arc<AntigravityReasoningReplayCache>,
    Arc<crate::sdk::cliproxy::usage::Manager>,
    Arc<AntigravityTerminalUsageCapture>,
    Arc<MemoryCooldown>,
);

async fn antigravity_terminal_fixture(
    wire: &[u8],
    error: Option<AntigravityGenerateTransportFailure>,
    claude: bool,
) -> AntigravityTerminalFixture {
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    sender.try_send(Ok(wire.to_vec())).unwrap();
    if let Some(error) = error {
        sender.try_send(Err(error)).unwrap();
    }
    // Retain sender in the test: receiving completion is not upstream EOF.
    let upstream = super::super::antigravity_executor::AntigravityGenerateStreamResponse::new(
        200, None, receiver,
    );
    let original = br#"{"model":"gemini-3","session_id":"terminal-session","input":"ask"}"#;
    let translated = br#"{"request":{"contents":[{"role":"user","parts":[{"text":"ask"}]}]}}"#;
    let cache = Arc::new(AntigravityReasoningReplayCache::new());
    let (_, replay) = prepare_antigravity_reasoning_replay(
        cache.clone(),
        "gemini-3",
        "responses:terminal-session",
        translated,
        replay_now_ms(),
    )
    .unwrap();
    let manager = Arc::new(crate::sdk::cliproxy::usage::Manager::new(4));
    let capture = Arc::new(AntigravityTerminalUsageCapture::default());
    manager.register(capture.clone());
    let reporter = Arc::new(super::super::helps::UsageReporter::new(
        manager.clone(),
        crate::sdk::cliproxy::usage::UsageContext::default(),
        "antigravity",
        "AntigravitySubscriptionExecutor",
        "gemini-3",
        None,
        "",
    ));
    let stream = if claude {
        AntigravityResponsesStream::new_claude(
            upstream,
            original.to_vec(),
            translated.to_vec(),
            String::new(),
            None,
        )
    } else {
        AntigravityResponsesStream::new(upstream, original.to_vec(), translated.to_vec())
    };
    let mut stream = stream.with_replay_accumulator(replay);
    stream.bootstrap().await.unwrap();
    let state = Arc::new(MemoryCooldown::default());
    let tracked = AntigravityStreamExecutionOutcome {
        stream,
        attempts: 1,
    }
    .into_tracked(
        "terminal-account".to_owned(),
        "gemini-3".to_owned(),
        Arc::new(CooldownConductor::new(state.clone())),
        Arc::new(FixedAccountClock(replay_now_ms())),
    );
    (
        tracked.with_usage_reporter(reporter),
        sender,
        cache,
        manager,
        capture,
        state,
    )
}

const ANTIGRAVITY_TERMINAL_WIRE: &[u8] = br#"data: {"response":{"responseId":"terminal-response","candidates":[{"content":{"parts":[{"text":"signed answer","thoughtSignature":"terminal-native-signature-123456"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}}

"#;

const ANTIGRAVITY_PARTIAL_WIRE: &[u8] = br#"data: {"response":{"responseId":"partial-response","candidates":[{"content":{"parts":[{"text":"signed answer","thoughtSignature":"terminal-native-signature-123456"}]}}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}}

"#;

async fn consume_antigravity_terminal(stream: &mut AntigravityTrackedResponsesStream, kind: &str) {
    for _ in 0..64 {
        let event = tokio::time::timeout(Duration::from_secs(1), stream.next_event())
            .await
            .expect("native terminal should not require upstream EOF")
            .expect("terminal event expected")
            .expect("terminal should not be a transport error");
        if event.split(|byte| *byte == b'\n').any(|line| {
            let line = line.trim_ascii();
            let payload = line.strip_prefix(b"data:").unwrap_or(line).trim_ascii();
            serde_json::from_slice::<serde_json::Value>(payload)
                .ok()
                .is_some_and(|value| value["type"] == kind)
        }) {
            return;
        }
    }
    panic!("terminal event was not delivered");
}

fn assert_antigravity_terminal_usage(
    manager: &crate::sdk::cliproxy::usage::Manager,
    capture: &AntigravityTerminalUsageCapture,
    failed: bool,
) {
    manager.stop(); // Drains and joins the request-owned usage worker.
    let records = capture.0.lock().unwrap();
    assert_eq!(records.len(), 1, "exactly one usage record per request");
    assert_eq!(records[0].failed, failed);
    assert_eq!(records[0].detail.input_tokens, 10);
    assert_eq!(records[0].detail.output_tokens, 5);
    assert_eq!(records[0].detail.total_tokens, 15);
}

fn assert_antigravity_terminal_replay(cache: &AntigravityReasoningReplayCache, present: bool) {
    let (items, _, found) = cache
        .read("gemini-3", "responses:terminal-session", replay_now_ms())
        .unwrap();
    assert_eq!(found, present);
    if present {
        assert!(!items.is_empty());
        assert!(items
            .iter()
            .any(
                |item| serde_json::from_slice::<serde_json::Value>(item).unwrap()
                    ["thoughtSignature"]
                    == "terminal-native-signature-123456"
            ));
    }
}

#[tokio::test]
async fn candidate_antigravity_terminal_drop_keeps_replay_and_observed_usage() {
    let (mut stream, sender, cache, manager, capture, state) =
        antigravity_terminal_fixture(ANTIGRAVITY_TERMINAL_WIRE, None, false).await;
    consume_antigravity_terminal(&mut stream, "response.completed").await;
    assert!(!sender.is_closed(), "the upstream body is still live");
    assert_antigravity_terminal_replay(&cache, true);
    stream.record_terminal_failure().await; // Post-completion downstream disconnect.
    assert!(state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_usage(&manager, &capture, false);
}

#[tokio::test]
async fn candidate_antigravity_terminal_cancel_is_success_not_cooldown() {
    let (mut stream, sender, cache, manager, capture, state) = antigravity_terminal_fixture(
        ANTIGRAVITY_TERMINAL_WIRE,
        Some(AntigravityGenerateTransportFailure::Cancelled),
        false,
    )
    .await;
    consume_antigravity_terminal(&mut stream, "response.completed").await;
    assert!(stream.next_event().await.is_none());
    stream.record_terminal_failure().await;
    assert!(state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_replay(&cache, true);
    assert_antigravity_terminal_usage(&manager, &capture, false);
}

#[tokio::test]
async fn candidate_antigravity_terminal_plain_cancel_on_claude_keeps_success() {
    let (mut stream, sender, cache, manager, capture, state) =
        antigravity_terminal_fixture(ANTIGRAVITY_TERMINAL_WIRE, None, true).await;
    consume_antigravity_terminal(&mut stream, "message_stop").await;
    stream.cancel();
    assert!(stream.next_event().await.is_none());
    stream.record_terminal_failure().await;
    assert!(state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_replay(&cache, true);
    assert_antigravity_terminal_usage(&manager, &capture, false);
}

#[tokio::test]
async fn candidate_antigravity_terminal_partial_cancel_keeps_measured_failure() {
    let (mut stream, sender, cache, manager, capture, state) = antigravity_terminal_fixture(
        ANTIGRAVITY_PARTIAL_WIRE,
        Some(AntigravityGenerateTransportFailure::Cancelled),
        false,
    )
    .await;
    loop {
        match stream.next_event().await {
            Some(Ok(_)) => {}
            Some(Err(error)) => {
                assert_eq!(error, AntigravityGenerateTransportFailure::Cancelled);
                break;
            }
            None => panic!("partial cancellation must not become successful EOF"),
        }
    }
    assert!(!state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_replay(&cache, false);
    assert_antigravity_terminal_usage(&manager, &capture, true);
}

#[tokio::test]
async fn candidate_antigravity_terminal_protocol_failure_is_not_cancellation() {
    let (mut stream, sender, cache, manager, capture, state) = antigravity_terminal_fixture(
        ANTIGRAVITY_TERMINAL_WIRE,
        Some(AntigravityGenerateTransportFailure::Protocol),
        false,
    )
    .await;
    consume_antigravity_terminal(&mut stream, "response.completed").await;
    assert_eq!(
        stream.next_event().await,
        Some(Err(AntigravityGenerateTransportFailure::Protocol))
    );
    assert!(!state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_replay(&cache, true);
    assert_antigravity_terminal_usage(&manager, &capture, true);
}

#[tokio::test]
async fn candidate_antigravity_terminal_unconsumed_completion_is_not_success() {
    let (stream, sender, cache, manager, capture, state) =
        antigravity_terminal_fixture(ANTIGRAVITY_TERMINAL_WIRE, None, false).await;
    drop(stream); // Bootstrap queued completion, but the consumer never received it.
    assert!(sender.is_closed());
    assert!(
        state.0.lock().unwrap().is_empty(),
        "drop does not spawn an unowned cooldown task"
    );
    assert_antigravity_terminal_replay(&cache, false);
    assert_antigravity_terminal_usage(&manager, &capture, true);
}

#[tokio::test]
async fn candidate_antigravity_terminal_buffered_last_line_precedes_cancellation() {
    let wire = ANTIGRAVITY_TERMINAL_WIRE.trim_ascii();
    let (mut stream, sender, cache, manager, capture, state) = antigravity_terminal_fixture(
        wire,
        Some(AntigravityGenerateTransportFailure::Cancelled),
        false,
    )
    .await;
    consume_antigravity_terminal(&mut stream, "response.completed").await;
    assert!(stream.next_event().await.is_none());
    assert!(state.0.lock().unwrap().is_empty());
    drop(stream);
    assert!(sender.is_closed());
    assert_antigravity_terminal_replay(&cache, true);
    assert_antigravity_terminal_usage(&manager, &capture, false);
}
