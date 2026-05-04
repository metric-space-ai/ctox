//! Double-Ratchet session state.
//!
//! Ported from `go.mau.fi/libsignal/state/record/SessionState.go` (the
//! per-conversation state container) and the orchestration in
//! `go.mau.fi/libsignal/session/Session.go` (the bits that decide whether
//! we're Alice — first flight, has a sender chain, has a pending pre-key —
//! or Bob — installs a receiver chain and waits to ratchet).
//!
//! What this module owns:
//! - The struct with root key, sender chain, sender ratchet keypair,
//!   receiver-chain ring, previous counter, pending pre-key, and identity /
//!   registration metadata.
//! - The two construction entry points (`initialize_as_alice`,
//!   `initialize_as_bob`) that drop a freshly-derived
//!   [`crate::x3dh::OutgoingX3dh`] / [`crate::x3dh::IncomingX3dh`] into the
//!   right slots so the very next encrypt/decrypt is well-formed.
//! - The DH-ratchet step (`dh_ratchet_step`): receiver-chain first, then
//!   rotate the sender ratchet, advance the root, and bound the
//!   receiver-chain ring at libsignal's `MAX_RECEIVER_CHAINS = 5`.
//!
//! What lives elsewhere:
//! - X3DH key agreement — [`crate::x3dh`].
//! - Symmetric chain advance / message-key derivation — [`crate::ChainKey`].
//! - Skipped-message-key cache — [`crate::skipped_keys`].
//! - Wire-format encode/decode + MAC — [`crate::protocol_message`].
//! - The `encrypt` / `decrypt` orchestration on top of this state —
//!   [`crate::SessionCipher`].

use crate::chain_key::ChainKey;
use crate::root_key::RootKey;
use crate::skipped_keys::SkippedKeyCache;
use crate::x3dh;
use crate::SignalProtocolError;
use wha_crypto::KeyPair;

/// libsignal's `maxReceiverChains` — once we install a sixth receiver chain
/// we drop the oldest. Out-of-order delivery for chains older than this is
/// no longer recoverable.
pub const MAX_RECEIVER_CHAINS: usize = 5;

/// Default protocol version libsignal advertises (`protocol.CurrentVersion`).
pub const DEFAULT_SESSION_VERSION: u32 = 3;

/// Pending state recorded after Alice runs X3DH but before Bob has answered:
/// every outgoing message during this window must go on the wire as a
/// `PreKeySignalMessage`, not a bare `SignalMessage`. Once Bob's first
/// message comes in we clear this via [`SessionState::clear_pending_pre_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPreKeyState {
    /// Optional one-time pre-key id Bob advertised in his bundle. `None`
    /// means we ran X3DH without a one-time pre-key (signed-only path).
    pub pre_key_id: Option<u32>,
    /// Bob's signed pre-key id we used.
    pub signed_pre_key_id: u32,
    /// Our X3DH base key public — the wire `BaseKey` field of the outgoing
    /// `PreKeySignalMessage`.
    pub base_key: [u8; 32],
}

/// Per-peer Double-Ratchet state. One of these lives per `(name, device_id)`
/// session, wrapped by the persistence layer.
///
/// Field order matches `state/record/SessionState.go`'s `State` struct so
/// reviewers can diff the two side-by-side.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// `protocol.CurrentVersion` — `3` for current libsignal.
    pub session_version: u32,
    /// Our long-term identity public key (32-byte X25519).
    pub local_identity_public: [u8; 32],
    /// Peer's long-term identity public key.
    pub remote_identity_public: [u8; 32],
    /// Current root key. Advanced on every DH ratchet step.
    pub root_key: RootKey,
    /// Sender chain key. `None` while we're Bob waiting on Alice's first
    /// flight; `Some` after we've ratcheted at least once.
    pub sender_chain_key: Option<ChainKey>,
    /// Our current sender ratchet keypair. Rotated on every DH ratchet
    /// step. `None` for a Bob-side state that hasn't ratcheted yet.
    pub sender_ratchet_keypair: Option<KeyPair>,
    /// Ring of `(peer_ratchet_pub, chain_key)` pairs covering the last
    /// [`MAX_RECEIVER_CHAINS`] chains we've seen. Newest at the back.
    pub receiver_chains: Vec<([u8; 32], ChainKey)>,
    /// Counter of the previous sender chain at the moment we last
    /// rotated. Sent on the wire as `PREVIOUS_COUNTER` so the peer can
    /// derive any skipped message keys it still needs.
    pub previous_counter: u32,
    /// X3DH first-flight state. `Some` on Alice while waiting for Bob's
    /// first response; `None` afterwards.
    pub pending_pre_key: Option<PendingPreKeyState>,
    /// Our local registration id (peer-assigned identifier we advertise).
    pub local_registration_id: u32,
    /// Peer's registration id, if known.
    pub remote_registration_id: u32,
    /// `true` iff this state has been initialised via one of the
    /// constructors. The cipher rejects uninitialised states with
    /// [`SignalProtocolError::UninitialisedSession`].
    pub initialised: bool,
    /// Cache of message keys derived for counters we skipped past while
    /// walking a receiver chain forward. Indexed by `(peer_ratchet_pub,
    /// counter)`. Populated by the cipher on every receive that walks a
    /// chain by more than one step; drained when an out-of-order
    /// (smaller-counter) message later arrives. See
    /// [`crate::skipped_keys::SkippedKeyCache`] for the eviction policy.
    pub skipped_message_keys: SkippedKeyCache,
}

