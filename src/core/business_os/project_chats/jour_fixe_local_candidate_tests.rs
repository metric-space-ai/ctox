// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::{command_plane, workjet_jour_fixe_contract as wire};
use wire::WireValidate;

fn token(root: &Path, actor: &str) -> anyhow::Result<String> {
    Ok(store::issue_business_os_capability_token_for_managed_user(
        root,
        actor,
        actor,
        "admin",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0)
}
fn request(root: &Path) -> anyhow::Result<Value> {
    Ok(
        json!({"operation_id":"local-op","request_id":"helper-stream:final:1",
        "instance_id":store::stable_instance_id(root)?,"project_id":"project","meeting_id":"meeting-1",
        "deck_revision":1,"expected_revision":0,"text":"Lokaler Kandidat."}),
    )
}
fn send(root: &Path, id: &str, actor: &str, token: &str, payload: Value) -> anyhow::Result<Value> {
    command_plane::accept_rxdb_business_command_with_origin(
        root,
        json!({"id":id,"module":"ctox","record_id":"project",
            "command_type":"ctox.workjet.jour_fixe.transcript.local_candidate","payload":payload,
            "client_context":{"actor":{"id":actor},"capability_token":token}}),
        CommandOrigin::ReplicatedPeer,
    )
}
fn rejected(value: anyhow::Result<Value>) {
    assert!(
        value.is_err()
            || value
                .as_ref()
                .is_ok_and(|v| v["status"] == "failed" || v["ok"] == false),
        "{value:?}"
    );
}
fn saved(root: &Path) -> anyhow::Result<Value> {
    super::jour_fixe_owner::saved(root)
}
fn change(root: &Path, edit: impl FnOnce(&mut Value)) -> anyhow::Result<()> {
    let mut value = saved(root)?;
    edit(&mut value);
    open_store(root)?.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
        [value.to_string()],
    )?;
    Ok(())
}

