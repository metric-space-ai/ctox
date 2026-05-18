use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use mailparse::{parse_mail, DispositionType, MailHeaderMap, ParsedMail};
use native_tls::{TlsConnector, TlsStream};
use roxmltree::Document;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

use crate::communication::adapters::{
    AdapterSyncCommandRequest, EmailSendCommandRequest, EmailTestCommandRequest,
};
use crate::communication::attachments::{load_outbound_attachments, refs_for_paths};
use crate::communication::microsoft_graph_auth::{
    acquire_app_token, acquire_ropc_token, ROPC_PUBLIC_CLIENT_ID,
};
use crate::communication::runtime as communication_runtime;
use crate::mission::channels::{
    ensure_account, ensure_routing_rows_for_inbound, now_iso_string, open_channel_db, preview_text,
    record_communication_sync_run, refresh_thread, stable_digest, upsert_communication_message,
    CommunicationSyncRun, UpsertMessage,
};

const DEFAULT_IMAP_HOST: &str = "imap.one.com";
const DEFAULT_IMAP_PORT: u16 = 993;
const DEFAULT_SMTP_HOST: &str = "send.one.com";
const DEFAULT_SMTP_PORT: u16 = 465;
const DEFAULT_FOLDER: &str = "INBOX";
const DEFAULT_LIMIT: usize = 20;
const DEFAULT_GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";
const DEFAULT_GRAPH_USER: &str = "me";
const DEFAULT_EWS_VERSION: &str = "Exchange2013";
const DEFAULT_EWS_AUTH_TYPE: &str = "basic";
const DEFAULT_ACTIVE_SYNC_PATH: &str = "Microsoft-Server-ActiveSync";
const DEFAULT_ACTIVE_SYNC_DEVICE_TYPE: &str = "CodexCLI";
const DEFAULT_ACTIVE_SYNC_PROTOCOL_VERSION: &str = "14.1";
const DEFAULT_ACTIVE_SYNC_POLICY_KEY: &str = "0";
const DEFAULT_TRUST_LEVEL: &str = "low";
const DEFAULT_SENT_VERIFY_WINDOW_SECONDS: u64 = 90;

#[derive(Clone, Debug)]
struct EmailOptions {
    db_path: PathBuf,
    raw_dir: PathBuf,
    email: String,
    provider: String,
    folder: String,
    limit: usize,
    trust_level: String,
    verify_send: bool,
    sent_verify_window_seconds: u64,
    imap_host: String,
    imap_port: u16,
    smtp_host: String,
    smtp_port: u16,
    password: String,
    graph_access_token: String,
    graph_base_url: String,
    graph_user: String,
    graph_tenant_id: String,
    graph_client_id: String,
    graph_client_secret: String,
    graph_username: String,
    graph_password: String,
    ews_url: String,
    owa_url: String,
    ews_version: String,
    ews_auth_type: String,
    ews_username: String,
    ews_bearer_token: String,
    active_sync_server: String,
    active_sync_username: String,
    active_sync_path: String,
    active_sync_device_id: String,
    active_sync_device_type: String,
    active_sync_protocol_version: String,
    active_sync_policy_key: String,
}

#[derive(Clone, Debug)]
struct ParsedEmailMessage {
    subject: String,
    from_header: String,
    to_header: String,
    cc_header: String,
    message_id: String,
    references: String,
    in_reply_to: String,
    sent_at_iso: Option<String>,
    body_text: String,
    body_html: String,
    has_attachments: bool,
    attachments: Vec<Value>,
    /// RFC 3834 `Auto-Submitted` marker. Populated when the inbound
    /// header is present and its value is anything other than `no` /
    /// empty. We intentionally key off the structured RFC field instead
    /// of subject- or body-pattern matching, so the core remains free of
    /// language- or template-specific heuristics.
    auto_submitted: bool,
    /// The raw Auto-Submitted header value (e.g. `auto-replied`,
    /// `auto-generated`) so downstream skills can branch on the
    /// structured value when that is useful.
    auto_submitted_value: Option<String>,
    /// Defense-in-depth: Outlook/Exchange and Notes use the
    /// `X-Auto-Response-Suppress` header on auto-responders (and on
    /// out-of-office assistant mails) to avoid loops. Treating the
    /// presence of this header as a non-actionable marker is a
    /// structured deterministic check (not a string-pattern match).
    auto_response_suppress: bool,
}

#[derive(Clone, Debug)]
struct MailboxMessage {
    remote_id: String,
    thread_key: String,
    folder_hint: String,
    subject: String,
    sender_display: String,
    sender_address: String,
    recipient_addresses: Vec<String>,
    cc_addresses: Vec<String>,
    body_text: String,
    body_html: String,
    preview: String,
    seen: bool,
    has_attachments: bool,
    external_created_at: String,
    metadata: Value,
}

#[derive(Clone, Debug)]
struct DeliveryStatus {
    confirmed: bool,
    skipped: bool,
    method: String,
    detail: Option<String>,
    remote_id: Option<String>,
    thread_key: Option<String>,
    observed_at: Option<String>,
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
    request: &EmailSendCommandRequest<'_>,
) -> Result<Value> {
    let options = send_options_from_request(root, runtime, request)?;
    execute_send(&options, request)
}

pub(crate) fn test(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &EmailTestCommandRequest<'_>,
) -> Result<Value> {
    let options = test_options_from_request(root, runtime, request)?;
    execute_test(&options)
}

pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let email = setting(settings, "CTO_EMAIL_ADDRESS");
    if email.is_empty() {
        return Ok(None);
    }
    let db_path = root.join("runtime/ctox.sqlite3");
    let mut args = vec!["sync".to_string(), "--email".to_string(), email];
    if let Some(provider) = settings
        .get("CTO_EMAIL_PROVIDER")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        args.push("--provider".to_string());
        args.push(provider.to_string());
    }
    for (key, flag) in [
        ("CTO_EMAIL_IMAP_HOST", "--imap-host"),
        ("CTO_EMAIL_IMAP_PORT", "--imap-port"),
        ("CTO_EMAIL_SMTP_HOST", "--smtp-host"),
        ("CTO_EMAIL_SMTP_PORT", "--smtp-port"),
        ("CTO_EMAIL_GRAPH_USER", "--graph-user"),
        ("CTO_EMAIL_GRAPH_TENANT_ID", "--graph-tenant-id"),
        ("CTO_EMAIL_GRAPH_CLIENT_ID", "--graph-client-id"),
        ("CTO_EMAIL_GRAPH_USERNAME", "--graph-username"),
        ("CTO_EMAIL_EWS_URL", "--ews-url"),
        ("CTO_EMAIL_OWA_URL", "--owa-url"),
        ("CTO_EMAIL_EWS_VERSION", "--ews-version"),
        ("CTO_EMAIL_EWS_AUTH_TYPE", "--ews-auth-type"),
        ("CTO_EMAIL_EWS_USERNAME", "--ews-username"),
        ("CTO_EMAIL_ACTIVESYNC_SERVER", "--active-sync-server"),
        ("CTO_EMAIL_ACTIVESYNC_USERNAME", "--active-sync-username"),
        ("CTO_EMAIL_ACTIVESYNC_PATH", "--active-sync-path"),
        ("CTO_EMAIL_ACTIVESYNC_DEVICE_ID", "--active-sync-device-id"),
        (
            "CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE",
            "--active-sync-device-type",
        ),
        (
            "CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION",
            "--active-sync-protocol-version",
        ),
        (
            "CTO_EMAIL_ACTIVESYNC_POLICY_KEY",
            "--active-sync-policy-key",
        ),
    ] {
        if let Some(value) = settings
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            args.push(flag.to_string());
            args.push(value.to_string());
        }
    }
    let runtime = runtime_from_settings(root, settings);
    let request = AdapterSyncCommandRequest {
        db_path: db_path.as_path(),
        passthrough_args: &args,
        skip_flags: &["--db", "--channel"],
    };
    sync(root, &runtime, &request).map(Some)
}

fn execute_send(options: &EmailOptions, request: &EmailSendCommandRequest<'_>) -> Result<Value> {
    require_provider_credentials(options)?;
    if request.to.is_empty() {
        bail!("Need at least one --to recipient.");
    }
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_from_email(&options.email);
    ensure_account(
        &mut conn,
        &account_key,
        "email",
        &options.email,
        &options.provider,
        build_profile_json(options),
    )?;
    let message_id = generated_message_id(&options.email, request.subject, request.body);
    let send_started_at = now_iso_string();

    match options.provider.as_str() {
        "imap" => {
            let mut smtp = SmtpClient::connect(options)?;
            smtp.login(&options.email, &options.password)?;
            let raw_message = build_smtp_raw_message(
                &options.email,
                request.to,
                request.cc,
                request.subject,
                request.body,
                &message_id,
                request.thread_key,
                request.attachments,
            )?;
            smtp.send_mail(&options.email, request.to, request.cc, &[], &raw_message)?;
            smtp.close();
        }
        "graph" => {
            GraphClient::from_options(options)?.send_mail(
                request.subject,
                request.body,
                request.to,
                request.cc,
                &[],
                request.attachments,
            )?;
        }
        "ews" | "owa" => {
            EwsClient::from_options(options)?.send_mail(
                request.subject,
                request.body,
                request.to,
                request.cc,
                &[],
                request.attachments,
            )?;
        }
        "activesync" => {
            bail!("ActiveSync outbound send is not implemented in the native Rust adapter.");
        }
        other => bail!("Unsupported email provider: {other}"),
    }

    let attachment_refs = refs_for_paths(request.attachments)?;
    let delivery = verify_sent_delivery(options, request, &message_id, &send_started_at)?;
    let observed_at = now_iso_string();
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key_from_remote(&account_key, "sent", &message_id),
            channel: "email",
            account_key: &account_key,
            thread_key: request.thread_key,
            remote_id: &message_id,
            direction: "outbound",
            folder_hint: "sent",
            sender_display: request.sender_display.unwrap_or(""),
            sender_address: &options.email,
            recipient_addresses_json: &serde_json::to_string(
                &request
                    .to
                    .iter()
                    .map(|value| value.trim().to_lowercase())
                    .collect::<Vec<_>>(),
            )?,
            cc_addresses_json: &serde_json::to_string(
                &request
                    .cc
                    .iter()
                    .map(|value| value.trim().to_lowercase())
                    .collect::<Vec<_>>(),
            )?,
            bcc_addresses_json: "[]",
            subject: request.subject,
            preview: &preview_text(request.body, request.subject),
            body_text: request.body,
            body_html: "",
            raw_payload_ref: &request.attachments.join("\n"),
            trust_level: &options.trust_level,
            status: if delivery.confirmed {
                "confirmed"
            } else {
                "accepted"
            },
            seen: true,
            has_attachments: !request.attachments.is_empty(),
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&json!({
                "messageId": message_id,
                "delivery": delivery_json(&delivery),
                "attachments": attachment_refs,
            }))?,
        },
    )?;
    refresh_thread(&mut conn, request.thread_key)?;
    Ok(json!({
        "ok": true,
        "accountKey": account_key,
        "to": request.to,
        "subject": request.subject,
        "messageId": message_id,
        "status": if delivery.confirmed { "confirmed" } else { "accepted" },
        "delivery": delivery_json(&delivery),
        "dbPath": options.db_path,
    }))
}

fn execute_test(options: &EmailOptions) -> Result<Value> {
    require_provider_credentials(options)?;
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_from_email(&options.email);
    ensure_account(
        &mut conn,
        &account_key,
        "email",
        &options.email,
        &options.provider,
        build_profile_json(options),
    )?;

    let mut checks = Vec::<Value>::new();
    match options.provider.as_str() {
        "imap" => {
            let mut imap = ImapClient::connect(options)?;
            checks.push(json!({"name":"imap_connect","ok":true}));
            imap.login(&options.email, &options.password)?;
            checks.push(json!({"name":"imap_login","ok":true}));
            imap.select("INBOX")?;
            checks.push(json!({"name":"imap_inbox_select","ok":true}));
            let sent_folders = resolve_imap_sent_folders(&mut imap).unwrap_or_default();
            checks.push(json!({
                "name":"imap_sent_folder_probe",
                "ok": true,
                "detail": if sent_folders.is_empty() {
                    "no sent mailbox detected".to_string()
                } else {
                    sent_folders.join(", ")
                }
            }));
            imap.logout();

            let subject = format!("[CTOX mail self-test] {}", now_iso_string());
            let body = format!(
                "CTOX self-test {}",
                generated_message_id(&options.email, &subject, &account_key)
            );
            let message_id = generated_message_id(&options.email, &subject, &body);
            let send_started_at = now_iso_string();

            let mut smtp = SmtpClient::connect(options)?;
            smtp.login(&options.email, &options.password)?;
            checks.push(json!({"name":"smtp_login","ok":true,"detail":smtp.connection_mode()}));
            let raw_message = build_smtp_raw_message(
                &options.email,
                &[options.email.clone()],
                &[],
                &subject,
                &body,
                &message_id,
                "",
                &[],
            )?;
            smtp.send_mail(
                &options.email,
                &[options.email.clone()],
                &[],
                &[],
                &raw_message,
            )?;
            smtp.close();
            checks.push(json!({"name":"smtp_self_send","ok":true}));

            let inbox_check =
                verify_imap_inbox_delivery(options, &message_id, Some(&send_started_at))?;
            checks.push(json!({
                "name":"imap_self_delivery",
                "ok": inbox_check.confirmed,
                "detail": inbox_check.detail
                    .clone()
                    .or(inbox_check.remote_id.clone())
                    .unwrap_or_else(|| inbox_check.method.clone()),
            }));
            if !inbox_check.confirmed {
                bail!(
                    "Mail self-test did not roundtrip into inbox: {}",
                    inbox_check.detail.as_deref().unwrap_or("unknown error")
                );
            }
        }
        "graph" => {
            let client = GraphClient::from_options(options)?;
            client.list_folder("inbox", 1)?;
            checks.push(json!({"name":"graph_inbox_probe","ok":true}));
            client.list_folder("sentitems", 1)?;
            checks.push(json!({"name":"graph_sent_probe","ok":true}));
        }
        "ews" | "owa" => {
            let client = EwsClient::from_options(options)?;
            client.list_folder("inbox", 1, None)?;
            checks.push(json!({"name":"ews_inbox_probe","ok":true}));
            client.list_folder("sentitems", 1, None)?;
            checks.push(json!({"name":"ews_sent_probe","ok":true}));
        }
        "activesync" => {
            let mut client = ActiveSyncClient::from_options(options)?;
            client.options()?;
            checks.push(json!({"name":"activesync_options","ok":true}));
            client.list_folder("inbox", 1, None)?;
            checks.push(json!({"name":"activesync_inbox_probe","ok":true}));
            client.list_folder("sentitems", 1, None)?;
            checks.push(json!({"name":"activesync_sent_probe","ok":true}));
        }
        other => bail!("Unsupported email provider: {other}"),
    }

    Ok(json!({
        "ok": true,
        "channel": "email",
        "provider": options.provider,
        "accountKey": account_key,
        "checks": checks,
        "dbPath": options.db_path,
    }))
}