impl SessionState {
    /// Construct an explicitly-uninitialised state. The cipher rejects this
    /// with [`SignalProtocolError::UninitialisedSession`] — exists so older
    /// callers (and the cipher's stub tests) can construct a placeholder
    /// without us needing a `Default` impl.
    pub fn empty() -> Self {
        Self {
            session_version: DEFAULT_SESSION_VERSION,
            local_identity_public: [0u8; 32],
            remote_identity_public: [0u8; 32],
            root_key: RootKey::new([0u8; 32]),
            sender_chain_key: None,
            sender_ratchet_keypair: None,
            receiver_chains: Vec::new(),
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id: 0,
            remote_registration_id: 0,
            initialised: false,
            skipped_message_keys: SkippedKeyCache::new(),
        }
    }

    /// Build the Alice-side state right after running X3DH against a peer's
    /// pre-key bundle.
    ///
    /// Mirrors `Session.go::ProcessBundle` lines 240-273:
    /// - sender ratchet keypair = the X3DH ephemeral (Go: `sendingRatchetKey`
    ///   when Bob's bundle has no one-time pre-key, otherwise the same
    ///   ephemeral that fed DH4),
    /// - root key = derived `RootKey` from X3DH,
    /// - sender chain = `ChainKey::new(first_chain_key, 0)`,
    /// - `pending_pre_key` populated so the next outgoing message is sent
    ///   as a `PreKeySignalMessage` (Go: `SetUnacknowledgedPreKeyMessage`).
    ///
    /// `signed_pre_key_id` and `pre_key_id` are taken straight from the
    /// `PreKeyBundle`; `base_key` is `our_ephemeral.public`.
    pub fn initialize_as_alice(
        our_identity_pub: [u8; 32],
        their_identity_pub: [u8; 32],
        x3dh: x3dh::OutgoingX3dh,
        signed_pre_key_id: u32,
        pre_key_id: Option<u32>,
        local_registration_id: u32,
        remote_registration_id: u32,
    ) -> Self {
        let base_key = x3dh.our_ephemeral.public;
        Self {
            session_version: DEFAULT_SESSION_VERSION,
            local_identity_public: our_identity_pub,
            remote_identity_public: their_identity_pub,
            root_key: x3dh.root,
            sender_chain_key: Some(ChainKey::new(x3dh.first_chain_key, 0)),
            sender_ratchet_keypair: Some(x3dh.our_ephemeral),
            receiver_chains: Vec::new(),
            previous_counter: 0,
            pending_pre_key: Some(PendingPreKeyState {
                pre_key_id,
                signed_pre_key_id,
                base_key,
            }),
            local_registration_id,
            remote_registration_id,
            initialised: true,
            skipped_message_keys: SkippedKeyCache::new(),
        }
    }

