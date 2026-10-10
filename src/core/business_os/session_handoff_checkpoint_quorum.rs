// Origin: CTOX
// License: AGPL-3.0-only
//! Native durable-copy acknowledgement and source quorum protection.
//! Public signed metadata only; neither operation resumes an executor.
use super::*;
use anyhow::Context;
use ctox_sync::authority::{client::ExecutionAuthority, Command, Receipt, Request};
use ctox_sync::contracts::CheckpointCopyReceipt;

type Registry = super::super::super::super::NativeGuestRegistry;

pub(super) async fn acknowledge<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<Registry>,
    binding: String,
) -> anyhow::Result<CopyResponse> {
    let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
    let live = lifetime.0.clone();
    let target = tokio::task::spawn_blocking(move || {
        let target = Target::prepare(server, &binding, None, live, SessionHandoffPhase::Resume)?;
        let authority = registry.checkpoint_authority(&target.server.gate.root)?;
        anyhow::ensure!(
            authority.scope_id() == target.request.spec.scope_id,
            "foreign checkpoint authority"
        );
        target.current(&target.request, |_, identity| {
            registry.checkpoint_authority(&target.server.gate.root)?;
            let path = target
                .server
                .gate
                .root
                .join("runtime/ctox-sync/received-checkpoints");
            private_dir(&path)?;
            let store = CheckpointStore::open(path, BLOB_LIMIT)?;
            let receipt = identity.acknowledge_checkpoint(
                &store,
                authority.node_id(),
                &target.request.spec,
                &target.request.ownership,
                &target.request.checkpoint_digest,
            )?;
            anyhow::ensure!(
                receipt.sequence == target.request.checkpoint_sequence,
                "checkpoint sequence changed"
            );
            Ok(CopyResponse::CopyAcknowledged { receipt })
        })
    })
    .await??;
    Ok(target)
}

