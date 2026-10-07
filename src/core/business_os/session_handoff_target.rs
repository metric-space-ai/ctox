// Origin: CTOX
// License: AGPL-3.0-only
//! Trusted native target enrollment. Public metadata, no checkpoint/credential bytes.
#[cfg(test)]
#[path = "session_handoff_target_tests.rs"]
pub(crate) mod tests;
use super::super::guest_registry::target_handoff::{self, TargetPolicyScope};
use super::*;
use ctox_sync::{
    authority::auth::session_handoff::{
        verify_fresh_session_handoff_permit, verify_session_handoff_permit,
    },
    contracts::{SessionHandoffPermit, SessionHandoffPhase},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceOfferBody {
    pub version: u32,
    pub binding_id: String,
    pub binding_digest: String,
    pub binding_revision: u64,
    pub source: SourceHandoffFacts,
    pub selection: SourceHandoffEnrollment,
    pub source_identity: String,
    pub target_identity: String,
    /// Logical Workjet chat, distinct from the captured Core provider session.
    pub thread_id: String,
    pub target_challenge: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceOffer {
    pub body: SourceOfferBody,
    pub disclosure: SessionHandoffPermit,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TargetEnrollment {
    pub offer: SourceOffer,
    pub worker_profile_id: String,
}

fn nonce(body: &SourceOfferBody) -> Result<String> {
    let mut encoded = serde_json::to_value(body)?;
    encoded.sort_all_objects();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::json!([
            "ctox-native-target-enrollment-offer-v1",
            encoded
        ]))?)
    ))
}
pub(crate) fn offer_request(body: &SourceOfferBody) -> Result<SessionHandoffGateRequest> {
    Ok(SessionHandoffGateRequest {
        issuer_identity: body.source_identity.clone(),
        phase: SessionHandoffPhase::Disclose,
        binding_digest: body.binding_digest.clone(),
        audience: body.source.spec.scope_id.clone(),
        nonce: nonce(body)?,
        spec: body.source.spec.clone(),
        checkpoint_digest: body.source.checkpoint_digest.clone(),
        checkpoint_sequence: body.source.checkpoint_sequence,
        ownership: body.source.ownership.clone(),
    })
}
pub(crate) fn offer_body(
    policy: &Connection,
    binding: &str,
    challenge: &str,
) -> Result<SourceOfferBody> {
    ensure!(
        challenge.len() == 32
            && challenge
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid target challenge"
    );
    let (source,input,id,target,hash,revision):(String,String,String,String,String,i64)=policy.query_row(
        "SELECT COALESCE(a.source_json,n.source_json),n.input_json,b.source_identity,b.target_identity,
        b.binding_digest,b.revision FROM business_session_handoff_bindings b
        JOIN business_native_source_handoff_bindings n ON n.binding_id=b.binding_id
        LEFT JOIN business_native_source_handoff_authorizations a ON a.binding_id=b.binding_id
        WHERE b.binding_id=?1 AND b.side='source' AND b.state='active'",[binding],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let source: SourceHandoffFacts = serde_json::from_str(&source)?;
    let thread_id: String = policy.query_row(
        "SELECT thread_id FROM business_native_source_journals WHERE capture_id=?1",
        [&source.capture_id],
        |r| r.get(0),
    )?;
    Ok(SourceOfferBody {
        version: 1,
        binding_id: binding.into(),
        binding_digest: hash,
        binding_revision: u64::try_from(revision)?,
        source,
        selection: serde_json::from_str(&input)?,
        source_identity: id,
        target_identity: target,
        thread_id,
        target_challenge: challenge.into(),
    })
}

/// A target-owned, bounded, single-use challenge; it is not an execution permit.
pub(crate) fn challenge(
    root: &Path,
    config: &HostConfiguration,
    identity: &SigningIdentity,
) -> Result<String> {
    config.validate_key(identity)?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let mut policy = open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = i64::try_from(now_ms())?;
    tx.execute(
        "DELETE FROM business_native_target_handoff_challenges WHERE expires_at_ms<?1",
        [now],
    )?;
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM business_native_target_handoff_challenges",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        count < 64,
        "native target has too many outstanding challenges"
    );
    tx.execute(
        "INSERT INTO business_native_target_handoff_challenges
        (nonce,target_identity,scope_id,expires_at_ms) VALUES (?1,?2,?3,?4)",
        params![
            nonce,
            identity.public_identity(),
            config.scope_id,
            now.checked_add(60_000).context("clock overflow")?
        ],
    )?;
    tx.commit()?;
    Ok(nonce)
}

