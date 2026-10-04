// ref: sdk/cliproxy/auth/metadata_merge.go:10-366 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned maps and injected clock/store authority
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{Auth, AuthStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn is_auth_token_payload_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "access_token"
            | "refresh_token"
            | "id_token"
            | "session_id"
            | "expired"
            | "last_refresh"
            | "expires_in"
            | "timestamp"
            | "token_type"
            | "user_code"
            | "verification_uri"
            | "verification_uri_complete"
    )
}

/// The native AuthStore persists owned snapshots; shared storage is not mutated.
pub fn merge_existing_auth_metadata(target: &mut Auth, existing: &BTreeMap<String, Value>) {
    if !target.metadata.contains_key("disabled") {
        if let Some(disabled) = existing.get("disabled").and_then(Value::as_bool) {
            target.disabled = disabled;
        }
    }
    for (key, value) in existing {
        if is_auth_token_payload_key(key)
            || (target.provider.trim().eq_ignore_ascii_case("meta")
                && matches!(
                    key.trim().to_ascii_lowercase().as_str(),
                    "api_key" | "dca_token" | "dca_expired" | "dca_expires_at"
                ))
        {
            continue;
        }
        target
            .metadata
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// Preparation preserves current refresh timestamps, failures and cooldowns.
pub fn merge_prepared_auth(base: &Auth, current: &Auth, updated: &Auth) -> Auth {
    merge_auth_content(base, current, updated)
}

pub fn merge_refreshed_auth(
    base: &Auth,
    current: &Auth,
    updated: &Auth,
    now: DateTime<Utc>,
) -> Auth {
    let mut merged = merge_auth_content(base, current, updated);
    if base.registration_epoch != current.registration_epoch {
        return merged;
    }
    let zero = super::types::go_zero_time();
    if updated.last_refreshed_at != zero {
        merged.last_refreshed_at = updated.last_refreshed_at;
    }
    if updated.next_refresh_after != zero || base.next_refresh_after != zero {
        merged.next_refresh_after = updated.next_refresh_after;
    }
    let error_message = |auth: &Auth| {
        auth.last_error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_default()
    };
    let current_error = error_message(current);
    let concurrent_error = !current_error.is_empty() && current_error != error_message(base);
    let disabled = |auth: &Auth| auth.disabled || auth.status == AuthStatus::Disabled;
    let executor_changed_disabled = disabled(updated) != disabled(base);
    let user_changed_disabled = disabled(current) != disabled(base);
    let final_disabled = if executor_changed_disabled && !user_changed_disabled {
        disabled(updated)
    } else {
        disabled(current)
    };
    merged.disabled = final_disabled;
    merged
        .metadata
        .insert("disabled".into(), Value::Bool(final_disabled));
    if final_disabled {
        merged.status = AuthStatus::Disabled;
    } else {
        if merged.status == AuthStatus::Disabled {
            merged.status = AuthStatus::Active;
        }
        let credential_quota = current.quota.exceeded
            && current.quota.reason == "credential_quota"
            && current.quota.next_recover_at > now;
        let cooldown = current.unavailable && current.next_retry_after > now;
        if concurrent_error || credential_quota || cooldown {
            merged.status = current.status.clone();
            merged.unavailable = current.unavailable;
            merged.status_message = current.status_message.clone();
            if concurrent_error {
                merged.last_error = current.last_error.clone();
            }
        } else if updated.status == AuthStatus::Active
            || matches!(&updated.status, AuthStatus::Other(value) if value.is_empty())
        {
            merged.status = AuthStatus::Active;
            merged.unavailable = false;
            merged.status_message.clear();
            merged.last_error = None;
        }
    }
    merge_map(
        &base.model_states,
        &current.model_states,
        &updated.model_states,
        &mut merged.model_states,
        |_| false,
        |_| false,
    );
    merged
}

fn merge_auth_content(base: &Auth, current: &Auth, updated: &Auth) -> Auth {
    let mut merged = current.clone();
    if base.registration_epoch != current.registration_epoch {
        return merged;
    }
    merge_map(
        &base.metadata,
        &current.metadata,
        &updated.metadata,
        &mut merged.metadata,
        is_auth_token_payload_key,
        |key| key.trim().eq_ignore_ascii_case("proxy_url"),
    );
    if updated.storage.is_some() {
        merged.storage = updated.storage.clone();
    }
    if updated.runtime.is_some() {
        merged.runtime = updated.runtime.clone();
    }
    let meta_proxy = |auth: &Auth| {
        auth.metadata
            .get("proxy_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let base_struct = base.proxy_url.trim();
    let current_struct = current.proxy_url.trim();
    let updated_struct = updated.proxy_url.trim();
    let base_meta = meta_proxy(base);
    let current_meta = meta_proxy(current);
    let updated_meta = meta_proxy(updated);
    let choose =
        |structure: &str, metadata: &str, structure_changed: bool, metadata_changed: bool| {
            if structure_changed && !metadata_changed {
                structure.to_owned()
            } else if metadata_changed && !structure_changed {
                metadata.to_owned()
            } else if !structure.is_empty() {
                structure.to_owned()
            } else {
                metadata.to_owned()
            }
        };
    let user_struct = current_struct != base_struct;
    let user_meta = current_meta != base_meta;
    let executor_struct = updated_struct != base_struct;
    let executor_meta = updated_meta != base_meta;
    let final_proxy = if user_struct || user_meta {
        choose(current_struct, &current_meta, user_struct, user_meta)
    } else if executor_struct || executor_meta {
        choose(
            updated_struct,
            &updated_meta,
            executor_struct,
            executor_meta,
        )
    } else if current_struct.is_empty() && !current_meta.is_empty() {
        current_meta
    } else {
        current_struct.to_owned()
    };
    merged.proxy_url = final_proxy.clone();
    if final_proxy.is_empty() {
        merged.metadata.remove("proxy_url");
    } else {
        merged
            .metadata
            .insert("proxy_url".into(), Value::String(final_proxy));
    }
    merged.prefix = if updated.prefix.trim() != base.prefix.trim()
        && current.prefix.trim() == base.prefix.trim()
    {
        updated.prefix.trim().into()
    } else {
        current.prefix.trim().into()
    };
    merge_map(
        &base.attributes,
        &current.attributes,
        &updated.attributes,
        &mut merged.attributes,
        |_| false,
        |_| false,
    );
    merged
}

fn merge_map<T: Clone + PartialEq>(
    base: &BTreeMap<String, T>,
    current: &BTreeMap<String, T>,
    updated: &BTreeMap<String, T>,
    merged: &mut BTreeMap<String, T>,
    force: impl Fn(&str) -> bool,
    skip: impl Fn(&str) -> bool,
) {
    for (key, value) in updated {
        if !skip(key)
            && base.get(key) != Some(value)
            && (base.get(key) == current.get(key) || force(key))
        {
            merged.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in base {
        if !skip(key) && !updated.contains_key(key) && current.get(key) == Some(value) {
            merged.remove(key);
        }
    }
}

#[cfg(test)]
#[path = "metadata_merge_test.rs"]
mod tests;
