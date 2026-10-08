// Origin: CTOX
// License: AGPL-3.0-only
//! Target import under the same live account/issuer/policy/guest owner.
//! This completes import and retains its owner for the original Core constructor.
//! It cannot restore a VM or start a Core turn.
use super::super::super::super::guest_registry::target_import::NativeGuestImportFence;
use super::*;
use ctox_sync::guest_restore::GuestRestoreDestination;

impl<P: Clone + Eq + Hash + Send + Sync + 'static> NativeGuestImportFence for Target<P> {
    fn with_current_checkpoint(
        &self,
        policy: &Connection,
        identity: &SigningIdentity,
        destination: &GuestRestoreDestination,
        binding: &str,
        digest: &str,
        spec: &ctox_sync::contracts::ExecutionSpec,
        ownership: &ctox_sync::authority::Ownership,
        publish: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        if binding != self.request.binding_digest
            || digest != self.request.checkpoint_digest
            || *spec != self.request.spec
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "protected import belongs to another native binding/checkpoint",
            ));
        }
        self.with_current(policy, identity, destination, &mut || {
            let permit = self
                .server
                .gate
                .resolve_fenced(policy, identity, &self.request)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            guest_enrollment::verify_owned_target(policy, &self.request, &permit, ownership)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            publish()
        })
    }
    fn with_current(
        &self,
        policy: &Connection,
        identity: &SigningIdentity,
        destination: &GuestRestoreDestination,
        publish: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let mut action = || -> anyhow::Result<()> {
            let _account = self.auth.current_runtime_account_guard()?;
            anyhow::ensure!(
                self.request.phase == SessionHandoffPhase::Resume,
                "guest import requires target execution permission"
            );
            for phase in [SessionHandoffPhase::Receive, SessionHandoffPhase::Resume] {
                let mut request = self.request.clone();
                request.phase = phase;
                let permit = self
                    .server
                    .gate
                    .resolve_fenced(policy, identity, &request)?;
                anyhow::ensure!(
                    operation_authority_matches(&self.original, &permit),
                    "target authority changed during guest import"
                );
            }
            let encoded: String = policy.query_row(
                "SELECT n.target_scope_json FROM business_native_target_handoff_bindings n
                 JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id
                 WHERE b.binding_digest=?1",
                [&self.request.binding_digest],
                |row| row.get(0),
            )?;
            let scope: super::super::super::super::guest_registry::target_handoff::TargetPolicyScope =
                serde_json::from_str(&encoded)?;
            anyhow::ensure!(
                destination.human_owner_id == scope.owner_user_id
                    && destination.project_id == scope.project_id
                    && destination.thread_id == scope.thread_id
                    && destination.worker_profile_id == scope.worker_profile_id,
                "guest does not belong to the enrolled target scope"
            );
            // These actual guards survive the publication callback. The guest
            // owner already holds worker, issuer, policy and controller fences.
            let ledger = self.server.lock_ledger()?;
            anyhow::ensure!(ledger.alive, "handoff host retired");
            let live = self
                .live
                .lock()
                .map_err(|_| anyhow::anyhow!("guest import retired"))?;
            anyhow::ensure!(*live, "guest import retired");
            publish()?;
            Ok(())
        };
        action().map_err(|error| std::io::Error::other(error.to_string()))
    }
}

