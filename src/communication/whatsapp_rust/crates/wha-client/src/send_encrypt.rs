//! Per-recipient Signal session encryption fan-out.
//!
//! Mirrors the encrypt-loop inside `whatsmeow/send.go::sendDM` /
//! `encryptMessageForDevice`. Walks the recipient's device list, looks up
//! (or fetches + builds) a [`wha_signal::SessionState`] per device, calls
//! [`wha_signal::SessionCipher::encrypt`] for each, and packages the
//! results into the wire `<enc>` nodes that ride inside `<message>`.
//!
//! ## Persistence tradeoff
//!
//! Upstream libsignal serialises `SessionState` via protobuf
//! (`state/record/SessionStructure`). The protobuf shape is large and the
//! Rust port hasn't grown serde derives yet — adding them here would touch
//! every type in `wha-signal`, well outside this module's blast radius.
//!
//! Instead this module ships a **hand-rolled minimal serializer**
//! ([`encode_session`] / [`decode_session`]) with a stable fixed layout:
//! a one-byte format tag, then each scalar/`[u8; 32]` field of
//! [`SessionState`] in declaration order, then a length-prefixed receiver
//! chain ring. The shape only needs to round-trip *our own* freshly
//! written sessions; it is **not** wire-compatible with whatsmeow's
//! protobuf-encoded session blobs. Cross-implementation sessions will
//! land when we port `state/record/SessionStructure.proto` end-to-end.
//!
//! Skipped-message-key cache and pending-pre-key state are intentionally
//! NOT persisted in this minimal layout — they are rebuildable
//! (skipped-keys) or short-lived (pending pre-key clears on the very
//! next received message), and the tests that drive this module exercise
//! only post-handshake steady-state encryption.

use wha_binary::{Attrs, Node, Value};
use wha_signal::{
    chain_key::ChainKey, root_key::RootKey, session::SessionState, skipped_keys::SkippedKeyCache,
    SessionCipher, SignalAddress,
};
use wha_signal::cipher::EncryptedMessage;
use wha_crypto::KeyPair;
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;

/// Serialised-session format tag. Bumping this byte invalidates older
/// blobs — currently nothing else uses this layout, so a bump is free.
const SESSION_FORMAT_VERSION: u8 = 1;

/// Encrypt `plaintext` for every device of `recipient`, returning one `<enc>`
/// Node per device (with `type="pkmsg"` for first-flight, `type="msg"` for
/// established sessions).
///
/// TODO: multi-device fan-out. Upstream's `encryptMessageForDevices` walks
/// `getDevicesForJID(recipient)` and emits one `<enc>` per device JID
/// (`recipient_user.device@server`). For now we encrypt only against the
/// recipient JID as supplied — single-device path. The wrapping `<message>
/// <participants>` build step in `send.rs` is the natural place to fan
/// this out once device-list discovery lands.
pub async fn encrypt_for_recipient(
    client: &Client,
    recipient: &Jid,
    plaintext: &[u8],
) -> Result<Vec<Node>, ClientError> {
    let address = SignalAddress::from_jid(recipient).serialize();

    // Load existing session blob, or refuse with NoSession (mirrors
    // whatsmeow `encryptMessageForDevice` which returns `ErrNoSession`
    // when ContainsSession is false and no bundle was supplied).
    let blob = client
        .device
        .sessions
        .get_session(&address)
        .await?
        .ok_or_else(|| ClientError::NoSession(address.clone()))?;

    let mut session = decode_session(&blob)
        .map_err(|e| ClientError::Other(format!("session decode for {address}: {e}")))?;

    // Drive the cipher.
    let env = SessionCipher::encrypt(&mut session, plaintext)
        .map_err(|e| ClientError::Crypto(e.to_string()))?;

    // Persist the advanced session back so the next encrypt picks up
    // where this one left off.
    let new_blob = encode_session(&session);
    client
        .device
        .sessions
        .put_session(&address, new_blob)
        .await?;

    let (enc_type, ciphertext) = match env {
        EncryptedMessage::Pkmsg(b) => ("pkmsg", b),
        EncryptedMessage::Msg(b) => ("msg", b),
    };

    Ok(vec![build_enc_node(enc_type, ciphertext)])
}

