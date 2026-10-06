// ref: internal/auth/devin/record.go:16-132
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned record; native owner persists; absent limits stay unknown
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    auth::{
        format_session_token, DevinAuthError, DevinAuthService, DevinSelfProfile,
        DEFAULT_SERVER_URL,
    },
    user_status::{apply_user_status, DevinUserStatus},
};
use crate::sdk::cliproxy::auth::{Auth, AuthStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

impl DevinAuthService {
    /// Profile/status enrichment is best-effort. Dropping this future cancels
    /// whichever selected transport operation is running, with no partial write.
    pub async fn create_auth_record(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Auth, DevinAuthError> {
        let token = format_session_token(token);
        if token.is_empty() {
            return Err(DevinAuthError::EmptySessionToken);
        }
        let profile = self.fetch_self_profile(&token).await.unwrap_or_default();
        let status = self
            .status_service()
            .fetch_user_status(&token, "")
            .await
            .ok();
        Ok(create_record(&token, profile, status.as_ref(), now))
    }
    pub async fn exchange_code_for_auth(
        &self,
        code: &str,
        code_verifier: &str,
        now: DateTime<Utc>,
    ) -> Result<Auth, DevinAuthError> {
        let token = self.exchange_code_for_token(code, code_verifier).await?;
        self.create_auth_record(&token, now).await
    }
}
fn create_record(
    token: &str,
    mut profile: DevinSelfProfile,
    status: Option<&DevinUserStatus>,
    now: DateTime<Utc>,
) -> Auth {
    if let Some(status) = status {
        if profile.user_name.is_empty() {
            profile.user_name.clone_from(&status.user_name);
        }
        if profile.user_id.is_empty() {
            profile.user_id.clone_from(&status.user_id);
        }
        if profile.org_id.is_empty() {
            profile.org_id.clone_from(&status.org_id);
        }
    }
    let email = status.map(|value| value.email.as_str()).unwrap_or_default();
    let plan = status.map(|value| value.plan.as_str()).unwrap_or_default();
    let identifier = if !profile.user_name.is_empty() {
        profile.user_name.clone()
    } else if !profile.user_id.is_empty() {
        profile.user_id.clone()
    } else {
        digest_identifier(token)
    };
    let filename_identifier: String = identifier
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.' | '@') {
                value
            } else {
                '_'
            }
        })
        .collect();
    let filename_identifier =
        if filename_identifier != identifier || filename_identifier.len() > 160 {
            digest_identifier(&identifier)
        } else {
            filename_identifier
        };
    let filename = format!("devin-{filename_identifier}.json");
    let mut auth = Auth::default();
    auth.id = filename.clone();
    auth.file_name = filename;
    auth.provider = "devin".into();
    auth.label = if email.is_empty() {
        format!("Devin ({identifier})")
    } else {
        format!("Devin ({identifier} - {email})")
    };
    auth.status = AuthStatus::Active;
    for (key, value) in [
        ("api_key", token),
        ("session_token", token),
        ("user_name", profile.user_name.as_str()),
        ("user_id", profile.user_id.as_str()),
        ("org_id", profile.org_id.as_str()),
        ("auth_kind", "oauth"),
    ] {
        auth.attributes.insert(key.into(), value.into());
        auth.metadata
            .insert(key.into(), Value::String(value.into()));
    }
    auth.attributes
        .insert("base_url".into(), DEFAULT_SERVER_URL.into());
    auth.metadata
        .insert("type".into(), Value::String("devin".into()));
    for (key, value) in [("email", email), ("plan", plan)] {
        if !value.is_empty() {
            auth.attributes.insert(key.into(), value.into());
            auth.metadata
                .insert(key.into(), Value::String(value.into()));
        }
    }
    if let Some(status) = status {
        // Preserve profile precedence when status has a different identity;
        // apply only its quota observation to the newly constructed record.
        auth.quota = apply_user_status(&Auth::default(), status, now).quota;
    }
    auth.quota.observed_at = now;
    auth
}
fn digest_identifier(value: &str) -> String {
    let hash = Sha256::digest(value.as_bytes());
    let suffix: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    format!("user-{suffix}")
}

#[cfg(test)]
#[path = "record_test.rs"]
mod tests;