fn execute_sync(options: &EmailOptions) -> Result<Value> {
    require_provider_credentials(options)?;
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_from_email(&options.email);
    ensure_account(
        &mut conn,
        &account_key,
        "email",
        &options.email,
        &options.provider,
        build_profile_json(options),
    )?;
    let started_at = now_iso_string();
    let mut fetched_count = 0i64;
    let mut stored_count = 0i64;

    let sync_result = (|| -> Result<()> {
        match options.provider.as_str() {
            "imap" => {
                let mut imap = ImapClient::connect(options)?;
                imap.login(&options.email, &options.password)?;
                imap.select(&options.folder)?;
                let selected = latest_imap_uids(imap.search_all_uids()?, options.limit);
                fetched_count = selected.len() as i64;
                for uid in selected {
                    let message_key = message_key_from_remote(&account_key, &options.folder, &uid);
                    if known_communication_message(&conn, &message_key)? {
                        continue;
                    }
                    let fetched = imap.fetch_raw(&uid)?;
                    let parsed = parse_rfc822_message(&fetched.raw)?;
                    let sender_address = extract_address(&parsed.from_header);
                    let sender_display = extract_display_name(&parsed.from_header)
                        .unwrap_or_else(|| sender_address.clone());
                    let direction = synced_message_direction(&sender_address, &options.email);
                    let thread_key =
                        thread_key_from_email(&parsed, &format!("{account_key}::{uid}"));
                    let observed_at = now_iso_string();
                    let raw_payload_ref = write_raw_payload(&options.raw_dir, &uid, &fetched.raw)?;
                    let technical_self_test = is_ctox_mail_self_test(
                        &parsed.subject,
                        &parsed.body_text,
                        &sender_address,
                        &options.email,
                    );
                    upsert_communication_message(
                        &mut conn,
                        UpsertMessage {
                            message_key: &message_key,
                            channel: "email",
                            account_key: &account_key,
                            thread_key: &thread_key,
                            remote_id: &uid,
                            direction,
                            folder_hint: &options.folder,
                            sender_display: &sender_display,
                            sender_address: &sender_address,
                            recipient_addresses_json: &serde_json::to_string(&extract_addresses(
                                &parsed.to_header,
                            ))?,
                            cc_addresses_json: &serde_json::to_string(&extract_addresses(
                                &parsed.cc_header,
                            ))?,
                            bcc_addresses_json: "[]",
                            subject: &parsed.subject,
                            preview: &preview_text(&parsed.body_text, &parsed.subject),
                            body_text: &parsed.body_text,
                            body_html: &parsed.body_html,
                            raw_payload_ref: &raw_payload_ref,
                            trust_level: if technical_self_test {
                                "system_probe"
                            } else {
                                &options.trust_level
                            },
                            status: if technical_self_test {
                                "self_test_received"
                            } else {
                                "received"
                            },
                            seen: fetched
                                .flags
                                .iter()
                                .any(|flag| flag.eq_ignore_ascii_case("\\Seen")),
                            has_attachments: parsed.has_attachments,
                            external_created_at: parsed
                                .sent_at_iso
                                .as_deref()
                                .unwrap_or(&observed_at),
                            observed_at: &observed_at,
                            metadata_json: &serde_json::to_string(&json!({
                                "messageId": parsed.message_id,
                                "references": parsed.references,
                                "inReplyTo": parsed.in_reply_to,
                                "imapFlags": fetched.flags,
                                "technicalSelfTest": technical_self_test,
                                "autoSubmitted": parsed.auto_submitted
                                    || parsed.auto_response_suppress,
                                "autoSubmittedValue": parsed.auto_submitted_value,
                                "autoResponseSuppress": parsed.auto_response_suppress,
                                "attachments": parsed.attachments,
                            }))?,
                        },
                    )?;
                    refresh_thread(&mut conn, &thread_key)?;
                    stored_count += 1;
                }
                imap.logout();
            }
            "graph" => {
                let client = GraphClient::from_options(options)?;
                let items = client.list_folder(
                    &folder_hint_to_mailbox_folder(&options.folder),
                    options.limit,
                )?;
                fetched_count = items.len() as i64;
                for item in items {
                    if store_provider_message(&mut conn, options, &account_key, item)? {
                        stored_count += 1;
                    }
                }
            }
            "ews" | "owa" => {
                let client = EwsClient::from_options(options)?;
                let items = client.list_folder(
                    &folder_hint_to_mailbox_folder(&options.folder),
                    options.limit,
                    None,
                )?;
                fetched_count = items.len() as i64;
                for item in items {
                    store_provider_message(&mut conn, options, &account_key, item)?;
                    stored_count += 1;
                }
            }
            "activesync" => {
                let mut client = ActiveSyncClient::from_options(options)?;
                let items = client.list_folder(
                    &folder_hint_to_mailbox_folder(&options.folder),
                    options.limit,
                    None,
                )?;
                fetched_count = items.len() as i64;
                for item in items {
                    store_provider_message(&mut conn, options, &account_key, item)?;
                    stored_count += 1;
                }
            }
            other => bail!("Unsupported email provider: {other}"),
        }
        ensure_routing_rows_for_inbound(&conn)?;
        Ok(())
    })();

    let finished_at = now_iso_string();
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &generated_sync_run_key(&account_key, &options.folder, &started_at),
            channel: "email",
            account_key: &account_key,
            folder_hint: &options.folder,
            started_at: &started_at,
            finished_at: &finished_at,
            ok: sync_result.is_ok(),
            fetched_count,
            stored_count,
            error_text: sync_result
                .as_ref()
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default()
                .as_str(),
            metadata_json: &serde_json::to_string(&json!({
                "adapter": "native-rust-email",
                "provider": options.provider,
            }))?,
        },
    )?;
    sync_result?;

    Ok(json!({
        "ok": true,
        "accountKey": account_key,
        "folder": options.folder,
        "fetchedCount": fetched_count,
        "storedCount": stored_count,
        "dbPath": options.db_path,
    }))
}

fn generated_sync_run_key(account_key: &str, folder: &str, started_at: &str) -> String {
    format!(
        "sync-{}",
        stable_digest(&format!("{account_key}:{folder}:{started_at}"))
    )
}

fn store_provider_message(
    conn: &mut Connection,
    options: &EmailOptions,
    account_key: &str,
    item: MailboxMessage,
) -> Result<bool> {
    let message_key = message_key_from_remote(account_key, &item.folder_hint, &item.remote_id);
    if known_communication_message(conn, &message_key)? {
        return Ok(false);
    }
    let observed_at = now_iso_string();
    let direction = synced_message_direction(&item.sender_address, &options.email);
    let raw_payload_ref = provider_attachment_refs(&item.metadata).join("\n");
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "email",
            account_key,
            thread_key: &item.thread_key,
            remote_id: &item.remote_id,
            direction,
            folder_hint: &item.folder_hint,
            sender_display: &item.sender_display,
            sender_address: &item.sender_address,
            recipient_addresses_json: &serde_json::to_string(&item.recipient_addresses)?,
            cc_addresses_json: &serde_json::to_string(&item.cc_addresses)?,
            bcc_addresses_json: "[]",
            subject: &item.subject,
            preview: &item.preview,
            body_text: &item.body_text,
            body_html: &item.body_html,
            raw_payload_ref: &raw_payload_ref,
            trust_level: &options.trust_level,
            status: "received",
            seen: item.seen,
            has_attachments: item.has_attachments,
            external_created_at: &item.external_created_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&item.metadata)?,
        },
    )?;
    refresh_thread(conn, &item.thread_key)?;
    Ok(true)
}

fn provider_attachment_refs(metadata: &Value) -> Vec<String> {
    metadata
        .get("attachments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attachment| {
            attachment
                .get("contentUrl")
                .and_then(Value::as_str)
                .or_else(|| attachment.get("name").and_then(Value::as_str))
                .map(str::to_string)
        })
        .collect()
}

fn known_communication_message(conn: &Connection, message_key: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM communication_messages WHERE message_key = ?1 LIMIT 1",
            [message_key],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

fn sync_options_from_args(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<EmailOptions> {
    let args = request.passthrough_args;
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    options.email = required_flag(args, "--email")
        .map(str::to_string)
        .unwrap_or_else(|_| setting(runtime, "CTO_EMAIL_ADDRESS"));
    if options.email.trim().is_empty() {
        bail!("Missing --email or CTO_EMAIL_ADDRESS.");
    }
    if let Some(provider) = optional_flag(args, "--provider") {
        options.provider = normalize_provider(provider);
    }
    if let Some(folder) = optional_flag(args, "--folder") {
        options.folder = folder.to_string();
    }
    if let Some(limit) = optional_flag(args, "--limit") {
        options.limit = limit.parse::<usize>().unwrap_or(DEFAULT_LIMIT);
    }
    if let Some(trust_level) = optional_flag(args, "--trust-level") {
        options.trust_level = trust_level.to_string();
    }
    apply_flag_overrides(&mut options, args);
    Ok(options)
}

fn send_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &EmailSendCommandRequest<'_>,
) -> Result<EmailOptions> {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    options.email = request.sender_email.trim().to_lowercase();
    if options.email.is_empty() {
        bail!("Missing sender email.");
    }
    if let Some(provider) = request.provider {
        options.provider = normalize_provider(provider);
    }
    if let Some(profile_json) = request.profile_json {
        apply_profile_json(&mut options, profile_json);
    }
    Ok(options)
}

fn test_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &EmailTestCommandRequest<'_>,
) -> Result<EmailOptions> {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    options.email = request.email_address.trim().to_lowercase();
    options.provider = normalize_provider(request.provider);
    apply_profile_json(&mut options, request.profile_json);
    Ok(options)
}

fn runtime_from_settings(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut runtime = BTreeMap::new();
    if let Ok(from_env) = crate::inference::runtime_env::load_runtime_env_map(root) {
        runtime.extend(from_env);
    }
    runtime.extend(settings.clone());
    runtime
}

fn base_options_from_runtime(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    db_path: &Path,
) -> EmailOptions {
    EmailOptions {
        db_path: db_path.to_path_buf(),
        raw_dir: communication_runtime::raw_dir(root, "email"),
        email: setting(runtime, "CTO_EMAIL_ADDRESS"),
        provider: normalize_provider(&setting(runtime, "CTO_EMAIL_PROVIDER")),
        folder: optional_setting(runtime, "CTO_EMAIL_FOLDER")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_FOLDER.to_string()),
        limit: runtime
            .get("CTO_EMAIL_LIMIT")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_LIMIT),
        trust_level: optional_setting(runtime, "CTO_EMAIL_TRUST_LEVEL")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_TRUST_LEVEL.to_string()),
        verify_send: truthy(&setting(runtime, "CTO_EMAIL_VERIFY_SEND"), true),
        sent_verify_window_seconds: runtime
            .get("CTO_EMAIL_SENT_VERIFY_WINDOW_SECONDS")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_SENT_VERIFY_WINDOW_SECONDS),
        imap_host: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_IMAP_HOST"),
            DEFAULT_IMAP_HOST,
        ),
        imap_port: runtime
            .get("CTO_EMAIL_IMAP_PORT")
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(DEFAULT_IMAP_PORT),
        smtp_host: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_SMTP_HOST"),
            DEFAULT_SMTP_HOST,
        ),
        smtp_port: runtime
            .get("CTO_EMAIL_SMTP_PORT")
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(DEFAULT_SMTP_PORT),
        password: setting(runtime, "CTO_EMAIL_PASSWORD"),
        graph_access_token: setting(runtime, "CTO_EMAIL_GRAPH_ACCESS_TOKEN"),
        graph_base_url: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_GRAPH_BASE_URL"),
            DEFAULT_GRAPH_BASE_URL,
        ),
        graph_user: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_GRAPH_USER"),
            DEFAULT_GRAPH_USER,
        ),
        graph_tenant_id: setting(runtime, "CTO_EMAIL_GRAPH_TENANT_ID"),
        graph_client_id: setting(runtime, "CTO_EMAIL_GRAPH_CLIENT_ID"),
        graph_client_secret: setting(runtime, "CTO_EMAIL_GRAPH_CLIENT_SECRET"),
        graph_username: setting(runtime, "CTO_EMAIL_GRAPH_USERNAME"),
        graph_password: setting(runtime, "CTO_EMAIL_GRAPH_PASSWORD"),
        ews_url: setting(runtime, "CTO_EMAIL_EWS_URL"),
        owa_url: setting(runtime, "CTO_EMAIL_OWA_URL"),
        ews_version: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_EWS_VERSION"),
            DEFAULT_EWS_VERSION,
        ),
        ews_auth_type: normalize_ews_auth_type(&setting(runtime, "CTO_EMAIL_EWS_AUTH_TYPE")),
        ews_username: setting(runtime, "CTO_EMAIL_EWS_USERNAME"),
        ews_bearer_token: setting(runtime, "CTO_EMAIL_EWS_BEARER_TOKEN"),
        active_sync_server: setting(runtime, "CTO_EMAIL_ACTIVESYNC_SERVER"),
        active_sync_username: setting(runtime, "CTO_EMAIL_ACTIVESYNC_USERNAME"),
        active_sync_path: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_ACTIVESYNC_PATH"),
            DEFAULT_ACTIVE_SYNC_PATH,
        ),
        active_sync_device_id: setting(runtime, "CTO_EMAIL_ACTIVESYNC_DEVICE_ID"),
        active_sync_device_type: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE"),
            DEFAULT_ACTIVE_SYNC_DEVICE_TYPE,
        ),
        active_sync_protocol_version: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION"),
            DEFAULT_ACTIVE_SYNC_PROTOCOL_VERSION,
        ),
        active_sync_policy_key: non_empty_or_default(
            optional_setting(runtime, "CTO_EMAIL_ACTIVESYNC_POLICY_KEY"),
            DEFAULT_ACTIVE_SYNC_POLICY_KEY,
        ),
    }
}

fn apply_flag_overrides(options: &mut EmailOptions, args: &[String]) {
    if let Some(value) = optional_flag(args, "--imap-host") {
        options.imap_host = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--smtp-host") {
        options.smtp_host = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--graph-user") {
        options.graph_user = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--graph-base-url") {
        options.graph_base_url = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--graph-tenant-id") {
        options.graph_tenant_id = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--graph-client-id") {
        options.graph_client_id = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--graph-username") {
        options.graph_username = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--ews-url") {
        options.ews_url = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--owa-url") {
        options.owa_url = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--ews-version") {
        options.ews_version = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--ews-auth-type") {
        options.ews_auth_type = normalize_ews_auth_type(value);
    }
    if let Some(value) = optional_flag(args, "--ews-username") {
        options.ews_username = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-server") {
        options.active_sync_server = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-username") {
        options.active_sync_username = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-path") {
        options.active_sync_path = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-device-id") {
        options.active_sync_device_id = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-device-type") {
        options.active_sync_device_type = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-protocol-version") {
        options.active_sync_protocol_version = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--active-sync-policy-key") {
        options.active_sync_policy_key = value.to_string();
    }
    if let Some(value) = optional_flag(args, "--imap-port") {
        options.imap_port = value.parse::<u16>().unwrap_or(DEFAULT_IMAP_PORT);
    }
    if let Some(value) = optional_flag(args, "--smtp-port") {
        options.smtp_port = value.parse::<u16>().unwrap_or(DEFAULT_SMTP_PORT);
    }
    if let Some(value) = optional_flag(args, "--raw-dir") {
        options.raw_dir = PathBuf::from(value);
    }
}

