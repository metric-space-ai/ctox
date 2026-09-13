// Business OS projection surface: account/thread/message pull documents,
// pairing state, channel settings and the intake source stamp that the
// native RxDB peer projects into the browser store.

use super::{
    channel_projection_tables_exist, communication_adapters, empty_business_os_projection,
    open_channel_db, open_channel_db_read_only, resolve_db_path, sync_channel,
    sync_prompt_identity, test_channel, CommunicationIntakeSourceStamp,
    COMMUNICATION_INTAKE_SOURCE_STAMP_SQL,
};
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub fn list_communication_accounts_for_business_os(root: &Path) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            account_key, channel, address, provider, profile_json,
            created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at
        FROM communication_accounts
        ORDER BY channel ASC, address ASC
        "#,
    )?;
    let mut accounts: Vec<Value> = Vec::new();
    let rows = stmt.query_map([], |row| {
        let account_key: String = row.get(0)?;
        let channel: String = row.get(1)?;
        let address: String = row.get(2)?;
        let provider: String = row.get(3)?;
        let profile_raw: String = row.get(4)?;
        let created_at: String = row.get(5)?;
        let updated_at: String = row.get(6)?;
        let last_inbound_ok_at: Option<String> = row.get(7)?;
        let last_outbound_ok_at: Option<String> = row.get(8)?;
        Ok(json!({
            "account_key": account_key,
            "channel": channel,
            "address": address,
            "provider": provider,
            "profile_json": serde_json::from_str::<Value>(&profile_raw).unwrap_or_else(|_| json!({})),
            "created_at": created_at,
            "updated_at": updated_at,
            "last_inbound_ok_at": last_inbound_ok_at,
            "last_outbound_ok_at": last_outbound_ok_at,
        }))
    })?;
    for row in rows {
        accounts.push(row?);
    }
    Ok(json!({ "ok": true, "accounts": accounts }))
}

pub fn disconnect_communication_account_for_business_os(
    root: &Path,
    account_key: &str,
) -> Result<Value> {
    if account_key.trim().is_empty() {
        anyhow::bail!("account_key is required");
    }
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let affected = conn.execute(
        "DELETE FROM communication_accounts WHERE account_key = ?1",
        params![account_key],
    )?;
    Ok(json!({
        "ok": true,
        "account_key": account_key,
        "removed": affected,
    }))
}

pub fn test_channel_for_business_os(
    root: &Path,
    channel: &str,
    account_key: Option<&str>,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    test_channel(root, &db_path, channel, account_key)
}

pub fn sync_channel_for_business_os(root: &Path, channel: &str) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    sync_channel(root, &db_path, channel, &[])
}

