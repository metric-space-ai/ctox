// Origin: CTOX
// License: AGPL-3.0-only
//! Private native meeting reports. No projection or browser data transport.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) const SCHEMA: &str = "ctox.workjet.meeting_report.v1";
pub(crate) const SCHEMA_JSON: &str = include_str!("fixtures/meeting-report.v1.schema.json");
const TABLE: &str = "CREATE TABLE IF NOT EXISTS workjet_meeting_reports (
 meeting_id TEXT NOT NULL, project_id TEXT NOT NULL, revision INTEGER NOT NULL,
 report_json TEXT NOT NULL, created_at TEXT NOT NULL, operation_id TEXT NOT NULL UNIQUE,
 PRIMARY KEY(meeting_id, revision));";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaveReport {
    pub operation_id: String,
    pub meeting_id: String,
    pub report: Value,
}

pub(crate) fn report_schema() -> Value {
    serde_json::from_str(SCHEMA_JSON).expect("embedded meeting report schema")
}

/// Evaluate every assertion used by the pinned, local-only schema. Unknown
/// keywords fail closed, so extending the contract cannot silently skip checks.
/// This is intentionally not an arbitrary or remotely resolving schema engine.
fn validate_at(root: &Value, schema: &Value, value: &Value, path: &str) -> Result<()> {
    let object = schema
        .as_object()
        .context("invalid embedded report schema")?;
    for key in object.keys() {
        anyhow::ensure!(
            matches!(
                key.as_str(),
                "$schema"
                    | "$id"
                    | "$defs"
                    | "title"
                    | "description"
                    | "$ref"
                    | "type"
                    | "const"
                    | "enum"
                    | "required"
                    | "properties"
                    | "additionalProperties"
                    | "items"
                    | "minItems"
                    | "minLength"
                    | "maxLength"
                    | "minimum"
                    | "format"
            ),
            "unsupported embedded report schema keyword: {key}"
        );
    }
    if let Some(reference) = schema["$ref"].as_str() {
        let pointer = reference
            .strip_prefix('#')
            .context("external report schema reference denied")?;
        validate_at(
            root,
            root.pointer(pointer)
                .context("invalid report schema reference")?,
            value,
            path,
        )?;
    }
    if let Some(kind) = schema["type"].as_str() {
        let valid = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            _ => anyhow::bail!("unsupported embedded report type: {kind}"),
        };
        anyhow::ensure!(valid, "report {path}: expected {kind}");
    }
    if let Some(expected) = schema.get("const") {
        anyhow::ensure!(value == expected, "report {path}: const mismatch");
    }
    if let Some(choices) = schema["enum"].as_array() {
        anyhow::ensure!(choices.contains(value), "report {path}: value outside enum");
    }
    if let Some(values) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for field in required {
                let field = field.as_str().context("invalid report required field")?;
                anyhow::ensure!(
                    values.contains_key(field),
                    "report {path}/{field}: required field missing"
                );
            }
        }
        for (name, child) in values {
            let child_path = format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"));
            if let Some(child_schema) = schema["properties"].get(name) {
                validate_at(root, child_schema, child, &child_path)?;
            } else {
                anyhow::ensure!(
                    schema["additionalProperties"] != false,
                    "report {child_path}: unknown field"
                );
            }
        }
    }
    if let Some(values) = value.as_array() {
        if let Some(minimum) = schema["minItems"].as_u64() {
            anyhow::ensure!(
                values.len() as u64 >= minimum,
                "report {path}: too few items"
            );
        }
        if let Some(items) = schema.get("items") {
            for (index, child) in values.iter().enumerate() {
                validate_at(root, items, child, &format!("{path}/{index}"))?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        for (keyword, is_min) in [("minLength", true), ("maxLength", false)] {
            if let Some(bound) = schema[keyword].as_u64() {
                let size = text.chars().count() as u64;
                anyhow::ensure!(
                    if is_min { size >= bound } else { size <= bound },
                    "report {path}: {keyword} violated"
                );
            }
        }
        if let Some(format) = schema["format"].as_str() {
            let valid = match format {
                "date-time" => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
                "uri" => url::Url::parse(text).is_ok(),
                _ => anyhow::bail!("unsupported embedded report format: {format}"),
            };
            anyhow::ensure!(valid, "report {path}: invalid {format}");
        }
    }
    if let Some(minimum) = schema["minimum"].as_f64() {
        anyhow::ensure!(
            value.as_f64().is_some_and(|n| n >= minimum),
            "report {path}: below minimum"
        );
    }
    Ok(())
}

pub(crate) fn validate(report: &Value) -> Result<()> {
    anyhow::ensure!(
        serde_json::to_vec(report)?.len() <= 256 * 1024,
        "report /: exceeds 256 KiB"
    );
    let schema = report_schema();
    validate_at(&schema, &schema, report, "")
}

fn table_exists(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_meeting_reports')", [], |row| row.get(0))?)
}
fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, i64, String, String, String)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}
fn record(v: (String, String, i64, String, String, String)) -> Result<Value> {
    Ok(json!({"meeting_id":v.0,"project_id":v.1,"revision":v.2,
        "report":serde_json::from_str::<Value>(&v.3)?,"created_at":v.4,"operation_id":v.5}))
}

