// ref: internal/config/max_context_length_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::ProviderCompatConfig;

const YAML: &str = r#"codex-api-key: [{models: [{name: codex, alias: c, max-context-length: 1048576}]}]
claude-api-key: [{models: [{name: claude, alias: c, max-context-length: 1048576}]}]
gemini-api-key: [{models: [{name: gemini, alias: g, max-context-length: 1048576}]}]
interactions-api-key: [{models: [{name: interactions, alias: i, max-context-length: 1048576}]}]
xai-api-key: [{models: [{name: xai, alias: x, max-context-length: 1048576}]}]
openai-compatibility: [{models: [{name: compat, alias: o, max-context-length: 1048576}]}]
"#;
