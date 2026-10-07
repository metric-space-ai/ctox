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
    let mut request = SessionHandoffGateRequest {
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
    let captured_epoch: i64 = registry
        .with_policy(|tx| {
            Ok(tx.query_row(
                "SELECT capability_epoch FROM business_users WHERE user_id='owner'",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    registry.with_policy(|tx| {
        tx.execute(
            "INSERT INTO business_permission_grants
            (grant_id,subject_type,subject_id,permission,scope_type,scope_id,active,created_by,created_at_ms,updated_at_ms)
            VALUES ('native-handoff-grant','user','owner',?1,'session_handoff',?2,1,'owner',1,1)",
            rusqlite::params![
                super::super::super::policy::BusinessOsPermission::SessionHandoffDisclose.as_str(),
                first.binding_id,
            ],
        )?;
        Ok(())
    }).unwrap();
    assert_eq!(
        gate.authorize(&request).unwrap_err().reason_code,
        "source_authority_changed",
        "grant mutation bumps the principal epoch and cannot authorize an old capture"
    );
    assert!(
        crate::sync_host::with_current_signing_identity(root, |key| {
            enrollment::reauthorize_source(root, &config, key, &first.binding_id)
        })
        .is_err(),
        "stale provider/workspace assignments cannot be renewed implicitly"
    );
    let (computer,copy,workspace,profile,project): (String,String,String,String,String) = registry.with_policy(|tx| {
        Ok(tx.query_row(
            "SELECT p.computer_id,w.working_copy_id,w.native_workspace,j.worker_profile_id,j.project_id
             FROM business_native_source_journals j
             JOIN business_native_guest_provider_assignments p
             ON p.owner_user_id=j.owner_user_id AND p.worker_profile_id=j.worker_profile_id
             JOIN business_native_guest_workspace_assignments w
             ON w.owner_user_id=j.owner_user_id AND w.worker_profile_id=j.worker_profile_id AND w.project_id=j.project_id
             WHERE j.capture_id=?1",
            [&receipt.capture_id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        )?)
    }).unwrap();
    let regrant = || {
        crate::business_os::configure_native_guest_assignments(
            root,
            &computer,
            &[crate::business_os::ProviderAssignmentInput {
                owner_user_id: "owner".into(),
                worker_profile_id: profile.clone(),
                gateway_account_id: spec.gateway_account_id.clone(),
                model_id: spec.model_id.clone(),
            }],
            &[crate::business_os::WorkspaceAssignmentInput {
                owner_user_id: "owner".into(),
                worker_profile_id: profile.clone(),
                project_id: project.clone(),
                working_copy_id: copy.clone(),
                native_workspace: workspace.clone().into(),
            }],
        )
        .unwrap()
    };
    regrant();
    let role: String = registry
        .with_policy(|tx| {
            Ok(tx.query_row(
                "SELECT role FROM business_users WHERE user_id='owner'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let foreign_role = if role == "member" { "owner" } else { "member" };
    registry
        .with_policy(|tx| {
            tx.execute(
                "UPDATE business_users SET role=?1 WHERE user_id='owner'",
                [foreign_role],
            )?;
            Ok(())
        })
        .unwrap();
    regrant();
    assert!(
        crate::sync_host::with_current_signing_identity(root, |key| {
            enrollment::reauthorize_source(root, &config, key, &first.binding_id)
        })
        .is_err(),
        "current grants cannot mask a changed native role"
    );
    registry
        .with_policy(|tx| {
            tx.execute(
                "UPDATE business_users SET role=?1 WHERE user_id='owner'",
                [&role],
            )?;
            Ok(())
        })
        .unwrap();
    regrant();
    let renew = || {
        crate::sync_host::with_current_signing_identity(root, |key| {
            enrollment::reauthorize_source(root, &config, key, &first.binding_id)
        })
    };
    let snapshot: String = registry
        .with_policy(|tx| {
            Ok(tx.query_row(
            "SELECT snapshot_json FROM business_native_source_policy_snapshots WHERE capture_id=?1",
            [&receipt.capture_id], |r| r.get(0),
        )?)
        })
        .unwrap();
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_native_source_policy_snapshots SET snapshot_json='[]' WHERE capture_id=?1",
            [&receipt.capture_id])?;
        Ok(())
    }).unwrap();
    assert!(
        renew().is_err(),
        "historical policy proof cannot be fabricated"
    );
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_native_source_policy_snapshots SET snapshot_json=?1 WHERE capture_id=?2",
            rusqlite::params![snapshot,receipt.capture_id])?;
        tx.execute("UPDATE business_permission_grants SET active=0 WHERE grant_id='native-handoff-grant'",[])?;
        Ok(())
    }).unwrap();
    regrant();
    assert_eq!(
        renew().unwrap_err().to_string(),
        "exact source disclosure grant required"
    );
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_permission_grants SET active=1 WHERE grant_id='native-handoff-grant'",[])?;
        Ok(())
    }).unwrap();
    regrant();
    crate::business_os::configure_native_guest_assignments(
        root,
        &computer,
        &[crate::business_os::ProviderAssignmentInput {
            owner_user_id: "owner".into(),
            worker_profile_id: profile.clone(),
            gateway_account_id: "foreign-native-account".into(),
            model_id: spec.model_id.clone(),
        }],
        &[crate::business_os::WorkspaceAssignmentInput {
            owner_user_id: "owner".into(),
            worker_profile_id: profile.clone(),
            project_id: project.clone(),
            working_copy_id: copy.clone(),
            native_workspace: workspace.clone().into(),
        }],
    )
    .unwrap();
    assert!(
        renew().is_err(),
        "current regrant cannot move the capture to another account"
    );
    regrant();
    let foreign_workspace = tempfile::tempdir_in(root).unwrap();
    crate::business_os::configure_native_guest_assignments(
        root,
        &computer,
        &[crate::business_os::ProviderAssignmentInput {
            owner_user_id: "owner".into(),
            worker_profile_id: profile.clone(),
            gateway_account_id: spec.gateway_account_id.clone(),
            model_id: spec.model_id.clone(),
        }],
        &[crate::business_os::WorkspaceAssignmentInput {
            owner_user_id: "owner".into(),
            worker_profile_id: profile.clone(),
            project_id: project.clone(),
            working_copy_id: copy.clone(),
            native_workspace: foreign_workspace.path().into(),
        }],
    )
    .unwrap();
    assert!(
        renew().is_err(),
        "same working-copy label cannot substitute a different native directory"
    );
    regrant();
    registry
        .with_policy(|tx| {
            tx.execute_batch(
                "CREATE TRIGGER fail_source_renewal BEFORE INSERT ON business_events
            WHEN NEW.command_type='business_os.session_handoff.reauthorized'
            BEGIN SELECT RAISE(ABORT,'renewal audit fixture failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    assert!(
        renew().is_err(),
        "failed renewal audit cannot commit a new authorization"
    );
    registry.with_policy(|tx| {
        tx.execute_batch("DROP TRIGGER fail_source_renewal")?;
        let revision: i64 = tx.query_row(
            "SELECT revision FROM business_session_handoff_bindings WHERE binding_id=?1",
            [&first.binding_id],|r| r.get(0),
        )?;
        assert_eq!(revision,1,"failed audit rolls back binding revision");
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM business_native_source_handoff_authorizations WHERE binding_id=?1",
            [&first.binding_id],|r| r.get(0),
        )?;
        assert_eq!(count,0,"failed audit rolls back authorization provenance");
        Ok(())
    }).unwrap();
    let original_digest = request.binding_digest.clone();
    let renewed = crate::sync_host::with_current_signing_identity(root, |key| {
        enrollment::reauthorize_source(root, &config, key, &first.binding_id)
    })
    .unwrap();
    assert_eq!(renewed.binding_id, first.binding_id);
    assert_eq!(renewed.binding_revision, 2);
    assert_ne!(renewed.binding_digest, original_digest);
    assert!(
        gate.authorize(&request).is_err(),
        "old binding digest cannot mint after renewal"
    );
    request.binding_digest = renewed.binding_digest.clone();
    crate::sync_host::handle_command(
        root,
        &[
            "handoff-reauthorize-source".into(),
            first.binding_id.clone(),
        ],
    )
    .unwrap();
    let retry = crate::sync_host::with_current_signing_identity(root, |key| {
        enrollment::reauthorize_source(root, &config, key, &first.binding_id)
    })
    .unwrap();
    assert_eq!(
        retry.binding_revision, 2,
        "reauthorization retry is idempotent"
    );
    assert_eq!(retry.binding_digest, renewed.binding_digest);
    let current_epoch: i64 = registry
        .with_policy(|tx| {
            Ok(tx.query_row(
                "SELECT capability_epoch FROM business_users WHERE user_id='owner'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(
        current_epoch > captured_epoch,
        "production never lowers the principal epoch"
    );
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
    assert_eq!(
        events
            .iter()
            .filter(|(kind, _)| kind == "business_os.session_handoff.reauthorized")
            .count(),
        1,
        "retry and failed renewal audit cannot create duplicate committed events"
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
