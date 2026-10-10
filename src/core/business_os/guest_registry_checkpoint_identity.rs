// Origin: CTOX
// License: AGPL-3.0-only
//! Runtime identity from the protected native manifest, never a caller's mode flag.
use super::*;
use ctox_sync::checkpoint::CheckpointStore;
use ctox_sync::contracts::{CheckpointManifest, WorkspaceEntryKind};
use sha2::{Digest, Sha256};
use std::io::Read;

pub(super) const CORE_RUNTIME_PATH: &str = "native-core-runtime.json";

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CoreRuntimeIdentity {
    pub(super) version: u32,
    pub(super) guest_id: String,
    pub(super) session_id: String,
}

pub(in crate::business_os) struct ProtectedNativeGuestIdentity {
    guest_id: String,
    service_session: Option<String>,
}

impl ProtectedNativeGuestIdentity {
    pub(in crate::business_os) fn from_checkpoint(
        store: &CheckpointStore,
        manifest: &CheckpointManifest,
        spec: &ExecutionSpec,
    ) -> Result<Self> {
        ensure!(
            manifest.pending_effects.is_empty(),
            "native checkpoint effects remain unresolved"
        );
        ensure!(
            manifest.session.version == 1
                && manifest.session.scope_id == spec.scope_id
                && manifest.session.session_id == spec.session_id
                && manifest.session.harness == spec.harness
                && manifest.session.harness_version == spec.harness_version
                && manifest.session.gateway_account_id == spec.gateway_account_id
                && manifest.session.model_route_id == spec.model_route_id
                && manifest.session.model_id == spec.model_id
                && manifest.session.required_capabilities == spec.required_capabilities,
            "native runtime manifest differs from enrolled execution"
        );
        let core: Vec<_> = manifest
            .provider_state
            .iter()
            .filter(|entry| entry.path == CORE_RUNTIME_PATH)
            .collect();
        let machine = manifest
            .provider_state
            .iter()
            .any(|entry| entry.path.starts_with("native-guest-"));
        if machine {
            ensure!(
                core.is_empty(),
                "checkpoint mixes Core-only and machine identities"
            );
            #[cfg(target_os = "linux")]
            {
                let identity =
                    super::super::guest_runtime::ProtectedGuestIdentity::from_checkpoint(
                        store,
                        &manifest.provider_state,
                    )?;
                return Ok(Self::from_machine(&identity));
            }
            #[cfg(not(target_os = "linux"))]
            anyhow::bail!("machine checkpoint requires the retained Linux QEMU runtime");
        }
        ensure!(
            core.len() == 1 && core[0].kind == WorkspaceEntryKind::File,
            "Core-only native runtime identity is absent or ambiguous"
        );
        let identity: CoreRuntimeIdentity =
            serde_json::from_slice(&read_blob(store, &core[0].artifact, 4096)?)?;
        ensure!(
            identity.version == 1
                && identifier(&identity.guest_id)
                && identity.session_id == spec.session_id,
            "Core-only native runtime identity differs from original session"
        );
        let states: Vec<_> = manifest
            .provider_state
            .iter()
            .filter(|entry| {
                entry.path == "native-session-state.json" && entry.kind == WorkspaceEntryKind::File
            })
            .collect();
        ensure!(
            states.len() == 1,
            "original Core state is absent or ambiguous"
        );
        let state = ctox_core::NativeSessionState::from_checkpoint(
            &read_blob(store, &states[0].artifact, 64 * 1024 * 1024)?,
            ctox_protocol::ThreadId::from_string(&spec.session_id)?,
            &spec.model_id,
            &spec.model_route_id,
        )?;
        source_journal::validate_session_state(spec, &state)?;
        Ok(Self {
            guest_id: identity.guest_id,
            service_session: None,
        })
    }

    #[cfg(target_os = "linux")]
    pub(super) fn from_machine(
        identity: &super::super::guest_runtime::ProtectedGuestIdentity,
    ) -> Self {
        Self {
            guest_id: identity.guest_id().into(),
            service_session: Some(identity.service_session().into()),
        }
    }

    pub(in crate::business_os) fn guest_id(&self) -> &str {
        &self.guest_id
    }
    pub(in crate::business_os) fn service_session(&self) -> Option<&str> {
        self.service_session.as_deref()
    }
}

