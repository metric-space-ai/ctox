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
const MAX_REVISION: u64 = 9_007_199_254_740_991;
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {}

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
                "expected_revision":{"type":"integer","minimum":0,"maximum":MAX_REVISION},
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
        READ_TOOL => {
            let _: ReadRequest = serde_json::from_value(args.clone())
                .context("invalid Luma configuration read request")?;
            read(root, context)
        }
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
            "revision": revision_of(&record)?,
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
    anyhow::ensure!(
        request.expected_revision <= MAX_REVISION,
        "invalid Luma revision"
    );
    let mut conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_workspace(&tx, context, BusinessOsPermission::RuntimeManage)?;
    let current_revision = store::outbound_load_record(&tx, COLLECTION, RECORD_ID)?
        .as_ref()
        .map(revision_of)
        .transpose()?
        .unwrap_or(0);
    if request.expected_revision != current_revision {
        return Ok(json!({
            "ok": false,
            "conflict": true,
            "revision": current_revision,
        }));
    }
    let revision = current_revision
        .checked_add(1)
        .filter(|revision| *revision <= MAX_REVISION)
        .context("Luma configuration revision exhausted")?;
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

fn revision_of(record: &Value) -> anyhow::Result<u64> {
    record["revision"]
        .as_u64()
        .filter(|revision| *revision > 0 && *revision <= MAX_REVISION)
        .context("invalid stored Luma configuration revision")
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
    // The MCP result includes both structured data and a text representation.
    // Reject an unreadable document before persisting it; no client should
    // successfully save a value that exceeds the canonical response budget.
    let receipt = mcp_tool_result(json!({
        "ok": true,
        "revision": MAX_REVISION,
        "configuration": value,
        "updated_at_ms": i64::MAX,
    }))?;
    anyhow::ensure!(
        serde_json::to_vec(&receipt)?.len() + 1024 <= MAX_MCP_RESPONSE_BYTES,
        "Luma configuration exceeds its readable MCP response budget"
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
    fn large_configuration_is_rejected_before_it_becomes_unreadable() {
        assert!(validate_configuration(&json!({
            "managedSystemPrompt": "x".repeat(200_000)
        })).is_err());
        assert!(validate_configuration(&json!({
            "managedSystemPrompt": "Shared instructions"
        })).is_ok());
    }

    fn fixture() -> anyhow::Result<tempfile::TempDir> {
        let root = tempfile::tempdir()?;
        store::tests::seed_business_user(root.path(), "owner", "chef")?;
        store::tests::seed_business_user(root.path(), "reader", "user")?;
        save_mcp_policy(root.path(), &default_mcp_policy())?;
        Ok(root)
    }

    fn call(
        root: &Path,
        tool: &str,
        args: Value,
        actor: &str,
        role: &str,
    ) -> anyhow::Result<Value> {
        let gateway = json!({
            "auth_source":"ctox_dev_managed_mcp_token", "channel":"ctox_dev_managed_mcp",
            "surface":"workjet", "actor":actor, "role":role,
            "workspace":"tenant:instance", "instance_id":"source-instance"
        });
        call_tool_inner(root, tool, args, Some(&gateway))
    }

    #[test]
    fn instance_configuration_is_durable_and_stale_writers_cannot_replace_it() -> anyhow::Result<()>
    {
        let root = fixture()?;
        let first =
            json!({"workerProfiles":[], "managedSystemPrompt":"Shared instance instructions"});
        assert_eq!(
            call(root.path(), READ_TOOL, json!({}), "owner", "chef")?["revision"],
            0
        );
        let saved = call(
            root.path(),
            WRITE_TOOL,
            json!({"expected_revision":0, "configuration":first}),
            "owner",
            "chef",
        )?;
        assert_eq!(saved, json!({"ok":true,"revision":1}));
        // A second client that read revision zero loses the compare-and-swap.
        let stale = call(
            root.path(),
            WRITE_TOOL,
            json!({"expected_revision":0, "configuration":{"managedSystemPrompt":"Stale client"}}),
            "owner",
            "chef",
        )?;
        assert_eq!(stale, json!({"ok":false,"conflict":true,"revision":1}));
        // Every call opens a new connection; reading here proves committed persistence.
        let read = call(root.path(), READ_TOOL, json!({}), "owner", "chef")?;
        assert_eq!(read["configuration"], first);
        assert!(read["updated_at_ms"].as_i64().is_some());
        assert_eq!(
            call(
                root.path(),
                WRITE_TOOL,
                json!({"expected_revision":1,"configuration":{"managedSystemPrompt":"New instructions"}}),
                "owner",
                "chef"
            )?["revision"],
            2
        );
        assert_eq!(
            call(root.path(), READ_TOOL, json!({}), "owner", "chef")?["configuration"]
                ["managedSystemPrompt"],
            "New instructions"
        );
        Ok(())
    }

    #[test]
    fn instance_configuration_requires_runtime_authority_and_channel_scope() -> anyhow::Result<()> {
        let root = fixture()?;
        let args = json!({"expected_revision":0,"configuration":{"workerProfiles":[]}});
        assert!(call(root.path(), WRITE_TOOL, args.clone(), "reader", "user").is_err());
        assert!(call(root.path(), READ_TOOL, json!({}), "reader", "user").is_err());
        // Arguments cannot manufacture authenticated authority.
        assert!(call_tool_inner(
            root.path(),
            WRITE_TOOL,
            json!({"expected_revision":0,"configuration":{},"trusted_role":"chef"}),
            None
        )
        .is_err());
        let mut policy = default_mcp_policy();
        policy.allowed_collections = vec!["workjet_projects".to_string()];
        save_mcp_policy(root.path(), &policy)?;
        assert!(call(root.path(), READ_TOOL, json!({}), "owner", "chef").is_err());
        assert!(call(root.path(), WRITE_TOOL, args, "owner", "chef").is_err());
        Ok(())
    }

    #[test]
    fn configuration_requests_reject_machine_state_unknown_fields_and_unsafe_revisions(
    ) -> anyhow::Result<()> {
        let root = fixture()?;
        for args in [
            json!({"expected_revision":0,"configuration":{"computers":[]}}),
            json!({"expected_revision":0,"configuration":{},"computer_id":"other"}),
            json!({"expected_revision":MAX_REVISION + 1,"configuration":{}}),
        ] {
            assert!(call(root.path(), WRITE_TOOL, args, "owner", "chef").is_err());
        }
        assert!(call(
            root.path(),
            READ_TOOL,
            json!({"computer_id":"other"}),
            "owner",
            "chef"
        )
        .is_err());
        assert_eq!(
            call(root.path(), READ_TOOL, json!({}), "owner", "chef")?["revision"],
            0
        );
        assert!(revision_of(&json!({"revision":-1})).is_err());
        assert!(revision_of(&json!({"revision":MAX_REVISION + 1})).is_err());
        Ok(())
    }

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
