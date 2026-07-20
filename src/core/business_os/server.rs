// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use base64::Engine;
use ctox_app_server_protocol::AuthMode as ApiAuthMode;
use polars::prelude::*;
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Write;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;
use url::Url;
use uuid::Uuid;

use super::policy;
use super::store;

const CORE_MODULE_IDS: &[&str] = &["ctox", "appsec-pentest", "knowledge"];
const CHATGPT_AUTH_ISSUER: &str = "https://auth.openai.com";
const CHATGPT_AUTH_CALLBACK_PORT: u16 = 1455;
const CHATGPT_AUTH_CALLBACK_FALLBACK_PORT: u16 = 1457;
const CHATGPT_AUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CHATGPT_AUTH_SECRET_SCOPE: &str = "ctox-auth";
const CHATGPT_AUTH_SECRET_NAME: &str = "chatgpt_subscription_auth_json";
const BUSINESS_OS_HTTP_WORKERS: usize = 4;
const BUSINESS_OS_HTTP_QUEUE_CAPACITY: usize = 256;

#[derive(Clone)]
struct PendingChatgptSubscriptionLogin {
    redirect_uri: String,
    pkce: ChatgptLoginPkce,
    state: String,
}

#[derive(Debug, Clone)]
pub struct BusinessOsServeOptions {
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    developer: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    store: Value,
    #[serde(default)]
    install_scope: String,
    #[serde(default)]
    default_installed: bool,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    collections: Vec<String>,
    #[serde(default)]
    layout: Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    launch_kind: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    presentation: Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    icon_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    icon_svg: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    core: bool,
    #[serde(default)]
    editable: bool,
    #[serde(default)]
    deletable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    manifest_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    local_manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    source_module: String,
    #[serde(default)]
    default_title: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct KnowledgeCommandRequest {
    args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallTemplateRequest {
    template_id: String,
    #[serde(default)]
    module_id: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertModuleRequest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    collections: Vec<String>,
    #[serde(default)]
    layout: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct DeleteModuleRequest {
    module_id: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct SubscriptionAuthStartRequest {
    #[serde(default)]
    callback_url: Option<String>,
}

pub fn serve_business_os(root: &Path, options: BusinessOsServeOptions) -> anyhow::Result<()> {
    let app_root = resolve_business_os_app_root(root);
    if !app_root.join("index.html").is_file() {
        anyhow::bail!(
            "native Business OS app is missing at {}",
            app_root.display()
        );
    }
    let _conn = store::open_store(root)?;
    match super::rxdb_peer::ensure_native_peer(root) {
        Ok(()) => {}
        Err(err) => eprintln!("[business-os] native rxdb peer config failed: {err:#}"),
    }
    if let Err(err) = store::write_module_catalog_projection_to_rxdb(root) {
        eprintln!("[business-os] module catalog refresh failed at serve start: {err:#}");
    }
    let server = Server::http(&options.addr)
        .map_err(|err| anyhow::anyhow!("failed to bind Business OS server: {err}"))?;
    println!("CTOX Business OS listening on http://{}", options.addr);
    println!("Serving {}", app_root.display());
    let (request_tx, request_rx) = sync_channel(BUSINESS_OS_HTTP_QUEUE_CAPACITY);
    let request_rx = Arc::new(Mutex::new(request_rx));
    for worker_index in 0..BUSINESS_OS_HTTP_WORKERS {
        let root = root.to_path_buf();
        let app_root = app_root.clone();
        let request_rx = Arc::clone(&request_rx);
        thread::Builder::new()
            .name(format!("business-os-http-{worker_index}"))
            .spawn(move || loop {
                let request = {
                    let receiver = request_rx
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    receiver.recv()
                };
                let Ok(request) = request else {
                    break;
                };
                if let Err(err) = handle_request(&root, &app_root, request) {
                    eprintln!("[business-os] request failed: {err:#}");
                }
            })
            .context("failed to start Business OS HTTP worker")?;
    }
    for request in server.incoming_requests() {
        if request_tx.send(request).is_err() {
            anyhow::bail!("Business OS HTTP workers stopped");
        }
    }
    Ok(())
}

fn resolve_business_os_app_root(root: &Path) -> PathBuf {
    [
        root.join("business-os"),
        root.join("src/apps/business-os"),
        root.join("archive/2026-05-18-cleanup/generated/business-os"),
    ]
    .into_iter()
    .find(|candidate| candidate.join("index.html").is_file())
    .unwrap_or_else(|| root.join("business-os"))
}

fn resolve_business_os_installed_app_root(root: &Path) -> PathBuf {
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "runtime")
    {
        return root.join("business-os");
    }
    let runtime = root.join("runtime");
    if runtime.exists() {
        return runtime.join("business-os");
    }
    let direct = root.join("business-os");
    if direct.exists() {
        return direct;
    }
    root.join("business-os")
}

fn handle_request(root: &Path, app_root: &Path, mut request: Request) -> anyhow::Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    if method == Method::Options {
        respond_options(request)?;
        return Ok(());
    }
    if rejects_cross_origin_browser_mutation(
        &method,
        path,
        business_os_session_cookie_value(&request).as_deref(),
        header_value(&request, "Origin").as_deref(),
        header_value(&request, "Host").as_deref(),
        header_value(&request, "X-Forwarded-Proto").as_deref(),
    ) {
        respond_status(request, 403, "cross-origin session mutation rejected")?;
        return Ok(());
    }
    // RxDB/WebRTC-only data plane: Business OS HTTP data APIs stay hard-disabled
    // except for explicit control-plane endpoints such as ChatGPT subscription
    // auth, which cannot depend on a healthy browser-to-native peer before the
    // account is connected.
    if path.starts_with("/api/business-os") && !is_business_os_control_plane_path(path) {
        respond_status(
            request,
            410,
            "Business OS HTTP data APIs are disabled; use RxDB/WebRTC.",
        )?;
        return Ok(());
    }
    match (method.clone(), path) {
        (Method::Get, "/api/business-os/status") => {
            respond_json(request, &store::status(root)?)?;
        }
        (Method::Post, "/api/business-os/auth/capability") => {
            // §9.1: issue a capability token bound to the SERVER-authenticated
            // session user (id + role come from the validated session, never from
            // caller-supplied input). The browser attaches the returned token to
            // every command so native authorization stops trusting the
            // browser-asserted client_context.actor.
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                match store::issue_business_os_capability_token_for_session(root, &session, now) {
                    Ok((token, expires_at_ms)) => respond_json_value(
                        request,
                        serde_json::json!({
                            "ok": true,
                            "capability_token": token,
                            "expires_at_ms": expires_at_ms
                        }),
                    )?,
                    Err(err) => respond_status(
                        request,
                        403,
                        &format!("capability token unavailable: {err}"),
                    )?,
                }
            }
        }
        (Method::Get, "/api/business-os/mcp/connect-info") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let payload = mcp_connect_info_payload(root, &request)?;
                respond_json_value_no_store(request, payload)?;
            }
        }
        (Method::Post, "/api/business-os/ctox/subscription-auth/start") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let options: SubscriptionAuthStartRequest = serde_json::from_value(body)?;
                respond_json_value(
                    request,
                    subscription_auth_start_payload(root, options.callback_url)?,
                )?;
            }
        }
        (Method::Get, "/api/business-os/ctox/subscription-auth/callback") => {
            let url_raw = request.url().to_owned();
            handle_subscription_auth_callback(request, root, &url_raw)?;
        }
        (Method::Get, "/api/business-os/ctox/maintenance") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                // Instance-scoped operational state: every authenticated actor
                // sees the same lease. No Business OS records or native status
                // documents are transported through this control-plane route.
                match crate::install::business_os_maintenance_status(root) {
                    Ok(payload) => respond_json_value_no_store(request, payload)?,
                    Err(error) => respond_status(request, 500, &error.to_string())?,
                }
            }
        }
        (Method::Get, "/api/business-os/ctox/update/check") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                match crate::install::business_os_update_check(root) {
                    Ok(payload) => respond_json_value_no_store(request, payload)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/ctox/update/apply") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                match start_business_os_update_apply(root) {
                    Ok(payload) => respond_json_value_no_store(request, payload)?,
                    Err(error) => respond_status(request, 500, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/ctox/tasks/update") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation = serde_json::from_value(body)?;
                respond_json_value(request, store::update_ctox_task(root, &session, mutation)?)?;
            }
        }
        (Method::Post, "/api/business-os/ctox/tasks/delete") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation = serde_json::from_value(body)?;
                respond_json_value(request, store::delete_ctox_task(root, &session, mutation)?)?;
            }
        }
        (Method::Get, "/api/business-os/session") => {
            let session = request_session(root, &request);
            respond_json(request, &session)?;
        }
        (Method::Post, "/login") => {
            handle_login_request(root, request)?;
        }
        (Method::Get, "/logout") => {
            // Revoke the server-side session so the opaque token is dead even if
            // the cookie is replayed; then clear the cookie in the browser.
            if let Some(token) = business_os_session_cookie_value(&request) {
                let _ = store::revoke_business_session(root, &token);
            }
            respond_redirect_with_cookie(request, "/", "", 0)?;
        }
        (Method::Get, "/api/business-os/users") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                respond_json_value(request, store::list_users(root, &session)?)?;
            }
        }
        (Method::Post, "/api/business-os/users") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !session
                .user
                .as_ref()
                .map(|user| user.is_admin)
                .unwrap_or(false)
            {
                respond_status(request, 403, "admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation = serde_json::from_value(body)?;
                respond_json_value(request, store::upsert_user(root, &session, mutation)?)?;
            }
        }
        (Method::Get, "/api/business-os/modules") => {
            let session = request_session(root, &request);
            let installed_app_root = resolve_business_os_installed_app_root(root);
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "modules": load_module_manifests(app_root, &installed_app_root)?,
                    "governance": store::module_governance_map(root, &session)?
                }),
            )?;
        }
        (Method::Get, "/api/business-os/module-governance") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                respond_json_value(request, store::module_governance_map(root, &session)?)?;
            }
        }
        (Method::Post, "/api/business-os/modules") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation: UpsertModuleRequest = serde_json::from_value(body)?;
                if !store::session_can_modify_module(root, &session, &mutation.id)? {
                    respond_status(request, 403, "module modification rights required")?;
                    return Ok(());
                }
                let installed_app_root = resolve_business_os_installed_app_root(root);
                let manifest = upsert_module_manifest(app_root, &installed_app_root, mutation)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true,
                        "module": manifest
                    }),
                )?;
            }
        }
        (Method::Get, "/api/business-os/module-layout") => {
            respond_json_value(request, load_module_layout(root)?)?;
        }
        (Method::Post, "/api/business-os/module-layout") => {
            let body = read_json(&mut request)?;
            save_module_layout(root, &body)?;
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "layout": body
                }),
            )?;
        }
        (Method::Get, "/api/business-os/templates") => {
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "templates": load_template_manifests(app_root)?
                }),
            )?;
        }
        (Method::Get, "/api/business-os/knowledge") => {
            respond_json_value(request, knowledge_index_payload(root)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/document") => {
            let id = query_param(&url, "id").unwrap_or_default();
            respond_json_value(request, knowledge_document_payload(root, &id)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/dataframe/schema") => {
            let id = query_param(&url, "id").unwrap_or_default();
            respond_json_value(request, knowledge_dataframe_schema_payload(root, &id)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/dataframe/rows") => {
            let query = parse_query(&url);
            let id = query.get("id").cloned().unwrap_or_default();
            let offset = parse_usize_query(&query, "offset", 0);
            let limit = parse_usize_query(&query, "limit", 120).clamp(1, 500);
            respond_json_value(
                request,
                knowledge_dataframe_rows_payload(root, &id, offset, limit)?,
            )?;
        }
        _ if path.starts_with("/api/business-os/knowledge") => {
            respond_status(request, 404, "unknown Business OS knowledge endpoint")?;
        }
        (Method::Post, "/api/business-os/modules/install-template") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let install = serde_json::from_value(body)?;
                let installed_app_root = resolve_business_os_installed_app_root(root);
                let manifest = install_template_module(app_root, &installed_app_root, install)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true,
                        "module": manifest
                    }),
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/delete") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let delete: DeleteModuleRequest = serde_json::from_value(body)?;
                if !store::session_can_modify_module(root, &session, &delete.module_id)? {
                    respond_status(request, 403, "module modification rights required")?;
                    return Ok(());
                }
                delete_installed_module(app_root, root, delete)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true
                    }),
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/assign-founder") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let assignment = serde_json::from_value(body)?;
                respond_json_value(
                    request,
                    store::assign_module_founder(root, &session, assignment)?,
                )?;
            }
        }
        (Method::Post, "/api/business-os/reports") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let report = serde_json::from_value(body)?;
                respond_json_value(request, store::record_report(root, &session, report)?)?;
            }
        }
        (Method::Get, "/api/business-os/sync/config") => {
            let session = request_session(root, &request);
            let turn_session = session
                .user
                .as_ref()
                .map(|user| user.id.clone())
                .unwrap_or_else(|| "browser".to_owned());
            respond_json(
                request,
                &store::sync_config_for_browser(root, &turn_session)?,
            )?;
        }
        (Method::Post, "/api/business-os/sync/native-peer/restart") => {
            if std::env::var_os("CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS").is_none() {
                respond_status(request, 403, "native peer restart is not enabled")?;
            } else {
                respond_json_value(request, super::rxdb_peer::restart_native_peer(root)?)?;
            }
        }
        (Method::Get, "/api/business-os/ctox/harness-flow") => {
            respond_json_value(request, latest_harness_flow_payload(root))?;
        }
        // ---------- Channels tab ----------
        (Method::Get, "/api/business-os/channels/accounts") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                match crate::mission::channels::list_communication_accounts_for_business_os(root) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 500, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/channels/test") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let channel = body.get("channel").and_then(Value::as_str).unwrap_or("");
                let account_key = body
                    .get("account_key")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty());
                match crate::mission::channels::test_channel_for_business_os(
                    root,
                    channel,
                    account_key,
                ) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/channels/sync") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let channel = body.get("channel").and_then(Value::as_str).unwrap_or("");
                match crate::mission::channels::sync_channel_for_business_os(root, channel) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/channels/settings") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let channel = body.get("channel").and_then(Value::as_str).unwrap_or("");
                let config = body.get("config").cloned().unwrap_or_else(|| Value::Null);
                match crate::mission::channels::save_channel_settings_for_business_os(
                    root, channel, &config,
                ) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/channels/disconnect") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let account_key = body
                    .get("account_key")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match crate::mission::channels::disconnect_communication_account_for_business_os(
                    root,
                    account_key,
                ) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Post, "/api/business-os/channels/pair/start") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let channel = body.get("channel").and_then(Value::as_str).unwrap_or("");
                match crate::mission::channels::start_pairing_for_business_os(root, channel) {
                    Ok(value) => respond_json_value(request, value)?,
                    Err(error) => respond_status(request, 400, &error.to_string())?,
                }
            }
        }
        (Method::Get, "/api/business-os/channels/pair/state") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let channel = query_param(&url, "channel").unwrap_or_default();
                let payload =
                    crate::mission::channels::read_pairing_state_for_business_os(root, &channel);
                respond_json_value(request, payload)?;
            }
        }
        (Method::Post, "/api/business-os/channels/jami/export") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let payload = crate::mission::channels::export_jami_archive_for_business_os(root);
                respond_json_value(request, payload)?;
            }
        }
        (Method::Post, "/api/business-os/channels/jami/create") => {
            let session = request_session(root, &request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let display_name = body
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or("CTOX");
                let config = serde_json::json!({ "profile_name": display_name });
                let save_result = crate::mission::channels::save_channel_settings_for_business_os(
                    root, "jami", &config,
                );
                if let Err(error) = save_result {
                    respond_status(request, 400, &error.to_string())?;
                } else {
                    match crate::mission::channels::start_pairing_for_business_os(root, "jami") {
                        Ok(value) => respond_json_value(request, value)?,
                        Err(error) => respond_status(request, 400, &error.to_string())?,
                    }
                }
            }
        }
        _ if method == Method::Get => serve_static(root, app_root, request, path)?,
        _ => respond_status(request, 405, "method not allowed")?,
    }
    Ok(())
}

