//! Composio-aligned core types.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Localized user-facing text (DE + EN are mandatory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedText {
    pub de: String,
    pub en: String,
}

impl LocalizedText {
    pub fn new(de: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            de: de.into(),
            en: en.into(),
        }
    }
}

/// OAuth2 endpoint description for one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth2Scheme {
    pub authorize_url: String,
    pub token_url: String,
    #[serde(default)]
    pub default_scopes: Vec<String>,
    /// Use RFC 7636 PKCE (S256).
    pub pkce: bool,
    /// Separator for the `scope` parameter (" " per RFC 6749, some providers use ",").
    #[serde(default = "default_scope_separator")]
    pub scope_separator: String,
    /// Whether the provider issues refresh tokens.
    #[serde(default)]
    pub refreshable: bool,
}

fn default_scope_separator() -> String {
    " ".into()
}

/// Supported authentication schemes (Composio: `AuthSchemeTypes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthScheme {
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2Scheme),
    /// Static API key sent in a header (`header_name: <prefix><key>`).
    ApiKey {
        header_name: String,
        #[serde(default)]
        prefix: String,
    },
    Basic,
    Bearer,
}

impl AuthScheme {
    pub fn kind(&self) -> AuthSchemeKind {
        match self {
            Self::OAuth2(_) => AuthSchemeKind::OAuth2,
            Self::ApiKey { .. } => AuthSchemeKind::ApiKey,
            Self::Basic => AuthSchemeKind::Basic,
            Self::Bearer => AuthSchemeKind::Bearer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthSchemeKind {
    #[serde(rename = "oauth2")]
    OAuth2,
    ApiKey,
    Basic,
    Bearer,
}

/// A tool (Composio: Tool / Action) with JSON-schema input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Globally unique slug, e.g. `GITHUB_GET_REPO`.
    pub slug: String,
    pub toolkit: String,
    pub title: LocalizedText,
    pub description: LocalizedText,
    pub input_schema: Value,
    /// Provider scopes needed (informational, used for scope recommendations).
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Side-effecting tools require approval by the existing CTOX policy gates.
    pub mutating: bool,
}

/// A toolkit (Composio: Toolkit), e.g. `github`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolkitSpec {
    pub slug: String,
    pub name: String,
    pub description: LocalizedText,
    pub auth_schemes: Vec<AuthScheme>,
    pub tools: Vec<ToolSpec>,
}

/// Which client credentials are used (Composio: managed vs. custom auth config).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientCredentialSource {
    /// No client credentials needed (API key / bearer / basic).
    None,
    /// OAuth client registered by the CTOX operator for this instance.
    Instance,
    /// Customer-owned OAuth client ("bring your own app").
    Custom,
}

/// An auth configuration for a toolkit (Composio: AuthConfig).
/// Client secrets are never part of this struct; they live in the secret store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConfig {
    pub id: String,
    pub toolkit: String,
    pub scheme: AuthSchemeKind,
    pub source: ClientCredentialSource,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub enabled: bool,
}

/// Connected-account lifecycle (Composio: ConnectionStatuses subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionStatus {
    Initializing,
    Active,
    Expired,
    Failed,
}

/// Non-secret metadata of a connected account (Composio: ConnectedAccount).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub id: String,
    pub toolkit: String,
    pub auth_config_id: String,
    /// CTOX actor/workspace owning the connection.
    pub owner: String,
    pub status: ConnectionStatus,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Unix seconds; None = no expiry known.
    #[serde(default)]
    pub expires_at: Option<u64>,
    /// Unix seconds when the scheduler should refresh.
    #[serde(default)]
    pub next_refresh_after: Option<u64>,
    #[serde(default)]
    pub status_reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Secret material of a connected account. Never serialized to clients.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSecret {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}

impl std::fmt::Debug for AccountSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountSecret")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("token_type", &self.token_type)
            .finish()
    }
}

/// A pending interactive connection (Composio: ConnectionRequest).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRequest {
    pub account_id: String,
    pub toolkit: String,
    /// URL the user opens to grant access.
    pub redirect_url: String,
    /// Opaque CSRF state; correlates the callback.
    pub state: String,
    pub status: ConnectionStatus,
}
