use anyhow::{anyhow, bail, Context, Result};
use git2::{Repository, Sort};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};

use crate::mission::channels::{
    ensure_account, ensure_routing_rows_for_inbound, now_iso_string, open_channel_db, preview_text,
    record_communication_sync_run, refresh_thread, stable_digest, upsert_communication_message,
    CommunicationSyncRun, UpsertMessage,
};
use crate::mission::communication_adapters::{
    AdapterSyncCommandRequest, JamiResolveAccountCommandRequest, JamiSendCommandRequest,
    JamiTestCommandRequest,
};

const JAMI_DBUS_SERVICE: &str = "cx.ring.Ring";
const JAMI_DBUS_OBJECT_PATH: &str = "/cx/ring/Ring/ConfigurationManager";
const JAMI_DBUS_INTERFACE: &str = "cx.ring.Ring.ConfigurationManager";

#[derive(Clone, Debug)]
struct JamiOptions {
    root: PathBuf,
    db_path: PathBuf,
    raw_dir: PathBuf,
    inbox_dir: PathBuf,
    outbox_dir: PathBuf,
    archive_dir: PathBuf,
    account_id: String,
    /// The Jami username hash of the own account, used to filter self-echo.
    username: String,
    profile_name: String,
    account_uri: String,
    device_name: String,
    provider: String,
    limit: usize,
    trust_level: String,
    dbus_env_file: String,
    transcription_model: String,
    speech_model: String,
    speech_voice: String,
}

#[derive(Clone, Debug)]
struct JamiResolvedAccount {
    account_id: String,
    account_type: String,
    enabled: bool,
    registration_status: String,
    username: String,
    share_uri: String,
    display_name: String,
    details: Value,
    volatile_details: Value,
    provisioned: bool,
}

#[derive(Clone, Debug)]
struct JamiInboundMessage {
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

#[derive(Clone, Debug)]
struct JamiOutboundDelivery {
    remote_uri: Option<String>,
    conversation_id: Option<String>,
    voice_attachment: Option<PathBuf>,
    submitted_text: bool,
    submitted_file: bool,
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
    request: &JamiSendCommandRequest<'_>,
) -> Result<Value> {
    let options = send_options_from_request(root, runtime, request);
    execute_send(&options, request)
}

pub(crate) fn test(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &JamiTestCommandRequest<'_>,
) -> Result<Value> {
    let options = test_options_from_request(root, runtime, request);
    execute_test(&options)
}

pub(crate) fn resolve_account(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &JamiResolveAccountCommandRequest<'_>,
) -> Result<Value> {
    let mut options =
        base_options_from_runtime(root, runtime, root.join("runtime/ctox.sqlite3").as_path());
    if let Some(account_id) = request.account_id.filter(|value| !value.trim().is_empty()) {
        options.account_id = account_id.trim().to_string();
    }
    if let Some(profile_name) = request
        .profile_name
        .filter(|value| !value.trim().is_empty())
    {
        options.profile_name = profile_name.trim().to_string();
    }
    resolve_account_response(&options, true)
}

pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let preferred_is_jami = settings
        .get("CTOX_OWNER_PREFERRED_CHANNEL")
        .map(|value| value.trim())
        == Some("jami");
    let account_id = settings
        .get("CTO_JAMI_ACCOUNT_ID")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let profile_name = settings
        .get("CTO_JAMI_PROFILE_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    if !preferred_is_jami && account_id.is_none() {
        return Ok(None);
    }
    if account_id.is_none() && profile_name.is_none() {
        return Ok(None);
    }
    let mut args = vec!["sync".to_string()];
    if let Some(account_id) = account_id {
        args.push("--account-id".to_string());
        args.push(account_id.to_string());
    }
    if let Some(profile_name) = profile_name {
        args.push("--profile-name".to_string());
        args.push(profile_name.to_string());
    }
    let runtime = runtime_from_settings(root, settings);
    let db_path = root.join("runtime/ctox.sqlite3");
    let request = AdapterSyncCommandRequest {
        db_path: db_path.as_path(),
        passthrough_args: &args,
        skip_flags: &["--db", "--channel"],
    };
    sync(root, &runtime, &request).map(Some)
}

pub(crate) fn handle_daemon_command(_root: &Path, args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) != Some("--foreground") {
        bail!("usage: ctox jami-daemon --foreground");
    }
    if cfg!(not(target_os = "linux")) {
        bail!("ctox jami-daemon is only supported on Linux");
    }
    let dbus_address = resolve_linux_session_bus_address()?;
    let dbus_env_file = write_jami_dbus_env_file(&dbus_address)?;
    let daemon_bin = find_jami_daemon_binary()?;
    let daemon_args = jami_daemon_args();
    let status = Command::new(&daemon_bin)
        .args(&daemon_args)
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("CTO_JAMI_DBUS_ENV_FILE", &dbus_env_file)
        .status()
        .with_context(|| format!("failed to start Jami daemon {}", daemon_bin.display()))?;
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    bail!("Jami daemon exited with status {code}")
}

fn execute_send(options: &JamiOptions, request: &JamiSendCommandRequest<'_>) -> Result<Value> {
    let mut options = options.clone();
    let resolved = resolve_jami_account(&mut options, true)?;
    ensure_dirs(&options)?;
    let mut conn = open_channel_db(&options.db_path)?;
    let account_key = account_key_from_jami(&options.account_id);
    ensure_account(
        &mut conn,
        &account_key,
        "jami",
        &options.account_id,
        &options.provider,
        build_profile_json(&options),
    )?;
    let timestamp = now_iso_string();
    let remote_id = format!(
        "queued-{}",
        stable_digest(&format!("{}:{}", timestamp, request.body))
    );
    let thread_key = if request.thread_key.trim().is_empty() {
        format!("{account_key}::{}", sanitize_file_component(&remote_id))
    } else {
        request.thread_key.to_string()
    };
    let subject = if request.subject.trim().is_empty() {
        "(Jami)".to_string()
    } else {
        request.subject.to_string()
    };
    let requested_modality = if request.send_voice { "voice" } else { "text" };
    let sender_display = if options.profile_name.trim().is_empty() {
        resolved.display_name.clone()
    } else {
        options.profile_name.clone()
    };
    let delivery = submit_jami_delivery(&options, request, &remote_id)?;
    let final_thread_key = delivery
        .conversation_id
        .as_ref()
        .map(|conversation_id| thread_key_for_conversation(&account_key, conversation_id))
        .unwrap_or_else(|| thread_key.clone());
    let raw_payload_ref = delivery
        .voice_attachment
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key_from_remote(&account_key, "OUTBOUND", &remote_id),
            channel: "jami",
            account_key: &account_key,
            thread_key: &final_thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "SENT",
            sender_display: &sender_display,
            sender_address: &options.account_id,
            recipient_addresses_json: &serde_json::to_string(
                &request
                    .to
                    .iter()
                    .map(|value| value.trim().to_string())
                    .collect::<Vec<_>>(),
            )?,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &subject,
            preview: &preview_text(request.body, &subject),
            body_text: request.body,
            body_html: "",
            raw_payload_ref: &raw_payload_ref,
            trust_level: &options.trust_level,
            status: "submitted",
            seen: true,
            has_attachments: request.send_voice,
            external_created_at: &timestamp,
            observed_at: &timestamp,
            metadata_json: &serde_json::to_string(&json!({
                "backend": "jami-dbus",
                "delivery": "submitted_to_jami_dbus",
                "requestedModality": requested_modality,
                "remoteUri": delivery.remote_uri,
                "conversationId": delivery.conversation_id,
                "submittedText": delivery.submitted_text,
                "submittedFile": delivery.submitted_file,
                "voiceAttachmentPath": delivery.voice_attachment.as_ref().map(|path| path.display().to_string()),
            }))?,
        },
    )?;
    refresh_thread(&mut conn, &final_thread_key)?;
    Ok(json!({
        "ok": true,
        "status": "submitted",
        "submitted": true,
        "delivery": {
            "confirmed": false,
            "method": "jami-dbus",
            "state": "submitted_to_daemon",
            "requestedModality": requested_modality,
            "remoteUri": delivery.remote_uri,
            "conversationId": delivery.conversation_id,
            "submittedText": delivery.submitted_text,
            "submittedFile": delivery.submitted_file,
            "voiceAttachmentPath": delivery.voice_attachment.as_ref().map(|path| path.display().to_string()),
        },
        "accountKey": account_key,
        "to": request.to,
        "subject": subject,
        "threadKey": final_thread_key,
        "dbPath": options.db_path,
    }))
}