fn is_business_os_control_plane_path(path: &str) -> bool {
    matches!(
        path,
        "/api/business-os/ctox/subscription-auth/start"
            | "/api/business-os/ctox/subscription-auth/callback"
            // Admin-triggered release control-plane: release metadata check
            // and update subprocess launch. No Business OS records flow here.
            | "/api/business-os/ctox/update/check"
            | "/api/business-os/ctox/update/apply"
            // Instance-scoped upgrade lease only. This endpoint never carries
            // Business OS collection records and is deliberately identical for
            // Owner/Admin and every other authenticated actor.
            | "/api/business-os/ctox/maintenance"
            // §9.1 auth/control-plane: issues a capability token bound to the
            // server-authenticated session. No Business OS records flow here.
            | "/api/business-os/auth/capability"
            // Browser launch/ICE refresh control-plane. This returns signaling,
            // TURN, and native-peer metadata only; Business OS records still
            // move exclusively through RxDB/WebRTC.
            | "/api/business-os/sync/config"
            // Admin-only MCP setup metadata. It may reveal the inbound MCP
            // bearer token, so it is a no-store control-plane route and never
            // crosses the RxDB/WebRTC business-data plane.
            | "/api/business-os/mcp/connect-info"
            // Peer-lifecycle control for the rxdb-soak rollover mode: restarts
            // the in-process native peer. No Business OS records flow here and
            // the route itself answers 403 unless
            // CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS is set (smoke runs only).
            | "/api/business-os/sync/native-peer/restart"
    )
}

fn start_business_os_update_apply(root: &Path) -> anyhow::Result<Value> {
    let runtime_dir = root.join("runtime");
    fs::create_dir_all(&runtime_dir).with_context(|| {
        format!(
            "failed to create Business OS update log directory {}",
            runtime_dir.display()
        )
    })?;
    let log_path = runtime_dir.join(format!(
        "business-os-update-{}.log",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    let stdout = fs::File::create(&log_path)
        .with_context(|| format!("failed to create {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone {}", log_path.display()))?;
    let exe = std::env::current_exe().context("failed to resolve current CTOX executable")?;
    let mut child = std::process::Command::new(exe)
        .args(["update", "apply", "--latest"])
        .env("CTOX_ROOT", root.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("failed to start CTOX update subprocess")?;
    let pid = child.id();
    thread::spawn(move || {
        if let Err(err) = child.wait() {
            eprintln!("[business-os] CTOX update subprocess wait failed: {err}");
        }
    });
    Ok(serde_json::json!({
        "ok": true,
        "status": "started",
        "pid": pid,
        "log_path": log_path,
        "command": ["ctox", "update", "apply", "--latest"],
    }))
}

fn request_session(root: &Path, request: &Request) -> store::BusinessOsSession {
    // 1. Browser shell session: an opaque cookie token resolved against the
    //    server-side session store. The cookie never carries credentials, so a
    //    successful lookup IS the authentication; the live actor role/active
    //    state is then refreshed by session_with_persisted_user.
    if let Some(token) = business_os_session_cookie_value(request) {
        if let Ok(Some(session)) = store::session_from_cookie_token(root, &token) {
            return store::session_with_persisted_user(root, session.clone()).unwrap_or(session);
        }
    }
    // 2. Header-based auth for API/MCP clients (Authorization Bearer/Basic,
    //    X-CTOX-Business-OS-Session). The reversible cookie credential-replay
    //    path is gone — cookies are opaque tokens only.
    let auth_header = header_value(request, "Authorization");
    let session_header = header_value(request, "X-CTOX-Business-OS-Session");
    let session = store::session_for_request(
        auth_header.as_deref(),
        session_header.as_deref(),
        request_allows_local_dev_session(request),
    );
    let session = store::session_with_persisted_user(root, session).unwrap_or_else(|_| {
        store::session_for_request(
            auth_header.as_deref(),
            session_header.as_deref(),
            request_allows_local_dev_session(request),
        )
    });
    if session.authenticated {
        return session;
    }

    // Managed ctox.dev control-plane requests mint a short-lived native
    // capability token over SSH and replay it as Authorization: Bearer for
    // localhost-only control-plane fetches. Treat that signed token as an API
    // session, then let the normal route-level role checks decide access.
    session_from_capability_bearer(root, auth_header.as_deref()).unwrap_or(session)
}

fn session_from_capability_bearer(
    root: &Path,
    auth_header: Option<&str>,
) -> Option<store::BusinessOsSession> {
    let token = auth_header
        .map(str::trim)?
        .strip_prefix("Bearer ")
        .map(str::trim)?;
    let (id, role) = store::verify_capability_actor(root, token)?;
    let role = policy::normalize_role(&role);
    let session = store::BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: true,
        user: Some(store::BusinessOsSessionUser {
            id: id.clone(),
            display_name: id,
            role: role.clone(),
            is_admin: policy::role_can_manage(&role),
        }),
        login_url: None,
        reason: None,
    };
    Some(store::session_with_persisted_user(root, session.clone()).unwrap_or(session))
}

fn request_allows_local_dev_session(request: &Request) -> bool {
    // SECURITY: the implicit local-dev (admin) session requires both a loopback
    // TCP peer and, when present, a loopback Host header. The Host header is
    // client-controlled, so it is never sufficient by itself; it is only used as
    // an additional deny signal for managed/public domains that proxy to a
    // same-host loopback service.
    let peer_is_loopback = request
        .remote_addr()
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false);
    if !peer_is_loopback {
        return false;
    }
    header_value(request, "Host")
        .as_deref()
        .map(host_header_allows_local_dev_session)
        .unwrap_or(true)
}

fn host_header_allows_local_dev_session(host: &str) -> bool {
    let Some(hostname) = host_header_hostname(host) else {
        return false;
    };
    hostname.eq_ignore_ascii_case("localhost")
        || hostname.to_ascii_lowercase().ends_with(".localhost")
        || hostname
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

#[allow(dead_code)]
fn host_header_hostname(host: &str) -> Option<String> {
    let value = host.split(',').next()?.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix('[') {
        let (hostname, _) = rest.split_once(']')?;
        return (!hostname.trim().is_empty()).then(|| hostname.trim().to_owned());
    }
    let hostname = match value.rsplit_once(':') {
        Some((prefix, port))
            if !prefix.contains(':') && port.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            prefix
        }
        _ => value,
    };
    let hostname = hostname.trim();
    (!hostname.is_empty()).then(|| hostname.to_owned())
}

fn query_param(url_raw: &str, key: &str) -> Option<String> {
    Url::parse(&format!("http://localhost{url_raw}"))
        .ok()?
        .query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn subscription_auth_start_payload(
    root: &Path,
    callback_url: Option<String>,
) -> anyhow::Result<Value> {
    let login = start_chatgpt_subscription_login(root, callback_url)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": if login.device_user_code.is_some() { "device_code" } else { "auth_url" },
        "login_id": login.login_id,
        "auth_url": login.auth_url,
        "redirect_uri": login.redirect_uri,
        "verification_url": login.verification_url,
        "user_code": login.device_user_code,
        "message": "ChatGPT Subscription Autorisierung gestartet."
    }))
}

