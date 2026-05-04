//! Group-session distribution-message handling.
//!
//! Faithfully ported from `go.mau.fi/libsignal`:
//! * `protocol/SenderKeyDistributionMessage.go` — wire-format message that
//!   ships a fresh sender-key state (id + chain key + signing pubkey) to
//!   every other group member.
//! * `groups/GroupSessionBuilder.go` — installs an inbound distribution
//!   message into a record (`Process`) or generates a fresh state on first
//!   send and emits the matching distribution message (`Create`).
//!
//! Wire layout (matches libsignal byte-for-byte):
//!
//! ```text
//! version_byte (0x33) || protobuf_body
//! ```
//!
//! The protobuf body has four required fields:
//!
//! ```text
//! 1: uint32 id          (key_id)
//! 2: uint32 iteration
//! 3: bytes  chainKey    (32 bytes)
//! 4: bytes  signingKey  (32 bytes — raw X25519 public key)
//! ```
//!
//! We hand-roll the protobuf encoding here for the same reason as
//! `protocol_message.rs`: the schema is tiny and stable, and avoiding
//! `prost` keeps the dependency surface unchanged. Varints are encoded
//! LSB-first (canonical proto3).

use rand::rngs::OsRng;
use rand::RngCore;

use wha_crypto::KeyPair;

use crate::address::SenderKeyName;
use crate::sender_key::SenderKeyState;
use crate::sender_key_record::SenderKeyRecord;
use crate::SignalProtocolError;

/// Version prefix byte. Matches libsignal's `((CURRENT_VERSION & 0x0F) << 4) | (CURRENT_VERSION & 0x0F)`
/// for v3, which is `0x33`.
const VERSION_BYTE: u8 = 0x33;

// ---------- protobuf primitives ----------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(((value as u8) & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(buf: &[u8], offset: usize) -> Result<(u64, usize), SignalProtocolError> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    let mut i = offset;
    loop {
        if i >= buf.len() {
            return Err(SignalProtocolError::InvalidMessage("truncated varint"));
        }
        let b = buf[i];
        i += 1;
        value |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((value, i - offset));
        }
        shift += 7;
        if shift >= 64 {
            return Err(SignalProtocolError::InvalidMessage("varint overflow"));
        }
    }
}

fn put_tag(out: &mut Vec<u8>, field: u32, wire: u32) {
    put_varint(out, ((field as u64) << 3) | (wire as u64));
}

fn put_uint32_field(out: &mut Vec<u8>, field: u32, value: u32) {
    put_tag(out, field, 0); // varint wire
    put_varint(out, value as u64);
}

fn put_bytes_field(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    put_tag(out, field, 2); // length-delimited
    put_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn skip_unknown(buf: &[u8], i: &mut usize, wire: u32) -> Result<(), SignalProtocolError> {
    match wire {
        0 => {
            let (_, n) = read_varint(buf, *i)?;
            *i += n;
        }
        1 => {
            *i = i
                .checked_add(8)
                .ok_or(SignalProtocolError::InvalidMessage("fixed64 overflow"))?;
            if *i > buf.len() {
                return Err(SignalProtocolError::InvalidMessage("fixed64 truncated"));
            }
        }
        2 => {
            let (len, n) = read_varint(buf, *i)?;
            *i += n;
            *i = i
                .checked_add(len as usize)
                .ok_or(SignalProtocolError::InvalidMessage("bytes overflow"))?;
            if *i > buf.len() {
                return Err(SignalProtocolError::InvalidMessage("bytes truncated"));
            }
        }
        5 => {
            *i = i
                .checked_add(4)
                .ok_or(SignalProtocolError::InvalidMessage("fixed32 overflow"))?;
            if *i > buf.len() {
                return Err(SignalProtocolError::InvalidMessage("fixed32 truncated"));
            }
        }
        _ => return Err(SignalProtocolError::InvalidMessage("unknown wire type")),
    }
    Ok(())
}

// ---------- SenderKeyDistributionMessage --------------------------------------

/// Wire-format SenderKeyDistributionMessage. Format:
/// `version_byte (0x33) || protobuf{id, iteration, chain_key, signing_key}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderKeyDistributionMessage {
    pub key_id: u32,
    pub iteration: u32,
    pub chain_key: [u8; 32],
    pub signing_key_public: [u8; 32],
}

impl SenderKeyDistributionMessage {
    /// Serialise to the `version_byte || pb_body` wire form.
    pub fn encode(&self) -> Result<Vec<u8>, SignalProtocolError> {
        let mut out = Vec::with_capacity(1 + 80);
        out.push(VERSION_BYTE);
        put_uint32_field(&mut out, 1, self.key_id);
        put_uint32_field(&mut out, 2, self.iteration);
        put_bytes_field(&mut out, 3, &self.chain_key);
        put_bytes_field(&mut out, 4, &self.signing_key_public);
        Ok(out)
    }

