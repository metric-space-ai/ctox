//! Native portable-state import. Materialization is not permission to run a guest.
#[cfg(test)]
#[path = "guest_restore_tests.rs"]
mod tests;
use crate::{
    authority::{client::ExecutionAuthority, Command, Job, Ownership, Receipt, Request},
    checkpoint::CheckpointStore,
    contracts::{CheckpointManifest, ExecutionSpec},
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Resolved by the native lifecycle owner, never taken from a payload path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestRestoreDestination {
    pub instance_id: String,
    pub guest_id: String,
    pub human_owner_id: String,
    pub project_id: String,
    pub thread_id: String,
    pub worker_profile_id: String,
    pub controller_id: String,
    pub controller_generation: u64,
    pub import_parent: PathBuf,
}

/// Both authorities are required: quorum execution ownership does not grant
/// access to a human's guest. There is deliberately no permissive implementation.
pub trait GuestRestoreOwner: Send + Sync {
    fn resolve_destination(
        &self,
        guest_id: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination>;

    /// Invoke publication synchronously while holding the REAL controller and
    /// live execution fence. Every revoke/takeover/shutdown path must serialize
    /// with that same guard. A preceding boolean check is insufficient.
    /// The owner must reject changed destination, principal, policy, controller,
    /// process lifetime or execution attempt before invoking the callback.
    fn with_current_fence(
        &self,
        expected: &GuestRestoreDestination,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()>;
}

/// Local native receipt, not a wire contract or execution/readiness capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestImportReceipt {
    pub destination: GuestRestoreDestination,
    pub spec: ExecutionSpec,
    pub ownership: Ownership,
    pub checkpoint_digest: String,
    pub sequence: u64,
    pub imported_directory: PathBuf,
    pub effect_id: String,
}

/// Unpublished private state. Dropping it removes only this call's staging tree.
pub struct StagedGuestRestore {
    destination: GuestRestoreDestination,
    spec: ExecutionSpec,
    ownership: Ownership,
    digest: String,
    manifest: CheckpointManifest,
    staging: tempfile::TempDir,
}
impl StagedGuestRestore {
    /// Inspection only: these checkpoint bytes remain immutable.
    /// Reconstruct runtime state in a separate native-owned path after import.
    /// This directory is never exposed as a ready guest or execution workspace.
    pub fn staged_directory(&self) -> PathBuf {
        self.staging.path().join("state")
    }
    pub fn manifest(&self) -> &CheckpointManifest {
        &self.manifest
    }
    pub fn destination(&self) -> &GuestRestoreDestination {
        &self.destination
    }
}

fn denied(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn validate_destination(destination: &GuestRestoreDestination, guest_id: &str) -> io::Result<()> {
    if destination.guest_id != guest_id
        || [
            &destination.instance_id,
            &destination.guest_id,
            &destination.human_owner_id,
            &destination.project_id,
            &destination.thread_id,
            &destination.worker_profile_id,
            &destination.controller_id,
        ]
        .iter()
        .any(|id| !valid_id(id))
        || destination.controller_generation == 0
        || !destination.import_parent.is_absolute()
    {
        return Err(denied("incomplete native guest destination"));
    }
    let meta = fs::symlink_metadata(&destination.import_parent)?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || fs::canonicalize(&destination.import_parent)? != destination.import_parent
    {
        return Err(denied(
            "guest import parent must be a canonical native directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err(denied(
                "guest import parent must be private and owned by this user",
            ));
        }
    }
    Ok(())
}

fn validate_job(
    job: &Job,
    authority: &dyn ExecutionAuthority,
    job_id: &str,
    ownership: &Ownership,
    digest: &str,
) -> io::Result<()> {
    if job.spec.job_id != job_id
        || job.spec.scope_id != authority.scope_id()
        || job.ownership != *ownership
        || ownership.node_id != authority.node_id()
        || job.stopped
        || !job.pending_effects.is_empty()
        || job.checkpoint.as_ref().is_none_or(|c| c.digest != digest)
    {
        return Err(denied(
            "guest restore requires current protected execution state",
        ));
    }
    Ok(())
}

fn matches_manifest(job: &Job, manifest: &CheckpointManifest) -> bool {
    let session = &manifest.session;
    session.scope_id == job.spec.scope_id
        && session.session_id == job.spec.session_id
        && session.harness == job.spec.harness
        && session.harness_version == job.spec.harness_version
        && session.model_route_id == job.spec.model_route_id
        && session.gateway_account_id == job.spec.gateway_account_id
        && session.model_id == job.spec.model_id
        && session.required_capabilities == job.spec.required_capabilities
        && job
            .checkpoint
            .as_ref()
            .is_some_and(|c| c.sequence == manifest.sequence)
        && manifest.pending_effects.is_empty()
}

/// Stage only a protected checkpoint for the current local executor. Source
/// grants and the handoff-disclosure gate still authorize obtaining its bytes.
pub async fn stage_guest_restore(
    store: &CheckpointStore,
    authority: &dyn ExecutionAuthority,
    owner: &dyn GuestRestoreOwner,
    guest_id: &str,
    job_id: &str,
    ownership: Ownership,
    digest: &str,
) -> io::Result<StagedGuestRestore> {
    if !cfg!(unix) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "guest import requires certified directory durability",
        ));
    }
    if !valid_id(guest_id) || !valid_id(job_id) {
        return Err(denied("invalid guest restore identity"));
    }
    let job = authority.validate_ownership(job_id, &ownership).await?;
    validate_job(&job, authority, job_id, &ownership, digest)?;
    let destination = owner.resolve_destination(guest_id, &job.spec, &ownership)?;
    validate_destination(&destination, guest_id)?;
    // The CAS store revalidates every artifact before restoration.
    let manifest = store.load(digest)?;
    if !matches_manifest(&job, &manifest) {
        return Err(denied(
            "checkpoint does not match the native execution contract",
        ));
    }
    let staging = tempfile::Builder::new()
        .prefix(".guest-import-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&destination.import_parent)?;
    store.restore(digest, &staging.path().join("state"))?;
    sync_tree(&staging.path().join("state"))?;
    let current = authority.validate_ownership(job_id, &ownership).await?;
    validate_job(&current, authority, job_id, &ownership, digest)?;
    if current.spec != job.spec
        || !matches_manifest(&current, &manifest)
        || owner.resolve_destination(guest_id, &job.spec, &ownership)? != destination
    {
        return Err(denied("guest destination changed while staging"));
    }
    Ok(StagedGuestRestore {
        destination,
        spec: job.spec,
        ownership,
        digest: digest.into(),
        manifest,
        staging,
    })
}

