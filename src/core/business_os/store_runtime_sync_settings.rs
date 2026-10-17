pub fn runtime_settings_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let key = business_os_root_cache_key(root);
    let stamp = runtime_settings_cache_stamp(root);
    let cache = RUNTIME_SETTINGS_RXDB_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let previous_value = {
        let cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(&key).map(|entry| entry.value.clone())
    };
    {
        let cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.get(&key).filter(|entry| {
            entry.stamp == stamp
                && entry.generated_at.elapsed()
                    < Duration::from_secs(RUNTIME_SETTINGS_RXDB_CACHE_TTL_SECS)
        }) {
            return Ok(entry.value.clone());
        }
    }

    let value = stabilize_runtime_settings_timestamp(
        build_runtime_settings_for_rxdb(root)?,
        previous_value.as_ref(),
    );
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(
        key,
        RuntimeSettingsCacheEntry {
            generated_at: Instant::now(),
            stamp,
            value: value.clone(),
        },
    );
    Ok(value)
}

pub(crate) fn runtime_settings_projection_stamp(root: &Path) -> RuntimeSettingsProjectionStamp {
    RuntimeSettingsProjectionStamp {
        cache: runtime_settings_cache_stamp(root),
    }
}

fn stabilize_runtime_settings_timestamp(value: Value, previous: Option<&Value>) -> Value {
    let Some(previous) = previous else {
        return value;
    };
    if runtime_settings_without_timestamp(&value) != runtime_settings_without_timestamp(previous) {
        return value;
    }
    previous.clone()
}

fn runtime_settings_without_timestamp(value: &Value) -> Value {
    let mut value = value.clone();
    remove_runtime_settings_volatile_metadata(&mut value);
    value
}

fn remove_runtime_settings_volatile_metadata(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("updated_at_ms");
            object.remove("generated_at_ms");
            for value in object.values_mut() {
                remove_runtime_settings_volatile_metadata(value);
            }
        }
        Value::Array(items) => {
            for value in items {
                remove_runtime_settings_volatile_metadata(value);
            }
        }
        _ => {}
    }
}

