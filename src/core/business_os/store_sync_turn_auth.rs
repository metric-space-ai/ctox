pub fn rxdb_store_path(root: &Path) -> PathBuf {
    crate::paths::runtime_dir(root).join(RXDB_STORE_FILE)
}

pub fn status(root: &Path) -> anyhow::Result<BusinessOsStatus> {
    let path = crate::paths::runtime_dir(root).join(STORE_FILE);
    let ctox_service = Some(cheap_ctox_service_status(root));
    let sync_config = sync_config(root)?;
    Ok(BusinessOsStatus {
        ok: true,
        runtime: "native-rust",
        store_path: path.display().to_string(),
        now_ms: now_ms(),
        sync: serde_json::json!({
            "transport": sync_config.transport,
            "sync_mode": sync_config.sync_mode,
            "sync_room": sync_config.sync_room,
            "signaling_urls": sync_config.signaling_urls,
            "signaling_urls_source": sync_config.signaling_urls_source,
            "native_rxdb_peer_available": sync_config.native_rxdb_peer_available,
            "native_rxdb_peer_reason": sync_config.native_rxdb_peer_reason,
            "native_rxdb_peer_status": sync_config.native_rxdb_peer_status,
            "http_bridge_available": sync_config.http_bridge_available,
        }),
        module_catalog: rxdb_module_catalog_status(root),
        data_plane: rxdb_data_plane_status(root),
        ctox_service,
    })
}

fn rxdb_data_plane_status(root: &Path) -> Value {
    const CRITICAL_COLLECTIONS: &[(&str, bool)] = &[
        ("business_module_catalog", true),
        ("ctox_runtime_settings", true),
        ("desktop_files", false),
        ("desktop_file_chunks", false),
        ("business_commands", false),
        ("ctox_queue_tasks", false),
    ];

    let path = rxdb_store_path(root);
    if !path.is_file() {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "native RxDB SQLite store is missing",
            "collections": {},
        });
    }
    let conn = match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "reason": format!("open native RxDB SQLite store: {err}"),
                "collections": {},
            });
        }
    };
    let _ = conn.busy_timeout(Duration::from_millis(100));

    let mut collections = BTreeMap::new();
    let mut required_ok = true;
    for (collection, required_for_shell) in CRITICAL_COLLECTIONS {
        let table = rxdb_collection_table_name(&path, &conn, collection);
        let table_exists = table
            .as_deref()
            .map(|table| rxdb_table_exists_cached(&path, &conn, table).unwrap_or(false))
            .unwrap_or(false);
        let row_count = if table_exists && rxdb_collection_tracks_row_count(collection) {
            table
                .as_deref()
                .and_then(|table| rxdb_table_row_count(&conn, table).ok())
        } else {
            None
        };
        let latest_updated_at_ms =
            if table_exists && rxdb_collection_tracks_updated_at_ms(collection) {
                table
                    .as_deref()
                    .and_then(|table| rxdb_table_latest_updated_at_ms(&conn, table).ok())
                    .flatten()
            } else {
                None
            };
        let collection_ok =
            table_exists && (!required_for_shell || row_count.unwrap_or_default() > 0);
        if *required_for_shell && !collection_ok {
            required_ok = false;
        }
        collections.insert(
            (*collection).to_string(),
            serde_json::json!({
                "ok": collection_ok,
                "required_for_shell": required_for_shell,
                "table": table,
                "table_exists": table_exists,
                "row_count": row_count,
                "latest_updated_at_ms": latest_updated_at_ms,
            }),
        );
    }

    serde_json::json!({
        "ok": required_ok,
        "path": path.display().to_string(),
        "required_collections_ready": required_ok,
        "collections": collections,
    })
}

fn rxdb_table_exists_cached(path: &Path, conn: &Connection, table: &str) -> anyhow::Result<bool> {
    Ok(rxdb_table_names_cached(path, conn)?.contains(table))
}

fn rxdb_table_names_cached(path: &Path, conn: &Connection) -> anyhow::Result<BTreeSet<String>> {
    let key = rxdb_store_cache_key(path);
    let stamp = rxdb_store_stamp(path);
    let now = Instant::now();
    let cache = RXDB_TABLE_NAMES_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    {
        let cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.get(&key).filter(|entry| {
            entry.stamp == stamp
                && now.duration_since(entry.generated_at)
                    < Duration::from_secs(RXDB_TABLE_CACHE_TTL_SECS)
        }) {
            return Ok(entry.tables.clone());
        }
    }

    let tables = rxdb_table_names_uncached(conn)?;
    cache_rxdb_table_names(cache, key, stamp, now, tables)
}

fn rxdb_table_names_uncached(conn: &Connection) -> anyhow::Result<BTreeSet<String>> {
    let mut statement = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .context("list RxDB SQLite tables")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut tables = BTreeSet::new();
    for row in rows {
        tables.insert(row?);
    }
    Ok(tables)
}

fn cache_rxdb_table_names(
    cache: &Mutex<BTreeMap<PathBuf, RxdbTableNamesCacheEntry>>,
    key: PathBuf,
    stamp: RxdbStoreStamp,
    generated_at: Instant,
    tables: BTreeSet<String>,
) -> anyhow::Result<BTreeSet<String>> {
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= RXDB_TABLE_CACHE_MAX_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(
        key,
        RxdbTableNamesCacheEntry {
            generated_at,
            stamp,
            tables: tables.clone(),
        },
    );
    Ok(tables)
}

fn rxdb_store_cache_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn rxdb_store_stamp(path: &Path) -> RxdbStoreStamp {
    business_os_file_change_stamp(path)
}

fn business_os_file_change_stamp(path: &Path) -> BusinessOsFileChangeStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return (0, 0);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (metadata.len(), modified_at)
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn rxdb_table_row_count(conn: &Connection, table: &str) -> anyhow::Result<i64> {
    let quoted = sqlite_quote_identifier(table);
    conn.query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |row| {
        row.get::<_, i64>(0)
    })
    .with_context(|| format!("count rows in {table}"))
}

fn rxdb_collection_tracks_row_count(collection: &str) -> bool {
    !matches!(collection, "desktop_file_chunks")
}

fn rxdb_collection_tracks_updated_at_ms(collection: &str) -> bool {
    !matches!(collection, "desktop_file_chunks")
}

fn rxdb_table_latest_updated_at_ms(conn: &Connection, table: &str) -> anyhow::Result<Option<i64>> {
    let quoted = sqlite_quote_identifier(table);
    conn.query_row(
        &format!(
            "SELECT MAX(CAST(json_extract(data, '$.updated_at_ms') AS INTEGER)) FROM {quoted}"
        ),
        [],
        |row| row.get::<_, Option<i64>>(0),
    )
    .with_context(|| format!("read latest updated_at_ms in {table}"))
}

fn sqlite_quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(super) fn rxdb_collection_table_name(
    path: &Path,
    conn: &Connection,
    collection: &str,
) -> Option<String> {
    let expected = format!(
        "ctox_business_os__{collection}__v{}",
        rxdb_schema_version(collection)
    );
    let tables = rxdb_table_names_cached(path, conn).ok()?;
    rxdb_collection_table_name_from_tables(&tables, &expected, collection)
}

fn rxdb_collection_table_name_from_tables(
    tables: &BTreeSet<String>,
    expected: &str,
    collection: &str,
) -> Option<String> {
    if tables.contains(expected) {
        return Some(expected.to_string());
    }
    let prefix = format!("ctox_business_os__{collection}__v");
    tables
        .iter()
        .filter(|table| table.starts_with(&prefix))
        .cloned()
        .max_by_key(|table| {
            table
                .strip_prefix(&prefix)
                .and_then(|version| version.parse::<i64>().ok())
                .unwrap_or(-1)
        })
}

fn rxdb_schema_version(collection: &str) -> i64 {
    business_os_schema_contract_for_store()
        .get(collection)
        .and_then(|schema| schema.get("version"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn business_os_schema_contract_for_store() -> &'static BTreeMap<String, Value> {
    static CONTRACT: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("business_os_schema_contract.json"))
            .expect("Business OS RxDB schema contract JSON must be valid")
    })
}

