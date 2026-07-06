// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use futures_util::SinkExt;
use futures_util::StreamExt;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::policy::{
    allow_decision, normalize_role, BusinessOsPermission, BusinessOsScope, BusinessOsScopeType,
    PolicyDecision,
};
use super::store;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;
const MAX_MCP_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_RATE_LIMIT_PER_MINUTE: usize = 120;
const DEFAULT_AUDIT_RETENTION_DAYS: usize = 90;
const MCP_POLICY_PAYLOAD_KEY: &str = "business_os.mcp_policy.v1";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const APPSEC_MCP_MODULE_ID: &str = "appsec-pentest";
const DEFAULT_GATEWAY_RECONNECT_MAX_DELAY_MS: u64 = 30_000;
const DEFAULT_GATEWAY_HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const DEFAULT_GATEWAY_MAX_CONNECTION_AGE_MS: u64 = 15 * 60 * 1000;
const MAX_APP_SOURCE_WRITE_BYTES: usize = 1024 * 1024;
const MAX_APP_RUN_OUTPUT_CHARS: usize = 24_000;

#[derive(Debug, Clone)]
pub struct BusinessOsMcpServeOptions {
    pub addr: String,
}

#[derive(Debug, Clone)]
pub struct BusinessOsMcpGatewayConnectOptions {
    pub url: String,
    pub token: Option<String>,
    pub reconnect: bool,
    pub max_reconnect_delay_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub max_connection_age_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BusinessOsMcpGatewayStatusOptions {
    pub url: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpConfirmationState {
    #[serde(rename = "not_required")]
    NotRequired,
    #[serde(rename = "required")]
    Required,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "rejected")]
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpChannelRequestContext {
    pub channel: String,
    pub surface: String,
    pub actor: String,
    pub workspace: String,
    pub tool: String,
    pub request_id: String,
    pub confirmation_state: McpConfirmationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_role_source: Option<String>,
}

impl McpChannelRequestContext {
    pub fn validate(&self) -> Result<(), BusinessOsMcpError> {
        ensure_non_empty("channel", &self.channel)?;
        ensure_non_empty("surface", &self.surface)?;
        ensure_non_empty("actor", &self.actor)?;
        ensure_non_empty("workspace", &self.workspace)?;
        ensure_non_empty("tool", &self.tool)?;
        ensure_non_empty("request_id", &self.request_id)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BusinessOsMcpErrorCode {
    #[serde(rename = "not_authenticated")]
    NotAuthenticated,
    #[serde(rename = "not_authorized")]
    NotAuthorized,
    #[serde(rename = "module_not_found")]
    ModuleNotFound,
    #[serde(rename = "entity_not_found")]
    EntityNotFound,
    #[serde(rename = "record_not_found")]
    RecordNotFound,
    #[serde(rename = "action_not_allowed")]
    ActionNotAllowed,
    #[serde(rename = "confirmation_required")]
    ConfirmationRequired,
    #[serde(rename = "sync_not_ready")]
    SyncNotReady,
    #[serde(rename = "runtime_unavailable")]
    RuntimeUnavailable,
    #[serde(rename = "validation_failed")]
    ValidationFailed,
    #[serde(rename = "external_effect_blocked")]
    ExternalEffectBlocked,
    #[serde(rename = "channel_disabled")]
    ChannelDisabled,
    #[serde(rename = "permission_denied")]
    PermissionDenied,
    #[serde(rename = "response_too_large")]
    ResponseTooLarge,
    #[serde(rename = "rate_limited")]
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessOsMcpError {
    pub code: BusinessOsMcpErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl BusinessOsMcpError {
    pub fn validation(field: &str, message: impl Into<String>) -> Self {
        Self {
            code: BusinessOsMcpErrorCode::ValidationFailed,
            message: message.into(),
            field: Some(field.to_string()),
        }
    }

    pub fn not_found(code: BusinessOsMcpErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }
}

impl std::fmt::Display for BusinessOsMcpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for BusinessOsMcpError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsModuleDescriptor {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub source: String,
    pub install_scope: String,
    pub core: bool,
    pub entry: String,
    pub collections: Vec<String>,
    pub deep_link: BusinessOsDeepLink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsEntityDescriptor {
    pub module_id: String,
    pub entity_id: String,
    pub collection: String,
    pub title: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsRecordSummary {
    pub id: String,
    pub collection: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub data: Value,
    pub deep_link: BusinessOsDeepLink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpMutationResponse {
    pub ok: bool,
    pub collection: String,
    pub record_id: String,
    pub record: BusinessOsRecordSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsActionDescriptor {
    pub action_id: String,
    pub module_id: String,
    pub title: String,
    pub description: String,
    pub risk_class: String,
    pub confirmation_required: bool,
    pub external_effect: bool,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsActionProposal {
    pub ok: bool,
    pub action: BusinessOsActionDescriptor,
    pub module_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    pub command_type: String,
    pub payload: Value,
    pub client_context: Value,
    pub confirmation_required: bool,
    pub would_execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsActionExecution {
    pub ok: bool,
    pub action: BusinessOsActionDescriptor,
    pub module_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    pub command_type: String,
    pub command_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    pub confirmation_required: bool,
    pub client_context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppCommandExecution {
    pub ok: bool,
    pub module_id: String,
    pub command_type: String,
    pub command_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    pub install_target: String,
    pub app_directory: String,
    pub development_contract: BusinessOsAppDevelopmentContract,
    pub client_context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppDevelopmentContract {
    pub source_root: String,
    pub required_skill: String,
    pub skill_resources: Vec<String>,
    pub source_files: Vec<String>,
    pub source_tools: Vec<String>,
    pub reference_catalog_command: String,
    pub validation_command: String,
    pub smoke_command: String,
    pub e2e_command: String,
    pub command_status_tool: String,
    pub lifecycle: Vec<String>,
    pub data_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppSourceFileSummary {
    pub module_id: String,
    pub path: String,
    pub language: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub source_file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppSourceFile {
    pub ok: bool,
    pub module_id: String,
    pub path: String,
    pub language: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub source_file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at_ms: Option<i64>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppSourceSearchMatch {
    pub module_id: String,
    pub path: String,
    pub line: usize,
    pub preview: String,
    pub source_file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppSourceWrite {
    pub ok: bool,
    pub module_id: String,
    pub path: String,
    pub source_file_id: String,
    pub source_file_ids: Vec<String>,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    pub changed: bool,
    pub validation_tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsAppRunResult {
    pub ok: bool,
    pub module_id: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsApprovalDecision {
    pub ok: bool,
    pub decision: String,
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    pub command_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    pub client_context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsDeepLink {
    pub kind: String,
    pub path: String,
    pub url_fragment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpList<T> {
    pub ok: bool,
    pub items: Vec<T>,
    pub count: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpRecordResponse {
    pub ok: bool,
    pub record: BusinessOsRecordSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpRecordContextResponse {
    pub ok: bool,
    pub record: BusinessOsRecordSummary,
    pub activity: BusinessOsMcpList<BusinessOsRecordSummary>,
    pub commands: BusinessOsMcpList<BusinessOsRecordSummary>,
    pub runs: BusinessOsMcpList<BusinessOsRecordSummary>,
    pub approvals: BusinessOsMcpList<BusinessOsRecordSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpArtifactResponse {
    pub ok: bool,
    pub artifact: BusinessOsRecordSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpAuditEvent {
    pub event_id: String,
    pub channel: String,
    pub surface: String,
    pub actor: String,
    pub workspace: String,
    pub tool: String,
    pub request_id: String,
    pub confirmation_state: McpConfirmationState,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub metadata: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessOsMcpAuditExportFormat {
    Json,
    Jsonl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOsMcpToolDescriptor {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessOsMcpPolicy {
    pub enabled: bool,
    pub allow_reads: bool,
    pub allow_writes: bool,
    pub allow_approvals: bool,
    pub allow_external_effects: bool,
    pub rate_limit_per_minute: usize,
    pub audit_retention_days: usize,
    pub allowed_actors: Vec<String>,
    pub allowed_workspaces: Vec<String>,
    pub allowed_modules: Vec<String>,
    pub allowed_collections: Vec<String>,
    pub denied_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BusinessOsMcpPolicyPayload {
    schema_version: u8,
    policy: BusinessOsMcpPolicy,
}

const MCP_AUTH_SECRET_SCOPE: &str = "business_os";
const MCP_AUTH_SECRET_NAME: &str = "mcp_inbound_auth_token";

/// Per-instance bearer token that an inbound MCP client must present on `/mcp`.
/// Auto-generated and persisted in the CTOX secret store on first use; the
/// operator retrieves it (secret `business_os/mcp_inbound_auth_token`) to
/// configure trusted external agents. Mirrors `capability_signing_secret`.
fn mcp_auth_token(root: &Path) -> anyhow::Result<String> {
    if crate::secrets::secret_exists(root, MCP_AUTH_SECRET_SCOPE, MCP_AUTH_SECRET_NAME)? {
        let value =
            crate::secrets::read_secret_value(root, MCP_AUTH_SECRET_SCOPE, MCP_AUTH_SECRET_NAME)?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }
    use base64::Engine as _;
    use ring::rand::SecureRandom;
    let mut buf = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut buf)
        .map_err(|_| anyhow::anyhow!("failed to generate MCP auth token"))?;
    let value = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf);
    crate::secrets::write_secret_record(
        root,
        MCP_AUTH_SECRET_SCOPE,
        MCP_AUTH_SECRET_NAME,
        &value,
        Some("Business OS inbound MCP bearer token (auto-generated)".to_string()),
        serde_json::json!({ "auto_managed": true }),
    )?;
    Ok(value)
}

pub fn mcp_operator_auth_token(root: &Path) -> anyhow::Result<String> {
    mcp_auth_token(root)
}

fn request_bearer_token(request: &Request) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .and_then(|header| {
            header
                .value
                .as_str()
                .trim()
                .strip_prefix("Bearer ")
                .map(|token| token.trim().to_string())
        })
}

/// Inbound `/mcp` requests must carry a valid bearer token. Fail closed: a
/// missing token, a wrong token, or an unreadable secret store all deny.
fn mcp_request_authorized(root: &Path, request: &Request) -> bool {
    let expected = match mcp_auth_token(root) {
        Ok(token) => token,
        Err(error) => {
            eprintln!("[business-os-mcp] auth token unavailable: {error:#}");
            return false;
        }
    };
    let Some(presented) = request_bearer_token(request) else {
        return false;
    };
    ring::constant_time::verify_slices_are_equal(expected.as_bytes(), presented.as_bytes()).is_ok()
}

pub fn serve_mcp_channel(root: &Path, options: BusinessOsMcpServeOptions) -> anyhow::Result<()> {
    let server = Server::http(&options.addr)
        .map_err(|error| anyhow::anyhow!("failed to bind Business OS MCP server: {error}"))?;
    // Materialize the auth token at startup so it exists in the secret store for
    // the operator to retrieve, and warn loudly if bound beyond loopback.
    if let Err(error) = mcp_auth_token(root) {
        eprintln!("[business-os-mcp] failed to provision inbound auth token: {error:#}");
    }
    println!("CTOX Business OS MCP listening on http://{}", options.addr);
    println!(
        "MCP endpoint: http://{}/mcp (requires Authorization: Bearer <secret business_os/mcp_inbound_auth_token>)",
        options.addr
    );
    for request in server.incoming_requests() {
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            if let Err(error) = handle_mcp_http_request(&root, request) {
                eprintln!("[business-os-mcp] request failed: {error:#}");
            }
        });
    }
    Ok(())
}

pub fn connect_managed_gateway(
    root: &Path,
    options: BusinessOsMcpGatewayConnectOptions,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(connect_managed_gateway_async(root, options))
}

pub fn managed_gateway_status(options: BusinessOsMcpGatewayStatusOptions) -> anyhow::Result<Value> {
    let mut request = ureq::get(&options.url).set("accept", "application/json");
    if let Some(token) = options
        .token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    {
        request = request.set("authorization", &format!("Bearer {}", token.trim()));
    }
    match request.call() {
        Ok(response) => {
            let body = response.into_string()?;
            serde_json::from_str(&body).context("invalid managed MCP gateway status JSON")
        }
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| {
                serde_json::json!({
                    "ok": false,
                    "error": "gateway_status_failed",
                    "message": body
                })
            });
            Ok(serde_json::json!({
                "ok": false,
                "status": status,
                "gateway": parsed
            }))
        }
        Err(error) => Err(anyhow::anyhow!(
            "failed to query Business OS MCP gateway status: {error}"
        )),
    }
}

async fn connect_managed_gateway_async(
    root: &Path,
    options: BusinessOsMcpGatewayConnectOptions,
) -> anyhow::Result<()> {
    let max_delay_ms = options
        .max_reconnect_delay_ms
        .max(250)
        .min(DEFAULT_GATEWAY_RECONNECT_MAX_DELAY_MS);
    let mut attempt = 0_u32;
    loop {
        let result = connect_managed_gateway_once(root, &options).await;
        if !options.reconnect {
            return result;
        }
        if let Err(error) = &result {
            eprintln!("[business-os-mcp] managed gateway disconnected: {error:#}");
        } else {
            eprintln!("[business-os-mcp] managed gateway disconnected");
        }
        attempt = attempt.saturating_add(1);
        let delay_ms = reconnect_delay_ms(attempt, max_delay_ms);
        eprintln!("[business-os-mcp] reconnecting in {delay_ms}ms");
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

async fn connect_managed_gateway_once(
    root: &Path,
    options: &BusinessOsMcpGatewayConnectOptions,
) -> anyhow::Result<()> {
    install_rustls_crypto_provider();
    let mut request = options.url.as_str().into_client_request()?;
    if let Some(token) = options
        .token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    {
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token.trim()))
                .context("invalid managed MCP gateway token")?,
        );
    }
    request.headers_mut().insert(
        "x-ctox-mcp-timestamp",
        HeaderValue::from_str(&now_ms().to_string())
            .context("invalid managed MCP gateway timestamp")?,
    );
    request.headers_mut().insert(
        "x-ctox-mcp-nonce",
        HeaderValue::from_str(&format!("ctox-{}", uuid::Uuid::new_v4()))
            .context("invalid managed MCP gateway nonce")?,
    );
    let (stream, _) = connect_async(request)
        .await
        .context("failed to connect to Business OS MCP gateway")?;
    println!("CTOX Business OS MCP gateway connected: {}", options.url);
    let (mut write, mut read) = stream.split();
    write
        .send(Message::Text(gateway_hello_message().into()))
        .await
        .context("failed to send Business OS MCP gateway hello")?;
    let configured_heartbeat_ms = if options.heartbeat_interval_ms == 0 {
        DEFAULT_GATEWAY_HEARTBEAT_INTERVAL_MS
    } else {
        options.heartbeat_interval_ms
    };
    let heartbeat_interval_ms = configured_heartbeat_ms
        .max(5_000)
        .min(DEFAULT_GATEWAY_MAX_CONNECTION_AGE_MS);
    let max_connection_age_ms = options
        .max_connection_age_ms
        .max(heartbeat_interval_ms.saturating_mul(2))
        .min(24 * 60 * 60 * 1000);
    let mut heartbeat = tokio::time::interval(Duration::from_millis(heartbeat_interval_ms));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let reconnect_after = tokio::time::sleep(Duration::from_millis(max_connection_age_ms));
    tokio::pin!(reconnect_after);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                write
                    .send(Message::Text(gateway_hello_message().into()))
                    .await
                    .context("Business OS MCP gateway heartbeat failed")?;
            }
            _ = &mut reconnect_after => {
                anyhow::bail!("Business OS MCP gateway connection reached max age of {max_connection_age_ms}ms");
            }
            message = read.next() => {
                let Some(message) = message else {
                    break;
                };
                let message = message.context("Business OS MCP gateway websocket read failed")?;
                let Message::Text(text) = message else {
                    continue;
                };
                let response = handle_gateway_message(root, text.as_ref());
                write
                    .send(Message::Text(response.into()))
                    .await
                    .context("Business OS MCP gateway websocket write failed")?;
            }
        }
    }
    Ok(())
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn reconnect_delay_ms(attempt: u32, max_delay_ms: u64) -> u64 {
    let exponent = attempt.saturating_sub(1).min(6);
    let base = 500_u64.saturating_mul(1_u64 << exponent);
    base.min(max_delay_ms.max(250))
}

pub fn gateway_hello_message() -> String {
    serde_json::json!({
        "type": "ctox_hello",
        "ctox_version": env!("CARGO_PKG_VERSION"),
        "mcp_protocol_version": MCP_PROTOCOL_VERSION,
        "capabilities": [
            "business_os_mcp_channel_v1",
            "managed_gateway_connector",
            "typed_tools",
            "audit_events",
            "approval_gated_actions",
            "app_source_tools"
        ],
        "connected_at_ms": now_ms()
    })
    .to_string()
}

pub fn handle_gateway_message(root: &Path, message: &str) -> String {
    let response = match serde_json::from_str::<Value>(message) {
        Ok(envelope) => handle_gateway_envelope(root, envelope),
        Err(error) => serde_json::json!({
            "type": "mcp_response",
            "request_id": "",
            "status": 400,
            "headers": { "content-type": "application/json; charset=utf-8" },
            "body": serde_json::to_string(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {
                    "code": -32700,
                    "message": format!("invalid gateway envelope JSON: {error}")
                }
            })).unwrap_or_else(|_| "{}".to_string())
        }),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"type":"mcp_response","request_id":"","status":500,"headers":{"content-type":"application/json; charset=utf-8"},"body":"{}"}"#.to_string()
    })
}

fn handle_gateway_envelope(root: &Path, envelope: Value) -> Value {
    let request_id = string_field(&envelope, "request_id").unwrap_or_default();
    if envelope.get("type").and_then(Value::as_str) != Some("mcp_request") {
        return gateway_json_rpc_error(
            request_id,
            None,
            400,
            -32600,
            "unsupported gateway envelope type",
        );
    }
    let Some(body) = envelope.get("body").and_then(Value::as_str) else {
        return gateway_json_rpc_error(request_id, None, 400, -32600, "gateway body is required");
    };
    let mut parsed = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(error) => {
            return gateway_json_rpc_error(
                request_id,
                None,
                400,
                -32700,
                &format!("invalid MCP JSON-RPC body: {error}"),
            );
        }
    };
    if let Some(context) = envelope.get("context") {
        inject_gateway_context(&mut parsed, context);
    }
    if is_json_rpc_notification(&parsed) {
        return serde_json::json!({
            "type": "mcp_response",
            "request_id": request_id,
            "status": 202,
            "headers": {},
            "body": ""
        });
    }
    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
    let response = handle_json_rpc_with_gateway_context(root, parsed, envelope.get("context"));
    serde_json::json!({
        "type": "mcp_response",
        "request_id": request_id,
        "status": 200,
        "headers": { "content-type": "application/json; charset=utf-8" },
        "body": serde_json::to_string(&response).unwrap_or_else(|_| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32603,
                    "message": "failed to serialize MCP JSON-RPC response"
                }
            }).to_string()
        })
    })
}

fn gateway_json_rpc_error(
    request_id: String,
    id: Option<Value>,
    status: u16,
    code: i64,
    message: &str,
) -> Value {
    serde_json::json!({
        "type": "mcp_response",
        "request_id": request_id,
        "status": status,
        "headers": { "content-type": "application/json; charset=utf-8" },
        "body": serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "error": {
                "code": code,
                "message": message
            }
        })).unwrap_or_else(|_| "{}".to_string())
    })
}

pub fn tool_descriptors() -> Vec<BusinessOsMcpToolDescriptor> {
    vec![
        read_tool(
            "business_os.status",
            "Use this when you need CTOX Business OS MCP channel and runtime status.",
            object_schema(vec![]),
        ),
        read_tool(
            "business_os.list_modules",
            "Use this when you need the installed Business OS module catalog.",
            object_schema(vec![]),
        ),
        read_tool(
            "business_os.get_module",
            "Use this when you need metadata for one Business OS module.",
            object_schema(vec![required_string("module_id")]),
        ),
        read_tool(
            "business_os.list_entities",
            "Use this when you need the entity/collection contract exposed by a module.",
            object_schema(vec![required_string("module_id")]),
        ),
        read_tool(
            "business_os.query_records",
            "Use this when you need bounded records from a Business OS collection.",
            object_schema(vec![
                required_string("collection"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.search_records",
            "Use this when you need to search bounded records in a Business OS collection.",
            object_schema(vec![
                required_string("collection"),
                required_string("query"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.get_record",
            "Use this when you need one Business OS record by collection and id.",
            object_schema(vec![
                required_string("collection"),
                required_string("record_id"),
            ]),
        ),
        write_tool(
            "business_os.upsert_record",
            "Use this when an admin or authorized module owner needs to create or update one Business OS app data record through the policy-gated MCP channel. Do not use this for users, commands, queue tasks, app source files, credentials, runtime settings, or raw SQL.",
            object_schema(vec![
                required_string("collection"),
                optional_string("record_id"),
                required_object("record"),
            ]),
        ),
        write_tool(
            "business_os.upsert_user",
            "Use this when an admin needs to create, edit, activate, deactivate, or change the role of a Business OS user through the policy-gated MCP channel.",
            object_schema(vec![
                required_string("id"),
                required_string("display_name"),
                required_string("role"),
                optional_boolean("active"),
                optional_boolean("accept_recovery_responsibility"),
            ]),
        ),
        read_tool(
            "business_os.get_record_context",
            "Use this when you need the operational context around one Business OS record.",
            object_schema(vec![
                required_string("collection"),
                required_string("record_id"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.list_record_activity",
            "Use this when you need recent commands, runs, or approvals related to one Business OS record.",
            object_schema(vec![
                required_string("collection"),
                required_string("record_id"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.list_runs",
            "Use this when you need bounded CTOX work runs and queue tasks visible in Business OS.",
            object_schema(vec![
                optional_string("status"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.get_run",
            "Use this when you need one CTOX run or queue task by id.",
            object_schema(vec![required_string("run_id")]),
        ),
        read_tool(
            "business_os.list_artifacts",
            "Use this when you need bounded generated artifacts visible in Business OS.",
            object_schema(vec![
                optional_string("collection"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        read_tool(
            "business_os.get_artifact",
            "Use this when you need one generated artifact by id, optionally scoped to a collection.",
            object_schema(vec![
                required_string("artifact_id"),
                optional_string("collection"),
            ]),
        ),
        read_tool(
            "business_os.list_approvals",
            "Use this when you need bounded Business OS approval records.",
            object_schema(vec![
                optional_string("status"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        write_tool(
            "business_os.approve",
            "Use this when the user explicitly approved a pending Business OS approval item.",
            object_schema(vec![
                optional_string("approval_id"),
                optional_string("message_id"),
                optional_string("comment"),
            ]),
        ),
        write_tool(
            "business_os.reject",
            "Use this when the user explicitly rejected a pending Business OS approval item.",
            object_schema(vec![
                optional_string("approval_id"),
                optional_string("message_id"),
                optional_string("comment"),
            ]),
        ),
        write_tool(
            "business_os.request_changes",
            "Use this when the user explicitly requested changes for a pending Business OS approval item.",
            object_schema(vec![
                optional_string("approval_id"),
                optional_string("message_id"),
                optional_string("comment"),
            ]),
        ),
        read_tool(
            "business_os.open_link",
            "Use this when you need a deterministic Business OS deep link.",
            object_schema(vec![
                required_string("kind"),
                required_string("module_or_collection"),
                optional_string("id"),
            ]),
        ),
        write_tool(
            "business_os.create_app",
            "Use this when a coding agent should ask CTOX Business OS to create and deploy a new runtime-installed app; returns command ids, app source paths, required skill resources, and validation commands.",
            object_schema(vec![
                required_string("instruction"),
                optional_string("module_id"),
                optional_string("title"),
                optional_string("description"),
                optional_string("category"),
                optional_string("version"),
            ]),
        ),
        write_tool(
            "business_os.modify_app",
            "Use this when a coding agent should ask CTOX Business OS to modify and redeploy an existing app; returns command ids, app source paths, required skill resources, and validation commands.",
            object_schema(vec![
                required_string("module_id"),
                required_string("instruction"),
                optional_string("title"),
            ]),
        ),
        write_tool(
            "business_os.prepare_app_source",
            "Use this when a coding agent needs a runtime-installed Business OS app source workspace it can edit directly through MCP.",
            object_schema(vec![
                required_string("module_id"),
                optional_string("title"),
                optional_string("description"),
                optional_string("category"),
                optional_string("version"),
                optional_string("instruction"),
            ]),
        ),
        read_tool(
            "business_os.list_app_files",
            "Use this when a coding agent needs the app-scoped source file list for a Business OS module.",
            object_schema(vec![required_string("module_id")]),
        ),
        read_tool(
            "business_os.read_app_file",
            "Use this when a coding agent needs one source file from a Business OS app module.",
            object_schema(vec![required_string("module_id"), required_string("path")]),
        ),
        read_tool(
            "business_os.search_app_source",
            "Use this when a coding agent needs to search app-scoped Business OS source files.",
            object_schema(vec![
                required_string("module_id"),
                required_string("query"),
                optional_integer("limit", 1, MAX_LIMIT),
            ]),
        ),
        write_tool(
            "business_os.write_app_file",
            "Use this when a coding agent needs to create or replace an app-scoped source file, including relative browser ESM modules such as vendor/<name>.mjs.",
            object_schema(vec![
                required_string("module_id"),
                required_string("path"),
                required_string("content"),
            ]),
        ),
        write_tool(
            "business_os.validate_app",
            "Use this when a coding agent needs to run the bounded Business OS app validator for a module.",
            object_schema(vec![
                required_string("module_id"),
                optional_boolean("skip_tests"),
                optional_boolean("skip_node_check"),
            ]),
        ),
        write_tool(
            "business_os.smoke_app",
            "Use this when a coding agent needs a bounded browser smoke test for a Business OS app module.",
            object_schema(vec![
                required_string("module_id"),
                optional_string("url"),
                optional_integer("timeout_ms", 1_000, 300_000),
            ]),
        ),
        write_tool(
            "business_os.e2e_app",
            "Use this when a coding agent needs the bounded Business OS app E2E test for save/reload/command-bus behavior.",
            object_schema(vec![
                required_string("module_id"),
                optional_string("url"),
                optional_integer("timeout_ms", 1_000, 300_000),
                optional_string("marker"),
            ]),
        ),
        read_tool(
            "business_os.list_module_actions",
            "Use this when you need allowed action descriptors for a Business OS module.",
            object_schema(vec![required_string("module_id")]),
        ),
        read_tool(
            "business_os.propose_action",
            "Use this when you need to prepare a Business OS action without executing it.",
            object_schema(vec![
                required_string("module_id"),
                required_string("action_id"),
                optional_string("record_id"),
                optional_string("title"),
                optional_string("objective"),
                optional_object("payload"),
            ]),
        ),
        write_tool(
            "business_os.execute_action",
            "Use this when you need to enqueue an approved, typed Business OS action.",
            object_schema(vec![
                required_string("module_id"),
                required_string("action_id"),
                optional_string("record_id"),
                optional_string("title"),
                optional_string("objective"),
                optional_object("payload"),
            ]),
        ),
        read_tool(
            "business_os.get_command_status",
            "Use this when you need the current status for a Business OS command.",
            object_schema(vec![required_string("command_id")]),
        ),
        read_tool(
            "business_os.list_mcp_activity",
            "Use this when you need recent Business OS MCP channel audit events.",
            object_schema(vec![optional_integer("limit", 1, MAX_LIMIT)]),
        ),
        write_tool(
            "appsec_assessment_create",
            "Use this when an authorized external agent needs to create an AppSec assessment scope in the local CTOX AppSec state. This initializes state only; it does not run scanners.",
            object_schema(vec![
                required_string("url"),
                optional_string("state_dir"),
            ]),
        ),
        write_tool(
            "appsec_lab_create",
            "Use this when an authorized external agent needs to create the local vulnerable AppSec lab fixture for sandbox validation.",
            object_schema(vec![
                optional_string("state_dir"),
                optional_string("out"),
            ]),
        ),
        write_tool(
            "appsec_lab_run",
            "Use this when an authorized external agent needs to run the local AppSec lab validation matrix against a sandbox URL and generate normal AppSec evidence/report artifacts.",
            object_schema(vec![
                required_string("url"),
                optional_string("state_dir"),
                optional_string("profile"),
                optional_boolean("rebuild_coverage"),
                optional_boolean("report"),
            ]),
        ),
        read_tool(
            "appsec_assessment_status",
            "Use this when an external agent needs the durable AppSec assessment status and completion blockers through the policy-gated Business OS MCP channel.",
            object_schema(vec![
                optional_string("state_dir"),
                optional_boolean("sync"),
            ]),
        ),
        read_tool(
            "appsec_completion_review",
            "Use this when an external agent needs the local AppSec completion review gate produced by the same CLI path used by finish and report.",
            object_schema(vec![optional_string("state_dir")]),
        ),
        read_tool(
            "appsec_tools_doctor",
            "Use this when an external agent needs AppSec scanner readiness evidence through the policy-gated Business OS MCP channel. This does not scan a target.",
            object_schema(vec![
                optional_string("state_dir"),
                optional_string("profile"),
                optional_boolean("probe_versions"),
            ]),
        ),
        write_tool(
            "appsec_authz_plan",
            "Use this when an authorized external agent needs a redacted CTOX web-stack authorization test plan for an initialized AppSec assessment. This writes a local plan artifact only; it does not log in or run browser sessions.",
            object_schema(vec![
                required_string("target"),
                optional_string("state_dir"),
                optional_string("source_id"),
                optional_string("subjects"),
            ]),
        ),
        write_tool(
            "appsec_authz_credential_proof_template",
            "Use this when an authorized external agent needs to create a redacted credential-proof template from an AppSec authz subjects file before a live authenticated preflight. This writes no secret values.",
            object_schema(vec![
                required_string("subjects"),
                optional_string("state_dir"),
                optional_string("out"),
                optional_boolean("force"),
            ]),
        ),
        write_tool(
            "appsec_authz_preflight",
            "Use this before a live authenticated AppSec authz run to validate redacted subjects, credential references, optional Web-Stack evidence, and next commands without reading secret values or starting browser work.",
            object_schema(vec![
                required_string("target"),
                optional_string("state_dir"),
                optional_string("source_id"),
                optional_string("subjects"),
                optional_string("run"),
                optional_string("evidence_dir"),
                optional_string("credential_proof"),
                optional_boolean("require_credentials"),
                optional_boolean("require_evidence"),
            ]),
        ),
        write_tool(
            "appsec_authz_run",
            "Use this when an authorized external agent needs to create the durable CTOX web-stack authz run artifact from redacted subject references. Browser execution still happens through the CTOX web-stack contracts in the artifact.",
            object_schema(vec![
                required_string("target"),
                required_string("subjects"),
                optional_string("state_dir"),
                optional_string("source_id"),
                optional_string("credential_proof"),
            ]),
        ),
        read_tool(
            "appsec_authz_status",
            "Use this when an external agent needs the current AppSec authz readiness, latest preflight/run/matrix artifacts, and next commands before or after a live authenticated run.",
            object_schema(vec![optional_string("state_dir")]),
        ),
        write_tool(
            "appsec_authz_build_matrix",
            "Use this when an authorized external agent needs to normalize redacted CTOX web-stack evidence into an AppSec authz matrix and optionally import it into coverage/findings.",
            object_schema(vec![
                required_string("run"),
                required_string("evidence_dir"),
                optional_string("state_dir"),
                optional_string("out"),
                optional_boolean("import"),
                optional_boolean("no_mark_coverage"),
            ]),
        ),
        write_tool(
            "appsec_pipeline_rework",
            "Use this when an authorized external agent needs to attach operator-reviewed redacted evidence to one AppSec pipeline stage. This does not create scanner results or findings.",
            object_schema(vec![
                optional_string("state_dir"),
                optional_string("stage_id"),
                optional_string("phase"),
                optional_string("target"),
                optional_string("status"),
                required_string("reason"),
                optional_string("artifact"),
                optional_string_array("artifacts"),
            ]),
        ),
        read_tool(
            "appsec_report_get",
            "Use this when an external agent needs the existing AppSec report artifact. The tool reads the sanitized report from local AppSec state and does not generate a new report.",
            object_schema(vec![
                optional_string("state_dir"),
                optional_string("format"),
            ]),
        ),
        read_tool(
            "appsec_finding_get",
            "Use this when an external agent needs one AppSec finding by id through the policy-gated Business OS MCP channel.",
            object_schema(vec![
                required_string("finding_id"),
                optional_string("state_dir"),
            ]),
        ),
    ]
}

pub fn mcp_status(root: &Path, context: &McpChannelRequestContext) -> anyhow::Result<Value> {
    context.validate()?;
    let status = store::status(root)?;
    let policy = mcp_policy(root);
    Ok(serde_json::json!({
        "ok": true,
        "channel": &context.channel,
        "surface": &context.surface,
        "actor": &context.actor,
        "workspace": &context.workspace,
        "policy": policy,
        "business_os": status
    }))
}

pub fn list_modules(
    root: &Path,
    context: &McpChannelRequestContext,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsModuleDescriptor>> {
    context.validate()?;
    let catalog = store::module_catalog_for_rxdb(root)?;
    let modules = catalog
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let policy = mcp_policy(root);
    let modules = modules
        .into_iter()
        .filter(|module| {
            let module_id = string_field(module, "id").unwrap_or_default();
            policy.allowed_modules.is_empty() || policy.allowed_modules.contains(&module_id)
        })
        .filter(|module| module_value_visible_to_mcp_actor(root, context, module))
        .map(module_descriptor_from_value)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(BusinessOsMcpList {
        ok: true,
        count: modules.len(),
        limit: modules.len(),
        items: modules,
    })
}

pub fn get_module(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
) -> anyhow::Result<BusinessOsModuleDescriptor> {
    ensure_non_empty("module_id", module_id)?;
    list_modules(root, context)?
        .items
        .into_iter()
        .find(|module| module.id == module_id)
        .ok_or_else(|| {
            anyhow::Error::new(BusinessOsMcpError::not_found(
                BusinessOsMcpErrorCode::ModuleNotFound,
                format!("Business OS module `{module_id}` was not found"),
            ))
        })
}

pub fn list_entities(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsEntityDescriptor>> {
    let module = get_module(root, context, module_id)?;
    let policy = mcp_policy(root);
    let entities = module
        .collections
        .iter()
        .filter(|collection| {
            policy.allowed_collections.is_empty()
                || policy.allowed_collections.contains(&collection.to_string())
        })
        .map(|collection| {
            let write_decision =
                business_os_mcp_collection_write_decision(root, context, collection)?;
            Ok(BusinessOsEntityDescriptor {
                module_id: module.id.clone(),
                entity_id: collection.to_string(),
                collection: collection.to_string(),
                title: titleize_collection(collection),
                read_only: !write_decision.allowed
                    || collection_requires_typed_mcp_tool(collection),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(BusinessOsMcpList {
        ok: true,
        count: entities.len(),
        limit: entities.len(),
        items: entities,
    })
}

pub fn query_records(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    context.validate()?;
    ensure_non_empty("collection", collection)?;
    enforce_collection_policy(root, collection)?;
    let limit = bounded_limit(limit);
    let payload = store::pull_latest_collection_records(root, collection, Some(limit))?;
    let documents = payload
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let records = documents
        .into_iter()
        .map(|record| record_summary_from_value(collection, record))
        .collect::<Vec<_>>();
    Ok(BusinessOsMcpList {
        ok: true,
        count: records.len(),
        limit,
        items: records,
    })
}

pub fn search_records(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    query: &str,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    ensure_non_empty("query", query)?;
    let query_lc = query.trim().to_lowercase();
    let mut records = query_records(root, context, collection, limit)?;
    records.items.retain(|record| {
        record.title.to_lowercase().contains(&query_lc)
            || record
                .summary
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&query_lc)
            || serde_json::to_string(&record.data)
                .unwrap_or_default()
                .to_lowercase()
                .contains(&query_lc)
    });
    records.count = records.items.len();
    Ok(records)
}

pub fn upsert_record(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsMcpMutationResponse> {
    context.validate()?;
    let collection = required_arg(arguments, "collection")?;
    ensure_non_empty("collection", &collection)?;
    enforce_collection_policy(root, &collection)?;
    if collection_requires_typed_mcp_tool(&collection) {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "collection",
            format!(
                "`{collection}` is a control collection; use the dedicated Business OS MCP tool instead"
            ),
        )));
    }
    enforce_business_os_mcp_policy(root, context, "business_os.upsert_record", arguments)?;

    let record = arguments
        .get("record")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| {
            anyhow::Error::new(BusinessOsMcpError::validation("record", "object required"))
        })?;
    let record_id = optional_string_arg(arguments, "record_id")
        .or_else(|| string_field(&record, "id"))
        .or_else(|| string_field(&record, "record_id"))
        .ok_or_else(|| {
            anyhow::Error::new(BusinessOsMcpError::validation("record_id", "required"))
        })?;
    ensure_non_empty("record_id", &record_id)?;

    let conn = store::open_store(root)?;
    let updated_at_ms = now_ms() as i64;
    store::upsert_business_record(&conn, &collection, &record_id, updated_at_ms, record)?;
    drop(conn);

    let record = get_record(root, context, &collection, &record_id)?.record;
    Ok(BusinessOsMcpMutationResponse {
        ok: true,
        collection,
        record_id,
        record,
    })
}

pub fn upsert_user(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    context.validate()?;
    enforce_collection_policy(root, "business_users")?;
    enforce_business_os_mcp_policy(root, context, "business_os.upsert_user", arguments)?;
    let mutation = store::BusinessOsUserMutation {
        id: required_arg(arguments, "id")?,
        display_name: required_arg(arguments, "display_name")?,
        role: required_arg(arguments, "role")?,
        active: arguments
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        profile: arguments.get("profile").cloned(),
        accept_recovery_responsibility: optional_bool_arg(
            arguments,
            "accept_recovery_responsibility",
        ),
    };
    let session = mcp_session(root, context)?;
    store::upsert_user(root, &session, mutation)
}

pub fn create_app(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppCommandExecution> {
    context.validate()?;
    let instruction = required_arg(arguments, "instruction")?;
    let module_id = app_module_id_from_arguments(arguments, &instruction)?;
    enforce_module_policy(root, &module_id)?;
    enforce_business_os_mcp_policy(root, context, "business_os.create_app", arguments)?;
    let title = optional_string_arg(arguments, "title")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| title_from_module_id(&module_id));
    let description = optional_string_arg(arguments, "description")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| instruction.chars().take(220).collect::<String>());
    let category = optional_string_arg(arguments, "category").unwrap_or_default();
    let version = normalize_app_semver(
        optional_string_arg(arguments, "version")
            .as_deref()
            .unwrap_or("0.1.0"),
    )?;
    let actor = resolved_mcp_actor_context(root, context)?;
    let client_context = serde_json::json!({
        "channel": &context.channel,
        "surface": &context.surface,
        "actor": actor,
        "mcp_actor": &context.actor,
        "workspace": &context.workspace,
        "request_id": &context.request_id,
        "mcp_tool": &context.tool,
        "source": "business-os-mcp",
        "target": "app",
        "mode": "app",
        "module_id": module_id.as_str(),
        "app_id": module_id.as_str(),
        "install_target": "runtime-installed-module",
        "required_skills": ["business-os-app-module-development"]
    });
    let accepted = store::record_command(
        root,
        store::BusinessCommand {
            origin: store::CommandOrigin::TrustedLocal,
            id: None,
            module: "creator".to_string(),
            command_type: "ctox.business_os.app.create".to_string(),
            record_id: Some(module_id.clone()),
            payload: serde_json::json!({
                "title": format!("Create {title}"),
                "instruction": instruction.as_str(),
                "module_id": module_id.as_str(),
                "app_id": module_id.as_str(),
                "app_title": title.as_str(),
                "description": description.as_str(),
                "category": category.as_str(),
                "desired_version": version.as_str(),
                "install_target": "runtime-installed-module",
                "target": "app",
                "mode": "app",
                "required_skills": ["business-os-app-module-development"]
            }),
            client_context: client_context.clone(),
        },
    )?;
    Ok(BusinessOsAppCommandExecution {
        ok: accepted.ok,
        module_id: module_id.clone(),
        command_type: "ctox.business_os.app.create".to_string(),
        command_id: accepted.command_id,
        status: accepted.status.to_string(),
        task_id: accepted.task_id,
        task_status: accepted.task_status,
        install_target: "runtime-installed-module".to_string(),
        app_directory: format!("runtime/business-os/installed-modules/{module_id}"),
        development_contract: app_development_contract(&module_id, "runtime-installed-module"),
        client_context,
    })
}

pub fn modify_app(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppCommandExecution> {
    context.validate()?;
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    let instruction = required_arg(arguments, "instruction")?;
    enforce_module_policy(root, &module_id)?;
    enforce_business_os_mcp_policy(root, context, "business_os.modify_app", arguments)?;
    let _module = get_module(root, context, &module_id)?;
    let title = optional_string_arg(arguments, "title")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("Modify {}", title_from_module_id(&module_id)));
    let actor = resolved_mcp_actor_context(root, context)?;
    let client_context = serde_json::json!({
        "channel": &context.channel,
        "surface": &context.surface,
        "actor": actor,
        "mcp_actor": &context.actor,
        "workspace": &context.workspace,
        "request_id": &context.request_id,
        "mcp_tool": &context.tool,
        "source": "business-os-mcp",
        "target": "app",
        "mode": "app",
        "module_id": module_id.as_str(),
        "app_id": module_id.as_str(),
        "install_target": "runtime-installed-module",
        "required_skills": ["business-os-app-module-development"]
    });
    let accepted = store::record_command(
        root,
        store::BusinessCommand {
            origin: store::CommandOrigin::TrustedLocal,
            id: None,
            module: "creator".to_string(),
            command_type: "ctox.business_os.app.modify".to_string(),
            record_id: Some(module_id.clone()),
            payload: serde_json::json!({
                "title": title.as_str(),
                "instruction": instruction.as_str(),
                "module_id": module_id.as_str(),
                "app_id": module_id.as_str(),
                "install_target": "runtime-installed-module",
                "target": "app",
                "mode": "app",
                "required_skills": ["business-os-app-module-development"]
            }),
            client_context: client_context.clone(),
        },
    )?;
    Ok(BusinessOsAppCommandExecution {
        ok: accepted.ok,
        module_id: module_id.clone(),
        command_type: "ctox.business_os.app.modify".to_string(),
        command_id: accepted.command_id,
        status: accepted.status.to_string(),
        task_id: accepted.task_id,
        task_status: accepted.task_status,
        install_target: "runtime-installed-module".to_string(),
        app_directory: format!("runtime/business-os/installed-modules/{module_id}"),
        development_contract: app_development_contract(&module_id, "runtime-installed-module"),
        client_context,
    })
}

fn app_development_contract(
    module_id: &str,
    install_target: &str,
) -> BusinessOsAppDevelopmentContract {
    let source_root = if install_target == "runtime-installed-module" {
        format!("runtime/business-os/installed-modules/{module_id}")
    } else {
        format!("src/apps/business-os/modules/{module_id}")
    };
    let source_files = [
        "module.json",
        "collections.schema.json",
        "schema.js",
        "index.html",
        "index.css",
        "index.js",
        "icon.svg",
        "core/records.mjs",
        "core/automation.mjs",
        "lib/*.mjs",
        "vendor/*.mjs",
        "locales/en.json",
        "locales/de.json",
        "tests/*.test.mjs",
    ]
    .iter()
    .map(|file| format!("{source_root}/{file}"))
    .collect::<Vec<_>>();
    BusinessOsAppDevelopmentContract {
        source_root: source_root.clone(),
        required_skill: "business-os-app-module-development".to_string(),
        skill_resources: vec![
            "src/skills/system/product_engineering/business-os-app-module-development/SKILL.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/design-guide.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/standalone-porting.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md"
                .to_string(),
            "src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md"
                .to_string(),
        ],
        source_files,
        source_tools: vec![
            "business_os.prepare_app_source".to_string(),
            "business_os.list_app_files".to_string(),
            "business_os.read_app_file".to_string(),
            "business_os.search_app_source".to_string(),
            "business_os.write_app_file".to_string(),
            "business_os.validate_app".to_string(),
            "business_os.smoke_app".to_string(),
            "business_os.e2e_app".to_string(),
        ],
        reference_catalog_command:
            "ctox business-os app references --query \"<workflow data keywords>\" --json --limit 8"
                .to_string(),
        validation_command: format!(
            "ctox business-os app validate {module_id} {}",
            app_validation_mode_flag(install_target)
        ),
        smoke_command: format!(
            "ctox business-os app smoke {module_id} {} --json",
            app_validation_mode_flag(install_target)
        ),
        e2e_command: format!(
            "ctox business-os app e2e {module_id} {} --json",
            app_validation_mode_flag(install_target)
        ),
        command_status_tool:
            "business_os.get_command_status command_id=<command_id from this response>".to_string(),
        lifecycle: vec![
            "MCP accepts a typed command and returns command_id/task_id; it does not expose shell, SQL, or raw RxDB writes."
                .to_string(),
            "A coding agent can alternatively use business_os.prepare_app_source, then read/write app-scoped source files directly through MCP."
                .to_string(),
            format!(
                "The CTOX worker edits or creates the app under `{source_root}` with the required Business OS app skill."
            ),
            "Browser ESM dependencies must be checked in as relative .mjs files under the app source root, for example vendor/<name>.mjs or lib/<name>.mjs."
                .to_string(),
            "Validation must pass before the app task can finalize a runtime-installed module version."
                .to_string(),
            "Schema changes refresh the native Business OS RxDB peer so new collections replicate over WebRTC."
                .to_string(),
            "Poll business_os.get_command_status until completed, failed, cancelled, or blocked."
                .to_string(),
        ],
        data_boundary:
            "Business OS app records, commands, module manifests, and runtime state remain in CTOX/RxDB; MCP is a typed control channel, not a data-plane or file bridge."
                .to_string(),
    }
}

fn app_validation_mode_flag(install_target: &str) -> &'static str {
    if install_target == "runtime-installed-module" {
        "--installed"
    } else {
        "--source"
    }
}

pub fn prepare_app_source(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    context.validate()?;
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    enforce_module_policy(root, &module_id)?;
    enforce_business_os_mcp_policy(root, context, "business_os.prepare_app_source", arguments)?;
    let version = normalize_app_semver(
        optional_string_arg(arguments, "version")
            .as_deref()
            .unwrap_or("0.1.0"),
    )?;
    let mut result = store::prepare_runtime_app_source_workspace(
        root,
        store::RuntimeAppSourceWorkspaceRequest {
            module_id: module_id.clone(),
            title: optional_string_arg(arguments, "title")
                .unwrap_or_else(|| title_from_module_id(&module_id)),
            description: optional_string_arg(arguments, "description").unwrap_or_default(),
            category: optional_string_arg(arguments, "category")
                .unwrap_or_else(|| "Agent Apps".to_string()),
            version,
            instruction: optional_string_arg(arguments, "instruction").unwrap_or_default(),
        },
    )?;
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "development_contract".to_string(),
            serde_json::to_value(app_development_contract(
                &module_id,
                "runtime-installed-module",
            ))?,
        );
    }
    Ok(result)
}

pub fn list_app_files(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsAppSourceFileSummary>> {
    let module_id = sanitize_app_module_id(module_id)?;
    let files = load_app_source_documents(root, context, &module_id, "business_os.list_app_files")?
        .into_iter()
        .map(app_source_summary_from_value)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(BusinessOsMcpList {
        ok: true,
        count: files.len(),
        limit: files.len(),
        items: files,
    })
}

pub fn read_app_file(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    path: &str,
) -> anyhow::Result<BusinessOsAppSourceFile> {
    let module_id = sanitize_app_module_id(module_id)?;
    let path = normalize_mcp_source_path(path)?;
    let files = load_app_source_documents(root, context, &module_id, "business_os.read_app_file")?;
    let file = files
        .into_iter()
        .find(|file| string_field(file, "path").as_deref() == Some(path.as_str()))
        .ok_or_else(|| {
            anyhow::Error::new(BusinessOsMcpError::not_found(
                BusinessOsMcpErrorCode::RecordNotFound,
                format!("Business OS app file `{path}` was not found in module `{module_id}`"),
            ))
        })?;
    let summary = app_source_summary_from_value(file.clone())?;
    Ok(BusinessOsAppSourceFile {
        ok: true,
        module_id: summary.module_id,
        path: summary.path,
        language: summary.language,
        size_bytes: summary.size_bytes,
        sha256: summary.sha256,
        source_file_id: summary.source_file_id,
        modified_at_ms: summary.modified_at_ms,
        content: file
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn search_app_source(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    query: &str,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsAppSourceSearchMatch>> {
    ensure_non_empty("query", query)?;
    let module_id = sanitize_app_module_id(module_id)?;
    let query_lc = query.trim().to_lowercase();
    let limit = bounded_limit(limit);
    let mut matches = Vec::new();
    for file in
        load_app_source_documents(root, context, &module_id, "business_os.search_app_source")?
    {
        let path = string_field(&file, "path").unwrap_or_default();
        let source_file_id = string_field(&file, "id").unwrap_or_default();
        let content = file
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for (line_index, line) in content.lines().enumerate() {
            if !line.to_lowercase().contains(&query_lc) {
                continue;
            }
            matches.push(BusinessOsAppSourceSearchMatch {
                module_id: module_id.clone(),
                path: path.clone(),
                line: line_index + 1,
                preview: mcp_audit_truncate(line.trim(), 240),
                source_file_id: source_file_id.clone(),
            });
            if matches.len() >= limit {
                return Ok(BusinessOsMcpList {
                    ok: true,
                    count: matches.len(),
                    limit,
                    items: matches,
                });
            }
        }
    }
    Ok(BusinessOsMcpList {
        ok: true,
        count: matches.len(),
        limit,
        items: matches,
    })
}

pub fn write_app_file(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppSourceWrite> {
    context.validate()?;
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    let path = normalize_mcp_source_path(&required_arg(arguments, "path")?)?;
    let content = required_raw_string_arg(arguments, "content")?;
    if content.len() > MAX_APP_SOURCE_WRITE_BYTES {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ValidationFailed,
            message: format!(
                "app source file is too large: {} bytes exceeds {} bytes",
                content.len(),
                MAX_APP_SOURCE_WRITE_BYTES
            ),
            field: Some("content".to_string()),
        }));
    }
    enforce_module_policy(root, &module_id)?;
    enforce_business_os_mcp_policy(root, context, "business_os.write_app_file", arguments)?;
    let _module = get_module(root, context, &module_id)?;
    let outcome = store::save_module_source_record(
        root,
        store::ModuleSourceSaveMutation {
            module_id: module_id.clone(),
            path: path.clone(),
            content,
        },
    )?;
    Ok(BusinessOsAppSourceWrite {
        ok: outcome.get("ok").and_then(Value::as_bool).unwrap_or(false),
        module_id,
        path,
        source_file_id: string_field(&outcome, "source_file_id").unwrap_or_default(),
        source_file_ids: outcome
            .get("source_file_ids")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        size_bytes: outcome
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        sha256: string_field(&outcome, "sha256").unwrap_or_default(),
        previous_sha256: optional_non_empty_string_field(&outcome, "previous_sha256"),
        snapshot_id: optional_non_empty_string_field(&outcome, "snapshot_id"),
        changed: outcome
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        validation_tool: "business_os.validate_app".to_string(),
    })
}

pub fn validate_app(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppRunResult> {
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    let mut args = vec![
        "--installed".to_string(),
        "--json".to_string(),
        "--workspace".to_string(),
        root.display().to_string(),
    ];
    if optional_bool_arg(arguments, "skip_tests") {
        args.push("--skip-tests".to_string());
    }
    if optional_bool_arg(arguments, "skip_node_check") {
        args.push("--skip-node-check".to_string());
    }
    run_app_check_tool(
        root,
        context,
        "business_os.validate_app",
        &module_id,
        "validate-app-module.mjs",
        args,
    )
}

pub fn smoke_app(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppRunResult> {
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    let mut args = vec!["--installed".to_string(), "--json".to_string()];
    if let Some(url) = optional_string_arg(arguments, "url") {
        args.push("--url".to_string());
        args.push(url);
    }
    if let Some(timeout_ms) = optional_usize_arg(arguments, "timeout_ms") {
        args.push("--timeout-ms".to_string());
        args.push(timeout_ms.to_string());
    }
    run_app_check_tool(
        root,
        context,
        "business_os.smoke_app",
        &module_id,
        "smoke-app-module.mjs",
        args,
    )
}

pub fn e2e_app(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<BusinessOsAppRunResult> {
    let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
    let mut args = vec!["--installed".to_string(), "--json".to_string()];
    if let Some(url) = optional_string_arg(arguments, "url") {
        args.push("--url".to_string());
        args.push(url);
    }
    if let Some(timeout_ms) = optional_usize_arg(arguments, "timeout_ms") {
        args.push("--timeout-ms".to_string());
        args.push(timeout_ms.to_string());
    }
    if let Some(marker) = optional_string_arg(arguments, "marker") {
        args.push("--marker".to_string());
        args.push(marker);
    }
    run_app_check_tool(
        root,
        context,
        "business_os.e2e_app",
        &module_id,
        "e2e-app-module.mjs",
        args,
    )
}

fn load_app_source_documents(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    tool_name: &str,
) -> anyhow::Result<Vec<Value>> {
    context.validate()?;
    enforce_module_policy(root, module_id)?;
    enforce_business_os_mcp_policy(
        root,
        context,
        tool_name,
        &serde_json::json!({ "module_id": module_id }),
    )?;
    let _module = get_module(root, context, module_id)?;
    let loaded = store::load_module_source_records(
        root,
        &store::ModuleSourceLoadMutation {
            module_id: module_id.to_string(),
        },
    )?;
    let source_file_ids = loaded
        .get("source_file_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let conn = store::open_store(root)?;
    let mut files = Vec::new();
    for source_file_id in source_file_ids.iter().filter_map(Value::as_str) {
        let payload_json: Option<String> = conn
            .query_row(
                "SELECT payload_json
                 FROM business_records
                 WHERE collection = 'business_module_source_files'
                   AND record_id = ?1
                   AND deleted = 0",
                params![source_file_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(payload_json) = payload_json {
            let value: Value = serde_json::from_str(&payload_json).with_context(|| {
                format!("invalid Business OS source file projection `{source_file_id}`")
            })?;
            if string_field(&value, "module_id").as_deref() == Some(module_id) {
                files.push(value);
            }
        }
    }
    files.sort_by(|left, right| {
        string_field(left, "path")
            .unwrap_or_default()
            .cmp(&string_field(right, "path").unwrap_or_default())
    });
    Ok(files)
}

fn app_source_summary_from_value(value: Value) -> anyhow::Result<BusinessOsAppSourceFileSummary> {
    Ok(BusinessOsAppSourceFileSummary {
        module_id: string_field(&value, "module_id").context("source module_id missing")?,
        path: string_field(&value, "path").context("source path missing")?,
        language: string_field(&value, "language").unwrap_or_else(|| "text".to_string()),
        size_bytes: value
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        sha256: string_field(&value, "sha256").unwrap_or_default(),
        source_file_id: string_field(&value, "id").context("source file id missing")?,
        modified_at_ms: value.get("updated_at_ms").and_then(Value::as_i64),
    })
}

fn run_app_check_tool(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    module_id: &str,
    script_name: &str,
    args: Vec<String>,
) -> anyhow::Result<BusinessOsAppRunResult> {
    context.validate()?;
    enforce_module_policy(root, module_id)?;
    enforce_business_os_mcp_policy(
        root,
        context,
        tool_name,
        &serde_json::json!({ "module_id": module_id }),
    )?;
    let _module = get_module(root, context, module_id)?;
    let script = root
        .join("src")
        .join("apps")
        .join("business-os")
        .join("scripts")
        .join(script_name);
    if !script.is_file() {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::RuntimeUnavailable,
            message: format!(
                "Business OS app check script is not available at {}",
                script.display()
            ),
            field: Some("script".to_string()),
        }));
    }
    let mut command = std::process::Command::new(
        crate::service::business_os::resolve_business_os_validator_node(root),
    );
    command.current_dir(root).arg(&script).arg(module_id);
    for arg in &args {
        command.arg(arg);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run Business OS app check `{script_name}`"))?;
    let mut command_parts = vec![
        "node".to_string(),
        script.display().to_string(),
        module_id.to_string(),
    ];
    command_parts.extend(args);
    Ok(BusinessOsAppRunResult {
        ok: output.status.success(),
        module_id: module_id.to_string(),
        command: command_parts.join(" "),
        exit_code: output.status.code(),
        stdout: bounded_output_text(&output.stdout),
        stderr: bounded_output_text(&output.stderr),
    })
}

fn normalize_mcp_source_path(path: &str) -> anyhow::Result<String> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.starts_with('.'))
    {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "path",
            "source path must be a relative non-hidden path inside the app module",
        )));
    }
    Ok(trimmed)
}

fn required_raw_string_arg(arguments: &Value, field: &str) -> anyhow::Result<String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow::Error::new(BusinessOsMcpError::validation(field, "required")))
}

fn optional_bool_arg(arguments: &Value, field: &str) -> bool {
    arguments
        .get(field)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn optional_non_empty_string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn bounded_output_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    let char_count = text.chars().count();
    if char_count <= MAX_APP_RUN_OUTPUT_CHARS {
        return text;
    }
    let tail = text
        .chars()
        .skip(char_count.saturating_sub(MAX_APP_RUN_OUTPUT_CHARS))
        .collect::<String>();
    format!("[truncated to last {MAX_APP_RUN_OUTPUT_CHARS} chars]\n{tail}")
}

pub fn call_tool(root: &Path, tool_name: &str, arguments: Value) -> anyhow::Result<Value> {
    call_tool_inner(root, tool_name, arguments, None)
}

fn call_tool_with_trusted_gateway_context(
    root: &Path,
    tool_name: &str,
    arguments: Value,
    trusted_gateway_context: Option<&Value>,
) -> anyhow::Result<Value> {
    call_tool_inner(root, tool_name, arguments, trusted_gateway_context)
}

fn call_tool_inner(
    root: &Path,
    tool_name: &str,
    arguments: Value,
    trusted_gateway_context: Option<&Value>,
) -> anyhow::Result<Value> {
    let context = context_from_arguments_with_trusted_gateway_context(
        tool_name,
        &arguments,
        trusted_gateway_context,
    )?;
    enforce_tool_policy(root, tool_name)?;
    enforce_context_policy(root, &context)?;
    enforce_argument_scope_policy(root, &context, tool_name, &arguments)?;
    enforce_rate_limit(root, &context)?;
    let result = match tool_name {
        "business_os.status" => mcp_status(root, &context)?,
        "business_os.list_modules" => serde_json::to_value(list_modules(root, &context)?)?,
        "business_os.get_module" => {
            let module_id = required_arg(&arguments, "module_id")?;
            serde_json::to_value(get_module(root, &context, &module_id)?)?
        }
        "business_os.list_entities" => {
            let module_id = required_arg(&arguments, "module_id")?;
            serde_json::to_value(list_entities(root, &context, &module_id)?)?
        }
        "business_os.query_records" => {
            let collection = required_arg(&arguments, "collection")?;
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(query_records(root, &context, &collection, limit)?)?
        }
        "business_os.search_records" => {
            let collection = required_arg(&arguments, "collection")?;
            let query = required_arg(&arguments, "query")?;
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(search_records(root, &context, &collection, &query, limit)?)?
        }
        "business_os.get_record" => {
            let collection = required_arg(&arguments, "collection")?;
            let record_id = required_arg(&arguments, "record_id")?;
            serde_json::to_value(get_record(root, &context, &collection, &record_id)?)?
        }
        "business_os.upsert_record" => {
            serde_json::to_value(upsert_record(root, &context, &arguments)?)?
        }
        "business_os.upsert_user" => {
            serde_json::to_value(upsert_user(root, &context, &arguments)?)?
        }
        "business_os.get_record_context" => {
            let collection = required_arg(&arguments, "collection")?;
            let record_id = required_arg(&arguments, "record_id")?;
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(get_record_context(
                root,
                &context,
                &collection,
                &record_id,
                limit,
            )?)?
        }
        "business_os.list_record_activity" => {
            let collection = required_arg(&arguments, "collection")?;
            let record_id = required_arg(&arguments, "record_id")?;
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(list_record_activity(
                root,
                &context,
                &collection,
                &record_id,
                limit,
            )?)?
        }
        "business_os.list_runs" => {
            let status = optional_string_arg(&arguments, "status");
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(list_runs(root, &context, status.as_deref(), limit)?)?
        }
        "business_os.get_run" => {
            let run_id = required_arg(&arguments, "run_id")?;
            serde_json::to_value(get_run(root, &context, &run_id)?)?
        }
        "business_os.list_artifacts" => {
            let collection = optional_string_arg(&arguments, "collection");
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(list_artifacts(
                root,
                &context,
                collection.as_deref(),
                limit,
            )?)?
        }
        "business_os.get_artifact" => {
            let artifact_id = required_arg(&arguments, "artifact_id")?;
            let collection = optional_string_arg(&arguments, "collection");
            serde_json::to_value(get_artifact(
                root,
                &context,
                collection.as_deref(),
                &artifact_id,
            )?)?
        }
        "business_os.list_approvals" => {
            let status = optional_string_arg(&arguments, "status");
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(list_approvals(root, &context, status.as_deref(), limit)?)?
        }
        "business_os.approve" => serde_json::to_value(record_approval_decision(
            root, &context, &arguments, "approved",
        )?)?,
        "business_os.reject" => serde_json::to_value(record_approval_decision(
            root, &context, &arguments, "rejected",
        )?)?,
        "business_os.request_changes" => serde_json::to_value(record_approval_decision(
            root,
            &context,
            &arguments,
            "changes_requested",
        )?)?,
        "business_os.open_link" => {
            let kind = required_arg(&arguments, "kind")?;
            let module_or_collection = required_arg(&arguments, "module_or_collection")?;
            let id = optional_string_arg(&arguments, "id");
            serde_json::to_value(open_link(&kind, &module_or_collection, id.as_deref()))?
        }
        "business_os.create_app" => serde_json::to_value(create_app(root, &context, &arguments)?)?,
        "business_os.modify_app" => serde_json::to_value(modify_app(root, &context, &arguments)?)?,
        "business_os.prepare_app_source" => {
            serde_json::to_value(prepare_app_source(root, &context, &arguments)?)?
        }
        "business_os.list_app_files" => {
            let module_id = required_arg(&arguments, "module_id")?;
            serde_json::to_value(list_app_files(root, &context, &module_id)?)?
        }
        "business_os.read_app_file" => {
            let module_id = required_arg(&arguments, "module_id")?;
            let path = required_arg(&arguments, "path")?;
            serde_json::to_value(read_app_file(root, &context, &module_id, &path)?)?
        }
        "business_os.search_app_source" => {
            let module_id = required_arg(&arguments, "module_id")?;
            let query = required_arg(&arguments, "query")?;
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(search_app_source(
                root, &context, &module_id, &query, limit,
            )?)?
        }
        "business_os.write_app_file" => {
            serde_json::to_value(write_app_file(root, &context, &arguments)?)?
        }
        "business_os.validate_app" => {
            serde_json::to_value(validate_app(root, &context, &arguments)?)?
        }
        "business_os.smoke_app" => serde_json::to_value(smoke_app(root, &context, &arguments)?)?,
        "business_os.e2e_app" => serde_json::to_value(e2e_app(root, &context, &arguments)?)?,
        "business_os.list_module_actions" => {
            let module_id = required_arg(&arguments, "module_id")?;
            serde_json::to_value(list_module_actions(root, &context, &module_id)?)?
        }
        "business_os.propose_action" => {
            let module_id = required_arg(&arguments, "module_id")?;
            let action_id = required_arg(&arguments, "action_id")?;
            serde_json::to_value(propose_action(
                root, &context, &module_id, &action_id, &arguments,
            )?)?
        }
        "business_os.execute_action" => {
            let module_id = required_arg(&arguments, "module_id")?;
            let action_id = required_arg(&arguments, "action_id")?;
            serde_json::to_value(execute_action(
                root, &context, &module_id, &action_id, &arguments,
            )?)?
        }
        "business_os.get_command_status" => {
            let command_id = required_arg(&arguments, "command_id")?;
            serde_json::to_value(get_command_status(root, &context, &command_id)?)?
        }
        "business_os.list_mcp_activity" => {
            let limit = optional_usize_arg(&arguments, "limit");
            serde_json::to_value(list_mcp_activity(root, &context, limit)?)?
        }
        "appsec_assessment_create" => {
            serde_json::to_value(appsec_assessment_create(root, &context, &arguments)?)?
        }
        "appsec_lab_create" => {
            serde_json::to_value(appsec_lab_create(root, &context, &arguments)?)?
        }
        "appsec_lab_run" => serde_json::to_value(appsec_lab_run(root, &context, &arguments)?)?,
        "appsec_assessment_status" => {
            serde_json::to_value(appsec_assessment_status(root, &context, &arguments)?)?
        }
        "appsec_completion_review" => {
            serde_json::to_value(appsec_completion_review(root, &context, &arguments)?)?
        }
        "appsec_tools_doctor" => {
            serde_json::to_value(appsec_tools_doctor(root, &context, &arguments)?)?
        }
        "appsec_authz_plan" => {
            serde_json::to_value(appsec_authz_plan(root, &context, &arguments)?)?
        }
        "appsec_authz_credential_proof_template" => serde_json::to_value(
            appsec_authz_credential_proof_template(root, &context, &arguments)?,
        )?,
        "appsec_authz_preflight" => {
            serde_json::to_value(appsec_authz_preflight(root, &context, &arguments)?)?
        }
        "appsec_authz_run" => serde_json::to_value(appsec_authz_run(root, &context, &arguments)?)?,
        "appsec_authz_status" => {
            serde_json::to_value(appsec_authz_status(root, &context, &arguments)?)?
        }
        "appsec_authz_build_matrix" => {
            serde_json::to_value(appsec_authz_build_matrix(root, &context, &arguments)?)?
        }
        "appsec_pipeline_rework" => {
            serde_json::to_value(appsec_pipeline_rework(root, &context, &arguments)?)?
        }
        "appsec_report_get" => {
            serde_json::to_value(appsec_report_get(root, &context, &arguments)?)?
        }
        "appsec_finding_get" => {
            serde_json::to_value(appsec_finding_get(root, &context, &arguments)?)?
        }
        other => {
            return Err(anyhow::Error::new(BusinessOsMcpError::not_found(
                BusinessOsMcpErrorCode::ActionNotAllowed,
                format!("Business OS MCP tool `{other}` is not available"),
            )));
        }
    };
    let result = redact_mcp_response(result);
    ensure_mcp_response_size(&result)?;
    record_tool_event(
        root,
        &context,
        "completed",
        None,
        argument_metadata_with_policy(root, &context, tool_name, &arguments),
    )?;
    Ok(result)
}

pub fn call_tool_audited(root: &Path, tool_name: &str, arguments: Value) -> anyhow::Result<Value> {
    call_tool_audited_inner(root, tool_name, arguments, None)
}

fn call_tool_audited_with_trusted_gateway_context(
    root: &Path,
    tool_name: &str,
    arguments: Value,
    trusted_gateway_context: Option<&Value>,
) -> anyhow::Result<Value> {
    call_tool_audited_inner(root, tool_name, arguments, trusted_gateway_context)
}

fn call_tool_audited_inner(
    root: &Path,
    tool_name: &str,
    arguments: Value,
    trusted_gateway_context: Option<&Value>,
) -> anyhow::Result<Value> {
    let context = context_from_arguments_with_trusted_gateway_context(
        tool_name,
        &arguments,
        trusted_gateway_context,
    )?;
    let result = call_tool_with_trusted_gateway_context(
        root,
        tool_name,
        arguments.clone(),
        trusted_gateway_context,
    );
    if let Err(error) = &result {
        let _ = record_tool_event(
            root,
            &context,
            "failed",
            Some(error.to_string()),
            argument_metadata_with_policy(root, &context, tool_name, &arguments),
        );
    }
    result
}

fn appsec_assessment_create(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_assessment_create", arguments)?;
    let url = required_arg(arguments, "url")?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let state_dir_arg = path_string(&state_dir);
    let init = crate::run_projected_appsec_command(
        root,
        &[
            "--state-dir".to_string(),
            state_dir_arg.clone(),
            "init".to_string(),
            "--url".to_string(),
            url.clone(),
        ],
    )?;
    let status = appsec_state_status(root, &state_dir, true)?;
    Ok(serde_json::json!({
        "ok": init.get("ok").and_then(Value::as_bool) != Some(false),
        "command": "appsec_assessment_create",
        "module_id": APPSEC_MCP_MODULE_ID,
        "state_dir": state_dir_arg,
        "url": url,
        "init": init,
        "status": status,
    }))
}

fn appsec_lab_create(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_lab_create", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "lab".to_string(),
        "create".to_string(),
    ];
    if let Some(out) = optional_string_arg(arguments, "out") {
        let out_path = appsec_mcp_workspace_path(root, "out", &out)?;
        args.extend(["--out".to_string(), path_string(&out_path)]);
    }
    let output = crate::run_projected_appsec_command(root, &args)?;
    Ok(serde_json::json!({
        "ok": output.get("ok").and_then(Value::as_bool) != Some(false),
        "command": "appsec_lab_create",
        "module_id": APPSEC_MCP_MODULE_ID,
        "state_dir": path_string(&state_dir),
        "output": output,
    }))
}

fn appsec_lab_run(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_lab_run", arguments)?;
    let url = required_arg(arguments, "url")?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "lab".to_string(),
        "run".to_string(),
        "--url".to_string(),
        url.clone(),
    ];
    if let Some(profile) = optional_string_arg(arguments, "profile") {
        args.extend(["--profile".to_string(), profile]);
    }
    if arguments.get("rebuild_coverage").and_then(Value::as_bool) == Some(true) {
        args.push("--rebuild-coverage".to_string());
    }
    if arguments.get("report").and_then(Value::as_bool) == Some(false) {
        args.push("--no-report".to_string());
    }
    let output = crate::run_projected_appsec_command(root, &args)?;
    let status = appsec_state_status(root, &state_dir, true)?;
    Ok(serde_json::json!({
        "ok": output.get("ok").and_then(Value::as_bool) != Some(false),
        "command": "appsec_lab_run",
        "module_id": APPSEC_MCP_MODULE_ID,
        "state_dir": path_string(&state_dir),
        "url": url,
        "output": output,
        "status": status,
    }))
}

fn appsec_assessment_status(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_assessment_status", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let sync = arguments
        .get("sync")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut status = appsec_state_status(root, &state_dir, sync)?;
    if let Some(object) = status.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_assessment_status".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
    }
    Ok(status)
}

fn appsec_completion_review(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_completion_review", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let mut output = crate::run_projected_appsec_command(
        root,
        &[
            "--state-dir".to_string(),
            path_string(&state_dir),
            "review".to_string(),
        ],
    )?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_completion_review".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_tools_doctor(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_tools_doctor", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let profile = appsec_mcp_profile(arguments)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "tools".to_string(),
        "doctor".to_string(),
        "--profile".to_string(),
        profile,
    ];
    if optional_bool_arg(arguments, "probe_versions") {
        args.push("--probe-versions".to_string());
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_tools_doctor".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
    }
    Ok(output)
}

fn appsec_authz_plan(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_authz_plan", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let target = required_arg(arguments, "target")?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "authz".to_string(),
        "plan".to_string(),
        "--target".to_string(),
        target,
    ];
    if let Some(source_id) = optional_string_arg(arguments, "source_id") {
        args.push("--source-id".to_string());
        args.push(source_id);
    }
    if let Some(subjects) = optional_string_arg(arguments, "subjects") {
        let subjects_path = appsec_mcp_workspace_path(root, "subjects", &subjects)?;
        args.push("--subjects".to_string());
        args.push(path_string(&subjects_path));
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_plan".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
    }
    Ok(output)
}

fn appsec_authz_credential_proof_template(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(
        root,
        context,
        "appsec_authz_credential_proof_template",
        arguments,
    )?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let subjects = required_arg(arguments, "subjects")?;
    let subjects_path = appsec_mcp_workspace_path(root, "subjects", &subjects)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "authz".to_string(),
        "credential-proof-template".to_string(),
        "--subjects".to_string(),
        path_string(&subjects_path),
    ];
    if let Some(out) = optional_string_arg(arguments, "out") {
        let out_path = appsec_mcp_workspace_path(root, "out", &out)?;
        args.extend(["--out".to_string(), path_string(&out_path)]);
    }
    if optional_bool_arg(arguments, "force") {
        args.push("--force".to_string());
    }
    args.push("--json".to_string());
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_credential_proof_template".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_authz_preflight(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_authz_preflight", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let target = required_arg(arguments, "target")?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "authz".to_string(),
        "preflight".to_string(),
        "--target".to_string(),
        target,
    ];
    if let Some(source_id) = optional_string_arg(arguments, "source_id") {
        args.extend(["--source-id".to_string(), source_id]);
    }
    if let Some(subjects) = optional_string_arg(arguments, "subjects") {
        let subjects_path = appsec_mcp_workspace_path(root, "subjects", &subjects)?;
        args.extend(["--subjects".to_string(), path_string(&subjects_path)]);
    }
    if let Some(run) = optional_string_arg(arguments, "run") {
        let run_path = appsec_mcp_workspace_path(root, "run", &run)?;
        args.extend(["--run".to_string(), path_string(&run_path)]);
    }
    if let Some(evidence_dir) = optional_string_arg(arguments, "evidence_dir") {
        let evidence_dir = appsec_mcp_workspace_path(root, "evidence_dir", &evidence_dir)?;
        args.extend(["--evidence-dir".to_string(), path_string(&evidence_dir)]);
    }
    if let Some(credential_proof) = optional_string_arg(arguments, "credential_proof") {
        let proof_path = appsec_mcp_workspace_path(root, "credential_proof", &credential_proof)?;
        args.extend(["--credential-proof".to_string(), path_string(&proof_path)]);
    }
    if optional_bool_arg(arguments, "require_credentials") {
        args.push("--require-credentials".to_string());
    }
    if optional_bool_arg(arguments, "require_evidence") {
        args.push("--require-evidence".to_string());
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_preflight".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_authz_run(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_authz_run", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let target = required_arg(arguments, "target")?;
    let subjects = required_arg(arguments, "subjects")?;
    let subjects_path = appsec_mcp_workspace_path(root, "subjects", &subjects)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "authz".to_string(),
        "run".to_string(),
        "--target".to_string(),
        target,
        "--subjects".to_string(),
        path_string(&subjects_path),
    ];
    if let Some(source_id) = optional_string_arg(arguments, "source_id") {
        args.push("--source-id".to_string());
        args.push(source_id);
    }
    if let Some(credential_proof) = optional_string_arg(arguments, "credential_proof") {
        let proof_path = appsec_mcp_workspace_path(root, "credential_proof", &credential_proof)?;
        args.extend(["--credential-proof".to_string(), path_string(&proof_path)]);
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_run".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_authz_status(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_authz_status", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let mut output = crate::run_projected_appsec_command(
        root,
        &[
            "--state-dir".to_string(),
            path_string(&state_dir),
            "authz".to_string(),
            "status".to_string(),
            "--json".to_string(),
        ],
    )?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_status".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_authz_build_matrix(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_authz_build_matrix", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let run = required_arg(arguments, "run")?;
    let evidence_dir = required_arg(arguments, "evidence_dir")?;
    let run_path = appsec_mcp_workspace_path(root, "run", &run)?;
    let evidence_dir = appsec_mcp_workspace_path(root, "evidence_dir", &evidence_dir)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "authz".to_string(),
        "build-matrix".to_string(),
        "--run".to_string(),
        path_string(&run_path),
        "--evidence-dir".to_string(),
        path_string(&evidence_dir),
    ];
    if optional_bool_arg(arguments, "import") {
        args.push("--import".to_string());
    }
    if optional_bool_arg(arguments, "no_mark_coverage") {
        args.push("--no-mark-coverage".to_string());
    }
    if let Some(out) = optional_string_arg(arguments, "out") {
        let out_path = appsec_mcp_workspace_path(root, "out", &out)?;
        args.extend(["--out".to_string(), path_string(&out_path)]);
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_authz_build_matrix".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert(
            "state_dir".to_string(),
            Value::String(path_string(&state_dir)),
        );
    }
    Ok(output)
}

fn appsec_pipeline_rework(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_pipeline_rework", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let mut args = vec![
        "--state-dir".to_string(),
        path_string(&state_dir),
        "pipeline".to_string(),
        "rework".to_string(),
    ];
    if let Some(stage_id) = optional_string_arg(arguments, "stage_id") {
        args.extend(["--stage-id".to_string(), stage_id]);
    } else if let Some(phase) = optional_string_arg(arguments, "phase") {
        args.extend(["--phase".to_string(), phase]);
    } else {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "stage_id",
            "stage_id or phase is required",
        )));
    }
    if let Some(target) = optional_string_arg(arguments, "target") {
        args.extend(["--target".to_string(), target]);
    }
    if let Some(status) = optional_string_arg(arguments, "status") {
        args.extend(["--status".to_string(), status]);
    }
    args.extend(["--reason".to_string(), required_arg(arguments, "reason")?]);
    args.extend(["--operator".to_string(), context.actor.clone()]);
    let mut artifacts = optional_string_array_arg(arguments, "artifacts");
    if let Some(artifact) = optional_string_arg(arguments, "artifact") {
        artifacts.push(artifact);
    }
    if artifacts.is_empty() {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "artifact",
            "artifact or artifacts is required",
        )));
    }
    for artifact in artifacts {
        let artifact_path = appsec_mcp_workspace_path(root, "artifact", &artifact)?;
        args.extend(["--artifact".to_string(), path_string(&artifact_path)]);
    }
    let mut output = crate::run_projected_appsec_command(root, &args)?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_pipeline_rework".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
    }
    Ok(output)
}

fn appsec_report_get(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_report_get", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let format = optional_string_arg(arguments, "format").unwrap_or_else(|| "json".to_string());
    let (format, report_path) = match format.as_str() {
        "json" => (
            "json",
            state_dir.join("reports").join("pentest-report.json"),
        ),
        "markdown" | "md" => (
            "markdown",
            state_dir.join("reports").join("pentest-report.md"),
        ),
        other => {
            return Err(anyhow::Error::new(BusinessOsMcpError::validation(
                "format",
                format!("unsupported AppSec report format `{other}`"),
            )));
        }
    };
    if !report_path.is_file() {
        return Ok(serde_json::json!({
            "ok": true,
            "command": "appsec_report_get",
            "module_id": APPSEC_MCP_MODULE_ID,
            "status": "missing",
            "format": format,
            "state_dir": path_string(&state_dir),
            "report_path": path_string(&report_path),
            "note": "No existing report artifact was found. Generate a report through the AppSec CLI or pipeline before reading it through MCP.",
        }));
    }
    let content = fs::read_to_string(&report_path)
        .with_context(|| format!("failed to read AppSec report {}", report_path.display()))?;
    if format == "json" {
        let report: Value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse AppSec report {}", report_path.display()))?;
        Ok(serde_json::json!({
            "ok": true,
            "command": "appsec_report_get",
            "module_id": APPSEC_MCP_MODULE_ID,
            "status": "available",
            "format": format,
            "state_dir": path_string(&state_dir),
            "report_path": path_string(&report_path),
            "report": report,
        }))
    } else {
        Ok(serde_json::json!({
            "ok": true,
            "command": "appsec_report_get",
            "module_id": APPSEC_MCP_MODULE_ID,
            "status": "available",
            "format": format,
            "state_dir": path_string(&state_dir),
            "report_path": path_string(&report_path),
            "markdown": content,
        }))
    }
}

fn appsec_finding_get(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    enforce_business_os_mcp_policy(root, context, "appsec_finding_get", arguments)?;
    let state_dir = appsec_mcp_state_dir(root, arguments)?;
    let finding_id = required_arg(arguments, "finding_id")?;
    let mut output = crate::run_projected_appsec_command(
        root,
        &[
            "--state-dir".to_string(),
            path_string(&state_dir),
            "finding".to_string(),
            "show".to_string(),
            "--id".to_string(),
            finding_id.clone(),
        ],
    )?;
    if let Some(object) = output.as_object_mut() {
        object.insert(
            "mcp_tool".to_string(),
            Value::String("appsec_finding_get".to_string()),
        );
        object.insert(
            "module_id".to_string(),
            Value::String(APPSEC_MCP_MODULE_ID.to_string()),
        );
        object.insert("finding_id".to_string(), Value::String(finding_id));
    }
    Ok(output)
}

fn appsec_state_status(root: &Path, state_dir: &Path, sync: bool) -> anyhow::Result<Value> {
    let mut args = vec![
        "state".to_string(),
        "status".to_string(),
        "--state-dir".to_string(),
        path_string(state_dir),
    ];
    if sync {
        args.push("--sync".to_string());
    }
    crate::appsec_state::handle_state_command(root, &args)
}

fn appsec_mcp_profile(arguments: &Value) -> anyhow::Result<String> {
    let profile =
        optional_string_arg(arguments, "profile").unwrap_or_else(|| "standard".to_string());
    match profile.as_str() {
        "minimal" | "standard" | "full" => Ok(profile),
        other => Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "profile",
            format!("unsupported AppSec scanner profile `{other}`"),
        ))),
    }
}

fn appsec_mcp_state_dir(root: &Path, arguments: &Value) -> anyhow::Result<PathBuf> {
    match optional_string_arg(arguments, "state_dir") {
        Some(raw) => appsec_mcp_workspace_path(root, "state_dir", &raw),
        None => Ok(root.join("runtime/appsec/default")),
    }
}

fn appsec_mcp_workspace_path(root: &Path, field: &str, raw: &str) -> anyhow::Result<PathBuf> {
    let raw_path = PathBuf::from(raw);
    if raw_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            field,
            "path must not contain parent directory components",
        )));
    }
    let path = if raw_path.is_absolute() {
        raw_path
    } else {
        root.join(raw_path)
    };
    if !path.starts_with(root) {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            field,
            "path must stay inside the CTOX workspace",
        )));
    }
    Ok(path)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn get_record(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<BusinessOsMcpRecordResponse> {
    context.validate()?;
    ensure_non_empty("record_id", record_id)?;
    ensure_non_empty("collection", collection)?;
    enforce_collection_policy(root, collection)?;
    let payload = store::pull_collection_record(root, collection, record_id)?.ok_or_else(|| {
        BusinessOsMcpError::not_found(
            BusinessOsMcpErrorCode::RecordNotFound,
            format!("Business OS record `{record_id}` was not found in `{collection}`"),
        )
    })?;
    let record = record_summary_from_value(collection, payload);
    Ok(BusinessOsMcpRecordResponse { ok: true, record })
}

pub fn get_command_status(
    root: &Path,
    context: &McpChannelRequestContext,
    command_id: &str,
) -> anyhow::Result<BusinessOsMcpRecordResponse> {
    context.validate()?;
    ensure_non_empty("command_id", command_id)?;
    enforce_collection_policy(root, "business_commands")?;
    let payload =
        store::pull_business_command_status_record(root, command_id)?.ok_or_else(|| {
            BusinessOsMcpError::not_found(
                BusinessOsMcpErrorCode::RecordNotFound,
                format!("Business OS command `{command_id}` was not found"),
            )
        })?;
    Ok(BusinessOsMcpRecordResponse {
        ok: true,
        record: record_summary_from_value("business_commands", payload),
    })
}

pub fn get_record_context(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    record_id: &str,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpRecordContextResponse> {
    let record = get_record(root, context, collection, record_id)?.record;
    let limit = bounded_limit(limit);
    let activity = list_record_activity(root, context, collection, record_id, Some(limit))?;
    let commands = related_records(root, context, "business_commands", record_id, limit)?;
    let runs = related_records(root, context, "ctox_queue_tasks", record_id, limit)?;
    let approvals = related_records(root, context, "outbound_approvals", record_id, limit)?;
    Ok(BusinessOsMcpRecordContextResponse {
        ok: true,
        record,
        activity,
        commands,
        runs,
        approvals,
    })
}

pub fn list_record_activity(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    record_id: &str,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    context.validate()?;
    ensure_non_empty("collection", collection)?;
    ensure_non_empty("record_id", record_id)?;
    let limit = bounded_limit(limit);
    let mut items = Vec::new();
    for activity_collection in [
        "business_commands",
        "ctox_runs",
        "ctox_queue_tasks",
        "outbound_approvals",
        "outbound_messages",
    ] {
        let remaining = limit.saturating_sub(items.len());
        if remaining == 0 {
            break;
        }
        items.extend(
            related_records(root, context, activity_collection, record_id, remaining)?.items,
        );
    }
    sort_records_desc(&mut items);
    items.truncate(limit);
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

pub fn list_runs(
    root: &Path,
    context: &McpChannelRequestContext,
    status: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    context.validate()?;
    let limit = bounded_limit(limit);
    let mut items = Vec::new();
    for collection in ["ctox_runs", "ctox_queue_tasks"] {
        let remaining = limit.saturating_sub(items.len());
        if remaining == 0 {
            break;
        }
        let mut records = query_records(root, context, collection, Some(MAX_LIMIT))?.items;
        if let Some(status) = status.filter(|value| !value.trim().is_empty()) {
            records.retain(|record| {
                record.status.as_deref() == Some(status)
                    || string_field(&record.data, "status").as_deref() == Some(status)
            });
        }
        sort_records_desc(&mut records);
        records.truncate(remaining);
        items.extend(records);
    }
    sort_records_desc(&mut items);
    items.truncate(limit);
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

pub fn get_run(
    root: &Path,
    context: &McpChannelRequestContext,
    run_id: &str,
) -> anyhow::Result<BusinessOsMcpRecordResponse> {
    ensure_non_empty("run_id", run_id)?;
    get_record(root, context, "ctox_runs", run_id)
        .or_else(|_| get_record(root, context, "ctox_queue_tasks", run_id))
}

pub fn list_artifacts(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    context.validate()?;
    let limit = bounded_limit(limit);
    let collections = collection
        .map(|value| vec![value.to_string()])
        .unwrap_or_else(|| {
            vec![
                "desktop_files".to_string(),
                "documents".to_string(),
                "business_commands".to_string(),
                "ctox_queue_tasks".to_string(),
            ]
        });
    let mut items = Vec::new();
    for collection in collections {
        let remaining = limit.saturating_sub(items.len());
        if remaining == 0 {
            break;
        }
        let mut records = query_records(root, context, &collection, Some(MAX_LIMIT))?.items;
        if !is_artifact_collection(&collection) {
            records.retain(|record| record_contains_artifact(&record.data));
        }
        sort_records_desc(&mut records);
        records.truncate(remaining);
        items.extend(records);
    }
    sort_records_desc(&mut items);
    items.truncate(limit);
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

pub fn get_artifact(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: Option<&str>,
    artifact_id: &str,
) -> anyhow::Result<BusinessOsMcpArtifactResponse> {
    ensure_non_empty("artifact_id", artifact_id)?;
    let collections = collection
        .map(|value| vec![value.to_string()])
        .unwrap_or_else(|| {
            vec![
                "desktop_files".to_string(),
                "documents".to_string(),
                "business_commands".to_string(),
                "ctox_queue_tasks".to_string(),
            ]
        });
    for collection in collections {
        if let Ok(response) = get_record(root, context, &collection, artifact_id) {
            if is_artifact_collection(&collection)
                || record_contains_artifact(&response.record.data)
            {
                return Ok(BusinessOsMcpArtifactResponse {
                    ok: true,
                    artifact: response.record,
                });
            }
        }
    }
    Err(anyhow::Error::new(BusinessOsMcpError::not_found(
        BusinessOsMcpErrorCode::RecordNotFound,
        format!("Business OS artifact `{artifact_id}` was not found"),
    )))
}

pub fn list_approvals(
    root: &Path,
    context: &McpChannelRequestContext,
    status: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    context.validate()?;
    let limit = bounded_limit(limit);
    let mut items = Vec::new();
    let mut approval_records =
        query_records(root, context, "outbound_approvals", Some(MAX_LIMIT))?.items;
    if let Some(status) = status.filter(|value| !value.trim().is_empty()) {
        approval_records.retain(|record| {
            string_field(&record.data, "decision").as_deref() == Some(status)
                || record.status.as_deref() == Some(status)
        });
    }
    items.extend(approval_records);
    let mut message_records =
        query_records(root, context, "outbound_messages", Some(MAX_LIMIT))?.items;
    message_records.retain(|record| {
        let approval_status = string_field(&record.data, "approval_status");
        match status.filter(|value| !value.trim().is_empty()) {
            Some(status) => approval_status.as_deref() == Some(status),
            None => approval_status.is_some(),
        }
    });
    items.extend(message_records);
    sort_records_desc(&mut items);
    items.truncate(limit);
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

pub fn record_approval_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    decision: &str,
) -> anyhow::Result<BusinessOsApprovalDecision> {
    context.validate()?;
    if context.confirmation_state != McpConfirmationState::Approved {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ConfirmationRequired,
            message: "approval decision tools require explicit user approval".to_string(),
            field: Some("_context.confirmation_state".to_string()),
        }));
    }
    enforce_business_os_mcp_policy(root, context, context.tool.as_str(), arguments)?;
    let approval_id = optional_string_arg(arguments, "approval_id");
    let message_id = resolve_approval_message_id(root, context, arguments, approval_id.as_deref())?;
    let comment = optional_string_arg(arguments, "comment");
    let command_type = match decision {
        "approved" => "outbound.message.approve",
        "rejected" => "outbound.message.reject",
        "changes_requested" => "outbound.message.request_changes",
        other => {
            return Err(anyhow::Error::new(BusinessOsMcpError::validation(
                "decision",
                format!("unsupported approval decision `{other}`"),
            )));
        }
    };
    let client_context = serde_json::json!({
        "channel": &context.channel,
        "surface": &context.surface,
        "actor": resolved_mcp_actor_context(root, context)?,
        "mcp_actor": &context.actor,
        "workspace": &context.workspace,
        "request_id": &context.request_id,
        "confirmation_state": confirmation_state_as_str(&context.confirmation_state),
        "mcp_tool": &context.tool
    });
    let accepted = store::record_command(
        root,
        store::BusinessCommand {
            origin: store::CommandOrigin::TrustedLocal,
            id: None,
            module: "outbound".to_string(),
            command_type: command_type.to_string(),
            record_id: Some(message_id.clone()),
            payload: serde_json::json!({
                "message_id": message_id.clone(),
                "approval_id": approval_id.clone(),
                "comment": comment
            }),
            client_context: client_context.clone(),
        },
    )?;
    Ok(BusinessOsApprovalDecision {
        ok: accepted.ok,
        decision: decision.to_string(),
        message_id,
        approval_id,
        command_id: accepted.command_id,
        status: accepted.status.to_string(),
        task_id: accepted.task_id,
        task_status: accepted.task_status,
        client_context,
    })
}

pub fn open_link(kind: &str, module_or_collection: &str, id: Option<&str>) -> BusinessOsDeepLink {
    let mut path = format!("/{kind}/{}", encode_link_part(module_or_collection));
    let mut fragment = format!("module={}", encode_link_part(module_or_collection));
    if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
        path.push('/');
        path.push_str(&encode_link_part(id));
        fragment.push_str("&record=");
        fragment.push_str(&encode_link_part(id));
    }
    BusinessOsDeepLink {
        kind: kind.to_string(),
        path,
        url_fragment: format!("#{fragment}"),
    }
}

pub fn list_mcp_activity(
    root: &Path,
    context: &McpChannelRequestContext,
    limit: Option<usize>,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsMcpAuditEvent>> {
    context.validate()?;
    let limit = bounded_limit(limit);
    let conn = store::open_store(root)?;
    let mut statement = conn.prepare(
        "SELECT event_id, channel, surface, actor, workspace, tool, request_id,
                confirmation_state, status, error_code, error_message,
                metadata_json, created_at_ms
         FROM business_os_mcp_events
         ORDER BY created_at_ms DESC, event_id DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let confirmation_state_raw: String = row.get(7)?;
        let metadata_json: String = row.get(11)?;
        Ok(BusinessOsMcpAuditEvent {
            event_id: row.get(0)?,
            channel: row.get(1)?,
            surface: row.get(2)?,
            actor: row.get(3)?,
            workspace: row.get(4)?,
            tool: row.get(5)?,
            request_id: row.get(6)?,
            confirmation_state: confirmation_state_from_str(&confirmation_state_raw),
            status: row.get(8)?,
            error_code: row.get(9)?,
            error_message: row.get(10)?,
            metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
            created_at_ms: row.get(12)?,
        })
    })?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

pub fn export_mcp_activity(
    root: &Path,
    context: &McpChannelRequestContext,
    limit: Option<usize>,
    format: BusinessOsMcpAuditExportFormat,
) -> anyhow::Result<String> {
    let events = list_mcp_activity(root, context, limit)?;
    match format {
        BusinessOsMcpAuditExportFormat::Json => {
            serde_json::to_string_pretty(&events).context("failed to serialize MCP audit export")
        }
        BusinessOsMcpAuditExportFormat::Jsonl => {
            let mut output = String::new();
            for event in events.items {
                output.push_str(
                    &serde_json::to_string(&event)
                        .context("failed to serialize MCP audit event")?,
                );
                output.push('\n');
            }
            Ok(output)
        }
    }
}

pub fn list_module_actions(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsActionDescriptor>> {
    let module = get_module(root, context, module_id)?;
    let mut actions = vec![generic_delegate_action(&module.id)];
    actions.extend(match module.id.as_str() {
        "tickets" => vec![
            action_descriptor(
                "tickets.create_followup",
                &module.id,
                "Create ticket follow-up",
                "Prepare a CTOX follow-up for a ticket or internal work item.",
                "write",
                false,
                false,
            ),
            action_descriptor(
                "tickets.request_review",
                &module.id,
                "Request ticket review",
                "Prepare a review request for ticket state or outcome evidence.",
                "write",
                false,
                false,
            ),
        ],
        "knowledge" => vec![action_descriptor(
            "knowledge.create_note_candidate",
            &module.id,
            "Create knowledge note candidate",
            "Prepare a Knowledge note candidate from the current context.",
            "write",
            false,
            false,
        )],
        "customers" => vec![
            action_descriptor(
                "customers.create_followup",
                &module.id,
                "Create customer follow-up",
                "Prepare a customer follow-up task without external communication.",
                "write",
                false,
                false,
            ),
            action_descriptor(
                "customers.propose_update",
                &module.id,
                "Propose customer update",
                "Prepare a customer record update for review.",
                "write",
                false,
                false,
            ),
        ],
        "matching" => vec![action_descriptor(
            "matching.run_match",
            &module.id,
            "Run matching workflow",
            "Prepare a matching workflow command for CTOX execution.",
            "long_running",
            true,
            false,
        )],
        "outbound" => vec![
            action_descriptor(
                "outbound.draft_message",
                &module.id,
                "Draft outbound message",
                "Prepare an outbound message draft without sending.",
                "write",
                false,
                false,
            ),
            action_descriptor(
                "outbound.request_send_approval",
                &module.id,
                "Request send approval",
                "Prepare an approval request for an outbound send action.",
                "external_effect",
                true,
                true,
            ),
        ],
        "support" => vec![
            action_descriptor(
                "support.agent.writeback",
                &module.id,
                "Write Support suggestion",
                "Write a structured Support Agent suggestion for human review.",
                "write",
                false,
                false,
            ),
            action_descriptor(
                "support.agent.apply_suggestion",
                &module.id,
                "Apply Support suggestion",
                "Mark a Support Agent suggestion as applied after human review.",
                "write",
                false,
                false,
            ),
            action_descriptor(
                "support.agent.reject_suggestion",
                &module.id,
                "Reject Support suggestion",
                "Mark a Support Agent suggestion as rejected after human review.",
                "write",
                false,
                false,
            ),
        ],
        _ => Vec::new(),
    });
    Ok(BusinessOsMcpList {
        ok: true,
        count: actions.len(),
        limit: actions.len(),
        items: actions,
    })
}

pub fn propose_action(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    action_id: &str,
    arguments: &Value,
) -> anyhow::Result<BusinessOsActionProposal> {
    context.validate()?;
    let actions = list_module_actions(root, context, module_id)?;
    let action = actions
        .items
        .into_iter()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| {
            anyhow::Error::new(BusinessOsMcpError::not_found(
                BusinessOsMcpErrorCode::ActionNotAllowed,
                format!("Business OS action `{action_id}` is not allowed for module `{module_id}`"),
            ))
        })?;
    let record_id = optional_string_arg(arguments, "record_id");
    let title = optional_string_arg(arguments, "title").unwrap_or_else(|| action.title.clone());
    let objective =
        optional_string_arg(arguments, "objective").unwrap_or_else(|| action.description.clone());
    let payload = arguments
        .get("payload")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if module_id == "support" && action.action_id.starts_with("support.agent.") {
        let support_payload = support_agent_action_payload(arguments, &record_id, payload);
        return Ok(BusinessOsActionProposal {
            ok: true,
            command_type: action.action_id.clone(),
            payload: support_payload,
            client_context: serde_json::json!({
                "channel": &context.channel,
                "surface": &context.surface,
                "actor": resolved_mcp_actor_context(root, context)?,
                "mcp_actor": &context.actor,
                "workspace": &context.workspace,
                "request_id": &context.request_id,
                "requires_confirmation": action.confirmation_required,
                "proposal_only": true,
                "writeback_contract": "support.agent"
            }),
            confirmation_required: action.confirmation_required,
            would_execute: false,
            module_id: module_id.to_string(),
            record_id,
            action,
        });
    }
    Ok(BusinessOsActionProposal {
        ok: true,
        command_type: action.action_id.clone(),
        payload: serde_json::json!({
            "title": title,
            "objective": objective,
            "record_id": record_id.clone(),
            "input": payload
        }),
        client_context: serde_json::json!({
            "channel": &context.channel,
            "surface": &context.surface,
            "actor": resolved_mcp_actor_context(root, context)?,
            "mcp_actor": &context.actor,
            "workspace": &context.workspace,
            "request_id": &context.request_id,
            "requires_confirmation": action.confirmation_required,
            "proposal_only": true
        }),
        confirmation_required: action.confirmation_required,
        would_execute: false,
        module_id: module_id.to_string(),
        record_id,
        action,
    })
}

pub fn execute_action(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    action_id: &str,
    arguments: &Value,
) -> anyhow::Result<BusinessOsActionExecution> {
    let policy_arguments = arguments_with_module_id(arguments, module_id);
    enforce_business_os_mcp_policy(
        root,
        context,
        "business_os.execute_action",
        &policy_arguments,
    )?;
    let proposal = propose_action(root, context, module_id, action_id, arguments)?;
    if proposal.confirmation_required
        && context.confirmation_state != McpConfirmationState::Approved
    {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ConfirmationRequired,
            message: format!(
                "Business OS action `{action_id}` requires explicit approval before execution"
            ),
            field: Some("_context.confirmation_state".to_string()),
        }));
    }
    if proposal.action.external_effect {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ExternalEffectBlocked,
            message: format!(
                "Business OS action `{action_id}` may create an external effect and is blocked in MCP Channel v1"
            ),
            field: Some("action_id".to_string()),
        }));
    }
    let client_context = serde_json::json!({
        "channel": &context.channel,
        "surface": &context.surface,
        "actor": resolved_mcp_actor_context(root, context)?,
        "mcp_actor": &context.actor,
        "workspace": &context.workspace,
        "request_id": &context.request_id,
        "requires_confirmation": proposal.confirmation_required,
        "confirmation_state": confirmation_state_as_str(&context.confirmation_state),
        "proposal_only": false,
        "mcp_tool": &context.tool
    });
    let accepted = store::record_command(
        root,
        store::BusinessCommand {
            origin: store::CommandOrigin::TrustedLocal,
            id: None,
            module: module_id.to_string(),
            command_type: proposal.command_type.clone(),
            record_id: proposal.record_id.clone(),
            payload: proposal.payload.clone(),
            client_context: client_context.clone(),
        },
    )?;
    Ok(BusinessOsActionExecution {
        ok: accepted.ok,
        action: proposal.action,
        module_id: module_id.to_string(),
        record_id: proposal.record_id,
        command_type: proposal.command_type,
        command_id: accepted.command_id,
        status: accepted.status.to_string(),
        task_id: accepted.task_id,
        task_status: accepted.task_status,
        confirmation_required: proposal.confirmation_required,
        client_context,
    })
}