fn mcp_connect_info_payload(root: &Path, request: &Request) -> anyhow::Result<Value> {
    let sync = store::sync_config(root)?;
    let managed_alias = managed_mcp_instance_id(request, &sync.instance_id);
    let local_endpoint = "http://127.0.0.1:8788/mcp".to_string();
    let managed_endpoint = format!("https://mcp.ctox.dev/mcp/{managed_alias}");
    let managed_connect_url = format!("wss://mcp.ctox.dev/connect/{managed_alias}");
    let managed_dashboard_url = format!("https://ctox.dev/dashboard?tenant={managed_alias}#mcp");
    let token = super::mcp_channel::mcp_operator_auth_token(root)?;
    let server_name = format!("{}-business-os-local", managed_alias.replace('.', "-"));
    let authorization_header = format!("Bearer {token}");

    let mut codex_servers = serde_json::Map::new();
    codex_servers.insert(
        server_name.clone(),
        serde_json::json!({
            "url": local_endpoint,
            "headers": {
                "Authorization": authorization_header
            }
        }),
    );
    let mut claude_servers = serde_json::Map::new();
    claude_servers.insert(
        server_name.clone(),
        serde_json::json!({
            "type": "http",
            "url": local_endpoint,
            "headers": {
                "Authorization": authorization_header
            }
        }),
    );

    Ok(serde_json::json!({
        "ok": true,
        "status": "local_ready_managed_not_connected",
        "mode": "local",
        "server_name": server_name,
        "endpoint": local_endpoint,
        "managed_instance_id": managed_alias,
        "native_instance_id": sync.instance_id,
        "token": token,
        "token_type": "bearer",
        "authorization_header": authorization_header,
        "secret": {
            "scope": "business_os",
            "name": "mcp_inbound_auth_token",
            "source": "ctox_secret_store"
        },
        "codex": {
            "mcpServers": codex_servers
        },
        "claude": {
            "mcpServers": claude_servers
        },
        "managed": {
            "status": "not_connected",
            "endpoint": managed_endpoint,
            "connect_url": managed_connect_url,
            "dashboard_url": managed_dashboard_url,
            "instance_alias": managed_alias,
            "native_instance_id": sync.instance_id,
            "requires": [
                "ctox.dev Managed MCP client token",
                "ctox.dev instance connect token",
                "running outbound ctox business-os mcp connect service"
            ]
        },
        "notes": [
            "Local MCP is ready on 127.0.0.1 for agents running on the same machine or through an operator-managed tunnel.",
            "Managed Web Auth is not connected yet. Do not use the local bearer token as a mcp.ctox.dev client token.",
            "For managed setup, open the managed.dashboard_url, switch to MCP, press Token rotieren, and copy Neuer Token.",
            "MCP clients must be configured on the client side; a running chat cannot safely install this server by tool call."
        ]
    }))
}

fn managed_mcp_instance_id(request: &Request, fallback_instance_id: &str) -> String {
    for header in ["X-Forwarded-Host", "Host"] {
        if let Some(host) =
            header_value(request, header).and_then(|value| host_header_hostname(&value))
        {
            let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
            if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
                continue;
            }
            if host.ends_with(".ctox.dev") {
                if let Some(slug) = host.strip_suffix(".ctox.dev").map(str::trim) {
                    if !slug.is_empty() && !slug.contains('.') {
                        return slug.to_string();
                    }
                }
            }
            if host.parse::<IpAddr>().is_err() {
                return host;
            }
        }
    }
    fallback_instance_id.to_string()
}

struct StartedChatgptSubscriptionLogin {
    login_id: String,
    auth_url: String,
    redirect_uri: String,
    device_user_code: Option<String>,
    verification_url: Option<String>,
}

#[derive(Clone)]
struct ChatgptLoginPkce {
    verifier: String,
    challenge: String,
}

fn start_chatgpt_subscription_login(
    root: &Path,
    callback_url: Option<String>,
) -> anyhow::Result<StartedChatgptSubscriptionLogin> {
    let codex_home = ctox_core::config::find_codex_home()
        .context("Codex/CTOX Auth-Store konnte nicht aufgelöst werden")?;
    let pkce = chatgpt_login_pkce();
    let state = chatgpt_login_state();
    let login_id = Uuid::new_v4().to_string();
    if let Some(callback_url) = callback_url
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        let _ = external_chatgpt_callback_url(&callback_url, &login_id)?;
        let device = request_chatgpt_device_code()?;
        let verification_url = format!("{CHATGPT_AUTH_ISSUER}/codex/device");
        let redirect_uri = format!("{CHATGPT_AUTH_ISSUER}/deviceauth/callback");
        let auth_url = verification_url.clone();
        let device_auth_id = device.device_auth_id.clone();
        let device_user_code = device.user_code.clone();
        let device_interval_secs = device.interval_secs;
        let worker_login_id = login_id.clone();
        let worker_redirect_uri = redirect_uri.clone();
        let worker_root = root.to_path_buf();
        thread::spawn(move || {
            if let Err(err) = complete_chatgpt_device_code_login(
                &worker_root,
                &codex_home,
                device_auth_id,
                device_user_code,
                device_interval_secs,
                worker_redirect_uri,
            ) {
                eprintln!("CTOX ChatGPT subscription device login {worker_login_id} failed: {err}");
            }
        });
        return Ok(StartedChatgptSubscriptionLogin {
            login_id,
            auth_url,
            redirect_uri,
            device_user_code: Some(device.user_code),
            verification_url: Some(verification_url),
        });
    }
    let (server, port) = bind_chatgpt_login_server()
        .context("Lokaler ChatGPT-Login-Callback konnte nicht gestartet werden")?;
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let auth_url = build_chatgpt_authorize_url(&redirect_uri, &pkce.challenge, &state);
    let worker_login_id = login_id.clone();
    let worker_redirect_uri = redirect_uri.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        if let Err(err) = run_chatgpt_login_callback_server(
            server,
            root,
            codex_home,
            worker_redirect_uri,
            pkce,
            state,
        ) {
            eprintln!("CTOX ChatGPT subscription login {worker_login_id} failed: {err}");
        }
    });
    Ok(StartedChatgptSubscriptionLogin {
        login_id,
        auth_url,
        redirect_uri,
        device_user_code: None,
        verification_url: None,
    })
}

fn external_chatgpt_callback_url(callback_url: &str, login_id: &str) -> anyhow::Result<String> {
    let mut parsed = Url::parse(callback_url)
        .with_context(|| format!("Ungültige ChatGPT Callback-URL: {callback_url}"))?;
    match parsed.scheme() {
        "https" => {}
        "http" if matches!(parsed.host_str(), Some("localhost" | "127.0.0.1")) => {}
        _ => anyhow::bail!("ChatGPT Callback-URL muss HTTPS verwenden"),
    }
    parsed.set_fragment(None);
    parsed.query_pairs_mut().append_pair("login_id", login_id);
    Ok(parsed.to_string())
}

struct ChatgptDeviceCode {
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct ChatgptDeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

fn request_chatgpt_device_code() -> anyhow::Result<ChatgptDeviceCode> {
    let response = ureq::post(&format!(
        "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/usercode"
    ))
    .set("Content-Type", "application/json")
    .send_json(serde_json::json!({
        "client_id": ctox_core::auth::CLIENT_ID,
    }));
    let body: Value = match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from)?,
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("Device-Code-Anforderung fehlgeschlagen ({status}): {body}")
        }
        Err(err) => return Err(anyhow::Error::from(err)),
    };
    let device_auth_id = body
        .get("device_auth_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keine device_auth_id")?;
    let user_code = body
        .get("user_code")
        .or_else(|| body.get("usercode"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keinen user_code")?;
    let interval_secs = body
        .get("interval")
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(5)
        .max(1);
    Ok(ChatgptDeviceCode {
        device_auth_id,
        user_code,
        interval_secs,
    })
}

fn complete_chatgpt_device_code_login(
    root: &Path,
    codex_home: &Path,
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
    redirect_uri: String,
) -> anyhow::Result<()> {
    let token = poll_chatgpt_device_token(device_auth_id, user_code, interval_secs)?;
    let tokens = exchange_chatgpt_authorization_code(
        &token.authorization_code,
        &redirect_uri,
        &token.code_verifier,
    )?;
    persist_chatgpt_subscription_auth(root, codex_home, tokens)
}

fn poll_chatgpt_device_token(
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
) -> anyhow::Result<ChatgptDeviceTokenResponse> {
    let started = Instant::now();
    let max_wait = Duration::from_secs(15 * 60);
    let sleep_for = Duration::from_secs(interval_secs).min(Duration::from_secs(15));
    loop {
        let response = ureq::post(&format!(
            "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/token"
        ))
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "device_auth_id": &device_auth_id,
            "user_code": &user_code,
        }));
        match response {
            Ok(response) => return response.into_json().map_err(anyhow::Error::from),
            Err(ureq::Error::Status(status, response)) if status == 403 || status == 404 => {
                if started.elapsed() >= max_wait {
                    anyhow::bail!("Device-Code-Login ist nach 15 Minuten abgelaufen");
                }
                let _ = response.into_string();
                thread::sleep(sleep_for);
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                anyhow::bail!("Device-Code-Token-Abfrage fehlgeschlagen ({status}): {body}")
            }
            Err(err) => return Err(anyhow::Error::from(err)),
        }
    }
}

fn pending_chatgpt_logins() -> &'static Mutex<HashMap<String, PendingChatgptSubscriptionLogin>> {
    static LOGINS: OnceLock<Mutex<HashMap<String, PendingChatgptSubscriptionLogin>>> =
        OnceLock::new();
    LOGINS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_pending_chatgpt_login(login: PendingChatgptSubscriptionLogin) -> anyhow::Result<()> {
    let mut logins = pending_chatgpt_logins()
        .lock()
        .map_err(|_| anyhow::anyhow!("ChatGPT Login-State konnte nicht gespeichert werden"))?;
    logins.insert(login.state.clone(), login);
    Ok(())
}

fn take_pending_chatgpt_login(
    state: &str,
) -> anyhow::Result<Option<PendingChatgptSubscriptionLogin>> {
    let mut logins = pending_chatgpt_logins()
        .lock()
        .map_err(|_| anyhow::anyhow!("ChatGPT Login-State konnte nicht gelesen werden"))?;
    Ok(logins.remove(state))
}

fn chatgpt_login_pkce() -> ChatgptLoginPkce {
    let verifier = format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    ChatgptLoginPkce {
        verifier,
        challenge,
    }
}

fn chatgpt_login_state() -> String {
    let seed = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = Sha256::digest(seed.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn bind_chatgpt_login_server() -> anyhow::Result<(Server, u16)> {
    for port in [
        CHATGPT_AUTH_CALLBACK_PORT,
        CHATGPT_AUTH_CALLBACK_FALLBACK_PORT,
    ] {
        match Server::http(format!("127.0.0.1:{port}")) {
            Ok(server) => return Ok((server, port)),
            Err(_) => continue,
        }
    }
    anyhow::bail!(
        "Ports {CHATGPT_AUTH_CALLBACK_PORT} und {CHATGPT_AUTH_CALLBACK_FALLBACK_PORT} sind belegt"
    )
}

fn build_chatgpt_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", ctox_core::auth::CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", CHATGPT_AUTH_SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "ctox_business_os"),
    ];
    let qs = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", urlencoding_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{CHATGPT_AUTH_ISSUER}/oauth/authorize?{qs}")
}

fn run_chatgpt_login_callback_server(
    server: Server,
    root: PathBuf,
    codex_home: PathBuf,
    redirect_uri: String,
    pkce: ChatgptLoginPkce,
    state: String,
) -> anyhow::Result<()> {
    for request in server.incoming_requests() {
        let url_raw = request.url().to_owned();
        let handled = handle_chatgpt_login_callback_request(
            request,
            &url_raw,
            &root,
            &codex_home,
            &redirect_uri,
            &pkce,
            &state,
        )?;
        if handled {
            break;
        }
    }
    server.unblock();
    Ok(())
}

fn handle_chatgpt_login_callback_request(
    request: Request,
    url_raw: &str,
    root: &Path,
    codex_home: &Path,
    redirect_uri: &str,
    pkce: &ChatgptLoginPkce,
    expected_state: &str,
) -> anyhow::Result<bool> {
    let parsed = Url::parse(&format!("http://localhost{url_raw}"))?;
    if parsed.path() != "/auth/callback" {
        respond_html(request, 404, "Not Found")?;
        return Ok(false);
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state mismatch.",
        )?;
        return Ok(true);
    }
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        respond_html(
            request,
            400,
            &format!("CTOX Login wurde von ChatGPT abgelehnt: {description}"),
        )?;
        return Ok(true);
    }
    let Some(code) = params.get("code").filter(|value| !value.trim().is_empty()) else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: code fehlt.",
        )?;
        return Ok(true);
    };
    match exchange_chatgpt_authorization_code(code, redirect_uri, &pkce.verifier)
        .and_then(|tokens| persist_chatgpt_subscription_auth(root, codex_home, tokens))
    {
        Ok(()) => {
            respond_html(
                request,
                200,
                "CTOX ChatGPT Subscription ist autorisiert. Dieses Fenster kann geschlossen werden.",
            )?;
            Ok(true)
        }
        Err(err) => {
            respond_html(
                request,
                500,
                &format!("CTOX konnte die ChatGPT Subscription nicht speichern: {err}"),
            )?;
            Ok(true)
        }
    }
}

