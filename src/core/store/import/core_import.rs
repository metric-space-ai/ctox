//! Allowlisted reads of the frozen core snapshot. Never modifies the source.
//! The caller stages files through the existing files primitive, not a parallel
//! blob store. No secret table, password hash or DKIM private key is selected.
use super::*;
use rusqlite::{types::ValueRef, Connection};
use serde_json::{json, Map};
use std::collections::{BTreeMap, BTreeSet};

pub trait ImportServices {
    /// Existing crew member_conversation_id; no replacement hash algorithm.
    fn member_conversation_id(&self, member: &str) -> i64;
    /// Must hash trimmed UTF-8 bytes and reject a mismatched expected SHA256.
    /// Returns an existing files.ref and verified size, never a transfer ID.
    /// None means the digest mismatched: caller reports and skips that revision.
    fn script(
        &mut self,
        name: &str,
        body: &str,
        expected_sha256: &str,
    ) -> Result<Option<(String, u64)>>;
    /// Restricted to the offline profile's artifact root. Missing files are None.
    /// The files owner must verify expected_sha256 if present.
    fn artifact(&mut self, path: &str, expected_sha256: Option<&str>) -> Result<Option<String>>;
}

pub struct CoreImport {
    pub documents: Vec<Document>,
    pub dataframe_rows: Vec<Value>,
    pub report: ImportReport,
}

/// The dataframe owner consumes a stable import key and preserves source row
/// multiplicity; it must commit its receipt with the rows, not guess completion.
pub trait DataframeImporter {
    fn import_timeseries(&mut self, import_id: &str, rows: &[Value]) -> Result<usize>;
}

pub async fn apply(
    writer: &WriterHandle,
    snapshot: CoreImport,
    dataframe: &mut impl DataframeImporter,
    import_id: &str,
) -> Result<ImportReport> {
    if import_id.is_empty() {
        return Err(invalid("import_id_required"));
    }
    let mut report = snapshot.report;
    let mut pending = snapshot.documents.into_iter().peekable();
    while pending.peek().is_some() {
        let mut page = Vec::new();
        let mut bytes = 0;
        while let Some(doc) = pending.peek() {
            let size = doc.body.to_string().len();
            if !page.is_empty() && (page.len() == 128 || bytes + size > 1024 * 1024) {
                break;
            }
            bytes += size;
            page.push(pending.next().unwrap());
        }
        let written = super::write_page(writer, page).await?;
        for (collection, counts) in written.collections {
            let total = report.collections.entry(collection).or_default();
            total.target_documents += counts.target_documents;
            total.inserted += counts.inserted;
            total.already_present += counts.already_present;
        }
    }
    if !snapshot.dataframe_rows.is_empty() {
        let imported = dataframe.import_timeseries(import_id, &snapshot.dataframe_rows)?;
        if imported != snapshot.dataframe_rows.len() {
            return Err(invalid("dataframe_import_incomplete"));
        }
        report
            .collections
            .entry("dataframe:iot_timeseries".into())
            .or_default()
            .target_documents = imported;
    }
    Ok(report)
}

fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    sql(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    ))
}
/// Only literal allowlisted columns. Source schema variation cannot cause SELECT *.
fn rows(conn: &Connection, table: &str, fields: &str) -> Result<Vec<Value>> {
    if !has_table(conn, table)? {
        return Ok(Vec::new());
    }
    let known = sql(conn.prepare(&format!("PRAGMA table_info(\"{table}\")")))?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(StoreError::from)?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .map_err(StoreError::from)?;
    let fields: Vec<_> = fields.split(',').collect();
    let selected: Vec<_> = fields
        .iter()
        .filter(|f| known.contains(**f))
        .copied()
        .collect();
    if selected.is_empty() {
        return Err(invalid(format!("import_source_schema:{table}")));
    }
    let mut statement = sql(conn.prepare(&format!(
        "SELECT {} FROM \"{table}\"",
        selected
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(",")
    )))?;
    let mut cursor = sql(statement.query([]))?;
    let mut output = Vec::new();
    while let Some(row) = sql(cursor.next())? {
        let mut object = Map::new();
        for (i, field) in selected.iter().enumerate() {
            let value = match sql(row.get_ref(i))? {
                ValueRef::Null => continue,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(t) => Value::String(
                    std::str::from_utf8(t)
                        .map_err(|_| invalid(format!("import_utf8:{table}")))?
                        .into(),
                ),
                ValueRef::Blob(_) => {
                    return Err(invalid(format!("import_unexpected_blob:{table}:{field}")))
                }
            };
            object.insert((*field).to_owned(), value);
        }
        output.push(Value::Object(object));
    }
    Ok(output)
}
fn text<'a>(row: &'a Value, field: &str) -> Result<&'a str> {
    row[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("import_required:{field}")))
}
fn json_field(row: &mut Value, field: &str, target: &str, default: Value) -> Result<()> {
    let value = row
        .as_object_mut()
        .unwrap()
        .remove(field)
        .unwrap_or(Value::Null);
    row[target] = match value {
        Value::Null => default,
        Value::String(s) => {
            serde_json::from_str(&s).map_err(|_| invalid(format!("import_invalid_json:{field}")))?
        }
        other => other,
    };
    Ok(())
}
fn millis(conn: &Connection, value: &Value) -> Result<i64> {
    if let Some(n) = value.as_i64() {
        return Ok(n);
    }
    if let Some(s) = value.as_str() {
        if let Ok(n) = s.parse::<i64>() {
            return Ok(n);
        }
        let parsed: Option<i64> = sql(conn.query_row(
            "SELECT CAST(round((julianday(?1)-2440587.5)*86400000) AS INTEGER)",
            [s],
            |r| r.get(0),
        ))?;
        if let Some(n) = parsed {
            return Ok(n);
        }
    }
    Err(invalid("import_invalid_timestamp"))
}
fn boolean(row: &mut Value, field: &str) {
    if let Some(n) = row[field].as_i64() {
        row[field] = Value::Bool(n != 0);
    }
}
fn common(conn: &Connection, row: &mut Value, id: &str) -> Result<()> {
    row["id"] = id.into();
    row["is_deleted"] = false.into();
    let stamp = [
        "updated_at",
        "created_at",
        "observed_at",
        "opened_at",
        "external_created_at",
    ]
    .iter()
    .find_map(|k| row.get(*k))
    .cloned();
    if let Some(stamp) = stamp {
        row["updated_at_ms"] = millis(conn, &stamp)?.into();
    }
    if let Some(stamp) = row.get("created_at").cloned() {
        row["created_at_ms"] = millis(conn, &stamp)?.into();
    }
    Ok(())
}
fn push(output: &mut CoreImport, collection: &str, id: String, body: Value) -> Result<()> {
    if output
        .documents
        .iter()
        .any(|d| d.collection == collection && d.id == id)
    {
        return Err(invalid(format!(
            "import_duplicate_identity:{collection}:{id}"
        )));
    }
    output
        .report
        .collections
        .entry(collection.into())
        .or_default()
        .source_rows += 1;
    output.documents.push(Document {
        revision: None,
        collection: collection.into(),
        id,
        body,
    });
    Ok(())
}

/// All reads share the caller's transaction. Call through the core read owner;
/// never ATTACH the target. Missing tables are explicitly recorded, not created.
/// Source byte/row budgets are supplied by the operator, not ambient env vars.
pub fn collect(
    tx: &Transaction<'_>,
    services: &mut impl ImportServices,
    max_documents: usize,
    max_bytes: usize,
) -> Result<CoreImport> {
    let mut out = CoreImport {
        documents: Vec::new(),
        dataframe_rows: Vec::new(),
        report: ImportReport::default(),
    };
    for name in [
        "ctox_ticket_items",
        "ctox_ticket_events",
        "ctox_ticket_cases",
        "ctox_ticket_approvals",
        "ctox_ticket_verifications",
        "ctox_ticket_writebacks",
        "ctox_ticket_clarification_requests",
        "iot_assets",
        "iot_attributes",
        "iot_asset_types",
        "iot_alarms",
        "iot_rulesets",
        "iot_agents",
        "iot_agent_status",
        "iot_dashboards",
        "iot_widgets",
        "ctox_crew_members",
        "ctox_crew_learnings",
        "ctox_crew_continuity",
        "ctox_crew_continuity_commits",
        "ctox_crew_assignments",
        "workjet_supervisor_bindings",
        "outbound_lead_generation_scrape_targets",
        "outbound_lead_generation_scrape_scripts",
        "outbound_lead_generation_scrape_runs",
        "outbound_lead_generation_scrape_account_states",
        "outbound_lead_generation_field_status_witnesses",
    ] {
        out.report
            .collections
            .insert(name.into(), Counts::default());
    }
    tickets(tx, &mut out)?;
    iot(tx, &mut out)?;
    crew(tx, services, &mut out)?;
    scrape(tx, services, &mut out)?;
    for mut row in rows(
        tx,
        "workjet_supervisor_bindings",
        "project_id,owner_user_id,thread_id,created_at_ms",
    )? {
        let id = text(&row, "project_id")?.to_owned();
        row["updated_at_ms"] = row["created_at_ms"].clone();
        row["id"] = id.clone().into();
        row["is_deleted"] = false.into();
        push(&mut out, "workjet_supervisor_bindings", id, row)?;
    }
    for table in [
        "ticket_items",
        "iot_assets",
        "crew_members",
        "scrape_target",
        "workjet_supervisor_bindings",
    ] {
        if !has_table(tx, table)? {
            out.report
                .diagnostics
                .push(format!("source_table_absent:{table}"));
        }
    }
    let bytes: usize = out
        .documents
        .iter()
        .map(|d| d.body.to_string().len())
        .chain(out.dataframe_rows.iter().map(|v| v.to_string().len()))
        .sum();
    if out.documents.len() + out.dataframe_rows.len() > max_documents || bytes > max_bytes {
        return Err(invalid("import_snapshot_budget_exceeded"));
    }
    out.documents
        .sort_by(|a, b| (&a.collection, &a.id).cmp(&(&b.collection, &b.id)));
    Ok(out)
}

