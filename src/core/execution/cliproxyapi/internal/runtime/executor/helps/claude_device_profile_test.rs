// ref: internal/runtime/executor/helps/claude_device_profile_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;

use super::claude_device_profile::*;
use super::user_id_cache::{ClaudeIdentityKvStore, ClaudeIdentityStoreError};
use crate::sdk::api::handlers::header_filter::HeaderMap;

#[derive(Default)]
struct FakeStore {
    inner: Mutex<FakeStoreInner>,
}

#[derive(Default)]
struct FakeStoreInner {
    values: HashMap<String, Vec<u8>>,
    get_error: bool,
    set_error: bool,
    set_nx_error: bool,
    expire_error: bool,
    set_nx_result: Option<bool>,
    get_count: usize,
    set_count: usize,
    set_nx_count: usize,
    expire_count: usize,
    last_set_ttl: Option<Duration>,
    last_set_nx_ttl: Option<Duration>,
    last_expire_ttl: Option<Duration>,
}

impl FakeStore {
    fn with_lock_result(result: bool) -> Self {
        let store = Self::default();
        store.inner.lock().unwrap().set_nx_result = Some(result);
        store
    }
}

impl ClaudeIdentityKvStore for FakeStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.get_count += 1;
        if inner.get_error {
            return Err(ClaudeIdentityStoreError::Backend("get failed".to_owned()));
        }
        Ok(inner.values.get(key).cloned())
    }

    fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.set_count += 1;
        inner.last_set_ttl = Some(ttl);
        if inner.set_error {
            return Err(ClaudeIdentityStoreError::Backend("set failed".to_owned()));
        }
        inner.values.insert(key.to_owned(), value.to_vec());
        Ok(true)
    }

    fn set_nx(
        &self,
        key: &str,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.set_nx_count += 1;
        inner.last_set_nx_ttl = Some(ttl);
        if inner.set_nx_error {
            return Err(ClaudeIdentityStoreError::Backend("lock failed".to_owned()));
        }
        if inner.values.contains_key(key) {
            return Ok(false);
        }
        let result = inner.set_nx_result.unwrap_or(true);
        if result {
            inner.values.insert(key.to_owned(), value.to_vec());
        }
        Ok(result)
    }

    fn expire(&self, _key: &str, ttl: Duration) -> Result<bool, ClaudeIdentityStoreError> {
        let mut inner = self.inner.lock().unwrap();
        inner.expire_count += 1;
        inner.last_expire_ttl = Some(ttl);
        if inner.expire_error {
            return Err(ClaudeIdentityStoreError::Backend(
                "expire failed".to_owned(),
            ));
        }
        Ok(true)
    }
}

fn device_headers(user_agent: &str) -> HeaderMap {
    [
        ("User-Agent", user_agent),
        (
            "X-Stainless-Package-Version",
            DEFAULT_CLAUDE_FINGERPRINT_PACKAGE_VERSION,
        ),
        (
            "X-Stainless-Runtime-Version",
            DEFAULT_CLAUDE_FINGERPRINT_RUNTIME_VERSION,
        ),
        ("X-Stainless-Os", "Windows"),
        ("X-Stainless-Arch", "x64"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), vec![value.to_owned()]))
    .collect()
}

fn stored_profile(user_agent: &str, package: &str, runtime: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "user_agent": user_agent,
        "package_version": package,
        "runtime_version": runtime,
        "os": "Windows",
        "arch": "x64"
    }))
    .unwrap()
}

#[test]
fn required_store_read_without_candidate() {
    let store = FakeStore::default();
    let key = claude_device_profile_kv_key(Some("auth-1"), "api-key");
    store.inner.lock().unwrap().values.insert(
        key,
        stored_profile("claude-cli/2.2.0 (external, cli)", "0.80.0", "v24.4.0"),
    );
    let profile = ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            None,
            &ClaudeHeaderDefaults::default(),
        )
        .unwrap();
    assert_eq!(profile.user_agent, DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT);
    assert_eq!(
        profile.package_version,
        DEFAULT_CLAUDE_FINGERPRINT_PACKAGE_VERSION
    );
    assert_eq!(
        profile.runtime_version,
        DEFAULT_CLAUDE_FINGERPRINT_RUNTIME_VERSION
    );
    assert_eq!(profile.os, DEFAULT_CLAUDE_FINGERPRINT_OS);
    assert_eq!(profile.arch, DEFAULT_CLAUDE_FINGERPRINT_ARCH);
    let inner = store.inner.lock().unwrap();
    assert_eq!(inner.expire_count, 1);
    assert_eq!(inner.last_expire_ttl, Some(CLAUDE_DEVICE_PROFILE_TTL));
}

