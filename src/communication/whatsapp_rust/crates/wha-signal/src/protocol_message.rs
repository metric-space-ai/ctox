//! Wire format for `SignalMessage` (whisper) and `PreKeySignalMessage`
//! (a.k.a. PreKeyWhisperMessage).
//!
//! Faithfully ported from `go.mau.fi/libsignal/protocol/SignalMessage.go`,
//! `protocol/PreKeySignalMessage.go` and the protobuf shape in
//! `serialize/WhisperTextProtocol.proto`.
//!
//! Layout (all multi-byte integers little-endian per protobuf varint rules):
//!
//! ```text
//! SignalMessage         := version_byte || protobuf_body || mac8
//! PreKeySignalMessage   := version_byte || protobuf_body
//! ```
//!
//! `version_byte = (current_version << 4) | message_version` (0x33 for v3).
//! libsignal historically prepends the *ASCII digit* of the version (`'3' = 0x33`),
//! which happens to coincide with the high-nibble/low-nibble packing for v3
//! and is what the deserialiser expects (see `highBitsToInt`).
//!
//! MAC = HMAC-SHA256(mac_key, sender_id33 || receiver_id33 || version_byte || pb_body)[..8]
//! where `*_id33` is `0x05 || pub_key32` to match libsignal's `DjbECPublicKey.Serialize()`.
//!
//! The protobuf body for `SignalMessage`:
//! ```text
//! 1: bytes  ratchetKey       (33 bytes: 0x05 || pub32)
//! 2: uint32 counter
//! 3: uint32 previousCounter
//! 4: bytes  ciphertext
//! ```
//! and for `PreKeySignalMessage`:
//! ```text
//! 1: uint32 preKeyId          (optional)
//! 2: bytes  baseKey           (33 bytes)
//! 3: bytes  identityKey       (33 bytes)
//! 4: bytes  message           (serialised SignalMessage)
//! 5: uint32 registrationId
//! 6: uint32 signedPreKeyId
//! ```
//!
//! We hand-roll the protobuf encoding rather than pulling in `prost` here:
//! the schema is tiny and stable, and avoiding the dep keeps `wha-signal`'s
//! dependency surface unchanged.

use wha_crypto::hmac_sha256_concat;

use crate::SignalProtocolError;

/// Current Signal protocol version (3). All current WhatsApp / libsignal
/// peers use this; we don't try to negotiate down.
pub const CURRENT_VERSION: u8 = 3;
const MAC_LENGTH: usize = 8;
const DJB_TYPE: u8 = 0x05;

// ---------- protobuf primitives -----------------------------------------------

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

// ---------- SignalMessage (WhisperMessage) -----------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalMessage {
    pub version: u8,
    pub sender_ratchet_key: [u8; 32],
    pub counter: u32,
    pub previous_counter: u32,
    pub ciphertext: Vec<u8>,
    /// 8-byte truncated MAC. Set after [`SignalMessage::with_mac`].
    pub mac: [u8; MAC_LENGTH],
}

impl SignalMessage {
    /// Build a fresh `SignalMessage`, computing the MAC over the canonical
    /// libsignal input (sender id || receiver id || version_byte || pb_body).
    pub fn new(
        version: u8,
        sender_ratchet_key: [u8; 32],
        counter: u32,
        previous_counter: u32,
        mac_key: &[u8],
        ciphertext: Vec<u8>,
        sender_identity: &[u8; 32],
        receiver_identity: &[u8; 32],
    ) -> Result<Self, SignalProtocolError> {
        if version != CURRENT_VERSION {
            return Err(SignalProtocolError::UnsupportedVersion(version));
        }
        let body = encode_signal_message_body(&sender_ratchet_key, counter, previous_counter, &ciphertext);
        let version_byte = version_byte(version);
        let sender_id33 = id33(sender_identity);
        let receiver_id33 = id33(receiver_identity);
        let mac_full = hmac_sha256_concat(
            mac_key,
            &[
                &sender_id33,
                &receiver_id33,
                &[version_byte][..],
                &body,
            ],
        );
        let mut mac = [0u8; MAC_LENGTH];
        mac.copy_from_slice(&mac_full[..MAC_LENGTH]);

        Ok(Self {
            version,
            sender_ratchet_key,
            counter,
            previous_counter,
            ciphertext,
            mac,
        })
    }

