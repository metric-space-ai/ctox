// Origin: CTOX
// License: AGPL-3.0-only

use super::super::store::{
    issue_business_os_capability_token_for_managed_user, load_rxdb_collection_record,
};
use super::super::store_projections::tests::create_repair_rxdb_tables;
use super::accept_rxdb_business_command_with_guest_runtime;
use super::*;
use serde_json::json;
use tempfile::tempdir;

fn actor(id: &str, role: &str) -> serde_json::Value {
    json!({
        "id": id,
        "role": role,
        "is_admin": role == "admin" || role == "chef",
        "display_name": id,
        "email": "",
        "login": ""
    })
}

fn document(
    command_id: &str,
    command_type: &str,
    payload: serde_json::Value,
    actor: serde_json::Value,
    token: Option<&str>,
) -> serde_json::Value {
    let mut client_context = json!({ "actor": actor });
    if let Some(token) = token {
        client_context["capability_token"] = json!(token);
    }
    json!({
        "id": command_id,
        "command_id": command_id,
        "module": "ctox",
        "command_type": command_type,
        "record_id": "guest-a",
        "payload": payload,
        "client_context": client_context
    })
}

#[test]
fn guest_observe_without_owner_fails_closed_and_writes_failed_receipt() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    let command_id = "cmd-guest-observe-unregistered";
    let outcome = accept_rxdb_business_command(
        root.path(),
        document(
            command_id,
            "ctox.guest.observe",
            json!({"guest_id": "guest-a"}),
            actor("owner-1", "admin"),
            None,
        ),
    );
    let error = outcome.expect_err("unregistered guest owner must fail closed");
    assert!(
        error
            .to_string()
            .contains("guest command owner is not registered for this runtime"),
        "{error}"
    );
    let stored = load_rxdb_collection_record(root.path(), "business_commands", command_id)?
        .expect("failed guest command must persist a receipt");
    assert_eq!(stored["status"], "failed");
    assert_eq!(stored["command_id"], command_id);
    assert_eq!(stored["command_type"], "ctox.guest.observe");
    let result = &stored["result"];
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|value| value.contains("guest command owner is not registered")),
        "{result}"
    );
    assert!(result.get("png").is_none());
    assert!(!result.to_string().contains("png"));
    Ok(())
}

#[test]
fn guest_input_user_is_denied_before_dispatch() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    let issued_at_ms = now_ms() as i64;
    let (token, _) = issue_business_os_capability_token_for_managed_user(
        root.path(),
        "guest-user",
        "Guest User",
        "user",
        issued_at_ms,
    )?;
    let result = accept_rxdb_business_command_with_origin(
        root.path(),
        document(
            "cmd-guest-input-user",
            "ctox.guest.input",
            json!({
                "guest_id": "guest-a",
                "frame_id": "frame-current",
                "input": {"kind": "click", "x": 1, "y": 1, "button": "left"}
            }),
            actor("guest-user", "user"),
            Some(&token),
        ),
        CommandOrigin::ReplicatedPeer,
    )?;
    assert!(
        result.to_string().contains("denied"),
        "user must be policy-denied without guest ownership: {result}"
    );
    Ok(())
}

#[test]
fn guest_observe_rejects_unknown_fields_and_arbitrary_commands() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    for (command_id, command_type, payload) in [
        (
            "cmd-guest-actor",
            "ctox.guest.observe",
            json!({"guest_id": "guest-a", "actor": "admin"}),
        ),
        (
            "cmd-guest-shell",
            "ctox.guest.input",
            json!({
                "guest_id": "guest-a",
                "frame_id": "frame-current",
                "input": {"kind": "shell", "command": "id"}
            }),
        ),
    ] {
        let error = accept_rxdb_business_command(
            root.path(),
            document(
                command_id,
                command_type,
                payload,
                actor("owner-1", "admin"),
                None,
            ),
        )
        .expect_err(command_id);
        let message = error.to_string();
        assert!(
            message.contains("invalid ctox.guest")
                || message.contains("unknown field")
                || message.contains("guest command"),
            "{command_id}: {message}"
        );
    }
    Ok(())
}

#[test]
fn guest_command_replay_keeps_failed_receipt_correlated() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    let command_id = "cmd-guest-observe-replay";
    let request = document(
        command_id,
        "ctox.guest.observe",
        json!({"guest_id": "guest-a"}),
        actor("owner-1", "admin"),
        None,
    );
    let first = accept_rxdb_business_command(root.path(), request.clone());
    assert!(first.is_err(), "{first:?}");
    let replay = accept_rxdb_business_command(root.path(), request)?;
    assert_eq!(replay["already_accepted"], true);
    assert_eq!(replay["command_id"], command_id);
    assert_eq!(replay["status"], "failed");
    assert!(
        replay["result"]["error"]
            .as_str()
            .is_some_and(|value| value.contains("guest command owner is not registered")),
        "{replay}"
    );
    Ok(())
}

#[test]
fn registered_guest_owner_observe_and_input_use_command_plane_receipts() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    let injection = super::super::guest_commands::test_guest_runtime(false);
    let observe = accept_rxdb_business_command_with_guest_runtime(
        root.path(),
        document(
            "cmd-guest-observe-registered",
            "ctox.guest.observe",
            json!({"guest_id": "guest-a"}),
            actor("owner", "admin"),
            None,
        ),
        CommandOrigin::TrustedLocal,
        injection.clone(),
    )?;
    assert_eq!(observe["ok"], true);
    assert_eq!(observe["status"], "completed");
    assert_eq!(observe["command_id"], "cmd-guest-observe-registered");
    assert_eq!(observe["result"]["outcome"], "observation_published");
    assert_eq!(observe["result"]["frame_id"], "frame-current");
    assert_eq!(observe["result"]["guest_id"], "guest-a");
    assert!(observe["result"].get("png").is_none());
    assert!(!observe["result"].to_string().contains("png"));

    let applied = accept_rxdb_business_command_with_guest_runtime(
        root.path(),
        document(
            "cmd-guest-input-registered",
            "ctox.guest.input",
            json!({
                "guest_id": "guest-a",
                "frame_id": "frame-current",
                "input": {"kind": "click", "x": 2, "y": 3, "button": "left"}
            }),
            actor("owner", "admin"),
            None,
        ),
        CommandOrigin::TrustedLocal,
        injection,
    )?;
    assert_eq!(applied["ok"], true);
    assert_eq!(applied["status"], "completed");
    assert_eq!(applied["command_id"], "cmd-guest-input-registered");
    assert_eq!(applied["result"]["outcome"], "input_applied");
    assert_eq!(
        applied["result"]["command_id"],
        "cmd-guest-input-registered"
    );
    assert!(applied["result"].get("png").is_none());
    Ok(())
}

#[test]
fn public_intake_wrapper_stays_unregistered_and_cannot_execute() -> anyhow::Result<()> {
    let root = tempdir()?;
    drop(create_repair_rxdb_tables(root.path())?);
    let error = accept_rxdb_business_command_with_origin(
        root.path(),
        document(
            "cmd-guest-public-unregistered",
            "ctox.guest.observe",
            json!({"guest_id": "guest-a"}),
            actor("owner-1", "admin"),
            None,
        ),
        CommandOrigin::TrustedLocal,
    )
    .expect_err("public intake must pass Unregistered");
    assert!(
        error
            .to_string()
            .contains("guest command owner is not registered for this runtime"),
        "{error}"
    );
    Ok(())
}