#[test]
fn required_store_candidate_locks_rereads_and_writes() {
    let store = FakeStore::with_lock_result(true);
    let profile = ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &ClaudeHeaderDefaults::default(),
        )
        .unwrap();
    assert_eq!(profile.user_agent, DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT);
    let inner = store.inner.lock().unwrap();
    assert_eq!(inner.set_nx_count, 1);
    assert_eq!(inner.last_set_nx_ttl, Some(CLAUDE_DEVICE_PROFILE_LOCK_TTL));
    assert_eq!(inner.get_count, 1);
    assert_eq!(inner.set_count, 1);
    assert_eq!(inner.last_set_ttl, Some(CLAUDE_DEVICE_PROFILE_TTL));
}

#[test]
fn required_store_normalizes_unmeasured_cached_profile() {
    let store = FakeStore::with_lock_result(true);
    let key = claude_device_profile_kv_key(Some("auth-1"), "api-key");
    store.inner.lock().unwrap().values.insert(
        key,
        stored_profile("claude-cli/2.4.0 (external, cli)", "0.90.0", "v24.5.0"),
    );
    let profile = ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers("claude-cli/2.3.0 (external, cli)")),
            &ClaudeHeaderDefaults::default(),
        )
        .unwrap();
    assert_eq!(profile.user_agent, DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT);
    assert_eq!(profile.os, DEFAULT_CLAUDE_FINGERPRINT_OS);
    assert_eq!(profile.arch, DEFAULT_CLAUDE_FINGERPRINT_ARCH);
    let inner = store.inner.lock().unwrap();
    assert_eq!(inner.set_count, 0);
    assert_eq!(inner.expire_count, 1);
}

#[test]
fn required_store_read_failure() {
    let store = FakeStore::default();
    store.inner.lock().unwrap().get_error = true;
    assert!(ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            None,
            &ClaudeHeaderDefaults::default(),
        )
        .is_err());
}

#[test]
fn required_store_lock_failure() {
    let store = FakeStore::default();
    store.inner.lock().unwrap().set_nx_error = true;
    assert!(ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &ClaudeHeaderDefaults::default(),
        )
        .is_err());
}

#[test]
fn required_store_lock_miss_without_profile() {
    let store = FakeStore::with_lock_result(false);
    assert!(ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &ClaudeHeaderDefaults::default(),
        )
        .is_err());
}

#[test]
fn required_store_reread_failure() {
    let store = FakeStore::with_lock_result(true);
    store.inner.lock().unwrap().get_error = true;
    assert!(ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &ClaudeHeaderDefaults::default(),
        )
        .is_err());
}

#[test]
fn required_store_write_failure() {
    let store = FakeStore::with_lock_result(true);
    store.inner.lock().unwrap().set_error = true;
    assert!(ClaudeDeviceProfileCache::new()
        .resolve_required(
            Some(&store),
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &ClaudeHeaderDefaults::default(),
        )
        .is_err());
}