/// Commit the staged import under the native owner's live fence. The existing
/// BeginEffect ledger prevents ordinary takeover while publication is pending.
/// Cancellation/error after admission leaves that effect unresolved: never
/// retry it automatically or synthesize CompleteEffect from a file's presence.
pub async fn commit_guest_restore(
    authority: &dyn ExecutionAuthority,
    owner: &dyn GuestRestoreOwner,
    staged: StagedGuestRestore,
) -> io::Result<GuestImportReceipt> {
    let job = authority
        .validate_ownership(&staged.spec.job_id, &staged.ownership)
        .await?;
    validate_job(
        &job,
        authority,
        &staged.spec.job_id,
        &staged.ownership,
        &staged.digest,
    )?;
    if job.spec != staged.spec
        || owner.resolve_destination(
            &staged.destination.guest_id,
            &staged.spec,
            &staged.ownership,
        )? != staged.destination
    {
        return Err(denied("guest restore binding changed before admission"));
    }
    let effect_bytes = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}",
        staged.spec.job_id,
        staged.destination.instance_id,
        staged.destination.guest_id,
        staged.destination.controller_id,
        staged.destination.controller_generation,
        staged.ownership.generation,
        staged.digest
    );
    let effect_id = format!("guest-import:{:x}", Sha256::digest(effect_bytes.as_bytes()));
    let receipt = authority
        .submit(Request {
            request_id: format!("{effect_id}:begin"),
            actor: authority.node_id(),
            command: Command::BeginEffect {
                job_id: staged.spec.job_id.clone(),
                ownership: staged.ownership.clone(),
                effect_id: effect_id.clone(),
            },
        })
        .await?;
    match receipt {
        Receipt::Applied(ref admitted)
            if admitted.spec == staged.spec
                && admitted.ownership == staged.ownership
                && !admitted.stopped
                && admitted.pending_effects.len() == 1
                && admitted.pending_effects.contains(&effect_id) => {}
        // Replayed is evidence of a prior admission, never permission to repeat it.
        _ => return Err(denied("guest import effect was not freshly admitted")),
    }
    let target = staged
        .destination
        .import_parent
        .join(format!("import-{:x}", Sha256::digest(effect_id.as_bytes())));
    let mut invoked = false;
    let mut published = false;
    owner.with_current_fence(
        &staged.destination,
        &staged.spec,
        &staged.ownership,
        &mut || {
            if invoked {
                return Err(denied("guest import publication invoked twice"));
            }
            invoked = true;
            validate_destination(&staged.destination, &staged.destination.guest_id)?;
            verify_staged_tree(&staged)?;
            sync_tree(&staged.staged_directory())?;
            // Reserve a new name without replacing any existing user content.
            // The owner's cross-process fence owns this directory through rename.
            fs::create_dir(&target)?;
            if let Err(error) = fs::rename(staged.staged_directory(), &target) {
                let _ = fs::remove_dir(&target);
                return Err(error);
            }
            // Once rename occurred, preserve the target even on durability failure.
            // A recovery owner must reconcile it against the pending effect.
            fs::File::open(&staged.destination.import_parent)?.sync_all()?;
            published = true;
            Ok(())
        },
    )?;
    if !invoked || !published {
        return Err(denied("native owner did not publish the guest import"));
    }
    match authority
        .submit(Request {
            request_id: format!("{effect_id}:complete"),
            actor: authority.node_id(),
            command: Command::CompleteEffect {
                job_id: staged.spec.job_id.clone(),
                ownership: staged.ownership.clone(),
                effect_id: effect_id.clone(),
            },
        })
        .await?
    {
        Receipt::Applied(ref complete)
            if complete.spec == staged.spec
                && complete.ownership == staged.ownership
                && !complete.stopped
                && complete.completed_effects.contains(&effect_id) => {}
        _ => return Err(denied("guest import completion requires reconciliation")),
    }
    Ok(GuestImportReceipt {
        destination: staged.destination.clone(),
        spec: staged.spec.clone(),
        ownership: staged.ownership.clone(),
        checkpoint_digest: staged.digest.clone(),
        sequence: staged.manifest.sequence,
        imported_directory: target,
        effect_id,
    })
}

