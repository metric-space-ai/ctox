//! Incoming group (`skmsg`) decryption.
//!
//! Mirrors the `skmsg` branch in `whatsmeow/message.go::decryptGroupMsg` plus
//! `handleSenderKeyDistributionMessage`. We pull the per-`(group, sender)`
//! [`wha_signal::sender_key_record::SenderKeyRecord`] out of the
//! [`wha_store::SenderKeyStore`], call into [`wha_signal::group_cipher`] for
//! the actual chain-key walk, and then persist the (now-mutated) record back.
//!
//! Persistence note — there is no upstream-compatible serialiser yet for
//! [`SenderKeyRecord`] (libsignal's Go original wraps a protobuf the world
//! hasn't agreed on the shape of). We carry our own minimal versioned binary
//! format here. The on-wire/at-rest layout is local to this client and
//! intentionally not exposed; if/when an interop format lands, the encoder
//! is a single-call replacement.
//!
//! Current scope — we only persist the **newest** [`SenderKeyState`] of the
//! record. `SenderKeyRecord` keeps up to `MAX_STATES = 5` generations for
//! in-flight rotations, but in WhatsApp's group flow the receiver's record
//! only ever needs the latest state to decrypt incoming `skmsg`s; older
//! generations are an optimisation for transient rotation overlap. Without
//! an iteration accessor on `SenderKeyRecord` we cannot enumerate the full
//! list through its public surface, so this is the minimal correct subset.
//! A `TODO` is left below for the day we want full multi-state retention.
//!
//! Layout (all little-endian, no padding):
//! ```text
//! magic    : "WSKR"      (4 bytes)
//! version  : u8          (= 1)
//! has_state: u8          (0 → empty record, 1 → state follows)
//! state    : SerState    (only when has_state = 1)
//!
//! SerState
//!   key_id           : u32
//!   chain_iter       : u32
//!   chain_seed       : [u8; 32]
//!   signing_pub      : [u8; 32]
//!   has_signing_priv : u8 (0 | 1)
//!   signing_priv?    : [u8; 32]   (only when flag = 1)
//!   skipped_count    : u32
//!   skipped_keys     : SerSkipped * skipped_count   (insertion order)
//!
//! SerSkipped
//!   iteration  : u32
//!   seed       : [u8; 32]
//!   iv         : [u8; 16]
//!   cipher_key : [u8; 32]
//! ```
// TODO: extend with full multi-state retention once SenderKeyRecord exposes
// an enumeration accessor (or once we adopt the libsignal interop layout).

use wha_signal::address::{SenderKeyName, SignalAddress};
use wha_signal::group_cipher::SenderKeyMessage;
use wha_signal::group_session::{GroupSessionBuilder, SenderKeyDistributionMessage};
use wha_signal::sender_key::{SenderChainKey, SenderKeyState, SenderMessageKey};
use wha_signal::sender_key_record::SenderKeyRecord;
use wha_signal::SignalProtocolError;
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;

const MAGIC: &[u8; 4] = b"WSKR";
const VERSION: u8 = 1;

// ---------- error mapping ----------------------------------------------------

fn map_signal_err(e: SignalProtocolError) -> ClientError {
    ClientError::Crypto(e.to_string())
}

// ---------- serialiser -------------------------------------------------------

/// Serialise a [`SenderKeyRecord`] to the local binary format. Persists only
/// the newest state — see module-level docs.
pub(crate) fn serialise_record(record: &SenderKeyRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(192);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    match record.sender_key_state() {
        None => {
            out.push(0);
        }
        Some(state) => {
            out.push(1);
            write_state(&mut out, state);
        }
    }
    out
}

fn write_state(out: &mut Vec<u8>, state: &SenderKeyState) {
    out.extend_from_slice(&state.key_id.to_le_bytes());
    out.extend_from_slice(&state.chain_key.iteration.to_le_bytes());
    out.extend_from_slice(&state.chain_key.seed);
    out.extend_from_slice(&state.signing_key_public);
    if let Some(priv_) = state.signing_key_private {
        out.push(1);
        out.extend_from_slice(&priv_);
    } else {
        out.push(0);
    }

    debug_assert!(state.skipped_order.len() <= u32::MAX as usize);
    out.extend_from_slice(&(state.skipped_order.len() as u32).to_le_bytes());
    for it in &state.skipped_order {
        let mk = state
            .skipped_message_keys
            .get(it)
            .expect("skipped_order ↔ skipped_message_keys invariant");
        out.extend_from_slice(&mk.iteration.to_le_bytes());
        out.extend_from_slice(&mk.seed);
        out.extend_from_slice(&mk.iv);
        out.extend_from_slice(&mk.cipher_key);
    }
}