    /// Serialise to the `version_byte || pb_body || mac8` wire form.
    pub fn serialize(&self) -> Vec<u8> {
        let body = encode_signal_message_body(
            &self.sender_ratchet_key,
            self.counter,
            self.previous_counter,
            &self.ciphertext,
        );
        let mut out = Vec::with_capacity(1 + body.len() + MAC_LENGTH);
        out.push(version_byte(self.version));
        out.extend_from_slice(&body);
        out.extend_from_slice(&self.mac);
        out
    }

    /// Parse the wire form. Does NOT verify the MAC — call
    /// [`Self::verify_mac`] separately so the chain key can be computed
    /// in between.
    pub fn deserialize(serialized: &[u8]) -> Result<Self, SignalProtocolError> {
        if serialized.len() < 1 + MAC_LENGTH {
            return Err(SignalProtocolError::InvalidMessage("signal message too short"));
        }
        let v_byte = serialized[0];
        let version = (v_byte >> 4) & 0x0F;
        if version != CURRENT_VERSION {
            return Err(SignalProtocolError::UnsupportedVersion(version));
        }
        let body_end = serialized.len() - MAC_LENGTH;
        let body = &serialized[1..body_end];
        let mut mac = [0u8; MAC_LENGTH];
        mac.copy_from_slice(&serialized[body_end..]);

        let pb = decode_signal_message_body(body)?;
        Ok(Self {
            version,
            sender_ratchet_key: pb.sender_ratchet_key,
            counter: pb.counter,
            previous_counter: pb.previous_counter,
            ciphertext: pb.ciphertext,
            mac,
        })
    }

    /// Re-compute the MAC and constant-time-compare against `self.mac`.
    pub fn verify_mac(
        &self,
        mac_key: &[u8],
        sender_identity: &[u8; 32],
        receiver_identity: &[u8; 32],
    ) -> Result<(), SignalProtocolError> {
        let body = encode_signal_message_body(
            &self.sender_ratchet_key,
            self.counter,
            self.previous_counter,
            &self.ciphertext,
        );
        let version_byte = version_byte(self.version);
        let sender_id33 = id33(sender_identity);
        let receiver_id33 = id33(receiver_identity);
        let full = hmac_sha256_concat(
            mac_key,
            &[
                &sender_id33,
                &receiver_id33,
                &[version_byte][..],
                &body,
            ],
        );
        let expected = &full[..MAC_LENGTH];
        // Constant-time compare via subtle-style accumulator to keep the
        // dependency surface narrow.
        let mut diff: u8 = 0;
        for (a, b) in expected.iter().zip(self.mac.iter()) {
            diff |= a ^ b;
        }
        if diff == 0 {
            Ok(())
        } else {
            Err(SignalProtocolError::BadMac)
        }
    }
}

struct SignalMessageBody {
    sender_ratchet_key: [u8; 32],
    counter: u32,
    previous_counter: u32,
    ciphertext: Vec<u8>,
}

fn encode_signal_message_body(
    sender_ratchet_key: &[u8; 32],
    counter: u32,
    previous_counter: u32,
    ciphertext: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + ciphertext.len());
    let serialized_pub = id33(sender_ratchet_key);
    put_bytes_field(&mut out, 1, &serialized_pub);
    put_uint32_field(&mut out, 2, counter);
    put_uint32_field(&mut out, 3, previous_counter);
    put_bytes_field(&mut out, 4, ciphertext);
    out
}

