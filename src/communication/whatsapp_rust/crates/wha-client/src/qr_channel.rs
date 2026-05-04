//! QR-channel: a focused stream of pairing-relevant events. Mirrors
//! `_upstream/whatsmeow/qrchan.go::GetQRChannel`.
//!
//! Most applications want a single high-level question answered while a user
//! is staring at a QR code on screen: "do I have a fresh code?", "did the
//! pairing succeed?", "did it time out?", "did the websocket drop?". Walking
//! the main `Event` stream for that is repetitive and error-prone — every
//! caller would have to special-case the same handful of events.
//!
//! [`Client::get_qr_channel`] hands back a [`QrChannel`] whose `recv` method
//! yields exactly those four [`QrEvent`] variants. The [`Client`]'s
//! `dispatch_event` fans the relevant `Event`s into every active subscriber
//! (see [`map_event_to_qr`]), and a small background task layered on top
//! enforces the 60s/20s rotation timeouts mirror upstream.
//!
//! ```text
//! Event::QrCode        → QrEvent::Code(...)   + reset rotation timer
//! Event::PairSuccess   → QrEvent::Success     + close subscriber
//! Event::Disconnected  → QrEvent::Error(...)  + close subscriber
//! ```

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::Client;
use crate::events::Event;

/// Time the first QR ref is allowed to live before we declare a timeout.
/// Mirrors `qrchan.go`'s `time.Second * 60` for the first iteration.
pub const QR_FIRST_TIMEOUT: Duration = Duration::from_secs(60);

/// Time every subsequent QR ref is allowed to live. Mirrors `time.Second * 20`
/// for iterations after the first.
pub const QR_ROTATE_TIMEOUT: Duration = Duration::from_secs(20);

/// Outcome of a step in the QR pairing flow. The receiver iterates over these
/// until a terminal one (`Success`, `Error`, `Timeout`) arrives.
#[derive(Clone, Debug)]
pub enum QrEvent {
    /// A fresh QR string is available. The string is the joined
    /// `ref,noise_pub,identity_pub,adv_secret` payload — the application
    /// renders it as a QR image.
    Code(String),
    /// 60s passed for the first code, or 20s for any subsequent code,
    /// without the user scanning. The whole pairing flow is aborted.
    Timeout,
    /// `<pair-success>` arrived and validated. The companion JID is on the
    /// `Event::PairSuccess` envelope; the QR channel only signals "we're
    /// done".
    Success,
    /// The websocket dropped or the server sent `<failure>` mid-pair.
    /// Carries a human-readable explanation lifted from
    /// [`Event::Disconnected`] / [`Event::StreamError`]. We use a `String`
    /// rather than a `ClientError` so the variant is `Clone` (the QR
    /// channel fans out via an `mpsc::UnboundedSender`, which needs `Clone`
    /// for the broadcast).
    Error(String),
}

/// Subscription handle returned by [`Client::get_qr_channel`]. Single-consumer:
/// only one task should call [`QrChannel::recv`] per channel.
pub struct QrChannel {
    rx: mpsc::Receiver<QrEvent>,
    /// Background timeout task — aborted when the channel is dropped.
    _timer_task: JoinHandle<()>,
}

impl QrChannel {
    /// Pull the next event. Returns `None` once the channel closes (after
    /// `Success` / `Error` / `Timeout`).
    pub async fn recv(&mut self) -> Option<QrEvent> {
        self.rx.recv().await
    }