#[test]
fn owner_candidate_has_exact_scope_receipt_without_gateway_or_audio_claims() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let p = request(root.path())?;
    let result = send(root.path(), "local-1", "owner", &t, p.clone())?;
    assert_eq!(result["status"], "completed", "{result}");
    let receipt: wire::LocalTranscriptCandidateReceipt =
        serde_json::from_value(result["result"]["local_candidate"].clone())?;
    receipt.validate().map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.instance_id, p["instance_id"].as_str().unwrap());
    assert_eq!(receipt.project_id, "project");
    assert_eq!(receipt.meeting_id, "meeting-1");
    assert_eq!(receipt.deck_revision, 1);
    assert_eq!(receipt.owner_user_id, "owner");
    assert_eq!(receipt.request_id, "helper-stream:final:1");
    assert_eq!(receipt.sequence, 1);
    assert_eq!(receipt.revision, 1);
    assert!(!receipt.provider_verified);
    assert_eq!(
        receipt.provenance,
        wire::LocalTranscriptProvenance::AuthenticatedOwnerLocalCandidate
    );
    assert_eq!(
        receipt.text_sha256,
        format!(
            "{:x}",
            Sha256::digest(p["text"].as_str().unwrap().as_bytes())
        )
    );
    let meeting = saved(root.path())?;
    let turns = meeting["transcript"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["id"], receipt.turn_id);
    assert_eq!(turns[0]["speaker"], "owner");
    assert_eq!(turns[0]["modality"], "text");
    for field in [
        "audio",
        "source_run_id",
        "stream_id",
        "sentence_end_latency_ms",
    ] {
        assert!(turns[0][field].is_null());
    }
    let replay = send(root.path(), "local-replay", "owner", &t, p.clone())?;
    assert_eq!(replay["result"], result["result"]);
    let recovered =
        command_plane::recover_applied_domain_effect_for_intake(root.path(), "local-1")?
            .context("domain replay")?;
    assert_eq!(recovered["result"], result["result"]);
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
#[test]
fn foreign_peer_or_forged_owner_cannot_submit_local_candidate() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "foreign")?;
    for (id, actor) in [("foreign", "foreign"), ("forged", "owner")] {
        rejected(send(root.path(), id, actor, &t, request(root.path())?));
    }
    assert_eq!(saved(root.path())?["revision"], 0);
    Ok(())
}
#[test]
fn candidate_cannot_supply_speaker_provider_audio_or_native_revision() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    for (i, (key, value)) in [
        ("speaker", json!("supervisor")),
        ("provider_verified", json!(true)),
        ("source_run_id", json!("fake")),
        ("audio", json!({})),
        ("stream_id", json!("fake")),
        ("sequence", json!(1)),
        ("revision", json!(1)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut p = request(root.path())?;
        p[key] = value;
        rejected(send(root.path(), &format!("forged-{i}"), "owner", &t, p));
    }
    assert_eq!(saved(root.path())?["revision"], 0);
    Ok(())
}
#[test]
fn instance_project_deck_revision_and_live_state_are_required() -> anyhow::Result<()> {
    for (key, value) in [
        ("instance_id", json!("managed:local-ui-alias")),
        ("project_id", json!("foreign-project")),
        ("deck_revision", json!(2)),
        ("expected_revision", json!(1)),
    ] {
        let root = super::jour_fixe_owner::fixture("live")?;
        let t = token(root.path(), "owner")?;
        let mut p = request(root.path())?;
        p[key] = value;
        rejected(send(root.path(), "stale-scope", "owner", &t, p));
        assert_eq!(saved(root.path())?["revision"], 0);
    }
    for state in [
        "planned",
        "preparing",
        "ready",
        "review",
        "confirmed",
        "cancelled",
        "failed",
    ] {
        let root = super::jour_fixe_owner::fixture(state)?;
        let t = token(root.path(), "owner")?;
        rejected(send(
            root.path(),
            "nonlive",
            "owner",
            &t,
            request(root.path())?,
        ));
        assert_eq!(saved(root.path())?["revision"], 0);
    }
    Ok(())
}
#[test]
fn repeated_final_cannot_be_duplicated_with_another_operation_or_changed_text() -> anyhow::Result<()>
{
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let p = request(root.path())?;
    let result = send(root.path(), "first", "owner", &t, p.clone())?;
    assert_eq!(result["status"], "completed");
    let mut changed = p.clone();
    changed["text"] = json!("Different candidate");
    rejected(send(root.path(), "changed-text", "owner", &t, changed));
    let mut duplicate = p;
    duplicate["operation_id"] = json!("another-op");
    duplicate["expected_revision"] = json!(1);
    rejected(send(root.path(), "duplicate-final", "owner", &t, duplicate));
    assert_eq!(
        saved(root.path())?["transcript"].as_array().unwrap().len(),
        1
    );
    Ok(())
}
#[test]
fn revoked_peer_cannot_receive_an_old_local_candidate_receipt() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let p = request(root.path())?;
    assert_eq!(
        send(root.path(), "before-revocation", "owner", &t, p.clone())?["status"],
        "completed"
    );
    open_store(root.path())?.execute(
        "UPDATE business_users SET active=0 WHERE user_id='owner'",
        [],
    )?;
    rejected(send(root.path(), "before-revocation", "owner", &t, p));
    assert!(command_plane::recover_applied_domain_effect_for_intake(
        root.path(),
        "before-revocation"
    )
    .is_err());
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
#[test]
fn receipt_recovery_rejects_closed_or_replaced_meeting_scope() -> anyhow::Result<()> {
    for case in ["closed", "deck", "supervisor", "project"] {
        let root = super::jour_fixe_owner::fixture("live")?;
        let t = token(root.path(), "owner")?;
        let p = request(root.path())?;
        assert_eq!(
            send(root.path(), "original", "owner", &t, p.clone())?["status"],
            "completed"
        );
        match case {
            "closed" => change(root.path(), |v| v["state"] = json!("review"))?,
            "deck" => change(root.path(), |v| v["deck_revision"] = json!(2))?,
            "supervisor" => {
                open_store(root.path())?.execute("DELETE FROM user_threads", [])?;
            }
            "project" => {
                let conn = open_store(root.path())?;
                let mut project =
                    outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
                project["status"] = json!("archived");
                store::upsert_business_record(&conn, "workjet_projects", "project", 2, project)?;
            }
            _ => unreachable!(),
        }
        rejected(send(root.path(), "original", "owner", &t, p));
        assert!(
            command_plane::recover_applied_domain_effect_for_intake(root.path(), "original")
                .is_err()
        );
        assert_eq!(saved(root.path())?["revision"], 1);
    }
    Ok(())
}
#[test]
fn receipt_failure_rolls_back_candidate_and_meeting_together() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let p = request(root.path())?;
    let conn = open_store(root.path())?;
    conn.execute_batch("CREATE TRIGGER reject_local_receipt BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(FAIL,'fixture receipt failure'); END;")?;
    rejected(send(root.path(), "receipt-failure", "owner", &t, p.clone()));
    assert_eq!(saved(root.path())?["revision"], 0);
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM workjet_jour_fixe_local_candidates",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 0);
    conn.execute_batch("DROP TRIGGER reject_local_receipt")?;
    assert_eq!(
        send(root.path(), "receipt-retry", "owner", &t, p)?["status"],
        "completed"
    );
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
#[test]
fn utf8_candidate_budget_counts_bytes_not_characters() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let mut p = request(root.path())?;
    p["text"] = json!("é".repeat(2049));
    rejected(send(root.path(), "oversize-utf8", "owner", &t, p.clone()));
    assert_eq!(saved(root.path())?["revision"], 0);
    p["text"] = json!("é".repeat(2048));
    assert_eq!(
        send(root.path(), "bounded-utf8", "owner", &t, p)?["status"],
        "completed"
    );
    Ok(())
}
#[test]
fn concurrent_retries_apply_one_final_with_original_receipt() -> anyhow::Result<()> {
    let root = super::jour_fixe_owner::fixture("live")?;
    let t = token(root.path(), "owner")?;
    let p = request(root.path())?;
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for i in 0..2 {
        let path = root.path().to_owned();
        let t = t.clone();
        let p = p.clone();
        let b = barrier.clone();
        workers.push(std::thread::spawn(move || {
            b.wait();
            send(&path, &format!("concurrent-{i}"), "owner", &t, p)
        }));
    }
    barrier.wait();
    let a = workers.remove(0).join().unwrap()?;
    let b = workers.remove(0).join().unwrap()?;
    assert_eq!(a["status"], "completed", "{a}");
    assert_eq!(b["status"], "completed", "{b}");
    assert_eq!(a["result"], b["result"]);
    assert_eq!(saved(root.path())?["revision"], 1);
    Ok(())
}
