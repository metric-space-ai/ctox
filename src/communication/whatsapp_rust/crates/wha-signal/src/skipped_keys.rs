//! Skipped-message-key cache.
//!
//! Out-of-order delivery in the Double Ratchet requires caching message
//! keys we derived but didn't consume. Mirrors libsignal's behaviour in
//! `session/SessionCipher.go::getOrCreateMessageKeys`:
//!
//! * If the incoming counter is **less** than the chain's current index
//!   the message is either a duplicate or arrived after we already
//!   advanced past it; the caller checks the cache (via [`pop`]) and
//!   otherwise treats the message as a duplicate.
//! * If the incoming counter is **equal** to the chain's index the
//!   chain is already aligned and no skipping is needed.
//! * If the incoming counter is **ahead** of the chain by no more than
//!   [`MAX_SKIP`] entries the chain is walked forward, caching each
//!   intermediate `(peer_ratchet_pub, counter) -> MessageKeys` so that
//!   a later out-of-order message can still be decrypted.
//! * Anything further into the future is rejected as
//!   `TooFarIntoFuture`, matching libsignal's per-chain cap.
//!
//! The cache is bounded globally (across multiple ratchet chains) at
//! [`MAX_SKIP`] × 4 entries; FIFO eviction kicks in once that limit is
//! exceeded.
//!
//! Sources:
//!   * `state/record/SessionState.go` — `SetMessageKeys` / `RemoveMessageKeys`
//!   * `session/SessionCipher.go`     — `getOrCreateMessageKeys`

use std::collections::HashMap;

use crate::chain_key::{ChainKey, MessageKeys};
use crate::SignalProtocolError;

/// libsignal's per-chain cap on the number of message keys we are
/// willing to skip ahead before refusing a message as too-far-into-future.
pub const MAX_SKIP: u32 = 1000;

/// Global cap on the total number of cached skipped keys across every
/// peer ratchet chain. Beyond this limit the oldest entries are evicted
/// in FIFO order. libsignal enforces the per-chain cap; the global cap
/// is a defensive bound so a long-lived session can't accumulate
/// unbounded state if many ratchet chains have stale skipped keys.
const GLOBAL_CAP: usize = (MAX_SKIP as usize) * 4;

#[derive(Debug, Default, Clone)]
pub struct SkippedKeyCache {
    pub entries: HashMap<(/* peer ratchet pub */ [u8; 32], /* counter */ u32), MessageKeys>,
    insertion_order: Vec<([u8; 32], u32)>,
}

