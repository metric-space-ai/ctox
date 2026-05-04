use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Once;
use std::thread;
use std::time::Duration;

use crate::communication::adapters::{
    AdapterSyncCommandRequest, WhatsappSendCommandRequest, WhatsappTestCommandRequest,
};
use crate::communication::runtime as communication_runtime;
use crate::mission::channels::{
    ensure_account, ensure_routing_rows_for_inbound, now_iso_string, open_channel_db, preview_text,
    record_communication_sync_run, refresh_thread, stable_digest, upsert_communication_message,
    CommunicationSyncRun, UpsertMessage,
};

use whatsapp::client::{pair, Client, Event as LowEvent};
use whatsapp::store::sqlite::SqliteStore;
use whatsapp::{Account, Event, IncomingMessage, Jid};

const DEFAULT_PUSH_NAME: &str = "CTOX";
const DEFAULT_PAIR_TIMEOUT_SECONDS: u64 = 120;
const DEFAULT_SYNC_TIMEOUT_SECONDS: u64 = 8;
const DEFAULT_LIMIT: usize = 25;

static RUSTLS_PROVIDER: Once = Once::new();

#[derive(Clone, Debug)]
struct WhatsappOptions {
    root: PathBuf,
    db_path: PathBuf,
    device_db_path: PathBuf,
    push_name: String,
    pair_timeout_seconds: u64,
    sync_timeout_seconds: u64,
    limit: usize,
    no_pair: bool,
}

#[derive(Clone, Debug)]
struct OwnedWhatsappSendRequest {
    db_path: PathBuf,
    account_key: String,
    thread_key: String,
    to: Vec<String>,
    sender_display: Option<String>,
    body: String,
    attachments: Vec<String>,
}

#[derive(Clone, Debug)]
struct WhatsappInboundMessage {
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
    has_attachments: bool,
    external_created_at: String,
    metadata: Value,
}

pub(crate) fn sync(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<Value> {
    let options = options_from_sync_args(root, runtime, request)?;
    run_async(execute_sync_async(options))
}

pub(crate) fn send(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &WhatsappSendCommandRequest<'_>,
) -> Result<Value> {
    let options = base_options_from_runtime(root, runtime, request.db_path);
    let owned = OwnedWhatsappSendRequest {
        db_path: request.db_path.to_path_buf(),
        account_key: request.account_key.to_string(),
        thread_key: request.thread_key.to_string(),
        to: request.to.to_vec(),
        sender_display: request.sender_display.map(str::to_string),
        body: request.body.to_string(),
        attachments: request.attachments.to_vec(),
    };
    run_async(execute_send_async(options, owned))
}

pub(crate) fn test(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &WhatsappTestCommandRequest<'_>,
) -> Result<Value> {
    let options = base_options_from_runtime(root, runtime, request.db_path);
    run_async(execute_test_async(
        options,
        request.account_key.map(str::to_string),
    ))
}

pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let preferred_is_whatsapp = settings
        .get("CTOX_OWNER_PREFERRED_CHANNEL")
        .map(|value| value.trim())
        == Some("whatsapp");
    let configured_device_db = settings
        .get("CTO_WHATSAPP_DEVICE_DB")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let default_device_db = default_device_db_path(root);
    let legacy_device_db = legacy_default_device_db_path(root);
    if !preferred_is_whatsapp
        && configured_device_db.is_none()
        && !default_device_db.exists()
        && !legacy_device_db.exists()
    {
        return Ok(None);
    }

    let runtime = runtime_from_settings(root, settings);
    let db_path = root.join("runtime/ctox.sqlite3");
    let args = vec![
        "sync".to_string(),
        "--no-pair".to_string(),
        "--sync-timeout-seconds".to_string(),
        runtime
            .get("CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS")
            .cloned()
            .unwrap_or_else(|| "5".to_string()),
        "--limit".to_string(),
        "25".to_string(),
    ];
    let request = AdapterSyncCommandRequest {
        db_path: db_path.as_path(),
        passthrough_args: &args,
        skip_flags: &["--db", "--channel"],
    };
    sync(root, &runtime, &request).map(Some)
}

