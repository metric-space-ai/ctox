// ref: internal/runtime/executor/helps/claude_device_profile.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::claude_code_session::header_value_case_insensitive;
use super::user_id_cache::{ClaudeIdentityKvStore, ClaudeIdentityStoreError};
use crate::internal::home::hash_key_part;
use crate::sdk::api::handlers::header_filter::HeaderMap;

pub const DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT: &str = "claude-cli/2.1.280 (external, cli)";
pub const DEFAULT_CLAUDE_FINGERPRINT_PACKAGE_VERSION: &str = "0.112.1";
pub const DEFAULT_CLAUDE_FINGERPRINT_RUNTIME_VERSION: &str = "v26.3.0";
pub const DEFAULT_CLAUDE_FINGERPRINT_OS: &str = "MacOS";
pub const DEFAULT_CLAUDE_FINGERPRINT_ARCH: &str = "arm64";
pub const CLAUDE_DEVICE_PROFILE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const CLAUDE_DEVICE_PROFILE_LOCK_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeHeaderDefaults {
    pub user_agent: String,
    pub package_version: String,
    pub runtime_version: String,
    pub os: String,
    pub arch: String,
    pub stabilize_device_profile: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ClaudeCliVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeDeviceProfile {
    pub user_agent: String,
    pub package_version: String,
    pub runtime_version: String,
    pub os: String,
    pub arch: String,
    version: Option<ClaudeCliVersion>,
}

#[derive(Clone, Debug)]
struct ClaudeDeviceProfileCacheEntry {
    profile: ClaudeDeviceProfile,
    expire: Instant,
}

#[derive(Default, Debug)]
pub struct ClaudeDeviceProfileCache {
    entries: Mutex<HashMap<String, ClaudeDeviceProfileCacheEntry>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClaudeDeviceProfileKvValue {
    user_agent: String,
    package_version: String,
    runtime_version: String,
    os: String,
    arch: String,
}

pub fn claude_device_profile_stabilization_enabled(defaults: &ClaudeHeaderDefaults) -> bool {
    defaults.stabilize_device_profile.unwrap_or(false)
}

pub fn map_stainless_os() -> String {
    match std::env::consts::OS {
        "macos" => "MacOS".to_owned(),
        "windows" => "Windows".to_owned(),
        "linux" => "Linux".to_owned(),
        "freebsd" => "FreeBSD".to_owned(),
        other => format!("Other::{other}"),
    }
}

pub fn map_stainless_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x64".to_owned(),
        "aarch64" => "arm64".to_owned(),
        "x86" => "x86".to_owned(),
        other => format!("other::{other}"),
    }
}

pub fn default_claude_device_profile(defaults: &ClaudeHeaderDefaults) -> ClaudeDeviceProfile {
    let mut profile = ClaudeDeviceProfile {
        user_agent: defaulted(&defaults.user_agent, DEFAULT_CLAUDE_FINGERPRINT_USER_AGENT),
        package_version: defaulted(
            &defaults.package_version,
            DEFAULT_CLAUDE_FINGERPRINT_PACKAGE_VERSION,
        ),
        runtime_version: defaulted(
            &defaults.runtime_version,
            DEFAULT_CLAUDE_FINGERPRINT_RUNTIME_VERSION,
        ),
        os: defaulted(&defaults.os, DEFAULT_CLAUDE_FINGERPRINT_OS),
        arch: defaulted(&defaults.arch, DEFAULT_CLAUDE_FINGERPRINT_ARCH),
        version: None,
    };
    profile.version = parse_claude_cli_version(&profile.user_agent);
    profile
}

impl ClaudeDeviceProfileCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    pub fn resolve(
        &self,
        store: Option<&dyn ClaudeIdentityKvStore>,
        auth_id: Option<&str>,
        api_key: &str,
        headers: Option<&HeaderMap>,
        defaults: &ClaudeHeaderDefaults,
    ) -> ClaudeDeviceProfile {
        self.resolve_required(store, auth_id, api_key, headers, defaults)
            .unwrap_or_else(|_| default_claude_device_profile(defaults))
    }

    pub fn resolve_required(
        &self,
        store: Option<&dyn ClaudeIdentityKvStore>,
        auth_id: Option<&str>,
        api_key: &str,
        headers: Option<&HeaderMap>,
        defaults: &ClaudeHeaderDefaults,
    ) -> Result<ClaudeDeviceProfile, ClaudeIdentityStoreError> {
        match store {
            Some(store) => {
                resolve_claude_device_profile_home(store, auth_id, api_key, headers, defaults)
            }
            None => Ok(self.resolve_local(auth_id, api_key, headers, defaults)),
        }
    }

    fn resolve_local(
        &self,
        auth_id: Option<&str>,
        api_key: &str,
        headers: Option<&HeaderMap>,
        defaults: &ClaudeHeaderDefaults,
    ) -> ClaudeDeviceProfile {
        let now = Instant::now();
        let baseline = default_claude_device_profile(defaults);
        let candidate = extract_claude_device_profile(headers, defaults)
            .map(|profile| pin_claude_device_profile_platform(profile, &baseline))
            .filter(|profile| meets_claude_device_profile_baseline(profile, &baseline));
        let cache_key =
            claude_device_profile_cache_key_scoped(auth_id, api_key, candidate.as_ref());
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        entries.retain(|_, entry| entry.expire > now);

        if let Some(candidate) = candidate {
            if let Some(entry) = entries.get_mut(&cache_key) {
                entry.profile = normalize_claude_device_profile(entry.profile.clone(), &baseline);
                if !should_upgrade_claude_device_profile(&candidate, &entry.profile) {
                    entry.expire = now + CLAUDE_DEVICE_PROFILE_TTL;
                    return entry.profile.clone();
                }
            }
            entries.insert(
                cache_key,
                ClaudeDeviceProfileCacheEntry {
                    profile: candidate.clone(),
                    expire: now + CLAUDE_DEVICE_PROFILE_TTL,
                },
            );
            return candidate;
        }

        if let Some(entry) = entries.get_mut(&cache_key) {
            entry.profile = normalize_claude_device_profile(entry.profile.clone(), &baseline);
            entry.expire = now + CLAUDE_DEVICE_PROFILE_TTL;
            return entry.profile.clone();
        }
        baseline
    }

    pub fn purge_expired(&self) {
        let now = Instant::now();
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|_, entry| entry.expire > now);
    }
}

fn resolve_claude_device_profile_home(
    store: &dyn ClaudeIdentityKvStore,
    auth_id: Option<&str>,
    api_key: &str,
    headers: Option<&HeaderMap>,
    defaults: &ClaudeHeaderDefaults,
) -> Result<ClaudeDeviceProfile, ClaudeIdentityStoreError> {
    let baseline = default_claude_device_profile(defaults);
    let candidate = extract_claude_device_profile(headers, defaults)
        .map(|profile| pin_claude_device_profile_platform(profile, &baseline))
        .filter(|profile| meets_claude_device_profile_baseline(profile, &baseline));
    let value_key = claude_device_profile_kv_key_scoped(auth_id, api_key, candidate.as_ref());
    let Some(candidate) = candidate else {
        return read_claude_device_profile_from_home(store, &value_key, &baseline);
    };

    let lock_key = claude_device_profile_lock_kv_key_scoped(auth_id, api_key, Some(&candidate));
    let got_lock = store.set_nx(&lock_key, b"1", CLAUDE_DEVICE_PROFILE_LOCK_TTL)?;
    let cached = read_claude_device_profile_value_from_home(store, &value_key, &baseline)?;
    if let Some(cached) = cached {
        if !should_upgrade_claude_device_profile(&candidate, &cached) {
            store.expire(&value_key, CLAUDE_DEVICE_PROFILE_TTL)?;
            return Ok(cached);
        }
        if !got_lock {
            return Ok(cached);
        }
    } else if !got_lock {
        return Err(ClaudeIdentityStoreError::MissingAfterSet);
    }

    write_claude_device_profile_to_home(store, &value_key, &candidate)?;
    Ok(candidate)
}

fn read_claude_device_profile_from_home(
    store: &dyn ClaudeIdentityKvStore,
    key: &str,
    baseline: &ClaudeDeviceProfile,
) -> Result<ClaudeDeviceProfile, ClaudeIdentityStoreError> {
    let Some(profile) = read_claude_device_profile_value_from_home(store, key, baseline)? else {
        return Ok(baseline.clone());
    };
    store.expire(key, CLAUDE_DEVICE_PROFILE_TTL)?;
    Ok(profile)
}

fn read_claude_device_profile_value_from_home(
    store: &dyn ClaudeIdentityKvStore,
    key: &str,
    baseline: &ClaudeDeviceProfile,
) -> Result<Option<ClaudeDeviceProfile>, ClaudeIdentityStoreError> {
    let Some(raw) = store.get(key)? else {
        return Ok(None);
    };
    let value = serde_json::from_slice::<ClaudeDeviceProfileKvValue>(&raw)
        .map_err(|_| ClaudeIdentityStoreError::InvalidJson)?;
    let profile = value.into_profile();
    if profile.user_agent.is_empty() {
        return Ok(None);
    }
    Ok(Some(normalize_claude_device_profile(profile, baseline)))
}