fn tickets(conn: &Connection, out: &mut CoreImport) -> Result<()> {
    for (source,target,key,fields) in [
        ("ticket_items","ctox_ticket_items","ticket_key","ticket_key,source_system,remote_ticket_id,title,body_text,remote_status,priority,requester,metadata_json,created_at,updated_at,last_synced_at"),
        ("ticket_events","ctox_ticket_events","event_key","event_key,ticket_key,source_system,remote_event_id,direction,event_type,summary,body_text,metadata_json,external_created_at,observed_at"),
        ("ticket_cases","ctox_ticket_cases","case_id","case_id,ticket_key,label,bundle_label,bundle_version,state,approval_mode,autonomy_level,support_mode,risk_level,opened_at,updated_at,closed_at"),
        ("ticket_approvals","ctox_ticket_approvals","approval_id","approval_id,case_id,status,decided_by,rationale,created_at"),
        ("ticket_verifications","ctox_ticket_verifications","verification_id","verification_id,case_id,status,summary,created_at"),
        ("ticket_writebacks","ctox_ticket_writebacks","writeback_id","writeback_id,case_id,ticket_key,operation,payload_json,status,created_at"),
        ("ticket_clarification_requests","ctox_ticket_clarification_requests","clarification_id","clarification_id,ticket_key,case_id,work_id,target_type,target_channel,question,missing_inputs_json,unblock_criteria,status,outbound_message_key,inbound_response_key,inbound_response_body,resume_state,created_by,metadata_json,created_at,updated_at,sent_at,resolved_at"),
    ] {
        for mut row in rows(conn,source,fields)? {
            let id = text(&row,key)?.to_owned();
            for (field,target) in [("metadata_json","metadata"),("payload_json","payload"),("missing_inputs_json","missing_inputs")] {
                if row.get(field).is_some() { json_field(&mut row,field,target,if target == "missing_inputs" {json!([])} else {json!({})})?; }
            }
            common(conn,&mut row,&id)?; push(out,target,id,row)?;
        }
    }
    let mut actions = rows(
        conn,
        "ticket_execution_actions",
        "action_id,case_id,summary,created_at",
    )?;
    actions.sort_by(|a, b| {
        (a["created_at"].as_str(), a["action_id"].as_str())
            .cmp(&(b["created_at"].as_str(), b["action_id"].as_str()))
    });
    for action in actions {
        if let Some(doc) = out.documents.iter_mut().find(|d| {
            d.collection == "ctox_ticket_cases" && d.id == action["case_id"].as_str().unwrap_or("")
        }) {
            let mut action = action;
            action.as_object_mut().unwrap().remove("case_id");
            doc.body["last_execution_action"] = action;
        }
    }
    for mut row in rows(
        conn,
        "local_ticket_events",
        "event_id,ticket_id,event_type,summary,body_text,created_at,metadata_json",
    )? {
        let event = text(&row, "event_id")?.to_owned();
        let ticket = text(&row, "ticket_id")?.to_owned();
        if out.documents.iter().any(|d| {
            d.collection == "ctox_ticket_events"
                && d.body["source_system"] == "local"
                && d.body["remote_event_id"] == event
        }) {
            continue;
        }
        let id = format!("local:{event}");
        json_field(&mut row, "metadata_json", "metadata", json!({}))?;
        row["remote_event_id"] = event.into();
        row["event_key"] = id.clone().into();
        row["ticket_key"] = format!("local:{ticket}").into();
        row["source_system"] = "local".into();
        row["direction"] = if row["metadata"]["origin"] == "ctox-writeback" {
            "outbound"
        } else {
            "inbound"
        }
        .into();
        row["external_created_at"] = row["created_at"].clone();
        row["observed_at"] = row["created_at"].clone();
        common(conn, &mut row, &id)?;
        push(out, "ctox_ticket_events", id, row)?;
    }
    Ok(())
}

