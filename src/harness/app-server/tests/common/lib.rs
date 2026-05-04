#![expect(clippy::expect_used)]

use anyhow::Result;
use core_test_support::responses;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

pub async fn create_mock_responses_server_repeating_assistant(text: &str) -> MockServer {
    let server = responses::start_mock_server().await;
    let body = responses::sse(vec![
        responses::ev_assistant_message("msg_test", text),
        responses::ev_completed("resp_test"),
    ]);

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(body, "text/event-stream"),
        )
        .mount(&server)
        .await;

    server
}

pub fn write_mock_responses_config_toml(
    codex_home: &Path,
    server_uri: &str,
    model_overrides: &BTreeMap<String, toml::Value>,
    context_window: i64,
    include_reasoning: Option<bool>,
    provider_id: &str,
    model: &str,
) -> Result<()> {
    fs::create_dir_all(codex_home)?;

    let mut root = toml::map::Map::new();
    root.insert("model".to_string(), toml::Value::String(model.to_string()));
    root.insert(
        "model_provider".to_string(),
        toml::Value::String(provider_id.to_string()),
    );
    root.insert(
        "model_context_window".to_string(),
        toml::Value::Integer(context_window),
    );
    if matches!(include_reasoning, Some(false)) {
        root.insert(
            "model_reasoning_summary".to_string(),
            toml::Value::String("none".to_string()),
        );
    }

    let mut provider = toml::map::Map::new();
    provider.insert(
        "name".to_string(),
        toml::Value::String(provider_id.to_string()),
    );
    provider.insert(
        "base_url".to_string(),
        toml::Value::String(format!("{}/v1", server_uri.trim_end_matches('/'))),
    );
    provider.insert(
        "wire_api".to_string(),
        toml::Value::String("responses".to_string()),
    );

    let mut model_entry = toml::map::Map::new();
    model_entry.insert("name".to_string(), toml::Value::String(model.to_string()));
    model_entry.insert(
        "context_window".to_string(),
        toml::Value::Integer(context_window),
    );
    for (key, value) in model_overrides {
        model_entry.insert(key.clone(), value.clone());
    }

    let mut models = toml::map::Map::new();
    models.insert(model.to_string(), toml::Value::Table(model_entry));
    provider.insert("models".to_string(), toml::Value::Table(models));

    let mut providers = toml::map::Map::new();
    providers.insert(provider_id.to_string(), toml::Value::Table(provider));
    root.insert("model_providers".to_string(), toml::Value::Table(providers));

    fs::write(codex_home.join("config.toml"), toml::to_string(&root)?)?;
    Ok(())
}