async fn execute_sync_async(options: WhatsappOptions) -> Result<Value> {
    install_rustls_provider();
    ensure_parent_dir(&options.device_db_path)?;

    let sync_start = now_iso_string();
    let mut paired_now = false;
    let mut pairing_artifact: Option<Value> = None;
    let probe = Account::open(&options.device_db_path)
        .await
        .context("failed to open WhatsApp account store")?;
    if !probe.is_paired().await? {
        if options.no_pair {
            let artifact = latest_pairing_artifact(&options.root);
            return Ok(json!({
                "ok": false,
                "adapter": "whatsapp",
                "status": "pairing_required",
                "device_db": options.device_db_path,
                "pairing": artifact,
            }));
        }
        let paired_jid = pair_device_until_success(&options).await?;
        paired_now = true;
        pairing_artifact = Some(json!({
            "status": "paired",
            "jid": paired_jid,
        }));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let account = Account::open(&options.device_db_path)
        .await?
        .with_push_name(options.push_name.clone());
    let mut events = account.connect().await?;
    let own_jid = account
        .jid()
        .map(|jid| jid.to_non_ad().to_string())
        .unwrap_or_else(|| "unknown@s.whatsapp.net".to_string());
    let account_key = account_key_from_jid(&own_jid);
    let mut conn = open_channel_db(&options.db_path)?;
    ensure_account(
        &mut conn,
        &account_key,
        "whatsapp",
        &own_jid,
        "whatsapp-web-md",
        build_profile_json(&options, &own_jid),
    )?;

    let mut fetched_count = 0usize;
    let mut stored_count = 0usize;
    let mut history_syncs = 0usize;
    let deadline = tokio::time::sleep(Duration::from_secs(options.sync_timeout_seconds));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => break,
            maybe_event = events.recv() => {
                let Some(event) = maybe_event else { break };
                match event {
                    Event::Connected { .. } => {}
                    Event::Message(message) => {
                        fetched_count += 1;
                        let inbound = normalize_inbound_message(&account_key, &own_jid, &message)?;
                        if !known_message(&conn, &inbound.message_key)? {
                            stored_count += 1;
                        }
                        store_inbound_message(&mut conn, &inbound)?;
                        if stored_count >= options.limit {
                            break;
                        }
                    }
                    Event::HistorySync(_) => {
                        history_syncs += 1;
                    }
                    Event::Receipt { .. } => {}
                    Event::Qr { code, .. } => {
                        pairing_artifact = Some(write_pairing_artifacts(&options.root, &code)?);
                    }
                    Event::Paired { jid } => {
                        pairing_artifact = Some(json!({"status": "paired", "jid": jid.to_string()}));
                    }
                    Event::Disconnected { reason } => {
                        break_with_sync_error(&mut conn, &account_key, &sync_start, fetched_count, stored_count, &reason)?;
                        bail!("WhatsApp disconnected: {reason}");
                    }
                    Event::Error(message) => {
                        break_with_sync_error(&mut conn, &account_key, &sync_start, fetched_count, stored_count, &message)?;
                        bail!("WhatsApp sync error: {message}");
                    }
                }
            }
        }
    }

    ensure_routing_rows_for_inbound(&conn)?;
    let finished_at = now_iso_string();
    let run_key = stable_digest(&format!("whatsapp:{account_key}:{sync_start}"));
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &run_key,
            channel: "whatsapp",
            account_key: &account_key,
            folder_hint: "INBOX",
            started_at: &sync_start,
            finished_at: &finished_at,
            ok: true,
            fetched_count: fetched_count as i64,
            stored_count: stored_count as i64,
            error_text: "",
            metadata_json: &serde_json::to_string(&json!({
                "paired_now": paired_now,
                "history_syncs_seen": history_syncs,
            }))?,
        },
    )?;

    Ok(json!({
        "ok": true,
        "adapter": "whatsapp",
        "account_key": account_key,
        "device_db": options.device_db_path,
        "messages_fetched": fetched_count,
        "messages_stored": stored_count,
        "history_syncs_seen": history_syncs,
        "pairing": pairing_artifact,
    }))
}

