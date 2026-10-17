// ref: sdk/cliproxy/auth/home_v8_model_capabilities_test.go @ d7914afd
// ref: sdk/cliproxy/auth/home_configuration_update_test.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;

fn auth_with_options(raw: Option<&str>) -> Auth {
    let mut auth = Auth::default();
    auth.prefix = "tenant".to_owned();
    if let Some(raw) = raw {
        auth.metadata.insert(
            "credential_options".to_owned(),
            serde_json::from_str(raw).unwrap(),
        );
    }
    auth
}

fn dispatch(enabled: bool) -> serde_json::Value {
    serde_json::json!({
        "provider":"codex","model":"upstream(high)","auth_index":"home-model-account",
        "force_mapping":true,"original_alias":"tenant/alias(high)",
        "model_info":{"id":"upstream","context_length":32768,"thinking":{"levels":["high"]},
                      "native_capabilities":{"web_search":true},"support_configuration_update":false},
        "auth":{"id":"home-model-account","index":"home-model-account","provider":"codex","prefix":"tenant",
                "metadata":{"credential_options":{"models":[
                    {"name":"upstream","alias":"other","is-compat":!enabled},
                    {"name":"upstream","alias":"alias","is-compat":enabled}]}}}
    })
}