fn build_runtime_settings_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .context("runtime settings configuration read failed")?;
    let runtime_state = crate::inference::runtime_state::load_runtime_state(root)
        .context("runtime settings state read failed")?;
    let configured_auth_mode = env_map
        .get("CTOX_OPENAI_AUTH_MODE")
        .or_else(|| env_map.get("OPENAI_AUTH_MODE"))
        .cloned()
        .unwrap_or_else(|| "api_key".to_owned());
    let mut provider = runtime_state
        .as_ref()
        .map(crate::inference::runtime_state::api_provider_for_runtime_state)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            crate::inference::runtime_state::infer_api_provider_from_env_map(&env_map)
        });
    let runtime_source_explicit =
        runtime_state.is_some() || env_map.contains_key("CTOX_CHAT_SOURCE");
    let provider_explicit = env_map.contains_key("CTOX_API_PROVIDER");
    let mut source = runtime_state
        .as_ref()
        .map(|state| state.source.as_env_value().to_owned())
        .unwrap_or_else(|| {
            env_map
                .get("CTOX_CHAT_SOURCE")
                .cloned()
                .unwrap_or_else(|| "local".to_owned())
        });
    let mut subscription_auth_probe = ChatgptSubscriptionAuthStatus::default();
    if provider.eq_ignore_ascii_case("local")
        && runtime_settings_auth_mode_is_subscription(&configured_auth_mode)
        && !runtime_source_explicit
        && !provider_explicit
    {
        subscription_auth_probe = chatgpt_subscription_auth_status(root);
        if subscription_auth_probe.configured {
            provider = "openai".to_owned();
            source = "api".to_owned();
        }
    }
    let runtime_provider = provider.clone();
    let subscription_provider = env_map
        .get(crate::inference::runtime_state::CTOX_SUBSCRIPTION_PROVIDER_ENV)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "codex" | "claude" | "antigravity" | "kimi"));
    let proxy_subscription_selected = runtime_provider.eq_ignore_ascii_case("ctox_subscription")
        && subscription_provider.is_some();
    if proxy_subscription_selected {
        if let Some(subscription_provider) = subscription_provider.as_deref() {
            provider = runtime_provider_for_subscription(subscription_provider).to_owned();
        }
    }
    let preset = runtime_settings_preset(runtime_state.as_ref(), &env_map);
    let context =
        runtime_settings_context(env_map.get("CTOX_CHAT_MODEL_MAX_CONTEXT").cloned().or_else(
            || {
                runtime_state.as_ref().and_then(|state| {
                    state
                        .configured_context_tokens
                        .map(|value| value.to_string())
                })
            },
        ));
    let upstream_base_url = runtime_state
        .as_ref()
        .filter(|state| !state.source.is_local())
        .map(|state| state.upstream_base_url.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!source.eq_ignore_ascii_case("local"))
                .then(|| runtime_settings_api_upstream_base_url(&runtime_provider, &env_map))
        })
        .unwrap_or_default();
    let key_name = crate::inference::runtime_state::api_key_env_var_for_provider_with_env_map(
        &runtime_provider,
        &env_map,
    );
    let key_configured =
        !proxy_subscription_selected && crate::secrets::get_credential(root, key_name).is_some();
    let available_models_by_provider = available_subscription_models_by_provider(root);
    let available_models = if proxy_subscription_selected {
        available_models_by_provider
            .get(&provider)
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else if provider.eq_ignore_ascii_case("ctox_proxy") {
        discover_ctox_proxy_models(
            &upstream_base_url,
            crate::secrets::get_credential(root, key_name).as_deref(),
        )
    } else {
        Vec::new()
    };
    let auth_mode = if provider.eq_ignore_ascii_case("local") {
        "local".to_owned()
    } else if proxy_subscription_selected {
        "subscription".to_owned()
    } else {
        configured_auth_mode
    };
    let service = cheap_ctox_service_status(root);
    let service_running = service
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let service_last_error = service
        .get("last_error")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    let legacy_openai_subscription = provider.eq_ignore_ascii_case("openai")
        && !proxy_subscription_selected
        && runtime_settings_auth_mode_is_subscription(&auth_mode);
    let subscription_selected = proxy_subscription_selected || legacy_openai_subscription;
    let mut subscription_auth = if legacy_openai_subscription {
        if subscription_auth_probe.configured {
            subscription_auth_probe
        } else {
            chatgpt_subscription_auth_status(root)
        }
    } else {
        ChatgptSubscriptionAuthStatus::default()
    };
    let provider_subscriptions = provider_subscription_status_projection(root);
    let subscription_account = subscription_provider.as_deref().and_then(|selected| {
        provider_subscriptions
            .get("accounts")
            .and_then(Value::as_array)
            .and_then(|accounts| {
                accounts.iter().find(|account| {
                    account.get("provider").and_then(Value::as_str) == Some(selected)
                        && account
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(true)
                        && account.get("status").and_then(Value::as_str) != Some("disabled")
                        && (account.get("ready").and_then(Value::as_bool) == Some(true)
                            || account.get("status").and_then(Value::as_str) == Some("ready")
                            || account.get("managed_by").and_then(Value::as_str)
                                == Some("ctox-auth"))
                })
            })
    });
    let proxy_subscription_configured =
        proxy_subscription_selected && subscription_account.is_some();
    if proxy_subscription_configured {
        subscription_auth.configured = true;
    }
    let auth_configured = provider.eq_ignore_ascii_case("local")
        || key_configured
        || (legacy_openai_subscription && subscription_auth.configured)
        || proxy_subscription_configured;
    let service_needs_attention = !service_running || !service_last_error.trim().is_empty();
    let auth_needs_attention = !auth_configured;
    let needs_attention = service_needs_attention || auth_needs_attention;
    let auth_message = runtime_auth_message(
        provider.as_str(),
        key_name,
        key_configured,
        subscription_selected,
        &subscription_auth,
    );
    let service_message = if service_needs_attention {
        if !service_running {
            "CTOX Service läuft nicht.".to_owned()
        } else {
            format!("CTOX kann Aufgaben nicht ausführen: {service_last_error}")
        }
    } else {
        "CTOX Service läuft.".to_owned()
    };
    let diagnostics_message = if needs_attention {
        if !service_running {
            "CTOX Service läuft nicht.".to_owned()
        } else if !service_last_error.trim().is_empty() {
            format!("CTOX kann Aufgaben nicht ausführen: {service_last_error}")
        } else {
            auth_message.clone()
        }
    } else {
        auth_message.clone()
    };
    let harness_flow = harness_flow_projection(root);
    let queue_health = harness_queue_health(root);
    let communication_channels = communication_channel_health(root);
    let web_stack = web_stack_projection(root, &env_map);
    let platform = crate::install::business_os_platform_status(root).unwrap_or_else(|error| {
        serde_json::json!({
            "ok": false,
            "version": "",
            "error": error.to_string(),
        })
    });
    let office = load_office_runtime_settings(root)?;
    let updated_at_ms = now_ms() as u64;
    Ok(serde_json::json!({
        "id": "runtime-settings",
        "ok": true,
        "can_manage": true,
        "updated_at_ms": updated_at_ms,
        "harness_flow": harness_flow,
        "queue_health": queue_health,
        "communication_channels": communication_channels,
        "web_stack": web_stack,
        "platform": platform,
        "office": office,
        "provider_subscriptions": provider_subscriptions,
        "runtime": {
            "source": source,
            "provider": provider,
            "runtime_provider": runtime_provider,
            "subscription_provider": subscription_provider,
            "chat_model": env_map.get("CTOX_CHAT_MODEL")
                .or_else(|| env_map.get("CTOX_CHAT_MODEL_BASE"))
                .cloned()
                .or_else(|| runtime_state.as_ref().and_then(|state| state.requested_model.clone()))
                .or_else(|| runtime_state.as_ref().and_then(|state| state.active_model.clone()))
                .unwrap_or_default(),
            "reasoning_effort": env_map.get("CTOX_CHAT_REASONING_EFFORT")
                .cloned()
                .unwrap_or_default(),
            "preset": preset,
            "context": context,
            "max_run_secs": env_map.get("CTOX_CHAT_TURN_TIMEOUT_SECS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1800),
            "upstream_base_url": upstream_base_url,
            "available_models": available_models,
            "available_models_by_provider": available_models_by_provider,
            "model_catalog_source": if proxy_subscription_selected {
                "subscription"
            } else if provider.eq_ignore_ascii_case("ctox_proxy") {
                "proxy"
            } else {
                "static"
            }
        },
        "auth": {
            "mode": auth_mode,
            "api_key_name": key_name,
            "api_key_configured": key_configured,
            "subscription_selected": subscription_selected,
            "subscription_session_configured": proxy_subscription_configured || subscription_auth.configured,
            "subscription_account_id": subscription_account.and_then(|account| account.get("id")).and_then(Value::as_str),
            "subscription_account_email": subscription_auth.account_email,
            "subscription_plan": subscription_auth.plan,
            "configured": auth_configured
        },
        "service": service,
        "diagnostics": {
            "needs_attention": needs_attention,
            "service_needs_attention": service_needs_attention,
            "auth_needs_attention": auth_needs_attention,
            "service_message": service_message,
            "auth_message": auth_message,
            "last_error": service_last_error,
            "message": diagnostics_message
        }
    }))
}

fn available_subscription_models_by_provider(root: &Path) -> BTreeMap<String, Vec<Value>> {
    let routes = crate::execution::cliproxyapi_host::instance_proxy_route_capabilities(root);
    let catalog = ctox_cliproxyapi::internal::registry::embedded_models_catalog().ok();
    let mut models_by_provider = BTreeMap::<String, Vec<Value>>::new();

    for route in routes {
        let provider = runtime_provider_for_subscription(&route.provider).to_owned();
        let reasoning_levels = catalog
            .as_ref()
            .and_then(|catalog| {
                ctox_cliproxyapi::internal::registry::models_for_channel(catalog, &route.provider)
            })
            .and_then(|models| {
                models
                    .into_iter()
                    .find(|model| model.id.eq_ignore_ascii_case(&route.model))
            })
            .and_then(|model| model.thinking)
            .map(|thinking| thinking.levels)
            .unwrap_or_default();
        models_by_provider
            .entry(provider)
            .or_default()
            .push(serde_json::json!({
                "id": route.model,
                "reasoning_levels": reasoning_levels,
                "default": route.default,
            }));
    }

    models_by_provider
}

fn ctox_proxy_models_url(base_url: &str) -> Option<String> {
    let mut url = Url::parse(base_url.trim()).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/');
    let models_path = if path.to_ascii_lowercase().ends_with("/api/fallback-llm")
        || path.to_ascii_lowercase().ends_with("/v1")
    {
        format!("{path}/models")
    } else {
        format!("{path}/v1/models")
    };
    url.set_path(&models_path);
    Some(url.to_string())
}

fn parse_ctox_proxy_model_catalog(payload: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return Vec::new();
    };
    let mut models = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    models.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    models
}