async fn execute_send_async(
    options: WhatsappOptions,
    request: OwnedWhatsappSendRequest,
) -> Result<Value> {
    install_rustls_provider();
    ensure_parent_dir(&options.device_db_path)?;
    let account = Account::open(&options.device_db_path)
        .await
        .context("failed to open WhatsApp account store")?
        .with_push_name(options.push_name.clone());
    if !account.is_paired().await? {
        bail!("WhatsApp account is not paired yet; run `ctox channel sync --channel whatsapp` and scan the QR code first");
    }
    let mut _events = account.connect().await?;
    let own_jid = account
        .jid()
        .map(|jid| jid.to_non_ad().to_string())
        .unwrap_or_else(|| address_from_account_key(&request.account_key));
    let account_key = if request.account_key.trim().is_empty() {
        account_key_from_jid(&own_jid)
    } else {
        request.account_key.clone()
    };
    let chat = resolve_destination_jid(&request)?;

    let mut remote_ids = Vec::new();
    let mut sent_text = false;
    if request.attachments.is_empty()
        || !request.body.trim().is_empty() && has_document_attachment(&request.attachments)
    {
        if !request.body.trim().is_empty() {
            remote_ids.push(account.send_text(&chat, request.body.trim()).await?);
            sent_text = true;
        }
    }
    for attachment in &request.attachments {
        let path = PathBuf::from(attachment);
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read WhatsApp attachment {}", path.display()))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("attachment")
            .to_string();
        if is_jpeg_file(&path) {
            let caption = if sent_text || request.body.trim().is_empty() {
                None
            } else {
                Some(request.body.trim())
            };
            remote_ids.push(account.send_image(&chat, &bytes, caption).await?);
            sent_text = true;
        } else {
            remote_ids.push(
                account
                    .send_document(&chat, &bytes, mime_type_for_path(&path), &file_name)
                    .await?,
            );
        }
    }
    if remote_ids.is_empty() {
        bail!("WhatsApp send requires non-empty --body or --attach-file");
    }

    let timestamp = now_iso_string();
    let remote_id = remote_ids.join(",");
    let thread_key = if request.thread_key.trim().is_empty() {
        thread_key_for_chat(&account_key, &chat.to_non_ad().to_string())
    } else {
        request.thread_key.clone()
    };
    let recipients_json = serde_json::to_string(&vec![chat.to_non_ad().to_string()])?;
    let metadata_json = serde_json::to_string(&json!({
        "whatsapp_sent_message_ids": remote_ids,
        "whatsapp_chat_jid": chat.to_non_ad().to_string(),
        "attachments": request.attachments,
    }))?;
    let message_key = format!(
        "{account_key}::SENT::{}",
        stable_digest(&format!("{thread_key}:{remote_id}:{timestamp}"))
    );

    let mut conn = open_channel_db(&request.db_path)?;
    ensure_account(
        &mut conn,
        &account_key,
        "whatsapp",
        &own_jid,
        "whatsapp-web-md",
        build_profile_json(&options, &own_jid),
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "whatsapp",
            account_key: &account_key,
            thread_key: &thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "SENT",
            sender_display: request
                .sender_display
                .as_deref()
                .unwrap_or(&options.push_name),
            sender_address: &own_jid,
            recipient_addresses_json: &recipients_json,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "WhatsApp",
            preview: &preview_text(&request.body, "WhatsApp"),
            body_text: &request.body,
            body_html: "",
            raw_payload_ref: &request.attachments.join("\n"),
            trust_level: "high",
            status: "sent",
            seen: true,
            has_attachments: !request.attachments.is_empty(),
            external_created_at: &timestamp,
            observed_at: &timestamp,
            metadata_json: &metadata_json,
        },
    )?;
    refresh_thread(&mut conn, &thread_key)?;

    Ok(json!({
        "ok": true,
        "adapter": "whatsapp",
        "account_key": account_key,
        "thread_key": thread_key,
        "message_key": message_key,
        "status": "sent",
        "delivery": {"confirmed": true, "method": "whatsapp-web-md"},
        "remote_ids": remote_id,
    }))
}