fn apply_profile_json(options: &mut EmailOptions, profile_json: &Value) {
    let Some(object) = profile_json.as_object() else {
        return;
    };
    if let Some(provider) = object.get("provider").and_then(Value::as_str) {
        options.provider = normalize_provider(provider);
    }
    if let Some(folder) = object.get("folder").and_then(Value::as_str) {
        options.folder = folder.to_string();
    }
    if let Some(value) = object.get("imapHost").and_then(Value::as_str) {
        options.imap_host = value.to_string();
    }
    if let Some(value) = object.get("imapPort").and_then(number_like_u16) {
        options.imap_port = value;
    }
    if let Some(value) = object.get("smtpHost").and_then(Value::as_str) {
        options.smtp_host = value.to_string();
    }
    if let Some(value) = object.get("smtpPort").and_then(number_like_u16) {
        options.smtp_port = value;
    }
    if let Some(value) = object.get("graphBaseUrl").and_then(Value::as_str) {
        options.graph_base_url = value.to_string();
    }
    if let Some(value) = object.get("graphUser").and_then(Value::as_str) {
        options.graph_user = value.to_string();
    }
    if let Some(value) = object.get("graphTenantId").and_then(Value::as_str) {
        options.graph_tenant_id = value.to_string();
    }
    if let Some(value) = object.get("graphClientId").and_then(Value::as_str) {
        options.graph_client_id = value.to_string();
    }
    if let Some(value) = object.get("graphUsername").and_then(Value::as_str) {
        options.graph_username = value.to_string();
    }
    if let Some(value) = object.get("ewsUrl").and_then(Value::as_str) {
        options.ews_url = value.to_string();
    }
    if let Some(value) = object.get("owaUrl").and_then(Value::as_str) {
        options.owa_url = value.to_string();
    }
    if let Some(value) = object.get("ewsVersion").and_then(Value::as_str) {
        options.ews_version = value.to_string();
    }
    if let Some(value) = object.get("ewsAuthType").and_then(Value::as_str) {
        options.ews_auth_type = normalize_ews_auth_type(value);
    }
    if let Some(value) = object.get("ewsUsername").and_then(Value::as_str) {
        options.ews_username = value.to_string();
    }
    if let Some(value) = object.get("activeSyncServer").and_then(Value::as_str) {
        options.active_sync_server = value.to_string();
    }
    if let Some(value) = object.get("activeSyncUsername").and_then(Value::as_str) {
        options.active_sync_username = value.to_string();
    }
    if let Some(value) = object.get("activeSyncPath").and_then(Value::as_str) {
        options.active_sync_path = value.to_string();
    }
    if let Some(value) = object.get("activeSyncDeviceId").and_then(Value::as_str) {
        options.active_sync_device_id = value.to_string();
    }
    if let Some(value) = object.get("activeSyncDeviceType").and_then(Value::as_str) {
        options.active_sync_device_type = value.to_string();
    }
    if let Some(value) = object
        .get("activeSyncProtocolVersion")
        .and_then(Value::as_str)
    {
        options.active_sync_protocol_version = value.to_string();
    }
    if let Some(value) = object.get("activeSyncPolicyKey").and_then(Value::as_str) {
        options.active_sync_policy_key = value.to_string();
    }
}

fn build_profile_json(options: &EmailOptions) -> Value {
    json!({
        "provider": options.provider,
        "imapHost": options.imap_host,
        "imapPort": options.imap_port,
        "smtpHost": options.smtp_host,
        "smtpPort": options.smtp_port,
        "folder": options.folder,
        "graphBaseUrl": options.graph_base_url,
        "graphUser": options.graph_user,
        "graphTenantId": options.graph_tenant_id,
        "graphClientId": options.graph_client_id,
        "graphUsername": options.graph_username,
        "ewsUrl": resolve_ews_url(options),
        "owaUrl": options.owa_url,
        "ewsVersion": options.ews_version,
        "ewsAuthType": options.ews_auth_type,
        "activeSyncServer": options.active_sync_server,
        "activeSyncUsername": options.active_sync_username,
        "activeSyncPath": options.active_sync_path,
        "activeSyncDeviceId": options.active_sync_device_id,
        "activeSyncDeviceType": options.active_sync_device_type,
        "activeSyncProtocolVersion": options.active_sync_protocol_version,
        "activeSyncPolicyKey": options.active_sync_policy_key,
    })
}

fn verify_sent_delivery(
    options: &EmailOptions,
    request: &EmailSendCommandRequest<'_>,
    message_id: &str,
    earliest_iso: &str,
) -> Result<DeliveryStatus> {
    if !options.verify_send {
        return Ok(DeliveryStatus {
            confirmed: false,
            skipped: true,
            method: "disabled".to_string(),
            detail: None,
            remote_id: None,
            thread_key: None,
            observed_at: None,
        });
    }
    if options.provider == "imap" {
        let self_address = options.email.to_lowercase();
        let recipients = request
            .to
            .iter()
            .chain(request.cc.iter())
            .map(|value| value.trim().to_lowercase())
            .collect::<BTreeSet<_>>();
        if recipients.contains(&self_address) {
            return verify_imap_inbox_delivery(options, message_id, Some(earliest_iso));
        }
        return Ok(DeliveryStatus {
            confirmed: false,
            skipped: true,
            method: "smtp-accepted-external".to_string(),
            detail: Some(
                "external recipient delivery was accepted over SMTP; sent-folder confirmation is skipped for normal outbound mail"
                    .to_string(),
            ),
            remote_id: None,
            thread_key: None,
            observed_at: None,
        });
    }
    verify_provider_sent_copy(options, request, earliest_iso)
}

fn verify_provider_sent_copy(
    options: &EmailOptions,
    request: &EmailSendCommandRequest<'_>,
    earliest_iso: &str,
) -> Result<DeliveryStatus> {
    let items = match options.provider.as_str() {
        "graph" => GraphClient::from_options(options)?.list_folder("sentitems", 25)?,
        "ews" | "owa" => {
            EwsClient::from_options(options)?.list_folder("sentitems", 25, Some(request.subject))?
        }
        "activesync" => ActiveSyncClient::from_options(options)?.list_folder(
            "sentitems",
            25,
            Some(request.subject),
        )?,
        other => bail!("unsupported email provider for sent verification: {other}"),
    };
    for item in items {
        if message_looks_like_sent_copy(&item, request, earliest_iso) {
            return Ok(DeliveryStatus {
                confirmed: true,
                skipped: false,
                method: format!("{}-sent-folder", options.provider),
                detail: None,
                remote_id: Some(item.remote_id),
                thread_key: Some(item.thread_key),
                observed_at: Some(item.external_created_at),
            });
        }
    }
    Ok(DeliveryStatus {
        confirmed: false,
        skipped: false,
        method: format!("{}-sent-folder", options.provider),
        detail: Some("matching sent copy not found".to_string()),
        remote_id: None,
        thread_key: None,
        observed_at: None,
    })
}

fn message_looks_like_sent_copy(
    item: &MailboxMessage,
    request: &EmailSendCommandRequest<'_>,
    earliest_iso: &str,
) -> bool {
    if item.subject.trim() != request.subject.trim() {
        return false;
    }
    let expected = request
        .to
        .iter()
        .chain(request.cc.iter())
        .map(|value| value.trim().to_lowercase())
        .collect::<BTreeSet<_>>();
    let actual = item
        .recipient_addresses
        .iter()
        .chain(item.cc_addresses.iter())
        .map(|value| value.trim().to_lowercase())
        .collect::<BTreeSet<_>>();
    if !expected.iter().all(|value| actual.contains(value)) {
        return false;
    }
    !iso_before(&item.external_created_at, earliest_iso)
}

fn verify_imap_inbox_delivery(
    options: &EmailOptions,
    message_id: &str,
    earliest_iso: Option<&str>,
) -> Result<DeliveryStatus> {
    let attempts = options.sent_verify_window_seconds.clamp(1, 30);
    let mut imap = ImapClient::connect(options)?;
    imap.login(&options.email, &options.password)?;
    imap.select("INBOX")?;
    for attempt in 0..attempts {
        for uid in latest_imap_uids(imap.search_all_uids()?, 25) {
            let fetched = imap.fetch_raw(&uid)?;
            let parsed = parse_rfc822_message(&fetched.raw)?;
            if parsed.message_id.trim() != message_id.trim() {
                continue;
            }
            let observed_at = parsed.sent_at_iso.clone().unwrap_or_else(now_iso_string);
            if let Some(earliest) = earliest_iso {
                if iso_before(&observed_at, earliest) {
                    continue;
                }
            }
            imap.logout();
            return Ok(DeliveryStatus {
                confirmed: true,
                skipped: false,
                method: "imap-inbox-roundtrip".to_string(),
                detail: None,
                remote_id: Some(message_id.to_string()),
                thread_key: None,
                observed_at: Some(observed_at),
            });
        }
        if attempt + 1 < attempts {
            thread::sleep(Duration::from_secs(1));
        }
    }
    imap.logout();
    Ok(DeliveryStatus {
        confirmed: false,
        skipped: false,
        method: "imap-inbox-roundtrip".to_string(),
        detail: Some("message-id not found in inbox".to_string()),
        remote_id: None,
        thread_key: None,
        observed_at: None,
    })
}

fn resolve_imap_sent_folders(imap: &mut ImapClient) -> Result<Vec<String>> {
    let fallback = [
        "Sent",
        "Sent Items",
        "Sent Messages",
        "Gesendet",
        "INBOX.Sent",
        "INBOX.Sent Items",
    ];
    let mut discovered = BTreeSet::new();
    for mailbox in imap.list_mailboxes()? {
        let lower = mailbox.name.to_lowercase();
        let flags = mailbox
            .flags
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<BTreeSet<_>>();
        if flags.contains("\\sent")
            || lower.ends_with("sent")
            || lower.ends_with("sent items")
            || lower.ends_with("sent messages")
            || lower.ends_with("gesendet")
        {
            discovered.insert(mailbox.name);
        }
    }
    for item in fallback {
        discovered.insert(item.to_string());
    }
    Ok(discovered.into_iter().collect())
}

fn parse_rfc822_message(raw: &[u8]) -> Result<ParsedEmailMessage> {
    let parsed = parse_mail(raw).context("failed to parse RFC822 message")?;
    let subject = parsed
        .headers
        .get_first_value("Subject")
        .unwrap_or_else(|| "(ohne Betreff)".to_string());
    let from_header = parsed.headers.get_first_value("From").unwrap_or_default();
    let to_header = parsed.headers.get_first_value("To").unwrap_or_default();
    let cc_header = parsed.headers.get_first_value("Cc").unwrap_or_default();
    let message_id = parsed
        .headers
        .get_first_value("Message-ID")
        .unwrap_or_default();
    let references = parsed
        .headers
        .get_first_value("References")
        .unwrap_or_default();
    let in_reply_to = parsed
        .headers
        .get_first_value("In-Reply-To")
        .unwrap_or_default();
    let sent_at_iso = parsed
        .headers
        .get_first_value("Date")
        .and_then(|value| mailparse::dateparse(&value).ok())
        .and_then(epoch_seconds_to_iso);
    let auto_submitted_raw = parsed.headers.get_first_value("Auto-Submitted");
    let (auto_submitted, auto_submitted_value) =
        classify_auto_submitted_header(auto_submitted_raw.as_deref());
    let auto_response_suppress = parsed
        .headers
        .get_first_value("X-Auto-Response-Suppress")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let (body_text, body_html, attachments) = collect_mail_bodies(&parsed)?;
    let has_attachments = !attachments.is_empty();
    Ok(ParsedEmailMessage {
        subject,
        from_header,
        to_header,
        cc_header,
        message_id,
        references,
        in_reply_to,
        sent_at_iso,
        body_text,
        body_html,
        has_attachments,
        attachments,
        auto_submitted,
        auto_submitted_value,
        auto_response_suppress,
    })
}

/// Parse an `Auto-Submitted` header value per RFC 3834 §5. We treat any
/// keyword other than the literal `no` (or an empty/missing header) as
/// "this is an auto-submitted message". The full set of registered
/// keywords currently in use is `auto-generated`, `auto-replied`,
/// `auto-notified`; downstream code should not encode that list, only
/// the boolean.
fn classify_auto_submitted_header(raw: Option<&str>) -> (bool, Option<String>) {
    let Some(raw) = raw else {
        return (false, None);
    };
    // Per RFC 3834 the value is a structured token followed by optional
    // `;`-separated parameters (e.g. `auto-replied; foo=bar`). Keep only
    // the leading token, trimmed and lowercased, for the boolean check.
    let token = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if token.is_empty() || token == "no" {
        return (false, Some(raw.trim().to_string()));
    }
    (true, Some(raw.trim().to_string()))
}

fn collect_mail_bodies(parsed: &ParsedMail<'_>) -> Result<(String, String, Vec<Value>)> {
    if parsed.subparts.is_empty() {
        let mimetype = parsed.ctype.mimetype.to_lowercase();
        let disposition = parsed.get_content_disposition();
        let has_attachment = matches!(disposition.disposition, DispositionType::Attachment);
        let attachments = if has_attachment {
            let fallback_name = format!("attachment.{}", extension_for_content_type(&mimetype));
            let name = disposition
                .params
                .get("filename")
                .or_else(|| parsed.ctype.params.get("name"))
                .map(String::as_str)
                .unwrap_or(&fallback_name)
                .to_string();
            let size_bytes = parsed.get_body_raw().map(|bytes| bytes.len()).unwrap_or(0);
            vec![json!({
                "name": name,
                "contentType": mimetype.clone(),
                "sizeBytes": size_bytes,
                "source": "mime",
            })]
        } else {
            Vec::new()
        };
        let body = parsed.get_body().unwrap_or_default();
        if mimetype == "text/html" {
            return Ok((strip_html(&body), body, attachments));
        }
        if mimetype.starts_with("text/") || mimetype.is_empty() {
            return Ok((body, String::new(), attachments));
        }
        return Ok((String::new(), String::new(), attachments));
    }

    let mut body_text = String::new();
    let mut body_html = String::new();
    let mut attachments = Vec::new();
    for part in &parsed.subparts {
        let (part_text, part_html, mut part_attachments) = collect_mail_bodies(part)?;
        if body_text.is_empty() && !part_text.trim().is_empty() {
            body_text = part_text;
        } else if looks_like_calendar_text(&part_text) {
            if !body_text.is_empty() {
                body_text.push_str("\n\n");
            }
            body_text.push_str(&part_text);
        }
        if body_html.is_empty() && !part_html.trim().is_empty() {
            body_html = part_html;
        }
        attachments.append(&mut part_attachments);
    }
    if body_text.is_empty() && !body_html.is_empty() {
        body_text = strip_html(&body_html);
    }
    Ok((body_text, body_html, attachments))
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "application/pdf" => "pdf",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "text/csv" => "csv",
        "text/html" => "html",
        "text/plain" => "txt",
        _ => "bin",
    }
}

fn looks_like_calendar_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("begin:vcalendar") || lower.contains("dtstart")
}

