//! Session-handoff permit crypto and bounded-transfer gate behavior.
//! The scripted gate here is a test double for transport/gate mechanics only;
//! production authorization comes from the native Business OS policy adapter.
use ctox_sync::authority::auth::session_handoff::{
    validate_session_handoff_permit, verify_fresh_session_handoff_permit, verify_permit_evidence,
    verify_session_handoff_permit,
};
use ctox_sync::authority::auth::SigningIdentity;
use ctox_sync::authority::handoff::{
    SessionHandoffDenial, SessionHandoffGate, SessionHandoffGateRequest, SessionHandoffTransfer,
};
use ctox_sync::authority::{ExecutionSpec, Ownership};
use ctox_sync::contracts::{
    SessionHandoffPermit, SessionHandoffPhase, CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn identity() -> Arc<SigningIdentity> {
    Arc::new(SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap())
}

fn spec() -> ExecutionSpec {
    ExecutionSpec {
        job_id: "job".into(),
        session_id: "session".into(),
        scope_id: "scope".into(),
        harness: "codex".into(),
        harness_version: "fixture".into(),
        model_route_id: "route".into(),
        gateway_account_id: "account".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::new(),
    }
}

fn unsigned_permit(phase: SessionHandoffPhase, nonce: &str) -> SessionHandoffPermit {
    let now = now_ms();
    SessionHandoffPermit {
        version: CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
        binding_digest: "b".repeat(64),
        phase,
        audience: "scope".into(),
        nonce: nonce.into(),
        job_id: "job".into(),
        session_id: "session".into(),
        scope_id: "scope".into(),
        checkpoint_digest: "a".repeat(64),
        checkpoint_sequence: 1,
        ownership_generation: 1,
        principal_epoch: 7,
        binding_revision: 3,
        issued_at_ms: now,
        expires_at_ms: now + 60_000,
        signature: String::new(),
    }
}

#[test]
fn permits_bind_signer_audience_nonce_phase_and_checkpoint() {
    let issuer = identity();
    let other = identity();
    let permit = issuer
        .sign_session_handoff_permit(&unsigned_permit(SessionHandoffPhase::Disclose, "request-1"))
        .unwrap();
    validate_session_handoff_permit(&permit).unwrap();
    verify_session_handoff_permit(&permit, &issuer.public_identity(), "scope", "request-1")
        .unwrap();
    // Wrong signer, audience, nonce, tampering and re-signing all fail.
    assert!(
        verify_session_handoff_permit(&permit, &other.public_identity(), "scope", "request-1")
            .is_err()
    );
    assert!(verify_session_handoff_permit(
        &permit,
        &issuer.public_identity(),
        "other",
        "request-1"
    )
    .is_err());
    assert!(verify_session_handoff_permit(
        &permit,
        &issuer.public_identity(),
        "scope",
        "request-2"
    )
    .is_err());
    let mut tampered = permit.clone();
    tampered.binding_digest = "c".repeat(64);
    assert!(verify_session_handoff_permit(
        &tampered,
        &issuer.public_identity(),
        "scope",
        "request-1"
    )
    .is_err());
    assert!(issuer.sign_session_handoff_permit(&permit).is_err());
}

#[test]
fn permit_validity_window_is_enforced_at_transport_time() {
    let issuer = identity();
    let permit = issuer
        .sign_session_handoff_permit(&unsigned_permit(SessionHandoffPhase::Receive, "n-1"))
        .unwrap();
    verify_fresh_session_handoff_permit(
        &permit,
        &issuer.public_identity(),
        "scope",
        "n-1",
        now_ms(),
    )
    .unwrap();
    // Expired evidence never authorizes, even with a valid signature.
    let now = now_ms();
    let mut expired = unsigned_permit(SessionHandoffPhase::Receive, "n-2");
    expired.issued_at_ms = now.saturating_sub(120_000);
    expired.expires_at_ms = now.saturating_sub(60_000);
    let expired = issuer.sign_session_handoff_permit(&expired).unwrap();
    assert!(verify_fresh_session_handoff_permit(
        &expired,
        &issuer.public_identity(),
        "scope",
        "n-2",
        now
    )
    .is_err());
    // Not-yet-valid evidence is likewise rejected.
    let mut future = unsigned_permit(SessionHandoffPhase::Receive, "n-3");
    future.issued_at_ms = now + 120_000;
    future.expires_at_ms = now + 180_000;
    let future = issuer.sign_session_handoff_permit(&future).unwrap();
    assert!(verify_fresh_session_handoff_permit(
        &future,
        &issuer.public_identity(),
        "scope",
        "n-3",
        now
    )
    .is_err());
}

#[test]
fn quorum_evidence_requires_exact_command_fields() {
    let issuer = identity();
    let permit = issuer
        .sign_session_handoff_permit(&unsigned_permit(SessionHandoffPhase::Disclose, "cmd-1"))
        .unwrap();
    verify_permit_evidence(
        &permit,
        &issuer.public_identity(),
        SessionHandoffPhase::Disclose,
        &spec(),
        &"a".repeat(64),
        1,
        1,
        "cmd-1",
    )
    .unwrap();
    for (phase, digest, sequence, generation, request_id) in [
        (
            SessionHandoffPhase::Resume,
            "a".repeat(64),
            1,
            1,
            "cmd-1".to_string(),
        ),
        (
            SessionHandoffPhase::Disclose,
            "d".repeat(64),
            1,
            1,
            "cmd-1".to_string(),
        ),
        (
            SessionHandoffPhase::Disclose,
            "a".repeat(64),
            2,
            1,
            "cmd-1".to_string(),
        ),
        (
            SessionHandoffPhase::Disclose,
            "a".repeat(64),
            1,
            2,
            "cmd-1".to_string(),
        ),
        (
            SessionHandoffPhase::Disclose,
            "a".repeat(64),
            1,
            1,
            "cmd-2".to_string(),
        ),
    ] {
        assert!(
            verify_permit_evidence(
                &permit,
                &issuer.public_identity(),
                phase,
                &spec(),
                &digest,
                sequence,
                generation,
                &request_id,
            )
            .is_err(),
            "evidence accepted mismatched phase/digest/sequence/generation/request"
        );
    }
    let mut other_spec = spec();
    other_spec.session_id = "other-session".into();
    assert!(verify_permit_evidence(
        &permit,
        &issuer.public_identity(),
        SessionHandoffPhase::Disclose,
        &other_spec,
        &"a".repeat(64),
        1,
        1,
        "cmd-1",
    )
    .is_err());
}

/// Scripted test gate: mints real signatures, denies after revocation.
/// This is a test double for gate mechanics, not production authorization.
struct ScriptedGate {
    key: Arc<SigningIdentity>,
    revoked: AtomicBool,
    calls: AtomicU64,
}

impl SessionHandoffGate for ScriptedGate {
    fn authorize(
        &self,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.revoked.load(Ordering::SeqCst) {
            return Err(SessionHandoffDenial::new("binding_revoked"));
        }
        let now = now_ms();
        self.key
            .sign_session_handoff_permit(&SessionHandoffPermit {
                version: CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
                binding_digest: request.binding_digest.clone(),
                phase: request.phase.clone(),
                audience: request.audience.clone(),
                nonce: request.nonce.clone(),
                job_id: request.spec.job_id.clone(),
                session_id: request.spec.session_id.clone(),
                scope_id: request.spec.scope_id.clone(),
                checkpoint_digest: request.checkpoint_digest.clone(),
                checkpoint_sequence: request.checkpoint_sequence,
                ownership_generation: request.ownership.generation,
                principal_epoch: 0,
                binding_revision: 1,
                issued_at_ms: now,
                expires_at_ms: now + 60_000,
                signature: String::new(),
            })
            .map_err(|_| SessionHandoffDenial::new("permit_signing_failed"))
    }
}

struct PausedGate {
    inner: ScriptedGate,
    pause: AtomicBool,
    entered: tokio::sync::Notify,
    finished: tokio::sync::Notify,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl SessionHandoffGate for PausedGate {
    fn authorize(
        &self,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        if self.pause.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| SessionHandoffDenial::new("fixture_release_timeout"))?;
            let result = self.inner.authorize(request);
            self.finished.notify_one();
            return result;
        }
        self.inner.authorize(request)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_revalidation_cannot_restore_old_or_late_evidence() {
    let issuer = identity();
    let (release, receiver) = std::sync::mpsc::channel();
    let gate = Arc::new(PausedGate {
        inner: ScriptedGate {
            key: issuer.clone(),
            revoked: AtomicBool::new(false),
            calls: AtomicU64::new(0),
        },
        pause: AtomicBool::new(false),
        entered: tokio::sync::Notify::new(),
        finished: tokio::sync::Notify::new(),
        release: std::sync::Mutex::new(receiver),
    });
    let mut transfer = SessionHandoffTransfer::begin(
        gate.clone(),
        gate_request(SessionHandoffPhase::Disclose, &issuer),
    )
    .await
    .unwrap();
    let initial_nonce = transfer.permit().unwrap().nonce.clone();
    gate.pause.store(true, Ordering::SeqCst);
    let mut pending = Box::pin(transfer.revalidate());
    tokio::select! {
        result = &mut pending => panic!("policy returned before release: {result:?}"),
        result = tokio::time::timeout(std::time::Duration::from_secs(5), gate.entered.notified()) => result.expect("policy entered"),
    }
    drop(pending);
    assert!(transfer.permit().is_none());
    assert_eq!(transfer.decisions(), 1);
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), gate.finished.notified())
        .await
        .expect("detached policy evaluation completed");
    assert!(
        transfer.permit().is_none(),
        "late success cannot revive a cancelled decision"
    );
    assert_eq!(transfer.decisions(), 1);
    let fresh = transfer.revalidate().await.unwrap();
    assert_ne!(fresh.nonce, initial_nonce);
    assert_eq!(transfer.decisions(), 2);
}

