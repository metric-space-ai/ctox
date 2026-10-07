// Origin: CTOX
// License: AGPL-3.0-only
//! Actual target policy/store enrollment; standalone issuer fixtures do not prove a Core source.
use super::*;
use ctox_sync::contracts::{
    ExecutionOwnership, ExecutionPeer, ExecutionSpec, SyncHostMember, SyncHostTiming,
};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;

fn put(policy: &Connection, collection: &str, id: &str, value: serde_json::Value) {
    super::super::super::store::upsert_business_record(policy, collection, id, 1, value).unwrap();
}

pub(crate) fn prepare_policy(
    root: &Path,
    owner: &str,
    profile: &str,
    project: &str,
    thread: &str,
    repository: &str,
    copy: &str,
    spec: &ExecutionSpec,
    binding: &str,
) -> std::path::PathBuf {
    let policy = open_store(root).unwrap();
    policy.execute("INSERT INTO business_users(user_id,display_name,role,active,created_at_ms,updated_at_ms)
        VALUES (?1,'Target owner','user',1,1,1) ON CONFLICT(user_id) DO NOTHING",[owner]).unwrap();
    put(
        &policy,
        "workjet_projects",
        project,
        serde_json::json!({"owner_user_id":owner,"status":"active","is_deleted":false}),
    );
    put(
        &policy,
        "workjet_computers",
        "target-computer",
        serde_json::json!({"owner_user_id":owner,
        "status":"assigned","hosting_mode":"self_hosted","is_deleted":false}),
    );
    let profile_binding = super::super::super::worker_profile_bindings::binding_id(owner, profile);
    put(
        &policy,
        "workjet_worker_profile_bindings",
        &profile_binding,
        serde_json::json!({"owner_user_id":owner,
        "worker_profile_id":profile,"computer_id":"target-computer","crew_member_id":"crew-pico","status":"active","is_deleted":false}),
    );
    let member =
        super::super::super::project_chats::stable_id("workjet_member", &[owner, project, profile]);
    put(
        &policy,
        "workjet_project_workers",
        &member,
        serde_json::json!({"owner_user_id":owner,
        "project_id":project,"worker_profile_id":profile,"group_chat_id":thread,"status":"active","is_deleted":false}),
    );
    put(
        &policy,
        "workjet_project_chats",
        thread,
        serde_json::json!({"owner_user_id":owner,"project_id":project,
        "thread_id":thread,"kind":"group","is_deleted":false}),
    );
    put(
        &policy,
        "user_threads",
        thread,
        serde_json::json!({"owner_user_id":owner,"status":"open","is_deleted":false,"archived_at_ms":0}),
    );
    put(
        &policy,
        "workjet_working_copies",
        copy,
        serde_json::json!({"owner_user_id":owner,"project_id":project,
        "computer_id":"target-computer","status":"active","is_deleted":false,"path":"opaque-device://never/a/native/path"}),
    );
    for permission in [
        "ctox.session_handoff.receive",
        "ctox.session_handoff.execute",
    ] {
        policy.execute("INSERT INTO business_permission_grants
            (grant_id,subject_type,subject_id,permission,scope_type,scope_id,active,created_by,created_at_ms,updated_at_ms)
            VALUES (?1,'user',?2,?3,'session_handoff',?4,1,?2,1,1)
            ON CONFLICT(grant_id) DO UPDATE SET active=1",
            params![format!("{binding}:{permission}"),owner,permission,binding]).unwrap();
    }
    drop(policy);
    let workspace = root.join("target-native-workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    crate::business_os::configure_native_guest_assignments(
        root,
        "target-computer",
        &[crate::business_os::ProviderAssignmentInput {
            owner_user_id: owner.into(),
            worker_profile_id: profile.into(),
            gateway_account_id: spec.gateway_account_id.clone(),
            model_id: spec.model_id.clone(),
        }],
        &[crate::business_os::WorkspaceAssignmentInput {
            owner_user_id: owner.into(),
            worker_profile_id: profile.into(),
            project_id: project.into(),
            working_copy_id: copy.into(),
            native_workspace: workspace.clone(),
        }],
    )
    .unwrap();
    let caps = if spec.required_capabilities.is_empty() {
        BTreeSet::from(["execution.native".to_owned()])
    } else {
        spec.required_capabilities.clone()
    };
    crate::inference::runtime_env::set_runtime_env_value(root,"native_guest_host_config",
        &serde_json::json!({"version":1,"computerId":"target-computer","requiredCapabilities":caps}).to_string()).unwrap();
    target_handoff::configure_repository(
        root,
        &serde_json::to_string(&target_handoff::TargetRepositoryAssignment {
            owner_user_id: owner.into(),
            worker_profile_id: profile.into(),
            project_id: project.into(),
            working_copy_id: copy.into(),
            repository_id: repository.into(),
        })
        .unwrap(),
    )
    .unwrap();
    workspace
}

pub(crate) fn save_configuration(root: &Path, config: &HostConfiguration) {
    crate::persistence::store_text_value(root, "handoff_fixture", Some("present")).unwrap();
    let mut runtime =
        Connection::open(crate::inference::runtime_env::runtime_config_path(root)).unwrap();
    ctox_sync::host_config::save(&mut runtime, config).unwrap();
}

pub(crate) struct GateFixture {
    pub root: tempfile::TempDir,
    pub identity: std::sync::Arc<SigningIdentity>,
    pub source: SigningIdentity,
    pub request: SessionHandoffGateRequest,
}
pub(crate) fn gate_fixture() -> GateFixture {
    let root = tempfile::tempdir().unwrap();
    crate::sync_host::handle_command(root.path(), &["init".into()]).unwrap();
    let identity = crate::sync_host::signing_identity(root.path()).unwrap();
    let source = SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap();
    let third = SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap();
    let spec = ExecutionSpec {
        job_id: "job-1".into(),
        session_id: "session-1".into(),
        scope_id: "scope-1".into(),
        harness: ctox_core::native_harness_name().into(),
        harness_version: ctox_core::native_harness_version().into(),
        model_route_id: "openai".into(),
        gateway_account_id: "account-1".into(),
        model_id: "model-1".into(),
        required_capabilities: BTreeSet::from(["execution.native".into()]),
    };
    let config = HostConfiguration {
        version: 1,
        scope_id: spec.scope_id.clone(),
        local: SyncHostMember::Voter { node_id: 2 },
        voters: BTreeMap::from([
            (
                1,
                ExecutionPeer {
                    identity: source.public_identity(),
                    executor: true,
                    data_replica: true,
                },
            ),
            (
                2,
                ExecutionPeer {
                    identity: identity.public_identity(),
                    executor: true,
                    data_replica: true,
                },
            ),
            (
                3,
                ExecutionPeer {
                    identity: third.public_identity(),
                    executor: false,
                    data_replica: true,
                },
            ),
        ]),
        timing: SyncHostTiming {
            heartbeat_ms: 100,
            election_min_ms: 300,
            election_max_ms: 600,
        },
    };
    save_configuration(root.path(), &config);
    let instance = super::super::super::store::sync_connection_config(root.path())
        .unwrap()
        .instance_id;
    let facts = SourceHandoffFacts {
        capture_id: "isolated-signature-fixture".into(),
        source_instance_id: "source-instance".into(),
        owner_user_id: "source-owner".into(),
        project_id: "project".into(),
        worker_profile_id: "source-profile".into(),
        policy_revision: "ab".repeat(32),
        spec: spec.clone(),
        ownership: ExecutionOwnership {
            node_id: 1,
            generation: 2,
        },
        checkpoint_digest: "ab".repeat(32),
        checkpoint_sequence: 7,
        source_working_copy_id: "source-copy".into(),
        workspace_revision: 1,
    };
    let selection = SourceHandoffEnrollment {
        capture_id: facts.capture_id.clone(),
        target_node_id: 2,
        target_instance_id: instance,
        target_principal_user_id: "alice".into(),
        repository_id: "repository".into(),
        target_working_copy_id: "copy".into(),
    };
    let hash = digest(
        &facts,
        &selection,
        &source.public_identity(),
        &identity.public_identity(),
    )
    .unwrap();
    let binding = format!("handoff_{hash}");
    prepare_policy(
        root.path(),
        "alice",
        "profile",
        "project",
        "thread",
        "repository",
        "copy",
        &spec,
        &binding,
    );
    let challenge = challenge(root.path(), &config, &identity).unwrap();
    let body = SourceOfferBody {
        version: 1,
        binding_id: binding,
        binding_digest: hash.clone(),
        binding_revision: 3,
        source: facts,
        selection,
        source_identity: source.public_identity(),
        target_identity: identity.public_identity(),
        thread_id: "thread".into(),
        target_challenge: challenge,
    };
    let request = offer_request(&body).unwrap();
    // This standalone fixture tests the target/signature/issuer seams only.
    // The actual Core capture regression separately invokes native_source_offer.
    let at = u64::try_from(now_ms()).unwrap();
    let disclosure = source
        .sign_session_handoff_permit(&SessionHandoffPermit {
            version: 1,
            binding_digest: hash.clone(),
            phase: SessionHandoffPhase::Disclose,
            audience: spec.scope_id.clone(),
            nonce: request.nonce,
            job_id: spec.job_id.clone(),
            session_id: spec.session_id.clone(),
            scope_id: spec.scope_id.clone(),
            checkpoint_digest: body.source.checkpoint_digest.clone(),
            checkpoint_sequence: 7,
            ownership_generation: 2,
            principal_epoch: 11,
            binding_revision: 3,
            issued_at_ms: at,
            expires_at_ms: at + 60_000,
            signature: String::new(),
        })
        .unwrap();
    let input = TargetEnrollment {
        offer: SourceOffer { body, disclosure },
        worker_profile_id: "profile".into(),
    };
    crate::sync_host::with_current_signing_identity(root.path(), |key| {
        enroll(
            root.path(),
            &config,
            key,
            &serde_json::json!({"offer":input.offer,"workerProfileId":input.worker_profile_id})
                .to_string(),
        )
    })
    .unwrap();
    GateFixture {
        root,
        identity: identity.clone(),
        source,
        request: SessionHandoffGateRequest {
            issuer_identity: identity.public_identity(),
            phase: SessionHandoffPhase::Receive,
            binding_digest: hash,
            audience: spec.scope_id.clone(),
            nonce: "nonce-1".into(),
            spec,
            checkpoint_digest: "ab".repeat(32),
            checkpoint_sequence: 7,
            ownership: ExecutionOwnership {
                node_id: 1,
                generation: 2,
            },
        },
    }
}

pub(crate) fn assert_actual_source_target_enrollment(
    source_root: &Path,
    target_root: &Path,
    source_config: &HostConfiguration,
    spec: &ExecutionSpec,
    binding: &str,
    selection: &SourceHandoffEnrollment,
) {
    let mut config = source_config.clone();
    config.local = SyncHostMember::Voter { node_id: 2 };
    save_configuration(target_root, &config);
    prepare_policy(
        target_root,
        "owner",
        "target-profile",
        "project",
        "thread",
        &selection.repository_id,
        &selection.target_working_copy_id,
        spec,
        binding,
    );
    let target_key = crate::sync_host::signing_identity(target_root).unwrap();
    let challenge = challenge(target_root, &config, &target_key).unwrap();
    let offer = crate::business_os::native_source_offer(source_root, binding, &challenge).unwrap();
    assert_eq!(offer.body.source.spec, *spec);
    assert_eq!(offer.body.thread_id, "thread");
    let encoded = serde_json::json!({"offer":offer,"workerProfileId":"target-profile"}).to_string();
    let enroll_now = |text: &str| {
        crate::sync_host::with_current_signing_identity(target_root, |key| {
            enroll(target_root, &config, key, text)
        })
    };
    let policy = open_store(target_root).unwrap();
    policy
        .execute_batch(
            "CREATE TRIGGER fail_target_enrollment BEFORE INSERT ON business_events
        WHEN NEW.command_type='business_os.session_handoff.target_enrolled'
        BEGIN SELECT RAISE(ABORT,'target audit fixture'); END;",
        )
        .unwrap();
    assert!(enroll_now(&encoded).is_err());
    assert_eq!(
        policy
            .query_row(
                "SELECT count(*) FROM business_native_target_handoff_bindings",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(policy.query_row("SELECT count(*) FROM business_native_target_handoff_challenges WHERE used_binding_id IS NOT NULL",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    policy
        .execute_batch("DROP TRIGGER fail_target_enrollment")
        .unwrap();
    let mut wrong_profile: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    wrong_profile["workerProfileId"] = serde_json::json!("unassigned-profile");
    assert!(enroll_now(&wrong_profile.to_string()).is_err());
    policy.execute("UPDATE business_native_target_repository_assignments SET repository_id='foreign-repository'", []).unwrap();
    assert!(
        enroll_now(&encoded).is_err(),
        "a valid source proof cannot assign a target repository"
    );
    policy
        .execute(
            "UPDATE business_native_target_repository_assignments SET repository_id=?1",
            [&selection.repository_id],
        )
        .unwrap();
    for field in [
        "source",
        "selection",
        "threadId",
        "targetChallenge",
        "targetIdentity",
    ] {
        let mut changed: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        match field {
            "source" => {
                changed["offer"]["body"]["source"]["spec"]["gatewayAccountId"] =
                    serde_json::json!("foreign-account")
            }
            "selection" => {
                changed["offer"]["body"]["selection"]["repositoryId"] =
                    serde_json::json!("foreign-repository")
            }
            _ => changed["offer"]["body"][field] = serde_json::json!("foreign"),
        }
        assert!(
            enroll_now(&changed.to_string()).is_err(),
            "tampered {field}"
        );
    }
    policy
        .execute(
            "UPDATE business_native_target_handoff_challenges SET expires_at_ms=1 WHERE nonce=?1",
            [&challenge],
        )
        .unwrap();
    assert!(
        enroll_now(&encoded).is_err(),
        "expired target challenge cannot enroll"
    );
    policy
        .execute(
            "UPDATE business_native_target_handoff_challenges SET expires_at_ms=?1 WHERE nonce=?2",
            params![i64::try_from(now_ms()).unwrap() + 60_000, challenge],
        )
        .unwrap();
    let first = enroll_now(&encoded).unwrap();
    let retry = enroll_now(&encoded).unwrap();
    assert_eq!(first.binding_revision, retry.binding_revision);
    let gate = super::super::super::native_session_handoff_gate(target_root).unwrap();
    let mut request = offer_request(
        &serde_json::from_str::<SourceOffer>(
            &serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["offer"].to_string(),
        )
        .unwrap()
        .body,
    )
    .unwrap();
    request.issuer_identity = target_key.public_identity();
    request.phase = SessionHandoffPhase::Receive;
    request.nonce = "target-fresh-decision".into();
    assert!(gate.authorize(&request).is_ok());
    request.phase = SessionHandoffPhase::Resume;
    assert!(gate.authorize(&request).is_ok());
    let workspace = target_root.join("target-native-workspace");
    let moved = target_root.join("retained-native-workspace");
    std::fs::rename(&workspace, &moved).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "same path with a different inode must not retain authority"
    );
    std::fs::remove_dir(&workspace).unwrap();
    std::fs::rename(&moved, &workspace).unwrap();
    assert!(gate.authorize(&request).is_ok());
    target_handoff::configure_repository(
        target_root,
        &serde_json::json!({
            "ownerUserId":"owner", "workerProfileId":"target-profile", "projectId":"project",
            "workingCopyId":selection.target_working_copy_id, "repositoryId":selection.repository_id
        })
        .to_string(),
    )
    .unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "repository revision changes fence the previous enrollment"
    );
    assert!(
        enroll_now(&encoded).is_err(),
        "a consumed challenge cannot renew changed policy"
    );
    let renewed_challenge = super::challenge(target_root, &config, &target_key).unwrap();
    let renewed_offer =
        crate::business_os::native_source_offer(source_root, binding, &renewed_challenge).unwrap();
    let renewed_encoded =
        serde_json::json!({"offer":renewed_offer,"workerProfileId":"target-profile"}).to_string();
    let renewed = enroll_now(&renewed_encoded).unwrap();
    assert_eq!(renewed.binding_revision, first.binding_revision + 1);
    assert_eq!(
        enroll_now(&renewed_encoded).unwrap().binding_revision,
        renewed.binding_revision
    );
    assert!(gate.authorize(&request).is_ok());
    let retry_challenge = super::challenge(target_root, &config, &target_key).unwrap();
    let retry_offer =
        crate::business_os::native_source_offer(source_root, binding, &retry_challenge).unwrap();
    let retry_encoded =
        serde_json::json!({"offer":retry_offer,"workerProfileId":"target-profile"}).to_string();
    assert_eq!(
        enroll_now(&retry_encoded).unwrap().binding_revision,
        renewed.binding_revision
    );
    assert_eq!(
        enroll_now(&retry_encoded).unwrap().binding_revision,
        renewed.binding_revision
    );
    policy.execute("UPDATE business_native_guest_provider_assignments SET gateway_account_id='foreign-account'",[]).unwrap();
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "target_authority_changed"
    );
    policy
        .execute(
            "UPDATE business_native_guest_provider_assignments SET gateway_account_id=?1",
            [&spec.gateway_account_id],
        )
        .unwrap();
    let member = super::super::super::project_chats::stable_id(
        "workjet_member",
        &["owner", "project", "target-profile"],
    );
    put(
        &policy,
        "workjet_project_workers",
        &member,
        serde_json::json!({"owner_user_id":"owner","project_id":"project",
        "worker_profile_id":"target-profile","group_chat_id":"thread","status":"inactive","is_deleted":false}),
    );
    assert!(gate.authorize(&request).is_err());
    put(
        &policy,
        "workjet_project_workers",
        &member,
        serde_json::json!({"owner_user_id":"owner","project_id":"project",
        "worker_profile_id":"target-profile","group_chat_id":"thread","status":"active","is_deleted":false}),
    );
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "target_authority_changed"
    );
    // Returning to active creates a new durable membership revision. It must
    // require a fresh target challenge rather than resurrect an old permit.
    assert!(enroll_now(&retry_encoded).is_err());
    let member_challenge = super::challenge(target_root, &config, &target_key).unwrap();
    let member_offer =
        crate::business_os::native_source_offer(source_root, binding, &member_challenge).unwrap();
    let member_encoded =
        serde_json::json!({"offer":member_offer,"workerProfileId":"target-profile"}).to_string();
    assert_eq!(
        enroll_now(&member_encoded).unwrap().binding_revision,
        renewed.binding_revision + 1
    );
    assert!(gate.authorize(&request).is_ok());
    policy.execute("UPDATE business_permission_grants SET active=0 WHERE permission='ctox.session_handoff.receive'",[]).unwrap();
    request.phase = SessionHandoffPhase::Receive;
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "grant_missing"
    );
    request.phase = SessionHandoffPhase::Resume;
    assert!(
        gate.authorize(&request).is_err(),
        "epoch mutation fences execution even with its grant retained"
    );
    assert!(
        enroll_now(&encoded).is_err(),
        "used challenge cannot refresh stale target policy"
    );
    assert!(
        crate::business_os::session_handoff_enrollment::revoke_binding(target_root, binding)
            .unwrap()
    );
    assert!(
        enroll_now(&encoded).is_err(),
        "revoked target cannot be resurrected"
    );
    let payloads: String = policy
        .query_row(
            "SELECT group_concat(payload_json) FROM business_events
        WHERE command_type LIKE 'business_os.session_handoff.%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!payloads.contains(&challenge));
    assert!(!payloads.contains(&renewed_challenge));
    assert!(!payloads.contains(&retry_challenge));
    assert!(!payloads.contains(&member_challenge));
    assert!(!payloads.contains("target-native-workspace"));
    assert!(!payloads.contains("actual-native-workspace"));
}