    /// Parse the wire form. Rejects anything that does not start with the
    /// expected v3 version byte; rejects payloads missing required fields.
    pub fn decode(bytes: &[u8]) -> Result<Self, SignalProtocolError> {
        if bytes.is_empty() {
            return Err(SignalProtocolError::InvalidMessage(
                "sender key distribution empty",
            ));
        }
        if bytes[0] != VERSION_BYTE {
            // libsignal rejects both pre-v3 ("old") and unknown future
            // versions with the same surface error here. We squeeze them
            // both into `UnsupportedVersion` carrying the high nibble, the
            // same field libsignal compares against.
            let version = (bytes[0] >> 4) & 0x0F;
            return Err(SignalProtocolError::UnsupportedVersion(version));
        }
        let buf = &bytes[1..];

        let mut key_id: Option<u32> = None;
        let mut iteration: Option<u32> = None;
        let mut chain_key: Option<[u8; 32]> = None;
        let mut signing_key_public: Option<[u8; 32]> = None;

        let mut i = 0;
        while i < buf.len() {
            let (tag, n) = read_varint(buf, i)?;
            i += n;
            let field = (tag >> 3) as u32;
            let wire = (tag & 0x07) as u32;
            match (field, wire) {
                (1, 0) => {
                    let (v, n) = read_varint(buf, i)?;
                    i += n;
                    key_id = Some(v as u32);
                }
                (2, 0) => {
                    let (v, n) = read_varint(buf, i)?;
                    i += n;
                    iteration = Some(v as u32);
                }
                (3, 2) => {
                    let (len, n) = read_varint(buf, i)?;
                    i += n;
                    let end = i
                        .checked_add(len as usize)
                        .ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                    if end > buf.len() {
                        return Err(SignalProtocolError::InvalidMessage("chainKey truncated"));
                    }
                    let raw = &buf[i..end];
                    if raw.len() != 32 {
                        return Err(SignalProtocolError::InvalidMessage(
                            "chainKey wrong length",
                        ));
                    }
                    let mut ck = [0u8; 32];
                    ck.copy_from_slice(raw);
                    chain_key = Some(ck);
                    i = end;
                }
                (4, 2) => {
                    let (len, n) = read_varint(buf, i)?;
                    i += n;
                    let end = i
                        .checked_add(len as usize)
                        .ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                    if end > buf.len() {
                        return Err(SignalProtocolError::InvalidMessage("signingKey truncated"));
                    }
                    let raw = &buf[i..end];
                    if raw.len() != 32 {
                        return Err(SignalProtocolError::InvalidMessage(
                            "signingKey wrong length",
                        ));
                    }
                    let mut pk = [0u8; 32];
                    pk.copy_from_slice(raw);
                    signing_key_public = Some(pk);
                    i = end;
                }
                _ => skip_unknown(buf, &mut i, wire)?,
            }
        }

        Ok(Self {
            key_id: key_id.ok_or(SignalProtocolError::InvalidMessage("missing field"))?,
            iteration: iteration.ok_or(SignalProtocolError::InvalidMessage("missing field"))?,
            chain_key: chain_key.ok_or(SignalProtocolError::InvalidMessage("missing field"))?,
            signing_key_public: signing_key_public
                .ok_or(SignalProtocolError::InvalidMessage("missing field"))?,
        })
    }
}

// ---------- GroupSessionBuilder ----------------------------------------------

/// Stateless helper bundling the two libsignal flows around sender-key
/// records. The Go original threads a `SenderKeyStore` through every call;
/// in Rust we operate on the loaded record directly so callers (the SDK
/// store layer) keep ownership of persistence.
pub struct GroupSessionBuilder;

impl GroupSessionBuilder {
    /// Install the state carried by `msg` into `record`. Mirrors libsignal's
    /// `SessionBuilder.Process`: the new state is added at the front of the
    /// record's ring buffer (newest-first), so the next decrypt for this
    /// `(group, sender)` will pick it up.
    pub fn process_distribution_message(
        record: &mut SenderKeyRecord,
        _name: &SenderKeyName,
        msg: &SenderKeyDistributionMessage,
    ) -> Result<(), SignalProtocolError> {
        record.add_sender_key_state(SenderKeyState::new_remote(
            msg.key_id,
            msg.iteration,
            msg.chain_key,
            msg.signing_key_public,
        ));
        Ok(())
    }

