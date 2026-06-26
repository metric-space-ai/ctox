use anyhow::{bail, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use url::Url;

use crate::communication::adapters::{
    AdapterSyncCommandRequest, ChatSendCommandRequest, ChatTestCommandRequest,
};
use crate::communication::email_native as communication_email_native;
use crate::communication::microsoft_graph_auth::urlencoding_encode;
use crate::communication::runtime as communication_runtime;
use crate::mission::channels::{
    ensure_account, ensure_routing_rows_for_inbound, now_iso_string, open_channel_db, preview_text,
    record_communication_sync_run, refresh_thread, stable_digest, upsert_communication_message,
    CommunicationSyncRun, UpsertMessage,
};

const DEFAULT_LIMIT: usize = 50;
const FAKE_BASE_URL_PREFIX: &str = "ctox-fake://";
const FAKE_TOKEN: &str = "ctox-fake";
const DISCORD_GATEWAY_INTENT_GUILDS: i64 = 1 << 0;
const DISCORD_GATEWAY_INTENT_GUILD_MESSAGES: i64 = 1 << 9;
const DISCORD_GATEWAY_INTENT_DIRECT_MESSAGES: i64 = 1 << 12;
const DISCORD_GATEWAY_INTENT_MESSAGE_CONTENT: i64 = 1 << 15;
const SLACK_SOCKET_MODE_CONNECT_TIMEOUT_MS: u64 = 5_000;
const SLACK_SOCKET_MODE_IDLE_TIMEOUT_MS: u64 = 1_500;
const SLACK_SOCKET_MODE_MAX_ENVELOPES_PER_TICK: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatPlatform {
    Slack,
    Discord,
    Telegram,
    Matrix,
    Mattermost,
    Zulip,
    GoogleChat,
}

#[derive(Clone, Debug)]
struct ChatOptions {
    root: PathBuf,
    db_path: PathBuf,
    platform: ChatPlatform,
    base_url: String,
    token: String,
    realtime_token: String,
    username: String,
    account_id: String,
    workspace_id: String,
    channel_ids: Vec<String>,
    topic: String,
    limit: usize,
}

#[derive(Clone, Debug)]
struct NormalizedChatMessage {
    message_key: String,
    account_key: String,
    thread_key: String,
    remote_id: String,
    sender_display: String,
    sender_address: String,
    recipients: Vec<String>,
    subject: String,
    body_text: String,
    preview: String,
    seen: bool,
    external_created_at: String,
    metadata: Value,
}

#[derive(Clone, Debug, Default)]
struct ChatSyncOutput {
    messages: Vec<NormalizedChatMessage>,
    post_store_updates: Vec<ChatPostStoreUpdate>,
}

impl ChatSyncOutput {
    fn messages(messages: Vec<NormalizedChatMessage>) -> Self {
        Self {
            messages,
            post_store_updates: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum ChatPostStoreUpdate {
    ZulipUpdateMessage { event: Value },
}

#[derive(Debug, Default)]
struct SlackSocketModeCycleOutput {
    envelopes_seen: usize,
    envelopes_acked: usize,
    duplicate_envelopes: usize,
    ignored_envelopes: usize,
    messages: Vec<NormalizedChatMessage>,
}

#[derive(Debug, Default)]
struct SlackSocketModeEnvelopeOutcome {
    envelope_id: Option<String>,
    duplicate: bool,
    message: Option<NormalizedChatMessage>,
}

impl ChatPlatform {
    fn channel(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Matrix => "matrix",
            Self::Mattermost => "mattermost",
            Self::Zulip => "zulip",
            Self::GoogleChat => "google_chat",
        }
    }

    fn provider(self) -> &'static str {
        match self {
            Self::Slack => "slack-web-api",
            Self::Discord => "discord-rest",
            Self::Telegram => "telegram-bot-api",
            Self::Matrix => "matrix-client-server",
            Self::Mattermost => "mattermost-api-v4",
            Self::Zulip => "zulip-rest-api",
            Self::GoogleChat => "google-chat-api",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Slack => "Slack",
            Self::Discord => "Discord",
            Self::Telegram => "Telegram",
            Self::Matrix => "Matrix",
            Self::Mattermost => "Mattermost",
            Self::Zulip => "Zulip",
            Self::GoogleChat => "Google Chat",
        }
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::Slack => "https://slack.com/api",
            Self::Discord => "https://discord.com/api/v10",
            Self::Telegram => "https://api.telegram.org",
            Self::Matrix => "",
            Self::Mattermost => "",
            Self::Zulip => "",
            Self::GoogleChat => "https://chat.googleapis.com",
        }
    }
}

pub(crate) fn sync(
    platform: ChatPlatform,
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<Value> {
    let options = options_from_sync_args(platform, root, runtime, request)?;
    execute_sync(&options)
}

pub(crate) fn send(
    platform: ChatPlatform,
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &ChatSendCommandRequest<'_>,
) -> Result<Value> {
    let options = options_from_runtime(platform, root, runtime, request.db_path);
    execute_send(&options, request)
}

pub(crate) fn test(
    platform: ChatPlatform,
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &ChatTestCommandRequest<'_>,
) -> Result<Value> {
    let mut options = options_from_runtime(platform, root, runtime, request.db_path);
    if let Some(profile) = request.profile_json.as_object() {
        merge_profile_json(&mut options, profile);
    }
    execute_test(&options)
}

pub(crate) fn service_sync(
    platform: ChatPlatform,
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let runtime = crate::communication::gateway::runtime_settings_from_settings(
        root,
        platform_to_gateway_kind(platform),
        settings,
    );
    let db_path = root.join("runtime/ctox.sqlite3");
    let options = options_from_runtime(platform, root, &runtime, &db_path);
    if !has_minimum_sync_config(&options) {
        return Ok(None);
    }
    if platform == ChatPlatform::Slack {
        return service_sync_slack(&options).map(Some);
    }
    let args = vec!["sync".to_string()];
    let request = AdapterSyncCommandRequest {
        db_path: db_path.as_path(),
        passthrough_args: &args,
        skip_flags: &["--db", "--channel"],
    };
    sync(platform, root, &runtime, &request).map(Some)
}

fn service_sync_slack(options: &ChatOptions) -> Result<Value> {
    let mut result = execute_sync(options)?;
    let socket_result = if realtime_config_state(options) == "configured" {
        execute_slack_socket_mode_service_sync(options)
            .unwrap_or_else(|error| slack_socket_mode_service_error(options, error))
    } else {
        json!({
            "ok": false,
            "status": "not_configured",
            "reason": realtime_config_state(options),
        })
    };
    if let Some(object) = result.as_object_mut() {
        object.insert("socketMode".to_string(), socket_result);
    }
    Ok(result)
}

fn execute_slack_socket_mode_service_sync(options: &ChatOptions) -> Result<Value> {
    let backoff_until = realtime_backoff_until_timestamp_ms(options);
    let now_ms = current_unix_millis();
    if backoff_until.is_some_and(|until| until > now_ms) {
        mark_slack_socket_mode_supervisor_state(options, "backing_off", None)?;
        return Ok(json!({
            "ok": true,
            "status": "backing_off",
            "realtime_backoff_until_ms": backoff_until,
        }));
    }

    let account_key = configured_account_key(options);
    let started_at = now_iso_string();
    let cycle = run_slack_socket_mode_cycle(options, &account_key)?;
    let finished_at = now_iso_string();

    let mut conn = open_channel_db(&options.db_path)?;
    let mut stored_count = 0usize;
    for message in cycle.messages {
        store_chat_message(&mut conn, options.platform, &message)?;
        refresh_thread(&mut conn, &message.thread_key)?;
        stored_count += 1;
    }
    ensure_routing_rows_for_inbound(&conn)?;
    if stored_count > 0 {
        mark_account_activity(&mut conn, &account_key, Some(&finished_at), None)?;
    }
    ensure_account(
        &mut conn,
        &account_key,
        options.platform.channel(),
        &configured_account_address(options),
        options.platform.provider(),
        profile_json(
            options,
            &json!({}),
            adapter_status("realtime_sync", true, None, last_cursor(options), options),
        ),
    )?;
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &format!(
                "socket-mode-{}",
                stable_digest(&format!("{account_key}:{started_at}"))
            ),
            channel: options.platform.channel(),
            account_key: &account_key,
            folder_hint: "SOCKET_MODE",
            started_at: &started_at,
            finished_at: &finished_at,
            ok: true,
            fetched_count: cycle.envelopes_seen as i64,
            stored_count: stored_count as i64,
            error_text: "",
            metadata_json: &serde_json::to_string(&json!({
                "adapter": "native-rust-slack-socket-mode",
                "envelopesAcked": cycle.envelopes_acked,
                "duplicateEnvelopes": cycle.duplicate_envelopes,
                "ignoredEnvelopes": cycle.ignored_envelopes,
            }))?,
        },
    )?;

    clear_realtime_backoff(options)?;
    Ok(json!({
        "ok": true,
        "status": "stopped",
        "envelopesSeen": cycle.envelopes_seen,
        "envelopesAcked": cycle.envelopes_acked,
        "duplicateEnvelopes": cycle.duplicate_envelopes,
        "ignoredEnvelopes": cycle.ignored_envelopes,
        "storedCount": stored_count,
    }))
}

fn slack_socket_mode_service_error(options: &ChatOptions, error: anyhow::Error) -> Value {
    let error_text = redact_sensitive_text(options, &error.to_string());
    let attempt = next_realtime_backoff_attempt(options);
    let backoff_until = record_realtime_backoff(options, "slack_socket_mode_failed", attempt)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let _ = mark_slack_socket_mode_supervisor_state(options, "failed", Some(&error_text));
    if let Ok(mut conn) = open_channel_db(&options.db_path) {
        let account_key = configured_account_key(options);
        let _ = ensure_account(
            &mut conn,
            &account_key,
            options.platform.channel(),
            &configured_account_address(options),
            options.platform.provider(),
            profile_json(
                options,
                &json!({}),
                adapter_status(
                    "realtime_sync",
                    false,
                    Some(error_text.clone()),
                    last_cursor(options),
                    options,
                ),
            ),
        );
    }
    json!({
        "ok": false,
        "status": "failed",
        "error": error_text,
        "realtime_backoff_until_ms": backoff_until,
    })
}

fn platform_to_gateway_kind(
    platform: ChatPlatform,
) -> crate::communication::gateway::CommunicationAdapterKind {
    use crate::communication::gateway::CommunicationAdapterKind;
    match platform {
        ChatPlatform::Slack => CommunicationAdapterKind::Slack,
        ChatPlatform::Discord => CommunicationAdapterKind::Discord,
        ChatPlatform::Telegram => CommunicationAdapterKind::Telegram,
        ChatPlatform::Matrix => CommunicationAdapterKind::Matrix,
        ChatPlatform::Mattermost => CommunicationAdapterKind::Mattermost,
        ChatPlatform::Zulip => CommunicationAdapterKind::Zulip,
        ChatPlatform::GoogleChat => CommunicationAdapterKind::GoogleChat,
    }
}

fn execute_test(options: &ChatOptions) -> Result<Value> {
    if !has_minimum_auth_config(options) {
        return Ok(json!({
            "ok": false,
            "adapter": options.platform.channel(),
            "status": "missing_config",
            "error": missing_config_message(options.platform),
        }));
    }

    let value = if is_fake_mode(options) {
        Ok(fake_test_value(options))
    } else {
        match options.platform {
            ChatPlatform::Slack => test_slack_value(options),
            ChatPlatform::Discord => test_discord_value(options),
            ChatPlatform::Telegram => http_json(
                "GET",
                &telegram_url(options, "getMe")?,
                &BTreeMap::new(),
                None,
            ),
            ChatPlatform::Matrix => http_json(
                "GET",
                &api_url(options, "/_matrix/client/v3/account/whoami")?,
                &bearer_headers(&options.token),
                None,
            ),
            ChatPlatform::Mattermost => test_mattermost_value(options),
            ChatPlatform::Zulip => test_zulip_value(options),
            ChatPlatform::GoogleChat => http_json(
                "GET",
                &api_url(options, "/v1/spaces?pageSize=1")?,
                &bearer_headers(&options.token),
                None,
            ),
        }
    };

    match value {
        Ok(value) => {
            let mut conn = open_channel_db(&options.db_path)?;
            let account_key = account_key_from_test_value(options, &value);
            ensure_account(
                &mut conn,
                &account_key,
                options.platform.channel(),
                &account_address_from_test_value(options, &value),
                options.platform.provider(),
                profile_json(
                    options,
                    &value,
                    adapter_status("test", true, None, None, options),
                ),
            )?;
            Ok(json!({
                "ok": true,
                "adapter": options.platform.channel(),
                "status": "connected",
                "account_key": account_key,
                "account": value,
            }))
        }
        Err(error) => {
            let error_text = redact_sensitive_text(options, &error.to_string());
            let mut conn = open_channel_db(&options.db_path)?;
            let account_key = configured_account_key(options);
            ensure_account(
                &mut conn,
                &account_key,
                options.platform.channel(),
                &configured_account_address(options),
                options.platform.provider(),
                profile_json(
                    options,
                    &json!({}),
                    adapter_status("test", false, Some(error_text.clone()), None, options),
                ),
            )?;
            Ok(json!({
                "ok": false,
                "adapter": options.platform.channel(),
                "status": "failed",
                "account_key": account_key,
                "error": error_text,
            }))
        }
    }
}

fn execute_sync(options: &ChatOptions) -> Result<Value> {
    if !has_minimum_sync_config(options) {
        bail!("{}", missing_config_message(options.platform));
    }

    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = configured_account_key(options);
    ensure_account(
        &mut conn,
        &account_key,
        options.platform.channel(),
        &configured_account_address(options),
        options.platform.provider(),
        profile_json(
            options,
            &json!({}),
            adapter_status("sync", true, None, last_cursor(options), options),
        ),
    )?;

    let started_at = now_iso_string();
    let mut fetched_count = 0usize;
    let mut stored_count = 0usize;
    let mut updated_count = 0usize;
    let mut errors = Vec::new();

    let sync_output = if is_fake_mode(options) {
        fake_sync_messages(options, &account_key).map(ChatSyncOutput::messages)
    } else {
        match options.platform {
            ChatPlatform::Slack => {
                sync_slack_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
            ChatPlatform::Discord => {
                sync_discord_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
            ChatPlatform::Telegram => {
                sync_telegram_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
            ChatPlatform::Matrix => {
                sync_matrix_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
            ChatPlatform::Mattermost => {
                sync_mattermost_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
            ChatPlatform::Zulip => sync_zulip_messages(options, &account_key),
            ChatPlatform::GoogleChat => {
                sync_google_chat_messages(options, &account_key).map(ChatSyncOutput::messages)
            }
        }
    };

    match sync_output {
        Ok(sync_output) => {
            fetched_count = sync_output.messages.len() + sync_output.post_store_updates.len();
            for message in sync_output.messages {
                store_chat_message(&mut conn, options.platform, &message)?;
                stored_count += 1;
            }
            for update in sync_output.post_store_updates {
                updated_count +=
                    apply_chat_post_store_update(&mut conn, options, &account_key, update)?;
            }
        }
        Err(error) => errors.push(redact_sensitive_text(options, &error.to_string())),
    }

    ensure_routing_rows_for_inbound(&conn)?;
    let finished_at = now_iso_string();
    let error_text = errors.join("; ");
    ensure_account(
        &mut conn,
        &account_key,
        options.platform.channel(),
        &configured_account_address(options),
        options.platform.provider(),
        profile_json(
            options,
            &json!({}),
            adapter_status(
                "sync",
                errors.is_empty(),
                (!errors.is_empty()).then(|| error_text.clone()),
                last_cursor(options),
                options,
            ),
        ),
    )?;
    if errors.is_empty() {
        mark_account_activity(&mut conn, &account_key, Some(&finished_at), None)?;
    }
    let run_key = format!(
        "sync-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            options.platform.channel(),
            account_key,
            started_at
        ))
    );
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &run_key,
            channel: options.platform.channel(),
            account_key: &account_key,
            folder_hint: "INBOX",
            started_at: &started_at,
            finished_at: &finished_at,
            ok: errors.is_empty(),
            fetched_count: fetched_count as i64,
            stored_count: stored_count as i64,
            error_text: &error_text,
            metadata_json: &serde_json::to_string(&json!({
                "adapter": format!("native-rust-{}", options.platform.channel()),
                "channelIds": options.channel_ids,
                "updatedCount": updated_count,
            }))?,
        },
    )?;

    Ok(json!({
        "ok": errors.is_empty(),
        "adapter": options.platform.channel(),
        "account_key": account_key,
        "fetchedCount": fetched_count,
        "storedCount": stored_count,
        "updatedCount": updated_count,
        "errors": errors,
    }))
}

fn execute_send(options: &ChatOptions, request: &ChatSendCommandRequest<'_>) -> Result<Value> {
    if !has_minimum_auth_config(options) {
        bail!("{}", missing_config_message(options.platform));
    }
    if !request.attachments.is_empty() {
        bail!(
            "{} native chat adapter v1 is text-only: {} attachment(s) rejected until provider-specific upload, MIME, size, persistence, and security-review handling is implemented",
            options.platform.display_name(),
            request.attachments.len(),
        );
    }

    let destination = resolve_destination(options, request)?;
    let timestamp = now_iso_string();
    let fallback_remote_id = format!(
        "queued-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            options.platform.channel(),
            destination,
            request.body
        ))
    );
    let send_result = if is_fake_mode(options) {
        Ok(fake_send_result(options, request, &destination))
    } else {
        match options.platform {
            ChatPlatform::Slack => send_slack_message(options, request, &destination),
            ChatPlatform::Discord => send_discord_message(options, request, &destination),
            ChatPlatform::Telegram => send_telegram_message(options, request, &destination),
            ChatPlatform::Matrix => send_matrix_message(options, request, &destination),
            ChatPlatform::Mattermost => send_mattermost_message(options, request, &destination),
            ChatPlatform::Zulip => send_zulip_message(options, request, &destination),
            ChatPlatform::GoogleChat => send_google_chat_message(options, request, &destination),
        }
    };
    let delivery_confirmed = send_result.is_ok();
    let sent_value = send_result.as_ref().ok();
    let send_error_text = send_result
        .as_ref()
        .err()
        .map(|error| redact_sensitive_text(options, &error.to_string()));
    let sent_remote_id = remote_id_from_send_result(options.platform, sent_value)
        .unwrap_or_else(|| fallback_remote_id.clone());
    let account_key = if request.account_key.trim().is_empty() {
        configured_account_key(options)
    } else {
        request.account_key.trim().to_string()
    };
    let thread_key = if request.thread_key.trim().is_empty() {
        thread_key_for_destination(
            options.platform,
            &account_key,
            &destination,
            &sent_remote_id,
        )
    } else {
        request.thread_key.trim().to_string()
    };
    let subject = if request.subject.trim().is_empty() {
        format!("({})", options.platform.display_name())
    } else {
        request.subject.trim().to_string()
    };

    let mut conn = open_channel_db(&options.db_path)?;
    ensure_account(
        &mut conn,
        &account_key,
        options.platform.channel(),
        &configured_account_address(options),
        options.platform.provider(),
        profile_json(
            options,
            &json!({}),
            adapter_status(
                "send",
                delivery_confirmed,
                send_error_text.clone(),
                last_cursor(options),
                options,
            ),
        ),
    )?;
    if delivery_confirmed {
        mark_account_activity(&mut conn, &account_key, None, Some(&timestamp))?;
    }
    let message_key = format!("{account_key}::SENT::{sent_remote_id}");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: options.platform.channel(),
            account_key: &account_key,
            thread_key: &thread_key,
            remote_id: &sent_remote_id,
            direction: "outbound",
            folder_hint: "SENT",
            sender_display: request.sender_display.unwrap_or("CTOX Bot"),
            sender_address: &configured_account_address(options),
            recipient_addresses_json: &serde_json::to_string(&request.to)?,
            cc_addresses_json: &serde_json::to_string(&request.cc)?,
            bcc_addresses_json: "[]",
            subject: &subject,
            preview: &preview_text(request.body, &subject),
            body_text: request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: if delivery_confirmed { "sent" } else { "failed" },
            seen: true,
            has_attachments: false,
            external_created_at: &timestamp,
            observed_at: &timestamp,
            metadata_json: &serde_json::to_string(&json!({
                "adapter": options.platform.channel(),
                "destination": destination,
                "providerResponse": sent_value,
                "error": send_error_text.clone(),
            }))?,
        },
    )?;
    refresh_thread(&mut conn, &thread_key)?;

    Ok(json!({
        "ok": true,
        "adapter": options.platform.channel(),
        "status": if delivery_confirmed { "sent" } else { "failed" },
        "delivery": {
            "confirmed": delivery_confirmed,
            "message_key": message_key,
            "remote_id": sent_remote_id,
        },
        "error": send_error_text,
        "adapter_result": sent_value,
    }))
}

fn sync_slack_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut out = Vec::new();
    for channel_id in required_destinations(options)? {
        let state_key = destination_state_key("slack-latest-ts", &channel_id);
        let previous_cursor = read_state_value(options, &state_key)?;
        let mut url = Url::parse(&api_url(options, "/conversations.history")?)?;
        url.query_pairs_mut()
            .append_pair("channel", &channel_id)
            .append_pair("limit", &options.limit.to_string());
        if let Some(cursor) = previous_cursor.as_deref().filter(|value| !value.is_empty()) {
            url.query_pairs_mut()
                .append_pair("oldest", cursor)
                .append_pair("inclusive", "false");
        }
        let value = http_json("GET", url.as_str(), &bearer_headers(&options.token), None)?;
        ensure_slack_ok(&value)?;
        let mut max_ts = previous_cursor.clone();
        for message in value
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(ts) = value_str(message, "ts") {
                max_ts = max_decimal_cursor(max_ts, ts);
            }
            out.push(normalize_slack_message(
                options,
                account_key,
                &channel_id,
                message,
            ));
        }
        if max_ts != previous_cursor {
            if let Some(max_ts) = max_ts.as_deref() {
                write_state_value(options, &state_key, max_ts)?;
            }
        }
    }
    Ok(out)
}