pub(crate) fn cheap_ctox_service_status(root: &Path) -> Value {
    let pid = std::fs::read_to_string(root.join("runtime/ctox_service.pid"))
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let running = pid.map(process_is_running).unwrap_or(false);
    serde_json::json!({
        "running": running,
        "busy": null,
        "pid": pid,
        "listen_addr": "",
        "autostart_enabled": false,
        "manager": "process",
        "pending_count": null,
        "pending_previews": [],
        "blocked_count": null,
        "blocked_previews": [],
        "current_goal_preview": null,
        "active_source_label": null,
        "recent_events": [],
        "last_error": null,
        "last_completed_at": null,
        "last_reply_chars": null,
        "monitor_last_check_at": null,
        "monitor_alerts": [],
        "monitor_last_error": null
    })
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_running(_pid: u32) -> bool {
    false
}

/// Per-instance module allowlist for the Business OS shell.
///
/// Read from the persisted SQLite runtime store via `runtime_env::get_runtime_env_value`
/// (key `CTOX_BUSINESS_OS_MODULE_ALLOWLIST`). The value is a comma/whitespace
/// separated list of module ids. System modules are always added, so an old
/// tenant allowlist can never remove the Business OS base system. An empty/unset
/// value means "no additional restriction". This is intentionally not a
/// process-env toggle: operators set it in the runtime store.
pub fn business_os_module_allowlist(root: &Path) -> Vec<String> {
    let raw = match crate::inference::runtime_env::get_runtime_env_value(
        root,
        "CTOX_BUSINESS_OS_MODULE_ALLOWLIST",
    ) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut ids = Vec::new();
    for id in raw.split([',', ';', '\n', '\t', ' ']) {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if seen.insert(id.to_owned()) {
            ids.push(id.to_owned());
        }
    }
    for id in system_module_ids() {
        if seen.insert(id.clone()) {
            ids.push(id.clone());
        }
    }
    ids
}

// Compatibility-only state for older `ctox.module.set_visible` commands.
// Visibility is no longer an installation mechanism: static core manifests,
// runtime installed-modules/, and runtime local-modules/ are the three sources
// of truth.
const VISIBLE_MODULES_KEY: &str = "CTOX_BUSINESS_OS_VISIBLE_MODULES";

/// Legacy tab-visibility state retained only so in-flight old commands can be
/// completed idempotently during an upgrade.
fn business_os_visible_modules(root: &Path) -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    if let Some(raw) =
        crate::inference::runtime_env::get_runtime_env_value(root, VISIBLE_MODULES_KEY)
    {
        for id in raw.split([',', ';', '\n', '\t', ' ']) {
            let id = id.trim();
            if !id.is_empty() {
                set.insert(id.to_owned());
            }
        }
    }
    set
}

/// Add or remove a module from this instance's installed-tabs set.
fn business_os_set_module_visible(
    root: &Path,
    module_id: &str,
    visible: bool,
) -> anyhow::Result<()> {
    let mut set = business_os_visible_modules(root);
    if visible {
        set.insert(module_id.to_owned());
    } else {
        set.remove(module_id);
    }
    let joined = set.into_iter().collect::<Vec<_>>().join(",");
    crate::inference::runtime_env::set_runtime_env_value(root, VISIBLE_MODULES_KEY, &joined)?;
    Ok(())
}

/// Every module that reaches the installed catalog is visible. Static non-core
/// modules never reach this list; they stay in the marketplace until installed
/// into `installed-modules/`. Operator-owned `local-modules/` are installed by
/// directory presence. This makes filesystem placement the installation truth.
pub(super) fn augment_modules_with_instance_visibility(
    _root: &Path,
    _installed_app_root: &Path,
    modules: &mut [Value],
) {
    for module in modules.iter_mut() {
        if let Some(object) = module.as_object_mut() {
            object.insert("instance_visible".to_owned(), Value::Bool(true));
        }
    }
}

/// Last known upstream bundle hash for a github-sourced module, recorded by
/// `ctox.module.check_updates`. Kept in the runtime store (NOT in module.json,
/// which is part of the bundle hash) so it never makes the app look modified.
fn business_os_module_remote_sha(root: &Path, module_id: &str) -> Option<String> {
    let key = format!("CTOX_BUSINESS_OS_REMOTE_SHA__{module_id}");
    crate::inference::runtime_env::get_runtime_env_value(root, &key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn business_os_set_module_remote_sha(
    root: &Path,
    module_id: &str,
    sha: &str,
) -> anyhow::Result<()> {
    let key = format!("CTOX_BUSINESS_OS_REMOTE_SHA__{module_id}");
    crate::inference::runtime_env::set_runtime_env_value(root, &key, sha)?;
    Ok(())
}

pub(crate) fn sync_connection_config(
    root: &Path,
) -> anyhow::Result<BusinessOsSyncConnectionConfig> {
    let key = business_os_root_cache_key(root);
    let stamp = sync_connection_config_cache_stamp(root);
    let cache = SYNC_CONNECTION_CONFIG_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    {
        let cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.get(&key).filter(|entry| {
            entry.stamp == stamp
                && entry.generated_at.elapsed()
                    < Duration::from_secs(SYNC_CONNECTION_CONFIG_CACHE_TTL_SECS)
        }) {
            return Ok(entry.config.clone());
        }
    }

    let config = build_sync_connection_config(root)?;
    let stamp = sync_connection_config_cache_stamp(root);
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(
        key,
        SyncConnectionConfigCacheEntry {
            generated_at: Instant::now(),
            stamp,
            config: config.clone(),
        },
    );
    Ok(config)
}

fn build_sync_connection_config(root: &Path) -> anyhow::Result<BusinessOsSyncConnectionConfig> {
    let instance_id = stable_instance_id(root)?;
    let signaling_room_password = business_os_room_password(root)?;
    let signaling = signaling_urls_config(root);
    Ok(BusinessOsSyncConnectionConfig {
        sync_room: format!(
            "ctox-business-os:{instance_id}:{}",
            room_secret_id(&signaling_room_password)
        ),
        signaling_room_password,
        instance_id,
        signaling_urls: signaling.urls,
        signaling_urls_source: signaling.source,
    })
}

pub fn sync_config(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    let connection = sync_connection_config(root)?;
    let signaling_auth = signaling_auth_config(root, &connection.signaling_room_password)?;
    let peer_id = format!("ctox-core-{}", short_hash(&connection.instance_id));
    let native_rxdb_peer_available = super::rxdb_peer::is_native_peer_running_for_root(root);
    let native_rxdb_peer_status = super::rxdb_peer::native_peer_status(root);
    // This is an identity, not a process-session id. Keeping it stable across
    // supervised respawns lets browsers fail closed to one native endpoint;
    // the random peer_session_id remains available in native_rxdb_peer_status
    // for checkpoint/lifecycle invalidation.
    let native_peer_id = peer_id.clone();
    let ice_servers = ice_servers_config(root);
    let ice_diagnostics = ice_diagnostics(&ice_servers);
    Ok(BusinessOsSyncConfig {
        ok: true,
        app_hosting: "ctox_instance_webserver",
        sync_mode: "p2p-first",
        sync_room: connection.sync_room,
        signaling_room_password: connection.signaling_room_password,
        signaling_auth_version: signaling_auth.version,
        signaling_browser_token: signaling_auth.browser_token,
        signaling_browser_token_hash: signaling_auth.browser_token_hash,
        signaling_native_token_hash: signaling_auth.native_token_hash,
        instance_id: connection.instance_id,
        peer_id,
        native_peer_id,
        peer_role: "ctox_instance",
        signaling_urls: connection.signaling_urls,
        signaling_urls_source: connection.signaling_urls_source,
        ice_servers,
        ice_servers_refresh_url: "/api/business-os/sync/config",
        ice_diagnostics,
        transport: "webrtc",
        http_bridge_available: false,
        ctox_instance_required: true,
        native_rxdb_peer_available,
        native_rxdb_peer_reason: if native_rxdb_peer_available {
            ""
        } else {
            "CTOX native WebRTC peer is starting or unavailable"
        },
        native_rxdb_peer_status,
        module_allowlist: business_os_module_allowlist(root),
    })
}

/// Builds only the immutable connection material required by a short-lived
/// Workjet device invite. Unlike `sync_config`, this deliberately does not
/// inspect the running native peer or transport status: invite creation can
/// itself execute inside that peer's auxiliary request handler, where a
/// recursive liveness snapshot would contend with the response path.
pub(crate) fn mobile_invite_sync_config(
    root: &Path,
) -> anyhow::Result<BusinessOsMobileInviteSyncConfig> {
    let connection = sync_connection_config(root)?;
    let signaling_auth = signaling_auth_config(root, &connection.signaling_room_password)?;
    Ok(BusinessOsMobileInviteSyncConfig {
        native_peer_id: format!("ctox-core-{}", short_hash(&connection.instance_id)),
        instance_id: connection.instance_id,
        sync_room: connection.sync_room,
        signaling_urls: connection.signaling_urls,
        signaling_auth_version: signaling_auth.version,
        signaling_browser_token: signaling_auth.browser_token,
        signaling_browser_token_hash: signaling_auth.browser_token_hash,
        signaling_native_token_hash: signaling_auth.native_token_hash,
    })
}
pub(crate) const BUSINESS_OS_SIGNALING_AUTH_VERSION: &str = "ctox-role-bound-v1";

/// Safe inside an auxiliary handler: avoids the recursive peer liveness
/// snapshot in sync_config while preserving the source's configured ICE/TURN.
pub(super) fn native_transfer_ice_config(
    root: &Path,
    device_id: &str,
) -> (Vec<Value>, Option<i64>) {
    let mut servers = ice_servers_config(root);
    if let Some(server) = ephemeral_turn_server(root, device_id) {
        servers.push(server);
    }
    let expiry = ice_diagnostics(&servers)
        .get("credentialExpiresAtMs")
        .and_then(Value::as_i64);
    (servers, expiry)
}

pub(crate) fn signaling_auth_config(
    root: &Path,
    room_password: &str,
) -> anyhow::Result<BusinessOsSignalingAuthConfig> {
    let browser_token = signaling_token_from_room_password(room_password)
        .ok_or_else(|| anyhow::anyhow!("Business OS signaling room password is missing"))?;
    let native_token = business_os_native_signaling_token(root)?;
    Ok(BusinessOsSignalingAuthConfig {
        version: BUSINESS_OS_SIGNALING_AUTH_VERSION,
        browser_token_hash: signaling_token_hash(&browser_token),
        native_token_hash: signaling_token_hash(&native_token),
        browser_token,
        native_token,
    })
}

pub(crate) fn signaling_token_from_room_password(room_password: &str) -> Option<String> {
    let password = room_password.trim();
    if password.is_empty() {
        return None;
    }
    let digest = Sha256::digest(password.as_bytes());
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)[..32].to_string())
}