pub(crate) fn save(
    conn: &Connection,
    request: &SaveReport,
    meeting_id: &str,
    project_id: &str,
    supervisor: &str,
) -> Result<Value> {
    anyhow::ensure!(
        !request.operation_id.is_empty()
            && request.operation_id.len() <= 128
            && request.operation_id.trim() == request.operation_id,
        "report operation_id must be canonical and bounded"
    );
    validate(&request.report)?;
    anyhow::ensure!(
        request.meeting_id == meeting_id && request.report["meeting"]["id"] == meeting_id,
        "report /meeting/id: differs from authorized meeting"
    );
    anyhow::ensure!(
        request.report["project"]["id"] == project_id,
        "report /project/id: differs from authorized project"
    );
    anyhow::ensure!(
        request.report["generated"]["by"] == supervisor,
        "report /generated/by: differs from registered supervisor"
    );
    conn.execute_batch(TABLE)?;
    let prior = conn.query_row(
        "SELECT meeting_id,project_id,revision,report_json,created_at,operation_id FROM workjet_meeting_reports WHERE operation_id=?1",
        [&request.operation_id], row).optional()?;
    if let Some(prior) = prior {
        let prior = record(prior)?;
        anyhow::ensure!(
            prior["meeting_id"] == meeting_id
                && prior["project_id"] == project_id
                && prior["report"] == request.report,
            "report operation_id {} conflicts with previously stored content",
            request.operation_id
        );
        return Ok(prior);
    }
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM workjet_meeting_reports WHERE meeting_id=?1",
        [meeting_id],
        |r| r.get(0),
    )?;
    let raw = serde_json::to_string(&request.report)?;
    conn.execute("INSERT INTO workjet_meeting_reports(meeting_id,project_id,revision,report_json,created_at,operation_id)
        VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now'),?5)",
        params![meeting_id,project_id,revision,raw,request.operation_id])?;
    Ok(record(conn.query_row(
        "SELECT meeting_id,project_id,revision,report_json,created_at,operation_id FROM workjet_meeting_reports WHERE operation_id=?1",
        [&request.operation_id], row)?)?)
}

pub(crate) fn latest(conn: &Connection, meeting_id: &str) -> Result<Value> {
    if !table_exists(conn)? {
        return Ok(Value::Null);
    }
    conn.query_row(
        "SELECT meeting_id,project_id,revision,report_json,created_at,operation_id
        FROM workjet_meeting_reports WHERE meeting_id=?1 ORDER BY revision DESC LIMIT 1",
        [meeting_id],
        row,
    )
    .optional()?
    .map(record)
    .transpose()
    .map(|v| v.unwrap_or(Value::Null))
}

