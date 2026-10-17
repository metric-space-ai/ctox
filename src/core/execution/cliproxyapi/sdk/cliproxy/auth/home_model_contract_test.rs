// ref: sdk/cliproxy/auth/api_key_model_capabilities.go:286-328 @ d7914afd
// ref: sdk/cliproxy/auth/home_v8_model_capabilities_test.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::api_key_model_capabilities_test::{auth, register};
use super::super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;
use crate::internal::config::{CodexKey, CodexModel, ProviderCompatConfig};

fn selected_auth(raw: Option<serde_json::Value>) -> Auth {
    let mut selected = auth("legacy-home", "legacy-home-key");
    selected.provider = "codex".into();
    selected
        .attributes
        .insert("config_index".into(), "0".into());
    if let Some(raw) = raw {
        selected.metadata.insert("credential_options".into(), raw);
    }
    selected
}

fn options_cases() -> Vec<(Option<serde_json::Value>, Option<bool>)> {
    vec![
        (None, None),
        (Some(serde_json::json!({"weight":2})), None),
        (Some(serde_json::json!({"models":"invalid"})), None),
        (Some(serde_json::json!({"models":null})), Some(false)),
        (Some(serde_json::json!({"models":[]})), Some(false)),
        (
            Some(serde_json::json!({"models":[{"name":"other","is-compat":true}]})),
            Some(false),
        ),
        (
            Some(serde_json::json!({"models":[{"name":"upstream","is-compat":true}]})),
            Some(true),
        ),
        (
            Some(serde_json::json!({"models":[{"name":"upstream","is-compat":false}]})),
            Some(false),
        ),
    ]
}
