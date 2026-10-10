// Origin: CTOX
// License: AGPL-3.0-only
//! Existing ownership -> original protected guest names -> fresh native target controller.
//! Enrollment never creates a provider, imports a VM or submits a Core turn.
use super::*;
use ctox_sync::authority::Ownership;

type Registry = super::super::super::super::NativeGuestRegistry;
type Scope = super::super::super::super::guest_registry::target_handoff::TargetPolicyScope;

pub(super) fn verify_owned_target(
    policy: &Connection,
    request: &SessionHandoffGateRequest,
    permit: &SessionHandoffPermit,
    next: &Ownership,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        next.node_id != request.ownership.node_id
            && request.ownership.generation.checked_add(1) == Some(next.generation),
        "target must have the exact next ownership"
    );
    let owned: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_checkpoint_takeovers
         WHERE binding_digest=?1 AND checkpoint_digest=?2 AND source_generation=?3
         AND spec_json=?4 AND source_ownership_json=?5 AND target_ownership_json=?6
         AND principal_epoch=?7 AND binding_revision=?8 AND phase='Owned')",
        params![
            request.binding_digest,
            request.checkpoint_digest,
            i64::try_from(request.ownership.generation)?,
            serde_json::to_string(&request.spec)?,
            serde_json::to_string(&request.ownership)?,
            serde_json::to_string(next)?,
            i64::try_from(permit.principal_epoch)?,
            i64::try_from(permit.binding_revision)?
        ],
        |r| r.get(0),
    )?;
    anyhow::ensure!(owned, "target has no completed native takeover; reconcile");
    Ok(())
}

fn owned_target<P: Clone + Eq + Hash + Send + Sync + 'static>(
    target: &Target<P>,
    registry: &Registry,
    permit: &SessionHandoffPermit,
    policy: &Connection,
) -> anyhow::Result<(Ownership, Scope)> {
    let authority = registry.checkpoint_authority(&target.server.gate.root)?;
    let next = Ownership {
        node_id: authority.node_id(),
        generation: target
            .request
            .ownership
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("ownership generation exhausted"))?,
    };
    anyhow::ensure!(
        next.node_id != target.request.ownership.node_id,
        "target must be independent"
    );
    verify_owned_target(policy, &target.request, permit, &next)?;
    let encoded: String = policy.query_row(
        "SELECT n.target_scope_json FROM business_native_target_handoff_bindings n
         JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id
         WHERE b.binding_digest=?1",
        [&target.request.binding_digest],
        |r| r.get(0),
    )?;
    let scope: Scope = serde_json::from_str(&encoded)?;
    let current = super::super::super::super::guest_registry::target_handoff::resolve(
        &target.server.gate.root,
        policy,
        &scope,
        &target.request.spec,
    )?;
    anyhow::ensure!(current == scope, "target native assignment changed");
    Ok((next, current))
}

pub(super) async fn enroll<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<Registry>,
    binding: String,
) -> anyhow::Result<CopyResponse> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (server, registry, binding);
        anyhow::bail!("protected guest continuation requires the retained Linux QEMU runtime");
    }
    #[cfg(target_os = "linux")]
    {
        let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
        let live = lifetime.0.clone();
        let preparing = registry.clone();
        let (target, authority, ownership, scope, identity) = tokio::task::spawn_blocking(move || {
            let target = Arc::new(Target::prepare(
                server, &binding, None, live, SessionHandoffPhase::Resume,
            )?);
            let authority = preparing.checkpoint_authority(&target.server.gate.root)?;
            let (ownership, scope) = target.current_policy(&target.request, |permit, _, policy|
                owned_target(&target, &preparing, permit, policy))?;
            let store = takeover::received_store(&target)?;
            // Full content verification and metadata reads hold no native
            // account/issuer/policy/controller publication fence.
            store.verify_durable_copy(&target.request.checkpoint_digest)?;
            let manifest = store.load_manifest(&target.request.checkpoint_digest)?;
            let identity = super::super::super::super::guest_registry::checkpoint_identity::ProtectedNativeGuestIdentity::from_checkpoint(
                &store, &manifest, &target.request.spec,
            )?;
            Ok::<_, anyhow::Error>((target, authority, ownership, scope, identity))
        }).await??;
        let job = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            authority.validate_ownership(&target.request.spec.job_id, &ownership),
        )
        .await
        .map_err(|_| anyhow::anyhow!("target ownership deadline"))??;
        anyhow::ensure!(
            job.spec == target.request.spec
                && job.ownership == ownership
                && !job.stopped
                && job.pending_effects.is_empty()
                && !job.checkpoint_requires_refresh
                && job
                    .checkpoint
                    .as_ref()
                    .is_some_and(|c| c.digest == target.request.checkpoint_digest
                        && c.sequence == target.request.checkpoint_sequence
                        && c.replicas.contains(&ownership.node_id)),
            "target ownership/checkpoint changed"
        );
        tokio::task::spawn_blocking(move || {
            target.current_policy(&target.request, |permit, _, policy| {
                let (current, current_scope) = owned_target(&target, &registry, permit, policy)?;
                anyhow::ensure!(
                    current == ownership && current_scope == scope,
                    "target enrollment changed after quorum await"
                );
                let imports = target.server.gate.root.join("runtime/ctox-sync/guests");
                private_dir(&imports)?;
                use std::os::unix::fs::PermissionsExt;
                let parent = tempfile::Builder::new()
                    .prefix("enrollment-")
                    .permissions(std::fs::Permissions::from_mode(0o700))
                    .tempdir_in(imports)?;
                let assignment = registry.enroll_protected_target(
                    policy,
                    &scope,
                    &identity,
                    &target.request.binding_digest,
                    &target.request.checkpoint_digest,
                    &target.request.spec,
                    &ownership,
                    parent.path(),
                )?;
                if assignment.destination.import_parent == parent.path() {
                    parent.keep();
                }
                Ok(CopyResponse::GuestEnrolled {
                    checkpoint_digest: target.request.checkpoint_digest.clone(),
                    guest_id: assignment.destination.guest_id,
                    controller_id: assignment.destination.controller_id,
                    controller_generation: assignment.destination.controller_generation,
                })
            })
        })
        .await?
    }
}
