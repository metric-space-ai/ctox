//! Actual command/lease input; provider and quorum remain component fixtures.
use super::*;

#[test]
fn native_guest_queue_selection_uses_real_signed_command_and_crew_attempt() {
    let (root, registry, assignment) = fixture();
    let (_worker, facts, token) = worker_store(root.path());
    let context = facts.command_provenance.as_ref().unwrap();
    assert_eq!(
        registry
            .select_command_context(root.path(), context)
            .unwrap(),
        Some(assignment.destination.guest_id.clone())
    );
    // This fixture owns no native peer. Denial precedes model/harness startup.
    let result = crate::execution::agent::turn_loop::PersistentSession::start_native_guest_with_business_os_mcp(
        root.path(), &Default::default(), &token, None, registry.clone(),
        &assignment.destination.guest_id,
    );
    let error = result
        .err()
        .expect("missing native peer must deny before startup");
    assert!(
        error
            .to_string()
            .contains("native frame transport is not attached"),
        "{error:#}"
    );
    let foreign = tempfile::tempdir().unwrap();
    assert!(registry
        .select_command_context(foreign.path(), context)
        .is_err());
}

#[test]
fn native_guest_command_scope_changes_after_selection_never_publish() {
    for mutation in [
        "UPDATE business_command_aggregates SET intent_json=json_set(intent_json,'$.payload.thread_id','foreign-chat')",
        "UPDATE business_command_aggregates SET intent_json=json_set(intent_json,'$.payload.project_id','foreign-project')",
        "UPDATE business_command_aggregates SET intent_json=json_set(intent_json,'$.payload.worker_profile_id','foreign-profile')",
        "UPDATE business_command_aggregates SET intent_json=json_set(intent_json,'$.payload.external_executor',json('{\"executor_id\":\"other\"}'))",
        "UPDATE business_command_aggregates SET payload_hash='replaced-payload'",
        "UPDATE business_command_aggregates SET execution_phase='terminal'",
        "UPDATE crew_attempts SET member_id='crew-nori'",
        "UPDATE crew_attempts SET finalized_at='2000-01-01T00:00:00Z'",
        "UPDATE communication_routing_state SET lease_worker_id='foreign-worker'",
    ] {
        let (root, registry, assignment) = fixture();
        let (mut worker, facts, _) = worker_store(root.path());
        assert!(registry.select_command_context(
            root.path(), facts.command_provenance.as_ref().unwrap()
        ).unwrap().is_some());
        // ctox-allow-direct-state-write: isolated injected canonical authority mutation
        worker.execute(mutation, []).unwrap();
        let tx = worker.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        let resolver = NativeGuestAdmissionResolver {
            registry, guest_id: assignment.destination.guest_id,
        };
        let mut published = false;
        assert!(resolver.with_current_destination(
            &tx, root.path(), &facts, None, &mut |_| { published = true; Ok(()) },
        ).is_err(), "{mutation}");
        assert!(!published, "{mutation}");
    }
}

#[test]
fn native_guest_queue_selection_preserves_external_execution_owner() {
    let (root, registry, _) = fixture();
    let (worker, facts, _) = worker_store(root.path());
    // ctox-allow-direct-state-write: isolated canonical external owner fixture
    worker.execute(
        "UPDATE business_command_aggregates SET intent_json=json_set(intent_json,'$.payload.external_executor',json('{\"executor_id\":\"other\"}'))", [],
    ).unwrap();
    assert!(registry
        .select_command_context(root.path(), facts.command_provenance.as_ref().unwrap(),)
        .unwrap()
        .is_none());
}

#[test]
fn native_guest_profile_reassignment_denies_current_command_callback() {
    let (root, registry, assignment) = fixture();
    let (mut worker, facts, _) = worker_store(root.path());
    assert!(registry
        .select_command_context(root.path(), facts.command_provenance.as_ref().unwrap())
        .unwrap()
        .is_some());
    let policy = super::super::super::store::open_store(root.path()).unwrap();
    let profile = super::super::super::worker_profile_bindings::binding_id("owner", "profile");
    put(
        &policy,
        "workjet_worker_profile_bindings",
        &profile,
        json!({"owner_user_id":"owner","worker_profile_id":"profile","computer_id":"computer",
            "crew_member_id":"crew-nori","status":"active","is_deleted":false}),
    );
    let tx = worker.transaction().unwrap();
    let resolver = NativeGuestAdmissionResolver {
        registry,
        guest_id: assignment.destination.guest_id,
    };
    let mut published = false;
    assert!(resolver
        .with_current_destination(&tx, root.path(), &facts, None, &mut |_| {
            published = true;
            Ok(())
        },)
        .is_err());
    assert!(!published);
}