/// Operator-local, strictly read-only JSON command. No migrations or secret access.
pub(crate) fn cli(root: &Path, args: &[String]) -> Result<Value> {
    anyhow::ensure!(
        args.first().map(String::as_str) == Some("report"),
        "usage: ctox jour-fixe report show --meeting-id ID | list --project-id ID [--limit 1..100]"
    );
    let action = args
        .get(1)
        .map(String::as_str)
        .context("report action required")?;
    let mut meeting = None;
    let mut project = None;
    let mut limit = 20_u32;
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 2;
    while i < args.len() {
        let flag = &args[i];
        anyhow::ensure!(seen.insert(flag.clone()), "duplicate report option: {flag}");
        let value = args.get(i + 1).context("report option value required")?;
        match flag.as_str() {
            "--meeting-id" => meeting = Some(value.as_str()),
            "--project-id" => project = Some(value.as_str()),
            "--limit" => limit = value.parse().context("report limit must be an integer")?,
            _ => anyhow::bail!("unknown report option: {flag}"),
        }
        i += 2;
    }
    anyhow::ensure!((1..=100).contains(&limit), "report limit must be 1..100");
    match action {
        "show" => anyhow::ensure!(
            meeting.is_some_and(|v| !v.trim().is_empty())
                && project.is_none()
                && !seen.contains("--limit"),
            "show requires only --meeting-id"
        ),
        "list" => anyhow::ensure!(
            project.is_some_and(|v| !v.trim().is_empty()) && meeting.is_none(),
            "list requires --project-id"
        ),
        _ => anyhow::bail!("unknown report action: {action}"),
    }
    let path = super::store::business_os_store_path(root);
    if !path.exists() {
        return Ok(if action == "show" {
            Value::Null
        } else {
            json!([])
        });
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    if action == "show" {
        return latest(&conn, meeting.unwrap());
    }
    if !table_exists(&conn)? {
        return Ok(json!([]));
    }
    let mut statement = conn.prepare("SELECT meeting_id,project_id,revision,report_json,created_at,operation_id FROM workjet_meeting_reports
        WHERE project_id=?1 ORDER BY created_at DESC,meeting_id,revision DESC LIMIT ?2")?;
    let rows = statement.query_map(params![project.unwrap(), limit], row)?;
    Ok(Value::Array(
        rows.map(|v| record(v?)).collect::<Result<Vec<_>>>()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_example_validates_and_all_schema_assertions_reject_bad_values() -> Result<()> {
        let example: Value =
            serde_json::from_str(include_str!("fixtures/meeting-report-example.json"))?;
        validate(&example)?;
        for (path, invalid) in [
            ("/overall/status", json!("invented")),
            ("/period/from", json!("yesterday")),
            ("/project/repository_url", json!("relative")),
            ("/meeting/deck_revision", json!(0)),
            ("/dimensions", json!([])),
            ("/overall/summary", json!("x".repeat(281))),
            ("/progress/0/evidence", json!([])),
            ("/protocol/notes", json!("not an array")),
            ("/schema", json!("wrong")),
            ("/report_id", json!("")),
        ] {
            let mut value = example.clone();
            *value.pointer_mut(path).unwrap() = invalid;
            let error = validate(&value).unwrap_err().to_string();
            assert!(error.contains(path), "{error}");
        }
        let mut value = example.clone();
        value["project"]["unknown"] = json!(true);
        assert!(validate(&value)
            .unwrap_err()
            .to_string()
            .contains("/project/unknown"));
        value = example;
        value.as_object_mut().unwrap().remove("protocol");
        assert!(validate(&value)
            .unwrap_err()
            .to_string()
            .contains("/protocol"));
        Ok(())
    }
}