fn discover_ctox_proxy_models(base_url: &str, api_key: Option<&str>) -> Vec<String> {
    let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let Some(models_url) = ctox_proxy_models_url(base_url) else {
        return Vec::new();
    };
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(3))
        .timeout_write(Duration::from_secs(2))
        .build();
    let Ok(response) = agent
        .get(&models_url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call()
    else {
        return Vec::new();
    };
    response
        .into_string()
        .ok()
        .map(|payload| parse_ctox_proxy_model_catalog(&payload))
        .unwrap_or_default()
}

fn business_os_root_cache_key(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| {
        if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(root))
                .unwrap_or_else(|_| root.to_path_buf())
        }
    })
}

fn runtime_settings_cache_stamp(root: &Path) -> RuntimeSettingsCacheStamp {
    let runtime = root.join("runtime");
    RuntimeSettingsCacheStamp {
        runtime_config: business_os_sqlite_store_stamp(&runtime.join("ctox-runtime.sqlite3")),
        office_config: business_os_sqlite_store_stamp(&runtime.join("ctox-office.sqlite3")),
        secrets: business_os_sqlite_store_stamp(&runtime.join("ctox-secrets.sqlite3")),
        service: runtime_settings_service_stamp(&runtime.join("ctox_service.pid")),
        update_state: business_os_file_change_stamp(&runtime.join("update_state.json")),
    }
}

