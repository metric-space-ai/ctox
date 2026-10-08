// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::{command_plane, store, store_workjet_projects::tests::*};
use serde_json::{json, Value};
const OWNER: &str = "196a89ba-ee86-4413-885c-04ca60e6f291";
const ALIAS: &str = "michael.welsch@metric-space.ai";
const FOREIGN: &str = "foreign@example.org";
fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    drop(crate::business_os::store_projections::tests::create_repair_rxdb_tables(root.path())?);
    create_workjet_rxdb_projection_tables(root.path())?;
    let now = 1_791_395_000_000;
    // These calls represent the authenticated managed control plane, not a
    // replicated caller-provided email/name/profile.
    let _ = store::issue_business_os_capability_token_for_managed_user_with_email(
        root.path(),
        OWNER,
        Some(ALIAS),
        "Michael",
        "chef",
        now,
    )?;
    let _ = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        ALIAS,
        "Michael",
        "admin",
        now,
    )?;
    let _ = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        FOREIGN,
        "Michael",
        "admin",
        now,
    )?;
    for n in 0..16 {
        let document = json!({"id":format!("p{n}"),"name":format!("Project {n}"),"owner_user_id":OWNER,"status":if n<12 {"active"} else {"archived"},"created_at_ms":1,"updated_at_ms":2,"is_deleted":false});
        store::upsert_business_record(
            &store::open_store(root.path())?,
            "workjet_projects",
            &format!("p{n}"),
            2,
            document,
        )?;
    }
    Ok(root)
}
fn list(root: &std::path::Path, actor: &str, id: &str, extra: Value) -> anyhow::Result<Value> {
    let mut context = json!({"actor":{"id":actor,"role":"admin","display_name":"Michael"}});
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            context[key] = value.clone();
        }
    }
    command_plane::accept_rxdb_business_command(
        root,
        json!({"id":id,"module":"ctox","command_type":"ctox.workjet.project.list","payload":{"limit":100},"client_context":context}),
    )
}
#[test]
fn verified_alias_lists_twelve_without_rewriting_any_project_owner() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = store::open_store(root.path())?;
    let before = store::outbound_load_records_by_string_field(
        &conn,
        "workjet_projects",
        "owner_user_id",
        OWNER,
    )?;
    let result = list(root.path(), ALIAS, "alias-list", json!({}))?;
    assert_eq!(result["status"], "completed");
    assert_eq!(result["result"]["count"], 12);
    assert_eq!(result["result"]["owner_user_id"], OWNER);
    assert_eq!(result["result"]["truncated"], false);
    assert_eq!(
        store::outbound_load_records_by_string_field(
            &conn,
            "workjet_projects",
            "owner_user_id",
            OWNER
        )?,
        before
    );
    assert_eq!(
        list(
            root.path(),
            FOREIGN,
            "foreign-list",
            json!({"email":ALIAS,"canonical_owner_user_id":OWNER} )
        )?["result"]["count"],
        0
    );
    assert_eq!(
        list(root.path(), OWNER, "owner-list", json!({}))?["result"]["count"],
        12
    );
    Ok(())
}
#[test]
fn claimed_email_name_or_profile_does_not_enroll_an_alias() -> anyhow::Result<()> {
    let root = fixture()?;
    let conn = store::open_store(root.path())?;
    conn.execute(
        "UPDATE business_users SET profile_json=?1 WHERE user_id=?2",
        params![
            json!({"email":ALIAS,"canonical_user_id":OWNER}).to_string(),
            FOREIGN
        ],
    )?;
    assert_eq!(owner(root.path(), FOREIGN)?, FOREIGN);
    assert_eq!(
        list(
            root.path(),
            FOREIGN,
            "spoof",
            json!({"email":ALIAS,"actor":{"id":FOREIGN,"role":"chef","email":ALIAS}})
        )?["result"]["count"],
        0
    );
    Ok(())
}
#[test]
fn revoked_alias_or_canonical_account_cannot_keep_project_authority() -> anyhow::Result<()> {
    for user in [OWNER, ALIAS] {
        let root = fixture()?;
        store::open_store(root.path())?.execute(
            "UPDATE business_users SET active=0 WHERE user_id=?1",
            [user],
        )?;
        assert!(owner(root.path(), ALIAS).is_err());
        let result = list(root.path(), ALIAS, "revoked", json!({}));
        assert!(result.is_err() || result.as_ref().is_ok_and(|v| v["status"] == "failed"));
    }
    Ok(())
}
#[test]
fn conflicting_managed_identity_claim_fails_closed_and_keeps_original_alias() -> anyhow::Result<()>
{
    let root = fixture()?;
    assert!(
        store::issue_business_os_capability_token_for_managed_user_with_email(
            root.path(),
            "aa6d596d-dbab-4dc2-bb6b-0e2ef6825c84",
            Some(ALIAS),
            "Michael",
            "chef",
            1_791_395_000_001
        )
        .is_err()
    );
    assert_eq!(owner(root.path(), ALIAS)?, OWNER);
    Ok(())
}
#[test]
fn managed_email_change_removes_the_previous_alias_without_changing_projects() -> anyhow::Result<()>
{
    let root = fixture()?;
    let _ = store::issue_business_os_capability_token_for_managed_user_with_email(
        root.path(),
        OWNER,
        Some("new@example.org"),
        "Michael",
        "chef",
        1_791_395_000_001,
    )?;
    assert_eq!(owner(root.path(), ALIAS)?, ALIAS);
    assert_eq!(
        list(root.path(), ALIAS, "old-email", json!({}))?["result"]["count"],
        0
    );
    assert_eq!(
        list(root.path(), OWNER, "unchanged", json!({}))?["result"]["count"],
        12
    );
    Ok(())
}
#[test]
fn replicated_foreign_capability_cannot_claim_the_alias_actor() -> anyhow::Result<()> {
    let root = fixture()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        FOREIGN,
        "Michael",
        "admin",
        now,
    )?;
    let result = store::accept_rxdb_business_command_with_origin(
        root.path(),
        json!({"id":"foreign-replicated","module":"ctox","command_type":"ctox.workjet.project.list","payload":{},"client_context":{"actor":{"id":ALIAS,"role":"chef","email":ALIAS},"capability_token":token}}),
        store::CommandOrigin::ReplicatedPeer,
    )?;
    assert_eq!(result["status"], "completed");
    assert_eq!(result["result"]["count"], 0);
    assert_eq!(result["result"]["owner_user_id"], FOREIGN);
    Ok(())
}