    /// Build the Bob-side state right after running X3DH on Alice's first
    /// `PreKeySignalMessage`.
    ///
    /// Mirrors `Session.go::processV3` lines 161-176 with one twist:
    /// libsignal sets the *sender* chain on Bob to (our signed pre-key,
    /// derived chain key). We mirror that exactly — Bob's sender ratchet
    /// keypair starts as `our_signed_pre_key`, and his sender chain key is
    /// the X3DH `first_chain_key`. Bob can therefore answer Alice without
    /// waiting for a DH ratchet step.
    ///
    /// In addition, libsignal records `their_base_key_pub` as the sender's
    /// "base key" — we install a *receiver* chain at that key so Alice's
    /// initial message (which advertises the same ratchet pub on the wire)
    /// can be looked up by [`Self::find_receiver_chain`].
    pub fn initialize_as_bob(
        our_identity_pub: [u8; 32],
        their_identity_pub: [u8; 32],
        x3dh: x3dh::IncomingX3dh,
        their_base_key_pub: [u8; 32],
        our_signed_pre_key: KeyPair,
        local_registration_id: u32,
        remote_registration_id: u32,
    ) -> Self {
        // Bob installs his signed-pre-key as the initial sender ratchet
        // keypair, mirroring `BobSessionStateBuilder.go::Process` in
        // go-libsignal. Without this, the very first DH-ratchet step
        // (when Alice sends her second message under a fresh ratchet pub)
        // fails with `UninitialisedSession`.
        let mut state = Self {
            session_version: DEFAULT_SESSION_VERSION,
            local_identity_public: our_identity_pub,
            remote_identity_public: their_identity_pub,
            root_key: x3dh.root,
            sender_chain_key: None,
            sender_ratchet_keypair: Some(our_signed_pre_key),
            receiver_chains: Vec::new(),
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id,
            remote_registration_id,
            initialised: true,
            skipped_message_keys: SkippedKeyCache::new(),
        };
        state.add_receiver_chain(their_base_key_pub, ChainKey::new(x3dh.first_chain_key, 0));
        state
    }

    /// Run the asymmetric ratchet step on the recipient's new ratchet pub.
    ///
    /// This is the byte-for-byte port of the receive-side block at the top
    /// of `SessionCipher.go::getOrCreateRatchetKey` plus the receive-rotate
    /// dance immediately after. The exact ordering matters for interop:
    ///
    /// 1. **Derive a receive chain** from the *current* sender ratchet
    ///    keypair × the peer's new ratchet pub. Push it onto the ring.
    /// 2. **Rotate**: generate a fresh sender ratchet keypair, then derive
    ///    a fresh sender chain from the *new* sender keypair × the peer's
    ///    new ratchet pub. The root we use for step 2 is the one
    ///    *advanced* by step 1.
    /// 3. Update `previous_counter` to whatever index the (now-stale)
    ///    sender chain reached.
    pub fn dh_ratchet_step<R: rand::RngCore>(
        &mut self,
        their_new_ratchet_pub: [u8; 32],
        rng: &mut R,
    ) -> Result<(), SignalProtocolError> {
        let our_old_ratchet = self
            .sender_ratchet_keypair
            .as_ref()
            .ok_or(SignalProtocolError::UninitialisedSession)?
            .clone();

        // Step 1: derive the receive chain off the *old* sender keypair.
        let (root_after_recv, recv_chain) =
            self.root_key.create_chain(&their_new_ratchet_pub, &our_old_ratchet);
        self.add_receiver_chain(their_new_ratchet_pub, recv_chain);

        // Step 2: rotate. Fresh sender ratchet, fresh sender chain off the
        // *new* sender keypair, root advances again.
        let new_sender = KeyPair::generate(rng);
        let (root_after_send, send_chain) =
            root_after_recv.create_chain(&their_new_ratchet_pub, &new_sender);

        // Stash the previous-counter before we drop the old chain.
        let previous_counter = self
            .sender_chain_key
            .as_ref()
            .map(|c| c.index)
            .unwrap_or(0);

        self.root_key = root_after_send;
        self.sender_ratchet_keypair = Some(new_sender);
        self.sender_chain_key = Some(send_chain);
        self.previous_counter = previous_counter;
        Ok(())
    }

    /// `true` if this state has a sender chain installed and can encrypt
    /// without ratcheting first. Bob right after `initialize_as_bob` does
    /// *not* satisfy this — he must wait for Alice's first message and
    /// `dh_ratchet_step` before he can encrypt.
    pub fn has_sender_chain(&self) -> bool {
        self.sender_chain_key.is_some() && self.sender_ratchet_keypair.is_some()
    }