fn ensure_non_empty(field: &str, value: &str) -> Result<(), BusinessOsMcpError> {
    if value.trim().is_empty() {
        return Err(BusinessOsMcpError::validation(
            field,
            format!("{field} is required"),
        ));
    }
    Ok(())
}

fn support_agent_action_payload(
    arguments: &Value,
    record_id: &Option<String>,
    payload: Value,
) -> Value {
    let mut payload = match payload {
        Value::Object(map) => Value::Object(map),
        _ => serde_json::json!({}),
    };
    if let Some(object) = payload.as_object_mut() {
        let conversation_id = object
            .get("conversation_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                arguments
                    .get("conversation_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
            })
            .or_else(|| record_id.clone());
        if let Some(conversation_id) = conversation_id {
            object
                .entry("conversation_id".to_string())
                .or_insert(Value::String(conversation_id));
        }
    }
    payload
}

fn handle_mcp_http_request(root: &Path, mut request: Request) -> anyhow::Result<()> {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    if method == Method::Options {
        respond_json_value(request, serde_json::json!({ "ok": true }))?;
        return Ok(());
    }
    match (method, path.as_str()) {
        (Method::Get, "/health") => {
            respond_json_value(
                request,
                serde_json::json!({
                    "ok": true,
                    "service": "ctox-business-os-mcp",
                    "endpoint": "/mcp"
                }),
            )?;
        }
        (Method::Post, "/mcp") => {
            if !mcp_request_authorized(root, &request) {
                respond_json_status(
                    request,
                    401,
                    serde_json::json!({
                        "ok": false,
                        "error": "unauthorized",
                        "message": "Business OS MCP requires a valid Authorization: Bearer token."
                    }),
                )?;
                return Ok(());
            }
            let body = read_json(&mut request)?;
            let response = handle_json_rpc(root, body);
            respond_json_value(request, response)?;
        }
        _ => respond_json_status(
            request,
            404,
            serde_json::json!({
                "ok": false,
                "error": "not_found"
            }),
        )?,
    }
    Ok(())
}

