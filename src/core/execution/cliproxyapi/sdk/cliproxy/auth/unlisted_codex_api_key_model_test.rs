// ref: sdk/cliproxy/auth/api_key_model_capabilities.go:194-246 @ d7914afd
// ref: sdk/cliproxy/auth/conductor_models.go:732-775 @ d7914afd
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use crate::internal::config::{
    CodexKey, CodexModel, ProviderCompatConfig, VertexCompatKey, VertexCompatModel,
};
use crate::internal::modelconfig::ModelInfo;
use crate::internal::registry::RegistryThinkingSupport;
use crate::sdk::cliproxy::executor::Request;

use super::api_key_model_capabilities::attach_resolved_api_key_model_info;
use super::api_key_model_capabilities_test::{auth, manager, register};
use super::home_execution_paths_test::{request, runtime, TestExecutor, TestHomeTransport};
use super::*;

fn configured(support: bool, level: &str) -> ProviderCompatConfig {
    ProviderCompatConfig {
        codex_api_key: vec![CodexKey {
            api_key: "unlisted-test-key".into(),
            models: vec![CodexModel {
                name: "gpt-6-astra".into(),
                alias: "other-public".into(),
                is_compat: support,
                support_configuration_update: support,
                thinking: Some(RegistryThinkingSupport {
                    levels: vec![level.into()],
                    ..RegistryThinkingSupport::default()
                }),
                ..CodexModel::default()
            }],
            ..CodexKey::default()
        }],
        ..ProviderCompatConfig::default()
    }
}

fn account() -> Auth {
    let mut account = auth("unlisted", "unlisted-test-key");
    account.provider = "codex".into();
    account.attributes.insert("config_index".into(), "0".into());
    account
}

#[test]
fn candidate_unlisted_codex_selected_config_and_static_defaults() {
    let manager = manager();
    manager.set_provider_config(&configured(true, " MAX "));
    let account = register(&manager, account());
    let selected = manager.attach_resolved_api_key_model_info(
        Request::default(),
        &account,
        "not-a-configured-alias",
        "gpt-6-astra(high)",
    );
    let info = resolved_api_key_model_info(&selected).unwrap();
    assert_eq!(info.id, "gpt-6-astra(high)");
    assert_eq!(info.context_length, 272_000);
    assert!(info.is_compat && info.support_configuration_update);
    assert_eq!(info.thinking.as_ref().unwrap().levels, ["max"]);
    assert_eq!(
        info.native_capabilities.as_ref().unwrap().web_search,
        Some(true)
    );
    assert!(selected.metadata.resolved_codex_oauth_model_info.is_none());

    let mut empty = configured(false, "low");
    empty.codex_api_key[0].models.clear();
    manager.set_provider_config(&empty);
    let selected = manager.attach_resolved_api_key_model_info(
        Request::default(),
        &account,
        "direct",
        "gpt-6-astra",
    );
    let info = resolved_api_key_model_info(&selected).unwrap();
    assert_eq!(info.context_length, 272_000);
    assert!(!info.support_configuration_update && !info.is_compat);
    assert_eq!(
        info.native_capabilities.as_ref().unwrap().web_search,
        Some(true)
    );
}

#[test]
fn candidate_unlisted_codex_rejects_wrong_credentials_and_origin() {
    let manager = manager();
    let mut config = configured(true, "high");
    config.codex_api_key[0].models.clear();
    config.codex_api_key[0].base_url = "https://bound.invalid/v1".into();
    manager.set_provider_config(&config);
    let mut valid = account();
    valid
        .attributes
        .insert("base_url".into(), "https://bound.invalid/v1".into());
    let valid = register(&manager, valid);
    assert!(
        resolved_api_key_model_info(&manager.attach_resolved_api_key_model_info(
            Request::default(),
            &valid,
            "direct",
            "gpt-6-astra",
        ))
        .is_some()
    );
    for (field, value, model) in [
        ("api_key", "wrong-key", "gpt-6-astra"),
        ("base_url", "https://other.invalid/v1", "gpt-6-astra"),
        ("base_url", "", "gpt-6-astra"),
        ("provider", "claude", "gpt-6-astra"),
        ("auth_kind", "oauth", "gpt-6-astra"),
        ("api_key", "", "gpt-6-astra"),
        ("unchanged", "", ""),
    ] {
        let mut changed = valid.clone();
        changed.attributes.insert("plan_type".into(), "free".into());
        if field == "provider" {
            changed.provider = value.into();
        } else if field != "unchanged" {
            changed.attributes.insert(field.into(), value.into());
        }
        let mut stale = Request::default();
        stale.metadata.resolved_api_key_model_info = Some(Arc::new(ModelInfo {
            id: "stale".into(),
            support_configuration_update: true,
            ..ModelInfo::default()
        }));
        let rebound = manager.attach_resolved_api_key_model_info(stale, &changed, "direct", model);
        assert!(
            resolved_api_key_model_info(&rebound).is_none(),
            "{field}/{value}/{model}"
        );
        assert!(
            resolved_model_info(&rebound).is_none(),
            "{field}/{value}/{model}"
        );
    }
}