fn run_slack_socket_mode_cycle(
    options: &ChatOptions,
    account_key: &str,
) -> Result<SlackSocketModeCycleOutput> {
    let open = slack_socket_mode_open(options)?;
    let websocket_url = value_str(&open, "url")
        .filter(|value| value.starts_with("wss://"))
        .ok_or_else(|| {
            anyhow::anyhow!("Slack Socket Mode open response did not include wss url")
        })?;
    mark_slack_socket_mode_supervisor_state(options, "starting", None)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(slack_socket_mode_websocket_cycle(
        options.clone(),
        account_key.to_string(),
        websocket_url,
    ));
    match &result {
        Ok(_) => mark_slack_socket_mode_supervisor_state(options, "stopped", None)?,
        Err(error) => {
            mark_slack_socket_mode_supervisor_state(
                options,
                "failed",
                Some(&redact_sensitive_text(options, &error.to_string())),
            )?;
        }
    }
    result
}

fn slack_socket_mode_open(options: &ChatOptions) -> Result<Value> {
    let request = slack_socket_mode_open_request(options)?;
    let value = http_json(request.method, &request.url, &request.headers, None)?;
    ensure_slack_ok(&value)?;
    Ok(value)
}

async fn slack_socket_mode_websocket_cycle(
    options: ChatOptions,
    account_key: String,
    websocket_url: String,
) -> Result<SlackSocketModeCycleOutput> {
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_millis(SLACK_SOCKET_MODE_CONNECT_TIMEOUT_MS),
        connect_async(&websocket_url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Slack Socket Mode connect timed out"))??;
    mark_slack_socket_mode_supervisor_state(&options, "running", None)?;

    let mut output = SlackSocketModeCycleOutput::default();
    for _ in 0..SLACK_SOCKET_MODE_MAX_ENVELOPES_PER_TICK {
        let message = match tokio::time::timeout(
            Duration::from_millis(SLACK_SOCKET_MODE_IDLE_TIMEOUT_MS),
            socket.next(),
        )
        .await
        {
            Ok(Some(message)) => message,
            Ok(None) | Err(_) => break,
        };
        let message = message?;
        match message {
            WebSocketMessage::Text(text) => {
                output.envelopes_seen += 1;
                let value: Value = serde_json::from_str(&text)?;
                let outcome = normalize_slack_socket_mode_envelope(&options, &account_key, &value)?;
                if let Some(envelope_id) = outcome.envelope_id.as_deref() {
                    let ack = slack_socket_mode_ack_payload(envelope_id)?;
                    socket
                        .send(WebSocketMessage::Text(ack.to_string().into()))
                        .await?;
                    output.envelopes_acked += 1;
                }
                if outcome.duplicate {
                    output.duplicate_envelopes += 1;
                    continue;
                }
                if let Some(message) = outcome.message {
                    output.messages.push(message);
                } else {
                    output.ignored_envelopes += 1;
                }
            }
            WebSocketMessage::Ping(payload) => {
                socket.send(WebSocketMessage::Pong(payload)).await?;
            }
            WebSocketMessage::Close(_) => break,
            _ => {}
        }
    }
    let _ = socket.close(None).await;
    Ok(output)
}

fn normalize_slack_socket_mode_envelope(
    options: &ChatOptions,
    account_key: &str,
    envelope: &Value,
) -> Result<SlackSocketModeEnvelopeOutcome> {
    let envelope_id = value_str(envelope, "envelope_id");
    let duplicate = if let Some(envelope_id) = envelope_id.as_deref() {
        !mark_slack_socket_mode_envelope_seen(options, envelope_id)?
    } else {
        false
    };
    if duplicate {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: true,
            message: None,
        });
    }
    let Some(event) = slack_socket_mode_event(envelope) else {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: false,
            message: None,
        });
    };
    if event.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: false,
            message: None,
        });
    }
    if event.get("subtype").and_then(Value::as_str).is_some() {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: false,
            message: None,
        });
    }
    let channel_id = value_str(event, "channel").unwrap_or_default();
    if channel_id.trim().is_empty() {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: false,
            message: None,
        });
    }
    if !options.channel_ids.is_empty()
        && !options
            .channel_ids
            .iter()
            .any(|candidate| candidate == &channel_id)
    {
        return Ok(SlackSocketModeEnvelopeOutcome {
            envelope_id,
            duplicate: false,
            message: None,
        });
    }
    Ok(SlackSocketModeEnvelopeOutcome {
        envelope_id,
        duplicate: false,
        message: Some(normalize_slack_message(
            options,
            account_key,
            &channel_id,
            event,
        )),
    })
}

fn slack_socket_mode_event(envelope: &Value) -> Option<&Value> {
    envelope
        .get("payload")
        .and_then(|payload| payload.get("event"))
        .or_else(|| envelope.get("event"))
}

fn sync_discord_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut out = Vec::new();
    for channel_id in required_destinations(options)? {
        let state_key = destination_state_key("discord-latest-id", &channel_id);
        let previous_cursor = read_state_value(options, &state_key)?;
        let mut url = Url::parse(&api_url(
            options,
            &format!("/channels/{}/messages", urlencoding_encode(&channel_id)),
        )?)?;
        url.query_pairs_mut()
            .append_pair("limit", &options.limit.min(100).to_string());
        if let Some(cursor) = previous_cursor.as_deref().filter(|value| !value.is_empty()) {
            url.query_pairs_mut().append_pair("after", cursor);
        }
        let value = http_json("GET", url.as_str(), &discord_headers(&options.token), None)?;
        let mut max_id = previous_cursor.clone();
        for message in value.as_array().into_iter().flatten() {
            if let Some(id) = value_str(message, "id") {
                max_id = max_numeric_cursor(max_id, id);
            }
            out.push(normalize_discord_message(
                options,
                account_key,
                &channel_id,
                message,
            ));
        }
        if max_id != previous_cursor {
            if let Some(max_id) = max_id.as_deref() {
                write_state_value(options, &state_key, max_id)?;
            }
        }
    }
    Ok(out)
}

fn normalize_discord_gateway_message_create(
    options: &ChatOptions,
    account_key: &str,
    event: &Value,
) -> Result<Option<NormalizedChatMessage>> {
    if event.get("t").and_then(Value::as_str) != Some("MESSAGE_CREATE") {
        return Ok(None);
    }
    if let Some(sequence) = event.get("s").and_then(Value::as_i64) {
        persist_discord_gateway_sequence(options, sequence)?;
    }
    let Some(message) = event.get("d") else {
        return Ok(None);
    };
    let channel_id = value_str(message, "channel_id").unwrap_or_default();
    if channel_id.trim().is_empty() {
        return Ok(None);
    }
    if !options.channel_ids.is_empty()
        && !options
            .channel_ids
            .iter()
            .any(|candidate| candidate == &channel_id)
    {
        return Ok(None);
    }
    Ok(Some(normalize_discord_message(
        options,
        account_key,
        &channel_id,
        message,
    )))
}

fn sync_telegram_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut url = Url::parse(&telegram_url(options, "getUpdates")?)?;
    if let Some(offset) = read_state_value(options, "update-offset")? {
        url.query_pairs_mut().append_pair("offset", &offset);
    }
    url.query_pairs_mut()
        .append_pair("timeout", "0")
        .append_pair("limit", &options.limit.min(100).to_string());
    let value = http_json("GET", url.as_str(), &BTreeMap::new(), None)?;
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        bail!("Telegram getUpdates failed: {value}");
    }
    let allowed = options
        .channel_ids
        .iter()
        .map(|id| id.trim().to_string())
        .collect::<Vec<_>>();
    let mut max_update_id: Option<i64> = None;
    let mut out = Vec::new();
    for update in value
        .get("result")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(update_id) = update.get("update_id").and_then(Value::as_i64) {
            max_update_id = Some(max_update_id.map_or(update_id, |current| current.max(update_id)));
        }
        let Some(message) = update
            .get("message")
            .or_else(|| update.get("channel_post"))
            .or_else(|| update.get("edited_message"))
        else {
            continue;
        };
        let chat_id = message
            .get("chat")
            .and_then(|chat| chat.get("id"))
            .map(value_to_string)
            .unwrap_or_default();
        if !allowed.is_empty() && !allowed.iter().any(|candidate| candidate == &chat_id) {
            continue;
        }
        out.push(normalize_telegram_message(
            options,
            account_key,
            &chat_id,
            update,
            message,
        ));
    }
    if let Some(max_update_id) = max_update_id {
        write_state_value(options, "update-offset", &(max_update_id + 1).to_string())?;
    }
    Ok(out)
}

fn sync_matrix_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut url = Url::parse(&api_url(options, "/_matrix/client/v3/sync")?)?;
    if let Some(since) = read_state_value(options, "sync-token")? {
        url.query_pairs_mut().append_pair("since", &since);
    }
    url.query_pairs_mut().append_pair("timeout", "0");
    let value = http_json("GET", url.as_str(), &bearer_headers(&options.token), None)?;
    if let Some(next_batch) = value.get("next_batch").and_then(Value::as_str) {
        write_state_value(options, "sync-token", next_batch)?;
    }
    let mut out = Vec::new();
    let Some(joined) = value
        .get("rooms")
        .and_then(|rooms| rooms.get("join"))
        .and_then(Value::as_object)
    else {
        return Ok(out);
    };
    let mut encrypted_events_seen = 0usize;
    for (room_id, room) in joined {
        if !options.channel_ids.is_empty()
            && !options
                .channel_ids
                .iter()
                .any(|candidate| candidate == room_id)
        {
            continue;
        }
        let events = room
            .get("timeline")
            .and_then(|timeline| timeline.get("events"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for event in events {
            match event.get("type").and_then(Value::as_str) {
                Some("m.room.message") => {}
                Some("m.room.encrypted") => {
                    encrypted_events_seen += 1;
                    continue;
                }
                _ => continue,
            }
            if event
                .get("content")
                .and_then(|content| content.get("msgtype"))
                .and_then(Value::as_str)
                .is_none()
            {
                continue;
            }
            out.push(normalize_matrix_message(
                options,
                account_key,
                room_id,
                &event,
            ));
        }
    }
    if encrypted_events_seen > 0 {
        write_state_value(
            options,
            "matrix-encrypted-events-seen",
            &encrypted_events_seen.to_string(),
        )?;
        write_state_value(
            options,
            "matrix-e2ee-state",
            "encrypted_events_not_supported",
        )?;
        write_state_value(
            options,
            "matrix-sdk-state-persistence",
            "required_for_e2ee_not_configured",
        )?;
        write_state_value(
            options,
            "matrix-e2ee-policy",
            "disabled_until_sdk_state_store",
        )?;
    }
    Ok(out)
}

fn sync_mattermost_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut out = Vec::new();
    for channel_id in required_destinations(options)? {
        let state_key = destination_state_key("mattermost-latest-create-at", &channel_id);
        let previous_cursor = read_state_value(options, &state_key)?;
        let mut url = Url::parse(&api_url(
            options,
            &format!("/channels/{}/posts", urlencoding_encode(&channel_id)),
        )?)?;
        url.query_pairs_mut()
            .append_pair("page", "0")
            .append_pair("per_page", &options.limit.to_string());
        if let Some(cursor) = previous_cursor.as_deref().filter(|value| !value.is_empty()) {
            url.query_pairs_mut().append_pair("since", cursor);
        }
        let value = http_json("GET", url.as_str(), &bearer_headers(&options.token), None)?;
        let order = value
            .get("order")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let posts = value
            .get("posts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut max_create_at = previous_cursor.clone();
        for id in order {
            let Some(id) = id.as_str() else {
                continue;
            };
            if let Some(post) = posts.get(id) {
                if let Some(create_at) = post.get("create_at").map(value_to_string) {
                    max_create_at = max_numeric_cursor(max_create_at, create_at);
                }
                out.push(normalize_mattermost_post(
                    options,
                    account_key,
                    &channel_id,
                    post,
                ));
            }
        }
        if max_create_at != previous_cursor {
            if let Some(max_create_at) = max_create_at.as_deref() {
                write_state_value(options, &state_key, max_create_at)?;
            }
        }
    }
    Ok(out)
}

fn sync_zulip_messages(options: &ChatOptions, account_key: &str) -> Result<ChatSyncOutput> {
    let mut out = ChatSyncOutput::messages(sync_zulip_rest_messages(options, account_key)?);
    if !is_fake_mode(options) {
        let event_output = sync_zulip_event_queue_messages(options, account_key)?;
        out.messages.extend(event_output.messages);
        out.post_store_updates
            .extend(event_output.post_store_updates);
    }
    Ok(out)
}

fn sync_zulip_rest_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut url = Url::parse(&api_url(options, "/api/v1/messages")?)?;
    let previous_cursor = read_state_value(options, "zulip-latest-id")?;
    if let Some(cursor) = previous_cursor.as_deref().filter(|value| !value.is_empty()) {
        url.query_pairs_mut()
            .append_pair("anchor", cursor)
            .append_pair("num_before", "0")
            .append_pair("num_after", &options.limit.to_string())
            .append_pair("include_anchor", "false");
    } else {
        url.query_pairs_mut()
            .append_pair("anchor", "newest")
            .append_pair("num_before", &options.limit.to_string())
            .append_pair("num_after", "0");
    }
    if let Some(narrow) = zulip_message_narrow(options) {
        url.query_pairs_mut()
            .append_pair("narrow", &serde_json::to_string(&narrow)?);
    }
    let value = http_json("GET", url.as_str(), &zulip_headers(options), None)?;
    ensure_zulip_ok("get messages", &value)?;
    let mut out = Vec::new();
    let mut max_id = previous_cursor.clone();
    for message in value
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = message.get("id").map(value_to_string) {
            max_id = max_numeric_cursor(max_id, id);
        }
        out.push(normalize_zulip_message(options, account_key, message));
    }
    if max_id != previous_cursor {
        if let Some(max_id) = max_id.as_deref() {
            write_state_value(options, "zulip-latest-id", max_id)?;
        }
    }
    Ok(out)
}

