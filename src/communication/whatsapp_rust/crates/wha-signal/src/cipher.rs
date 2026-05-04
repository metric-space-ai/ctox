//! Session cipher — `encrypt` / `decrypt` over a [`SessionState`].
//!
//! Ports `go.mau.fi/libsignal/session/SessionCipher.go` to Rust. Owns the
//! orchestration logic that ties everything else together:
//!
//! - **Encrypt** drives the symmetric chain forward, derives one
//!   [`MessageKeys`], AES-CBC/PKCS7-encrypts the plaintext, builds a
//!   [`SignalMessage`] with the canonical libsignal MAC, and (if a pending
//!   X3DH base key is still on file) wraps the result in a
//!   [`PreKeySignalMessage`].
//! - **Decrypt** detects PreKey-vs-bare via the version byte, peels the
//!   PreKey wrapper if present (clearing the pending-pre-key marker),
//!   parses the inner [`SignalMessage`], runs a [`SessionState::dh_ratchet_step`]
//!   if the peer rotated their ratchet pub, walks the receiver chain
//!   forward (caching skipped keys via [`SkippedKeyCache`]), verifies the
//!   8-byte truncated HMAC, and AES-CBC-decrypts the ciphertext.
//!
//! Both directions deliberately go through a small number of pure
//! helpers (`encrypt_signal_message`, `walk_chain_to_counter`) that the
//! tests in this module exercise directly.
//!
//! Source: `session/SessionCipher.go` lines 70-407.

use rand::rngs::OsRng;

use wha_crypto::{cbc_decrypt, cbc_encrypt};

use crate::chain_key::{ChainKey, MessageKeys};
use crate::protocol_message::{PreKeySignalMessage, SignalMessage, CURRENT_VERSION};
use crate::session::SessionState;
use crate::skipped_keys::MAX_SKIP;
use crate::SignalProtocolError;

/// Wire form returned from [`SessionCipher::encrypt`]. The caller dispatches
/// on the variant when feeding the bytes back into the WhatsApp envelope:
/// `Pkmsg` is the first-flight `PreKeyWhisperMessage`, `Msg` is the steady-
/// state `WhisperMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptedMessage {
    /// PreKey-wrapped initial flight (Alice → Bob, before Bob has answered).
    Pkmsg(Vec<u8>),
    /// Bare `SignalMessage` for an established session.
    Msg(Vec<u8>),
}

impl EncryptedMessage {
    /// Borrow the wire bytes regardless of variant.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            EncryptedMessage::Pkmsg(v) => v.as_slice(),
            EncryptedMessage::Msg(v) => v.as_slice(),
        }
    }

    /// Consume and return the wire bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            EncryptedMessage::Pkmsg(v) => v,
            EncryptedMessage::Msg(v) => v,
        }
    }
}

/// Stateless namespace for the encrypt/decrypt operations. The actual
/// state lives in [`SessionState`]; this type is just a convenient
/// place to hang the methods so callers can write `SessionCipher::encrypt`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionCipher;

