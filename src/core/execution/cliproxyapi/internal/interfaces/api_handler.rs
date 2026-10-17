// ref: internal/interfaces/api_handler.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use serde_json::Value;

/// Common contract for API handler identity and model metadata.
pub trait ApiHandler: Send + Sync {
    fn handler_type(&self) -> &str;
    fn models(&self) -> Vec<BTreeMap<String, Value>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Handler;

    impl ApiHandler for Handler {
        fn handler_type(&self) -> &str {
            "claude"
        }

        fn models(&self) -> Vec<BTreeMap<String, Value>> {
            vec![BTreeMap::from([(
                "id".to_owned(),
                Value::String("claude-test".to_owned()),
            )])]
        }
    }
}
