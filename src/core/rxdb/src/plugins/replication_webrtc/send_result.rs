//! Observable physical DataChannel handoff. No payloads are logged.
//!
//! Success means accepted by SCTP, not delivered. Compare class-separated
//! handoff/receive deltas only after draining a loss-free session; reconnect
//! replay and application acknowledgements are a higher-layer contract.
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;
use webrtc::data_channel::{DataChannel, RTCDataChannelState};
use webrtc::error::{Error, Result};

use crate::rxjs_compat::{RxStream, RxSubject};

pub const MAX_SCTP_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageClass {
    Reliable,
    Realtime,
}
impl MessageClass {
    fn index(self) -> usize {
        match self {
            Self::Reliable => 0,
            Self::Realtime => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SendFailureKind {
    TooLarge,
    StreamMissing,
    NotReady,
    SctpRejected,
    EventSubscriberLagged,
}

#[derive(Clone, Debug, Serialize)]
pub struct SendFailureEvent {
    pub class: MessageClass,
    pub kind: SendFailureKind,
    pub channel_id: u16,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ClassCounters {
    pub attempted: u64,
    pub handed_to_sctp: u64,
    pub received: u64,
    pub logical_sent: u64,
    pub logical_received: u64,
    pub too_large: u64,
    pub stream_missing: u64,
    pub not_ready: u64,
    pub sctp_rejected: u64,
}
impl ClassCounters {
    pub fn failed(self) -> u64 {
        self.too_large + self.stream_missing + self.not_ready + self.sctp_rejected
    }
}

/// Snapshot and subscribe before a measurement; counters survive event lag.
/// An explicit lag event makes a slow observer detectable as well.
pub struct SendDiagnostics {
    counters: Mutex<[ClassCounters; 2]>,
    events: RxSubject<SendFailureEvent>,
}
impl Default for SendDiagnostics {
    fn default() -> Self {
        Self {
            counters: Mutex::new([ClassCounters::default(); 2]),
            events: RxSubject::with_lag_signal(256, |skipped| {
                Some(SendFailureEvent {
                    class: MessageClass::Reliable,
                    kind: SendFailureKind::EventSubscriberLagged,
                    channel_id: 0,
                    bytes: skipped as usize,
                })
            }),
        }
    }
}
impl SendDiagnostics {
    pub fn counters(&self, class: MessageClass) -> ClassCounters {
        self.counters.lock()[class.index()]
    }
    pub fn errors(&self) -> RxStream<SendFailureEvent> {
        self.events.subscribe()
    }
    pub fn received(&self, class: MessageClass) {
        self.counters.lock()[class.index()].received += 1;
    }
    /// Primary replication channel only: complete logical messages, excluding
    /// chunks, ACKs and the separate auxiliary reassembly protocol.
    pub fn logical_sent(&self, class: MessageClass) {
        self.counters.lock()[class.index()].logical_sent += 1;
    }
    pub fn logical_received(&self, class: MessageClass) {
        self.counters.lock()[class.index()].logical_received += 1;
    }
    pub fn reject_not_ready(&self, bytes: usize) {
        self.complete(
            MessageClass::Reliable,
            0,
            bytes,
            Some(SendFailureKind::NotReady),
        );
    }
    fn complete(
        &self,
        class: MessageClass,
        channel_id: u16,
        bytes: usize,
        failure: Option<SendFailureKind>,
    ) {
        {
            let mut classes = self.counters.lock();
            let count = &mut classes[class.index()];
            count.attempted += 1;
            match failure {
                None => count.handed_to_sctp += 1,
                Some(SendFailureKind::TooLarge) => count.too_large += 1,
                Some(SendFailureKind::StreamMissing) => count.stream_missing += 1,
                Some(SendFailureKind::NotReady) => count.not_ready += 1,
                Some(_) => count.sctp_rejected += 1,
            }
        }
        if let Some(kind) = failure {
            tracing::warn!(target: "ctox_webrtc_send", ?class, ?kind,
                channel_id, bytes, "DataChannel send rejected before SCTP handoff");
            self.events.next(SendFailureEvent {
                class,
                kind,
                channel_id,
                bytes,
            });
        }
    }

    pub async fn send_text(
        &self,
        channel: &dyn DataChannel,
        text: &str,
        class: MessageClass,
    ) -> Result<()> {
        let result = if text.len() > MAX_SCTP_MESSAGE_BYTES {
            Err((SendFailureKind::TooLarge, Error::ErrOutboundPacketTooLarge))
        } else {
            match channel.ready_state().await {
                Ok(RTCDataChannelState::Open) => channel.send_text(text).await.map_err(|error| {
                    let kind = match &error {
                        Error::ErrDataChannelNotExisted
                        | Error::ErrAssociationNotExisted
                        | Error::ErrStreamNotExisted => SendFailureKind::StreamMissing,
                        Error::ErrDataChannelClosed
                        | Error::ErrSCTPNotEstablished
                        | Error::ErrStreamClosed => SendFailureKind::NotReady,
                        Error::ErrOutboundPacketTooLarge => SendFailureKind::TooLarge,
                        _ => SendFailureKind::SctpRejected,
                    };
                    (kind, error)
                }),
                Ok(_) => Err((SendFailureKind::NotReady, Error::ErrDataChannelClosed)),
                Err(error) => Err((SendFailureKind::StreamMissing, error)),
            }
        };
        self.complete(
            class,
            channel.id(),
            text.len(),
            result.as_ref().err().map(|e| e.0),
        );
        result.map_err(|(_, error)| error)
    }
}

pub fn transport_diagnostics() -> &'static SendDiagnostics {
    static DIAGNOSTICS: OnceLock<SendDiagnostics> = OnceLock::new();
    DIAGNOSTICS.get_or_init(SendDiagnostics::default)
}

pub(super) async fn send_text_checked(
    channel: &std::sync::Arc<dyn DataChannel>,
    text: &str,
) -> Result<()> {
    // All current CTOX text channels are reliable (no lifetime/retransmit cap).
    transport_diagnostics()
        .send_text(channel.as_ref(), text, MessageClass::Reliable)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::BytesMut;
    use futures::{FutureExt, StreamExt};
    use webrtc::data_channel::{DataChannelEvent, RTCDataChannelId};

    struct Channel {
        open: bool,
        missing: bool,
    }
    #[async_trait]
    impl DataChannel for Channel {
        async fn label(&self) -> Result<String> {
            Ok("p4-test".into())
        }
        async fn ordered(&self) -> Result<bool> {
            Ok(true)
        }
        async fn max_packet_life_time(&self) -> Result<Option<u16>> {
            Ok(None)
        }
        async fn max_retransmits(&self) -> Result<Option<u16>> {
            Ok(None)
        }
        async fn protocol(&self) -> Result<String> {
            Ok(String::new())
        }
        async fn negotiated(&self) -> Result<bool> {
            Ok(false)
        }
        fn id(&self) -> RTCDataChannelId {
            7
        }
        async fn ready_state(&self) -> Result<RTCDataChannelState> {
            Ok(if self.open {
                RTCDataChannelState::Open
            } else {
                RTCDataChannelState::Connecting
            })
        }
        async fn buffered_amount_high_threshold(&self) -> Result<u32> {
            Ok(0)
        }
        async fn set_buffered_amount_high_threshold(&self, _: u32) -> Result<()> {
            Ok(())
        }
        async fn buffered_amount_low_threshold(&self) -> Result<u32> {
            Ok(0)
        }
        async fn set_buffered_amount_low_threshold(&self, _: u32) -> Result<()> {
            Ok(())
        }
        async fn send(&self, _: BytesMut) -> Result<()> {
            unreachable!()
        }
        async fn send_text(&self, _: &str) -> Result<()> {
            if self.missing {
                Err(Error::ErrStreamNotExisted)
            } else {
                Ok(())
            }
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            None
        }
        async fn close(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn p4_forced_send_errors_emit_exactly_one_event_each() {
        for (channel, text, expected) in [
            (
                Channel {
                    open: true,
                    missing: false,
                },
                "x".repeat(65_537),
                SendFailureKind::TooLarge,
            ),
            (
                Channel {
                    open: true,
                    missing: true,
                },
                "x".into(),
                SendFailureKind::StreamMissing,
            ),
            (
                Channel {
                    open: false,
                    missing: false,
                },
                "x".into(),
                SendFailureKind::NotReady,
            ),
        ] {
            let diagnostics = SendDiagnostics::default();
            let mut events = diagnostics.errors();
            assert!(diagnostics
                .send_text(&channel, &text, MessageClass::Reliable)
                .await
                .is_err());
            let event = events.next().now_or_never().flatten().expect("one event");
            assert_eq!(event.kind, expected);
            assert_eq!(event.bytes, text.len());
            assert!(events.next().now_or_never().is_none(), "duplicate event");
            let count = diagnostics.counters(MessageClass::Reliable);
            assert_eq!(count.attempted, 1);
            assert_eq!(count.handed_to_sctp, 0);
            assert_eq!(count.failed(), 1);
        }
    }

    #[tokio::test]
    async fn p4_class_counters_separate_reliable_handoff_and_receive() {
        let diagnostics = SendDiagnostics::default();
        let mut events = diagnostics.errors();
        let channel = Channel {
            open: true,
            missing: false,
        };
        // Exactly 64 KiB is legal; only larger physical messages are rejected.
        diagnostics
            .send_text(&channel, &"x".repeat(65_536), MessageClass::Reliable)
            .await
            .unwrap();
        diagnostics.received(MessageClass::Reliable);
        diagnostics
            .send_text(
                &Channel {
                    open: false,
                    missing: false,
                },
                "audio",
                MessageClass::Realtime,
            )
            .await
            .unwrap_err();
        let reliable = diagnostics.counters(MessageClass::Reliable);
        assert_eq!(
            reliable.attempted,
            reliable.handed_to_sctp + reliable.failed()
        );
        assert_eq!(reliable.handed_to_sctp, reliable.received);
        assert_eq!(reliable.failed(), 0);
        assert_eq!(diagnostics.counters(MessageClass::Realtime).not_ready, 1);
        assert_eq!(
            events.next().now_or_never().flatten().unwrap().class,
            MessageClass::Realtime
        );
        assert!(events.next().now_or_never().is_none());
    }
}