fn sync_zulip_event_queue_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<ChatSyncOutput> {
    let queue = register_zulip_event_queue(options)?;
    write_state_value(options, "zulip-event-queue-id", &queue.queue_id)?;
    write_state_value(
        options,
        "zulip-event-last-id",
        &queue.last_event_id.to_string(),
    )?;

    let events_result = fetch_zulip_event_queue_messages(options, account_key, &queue);
    let delete_result = delete_zulip_event_queue(options, &queue.queue_id);
    match (events_result, delete_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Ok(output), Err(delete_error)) => {
            record_realtime_backoff(options, "zulip_queue_delete_failed", 6)?;
            bail!(
                "Zulip event queue read succeeded but queue delete failed: {delete_error}; stored {} message(s)",
                output.messages.len()
            )
        }
        (Err(events_error), Ok(())) => {
            record_realtime_backoff(options, "zulip_queue_read_failed", 6)?;
            Err(events_error)
        }
        (Err(events_error), Err(delete_error)) => {
            record_realtime_backoff(options, "zulip_queue_read_and_delete_failed", 6)?;
            bail!(
                "Zulip event queue read failed: {events_error}; additionally failed to delete queue: {delete_error}"
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ZulipEventQueue {
    queue_id: String,
    last_event_id: i64,
}

fn register_zulip_event_queue(options: &ChatOptions) -> Result<ZulipEventQueue> {
    let form = zulip_event_queue_register_form(options)?;
    let value = http_form(
        "POST",
        &api_url(options, "/api/v1/register")?,
        &zulip_headers(options),
        &form,
    )?;
    ensure_zulip_ok("register queue", &value)?;
    let queue_id = value_str(&value, "queue_id").unwrap_or_default();
    if queue_id.trim().is_empty() {
        bail!("Zulip register queue response did not include queue_id: {value}");
    }
    let last_event_id = value
        .get("last_event_id")
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .get("last_event_id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(-1);
    Ok(ZulipEventQueue {
        queue_id,
        last_event_id,
    })
}

fn fetch_zulip_event_queue_messages(
    options: &ChatOptions,
    account_key: &str,
    queue: &ZulipEventQueue,
) -> Result<ChatSyncOutput> {
    let mut url = Url::parse(&api_url(options, "/api/v1/events")?)?;
    url.query_pairs_mut()
        .append_pair("queue_id", &queue.queue_id)
        .append_pair("last_event_id", &queue.last_event_id.to_string())
        .append_pair("dont_block", "true");
    let value = http_json("GET", url.as_str(), &zulip_headers(options), None)?;
    ensure_zulip_ok("get events", &value)?;
    let mut max_event_id = queue.last_event_id;
    let mut out = ChatSyncOutput::default();
    for event in value
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(event_id) = event.get("id").and_then(Value::as_i64) {
            max_event_id = max_event_id.max(event_id);
        }
        match event.get("type").and_then(Value::as_str) {
            Some("message") => {
                let Some(message) = event.get("message") else {
                    continue;
                };
                out.messages
                    .push(normalize_zulip_message(options, account_key, message));
            }
            Some("update_message") => {
                out.post_store_updates
                    .push(ChatPostStoreUpdate::ZulipUpdateMessage {
                        event: event.clone(),
                    });
            }
            _ => {}
        }
    }
    write_state_value(options, "zulip-event-last-id", &max_event_id.to_string())?;
    Ok(out)
}

fn delete_zulip_event_queue(options: &ChatOptions, queue_id: &str) -> Result<()> {
    let mut form = BTreeMap::new();
    form.insert("queue_id".to_string(), queue_id.to_string());
    let value = http_form(
        "DELETE",
        &api_url(options, "/api/v1/events")?,
        &zulip_headers(options),
        &form,
    )?;
    ensure_zulip_ok("delete queue", &value)?;
    Ok(())
}

fn zulip_event_queue_register_form(options: &ChatOptions) -> Result<BTreeMap<String, String>> {
    let mut form = BTreeMap::new();
    form.insert(
        "event_types".to_string(),
        serde_json::to_string(&json!(["message", "update_message"]))?,
    );
    form.insert(
        "fetch_event_types".to_string(),
        serde_json::to_string(&json!(["message", "update_message"]))?,
    );
    form.insert("apply_markdown".to_string(), "false".to_string());
    if let Some(narrow) = zulip_event_queue_narrow(options) {
        form.insert("narrow".to_string(), serde_json::to_string(&narrow)?);
    }
    Ok(form)
}

fn zulip_message_narrow(options: &ChatOptions) -> Option<Value> {
    options.channel_ids.first().map(|stream| {
        if options.topic.trim().is_empty() {
            json!([{ "operator": "stream", "operand": stream }])
        } else {
            json!([
                { "operator": "stream", "operand": stream },
                { "operator": "topic", "operand": options.topic }
            ])
        }
    })
}

fn zulip_event_queue_narrow(options: &ChatOptions) -> Option<Value> {
    options.channel_ids.first().map(|stream| {
        if options.topic.trim().is_empty() {
            json!([["channel", stream]])
        } else {
            json!([["channel", stream], ["topic", options.topic]])
        }
    })
}

fn ensure_zulip_ok(operation: &str, value: &Value) -> Result<()> {
    if value.get("result").and_then(Value::as_str) == Some("error") {
        bail!("Zulip {operation} failed: {value}");
    }
    Ok(())
}

fn sync_google_chat_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let mut out = Vec::new();
    for space_name in required_destinations(options)? {
        let mut url = Url::parse(&api_url(
            options,
            &format!("/v1/{}/messages", trim_slashes(&space_name)),
        )?)?;
        url.query_pairs_mut()
            .append_pair("pageSize", &options.limit.min(100).to_string());
        let value = http_json("GET", url.as_str(), &bearer_headers(&options.token), None)?;
        for message in value
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push(normalize_google_chat_message(
                options,
                account_key,
                &space_name,
                message,
            ));
        }
    }
    Ok(out)
}

fn fake_test_value(options: &ChatOptions) -> Value {
    match options.platform {
        ChatPlatform::Slack => json!({
            "ok": true,
            "url": "https://fake.slack.test/",
            "team": "CTOX Fake Slack",
            "team_id": fake_workspace_id(options),
            "user": "ctox-fake-bot",
            "user_id": fake_account_id(options),
        }),
        ChatPlatform::Discord => json!({
            "id": fake_account_id(options),
            "username": "ctox-fake-bot",
            "bot": true,
        }),
        ChatPlatform::Telegram => json!({
            "ok": true,
            "result": {
                "id": 424242,
                "is_bot": true,
                "first_name": "CTOX Fake Bot",
                "username": fake_account_id(options),
            }
        }),
        ChatPlatform::Matrix => json!({
            "user_id": fake_account_id(options),
        }),
        ChatPlatform::Mattermost => json!({
            "id": fake_account_id(options),
            "username": "ctox-fake-bot",
        }),
        ChatPlatform::Zulip => json!({
            "email": fake_account_id(options),
            "full_name": "CTOX Fake Bot",
        }),
        ChatPlatform::GoogleChat => json!({
            "spaces": [{
                "name": fake_destination(options),
                "displayName": "CTOX Fake Space",
            }]
        }),
    }
}

fn test_slack_value(options: &ChatOptions) -> Result<Value> {
    let auth = http_json(
        "POST",
        &api_url(options, "/auth.test")?,
        &bearer_headers(&options.token),
        None,
    )?;
    ensure_slack_ok(&auth)?;
    let mut object = provider_value_object(auth);
    if !options.channel_ids.is_empty() {
        object.insert(
            "channelProbes".to_string(),
            Value::Array(
                options
                    .channel_ids
                    .iter()
                    .map(|channel_id| slack_channel_probe_value(options, channel_id))
                    .collect(),
            ),
        );
    }
    Ok(Value::Object(object))
}

fn slack_channel_probe_value(options: &ChatOptions, channel_id: &str) -> Value {
    let result = (|| -> Result<Value> {
        let mut url = Url::parse(&api_url(options, "/conversations.info")?)?;
        url.query_pairs_mut().append_pair("channel", channel_id);
        let value = http_json("GET", url.as_str(), &bearer_headers(&options.token), None)?;
        ensure_slack_ok(&value)?;
        Ok(value)
    })();
    match result {
        Ok(response) => json!({
            "ok": true,
            "channel_id": channel_id,
            "response": response,
        }),
        Err(error) => json!({
            "ok": false,
            "channel_id": channel_id,
            "error": redact_sensitive_text(options, &error.to_string()),
        }),
    }
}

fn test_discord_value(options: &ChatOptions) -> Result<Value> {
    let user = http_json(
        "GET",
        &api_url(options, "/users/@me")?,
        &discord_headers(&options.token),
        None,
    )?;
    let mut object = provider_value_object(user);
    object.insert(
        "gatewayProbe".to_string(),
        discord_get_probe_value(options, "/gateway/bot", "gateway"),
    );
    object.insert(
        "applicationProbe".to_string(),
        discord_get_probe_value(options, "/oauth2/applications/@me", "application"),
    );
    let guild_ids = split_list(&options.workspace_id);
    if !guild_ids.is_empty() {
        object.insert(
            "guildProbes".to_string(),
            Value::Array(
                guild_ids
                    .iter()
                    .map(|guild_id| {
                        discord_get_probe_value(
                            options,
                            &format!("/guilds/{}", urlencoding_encode(guild_id)),
                            guild_id,
                        )
                    })
                    .collect(),
            ),
        );
    }
    if !options.channel_ids.is_empty() {
        object.insert(
            "channelProbes".to_string(),
            Value::Array(
                options
                    .channel_ids
                    .iter()
                    .map(|channel_id| {
                        discord_get_probe_value(
                            options,
                            &format!("/channels/{}", urlencoding_encode(channel_id)),
                            channel_id,
                        )
                    })
                    .collect(),
            ),
        );
    }
    Ok(Value::Object(object))
}

fn discord_get_probe_value(options: &ChatOptions, path: &str, label: &str) -> Value {
    let url = match api_url(options, path) {
        Ok(url) => url,
        Err(error) => {
            return json!({
                "ok": false,
                "label": label,
                "error": redact_sensitive_text(options, &error.to_string()),
            });
        }
    };
    match http_json("GET", &url, &discord_headers(&options.token), None) {
        Ok(response) => json!({
            "ok": true,
            "label": label,
            "response": response,
        }),
        Err(error) => json!({
            "ok": false,
            "label": label,
            "error": redact_sensitive_text(options, &error.to_string()),
        }),
    }
}

fn slack_socket_mode_open_request(options: &ChatOptions) -> Result<SlackSocketModeOpenRequest> {
    if options.realtime_token.trim().is_empty() {
        bail!("Slack Socket Mode requires CTO_SLACK_APP_TOKEN");
    }
    Ok(SlackSocketModeOpenRequest {
        method: "POST",
        url: api_url(options, "/apps.connections.open")?,
        headers: bearer_headers(&options.realtime_token),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SlackSocketModeOpenRequest {
    method: &'static str,
    url: String,
    headers: BTreeMap<String, String>,
}

fn slack_socket_mode_ack_payload(envelope_id: &str) -> Result<Value> {
    if envelope_id.trim().is_empty() {
        bail!("Slack Socket Mode envelope_id is required for ack");
    }
    Ok(json!({ "envelope_id": envelope_id.trim() }))
}

fn mark_slack_socket_mode_envelope_seen(options: &ChatOptions, envelope_id: &str) -> Result<bool> {
    let envelope_id = envelope_id.trim();
    if envelope_id.is_empty() {
        bail!("Slack Socket Mode envelope_id is required");
    }
    let state_key = slack_socket_mode_envelope_state_key(envelope_id);
    if read_state_value(options, &state_key)?.is_some() {
        write_state_value(options, "slack-socket-mode-envelope-id", envelope_id)?;
        return Ok(false);
    }
    write_state_value(options, &state_key, &current_unix_millis().to_string())?;
    write_state_value(options, "slack-socket-mode-envelope-id", envelope_id)?;
    Ok(true)
}

fn slack_socket_mode_envelope_state_key(envelope_id: &str) -> String {
    format!(
        "slack-socket-mode-envelope-seen-{}",
        stable_digest(envelope_id)
    )
}

fn discord_gateway_identify_payload(options: &ChatOptions) -> Value {
    json!({
        "op": 2,
        "d": {
            "token": options.token,
            "intents": discord_gateway_intents(),
            "properties": {
                "os": std::env::consts::OS,
                "browser": "ctox",
                "device": "ctox",
            }
        }
    })
}

fn discord_gateway_resume_payload(options: &ChatOptions) -> Result<Value> {
    let session_id = read_state_value(options, "discord-gateway-session-id")?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Discord Gateway resume requires a stored session ID"))?;
    let sequence = read_state_value(options, "discord-gateway-sequence")?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Discord Gateway resume requires a stored sequence"))?;
    let sequence = sequence
        .parse::<i64>()
        .map_err(|_| anyhow::anyhow!("Discord Gateway sequence is not numeric"))?;
    Ok(json!({
        "op": 6,
        "d": {
            "token": options.token,
            "session_id": session_id,
            "seq": sequence,
        }
    }))
}

fn persist_discord_gateway_ready_state(
    options: &ChatOptions,
    session_id: &str,
    sequence: i64,
) -> Result<()> {
    if session_id.trim().is_empty() {
        bail!("Discord Gateway ready event did not include a session ID");
    }
    write_state_value(options, "discord-gateway-session-id", session_id.trim())?;
    persist_discord_gateway_sequence(options, sequence)
}

fn persist_discord_gateway_sequence(options: &ChatOptions, sequence: i64) -> Result<()> {
    if sequence < 0 {
        return Ok(());
    }
    let current = read_state_value(options, "discord-gateway-sequence")?;
    if let Some(max_sequence) = max_numeric_cursor(current, sequence.to_string()) {
        write_state_value(options, "discord-gateway-sequence", &max_sequence)?;
    }
    Ok(())
}

fn discord_gateway_intents() -> i64 {
    DISCORD_GATEWAY_INTENT_GUILDS
        | DISCORD_GATEWAY_INTENT_GUILD_MESSAGES
        | DISCORD_GATEWAY_INTENT_DIRECT_MESSAGES
        | DISCORD_GATEWAY_INTENT_MESSAGE_CONTENT
}

fn test_mattermost_value(options: &ChatOptions) -> Result<Value> {
    let user = http_json(
        "GET",
        &api_url(options, "/users/me")?,
        &bearer_headers(&options.token),
        None,
    )?;
    let mut object = provider_value_object(user);
    let probe = match http_json_response(
        "GET",
        &api_url(options, "/system/ping")?,
        &bearer_headers(&options.token),
        None,
    ) {
        Ok(response) => mattermost_server_probe_value(response),
        Err(error) => json!({
            "ok": false,
            "error": redact_sensitive_text(options, &error.to_string()),
        }),
    };
    object.insert("serverProbe".to_string(), probe);
    Ok(Value::Object(object))
}

fn test_zulip_value(options: &ChatOptions) -> Result<Value> {
    let user = http_json(
        "GET",
        &api_url(options, "/api/v1/users/me")?,
        &zulip_headers(options),
        None,
    )?;
    let mut object = provider_value_object(user);
    let settings = match http_json(
        "GET",
        &api_url(options, "/api/v1/server_settings")?,
        &BTreeMap::new(),
        None,
    ) {
        Ok(settings) => settings,
        Err(error) => json!({
            "ok": false,
            "error": redact_sensitive_text(options, &error.to_string()),
        }),
    };
    object.insert("serverSettings".to_string(), settings);
    Ok(Value::Object(object))
}

fn provider_value_object(value: Value) -> serde_json::Map<String, Value> {
    match value {
        Value::Object(object) => object,
        other => {
            let mut object = serde_json::Map::new();
            object.insert("response".to_string(), other);
            object
        }
    }
}

#[derive(Debug)]
struct ChatHttpJsonResponse {
    headers: BTreeMap<String, String>,
    value: Value,
}

fn mattermost_server_probe_value(response: ChatHttpJsonResponse) -> Value {
    let headers = selected_headers(
        &response.headers,
        &["x-mattermost-version", "x-version-id", "x-request-id"],
    );
    let version = value_str(&response.value, "version")
        .or_else(|| value_str(&headers, "x-mattermost-version"))
        .or_else(|| value_str(&headers, "x-version-id"));
    json!({
        "ok": true,
        "version": version,
        "headers": headers,
        "response": response.value,
    })
}

fn selected_headers(headers: &BTreeMap<String, String>, names: &[&str]) -> Value {
    let mut selected = serde_json::Map::new();
    for name in names {
        if let Some(value) = headers.get(*name) {
            selected.insert((*name).to_string(), Value::String(value.clone()));
        }
    }
    Value::Object(selected)
}

fn fake_sync_messages(
    options: &ChatOptions,
    account_key: &str,
) -> Result<Vec<NormalizedChatMessage>> {
    let destination = fake_destination(options);
    let remote_id = fake_remote_id(options, "inbound");
    let subject = format!("{} {}", options.platform.display_name(), destination);
    let body = format!(
        "Fake inbound {} message for {}",
        options.platform.display_name(),
        destination
    );
    Ok(vec![NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{destination}::{remote_id}"),
        account_key: account_key.to_string(),
        thread_key: thread_key_for_destination(
            options.platform,
            account_key,
            &destination,
            "fake-thread",
        ),
        remote_id,
        sender_display: "CTOX Fake Sender".to_string(),
        sender_address: format!("fake-sender@{}", options.platform.channel()),
        recipients: vec![destination.clone()],
        subject: subject.clone(),
        body_text: body.clone(),
        preview: preview_text(&body, &subject),
        seen: true,
        external_created_at: "2026-06-26T00:00:00Z".to_string(),
        metadata: json!({
            "adapter": options.platform.channel(),
            "fakeProvider": true,
            "destination": destination,
        }),
    }])
}

fn fake_send_result(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Value {
    let remote_id = fake_remote_id(options, &format!("{destination}:{}", request.body));
    match options.platform {
        ChatPlatform::Slack => json!({
            "ok": true,
            "channel": destination,
            "ts": remote_id,
            "message": {
                "ts": remote_id,
                "text": request.body,
            }
        }),
        ChatPlatform::Discord => json!({
            "id": remote_id,
            "channel_id": destination,
            "content": request.body,
        }),
        ChatPlatform::Telegram => json!({
            "ok": true,
            "result": {
                "message_id": remote_id,
                "chat": { "id": destination },
                "text": request.body,
            }
        }),
        ChatPlatform::Matrix => json!({
            "event_id": format!("${remote_id}:fake.matrix"),
        }),
        ChatPlatform::Mattermost => json!({
            "id": remote_id,
            "channel_id": destination,
            "message": request.body,
        }),
        ChatPlatform::Zulip => json!({
            "id": remote_id,
            "result": "success",
        }),
        ChatPlatform::GoogleChat => json!({
            "name": format!("{}/messages/{}", trim_slashes(destination), remote_id),
            "text": request.body,
        }),
    }
}

fn fake_destination(options: &ChatOptions) -> String {
    options
        .channel_ids
        .first()
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match options.platform {
            ChatPlatform::Telegram => "fake-chat".to_string(),
            ChatPlatform::Matrix => "!fake-room:matrix.test".to_string(),
            ChatPlatform::GoogleChat => "spaces/fake-space".to_string(),
            ChatPlatform::Zulip => "fake-stream".to_string(),
            _ => "fake-channel".to_string(),
        })
}

fn fake_workspace_id(options: &ChatOptions) -> String {
    options
        .workspace_id
        .trim()
        .to_string()
        .if_empty_else(|| format!("fake-{}", options.platform.channel()))
}

fn fake_account_id(options: &ChatOptions) -> String {
    options
        .account_id
        .trim()
        .to_string()
        .if_empty_else(|| format!("ctox-fake-{}", options.platform.channel()))
}

fn fake_remote_id(options: &ChatOptions, seed: &str) -> String {
    match options.platform {
        ChatPlatform::Slack => "1719360000.000001".to_string(),
        ChatPlatform::Telegram => "424242".to_string(),
        _ => format!(
            "fake-{}",
            stable_digest(&format!("{}:{seed}", options.platform.channel()))
        ),
    }
}

fn send_slack_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let mut payload = json!({
        "channel": destination,
        "text": request.body,
    });
    if let Some(thread_ts) = thread_component(&request.thread_key, "thread") {
        payload["thread_ts"] = Value::String(thread_ts);
    }
    let value = http_json(
        "POST",
        &api_url(options, "/chat.postMessage")?,
        &bearer_json_headers(&options.token),
        Some(&payload),
    )?;
    ensure_slack_ok(&value)?;
    Ok(value)
}

fn send_discord_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let payload = discord_message_payload(request);
    http_json(
        "POST",
        &api_url(
            options,
            &format!("/channels/{}/messages", urlencoding_encode(destination)),
        )?,
        &discord_json_headers(&options.token),
        Some(&payload),
    )
}

fn send_telegram_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let value = http_json(
        "POST",
        &telegram_url(options, "sendMessage")?,
        &json_headers(),
        Some(&telegram_message_payload(destination, request)),
    )?;
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        bail!("Telegram sendMessage failed: {value}");
    }
    Ok(value)
}

fn send_matrix_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let txn_id = stable_digest(&format!("{}:{}", destination, request.body));
    http_json(
        "PUT",
        &api_url(
            options,
            &format!(
                "/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
                urlencoding_encode(destination),
                txn_id
            ),
        )?,
        &bearer_json_headers(&options.token),
        Some(&json!({
            "msgtype": "m.text",
            "body": request.body,
        })),
    )
}

fn send_mattermost_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let payload = mattermost_post_payload(destination, request);
    http_json(
        "POST",
        &api_url(options, "/posts")?,
        &bearer_json_headers(&options.token),
        Some(&payload),
    )
}

fn send_zulip_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let topic = if request.subject.trim().is_empty() {
        if options.topic.trim().is_empty() {
            "CTOX".to_string()
        } else {
            options.topic.clone()
        }
    } else {
        request.subject.trim().to_string()
    };
    let mut form = BTreeMap::new();
    form.insert("type".to_string(), "stream".to_string());
    form.insert("to".to_string(), destination.to_string());
    form.insert("topic".to_string(), topic);
    form.insert("content".to_string(), request.body.to_string());
    http_form(
        "POST",
        &api_url(options, "/api/v1/messages")?,
        &zulip_headers(options),
        &form,
    )
}

fn send_google_chat_message(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
    destination: &str,
) -> Result<Value> {
    let payload = google_chat_message_payload(request);
    http_json(
        "POST",
        &api_url(
            options,
            &format!("/v1/{}/messages", trim_slashes(destination)),
        )?,
        &bearer_json_headers(&options.token),
        Some(&payload),
    )
}

fn discord_message_payload(request: &ChatSendCommandRequest<'_>) -> Value {
    let mut payload = json!({ "content": request.body });
    if let Some(message_id) = reply_thread_component(&request.thread_key) {
        payload["message_reference"] = json!({
            "message_id": message_id,
            "fail_if_not_exists": false,
        });
    }
    payload
}

fn telegram_message_payload(destination: &str, request: &ChatSendCommandRequest<'_>) -> Value {
    let mut payload = json!({
        "chat_id": destination,
        "text": request.body,
    });
    if let Some(message_id) =
        reply_thread_component(&request.thread_key).and_then(|value| value.parse::<i64>().ok())
    {
        payload["reply_to_message_id"] = Value::from(message_id);
        payload["allow_sending_without_reply"] = Value::Bool(true);
    }
    payload
}

fn mattermost_post_payload(destination: &str, request: &ChatSendCommandRequest<'_>) -> Value {
    let mut payload = json!({
        "channel_id": destination,
        "message": request.body,
    });
    if let Some(root_id) = reply_thread_component(&request.thread_key) {
        payload["root_id"] = Value::String(root_id);
    }
    payload
}

fn google_chat_message_payload(request: &ChatSendCommandRequest<'_>) -> Value {
    let mut payload = json!({ "text": request.body });
    if let Some(thread_name) = reply_thread_component(&request.thread_key) {
        payload["thread"] = json!({ "name": thread_name });
    }
    payload
}

fn normalize_slack_message(
    options: &ChatOptions,
    account_key: &str,
    channel_id: &str,
    message: &Value,
) -> NormalizedChatMessage {
    let ts = value_str(message, "ts").unwrap_or_else(|| stable_digest(&message.to_string()));
    let thread_ts = value_str(message, "thread_ts").unwrap_or_else(|| ts.clone());
    let user = value_str(message, "user")
        .or_else(|| value_str(message, "bot_id"))
        .unwrap_or_else(|| "slack".to_string());
    let body = value_str(message, "text").unwrap_or_default();
    let thread_key = format!(
        "{}:{}::channel::{}::thread::{}",
        options.platform.channel(),
        account_key_suffix(account_key),
        channel_id,
        thread_ts
    );
    let subject = format!("Slack {}", channel_id);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{channel_id}::{ts}"),
        account_key: account_key.to_string(),
        thread_key,
        remote_id: ts,
        sender_display: user.clone(),
        sender_address: user,
        recipients: vec![channel_id.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: slack_ts_to_iso(message.get("ts")),
        metadata: json!({
            "adapter": "slack",
            "channelId": channel_id,
            "raw": message,
        }),
    }
}

fn normalize_discord_message(
    options: &ChatOptions,
    account_key: &str,
    channel_id: &str,
    message: &Value,
) -> NormalizedChatMessage {
    let id = value_str(message, "id").unwrap_or_else(|| stable_digest(&message.to_string()));
    let author = message.get("author").unwrap_or(&Value::Null);
    let sender_id = value_str(author, "id").unwrap_or_else(|| "discord".to_string());
    let sender_name = value_str(author, "global_name")
        .or_else(|| value_str(author, "username"))
        .unwrap_or_else(|| sender_id.clone());
    let body = value_str(message, "content").unwrap_or_default();
    let subject = format!("Discord {}", channel_id);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{channel_id}::{id}"),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::channel::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            channel_id
        ),
        remote_id: id,
        sender_display: sender_name,
        sender_address: sender_id,
        recipients: vec![channel_id.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: value_str(message, "timestamp").unwrap_or_else(now_iso_string),
        metadata: json!({
            "adapter": "discord",
            "channelId": channel_id,
            "raw": message,
        }),
    }
}

fn normalize_telegram_message(
    options: &ChatOptions,
    account_key: &str,
    chat_id: &str,
    update: &Value,
    message: &Value,
) -> NormalizedChatMessage {
    let message_id = message
        .get("message_id")
        .map(value_to_string)
        .unwrap_or_else(|| stable_digest(&message.to_string()));
    let from = message
        .get("from")
        .or_else(|| message.get("sender_chat"))
        .unwrap_or(&Value::Null);
    let sender_id = from
        .get("id")
        .map(value_to_string)
        .unwrap_or_else(|| "telegram".to_string());
    let sender_name = telegram_display_name(from).unwrap_or_else(|| sender_id.clone());
    let body = value_str(message, "text")
        .or_else(|| value_str(message, "caption"))
        .unwrap_or_default();
    let subject = format!("Telegram {}", chat_id);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{chat_id}::{message_id}"),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::chat::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            chat_id
        ),
        remote_id: message_id,
        sender_display: sender_name,
        sender_address: sender_id,
        recipients: vec![chat_id.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: unix_seconds_to_iso(message.get("date").and_then(Value::as_i64)),
        metadata: json!({
            "adapter": "telegram",
            "chatId": chat_id,
            "updateId": update.get("update_id"),
            "raw": update,
        }),
    }
}

fn normalize_matrix_message(
    options: &ChatOptions,
    account_key: &str,
    room_id: &str,
    event: &Value,
) -> NormalizedChatMessage {
    let event_id =
        value_str(event, "event_id").unwrap_or_else(|| stable_digest(&event.to_string()));
    let sender = value_str(event, "sender").unwrap_or_else(|| "matrix".to_string());
    let body = event
        .get("content")
        .and_then(|content| value_str(content, "body"))
        .unwrap_or_default();
    let subject = format!("Matrix {}", room_id);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{room_id}::{event_id}"),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::room::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            room_id
        ),
        remote_id: event_id,
        sender_display: sender.clone(),
        sender_address: sender,
        recipients: vec![room_id.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: unix_millis_to_iso(
            event.get("origin_server_ts").and_then(Value::as_i64),
        ),
        metadata: json!({
            "adapter": "matrix",
            "roomId": room_id,
            "raw": event,
        }),
    }
}

fn normalize_mattermost_post(
    options: &ChatOptions,
    account_key: &str,
    channel_id: &str,
    post: &Value,
) -> NormalizedChatMessage {
    let id = value_str(post, "id").unwrap_or_else(|| stable_digest(&post.to_string()));
    let user_id = value_str(post, "user_id").unwrap_or_else(|| "mattermost".to_string());
    let root_id = value_str(post, "root_id")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| id.clone());
    let body = value_str(post, "message").unwrap_or_default();
    let subject = format!("Mattermost {}", channel_id);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{channel_id}::{id}"),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::channel::{}::thread::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            channel_id,
            root_id
        ),
        remote_id: id,
        sender_display: user_id.clone(),
        sender_address: user_id,
        recipients: vec![channel_id.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: unix_millis_to_iso(post.get("create_at").and_then(Value::as_i64)),
        metadata: json!({
            "adapter": "mattermost",
            "channelId": channel_id,
            "raw": post,
        }),
    }
}

fn normalize_zulip_message(
    options: &ChatOptions,
    account_key: &str,
    message: &Value,
) -> NormalizedChatMessage {
    let id = message
        .get("id")
        .map(value_to_string)
        .unwrap_or_else(|| stable_digest(&message.to_string()));
    let stream = value_str(message, "display_recipient")
        .or_else(|| value_str(message, "stream"))
        .unwrap_or_else(|| "direct".to_string());
    let topic = value_str(message, "subject").unwrap_or_else(|| "direct".to_string());
    let sender = value_str(message, "sender_full_name")
        .or_else(|| value_str(message, "sender_email"))
        .unwrap_or_else(|| "zulip".to_string());
    let sender_address = value_str(message, "sender_email").unwrap_or_else(|| sender.clone());
    let body = value_str(message, "content").unwrap_or_default();
    let subject = format!("Zulip {stream} / {topic}");
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{id}"),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::stream::{}::topic::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            stream,
            topic
        ),
        remote_id: id,
        sender_display: sender,
        sender_address,
        recipients: vec![stream],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: unix_seconds_to_iso(message.get("timestamp").and_then(Value::as_i64)),
        metadata: json!({
            "adapter": "zulip",
            "raw": message,
        }),
    }
}