fn execute_test(options: &JamiOptions) -> Result<Value> {
    let mut options = options.clone();
    ensure_dirs(&options)?;
    let checks = collect_jami_doctor_checks(&mut options);
    let mut resolved = None;
    let mut resolve_error = String::new();
    match resolve_jami_account(&mut options, true) {
        Ok(account) => resolved = Some(account),
        Err(error) => resolve_error = error.to_string(),
    }
    let mut checks = checks;
    checks.push(json!({
        "name": "account_resolution",
        "ok": resolved.is_some(),
        "detail": resolved
            .as_ref()
            .map(|account| format!("resolved {}", if account.share_uri.is_empty() { &account.account_id } else { &account.share_uri }))
            .unwrap_or_else(|| if resolve_error.is_empty() { "Jami account could not be resolved".to_string() } else { resolve_error.clone() }),
    }));

    let account_key = if options.account_id.trim().is_empty() {
        String::new()
    } else {
        account_key_from_jami(&options.account_id)
    };
    if !account_key.is_empty() {
        let mut conn = open_channel_db(&options.db_path)?;
        ensure_account(
            &mut conn,
            &account_key,
            "jami",
            &options.account_id,
            &options.provider,
            build_profile_json(&options),
        )?;
    }

    let dbus_probe = if load_jami_dbus_environment(&mut options).is_ok()
        && !options.account_id.trim().is_empty()
    {
        get_conversations(&options.account_id).map(|_| "conversation probe succeeded".to_string())
    } else {
        Err(anyhow!("DBus session unavailable"))
    };
    checks.push(json!({
        "name": "dbus_probe",
        "ok": dbus_probe.is_ok(),
        "detail": dbus_probe.unwrap_or_else(|error| error.to_string()),
    }));
    Ok(json!({
        "ok": checks.iter().all(|item| item.get("ok").and_then(Value::as_bool).unwrap_or(false)),
        "channel": "jami",
        "accountKey": account_key,
        "resolvedAccount": resolved.as_ref().map(jami_account_json),
        "checks": checks,
        "error": if resolve_error.is_empty() { Value::Null } else { Value::String(resolve_error) },
        "dbPath": options.db_path,
    }))
}

fn execute_sync(options: &JamiOptions) -> Result<Value> {
    let mut options = options.clone();
    ensure_dirs(&options)?;
    require_explicit_jami_identity(&options)?;
    let resolved = resolve_jami_account(&mut options, false)?;
    // Populate own username so the self-echo filter in normalize_jami_git_commit works.
    if options.username.is_empty() && !resolved.username.is_empty() {
        options.username = resolved.username.clone();
    }
    let account_key = account_key_from_jami(&options.account_id);
    let mut conn = open_channel_db(&options.db_path)?;
    ensure_account(
        &mut conn,
        &account_key,
        "jami",
        &options.account_id,
        &options.provider,
        build_profile_json(&options),
    )?;
    let started_at = now_iso_string();
    let mut fetched_count = 0i64;
    let mut stored_count = 0i64;
    let sync_result = (|| -> Result<()> {
        let _ = load_jami_dbus_environment(&mut options);
        if !options.account_id.trim().is_empty() {
            let _ = accept_pending_jami_requests(&options.account_id);
        }
        let conversation_ids = get_conversations(&options.account_id).unwrap_or_default();
        for loaded in load_conversation_messages_from_git(&options, &conversation_ids)? {
            for entry in loaded.messages {
                fetched_count += 1;
                if let Some(inbound) = normalize_jami_git_commit(
                    &options,
                    &loaded.conversation_id,
                    &loaded.repo_path,
                    &entry,
                )? {
                    let raw_entry = json!({
                        "source": "git",
                        "conversationId": loaded.conversation_id,
                        "repoPath": loaded.repo_path.display().to_string(),
                        "remoteId": entry.remote_id,
                        "timestamp": entry.timestamp,
                        "authorDevice": entry.author_device,
                        "subject": entry.subject,
                    });
                    let already_known =
                        store_inbound_message(&mut conn, &options, &inbound, &raw_entry)?;
                    if !already_known {
                        stored_count += 1;
                    }
                }
            }
        }
        for source_file in load_source_files(&options.inbox_dir, options.limit)? {
            let entries = load_source_entries(&source_file)?;
            let mut archived = Vec::new();
            fetched_count += entries.len() as i64;
            for (index, entry) in entries.into_iter().enumerate() {
                let inbound = normalize_inbound_entry(&options, &entry, &source_file, index);
                let already_known = store_inbound_message(&mut conn, &options, &inbound, &entry)?;
                if !already_known {
                    stored_count += 1;
                }
                archived.push(entry);
            }
            archive_source_file(&source_file, &options.archive_dir, &archived)?;
        }
        ensure_routing_rows_for_inbound(&conn)?;
        Ok(())
    })();
    let finished_at = now_iso_string();
    let error_text = sync_result
        .as_ref()
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    record_communication_sync_run(
        &mut conn,
        CommunicationSyncRun {
            run_key: &format!(
                "jami-sync-{}",
                stable_digest(&format!("{}:{}:{}", account_key, started_at, stored_count))
            ),
            channel: "jami",
            account_key: &account_key,
            folder_hint: "INBOX",
            started_at: &started_at,
            finished_at: &finished_at,
            ok: sync_result.is_ok(),
            fetched_count,
            stored_count,
            error_text: &error_text,
            metadata_json: &serde_json::to_string(&json!({
                "adapter": "native-rust-jami",
                "resolvedAccount": jami_account_json(&resolved),
            }))?,
        },
    )?;
    sync_result?;
    Ok(json!({
        "ok": true,
        "accountKey": account_key,
        "fetchedCount": fetched_count,
        "storedCount": stored_count,
        "inboxDir": options.inbox_dir.display().to_string(),
        "archiveDir": options.archive_dir.display().to_string(),
        "dbPath": options.db_path,
    }))
}

fn resolve_account_response(options: &JamiOptions, provision_if_missing: bool) -> Result<Value> {
    let mut options = options.clone();
    let mut checks = collect_jami_doctor_checks(&mut options);
    if cfg!(target_os = "macos") {
        return Ok(json!({
            "ok": false,
            "resolvedAccount": Value::Null,
            "error": "Official Jami macOS builds use the libwrap API, while the current CTOX Jami adapter expects a DBus-compatible backend. Configure a share URI manually for QR onboarding or use a Linux Jami runtime for automation.",
            "dbusEnvFile": options.dbus_env_file,
            "checks": checks,
        }));
    }
    let resolved = resolve_jami_account(&mut options, provision_if_missing);
    checks.push(json!({
        "name": "account_resolution",
        "ok": resolved.is_ok(),
        "detail": resolved
            .as_ref()
            .map(|account| format!("resolved {}", if account.share_uri.is_empty() { &account.account_id } else { &account.share_uri }))
            .unwrap_or_else(|error| error.to_string()),
    }));
    match resolved {
        Ok(account) => Ok(json!({
            "ok": true,
            "resolvedAccount": jami_account_json(&account),
            "dbusEnvFile": options.dbus_env_file,
            "checks": checks,
        })),
        Err(error) => Ok(json!({
            "ok": false,
            "resolvedAccount": Value::Null,
            "error": error.to_string(),
            "dbusEnvFile": options.dbus_env_file,
            "checks": checks,
        })),
    }
}

fn sync_options_from_args(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<JamiOptions> {
    let args = request.passthrough_args;
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    if let Some(account_id) = optional_flag(args, "--account-id") {
        options.account_id = account_id.to_string();
    }
    if let Some(profile_name) = optional_flag(args, "--profile-name") {
        options.profile_name = profile_name.to_string();
    }
    if let Some(limit) = optional_flag(args, "--limit") {
        options.limit = limit.parse::<usize>().unwrap_or(options.limit);
    }
    if let Some(trust_level) = optional_flag(args, "--trust-level") {
        options.trust_level = trust_level.to_string();
    }
    if let Some(value) = optional_flag(args, "--inbox-dir") {
        options.inbox_dir = PathBuf::from(value);
    }
    if let Some(value) = optional_flag(args, "--outbox-dir") {
        options.outbox_dir = PathBuf::from(value);
    }
    if let Some(value) = optional_flag(args, "--archive-dir") {
        options.archive_dir = PathBuf::from(value);
    }
    if let Some(value) = optional_flag(args, "--dbus-env-file") {
        options.dbus_env_file = value.to_string();
    }
    Ok(options)
}

fn send_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &JamiSendCommandRequest<'_>,
) -> JamiOptions {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    options.account_id = request.account_id.to_string();
    if let Some(profile_name) = request
        .sender_display
        .filter(|value| !value.trim().is_empty())
    {
        options.profile_name = profile_name.to_string();
    }
    options
}

