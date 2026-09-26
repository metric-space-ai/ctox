//! Policy gate seam for session handoff. Generic Sync stays policy-agnostic:
//! it defines the gate interface and enforces that no checkpoint stream or
//! takeover proceeds without the responsible instance's current, signed
//! decision. Production policy and checkpoint-stream wiring remain required.
use super::auth::session_handoff::verify_fresh_session_handoff_permit;
use crate::contracts::{
    ExecutionOwnership, ExecutionSpec, SessionHandoffPermit, SessionHandoffPhase,
};
use ring::rand::{SecureRandom, SystemRandom};
use std::fmt;
use std::sync::Arc;

/// One phase authorization request evaluated against current native policy.
/// `nonce` is caller-fresh per evaluation; permits are never reused as proof
/// of current authorization.
#[derive(Debug, Clone)]
pub struct SessionHandoffGateRequest {
    /// Resolve from trusted enrollment, never from the returned permit or peer payload.
    pub issuer_identity: String,
    pub phase: SessionHandoffPhase,
    pub binding_digest: String,
    pub audience: String,
    pub nonce: String,
    pub spec: ExecutionSpec,
    pub checkpoint_digest: String,
    pub checkpoint_sequence: u64,
    pub ownership: ExecutionOwnership,
}

/// A stable, typed policy denial. Reason codes are identifiers for audit and
/// operators; they never carry journal payloads, secrets or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHandoffDenial {
    pub reason_code: String,
}

impl SessionHandoffDenial {
    pub fn new(reason_code: impl Into<String>) -> Self {
        Self {
            reason_code: reason_code.into(),
        }
    }
}

impl fmt::Display for SessionHandoffDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "session handoff denied: {}", self.reason_code)
    }
}

impl std::error::Error for SessionHandoffDenial {}

/// Current native session-handoff policy. Every implementation must re-read
/// its authorities (actor epoch, grants, binding revision, workspace/account
/// mappings, peer/member revocation) on each call and deny on any missing or
/// stale dependency; there is no cached affirmative. Implementations perform
/// blocking store I/O and are invoked on a blocking executor by the guard.
pub trait SessionHandoffGate: Send + Sync + 'static {
    fn authorize(
        &self,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial>;
}

fn fresh_nonce() -> Result<String, SessionHandoffDenial> {
    let mut nonce = [0u8; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| SessionHandoffDenial::new("nonce_generation_failed"))?;
    Ok(nonce.iter().map(|b| format!("{b:02x}")).collect())
}

async fn call_gate(
    gate: Arc<dyn SessionHandoffGate>,
    request: SessionHandoffGateRequest,
) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
    let expected = request.clone();
    let permit = tokio::task::spawn_blocking(move || gate.authorize(&request))
        .await
        .map_err(|_| SessionHandoffDenial::new("policy_executor_failed"))??;
    if permit.binding_digest != expected.binding_digest
        || permit.phase != expected.phase
        || permit.job_id != expected.spec.job_id
        || permit.session_id != expected.spec.session_id
        || permit.scope_id != expected.spec.scope_id
        || permit.checkpoint_digest != expected.checkpoint_digest
        || permit.checkpoint_sequence != expected.checkpoint_sequence
        || permit.ownership_generation != expected.ownership.generation
    {
        return Err(SessionHandoffDenial::new("permit_request_mismatch"));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or_else(|| SessionHandoffDenial::new("clock_unavailable"))?;
    verify_fresh_session_handoff_permit(
        &permit,
        &expected.issuer_identity,
        &expected.audience,
        &expected.nonce,
        now_ms,
    )
    .map_err(|_| SessionHandoffDenial::new("permit_verification_failed"))?;
    Ok(permit)
}

/// Bounded protected-transfer enforcement. `begin` gates the first protected
/// manifest/blob byte; every chunk boundary must call `revalidate`, which
/// re-authorizes against current policy with a fresh nonce. A denial stops the
/// transfer: no further chunks may be sent or ingested and resume is forbidden
/// until a fresh `begin`/`revalidate` succeeds. Bytes disclosed under an
/// earlier valid permit are not claimed recallable.
pub struct SessionHandoffTransfer {
    gate: Arc<dyn SessionHandoffGate>,
    request: SessionHandoffGateRequest,
    permit: Option<SessionHandoffPermit>,
    decisions: u64,
}

impl SessionHandoffTransfer {
    pub async fn begin(
        gate: Arc<dyn SessionHandoffGate>,
        mut request: SessionHandoffGateRequest,
    ) -> Result<Self, SessionHandoffDenial> {
        request.nonce = fresh_nonce()?;
        let permit = call_gate(gate.clone(), request.clone()).await?;
        Ok(Self {
            gate,
            request,
            permit: Some(permit),
            decisions: 1,
        })
    }

    /// Re-authorize at a bounded chunk/phase boundary with a fresh nonce.
    /// Returns the current permit evidence for audit; on denial the transfer
    /// must stop.
    pub async fn revalidate(&mut self) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        // Clear before fallible work or awaits, including future cancellation.
        self.permit = None;
        self.request.nonce = fresh_nonce()?;
        let permit = call_gate(self.gate.clone(), self.request.clone()).await?;
        self.decisions += 1;
        self.permit = Some(permit.clone());
        Ok(permit)
    }

    /// Audit evidence only; revalidate at every protected chunk boundary.
    /// Failed or cancelled revalidation clears the previous decision.
    pub fn permit(&self) -> Option<&SessionHandoffPermit> {
        self.permit.as_ref()
    }

    pub fn decisions(&self) -> u64 {
        self.decisions
    }
}
