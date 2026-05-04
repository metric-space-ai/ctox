//! X3DH initial key agreement.
//!
//! Ports `go.mau.fi/libsignal/ratchet/Ratchet.go` (`CalculateSenderSession` /
//! `CalculateReceiverSession`). Both sides build the same master secret —
//! `(32 x 0xFF) || DH1 || DH2 || DH3 [|| DH4]` — and then run HKDF-SHA256
//! with an all-zero salt and `info = "WhisperText"` to expand 64 bytes into
//! the first root key (32) + first chain key (32).
//!
//! The four DH ops correspond to libsignal's Ratchet.go pairings:
//!   * Sender side:
//!       DH1 = our_identity_priv  · their_signed_pre_key_pub
//!       DH2 = our_ephemeral_priv · their_identity_pub
//!       DH3 = our_ephemeral_priv · their_signed_pre_key_pub
//!       DH4 = our_ephemeral_priv · their_one_time_pre_key_pub  (optional)
//!   * Receiver side mirrors this — see `initiate_incoming` for the exact
//!     pairings.

use rand::rngs::OsRng;

use wha_crypto::{hkdf_sha256, KeyPair};

use crate::bundle::PreKeyBundle;
use crate::identity::IdentityKeyPair;
use crate::root_key::RootKey;
use crate::SignalProtocolError;

/// libsignal's discontinuity prefix: 32 bytes of 0xFF prepended to the IKM.
const DISCONTINUITY: [u8; 32] = [0xFFu8; 32];

/// HKDF salt for the X3DH expansion (libsignal passes `nil` here, which the
/// HKDF spec treats as a zeroed salt of `HashLen` bytes).
const SALT: [u8; 32] = [0u8; 32];

/// HKDF info string. Matches `kdf.DeriveSecrets(..., []byte("WhisperText"), ...)`.
const KDF_INFO: &[u8] = b"WhisperText";

/// 32 bytes of root key + 32 bytes of first chain key.
const DERIVED_SECRETS_SIZE: usize = 64;

/// Outcome of the sender-side X3DH: the freshly-derived root key + first
/// chain key seed, plus the ephemeral keypair we used (becomes the first
/// sender ratchet key in the Double Ratchet).
pub struct OutgoingX3dh {
    pub root: RootKey,
    pub first_chain_key: [u8; 32],
    pub our_ephemeral: KeyPair,
}

/// Sender-side initial agreement. Generates a fresh ephemeral X25519 keypair
/// and runs the four-DH X3DH against `bundle`. Mirrors libsignal's
/// `Ratchet.go::CalculateSenderSession`.
pub fn initiate_outgoing(
    our_identity: &IdentityKeyPair,
    bundle: &PreKeyBundle,
) -> Result<OutgoingX3dh, SignalProtocolError> {
    let our_ephemeral = KeyPair::generate(&mut OsRng);

    // DH1 = our_identity_priv · their_signed_pre_key_pub
    let dh1 = our_identity
        .key_pair
        .shared_secret(&bundle.signed_pre_key_public);
    // DH2 = our_ephemeral_priv · their_identity_pub
    let dh2 = our_ephemeral.shared_secret(&bundle.identity_key);
    // DH3 = our_ephemeral_priv · their_signed_pre_key_pub
    let dh3 = our_ephemeral.shared_secret(&bundle.signed_pre_key_public);

    let mut master = Vec::with_capacity(32 + 32 * 4);
    master.extend_from_slice(&DISCONTINUITY);
    master.extend_from_slice(&dh1);
    master.extend_from_slice(&dh2);
    master.extend_from_slice(&dh3);

    if let Some(ref otpk) = bundle.pre_key_public {
        // DH4 = our_ephemeral_priv · their_one_time_pre_key_pub
        let dh4 = our_ephemeral.shared_secret(otpk);
        master.extend_from_slice(&dh4);
    }

    let (root, first_chain_key) = expand_master_secret(&master)?;

    Ok(OutgoingX3dh { root, first_chain_key, our_ephemeral })
}

/// Outcome of the receiver-side X3DH.
pub struct IncomingX3dh {
    pub root: RootKey,
    pub first_chain_key: [u8; 32],
}

/// Receiver-side initial agreement. Mirror image of `initiate_outgoing`,
/// invoked when we receive a `PreKeySignalMessage`. Mirrors libsignal's
/// `Ratchet.go::CalculateReceiverSession`.
pub fn initiate_incoming(
    our_identity: &IdentityKeyPair,
    our_signed_pre_key: &KeyPair,
    our_one_time_pre_key: Option<&KeyPair>,
    their_identity_pub: &[u8; 32],
    their_base_key_pub: &[u8; 32],
) -> Result<IncomingX3dh, SignalProtocolError> {
    // DH1 = our_signed_pre_key_priv · their_identity_pub
    let dh1 = our_signed_pre_key.shared_secret(their_identity_pub);
    // DH2 = our_identity_priv · their_base_key_pub
    let dh2 = our_identity.key_pair.shared_secret(their_base_key_pub);
    // DH3 = our_signed_pre_key_priv · their_base_key_pub
    let dh3 = our_signed_pre_key.shared_secret(their_base_key_pub);

    let mut master = Vec::with_capacity(32 + 32 * 4);
    master.extend_from_slice(&DISCONTINUITY);
    master.extend_from_slice(&dh1);
    master.extend_from_slice(&dh2);
    master.extend_from_slice(&dh3);

    if let Some(otpk) = our_one_time_pre_key {
        // DH4 = our_one_time_pre_key_priv · their_base_key_pub
        let dh4 = otpk.shared_secret(their_base_key_pub);
        master.extend_from_slice(&dh4);
    }

    let (root, first_chain_key) = expand_master_secret(&master)?;

    Ok(IncomingX3dh { root, first_chain_key })
}

