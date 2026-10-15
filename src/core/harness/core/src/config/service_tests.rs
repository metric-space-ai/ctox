use super::*;
use anyhow::Result;
use ctox_app_server_protocol::AppConfig;
use ctox_app_server_protocol::AppToolApproval;
use ctox_app_server_protocol::AppsConfig;
use ctox_app_server_protocol::AskForApproval;
use ctox_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use tempfile::tempdir;


#[tokio::test]
async fn write_value_preserves_comments_and_order() -> Result<()> {
    let tmp = tempdir().expect("tempdir");
    let original = r#"# Codex user configuration
model = "gpt-5"
approval_policy = "on-request"

[notice]
# Preserve this comment
hide_full_access_warning = true

[features]
unified_exec = true
"#;
    std::fs::write(tmp.path().join(CONFIG_TOML_FILE), original)?;

    let service = ConfigService::new_with_defaults(tmp.path().to_path_buf());
    service
        .write_value(ConfigValueWriteParams {
            file_path: Some(tmp.path().join(CONFIG_TOML_FILE).display().to_string()),
            key_path: "features.personality".to_string(),
            value: serde_json::json!(true),
            merge_strategy: MergeStrategy::Replace,
            expected_version: None,
        })
        .await
        .expect("write succeeds");

    let updated = std::fs::read_to_string(tmp.path().join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"# Codex user configuration
model = "gpt-5"
approval_policy = "on-request"

[notice]
# Preserve this comment
hide_full_access_warning = true

[features]
unified_exec = true
personality = true
"#;
    assert_eq!(updated, expected);
    Ok(())
}


#[tokio::test]
async fn read_includes_origins_and_layers() {
    let tmp = tempdir().expect("tempdir");
    let user_path = tmp.path().join(CONFIG_TOML_FILE);
    std::fs::write(&user_path, "model = \"user\"").unwrap();
    let user_file = AbsolutePathBuf::try_from(user_path.clone()).expect("user file");

    let managed_path = tmp.path().join("managed_config.toml");
    std::fs::write(&managed_path, "approval_policy = \"never\"").unwrap();
    let managed_file = AbsolutePathBuf::try_from(managed_path.clone()).expect("managed file");

    let service = ConfigService::new(
        tmp.path().to_path_buf(),
        vec![],
        LoaderOverrides {
            managed_config_path: Some(managed_path.clone()),
            #[cfg(target_os = "macos")]
            managed_preferences_base64: None,
            macos_managed_config_requirements_base64: None,
        },
        CloudRequirementsLoader::default(),
    );

    let response = service
        .read(ConfigReadParams {
            include_layers: true,
            cwd: None,
        })
        .await
        .expect("response");

    assert_eq!(response.config.approval_policy, Some(AskForApproval::Never));

    assert_eq!(
        response
            .origins
            .get("approval_policy")
            .expect("origin")
            .name,
        ConfigLayerSource::LegacyManagedConfigTomlFromFile {
            file: managed_file.clone()
        },
    );
    let layers = response.layers.expect("layers present");
    // Local macOS machines can surface an MDM-managed config layer at the
    // top of the stack; ignore it so this test stays focused on file/user/system ordering.
    let layers = if matches!(
        layers.first().map(|layer| &layer.name),
        Some(ConfigLayerSource::LegacyManagedConfigTomlFromMdm)
    ) {
        &layers[1..]
    } else {
        layers.as_slice()
    };
    assert_eq!(layers.len(), 3, "expected three layers");
    assert_eq!(
        layers.first().unwrap().name,
        ConfigLayerSource::LegacyManagedConfigTomlFromFile {
            file: managed_file.clone()
        }
    );
    assert_eq!(
        layers.get(1).unwrap().name,
        ConfigLayerSource::User {
            file: user_file.clone()
        }
    );
    assert!(matches!(
        layers.get(2).unwrap().name,
        ConfigLayerSource::System { .. }
    ));
}

#[tokio::test]
async fn write_value_reports_override() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join(CONFIG_TOML_FILE),
        "approval_policy = \"on-request\"",
    )
    .unwrap();

    let managed_path = tmp.path().join("managed_config.toml");
    std::fs::write(&managed_path, "approval_policy = \"never\"").unwrap();
    let managed_file = AbsolutePathBuf::try_from(managed_path.clone()).expect("managed file");

    let service = ConfigService::new(
        tmp.path().to_path_buf(),
        vec![],
        LoaderOverrides {
            managed_config_path: Some(managed_path.clone()),
            #[cfg(target_os = "macos")]
            managed_preferences_base64: None,
            macos_managed_config_requirements_base64: None,
        },
        CloudRequirementsLoader::default(),
    );

    let result = service
        .write_value(ConfigValueWriteParams {
            file_path: Some(tmp.path().join(CONFIG_TOML_FILE).display().to_string()),
            key_path: "approval_policy".to_string(),
            value: serde_json::json!("never"),
            merge_strategy: MergeStrategy::Replace,
            expected_version: None,
        })
        .await
        .expect("result");

    let read_after = service
        .read(ConfigReadParams {
            include_layers: true,
            cwd: None,
        })
        .await
        .expect("read");
    assert_eq!(
        read_after.config.approval_policy,
        Some(AskForApproval::Never)
    );
    assert_eq!(
        read_after
            .origins
            .get("approval_policy")
            .expect("origin")
            .name,
        ConfigLayerSource::LegacyManagedConfigTomlFromFile {
            file: managed_file.clone()
        }
    );
    assert_eq!(result.status, WriteStatus::Ok);
    assert!(result.overridden_metadata.is_none());
}

