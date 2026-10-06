#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use rand::RngCore;

/// C++ aria2 GID: 16 lowercase hex characters (64-bit).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gid(pub String);

impl Gid {
    pub fn generate() -> Self {
        let mut b = [0u8; 8];
        rand::rng().fill_bytes(&mut b);
        Self(hex::encode(b))
    }

    /// C++ `--gid=GID`: 16 hex chars `[0-9a-fA-F]`. Stored lowercase.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.len() != 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Other("GID must be 16 hex characters".into()));
        }
        Ok(Gid(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Gid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gid_is_16_hex() {
        let g = Gid::generate();
        assert_eq!(g.0.len(), 16);
        assert!(g.0.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parse_accepts_upper_and_rejects_bad() {
        assert_eq!(Gid::parse("0123456789ABCDEF").unwrap().as_str(), "0123456789abcdef");
        assert!(Gid::parse("short").is_err());
        assert!(Gid::parse("0123456789abcdeg").is_err());
        assert!(Gid::parse("0123456789abcdef0").is_err());
    }
}
