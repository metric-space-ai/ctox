//! `config.get` — read-only typed runtime configuration (CATALOG Nr. 14).
//! Replaces `runtime_env::env_or_config` in handlers. Only whitelisted,
//! non-secret keys; a credential is never readable here (secrets keep values).
use super::*;
use serde::Deserialize;

/// Core whitelist. The per-app grant (manifest) is checked by the host on
/// `args.key`; this list bounds what any grant can ever expose.
pub const ALLOWED_KEYS: &[&str] = &[
    "CTO_EMAIL_ADDRESS",
    "CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS",
    "CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE",
    "CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE",
];

/// Typed platform configuration (runtime config store, never a process env):
/// readable only by platform callers.
pub const PUBLIC_ORIGIN_KEY: &str = "business_os.public_origin";
pub const PLATFORM_KEYS: &[&str] = &[PUBLIC_ORIGIN_KEY];

/// The Core HTTPS origin (`https://host[:port]`, no path) from the runtime
/// config store; anything else is treated as unconfigured.
pub fn public_origin(root: &Path) -> Result<Option<String>, CtoxError> {
    let work = root.to_owned();
    let value = crate::persistence::run_core_blocking_at(root, move || {
        crate::inference::runtime_env::try_get_runtime_env_value(&work, PUBLIC_ORIGIN_KEY)
    })
    .map_err(native_error)?;
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().trim_end_matches('/').to_owned();
    let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return Ok(None);
    };
    Ok((super::browser::web_url(&value) && !rest.contains(['/', '?', '#'])).then_some(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    key: String,
}

pub(super) fn execute(
    root: &Path,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    if function != "get" {
        return Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown config function",
        ));
    }
    let request: Get = decode(args)?;
    text(&request.key, 128, false, "$.key")?;
    if PLATFORM_KEYS.contains(&request.key.as_str()) {
        if !is_platform(ctx) {
            return Err(forbidden("platform configuration"));
        }
        return Ok(json!({ "key": request.key, "value": public_origin(root)? }));
    }
    if !ALLOWED_KEYS.contains(&request.key.as_str()) {
        return Err(forbidden("config key is not readable by apps"));
    }
    if crate::secrets::is_secret_key(&request.key) {
        return Err(forbidden("credentials are not configuration"));
    }
    let work = root.to_owned();
    let key = request.key.clone();
    let value = crate::persistence::run_core_blocking_at(root, move || {
        crate::inference::runtime_env::try_get_runtime_env_value(&work, &key)
    })
    .map_err(native_error)?;
    Ok(json!({ "key": request.key, "value": value }))
}