    /// Index into [`Self::receiver_chains`] for the chain matching the
    /// supplied peer ratchet public, or `None` if no such chain is on the
    /// ring.
    pub fn find_receiver_chain(&self, peer_ratchet_pub: &[u8; 32]) -> Option<usize> {
        self.receiver_chains
            .iter()
            .position(|(pub_key, _)| pub_key == peer_ratchet_pub)
    }

    /// Lookup helper that returns the chain key itself.
    pub fn receiver_chain_key(&self, peer_ratchet_pub: &[u8; 32]) -> Option<&ChainKey> {
        self.find_receiver_chain(peer_ratchet_pub)
            .map(|i| &self.receiver_chains[i].1)
    }

    /// Mutable lookup helper used by the cipher when stepping a receive
    /// chain forward to a particular message counter.
    pub fn receiver_chain_key_mut(&mut self, peer_ratchet_pub: &[u8; 32]) -> Option<&mut ChainKey> {
        self.find_receiver_chain(peer_ratchet_pub)
            .map(move |i| &mut self.receiver_chains[i].1)
    }

    /// Append a chain to the ring, dropping the oldest if we'd exceed
    /// [`MAX_RECEIVER_CHAINS`].
    fn add_receiver_chain(&mut self, peer_ratchet_pub: [u8; 32], chain: ChainKey) {
        self.receiver_chains.push((peer_ratchet_pub, chain));
        if self.receiver_chains.len() > MAX_RECEIVER_CHAINS {
            // Drop the oldest entry. `remove(0)` is O(n) but n<=5, so the
            // arithmetic is dominated by everything else around it.
            self.receiver_chains.remove(0);
        }
    }

    /// Drop the pending-pre-key marker. Called by the cipher the first
    /// time we successfully decrypt a message from the peer (i.e. Bob has
    /// answered, so we no longer need to retransmit our X3DH base key).
    pub fn clear_pending_pre_key(&mut self) {
        self.pending_pre_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Build a synthetic `OutgoingX3dh` without going through the (still
    /// stubbed) `x3dh::initiate_outgoing`. Lets us exercise the state
    /// machine in isolation.
    fn fake_outgoing(rng: &mut impl rand::RngCore) -> x3dh::OutgoingX3dh {
        x3dh::OutgoingX3dh {
            root: RootKey::new([1u8; 32]),
            first_chain_key: [2u8; 32],
            our_ephemeral: KeyPair::generate(rng),
        }
    }

    fn fake_incoming() -> x3dh::IncomingX3dh {
        x3dh::IncomingX3dh {
            root: RootKey::new([3u8; 32]),
            first_chain_key: [4u8; 32],
        }
    }

    #[test]
    fn alice_state_has_sender_chain_and_pending_prekey() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(101);
        let our_id = KeyPair::generate(&mut rng);
        let their_id_pub = [9u8; 32];
        let x = fake_outgoing(&mut rng);
        let ephemeral_pub = x.our_ephemeral.public;
        let alice = SessionState::initialize_as_alice(
            our_id.public,
            their_id_pub,
            x,
            7,
            Some(42),
            123,
            456,
        );
        assert!(alice.initialised);
        assert!(alice.has_sender_chain());
        let pending = alice
            .pending_pre_key
            .as_ref()
            .expect("alice must record pending pre-key");
        assert_eq!(pending.pre_key_id, Some(42));
        assert_eq!(pending.signed_pre_key_id, 7);
        assert_eq!(pending.base_key, ephemeral_pub);
        assert_eq!(alice.local_registration_id, 123);
        assert_eq!(alice.remote_registration_id, 456);
        assert!(alice.receiver_chains.is_empty());
        assert_eq!(alice.session_version, DEFAULT_SESSION_VERSION);
    }

    #[test]
    fn bob_state_has_no_sender_chain_initially() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(202);
        let our_id = KeyPair::generate(&mut rng);
        let their_id_pub = [11u8; 32];
        let their_base_pub = [22u8; 32];
        let bob = SessionState::initialize_as_bob(
            our_id.public,
            their_id_pub,
            fake_incoming(),
            their_base_pub,
            KeyPair::generate(&mut rng),
            7,
            8,
        );
        assert!(bob.initialised);
        assert!(!bob.has_sender_chain(), "bob has no sender chain until first ratchet");
        assert!(bob.pending_pre_key.is_none());
        assert_eq!(bob.receiver_chains.len(), 1);
        assert_eq!(bob.receiver_chains[0].0, their_base_pub);
        assert_eq!(bob.receiver_chains[0].1.index, 0);
        assert_eq!(bob.find_receiver_chain(&their_base_pub), Some(0));
        assert_eq!(bob.find_receiver_chain(&[0u8; 32]), None);
    }

