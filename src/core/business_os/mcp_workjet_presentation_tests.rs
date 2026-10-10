// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use serde_json::json;

const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";

fn node_available() -> bool {
    crate::business_os::project_chats::presentation_validator::node().is_ok()
}

fn deck() -> Value {
    serde_json::from_str(include_str!(
        "project_chats/slide-engine/jour-fixe-deck.json"
    ))
    .unwrap()
}

fn meeting(id: &str, scheduled_at_ms: i64, state: &str) -> anyhow::Result<Value> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))?;
    let mut meeting = corpus["valid_cases"][0]["value"].clone();
    meeting["id"] = json!(id);
    meeting["project_id"] = json!("project");
    meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!(state);
    meeting["scheduled_at_ms"] = json!(scheduled_at_ms);
    meeting["revision"] = json!(0);
    meeting["comments"] = json!([]);
    meeting["transcript"] = json!([]);
    meeting["todos"] = Value::Null;
    meeting["previous_goal"] = Value::Null;
    meeting["slides"] = json!([]);
    meeting["deck_revision"] = json!(0);
    Ok(meeting)
}

fn fixture() -> anyhow::Result<(tempfile::TempDir, Value)> {
    let (root, trusted) = workjet_worker_dispatch::meeting_test_fixture()?;
    store::stable_instance_id(root.path())?;
    let policy = store::open_store(root.path())?;
    policy.execute_batch(
        "CREATE TABLE workjet_jour_fixe_meetings (
        meeting_id TEXT PRIMARY KEY,project_id TEXT NOT NULL,owner_user_id TEXT NOT NULL,
        scheduled_at_ms INTEGER NOT NULL,metadata_json TEXT NOT NULL,preparation_task_id TEXT);",
    )?;
    policy.execute(
        "INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791450000000,?1,NULL)",
        [meeting("meeting-1", 1_791_450_000_000, "planned")?.to_string()],
    )?;
    Ok((root, trusted))
}

fn call(root: &Path, trusted: &Value, tool: &str, args: Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(root, tool, args, Some(trusted))
}

fn scope(extra: Value) -> Value {
    let mut request = json!({"project_id":"project","meeting_id":"meeting-1"});
    if let (Some(target), Value::Object(extra)) = (request.as_object_mut(), extra) {
        target.extend(extra);
    }
    request
}

#[test]
fn guide_and_dry_run_validation_reach_the_supervisor() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, trusted) = fixture()?;
    let guide = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"read_guide","request":{}}),
    )?;
    assert!(guide["guide"]
        .as_str()
        .unwrap()
        .contains("learnordie.slide.v1"));
    let valid = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"validate_document","request":scope(json!({"document":deck()}))}),
    )?;
    assert_eq!(valid["validation"]["ok"], true, "{valid}");
    let mut broken = deck();
    broken["slides"][1]["blocks"][1]["data"] = Value::Null;
    let invalid = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"validate_document","request":scope(json!({"document":broken}))}),
    )?;
    assert_eq!(invalid["validation"]["ok"], false, "{invalid}");
    Ok(())
}

#[test]
fn supervisor_decks_must_pass_the_content_rules() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, trusted) = fixture()?;
    // Wording from the first real decks (09.10.2026): a slide that only says
    // "keine Daten" and a sentence about the display instead of the project.
    let mut filler = deck();
    filler["slides"][2]["title"] = json!("Exitwert E5: keine Daten");
    filler["slides"][3]["blocks"][1]["text"] =
        json!("Die Trenddarstellung bleibt deshalb bewusst leer.");
    let dry = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"validate_document","request":scope(json!({"document":filler}))}),
    )?;
    assert_eq!(dry["validation"]["ok"], false, "{dry}");
    let codes: Vec<&str> = dry["validation"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|issue| issue["code"].as_str())
        .collect();
    assert!(codes.contains(&"content.empty_slide"), "{dry}");
    assert!(codes.contains(&"content.meta_phrase"), "{dry}");
    let rejected = call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"create_presentation","request":scope(json!({"operation_id":"create-filler","document":filler}))}),
    )
    .unwrap_err()
    .to_string();
    assert!(rejected.contains("content.empty_slide"), "{rejected}");
    assert!(rejected.contains("Repair:"), "{rejected}");
    let created = call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"create_presentation","request":scope(json!({"operation_id":"create-clean","document":deck()}))}),
    )?;
    assert_eq!(created["mutation"]["revision"], 1, "{created}");
    Ok(())
}