async fn execute_test_async(
    options: WhatsappOptions,
    account_key: Option<String>,
) -> Result<Value> {
    install_rustls_provider();
    ensure_parent_dir(&options.device_db_path)?;
    let account = Account::open(&options.device_db_path).await?;
    let paired = account.is_paired().await?;
    let jid = account.jid().map(|value| value.to_non_ad().to_string());
    Ok(json!({
        "ok": paired,
        "adapter": "whatsapp",
        "status": if paired { "paired" } else { "pairing_required" },
        "account_key": account_key,
        "jid": jid,
        "device_db": options.device_db_path,
        "pairing": latest_pairing_artifact(&options.root),
    }))
}

async fn pair_device_until_success(options: &WhatsappOptions) -> Result<String> {
    let store = Arc::new(SqliteStore::open(path_to_str(&options.device_db_path)?)?);
    let saved = store.load_device().await?;
    if let Some(device) = saved.as_ref().filter(|device| device.id.is_some()) {
        return Ok(device
            .id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default());
    }
    let mut device = saved.unwrap_or_else(|| store.new_device());
    if device.push_name.is_empty() {
        device.push_name = options.push_name.clone();
    }
    let deadline = tokio::time::sleep(Duration::from_secs(options.pair_timeout_seconds));
    tokio::pin!(deadline);
    let (mut client, mut events) = Client::new(device);
    client.connect().await?;

    loop {
        tokio::select! {
            _ = &mut deadline => {
                bail!("WhatsApp pairing timed out after {} seconds; QR artifact is available at runtime/communication/whatsapp/artifacts", options.pair_timeout_seconds);
            }
            maybe_event = events.recv() => {
                let Some(event) = maybe_event else {
                    bail!("WhatsApp pairing event stream ended before pair-success");
                };
                match event {
                    LowEvent::QrCode { code } => {
                        write_pairing_artifacts(&options.root, &code)?;
                    }
                    LowEvent::PairSuccess { id } => {
                        store.save_device(&client.device).await?;
                        return Ok(id.to_non_ad().to_string());
                    }
                    LowEvent::UnhandledNode { node } if node.tag == "iq" => {
                        if node.child_by_tag(&["pair-device"]).is_some() {
                            pair::handle_pair_device(&client, &node).await?;
                            continue;
                        }
                        if node.child_by_tag(&["pair-success"]).is_some() {
                            pair::handle_pair_success(&mut client, &node).await?;
                            continue;
                        }
                    }
                    LowEvent::Disconnected { reason } => {
                        bail!("WhatsApp pairing disconnected: {reason}");
                    }
                    LowEvent::StreamError { code, text } => {
                        bail!("WhatsApp pairing stream error {code}: {text}");
                    }
                    _ => {}
                }
            }
        }
    }
}

fn normalize_inbound_message(
    account_key: &str,
    own_jid: &str,
    message: &IncomingMessage,
) -> Result<WhatsappInboundMessage> {
    let chat_jid = message.chat.to_non_ad().to_string();
    let sender_jid = message.from.to_non_ad().to_string();
    let body_text = message.text().map(str::to_string).unwrap_or_else(|| {
        if message.is_media() {
            "(WhatsApp media message)".to_string()
        } else if message.is_reaction() {
            "(WhatsApp reaction)".to_string()
        } else {
            "(WhatsApp message without text)".to_string()
        }
    });
    let thread_key = thread_key_for_chat(account_key, &chat_jid);
    let external_created_at = timestamp_to_iso(message.timestamp);
    let message_key = format!(
        "{account_key}::INBOX::{}",
        stable_digest(&format!(
            "{}:{}:{}",
            chat_jid, sender_jid, message.message_id
        ))
    );
    let message_type = whatsapp_message_type(message);
    Ok(WhatsappInboundMessage {
        message_key,
        account_key: account_key.to_string(),
        thread_key,
        remote_id: message.message_id.clone(),
        sender_display: sender_jid.clone(),
        sender_address: sender_jid.clone(),
        recipients: vec![own_jid.to_string()],
        subject: if message.chat.is_group() {
            format!("WhatsApp group: {chat_jid}")
        } else {
            "WhatsApp".to_string()
        },
        preview: preview_text(&body_text, "WhatsApp"),
        body_text,
        seen: false,
        has_attachments: message.is_media(),
        external_created_at,
        metadata: json!({
            "whatsapp_chat_jid": chat_jid,
            "whatsapp_sender_jid": sender_jid,
            "whatsapp_message_id": message.message_id,
            "whatsapp_timestamp": message.timestamp,
            "is_group": message.chat.is_group(),
            "is_media": message.is_media(),
            "is_reaction": message.is_reaction(),
            "message_type": message_type,
        }),
    })
}

