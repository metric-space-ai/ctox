// ref: sdk/cliproxy/auth/api_key_model_capabilities.go:194-268 @ d7914afd
// ref: sdk/cliproxy/auth/home_configuration_update_test.go @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::internal::modelconfig::ModelInfo;
use crate::sdk::cliproxy::executor::Request;

use super::api_key_model_capabilities::attach_resolved_api_key_model_info;
use super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;

fn oauth_auth(provider: &str, plan: &str) -> Auth {
    Auth {
        id: "selected-oauth".into(),
        provider: provider.into(),
        attributes: [
            ("auth_kind".into(), "oauth".into()),
            ("plan_type".into(), plan.into()),
        ]
        .into_iter()
        .collect(),
        ..Auth::default()
    }
}

#[test]
fn candidate_codex_oauth_plan_binds_only_its_catalog() {
    let snapshot = ApiKeyModelRoutingSnapshot::default();
    for (plan, model, expected) in [
        ("free", "gpt-6-luna(high)", Some(true)),
        ("free", "gpt-6-astra", None),
        ("free", "gpt-5.5", Some(false)),
        ("plus", " GPT-6-ASTRA(high) ", Some(true)),
        ("team", "gpt-6.1-sol", Some(true)),
        ("business", "gpt-6-sol", Some(true)),
        (" GO ", "gpt-6-astra", Some(true)),
        ("pro", "gpt-6-astra", Some(true)),
        ("", "gpt-6-astra", Some(true)),
        ("unknown-plan", "gpt-6-astra", Some(true)),
        ("plus", "unknown-model", None),
    ] {
        let selected = attach_resolved_api_key_model_info(
            &snapshot,
            Request::default(),
            &oauth_auth(" CoDeX ", plan),
            "public",
            model,
        );
        assert!(
            resolved_api_key_model_info(&selected).is_none(),
            "{plan}/{model}"
        );
        let info = resolved_model_info(&selected);
        assert_eq!(
            info.as_ref().map(|info| info.support_configuration_update),
            expected,
            "{plan}/{model}"
        );
        if let Some(info) = info {
            assert_eq!(info.provider_type, "openai");
            assert!(!info.is_compat && !info.user_defined);
            assert_eq!(
                info.native_capabilities.as_ref().unwrap().web_search,
                Some(true)
            );
        }
    }
}

#[test]
fn candidate_codex_oauth_rebind_clears_stale_api_and_oauth_capabilities() {
    let snapshot = ApiKeyModelRoutingSnapshot::default();
    let mut original = Request::default();
    original.metadata.resolved_api_key_model_info = Some(Arc::new(ModelInfo {
        id: "stale-api".into(),
        is_compat: true,
        ..ModelInfo::default()
    }));
    original.metadata.resolved_codex_oauth_model_info = Some(Arc::new(ModelInfo {
        id: "stale-oauth".into(),
        support_configuration_update: true,
        ..ModelInfo::default()
    }));
    original
        .metadata
        .extensions
        .insert("caller-value".into(), serde_json::json!("preserved"));
    let selected = attach_resolved_api_key_model_info(
        &snapshot,
        original.clone(),
        &oauth_auth("codex", "plus"),
        "public",
        "gpt-6-astra(high)",
    );
    assert!(resolved_api_key_model_info(&selected).is_none());
    assert_eq!(resolved_model_info(&selected).unwrap().id, "gpt-6-astra");
    assert_eq!(selected.metadata.extensions["caller-value"], "preserved");
    assert_eq!(
        original.metadata.resolved_api_key_model_info.unwrap().id,
        "stale-api"
    );
    assert_eq!(
        original
            .metadata
            .resolved_codex_oauth_model_info
            .unwrap()
            .id,
        "stale-oauth"
    );

    // A free account cannot inherit the preceding Plus account's model capability.
    let unlisted = attach_resolved_api_key_model_info(
        &snapshot,
        selected,
        &oauth_auth("codex", "free"),
        "public",
        "gpt-6-astra(high)",
    );
    assert!(resolved_model_info(&unlisted).is_none());
    assert!(unlisted.metadata.resolved_api_key_model_info.is_none());
    assert!(unlisted.metadata.resolved_codex_oauth_model_info.is_none());
    assert_eq!(unlisted.metadata.extensions["caller-value"], "preserved");
}

