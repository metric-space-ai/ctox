//! Live pairing smoke test — fails if WhatsApp's real server doesn't send
//! us a QR ref within 30 seconds of the Noise handshake.
//!
//! This is the test that was missing from the "243 tests pass" claim:
//! every other test exercises codec / crypto / state-machine self-consistency
//! or interop with go-libsignal. **This one** verifies that WA's edge
//! actually accepts our ClientPayload and starts the pairing flow.
//!
//! Marked `#[ignore]` by default because it makes a real network call.
//! Run explicitly with:
//!
//!     cargo test -p whatsapp --test live_pair --ignored -- --nocapture
//!
//! Pass criteria:
//!   - websocket connects to wss://web.whatsapp.com/ws/chat
//!   - Noise XX handshake completes
//!   - Server sends an inbound `<iq><pair-device>` and we extract ≥1 ref
//!   - We emit Event::QrCode within the timeout
//!
//! Fail criteria (hard panic with diagnosis, no silent skip):
//!   - timeout expires without a QrCode event
//!   - server closes the WebSocket before sending pair-device
//!   - any error from connect() or the dispatch loop
//!
//! This is the test that, if it passes, you can claim "the live pairing
//! flow works against the real server" — and if it fails, you cannot.

use std::sync::Arc;
use std::time::Duration;

use wha_client::client::Client;
use wha_client::events::Event;
use wha_client::pair;
use wha_client::version;
use wha_store::MemoryStore;

const TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test]
#[ignore = "live network test against wss://web.whatsapp.com — run with --ignored"]
async fn server_sends_qr_ref_within_30s() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls crypto provider");

    // NOTE: WA's check-update endpoint reports `2.2413.51` for the legacy
    // smartphone-bridge web client, not the multi-device version we need.
    // Multi-device pairing wants the `(2, 3000, X)` series — use the
    // compile-time constant whatsmeow ships.
    let v = wha_client::payload::WA_VERSION;
    eprintln!("[live_pair] using WA Web version: {v}");

    let store = Arc::new(MemoryStore::new());
    let device = store.new_device();
    let (client, mut events) = Client::new(device);

    eprintln!("[live_pair] connecting to wss://web.whatsapp.com/ws/chat");
    client
        .connect()
        .await
        .expect("connect to wss://web.whatsapp.com/ws/chat must succeed");
    eprintln!("[live_pair] connected; awaiting <iq><pair-device> within {TIMEOUT:?}");

    // Drive the event loop until either:
    // - we see Event::QrCode (success), or
    // - we see Event::Disconnected / StreamError (hard fail), or
    // - the timeout fires (hard fail).
    let outcome = tokio::time::timeout(TIMEOUT, async {
        while let Some(evt) = events.recv().await {
            match evt {
                Event::Connected => {
                    eprintln!("[live_pair] event: Connected");
                }
                Event::QrCode { code } => {
                    eprintln!(
                        "[live_pair] event: QrCode received ({} chars). PASS.",
                        code.len()
                    );
                    return Ok(code);
                }
                Event::Disconnected { reason } => {
                    return Err(format!(
                        "server closed websocket before sending pair-device: {reason}"
                    ));
                }
                Event::StreamError { code, text } => {
                    return Err(format!("server sent stream:error code={code} text={text}"));
                }
                Event::UnhandledNode { node } => {
                    if node.tag == "iq" && node.child_by_tag(&["pair-device"]).is_some() {
                        eprintln!("[live_pair] received <iq><pair-device>; routing to handler");
                        if let Err(e) = pair::handle_pair_device(&client, &node).await {
                            return Err(format!("handle_pair_device failed: {e}"));
                        }
                        // The handler will spawn the QR emitter task; the next
                        // iteration of this loop will see Event::QrCode.
                        continue;
                    }
                    eprintln!("[live_pair] (ignored unhandled <{}>)", node.tag);
                }
                Event::PairSuccess { id: _ } => {
                    return Err(
                        "got PairSuccess before QrCode, which shouldn't be possible".into(),
                    );
                }
                _ => {
                    // Notification-derived events, ConnectFailure, offline-sync,
                    // dirty notifications etc. are irrelevant to the pair flow —
                    // ignore them.
                }
            }
        }
        Err("event channel closed before any QrCode arrived".to_string())
    })
    .await;

    match outcome {
        Ok(Ok(code)) => {
            assert!(
                !code.is_empty(),
                "QR code should not be an empty string"
            );
            // Expected shape: ref,base64(noise),base64(identity),base64(adv).
            assert_eq!(
                code.matches(',').count(),
                3,
                "QR string should have 4 comma-separated fields, got: {code:?}"
            );
        }
        Ok(Err(reason)) => panic!("[live_pair] HARD FAIL: {reason}"),
        Err(_) => panic!(
            "[live_pair] HARD FAIL: timed out after {TIMEOUT:?} waiting for Event::QrCode. \
             The server accepted our connection but did not send <iq><pair-device>. \
             Either our ClientPayload is rejected (silently — no stream:error), or our \
             frame decoder dropped the inbound IQ. Add tracing-subscriber and rerun \
             with RUST_LOG=trace to see whether decrypt_frame succeeded for the \
             frames that arrived."
        ),
    }
}
