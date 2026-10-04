//! Authorized transfer contents feed the canonical checkpoint importer, never a
//! caller-selected guest path or an execution/readiness permission.
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    authority::{client::ExecutionAuthority, Ownership},
    checkpoint::{validate_manifest, CheckpointStore},
    contracts::{ArtifactRef, CheckpointManifest},
    guest_restore::{stage_guest_restore, GuestRestoreOwner, StagedGuestRestore},
};
use ctox_transfers::{DownloadRequest, PeerRangeSource, Store};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// Native call inputs, not a renderer/wire DTO. Transfer IDs select immutable
/// saved jobs. The protected digest and ownership come from execution authority.
pub(crate) struct GuestCheckpointTransfer<'a> {
    pub guest_id: &'a str,
    pub execution_job_id: &'a str,
    pub ownership: Ownership,
    pub checkpoint_digest: &'a str,
    pub manifest_transfer_id: &'a str,
    /// Each distinct manifest artifact SHA-256 maps to its original native job.
    pub artifact_transfer_ids: &'a BTreeMap<String, String>,
}

fn same_source_account(original: &DownloadRequest, artifact: &DownloadRequest) -> Result<()> {
    let a = original
        .peer_source
        .as_ref()
        .context("native manifest source required")?;
    let b = artifact
        .peer_source
        .as_ref()
        .context("native artifact source required")?;
    let ab = a
        .account_binding
        .as_ref()
        .context("original manifest account required")?;
    let bb = b
        .account_binding
        .as_ref()
        .context("original artifact account required")?;
    ab.validate()?;
    bb.validate()?;
    ensure!(
        a.instance_id == b.instance_id
            && a.public_key == b.public_key
            && a.collection == "desktop_files"
            && b.collection == "desktop_files"
            && ab.target_id == bb.target_id
            && ab.account_epoch == bb.account_epoch
            && ab.principal_sha256 == bb.principal_sha256,
        "checkpoint artifacts changed source or account"
    );
    // Each file retains its own original grant, not the manifest file's grant.
    Ok(())
}

fn required_artifacts(manifest: &CheckpointManifest) -> Result<BTreeMap<String, ArtifactRef>> {
    let all = manifest
        .history
        .iter()
        .chain(&manifest.attachments)
        .chain(std::iter::once(&manifest.workspace_state.index_patch))
        .chain(std::iter::once(&manifest.workspace_state.worktree_patch))
        .chain(manifest.workspace.iter().map(|entry| &entry.artifact))
        .chain(
            manifest
                .workspace_state
                .required_untracked
                .iter()
                .map(|entry| &entry.artifact),
        )
        .chain(manifest.provider_state.iter().map(|entry| &entry.artifact));
    let mut unique: BTreeMap<String, ArtifactRef> = BTreeMap::new();
    for artifact in all {
        if let Some(previous) = unique.insert(artifact.sha256.clone(), artifact.clone()) {
            ensure!(
                previous.size_bytes == artifact.size_bytes,
                "conflicting checkpoint artifact lengths"
            );
        }
    }
    Ok(unique)
}

