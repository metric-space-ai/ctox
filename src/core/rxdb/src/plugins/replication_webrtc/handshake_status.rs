//! Bounded native handshake evidence. Never retain SDP, ICE addresses, tokens or peer names.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use webrtc::peer_connection::RTCPeerConnectionState;

#[derive(Clone, Copy)]
pub(super) enum SignalKind {
    Offer,
    Answer,
    Candidate,
    Other,
}
impl SignalKind {
    pub(super) fn of(signal: &Value) -> Self {
        match signal.get("type").and_then(Value::as_str) {
            Some("offer") => Self::Offer,
            Some("answer") => Self::Answer,
            _ if signal.get("candidate").is_some() => Self::Candidate,
            _ => Self::Other,
        }
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HandshakeStatus {
    received_offers: u64,
    received_answers: u64,
    received_candidates: u64,
    sent_offers: u64,
    sent_answers: u64,
    sent_candidates: u64,
    signal_errors: u64,
    connected_events: u64,
    disconnected_events: u64,
    failed_events: u64,
    closed_events: u64,
    last_connection_state: Option<&'static str>,
    last_error_stage: Option<&'static str>,
}

impl HandshakeStatus {
    pub(super) fn received(&mut self, signal: &Value) {
        let counter = match signal.get("type").and_then(Value::as_str) {
            Some("offer") => Some(&mut self.received_offers),
            Some("answer") => Some(&mut self.received_answers),
            _ if signal.get("candidate").is_some() => Some(&mut self.received_candidates),
            _ => None,
        };
        if let Some(counter) = counter {
            *counter = counter.saturating_add(1);
        }
    }
    pub(super) fn sent(&mut self, signal: SignalKind) {
        let counter = match signal {
            SignalKind::Offer => Some(&mut self.sent_offers),
            SignalKind::Answer => Some(&mut self.sent_answers),
            SignalKind::Candidate => Some(&mut self.sent_candidates),
            _ => None,
        };
        if let Some(counter) = counter {
            *counter = counter.saturating_add(1);
        }
    }
    pub(super) fn signal_error(&mut self, stage: &'static str) {
        self.signal_errors = self.signal_errors.saturating_add(1);
        self.last_error_stage = Some(stage);
    }
    pub(super) fn connection(&mut self, state: RTCPeerConnectionState) {
        self.last_connection_state = Some(match state {
            RTCPeerConnectionState::New => "new",
            RTCPeerConnectionState::Connecting => "connecting",
            RTCPeerConnectionState::Connected => {
                self.connected_events = self.connected_events.saturating_add(1);
                "connected"
            }
            RTCPeerConnectionState::Disconnected => {
                self.disconnected_events = self.disconnected_events.saturating_add(1);
                "disconnected"
            }
            RTCPeerConnectionState::Failed => {
                self.failed_events = self.failed_events.saturating_add(1);
                "failed"
            }
            RTCPeerConnectionState::Closed => {
                self.closed_events = self.closed_events.saturating_add(1);
                "closed"
            }
            _ => "unspecified",
        });
    }
    pub(super) fn json(&self, registered_peer: Option<&str>) -> Value {
        let mut value = serde_json::to_value(self).expect("fixed handshake counters serialize");
        value["registeredSignalingPeerSha256"] = registered_peer
            .map(|peer| Value::String(format!("{:x}", Sha256::digest(peer.as_bytes()))))
            .unwrap_or(Value::Null);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_retains_only_counts_static_states_and_actual_registration_hash() {
        let secret = "sensitive-fixture-token-and-address";
        let mut status = HandshakeStatus::default();
        status.received(&serde_json::json!({"type":"offer","sdp":secret}));
        status.received(&serde_json::json!({"candidate":{"candidate":secret}}));
        status.sent(SignalKind::Answer);
        status.signal_error("apply-signal");
        status.connection(RTCPeerConnectionState::Failed);
        status.connection(RTCPeerConnectionState::Connected);
        let json = status.json(Some(secret));
        assert_eq!(json["receivedOffers"], 1);
        assert_eq!(json["receivedCandidates"], 1);
        assert_eq!(json["sentAnswers"], 1);
        assert_eq!(json["signalErrors"], 1);
        assert_eq!(json["failedEvents"], 1);
        assert_eq!(json["connectedEvents"], 1);
        assert_eq!(json["lastConnectionState"], "connected");
        assert_eq!(
            json["registeredSignalingPeerSha256"],
            format!("{:x}", Sha256::digest(secret.as_bytes()))
        );
        assert!(!json.to_string().contains(secret));
        assert!(status.json(None)["registeredSignalingPeerSha256"].is_null());
        assert!(json.to_string().len() < 1024);
    }
}
