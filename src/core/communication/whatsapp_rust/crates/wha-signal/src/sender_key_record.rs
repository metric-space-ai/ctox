//! Sender-key record: the per-`(group, sender)` slot in a sender-key store.
//!
//! Mirrors `go.mau.fi/libsignal/groups/state/record/SenderKeyRecord`. A
//! record holds up to 5 [`SenderKeyState`]s — one per "generation" — with
//! the newest at index 0. Holding multiple states lets us still decrypt
//! in-flight messages for a generation that's about to be replaced (e.g.
//! when a peer rotates their sender key).

use crate::sender_key::SenderKeyState;

/// libsignal's per-record cap on retained generations.
pub const MAX_STATES: usize = 5;

/// Per-`(group, sender)` record. Newest state at index 0; older ones tail
/// off and are evicted when a sixth state is added.
#[derive(Debug, Clone, Default)]
pub struct SenderKeyRecord {
    /// Newest first.
    states: Vec<SenderKeyState>,
}

impl SenderKeyRecord {
    pub fn new() -> Self {
        Self { states: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Return the newest state, if any.
    pub fn sender_key_state(&self) -> Option<&SenderKeyState> {
        self.states.first()
    }

    /// Mutable access to the newest state, if any.
    pub fn sender_key_state_mut(&mut self) -> Option<&mut SenderKeyState> {
        self.states.first_mut()
    }

    /// Find a state by its `key_id`.
    pub fn get_sender_key_state(&self, key_id: u32) -> Option<&SenderKeyState> {
        self.states.iter().find(|s| s.key_id == key_id)
    }

    /// Find a state by its `key_id` (mutable).
    pub fn get_sender_key_state_mut(&mut self, key_id: u32) -> Option<&mut SenderKeyState> {
        self.states.iter_mut().find(|s| s.key_id == key_id)
    }

    /// Insert a new state at the front. Drops the oldest if we exceed
    /// [`MAX_STATES`].
    pub fn add_sender_key_state(&mut self, state: SenderKeyState) {
        self.states.insert(0, state);
        if self.states.len() > MAX_STATES {
            self.states.truncate(MAX_STATES);
        }
    }

    /// Replace the entire state set with a single state. libsignal calls
    /// this `SetSenderKeyState`, used when *this* peer creates a fresh
    /// sender key for a group.
    pub fn set_sender_key_state(&mut self, state: SenderKeyState) {
        self.states.clear();
        self.states.push(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender_key::SenderChainKey;

    fn dummy_state(id: u32) -> SenderKeyState {
        SenderKeyState {
            key_id: id,
            chain_key: SenderChainKey::new(0, [id as u8; 32]),
            signing_key_public: [0u8; 32],
            signing_key_private: None,
            skipped_message_keys: Default::default(),
            skipped_order: Vec::new(),
        }
    }

    #[test]
    fn newest_first_with_eviction() {
        let mut rec = SenderKeyRecord::new();
        for i in 0..7u32 {
            rec.add_sender_key_state(dummy_state(i));
        }
        assert_eq!(rec.len(), MAX_STATES);
        // Newest should be id=6.
        assert_eq!(rec.sender_key_state().unwrap().key_id, 6);
        // Oldest retained should be 6 - (MAX_STATES-1) = 2.
        assert!(rec.get_sender_key_state(2).is_some());
        // Evicted older ones gone.
        assert!(rec.get_sender_key_state(0).is_none());
        assert!(rec.get_sender_key_state(1).is_none());
    }

    #[test]
    fn lookup_by_key_id() {
        let mut rec = SenderKeyRecord::new();
        rec.add_sender_key_state(dummy_state(42));
        rec.add_sender_key_state(dummy_state(7));
        assert_eq!(rec.get_sender_key_state(42).unwrap().key_id, 42);
        assert_eq!(rec.get_sender_key_state(7).unwrap().key_id, 7);
        assert!(rec.get_sender_key_state(99).is_none());
    }

    #[test]
    fn set_replaces_all() {
        let mut rec = SenderKeyRecord::new();
        rec.add_sender_key_state(dummy_state(1));
        rec.add_sender_key_state(dummy_state(2));
        rec.set_sender_key_state(dummy_state(99));
        assert_eq!(rec.len(), 1);
        assert_eq!(rec.sender_key_state().unwrap().key_id, 99);
    }
}
