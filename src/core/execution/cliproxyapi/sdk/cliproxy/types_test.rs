// Origin: CTOX supplemental tests for sdk/cliproxy/types.go.
// License: AGPL-3.0-only

use std::sync::{Arc, Mutex};

use super::providers::LoadContext;
use super::types::*;

struct Parser;

impl PluginAuthParser for Parser {
    fn parse_auth<'a>(
        &'a self,
        _request: PluginAuthParseRequest,
    ) -> PluginAuthParseFuture<'a, Option<super::auth::Auth>> {
        Box::pin(async { Ok((None, false)) })
    }
}

fn validated_config() -> crate::internal::config::ValidatedRuntimeConfig {
    let config: crate::internal::config::CliproxyRuntimeConfig =
        serde_json::from_value(serde_json::json!({
            "claude_accounts": [{
                "id": "claude-a",
                "access_token_secret": {"scope": "cliproxy", "name": "access"},
                "refresh_token_secret": {"scope": "cliproxy", "name": "refresh"}
            }]
        }))
        .unwrap();
    config.validate().unwrap()
}

#[tokio::test]
async fn token_provider_trait_uses_existing_stateless_provider_capability() {
    let provider = super::providers::new_file_token_client_provider();
    let result = TokenClientProvider::load(&provider, &LoadContext::default(), &validated_config())
        .await
        .unwrap();
    assert_eq!(result.successful_authed, 0);
}
