// ref: internal/auth/claude/pkce.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// PKCE verification pair used by Anthropic OAuth.
///
/// The DTO originates in upstream `internal/auth/claude/anthropic.go`; it is
/// colocated here until that scaffold joins the Rust module graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceError {
    Randomness,
}

impl fmt::Display for PkceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Randomness => formatter
                .write_str("failed to generate code verifier: failed to generate random bytes"),
        }
    }
}

impl std::error::Error for PkceError {}

/// Generates the upstream-compatible 96-byte/128-character PKCE verifier and
/// its RFC 7636 S256 challenge.
pub fn generate_pkce_codes() -> Result<PkceCodes, PkceError> {
    let code_verifier = generate_code_verifier()?;
    let code_challenge = generate_code_challenge(&code_verifier);
    Ok(PkceCodes {
        code_verifier,
        code_challenge,
    })
}

fn generate_code_verifier() -> Result<String, PkceError> {
    let mut random = [0_u8; 96];
    getrandom::fill(&mut random).map_err(|_| PkceError::Randomness)?;
    Ok(URL_SAFE_NO_PAD.encode(random))
}

fn generate_code_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_has_upstream_entropy_length_and_url_safe_alphabet() {
        let codes = generate_pkce_codes().unwrap();

        assert_eq!(codes.code_verifier.len(), 128);
        assert!(codes
            .code_verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        assert!(!codes.code_verifier.contains('='));
        assert_eq!(codes.code_challenge.len(), 43);
        assert!(!codes.code_challenge.contains('='));
    }

    #[test]
    fn challenge_matches_rfc_7636_s256_vector() {
        assert_eq!(
            generate_code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_challenge_is_derived_from_generated_verifier() {
        let codes = generate_pkce_codes().unwrap();
        assert_eq!(
            codes.code_challenge,
            generate_code_challenge(&codes.code_verifier)
        );
    }

}