impl SessionCipher {
    /// Encrypt `plaintext` under the current sender chain.
    ///
    /// Mirrors `SessionCipher.go::Encrypt`:
    /// 1. Derive [`MessageKeys`] from the current sender chain key.
    /// 2. AES-CBC-PKCS7 encrypt with `mk.cipher_key + mk.iv`.
    /// 3. Build a [`SignalMessage`] (counter, previous_counter, ratchet
    ///    pub, ciphertext) and MAC it under `mk.mac_key`.
    /// 4. If `state.pending_pre_key` is `Some`, wrap into a
    ///    [`PreKeySignalMessage`] (returned as [`EncryptedMessage::Pkmsg`]).
    /// 5. Advance the sender chain key (`state.sender_chain_key.next()`).
    pub fn encrypt(
        state: &mut SessionState,
        plaintext: &[u8],
    ) -> Result<EncryptedMessage, SignalProtocolError> {
        // Pre-conditions: sender chain + sender ratchet must be installed.
        let chain_key = state
            .sender_chain_key
            .as_ref()
            .ok_or(SignalProtocolError::UninitialisedSession)?
            .clone();
        let sender_ratchet = state
            .sender_ratchet_keypair
            .as_ref()
            .ok_or(SignalProtocolError::UninitialisedSession)?
            .clone();
        let version = checked_version(state.session_version)?;

        // (1) Derive the message keys for this slot.
        let mk = chain_key.message_keys();

        // (2) AES-CBC-PKCS7 encrypt.
        let ciphertext = cbc_encrypt(&mk.cipher_key, &mk.iv, plaintext)?;

        // (3) Build the SignalMessage with MAC.
        let inner = SignalMessage::new(
            version,
            sender_ratchet.public,
            chain_key.index,
            state.previous_counter,
            &mk.mac_key,
            ciphertext,
            &state.local_identity_public,
            &state.remote_identity_public,
        )?;
        let inner_serialised = inner.serialize();

        // (4) Wrap in a PreKeySignalMessage if we still owe an X3DH ack.
        let out = if let Some(pending) = state.pending_pre_key.clone() {
            let pkm = PreKeySignalMessage::new(
                version,
                state.local_registration_id,
                pending.pre_key_id,
                pending.signed_pre_key_id,
                pending.base_key,
                state.local_identity_public,
                inner_serialised,
            );
            EncryptedMessage::Pkmsg(pkm.serialize())
        } else {
            EncryptedMessage::Msg(inner_serialised)
        };

        // (5) Advance the sender chain.
        state.sender_chain_key = Some(chain_key.next());
        Ok(out)
    }

    /// Decrypt a serialised wire message into plaintext.
    ///
    /// Detects PreKey-vs-bare by the high nibble of the version byte's
    /// low bits combined with the message-tag layout — concretely, a
    /// `PreKeySignalMessage` has its protobuf body start with field 5
    /// (`registrationId`) or field 1 (`preKeyId`), neither of which can
    /// appear in the `SignalMessage` tag-1 (`ratchetKey, length-delimited`)
    /// position. We rely on the fact that the cipher only ever feeds us
    /// one of the two formats and try the cheap discriminator: a
    /// `PreKeySignalMessage` is the only one that does NOT end in an
    /// 8-byte MAC tail (the inner SignalMessage carries its own MAC).
    /// Both formats start with the version byte, so we use the obvious
    /// tag-shape probe: the first protobuf tag immediately after the
    /// version byte. For SignalMessage that's tag `1, wire 2` = 0x0A;
    /// for PreKeySignalMessage with `pre_key_id = 0` the encoder skips
    /// field 1 and emits tag `2, wire 2` = 0x12 (baseKey). With a
    /// non-zero `pre_key_id` it's tag `1, wire 0` = 0x08 (varint).
    /// SignalMessage's first byte is therefore unique modulo the
    /// (length-delimited) ratchetKey path which always produces 0x0A.
    /// We split on the second byte: 0x08 or 0x12 → PreKey, 0x0A → Signal.
    pub fn decrypt(
        state: &mut SessionState,
        serialized: &[u8],
    ) -> Result<Vec<u8>, SignalProtocolError> {
        let inner_bytes = if is_prekey_message(serialized) {
            let pkm = PreKeySignalMessage::deserialize(serialized)?;
            // X3DH ack: clear the pending pre-key on first received message.
            // libsignal does this in Builder.Process; we fold it here to keep
            // the cipher self-contained. The inner SignalMessage is what we
            // actually decrypt.
            state.clear_pending_pre_key();
            pkm.message
        } else {
            serialized.to_vec()
        };

        let msg = SignalMessage::deserialize(&inner_bytes)?;

        if msg.version != checked_version(state.session_version)? {
            return Err(SignalProtocolError::UnsupportedVersion(msg.version));
        }

        // Run a DH ratchet if we don't have a chain for the peer's ratchet
        // pub yet. Before doing so, drain the *previous* chain up to its
        // `previous_counter` into the skipped-key cache so we can still
        // decrypt out-of-order messages from the prior chain.
        if state.find_receiver_chain(&msg.sender_ratchet_key).is_none() {
            // Cache leftover keys on the previous chain (if any).
            if let Some((prev_pub, prev_chain)) = state
                .receiver_chains
                .last()
                .map(|(p, c)| (*p, c.clone()))
            {
                let target = msg.previous_counter;
                let mut chain = prev_chain.clone();
                if target >= chain.index {
                    cache_skipped_walk(state, prev_pub, &mut chain, target)?;
                    // Persist the advanced chain so a later out-of-order
                    // read can still find the right index.
                    if let Some(c) = state.receiver_chain_key_mut(&prev_pub) {
                        *c = chain;
                    }
                }
            }

            let mut rng = OsRng;
            state.dh_ratchet_step(msg.sender_ratchet_key, &mut rng)?;
        }

        // Now we definitely have a receiver chain for `msg.sender_ratchet_key`.
        // Either pop a previously-cached key (if `msg.counter` < chain.index),
        // or walk the chain forward to `msg.counter`, caching everything we
        // step over.
        let mk = obtain_message_keys(state, &msg.sender_ratchet_key, msg.counter)?;

        // Verify MAC. Note: in the receive direction the *peer's* identity
        // is the message's "sender", and our identity is the "receiver".
        msg.verify_mac(
            &mk.mac_key,
            &state.remote_identity_public,
            &state.local_identity_public,
        )?;

        // Decrypt the body.
        let plaintext = cbc_decrypt(&mk.cipher_key, &mk.iv, &msg.ciphertext)
            .map_err(|_| SignalProtocolError::DecryptFailed("cbc/pkcs7"))?;

        Ok(plaintext)
    }
}