/// Read the latest pairing artifact (QR-SVG + status JSON) for a channel from
/// runtime/communication/<channel>/artifacts/. Used by the Business OS UI to
/// poll the QR while WhatsApp pair_device_until_success runs in the background.
/// For channels without an artifact file we derive a sensible state from the
/// communication_accounts table.
pub fn read_pairing_state_for_business_os(root: &Path, channel: &str) -> Value {
    if channel.is_empty() {
        return json!({
            "ok": false,
            "error": "channel parameter is required",
        });
    }
    let artifacts = crate::communication::runtime::artifacts_dir_for_business_os(root, channel);
    let status_path = artifacts.join("pairing-status.json");
    let svg_path = artifacts.join("pairing-qr.svg");
    let artifact_json: Option<Value> = std::fs::read_to_string(&status_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let svg = std::fs::read_to_string(&svg_path).ok();

    // Latest account for this channel — drives both account_key and the
    // fallback status when no artifact JSON has been written.
    let db_path = resolve_db_path(root, None);
    let account_row: Option<(String, Option<String>, Option<String>)> =
        open_channel_db(&db_path).ok().and_then(|conn| {
            conn.query_row(
                "SELECT account_key, last_inbound_ok_at, last_outbound_ok_at \
                 FROM communication_accounts \
                 WHERE channel = ?1 \
                 ORDER BY updated_at DESC LIMIT 1",
                params![channel],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .ok()
        });

    let artifact_status = artifact_json
        .as_ref()
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let derived_status = match (artifact_status.as_deref(), &account_row) {
        (Some(status), _) => status.to_owned(),
        (None, Some((_, inbound, outbound))) if inbound.is_some() || outbound.is_some() => {
            "paired".to_owned()
        }
        (None, Some(_)) => "registered".to_owned(),
        (None, None) => "idle".to_owned(),
    };

    let qr_payload = artifact_json
        .as_ref()
        .and_then(|v| v.get("qr_payload"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    json!({
        "ok": true,
        "channel": channel,
        "status": derived_status,
        "qr_svg": svg,
        "qr_payload": qr_payload,
        "account_key": account_row.as_ref().map(|(key, _, _)| key.clone()),
        "last_inbound_ok_at": account_row.as_ref().and_then(|(_, ts, _)| ts.clone()),
        "last_outbound_ok_at": account_row.as_ref().and_then(|(_, _, ts)| ts.clone()),
        "artifact": artifact_json,
    })
}

/// Spawn a background thread that runs the channel sync (which includes pair_device
/// for WhatsApp). The thread writes pairing artifacts to disk as it progresses; the
/// UI polls read_pairing_state_for_business_os to render them.
pub fn start_pairing_for_business_os(root: &Path, channel: &str) -> Result<Value> {
    let supported = communication_adapters::external_adapter_for_channel(channel).is_some();
    if !supported {
        anyhow::bail!("unsupported channel for pairing: {channel}");
    }
    let root_owned = root.to_path_buf();
    let channel_owned = channel.to_string();
    std::thread::spawn(move || {
        let db_path = resolve_db_path(&root_owned, None);
        if let Err(error) = sync_channel(&root_owned, &db_path, &channel_owned, &[]) {
            eprintln!(
                "[business-os] channel {} background pairing failed: {:#}",
                channel_owned, error
            );
        }
    });
    Ok(json!({
        "ok": true,
        "channel": channel,
        "started": true,
    }))
}

/// Stub for the Jami account-archive export. The Jami daemon supports
/// `Account.exportToFile(path)` via DBus, but that integration is not yet wired
/// in CTOX-Core. The route returns a clear `not_implemented` body so the UI
/// surfaces the gap honestly instead of pretending the export ran.
pub fn export_jami_archive_for_business_os(root: &Path) -> Value {
    let archive_dir = crate::communication::runtime::artifacts_dir_for_business_os(root, "jami");
    json!({
        "ok": false,
        "error": "not_implemented",
        "message": "Jami account-key export needs the Jami daemon DBus call Account.exportToFile, which is not yet wired in CTOX-Core. Message archives live in this directory.",
        "archive_path": archive_dir.parent().map(|p| p.join("archive").display().to_string()),
    })
}

/// Persist channel-specific settings (Email IMAP/SMTP/Graph/EWS/ActiveSync,
/// Teams tenant/client, Jami account id, …) into the operator env map and then re-run
/// sync_prompt_identity so communication_accounts is updated immediately.
pub fn save_channel_settings_for_business_os(
    root: &Path,
    channel: &str,
    config: &Value,
) -> Result<Value> {
    let mut env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| BTreeMap::new());
    let str_field = |key: &str| -> Option<String> {
        config
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    };
    let num_field = |key: &str| -> Option<String> {
        config
            .get(key)
            .and_then(Value::as_i64)
            .map(|value| value.to_string())
    };
    match channel {
        "email" => {
            if let Some(value) = str_field("address") {
                env_map.insert("CTO_EMAIL_ADDRESS".to_owned(), value);
            }
            if let Some(value) = str_field("provider") {
                env_map.insert("CTO_EMAIL_PROVIDER".to_owned(), value);
            }
            if let Some(value) = str_field("password") {
                // save_runtime_env_map routes CTO_EMAIL_PASSWORD through the
                // encrypted secret store; it must never land in runtime_env_kv.
                env_map.insert("CTO_EMAIL_PASSWORD".to_owned(), value);
            }
            if let Some(value) = str_field("imap_host") {
                env_map.insert("CTO_EMAIL_IMAP_HOST".to_owned(), value);
            }
            if let Some(value) = num_field("imap_port") {
                env_map.insert("CTO_EMAIL_IMAP_PORT".to_owned(), value);
            }
            if let Some(value) = str_field("smtp_host") {
                env_map.insert("CTO_EMAIL_SMTP_HOST".to_owned(), value);
            }
            if let Some(value) = num_field("smtp_port") {
                env_map.insert("CTO_EMAIL_SMTP_PORT".to_owned(), value);
            }
            if let Some(value) = str_field("tenant_id") {
                env_map.insert("CTO_EMAIL_GRAPH_TENANT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("client_id") {
                env_map.insert("CTO_EMAIL_GRAPH_CLIENT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("graph_user") {
                env_map.insert("CTO_EMAIL_GRAPH_USER".to_owned(), value);
            }
            if let Some(value) = str_field("ews_url") {
                env_map.insert("CTO_EMAIL_EWS_URL".to_owned(), value);
            }
            if let Some(value) = str_field("owa_url") {
                env_map.insert("CTO_EMAIL_OWA_URL".to_owned(), value);
            }
            if let Some(value) = str_field("ews_auth_type") {
                env_map.insert("CTO_EMAIL_EWS_AUTH_TYPE".to_owned(), value);
            }
            if let Some(value) = str_field("ews_username") {
                env_map.insert("CTO_EMAIL_EWS_USERNAME".to_owned(), value);
            }
            if let Some(value) = str_field("ews_version") {
                env_map.insert("CTO_EMAIL_EWS_VERSION".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_server") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_SERVER".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_username") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_USERNAME".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_path") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_PATH".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_device_id") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_DEVICE_ID".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_device_type") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_protocol_version") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION".to_owned(), value);
            }
            if let Some(value) = str_field("active_sync_policy_key") {
                env_map.insert("CTO_EMAIL_ACTIVESYNC_POLICY_KEY".to_owned(), value);
            }
        }
        "teams" => {
            if let Some(value) = str_field("tenant_id") {
                env_map.insert("CTO_TEAMS_TENANT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("client_id") {
                env_map.insert("CTO_TEAMS_CLIENT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("client_secret") {
                env_map.insert("CTO_TEAMS_CLIENT_SECRET".to_owned(), value);
            }
            if let Some(value) = str_field("username") {
                env_map.insert("CTO_TEAMS_USERNAME".to_owned(), value);
            }
            if let Some(value) = str_field("password") {
                env_map.insert("CTO_TEAMS_PASSWORD".to_owned(), value);
            }
        }
        "slack" => {
            if let Some(value) = str_field("bot_token") {
                env_map.insert("CTO_SLACK_BOT_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("app_token") {
                env_map.insert("CTO_SLACK_APP_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("signing_secret") {
                env_map.insert("CTO_SLACK_SIGNING_SECRET".to_owned(), value);
            }
            if let Some(value) = str_field("workspace_id") {
                env_map.insert("CTO_SLACK_WORKSPACE_ID".to_owned(), value);
            }
            if let Some(value) = str_field("bot_user_id") {
                env_map.insert("CTO_SLACK_BOT_USER_ID".to_owned(), value);
            }
            if let Some(value) = str_field("channel_id") {
                env_map.insert("CTO_SLACK_CHANNEL_ID".to_owned(), value);
            }
            if let Some(value) = str_field("channel_ids") {
                env_map.insert("CTO_SLACK_CHANNEL_IDS".to_owned(), value);
            }
            if let Some(value) = str_field("api_base_url") {
                env_map.insert("CTO_SLACK_API_BASE_URL".to_owned(), value);
            }
        }
        "discord" => {
            if let Some(value) = str_field("bot_token") {
                env_map.insert("CTO_DISCORD_BOT_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("application_id") {
                env_map.insert("CTO_DISCORD_APPLICATION_ID".to_owned(), value);
            }
            if let Some(value) = str_field("bot_user_id") {
                env_map.insert("CTO_DISCORD_BOT_USER_ID".to_owned(), value);
            }
            if let Some(value) = str_field("guild_id") {
                env_map.insert("CTO_DISCORD_GUILD_ID".to_owned(), value);
            }
            if let Some(value) = str_field("guild_ids") {
                env_map.insert("CTO_DISCORD_GUILD_IDS".to_owned(), value);
            }
            if let Some(value) = str_field("channel_id") {
                env_map.insert("CTO_DISCORD_CHANNEL_ID".to_owned(), value);
            }
            if let Some(value) = str_field("channel_ids") {
                env_map.insert("CTO_DISCORD_CHANNEL_IDS".to_owned(), value);
            }
            if let Some(value) = str_field("api_base_url") {
                env_map.insert("CTO_DISCORD_API_BASE_URL".to_owned(), value);
            }
        }
        "telegram" => {
            if let Some(value) = str_field("bot_token") {
                env_map.insert("CTO_TELEGRAM_BOT_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("bot_username") {
                env_map.insert("CTO_TELEGRAM_BOT_USERNAME".to_owned(), value);
            }
            if let Some(value) = str_field("chat_id") {
                env_map.insert("CTO_TELEGRAM_CHAT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("chat_ids") {
                env_map.insert("CTO_TELEGRAM_CHAT_IDS".to_owned(), value);
            }
            if let Some(value) = str_field("api_base_url") {
                env_map.insert("CTO_TELEGRAM_API_BASE_URL".to_owned(), value);
            }
        }
        "matrix" => {
            if let Some(value) = str_field("homeserver_url") {
                env_map.insert("CTO_MATRIX_HOMESERVER_URL".to_owned(), value);
            }
            if let Some(value) = str_field("access_token") {
                env_map.insert("CTO_MATRIX_ACCESS_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("user_id") {
                env_map.insert("CTO_MATRIX_USER_ID".to_owned(), value);
            }
            if let Some(value) = str_field("room_id") {
                env_map.insert("CTO_MATRIX_ROOM_ID".to_owned(), value);
            }
            if let Some(value) = str_field("room_ids") {
                env_map.insert("CTO_MATRIX_ROOM_IDS".to_owned(), value);
            }
        }
        "mattermost" => {
            if let Some(value) = str_field("server_url") {
                env_map.insert("CTO_MATTERMOST_SERVER_URL".to_owned(), value);
            }
            if let Some(value) = str_field("bot_token") {
                env_map.insert("CTO_MATTERMOST_BOT_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("access_token") {
                env_map.insert("CTO_MATTERMOST_ACCESS_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("bot_username") {
                env_map.insert("CTO_MATTERMOST_BOT_USERNAME".to_owned(), value);
            }
            if let Some(value) = str_field("bot_user_id") {
                env_map.insert("CTO_MATTERMOST_BOT_USER_ID".to_owned(), value);
            }
            if let Some(value) = str_field("team_id") {
                env_map.insert("CTO_MATTERMOST_TEAM_ID".to_owned(), value);
            }
            if let Some(value) = str_field("channel_id") {
                env_map.insert("CTO_MATTERMOST_CHANNEL_ID".to_owned(), value);
            }
            if let Some(value) = str_field("channel_ids") {
                env_map.insert("CTO_MATTERMOST_CHANNEL_IDS".to_owned(), value);
            }
        }
        "zulip" => {
            if let Some(value) = str_field("realm_url") {
                env_map.insert("CTO_ZULIP_REALM_URL".to_owned(), value);
            }
            if let Some(value) = str_field("bot_email") {
                env_map.insert("CTO_ZULIP_BOT_EMAIL".to_owned(), value);
            }
            if let Some(value) = str_field("email") {
                env_map.insert("CTO_ZULIP_EMAIL".to_owned(), value);
            }
            if let Some(value) = str_field("api_key") {
                env_map.insert("CTO_ZULIP_API_KEY".to_owned(), value);
            }
            if let Some(value) = str_field("stream") {
                env_map.insert("CTO_ZULIP_STREAM".to_owned(), value);
            }
            if let Some(value) = str_field("streams") {
                env_map.insert("CTO_ZULIP_STREAMS".to_owned(), value);
            }
            if let Some(value) = str_field("topic") {
                env_map.insert("CTO_ZULIP_TOPIC".to_owned(), value);
            }
        }
        "google_chat" => {
            if let Some(value) = str_field("access_token") {
                env_map.insert("CTO_GOOGLE_CHAT_ACCESS_TOKEN".to_owned(), value);
            }
            if let Some(value) = str_field("user") {
                env_map.insert("CTO_GOOGLE_CHAT_USER".to_owned(), value);
            }
            if let Some(value) = str_field("app_id") {
                env_map.insert("CTO_GOOGLE_CHAT_APP_ID".to_owned(), value);
            }
            if let Some(value) = str_field("space_name") {
                env_map.insert("CTO_GOOGLE_CHAT_SPACE_NAME".to_owned(), value);
            }
            if let Some(value) = str_field("space_names") {
                env_map.insert("CTO_GOOGLE_CHAT_SPACE_NAMES".to_owned(), value);
            }
            if let Some(value) = str_field("api_base_url") {
                env_map.insert("CTO_GOOGLE_CHAT_API_BASE_URL".to_owned(), value);
            }
        }
        "jami" => {
            if let Some(value) = str_field("account_id") {
                env_map.insert("CTO_JAMI_ACCOUNT_ID".to_owned(), value);
            }
            if let Some(value) = str_field("profile_name") {
                env_map.insert("CTO_JAMI_PROFILE_NAME".to_owned(), value);
            }
        }
        "whatsapp" => {
            // WhatsApp has no static credentials; pairing produces a device-bound
            // session under runtime/communication/whatsapp/. Nothing to persist here.
        }
        _ => anyhow::bail!("unsupported channel for settings: {channel}"),
    }
    crate::inference::runtime_env::save_runtime_env_map(root, &env_map)?;
    sync_prompt_identity(root, &env_map)?;
    Ok(json!({ "ok": true, "channel": channel }))
}

/// RxDB-shaped projection of communication_accounts for the Business OS pull
/// bridge. The bridge polls /api/business-os/rxdb/pull?collection=communication_accounts
/// and feeds results into the browser-side RxDB collection of the Conversations
/// audit module.
pub(super) fn communication_account_business_os_document(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Value> {
    let account_key: String = row.get(0)?;
    let channel: String = row.get(1)?;
    let address: String = row.get(2)?;
    let provider: String = row.get(3)?;
    let profile_raw: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    let last_inbound_ok_at: Option<String> = row.get(7)?;
    let last_outbound_ok_at: Option<String> = row.get(8)?;
    let updated_at_ms: Option<i64> = row.get(9)?;
    Ok(json!({
        "id": account_key,
        "account_key": account_key,
        "channel": channel,
        "address": address,
        "provider": provider,
        "profile_json": serde_json::from_str::<Value>(&profile_raw).unwrap_or_else(|_| json!({})),
        "created_at": created_at,
        "updated_at": updated_at,
        "last_inbound_ok_at": last_inbound_ok_at,
        "last_outbound_ok_at": last_outbound_ok_at,
        "updated_at_ms": updated_at_ms.unwrap_or(0),
        "_deleted": false,
    }))
}

pub fn pull_communication_accounts_for_business_os(
    root: &Path,
    since_ms: Option<i64>,
    limit: Option<usize>,
) -> Result<Value> {
    pull_communication_accounts_for_business_os_after(root, since_ms, None, limit)
}

pub fn pull_communication_accounts_for_business_os_after(
    root: &Path,
    since_ms: Option<i64>,
    after_record_id: Option<&str>,
    limit: Option<usize>,
) -> Result<Value> {
    let since_ms = since_ms.unwrap_or(0).max(0);
    let after_record_id = after_record_id.unwrap_or("");
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let db_path = resolve_db_path(root, None);
    let Some(conn) = open_channel_db_read_only(&db_path)? else {
        return Ok(empty_business_os_projection("communication_accounts"));
    };
    if !channel_projection_tables_exist(&conn, &["communication_accounts"])? {
        return Ok(empty_business_os_projection("communication_accounts"));
    }
    let mut stmt = conn.prepare(
        r#"
        SELECT *
        FROM (
            SELECT
                account_key, channel, address, provider, profile_json,
                created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at,
                CAST(strftime('%s', COALESCE(updated_at, created_at)) AS INTEGER) * 1000 AS updated_at_ms
            FROM communication_accounts
        )
        WHERE COALESCE(updated_at_ms, 0) > ?1
           OR (COALESCE(updated_at_ms, 0) = ?1 AND account_key > ?2)
        ORDER BY updated_at_ms ASC, account_key ASC
        LIMIT ?3
        "#,
    )?;
    let documents = stmt
        .query_map(params![since_ms, after_record_id, limit as i64], |row| {
            communication_account_business_os_document(row)
        })?
        .collect::<rusqlite::Result<Vec<Value>>>()?;
    let count = documents.len();
    Ok(json!({
        "ok": true,
        "collection": "communication_accounts",
        "documents": documents,
        "count": count,
        "since_ms": since_ms,
        "after_record_id": after_record_id,
    }))
}

/// RxDB-shaped projection of communication_threads.
pub(super) fn communication_thread_business_os_document(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Value> {
    let thread_key: String = row.get(0)?;
    let channel: String = row.get(1)?;
    let account_key: String = row.get(2)?;
    let subject: String = row.get(3)?;
    let participants_raw: String = row.get(4)?;
    let last_message_key: String = row.get(5)?;
    let last_message_at: String = row.get(6)?;
    let message_count: i64 = row.get(7)?;
    let unread_count: i64 = row.get(8)?;
    let metadata_raw: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    let updated_at_ms: Option<i64> = row.get(11)?;
    Ok(json!({
        "id": thread_key,
        "thread_key": thread_key,
        "channel": channel,
        "account_key": account_key,
        "subject": subject,
        "participant_keys_json": serde_json::from_str::<Value>(&participants_raw).unwrap_or_else(|_| json!([])),
        "last_message_key": last_message_key,
        "last_message_at": last_message_at,
        "message_count": message_count,
        "unread_count": unread_count,
        "metadata_json": serde_json::from_str::<Value>(&metadata_raw).unwrap_or_else(|_| json!({})),
        "updated_at": updated_at,
        "updated_at_ms": updated_at_ms.unwrap_or(0),
        "_deleted": false,
    }))
}

pub fn pull_communication_threads_for_business_os(
    root: &Path,
    since_ms: Option<i64>,
    limit: Option<usize>,
) -> Result<Value> {
    pull_communication_threads_for_business_os_after(root, since_ms, None, limit)
}

pub fn pull_communication_threads_for_business_os_after(
    root: &Path,
    since_ms: Option<i64>,
    after_record_id: Option<&str>,
    limit: Option<usize>,
) -> Result<Value> {
    let since_ms = since_ms.unwrap_or(0).max(0);
    let after_record_id = after_record_id.unwrap_or("");
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let db_path = resolve_db_path(root, None);
    let Some(conn) = open_channel_db_read_only(&db_path)? else {
        return Ok(empty_business_os_projection("communication_threads"));
    };
    if !channel_projection_tables_exist(&conn, &["communication_threads"])? {
        return Ok(empty_business_os_projection("communication_threads"));
    }
    let mut stmt = conn.prepare(
        r#"
        SELECT *
        FROM (
            SELECT
                thread_key, channel, account_key, subject,
                participant_keys_json, last_message_key, last_message_at,
                message_count, unread_count, metadata_json, updated_at,
                CAST(strftime('%s', COALESCE(updated_at, last_message_at)) AS INTEGER) * 1000 AS updated_at_ms
            FROM communication_threads
        )
        WHERE COALESCE(updated_at_ms, 0) > ?1
           OR (COALESCE(updated_at_ms, 0) = ?1 AND thread_key > ?2)
        ORDER BY updated_at_ms ASC, thread_key ASC
        LIMIT ?3
        "#,
    )?;
    let documents = stmt
        .query_map(params![since_ms, after_record_id, limit as i64], |row| {
            communication_thread_business_os_document(row)
        })?
        .collect::<rusqlite::Result<Vec<Value>>>()?;
    let count = documents.len();
    Ok(json!({
        "ok": true,
        "collection": "communication_threads",
        "documents": documents,
        "count": count,
        "since_ms": since_ms,
        "after_record_id": after_record_id,
    }))
}

/// RxDB-shaped projection of communication_messages. Joins routing_state for
/// `route_status` and extracts `ticket_self_work_id` from metadata_json so the
/// Conversations audit module can show task/work IDs and link to the harness
/// flowview without extra round-trips.
pub(super) fn communication_message_business_os_document(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Value> {
    let message_key: String = row.get(0)?;
    let channel: String = row.get(1)?;
    let account_key: String = row.get(2)?;
    let thread_key: String = row.get(3)?;
    let remote_id: String = row.get(4)?;
    let direction: String = row.get(5)?;
    let folder_hint: String = row.get(6)?;
    let sender_display: String = row.get(7)?;
    let sender_address: String = row.get(8)?;
    let recipients_raw: String = row.get(9)?;
    let cc_raw: String = row.get(10)?;
    let bcc_raw: String = row.get(11)?;
    let subject: String = row.get(12)?;
    let preview: String = row.get(13)?;
    let body_text: String = row.get(14)?;
    let body_html: String = row.get(15)?;
    let raw_payload_ref: String = row.get(16)?;
    let trust_level: String = row.get(17)?;
    let status: String = row.get(18)?;
    let seen: i64 = row.get(19)?;
    let has_attachments: i64 = row.get(20)?;
    let external_created_at: String = row.get(21)?;
    let observed_at: String = row.get(22)?;
    let metadata_raw: String = row.get(23)?;
    let route_status: Option<String> = row.get(24)?;
    let updated_at_ms: Option<i64> = row.get(25)?;
    let metadata: Value =
        serde_json::from_str::<Value>(&metadata_raw).unwrap_or_else(|_| json!({}));
    let ticket_self_work_id = metadata
        .get("ticket_self_work_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let work_id = metadata
        .get("work_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(json!({
        "id": message_key,
        "message_key": message_key,
        "channel": channel,
        "account_key": account_key,
        "thread_key": thread_key,
        "remote_id": remote_id,
        "direction": direction,
        "folder_hint": folder_hint,
        "sender_display": sender_display,
        "sender_address": sender_address,
        "recipient_addresses_json": serde_json::from_str::<Value>(&recipients_raw).unwrap_or_else(|_| json!([])),
        "cc_addresses_json": serde_json::from_str::<Value>(&cc_raw).unwrap_or_else(|_| json!([])),
        "bcc_addresses_json": serde_json::from_str::<Value>(&bcc_raw).unwrap_or_else(|_| json!([])),
        "subject": subject,
        "preview": preview,
        "body_text": body_text,
        "body_html": body_html,
        "raw_payload_ref": raw_payload_ref,
        "trust_level": trust_level,
        "status": status,
        "seen": seen,
        "has_attachments": has_attachments,
        "external_created_at": external_created_at,
        "observed_at": observed_at,
        "metadata_json": metadata,
        "route_status": route_status,
        "ticket_self_work_id": ticket_self_work_id,
        "work_id": work_id,
        "updated_at_ms": updated_at_ms.unwrap_or(0),
        "_deleted": false,
    }))
}

pub fn pull_communication_messages_for_business_os(
    root: &Path,
    since_ms: Option<i64>,
    limit: Option<usize>,
) -> Result<Value> {
    pull_communication_messages_for_business_os_after(root, since_ms, None, limit)
}

pub fn pull_communication_messages_for_business_os_after(
    root: &Path,
    since_ms: Option<i64>,
    after_record_id: Option<&str>,
    limit: Option<usize>,
) -> Result<Value> {
    let since_ms = since_ms.unwrap_or(0).max(0);
    let after_record_id = after_record_id.unwrap_or("");
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let db_path = resolve_db_path(root, None);
    let Some(conn) = open_channel_db_read_only(&db_path)? else {
        return Ok(empty_business_os_projection("communication_messages"));
    };
    if !channel_projection_tables_exist(
        &conn,
        &["communication_messages", "communication_routing_state"],
    )? {
        return Ok(empty_business_os_projection("communication_messages"));
    }
    let mut stmt = conn.prepare(
        r#"
        SELECT *
        FROM (
            SELECT
                m.message_key, m.channel, m.account_key, m.thread_key, m.remote_id,
                m.direction, m.folder_hint, m.sender_display, m.sender_address,
                m.recipient_addresses_json, m.cc_addresses_json, m.bcc_addresses_json,
                m.subject, m.preview, m.body_text, m.body_html, m.raw_payload_ref,
                m.trust_level, m.status, m.seen, m.has_attachments,
                m.external_created_at, m.observed_at, m.metadata_json,
                r.route_status,
                CAST(strftime('%s', COALESCE(m.observed_at, m.external_created_at)) AS INTEGER) * 1000 AS updated_at_ms
            FROM communication_messages m
            LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        )
        WHERE COALESCE(updated_at_ms, 0) > ?1
           OR (COALESCE(updated_at_ms, 0) = ?1 AND message_key > ?2)
        ORDER BY updated_at_ms ASC, message_key ASC
        LIMIT ?3
        "#,
    )?;
    let documents = stmt
        .query_map(params![since_ms, after_record_id, limit as i64], |row| {
            communication_message_business_os_document(row)
        })?
        .collect::<rusqlite::Result<Vec<Value>>>()?;
    let count = documents.len();
    Ok(json!({
        "ok": true,
        "collection": "communication_messages",
        "documents": documents,
        "count": count,
        "since_ms": since_ms,
        "after_record_id": after_record_id,
    }))
}

pub fn pull_communication_record_for_business_os(
    root: &Path,
    collection: &str,
    record_id: &str,
) -> Result<Option<Value>> {
    let record_id = record_id.trim();
    if record_id.is_empty() {
        return Ok(None);
    }
    let db_path = resolve_db_path(root, None);
    let Some(conn) = open_channel_db_read_only(&db_path)? else {
        return Ok(None);
    };
    match collection {
        "communication_accounts" => {
            if !channel_projection_tables_exist(&conn, &["communication_accounts"])? {
                return Ok(None);
            }
            conn.query_row(
                r#"
                SELECT
                    account_key, channel, address, provider, profile_json,
                    created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at,
                    CAST(strftime('%s', COALESCE(updated_at, created_at)) AS INTEGER) * 1000 AS updated_at_ms
                FROM communication_accounts
                WHERE account_key = ?1
                "#,
                [record_id],
                communication_account_business_os_document,
            )
            .optional()
            .map_err(Into::into)
        }
        "communication_threads" => {
            if !channel_projection_tables_exist(&conn, &["communication_threads"])? {
                return Ok(None);
            }
            conn.query_row(
                r#"
                SELECT
                    thread_key, channel, account_key, subject,
                    participant_keys_json, last_message_key, last_message_at,
                    message_count, unread_count, metadata_json, updated_at,
                    CAST(strftime('%s', COALESCE(updated_at, last_message_at)) AS INTEGER) * 1000 AS updated_at_ms
                FROM communication_threads
                WHERE thread_key = ?1
                "#,
                [record_id],
                communication_thread_business_os_document,
            )
            .optional()
            .map_err(Into::into)
        }
        "communication_messages" => {
            if !channel_projection_tables_exist(
                &conn,
                &["communication_messages", "communication_routing_state"],
            )? {
                return Ok(None);
            }
            conn.query_row(
                r#"
                SELECT
                    m.message_key, m.channel, m.account_key, m.thread_key, m.remote_id,
                    m.direction, m.folder_hint, m.sender_display, m.sender_address,
                    m.recipient_addresses_json, m.cc_addresses_json, m.bcc_addresses_json,
                    m.subject, m.preview, m.body_text, m.body_html, m.raw_payload_ref,
                    m.trust_level, m.status, m.seen, m.has_attachments,
                    m.external_created_at, m.observed_at, m.metadata_json,
                    r.route_status,
                    CAST(strftime('%s', COALESCE(m.observed_at, m.external_created_at)) AS INTEGER) * 1000 AS updated_at_ms
                FROM communication_messages m
                LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
                WHERE m.message_key = ?1
                "#,
                [record_id],
                communication_message_business_os_document,
            )
            .optional()
            .map_err(Into::into)
        }
        _ => Ok(None),
    }
}

pub(crate) fn communication_intake_source_stamp(
    root: &Path,
) -> Result<CommunicationIntakeSourceStamp> {
    let db_path = resolve_db_path(root, None);
    if !db_path.exists() {
        return Ok(empty_communication_intake_source_stamp(false));
    }
    super::outbound_review::with_cached_channel_db_read_only(&db_path, |conn| {
        let Some(conn) = conn else {
            return Ok(empty_communication_intake_source_stamp(false));
        };
        let mut accounts_table_exists =
            channel_projection_tables_exist(conn, &["communication_accounts"])?;
        let mut threads_table_exists =
            channel_projection_tables_exist(conn, &["communication_threads"])?;
        let mut messages_table_exists =
            channel_projection_tables_exist(conn, &["communication_messages"])?;
        let mut routing_table_exists =
            channel_projection_tables_exist(conn, &["communication_routing_state"])?;
        let clock_table_exists =
            channel_projection_tables_exist(conn, &["communication_projection_clock"])?;

        if !clock_table_exists {
            let schema_conn = open_channel_db(&db_path)?;
            drop(schema_conn);
            accounts_table_exists =
                channel_projection_tables_exist(conn, &["communication_accounts"])?;
            threads_table_exists =
                channel_projection_tables_exist(conn, &["communication_threads"])?;
            messages_table_exists =
                channel_projection_tables_exist(conn, &["communication_messages"])?;
            routing_table_exists =
                channel_projection_tables_exist(conn, &["communication_routing_state"])?;
        }

        let (
            projection_version,
            account_count,
            thread_count,
            message_count,
            routing_count,
            clock_updated_at,
        ) = conn
            .query_row(COMMUNICATION_INTAKE_SOURCE_STAMP_SQL, [], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .optional()?
            .unwrap_or_else(|| (0, 0, 0, 0, 0, String::new()));

        let earliest_pending_retry_not_before = if routing_table_exists {
            conn.query_row(
                r#"
                SELECT MIN(retry_not_before)
                FROM communication_routing_state
                WHERE lower(route_status) = 'pending'
                  AND retry_not_before IS NOT NULL
                  AND TRIM(retry_not_before) <> ''
                "#,
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .unwrap_or_default()
        } else {
            String::new()
        };

        Ok(CommunicationIntakeSourceStamp {
            database_exists: true,
            accounts_table_exists,
            threads_table_exists,
            messages_table_exists,
            routing_table_exists,
            projection_version,
            account_count: non_negative_i64_to_usize(account_count),
            latest_account_updated_at_ms: 0,
            thread_count: non_negative_i64_to_usize(thread_count),
            latest_thread_updated_at_ms: 0,
            message_count: non_negative_i64_to_usize(message_count),
            latest_message_updated_at_ms: 0,
            routing_count: non_negative_i64_to_usize(routing_count),
            clock_updated_at: clock_updated_at.clone(),
            content_hash: format!(
                "communication-projection-clock:{projection_version}:{clock_updated_at}"
            ),
            earliest_pending_retry_not_before,
        })
    })
}

pub(super) fn empty_communication_intake_source_stamp(
    database_exists: bool,
) -> CommunicationIntakeSourceStamp {
    CommunicationIntakeSourceStamp {
        database_exists,
        accounts_table_exists: false,
        threads_table_exists: false,
        messages_table_exists: false,
        routing_table_exists: false,
        projection_version: 0,
        account_count: 0,
        latest_account_updated_at_ms: 0,
        thread_count: 0,
        latest_thread_updated_at_ms: 0,
        message_count: 0,
        latest_message_updated_at_ms: 0,
        routing_count: 0,
        clock_updated_at: String::new(),
        content_hash: String::new(),
        earliest_pending_retry_not_before: String::new(),
    }
}

pub(super) fn non_negative_i64_to_usize(value: i64) -> usize {
    value.max(0) as usize
}

#[cfg(test)]
mod communication_intake_cache_tests {
    use super::*;

    #[test]
    fn intake_stamp_reuses_reader_and_observes_projection_commits() {
        let root = tempfile::tempdir().expect("create intake cache test root");
        let db_path = resolve_db_path(root.path(), None);
        let conn = open_channel_db(&db_path).expect("create channel schema");
        super::super::outbound_review::reset_channel_db_read_only_cache_for_tests();

        let first = communication_intake_source_stamp(root.path()).expect("read first stamp");
        let repeated = communication_intake_source_stamp(root.path()).expect("repeat stamp");
        assert_eq!(repeated, first);
        assert_eq!(
            super::super::outbound_review::channel_db_read_only_open_count_for_tests(),
            1
        );

        conn.execute(
            "UPDATE communication_projection_clock SET version = version + 1, updated_at = '2026-08-17T02:00:00Z' WHERE id = 1",
            [],
        )
        .expect("advance projection clock");
        let advanced = communication_intake_source_stamp(root.path()).expect("read advanced stamp");
        assert_ne!(advanced, first);
        assert_eq!(
            super::super::outbound_review::channel_db_read_only_open_count_for_tests(),
            1
        );
        super::super::outbound_review::reset_channel_db_read_only_cache_for_tests();
    }
}
