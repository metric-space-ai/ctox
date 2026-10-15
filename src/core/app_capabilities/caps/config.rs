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
pub fn public_origin(root: &Path) -> Option<String> {
    let value = crate::inference::runtime_env::get_runtime_env_value(root, PUBLIC_ORIGIN_KEY)?;
    let value = value.trim().trim_end_matches('/').to_owned();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))?;
    (super::browser::web_url(&value) && !rest.contains(['/', '?', '#'])).then_some(value)
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
        return Ok(json!({ "key": request.key, "value": public_origin(root) }));
    }
    if !ALLOWED_KEYS.contains(&request.key.as_str()) {
        return Err(forbidden("config key is not readable by apps"));
    }
    if crate::secrets::is_secret_key(&request.key) {
        return Err(forbidden("credentials are not configuration"));
    }
    let value = crate::inference::runtime_env::env_or_config(root, &request.key)
        .filter(|value| !value.trim().is_empty());
    Ok(json!({ "key": request.key, "value": value }))
}