fn iot(conn: &Connection, out: &mut CoreImport) -> Result<()> {
    let attributes = rows(
        conn,
        "iot_attributes",
        "asset_id,name,value,value_type,timestamp_ms,meta",
    )?;
    let conditions = rows(conn,"iot_condition_state","ruleset_id,condition_index,asset_id,first_match_ms,previously_matched,previously_unmatched")?;
    let recurrences = rows(conn, "iot_recurrence_block", "scope_key,last_fired_ms")?;
    for (table,key,fields) in [
        ("iot_assets","id","id,realm,parent_id,asset_type,name,data,created_at,updated_at"),
        ("iot_asset_types","asset_type","asset_type,data,created_at,updated_at"),
        ("iot_alarms","id","id,realm,status,severity,created_ms,data,created_at,updated_at"),
        ("iot_rulesets","id","id,realm,name,enabled,data,last_fired_ms,created_at,updated_at"),
        ("iot_agents","id","id,realm,name,kind,enabled,data,created_at,updated_at"),
        ("iot_agent_status","agent_id","agent_id,realm,link_state,last_event_ms,data,created_at,updated_at"),
        ("iot_dashboards","id","id,realm,name,scope,scope_ref,view_mode,sort_index,created_at,updated_at"),
        ("iot_widgets","id","id,dashboard_id,realm,signal_ref,cond_text,action_prompt,trigger_code,trigger_state,trigger_status,render_code,x,y,w,h,sort_index,created_at,updated_at"),
    ] {
        for mut row in rows(conn,table,fields)? {
            let id = text(&row,key)?.to_owned(); boolean(&mut row,"enabled");
            if row.get("data").is_some() || ["iot_agents","iot_rulesets"].contains(&table) { json_field(&mut row,"data","data",json!({}))?; }
            if table == "iot_assets" {
                let mut attrs = Map::new();
                for a in attributes.iter().filter(|a| a["asset_id"] == id) {
                    let attr = attribute(a.clone())?; attrs.insert(text(&attr,"name")?.into(),attr);
                }
                row["data"]["attributes"] = Value::Object(attrs); row["index_text"] = text(&row,"name")?.to_lowercase().into(); row["sort_key"] = row["name"].clone();
            }
            if table == "iot_asset_types" { row["attribute_count"] = json!(row["data"]["attributes"].as_array().map_or(0,|v|v.len())); }
            if table == "iot_alarms" { row["status_key"] = row["status"].clone(); row["source"] = row["data"]["source"].clone(); row["sort_key"] = format!("{:020}",row["created_ms"].as_i64().unwrap_or(0)).into(); }
            if table == "iot_widgets" {
                if row.get("trigger_state").is_some() { json_field(&mut row,"trigger_state","trigger_state",json!({}))?; }
                if row["trigger_code"].as_str().is_some_and(|s|!s.trim().is_empty()) { row["trigger_status"] = "needs_attention".into(); }
            }
            if table == "iot_rulesets" {
                let mut state = Map::new();
                for condition in conditions.iter().filter(|r|r["ruleset_id"] == id) {
                    let entry = state.entry(text(condition,"asset_id")?.to_owned()).or_insert_with(||json!({"matched":false}));
                    // Native marks the primary condition; OR prevents a later
                    // sibling row from accidentally re-arming an imported rule.
                    entry["matched"] = (entry["matched"] == true || condition["previously_matched"].as_i64().unwrap_or(0)!=0).into();
                    if condition["previously_unmatched"].as_i64().unwrap_or(0)!=0 { entry["unmatched"] = true.into(); }
                    if let Some(first) = condition["first_match_ms"].as_i64() {
                        if !entry["windows"].is_object() { entry["windows"] = json!({}); }
                        let index = condition["condition_index"].as_i64().ok_or_else(||invalid("iot_condition_index_missing"))?;
                        entry["windows"][index.to_string()] = first.into();
                    }
                }
                if row["data"]["recurrence"]["scope"] == "GLOBAL" {
                    let matched = state.values().any(|s|s["matched"] == true);
                    state.insert("*".into(),json!({"matched":matched}));
                }
                for recurrence in &recurrences {
                    let scope = text(recurrence,"scope_key")?;
                    if scope == id {
                        for entry in state.values_mut() { entry["fired_ms"] = recurrence["last_fired_ms"].clone(); }
                        state.entry("*".to_owned()).or_insert_with(||json!({"matched":false}))["fired_ms"] = recurrence["last_fired_ms"].clone();
                    } else if let Some(asset) = scope.strip_prefix(&format!("{id}:")) {
                        state.entry(asset.to_owned()).or_insert_with(||json!({"matched":false}))["fired_ms"] = recurrence["last_fired_ms"].clone();
                    }
                }
                row["match_state"] = Value::Object(state);
            }
            common(conn,&mut row,&id)?; push(out,table,id,row)?;
        }
    }
    for row in attributes {
        let id = format!("{}:{}", text(&row, "asset_id")?, text(&row, "name")?);
        let attr = attribute(row.clone())?;
        let body = json!({"id":id,"asset_id":row["asset_id"],"attribute_name":row["name"],"value_type":row["value_type"],"timestamp_ms":row["timestamp_ms"],"data":attr,"status_key":row["value_type"],"updated_at_ms":row["timestamp_ms"],"is_deleted":false});
        push(out, "iot_attributes", id, body)?;
    }
    out.dataframe_rows = rows(
        conn,
        "iot_datapoints",
        "asset_id,attribute_name,timestamp_ms,value",
    )?;
    for row in &mut out.dataframe_rows {
        json_field(row, "value", "value", Value::Null)?;
    }
    out.report
        .collections
        .entry("dataframe:iot_timeseries".into())
        .or_default()
        .source_rows = out.dataframe_rows.len();
    Ok(())
}
fn attribute(mut row: Value) -> Result<Value> {
    json_field(&mut row, "value", "value", Value::Null)?;
    json_field(&mut row, "meta", "meta", json!({}))?;
    row["timestamp"] = row["timestamp_ms"].clone();
    row.as_object_mut().unwrap().remove("asset_id");
    row.as_object_mut().unwrap().remove("timestamp_ms");
    Ok(row)
}

