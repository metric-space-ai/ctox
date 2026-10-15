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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    key: String,
}

pub(super) fn execute(root: &Path, function: &str, args: &Value) -> Result<Value, CtoxError> {
    if function != "get" {
        return Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown config function",
        ));
    }
    let request: Get = decode(args)?;
    text(&request.key, 128, false, "$.key")?;
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