fn office_runtime_settings_path(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox-office.sqlite3")
}

fn normalize_office_engine(value: &str, kind: &str) -> anyhow::Result<&'static str> {
    // ctox_office/ctox-office are read-only migration aliases for settings
    // persisted before Documents and Spreadsheets became separate products.
    match (kind, value.trim().to_ascii_lowercase().as_str()) {
        ("document", "" | "ctox_documents" | "ctox-documents" | "ctox_office" | "ctox-office") => {
            Ok("ctox_documents")
        }
        (
            "spreadsheet",
            "" | "ctox_spreadsheets" | "ctox-spreadsheets" | "ctox_office" | "ctox-office",
        ) => Ok("ctox_spreadsheets"),
        ("spreadsheet", "legacy") => Ok("ctox_spreadsheets"),
        ("document", "legacy") => Ok("legacy"),
        (_, other) => anyhow::bail!("unsupported {kind} office engine: {other}"),
    }
}

fn open_office_runtime_settings(root: &Path) -> anyhow::Result<Connection> {
    let path = office_runtime_settings_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("open Office runtime settings {}", path.display()))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS office_runtime_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            documents_engine TEXT NOT NULL,
            spreadsheets_engine TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    )?;
    Ok(conn)
}

fn load_office_runtime_settings(root: &Path) -> anyhow::Result<Value> {
    let conn = open_office_runtime_settings(root)?;
    let value = conn
        .query_row(
            "SELECT documents_engine, spreadsheets_engine, updated_at_ms
             FROM office_runtime_settings WHERE id = 1",
            [],
            |row| {
                Ok(serde_json::json!({
                    "documents_engine": row.get::<_, String>(0)?,
                    "spreadsheets_engine": row.get::<_, String>(1)?,
                    "updated_at_ms": row.get::<_, i64>(2)?,
                    "document_available": true,
                    "spreadsheet_available": true,
                    "protocol": super::office_engine::EDITOR_PROTOCOL,
                    "protocol_version": super::office_engine::EDITOR_PROTOCOL_VERSION,
                }))
            },
        )
        .optional()?;
    if let Some(mut settings) = value {
        let documents_engine = normalize_office_engine(
            settings["documents_engine"].as_str().unwrap_or_default(),
            "document",
        )?;
        let spreadsheets_engine = normalize_office_engine(
            settings["spreadsheets_engine"].as_str().unwrap_or_default(),
            "spreadsheet",
        )?;
        if settings["documents_engine"].as_str() != Some(documents_engine)
            || settings["spreadsheets_engine"].as_str() != Some(spreadsheets_engine)
        {
            conn.execute(
                "UPDATE office_runtime_settings
                 SET documents_engine = ?1, spreadsheets_engine = ?2
                 WHERE id = 1",
                params![documents_engine, spreadsheets_engine],
            )?;
        }
        settings["documents_engine"] = Value::String(documents_engine.to_string());
        settings["spreadsheets_engine"] = Value::String(spreadsheets_engine.to_string());
        return Ok(settings);
    }
    Ok(serde_json::json!({
        "documents_engine": "ctox_documents",
        "spreadsheets_engine": "ctox_spreadsheets",
        "updated_at_ms": 0,
        "document_available": true,
        "spreadsheet_available": true,
        "protocol": super::office_engine::EDITOR_PROTOCOL,
        "protocol_version": super::office_engine::EDITOR_PROTOCOL_VERSION,
    }))
}