#[test]
fn supervisor_creates_edits_and_publishes_the_meeting_deck() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, trusted) = fixture()?;
    let create = json!({"action":"create_presentation","request":scope(json!({"operation_id":"create-1","document":deck()}))});
    let first = call(root.path(), &trusted, WRITE_TOOL, create.clone())?;
    assert_eq!(first["mutation"]["revision"], 1, "{first}");
    assert_eq!(first["presentation"]["source"], "agent");
    let replay = call(root.path(), &trusted, WRITE_TOOL, create)?;
    assert_eq!(replay["mutation"], first["mutation"]);
    assert!(call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"create_presentation","request":scope(json!({"operation_id":"create-2","document":deck()}))}),
    )
    .is_err());

    let ids: Vec<String> = first["mutation"]["slide_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    let last = ids.last().unwrap().clone();
    let edited = call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"apply_edits","request":scope(json!({"operation_id":"edit-1","expected_revision":1,
            "operations":[{"kind":"deleteSlide","slideId":last}]}))}),
    )?;
    assert_eq!(edited["mutation"]["revision"], 2, "{edited}");
    assert!(call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"apply_edits","request":scope(json!({"operation_id":"edit-stale","expected_revision":1,
            "operations":[{"kind":"deleteSlide","slideId":ids[0]}]}))}),
    )
    .is_err());

    let outline = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"read_presentation","request":scope(json!({}))}),
    )?;
    assert_eq!(outline["presentation"]["revision"], 2);
    assert!(outline["outline"]["slides"].as_array().unwrap().len() == ids.len() - 1);

    let published = call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"publish_deck","request":scope(json!({"operation_id":"publish-1",
            "presentation_revision":2,"expected_meeting_revision":0,"deck_revision":1}))}),
    )?;
    assert_eq!(published["mutation"]["state"], "preparing", "{published}");
    let read = call(
        root.path(),
        &trusted,
        workjet_jour_fixe::READ_TOOL,
        json!({"action":"read_meeting","request":{"project_id":"project","meeting_id":"meeting-1"}}),
    )?;
    let deck_ids: Vec<&str> = read["meeting"]["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slide| slide["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        deck_ids,
        ids[..ids.len() - 1]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert!(read["meeting"]["slides"]
        .as_array()
        .unwrap()
        .iter()
        .all(
            |slide| !slide["body_markdown"].as_str().unwrap().trim().is_empty()
                && slide["audio"].is_null()
        ));
    Ok(())
}

#[test]
fn history_returns_what_earlier_meetings_showed() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, trusted) = fixture()?;
    let earlier = meeting("meeting-0", 1_790_845_200_000, "confirmed")?;
    let conn = store::open_store(root.path())?;
    conn.execute(
        "INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-0','project','owner',1790845200000,?1,NULL)",
        [earlier.to_string()],
    )?;
    let typed: super::super::super::workjet_jour_fixe_contract::Meeting =
        serde_json::from_value(earlier)?;
    let document = store_presentation::validated_document(root.path(), deck())?;
    let tx = conn.unchecked_transaction()?;
    store_presentation::commit_revision(
        root.path(),
        &tx,
        store_presentation::NewRevision {
            meeting: &typed,
            prior: None,
            document: &document,
            source: wire::PresentationSource::Agent,
            actor: "supervisor:test",
            operation_id: "earlier",
        },
    )?;
    tx.commit()?;
    let history = call(
        root.path(),
        &trusted,
        READ_TOOL,
        json!({"action":"read_history","request":scope(json!({"limit":2}))}),
    )?;
    let occurrences = history["occurrences"].as_array().unwrap();
    assert_eq!(occurrences.len(), 1, "{history}");
    assert_eq!(occurrences[0]["index"], 1);
    assert_eq!(occurrences[0]["meeting_id"], "meeting-0");
    let scenes = occurrences[0]["scenes"].as_array().unwrap();
    assert!(
        scenes
            .iter()
            .any(|scene| scene["scene_id"] == "business.kpi-bars"
                && scene["data"]["items"].is_array())
    );
    Ok(())
}

#[test]
fn earlier_meetings_keep_the_presentation_they_showed() -> anyhow::Result<()> {
    if !node_available() {
        eprintln!("SKIP: node not available");
        return Ok(());
    }
    let (root, trusted) = fixture()?;
    store::open_store(root.path())?.execute(
        "INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-2','project','owner',1792054800000,?1,NULL)",
        [meeting("meeting-2", 1_792_054_800_000, "planned")?.to_string()],
    )?;
    let refused = call(
        root.path(),
        &trusted,
        WRITE_TOOL,
        json!({"action":"create_presentation","request":scope(json!({"operation_id":"late-1","document":deck()}))}),
    );
    let message = refused
        .expect_err("an earlier meeting accepted a new presentation")
        .to_string();
    assert!(message.contains("later meeting"), "{message}");
    Ok(())
}
