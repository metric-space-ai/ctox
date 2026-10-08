// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::business_os::project_chats::presentation::*;
use crate::business_os::{
    workjet_jour_fixe_contract as meeting_wire, workjet_presentation_contract as wire,
};

const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

fn node_available() -> bool {
    crate::business_os::project_chats::presentation_validator::node().is_ok()
}

fn deck() -> Value {
    serde_json::from_str(include_str!("slide-engine/jour-fixe-deck.json")).unwrap()
}

fn fixture() -> anyhow::Result<(TempDir, meeting_wire::Meeting)> {
    let root = super::supervisor_turns::fixture()?;
    crate::business_os::store::stable_instance_id(root.path())?;
    let corpus: Value = serde_json::from_str(include_str!(
        "../../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))?;
    let mut meeting = corpus["valid_cases"][0]["value"].clone();
    meeting["project_id"] = json!("project");
    meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!("preparing");
    meeting["revision"] = json!(0);
    meeting["comments"] = json!([]);
    meeting["transcript"] = json!([]);
    meeting["todos"] = Value::Null;
    meeting["previous_goal"] = Value::Null;
    let conn = open_store(root.path())?;
    conn.execute_batch(crate::business_os::project_chats::jour_fixe_preparation::SCHEMA)?;
    conn.execute(
        "INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791450000000,?1,NULL)",
        [meeting.to_string()],
    )?;
    let db = Connection::open(crate::business_os::store::rxdb_store_path(root.path()))?;
    for collection in ["desktop_files", "desktop_file_chunks"] {
        db.execute_batch(&format!("CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v0(id TEXT PRIMARY KEY,revision TEXT NOT NULL,deleted INTEGER NOT NULL,lastWriteTime REAL NOT NULL,data TEXT NOT NULL)"))?;
    }
    let typed: meeting_wire::Meeting = serde_json::from_value(meeting)?;
    Ok((root, typed))
}

/// Stores revision 1 the way the Supervisor tool does.
fn agent_revision(root: &Path, meeting: &meeting_wire::Meeting) -> anyhow::Result<Value> {
    let document = validated_document(root, deck())?;
    let conn = open_store(root)?;
    let tx = conn.unchecked_transaction()?;
    let (manifest, refs) = commit_revision(
        root,
        &tx,
        NewRevision {
            meeting,
            prior: None,
            document: &document,
            source: wire::PresentationSource::Agent,
            actor: "supervisor:test",
            operation_id: "agent-create",
        },
    )?;
    let result = receipt("agent-create", &manifest)?;
    record_operation(
        &tx,
        "agent-create",
        &meeting.owner_user_id,
        &manifest.presentation_id,
        "intent",
        &result,
        &refs,
    )?;
    tx.commit()?;
    Ok(serde_json::to_value(manifest)?)
}

fn send(
    root: &Path,
    command: &str,
    kind: &str,
    actor: &str,
    payload: Value,
) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":command,"module":"ctox","record_id":"project",
            "command_type":format!("ctox.workjet.presentation.{kind}"),"payload":payload,
            "client_context":{"actor":{"id":actor,"role":"admin","is_admin":true}}}),
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

fn canvas_scene(text: &str) -> String {
    json!({"version":"learnordie.excalidraw.v1","width":1600,"height":900,"backgroundColor":"#fffef8",
        "elements":[{"id":"note-1","type":"text","x":120,"y":140,"width":600,"height":60,
            "text":text,"originalText":text,"fontSize":36,"fontFamily":5},
            {"id":"box-1","type":"rectangle","x":820,"y":300,"width":300,"height":180}],"files":{}})
    .to_string()
}