fn handle_subscription_auth_callback(
    request: Request,
    root: &Path,
    url_raw: &str,
) -> anyhow::Result<()> {
    let parsed = Url::parse(&format!("http://localhost{url_raw}"))?;
    if parsed.path() != "/api/business-os/ctox/subscription-auth/callback" {
        respond_html(request, 404, "Not Found")?;
        return Ok(());
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    let Some(state) = params.get("state").filter(|value| !value.trim().is_empty()) else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state fehlt.",
        )?;
        return Ok(());
    };
    let Some(login) = take_pending_chatgpt_login(state)? else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: unbekannter oder abgelaufener state.",
        )?;
        return Ok(());
    };
    if state.as_str() != login.state.as_str() {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state mismatch.",
        )?;
        return Ok(());
    }
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        respond_html(
            request,
            400,
            &format!("CTOX Login wurde von ChatGPT abgelehnt: {description}"),
        )?;
        return Ok(());
    }
    let Some(code) = params.get("code").filter(|value| !value.trim().is_empty()) else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: code fehlt.",
        )?;
        return Ok(());
    };
    let codex_home = ctox_core::config::find_codex_home()
        .context("Codex/CTOX Auth-Store konnte nicht aufgelöst werden")?;
    match exchange_chatgpt_authorization_code(code, &login.redirect_uri, &login.pkce.verifier)
        .and_then(|tokens| persist_chatgpt_subscription_auth(root, &codex_home, tokens))
    {
        Ok(()) => respond_html(
            request,
            200,
            "CTOX ChatGPT Subscription ist autorisiert. Dieses Fenster kann geschlossen werden.",
        )?,
        Err(err) => respond_html(
            request,
            500,
            &format!("CTOX konnte die ChatGPT Subscription nicht speichern: {err}"),
        )?,
    }
    Ok(())
}

fn respond_html(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let mut response = Response::from_string(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>CTOX Login</title><body style=\"font:16px system-ui;padding:32px;background:#10181b;color:#eef5f3\"><h1>CTOX Login</h1><p>{}</p></body>",
        html_escape(body)
    ))
    .with_status_code(status)
    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
    add_common_response_headers(&mut response);
    request.respond(response).map_err(io::Error::other)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ChatgptTokenExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

fn exchange_chatgpt_authorization_code(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> anyhow::Result<ChatgptTokenExchangeResponse> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding_encode(code),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(ctox_core::auth::CLIENT_ID),
        urlencoding_encode(code_verifier)
    );
    let response = ureq::post(&format!("{CHATGPT_AUTH_ISSUER}/oauth/token"))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body);
    match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from),
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("OAuth Token-Exchange fehlgeschlagen ({status}): {body}")
        }
        Err(err) => Err(anyhow::Error::from(err)),
    }
}

fn persist_chatgpt_subscription_auth(
    root: &Path,
    codex_home: &Path,
    tokens: ChatgptTokenExchangeResponse,
) -> anyhow::Result<()> {
    let token_data = ctox_core::token_data::TokenData {
        id_token: ctox_core::token_data::parse_chatgpt_jwt_claims(&tokens.id_token)
            .map_err(anyhow::Error::msg)?,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: chatgpt_account_id_from_jwt(&tokens.id_token),
    };
    let auth = ctox_core::auth::AuthDotJson {
        auth_mode: Some(ApiAuthMode::Chatgpt),
        openai_api_key: None,
        tokens: Some(token_data),
        last_refresh: Some(chrono::Utc::now()),
    };
    ctox_core::auth::save_auth(
        codex_home,
        &auth,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    )?;
    crate::secrets::write_secret_record(
        root,
        CHATGPT_AUTH_SECRET_SCOPE,
        CHATGPT_AUTH_SECRET_NAME,
        &serde_json::to_string(&auth)?,
        Some("ChatGPT Subscription OAuth state for this CTOX instance".to_owned()),
        serde_json::json!({"source": "business_os_subscription_login", "auth_mode": "chatgpt_subscription"}),
    )?;
    Ok(())
}

fn chatgpt_account_id_from_jwt(jwt: &str) -> Option<String> {
    let mut parts = jwt.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .and_then(|claims| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn urlencoding_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn latest_harness_flow_payload(root: &Path) -> Value {
    match crate::service::harness_flow::load_latest_flow(root) {
        Ok(flow) => serde_json::json!({
            "ok": true,
            "mode": "ctox_core",
            "flow": flow
        }),
        Err(err) => serde_json::json!({
            "ok": false,
            "mode": "ctox_core",
            "error": err.to_string()
        }),
    }
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().to_owned())
}

fn handle_login_request(root: &Path, mut request: Request) -> anyhow::Result<()> {
    // Fetch-based logins ask for JSON so the login gate can show an inline error
    // without a full-page reload that flashes the workspace startup loader.
    let wants_json = header_value(&request, "Accept")
        .map(|accept| accept.contains("application/json"))
        .unwrap_or(false);
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let form = Url::parse(&format!("http://localhost/login?{body}"))
        .ok()
        .map(|url| url.query_pairs().into_owned().collect::<HashMap<_, _>>())
        .unwrap_or_default();
    let user = form.get("user").map(String::as_str).unwrap_or("").trim();
    let password = form.get("password").map(String::as_str).unwrap_or("");
    let credentials = format!("{user}:{password}");
    let auth_header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes())
    );
    let session = store::session_for_request(
        Some(&auth_header),
        None,
        request_allows_local_dev_session(&request),
    );
    let session = store::session_with_persisted_user(root, session)?;
    if session.authenticated {
        store::remember_authenticated_session_user(root, &session)?;
        // Opaque server-side session: the cookie is a random token mapped to a
        // server-side record (actor/role/expiry/revoked), never the user's
        // credentials. This makes it non-reversible and revocable at logout.
        let cookie = match session.user.as_ref() {
            Some(user) => store::create_business_session(
                root,
                &user.id,
                &user.role,
                &user.display_name,
                store::BUSINESS_SESSION_TTL_SECS,
            )?,
            None => String::new(),
        };
        if wants_json {
            respond_login_json(request, true, &cookie, 60 * 60 * 24 * 30)
        } else {
            respond_redirect_with_cookie(request, "/", &cookie, 60 * 60 * 24 * 30)
        }
    } else if wants_json {
        respond_login_json(request, false, "", 0)
    } else {
        respond_redirect_with_cookie(request, "/login?loginFailed=1", "", 0)
    }
}

fn respond_login_json(
    request: Request,
    authenticated: bool,
    cookie_value: &str,
    max_age_secs: u64,
) -> anyhow::Result<()> {
    let body = serde_json::to_string(&serde_json::json!({ "authenticated": authenticated }))?;
    let status = if authenticated { 200 } else { 401 };
    let mut response = Response::from_string(body).with_status_code(status);
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    let secure = if cookie_should_be_secure(&request) {
        "; Secure"
    } else {
        ""
    };
    let cookie = if cookie_value.is_empty() {
        format!("ctox_business_os_auth=; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age=0")
    } else {
        format!(
            "ctox_business_os_auth={cookie_value}; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age={max_age_secs}"
        )
    };
    response.add_header(Header::from_bytes("Set-Cookie", cookie.as_bytes()).unwrap());
    add_cors_headers(&mut response);
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

/// Extract the raw opaque session token from the `ctox_business_os_auth` cookie.
/// The value is an opaque server-side token (resolved by
/// `store::session_from_cookie_token`), not credentials.
fn business_os_session_cookie_value(request: &Request) -> Option<String> {
    let cookie_header = header_value(request, "Cookie")?;
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == "ctox_business_os_auth").then(|| value.trim().to_owned())
    })
}

fn rejects_cross_origin_browser_mutation(
    method: &Method,
    path: &str,
    session_cookie: Option<&str>,
    origin: Option<&str>,
    host: Option<&str>,
    forwarded_proto: Option<&str>,
) -> bool {
    let mutation = matches!(
        method,
        &Method::Post | &Method::Put | &Method::Patch | &Method::Delete
    );
    if !mutation || (path != "/login" && session_cookie.is_none()) {
        return false;
    }
    let Some(origin) = origin.map(str::trim) else {
        return false;
    };
    if origin.eq_ignore_ascii_case("null") {
        return true;
    }
    let Some(host) = host.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let scheme = forwarded_proto
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| value.eq_ignore_ascii_case("http") || value.eq_ignore_ascii_case("https"))
        .unwrap_or("http");
    let Ok(target) = Url::parse(&format!("{scheme}://{host}")) else {
        return true;
    };
    let Ok(source) = Url::parse(origin) else {
        return true;
    };
    !(source.scheme().eq_ignore_ascii_case(target.scheme())
        && source.host_str().map(str::to_ascii_lowercase)
            == target.host_str().map(str::to_ascii_lowercase)
        && source.port_or_known_default() == target.port_or_known_default())
}

/// Decide whether the session cookie must carry the `Secure` attribute. It is
/// set whenever the request is not plain loopback HTTP — i.e. it arrived over
/// HTTPS (directly or via a terminating proxy that sets `X-Forwarded-Proto`) or
/// from a non-loopback peer (a public bind, which must be fronted by TLS). Plain
/// loopback HTTP dev keeps it off so the cookie is still delivered.
fn cookie_should_be_secure(request: &Request) -> bool {
    let forwarded_https = header_value(request, "X-Forwarded-Proto")
        .map(|proto| {
            proto
                .split(',')
                .next()
                .map(|first| first.trim().eq_ignore_ascii_case("https"))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if forwarded_https {
        return true;
    }
    request
        .remote_addr()
        .map(|addr| !addr.ip().is_loopback())
        .unwrap_or(false)
}

fn respond_redirect_with_cookie(
    request: Request,
    location: &str,
    cookie_value: &str,
    max_age_secs: u64,
) -> anyhow::Result<()> {
    let mut response = Response::empty(303);
    response.add_header(Header::from_bytes("Location", location.as_bytes()).unwrap());
    let secure = if cookie_should_be_secure(&request) {
        "; Secure"
    } else {
        ""
    };
    let cookie = if cookie_value.is_empty() {
        format!("ctox_business_os_auth=; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age=0")
    } else {
        format!(
            "ctox_business_os_auth={cookie_value}; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age={max_age_secs}"
        )
    };
    response.add_header(Header::from_bytes("Set-Cookie", cookie.as_bytes()).unwrap());
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

fn load_module_manifests(
    source_app_root: &Path,
    installed_app_root: &Path,
) -> anyhow::Result<Vec<ModuleManifest>> {
    let app_root = source_app_root;
    let modules_root = app_root.join("modules");
    let mut manifests = Vec::new();
    if modules_root.is_dir() {
        for entry in fs::read_dir(&modules_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("module.json");
            if !path.is_file() {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read module manifest {}", path.display()))?;
            let mut manifest: ModuleManifest = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
            manifest.manifest_sha256 = hex_sha256(text.as_bytes());
            manifest.local_manifest_path = path.display().to_string();
            backfill_local_module_icon(&mut manifest, &entry.path());
            if manifest.entry.is_empty() {
                manifest.entry = format!("modules/{}/index.html", manifest.id);
            }
            let declared_scope = manifest.install_scope.trim().to_ascii_lowercase();
            let scope = match (store::is_core_module(&manifest.id), declared_scope.as_str()) {
                (true, _) => "core",
                (false, "internal") => "internal",
                // `starter` is retired; old manifests remain marketplace apps.
                _ => continue,
            };
            let core = scope == "core";
            manifest.install_scope = scope.to_owned();
            manifest.default_installed = true;
            manifest.source = scope.to_owned();
            manifest.core = core;
            manifest.editable = true;
            manifest.deletable = !core;
            manifests.push(manifest);
        }
    }
    for manifest in load_installed_module_manifests(installed_app_root)? {
        if manifests.iter().any(|existing| existing.id == manifest.id) {
            continue;
        }
        manifests.push(manifest);
    }
    for manifest in load_local_module_manifests(installed_app_root)? {
        if manifests.iter().any(|existing| existing.id == manifest.id) {
            continue;
        }
        manifests.push(manifest);
    }
    manifests.sort_by(|a, b| match (a.id.as_str(), b.id.as_str()) {
        ("ctox", "ctox") => std::cmp::Ordering::Equal,
        ("ctox", _) => std::cmp::Ordering::Less,
        (_, "ctox") => std::cmp::Ordering::Greater,
        _ => a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)),
    });
    Ok(manifests)
}

fn load_installed_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("installed-modules");
    let mut manifests = Vec::new();
    if !modules_root.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let mut manifest: ModuleManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        manifest.manifest_sha256 = hex_sha256(text.as_bytes());
        manifest.local_manifest_path = path.display().to_string();
        backfill_local_module_icon(&mut manifest, &entry.path());
        if store::is_core_module(&manifest.id) {
            continue;
        }
        if manifest.entry.is_empty() {
            manifest.entry = format!("installed-modules/{}/index.html", manifest.id);
        }
        manifest.source = "installed".to_owned();
        manifest.install_scope = "installed".to_owned();
        manifest.default_installed = false;
        manifest.core = false;
        manifest.editable = true;
        manifest.deletable = true;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn load_local_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("local-modules");
    let mut manifests = Vec::new();
    if !modules_root.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let mut manifest: ModuleManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        manifest.manifest_sha256 = hex_sha256(text.as_bytes());
        manifest.local_manifest_path = path.display().to_string();
        backfill_local_module_icon(&mut manifest, &entry.path());
        if store::is_core_module(&manifest.id) {
            continue;
        }
        manifest.entry = format!("local-modules/{}/index.html", manifest.id);
        manifest.source = "local".to_owned();
        manifest.install_scope = "local".to_owned();
        manifest.default_installed = false;
        manifest.core = false;
        manifest.editable = true;
        manifest.deletable = false;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn backfill_local_module_icon(manifest: &mut ModuleManifest, module_dir: &Path) {
    if manifest.icon.trim().is_empty()
        && manifest.icon_path.trim().is_empty()
        && module_dir.join("icon.svg").is_file()
    {
        manifest.icon = "icon.svg".to_owned();
    }
}

fn ensure_local_icon_manifest_value(manifest: &mut Value, module_dir: &Path) {
    let existing_icon = manifest
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if existing_icon.is_empty() && module_dir.join("icon.svg").is_file() {
        manifest["icon"] = Value::String("icon.svg".to_owned());
    }
}

fn load_template_manifests(app_root: &Path) -> anyhow::Result<Vec<TemplateManifest>> {
    let templates_root = app_root.join("template-store");
    let mut templates = Vec::new();
    if !templates_root.is_dir() {
        return Ok(templates);
    }
    for entry in fs::read_dir(&templates_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("template.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read template manifest {}", path.display()))?;
        let template: TemplateManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse template manifest {}", path.display()))?;
        templates.push(template);
    }
    templates.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)));
    Ok(templates)
}