fn write_claude_device_profile_to_home(
    store: &dyn ClaudeIdentityKvStore,
    key: &str,
    profile: &ClaudeDeviceProfile,
) -> Result<(), ClaudeIdentityStoreError> {
    let raw = serde_json::to_vec(&ClaudeDeviceProfileKvValue::from_profile(profile))
        .map_err(|_| ClaudeIdentityStoreError::InvalidJson)?;
    if !store.set(key, &raw, CLAUDE_DEVICE_PROFILE_TTL)? {
        return Err(ClaudeIdentityStoreError::WriteSkipped);
    }
    Ok(())
}

impl ClaudeDeviceProfileKvValue {
    fn from_profile(profile: &ClaudeDeviceProfile) -> Self {
        Self {
            user_agent: profile.user_agent.clone(),
            package_version: profile.package_version.clone(),
            runtime_version: profile.runtime_version.clone(),
            os: profile.os.clone(),
            arch: profile.arch.clone(),
        }
    }

    fn into_profile(self) -> ClaudeDeviceProfile {
        let user_agent = self.user_agent.trim().to_owned();
        let version = parse_claude_cli_version(&user_agent);
        ClaudeDeviceProfile {
            user_agent,
            package_version: self.package_version.trim().to_owned(),
            runtime_version: self.runtime_version.trim().to_owned(),
            os: self.os.trim().to_owned(),
            arch: self.arch.trim().to_owned(),
            version,
        }
    }
}

pub fn apply_claude_device_profile_headers(headers: &mut HeaderMap, profile: &ClaudeDeviceProfile) {
    for name in [
        "User-Agent",
        "X-Stainless-Package-Version",
        "X-Stainless-Runtime-Version",
        "X-Stainless-Os",
        "X-Stainless-Arch",
    ] {
        remove_header(headers, name);
    }
    set_header(headers, "User-Agent", &profile.user_agent);
    set_header(
        headers,
        "X-Stainless-Package-Version",
        &profile.package_version,
    );
    set_header(
        headers,
        "X-Stainless-Runtime-Version",
        &profile.runtime_version,
    );
    set_header(headers, "X-Stainless-Os", &profile.os);
    set_header(headers, "X-Stainless-Arch", &profile.arch);
}

pub fn default_claude_version(defaults: &ClaudeHeaderDefaults) -> String {
    parse_claude_cli_version(&default_claude_device_profile(defaults).user_agent)
        .map(|version| format!("{}.{}.{}", version.major, version.minor, version.patch))
        .unwrap_or_else(|| "2.1.280".to_owned())
}

pub fn apply_claude_default_device_profile_headers(
    headers: &mut HeaderMap,
    defaults: &ClaudeHeaderDefaults,
) {
    apply_claude_device_profile_headers(headers, &default_claude_device_profile(defaults));
}

pub fn apply_claude_legacy_device_headers(
    headers: &mut HeaderMap,
    client_headers: Option<&HeaderMap>,
    defaults: &ClaudeHeaderDefaults,
    confirmed_claude_code: bool,
) {
    let profile = default_claude_device_profile(defaults);
    if confirmed_claude_code {
        let mapped_os = map_stainless_os();
        let mapped_arch = map_stainless_arch();
        for (name, fallback, require_exact) in [
            (
                "X-Stainless-Runtime-Version",
                profile.runtime_version.as_str(),
                true,
            ),
            (
                "X-Stainless-Package-Version",
                profile.package_version.as_str(),
                true,
            ),
            ("X-Stainless-Os", mapped_os.as_str(), false),
            ("X-Stainless-Arch", mapped_arch.as_str(), false),
        ] {
            let current = header_value_case_insensitive(Some(headers), name);
            if !current.is_empty() && (!require_exact || current == fallback) {
                continue;
            }
            let incoming = header_value_case_insensitive(client_headers, name);
            let selected = if !incoming.is_empty() && (!require_exact || incoming == fallback) {
                incoming
            } else {
                fallback.to_owned()
            };
            set_header(headers, name, &selected);
        }
        let client_user_agent = header_value_case_insensitive(client_headers, "User-Agent");
        if plausible_claude_code_user_agent(&client_user_agent, defaults) {
            set_header(headers, "User-Agent", &client_user_agent);
            return;
        }
    }
    apply_claude_device_profile_headers(headers, &profile);
}