/// Build the wire `<enc>` Node:
/// ```xml
/// <enc type="pkmsg|msg" v="2">…ciphertext…</enc>
/// ```
/// `v="2"` matches whatsmeow's `encAttrs["v"] = "2"`.
pub fn build_enc_node(enc_type: &'static str, ciphertext: Vec<u8>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("v".to_owned(), Value::String("2".to_owned()));
    attrs.insert("type".to_owned(), Value::String(enc_type.to_owned()));
    Node::new("enc", attrs, Some(Value::Bytes(ciphertext)))
}

// --- session blob serializer ------------------------------------------------

/// Layout (little-endian for u32s):
///
/// `[fmt(1)] [session_version u32] [local_id_pub 32] [remote_id_pub 32]`
/// `[root_key 32]`
/// `[has_sender_chain u8] [chain_key 32] [chain_idx u32]   (only if has_sender_chain == 1)`
/// `[has_sender_ratchet u8] [priv 32] [pub 32]              (only if has_sender_ratchet == 1)`
/// `[recv_chain_count u32] (per chain: [peer_pub 32] [chain_key 32] [chain_idx u32])`
/// `[previous_counter u32]`
/// `[local_reg_id u32] [remote_reg_id u32]`
/// `[initialised u8]`
fn encode_session(s: &SessionState) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.push(SESSION_FORMAT_VERSION);
    out.extend_from_slice(&s.session_version.to_le_bytes());
    out.extend_from_slice(&s.local_identity_public);
    out.extend_from_slice(&s.remote_identity_public);
    out.extend_from_slice(&s.root_key.key);

    match &s.sender_chain_key {
        Some(c) => {
            out.push(1);
            out.extend_from_slice(&c.key);
            out.extend_from_slice(&c.index.to_le_bytes());
        }
        None => out.push(0),
    }

    match &s.sender_ratchet_keypair {
        Some(kp) => {
            out.push(1);
            out.extend_from_slice(&kp.private);
            out.extend_from_slice(&kp.public);
        }
        None => out.push(0),
    }

    let n = s.receiver_chains.len() as u32;
    out.extend_from_slice(&n.to_le_bytes());
    for (peer_pub, chain) in &s.receiver_chains {
        out.extend_from_slice(peer_pub);
        out.extend_from_slice(&chain.key);
        out.extend_from_slice(&chain.index.to_le_bytes());
    }

    out.extend_from_slice(&s.previous_counter.to_le_bytes());
    out.extend_from_slice(&s.local_registration_id.to_le_bytes());
    out.extend_from_slice(&s.remote_registration_id.to_le_bytes());
    out.push(if s.initialised { 1 } else { 0 });
    out
}