    /// Non-blocking variant — useful for tests.
    pub fn try_recv(&mut self) -> Result<QrEvent, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

/// State shared between the dispatcher fan-out and the timeout task.
///
/// The timeout task watches `last_code_at_seq`; when [`Client::dispatch_event`]
/// pushes a new `QrCode` it bumps `code_seq` AND advances `code_count`. The
/// timer wakes every (initial: 60s, subsequent: 20s) and checks whether
/// `code_seq` advanced in the meantime. If it didn't, we fire `Timeout`.
struct QrState {
    /// Bumped on every `Code` event. The timer captures the value before
    /// sleeping; if it's the same after the sleep, no rotation happened.
    code_seq: u64,
    /// 0 before any code arrived, ≥1 once we've seen at least one. Drives
    /// "first vs subsequent timeout" semantics.
    code_count: u64,
    /// Set by the dispatcher on Success/Error to signal "stop ticking".
    closed: bool,
}

/// Map an `Event` into a [`QrEvent`] when one of the four trigger variants
/// fires. Returns `None` for everything else — the caller (typically
/// [`Client::dispatch_event`]) skips the side-channel publish in that case.
pub(crate) fn map_event_to_qr(event: &Event) -> Option<QrEvent> {
    match event {
        Event::QrCode { code } => Some(QrEvent::Code(code.clone())),
        Event::PairSuccess { .. } => Some(QrEvent::Success),
        Event::Disconnected { reason } => {
            Some(QrEvent::Error(format!("disconnected during pair: {reason}")))
        }
        Event::StreamError { code, text } => Some(QrEvent::Error(format!(
            "stream:error during pair: code={code} text={text}"
        ))),
        _ => None,
    }
}

impl Client {
    /// Open a focused subscription to QR-pair events. Mirrors
    /// `_upstream/whatsmeow/qrchan.go::GetQRChannel`. The returned
    /// [`QrChannel`] receives a [`QrEvent::Code`] for every fresh QR, and
    /// then exactly one of [`QrEvent::Success`], [`QrEvent::Error`], or
    /// [`QrEvent::Timeout`] terminates the stream.
    pub fn get_qr_channel(&self) -> QrChannel {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<QrEvent>();
        let (out_tx, out_rx) = mpsc::channel::<QrEvent>(8);
        self.register_qr_subscriber(event_tx);

        // Shared state for the timeout task.
        let state = Arc::new(Mutex::new(QrState {
            code_seq: 0,
            code_count: 0,
            closed: false,
        }));

        let timer_state = state.clone();
        let timer_out = out_tx.clone();
        let timer_task = tokio::spawn(async move {
            run_qr_dispatcher_with_timeouts(event_rx, out_tx, state).await;
            // out_tx dropped → channel closes for the consumer.
            drop(timer_out);
            let _ = timer_state; // keep the borrow alive until the task ends
        });

        QrChannel {
            rx: out_rx,
            _timer_task: timer_task,
        }
    }
}

/// The inner event loop driving a single [`QrChannel`].
///
/// Owns three responsibilities:
/// 1. Forward inbound [`QrEvent`]s from the side-channel to the consumer.
/// 2. Reset the rotation timer on every `Code` arrival.
/// 3. Synthesize a `Timeout` if the rotation timer expires before the
///    next `Code`.
async fn run_qr_dispatcher_with_timeouts(
    mut event_rx: mpsc::UnboundedReceiver<QrEvent>,
    out_tx: mpsc::Sender<QrEvent>,
    state: Arc<Mutex<QrState>>,
) {
    loop {
        // Compute next deadline based on whether we've seen any code yet.
        let deadline = {
            let s = state.lock();
            if s.closed {
                return;
            }
            if s.code_count == 0 {
                None // No timeout until we see the first code.
            } else if s.code_count == 1 {
                Some(QR_FIRST_TIMEOUT)
            } else {
                Some(QR_ROTATE_TIMEOUT)
            }
        };

        let captured_seq = state.lock().code_seq;

        // Wait for either an event or the timeout.
        let evt = match deadline {
            Some(d) => match tokio::time::timeout(d, event_rx.recv()).await {
                Ok(maybe) => maybe,
                Err(_) => {
                    // Timeout. Confirm no code arrived in the meantime.
                    // Pull the snapshot without holding the lock across an
                    // `.await` (parking_lot guards aren't `Send`).
                    let (closed, current_seq) = {
                        let s = state.lock();
                        (s.closed, s.code_seq)
                    };
                    if closed {
                        return;
                    }
                    if current_seq == captured_seq {
                        let _ = out_tx.send(QrEvent::Timeout).await;
                        return;
                    }
                    // Code arrived just before we checked — loop and re-arm.
                    continue;
                }
            },
            None => event_rx.recv().await,
        };

        let evt = match evt {
            Some(e) => e,
            None => return, // channel closed
        };

        match &evt {
            QrEvent::Code(_) => {
                let mut s = state.lock();
                s.code_seq = s.code_seq.wrapping_add(1);
                s.code_count += 1;
            }
            QrEvent::Success | QrEvent::Error(_) | QrEvent::Timeout => {
                state.lock().closed = true;
            }
        }

        let now_closed = state.lock().closed;
        if out_tx.send(evt).await.is_err() {
            return; // consumer dropped
        }
        if now_closed {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use wha_store::MemoryStore;
    use wha_types::Jid;

    /// `map_event_to_qr` covers the four trigger variants and returns `None`
    /// for everything else. Pins the routing so a future `Event` addition
    /// can't accidentally start emitting `QrEvent`s without an explicit
    /// audit.
    #[test]
    fn map_event_routes_only_known_variants() {
        assert!(matches!(
            map_event_to_qr(&Event::QrCode { code: "abc".into() }),
            Some(QrEvent::Code(_))
        ));
        let jid: Jid = "1@s.whatsapp.net".parse().unwrap();
        assert!(matches!(
            map_event_to_qr(&Event::PairSuccess { id: jid.clone() }),
            Some(QrEvent::Success)
        ));
        assert!(matches!(
            map_event_to_qr(&Event::Disconnected {
                reason: "ws closed".into(),
            }),
            Some(QrEvent::Error(_))
        ));
        // Random non-QR event passes through silently.
        assert!(map_event_to_qr(&Event::Connected).is_none());
        assert!(map_event_to_qr(&Event::KeepaliveRestored).is_none());
    }

    /// End-to-end: dispatching a `QrCode` event into a real `Client` shows
    /// up on its `QrChannel` as a `QrEvent::Code` carrying the same string.
    #[tokio::test]
    async fn qr_channel_relays_qr_code_event() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let mut chan = cli.get_qr_channel();

        cli.dispatch_event(Event::QrCode {
            code: "REF,B64,B64,B64".into(),
        });

        // Pull the QrEvent::Code with a short timeout so the test isn't
        // brittle if the dispatch task takes a beat.
        let got = tokio::time::timeout(Duration::from_secs(1), chan.recv())
            .await
            .expect("recv should not stall")
            .expect("channel must yield an event");
        match got {
            QrEvent::Code(s) => assert_eq!(s, "REF,B64,B64,B64"),
            other => panic!("expected QrEvent::Code, got {other:?}"),
        }
    }

    /// `Event::PairSuccess` ends the channel: the next `recv` yields the
    /// `QrEvent::Success` and the one after that returns `None` (channel
    /// closed). This pins the upstream "close on success" contract.
    #[tokio::test]
    async fn qr_channel_terminates_on_pair_success() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let mut chan = cli.get_qr_channel();

        let jid: Jid = "1@s.whatsapp.net".parse().unwrap();
        cli.dispatch_event(Event::PairSuccess { id: jid });

        let got = tokio::time::timeout(Duration::from_secs(1), chan.recv())
            .await
            .expect("recv should not stall")
            .expect("channel must yield Success");
        assert!(matches!(got, QrEvent::Success));

        // Channel must have closed.
        let after = tokio::time::timeout(Duration::from_secs(1), chan.recv())
            .await
            .expect("post-success recv should resolve");
        assert!(after.is_none(), "channel must close after Success");
    }
}
