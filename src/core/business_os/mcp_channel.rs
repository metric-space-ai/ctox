// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use futures_util::SinkExt;
use futures_util::StreamExt;
use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
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

use super::store;

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;
const MAX_MCP_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_RATE_LIMIT_PER_MINUTE: usize = 120;
const DEFAULT_AUDIT_RETENTION_DAYS: usize = 90;
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const DEFAULT_GATEWAY_RECONNECT_MAX_DELAY_MS: u64 = 30_000;
const DEFAULT_GATEWAY_HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const DEFAULT_GATEWAY_MAX_CONNECTION_AGE_MS: u64 = 15 * 60 * 1000;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn serve_mcp_channel(root: &Path, options: BusinessOsMcpServeOptions) -> anyhow::Result<()> {
    let server = Server::http(&options.addr)
        .map_err(|error| anyhow::anyhow!("failed to bind Business OS MCP server: {error}"))?;
    println!("CTOX Business OS MCP listening on http://{}", options.addr);
    println!("MCP endpoint: http://{}/mcp", options.addr);
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
            "approval_gated_actions"
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
    let response = handle_json_rpc(root, parsed);
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
        .unwrap_or_default()
        .into_iter()
        .map(module_descriptor_from_value)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let policy = mcp_policy(root);
    let modules = if policy.allowed_modules.is_empty() {
        modules
    } else {
        modules
            .into_iter()
            .filter(|module| policy.allowed_modules.contains(&module.id))
            .collect()
    };
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
        .map(|collection| BusinessOsEntityDescriptor {
            module_id: module.id.clone(),
            entity_id: collection.to_string(),
            collection: collection.to_string(),
            title: titleize_collection(collection),
            read_only: true,
        })
        .collect::<Vec<_>>();
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

pub fn call_tool(root: &Path, tool_name: &str, arguments: Value) -> anyhow::Result<Value> {
    let context = context_from_arguments(tool_name, &arguments)?;
    enforce_tool_policy(root, tool_name)?;
    enforce_context_policy(root, &context)?;
    enforce_argument_scope_policy(root, tool_name, &arguments)?;
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
        argument_metadata(&arguments),
    )?;
    Ok(result)
}

pub fn call_tool_audited(root: &Path, tool_name: &str, arguments: Value) -> anyhow::Result<Value> {
    let context = context_from_arguments(tool_name, &arguments)?;
    let result = call_tool(root, tool_name, arguments.clone());
    if let Err(error) = &result {
        let _ = record_tool_event(
            root,
            &context,
            "failed",
            Some(error.to_string()),
            argument_metadata(&arguments),
        );
    }
    result
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
        "actor": &context.actor,
        "workspace": &context.workspace,
        "request_id": &context.request_id,
        "confirmation_state": confirmation_state_as_str(&context.confirmation_state),
        "mcp_tool": &context.tool
    });
    let accepted = store::record_command(
        root,
        store::BusinessCommand {
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
            "actor": &context.actor,
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
        "actor": &context.actor,
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
            call_tool_audited(root, name, arguments).and_then(mcp_tool_result)
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
    response.add_header(
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
            .map_err(|_| anyhow::anyhow!("failed to build cors header"))?,
    );
    response.add_header(
        Header::from_bytes(
            &b"Access-Control-Allow-Headers"[..],
            &b"Content-Type, Authorization"[..],
        )
        .map_err(|_| anyhow::anyhow!("failed to build cors header"))?,
    );
    response.add_header(
        Header::from_bytes(
            &b"Access-Control-Allow-Methods"[..],
            &b"GET, POST, OPTIONS"[..],
        )
        .map_err(|_| anyhow::anyhow!("failed to build cors header"))?,
    );
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
    let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| Default::default());
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
    tool_name: &str,
    arguments: &Value,
) -> anyhow::Result<()> {
    match tool_name {
        "business_os.get_module"
        | "business_os.list_entities"
        | "business_os.list_module_actions"
        | "business_os.propose_action"
        | "business_os.execute_action" => {
            if let Some(module_id) = string_field(arguments, "module_id") {
                enforce_module_policy(root, &module_id)?;
            }
        }
        "business_os.query_records"
        | "business_os.search_records"
        | "business_os.get_record"
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
        _ => {}
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
        "business_os.execute_action" => McpToolPolicyClass::Write,
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn context_from_arguments(
    tool_name: &str,
    arguments: &Value,
) -> anyhow::Result<McpChannelRequestContext> {
    let context = arguments.get("_context").unwrap_or(&Value::Null);
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
    };
    request_context.validate()?;
    Ok(request_context)
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

fn required_arg(arguments: &Value, field: &str) -> anyhow::Result<String> {
    string_field(arguments, field)
        .ok_or_else(|| anyhow::Error::new(BusinessOsMcpError::validation(field, "required")))
}

fn optional_string_arg(arguments: &Value, field: &str) -> Option<String> {
    string_field(arguments, field)
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

    fn save_mcp_policy_env(root: &Path, entries: &[(&str, &str)]) -> anyhow::Result<()> {
        let mut env_map = std::collections::BTreeMap::new();
        for (key, value) in entries {
            env_map.insert((*key).to_string(), (*value).to_string());
        }
        crate::inference::runtime_env::save_runtime_env_map(root, &env_map)?;
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
        write_module(temp.path(), "tickets", "Tickets", &["ctox_ticket_items"])?;

        let modules = list_modules(temp.path(), &test_context("business_os.list_modules"))?;

        assert_eq!(modules.count, 1);
        assert_eq!(modules.items[0].id, "tickets");
        assert_eq!(modules.items[0].collections, vec!["ctox_ticket_items"]);
        assert_eq!(modules.items[0].deep_link.url_fragment, "#module=tickets");
        Ok(())
    }

    #[test]
    fn list_entities_derives_read_only_entities_from_module_collections() -> anyhow::Result<()> {
        let temp = tempdir()?;
        write_module(
            temp.path(),
            "customers",
            "Customers",
            &["customer_accounts", "customer_contacts"],
        )?;

        let entities = list_entities(
            temp.path(),
            &test_context("business_os.list_entities"),
            "customers",
        )?;

        assert_eq!(entities.count, 2);
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
    fn call_tool_dispatches_query_records() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
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
    fn audited_call_records_mcp_channel_event() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
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
        Ok(())
    }

    #[test]
    fn audit_export_supports_jsonl() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
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
        save_mcp_policy_env(root, &[("CTOX_BUSINESS_OS_MCP_AUDIT_RETENTION_DAYS", "1")])?;
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
        write_module(temp.path(), "outbound", "Outbound", &["outbound_campaigns"])?;

        let proposal = propose_action(
            temp.path(),
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
                .get("proposal_only")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn execute_action_requires_approval_for_risky_actions() -> anyhow::Result<()> {
        let temp = tempdir()?;
        write_module(temp.path(), "matching", "Matching", &["business_matches"])?;

        let error = execute_action(
            temp.path(),
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
        write_module(temp.path(), "outbound", "Outbound", &["outbound_campaigns"])?;
        let mut context = test_context("business_os.execute_action");
        context.confirmation_state = McpConfirmationState::Approved;

        let error = execute_action(
            temp.path(),
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

        let response = handle_gateway_message(
            temp.path(),
            &serde_json::json!({
                "type": "mcp_request",
                "request_id": "gw_req_context",
                "context": {
                    "actor": "ctox-dev:user:user_1",
                    "workspace": "tenant:tenant_1",
                    "instance_id": "cto1.kunstmen.com"
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