fn thread_key_from_email(parsed: &ParsedEmailMessage, fallback: &str) -> String {
    let references = parsed
        .references
        .split_whitespace()
        .map(str::trim)
        .find(|value| !value.is_empty());
    references
        .or_else(|| {
            let trimmed = parsed.in_reply_to.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .or_else(|| {
            let trimmed = parsed.message_id.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or(fallback)
        .to_string()
}

fn is_ctox_mail_self_test(
    subject: &str,
    body_text: &str,
    sender_address: &str,
    account_email: &str,
) -> bool {
    subject
        .trim()
        .to_lowercase()
        .starts_with("[ctox mail self-test]")
        && body_text
            .trim()
            .to_lowercase()
            .starts_with("ctox self-test ")
        && !sender_address.trim().is_empty()
        && sender_address
            .trim()
            .eq_ignore_ascii_case(account_email.trim())
}

fn strip_html(input: &str) -> String {
    input
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace('\r', "")
        .chars()
        .scan(false, |inside_tag, ch| match ch {
            '<' => {
                *inside_tag = true;
                Some(None)
            }
            '>' => {
                *inside_tag = false;
                Some(None)
            }
            _ if *inside_tag => Some(None),
            _ => Some(Some(ch)),
        })
        .flatten()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn account_key_from_email(address: &str) -> String {
    format!("email:{}", address.trim().to_lowercase())
}

fn message_key_from_remote(account_key: &str, folder: &str, remote_id: &str) -> String {
    format!("{account_key}::{folder}::{remote_id}")
}

fn generated_message_id(email: &str, subject: &str, body: &str) -> String {
    let domain = email
        .split('@')
        .nth(1)
        .filter(|value| !value.is_empty())
        .unwrap_or("local");
    format!(
        "<{}@{}>",
        stable_digest(&format!(
            "{}:{}:{}:{}",
            now_iso_string(),
            email,
            subject,
            body
        )),
        domain
    )
}

fn build_smtp_raw_message(
    from: &str,
    to: &[String],
    cc: &[String],
    subject: &str,
    body: &str,
    message_id: &str,
    thread_key: &str,
    attachments: &[String],
) -> Result<String> {
    let mut lines = vec![format!("From: {from}"), format!("To: {}", to.join(", "))];
    if !cc.is_empty() {
        lines.push(format!("Cc: {}", cc.join(", ")));
    }
    lines.push(format!("Subject: {}", mime_header_value(subject)));
    lines.push(format!("Message-ID: {message_id}"));
    if !thread_key.trim().is_empty() {
        lines.push(format!("In-Reply-To: {thread_key}"));
        lines.push(format!("References: {thread_key}"));
    }
    lines.push(format!("Date: {}", smtp_date_header()));
    lines.push("MIME-Version: 1.0".to_string());
    if attachments.is_empty() {
        lines.push("Content-Type: text/plain; charset=utf-8".to_string());
        lines.push("Content-Transfer-Encoding: 8bit".to_string());
        lines.push(String::new());
        for line in body.lines() {
            if line.starts_with('.') {
                lines.push(format!(".{line}"));
            } else {
                lines.push(line.to_string());
            }
        }
        lines.push(String::new());
        return Ok(lines.join("\r\n"));
    }

    let boundary = format!("ctox-mixed-{}", stable_digest(message_id));
    lines.push(format!(
        "Content-Type: multipart/mixed; boundary=\"{boundary}\""
    ));
    lines.push(String::new());
    lines.push(format!("--{boundary}"));
    lines.push("Content-Type: text/plain; charset=utf-8".to_string());
    lines.push("Content-Transfer-Encoding: 8bit".to_string());
    lines.push(String::new());
    for line in body.lines() {
        if line.starts_with('.') {
            lines.push(format!(".{line}"));
        } else {
            lines.push(line.to_string());
        }
    }
    lines.push(String::new());
    for attachment in load_outbound_attachments(attachments)? {
        lines.push(format!("--{boundary}"));
        lines.push(format!(
            "Content-Type: {}; name=\"{}\"",
            attachment.content_type,
            mime_header_value(&attachment.file_name)
        ));
        lines.push("Content-Transfer-Encoding: base64".to_string());
        lines.push(format!(
            "Content-Disposition: attachment; filename=\"{}\"",
            mime_header_value(&attachment.file_name)
        ));
        lines.push(String::new());
        for chunk in BASE64_STANDARD
            .encode(attachment.bytes)
            .as_bytes()
            .chunks(76)
        {
            lines.push(String::from_utf8_lossy(chunk).to_string());
        }
        lines.push(String::new());
    }
    lines.push(format!("--{boundary}--"));
    lines.push(String::new());
    Ok(lines.join("\r\n"))
}

fn mime_header_value(value: &str) -> String {
    if value.is_ascii() {
        value.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", BASE64_STANDARD.encode(value.as_bytes()))
    }
}

fn smtp_date_header() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    chrono::DateTime::<chrono::Utc>::from_timestamp(now.as_secs() as i64, now.subsec_nanos())
        .map(|value| value.to_rfc2822())
        .unwrap_or_else(|| "Thu, 01 Jan 1970 00:00:00 +0000".to_string())
}

fn write_raw_payload(raw_dir: &Path, remote_id: &str, raw: &[u8]) -> Result<String> {
    std::fs::create_dir_all(raw_dir)
        .with_context(|| format!("failed to create raw dir {}", raw_dir.display()))?;
    let safe_id = remote_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let full_path = raw_dir.join(format!("{safe_id}.eml"));
    std::fs::write(&full_path, raw)
        .with_context(|| format!("failed to write raw payload {}", full_path.display()))?;
    Ok(full_path.display().to_string())
}

fn extract_address(token: &str) -> String {
    let trimmed = token.trim();
    if let Some(start) = trimmed.rfind('<') {
        if let Some(end) = trimmed[start..].find('>') {
            return trimmed[start + 1..start + end].trim().to_lowercase();
        }
    }
    let lowered = trimmed.to_lowercase();
    lowered
        .split_whitespace()
        .find(|part| part.contains('@') && part.contains('.'))
        .unwrap_or("")
        .trim_matches(|ch| ch == '<' || ch == '>' || ch == ',' || ch == ';')
        .to_string()
}

fn extract_display_name(token: &str) -> Option<String> {
    let trimmed = token.trim();
    trimmed
        .split('<')
        .next()
        .map(|value| value.trim().trim_matches('"'))
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn extract_addresses(raw: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for token in raw.split(',') {
        let address = extract_address(token);
        if address.is_empty() || !seen.insert(address.clone()) {
            continue;
        }
        out.push(address);
    }
    out
}

fn normalize_email_identity(value: &str) -> String {
    extract_address(value)
}

fn synced_message_direction(sender_address: &str, account_email: &str) -> &'static str {
    let sender = normalize_email_identity(sender_address);
    let account = normalize_email_identity(account_email);
    if !sender.is_empty() && !account.is_empty() && sender == account {
        "outbound"
    } else {
        "inbound"
    }
}

fn normalize_provider(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "" | "classic" | "smtp" | "imap-smtp" | "one" | "one.com" | "onecom" => "imap".to_string(),
        "m365" | "graph-cloud" | "exchange-online" => "graph".to_string(),
        "outlook" | "exchange" => "ews".to_string(),
        "owa" => "owa".to_string(),
        "eas" => "activesync".to_string(),
        other => other.to_string(),
    }
}

fn normalize_ews_auth_type(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "" => DEFAULT_EWS_AUTH_TYPE.to_string(),
        "oauth2" => "bearer".to_string(),
        other => other.to_string(),
    }
}

fn resolve_ews_url(options: &EmailOptions) -> String {
    if !options.ews_url.trim().is_empty() {
        return options.ews_url.trim().to_string();
    }
    derive_ews_url_from_owa_url(&options.owa_url).unwrap_or_default()
}

fn derive_ews_url_from_owa_url(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    let mut url = Url::parse(text).ok()?;
    url.set_path("/EWS/Exchange.asmx");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

fn folder_hint_to_mailbox_folder(folder_hint: &str) -> String {
    match folder_hint.trim().to_lowercase().as_str() {
        "sent" | "sentitems" => "sentitems".to_string(),
        "drafts" => "drafts".to_string(),
        _ => "inbox".to_string(),
    }
}

fn mailbox_folder_to_hint(folder_id: &str) -> String {
    match folder_id.trim().to_lowercase().as_str() {
        "sentitems" | "sent" => "sent".to_string(),
        "drafts" => "drafts".to_string(),
        _ => "inbox".to_string(),
    }
}

fn iso_before(candidate: &str, earliest: &str) -> bool {
    let candidate_ts = chrono::DateTime::parse_from_rfc3339(candidate)
        .map(|value| value.timestamp())
        .ok();
    let earliest_ts = chrono::DateTime::parse_from_rfc3339(earliest)
        .map(|value| value.timestamp())
        .ok();
    match (candidate_ts, earliest_ts) {
        (Some(candidate), Some(earliest)) => candidate + 1 < earliest,
        _ => false,
    }
}

fn epoch_seconds_to_iso(epoch_seconds: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(epoch_seconds, 0)
        .map(|value| value.to_rfc3339())
}

fn optional_setting(map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    map.get(key).map(|value| value.trim().to_string())
}

fn setting(map: &BTreeMap<String, String>, key: &str) -> String {
    optional_setting(map, key).unwrap_or_default()
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> Result<&'a str> {
    optional_flag(args, flag).ok_or_else(|| anyhow!("missing required flag {flag}"))
}

fn optional_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            return args.get(index + 1).map(String::as_str);
        }
        index += 1;
    }
    None
}

fn number_like_u16(value: &Value) -> Option<u16> {
    value
        .as_u64()
        .and_then(|number| u16::try_from(number).ok())
        .or_else(|| value.as_str().and_then(|value| value.parse::<u16>().ok()))
}

fn non_empty_or_default(value: Option<String>, default: &str) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn truthy(value: &str, default: bool) -> bool {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return default;
    }
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

fn require_provider_credentials(options: &EmailOptions) -> Result<()> {
    if options.email.trim().is_empty() {
        bail!("Missing --email or CTO_EMAIL_ADDRESS.");
    }
    match options.provider.as_str() {
        "imap" => {
            if options.password.trim().is_empty() {
                bail!("Missing CTO_EMAIL_PASSWORD for IMAP/SMTP.");
            }
        }
        "graph" => {
            if !options.graph_access_token.trim().is_empty() {
                // Pre-acquired bearer token — nothing else to check.
            } else {
                let username = effective_graph_username(options);
                let password = effective_graph_password(options);
                let has_user_creds = !username.trim().is_empty() && !password.is_empty();
                let has_app_creds = !options.graph_tenant_id.trim().is_empty()
                    && !options.graph_client_id.trim().is_empty()
                    && !options.graph_client_secret.trim().is_empty();
                if !has_user_creds && !has_app_creds {
                    bail!(
                        "Graph adapter needs credentials: set CTO_EMAIL_GRAPH_ACCESS_TOKEN, \
                         OR provide a username+password (CTO_EMAIL_GRAPH_USERNAME/PASSWORD, \
                         falling back to CTO_EMAIL_ADDRESS/CTO_EMAIL_PASSWORD) for ROPC, \
                         OR provide CTO_EMAIL_GRAPH_TENANT_ID + CTO_EMAIL_GRAPH_CLIENT_ID + \
                         CTO_EMAIL_GRAPH_CLIENT_SECRET for the client-credentials flow."
                    );
                }
            }
        }
        "ews" | "owa" => {
            if resolve_ews_url(options).trim().is_empty() {
                bail!("Missing CTO_EMAIL_EWS_URL or CTO_EMAIL_OWA_URL for EWS/OWA.");
            }
            match options.ews_auth_type.as_str() {
                "basic" => {
                    if options.password.trim().is_empty() {
                        bail!("Missing CTO_EMAIL_PASSWORD for EWS basic auth.");
                    }
                }
                "bearer" => {
                    if options.ews_bearer_token.trim().is_empty() {
                        bail!("Missing CTO_EMAIL_EWS_BEARER_TOKEN for EWS bearer auth.");
                    }
                }
                "ntlm" => {}
                other => bail!("Unsupported EWS auth type: {other}"),
            }
        }
        "activesync" => {
            if options.active_sync_server.trim().is_empty() {
                bail!("Missing CTO_EMAIL_ACTIVESYNC_SERVER for ActiveSync.");
            }
            if options.password.trim().is_empty() {
                bail!("Missing CTO_EMAIL_PASSWORD for ActiveSync.");
            }
        }
        other => bail!("Unsupported email provider: {other}"),
    }
    Ok(())
}

fn delivery_json(delivery: &DeliveryStatus) -> Value {
    json!({
        "confirmed": delivery.confirmed,
        "skipped": delivery.skipped,
        "method": delivery.method,
        "detail": delivery.detail,
        "remoteId": delivery.remote_id,
        "threadKey": delivery.thread_key,
        "observedAt": delivery.observed_at,
    })
}

struct ImapMailbox {
    name: String,
    flags: Vec<String>,
}

struct ImapFetchedMessage {
    flags: Vec<String>,
    raw: Vec<u8>,
}

struct BufferStream<S: Read + Write> {
    stream: S,
    buffer: Vec<u8>,
}

impl<S: Read + Write> BufferStream<S> {
    fn new(stream: S) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.stream.write_all(bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    fn fill(&mut self) -> Result<usize> {
        let mut chunk = [0u8; 8192];
        let read = self.stream.read(&mut chunk)?;
        if read > 0 {
            self.buffer.extend_from_slice(&chunk[..read]);
        }
        Ok(read)
    }

    fn read_line(&mut self) -> Result<String> {
        loop {
            if let Some(index) = find_crlf(&self.buffer) {
                let line = String::from_utf8_lossy(&self.buffer[..index]).to_string();
                self.buffer.drain(..index + 2);
                return Ok(line);
            }
            if self.fill()? == 0 {
                bail!("socket closed before response");
            }
        }
    }

    fn read_until_tagged(&mut self, tag: &str) -> Result<Vec<u8>> {
        let tag_prefix = format!("{tag} ");
        loop {
            if let Some(end) = find_tagged_line_end(&self.buffer, &tag_prefix) {
                let out = self.buffer[..end].to_vec();
                self.buffer.drain(..end);
                return Ok(out);
            }
            if self.fill()? == 0 {
                bail!("socket closed before tagged response");
            }
        }
    }

    fn into_inner(self) -> S {
        self.stream
    }
}

fn find_crlf(buffer: &[u8]) -> Option<usize> {
    buffer.windows(2).position(|window| window == b"\r\n")
}

fn find_tagged_line_end(buffer: &[u8], tag_prefix: &str) -> Option<usize> {
    let pattern = tag_prefix.as_bytes();
    let mut line_start = 0usize;
    while line_start < buffer.len() {
        let line_end = buffer[line_start..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| line_start + offset + 2)?;
        if buffer[line_start..].starts_with(pattern) {
            return Some(line_end);
        }
        line_start = line_end;
    }
    None
}

struct ImapClient {
    stream: BufferStream<TlsStream<TcpStream>>,
    tag_counter: usize,
}

impl ImapClient {
    fn connect(options: &EmailOptions) -> Result<Self> {
        let tcp = TcpStream::connect((options.imap_host.as_str(), options.imap_port))
            .with_context(|| {
                format!(
                    "failed to connect to IMAP {}:{}",
                    options.imap_host, options.imap_port
                )
            })?;
        tcp.set_read_timeout(Some(Duration::from_secs(20)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(20)))?;
        let connector =
            TlsConnector::new().context("failed to initialize TLS connector for IMAP")?;
        let tls = connector
            .connect(options.imap_host.as_str(), tcp)
            .map_err(|error| anyhow!("failed to establish IMAP TLS session: {error}"))?;
        let mut stream = BufferStream::new(tls);
        let greeting = stream.read_line()?;
        if !greeting.to_uppercase().contains("OK") {
            bail!("IMAP greeting failed: {greeting}");
        }
        Ok(Self {
            stream,
            tag_counter: 0,
        })
    }