struct Source<P> {
    server: Arc<Server<P>>,
    registry: Arc<Registry>,
    authority: Arc<dyn ExecutionAuthority>,
    request: SessionHandoffGateRequest,
    original: SessionHandoffPermit,
    auth: Arc<ctox_core::AuthManager>,
    live: Arc<Mutex<bool>>,
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> Source<P> {
    fn current<T>(
        &self,
        action: impl FnOnce(&Connection, &SigningIdentity, &SessionHandoffPermit) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let _account = self.auth.current_runtime_account_guard()?;
        self.registry.checkpoint_authority(&self.server.gate.root)?;
        let mut result = None;
        self.server
            .gate
            .with_current_authority(|policy, identity| {
                let permit = self
                    .server
                    .gate
                    .resolve_fenced(policy, identity, &self.request)?;
                if !operation_authority_matches(&self.original, &permit) {
                    return Err(deny("source_authority_changed"));
                }
                let ledger = self.server.lock_ledger()?;
                if !ledger.alive {
                    return Err(deny("host_retired"));
                }
                let live = self.live.lock().map_err(|_| deny("protection_retired"))?;
                if !*live {
                    return Err(deny("protection_retired"));
                }
                result = Some(action(policy, identity, &permit));
                Ok(())
            })?;
        result.context("source protection did not publish")?
    }
}
fn validate_receipts(receipts: &[CheckpointCopyReceipt]) -> anyhow::Result<&CheckpointCopyReceipt> {
    anyhow::ensure!(
        !receipts.is_empty() && receipts.len() <= 8,
        "invalid checkpoint receipt count"
    );
    let first = &receipts[0];
    let mut nodes = std::collections::BTreeSet::new();
    for receipt in receipts {
        anyhow::ensure!(
            receipt.version == 1
                && receipt.node_id != 0
                && nodes.insert(receipt.node_id)
                && receipt.spec == first.spec
                && receipt.ownership == first.ownership
                && receipt.checkpoint_digest == first.checkpoint_digest
                && receipt.sequence == first.sequence,
            "checkpoint receipts differ or repeat a node"
        );
    }
    Ok(first)
}
pub(super) async fn protect<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    registry: Arc<Registry>,
    binding: String,
    mut receipts: Vec<CheckpointCopyReceipt>,
) -> anyhow::Result<CopyResponse> {
    let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
    let live = lifetime.0.clone();
    let (source, receipts_json, disclosure) = tokio::task::spawn_blocking(move || {
        let first = validate_receipts(&receipts)?;
        let authority = registry.checkpoint_authority(&server.gate.root)?;
        anyhow::ensure!(
            authority.node_id() == first.ownership.node_id
                && authority.scope_id() == first.spec.scope_id
                && receipts.iter().all(|r| r.node_id != authority.node_id()),
            "source protection requires the actual owner and independent copy receipts"
        );
        let request = SessionHandoffGateRequest {
            binding_digest: binding, phase: SessionHandoffPhase::Disclose,
            spec: first.spec.clone(), ownership: first.ownership.clone(),
            checkpoint_digest: first.checkpoint_digest.clone(), checkpoint_sequence: first.sequence,
            issuer_identity: server.gate.issuer_identity.clone(),
            audience: server.scope.clone(), nonce: fresh_nonce()?,
        };
        let auth = account(&server.gate.root, &request)?;
        let original = server.gate.authorize(&request)?;
        let source = Arc::new(Source { server, registry, authority, request, original, auth, live });
        let (encoded, disclosure) = source.current(|policy, identity, permit| {
            let store = source_store(&source.server.gate.root, policy, &source.request)?;
            let own = identity.acknowledge_checkpoint(
                &store, source.authority.node_id(), &source.request.spec,
                &source.request.ownership, &source.request.checkpoint_digest,
            )?;
            anyhow::ensure!(own.sequence == source.request.checkpoint_sequence, "source checkpoint changed");
            receipts.push(own);
            let encoded = serde_json::to_string(&receipts)?;
            anyhow::ensure!(encoded.len() <= 30 * 1024, "checkpoint receipts exceed control bound");
            // Commit the exact operation before the quorum await. Pending or
            // lost responses require reconciliation; a second CLI call cannot
            // resubmit another request for this binding/checkpoint generation.
            policy.execute_batch(
                "CREATE TABLE IF NOT EXISTS business_native_checkpoint_protections (
                 binding_digest TEXT NOT NULL, checkpoint_digest TEXT NOT NULL,
                 ownership_generation INTEGER NOT NULL, request_id TEXT NOT NULL UNIQUE,
                 receipts_json TEXT NOT NULL, principal_epoch INTEGER NOT NULL,
                 binding_revision INTEGER NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('Pending','Protected')),
                 PRIMARY KEY(binding_digest,checkpoint_digest,ownership_generation))"
            )?;
            policy.execute(
                "INSERT INTO business_native_checkpoint_protections
                 (binding_digest,checkpoint_digest,ownership_generation,request_id,receipts_json,
                  principal_epoch,binding_revision,phase) VALUES (?1,?2,?3,?4,?5,?6,?7,'Pending')",
                rusqlite::params![source.request.binding_digest, source.request.checkpoint_digest,
                    i64::try_from(source.request.ownership.generation)?, source.request.nonce,
                    encoded, i64::try_from(permit.principal_epoch)?, i64::try_from(permit.binding_revision)?],
            )?;
            Ok((encoded, permit.clone()))
        })?;
        Ok::<_, anyhow::Error>((source, encoded, disclosure))
    }).await??;
    let receipts: Vec<CheckpointCopyReceipt> = serde_json::from_str(&receipts_json)?;
    let receipt = source
        .authority
        .submit(Request {
            request_id: source.request.nonce.clone(),
            actor: source.authority.node_id(),
            command: Command::ProtectCheckpoint {
                job_id: source.request.spec.job_id.clone(),
                ownership: source.request.ownership.clone(),
                receipts,
                disclosure,
            },
        })
        .await?;
    let applied = match receipt {
        Receipt::Applied(job) => job,
        _ => {
            anyhow::bail!("checkpoint protection did not produce a fresh quorum receipt; reconcile")
        }
    };
    let current = source
        .authority
        .validate_ownership(&source.request.spec.job_id, &source.request.ownership)
        .await?;
    anyhow::ensure!(
        current == applied
            && current.spec == source.request.spec
            && current.ownership == source.request.ownership
            && !current.stopped
            && current.pending_effects.is_empty()
            && !current.checkpoint_requires_refresh
            && current
                .checkpoint
                .as_ref()
                .is_some_and(|c| c.digest == source.request.checkpoint_digest
                    && c.sequence == source.request.checkpoint_sequence),
        "quorum checkpoint changed before publication"
    );
    tokio::task::spawn_blocking(move || {
        source.current(|policy, _, permit| {
            let changed = policy.execute(
                "UPDATE business_native_checkpoint_protections SET phase='Protected'
                 WHERE binding_digest=?1 AND checkpoint_digest=?2 AND ownership_generation=?3
                 AND request_id=?4 AND receipts_json=?5 AND principal_epoch=?6 AND binding_revision=?7 AND phase='Pending'",
                rusqlite::params![source.request.binding_digest, source.request.checkpoint_digest,
                    i64::try_from(source.request.ownership.generation)?, source.request.nonce,
                    receipts_json, i64::try_from(permit.principal_epoch)?, i64::try_from(permit.binding_revision)?],
            )?;
            anyhow::ensure!(changed == 1, "pending protection changed; reconcile");
            Ok(CopyResponse::CheckpointProtected {
                checkpoint_digest: source.request.checkpoint_digest.clone(),
                sequence: source.request.checkpoint_sequence,
                ownership_generation: source.request.ownership.generation,
            })
        })
    }).await?
}