fn defaulted(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn parse_claude_cli_version(user_agent: &str) -> Option<ClaudeCliVersion> {
    let version = user_agent.trim().strip_prefix("claude-cli/")?;
    let (major, version) = take_decimal(version)?;
    let version = version.strip_prefix('.')?;
    let (minor, version) = take_decimal(version)?;
    let version = version.strip_prefix('.')?;
    let (patch, _) = take_decimal(version)?;
    Some(ClaudeCliVersion {
        major,
        minor,
        patch,
    })
}

fn take_decimal(value: &str) -> Option<(u64, &str)> {
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    Some((value[..digits].parse().ok()?, &value[digits..]))
}

fn should_upgrade_claude_device_profile(
    candidate: &ClaudeDeviceProfile,
    current: &ClaudeDeviceProfile,
) -> bool {
    match (candidate.version, current.version) {
        (Some(candidate), Some(current)) => candidate > current,
        (Some(_), None) if !candidate.user_agent.is_empty() => true,
        _ => false,
    }
}

/// The baseline is a floor for patch releases. Claude Code auto-updates, so a
/// newer patch in the same major/minor line stays a native client.
fn plausible_claude_cli_version(candidate: ClaudeCliVersion, baseline: ClaudeCliVersion) -> bool {
    candidate.major == baseline.major
        && candidate.minor == baseline.minor
        && candidate.patch >= baseline.patch
}

fn meets_claude_device_profile_baseline(
    candidate: &ClaudeDeviceProfile,
    baseline: &ClaudeDeviceProfile,
) -> bool {
    matches!(
        (candidate.version, baseline.version),
        (Some(candidate_version), Some(baseline_version)) if candidate_version == baseline_version
    ) && candidate.package_version == baseline.package_version
        && candidate.runtime_version == baseline.runtime_version
}

fn pin_claude_device_profile_platform(
    mut profile: ClaudeDeviceProfile,
    baseline: &ClaudeDeviceProfile,
) -> ClaudeDeviceProfile {
    profile.os.clone_from(&baseline.os);
    profile.arch.clone_from(&baseline.arch);
    profile
}

fn normalize_claude_device_profile(
    mut profile: ClaudeDeviceProfile,
    baseline: &ClaudeDeviceProfile,
) -> ClaudeDeviceProfile {
    profile = pin_claude_device_profile_platform(profile, baseline);
    if !meets_claude_device_profile_baseline(&profile, baseline) {
        profile.user_agent.clone_from(&baseline.user_agent);
        profile
            .package_version
            .clone_from(&baseline.package_version);
        profile
            .runtime_version
            .clone_from(&baseline.runtime_version);
        profile.version = baseline.version;
    }
    profile
}

fn extract_claude_device_profile(
    headers: Option<&HeaderMap>,
    defaults: &ClaudeHeaderDefaults,
) -> Option<ClaudeDeviceProfile> {
    let user_agent = header_value_case_insensitive(headers, "User-Agent");
    let version = parse_claude_cli_version(&user_agent)?;
    if !is_claude_code_native_user_agent(&user_agent) {
        return None;
    }
    let baseline = default_claude_device_profile(defaults);
    let package_version = first_non_empty_header(
        headers,
        "X-Stainless-Package-Version",
        &baseline.package_version,
    );
    let package_version = if valid_semver_triplet(&package_version, false) {
        package_version
    } else {
        baseline.package_version.clone()
    };
    let runtime_version = first_non_empty_header(
        headers,
        "X-Stainless-Runtime-Version",
        &baseline.runtime_version,
    );
    let runtime_version = if valid_semver_triplet(&runtime_version, true) {
        runtime_version
    } else {
        baseline.runtime_version.clone()
    };
    Some(ClaudeDeviceProfile {
        user_agent,
        package_version,
        runtime_version,
        os: first_non_empty_header(headers, "X-Stainless-Os", &baseline.os),
        arch: first_non_empty_header(headers, "X-Stainless-Arch", &baseline.arch),
        version: Some(version),
    })
}

pub(crate) fn plausible_claude_code_user_agent(
    user_agent: &str,
    defaults: &ClaudeHeaderDefaults,
) -> bool {
    let user_agent = user_agent.trim();
    if !is_claude_code_native_user_agent(user_agent) {
        return false;
    }
    matches!(
        (
            parse_claude_cli_version(user_agent),
            parse_claude_cli_version(&default_claude_device_profile(defaults).user_agent),
        ),
        (Some(candidate), Some(baseline)) if plausible_claude_cli_version(candidate, baseline)
    )
}

pub(crate) fn parse_claude_code_user_agent_details(user_agent: &str) -> (String, String) {
    let Some((_, details)) = user_agent.trim().split_once(" (external, ") else {
        return (String::new(), String::new());
    };
    let Some(details) = details.strip_suffix(')') else {
        return (String::new(), String::new());
    };
    let mut parts = details.split(',').map(str::trim);
    let entrypoint = parts.next().unwrap_or_default().to_ascii_lowercase();
    let agent_sdk = parts
        .next()
        .and_then(|part| part.strip_prefix("agent-sdk/"))
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    (entrypoint, agent_sdk)
}

fn is_claude_code_native_user_agent(user_agent: &str) -> bool {
    let Some(version) = parse_claude_cli_version(user_agent) else {
        return false;
    };
    let prefix = format!(
        "claude-cli/{}.{}.{} (external, ",
        version.major, version.minor, version.patch
    );
    if !user_agent.starts_with(&prefix) || !user_agent.ends_with(')') {
        return false;
    }
    let (entrypoint, agent_sdk) = parse_claude_code_user_agent_details(user_agent);
    !entrypoint.is_empty()
        && !entrypoint.contains([',', ')'])
        && (agent_sdk.is_empty() || valid_semver_triplet(&agent_sdk, false))
}

fn valid_semver_triplet(value: &str, require_v: bool) -> bool {
    let value = if require_v {
        let Some(value) = value.strip_prefix('v') else {
            return false;
        };
        value
    } else {
        value
    };
    let mut parts = value.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None)
            if [a, b, c].iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    )
}