fn decode_signal_message_body(buf: &[u8]) -> Result<SignalMessageBody, SignalProtocolError> {
    let mut sender_ratchet_key: Option<[u8; 32]> = None;
    let mut counter: u32 = 0;
    let mut previous_counter: u32 = 0;
    let mut ciphertext: Option<Vec<u8>> = None;

    let mut i = 0;
    while i < buf.len() {
        let (tag, n) = read_varint(buf, i)?;
        i += n;
        let field = (tag >> 3) as u32;
        let wire = (tag & 0x07) as u32;
        match (field, wire) {
            (1, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i += n;
                let end = i.checked_add(len as usize).ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                if end > buf.len() {
                    return Err(SignalProtocolError::InvalidMessage("ratchetKey truncated"));
                }
                let raw = &buf[i..end];
                i = end;
                sender_ratchet_key = Some(parse_djb_pub(raw)?);
            }
            (2, 0) => {
                let (v, n) = read_varint(buf, i)?;
                i += n;
                counter = v as u32;
            }
            (3, 0) => {
                let (v, n) = read_varint(buf, i)?;
                i += n;
                previous_counter = v as u32;
            }
            (4, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i += n;
                let end = i.checked_add(len as usize).ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                if end > buf.len() {
                    return Err(SignalProtocolError::InvalidMessage("ciphertext truncated"));
                }
                ciphertext = Some(buf[i..end].to_vec());
                i = end;
            }
            _ => skip_unknown(buf, &mut i, wire)?,
        }
    }
    Ok(SignalMessageBody {
        sender_ratchet_key: sender_ratchet_key
            .ok_or(SignalProtocolError::InvalidMessage("missing ratchetKey"))?,
        counter,
        previous_counter,
        ciphertext: ciphertext.ok_or(SignalProtocolError::InvalidMessage("missing ciphertext"))?,
    })
}

// ---------- PreKeySignalMessage ----------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreKeySignalMessage {
    pub version: u8,
    pub registration_id: u32,
    pub pre_key_id: Option<u32>,
    pub signed_pre_key_id: u32,
    pub base_key: [u8; 32],
    pub identity_key: [u8; 32],
    /// Embedded SignalMessage, already serialised (`version || pb || mac8`).
    pub message: Vec<u8>,
}

impl PreKeySignalMessage {
    pub fn new(
        version: u8,
        registration_id: u32,
        pre_key_id: Option<u32>,
        signed_pre_key_id: u32,
        base_key: [u8; 32],
        identity_key: [u8; 32],
        message: Vec<u8>,
    ) -> Self {
        Self {
            version,
            registration_id,
            pre_key_id,
            signed_pre_key_id,
            base_key,
            identity_key,
            message,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let body = encode_prekey_signal_message_body(self);
        let mut out = Vec::with_capacity(1 + body.len());
        out.push(version_byte(self.version));
        out.extend_from_slice(&body);
        out
    }

    pub fn deserialize(serialized: &[u8]) -> Result<Self, SignalProtocolError> {
        if serialized.is_empty() {
            return Err(SignalProtocolError::InvalidMessage("prekey message empty"));
        }
        let v_byte = serialized[0];
        let version = (v_byte >> 4) & 0x0F;
        if version != CURRENT_VERSION {
            return Err(SignalProtocolError::UnsupportedVersion(version));
        }
        decode_prekey_signal_message_body(version, &serialized[1..])
    }
}

fn encode_prekey_signal_message_body(m: &PreKeySignalMessage) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + m.message.len());
    if let Some(id) = m.pre_key_id {
        // Field 1: preKeyId. libsignal omits this if 0/empty.
        if id != 0 {
            put_uint32_field(&mut out, 1, id);
        }
    }
    put_bytes_field(&mut out, 2, &id33(&m.base_key));
    put_bytes_field(&mut out, 3, &id33(&m.identity_key));
    put_bytes_field(&mut out, 4, &m.message);
    put_uint32_field(&mut out, 5, m.registration_id);
    put_uint32_field(&mut out, 6, m.signed_pre_key_id);
    out
}

fn decode_prekey_signal_message_body(
    version: u8,
    buf: &[u8],
) -> Result<PreKeySignalMessage, SignalProtocolError> {
    let mut registration_id: u32 = 0;
    let mut pre_key_id: Option<u32> = None;
    let mut signed_pre_key_id: u32 = 0;
    let mut base_key: Option<[u8; 32]> = None;
    let mut identity_key: Option<[u8; 32]> = None;
    let mut message: Option<Vec<u8>> = None;

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
                pre_key_id = Some(v as u32);
            }
            (2, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i += n;
                let end = i.checked_add(len as usize).ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                if end > buf.len() {
                    return Err(SignalProtocolError::InvalidMessage("baseKey truncated"));
                }
                base_key = Some(parse_djb_pub(&buf[i..end])?);
                i = end;
            }
            (3, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i += n;
                let end = i.checked_add(len as usize).ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                if end > buf.len() {
                    return Err(SignalProtocolError::InvalidMessage("identityKey truncated"));
                }
                identity_key = Some(parse_djb_pub(&buf[i..end])?);
                i = end;
            }
            (4, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i += n;
                let end = i.checked_add(len as usize).ok_or(SignalProtocolError::InvalidMessage("len overflow"))?;
                if end > buf.len() {
                    return Err(SignalProtocolError::InvalidMessage("message truncated"));
                }
                message = Some(buf[i..end].to_vec());
                i = end;
            }
            (5, 0) => {
                let (v, n) = read_varint(buf, i)?;
                i += n;
                registration_id = v as u32;
            }
            (6, 0) => {
                let (v, n) = read_varint(buf, i)?;
                i += n;
                signed_pre_key_id = v as u32;
            }
            _ => skip_unknown(buf, &mut i, wire)?,
        }
    }

    Ok(PreKeySignalMessage {
        version,
        registration_id,
        pre_key_id,
        signed_pre_key_id,
        base_key: base_key.ok_or(SignalProtocolError::InvalidMessage("missing baseKey"))?,
        identity_key: identity_key.ok_or(SignalProtocolError::InvalidMessage("missing identityKey"))?,
        message: message.ok_or(SignalProtocolError::InvalidMessage("missing inner message"))?,
    })
}