    fn next_tag(&mut self) -> String {
        self.tag_counter += 1;
        format!("A{:04}", self.tag_counter)
    }

    fn command(&mut self, command: &str) -> Result<Vec<u8>> {
        let tag = self.next_tag();
        self.stream
            .write_all(format!("{tag} {command}\r\n").as_bytes())?;
        let response = self.stream.read_until_tagged(&tag)?;
        let response_text = String::from_utf8_lossy(&response);
        let status = response_text
            .lines()
            .rev()
            .find(|line| line.starts_with(&tag))
            .unwrap_or_default()
            .to_string();
        if !status.to_uppercase().contains(" OK") {
            bail!("IMAP command failed: {command}");
        }
        Ok(response)
    }

    fn login(&mut self, email: &str, password: &str) -> Result<()> {
        self.command(&format!(
            "LOGIN {} {}",
            imap_quote(email),
            imap_quote(password)
        ))?;
        Ok(())
    }

    fn select(&mut self, folder: &str) -> Result<()> {
        self.command(&format!("SELECT {}", imap_quote(folder)))?;
        Ok(())
    }

    fn list_mailboxes(&mut self) -> Result<Vec<ImapMailbox>> {
        let response = self.command(&format!("LIST {} {}", imap_quote(""), imap_quote("*")))?;
        let text = String::from_utf8_lossy(&response);
        let mut mailboxes = Vec::new();
        for line in text.lines() {
            if !line.starts_with("* LIST ") {
                continue;
            }
            let parts = line.splitn(4, ' ').collect::<Vec<_>>();
            if parts.len() < 4 {
                continue;
            }
            let flags_text = parts[2].trim_matches(|ch| ch == '(' || ch == ')');
            let flags = flags_text
                .split_whitespace()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            let name = line
                .rsplit('"')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string();
            if !name.is_empty() {
                mailboxes.push(ImapMailbox { name, flags });
            }
        }
        Ok(mailboxes)
    }

    fn search_all_uids(&mut self) -> Result<Vec<String>> {
        let response = self.command("UID SEARCH ALL")?;
        let text = String::from_utf8_lossy(&response);
        let match_line = text
            .lines()
            .find(|line| line.starts_with("* SEARCH "))
            .unwrap_or("");
        Ok(match_line
            .trim_start_matches("* SEARCH ")
            .split_whitespace()
            .map(|value| value.to_string())
            .collect())
    }

    fn fetch_raw(&mut self, uid: &str) -> Result<ImapFetchedMessage> {
        let response = self.command(&format!("UID FETCH {uid} (UID FLAGS RFC822)"))?;
        let text = String::from_utf8_lossy(&response);
        let flags = text
            .lines()
            .find(|line| line.contains(" FLAGS "))
            .and_then(|line| {
                let start = line.find("FLAGS (")?;
                let rest = &line[start + 7..];
                let end = rest.find(')')?;
                Some(
                    rest[..end]
                        .split_whitespace()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        let marker = b"}\r\n";
        let marker_index = response
            .windows(marker.len())
            .position(|window| window == marker)
            .ok_or_else(|| anyhow!("IMAP FETCH response contained no literal"))?;
        let size_start = response[..marker_index]
            .iter()
            .rposition(|byte| *byte == b'{')
            .ok_or_else(|| anyhow!("IMAP FETCH response contained no literal length"))?;
        let literal_size = String::from_utf8_lossy(&response[size_start + 1..marker_index])
            .parse::<usize>()
            .context("failed to parse IMAP literal size")?;
        let literal_start = marker_index + marker.len();
        let literal_end = literal_start + literal_size;
        if response.len() < literal_end {
            bail!("IMAP literal truncated");
        }
        Ok(ImapFetchedMessage {
            flags,
            raw: response[literal_start..literal_end].to_vec(),
        })
    }

    fn logout(mut self) {
        let _ = self.command("LOGOUT");
    }
}

fn imap_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn latest_imap_uids(mut uids: Vec<String>, limit: usize) -> Vec<String> {
    uids.sort_by(|left, right| {
        let left_num = left.parse::<u64>();
        let right_num = right.parse::<u64>();
        match (left_num, right_num) {
            (Ok(left_num), Ok(right_num)) => right_num.cmp(&left_num),
            _ => right.cmp(left),
        }
    });
    uids.truncate(limit);
    uids
}

enum SmtpStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl Read for SmtpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for SmtpStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

struct SmtpClient {
    stream: Option<BufferStream<SmtpStream>>,
    smtp_host: String,
    connection_mode: String,
}

impl SmtpClient {
    fn connect(options: &EmailOptions) -> Result<Self> {
        let tcp = TcpStream::connect((options.smtp_host.as_str(), options.smtp_port))
            .with_context(|| {
                format!(
                    "failed to connect to SMTP {}:{}",
                    options.smtp_host, options.smtp_port
                )
            })?;
        tcp.set_read_timeout(Some(Duration::from_secs(20)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(20)))?;
        let stream = if options.smtp_port == 587 {
            BufferStream::new(SmtpStream::Plain(tcp))
        } else {
            let connector =
                TlsConnector::new().context("failed to initialize TLS connector for SMTP")?;
            let tls = connector
                .connect(options.smtp_host.as_str(), tcp)
                .map_err(|error| anyhow!("failed to establish SMTP TLS session: {error}"))?;
            BufferStream::new(SmtpStream::Tls(tls))
        };
        let mut client = Self {
            stream: Some(stream),
            smtp_host: options.smtp_host.clone(),
            connection_mode: if options.smtp_port == 587 {
                "plain".to_string()
            } else {
                "implicit-tls".to_string()
            },
        };
        client.expect(&[220])?;
        Ok(client)
    }

    fn connection_mode(&self) -> &str {
        &self.connection_mode
    }

    fn stream_mut(&mut self) -> Result<&mut BufferStream<SmtpStream>> {
        self.stream
            .as_mut()
            .ok_or_else(|| anyhow!("SMTP session stream unavailable"))
    }

    fn expect(&mut self, allowed: &[u16]) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        let first = self.stream_mut()?.read_line()?;
        let mut current = first.clone();
        let code = current
            .get(0..3)
            .unwrap_or("")
            .parse::<u16>()
            .unwrap_or_default();
        lines.push(first);
        while current.as_bytes().get(3) == Some(&b'-') {
            current = self.stream_mut()?.read_line()?;
            lines.push(current.clone());
        }
        if !allowed.contains(&code) {
            bail!("SMTP failed: {}", lines.join(" | "));
        }
        Ok(lines)
    }

    fn send_command(&mut self, command: &str, allowed: &[u16]) -> Result<Vec<String>> {
        self.stream_mut()?
            .write_all(format!("{command}\r\n").as_bytes())?;
        self.expect(allowed)
    }

    fn upgrade_to_starttls(&mut self) -> Result<()> {
        self.send_command("STARTTLS", &[220])?;
        let plain_stream = match self
            .stream
            .take()
            .ok_or_else(|| anyhow!("SMTP session stream unavailable"))?
            .into_inner()
        {
            SmtpStream::Plain(stream) => stream,
            SmtpStream::Tls(_) => bail!("SMTP STARTTLS upgrade attempted on TLS stream"),
        };
        let connector = TlsConnector::new().context("failed to initialize STARTTLS connector")?;
        let tls = connector
            .connect(self.smtp_host.as_str(), plain_stream)
            .map_err(|error| anyhow!("failed to upgrade SMTP to STARTTLS: {error}"))?;
        self.stream = Some(BufferStream::new(SmtpStream::Tls(tls)));
        self.connection_mode = "starttls".to_string();
        Ok(())
    }

    fn authenticate(&mut self, email: &str, password: &str) -> Result<()> {
        let plain = BASE64_STANDARD.encode(format!("\u{0}{email}\u{0}{password}"));
        if self
            .send_command(&format!("AUTH PLAIN {plain}"), &[235])
            .is_ok()
        {
            return Ok(());
        }
        self.send_command("AUTH LOGIN", &[334])?;
        self.send_command(&BASE64_STANDARD.encode(email.as_bytes()), &[334])?;
        self.send_command(&BASE64_STANDARD.encode(password.as_bytes()), &[235])?;
        Ok(())
    }

    fn login(&mut self, email: &str, password: &str) -> Result<()> {
        if self.send_command("EHLO localhost", &[250]).is_err() {
            self.send_command("HELO localhost", &[250])?;
        }
        if self.connection_mode == "plain" {
            self.upgrade_to_starttls()?;
            self.send_command("EHLO localhost", &[250])?;
        }
        self.authenticate(email, password)
    }

    fn send_mail(
        &mut self,
        from: &str,
        to: &[String],
        cc: &[String],
        bcc: &[String],
        raw_message: &str,
    ) -> Result<()> {
        self.send_command(&format!("MAIL FROM:<{from}>"), &[250])?;
        for recipient in to.iter().chain(cc.iter()).chain(bcc.iter()) {
            self.send_command(&format!("RCPT TO:<{recipient}>"), &[250, 251])?;
        }
        self.send_command("DATA", &[354])?;
        self.stream_mut()?.write_all(raw_message.as_bytes())?;
        self.stream_mut()?.write_all(b"\r\n.\r\n")?;
        self.expect(&[250])?;
        Ok(())
    }

    fn close(&mut self) {
        if let Some(stream) = self.stream.as_mut() {
            let _ = stream.write_all(b"QUIT\r\n");
        }
    }
}

#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
}

pub(crate) fn http_request(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&[u8]>,
) -> Result<HttpResponse> {
    let mut request = ureq::request(method, url);
    for (key, value) in headers {
        request = request.set(key, value);
    }
    let response = match body {
        Some(bytes) => match request.send_bytes(bytes) {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(error)) => {
                return Err(anyhow!("HTTP transport failed for {url}: {error}"));
            }
        },
        None => match request.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(error)) => {
                return Err(anyhow!("HTTP transport failed for {url}: {error}"));
            }
        },
    };
    let headers = response
        .headers_names()
        .into_iter()
        .filter_map(|name| {
            response
                .header(&name)
                .map(|value| (name.to_ascii_lowercase(), value.to_string()))
        })
        .collect::<BTreeMap<_, _>>();
    let status = response.status();
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(HttpResponse {
        status,
        headers,
        body: bytes,
    })
}

fn http_json_value(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let payload = body.map(serde_json::to_vec).transpose()?;
    let response = http_request(method, url, headers, payload.as_deref())?;
    let value = if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&response.body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&response.body).into_owned()))
    };
    Ok((response.status, response.headers, value))
}

struct GraphClient {
    access_token: String,
    base_url: String,
    user: String,
}

impl GraphClient {
    fn from_options(options: &EmailOptions) -> Result<Self> {
        let access_token = if !options.graph_access_token.trim().is_empty() {
            options.graph_access_token.clone()
        } else {
            acquire_graph_access_token(options)?
        };
        Ok(Self {
            access_token,
            base_url: options.graph_base_url.trim_end_matches('/').to_string(),
            user: options.graph_user.clone(),
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
            .with_context(|| format!("invalid Graph url base {}", self.base_url))?;
        for (key, value) in query {
            if !value.is_empty() {
                url.query_pairs_mut().append_pair(key, value);
            }
        }
        let (status, _, value) = http_json_value(method, url.as_str(), &self.headers(), body)?;
        if !(200..300).contains(&status) {
            bail!("Graph HTTP {status}: {value}");
        }
        Ok(value)
    }

    fn list_folder(&self, folder_id: &str, top: usize) -> Result<Vec<MailboxMessage>> {
        let value = self.request(
            "GET",
            &format!("/{}/mailFolders/{}/messages", self.user, folder_id),
            &[
                ("$top", top.to_string()),
                ("$orderby", "receivedDateTime desc".to_string()),
                (
                    "$select",
                    "id,subject,from,toRecipients,ccRecipients,receivedDateTime,sentDateTime,isRead,hasAttachments,bodyPreview,parentFolderId,conversationId,internetMessageId,body".to_string(),
                ),
                (
                    "$expand",
                    "attachments($select=id,name,contentType,size,isInline)".to_string(),
                ),
            ],
            None,
        )?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| normalize_graph_mail_item(item, folder_id))
            .collect())
    }