fn normalize_google_chat_message(
    options: &ChatOptions,
    account_key: &str,
    space_name: &str,
    message: &Value,
) -> NormalizedChatMessage {
    let name = value_str(message, "name").unwrap_or_else(|| stable_digest(&message.to_string()));
    let sender = message.get("sender").unwrap_or(&Value::Null);
    let sender_name = value_str(sender, "displayName")
        .or_else(|| value_str(sender, "name"))
        .unwrap_or_else(|| "google_chat".to_string());
    let body = value_str(message, "text")
        .or_else(|| value_str(message, "argumentText"))
        .unwrap_or_default();
    let thread_name = message
        .get("thread")
        .and_then(|thread| value_str(thread, "name"))
        .unwrap_or_else(|| name.clone());
    let subject = format!("Google Chat {}", space_name);
    NormalizedChatMessage {
        message_key: format!("{account_key}::INBOX::{}", stable_digest(&name)),
        account_key: account_key.to_string(),
        thread_key: format!(
            "{}:{}::space::{}::thread::{}",
            options.platform.channel(),
            account_key_suffix(account_key),
            space_name,
            thread_name
        ),
        remote_id: name,
        sender_display: sender_name.clone(),
        sender_address: sender_name,
        recipients: vec![space_name.to_string()],
        subject: subject.clone(),
        preview: preview_text(&body, &subject),
        body_text: body,
        seen: true,
        external_created_at: value_str(message, "createTime").unwrap_or_else(now_iso_string),
        metadata: json!({
            "adapter": "google_chat",
            "spaceName": space_name,
            "raw": message,
        }),
    }
}

fn store_chat_message(
    conn: &mut rusqlite::Connection,
    platform: ChatPlatform,
    message: &NormalizedChatMessage,
) -> Result<()> {
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message.message_key,
            channel: platform.channel(),
            account_key: &message.account_key,
            thread_key: &message.thread_key,
            remote_id: &message.remote_id,
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: &message.sender_display,
            sender_address: &message.sender_address,
            recipient_addresses_json: &serde_json::to_string(&message.recipients)?,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &message.subject,
            preview: &message.preview,
            body_text: &message.body_text,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "medium",
            status: "received",
            seen: message.seen,
            has_attachments: false,
            external_created_at: &message.external_created_at,
            observed_at: &now_iso_string(),
            metadata_json: &serde_json::to_string(&message.metadata)?,
        },
    )?;
    refresh_thread(conn, &message.thread_key)?;
    Ok(())
}

fn apply_chat_post_store_update(
    conn: &mut rusqlite::Connection,
    options: &ChatOptions,
    account_key: &str,
    update: ChatPostStoreUpdate,
) -> Result<usize> {
    match update {
        ChatPostStoreUpdate::ZulipUpdateMessage { event } => {
            apply_zulip_update_message_event(conn, options, account_key, &event)
        }
    }
}

fn apply_zulip_update_message_event(
    conn: &mut rusqlite::Connection,
    options: &ChatOptions,
    account_key: &str,
    event: &Value,
) -> Result<usize> {
    let mut updated = 0usize;
    updated += apply_zulip_topic_move_event(conn, options, account_key, event)?;
    updated += apply_zulip_content_edit_event(conn, account_key, event)?;
    Ok(updated)
}

fn apply_zulip_content_edit_event(
    conn: &mut rusqlite::Connection,
    account_key: &str,
    event: &Value,
) -> Result<usize> {
    let Some(message_id) = event.get("message_id").map(value_to_string) else {
        return Ok(0);
    };
    let Some(content) = value_str(event, "content") else {
        return Ok(0);
    };
    let message_key = format!("{account_key}::INBOX::{message_id}");
    let Some(context) = read_message_update_context(conn, &message_key)? else {
        return Ok(0);
    };
    let preview = preview_text(&content, &context.subject);
    let metadata_json =
        metadata_json_with_event(&context.metadata_json, "zulipUpdateMessageEvent", event)?;
    let changed = conn.execute(
        r#"
        UPDATE communication_messages
        SET body_text = ?1,
            preview = ?2,
            observed_at = ?3,
            metadata_json = ?4
        WHERE message_key = ?5
        "#,
        rusqlite::params![
            content,
            preview,
            now_iso_string(),
            metadata_json,
            message_key
        ],
    )?;
    Ok(changed)
}

fn apply_zulip_topic_move_event(
    conn: &mut rusqlite::Connection,
    options: &ChatOptions,
    account_key: &str,
    event: &Value,
) -> Result<usize> {
    if event.get("subject").is_none() && event.get("new_stream_id").is_none() {
        return Ok(0);
    }
    let message_ids = zulip_update_message_ids(event);
    if message_ids.is_empty() {
        return Ok(0);
    }
    let stream = zulip_update_stream(options, event);
    let topic = value_str(event, "subject")
        .or_else(|| value_str(event, "orig_subject"))
        .unwrap_or_else(|| "direct".to_string());
    let subject = format!("Zulip {stream} / {topic}");
    let new_thread_key = format!(
        "{}:{}::stream::{}::topic::{}",
        options.platform.channel(),
        account_key_suffix(account_key),
        stream,
        topic
    );
    let recipients_json = serde_json::to_string(&vec![stream])?;
    let mut updated = 0usize;
    for message_id in message_ids {
        let message_key = format!("{account_key}::INBOX::{message_id}");
        let Some(context) = read_message_update_context(conn, &message_key)? else {
            continue;
        };
        let preview = preview_text(&context.body_text, &subject);
        let metadata_json =
            metadata_json_with_event(&context.metadata_json, "zulipTopicMoveEvent", event)?;
        let changed = conn.execute(
            r#"
            UPDATE communication_messages
            SET thread_key = ?1,
                recipient_addresses_json = ?2,
                subject = ?3,
                preview = ?4,
                observed_at = ?5,
                metadata_json = ?6
            WHERE message_key = ?7
            "#,
            rusqlite::params![
                new_thread_key,
                recipients_json,
                subject,
                preview,
                now_iso_string(),
                metadata_json,
                message_key
            ],
        )?;
        if changed > 0 {
            updated += changed;
            refresh_thread(conn, &context.thread_key)?;
            refresh_thread(conn, &new_thread_key)?;
        }
    }
    Ok(updated)
}

#[derive(Debug)]
struct MessageUpdateContext {
    thread_key: String,
    subject: String,
    body_text: String,
    metadata_json: String,
}

fn read_message_update_context(
    conn: &rusqlite::Connection,
    message_key: &str,
) -> Result<Option<MessageUpdateContext>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT thread_key, subject, body_text, metadata_json
        FROM communication_messages
        WHERE message_key = ?1
        "#,
    )?;
    let mut rows = stmt.query(rusqlite::params![message_key])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(MessageUpdateContext {
        thread_key: row.get(0)?,
        subject: row.get(1)?,
        body_text: row.get(2)?,
        metadata_json: row.get(3)?,
    }))
}

fn metadata_json_with_event(metadata_json: &str, key: &str, event: &Value) -> Result<String> {
    let mut metadata = serde_json::from_str::<Value>(metadata_json).unwrap_or_else(|_| json!({}));
    if !metadata.is_object() {
        metadata = json!({ "previousMetadata": metadata });
    }
    if let Some(map) = metadata.as_object_mut() {
        map.insert(key.to_string(), event.clone());
    }
    Ok(serde_json::to_string(&metadata)?)
}

fn zulip_update_message_ids(event: &Value) -> Vec<String> {
    if let Some(ids) = event.get("message_ids").and_then(Value::as_array) {
        return ids.iter().map(value_to_string).collect();
    }
    event
        .get("message_id")
        .map(value_to_string)
        .into_iter()
        .collect()
}

fn zulip_update_stream(options: &ChatOptions, event: &Value) -> String {
    if let Some(new_stream_id) = event.get("new_stream_id") {
        return format!("stream:{}", value_to_string(new_stream_id));
    }
    value_str(event, "stream_name")
        .or_else(|| options.channel_ids.first().cloned())
        .unwrap_or_else(|| "direct".to_string())
}

fn mark_account_activity(
    conn: &mut rusqlite::Connection,
    account_key: &str,
    inbound_at: Option<&str>,
    outbound_at: Option<&str>,
) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE communication_accounts
        SET
            last_inbound_ok_at = COALESCE(?2, last_inbound_ok_at),
            last_outbound_ok_at = COALESCE(?3, last_outbound_ok_at),
            updated_at = ?4
        WHERE account_key = ?1
        "#,
        rusqlite::params![account_key, inbound_at, outbound_at, now],
    )?;
    Ok(())
}

fn options_from_sync_args(
    platform: ChatPlatform,
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<ChatOptions> {
    let mut options = options_from_runtime(platform, root, runtime, request.db_path);
    for pair in request.passthrough_args.windows(2) {
        let flag = pair[0].as_str();
        let value = pair[1].as_str();
        if request.skip_flags.contains(&flag) {
            continue;
        }
        match flag {
            "--channel-id" | "--room-id" | "--space" | "--chat-id" | "--stream" => {
                options.channel_ids = split_list(value);
            }
            "--topic" => options.topic = value.to_string(),
            "--limit" => options.limit = value.parse().unwrap_or(DEFAULT_LIMIT),
            "--base-url" | "--server-url" | "--homeserver-url" | "--realm-url" => {
                options.base_url = trim_trailing_slash(value);
            }
            _ => {}
        }
    }
    Ok(options)
}

fn options_from_runtime(
    platform: ChatPlatform,
    root: &Path,
    runtime: &BTreeMap<String, String>,
    db_path: &Path,
) -> ChatOptions {
    let mut options = match platform {
        ChatPlatform::Slack => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting_or_default(
                runtime,
                &["CTO_SLACK_API_BASE_URL"],
                platform.default_base_url(),
            ),
            token: first_setting(runtime, &["CTO_SLACK_BOT_TOKEN"]),
            realtime_token: first_setting(runtime, &["CTO_SLACK_APP_TOKEN"]),
            username: String::new(),
            account_id: first_setting(runtime, &["CTO_SLACK_BOT_USER_ID"]),
            workspace_id: first_setting(runtime, &["CTO_SLACK_WORKSPACE_ID"]),
            channel_ids: first_list(runtime, &["CTO_SLACK_CHANNEL_IDS", "CTO_SLACK_CHANNEL_ID"]),
            topic: String::new(),
            limit: setting(runtime, "CTO_SLACK_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::Discord => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting_or_default(
                runtime,
                &["CTO_DISCORD_API_BASE_URL"],
                platform.default_base_url(),
            ),
            token: first_setting(runtime, &["CTO_DISCORD_BOT_TOKEN"]),
            realtime_token: first_setting(runtime, &["CTO_DISCORD_BOT_TOKEN"]),
            username: String::new(),
            account_id: first_setting(
                runtime,
                &["CTO_DISCORD_APPLICATION_ID", "CTO_DISCORD_BOT_USER_ID"],
            ),
            workspace_id: first_setting(
                runtime,
                &["CTO_DISCORD_GUILD_IDS", "CTO_DISCORD_GUILD_ID"],
            ),
            channel_ids: first_list(
                runtime,
                &["CTO_DISCORD_CHANNEL_IDS", "CTO_DISCORD_CHANNEL_ID"],
            ),
            topic: String::new(),
            limit: setting(runtime, "CTO_DISCORD_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::Telegram => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting_or_default(
                runtime,
                &["CTO_TELEGRAM_API_BASE_URL"],
                platform.default_base_url(),
            ),
            token: first_setting(runtime, &["CTO_TELEGRAM_BOT_TOKEN"]),
            realtime_token: first_setting(runtime, &["CTO_TELEGRAM_BOT_TOKEN"]),
            username: first_setting(runtime, &["CTO_TELEGRAM_BOT_USERNAME"]),
            account_id: first_setting(runtime, &["CTO_TELEGRAM_BOT_USERNAME"]),
            workspace_id: String::new(),
            channel_ids: first_list(runtime, &["CTO_TELEGRAM_CHAT_IDS", "CTO_TELEGRAM_CHAT_ID"]),
            topic: String::new(),
            limit: setting(runtime, "CTO_TELEGRAM_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::Matrix => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting_or_default(
                runtime,
                &["CTO_MATRIX_HOMESERVER_URL"],
                platform.default_base_url(),
            ),
            token: first_setting(runtime, &["CTO_MATRIX_ACCESS_TOKEN"]),
            realtime_token: first_setting(runtime, &["CTO_MATRIX_ACCESS_TOKEN"]),
            username: first_setting(runtime, &["CTO_MATRIX_USER_ID"]),
            account_id: first_setting(runtime, &["CTO_MATRIX_USER_ID"]),
            workspace_id: String::new(),
            channel_ids: first_list(runtime, &["CTO_MATRIX_ROOM_IDS", "CTO_MATRIX_ROOM_ID"]),
            topic: String::new(),
            limit: setting(runtime, "CTO_MATRIX_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::Mattermost => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: mattermost_base_url(first_setting(
                runtime,
                &["CTO_MATTERMOST_SERVER_URL", "CTO_MATTERMOST_API_BASE_URL"],
            )),
            token: first_setting(
                runtime,
                &["CTO_MATTERMOST_BOT_TOKEN", "CTO_MATTERMOST_ACCESS_TOKEN"],
            ),
            realtime_token: first_setting(
                runtime,
                &["CTO_MATTERMOST_BOT_TOKEN", "CTO_MATTERMOST_ACCESS_TOKEN"],
            ),
            username: first_setting(runtime, &["CTO_MATTERMOST_BOT_USERNAME"]),
            account_id: first_setting(runtime, &["CTO_MATTERMOST_BOT_USER_ID"]),
            workspace_id: first_setting(runtime, &["CTO_MATTERMOST_TEAM_ID"]),
            channel_ids: first_list(
                runtime,
                &["CTO_MATTERMOST_CHANNEL_IDS", "CTO_MATTERMOST_CHANNEL_ID"],
            ),
            topic: String::new(),
            limit: setting(runtime, "CTO_MATTERMOST_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::Zulip => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting(runtime, &["CTO_ZULIP_REALM_URL"]),
            token: first_setting(runtime, &["CTO_ZULIP_API_KEY"]),
            realtime_token: first_setting(runtime, &["CTO_ZULIP_API_KEY"]),
            username: first_setting(runtime, &["CTO_ZULIP_BOT_EMAIL", "CTO_ZULIP_EMAIL"]),
            account_id: first_setting(runtime, &["CTO_ZULIP_BOT_EMAIL", "CTO_ZULIP_EMAIL"]),
            workspace_id: String::new(),
            channel_ids: first_list(runtime, &["CTO_ZULIP_STREAMS", "CTO_ZULIP_STREAM"]),
            topic: first_setting(runtime, &["CTO_ZULIP_TOPIC"]),
            limit: setting(runtime, "CTO_ZULIP_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
        ChatPlatform::GoogleChat => ChatOptions {
            root: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            platform,
            base_url: first_setting_or_default(
                runtime,
                &["CTO_GOOGLE_CHAT_API_BASE_URL"],
                platform.default_base_url(),
            ),
            token: first_setting(runtime, &["CTO_GOOGLE_CHAT_ACCESS_TOKEN"]),
            realtime_token: first_setting(runtime, &["CTO_GOOGLE_CHAT_ACCESS_TOKEN"]),
            username: first_setting(runtime, &["CTO_GOOGLE_CHAT_USER"]),
            account_id: first_setting(runtime, &["CTO_GOOGLE_CHAT_USER", "CTO_GOOGLE_CHAT_APP_ID"]),
            workspace_id: String::new(),
            channel_ids: first_list(
                runtime,
                &["CTO_GOOGLE_CHAT_SPACE_NAMES", "CTO_GOOGLE_CHAT_SPACE_NAME"],
            ),
            topic: String::new(),
            limit: setting(runtime, "CTO_GOOGLE_CHAT_LIMIT")
                .parse()
                .unwrap_or(DEFAULT_LIMIT),
        },
    };
    options.base_url = trim_trailing_slash(&options.base_url);
    options
}

fn merge_profile_json(options: &mut ChatOptions, profile: &serde_json::Map<String, Value>) {
    for key in [
        "workspaceId",
        "guildId",
        "teamId",
        "serverUrl",
        "homeserverUrl",
        "realmUrl",
        "accountId",
        "userId",
        "botUserId",
        "botUsername",
        "botEmail",
    ] {
        let Some(value) = profile
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        match key {
            "workspaceId" | "guildId" | "teamId" => options.workspace_id = value.to_string(),
            "serverUrl" | "homeserverUrl" | "realmUrl" => {
                options.base_url = trim_trailing_slash(value)
            }
            "accountId" | "userId" | "botUserId" | "botUsername" | "botEmail" => {
                options.account_id = value.to_string()
            }
            _ => {}
        }
    }
    for key in [
        "channelIds",
        "channelId",
        "roomIds",
        "roomId",
        "chatIds",
        "chatId",
        "spaceNames",
        "spaceName",
        "streams",
        "stream",
    ] {
        let Some(value) = profile.get(key) else {
            continue;
        };
        let list = if let Some(array) = value.as_array() {
            array
                .iter()
                .filter_map(Value::as_str)
                .flat_map(split_list)
                .collect::<Vec<_>>()
        } else if let Some(raw) = value.as_str() {
            split_list(raw)
        } else {
            Vec::new()
        };
        if !list.is_empty() {
            options.channel_ids = list;
        }
    }
}

fn has_minimum_auth_config(options: &ChatOptions) -> bool {
    if is_fake_mode(options) {
        return true;
    }
    if options.token.trim().is_empty() {
        return false;
    }
    match options.platform {
        ChatPlatform::Matrix
        | ChatPlatform::Mattermost
        | ChatPlatform::Zulip
        | ChatPlatform::GoogleChat => !options.base_url.trim().is_empty(),
        _ => true,
    }
}

fn is_fake_mode(options: &ChatOptions) -> bool {
    options.token.trim() == FAKE_TOKEN || options.base_url.trim().starts_with(FAKE_BASE_URL_PREFIX)
}

fn has_minimum_sync_config(options: &ChatOptions) -> bool {
    if !has_minimum_auth_config(options) {
        return false;
    }
    match options.platform {
        ChatPlatform::Telegram | ChatPlatform::Matrix | ChatPlatform::Zulip => true,
        _ => !options.channel_ids.is_empty(),
    }
}

fn missing_config_message(platform: ChatPlatform) -> String {
    match platform {
        ChatPlatform::Slack => {
            "Slack requires CTO_SLACK_BOT_TOKEN and at least one CTO_SLACK_CHANNEL_ID for sync/send".to_string()
        }
        ChatPlatform::Discord => {
            "Discord requires CTO_DISCORD_BOT_TOKEN and at least one CTO_DISCORD_CHANNEL_ID for sync/send".to_string()
        }
        ChatPlatform::Telegram => "Telegram requires CTO_TELEGRAM_BOT_TOKEN".to_string(),
        ChatPlatform::Matrix => {
            "Matrix requires CTO_MATRIX_HOMESERVER_URL and CTO_MATRIX_ACCESS_TOKEN".to_string()
        }
        ChatPlatform::Mattermost => {
            "Mattermost requires CTO_MATTERMOST_SERVER_URL, CTO_MATTERMOST_BOT_TOKEN, and at least one CTO_MATTERMOST_CHANNEL_ID".to_string()
        }
        ChatPlatform::Zulip => {
            "Zulip requires CTO_ZULIP_REALM_URL, CTO_ZULIP_BOT_EMAIL, and CTO_ZULIP_API_KEY".to_string()
        }
        ChatPlatform::GoogleChat => {
            "Google Chat requires CTO_GOOGLE_CHAT_ACCESS_TOKEN and at least one CTO_GOOGLE_CHAT_SPACE_NAME".to_string()
        }
    }
}

fn configured_account_key(options: &ChatOptions) -> String {
    let suffix = if !options.account_id.trim().is_empty() {
        options.account_id.trim().to_string()
    } else if !options.username.trim().is_empty() {
        options.username.trim().to_string()
    } else if !options.workspace_id.trim().is_empty() {
        options.workspace_id.trim().to_string()
    } else if !options.base_url.trim().is_empty() {
        stable_digest(&options.base_url)
    } else {
        stable_digest(options.platform.channel())
    };
    format!("{}:{}", options.platform.channel(), suffix)
}

fn configured_account_address(options: &ChatOptions) -> String {
    if !options.username.trim().is_empty() {
        return options.username.trim().to_string();
    }
    if !options.account_id.trim().is_empty() {
        return options.account_id.trim().to_string();
    }
    if !options.workspace_id.trim().is_empty() {
        return options.workspace_id.trim().to_string();
    }
    options.platform.channel().to_string()
}

fn account_key_from_test_value(options: &ChatOptions, value: &Value) -> String {
    let id = match options.platform {
        ChatPlatform::Slack => value_str(value, "user_id")
            .or_else(|| value_str(value, "team_id"))
            .or_else(|| (!options.workspace_id.is_empty()).then(|| options.workspace_id.clone())),
        ChatPlatform::Discord => value_str(value, "id"),
        ChatPlatform::Telegram => value
            .get("result")
            .and_then(|result| result.get("username"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                value
                    .get("result")
                    .and_then(|result| result.get("id"))
                    .map(value_to_string)
            }),
        ChatPlatform::Matrix => value_str(value, "user_id"),
        ChatPlatform::Mattermost => value_str(value, "id").or_else(|| value_str(value, "username")),
        ChatPlatform::Zulip => value_str(value, "email").or_else(|| Some(options.username.clone())),
        ChatPlatform::GoogleChat => Some(
            options
                .account_id
                .trim()
                .to_string()
                .if_empty_else(|| stable_digest(&options.base_url)),
        ),
    }
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| stable_digest(&value.to_string()));
    format!("{}:{}", options.platform.channel(), id)
}

fn account_address_from_test_value(options: &ChatOptions, value: &Value) -> String {
    match options.platform {
        ChatPlatform::Slack => value_str(value, "user")
            .or_else(|| value_str(value, "user_id"))
            .unwrap_or_else(|| configured_account_address(options)),
        ChatPlatform::Discord => value_str(value, "username")
            .or_else(|| value_str(value, "id"))
            .unwrap_or_else(|| configured_account_address(options)),
        ChatPlatform::Telegram => value
            .get("result")
            .and_then(|result| result.get("username"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| configured_account_address(options)),
        ChatPlatform::Matrix => {
            value_str(value, "user_id").unwrap_or_else(|| configured_account_address(options))
        }
        ChatPlatform::Mattermost => value_str(value, "username")
            .or_else(|| value_str(value, "id"))
            .unwrap_or_else(|| configured_account_address(options)),
        ChatPlatform::Zulip => {
            value_str(value, "email").unwrap_or_else(|| configured_account_address(options))
        }
        ChatPlatform::GoogleChat => configured_account_address(options),
    }
}

fn profile_json(options: &ChatOptions, provider_value: &Value, adapter_status: Value) -> Value {
    let adapter_status = enrich_adapter_status(options, adapter_status, provider_value);
    json!({
        "platform": options.platform.channel(),
        "provider": options.platform.provider(),
        "baseUrl": options.base_url,
        "workspaceId": options.workspace_id,
        "accountId": options.account_id,
        "username": options.username,
        "channelIds": options.channel_ids,
        "topic": options.topic,
        "adapterStatus": adapter_status,
        "providerProbe": provider_value,
    })
}

fn adapter_status(
    operation: &str,
    ok: bool,
    error: Option<String>,
    cursor: Option<String>,
    options: &ChatOptions,
) -> Value {
    let error = error.map(|value| redact_sensitive_text(options, &value));
    let classified = classify_provider_error(options.platform, error.as_deref());
    let rate_limited_until_ms = error
        .as_deref()
        .and_then(rate_limited_until_ms_from_error)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let realtime_cursor_key = realtime_cursor_state_key(options.platform);
    json!({
        "auth_state": if ok { "ok" } else { classified.auth_state },
        "scope_state": if ok { "ok" } else { classified.scope_state },
        "permission_state": if ok { "ok" } else { classified.permission_state },
        "sync_state": match operation {
            "sync" if ok => "ok",
            "sync" => "failed",
            _ => "not_running",
        },
        "last_cursor": cursor,
        "rate_limited_until_ms": rate_limited_until_ms,
        "last_error": error,
        "last_success_at_ms": ok.then(current_unix_millis),
        "last_operation": operation,
        "fake_provider": is_fake_mode(options),
        "provider_error_kind": classified.kind,
        "provider_remediation": classified.remediation,
        "realtime_transport": realtime_transport(options.platform),
        "realtime_config_state": realtime_config_state(options),
        "realtime_supervision_state": realtime_supervision_state(options),
        "realtime_cursor_state_key": realtime_cursor_key,
        "realtime_last_cursor": realtime_cursor_key
            .and_then(|key| read_state_value(options, key).ok().flatten()),
        "realtime_backoff_until_ms": realtime_backoff_until_ms(options),
        "realtime_backoff_attempt": realtime_backoff_attempt(options),
        "realtime_backoff_reason": realtime_backoff_reason(options),
    })
}

fn realtime_transport(platform: ChatPlatform) -> &'static str {
    match platform {
        ChatPlatform::Slack => "socket_mode",
        ChatPlatform::Discord => "gateway",
        ChatPlatform::Telegram => "long_poll",
        ChatPlatform::Matrix => "client_sync",
        ChatPlatform::Mattermost => "websocket",
        ChatPlatform::Zulip => "events_api",
        ChatPlatform::GoogleChat => "workspace_events",
    }
}

fn realtime_cursor_state_key(platform: ChatPlatform) -> Option<&'static str> {
    Some(match platform {
        ChatPlatform::Slack => "slack-socket-mode-envelope-id",
        ChatPlatform::Discord => "discord-gateway-sequence",
        ChatPlatform::Telegram => "update-offset",
        ChatPlatform::Matrix => "sync-token",
        ChatPlatform::Mattermost => "mattermost-websocket-sequence",
        ChatPlatform::Zulip => "zulip-event-last-id",
        ChatPlatform::GoogleChat => "google-chat-events-page-token",
    })
}

