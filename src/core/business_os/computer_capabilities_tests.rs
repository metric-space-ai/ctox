// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use crate::business_os::computer_capabilities::{
    load_registered_computer_capabilities, select_build_target, BuildAvailability,
};
use crate::business_os::store::{load_rxdb_collection_record, CommandOrigin};
use serde_json::json;
use tempfile::tempdir;

fn command(payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: None,
        module: "ctox".to_owned(),
        command_type: "ctox.workjet.computer.assign".to_owned(),
        record_id: None,
        payload,
        client_context: json!({}),
        origin: CommandOrigin::TrustedLocal,
    }
}

fn build(slots: u16) -> Value {
    json!({"kind":"build", "ssh_endpoint_ref":"gpu-endpoint-opaque", "slots":slots,
        "jobs":6, "lane_root":"/mnt/nvme1/build-lane", "disk_floor_gib":60,
        "toolchains":["rust-1.93"]})
}

fn payload(id: &str, config: Value) -> Value {
    json!({"computer_id":id, "display_name":"Build computer", "hosting_mode":"self_hosted",
        "capabilities":["codex"], "capability_config":config})
}

fn register(root: &Path, id: &str, config: Value) -> anyhow::Result<Value> {
    handle_workjet_computer_store_command(
        root,
        &command(payload(id, config)),
        "owner-1",
        None,
        "chef",
    )
}

#[test]
fn capability_assignment_requires_native_owner_admin_and_never_transfers_ownership(
) -> anyhow::Result<()> {
    let root = tempdir()?;
    let request = command(payload("opaque-build-1", json!([build(3)])));
    for role in ["user", "founder", "team", ""] {
        assert!(handle_workjet_computer_store_command(
            root.path(),
            &request,
            "owner-1",
            None,
            role
        )
        .is_err());
    }
    assert!(outbound_load_record(
        &open_store(root.path())?,
        COMPUTERS_COLLECTION,
        "opaque-build-1"
    )?
    .is_none());
    handle_workjet_computer_store_command(root.path(), &request, "owner-1", None, "admin")?;
    assert!(
        handle_workjet_computer_store_command(root.path(), &request, "owner-2", None, "chef")
            .is_err()
    );
    assert_eq!(
        load_registered_computer_capabilities(root.path(), "owner-1")?.len(),
        1
    );
    assert!(load_registered_computer_capabilities(root.path(), "owner-2")?.is_empty());
    Ok(())
}

#[test]
fn legacy_refresh_preserves_typed_build_settings_and_is_idempotent() -> anyhow::Result<()> {
    let root = tempdir()?;
    let first = register(root.path(), "opaque-build-1", json!([build(3)]))?;
    let legacy = command(
        json!({"computer_id":"opaque-build-1", "display_name":"Build computer",
        "hosting_mode":"self_hosted", "capabilities":["codex"]}),
    );
    let refreshed =
        handle_workjet_computer_store_command(root.path(), &legacy, "owner-1", None, "chef")?;
    assert_eq!(refreshed["computer"]["_rev"], first["computer"]["_rev"]);
    assert_eq!(
        refreshed["computer"]["capability_config"],
        first["computer"]["capability_config"]
    );
    assert_eq!(
        refreshed["computer"]["capabilities"],
        json!(["build", "codex"])
    );
    let removed = register(root.path(), "opaque-build-1", json!([]))?;
    assert_eq!(removed["computer"]["capabilities"], json!(["codex"]));
    assert!(
        load_registered_computer_capabilities(root.path(), "owner-1")?[0]
            .capabilities
            .is_empty()
    );
    Ok(())
}