#[test]
fn owner_reads_manifest_and_saves_a_canvas_revision_with_replay() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, meeting) = fixture()?;
    let created = agent_revision(root.path(), &meeting)?;
    assert_eq!(created["revision"], 1);
    assert_eq!(created["source"], "agent");

    let read = send(
        root.path(),
        "read-1",
        "read",
        "owner",
        json!({"project_id":"project","meeting_id":"meeting-1"}),
    )?;
    assert_eq!(read["status"], "completed", "{read}");
    assert_eq!(read["result"]["presentation"]["revision"], 1);
    let slide = read["result"]["presentation"]["slide_ids"][1]
        .as_str()
        .unwrap()
        .to_owned();
    let presentation = read["result"]["presentation"]["presentation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let save = json!({"operation_id":"canvas-op","presentation_id":presentation,"expected_revision":1,
        "slide_id":slide,"scene_json":canvas_scene("Neuer Gedanke")});
    let saved = send(root.path(), "save-1", "canvas.save", "owner", save.clone())?;
    assert_eq!(saved["status"], "completed", "{saved}");
    assert_eq!(saved["result"]["mutation"]["revision"], 2);
    assert_eq!(saved["result"]["presentation"]["source"], "owner");

    // Same operation and intent: the stored receipt, no third revision.
    let replay = send(root.path(), "save-2", "canvas.save", "owner", save)?;
    assert_eq!(replay["result"]["mutation"], saved["result"]["mutation"]);
    let conn = open_store(root.path())?;
    let manifest = load_by_meeting(&conn, "meeting-1")?.unwrap();
    assert_eq!(manifest.revision, 2);
    let document: Value = serde_json::from_str(&current_document(&conn, &manifest)?)?;
    let stored = document["slides"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == slide.as_str())
        .unwrap();
    assert!(stored["canvas"]["elements"]
        .to_string()
        .contains("Neuer Gedanke"));

    // Each revision is its own immutable desktop file with the document hash.
    let files: i64 = conn.query_row(
        "SELECT COUNT(*) FROM business_records WHERE collection='desktop_files' AND json_extract(payload_json,'$.linked_record_id')=?1",
        [&presentation],
        |r| r.get(0),
    )?;
    assert_eq!(files, 2);
    let hash: String = conn.query_row(
        "SELECT json_extract(payload_json,'$.content_hash') FROM business_records WHERE collection='desktop_files' AND record_id=?1",
        [&manifest.document_file_id],
        |r| r.get(0),
    )?;
    assert_eq!(hash, manifest.document_sha256);
    Ok(())
}

#[test]
fn stale_foreign_and_invalid_saves_keep_the_revision() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, meeting) = fixture()?;
    let created = agent_revision(root.path(), &meeting)?;
    let presentation = created["presentation_id"].as_str().unwrap().to_owned();
    let slide = created["slide_ids"][0].as_str().unwrap().to_owned();
    let request = |revision: u64, scene: String| {
        json!({"operation_id":format!("op-{revision}-{}", scene.len()),"presentation_id":presentation,
            "expected_revision":revision,"slide_id":slide,"scene_json":scene})
    };
    rejected(send(
        root.path(),
        "stale",
        "canvas.save",
        "owner",
        request(7, canvas_scene("x")),
    ));
    rejected(send(
        root.path(),
        "foreign",
        "canvas.save",
        "intruder",
        request(1, canvas_scene("x")),
    ));
    let invalid = json!({"version":"learnordie.excalidraw.v1","width":1600,"height":900,
        "backgroundColor":"#fffef8","elements":[{"id":"t","type":"text","x":1,"y":1,"width":10,"height":10}],"files":{}});
    let failed = send(
        root.path(),
        "invalid",
        "canvas.save",
        "owner",
        request(1, invalid.to_string()),
    );
    let message = match failed {
        Ok(value) => {
            assert_eq!(value["status"], "failed", "{value}");
            value.to_string()
        }
        Err(error) => error.to_string(),
    };
    assert!(message.contains("presentation rejected"), "{message}");
    let conn = open_store(root.path())?;
    assert_eq!(load_by_meeting(&conn, "meeting-1")?.unwrap().revision, 1);
    Ok(())
}

#[test]
fn owner_edit_batches_use_the_engine_semantics() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, meeting) = fixture()?;
    let created = agent_revision(root.path(), &meeting)?;
    let presentation = created["presentation_id"].as_str().unwrap().to_owned();
    let last = created["slide_ids"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let operations = json!([{"kind":"deleteSlide","slideId":last}]).to_string();
    let applied = send(
        root.path(),
        "edits",
        "edits.apply",
        "owner",
        json!({"operation_id":"edit-op","presentation_id":presentation,"expected_revision":1,
            "operations_json":operations}),
    )?;
    assert_eq!(applied["status"], "completed", "{applied}");
    let ids = applied["result"]["mutation"]["slide_ids"]
        .as_array()
        .unwrap();
    assert!(!ids.iter().any(|id| id == last.as_str()));
    let missing = json!([{"kind":"deleteSlide","slideId":"does-not-exist"}]).to_string();
    rejected(send(
        root.path(),
        "edits-missing",
        "edits.apply",
        "owner",
        json!({"operation_id":"edit-op-2","presentation_id":presentation,"expected_revision":2,
            "operations_json":missing}),
    ));
    Ok(())
}

#[test]
fn meeting_slides_keep_presentation_ids_and_fit_the_narration_budget() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let document = deck();
    let slides = meeting_slides(root.path(), &document, "meeting-1")?;
    let ids: Vec<&str> = document["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        slides
            .iter()
            .map(|s| s["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ids
    );
    for (position, slide) in slides.iter().enumerate() {
        assert_eq!(slide["position"], position);
        assert_eq!(slide["meeting_id"], "meeting-1");
        let body = slide["body_markdown"].as_str().unwrap();
        assert!(!body.trim().is_empty() && body.len() <= 4096);
        let typed: meeting_wire::Slide = serde_json::from_value(slide.clone())?;
        meeting_wire::WireValidate::validate(&typed).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