    fn send_mail(
        &self,
        subject: &str,
        body: &str,
        to: &[String],
        cc: &[String],
        bcc: &[String],
        attachments: &[String],
    ) -> Result<()> {
        let mk = |items: &[String]| -> Vec<Value> {
            items
                .iter()
                .map(|address| json!({"emailAddress":{"address": address.trim().to_lowercase()}}))
                .collect()
        };
        let attachment_files = load_outbound_attachments(attachments)?;
        let attachment_payload = attachment_files
            .iter()
            .map(|attachment| {
                if attachment.size_bytes >= 3 * 1024 * 1024 {
                    bail!(
                        "Graph email attachment {} is {} bytes; simple Graph fileAttachment send supports files below 3 MB",
                        attachment.path.display(),
                        attachment.size_bytes
                    );
                }
                Ok(json!({
                    "@odata.type": "#microsoft.graph.fileAttachment",
                    "name": attachment.file_name,
                    "contentType": attachment.content_type,
                    "contentBytes": BASE64_STANDARD.encode(&attachment.bytes),
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut message = json!({
                "subject": subject,
                "body": {"contentType": "Text", "content": body},
                "toRecipients": mk(to),
                "ccRecipients": mk(cc),
                "bccRecipients": mk(bcc),
        });
        if !attachment_payload.is_empty() {
            message["attachments"] = Value::Array(attachment_payload);
        }
        let body = json!({
            "message": message,
            "saveToSentItems": true,
        });
        self.request(
            "POST",
            &format!("/{}/sendMail", self.user),
            &[],
            Some(&body),
        )?;
        Ok(())
    }
}

fn normalize_graph_mail_item(raw: &Value, folder_id_fallback: &str) -> Option<MailboxMessage> {
    let remote_id = raw.get("id")?.as_str()?.to_string();
    let thread_key = raw
        .get("conversationId")
        .and_then(Value::as_str)
        .unwrap_or(&remote_id)
        .to_string();
    let from = raw.get("from").and_then(|value| value.get("emailAddress"));
    let sender_address = from
        .and_then(|value| value.get("address"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let sender_display = from
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(if sender_address.is_empty() {
            "unknown"
        } else {
            &sender_address
        })
        .to_string();
    let body = raw.get("body").cloned().unwrap_or(Value::Null);
    let body_type = body
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let body_content = body.get("content").and_then(Value::as_str).unwrap_or("");
    Some(MailboxMessage {
        remote_id,
        thread_key,
        folder_hint: mailbox_folder_to_hint(
            raw.get("parentFolderId")
                .and_then(Value::as_str)
                .unwrap_or(folder_id_fallback),
        ),
        subject: raw
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or("(ohne Betreff)")
            .to_string(),
        sender_display,
        sender_address,
        recipient_addresses: json_address_list(raw.get("toRecipients")),
        cc_addresses: json_address_list(raw.get("ccRecipients")),
        body_text: if body_type == "html" {
            strip_html(body_content)
        } else {
            body_content.to_string()
        },
        body_html: if body_type == "html" {
            body_content.to_string()
        } else {
            String::new()
        },
        preview: preview_text(
            raw.get("bodyPreview")
                .and_then(Value::as_str)
                .unwrap_or(body_content),
            raw.get("subject")
                .and_then(Value::as_str)
                .unwrap_or("(ohne Betreff)"),
        ),
        seen: raw.get("isRead").and_then(Value::as_bool).unwrap_or(true),
        has_attachments: raw
            .get("hasAttachments")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        external_created_at: raw
            .get("receivedDateTime")
            .and_then(Value::as_str)
            .or_else(|| raw.get("sentDateTime").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
        metadata: json!({
            "internetMessageId": raw.get("internetMessageId").and_then(Value::as_str).unwrap_or(""),
            "conversationId": raw.get("conversationId").and_then(Value::as_str).unwrap_or(""),
            "graphFolderId": raw.get("parentFolderId").and_then(Value::as_str).unwrap_or(folder_id_fallback),
            "attachments": raw.get("attachments").cloned().unwrap_or(Value::Array(Vec::new())),
        }),
    })
}

fn json_address_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("emailAddress")
                .and_then(|value| value.get("address"))
                .and_then(Value::as_str)
        })
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

struct EwsClient {
    url: String,
    version: String,
    auth_type: String,
    username: String,
    password: String,
    bearer_token: String,
}

impl EwsClient {
    fn from_options(options: &EmailOptions) -> Result<Self> {
        Ok(Self {
            url: resolve_ews_url(options),
            version: options.ews_version.clone(),
            auth_type: options.ews_auth_type.clone(),
            username: if options.ews_username.trim().is_empty() {
                options.email.clone()
            } else {
                options.ews_username.clone()
            },
            password: options.password.clone(),
            bearer_token: options.ews_bearer_token.clone(),
        })
    }

    fn headers(&self) -> Result<BTreeMap<String, String>> {
        let mut headers = BTreeMap::new();
        headers.insert(
            "content-type".to_string(),
            "text/xml; charset=utf-8".to_string(),
        );
        headers.insert("accept".to_string(), "text/xml".to_string());
        match self.auth_type.as_str() {
            "basic" => {
                headers.insert(
                    "authorization".to_string(),
                    format!(
                        "Basic {}",
                        BASE64_STANDARD.encode(format!("{}:{}", self.username, self.password))
                    ),
                );
            }
            "bearer" => {
                headers.insert(
                    "authorization".to_string(),
                    format!("Bearer {}", self.bearer_token),
                );
            }
            "ntlm" => {}
            other => bail!("Unsupported EWS auth type: {other}"),
        }
        Ok(headers)
    }

    fn request(&self, op_name: &str, op_attributes: &str, body: &str) -> Result<Document<'static>> {
        let envelope = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/" xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
<soap:Header><t:RequestServerVersion Version="{}"/></soap:Header>
<soap:Body><m:{}{}>{}</m:{}></soap:Body>
</soap:Envelope>"#,
            xml_escape(&self.version),
            op_name,
            op_attributes,
            body,
            op_name
        );
        let response = http_request(
            "POST",
            &self.url,
            &self.headers()?,
            Some(envelope.as_bytes()),
        )?;
        let body = String::from_utf8_lossy(&response.body).into_owned();
        if !(200..300).contains(&response.status) && !body.trim_start().starts_with('<') {
            bail!("EWS HTTP {}: {}", response.status, body);
        }
        let leaked: &'static str = Box::leak(body.into_boxed_str());
        let document = Document::parse(leaked).context("failed to parse EWS XML response")?;
        assert_ews_success(response.status, &document)?;
        Ok(document)
    }

    fn list_folder(
        &self,
        folder_id: &str,
        top: usize,
        query: Option<&str>,
    ) -> Result<Vec<MailboxMessage>> {
        let parent_folder = distinguished_folder_xml(folder_id);
        let query_xml = query
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("<m:QueryString>{}</m:QueryString>", xml_escape(value)))
            .unwrap_or_default();
        let body = format!(
            r#"<m:ItemShape>
<t:BaseShape>IdOnly</t:BaseShape>
<t:AdditionalProperties>
<t:FieldURI FieldURI="item:Subject"/>
<t:FieldURI FieldURI="message:From"/>
<t:FieldURI FieldURI="message:ToRecipients"/>
<t:FieldURI FieldURI="message:CcRecipients"/>
<t:FieldURI FieldURI="item:DateTimeReceived"/>
<t:FieldURI FieldURI="message:IsRead"/>
<t:FieldURI FieldURI="item:HasAttachments"/>
<t:FieldURI FieldURI="item:ConversationId"/>
</t:AdditionalProperties>
</m:ItemShape>
<m:ParentFolderIds>{}</m:ParentFolderIds>
<m:IndexedPageItemView MaxEntriesReturned="{}" Offset="0" BasePoint="Beginning"/>{}"#,
            parent_folder, top, query_xml
        );
        let document = self.request("FindItem", r#" Traversal="Shallow""#, &body)?;
        Ok(document
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "Message")
            .filter_map(|node| normalize_ews_mail_item(node, folder_id))
            .collect())
    }

    fn send_mail(
        &self,
        subject: &str,
        body: &str,
        to: &[String],
        cc: &[String],
        bcc: &[String],
        attachments: &[String],
    ) -> Result<()> {
        let recipients = |tag: &str, values: &[String]| -> String {
            if values.is_empty() {
                return String::new();
            }
            format!(
                "<t:{tag}>{}</t:{tag}>",
                values
                    .iter()
                    .map(|value| {
                        format!(
                            "<t:Mailbox><t:EmailAddress>{}</t:EmailAddress></t:Mailbox>",
                            xml_escape(&value.trim().to_lowercase())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("")
            )
        };
        let attachment_xml = build_ews_file_attachments_xml(attachments)?;
        let body = format!(
            r#"<m:SavedItemFolderId>{}</m:SavedItemFolderId>
<m:Items><t:Message>
<t:ItemClass>IPM.Note</t:ItemClass>
<t:Subject>{}</t:Subject>
<t:Body BodyType="Text">{}</t:Body>{}{}{}{}
</t:Message></m:Items>"#,
            distinguished_folder_xml("sentitems"),
            xml_escape(subject),
            xml_escape(body),
            recipients("ToRecipients", to),
            recipients("CcRecipients", cc),
            recipients("BccRecipients", bcc),
            attachment_xml,
        );
        self.request(
            "CreateItem",
            r#" MessageDisposition="SendAndSaveCopy""#,
            &body,
        )?;
        Ok(())
    }
}

fn build_ews_file_attachments_xml(paths: &[String]) -> Result<String> {
    let attachments = load_outbound_attachments(paths)?;
    if attachments.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(
        "<t:Attachments>{}</t:Attachments>",
        attachments
            .iter()
            .map(|attachment| {
                format!(
                    "<t:FileAttachment><t:Name>{}</t:Name><t:ContentType>{}</t:ContentType><t:Content>{}</t:Content></t:FileAttachment>",
                    xml_escape(&attachment.file_name),
                    xml_escape(&attachment.content_type),
                    BASE64_STANDARD.encode(&attachment.bytes),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    ))
}

fn assert_ews_success(status: u16, document: &Document<'_>) -> Result<()> {
    if let Some(fault) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "Fault")
    {
        let text =
            descendant_text(fault, "faultstring").unwrap_or_else(|| "SOAP Fault".to_string());
        bail!("EWS SOAP Fault: {text}");
    }
    for response_message in document.descendants().filter(|node| {
        node.is_element()
            && node
                .tag_name()
                .name()
                .to_lowercase()
                .contains("responsemessage")
    }) {
        let response_class = response_message
            .attribute("ResponseClass")
            .unwrap_or("Success");
        if response_class != "Success" {
            let code = descendant_text(response_message, "ResponseCode")
                .unwrap_or_else(|| "Error".to_string());
            let text = descendant_text(response_message, "MessageText")
                .unwrap_or_else(|| "EWS error".to_string());
            bail!("EWS {response_class}: {code} - {text}");
        }
    }
    if !(200..300).contains(&status) {
        bail!("EWS HTTP {status} without successful SOAP payload");
    }
    Ok(())
}

fn distinguished_folder_xml(folder_id: &str) -> String {
    format!(
        r#"<t:DistinguishedFolderId Id="{}"/>"#,
        xml_escape(folder_id)
    )
}

fn normalize_ews_mail_item(
    node: roxmltree::Node<'_, '_>,
    folder_id_fallback: &str,
) -> Option<MailboxMessage> {
    let remote_id = node
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "ItemId")
        .and_then(|child| child.attribute("Id"))?
        .to_string();
    let conversation_id = node
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "ConversationId")
        .and_then(|child| child.attribute("Id"))
        .unwrap_or(&remote_id)
        .to_string();
    let from_mailbox = node
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "From")
        .and_then(|child| {
            child
                .descendants()
                .find(|node| node.is_element() && node.tag_name().name() == "Mailbox")
        });
    let sender_address = from_mailbox
        .and_then(|mailbox| descendant_text(mailbox, "EmailAddress"))
        .unwrap_or_default();
    let sender_display = from_mailbox
        .and_then(|mailbox| descendant_text(mailbox, "Name"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if sender_address.is_empty() {
                "unknown".to_string()
            } else {
                sender_address.clone()
            }
        });
    Some(MailboxMessage {
        remote_id,
        thread_key: conversation_id.clone(),
        folder_hint: mailbox_folder_to_hint(folder_id_fallback),
        subject: descendant_text(node, "Subject").unwrap_or_else(|| "(ohne Betreff)".to_string()),
        sender_display,
        sender_address,
        recipient_addresses: descendant_mailbox_addresses(node, "ToRecipients"),
        cc_addresses: descendant_mailbox_addresses(node, "CcRecipients"),
        body_text: String::new(),
        body_html: String::new(),
        preview: preview_text(
            "",
            &descendant_text(node, "Subject").unwrap_or_else(|| "(ohne Betreff)".to_string()),
        ),
        seen: descendant_text(node, "IsRead")
            .map(|value| value.to_lowercase() != "false")
            .unwrap_or(true),
        has_attachments: descendant_text(node, "HasAttachments")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        external_created_at: descendant_text(node, "DateTimeReceived")
            .unwrap_or_else(now_iso_string),
        metadata: json!({
            "conversationId": conversation_id,
            "ewsFolderId": folder_id_fallback,
        }),
    })
}

fn descendant_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.descendants()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(|value| value.trim().to_string())
}

fn descendant_mailbox_addresses(
    node: roxmltree::Node<'_, '_>,
    collection_name: &str,
) -> Vec<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == collection_name)
        .into_iter()
        .flat_map(|collection| {
            collection
                .descendants()
                .filter(|mailbox| mailbox.is_element() && mailbox.tag_name().name() == "Mailbox")
                .filter_map(|mailbox| descendant_text(mailbox, "EmailAddress"))
                .collect::<Vec<_>>()
        })
        .map(|value| value.to_lowercase())
        .collect()
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Clone, Debug)]
struct ActiveSyncFolder {
    server_id: String,
    display_name: String,
    folder_type: i64,
}

#[derive(Clone, Debug)]
enum EasChild {
    Text(String),
    Node(EasNode),
}

#[derive(Clone, Debug)]
struct EasNode {
    name: String,
    children: Vec<EasChild>,
}

impl EasNode {
    fn text_child(&self, name: &str) -> Option<String> {
        self.children.iter().find_map(|child| match child {
            EasChild::Node(node) if node.name == name => Some(node.node_text()),
            _ => None,
        })
    }

    fn child(&self, name: &str) -> Option<&EasNode> {
        self.children.iter().find_map(|child| match child {
            EasChild::Node(node) if node.name == name => Some(node),
            _ => None,
        })
    }

    fn children_named(&self, name: &str) -> Vec<&EasNode> {
        self.children
            .iter()
            .filter_map(|child| match child {
                EasChild::Node(node) if node.name == name => Some(node),
                _ => None,
            })
            .collect()
    }

    fn node_text(&self) -> String {
        self.children
            .iter()
            .filter_map(|child| match child {
                EasChild::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string()
    }
}

fn eas_elem(name: &str, children: Vec<EasChild>) -> EasNode {
    EasNode {
        name: name.to_string(),
        children,
    }
}

fn eas_text(name: &str, value: &str) -> EasChild {
    EasChild::Node(EasNode {
        name: name.to_string(),
        children: vec![EasChild::Text(value.to_string())],
    })
}

fn eas_tag_to_token(name: &str) -> Option<(u8, u8)> {
    Some(match name {
        "AirSync:Sync" => (0, 0x05),
        "AirSync:Responses" => (0, 0x06),
        "AirSync:Add" => (0, 0x07),
        "AirSync:Change" => (0, 0x08),
        "AirSync:Fetch" => (0, 0x0a),
        "AirSync:SyncKey" => (0, 0x0b),
        "AirSync:ServerId" => (0, 0x0d),
        "AirSync:Status" => (0, 0x0e),
        "AirSync:Collection" => (0, 0x0f),
        "AirSync:Class" => (0, 0x10),
        "AirSync:CollectionId" => (0, 0x12),
        "AirSync:GetChanges" => (0, 0x13),
        "AirSync:MoreAvailable" => (0, 0x14),
        "AirSync:WindowSize" => (0, 0x15),
        "AirSync:Commands" => (0, 0x16),
        "AirSync:Options" => (0, 0x17),
        "AirSync:Collections" => (0, 0x1c),
        "AirSync:ApplicationData" => (0, 0x1d),
        "Email:Body" => (2, 0x0c),
        "Email:DateReceived" => (2, 0x0f),
        "Email:DisplayTo" => (2, 0x11),
        "Email:Subject" => (2, 0x14),
        "Email:Read" => (2, 0x15),
        "Email:To" => (2, 0x16),
        "Email:Cc" => (2, 0x17),
        "Email:From" => (2, 0x18),
        "FolderHierarchy:Folders" => (7, 0x05),
        "FolderHierarchy:Folder" => (7, 0x06),
        "FolderHierarchy:DisplayName" => (7, 0x07),
        "FolderHierarchy:ServerId" => (7, 0x08),
        "FolderHierarchy:ParentId" => (7, 0x09),
        "FolderHierarchy:Type" => (7, 0x0a),
        "FolderHierarchy:Status" => (7, 0x0c),
        "FolderHierarchy:ContentClass" => (7, 0x0d),
        "FolderHierarchy:Changes" => (7, 0x0e),
        "FolderHierarchy:Add" => (7, 0x0f),
        "FolderHierarchy:Delete" => (7, 0x10),
        "FolderHierarchy:Update" => (7, 0x11),
        "FolderHierarchy:SyncKey" => (7, 0x12),
        "FolderHierarchy:FolderSync" => (7, 0x16),
        "AirSyncBase:BodyPreference" => (17, 0x05),
        "AirSyncBase:Type" => (17, 0x06),
        "AirSyncBase:TruncationSize" => (17, 0x07),
        "AirSyncBase:Body" => (17, 0x0a),
        "AirSyncBase:Data" => (17, 0x0b),
        "AirSyncBase:Preview" => (17, 0x19),
        _ => return None,
    })
}

fn eas_token_to_tag(page: u8, token: u8) -> Option<&'static str> {
    Some(match (page, token) {
        (0, 0x05) => "AirSync:Sync",
        (0, 0x06) => "AirSync:Responses",
        (0, 0x07) => "AirSync:Add",
        (0, 0x08) => "AirSync:Change",
        (0, 0x0a) => "AirSync:Fetch",
        (0, 0x0b) => "AirSync:SyncKey",
        (0, 0x0d) => "AirSync:ServerId",
        (0, 0x0e) => "AirSync:Status",
        (0, 0x0f) => "AirSync:Collection",
        (0, 0x10) => "AirSync:Class",
        (0, 0x12) => "AirSync:CollectionId",
        (0, 0x13) => "AirSync:GetChanges",
        (0, 0x14) => "AirSync:MoreAvailable",
        (0, 0x15) => "AirSync:WindowSize",
        (0, 0x16) => "AirSync:Commands",
        (0, 0x17) => "AirSync:Options",
        (0, 0x1c) => "AirSync:Collections",
        (0, 0x1d) => "AirSync:ApplicationData",
        (2, 0x0c) => "Email:Body",
        (2, 0x0f) => "Email:DateReceived",
        (2, 0x11) => "Email:DisplayTo",
        (2, 0x14) => "Email:Subject",
        (2, 0x15) => "Email:Read",
        (2, 0x16) => "Email:To",
        (2, 0x17) => "Email:Cc",
        (2, 0x18) => "Email:From",
        (7, 0x05) => "FolderHierarchy:Folders",
        (7, 0x06) => "FolderHierarchy:Folder",
        (7, 0x07) => "FolderHierarchy:DisplayName",
        (7, 0x08) => "FolderHierarchy:ServerId",
        (7, 0x09) => "FolderHierarchy:ParentId",
        (7, 0x0a) => "FolderHierarchy:Type",
        (7, 0x0c) => "FolderHierarchy:Status",
        (7, 0x0d) => "FolderHierarchy:ContentClass",
        (7, 0x0e) => "FolderHierarchy:Changes",
        (7, 0x0f) => "FolderHierarchy:Add",
        (7, 0x10) => "FolderHierarchy:Delete",
        (7, 0x11) => "FolderHierarchy:Update",
        (7, 0x12) => "FolderHierarchy:SyncKey",
        (7, 0x16) => "FolderHierarchy:FolderSync",
        (17, 0x05) => "AirSyncBase:BodyPreference",
        (17, 0x06) => "AirSyncBase:Type",
        (17, 0x07) => "AirSyncBase:TruncationSize",
        (17, 0x0a) => "AirSyncBase:Body",
        (17, 0x0b) => "AirSyncBase:Data",
        (17, 0x19) => "AirSyncBase:Preview",
        _ => return None,
    })
}

fn wbxml_decode_mb_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let mut value = 0u32;
    loop {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| anyhow!("WBXML mb_u_int32 overflow"))?;
        *cursor += 1;
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            break;
        }
    }
    Ok(value)
}