fn module_layout_path(root: &Path) -> PathBuf {
    root.join("runtime").join("business-os-module-layout.json")
}

fn load_module_layout(root: &Path) -> anyhow::Result<Value> {
    let path = module_layout_path(root);
    if !path.is_file() {
        return Ok(serde_json::json!({
            "ok": true,
            "version": 1,
            "labels": {},
            "ungrouped": [],
            "groups": []
        }));
    }
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("failed to read module layout {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse module layout {}", path.display()))?;
    if let Value::Object(map) = &mut value {
        map.insert("ok".to_owned(), Value::Bool(true));
    }
    Ok(value)
}

fn save_module_layout(root: &Path, layout: &Value) -> anyhow::Result<()> {
    let path = module_layout_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut clean = layout.clone();
    if let Value::Object(map) = &mut clean {
        map.remove("ok");
    }
    fs::write(&path, serde_json::to_vec_pretty(&clean)?)
        .with_context(|| format!("failed to write module layout {}", path.display()))?;
    Ok(())
}

fn install_template_module(
    source_app_root: &Path,
    installed_app_root: &Path,
    request: InstallTemplateRequest,
) -> anyhow::Result<ModuleManifest> {
    let template_id = sanitize_slug(&request.template_id);
    if template_id.is_empty() {
        anyhow::bail!("template_id is required");
    }
    let template_path = source_app_root
        .join("template-store")
        .join(&template_id)
        .join("template.json");
    let text = fs::read_to_string(&template_path).with_context(|| {
        format!(
            "failed to read template manifest {}",
            template_path.display()
        )
    })?;
    let template: TemplateManifest = serde_json::from_str(&text).with_context(|| {
        format!(
            "failed to parse template manifest {}",
            template_path.display()
        )
    })?;
    let source_module = sanitize_slug(if template.source_module.is_empty() {
        &template.id
    } else {
        &template.source_module
    });
    let source = source_app_root.join("modules").join(&source_module);
    if !source.join("module.json").is_file() {
        anyhow::bail!("template source module `{source_module}` is missing");
    }
    let requested_id = sanitize_slug(if request.module_id.trim().is_empty() {
        if request.title.trim().is_empty() {
            &template.id
        } else {
            &request.title
        }
    } else {
        &request.module_id
    });
    let module_id = unique_module_id(installed_app_root, &requested_id);
    let module_title = if request.title.trim().is_empty() {
        if template.default_title.trim().is_empty() {
            template.title.clone()
        } else {
            template.default_title.clone()
        }
    } else {
        request.title.trim().to_owned()
    };
    let target = installed_app_root
        .join("installed-modules")
        .join(&module_id);
    copy_dir_recursive(&source, &target)?;

    let manifest_path = target.join("module.json");
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(module_title);
    manifest_value["entry"] = Value::String(format!("installed-modules/{module_id}/index.html"));
    manifest_value["install_scope"] = Value::String("installed".to_owned());
    manifest_value["default_installed"] = Value::Bool(false);
    manifest_value["template_id"] = Value::String(template.id);
    ensure_local_icon_manifest_value(&mut manifest_value, &target);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = "installed".to_owned();
    manifest.core = false;
    manifest.editable = true;
    manifest.deletable = true;
    Ok(manifest)
}

