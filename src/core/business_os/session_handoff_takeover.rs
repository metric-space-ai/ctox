// Origin: CTOX
// License: AGPL-3.0-only
//! Existing-job target ownership only. No Core turn or VM activation.
use super::*;
use ctox_sync::authority::{client::take_over_checkpoint, Command, Ownership, Request};

type Registry = super::super::super::super::NativeGuestRegistry;

fn validate_received<P: Clone + Eq + Hash + Send + Sync + 'static>(
    target: &Target<P>,
    store: &CheckpointStore,
) -> anyhow::Result<()> {
    target.current(&target.request, |_, _| {
        anyhow::ensure!(
            target.request.phase == SessionHandoffPhase::Resume,
            "native takeover requires target execution authority"
        );
        // Actual source unknown-effects captures fail here before any pending
        // takeover record or quorum command. Never clear a marker at intake.
        reconstruction::verify_manifest(
            &store.load(&target.request.checkpoint_digest)?,
            &target.request,
        )
    })
}

fn received_store<P: Clone + Eq + Hash + Send + Sync + 'static>(
    target: &Target<P>,
) -> anyhow::Result<CheckpointStore> {
    let path = target
        .server
        .gate
        .root
        .join("runtime/ctox-sync/received-checkpoints");
    private_dir(&path)?;
    let store = CheckpointStore::open(path, BLOB_LIMIT)?;
    validate_received(target, &store)?;
    Ok(store)
}

pub(super) async fn take_over<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<Registry>,
    binding: String,
) -> anyhow::Result<CopyResponse> {
    let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
    let live = lifetime.0.clone();
    let preparing_registry = registry.clone();
    let (target, authority, next) = tokio::task::spawn_blocking(move || {
        let target = Arc::new(Target::prepare(
            server,
            &binding,
            None,
            live,
            SessionHandoffPhase::Resume,
        )?);
        let authority = preparing_registry.checkpoint_authority(&target.server.gate.root)?;
        anyhow::ensure!(
            authority.scope_id() == target.request.spec.scope_id
                && authority.node_id() != target.request.ownership.node_id,
            "native takeover needs the independently enrolled target"
        );
        let next = Ownership {
            node_id: authority.node_id(),
            generation: target
                .request
                .ownership
                .generation
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("native ownership generation exhausted"))?,
        };
        let store = received_store(&target)?;
        // Full content verification releases account/issuer/policy/host locks.
        store.verify_durable_copy(&target.request.checkpoint_digest)?;
        target.current(&target.request, |_, _| {
            preparing_registry.checkpoint_authority(&target.server.gate.root)?;
            Ok(())
        })?;
        Ok::<_, anyhow::Error>((target, authority, next))
    })
    .await??;
    // TakeOver validates source ownership and the protected target copy inside
    // the quorum. Worker clients cannot authorize a foreign source executor.
    let t = target.clone();
    let r = registry.clone();
    let a = authority.clone();
    let expected_next = next.clone();
    let resume = tokio::task::spawn_blocking(move || {
        t.current_policy(&t.request, |permit, _, policy| {
            r.checkpoint_authority(&t.server.gate.root)?;
            anyhow::ensure!(a.node_id() == expected_next.node_id, "target node changed");
            // Committed BEFORE the remote submit. Lost/replayed/rejected or
            // cancelled requests remain Pending and cannot become another try.
            policy.execute_batch(
                "CREATE TABLE IF NOT EXISTS business_native_checkpoint_takeovers (
                 binding_digest TEXT NOT NULL, checkpoint_digest TEXT NOT NULL,
                 source_generation INTEGER NOT NULL, request_id TEXT NOT NULL UNIQUE,
                 spec_json TEXT NOT NULL, source_ownership_json TEXT NOT NULL,
                 target_ownership_json TEXT NOT NULL, principal_epoch INTEGER NOT NULL,
                 binding_revision INTEGER NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('Pending','Owned')),
                 PRIMARY KEY(binding_digest,checkpoint_digest,source_generation))")?;
            policy.execute(
                "INSERT INTO business_native_checkpoint_takeovers
                 (binding_digest,checkpoint_digest,source_generation,request_id,spec_json,
                  source_ownership_json,target_ownership_json,principal_epoch,binding_revision,phase)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'Pending')",
                rusqlite::params![t.request.binding_digest,t.request.checkpoint_digest,
                    i64::try_from(t.request.ownership.generation)?,t.request.nonce,
                    serde_json::to_string(&t.request.spec)?,serde_json::to_string(&t.request.ownership)?,
                    serde_json::to_string(&expected_next)?,i64::try_from(permit.principal_epoch)?,
                    i64::try_from(permit.binding_revision)?])?;
            Ok(permit.clone())
        })
    }).await??;
    take_over_checkpoint(
        authority.as_ref(),
        Request {
            request_id: target.request.nonce.clone(),
            actor: authority.node_id(),
            command: Command::TakeOver {
                job_id: target.request.spec.job_id.clone(),
                expected: target.request.ownership.clone(),
                checkpoint_digest: target.request.checkpoint_digest.clone(),
                owner: authority.node_id(),
                resume,
            },
        },
        &target.request.spec,
        target.request.checkpoint_sequence,
    )
    .await?;
    tokio::task::spawn_blocking(move || {
        target.current_policy(&target.request, |permit, _, policy| {
            registry.checkpoint_authority(&target.server.gate.root)?;
            let changed = policy.execute(
                "UPDATE business_native_checkpoint_takeovers SET phase='Owned'
                 WHERE binding_digest=?1 AND checkpoint_digest=?2 AND source_generation=?3
                 AND request_id=?4 AND spec_json=?5 AND source_ownership_json=?6
                 AND target_ownership_json=?7 AND principal_epoch=?8 AND binding_revision=?9 AND phase='Pending'",
                rusqlite::params![target.request.binding_digest,target.request.checkpoint_digest,
                    i64::try_from(target.request.ownership.generation)?,target.request.nonce,
                    serde_json::to_string(&target.request.spec)?,serde_json::to_string(&target.request.ownership)?,
                    serde_json::to_string(&next)?,i64::try_from(permit.principal_epoch)?,
                    i64::try_from(permit.binding_revision)?])?;
            anyhow::ensure!(changed == 1, "native pending takeover changed; reconcile");
            Ok(CopyResponse::OwnershipTaken {
                checkpoint_digest: target.request.checkpoint_digest.clone(),
                job_id: target.request.spec.job_id.clone(), session_id: target.request.spec.session_id.clone(),
                ownership: next.clone(),
            })
        })
    }).await?
}

#[cfg(test)]
pub(super) fn assert_dirty_copy_cannot_take_over<P: Clone + Eq + Hash + Send + Sync + 'static>(
    received: &Target<P>,
    store: &CheckpointStore,
) {
    let mut request = received.request.clone();
    request.phase = SessionHandoffPhase::Resume;
    let original = received.server.gate.authorize(&request).unwrap();
    let target = Target {
        request,
        original,
        auth: received.auth.clone(),
        server: received.server.clone(),
        source_identity: received.source_identity.clone(),
        peer_guard: None,
        live: Arc::new(Mutex::new(true)),
    };
    let error = validate_received(&target, store).unwrap_err();
    assert!(
        error.to_string().contains("unreconciled effects"),
        "{error:#}"
    );
    target.current_policy(&target.request, |_, _, policy| {
        let pending: bool = policy.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_native_checkpoint_takeovers')",
            [], |row| row.get(0))?;
        anyhow::ensure!(!pending, "dirty intake installed a pending takeover");
        Ok(())
    }).unwrap();
}
