//! Group cipher (skmsg) — `SenderKeyMessage` wire format plus encrypt /
//! decrypt over a [`SenderKeyRecord`].
//!
//! Faithfully ported from `go.mau.fi/libsignal/groups/GroupCipher.go` and
//! `go.mau.fi/libsignal/protocol/SenderKeyMessage.go`. The wire format is:
//!
//! ```text
//! SenderKeyMessage := version_byte (0x33) || protobuf_body || signature(64)
//! ```
//!
//! The protobuf body has three fields, all required:
//!
//! ```text
//! 1: uint32 id          (varint)
//! 2: uint32 iteration   (varint)
//! 3: bytes  ciphertext  (length-delimited)
//! ```
//!
//! The XEdDSA signature is computed over `version_byte || protobuf_body`
//! (i.e. everything before the 64-byte signature itself). Signing key is the
//! per-state Curve25519 key pair held in [`SenderKeyState`]; the public half
//! comes along on the distribution message and is what remote peers feed into
//! [`KeyPair::verify`].
//!
//! As with `protocol_message.rs` the protobuf encoder/decoder is hand-rolled —
//! three fields, no oneofs, no reserved tags. Pulling in `prost` for this
//! would inflate the dependency surface for no benefit.

use wha_crypto::{cbc_decrypt, cbc_encrypt, KeyPair};

use crate::address::SenderKeyName;
use crate::sender_key::SenderMessageKey;
use crate::sender_key_record::SenderKeyRecord;
use crate::SignalProtocolError;

/// libsignal's per-state cap on how far a chain may be advanced in one shot
/// when catching up on out-of-order messages. Mirrors the `2000` constant in
/// `groups/GroupCipher.go::getSenderKey`.
pub const MAX_FORWARD_JUMP: u32 = 2000;

/// libsignal version byte for v3 sender-key messages — high nibble = current
/// version (3), low nibble = message version (3). Matches the
/// `(CURRENT_VERSION << 4) | message_version` shape used in `protocol_message.rs`.
pub const SENDER_KEY_VERSION_BYTE: u8 = 0x33;

const SIGNATURE_LEN: usize = 64;

// ---------- protobuf primitives ---------------------------------------------

/// Append a varint-encoded `u64` to `out`.
fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(((value as u8) & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Read a varint from `buf` starting at `offset`. Returns `(value, bytes_read)`.
fn read_varint(buf: &[u8], offset: usize) -> Result<(u64, usize), SignalProtocolError> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
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

// ---------- SenderKeyMessage body codec --------------------------------------

/// Decoded protobuf body — pure data, no signature, no version byte.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SenderKeyMessageBody {
    id: u32,
    iteration: u32,
    ciphertext: Vec<u8>,
}

fn encode_body(id: u32, iteration: u32, ciphertext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + ciphertext.len());
    put_uint32_field(&mut out, 1, id);
    put_uint32_field(&mut out, 2, iteration);
    put_bytes_field(&mut out, 3, ciphertext);
    out
}

fn decode_body(buf: &[u8]) -> Result<SenderKeyMessageBody, SignalProtocolError> {
    let mut id: Option<u32> = None;
    let mut iteration: Option<u32> = None;
    let mut ciphertext: Option<Vec<u8>> = None;

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
                id = Some(v as u32);
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
                    return Err(SignalProtocolError::InvalidMessage("ciphertext truncated"));
                }
                ciphertext = Some(buf[i..end].to_vec());
                i = end;
            }
            _ => skip_unknown(buf, &mut i, wire)?,
        }
    }

    Ok(SenderKeyMessageBody {
        id: id.ok_or(SignalProtocolError::InvalidMessage("missing senderkey id"))?,
        iteration: iteration
            .ok_or(SignalProtocolError::InvalidMessage("missing senderkey iteration"))?,
        ciphertext: ciphertext
            .ok_or(SignalProtocolError::InvalidMessage("missing senderkey ciphertext"))?,
    })
}

// ---------- public API -------------------------------------------------------

