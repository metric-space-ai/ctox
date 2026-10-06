// ref: internal/auth/meta/meta.go:87-222,474-564 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned snapshots; only the injected manager store persists
// License: MIT (upstream); modifications AGPL-3.0-only

use super::flow::{rfc3339, MetaAuthBundle, DEFAULT_API_BASE_URL};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Default)]
pub struct MetaTokenStorage {
    pub access_token: String,
    pub dca_token: String,
    pub api_key: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expired: String,
    pub dca_expired: String,
    pub dca_expires_at: i64,
    pub last_refresh: String,
    pub base_url: String,
    pub email: String,
    pub name: String,
    metadata: Option<BTreeMap<String, Value>>,
}
impl fmt::Debug for MetaTokenStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaTokenStorage")
            .field("has_minted_key", &!self.api_key.is_empty())
            .field("has_dca_token", &!self.dca_token.is_empty())
            .finish_non_exhaustive()
    }
}
impl MetaTokenStorage {
    pub fn from_bundle(bundle: &MetaAuthBundle, now: DateTime<Utc>) -> Option<Self> {
        let token = bundle.token_data.as_ref()?;
        let dca_expired = if token.expires_at > 0 {
            DateTime::<Utc>::from_timestamp(token.expires_at, 0)
                .map(rfc3339)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut api_key = String::new();
        let mut base_url = DEFAULT_API_BASE_URL.to_owned();
        let mut email = bundle.email.clone();
        let mut name = bundle.name.clone();
        if let Some(minted) = &bundle.minted_key {
            api_key = minted.api_key.clone();
            if !minted.base_url.trim().is_empty() {
                base_url = minted.base_url.trim().into();
            }
            if !minted.user_email.is_empty() {
                email = minted.user_email.clone();
            }
            if !minted.user_full_name.is_empty() {
                name = minted.user_full_name.clone();
            }
        }
        let (access_token, expired) = if api_key.is_empty() {
            (token.access_token.clone(), dca_expired.clone())
        } else {
            // An API key does not inherit the DCA token's expiry.
            (api_key.clone(), String::new())
        };
        Some(Self {
            access_token,
            dca_token: token.access_token.clone(),
            api_key,
            token_type: token.token_type.clone(),
            expires_in: token.expires_in,
            expired,
            dca_expired,
            dca_expires_at: token.expires_at,
            last_refresh: rfc3339(now),
            base_url,
            email,
            name,
            metadata: None,
        })
    }
    pub fn set_metadata(&mut self, metadata: BTreeMap<String, Value>) {
        self.metadata = Some(metadata);
    }
    /// Build one current credential snapshot for the existing injected store.
    /// Only a login without current metadata may inherit old noncredential settings.
    pub fn snapshot(&self, previous: Option<&BTreeMap<String, Value>>) -> BTreeMap<String, Value> {
        let mut data = BTreeMap::from([
            ("type".into(), Value::String("meta".into())),
            ("auth_kind".into(), Value::String("oauth".into())),
            (
                "access_token".into(),
                Value::String(self.access_token.clone()),
            ),
        ]);
        for (key, value) in [
            ("dca_token", &self.dca_token),
            ("api_key", &self.api_key),
            ("token_type", &self.token_type),
            ("expired", &self.expired),
            ("dca_expired", &self.dca_expired),
            ("last_refresh", &self.last_refresh),
            ("base_url", &self.base_url),
            ("email", &self.email),
            ("name", &self.name),
        ] {
            if !value.is_empty() {
                data.insert(key.into(), Value::String(value.clone()));
            }
        }
        if self.expires_in > 0 {
            data.insert("expires_in".into(), Value::from(self.expires_in));
        }
        if self.dca_expires_at > 0 {
            data.insert("dca_expires_at".into(), Value::from(self.dca_expires_at));
        }
        let metadata = self.metadata.as_ref().or(previous);
        if let Some(metadata) = metadata {
            for (key, value) in metadata {
                if credential_field(key) {
                    continue;
                }
                if !data.contains_key(key) || self.metadata.is_some() {
                    data.insert(key.clone(), value.clone());
                }
            }
        }
        data
    }
}
fn credential_field(key: &str) -> bool {
    matches!(
        key,
        "type"
            | "auth_kind"
            | "access_token"
            | "token_type"
            | "dca_token"
            | "api_key"
            | "expires_in"
            | "expired"
            | "dca_expired"
            | "dca_expires_at"
            | "last_refresh"
    )
}
// ref: internal/auth/meta/meta.go:536-564
pub fn credential_file_name(email: &str, subject: &str) -> String {
    let email = email.trim();
    let suffix = |identity: &str| {
        Sha256::digest(identity.as_bytes())[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    if !email.is_empty() {
        let sanitized: String = email
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .take(120)
            .collect();
        format!("meta-{sanitized}-{}.json", suffix(email))
    } else if !subject.trim().is_empty() {
        format!("meta-{}.json", suffix(subject.trim()))
    } else {
        "meta-oauth.json".into()
    }
}
