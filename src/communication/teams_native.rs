use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use url::Url;

use crate::communication::adapters::{
    AdapterSyncCommandRequest, TeamsSendCommandRequest, TeamsTestCommandRequest,
};
use crate::communication::email_native as communication_email_native;
use crate::communication::microsoft_graph_auth::{
    acquire_app_token, acquire_ropc_token, ROPC_PUBLIC_CLIENT_ID,
};
use crate::mission::channels::{
    ensure_account, ensure_routing_rows_for_inbound, now_iso_string, open_channel_db, preview_text,
    record_communication_sync_run, refresh_thread, stable_digest, upsert_communication_message,
    CommunicationSyncRun, UpsertMessage,
};

const GRAPH_DEFAULT_BASE_URL: &str = "https://graph.microsoft.com/v1.0";
const TEAMS_SYNC_LIMIT: usize = 50;

#[derive(Clone, Debug)]
struct TeamsOptions {
    db_path: std::path::PathBuf,
    graph_access_token: String,
    tenant_id: String,
    client_id: String,
    client_secret: String,
    username: String,
    password: String,
    bot_id: String,
    graph_base_url: String,
    team_id: String,
    channel_id: String,
    chat_id: String,
    limit: usize,
    discovery_limit: usize,
}

#[derive(Clone, Debug)]
struct TeamsInboundMessage {
    account_key: String,
    thread_key: String,
    message_key: String,
    remote_id: String,
    sender_display: String,
    sender_address: String,
    recipients: Vec<String>,
    subject: String,
    body_text: String,
    preview: String,
    seen: bool,
    has_attachments: bool,
    external_created_at: String,
    metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TeamsSendDestination {
    Chat {
        chat_id: String,
    },
    Channel {
        team_id: String,
        channel_id: String,
        parent_message_id: Option<String>,
    },
}

struct GraphTeamsClient {
    access_token: String,
    base_url: String,
}

#[derive(Clone, Debug, Default)]
struct TeamsSelfIdentity {
    user_id: String,
    user_principal_name: String,
    mail: String,
    display_name: String,
}

impl GraphTeamsClient {
    fn from_options(options: &TeamsOptions) -> Result<Self> {
        if !options.graph_access_token.trim().is_empty() {
            return Ok(Self {
                access_token: options.graph_access_token.trim().to_string(),
                base_url: options.graph_base_url.trim_end_matches('/').to_string(),
            });
        }
        let has_user_creds = !options.username.is_empty() && !options.password.is_empty();
        let has_client_creds = !options.client_id.is_empty() && !options.client_secret.is_empty();
        let access_token = if has_user_creds {
            acquire_ropc_token(
                &options.tenant_id,
                &options.username,
                &options.password,
                if options.client_id.is_empty() {
                    ROPC_PUBLIC_CLIENT_ID
                } else {
                    &options.client_id
                },
            )?
        } else if has_client_creds {
            acquire_app_token(
                &options.tenant_id,
                &options.client_id,
                &options.client_secret,
            )?
        } else {
            bail!("Teams requires either username+password or client_id+client_secret");
        };
        Ok(Self {
            access_token,
            base_url: options.graph_base_url.trim_end_matches('/').to_string(),
        })
    }