fn store_inbound_message(
    conn: &mut rusqlite::Connection,
    message: &WhatsappInboundMessage,
) -> Result<()> {
    let recipients_json = serde_json::to_string(&message.recipients)?;
    let metadata_json = serde_json::to_string(&message.metadata)?;
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message.message_key,
            channel: "whatsapp",
            account_key: &message.account_key,
            thread_key: &message.thread_key,
            remote_id: &message.remote_id,
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: &message.sender_display,
            sender_address: &message.sender_address,
            recipient_addresses_json: &recipients_json,
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
            has_attachments: message.has_attachments,
            external_created_at: &message.external_created_at,
            observed_at: &now_iso_string(),
            metadata_json: &metadata_json,
        },
    )?;
    refresh_thread(conn, &message.thread_key)?;
    Ok(())
}

fn break_with_sync_error(
    conn: &mut rusqlite::Connection,
    account_key: &str,
    sync_start: &str,
    fetched_count: usize,
    stored_count: usize,
    error: &str,
) -> Result<()> {
    let finished_at = now_iso_string();
    let run_key = stable_digest(&format!("whatsapp:{account_key}:{sync_start}:error"));
    record_communication_sync_run(
        conn,
        CommunicationSyncRun {
            run_key: &run_key,
            channel: "whatsapp",
            account_key,
            folder_hint: "INBOX",
            started_at: sync_start,
            finished_at: &finished_at,
            ok: false,
            fetched_count: fetched_count as i64,
            stored_count: stored_count as i64,
            error_text: error,
            metadata_json: "{}",
        },
    )
}

fn options_from_sync_args(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<WhatsappOptions> {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    if let Some(value) = flag_value(request.passthrough_args, "--device-db") {
        options.device_db_path = communication_runtime::resolve_configured_path(
            root,
            Some(value),
            default_device_db_path(root),
        );
    }
    if let Some(value) = flag_value(request.passthrough_args, "--push-name") {
        options.push_name = value.to_string();
    }
    if let Some(value) = flag_value(request.passthrough_args, "--pair-timeout-seconds") {
        options.pair_timeout_seconds = parse_u64(value, "--pair-timeout-seconds")?;
    }
    if let Some(value) = flag_value(request.passthrough_args, "--sync-timeout-seconds") {
        options.sync_timeout_seconds = parse_u64(value, "--sync-timeout-seconds")?;
    }
    if let Some(value) = flag_value(request.passthrough_args, "--limit") {
        options.limit = parse_usize(value, "--limit")?;
    }
    options.no_pair = has_flag(request.passthrough_args, "--no-pair");
    Ok(options)
}

fn base_options_from_runtime(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    db_path: &Path,
) -> WhatsappOptions {
    let device_db_path = communication_runtime::resolve_configured_path(
        root,
        runtime.get("CTO_WHATSAPP_DEVICE_DB").map(String::as_str),
        default_device_db_path(root),
    );
    WhatsappOptions {
        root: root.to_path_buf(),
        db_path: db_path.to_path_buf(),
        device_db_path,
        push_name: runtime
            .get("CTO_WHATSAPP_PUSH_NAME")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_PUSH_NAME)
            .to_string(),
        pair_timeout_seconds: runtime
            .get("CTO_WHATSAPP_PAIR_TIMEOUT_SECONDS")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_PAIR_TIMEOUT_SECONDS),
        sync_timeout_seconds: runtime
            .get("CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_SYNC_TIMEOUT_SECONDS),
        limit: DEFAULT_LIMIT,
        no_pair: false,
    }
}

