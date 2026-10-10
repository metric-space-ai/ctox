//! base64url, in both of the flavours this crate needs.
//!
//! They are not interchangeable, and mixing them up is a silent failure: a
//! verifier that ignores padding accepts a string a stricter one rejects, and
//! vice versa. Two call sites, two requirements:
//!
//! * **padded** — act's V4 artifact signature, which uses Go's
//!   `base64.URLEncoding`. The `=` survives into the signed URL, and the
//!   client echoes it back unchanged.
//! * **raw** — the JWT in act's authorization token, where RFC 7515 requires
//!   `base64.RawURLEncoding` and a stray `=` would corrupt the signature input.
//!
//! Hand-rolled rather than a dependency: the alphabet is a line, padding is
//! three branches, and the reason for not using the `base64` crate is that its
//! *engine* API is where these two modes differ, which is exactly the part
//! worth not delegating.

/// base64url with `=` padding — Go's `base64.URLEncoding`.
pub fn encode_padded(data: &[u8]) -> String {
    encode(data, true)
}

/// base64url without padding — Go's `base64.RawURLEncoding`, and RFC 7515.
pub fn encode_raw(data: &[u8]) -> String {
    encode(data, false)
}

fn encode(data: &[u8], padded: bool) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let triple = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 0x3f] as char);
        } else if padded {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 0x3f] as char);
        } else if padded {
            out.push('=');
        }
    }
    out
}

/// Decodes either flavour: `=` is skipped, and the standard alphabet is
/// accepted too, which is what Go's `DecodeString` does.
///
/// Bytes that are not part of the alphabet are skipped rather than rejected.
/// Both call sites discard Go's decode error, so a malformed `sig` becomes a
/// short byte string that then fails comparison instead of a 400.
pub fn decode(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for byte in value.bytes() {
        if byte == b'=' {
            continue;
        }
        let Some(index) = value_of(byte) else { continue };
        acc = (acc << 6) | index as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn value_of(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        // Both the URL-safe and the standard spelling, because Go's
        // `URLEncoding.DecodeString` falls back to the standard alphabet.
        b'-' | b'+' => Some(62),
        b'_' | b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Go's `encoding/base64` test vectors, so the padding is checked against
    /// something other than my own expectation.
    #[test]
    fn the_padded_encoding_matches_go() {
        for (raw, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_padded(raw.as_bytes()), want, "{raw:?}");
        }
        // Bytes whose low six bits are all set, which is where the URL-safe
        // alphabet differs from the standard one.
        assert_eq!(encode_padded(&[0xfb, 0xff, 0xbf]), "-_-_");
        assert_eq!(encode_padded(&[0xfb, 0xff]), "-_8=");
    }

    #[test]
    fn the_raw_encoding_omits_padding() {
        for (raw, want) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
        ] {
            assert_eq!(encode_raw(raw.as_bytes()), want, "{raw:?}");
        }
    }

    #[test]
    fn both_flavours_decode_to_the_same_bytes() {
        let data: Vec<u8> = (0u8..=255).collect();
        assert_eq!(decode(&encode_padded(&data)), data);
        assert_eq!(decode(&encode_raw(&data)), data);
    }

    #[test]
    fn decoding_tolerates_what_go_tolerates() {
        assert_eq!(decode("Zm9v YmFy\n"), b"foobar");
        // Padding is skipped, so both flavours decode alike.
        assert_eq!(decode("Zm9vYmFy=="), b"foobar");
        // The standard alphabet is accepted alongside the URL-safe one, so
        // "+/8=" and "-_8=" are the same three bytes.
        assert_eq!(decode("+/8="), vec![0xfb, 0xff]);
        assert_eq!(decode("-_8="), vec![0xfb, 0xff]);
        // Junk is dropped rather than rejected, which is what a caller that
        // discards Go's decode error sees.
        assert_eq!(decode("not base64!!"), decode("notbase64"));
        assert!(decode("").is_empty());
        assert!(decode("!!").is_empty());
    }
}