fn wbxml_encode(root: &EasNode) -> Result<Vec<u8>> {
    let mut out = vec![0x03, 0x01, 0x6a, 0x00];
    let mut current_page = 0u8;
    fn encode_node(node: &EasNode, out: &mut Vec<u8>, current_page: &mut u8) -> Result<()> {
        let (page, token) = eas_tag_to_token(&node.name)
            .ok_or_else(|| anyhow!("ActiveSync WBXML encode: unknown tag {}", node.name))?;
        if page != *current_page {
            out.push(0x00);
            out.push(page);
            *current_page = page;
        }
        let has_content = !node.children.is_empty();
        out.push(if has_content { token | 0x40 } else { token });
        if !has_content {
            return Ok(());
        }
        for child in &node.children {
            match child {
                EasChild::Text(text) => {
                    out.push(0x03);
                    out.extend_from_slice(text.as_bytes());
                    out.push(0x00);
                }
                EasChild::Node(child) => encode_node(child, out, current_page)?,
            }
        }
        out.push(0x01);
        Ok(())
    }
    encode_node(root, &mut out, &mut current_page)?;
    Ok(out)
}

fn wbxml_decode(bytes: &[u8]) -> Result<Option<EasNode>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut cursor = 0usize;
    cursor += 1;
    let _ = wbxml_decode_mb_u32(bytes, &mut cursor)?;
    let _ = wbxml_decode_mb_u32(bytes, &mut cursor)?;
    let string_table_len = wbxml_decode_mb_u32(bytes, &mut cursor)? as usize;
    cursor += string_table_len;

    let mut current_page = 0u8;
    let mut roots = Vec::<EasNode>::new();
    let mut stack = Vec::<EasNode>::new();

    while cursor < bytes.len() {
        let token = bytes[cursor];
        cursor += 1;
        match token {
            0x00 => {
                current_page = *bytes.get(cursor).unwrap_or(&0);
                cursor += 1;
            }
            0x01 => {
                if let Some(node) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(EasChild::Node(node));
                    } else {
                        roots.push(node);
                    }
                }
            }
            0x03 => {
                let start = cursor;
                while cursor < bytes.len() && bytes[cursor] != 0x00 {
                    cursor += 1;
                }
                let text = String::from_utf8_lossy(&bytes[start..cursor]).into_owned();
                cursor += 1;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(EasChild::Text(text));
                }
            }
            _ => {
                let has_content = token & 0x40 != 0;
                let tag = token & 0x3f;
                let name = eas_token_to_tag(current_page, tag)
                    .ok_or_else(|| {
                        anyhow!("Unknown WBXML token page={} token={}", current_page, tag)
                    })?
                    .to_string();
                let node = EasNode {
                    name,
                    children: Vec::new(),
                };
                if has_content {
                    stack.push(node);
                } else if let Some(parent) = stack.last_mut() {
                    parent.children.push(EasChild::Node(node));
                } else {
                    roots.push(node);
                }
            }
        }
    }

    while let Some(node) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(EasChild::Node(node));
        } else {
            roots.push(node);
        }
    }

    Ok(roots.into_iter().next())
}

struct ActiveSyncClient {
    server: String,
    username: String,
    password: String,
    path: String,
    protocol_version: String,
    policy_key: String,
    device_id: String,
    device_type: String,
    folder_sync_key: String,
    folders_by_id: BTreeMap<String, ActiveSyncFolder>,
    mail_sync_keys: BTreeMap<String, String>,
}

impl ActiveSyncClient {
    fn from_options(options: &EmailOptions) -> Result<Self> {
        let username = if options.active_sync_username.trim().is_empty() {
            options.email.clone()
        } else {
            options.active_sync_username.clone()
        };
        Ok(Self {
            server: options
                .active_sync_server
                .trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string(),
            username,
            password: options.password.clone(),
            path: options.active_sync_path.trim_start_matches('/').to_string(),
            protocol_version: options.active_sync_protocol_version.clone(),
            policy_key: options.active_sync_policy_key.clone(),
            device_id: if options.active_sync_device_id.trim().is_empty() {
                generated_sync_run_key(&options.email, "activesync-device", &now_iso_string())
            } else {
                options.active_sync_device_id.clone()
            },
            device_type: options.active_sync_device_type.clone(),
            folder_sync_key: "0".to_string(),
            folders_by_id: BTreeMap::new(),
            mail_sync_keys: BTreeMap::new(),
        })
    }

    fn base_url(&self) -> String {
        format!("https://{}/{}", self.server, self.path)
    }

    fn command_url(&self, cmd: &str) -> Result<String> {
        let mut url = Url::parse(&self.base_url()).context("invalid ActiveSync base url")?;
        url.query_pairs_mut()
            .append_pair("Cmd", cmd)
            .append_pair("User", &self.username)
            .append_pair("DeviceId", &self.device_id)
            .append_pair("DeviceType", &self.device_type);
        Ok(url.to_string())
    }