fn realtime_config_state(options: &ChatOptions) -> &'static str {
    if is_fake_mode(options) {
        return "fake";
    }
    match options.platform {
        ChatPlatform::Slack if options.realtime_token.trim().is_empty() => "missing_app_token",
        ChatPlatform::Discord
        | ChatPlatform::Telegram
        | ChatPlatform::Matrix
        | ChatPlatform::Mattermost
        | ChatPlatform::Zulip
        | ChatPlatform::GoogleChat
            if options.realtime_token.trim().is_empty() =>
        {
            "missing_token"
        }
        ChatPlatform::Mattermost | ChatPlatform::Zulip | ChatPlatform::Matrix
            if options.base_url.trim().is_empty() =>
        {
            "missing_server_url"
        }
        ChatPlatform::Slack
        | ChatPlatform::Discord
        | ChatPlatform::Telegram
        | ChatPlatform::Matrix
        | ChatPlatform::Mattermost
        | ChatPlatform::Zulip
        | ChatPlatform::GoogleChat => "configured",
    }
}

fn realtime_supervision_state(options: &ChatOptions) -> String {
    match realtime_config_state(options) {
        "fake" => "fake".to_string(),
        "configured" => match options.platform {
            ChatPlatform::Slack => slack_socket_mode_supervisor_state(options)
                .unwrap_or_else(|| "supervised_via_service_sync".to_string()),
            ChatPlatform::Telegram | ChatPlatform::Matrix | ChatPlatform::Zulip => {
                "polling_via_service_sync".to_string()
            }
            _ => "not_implemented".to_string(),
        },
        _ => "not_configured".to_string(),
    }
}

fn realtime_backoff_until_ms(options: &ChatOptions) -> Value {
    realtime_backoff_until_timestamp_ms(options)
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn realtime_backoff_until_timestamp_ms(options: &ChatOptions) -> Option<i64> {
    read_state_value(options, "realtime-backoff-until-ms")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
}

fn realtime_backoff_attempt(options: &ChatOptions) -> Value {
    read_state_value(options, "realtime-backoff-attempt")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn realtime_backoff_reason(options: &ChatOptions) -> Value {
    read_state_value(options, "realtime-backoff-reason")
        .ok()
        .flatten()
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn record_realtime_backoff(options: &ChatOptions, reason: &str, attempt: u32) -> Result<i64> {
    let delay_ms = realtime_backoff_delay_ms(attempt);
    let until_ms = current_unix_millis() + delay_ms;
    write_state_value(options, "realtime-backoff-until-ms", &until_ms.to_string())?;
    write_state_value(options, "realtime-backoff-attempt", &attempt.to_string())?;
    write_state_value(options, "realtime-backoff-reason", reason.trim())?;
    Ok(until_ms)
}

fn clear_realtime_backoff(options: &ChatOptions) -> Result<()> {
    write_state_value(options, "realtime-backoff-until-ms", "")?;
    write_state_value(options, "realtime-backoff-attempt", "")?;
    write_state_value(options, "realtime-backoff-reason", "")?;
    Ok(())
}

fn realtime_backoff_delay_ms(attempt: u32) -> i64 {
    let shift = attempt.min(6);
    (1_000_i64.saturating_mul(1_i64 << shift)).min(60_000)
}

fn next_realtime_backoff_attempt(options: &ChatOptions) -> u32 {
    read_state_value(options, "realtime-backoff-attempt")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .saturating_add(1)
}

fn slack_socket_mode_supervisor_state(options: &ChatOptions) -> Option<String> {
    read_state_value(options, "slack-socket-mode-supervisor-state")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
}

fn mark_slack_socket_mode_supervisor_state(
    options: &ChatOptions,
    state: &str,
    error: Option<&str>,
) -> Result<()> {
    write_state_value(options, "slack-socket-mode-supervisor-state", state)?;
    match state {
        "starting" => {
            write_state_value(
                options,
                "slack-socket-mode-last-started-at-ms",
                &current_unix_millis().to_string(),
            )?;
            write_state_value(options, "slack-socket-mode-last-error", "")?;
        }
        "running" => {
            write_state_value(
                options,
                "slack-socket-mode-last-running-at-ms",
                &current_unix_millis().to_string(),
            )?;
        }
        "stopped" => {
            write_state_value(
                options,
                "slack-socket-mode-last-stopped-at-ms",
                &current_unix_millis().to_string(),
            )?;
            write_state_value(options, "slack-socket-mode-last-error", "")?;
        }
        "failed" => {
            write_state_value(
                options,
                "slack-socket-mode-last-failed-at-ms",
                &current_unix_millis().to_string(),
            )?;
            if let Some(error) = error {
                write_state_value(options, "slack-socket-mode-last-error", error)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn enrich_adapter_status(options: &ChatOptions, status: Value, provider_value: &Value) -> Value {
    let mut object = match status {
        Value::Object(object) => object,
        other => return other,
    };

    enrich_provider_probe_status(options, &mut object, provider_value);
    enrich_slack_status(options, &mut object);
    enrich_discord_status(options, &mut object);
    enrich_telegram_status(options, &mut object, provider_value);
    enrich_matrix_status(options, &mut object);

    if !matches!(
        options.platform,
        ChatPlatform::Mattermost | ChatPlatform::Zulip
    ) {
        return Value::Object(object);
    }

    let diagnostics = server_url_diagnostics(options);
    object.insert(
        "server_url_state".to_string(),
        Value::String(diagnostics.url_state.to_string()),
    );
    object.insert(
        "tls_state".to_string(),
        Value::String(diagnostics.tls_state.to_string()),
    );
    if let Some(host) = diagnostics.host {
        object.insert("server_host".to_string(), Value::String(host));
    }
    if let Some(version) = server_version_from_probe(options.platform, provider_value) {
        object.insert("server_version".to_string(), Value::String(version));
    }
    object.insert(
        "server_probe_state".to_string(),
        Value::String(server_probe_state(options.platform, provider_value).to_string()),
    );
    Value::Object(object)
}

fn enrich_slack_status(options: &ChatOptions, status: &mut serde_json::Map<String, Value>) {
    if options.platform != ChatPlatform::Slack {
        return;
    }
    let last_envelope = read_state_value(options, "slack-socket-mode-envelope-id")
        .ok()
        .flatten();
    let socket_state = if options.realtime_token.trim().is_empty() {
        "missing_app_token"
    } else if last_envelope
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        "envelope_seen"
    } else {
        "ready_to_connect"
    };
    status.insert(
        "slack_socket_mode_state".to_string(),
        Value::String(socket_state.to_string()),
    );
    if let Some(supervisor_state) = slack_socket_mode_supervisor_state(options) {
        status.insert(
            "slack_socket_mode_supervisor_state".to_string(),
            Value::String(supervisor_state),
        );
    }
    if let Some(last_envelope) = last_envelope {
        status.insert(
            "slack_socket_mode_last_envelope".to_string(),
            Value::String(last_envelope),
        );
    }
    for (state_key, status_key) in [
        (
            "slack-socket-mode-last-started-at-ms",
            "slack_socket_mode_last_started_at_ms",
        ),
        (
            "slack-socket-mode-last-running-at-ms",
            "slack_socket_mode_last_running_at_ms",
        ),
        (
            "slack-socket-mode-last-stopped-at-ms",
            "slack_socket_mode_last_stopped_at_ms",
        ),
        (
            "slack-socket-mode-last-failed-at-ms",
            "slack_socket_mode_last_failed_at_ms",
        ),
    ] {
        if let Some(value) = read_state_value(options, state_key)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<i64>().ok())
        {
            status.insert(status_key.to_string(), Value::from(value));
        }
    }
}

fn enrich_discord_status(options: &ChatOptions, status: &mut serde_json::Map<String, Value>) {
    if options.platform != ChatPlatform::Discord {
        return;
    }
    let session_id = read_state_value(options, "discord-gateway-session-id")
        .ok()
        .flatten();
    let sequence = read_state_value(options, "discord-gateway-sequence")
        .ok()
        .flatten();
    status.insert(
        "discord_gateway_resume_state".to_string(),
        match (session_id.as_deref(), sequence.as_deref()) {
            (Some(session_id), Some(sequence))
                if !session_id.trim().is_empty() && !sequence.trim().is_empty() =>
            {
                Value::String("resume_ready".to_string())
            }
            _ => Value::String("no_session".to_string()),
        },
    );
    if let Some(sequence) = sequence {
        status.insert(
            "discord_gateway_sequence".to_string(),
            Value::String(sequence),
        );
    }
}

fn enrich_provider_probe_status(
    options: &ChatOptions,
    status: &mut serde_json::Map<String, Value>,
    provider_value: &Value,
) {
    if !matches!(
        options.platform,
        ChatPlatform::Slack | ChatPlatform::Discord
    ) {
        return;
    }

    let mut has_probe = false;
    if let Some(state) = probe_array_state(provider_value.get("channelProbes")) {
        has_probe = true;
        status.insert(
            "channel_probe_state".to_string(),
            Value::String(state.to_string()),
        );
    }
    if let Some(state) = probe_array_state(provider_value.get("guildProbes")) {
        has_probe = true;
        status.insert(
            "guild_probe_state".to_string(),
            Value::String(state.to_string()),
        );
    }
    if let Some(state) = probe_object_state(provider_value.get("gatewayProbe")) {
        has_probe = true;
        status.insert(
            "gateway_probe_state".to_string(),
            Value::String(state.to_string()),
        );
    }
    if let Some(state) = probe_object_state(provider_value.get("applicationProbe")) {
        has_probe = true;
        status.insert(
            "application_probe_state".to_string(),
            Value::String(state.to_string()),
        );
    }

    let Some(error) = provider_probe_error_summary(provider_value) else {
        if has_probe {
            status.insert("probe_state".to_string(), Value::String("ok".to_string()));
        }
        return;
    };
    let error = redact_sensitive_text(options, &error);
    let classified = classify_provider_error(options.platform, Some(&error));
    status.insert(
        "probe_state".to_string(),
        Value::String("failed".to_string()),
    );
    status.insert("last_error".to_string(), Value::String(error));
    status.insert(
        "provider_error_kind".to_string(),
        Value::String(classified.kind.to_string()),
    );
    status.insert(
        "provider_remediation".to_string(),
        Value::String(classified.remediation.to_string()),
    );
    if classified.auth_state != "unknown" && classified.auth_state != "failed" {
        status.insert(
            "auth_state".to_string(),
            Value::String(classified.auth_state.to_string()),
        );
    }
    if classified.scope_state != "unknown" {
        status.insert(
            "scope_state".to_string(),
            Value::String(classified.scope_state.to_string()),
        );
    }
    if classified.permission_state != "unknown" {
        status.insert(
            "permission_state".to_string(),
            Value::String(classified.permission_state.to_string()),
        );
    }
}

fn enrich_telegram_status(
    options: &ChatOptions,
    status: &mut serde_json::Map<String, Value>,
    provider_value: &Value,
) {
    if !matches!(options.platform, ChatPlatform::Telegram) {
        return;
    }

    if let Some(can_read_all) = telegram_can_read_all_group_messages(provider_value) {
        let _ = write_state_value(
            options,
            "telegram-can-read-all-group-messages",
            if can_read_all { "true" } else { "false" },
        );
    }

    let can_read_all = read_state_value(options, "telegram-can-read-all-group-messages")
        .ok()
        .flatten()
        .and_then(|value| match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        });
    let state = match (can_read_all, has_telegram_group_destinations(options)) {
        (Some(true), _) => "all_group_messages_visible",
        (Some(false), true) => "privacy_mode_limited",
        (Some(false), false) => "privacy_mode_enabled",
        (None, true) => "privacy_mode_unknown_for_groups",
        (None, false) => "unknown",
    };
    status.insert(
        "telegram_group_privacy_state".to_string(),
        Value::String(state.to_string()),
    );
}

fn telegram_can_read_all_group_messages(provider_value: &Value) -> Option<bool> {
    provider_value
        .pointer("/result/can_read_all_group_messages")
        .or_else(|| provider_value.get("can_read_all_group_messages"))
        .and_then(Value::as_bool)
}

fn has_telegram_group_destinations(options: &ChatOptions) -> bool {
    options
        .channel_ids
        .iter()
        .any(|id| id.trim().starts_with('-'))
}

fn enrich_matrix_status(options: &ChatOptions, status: &mut serde_json::Map<String, Value>) {
    if !matches!(options.platform, ChatPlatform::Matrix) {
        return;
    }
    let encrypted_seen = read_state_value(options, "matrix-encrypted-events-seen")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(0);
    let e2ee_state = if encrypted_seen > 0 {
        "encrypted_events_not_supported".to_string()
    } else {
        read_state_value(options, "matrix-e2ee-state")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "plaintext_only".to_string())
    };
    let sdk_state_persistence = read_state_value(options, "matrix-sdk-state-persistence")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if e2ee_state == "encrypted_events_not_supported" {
                "required_for_e2ee_not_configured".to_string()
            } else {
                "not_required_plaintext_v1".to_string()
            }
        });
    let e2ee_policy = read_state_value(options, "matrix-e2ee-policy")
        .ok()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if e2ee_state == "encrypted_events_not_supported" {
                "disabled_until_sdk_state_store".to_string()
            } else {
                "plaintext_only_v1".to_string()
            }
        });
    status.insert("matrix_e2ee_state".to_string(), Value::String(e2ee_state));
    status.insert(
        "matrix_sdk_state_persistence".to_string(),
        Value::String(sdk_state_persistence),
    );
    status.insert("matrix_e2ee_policy".to_string(), Value::String(e2ee_policy));
    if encrypted_seen > 0 {
        status.insert(
            "matrix_encrypted_events_seen".to_string(),
            Value::from(encrypted_seen),
        );
    }
}

fn probe_array_state(value: Option<&Value>) -> Option<&'static str> {
    let values = value.and_then(Value::as_array)?;
    if values.is_empty() {
        return Some("unknown");
    }
    if values.iter().any(probe_failed) {
        Some("failed")
    } else {
        Some("ok")
    }
}

fn probe_object_state(value: Option<&Value>) -> Option<&'static str> {
    value.map(|value| if probe_failed(value) { "failed" } else { "ok" })
}

fn probe_failed(value: &Value) -> bool {
    value.get("error").is_some() || value.get("ok").and_then(Value::as_bool) == Some(false)
}

fn provider_probe_error_summary(provider_value: &Value) -> Option<String> {
    let mut errors = Vec::new();
    push_probe_error(
        &mut errors,
        "gateway",
        provider_value.get("gatewayProbe"),
        None,
    );
    push_probe_error(
        &mut errors,
        "application",
        provider_value.get("applicationProbe"),
        None,
    );
    push_probe_array_errors(
        &mut errors,
        "guild",
        provider_value.get("guildProbes"),
        "label",
    );
    push_probe_array_errors(
        &mut errors,
        "channel",
        provider_value.get("channelProbes"),
        "channel_id",
    );
    if errors.is_empty() {
        None
    } else {
        Some(errors.into_iter().take(5).collect::<Vec<_>>().join("; "))
    }
}

fn push_probe_array_errors(
    errors: &mut Vec<String>,
    label: &str,
    value: Option<&Value>,
    id_field: &str,
) {
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let id = item
            .get(id_field)
            .or_else(|| item.get("label"))
            .and_then(Value::as_str);
        push_probe_error(errors, label, Some(item), id);
    }
}

fn push_probe_error(
    errors: &mut Vec<String>,
    label: &str,
    value: Option<&Value>,
    id: Option<&str>,
) {
    let Some(value) = value.filter(|value| probe_failed(value)) else {
        return;
    };
    let error = value
        .get("error")
        .map(value_to_string)
        .unwrap_or_else(|| "probe failed".to_string());
    let prefix = id
        .map(|id| format!("{label} {id}"))
        .unwrap_or_else(|| label.to_string());
    errors.push(format!("{prefix}: {error}"));
}

#[derive(Debug)]
struct ServerUrlDiagnostics {
    url_state: &'static str,
    tls_state: &'static str,
    host: Option<String>,
}

fn server_url_diagnostics(options: &ChatOptions) -> ServerUrlDiagnostics {
    let raw = options.base_url.trim();
    if raw.is_empty() {
        return ServerUrlDiagnostics {
            url_state: "missing",
            tls_state: "unknown",
            host: None,
        };
    }
    if is_fake_mode(options) {
        return ServerUrlDiagnostics {
            url_state: "fake",
            tls_state: "not_applicable",
            host: None,
        };
    }
    match Url::parse(raw) {
        Ok(url) => match url.scheme() {
            "https" => ServerUrlDiagnostics {
                url_state: "ok",
                tls_state: "https",
                host: url.host_str().map(str::to_string),
            },
            "http" => ServerUrlDiagnostics {
                url_state: "ok",
                tls_state: "plain_http",
                host: url.host_str().map(str::to_string),
            },
            _ => ServerUrlDiagnostics {
                url_state: "unsupported_scheme",
                tls_state: "unsupported_scheme",
                host: url.host_str().map(str::to_string),
            },
        },
        Err(_) => ServerUrlDiagnostics {
            url_state: "invalid",
            tls_state: "unknown",
            host: None,
        },
    }
}

fn server_probe_state(platform: ChatPlatform, provider_value: &Value) -> &'static str {
    let probe = match platform {
        ChatPlatform::Mattermost => provider_value.get("serverProbe"),
        ChatPlatform::Zulip => provider_value.get("serverSettings"),
        _ => None,
    };
    let Some(probe) = probe else {
        return "unknown";
    };
    if probe.get("error").is_some() || probe.get("ok").and_then(Value::as_bool) == Some(false) {
        return "failed";
    }
    "ok"
}