fn save_office_runtime_settings(
    root: &Path,
    request: OfficeRuntimeSettingsRequest,
) -> anyhow::Result<Value> {
    let documents_engine = normalize_office_engine(&request.documents_engine, "document")?;
    let spreadsheets_engine = normalize_office_engine(&request.spreadsheets_engine, "spreadsheet")?;
    let updated_at_ms = now_ms() as i64;
    let conn = open_office_runtime_settings(root)?;
    conn.execute(
        "INSERT INTO office_runtime_settings
            (id, documents_engine, spreadsheets_engine, updated_at_ms)
         VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
            documents_engine = excluded.documents_engine,
            spreadsheets_engine = excluded.spreadsheets_engine,
            updated_at_ms = excluded.updated_at_ms",
        params![documents_engine, spreadsheets_engine, updated_at_ms],
    )?;
    load_office_runtime_settings(root)
}

fn business_os_sqlite_store_stamp(path: &Path) -> BusinessOsSqliteStoreStamp {
    BusinessOsSqliteStoreStamp {
        db: business_os_file_change_stamp(path),
        wal: business_os_file_change_stamp(&sqlite_sidecar_path(path, "-wal")),
        shm: business_os_file_change_stamp(&sqlite_sidecar_path(path, "-shm")),
    }
}

fn runtime_settings_service_stamp(pid_path: &Path) -> RuntimeSettingsServiceStamp {
    let pid = std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    RuntimeSettingsServiceStamp {
        pid_file: business_os_file_change_stamp(pid_path),
        pid,
        running: pid.map(process_is_running).unwrap_or(false),
    }
}