fn handle_json_rpc(root: &Path, body: Value) -> Value {
    handle_json_rpc_with_gateway_context(root, body, None)
}

fn handle_json_rpc_with_gateway_context(
    root: &Path,
    body: Value,
    trusted_gateway_context: Option<&Value>,
) -> Value {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "ctox-business-os-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(serde_json::json!({
            "tools": tool_descriptors()
        })),
        "tools/call" => {
            let params = body
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            call_tool_audited_with_trusted_gateway_context(
                root,
                name,
                arguments,
                trusted_gateway_context,
            )
            .and_then(mcp_tool_result)
        }
        _ => Err(anyhow::anyhow!("unsupported JSON-RPC method `{method}`")),
    };
    match result {
        Ok(result) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }),
        Err(error) => json_rpc_error_response(id, error),
    }
}

fn is_json_rpc_notification(body: &Value) -> bool {
    body.get("id").is_none()
        && body
            .get("method")
            .and_then(Value::as_str)
            .map(|method| method.starts_with("notifications/"))
            .unwrap_or(false)
}

fn inject_gateway_context(body: &mut Value, context: &Value) {
    if !context.is_object() {
        return;
    }
    match body {
        Value::Array(items) => {
            for item in items {
                inject_gateway_context(item, context);
            }
        }
        Value::Object(map) if map.get("method").and_then(Value::as_str) == Some("tools/call") => {
            let params = map.entry("params").or_insert_with(|| serde_json::json!({}));
            if !params.is_object() {
                *params = serde_json::json!({});
            }
            if let Some(params_map) = params.as_object_mut() {
                let arguments = params_map
                    .entry("arguments")
                    .or_insert_with(|| serde_json::json!({}));
                if !arguments.is_object() {
                    *arguments = serde_json::json!({});
                }
                if let Some(arguments_map) = arguments.as_object_mut() {
                    arguments_map.insert("_context".to_string(), context.clone());
                }
            }
        }
        _ => {}
    }
}