fn server_version_from_probe(platform: ChatPlatform, provider_value: &Value) -> Option<String> {
    let paths = match platform {
        ChatPlatform::Mattermost => &[
            "/serverProbe/version",
            "/serverProbe/headers/x-mattermost-version",
            "/serverProbe/headers/x-version-id",
            "/serverProbe/response/version",
            "/serverProbe/response/server_version",
        ][..],
        ChatPlatform::Zulip => &[
            "/serverSettings/zulip_version",
            "/serverSettings/version",
            "/serverSettings/server_version",
        ][..],
        _ => &[][..],
    };
    paths.iter().find_map(|path| {
        provider_value
            .pointer(path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderErrorClassification {
    kind: &'static str,
    auth_state: &'static str,
    scope_state: &'static str,
    permission_state: &'static str,
    remediation: &'static str,
}

fn classify_provider_error(
    platform: ChatPlatform,
    error: Option<&str>,
) -> ProviderErrorClassification {
    let Some(error) = error else {
        return ProviderErrorClassification {
            kind: "none",
            auth_state: "unknown",
            scope_state: "unknown",
            permission_state: "unknown",
            remediation: "No provider error observed.",
        };
    };
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("http 429")
        || normalized.contains("rate_limited")
        || normalized.contains("rate limit")
        || normalized.contains("m_limit_exceeded")
        || normalized.contains("too many requests")
    {
        return ProviderErrorClassification {
            kind: "rate_limited",
            auth_state: "ok",
            scope_state: "unknown",
            permission_state: "unknown",
            remediation:
                "Respect the provider Retry-After value and let the next scheduled sync retry.",
        };
    }
    if normalized.contains("http 401")
        || normalized.contains("unauthorized")
        || normalized.contains("invalid_auth")
        || normalized.contains("not_authed")
        || normalized.contains("invalid token")
        || normalized.contains("unknown_token")
        || normalized.contains("m_unknown_token")
        || normalized.contains("token_revoked")
        || normalized.contains("access token")
    {
        return ProviderErrorClassification {
            kind: "deauthorized",
            auth_state: "deauthorized",
            scope_state: "unknown",
            permission_state: "unknown",
            remediation: "Reconnect the account or rotate the bot token in CTOX secrets.",
        };
    }
    if normalized.contains("missing_scope")
        || normalized.contains("insufficient_scope")
        || normalized.contains("scope")
        || normalized.contains("access_token_scope_insufficient")
        || normalized.contains("insufficient authentication scopes")
        || normalized.contains("consent_required")
        || normalized.contains("admin_policy_enforced")
    {
        return ProviderErrorClassification {
            kind: "missing_scope",
            auth_state: "ok",
            scope_state: "missing_scope",
            permission_state: "unknown",
            remediation: missing_scope_remediation(platform),
        };
    }
    if normalized.contains("http 403")
        || normalized.contains("forbidden")
        || normalized.contains("missing access")
        || normalized.contains("missing permissions")
        || normalized.contains("missing permission")
        || normalized.contains("not_in_channel")
        || normalized.contains("channel_not_found")
        || normalized.contains("chat not found")
        || normalized.contains("room membership")
        || normalized.contains("m_forbidden")
        || normalized.contains("m_not_joined")
        || normalized.contains("not joined")
        || normalized.contains("permission_denied")
        || normalized.contains("accessnotconfigured")
        || normalized.contains("domain restricted")
        || normalized.contains("domain-restricted")
        || normalized.contains("restricted by administrator")
        || normalized.contains("message_content")
        || normalized.contains("intent")
    {
        return ProviderErrorClassification {
            kind: match platform {
                ChatPlatform::Discord if normalized.contains("intent") => "missing_intent",
                _ => "missing_permission",
            },
            auth_state: "ok",
            scope_state: "unknown",
            permission_state: "missing_permission",
            remediation: missing_permission_remediation(platform, &normalized),
        };
    }
    ProviderErrorClassification {
        kind: "failed",
        auth_state: "failed",
        scope_state: "unknown",
        permission_state: "unknown",
        remediation:
            "Inspect last_error and run the adapter test again after fixing provider configuration.",
    }
}

fn missing_scope_remediation(platform: ChatPlatform) -> &'static str {
    match platform {
        ChatPlatform::Slack => {
            "Grant the Slack app the required bot scopes and reinstall it into the workspace."
        }
        ChatPlatform::Discord => {
            "Review the Discord bot application settings and enabled gateway intents."
        }
        ChatPlatform::Telegram => {
            "Check the Telegram bot token and bot permissions for the configured chat."
        }
        ChatPlatform::Matrix => "Check the Matrix access token and homeserver account grants.",
        ChatPlatform::Mattermost => {
            "Check the Mattermost token type and bot account permissions on this server."
        }
        ChatPlatform::Zulip => "Check the Zulip bot email/API key and stream permissions.",
        ChatPlatform::GoogleChat => {
            "Grant the Google Chat OAuth scopes and workspace admin approval required by the app."
        }
    }
}

fn missing_permission_remediation(platform: ChatPlatform, normalized_error: &str) -> &'static str {
    match platform {
        ChatPlatform::Discord if normalized_error.contains("intent") => {
            "Enable the Discord MESSAGE_CONTENT intent or limit CTOX to DMs, mentions, and visible event fields."
        }
        ChatPlatform::Slack => {
            "Invite the Slack bot to the channel and verify channel allow-list configuration."
        }
        ChatPlatform::Discord => {
            "Grant the Discord bot channel permissions and verify guild/channel allow-lists."
        }
        ChatPlatform::Telegram => {
            "Add the Telegram bot to the chat and review privacy-mode limitations for groups."
        }
        ChatPlatform::Matrix => {
            "Join the Matrix room with the configured account and verify room membership."
        }
        ChatPlatform::Mattermost => {
            "Add the Mattermost bot to the channel and verify team/channel permissions."
        }
        ChatPlatform::Zulip => {
            "Subscribe the Zulip bot to the stream/topic or adjust stream permissions."
        }
        ChatPlatform::GoogleChat => {
            "Add the Google Chat app to the space and verify workspace app access policy."
        }
    }
}

fn redact_sensitive_text(options: &ChatOptions, text: &str) -> String {
    let mut redacted = text.to_string();
    let token = options.token.trim();
    if token.len() >= 4 {
        redacted = redacted.replace(token, "[redacted]");
    }
    redacted
}

fn last_cursor(options: &ChatOptions) -> Option<String> {
    match options.platform {
        ChatPlatform::Telegram => read_state_value(options, "update-offset").ok().flatten(),
        ChatPlatform::Matrix => read_state_value(options, "sync-token").ok().flatten(),
        ChatPlatform::Slack => last_destination_cursor(options, "slack-latest-ts"),
        ChatPlatform::Discord => last_destination_cursor(options, "discord-latest-id"),
        ChatPlatform::Mattermost => last_destination_cursor(options, "mattermost-latest-create-at"),
        ChatPlatform::Zulip => read_state_value(options, "zulip-latest-id").ok().flatten(),
        _ => None,
    }
}

fn last_destination_cursor(options: &ChatOptions, prefix: &str) -> Option<String> {
    let mut cursors = Vec::new();
    for destination in &options.channel_ids {
        let cursor = read_state_value(options, &destination_state_key(prefix, destination))
            .ok()
            .flatten();
        if let Some(cursor) = cursor.filter(|value| !value.is_empty()) {
            cursors.push(format!("{destination}:{cursor}"));
        }
    }
    (!cursors.is_empty()).then(|| cursors.join(","))
}

fn destination_state_key(prefix: &str, destination: &str) -> String {
    format!("{prefix}-{}", stable_digest(destination))
}

fn max_decimal_cursor(current: Option<String>, candidate: String) -> Option<String> {
    max_cursor_by(current, candidate, |left, right| {
        let left_num = left.parse::<f64>().ok();
        let right_num = right.parse::<f64>().ok();
        match (left_num, right_num) {
            (Some(left_num), Some(right_num)) => left_num
                .partial_cmp(&right_num)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => left.cmp(right),
        }
    })
}

fn max_numeric_cursor(current: Option<String>, candidate: String) -> Option<String> {
    max_cursor_by(current, candidate, |left, right| {
        let left_num = left.parse::<u128>().ok();
        let right_num = right.parse::<u128>().ok();
        match (left_num, right_num) {
            (Some(left_num), Some(right_num)) => left_num.cmp(&right_num),
            _ => left.cmp(right),
        }
    })
}

fn max_cursor_by<F>(current: Option<String>, candidate: String, compare: F) -> Option<String>
where
    F: Fn(&str, &str) -> std::cmp::Ordering,
{
    if candidate.trim().is_empty() {
        return current;
    }
    match current {
        Some(current) if compare(&candidate, &current).is_le() => Some(current),
        _ => Some(candidate),
    }
}

fn rate_limited_until_ms_from_error(error: &str) -> Option<i64> {
    if !(error.contains("HTTP 429") || error.to_ascii_lowercase().contains("rate")) {
        return None;
    }
    error
        .split("retry_after=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.trim_matches(['"', '\'']).parse::<i64>().ok())
        .map(|seconds| current_unix_millis() + seconds.max(0) * 1000)
}

fn current_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn resolve_destination(
    options: &ChatOptions,
    request: &ChatSendCommandRequest<'_>,
) -> Result<String> {
    if let Some(value) = request
        .to
        .iter()
        .map(String::as_str)
        .find_map(nonempty_string)
    {
        return Ok(value);
    }
    for marker in ["channel", "chat", "room", "space", "stream"] {
        if let Some(value) = thread_component(&request.thread_key, marker) {
            return Ok(value);
        }
    }
    if let Some(value) = options
        .channel_ids
        .first()
        .map(String::as_str)
        .and_then(nonempty_string)
    {
        return Ok(value);
    }
    bail!(
        "{} send requires --to or a configured default destination",
        options.platform.display_name()
    )
}

fn required_destinations(options: &ChatOptions) -> Result<Vec<String>> {
    if options.channel_ids.is_empty() {
        bail!(
            "{} sync requires at least one configured destination",
            options.platform.display_name()
        );
    }
    Ok(options.channel_ids.clone())
}

fn thread_key_for_destination(
    platform: ChatPlatform,
    account_suffix: &str,
    destination: &str,
    remote_id: &str,
) -> String {
    let marker = match platform {
        ChatPlatform::Telegram => "chat",
        ChatPlatform::Matrix => "room",
        ChatPlatform::GoogleChat => "space",
        ChatPlatform::Zulip => "stream",
        _ => "channel",
    };
    format!(
        "{}:{}::{}::{}::thread::{}",
        platform.channel(),
        account_key_suffix(account_suffix),
        marker,
        destination,
        remote_id
    )
}

fn remote_id_from_send_result(platform: ChatPlatform, value: Option<&Value>) -> Option<String> {
    let value = value?;
    match platform {
        ChatPlatform::Slack => value
            .get("message")
            .and_then(|message| value_str(message, "ts"))
            .or_else(|| value_str(value, "ts")),
        ChatPlatform::Discord | ChatPlatform::Mattermost => value_str(value, "id"),
        ChatPlatform::Telegram => value
            .get("result")
            .and_then(|result| result.get("message_id"))
            .map(value_to_string),
        ChatPlatform::Matrix => value_str(value, "event_id"),
        ChatPlatform::Zulip => value.get("id").map(value_to_string),
        ChatPlatform::GoogleChat => value_str(value, "name"),
    }
}

fn api_url(options: &ChatOptions, path: &str) -> Result<String> {
    let base = options.base_url.trim();
    if base.is_empty() {
        bail!(
            "{} base URL is not configured",
            options.platform.display_name()
        );
    }
    let mut url = trim_trailing_slash(base);
    if !path.starts_with('/') {
        url.push('/');
    }
    url.push_str(path);
    Ok(url)
}

fn telegram_url(options: &ChatOptions, method: &str) -> Result<String> {
    let base = trim_trailing_slash(&options.base_url);
    if options.token.trim().is_empty() {
        bail!("Telegram bot token is not configured");
    }
    Ok(format!("{base}/bot{}/{}", options.token.trim(), method))
}

fn http_json(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
) -> Result<Value> {
    http_json_response(method, url, headers, body).map(|response| response.value)
}

fn http_json_response(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
) -> Result<ChatHttpJsonResponse> {
    let payload = body.map(serde_json::to_vec).transpose()?;
    let response =
        communication_email_native::http_request(method, url, headers, payload.as_deref())?;
    let status = response.status;
    let headers = response.headers;
    let value = if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&response.body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&response.body).into_owned()))
    };
    if !(200..300).contains(&status) {
        let retry_after = headers.get("retry-after").map(String::as_str).unwrap_or("");
        if retry_after.is_empty() {
            bail!("HTTP {} from {url}: {value}", status);
        }
        bail!(
            "HTTP {} from {url}: {value}; retry_after={retry_after}",
            status
        );
    }
    Ok(ChatHttpJsonResponse { headers, value })
}

fn http_form(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    form: &BTreeMap<String, String>,
) -> Result<Value> {
    let mut headers = headers.clone();
    headers.insert(
        "content-type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    );
    let mut encoded = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in form {
        encoded.append_pair(key, value);
    }
    let payload = encoded.finish();
    let response =
        communication_email_native::http_request(method, url, &headers, Some(payload.as_bytes()))?;
    let value = if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&response.body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&response.body).into_owned()))
    };
    if !(200..300).contains(&response.status) {
        let retry_after = response
            .headers
            .get("retry-after")
            .map(String::as_str)
            .unwrap_or("");
        if retry_after.is_empty() {
            bail!("HTTP {} from {url}: {value}", response.status);
        }
        bail!(
            "HTTP {} from {url}: {value}; retry_after={retry_after}",
            response.status
        );
    }
    Ok(value)
}

fn bearer_headers(token: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    headers.insert(
        "authorization".to_string(),
        format!("Bearer {}", token.trim()),
    );
    headers
}

fn bearer_json_headers(token: &str) -> BTreeMap<String, String> {
    let mut headers = bearer_headers(token);
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers
}

fn discord_headers(token: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    headers.insert("authorization".to_string(), format!("Bot {}", token.trim()));
    headers
}

fn discord_json_headers(token: &str) -> BTreeMap<String, String> {
    let mut headers = discord_headers(token);
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers
}

fn zulip_headers(options: &ChatOptions) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    let auth = BASE64_STANDARD.encode(format!("{}:{}", options.username, options.token));
    headers.insert("authorization".to_string(), format!("Basic {auth}"));
    headers
}

fn json_headers() -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers
}

fn ensure_slack_ok(value: &Value) -> Result<()> {
    if value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(());
    }
    bail!(
        "Slack API failed: {}",
        value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error")
    )
}

fn read_state_value(options: &ChatOptions, name: &str) -> Result<Option<String>> {
    let path = state_file(options, name);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(fs::read_to_string(&path)?.trim().to_string()).filter(|value| !value.is_empty()))
}

fn write_state_value(options: &ChatOptions, name: &str, value: &str) -> Result<()> {
    let path = state_file(options, name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value)?;
    Ok(())
}

fn state_file(options: &ChatOptions, name: &str) -> PathBuf {
    communication_runtime::state_file(
        &options.root,
        options.platform.channel(),
        &format!("{name}.txt"),
    )
}