/// Deserialise a [`SenderKeyRecord`]. Bails on truncation, magic mismatch, or
/// version skew.
pub(crate) fn deserialise_record(bytes: &[u8]) -> Result<SenderKeyRecord, ClientError> {
    let mut r = Reader::new(bytes);
    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(ClientError::Crypto("sender key record: bad magic".into()));
    }
    let version = r.u8()?;
    if version != VERSION {
        return Err(ClientError::Crypto(format!(
            "sender key record: unsupported version {version}"
        )));
    }
    let has_state = r.u8()?;
    let mut record = SenderKeyRecord::new();
    match has_state {
        0 => {}
        1 => {
            let st = read_state(&mut r)?;
            record.add_sender_key_state(st);
        }
        other => {
            return Err(ClientError::Crypto(format!(
                "sender key record: bad has_state flag {other}"
            )));
        }
    }
    Ok(record)
}

fn read_state(r: &mut Reader<'_>) -> Result<SenderKeyState, ClientError> {
    let key_id = r.u32()?;
    let chain_iter = r.u32()?;
    let chain_seed: [u8; 32] = r.array32()?;
    let signing_pub: [u8; 32] = r.array32()?;
    let priv_flag = r.u8()?;
    let signing_priv = match priv_flag {
        0 => None,
        1 => Some(r.array32()?),
        other => {
            return Err(ClientError::Crypto(format!(
                "sender key state: bad priv flag {other}"
            )));
        }
    };

    let mut state = SenderKeyState {
        key_id,
        chain_key: SenderChainKey::new(chain_iter, chain_seed),
        signing_key_public: signing_pub,
        signing_key_private: signing_priv,
        skipped_message_keys: std::collections::HashMap::new(),
        skipped_order: Vec::new(),
    };

    let skipped_count = r.u32()? as usize;
    for _ in 0..skipped_count {
        let it = r.u32()?;
        let seed: [u8; 32] = r.array32()?;
        let iv: [u8; 16] = r.array16()?;
        let cipher_key: [u8; 32] = r.array32()?;
        state.add_sender_message_key(SenderMessageKey { iteration: it, seed, iv, cipher_key });
    }

    Ok(state)
}

struct Reader<'a> {
    buf: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, i: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ClientError> {
        let end = self
            .i
            .checked_add(n)
            .ok_or_else(|| ClientError::Crypto("sender key record: length overflow".into()))?;
        if end > self.buf.len() {
            return Err(ClientError::Crypto("sender key record: truncated".into()));
        }
        let slice = &self.buf[self.i..end];
        self.i = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, ClientError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, ClientError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn array16(&mut self) -> Result<[u8; 16], ClientError> {
        let s = self.take(16)?;
        let mut out = [0u8; 16];
        out.copy_from_slice(s);
        Ok(out)
    }

    fn array32(&mut self) -> Result<[u8; 32], ClientError> {
        let s = self.take(32)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(s);
        Ok(out)
    }
}

// ---------- public API -------------------------------------------------------

/// Decrypt a single `<enc type="skmsg">` body. Mirrors
/// `whatsmeow/message.go::decryptGroupMsg` minus the unpadding step (the
/// caller still owns padding-version handling, since it depends on the parent
/// `<enc>` node's `v` attribute).
pub async fn handle_group_message(
    client: &Client,
    group: &Jid,
    sender: &Jid,
    ciphertext: &[u8],
) -> Result<Vec<u8>, ClientError> {
    let group_str = group.to_string();
    let sender_str = sender.to_string();
    let name = SenderKeyName::new(group_str.clone(), SignalAddress::from_jid(sender));

    let blob = client
        .device
        .sender_keys
        .get_sender_key(&group_str, &sender_str)
        .await?
        .ok_or_else(|| ClientError::NoSession("no group sender key".into()))?;

    let mut record = deserialise_record(&blob)?;

    let plaintext =
        SenderKeyMessage::decrypt(&mut record, &name, ciphertext).map_err(map_signal_err)?;

    let new_blob = serialise_record(&record);
    client
        .device
        .sender_keys
        .put_sender_key(&group_str, &sender_str, new_blob)
        .await?;

    Ok(plaintext)
}