fn first_non_empty_header(headers: Option<&HeaderMap>, name: &str, fallback: &str) -> String {
    let value = header_value_case_insensitive(headers, name);
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}

fn claude_device_profile_scope_key(auth_id: Option<&str>, api_key: &str) -> String {
    if let Some(auth_id) = auth_id.map(str::trim).filter(|value| !value.is_empty()) {
        format!("auth:{auth_id}")
    } else if !api_key.trim().is_empty() {
        format!("api_key:{}", api_key.trim())
    } else {
        "global".to_owned()
    }
}

fn claude_device_profile_cache_key_scoped(
    auth_id: Option<&str>,
    api_key: &str,
    profile: Option<&ClaudeDeviceProfile>,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(claude_device_profile_scoped_key(auth_id, api_key, profile).as_bytes())
    )
}

#[cfg(test)]
pub fn claude_device_profile_kv_key(auth_id: Option<&str>, api_key: &str) -> String {
    claude_device_profile_kv_key_scoped(auth_id, api_key, None)
}

fn claude_device_profile_kv_key_scoped(
    auth_id: Option<&str>,
    api_key: &str,
    profile: Option<&ClaudeDeviceProfile>,
) -> String {
    format!(
        "cpa:claude:device-profile:{}",
        hash_key_part(&claude_device_profile_scoped_key(auth_id, api_key, profile))
    )
}

fn claude_device_profile_lock_kv_key_scoped(
    auth_id: Option<&str>,
    api_key: &str,
    profile: Option<&ClaudeDeviceProfile>,
) -> String {
    format!(
        "cpa:claude:device-profile-lock:{}",
        hash_key_part(&claude_device_profile_scoped_key(auth_id, api_key, profile))
    )
}

fn claude_device_profile_scoped_key(
    auth_id: Option<&str>,
    api_key: &str,
    profile: Option<&ClaudeDeviceProfile>,
) -> String {
    let mut key = claude_device_profile_scope_key(auth_id, api_key);
    let Some(profile) = profile else {
        return key;
    };
    let (entrypoint, _) = parse_claude_code_user_agent_details(&profile.user_agent);
    let subclient = match entrypoint.as_str() {
        "" | "cli" => "",
        "sdk-cli" | "claude-vscode" => entrypoint.as_str(),
        _ => "other",
    };
    if !subclient.is_empty() {
        key.push_str("|subclient:");
        key.push_str(subclient);
    }
    key
}

fn remove_header(headers: &mut HeaderMap, name: &str) {
    headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
}

fn set_header(headers: &mut HeaderMap, name: &str, value: &str) {
    remove_header(headers, name);
    headers.insert(name.to_owned(), vec![value.to_owned()]);
}