fn verify_offer(
    offer: &SourceOffer,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    fresh: bool,
) -> Result<()> {
    config.validate_key(identity)?;
    let b = &offer.body;
    validate_input(&b.selection)?;
    ensure!(
        b.version == 1
            && b.binding_id.starts_with("handoff_")
            && b.binding_id.len() == 72
            && b.binding_id[8..]
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            && b.binding_revision > 0
            && b.selection.capture_id == b.source.capture_id
            && b.selection.target_node_id == config.node_id()
            && b.source.ownership.node_id != config.node_id()
            && b.source.spec.scope_id == config.scope_id
            && b.target_identity == identity.public_identity()
            && b.target_challenge.len() == 32
            && b.target_challenge
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "offer does not name this enrolled target"
    );
    let source = config
        .voters
        .get(&b.source.ownership.node_id)
        .context("source peer is not enrolled")?;
    let target = config
        .voters
        .get(&config.node_id())
        .context("target is not a voter")?;
    ensure!(
        source.identity == b.source_identity
            && source.executor
            && source.data_replica
            && target.executor
            && target.data_replica,
        "source/target membership is unavailable"
    );
    ensure!(
        digest(
            &b.source,
            &b.selection,
            &b.source_identity,
            &b.target_identity
        )? == b.binding_digest,
        "offer capture digest differs"
    );
    let request = offer_request(b)?;
    let p = &offer.disclosure;
    ensure!(
        p.binding_digest == b.binding_digest
            && p.binding_revision == b.binding_revision
            && p.phase == SessionHandoffPhase::Disclose
            && p.job_id == b.source.spec.job_id
            && p.session_id == b.source.spec.session_id
            && p.scope_id == config.scope_id
            && p.checkpoint_digest == b.source.checkpoint_digest
            && p.checkpoint_sequence == b.source.checkpoint_sequence
            && p.ownership_generation == b.source.ownership.generation,
        "source disclosure does not bind this offer"
    );
    if fresh {
        verify_fresh_session_handoff_permit(
            p,
            &source.identity,
            &config.scope_id,
            &request.nonce,
            u64::try_from(now_ms())?,
        )?;
    } else {
        verify_session_handoff_permit(p, &source.identity, &config.scope_id, &request.nonce)?;
    }
    Ok(())
}

pub(crate) fn enroll(
    root: &Path,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    encoded: &str,
) -> Result<SourceHandoffEnrollmentReceipt> {
    ensure!(
        encoded.len() <= 32 * 1024,
        "target enrollment exceeds its bound"
    );
    let input: TargetEnrollment = serde_json::from_str(encoded)?;
    let mut policy = open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let receipt = enroll_with_conn(root, &tx, config, identity, &input)?;
    tx.commit()?;
    Ok(receipt)
}