fn read_blob(
    store: &CheckpointStore,
    artifact: &ctox_sync::contracts::ArtifactRef,
    limit: u64,
) -> Result<Vec<u8>> {
    ensure!(
        artifact.size_bytes <= limit,
        "native runtime identity exceeds its bound"
    );
    let mut bytes = Vec::new();
    store
        .open_blob(artifact)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == artifact.size_bytes
            && format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256,
        "native runtime identity blob changed"
    );
    Ok(bytes)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use ctox_sync::contracts::{
        ArtifactRef, GitWorkspaceState, PendingEffect, SessionManifest, WorkspaceEntry,
    };

    fn blob(store: &CheckpointStore, bytes: &[u8]) -> Result<ArtifactRef> {
        let artifact = ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        };
        store.ingest_blob(&artifact, bytes)?;
        Ok(artifact)
    }
    // Decoder/store component regression with the actual checked Core capsule.
    // The Git/history scaffold is not an installed capture or a DATA receipt.
    pub(in crate::business_os) fn assert_core_only_identity(
        state: &ctox_core::NativeSessionState,
        spec: &ExecutionSpec,
        runtime: &[u8],
    ) -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = CheckpointStore::open(root.path().into(), 64 * 1024 * 1024)?;
        let empty = blob(&store, b"")?;
        let mut manifest = CheckpointManifest {
            version: 1,
            sequence: 1,
            session: SessionManifest {
                version: 1,
                scope_id: spec.scope_id.clone(),
                session_id: spec.session_id.clone(),
                harness: spec.harness.clone(),
                harness_version: spec.harness_version.clone(),
                model_route_id: spec.model_route_id.clone(),
                gateway_account_id: spec.gateway_account_id.clone(),
                model_id: spec.model_id.clone(),
                required_capabilities: spec.required_capabilities.clone(),
                credential_references: BTreeSet::from([spec.gateway_account_id.clone()]),
            },
            workspace_state: GitWorkspaceState {
                base_commit: "a".repeat(40),
                index_patch: empty.clone(),
                worktree_patch: empty.clone(),
                required_untracked: Vec::new(),
                deleted_paths: BTreeSet::new(),
            },
            history: vec![empty.clone()],
            attachments: Vec::new(),
            workspace: Vec::new(),
            provider_state: vec![
                WorkspaceEntry {
                    path: CORE_RUNTIME_PATH.into(),
                    kind: WorkspaceEntryKind::File,
                    artifact: blob(&store, runtime)?,
                    executable: false,
                },
                WorkspaceEntry {
                    path: "native-session-state.json".into(),
                    kind: WorkspaceEntryKind::File,
                    artifact: blob(&store, state.as_bytes())?,
                    executable: false,
                },
            ],
            pending_effects: Vec::new(),
        };
        let native = ProtectedNativeGuestIdentity::from_checkpoint(&store, &manifest, spec)?;
        assert_eq!(
            native.service_session(),
            None,
            "Core session invented a VM service session"
        );
        let original: CoreRuntimeIdentity = serde_json::from_slice(runtime)?;
        assert_eq!(native.guest_id(), original.guest_id);
        for change in 0..7 {
            let mut bad = manifest.clone();
            match change {
                0 => {
                    bad.provider_state.remove(0);
                }
                1 => bad.provider_state.push(bad.provider_state[0].clone()),
                2 => bad.provider_state[0].kind = WorkspaceEntryKind::Symlink,
                3 => bad.provider_state.push(WorkspaceEntry {
                    path: "native-guest-disk/0000.bin".into(),
                    kind: WorkspaceEntryKind::File,
                    artifact: empty.clone(),
                    executable: false,
                }),
                4 => bad.pending_effects.push(PendingEffect {
                    effect_id: "unknown-effect".into(),
                    idempotency_key: None,
                    description: "unresolved component input".into(),
                }),
                5 => bad.session.session_id = uuid::Uuid::new_v4().to_string(),
                _ => {
                    bad.provider_state.pop();
                }
            }
            assert!(ProtectedNativeGuestIdentity::from_checkpoint(&store, &bad, spec).is_err());
        }
        let mut foreign = original.clone();
        foreign.session_id = uuid::Uuid::new_v4().to_string();
        manifest.provider_state[0].artifact = blob(&store, &serde_json::to_vec(&foreign)?)?;
        assert!(ProtectedNativeGuestIdentity::from_checkpoint(&store, &manifest, spec).is_err());
        let mut unknown = serde_json::to_value(original)?;
        unknown["clean"] = serde_json::json!(true);
        manifest.provider_state[0].artifact = blob(&store, &serde_json::to_vec(&unknown)?)?;
        assert!(ProtectedNativeGuestIdentity::from_checkpoint(&store, &manifest, spec).is_err());
        Ok(())
    }
}
