// Origin: CTOX
// License: AGPL-3.0-only
//! Reuse the actual stopped Core and Git checkpoint, not fabricated source rows.
use super::super::super::session_handoff_enrollment as enrollment;
use super::*;
use ctox_sync::authority::handoff::SessionHandoffGateRequest;
use ctox_sync::contracts::{ExecutionPeer, SessionHandoffPhase, SyncHostMember, SyncHostTiming};

fn new_identity() -> ctox_sync::authority::auth::SigningIdentity {
    use ctox_sync::authority::auth::SigningIdentity;
    SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap()
}

pub(super) fn assert_native_source_handoff_enrollment(
    root: &Path,
    registry: &NativeGuestRegistry,
    receipt: &super::super::NativeSourceJournalReceipt,
    spec: &ExecutionSpec,
    ownership: &Ownership,
) {
    crate::sync_host::handle_command(root, &["init".into()]).unwrap();
    let identity = crate::sync_host::signing_identity(root).unwrap();
    let target = new_identity();
    let third = new_identity();
    let config = ctox_sync::host_config::HostConfiguration {
        version: 1,
        scope_id: spec.scope_id.clone(),
        local: SyncHostMember::Voter {
            node_id: ownership.node_id,
        },
        voters: std::collections::BTreeMap::from([
            (
                ownership.node_id,
                ExecutionPeer {
                    identity: identity.public_identity(),
                    executor: true,
                    data_replica: true,
                },
            ),
            (
                2,
                ExecutionPeer {
                    identity: target.public_identity(),
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
    config.validate().unwrap();
    let mut runtime =
        Connection::open(crate::inference::runtime_env::runtime_config_path(root)).unwrap();
    ctox_sync::host_config::save(&mut runtime, &config).unwrap();
    drop(runtime);
    let mut input = enrollment::SourceHandoffEnrollment {
        capture_id: receipt.capture_id.clone(),
        target_node_id: 2,
        target_instance_id: "operator-selected-target-instance".into(),
        target_principal_user_id: "owner".into(),
        repository_id: "operator-selected-repository".into(),
        target_working_copy_id: "target-copy".into(),
    };
    let encoded = serde_json::to_string(&input).unwrap();
    let first = crate::sync_host::with_current_signing_identity(root, |key| {
        enrollment::enroll_source(root, &config, key, &encoded)
    })
    .unwrap();
    let repeated = registry
        .with_policy(|tx| enrollment::enroll_source_with_conn(root, tx, &config, &identity, &input))
        .unwrap();
    assert_eq!(first.binding_id, repeated.binding_id);
    assert_eq!(first.binding_digest, repeated.binding_digest);
    let gate = super::super::super::native_session_handoff_gate(root).unwrap();
    let request = SessionHandoffGateRequest {
        issuer_identity: identity.public_identity(),
        phase: SessionHandoffPhase::Disclose,
        binding_digest: first.binding_digest.clone(),
        audience: spec.scope_id.clone(),
        nonce: "fresh-native-disclosure-fixture".into(),
        spec: spec.clone(),
        checkpoint_digest: first.checkpoint_digest.clone(),
        checkpoint_sequence: first.checkpoint_sequence,
        ownership: ownership.clone(),
    };
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "grant_missing"
    );
    registry.with_policy(|tx| {
        tx.execute(
            "INSERT INTO business_permission_grants
            (grant_id,subject_type,subject_id,permission,scope_type,scope_id,active,created_by,created_at_ms,updated_at_ms)
            VALUES ('native-handoff-grant','user','owner','session.handoff.disclose','session_handoff',?1,1,'owner',1,1)",
            [&first.binding_id],
        )?;
        Ok(())
    }).unwrap();
    // The permission spelling comes from the production enum, not a client.
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_permission_grants SET permission=?1 WHERE grant_id='native-handoff-grant'",
            [super::super::super::policy::BusinessOsPermission::SessionHandoffDisclose.as_str()])?;
        Ok(())
    }).unwrap();
    registry
        .with_policy(|tx| {
            enrollment::validate_source_decision(root, tx, &config, &identity, &request)
        })
        .expect("exact disclosure grant must preserve native source authority");
    registry
        .with_policy(|tx| {
            // The persistent trigger is visible to the gate's independent connection.
            tx.execute_batch(
                "CREATE TRIGGER fail_native_handoff_audit BEFORE INSERT ON business_events
            WHEN NEW.command_type='business_os.session_handoff.allowed'
            BEGIN SELECT RAISE(ABORT,'native audit fixture failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "policy_audit_unavailable",
        "an unaudited permit must never escape"
    );
    registry
        .with_policy(|tx| {
            tx.execute_batch("DROP TRIGGER fail_native_handoff_audit")?;
            Ok(())
        })
        .unwrap();
    let permitted = gate.authorize(&request).unwrap();
    assert_eq!(permitted.session_id, spec.session_id);
    ctox_sync::authority::auth::session_handoff::verify_session_handoff_permit(
        &permitted,
        &identity.public_identity(),
        &request.audience,
        &request.nonce,
    )
    .unwrap();

    for changed in 0..3 {
        let mut wrong = request.clone();
        match changed {
            0 => {
                wrong
                    .spec
                    .required_capabilities
                    .insert("foreign-capability".into());
            }
            1 => wrong.ownership.node_id = 2,
            _ => wrong.spec.gateway_account_id = "foreign-account".into(),
        }
        assert!(
            gate.authorize(&wrong).is_err(),
            "entire native producer binding must match"
        );
    }
    let saved = input.capture_id.clone();
    input.capture_id = "capture_missing".into();
    assert!(registry
        .with_policy(|tx| enrollment::enroll_source_with_conn(root, tx, &config, &identity, &input))
        .is_err());
    input.capture_id = saved;
    input.target_node_id = 3;
    assert!(registry
        .with_policy(|tx| enrollment::enroll_source_with_conn(root, tx, &config, &identity, &input))
        .is_err());
    input.target_node_id = 99;
    assert!(registry
        .with_policy(|tx| enrollment::enroll_source_with_conn(root, tx, &config, &identity, &input))
        .is_err());
    input.target_node_id = 2;
    let mut foreign_config = config.clone();
    foreign_config.voters.get_mut(&2).unwrap().identity = new_identity().public_identity();
    let mut runtime =
        Connection::open(crate::inference::runtime_env::runtime_config_path(root)).unwrap();
    runtime
        .execute(
            "UPDATE ctox_sync_host SET configuration=?1 WHERE singleton=1",
            [serde_json::to_string(&foreign_config).unwrap()],
        )
        .unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "peer replacement invalidates enrollment"
    );
    runtime
        .execute(
            "UPDATE ctox_sync_host SET configuration=?1 WHERE singleton=1",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
    drop(runtime);
    assert!(gate.authorize(&request).is_ok());

    registry.with_policy(|tx| {
        tx.execute("UPDATE business_session_handoff_bindings SET target_working_copy_id='tampered-copy' WHERE binding_id=?1",
            [&first.binding_id])?;
        Ok(())
    }).unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "stored row cannot escape its native digest"
    );
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_session_handoff_bindings SET target_working_copy_id=?1 WHERE binding_id=?2",
            rusqlite::params![input.target_working_copy_id,first.binding_id])?;
        Ok(())
    }).unwrap();
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_native_guest_provider_assignments SET revision=revision+1 WHERE owner_user_id='owner'",[])?;
        Ok(())
    }).unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "provider regrant invalidates the captured policy"
    );
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_native_guest_provider_assignments SET revision=revision-1 WHERE owner_user_id='owner'",[])?;
        Ok(())
    }).unwrap();
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'",[])?;
        Ok(())
    }).unwrap();
    assert!(
        gate.authorize(&request).is_err(),
        "principal epoch fences disclosure"
    );
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_users SET capability_epoch=capability_epoch-1 WHERE user_id='owner'",[])?;
        Ok(())
    }).unwrap();
    crate::sync_host::handle_command(root, &["handoff-revoke".into(), first.binding_id.clone()])
        .unwrap();
    assert!(gate.authorize(&request).is_err());
    assert!(
        registry
            .with_policy(|tx| enrollment::enroll_source_with_conn(
                root, tx, &config, &identity, &input,
            ))
            .is_err(),
        "retry cannot resurrect a revoked binding"
    );
    let policy = super::super::super::store::open_store(root).unwrap();
    let target_rows: i64 = policy
        .query_row(
            "SELECT count(*) FROM business_session_handoff_bindings WHERE side='target'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        target_rows, 0,
        "source preparation cannot enroll the target"
    );
    drop(policy);
    let mut untrusted = request.clone();
    untrusted.binding_digest = "0".repeat(64);
    untrusted.spec.job_id = "PRIVATE_UNTRUSTED_AUDIT_PAYLOAD".into();
    untrusted.nonce = "PRIVATE_UNTRUSTED_AUDIT_NONCE".into();
    assert!(gate.authorize(&untrusted).is_err());
    let policy = super::super::super::store::open_store(root).unwrap();
    let events: Vec<(String, String)> = policy
        .prepare(
            "SELECT command_type,payload_json FROM business_events
         WHERE collection='business_session_handoff_bindings'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|(kind, _)| kind == "business_os.session_handoff.enrolled")
            .count(),
        1,
        "exact enrollment retry emits no duplicate audit"
    );
    assert_eq!(
        events
            .iter()
            .filter(|(kind, _)| kind == "business_os.session_handoff.revoked")
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|(kind, _)| kind == "business_os.session_handoff.allowed"));
    assert!(events
        .iter()
        .any(|(kind, _)| kind == "business_os.session_handoff.denied"));
    for (_, payload) in events {
        assert!(!payload.contains("PRIVATE_UNTRUSTED_AUDIT_"));
        assert!(!payload.contains("actual fixture reply"));
        assert!(!payload.contains("capture original fixture turn"));
        assert!(!payload.contains("fresh-native-disclosure-fixture"));
        assert!(serde_json::from_str::<serde_json::Value>(&payload).is_ok());
    }
}
