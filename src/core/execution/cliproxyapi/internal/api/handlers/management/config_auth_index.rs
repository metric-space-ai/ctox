// ref: internal/api/handlers/management/config_auth_index.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt::Write;

use sha2::{Digest, Sha256};

use crate::internal::config::ProviderCompatConfig;
use crate::internal::watcher::synthesizer::helpers::StableIdGenerator;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ManagementConfigAuthIndex {
    pub provider: String,
    pub auth_index: String,
    pub ordinal: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ManagementOpenAiCompatibilityView {
    pub name: String,
    pub priority: i32,
    pub disabled: bool,
    pub prefix: String,
    pub support_prompt_cache_key: bool,
    pub disable_cooling: bool,
    pub credential_count: usize,
    pub model_count: usize,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Builds the upstream-compatible 16-hex-character management index from a
/// CTOX public account ID.
///
/// Upstream may seed this index with auth-file paths or API keys. CTOX keeps
/// those values outside management DTOs and uses the upstream `id:<ID>`
/// fallback exclusively.
pub fn management_auth_index_for_id(auth_id: &str) -> Option<String> {
    let auth_id = auth_id.trim();
    if auth_id.is_empty() {
        return None;
    }
    let digest = Sha256::digest(format!("id:{auth_id}").as_bytes());
    let mut index = String::with_capacity(16);
    for byte in &digest[..8] {
        write!(&mut index, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Some(index)
}

/// Produces a secret-free management projection. Stable IDs are derived
/// inside this boundary; only the live public index (or its deterministic
/// public fallback) escapes it.
#[must_use]
pub fn management_config_auth_indices(
    config: &ProviderCompatConfig,
    live_index_by_id: &BTreeMap<String, String>,
) -> Vec<ManagementConfigAuthIndex> {
    let mut ids = StableIdGenerator::default();
    let mut out = Vec::new();
    for (provider, entries) in [
        ("gemini", &config.gemini_api_key),
        ("gemini-interactions", &config.interactions_api_key),
        ("claude", &config.claude_api_key),
        ("codex", &config.codex_api_key),
        ("xai", &config.xai_api_key),
    ] {
        for (ordinal, entry) in entries.iter().enumerate() {
            let (id, _) = ids.next(
                &format!("{provider}:apikey"),
                &[&entry.api_key, &entry.base_url],
            );
            out.push(index_view(
                provider,
                ordinal,
                &entry.prefix,
                false,
                &id,
                live_index_by_id,
            ));
        }
    }
    for (ordinal, entry) in config.vertex_api_key.iter().enumerate() {
        let (id, _) = ids.next(
            "vertex:apikey",
            &[&entry.api_key, &entry.base_url, &entry.proxy_url],
        );
        out.push(index_view(
            "vertex",
            ordinal,
            &entry.prefix,
            false,
            &id,
            live_index_by_id,
        ));
    }
    for (provider_ordinal, provider) in config.openai_compatibility.iter().enumerate() {
        let normalized_name = provider.name.trim().to_ascii_lowercase();
        let provider_name = if normalized_name.is_empty() {
            "openai-compatibility"
        } else {
            normalized_name.as_str()
        };
        let kind = format!("openai-compatibility:{provider_name}");
        if provider.api_key_entries.is_empty() {
            let (id, _) = ids.next(&kind, &[&provider.base_url]);
            out.push(index_view(
                provider_name,
                provider_ordinal,
                &provider.prefix,
                provider.disabled,
                &id,
                live_index_by_id,
            ));
        } else {
            for (key_ordinal, entry) in provider.api_key_entries.iter().enumerate() {
                let (id, _) = ids.next(
                    &kind,
                    &[&entry.api_key, &provider.base_url, &entry.proxy_url],
                );
                out.push(index_view(
                    provider_name,
                    key_ordinal,
                    &provider.prefix,
                    provider.disabled,
                    &id,
                    live_index_by_id,
                ));
            }
        }
    }
    out
}

#[must_use]
pub fn management_openai_compatibility_views(
    config: &ProviderCompatConfig,
) -> Vec<ManagementOpenAiCompatibilityView> {
    config
        .openai_compatibility
        .iter()
        .map(|entry| ManagementOpenAiCompatibilityView {
            name: entry.name.clone(),
            priority: entry.priority,
            disabled: entry.disabled,
            prefix: entry.prefix.clone(),
            support_prompt_cache_key: entry.support_prompt_cache_key,
            disable_cooling: entry.disable_cooling,
            credential_count: entry.api_key_entries.len(),
            model_count: entry.models.len(),
        })
        .collect()
}

fn index_view(
    provider: &str,
    ordinal: usize,
    prefix: &str,
    disabled: bool,
    id: &str,
    live_index_by_id: &BTreeMap<String, String>,
) -> ManagementConfigAuthIndex {
    let live = live_index_by_id
        .get(id)
        .map(String::as_str)
        .unwrap_or_default()
        .trim();
    ManagementConfigAuthIndex {
        provider: provider.to_owned(),
        auth_index: if live.is_empty() {
            management_auth_index_for_id(id).unwrap_or_default()
        } else {
            live.to_owned()
        },
        ordinal,
        prefix: prefix.trim().to_owned(),
        disabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

#[test]
    fn projection_uses_live_index_and_never_contains_api_keys() {
        let config = ProviderCompatConfig {
            codex_api_key: vec![crate::internal::config::CodexKey {
                api_key: "do-not-project".into(),
                base_url: "https://example.test/v1".into(),
                prefix: " team ".into(),
                ..crate::internal::config::CodexKey::default()
            }],
            ..ProviderCompatConfig::default()
        };
        let mut ids = StableIdGenerator::default();
        let (id, _) = ids.next(
            "codex:apikey",
            &["do-not-project", "https://example.test/v1"],
        );
        let views =
            management_config_auth_indices(&config, &BTreeMap::from([(id, "live-index".into())]));
        assert_eq!(views[0].auth_index, "live-index");
        assert_eq!(views[0].prefix, "team");
        assert!(!serde_json::to_string(&views)
            .unwrap()
            .contains("do-not-project"));
    }
}
