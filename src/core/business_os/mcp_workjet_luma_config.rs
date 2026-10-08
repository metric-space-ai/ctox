// Origin: CTOX
// License: AGPL-3.0-only

//! Instance-wide Luma configuration. The CTOX instance owns one document; every
//! Workjet client reads and writes it through this typed tool, so the revision
//! fences concurrent writers. Computers are deliberately not part of the
//! document: they stay per environment.
use super::*;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde_json::json;

pub(super) const READ_TOOL: &str = "business_os.luma_configuration_read";
pub(super) const WRITE_TOOL: &str = "business_os.luma_configuration_update";

const COLLECTION: &str = "workjet_luma_configuration";
const RECORD_ID: &str = "instance";
const DOCUMENT_SCHEMA_VERSION: i64 = 1;
const MAX_CONFIGURATION_BYTES: usize = 1024 * 1024;
/// Keys that describe the whole instance. Anything else, notably `computers`
/// and `selectedComputerId`, is refused so machine state cannot leak in.
const INSTANCE_KEYS: &[&str] = &[
    "workerProfiles",
    "llmRoutes",
    "modelPrompts",
    "managedSystemPrompt",
    "managerThreadReference",
    "workerGraph",
    "execution",
    "telemetry",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    expected_revision: u64,
    configuration: Value,
}

pub(super) fn read_descriptor() -> BusinessOsMcpToolDescriptor {
    read_tool(
        READ_TOOL,
        "Read the instance-wide Luma configuration of this CTOX instance: Luma profiles, LLM routes, model prompts, worker graph, execution and telemetry settings. Returns the current revision, which the next update must pass as expected_revision. Revision 0 means no configuration has been stored yet.",
        json!({"type":"object","additionalProperties":false,"properties":{}}),
    )
}

pub(super) fn write_descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(
        WRITE_TOOL,
        "Replace the instance-wide Luma configuration. expected_revision must equal the revision last read (0 for the first write). A stale revision returns a conflict with the current revision and writes nothing. Keys outside the instance-wide set, such as computers, are refused.",
        json!({"type":"object","additionalProperties":false,"required":["expected_revision","configuration"],
            "properties":{
                "expected_revision":{"type":"integer","minimum":0},
                "configuration":{"type":"object"}}}),
    )
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    match tool_name {
        READ_TOOL => read(root, context),
        WRITE_TOOL => save(root, context, args),
        _ => anyhow::bail!("unsupported Luma configuration tool"),
    }
}

fn read(root: &Path, context: &McpChannelRequestContext) -> anyhow::Result<Value> {
    let conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    require_workspace(&conn, context, BusinessOsPermission::DataRead)?;
    let record = store::outbound_load_record(&conn, COLLECTION, RECORD_ID)?;
    Ok(match record {
        Some(record) => json!({
            "ok": true,
            "revision": revision_of(&record),
            "configuration": record["configuration"],
            "updated_at_ms": record["updated_at_ms"],
        }),
        None => json!({"ok": true, "revision": 0, "configuration": null, "updated_at_ms": null}),
    })
}

fn save(root: &Path, context: &McpChannelRequestContext, args: &Value) -> anyhow::Result<Value> {
    let request: SaveRequest =
        serde_json::from_value(args.clone()).context("invalid Luma configuration request")?;
    validate_configuration(&request.configuration)?;
    let mut conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_workspace(&tx, context, BusinessOsPermission::RuntimeManage)?;
    let current_revision = store::outbound_load_record(&tx, COLLECTION, RECORD_ID)?
        .as_ref()
        .map_or(0, revision_of);
    if request.expected_revision != current_revision {
        return Ok(json!({
            "ok": false,
            "conflict": true,
            "revision": current_revision,
        }));
    }
    let revision = current_revision + 1;
    store::upsert_business_record(
        &tx,
        COLLECTION,
        RECORD_ID,
        store::now_ms() as i64,
        json!({
            "schema_version": DOCUMENT_SCHEMA_VERSION,
            "revision": revision,
            "configuration": request.configuration,
            "updated_by": context.actor,
        }),
    )?;
    tx.commit()?;
    Ok(json!({"ok": true, "revision": revision}))
}

fn revision_of(record: &Value) -> u64 {
    record["revision"].as_u64().unwrap_or(0)
}

fn validate_configuration(value: &Value) -> anyhow::Result<()> {
    let object = value
        .as_object()
        .context("Luma configuration must be an object")?;
    for key in object.keys() {
        anyhow::ensure!(
            INSTANCE_KEYS.contains(&key.as_str()),
            "Luma configuration key `{key}` is not instance-wide"
        );
    }
    anyhow::ensure!(
        serde_json::to_vec(value)?.len() <= MAX_CONFIGURATION_BYTES,
        "Luma configuration exceeds its size budget"
    );
    Ok(())
}

fn require_workspace(
    conn: &Connection,
    context: &McpChannelRequestContext,
    permission: BusinessOsPermission,
) -> anyhow::Result<()> {
    let role = context
        .trusted_role
        .as_deref()
        .context("trusted role missing for Luma configuration")?;
    anyhow::ensure!(
        super::super::store_policy::trusted_actor_policy_decision_with_conn(
            conn,
            &context.actor,
            role,
            permission,
            BusinessOsScopeType::Workspace,
            None,
        )?
        .allowed,
        "Luma configuration policy denied"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_instance_wide_keys_only() {
        assert!(validate_configuration(&json!({
            "workerProfiles": [],
            "llmRoutes": [],
            "execution": {"probeTimeoutSeconds": 120}
        }))
        .is_ok());
        let refused = validate_configuration(&json!({"computers": []}))
            .expect_err("computers are per environment");
        assert!(refused.to_string().contains("not instance-wide"));
        assert!(validate_configuration(&json!(["not", "an", "object"])).is_err());
    }

    #[test]
    fn rejects_oversized_configuration() {
        let padding = "x".repeat(MAX_CONFIGURATION_BYTES);
        assert!(validate_configuration(&json!({"managedSystemPrompt": padding})).is_err());
    }

    #[test]
    fn descriptors_require_revision_fence() {
        let write = write_descriptor();
        assert_eq!(write.name, WRITE_TOOL);
        assert_eq!(
            write.input_schema["required"],
            json!(["expected_revision", "configuration"])
        );
        assert!(read_descriptor().input_schema["properties"]
            .as_object()
            .is_some_and(|properties| properties.is_empty()));
    }
}