#[cfg(test)]
pub(super) fn assert_dirty_copy_cannot_acknowledge<P: Clone + Eq + Hash + Send + Sync + 'static>(
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
    let mut signed = false;
    let result = target.current(&target.request, |_, identity| {
        // This is the actual protected native capture from the service fixture.
        // Its original unknown-effect marker must reject every signed DATA
        // acknowledgement; no clean checkpoint is manufactured for this test.
        let receipt = identity.acknowledge_checkpoint(
            store,
            target.request.ownership.node_id + 1,
            &target.request.spec,
            &target.request.ownership,
            &target.request.checkpoint_digest,
        )?;
        signed = true;
        Ok(receipt)
    });
    assert!(result.is_err());
    assert!(!signed);
    assert!(!store
        .load(&target.request.checkpoint_digest)
        .unwrap()
        .pending_effects
        .is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn copy(node: u64) -> CheckpointCopyReceipt {
        CheckpointCopyReceipt {
            version: 1,
            node_id: node,
            spec: ctox_sync::contracts::ExecutionSpec {
                job_id: "test-job".into(),
                session_id: "d7815459-b5dd-42e5-8077-ece996330c4e".into(),
                scope_id: "test-scope".into(),
                harness: "ctox-core".into(),
                harness_version: "test".into(),
                model_route_id: "openai".into(),
                gateway_account_id: "test-account".into(),
                model_id: "test-model".into(),
                required_capabilities: std::collections::BTreeSet::from(["desktop".into()]),
            },
            ownership: ctox_sync::authority::Ownership {
                node_id: 1,
                generation: 3,
            },
            checkpoint_digest: "a".repeat(64),
            sequence: 7,
            signature: "b".repeat(128),
        }
    }
    #[test]
    fn native_checkpoint_quorum_receipt_set_rejects_mixed_or_duplicate_evidence() {
        assert!(validate_receipts(&[]).is_err());
        assert!(validate_receipts(&vec![copy(2); 9]).is_err());
        assert!(validate_receipts(&[copy(2), copy(2)]).is_err());
        for field in 0..7 {
            let mut changed = copy(3);
            match field {
                0 => changed.node_id = 0,
                1 => changed.version = 2,
                2 => changed.spec.job_id = "foreign-job".into(),
                3 => changed.spec.gateway_account_id = "foreign-account".into(),
                4 => changed.ownership.generation += 1,
                5 => changed.checkpoint_digest = "c".repeat(64),
                _ => changed.sequence += 1,
            }
            assert!(validate_receipts(&[copy(2), changed]).is_err());
        }
        // Structural agreement is not signature authority. Only the existing
        // committed quorum validates member roles and actual signatures.
        assert!(validate_receipts(&[copy(2), copy(3)]).is_ok());
    }
    #[test]
    fn native_checkpoint_quorum_control_rejects_mixed_operations_and_accepts_legacy_copy() {
        let original = serde_json::json!({"bindingDigest":"a".repeat(64), "enrollGuest":true});
        let request: CopyRequest = serde_json::from_value(original.clone()).unwrap();
        assert!(request.valid_operation());
        for (field, value) in [
            ("takeOver", serde_json::json!(true)),
            ("acknowledge", serde_json::json!(true)),
            ("reconstruct", serde_json::json!(true)),
            ("guestId", serde_json::json!("supplied-guest")),
            ("sourceRoute", serde_json::json!("supplied-route")),
            ("protectionReceipts", serde_json::json!([copy(2)])),
        ] {
            let mut mixed = original.clone();
            mixed[field] = value;
            let request: CopyRequest = serde_json::from_value(mixed).unwrap();
            assert!(!request.valid_operation(), "{field}");
        }
        let legacy = serde_json::json!({"bindingDigest":"a".repeat(64),"sourceRoute":"peer"});
        let mut request: CopyRequest = serde_json::from_value(legacy).unwrap();
        assert!(request.valid_operation());
        assert_eq!(request.operation_timeout().as_secs(), 60);
        request.acknowledge = true;
        assert!(!request.valid_operation());
        request.source_route.clear();
        assert!(request.valid_operation());
        assert_eq!(request.operation_timeout().as_secs(), 60);
        request.reconstruct = true;
        assert!(!request.valid_operation());
        request.reconstruct = false;
        request.guest_id = "test-guest".into();
        assert!(!request.valid_operation());
        request.guest_id.clear();
        request.protection_receipts = vec![copy(2)];
        assert!(!request.valid_operation());
        request.acknowledge = false;
        assert!(request.valid_operation());
        request.source_route = "peer".into();
        assert!(!request.valid_operation());
        request.source_route.clear();
        request.protection_receipts = vec![copy(2); 9];
        assert!(!request.valid_operation());
        request.protection_receipts.clear();
        assert!(!request.valid_operation());
        request.take_over = true;
        assert!(request.valid_operation());
        assert_eq!(request.operation_timeout().as_secs(), 60);
        request.acknowledge = true;
        assert!(!request.valid_operation());
        request.acknowledge = false;
        request.protection_receipts = vec![copy(2)];
        assert!(!request.valid_operation());
        request.protection_receipts.clear();
        request.reconstruct = true;
        assert!(!request.valid_operation());
        request.reconstruct = false;
        request.guest_id = "foreign-guest".into();
        assert!(!request.valid_operation());
        request.guest_id.clear();
        request.source_route = "foreign-route".into();
        assert!(!request.valid_operation());
    }
}