fn json_rpc_error_response(id: Value, error: anyhow::Error) -> Value {
    if let Some(typed) = error.downcast_ref::<BusinessOsMcpError>() {
        let code = business_os_error_json_rpc_code(&typed.code);
        let mut data = serde_json::json!({
            "code": typed_error_code_value(&typed.code),
            "type": "business_os_mcp_error"
        });
        if let Some(field) = &typed.field {
            data["field"] = Value::String(field.clone());
        }
        return serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": typed.message.clone(),
                "data": data
            }
        });
    }
    let message = error.to_string();
    if message.starts_with("unsupported JSON-RPC method") {
        return serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": message,
                "data": {
                    "code": "method_not_found",
                    "type": "json_rpc_error"
                }
            }
        });
    }
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32603,
            "message": message,
            "data": {
                "code": "internal_error",
                "type": "business_os_mcp_error"
            }
        }
    })
}

fn business_os_error_json_rpc_code(code: &BusinessOsMcpErrorCode) -> i64 {
    match code {
        BusinessOsMcpErrorCode::ValidationFailed => -32602,
        BusinessOsMcpErrorCode::ActionNotAllowed => -32601,
        BusinessOsMcpErrorCode::ModuleNotFound
        | BusinessOsMcpErrorCode::EntityNotFound
        | BusinessOsMcpErrorCode::RecordNotFound => -32004,
        BusinessOsMcpErrorCode::NotAuthenticated | BusinessOsMcpErrorCode::NotAuthorized => -32001,
        BusinessOsMcpErrorCode::PermissionDenied
        | BusinessOsMcpErrorCode::ChannelDisabled
        | BusinessOsMcpErrorCode::ConfirmationRequired
        | BusinessOsMcpErrorCode::ExternalEffectBlocked => -32003,
        BusinessOsMcpErrorCode::RateLimited => -32029,
        BusinessOsMcpErrorCode::ResponseTooLarge => -32013,
        BusinessOsMcpErrorCode::SyncNotReady | BusinessOsMcpErrorCode::RuntimeUnavailable => -32002,
    }
}

fn typed_error_code_value(code: &BusinessOsMcpErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{code:?}"))
}

fn mcp_tool_result(value: Value) -> anyhow::Result<Value> {
    let value = redact_mcp_response(value);
    ensure_mcp_response_size(&value)?;
    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
        }],
        "structuredContent": value
    }))
}

fn read_json(request: &mut Request) -> anyhow::Result<Value> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    if body.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&body).context("invalid JSON request body")
}

fn respond_json_value(request: Request, value: Value) -> anyhow::Result<()> {
    respond_json_status(request, 200, value)
}

fn respond_json_status(request: Request, status: u16, value: Value) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(&value)?;
    let mut response = Response::from_string(body).with_status_code(status);
    response.add_header(
        Header::from_bytes(
            &b"Content-Type"[..],
            &b"application/json; charset=utf-8"[..],
        )
        .map_err(|_| anyhow::anyhow!("failed to build content-type header"))?,
    );
    // No CORS allow-origin: the MCP endpoint is a token-authenticated control
    // surface for non-browser agents, not a web API. Omitting ACAO prevents a
    // malicious web page in a local browser from reading responses cross-origin
    // (drive-by against a loopback-bound MCP server).
    request
        .respond(response)
        .map_err(|error| anyhow::anyhow!("failed to send response: {error}"))
}

fn bounded_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn ensure_mcp_response_size(value: &Value) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(value).context("failed to measure MCP response size")?;
    if bytes.len() > MAX_MCP_RESPONSE_BYTES {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ResponseTooLarge,
            message: format!(
                "Business OS MCP response is too large: {} bytes exceeds {} bytes",
                bytes.len(),
                MAX_MCP_RESPONSE_BYTES
            ),
            field: Some("response".to_string()),
        }));
    }
    Ok(())
}

pub fn mcp_policy(root: &Path) -> BusinessOsMcpPolicy {
    if let Ok(Some(payload)) = crate::persistence::load_json_payload::<BusinessOsMcpPolicyPayload>(
        root,
        MCP_POLICY_PAYLOAD_KEY,
    ) {
        return normalize_mcp_policy(payload.policy);
    }
    legacy_mcp_policy(root)
}

pub fn save_mcp_policy(root: &Path, policy: &BusinessOsMcpPolicy) -> anyhow::Result<()> {
    let payload = BusinessOsMcpPolicyPayload {
        schema_version: 1,
        policy: normalize_mcp_policy(policy.clone()),
    };
    crate::persistence::store_json_payload(root, MCP_POLICY_PAYLOAD_KEY, Some(&payload))
}

pub fn default_mcp_policy() -> BusinessOsMcpPolicy {
    BusinessOsMcpPolicy {
        enabled: true,
        allow_reads: true,
        allow_writes: true,
        allow_approvals: true,
        allow_external_effects: false,
        rate_limit_per_minute: DEFAULT_RATE_LIMIT_PER_MINUTE,
        audit_retention_days: DEFAULT_AUDIT_RETENTION_DAYS,
        allowed_actors: Vec::new(),
        allowed_workspaces: Vec::new(),
        allowed_modules: Vec::new(),
        allowed_collections: Vec::new(),
        denied_tools: Vec::new(),
    }
}

fn legacy_mcp_policy(root: &Path) -> BusinessOsMcpPolicy {
    let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| Default::default());
    mcp_policy_from_env_map(&env_map)
}

fn mcp_policy_from_env_map(env_map: &BTreeMap<String, String>) -> BusinessOsMcpPolicy {
    let denied_tools = env_map
        .get("CTOX_BUSINESS_OS_MCP_DENY_TOOLS")
        .map(|value| split_csv(value))
        .unwrap_or_default();
    BusinessOsMcpPolicy {
        enabled: env_bool(&env_map, "CTOX_BUSINESS_OS_MCP_ENABLED", true),
        allow_reads: env_bool(&env_map, "CTOX_BUSINESS_OS_MCP_ALLOW_READS", true),
        allow_writes: env_bool(&env_map, "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES", true),
        allow_approvals: env_bool(&env_map, "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS", true),
        allow_external_effects: env_bool(
            &env_map,
            "CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS",
            false,
        ),
        rate_limit_per_minute: env_usize(
            &env_map,
            "CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE",
            DEFAULT_RATE_LIMIT_PER_MINUTE,
        ),
        audit_retention_days: env_usize(
            &env_map,
            "CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS",
            DEFAULT_AUDIT_RETENTION_DAYS,
        ),
        allowed_actors: env_map
            .get("CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        allowed_workspaces: env_map
            .get("CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        allowed_modules: env_map
            .get("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        allowed_collections: env_map
            .get("CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        denied_tools,
    }
}

fn normalize_mcp_policy(mut policy: BusinessOsMcpPolicy) -> BusinessOsMcpPolicy {
    policy.allowed_actors = dedupe_policy_values(policy.allowed_actors);
    policy.allowed_workspaces = dedupe_policy_values(policy.allowed_workspaces);
    policy.allowed_modules = dedupe_policy_values(policy.allowed_modules);
    policy.allowed_collections = dedupe_policy_values(policy.allowed_collections);
    policy.denied_tools = dedupe_policy_values(policy.denied_tools);
    policy
}

fn dedupe_policy_values(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            if seen.insert(value.clone()) {
                Some(value)
            } else {
                None
            }
        })
        .collect()
}

fn enforce_tool_policy(root: &Path, tool_name: &str) -> anyhow::Result<()> {
    let policy = mcp_policy(root);
    if !policy.enabled {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ChannelDisabled,
            message: "Business OS MCP channel is disabled by policy".to_string(),
            field: Some("CTOX_BUSINESS_OS_MCP_ENABLED".to_string()),
        }));
    }
    if policy.denied_tools.iter().any(|tool| tool == tool_name) {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::PermissionDenied,
            message: format!("Business OS MCP tool `{tool_name}` is denied by policy"),
            field: Some("CTOX_BUSINESS_OS_MCP_DENY_TOOLS".to_string()),
        }));
    }
    match tool_policy_class(tool_name) {
        McpToolPolicyClass::Read if !policy.allow_reads => Err(policy_denied(
            "read tools are disabled by policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_READS",
        )),
        McpToolPolicyClass::Write if !policy.allow_writes => Err(policy_denied(
            "write tools are disabled by policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES",
        )),
        McpToolPolicyClass::Approval if !policy.allow_writes => Err(policy_denied(
            "approval tools require write policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES",
        )),
        McpToolPolicyClass::Approval if !policy.allow_approvals => Err(policy_denied(
            "approval tools are disabled by policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS",
        )),
        McpToolPolicyClass::ExternalEffect if !policy.allow_writes => Err(policy_denied(
            "external-effect tools require write policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_WRITES",
        )),
        McpToolPolicyClass::ExternalEffect if !policy.allow_approvals => Err(policy_denied(
            "external-effect tools require approval policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS",
        )),
        McpToolPolicyClass::ExternalEffect if !policy.allow_external_effects => Err(policy_denied(
            "external-effect tools are disabled by policy",
            "CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS",
        )),
        _ => Ok(()),
    }
}

fn policy_denied(message: &str, field: &str) -> anyhow::Error {
    anyhow::Error::new(BusinessOsMcpError {
        code: BusinessOsMcpErrorCode::PermissionDenied,
        message: message.to_string(),
        field: Some(field.to_string()),
    })
}

fn enforce_business_os_mcp_policy(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    arguments: &Value,
) -> anyhow::Result<()> {
    let Some(decision) = business_os_mcp_policy_decision(root, context, tool_name, arguments)?
    else {
        return Ok(());
    };
    if decision.allowed {
        return Ok(());
    }
    Err(anyhow::Error::new(BusinessOsMcpError {
        code: BusinessOsMcpErrorCode::PermissionDenied,
        message: decision.display_reason.to_string(),
        field: Some("business_os_policy".to_string()),
    }))
}

fn trusted_mcp_actor_policy_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    permission: BusinessOsPermission,
    scope_type: BusinessOsScopeType,
    scope_id: Option<&str>,
) -> anyhow::Result<PolicyDecision> {
    if let Some(role) = context.trusted_role.as_deref() {
        return store::trusted_mcp_actor_policy_decision_with_role(
            root,
            &context.actor,
            role,
            permission,
            scope_type,
            scope_id,
        );
    }
    store::trusted_mcp_actor_policy_decision(
        root,
        &context.actor,
        &context.actor,
        permission,
        scope_type,
        scope_id,
    )
}

