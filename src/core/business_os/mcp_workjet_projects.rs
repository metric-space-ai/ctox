// Origin: CTOX
// License: AGPL-3.0-only
//! Typed project registration through the durable native command plane.
use super::*;
use serde_json::json;

pub(super) const TOOL: &str = "business_os.upsert_project";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    project_id: String,
    name: String,
    idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    info: Option<ProjectInfo>,
    #[serde(default, rename = "_context", skip_serializing)]
    _context: Option<Value>,
}

pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(TOOL,
        "Create or update an owned Workjet project using its stable project_id. Native admission owns identity, ownership and projection. The same actor, project and idempotency_key replay one command; changed intent with that key is rejected. Omitted or null optional fields preserve existing values; an explicit info object replaces the project-info object. info.goal stores the project goals; it does not claim a Supervisor has executed them. Existing website, schedule, archive state and working copies are preserved. No credentials, terminal input or server updates.",
        json!({"type":"object","additionalProperties":false,
            "required":["project_id","name","idempotency_key"],
            "properties":{
                "project_id":{"type":"string","minLength":1,"maxLength":128},
                "name":{"type":"string","minLength":1,"maxLength":256},
                "idempotency_key":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$"},
                "repo_url":{"type":["string","null"],"maxLength":2048},
                "description":{"type":["string","null"],"maxLength":4096},
                "info":{"type":["object","null"],"additionalProperties":false,"properties":{
                    "summary":{"type":["string","null"],"maxLength":4096},
                    "description":{"type":["string","null"],"maxLength":4096},
                    "goal":{"type":["string","null"],"maxLength":4096},
                    "phase":{"type":["string","null"],"maxLength":128},
                    "status":{"type":["string","null"],"maxLength":128}
                }}
            }}))
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        serde_json::to_vec(arguments)?.len() <= 32 * 1024,
        "project request exceeds limit"
    );
    let request: Request = serde_json::from_value(arguments.clone())?;
    for (value, max, field) in [
        (request.project_id.as_str(), 128, "project_id"),
        (request.name.as_str(), 256, "name"),
    ] {
        anyhow::ensure!(
            !value.trim().is_empty() && value.trim() == value && value.len() <= max,
            "{field} is empty, padded or exceeds limit"
        );
    }
    let key = request.idempotency_key.as_bytes();
    anyhow::ensure!(
        !key.is_empty()
            && key.len() <= 256
            && key[0].is_ascii_alphanumeric()
            && key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b':' | b'-')),
        "invalid project idempotency key"
    );
    let actor = resolved_mcp_actor_context(root, context)?;
    anyhow::ensure!(actor["active"] == true, "project actor is inactive");
    let actor_id = required_arg(&actor, "id")?;
    let owner = super::super::workjet_identity::owner(root, &actor_id)?;
    enforce_module_policy(root, "ctox")?;
    enforce_collection_policy(root, "workjet_projects")?;
    enforce_managed_collection_read_scope(context, "workjet_projects")?;
    let mut authorized = context.clone();
    authorized.actor = actor_id.clone();
    authorized.trusted_role = actor["role"].as_str().map(normalize_role);
    enforce_business_os_mcp_policy(root, &authorized, TOOL, arguments)?;
    let conn = store::open_store(root)?;
    if let Some(existing) =
        store::outbound_load_record(&conn, "workjet_projects", &request.project_id)?
    {
        anyhow::ensure!(
            existing["owner_user_id"].as_str() == Some(owner.as_str()),
            "project is not owned by this actor"
        );
    }
    let mut payload = serde_json::to_value(&request)?;
    payload
        .as_object_mut()
        .context("project payload")?
        .remove("idempotency_key");
    let identity = serde_json::to_vec(&(&actor_id, &request.project_id, &request.idempotency_key))?;
    let command_id = format!(
        "workjet_project_upsert_{}",
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, &identity).as_ref())
    );
    let fingerprint = URL_SAFE_NO_PAD
        .encode(digest::digest(&digest::SHA256, &serde_json::to_vec(&payload)?).as_ref());
    if crate::mission::channels::inspect_business_command(root, &command_id)?.is_some() {
        let previous = store::load_business_command(&conn, &command_id)?;
        anyhow::ensure!(
            previous.client_context["workjet_project_upsert_fingerprint"].as_str()
                == Some(fingerprint.as_str()),
            "project idempotency key conflicts with existing intent"
        );
    }
    drop(conn);
    let accepted = store::accept_rxdb_business_command(
        root,
        json!({
            "id":command_id,"command_id":command_id,"module":"ctox",
            "command_type":"ctox.workjet.project.upsert","record_id":request.project_id,
            "payload":payload,
            "client_context":{"actor":actor,"channel":context.channel,"surface":context.surface,
                "workspace":context.workspace,"mcp_actor":context.actor,"request_id":command_id,
                "workjet_project_upsert_fingerprint":fingerprint}
        }),
    )?;
    anyhow::ensure!(accepted["ok"] != false, "native project admission rejected");
    let canonical = crate::mission::channels::business_command_projection(root, &command_id)?;
    anyhow::ensure!(
        canonical["status"] == "completed"
            && canonical["result"]["ok"] == true
            && canonical["result"]["project"]["id"] == request.project_id
            && canonical["result"]["project"]["owner_user_id"] == owner,
        "native project command has no matching completed receipt"
    );
    Ok(
        json!({"schema":"ctox.workjet_project_upsert.v1","command_id":command_id,
        "status":canonical["status"],"project":canonical["result"]["project"],
        "group_chat_id":canonical["result"]["group_chat_id"]}),
    )
}

#[cfg(test)]
#[path = "mcp_workjet_projects_tests.rs"]
mod tests;