fn signaling_token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn business_os_native_signaling_token(root: &Path) -> anyhow::Result<String> {
    let _token_init_guard = native_signaling_token_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("native signaling token initialization lock poisoned"))?;

    read_or_create_native_signaling_token(root)
}

fn native_signaling_token_lock() -> &'static Mutex<()> {
    static TOKEN_INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    TOKEN_INIT_LOCK.get_or_init(|| Mutex::new(()))
}

fn read_or_create_native_signaling_token(root: &Path) -> anyhow::Result<String> {
    if let Ok(value) = crate::secrets::read_secret_value(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_SIGNALING_NATIVE_TOKEN_SECRET_NAME,
    ) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }

    let generated = generate_native_signaling_token()?;
    write_native_signaling_token(root, &generated, "business_os_sync_config")?;
    Ok(generated)
}

fn generate_native_signaling_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate native signaling token"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn write_native_signaling_token(root: &Path, token: &str, source: &str) -> anyhow::Result<()> {
    crate::secrets::write_secret_record(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_SIGNALING_NATIVE_TOKEN_SECRET_NAME,
        token,
        Some("Business OS native-only WebRTC signaling credential".to_owned()),
        serde_json::json!({"source": source, "auto_managed": true}),
    )?;
    Ok(())
}

pub fn sync_config_for_browser(
    root: &Path,
    turn_session_id: &str,
) -> anyhow::Result<BusinessOsSyncConfig> {
    let mut config = sync_config(root)?;
    // Role-bound signaling gives the browser its own credential. Never expose
    // the root room secret as a redundant credential in a browser payload.
    config.signaling_room_password.clear();
    if let Some(turn) = ephemeral_turn_server(root, turn_session_id) {
        config.ice_servers.push(turn);
    }
    config.ice_diagnostics = ice_diagnostics(&config.ice_servers);
    Ok(config)
}

pub fn rotate_sync_native_signaling_token(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    let rotation_guard = native_signaling_token_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("native signaling token rotation lock poisoned"))?;
    let generated = generate_native_signaling_token()?;
    write_native_signaling_token(
        root,
        &generated,
        "business_os_native_signaling_token_rotation",
    )?;
    // sync_config reads the token under this same non-reentrant lock.
    drop(rotation_guard);
    sync_config(root)
}

/// Rotate both signaling roles for incident response and pairing revocation.
pub fn rotate_sync_credentials(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    ensure_sync_room_password_rotation_allowed()?;
    rotate_sync_native_signaling_token(root)?;
    rotate_sync_room_password(root)
}

pub fn rotate_sync_room_password(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    ensure_sync_room_password_rotation_allowed()?;

    let generated = format!("ctox-room-{}", Uuid::new_v4().simple());
    crate::secrets::write_secret_record(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
        &generated,
        Some("Business OS WebRTC signaling room password".to_owned()),
        serde_json::json!({"source": "business_os_sync_config_rotation"}),
    )?;
    sync_config(root)
}

fn ensure_sync_room_password_rotation_allowed() -> anyhow::Result<()> {
    if env::var("CTOX_BUSINESS_OS_ROOM_PASSWORD")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "CTOX_BUSINESS_OS_ROOM_PASSWORD is set; unset the environment override before rotating the persisted Business OS room password"
        );
    }
    Ok(())
}

fn ice_servers_config(root: &Path) -> Vec<Value> {
    for key in [
        BUSINESS_OS_ICE_SERVERS_KEY,
        BUSINESS_OS_WEBRTC_ICE_SERVERS_KEY,
    ] {
        if let Some(servers) = configured_ice_servers(root, key) {
            return servers;
        }
    }
    if let Some(servers) = managed_turn_edge_ice_servers(root) {
        return servers;
    }
    vec![serde_json::json!({ "urls": DEFAULT_STUN_URL })]
}

fn configured_ice_servers(root: &Path, key: &str) -> Option<Vec<Value>> {
    if let Some(raw) = crate::inference::runtime_env::get_runtime_env_value(root, key) {
        return parse_ice_servers(&raw);
    }
    let raw = std::env::var(key).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let _ = crate::inference::runtime_env::set_runtime_env_value(root, key, trimmed);
    parse_ice_servers(trimmed)
}

fn parse_ice_servers(raw: &str) -> Option<Vec<Value>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) {
        let servers = items
            .into_iter()
            .filter_map(normalize_ice_server)
            .collect::<Vec<_>>();
        if !servers.is_empty() {
            return Some(servers);
        }
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(server) = normalize_ice_server(value) {
            return Some(vec![server]);
        }
    }
    let servers = trimmed
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| serde_json::json!({ "urls": url }))
        .collect::<Vec<_>>();
    (!servers.is_empty()).then_some(servers)
}