fn business_os_mcp_policy_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    arguments: &Value,
) -> anyhow::Result<Option<PolicyDecision>> {
    match tool_name {
        "business_os.status" | "business_os.list_mcp_activity" => {
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::McpManage,
                BusinessOsScopeType::Mcp,
                Some("business_os_mcp"),
            )?))
        }
        "business_os.get_module"
        | "business_os.list_entities"
        | "business_os.list_module_actions"
        | "business_os.propose_action" => {
            let module_id = required_arg(arguments, "module_id")?;
            Ok(Some(business_os_mcp_module_data_decision(
                root,
                context,
                &module_id,
                BusinessOsPermission::DataRead,
            )?))
        }
        "business_os.query_records"
        | "business_os.search_records"
        | "business_os.get_record_context"
        | "business_os.list_record_activity" => {
            let collection = required_arg(arguments, "collection")?;
            Ok(Some(business_os_mcp_collection_read_decision(
                root,
                context,
                &collection,
            )?))
        }
        "business_os.get_record" => {
            let collection = required_arg(arguments, "collection")?;
            let record_id = required_arg(arguments, "record_id")?;
            Ok(Some(business_os_mcp_record_read_decision(
                root,
                context,
                &collection,
                &record_id,
            )?))
        }
        "business_os.upsert_record" => {
            let collection = required_arg(arguments, "collection")?;
            Ok(Some(business_os_mcp_collection_write_decision(
                root,
                context,
                &collection,
            )?))
        }
        "business_os.upsert_user" => Ok(Some(trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::UsersManage,
            BusinessOsScopeType::Workspace,
            None,
        )?)),
        "business_os.list_runs" | "business_os.get_run" => Ok(Some(
            business_os_mcp_collection_read_decision(root, context, "ctox_queue_tasks")?,
        )),
        "business_os.list_artifacts" | "business_os.get_artifact" => {
            if let Some(collection) = optional_string_arg(arguments, "collection") {
                Ok(Some(business_os_mcp_collection_read_decision(
                    root,
                    context,
                    &collection,
                )?))
            } else {
                Ok(Some(trusted_mcp_actor_policy_decision(
                    root,
                    context,
                    BusinessOsPermission::DataRead,
                    BusinessOsScopeType::Workspace,
                    None,
                )?))
            }
        }
        "business_os.list_approvals" => Ok(Some(business_os_mcp_collection_read_decision(
            root,
            context,
            "outbound_approvals",
        )?)),
        "business_os.execute_action" => {
            let module_id = required_arg(arguments, "module_id")?;
            Ok(Some(business_os_mcp_module_data_decision(
                root,
                context,
                &module_id,
                BusinessOsPermission::DataWrite,
            )?))
        }
        "business_os.create_app" => {
            let instruction = required_arg(arguments, "instruction")?;
            let module_id = app_module_id_from_arguments(arguments, &instruction)?;
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::AppsInstall,
                BusinessOsScopeType::Module,
                Some(module_id.as_str()),
            )?))
        }
        "business_os.prepare_app_source" => {
            let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::AppsInstall,
                BusinessOsScopeType::Module,
                Some(module_id.as_str()),
            )?))
        }
        "business_os.modify_app" => {
            let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::AppsModify,
                BusinessOsScopeType::Module,
                Some(module_id.as_str()),
            )?))
        }
        "business_os.list_app_files"
        | "business_os.read_app_file"
        | "business_os.search_app_source" => {
            let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::AppsSourceView,
                BusinessOsScopeType::Module,
                Some(module_id.as_str()),
            )?))
        }
        "business_os.write_app_file"
        | "business_os.validate_app"
        | "business_os.smoke_app"
        | "business_os.e2e_app" => {
            let module_id = sanitize_app_module_id(&required_arg(arguments, "module_id")?)?;
            Ok(Some(trusted_mcp_actor_policy_decision(
                root,
                context,
                BusinessOsPermission::AppsModify,
                BusinessOsScopeType::Module,
                Some(module_id.as_str()),
            )?))
        }
        "business_os.approve" | "business_os.reject" | "business_os.request_changes" => Ok(Some(
            business_os_mcp_approval_decision(root, context, arguments)?,
        )),
        "business_os.get_command_status" => Ok(Some(business_os_mcp_collection_read_decision(
            root,
            context,
            "business_commands",
        )?)),
        "business_os.open_link" => {
            let target = required_arg(arguments, "module_or_collection")?;
            if string_field(arguments, "kind").as_deref() == Some("module") {
                Ok(Some(business_os_mcp_module_visibility_decision(
                    root, context, &target,
                )?))
            } else {
                Ok(Some(business_os_mcp_collection_read_decision(
                    root, context, &target,
                )?))
            }
        }
        "appsec_assessment_create"
        | "appsec_lab_create"
        | "appsec_lab_run"
        | "appsec_authz_plan"
        | "appsec_authz_credential_proof_template"
        | "appsec_authz_preflight"
        | "appsec_authz_run"
        | "appsec_authz_build_matrix"
        | "appsec_pipeline_rework" => Ok(Some(trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::DataWrite,
            BusinessOsScopeType::Module,
            Some(APPSEC_MCP_MODULE_ID),
        )?)),
        "appsec_assessment_status"
        | "appsec_completion_review"
        | "appsec_tools_doctor"
        | "appsec_authz_status"
        | "appsec_report_get"
        | "appsec_finding_get" => Ok(Some(trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::DataRead,
            BusinessOsScopeType::Module,
            Some(APPSEC_MCP_MODULE_ID),
        )?)),
        _ => Ok(None),
    }
}

fn business_os_mcp_module_data_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
    permission: BusinessOsPermission,
) -> anyhow::Result<PolicyDecision> {
    let visibility_decision = business_os_mcp_module_visibility_decision(root, context, module_id)?;
    if !visibility_decision.allowed {
        return Ok(visibility_decision);
    }
    trusted_mcp_actor_policy_decision(
        root,
        context,
        permission,
        BusinessOsScopeType::Module,
        Some(module_id),
    )
}

fn business_os_mcp_module_visibility_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    module_id: &str,
) -> anyhow::Result<PolicyDecision> {
    if module_public_for_mcp_actor(root, module_id)? {
        let scope = BusinessOsScope::module(module_id.trim(), false);
        return Ok(allow_decision(BusinessOsPermission::AppsView, &scope));
    }
    trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::AppsView,
        BusinessOsScopeType::Module,
        Some(module_id),
    )
}

fn module_public_for_mcp_actor(root: &Path, module_id: &str) -> anyhow::Result<bool> {
    let catalog = store::module_catalog_for_rxdb(root)?;
    let modules = catalog
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(modules
        .iter()
        .find(|module| string_field(module, "id").as_deref() == Some(module_id.trim()))
        .is_some_and(module_value_is_public_for_mcp))
}

fn module_value_visible_to_mcp_actor(
    root: &Path,
    context: &McpChannelRequestContext,
    module: &Value,
) -> bool {
    let module_id = string_field(module, "id").unwrap_or_default();
    if module_id.trim().is_empty() {
        return false;
    }
    if module_value_is_public_for_mcp(module) {
        return true;
    }
    trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::AppsView,
        BusinessOsScopeType::Module,
        Some(module_id.as_str()),
    )
    .map(|decision| decision.allowed)
    .unwrap_or(false)
}

fn module_value_is_public_for_mcp(module: &Value) -> bool {
    if let Some(public) = module
        .get("lifecycle")
        .and_then(|lifecycle| lifecycle.get("public"))
        .and_then(Value::as_bool)
    {
        return public;
    }
    let install_scope = string_field(module, "install_scope").unwrap_or_default();
    let entry = string_field(module, "entry").unwrap_or_default();
    module.get("core").and_then(Value::as_bool).unwrap_or(false)
        || (install_scope.trim() != "installed" && !entry.trim().starts_with("installed-modules/"))
}

fn business_os_mcp_collection_read_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
) -> anyhow::Result<PolicyDecision> {
    let collection_decision = trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::DataRead,
        BusinessOsScopeType::Collection,
        Some(collection),
    )?;
    if collection_decision.allowed {
        return Ok(collection_decision);
    }

    for module_id in module_ids_for_collection(root, collection)? {
        let module_decision = trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::DataRead,
            BusinessOsScopeType::Module,
            Some(module_id.as_str()),
        )?;
        if module_decision.allowed {
            return Ok(module_decision);
        }
    }

    Ok(collection_decision)
}

fn business_os_mcp_record_read_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<PolicyDecision> {
    let scope_id = record_scope_id(collection, record_id);
    let record_decision = trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::DataRead,
        BusinessOsScopeType::Record,
        Some(scope_id.as_str()),
    )?;
    if record_decision.allowed {
        return Ok(record_decision);
    }

    let collection_decision = business_os_mcp_collection_read_decision(root, context, collection)?;
    if collection_decision.allowed {
        return Ok(collection_decision);
    }

    Ok(record_decision)
}

fn business_os_mcp_collection_write_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
) -> anyhow::Result<PolicyDecision> {
    let collection_decision = trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::DataWrite,
        BusinessOsScopeType::Collection,
        Some(collection),
    )?;
    if collection_decision.allowed {
        return Ok(collection_decision);
    }

    for module_id in module_ids_for_collection(root, collection)? {
        let module_decision = trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::DataWrite,
            BusinessOsScopeType::Module,
            Some(module_id.as_str()),
        )?;
        if module_decision.allowed {
            return Ok(module_decision);
        }
    }

    Ok(collection_decision)
}

fn business_os_mcp_approval_decision(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<PolicyDecision> {
    if let Some(approval_id) = optional_string_arg(arguments, "approval_id") {
        let approval_decision = trusted_mcp_actor_policy_decision(
            root,
            context,
            BusinessOsPermission::ExternalApprove,
            BusinessOsScopeType::Approval,
            Some(approval_id.as_str()),
        )?;
        if approval_decision.allowed {
            return Ok(approval_decision);
        }

        let module_decision = outbound_module_approval_decision(root, context)?;
        if module_decision.allowed {
            return Ok(module_decision);
        }
        return Ok(approval_decision);
    }

    outbound_module_approval_decision(root, context)
}

fn outbound_module_approval_decision(
    root: &Path,
    context: &McpChannelRequestContext,
) -> anyhow::Result<PolicyDecision> {
    trusted_mcp_actor_policy_decision(
        root,
        context,
        BusinessOsPermission::ExternalApprove,
        BusinessOsScopeType::Module,
        Some("outbound"),
    )
}

fn record_scope_id(collection: &str, record_id: &str) -> String {
    format!("{}/{}", collection.trim(), record_id.trim())
}

fn collection_requires_typed_mcp_tool(collection: &str) -> bool {
    matches!(
        collection.trim(),
        "business_users"
            | "business_permission_grants"
            | "business_sessions"
            | "business_peer_revocations"
            | "business_events"
            | "business_os_mcp_events"
            | "business_commands"
            | "ctox_queue_tasks"
            | "ctox_runs"
            | "business_module_acl"
            | "business_module_catalog"
            | "business_module_releases"
            | "business_module_versions"
            | "business_module_reports"
            | "business_module_source_files"
            | "business_consents"
            | "business_credentials"
            | "ctox_runtime_settings"
            | "ctox_task_approval_requests"
            | "desktop_files"
            | "desktop_file_chunks"
    )
}

fn module_ids_for_collection(root: &Path, collection: &str) -> anyhow::Result<Vec<String>> {
    let Ok(catalog) = store::module_catalog_for_rxdb(root) else {
        return Ok(Vec::new());
    };
    let modules = catalog
        .get("modules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut module_ids = Vec::new();
    for module in modules {
        let module_id = string_field(&module, "id").unwrap_or_default();
        if module_id.trim().is_empty() {
            continue;
        }
        let has_collection = module
            .get("collections")
            .and_then(Value::as_array)
            .map(|collections| {
                collections
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|candidate| candidate == collection)
            })
            .unwrap_or(false);
        if has_collection {
            module_ids.push(module_id);
        }
    }
    Ok(module_ids)
}

fn arguments_with_module_id(arguments: &Value, module_id: &str) -> Value {
    let mut value = arguments.clone();
    if let Some(object) = value.as_object_mut() {
        object
            .entry("module_id".to_string())
            .or_insert_with(|| Value::String(module_id.to_string()));
        return value;
    }
    serde_json::json!({ "module_id": module_id })
}

fn resolved_mcp_actor_context(
    root: &Path,
    context: &McpChannelRequestContext,
) -> anyhow::Result<Value> {
    if let Some(role) = context.trusted_role.as_deref() {
        return Ok(serde_json::json!({
            "id": &context.actor,
            "display_name": &context.actor,
            "role": role,
            "active": true,
            "persisted": false,
            "raw_actor": &context.actor,
            "role_source": context.trusted_role_source.as_deref().unwrap_or("trusted_gateway_context")
        }));
    }
    let actor = store::trusted_mcp_actor(root, &context.actor, &context.actor)?;
    Ok(serde_json::json!({
        "id": actor.id,
        "display_name": actor.display_name,
        "role": actor.role,
        "active": actor.active,
        "persisted": actor.persisted,
        "raw_actor": &context.actor
    }))
}

fn mcp_session(
    root: &Path,
    context: &McpChannelRequestContext,
) -> anyhow::Result<store::BusinessOsSession> {
    let actor = if let Some(role) = context.trusted_role.as_deref() {
        store::BusinessOsTrustedActor {
            id: context.actor.clone(),
            display_name: context.actor.clone(),
            role: normalize_role(role),
            active: true,
            persisted: false,
        }
    } else {
        store::trusted_mcp_actor(root, &context.actor, &context.actor)?
    };
    let role = normalize_role(&actor.role);
    Ok(store::BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(store::BusinessOsSessionUser {
            id: actor.id,
            display_name: actor.display_name,
            is_admin: super::policy::role_can_manage(&role),
            role,
        }),
        login_url: None,
        reason: None,
    })
}

fn enforce_context_policy(root: &Path, context: &McpChannelRequestContext) -> anyhow::Result<()> {
    let policy = mcp_policy(root);
    if !policy.allowed_actors.is_empty() && !policy.allowed_actors.contains(&context.actor) {
        return Err(policy_denied(
            "actor is not allowed for Business OS MCP channel",
            "CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS",
        ));
    }
    if !policy.allowed_workspaces.is_empty()
        && !policy.allowed_workspaces.contains(&context.workspace)
    {
        return Err(policy_denied(
            "workspace is not allowed for Business OS MCP channel",
            "CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES",
        ));
    }
    Ok(())
}

fn enforce_argument_scope_policy(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    arguments: &Value,
) -> anyhow::Result<()> {
    match tool_name {
        "business_os.get_module"
        | "business_os.list_entities"
        | "business_os.list_module_actions"
        | "business_os.propose_action"
        | "business_os.execute_action"
        | "business_os.modify_app"
        | "business_os.prepare_app_source"
        | "business_os.list_app_files"
        | "business_os.read_app_file"
        | "business_os.search_app_source"
        | "business_os.write_app_file"
        | "business_os.validate_app"
        | "business_os.smoke_app"
        | "business_os.e2e_app" => {
            if let Some(module_id) = string_field(arguments, "module_id") {
                enforce_module_policy(root, &module_id)?;
            }
        }
        "business_os.create_app" => {
            if let Ok(module_id) = app_module_id_from_arguments(
                arguments,
                string_field(arguments, "instruction")
                    .unwrap_or_default()
                    .as_str(),
            ) {
                enforce_module_policy(root, &module_id)?;
            }
        }
        "business_os.query_records"
        | "business_os.search_records"
        | "business_os.get_record"
        | "business_os.upsert_record"
        | "business_os.get_record_context"
        | "business_os.list_record_activity" => {
            if let Some(collection) = string_field(arguments, "collection") {
                enforce_collection_policy(root, &collection)?;
            }
        }
        "business_os.open_link" => {
            if let Some(value) = string_field(arguments, "module_or_collection") {
                match string_field(arguments, "kind").as_deref() {
                    Some("module") => enforce_module_policy(root, &value)?,
                    _ => enforce_collection_policy(root, &value)?,
                }
            }
        }
        "appsec_assessment_create"
        | "appsec_lab_create"
        | "appsec_lab_run"
        | "appsec_assessment_status"
        | "appsec_completion_review"
        | "appsec_tools_doctor"
        | "appsec_authz_plan"
        | "appsec_authz_credential_proof_template"
        | "appsec_authz_preflight"
        | "appsec_authz_run"
        | "appsec_authz_status"
        | "appsec_authz_build_matrix"
        | "appsec_pipeline_rework"
        | "appsec_report_get"
        | "appsec_finding_get" => {
            enforce_module_policy(root, APPSEC_MCP_MODULE_ID)?;
        }
        _ => {}
    }
    if tool_policy_class(tool_name) == McpToolPolicyClass::Read {
        enforce_business_os_mcp_policy(root, context, tool_name, arguments)?;
    }
    Ok(())
}

fn enforce_module_policy(root: &Path, module_id: &str) -> anyhow::Result<()> {
    let policy = mcp_policy(root);
    if !policy.allowed_modules.is_empty()
        && !policy
            .allowed_modules
            .iter()
            .any(|module| module == module_id)
    {
        return Err(policy_denied(
            "module is not allowed for Business OS MCP channel",
            "CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES",
        ));
    }
    Ok(())
}

fn enforce_collection_policy(root: &Path, collection: &str) -> anyhow::Result<()> {
    let policy = mcp_policy(root);
    if !policy.allowed_collections.is_empty()
        && !policy
            .allowed_collections
            .iter()
            .any(|allowed_collection| allowed_collection == collection)
    {
        return Err(policy_denied(
            "collection is not allowed for Business OS MCP channel",
            "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
        ));
    }
    Ok(())
}

fn enforce_rate_limit(root: &Path, context: &McpChannelRequestContext) -> anyhow::Result<()> {
    let limit = mcp_policy(root).rate_limit_per_minute;
    if limit == 0 {
        return Ok(());
    }
    let window_start_ms = now_ms().saturating_sub(60_000);
    let conn = store::open_store(root)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM business_os_mcp_events
         WHERE actor = ?1
           AND workspace = ?2
           AND created_at_ms >= ?3",
        params![
            context.actor.as_str(),
            context.workspace.as_str(),
            window_start_ms,
        ],
        |row| row.get(0),
    )?;
    if count >= limit as i64 {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::RateLimited,
            message: format!(
                "Business OS MCP actor `{}` exceeded {} calls per minute for workspace `{}`",
                context.actor, limit, context.workspace
            ),
            field: Some("CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE".to_string()),
        }));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpToolPolicyClass {
    Read,
    Write,
    Approval,
    ExternalEffect,
}

fn tool_policy_class(tool_name: &str) -> McpToolPolicyClass {
    match tool_name {
        "business_os.approve" => McpToolPolicyClass::ExternalEffect,
        "business_os.reject" | "business_os.request_changes" => McpToolPolicyClass::Approval,
        "business_os.execute_action"
        | "appsec_assessment_create"
        | "appsec_lab_create"
        | "appsec_lab_run"
        | "appsec_authz_plan"
        | "appsec_authz_credential_proof_template"
        | "appsec_authz_preflight"
        | "appsec_authz_run"
        | "appsec_authz_build_matrix"
        | "appsec_pipeline_rework"
        | "business_os.create_app"
        | "business_os.modify_app"
        | "business_os.prepare_app_source"
        | "business_os.upsert_record"
        | "business_os.upsert_user"
        | "business_os.write_app_file"
        | "business_os.validate_app"
        | "business_os.smoke_app"
        | "business_os.e2e_app" => McpToolPolicyClass::Write,
        _ => McpToolPolicyClass::Read,
    }
}

fn record_tool_event(
    root: &Path,
    context: &McpChannelRequestContext,
    status: &str,
    error_message: Option<String>,
    metadata: Value,
) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    prune_mcp_activity_with_conn(root, &conn)?;
    let event_id = format!("mcp_evt_{}", uuid::Uuid::new_v4());
    let error_code = error_message
        .as_ref()
        .map(|_| "tool_call_failed".to_string());
    conn.execute(
        "INSERT INTO business_os_mcp_events
            (event_id, channel, surface, actor, workspace, tool, request_id,
             confirmation_state, status, error_code, error_message, metadata_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            event_id,
            context.channel.as_str(),
            context.surface.as_str(),
            context.actor.as_str(),
            context.workspace.as_str(),
            context.tool.as_str(),
            context.request_id.as_str(),
            confirmation_state_as_str(&context.confirmation_state),
            status,
            error_code.as_deref(),
            error_message.as_deref(),
            serde_json::to_string(&metadata)?,
            now_ms(),
        ],
    )?;
    Ok(())
}

pub fn prune_mcp_activity(root: &Path) -> anyhow::Result<usize> {
    let conn = store::open_store(root)?;
    prune_mcp_activity_with_conn(root, &conn)
}

fn prune_mcp_activity_with_conn(root: &Path, conn: &rusqlite::Connection) -> anyhow::Result<usize> {
    let retention_days = mcp_policy(root).audit_retention_days;
    if retention_days == 0 {
        return Ok(0);
    }
    let retention_ms = (retention_days as i64)
        .saturating_mul(24)
        .saturating_mul(60)
        .saturating_mul(60)
        .saturating_mul(1000);
    let cutoff_ms = now_ms().saturating_sub(retention_ms);
    let deleted = conn.execute(
        "DELETE FROM business_os_mcp_events WHERE created_at_ms < ?1",
        params![cutoff_ms],
    )?;
    Ok(deleted)
}

fn argument_metadata(arguments: &Value) -> Value {
    let arg_keys = arguments
        .as_object()
        .map(|object| {
            object
                .keys()
                .filter(|key| key.as_str() != "_context")
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::json!({
        "argument_keys": arg_keys,
        "has_context": arguments.get("_context").is_some()
    })
}

fn argument_metadata_with_policy(
    root: &Path,
    context: &McpChannelRequestContext,
    tool_name: &str,
    arguments: &Value,
) -> Value {
    let mut metadata = argument_metadata(arguments);
    if let Some(object) = metadata.as_object_mut() {
        object.insert(
            "business_scope".to_string(),
            argument_business_scope_metadata(tool_name, arguments),
        );
        if let Ok(actor) = resolved_mcp_actor_context(root, context) {
            object.insert("resolved_actor".to_string(), actor);
        }
        if let Ok(Some(decision)) =
            business_os_mcp_policy_decision(root, context, tool_name, arguments)
        {
            object.insert(
                "policy_decision".to_string(),
                policy_decision_json(&decision),
            );
        }
    }
    metadata
}

fn argument_business_scope_metadata(tool_name: &str, arguments: &Value) -> Value {
    let mut scope = serde_json::Map::new();
    scope.insert("tool".to_string(), Value::String(tool_name.to_string()));
    if let Some(object) = arguments.as_object() {
        for key in [
            "module_id",
            "action_id",
            "collection",
            "record_id",
            "module_or_collection",
            "kind",
            "id",
            "command_id",
            "run_id",
            "artifact_id",
            "approval_id",
            "limit",
            "path",
            "query",
            "timeout_ms",
        ] {
            if let Some(value) = object.get(key).and_then(mcp_audit_safe_scalar) {
                scope.insert(key.to_string(), value);
            }
        }
    }
    Value::Object(scope)
}

fn mcp_audit_safe_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(mcp_audit_truncate(text, 160))),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn mcp_audit_truncate(text: &str, max_chars: usize) -> String {
    let mut truncated = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            truncated.push_str("...");
            return truncated;
        }
        truncated.push(ch);
    }
    truncated
}

fn policy_decision_json(decision: &PolicyDecision) -> Value {
    serde_json::json!({
        "allowed": decision.allowed,
        "permission": decision.permission,
        "scope_type": decision.scope_type,
        "scope_id": decision.scope_id.clone(),
        "reason_code": decision.reason_code,
        "display_reason": decision.display_reason,
        "requires_approval": decision.requires_approval,
        "audit_level": decision.audit_level,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn context_from_arguments_with_trusted_gateway_context(
    tool_name: &str,
    arguments: &Value,
    trusted_gateway_context: Option<&Value>,
) -> anyhow::Result<McpChannelRequestContext> {
    let context = arguments.get("_context").unwrap_or(&Value::Null);
    let trusted_role = trusted_managed_gateway_role(trusted_gateway_context);
    let request_context = McpChannelRequestContext {
        channel: string_field(context, "channel").unwrap_or_else(|| "chatgpt_mcp".to_string()),
        surface: string_field(context, "surface").unwrap_or_else(|| "business_os_mcp".to_string()),
        actor: string_field(context, "actor").unwrap_or_else(|| "mcp:local".to_string()),
        workspace: string_field(context, "workspace").unwrap_or_else(|| "local".to_string()),
        tool: tool_name.to_string(),
        request_id: string_field(context, "request_id")
            .unwrap_or_else(|| format!("local-{}", uuid::Uuid::new_v4())),
        confirmation_state: match string_field(context, "confirmation_state")
            .unwrap_or_else(|| "not_required".to_string())
            .as_str()
        {
            "required" => McpConfirmationState::Required,
            "approved" => McpConfirmationState::Approved,
            "rejected" => McpConfirmationState::Rejected,
            _ => McpConfirmationState::NotRequired,
        },
        trusted_role,
        trusted_role_source: trusted_gateway_context
            .and_then(|context| string_field(context, "auth_source")),
    };
    request_context.validate()?;
    Ok(request_context)
}

fn trusted_managed_gateway_role(context: Option<&Value>) -> Option<String> {
    let context = context?;
    let auth_source = string_field(context, "auth_source")?;
    if auth_source != "ctox_dev_managed_mcp_token" {
        return None;
    }
    if string_field(context, "channel").as_deref() != Some("ctox_dev_managed_mcp") {
        return None;
    }
    let role = normalize_role(&string_field(context, "role")?);
    match role.as_str() {
        "chef" | "admin" => Some(role),
        _ => None,
    }
}

fn confirmation_state_as_str(state: &McpConfirmationState) -> &'static str {
    match state {
        McpConfirmationState::NotRequired => "not_required",
        McpConfirmationState::Required => "required",
        McpConfirmationState::Approved => "approved",
        McpConfirmationState::Rejected => "rejected",
    }
}

fn confirmation_state_from_str(value: &str) -> McpConfirmationState {
    match value {
        "required" => McpConfirmationState::Required,
        "approved" => McpConfirmationState::Approved,
        "rejected" => McpConfirmationState::Rejected,
        _ => McpConfirmationState::NotRequired,
    }
}

fn app_module_id_from_arguments(arguments: &Value, instruction: &str) -> anyhow::Result<String> {
    optional_string_arg(arguments, "module_id")
        .or_else(|| optional_string_arg(arguments, "app_id"))
        .or_else(|| optional_string_arg(arguments, "title"))
        .map(|value| sanitize_app_module_id(&value))
        .unwrap_or_else(|| sanitize_app_module_id(instruction))
}

fn sanitize_app_module_id(value: &str) -> anyhow::Result<String> {
    let slug = value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ValidationFailed,
            message: "module_id is required".to_string(),
            field: Some("module_id".to_string()),
        }));
    }
    Ok(slug.chars().take(72).collect())
}

fn title_from_module_id(module_id: &str) -> String {
    let title = module_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        "Business OS App".to_string()
    } else {
        title
    }
}

fn normalize_app_semver(value: &str) -> anyhow::Result<String> {
    let version = value.trim();
    let valid = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .is_ok_and(|parts| parts.len() == 3);
    if !valid {
        return Err(anyhow::Error::new(BusinessOsMcpError {
            code: BusinessOsMcpErrorCode::ValidationFailed,
            message:
                "Business OS app version must use semver without a v prefix, for example 0.1.0"
                    .to_string(),
            field: Some("version".to_string()),
        }));
    }
    Ok(version.to_string())
}

fn required_arg(arguments: &Value, field: &str) -> anyhow::Result<String> {
    string_field(arguments, field)
        .ok_or_else(|| anyhow::Error::new(BusinessOsMcpError::validation(field, "required")))
}

fn optional_string_arg(arguments: &Value, field: &str) -> Option<String> {
    string_field(arguments, field)
}

