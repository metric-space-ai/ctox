//! Sender-only RACK/TLP adaptation for sparse, reliable high-priority streams.
//! Legacy DATA fragment ordering, congestion credit and the normal T3 remain intact.
use super::*;

#[derive(Default)]
pub(super) struct ControlLoss {
    delivered_sent: Option<Instant>,
    rtt: Option<Duration>,
    min_rtt: Option<Duration>,
    probe_at: Option<Instant>,
    probed: bool,
}
impl ControlLoss {
    pub(super) fn acknowledge(&mut self, sent: Option<Instant>, transmissions: u32, now: Instant) {
        self.probe_at = None;
        self.probed = false;
        // Karn: retransmissions cannot provide an unambiguous time witness.
        if transmissions != 1 {
            return;
        }
        if let Some(sent) = sent {
            let rtt = now.saturating_duration_since(sent);
            self.min_rtt = Some(self.min_rtt.map_or(rtt, |old| old.min(rtt)));
            if self.delivered_sent.is_none_or(|old| sent >= old) {
                self.delivered_sent = Some(sent);
                self.rtt = Some(rtt);
            }
        }
    }
}
impl Association {
    fn control_candidates(&self) -> impl Iterator<Item = &ChunkPayloadData> {
        self.inflight_queue
            .sorted
            .iter()
            .filter_map(|tsn| self.inflight_queue.get(*tsn))
            .filter(|c| {
                !c.acked
                    && !c.abandoned()
                    && !c.retransmit
                    && c.nsent == 1
                    && self.pending_queue.priority(c.stream_identifier) > 256
                    && self
                        .streams
                        .get(&c.stream_identifier)
                        .is_some_and(|s| s.reliability_type == ReliabilityType::Reliable)
            })
    }
    fn rack_deadline(&self, c: &ChunkPayloadData) -> Option<Instant> {
        let sent = c.since?;
        let delivered = self.control_loss.delivered_sent?;
        let rtt = self.control_loss.rtt?;
        let reorder = self.control_loss.min_rtt? / 4;
        // ref: pion/sctp@af90d496d57ae47de0663a1d9cd2dd6ece3844a6/association.go:5028-5069.
        // A later original transmission must have been delivered, allowing a
        // min-RTT/4 reordering window; never infer loss from TSN distance alone.
        (sent + reorder < delivered).then_some(sent + rtt + reorder)
    }
    pub(super) fn control_loss_deadline(&self) -> Option<Instant> {
        let rack = self
            .control_candidates()
            .filter_map(|c| self.rack_deadline(c))
            .min();
        rack.into_iter().chain(self.control_loss.probe_at).min()
    }
    pub(super) fn arm_control_probe(&mut self, now: Instant) {
        if self.control_candidates().next().is_none() {
            self.control_loss.probe_at = None;
            return;
        }
        if self.control_loss.probed || self.control_loss.probe_at.is_some() {
            return;
        }
        // ref: pion/sctp@af90d496d57ae47de0663a1d9cd2dd6ece3844a6/association.go:4997-5020.
        // Keep the conservative single-flight delayed-ACK allowance. One probe
        // per ACK-progress epoch; no idle probes or retransmission timer storm.
        let pto = if self.rto_mgr.srtt > 0 {
            Duration::from_millis(self.rto_mgr.srtt.saturating_mul(2))
                + Duration::from_millis(if self.inflight_queue.len() == 1 {
                    200
                } else {
                    2
                })
        } else {
            Duration::from_secs(1)
        };
        self.control_loss.probe_at = Some(now + pto);
    }
    pub(super) fn recover_control_loss(&mut self, now: Instant) {
        let lost: Vec<_> = self
            .control_candidates()
            .filter(|c| self.rack_deadline(c).is_some_and(|at| at <= now))
            .map(|c| c.tsn)
            .collect();
        if !lost.is_empty() && !self.in_fast_recovery {
            // Use SCTP's existing congestion response, not extra probe credit.
            // ref: webrtc-rs/rtc@b808b74f712ed379312a114b848ede133880d58a/rtc-sctp/src/association/mod.rs:1809-1823.
            self.in_fast_recovery = true;
            self.fast_recover_exit_point = self.my_next_tsn.wrapping_sub(1);
            self.ssthresh = (self.cwnd / 2).max(4 * self.mtu);
            self.cwnd = self.ssthresh;
            self.partial_bytes_acked = 0;
        }
        for tsn in lost {
            self.inflight_queue.get_mut(tsn).unwrap().retransmit = true;
            trace!(target: "ctox_sctp_timeline", "event=rack_loss side={} tsn={}", self.side, tsn);
        }
        if self.control_loss.probe_at.is_some_and(|at| at <= now) {
            self.control_loss.probe_at = None;
            self.control_loss.probed = true;
            // ref: pion/sctp@af90d496d57ae47de0663a1d9cd2dd6ece3844a6/association.go:5080-5128.
            // Restrict tail probes to the sparse control class. The normal
            // retransmit packetizer still enforces cwnd/rwnd and path MTU.
            if let Some(tsn) = self.control_candidates().last().map(|c| c.tsn) {
                self.inflight_queue.get_mut(tsn).unwrap().retransmit = true;
                trace!(target: "ctox_sctp_timeline", "event=tail_probe side={} tsn={}", self.side, tsn);
            }
        }
    }
}

