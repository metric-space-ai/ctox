// ref: internal/auth/codex/jwt_parser.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

/// Claims consumed by Codex account discovery. Parsing does not verify the
/// signature; callers may only use this after OpenAI has returned the token on
/// a successfully authenticated TLS connection.
/// ref: internal/auth/codex/jwt_parser.go:13-30
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct JwtClaims {
    #[serde(default)]
    pub at_hash: String,
    #[serde(default)]
    pub aud: Vec<String>,
    #[serde(default)]
    pub auth_provider: String,
    #[serde(default)]
    pub auth_time: i64,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub exp: i64,
    #[serde(default, rename = "https://api.openai.com/auth")]
    pub codex_auth_info: CodexAuthInfo,
    #[serde(default)]
    pub iat: i64,
    #[serde(default)]
    pub iss: String,
    #[serde(default)]
    pub jti: String,
    #[serde(default)]
    pub rat: i64,
    #[serde(default)]
    pub sid: String,
    #[serde(default)]
    pub sub: String,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct CodexAuthInfo {
    #[serde(default)]
    pub chatgpt_account_id: String,
    #[serde(default)]
    pub chatgpt_plan_type: String,
    #[serde(default)]
    pub chatgpt_subscription_active_start: Value,
    #[serde(default)]
    pub chatgpt_subscription_active_until: Value,
    #[serde(default)]
    pub chatgpt_subscription_last_checked: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub chatgpt_user_id: String,
    #[serde(default)]
    pub groups: Vec<Value>,
    #[serde(default)]
    pub organizations: Vec<Organization>,
    #[serde(default)]
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Organization {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub title: String,
}

impl JwtClaims {
    pub fn user_email(&self) -> &str {
        &self.email
    }

    pub fn account_id(&self) -> &str {
        &self.codex_auth_info.chatgpt_account_id
    }

    /// ChatGPT plan from the access-token claims. Missing or blank claims use
    /// `free`, which is the upstream default when the claim is absent.
    #[must_use]
    pub fn plan_type(&self) -> &str {
        let plan = self.codex_auth_info.chatgpt_plan_type.trim();
        if plan.is_empty() {
            DEFAULT_PLAN_TYPE
        } else {
            plan
        }
    }
}

pub const DEFAULT_PLAN_TYPE: &str = "free";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtParseError {
    InvalidPartCount,
    InvalidEncoding,
    InvalidClaims,
}

impl fmt::Display for JwtParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPartCount => "invalid JWT token format",
            Self::InvalidEncoding => "failed to decode JWT claims",
            Self::InvalidClaims => "failed to parse JWT claims",
        })
    }
}

impl std::error::Error for JwtParseError {}

/// Decodes the claims segment without verifying the JWT signature, matching
/// upstream's post-token-exchange introspection behavior.
/// ref: internal/auth/codex/jwt_parser.go:61-82
pub fn parse_jwt_token(token: &str) -> Result<JwtClaims, JwtParseError> {
    let mut parts = token.split('.');
    let Some(_header) = parts.next() else {
        return Err(JwtParseError::InvalidPartCount);
    };
    let Some(payload) = parts.next() else {
        return Err(JwtParseError::InvalidPartCount);
    };
    let Some(_signature) = parts.next() else {
        return Err(JwtParseError::InvalidPartCount);
    };
    if parts.next().is_some() {
        return Err(JwtParseError::InvalidPartCount);
    }
    let mut padded = payload.to_owned();
    match padded.len() % 4 {
        2 => padded.push_str("=="),
        3 => padded.push('='),
        _ => {}
    }
    let bytes = URL_SAFE
        .decode(padded)
        .map_err(|_| JwtParseError::InvalidEncoding)?;
    serde_json::from_slice(&bytes).map_err(|_| JwtParseError::InvalidClaims)
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::*;

    #[test]
    fn extracts_account_and_email_without_echoing_token_on_error() {
        let claims = serde_json::json!({
            "email": "operator@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct-123",
                "chatgpt_plan_type": "plus"
            }
        });
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let parsed = parse_jwt_token(&format!("header.{payload}.signature")).unwrap();
        assert_eq!(parsed.account_id(), "acct-123");
        assert_eq!(parsed.user_email(), "operator@example.com");
        assert_eq!(parsed.plan_type(), "plus");

        let secret = "not-a-token-do-not-leak";
        let error = parse_jwt_token(secret).unwrap_err();
        assert!(!format!("{error:?} {error}").contains(secret));
    }

    #[test]
    fn preserves_complete_upstream_claim_shape_and_accepts_padding() {
        let claims = serde_json::json!({
            "at_hash": "hash",
            "aud": ["codex", "chatgpt"],
            "auth_provider": "openai",
            "auth_time": 10,
            "email": "operator@example.com",
            "email_verified": true,
            "exp": 20,
            "iat": 11,
            "iss": "https://auth.openai.com",
            "jti": "jwt-id",
            "rat": 12,
            "sid": "session-id",
            "sub": "subject",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct-123",
                "chatgpt_plan_type": "plus",
                "chatgpt_subscription_active_start": 1,
                "chatgpt_subscription_active_until": "later",
                "chatgpt_subscription_last_checked": "2026-08-03T12:00:00Z",
                "chatgpt_user_id": "chat-user",
                "groups": ["group-a", {"id": "group-b"}],
                "organizations": [{
                    "id": "org-a", "is_default": true,
                    "role": "owner", "title": "A"
                }],
                "user_id": "user-a"
            }
        });
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let mut padded = payload;
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        let parsed = parse_jwt_token(&format!("header.{padded}.signature")).unwrap();
        assert_eq!(parsed.aud, ["codex", "chatgpt"]);
        assert!(parsed.email_verified);
        assert_eq!(parsed.codex_auth_info.groups.len(), 2);
        assert_eq!(parsed.codex_auth_info.organizations[0].id, "org-a");
        assert!(parsed
            .codex_auth_info
            .chatgpt_subscription_last_checked
            .is_some());
    }

    #[test]
    fn plan_type_defaults_blank_claims_to_free() {
        assert_eq!(JwtClaims::default().plan_type(), DEFAULT_PLAN_TYPE);
        let whitespace = JwtClaims {
            codex_auth_info: CodexAuthInfo {
                chatgpt_plan_type: "   ".to_owned(),
                ..CodexAuthInfo::default()
            },
            ..JwtClaims::default()
        };
        assert_eq!(whitespace.plan_type(), "free");
        let pro = JwtClaims {
            codex_auth_info: CodexAuthInfo {
                chatgpt_plan_type: "pro".to_owned(),
                ..CodexAuthInfo::default()
            },
            ..JwtClaims::default()
        };
        assert_eq!(pro.plan_type(), "pro");
    }
}