fn optional_string_array_arg(arguments: &Value, field: &str) -> Vec<String> {
    match arguments.get(field) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.is_empty() {
                Vec::new()
            } else {
                vec![value.to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn optional_usize_arg(arguments: &Value, field: &str) -> Option<usize> {
    arguments
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn env_bool(
    env_map: &std::collections::BTreeMap<String, String>,
    key: &str,
    default_value: bool,
) -> bool {
    env_map
        .get(key)
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" | "enabled" => true,
            "0" | "false" | "no" | "off" | "disabled" => false,
            _ => default_value,
        })
        .unwrap_or(default_value)
}

fn env_usize(
    env_map: &std::collections::BTreeMap<String, String>,
    key: &str,
    default_value: usize,
) -> usize {
    env_map
        .get(key)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default_value)
}

fn split_csv(value: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            if seen.insert(item.to_string()) {
                Some(item.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn related_records(
    root: &Path,
    context: &McpChannelRequestContext,
    collection: &str,
    record_id: &str,
    limit: usize,
) -> anyhow::Result<BusinessOsMcpList<BusinessOsRecordSummary>> {
    let mut items = query_records(root, context, collection, Some(MAX_LIMIT))?.items;
    items.retain(|record| record_references_id(&record.data, record_id));
    sort_records_desc(&mut items);
    items.truncate(limit);
    Ok(BusinessOsMcpList {
        ok: true,
        count: items.len(),
        limit,
        items,
    })
}

fn resolve_approval_message_id(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    approval_id: Option<&str>,
) -> anyhow::Result<String> {
    if let Some(message_id) = optional_string_arg(arguments, "message_id") {
        return Ok(message_id);
    }
    let Some(approval_id) = approval_id else {
        return Err(anyhow::Error::new(BusinessOsMcpError::validation(
            "message_id",
            "message_id or approval_id is required",
        )));
    };
    let approval = get_record(root, context, "outbound_approvals", approval_id)?.record;
    string_field(&approval.data, "message_id").ok_or_else(|| {
        anyhow::Error::new(BusinessOsMcpError::validation(
            "approval_id",
            "approval has no message_id",
        ))
    })
}

fn record_references_id(value: &Value, record_id: &str) -> bool {
    if record_id.trim().is_empty() {
        return false;
    }
    for field in [
        "id",
        "record_id",
        "command_id",
        "task_id",
        "run_id",
        "work_id",
        "message_id",
        "engagement_id",
        "campaign_id",
        "document_id",
        "source_id",
        "source_record_id",
        "parent_id",
    ] {
        if string_field(value, field).as_deref() == Some(record_id) {
            return true;
        }
    }
    value_contains_string(value, record_id)
}

fn value_contains_string(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text == needle || text.contains(needle),
        Value::Array(items) => items.iter().any(|item| value_contains_string(item, needle)),
        Value::Object(object) => object
            .values()
            .any(|item| value_contains_string(item, needle)),
        _ => false,
    }
}

fn sort_records_desc(records: &mut [BusinessOsRecordSummary]) {
    records.sort_by(|left, right| {
        right
            .updated_at_ms
            .unwrap_or(0)
            .cmp(&left.updated_at_ms.unwrap_or(0))
            .then_with(|| right.id.cmp(&left.id))
    });
}

fn is_artifact_collection(collection: &str) -> bool {
    matches!(
        collection,
        "desktop_files" | "documents" | "match_artifacts"
    ) || collection.contains("artifact")
}

fn record_contains_artifact(value: &Value) -> bool {
    for field in [
        "artifact",
        "artifacts",
        "artifact_path",
        "artifact_url",
        "browser_context_artifact",
        "browser_extract_artifact",
        "document_id",
        "document_version_id",
        "document_pdf_url",
        "required_artifacts",
        "output_path",
    ] {
        if value.get(field).is_some() {
            return true;
        }
    }
    false
}

fn read_tool(name: &str, description: &str, input_schema: Value) -> BusinessOsMcpToolDescriptor {
    BusinessOsMcpToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        annotations: Some(serde_json::json!({
            "readOnlyHint": true,
            "destructiveHint": false,
            "openWorldHint": false
        })),
    }
}

fn write_tool(name: &str, description: &str, input_schema: Value) -> BusinessOsMcpToolDescriptor {
    BusinessOsMcpToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        annotations: Some(serde_json::json!({
            "readOnlyHint": false,
            "destructiveHint": false,
            "openWorldHint": false
        })),
    }
}

fn object_schema(properties: Vec<(&'static str, Value, bool)>) -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, schema, is_required) in properties {
        props.insert(name.to_string(), schema);
        if is_required {
            required.push(Value::String(name.to_string()));
        }
    }
    serde_json::json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false
    })
}

fn required_string(name: &'static str) -> (&'static str, Value, bool) {
    (name, serde_json::json!({ "type": "string" }), true)
}

fn optional_string(name: &'static str) -> (&'static str, Value, bool) {
    (name, serde_json::json!({ "type": "string" }), false)
}

fn optional_string_array(name: &'static str) -> (&'static str, Value, bool) {
    (
        name,
        serde_json::json!({
            "type": "array",
            "items": { "type": "string" }
        }),
        false,
    )
}

fn optional_integer(
    name: &'static str,
    minimum: usize,
    maximum: usize,
) -> (&'static str, Value, bool) {
    (
        name,
        serde_json::json!({
            "type": "integer",
            "minimum": minimum,
            "maximum": maximum
        }),
        false,
    )
}

fn optional_boolean(name: &'static str) -> (&'static str, Value, bool) {
    (name, serde_json::json!({ "type": "boolean" }), false)
}

fn optional_object(name: &'static str) -> (&'static str, Value, bool) {
    (
        name,
        serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }),
        false,
    )
}

fn required_object(name: &'static str) -> (&'static str, Value, bool) {
    (
        name,
        serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }),
        true,
    )
}

fn generic_delegate_action(module_id: &str) -> BusinessOsActionDescriptor {
    action_descriptor(
        "ctox.delegate_task",
        module_id,
        "Delegate CTOX task",
        "Prepare a durable CTOX task scoped to this Business OS module.",
        "write",
        false,
        false,
    )
}

fn action_descriptor(
    action_id: &str,
    module_id: &str,
    title: &str,
    description: &str,
    risk_class: &str,
    confirmation_required: bool,
    external_effect: bool,
) -> BusinessOsActionDescriptor {
    BusinessOsActionDescriptor {
        action_id: action_id.to_string(),
        module_id: module_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        risk_class: risk_class.to_string(),
        confirmation_required,
        external_effect,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "objective": { "type": "string" },
                "record_id": { "type": "string" },
                "payload": { "type": "object", "additionalProperties": true }
            },
            "additionalProperties": true
        }),
    }
}

fn module_descriptor_from_value(value: Value) -> anyhow::Result<BusinessOsModuleDescriptor> {
    let id = string_field(&value, "id").context("module id is required")?;
    Ok(BusinessOsModuleDescriptor {
        title: string_field(&value, "title").unwrap_or_else(|| id.clone()),
        description: string_field(&value, "description").unwrap_or_default(),
        category: string_field(&value, "category").unwrap_or_default(),
        source: string_field(&value, "source").unwrap_or_default(),
        install_scope: string_field(&value, "install_scope").unwrap_or_default(),
        core: value.get("core").and_then(Value::as_bool).unwrap_or(false),
        entry: string_field(&value, "entry").unwrap_or_default(),
        collections: value
            .get("collections")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        deep_link: open_link("module", &id, None),
        id,
    })
}

fn record_summary_from_value(collection: &str, value: Value) -> BusinessOsRecordSummary {
    let id = string_field(&value, "id")
        .or_else(|| string_field(&value, "record_id"))
        .unwrap_or_else(|| "unknown".to_string());
    let title = first_string_field(&value, &["title", "name", "display_name", "summary"])
        .unwrap_or_else(|| id.clone());
    let status = first_string_field(&value, &["status", "state", "status_key"]);
    let updated_at_ms = value.get("updated_at_ms").and_then(Value::as_i64);
    let summary = first_string_field(&value, &["summary", "description", "index_text"]);
    BusinessOsRecordSummary {
        deep_link: open_link("record", collection, Some(&id)),
        id,
        collection: collection.to_string(),
        title,
        status,
        updated_at_ms,
        summary,
        data: value,
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn first_string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| string_field(value, field))
}

fn titleize_collection(collection: &str) -> String {
    collection
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn encode_link_part(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => ch.to_string(),
            _ => format!("%{:02X}", ch as u32),
        })
        .collect::<String>()
}

const REDACTED_MCP_VALUE: &str = "[REDACTED]";

fn redact_mcp_response(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_mcp_key(&key) {
                        (key, Value::String(REDACTED_MCP_VALUE.to_string()))
                    } else {
                        (key, redact_mcp_response(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_mcp_response).collect()),
        other => other,
    }
}