fn runtime_from_settings(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    crate::communication::gateway::runtime_settings_from_settings(
        root,
        crate::communication::gateway::CommunicationAdapterKind::Whatsapp,
        settings,
    )
}

fn run_async<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .context("failed to build WhatsApp runtime")?
            .block_on(future)
    })
    .join()
    .map_err(|_| anyhow!("WhatsApp worker thread panicked"))?
}

fn install_rustls_provider() {
    RUSTLS_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn build_profile_json(options: &WhatsappOptions, jid: &str) -> Value {
    json!({
        "jid": jid,
        "deviceDb": options.device_db_path,
        "pushName": options.push_name,
    })
}

fn write_pairing_artifacts(root: &Path, code: &str) -> Result<Value> {
    let artifact_dir = communication_runtime::artifacts_dir(root, "whatsapp");
    fs::create_dir_all(&artifact_dir).with_context(|| {
        format!(
            "failed to create WhatsApp artifact dir {}",
            artifact_dir.display()
        )
    })?;
    let svg_path = artifact_dir.join("pairing-qr.svg");
    let status_path = artifact_dir.join("pairing-status.json");
    let svg = qr_svg(code)?;
    fs::write(&svg_path, svg)
        .with_context(|| format!("failed to write WhatsApp QR {}", svg_path.display()))?;
    let status = json!({
        "status": "qr",
        "qr_svg": svg_path,
        "updated_at": now_iso_string(),
    });
    fs::write(&status_path, serde_json::to_vec_pretty(&status)?).with_context(|| {
        format!(
            "failed to write WhatsApp pairing status {}",
            status_path.display()
        )
    })?;
    Ok(status)
}

fn latest_pairing_artifact(root: &Path) -> Value {
    [
        communication_runtime::artifacts_dir(root, "whatsapp").join("pairing-status.json"),
        root.join("runtime/communication/artifacts/whatsapp/pairing-status.json"),
    ]
    .into_iter()
    .find_map(|status_path| {
        fs::read_to_string(&status_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    })
    .unwrap_or_else(|| json!({"status": "missing"}))
}

fn qr_svg(code: &str) -> Result<String> {
    let qr = QrCode::new(code.as_bytes()).context("failed to build WhatsApp QR")?;
    let width = qr.width();
    let module = 8usize;
    let border = 4usize;
    let size = (width + border * 2) * module;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" width="{size}" height="{size}"><rect width="100%" height="100%" fill="#fff"/>"##
    );
    for y in 0..width {
        for x in 0..width {
            if matches!(qr[(x, y)], QrColor::Dark) {
                let px = (x + border) * module;
                let py = (y + border) * module;
                svg.push_str(&format!(
                    r##"<rect x="{px}" y="{py}" width="{module}" height="{module}" fill="#111"/>"##
                ));
            }
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
}

fn resolve_destination_jid(request: &OwnedWhatsappSendRequest) -> Result<Jid> {
    if let Some(to) = request.to.iter().find(|value| !value.trim().is_empty()) {
        return parse_whatsapp_jid(to);
    }
    if let Some(chat) = chat_jid_from_thread_key(&request.thread_key) {
        return parse_whatsapp_jid(&chat);
    }
    bail!("WhatsApp send requires --to unless --thread-key contains a WhatsApp chat JID")
}

fn parse_whatsapp_jid(value: &str) -> Result<Jid> {
    let trimmed = value.trim();
    if trimmed.contains('@') {
        return trimmed
            .parse::<Jid>()
            .with_context(|| format!("invalid WhatsApp JID: {trimmed}"));
    }
    let digits = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        bail!("invalid WhatsApp recipient: {trimmed}");
    }
    Ok(Jid::new(digits, whatsapp::types::jid::server::DEFAULT_USER))
}

fn whatsapp_message_type(message: &IncomingMessage) -> &'static str {
    let proto = message.proto.as_ref();
    if proto.conversation.is_some() {
        "text"
    } else if proto.extended_text_message.is_some() {
        "extended_text"
    } else if proto.image_message.is_some() {
        "image"
    } else if proto.video_message.is_some() {
        "video"
    } else if proto.audio_message.is_some() {
        "audio"
    } else if proto.document_message.is_some() {
        "document"
    } else if proto.sticker_message.is_some() {
        "sticker"
    } else if proto.reaction_message.is_some() {
        "reaction"
    } else {
        "other"
    }
}

fn timestamp_to_iso(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn known_message(conn: &rusqlite::Connection, message_key: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM communication_messages WHERE message_key = ?1 LIMIT 1",
            rusqlite::params![message_key],
            |_| Ok(()),
        )
        .is_ok())
}