fn upsert_module_manifest(
    source_app_root: &Path,
    installed_app_root: &Path,
    request: UpsertModuleRequest,
) -> anyhow::Result<ModuleManifest> {
    let module_id = sanitize_slug(&request.id);
    if module_id.is_empty() {
        anyhow::bail!("module id is required");
    }
    let title = request.title.trim();
    if title.is_empty() {
        anyhow::bail!("module title is required");
    }
    let is_core = store::is_core_module(&module_id);
    let target = if is_core {
        source_app_root.join("modules").join(&module_id)
    } else {
        installed_app_root
            .join("installed-modules")
            .join(&module_id)
    };
    let manifest_path = target.join("module.json");
    if !manifest_path.is_file() {
        anyhow::bail!(
            "module `{module_id}` does not exist. Create new Business OS apps through the App Creator (`ctox.business_os.app.create`) or install a shipped template; `ctox.module.save` only updates existing module manifests."
        );
    }
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(title.to_owned());
    manifest_value["description"] = Value::String(request.description.trim().to_owned());
    let entry = if is_core {
        format!("modules/{module_id}/index.html")
    } else if request.entry.trim().is_empty() {
        format!("installed-modules/{module_id}/index.html")
    } else {
        request.entry.trim().to_owned()
    };
    manifest_value["entry"] = Value::String(entry);
    manifest_value["collections"] = Value::Array(
        request
            .collections
            .into_iter()
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect(),
    );
    if !request.layout.is_null() {
        manifest_value["layout"] = request.layout;
    }
    if !is_core {
        ensure_local_icon_manifest_value(&mut manifest_value, &target);
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = if is_core { "core" } else { "installed" }.to_owned();
    manifest.core = is_core;
    manifest.editable = true;
    manifest.deletable = !is_core;
    Ok(manifest)
}

fn delete_installed_module(
    app_root: &Path,
    root: &Path,
    request: DeleteModuleRequest,
) -> anyhow::Result<()> {
    let module_id = sanitize_slug(&request.module_id);
    if module_id.is_empty() {
        anyhow::bail!("module id is required");
    }
    if store::is_core_module(&module_id) {
        anyhow::bail!("core modules cannot be deleted");
    }
    let target = app_root.join("installed-modules").join(&module_id);
    if !target.is_dir() {
        anyhow::bail!("installed module not found: {module_id}");
    }
    fs::remove_dir_all(&target)
        .with_context(|| format!("failed to delete module dir {}", target.display()))?;
    let mut layout = load_module_layout(root)?;
    remove_module_from_layout_value(&mut layout, &module_id);
    save_module_layout(root, &layout)?;
    Ok(())
}

fn remove_module_from_layout_value(layout: &mut Value, module_id: &str) {
    let Some(map) = layout.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(items)) = map.get_mut("ungrouped") {
        items.retain(|item| item.as_str() != Some(module_id));
    }
    if let Some(Value::Array(groups)) = map.get_mut("groups") {
        for group in groups {
            if let Some(Value::Array(items)) = group.get_mut("items") {
                items.retain(|item| item.as_str() != Some(module_id));
            }
        }
    }
    if let Some(Value::Object(labels)) = map.get_mut("labels") {
        labels.remove(module_id);
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unique_module_id(app_root: &Path, requested_id: &str) -> String {
    let base = if requested_id.is_empty() {
        "module".to_owned()
    } else if is_core_module(requested_id) {
        format!("{requested_id}-copy")
    } else {
        requested_id.to_owned()
    };
    let installed_root = app_root.join("installed-modules");
    if !installed_root.join(&base).exists() {
        return base;
    }
    for index in 2..1000 {
        let candidate = format!("{base}-{index}");
        if !installed_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", uuid::Uuid::new_v4())
}

fn is_core_module(id: &str) -> bool {
    CORE_MODULE_IDS.iter().any(|core| id == *core)
}

fn sanitize_slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn copy_dir_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        anyhow::bail!("target module already exists: {}", target.display());
    }
    fs::create_dir_all(target)
        .with_context(|| format!("failed to create module dir {}", target.display()))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

pub(super) fn knowledge_index_payload(root: &Path) -> anyhow::Result<Value> {
    let mut items = Vec::new();
    let mut runbooks = Vec::new();
    let mut tables = Vec::new();
    let mut catalog_parquet_paths = HashSet::new();
    let sqlite_path = ctox_sqlite_path(root);

    if sqlite_path.is_file() {
        let conn = open_ctox_sqlite(root)?;

        if sqlite_table_exists(&conn, "ctox_skill_bundles")? {
            let skill_sql = if sqlite_table_exists(&conn, "ctox_skill_files")? {
                "SELECT b.skill_id, b.skill_name, b.class, b.state, b.description, b.source_path,
                        b.cluster, b.updated_at, COALESCE(f.file_count, 0) AS file_count
                   FROM ctox_skill_bundles b
                   LEFT JOIN (
                     SELECT skill_id, COUNT(*) AS file_count FROM ctox_skill_files GROUP BY skill_id
                   ) f ON f.skill_id = b.skill_id
                  ORDER BY b.updated_at DESC, b.skill_name
                  LIMIT 240"
            } else {
                "SELECT b.skill_id, b.skill_name, b.class, b.state, b.description, b.source_path,
                        b.cluster, b.updated_at, 0 AS file_count
                   FROM ctox_skill_bundles b
                  ORDER BY b.updated_at DESC, b.skill_name
                  LIMIT 240"
            };
            let mut stmt = conn.prepare(skill_sql)?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let skill_id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let class_name: String = row.get(2)?;
                let state: String = row.get(3)?;
                let summary: String = row.get(4)?;
                let source_path: Option<String> = row.get(5)?;
                let cluster: String = row.get(6)?;
                let updated_at: String = row.get(7)?;
                let file_count: i64 = row.get(8)?;
                items.push(serde_json::json!({
                    "id": format!("skill:{skill_id}"),
                    "kind": "skill",
                    "title": title,
                    "subtitle": format!("{class_name} · {state} · {cluster}"),
                    "summary": summary,
                    "source_path": source_path,
                    "updated_at": updated_at,
                    "file_count": file_count,
                    "has_table": false
                }));
            }
        }

        if sqlite_table_exists(&conn, "knowledge_skillbooks")? {
            let mut stmt = conn.prepare(
                "SELECT skillbook_id, title, status, summary, linked_runbooks_json, updated_at
                   FROM knowledge_skillbooks
                  WHERE status = 'active'
                  ORDER BY updated_at DESC, title
                  LIMIT 160",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let status: String = row.get(2)?;
                let summary: String = row.get(3)?;
                let linked_runbooks_json: String = row.get(4)?;
                let updated_at: String = row.get(5)?;
                let linked_runbook_ids = serde_json::from_str::<Value>(&linked_runbooks_json)
                    .unwrap_or_else(|_| serde_json::json!([]));
                items.push(serde_json::json!({
                    "id": format!("skillbook:{id}"),
                    "kind": "skillbook",
                    "title": title,
                    "subtitle": format!("Skillbook · {status}"),
                    "summary": summary,
                    "linked_runbook_ids": linked_runbook_ids,
                    "linked_runbooks_json": linked_runbooks_json,
                    "updated_at": updated_at,
                    "file_count": 1,
                    "has_table": false
                }));
            }
        }

        if sqlite_table_exists(&conn, "knowledge_runbooks")? {
            let mut stmt = conn.prepare(
                "SELECT runbook_id, skillbook_id, title, status, summary, problem_domain, updated_at
                   FROM knowledge_runbooks
                  WHERE status = 'active'
                  ORDER BY updated_at DESC, title
                  LIMIT 220",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let skillbook_id: String = row.get(1)?;
                let title: String = row.get(2)?;
                let status: String = row.get(3)?;
                let summary: String = row.get(4)?;
                let domain: String = row.get(5)?;
                let updated_at: String = row.get(6)?;
                let runbook = serde_json::json!({
                    "id": format!("runbook:{id}"),
                    "kind": "runbook",
                    "runbook_id": id,
                    "skillbook_id": skillbook_id,
                    "title": title,
                    "status": status,
                    "summary": summary,
                    "problem_domain": domain,
                    "updated_at": updated_at
                });
                items.push(serde_json::json!({
                    "id": runbook["id"],
                    "kind": "runbook",
                    "runbook_id": runbook["runbook_id"],
                    "skillbook_id": runbook["skillbook_id"],
                    "title": runbook["title"],
                    "subtitle": format!("Runbook · {} · {}", runbook["status"].as_str().unwrap_or(""), runbook["problem_domain"].as_str().unwrap_or("")),
                    "summary": runbook["summary"],
                    "problem_domain": runbook["problem_domain"],
                    "updated_at": runbook["updated_at"],
                    "file_count": 1,
                    "has_table": false
                }));
                runbooks.push(runbook);
            }
        }

        if sqlite_table_exists(&conn, "knowledge_resources")? {
            let mut stmt = conn.prepare(
                "SELECT resource_id, skillbook_id, title, kind, role, canonical_url,
                        evidence_eligible, updated_at
                   FROM knowledge_resources
                  ORDER BY updated_at DESC, title
                  LIMIT 320",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let skillbook_id: String = row.get(1)?;
                let title: String = row.get(2)?;
                let kind: String = row.get(3)?;
                let role: String = row.get(4)?;
                let canonical_url: String = row.get(5)?;
                let evidence_eligible: bool = row.get(6)?;
                let updated_at: String = row.get(7)?;
                items.push(serde_json::json!({
                    "id": format!("resource:{id}"),
                    "resource_id": id,
                    "skillbook_id": skillbook_id,
                    "kind": "resource",
                    "resource_kind": kind,
                    "title": title,
                    "subtitle": format!("Resource · {role}"),
                    "summary": canonical_url,
                    "canonical_url": canonical_url,
                    "evidence_eligible": evidence_eligible,
                    "updated_at": updated_at,
                    "file_count": 1,
                    "has_table": false
                }));
            }
        }

        if sqlite_table_exists(&conn, "knowledge_data_tables")? {
            let mut catalog_paths =
                conn.prepare("SELECT parquet_path FROM knowledge_data_tables")?;
            let mut path_rows = catalog_paths.query([])?;
            while let Some(row) = path_rows.next()? {
                let parquet_path: String = row.get(0)?;
                catalog_parquet_paths.insert(parquet_path);
            }
            let mut stmt = conn.prepare(
                "SELECT table_id, domain, table_key, source_system, title, description, parquet_path,
                        row_count, bytes, updated_at
                   FROM knowledge_data_tables
                  WHERE archived_at IS NULL
                  ORDER BY updated_at DESC, title
                  LIMIT 160",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let table_id: String = row.get(0)?;
                let domain: String = row.get(1)?;
                let table_key: String = row.get(2)?;
                let source_system: String = row.get(3)?;
                let title: String = row.get(4)?;
                let description: String = row.get(5)?;
                let parquet_path: String = row.get(6)?;
                let row_count: i64 = row.get(7)?;
                let bytes: i64 = row.get(8)?;
                let updated_at: String = row.get(9)?;
                let id = format!("table:{table_id}");
                let table = serde_json::json!({
                    "id": id,
                    "kind": "dataframe",
                    "title": title,
                    "domain": domain,
                    "table_key": table_key,
                    "source_system": source_system,
                    "description": description,
                    "parquet_path": parquet_path,
                    "row_count": row_count,
                    "bytes": bytes,
                    "updated_at": updated_at
                });
                items.push(serde_json::json!({
                    "id": table["id"],
                    "kind": "dataframe",
                    "title": table["title"],
                    "subtitle": format!("{} · {} rows", table["domain"].as_str().unwrap_or("data"), row_count),
                    "summary": table["description"],
                    "updated_at": table["updated_at"],
                    "file_count": 1,
                    "has_table": true
                }));
                tables.push(table);
            }
        }
    }

    for table in scan_runtime_parquet_tables(root)? {
        let id = table["id"].as_str().unwrap_or_default().to_owned();
        let parquet_path = table["parquet_path"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if catalog_parquet_paths.contains(&parquet_path) {
            continue;
        }
        if tables.iter().any(|existing| {
            existing["id"].as_str() == Some(id.as_str())
                || existing["parquet_path"].as_str() == Some(parquet_path.as_str())
        }) {
            continue;
        }
        items.push(serde_json::json!({
            "id": table["id"],
            "kind": "dataframe",
            "title": table["title"],
            "subtitle": table["subtitle"],
            "summary": table["description"],
            "updated_at": table["updated_at"],
            "file_count": 1,
            "has_table": true
        }));
        tables.push(table);
    }

    if runbooks.is_empty() {
        let runbook = serde_json::json!({
            "id": "runbook:knowledge-runtime-maintenance",
            "kind": "runbook",
            "runbook_id": "knowledge-runtime-maintenance",
            "skillbook_id": "native-business-os-knowledge",
            "title": "Knowledge Runtime Maintenance",
            "status": "draft",
            "summary": "Operatives Standard-Runbook zum Prüfen, Aktualisieren und Anwenden von CTOX Knowledge.",
            "problem_domain": "knowledge",
            "updated_at": ""
        });
        items.push(serde_json::json!({
            "id": runbook["id"],
            "kind": "runbook",
            "title": runbook["title"],
            "subtitle": "Runbook · draft · knowledge",
            "summary": runbook["summary"],
            "updated_at": "",
            "file_count": 1,
            "has_table": false
        }));
        runbooks.push(runbook);
    }

    Ok(serde_json::json!({
        "ok": true,
        "source": if sqlite_path.is_file() { "ctox.sqlite3+runtime" } else { "runtime" },
        "items": items,
        "runbooks": runbooks,
        "tables": tables,
        "counts": {
            "items": items.len(),
            "runbooks": runbooks.len(),
            "tables": tables.len()
        }
    }))
}

fn knowledge_document_payload(root: &Path, id: &str) -> anyhow::Result<Value> {
    let markdown = if let Some(skill_id) = id.strip_prefix("skill:") {
        skill_markdown(root, skill_id)?
    } else if let Some(skillbook_id) = id.strip_prefix("skillbook:") {
        skillbook_markdown(root, skillbook_id)?
    } else if let Some(runbook_id) = id.strip_prefix("runbook:") {
        runbook_markdown(root, runbook_id)?
    } else if let Some(resource_id) = id.strip_prefix("resource:") {
        knowledge_resource_markdown(root, resource_id)?
    } else if id.starts_with("table:") || id.starts_with("parquet:") {
        let table = resolve_parquet_table(root, id)?;
        format!(
            "# {}\n\n{}\n\n- Quelle: `{}`\n- Zeilen: {}\n- Bytes: {}\n\nDie Tabellenansicht lädt diese Daten windowed aus der CTOX-Polars-Schicht.",
            table.title,
            table.description,
            table.path.display(),
            table
                .row_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unbekannt".to_owned()),
            table
                .bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unbekannt".to_owned())
        )
    } else {
        "# Knowledge\n\nKein Knowledge-Eintrag ausgewählt.".to_owned()
    };
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "markdown": markdown
    }))
}

fn skill_markdown(root: &Path, skill_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut bundle = conn.prepare(
        "SELECT skill_name, class, state, description, source_path, cluster, updated_at
           FROM ctox_skill_bundles WHERE skill_id = ?1",
    )?;
    let (name, class_name, state, description, source_path, cluster, updated_at): (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ) = bundle.query_row([skill_id], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        ))
    })?;
    let mut text = format!(
        "# {name}\n\n{description}\n\n- Klasse: `{class_name}`\n- Status: `{state}`\n- Cluster: `{cluster}`\n- Quelle: `{}`\n- Aktualisiert: `{updated_at}`\n",
        source_path.unwrap_or_else(|| "unbekannt".to_owned())
    );
    let mut files = conn.prepare(
        "SELECT relative_path, substr(CAST(content_blob AS TEXT), 1, 120000) AS content_text
           FROM ctox_skill_files
          WHERE skill_id = ?1
          ORDER BY CASE WHEN relative_path = 'SKILL.md' THEN 0 ELSE 1 END, relative_path
          LIMIT 24",
    )?;
    let mut rows = files.query([skill_id])?;
    while let Some(row) = rows.next()? {
        let relative_path: String = row.get(0)?;
        let content: String = row.get(1)?;
        text.push_str(&format!("\n\n## {relative_path}\n\n{content}"));
    }
    Ok(text)
}