    fn headers(&self, include_policy: bool, content_type: bool) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        headers.insert(
            "authorization".to_string(),
            format!(
                "Basic {}",
                BASE64_STANDARD.encode(format!("{}:{}", self.username, self.password))
            ),
        );
        headers.insert(
            "ms-asprotocolversion".to_string(),
            self.protocol_version.clone(),
        );
        headers.insert("user-agent".to_string(), "CTOX-ActiveSync/1.0".to_string());
        headers.insert(
            "accept".to_string(),
            "application/vnd.ms-sync.wbxml, */*".to_string(),
        );
        if include_policy {
            headers.insert("x-ms-policykey".to_string(), self.policy_key.clone());
        }
        if content_type {
            headers.insert(
                "content-type".to_string(),
                "application/vnd.ms-sync.wbxml".to_string(),
            );
        }
        headers
    }

    fn options(&self) -> Result<()> {
        let response = http_request(
            "OPTIONS",
            &self.base_url(),
            &self.headers(false, false),
            None,
        )?;
        if response.status == 401 {
            bail!("ActiveSync auth failed (401). Username/Passwort pruefen.");
        }
        if !(200..300).contains(&response.status) {
            bail!("ActiveSync OPTIONS failed: HTTP {}", response.status);
        }
        Ok(())
    }

    fn command(&self, cmd: &str, node: &EasNode) -> Result<Option<EasNode>> {
        let body = wbxml_encode(node)?;
        let response = http_request(
            "POST",
            &self.command_url(cmd)?,
            &self.headers(true, true),
            Some(&body),
        )?;
        if response.status == 401 {
            bail!("ActiveSync auth failed (401). Username/Passwort pruefen.");
        }
        if response.status == 449 {
            bail!("ActiveSync server verlangt Device Provisioning (HTTP 449).");
        }
        if !(200..300).contains(&response.status) {
            bail!("ActiveSync {} failed: HTTP {}", cmd, response.status);
        }
        wbxml_decode(&response.body)
    }

    fn folder_sync(&mut self) -> Result<()> {
        let request = eas_elem(
            "FolderHierarchy:FolderSync",
            vec![eas_text("FolderHierarchy:SyncKey", &self.folder_sync_key)],
        );
        let root = self
            .command("FolderSync", &request)?
            .ok_or_else(|| anyhow!("ActiveSync FolderSync: ungueltige Antwort"))?;
        if root.name != "FolderHierarchy:FolderSync" {
            bail!("ActiveSync FolderSync: ungueltige Antwort");
        }
        let status = root
            .text_child("FolderHierarchy:Status")
            .unwrap_or_default();
        if !status.is_empty() && status != "1" {
            bail!("ActiveSync FolderSync status {status}");
        }
        if let Some(sync_key) = root.text_child("FolderHierarchy:SyncKey") {
            self.folder_sync_key = sync_key;
        }
        if let Some(changes) = root.child("FolderHierarchy:Changes") {
            for tag in ["FolderHierarchy:Add", "FolderHierarchy:Update"] {
                for node in changes.children_named(tag) {
                    if let Some(folder) = folder_from_node(node) {
                        self.folders_by_id.insert(folder.server_id.clone(), folder);
                    }
                }
            }
            for node in changes.children_named("FolderHierarchy:Delete") {
                if let Some(server_id) = node.text_child("FolderHierarchy:ServerId") {
                    self.folders_by_id.remove(&server_id);
                }
            }
        }
        if self.folders_by_id.is_empty() {
            if let Some(folders) = root.child("FolderHierarchy:Folders") {
                for node in folders.children_named("FolderHierarchy:Folder") {
                    if let Some(folder) = folder_from_node(node) {
                        self.folders_by_id.insert(folder.server_id.clone(), folder);
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve_folder_id(&mut self, folder_hint: &str) -> Result<String> {
        self.folder_sync()?;
        if self.folders_by_id.contains_key(folder_hint) {
            return Ok(folder_hint.to_string());
        }
        let wanted_type = match folder_hint.to_lowercase().as_str() {
            "inbox" | "important" => Some(2),
            "drafts" | "draft" => Some(3),
            "deleteditems" | "trash" => Some(4),
            "sentitems" | "sent" => Some(5),
            "outbox" => Some(6),
            _ => None,
        };
        if let Some(wanted_type) = wanted_type {
            if let Some(folder) = self
                .folders_by_id
                .values()
                .find(|folder| folder.folder_type == wanted_type)
            {
                return Ok(folder.server_id.clone());
            }
        }
        if let Some(folder) = self.folders_by_id.values().find(|folder| {
            folder
                .display_name
                .to_lowercase()
                .contains(&folder_hint.to_lowercase())
        }) {
            return Ok(folder.server_id.clone());
        }
        bail!("ActiveSync folder not found: {folder_hint}");
    }

    fn sync_folder(&mut self, folder_id: &str, page_size: usize) -> Result<Vec<MailboxMessage>> {
        let sync_key = self
            .mail_sync_keys
            .get(folder_id)
            .cloned()
            .unwrap_or_else(|| "0".to_string());
        let request = eas_elem(
            "AirSync:Sync",
            vec![EasChild::Node(eas_elem(
                "AirSync:Collections",
                vec![EasChild::Node(eas_elem(
                    "AirSync:Collection",
                    vec![
                        eas_text("AirSync:Class", "Email"),
                        eas_text("AirSync:SyncKey", &sync_key),
                        eas_text("AirSync:CollectionId", folder_id),
                        eas_text("AirSync:GetChanges", "1"),
                        eas_text("AirSync:WindowSize", &page_size.to_string()),
                        EasChild::Node(eas_elem(
                            "AirSync:Options",
                            vec![EasChild::Node(eas_elem(
                                "AirSyncBase:BodyPreference",
                                vec![
                                    eas_text("AirSyncBase:Type", "1"),
                                    eas_text("AirSyncBase:TruncationSize", "8192"),
                                ],
                            ))],
                        )),
                    ],
                ))],
            ))],
        );
        let root = self
            .command("Sync", &request)?
            .ok_or_else(|| anyhow!("ActiveSync Sync: ungueltige Antwort"))?;
        if root.name != "AirSync:Sync" {
            bail!("ActiveSync Sync: ungueltige Antwort");
        }
        let collection = root
            .child("AirSync:Collections")
            .and_then(|node| node.child("AirSync:Collection"))
            .ok_or_else(|| anyhow!("ActiveSync Sync: Collection fehlt"))?;
        let status = collection.text_child("AirSync:Status").unwrap_or_default();
        if !status.is_empty() && status != "1" {
            bail!("ActiveSync Sync status {status}");
        }
        if let Some(next_sync_key) = collection.text_child("AirSync:SyncKey") {
            self.mail_sync_keys
                .insert(folder_id.to_string(), next_sync_key);
        }
        let mut items = Vec::new();
        if let Some(commands) = collection.child("AirSync:Commands") {
            for tag in ["AirSync:Add", "AirSync:Change", "AirSync:Fetch"] {
                for node in commands.children_named(tag) {
                    let server_id = node.text_child("AirSync:ServerId").unwrap_or_default();
                    if let Some(app) = node.child("AirSync:ApplicationData") {
                        if let Some(item) =
                            normalize_activesync_mail_item(app, &server_id, folder_id)
                        {
                            items.push(item);
                        }
                    }
                }
            }
        }
        if let Some(responses) = collection.child("AirSync:Responses") {
            for node in responses.children_named("AirSync:Fetch") {
                let server_id = node.text_child("AirSync:ServerId").unwrap_or_default();
                if let Some(app) = node.child("AirSync:ApplicationData") {
                    if let Some(item) = normalize_activesync_mail_item(app, &server_id, folder_id) {
                        items.push(item);
                    }
                }
            }
        }
        let mut seen = BTreeSet::new();
        items.retain(|item| seen.insert(item.remote_id.clone()));
        Ok(items)
    }

    fn list_folder(
        &mut self,
        folder_hint: &str,
        page_size: usize,
        query: Option<&str>,
    ) -> Result<Vec<MailboxMessage>> {
        let folder_id = self.resolve_folder_id(folder_hint)?;
        let mut items = self.sync_folder(&folder_id, page_size)?;
        if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            let query = query.to_lowercase();
            items.retain(|item| {
                item.sender_display.to_lowercase().contains(&query)
                    || item.sender_address.to_lowercase().contains(&query)
                    || item.subject.to_lowercase().contains(&query)
                    || item.preview.to_lowercase().contains(&query)
            });
        }
        items.truncate(page_size);
        Ok(items)
    }
}

fn folder_from_node(node: &EasNode) -> Option<ActiveSyncFolder> {
    Some(ActiveSyncFolder {
        server_id: node.text_child("FolderHierarchy:ServerId")?,
        display_name: node
            .text_child("FolderHierarchy:DisplayName")
            .unwrap_or_default(),
        folder_type: node
            .text_child("FolderHierarchy:Type")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default(),
    })
}

fn normalize_activesync_mail_item(
    app_node: &EasNode,
    server_id: &str,
    folder_id: &str,
) -> Option<MailboxMessage> {
    let subject = app_node
        .text_child("Email:Subject")
        .unwrap_or_else(|| "(ohne Betreff)".to_string());
    let from_raw = app_node.text_child("Email:From").unwrap_or_default();
    let sender_address = extract_address(&from_raw);
    let sender_display = extract_display_name(&from_raw).unwrap_or_else(|| {
        if sender_address.is_empty() {
            from_raw.clone()
        } else {
            sender_address.clone()
        }
    });
    let body = app_node
        .child("AirSyncBase:Body")
        .and_then(|node| node.text_child("AirSyncBase:Data"))
        .or_else(|| app_node.text_child("Email:Body"))
        .unwrap_or_default();
    let preview = app_node
        .child("AirSyncBase:Body")
        .and_then(|node| node.text_child("AirSyncBase:Preview"))
        .unwrap_or_else(|| preview_text(&body, &subject));
    Some(MailboxMessage {
        remote_id: server_id.to_string(),
        thread_key: server_id.to_string(),
        folder_hint: mailbox_folder_to_hint(folder_id),
        subject,
        sender_display,
        sender_address,
        recipient_addresses: extract_addresses(
            &app_node.text_child("Email:To").unwrap_or_default(),
        ),
        cc_addresses: extract_addresses(&app_node.text_child("Email:Cc").unwrap_or_default()),
        body_text: body.clone(),
        body_html: String::new(),
        preview,
        seen: app_node
            .text_child("Email:Read")
            .map(|value| value == "1")
            .unwrap_or(false),
        has_attachments: false,
        external_created_at: app_node
            .text_child("Email:DateReceived")
            .unwrap_or_else(now_iso_string),
        metadata: json!({
            "activeSyncFolderId": folder_id,
        }),
    })
}

/// The Graph adapter accepts a dedicated `CTO_EMAIL_GRAPH_USERNAME`, but for
/// the common single-account case ROPC just needs the mailbox owner. Fall
/// back to `CTO_EMAIL_ADDRESS` so a minimal config (`provider=graph`,
/// `CTO_EMAIL_ADDRESS`, `CTO_EMAIL_PASSWORD`) is sufficient.
fn effective_graph_username(options: &EmailOptions) -> String {
    let explicit = options.graph_username.trim();
    if explicit.is_empty() {
        options.email.trim().to_string()
    } else {
        explicit.to_string()
    }
}

/// Same fallback story as `effective_graph_username`: prefer the dedicated
/// Graph credential, otherwise reuse `CTO_EMAIL_PASSWORD`.
fn effective_graph_password(options: &EmailOptions) -> String {
    if !options.graph_password.is_empty() {
        options.graph_password.clone()
    } else {
        options.password.clone()
    }
}

/// Acquire an access token for the Graph adapter when the operator has not
/// supplied a long-lived `CTO_EMAIL_GRAPH_ACCESS_TOKEN`. Prefers ROPC when a
/// username + password are reachable (with the well-known Microsoft Office
/// public client id as a default), otherwise falls back to the OAuth2
/// client-credentials flow.
fn acquire_graph_access_token(options: &EmailOptions) -> Result<String> {
    let username = effective_graph_username(options);
    let password = effective_graph_password(options);
    let has_user_creds = !username.trim().is_empty() && !password.is_empty();
    let has_app_creds = !options.graph_tenant_id.trim().is_empty()
        && !options.graph_client_id.trim().is_empty()
        && !options.graph_client_secret.trim().is_empty();

    if has_user_creds {
        let client_id = if options.graph_client_id.trim().is_empty() {
            ROPC_PUBLIC_CLIENT_ID.to_string()
        } else {
            options.graph_client_id.clone()
        };
        return acquire_ropc_token(&options.graph_tenant_id, &username, &password, &client_id);
    }
    if has_app_creds {
        return acquire_app_token(
            &options.graph_tenant_id,
            &options.graph_client_id,
            &options.graph_client_secret,
        );
    }
    bail!(
        "Graph adapter has no usable credentials: set CTO_EMAIL_GRAPH_ACCESS_TOKEN, \
         OR provide a username+password (CTO_EMAIL_GRAPH_USERNAME/PASSWORD or \
         CTO_EMAIL_ADDRESS/CTO_EMAIL_PASSWORD) for ROPC, OR provide \
         CTO_EMAIL_GRAPH_TENANT_ID + CTO_EMAIL_GRAPH_CLIENT_ID + \
         CTO_EMAIL_GRAPH_CLIENT_SECRET for the client-credentials flow."
    )
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_graph_access_token, build_ews_file_attachments_xml, effective_graph_password,
        effective_graph_username, extract_address, latest_imap_uids, require_provider_credentials,
        synced_message_direction, EmailOptions,
    };
    use std::io::Write;
    use std::path::PathBuf;

    fn empty_options() -> EmailOptions {
        EmailOptions {
            db_path: PathBuf::new(),
            raw_dir: PathBuf::new(),
            email: String::new(),
            provider: String::new(),
            folder: String::new(),
            limit: 0,
            trust_level: String::new(),
            verify_send: false,
            sent_verify_window_seconds: 0,
            imap_host: String::new(),
            imap_port: 0,
            smtp_host: String::new(),
            smtp_port: 0,
            password: String::new(),
            graph_access_token: String::new(),
            graph_base_url: String::new(),
            graph_user: String::new(),
            graph_tenant_id: String::new(),
            graph_client_id: String::new(),
            graph_client_secret: String::new(),
            graph_username: String::new(),
            graph_password: String::new(),
            ews_url: String::new(),
            owa_url: String::new(),
            ews_version: String::new(),
            ews_auth_type: String::new(),
            ews_username: String::new(),
            ews_bearer_token: String::new(),
            active_sync_server: String::new(),
            active_sync_username: String::new(),
            active_sync_path: String::new(),
            active_sync_device_id: String::new(),
            active_sync_device_type: String::new(),
            active_sync_protocol_version: String::new(),
            active_sync_policy_key: String::new(),
        }
    }

    fn graph_options_with_token(token: &str) -> EmailOptions {
        let mut options = empty_options();
        options.email = "user@example.com".into();
        options.provider = "graph".into();
        options.graph_access_token = token.into();
        options
    }
    #[test]
    fn synced_message_direction_treats_self_authored_mail_as_outbound() {
        assert_eq!(
            synced_message_direction("CTO1 <cto1@metric-space.ai>", "cto1@metric-space.ai"),
            "outbound"
        );
    }

    #[test]
    fn synced_message_direction_keeps_external_sender_as_inbound() {
        assert_eq!(
            synced_message_direction(
                "Michael Welsch <michael.welsch@metric-space.ai>",
                "cto1@metric-space.ai"
            ),
            "inbound"
        );
    }

    #[test]
    fn extract_address_normalizes_wrapped_email() {
        assert_eq!(
            extract_address("CTO1 <cto1@metric-space.ai>"),
            "cto1@metric-space.ai"
        );
    }

    #[test]
    fn latest_imap_uids_sorts_numeric_uids_not_lexicographic_strings() {
        let selected = latest_imap_uids(
            vec![
                "98".to_string(),
                "99".to_string(),
                "100".to_string(),
                "101".to_string(),
                "9".to_string(),
            ],
            3,
        );
        assert_eq!(selected, vec!["101", "100", "99"]);
    }

    #[test]
    fn graph_username_falls_back_to_email() {
        let mut options = empty_options();
        options.email = "yoda@example.com".into();
        assert_eq!(effective_graph_username(&options), "yoda@example.com");
        options.graph_username = "graphuser@example.com".into();
        assert_eq!(effective_graph_username(&options), "graphuser@example.com");
    }

    #[test]
    fn graph_password_falls_back_to_email_password() {
        let mut options = empty_options();
        options.password = "imap-pw".into();
        assert_eq!(effective_graph_password(&options), "imap-pw");
        options.graph_password = "graph-pw".into();
        assert_eq!(effective_graph_password(&options), "graph-pw");
    }

    #[test]
    fn require_provider_credentials_accepts_graph_with_pre_acquired_token() {
        let options = graph_options_with_token("eyJ0eXAi...redacted...");
        assert!(require_provider_credentials(&options).is_ok());
    }

    #[test]
    fn require_provider_credentials_accepts_graph_with_user_password_fallback() {
        let mut options = graph_options_with_token("");
        options.password = "secret".into();
        assert!(require_provider_credentials(&options).is_ok());
    }

    #[test]
    fn require_provider_credentials_accepts_graph_with_client_credentials() {
        let mut options = graph_options_with_token("");
        options.graph_tenant_id = "contoso".into();
        options.graph_client_id = "client-id".into();
        options.graph_client_secret = "client-secret".into();
        assert!(require_provider_credentials(&options).is_ok());
    }

    #[test]
    fn require_provider_credentials_rejects_graph_without_any_credentials() {
        let options = graph_options_with_token("");
        let err = require_provider_credentials(&options).unwrap_err();
        assert!(err.to_string().contains("Graph adapter"));
    }

    #[test]
    fn ews_file_attachment_payload_embeds_file_content() {
        let mut file = tempfile::Builder::new()
            .suffix(".csv")
            .tempfile()
            .expect("create temp attachment");
        file.write_all(b"a,b\n").expect("write attachment");
        let path = file.path().display().to_string();

        let xml = build_ews_file_attachments_xml(&[path]).expect("build EWS attachment xml");

        assert!(xml.contains("<t:Attachments>"));
        assert!(xml.contains("<t:FileAttachment>"));
        assert!(xml.contains("<t:ContentType>text/csv; charset=utf-8</t:ContentType>"));
        assert!(xml.contains("<t:Content>YSxiCg==</t:Content>"));
    }

    #[test]
    fn acquire_graph_access_token_rejects_when_no_credentials() {
        let options = graph_options_with_token("");
        let err = acquire_graph_access_token(&options).unwrap_err();
        assert!(err.to_string().contains("no usable credentials"));
    }

    #[test]
    fn classify_auto_submitted_header_recognises_rfc3834_keywords() {
        use super::classify_auto_submitted_header;
        // Missing or `no` is not auto-submitted.
        assert_eq!(classify_auto_submitted_header(None), (false, None));
        let (flag, raw) = classify_auto_submitted_header(Some("no"));
        assert!(!flag);
        assert_eq!(raw.as_deref(), Some("no"));

        // RFC 3834 keywords (the structurally-defined set we care about).
        for keyword in ["auto-replied", "auto-generated", "auto-notified"] {
            let (flag, raw) = classify_auto_submitted_header(Some(keyword));
            assert!(
                flag,
                "expected `{keyword}` to be classified as auto-submitted",
            );
            assert_eq!(raw.as_deref(), Some(keyword));
        }

        // RFC 3834 §5 allows `;`-separated parameters after the token;
        // we must only inspect the leading token.
        let (flag, raw) = classify_auto_submitted_header(Some("auto-replied; foo=bar"));
        assert!(flag);
        assert_eq!(raw.as_deref(), Some("auto-replied; foo=bar"));

        // Whitespace-tolerant.
        let (flag, _) = classify_auto_submitted_header(Some("  auto-replied  "));
        assert!(flag);
    }

    #[test]
    fn parse_rfc822_message_extracts_auto_submitted_marker_for_outlook_ooo() {
        use super::parse_rfc822_message;
        // Realistic Outlook OoO synthetic mail: RFC 3834
        // Auto-Submitted plus the Outlook X-Auto-Response-Suppress
        // defense-in-depth header. Subject is German on purpose to
        // make sure we are NOT relying on string matching.
        let raw = b"From: jill@example.org\r\n\
            To: yoda@example.org\r\n\
            Subject: =?utf-8?B?QXV0b21hdGlzY2hlIEFudHdvcnQ6IGFsbGVz?=\r\n\
            Date: Mon, 27 Apr 2026 09:00:00 +0000\r\n\
            Message-ID: <ooo-1@example.org>\r\n\
            Auto-Submitted: auto-replied\r\n\
            X-Auto-Response-Suppress: All\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            Bin im Urlaub bis 2026-05-12.\r\n";
        let parsed = parse_rfc822_message(raw).expect("parse OoO mail");
        assert!(parsed.auto_submitted, "Auto-Submitted: auto-replied");
        assert_eq!(parsed.auto_submitted_value.as_deref(), Some("auto-replied"));
        assert!(
            parsed.auto_response_suppress,
            "X-Auto-Response-Suppress present"
        );
    }

    #[test]
    fn parse_rfc822_message_does_not_flag_human_reply_as_auto_submitted() {
        use super::parse_rfc822_message;
        let raw = b"From: jill@example.org\r\n\
            To: yoda@example.org\r\n\
            Subject: Re: REM Capital next steps\r\n\
            Date: Mon, 27 Apr 2026 09:00:00 +0000\r\n\
            Message-ID: <human-1@example.org>\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            Danke fuer den Status!\r\n";
        let parsed = parse_rfc822_message(raw).expect("parse human mail");
        assert!(!parsed.auto_submitted);
        assert_eq!(parsed.auto_submitted_value, None);
        assert!(!parsed.auto_response_suppress);
    }

    #[test]
    fn parse_rfc822_message_treats_explicit_no_marker_as_human_reply() {
        use super::parse_rfc822_message;
        let raw = b"From: jill@example.org\r\n\
            To: yoda@example.org\r\n\
            Subject: Re: REM Capital next steps\r\n\
            Date: Mon, 27 Apr 2026 09:00:00 +0000\r\n\
            Message-ID: <human-2@example.org>\r\n\
            Auto-Submitted: no\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            Habe ich gelesen.\r\n";
        let parsed = parse_rfc822_message(raw).expect("parse no-marker");
        assert!(!parsed.auto_submitted);
        assert_eq!(parsed.auto_submitted_value.as_deref(), Some("no"));
    }

    #[test]
    fn parse_rfc822_message_records_attachment_metadata() {
        use super::parse_rfc822_message;
        let raw = b"From: jill@example.org\r\n\
            To: yoda@example.org\r\n\
            Subject: Datei\r\n\
            Message-ID: <file-1@example.org>\r\n\
            MIME-Version: 1.0\r\n\
            Content-Type: multipart/mixed; boundary=\"b\"\r\n\
            \r\n\
            --b\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            siehe Anhang\r\n\
            --b\r\n\
            Content-Type: text/csv; name=\"result.csv\"\r\n\
            Content-Disposition: attachment; filename=\"result.csv\"\r\n\
            Content-Transfer-Encoding: base64\r\n\
            \r\n\
            YSxiCg==\r\n\
            --b--\r\n";

        let parsed = parse_rfc822_message(raw).expect("parse mail with attachment");

        assert!(parsed.has_attachments);
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0]["name"], "result.csv");
        assert_eq!(parsed.attachments[0]["contentType"], "text/csv");
    }
}
