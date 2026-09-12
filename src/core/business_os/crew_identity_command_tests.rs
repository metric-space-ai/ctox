use super::*;
use crate::business_os::store;
use crate::mission::channels;
use serde_json::json;

#[test]
fn crew_controls_enforce_roles_even_with_explicit_grants_and_replay_receipts() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let task = channels::create_queue_task(
        root.path(),
        channels::QueueTaskCreateRequest {
            title: "Crew controls".into(),
            prompt: "Read-only fixture".into(),
            thread_key: "crew-control-fixture".into(),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    let core = rusqlite::Connection::open(crate::paths::core_db(root.path()))?;
    core.execute("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at)
        VALUES('learning-fixture','crew-milo','Prüfen','prüfen','pitfall','{}','attempt','2026-09-05T12:00:00Z')",[])?;
    let soul = serde_json::to_value(crate::crew::members(&core)?.remove(0).soul)?;
    let store_conn = store::open_store(root.path())?;
    for role in ["user", "founder", "admin"] {
        let actor_id = format!("crew-control-actor-{role}");
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            &actor_id,
            role,
            role,
            now_ms() as i64,
        )?;
        assert_eq!(
            store::verify_webrtc_capability_actor(root.path(), &token),
            Some((actor_id.clone(), role.to_string())),
            "fresh fixture capability for {role}"
        );
        for (i, kind) in [
            "member.create",
            "member.update",
            "learning.confirm",
            "learning.update",
            "assign",
            "learning.delete",
        ]
        .into_iter()
        .enumerate()
        {
            let command_type = format!("ctox.crew.{kind}");
            // A database grant is deliberately stronger than the browser helper;
            // the native CrewManage short circuit must still reject it.
            store_conn.execute("INSERT INTO business_permission_grants(grant_id,subject_type,subject_id,permission,scope_type,scope_id,active,reason,created_by,created_at_ms,updated_at_ms)
                VALUES(?1,'role',?2,'ctox.crew.manage','record',?3,1,'fixture','fixture',1,1)",rusqlite::params![format!("grant-{role}-{i}"),role,command_type])?;
            let id = format!("crew-control-{role}-{i}");
            // Grant mutations revoke capabilities through the existing epoch
            // trigger. Exercise CrewManage with a fresh authenticated token,
            // not the earlier token invalidated by the adversarial grant.
            let (token, _) =
                store::issue_business_os_capability_token(root.path(), &actor_id, now_ms() as i64)?;
            let request = json!({"id":id,"command_id":id,"module":"ctox","command_type":command_type,
                "payload":{"member_id":"crew-milo","learning_id":"learning-fixture","task_id":task.message_key,
                    "name":"Milo","shape":"round","color":"#1685ee","soul":soul,"text":"Prüfen","foreign":"ignored audit field"},
                "client_context":{"capability_token":token,"actor":{"id":actor_id,"role":role}}});
            let result = accept_rxdb_business_command_with_origin(
                root.path(),
                request.clone(),
                CommandOrigin::ReplicatedPeer,
            )
            .with_context(|| format!("accept crew control: {role} {kind}"))?;
            let allowed = role == "admin"
                || (role == "founder" && matches!(kind, "learning.confirm" | "learning.update"));
            if allowed {
                assert_eq!(result["ok"], true, "{role} {kind}: {result}");
                let replay = accept_rxdb_business_command_with_origin(
                    root.path(),
                    request,
                    CommandOrigin::ReplicatedPeer,
                )?;
                assert_eq!(replay["already_accepted"], true, "{replay}");
                assert_eq!(replay["status"], "completed", "{replay}");
                if kind == "assign" {
                    core.execute("UPDATE communication_routing_state SET route_status='leased',lease_owner='fixture-worker' WHERE message_key=?1", [&task.message_key])?;
                    let denied = accept_rxdb_business_command_with_origin(
                        root.path(),
                        json!({
                            "id":"crew-assign-leased", "command_type":"ctox.crew.assign", "module":"ctox",
                            "payload":{"task_id":task.message_key,"member_id":"crew-nori"},
                            "client_context":{"capability_token":token,"actor":{"id":actor_id,"role":role}}
                        }),
                        CommandOrigin::ReplicatedPeer,
                    ).expect_err("assignment during a held lease must be rejected");
                    assert!(
                        denied
                            .to_string()
                            .contains("unleased pending or blocked task"),
                        "{denied:#}"
                    );
                    let assigned: String = core.query_row("SELECT crew_assigned_member_id FROM communication_routing_state WHERE message_key=?1", [&task.message_key], |r|r.get(0))?;
                    assert_eq!(assigned, "crew-milo", "an active slice keeps its member");
                }
            } else {
                assert!(
                    result.to_string().contains("denied"),
                    "{role} {kind}: {result}"
                );
            }
        }
    }
    let oversized:i64=core.query_row("SELECT COUNT(*) FROM ctox_harness_flow_events WHERE event_kind='crew.control' AND length(metadata_json)>2000",[],|r|r.get(0))?;
    assert_eq!(oversized, 0);
    // The control plane never manufactures an execution task per control.
    let count: i64 = core.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE channel='queue'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn crew_memory_update_replaces_a_long_entry_and_reports_the_success() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    channels::create_queue_task(
        root.path(),
        channels::QueueTaskCreateRequest {
            title: "Crew memory".into(),
            prompt: "Read-only fixture".into(),
            thread_key: "crew-memory-fixture".into(),
            workspace_root: None,
            priority: "normal".into(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: None,
        },
    )?;
    let actor_id = "crew-memory-admin";
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        actor_id,
        "admin",
        "admin",
        now_ms() as i64,
    )?;
    let dispatch = |id: &str, payload: serde_json::Value| {
        accept_rxdb_business_command_with_origin(
            root.path(),
            json!({"id":id,"command_id":id,"module":"ctox","command_type":"ctox.crew.memory.update",
                "payload":payload,
                "client_context":{"capability_token":token,"actor":{"id":actor_id,"role":"admin"}}}),
            CommandOrigin::ReplicatedPeer,
        )
    };
    // Longer than the 200 characters `text()` allows: whole entries must be
    // replaceable, and the member touch after the LCM write must not fail.
    let long = format!(
        "statement: {}",
        "Ohne Werkzeugaufruf keine Blocker-Antwort. ".repeat(8)
    );
    let added = dispatch(
        "crew-memory-add",
        json!({"member_id":"crew-milo","kind":"anchors","mode":"diff","diff":format!("## Entries\n+ {long}")}),
    )?;
    assert_eq!(added["ok"], true, "{added}");
    let replaced = dispatch(
        "crew-memory-replace",
        json!({"member_id":"crew-milo","kind":"anchors","mode":"replace","find":long.trim(),"replace":"statement: korrigiert"}),
    )?;
    assert_eq!(replaced["ok"], true, "{replaced}");
    let engine = crate::crew::open_engine(root.path())?;
    let memory = crate::crew::load_member_memory(&engine, "crew-milo");
    assert!(
        memory.anchors.contains("statement: korrigiert"),
        "{}",
        memory.anchors
    );
    assert!(!memory.anchors.contains(long.trim()), "{}", memory.anchors);
    Ok(())
}