fn skillbook_markdown(root: &Path, skillbook_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut stmt = conn.prepare(
        "SELECT title, version, status, summary, mission, runtime_policy, answer_contract,
                workflow_backbone_json, routing_taxonomy_json, linked_runbooks_json, updated_at
           FROM knowledge_skillbooks WHERE skillbook_id = ?1",
    )?;
    let row = stmt.query_row([skillbook_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    Ok(format!(
        "# {}\n\n{}\n\n- Version: `{}`\n- Status: `{}`\n- Aktualisiert: `{}`\n\n## Mission\n\n{}\n\n## Runtime Policy\n\n{}\n\n## Answer Contract\n\n{}\n\n## Workflow Backbone\n\n```json\n{}\n```\n\n## Routing Taxonomy\n\n```json\n{}\n```\n\n## Linked Runbooks\n\n```json\n{}\n```",
        row.0, row.3, row.1, row.2, row.10, row.4, row.5, row.6, row.7, row.8, row.9
    ))
}

fn runbook_markdown(root: &Path, runbook_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut stmt = conn.prepare(
        "SELECT title, version, status, summary, problem_domain, item_labels_json, updated_at
           FROM knowledge_runbooks WHERE runbook_id = ?1",
    )?;
    let row = stmt.query_row([runbook_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut text = format!(
        "# {}\n\n{}\n\n- Version: `{}`\n- Status: `{}`\n- Domain: `{}`\n- Labels: `{}`\n- Aktualisiert: `{}`\n",
        row.0, row.3, row.1, row.2, row.4, row.5, row.6
    );
    let mut items = conn.prepare(
        "SELECT label, title, problem_class, chunk_text, structured_json, status, version
           FROM knowledge_runbook_items
          WHERE runbook_id = ?1
          ORDER BY label, updated_at DESC
          LIMIT 120",
    )?;
    let mut rows = items.query([runbook_id])?;
    while let Some(item) = rows.next()? {
        let label: String = item.get(0)?;
        let title: String = item.get(1)?;
        let problem_class: String = item.get(2)?;
        let chunk_text: String = item.get(3)?;
        let structured_json: String = item.get(4)?;
        let status: String = item.get(5)?;
        let version: String = item.get(6)?;
        text.push_str(&format!(
            "\n\n## {label} · {title}\n\n- Problemklasse: `{problem_class}`\n- Status: `{status}`\n- Version: `{version}`\n\n{chunk_text}\n\n```json\n{structured_json}\n```"
        ));
    }
    Ok(text)
}

fn knowledge_resource_markdown(root: &Path, resource_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut stmt = conn.prepare(
        "SELECT title, kind, role, canonical_url, snapshot_hash, evidence_eligible,
                linked_runbook_items_json, metadata_json, updated_at
           FROM knowledge_resources WHERE resource_id = ?1",
    )?;
    let row = stmt.query_row([resource_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    Ok(format!(
        "# {}\n\n- Art: `{}`\n- Rolle: `{}`\n- Evidenzfähig: `{}`\n- Quelle: {}\n- Snapshot SHA-256: `{}`\n- Verknüpfte Runbook-Items: `{}`\n- Aktualisiert: `{}`\n\n## Receipt\n\n```json\n{}\n```",
        row.0, row.1, row.2, row.5, row.3, row.4, row.6, row.8, row.7
    ))
}

#[derive(Debug, Clone)]
struct ParquetTableRef {
    title: String,
    description: String,
    path: PathBuf,
    row_count: Option<i64>,
    bytes: Option<i64>,
}

fn knowledge_dataframe_schema_payload(root: &Path, id: &str) -> anyhow::Result<Value> {
    let table = resolve_parquet_table(root, id)?;
    let mut lf = scan_parquet(&table.path)?;
    let schema = lf.collect_schema().context("collect parquet schema")?;
    let columns: Vec<Value> = schema
        .iter()
        .map(|(name, dtype)| {
            serde_json::json!({
                "name": name.to_string(),
                "dtype": format!("{dtype:?}")
            })
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "title": table.title,
        "columns": columns,
        "row_count": table.row_count,
        "bytes": table.bytes
    }))
}

fn knowledge_dataframe_rows_payload(
    root: &Path,
    id: &str,
    offset: usize,
    limit: usize,
) -> anyhow::Result<Value> {
    let table = resolve_parquet_table(root, id)?;
    let df = scan_parquet(&table.path)?
        .slice(offset as i64, limit as IdxSize)
        .collect()
        .context("collect parquet row window")?;
    let rows = dataframe_to_json_rows(&df)?;
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "offset": offset,
        "limit": limit,
        "returned": rows.len(),
        "row_count": table.row_count,
        "rows": rows
    }))
}

fn resolve_parquet_table(root: &Path, id: &str) -> anyhow::Result<ParquetTableRef> {
    if let Some(table_id) = id.strip_prefix("table:") {
        let conn = open_ctox_sqlite(root)?;
        let mut stmt = conn.prepare(
            "SELECT title, description, parquet_path, row_count, bytes
               FROM knowledge_data_tables
              WHERE table_id = ?1 AND archived_at IS NULL",
        )?;
        let table = stmt.query_row([table_id], |row| {
            Ok(ParquetTableRef {
                title: row.get(0)?,
                description: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                row_count: Some(row.get(3)?),
                bytes: Some(row.get(4)?),
            })
        })?;
        if !table.path.is_file() {
            anyhow::bail!("parquet file is missing: {}", table.path.display());
        }
        return Ok(table);
    }
    for table in scan_runtime_parquet_table_refs(root)? {
        if format!("parquet:{}", short_path_hash(&table.path)) == id {
            return Ok(table);
        }
    }
    anyhow::bail!("unknown knowledge dataframe id: {id}")
}

fn scan_runtime_parquet_tables(root: &Path) -> anyhow::Result<Vec<Value>> {
    Ok(scan_runtime_parquet_table_refs(root)?
        .into_iter()
        .map(|table| {
            serde_json::json!({
                "id": format!("parquet:{}", short_path_hash(&table.path)),
                "kind": "dataframe",
                "title": table.title,
                "subtitle": "Runtime Parquet · Polars",
                "description": table.description,
                "parquet_path": table.path.display().to_string(),
                "row_count": table.row_count,
                "bytes": table.bytes,
                "updated_at": file_modified_label(&table.path)
            })
        })
        .collect())
}

fn scan_runtime_parquet_table_refs(root: &Path) -> anyhow::Result<Vec<ParquetTableRef>> {
    let mut out = Vec::new();
    let base = root.join("runtime").join("knowledge").join("data");
    collect_parquet_files(&base, &mut out)?;
    out.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

fn collect_parquet_files(dir: &Path, out: &mut Vec<ParquetTableRef>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_parquet_files(&path, out)?;
        } else if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("parquet")
        {
            let bytes = fs::metadata(&path).map(|meta| meta.len() as i64).ok();
            let title = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("DataFrame")
                .replace(['_', '-'], " ");
            out.push(ParquetTableRef {
                title,
                description: "Record-shaped Knowledge aus runtime/knowledge/data.".to_owned(),
                path,
                row_count: None,
                bytes,
            });
        }
    }
    Ok(())
}

fn dataframe_to_json_rows(df: &DataFrame) -> anyhow::Result<Vec<Value>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }
    let mut buf = Vec::new();
    JsonWriter::new(&mut buf)
        .with_json_format(JsonFormat::JsonLines)
        .finish(&mut df.clone())
        .context("serialize DataFrame as JSON lines")?;
    let mut rows = Vec::new();
    for line in buf.split(|byte| *byte == b'\n') {
        if !line.is_empty() {
            rows.push(serde_json::from_slice(line)?);
        }
    }
    Ok(rows)
}

fn scan_parquet(path: &Path) -> PolarsResult<LazyFrame> {
    let pl_path = PlPath::new(&path.to_string_lossy());
    LazyFrame::scan_parquet(pl_path, ScanArgsParquet::default())
}

fn open_ctox_sqlite(root: &Path) -> anyhow::Result<Connection> {
    let path = ctox_sqlite_path(root);
    let conn =
        Connection::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 10000;")?;
    Ok(conn)
}