/// Native process/endpoint observation, supplied only by the guest lifecycle
/// owner after a real guest handshake; QMP "running" is not this evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestLiveEndpoint {
    pub process_instance_id: String,
    pub guest_session_id: String,
    pub endpoint_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestReadyReceipt {
    pub import: GuestImportReceipt,
    pub endpoint: GuestLiveEndpoint,
}

/// Implemented by the owner that actually retains the prepared guest process,
/// its endpoint/session and the current controller/execution fence.
pub trait GuestReadinessOwner: GuestRestoreOwner {
    /// Resolve the import against native registration, prove the actual live
    /// process and guest endpoint, and publish once while holding the SAME
    /// fences used by input, revoke, takeover, expiry and shutdown.
    /// Unknown old effects/stop outcomes and unregistered guests must be denied.
    fn with_live_guest(
        &self,
        imported: &GuestImportReceipt,
        publish: &mut dyn FnMut(GuestLiveEndpoint) -> io::Result<()>,
    ) -> io::Result<()>;
}

/// A current observation, not a reusable input permit or execution capability.
/// The VM owner must register and probe the real guest before this can succeed.
pub async fn confirm_guest_ready(
    authority: &dyn ExecutionAuthority,
    owner: &dyn GuestReadinessOwner,
    imported: GuestImportReceipt,
) -> io::Result<GuestReadyReceipt> {
    let job = authority
        .validate_ownership(&imported.spec.job_id, &imported.ownership)
        .await?;
    validate_job(
        &job,
        authority,
        &imported.spec.job_id,
        &imported.ownership,
        &imported.checkpoint_digest,
    )?;
    if job.spec != imported.spec
        || !job.completed_effects.contains(&imported.effect_id)
        || job
            .checkpoint
            .as_ref()
            .is_none_or(|c| c.sequence != imported.sequence)
        || owner.resolve_destination(
            &imported.destination.guest_id,
            &imported.spec,
            &imported.ownership,
        )? != imported.destination
    {
        return Err(denied(
            "guest readiness does not match the completed native import",
        ));
    }
    let mut endpoint = None;
    owner.with_live_guest(&imported, &mut |live| {
        if endpoint.is_some() {
            return Err(denied("guest readiness published twice"));
        }
        if [
            &live.process_instance_id,
            &live.guest_session_id,
            &live.endpoint_id,
        ]
        .iter()
        .any(|id| !valid_id(id))
        {
            return Err(denied(
                "guest readiness requires a live process and endpoint identity",
            ));
        }
        endpoint = Some(live);
        Ok(())
    })?;
    Ok(GuestReadyReceipt {
        import: imported,
        endpoint: endpoint.ok_or_else(|| denied("native guest readiness was not observed"))?,
    })
}

