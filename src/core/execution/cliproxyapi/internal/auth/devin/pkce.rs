// ref: internal/auth/devin/pkce.go:10-31
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — host cryptographic entropy; redacted diagnostics
// License: MIT (upstream); modifications AGPL-3.0-only

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Clone, Eq, PartialEq)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}
impl fmt::Debug for PkceCodes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PkceCodes")
            .field("verifier_bytes", &self.code_verifier.len())
            .field("challenge_bytes", &self.code_challenge.len())
            .finish()
    }
}

pub fn generate_pkce_codes() -> Result<PkceCodes, PkceEntropyError> {
    let mut bytes = [0_u8; 64];
    getrandom::fill(&mut bytes).map_err(|_| PkceEntropyError)?;
    Ok(pkce_from_bytes(bytes))
}
fn pkce_from_bytes(bytes: [u8; 64]) -> PkceCodes {
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PkceEntropyError;
impl fmt::Display for PkceEntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to generate random bytes")
    }
}
impl std::error::Error for PkceEntropyError {}

#[cfg(test)]
#[path = "pkce_test.rs"]
mod tests;