impl SkippedKeyCache {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), insertion_order: Vec::new() }
    }

    /// Walk `chain` from its current index up to (but not including)
    /// `target_counter`, caching each derived [`MessageKeys`] under
    /// `(peer_ratchet_pub, counter)` so that a later out-of-order
    /// message can still be decrypted.
    ///
    /// Mirrors libsignal's `getOrCreateMessageKeys` skip loop. The
    /// caller is responsible for deriving the keys at `target_counter`
    /// itself (via `chain.message_keys()` after this returns) and for
    /// then advancing the chain past the consumed counter.
    pub fn advance_caching(
        &mut self,
        peer_ratchet_pub: [u8; 32],
        chain: &mut ChainKey,
        target_counter: u32,
    ) -> Result<(), SignalProtocolError> {
        if target_counter < chain.index {
            return Err(SignalProtocolError::DuplicateMessage {
                chain: chain.index,
                counter: target_counter,
            });
        }
        if target_counter == chain.index {
            return Ok(());
        }
        // target_counter > chain.index — safe to subtract.
        if target_counter - chain.index > MAX_SKIP {
            return Err(SignalProtocolError::TooFarIntoFuture);
        }

        while chain.index < target_counter {
            let mk = chain.message_keys();
            self.insert(peer_ratchet_pub, chain.index, mk);
            *chain = chain.next();
        }
        Ok(())
    }

    /// Remove and return the cached message keys for `(peer_ratchet_pub, counter)`,
    /// if present. Used by the receiver to satisfy out-of-order
    /// messages whose keys were derived earlier and stashed here.
    pub fn pop(&mut self, peer_ratchet_pub: [u8; 32], counter: u32) -> Option<MessageKeys> {
        let k = (peer_ratchet_pub, counter);
        let v = self.entries.remove(&k)?;
        if let Some(p) = self.insertion_order.iter().position(|x| *x == k) {
            self.insertion_order.remove(p);
        }
        Some(v)
    }

    /// Insert a keyed entry, replacing any prior entry for the same
    /// `(peer_ratchet_pub, counter)` and FIFO-evicting the oldest
    /// entries if the global cap is exceeded.
    fn insert(&mut self, peer_ratchet_pub: [u8; 32], counter: u32, keys: MessageKeys) {
        let k = (peer_ratchet_pub, counter);
        // If the slot already has an entry, drop its position from the
        // FIFO list so we re-record it at the tail (mirrors a fresh
        // insertion).
        if self.entries.insert(k, keys).is_some() {
            if let Some(p) = self.insertion_order.iter().position(|x| *x == k) {
                self.insertion_order.remove(p);
            }
        }
        self.insertion_order.push(k);

        while self.entries.len() > GLOBAL_CAP {
            // Pop oldest. `insertion_order` is non-empty because
            // entries.len() > GLOBAL_CAP > 0.
            let oldest = self.insertion_order.remove(0);
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cache_pop_returns_none() {
        let mut c = SkippedKeyCache::new();
        assert!(c.pop([0u8; 32], 0).is_none());
    }

    #[test]
    fn advance_to_same_counter_no_op() {
        let mut c = SkippedKeyCache::new();
        let mut ck = ChainKey::new([1u8; 32], 7);
        let before = ck.clone();
        c.advance_caching([9u8; 32], &mut ck, 7).expect("no-op should succeed");
        assert_eq!(ck, before);
        assert!(c.entries.is_empty());
    }

    #[test]
    fn advance_caches_intermediate_keys() {
        let mut c = SkippedKeyCache::new();
        let mut ck = ChainKey::new([2u8; 32], 0);
        let peer = [3u8; 32];

        // Advance to 5: entries 0..=4 should be cached, chain ends at index 5.
        c.advance_caching(peer, &mut ck, 5).expect("within MAX_SKIP");
        assert_eq!(ck.index, 5);
        assert_eq!(c.entries.len(), 5);

        for counter in 0..5u32 {
            let mk = c.pop(peer, counter).expect("counter must be cached");
            assert_eq!(mk.index, counter);
        }
        // 5 was the target, never cached — caller derives it directly.
        assert!(c.pop(peer, 5).is_none());
        // Cache emptied by the pops.
        assert!(c.entries.is_empty());
        assert!(c.insertion_order.is_empty());
    }

    #[test]
    fn advance_past_max_skip_returns_error() {
        let mut c = SkippedKeyCache::new();
        let mut ck = ChainKey::new([4u8; 32], 0);
        let target = MAX_SKIP + 1;
        let err = c
            .advance_caching([5u8; 32], &mut ck, target)
            .expect_err("MAX_SKIP+1 must fail");
        assert!(matches!(err, SignalProtocolError::TooFarIntoFuture));
        // Chain unchanged.
        assert_eq!(ck.index, 0);
        assert!(c.entries.is_empty());
    }

    #[test]
    fn advance_to_past_counter_returns_duplicate() {
        let mut c = SkippedKeyCache::new();
        let mut ck = ChainKey::new([6u8; 32], 10);
        let err = c
            .advance_caching([7u8; 32], &mut ck, 4)
            .expect_err("past counter must fail");
        match err {
            SignalProtocolError::DuplicateMessage { chain, counter } => {
                assert_eq!(chain, 10);
                assert_eq!(counter, 4);
            }
            other => panic!("expected DuplicateMessage, got {other:?}"),
        }
        assert_eq!(ck.index, 10);
    }

    #[test]
    fn advance_at_max_skip_boundary_succeeds() {
        let mut c = SkippedKeyCache::new();
        let mut ck = ChainKey::new([8u8; 32], 0);
        c.advance_caching([9u8; 32], &mut ck, MAX_SKIP)
            .expect("exactly MAX_SKIP must succeed");
        assert_eq!(ck.index, MAX_SKIP);
        assert_eq!(c.entries.len(), MAX_SKIP as usize);
    }

    #[test]
    fn fifo_eviction_when_global_cap_exceeded() {
        // Drive the cache past GLOBAL_CAP across multiple chains and
        // confirm the earliest entries are gone while the most recent
        // ones survive.
        let mut c = SkippedKeyCache::new();
        let chains = (GLOBAL_CAP / (MAX_SKIP as usize)) + 1; // > GLOBAL_CAP / MAX_SKIP
        for i in 0..chains {
            let peer = [i as u8; 32];
            let mut ck = ChainKey::new([(i as u8).wrapping_add(1); 32], 0);
            c.advance_caching(peer, &mut ck, MAX_SKIP)
                .expect("each chain within MAX_SKIP");
        }
        assert!(c.entries.len() <= GLOBAL_CAP);
        // Earliest peer's index 0 must have been evicted.
        let first_peer = [0u8; 32];
        assert!(c.pop(first_peer, 0).is_none());
        // Latest peer's last cached counter (MAX_SKIP - 1) survives.
        let last_peer = [(chains - 1) as u8; 32];
        assert!(c.pop(last_peer, MAX_SKIP - 1).is_some());
    }
}