// Validate the immutable restored payload again at publication. Native runtime
// reconstruction belongs in a separate path, never in these checkpoint bytes.
fn verify_staged_tree(staged: &StagedGuestRestore) -> io::Result<()> {
    use crate::contracts::{ArtifactRef, WorkspaceEntryKind};
    use std::{
        collections::{BTreeMap, BTreeSet},
        io::Read,
        os::unix::fs::OpenOptionsExt,
    };
    struct ExpectedFile {
        artifact: ArtifactRef,
        symlink: bool,
        executable: Option<bool>,
    }
    let root = staged.staged_directory();
    let root_metadata = fs::symlink_metadata(&root)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(denied("staged root changed"));
    }
    let mut files = BTreeMap::<PathBuf, ExpectedFile>::new();
    let mut dirs = BTreeSet::from(
        ["workspace", "provider", "history", "attachments", "git"].map(|name| root.join(name)),
    );
    for (prefix, entries) in [
        ("workspace", &staged.manifest.workspace),
        (
            "workspace",
            &staged.manifest.workspace_state.required_untracked,
        ),
        ("provider", &staged.manifest.provider_state),
    ] {
        for entry in entries {
            files.insert(
                root.join(prefix).join(&entry.path),
                ExpectedFile {
                    artifact: entry.artifact.clone(),
                    symlink: entry.kind == WorkspaceEntryKind::Symlink,
                    executable: Some(entry.executable),
                },
            );
        }
    }
    for (prefix, artifacts) in [
        ("history", &staged.manifest.history),
        ("attachments", &staged.manifest.attachments),
    ] {
        for artifact in artifacts {
            files.insert(
                root.join(prefix).join(&artifact.sha256),
                ExpectedFile {
                    artifact: artifact.clone(),
                    symlink: false,
                    executable: None,
                },
            );
        }
    }
    for (name, artifact) in [
        ("index.patch", &staged.manifest.workspace_state.index_patch),
        (
            "worktree.patch",
            &staged.manifest.workspace_state.worktree_patch,
        ),
    ] {
        files.insert(
            root.join("git").join(name),
            ExpectedFile {
                artifact: artifact.clone(),
                symlink: false,
                executable: None,
            },
        );
    }
    let manifest_bytes = serde_json::to_vec(&staged.manifest).map_err(io::Error::other)?;
    let base_bytes = format!("{}\n", staged.manifest.workspace_state.base_commit);
    for (name, bytes) in [
        ("checkpoint.json", manifest_bytes.as_slice()),
        ("git/base-commit", base_bytes.as_bytes()),
    ] {
        files.insert(
            root.join(name),
            ExpectedFile {
                artifact: ArtifactRef {
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                    size_bytes: bytes.len() as u64,
                },
                symlink: false,
                executable: None,
            },
        );
    }
    for path in files.keys() {
        let mut parent = path.parent();
        while let Some(directory) = parent.filter(|directory| *directory != root) {
            dirs.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let mut pending = vec![root.clone()];
    let mut seen = BTreeSet::new();
    let mut seen_dirs = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                if !dirs.contains(&path) {
                    return Err(denied("unexpected staged directory"));
                }
                seen_dirs.insert(path.clone());
                pending.push(path);
                continue;
            }
            let expected = files
                .get(&path)
                .ok_or_else(|| denied("unexpected staged file"))?;
            let mut hash = Sha256::new();
            let size;
            if expected.symlink {
                if !metadata.file_type().is_symlink() {
                    return Err(denied("staged link changed"));
                }
                let text = fs::read_link(&path)?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| denied("staged link is not UTF-8"))?;
                hash.update(text.as_bytes());
                size = text.len() as u64;
            } else {
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(denied("staged file type changed"));
                }
                use std::os::unix::fs::MetadataExt;
                if expected
                    .executable
                    .is_some_and(|executable| (metadata.mode() & 0o111 != 0) != executable)
                {
                    return Err(denied("staged file executable state changed"));
                }
                let mut input = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)?;
                let mut count = 0u64;
                let mut buffer = [0u8; 65536];
                loop {
                    let n = input.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    count = count
                        .checked_add(n as u64)
                        .ok_or_else(|| denied("staged file overflow"))?;
                    if count > expected.artifact.size_bytes {
                        return Err(denied("staged file grew"));
                    }
                    hash.update(&buffer[..n]);
                }
                size = count;
            }
            if size != expected.artifact.size_bytes
                || format!("{:x}", hash.finalize()) != expected.artifact.sha256
            {
                return Err(denied("staged checkpoint content changed"));
            }
            seen.insert(path);
        }
    }
    if seen.len() != files.len() || seen_dirs != dirs {
        return Err(denied("staged checkpoint entry is missing"));
    }
    Ok(())
}

// Flush nested directory entries too; a top-level fsync alone is insufficient.
fn sync_tree(root: &Path) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            sync_tree(&entry.path())?;
        } else if kind.is_file() {
            fs::File::open(entry.path())?.sync_all()?;
        } else if !kind.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported staged entry",
            ));
        }
    }
    fs::File::open(root)?.sync_all()
}