#[cfg(test)]
mod ctox_control_loss_contract {
    use super::*;
    fn association(now: Instant) -> Association {
        let mut a = Association {
            cwnd: 12000,
            rwnd: 12000,
            mtu: 1200,
            my_next_tsn: 3,
            ..Default::default()
        };
        a.pending_queue.set_priority(1, 1024);
        a.streams.insert(
            1,
            StreamState::new(Side::Client, 1, 1200, PayloadProtocolIdentifier::Binary),
        );
        a.inflight_queue.push_no_check(ChunkPayloadData {
            tsn: 1,
            stream_identifier: 1,
            since: Some(now),
            nsent: 1,
            beginning_fragment: true,
            ending_fragment: true,
            user_data: Bytes::from_static(b"control"),
            ..Default::default()
        });
        a
    }
    #[test]
    fn later_original_ack_recovers_control_before_t3_without_extra_credit() {
        let now = Instant::now();
        let mut a = association(now);
        a.control_loss.acknowledge(
            Some(now + Duration::from_millis(20)),
            1,
            now + Duration::from_millis(50),
        );
        a.recover_control_loss(now + Duration::from_millis(50));
        assert!(
            a.inflight_queue.get(1).unwrap().retransmit,
            "RACK must recover a witnessed control loss"
        );
        assert!(a.cwnd <= 12000);
        assert_eq!(a.get_data_packets_to_retransmit(now).len(), 1);
        assert_eq!(
            a.inflight_queue.get(1).unwrap().since,
            Some(now),
            "partial reliability origin must stay intact"
        );
        a.recover_control_loss(now + Duration::from_secs(1));
        assert!(
            !a.inflight_queue.get(1).unwrap().retransmit,
            "RACK must not re-probe a retransmission"
        );
    }
    #[test]
    fn sparse_tail_probes_once_and_ignores_ambiguous_ack_witnesses() {
        let now = Instant::now();
        let mut a = association(now);
        a.control_loss
            .acknowledge(Some(now), 2, now + Duration::from_millis(20));
        assert!(a.control_loss.delivered_sent.is_none());
        a.arm_control_probe(now);
        let deadline = a.control_loss_deadline().unwrap();
        a.recover_control_loss(deadline - Duration::from_millis(1));
        assert!(!a.inflight_queue.get(1).unwrap().retransmit);
        a.recover_control_loss(deadline);
        assert!(
            a.inflight_queue.get(1).unwrap().retransmit,
            "PTO must recover a sparse control tail"
        );
        a.get_data_packets_to_retransmit(deadline);
        a.arm_control_probe(deadline);
        assert!(
            a.control_loss_deadline().is_none(),
            "no unbounded probe loop without ACK progress"
        );
    }
}