/// Run HKDF-SHA256(IKM=master, salt=zeroed[32], info="WhisperText", out=64)
/// and split into (root, first_chain_key).
fn expand_master_secret(master: &[u8]) -> Result<(RootKey, [u8; 32]), SignalProtocolError> {
    let derived = hkdf_sha256(master, &SALT, KDF_INFO, DERIVED_SECRETS_SIZE)?;
    let mut root = [0u8; 32];
    let mut chain = [0u8; 32];
    root.copy_from_slice(&derived[..32]);
    chain.copy_from_slice(&derived[32..]);
    Ok((RootKey::new(root), chain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Build a deterministic test bundle from bob's identity + signed pre-key
    /// (and optionally a one-time pre-key).
    fn make_bundle(
        bob_identity: &IdentityKeyPair,
        bob_signed_pre_key: &KeyPair,
        bob_one_time_pre_key: Option<&KeyPair>,
    ) -> PreKeyBundle {
        PreKeyBundle {
            registration_id: 1,
            device_id: 0,
            pre_key_id: bob_one_time_pre_key.map(|_| 42),
            pre_key_public: bob_one_time_pre_key.map(|kp| kp.public),
            signed_pre_key_id: 1,
            signed_pre_key_public: bob_signed_pre_key.public,
            signed_pre_key_signature: [0u8; 64],
            identity_key: bob_identity.public(),
        }
    }

    #[test]
    fn outgoing_then_incoming_converge() {
        // Alice runs the sender side against bob's bundle; bob then runs the
        // receiver side against alice's identity and ephemeral. Both must
        // derive byte-identical (root, first_chain_key) — that's the whole
        // point of X3DH.
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xABCD);
        let alice_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_signed_pre_key = KeyPair::generate(&mut rng);
        let bob_one_time_pre_key = KeyPair::generate(&mut rng);

        let bundle = make_bundle(
            &bob_identity,
            &bob_signed_pre_key,
            Some(&bob_one_time_pre_key),
        );

        let outgoing = initiate_outgoing(&alice_identity, &bundle)
            .expect("alice X3DH should succeed");

        let incoming = initiate_incoming(
            &bob_identity,
            &bob_signed_pre_key,
            Some(&bob_one_time_pre_key),
            &alice_identity.public(),
            &outgoing.our_ephemeral.public,
        )
        .expect("bob X3DH should succeed");

        assert_eq!(outgoing.root, incoming.root);
        assert_eq!(outgoing.first_chain_key, incoming.first_chain_key);
    }

    #[test]
    fn outgoing_then_incoming_converge_without_one_time_pre_key() {
        // Same as above, but the bundle has no one-time pre-key — DH4 is
        // skipped on both sides. Sanity check the optional branch.
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x1234);
        let alice_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_signed_pre_key = KeyPair::generate(&mut rng);

        let bundle = make_bundle(&bob_identity, &bob_signed_pre_key, None);

        let outgoing = initiate_outgoing(&alice_identity, &bundle)
            .expect("alice X3DH should succeed");

        let incoming = initiate_incoming(
            &bob_identity,
            &bob_signed_pre_key,
            None,
            &alice_identity.public(),
            &outgoing.our_ephemeral.public,
        )
        .expect("bob X3DH should succeed");

        assert_eq!(outgoing.root, incoming.root);
        assert_eq!(outgoing.first_chain_key, incoming.first_chain_key);
    }

    #[test]
    fn with_one_time_pre_key_changes_output() {
        // Holding alice's identity, alice's ephemeral, and bob's identity +
        // signed pre-key fixed, toggling the one-time pre-key on/off must
        // produce a different (root, chain). We force the same ephemeral on
        // both sides by running the receiver path with a known alice base
        // key — that lets us isolate the DH4 contribution.
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xDEAD_BEEF);
        let alice_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_signed_pre_key = KeyPair::generate(&mut rng);
        let bob_one_time_pre_key = KeyPair::generate(&mut rng);
        let alice_ephemeral = KeyPair::generate(&mut rng);

        // Receiver side without DH4.
        let no_otpk = initiate_incoming(
            &bob_identity,
            &bob_signed_pre_key,
            None,
            &alice_identity.public(),
            &alice_ephemeral.public,
        )
        .expect("X3DH without OTPK should succeed");

        // Receiver side WITH DH4 — same inputs otherwise.
        let with_otpk = initiate_incoming(
            &bob_identity,
            &bob_signed_pre_key,
            Some(&bob_one_time_pre_key),
            &alice_identity.public(),
            &alice_ephemeral.public,
        )
        .expect("X3DH with OTPK should succeed");

        assert_ne!(no_otpk.root, with_otpk.root);
        assert_ne!(no_otpk.first_chain_key, with_otpk.first_chain_key);
    }

    #[test]
    fn ephemeral_is_fresh_each_call() {
        // Two consecutive calls to initiate_outgoing with the same inputs
        // must produce different ephemerals (and therefore different roots),
        // proving we're actually pulling from OsRng, not a fixed seed.
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        let alice_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let bob_signed_pre_key = KeyPair::generate(&mut rng);
        let bundle = make_bundle(&bob_identity, &bob_signed_pre_key, None);

        let a = initiate_outgoing(&alice_identity, &bundle).expect("first");
        let b = initiate_outgoing(&alice_identity, &bundle).expect("second");

        assert_ne!(a.our_ephemeral.public, b.our_ephemeral.public);
        assert_ne!(a.root, b.root);
    }
}