#[test]
fn candidate_unlisted_codex_keyless_origin_and_snapshot_redaction() {
    let manager = manager();
    manager.set_provider_config(&ProviderCompatConfig {
        codex_api_key: vec![
            CodexKey {
                base_url: "https://keyless.invalid/v1".into(),
                ..CodexKey::default()
            },
            CodexKey {
                api_key: "never-expose-config-secret".into(),
                base_url: "https://private.invalid/v1".into(),
                ..CodexKey::default()
            },
        ],
        ..ProviderCompatConfig::default()
    });
    let mut selected = account();
    selected.attributes.insert("api_key".into(), "".into());
    selected
        .attributes
        .insert("base_url".into(), " HTTPS://KEYLESS.INVALID/v1 ".into());
    selected = register(&manager, selected);
    let request = manager.attach_resolved_api_key_model_info(
        Request::default(),
        &selected,
        "direct",
        "custom-model",
    );
    let info = resolved_api_key_model_info(&request).unwrap();
    assert_eq!(info.id, "custom-model");
    assert!(!info.support_configuration_update);
    let debug = format!(
        "{:?} {:?}",
        manager.api_key_model_routing_snapshot(),
        request
    );
    assert!(!debug.contains("never-expose-config-secret"));
    assert!(!debug.contains("private.invalid"));
    assert!(request.metadata.extensions.is_empty());
}

#[test]
fn candidate_unlisted_codex_immutable_snapshot_survives_reload() {
    let manager = manager();
    manager.set_provider_config(&configured(true, "high"));
    let account = register(&manager, account());
    let old = manager.api_key_model_routing_snapshot();
    manager.set_provider_config(&configured(false, "low"));
    let previous = attach_resolved_api_key_model_info(
        &old,
        Request::default(),
        &account,
        "not-a-configured-alias",
        "gpt-6-astra(high)",
    );
    let latest = manager.attach_resolved_api_key_model_info(
        Request::default(),
        &account,
        "not-a-configured-alias",
        "gpt-6-astra(high)",
    );
    let previous = resolved_api_key_model_info(&previous).unwrap();
    let latest = resolved_api_key_model_info(&latest).unwrap();
    assert!(previous.is_compat && previous.support_configuration_update);
    assert_eq!(previous.thinking.as_ref().unwrap().levels, ["high"]);
    assert!(!latest.is_compat && !latest.support_configuration_update);
    assert_eq!(latest.thinking.as_ref().unwrap().levels, ["low"]);
}

