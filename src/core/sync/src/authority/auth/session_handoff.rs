//! Phase-scoped session-handoff permits signed by enrolled instance keys.
//!
//! A permit is the only authorization evidence a checkpoint stream or a
//! takeover/resume may carry: it is minted by the responsible instance's native
//! policy adapter (Business OS session-handoff policy), bound to one exact
//! binding digest, phase, audience, nonce, checkpoint and ownership generation,
//! and verified against the enrolled Ed25519 peer identity. A sender-supplied
//! boolean, a permit from an unknown key, or a permit replayed across commands
//! is never authorization.
use super::{hex, invalid, public_key, unhex, SigningIdentity};
use crate::contracts::{
    ExecutionOwnership, ExecutionSpec, SessionHandoffPermit, SessionHandoffPhase,
    CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
};
use ring::signature::{UnparsedPublicKey, ED25519};
use std::io;

const DOMAIN: &[u8] = b"ctox.sync.session-handoff.permit.v1\0";
const MAX_ID_BYTES: usize = 256;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_counter(value: u64) -> bool {
    value <= MAX_SAFE_INTEGER
}

/// Field validation shared by signing, transport verification and the
/// deterministic quorum-evidence check. No wall clock and no store reads.
/// The nonce binds the permit to one exact request (a command request id or a
/// caller-fresh transport nonce); its format is the caller's choice, its
/// uniqueness is enforced by the channel that consumes it.
pub fn validate_session_handoff_permit(permit: &SessionHandoffPermit) -> io::Result<()> {
    if permit.version != CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION
        || !valid_digest(&permit.binding_digest)
        || !valid_digest(&permit.checkpoint_digest)
        || !valid_id(&permit.audience)
        || !valid_id(&permit.nonce)
        || !valid_id(&permit.job_id)
        || !valid_id(&permit.session_id)
        || !valid_id(&permit.scope_id)
        || !valid_counter(permit.checkpoint_sequence)
        || !valid_counter(permit.ownership_generation)
        || !valid_counter(permit.principal_epoch)
        || !valid_counter(permit.binding_revision)
        || !valid_counter(permit.issued_at_ms)
        || !valid_counter(permit.expires_at_ms)
        || permit.expires_at_ms <= permit.issued_at_ms
    {
        return Err(invalid("invalid session handoff permit"));
    }
    Ok(())
}

fn permit_signing_bytes(permit: &SessionHandoffPermit) -> io::Result<Vec<u8>> {
    validate_session_handoff_permit(permit)?;
    let mut value = serde_json::to_value(permit).map_err(io::Error::other)?;
    value
        .as_object_mut()
        .expect("permit is an object")
        .remove("signature");
    value.sort_all_objects();
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(serde_json::to_vec(&value).map_err(io::Error::other)?);
    Ok(bytes)
}

impl SigningIdentity {
    /// Mint a permit after the local native policy adapter authorized the phase.
    /// The caller supplies the decision inputs; this signs exactly those bytes.
    pub fn sign_session_handoff_permit(
        &self,
        permit: &SessionHandoffPermit,
    ) -> io::Result<SessionHandoffPermit> {
        if !permit.signature.is_empty() {
            return Err(invalid("session handoff permit is already signed"));
        }
        let bytes = permit_signing_bytes(permit)?;
        let mut signed = permit.clone();
        signed.signature = hex(self.key.sign(&bytes).as_ref());
        Ok(signed)
    }
}

/// Verify a permit's signature against an enrolled peer identity and bind it to
/// the expected audience and nonce. Does not check expiry or policy currency.
pub fn verify_session_handoff_permit(
    permit: &SessionHandoffPermit,
    signer_identity: &str,
    audience: &str,
    nonce: &str,
) -> io::Result<()> {
    validate_session_handoff_permit(permit)?;
    if permit.audience != audience || permit.nonce != nonce {
        return Err(invalid("session handoff permit audience or nonce mismatch"));
    }
    UnparsedPublicKey::new(&ED25519, public_key(signer_identity)?)
        .verify(
            &permit_signing_bytes(permit)?,
            &unhex::<64>(&permit.signature)?,
        )
        .map_err(|_| invalid("session handoff permit signature rejected"))
}

/// Transport-time verification additionally enforces the permit's own validity
/// window. The native policy adapter remains the only source of issuance.
pub fn verify_fresh_session_handoff_permit(
    permit: &SessionHandoffPermit,
    signer_identity: &str,
    audience: &str,
    nonce: &str,
    now_ms: u64,
) -> io::Result<()> {
    verify_session_handoff_permit(permit, signer_identity, audience, nonce)?;
    if permit.issued_at_ms > now_ms || permit.expires_at_ms <= now_ms {
        return Err(invalid("session handoff permit is not currently valid"));
    }
    Ok(())
}

/// Deterministic quorum-evidence check for a permit embedded in an authority
/// command. No wall clock: expiry is an admission concern, while the committed
/// log must replay identically on every voter. The permit must be signed by the
/// enrolled issuer and name this exact phase, job, checkpoint, ownership
/// generation, scope (audience) and command request id (nonce).
#[allow(clippy::too_many_arguments)]
pub fn verify_permit_evidence(
    permit: &SessionHandoffPermit,
    issuer_identity: &str,
    phase: SessionHandoffPhase,
    spec: &ExecutionSpec,
    checkpoint_digest: &str,
    checkpoint_sequence: u64,
    ownership_generation: u64,
    request_id: &str,
) -> io::Result<()> {
    if permit.phase != phase
        || permit.job_id != spec.job_id
        || permit.session_id != spec.session_id
        || permit.scope_id != spec.scope_id
        || permit.checkpoint_digest != checkpoint_digest
        || permit.checkpoint_sequence != checkpoint_sequence
        || permit.ownership_generation != ownership_generation
    {
        return Err(invalid("session handoff permit does not match the command"));
    }
    verify_session_handoff_permit(permit, issuer_identity, &spec.scope_id, request_id)
}

/// Ownership helper for callers that carry a full ownership record.
pub fn permit_matches_ownership(
    permit: &SessionHandoffPermit,
    ownership: &ExecutionOwnership,
) -> bool {
    permit.ownership_generation == ownership.generation
}