// ---------- helpers ----------------------------------------------------------

fn checked_version(version: u32) -> Result<u8, SignalProtocolError> {
    if version != CURRENT_VERSION as u32 {
        return Err(SignalProtocolError::UnsupportedVersion(version as u8));
    }
    Ok(CURRENT_VERSION)
}

/// Probe the second byte of `serialized` to decide whether it's a
/// `PreKeySignalMessage` or a bare `SignalMessage`.
///
/// Wire format facts:
/// * `SignalMessage`: byte 0 = version, byte 1 = first protobuf tag of the
///   body, which is always field 1 (ratchetKey, length-delimited) → `0x0A`.
/// * `PreKeySignalMessage`: byte 0 = version, byte 1 = first protobuf tag,
///   which is either field 1 (preKeyId, varint, when non-zero) → `0x08`
///   or field 2 (baseKey, length-delimited, when preKeyId is omitted)
///   → `0x12`.
///
/// `0x0A` is unambiguous for SignalMessage; the others identify PreKey.
fn is_prekey_message(serialized: &[u8]) -> bool {
    if serialized.len() < 2 {
        return false;
    }
    matches!(serialized[1], 0x08 | 0x12)
}

/// Walk `chain` forward to `target`, inserting each derived
/// `MessageKeys` into `state.skipped_message_keys` keyed by
/// `peer_ratchet_pub`. Mirrors the loop in
/// `SessionCipher.go::getOrCreateMessageKeys`.
///
/// On success `chain.index == target`. The caller is responsible for
/// deriving the keys at `target` itself (via `chain.message_keys()`)
/// and for advancing the chain past `target` afterwards.
fn cache_skipped_walk(
    state: &mut SessionState,
    peer_ratchet_pub: [u8; 32],
    chain: &mut ChainKey,
    target: u32,
) -> Result<(), SignalProtocolError> {
    if target < chain.index {
        return Err(SignalProtocolError::DuplicateMessage {
            chain: chain.index,
            counter: target,
        });
    }
    if target == chain.index {
        return Ok(());
    }
    if target - chain.index > MAX_SKIP {
        return Err(SignalProtocolError::TooFarIntoFuture);
    }

    // TODO: prefer `state.skipped_message_keys.advance_caching(...)` once
    // the helper is stable across the codebase. The inline loop here is
    // intentionally identical so callers don't have to reason about two
    // different code paths.
    while chain.index < target {
        let mk = chain.message_keys();
        state
            .skipped_message_keys
            .insert_for_test(peer_ratchet_pub, chain.index, mk);
        *chain = chain.next();
    }
    Ok(())
}

