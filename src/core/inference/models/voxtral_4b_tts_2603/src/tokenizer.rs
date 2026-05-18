//! Tekken tokenizer decode support, dependency-free.
//!
//! This seed implements decode-only behavior needed by greedy inference:
//! model IDs 0..999 are special/control tokens; IDs >= 1000 map to
//! `vocab[id - 1000].token_bytes` from `tekken.json`.

use crate::consts::TOKEN_TEXT_MIN;
use crate::{Error, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct Tekken {
    vocab: Vec<Vec<u8>>,
}

impl Tekken {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = fs::read(path)?;
        Self::from_json_bytes(&bytes)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        let s = std::str::from_utf8(bytes)?;
        let mut vocab = Vec::new();
        let needle = "\"token_bytes\"";
        let mut pos = 0usize;
        while let Some(rel) = s[pos..].find(needle) {
            pos += rel + needle.len();
            let rest = &s[pos..];
            let colon = rest
                .find(':')
                .ok_or(Error::InvalidFormat("token_bytes without colon"))?;
            let after = &rest[colon + 1..];
            let start = after
                .find('"')
                .ok_or(Error::InvalidFormat("token_bytes without string"))?;
            let mut end = start + 1;
            let b = after.as_bytes();
            let mut esc = false;
            while end < b.len() {
                if esc {
                    esc = false;
                    end += 1;
                    continue;
                }
                if b[end] == b'\\' {
                    esc = true;
                    end += 1;
                    continue;
                }
                if b[end] == b'"' {
                    break;
                }
                end += 1;
            }
            if end >= b.len() {
                return Err(Error::InvalidFormat("unterminated token_bytes string"));
            }
            let encoded = &after[start + 1..end];
            vocab.push(base64_decode(encoded)?);
            pos += colon + 1 + end + 1;
        }
        if vocab.is_empty() {
            return Err(Error::InvalidFormat("no token_bytes entries found"));
        }
        Ok(Self { vocab })
    }

    pub fn decode_token_bytes(&self, token_id: u32) -> Option<&[u8]> {
        if token_id < TOKEN_TEXT_MIN {
            return None;
        }
        self.vocab
            .get((token_id - TOKEN_TEXT_MIN) as usize)
            .map(|v| v.as_slice())
    }

    pub fn decode_token_lossy(&self, token_id: u32) -> Option<String> {
        self.decode_token_bytes(token_id)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    pub fn vocab_len(&self) -> usize {
        self.vocab.len()
    }
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut n = 0usize;
    for &b in s.as_bytes() {
        if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
            continue;
        }
        if b == b'=' {
            chunk[n] = 64;
        } else {
            chunk[n] = val(b).ok_or(Error::InvalidFormat("invalid base64 character"))?;
        }
        n += 1;
        if n == 4 {
            let a = chunk[0];
            let c = chunk[2];
            let d = chunk[3];
            let triple = ((a as u32) << 18)
                | ((chunk[1] as u32) << 12)
                | (if c == 64 { 0 } else { (c as u32) << 6 })
                | (if d == 64 { 0 } else { d as u32 });
            out.push(((triple >> 16) & 0xff) as u8);
            if c != 64 {
                out.push(((triple >> 8) & 0xff) as u8);
            }
            if d != 64 {
                out.push((triple & 0xff) as u8);
            }
            n = 0;
        }
    }
    if n != 0 {
        return Err(Error::InvalidFormat("base64 length not multiple of 4"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_smoke() {
        assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello");
    }

    #[test]
    fn parse_fake_tekken() {
        let json = br#"{"vocab":[{"token_bytes":"SGk="},{"token_bytes":"IQ=="}]}"#;
        let t = Tekken::from_json_bytes(json).unwrap();
        assert_eq!(t.decode_token_lossy(1000).unwrap(), "Hi");
        assert_eq!(t.decode_token_lossy(1001).unwrap(), "!");
        assert!(t.decode_token_lossy(2).is_none());
    }
}
