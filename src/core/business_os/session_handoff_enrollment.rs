// Origin: CTOX
// License: AGPL-3.0-only

//! Trusted local operator preparation. No renderer intake or handoff grants.
use super::guest_registry::source_handoff::{resolve_source_handoff, SourceHandoffFacts};
use super::store::{now_ms, open_store};
use anyhow::{ensure, Context, Result};
use ctox_sync::{
    authority::{auth::SigningIdentity, handoff::SessionHandoffGateRequest},
    host_config::HostConfiguration,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceHandoffEnrollment {
    pub capture_id: String,
    pub target_node_id: u64,
    /// Opaque target-local references explicitly selected by the operator.
    /// They are NOT filesystem paths, target credentials or target authority.
    pub target_instance_id: String,
    pub target_principal_user_id: String,
    pub repository_id: String,
    pub target_working_copy_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceHandoffEnrollmentReceipt {
    pub binding_id: String,
    pub binding_digest: String,
    pub checkpoint_digest: String,
    pub checkpoint_sequence: u64,
}

fn validate_input(input: &SourceHandoffEnrollment) -> Result<()> {
    let id = |s: &str| {
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
    };
    ensure!(
        id(&input.capture_id)
            && id(&input.target_instance_id)
            && id(&input.target_principal_user_id)
            && id(&input.repository_id)
            && id(&input.target_working_copy_id),
        "invalid native source handoff enrollment"
    );
    Ok(())
}

fn target_identity<'a>(
    config: &'a HostConfiguration,
    identity: &SigningIdentity,
    source: &SourceHandoffFacts,
    input: &SourceHandoffEnrollment,
) -> Result<&'a str> {
    validate_input(input)?;
    config.validate_key(identity)?;
    ensure!(
        source.spec.scope_id == config.scope_id
            && source.ownership.node_id == config.node_id()
            && input.target_node_id != config.node_id()
            && input.target_instance_id != source.source_instance_id,
        "handoff source or target does not match the enrolled host"
    );
    let target = config
        .voters
        .get(&input.target_node_id)
        .context("handoff target is not an enrolled peer")?;
    ensure!(
        target.executor && target.data_replica,
        "handoff target cannot receive and execute"
    );
    Ok(&target.identity)
}

fn digest(
    source: &SourceHandoffFacts,
    input: &SourceHandoffEnrollment,
    source_identity: &str,
    target_identity: &str,
) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            "ctox-native-source-handoff-v1",
            source,
            input,
            source_identity,
            target_identity
        ))?)
    ))
}

fn binding_matches(
    policy: &Connection,
    id: &str,
    hash: &str,
    source: &SourceHandoffFacts,
    input: &SourceHandoffEnrollment,
    source_identity: &str,
    target_identity: &str,
) -> Result<bool> {
    Ok(policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_session_handoff_bindings
        WHERE binding_id=?1 AND binding_digest=?2 AND revision=1 AND state='active' AND side='source'
        AND job_id=?3 AND session_id=?4 AND scope_id=?5 AND checkpoint_digest=?6
        AND checkpoint_sequence=?7 AND ownership_generation=?8 AND source_instance_id=?9
        AND source_identity=?10 AND source_actor_user_id=?11 AND target_instance_id=?12
        AND target_identity=?13 AND target_principal_user_id=?14 AND repository_id=?15
        AND target_working_copy_id=?16 AND provider=?17 AND gateway_account_id=?18
        AND model_route_id=?19 AND model_id=?20 AND required_capabilities_json=?21
        AND harness=?22 AND harness_version=?23 AND created_by=?11)",
        params![id,hash,source.spec.job_id,source.spec.session_id,source.spec.scope_id,
            source.checkpoint_digest,i64::try_from(source.checkpoint_sequence)?,
            i64::try_from(source.ownership.generation)?,source.source_instance_id,source_identity,
            source.owner_user_id,input.target_instance_id,target_identity,input.target_principal_user_id,
            input.repository_id,input.target_working_copy_id,source.spec.model_route_id,
            source.spec.gateway_account_id,source.spec.model_route_id,source.spec.model_id,
            serde_json::to_string(&source.spec.required_capabilities)?,
            source.spec.harness,source.spec.harness_version],
        |r| r.get(0),
    )?)
}