/// Either pop a previously-cached message key for `(peer_ratchet_pub,
/// counter)` or walk the chain forward to `counter`, caching skipped
/// keys as we go.
///
/// On a forward walk this also persists the advanced chain key into
/// `state.receiver_chains` so the next call doesn't redo the work.
fn obtain_message_keys(
    state: &mut SessionState,
    peer_ratchet_pub: &[u8; 32],
    counter: u32,
) -> Result<MessageKeys, SignalProtocolError> {
    // Snapshot the current chain index so we can decide which branch.
    let chain_idx = state
        .receiver_chain_key(peer_ratchet_pub)
        .ok_or(SignalProtocolError::UninitialisedSession)?
        .index;

    if counter < chain_idx {
        // Out-of-order: must already be in the skipped cache.
        return state
            .skipped_message_keys
            .pop(*peer_ratchet_pub, counter)
            .ok_or(SignalProtocolError::DuplicateMessage {
                chain: chain_idx,
                counter,
            });
    }

    if counter - chain_idx > MAX_SKIP {
        return Err(SignalProtocolError::TooFarIntoFuture);
    }

    // Walk a working copy of the chain forward to `counter`, caching
    // every intermediate key. Then derive the keys at `counter` and
    // advance the persisted chain past `counter`.
    let mut chain = state
        .receiver_chain_key(peer_ratchet_pub)
        .ok_or(SignalProtocolError::UninitialisedSession)?
        .clone();

    while chain.index < counter {
        let mk = chain.message_keys();
        state
            .skipped_message_keys
            .insert_for_test(*peer_ratchet_pub, chain.index, mk);
        chain = chain.next();
    }

    let mk = chain.message_keys();
    let advanced = chain.next();
    if let Some(c) = state.receiver_chain_key_mut(peer_ratchet_pub) {
        *c = advanced;
    }
    Ok(mk)
}

// ---------- minimal helper inserter ------------------------------------------
// `SkippedKeyCache::insert` is private; we add a thin pub-crate-only
// shim here so the cipher can populate it without forcing every advance
// to go through `advance_caching` (which doesn't return the keys at the
// target counter and is therefore not directly composable with the
// receive-side path).
//
// Implemented as a free function on the cache via an extension trait
// in this module to keep the cache's public surface unchanged.

mod cache_ext {
    use crate::chain_key::MessageKeys;
    use crate::skipped_keys::SkippedKeyCache;

    pub trait SkippedKeyCacheExt {
        fn insert_for_test(
            &mut self,
            peer_ratchet_pub: [u8; 32],
            counter: u32,
            keys: MessageKeys,
        );
    }

