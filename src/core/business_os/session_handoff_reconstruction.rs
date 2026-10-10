// Origin: CTOX
// License: AGPL-3.0-only
//! Target-local workspace preparation. This never starts Core or changes ownership.
use super::*;
use ctox_sync::contracts::WorkspaceEntryKind;
use std::io::{Read, Write};

#[cfg(test)]
#[path = "session_handoff_core_import_tests.rs"]
mod core_import_tests;

struct StagedWorkspace {
    directory: tempfile::TempDir,
    checkpoint_digest: String,
}

pub(super) fn verify_manifest(
    manifest: &CheckpointManifest,
    request: &SessionHandoffGateRequest,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        manifest.sequence == request.checkpoint_sequence
            && manifest.session.session_id == request.spec.session_id
            && manifest.session.scope_id == request.spec.scope_id
            && manifest.session.harness == request.spec.harness
            && manifest.session.harness_version == request.spec.harness_version
            && manifest.session.gateway_account_id == request.spec.gateway_account_id
            && manifest.session.model_route_id == request.spec.model_route_id
            && manifest.session.model_id == request.spec.model_id
            && manifest.session.required_capabilities == request.spec.required_capabilities
            && manifest.pending_effects.is_empty(),
        "checkpoint differs from target enrollment or has unreconciled effects"
    );
    Ok(())
}

async fn stage<P: Clone + Eq + Hash + Send + Sync + 'static>(
    target: Arc<Target<P>>,
    store: Arc<CheckpointStore>,
) -> anyhow::Result<StagedWorkspace> {
    let t = target.clone();
    let (manifest, directory, bundle, store) = tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            anyhow::ensure!(
                t.request.phase == SessionHandoffPhase::Resume,
                "workspace preparation requires execute permission"
            );
            let manifest = store.load(&t.request.checkpoint_digest)?;
            verify_manifest(&manifest, &t.request)?;
            // Only already protected checkpoint bytes reach the Core decoder;
            // it grants neither clean effects nor permission to start a turn.
            let states: Vec<_> = manifest
                .provider_state
                .iter()
                .filter(|e| {
                    e.path == "native-session-state.json" && e.kind == WorkspaceEntryKind::File
                })
                .collect();
            anyhow::ensure!(states.len() == 1, "native Core state absent or ambiguous");
            let mut bytes = Vec::new();
            store
                .open_blob(&states[0].artifact)?
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            let _decoded = ctox_core::NativeSessionState::from_checkpoint(
                &bytes,
                ctox_protocol::ThreadId::from_string(&manifest.session.session_id)?,
                &manifest.session.model_id,
                &manifest.session.model_route_id,
            )?;
            let mut bundles = manifest
                .provider_state
                .iter()
                .filter(|e| e.path == "native-workspace.bundle");
            let bundle = bundles
                .next()
                .filter(|e| e.kind == WorkspaceEntryKind::File)
                .ok_or_else(|| anyhow::anyhow!("native workspace bundle absent"))?
                .artifact
                .clone();
            anyhow::ensure!(
                bundles.next().is_none(),
                "native workspace bundle is ambiguous"
            );
            let parent = t.server.gate.root.join("runtime/ctox-sync");
            private_dir(&parent)?;
            let parent = parent.join("prepared-workspaces");
            create_private(&parent)?;
            let directory = tempfile::Builder::new()
                .prefix("workspace-")
                .tempdir_in(parent)?;
            Ok((manifest, directory, bundle, store))
        })
    })
    .await??;

    // No account, policy, host or peer lock survives Git IO. Only a private,
    // non-executable stage exists until the post-await publication check.
    let reconstructed = store
        .reconstruct_workspace_from_bundle(
            &target.request.checkpoint_digest,
            &bundle,
            &directory.path().join("workspace"),
        )
        .await?;
    anyhow::ensure!(
        serde_json::to_vec(&manifest)? == serde_json::to_vec(&reconstructed)?,
        "reconstructed checkpoint changed"
    );
    let t = target.clone();
    tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            verify_manifest(&reconstructed, &t.request)?;
            Ok(StagedWorkspace {
                directory,
                checkpoint_digest: t.request.checkpoint_digest.clone(),
            })
        })
    })
    .await?
}

impl StagedWorkspace {
    fn publish<P: Clone + Eq + Hash + Send + Sync + 'static>(
        self,
        target: &Target<P>,
    ) -> anyhow::Result<(String, String)> {
        target.current_policy(&target.request, |permit, _, policy| {
            private_dir(self.directory.path())?;
            let preparation_id = self.directory.path().file_name()
                .and_then(|s| s.to_str()).ok_or_else(|| anyhow::anyhow!("workspace preparation id absent"))?.to_owned();
            let metadata = serde_json::json!({
                "version": 1, "bindingDigest": target.request.binding_digest,
                "checkpointDigest": self.checkpoint_digest, "sessionId": target.request.spec.session_id,
                "principalEpoch": permit.principal_epoch, "bindingRevision": permit.binding_revision,
                "ownership": target.request.ownership, "resumed": false
            });
            let mut marker = std::fs::OpenOptions::new().write(true).create_new(true)
                .open(self.directory.path().join("prepared.json"))?;
            marker.write_all(&serde_json::to_vec(&metadata)?)?;
            marker.sync_all()?;
            std::fs::File::open(self.directory.path())?.sync_all()?;
            let binding_id: String = policy.query_row(
                "SELECT binding_id FROM business_session_handoff_bindings WHERE binding_digest=?1",
                [&target.request.binding_digest], |r| r.get(0))?;
            super::super::super::super::store::insert_business_event(
                policy, "business_session_handoff_bindings", &binding_id,
                "business_os.session_handoff.workspace_prepared",
                serde_json::json!({"version":1,"binding_digest":target.request.binding_digest,
                    "checkpoint_digest":self.checkpoint_digest,"session_id":target.request.spec.session_id,
                    "preparation_id":preparation_id,"principal_epoch":permit.principal_epoch,
                    "binding_revision":permit.binding_revision,"resumed":false}), now_ms() as i64)?;
            let _path = self.directory.keep();
            Ok((self.checkpoint_digest, preparation_id))
        })
    }
}

