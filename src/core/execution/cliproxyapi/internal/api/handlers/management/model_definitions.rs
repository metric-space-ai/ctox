// ref: internal/api/handlers/management/model_definitions.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use crate::internal::registry::{static_model_definitions_by_channel, StaticModelCatalogError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticModelDefinitionsError {
    MissingChannel,
    UnknownChannel(String),
    InvalidCatalog,
}

impl fmt::Display for StaticModelDefinitionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingChannel => formatter.write_str("channel is required"),
            Self::UnknownChannel(_) => formatter.write_str("unknown channel"),
            Self::InvalidCatalog => formatter.write_str("static model catalog is invalid"),
        }
    }
}

impl std::error::Error for StaticModelDefinitionsError {}

pub fn static_model_definitions_payload(
    channel: &str,
) -> Result<Vec<u8>, StaticModelDefinitionsError> {
    let channel = channel.trim().to_ascii_lowercase();
    if channel.is_empty() {
        return Err(StaticModelDefinitionsError::MissingChannel);
    }
    let models = static_model_definitions_by_channel(&channel)
        .map_err(|StaticModelCatalogError::InvalidCatalog(_)| {
            StaticModelDefinitionsError::InvalidCatalog
        })?
        .ok_or_else(|| StaticModelDefinitionsError::UnknownChannel(channel.clone()))?;
    serde_json::to_vec(&serde_json::json!({"channel": channel, "models": models}))
        .map_err(|_| StaticModelDefinitionsError::InvalidCatalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_management_channels_have_pinned_upstream_shape_and_order() {
        let payload = static_model_definitions_payload(" CLAUDE ").unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["channel"], "claude");
        assert_eq!(value["models"].as_array().unwrap().len(), 18);
        assert_eq!(value["models"][0]["id"], "claude-haiku-4-5-20251001");
        assert_eq!(value["models"][17]["id"], "claude-3-5-haiku-20241022");
        let codex: serde_json::Value =
            serde_json::from_slice(&static_model_definitions_payload("codex").unwrap()).unwrap();
        assert_eq!(codex["models"].as_array().unwrap().len(), 14);
        assert_eq!(codex["models"][9]["id"], "gpt-image-1.5");
        assert_eq!(codex["models"][10]["id"], "gpt-image-2");
        let grok: serde_json::Value =
            serde_json::from_slice(&static_model_definitions_payload("grok").unwrap()).unwrap();
        assert_eq!(grok["channel"], "grok");
        assert_eq!(grok["models"].as_array().unwrap().len(), 18);
    }

    #[test]
    fn unported_channels_are_explicit_not_empty_successes() {
        assert_eq!(
            static_model_definitions_payload("unknown"),
            Err(StaticModelDefinitionsError::UnknownChannel(
                "unknown".to_owned()
            ))
        );
    }
}