fn normalize_ice_server(value: Value) -> Option<Value> {
    let object = value.as_object()?;
    let urls = object.get("urls")?;
    let normalized_urls = if let Some(url) = urls.as_str() {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return None;
        }
        Value::String(trimmed.to_owned())
    } else if let Some(items) = urls.as_array() {
        let urls = items
            .iter()
            .filter_map(|item| item.as_str())
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| Value::String(url.to_owned()))
            .collect::<Vec<_>>();
        if urls.is_empty() {
            return None;
        }
        Value::Array(urls)
    } else {
        return None;
    };

    let mut server = serde_json::Map::new();
    server.insert("urls".to_owned(), normalized_urls);
    if let Some(username) = object
        .get("username")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        server.insert("username".to_owned(), Value::String(username.to_owned()));
    }
    if let Some(credential) = object
        .get("credential")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        server.insert(
            "credential".to_owned(),
            Value::String(credential.to_owned()),
        );
    }
    Some(Value::Object(server))
}

fn managed_turn_edge_url(root: &Path) -> String {
    if let Some(value) =
        crate::inference::runtime_env::get_runtime_env_value(root, BUSINESS_OS_TURN_EDGE_URL_KEY)
    {
        return value;
    }
    if let Ok(value) = env::var(BUSINESS_OS_TURN_EDGE_URL_KEY) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let _ = crate::inference::runtime_env::set_runtime_env_value(
                root,
                BUSINESS_OS_TURN_EDGE_URL_KEY,
                trimmed,
            );
            return trimmed.to_owned();
        }
    }
    DEFAULT_TURN_EDGE_URL.to_owned()
}

fn managed_turn_edge_key(root: &Path) -> Option<String> {
    if let Ok(value) = env::var(BUSINESS_OS_TURN_EDGE_KEY) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let _ = crate::secrets::set_credential(root, BUSINESS_OS_TURN_EDGE_KEY, trimmed);
            return Some(trimmed.to_owned());
        }
    }
    crate::secrets::get_credential(root, BUSINESS_OS_TURN_EDGE_KEY)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn managed_turn_edge_ice_servers(root: &Path) -> Option<Vec<Value>> {
    let edge_key = managed_turn_edge_key(root)?;
    let url = managed_turn_edge_url(root);
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build()
        .get(&url)
        .set("X-Edge-Key", &edge_key)
        .call()
        .ok()?;
    let payload = response.into_json::<Value>().ok()?;
    let ice_servers = payload
        .get("iceServers")
        .cloned()
        .or_else(|| {
            payload
                .get("result")
                .and_then(|value| value.get("iceServers"))
                .cloned()
        })
        .unwrap_or(payload);
    match ice_servers {
        Value::Array(items) => {
            let servers = items
                .into_iter()
                .filter_map(normalize_ice_server)
                .collect::<Vec<_>>();
            (!servers.is_empty()).then_some(servers)
        }
        value => normalize_ice_server(value).map(|server| vec![server]),
    }
}

fn ice_diagnostics(ice_servers: &[Value]) -> Value {
    let mut has_turn = false;
    let mut has_credentialed_turn = false;
    let mut credential_expires_at_ms: Option<i64> = None;
    for server in ice_servers {
        let Some(object) = server.as_object() else {
            continue;
        };
        let urls = object.get("urls");
        let url_values = if let Some(url) = urls.and_then(Value::as_str) {
            vec![url.to_owned()]
        } else {
            urls.and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let server_has_turn = url_values.iter().any(|url| {
            url.trim_start().starts_with("turn:") || url.trim_start().starts_with("turns:")
        });
        if !server_has_turn {
            continue;
        }
        has_turn = true;
        let username = object
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let credential = object
            .get("credential")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !username.is_empty() && !credential.is_empty() {
            has_credentialed_turn = true;
        }
        if let Some((expiry, _)) = username.split_once(':') {
            if let Ok(expiry_secs) = expiry.parse::<i64>() {
                let expiry_ms = expiry_secs.saturating_mul(1000);
                credential_expires_at_ms = Some(
                    credential_expires_at_ms
                        .map(|current| current.min(expiry_ms))
                        .unwrap_or(expiry_ms),
                );
            }
        }
    }
    serde_json::json!({
        "state": if has_credentialed_turn {
            "credentialed-turn"
        } else if has_turn {
            "turn-without-credentials"
        } else {
            "stun-or-host-only"
        },
        "iceServersConfigured": ice_servers.len(),
        "iceServersHaveTurn": has_turn,
        "iceServersHaveCredentialedTurn": has_credentialed_turn,
        "credentialExpiresAtMs": credential_expires_at_ms,
        "nativeRelaySupported": true,
        "nativeRelayStrategy": "browser-turn-plus-native-host-srflx",
        "warning": if has_credentialed_turn {
            ""
        } else {
            "No credentialed TURN is active; private/NAT browsers may need LAN-reachable native host or managed relay setup."
        },
    })
}

/// Optional TURN server URL (e.g. `turn:turn.example.com:3478`). TURN is opt-in:
/// when unset, sync uses STUN only and no ephemeral credentials are minted.
fn business_os_turn_url(root: &Path) -> Option<String> {
    if let Some(value) =
        crate::inference::runtime_env::get_runtime_env_value(root, BUSINESS_OS_TURN_URL_KEY)
    {
        return Some(value);
    }
    if let Ok(value) = env::var(BUSINESS_OS_TURN_URL_KEY) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let _ = crate::inference::runtime_env::set_runtime_env_value(
                root,
                BUSINESS_OS_TURN_URL_KEY,
                trimmed,
            );
            return Some(trimmed.to_owned());
        }
    }
    None
}