/// SenderKeyMessage wire format:
/// `version_byte (0x33) || protobuf{id, iteration, ciphertext} || sig(64)`.
///
/// All the operations on this type are stateless w.r.t. `Self` itself — they
/// take a `&mut SenderKeyRecord` and return raw wire bytes / plaintext. The
/// shape mirrors libsignal's `GroupCipher`, where `SenderKeyMessage` is just
/// the wire-format struct and the `GroupCipher` does the actual key
/// management.
pub struct SenderKeyMessage;

impl SenderKeyMessage {
    /// Encrypt `plaintext` under the newest state in `record`, advancing the
    /// chain. Returns the wire-format bytes (`version || body || signature`).
    ///
    /// Errors:
    /// * `InvalidMessage("no sender state")` if the record is empty.
    /// * `InvalidMessage("missing private signing key")` if the newest state
    ///   was reconstructed from a remote distribution message (we don't own
    ///   the private signing half and therefore can't sign anything).
    /// * `Crypto(_)` if AES-CBC encryption fails (shouldn't, but propagated
    ///   for completeness).
    pub fn encrypt(
        record: &mut SenderKeyRecord,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SignalProtocolError> {
        let state = record
            .sender_key_state_mut()
            .ok_or(SignalProtocolError::InvalidMessage("no sender state"))?;

        let signing_key = state
            .signing_key_pair()
            .ok_or(SignalProtocolError::InvalidMessage("missing private signing key"))?;

        let sender_message_key = state.chain_key.sender_message_key();
        let ciphertext =
            cbc_encrypt(&sender_message_key.cipher_key, &sender_message_key.iv, plaintext)?;

        let body = encode_body(state.key_id, state.chain_key.iteration, &ciphertext);

        // Build the signed payload: version byte || body. Sign with our
        // private signing key, then concatenate signature(64) on the end to
        // form the wire-format message.
        let mut signed_payload = Vec::with_capacity(1 + body.len());
        signed_payload.push(SENDER_KEY_VERSION_BYTE);
        signed_payload.extend_from_slice(&body);

        let signature = signing_key.sign(&signed_payload);

        let mut out = Vec::with_capacity(signed_payload.len() + SIGNATURE_LEN);
        out.extend_from_slice(&signed_payload);
        out.extend_from_slice(&signature);

        // Advance the chain key — once a message-key is consumed, we never
        // reuse it. (Skipped-key recovery for the receiver lives on the other
        // side, not here.)
        let next = state.chain_key.next();
        state.set_chain_key(next);

        Ok(out)
    }

    /// Decrypt a wire-format `SenderKeyMessage` under the matching state in
    /// `record`. The record may contain multiple states (one per generation
    /// of the sender's key); we look up the one whose `key_id` matches the
    /// message's `id` field.
    ///
    /// `name` is currently informational — libsignal uses it to look up the
    /// record from a store, but here the caller has already done that. We
    /// accept it on the signature for parity with the Go API and so callers
    /// can plumb the correct sender into log lines / errors as we add them.
    ///
    /// Errors:
    /// * `UnsupportedVersion(byte)` if the leading byte isn't `0x33`.
    /// * `InvalidMessage(_)` for malformed framing or unknown `key_id`.
    /// * `BadSignature` if XEdDSA verification fails.
    /// * `DuplicateMessage` if the message is older than the current chain
    ///   index and we no longer have the cached skipped key.
    /// * `TooFarIntoFuture` if the message would require advancing the chain
    ///   more than `MAX_FORWARD_JUMP` steps.
    pub fn decrypt(
        record: &mut SenderKeyRecord,
        _name: &SenderKeyName,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SignalProtocolError> {
        if record.is_empty() {
            return Err(SignalProtocolError::InvalidMessage("no sender state"));
        }
        if ciphertext.len() < 1 + SIGNATURE_LEN {
            return Err(SignalProtocolError::InvalidMessage("sender key message too short"));
        }
        if ciphertext[0] != SENDER_KEY_VERSION_BYTE {
            return Err(SignalProtocolError::UnsupportedVersion(ciphertext[0]));
        }

        // Split into [signed_payload || signature(64)]. The signed payload is
        // the version byte plus the protobuf body — the same bytes the sender
        // ran through XEdDSA.
        let split_at = ciphertext.len() - SIGNATURE_LEN;
        let signed_payload = &ciphertext[..split_at];
        let mut signature = [0u8; SIGNATURE_LEN];
        signature.copy_from_slice(&ciphertext[split_at..]);

        // Body is everything after the version byte.
        let body_bytes = &signed_payload[1..];
        let body = decode_body(body_bytes)?;

        // Find the matching state by key_id. We use a borrowed lookup just
        // to verify the signature against the (immutable) public key, then
        // re-borrow mutably for the chain walk. Doing it in two steps keeps
        // the borrow checker happy without cloning the whole state.
        let signing_pub = {
            let state = record
                .get_sender_key_state(body.id)
                .ok_or(SignalProtocolError::InvalidMessage("no sender state for key id"))?;
            state.signing_key_public
        };

        // XEdDSA verify is constant-time inside `KeyPair::verify`.
        KeyPair::verify(&signing_pub, signed_payload, &signature)
            .map_err(|_| SignalProtocolError::BadSignature)?;

        // Walk / fetch the message key for `body.iteration`.
        let state = record
            .get_sender_key_state_mut(body.id)
            .ok_or(SignalProtocolError::InvalidMessage("no sender state for key id"))?;
        let message_key = walk_to_iteration(state, body.iteration)?;

        // Decrypt with the derived AES-CBC key + IV.
        let plaintext = cbc_decrypt(&message_key.cipher_key, &message_key.iv, &body.ciphertext)
            .map_err(SignalProtocolError::from)?;

        Ok(plaintext)
    }
}

/// Advance / consume the chain in `state` to derive (or fetch from cache) the
/// [`SenderMessageKey`] for `target_iteration`.
///
/// Mirrors `GroupCipher.getSenderKey` from libsignal:
///
/// * If the chain is *ahead* of `target_iteration`: try the skipped-keys
///   cache; otherwise this is a duplicate or too-old message.
/// * If the chain is *at* `target_iteration`: derive directly, then advance.
/// * If the chain is *behind* `target_iteration`: walk forward, caching each
///   skipped iteration's message key, then derive at the target and advance
///   the chain past it.
fn walk_to_iteration(
    state: &mut crate::sender_key::SenderKeyState,
    target_iteration: u32,
) -> Result<SenderMessageKey, SignalProtocolError> {
    let current = state.chain_key.iteration;

    if current > target_iteration {
        // Older message — only valid if we cached the skipped key.
        return state
            .remove_sender_message_key(target_iteration)
            .ok_or(SignalProtocolError::DuplicateMessage {
                chain: current,
                counter: target_iteration,
            });
    }

    // current <= target_iteration — bound the forward jump.
    if target_iteration - current > MAX_FORWARD_JUMP {
        return Err(SignalProtocolError::TooFarIntoFuture);
    }

    // Walk forward. After the loop, `state.chain_key.iteration == target_iteration`.
    while state.chain_key.iteration < target_iteration {
        let mk = state.chain_key.sender_message_key();
        state.add_sender_message_key(mk);
        let next = state.chain_key.next();
        state.set_chain_key(next);
    }

    // At the target now — derive, then advance the chain past it so we never
    // hand the same key out twice.
    let mk = state.chain_key.sender_message_key();
    let next = state.chain_key.next();
    state.set_chain_key(next);
    Ok(mk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::{SenderKeyName, SignalAddress};
    use crate::sender_key::SenderKeyState;
    use crate::sender_key_record::SenderKeyRecord;
    use wha_crypto::KeyPair;

    fn fresh_own_record(key_id: u32) -> SenderKeyRecord {
        // Deterministic-ish test key material — the values themselves don't
        // matter, only that the state has a private signing key so we can
        // encrypt.
        let chain_seed = [11u8; 32];
        let signing = KeyPair::from_private([
            0x18, 0x77, 0x21, 0x4f, 0x2e, 0x73, 0x10, 0x4d, 0x83, 0x40, 0x66, 0x42, 0x9c, 0x55,
            0x09, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc,
            0xba, 0x98, 0x76, 0x54,
        ]);
        let st = SenderKeyState::new_own(key_id, 0, chain_seed, signing);
        let mut rec = SenderKeyRecord::new();
        rec.set_sender_key_state(st);
        rec
    }

    fn sender_name() -> SenderKeyName {
        SenderKeyName::new("group@g.us", SignalAddress::new("1234", 0))
    }

    #[test]
    fn encrypt_self_decrypt_round_trip() {
        let mut sender_rec = fresh_own_record(42);
        let plaintext = b"hello whatsapp group";

        let wire = SenderKeyMessage::encrypt(&mut sender_rec, plaintext).unwrap();

        // Receiver-side: clone of the original (pre-encrypt) record. The
        // chain index is still 0, matching the message's iteration field.
        let mut recv_rec = fresh_own_record(42);
        let name = sender_name();
        let back = SenderKeyMessage::decrypt(&mut recv_rec, &name, &wire).unwrap();
        assert_eq!(back, plaintext);
    }

    #[test]
    fn out_of_order_in_group_decrypts() {
        // Sender encrypts three messages in order. Receiver gets them as
        // 2nd, 1st, 3rd. All three must decrypt successfully.
        let mut sender_rec = fresh_own_record(7);
        let m1 = SenderKeyMessage::encrypt(&mut sender_rec, b"first").unwrap();
        let m2 = SenderKeyMessage::encrypt(&mut sender_rec, b"second").unwrap();
        let m3 = SenderKeyMessage::encrypt(&mut sender_rec, b"third").unwrap();

        let mut recv_rec = fresh_own_record(7);
        let name = sender_name();

        // Decrypt 2 first — should work, caching the iteration-0 skipped key.
        let p2 = SenderKeyMessage::decrypt(&mut recv_rec, &name, &m2).unwrap();
        assert_eq!(p2, b"second");
        // Now decrypt 1 — pulled from skipped cache.
        let p1 = SenderKeyMessage::decrypt(&mut recv_rec, &name, &m1).unwrap();
        assert_eq!(p1, b"first");
        // Finally 3 — chain advanced past 2 already, so this is a normal
        // forward step.
        let p3 = SenderKeyMessage::decrypt(&mut recv_rec, &name, &m3).unwrap();
        assert_eq!(p3, b"third");
    }

    #[test]
    fn bad_signature_fails() {
        let mut sender_rec = fresh_own_record(99);
        let mut wire = SenderKeyMessage::encrypt(&mut sender_rec, b"payload").unwrap();
        // Flip a bit in the last byte — that lives inside the signature.
        let last = wire.len() - 1;
        wire[last] ^= 0x01;

        let mut recv_rec = fresh_own_record(99);
        let name = sender_name();
        let err = SenderKeyMessage::decrypt(&mut recv_rec, &name, &wire).unwrap_err();
        assert!(matches!(err, SignalProtocolError::BadSignature));
    }

    #[test]
    fn wrong_version_byte_fails() {
        let mut sender_rec = fresh_own_record(1);
        let mut wire = SenderKeyMessage::encrypt(&mut sender_rec, b"payload").unwrap();
        wire[0] = 0x32; // not 0x33

        let mut recv_rec = fresh_own_record(1);
        let name = sender_name();
        let err = SenderKeyMessage::decrypt(&mut recv_rec, &name, &wire).unwrap_err();
        assert!(matches!(err, SignalProtocolError::UnsupportedVersion(0x32)));
    }

    #[test]
    fn body_codec_round_trip() {
        let body = encode_body(123, 456, b"opaque");
        let parsed = decode_body(&body).unwrap();
        assert_eq!(parsed.id, 123);
        assert_eq!(parsed.iteration, 456);
        assert_eq!(parsed.ciphertext, b"opaque");
    }

    #[test]
    fn empty_record_decrypt_errors() {
        let mut rec = SenderKeyRecord::new();
        let name = sender_name();
        let buf = vec![0x33u8; 1 + SIGNATURE_LEN];
        let err = SenderKeyMessage::decrypt(&mut rec, &name, &buf).unwrap_err();
        assert!(matches!(err, SignalProtocolError::InvalidMessage(_)));
    }

    #[test]
    fn unknown_key_id_errors() {
        let mut sender_rec = fresh_own_record(1);
        let wire = SenderKeyMessage::encrypt(&mut sender_rec, b"payload").unwrap();
        let mut recv_rec = fresh_own_record(2); // different key_id
        let name = sender_name();
        let err = SenderKeyMessage::decrypt(&mut recv_rec, &name, &wire).unwrap_err();
        assert!(matches!(err, SignalProtocolError::InvalidMessage(_)));
    }
}
