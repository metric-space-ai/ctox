// ref: internal/home/kv_helpers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::client::{Client, HomeError, KvSetOptions};
use super::global::HomeRuntime;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub fn hash_key_part(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn current_kv_client(
    runtime: &HomeRuntime,
) -> Result<(Option<std::sync::Arc<Client>>, bool), HomeError> {
    let Some(client) = runtime.current() else {
        return Ok((None, false));
    };
    if !client.enabled() {
        return Err(HomeError::Disabled);
    }
    if !client.heartbeat_ok() {
        return Err(HomeError::NotConnected);
    }
    Ok((Some(client), true))
}
pub fn kv_get_json_required<T: DeserializeOwned>(
    runtime: &HomeRuntime,
    key: &str,
) -> Result<(bool, Option<T>), HomeError> {
    let (client, home_mode) = current_kv_client(runtime)?;
    let Some(client) = client else {
        return Ok((home_mode, None));
    };
    let Some(raw) = client.kv_get(key)? else {
        return Ok((true, None));
    };
    let value =
        serde_json::from_slice(&raw).map_err(|e| HomeError::InvalidRequest(e.to_string()))?;
    Ok((true, Some(value)))
}
pub fn kv_set_json_required<T: Serialize>(
    runtime: &HomeRuntime,
    key: &str,
    value: &T,
    ttl: Duration,
) -> Result<bool, HomeError> {
    let raw = serde_json::to_vec(value).map_err(|e| HomeError::InvalidRequest(e.to_string()))?;
    kv_set_bytes_required(runtime, key, &raw, ttl)
}
pub fn kv_set_bytes_required(
    runtime: &HomeRuntime,
    key: &str,
    value: &[u8],
    ttl: Duration,
) -> Result<bool, HomeError> {
    let (client, home_mode) = current_kv_client(runtime)?;
    let Some(client) = client else {
        return Ok(home_mode);
    };
    if client.kv_set(key, value, kv_set_options_for_ttl(ttl))? {
        Ok(true)
    } else {
        Err(HomeError::NotConnected)
    }
}
pub fn kv_set_nx_required(
    runtime: &HomeRuntime,
    key: &str,
    value: &[u8],
    ttl: Duration,
) -> Result<(bool, bool), HomeError> {
    let (client, home_mode) = current_kv_client(runtime)?;
    let Some(client) = client else {
        return Ok((home_mode, false));
    };
    Ok((true, client.kv_set_nx(key, value, ttl)?))
}
pub fn kv_del_required(runtime: &HomeRuntime, keys: &[String]) -> Result<(bool, i64), HomeError> {
    let (client, home_mode) = current_kv_client(runtime)?;
    let Some(client) = client else {
        return Ok((home_mode, 0));
    };
    Ok((true, client.kv_del(keys)?))
}
pub fn kv_expire_required(
    runtime: &HomeRuntime,
    key: &str,
    ttl: Duration,
) -> Result<bool, HomeError> {
    let (client, home_mode) = current_kv_client(runtime)?;
    let Some(client) = client else {
        return Ok(home_mode);
    };
    client.kv_expire(key, ttl)?;
    Ok(true)
}
pub fn kv_get_json_best_effort<T: DeserializeOwned>(
    runtime: &HomeRuntime,
    key: &str,
) -> (bool, Option<T>) {
    kv_get_json_required(runtime, key).unwrap_or((runtime.current().is_some(), None))
}
pub fn kv_set_json_best_effort<T: Serialize>(
    runtime: &HomeRuntime,
    key: &str,
    value: &T,
    ttl: Duration,
) -> bool {
    kv_set_json_required(runtime, key, value, ttl).unwrap_or(false)
}
pub fn kv_set_bytes_best_effort(
    runtime: &HomeRuntime,
    key: &str,
    value: &[u8],
    ttl: Duration,
) -> bool {
    kv_set_bytes_required(runtime, key, value, ttl).unwrap_or(false)
}
pub fn kv_set_nx_best_effort(
    runtime: &HomeRuntime,
    key: &str,
    value: &[u8],
    ttl: Duration,
) -> bool {
    kv_set_nx_required(runtime, key, value, ttl)
        .map(|(_, written)| written)
        .unwrap_or(false)
}
pub fn kv_del_best_effort(runtime: &HomeRuntime, keys: &[String]) -> bool {
    kv_del_required(runtime, keys).is_ok()
}
pub fn kv_expire_best_effort(runtime: &HomeRuntime, key: &str, ttl: Duration) -> bool {
    kv_expire_required(runtime, key, ttl).unwrap_or(false)
}
pub fn kv_set_options_for_ttl(ttl: Duration) -> KvSetOptions {
    if ttl.is_zero() {
        KvSetOptions::default()
    } else {
        KvSetOptions {
            ex: ttl,
            ..KvSetOptions::default()
        }
    }
}
pub fn kv_log_prefix(key: &str) -> String {
    let mut parts = key.trim().split(':');
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) if !first.is_empty() => format!("{first}:{second}:*"),
        (Some(first), _) if !first.is_empty() => format!("{first}:*"),
        _ => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}