    fn headers(&self) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        headers.insert(
            "authorization".to_string(),
            format!("Bearer {}", self.access_token),
        );
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let mut url = Url::parse(&(self.base_url.clone() + path))
            .with_context(|| format!("invalid Graph url: {}{}", self.base_url, path))?;
        for (key, value) in query {
            if !value.is_empty() {
                url.query_pairs_mut().append_pair(key, value);
            }
        }
        let payload = body.map(serde_json::to_vec).transpose()?;
        let response = communication_email_native::http_request(
            method,
            url.as_str(),
            &self.headers(),
            payload.as_deref(),
        )?;
        let value = if response.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice::<Value>(&response.body).unwrap_or_else(|_| {
                Value::String(String::from_utf8_lossy(&response.body).into_owned())
            })
        };
        if !(200..300).contains(&response.status) {
            bail!("Graph HTTP {}: {value}", response.status);
        }
        Ok(value)
    }

    fn list_channel_messages(
        &self,
        team_id: &str,
        channel_id: &str,
        top: usize,
    ) -> Result<Vec<Value>> {
        let value = self.request(
            "GET",
            &format!("/teams/{team_id}/channels/{channel_id}/messages"),
            &[("$top", top.to_string())],
            None,
        )?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn list_channel_message_replies(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        top: usize,
    ) -> Result<Vec<Value>> {
        let value = self.request(
            "GET",
            &format!("/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies"),
            &[("$top", top.to_string())],
            None,
        )?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn list_chat_messages(&self, chat_id: &str, top: usize) -> Result<Vec<Value>> {
        let value = self.request(
            "GET",
            &format!("/chats/{chat_id}/messages"),
            &[("$top", top.to_string())],
            None,
        )?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn list_chats(&self, top: usize) -> Result<Vec<Value>> {
        let value = self.request("GET", "/me/chats", &[("$top", top.to_string())], None)?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn list_joined_teams(&self, top: usize) -> Result<Vec<Value>> {
        let value = self.request("GET", "/me/joinedTeams", &[], None)?;
        let mut teams = value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        teams.truncate(top);
        Ok(teams)
    }

    fn list_team_channels(&self, team_id: &str, top: usize) -> Result<Vec<Value>> {
        let value = self.request("GET", &format!("/teams/{team_id}/channels"), &[], None)?;
        let mut channels = value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        channels.truncate(top);
        Ok(channels)
    }

    fn get_self_identity(&self) -> Result<TeamsSelfIdentity> {
        let value = self.request(
            "GET",
            "/me",
            &[(
                "$select",
                "id,displayName,userPrincipalName,mail".to_string(),
            )],
            None,
        )?;
        Ok(TeamsSelfIdentity {
            user_id: value
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            user_principal_name: value
                .get("userPrincipalName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            mail: value
                .get("mail")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            display_name: value
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    fn send_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        body_content: &str,
    ) -> Result<Value> {
        let payload = json!({
            "body": {
                "contentType": "text",
                "content": body_content,
            },
        });
        self.request(
            "POST",
            &format!("/teams/{team_id}/channels/{channel_id}/messages"),
            &[],
            Some(&payload),
        )
    }

    fn send_channel_reply(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        body_content: &str,
    ) -> Result<Value> {
        let payload = json!({
            "body": {
                "contentType": "text",
                "content": body_content,
            },
        });
        self.request(
            "POST",
            &format!("/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies"),
            &[],
            Some(&payload),
        )
    }

    fn send_chat_message(&self, chat_id: &str, body_content: &str) -> Result<Value> {
        let payload = json!({
            "body": {
                "contentType": "text",
                "content": body_content,
            },
        });
        self.request(
            "POST",
            &format!("/chats/{chat_id}/messages"),
            &[],
            Some(&payload),
        )
    }

    fn get_app_info(&self) -> Result<Value> {
        self.request("GET", "/organization", &[], None)
    }
}

pub(crate) fn sync(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<Value> {
    let options = sync_options_from_args(root, runtime, request)?;
    execute_sync(&options)
}

pub(crate) fn send(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &TeamsSendCommandRequest<'_>,
) -> Result<Value> {
    let options = send_options_from_request(root, runtime, request);
    execute_send(&options, request)
}

pub(crate) fn test(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &TeamsTestCommandRequest<'_>,
) -> Result<Value> {
    let options = test_options_from_request(root, runtime, request);
    execute_test(&options)
}

pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let preferred_is_teams = settings
        .get("CTOX_OWNER_PREFERRED_CHANNEL")
        .map(|value| value.trim())
        == Some("teams");
    let runtime = runtime_from_settings(root, settings);
    let db_path = root.join("runtime/ctox.sqlite3");
    let options = base_options_from_runtime(root, &runtime, &db_path);
    if !preferred_is_teams && !has_any_auth(&options) {
        return Ok(None);
    }
    let request = AdapterSyncCommandRequest {
        db_path: db_path.as_path(),
        passthrough_args: &["sync".to_string()],
        skip_flags: &["--db", "--channel"],
    };
    sync(root, &runtime, &request).map(Some)
}

fn has_any_auth(options: &TeamsOptions) -> bool {
    if !options.graph_access_token.trim().is_empty() {
        return true;
    }
    let has_user = !options.username.is_empty() && !options.password.is_empty();
    let has_client = !options.client_id.is_empty() && !options.client_secret.is_empty();
    has_user || has_client
}

fn execute_sync(options: &TeamsOptions) -> Result<Value> {
    if !has_any_auth(options) {
        bail!(
            "Teams sync requires CTO_TEAMS_GRAPH_ACCESS_TOKEN, CTO_TEAMS_USERNAME + CTO_TEAMS_PASSWORD, CTO_TEAMS_CLIENT_ID + CTO_TEAMS_CLIENT_SECRET, or reusable CTO_EMAIL Graph credentials"
        );
    }
    let client = GraphTeamsClient::from_options(options)?;
    let self_identity = client.get_self_identity().unwrap_or_default();
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_for_teams(options);
    ensure_account(
        &mut conn,
        &account_key,
        "teams",
        &options.bot_id,
        "microsoft-graph",
        build_profile_json(options),
    )?;
    let sync_start = now_iso_string();
    let mut total_synced: usize = 0;
    let mut errors: Vec<String> = Vec::new();

    // Sync channel messages if team_id and channel_id are configured
    if !options.team_id.is_empty() && !options.channel_id.is_empty() {
        match sync_channel_messages(&client, &mut conn, options, &account_key, &self_identity) {
            Ok(count) => total_synced += count,
            Err(error) => errors.push(format!("channel sync: {error}")),
        }
    }

    // Sync 1:1 chat messages if chat_id is configured
    if !options.chat_id.is_empty() {
        match sync_chat_messages(&client, &mut conn, options, &account_key, &self_identity) {
            Ok(count) => total_synced += count,
            Err(error) => errors.push(format!("chat sync: {error}")),
        }
    }

    if options.team_id.is_empty() && options.channel_id.is_empty() && options.chat_id.is_empty() {
        match sync_discovered_chats(&client, &mut conn, options, &account_key, &self_identity) {
            Ok(count) => total_synced += count,
            Err(error) => errors.push(format!("chat discovery sync: {error}")),
        }
        match sync_discovered_channels(&client, &mut conn, options, &account_key, &self_identity) {
            Ok(count) => total_synced += count,
            Err(error) => errors.push(format!("channel discovery sync: {error}")),
        }
    }

    ensure_routing_rows_for_inbound(&conn)?;
    let finished_at = now_iso_string();
    let error_text = errors.join("; ");
    let run_key = stable_digest(&format!("teams:{}:{sync_start}", account_key));
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &run_key,
            channel: "teams",
            account_key: &account_key,
            folder_hint: "INBOX",
            started_at: &sync_start,
            finished_at: &finished_at,
            ok: errors.is_empty(),
            fetched_count: total_synced as i64,
            stored_count: total_synced as i64,
            error_text: &error_text,
            metadata_json: "{}",
        },
    )?;
    Ok(json!({
        "ok": errors.is_empty(),
        "adapter": "teams",
        "account_key": account_key,
        "messages_synced": total_synced,
        "errors": errors,
    }))
}

fn sync_channel_messages(
    client: &GraphTeamsClient,
    conn: &mut rusqlite::Connection,
    options: &TeamsOptions,
    account_key: &str,
    self_identity: &TeamsSelfIdentity,
) -> Result<usize> {
    let messages =
        client.list_channel_messages(&options.team_id, &options.channel_id, options.limit)?;
    let mut count = 0;
    for msg in &messages {
        if let Some(inbound) = normalize_teams_message_for_sync(
            msg,
            account_key,
            &options.team_id,
            &options.channel_id,
            self_identity,
            options,
        ) {
            store_teams_message(conn, &inbound)?;
            count += 1;
        }
        // Also sync replies to each top-level message
        let msg_id = msg.get("id").and_then(Value::as_str).unwrap_or_default();
        if !msg_id.is_empty() {
            if let Ok(replies) = client.list_channel_message_replies(
                &options.team_id,
                &options.channel_id,
                msg_id,
                options.limit,
            ) {
                for reply in &replies {
                    if let Some(inbound) = normalize_teams_message_for_sync(
                        reply,
                        account_key,
                        &options.team_id,
                        &options.channel_id,
                        self_identity,
                        options,
                    ) {
                        store_teams_message(conn, &inbound)?;
                        count += 1;
                    }
                }
            }
        }
    }
    Ok(count)
}

fn sync_chat_messages(
    client: &GraphTeamsClient,
    conn: &mut rusqlite::Connection,
    options: &TeamsOptions,
    account_key: &str,
    self_identity: &TeamsSelfIdentity,
) -> Result<usize> {
    let messages = client.list_chat_messages(&options.chat_id, options.limit)?;
    let mut count = 0;
    for msg in &messages {
        if let Some(inbound) = normalize_teams_chat_message_for_sync(
            msg,
            account_key,
            &options.chat_id,
            self_identity,
            options,
        ) {
            store_teams_message(conn, &inbound)?;
            count += 1;
        }
    }
    Ok(count)
}

fn sync_discovered_chats(
    client: &GraphTeamsClient,
    conn: &mut rusqlite::Connection,
    options: &TeamsOptions,
    account_key: &str,
    self_identity: &TeamsSelfIdentity,
) -> Result<usize> {
    let chats = client.list_chats(options.discovery_limit)?;
    let mut count = 0;
    for chat in chats {
        let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
            continue;
        };
        let messages = client.list_chat_messages(chat_id, options.limit)?;
        for msg in &messages {
            if let Some(inbound) = normalize_teams_chat_message_for_sync(
                msg,
                account_key,
                chat_id,
                self_identity,
                options,
            ) {
                store_teams_message(conn, &inbound)?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn sync_discovered_channels(
    client: &GraphTeamsClient,
    conn: &mut rusqlite::Connection,
    options: &TeamsOptions,
    account_key: &str,
    self_identity: &TeamsSelfIdentity,
) -> Result<usize> {
    let teams = client.list_joined_teams(options.discovery_limit)?;
    let mut count = 0;
    for team in teams {
        let Some(team_id) = team.get("id").and_then(Value::as_str) else {
            continue;
        };
        let channels = match client.list_team_channels(team_id, options.discovery_limit) {
            Ok(channels) => channels,
            Err(_) => continue,
        };
        for channel in channels {
            let Some(channel_id) = channel.get("id").and_then(Value::as_str) else {
                continue;
            };
            let messages = match client.list_channel_messages(team_id, channel_id, options.limit) {
                Ok(messages) => messages,
                Err(_) => continue,
            };
            for msg in &messages {
                if let Some(inbound) = normalize_teams_message_for_sync(
                    msg,
                    account_key,
                    team_id,
                    channel_id,
                    self_identity,
                    options,
                ) {
                    store_teams_message(conn, &inbound)?;
                    count += 1;
                }
                let msg_id = msg.get("id").and_then(Value::as_str).unwrap_or_default();
                if msg_id.is_empty() {
                    continue;
                }
                let replies = match client.list_channel_message_replies(
                    team_id,
                    channel_id,
                    msg_id,
                    options.limit,
                ) {
                    Ok(replies) => replies,
                    Err(_) => continue,
                };
                for reply in &replies {
                    if let Some(inbound) = normalize_teams_message_for_sync(
                        reply,
                        account_key,
                        team_id,
                        channel_id,
                        self_identity,
                        options,
                    ) {
                        store_teams_message(conn, &inbound)?;
                        count += 1;
                    }
                }
            }
        }
    }
    Ok(count)
}

fn normalize_teams_message_for_sync(
    msg: &Value,
    account_key: &str,
    team_id: &str,
    channel_id: &str,
    self_identity: &TeamsSelfIdentity,
    options: &TeamsOptions,
) -> Option<TeamsInboundMessage> {
    if is_self_teams_message(msg, self_identity, options) {
        return None;
    }
    normalize_teams_message(msg, account_key, team_id, channel_id)
}

fn normalize_teams_chat_message_for_sync(
    msg: &Value,
    account_key: &str,
    chat_id: &str,
    self_identity: &TeamsSelfIdentity,
    options: &TeamsOptions,
) -> Option<TeamsInboundMessage> {
    if is_self_teams_message(msg, self_identity, options) {
        return None;
    }
    normalize_teams_chat_message(msg, account_key, chat_id)
}

fn is_self_teams_message(
    msg: &Value,
    self_identity: &TeamsSelfIdentity,
    options: &TeamsOptions,
) -> bool {
    let user = msg
        .get("from")
        .and_then(|from| from.get("user"))
        .unwrap_or(&Value::Null);
    let sender_id = user.get("id").and_then(Value::as_str).unwrap_or_default();
    let sender_upn = user
        .get("userPrincipalName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sender_mail = user.get("mail").and_then(Value::as_str).unwrap_or_default();
    let sender_display = user
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let same = |left: &str, right: &str| {
        !left.trim().is_empty() && !right.trim().is_empty() && left.eq_ignore_ascii_case(right)
    };
    same(sender_id, &self_identity.user_id)
        || same(sender_upn, &self_identity.user_principal_name)
        || same(sender_mail, &self_identity.mail)
        || (sender_id.trim().is_empty() && same(sender_display, &self_identity.display_name))
        || same(sender_id, &options.bot_id)
        || same(sender_upn, &options.bot_id)
        || same(sender_mail, &options.bot_id)
        || same(sender_upn, &options.username)
        || same(sender_mail, &options.username)
        || display_matches_configured_bot(sender_display, options)
}

fn display_matches_configured_bot(sender_display: &str, options: &TeamsOptions) -> bool {
    let sender = normalize_teams_identity_label(sender_display);
    if sender.len() < 4 {
        return false;
    }
    [&options.bot_id, &options.username]
        .iter()
        .filter_map(|value| configured_identity_labels(value))
        .any(|labels| labels.iter().any(|label| label == &sender))
}

fn configured_identity_labels(value: &str) -> Option<Vec<String>> {
    let normalized = normalize_teams_identity_label(value);
    if normalized.is_empty() {
        return None;
    }
    let local = value.split('@').next().unwrap_or(value);
    let local_normalized = normalize_teams_identity_label(local);
    let last_segment = local
        .split(['.', '_', '-', ' '])
        .next_back()
        .map(normalize_teams_identity_label)
        .unwrap_or_default();
    let mut labels = vec![normalized, local_normalized, last_segment];
    labels.retain(|label| label.len() >= 4);
    labels.sort();
    labels.dedup();
    Some(labels)
}

fn normalize_teams_identity_label(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_teams_message(
    msg: &Value,
    account_key: &str,
    team_id: &str,
    channel_id: &str,
) -> Option<TeamsInboundMessage> {
    let id = msg.get("id").and_then(Value::as_str)?;
    let msg_type = msg
        .get("messageType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if msg_type != "message" {
        return None;
    }
    let from = msg.get("from").unwrap_or(&Value::Null);
    let user = from.get("user").unwrap_or(&Value::Null);
    let sender_display = user
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let sender_id = user
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let body = msg
        .get("body")
        .and_then(|body| body.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body_text = strip_html_basic(body);
    let subject = msg
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let created = msg
        .get("createdDateTime")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let has_attachments = msg
        .get("attachments")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    let thread_key = format!(
        "{account_key}::{team_id}::{channel_id}::{}",
        msg.get("replyToId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(id)
    );
    let remote_id = id.to_string();
    let message_key = format!("{account_key}::INBOX::{remote_id}");
    Some(TeamsInboundMessage {
        account_key: account_key.to_string(),
        thread_key,
        message_key,
        remote_id,
        sender_display,
        sender_address: sender_id,
        recipients: Vec::new(),
        preview: preview_text(&body_text, &subject),
        subject,
        body_text: body_text.clone(),
        seen: false,
        has_attachments,
        external_created_at: created,
        metadata: json!({
            "teams_message_id": id,
            "teams_team_id": team_id,
            "teams_channel_id": channel_id,
            "teams_reply_to_id": msg.get("replyToId").and_then(Value::as_str),
        }),
    })
}

fn normalize_teams_chat_message(
    msg: &Value,
    account_key: &str,
    chat_id: &str,
) -> Option<TeamsInboundMessage> {
    let id = msg.get("id").and_then(Value::as_str)?;
    let msg_type = msg
        .get("messageType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if msg_type != "message" {
        return None;
    }
    let from = msg.get("from").unwrap_or(&Value::Null);
    let user = from.get("user").unwrap_or(&Value::Null);
    let sender_display = user
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let sender_id = user
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let body = msg
        .get("body")
        .and_then(|body| body.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body_text = strip_html_basic(body);
    let created = msg
        .get("createdDateTime")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let has_attachments = msg
        .get("attachments")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    let thread_key = format!("{account_key}::chat::{chat_id}");
    let remote_id = id.to_string();
    let message_key = format!("{account_key}::INBOX::{remote_id}");
    Some(TeamsInboundMessage {
        account_key: account_key.to_string(),
        thread_key,
        message_key,
        remote_id,
        sender_display,
        sender_address: sender_id,
        recipients: Vec::new(),
        subject: String::new(),
        body_text: body_text.clone(),
        preview: preview_text(&body_text, ""),
        seen: false,
        has_attachments,
        external_created_at: created,
        metadata: json!({
            "teams_message_id": id,
            "teams_chat_id": chat_id,
        }),
    })
}

fn store_teams_message(conn: &mut rusqlite::Connection, msg: &TeamsInboundMessage) -> Result<()> {
    let recipients_json = serde_json::to_string(&msg.recipients)?;
    let metadata_json = serde_json::to_string(&msg.metadata)?;
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &msg.message_key,
            channel: "teams",
            account_key: &msg.account_key,
            thread_key: &msg.thread_key,
            remote_id: &msg.remote_id,
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: &msg.sender_display,
            sender_address: &msg.sender_address,
            recipient_addresses_json: &recipients_json,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &msg.subject,
            preview: &msg.preview,
            body_text: &msg.body_text,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "medium",
            status: "received",
            seen: msg.seen,
            has_attachments: msg.has_attachments,
            external_created_at: &msg.external_created_at,
            observed_at: &now_iso_string(),
            metadata_json: &metadata_json,
        },
    )?;
    refresh_thread(conn, &msg.thread_key)?;
    Ok(())
}

fn execute_send(options: &TeamsOptions, request: &TeamsSendCommandRequest<'_>) -> Result<Value> {
    if !has_any_auth(options) {
        bail!(
            "Teams send requires either CTO_TEAMS_USERNAME + CTO_TEAMS_PASSWORD, or CTO_TEAMS_CLIENT_ID + CTO_TEAMS_CLIENT_SECRET"
        );
    }
    let client = GraphTeamsClient::from_options(options)?;
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_for_teams(options);
    ensure_account(
        &mut conn,
        &account_key,
        "teams",
        &options.bot_id,
        "microsoft-graph",
        build_profile_json(options),
    )?;
    let timestamp = now_iso_string();
    let remote_id = format!(
        "queued-{}",
        stable_digest(&format!("{}:{}", timestamp, request.body))
    );
    let thread_key = if request.thread_key.trim().is_empty() {
        format!("{account_key}::outbound::{remote_id}")
    } else {
        request.thread_key.to_string()
    };
    let subject = if request.subject.trim().is_empty() {
        "(Teams)".to_string()
    } else {
        request.subject.to_string()
    };

    let destination = resolve_send_destination(&thread_key, options)?;
    let (send_result, sent_team_id, sent_channel_id, sent_chat_id) = match &destination {
        TeamsSendDestination::Chat { chat_id } => (
            client.send_chat_message(chat_id, request.body),
            String::new(),
            String::new(),
            chat_id.clone(),
        ),
        TeamsSendDestination::Channel {
            team_id,
            channel_id,
            parent_message_id,
        } => {
            let result = if let Some(parent_id) = parent_message_id {
                client.send_channel_reply(team_id, channel_id, parent_id, request.body)
            } else {
                client.send_channel_message(team_id, channel_id, request.body)
            };
            (result, team_id.clone(), channel_id.clone(), String::new())
        }
    };

    let sent_remote_id = send_result
        .as_ref()
        .ok()
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or(&remote_id);
    let delivery_confirmed = send_result.is_ok();
    let sender_display = request.sender_display.unwrap_or("CTOX Bot");
    let message_key = format!("{account_key}::SENT::{sent_remote_id}");
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "teams",
            account_key: &account_key,
            thread_key: &thread_key,
            remote_id: sent_remote_id,
            direction: "outbound",
            folder_hint: "SENT",
            sender_display,
            sender_address: &options.bot_id,
            recipient_addresses_json: &serde_json::to_string(request.to)?,
            cc_addresses_json: "[]",
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
                "teams_sent_message_id": sent_remote_id,
                "teams_team_id": sent_team_id,
                "teams_channel_id": sent_channel_id,
                "teams_chat_id": sent_chat_id,
            }))?,
        },
    )?;
    refresh_thread(&mut conn, &thread_key)?;
    Ok(json!({
        "ok": true,
        "status": if delivery_confirmed { "sent" } else { "failed" },
        "delivery": {
            "confirmed": delivery_confirmed,
            "message_key": message_key,
            "remote_id": sent_remote_id,
        },
        "error": send_result.as_ref().err().map(|error| error.to_string()),
    }))
}

fn execute_test(options: &TeamsOptions) -> Result<Value> {
    if !has_any_auth(options) {
        return Ok(json!({
            "ok": false,
            "error": "missing credentials: set CTO_TEAMS_GRAPH_ACCESS_TOKEN, CTO_TEAMS_USERNAME + CTO_TEAMS_PASSWORD, CTO_TEAMS_CLIENT_ID + CTO_TEAMS_CLIENT_SECRET, or reusable CTO_EMAIL Graph credentials",
        }));
    }
    let client = match GraphTeamsClient::from_options(options) {
        Ok(client) => client,
        Err(error) => {
            return Ok(json!({
                "ok": false,
                "error": format!("OAuth2 token acquisition failed: {error}"),
            }));
        }
    };
    match client.get_app_info() {
        Ok(org_info) => {
            let org_name = org_info
                .get("value")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("displayName"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Ok(json!({
                "ok": true,
                "status": "connected",
                "organization": org_name,
                "tenant_id": options.tenant_id,
                "bot_id": options.bot_id,
                "team_id": options.team_id,
                "channel_id": options.channel_id,
                "chat_id": options.chat_id,
            }))
        }
        Err(org_error) => match client.get_self_identity() {
            Ok(identity) => Ok(json!({
                "ok": true,
                "status": "connected",
                "organization": "unknown",
                "tenant_id": options.tenant_id,
                "bot_id": options.bot_id,
                "team_id": options.team_id,
                "channel_id": options.channel_id,
                "chat_id": options.chat_id,
                "user": {
                    "id": identity.user_id,
                    "displayName": identity.display_name,
                    "userPrincipalName": identity.user_principal_name,
                    "mail": identity.mail,
                },
                "warning": format!("organization probe failed: {org_error}"),
            })),
            Err(me_error) => Ok(json!({
                "ok": false,
                "error": format!("Graph API test failed: organization probe: {org_error}; /me probe: {me_error}"),
            })),
        },
    }
}

fn account_key_for_teams(options: &TeamsOptions) -> String {
    if !options.username.is_empty() {
        return format!("teams:{}", options.username.to_lowercase());
    }
    let id = if options.bot_id.is_empty() {
        &options.tenant_id
    } else {
        &options.bot_id
    };
    format!("teams:{id}")
}

fn extract_parent_message_id(thread_key: &str) -> Option<String> {
    // Thread key format: "teams:botid::teamid::channelid::parentmsgid"
    let parts: Vec<&str> = thread_key.split("::").collect();
    if parts.len() >= 4 {
        let candidate = parts[3];
        if !candidate.is_empty()
            && !candidate.starts_with("outbound")
            && !candidate.starts_with("chat")
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn resolve_send_destination(
    thread_key: &str,
    options: &TeamsOptions,
) -> Result<TeamsSendDestination> {
    let parts: Vec<&str> = thread_key.split("::").collect();
    if parts.len() >= 3 && parts[1] == "chat" && !parts[2].trim().is_empty() {
        return Ok(TeamsSendDestination::Chat {
            chat_id: parts[2].trim().to_string(),
        });
    }
    if parts.len() >= 3
        && !parts[1].trim().is_empty()
        && !parts[2].trim().is_empty()
        && parts[1] != "chat"
        && parts[1] != "outbound"
    {
        let parent_message_id = parts
            .get(3)
            .map(|value| value.trim())
            .filter(|value| {
                !value.is_empty() && !value.starts_with("outbound") && !value.starts_with("chat")
            })
            .map(ToOwned::to_owned);
        return Ok(TeamsSendDestination::Channel {
            team_id: parts[1].trim().to_string(),
            channel_id: parts[2].trim().to_string(),
            parent_message_id,
        });
    }
    if !options.chat_id.trim().is_empty() {
        return Ok(TeamsSendDestination::Chat {
            chat_id: options.chat_id.trim().to_string(),
        });
    }
    if !options.team_id.trim().is_empty() && !options.channel_id.trim().is_empty() {
        return Ok(TeamsSendDestination::Channel {
            team_id: options.team_id.trim().to_string(),
            channel_id: options.channel_id.trim().to_string(),
            parent_message_id: extract_parent_message_id(thread_key),
        });
    }
    bail!(
        "Teams send requires either CTO_TEAMS_CHAT_ID or CTO_TEAMS_TEAM_ID + CTO_TEAMS_CHANNEL_ID"
    )
}

fn build_profile_json(options: &TeamsOptions) -> Value {
    json!({
        "username": options.username,
        "tenantId": options.tenant_id,
        "botId": options.bot_id,
        "teamId": options.team_id,
        "channelId": options.channel_id,
        "chatId": options.chat_id,
        "graphBaseUrl": options.graph_base_url,
    })
}

fn strip_html_basic(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result.trim().to_string()
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

fn base_options_from_runtime(
    _root: &Path,
    runtime: &BTreeMap<String, String>,
    db_path: &Path,
) -> TeamsOptions {
    TeamsOptions {
        db_path: db_path.to_path_buf(),
        graph_access_token: first_setting(
            runtime,
            &[
                "CTO_TEAMS_GRAPH_ACCESS_TOKEN",
                "CTO_EMAIL_GRAPH_ACCESS_TOKEN",
            ],
        ),
        username: first_setting(
            runtime,
            &[
                "CTO_TEAMS_USERNAME",
                "CTO_EMAIL_GRAPH_USERNAME",
                "CTO_EMAIL_ADDRESS",
            ],
        ),
        password: first_setting(
            runtime,
            &[
                "CTO_TEAMS_PASSWORD",
                "CTO_EMAIL_GRAPH_PASSWORD",
                "CTO_EMAIL_PASSWORD",
            ],
        ),
        tenant_id: first_setting(
            runtime,
            &["CTO_TEAMS_TENANT_ID", "CTO_EMAIL_GRAPH_TENANT_ID"],
        ),
        client_id: first_setting(
            runtime,
            &["CTO_TEAMS_CLIENT_ID", "CTO_EMAIL_GRAPH_CLIENT_ID"],
        ),
        client_secret: first_setting(
            runtime,
            &["CTO_TEAMS_CLIENT_SECRET", "CTO_EMAIL_GRAPH_CLIENT_SECRET"],
        ),
        bot_id: first_setting(runtime, &["CTO_TEAMS_BOT_ID", "CTO_EMAIL_ADDRESS"]),
        graph_base_url: first_setting_or_default(
            runtime,
            &["CTO_TEAMS_GRAPH_BASE_URL", "CTO_EMAIL_GRAPH_BASE_URL"],
            GRAPH_DEFAULT_BASE_URL,
        ),
        team_id: setting(runtime, "CTO_TEAMS_TEAM_ID"),
        channel_id: setting(runtime, "CTO_TEAMS_CHANNEL_ID"),
        chat_id: setting(runtime, "CTO_TEAMS_CHAT_ID"),
        limit: setting(runtime, "CTO_TEAMS_LIMIT")
            .parse()
            .unwrap_or(TEAMS_SYNC_LIMIT),
        discovery_limit: setting(runtime, "CTO_TEAMS_DISCOVERY_LIMIT")
            .parse()
            .unwrap_or(12),
    }
}

fn sync_options_from_args(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<TeamsOptions> {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    for pair in request.passthrough_args.windows(2) {
        let flag = pair[0].as_str();
        let value = pair[1].as_str();
        if request.skip_flags.contains(&flag) {
            continue;
        }
        match flag {
            "--tenant-id" => options.tenant_id = value.to_string(),
            "--team-id" => options.team_id = value.to_string(),
            "--channel-id" => options.channel_id = value.to_string(),
            "--chat-id" => options.chat_id = value.to_string(),
            "--bot-id" => options.bot_id = value.to_string(),
            "--limit" => {
                options.limit = value.parse().unwrap_or(TEAMS_SYNC_LIMIT);
            }
            _ => {}
        }
    }
    Ok(options)
}

fn send_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &TeamsSendCommandRequest<'_>,
) -> TeamsOptions {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    if !request.tenant_id.trim().is_empty() {
        options.tenant_id = request.tenant_id.trim().to_string();
    }
    options
}

fn test_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &TeamsTestCommandRequest<'_>,
) -> TeamsOptions {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    if !request.tenant_id.trim().is_empty() {
        options.tenant_id = request.tenant_id.trim().to_string();
    }
    if let Some(profile) = request.profile_json.as_object() {
        if let Some(value) = profile.get("teamId").and_then(Value::as_str) {
            options.team_id = value.to_string();
        }
        if let Some(value) = profile.get("channelId").and_then(Value::as_str) {
            options.channel_id = value.to_string();
        }
        if let Some(value) = profile.get("chatId").and_then(Value::as_str) {
            options.chat_id = value.to_string();
        }
        if let Some(value) = profile.get("botId").and_then(Value::as_str) {
            options.bot_id = value.to_string();
        }
    }
    options
}

fn runtime_from_settings(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    use crate::communication::gateway as communication_gateway;
    communication_gateway::runtime_settings_from_settings(
        root,
        communication_gateway::CommunicationAdapterKind::Teams,
        settings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: `urlencoding_encode` lives in the shared Microsoft Graph auth module.
    // and is unit-tested there. The previous duplicate test for it has been removed.

    #[test]
    fn strip_html_basic_removes_tags() {
        assert_eq!(strip_html_basic("<p>hello</p>"), "hello");
        assert_eq!(strip_html_basic("<div><b>bold</b> text</div>"), "bold text");
        assert_eq!(strip_html_basic("plain text"), "plain text");
    }

    fn test_options(username: &str, bot_id: &str, tenant_id: &str) -> TeamsOptions {
        TeamsOptions {
            db_path: std::path::PathBuf::from("/tmp/test.db"),
            graph_access_token: String::new(),
            username: username.to_string(),
            password: if username.is_empty() {
                String::new()
            } else {
                "pw".to_string()
            },
            tenant_id: tenant_id.to_string(),
            client_id: String::new(),
            client_secret: String::new(),
            bot_id: bot_id.to_string(),
            graph_base_url: GRAPH_DEFAULT_BASE_URL.to_string(),
            team_id: String::new(),
            channel_id: String::new(),
            chat_id: String::new(),
            limit: 50,
            discovery_limit: 12,
        }
    }

    #[test]
    fn account_key_prefers_username() {
        assert_eq!(
            account_key_for_teams(&test_options("User@Example.COM", "bot-xyz", "t1")),
            "teams:user@example.com"
        );
    }

    #[test]
    fn account_key_falls_back_to_bot_id() {
        assert_eq!(
            account_key_for_teams(&test_options("", "bot-xyz", "tenant-abc")),
            "teams:bot-xyz"
        );
    }

    #[test]
    fn account_key_falls_back_to_tenant() {
        assert_eq!(
            account_key_for_teams(&test_options("", "", "tenant-abc")),
            "teams:tenant-abc"
        );
    }

    #[test]
    fn extract_parent_message_id_from_thread_key() {
        let thread = "teams:bot123::team456::chan789::msg001";
        assert_eq!(
            extract_parent_message_id(thread),
            Some("msg001".to_string())
        );
        let thread = "teams:bot123::team456::chan789::outbound-xyz";
        assert_eq!(extract_parent_message_id(thread), None);
        let thread = "teams:bot123::chat::chat-id-here";
        assert_eq!(extract_parent_message_id(thread), None);
    }

    #[test]
    fn resolve_send_destination_prefers_chat_thread_key() {
        let mut options = test_options("user@example.com", "bot", "tenant");
        options.chat_id = "configured-chat".to_string();
        let destination =
            resolve_send_destination("teams:bot::chat::thread-chat", &options).unwrap();
        assert_eq!(
            destination,
            TeamsSendDestination::Chat {
                chat_id: "thread-chat".to_string()
            }
        );
    }

    #[test]
    fn resolve_send_destination_prefers_channel_thread_key_over_configured_chat() {
        let mut options = test_options("user@example.com", "bot", "tenant");
        options.chat_id = "configured-chat".to_string();
        let destination =
            resolve_send_destination("teams:bot::team-1::channel-1::parent-1", &options).unwrap();
        assert_eq!(
            destination,
            TeamsSendDestination::Channel {
                team_id: "team-1".to_string(),
                channel_id: "channel-1".to_string(),
                parent_message_id: Some("parent-1".to_string())
            }
        );
    }

    #[test]
    fn resolve_send_destination_falls_back_to_configured_chat() {
        let mut options = test_options("user@example.com", "bot", "tenant");
        options.chat_id = "configured-chat".to_string();
        let destination =
            resolve_send_destination("teams:bot::outbound::queued", &options).unwrap();
        assert_eq!(
            destination,
            TeamsSendDestination::Chat {
                chat_id: "configured-chat".to_string()
            }
        );
    }

    #[test]
    fn normalize_teams_message_skips_system_messages() {
        let msg = json!({
            "id": "msg1",
            "messageType": "systemEventMessage",
            "body": {"content": "team created"},
        });
        assert!(normalize_teams_message(&msg, "teams:bot", "t1", "c1").is_none());
    }

    #[test]
    fn normalize_teams_message_extracts_user_message() {
        let msg = json!({
            "id": "msg42",
            "messageType": "message",
            "from": {"user": {"displayName": "Alice", "id": "user-alice"}},
            "body": {"content": "<p>Hello world</p>"},
            "subject": "Test",
            "createdDateTime": "2026-01-15T10:00:00Z",
            "attachments": [],
        });
        let result = normalize_teams_message(&msg, "teams:bot", "team1", "chan1").unwrap();
        assert_eq!(result.sender_display, "Alice");
        assert_eq!(result.sender_address, "user-alice");
        assert_eq!(result.body_text, "Hello world");
        assert_eq!(result.remote_id, "msg42");
        assert!(result.thread_key.contains("team1"));
        assert!(result.thread_key.contains("chan1"));
    }

    #[test]
    fn normalize_teams_chat_message_extracts_fields() {
        let msg = json!({
            "id": "chat-msg-1",
            "messageType": "message",
            "from": {"user": {"displayName": "Bob", "id": "user-bob"}},
            "body": {"content": "Hi there"},
            "createdDateTime": "2026-01-15T11:00:00Z",
        });
        let result = normalize_teams_chat_message(&msg, "teams:bot", "chat-123").unwrap();
        assert_eq!(result.sender_display, "Bob");
        assert_eq!(result.thread_key, "teams:bot::chat::chat-123");
    }

    #[test]
    fn self_message_detection_uses_configured_bot_display_alias() {
        let options = test_options("inf.yoda@remcapital.de", "INF.Yoda@remcapital.de", "");
        let identity = TeamsSelfIdentity::default();
        let self_msg = json!({
            "from": {"user": {"displayName": "Yoda", "id": "2cec2f2d-9b6d-4b36-9d47-ea3f7d7fb47e"}},
        });
        let other_msg = json!({
            "from": {"user": {"displayName": "Cakmak, Jill", "id": "cfbba921-291c-42d3-8e30-3032f36b4601"}},
        });

        assert!(is_self_teams_message(&self_msg, &identity, &options));
        assert!(!is_self_teams_message(&other_msg, &identity, &options));
    }

    #[test]
    fn build_profile_json_includes_all_fields() {
        let mut options = test_options("user@co.com", "b1", "t1");
        options.team_id = "team1".to_string();
        let profile = build_profile_json(&options);
        assert_eq!(
            profile.get("username").unwrap().as_str().unwrap(),
            "user@co.com"
        );
        assert_eq!(profile.get("tenantId").unwrap().as_str().unwrap(), "t1");
        assert_eq!(profile.get("teamId").unwrap().as_str().unwrap(), "team1");
    }
}