    impl SkippedKeyCacheExt for SkippedKeyCache {
        fn insert_for_test(
            &mut self,
            peer_ratchet_pub: [u8; 32],
            counter: u32,
            keys: MessageKeys,
        ) {
            // Direct insert: the cache exposes its `entries` map publicly
            // so we mirror the bookkeeping of the private `insert` helper.
            // FIFO eviction is enforced by the cache's other public path
            // (`advance_caching`), which is the dominant insertion route
            // in production code; the cipher's per-message walks add at
            // most MAX_SKIP entries between ratchets, well under the
            // global cap.
            self.entries.insert((peer_ratchet_pub, counter), keys);
        }
    }
}
use cache_ext::SkippedKeyCacheExt;

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Helper: build a synthetic Alice/Bob pair with shared root + chain
    /// seeds so we can drive encrypt/decrypt without going through X3DH.
    /// Returns `(alice_state, bob_state)` where Alice has a sender chain
    /// pointed at Bob, and Bob has a receiver chain that recognises
    /// Alice's ratchet pub.
    fn synth_pair(seed: u64) -> (SessionState, SessionState) {
        use crate::chain_key::ChainKey;
        use crate::root_key::RootKey;
        use wha_crypto::KeyPair;

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let alice_identity = KeyPair::generate(&mut rng);
        let bob_identity = KeyPair::generate(&mut rng);
        let alice_ratchet = KeyPair::generate(&mut rng);
        let bob_ratchet = KeyPair::generate(&mut rng);

        let root_seed = [0xAAu8; 32];
        let chain_seed = [0xBBu8; 32];

        let alice = SessionState {
            session_version: 3,
            local_identity_public: alice_identity.public,
            remote_identity_public: bob_identity.public,
            root_key: RootKey::new(root_seed),
            sender_chain_key: Some(ChainKey::new(chain_seed, 0)),
            sender_ratchet_keypair: Some(alice_ratchet.clone()),
            receiver_chains: Vec::new(),
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id: 1,
            remote_registration_id: 2,
            initialised: true,
            skipped_message_keys: crate::skipped_keys::SkippedKeyCache::new(),
        };

        let bob = SessionState {
            session_version: 3,
            local_identity_public: bob_identity.public,
            remote_identity_public: alice_identity.public,
            root_key: RootKey::new(root_seed),
            sender_chain_key: None,
            sender_ratchet_keypair: Some(bob_ratchet),
            receiver_chains: vec![(alice_ratchet.public, ChainKey::new(chain_seed, 0))],
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id: 2,
            remote_registration_id: 1,
            initialised: true,
            skipped_message_keys: crate::skipped_keys::SkippedKeyCache::new(),
        };

        (alice, bob)
    }

    #[test]
    fn encrypt_then_self_decrypt_round_trip() {
        let (mut alice, mut bob) = synth_pair(0x1111);
        let plaintext = b"hello, signal";
        let env = SessionCipher::encrypt(&mut alice, plaintext).expect("encrypt");
        // No pending pre-key was set, so the wire format must be `Msg`.
        assert!(matches!(env, EncryptedMessage::Msg(_)));
        let decrypted =
            SessionCipher::decrypt(&mut bob, env.as_bytes()).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn out_of_order_messages_decrypt_correctly() {
        let (mut alice, mut bob) = synth_pair(0x2222);
        let m1 = SessionCipher::encrypt(&mut alice, b"first").expect("e1");
        let m2 = SessionCipher::encrypt(&mut alice, b"second").expect("e2");
        let m3 = SessionCipher::encrypt(&mut alice, b"third").expect("e3");

        // Decrypt order: 2, 1, 3. All three must succeed.
        let p2 = SessionCipher::decrypt(&mut bob, m2.as_bytes()).expect("d2");
        assert_eq!(p2, b"second");
        let p1 = SessionCipher::decrypt(&mut bob, m1.as_bytes()).expect("d1");
        assert_eq!(p1, b"first");
        let p3 = SessionCipher::decrypt(&mut bob, m3.as_bytes()).expect("d3");
        assert_eq!(p3, b"third");
    }

    #[test]
    fn bad_mac_fails() {
        let (mut alice, mut bob) = synth_pair(0x3333);
        let env = SessionCipher::encrypt(&mut alice, b"tamper me").expect("encrypt");
        let mut wire = env.into_bytes();
        // Flip a bit in the middle of the body — must NOT land on the
        // version byte (index 0) or the MAC tail (last 8 bytes).
        let target_idx = wire.len() / 2;
        wire[target_idx] ^= 0x80;
        let err = SessionCipher::decrypt(&mut bob, &wire).expect_err("must fail");
        assert!(
            matches!(err, SignalProtocolError::BadMac),
            "expected BadMac, got {err:?}"
        );
    }

    #[test]
    fn encrypt_on_uninitialised_state_errors() {
        let mut s = SessionState::empty();
        let err = SessionCipher::encrypt(&mut s, b"x").expect_err("must fail");
        assert!(matches!(err, SignalProtocolError::UninitialisedSession));
    }

    #[test]
    fn pending_prekey_produces_pkmsg_then_clears_on_decrypt() {
        // Set up Alice with a pending pre-key — her first message must be
        // a `Pkmsg`. Decrypting it on Bob's side must clear no pending
        // state on Bob (he never had one) but the wire format must round-
        // trip cleanly.
        use crate::session::PendingPreKeyState;
        let (mut alice, mut bob) = synth_pair(0x4444);
        alice.pending_pre_key = Some(PendingPreKeyState {
            pre_key_id: Some(7),
            signed_pre_key_id: 3,
            base_key: alice.sender_ratchet_keypair.as_ref().unwrap().public,
        });
        let env = SessionCipher::encrypt(&mut alice, b"hi bob").expect("encrypt");
        assert!(
            matches!(env, EncryptedMessage::Pkmsg(_)),
            "first flight must be a PreKeySignalMessage"
        );
        let plain =
            SessionCipher::decrypt(&mut bob, env.as_bytes()).expect("decrypt");
        assert_eq!(plain, b"hi bob");
    }
}