/// Shared secret used to derive coturn `use-auth-secret` ephemeral credentials.
/// Read-only (env override -> secret store); never auto-generated, because TURN
/// is opt-in and a secret without a matching coturn deployment is meaningless.
fn business_os_turn_secret(root: &Path) -> Option<String> {
    if let Ok(value) = env::var("CTOX_BUSINESS_OS_TURN_SECRET") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    let secret_name = crate::inference::runtime_env::get_runtime_env_value(
        root,
        BUSINESS_OS_TURN_SECRET_NAME_KEY,
    )
    .unwrap_or_else(|| BUSINESS_OS_TURN_SECRET_NAME.to_owned());
    crate::secrets::read_secret_value(root, BUSINESS_OS_SECRET_SCOPE, &secret_name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Mint a short-lived TURN credential pair using the coturn REST API scheme
/// (draft-uberti-behave-turn-rest): `username = "<expiry-unix>:<session>"` and
/// `password = base64(HMAC-SHA1(secret, username))`. The credential is valid
/// only until `expiry`, so a leaked credential cannot be replayed indefinitely.
/// HMAC-SHA1 is the credential-derivation scheme coturn implements; the secret,
/// not the hash, is the security boundary.
fn mint_ephemeral_turn_credentials(
    turn_secret: &str,
    session_id: &str,
    now_ms: i64,
) -> (String, String) {
    let expiry = now_ms / 1000 + BUSINESS_OS_TURN_TTL_SECS;
    let session = session_id.trim();
    let session = if session.is_empty() { "anon" } else { session };
    let username = format!("{expiry}:{session}");
    let key = ring::hmac::Key::new(
        ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        turn_secret.as_bytes(),
    );
    let signature = ring::hmac::sign(&key, username.as_bytes());
    let password = base64::engine::general_purpose::STANDARD.encode(signature.as_ref());
    (username, password)
}

/// Build an ICE server entry for the configured TURN server with freshly minted
/// ephemeral credentials bound to `session_id`. Returns `None` when TURN is not
/// configured (no URL or no secret) — callers then fall back to STUN only.
pub fn ephemeral_turn_server(root: &Path, session_id: &str) -> Option<Value> {
    let url = business_os_turn_url(root)?;
    let secret = business_os_turn_secret(root)?;
    let (username, credential) =
        mint_ephemeral_turn_credentials(&secret, session_id, now_ms() as i64);
    Some(serde_json::json!({
        "urls": url,
        "username": username,
        "credential": credential,
    }))
}

/// Backlog OS-B1 (architecture decision, see docs/ctox-turn.md): the relay is
/// NOT embedded in the daemon — TURN exists precisely for the case where the
/// CTOX box is not publicly reachable, so a relay on that box could not be
/// reached either. CTOX only mints ephemeral credentials for an external
/// coturn (co-located with the signaling plane). These two functions are the
/// operator surface behind `ctox business-os turn set/status`.
pub fn set_turn_config(
    root: &Path,
    url: Option<&str>,
    secret: Option<&str>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        url.is_some() || secret.is_some(),
        "nothing to set: pass --url and/or --secret"
    );
    if let Some(url) = url {
        let trimmed = url.trim();
        anyhow::ensure!(
            trimmed.starts_with("turn:") || trimmed.starts_with("turns:"),
            "TURN url must start with turn: or turns: (got `{trimmed}`)"
        );
        crate::inference::runtime_env::set_runtime_env_value(
            root,
            BUSINESS_OS_TURN_URL_KEY,
            trimmed,
        )?;
    }
    if let Some(secret) = secret {
        let trimmed = secret.trim();
        anyhow::ensure!(!trimmed.is_empty(), "TURN secret must not be empty");
        crate::secrets::write_secret_record(
            root,
            BUSINESS_OS_SECRET_SCOPE,
            BUSINESS_OS_TURN_SECRET_NAME,
            trimmed,
            Some("coturn use-auth-secret shared secret (ephemeral TURN credentials)".to_owned()),
            serde_json::json!({"source": "business-os-turn-cli"}),
        )?;
    }
    turn_config_status(root)
}

/// Redacted TURN configuration state: never returns the secret or a minted
/// credential — only whether the pieces are in place and what the browser
/// would receive structurally.
pub fn turn_config_status(root: &Path) -> anyhow::Result<Value> {
    let url = business_os_turn_url(root);
    let secret_configured = business_os_turn_secret(root).is_some();
    let active = url.is_some() && secret_configured;
    let managed_edge_key_configured = managed_turn_edge_key(root).is_some();
    let ice_servers = ice_servers_config(root);
    Ok(serde_json::json!({
        "url": url,
        "secret_configured": secret_configured,
        "active": active,
        "managed_edge_url": managed_turn_edge_url(root),
        "managed_edge_key_configured": managed_edge_key_configured,
        "ice_diagnostics": ice_diagnostics(&ice_servers),
        "credential_scheme": "coturn use-auth-secret (ephemeral, HMAC-SHA1 REST)",
        "credential_ttl_secs": BUSINESS_OS_TURN_TTL_SECS,
        "note": if active || managed_edge_key_configured {
            "TURN is served to peers via sync config ice_servers."
        } else {
            "TURN inactive: peers fall back to STUN only. Set coturn --url/--secret or configure CTOX_TURN_EDGE_KEY."
        },
    }))
}

fn business_os_room_password(root: &Path) -> anyhow::Result<String> {
    if let Ok(value) = env::var("CTOX_BUSINESS_OS_ROOM_PASSWORD") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    if let Ok(value) = crate::secrets::read_secret_value(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
    ) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    let generated = format!("ctox-room-{}", Uuid::new_v4().simple());
    crate::secrets::write_secret_record(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
        &generated,
        Some("Business OS WebRTC signaling room password".to_owned()),
        serde_json::json!({"source": "business_os_sync_config"}),
    )?;
    Ok(generated)
}

pub fn session(auth_header: Option<&str>, session_header: Option<&str>) -> BusinessOsSession {
    session_for_request(auth_header, session_header, true)
}

pub fn session_for_request(
    auth_header: Option<&str>,
    session_header: Option<&str>,
    allow_local_dev_session: bool,
) -> BusinessOsSession {
    let token = env::var("CTOX_BUSINESS_OS_SESSION_TOKEN").unwrap_or_default();
    let password = env::var("CTOX_BUSINESS_PASSWORD").unwrap_or_default();
    let expected_user = env::var("CTOX_BUSINESS_USER").unwrap_or_else(|_| "admin".to_owned());
    let configured_users = configured_auth_users();
    let require_explicit_login = env::var("CTOX_BUSINESS_OS_REQUIRE_LOGIN").as_deref() == Ok("1");
    let login_url = env::var("CTOX_BUSINESS_OS_LOGIN_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());

    if token.trim().is_empty() && password.trim().is_empty() && configured_users.is_empty() {
        if !require_explicit_login && allow_local_dev_session {
            return BusinessOsSession {
                ok: true,
                authenticated: true,
                auth_required: false,
                user: Some(BusinessOsSessionUser {
                    id: env::var("CTOX_BUSINESS_OS_DESKTOP_USER")
                        .unwrap_or_else(|_| "local-dev".to_owned()),
                    display_name: env::var("CTOX_BUSINESS_OS_DESKTOP_DISPLAY_NAME")
                        .unwrap_or_else(|_| "Local CTOX".to_owned()),
                    role: normalize_business_role(
                        &env::var("CTOX_BUSINESS_OS_DESKTOP_ROLE")
                            .unwrap_or_else(|_| "admin".to_owned()),
                    ),
                    is_admin: role_can_manage(
                        &env::var("CTOX_BUSINESS_OS_DESKTOP_ROLE")
                            .unwrap_or_else(|_| "admin".to_owned()),
                    ),
                }),
                login_url,
                reason: None,
            };
        }
        return BusinessOsSession {
            ok: true,
            authenticated: false,
            auth_required: true,
            user: None,
            login_url,
            reason: Some("ctox_session_token_not_configured".to_owned()),
        };
    }

    let expected = token.trim();
    let expected_password = password.trim();
    let basic = auth_header.and_then(parse_basic_credentials);
    let bearer = auth_header
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .unwrap_or("");
    let session_token = session_header.unwrap_or("").trim();
    let configured_user = basic
        .as_ref()
        .and_then(|(supplied_user, supplied_password)| {
            configured_users.iter().find(|user| {
                user.id.eq_ignore_ascii_case(supplied_user) && user.password == *supplied_password
            })
        });
    let token_authenticated = !expected.is_empty()
        && (bearer == expected
            || session_token == expected
            || basic
                .as_ref()
                .map(|(_, supplied_password)| supplied_password == expected)
                .unwrap_or(false));
    let password_authenticated = !expected_password.is_empty()
        && basic
            .as_ref()
            .map(|(supplied_user, supplied_password)| {
                supplied_user == expected_user.trim() && supplied_password == expected_password
            })
            .unwrap_or(false);
    let authenticated = token_authenticated || password_authenticated || configured_user.is_some();
    // SECURITY: derive the session identity from HOW authentication succeeded,
    // never from the unverified client-supplied Basic username. Only a matched
    // per-user credential (configured_user) or the password path (which pins
    // supplied_user == expected_user) yield a trusted username. The workspace-wide
    // shared token is NOT a per-user credential, so a token holder must not be
    // able to claim an arbitrary persisted user's id and inherit its role via
    // session_with_persisted_user — it binds to a fixed service principal whose
    // role comes from server config (default_session_role).
    let session_user_id: &str = if let Some(user) = configured_user {
        user.id.as_str()
    } else if password_authenticated {
        expected_user.trim()
    } else {
        "ctox-shared-token-session"
    };

    let role = configured_user
        .map(|user| user.role.clone())
        .unwrap_or_else(|| default_session_role());
    let is_admin = role_can_manage(&role);
    BusinessOsSession {
        ok: true,
        authenticated,
        auth_required: true,
        user: authenticated.then(|| BusinessOsSessionUser {
            id: session_user_id.to_owned(),
            display_name: session_user_id.to_owned(),
            role,
            is_admin,
        }),
        login_url,
        reason: (!authenticated).then(|| "invalid_or_missing_session".to_owned()),
    }
}

fn parse_basic_credentials(auth_header: &str) -> Option<(String, String)> {
    let encoded = auth_header.trim().strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let value = String::from_utf8(decoded).ok()?;
    let (user, password) = value.split_once(':')?;
    Some((user.to_owned(), password.to_owned()))
}

fn default_session_role() -> String {
    normalize_business_role(
        &env::var("CTOX_BUSINESS_OS_DEFAULT_ROLE").unwrap_or_else(|_| "user".to_owned()),
    )
}

#[derive(Debug, Clone)]
struct ConfiguredAuthUser {
    id: String,
    password: String,
    role: String,
}

fn configured_auth_users() -> Vec<ConfiguredAuthUser> {
    let Ok(raw) = env::var("CTOX_AUTH_USERS") else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|entry| {
            let separator = if entry.contains('|') { '|' } else { ':' };
            let parts = entry.split(separator).map(str::trim).collect::<Vec<_>>();
            let id = parts.first().copied().unwrap_or("");
            let password = parts.get(1).copied().unwrap_or("");
            if id.is_empty() || password.is_empty() {
                return None;
            }
            let role = parts
                .get(2)
                .and_then(|roles| {
                    roles
                        .split(',')
                        .map(str::trim)
                        .find(|role| !role.is_empty())
                })
                .map(normalize_business_role)
                .unwrap_or_else(|| "user".to_owned());
            Some(ConfiguredAuthUser {
                id: id.to_owned(),
                password: password.to_owned(),
                role,
            })
        })
        .collect()
}

fn default_configured_business_user() -> Option<ConfiguredAuthUser> {
    let password = env::var("CTOX_BUSINESS_PASSWORD").unwrap_or_default();
    let user = env::var("CTOX_BUSINESS_USER").unwrap_or_else(|_| "admin".to_owned());
    let id = user.trim();
    if id.is_empty() || password.trim().is_empty() {
        return None;
    }
    Some(ConfiguredAuthUser {
        id: id.to_owned(),
        password,
        role: default_session_role(),
    })
}

fn configured_business_users() -> Vec<ConfiguredAuthUser> {
    let mut users = Vec::new();
    if let Some(default_user) = default_configured_business_user() {
        users.push(default_user);
    }
    for configured in configured_auth_users() {
        if users
            .iter()
            .any(|existing| existing.id.eq_ignore_ascii_case(&configured.id))
        {
            continue;
        }
        users.push(configured);
    }
    users
}

pub(super) fn seed_configured_business_users(conn: &Connection) -> anyhow::Result<()> {
    let now = now_ms() as i64;
    for user in configured_business_users() {
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES (?1, ?1, ?2, 1, ?3, ?3)
             ON CONFLICT(user_id) DO NOTHING",
            params![user.id.trim(), normalize_business_role(&user.role), now],
        )?;
    }
    Ok(())
}