pub(super) async fn import<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<super::super::super::super::NativeGuestRegistry>,
    binding: String,
    guest_id: String,
) -> anyhow::Result<CopyResponse> {
    let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
    let live = lifetime.0.clone();
    let target = Arc::new(
        tokio::task::spawn_blocking(move || {
            Target::prepare(server, &binding, None, live, SessionHandoffPhase::Resume)
        })
        .await??,
    );
    let t = target.clone();
    let store = tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            let path = t
                .server
                .gate
                .root
                .join("runtime/ctox-sync/received-checkpoints");
            private_dir(&path)?;
            let store = CheckpointStore::open(path, BLOB_LIMIT)?;
            // In particular, the protected native-effects unknown marker stays
            // pending. Target intake never clears it to obtain an import receipt.
            reconstruction::verify_manifest(
                &store.load(&t.request.checkpoint_digest)?,
                &t.request,
            )?;
            Ok(store)
        })
    })
    .await??;
    let receipt = registry
        .import_received_checkpoint(
            &guest_id,
            &target.request.spec,
            &target.request.ownership,
            &store,
            &target.request.checkpoint_digest,
            target.as_ref(),
        )
        .await?;
    registry.retain_core_owner(
        &guest_id,
        Arc::new(guest_core::ReceiverCore {
            target: target.clone(),
            lifetime,
        }),
    )?;
    Ok(CopyResponse::GuestImported {
        checkpoint_digest: receipt.checkpoint_digest,
        guest_id: receipt.destination.guest_id,
        controller_id: receipt.destination.controller_id,
        controller_generation: receipt.destination.controller_generation,
        effect_id: receipt.effect_id,
    })
}
#[cfg(test)]
pub(super) fn assert_native_import_fence<P: Clone + Eq + Hash + Send + Sync + 'static>(
    received: &Target<P>,
    store: &CheckpointStore,
) {
    // Real copy/policy/account fixture; no live provider or VM acceptance claim.
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
    let mut publications = 0;
    target.server.gate.with_current_authority(|policy, identity| {
        let encoded: String = policy.query_row(
            "SELECT n.target_scope_json FROM business_native_target_handoff_bindings n
             JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id
             WHERE b.binding_digest=?1", [&target.request.binding_digest], |row| row.get(0),
        ).unwrap();
        let scope: super::super::super::super::guest_registry::target_handoff::TargetPolicyScope =
            serde_json::from_str(&encoded).unwrap();
        let destination = GuestRestoreDestination {
            instance_id: "native-fixture-instance".into(), guest_id: "native-fixture-guest".into(),
            human_owner_id: scope.owner_user_id, project_id: scope.project_id,
            thread_id: scope.thread_id, worker_profile_id: scope.worker_profile_id,
            controller_id: "native-fixture-controller".into(), controller_generation: 1,
            import_parent: target.server.gate.root.clone(),
        };
        let mut publish = || { publications += 1; Ok(()) };
        NativeGuestImportFence::with_current(&target, policy, identity, &destination, &mut publish).unwrap();
        let next = ctox_sync::authority::Ownership {
            node_id: target.request.ownership.node_id + 1,
            generation: target.request.ownership.generation + 1,
        };
        // This is a genuine received-copy/account/policy fixture, not a
        // completed takeover. It cannot authorize the pre-provider import.
        for field in 0..4 {
            let mut binding = target.request.binding_digest.clone();
            let mut digest = target.request.checkpoint_digest.clone();
            let mut spec = target.request.spec.clone();
            match field {
                0 => binding = "foreign-binding".into(),
                1 => digest = "foreign-checkpoint".into(),
                2 => spec.session_id = "foreign-session".into(),
                _ => {}
            }
            assert!(NativeGuestImportFence::with_current_checkpoint(
                &target, policy, identity, &destination, &binding, &digest,
                &spec, &next, &mut publish,
            ).is_err());
        }
        for field in 0..4 {
            let mut foreign = destination.clone();
            match field {
                0 => foreign.human_owner_id = "foreign-owner".into(),
                1 => foreign.project_id = "foreign-project".into(),
                2 => foreign.thread_id = "foreign-chat".into(),
                _ => foreign.worker_profile_id = "foreign-profile".into(),
            }
            assert!(NativeGuestImportFence::with_current(&target, policy, identity, &foreign, &mut publish).is_err());
        }
        *target.live.lock().unwrap() = false;
        assert!(NativeGuestImportFence::with_current(&target, policy, identity, &destination, &mut publish).is_err());
        *target.live.lock().unwrap() = true;
        policy.execute("UPDATE business_session_handoff_bindings SET state='revoked' WHERE binding_digest=?1",
            [&target.request.binding_digest]).unwrap();
        assert!(NativeGuestImportFence::with_current(&target, policy, identity, &destination, &mut publish).is_err());
        policy.execute("UPDATE business_session_handoff_bindings SET state='active' WHERE binding_digest=?1",
            [&target.request.binding_digest]).unwrap();
        Ok(())
    }).unwrap();
    assert_eq!(publications, 1);
    let manifest = store.load(&target.request.checkpoint_digest).unwrap();
    assert!(
        reconstruction::verify_manifest(&manifest, &target.request).is_err(),
        "actual unknown-effect capture cannot be imported"
    );
    assert!(!manifest.pending_effects.is_empty());
}