// Same continuity entry framing as crew::entry_lines (one-time import only).
fn memory_count(content: &str, start: &str, primary: &str) -> usize {
    let mut current = false;
    let mut has_primary = false;
    let mut count = 0;
    for line in content.lines() {
        let line = line.trim().trim_start_matches("- ").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key == start {
            count += usize::from(current && has_primary);
            current = true;
            has_primary = false;
        }
        if current && key == primary && !value.trim().is_empty() {
            has_primary = true;
        }
    }
    count + usize::from(current && has_primary)
}

fn crew(conn: &Connection, services: &impl ImportServices, out: &mut CoreImport) -> Result<()> {
    let assignments = rows(
        conn,
        "communication_routing_state",
        "message_key,crew_assigned_member_id,crew_member_id,route_status,leased_at,updated_at",
    )?;
    let attempts = rows(
        conn,
        "crew_attempts",
        "attempt_id,member_id,module,finalized_at,succeeded",
    )?;
    for mut row in rows(conn,"crew_members","id,name,shape,color,archived,soul_json,specialties_json,stats_json,created_at,updated_at,last_memory_read_at,last_learning_at")? {
        let id = text(&row,"id")?.to_owned();
        let parsed = (|| { for (a,b) in [("soul_json","soul"),("specialties_json","specialties"),("stats_json","stats")] { json_field(&mut row,a,b,Value::Null)?; } Ok::<_,StoreError>(()) })();
        if parsed.is_err() { let count = out.report.collections.entry("ctox_crew_members".into()).or_default(); count.source_rows += 1; count.rejected += 1; out.report.diagnostics.push(format!("member_invalid_json:{id}")); continue; }
        boolean(&mut row,"archived");
        let active = assignments.iter().filter(|r|r["crew_member_id"] == id && r["route_status"] == "leased").min_by_key(|r|r["leased_at"].as_str().unwrap_or(""));
        row["active_task_id"] = active.map(|r|r["message_key"].clone()).unwrap_or(Value::Null);
        row["state"] = if active.is_some() {"on_duty"} else {"home"}.into();
        let mut domains: BTreeMap<String,usize> = BTreeMap::new();
        for attempt in attempts.iter().filter(|r|r["member_id"] == id && r["succeeded"] == 1) { if let Some(module) = attempt["module"].as_str() { *domains.entry(module.into()).or_default() += 1; } }
        let mut domains: Vec<_> = domains.into_iter().collect(); domains.sort_by(|a,b|b.1.cmp(&a.1).then(a.0.cmp(&b.0))); row["domain"] = json!(domains.into_iter().take(3).map(|(d,_)|d).collect::<Vec<_>>());
        for (a,b) in [("last_memory_read_at","last_memory_read_at_ms"),("last_learning_at","last_learning_at_ms")] { if let Some(value) = row.get(a) { row[b] = millis(conn,value)?.into(); } }
        let conversation = services.member_conversation_id(&id);
        let mut memory = json!({"anchors":"","narrative":"","anchor_count":0,"experience_count":0,"updated_at":""});
        for document in rows(conn,"continuity_documents","document_id,conversation_id,kind,head_commit_id,created_at,updated_at")?.into_iter().filter(|r|r["conversation_id"] == conversation) {
            let kind = text(&document,"kind")?; if !["anchors","narrative"].contains(&kind) {continue;}
            let new_id = format!("contdoc_crew:{id}_{kind}"); let old_base = format!("contbase_{conversation}_{kind}"); let new_base = format!("contbase_crew:{id}_{kind}");
            let rewrite = |value: &str| if value == old_base {new_base.clone()} else {value.to_owned()};
            let all = rows(conn,"continuity_commits","commit_id,document_id,parent_commit_id,diff_text,rendered_text,created_at")?;
            let mut head = text(&document,"head_commit_id")?.to_owned(); let first = head.clone(); let mut seen = BTreeSet::new();
            while !head.is_empty() {
                if !seen.insert(head.clone()) {return Err(invalid("continuity_cycle"));}
                let mut commit = all.iter().find(|r|r["commit_id"] == head && r["document_id"] == document["document_id"]).cloned().ok_or_else(||invalid("continuity_parent_missing"))?;
                let original = text(&commit,"commit_id")?.to_owned(); let next = commit["parent_commit_id"].as_str().unwrap_or("").to_owned();
                if original == first { memory[kind] = commit["rendered_text"].clone(); }
                let commit_id = rewrite(&original); commit["commit_id"] = commit_id.clone().into(); commit["document_id"] = new_id.clone().into(); commit["member_id"] = id.clone().into(); commit["kind"] = kind.into();
                if !next.is_empty() { commit["parent_commit_id"] = rewrite(&next).into(); }
                common(conn,&mut commit,&commit_id)?; push(out,"ctox_crew_continuity_commits",commit_id,commit)?; head = next;
            }
            let mut doc = document.clone(); doc["member_id"] = id.clone().into(); doc["head_commit_id"] = rewrite(&first).into(); doc["content"] = memory[kind].clone(); common(conn,&mut doc,&new_id)?;
            push(out,"ctox_crew_continuity",new_id,doc)?;
            memory["updated_at"] = document["updated_at"].clone();
        }
        memory["anchor_count"] = memory_count(memory["anchors"].as_str().unwrap_or(""),"anchor_id","statement").into();
        memory["experience_count"] = memory_count(memory["narrative"].as_str().unwrap_or(""),"entry_id","summary").into();
        row["memory"] = memory; common(conn,&mut row,&id)?; push(out,"ctox_crew_members",id,row)?;
    }
    for mut row in rows(conn,"crew_member_learnings","id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at,confirmed_by_owner,archived")? {
        let id = text(&row,"id")?.to_owned(); json_field(&mut row,"scope_json","scope",json!({}))?;
        boolean(&mut row,"confirmed_by_owner"); boolean(&mut row,"archived");
        if has_table(conn,"worker_run_identities")? {
            let projected: Option<String> = sql(conn.query_row("SELECT run_id FROM worker_run_identities WHERE attempt_id=?1",[text(&row,"evidence_run_id")?],|r|r.get(0)).optional())?;
            if let Some(run) = projected {row["evidence_run_id"] = run.into();}
        }
        common(conn,&mut row,&id)?; push(out,"ctox_crew_learnings",id,row)?;
    }
    for row in assignments
        .into_iter()
        .filter(|r| r["crew_assigned_member_id"].as_str().is_some())
    {
        let id = text(&row, "message_key")?.to_owned();
        let stamp = millis(conn, &row["updated_at"])?;
        push(
            out,
            "ctox_crew_assignments",
            id.clone(),
            json!({"id":id,"task_id":id,"member_id":row["crew_assigned_member_id"],"assigned_at":row["updated_at"],"updated_at_ms":stamp,"is_deleted":false}),
        )?;
    }
    Ok(())
}