struct MismatchedGate {
    inner: ScriptedGate,
    fault: &'static str,
}

impl SessionHandoffGate for MismatchedGate {
    fn authorize(
        &self,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        let mut permit = self.inner.authorize(request)?;
        match self.fault {
            "binding" => permit.binding_digest = "c".repeat(64),
            "phase" => permit.phase = SessionHandoffPhase::Resume,
            "job" => permit.job_id = "other-job".into(),
            "session" => permit.session_id = "other-session".into(),
            "scope" => permit.scope_id = "other-scope".into(),
            "checkpoint" => permit.checkpoint_digest = "d".repeat(64),
            "sequence" => permit.checkpoint_sequence += 1,
            "generation" => permit.ownership_generation += 1,
            "audience" => permit.audience = "other-audience".into(),
            "nonce" => permit.nonce = "replayed-nonce".into(),
            "expired" => {
                permit.issued_at_ms = 1;
                permit.expires_at_ms = 2;
            }
            "signature" => {
                permit.signature = "00".repeat(64);
                return Ok(permit);
            }
            "signer" => return Ok(permit),
            _ => unreachable!(),
        }
        // Re-sign mismatches: these are authentic but authorize another request.
        permit.signature.clear();
        self.inner
            .key
            .sign_session_handoff_permit(&permit)
            .map_err(|_| SessionHandoffDenial::new("fixture_signing_failed"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfer_rejects_authentic_mismatched_and_untrusted_permits() {
    for fault in [
        "binding",
        "phase",
        "job",
        "session",
        "scope",
        "checkpoint",
        "sequence",
        "generation",
        "audience",
        "nonce",
        "expired",
        "signature",
        "signer",
    ] {
        let issuer = identity();
        let gate = Arc::new(MismatchedGate {
            inner: ScriptedGate {
                key: if fault == "signer" {
                    identity()
                } else {
                    issuer.clone()
                },
                revoked: AtomicBool::new(false),
                calls: AtomicU64::new(0),
            },
            fault,
        });
        assert!(
            SessionHandoffTransfer::begin(
                gate,
                gate_request(SessionHandoffPhase::Disclose, &issuer)
            )
            .await
            .is_err(),
            "accepted {fault}"
        );
    }
}

fn gate_request(phase: SessionHandoffPhase, issuer: &SigningIdentity) -> SessionHandoffGateRequest {
    SessionHandoffGateRequest {
        issuer_identity: issuer.public_identity(),
        phase,
        binding_digest: "b".repeat(64),
        audience: "scope".into(),
        nonce: String::new(),
        spec: spec(),
        checkpoint_digest: "a".repeat(64),
        checkpoint_sequence: 1,
        ownership: Ownership {
            node_id: 1,
            generation: 1,
        },
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfer_revalidates_with_fresh_nonces_and_stops_on_revocation() {
    let key = identity();
    let gate = Arc::new(ScriptedGate {
        key: key.clone(),
        revoked: AtomicBool::new(false),
        calls: AtomicU64::new(0),
    });
    let mut transfer = SessionHandoffTransfer::begin(
        gate.clone(),
        gate_request(SessionHandoffPhase::Disclose, &key),
    )
    .await
    .expect("first protected byte requires an affirmative decision");
    let first = transfer
        .permit()
        .expect("begin stored verified evidence")
        .clone();
    verify_fresh_session_handoff_permit(
        &first,
        &key.public_identity(),
        "scope",
        &first.nonce.clone(),
        now_ms(),
    )
    .unwrap();

    let second = transfer
        .revalidate()
        .await
        .expect("chunk boundary authorized");
    assert_ne!(
        first.nonce, second.nonce,
        "chunk boundaries use fresh nonces"
    );
    assert_ne!(first.signature, second.signature);
    assert_eq!(transfer.decisions(), 2);

    // Mid-transfer revocation: no further chunk may be authorized.
    gate.revoked.store(true, Ordering::SeqCst);
    let denial = transfer
        .revalidate()
        .await
        .expect_err("revocation must stop subsequent chunks");
    assert_eq!(denial.reason_code, "binding_revoked");
    assert!(
        transfer.permit().is_none(),
        "revocation invalidates old evidence"
    );
    assert_eq!(
        transfer.decisions(),
        2,
        "a denied revalidation grants nothing"
    );
    assert!(
        transfer.revalidate().await.is_err(),
        "revocation is not self-healing"
    );
    assert!(transfer.permit().is_none());
    gate.revoked.store(false, Ordering::SeqCst);
    let restored = transfer
        .revalidate()
        .await
        .expect("fresh policy decision required");
    assert_ne!(restored.nonce, second.nonce);
    assert_eq!(transfer.decisions(), 3);
    assert_eq!(transfer.permit().unwrap().nonce, restored.nonce);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_policy_denies_the_first_protected_byte() {
    let gate = Arc::new(ScriptedGate {
        key: identity(),
        revoked: AtomicBool::new(true),
        calls: AtomicU64::new(0),
    });
    let denial = SessionHandoffTransfer::begin(
        gate.clone(),
        gate_request(SessionHandoffPhase::Resume, &gate.key),
    )
    .await
    .err()
    .expect("resume without current policy authorization must deny");
    assert_eq!(denial.reason_code, "binding_revoked");
    assert_eq!(gate.calls.load(Ordering::SeqCst), 1);
}