fn test_options_from_request(
    root: &Path,
    runtime: &BTreeMap<String, String>,
    request: &JamiTestCommandRequest<'_>,
) -> JamiOptions {
    let mut options = base_options_from_runtime(root, runtime, request.db_path);
    options.account_id = request.account_id.to_string();
    options.provider = request.provider.to_string();
    if let Some(object) = request.profile_json.as_object() {
        if let Some(profile_name) = object.get("profileName").and_then(Value::as_str) {
            options.profile_name = profile_name.to_string();
        }
        if let Some(inbox_dir) = object.get("inboxDir").and_then(Value::as_str) {
            options.inbox_dir = PathBuf::from(inbox_dir);
        }
        if let Some(outbox_dir) = object.get("outboxDir").and_then(Value::as_str) {
            options.outbox_dir = PathBuf::from(outbox_dir);
        }
        if let Some(archive_dir) = object.get("archiveDir").and_then(Value::as_str) {
            options.archive_dir = PathBuf::from(archive_dir);
        }
        if let Some(dbus_env_file) = object.get("dbusEnvFile").and_then(Value::as_str) {
            options.dbus_env_file = dbus_env_file.to_string();
        }
    }
    options
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
) -> JamiOptions {
    JamiOptions {
        root: root.to_path_buf(),
        db_path: db_path.to_path_buf(),
        raw_dir: root.join("runtime/communication/jami/raw"),
        inbox_dir: root.join(
            runtime
                .get("CTO_JAMI_INBOX_DIR")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or("runtime/communication/jami/inbox"),
        ),
        outbox_dir: root.join(
            runtime
                .get("CTO_JAMI_OUTBOX_DIR")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or("runtime/communication/jami/outbox"),
        ),
        archive_dir: root.join(
            runtime
                .get("CTO_JAMI_ARCHIVE_DIR")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or("runtime/communication/jami/archive"),
        ),
        account_id: runtime
            .get("CTO_JAMI_ACCOUNT_ID")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
        username: String::new(),
        profile_name: runtime
            .get("CTO_JAMI_PROFILE_NAME")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
        account_uri: String::new(),
        device_name: String::new(),
        provider: "jami".to_string(),
        limit: runtime
            .get("CTO_JAMI_LIMIT")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(50),
        trust_level: runtime
            .get("CTO_JAMI_TRUST_LEVEL")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "low".to_string()),
        dbus_env_file: runtime
            .get("CTO_JAMI_DBUS_ENV_FILE")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
        transcription_model: runtime
            .get("CTOX_STT_MODEL")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
        speech_model: runtime
            .get("CTOX_TTS_MODEL")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
        speech_voice: runtime
            .get("CTO_JAMI_TTS_VOICE")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
    }
}

fn ensure_dirs(options: &JamiOptions) -> Result<()> {
    for dir in [
        &options.raw_dir,
        &options.inbox_dir,
        &options.outbox_dir,
        &options.archive_dir,
    ] {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create Jami directory {}", dir.display()))?;
    }
    Ok(())
}

fn build_profile_json(options: &JamiOptions) -> Value {
    json!({
        "inboxDir": options.inbox_dir.display().to_string(),
        "outboxDir": options.outbox_dir.display().to_string(),
        "archiveDir": options.archive_dir.display().to_string(),
        "profileName": options.profile_name,
        "dbusEnvFile": options.dbus_env_file,
        "accountId": options.account_id,
    })
}

fn account_key_from_jami(account_id: &str) -> String {
    let normalized = account_id.trim().to_lowercase();
    if normalized.starts_with("jami:") {
        normalized
    } else {
        format!("jami:{normalized}")
    }
}

fn message_key_from_remote(account_key: &str, folder: &str, remote_id: &str) -> String {
    format!("{account_key}::{folder}::{remote_id}")
}

fn sanitize_file_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .chars()
        .take(120)
        .collect()
}

fn collect_jami_doctor_checks(options: &mut JamiOptions) -> Vec<Value> {
    let mut checks = Vec::new();
    checks.push(json!({
        "name": "platform",
        "ok": true,
        "detail": std::env::consts::OS,
    }));
    let backend_ok = !cfg!(target_os = "macos");
    checks.push(json!({
        "name": "automation_backend",
        "ok": backend_ok,
        "detail": if backend_ok {
            "DBus-style automation backend available on this platform"
        } else {
            "Official Jami macOS builds use libwrap, not a separate DBus daemon; the current CTOX Jami adapter cannot automate them yet"
        },
    }));
    let env_loaded = load_jami_dbus_environment(options).ok();
    checks.push(json!({
        "name": "dbus_env_file",
        "ok": env_loaded.is_some(),
        "detail": env_loaded.unwrap_or_else(|| "No Jami DBus env file loaded".to_string()),
    }));
    checks.push(json!({
        "name": "dbus_session",
        "ok": with_jami_proxy(|_| Ok(())).is_ok(),
        "detail": if with_jami_proxy(|_| Ok(())).is_ok() {
            "Jami DBus session reachable"
        } else {
            "Jami DBus session not reachable"
        },
    }));
    let jami_runtime = jami_runtime_available();
    checks.push(json!({
        "name": "jami_runtime",
        "ok": jami_runtime,
        "detail": if jami_runtime { "Jami daemon/cli present" } else { "No Jami runtime detected" },
    }));
    checks.push(json!({
        "name": "configured_identity",
        "ok": !options.account_id.trim().is_empty() || !options.profile_name.trim().is_empty(),
        "detail": if !options.account_id.trim().is_empty() {
            options.account_id.clone()
        } else if !options.profile_name.trim().is_empty() {
            options.profile_name.clone()
        } else {
            "No configured Jami account id or profile".to_string()
        },
    }));
    checks
}

fn dbus_env_file_candidates(options: &JamiOptions) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if !options.dbus_env_file.trim().is_empty() {
        candidates.push(PathBuf::from(options.dbus_env_file.trim()));
    }
    if let Ok(xdg_runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        candidates.push(PathBuf::from(xdg_runtime_dir).join("cto-jami-dbus.env"));
    }
    candidates.push(PathBuf::from("/tmp/cto-jami-dbus.env"));
    let mut dedup = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| dedup.insert(candidate.clone()))
        .collect()
}

fn load_jami_dbus_environment(options: &mut JamiOptions) -> Result<String> {
    for candidate in dbus_env_file_candidates(options) {
        if !candidate.exists() {
            continue;
        }
        let raw = fs::read_to_string(&candidate)
            .with_context(|| format!("failed to read DBus env file {}", candidate.display()))?;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("export ") {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let mut value = value.trim().trim_end_matches(';').trim().to_string();
                value = value.trim_matches('"').trim_matches('\'').to_string();
                std::env::set_var(key.trim(), value);
            }
        }
        options.dbus_env_file = candidate.display().to_string();
        return Ok(options.dbus_env_file.clone());
    }
    bail!("No Jami DBus env file loaded")
}

fn command_exists(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|entry| entry.join(name).exists())
}

fn jami_runtime_available() -> bool {
    Path::new("/usr/libexec/jamid").is_file()
        || command_exists("jamid")
        || command_exists("jami-daemon")
        || command_exists("jami-cli")
}