fn sqlite_table_exists(conn: &Connection, table_name: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn ctox_sqlite_path(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox.sqlite3")
}

fn parse_query(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn parse_usize_query(query: &HashMap<String, String>, key: &str, fallback: usize) -> usize {
    query
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn short_path_hash(path: &Path) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.display().to_string().as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_owned()
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn file_modified_label(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_default()
}

fn serve_static(root: &Path, app_root: &Path, request: Request, path: &str) -> anyhow::Result<()> {
    let raw_rel = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    let rel = raw_rel
        .strip_prefix("business-os/")
        .or_else(|| (raw_rel == "business-os").then_some("index.html"))
        .unwrap_or(raw_rel);
    if rel
        .split('/')
        .any(|part| part == ".." || part.starts_with('.'))
    {
        return respond_status(request, 403, "forbidden");
    }
    let file = resolve_business_os_static_file(root, app_root, rel);
    let target = if file.is_dir() {
        file.join("index.html")
    } else {
        file
    };
    let target = if !target.is_file() && should_serve_app_shell(rel) {
        app_root.join("index.html")
    } else {
        target
    };
    if !target.is_file() {
        return respond_status(request, 404, "not found");
    }
    let mut bytes = fs::read(&target)?;
    let mime = mime_for(&target);
    let is_index = target == app_root.join("index.html");
    if is_index {
        let session = request_session(root, &request);
        store::remember_authenticated_session_user(root, &session)?;
        let sync_config = if session.authenticated {
            let turn_session = session
                .user
                .as_ref()
                .map(|user| user.id.clone())
                .unwrap_or_default();
            let config = store::sync_config_for_browser(root, &turn_session)?;
            Some(launch_config_value(root, &config)?)
        } else {
            None
        };
        let html = String::from_utf8(bytes).context("Business OS index.html is not UTF-8")?;
        bytes = inject_launch_context(html, &session, sync_config.as_ref())?.into_bytes();
    }
    let cache_control = business_os_static_cache_control(is_index, &rel, request.url());
    respond_static_success(request, &bytes, mime, cache_control)?;
    Ok(())
}

fn business_os_static_cache_control(is_index: bool, rel: &str, request_url: &str) -> &'static str {
    if is_index {
        return "no-store";
    }

    // Runtime apps are replaced atomically at a stable tenant-local path.
    // Their revision query changes for a new catalog, but an already-open
    // shell can still request the previous URL. Revalidation prevents that
    // shell from mixing a cached old entry module with the new dependency
    // tree after a release.
    if rel.starts_with("installed-modules/") || rel.starts_with("local-modules/") {
        return "no-cache, must-revalidate";
    }

    // Release query keys remain useful for cache partitioning, but they are
    // not a correctness boundary. A missed manual key bump must never leave a
    // managed instance executing an old shell or module bundle after upgrade.
    // Revalidate versioned packaged assets on navigation so the server's
    // active release remains authoritative.
    if request_url.contains("?v=") || request_url.contains("&v=") {
        "no-cache, must-revalidate"
    } else {
        "public, max-age=300, stale-while-revalidate=86400"
    }
}

fn resolve_business_os_static_file(root: &Path, app_root: &Path, rel: &str) -> PathBuf {
    if rel.starts_with("installed-modules/") || rel.starts_with("local-modules/") {
        return resolve_business_os_installed_app_root(root).join(rel);
    }

    let app_file = app_root.join(rel);
    if app_file.exists() {
        return app_file;
    }

    // Imported repository notes and help surfaces can contain repository-root
    // relative references such as `docs/site/assets/...`. Release installs
    // already ship those documentation assets next to `business-os`; expose
    // only that static subtree instead of duplicating it into the shell bundle.
    // This is an asset route, never a Business OS collection/data fallback.
    if rel.starts_with("docs/") {
        let docs_file = root.join(rel);
        if docs_file.exists() {
            return docs_file;
        }
    }

    app_file
}

fn inject_launch_context(
    html: String,
    session: &store::BusinessOsSession,
    sync_config: Option<&Value>,
) -> anyhow::Result<String> {
    let html = ensure_shell_stylesheets_in_index(html);
    let script = format!(
        "<script>window.CTOX_BUSINESS_OS_SESSION={};window.CTOX_BUSINESS_OS_CONFIG={};</script>",
        script_json(session)?,
        sync_config
            .map(script_json)
            .transpose()?
            .unwrap_or_else(|| "null".to_owned())
    );
    if let Some(idx) = html.find("</head>") {
        let mut injected = String::with_capacity(html.len() + script.len());
        injected.push_str(&html[..idx]);
        injected.push_str(&script);
        injected.push_str(&html[idx..]);
        Ok(injected)
    } else {
        Ok(format!("{script}{html}"))
    }
}

fn launch_config_value(
    root: &Path,
    sync_config: &store::BusinessOsSyncConfig,
) -> anyhow::Result<Value> {
    let mut value = serde_json::to_value(sync_config)?;
    if let Ok(catalog) = store::module_catalog_for_rxdb(root) {
        if let Some(object) = value.as_object_mut() {
            object.insert("module_catalog_snapshot".to_owned(), catalog);
            object.insert(
                "module_catalog_snapshot_source".to_owned(),
                Value::String("native-launch-config".to_owned()),
            );
        }
    }
    Ok(value)
}

fn ensure_shell_stylesheets_in_index(html: String) -> String {
    let mut required = Vec::new();
    if !html.contains("app.css") {
        required.push(r#"<link rel="stylesheet" href="app.css?v=20260623-shell-icons" />"#);
    }
    if !html.contains("shared/base.css") {
        required.push(r#"<link rel="stylesheet" href="shared/base.css?v=20260609-base1" />"#);
    }
    if required.is_empty() {
        return html;
    }
    let styles = format!("\n    {}\n", required.join("\n    "));
    if let Some(idx) = html.find("</head>") {
        let mut injected = String::with_capacity(html.len() + styles.len());
        injected.push_str(&html[..idx]);
        injected.push_str(&styles);
        injected.push_str(&html[idx..]);
        injected
    } else {
        format!("{styles}{html}")
    }
}

fn script_json<T: Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?.replace("</", "<\\/"))
}

fn should_serve_app_shell(rel: &str) -> bool {
    if rel.starts_with("api/") || rel.contains('.') {
        return false;
    }
    matches!(rel, "app" | "login" | "settings") || rel.starts_with("app/")
}

fn read_json(request: &mut Request) -> anyhow::Result<Value> {
    let mut text = String::new();
    request.as_reader().read_to_string(&mut text)?;
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&text).context("invalid JSON request body")
}

fn respond_json<T: Serialize>(request: Request, value: &T) -> anyhow::Result<()> {
    respond_json_value(request, serde_json::to_value(value)?)
}

fn respond_json_value(request: Request, value: Value) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(&value)?;
    let mut response = Response::from_string(body);
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    add_cors_headers(&mut response);
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

fn respond_json_value_no_store(request: Request, value: Value) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(&value)?;
    let mut response = Response::from_string(body);
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    response.add_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
    add_cors_headers(&mut response);
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

fn respond_status(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let mut response = Response::from_string(body.to_string()).with_status_code(status);
    add_cors_headers(&mut response);
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

fn respond_options(request: Request) -> anyhow::Result<()> {
    let mut response = Response::empty(204);
    add_cors_headers(&mut response);
    response.add_header(
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
    );
    response.add_header(
        Header::from_bytes(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization, X-CTOX-Business-OS-Session",
        )
        .unwrap(),
    );
    response.add_header(Header::from_bytes("Access-Control-Max-Age", "600").unwrap());
    add_common_response_headers(&mut response);
    request.respond(response)?;
    Ok(())
}

fn add_cors_headers<R: io::Read>(response: &mut Response<R>) {
    response.add_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap());
    response.add_header(Header::from_bytes("Vary", "Origin").unwrap());
}

fn add_common_response_headers<R: io::Read>(response: &mut Response<R>) {
    let _ = response;
    // tiny_http filters hop-by-hop response headers such as Connection here.
    // Static Business OS assets use respond_static_success() so Chromium does
    // not leave the ES-module graph stuck on a kept-alive loopback connection.
}

fn respond_static_success(
    request: Request,
    bytes: &[u8],
    content_type: &str,
    cache_control: &str,
) -> anyhow::Result<()> {
    let mut writer = request.into_writer();
    match write_static_success_response(&mut writer, bytes, content_type, cache_control) {
        Ok(()) => Ok(()),
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionAborted
            ) =>
        {
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn write_static_success_response<W: Write>(
    mut writer: W,
    bytes: &[u8],
    content_type: &str,
    cache_control: &str,
) -> io::Result<()> {
    write!(writer, "HTTP/1.1 200 OK\r\n")?;
    write!(writer, "Content-Type: {content_type}\r\n")?;
    write!(writer, "Cache-Control: {cache_control}\r\n")?;
    write!(writer, "Content-Length: {}\r\n", bytes.len())?;
    write!(writer, "Connection: close\r\n")?;
    write!(writer, "\r\n")?;
    writer.write_all(bytes)?;
    writer.flush()
}

fn mime_for(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_module_loader_uses_system_installed_and_local_sources_only() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let source_root = temp.path().join("source");
        let runtime_root = temp.path().join("runtime");
        for (id, scope) in [
            ("ctox", "store"),
            ("research", "internal"),
            ("marketplace-research", "store"),
            ("rogue-system", "core"),
        ] {
            let dir = source_root.join("modules").join(id);
            fs::create_dir_all(&dir)?;
            fs::write(
                dir.join("module.json"),
                serde_json::to_vec(&serde_json::json!({
                    "id": id,
                    "title": id,
                    "install_scope": scope
                }))?,
            )?;
        }
        for (directory, id, scope) in [
            ("installed-modules", "public-addon", "installed"),
            ("local-modules", "private-addon", "local"),
        ] {
            let dir = runtime_root.join(directory).join(id);
            fs::create_dir_all(&dir)?;
            fs::write(
                dir.join("module.json"),
                serde_json::to_vec(&serde_json::json!({
                    "id": id,
                    "title": id,
                    "install_scope": scope
                }))?,
            )?;
        }

        let modules = load_module_manifests(&source_root, &runtime_root)?;
        let by_id = modules
            .into_iter()
            .map(|module| (module.id.clone(), module))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            by_id
                .get("ctox")
                .map(|module| module.install_scope.as_str()),
            Some("core")
        );
        assert_eq!(
            by_id
                .get("research")
                .map(|module| module.install_scope.as_str()),
            Some("internal")
        );
        assert_eq!(
            by_id
                .get("public-addon")
                .map(|module| module.install_scope.as_str()),
            Some("installed")
        );
        assert_eq!(
            by_id
                .get("private-addon")
                .map(|module| module.install_scope.as_str()),
            Some("local")
        );
        assert!(!by_id.contains_key("marketplace-research"));
        assert!(!by_id.contains_key("rogue-system"));
        Ok(())
    }

    #[test]
    fn local_dev_session_host_gate_is_loopback_only() {
        assert!(host_header_allows_local_dev_session("localhost:8765"));
        assert!(host_header_allows_local_dev_session("dev.localhost:8765"));
        assert!(host_header_allows_local_dev_session("127.0.0.1:8765"));
        assert!(host_header_allows_local_dev_session("[::1]:8765"));

        assert!(!host_header_allows_local_dev_session("ninja.ctox.dev"));
        assert!(!host_header_allows_local_dev_session("10.0.0.12:8765"));
        assert!(!host_header_allows_local_dev_session("192.168.1.10:8765"));
        assert!(!host_header_allows_local_dev_session("[2001:db8::1]:8765"));
        assert!(!host_header_allows_local_dev_session(""));
    }

    #[test]
    fn cookie_authenticated_mutations_require_same_origin() {
        let cookie = Some("opaque-session");
        assert!(!rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/api/business-os/users",
            cookie,
            Some("http://127.0.0.1:8765"),
            Some("127.0.0.1:8765"),
            None,
        ));
        assert!(rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/api/business-os/users",
            cookie,
            Some("http://127.0.0.1:18765"),
            Some("127.0.0.1:8765"),
            None,
        ));
        assert!(rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/api/business-os/users",
            cookie,
            Some("https://attacker.ctox.dev"),
            Some("tenant.ctox.dev"),
            Some("https"),
        ));
        assert!(rejects_cross_origin_browser_mutation(
            &Method::Delete,
            "/api/business-os/modules/delete",
            cookie,
            Some("null"),
            Some("tenant.ctox.dev"),
            Some("https"),
        ));

        assert!(!rejects_cross_origin_browser_mutation(
            &Method::Get,
            "/api/business-os/users",
            cookie,
            Some("https://attacker.ctox.dev"),
            Some("tenant.ctox.dev"),
            Some("https"),
        ));
        assert!(!rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/api/business-os/users",
            None,
            Some("https://attacker.ctox.dev"),
            Some("tenant.ctox.dev"),
            Some("https"),
        ));
        assert!(rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/login",
            None,
            Some("https://attacker.ctox.dev"),
            Some("tenant.ctox.dev"),
            Some("https"),
        ));
        assert!(!rejects_cross_origin_browser_mutation(
            &Method::Post,
            "/login",
            None,
            None,
            Some("tenant.ctox.dev"),
            Some("https"),
        ));
    }

    #[test]
    fn unauthenticated_shell_does_not_inject_sync_config() {
        let session = store::BusinessOsSession {
            ok: true,
            authenticated: false,
            auth_required: true,
            user: None,
            login_url: None,
            reason: Some("invalid_or_missing_session".to_owned()),
        };

        let html = inject_launch_context(
            "<html><head></head><body></body></html>".to_owned(),
            &session,
            None,
        )
        .expect("inject launch context");

        assert!(html.contains("window.CTOX_BUSINESS_OS_SESSION="));
        assert!(html.contains("window.CTOX_BUSINESS_OS_CONFIG=null"));
        assert!(html.contains(r#"href="app.css?v=20260623-shell-icons""#));
        assert!(html.contains(r#"href="shared/base.css?v=20260609-base1""#));
        assert!(!html.contains("sync_room"));
        assert!(!html.contains("signaling_room_password"));
    }

    #[test]
    fn capability_bearer_resolves_admin_api_session() {
        let root = tempfile::tempdir().expect("tempdir");
        let now = 1_789_000_000_000;
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "admin@example.com",
            "Admin User",
            "admin",
            now,
        )
        .expect("issue capability token");
        let auth_header = format!("Bearer {token}");

        let session = session_from_capability_bearer(root.path(), Some(&auth_header))
            .expect("capability bearer session");

        assert!(session.authenticated);
        assert!(store::session_can_manage_all(&session));
        let user = session.user.expect("session user");
        assert_eq!(user.id, "admin@example.com");
        assert_eq!(user.display_name, "Admin User");
        assert_eq!(user.role, "admin");
    }

    #[test]
    fn shell_stylesheet_guard_does_not_duplicate_existing_links() {
        let html = ensure_shell_stylesheets_in_index(
            r#"<html><head><link rel="stylesheet" href="app.css?v=old" /><link rel="stylesheet" href="shared/base.css?v=old" /></head><body></body></html>"#.to_owned(),
        );

        assert_eq!(html.matches("app.css").count(), 1);
        assert_eq!(html.matches("shared/base.css").count(), 1);
    }

    #[test]
    fn static_success_response_closes_http_connection() {
        let mut response = Vec::new();

        write_static_success_response(
            &mut response,
            b"console.log('ok');",
            "text/javascript; charset=utf-8",
            "public, max-age=300",
        )
        .expect("write static response");

        let raw = String::from_utf8(response).expect("utf8 response");
        assert!(raw.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(raw.contains("\r\nContent-Type: text/javascript; charset=utf-8\r\n"));
        assert!(raw.contains("\r\nCache-Control: public, max-age=300\r\n"));
        assert!(raw.contains("\r\nContent-Length: 18\r\n"));
        assert!(raw.contains("\r\nConnection: close\r\n"));
        assert!(raw.ends_with("\r\n\r\nconsole.log('ok');"));
    }

    #[test]
    fn tenant_runtime_module_assets_are_revalidated_across_releases() {
        assert_eq!(
            business_os_static_cache_control(
                false,
                "installed-modules/sellify/index.js",
                "/installed-modules/sellify/index.js?v=shell_0.4.27"
            ),
            "no-cache, must-revalidate"
        );
        assert_eq!(
            business_os_static_cache_control(
                false,
                "local-modules/thesen-outbound/core/view.js",
                "/local-modules/thesen-outbound/core/view.js?v=0.3.17"
            ),
            "no-cache, must-revalidate"
        );
        assert_eq!(
            business_os_static_cache_control(
                false,
                "modules/calendar/index.js",
                "/modules/calendar/index.js?v=shell-release"
            ),
            "no-cache, must-revalidate"
        );
    }

    #[test]
    fn business_os_static_files_can_reuse_packaged_documentation_assets() {
        let root = tempfile::tempdir().expect("tempdir");
        let app_root = root.path().join("business-os");
        let docs_asset = root.path().join("docs/site/assets/logo.png");
        std::fs::create_dir_all(docs_asset.parent().expect("asset parent"))
            .expect("create docs asset directory");
        std::fs::create_dir_all(&app_root).expect("create app root");
        std::fs::write(&docs_asset, b"png").expect("write docs asset");

        assert_eq!(
            resolve_business_os_static_file(root.path(), &app_root, "docs/site/assets/logo.png"),
            docs_asset
        );

        let app_asset = app_root.join("docs/site/assets/logo.png");
        std::fs::create_dir_all(app_asset.parent().expect("app asset parent"))
            .expect("create app asset directory");
        std::fs::write(&app_asset, b"app-png").expect("write app asset");
        assert_eq!(
            resolve_business_os_static_file(root.path(), &app_root, "docs/site/assets/logo.png"),
            app_asset
        );
    }

    #[test]
    fn static_image_assets_use_browser_image_mime_types() {
        assert_eq!(mime_for(&PathBuf::from("logo.png")), "image/png");
        assert_eq!(mime_for(&PathBuf::from("photo.jpeg")), "image/jpeg");
        assert_eq!(mime_for(&PathBuf::from("preview.webp")), "image/webp");
    }
}
