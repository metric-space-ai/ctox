// ref: internal/config/vertex_compat.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::internal::registry::RegistryThinkingSupport;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct VertexCompatKey {
    pub api_key: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub models: Vec<VertexCompatModel>,
    #[serde(default)]
    pub excluded_models: Vec<String>,
    /// `Some(true)` disables auth/model cooldowns for this credential.
    /// `Some(false)` explicitly enables them. `None` keeps the global policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_cooling: Option<bool>,
    /// `None` or a negative value uses the global request retry.
    /// `Some(0)` disables additional retry rounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_retry: Option<i32>,
}

impl VertexCompatKey {
    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    #[must_use]
    pub fn proxy_url(&self) -> &str {
        &self.proxy_url
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct VertexCompatModel {
    pub name: String,
    pub alias: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub force_mapping: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<RegistryThinkingSupport>,
}

impl VertexCompatModel {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub fn force_mapping(&self) -> bool {
        self.force_mapping
    }
    #[must_use]
    pub fn thinking(&self) -> Option<&RegistryThinkingSupport> {
        self.thinking.as_ref()
    }
}

pub fn sanitize_vertex_compat_keys(entries: &mut Vec<VertexCompatKey>) {
    let mut seen = HashSet::with_capacity(entries.len());
    entries.retain_mut(|entry| {
        entry.api_key = entry.api_key.trim().to_owned();
        if entry.api_key.is_empty() {
            return false;
        }
        entry.prefix = normalize_model_prefix(&entry.prefix);
        entry.base_url = entry.base_url.trim().to_owned();
        entry.proxy_url = entry.proxy_url.trim().to_owned();
        normalize_headers(&mut entry.headers);
        normalize_excluded_models(&mut entry.excluded_models);
        entry.models.retain_mut(|model| {
            model.alias = model.alias.trim().to_owned();
            model.name = model.name.trim().to_owned();
            !model.alias.is_empty() && !model.name.is_empty()
        });
        seen.insert(format!("{}|{}", entry.api_key, entry.base_url))
    });
}

pub(crate) fn normalize_model_prefix(prefix: &str) -> String {
    let prefix = prefix.trim().trim_matches('/');
    if prefix.contains('/') {
        String::new()
    } else {
        prefix.to_owned()
    }
}

pub(crate) fn normalize_headers(headers: &mut BTreeMap<String, String>) {
    *headers = std::mem::take(headers)
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_owned();
            let value = value.trim().to_owned();
            (!key.is_empty() && !value.is_empty()).then_some((key, value))
        })
        .collect();
}

pub(crate) fn normalize_excluded_models(models: &mut Vec<String>) {
    let mut seen = HashSet::with_capacity(models.len());
    *models = std::mem::take(models)
        .into_iter()
        .filter_map(|model| {
            let model = model.trim().to_ascii_lowercase();
            (!model.is_empty() && seen.insert(model.clone())).then_some(model)
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitization_is_stable_deduplicated_and_drops_invalid_models() {
        let mut keys = vec![
            VertexCompatKey {
                api_key: " key ".into(),
                prefix: " /team/ ".into(),
                base_url: " https://vertex.example ".into(),
                proxy_url: " http://proxy ".into(),
                headers: BTreeMap::from([
                    (" X-Test ".into(), " value ".into()),
                    ("empty".into(), " ".into()),
                ]),
                models: vec![
                    VertexCompatModel {
                        name: " model ".into(),
                        alias: " alias ".into(),
                        ..VertexCompatModel::default()
                    },
                    VertexCompatModel {
                        name: "missing-alias".into(),
                        alias: " ".into(),
                        ..VertexCompatModel::default()
                    },
                ],
                excluded_models: vec![" MODEL-* ".into(), "model-*".into()],
                ..VertexCompatKey::default()
            },
            VertexCompatKey {
                api_key: "key".into(),
                base_url: "https://vertex.example".into(),
                ..VertexCompatKey::default()
            },
            VertexCompatKey::default(),
        ];

        sanitize_vertex_compat_keys(&mut keys);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].prefix, "team");
        assert_eq!(keys[0].base_url(), "https://vertex.example");
        assert_eq!(keys[0].proxy_url(), "http://proxy");
        assert_eq!(
            keys[0].headers.get("X-Test").map(String::as_str),
            Some("value")
        );
        assert_eq!(keys[0].models.len(), 1);
        assert_eq!(keys[0].models[0].name(), "model");
        assert_eq!(keys[0].models[0].alias(), "alias");
        assert_eq!(keys[0].excluded_models, ["model-*"]);
    }

    #[test]
    fn cooling_and_retry_overrides_stay_distinct_from_unset() {
        let parsed: VertexCompatKey =
            serde_yaml::from_str("api-key: k\ndisable-cooling: false\nrequest-retry: 0\n").unwrap();
        assert_eq!(parsed.disable_cooling, Some(false));
        assert_eq!(parsed.request_retry, Some(0));

        let unset: VertexCompatKey = serde_yaml::from_str("api-key: k\n").unwrap();
        assert_eq!(unset.disable_cooling, None);
        assert_eq!(unset.request_retry, None);

        let negative: VertexCompatKey =
            serde_yaml::from_str("api-key: k\nrequest-retry: -1\n").unwrap();
        assert_eq!(negative.request_retry, Some(-1));
    }
}