fn is_sensitive_mcp_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "password"
            | "secret"
            | "token"
            | "api_key"
            | "apikey"
            | "credential"
            | "credentials"
            | "private_key"
            | "access_key"
            | "refresh_token"
            | "authorization"
            | "cookie"
            | "set_cookie"
    ) || key.contains("_secret")
        || key.contains("secret_")
        || key.contains("_token")
        || key.contains("token_")
        || key.contains("api_key")
        || key.contains("private_key")
        || key.contains("access_key")
        || key.contains("refresh_token")
        || key.contains("password")
        || key.contains("credential")
        || key.contains("authorization")
        || key.contains("cookie")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_context(tool: &str) -> McpChannelRequestContext {
        McpChannelRequestContext {
            channel: "chatgpt_mcp".to_string(),
            surface: "business_os_mcp".to_string(),
            actor: "chatgpt:test-user".to_string(),
            workspace: "test-workspace".to_string(),
            tool: tool.to_string(),
            request_id: format!("req-{tool}"),
            confirmation_state: McpConfirmationState::NotRequired,
            trusted_role: None,
            trusted_role_source: None,
        }
    }

    fn write_module(
        root: &Path,
        id: &str,
        title: &str,
        collections: &[&str],
    ) -> anyhow::Result<()> {
        let module_root = root.join("src/apps/business-os/modules").join(id);
        std::fs::create_dir_all(&module_root)?;
        std::fs::write(root.join("src/apps/business-os/index.html"), "")?;
        std::fs::write(
            module_root.join("module.json"),
            serde_json::to_string(&serde_json::json!({
                "id": id,
                "title": title,
                "description": "Test module",
                "install_scope": "core",
                "entry": format!("modules/{id}/index.html"),
                "collections": collections
            }))?,
        )?;
        Ok(())
    }

    fn write_installed_module(
        root: &Path,
        id: &str,
        title: &str,
        version: &str,
        collections: &[&str],
        lifecycle: Option<Value>,
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(root.join("src/apps/business-os"))?;
        std::fs::write(root.join("src/apps/business-os/index.html"), "")?;
        let module_root = root.join("runtime/business-os/installed-modules").join(id);
        std::fs::create_dir_all(&module_root)?;
        let mut manifest = serde_json::json!({
            "id": id,
            "title": title,
            "description": "Runtime installed test module",
            "version": version,
            "install_scope": "installed",
            "entry": format!("installed-modules/{id}/index.html"),
            "collections": collections
        });
        if let Some(lifecycle) = lifecycle {
            manifest["lifecycle"] = lifecycle;
        }
        std::fs::write(
            module_root.join("module.json"),
            serde_json::to_string(&manifest)?,
        )?;
        Ok(())
    }

    fn save_mcp_policy_env(root: &Path, entries: &[(&str, &str)]) -> anyhow::Result<()> {
        let mut env_map = std::collections::BTreeMap::new();
        for (key, value) in entries {
            env_map.insert((*key).to_string(), (*value).to_string());
        }
        crate::inference::runtime_env::save_runtime_env_map(root, &env_map)?;
        Ok(())
    }

    #[test]
    fn mcp_policy_uses_legacy_env_until_typed_policy_exists() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        save_mcp_policy_env(
            root,
            &[
                ("CTOX_BUSINESS_OS_MCP_ENABLED", "false"),
                ("CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS", "30"),
                ("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES", "legacy,legacy"),
            ],
        )?;

        let legacy = mcp_policy(root);
        assert!(!legacy.enabled);
        assert_eq!(legacy.audit_retention_days, 30);
        assert_eq!(legacy.allowed_modules, vec!["legacy".to_string()]);

        let mut typed = default_mcp_policy();
        typed.enabled = true;
        typed.audit_retention_days = 1;
        typed.allowed_modules = vec![
            "customers".to_string(),
            " customers ".to_string(),
            "outbound".to_string(),
        ];
        typed.denied_tools = vec![
            "business_os.execute_action".to_string(),
            "business_os.execute_action".to_string(),
        ];
        save_mcp_policy(root, &typed)?;

        let effective = mcp_policy(root);
        assert!(effective.enabled);
        assert_eq!(effective.audit_retention_days, 1);
        assert_eq!(
            effective.allowed_modules,
            vec!["customers".to_string(), "outbound".to_string()]
        );
        assert_eq!(
            effective.denied_tools,
            vec!["business_os.execute_action".to_string()]
        );
        Ok(())
    }

    fn seed_business_user(root: &Path, id: &str, role: &str) -> anyhow::Result<()> {
        let conn = store::open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES (?1, ?1, ?2, 1, ?3, ?3)
             ON CONFLICT(user_id) DO UPDATE SET
                role = excluded.role,
                active = 1,
                updated_at_ms = excluded.updated_at_ms",
            params![id, crate::business_os::policy::normalize_role(role), now],
        )?;
        Ok(())
    }

    fn seed_default_mcp_admin(root: &Path) -> anyhow::Result<()> {
        seed_business_user(root, "chatgpt:test-user", "admin")
    }

    fn seed_business_permission_grant(
        root: &Path,
        grant_id: &str,
        subject_type: &str,
        subject_id: &str,
        permission: BusinessOsPermission,
        scope_type: &str,
        scope_id: &str,
    ) -> anyhow::Result<()> {
        let conn = store::open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_permission_grants
                (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
                 active, reason, created_by, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 'test grant', 'mcp-test', ?7, ?7)
             ON CONFLICT(grant_id) DO UPDATE SET
                active = 1,
                updated_at_ms = excluded.updated_at_ms",
            params![
                grant_id,
                subject_type,
                subject_id,
                permission.as_str(),
                scope_type,
                scope_id,
                now
            ],
        )?;
        Ok(())
    }

    #[test]
    fn context_validation_rejects_missing_actor() {
        let mut context = test_context("business_os.status");
        context.actor.clear();
        let error = context.validate().expect_err("missing actor must fail");
        assert_eq!(error.code, BusinessOsMcpErrorCode::ValidationFailed);
        assert_eq!(error.field.as_deref(), Some("actor"));
    }

    #[test]
    fn list_modules_reads_business_os_catalog() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "tickets", "Tickets", &["ctox_ticket_items"])?;
        seed_default_mcp_admin(root)?;

        let modules = list_modules(root, &test_context("business_os.list_modules"))?;

        assert_eq!(modules.count, 1);
        assert_eq!(modules.items[0].id, "tickets");
        assert_eq!(modules.items[0].collections, vec!["ctox_ticket_items"]);
        assert_eq!(modules.items[0].deep_link.url_fragment, "#module=tickets");
        Ok(())
    }

    #[test]
    fn list_entities_marks_writable_entities_from_module_collections() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(
            root,
            "customers",
            "Customers",
            &["customer_accounts", "business_users"],
        )?;
        seed_default_mcp_admin(root)?;

        let entities = list_entities(
            root,
            &test_context("business_os.list_entities"),
            "customers",
        )?;

        assert_eq!(entities.count, 2);
        assert_eq!(entities.items[0].entity_id, "customer_accounts");
        assert!(!entities.items[0].read_only);
        assert_eq!(entities.items[1].entity_id, "business_users");
        assert!(entities.items[1].read_only);
        Ok(())
    }

    #[test]
    fn list_entities_keeps_non_admin_entities_read_only() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:test-user", "user")?;

        let entities = list_entities(
            root,
            &test_context("business_os.list_entities"),
            "customers",
        )?;

        assert_eq!(entities.count, 1);
        assert_eq!(entities.items[0].entity_id, "customer_accounts");
        assert!(entities.items[0].read_only);
        Ok(())
    }

    #[test]
    fn query_records_returns_bounded_record_summaries() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Metric Space",
                    "status": "active",
                    "summary": "Key account",
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let records = query_records(
            root,
            &test_context("business_os.query_records"),
            "customer_accounts",
            Some(10),
        )?;

        assert_eq!(records.count, 1);
        assert_eq!(records.items[0].id, "acct_1");
        assert_eq!(records.items[0].title, "Metric Space");
        assert_eq!(records.items[0].status.as_deref(), Some("active"));
        assert_eq!(
            records.items[0].deep_link.url_fragment,
            "#module=customer_accounts&record=acct_1"
        );
        Ok(())
    }

    #[test]
    fn upsert_record_persists_app_data_for_admin_mcp_actor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_default_mcp_admin(root)?;

        let result = call_tool(
            root,
            "business_os.upsert_record",
            serde_json::json!({
                "collection": "customer_accounts",
                "record": {
                    "id": "acct_mcp_1",
                    "name": "Metric Space",
                    "status": "active"
                },
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("record_id").and_then(Value::as_str),
            Some("acct_mcp_1")
        );
        assert_eq!(
            result.pointer("/record/data/name").and_then(Value::as_str),
            Some("Metric Space")
        );

        let stored = get_record(
            root,
            &test_context("business_os.get_record"),
            "customer_accounts",
            "acct_mcp_1",
        )?;
        assert_eq!(
            stored.record.data.get("status").and_then(Value::as_str),
            Some("active")
        );
        Ok(())
    }

    #[test]
    fn upsert_record_rejects_control_collections() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;

        let error = call_tool(
            root,
            "business_os.upsert_record",
            serde_json::json!({
                "collection": "business_users",
                "record": {
                    "id": "claude:user_1",
                    "display_name": "Claude User",
                    "role": "user"
                },
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("generic writes must reject control collections");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ValidationFailed);
        assert_eq!(typed.field.as_deref(), Some("collection"));
        Ok(())
    }

    #[test]
    fn upsert_user_creates_team_member_for_admin_mcp_actor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;

        let result = call_tool(
            root,
            "business_os.upsert_user",
            serde_json::json!({
                "id": "claude:user_1",
                "display_name": "Claude User",
                "role": "user",
                "active": true,
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        let users = result
            .get("users")
            .and_then(Value::as_array)
            .context("expected users array")?;
        let user = users
            .iter()
            .find(|user| user.get("id").and_then(Value::as_str) == Some("claude:user_1"))
            .context("expected created user")?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            user.get("display_name").and_then(Value::as_str),
            Some("Claude User")
        );
        assert_eq!(user.get("role").and_then(Value::as_str), Some("user"));
        assert_eq!(user.get("active").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn upsert_user_rejects_non_admin_mcp_actor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test-user", "user")?;

        let error = call_tool(
            root,
            "business_os.upsert_user",
            serde_json::json!({
                "id": "claude:user_2",
                "display_name": "Claude User 2",
                "role": "user",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("non-admin MCP actors must not manage users");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn tool_descriptors_expose_only_typed_business_os_tools() {
        let tools = tool_descriptors();
        assert!(tools.iter().any(|tool| tool.name == "business_os.status"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.query_records"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.execute_action"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.upsert_record"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.upsert_user"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.create_app"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.modify_app"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.prepare_app_source"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.list_app_files"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.read_app_file"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.search_app_source"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.write_app_file"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.validate_app"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.smoke_app"));
        assert!(tools.iter().any(|tool| tool.name == "business_os.e2e_app"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.get_command_status"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.get_record_context"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.list_runs"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.list_approvals"));
        assert!(tools.iter().any(|tool| tool.name == "business_os.approve"));
        assert!(tools.iter().any(|tool| tool.name == "business_os.reject"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "business_os.request_changes"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_assessment_create"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_lab_create"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_lab_run"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_assessment_status"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_completion_review"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_tools_doctor"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_authz_plan"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_authz_credential_proof_template"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_authz_preflight"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_authz_run"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_authz_status"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_authz_build_matrix"));
        assert!(tools
            .iter()
            .any(|tool| tool.name == "appsec_pipeline_rework"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_report_get"));
        assert!(tools.iter().any(|tool| tool.name == "appsec_finding_get"));
        for forbidden in [
            "run_cli",
            "run_shell",
            "write_sql",
            "push_rxdb_record",
            "remote_control_browser",
            "execute_raw_business_command",
        ] {
            assert!(
                tools.iter().all(|tool| tool.name != forbidden),
                "{forbidden} must not be exposed as a Business OS MCP tool"
            );
        }
    }

    #[test]
    fn appsec_mcp_tools_create_status_plan_report_and_finding() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;

        let create = call_tool(
            root,
            "appsec_assessment_create",
            serde_json::json!({
                "url": "https://example.test",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(create.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            create.get("module_id").and_then(Value::as_str),
            Some(APPSEC_MCP_MODULE_ID)
        );

        let lab = call_tool(
            root,
            "appsec_lab_create",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(lab.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            lab.get("command").and_then(Value::as_str),
            Some("appsec_lab_create")
        );
        assert_eq!(
            lab.get("module_id").and_then(Value::as_str),
            Some(APPSEC_MCP_MODULE_ID)
        );
        assert!(root
            .join("runtime/appsec/default/lab/vulnerable-webapp/ctox-vulnerable-webapp.py")
            .is_file());

        let status = call_tool(
            root,
            "appsec_assessment_status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(status.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(status.get("synced").and_then(Value::as_bool), Some(true));

        let review = call_tool(
            root,
            "appsec_completion_review",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            review.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_completion_review")
        );
        assert_eq!(
            review
                .pointer("/completion_review/closable")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(root
            .join("runtime/appsec/default/completion-review.json")
            .is_file());

        let doctor = call_tool(
            root,
            "appsec_tools_doctor",
            serde_json::json!({
                "profile": "minimal",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            doctor.get("command").and_then(Value::as_str),
            Some("tools doctor")
        );
        assert_eq!(
            doctor.get("module_id").and_then(Value::as_str),
            Some(APPSEC_MCP_MODULE_ID)
        );

        let authz = call_tool(
            root,
            "appsec_authz_plan",
            serde_json::json!({
                "target": "https://example.test",
                "source_id": "custom-web-app",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(authz.get("ok").and_then(Value::as_bool), Some(true));
        let authz_artifact = authz.get("artifact").and_then(Value::as_str).unwrap();
        assert!(Path::new(authz_artifact).is_file());

        let subjects_path = root.join("authz-subjects.json");
        fs::write(
            &subjects_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "subjects": [
                    {"id": "user-a", "role": "owner", "login_hint": "a@example.test", "credential_ref": "ctox-secret://appsec/a"},
                    {"id": "user-b", "role": "member", "login_hint": "b@example.test", "credential_ref": "ctox-secret://appsec/b"}
                ]
            }))?,
        )?;
        let authz_preflight = call_tool(
            root,
            "appsec_authz_credential_proof_template",
            serde_json::json!({
                "subjects": "authz-subjects.json",
                "force": true,
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            authz_preflight.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_authz_credential_proof_template")
        );
        assert_eq!(
            authz_preflight.get("ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            authz_preflight
                .pointer("/credential_proof/version")
                .and_then(Value::as_str),
            Some("ctox.appsec_pentest.authz_credential_proof.v1")
        );
        let credential_proof_artifact = authz_preflight
            .get("artifact")
            .and_then(Value::as_str)
            .expect("credential proof artifact")
            .to_string();
        assert!(Path::new(&credential_proof_artifact).is_file());

        let authz_preflight = call_tool(
            root,
            "appsec_authz_preflight",
            serde_json::json!({
                "target": "https://example.test",
                "subjects": "authz-subjects.json",
                "source_id": "custom-web-app",
                "credential_proof": credential_proof_artifact.clone(),
                "require_credentials": true,
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            authz_preflight.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_authz_preflight")
        );
        assert_eq!(
            authz_preflight.get("ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            authz_preflight.get("status").and_then(Value::as_str),
            Some("ready-for-web-stack-execution")
        );
        assert_eq!(
            authz_preflight
                .pointer("/preflight/subject_summary/cross_subject_pairs")
                .and_then(Value::as_u64),
            Some(2)
        );
        let authz_run = call_tool(
            root,
            "appsec_authz_run",
            serde_json::json!({
                "target": "https://example.test",
                "subjects": "authz-subjects.json",
                "source_id": "custom-web-app",
                "credential_proof": credential_proof_artifact,
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(authz_run.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            authz_run.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_authz_run")
        );
        let authz_run_artifact = authz_run.get("artifact").and_then(Value::as_str).unwrap();
        assert!(Path::new(authz_run_artifact).is_file());
        assert!(authz_run
            .pointer("/run/web_stack_tasks")
            .and_then(Value::as_array)
            .is_some_and(|tasks| !tasks.is_empty()));
        let authz_status = call_tool(
            root,
            "appsec_authz_status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            authz_status.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_authz_status")
        );
        assert_eq!(authz_status.get("ok").and_then(Value::as_bool), Some(true));
        assert!(authz_status
            .get("runs")
            .and_then(Value::as_array)
            .is_some_and(|runs| !runs.is_empty()));
        assert!(authz_status
            .get("preflights")
            .and_then(Value::as_array)
            .is_some_and(|preflights| !preflights.is_empty()));

        let authz_evidence_dir = root.join("runtime/appsec/default/authz/mcp-evidence");
        fs::create_dir_all(&authz_evidence_dir)?;
        fs::write(
            authz_evidence_dir.join("cross-subject-redacted.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                "redacted": true,
                "objects": [
                    {"id": "tenant-a", "object_type": "tenant", "owner_subject": "user-a"}
                ],
                "cases": [
                    {
                        "actor_subject": "user-b",
                        "owner_subject": "user-a",
                        "object_ref": "tenant-a",
                        "endpoint": "/api/tenants/tenant-a",
                        "method": "GET",
                        "expected": "deny",
                        "actual_status": 404,
                        "result": "pass",
                        "body_class": "not-found",
                        "leak": false,
                        "mutation": false
                    }
                ]
            }))?,
        )?;
        let authz_matrix = call_tool(
            root,
            "appsec_authz_build_matrix",
            serde_json::json!({
                "run": authz_run_artifact,
                "evidence_dir": "runtime/appsec/default/authz/mcp-evidence",
                "import": true,
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(authz_matrix.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            authz_matrix.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_authz_build_matrix")
        );
        assert_eq!(
            authz_matrix
                .pointer("/summary/cases")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            authz_matrix
                .pointer("/summary/cross_subject_cases")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            authz_matrix
                .pointer("/import_result/created_candidate_finding")
                .and_then(Value::as_bool),
            Some(false)
        );

        let state_dir = root.join("runtime/appsec/default");
        crate::run_projected_appsec_command(
            root,
            &[
                "--state-dir".to_string(),
                state_dir.to_string_lossy().to_string(),
                "assess".to_string(),
                "--profile".to_string(),
                "standard".to_string(),
                "--url".to_string(),
                "https://example.test".to_string(),
                "--json".to_string(),
            ],
        )?;
        let rework_evidence = state_dir.join("mcp-rework-evidence.json");
        fs::write(
            &rework_evidence,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": "test.redacted-rework-evidence.v1",
                "redacted": true,
                "observation": "operator-reviewed MCP rework evidence"
            }))?,
        )?;
        let rework = call_tool(
            root,
            "appsec_pipeline_rework",
            serde_json::json!({
                "phase": "blackbox-map",
                "target": "https://example.test",
                "status": "not-applicable",
                "reason": "MCP operator supplied redacted manual evidence for this stage.",
                "artifact": "runtime/appsec/default/mcp-rework-evidence.json",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(rework.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            rework.get("mcp_tool").and_then(Value::as_str),
            Some("appsec_pipeline_rework")
        );
        let rework_stages = rework
            .pointer("/pipeline_status/stages")
            .and_then(Value::as_array)
            .context("expected pipeline stages")?;
        assert!(rework_stages.iter().any(|stage| {
            stage.get("phase").and_then(Value::as_str) == Some("blackbox-map")
                && stage.get("status").and_then(Value::as_str) == Some("not-applicable")
        }));

        crate::run_projected_appsec_command(
            root,
            &[
                "--state-dir".to_string(),
                state_dir.to_string_lossy().to_string(),
                "finding".to_string(),
                "create".to_string(),
                "--title".to_string(),
                "Authorization check candidate".to_string(),
                "--target".to_string(),
                "https://example.test/account".to_string(),
                "--severity".to_string(),
                "medium".to_string(),
            ],
        )?;
        crate::run_projected_appsec_command(
            root,
            &[
                "--state-dir".to_string(),
                state_dir.to_string_lossy().to_string(),
                "report".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ],
        )?;

        let finding = call_tool(
            root,
            "appsec_finding_get",
            serde_json::json!({
                "finding_id": "F-001",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(finding.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            finding.pointer("/finding/id").and_then(Value::as_str),
            Some("F-001")
        );

        let report = call_tool(
            root,
            "appsec_report_get",
            serde_json::json!({
                "format": "json",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(report.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            report.get("status").and_then(Value::as_str),
            Some("available")
        );
        assert_eq!(
            report.pointer("/report/version").and_then(Value::as_str),
            Some("ctox.appsec_pentest.report.v1")
        );
        Ok(())
    }

    #[test]
    fn appsec_mcp_tools_respect_module_allowlist_and_path_boundary() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;

        let mut policy = default_mcp_policy();
        policy.allowed_modules = vec!["customers".to_string()];
        save_mcp_policy(root, &policy)?;
        let denied = call_tool(
            root,
            "appsec_assessment_status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("allowed_modules must gate appsec MCP tools");
        let typed = denied
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed MCP error");
        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(
            typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES")
        );

        policy.allowed_modules = vec![APPSEC_MCP_MODULE_ID.to_string()];
        save_mcp_policy(root, &policy)?;
        let traversal = call_tool(
            root,
            "appsec_assessment_status",
            serde_json::json!({
                "state_dir": "../outside",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("state_dir traversal must be rejected");
        let typed = traversal
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed MCP error");
        assert_eq!(typed.code, BusinessOsMcpErrorCode::ValidationFailed);
        assert_eq!(typed.field.as_deref(), Some("state_dir"));
        Ok(())
    }

    #[test]
    fn create_app_tool_enqueues_agent_led_app_command_without_writing_files() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;

        let result = call_tool(
            root,
            "business_os.create_app",
            serde_json::json!({
                "module_id": "mcp-inventory",
                "instruction": "Build a small inventory app with one CTOX follow-up automation.",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("command_type").and_then(Value::as_str),
            Some("ctox.business_os.app.create")
        );
        assert_eq!(
            result.get("app_directory").and_then(Value::as_str),
            Some("runtime/business-os/installed-modules/mcp-inventory")
        );
        assert_eq!(
            result
                .pointer("/development_contract/source_root")
                .and_then(Value::as_str),
            Some("runtime/business-os/installed-modules/mcp-inventory")
        );
        assert_eq!(
            result
                .pointer("/development_contract/validation_command")
                .and_then(Value::as_str),
            Some("ctox business-os app validate mcp-inventory --installed")
        );
        assert!(result
            .pointer("/development_contract/source_files")
            .and_then(Value::as_array)
            .context("expected development_contract.source_files")?
            .iter()
            .any(|path| path.as_str()
                == Some("runtime/business-os/installed-modules/mcp-inventory/module.json")));
        let skill_resources = result
            .pointer("/development_contract/skill_resources")
            .and_then(Value::as_array)
            .context("expected development_contract.skill_resources")?;
        assert!(skill_resources.iter().any(|path| {
            path.as_str()
                .is_some_and(|path| path.ends_with("references/design-guide.md"))
        }));
        assert!(skill_resources.iter().any(|path| {
            path.as_str()
                .is_some_and(|path| path.ends_with("references/standalone-porting.md"))
        }));
        assert!(
            !root
                .join("runtime/business-os/installed-modules/mcp-inventory")
                .exists(),
            "MCP create_app must not write app artifacts"
        );
        let task_id = result
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?;
        let task = crate::mission::channels::load_queue_task(root, task_id)?
            .context("expected queue task")?;
        assert_eq!(
            task.suggested_skill.as_deref(),
            Some("business-os-app-module-development")
        );
        assert!(task.prompt.contains("ctox.business_os.app.create"));
        assert!(task.prompt.contains(
            "ctox business-os app references --query \"<workflow data keywords>\" --json --limit 8"
        ));
        Ok(())
    }

    #[test]
    fn prepare_app_source_creates_runtime_workspace_for_direct_mcp_coding() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        std::fs::create_dir_all(root.join("src/apps/business-os"))?;
        std::fs::write(root.join("src/apps/business-os/index.html"), "")?;

        let result = call_tool(
            root,
            "business_os.prepare_app_source",
            serde_json::json!({
                "module_id": "mcp-direct-app",
                "title": "MCP Direct App",
                "description": "Direct MCP coded app",
                "instruction": "Prepare a workspace Claude can edit directly.",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("app_directory").and_then(Value::as_str),
            Some("runtime/business-os/installed-modules/mcp-direct-app")
        );
        assert!(root
            .join("runtime/business-os/installed-modules/mcp-direct-app/module.json")
            .is_file());
        assert!(result
            .pointer("/development_contract/source_tools")
            .and_then(Value::as_array)
            .context("expected source tools")?
            .iter()
            .any(|tool| tool.as_str() == Some("business_os.write_app_file")));
        Ok(())
    }

    #[test]
    fn app_source_tools_write_read_and_search_runtime_files() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        write_installed_module(root, "mcp-source", "MCP Source", "1.0.0", &[], None)?;

        let write = call_tool(
            root,
            "business_os.write_app_file",
            serde_json::json!({
                "module_id": "mcp-source",
                "path": "lib/math.mjs",
                "content": "export const answer = 42;\n",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(write.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            write.get("path").and_then(Value::as_str),
            Some("lib/math.mjs")
        );

        let read = call_tool(
            root,
            "business_os.read_app_file",
            serde_json::json!({
                "module_id": "mcp-source",
                "path": "lib/math.mjs",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(
            read.get("content").and_then(Value::as_str),
            Some("export const answer = 42;\n")
        );

        let search = call_tool(
            root,
            "business_os.search_app_source",
            serde_json::json!({
                "module_id": "mcp-source",
                "query": "answer",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;
        assert_eq!(search.get("count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            search.pointer("/items/0/path").and_then(Value::as_str),
            Some("lib/math.mjs")
        );
        Ok(())
    }

    #[test]
    fn app_source_write_rejects_path_traversal() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        write_installed_module(root, "mcp-source", "MCP Source", "1.0.0", &[], None)?;

        let error = call_tool(
            root,
            "business_os.write_app_file",
            serde_json::json!({
                "module_id": "mcp-source",
                "path": "../escape.js",
                "content": "export const bad = true;\n",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("path traversal must be rejected");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .context("expected typed Business OS MCP error")?;
        assert_eq!(typed.code, BusinessOsMcpErrorCode::ValidationFailed);
        assert_eq!(typed.field.as_deref(), Some("path"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn app_source_write_rejects_symlink_escape() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        write_installed_module(root, "mcp-source", "MCP Source", "1.0.0", &[], None)?;
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside)?;
        std::os::unix::fs::symlink(
            &outside,
            root.join("runtime/business-os/installed-modules/mcp-source/linked"),
        )?;

        let error = call_tool(
            root,
            "business_os.write_app_file",
            serde_json::json!({
                "module_id": "mcp-source",
                "path": "linked/escape.js",
                "content": "export const bad = true;\n",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("symlink escape must be rejected");

        assert!(
            error.to_string().contains("symlink"),
            "expected symlink rejection, got {error:#}"
        );
        assert!(!outside.join("escape.js").exists());
        Ok(())
    }

    #[test]
    fn validate_app_returns_structured_result_without_shell_tool() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        write_installed_module(root, "mcp-source", "MCP Source", "1.0.0", &[], None)?;
        let script_dir = root.join("src/apps/business-os/scripts");
        std::fs::create_dir_all(&script_dir)?;
        std::fs::write(
            script_dir.join("validate-app-module.mjs"),
            "console.log(JSON.stringify({ ok: true, module_id: process.argv[2] }));\n",
        )?;

        let result = call_tool(
            root,
            "business_os.validate_app",
            serde_json::json!({
                "module_id": "mcp-source",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert!(result
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("validate-app-module.mjs"));
        assert!(result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("\"module_id\":\"mcp-source\""));
        Ok(())
    }

    #[test]
    fn modify_app_tool_enqueues_app_modify_skill_task() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        write_module(root, "mcp-inventory", "MCP Inventory", &["inventory_items"])?;

        let result = call_tool(
            root,
            "business_os.modify_app",
            serde_json::json!({
                "module_id": "mcp-inventory",
                "instruction": "Add a reorder review action and keep existing data.",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("command_type").and_then(Value::as_str),
            Some("ctox.business_os.app.modify")
        );
        assert_eq!(
            result
                .pointer("/development_contract/source_root")
                .and_then(Value::as_str),
            Some("runtime/business-os/installed-modules/mcp-inventory")
        );
        assert_eq!(
            result
                .pointer("/development_contract/required_skill")
                .and_then(Value::as_str),
            Some("business-os-app-module-development")
        );
        let task_id = result
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?;
        let task = crate::mission::channels::load_queue_task(root, task_id)?
            .context("expected queue task")?;
        assert_eq!(
            task.suggested_skill.as_deref(),
            Some("business-os-app-module-development")
        );
        assert!(task.prompt.contains("ctox.business_os.app.modify"));
        assert!(task
            .prompt
            .contains("runtime/business-os/installed-modules/mcp-inventory"));
        Ok(())
    }

    #[test]
    fn modify_app_tool_rejects_unknown_module_without_recording_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_default_mcp_admin(root)?;
        std::fs::create_dir_all(root.join("src/apps/business-os"))?;
        std::fs::write(root.join("src/apps/business-os/index.html"), "")?;

        let error = call_tool(
            root,
            "business_os.modify_app",
            serde_json::json!({
                "module_id": "missing-inventory",
                "instruction": "Add a reorder review action and keep existing data.",
                "_context": {
                    "actor": "chatgpt:test-user",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("modify_app must reject unknown modules before queueing work");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .context("expected typed Business OS MCP error")?;

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ModuleNotFound);
        let conn = store::open_store(root)?;
        let command_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_commands WHERE record_id = 'missing-inventory'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(command_count, 0);
        Ok(())
    }

    #[test]
    fn call_tool_dispatches_query_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Metric Space",
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let result = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "limit": 5,
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("count").and_then(Value::as_u64), Some(1));
        Ok(())
    }

    #[test]
    fn call_tool_redacts_sensitive_record_fields() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Metric Space",
                    "api_key": "sk-test-secret",
                    "nested": {
                        "refresh_token": "rt-secret",
                        "label": "public-label"
                    },
                    "history": [{
                        "password": "pw-secret",
                        "status": "ok"
                    }],
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let result = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "limit": 5,
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )?;
        let output = serde_json::to_string(&result)?;

        assert_eq!(
            result
                .pointer("/items/0/data/api_key")
                .and_then(Value::as_str),
            Some(REDACTED_MCP_VALUE)
        );
        assert_eq!(
            result
                .pointer("/items/0/data/nested/refresh_token")
                .and_then(Value::as_str),
            Some(REDACTED_MCP_VALUE)
        );
        assert_eq!(
            result
                .pointer("/items/0/data/history/0/password")
                .and_then(Value::as_str),
            Some(REDACTED_MCP_VALUE)
        );
        assert_eq!(
            result
                .pointer("/items/0/data/nested/label")
                .and_then(Value::as_str),
            Some("public-label")
        );
        assert!(!output.contains("sk-test-secret"));
        assert!(!output.contains("rt-secret"));
        assert!(!output.contains("pw-secret"));
        Ok(())
    }

    #[test]
    fn mcp_tool_result_redacts_structured_content_and_text() -> anyhow::Result<()> {
        let result = mcp_tool_result(serde_json::json!({
            "ok": true,
            "token": "secret-token",
            "nested": {
                "authorization": "Bearer secret",
                "name": "visible"
            }
        }))?;
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or_default();

        assert_eq!(
            result
                .pointer("/structuredContent/token")
                .and_then(Value::as_str),
            Some(REDACTED_MCP_VALUE)
        );
        assert_eq!(
            result
                .pointer("/structuredContent/nested/authorization")
                .and_then(Value::as_str),
            Some(REDACTED_MCP_VALUE)
        );
        assert_eq!(
            result
                .pointer("/structuredContent/nested/name")
                .and_then(Value::as_str),
            Some("visible")
        );
        assert!(!text.contains("secret-token"));
        assert!(!text.contains("Bearer secret"));
        assert!(text.contains("visible"));
        Ok(())
    }

    #[test]
    fn call_tool_rejects_oversized_responses() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        let large_notes = "x".repeat(MAX_MCP_RESPONSE_BYTES + 1024);
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_big",
                    "name": "Large Account",
                    "notes": large_notes,
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let error = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "limit": 1,
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("oversized MCP responses must be rejected");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ResponseTooLarge);
        assert_eq!(typed.field.as_deref(), Some("response"));
        Ok(())
    }

    #[test]
    fn mcp_rate_limit_blocks_actor_workspace_after_configured_threshold() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:rate-limited", "admin")?;
        save_mcp_policy_env(root, &[("CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE", "2")])?;

        for index in 0..2 {
            let result = call_tool(
                root,
                "business_os.status",
                serde_json::json!({
                    "_context": {
                        "actor": "chatgpt:rate-limited",
                        "workspace": "test-workspace",
                        "request_id": format!("rate-limit-{index}")
                    }
                }),
            )?;
            assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        }

        let error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:rate-limited",
                    "workspace": "test-workspace",
                    "request_id": "rate-limit-blocked"
                }
            }),
        )
        .expect_err("third call in the same minute must be rate limited");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::RateLimited);
        assert_eq!(
            typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE")
        );
        Ok(())
    }

    #[test]
    fn mcp_policy_can_disable_channel_tool_calls() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        save_mcp_policy_env(root, &[("CTOX_BUSINESS_OS_MCP_ENABLED", "false")])?;

        let error = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("disabled channel must reject tool calls");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ChannelDisabled);
        Ok(())
    }

    #[test]
    fn mcp_policy_can_disable_reads_and_deny_specific_tools() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        save_mcp_policy_env(
            root,
            &[
                ("CTOX_BUSINESS_OS_MCP_ALLOW_READS", "false"),
                (
                    "CTOX_BUSINESS_OS_MCP_DENY_TOOLS",
                    "business_os.execute_action,business_os.execute_action",
                ),
            ],
        )?;

        let read_error = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("read policy must reject read tools");
        let read_typed = read_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed read error");
        assert_eq!(read_typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(
            mcp_policy(root).denied_tools,
            vec!["business_os.execute_action".to_string()]
        );

        let write_error = call_tool(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "tickets",
                "action_id": "ctox.delegate_task",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("denied tool must be rejected before dispatch");
        let write_typed = write_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed write error");
        assert_eq!(write_typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(
            write_typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_DENY_TOOLS")
        );
        Ok(())
    }

    #[test]
    fn mcp_policy_enforces_actor_workspace_module_and_collection_scopes() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(
            root,
            "customers",
            "Customers",
            &["customer_accounts", "customer_contacts"],
        )?;
        write_module(root, "outbound", "Outbound", &["outbound_messages"])?;
        seed_business_user(root, "chatgpt:allowed", "admin")?;
        save_mcp_policy_env(
            root,
            &[
                ("CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS", "chatgpt:allowed"),
                ("CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES", "workspace-a"),
                ("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES", "customers"),
                (
                    "CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS",
                    "customer_accounts",
                ),
            ],
        )?;

        let actor_error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:denied",
                    "workspace": "workspace-a"
                }
            }),
        )
        .expect_err("denied actor must be rejected");
        let actor_typed = actor_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(actor_typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(
            actor_typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS")
        );

        let workspace_error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:allowed",
                    "workspace": "workspace-b"
                }
            }),
        )
        .expect_err("denied workspace must be rejected");
        let workspace_typed = workspace_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(
            workspace_typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES")
        );

        let modules = call_tool(
            root,
            "business_os.list_modules",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:allowed",
                    "workspace": "workspace-a"
                }
            }),
        )?;
        assert_eq!(modules.get("count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            modules.pointer("/items/0/id").and_then(Value::as_str),
            Some("customers")
        );

        let module_error = call_tool(
            root,
            "business_os.get_module",
            serde_json::json!({
                "module_id": "outbound",
                "_context": {
                    "actor": "chatgpt:allowed",
                    "workspace": "workspace-a"
                }
            }),
        )
        .expect_err("denied module must be rejected");
        let module_typed = module_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(
            module_typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES")
        );

        let entities = call_tool(
            root,
            "business_os.list_entities",
            serde_json::json!({
                "module_id": "customers",
                "_context": {
                    "actor": "chatgpt:allowed",
                    "workspace": "workspace-a"
                }
            }),
        )?;
        assert_eq!(entities.get("count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            entities
                .pointer("/items/0/collection")
                .and_then(Value::as_str),
            Some("customer_accounts")
        );

        let collection_error = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "outbound_messages",
                "_context": {
                    "actor": "chatgpt:allowed",
                    "workspace": "workspace-a"
                }
            }),
        )
        .expect_err("denied collection must be rejected");
        let collection_typed = collection_error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(
            collection_typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS")
        );
        Ok(())
    }

    #[test]
    fn mcp_policy_blocks_external_effect_approval_by_default() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        let error = call_tool(
            root,
            "business_os.approve",
            serde_json::json!({
                "message_id": "msg_1",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test",
                    "confirmation_state": "approved"
                }
            }),
        )
        .expect_err("approval grants an external effect and is disabled by default");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(
            typed.field.as_deref(),
            Some("CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_ungranted_team_action_execution() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "customers",
                "action_id": "customers.create_followup",
                "title": "Follow up",
                "objective": "Prepare customer follow-up",
                "payload": { "customer_id": "acct_1" },
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("ungranted team user must not execute module writes");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_explicit_module_data_write_grant() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:delegate", "team")?;
        seed_business_permission_grant(
            root,
            "grant_delegate_customers_write",
            "user",
            "chatgpt:delegate",
            BusinessOsPermission::DataWrite,
            "module",
            "customers",
        )?;

        let result = call_tool(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "customers",
                "action_id": "customers.create_followup",
                "title": "Follow up",
                "objective": "Prepare customer follow-up",
                "payload": { "customer_id": "acct_1" },
                "_context": {
                    "actor": "chatgpt:delegate",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("module_id").and_then(Value::as_str),
            Some("customers")
        );
        assert_eq!(
            result
                .pointer("/client_context/actor")
                .and_then(Value::as_str),
            None
        );
        assert_eq!(
            result
                .pointer("/client_context/actor/id")
                .and_then(Value::as_str),
            Some("chatgpt:delegate")
        );
        assert_eq!(
            result
                .pointer("/client_context/mcp_actor")
                .and_then(Value::as_str),
            Some("chatgpt:delegate")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_unknown_actor_does_not_inherit_local_admin() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();

        let error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:unmapped",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("unmapped MCP actors must not inherit local bootstrap admin rights");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_unpersisted_service_actor_grant_and_audits_identity(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_permission_grant(
            root,
            "grant_service_customers_write",
            "user",
            "service:crm-agent",
            BusinessOsPermission::DataWrite,
            "module",
            "customers",
        )?;

        let result = call_tool_audited(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "customers",
                "action_id": "customers.create_followup",
                "title": "Follow up",
                "objective": "Prepare customer follow-up",
                "payload": { "customer_id": "acct_1" },
                "_context": {
                    "actor": "service:crm-agent",
                    "workspace": "test",
                    "request_id": "req_service_actor_write"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result
                .pointer("/client_context/actor/id")
                .and_then(Value::as_str),
            Some("service:crm-agent")
        );
        assert_eq!(
            result
                .pointer("/client_context/actor/persisted")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            result
                .pointer("/client_context/mcp_actor")
                .and_then(Value::as_str),
            Some("service:crm-agent")
        );

        let events = list_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(1),
        )?;
        assert_eq!(events.items[0].actor, "service:crm-agent");
        assert_eq!(
            events.items[0]
                .metadata
                .pointer("/resolved_actor/id")
                .and_then(Value::as_str),
            Some("service:crm-agent")
        );
        assert_eq!(
            events.items[0]
                .metadata
                .pointer("/resolved_actor/persisted")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            events.items[0]
                .metadata
                .pointer("/policy_decision/allowed")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_exact_approval_grant_without_outbound_module_grant(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [{
                    "id": "approval_1",
                    "message_id": "msg_1",
                    "engagement_id": "eng_1",
                    "decision": "pending",
                    "created_at_ms": 10,
                    "updated_at_ms": 10
                }]
            }),
        )?;
        seed_business_permission_grant(
            root,
            "grant_approval_agent_approval_1",
            "user",
            "service:approval-agent",
            BusinessOsPermission::ExternalApprove,
            "approval",
            "approval_1",
        )?;

        let result = call_tool(
            root,
            "business_os.reject",
            serde_json::json!({
                "approval_id": "approval_1",
                "comment": "No fit",
                "_context": {
                    "actor": "service:approval-agent",
                    "workspace": "test",
                    "confirmation_state": "approved"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("approval_id").and_then(Value::as_str),
            Some("approval_1")
        );
        assert_eq!(
            result
                .pointer("/client_context/actor/id")
                .and_then(Value::as_str),
            Some("service:approval-agent")
        );
        assert_eq!(
            result
                .pointer("/client_context/actor/persisted")
                .and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_other_approval_for_exact_approval_grant() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [
                    {
                        "id": "approval_1",
                        "message_id": "msg_1",
                        "engagement_id": "eng_1",
                        "decision": "pending",
                        "created_at_ms": 10,
                        "updated_at_ms": 10
                    },
                    {
                        "id": "approval_2",
                        "message_id": "msg_2",
                        "engagement_id": "eng_2",
                        "decision": "pending",
                        "created_at_ms": 11,
                        "updated_at_ms": 11
                    }
                ]
            }),
        )?;
        seed_business_permission_grant(
            root,
            "grant_approval_agent_approval_1_only",
            "user",
            "service:approval-agent",
            BusinessOsPermission::ExternalApprove,
            "approval",
            "approval_1",
        )?;

        let error = call_tool(
            root,
            "business_os.reject",
            serde_json::json!({
                "approval_id": "approval_2",
                "comment": "No fit",
                "_context": {
                    "actor": "service:approval-agent",
                    "workspace": "test",
                    "confirmation_state": "approved"
                }
            }),
        )
        .expect_err("approval_1 grant must not allow approval_2");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_approval_actor_user_id_does_not_replace_exact_approval_grant() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [{
                    "id": "approval_1",
                    "message_id": "msg_1",
                    "engagement_id": "eng_1",
                    "actor_user_id": "service:approval-agent",
                    "decision": "pending",
                    "created_at_ms": 10,
                    "updated_at_ms": 10
                }]
            }),
        )?;

        let error = call_tool(
            root,
            "business_os.reject",
            serde_json::json!({
                "approval_id": "approval_1",
                "comment": "No fit",
                "_context": {
                    "actor": "service:approval-agent",
                    "workspace": "test",
                    "confirmation_state": "approved"
                }
            }),
        )
        .expect_err("actor_user_id must not become an implicit approval grant");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn audited_mcp_policy_denial_records_business_os_policy_decision() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool_audited(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "customers",
                "action_id": "customers.create_followup",
                "title": "Follow up",
                "objective": "Prepare customer follow-up",
                "payload": { "customer_id": "acct_1" },
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test",
                    "request_id": "req_policy_denied"
                }
            }),
        )
        .expect_err("ungranted write must fail");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));

        let events = list_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(1),
        )?;
        let business_scope = events.items[0]
            .metadata
            .get("business_scope")
            .expect("business scope is audited");
        let policy = events.items[0]
            .metadata
            .get("policy_decision")
            .expect("policy decision is audited");

        assert_eq!(events.items[0].status, "failed");
        assert_eq!(events.items[0].request_id, "req_policy_denied");
        assert_eq!(
            business_scope.get("tool").and_then(Value::as_str),
            Some("business_os.execute_action")
        );
        assert_eq!(
            business_scope.get("module_id").and_then(Value::as_str),
            Some("customers")
        );
        assert_eq!(
            business_scope.get("action_id").and_then(Value::as_str),
            Some("customers.create_followup")
        );
        assert_eq!(business_scope.get("title"), None);
        assert_eq!(business_scope.get("objective"), None);
        assert_eq!(business_scope.get("payload"), None);
        assert_eq!(policy.get("allowed").and_then(Value::as_bool), Some(false));
        assert_eq!(
            policy.get("permission").and_then(Value::as_str),
            Some("data.write")
        );
        assert_eq!(
            policy.get("scope_type").and_then(Value::as_str),
            Some("module")
        );
        assert_eq!(
            policy.get("scope_id").and_then(Value::as_str),
            Some("customers")
        );
        assert_eq!(
            policy.get("reason_code").and_then(Value::as_str),
            Some("role_or_scope_denied")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_ungranted_team_module_read() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool(
            root,
            "business_os.get_module",
            serde_json::json!({
                "module_id": "customers",
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("ungranted team user must not read module details");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_collection_read_via_module_grant() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:reader", "team")?;
        seed_business_permission_grant(
            root,
            "grant_reader_customers_read",
            "user",
            "chatgpt:reader",
            BusinessOsPermission::DataRead,
            "module",
            "customers",
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Acme",
                    "updated_at_ms": 10
                }]
            }),
        )?;

        let result = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "_context": {
                    "actor": "chatgpt:reader",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            result.pointer("/items/0/id").and_then(Value::as_str),
            Some("acct_1")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_exact_record_grant_without_collection_read(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Acme",
                    "updated_at_ms": 10
                }, {
                    "id": "acct_2",
                    "name": "Globex",
                    "updated_at_ms": 11
                }]
            }),
        )?;
        seed_business_permission_grant(
            root,
            "grant_record_reader_acct_1",
            "user",
            "service:record-reader",
            BusinessOsPermission::DataRead,
            "record",
            &record_scope_id("customer_accounts", "acct_1"),
        )?;

        let result = call_tool(
            root,
            "business_os.get_record",
            serde_json::json!({
                "collection": "customer_accounts",
                "record_id": "acct_1",
                "_context": {
                    "actor": "service:record-reader",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.pointer("/record/id").and_then(Value::as_str),
            Some("acct_1")
        );

        let error = call_tool(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "_context": {
                    "actor": "service:record-reader",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("exact record grant must not allow listing the collection");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_record_owner_payload_field_does_not_replace_exact_record_grant() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "service:record-owner", "team")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_opportunities",
                "documents": [{
                    "id": "opp_1",
                    "name": "Opportunity",
                    "owner_id": "service:record-owner",
                    "updated_at_ms": 10
                }]
            }),
        )?;

        let error = call_tool(
            root,
            "business_os.get_record",
            serde_json::json!({
                "collection": "customer_opportunities",
                "record_id": "opp_1",
                "_context": {
                    "actor": "service:record-owner",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("owner-like payload fields must not become implicit record grants");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn audited_mcp_read_denial_records_business_os_policy_decision() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "customers", "Customers", &["customer_accounts"])?;
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool_audited(
            root,
            "business_os.query_records",
            serde_json::json!({
                "collection": "customer_accounts",
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test",
                    "request_id": "req_read_policy_denied"
                }
            }),
        )
        .expect_err("ungranted read must fail");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));

        let events = list_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(1),
        )?;
        let business_scope = events.items[0]
            .metadata
            .get("business_scope")
            .expect("business scope is audited");
        let policy = events.items[0]
            .metadata
            .get("policy_decision")
            .expect("policy decision is audited");

        assert_eq!(events.items[0].status, "failed");
        assert_eq!(events.items[0].request_id, "req_read_policy_denied");
        assert_eq!(
            business_scope.get("tool").and_then(Value::as_str),
            Some("business_os.query_records")
        );
        assert_eq!(
            business_scope.get("collection").and_then(Value::as_str),
            Some("customer_accounts")
        );
        assert_eq!(business_scope.get("query"), None);
        assert_eq!(policy.get("allowed").and_then(Value::as_bool), Some(false));
        assert_eq!(
            policy.get("permission").and_then(Value::as_str),
            Some("data.read")
        );
        assert_eq!(
            policy.get("scope_type").and_then(Value::as_str),
            Some("collection")
        );
        assert_eq!(
            policy.get("scope_id").and_then(Value::as_str),
            Some("customer_accounts")
        );
        assert_eq!(
            policy.get("reason_code").and_then(Value::as_str),
            Some("role_or_scope_denied")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_filters_module_list_by_app_visibility_not_data_read(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "private-zero",
            "Private Zero",
            "0.2.0",
            &["private_records"],
            None,
        )?;
        write_installed_module(
            root,
            "preview-zero",
            "Preview Zero",
            "0.3.0",
            &["preview_records"],
            None,
        )?;
        write_installed_module(
            root,
            "team-one",
            "Team One",
            "1.0.0",
            &["team_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:reader", "team")?;
        seed_business_permission_grant(
            root,
            "grant_reader_preview_zero_app_view",
            "user",
            "chatgpt:reader",
            BusinessOsPermission::AppsView,
            "module",
            "preview-zero",
        )?;
        seed_business_permission_grant(
            root,
            "grant_reader_private_zero_data_read",
            "user",
            "chatgpt:reader",
            BusinessOsPermission::DataRead,
            "module",
            "private-zero",
        )?;

        let result = call_tool(
            root,
            "business_os.list_modules",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:reader",
                    "workspace": "test"
                }
            }),
        )?;

        let ids = result
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| string_field(&item, "id"))
            .collect::<Vec<_>>();
        assert!(
            ids.contains(&"preview-zero".to_string()),
            "apps.view grant should make preview app visible"
        );
        assert!(
            ids.contains(&"team-one".to_string()),
            "1.0.0 app should be team-visible by default"
        );
        assert!(
            !ids.contains(&"private-zero".to_string()),
            "data.read must not make a private app visible"
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_visible_module_details_without_data_read() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "preview-zero",
            "Preview Zero",
            "0.3.0",
            &["preview_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:viewer", "team")?;
        seed_business_permission_grant(
            root,
            "grant_viewer_preview_zero_app_view",
            "user",
            "chatgpt:viewer",
            BusinessOsPermission::AppsView,
            "module",
            "preview-zero",
        )?;

        let error = call_tool(
            root,
            "business_os.get_module",
            serde_json::json!({
                "module_id": "preview-zero",
                "_context": {
                    "actor": "chatgpt:viewer",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("visible app details still require data.read");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        let decision = business_os_mcp_policy_decision(
            root,
            &McpChannelRequestContext {
                actor: "chatgpt:viewer".to_string(),
                ..test_context("business_os.get_module")
            },
            "business_os.get_module",
            &serde_json::json!({ "module_id": "preview-zero" }),
        )?
        .expect("policy decision");
        assert!(!decision.allowed);
        assert_eq!(decision.permission, BusinessOsPermission::DataRead.as_str());
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_hidden_module_even_with_data_read() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "private-zero",
            "Private Zero",
            "0.2.0",
            &["private_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:reader", "team")?;
        seed_business_permission_grant(
            root,
            "grant_reader_private_zero_data_read",
            "user",
            "chatgpt:reader",
            BusinessOsPermission::DataRead,
            "module",
            "private-zero",
        )?;

        let context = McpChannelRequestContext {
            actor: "chatgpt:reader".to_string(),
            ..test_context("business_os.get_module")
        };
        let decision = business_os_mcp_policy_decision(
            root,
            &context,
            "business_os.get_module",
            &serde_json::json!({ "module_id": "private-zero" }),
        )?
        .expect("policy decision");
        assert!(!decision.allowed);
        assert_eq!(decision.permission, BusinessOsPermission::AppsView.as_str());

        let error = call_tool(
            root,
            "business_os.get_module",
            serde_json::json!({
                "module_id": "private-zero",
                "_context": {
                    "actor": "chatgpt:reader",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("data.read must not reveal a private app without apps.view");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_hidden_action_execution_even_with_data_write(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "private-zero",
            "Private Zero",
            "0.2.0",
            &["private_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:writer", "team")?;
        seed_business_permission_grant(
            root,
            "grant_writer_private_zero_data_write",
            "user",
            "chatgpt:writer",
            BusinessOsPermission::DataWrite,
            "module",
            "private-zero",
        )?;

        let context = McpChannelRequestContext {
            actor: "chatgpt:writer".to_string(),
            ..test_context("business_os.execute_action")
        };
        let decision = business_os_mcp_policy_decision(
            root,
            &context,
            "business_os.execute_action",
            &serde_json::json!({ "module_id": "private-zero" }),
        )?
        .expect("policy decision");
        assert!(!decision.allowed);
        assert_eq!(decision.permission, BusinessOsPermission::AppsView.as_str());

        let error = call_tool(
            root,
            "business_os.execute_action",
            serde_json::json!({
                "module_id": "private-zero",
                "action_id": "ctox.delegate_task",
                "title": "Hidden write",
                "objective": "Should not execute without app visibility",
                "_context": {
                    "actor": "chatgpt:writer",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("data.write must not execute a hidden app action");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_ungranted_team_status() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("ungranted team user must not read MCP status");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_status_with_mcp_manage_grant() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:mcp-operator", "team")?;
        seed_business_permission_grant(
            root,
            "grant_mcp_operator_status",
            "user",
            "chatgpt:mcp-operator",
            BusinessOsPermission::McpManage,
            "mcp",
            "business_os_mcp",
        )?;

        let result = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:mcp-operator",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("actor").and_then(Value::as_str),
            Some("chatgpt:mcp-operator")
        );
        Ok(())
    }

    #[test]
    fn mcp_ignores_spoofed_context_role_without_gateway_trust() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();

        let error = call_tool(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "channel": "ctox_dev_managed_mcp",
                    "surface": "business_os_mcp",
                    "actor": "ctox-dev:user:owner_1",
                    "workspace": "tenant:tenant_1",
                    "auth_source": "ctox_dev_managed_mcp_token",
                    "role": "chef"
                }
            }),
        )
        .expect_err("plain tool arguments must not be able to self-assign chef");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_ungranted_open_link() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "private-zero",
            "Private Zero",
            "0.2.0",
            &["private_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:team", "team")?;

        let error = call_tool(
            root,
            "business_os.open_link",
            serde_json::json!({
                "kind": "module",
                "module_or_collection": "private-zero",
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("ungranted team user must not create scoped module link");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_allows_module_link_with_app_view_without_data_read(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_installed_module(
            root,
            "preview-zero",
            "Preview Zero",
            "0.3.0",
            &["preview_records"],
            None,
        )?;
        seed_business_user(root, "chatgpt:viewer", "team")?;
        seed_business_permission_grant(
            root,
            "grant_viewer_preview_zero_app_view",
            "user",
            "chatgpt:viewer",
            BusinessOsPermission::AppsView,
            "module",
            "preview-zero",
        )?;

        let result = call_tool(
            root,
            "business_os.open_link",
            serde_json::json!({
                "kind": "module",
                "module_or_collection": "preview-zero",
                "_context": {
                    "actor": "chatgpt:viewer",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(
            result.get("url_fragment").and_then(Value::as_str),
            Some("#module=preview-zero")
        );
        Ok(())
    }

    #[test]
    fn mcp_business_os_policy_denies_ungranted_command_status() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:team", "team")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "business_commands",
                "documents": [{
                    "id": "cmd_1",
                    "command_id": "cmd_1",
                    "module": "tickets",
                    "command_type": "ctox.delegate_task",
                    "status": "accepted",
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let error = call_tool(
            root,
            "business_os.get_command_status",
            serde_json::json!({
                "command_id": "cmd_1",
                "_context": {
                    "actor": "chatgpt:team",
                    "workspace": "test"
                }
            }),
        )
        .expect_err("ungranted team user must not read command status");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::PermissionDenied);
        assert_eq!(typed.field.as_deref(), Some("business_os_policy"));
        Ok(())
    }

    #[test]
    fn audited_call_records_mcp_channel_event() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        let _ = call_tool_audited(
            root,
            "business_os.open_link",
            serde_json::json!({
                "kind": "record",
                "module_or_collection": "customers",
                "id": "acct_1",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test",
                    "request_id": "req_audit"
                }
            }),
        )?;

        let events = list_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(10),
        )?;

        assert_eq!(events.count, 1);
        assert_eq!(events.items[0].tool, "business_os.open_link");
        assert_eq!(events.items[0].actor, "chatgpt:test");
        assert_eq!(events.items[0].status, "completed");
        assert_eq!(
            events.items[0]
                .metadata
                .pointer("/resolved_actor/id")
                .and_then(Value::as_str),
            Some("chatgpt:test")
        );
        Ok(())
    }

    #[test]
    fn audit_export_supports_jsonl() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        let _ = call_tool_audited(
            root,
            "business_os.open_link",
            serde_json::json!({
                "kind": "record",
                "module_or_collection": "customers",
                "id": "acct_1",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test",
                    "request_id": "req_audit_export"
                }
            }),
        )?;

        let export = export_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(10),
            BusinessOsMcpAuditExportFormat::Jsonl,
        )?;
        let lines = export.lines().collect::<Vec<_>>();
        let event: Value = serde_json::from_str(lines[0])?;

        assert_eq!(lines.len(), 1);
        assert_eq!(
            event.get("tool").and_then(Value::as_str),
            Some("business_os.open_link")
        );
        assert_eq!(
            event.get("actor").and_then(Value::as_str),
            Some("chatgpt:test")
        );
        assert!(export.ends_with('\n'));
        Ok(())
    }

    #[test]
    fn audit_retention_prunes_expired_mcp_events() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        let mut policy = default_mcp_policy();
        policy.audit_retention_days = 1;
        save_mcp_policy(root, &policy)?;
        let conn = store::open_store(root)?;
        let expired_ms = now_ms().saturating_sub(2 * 24 * 60 * 60 * 1000) as i64;
        conn.execute(
            "INSERT INTO business_os_mcp_events
                (event_id, channel, surface, actor, workspace, tool, request_id,
                 confirmation_state, status, metadata_json, created_at_ms)
             VALUES (?1, 'chatgpt_mcp', 'business_os_mcp', 'chatgpt:test', 'test',
                     'business_os.status', 'expired', 'not_required', 'completed', '{}', ?2)",
            params!["expired_event", expired_ms],
        )?;

        let _ = call_tool_audited(
            root,
            "business_os.status",
            serde_json::json!({
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test",
                    "request_id": "current"
                }
            }),
        )?;
        let events = list_mcp_activity(
            root,
            &test_context("business_os.list_mcp_activity"),
            Some(10),
        )?;

        assert_eq!(events.count, 1);
        assert_eq!(events.items[0].request_id, "current");
        Ok(())
    }

    #[test]
    fn propose_action_returns_non_executing_business_os_command_shape() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "outbound", "Outbound", &["outbound_campaigns"])?;
        seed_default_mcp_admin(root)?;

        let proposal = propose_action(
            root,
            &test_context("business_os.propose_action"),
            "outbound",
            "outbound.request_send_approval",
            &serde_json::json!({
                "record_id": "engagement_1",
                "title": "Approve first touch",
                "objective": "Request approval before sending",
                "payload": { "draft_id": "draft_1" }
            }),
        )?;

        assert!(proposal.ok);
        assert_eq!(proposal.command_type, "outbound.request_send_approval");
        assert!(proposal.confirmation_required);
        assert!(!proposal.would_execute);
        assert_eq!(
            proposal
                .client_context
                .pointer("/actor/id")
                .and_then(Value::as_str),
            Some("chatgpt:test-user")
        );
        assert_eq!(
            proposal
                .client_context
                .get("proposal_only")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn support_module_actions_expose_agent_writeback_contract() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "support", "Support", &["support_conversations"])?;
        seed_default_mcp_admin(root)?;

        let actions = list_module_actions(
            root,
            &test_context("business_os.list_module_actions"),
            "support",
        )?;
        let action_ids = actions
            .items
            .iter()
            .map(|action| action.action_id.as_str())
            .collect::<Vec<_>>();

        assert!(action_ids.contains(&"support.agent.writeback"));
        assert!(action_ids.contains(&"support.agent.apply_suggestion"));
        assert!(action_ids.contains(&"support.agent.reject_suggestion"));
        Ok(())
    }

    #[test]
    fn support_agent_action_proposal_keeps_typed_payload_unwrapped() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "support", "Support", &["support_conversations"])?;
        seed_default_mcp_admin(root)?;

        let proposal = propose_action(
            root,
            &test_context("business_os.propose_action"),
            "support",
            "support.agent.writeback",
            &serde_json::json!({
                "record_id": "support_conv_1",
                "payload": {
                    "source_command_id": "cmd_support_agent_1",
                    "task_id": "task_1",
                    "suggestion_kind": "summary",
                    "summary": "Customer waits for a status update.",
                    "payload": { "risk": "normal" },
                    "confidence": 0.82,
                    "required_human_action": "review"
                }
            }),
        )?;

        assert!(proposal.ok);
        assert_eq!(proposal.command_type, "support.agent.writeback");
        assert_eq!(
            proposal
                .payload
                .get("conversation_id")
                .and_then(Value::as_str),
            Some("support_conv_1")
        );
        assert_eq!(
            proposal
                .payload
                .get("source_command_id")
                .and_then(Value::as_str),
            Some("cmd_support_agent_1")
        );
        assert!(
            proposal.payload.get("input").is_none(),
            "support agent payload must not be wrapped in generic action input"
        );
        Ok(())
    }

    #[test]
    fn execute_action_requires_approval_for_risky_actions() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "matching", "Matching", &["business_matches"])?;
        seed_default_mcp_admin(root)?;

        let error = execute_action(
            root,
            &test_context("business_os.execute_action"),
            "matching",
            "matching.run_match",
            &serde_json::json!({
                "title": "Run match",
                "objective": "Find matching candidates",
                "payload": { "query": "cto" }
            }),
        )
        .expect_err("risky action without approval must fail");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ConfirmationRequired);
        Ok(())
    }

    #[test]
    fn execute_action_blocks_external_effects_even_when_approved() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_module(root, "outbound", "Outbound", &["outbound_campaigns"])?;
        seed_business_user(root, "chatgpt:test-user", "admin")?;
        let mut context = test_context("business_os.execute_action");
        context.confirmation_state = McpConfirmationState::Approved;

        let error = execute_action(
            root,
            &context,
            "outbound",
            "outbound.request_send_approval",
            &serde_json::json!({
                "record_id": "draft_1",
                "title": "Approve send",
                "objective": "Request approval before sending",
                "payload": { "draft_id": "draft_1" }
            }),
        )
        .expect_err("external effect action must be blocked in v1");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ExternalEffectBlocked);
        Ok(())
    }

    #[test]
    fn get_command_status_reads_business_command_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test", "admin")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "business_commands",
                "documents": [{
                    "id": "cmd_1",
                    "command_id": "cmd_1",
                    "module": "tickets",
                    "command_type": "ctox.delegate_task",
                    "status": "accepted",
                    "updated_at_ms": 42
                }]
            }),
        )?;

        let result = call_tool(
            root,
            "business_os.get_command_status",
            serde_json::json!({
                "command_id": "cmd_1",
                "_context": {
                    "actor": "chatgpt:test",
                    "workspace": "test"
                }
            }),
        )?;

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result
                .pointer("/record/data/command_id")
                .and_then(Value::as_str),
            Some("cmd_1")
        );
        Ok(())
    }

    #[test]
    fn json_rpc_error_contract_preserves_typed_business_os_errors() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        save_mcp_policy_env(
            root,
            &[("CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS", "chatgpt:allowed")],
        )?;

        let response = handle_json_rpc(
            root,
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "err-1",
                "method": "tools/call",
                "params": {
                    "name": "business_os.status",
                    "arguments": {
                        "_context": {
                            "actor": "chatgpt:denied",
                            "workspace": "test"
                        }
                    }
                }
            }),
        );

        assert_eq!(response.get("id").and_then(Value::as_str), Some("err-1"));
        assert_eq!(
            response.pointer("/error/code").and_then(Value::as_i64),
            Some(-32003)
        );
        assert_eq!(
            response.pointer("/error/data/code").and_then(Value::as_str),
            Some("permission_denied")
        );
        assert_eq!(
            response
                .pointer("/error/data/field")
                .and_then(Value::as_str),
            Some("CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS")
        );
        assert_eq!(
            response.pointer("/error/data/type").and_then(Value::as_str),
            Some("business_os_mcp_error")
        );
        Ok(())
    }

    #[test]
    fn json_rpc_error_contract_maps_validation_errors_to_invalid_params() {
        let temp = tempdir().expect("temp root");
        let response = handle_json_rpc(
            temp.path(),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "business_os.query_records",
                    "arguments": {
                        "_context": {
                            "actor": "chatgpt:test",
                            "workspace": "test"
                        }
                    }
                }
            }),
        );

        assert_eq!(
            response.pointer("/error/code").and_then(Value::as_i64),
            Some(-32602)
        );
        assert_eq!(
            response.pointer("/error/data/code").and_then(Value::as_str),
            Some("validation_failed")
        );
        assert_eq!(
            response
                .pointer("/error/data/field")
                .and_then(Value::as_str),
            Some("collection")
        );
    }

    #[test]
    fn gateway_message_dispatches_json_rpc_to_local_mcp_channel() -> anyhow::Result<()> {
        let temp = tempdir()?;
        write_module(temp.path(), "tickets", "Tickets", &["ctox_ticket_items"])?;

        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "mcp_request",
                "request_id": "gw_req_1",
                "body": serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }).to_string()
            })
            .to_string(),
        );
        let envelope: Value = serde_json::from_str(&response)?;
        let body: Value =
            serde_json::from_str(envelope.get("body").and_then(Value::as_str).unwrap())?;

        assert_eq!(
            envelope.get("type").and_then(Value::as_str),
            Some("mcp_response")
        );
        assert_eq!(
            envelope.get("request_id").and_then(Value::as_str),
            Some("gw_req_1")
        );
        assert_eq!(envelope.get("status").and_then(Value::as_u64), Some(200));
        assert!(body.pointer("/result/tools").is_some());
        Ok(())
    }

    #[test]
    fn gateway_context_overrides_spoofed_tool_context() -> anyhow::Result<()> {
        let temp = tempdir()?;
        seed_business_user(temp.path(), "ctox-dev:user:user_1", "admin")?;

        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "mcp_request",
                "request_id": "gw_req_context",
                "context": {
                    "actor": "ctox-dev:user:user_1",
                    "workspace": "tenant:tenant_1",
                    "instance_id": "cto1.example.com"
                },
                "body": serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "business_os.status",
                        "arguments": {
                            "_context": {
                                "actor": "spoofed",
                                "workspace": "spoofed"
                            }
                        }
                    }
                }).to_string()
            })
            .to_string(),
        );
        let envelope: Value = serde_json::from_str(&response)?;
        let body: Value =
            serde_json::from_str(envelope.get("body").and_then(Value::as_str).unwrap())?;

        assert_eq!(envelope.get("status").and_then(Value::as_u64), Some(200));
        assert_eq!(
            body.pointer("/result/content/0/text")
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .as_ref()
                .and_then(|value| value.get("actor"))
                .and_then(Value::as_str),
            Some("ctox-dev:user:user_1")
        );
        Ok(())
    }

    #[test]
    fn gateway_managed_owner_role_allows_status_without_local_user() -> anyhow::Result<()> {
        let temp = tempdir()?;

        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "mcp_request",
                "request_id": "gw_req_owner_role",
                "context": {
                    "channel": "ctox_dev_managed_mcp",
                    "surface": "business_os_mcp",
                    "actor": "ctox-dev:user:owner_1",
                    "workspace": "tenant:tenant_1",
                    "auth_source": "ctox_dev_managed_mcp_token",
                    "role": "chef"
                },
                "body": serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "business_os.status",
                        "arguments": {
                            "_context": {
                                "actor": "spoofed",
                                "workspace": "spoofed",
                                "role": "user"
                            }
                        }
                    }
                }).to_string()
            })
            .to_string(),
        );
        let envelope: Value = serde_json::from_str(&response)?;
        let body: Value =
            serde_json::from_str(envelope.get("body").and_then(Value::as_str).unwrap())?;
        let result = body
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .expect("tool result JSON");

        assert_eq!(envelope.get("status").and_then(Value::as_u64), Some(200));
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            result.get("actor").and_then(Value::as_str),
            Some("ctox-dev:user:owner_1")
        );
        Ok(())
    }

    #[test]
    fn gateway_managed_admin_role_can_upsert_user_without_local_user() -> anyhow::Result<()> {
        let temp = tempdir()?;

        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "mcp_request",
                "request_id": "gw_req_upsert_user",
                "context": {
                    "channel": "ctox_dev_managed_mcp",
                    "surface": "business_os_mcp",
                    "actor": "ctox-dev:user:admin_1",
                    "workspace": "tenant:tenant_1",
                    "auth_source": "ctox_dev_managed_mcp_token",
                    "role": "admin"
                },
                "body": serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "business_os.upsert_user",
                        "arguments": {
                            "id": "claude:user_3",
                            "display_name": "Claude User 3",
                            "role": "user",
                            "active": true,
                            "_context": {
                                "actor": "spoofed",
                                "workspace": "spoofed"
                            }
                        }
                    }
                }).to_string()
            })
            .to_string(),
        );
        let envelope: Value = serde_json::from_str(&response)?;
        let body: Value =
            serde_json::from_str(envelope.get("body").and_then(Value::as_str).unwrap())?;
        let result = body
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .expect("tool result JSON");
        let users = result
            .get("users")
            .and_then(Value::as_array)
            .context("expected users array")?;

        assert_eq!(envelope.get("status").and_then(Value::as_u64), Some(200));
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));
        assert!(users
            .iter()
            .any(|user| user.get("id").and_then(Value::as_str) == Some("claude:user_3")));
        Ok(())
    }

    #[test]
    fn gateway_message_rejects_invalid_envelopes() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "unknown",
                "request_id": "gw_req_bad",
                "body": "{}"
            })
            .to_string(),
        );
        let envelope: Value = serde_json::from_str(&response)?;
        let body: Value =
            serde_json::from_str(envelope.get("body").and_then(Value::as_str).unwrap())?;

        assert_eq!(envelope.get("status").and_then(Value::as_u64), Some(400));
        assert_eq!(
            body.pointer("/error/code").and_then(Value::as_i64),
            Some(-32600)
        );
        Ok(())
    }

    #[test]
    fn gateway_reconnect_delay_uses_bounded_exponential_backoff() {
        assert_eq!(reconnect_delay_ms(1, 30_000), 500);
        assert_eq!(reconnect_delay_ms(2, 30_000), 1_000);
        assert_eq!(reconnect_delay_ms(3, 30_000), 2_000);
        assert_eq!(reconnect_delay_ms(20, 30_000), 30_000);
        assert_eq!(reconnect_delay_ms(20, 2_500), 2_500);
        assert_eq!(reconnect_delay_ms(1, 1), 250);
    }

    #[test]
    fn gateway_hello_message_advertises_ctox_mcp_capabilities() -> anyhow::Result<()> {
        let hello: Value = serde_json::from_str(&gateway_hello_message())?;

        assert_eq!(
            hello.get("type").and_then(Value::as_str),
            Some("ctox_hello")
        );
        assert_eq!(
            hello.get("ctox_version").and_then(Value::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            hello.get("mcp_protocol_version").and_then(Value::as_str),
            Some(MCP_PROTOCOL_VERSION)
        );
        assert!(hello
            .get("capabilities")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("business_os_mcp_channel_v1")));
        assert!(
            hello
                .get("connected_at_ms")
                .and_then(Value::as_i64)
                .unwrap()
                > 0
        );
        Ok(())
    }

    #[test]
    fn get_record_reports_missing_record_with_typed_error() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let error = get_record(
            temp.path(),
            &test_context("business_os.get_record"),
            "customer_accounts",
            "missing",
        )
        .expect_err("missing record must fail");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");
        assert_eq!(typed.code, BusinessOsMcpErrorCode::RecordNotFound);
        Ok(())
    }

    #[test]
    fn get_record_context_collects_related_operational_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "customer_accounts",
                "documents": [{
                    "id": "acct_1",
                    "name": "Metric Space",
                    "updated_at_ms": 10
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "business_commands",
                "documents": [{
                    "id": "cmd_1",
                    "command_id": "cmd_1",
                    "module": "customers",
                    "command_type": "customers.create_followup",
                    "record_id": "acct_1",
                    "status": "accepted",
                    "updated_at_ms": 20
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "ctox_queue_tasks",
                "documents": [{
                    "id": "task_1",
                    "title": "Follow up",
                    "status": "queued",
                    "module": "customers",
                    "payload": { "record_id": "acct_1" },
                    "updated_at_ms": 30
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [{
                    "id": "approval_1",
                    "message_id": "msg_1",
                    "engagement_id": "eng_1",
                    "decision": "pending",
                    "payload": { "record_id": "acct_1" },
                    "created_at_ms": 40,
                    "updated_at_ms": 40
                }]
            }),
        )?;

        let context = get_record_context(
            root,
            &test_context("business_os.get_record_context"),
            "customer_accounts",
            "acct_1",
            Some(10),
        )?;

        assert_eq!(context.record.id, "acct_1");
        assert_eq!(context.commands.count, 1);
        assert!(context.runs.count >= 1);
        assert_eq!(context.approvals.count, 1);
        assert!(context.activity.count >= 3);
        Ok(())
    }

    #[test]
    fn list_runs_reads_runs_and_queue_tasks_with_status_filter() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "ctox_runs",
                "documents": [{
                    "id": "run_1",
                    "title": "Research",
                    "status": "running",
                    "updated_at_ms": 20
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "ctox_queue_tasks",
                "documents": [{
                    "id": "task_1",
                    "title": "Queued",
                    "status": "queued",
                    "module": "research",
                    "updated_at_ms": 10
                }]
            }),
        )?;

        let runs = list_runs(
            root,
            &test_context("business_os.list_runs"),
            Some("running"),
            Some(10),
        )?;

        assert_eq!(runs.count, 1);
        assert_eq!(runs.items[0].id, "run_1");
        Ok(())
    }

    #[test]
    fn list_approvals_reads_approval_records_and_approval_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [{
                    "id": "approval_1",
                    "message_id": "msg_1",
                    "engagement_id": "eng_1",
                    "decision": "pending",
                    "created_at_ms": 10,
                    "updated_at_ms": 10
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_messages",
                "documents": [{
                    "id": "msg_1",
                    "engagement_id": "eng_1",
                    "approval_status": "pending",
                    "subject": "Approval",
                    "updated_at_ms": 20
                }]
            }),
        )?;

        let approvals = list_approvals(
            root,
            &test_context("business_os.list_approvals"),
            Some("pending"),
            Some(10),
        )?;

        assert_eq!(approvals.count, 2);
        assert_eq!(approvals.items[0].id, "msg_1");
        Ok(())
    }

    #[test]
    fn list_artifacts_reads_desktop_files_and_artifact_bearing_commands() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "desktop_files",
                "documents": [{
                    "id": "file_1",
                    "name": "artifact.md",
                    "updated_at_ms": 10
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "ctox_queue_tasks",
                "documents": [{
                    "id": "task_1",
                    "title": "Browser extract",
                    "status": "completed",
                    "module": "browser",
                    "browser_extract_artifact": { "kind": "browser_extract" },
                    "updated_at_ms": 20
                }]
            }),
        )?;

        let artifacts = list_artifacts(
            root,
            &test_context("business_os.list_artifacts"),
            None,
            Some(10),
        )?;
        let ids = artifacts
            .items
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"file_1"));
        assert!(ids.contains(&"task_1"));
        Ok(())
    }

    #[test]
    fn approval_decision_requires_explicit_confirmation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let error = record_approval_decision(
            temp.path(),
            &test_context("business_os.approve"),
            &serde_json::json!({
                "message_id": "msg_1"
            }),
            "approved",
        )
        .expect_err("approval decision without explicit confirmation must fail");
        let typed = error
            .downcast_ref::<BusinessOsMcpError>()
            .expect("typed error");

        assert_eq!(typed.code, BusinessOsMcpErrorCode::ConfirmationRequired);
        Ok(())
    }

    #[test]
    fn approval_decision_enqueues_typed_outbound_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test-user", "admin")?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_approvals",
                "documents": [{
                    "id": "approval_1",
                    "message_id": "msg_1",
                    "engagement_id": "eng_1",
                    "decision": "pending",
                    "created_at_ms": 10,
                    "updated_at_ms": 10
                }]
            }),
        )?;
        let mut context = test_context("business_os.approve");
        context.confirmation_state = McpConfirmationState::Approved;

        let decision = record_approval_decision(
            root,
            &context,
            &serde_json::json!({
                "approval_id": "approval_1",
                "comment": "Looks good"
            }),
            "approved",
        )?;

        assert!(decision.ok);
        assert_eq!(decision.message_id, "msg_1");
        let command = get_record(
            root,
            &test_context("business_os.get_command_status"),
            "business_commands",
            &decision.command_id,
        )?;
        assert_eq!(
            command
                .record
                .data
                .get("command_type")
                .and_then(Value::as_str),
            Some("outbound.message.approve")
        );
        assert_eq!(
            command
                .record
                .data
                .pointer("/payload/approval_id")
                .and_then(Value::as_str),
            Some("approval_1")
        );
        Ok(())
    }

    #[test]
    fn request_changes_enqueues_typed_outbound_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "chatgpt:test-user", "admin")?;
        let mut context = test_context("business_os.request_changes");
        context.confirmation_state = McpConfirmationState::Approved;

        let decision = record_approval_decision(
            root,
            &context,
            &serde_json::json!({
                "message_id": "msg_1",
                "comment": "Please tighten the claim."
            }),
            "changes_requested",
        )?;

        assert!(decision.ok);
        assert_eq!(decision.decision, "changes_requested");
        let command = get_record(
            root,
            &test_context("business_os.get_command_status"),
            "business_commands",
            &decision.command_id,
        )?;
        assert_eq!(
            command
                .record
                .data
                .get("command_type")
                .and_then(Value::as_str),
            Some("outbound.message.request_changes")
        );
        assert_eq!(
            command
                .record
                .data
                .pointer("/payload/comment")
                .and_then(Value::as_str),
            Some("Please tighten the claim.")
        );
        Ok(())
    }

    #[test]
    fn request_changes_active_command_records_change_request() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "outbound_messages",
                "documents": [{
                    "id": "msg_1",
                    "engagement_id": "eng_1",
                    "campaign_id": "camp_1",
                    "message_type": "initial",
                    "direction": "outbound",
                    "subject": "Draft",
                    "body_text": "Hello",
                    "draft_status": "ready_for_review",
                    "approval_status": "awaiting_approval",
                    "send_status": "awaiting_approval",
                    "payload": {},
                    "created_at_ms": 10,
                    "updated_at_ms": 10
                }]
            }),
        )?;
        store::push_collection_records(
            root,
            serde_json::json!({
                "collection": "business_commands",
                "documents": [{
                    "id": "cmd_change_request",
                    "command_id": "cmd_change_request",
                    "module": "outbound",
                    "command_type": "outbound.message.request_changes",
                    "record_id": "msg_1",
                    "payload": {
                        "message_id": "msg_1",
                        "approval_id": "approval_change_1",
                        "comment": "Please revise the CTA."
                    },
                    "client_context": {
                        "channel": "chatgpt_mcp"
                    },
                    "updated_at_ms": 20
                }]
            }),
        )?;

        let message = get_record(
            root,
            &test_context("business_os.get_record"),
            "outbound_messages",
            "msg_1",
        )?;
        let approval = get_record(
            root,
            &test_context("business_os.get_record"),
            "outbound_approvals",
            "approval_change_1",
        )?;

        assert_eq!(
            message
                .record
                .data
                .get("approval_status")
                .and_then(Value::as_str),
            Some("changes_requested")
        );
        assert_eq!(
            approval.record.data.get("decision").and_then(Value::as_str),
            Some("changes_requested")
        );
        Ok(())
    }
}
