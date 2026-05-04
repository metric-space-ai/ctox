//! Reporting-token construction — port of
//! `_upstream/whatsmeow/reportingtoken.go`.
//!
//! WhatsApp attaches a "reporting token" to outbound messages so the server
//! can later verify a user-submitted abuse report claims the message it says
//! it does without exposing the plaintext. The token is the first 16 bytes
//! of `HMAC-SHA256(reporting_secret, canonicalised_subset_of_protobuf)`.
//!
//! The canonicalisation step (`extract_reporting_token_content`) walks the
//! `waE2E.Message` protobuf wire bytes, keeping only the fields whose
//! numbers appear in the static config and re-serialising them in
//! ascending field-number order. The config is exactly upstream's
//! `reportingfields.json` (also kept verbatim next to this file).
//!
//! The public API takes `(message_id, contents)` for ergonomics; the
//! upstream call site additionally derives `reporting_secret` from the
//! message's master secret + JIDs (`generateMsgSecretKey` with
//! `EncSecretReportToken`). For full parity the lower-level
//! [`build_reporting_token_with_secret`] takes the secret directly — and
//! [`build_reporting_token`] uses `message_id` as the HMAC key, which is
//! deterministic for tests but not what the wire actually carries. Callers
//! talking to the live server should use the `_with_secret` variant.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Static reporting-fields config — mirror of `reportingfields.json` upstream.
//
// Each entry says which protobuf field number to keep, whether its bytes
// payload is itself a sub-message we should recurse into, and which inner
// fields to keep when recursing. `m: true` from the JSON means "this is a
// message, recurse with the same top-level config" — we model that by
// pointing back into [`TOP_LEVEL`] via [`InnerConfig::TopLevel`].
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ReportingField {
    field_number: u32,
    inner: InnerConfig,
}

