//! Box connector (`slug: box`), ported from Activepieces `piece-box` (MIT,
//! `packages/pieces/community/box/src`).
//!
//! This adapter deliberately declares **no tools**. `piece-box` has no actions:
//! its only action is `createCustomApiCallAction` (excluded by the port rules)
//! and it ships three webhook triggers (`new_file`, `new_folder`, `new_comment`),
//! which the port rules exclude as well. The only HTTP endpoints anywhere in the
//! TypeScript source are the trigger plumbing in `src/lib/common/index.ts`
//! (`POST {baseUrl}/webhooks` / `DELETE {baseUrl}/webhooks/{id}`), which exist
//! solely to register the excluded webhooks; every other Box capability (files,
//! folders, search, comments, uploads) has no endpoint in this piece's source,
//! and ported requests must never invent REST paths.
//!
//! The toolkit is still registered so the catalog entry and its auth are
//! available: the OAuth2 connection is replicated exactly from `boxAuth` in
//! `src/index.ts` (authorize `https://account.box.com/api/oauth2/authorize`,
//! token `https://api.box.com/oauth2/token`, scopes `manage_webhook`,
//! `root_readonly`, `root_readwrite`, no PKCE), and [`BoxStorage::with_api_base`]
//! defaults to the piece's `common.baseUrl` (`https://api.box.com/2.0`), so
//! future actions of a newer `piece-box` only need a tool plus a
//! `// AP: <file>` request arm here.
use anyhow::{bail, Result};
use serde_json::Value;

use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "box";

/// `common.baseUrl` of the piece (`src/lib/common/index.ts`).
pub const API_BASE: &str = "https://api.box.com/2.0";

/// OAuth2 scopes exactly as `boxAuth` declares them (`src/index.ts`).
pub const SCOPES: &[&str] = &["manage_webhook", "root_readonly", "root_readwrite"];

pub struct BoxStorage {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for BoxStorage {
    fn default() -> Self {
        Self::with_api_base(API_BASE)
    }
}

impl BoxStorage {
    pub fn with_api_base(api_base: &str) -> Self {
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Box".into(),
            description: LocalizedText::new(
                "Box-Anbindung: das Activepieces-Piece enthält nur Webhook-Trigger und einen \
                 Custom-API-Call, daher gibt es noch keine portierten Werkzeuge.",
                "Box connector: the Activepieces piece ships only webhook triggers and a custom \
                 API call, so no tools are ported yet.",
            ),
            auth_schemes: vec![AuthScheme::OAuth2(OAuth2Scheme {
                authorize_url: "https://account.box.com/api/oauth2/authorize".into(),
                token_url: "https://api.box.com/oauth2/token".into(),
                default_scopes: SCOPES.iter().map(|s| (*s).into()).collect(),
                pkce: false,
                scope_separator: " ".into(),
                refreshable: true,
                extra_authorize_params: vec![],
            })],
            // No actions exist in the piece source; see the module documentation.
            tools: vec![],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// Base URL every Box request of the piece uses (`common.baseUrl`).
    pub fn api_base(&self) -> &str {
        &self.api_base
    }
}

impl ToolkitAdapter for BoxStorage {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, _args: &Value) -> Result<HttpRequest> {
        bail!(
            "unknown Box tool `{tool}`: the Activepieces piece-box has no actions to port \
             (see the module documentation)"
        )
    }

    fn parse_response(&self, tool: &str, _response: &HttpResponse) -> Result<Value> {
        bail!(
            "unknown Box tool `{tool}`: the Activepieces piece-box has no actions to port \
             (see the module documentation)"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AuthScheme;

    #[test]
    fn auth_is_replicated_from_boxauth() {
        let b = BoxStorage::default();
        assert_eq!(b.spec().slug, "box");
        assert_eq!(b.spec().name, "Box");
        assert!(!b.spec().description.de.is_empty());
        assert!(!b.spec().description.en.is_empty());
        let [AuthScheme::OAuth2(o)] = b.spec().auth_schemes.as_slice() else {
            panic!("expected the piece's OAuth2 scheme");
        };
        assert_eq!(o.authorize_url, "https://account.box.com/api/oauth2/authorize");
        assert_eq!(o.token_url, "https://api.box.com/oauth2/token");
        assert_eq!(o.default_scopes, ["manage_webhook", "root_readonly", "root_readwrite"]);
        assert!(!o.pkce);
    }

    #[test]
    fn api_base_matches_the_piece() {
        assert_eq!(API_BASE, "https://api.box.com/2.0");
        assert_eq!(BoxStorage::default().api_base(), API_BASE);
        assert_eq!(
            BoxStorage::with_api_base("https://api.box.com/2.0/").api_base(),
            API_BASE
        );
    }

    #[test]
    fn no_tools_and_no_invented_endpoints() {
        let b = BoxStorage::default();
        assert!(
            b.spec().tools.is_empty(),
            "piece-box has no actions; tools here would need endpoints the source does not contain"
        );
        let err = b
            .build_request("BOX_GET_FILE", &serde_json::json!({"id": "123"}))
            .unwrap_err();
        assert!(err.to_string().contains("no actions"), "{err}");
    }
}