// ---------- helpers ----------------------------------------------------------

fn version_byte(version: u8) -> u8 {
    ((CURRENT_VERSION & 0x0F) << 4) | (version & 0x0F)
}

fn id33(pub32: &[u8; 32]) -> [u8; 33] {
    let mut out = [0u8; 33];
    out[0] = DJB_TYPE;
    out[1..].copy_from_slice(pub32);
    out
}

fn parse_djb_pub(raw: &[u8]) -> Result<[u8; 32], SignalProtocolError> {
    if raw.len() != 33 {
        return Err(SignalProtocolError::InvalidMessage("pub key wrong length"));
    }
    if raw[0] != DJB_TYPE {
        return Err(SignalProtocolError::InvalidMessage("pub key wrong type"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&raw[1..]);
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_signal_message() {
        let mac_key = [9u8; 32];
        let sender_id = [1u8; 32];
        let receiver_id = [2u8; 32];
        let ratchet = [3u8; 32];
        let ciphertext = b"opaque ciphertext".to_vec();
        let m = SignalMessage::new(
            CURRENT_VERSION,
            ratchet,
            5,
            4,
            &mac_key,
            ciphertext.clone(),
            &sender_id,
            &receiver_id,
        )
        .unwrap();
        let bytes = m.serialize();
        // Reverse and verify.
        let parsed = SignalMessage::deserialize(&bytes).unwrap();
        assert_eq!(parsed.sender_ratchet_key, ratchet);
        assert_eq!(parsed.counter, 5);
        assert_eq!(parsed.previous_counter, 4);
        assert_eq!(parsed.ciphertext, ciphertext);
        parsed.verify_mac(&mac_key, &sender_id, &receiver_id).unwrap();
    }

    #[test]
    fn version_byte_is_0x33_for_v3() {
        assert_eq!(version_byte(3), 0x33);
    }

    #[test]
    fn round_trip_prekey_signal_message() {
        let inner = SignalMessage::new(
            CURRENT_VERSION,
            [3u8; 32],
            0,
            0,
            &[7u8; 32],
            b"ciphertext".to_vec(),
            &[1u8; 32],
            &[2u8; 32],
        )
        .unwrap();
        let pkm = PreKeySignalMessage::new(
            CURRENT_VERSION,
            42,
            Some(11),
            22,
            [4u8; 32],
            [5u8; 32],
            inner.serialize(),
        );
        let bytes = pkm.serialize();
        let parsed = PreKeySignalMessage::deserialize(&bytes).unwrap();
        assert_eq!(parsed.registration_id, 42);
        assert_eq!(parsed.pre_key_id, Some(11));
        assert_eq!(parsed.signed_pre_key_id, 22);
        assert_eq!(parsed.base_key, [4u8; 32]);
        assert_eq!(parsed.identity_key, [5u8; 32]);
        // Inner message survives.
        let inner_back = SignalMessage::deserialize(&parsed.message).unwrap();
        assert_eq!(inner_back, inner);
    }

    #[test]
    fn rejects_wrong_version() {
        let mut buf = vec![0x44]; // v4
        buf.extend_from_slice(&[0u8; 16]);
        assert!(matches!(
            SignalMessage::deserialize(&buf),
            Err(SignalProtocolError::UnsupportedVersion(_))
        ));
    }
}