async fn ingest_transferred_checkpoint(
    transfers: &Store,
    checkpoints: &CheckpointStore,
    peer: &dyn PeerRangeSource,
    request: &GuestCheckpointTransfer<'_>,
) -> Result<Vec<DownloadRequest>> {
    let original = transfers.get(request.manifest_transfer_id)?.request;
    same_source_account(&original, &original)?;
    ensure!(
        original.sha256 == request.checkpoint_digest && original.size <= MAX_MANIFEST_BYTES,
        "checkpoint manifest differs from protected digest or exceeds budget"
    );
    peer.authorize(&original).await?;
    let manifest = transfers.read_completed_peer_artifact(&original, |input| {
        let mut bytes = Vec::new();
        input.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == original.size,
            "checkpoint manifest length changed"
        );
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == request.checkpoint_digest,
            "checkpoint manifest content changed"
        );
        let manifest: CheckpointManifest = serde_json::from_slice(&bytes)?;
        validate_manifest(&manifest)?;
        ensure!(
            manifest.pending_effects.is_empty(),
            "checkpoint requires effect reconciliation"
        );
        ensure!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(&manifest)?))
                == request.checkpoint_digest,
            "checkpoint manifest must retain its canonical digest"
        );
        Ok(manifest)
    })?;
    peer.authorize(&original).await?;
    let required = required_artifacts(&manifest)?;
    ensure!(
        required.len() == request.artifact_transfer_ids.len()
            && required.keys().eq(request.artifact_transfer_ids.keys()),
        "checkpoint artifact jobs do not match the manifest"
    );
    let mut originals = vec![original.clone()];
    for (digest, artifact) in required {
        let id = request
            .artifact_transfer_ids
            .get(&digest)
            .context("missing checkpoint transfer")?;
        let saved = transfers.get(id)?.request;
        same_source_account(&original, &saved)?;
        ensure!(
            saved.sha256 == artifact.sha256 && saved.size == artifact.size_bytes,
            "transfer differs from checkpoint artifact"
        );
        peer.authorize(&saved).await?;
        transfers.read_completed_peer_artifact(&saved, |input| {
            // The CAS independently streams, rehashes and durably publishes the
            // exact bytes read. A completed transfer receipt is not sufficient.
            checkpoints.ingest_blob(&artifact, input)?;
            Ok(())
        })?;
        peer.authorize(&saved).await?;
        originals.push(saved);
    }
    for saved in &originals {
        ensure!(
            transfers.get(&saved.id)?.request == *saved,
            "checkpoint job binding changed"
        );
        peer.authorize(saved).await?;
    }
    ensure!(
        checkpoints.publish(&manifest)? == request.checkpoint_digest,
        "published checkpoint digest changed"
    );
    Ok(originals)
}

/// Run on the native lifecycle owner's supervised blocking-I/O boundary, as
/// required by CheckpointStore and guest_restore. No detached task is spawned.
/// This stages only; VM/Crew must separately commit under the real live fences
/// and observe their retained guest process before declaring readiness. The stage
/// is immutable; Git/provider reconstruction uses a separate native-owned path
/// after verified import. Returning this value grants no authority for later use.
pub(crate) async fn stage_transferred_guest_checkpoint(
    transfers: &Store,
    checkpoints: &CheckpointStore,
    peer: &dyn PeerRangeSource,
    authority: &dyn ExecutionAuthority,
    owner: &dyn GuestRestoreOwner,
    request: GuestCheckpointTransfer<'_>,
) -> Result<StagedGuestRestore> {
    let job = authority
        .validate_ownership(request.execution_job_id, &request.ownership)
        .await?;
    ensure!(
        job.spec.job_id == request.execution_job_id
            && job.spec.scope_id == authority.scope_id()
            && job.ownership == request.ownership
            && job.ownership.node_id == authority.node_id()
            && !job.stopped
            && job.pending_effects.is_empty()
            && job
                .checkpoint
                .as_ref()
                .is_some_and(|checkpoint| checkpoint.digest == request.checkpoint_digest),
        "checkpoint is not protected for the current native execution owner"
    );
    let originals = ingest_transferred_checkpoint(transfers, checkpoints, peer, &request).await?;
    let staged = stage_guest_restore(
        checkpoints,
        authority,
        owner,
        request.guest_id,
        request.execution_job_id,
        request.ownership,
        request.checkpoint_digest,
    )
    .await?;
    // Grant/account changes during staging cannot return a usable stage. Drop
    // removes only the unpublished staging tree; cached bytes grant no execution.
    for saved in &originals {
        ensure!(
            transfers.get(&saved.id)?.request == *saved,
            "checkpoint job binding changed"
        );
        peer.authorize(saved).await?;
    }
    Ok(staged)
}

#[cfg(test)]
#[path = "transfers_checkpoint_tests.rs"]
mod tests;