pub(super) async fn reconstruct<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    binding: String,
) -> anyhow::Result<(String, String)> {
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
            Ok(Arc::new(CheckpointStore::open(path, BLOB_LIMIT)?))
        })
    })
    .await??;
    let staged = stage(target.clone(), store).await?;
    tokio::task::spawn_blocking(move || staged.publish(&target)).await?
}

#[cfg(test)]
pub(super) fn assert_native_reconstruction<P: Clone + Eq + Hash + Send + Sync + 'static>(
    received_target: &Target<P>,
    store: &Arc<CheckpointStore>,
) {
    let mut request = received_target.request.clone();
    request.phase = SessionHandoffPhase::Resume;
    request.nonce = fresh_nonce().unwrap();
    let original = received_target.server.gate.authorize(&request).unwrap();
    let target = Arc::new(Target {
        server: received_target.server.clone(),
        request,
        original,
        source_identity: received_target.source_identity.clone(),
        auth: received_target.auth.clone(),
        peer_guard: None,
        live: received_target.live.clone(),
    });
    let store = store.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let parent = target
        .server
        .gate
        .root
        .join("runtime/ctox-sync/prepared-workspaces");
    let entries = || std::fs::read_dir(&parent).map(|r| r.count()).unwrap_or(0);
    assert_eq!(entries(), 0);
    let manifest = store.load(&target.request.checkpoint_digest).unwrap();
    assert!(!manifest.pending_effects.is_empty());
    let before = serde_json::to_vec(&manifest).unwrap();
    let error = runtime
        .block_on(stage(target.clone(), store.clone()))
        .err()
        .expect("actual native capture requires effect reconciliation");
    assert!(
        error.to_string().contains("unreconciled effects"),
        "{error}"
    );
    assert_eq!(entries(), 0, "dirty capture creates no private stage");
    assert_eq!(
        serde_json::to_vec(&store.load(&target.request.checkpoint_digest).unwrap()).unwrap(),
        before,
        "target preparation never clears source effect evidence"
    );
    let policy = Connection::open(business_os_store_path(&target.server.gate.root)).unwrap();
    let audited: i64 = policy.query_row(
        "SELECT count(*) FROM business_events WHERE command_type='business_os.session_handoff.workspace_prepared'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(audited, 0);

    // Isolated publication fixtures exercise the real native authority fence;
    // these empty directories are not a reconstructed native session.
    create_private(&parent).unwrap();
    let fixture = || StagedWorkspace {
        directory: tempfile::Builder::new()
            .prefix("workspace-")
            .tempdir_in(&parent)
            .unwrap(),
        checkpoint_digest: target.request.checkpoint_digest.clone(),
    };
    let staged = fixture();
    policy
        .execute(
            "UPDATE business_session_handoff_bindings SET state='revoked' WHERE binding_digest=?1",
            [&target.request.binding_digest],
        )
        .unwrap();
    assert!(
        staged.publish(&target).is_err(),
        "revocation fences publication"
    );
    assert_eq!(entries(), 0);
    assert!(runtime
        .block_on(stage(target.clone(), store.clone()))
        .is_err());
    assert_eq!(entries(), 0);
    policy
        .execute(
            "UPDATE business_session_handoff_bindings SET state='active' WHERE binding_digest=?1",
            [&target.request.binding_digest],
        )
        .unwrap();
    let staged = fixture();
    *target.live.lock().unwrap() = false;
    assert!(
        staged.publish(&target).is_err(),
        "client retirement fences publication"
    );
    assert_eq!(entries(), 0);
    *target.live.lock().unwrap() = true;
}

#[cfg(test)]
#[test]
fn checkpoint_control_keeps_copy_requests_compatible_and_rejects_path_inputs() {
    let old = serde_json::json!({"bindingDigest":"a".repeat(64),"sourceRoute":"peer"});
    let request: CopyRequest = serde_json::from_value(old.clone()).unwrap();
    assert!(!request.reconstruct);
    assert_eq!(serde_json::to_value(request).unwrap(), old);
    let mut reconstruct = old.clone();
    reconstruct["reconstruct"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<CopyRequest>(reconstruct.clone())
            .unwrap()
            .reconstruct
    );
    reconstruct["targetPath"] = serde_json::json!("/client/supplied/path");
    assert!(serde_json::from_value::<CopyRequest>(reconstruct).is_err());
    let offline = serde_json::json!({"bindingDigest":"a".repeat(64),"reconstruct":true});
    let request: CopyRequest = serde_json::from_value(offline.clone()).unwrap();
    assert!(
        request.source_route.is_empty(),
        "local reconstruction needs no live source route"
    );
    assert_eq!(serde_json::to_value(request).unwrap(), offline);
}