    /// Get-or-create the newest state for this peer's group session and
    /// return the matching distribution message. Mirrors libsignal's
    /// `SessionBuilder.Create`:
    ///
    /// * if `record` is empty, generate a fresh state with a random key id,
    ///   random 32-byte chain seed, and random signing keypair (X25519);
    ///   add it to the record so callers persist it back.
    /// * otherwise reuse the newest state — this is the read-side path
    ///   used when re-broadcasting our own sender key.
    pub fn create_distribution_message(
        record: &mut SenderKeyRecord,
        _name: &SenderKeyName,
    ) -> Result<SenderKeyDistributionMessage, SignalProtocolError> {
        if record.is_empty() {
            let mut rng = OsRng;
            let key_id = rng.next_u32();
            let mut chain_seed = [0u8; 32];
            rng.fill_bytes(&mut chain_seed);
            let signing = KeyPair::generate(&mut rng);
            let state = SenderKeyState::new_own(key_id, 0, chain_seed, signing);
            record.add_sender_key_state(state);
        }

        let state = record
            .sender_key_state()
            .ok_or(SignalProtocolError::UninitialisedSession)?;
        Ok(SenderKeyDistributionMessage {
            key_id: state.key_id,
            iteration: state.chain_key.iteration,
            chain_key: state.chain_key.seed,
            signing_key_public: state.signing_key_public,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::SignalAddress;

    fn name() -> SenderKeyName {
        SenderKeyName::new("group-id".to_string(), SignalAddress::new("alice", 1))
    }

    #[test]
    fn encode_decode_round_trip() {
        let m = SenderKeyDistributionMessage {
            key_id: 0xDEAD_BEEF,
            iteration: 7,
            chain_key: [0xAB; 32],
            signing_key_public: [0xCD; 32],
        };
        let bytes = m.encode().unwrap();
        assert_eq!(bytes[0], VERSION_BYTE);
        let parsed = SenderKeyDistributionMessage::decode(&bytes).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn decode_rejects_wrong_version_byte() {
        // First byte is 0x32 (high nibble 3, low nibble 2): not our v3 prefix.
        let mut bytes = vec![0x32u8];
        // Append a syntactically valid body so we know the version check
        // is what's tripping the error.
        let stub = SenderKeyDistributionMessage {
            key_id: 1,
            iteration: 0,
            chain_key: [0u8; 32],
            signing_key_public: [0u8; 32],
        };
        let valid = stub.encode().unwrap();
        bytes.extend_from_slice(&valid[1..]);
        assert!(matches!(
            SenderKeyDistributionMessage::decode(&bytes),
            Err(SignalProtocolError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn decode_rejects_missing_field() {
        // Encode only fields 1 and 2; omit chain_key and signing_key_public.
        let mut bytes = vec![VERSION_BYTE];
        put_uint32_field(&mut bytes, 1, 42);
        put_uint32_field(&mut bytes, 2, 0);
        assert!(matches!(
            SenderKeyDistributionMessage::decode(&bytes),
            Err(SignalProtocolError::InvalidMessage("missing field"))
        ));
    }

    #[test]
    fn process_distribution_message_installs_state() {
        let mut record = SenderKeyRecord::new();
        let msg = SenderKeyDistributionMessage {
            key_id: 12345,
            iteration: 9,
            chain_key: [0x11; 32],
            signing_key_public: [0x22; 32],
        };
        GroupSessionBuilder::process_distribution_message(&mut record, &name(), &msg).unwrap();
        assert_eq!(record.len(), 1);
        let state = record.get_sender_key_state(12345).expect("state added");
        assert_eq!(state.key_id, 12345);
        assert_eq!(state.chain_key.iteration, 9);
        assert_eq!(state.chain_key.seed, [0x11; 32]);
        assert_eq!(state.signing_key_public, [0x22; 32]);
        // Installed from a remote distribution: no private signing key.
        assert!(state.signing_key_private.is_none());
    }

    #[test]
    fn create_distribution_message_emits_consistent_state() {
        let mut record = SenderKeyRecord::new();
        let msg = GroupSessionBuilder::create_distribution_message(&mut record, &name()).unwrap();
        // create() must have populated the record.
        assert_eq!(record.len(), 1);
        let state = record.sender_key_state().expect("newest state present");
        assert_eq!(state.key_id, msg.key_id);
        assert_eq!(state.chain_key.iteration, msg.iteration);
        assert_eq!(state.chain_key.seed, msg.chain_key);
        assert_eq!(state.signing_key_public, msg.signing_key_public);
        // Owner state — private signing scalar should be retained.
        assert!(state.signing_key_private.is_some());
        // A second call returns the same state (no re-roll).
        let again = GroupSessionBuilder::create_distribution_message(&mut record, &name()).unwrap();
        assert_eq!(again, msg);
        assert_eq!(record.len(), 1);
    }
}
