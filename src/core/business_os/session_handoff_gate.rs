// Origin: CTOX
// License: Apache-2.0

//! Production native `SessionHandoffGate` adapter (issue183).
//!
//! Generic Sync stays policy-agnostic; this module is the native Business OS
//! policy authority the gate seam names. Every `authorize` call re-reads its
//! durable authorities — the enrolled binding row, the exact permission grant,
//! the principal's current role/capability epoch and the provisioned instance
//! signing identity — and denies on any missing or stale dependency. There is
//! no cached affirmative: a revocation between two calls denies the next one,
//! which is what makes phase and per-chunk revalidation meaningful.
//!
//! What this adapter deliberately does not do: it does not fabricate a
//! binding (enrollment is a separate authorized act), it does not infer
//! account or workspace entitlement from a reachable model route or a sole
//! configured account, and it does not consume checkpoint bytes. The
//! operational transfer consumer that must call `SessionHandoffTransfer` is
//! still absent; see `docs/ctox-sync-handoff-integration.md`.

use super::policy::{BusinessOsActor, BusinessOsPermission, BusinessOsScope, BusinessOsScopeType};
use super::store::{now_ms, open_store};
use super::store_policy::active_permission_grant_allows;
use ctox_sync::authority::auth::SigningIdentity;
use ctox_sync::authority::handoff::{
    SessionHandoffDenial, SessionHandoffGate, SessionHandoffGateRequest,
};
use ctox_sync::contracts::{
    SessionHandoffPermit, SessionHandoffPhase, CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Permits are admission evidence for one bounded transfer phase, not a
/// standing grant. Sixty seconds bounds the window between policy decision
/// and quorum admission without making expiry the replay protection (the
/// caller-fresh nonce and request-id binding do that).
const PERMIT_TTL_MS: u64 = 60_000;

fn deny(reason_code: &'static str) -> SessionHandoffDenial {
    SessionHandoffDenial::new(reason_code)
}

/// Production gate. Construct through [`native_session_handoff_gate`]; the
/// enrolled identity comes from the CTOX secret store via the sync host, so
/// permits are signed by the same key the authority cluster enrolled.
pub struct NativeSessionHandoffGate {
    root: PathBuf,
    identity: Arc<SigningIdentity>,
    permit_ttl_ms: u64,
}

impl NativeSessionHandoffGate {
    fn with_identity(root: PathBuf, identity: Arc<SigningIdentity>) -> Self {
        Self {
            root,
            identity,
            permit_ttl_ms: PERMIT_TTL_MS,
        }
    }

    fn authorize_with_conn(
        &self,
        conn: &Connection,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        if request.nonce.trim().is_empty() || request.audience.trim().is_empty() {
            return Err(deny("invalid_request"));
        }
        let binding = load_binding(conn, &request.binding_digest)
            .map_err(|_| deny("store_unavailable"))?
            .ok_or_else(|| deny("binding_unknown"))?;
        if binding.state != "active" {
            return Err(deny("binding_revoked"));
        }
        let (expected_side, permission, principal_id, expected_identity) = match request.phase {
            SessionHandoffPhase::Disclose => (
                "source",
                BusinessOsPermission::SessionHandoffDisclose,
                binding.source_actor_user_id.as_str(),
                binding.source_identity.as_str(),
            ),
            SessionHandoffPhase::Receive => (
                "target",
                BusinessOsPermission::SessionHandoffReceive,
                binding.target_principal_user_id.as_str(),
                binding.target_identity.as_str(),
            ),
            SessionHandoffPhase::Resume => (
                "target",
                BusinessOsPermission::SessionHandoffExecute,
                binding.target_principal_user_id.as_str(),
                binding.target_identity.as_str(),
            ),
        };
        if binding.side != expected_side {
            return Err(deny("binding_mismatch"));
        }
        let checkpoint_sequence =
            u64::try_from(binding.checkpoint_sequence).map_err(|_| deny("binding_mismatch"))?;
        let ownership_generation =
            u64::try_from(binding.ownership_generation).map_err(|_| deny("binding_mismatch"))?;
        let binding_revision =
            u64::try_from(binding.revision).map_err(|_| deny("binding_mismatch"))?;
        // The request must name exactly the enrolled binding: job, session,
        // scope (audience), checkpoint, ownership generation and the harness,
        // model route, gateway account and model the binding enrolled. A
        // caller-supplied variation of any of these is not this binding.
        if request.audience != binding.scope_id
            || request.spec.scope_id != binding.scope_id
            || request.spec.job_id != binding.job_id
            || request.spec.session_id != binding.session_id
            || request.spec.harness != binding.harness
            || request.spec.harness_version != binding.harness_version
            || request.spec.model_route_id != binding.model_route_id
            || request.spec.gateway_account_id != binding.gateway_account_id
            || request.spec.model_id != binding.model_id
            || request.checkpoint_digest != binding.checkpoint_digest
            || request.checkpoint_sequence != checkpoint_sequence
            || request.ownership.generation != ownership_generation
        {
            return Err(deny("binding_mismatch"));
        }
        // Only the responsible enrolled instance may mint for its side.
        if self.identity.public_identity() != expected_identity {
            return Err(deny("wrong_instance"));
        }
        // The principal must be a current active native user; its capability
        // epoch becomes the permit's principal epoch so a later epoch bump
        // (password/role/capability change) invalidates outstanding evidence.
        let (role, epoch): (String, i64) = conn
            .query_row(
                "SELECT role, capability_epoch
                 FROM business_users
                 WHERE user_id = ?1 AND active = 1",
                params![principal_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| deny("store_unavailable"))?
            .ok_or_else(|| deny("principal_unavailable"))?;
        let principal_epoch = u64::try_from(epoch).map_err(|_| deny("principal_unavailable"))?;
        // Exact-grant-only: no role, workspace-manage or data permission
        // implies a session-handoff decision (see store_policy).
        let actor = BusinessOsActor::new(Some(principal_id.to_owned()), role.as_str());
        let scope = BusinessOsScope {
            scope_type: BusinessOsScopeType::SessionHandoff,
            scope_id: Some(binding.binding_id.clone()),
            assigned_to_actor: false,
            owned_by_actor: false,
        };
        if !active_permission_grant_allows(conn, &actor, permission, &scope)
            .map_err(|_| deny("store_unavailable"))?
        {
            return Err(deny("grant_missing"));
        }
        let now = now_ms() as u64;
        let permit = SessionHandoffPermit {
            version: CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
            binding_digest: request.binding_digest.clone(),
            phase: request.phase.clone(),
            audience: binding.scope_id.clone(),
            nonce: request.nonce.clone(),
            job_id: binding.job_id.clone(),
            session_id: binding.session_id.clone(),
            scope_id: binding.scope_id.clone(),
            checkpoint_digest: binding.checkpoint_digest.clone(),
            checkpoint_sequence,
            ownership_generation,
            principal_epoch,
            binding_revision,
            issued_at_ms: now,
            expires_at_ms: now + self.permit_ttl_ms,
            signature: String::new(),
        };
        self.identity
            .sign_session_handoff_permit(&permit)
            .map_err(|_| deny("permit_signing_failed"))
    }
}

impl SessionHandoffGate for NativeSessionHandoffGate {
    fn authorize(
        &self,
        request: &SessionHandoffGateRequest,
    ) -> Result<SessionHandoffPermit, SessionHandoffDenial> {
        let conn = open_store(&self.root).map_err(|_| deny("store_unavailable"))?;
        self.authorize_with_conn(&conn, request)
    }
}

/// Construct the production gate for this instance. Fails visibly when the
/// provisioned native Sync identity is unavailable; a handoff decision can
/// never fall back to an anonymous or generated key.
pub fn native_session_handoff_gate(root: &Path) -> anyhow::Result<Arc<dyn SessionHandoffGate>> {
    let identity = crate::sync_host::signing_identity(root)?;
    Ok(Arc::new(NativeSessionHandoffGate::with_identity(
        root.to_path_buf(),
        identity,
    )))
}

/// The enrolled binding fields the gate resolves against. Enrollment (who may
/// write this table) is a separate authorized path; the gate only reads.
struct HandoffBindingRow {
    binding_id: String,
    revision: i64,
    state: String,
    side: String,
    job_id: String,
    session_id: String,
    scope_id: String,
    checkpoint_digest: String,
    checkpoint_sequence: i64,
    ownership_generation: i64,
    source_identity: String,
    source_actor_user_id: String,
    target_identity: String,
    target_principal_user_id: String,
    harness: String,
    harness_version: String,
    model_route_id: String,
    gateway_account_id: String,
    model_id: String,
}

fn load_binding(
    conn: &Connection,
    binding_digest: &str,
) -> rusqlite::Result<Option<HandoffBindingRow>> {
    conn.query_row(
        "SELECT binding_id, revision, state, side, job_id, session_id, scope_id,
                checkpoint_digest, checkpoint_sequence, ownership_generation,
                source_identity, source_actor_user_id,
                target_identity, target_principal_user_id,
                harness, harness_version, model_route_id, gateway_account_id, model_id
         FROM business_session_handoff_bindings
         WHERE binding_digest = ?1",
        params![binding_digest],
        |row| {
            Ok(HandoffBindingRow {
                binding_id: row.get(0)?,
                revision: row.get(1)?,
                state: row.get(2)?,
                side: row.get(3)?,
                job_id: row.get(4)?,
                session_id: row.get(5)?,
                scope_id: row.get(6)?,
                checkpoint_digest: row.get(7)?,
                checkpoint_sequence: row.get(8)?,
                ownership_generation: row.get(9)?,
                source_identity: row.get(10)?,
                source_actor_user_id: row.get(11)?,
                target_identity: row.get(12)?,
                target_principal_user_id: row.get(13)?,
                harness: row.get(14)?,
                harness_version: row.get(15)?,
                model_route_id: row.get(16)?,
                gateway_account_id: row.get(17)?,
                model_id: row.get(18)?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_sync::authority::auth::session_handoff::verify_session_handoff_permit;
    use ctox_sync::contracts::{ExecutionOwnership, ExecutionSpec};

    fn test_conn() -> rusqlite::Result<Connection> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE business_session_handoff_bindings (
                binding_id TEXT PRIMARY KEY,
                revision INTEGER NOT NULL,
                state TEXT NOT NULL,
                side TEXT NOT NULL,
                job_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                checkpoint_digest TEXT NOT NULL,
                checkpoint_sequence INTEGER NOT NULL,
                ownership_generation INTEGER NOT NULL,
                source_identity TEXT NOT NULL,
                source_actor_user_id TEXT NOT NULL,
                target_identity TEXT NOT NULL,
                target_principal_user_id TEXT NOT NULL,
                harness TEXT NOT NULL,
                harness_version TEXT NOT NULL,
                model_route_id TEXT NOT NULL,
                gateway_account_id TEXT NOT NULL,
                model_id TEXT NOT NULL
            );
            CREATE TABLE business_permission_grants (
                active INTEGER, permission TEXT, scope_type TEXT, scope_id TEXT,
                subject_type TEXT, subject_id TEXT
            );
            CREATE TABLE business_users (
                user_id TEXT PRIMARY KEY,
                role TEXT NOT NULL,
                capability_epoch INTEGER NOT NULL,
                active INTEGER NOT NULL
            );",
        )?;
        Ok(conn)
    }

    fn identity() -> Arc<SigningIdentity> {
        Arc::new(SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap())
    }

    fn digest(text: &str) -> String {
        text.chars().cycle().take(64).collect::<String>()
    }

    struct Fixture {
        conn: Connection,
        gate: NativeSessionHandoffGate,
        identity: Arc<SigningIdentity>,
        binding_digest: String,
    }

    fn fixture() -> Fixture {
        let conn = test_conn().unwrap();
        let identity = identity();
        let binding_digest = digest("ab");
        conn.execute(
            "INSERT INTO business_session_handoff_bindings VALUES (
                'binding-1', 3, 'active', 'source', 'job-1', 'session-1', 'scope-1',
                ?1, 7, 2, ?2, 'alice', 'ed25519:target', 'bob',
                'harness-a', '1.0', 'route-1', 'account-1', 'model-1'
            )",
            params![binding_digest.as_str(), identity.public_identity().as_str()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO business_users VALUES ('alice', 'admin', 11, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO business_permission_grants VALUES (
                1, 'ctox.session_handoff.disclose', 'session_handoff', 'binding-1',
                'user', 'alice'
            )",
            [],
        )
        .unwrap();
        let gate =
            NativeSessionHandoffGate::with_identity(PathBuf::from("test-root"), identity.clone());
        Fixture {
            conn,
            gate,
            identity,
            binding_digest,
        }
    }

    fn request(fixture: &Fixture, phase: SessionHandoffPhase) -> SessionHandoffGateRequest {
        SessionHandoffGateRequest {
            phase,
            binding_digest: fixture.binding_digest.clone(),
            audience: "scope-1".into(),
            nonce: "nonce-1".into(),
            spec: ExecutionSpec {
                job_id: "job-1".into(),
                session_id: "session-1".into(),
                scope_id: "scope-1".into(),
                harness: "harness-a".into(),
                harness_version: "1.0".into(),
                model_route_id: "route-1".into(),
                gateway_account_id: "account-1".into(),
                model_id: "model-1".into(),
            },
            checkpoint_digest: fixture.binding_digest.clone(),
            checkpoint_sequence: 7,
            ownership: ExecutionOwnership {
                node_id: 1,
                generation: 2,
            },
        }
    }

    #[test]
    fn authorized_disclose_mints_a_verifiable_current_permit() {
        let fixture = fixture();
        let permit = fixture
            .gate
            .authorize_with_conn(
                &fixture.conn,
                &request(&fixture, SessionHandoffPhase::Disclose),
            )
            .unwrap();
        verify_session_handoff_permit(
            &permit,
            &fixture.identity.public_identity(),
            "scope-1",
            "nonce-1",
        )
        .unwrap();
        assert_eq!(permit.principal_epoch, 11);
        assert_eq!(permit.binding_revision, 3);
        assert_eq!(permit.checkpoint_sequence, 7);
        assert_eq!(permit.ownership_generation, 2);
        assert!(permit.expires_at_ms > permit.issued_at_ms);
    }

    #[test]
    fn unknown_or_revoked_binding_denies() {
        let fixture = fixture();
        let mut unknown = request(&fixture, SessionHandoffPhase::Disclose);
        unknown.binding_digest = digest("cd");
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(&fixture.conn, &unknown)
                .unwrap_err()
                .reason_code,
            "binding_unknown"
        );
        fixture
            .conn
            .execute(
                "UPDATE business_session_handoff_bindings SET state = 'revoked'
                 WHERE binding_id = 'binding-1'",
                [],
            )
            .unwrap();
        // A revocation after an earlier allow must deny the very next call:
        // there is no cached affirmative.
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(
                    &fixture.conn,
                    &request(&fixture, SessionHandoffPhase::Disclose),
                )
                .unwrap_err()
                .reason_code,
            "binding_revoked"
        );
    }

    #[test]
    fn wrong_instance_or_missing_grant_denies() {
        let fixture = fixture();
        let foreign =
            NativeSessionHandoffGate::with_identity(PathBuf::from("test-root"), identity());
        assert_eq!(
            foreign
                .authorize_with_conn(
                    &fixture.conn,
                    &request(&fixture, SessionHandoffPhase::Disclose),
                )
                .unwrap_err()
                .reason_code,
            "wrong_instance"
        );
        fixture
            .conn
            .execute("DELETE FROM business_permission_grants", [])
            .unwrap();
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(
                    &fixture.conn,
                    &request(&fixture, SessionHandoffPhase::Disclose),
                )
                .unwrap_err()
                .reason_code,
            "grant_missing"
        );
    }

    #[test]
    fn tampered_request_fields_deny_as_binding_mismatch() {
        let fixture = fixture();
        let mut request = request(&fixture, SessionHandoffPhase::Disclose);
        request.checkpoint_sequence = 8;
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(&fixture.conn, &request)
                .unwrap_err()
                .reason_code,
            "binding_mismatch"
        );
        let mut request = request(&fixture, SessionHandoffPhase::Disclose);
        request.spec.gateway_account_id = "account-2".into();
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(&fixture.conn, &request)
                .unwrap_err()
                .reason_code,
            "binding_mismatch"
        );
        // The source-side binding cannot authorize a target-side phase.
        assert_eq!(
            fixture
                .gate
                .authorize_with_conn(
                    &fixture.conn,
                    &request(&fixture, SessionHandoffPhase::Resume),
                )
                .unwrap_err()
                .reason_code,
            "binding_mismatch"
        );
    }
}
