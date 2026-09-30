use super::*;
use serde_json::json;
use tempfile::{tempdir, TempDir};

const LEAD: &str = "lead-note-review";
const TABLE: &str = "ctox_business_os__outbound_lead_generation_leads__v0";

fn research_contract() -> Value {
    json!({
        "writeback_contract": {
            "mechanism": "business_command",
            "command_type": "outbound.lead.research_writeback",
            "collection": "outbound_lead_generation_leads",
            "record_ids": [LEAD]
        }
    })
}

fn command(payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: None,
        module: "outbound-lead-generation".into(),
        command_type: "business_os.chat.task".into(),
        record_id: Some(LEAD.into()),
        payload,
        client_context: json!({"source": "outbound-lead-generation-sperrvermerk"}),
        origin: CommandOrigin::TrustedLocal,
    }
}

fn fixture(status: &str) -> anyhow::Result<(TempDir, Connection, Value)> {
    let temp = tempdir()?;
    fs::create_dir_all(temp.path().join("runtime"))?;
    let conn = Connection::open(rxdb_store_path(temp.path()))?;
    conn.execute(
        &format!("CREATE TABLE {TABLE} (id TEXT PRIMARY KEY, data TEXT NOT NULL)"),
        [],
    )?;
    let lead = json!({
        "id": LEAD,
        "_rev": "7-existing",
        "_deleted": false,
        "research_status": status,
        "research_error": "existing research evidence",
        "command_id": "existing-research",
        "task_id": "existing-task",
        "research_updated_at_ms": 42,
        "data": {"firma_name": "Customer"}
    });
    conn.execute(
        &format!("INSERT INTO {TABLE} (id, data) VALUES (?1, ?2)"),
        params![LEAD, lead.to_string()],
    )?;
    Ok((temp, conn, lead))
}

fn read_lead(conn: &Connection) -> anyhow::Result<Value> {
    let raw: String = conn.query_row(
        &format!("SELECT data FROM {TABLE} WHERE id = ?1"),
        [LEAD],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}

fn attached_fixture(root: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute("ATTACH DATABASE ':memory:' AS business_os_projection", [])?;
    conn.execute_batch(
        "CREATE TABLE business_os_projection.business_records (
            collection TEXT, record_id TEXT, rev TEXT, deleted INTEGER,
            updated_at_ms INTEGER, payload_json TEXT,
            PRIMARY KEY (collection, record_id)
        )",
    )?;
    attach_rxdb_projection_store(root, &conn)?;
    Ok(conn)
}

fn projected_command(payload: Value) -> Value {
    json!({
        "module": "outbound-lead-generation",
        "command_type": "business_os.chat.task",
        "record_id": LEAD,
        "payload": payload
    })
}

#[test]
fn crm_note_review_with_a_lead_id_does_not_queue_research() -> anyhow::Result<()> {
    for status in ["new", "needs_review", "completed"] {
        let (root, conn, before) = fixture(status)?;
        let note = command(json!({
            "lead_ids": [LEAD],
            "thread_key": "business-os/outbound-lead-generation/sperrvermerk/review",
            "prompt": "Review the CRM note"
        }));
        project_outbound_lead_queued(root.path(), &note, "note-review", Some("note-task"))?;
        assert_eq!(read_lead(&conn)?, before, "status {status} was overwritten");
    }
    Ok(())
}

#[test]
fn unrelated_or_out_of_scope_contracts_do_not_change_the_lead() -> anyhow::Result<()> {
    let base = research_contract();
    let mut payloads = vec![json!({})];
    for (key, value) in [
        ("mechanism", json!("chat")),
        ("command_type", json!("outbound.note.review")),
        ("collection", json!("another_collection")),
        ("record_ids", json!(["another-lead"])),
        ("record_ids", json!([])),
        ("record_ids", json!([42])),
    ] {
        let mut payload = base.clone();
        payload["writeback_contract"][key] = value;
        payloads.push(payload);
    }
    for payload in payloads {
        let (root, conn, before) = fixture("needs_review")?;
        project_outbound_lead_queued(root.path(), &command(payload), "other-task", None)?;
        assert_eq!(read_lead(&conn)?, before);
    }
    Ok(())
}

#[test]
fn scoped_research_still_queues_and_preserves_customer_fields() -> anyhow::Result<()> {
    let (root, conn, before) = fixture("needs_review")?;
    project_outbound_lead_queued(
        root.path(),
        &command(research_contract()),
        "new-research",
        Some("new-task"),
    )?;
    let after = read_lead(&conn)?;
    assert_eq!(after["research_status"], "queued");
    assert_eq!(after["research_error"], "");
    assert_eq!(after["command_id"], "new-research");
    assert_eq!(after["task_id"], "new-task");
    assert_eq!(after["data"], before["data"]);
    assert_ne!(after["_rev"], before["_rev"]);
    Ok(())
}

#[test]
fn note_lease_does_not_promote_an_old_queued_lead() -> anyhow::Result<()> {
    let (root, conn, before) = fixture("queued")?;
    let attached = attached_fixture(root.path())?;
    mark_outbound_lead_running_attached(
        &attached,
        &projected_command(json!({"lead_ids": [LEAD]})),
        "existing-research",
        "note-task",
        100,
    )?;
    assert_eq!(read_lead(&conn)?, before);
    assert_eq!(
        attached.query_row(
            "SELECT COUNT(*) FROM business_os_projection.business_records",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    Ok(())
}

#[test]
fn scoped_research_lease_promotes_only_its_own_queued_lead() -> anyhow::Result<()> {
    let (root, conn, before) = fixture("queued")?;
    let attached = attached_fixture(root.path())?;
    let task = projected_command(research_contract());
    mark_outbound_lead_running_attached(&attached, &task, "another-command", "another-task", 100)?;
    assert_eq!(read_lead(&conn)?, before);
    mark_outbound_lead_running_attached(
        &attached,
        &task,
        "existing-research",
        "research-task",
        101,
    )?;
    let after = read_lead(&conn)?;
    assert_eq!(after["research_status"], "running");
    assert_eq!(after["task_id"], "research-task");
    assert_eq!(after["command_id"], before["command_id"]);
    assert_eq!(after["data"], before["data"]);
    let stored: String = attached.query_row(
        "SELECT payload_json FROM business_os_projection.business_records
         WHERE collection = 'outbound_lead_generation_leads' AND record_id = ?1",
        [LEAD],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<Value>(&stored)?["research_status"],
        "running"
    );
    Ok(())
}

#[test]
fn projection_requires_outbound_chat_and_trimmed_record_scope() {
    let payload = research_contract();
    for (module, kind, record) in [
        ("another-app", "business_os.chat.task", Some(LEAD)),
        (
            "outbound-lead-generation",
            "outbound.note.review",
            Some(LEAD),
        ),
        ("outbound-lead-generation", "business_os.chat.task", None),
        (
            "outbound-lead-generation",
            "business_os.chat.task",
            Some(" "),
        ),
    ] {
        assert_eq!(
            outbound_research_projection_record_id(module, kind, record, &payload),
            None
        );
    }
    let mut padded = payload;
    padded["writeback_contract"]["record_ids"] = json!([format!(" {LEAD} "), "another-lead"]);
    assert_eq!(
        outbound_research_projection_record_id(
            "outbound-lead-generation",
            "business_os.chat.task",
            Some(LEAD),
            &padded
        ),
        Some(LEAD)
    );
}