fn with_jami_proxy<T>(f: impl FnOnce(&Proxy<'_>) -> Result<T>) -> Result<T> {
    let connection = Connection::session().context("failed to connect to session DBus")?;
    let proxy = Proxy::new(
        &connection,
        JAMI_DBUS_SERVICE,
        JAMI_DBUS_OBJECT_PATH,
        JAMI_DBUS_INTERFACE,
    )
    .context("failed to create Jami DBus proxy")?;
    f(&proxy)
}

fn string_map_to_json(map: BTreeMap<String, String>) -> Value {
    Value::Object(
        map.into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    )
}

fn string_records_to_values(items: Vec<BTreeMap<String, String>>) -> Vec<Value> {
    items.into_iter().map(string_map_to_json).collect()
}

fn get_account_list() -> Result<Vec<String>> {
    let (accounts,): (Vec<String>,) = with_jami_proxy(|proxy| {
        proxy
            .call("getAccountList", &())
            .context("DBus call getAccountList failed")
    })?;
    Ok(accounts
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect())
}

fn get_account_details(account_id: &str) -> Result<Value> {
    let details: BTreeMap<String, String> = with_jami_proxy(|proxy| {
        proxy
            .call("getAccountDetails", &(account_id))
            .context("DBus call getAccountDetails failed")
    })?;
    Ok(string_map_to_json(details))
}

fn get_volatile_account_details(account_id: &str) -> Result<Value> {
    let details: BTreeMap<String, String> = with_jami_proxy(|proxy| {
        proxy
            .call("getVolatileAccountDetails", &(account_id))
            .context("DBus call getVolatileAccountDetails failed")
    })?;
    Ok(string_map_to_json(details))
}

fn get_account_template(account_type: &str) -> Result<BTreeMap<String, String>> {
    let details: BTreeMap<String, String> = with_jami_proxy(|proxy| {
        proxy
            .call("getAccountTemplate", &(account_type))
            .context("DBus call getAccountTemplate failed")
    })?;
    Ok(details)
}

fn add_account(details: &BTreeMap<String, String>) -> Result<String> {
    let account_id: String = with_jami_proxy(|proxy| {
        proxy
            .call("addAccount", &(details))
            .context("DBus call addAccount failed")
    })?;
    let account_id = account_id.trim().to_string();
    if account_id.is_empty() {
        bail!("Jami account creation returned no account id.");
    }
    Ok(account_id)
}

fn get_conversations(account_id: &str) -> Result<Vec<String>> {
    let (conversations,): (Vec<String>,) = with_jami_proxy(|proxy| {
        proxy
            .call("getConversations", &(account_id))
            .context("DBus call getConversations failed")
    })?;
    Ok(conversations
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect())
}

fn get_conversation_requests(account_id: &str) -> Result<Vec<Value>> {
    let items: Vec<BTreeMap<String, String>> = with_jami_proxy(|proxy| {
        proxy
            .call("getConversationRequests", &(account_id))
            .context("DBus call getConversationRequests failed")
    })?;
    Ok(string_records_to_values(items))
}

fn get_trust_requests(account_id: &str) -> Result<Vec<Value>> {
    let items: Vec<BTreeMap<String, String>> = with_jami_proxy(|proxy| {
        proxy
            .call("getTrustRequests", &(account_id))
            .context("DBus call getTrustRequests failed")
    })?;
    Ok(string_records_to_values(items))
}

fn get_contacts(account_id: &str) -> Result<Vec<Value>> {
    let items: Vec<BTreeMap<String, String>> = with_jami_proxy(|proxy| {
        proxy
            .call("getContacts", &(account_id))
            .context("DBus call getContacts failed")
    })?;
    Ok(string_records_to_values(items))
}

fn add_contact(account_id: &str, uri: &str) -> Result<()> {
    with_jami_proxy(|proxy| {
        let _: () = proxy
            .call("addContact", &(account_id, uri))
            .context("DBus call addContact failed")?;
        Ok(())
    })
}

fn send_trust_request(account_id: &str, uri: &str, payload: &[u8]) -> Result<()> {
    with_jami_proxy(|proxy| {
        let _: () = proxy
            .call("sendTrustRequest", &(account_id, uri, payload.to_vec()))
            .context("DBus call sendTrustRequest failed")?;
        Ok(())
    })
}

fn start_conversation(account_id: &str) -> Result<String> {
    let conversation_id: String = with_jami_proxy(|proxy| {
        proxy
            .call("startConversation", &(account_id))
            .context("DBus call startConversation failed")
    })?;
    let conversation_id = conversation_id.trim().to_string();
    if conversation_id.is_empty() {
        bail!("Jami did not return a conversation id");
    }
    Ok(conversation_id)
}

fn add_conversation_member(
    account_id: &str,
    conversation_id: &str,
    contact_uri: &str,
) -> Result<()> {
    with_jami_proxy(|proxy| {
        let _: () = proxy
            .call(
                "addConversationMember",
                &(account_id, conversation_id, contact_uri),
            )
            .context("DBus call addConversationMember failed")?;
        Ok(())
    })
}

fn get_conversation_members(account_id: &str, conversation_id: &str) -> Result<Vec<Value>> {
    let items: Vec<BTreeMap<String, String>> = with_jami_proxy(|proxy| {
        proxy
            .call("getConversationMembers", &(account_id, conversation_id))
            .context("DBus call getConversationMembers failed")
    })?;
    Ok(string_records_to_values(items))
}

fn send_conversation_message(
    account_id: &str,
    conversation_id: &str,
    message: &str,
    reply_to: &str,
    flag: i32,
) -> Result<()> {
    with_jami_proxy(|proxy| {
        let _: () = proxy
            .call(
                "sendMessage",
                &(account_id, conversation_id, message, reply_to, flag),
            )
            .context("DBus call sendMessage failed")?;
        Ok(())
    })
}

fn send_file(
    account_id: &str,
    conversation_id: &str,
    file_path: &Path,
    display_name: &str,
    reply_to: &str,
) -> Result<()> {
    let file_path = file_path.display().to_string();
    with_jami_proxy(|proxy| {
        let _: () = proxy
            .call(
                "sendFile",
                &(
                    account_id,
                    conversation_id,
                    file_path.as_str(),
                    display_name,
                    reply_to,
                ),
            )
            .context("DBus call sendFile failed")?;
        Ok(())
    })
}

fn normalize_jami_uri_for_match(value: &str) -> String {
    let lowered = value.trim().to_lowercase();
    lowered
        .trim_start_matches("jami:")
        .trim_start_matches("ring:")
        .to_string()
}

fn conversation_id_from_thread_key(account_key: &str, thread_key: &str) -> Option<String> {
    let prefix = format!("{account_key}::");
    thread_key
        .strip_prefix(&prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !value.eq_ignore_ascii_case("outbox"))
        .map(str::to_string)
}

fn thread_key_for_conversation(account_key: &str, conversation_id: &str) -> String {
    format!("{account_key}::{conversation_id}")
}

fn contact_conversation_id(account_id: &str, remote_uri: &str) -> Result<Option<String>> {
    let expected = normalize_jami_uri_for_match(remote_uri);
    for contact in get_contacts(account_id)? {
        let Some(contact_object) = contact.as_object() else {
            continue;
        };
        let candidate = [
            contact_object.get("id").and_then(Value::as_str),
            contact_object.get("uri").and_then(Value::as_str),
        ]
        .into_iter()
        .flatten()
        .find(|value| normalize_jami_uri_for_match(value) == expected);
        if candidate.is_none() {
            continue;
        }
        let conversation_id = contact_object
            .get("conversationId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if conversation_id.is_some() {
            return Ok(conversation_id);
        }
    }
    Ok(None)
}

fn existing_conversation_for_remote(account_id: &str, remote_uri: &str) -> Result<Option<String>> {
    let expected = normalize_jami_uri_for_match(remote_uri);
    for conversation_id in get_conversations(account_id)? {
        let members = match get_conversation_members(account_id, &conversation_id) {
            Ok(members) => members,
            Err(_) => continue,
        };
        let matches_remote = members.iter().any(|member| {
            member
                .as_object()
                .and_then(|object| {
                    [
                        object.get("uri").and_then(Value::as_str),
                        object.get("memberUri").and_then(Value::as_str),
                        object.get("id").and_then(Value::as_str),
                    ]
                    .into_iter()
                    .flatten()
                    .find(|value| normalize_jami_uri_for_match(value) == expected)
                })
                .is_some()
        });
        if matches_remote {
            return Ok(Some(conversation_id));
        }
    }
    Ok(None)
}

fn ensure_conversation_for_delivery(
    options: &JamiOptions,
    account_key: &str,
    thread_key: &str,
    remote_uri: Option<&str>,
) -> Result<Option<String>> {
    if let Some(conversation_id) = conversation_id_from_thread_key(account_key, thread_key) {
        return Ok(Some(conversation_id));
    }
    let Some(remote_uri) = remote_uri.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    if let Some(conversation_id) = contact_conversation_id(&options.account_id, remote_uri)? {
        return Ok(Some(conversation_id));
    }
    if let Some(conversation_id) =
        existing_conversation_for_remote(&options.account_id, remote_uri)?
    {
        return Ok(Some(conversation_id));
    }
    let _ = add_contact(&options.account_id, remote_uri);
    let _ = send_trust_request(&options.account_id, remote_uri, &[]);
    for _ in 0..3 {
        if let Some(conversation_id) = contact_conversation_id(&options.account_id, remote_uri)? {
            return Ok(Some(conversation_id));
        }
        if let Some(conversation_id) =
            existing_conversation_for_remote(&options.account_id, remote_uri)?
        {
            return Ok(Some(conversation_id));
        }
        thread::sleep(Duration::from_millis(150));
    }
    let conversation_id = start_conversation(&options.account_id)?;
    add_conversation_member(&options.account_id, &conversation_id, remote_uri)?;
    Ok(Some(conversation_id))
}

fn submit_jami_delivery(
    options: &JamiOptions,
    request: &JamiSendCommandRequest<'_>,
    remote_id: &str,
) -> Result<JamiOutboundDelivery> {
    let account_key = account_key_from_jami(&options.account_id);
    let remote_uri = request
        .to
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string);
    let voice_attachment = if request.send_voice {
        Some(synthesize_voice_attachment(
            options,
            request.body,
            remote_id,
        )?)
    } else {
        None
    };
    let conversation_id = ensure_conversation_for_delivery(
        options,
        &account_key,
        request.thread_key,
        remote_uri.as_deref(),
    )?;
    if conversation_id.is_none() && voice_attachment.is_some() {
        bail!("Jami voice delivery requires a resolvable conversation");
    }
    let mut submitted_text = false;
    if !request.body.trim().is_empty() {
        if let Some(conversation_id) = conversation_id.as_deref() {
            send_conversation_message(&options.account_id, conversation_id, request.body, "", 0)?;
            submitted_text = true;
        } else {
            bail!("Jami send requires a target conversation or recipient");
        }
    }
    let mut submitted_file = false;
    if let (Some(conversation_id), Some(voice_path)) =
        (conversation_id.as_deref(), voice_attachment.as_ref())
    {
        let display_name = voice_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("voice.wav");
        send_file(
            &options.account_id,
            conversation_id,
            voice_path,
            display_name,
            "",
        )?;
        submitted_file = true;
    }
    Ok(JamiOutboundDelivery {
        remote_uri,
        conversation_id,
        voice_attachment,
        submitted_text,
        submitted_file,
    })
}