fn role_can_manage(role: &str) -> bool {
    policy::role_can_manage(role)
}

pub fn trusted_mcp_actor(
    root: &Path,
    actor_id: &str,
    actor_display_name: &str,
) -> anyhow::Result<BusinessOsTrustedActor> {
    let conn = open_store(root)?;
    trusted_mcp_actor_with_conn(&conn, actor_id, actor_display_name)
}

pub(super) fn trusted_mcp_actor_with_conn(
    conn: &Connection,
    actor_id: &str,
    actor_display_name: &str,
) -> anyhow::Result<BusinessOsTrustedActor> {
    let actor_id = actor_id.trim();
    seed_configured_business_users(conn)?;
    if let Some(user) = active_business_user(conn, actor_id)? {
        return Ok(BusinessOsTrustedActor {
            id: user.id,
            display_name: user.display_name,
            role: user.role,
            active: user.active,
            persisted: true,
        });
    }
    let id = if actor_id.is_empty() {
        "mcp:local".to_owned()
    } else {
        actor_id.to_owned()
    };
    Ok(BusinessOsTrustedActor {
        id: id.clone(),
        display_name: if actor_display_name.trim().is_empty() {
            id
        } else {
            actor_display_name.trim().to_owned()
        },
        role: "user".to_owned(),
        active: true,
        persisted: false,
    })
}

pub fn session_can_manage_all(session: &BusinessOsSession) -> bool {
    let actor = policy_actor_from_session(session);
    policy::evaluate(
        &actor,
        BusinessOsPermission::UsersManage,
        &BusinessOsScope::workspace(),
    )
    .allowed
}

fn allowed_permissions_for_projection(
    actor: &BusinessOsActor,
    scope: &BusinessOsScope,
) -> Vec<Value> {
    BusinessOsPermission::all()
        .iter()
        .copied()
        .filter(|permission| policy::evaluate(actor, *permission, scope).allowed)
        .map(|permission| Value::String(permission.as_str().to_owned()))
        .collect()
}

fn default_permission_projection() -> Value {
    let roles = ["chef", "admin", "founder", "user"];
    let mut role_defaults = serde_json::Map::new();
    for role in roles {
        let actor = BusinessOsActor::new(None, role);
        role_defaults.insert(
            role.to_owned(),
            serde_json::json!({
                "workspace": allowed_permissions_for_projection(
                    &actor,
                    &BusinessOsScope::workspace()
                ),
                "module": allowed_permissions_for_projection(
                    &actor,
                    &BusinessOsScope::module("__module__", false)
                ),
                "assigned_module": allowed_permissions_for_projection(
                    &actor,
                    &BusinessOsScope::module("__module__", true)
                ),
                "owned_task": allowed_permissions_for_projection(
                    &actor,
                    &BusinessOsScope::task("__task__", true, false)
                )
            }),
        );
    }
    Value::Object(role_defaults)
}

fn founder_assignment_permission_projection(founders: &HashMap<String, Vec<Value>>) -> Value {
    let mut modules = serde_json::Map::new();
    for (module_id, assignments) in founders {
        let mut users = serde_json::Map::new();
        for assignment in assignments {
            if assignment.get("active").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            if assignment.get("user_active").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let Some(user_id) = assignment.get("user_id").and_then(Value::as_str) else {
                continue;
            };
            let actor = BusinessOsActor::new(Some(user_id.to_owned()), "founder");
            users.insert(
                user_id.to_owned(),
                Value::Array(allowed_permissions_for_projection(
                    &actor,
                    &BusinessOsScope::module(module_id.as_str(), true),
                )),
            );
        }
        if !users.is_empty() {
            modules.insert(module_id.clone(), Value::Object(users));
        }
    }
    Value::Object(modules)
}

fn explicit_permission_grants_projection(conn: &Connection) -> anyhow::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT grant_id, subject_type, subject_id, permission, scope_type,
                scope_id, active, reason, created_by, created_at_ms, updated_at_ms
         FROM business_permission_grants
         WHERE active = 1
           AND grant_id NOT LIKE 'migration.sync.%'
         ORDER BY scope_type ASC, scope_id ASC, permission ASC, subject_type ASC, subject_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "grant_id": row.get::<_, String>(0)?,
            "subject_type": row.get::<_, String>(1)?,
            "subject_id": row.get::<_, String>(2)?,
            "permission": row.get::<_, String>(3)?,
            "scope_type": row.get::<_, String>(4)?,
            "scope_id": row.get::<_, String>(5)?,
            "active": row.get::<_, i64>(6)? != 0,
            "reason": row.get::<_, String>(7)?,
            "created_by": row.get::<_, String>(8)?,
            "created_at_ms": row.get::<_, i64>(9)?,
            "updated_at_ms": row.get::<_, i64>(10)?,
        }))
    })?;
    let grants = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(grants))
}

fn permission_model_projection(
    conn: &Connection,
    founders: &HashMap<String, Vec<Value>>,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "version": 1,
        "source": "business_permission_grants+business_module_acl",
        "deny_supported": false,
        "permissions": BusinessOsPermission::all()
            .iter()
            .map(|permission| Value::String(permission.as_str().to_owned()))
            .collect::<Vec<_>>(),
        "role_defaults": default_permission_projection(),
        "module_assignments": founder_assignment_permission_projection(founders),
        "explicit_grants": explicit_permission_grants_projection(conn)?,
    }))
}

#[derive(Debug, Default)]
struct ModuleLifecycleProjectionContext {
    responsible_user_ids: BTreeMap<String, Vec<String>>,
    preview_grant_ids: BTreeMap<String, Vec<String>>,
    preview_user_ids: BTreeMap<String, Vec<String>>,
    creator_user_ids: BTreeMap<String, String>,
    latest_releases: BTreeMap<String, Value>,
    release_history: BTreeMap<String, Vec<Value>>,
}