#[test]
fn candidate_unlisted_codex_config_selection_checks_source_credentials_and_namespace() {
    let entry = |key: &str, base: &str, prefix: &str, proxy: &str, model: &str| CodexKey {
        api_key: key.into(),
        base_url: base.into(),
        prefix: prefix.into(),
        proxy_url: proxy.into(),
        models: vec![CodexModel {
            name: model.into(),
            alias: "public".into(),
            ..CodexModel::default()
        }],
        ..CodexKey::default()
    };
    let config = ProviderCompatConfig {
        codex_api_key: vec![
            entry(
                "key-one",
                "https://a.invalid",
                "north",
                "http://north.invalid",
                "first",
            ),
            entry(
                "key-two",
                "https://b.invalid",
                "south",
                "http://south.invalid",
                "second",
            ),
            entry(
                "key-one",
                "https://a.invalid",
                "wanted",
                "http://wanted.invalid",
                "preferred",
            ),
        ],
        ..ProviderCompatConfig::default()
    };
    for (source, index, key, base, prefix, proxy, expected) in [
        (
            "file",
            "0",
            "key-one",
            "https://a.invalid",
            "wanted",
            "http://wanted.invalid",
            "preferred",
        ),
        (
            "config:codex[0]",
            "0",
            "key-one",
            "https://a.invalid",
            "wanted",
            "http://wanted.invalid",
            "first",
        ),
        (
            "config:codex[1]",
            "1",
            "key-one",
            "https://a.invalid",
            "wanted",
            "http://wanted.invalid",
            "preferred",
        ),
        (
            "config:codex[99]",
            "99",
            "key-one",
            "https://a.invalid",
            "wanted",
            "http://wanted.invalid",
            "preferred",
        ),
        (
            "file",
            "0",
            " KEY-ONE ",
            " HTTPS://A.INVALID ",
            "WANTED",
            "HTTP://WANTED.INVALID",
            "preferred",
        ),
        (
            "file",
            "0",
            "key-one",
            "",
            "wanted",
            "http://wanted.invalid",
            "first",
        ),
        (
            "file",
            "0",
            "key-one",
            "https://missing.invalid",
            "wanted",
            "http://wanted.invalid",
            "first",
        ),
        (
            "file",
            "0",
            "key-two",
            "https://b.invalid",
            "",
            "",
            "second",
        ),
    ] {
        let manager = manager();
        manager.set_provider_config(&config);
        let mut selected = account();
        selected.prefix = prefix.into();
        selected.proxy_url = proxy.into();
        selected.attributes.extend([
            ("source".into(), source.into()),
            ("config_index".into(), index.into()),
            ("api_key".into(), key.into()),
            ("base_url".into(), base.into()),
        ]);
        let selected = register(&manager, selected);
        let (models, _, _) = manager.execution_model_candidates_with_alias(&selected, "public");
        assert_eq!(models, [expected], "{source}/{index}/{base}/{prefix}");
    }

    let manager = manager();
    manager.set_provider_config(&ProviderCompatConfig {
        vertex_api_key: vec![
            VertexCompatKey {
                api_key: "vertex-key".into(),
                base_url: "https://wrong.invalid".into(),
                models: vec![VertexCompatModel {
                    name: "vertex-wrong".into(),
                    alias: "public".into(),
                    ..VertexCompatModel::default()
                }],
                ..VertexCompatKey::default()
            },
            VertexCompatKey {
                api_key: "vertex-key".into(),
                base_url: "https://right.invalid".into(),
                models: vec![VertexCompatModel {
                    name: "vertex-right".into(),
                    alias: "public".into(),
                    ..VertexCompatModel::default()
                }],
                ..VertexCompatKey::default()
            },
        ],
        ..ProviderCompatConfig::default()
    });
    let mut selected = account();
    selected.provider = "vertex".into();
    selected
        .attributes
        .insert("api_key".into(), "vertex-key".into());
    selected
        .attributes
        .insert("base_url".into(), "https://right.invalid".into());
    let selected = register(&manager, selected);
    assert_eq!(
        manager
            .execution_model_candidates_with_alias(&selected, "public")
            .0,
        ["vertex-right"]
    );
}

#[tokio::test]
async fn candidate_unlisted_codex_legacy_home_execute_count_stream_reaches_selected_flag() {
    for path in ["execute", "count", "stream"] {
        for enabled in [false, true] {
            let transport = TestHomeTransport::with_auth_ids(&[]);
            let executor = TestExecutor::failing(0);
            let (runtime, _) = runtime(transport.clone(), executor.clone());
            let manager = runtime.manager();
            manager.set_provider_config(&configured(enabled, "high"));
            let selected = register(&manager, account());
            transport.push_dispatch(serde_json::json!({
                "provider":"codex", "model":"gpt-6-astra(high)", "auth_index":selected.index,
                "model_info":{"id":"gpt-6-astra", "context_length":12345}, "auth":selected
            }));
            if path == "stream" {
                let mut stream = runtime
                    .execute_home_stream(request("not-a-configured-alias"), "")
                    .await
                    .unwrap();
                while let Some(chunk) = stream.chunks.recv().await {
                    assert!(chunk.error.is_none());
                }
            } else {
                runtime
                    .execute_home(request("not-a-configured-alias"), "", path == "count")
                    .await
                    .unwrap();
            }
            let seen = executor.seen();
            assert_eq!(seen.len(), 1);
            let info = seen[0].resolved_model_info.as_ref().unwrap();
            assert_eq!(info.id, "gpt-6-astra");
            assert_eq!(info.context_length, 12345);
            assert_eq!(
                info.support_configuration_update, enabled,
                "{path}/{enabled}"
            );
            assert!(!info.is_compat); // Unconfigured central Home options stay authoritative.
        }
    }
}
