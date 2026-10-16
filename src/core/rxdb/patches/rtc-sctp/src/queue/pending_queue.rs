use crate::chunk::chunk_payload_data::ChunkPayloadData;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) type PendingBaseQueue = VecDeque<ChunkPayloadData>;

/// Stream priority applies before TSN assignment, never by jumping congestion credit.
// ref: webrtc-rs/rtc@b808b74f712ed379312a114b848ede133880d58a/rtc-sctp/src/queue/pending_queue.rs:8-113
// RFC 8831 section 6.4 / RFC 8832 section 5: higher DCEP weights mean higher priority.
// Local strict service classes preserve the V4 control-before-bulk contract.
// RFC 4960 section 3.3.1: DATA fragments of one message need consecutive TSNs.
#[derive(Debug, Default)]
pub(crate) struct PendingQueue {
    unordered_queue: PendingBaseQueue,
    ordered_queue: PendingBaseQueue,
    queue_len: usize,
    n_bytes: usize,
    priorities: HashMap<u16, u16>,
    selected: Option<(bool, u16, u16)>,
}

impl PendingQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_priority(&mut self, stream: u16, priority: u16) {
        self.priorities.insert(stream, priority);
    }

    pub(crate) fn push(&mut self, c: ChunkPayloadData) {
        self.n_bytes += c.user_data.len();
        if c.unordered {
            self.unordered_queue.push_back(c);
        } else {
            self.ordered_queue.push_back(c);
        }
        self.queue_len += 1;
    }

    fn selection(&self) -> Option<(bool, usize)> {
        if let Some((unordered, sid, ssn)) = self.selected {
            let queue = if unordered {
                &self.unordered_queue
            } else {
                &self.ordered_queue
            };
            return queue
                .iter()
                .position(|c| c.stream_identifier == sid && c.stream_sequence_number == ssn)
                .map(|index| (unordered, index));
        }
        let mut best = None;
        let mut best_priority = 0;
        // Equal priorities retain the upstream unordered-first/FIFO policy.
        for (unordered, queue) in [(true, &self.unordered_queue), (false, &self.ordered_queue)] {
            let mut seen = HashSet::new();
            for (index, c) in queue.iter().enumerate() {
                // Only the oldest message per stream is eligible (ordered SSN).
                if !seen.insert(c.stream_identifier) || !c.beginning_fragment {
                    continue;
                }
                let priority = self
                    .priorities
                    .get(&c.stream_identifier)
                    .copied()
                    .unwrap_or(256);
                if best.is_none() || priority > best_priority {
                    best = Some((unordered, index));
                    best_priority = priority;
                }
            }
        }
        best
    }

    pub(crate) fn peek(&self) -> Option<&ChunkPayloadData> {
        let (unordered, index) = self.selection()?;
        if unordered {
            self.unordered_queue.get(index)
        } else {
            self.ordered_queue.get(index)
        }
    }

    pub(crate) fn pop(
        &mut self,
        beginning_fragment: bool,
        unordered: bool,
    ) -> Option<ChunkPayloadData> {
        let (chosen_unordered, index) = self.selection()?;
        if chosen_unordered != unordered || (self.selected.is_none() && !beginning_fragment) {
            return None;
        }
        let popped = if unordered {
            self.unordered_queue.remove(index)?
        } else {
            self.ordered_queue.remove(index)?
        };
        self.selected = if popped.ending_fragment {
            None
        } else {
            Some((
                unordered,
                popped.stream_identifier,
                popped.stream_sequence_number,
            ))
        };
        self.n_bytes -= popped.user_data.len();
        self.queue_len -= 1;
        Some(popped)
    }

    pub(crate) fn get_num_bytes(&self) -> usize {
        self.n_bytes
    }
    pub(crate) fn len(&self) -> usize {
        self.queue_len
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod ctox_priority_contract {
    use super::*;
    use bytes::Bytes;
    fn chunk(sid: u16, ssn: u16, begin: bool, end: bool) -> ChunkPayloadData {
        ChunkPayloadData {
            stream_identifier: sid,
            stream_sequence_number: ssn,
            beginning_fragment: begin,
            ending_fragment: end,
            user_data: Bytes::from_static(b"payload"),
            ..Default::default()
        }
    }
    fn take(q: &mut PendingQueue) -> ChunkPayloadData {
        let c = q.peek().unwrap();
        let args = (c.beginning_fragment, c.unordered);
        q.pop(args.0, args.1).unwrap()
    }
    #[test]
    fn control_preempts_pending_bulk_without_reordering_or_splitting_a_message() {
        let mut q = PendingQueue::new();
        q.set_priority(17, 1024);
        q.set_priority(29, 512);
        for ssn in 0..2 {
            q.push(chunk(7, ssn, true, false));
            q.push(chunk(7, ssn, false, true));
        }
        q.push(chunk(29, 0, true, true));
        q.push(chunk(17, 0, true, true));
        q.push(chunk(17, 1, true, true));
        assert_eq!(
            take(&mut q).stream_identifier,
            17,
            "control must preempt pending bulk DATA"
        );
        assert_eq!(take(&mut q).stream_sequence_number, 1);
        assert_eq!(take(&mut q).stream_identifier, 29);
        assert!(take(&mut q).beginning_fragment);
        q.push(chunk(17, 2, true, true));
        assert!(
            take(&mut q).ending_fragment,
            "do not interleave DATA fragments (RFC4960)"
        );
        assert_eq!(take(&mut q).stream_identifier, 17);
        assert_eq!(take(&mut q).stream_sequence_number, 1);
        assert_eq!(take(&mut q).stream_sequence_number, 1);
        assert!(q.is_empty());
        assert_eq!(q.get_num_bytes(), 0);
    }
}