fn scrape(
    conn: &Connection,
    services: &mut impl ImportServices,
    out: &mut CoreImport,
) -> Result<()> {
    const PREFIX: &str = "outbound_lead_generation_";
    let targets = rows(conn,"scrape_target","target_id,target_key,display_name,start_url,target_kind,status,schedule_hint,config_json,output_schema_json,latest_script_revision_no,latest_script_sha256,created_at,updated_at")?;
    let key_for = |target: &Value| -> Result<String> {
        targets
            .iter()
            .find(|r| r["target_id"] == *target)
            .and_then(|r| r["target_key"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| invalid("scrape_target_missing"))
    };
    let mut registered = BTreeSet::new();
    for mut row in rows(conn,"scrape_script_revision","target_id,revision_no,language,entry_command_json,script_sha256,script_body,script_path,change_reason,notes,created_at")? {
        let key = key_for(&row["target_id"])?; let revision = row["revision_no"].as_i64().ok_or_else(||invalid("script_revision_missing"))?;
        let id = format!("{key}:r{revision}"); let sha = text(&row,"script_sha256")?.to_owned();
        row["target_key"] = key.clone().into();
        let Some((reference,size)) = services.script(&id,text(&row,"script_body")?.trim(),&sha)? else {
            let count = out.report.collections.entry(format!("{PREFIX}scrape_scripts")).or_default(); count.source_rows += 1; count.rejected += 1;
            out.report.diagnostics.push(format!("script_digest_mismatch:{id}")); continue;
        };
        if reference != sha { return Err(invalid("files_ref_must_equal_sha256")); }
        row.as_object_mut().unwrap().remove("script_body"); row["script_ref"] = reference.into(); row["size_bytes"] = size.into();
        json_field(&mut row,"entry_command_json","entry_command",json!([]))?;
        if let Some(path) = row["script_path"].as_str() {
            if let Some(materialized) = services.artifact(path,Some(&sha))? {
                if materialized != sha { return Err(invalid("registered_script_digest_mismatch")); }
                registered.insert((key,revision,sha));
            }
        }
        row.as_object_mut().unwrap().remove("script_path"); common(conn,&mut row,&id)?; push(out,&format!("{PREFIX}scrape_scripts"),id,row)?;
    }
    for mut row in targets.clone() {
        let id = text(&row, "target_key")?.to_owned();
        json_field(&mut row, "config_json", "config", json!({}))?;
        json_field(&mut row, "output_schema_json", "output_schema", json!({}))?;
        row["workspace_dir"] = format!("outbound-lead-generation/scrape/targets/{id}").into();
        row["script_registered"] = registered
            .contains(&(
                id.clone(),
                row["latest_script_revision_no"].as_i64().unwrap_or(0),
                row["latest_script_sha256"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
            ))
            .into();
        common(conn, &mut row, &id)?;
        push(out, &format!("{PREFIX}scrape_targets"), id, row)?;
    }
    let artifacts = rows(
        conn,
        "scrape_artifact",
        "run_id,artifact_kind,path,content_sha256,record_count",
    )?;
    for mut row in rows(conn,"scrape_run","run_id,target_id,trigger_kind,started_at,finished_at,status,script_revision_no,script_sha256,result_json,output_dir,created_at")? {
        let id = text(&row,"run_id")?.to_owned(); row["target_key"] = key_for(&row["target_id"])?.into(); json_field(&mut row,"result_json","result",json!({}))?;
        let mut imported = Vec::new();
        for artifact in artifacts.iter().filter(|a|a["run_id"] == id) {
            let reference = services.artifact(text(artifact,"path")?,artifact["content_sha256"].as_str())?;
            if reference.is_none() { out.report.diagnostics.push(format!("scrape_artifact_missing:{id}")); }
            imported.push(json!({"artifact_kind":artifact["artifact_kind"],"ref":reference,"content_sha256":artifact["content_sha256"],"record_count":artifact["record_count"]}));
        }
        row["artifacts"] = imported.into(); row["run_manifest_ref"] = match row["output_dir"].as_str() { Some(dir)=>services.artifact(&format!("{dir}/run.json"),None)?.into(),None=>Value::Null };
        row.as_object_mut().unwrap().remove("output_dir"); common(conn,&mut row,&id)?; push(out,&format!("{PREFIX}scrape_runs"),id,row)?;
    }
    for mut row in rows(conn,"scrape_account_state","target_id,generation,reason,causal_run_id,last_probe_run_id,credential_ref,credential_version,failed_probes,next_probe_at_ms,probe_lease_owner,probe_lease_until_ms,updated_at_ms")? {
        let id = key_for(&row["target_id"])?; row["id"] = id.clone().into(); row["target_key"] = id.clone().into(); row["is_deleted"] = false.into(); push(out,&format!("{PREFIX}scrape_account_states"),id,row)?;
    }
    let mut witnesses: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for mut row in rows(
        conn,
        "outbound_native_field_status_witnesses",
        "record_id,scope,person_key,field,status_json",
    )? {
        let id = text(&row, "record_id")?.to_owned();
        row.as_object_mut().unwrap().remove("record_id");
        json_field(&mut row, "status_json", "status", json!({}))?;
        witnesses.entry(id).or_default().push(row);
    }
    for (id, mut rows) in witnesses {
        rows.sort_by_key(Value::to_string);
        push(
            out,
            &format!("{PREFIX}field_status_witnesses"),
            id.clone(),
            json!({"id":id,"record_id":id,"witnesses":rows,"is_deleted":false}),
        )?;
    }
    Ok(())
}

/// Separate mail database snapshot, selected positively. In particular the DKIM
/// private key and password_hash columns never enter Rust memory.
pub fn mail_server(tx: &Transaction<'_>) -> Result<Vec<Document>> {
    let mut docs = Vec::new();
    for (table, fields, kind, key) in [
        (
            "stalwart_domains",
            "domain_name,dkim_selector,spf_record,dmarc_record",
            "domain",
            "domain_name",
        ),
        ("stalwart_users", "username,created_at", "user", "username"),
        (
            "stalwart_runtime_config",
            "id,enabled,updated_at",
            "runtime",
            "id",
        ),
    ] {
        for mut data in rows(tx, table, fields)? {
            let name = data[key]
                .as_str()
                .map(str::to_owned)
                .or_else(|| data[key].as_i64().map(|n| n.to_string()))
                .ok_or_else(|| invalid("mail_server_identity_missing"))?;
            boolean(&mut data, "enabled");
            if kind == "runtime" {
                // JSON paths are allowlisted too: never SELECT config_json.
                let runtime: Option<String> = sql(tx.query_row("SELECT json_object('hostname',json_extract(config_json,'$.hostname'),'bind_host',json_extract(config_json,'$.bind_host'),'smtp_port',json_extract(config_json,'$.smtp_port'),'imap_port',json_extract(config_json,'$.imap_port'),'outbound_throttle_per_min',json_extract(config_json,'$.outbound_throttle_per_min'),'max_connections',json_extract(config_json,'$.max_connections'),'tracking_base_url',json_extract(config_json,'$.tracking_base_url')) FROM stalwart_runtime_config WHERE id=1",[],|r|r.get(0)).optional())?;
                if let Some(runtime) = runtime {
                    let values: Value = serde_json::from_str(&runtime)
                        .map_err(|_| invalid("mail_runtime_config_invalid"))?;
                    for (key, value) in values
                        .as_object()
                        .ok_or_else(|| invalid("mail_runtime_config_invalid"))?
                    {
                        if !value.is_null() {
                            data[key] = value.clone();
                        }
                    }
                }
            }
            let id = format!("{kind}:{name}");
            docs.push(Document {
                revision: None,
                collection: "mail_server_state".into(),
                id: id.clone(),
                body: json!({"id":id,"kind":kind,"name":name,"data":data}),
            });
        }
    }
    Ok(docs)
}