    #[test]
    fn dh_ratchet_step_advances_root_and_sender_chain() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(303);
        let our_id = KeyPair::generate(&mut rng);
        let their_id_pub = [33u8; 32];
        let mut alice = SessionState::initialize_as_alice(
            our_id.public,
            their_id_pub,
            fake_outgoing(&mut rng),
            1,
            None,
            10,
            20,
        );

        let original_root = alice.root_key.clone();
        let original_sender_pub = alice.sender_ratchet_keypair.as_ref().unwrap().public;
        let original_chain_key = alice.sender_chain_key.clone().unwrap();

        // Pretend we've sent five messages on the old chain, so the
        // previous-counter book-keeping has something interesting to
        // capture.
        let mut advanced = original_chain_key.clone();
        for _ in 0..5 {
            advanced = advanced.next();
        }
        alice.sender_chain_key = Some(advanced);

        // Bob's freshly-rotated ratchet pub.
        let bob_ratchet = KeyPair::generate(&mut rng);
        alice
            .dh_ratchet_step(bob_ratchet.public, &mut rng)
            .expect("ratchet step succeeds with a sender keypair installed");

        // Root key must have moved.
        assert_ne!(alice.root_key, original_root);
        // Sender ratchet keypair must have rotated.
        assert_ne!(
            alice.sender_ratchet_keypair.as_ref().unwrap().public,
            original_sender_pub
        );
        // Fresh sender chain at index 0.
        let new_chain = alice
            .sender_chain_key
            .as_ref()
            .expect("sender chain reinstalled after rotate");
        assert_eq!(new_chain.index, 0);
        assert_ne!(new_chain.key, original_chain_key.key);
        // Previous-counter captured from the old chain (5 advances).
        assert_eq!(alice.previous_counter, 5);
        // Receiver chain for Bob's ratchet was added.
        assert_eq!(alice.find_receiver_chain(&bob_ratchet.public), Some(0));
    }

    #[test]
    fn receiver_chain_ring_caps_at_5() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(404);
        let our_id = KeyPair::generate(&mut rng);
        let their_id_pub = [44u8; 32];
        let mut state = SessionState::initialize_as_alice(
            our_id.public,
            their_id_pub,
            fake_outgoing(&mut rng),
            1,
            None,
            0,
            0,
        );
        // Add MAX + 1 receiver chains via repeated DH ratchet steps. The
        // oldest must fall off the ring once we cross the cap.
        let mut peer_pubs = Vec::new();
        for _ in 0..(MAX_RECEIVER_CHAINS + 1) {
            let peer = KeyPair::generate(&mut rng);
            state.dh_ratchet_step(peer.public, &mut rng).unwrap();
            peer_pubs.push(peer.public);
        }
        assert_eq!(state.receiver_chains.len(), MAX_RECEIVER_CHAINS);
        // Newest stays.
        assert!(state.find_receiver_chain(&peer_pubs[MAX_RECEIVER_CHAINS]).is_some());
        // Oldest fell off.
        assert!(state.find_receiver_chain(&peer_pubs[0]).is_none());
    }

    #[test]
    fn clear_pending_pre_key_drops_marker() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(505);
        let our_id = KeyPair::generate(&mut rng);
        let mut alice = SessionState::initialize_as_alice(
            our_id.public,
            [55u8; 32],
            fake_outgoing(&mut rng),
            3,
            Some(13),
            0,
            0,
        );
        assert!(alice.pending_pre_key.is_some());
        alice.clear_pending_pre_key();
        assert!(alice.pending_pre_key.is_none());
    }

    #[test]
    fn empty_state_is_uninitialised_and_has_no_sender_chain() {
        let s = SessionState::empty();
        assert!(!s.initialised);
        assert!(!s.has_sender_chain());
        assert!(s.pending_pre_key.is_none());
        assert!(s.receiver_chains.is_empty());
    }
}