#[test]
fn agentless_nas_is_storage_only_and_projects_compatible_capability_names() -> anyhow::Result<()> {
    let root = tempdir()?;
    super::tests::create_workjet_computer_rxdb_projection_table(root.path())?;
    let mut nas = payload(
        "opaque-nas-1",
        json!([{"kind":"storage", "endpoint_ref":"nas-transfer-1",
        "protocol":"ssh", "root":"/volume1/artifacts", "quota_gib":1024, "purposes":["artifacts","exchange"]}]),
    );
    nas["agentless"] = json!(true);
    nas["capabilities"] = json!([]);
    let assigned = handle_workjet_computer_store_command(
        root.path(),
        &command(nas.clone()),
        "owner-1",
        None,
        "chef",
    )?;
    assert_eq!(assigned["computer"]["agentless"], true);
    assert!(require_assigned_workjet_computer(
        &open_store(root.path())?,
        "opaque-nas-1",
        "owner-1"
    )
    .is_err());
    let projected =
        load_rxdb_collection_record(root.path(), COMPUTERS_COLLECTION, "opaque-nas-1")?.unwrap();
    assert_eq!(projected["capabilities"], json!(["storage"]));
    assert!(projected.get("capability_config").is_none());
    assert!(projected.get("agentless").is_none());
    nas.as_object_mut().unwrap().remove("capability_config");
    nas.as_object_mut().unwrap().remove("agentless");
    let refreshed =
        handle_workjet_computer_store_command(root.path(), &command(nas), "owner-1", None, "chef")?;
    assert_eq!(refreshed["computer"]["_rev"], assigned["computer"]["_rev"]);
    assert_eq!(refreshed["computer"]["agentless"], true);
    Ok(())
}

#[test]
fn invalid_operational_updates_leave_existing_configuration_intact() -> anyhow::Result<()> {
    let root = tempdir()?;
    let original = register(root.path(), "opaque-build-1", json!([build(3)]))?;
    for config in [json!([build(0)]), json!([build(3), build(3)])] {
        assert!(register(root.path(), "opaque-build-1", config).is_err());
        let record = outbound_load_record(
            &open_store(root.path())?,
            COMPUTERS_COLLECTION,
            "opaque-build-1",
        )?
        .unwrap();
        assert_eq!(record["_rev"], original["computer"]["_rev"]);
    }
    let mut agentless_build = payload("opaque-build-1", json!([build(3)]));
    agentless_build["agentless"] = json!(true);
    assert!(handle_workjet_computer_store_command(
        root.path(),
        &command(agentless_build),
        "owner-1",
        None,
        "chef"
    )
    .is_err());
    Ok(())
}

#[test]
fn build_selection_uses_fresh_native_capacity_toolchain_and_assignment() -> anyhow::Result<()> {
    let root = tempdir()?;
    register(root.path(), "opaque-gpu3", json!([build(3)]))?;
    register(root.path(), "opaque-gpu4", json!([build(2)]))?;
    let computers = load_registered_computer_capabilities(root.path(), "owner-1")?;
    let mut free = vec![
        BuildAvailability {
            computer_id: "opaque-gpu3".to_owned(),
            free_slots: 1,
            observed_at_ms: 90_000,
        },
        BuildAvailability {
            computer_id: "opaque-gpu4".to_owned(),
            free_slots: 2,
            observed_at_ms: 90_000,
        },
    ];
    assert_eq!(
        select_build_target(&computers, &free, "rust-1.93", 100_000)?
            .unwrap()
            .computer_id,
        "opaque-gpu4"
    );
    assert!(select_build_target(&computers, &free, "metal", 100_000)?.is_none());
    assert!(select_build_target(&computers, &free, "rust-1.93", 130_001)?.is_none());
    free[1].free_slots = 3; // impossible for a two-slot computer
    assert_eq!(
        select_build_target(&computers, &free, "rust-1.93", 100_000)?
            .unwrap()
            .computer_id,
        "opaque-gpu3"
    );
    free[0].observed_at_ms = 100_001; // a future report is not fresh evidence
    assert!(select_build_target(&computers, &free, "rust-1.93", 100_000)?.is_none());
    let unassign = BusinessCommand {
        command_type: "ctox.workjet.computer.unassign".to_owned(),
        payload: json!({"computer_id":"opaque-gpu4"}),
        ..command(json!({}))
    };
    handle_workjet_computer_store_command(root.path(), &unassign, "owner-1", None, "chef")?;
    assert_eq!(
        load_registered_computer_capabilities(root.path(), "owner-1")?.len(),
        1
    );
    Ok(())
}