fn jami_account_snapshot(
    account_id: &str,
    preferred_profile_name: &str,
) -> Result<JamiResolvedAccount> {
    let details = get_account_details(account_id)?;
    let volatile_details = get_volatile_account_details(account_id)?;
    let detail_map = details.as_object().cloned().unwrap_or_default();
    let volatile_map = volatile_details.as_object().cloned().unwrap_or_default();
    let username = detail_map
        .get("Account.username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let share_uri = detail_map
        .get("RingNS.uri")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with("jami:") {
                value.to_string()
            } else {
                format!("jami:{value}")
            }
        })
        .unwrap_or_else(|| {
            if username.is_empty() {
                String::new()
            } else if username.starts_with("jami:") {
                username.clone()
            } else {
                format!("jami:{username}")
            }
        });
    let display_name = [
        detail_map
            .get("Account.displayName")
            .and_then(Value::as_str)
            .unwrap_or(""),
        detail_map
            .get("Account.alias")
            .and_then(Value::as_str)
            .unwrap_or(""),
        detail_map
            .get("Account.deviceName")
            .and_then(Value::as_str)
            .unwrap_or(""),
        preferred_profile_name,
        account_id,
    ]
    .into_iter()
    .find(|value| !value.trim().is_empty())
    .unwrap_or(account_id)
    .to_string();
    Ok(JamiResolvedAccount {
        account_id: account_id.to_string(),
        account_type: detail_map
            .get("Account.type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_uppercase(),
        enabled: detail_map
            .get("Account.enable")
            .and_then(Value::as_str)
            .map(|value| !value.eq_ignore_ascii_case("false"))
            .unwrap_or(true),
        registration_status: volatile_map
            .get("Account.registrationStatus")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        username,
        share_uri,
        display_name,
        details,
        volatile_details,
        provisioned: false,
    })
}

fn provision_ring_account(profile_name: &str) -> Result<JamiResolvedAccount> {
    let mut details = get_account_template("RING")?;
    let label = if profile_name.trim().is_empty() {
        "CTOX".to_string()
    } else {
        profile_name.trim().to_string()
    };
    details.insert("Account.type".to_string(), "RING".to_string());
    details.insert("Account.enable".to_string(), "true".to_string());
    details.insert("Account.deviceName".to_string(), label.clone());
    details.insert("Account.displayName".to_string(), label.clone());
    details.insert("Account.alias".to_string(), label.clone());
    let account_id = add_account(&details)?;
    for _ in 0..20 {
        let snapshot = jami_account_snapshot(&account_id, &label)?;
        if !snapshot.share_uri.is_empty() {
            return Ok(JamiResolvedAccount {
                provisioned: true,
                ..snapshot
            });
        }
        thread::sleep(Duration::from_millis(500));
    }
    Ok(JamiResolvedAccount {
        provisioned: true,
        ..jami_account_snapshot(&account_id, &label)?
    })
}

fn resolve_jami_account(
    options: &mut JamiOptions,
    provision_if_missing: bool,
) -> Result<JamiResolvedAccount> {
    let _ = load_jami_dbus_environment(options)?;
    let preferred_account_id = options.account_id.trim().trim_start_matches("jami:");
    let preferred_profile_name = options.profile_name.trim().to_string();
    let ring_accounts = get_account_list()?
        .into_iter()
        .filter_map(|account_id| jami_account_snapshot(&account_id, &preferred_profile_name).ok())
        .filter(|snapshot| snapshot.account_type == "RING" && snapshot.enabled)
        .collect::<Vec<_>>();
    let mut selected = if !preferred_account_id.is_empty() {
        ring_accounts
            .iter()
            .find(|snapshot| {
                snapshot.account_id == preferred_account_id
                    || snapshot.username == preferred_account_id
                    || snapshot
                        .share_uri
                        .trim_start_matches("jami:")
                        .eq_ignore_ascii_case(preferred_account_id)
            })
            .cloned()
    } else {
        None
    };
    if selected.is_none() {
        selected = ring_accounts
            .iter()
            .find(|snapshot| {
                snapshot.registration_status == "REGISTERED" && !snapshot.share_uri.is_empty()
            })
            .cloned()
            .or_else(|| {
                ring_accounts
                    .iter()
                    .find(|snapshot| !snapshot.share_uri.is_empty())
                    .cloned()
            })
            .or_else(|| ring_accounts.first().cloned());
    }
    if selected.is_none() && provision_if_missing {
        selected = Some(provision_ring_account(&preferred_profile_name)?);
    }
    let selected = selected.ok_or_else(|| anyhow!("Jami account could not be resolved"))?;
    options.account_id = selected.account_id.clone();
    options.username = selected.username.clone();
    options.account_uri = selected.share_uri.clone();
    options.device_name = selected
        .details
        .get("Account.deviceName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if options.profile_name.trim().is_empty() {
        options.profile_name = selected.display_name.clone();
    }
    Ok(selected)
}