#[test]
fn candidate_codex_oauth_rejects_other_provider_and_api_key_fallback() {
    let snapshot = ApiKeyModelRoutingSnapshot::default();
    let mut other_provider = oauth_auth("claude", "plus");
    let mut api_key = oauth_auth("codex", "plus");
    api_key
        .attributes
        .insert("auth_kind".into(), "api_key".into());
    other_provider
        .metadata
        .insert("email".into(), serde_json::json!("test@example.invalid"));
    for account in [other_provider, api_key, Auth::default()] {
        let mut original = Request::default();
        original.metadata.resolved_codex_oauth_model_info = Some(Arc::new(ModelInfo {
            id: "gpt-6-astra".into(),
            support_configuration_update: true,
            ..ModelInfo::default()
        }));
        let selected = attach_resolved_api_key_model_info(
            &snapshot,
            original,
            &account,
            "public",
            "gpt-6-astra",
        );
        assert!(resolved_model_info(&selected).is_none());
        assert!(resolved_api_key_model_info(&selected).is_none());
    }
}

#[tokio::test]
async fn candidate_codex_oauth_home_execute_count_stream_preserve_legacy_update_capability() {
    for path in ["execute", "count", "stream"] {
        for (plan, model, explicit, expected) in [
            ("free", "gpt-6-luna", None, true),
            ("free", "gpt-5.5", None, false),
            ("plus", "gpt-6-astra", None, true),
            ("plus", "gpt-6-astra", Some(false), false),
            ("free", "gpt-6-astra", None, false),
        ] {
            let transport = TestHomeTransport::with_auth_ids(&[]);
            let executor = TestExecutor::failing(0);
            let (runtime, _) = runtime(transport.clone(), executor.clone());
            let mut model_info = serde_json::json!({"id":model, "context_length":12345});
            if let Some(explicit) = explicit {
                model_info["support_configuration_update"] = explicit.into();
            }
            let account = oauth_auth("codex", plan);
            transport.push_dispatch(serde_json::json!({
                "provider":"codex", "model":format!("{model}(high)"),
                "auth_index":account.id, "model_info":model_info, "auth":account
            }));
            if path == "stream" {
                let mut stream = runtime
                    .execute_home_stream(request("public"), "")
                    .await
                    .unwrap();
                while let Some(chunk) = stream.chunks.recv().await {
                    assert!(chunk.error.is_none());
                }
            } else {
                runtime
                    .execute_home(request("public"), "", path == "count")
                    .await
                    .unwrap();
            }
            let seen = executor.seen();
            assert_eq!(seen.len(), 1, "{path}/{plan}/{model}");
            let info = seen[0].resolved_model_info.as_ref().unwrap();
            assert_eq!(info.id, model);
            assert_eq!(info.context_length, 12345); // The central wire keeps authority.
            assert_eq!(
                info.support_configuration_update, expected,
                "{path}/{plan}/{model}"
            );
            assert!(!info.is_compat);
        }
    }
}

#[test]
fn candidate_codex_oauth_never_uses_same_id_api_key_routing_snapshot() {
    use super::api_key_model_capabilities_test::{manager, register};
    use crate::internal::config::{CodexKey, CodexModel, ProviderCompatConfig};
    let manager = manager();
    manager.set_provider_config(&ProviderCompatConfig {
        codex_api_key: vec![CodexKey {
            api_key: "collision-test-key".into(),
            models: vec![CodexModel {
                name: "gpt-6-astra".into(),
                alias: "public".into(),
                is_compat: true,
                support_configuration_update: false,
                ..CodexModel::default()
            }],
            ..CodexKey::default()
        }],
        ..ProviderCompatConfig::default()
    });
    let mut api = oauth_auth("codex", "plus");
    api.attributes.insert("auth_kind".into(), "api_key".into());
    api.attributes
        .insert("api_key".into(), "collision-test-key".into());
    api.attributes.insert("config_index".into(), "0".into());
    let api = register(&manager, api);
    let snapshot = manager.api_key_model_routing_snapshot();
    let (oauth_models, oauth_alias, _) =
        manager.execution_model_candidates_with_alias(&oauth_auth("codex", "plus"), "public");
    assert_eq!(oauth_models, ["public"]);
    assert_eq!(oauth_alias.upstream_model, "public");
    let previous = attach_resolved_api_key_model_info(
        &snapshot,
        Request::default(),
        &api,
        "public",
        "gpt-6-astra",
    );
    assert!(resolved_api_key_model_info(&previous).unwrap().is_compat);
    let selected = attach_resolved_api_key_model_info(
        &snapshot,
        previous,
        &oauth_auth("codex", "plus"),
        "public",
        "gpt-6-astra",
    );
    assert!(resolved_api_key_model_info(&selected).is_none());
    let info = resolved_model_info(&selected).unwrap();
    assert!(!info.is_compat);
    assert!(info.support_configuration_update);
}