#[test]
fn local_cache_survives_across_requests() {
    let cache = ClaudeDeviceProfileCache::new();
    let defaults = ClaudeHeaderDefaults::default();
    let first = cache
        .resolve_required(
            None,
            Some("auth-1"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &defaults,
        )
        .unwrap();
    let second = cache
        .resolve_required(None, Some("auth-1"), "api-key", None, &defaults)
        .unwrap();
    assert_eq!(second.user_agent, first.user_agent);
}

#[test]
fn application_helpers_preserve_pinned_and_legacy_semantics() {
    let defaults = ClaudeHeaderDefaults::default();
    let profile = default_claude_device_profile(&defaults);
    let mut upstream = device_headers("old");
    apply_claude_device_profile_headers(&mut upstream, &profile);
    assert_eq!(
        upstream.get("User-Agent").unwrap(),
        &vec![DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT.to_owned()]
    );

    let client = device_headers("claude-cli/2.9.0 (external, cli)");
    let mut legacy = HeaderMap::new();
    apply_claude_legacy_device_headers(&mut legacy, Some(&client), &defaults, false);
    assert_eq!(
        legacy.get("User-Agent").unwrap(),
        &vec![DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT.to_owned()]
    );
    assert_eq!(default_claude_version(&defaults), "2.1.280");

    let prefix_match = ClaudeHeaderDefaults {
        user_agent: "claude-cli/2.8.4suffix".to_owned(),
        ..ClaudeHeaderDefaults::default()
    };
    assert_eq!(default_claude_version(&prefix_match), "2.8.4");
}

#[test]
fn local_resolution_rejects_invalid_software_signals() {
    let mut headers = device_headers("claude-cli/999.0.0 (external, cli)");
    headers.insert(
        "X-Stainless-Package-Version".to_owned(),
        vec!["999.0.0".to_owned()],
    );
    headers.insert(
        "X-Stainless-Runtime-Version".to_owned(),
        vec!["v999.0.0".to_owned()],
    );
    let defaults = ClaudeHeaderDefaults::default();
    let profile = ClaudeDeviceProfileCache::new()
        .resolve_required(
            None,
            Some("auth-invalid"),
            "api-key",
            Some(&headers),
            &defaults,
        )
        .unwrap();
    assert_eq!(profile, default_claude_device_profile(&defaults));
}

#[test]
fn local_resolution_keeps_exact_measured_software_baseline() {
    let headers = device_headers("claude-cli/2.1.281 (external, cli)");
    let defaults = ClaudeHeaderDefaults::default();
    let profile = ClaudeDeviceProfileCache::new()
        .resolve_required(
            None,
            Some("auth-newer-patch-signals"),
            "api-key",
            Some(&headers),
            &defaults,
        )
        .unwrap();
    assert_eq!(profile, default_claude_device_profile(&defaults));
}

#[test]
fn legacy_headers_accept_only_confirmed_measured_baseline() {
    let defaults = ClaudeHeaderDefaults {
        user_agent: "claude-cli/2.2.0 (external, cli)".to_owned(),
        package_version: "0.95.0".to_owned(),
        runtime_version: "v26.4.0".to_owned(),
        os: "MacOS".to_owned(),
        arch: "arm64".to_owned(),
        stabilize_device_profile: None,
    };
    let mut client = device_headers("claude-cli/2.2.0 (external, cli)");
    client.insert(
        "X-Stainless-Package-Version".to_owned(),
        vec!["0.95.0".to_owned()],
    );
    client.insert(
        "X-Stainless-Runtime-Version".to_owned(),
        vec!["v26.4.0".to_owned()],
    );
    let mut legacy = HeaderMap::new();
    apply_claude_legacy_device_headers(&mut legacy, Some(&client), &defaults, true);
    assert_eq!(
        legacy.get("User-Agent").unwrap(),
        &vec!["claude-cli/2.2.0 (external, cli)".to_owned()]
    );
    assert_eq!(
        legacy.get("X-Stainless-Package-Version").unwrap(),
        &vec!["0.95.0".to_owned()]
    );
    assert_eq!(
        legacy.get("X-Stainless-Runtime-Version").unwrap(),
        &vec!["v26.4.0".to_owned()]
    );
}

#[test]
fn local_cache_separates_vscode_agent_sdk_from_cli() {
    let cache = ClaudeDeviceProfileCache::new();
    let defaults = ClaudeHeaderDefaults::default();
    let cli = cache
        .resolve_required(
            None,
            Some("auth-subclient-isolation"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &defaults,
        )
        .unwrap();
    let vscode_user_agent = "claude-cli/2.1.280 (external, claude-vscode, agent-sdk/0.3.220)";
    let vscode = cache
        .resolve_required(
            None,
            Some("auth-subclient-isolation"),
            "api-key",
            Some(&device_headers(vscode_user_agent)),
            &defaults,
        )
        .unwrap();
    assert_eq!(cli.user_agent, DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT);
    assert_eq!(vscode.user_agent, vscode_user_agent);

    let cli_again = cache
        .resolve_required(
            None,
            Some("auth-subclient-isolation"),
            "api-key",
            Some(&device_headers(DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT)),
            &defaults,
        )
        .unwrap();
    assert_eq!(cli_again.user_agent, cli.user_agent);
}