fn account_key_from_jid(jid: &str) -> String {
    format!("whatsapp:{}", jid.trim())
}

fn address_from_account_key(account_key: &str) -> String {
    account_key
        .strip_prefix("whatsapp:")
        .unwrap_or(account_key)
        .to_string()
}

fn thread_key_for_chat(account_key: &str, chat_jid: &str) -> String {
    format!("{account_key}::chat::{chat_jid}")
}

fn chat_jid_from_thread_key(thread_key: &str) -> Option<String> {
    thread_key
        .split_once("::chat::")
        .map(|(_, chat)| chat.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_device_db_path(root: &Path) -> PathBuf {
    communication_runtime::migration_aware_state_file(
        root,
        "whatsapp",
        "device.sqlite3",
        &[legacy_default_device_db_path(root)],
    )
}

fn legacy_default_device_db_path(root: &Path) -> PathBuf {
    root.join("runtime/communication/whatsapp/device.sqlite3")
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path must be valid UTF-8: {}", path.display()))
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|items| items.first().map(String::as_str) == Some(flag))
        .and_then(|items| items.get(1))
        .map(String::as_str)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|value| value == flag)
}

fn parse_u64(value: &str, flag: &str) -> Result<u64> {
    value
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid {flag}: {value}"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize> {
    value
        .trim()
        .parse::<usize>()
        .with_context(|| format!("invalid {flag}: {value}"))
}

fn has_document_attachment(paths: &[String]) -> bool {
    paths.iter().any(|path| !is_jpeg_file(Path::new(path)))
}

fn is_jpeg_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            lowered == "jpg" || lowered == "jpeg"
        })
        .unwrap_or(false)
}

fn mime_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("txt") | Some("md") => "text/plain",
        Some("html") => "text/html",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_recipients_normalize_to_user_jids() {
        let jid = parse_whatsapp_jid("+49 151 123456").expect("parse phone");
        assert_eq!(jid.to_string(), "49151123456@s.whatsapp.net");
    }

    #[test]
    fn thread_keys_carry_reply_chat_jids() {
        let thread_key = thread_key_for_chat("whatsapp:bot@s.whatsapp.net", "123@s.whatsapp.net");
        assert_eq!(
            chat_jid_from_thread_key(&thread_key).as_deref(),
            Some("123@s.whatsapp.net")
        );
    }

    #[test]
    fn qr_svg_renders_scan_payload() {
        let svg = qr_svg("test-pairing-code").expect("qr svg");
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("<rect"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn default_device_db_uses_namespaced_state_path_for_new_accounts() {
        let root = std::env::temp_dir().join(format!("ctox-whatsapp-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            default_device_db_path(&root),
            root.join("runtime/communication/whatsapp/state/device.sqlite3")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn default_device_db_preserves_existing_legacy_pairing_store() {
        let root =
            std::env::temp_dir().join(format!("ctox-whatsapp-legacy-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("runtime/communication/whatsapp/device.sqlite3");
        std::fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("legacy parent");
        std::fs::write(&legacy, b"legacy").expect("legacy db");
        assert_eq!(default_device_db_path(&root), legacy);
        let _ = std::fs::remove_dir_all(&root);
    }
}
