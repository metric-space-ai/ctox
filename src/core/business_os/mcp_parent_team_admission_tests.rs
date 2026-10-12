// Origin: CTOX
// License: AGPL-3.0-only
use super::tests::fixture;
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
use crate::business_os::project_chats::persistent_worker_binding::PersistentWorkerBinding;
const PARENT: &str = "b8958e94-4c45-4b47-b44b-03b9d8c47123";

fn command(root: &Path, id: &str, kind: &str, revision: u64) -> anyhow::Result<Value> {
    let result = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
            "id":id, "module":"ctox", "command_type":kind,
            "payload":{"project_id":"project","supervisor_thread_id":THREAD,"thread_id":PARENT,"expected_revision":revision},
            "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}
        }),
    )?;
    anyhow::ensure!(result["status"] == "completed", "{result}");
    Ok(result["result"]["binding"].clone())
}

fn context() -> McpChannelRequestContext {
    McpChannelRequestContext {
        channel: "ctox_dev_managed_mcp".into(),
        surface: "workjet".into(),
        actor: "owner".into(),
        workspace: "tenant:source-owner".into(),
        tool: TOOL.into(),
        request_id: "parent-admission".into(),
        confirmation_state: super::super::McpConfirmationState::NotRequired,
        trusted_role: Some("chef".into()),
        trusted_role_source: Some("ctox_dev_managed_mcp_token".into()),
        trusted_managed_read_scope: None,
        trusted_managed_instance_id: Some("source-instance".into()),
    }
}

#[test]
fn native_parent_source_admission_requires_exact_current_owner_issued_reference(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let policy = store::open_store(root.path())?;
    let context = context();
    // Existing Supervisor-only callers stay unchanged.
    current_project(&policy, &context, "project", THREAD)?;
    assert!(current_project(&policy, &context, "project", PARENT).is_err());
    let binding: PersistentWorkerBinding = serde_json::from_value(command(
        root.path(),
        "parent-enroll",
        "ctox.workjet.project.persistent_worker.bind",
        0,
    )?)?;
    current_project_with_team_binding(&policy, &context, "project", PARENT, Some(&binding))?;
    // Cannot substitute the Root UUID for the actual Parent, or omit provenance.
    assert!(current_project_with_team_binding(
        &policy,
        &context,
        "project",
        THREAD,
        Some(&binding)
    )
    .is_err());
    assert!(current_project(&policy, &context, "project", PARENT).is_err());
    let mut foreign = context.clone();
    foreign.actor = "foreign".into();
    assert!(current_project_with_team_binding(
        &policy,
        &foreign,
        "project",
        PARENT,
        Some(&binding)
    )
    .is_err());
    let mut stale = binding.clone();
    stale.revision += 1;
    assert!(
        current_project_with_team_binding(&policy, &context, "project", PARENT, Some(&stale))
            .is_err()
    );
    command(
        root.path(),
        "parent-revoke",
        "ctox.workjet.project.persistent_worker.revoke",
        1,
    )?;
    assert!(current_project_with_team_binding(
        &policy,
        &context,
        "project",
        PARENT,
        Some(&binding)
    )
    .is_err());
    let fresh: PersistentWorkerBinding = serde_json::from_value(command(
        root.path(),
        "parent-reenroll",
        "ctox.workjet.project.persistent_worker.bind",
        2,
    )?)?;
    current_project_with_team_binding(&policy, &context, "project", PARENT, Some(&fresh))?;
    assert!(current_project_with_team_binding(
        &policy,
        &context,
        "project",
        PARENT,
        Some(&binding)
    )
    .is_err());
    // The Owner-issued Parent does not bypass a deleted native Supervisor.
    let mut supervisor = store::outbound_load_record(&policy, "user_threads", THREAD)?.unwrap();
    supervisor["is_deleted"] = json!(true);
    store::upsert_business_record(&policy, "user_threads", THREAD, 2000, supervisor)?;
    assert!(
        current_project_with_team_binding(&policy, &context, "project", PARENT, Some(&fresh))
            .is_err()
    );
    Ok(())
}