/// Caller holds the provisioned issuer and native host configuration fence.
pub(crate) fn enroll_source(
    root: &Path,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    input: &str,
) -> Result<SourceHandoffEnrollmentReceipt> {
    let input: SourceHandoffEnrollment = serde_json::from_str(input)?;
    validate_input(&input)?;
    let mut policy = open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = enroll_source_with_conn(root, &tx, config, identity, &input)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn enroll_source_with_conn(
    root: &Path,
    policy: &Connection,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    input: &SourceHandoffEnrollment,
) -> Result<SourceHandoffEnrollmentReceipt> {
    validate_input(input)?;
    let source = resolve_source_handoff(root, policy, &input.capture_id)?;
    let target = target_identity(config, identity, &source, input)?;
    let issuer = identity.public_identity();
    let hash = digest(&source, input, &issuer, target)?;
    let id = format!("handoff_{hash}");
    let now = i64::try_from(now_ms())?;
    policy.execute(
        "INSERT INTO business_session_handoff_bindings
        (binding_id,binding_digest,revision,state,side,job_id,session_id,scope_id,
        checkpoint_digest,checkpoint_sequence,ownership_generation,source_instance_id,
        source_identity,source_actor_user_id,target_instance_id,target_identity,
        target_principal_user_id,repository_id,target_working_copy_id,provider,gateway_account_id,
        model_route_id,model_id,required_capabilities_json,harness,harness_version,
        created_by,created_at_ms,updated_at_ms)
        VALUES (?1,?2,1,'active','source',?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
        ?15,?16,?17,?18,?17,?19,?20,?21,?22,?11,?23,?23)
        ON CONFLICT(binding_id) DO NOTHING",
        params![
            id,
            hash,
            source.spec.job_id,
            source.spec.session_id,
            source.spec.scope_id,
            source.checkpoint_digest,
            i64::try_from(source.checkpoint_sequence)?,
            i64::try_from(source.ownership.generation)?,
            source.source_instance_id,
            issuer,
            source.owner_user_id,
            input.target_instance_id,
            target,
            input.target_principal_user_id,
            input.repository_id,
            input.target_working_copy_id,
            source.spec.model_route_id,
            source.spec.gateway_account_id,
            source.spec.model_id,
            serde_json::to_string(&source.spec.required_capabilities)?,
            source.spec.harness,
            source.spec.harness_version,
            now
        ],
    )?;
    ensure!(
        binding_matches(policy, &id, &hash, &source, input, &issuer, target)?,
        "native handoff binding revoked or conflicts; reconcile"
    );
    let source_json = serde_json::to_string(&source)?;
    let input_json = serde_json::to_string(input)?;
    policy.execute(
        "INSERT INTO business_native_source_handoff_bindings (binding_id,capture_id,source_json,input_json)
        VALUES (?1,?2,?3,?4) ON CONFLICT(binding_id) DO NOTHING",
        params![id,input.capture_id,source_json,input_json],
    )?;
    let exact: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_handoff_bindings
        WHERE binding_id=?1 AND capture_id=?2 AND source_json=?3 AND input_json=?4)",
        params![id, input.capture_id, source_json, input_json],
        |r| r.get(0),
    )?;
    ensure!(exact, "native source provenance conflicts; reconcile");
    // No permission grant, durable-copy receipt, clean-effect witness or target
    // enrollment is written by this source-side preparation.
    Ok(SourceHandoffEnrollmentReceipt {
        binding_id: id,
        binding_digest: hash,
        checkpoint_digest: source.checkpoint_digest,
        checkpoint_sequence: source.checkpoint_sequence,
    })
}

/// Re-resolve the actual source before the gate signs. Caller retains the same
/// issuer/policy fences; this is not a fence over a later async byte stream.
pub(crate) fn validate_source_decision(
    root: &Path,
    policy: &Connection,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    request: &SessionHandoffGateRequest,
) -> Result<()> {
    let row: (String, String, String) = policy
        .query_row(
            "SELECT n.binding_id,n.source_json,n.input_json
        FROM business_native_source_handoff_bindings n
        JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id
        WHERE b.binding_digest=?1 AND b.side='source'",
            [&request.binding_digest],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .context("native source binding has no capture provenance")?;
    let stored: SourceHandoffFacts = serde_json::from_str(&row.1)?;
    let input: SourceHandoffEnrollment = serde_json::from_str(&row.2)?;
    ensure!(
        input.capture_id == stored.capture_id,
        "native capture provenance mismatched"
    );
    let source = resolve_source_handoff(root, policy, &input.capture_id)?;
    ensure!(source == stored, "native source authority changed");
    let issuer = identity.public_identity();
    let target = target_identity(config, identity, &source, &input)?;
    let hash = digest(&source, &input, &issuer, target)?;
    ensure!(
        hash == request.binding_digest
            && row.0 == format!("handoff_{hash}")
            && request.spec == source.spec
            && request.ownership == source.ownership
            && request.checkpoint_digest == source.checkpoint_digest
            && request.checkpoint_sequence == source.checkpoint_sequence
            && binding_matches(policy, &row.0, &hash, &source, &input, &issuer, target)?,
        "native source handoff binding mismatched"
    );
    Ok(())
}