fn module_lifecycle_projection_context(
    root: &Path,
) -> anyhow::Result<ModuleLifecycleProjectionContext> {
    let conn = open_store(root)?;
    let mut context = ModuleLifecycleProjectionContext::default();

    let mut founder_stmt = conn.prepare(
        "SELECT acl.module_id, acl.user_id
         FROM business_module_acl acl
         LEFT JOIN business_users user ON user.user_id = acl.user_id
         WHERE acl.role = 'founder'
           AND acl.active = 1
           AND COALESCE(user.active, 1) = 1
         ORDER BY acl.module_id ASC, acl.user_id ASC",
    )?;
    let founder_rows = founder_stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in founder_rows {
        let (module_id, user_id) = row?;
        context
            .responsible_user_ids
            .entry(module_id)
            .or_default()
            .push(user_id);
    }
    drop(founder_stmt);

    let mut grant_stmt = conn.prepare(
        "SELECT scope_id, grant_id, subject_type, subject_id
         FROM business_permission_grants
         WHERE active = 1
           AND scope_type = 'module'
           AND permission = ?1
         ORDER BY scope_id ASC, grant_id ASC",
    )?;
    let grant_rows =
        grant_stmt.query_map(params![BusinessOsPermission::AppsView.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
    for row in grant_rows {
        let (module_id, grant_id, subject_type, subject_id) = row?;
        context
            .preview_grant_ids
            .entry(module_id.clone())
            .or_default()
            .push(grant_id);
        if subject_type == "user" && !subject_id.trim().is_empty() {
            context
                .preview_user_ids
                .entry(module_id)
                .or_default()
                .push(subject_id);
        }
    }
    drop(grant_stmt);

    let mut version_stmt = conn.prepare(
        "SELECT module_id, created_by
         FROM business_module_versions
         WHERE TRIM(created_by) <> ''
         ORDER BY module_id ASC, seq ASC",
    )?;
    let version_rows = version_stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in version_rows {
        let (module_id, created_by) = row?;
        context
            .creator_user_ids
            .entry(module_id)
            .or_insert(created_by);
    }
    drop(version_stmt);

    let mut release_stmt = conn.prepare(
        "SELECT module_id, version_id, version, status, created_by, created_at_ms, notes, snapshot_json
         FROM business_module_releases
         ORDER BY module_id ASC, version DESC",
    )?;
    let release_rows = release_stmt.query_map([], |row| {
        let snapshot_json = row.get::<_, String>(7)?;
        let snapshot = serde_json::from_str::<Value>(&snapshot_json).unwrap_or(Value::Null);
        Ok((
            row.get::<_, String>(0)?,
            serde_json::json!({
                "version_id": row.get::<_, String>(1)?,
                "version": row.get::<_, i64>(2)?,
                "status": row.get::<_, String>(3)?,
                "created_by": row.get::<_, String>(4)?,
                "created_at_ms": row.get::<_, i64>(5)?,
                "notes": row.get::<_, String>(6)?,
                "snapshot": snapshot,
            }),
        ))
    })?;
    for row in release_rows {
        let (module_id, release) = row?;
        if let Some(created_by) = release.get("created_by").and_then(Value::as_str) {
            if !created_by.trim().is_empty() {
                context
                    .creator_user_ids
                    .entry(module_id.clone())
                    .or_insert_with(|| created_by.to_owned());
            }
        }
        if release.get("status").and_then(Value::as_str) == Some("released") {
            context
                .latest_releases
                .entry(module_id.clone())
                .or_insert_with(|| release.clone());
        }
        context
            .release_history
            .entry(module_id)
            .or_default()
            .push(release);
    }

    Ok(context)
}

pub(super) fn parse_business_app_semver_major(version: &str) -> Option<u64> {
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some()
        || [major, minor, patch]
            .iter()
            .any(|part| !is_plain_semver_number(part))
    {
        return None;
    }
    major.parse::<u64>().ok()
}

fn is_plain_semver_number(part: &str) -> bool {
    !part.is_empty()
        && part.chars().all(|ch| ch.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'))
}

pub(super) fn module_manifest_collection_ids(manifest: &Value) -> Vec<String> {
    let mut collections = manifest
        .get("collections")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    collections.sort();
    collections.dedup();
    collections
}

pub(super) fn augment_module_manifest_file_plane(manifest: &mut ModuleManifest, module_dir: &Path) {
    let declared = manifest
        .collections
        .iter()
        .map(|collection| collection.trim())
        .filter(|collection| !collection.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    if declared.is_empty() {
        return;
    }
    let schema_path = module_dir.join("collections.schema.json");
    let Ok(schema_text) = fs::read_to_string(&schema_path) else {
        return;
    };
    let Ok(schema_doc) = serde_json::from_str::<Value>(&schema_text) else {
        return;
    };
    let declarations = business_os_file_plane_declarations_from_schema_doc(&schema_doc, &declared);
    if declarations.is_empty() {
        return;
    }
    manifest.file_plane = serde_json::json!({
        "schema_format": "ctox-business-os-file-plane-v1",
        "declarations": declarations,
    });
}

pub(crate) fn business_os_file_plane_declarations_from_schema_doc(
    schema_doc: &Value,
    declared_collections: &BTreeSet<String>,
) -> Vec<Value> {
    let mut declarations = Vec::new();
    for key in ["file_plane", "filePlane"] {
        if let Some(value) = schema_doc.get(key) {
            collect_business_os_file_plane_declarations(
                value,
                None,
                declared_collections,
                &mut declarations,
            );
        }
    }
    for key in ["collection_roles", "collectionRoles"] {
        if let Some(roles) = schema_doc.get(key).and_then(Value::as_object) {
            for (collection, value) in roles {
                collect_business_os_file_plane_declarations(
                    value,
                    Some(collection),
                    declared_collections,
                    &mut declarations,
                );
            }
        }
    }
    if let Some(collections) = schema_doc.get("collections").and_then(Value::as_object) {
        for (collection, value) in collections {
            if value.get("role").is_some() || value.get("file_plane").is_some() {
                collect_business_os_file_plane_declarations(
                    value,
                    Some(collection),
                    declared_collections,
                    &mut declarations,
                );
            }
        }
    }
    let mut by_request = BTreeMap::new();
    for declaration in declarations {
        if let Some(request_collection) = declaration
            .get("request_collection")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        {
            by_request.insert(request_collection, declaration);
        }
    }
    by_request.into_values().collect()
}

fn collect_business_os_file_plane_declarations(
    value: &Value,
    default_storage_collection: Option<&String>,
    declared_collections: &BTreeSet<String>,
    output: &mut Vec<Value>,
) {
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(declaration) = normalize_business_os_file_plane_declaration(
                item,
                default_storage_collection,
                declared_collections,
            ) {
                output.push(declaration);
            }
        }
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    for key in [
        "declarations",
        "demand_file_sources",
        "demandFileSources",
        "file_chunks",
        "fileChunks",
    ] {
        if let Some(items) = object.get(key).and_then(Value::as_array) {
            for item in items {
                if let Some(declaration) = normalize_business_os_file_plane_declaration(
                    item,
                    default_storage_collection,
                    declared_collections,
                ) {
                    output.push(declaration);
                }
            }
        }
    }
    for key in ["collection_roles", "collectionRoles"] {
        if let Some(roles) = object.get(key).and_then(Value::as_object) {
            for (collection, role) in roles {
                collect_business_os_file_plane_declarations(
                    role,
                    Some(collection),
                    declared_collections,
                    output,
                );
            }
        }
    }
    if object.contains_key("role")
        || object.contains_key("request_collection")
        || object.contains_key("requestCollection")
        || object.contains_key("storage_collection")
        || object.contains_key("storageCollection")
    {
        if let Some(declaration) = normalize_business_os_file_plane_declaration(
            value,
            default_storage_collection,
            declared_collections,
        ) {
            output.push(declaration);
        }
    }
}

fn normalize_business_os_file_plane_declaration(
    value: &Value,
    default_storage_collection: Option<&String>,
    declared_collections: &BTreeSet<String>,
) -> Option<Value> {
    let role = business_os_file_plane_string(value, &["role"]).unwrap_or_default();
    if !role.is_empty() && role != "file-chunks" {
        return None;
    }
    let storage_collection = business_os_file_plane_string(
        value,
        &["storage_collection", "storageCollection", "collection"],
    )
    .or_else(|| default_storage_collection.cloned())?;
    let request_collection =
        business_os_file_plane_string(value, &["request_collection", "requestCollection"])
            .unwrap_or_else(|| storage_collection.clone());
    let key_field = business_os_file_plane_string(value, &["key_field", "keyField"])?;
    let content_hash_field =
        business_os_file_plane_string(value, &["content_hash_field", "contentHashField"])
            .unwrap_or_else(|| "content_hash".to_owned());
    let chunk_index_field =
        business_os_file_plane_string(value, &["chunk_index_field", "chunkIndexField"])
            .unwrap_or_else(|| "idx".to_owned());

    for identifier in [
        request_collection.as_str(),
        storage_collection.as_str(),
        key_field.as_str(),
        content_hash_field.as_str(),
        chunk_index_field.as_str(),
    ] {
        if !business_os_file_plane_identifier_is_valid(identifier) {
            return None;
        }
    }
    if !declared_collections.is_empty()
        && (!declared_collections.contains(&request_collection)
            || !declared_collections.contains(&storage_collection))
    {
        return None;
    }
    Some(serde_json::json!({
        "role": "file-chunks",
        "request_collection": request_collection,
        "storage_collection": storage_collection,
        "key_field": key_field,
        "content_hash_field": content_hash_field,
        "chunk_index_field": chunk_index_field,
    }))
}

fn business_os_file_plane_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn business_os_file_plane_identifier_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        && value
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

pub(super) fn active_team_data_grant_scope(
    conn: &Connection,
    module_id: &str,
    collection: &str,
    permission: BusinessOsPermission,
) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT scope_type
         FROM business_permission_grants
         WHERE active = 1
           AND subject_type = 'role'
           AND subject_id = 'user'
           AND permission = ?1
           AND (
                (scope_type = 'collection' AND scope_id = ?2)
                OR (scope_type = 'module' AND scope_id = ?3)
           )
         ORDER BY CASE scope_type WHEN 'collection' THEN 0 ELSE 1 END
         LIMIT 1",
    )?;
    let scope = stmt
        .query_row(params![permission.as_str(), collection, module_id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    Ok(scope)
}

fn module_is_local(manifest: &ModuleManifest) -> bool {
    manifest.source == "local"
        || manifest.install_scope == "local"
        || manifest.entry.trim().starts_with("local-modules/")
}

fn lifecycle_declared_string(module: &Value, key: &str) -> Option<String> {
    module
        .get("lifecycle")
        .and_then(|lifecycle| lifecycle.get(key))
        .and_then(Value::as_str)
        .or_else(|| module.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn lifecycle_declared_string_array(module: &Value, key: &str) -> Vec<String> {
    module
        .get("lifecycle")
        .and_then(|lifecycle| lifecycle.get(key))
        .or_else(|| module.get(key))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(super) fn legacy_preview_audience_grant_id(module_id: &str, user_id: &str) -> String {
    let digest = Sha256::digest(format!("{}:{}", module_id.trim(), user_id.trim()).as_bytes());
    let suffix = digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let module_slug = source_sanitize_slug(module_id);
    format!("legacy_preview_app_view_{module_slug}_{suffix}")
}

fn backfill_manifest_preview_audience_grants(
    conn: &Connection,
    modules: &[ModuleManifest],
    now: i64,
) -> anyhow::Result<usize> {
    let mut inserted = 0usize;

    for manifest in modules {
        if !module_is_runtime_installed(manifest) {
            continue;
        }
        let module_value = serde_json::to_value(manifest)?;
        let mut preview_user_ids =
            lifecycle_declared_string_array(&module_value, "preview_user_ids");
        if preview_user_ids.is_empty() {
            continue;
        }
        preview_user_ids.sort();
        preview_user_ids.dedup();

        let semver_major = parse_business_app_semver_major(&manifest.version);
        let explicit_state = normalized_lifecycle_state(lifecycle_declared_string(
            &module_value,
            "visibility_state",
        ));
        let explicit_audience = lifecycle_declared_string(&module_value, "audience")
            .map(|value| value.to_ascii_lowercase());
        let migrate_audience = semver_major == Some(0)
            || matches!(explicit_state.as_deref(), Some("restricted"))
            || matches!(explicit_audience.as_deref(), Some("restricted"));
        if !migrate_audience {
            continue;
        }

        let module_id = manifest.id.trim();
        if module_id.is_empty() {
            continue;
        }
        for user_id in preview_user_ids {
            let user_id = user_id.trim();
            if user_id.is_empty() {
                continue;
            }
            let grant_id = legacy_preview_audience_grant_id(module_id, user_id);
            inserted += conn.execute(
                "INSERT OR IGNORE INTO business_permission_grants
                    (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
                     active, reason, created_by, created_at_ms, updated_at_ms)
                 SELECT ?1, 'user', ?2, ?3, 'module', ?4, 1, ?5, ?6, ?7, ?7
                 WHERE NOT EXISTS (
                    SELECT 1
                    FROM business_permission_grants
                    WHERE active = 1
                      AND subject_type = 'user'
                      AND subject_id = ?2
                      AND permission = ?3
                      AND scope_type = 'module'
                      AND scope_id = ?4
                 )",
                params![
                    grant_id,
                    user_id,
                    BusinessOsPermission::AppsView.as_str(),
                    module_id,
                    "Migrated from legacy lifecycle.preview_user_ids",
                    "ctox.lifecycle.preview_user_ids.backfill",
                    now
                ],
            )?;
        }
    }

    Ok(inserted)
}

fn backfill_semver_public_release_records(
    conn: &Connection,
    modules: &[ModuleManifest],
    now: i64,
) -> anyhow::Result<usize> {
    let mut inserted = 0usize;
    for manifest in modules {
        if !module_is_runtime_installed(manifest)
            || parse_business_app_semver_major(&manifest.version).unwrap_or(0) < 1
        {
            continue;
        }
        let module_id = manifest.id.trim();
        let already_released: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_module_releases
             WHERE module_id = ?1 AND status = 'released'",
            params![module_id],
            |row| row.get(0),
        )?;
        if already_released > 0 {
            continue;
        }
        let manifest_value = serde_json::to_value(manifest)?;
        let collections = module_manifest_collection_ids(&manifest_value);
        let version_id = format!("modrel_{}_legacy_{}", module_id, Uuid::new_v4());
        let data_access_review = serde_json::json!({
            "completed": true,
            "reviewed_at_ms": now,
            "reviewed_by": "ctox.release-record-migration",
            "read_collections": [],
            "write_collections": [],
            "locked_read_collections": collections,
            "locked_write_collections": module_manifest_collection_ids(&manifest_value),
            "locked_state_behavior": "Existing access is preserved by explicit grants; other users see a locked state.",
            "review_is_evidence_only": true,
            "grants_implied": false,
            "migration": "semver_visibility_to_release_record_v1"
        });
        let snapshot = serde_json::json!({
            "module_json": manifest_value.clone(),
            "target_version": manifest.version.clone(),
            "release_channel": "team",
            "source_version_id": "",
            "rollback_version_id": "",
            "responsible_user_ids": [],
            "data_access_review": data_access_review,
            "migration": "semver_visibility_to_release_record_v1"
        });
        inserted += conn.execute(
            "INSERT INTO business_module_releases
                (version_id, module_id, version, status, manifest_json, snapshot_json,
                 created_by, created_at_ms, notes)
             VALUES (?1, ?2, 1, 'released', ?3, ?4,
                     'ctox.release-record-migration', ?5,
                     'Backfilled from legacy SemVer-derived team visibility')",
            params![
                version_id,
                module_id,
                serde_json::to_string(&manifest_value)?,
                serde_json::to_string(&snapshot)?,
                now
            ],
        )?;
    }
    Ok(inserted)
}

const LEGACY_MODULE_LIFECYCLE_MIGRATION_ID: &str =
    "business_os.legacy_module_lifecycle_authority.v1";