#[derive(Debug)]
enum InnerConfig {
    /// Scalar field — keep verbatim.
    Scalar,
    /// Sub-message; recurse with the given child config.
    Children(&'static [ReportingField]),
    /// Sub-message; recurse with the same top-level config as the parent.
    TopLevel,
}

/// Static reporting-fields tree. The numbers and nesting are copied
/// byte-for-byte from `reportingfields.json` upstream.
static TOP_LEVEL: &[ReportingField] = &[
    ReportingField { field_number: 1, inner: InnerConfig::Scalar },
    ReportingField { field_number: 3, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 3, inner: InnerConfig::Scalar },
        ReportingField { field_number: 8, inner: InnerConfig::Scalar },
        ReportingField { field_number: 11, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 25, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 4, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ReportingField { field_number: 16, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 5, inner: InnerConfig::Children(&[
        ReportingField { field_number: 3, inner: InnerConfig::Scalar },
        ReportingField { field_number: 4, inner: InnerConfig::Scalar },
        ReportingField { field_number: 5, inner: InnerConfig::Scalar },
        ReportingField { field_number: 16, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 6, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 30, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 7, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 7, inner: InnerConfig::Scalar },
        ReportingField { field_number: 10, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 20, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 8, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 7, inner: InnerConfig::Scalar },
        ReportingField { field_number: 9, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 21, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 9, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 6, inner: InnerConfig::Scalar },
        ReportingField { field_number: 7, inner: InnerConfig::Scalar },
        ReportingField { field_number: 13, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 20, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 12, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 14, inner: InnerConfig::TopLevel },
        ReportingField { field_number: 15, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 18, inner: InnerConfig::Children(&[
        ReportingField { field_number: 6, inner: InnerConfig::Scalar },
        ReportingField { field_number: 16, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 26, inner: InnerConfig::Children(&[
        ReportingField { field_number: 4, inner: InnerConfig::Scalar },
        ReportingField { field_number: 5, inner: InnerConfig::Scalar },
        ReportingField { field_number: 8, inner: InnerConfig::Scalar },
        ReportingField { field_number: 13, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 28, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 4, inner: InnerConfig::Scalar },
        ReportingField { field_number: 5, inner: InnerConfig::Scalar },
        ReportingField { field_number: 6, inner: InnerConfig::Scalar },
        ReportingField { field_number: 7, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 37, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 49, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 3, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 5, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 8, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 53, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 55, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 58, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 59, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 60, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 3, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 5, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 8, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 64, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 3, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 5, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 8, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
            ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 66, inner: InnerConfig::Children(&[
        ReportingField { field_number: 2, inner: InnerConfig::Scalar },
        ReportingField { field_number: 6, inner: InnerConfig::Scalar },
        ReportingField { field_number: 7, inner: InnerConfig::Scalar },
        ReportingField { field_number: 13, inner: InnerConfig::Scalar },
        ReportingField { field_number: 17, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 20, inner: InnerConfig::Scalar },
    ])},
    ReportingField { field_number: 74, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 87, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 88, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ReportingField { field_number: 2, inner: InnerConfig::Children(&[
            ReportingField { field_number: 1, inner: InnerConfig::Scalar },
        ])},
        ReportingField { field_number: 3, inner: InnerConfig::Children(&[
            ReportingField { field_number: 21, inner: InnerConfig::Scalar },
            ReportingField { field_number: 22, inner: InnerConfig::Scalar },
        ])},
    ])},
    ReportingField { field_number: 92, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 93, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
    ReportingField { field_number: 94, inner: InnerConfig::Children(&[
        ReportingField { field_number: 1, inner: InnerConfig::TopLevel },
    ])},
];

// ---------------------------------------------------------------------------
// Protobuf wire decoding helpers — port of the Go implementation in upstream.
// ---------------------------------------------------------------------------

const WIRE_VARINT: u32 = 0;
const WIRE_64BIT: u32 = 1;
const WIRE_BYTES: u32 = 2;
const WIRE_32BIT: u32 = 5;

/// Decode an unsigned LEB128 varint. Returns `(value, bytes_read)` or `None`
/// on EOF / overflow. Matches `binary.Uvarint` semantics.
fn read_uvarint(data: &[u8]) -> Option<(u64, usize)> {
    let mut x: u64 = 0;
    let mut s: u32 = 0;
    for (i, b) in data.iter().enumerate() {
        if i == 10 {
            return None; // overflow
        }
        if *b < 0x80 {
            if i == 9 && *b > 1 {
                return None;
            }
            return Some((x | ((*b as u64) << s), i + 1));
        }
        x |= ((*b & 0x7f) as u64) << s;
        s += 7;
    }
    None
}

fn put_uvarint(buf: &mut Vec<u8>, mut x: u64) {
    while x >= 0x80 {
        buf.push((x as u8) | 0x80);
        x >>= 7;
    }
    buf.push(x as u8);
}

fn config_for_field(fields: &[ReportingField], num: u32) -> Option<&ReportingField> {
    fields.iter().find(|f| f.field_number == num)
}

/// Walk `data` as a protobuf message, keep only fields whose numbers appear in
/// `config`, sort the kept fields by number, and concatenate their on-the-wire
/// bytes. Recurses for sub-messages. Pure port of upstream's
/// `extractReportingTokenContent`.
pub fn extract_reporting_token_content_at_top(data: &[u8]) -> Vec<u8> {
    extract(data, TOP_LEVEL)
}

fn extract(data: &[u8], config: &[ReportingField]) -> Vec<u8> {
    let mut kept: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let (tag, tag_len) = match read_uvarint(&data[i..]) {
            Some(t) => t,
            None => break,
        };
        let field_num = (tag >> 3) as u32;
        let wire_type = (tag & 0x7) as u32;
        let field_start = i;
        i += tag_len;

        let cfg = config_for_field(config, field_num);

        if cfg.is_none() {
            // Skip the field entirely.
            match wire_type {
                WIRE_VARINT => {
                    let (_, n) = match read_uvarint(&data[i..]) {
                        Some(v) => v,
                        None => return Vec::new(),
                    };
                    i += n;
                }
                WIRE_64BIT => i += 8,
                WIRE_BYTES => {
                    let (l, n) = match read_uvarint(&data[i..]) {
                        Some(v) => v,
                        None => return Vec::new(),
                    };
                    i += n + l as usize;
                }
                WIRE_32BIT => i += 4,
                _ => return Vec::new(),
            }
            continue;
        }
        let fcfg = cfg.unwrap();

        match wire_type {
            WIRE_VARINT => {
                let (_, n) = match read_uvarint(&data[i..]) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
                i += n;
                kept.push((field_num, data[field_start..i].to_vec()));
            }
            WIRE_64BIT => {
                i += 8;
                kept.push((field_num, data[field_start..i].to_vec()));
            }
            WIRE_BYTES => {
                let (l, n) = match read_uvarint(&data[i..]) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
                let val_start = i + n;
                let val_end = val_start + l as usize;
                if val_end > data.len() {
                    return Vec::new();
                }
                let recurse_into = match &fcfg.inner {
                    InnerConfig::Scalar => None,
                    InnerConfig::Children(c) => Some(*c),
                    InnerConfig::TopLevel => Some(TOP_LEVEL),
                };
                if let Some(child_cfg) = recurse_into {
                    let sub = extract(&data[val_start..val_end], child_cfg);
                    if !sub.is_empty() {
                        let mut buf =
                            Vec::with_capacity(tag_len + sub.len() + 5);
                        put_uvarint(&mut buf, tag);
                        put_uvarint(&mut buf, sub.len() as u64);
                        buf.extend_from_slice(&sub);
                        kept.push((field_num, buf));
                    }
                } else {
                    kept.push((field_num, data[field_start..val_end].to_vec()));
                }
                i = val_end;
            }
            WIRE_32BIT => {
                i += 4;
                kept.push((field_num, data[field_start..i].to_vec()));
            }
            _ => return Vec::new(),
        }
    }
    kept.sort_by_key(|(n, _)| *n);
    let mut out = Vec::new();
    for (_, b) in kept {
        out.extend_from_slice(&b);
    }
    out
}

// ---------------------------------------------------------------------------
// Public API.
// ---------------------------------------------------------------------------

/// Build a 16-byte reporting token using `message_id` as the HMAC key.
///
/// This is a deterministic derivation suitable for tests and for callers
/// that don't have access to the per-message reporting secret. The wire
/// shape produced by upstream uses [`build_reporting_token_with_secret`]
/// instead — pass the per-message reporting secret derived via
/// `crate::msgsecret` (`MessageSecretUseCase::ReportToken`).
pub fn build_reporting_token(message_id: &str, contents: &[u8]) -> Vec<u8> {
    build_reporting_token_with_secret(message_id.as_bytes(), contents)
}

/// Full upstream-equivalent token: HMAC-SHA256 of the canonicalised wire
/// subset of `contents`, keyed by `secret`, truncated to 16 bytes.
pub fn build_reporting_token_with_secret(secret: &[u8], contents: &[u8]) -> Vec<u8> {
    let canonical = extract_reporting_token_content_at_top(contents);
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&canonical);
    let out = mac.finalize().into_bytes();
    out[..16].to_vec()
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small protobuf-shaped message:
    /// - field 1 (varint = 7) — kept (top-level f:1 is scalar)
    /// - field 2 (varint = 99) — dropped (no f:2 at top level)
    /// - field 1 (string "hi") — encoded as wire 2 over field 1; but the JSON
    ///   has top-level f:1 as scalar without sub-message, so this is kept
    ///   verbatim too.
    fn synth_msg() -> Vec<u8> {
        let mut buf = Vec::new();
        // tag = (1<<3) | 0 = 0x08, varint 7.
        put_uvarint(&mut buf, (1 << 3) | 0);
        put_uvarint(&mut buf, 7);
        // tag = (2<<3) | 0 = 0x10, varint 99 — should be dropped.
        put_uvarint(&mut buf, (2 << 3) | 0);
        put_uvarint(&mut buf, 99);
        buf
    }

    #[test]
    fn extract_drops_unknown_top_level_fields() {
        let msg = synth_msg();
        let canonical = extract_reporting_token_content_at_top(&msg);
        // Only the field-1 varint should survive.
        let mut expected = Vec::new();
        put_uvarint(&mut expected, (1 << 3) | 0);
        put_uvarint(&mut expected, 7);
        assert_eq!(canonical, expected);
    }

    #[test]
    fn build_reporting_token_is_deterministic_and_16_bytes() {
        let msg = synth_msg();
        let a = build_reporting_token("MSGID", &msg);
        let b = build_reporting_token("MSGID", &msg);
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);

        // Different message id → different token.
        let c = build_reporting_token("OTHER", &msg);
        assert_ne!(a, c);
        assert_eq!(c.len(), 16);

        // Different content → different token. Modify a kept field (f:1)
        // so the canonical bytes — and therefore the HMAC — differ.
        let mut diff = Vec::new();
        put_uvarint(&mut diff, (1 << 3) | 0);
        put_uvarint(&mut diff, 8); // changed from 7 to 8
        let d = build_reporting_token("MSGID", &diff);
        assert_ne!(a, d);
    }

    #[test]
    fn extract_keeps_field_1_string_value_top_level() {
        // Field 1, wire BYTES, payload "hi".
        let mut msg = Vec::new();
        let tag = (1 << 3) | 2;
        put_uvarint(&mut msg, tag);
        put_uvarint(&mut msg, 2);
        msg.extend_from_slice(b"hi");
        // Even though top-level f:1 is Scalar, we don't recurse for BYTES;
        // the field is kept verbatim.
        let canonical = extract_reporting_token_content_at_top(&msg);
        assert_eq!(canonical, msg);
    }

    #[test]
    fn empty_message_yields_empty_canonical_and_keyed_hmac() {
        let canonical = extract_reporting_token_content_at_top(&[]);
        assert!(canonical.is_empty());

        // Two empty messages with the same key yield the same 16 bytes.
        let a = build_reporting_token("k", &[]);
        let b = build_reporting_token("k", &[]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }
}