/// Install an inbound SenderKeyDistributionMessage. Mirrors
/// `whatsmeow/message.go::handleSenderKeyDistributionMessage`.
pub async fn handle_sender_key_distribution(
    client: &Client,
    group: &Jid,
    sender: &Jid,
    skdm_bytes: &[u8],
) -> Result<(), ClientError> {
    let group_str = group.to_string();
    let sender_str = sender.to_string();
    let name = SenderKeyName::new(group_str.clone(), SignalAddress::from_jid(sender));

    let msg = SenderKeyDistributionMessage::decode(skdm_bytes).map_err(map_signal_err)?;

    let mut record = match client
        .device
        .sender_keys
        .get_sender_key(&group_str, &sender_str)
        .await?
    {
        Some(blob) => deserialise_record(&blob)?,
        None => SenderKeyRecord::new(),
    };

    GroupSessionBuilder::process_distribution_message(&mut record, &name, &msg)
        .map_err(map_signal_err)?;

    let new_blob = serialise_record(&record);
    client
        .device
        .sender_keys
        .put_sender_key(&group_str, &sender_str, new_blob)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_crypto::KeyPair;
    use wha_signal::group_cipher::SenderKeyMessage;
    use wha_signal::sender_key::SenderKeyState;
    use wha_signal::sender_key_record::SenderKeyRecord;
    use wha_store::MemoryStore;

    fn group_jid() -> Jid {
        "120363010101010101@g.us".parse().unwrap()
    }
    fn sender_jid() -> Jid {
        "1234:0@s.whatsapp.net".parse().unwrap()
    }

    /// Same fixed signing-key bytes as `wha-signal`'s own group-cipher tests,
    /// so we can independently rebuild the matching public half on the
    /// receiver side.
    const TEST_SIGNING_PRIV: [u8; 32] = [
        0x18, 0x77, 0x21, 0x4f, 0x2e, 0x73, 0x10, 0x4d, 0x83, 0x40, 0x66, 0x42, 0x9c, 0x55, 0x09,
        0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98,
        0x76, 0x54,
    ];

    fn fresh_owner_record(key_id: u32) -> SenderKeyRecord {
        let chain_seed = [11u8; 32];
        let signing = KeyPair::from_private(TEST_SIGNING_PRIV);
        let st = SenderKeyState::new_own(key_id, 0, chain_seed, signing);
        let mut rec = SenderKeyRecord::new();
        rec.set_sender_key_state(st);
        rec
    }

    #[tokio::test]
    async fn decrypt_unknown_group_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);

        let group = group_jid();
        let sender = sender_jid();
        // Buffer length doesn't matter — the store lookup fails first.
        let res = handle_group_message(&client, &group, &sender, &[0u8; 65]).await;
        assert!(matches!(res, Err(ClientError::NoSession(_))), "got {:?}", res);
    }

    #[tokio::test]
    async fn process_distribution_idempotent() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let group = group_jid();
        let sender = sender_jid();

        // Build a real SKDM via the Create path against an empty record.
        let mut creator_record = SenderKeyRecord::new();
        let name = SenderKeyName::new(group.to_string(), SignalAddress::from_jid(&sender));
        let skdm = GroupSessionBuilder::create_distribution_message(&mut creator_record, &name)
            .expect("create skdm");
        let skdm_bytes = skdm.encode().unwrap();

        // First install — must succeed with no record present.
        handle_sender_key_distribution(&client, &group, &sender, &skdm_bytes)
            .await
            .expect("first install");
        // Second install — same SKDM, same call. Must not error.
        handle_sender_key_distribution(&client, &group, &sender, &skdm_bytes)
            .await
            .expect("second install");

        // Stored record round-trips successfully.
        let blob = client
            .device
            .sender_keys
            .get_sender_key(&group.to_string(), &sender.to_string())
            .await
            .unwrap()
            .expect("record present after install");
        let record = deserialise_record(&blob).unwrap();
        assert!(record.sender_key_state().is_some());
    }

    #[tokio::test]
    async fn decrypt_after_skdm_round_trip() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let group = group_jid();
        let sender = sender_jid();

        // Producer: own record with private signing key. Encrypt one message —
        // this also advances the producer's chain key past iteration 0 (we
        // don't touch the producer record after this).
        let mut producer = fresh_owner_record(7);
        let plaintext = b"group hello";
        let wire = SenderKeyMessage::encrypt(&mut producer, plaintext).expect("encrypt");

        // Receiver-side SKDM derived from the *initial* producer state
        // (iteration 0, original chain seed, matching signing pubkey).
        let initial_state_skdm = SenderKeyDistributionMessage {
            key_id: 7,
            iteration: 0,
            chain_key: [11u8; 32],
            signing_key_public: KeyPair::from_private(TEST_SIGNING_PRIV).public,
        };
        let skdm_bytes = initial_state_skdm.encode().unwrap();

        handle_sender_key_distribution(&client, &group, &sender, &skdm_bytes)
            .await
            .expect("install skdm");

        let decrypted = handle_group_message(&client, &group, &sender, &wire)
            .await
            .expect("decrypt skmsg");
        assert_eq!(decrypted, plaintext);
    }
}