pub(crate) fn enroll_with_conn(
    root: &Path,
    policy: &Connection,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    input: &TargetEnrollment,
) -> Result<SourceHandoffEnrollmentReceipt> {
    verify_offer(&input.offer, config, identity, true)?;
    let b = &input.offer.body;
    // Never create or infer an instance identity during enrollment.
    let instance = super::super::store::existing_instance_id(root)?;
    ensure!(
        instance == b.selection.target_instance_id,
        "target instance differs from source selection"
    );
    let scope = TargetPolicyScope {
        owner_user_id: b.selection.target_principal_user_id.clone(),
        worker_profile_id: input.worker_profile_id.clone(),
        project_id: b.source.project_id.clone(),
        thread_id: b.thread_id.clone(),
        working_copy_id: b.selection.target_working_copy_id.clone(),
        repository_id: b.selection.repository_id.clone(),
        policy_revision: String::new(),
    };
    let scope = target_handoff::resolve(root, policy, &scope, &b.source.spec)?;
    let (expiry, used): (i64, Option<String>) = policy.query_row(
        "SELECT expires_at_ms,used_binding_id FROM business_native_target_handoff_challenges
        WHERE nonce=?1 AND target_identity=?2 AND scope_id=?3",
        params![
            b.target_challenge,
            identity.public_identity(),
            config.scope_id
        ],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        expiry > i64::try_from(now_ms())?,
        "target challenge expired"
    );
    let existing: Option<(String, i64, String, String)> = policy
        .query_row(
            "SELECT b.state,b.revision,t.source_offer_json,t.target_scope_json
        FROM business_session_handoff_bindings b LEFT JOIN business_native_target_handoff_bindings t
        ON t.binding_id=b.binding_id WHERE b.binding_id=?1 AND b.side='target'",
            [&b.binding_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let offer_json = serde_json::to_string(&input.offer)?;
    let scope_json = serde_json::to_string(&scope)?;
    let (revision, changed) = if let Some((state, revision, previous, prior_scope)) = existing {
        ensure!(
            state == "active",
            "revoked target binding cannot be resurrected"
        );
        let old: SourceOffer = serde_json::from_str(&previous)?;
        let mut expected = b.clone();
        expected.target_challenge = old.body.target_challenge.clone();
        expected.source.policy_revision = old.body.source.policy_revision.clone();
        expected.source.workspace_revision = old.body.source.workspace_revision;
        expected.binding_digest = old.body.binding_digest.clone();
        expected.binding_revision = old.body.binding_revision;
        ensure!(
            serde_json::to_value(&expected)? == serde_json::to_value(&old.body)?
                && b.binding_revision >= old.body.binding_revision,
            "source scope changed or revision regressed"
        );
        if let Some(used) = &used {
            ensure!(
                used == &b.binding_id && previous == offer_json && prior_scope == scope_json,
                "used challenge cannot authorize a different enrollment"
            );
        }
        let changed = old.body.binding_digest != b.binding_digest || prior_scope != scope_json;
        (
            if changed {
                revision
                    .checked_add(1)
                    .context("target revision exhausted")?
            } else {
                revision
            },
            changed,
        )
    } else {
        ensure!(used.is_none(), "target challenge already used");
        (i64::try_from(b.binding_revision)?, true)
    };
    policy.execute("INSERT INTO business_session_handoff_bindings
        (binding_id,binding_digest,revision,state,side,job_id,session_id,scope_id,checkpoint_digest,
        checkpoint_sequence,ownership_generation,source_instance_id,source_identity,source_actor_user_id,
        target_instance_id,target_identity,target_principal_user_id,repository_id,target_working_copy_id,
        provider,gateway_account_id,model_route_id,model_id,required_capabilities_json,harness,harness_version,
        created_by,created_at_ms,updated_at_ms)
        VALUES (?1,?2,?3,'active','target',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
        ?18,?19,?18,?20,?21,?22,?23,?15,?24,?24)
        ON CONFLICT(binding_id) DO UPDATE SET binding_digest=excluded.binding_digest,
        revision=excluded.revision,updated_at_ms=excluded.updated_at_ms
        WHERE business_session_handoff_bindings.side='target' AND business_session_handoff_bindings.state='active'",
        params![b.binding_id,b.binding_digest,revision,b.source.spec.job_id,b.source.spec.session_id,
            b.source.spec.scope_id,b.source.checkpoint_digest,i64::try_from(b.source.checkpoint_sequence)?,
            i64::try_from(b.source.ownership.generation)?,b.source.source_instance_id,b.source_identity,
            b.source.owner_user_id,instance,b.target_identity,scope.owner_user_id,scope.repository_id,
            scope.working_copy_id,b.source.spec.model_route_id,b.source.spec.gateway_account_id,
            b.source.spec.model_id,serde_json::to_string(&b.source.spec.required_capabilities)?,
            b.source.spec.harness,b.source.spec.harness_version,i64::try_from(now_ms())?])?;
    if changed {
        policy.execute("INSERT INTO business_native_target_handoff_bindings
            (binding_id,binding_revision,source_offer_json,target_scope_json) VALUES (?1,?2,?3,?4)
            ON CONFLICT(binding_id) DO UPDATE SET binding_revision=excluded.binding_revision,
            source_offer_json=excluded.source_offer_json,target_scope_json=excluded.target_scope_json",
            params![b.binding_id,revision,offer_json,scope_json])?;
        super::super::store::insert_business_event(
            policy,
            "business_session_handoff_bindings",
            &b.binding_id,
            "business_os.session_handoff.target_enrolled",
            serde_json::json!({"version":1,
                "binding_digest":b.binding_digest,"binding_revision":revision,"target_principal_id":scope.owner_user_id,
                "project_id":scope.project_id,"thread_id":scope.thread_id}),
            i64::try_from(now_ms())?,
        )?;
    }
    if !changed {
        // Retain the exact newly consumed proof so its lost-response retry is
        // idempotent even when the existing target authority did not change.
        policy.execute(
            "UPDATE business_native_target_handoff_bindings SET source_offer_json=?1
            WHERE binding_id=?2 AND binding_revision=?3",
            params![offer_json, b.binding_id, revision],
        )?;
    }
    let changed = policy.execute(
        "UPDATE business_native_target_handoff_challenges SET used_binding_id=?1
        WHERE nonce=?2 AND (used_binding_id IS NULL OR used_binding_id=?1)",
        params![b.binding_id, b.target_challenge],
    )?;
    ensure!(changed == 1, "target challenge changed");
    validate_target_decision(
        root,
        policy,
        config,
        identity,
        &SessionHandoffGateRequest {
            issuer_identity: identity.public_identity(),
            phase: SessionHandoffPhase::Receive,
            binding_digest: b.binding_digest.clone(),
            audience: config.scope_id.clone(),
            nonce: "enrollment".into(),
            spec: b.source.spec.clone(),
            checkpoint_digest: b.source.checkpoint_digest.clone(),
            checkpoint_sequence: b.source.checkpoint_sequence,
            ownership: b.source.ownership.clone(),
        },
    )?;
    Ok(SourceHandoffEnrollmentReceipt {
        binding_id: b.binding_id.clone(),
        binding_digest: b.binding_digest.clone(),
        checkpoint_digest: b.source.checkpoint_digest.clone(),
        checkpoint_sequence: b.source.checkpoint_sequence,
        binding_revision: revision,
    })
}

pub(crate) fn validate_target_decision(
    root: &Path,
    policy: &Connection,
    config: &HostConfiguration,
    identity: &SigningIdentity,
    request: &SessionHandoffGateRequest,
) -> Result<()> {
    let (offer, scope, id, revision): (String, String, String, i64) = policy.query_row(
        "SELECT t.source_offer_json,t.target_scope_json,b.binding_id,b.revision
        FROM business_native_target_handoff_bindings t JOIN business_session_handoff_bindings b
        ON b.binding_id=t.binding_id WHERE b.binding_digest=?1 AND b.side='target'
        AND b.state='active' AND b.revision=t.binding_revision",
        [&request.binding_digest],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let offer: SourceOffer = serde_json::from_str(&offer)?;
    verify_offer(&offer, config, identity, false)?;
    let expected: TargetPolicyScope = serde_json::from_str(&scope)?;
    let current = target_handoff::resolve(root, policy, &expected, &offer.body.source.spec)?;
    let b = &offer.body;
    ensure!(
        current == expected
            && id == b.binding_id
            && revision > 0
            && request.phase != SessionHandoffPhase::Disclose
            && request.spec == b.source.spec
            && request.ownership == b.source.ownership
            && request.audience == config.scope_id
            && request.binding_digest == b.binding_digest
            && request.checkpoint_digest == b.source.checkpoint_digest
            && request.checkpoint_sequence == b.source.checkpoint_sequence
            && super::super::store::existing_instance_id(root)? == b.selection.target_instance_id,
        "current target authority differs from enrollment"
    );
    let exact:bool=policy.query_row("SELECT EXISTS(SELECT 1 FROM business_session_handoff_bindings
        WHERE binding_id=?1 AND binding_digest=?2 AND revision=?3 AND side='target' AND state='active'
        AND job_id=?4 AND session_id=?5 AND scope_id=?6 AND checkpoint_digest=?7 AND checkpoint_sequence=?8
        AND ownership_generation=?9 AND source_instance_id=?10 AND source_identity=?11 AND source_actor_user_id=?12
        AND target_instance_id=?13 AND target_identity=?14 AND target_principal_user_id=?15 AND repository_id=?16
        AND target_working_copy_id=?17 AND model_route_id=?18 AND gateway_account_id=?19 AND model_id=?20
        AND required_capabilities_json=?21 AND harness=?22 AND harness_version=?23 AND created_by=?15)",
        params![id,b.binding_digest,revision,b.source.spec.job_id,b.source.spec.session_id,b.source.spec.scope_id,
            b.source.checkpoint_digest,i64::try_from(b.source.checkpoint_sequence)?,i64::try_from(b.source.ownership.generation)?,
            b.source.source_instance_id,b.source_identity,b.source.owner_user_id,b.selection.target_instance_id,
            b.target_identity,current.owner_user_id,current.repository_id,current.working_copy_id,b.source.spec.model_route_id,
            b.source.spec.gateway_account_id,b.source.spec.model_id,serde_json::to_string(&b.source.spec.required_capabilities)?,
            b.source.spec.harness,b.source.spec.harness_version],|r|r.get(0))?;
    ensure!(exact, "target binding changed");
    Ok(())
}