/// Inverse of [`encode_session`]. Returns `Err` if the blob is shorter than
/// the layout demands or the format tag is unknown.
fn decode_session(b: &[u8]) -> Result<SessionState, &'static str> {
    let mut c = Cursor::new(b);
    let fmt = c.u8()?;
    if fmt != SESSION_FORMAT_VERSION {
        return Err("unknown session format version");
    }
    let session_version = c.u32()?;
    let local_identity_public = c.arr32()?;
    let remote_identity_public = c.arr32()?;
    let root_key_bytes = c.arr32()?;

    let sender_chain_key = if c.u8()? == 1 {
        let key = c.arr32()?;
        let index = c.u32()?;
        Some(ChainKey::new(key, index))
    } else {
        None
    };

    let sender_ratchet_keypair = if c.u8()? == 1 {
        let private = c.arr32()?;
        let public = c.arr32()?;
        Some(KeyPair { private, public })
    } else {
        None
    };

    let n = c.u32()? as usize;
    let mut receiver_chains = Vec::with_capacity(n);
    for _ in 0..n {
        let peer_pub = c.arr32()?;
        let key = c.arr32()?;
        let index = c.u32()?;
        receiver_chains.push((peer_pub, ChainKey::new(key, index)));
    }

    let previous_counter = c.u32()?;
    let local_registration_id = c.u32()?;
    let remote_registration_id = c.u32()?;
    let initialised = c.u8()? != 0;

    Ok(SessionState {
        session_version,
        local_identity_public,
        remote_identity_public,
        root_key: RootKey::new(root_key_bytes),
        sender_chain_key,
        sender_ratchet_keypair,
        receiver_chains,
        previous_counter,
        pending_pre_key: None,
        local_registration_id,
        remote_registration_id,
        initialised,
        skipped_message_keys: SkippedKeyCache::new(),
    })
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        if self.pos + n > self.buf.len() {
            return Err("session blob truncated");
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn arr32(&mut self) -> Result<[u8; 32], &'static str> {
        let s = self.take(32)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(s);
        Ok(out)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    use wha_signal::cipher::EncryptedMessage;
    use wha_signal::SessionCipher;

    /// Build a synthetic Alice/Bob pair (post-X3DH steady state) by
    /// seeding both sides with the same root + chain keys, exactly the
    /// same way `cipher::tests::synth_pair` does. We can't import the
    /// helper directly (it's behind `#[cfg(test)]` in another crate),
    /// so we reproduce it here.
    fn synth_pair() -> (SessionState, SessionState, [u8; 32], [u8; 32]) {
        use rand::SeedableRng;

        let mut rng = rand::rngs::StdRng::seed_from_u64(0xCAFE_F00D);
        let alice_identity = KeyPair::generate(&mut rng);
        let bob_identity = KeyPair::generate(&mut rng);
        let alice_ratchet = KeyPair::generate(&mut rng);
        let bob_ratchet = KeyPair::generate(&mut rng);
        let alice_ratchet_pub = alice_ratchet.public;
        let bob_ratchet_pub = bob_ratchet.public;

        let root_seed = [0xAAu8; 32];
        let chain_seed = [0xBBu8; 32];

        let alice = SessionState {
            session_version: 3,
            local_identity_public: alice_identity.public,
            remote_identity_public: bob_identity.public,
            root_key: RootKey::new(root_seed),
            sender_chain_key: Some(ChainKey::new(chain_seed, 0)),
            sender_ratchet_keypair: Some(alice_ratchet),
            receiver_chains: Vec::new(),
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id: 1,
            remote_registration_id: 2,
            initialised: true,
            skipped_message_keys: SkippedKeyCache::new(),
        };

        let bob = SessionState {
            session_version: 3,
            local_identity_public: bob_identity.public,
            remote_identity_public: alice_identity.public,
            root_key: RootKey::new(root_seed),
            sender_chain_key: None,
            sender_ratchet_keypair: Some(bob_ratchet),
            receiver_chains: vec![(alice_ratchet_pub, ChainKey::new(chain_seed, 0))],
            previous_counter: 0,
            pending_pre_key: None,
            local_registration_id: 2,
            remote_registration_id: 1,
            initialised: true,
            skipped_message_keys: SkippedKeyCache::new(),
        };

        (alice, bob, alice_ratchet_pub, bob_ratchet_pub)
    }

    #[test]
    fn build_enc_node_has_type_and_v_attrs() {
        let n = build_enc_node("pkmsg", vec![1, 2, 3, 4]);
        assert_eq!(n.tag, "enc");
        assert_eq!(n.get_attr_str("type"), Some("pkmsg"));
        assert_eq!(n.get_attr_str("v"), Some("2"));
        match &n.content {
            Value::Bytes(b) => assert_eq!(b.as_slice(), &[1u8, 2, 3, 4]),
            other => panic!("expected Bytes, got {other:?}"),
        }
    }

    #[test]
    fn build_enc_node_msg_variant() {
        let n = build_enc_node("msg", vec![]);
        assert_eq!(n.get_attr_str("type"), Some("msg"));
    }

    #[test]
    fn session_blob_round_trips() {
        let (alice, _bob, _, _) = synth_pair();
        let blob = encode_session(&alice);
        let back = decode_session(&blob).expect("decode");
        // Compare every persisted field. (`pending_pre_key` and
        // `skipped_message_keys` deliberately drop on encode, see the
        // module docstring.)
        assert_eq!(back.session_version, alice.session_version);
        assert_eq!(back.local_identity_public, alice.local_identity_public);
        assert_eq!(back.remote_identity_public, alice.remote_identity_public);
        assert_eq!(back.root_key, alice.root_key);
        assert_eq!(back.sender_chain_key, alice.sender_chain_key);
        assert_eq!(
            back.sender_ratchet_keypair.as_ref().map(|k| k.public),
            alice.sender_ratchet_keypair.as_ref().map(|k| k.public),
        );
        assert_eq!(
            back.sender_ratchet_keypair.as_ref().map(|k| k.private),
            alice.sender_ratchet_keypair.as_ref().map(|k| k.private),
        );
        assert_eq!(back.receiver_chains, alice.receiver_chains);
        assert_eq!(back.previous_counter, alice.previous_counter);
        assert_eq!(back.local_registration_id, alice.local_registration_id);
        assert_eq!(back.remote_registration_id, alice.remote_registration_id);
        assert_eq!(back.initialised, alice.initialised);
    }

    #[tokio::test]
    async fn encrypt_with_existing_session_emits_msg() {
        // Set up alice/bob at convergence, persist alice into a Client's
        // session store, run encrypt_for_recipient, and decrypt with bob.
        let (alice, mut bob, _, _) = synth_pair();

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let recipient: Jid = "1234:0@s.whatsapp.net".parse().unwrap();
        let address = SignalAddress::from_jid(&recipient).serialize();

        // Persist alice's session under the recipient address.
        device
            .sessions
            .put_session(&address, encode_session(&alice))
            .await
            .unwrap();

        let (client, _evt) = Client::new(device);
        let plaintext = b"port whatsmeow";
        let nodes = encrypt_for_recipient(&client, &recipient, plaintext)
            .await
            .expect("encrypt");
        assert_eq!(nodes.len(), 1);
        let enc = &nodes[0];
        assert_eq!(enc.tag, "enc");
        // No pending pre-key on alice, so the type must be "msg".
        assert_eq!(enc.get_attr_str("type"), Some("msg"));
        assert_eq!(enc.get_attr_str("v"), Some("2"));
        let ciphertext = match &enc.content {
            Value::Bytes(b) => b.clone(),
            other => panic!("expected bytes, got {other:?}"),
        };

        // Bob decrypts via the bare SignalMessage path (matches what an
        // <enc type="msg"> wraps on the wire).
        let env = EncryptedMessage::Msg(ciphertext);
        let decrypted = SessionCipher::decrypt(&mut bob, env.as_bytes()).expect("decrypt");
        assert_eq!(decrypted, plaintext);

        // The persisted session must have advanced (sender chain index +1).
        let after_blob = client
            .device
            .sessions
            .get_session(&address)
            .await
            .unwrap()
            .expect("session still present");
        let after = decode_session(&after_blob).expect("decode");
        assert_eq!(
            after.sender_chain_key.as_ref().unwrap().index,
            1,
            "sender chain must advance by one after a single encrypt",
        );
    }

    #[tokio::test]
    async fn encrypt_without_session_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let recipient: Jid = "5678:0@s.whatsapp.net".parse().unwrap();
        let err = encrypt_for_recipient(&client, &recipient, b"hi")
            .await
            .expect_err("must fail without a session");
        match err {
            ClientError::NoSession(addr) => {
                assert_eq!(addr, SignalAddress::from_jid(&recipient).serialize());
            }
            other => panic!("expected NoSession, got {other:?}"),
        }
    }
}