fn accept_pending_jami_requests(account_id: &str) -> Result<()> {
    for request in get_trust_requests(account_id)? {
        let from = request
            .get("from")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if from.is_empty() {
            continue;
        }
        let _ = with_jami_proxy(|proxy| {
            let _: () = proxy
                .call("acceptTrustRequest", &(account_id, from.as_str()))
                .context("DBus call acceptTrustRequest failed")?;
            Ok(())
        });
    }
    for request in get_conversation_requests(account_id)? {
        let conversation_id = request
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| request.get("conversationId").and_then(Value::as_str))
            .unwrap_or("")
            .trim()
            .to_string();
        if conversation_id.is_empty() {
            continue;
        }
        let _ = with_jami_proxy(|proxy| {
            let _: () = proxy
                .call(
                    "acceptConversationRequest",
                    &(account_id, conversation_id.as_str()),
                )
                .context("DBus call acceptConversationRequest failed")?;
            Ok(())
        });
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct GitConversationEntry {
    remote_id: String,
    timestamp: String,
    author_device: String,
    subject: String,
}

#[derive(Clone, Debug)]
struct GitConversationLoad {
    conversation_id: String,
    repo_path: PathBuf,
    messages: Vec<GitConversationEntry>,
}

fn jami_state_root() -> PathBuf {
    if let Ok(value) = std::env::var("CTO_JAMI_STATE_DIR") {
        return PathBuf::from(value);
    }
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"))
        .join(".local/share/jami")
}

fn conversation_repo_root(account_id: &str, conversation_id: &str) -> PathBuf {
    jami_state_root()
        .join(account_id)
        .join("conversations")
        .join(conversation_id)
}

fn load_conversation_messages_from_git(
    options: &JamiOptions,
    conversation_ids: &[String],
) -> Result<Vec<GitConversationLoad>> {
    let repos = if conversation_ids.is_empty() {
        let root = jami_state_root()
            .join(&options.account_id)
            .join("conversations");
        if !root.exists() {
            Vec::new()
        } else {
            fs::read_dir(&root)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().join(".git").exists())
                .map(|entry| {
                    (
                        entry.file_name().to_string_lossy().to_string(),
                        entry.path(),
                    )
                })
                .collect::<Vec<_>>()
        }
    } else {
        conversation_ids
            .iter()
            .map(|conversation_id| {
                (
                    conversation_id.clone(),
                    conversation_repo_root(&options.account_id, conversation_id),
                )
            })
            .filter(|(_, repo_path)| repo_path.join(".git").exists())
            .collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    for (conversation_id, repo_path) in repos {
        let messages = match load_git_conversation_commits(&repo_path, options.limit.max(1)) {
            Ok(messages) => messages,
            Err(_) => {
                out.push(GitConversationLoad {
                    conversation_id,
                    repo_path,
                    messages: Vec::new(),
                });
                continue;
            }
        };
        out.push(GitConversationLoad {
            conversation_id,
            repo_path,
            messages,
        });
    }
    Ok(out)
}

fn load_git_conversation_commits(
    repo_path: &Path,
    limit: usize,
) -> Result<Vec<GitConversationEntry>> {
    let repo = Repository::open(repo_path).with_context(|| {
        format!(
            "failed to open Jami conversation repo {}",
            repo_path.display()
        )
    })?;
    let mut revwalk = repo.revwalk().context("failed to open revwalk")?;
    revwalk
        .set_sorting(Sort::TIME)
        .context("failed to sort revwalk")?;
    let mut pushed = false;
    if revwalk.push_head().is_ok() {
        pushed = true;
    }
    if !pushed {
        let references = repo.references().context("failed to enumerate refs")?;
        for reference in references.flatten() {
            if let Some(oid) = reference.target() {
                revwalk.push(oid).ok();
                pushed = true;
            }
        }
    }
    if !pushed {
        return Ok(Vec::new());
    }

    let mut newest_first = Vec::new();
    let mut seen = BTreeSet::new();
    for oid in revwalk.flatten() {
        let commit = match repo.find_commit(oid) {
            Ok(commit) => commit,
            Err(_) => continue,
        };
        let remote_id = commit.id().to_string();
        if !seen.insert(remote_id.clone()) {
            continue;
        }
        newest_first.push(GitConversationEntry {
            remote_id,
            timestamp: commit.time().seconds().to_string(),
            author_device: commit.author().name().unwrap_or("").trim().to_string(),
            subject: commit.summary().unwrap_or("").trim().to_string(),
        });
        if newest_first.len() >= limit {
            break;
        }
    }
    newest_first.reverse();
    let messages = newest_first;
    Ok(messages)
}

fn normalize_jami_git_commit(
    options: &JamiOptions,
    conversation_id: &str,
    repo_path: &Path,
    entry: &GitConversationEntry,
) -> Result<Option<JamiInboundMessage>> {
    if entry.remote_id.trim().is_empty() {
        return Ok(None);
    }
    let payload = serde_json::from_str::<Value>(&entry.subject).unwrap_or(Value::Null);
    let payload_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let author = payload
        .get("author")
        .and_then(Value::as_str)
        .or_else(|| payload.get("uri").and_then(Value::as_str))
        .or_else(|| payload.get("from").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
            repo_participant_hints(repo_path)
                .into_iter()
                .next()
                .unwrap_or_else(|| entry.author_device.clone())
        });
    if author.trim().is_empty() {
        return Ok(None);
    }
    // Skip messages authored by our own account to avoid self-echo.
    // The author field may be the account username hash, the device hash,
    // or the display name depending on how the commit was created.
    let author_device = entry.author_device.trim();
    let is_own_message = (!options.username.is_empty() && author == options.username)
        || (!options.profile_name.is_empty() && author.eq_ignore_ascii_case(&options.profile_name))
        || (!options.account_uri.is_empty() && author == options.account_uri)
        || (!options.device_name.is_empty() && author.eq_ignore_ascii_case(&options.device_name))
        || (!author_device.is_empty()
            && ((!options.username.is_empty() && author_device == options.username)
                || (!options.account_uri.is_empty() && author_device == options.account_uri)
                || (!options.device_name.is_empty()
                    && author_device.eq_ignore_ascii_case(&options.device_name))));
    if is_own_message {
        return Ok(None);
    }
    let has_voice_payload =
        payload_type.starts_with("audio/") || first_audio_attachment(&payload).is_some();
    let body_text = payload
        .get("body")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if has_voice_payload {
                "Voice message received.".to_string()
            } else {
                String::new()
            }
        });
    if body_text.is_empty() {
        return Ok(None);
    }
    Ok(Some(JamiInboundMessage {
        account_key: account_key_from_jami(&options.account_id),
        thread_key: format!(
            "{}::{}",
            account_key_from_jami(&options.account_id),
            conversation_id
        ),
        message_key: message_key_from_remote(
            &account_key_from_jami(&options.account_id),
            "INBOX",
            &entry.remote_id,
        ),
        remote_id: entry.remote_id.clone(),
        sender_display: author.clone(),
        sender_address: author,
        recipients: vec![options.account_id.clone()],
        subject: payload
            .get("subject")
            .and_then(Value::as_str)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "Jami {}",
                    conversation_id.chars().take(8).collect::<String>()
                )
            }),
        body_text: maybe_transcribe_audio(options, &payload, &body_text)?,
        preview: preview_text(&body_text, ""),
        seen: false,
        has_attachments: has_voice_payload,
        external_created_at: normalize_jami_timestamp(&entry.timestamp),
        metadata: json!({
            "adapter": "jami-git",
            "conversationId": conversation_id,
            "payloadType": payload_type,
            "authorDevice": entry.author_device,
            "rawEntry": sanitize_persistent_value(&json!({
                "remoteId": entry.remote_id,
                "timestamp": entry.timestamp,
                "authorDevice": entry.author_device,
            })),
            "rawPayload": sanitize_persistent_value(&payload),
        }),
    }))
}

fn repo_participant_hints(repo_path: &Path) -> Vec<String> {
    let admin_dir = repo_path.join("admins");
    if !admin_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(&admin_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("crt"))
                .then(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .map(|value| value.to_string())
                })
                .flatten()
        })
        .collect()
}

fn load_source_files(inbox_dir: &Path, limit: usize) -> Result<Vec<PathBuf>> {
    let mut files = fs::read_dir(inbox_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("json") | Some("jsonl")
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files.truncate(limit);
    Ok(files)
}

fn load_source_entries(path: &Path) -> Result<Vec<Value>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Jami source file {}", path.display()))?;
    if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
        Ok(raw
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<_>, _>>()?)
    } else {
        let parsed = serde_json::from_str::<Value>(&raw)?;
        Ok(parsed.as_array().cloned().unwrap_or_else(|| vec![parsed]))
    }
}

fn archive_source_file(path: &Path, archive_dir: &Path, entries: &[Value]) -> Result<()> {
    fs::create_dir_all(archive_dir)?;
    let destination = {
        let base = archive_dir.join(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("jami-source.json"),
        );
        if base.exists() {
            archive_dir.join(format!(
                "{}-{}{}",
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("source"),
                stable_digest(&now_iso_string()),
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(|value| format!(".{value}"))
                    .unwrap_or_default()
            ))
        } else {
            base
        }
    };
    let body = if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
        let mut lines = entries
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n");
        if !lines.is_empty() {
            lines.push('\n');
        }
        lines
    } else {
        format!("{}\n", serde_json::to_string_pretty(entries)?)
    };
    fs::write(&destination, body).with_context(|| {
        format!(
            "failed to archive Jami source file {}",
            destination.display()
        )
    })?;
    fs::remove_file(path)?;
    Ok(())
}