fn setting(runtime: &BTreeMap<String, String>, key: &str) -> String {
    runtime
        .get(key)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn first_setting(runtime: &BTreeMap<String, String>, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| {
            runtime
                .get(*key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn first_setting_or_default(
    runtime: &BTreeMap<String, String>,
    keys: &[&str],
    default: &str,
) -> String {
    let value = first_setting(runtime, keys);
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

fn first_list(runtime: &BTreeMap<String, String>, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| {
            runtime
                .get(*key)
                .map(|value| split_list(value))
                .filter(|values| !values.is_empty())
        })
        .unwrap_or_default()
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(|ch| matches!(ch, ',' | ';' | '\n'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn nonempty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn trim_slashes(value: &str) -> String {
    value.trim().trim_matches('/').to_string()
}

fn mattermost_base_url(raw: String) -> String {
    let raw = trim_trailing_slash(&raw);
    if raw.is_empty() || raw.ends_with("/api/v4") {
        raw
    } else {
        format!("{raw}/api/v4")
    }
}

fn thread_component(thread_key: &str, marker: &str) -> Option<String> {
    let parts = thread_key.split("::").collect::<Vec<_>>();
    parts
        .windows(2)
        .find(|pair| pair[0] == marker && !pair[1].trim().is_empty())
        .map(|pair| pair[1].trim().to_string())
}

fn reply_thread_component(thread_key: &str) -> Option<String> {
    thread_component(thread_key, "thread")
}

fn value_str(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => value.to_string(),
    }
}

fn telegram_display_name(value: &Value) -> Option<String> {
    let first = value_str(value, "first_name");
    let last = value_str(value, "last_name");
    let username = value_str(value, "username");
    match (first, last, username) {
        (Some(first), Some(last), _) => Some(format!("{first} {last}")),
        (Some(first), None, _) => Some(first),
        (None, Some(last), _) => Some(last),
        (None, None, username) => username,
    }
}

fn account_key_suffix(account_key: &str) -> String {
    account_key
        .split_once(':')
        .map(|(_, suffix)| suffix)
        .unwrap_or(account_key)
        .to_string()
}

fn slack_ts_to_iso(value: Option<&Value>) -> String {
    let Some(ts) = value.and_then(Value::as_str) else {
        return now_iso_string();
    };
    let seconds = ts
        .split_once('.')
        .map(|(whole, _)| whole)
        .unwrap_or(ts)
        .parse::<i64>()
        .ok();
    unix_seconds_to_iso(seconds)
}

fn unix_seconds_to_iso(seconds: Option<i64>) -> String {
    seconds
        .and_then(|seconds| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .unwrap_or_else(now_iso_string)
}

fn unix_millis_to_iso(millis: Option<i64>) -> String {
    millis
        .and_then(|millis| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        })
        .unwrap_or_else(now_iso_string)
}

trait EmptyStringExt {
    fn if_empty_else<F: FnOnce() -> String>(self, fallback: F) -> String;
}

impl EmptyStringExt for String {
    fn if_empty_else<F: FnOnce() -> String>(self, fallback: F) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_component_reads_marker_pairs() {
        assert_eq!(
            thread_component("slack:acct::channel::C123::thread::171", "channel").as_deref(),
            Some("C123")
        );
        assert_eq!(
            thread_component("slack:acct::channel::C123::thread::171", "thread").as_deref(),
            Some("171")
        );
        assert_eq!(thread_component("slack:acct::channel::", "channel"), None);
    }

    #[test]
    fn mattermost_base_url_adds_api_suffix_once() {
        assert_eq!(
            mattermost_base_url("https://chat.example.test".to_string()),
            "https://chat.example.test/api/v4"
        );
        assert_eq!(
            mattermost_base_url("https://chat.example.test/api/v4".to_string()),
            "https://chat.example.test/api/v4"
        );
    }

    #[test]
    fn options_from_runtime_resolves_bot_chat_destinations() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut runtime = BTreeMap::new();
        runtime.insert("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string());
        runtime.insert(
            "CTO_SLACK_CHANNEL_IDS".to_string(),
            "C123, C456".to_string(),
        );

        let options = options_from_runtime(ChatPlatform::Slack, root, &runtime, &db_path);

        assert_eq!(options.base_url, "https://slack.com/api");
        assert_eq!(options.token, "xoxb-test");
        assert_eq!(options.channel_ids, vec!["C123", "C456"]);
        assert!(has_minimum_sync_config(&options));
    }

    #[test]
    fn resolve_destination_prefers_explicit_to_then_thread_then_default() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut runtime = BTreeMap::new();
        runtime.insert("CTO_DISCORD_BOT_TOKEN".to_string(), "bot-test".to_string());
        runtime.insert(
            "CTO_DISCORD_CHANNEL_ID".to_string(),
            "default-channel".to_string(),
        );
        let options = options_from_runtime(ChatPlatform::Discord, root, &runtime, &db_path);
        let explicit_to = vec!["explicit-channel".to_string()];
        let empty_to = Vec::new();
        let empty_cc = Vec::new();
        let empty_attachments = Vec::new();
        let explicit = ChatSendCommandRequest {
            db_path: &db_path,
            account_key: "discord:bot",
            thread_key: "discord:bot::channel::thread-channel",
            to: &explicit_to,
            cc: &empty_cc,
            sender_display: None,
            subject: "",
            body: "hello",
            attachments: &empty_attachments,
        };
        let from_thread = ChatSendCommandRequest {
            to: &empty_to,
            ..explicit
        };
        let defaulted = ChatSendCommandRequest {
            thread_key: "discord:bot::thread::abc",
            to: &empty_to,
            ..explicit
        };

        assert_eq!(
            resolve_destination(&options, &explicit).unwrap(),
            "explicit-channel"
        );
        assert_eq!(
            resolve_destination(&options, &from_thread).unwrap(),
            "thread-channel"
        );
        assert_eq!(
            resolve_destination(&options, &defaulted).unwrap(),
            "default-channel"
        );
    }

    #[test]
    fn pull_sync_cursor_helpers_keep_high_water_marks() {
        assert_eq!(
            max_decimal_cursor(
                Some("1719360000.000001".to_string()),
                "1719360001.000000".to_string()
            )
            .as_deref(),
            Some("1719360001.000000")
        );
        assert_eq!(
            max_decimal_cursor(
                Some("1719360001.000000".to_string()),
                "1719360000.000001".to_string()
            )
            .as_deref(),
            Some("1719360001.000000")
        );
        assert_eq!(
            max_numeric_cursor(
                Some("1250000000000000000".to_string()),
                "1250000000000000001".to_string()
            )
            .as_deref(),
            Some("1250000000000000001")
        );
        assert_eq!(
            max_numeric_cursor(
                Some("1250000000000000001".to_string()),
                "1250000000000000000".to_string()
            )
            .as_deref(),
            Some("1250000000000000001")
        );
        let key = destination_state_key("slack-latest-ts", "C/with:unsafe spaces");
        assert!(key.starts_with("slack-latest-ts-"));
        assert!(!key.contains('/'));
        assert!(!key.contains(' '));
    }

    #[test]
    fn bot_chat_send_payloads_include_thread_reply_metadata_when_available() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let empty_to = Vec::new();
        let empty_cc = Vec::new();
        let empty_attachments = Vec::new();
        let request = ChatSendCommandRequest {
            db_path: &db_path,
            account_key: "bot:acct",
            thread_key: "discord:acct::channel::C1::thread::123456789",
            to: &empty_to,
            cc: &empty_cc,
            sender_display: None,
            subject: "",
            body: "reply body",
            attachments: &empty_attachments,
        };

        let discord = discord_message_payload(&request);
        assert_eq!(
            discord
                .pointer("/message_reference/message_id")
                .and_then(Value::as_str),
            Some("123456789")
        );
        assert_eq!(
            discord
                .pointer("/message_reference/fail_if_not_exists")
                .and_then(Value::as_bool),
            Some(false)
        );

        let telegram = ChatSendCommandRequest {
            thread_key: "telegram:acct::chat::-1001::thread::424242",
            ..request
        };
        let telegram_payload = telegram_message_payload("-1001", &telegram);
        assert_eq!(
            telegram_payload
                .get("reply_to_message_id")
                .and_then(Value::as_i64),
            Some(424242)
        );
        assert_eq!(
            telegram_payload
                .get("allow_sending_without_reply")
                .and_then(Value::as_bool),
            Some(true)
        );

        let mattermost = ChatSendCommandRequest {
            thread_key: "mattermost:acct::channel::chan::thread::root-post",
            ..request
        };
        assert_eq!(
            mattermost_post_payload("chan", &mattermost)
                .get("root_id")
                .and_then(Value::as_str),
            Some("root-post")
        );

        let google = ChatSendCommandRequest {
            thread_key: "google_chat:acct::space::spaces/A::thread::spaces/A/threads/T",
            ..request
        };
        assert_eq!(
            google_chat_message_payload(&google)
                .pointer("/thread/name")
                .and_then(Value::as_str),
            Some("spaces/A/threads/T")
        );
    }

    #[test]
    fn telegram_normalization_covers_dm_and_group_messages() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let runtime = BTreeMap::from([
            (
                "CTO_TELEGRAM_BOT_TOKEN".to_string(),
                "bot-token".to_string(),
            ),
            (
                "CTO_TELEGRAM_BOT_USERNAME".to_string(),
                "ctox_bot".to_string(),
            ),
        ]);
        let options = options_from_runtime(ChatPlatform::Telegram, root, &runtime, &db_path);

        let dm_update = json!({
            "update_id": 100,
            "message": {
                "message_id": 10,
                "date": 1719360000,
                "chat": { "id": 12345, "type": "private" },
                "from": { "id": 12345, "first_name": "Ada", "username": "ada" },
                "text": "hello dm"
            }
        });
        let dm_message = dm_update.get("message").unwrap();
        let dm =
            normalize_telegram_message(&options, "telegram:acct", "12345", &dm_update, dm_message);
        assert_eq!(dm.message_key, "telegram:acct::INBOX::12345::10");
        assert_eq!(dm.thread_key, "telegram:acct::chat::12345");
        assert_eq!(dm.sender_display, "Ada");
        assert_eq!(dm.body_text, "hello dm");
        assert_eq!(dm.recipients, vec!["12345".to_string()]);

        let group_update = json!({
            "update_id": 101,
            "message": {
                "message_id": 11,
                "date": 1719360001,
                "chat": { "id": -1001, "type": "supergroup", "title": "Ops" },
                "from": { "id": 777, "first_name": "Grace", "last_name": "Hopper" },
                "text": "hello group"
            }
        });
        let group_message = group_update.get("message").unwrap();
        let group = normalize_telegram_message(
            &options,
            "telegram:acct",
            "-1001",
            &group_update,
            group_message,
        );
        assert_eq!(group.message_key, "telegram:acct::INBOX::-1001::11");
        assert_eq!(group.thread_key, "telegram:acct::chat::-1001");
        assert_eq!(group.sender_display, "Grace Hopper");
        assert_eq!(group.body_text, "hello group");
        assert_eq!(group.recipients, vec!["-1001".to_string()]);
    }

    #[test]
    fn slack_normalization_covers_channel_messages_and_thread_replies() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let runtime = BTreeMap::from([
            ("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string()),
            ("CTO_SLACK_WORKSPACE_ID".to_string(), "T123".to_string()),
            ("CTO_SLACK_BOT_USER_ID".to_string(), "Ubot".to_string()),
            ("CTO_SLACK_CHANNEL_ID".to_string(), "C123".to_string()),
        ]);
        let options = options_from_runtime(ChatPlatform::Slack, root, &runtime, &db_path);

        let channel_message = json!({
            "type": "message",
            "user": "U123",
            "text": "channel hello",
            "ts": "1719360000.000001"
        });
        let normalized = normalize_slack_message(&options, "slack:Ubot", "C123", &channel_message);
        assert_eq!(
            normalized.message_key,
            "slack:Ubot::INBOX::C123::1719360000.000001"
        );
        assert_eq!(
            normalized.thread_key,
            "slack:Ubot::channel::C123::thread::1719360000.000001"
        );
        assert_eq!(normalized.remote_id, "1719360000.000001");
        assert_eq!(normalized.body_text, "channel hello");
        assert_eq!(normalized.recipients, vec!["C123".to_string()]);

        let thread_reply = json!({
            "type": "message",
            "user": "U456",
            "text": "thread reply",
            "ts": "1719360001.000002",
            "thread_ts": "1719360000.000001"
        });
        let reply = normalize_slack_message(&options, "slack:Ubot", "C123", &thread_reply);
        assert_eq!(
            reply.message_key,
            "slack:Ubot::INBOX::C123::1719360001.000002"
        );
        assert_eq!(
            reply.thread_key,
            "slack:Ubot::channel::C123::thread::1719360000.000001"
        );
        assert_eq!(reply.remote_id, "1719360001.000002");
        assert_eq!(reply.body_text, "thread reply");
    }

    #[test]
    fn slack_socket_mode_state_builds_open_ack_and_dedupes_envelopes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        let runtime = BTreeMap::from([
            ("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string()),
            ("CTO_SLACK_APP_TOKEN".to_string(), "xapp-test".to_string()),
            ("CTO_SLACK_CHANNEL_ID".to_string(), "C123".to_string()),
        ]);
        let options = options_from_runtime(ChatPlatform::Slack, root.path(), &runtime, &db_path);

        let open = slack_socket_mode_open_request(&options)?;
        assert_eq!(open.method, "POST");
        assert!(open.url.ends_with("/apps.connections.open"));
        assert_eq!(
            open.headers.get("authorization").map(String::as_str),
            Some("Bearer xapp-test")
        );

        let ack = slack_socket_mode_ack_payload("envelope-123")?;
        assert_eq!(
            ack.get("envelope_id").and_then(Value::as_str),
            Some("envelope-123")
        );
        assert!(mark_slack_socket_mode_envelope_seen(
            &options,
            "envelope-123"
        )?);
        assert!(!mark_slack_socket_mode_envelope_seen(
            &options,
            "envelope-123"
        )?);
        assert_eq!(
            read_state_value(&options, "slack-socket-mode-envelope-id")?.as_deref(),
            Some("envelope-123")
        );
        mark_slack_socket_mode_supervisor_state(&options, "starting", None)?;
        mark_slack_socket_mode_supervisor_state(&options, "stopped", None)?;

        let envelope = json!({
            "envelope_id": "envelope-456",
            "type": "events_api",
            "payload": {
                "type": "event_callback",
                "event": {
                    "type": "message",
                    "channel": "C123",
                    "user": "U999",
                    "text": "socket hello",
                    "ts": "1719360002.000003"
                }
            }
        });
        let socket_message =
            normalize_slack_socket_mode_envelope(&options, "slack:Ubot", &envelope)?;
        assert_eq!(socket_message.envelope_id.as_deref(), Some("envelope-456"));
        assert!(!socket_message.duplicate);
        let normalized = socket_message.message.expect("socket mode message");
        assert_eq!(
            normalized.message_key,
            "slack:Ubot::INBOX::C123::1719360002.000003"
        );
        assert_eq!(normalized.body_text, "socket hello");

        let duplicate = normalize_slack_socket_mode_envelope(&options, "slack:Ubot", &envelope)?;
        assert_eq!(duplicate.envelope_id.as_deref(), Some("envelope-456"));
        assert!(duplicate.duplicate);
        assert!(duplicate.message.is_none());

        let profile = profile_json(
            &options,
            &json!({}),
            adapter_status("sync", true, None, None, &options),
        );
        assert_eq!(
            profile
                .pointer("/adapterStatus/slack_socket_mode_state")
                .and_then(Value::as_str),
            Some("envelope_seen")
        );
        assert_eq!(
            profile
                .pointer("/adapterStatus/slack_socket_mode_last_envelope")
                .and_then(Value::as_str),
            Some("envelope-456")
        );
        assert_eq!(
            profile
                .pointer("/adapterStatus/slack_socket_mode_supervisor_state")
                .and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            profile
                .pointer("/adapterStatus/realtime_supervision_state")
                .and_then(Value::as_str),
            Some("stopped")
        );

        Ok(())
    }

    #[test]
    fn provider_error_classification_maps_common_auth_scope_and_rate_states() {
        let slack_scope =
            classify_provider_error(ChatPlatform::Slack, Some("Slack API failed: missing_scope"));
        assert_eq!(slack_scope.kind, "missing_scope");
        assert_eq!(slack_scope.auth_state, "ok");
        assert_eq!(slack_scope.scope_state, "missing_scope");
        assert!(slack_scope.remediation.contains("Slack app"));

        let discord_intent = classify_provider_error(
            ChatPlatform::Discord,
            Some("Gateway event missing privileged MESSAGE_CONTENT intent"),
        );
        assert_eq!(discord_intent.kind, "missing_intent");
        assert_eq!(discord_intent.permission_state, "missing_permission");
        assert!(discord_intent.remediation.contains("MESSAGE_CONTENT"));

        let deauth = classify_provider_error(
            ChatPlatform::Telegram,
            Some("HTTP 401 from https://api.telegram.org/bot[redacted]/getMe: Unauthorized"),
        );
        assert_eq!(deauth.kind, "deauthorized");
        assert_eq!(deauth.auth_state, "deauthorized");
        assert!(deauth.remediation.contains("Reconnect"));

        let rate = classify_provider_error(
            ChatPlatform::Mattermost,
            Some("HTTP 429 from https://chat.example.test/api/v4/posts: {}; retry_after=7"),
        );
        assert_eq!(rate.kind, "rate_limited");
        assert!(rate.remediation.contains("Retry-After"));
        assert!(rate_limited_until_ms_from_error(
            "HTTP 429 from https://chat.example.test/api/v4/posts: {}; retry_after=7"
        )
        .is_some());

        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let slack_options = options_from_runtime(
            ChatPlatform::Slack,
            root,
            &BTreeMap::from([
                ("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string()),
                ("CTO_SLACK_CHANNEL_ID".to_string(), "C123".to_string()),
            ]),
            &db_path,
        );
        let before = current_unix_millis();
        let slack_status = adapter_status(
            "sync",
            false,
            Some("HTTP 429 from https://slack.com/api/conversations.history: {\"ok\":false,\"error\":\"ratelimited\"}; retry_after=7".to_string()),
            None,
            &slack_options,
        );
        assert_eq!(
            slack_status
                .get("provider_error_kind")
                .and_then(Value::as_str),
            Some("rate_limited")
        );
        let until = slack_status
            .get("rate_limited_until_ms")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        assert!(until >= before + 7_000, "unexpected rate limit: {until}");

        let matrix_token = classify_provider_error(
            ChatPlatform::Matrix,
            Some("Matrix error M_UNKNOWN_TOKEN from homeserver"),
        );
        assert_eq!(matrix_token.kind, "deauthorized");

        let google_scope = classify_provider_error(
            ChatPlatform::GoogleChat,
            Some("ACCESS_TOKEN_SCOPE_INSUFFICIENT: insufficient authentication scopes"),
        );
        assert_eq!(google_scope.kind, "missing_scope");

        let google_admin = classify_provider_error(
            ChatPlatform::GoogleChat,
            Some("PERMISSION_DENIED: app is restricted by administrator"),
        );
        assert_eq!(google_admin.kind, "missing_permission");
    }

    #[test]
    fn platform_status_enrichment_exposes_telegram_privacy_and_matrix_e2ee() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;

        let mut telegram_runtime = BTreeMap::new();
        telegram_runtime.insert(
            "CTO_TELEGRAM_BOT_TOKEN".to_string(),
            "bot-token".to_string(),
        );
        telegram_runtime.insert(
            "CTO_TELEGRAM_BOT_USERNAME".to_string(),
            "ctox_bot".to_string(),
        );
        telegram_runtime.insert("CTO_TELEGRAM_CHAT_ID".to_string(), "-1001".to_string());
        let telegram_options = options_from_runtime(
            ChatPlatform::Telegram,
            root.path(),
            &telegram_runtime,
            &db_path,
        );
        let telegram_profile = profile_json(
            &telegram_options,
            &json!({
                "ok": true,
                "result": {
                    "id": 42,
                    "is_bot": true,
                    "username": "ctox_bot",
                    "can_read_all_group_messages": false
                }
            }),
            adapter_status("test", true, None, None, &telegram_options),
        );
        assert_eq!(
            telegram_profile
                .pointer("/adapterStatus/telegram_group_privacy_state")
                .and_then(Value::as_str),
            Some("privacy_mode_limited")
        );
        let telegram_sync_profile = profile_json(
            &telegram_options,
            &json!({}),
            adapter_status("sync", true, None, None, &telegram_options),
        );
        assert_eq!(
            telegram_sync_profile
                .pointer("/adapterStatus/telegram_group_privacy_state")
                .and_then(Value::as_str),
            Some("privacy_mode_limited")
        );

        let mut matrix_runtime = BTreeMap::new();
        matrix_runtime.insert(
            "CTO_MATRIX_HOMESERVER_URL".to_string(),
            "https://matrix.example.test".to_string(),
        );
        matrix_runtime.insert(
            "CTO_MATRIX_ACCESS_TOKEN".to_string(),
            "matrix-token".to_string(),
        );
        matrix_runtime.insert(
            "CTO_MATRIX_ROOM_ID".to_string(),
            "!room:matrix.example.test".to_string(),
        );
        let matrix_options =
            options_from_runtime(ChatPlatform::Matrix, root.path(), &matrix_runtime, &db_path);
        write_state_value(&matrix_options, "matrix-encrypted-events-seen", "2")?;
        let matrix_profile = profile_json(
            &matrix_options,
            &json!({}),
            adapter_status("sync", true, None, None, &matrix_options),
        );
        assert_eq!(
            matrix_profile
                .pointer("/adapterStatus/matrix_e2ee_state")
                .and_then(Value::as_str),
            Some("encrypted_events_not_supported")
        );
        assert_eq!(
            matrix_profile
                .pointer("/adapterStatus/matrix_encrypted_events_seen")
                .and_then(Value::as_i64),
            Some(2)
        );
        assert_eq!(
            matrix_profile
                .pointer("/adapterStatus/matrix_sdk_state_persistence")
                .and_then(Value::as_str),
            Some("required_for_e2ee_not_configured")
        );
        assert_eq!(
            matrix_profile
                .pointer("/adapterStatus/matrix_e2ee_policy")
                .and_then(Value::as_str),
            Some("disabled_until_sdk_state_store")
        );

        let persisted_matrix_profile = profile_json(
            &matrix_options,
            &json!({}),
            adapter_status("test", true, None, None, &matrix_options),
        );
        assert_eq!(
            persisted_matrix_profile
                .pointer("/adapterStatus/matrix_sdk_state_persistence")
                .and_then(Value::as_str),
            Some("required_for_e2ee_not_configured")
        );

        let plaintext_root = tempfile::tempdir()?;
        let plaintext_db_path = plaintext_root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(plaintext_root.path().join("runtime"))?;
        let plaintext_options = options_from_runtime(
            ChatPlatform::Matrix,
            plaintext_root.path(),
            &matrix_runtime,
            &plaintext_db_path,
        );
        let plaintext_profile = profile_json(
            &plaintext_options,
            &json!({}),
            adapter_status("test", true, None, None, &plaintext_options),
        );
        assert_eq!(
            plaintext_profile
                .pointer("/adapterStatus/matrix_sdk_state_persistence")
                .and_then(Value::as_str),
            Some("not_required_plaintext_v1")
        );
        assert_eq!(
            plaintext_profile
                .pointer("/adapterStatus/matrix_e2ee_policy")
                .and_then(Value::as_str),
            Some("plaintext_only_v1")
        );
        Ok(())
    }

    #[test]
    fn adapter_status_exposes_realtime_supervision_readiness() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;

        let mut slack_runtime = BTreeMap::new();
        slack_runtime.insert("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string());
        slack_runtime.insert("CTO_SLACK_APP_TOKEN".to_string(), "xapp-test".to_string());
        slack_runtime.insert("CTO_SLACK_CHANNEL_ID".to_string(), "C123".to_string());
        let slack_options =
            options_from_runtime(ChatPlatform::Slack, root.path(), &slack_runtime, &db_path);
        write_state_value(
            &slack_options,
            "slack-socket-mode-envelope-id",
            "envelope-42",
        )?;
        let slack_status = adapter_status("sync", true, None, None, &slack_options);
        assert_eq!(
            slack_status
                .get("realtime_transport")
                .and_then(Value::as_str),
            Some("socket_mode")
        );
        assert_eq!(
            slack_status
                .get("realtime_config_state")
                .and_then(Value::as_str),
            Some("configured")
        );
        assert_eq!(
            slack_status
                .get("realtime_supervision_state")
                .and_then(Value::as_str),
            Some("supervised_via_service_sync")
        );
        assert_eq!(
            slack_status
                .get("realtime_last_cursor")
                .and_then(Value::as_str),
            Some("envelope-42")
        );

        let mut discord_runtime = BTreeMap::new();
        discord_runtime.insert(
            "CTO_DISCORD_BOT_TOKEN".to_string(),
            "discord-token".to_string(),
        );
        discord_runtime.insert("CTO_DISCORD_CHANNEL_ID".to_string(), "456".to_string());
        let discord_options = options_from_runtime(
            ChatPlatform::Discord,
            root.path(),
            &discord_runtime,
            &db_path,
        );
        let discord_status = adapter_status("test", true, None, None, &discord_options);
        assert_eq!(
            discord_status
                .get("realtime_transport")
                .and_then(Value::as_str),
            Some("gateway")
        );
        assert_eq!(
            discord_status
                .get("realtime_supervision_state")
                .and_then(Value::as_str),
            Some("not_implemented")
        );

        let mut matrix_runtime = BTreeMap::new();
        matrix_runtime.insert(
            "CTO_MATRIX_HOMESERVER_URL".to_string(),
            "https://matrix.example.test".to_string(),
        );
        matrix_runtime.insert(
            "CTO_MATRIX_ACCESS_TOKEN".to_string(),
            "matrix-token".to_string(),
        );
        let matrix_options =
            options_from_runtime(ChatPlatform::Matrix, root.path(), &matrix_runtime, &db_path);
        write_state_value(&matrix_options, "sync-token", "s123")?;
        let matrix_status = adapter_status(
            "sync",
            true,
            None,
            last_cursor(&matrix_options),
            &matrix_options,
        );
        assert_eq!(
            matrix_status
                .get("realtime_supervision_state")
                .and_then(Value::as_str),
            Some("polling_via_service_sync")
        );
        assert_eq!(
            matrix_status
                .get("realtime_last_cursor")
                .and_then(Value::as_str),
            Some("s123")
        );

        let mut zulip_runtime = BTreeMap::new();
        zulip_runtime.insert(
            "CTO_ZULIP_REALM_URL".to_string(),
            "https://zulip.example.test".to_string(),
        );
        zulip_runtime.insert("CTO_ZULIP_API_KEY".to_string(), "zulip-key".to_string());
        zulip_runtime.insert(
            "CTO_ZULIP_BOT_EMAIL".to_string(),
            "bot@zulip.example.test".to_string(),
        );
        zulip_runtime.insert("CTO_ZULIP_STREAM".to_string(), "general".to_string());
        zulip_runtime.insert("CTO_ZULIP_TOPIC".to_string(), "ctox".to_string());
        let zulip_options =
            options_from_runtime(ChatPlatform::Zulip, root.path(), &zulip_runtime, &db_path);
        write_state_value(&zulip_options, "zulip-event-last-id", "99")?;
        let zulip_status = adapter_status(
            "sync",
            true,
            None,
            last_cursor(&zulip_options),
            &zulip_options,
        );
        assert_eq!(
            zulip_status
                .get("realtime_supervision_state")
                .and_then(Value::as_str),
            Some("polling_via_service_sync")
        );
        assert_eq!(
            zulip_status
                .get("realtime_cursor_state_key")
                .and_then(Value::as_str),
            Some("zulip-event-last-id")
        );
        assert_eq!(
            zulip_status
                .get("realtime_last_cursor")
                .and_then(Value::as_str),
            Some("99")
        );
        let form = zulip_event_queue_register_form(&zulip_options)?;
        assert_eq!(
            form.get("apply_markdown").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            form.get("event_types").map(String::as_str),
            Some("[\"message\",\"update_message\"]")
        );
        assert!(
            form.get("narrow")
                .is_some_and(|raw| raw.contains("\"channel\"") && raw.contains("\"topic\"")),
            "Zulip event queue registration must include stream/topic narrow: {form:?}"
        );

        Ok(())
    }

    #[test]
    fn realtime_backoff_status_persists_attempt_reason_and_cap() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        let runtime = BTreeMap::from([
            (
                "CTO_MATTERMOST_SERVER_URL".to_string(),
                "https://mattermost.example.test".to_string(),
            ),
            (
                "CTO_MATTERMOST_BOT_TOKEN".to_string(),
                "mattermost-token".to_string(),
            ),
            (
                "CTO_MATTERMOST_CHANNEL_ID".to_string(),
                "mattermost-channel".to_string(),
            ),
        ]);
        let options =
            options_from_runtime(ChatPlatform::Mattermost, root.path(), &runtime, &db_path);
        assert_eq!(realtime_backoff_delay_ms(0), 1_000);
        assert_eq!(realtime_backoff_delay_ms(3), 8_000);
        assert_eq!(realtime_backoff_delay_ms(12), 60_000);

        let before = current_unix_millis();
        let until = record_realtime_backoff(&options, "websocket_connect_failed", 3)?;
        assert!(until >= before + 8_000, "unexpected backoff: {until}");
        let status = adapter_status("sync", false, None, None, &options);
        assert_eq!(
            status
                .get("realtime_backoff_attempt")
                .and_then(Value::as_i64),
            Some(3)
        );
        assert_eq!(
            status
                .get("realtime_backoff_reason")
                .and_then(Value::as_str),
            Some("websocket_connect_failed")
        );
        assert!(status
            .get("realtime_backoff_until_ms")
            .and_then(Value::as_i64)
            .is_some_and(|value| value >= before + 8_000));

        clear_realtime_backoff(&options)?;
        let cleared = adapter_status("sync", true, None, None, &options);
        assert!(cleared
            .get("realtime_backoff_until_ms")
            .is_some_and(Value::is_null));
        assert!(cleared
            .get("realtime_backoff_attempt")
            .is_some_and(Value::is_null));
        assert!(cleared
            .get("realtime_backoff_reason")
            .is_some_and(Value::is_null));

        Ok(())
    }

    #[test]
    fn discord_gateway_resume_state_builds_identify_and_resume_payloads() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        let runtime = BTreeMap::from([
            (
                "CTO_DISCORD_BOT_TOKEN".to_string(),
                "discord-token".to_string(),
            ),
            ("CTO_DISCORD_CHANNEL_ID".to_string(), "456".to_string()),
        ]);
        let options = options_from_runtime(ChatPlatform::Discord, root.path(), &runtime, &db_path);

        let identify = discord_gateway_identify_payload(&options);
        assert_eq!(identify.get("op").and_then(Value::as_i64), Some(2));
        assert_eq!(
            identify.pointer("/d/token").and_then(Value::as_str),
            Some("discord-token")
        );
        let intents = identify
            .pointer("/d/intents")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        assert_ne!(intents & DISCORD_GATEWAY_INTENT_GUILD_MESSAGES, 0);
        assert_ne!(intents & DISCORD_GATEWAY_INTENT_DIRECT_MESSAGES, 0);
        assert_ne!(intents & DISCORD_GATEWAY_INTENT_MESSAGE_CONTENT, 0);

        persist_discord_gateway_ready_state(&options, "session-123", 41)?;
        persist_discord_gateway_sequence(&options, 40)?;
        assert_eq!(
            read_state_value(&options, "discord-gateway-sequence")?.as_deref(),
            Some("41")
        );
        persist_discord_gateway_sequence(&options, 42)?;

        let resume = discord_gateway_resume_payload(&options)?;
        assert_eq!(resume.get("op").and_then(Value::as_i64), Some(6));
        assert_eq!(
            resume.pointer("/d/session_id").and_then(Value::as_str),
            Some("session-123")
        );
        assert_eq!(resume.pointer("/d/seq").and_then(Value::as_i64), Some(42));

        let profile = profile_json(
            &options,
            &json!({}),
            adapter_status("sync", true, None, None, &options),
        );
        let status = profile
            .pointer("/adapterStatus")
            .and_then(Value::as_object)
            .expect("adapter status");
        assert_eq!(
            status
                .get("discord_gateway_resume_state")
                .and_then(Value::as_str),
            Some("resume_ready")
        );
        assert_eq!(
            status
                .get("discord_gateway_sequence")
                .and_then(Value::as_str),
            Some("42")
        );
        assert!(status.get("discord_gateway_session_id").is_none());

        Ok(())
    }

    #[test]
    fn discord_gateway_message_create_resume_dedupes_by_message_key() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;
        let runtime = BTreeMap::from([
            (
                "CTO_DISCORD_BOT_TOKEN".to_string(),
                "discord-token".to_string(),
            ),
            ("CTO_DISCORD_CHANNEL_ID".to_string(), "456".to_string()),
        ]);
        let options = options_from_runtime(ChatPlatform::Discord, root.path(), &runtime, &db_path);
        let account_key = "discord:bot-user";
        let mut conn = open_channel_db(&db_path)?;
        ensure_account(
            &mut conn,
            account_key,
            "discord",
            "bot-user",
            "discord-rest",
            json!({}),
        )?;

        let event = json!({
            "op": 0,
            "t": "MESSAGE_CREATE",
            "s": 55,
            "d": {
                "id": "999",
                "channel_id": "456",
                "timestamp": "2026-06-26T12:00:00Z",
                "content": "gateway hello",
                "author": {
                    "id": "u1",
                    "username": "Ada"
                }
            }
        });
        let first = normalize_discord_gateway_message_create(&options, account_key, &event)?
            .expect("gateway message");
        let second = normalize_discord_gateway_message_create(&options, account_key, &event)?
            .expect("gateway message");
        assert_eq!(first.message_key, second.message_key);
        store_chat_message(&mut conn, ChatPlatform::Discord, &first)?;
        store_chat_message(&mut conn, ChatPlatform::Discord, &second)?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE channel = 'discord' AND remote_id = '999'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        assert_eq!(
            read_state_value(&options, "discord-gateway-sequence")?.as_deref(),
            Some("55")
        );

        Ok(())
    }

    #[test]
    fn probe_errors_update_slack_and_discord_adapter_status() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut slack_runtime = BTreeMap::new();
        slack_runtime.insert("CTO_SLACK_BOT_TOKEN".to_string(), "xoxb-test".to_string());
        slack_runtime.insert("CTO_SLACK_CHANNEL_ID".to_string(), "C123".to_string());
        let slack_options =
            options_from_runtime(ChatPlatform::Slack, root, &slack_runtime, &db_path);
        let slack_profile = profile_json(
            &slack_options,
            &json!({
                "ok": true,
                "team_id": "T123",
                "user_id": "U123",
                "channelProbes": [{
                    "ok": false,
                    "channel_id": "C123",
                    "error": "Slack API failed: missing_scope"
                }]
            }),
            adapter_status("test", true, None, None, &slack_options),
        );
        let slack_status = slack_profile
            .pointer("/adapterStatus")
            .and_then(Value::as_object)
            .expect("adapter status");
        assert_eq!(
            slack_status
                .get("provider_error_kind")
                .and_then(Value::as_str),
            Some("missing_scope")
        );
        assert_eq!(
            slack_status.get("scope_state").and_then(Value::as_str),
            Some("missing_scope")
        );
        assert_eq!(
            slack_status
                .get("channel_probe_state")
                .and_then(Value::as_str),
            Some("failed")
        );

        let mut discord_runtime = BTreeMap::new();
        discord_runtime.insert(
            "CTO_DISCORD_BOT_TOKEN".to_string(),
            "discord-token".to_string(),
        );
        discord_runtime.insert("CTO_DISCORD_CHANNEL_ID".to_string(), "456".to_string());
        let discord_options =
            options_from_runtime(ChatPlatform::Discord, root, &discord_runtime, &db_path);
        let discord_profile = profile_json(
            &discord_options,
            &json!({
                "id": "bot-user",
                "username": "ctox",
                "gatewayProbe": { "ok": true, "label": "gateway" },
                "applicationProbe": { "ok": true, "label": "application" },
                "channelProbes": [{
                    "ok": false,
                    "label": "456",
                    "error": "HTTP 403 from https://discord.com/api/v10/channels/456: Missing Access"
                }]
            }),
            adapter_status("test", true, None, None, &discord_options),
        );
        let discord_status = discord_profile
            .pointer("/adapterStatus")
            .and_then(Value::as_object)
            .expect("adapter status");
        assert_eq!(
            discord_status
                .get("provider_error_kind")
                .and_then(Value::as_str),
            Some("missing_permission")
        );
        assert_eq!(
            discord_status
                .get("permission_state")
                .and_then(Value::as_str),
            Some("missing_permission")
        );
        assert_eq!(
            discord_status
                .get("gateway_probe_state")
                .and_then(Value::as_str),
            Some("ok")
        );
        assert_eq!(
            discord_status
                .get("channel_probe_state")
                .and_then(Value::as_str),
            Some("failed")
        );
    }

    #[test]
    fn self_hosted_adapter_status_exposes_server_diagnostics() {
        let root = Path::new("/tmp/ctox-test-root");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut mattermost_runtime = BTreeMap::new();
        mattermost_runtime.insert(
            "CTO_MATTERMOST_SERVER_URL".to_string(),
            "https://mattermost.example.test".to_string(),
        );
        mattermost_runtime.insert(
            "CTO_MATTERMOST_BOT_TOKEN".to_string(),
            "mattermost-token".to_string(),
        );
        mattermost_runtime.insert(
            "CTO_MATTERMOST_CHANNEL_ID".to_string(),
            "mattermost-channel".to_string(),
        );
        let mattermost_options = options_from_runtime(
            ChatPlatform::Mattermost,
            root,
            &mattermost_runtime,
            &db_path,
        );
        let mattermost_profile = profile_json(
            &mattermost_options,
            &json!({
                "id": "bot-user",
                "username": "ctox",
                "serverProbe": {
                    "ok": true,
                    "version": "9.11.0",
                }
            }),
            adapter_status("test", true, None, None, &mattermost_options),
        );
        let mattermost_status = mattermost_profile
            .pointer("/adapterStatus")
            .and_then(Value::as_object)
            .expect("adapter status");
        assert_eq!(
            mattermost_status
                .get("server_url_state")
                .and_then(Value::as_str),
            Some("ok")
        );
        assert_eq!(
            mattermost_status.get("tls_state").and_then(Value::as_str),
            Some("https")
        );
        assert_eq!(
            mattermost_status
                .get("server_version")
                .and_then(Value::as_str),
            Some("9.11.0")
        );
        assert_eq!(
            mattermost_status
                .get("server_probe_state")
                .and_then(Value::as_str),
            Some("ok")
        );

        let mut zulip_runtime = BTreeMap::new();
        zulip_runtime.insert(
            "CTO_ZULIP_REALM_URL".to_string(),
            "http://zulip.example.test".to_string(),
        );
        zulip_runtime.insert("CTO_ZULIP_API_KEY".to_string(), "zulip-key".to_string());
        zulip_runtime.insert(
            "CTO_ZULIP_BOT_EMAIL".to_string(),
            "bot@zulip.example.test".to_string(),
        );
        let zulip_options =
            options_from_runtime(ChatPlatform::Zulip, root, &zulip_runtime, &db_path);
        let zulip_profile = profile_json(
            &zulip_options,
            &json!({
                "email": "bot@zulip.example.test",
                "serverSettings": {
                    "zulip_version": "9.4",
                }
            }),
            adapter_status("test", true, None, None, &zulip_options),
        );
        let zulip_status = zulip_profile
            .pointer("/adapterStatus")
            .and_then(Value::as_object)
            .expect("adapter status");
        assert_eq!(
            zulip_status.get("tls_state").and_then(Value::as_str),
            Some("plain_http")
        );
        assert_eq!(
            zulip_status.get("server_version").and_then(Value::as_str),
            Some("9.4")
        );
        assert_eq!(
            zulip_status
                .get("server_probe_state")
                .and_then(Value::as_str),
            Some("ok")
        );
    }

    #[test]
    fn zulip_update_message_event_updates_content_and_moves_topics() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;
        let runtime = BTreeMap::from([
            (
                "CTO_ZULIP_REALM_URL".to_string(),
                "https://zulip.example.test".to_string(),
            ),
            ("CTO_ZULIP_API_KEY".to_string(), "zulip-key".to_string()),
            (
                "CTO_ZULIP_BOT_EMAIL".to_string(),
                "bot@zulip.example.test".to_string(),
            ),
            ("CTO_ZULIP_STREAM".to_string(), "Verona".to_string()),
            ("CTO_ZULIP_TOPIC".to_string(), "test".to_string()),
        ]);
        let options = options_from_runtime(ChatPlatform::Zulip, root.path(), &runtime, &db_path);
        let account_key = "zulip:bot@zulip.example.test";
        let mut conn = open_channel_db(&db_path)?;
        ensure_account(
            &mut conn,
            account_key,
            "zulip",
            "bot@zulip.example.test",
            "zulip-rest-api",
            json!({}),
        )?;

        for (id, content) in [(58, "old content"), (59, "second body")] {
            let raw = json!({
                "id": id,
                "display_recipient": "Verona",
                "subject": "test",
                "sender_full_name": "Othello Bot",
                "sender_email": "othello@zulip.example.test",
                "content": content,
                "timestamp": 1594825416,
                "type": "stream"
            });
            let message = normalize_zulip_message(&options, account_key, &raw);
            store_chat_message(&mut conn, ChatPlatform::Zulip, &message)?;
        }

        let event = json!({
            "type": "update_message",
            "id": 42,
            "message_id": 58,
            "message_ids": [58, 59],
            "stream_name": "Verona",
            "orig_subject": "test",
            "subject": "new_topic",
            "content": "new content",
            "orig_content": "old content",
            "edit_timestamp": 1594825451,
            "propagate_mode": "change_all",
            "rendering_only": false
        });
        let updated = apply_zulip_update_message_event(&mut conn, &options, account_key, &event)?;
        assert_eq!(updated, 3);

        let first = zulip_message_row(&conn, account_key, "58")?;
        assert_eq!(first.body_text, "new content");
        assert_eq!(first.subject, "Zulip Verona / new_topic");
        assert_eq!(
            first.thread_key,
            "zulip:bot@zulip.example.test::stream::Verona::topic::new_topic"
        );
        assert!(first.metadata_json.contains("zulipUpdateMessageEvent"));
        assert!(first.metadata_json.contains("zulipTopicMoveEvent"));

        let second = zulip_message_row(&conn, account_key, "59")?;
        assert_eq!(second.body_text, "second body");
        assert_eq!(second.subject, "Zulip Verona / new_topic");
        assert_eq!(
            second.thread_key,
            "zulip:bot@zulip.example.test::stream::Verona::topic::new_topic"
        );
        assert!(second.metadata_json.contains("zulipTopicMoveEvent"));
        assert!(!second.metadata_json.contains("zulipUpdateMessageEvent"));

        Ok(())
    }

    #[derive(Debug)]
    struct ZulipMessageRow {
        thread_key: String,
        subject: String,
        body_text: String,
        metadata_json: String,
    }

    fn zulip_message_row(
        conn: &rusqlite::Connection,
        account_key: &str,
        message_id: &str,
    ) -> Result<ZulipMessageRow> {
        let message_key = format!("{account_key}::INBOX::{message_id}");
        conn.query_row(
            r#"
            SELECT thread_key, subject, body_text, metadata_json
            FROM communication_messages
            WHERE message_key = ?1
            "#,
            rusqlite::params![message_key],
            |row| {
                Ok(ZulipMessageRow {
                    thread_key: row.get(0)?,
                    subject: row.get(1)?,
                    body_text: row.get(2)?,
                    metadata_json: row.get(3)?,
                })
            },
        )
        .map_err(Into::into)
    }

    #[test]
    fn fake_provider_smoke_covers_all_bot_chat_adapters() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;
        let cases = vec![
            (
                ChatPlatform::Slack,
                BTreeMap::from([
                    (
                        "CTO_SLACK_API_BASE_URL".to_string(),
                        "ctox-fake://slack".to_string(),
                    ),
                    ("CTO_SLACK_BOT_TOKEN".to_string(), FAKE_TOKEN.to_string()),
                    ("CTO_SLACK_WORKSPACE_ID".to_string(), "TFAKE".to_string()),
                    ("CTO_SLACK_BOT_USER_ID".to_string(), "UFAKE".to_string()),
                    ("CTO_SLACK_CHANNEL_ID".to_string(), "CFAKE".to_string()),
                ]),
                "CFAKE",
            ),
            (
                ChatPlatform::Discord,
                BTreeMap::from([
                    (
                        "CTO_DISCORD_API_BASE_URL".to_string(),
                        "ctox-fake://discord".to_string(),
                    ),
                    ("CTO_DISCORD_BOT_TOKEN".to_string(), FAKE_TOKEN.to_string()),
                    (
                        "CTO_DISCORD_APPLICATION_ID".to_string(),
                        "DFAKE".to_string(),
                    ),
                    ("CTO_DISCORD_CHANNEL_ID".to_string(), "DCFAKE".to_string()),
                ]),
                "DCFAKE",
            ),
            (
                ChatPlatform::Telegram,
                BTreeMap::from([
                    (
                        "CTO_TELEGRAM_API_BASE_URL".to_string(),
                        "ctox-fake://telegram".to_string(),
                    ),
                    ("CTO_TELEGRAM_BOT_TOKEN".to_string(), FAKE_TOKEN.to_string()),
                    (
                        "CTO_TELEGRAM_BOT_USERNAME".to_string(),
                        "ctox_fake_bot".to_string(),
                    ),
                    ("CTO_TELEGRAM_CHAT_ID".to_string(), "-1001".to_string()),
                ]),
                "-1001",
            ),
            (
                ChatPlatform::Matrix,
                BTreeMap::from([
                    (
                        "CTO_MATRIX_HOMESERVER_URL".to_string(),
                        "ctox-fake://matrix".to_string(),
                    ),
                    (
                        "CTO_MATRIX_ACCESS_TOKEN".to_string(),
                        FAKE_TOKEN.to_string(),
                    ),
                    (
                        "CTO_MATRIX_USER_ID".to_string(),
                        "@ctox:matrix.test".to_string(),
                    ),
                    (
                        "CTO_MATRIX_ROOM_ID".to_string(),
                        "!fake-room:matrix.test".to_string(),
                    ),
                ]),
                "!fake-room:matrix.test",
            ),
            (
                ChatPlatform::Mattermost,
                BTreeMap::from([
                    (
                        "CTO_MATTERMOST_SERVER_URL".to_string(),
                        "ctox-fake://mattermost".to_string(),
                    ),
                    (
                        "CTO_MATTERMOST_BOT_TOKEN".to_string(),
                        FAKE_TOKEN.to_string(),
                    ),
                    (
                        "CTO_MATTERMOST_BOT_USER_ID".to_string(),
                        "MMFAKE".to_string(),
                    ),
                    (
                        "CTO_MATTERMOST_CHANNEL_ID".to_string(),
                        "MMCHAN".to_string(),
                    ),
                ]),
                "MMCHAN",
            ),
            (
                ChatPlatform::Zulip,
                BTreeMap::from([
                    (
                        "CTO_ZULIP_REALM_URL".to_string(),
                        "ctox-fake://zulip".to_string(),
                    ),
                    (
                        "CTO_ZULIP_BOT_EMAIL".to_string(),
                        "bot@zulip.test".to_string(),
                    ),
                    ("CTO_ZULIP_API_KEY".to_string(), FAKE_TOKEN.to_string()),
                    ("CTO_ZULIP_STREAM".to_string(), "general".to_string()),
                    ("CTO_ZULIP_TOPIC".to_string(), "ctox".to_string()),
                ]),
                "general",
            ),
            (
                ChatPlatform::GoogleChat,
                BTreeMap::from([
                    (
                        "CTO_GOOGLE_CHAT_API_BASE_URL".to_string(),
                        "ctox-fake://google-chat".to_string(),
                    ),
                    (
                        "CTO_GOOGLE_CHAT_ACCESS_TOKEN".to_string(),
                        FAKE_TOKEN.to_string(),
                    ),
                    (
                        "CTO_GOOGLE_CHAT_USER".to_string(),
                        "ctox@google.test".to_string(),
                    ),
                    (
                        "CTO_GOOGLE_CHAT_SPACE_NAME".to_string(),
                        "spaces/fake-space".to_string(),
                    ),
                ]),
                "spaces/fake-space",
            ),
        ];

        for (platform, runtime, destination) in cases {
            let test_result = super::test(
                platform,
                root.path(),
                &runtime,
                &ChatTestCommandRequest {
                    db_path: &db_path,
                    profile_json: &json!({}),
                },
            )?;
            assert_eq!(
                test_result.get("ok").and_then(Value::as_bool),
                Some(true),
                "{} fake test must pass",
                platform.channel()
            );

            let args = vec!["sync".to_string()];
            let sync_result = sync(
                platform,
                root.path(),
                &runtime,
                &AdapterSyncCommandRequest {
                    db_path: &db_path,
                    passthrough_args: &args,
                    skip_flags: &[],
                },
            )?;
            assert_eq!(
                sync_result.get("storedCount").and_then(Value::as_u64),
                Some(1),
                "{} fake sync must store one inbound message",
                platform.channel()
            );

            let account_key = sync_result
                .get("account_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let to = vec![destination.to_string()];
            let empty_cc = Vec::new();
            let empty_attachments = Vec::new();
            let send_result = send(
                platform,
                root.path(),
                &runtime,
                &ChatSendCommandRequest {
                    db_path: &db_path,
                    account_key: &account_key,
                    thread_key: "",
                    to: &to,
                    cc: &empty_cc,
                    sender_display: Some("CTOX Test"),
                    subject: "Fake reply",
                    body: "Fake outbound reply",
                    attachments: &empty_attachments,
                },
            )?;
            assert_eq!(
                send_result.get("status").and_then(Value::as_str),
                Some("sent"),
                "{} fake send must be marked sent",
                platform.channel()
            );

            let attachments = vec!["/tmp/fake-upload.txt".to_string()];
            let error = send(
                platform,
                root.path(),
                &runtime,
                &ChatSendCommandRequest {
                    db_path: &db_path,
                    account_key: &account_key,
                    thread_key: "",
                    to: &to,
                    cc: &empty_cc,
                    sender_display: Some("CTOX Test"),
                    subject: "Fake attachment",
                    body: "Fake attachment reply",
                    attachments: &attachments,
                },
            )
            .expect_err("bot chat adapter fake send must reject attachments");
            assert!(
                error.to_string().contains("text-only"),
                "unexpected {} attachment error: {error}",
                platform.channel()
            );
        }

        Ok(())
    }

    #[test]
    fn fake_provider_test_sync_and_send_persist_status() -> Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = root.path().join("runtime/ctox.sqlite3");
        fs::create_dir_all(root.path().join("runtime"))?;
        let mut runtime = BTreeMap::new();
        runtime.insert(
            "CTO_SLACK_API_BASE_URL".to_string(),
            "ctox-fake://slack".to_string(),
        );
        runtime.insert("CTO_SLACK_BOT_TOKEN".to_string(), FAKE_TOKEN.to_string());
        runtime.insert("CTO_SLACK_WORKSPACE_ID".to_string(), "TFAKE".to_string());
        runtime.insert("CTO_SLACK_BOT_USER_ID".to_string(), "UFAKE".to_string());
        runtime.insert("CTO_SLACK_CHANNEL_ID".to_string(), "CFAKE".to_string());

        let test_result = super::test(
            ChatPlatform::Slack,
            root.path(),
            &runtime,
            &ChatTestCommandRequest {
                db_path: &db_path,
                profile_json: &json!({}),
            },
        )?;
        assert_eq!(test_result.get("ok").and_then(Value::as_bool), Some(true));

        let args = vec!["sync".to_string()];
        let sync_result = sync(
            ChatPlatform::Slack,
            root.path(),
            &runtime,
            &AdapterSyncCommandRequest {
                db_path: &db_path,
                passthrough_args: &args,
                skip_flags: &[],
            },
        )?;
        assert_eq!(
            sync_result.get("storedCount").and_then(Value::as_u64),
            Some(1)
        );

        let empty_to = Vec::new();
        let empty_cc = Vec::new();
        let empty_attachments = Vec::new();
        let send_result = send(
            ChatPlatform::Slack,
            root.path(),
            &runtime,
            &ChatSendCommandRequest {
                db_path: &db_path,
                account_key: "slack:UFAKE",
                thread_key: "slack:UFAKE::channel::CFAKE::thread::1719360000.000001",
                to: &empty_to,
                cc: &empty_cc,
                sender_display: Some("CTOX Test"),
                subject: "Fake reply",
                body: "Fake outbound reply",
                attachments: &empty_attachments,
            },
        )?;
        assert_eq!(
            send_result.get("status").and_then(Value::as_str),
            Some("sent")
        );

        let conn = rusqlite::Connection::open(&db_path)?;
        let inbound_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE channel = 'slack' AND direction = 'inbound'",
            [],
            |row| row.get(0),
        )?;
        let outbound_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE channel = 'slack' AND direction = 'outbound'",
            [],
            |row| row.get(0),
        )?;
        let profile_raw: String = conn.query_row(
            "SELECT profile_json FROM communication_accounts WHERE account_key = 'slack:UFAKE'",
            [],
            |row| row.get(0),
        )?;
        let profile: Value = serde_json::from_str(&profile_raw)?;
        assert_eq!(inbound_count, 1);
        assert_eq!(outbound_count, 1);
        assert_eq!(
            profile
                .get("adapterStatus")
                .and_then(|status| status.get("auth_state"))
                .and_then(Value::as_str),
            Some("ok")
        );
        assert_eq!(
            profile
                .get("adapterStatus")
                .and_then(|status| status.get("fake_provider"))
                .and_then(Value::as_bool),
            Some(true)
        );

        Ok(())
    }
}