#[test]
fn alias_project_update_keeps_canonical_owner_and_private_chat_visibility() -> anyhow::Result<()> {
    let root = fixture()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        ALIAS,
        "Michael",
        "admin",
        now,
    )?;
    let result = store::accept_rxdb_business_command_with_origin(
        root.path(),
        json!({"id":"alias-edit","module":"ctox","command_type":"ctox.workjet.project.upsert","record_id":"p0","payload":{"project_id":"p0","name":"Updated","info":{"summary":"Saved via alias"}},"client_context":{"actor":{"id":ALIAS,"role":"admin"},"capability_token":token}}),
        store::CommandOrigin::ReplicatedPeer,
    )?;
    assert_eq!(result["status"], "completed");
    assert_eq!(result["result"]["owner_user_id"], OWNER);
    assert_eq!(result["result"]["project"]["owner_user_id"], OWNER);
    assert_eq!(result["result"]["project"]["info"]["summary"], "Saved via alias");
    let chat_id = result["result"]["group_chat_id"]
        .as_str()
        .context("group chat")?;
    let conn = store::open_store(root.path())?;
    let chat =
        store::outbound_load_record(&conn, "workjet_project_chats", chat_id)?.context("chat")?;
    assert_eq!(chat["owner_user_id"], OWNER);
    assert_eq!(
        crate::business_os::project_chats::document_visible_to_actor(
            root.path(),
            "workjet_project_chats",
            &chat,
            ALIAS
        ),
        Some(true)
    );
    assert_eq!(
        crate::business_os::project_chats::document_visible_to_actor(
            root.path(),
            "workjet_project_chats",
            &chat,
            FOREIGN
        ),
        Some(false)
    );
    let (foreign_token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        FOREIGN,
        "Michael",
        "admin",
        now,
    )?;
    let denied = store::accept_rxdb_business_command_with_origin(
        root.path(),
        json!({"id":"foreign-alias-edit","module":"ctox","command_type":"ctox.workjet.project.upsert","record_id":"p0","payload":{"project_id":"p0","name":"Forged"},"client_context":{"actor":{"id":ALIAS,"role":"chef"},"capability_token":foreign_token}}),
        store::CommandOrigin::ReplicatedPeer,
    );
    assert!(denied.is_err() || denied.as_ref().is_ok_and(|v| v["status"] == "failed"));
    let saved =
        store::outbound_load_record(&conn, "workjet_projects", "p0")?.context("saved project")?;
    assert_eq!(saved["owner_user_id"], OWNER);
    assert_eq!(saved["name"], "Updated");
    Ok(())
}

#[test]
fn alias_project_chat_ensure_uses_current_native_identity_and_foreign_is_denied(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let ensure_chat = |actor: &str, id: &str| {
        command_plane::accept_rxdb_business_command(
            root.path(),
            json!({"id":id,"module":"ctox","command_type":"ctox.workjet.project.chat.ensure","record_id":"p0","payload":{"project_id":"p0"},"client_context":{"actor":{"id":actor,"role":"admin"}}}),
        )
    };
    let admitted = ensure_chat(ALIAS, "alias-chat")?;
    assert_eq!(admitted["status"], "completed");
    let denied = ensure_chat(FOREIGN, "foreign-chat");
    assert!(denied.is_err() || denied.as_ref().is_ok_and(|v| v["status"] == "failed"));
    Ok(())
}