fn normalize_inbound_entry(
    options: &JamiOptions,
    entry: &Value,
    source_path: &Path,
    index: usize,
) -> JamiInboundMessage {
    let account_key = account_key_from_jami(&options.account_id);
    let sender_address = [
        entry.get("senderAddress").and_then(Value::as_str),
        entry.get("senderUri").and_then(Value::as_str),
        entry.get("fromAddress").and_then(Value::as_str),
        entry.get("fromUri").and_then(Value::as_str),
        entry.get("author").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .unwrap_or("")
    .to_string();
    let sender_display = [
        entry.get("senderDisplay").and_then(Value::as_str),
        entry.get("senderName").and_then(Value::as_str),
        entry.get("fromName").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .unwrap_or(if sender_address.is_empty() {
        "unknown sender"
    } else {
        &sender_address
    })
    .to_string();
    let remote_id = [
        entry.get("remoteId").and_then(Value::as_str),
        entry.get("id").and_then(Value::as_str),
        entry.get("messageId").and_then(Value::as_str),
        entry.get("uri").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .map(|value| value.to_string())
    .unwrap_or_else(|| {
        format!(
            "{}:{}",
            source_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("inbox"),
            index
        )
    });
    let subject = [
        entry.get("subject").and_then(Value::as_str),
        entry.get("conversationLabel").and_then(Value::as_str),
        entry.get("threadLabel").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .unwrap_or("(Jami)")
    .to_string();
    let body_text = [
        entry.get("bodyText").and_then(Value::as_str),
        entry.get("body").and_then(Value::as_str),
        entry.get("text").and_then(Value::as_str),
        entry.get("message").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .unwrap_or("")
    .to_string();
    let thread_key = [
        entry.get("threadKey").and_then(Value::as_str),
        entry.get("conversationId").and_then(Value::as_str),
        entry.get("conversationUri").and_then(Value::as_str),
        entry.get("threadId").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty())
    .map(|value| value.to_string())
    .unwrap_or_else(|| {
        format!(
            "{}::{}",
            account_key,
            sanitize_file_component(if sender_address.is_empty() {
                if sender_display.is_empty() {
                    &subject
                } else {
                    &sender_display
                }
            } else {
                &sender_address
            })
        )
    });
    JamiInboundMessage {
        account_key: account_key.clone(),
        thread_key,
        message_key: message_key_from_remote(&account_key, "INBOX", &remote_id),
        remote_id,
        sender_display,
        sender_address,
        recipients: extract_string_list(
            entry
                .get("recipientAddresses")
                .or_else(|| entry.get("to"))
                .or_else(|| entry.get("participants"))
                .or_else(|| entry.get("recipientUris")),
        ),
        subject: subject.clone(),
        body_text: maybe_transcribe_audio(options, entry, &body_text).unwrap_or(body_text.clone()),
        preview: preview_text(
            entry
                .get("preview")
                .and_then(Value::as_str)
                .unwrap_or(&body_text),
            &subject,
        ),
        seen: entry.get("seen").and_then(Value::as_bool).unwrap_or(false),
        has_attachments: entry
            .get("hasAttachments")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || entry
                .get("attachments")
                .and_then(Value::as_array)
                .map(|value| !value.is_empty())
                .unwrap_or(false),
        external_created_at: [
            entry.get("externalCreatedAt").and_then(Value::as_str),
            entry.get("createdAt").and_then(Value::as_str),
            entry.get("timestamp").and_then(Value::as_str),
            entry.get("sentAt").and_then(Value::as_str),
        ]
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .map(normalize_jami_timestamp)
        .unwrap_or_else(now_iso_string),
        metadata: json!({
            "sourceFile": source_path.file_name().and_then(|value| value.to_str()).unwrap_or(""),
            "conversationId": entry.get("conversationId").and_then(Value::as_str).unwrap_or(""),
            "threadLabel": entry.get("threadLabel").and_then(Value::as_str).unwrap_or(""),
            "rawEntry": sanitize_persistent_value(entry),
        }),
    }
}

fn store_inbound_message(
    conn: &mut rusqlite::Connection,
    options: &JamiOptions,
    inbound: &JamiInboundMessage,
    raw_payload: &Value,
) -> Result<bool> {
    if inbound_matches_existing_outbound(conn, inbound)? {
        return Ok(true);
    }
    let already_known: i64 = conn.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1",
        [inbound.message_key.as_str()],
        |row| row.get(0),
    )?;
    let observed_at = now_iso_string();
    fs::create_dir_all(&options.raw_dir)?;
    let raw_path = options.raw_dir.join(format!(
        "{}.json",
        sanitize_file_component(&inbound.remote_id)
    ));
    fs::write(
        &raw_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&sanitize_persistent_value(raw_payload))?
        ),
    )?;
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &inbound.message_key,
            channel: "jami",
            account_key: &inbound.account_key,
            thread_key: &inbound.thread_key,
            remote_id: &inbound.remote_id,
            direction: "inbound",
            folder_hint: "INBOX",
            sender_display: &inbound.sender_display,
            sender_address: &inbound.sender_address,
            recipient_addresses_json: &serde_json::to_string(&inbound.recipients)?,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &inbound.subject,
            preview: &inbound.preview,
            body_text: &inbound.body_text,
            body_html: "",
            raw_payload_ref: &raw_path.display().to_string(),
            trust_level: &options.trust_level,
            status: "received",
            seen: inbound.seen,
            has_attachments: inbound.has_attachments,
            external_created_at: &inbound.external_created_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&inbound.metadata)?,
        },
    )?;
    refresh_thread(conn, &inbound.thread_key)?;
    Ok(already_known > 0)
}

fn require_explicit_jami_identity(options: &JamiOptions) -> Result<()> {
    if !options.account_id.trim().is_empty() || !options.profile_name.trim().is_empty() {
        return Ok(());
    }
    bail!(
        "Jami sync requires CTO_JAMI_ACCOUNT_ID or CTO_JAMI_PROFILE_NAME; refusing implicit account resolution"
    )
}

fn inbound_matches_existing_outbound(
    conn: &rusqlite::Connection,
    inbound: &JamiInboundMessage,
) -> Result<bool> {
    if inbound.sender_address.trim().is_empty() || inbound.body_text.trim().is_empty() {
        return Ok(false);
    }
    let own_account = inbound
        .recipients
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    if own_account.is_empty() {
        return Ok(false);
    }

    let mut stmt = conn.prepare(
        r#"
        SELECT sender_address, recipient_addresses_json
        FROM communication_messages
        WHERE channel = 'jami'
          AND account_key = ?1
          AND thread_key = ?2
          AND direction = 'outbound'
          AND body_text = ?3
        ORDER BY external_created_at DESC, observed_at DESC
        LIMIT 25
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            inbound.account_key,
            inbound.thread_key,
            inbound.body_text.trim()
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    for row in rows {
        let (sender_address, recipients_json) = row?;
        if sender_address.trim() != own_account {
            continue;
        }
        let recipients = serde_json::from_str::<Vec<String>>(&recipients_json).unwrap_or_default();
        if recipients
            .iter()
            .any(|value| value.trim() == inbound.sender_address.trim())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn synthesize_voice_attachment(
    options: &JamiOptions,
    text: &str,
    remote_id: &str,
) -> Result<PathBuf> {
    let out_dir = options.outbox_dir.join("voice");
    fs::create_dir_all(&out_dir)?;
    let output_path = out_dir.join(format!("{}.wav", sanitize_file_component(remote_id)));
    let mut payload = json!({
        "input": text.trim(),
        "response_format": "wav",
    });
    if !options.speech_model.trim().is_empty() {
        payload["model"] = Value::String(options.speech_model.clone());
    }
    if !options.speech_voice.trim().is_empty() {
        payload["voice"] = Value::String(options.speech_voice.clone());
    }
    let input = payload
        .get("input")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let audio = super::communication_gateway::synthesize_speech(
        &options.root,
        input,
        &options.speech_model,
        &options.speech_voice,
        "wav",
    )?;
    fs::write(&output_path, audio)?;
    Ok(output_path)
}

fn maybe_transcribe_audio(
    options: &JamiOptions,
    value: &Value,
    existing_body: &str,
) -> Result<String> {
    let audio_path = first_audio_attachment(value);
    let Some(audio_path) = audio_path else {
        return Ok(existing_body.to_string());
    };
    let transcript = transcribe_audio_attachment(options, &audio_path).unwrap_or_default();
    if transcript.trim().is_empty() {
        return Ok(existing_body.to_string());
    }
    if existing_body.trim().is_empty() {
        return Ok(transcript);
    }
    if existing_body.trim() == transcript.trim() {
        return Ok(existing_body.to_string());
    }
    Ok(format!(
        "{}\n\n[Transcript]\n{}",
        existing_body.trim(),
        transcript
    ))
}

fn first_audio_attachment(value: &Value) -> Option<PathBuf> {
    match value {
        Value::Object(object) => {
            let path_candidate = [
                object.get("path").and_then(Value::as_str),
                object.get("filePath").and_then(Value::as_str),
                object.get("localPath").and_then(Value::as_str),
                object.get("audioPath").and_then(Value::as_str),
                object.get("voicePath").and_then(Value::as_str),
            ]
            .into_iter()
            .flatten()
            .find(|value| {
                let lowered = value.to_lowercase();
                lowered.ends_with(".wav")
                    || lowered.ends_with(".mp3")
                    || lowered.ends_with(".m4a")
                    || lowered.ends_with(".ogg")
                    || lowered.ends_with(".opus")
                    || lowered.ends_with(".flac")
                    || lowered.ends_with(".aac")
                    || lowered.ends_with(".webm")
            });
            if let Some(path) = path_candidate {
                let path = PathBuf::from(path);
                if path.exists() {
                    return Some(path);
                }
            }
            object.values().find_map(first_audio_attachment)
        }
        Value::Array(items) => items.iter().find_map(first_audio_attachment),
        _ => None,
    }
}

fn transcribe_audio_attachment(options: &JamiOptions, audio_path: &Path) -> Result<String> {
    super::communication_gateway::transcribe_audio_file(
        &options.root,
        audio_path,
        &options.transcription_model,
    )
}

fn sanitize_persistent_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut out = Map::new();
            for (key, entry) in object {
                let normalized = key.to_lowercase();
                if [
                    "path",
                    "filepath",
                    "localpath",
                    "tmppath",
                    "temppath",
                    "downloadpath",
                    "audiopath",
                    "voicepath",
                    "ref_audio",
                    "data",
                    "bytes",
                    "buffer",
                    "base64",
                ]
                .contains(&normalized.as_str())
                {
                    continue;
                }
                out.insert(key.clone(), sanitize_persistent_value(entry));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_persistent_value).collect()),
        other => other.clone(),
    }
}

fn extract_string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
        Some(Value::String(text)) => text
            .split(|ch| matches!(ch, ',' | '\n'))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_jami_timestamp(value: &str) -> String {
    let numeric = value.trim().parse::<f64>().unwrap_or_default();
    if numeric > 1e12 {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(numeric as i64)
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(now_iso_string)
    } else if numeric > 1e9 {
        chrono::DateTime::<chrono::Utc>::from_timestamp(numeric as i64, 0)
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(now_iso_string)
    } else if !value.trim().is_empty() {
        value.to_string()
    } else {
        now_iso_string()
    }
}

