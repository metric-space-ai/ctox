// Origin: CTOX
// License: AGPL-3.0-only

//! Private durable input from the actual retired native producer.
//! A journal receipt is neither a complete checkpoint nor a transfer/resume grant.
use super::*;
use ctox_protocol::portable_journal::{
    artifact_ref_for, validate_portable_journal, PortableJournalExpectation, PortableJournalFormat,
    PortableJournalLimits,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSourceJournalReceipt {
    pub capture_id: String,
    pub job_id: String,
    pub session_id: String,
    pub journal_sha256: String,
    pub journal_size_bytes: u64,
}

pub(super) fn source_store(
    parent: &Path,
) -> Result<(
    ctox_sync::checkpoint::CheckpointStore,
    PathBuf,
    FileIdentity,
)> {
    private_directory(parent)?;
    ensure!(
        std::fs::canonicalize(parent)? == parent,
        "native source parent is not canonical"
    );
    let root = parent.join("source-journals");
    for path in [&root, &root.join("blobs"), &root.join("manifests")] {
        use std::os::unix::fs::DirBuilderExt;
        match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        private_directory(path)?;
    }
    let identity = private_directory(&root)?;
    let store = ctox_sync::checkpoint::CheckpointStore::open(
        root.clone(),
        PortableJournalLimits::default().max_bytes,
    )?;
    ensure!(
        private_directory(&root)? == identity,
        "native source artifact store changed; reconcile"
    );
    Ok((store, root, identity))
}

pub(super) fn persist(
    policy: &Connection,
    store: &ctox_sync::checkpoint::CheckpointStore,
    store_root: &Path,
    destination: &GuestRestoreDestination,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    policy_revision: &str,
    bytes: &[u8],
) -> Result<NativeSourceJournalReceipt> {
    ensure!(
        spec.harness == ctox_core::native_harness_name()
            && spec.harness_version == ctox_core::native_harness_version()
            && spec.model_route_id == "openai",
        "unsupported native source journal producer"
    );
    let artifact = artifact_ref_for(bytes);
    let expected = PortableJournalExpectation {
        format: PortableJournalFormat::current(),
        session_id: ctox_protocol::ThreadId::from_string(&spec.session_id)
            .context("native source journal session is invalid")?,
    };
    let validated = validate_portable_journal(
        bytes,
        &artifact,
        &expected,
        &PortableJournalLimits::default(),
    )?;
    let stored_artifact = ctox_sync::contracts::ArtifactRef {
        sha256: artifact.sha256.clone(),
        size_bytes: artifact.size_bytes,
    };
    store.ingest_blob(&stored_artifact, std::io::Cursor::new(bytes))?;
    let store_path = store_root
        .to_str()
        .context("native source artifact path is not UTF-8")?;
    let spec_json = serde_json::to_string(spec)?;
    let ownership_json = serde_json::to_string(ownership)?;
    let capture_id = format!("capture_{}", uuid::Uuid::new_v4());
    policy.execute(
        "INSERT INTO business_native_source_journals
         (capture_id,guest_id,controller_id,controller_generation,owner_user_id,
          worker_profile_id,project_id,thread_id,source_instance_id,job_id,session_id,
          ownership_generation,spec_json,ownership_json,policy_revision,
          journal_format,journal_version,journal_sha256,journal_size_bytes,
          journal_record_count,artifact_store_path,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)
         ON CONFLICT(job_id,session_id,ownership_generation) DO NOTHING",
        rusqlite::params![
            capture_id,
            destination.guest_id,
            destination.controller_id,
            i64::try_from(destination.controller_generation)?,
            destination.human_owner_id,
            destination.worker_profile_id,
            destination.project_id,
            destination.thread_id,
            destination.instance_id,
            spec.job_id,
            spec.session_id,
            i64::try_from(ownership.generation)?,
            spec_json,
            ownership_json,
            policy_revision,
            validated.format,
            i64::from(validated.format_version),
            artifact.sha256,
            i64::try_from(artifact.size_bytes)?,
            i64::try_from(validated.record_count)?,
            store_path,
            super::super::store::now_ms() as i64,
        ],
    )?;
    // Repeated publication may recover the same receipt, never overwrite an
    // earlier journal or silently bind it to another controller/actor/policy.
    let stored: (String, String, i64) = policy
        .query_row(
            "SELECT capture_id,journal_sha256,journal_size_bytes
         FROM business_native_source_journals
         WHERE job_id=?1 AND session_id=?2 AND ownership_generation=?3
         AND guest_id=?4 AND controller_id=?5 AND controller_generation=?6
         AND owner_user_id=?7 AND worker_profile_id=?8 AND project_id=?9 AND thread_id=?10
         AND source_instance_id=?11 AND spec_json=?12 AND ownership_json=?13
         AND policy_revision=?14 AND journal_sha256=?15 AND journal_size_bytes=?16
         AND journal_record_count=?17 AND artifact_store_path=?18",
            rusqlite::params![
                spec.job_id,
                spec.session_id,
                i64::try_from(ownership.generation)?,
                destination.guest_id,
                destination.controller_id,
                i64::try_from(destination.controller_generation)?,
                destination.human_owner_id,
                destination.worker_profile_id,
                destination.project_id,
                destination.thread_id,
                destination.instance_id,
                spec_json,
                ownership_json,
                policy_revision,
                artifact.sha256,
                i64::try_from(artifact.size_bytes)?,
                i64::try_from(validated.record_count)?,
                store_path,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("native source journal publication conflicts; reconcile")?;
    Ok(NativeSourceJournalReceipt {
        capture_id: stored.0,
        job_id: spec.job_id.clone(),
        session_id: spec.session_id.clone(),
        journal_sha256: stored.1,
        journal_size_bytes: u64::try_from(stored.2)?,
    })
}

impl NativeGuestExecution {
    /// Only an actual Core journal reader and the exact retired producer can
    /// enter this worker -> account -> policy -> controller publication path.
    pub(crate) fn persist_source_journal(
        &self,
        source: &crate::channels::NativeProviderCaptureOwner,
        journal: &ctox_core::NativeJournalReader,
    ) -> Result<NativeSourceJournalReceipt> {
        ensure!(
            source.matches_provider(&self.provider),
            "foreign native capture owner"
        );
        source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, policy| {
                verify()?;
                let bytes = journal.read_bytes(PortableJournalLimits::default().max_bytes)?;
                let (store, store_root, store_identity) =
                    source_store(&entry.assignment.destination.import_parent)?;
                let receipt = persist(
                    policy,
                    &store,
                    &store_root,
                    &entry.assignment.destination,
                    &self.binding.spec,
                    &self.binding.ownership,
                    &self.binding.admission.policy_revision,
                    &bytes,
                )?;
                ensure!(
                    private_directory(&store_root)? == store_identity,
                    "native source artifact store changed; reconcile"
                );
                Ok(receipt)
            })
        })
    }
}
