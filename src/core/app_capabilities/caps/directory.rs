//! Read-only OS lookups (CATALOG Nr. 12 directory, Nr. 19 policy, platform
//! command read): users as an account directory, the server-side policy
//! decision for the Core-bound actor, and the caller's own command intent.
//! Nothing here writes; nothing returns credentials or profile data.
use super::*;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct User {
    user_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsersList {
    active: Option<bool>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    order: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleDecision {
    permission: String,
    module: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandGet {
    command_id: String,
}

/// Directory entry: identity and state only (no profile, mail or secrets).
fn users(root: &Path) -> Result<Vec<Value>, CtoxError> {
    let pulled =
        crate::business_os::store::pull_business_users_for_rxdb(root).map_err(native_error)?;
    Ok(pulled["documents"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|user| {
            json!({"user_id":user["user_id"],"display_name":user["display_name"],
                   "role":user["role"],"active":user["active"].as_bool().unwrap_or(false),
                   "created_at_ms":user["created_at_ms"]})
        })
        .collect())
}

pub(super) fn execute(
    root: &Path,
    ctx: &CapCallContext,
    capability: &str,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match (capability, function) {
        ("directory", "user") => {
            let request: User = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.user_id, "$.user_id")?;
            let found = users(root)?
                .into_iter()
                .find(|user| user["user_id"] == request.user_id.as_str());
            Ok(match found {
                Some(user) => json!({"found":true,"user_id":user["user_id"],
                    "display_name":user["display_name"],"active":user["active"]}),
                None => json!({"found":false,"user_id":request.user_id,"active":false}),
            })
        }
        ("identity.users", "list") => {
            let request: UsersList = decode(args)?;
            if request.roles.len() > 16 || request.order.len() > 4 {
                return Err(invalid("$.args"));
            }
            let mut list = users(root)?
                .into_iter()
                .filter(|user| request.active.map_or(true, |a| user["active"] == a))
                .filter(|user| {
                    request.roles.is_empty()
                        || user["role"]
                            .as_str()
                            .is_some_and(|role| request.roles.iter().any(|r| r == role))
                })
                .collect::<Vec<_>>();
            // Stable, explicit ordering only on the directory's own fields.
            for key in request.order.iter().rev() {
                let (field, descending) = match key.as_str() {
                    "created_at_ms:asc" => ("created_at_ms", false),
                    "created_at_ms:desc" => ("created_at_ms", true),
                    "user_id:asc" => ("user_id", false),
                    "user_id:desc" => ("user_id", true),
                    _ => return Err(invalid("$.order")),
                };
                list.sort_by(|a, b| {
                    let ordering = a[field].to_string().cmp(&b[field].to_string());
                    if descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                });
            }
            for user in &mut list {
                user.as_object_mut().map(|u| u.remove("created_at_ms"));
            }
            Ok(json!({ "users": list }))
        }
        ("policy", "module_decision") => {
            use crate::business_os::policy::{
                evaluate, BusinessOsActor, BusinessOsPermission, BusinessOsScope,
            };
            let request: ModuleDecision = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.module, "$.module")?;
            let permission = match request.permission.as_str() {
                "data.read" => BusinessOsPermission::DataRead,
                "data.write" => BusinessOsPermission::DataWrite,
                "apps.modify" => BusinessOsPermission::AppsModify,
                _ => return Err(invalid("$.permission")),
            };
            // The actor is the Core-bound command actor, never a payload field.
            // Module assignment is not resolved here: assigned-only grants are denied.
            let actor = BusinessOsActor::new(Some(ctx.actor.id.clone()), &ctx.actor.role);
            let decision = evaluate(
                &actor,
                permission,
                &BusinessOsScope::module(request.module, false),
            );
            Ok(
                json!({"allowed":decision.allowed,"display_reason":decision.display_reason,
                      "reason_code":decision.reason_code}),
            )
        }
        ("command", "get") => {
            let request: CommandGet = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.command_id, "$.command_id")?;
            let conn = crate::business_os::store::open_store(root).map_err(native_error)?;
            // Own app's commands only (Core-bound app id, never the payload).
            let row: Option<(String, String, Option<String>, String, String)> = conn
                .query_row(
                    "SELECT command_type, module, record_id, status, payload_json
                       FROM business_commands WHERE command_id=?1 AND module=?2",
                    params![request.command_id, ctx.app_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
                .map_err(native_error)?;
            Ok(match row {
                Some((command_type, module, record_id, status, payload)) => {
                    json!({"found":true,"command_id":request.command_id,"command_type":command_type,
                       "module":module,"record_id":record_id,"status":status,
                       "payload":serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null)})
                }
                None => json!({"found":false,"command_id":request.command_id}),
            })
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown directory function",
        )),
    }
}