fn jami_account_json(account: &JamiResolvedAccount) -> Value {
    json!({
        "accountId": account.account_id,
        "accountType": account.account_type,
        "enabled": account.enabled,
        "registrationStatus": account.registration_status,
        "username": account.username,
        "shareUri": account.share_uri,
        "displayName": account.display_name,
        "details": account.details,
        "volatileDetails": account.volatile_details,
        "provisioned": account.provisioned,
    })
}

fn resolve_linux_session_bus_address() -> Result<String> {
    if let Some(address) = std::env::var("DBUS_SESSION_BUS_ADDRESS")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(address);
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow!("XDG_RUNTIME_DIR is not set; cannot derive a user DBus session bus")
        })?;
    let user_bus = runtime_dir.join("bus");
    if !user_bus.exists() {
        bail!(
            "no user DBus session bus found at {}; start the user bus before running the Jami daemon",
            user_bus.display()
        );
    }
    Ok(format!("unix:path={}", user_bus.display()))
}

fn write_jami_dbus_env_file(dbus_address: &str) -> Result<String> {
    let path = if let Some(path) = std::env::var("CTO_JAMI_DBUS_ENV_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        PathBuf::from(path)
    } else if let Some(runtime_dir) = std::env::var("XDG_RUNTIME_DIR").ok().map(PathBuf::from) {
        runtime_dir.join("cto-jami-dbus.env")
    } else {
        PathBuf::from("/tmp/cto-jami-dbus.env")
    };
    let body = format!(
        "DBUS_SESSION_BUS_ADDRESS={}\nCTO_JAMI_DBUS_ENV_FILE={}\n",
        dbus_address,
        path.display()
    );
    fs::write(&path, body)
        .with_context(|| format!("failed to write Jami DBus env file {}", path.display()))?;
    Ok(path.display().to_string())
}

fn find_jami_daemon_binary() -> Result<PathBuf> {
    let configured = std::env::var("CTO_JAMI_DAEMON_BIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(configured) = configured {
        let path = PathBuf::from(&configured);
        if path.is_file() {
            return Ok(path);
        }
        if let Some(found) = find_executable_on_path(&configured) {
            return Ok(found);
        }
        bail!("configured CTO_JAMI_DAEMON_BIN was not found: {configured}");
    }
    for candidate in ["/usr/libexec/jamid", "jamid", "jami-daemon"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
        if let Some(found) = find_executable_on_path(candidate) {
            return Ok(found);
        }
    }
    bail!("no Jami daemon binary found; install jami-daemon or set CTO_JAMI_DAEMON_BIN")
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|entry| entry.join(name))
        .find(|candidate| candidate.is_file())
}

fn jami_daemon_args() -> Vec<String> {
    std::env::var("CTO_JAMI_DAEMON_ARGS")
        .ok()
        .map(|value| {
            value
                .split_whitespace()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| vec!["-p".to_string()])
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

#[cfg(test)]
mod tests {
    use super::inbound_matches_existing_outbound;
    use super::require_explicit_jami_identity;
    use super::store_inbound_message;
    use super::conversation_id_from_thread_key;
    use super::JamiInboundMessage;
    use super::JamiOptions;
    use super::normalize_jami_uri_for_match;
    use super::thread_key_for_conversation;
    use crate::mission::channels::{open_channel_db, upsert_communication_message, UpsertMessage};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn thread_key_round_trip_preserves_conversation_id() {
        let account_key = "jami:owner";
        let conversation_id = "abcd1234";
        let thread_key = thread_key_for_conversation(account_key, conversation_id);
        assert_eq!(
            conversation_id_from_thread_key(account_key, &thread_key).as_deref(),
            Some(conversation_id)
        );
    }

    #[test]
    fn thread_key_parser_rejects_outbox_marker() {
        assert_eq!(
            conversation_id_from_thread_key("jami:owner", "jami:owner::outbox"),
            None
        );
    }

    #[test]
    fn jami_uri_normalization_ignores_prefix_and_case() {
        assert_eq!(
            normalize_jami_uri_for_match("JAMI:AbCd"),
            normalize_jami_uri_for_match("abcd")
        );
        assert_eq!(
            normalize_jami_uri_for_match("ring:AbCd"),
            normalize_jami_uri_for_match("jami:abcd")
        );
    }

    #[test]
    fn jami_sync_requires_explicit_identity_configuration() {
        let options = JamiOptions {
            root: PathBuf::from("/tmp/ctox"),
            db_path: PathBuf::from("/tmp/ctox/runtime/ctox.sqlite3"),
            raw_dir: PathBuf::from("/tmp/ctox/runtime/raw"),
            inbox_dir: PathBuf::from("/tmp/ctox/runtime/inbox"),
            outbox_dir: PathBuf::from("/tmp/ctox/runtime/outbox"),
            archive_dir: PathBuf::from("/tmp/ctox/runtime/archive"),
            account_id: String::new(),
            username: String::new(),
            profile_name: String::new(),
            account_uri: String::new(),
            device_name: String::new(),
            provider: "native".to_string(),
            limit: 20,
            trust_level: "owner".to_string(),
            dbus_env_file: String::new(),
            transcription_model: String::new(),
            speech_model: String::new(),
            speech_voice: String::new(),
        };
        let err = require_explicit_jami_identity(&options).unwrap_err().to_string();
        assert!(err.contains("CTO_JAMI_ACCOUNT_ID"));
    }

    #[test]
    fn jami_sync_skips_outbound_self_echo_duplicates() {
        let temp_root = std::env::temp_dir().join(format!(
            "ctox-jami-self-echo-test-{}",
            std::process::id()
        ));
        let db_path = temp_root.join("runtime/ctox.sqlite3");
        let raw_dir = temp_root.join("runtime/communication/jami/raw");
        let mut conn = open_channel_db(&db_path).expect("open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "jami:cae::OUTBOUND::queued-1",
                channel: "jami",
                account_key: "jami:cae",
                thread_key: "jami:cae::conv-1",
                remote_id: "queued-1",
                direction: "outbound",
                folder_hint: "OUTBOX",
                sender_display: "CTO1",
                sender_address: "cae53c1469355a5d",
                recipient_addresses_json: "[\"e617d2f0a05095b8289415f92f8e7bbeab64dedf\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Jami conv-1",
                preview: "Verstanden. Sobald die Vercel-Freigabe sichtbar ist...",
                body_text: "Verstanden. Sobald die Vercel-Freigabe sichtbar ist...",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "owner",
                status: "submitted",
                seen: true,
                has_attachments: false,
                external_created_at: "2026-04-24T10:00:00Z",
                observed_at: "2026-04-24T10:00:00Z",
                metadata_json: "{}",
            },
        )
        .expect("seed outbound");

        let inbound = JamiInboundMessage {
            account_key: "jami:cae".to_string(),
            thread_key: "jami:cae::conv-1".to_string(),
            message_key: "jami:cae::INBOX::commit-1".to_string(),
            remote_id: "commit-1".to_string(),
            sender_display: "e617d2f0a05095b8289415f92f8e7bbeab64dedf".to_string(),
            sender_address: "e617d2f0a05095b8289415f92f8e7bbeab64dedf".to_string(),
            recipients: vec!["cae53c1469355a5d".to_string()],
            subject: "Jami conv-1".to_string(),
            body_text: "Verstanden. Sobald die Vercel-Freigabe sichtbar ist...".to_string(),
            preview: "Verstanden. Sobald die Vercel-Freigabe sichtbar ist...".to_string(),
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-24T10:01:00Z".to_string(),
            metadata: json!({}),
        };

        assert!(
            inbound_matches_existing_outbound(&conn, &inbound).expect("check echo"),
            "exact jami outbound/inbound duplicates should be treated as self-echo"
        );

        let options = JamiOptions {
            root: temp_root.clone(),
            db_path,
            raw_dir,
            inbox_dir: temp_root.join("runtime/communication/jami/inbox"),
            outbox_dir: temp_root.join("runtime/communication/jami/outbox"),
            archive_dir: temp_root.join("runtime/communication/jami/archive"),
            account_id: "cae53c1469355a5d".to_string(),
            username: String::new(),
            profile_name: "CTO1".to_string(),
            account_uri: "jami:cae53c1469355a5d".to_string(),
            device_name: "CTO1".to_string(),
            provider: "native".to_string(),
            limit: 20,
            trust_level: "owner".to_string(),
            dbus_env_file: String::new(),
            transcription_model: String::new(),
            speech_model: String::new(),
            speech_voice: String::new(),
        };
        let known = store_inbound_message(&mut conn, &options, &inbound, &json!({}))
            .expect("store inbound");
        assert!(known, "self-echo duplicates should not be persisted as fresh inbound");
        let inbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE direction = 'inbound'",
                [],
                |row| row.get(0),
            )
            .expect("count inbound");
        assert_eq!(inbox_count, 0);
    }
}