#[tokio::test]
async fn version_conflict_rejected() {
    let tmp = tempdir().expect("tempdir");
    let user_path = tmp.path().join(CONFIG_TOML_FILE);
    std::fs::write(&user_path, "model = \"user\"").unwrap();

    let service = ConfigService::new_with_defaults(tmp.path().to_path_buf());
    let error = service
        .write_value(ConfigValueWriteParams {
            file_path: Some(tmp.path().join(CONFIG_TOML_FILE).display().to_string()),
            key_path: "model".to_string(),
            value: serde_json::json!("gpt-5"),
            merge_strategy: MergeStrategy::Replace,
            expected_version: Some("sha256:bogus".to_string()),
        })
        .await
        .expect_err("should fail");

    assert_eq!(
        error.write_error_code(),
        Some(ConfigWriteErrorCode::ConfigVersionConflict)
    );
}

#[tokio::test]
async fn write_value_defaults_to_user_config_path() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(tmp.path().join(CONFIG_TOML_FILE), "").unwrap();

    let service = ConfigService::new_with_defaults(tmp.path().to_path_buf());
    service
        .write_value(ConfigValueWriteParams {
            file_path: None,
            key_path: "model".to_string(),
            value: serde_json::json!("gpt-new"),
            merge_strategy: MergeStrategy::Replace,
            expected_version: None,
        })
        .await
        .expect("write succeeds");

    let contents = std::fs::read_to_string(tmp.path().join(CONFIG_TOML_FILE)).expect("read config");
    assert!(
        contents.contains("model = \"gpt-new\""),
        "config.toml should be updated even when file_path is omitted"
    );
}


#[tokio::test]
async fn reserved_builtin_provider_override_rejected() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(tmp.path().join(CONFIG_TOML_FILE), "model = \"user\"\n").unwrap();

    let service = ConfigService::new_with_defaults(tmp.path().to_path_buf());
    let error = service
        .write_value(ConfigValueWriteParams {
            file_path: Some(tmp.path().join(CONFIG_TOML_FILE).display().to_string()),
            key_path: "model_providers.openai.name".to_string(),
            value: serde_json::json!("OpenAI Override"),
            merge_strategy: MergeStrategy::Replace,
            expected_version: None,
        })
        .await
        .expect_err("should reject reserved provider override");

    assert_eq!(
        error.write_error_code(),
        Some(ConfigWriteErrorCode::ConfigValidationError)
    );
    assert!(error.to_string().contains("reserved built-in provider IDs"));
    assert!(error.to_string().contains("`openai`"));

    let contents = std::fs::read_to_string(tmp.path().join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "model = \"user\"\n");
}





#[tokio::test]
async fn upsert_merges_tables_replace_overwrites() -> Result<()> {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join(CONFIG_TOML_FILE);
    let base = r#"[mcp_servers.linear]
bearer_token_env_var = "TOKEN"
name = "linear"
url = "https://linear.example"

[mcp_servers.linear.env_http_headers]
existing = "keep"

[mcp_servers.linear.http_headers]
alpha = "a"
"#;

    let overlay = serde_json::json!({
        "bearer_token_env_var": "NEW_TOKEN",
        "http_headers": {
            "alpha": "updated",
            "beta": "b"
        },
        "name": "linear",
        "url": "https://linear.example"
    });

    std::fs::write(&path, base)?;

    let service = ConfigService::new_with_defaults(tmp.path().to_path_buf());
    service
        .write_value(ConfigValueWriteParams {
            file_path: Some(path.display().to_string()),
            key_path: "mcp_servers.linear".to_string(),
            value: overlay.clone(),
            merge_strategy: MergeStrategy::Upsert,
            expected_version: None,
        })
        .await
        .expect("upsert succeeds");

    let upserted: TomlValue = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let expected_upsert: TomlValue = toml::from_str(
        r#"[mcp_servers.linear]
bearer_token_env_var = "NEW_TOKEN"
name = "linear"
url = "https://linear.example"

[mcp_servers.linear.env_http_headers]
existing = "keep"

[mcp_servers.linear.http_headers]
alpha = "updated"
beta = "b"
"#,
    )?;
    assert_eq!(upserted, expected_upsert);

    std::fs::write(&path, base)?;

    service
        .write_value(ConfigValueWriteParams {
            file_path: Some(path.display().to_string()),
            key_path: "mcp_servers.linear".to_string(),
            value: overlay,
            merge_strategy: MergeStrategy::Replace,
            expected_version: None,
        })
        .await
        .expect("replace succeeds");

    let replaced: TomlValue = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let expected_replace: TomlValue = toml::from_str(
        r#"[mcp_servers.linear]
bearer_token_env_var = "NEW_TOKEN"
name = "linear"
url = "https://linear.example"

[mcp_servers.linear.http_headers]
alpha = "updated"
beta = "b"
"#,
    )?;
    assert_eq!(replaced, expected_replace);

    Ok(())
}
