// Origin: CTOX
// License: Apache-2.0

//! Shared OAuth2 helpers for Microsoft Graph (and Azure AD / Entra ID).
//!
//! Two flows are supported:
//!
//! * **Resource Owner Password Credentials (ROPC)** — `acquire_ropc_token`. Used
//!   when an interactive user account (username + password) is available. If no
//!   client_id is supplied, the well-known Microsoft Office public client is
//!   used so that ROPC works on tenants that have not registered a custom app.
//! * **Client Credentials** — `acquire_app_token`. Used for service principals
//!   with `tenant_id` + `client_id` + `client_secret` and Application
//!   permissions on Microsoft Graph.
//!
//! These helpers are intentionally minimal: each call performs one HTTP form
//! POST against `https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token`
//! and returns the resulting `access_token`. Callers are responsible for
//! caching / refreshing tokens as needed.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::mission::communication_email_native::http_request;

/// Microsoft Office well-known public client id. Works for ROPC against most
/// tenants without the operator having to register a custom application.
pub(crate) const ROPC_PUBLIC_CLIENT_ID: &str = "d3590ed6-52b3-4102-aeff-aad2292ab01c";

const TOKEN_ENDPOINT_HOST: &str = "https://login.microsoftonline.com";
const GRAPH_DEFAULT_SCOPE: &str = "https%3A%2F%2Fgraph.microsoft.com%2F.default";

/// Acquire a Graph access token via the OAuth2 client-credentials flow.
pub(crate) fn acquire_app_token(
    tenant_id: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    if tenant_id.trim().is_empty() {
        bail!("acquire_app_token requires a non-empty tenant_id");
    }
    if client_id.trim().is_empty() {
        bail!("acquire_app_token requires a non-empty client_id");
    }
    if client_secret.trim().is_empty() {
        bail!("acquire_app_token requires a non-empty client_secret");
    }
    let token_url = format!("{TOKEN_ENDPOINT_HOST}/{tenant_id}/oauth2/v2.0/token");
    let form_body = format!(
        "client_id={}&scope={}&client_secret={}&grant_type=client_credentials",
        urlencoding_encode(client_id),
        GRAPH_DEFAULT_SCOPE,
        urlencoding_encode(client_secret),
    );
    request_token(&token_url, &form_body, "OAuth2 client-credentials")
}

/// Acquire a Graph access token via the OAuth2 ROPC (password) flow.
///
/// `tenant_id` may be empty — in that case `organizations` is used so the flow
/// works against any work or school tenant. Passing an empty `client_id` makes
/// the call use [`ROPC_PUBLIC_CLIENT_ID`].
pub(crate) fn acquire_ropc_token(
    tenant_id: &str,
    username: &str,
    password: &str,
    client_id: &str,
) -> Result<String> {
    if username.trim().is_empty() {
        bail!("acquire_ropc_token requires a non-empty username");
    }
    if password.is_empty() {
        bail!("acquire_ropc_token requires a non-empty password");
    }
    let effective_tenant = if tenant_id.trim().is_empty() {
        "organizations"
    } else {
        tenant_id
    };
    let effective_client_id = if client_id.trim().is_empty() {
        ROPC_PUBLIC_CLIENT_ID
    } else {
        client_id
    };
    let token_url = format!("{TOKEN_ENDPOINT_HOST}/{effective_tenant}/oauth2/v2.0/token");
    let form_body = format!(
        "client_id={}&scope={}+offline_access&username={}&password={}&grant_type=password",
        urlencoding_encode(effective_client_id),
        GRAPH_DEFAULT_SCOPE,
        urlencoding_encode(username),
        urlencoding_encode(password),
    );
    request_token(&token_url, &form_body, "ROPC")
}

fn request_token(token_url: &str, form_body: &str, label: &str) -> Result<String> {
    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    );
    let response = http_request("POST", token_url, &headers, Some(form_body.as_bytes()))?;
    if !(200..300).contains(&response.status) {
        let body_text = String::from_utf8_lossy(&response.body);
        bail!(
            "{label} token request failed (HTTP {}): {body_text}",
            response.status
        );
    }
    let token_json: Value = serde_json::from_slice(&response.body)
        .with_context(|| format!("failed to parse {label} token response"))?;
    token_json
        .get("access_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .with_context(|| format!("access_token missing from {label} response"))
}

pub(crate) fn urlencoding_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoding_encodes_reserved_characters() {
        assert_eq!(urlencoding_encode("plain"), "plain");
        assert_eq!(urlencoding_encode("a b"), "a%20b");
        assert_eq!(urlencoding_encode("a+b/c=d?e&f"), "a%2Bb%2Fc%3Dd%3Fe%26f");
        assert_eq!(urlencoding_encode("ä"), "%C3%A4");
        assert_eq!(urlencoding_encode("user@example.com"), "user%40example.com");
        assert_eq!(urlencoding_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn ropc_token_rejects_empty_username() {
        let err = acquire_ropc_token("contoso", "", "secret", "").unwrap_err();
        assert!(err.to_string().contains("username"));
    }

    #[test]
    fn ropc_token_rejects_empty_password() {
        let err = acquire_ropc_token("contoso", "user@example.com", "", "").unwrap_err();
        assert!(err.to_string().contains("password"));
    }

    #[test]
    fn app_token_rejects_missing_inputs() {
        assert!(acquire_app_token("", "id", "secret")
            .unwrap_err()
            .to_string()
            .contains("tenant_id"));
        assert!(acquire_app_token("contoso", "", "secret")
            .unwrap_err()
            .to_string()
            .contains("client_id"));
        assert!(acquire_app_token("contoso", "id", "")
            .unwrap_err()
            .to_string()
            .contains("client_secret"));
    }
}
