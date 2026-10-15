//! `secret.*` — keychain (CATALOG Nr. 11) over the encrypted CTOX secret
//! store. Scope = calling app (`app:<app_id>`), optionally narrowed to the
//! acting user (`app:<app_id>/user:<actor_id>`); both derive from the
//! Core-bound ctx, never from the payload. The instance credential scope
//! (`credentials`) is the platform keychain: readable as metadata, writable
//! only by secrets.manage roles. Values never leave through any function here:
//! put takes the value from the command's secret intake, list/exists return
//! names and metadata only.
use super::*;
use serde::Deserialize;

pub const INSTANCE_SCOPE: &str = "credentials";
/// policy.rs: SecretsManage = Chef | Admin.
const SECRETS_MANAGE_ROLES: &[&str] = &["chef", "admin"];

/// Core-owned access to the value detached from the triggering command at
/// intake (store::detach_secret_intake_value). Bound to ctx.command_id: an app
/// can only consume the intake of the command it is executing. Implementations
/// remove the value on take. No constructor from wire claims.
pub trait SecretIntake {
    fn take(&self, ctx: &CapCallContext, field: &str) -> Option<String>;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    name: Option<String>,
    scope: Option<String>,
    user: Option<bool>,
    command_id: Option<String>,
    value: Option<Value>,
    description: Option<String>,
}

pub fn app_scope(ctx: &CapCallContext) -> String {
    format!("app:{}", ctx.app_id)
}
pub fn user_scope(ctx: &CapCallContext) -> String {
    format!("app:{}/user:{}", ctx.app_id, ctx.actor.id)
}
/// The only scopes a caller can name: its own app/user scopes, or the
/// instance keychain. Another app's scope is never reachable.
fn resolve_scope(ctx: &CapCallContext, args: &Args) -> Result<String, CtoxError> {
    let own = if args.user == Some(true) {
        user_scope(ctx)
    } else {
        app_scope(ctx)
    };
    match args.scope.as_deref() {
        None => Ok(own),
        Some(scope) if scope == app_scope(ctx) || scope == user_scope(ctx) => Ok(scope.into()),
        Some(INSTANCE_SCOPE) => Ok(INSTANCE_SCOPE.into()),
        Some(_) => Err(forbidden("secret scope belongs to another owner")),
    }
}
fn require_manage(ctx: &CapCallContext, scope: &str) -> Result<(), CtoxError> {
    if scope == INSTANCE_SCOPE && !SECRETS_MANAGE_ROLES.contains(&ctx.actor.role.as_str()) {
        return Err(forbidden("instance credentials require secrets.manage"));
    }
    Ok(())
}
fn name(args: &Args) -> Result<String, CtoxError> {
    let name = args.name.as_deref().ok_or_else(|| invalid("$.name"))?;
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(invalid("$.name"));
    }
    Ok(name.to_owned())
}

pub(super) fn execute(
    root: &Path,
    intake: &dyn SecretIntake,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    let args: Args = decode(args)?;
    if function != "put" && (args.command_id.is_some() || args.value.is_some()) {
        return Err(invalid("$.args"));
    }
    let scope = resolve_scope(ctx, &args)?;
    match function {
        "scope" => {
            if args.name.is_some() || args.scope.is_some() {
                return Err(invalid("$.args"));
            }
            Ok(json!({ "scope": scope }))
        }
        "exists" => {
            let name = name(&args)?;
            let exists =
                crate::secrets::secret_exists(root, &scope, &name).map_err(native_error)?;
            Ok(json!({"scope":scope,"name":name,"exists":exists}))
        }
        "list" => {
            if args.name.is_some() {
                return Err(invalid("$.name"));
            }
            require_manage(ctx, &scope)?;
            let records = crate::secrets::list_secret_records(root, Some(scope.as_str()))
                .map_err(native_error)?;
            // Names and timestamps only: metadata is caller-supplied and may
            // carry anything, the value is never decrypted for a list.
            let secrets = records
                .into_iter()
                .filter(|record| record.scope == scope)
                .map(|record| {
                    json!({"name":record.secret_name,"description":record.description,
                           "created_at":record.created_at,"updated_at":record.updated_at})
                })
                .collect::<Vec<_>>();
            Ok(json!({"scope":scope,"secrets":secrets}))
        }
        "delete" => {
            let name = name(&args)?;
            require_manage(ctx, &scope)?;
            let existed =
                crate::secrets::secret_exists(root, &scope, &name).map_err(native_error)?;
            if existed {
                crate::secrets::delete_secret_record(root, &scope, &name).map_err(native_error)?;
            }
            Ok(json!({"scope":scope,"name":name,"deleted":existed}))
        }
        "put" => {
            let name = name(&args)?;
            // The intake belongs to the command being executed, never another.
            if args.command_id.as_deref() != Some(ctx.command_id.as_str()) {
                return Err(forbidden("secret intake belongs to another command"));
            }
            // A plaintext value never crosses the capability boundary.
            if args.value != Some(json!({"secret_intake": true})) {
                return Err(invalid("$.value"));
            }
            if let Some(description) = &args.description {
                text(description, 512, true, "$.description")?;
            }
            require_manage(ctx, &scope)?;
            let value = intake
                .take(ctx, "value")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| invalid("$.value"))?;
            if scope == INSTANCE_SCOPE {
                crate::secrets::set_credential(root, &name, &value).map_err(native_error)?;
            } else {
                crate::secrets::write_secret_record(
                    root,
                    &scope,
                    &name,
                    &value,
                    args.description.clone(),
                    json!({"source":"app_capability","app_id":ctx.app_id}),
                )
                .map_err(native_error)?;
            }
            Ok(json!({"scope":scope,"name":name,"stored":true}))
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown secret function",
        )),
    }
}
