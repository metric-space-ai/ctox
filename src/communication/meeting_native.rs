// Origin: CTOX
// License: Apache-2.0
//
// Meeting bot adapter — joins video meetings (Google Meet, Microsoft Teams, Zoom)
// as a silent participant via Playwright, captures audio for transcription, monitors
// the meeting chat, and responds when @CTOX is mentioned.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::communication::adapters::{AdapterSyncCommandRequest, MeetingSendCommandRequest};
use crate::communication::runtime as communication_runtime;
use crate::inference::{engine, native_stt, runtime_env, supervisor};
use crate::mission::channels::{
    ensure_routing_rows_for_inbound, open_channel_db, refresh_thread, upsert_communication_message,
    UpsertMessage,
};

const DEFAULT_MEETING_STT_MODEL: &str = "engineai/Voxtral-Mini-4B-Realtime-2602";
const MEETING_XVFB_SERVER_ARGS: &str = "-screen 0 1920x1080x24 -ac +extension RANDR";

// ---------------------------------------------------------------------------
// Public adapter interface (sync / send / service_sync)
// ---------------------------------------------------------------------------

/// Sync active meeting sessions — ingest new chat messages into the SQLite
/// communication_messages table, exactly like email/jami sync does.
/// Each chat message becomes a row with channel="meeting",
/// thread_key=session_id, direction="inbound".
pub(crate) fn sync(
    root: &Path,
    _runtime: &BTreeMap<String, String>,
    request: &AdapterSyncCommandRequest<'_>,
) -> Result<Value> {
    let session_dirs = existing_meeting_session_dirs(root);
    if session_dirs.is_empty() {
        return Ok(json!({"ok": true, "active_sessions": 0, "ingested": 0}));
    }
    let db_path = request.db_path;
    let mut conn = open_channel_db(db_path)?;
    let mut active = 0u64;
    let mut ingested = 0u64;
    let account_key = "meeting:system";

    for sessions_dir in session_dirs {
        for entry in fs::read_dir(&sessions_dir).unwrap_or_else(|_| fs::read_dir(".").unwrap()) {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut session) = serde_json::from_str::<Value>(&contents) else {
                continue;
            };
            let status = session
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if status != "active" && status != "joining" && status != "running" {
                continue;
            }
            active += 1;

            let session_id = session
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let provider = session
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            // Read chat messages from the session JSON and ingest any not yet in SQLite
            let chat_messages = session
                .get("chat_messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            for msg in &chat_messages {
                let sender = msg
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown");
                let text = msg.get("text").and_then(Value::as_str).unwrap_or("");
                let timestamp = msg.get("timestamp").and_then(Value::as_str).unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                if session_value_is_own_message(&session, sender, text) {
                    continue;
                }

                // Stable message_key prevents re-ingesting the same chat line
                let message_key = format!(
                    "meeting::{}::{}",
                    session_id,
                    stable_digest(&format!("{sender}:{text}:{timestamp}"))
                );

                let observed_at = if timestamp.is_empty() {
                    now_iso_string()
                } else {
                    timestamp.to_string()
                };

                let is_mention = MeetingSession::is_mention(text);
                if is_mention
                    && !session
                        .get("mention_ack_sent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                {
                    let ack_text = first_mention_ack_text();
                    let _ = write_chat_command_to_session(&session, ack_text);
                    record_meeting_outbound_message(
                        &mut conn,
                        &session_id,
                        &provider,
                        ack_text,
                        "ctox_first_mention_ack",
                    )?;
                    if let Some(object) = session.as_object_mut() {
                        object.insert("mention_ack_sent".to_string(), Value::Bool(true));
                        object.insert(
                            "mention_ack_sent_at".to_string(),
                            Value::String(now_iso_string()),
                        );
                    }
                    let _ = fs::write(&path, serde_json::to_string_pretty(&session)?);
                }
                let transcript_snapshot = session_transcript_snapshot(&session, 12);
                let chat_snapshot = session_chat_snapshot(&session, 20);
                let body_text = if is_mention {
                    render_meeting_mention_inbound_body(
                        &session_id,
                        &provider,
                        sender,
                        text,
                        timestamp,
                        &transcript_snapshot,
                        &chat_snapshot,
                    )
                } else {
                    text.to_string()
                };
                let preview = clip_chars(&body_text, 120);
                let metadata = json!({
                    "provider": &provider,
                    "session_id": &session_id,
                    "source": "meeting_chat",
                    "is_mention": is_mention,
                    "skill": if is_mention { "meeting-participant" } else { "" },
                    "priority": if is_mention { "urgent" } else { "normal" },
                    "transcript_chunk_count": session
                        .get("transcript_chunk_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_else(|| session
                            .get("transcript_chunks")
                            .and_then(Value::as_array)
                            .map(|items| items.len() as u64)
                            .unwrap_or(0)),
                    "chat_message_count": session
                        .get("chat_message_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_else(|| chat_messages.len() as u64),
                    "transcript_snapshot": transcript_snapshot,
                    "chat_snapshot": chat_snapshot,
                });

                upsert_communication_message(
                    &mut conn,
                    UpsertMessage {
                        message_key: &message_key,
                        channel: "meeting",
                        account_key,
                        thread_key: &session_id,
                        remote_id: &message_key,
                        direction: "inbound",
                        folder_hint: "chat",
                        sender_display: sender,
                        sender_address: sender,
                        recipient_addresses_json: "[]",
                        cc_addresses_json: "[]",
                        bcc_addresses_json: "[]",
                        subject: &format!("{} meeting chat", provider),
                        preview: &preview,
                        body_text: &body_text,
                        body_html: "",
                        raw_payload_ref: "",
                        trust_level: "internal",
                        status: "received",
                        seen: false,
                        has_attachments: false,
                        external_created_at: &observed_at,
                        observed_at: &observed_at,
                        metadata_json: &serde_json::to_string(&metadata)?,
                    },
                )?;
                ingested += 1;
            }

            if !chat_messages.is_empty() {
                let _ = refresh_thread(&mut conn, &session_id);
            }
        }
    }

    if ingested > 0 {
        ensure_routing_rows_for_inbound(&conn)?;
    }

    Ok(json!({"ok": true, "active_sessions": active, "ingested": ingested}))
}

/// Send a chat message to a running meeting session.
/// 1. Store the outbound message in SQLite (same pipeline as email/jami)
/// 2. Forward to the Playwright process via stdin pipe
pub(crate) fn send(
    root: &Path,
    _runtime: &BTreeMap<String, String>,
    request: &MeetingSendCommandRequest<'_>,
) -> Result<Value> {
    let session_path = meeting_session_file(root, request.session_id);
    if !session_path.exists() {
        bail!(
            "meeting session {} not found at {}",
            request.session_id,
            session_path.display()
        );
    }
    let contents = fs::read_to_string(&session_path)?;
    let session: Value = serde_json::from_str(&contents)?;
    let provider = session
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    // 1. Store outbound message in SQLite — same as email send does
    let db_path = request.db_path;
    let mut conn = open_channel_db(db_path)?;
    let observed_at = now_iso_string();
    let message_key = format!(
        "meeting::{}::out::{}",
        request.session_id,
        stable_digest(&format!("{}:{}", request.body, observed_at))
    );
    let metadata = json!({
        "provider": provider,
        "session_id": request.session_id,
        "source": "ctox_reply",
    });
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "meeting",
            account_key: "meeting:system",
            thread_key: request.session_id,
            remote_id: &message_key,
            direction: "outbound",
            folder_hint: "sent",
            sender_display: "INF Yoda Notetaker",
            sender_address: "ctox@local",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &format!("{} meeting chat reply", provider),
            preview: &request.body[..request.body.len().min(120)],
            body_text: request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "internal",
            status: "sent",
            seen: true,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    let _ = refresh_thread(&mut conn, request.session_id);

    // 2. Forward to Playwright process via stdin pipe
    let _ = write_chat_command_to_session(&session, request.body);

    Ok(
        json!({"ok": true, "status": "sent", "session_id": request.session_id, "message_key": message_key}),
    )
}

fn first_mention_ack_text() -> &'static str {
    "Ich habe die Frage gesehen und antworte hier im Chat. Das kann einen Augenblick dauern, weil mir Echtzeit-Antworten leider noch nicht zuverlaessig moeglich sind."
}

fn write_chat_command_to_session(session: &Value, text: &str) -> Result<()> {
    let Some(stdin_path) = session.get("stdin_pipe").and_then(Value::as_str) else {
        return Ok(());
    };
    let command = json!({"action": "send_chat", "text": text});
    match fs::OpenOptions::new().append(true).open(stdin_path) {
        Ok(mut file) => {
            let _ = writeln!(file, "{}", command);
        }
        Err(err) => {
            eprintln!("[meeting] warning: could not write to stdin pipe: {err}");
        }
    }
    Ok(())
}

fn recent_direct_speaker(signal: Option<&SpeakerSignal>) -> Option<&SpeakerSignal> {
    let signal = signal?;
    let speaker = signal.speaker_display.trim();
    if speaker.is_empty() || speaker.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let ts = DateTime::parse_from_rfc3339(&signal.timestamp).ok()?;
    let age = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
    if age <= Duration::seconds(45) {
        Some(signal)
    } else {
        None
    }
}

fn session_value_is_own_message(session: &Value, sender: &str, text: &str) -> bool {
    let bot_name = session
        .get("bot_name")
        .and_then(Value::as_str)
        .unwrap_or("INF Yoda Notetaker");
    let outbound = session
        .get("outbound_chat_texts")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    is_own_message_text(bot_name, &outbound, sender, text)
}

fn reconcile_stale_running_session(path: &Path, mut session: Value) -> Value {
    let is_running = session
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "running" || status == "joining" || status == "active");
    if !is_running {
        return session;
    }
    let pid = session
        .get("pid")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if pid != 0 && process_is_running(pid) {
        return session;
    }
    if let Some(object) = session.as_object_mut() {
        object.insert("status".to_string(), Value::String("ended".to_string()));
        object.insert("ended_at".to_string(), Value::String(now_iso_string()));
        object.insert(
            "end_reason".to_string(),
            Value::String("process_not_running".to_string()),
        );
    }
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&session).unwrap_or_default(),
    );
    session
}

fn process_is_running(pid: u64) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn record_meeting_outbound_message(
    conn: &mut rusqlite::Connection,
    session_id: &str,
    provider: &str,
    body: &str,
    source: &str,
) -> Result<()> {
    let observed_at = now_iso_string();
    let message_key = format!(
        "meeting::{}::out::{}",
        session_id,
        stable_digest(&format!("{source}:{body}:{observed_at}"))
    );
    let metadata = json!({
        "provider": provider,
        "session_id": session_id,
        "source": source,
    });
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "meeting",
            account_key: "meeting:system",
            thread_key: session_id,
            remote_id: &message_key,
            direction: "outbound",
            folder_hint: "sent",
            sender_display: "INF Yoda Notetaker",
            sender_address: "ctox@local",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &format!("{} meeting chat reply", provider),
            preview: &body[..body.len().min(120)],
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "internal",
            status: "sent",
            seen: true,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(conn, session_id)?;
    Ok(())
}

fn render_meeting_mention_inbound_body(
    session_id: &str,
    provider: &str,
    sender: &str,
    text: &str,
    timestamp: &str,
    transcript_snapshot: &str,
    chat_snapshot: &str,
) -> String {
    let timestamp = if timestamp.trim().is_empty() {
        "(unknown)"
    } else {
        timestamp
    };
    let transcript = if transcript_snapshot.trim().is_empty() {
        "(Noch kein Live-Transcript verfuegbar. Falls STT offline ist, antworte nur auf Basis von Chat und explizit bekannten Kontext.)"
    } else {
        transcript_snapshot
    };
    let chat = if chat_snapshot.trim().is_empty() {
        "(keine vorherigen Chatnachrichten)"
    } else {
        chat_snapshot
    };
    format!(
        "@CTOX Meeting-Chat-Erwaehnung\n\
         Provider: {provider}\n\
         Session: {session_id}\n\
         Sender: {sender}\n\
         Timestamp: {timestamp}\n\
         Nachricht: {text}\n\n\
         Live-Transcript bisher (neueste Chunks):\n{transcript}\n\n\
         Meeting-Chat bisher:\n{chat}\n\n\
         Antworte kurz im Meeting-Chat. Wenn das Transcript fuer die Frage nicht ausreicht, sage das knapp und frage nach der fehlenden Information."
    )
}

fn session_transcript_snapshot(session: &Value, max_chunks: usize) -> String {
    if let Some(snapshot) = session_transcript_segment_snapshot(session, max_chunks) {
        return snapshot;
    }
    let chunks = session
        .get("transcript_chunks")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .map(str::trim)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let start = chunks.len().saturating_sub(max_chunks);
    chunks[start..].join("\n")
}

fn session_transcript_segment_snapshot(session: &Value, max_segments: usize) -> Option<String> {
    let segments = session
        .get("transcript_segments")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(render_transcript_segment_value)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    let start = segments.len().saturating_sub(max_segments);
    Some(segments[start..].join("\n"))
}

fn render_transcript_segment_value(segment: &Value) -> Option<String> {
    let text = segment.get("text").and_then(Value::as_str)?.trim();
    if text.is_empty() {
        return None;
    }
    let speaker = segment
        .get("speaker_display")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let timestamp = segment
        .get("timestamp")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let source = segment
        .get("source")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("stt");
    let confidence = segment
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    Some(format!(
        "[{timestamp}] {speaker}: {text} [source={source} confidence={confidence:.2}]"
    ))
}

fn session_chat_snapshot(session: &Value, max_messages: usize) -> String {
    let messages = session
        .get("chat_messages")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|message| {
                    let sender = message
                        .get("sender")
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown");
                    let text = message.get("text").and_then(Value::as_str).unwrap_or("");
                    if text.trim().is_empty() {
                        return None;
                    }
                    let timestamp = message
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    Some(if timestamp.trim().is_empty() {
                        format!("{sender}: {text}")
                    } else {
                        format!("[{timestamp}] {sender}: {text}")
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let start = messages.len().saturating_sub(max_messages);
    messages[start..].join("\n")
}

fn clip_chars(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

struct MeetingSttRuntimeGuard {
    root: PathBuf,
    started_by_meeting: bool,
    restore_values: BTreeMap<String, Option<String>>,
    start_error: Option<String>,
    finished: bool,
}

impl MeetingSttRuntimeGuard {
    fn ensure_for_meeting(root: &Path) -> Self {
        let mut guard = Self {
            root: root.to_path_buf(),
            started_by_meeting: false,
            restore_values: BTreeMap::new(),
            start_error: None,
            finished: false,
        };
        if check_engine_reachable(root) {
            return guard;
        }
        if let Err(err) = guard.prepare_runtime_config() {
            guard.start_error = Some(err.to_string());
            return guard;
        }
        match supervisor::ensure_auxiliary_backend_ready(root, engine::AuxiliaryRole::Stt, false) {
            Ok(()) => {
                if check_engine_reachable(root) {
                    guard.started_by_meeting = true;
                    eprintln!("[meeting] STT runtime auto-started for this meeting");
                } else {
                    guard.start_error = Some(
                        "STT backend launch completed but the transcription transport is still unavailable"
                            .to_string(),
                    );
                }
            }
            Err(err) => {
                guard.start_error = Some(err.to_string());
            }
        }
        guard
    }

    fn prepare_runtime_config(&mut self) -> Result<()> {
        let mut env_map = runtime_env::load_runtime_env_map(&self.root).unwrap_or_default();
        self.set_runtime_key(
            &mut env_map,
            "CTOX_ENABLE_STT_BACKEND",
            Some("1".to_string()),
        );
        let current_model = env_map
            .get("CTOX_STT_MODEL")
            .map(String::as_str)
            .unwrap_or("");
        if normalize_meeting_stt_model(Some(current_model)) != current_model.trim() {
            self.set_runtime_key(
                &mut env_map,
                "CTOX_STT_MODEL",
                Some(DEFAULT_MEETING_STT_MODEL.to_string()),
            );
        }
        runtime_env::save_runtime_env_map(&self.root, &env_map)
    }

    fn set_runtime_key(
        &mut self,
        env_map: &mut BTreeMap<String, String>,
        key: &'static str,
        value: Option<String>,
    ) {
        self.restore_values
            .entry(key.to_string())
            .or_insert_with(|| env_map.get(key).cloned());
        match value {
            Some(value) => {
                env_map.insert(key.to_string(), value);
            }
            None => {
                env_map.remove(key);
            }
        }
    }

    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.started_by_meeting {
            if let Err(err) =
                supervisor::release_auxiliary_backend(&self.root, engine::AuxiliaryRole::Stt)
            {
                eprintln!("[meeting] warning: failed to stop meeting STT runtime: {err}");
            } else {
                eprintln!("[meeting] STT runtime stopped after meeting");
            }
        }
        if !self.restore_values.is_empty() {
            if let Err(err) = self.restore_runtime_config() {
                eprintln!("[meeting] warning: failed to restore STT runtime config: {err}");
            }
        }
    }

    fn restore_runtime_config(&self) -> Result<()> {
        let mut env_map = runtime_env::load_runtime_env_map(&self.root).unwrap_or_default();
        for (key, value) in &self.restore_values {
            match value {
                Some(value) => {
                    env_map.insert(key.clone(), value.clone());
                }
                None => {
                    env_map.remove(key);
                }
            }
        }
        runtime_env::save_runtime_env_map(&self.root, &env_map)
    }
}

impl Drop for MeetingSttRuntimeGuard {
    fn drop(&mut self) {
        self.finish();
    }
}

fn is_disabled_selector(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "none" | "disabled"
    )
}

/// Service sync — delegates to sync() with proper db_path.
pub(crate) fn service_sync(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    let db_path = root.join("runtime/ctox.sqlite3");
    let request = AdapterSyncCommandRequest {
        db_path: &db_path,
        passthrough_args: &[],
        skip_flags: &[],
    };
    Ok(Some(sync(root, settings, &request)?))
}

// ---------------------------------------------------------------------------
// CLI command handler
// ---------------------------------------------------------------------------

pub fn handle_meeting_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "join" => {
            let url = args
                .get(1)
                .context("usage: ctox meeting join <url> [--name <bot-name>]")?;
            let bot_name = find_flag_value(args, "--name").unwrap_or("INF Yoda Notetaker");
            let runtime = crate::communication::gateway::runtime_settings_from_root(
                root,
                crate::communication::gateway::CommunicationAdapterKind::Meeting,
            );
            let mut config = MeetingSessionConfig::from_runtime(root, url, &runtime)?;
            if bot_name != "INF Yoda Notetaker" {
                config.bot_name = bot_name.to_string();
            }
            let result = run_meeting_session(root, &config)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        "schedule" => {
            let url = args
                .get(1)
                .context("usage: ctox meeting schedule <url> --time <ISO-8601>")?;
            let time = find_flag_value(args, "--time").context("--time <ISO-8601> is required")?;
            let bot_name = find_flag_value(args, "--name").unwrap_or("INF Yoda Notetaker");
            let result = schedule_meeting_join(root, url, time, bot_name)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        "cancel" => {
            let url = args.get(1).context("usage: ctox meeting cancel <url>")?;
            let result = cancel_meeting_join(root, url)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        "dump-script" => {
            let url = args
                .get(1)
                .context("usage: ctox meeting dump-script <url>")?;
            let runtime = crate::communication::gateway::runtime_settings_from_root(
                root,
                crate::communication::gateway::CommunicationAdapterKind::Meeting,
            );
            let config = MeetingSessionConfig::from_runtime(root, url, &runtime)?;
            let script = build_meeting_runner_script(&config)?;
            print!("{script}");
            Ok(())
        }
        "status" => {
            let mut sessions = Vec::new();
            for sessions_dir in existing_meeting_session_dirs(root) {
                for entry in fs::read_dir(&sessions_dir)? {
                    let entry = entry?;
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(contents) = fs::read_to_string(entry.path()) {
                            if let Ok(session) = serde_json::from_str::<Value>(&contents) {
                                let session =
                                    reconcile_stale_running_session(&entry.path(), session);
                                sessions.push(session);
                            }
                        }
                    }
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "sessions": sessions,
                }))?
            );
            Ok(())
        }
        "transcript" => {
            let session_id = args
                .get(1)
                .context("usage: ctox meeting transcript <session_id>")?;
            let result = load_meeting_transcript(root, session_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        "simulate" => {
            let result = simulate_meeting_session(root, &args[1..])?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        _ => {
            println!(
                "usage: ctox meeting <join|schedule|cancel|status|transcript|simulate> [args]"
            );
            println!();
            println!("  join <url> [--name <bot-name>]       Join a meeting now");
            println!("  schedule <url> --time <ISO-8601>     Schedule a future join");
            println!("  cancel <url>                         Cancel a scheduled join");
            println!("  status                               Show active/scheduled sessions");
            println!("  transcript <session_id>              Print transcript + chatlog as JSON");
            println!(
                "  simulate [--audio <wav>]... [--transcript <text>]... [--chat <sender:text>]..."
            );
            Ok(())
        }
    }
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn find_flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.iter()
        .enumerate()
        .filter_map(|(idx, value)| {
            if value == flag {
                args.get(idx + 1).map(String::as_str)
            } else {
                None
            }
        })
        .collect()
}

fn simulate_meeting_session(root: &Path, args: &[String]) -> Result<Value> {
    let provider = match find_flag_value(args, "--provider")
        .unwrap_or("google")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "google" | "meet" | "google-meet" => MeetingProvider::GoogleMeet,
        "microsoft" | "teams" | "microsoft-teams" => MeetingProvider::MicrosoftTeams,
        "zoom" => MeetingProvider::Zoom,
        other => bail!("unsupported --provider `{other}`; expected google, teams, or zoom"),
    };
    let meeting_url = find_flag_value(args, "--url")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| match provider {
            MeetingProvider::GoogleMeet => "https://meet.google.com/demo-meet-test".to_string(),
            MeetingProvider::MicrosoftTeams => "https://teams.microsoft.com/meet/demo".to_string(),
            MeetingProvider::Zoom => "https://zoom.us/j/123456789".to_string(),
        });
    let bot_name = find_flag_value(args, "--name").unwrap_or("INF Yoda Notetaker");
    let config = MeetingSessionConfig {
        root: root.to_path_buf(),
        meeting_url,
        provider,
        bot_name: bot_name.to_string(),
        max_duration_minutes: 60,
        audio_chunk_seconds: 3,
        stt_model: String::new(),
        realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
        mistral_api_key: None,
    };
    let mut session = MeetingSession::new(&config);
    session.status = "ended".to_string();
    session.ended_at = Some(now_iso_string());

    for transcript in find_flag_values(args, "--transcript") {
        let transcript = transcript.trim();
        if !transcript.is_empty() {
            session.push_stt_transcript(transcript.to_string(), None);
        }
    }
    for chat in find_flag_values(args, "--chat") {
        let (sender, text) = chat
            .split_once(':')
            .map(|(sender, text)| (sender.trim(), text.trim()))
            .unwrap_or(("Participant", chat.trim()));
        if !text.is_empty() {
            session.chat_messages.push(ChatMessage {
                sender: if sender.is_empty() {
                    "Participant".to_string()
                } else {
                    sender.to_string()
                },
                text: text.to_string(),
                timestamp: now_iso_string(),
            });
        }
    }
    for audio_path in find_flag_values(args, "--audio") {
        match persist_audio_chunk(root, &session.session_id, audio_path) {
            Some(path) => session.pending_audio_chunks.push(path),
            None => eprintln!("[meeting] warning: could not persist fixture audio {audio_path}"),
        }
    }

    session.save(root)?;
    let finalization = finalize_meeting(root, &session, &config)?;
    Ok(json!({
        "ok": true,
        "session_id": session.session_id,
        "provider": session.provider,
        "transcript_chunks": session.transcript_chunks.len(),
        "transcript_segments": session.transcript_segments.len(),
        "speaker_signals": session.speaker_signals.len(),
        "chat_messages": session.chat_messages.len(),
        "recording_artifacts": list_recording_artifacts(root, &session.session_id),
        "finalization": finalization,
    }))
}

// ---------------------------------------------------------------------------
// Meeting runner — spawns Node.js, reads events, drives STT + chat
// ---------------------------------------------------------------------------

/// Run a meeting session synchronously: spawn Playwright, capture audio,
/// transcribe chunks, handle @CTOX mentions, finalize on meeting end.
pub(crate) fn run_meeting_session(root: &Path, config: &MeetingSessionConfig) -> Result<Value> {
    let mut session = MeetingSession::new(config);
    session.save(root)?;

    // Generate the runner script
    let script = build_meeting_runner_script_with_timeout(config)?;

    // Find the Playwright reference dir — script must live inside it so
    // Node's ESM resolver finds the local node_modules/playwright.
    let reference_dir = root.join("runtime/browser/interactive-reference");
    if !reference_dir.exists() {
        bail!(
            "Playwright reference directory not found at {}. Run `cd {} && npm install` first.",
            reference_dir.display(),
            reference_dir.display()
        );
    }
    let script_path = reference_dir.join(format!(".meeting-{}.mjs", session.session_id));
    fs::write(&script_path, &script)?;
    let command_path =
        meeting_sessions_dir(root).join(format!("{}.commands.jsonl", session.session_id));
    fs::write(&command_path, "")?;
    session.stdin_pipe = Some(command_path.display().to_string());
    session.save(root)?;

    // Find Node.js executable
    let node = find_node_executable()?;

    eprintln!(
        "[meeting] Starting {} session: {}",
        config.provider.as_str(),
        config.meeting_url
    );
    eprintln!("[meeting] Script: {}", script_path.display());

    // Pre-flight: start a transient STT backend when the meeting needs one.
    // If STT was already running, leave it alone. If this call starts it only
    // for the meeting, the guard tears it back down after finalization.
    let mut stt_guard = MeetingSttRuntimeGuard::ensure_for_meeting(&config.root);
    let engine_reachable = check_engine_reachable(&config.root);
    let live_transcription_status = native_stt::live_transcription_status_json(&config.root);
    let local_live_ready = live_transcription_status
        .get("local_enabled_for_live_meetings")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    session.engine_was_reachable_at_start = engine_reachable;
    session.live_transcription_ready_at_start = local_live_ready;
    session.live_transcription_status_at_start = Some(live_transcription_status.clone());
    if engine_reachable {
        eprintln!("[meeting] STT runtime reachable via managed transport");
    } else {
        eprintln!("[meeting] WARNING: STT runtime not reachable via managed transport");
        eprintln!("[meeting] Audio chunks will still be captured and saved to disk.");
        if let Some(reason) = stt_guard.start_error.as_deref() {
            eprintln!("[meeting] STT auto-start failed: {reason}");
        }
        eprintln!(
            "[meeting] Unsent chunks will be retried at meeting end if the engine becomes available."
        );
    }
    if local_live_ready {
        eprintln!("[meeting] local live STT enabled; realtime streaming proof is present");
    } else {
        let reason = live_transcription_status
            .get("local_live_disabled_reason")
            .and_then(Value::as_str)
            .unwrap_or("not_live_ready");
        eprintln!(
            "[meeting] local live STT disabled ({reason}); microsoft meeting overlay requires realtime STT and will not fall back to Teams captions"
        );
    }

    // Spawn the Node.js process. On Linux VPS hosts there is usually no
    // interactive X server, but Teams needs a headed browser for media capture.
    let mut runner_cmd = build_meeting_runner_command(&node, &reference_dir, &script_path)?;
    runner_cmd
        .env("CTOX_MEETING_COMMAND_FILE", &command_path)
        .env(
            "CTOX_MISTRAL_REALTIME_STT_MODEL",
            &config.realtime_stt_model,
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(api_key) = config.mistral_api_key.as_deref() {
        runner_cmd
            .env("CTOX_MISTRAL_API_KEY", api_key)
            .env("MISTRAL_API_KEY", api_key);
    }
    let mut child = runner_cmd.spawn().with_context(|| {
        format!(
            "failed to spawn meeting browser runner via {:?}",
            runner_cmd
        )
    })?;

    // Drain stderr in a background thread so we surface Node.js errors
    // (otherwise the pipe fills, blocks, and we never see the failure)
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                eprintln!("[meeting:node] {line}");
            }
        });
    }

    session.pid = Some(child.id());
    session.status = "running".to_string();
    session.save(root)?;

    let stdout = child.stdout.take().context("no stdout from node process")?;
    let stdin = child.stdin.take();

    // Read stdout line by line (JSON-lines protocol)
    let reader = BufReader::new(stdout);
    let mut join_failure_reason: Option<String> = None;
    let mut last_speaker_signal: Option<SpeakerSignal> = None;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let event: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                eprintln!(
                    "[meeting] non-JSON output: {}",
                    &line[..line.len().min(200)]
                );
                continue;
            }
        };

        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");

        match event_type {
            "status" => {
                let status = event.get("status").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] status: {status}");
            }
            "joined" => {
                let reason = event.get("reason").and_then(Value::as_str).unwrap_or("");
                if reason.is_empty() {
                    eprintln!("[meeting] Joined meeting successfully");
                } else {
                    eprintln!("[meeting] Joined meeting successfully ({reason})");
                }
                session.status = "active".to_string();
                session.save(root)?;
            }
            "join_failed" => {
                let reason = event.get("reason").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] join verification failed: {reason}");
                session.status = "join_failed".to_string();
                session.ended_at = Some(now_iso_string());
                session.save(root)?;
                join_failure_reason = Some(reason.to_string());
            }
            "audio_chunk" => {
                let chunk_path = event.get("path").and_then(Value::as_str).unwrap_or("");
                if chunk_path.is_empty() {
                    continue;
                }
                // Copy the chunk into the session's persistent directory so it
                // survives after the node process exits (the JS writes to a
                // tempDir that gets cleaned up).
                let persisted_path = persist_audio_chunk(root, &session.session_id, chunk_path);
                let chunk_for_stt = persisted_path.as_deref().unwrap_or(chunk_path);

                match transcribe_audio_chunk(
                    &config.root,
                    Path::new(chunk_for_stt),
                    &config.stt_model,
                ) {
                    Ok(text) if !text.is_empty() => {
                        eprintln!("[meeting] transcript: {}...", &text[..text.len().min(80)]);
                        let direct_speaker = recent_direct_speaker(last_speaker_signal.as_ref());
                        session.push_stt_transcript(text, direct_speaker);
                        session.save(root)?;
                        if let Some(p) = persisted_path.as_ref() {
                            let _ = fs::remove_file(p);
                        }
                    }
                    Ok(_) => {
                        // Empty transcript (silence) — drop the chunk, it's useless
                        if let Some(p) = persisted_path.as_ref() {
                            let _ = fs::remove_file(p);
                        }
                    }
                    Err(err) => {
                        eprintln!("[meeting] STT error: {err}");
                        // Keep the chunk for retry at finalize time
                        if let Some(p) = persisted_path {
                            session.pending_audio_chunks.push(p);
                            session.save(root)?;
                        }
                    }
                }
            }
            "active_speaker" => {
                if let Some(signal) = SpeakerSignal::from_event(&event) {
                    eprintln!(
                        "[meeting] active speaker [{}]: {}",
                        signal.source, signal.speaker_display
                    );
                    last_speaker_signal = Some(signal.clone());
                    session.speaker_signals.push(signal);
                    session.save(root)?;
                }
            }
            "speaker_probe" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] speaker probe: {}", &text[..text.len().min(500)]);
            }
            "transcript_segment" => {
                if let Some(segment) = TranscriptSegment::from_platform_event(&event) {
                    eprintln!(
                        "[meeting] transcript segment [{}] {}: {}...",
                        segment.source,
                        segment.speaker_display,
                        &segment.text[..segment.text.len().min(80)]
                    );
                    session.push_platform_transcript(segment);
                    session.save(root)?;
                }
            }
            "chat" => {
                let sender = event
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown");
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                let ts = event.get("ts").and_then(Value::as_str).unwrap_or("");

                // --- Self-loop protection ---
                // Skip messages that originated from this bot itself, or that
                // duplicate the most recent message we sent (sometimes the
                // chat-send round-trips back through the scraper).
                if session.is_own_message(sender, text) {
                    eprintln!(
                        "[meeting] skipped own message: {}",
                        &text[..text.len().min(60)]
                    );
                    continue;
                }

                eprintln!("[meeting] chat [{sender}]: {text}");
                session.chat_messages.push(ChatMessage {
                    sender: sender.to_string(),
                    text: text.to_string(),
                    timestamp: ts.to_string(),
                });
                if MeetingSession::is_mention(text) {
                    let ack_text = first_mention_ack_text();
                    if !session
                        .outbound_chat_texts
                        .iter()
                        .any(|sent| normalize_chat_text(sent) == normalize_chat_text(ack_text))
                    {
                        if let Some(stdin_path) = session.stdin_pipe.as_deref() {
                            let command = json!({"action": "send_chat", "text": ack_text});
                            match fs::OpenOptions::new().append(true).open(stdin_path) {
                                Ok(mut file) => {
                                    let _ = writeln!(file, "{}", command);
                                    eprintln!("[meeting] queued immediate mention ack");
                                }
                                Err(err) => {
                                    eprintln!(
                                        "[meeting] warning: could not queue immediate mention ack: {err}"
                                    );
                                }
                            }
                        }
                        session.outbound_chat_texts.push(ack_text.to_string());
                    }
                }
                // Persist session so sync() can pick up new chat messages
                session.save(root)?;

                // @CTOX mentions are now ingested as normal inbound messages
                // via sync() → upsert_communication_message(). The service
                // loop's route_external_messages() will pick them up and
                // route them to the agent with the meeting-participant skill.
                // No extra queue task needed — the standard pipeline handles it.
            }
            "command_received" => {
                let action = event.get("action").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] command received: {action}");
            }
            "chat_sent" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] chat sent: {}", &text[..text.len().min(80)]);
                if !text.is_empty() {
                    session.outbound_chat_texts.push(text.to_string());
                    session.save(root)?;
                }
            }
            "chat_send_failed" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                eprintln!(
                    "[meeting] chat send failed: {}",
                    &text[..text.len().min(80)]
                );
            }
            "recording_artifact" => {
                let artifact_path = event.get("path").and_then(Value::as_str).unwrap_or("");
                if artifact_path.is_empty() {
                    continue;
                }
                match persist_recording_artifact(root, &session.session_id, artifact_path) {
                    Some(path) => eprintln!("[meeting] recording artifact: {path}"),
                    None => eprintln!(
                        "[meeting] recording artifact persist failed: {}",
                        &artifact_path[..artifact_path.len().min(160)]
                    ),
                }
            }
            "ffmpeg_error" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] ffmpeg error: {}", &text[..text.len().min(200)]);
            }
            "ffmpeg_exit" => {
                let code = event.get("code").and_then(Value::as_i64).unwrap_or(-1);
                eprintln!("[meeting] ffmpeg exited with code {code}");
            }
            "participant_count" => {
                let count = event.get("count").and_then(Value::as_u64).unwrap_or(0);
                eprintln!("[meeting] participants: {count}");
            }
            "ended" => {
                let reason = event
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                eprintln!("[meeting] Meeting ended: {reason}");
                session.status = "ended".to_string();
                session.ended_at = Some(now_iso_string());
                break;
            }
            "finalized" => {
                break;
            }
            "error" => {
                let msg = event.get("message").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] error: {msg}");
            }
            "warning" => {
                let msg = event.get("message").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting] warning: {msg}");
            }
            "browser_log" => {
                let level = event.get("level").and_then(Value::as_str).unwrap_or("log");
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                eprintln!("[meeting:browser:{level}] {text}");
            }
            _ => {}
        }
    }

    // Wait for process to exit
    let _ = child.wait();

    // Clean up script file
    let _ = fs::remove_file(&script_path);

    if let Some(reason) = join_failure_reason {
        session.status = "join_failed".to_string();
        if session.ended_at.is_none() {
            session.ended_at = Some(now_iso_string());
        }
        session.save(root)?;
        drop(stdin); // close stdin pipe
        stt_guard.finish();
        return Ok(json!({
            "ok": false,
            "session_id": session.session_id,
            "provider": session.provider,
            "status": "join_failed",
            "reason": reason,
            "transcript_chunks": session.transcript_chunks.len(),
            "transcript_segments": session.transcript_segments.len(),
            "speaker_signals": session.speaker_signals.len(),
            "chat_messages": session.chat_messages.len(),
            "pending_audio_chunks": session.pending_audio_chunks.len(),
            "finalization": {
                "action": "skipped",
                "reason": "meeting was not joined"
            },
        }));
    }

    // Finalize
    session.status = "ended".to_string();
    if session.ended_at.is_none() {
        session.ended_at = Some(now_iso_string());
    }
    session.save(root)?;

    // Lazy re-transcription: if the engine is now reachable and we have
    // pending chunks from failed STT attempts, retry them now.
    let retry_result = retry_pending_audio_chunks(root, &mut session, config);
    let recording_transcript_result =
        transcribe_full_recording_if_needed(root, &mut session, config);

    let finalization = finalize_meeting(root, &session, config)?;

    drop(stdin); // close stdin pipe
    stt_guard.finish();

    Ok(json!({
        "ok": true,
        "session_id": session.session_id,
        "provider": session.provider,
        "status": "finalized",
        "transcript_chunks": session.transcript_chunks.len(),
        "transcript_segments": session.transcript_segments.len(),
        "speaker_signals": session.speaker_signals.len(),
        "chat_messages": session.chat_messages.len(),
        "pending_audio_chunks": session.pending_audio_chunks.len(),
        "stt_retry": retry_result,
        "recording_transcript": recording_transcript_result,
        "finalization": finalization,
    }))
}

/// Finalize a meeting: combine transcript, create system-onboarding queue task.
fn finalize_meeting(
    root: &Path,
    session: &MeetingSession,
    _config: &MeetingSessionConfig,
) -> Result<Value> {
    let transcript = session.full_transcript();
    let chat_log = session.full_chat_log();

    if transcript.is_empty() && chat_log.is_empty() {
        return Ok(json!({
            "action": "skipped",
            "reason": "no transcript or chat content to process",
        }));
    }

    // Save full transcript to file
    let transcript_path =
        meeting_sessions_dir(root).join(format!("{}-transcript.txt", session.session_id));
    fs::write(&transcript_path, &transcript)?;

    let chat_log_path =
        meeting_sessions_dir(root).join(format!("{}-chatlog.txt", session.session_id));
    if !chat_log.is_empty() {
        fs::write(&chat_log_path, &chat_log)?;
    }
    let recording_artifacts = list_recording_artifacts(root, &session.session_id);
    let artifact_manifest_path =
        meeting_sessions_dir(root).join(format!("{}-artifacts.json", session.session_id));
    fs::write(
        &artifact_manifest_path,
        serde_json::to_string_pretty(&json!({
            "session_id": session.session_id,
            "provider": session.provider,
            "meeting_url": session.meeting_url,
            "started_at": session.started_at,
            "ended_at": session.ended_at,
            "transcript_path": transcript_path.display().to_string(),
            "chatlog_path": chat_log_path.display().to_string(),
            "transcript_segment_count": session.transcript_segments.len(),
            "speaker_signal_count": session.speaker_signals.len(),
            "recording_artifacts": recording_artifacts,
        }))?,
    )?;

    // Build the post-meeting processing prompt
    let prompt = format!(
        "## Post-meeting transcript processing\n\
         \n\
         A **{provider}** meeting has ended. Process the transcript and chat log below.\n\
         \n\
         ### Meeting metadata\n\
         - Provider: {provider}\n\
         - URL: {url}\n\
         - Session: `{session_id}`\n\
         - Started: {started}\n\
         - Ended: {ended}\n\
         - Total transcript chunks: {chunk_count}\n\
         - Total structured transcript segments: {segment_count}\n\
         - Total speaker signals: {speaker_signal_count}\n\
         - Total chat messages: {chat_count}\n\
         - Transcript file: `{transcript_path}`\n\
         \n\
         ### What to extract\n\
         \n\
         Read the transcript carefully and extract:\n\
         \n\
         1. **Decisions** -- What was agreed upon? By whom?\n\
         2. **Action items** -- Who committed to doing what? By when?\n\
         3. **Open questions** -- What was discussed but not resolved?\n\
         4. **Reusable operational knowledge candidates** -- Only extract items that can become a \
         durable Skillbook/Runbook/Runbook-Item. Meeting facts, status notes, and one-off decisions are \
         not knowledge by themselves; keep those in the summary or tickets.\n\
         \n\
         ### What to create\n\
         \n\
         - For each **action item**: Create a ticket with clear title, assignee, and deadline.\n\
         - For each **reusable operational knowledge candidate**: create or update a Skillbook/Runbook \
         bundle via `ctox ticket source-skill-import-bundle`. The durable knowledge artifact must land in \
         `knowledge_main_skills`, `knowledge_skillbooks`, `knowledge_runbooks`, and \
         `knowledge_runbook_items`. Do not use `ticket_knowledge_entries` as the final knowledge store.\n\
         - If the meeting produced only facts, decisions, or follow-up work and no reusable procedure, \
         do not create a knowledge artifact; keep the facts in the meeting summary and tickets.\n\
         - For **open questions**: Create a follow-up queue task.\n\
         - **Always**: Send a meeting summary to the relevant communication channel.\n\
         \n\
         ### Structured extraction contract\n\
         \n\
         Start by producing a compact JSON object with keys `decisions`, `action_items`, \
         `open_questions`, `runbook_candidates`, and `tickets_to_create`. Then perform the durable writes. \
         Each `runbook_candidates` item must name the target skillbook/runbook and explain why it is \
         reusable operational procedure rather than a meeting note. \
         Every ticket candidate must include `title`, `body`, `source_session_id`, and \
         `dedupe_rationale`.\n\
         \n\
         ### Quality checks\n\
         \n\
         - Use a participant name only when a transcript line has a platform speaker source or high confidence.\n\
         - If the source is plain STT, unknown, or low-confidence active-speaker correlation, say \"a participant\" instead of inventing a name.\n\
         - Distinguish between decisions (confirmed) and suggestions (discussed but not confirmed).\n\
         - Check existing tickets before creating duplicates.\n\
         - The summary should be something a human who missed the meeting can act on.\n\
         \n\
         ### Full transcript\n\
         {transcript}\n\
         \n\
         ### Chat log\n\
         {chat_log}\n",
        provider = session.provider,
        url = session.meeting_url,
        session_id = session.session_id,
        started = session.started_at,
        ended = session.ended_at.as_deref().unwrap_or("unknown"),
        chunk_count = session.transcript_chunks.len(),
        segment_count = session.transcript_segments.len(),
        speaker_signal_count = session.speaker_signals.len(),
        chat_count = session.chat_messages.len(),
        transcript_path = transcript_path.display(),
        transcript = if transcript.is_empty() {
            "(empty)"
        } else {
            &transcript
        },
        chat_log = if chat_log.is_empty() {
            "(no chat)"
        } else {
            &chat_log
        },
    );

    let post_meeting_ticket = crate::mission::ticket_local_native::create_local_ticket(
        root,
        &format!("Meeting Nachbereitung: {}", session.provider),
        &format!(
            "Default post-meeting processing ticket for session `{}`.\n\nTranscript: {}\nChat log: {}\nArtifact manifest: {}",
            session.session_id,
            transcript_path.display(),
            chat_log_path.display(),
            artifact_manifest_path.display(),
        ),
        Some("open"),
        Some("normal"),
    )
    .ok();

    // Ingest the summary as a normal inbound message in the "meeting" channel.
    // The service loop's route_external_messages() will pick it up and route
    // it to the agent with the meeting-participant skill via metadata.
    let db_path = root.join("runtime/ctox.sqlite3");
    let mut conn = open_channel_db(&db_path)?;
    let observed_at = now_iso_string();
    let message_key = format!(
        "meeting::{}::summary::{}",
        session.session_id,
        stable_digest(&format!("summary:{}", observed_at))
    );
    let metadata = json!({
        "provider": session.provider,
        "session_id": session.session_id,
        "source": "meeting_summary",
        "skill": "meeting-participant",
        "transcript_path": transcript_path.display().to_string(),
        "artifact_manifest_path": artifact_manifest_path.display().to_string(),
        "recording_artifacts": recording_artifacts.clone(),
        "transcript_segment_count": session.transcript_segments.len(),
        "speaker_signal_count": session.speaker_signals.len(),
        "post_meeting_ticket_id": post_meeting_ticket.as_ref().map(|ticket| ticket.ticket_id.clone()),
    });
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "meeting",
            account_key: "meeting:system",
            thread_key: &session.session_id,
            remote_id: &message_key,
            direction: "inbound",
            folder_hint: "summary",
            sender_display: "Meeting Bot",
            sender_address: "ctox@local",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &format!("{} meeting summary", session.provider),
            preview: &format!(
                "{} meeting ended — {} transcript chunks, {} chat messages",
                session.provider,
                session.transcript_chunks.len(),
                session.chat_messages.len()
            ),
            body_text: &prompt,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "internal",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, &session.session_id)?;
    ensure_routing_rows_for_inbound(&conn)?;

    Ok(json!({
        "action": "ingested",
        "message_key": message_key,
        "transcript_path": transcript_path.display().to_string(),
        "artifact_manifest_path": artifact_manifest_path.display().to_string(),
        "recording_artifact_count": recording_artifacts.len(),
        "post_meeting_ticket_id": post_meeting_ticket.as_ref().map(|ticket| ticket.ticket_id.clone()),
        "skill": "meeting-participant",
    }))
}

fn list_recording_artifacts(root: &Path, session_id: &str) -> Vec<String> {
    let session_dir = meeting_sessions_dir(root);
    let mut artifacts = Vec::new();
    if let Ok(entries) = fs::read_dir(&session_dir) {
        artifacts.extend(
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.starts_with(session_id))
                        .unwrap_or(false)
                })
                .filter(|path| is_recording_media_path(path))
                .map(|path| path.display().to_string()),
        );
    }

    let artifact_dir = meeting_sessions_dir(root).join(format!("{session_id}-audio"));
    if let Ok(entries) = fs::read_dir(&artifact_dir) {
        artifacts.extend(
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .filter(|path| is_recording_media_path(path))
                .map(|path| path.display().to_string()),
        );
    }
    artifacts.sort();
    artifacts.dedup();
    artifacts
}

fn is_recording_media_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "webm" | "mp4" | "wav" | "m4a" | "ogg"
            )
        })
        .unwrap_or(false)
}

fn is_full_meeting_recording_path(path: &Path, session_id: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.starts_with(session_id)
                && name.to_ascii_lowercase().contains("recording")
                && is_recording_media_path(path)
        })
        .unwrap_or(false)
}

fn full_meeting_recording_candidates(root: &Path, session_id: &str) -> Vec<PathBuf> {
    let mut candidates = list_recording_artifacts(root, session_id)
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| is_full_meeting_recording_path(path, session_id))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_len = fs::metadata(left).map(|meta| meta.len()).unwrap_or(0);
        let right_len = fs::metadata(right).map(|meta| meta.len()).unwrap_or(0);
        right_len.cmp(&left_len).then_with(|| left.cmp(right))
    });
    candidates
}

fn meeting_transcript_needs_recording_fallback(session: &MeetingSession) -> bool {
    let has_stt_segment = session
        .transcript_segments
        .iter()
        .any(|segment| segment.source.starts_with("stt"));
    if has_stt_segment {
        return false;
    }
    session.full_transcript().trim().chars().count() < 2_000
}

fn transcribe_full_recording_if_needed(
    root: &Path,
    session: &mut MeetingSession,
    config: &MeetingSessionConfig,
) -> Value {
    if !meeting_transcript_needs_recording_fallback(session) {
        return json!({"action": "skipped", "reason": "transcript already has usable STT or captions"});
    }

    let candidates = full_meeting_recording_candidates(root, &session.session_id);
    let Some(recording_path) = candidates.first() else {
        return json!({"action": "skipped", "reason": "no full recording artifact"});
    };

    eprintln!(
        "[meeting] transcript is incomplete; transcribing full recording {}",
        recording_path.display()
    );
    match transcribe_audio_chunk(&config.root, recording_path, &config.stt_model) {
        Ok(text) if !text.trim().is_empty() => {
            let text_chars = text.chars().count();
            session.push_stt_transcript(text, None);
            let _ = session.save(root);
            json!({
                "action": "transcribed_full_recording",
                "recording_path": recording_path.display().to_string(),
                "text_chars": text_chars,
            })
        }
        Ok(_) => json!({
            "action": "skipped",
            "reason": "full recording transcription returned empty text",
            "recording_path": recording_path.display().to_string(),
        }),
        Err(err) => {
            eprintln!(
                "[meeting] full recording transcription failed for {}: {err}",
                recording_path.display()
            );
            json!({
                "action": "failed",
                "reason": err.to_string(),
                "recording_path": recording_path.display().to_string(),
            })
        }
    }
}

/// At finalize time, re-check if the STT engine is reachable. If it is and we
/// have pending audio chunks from earlier failures, transcribe them now and
/// append the results to the transcript. Successfully transcribed chunks are
/// removed from disk. Returns a summary value.
fn retry_pending_audio_chunks(
    root: &Path,
    session: &mut MeetingSession,
    config: &MeetingSessionConfig,
) -> Value {
    if session.pending_audio_chunks.is_empty() {
        return json!({"action": "skipped", "reason": "no pending chunks"});
    }
    let engine_now_reachable = check_engine_reachable(&config.root);
    if !engine_now_reachable {
        return json!({
            "action": "skipped",
            "reason": "engine still unreachable",
            "pending_count": session.pending_audio_chunks.len(),
        });
    }

    eprintln!(
        "[meeting] STT engine now reachable — retrying {} pending chunks",
        session.pending_audio_chunks.len()
    );
    let mut succeeded = 0u32;
    let mut still_failing = Vec::new();
    let pending = std::mem::take(&mut session.pending_audio_chunks);
    for chunk_path in pending {
        match transcribe_audio_chunk(&config.root, Path::new(&chunk_path), &config.stt_model) {
            Ok(text) if !text.is_empty() => {
                session.push_stt_transcript(text, None);
                let _ = fs::remove_file(&chunk_path);
                succeeded += 1;
            }
            Ok(_) => {
                // Silence — drop the chunk
                let _ = fs::remove_file(&chunk_path);
            }
            Err(err) => {
                eprintln!("[meeting] retry STT error on {}: {err}", chunk_path);
                still_failing.push(chunk_path);
            }
        }
    }
    session.pending_audio_chunks = still_failing.clone();
    let _ = session.save(root);

    json!({
        "action": "retried",
        "succeeded": succeeded,
        "still_failing": still_failing.len(),
    })
}

/// Copy an audio chunk from the node-managed tempDir into a persistent
/// per-session directory so it survives after the node process exits.
/// Returns the persisted path, or None if the copy failed.
fn persist_audio_chunk(root: &Path, session_id: &str, source_path: &str) -> Option<String> {
    let src = Path::new(source_path);
    if !src.exists() {
        return None;
    }
    let metadata = fs::metadata(src).ok()?;
    if metadata.len() < 4096 {
        return None;
    }
    let dest_dir = meeting_sessions_dir(root).join(format!("{session_id}-audio"));
    if fs::create_dir_all(&dest_dir).is_err() {
        return None;
    }
    let filename = src.file_name()?;
    let dest = dest_dir.join(filename);
    if fs::copy(src, &dest).is_ok() {
        Some(dest.display().to_string())
    } else {
        None
    }
}

fn persist_recording_artifact(root: &Path, session_id: &str, source_path: &str) -> Option<String> {
    let src = Path::new(source_path);
    if !src.exists() {
        return None;
    }
    let ext = src
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("mp4");
    let dest = meeting_sessions_dir(root).join(format!("{session_id}-recording.{ext}"));
    if fs::copy(src, &dest).is_ok() {
        Some(dest.display().to_string())
    } else {
        None
    }
}

/// Check whether the managed STT runtime responds on its configured transport.
pub(crate) fn check_engine_reachable(root: &Path) -> bool {
    crate::communication::gateway::transcription_backend_reachable(root)
}

fn find_node_executable() -> Result<String> {
    for candidate in ["node", "/usr/local/bin/node", "/usr/bin/node"] {
        if Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Ok(candidate.to_string());
        }
    }
    // Try to find via PATH
    if let Ok(output) = Command::new("which").arg("node").output() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(path);
        }
    }
    bail!("node executable not found — install Node.js >= 18")
}

fn build_meeting_runner_command(
    node: &str,
    reference_dir: &Path,
    script_path: &Path,
) -> Result<Command> {
    if should_wrap_browser_runner_with_xvfb(std::env::var_os("DISPLAY").as_deref()) {
        let xvfb_run = find_xvfb_run_executable().with_context(|| {
            "DISPLAY is not set and xvfb-run was not found; install xvfb for VPS meeting capture"
        })?;
        eprintln!(
            "[meeting] DISPLAY is not set; launching headed browser runner via {}",
            xvfb_run.display()
        );
        let mut cmd = Command::new(xvfb_run);
        cmd.current_dir(reference_dir)
            .arg("-a")
            .arg("-s")
            .arg(MEETING_XVFB_SERVER_ARGS)
            .arg(node)
            .arg(script_path);
        Ok(cmd)
    } else {
        let mut cmd = Command::new(node);
        cmd.current_dir(reference_dir).arg(script_path);
        Ok(cmd)
    }
}

fn should_wrap_browser_runner_with_xvfb(display: Option<&OsStr>) -> bool {
    cfg!(target_os = "linux") && display.map(|value| value.is_empty()).unwrap_or(true)
}

fn find_xvfb_run_executable() -> Option<PathBuf> {
    for candidate in ["/usr/bin/xvfb-run", "/usr/local/bin/xvfb-run"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(output) = Command::new("which").arg("xvfb-run").output() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Meeting provider detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeetingProvider {
    GoogleMeet,
    MicrosoftTeams,
    Zoom,
}

impl MeetingProvider {
    pub(crate) fn detect(url: &str) -> Option<Self> {
        let lower = url.to_lowercase();
        if lower.contains("meet.google.com") {
            Some(Self::GoogleMeet)
        } else if lower.contains("teams.microsoft.com") || lower.contains("teams.live.com") {
            Some(Self::MicrosoftTeams)
        } else if lower.contains("zoom.us") || lower.contains("zoom.com") {
            Some(Self::Zoom)
        } else {
            None
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::GoogleMeet => "google",
            Self::MicrosoftTeams => "microsoft",
            Self::Zoom => "zoom",
        }
    }
}

// ---------------------------------------------------------------------------
// Meeting session management
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct MeetingSessionConfig {
    pub root: PathBuf,
    pub meeting_url: String,
    pub provider: MeetingProvider,
    pub bot_name: String,
    pub max_duration_minutes: u64,
    pub audio_chunk_seconds: u64,
    pub stt_model: String,
    pub realtime_stt_model: String,
    pub mistral_api_key: Option<String>,
}

impl MeetingSessionConfig {
    pub(crate) fn from_runtime(
        root: &Path,
        meeting_url: &str,
        runtime: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let provider = MeetingProvider::detect(meeting_url)
            .context("cannot detect meeting provider from URL")?;
        let bot_name = runtime_setting_or_env(runtime, "CTO_MEETING_BOT_NAME")
            .unwrap_or_else(|| "INF Yoda Notetaker".to_string());
        let max_duration_minutes =
            runtime_setting_or_env(runtime, "CTO_MEETING_MAX_DURATION_MINUTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(180u64);
        let audio_chunk_seconds =
            runtime_setting_or_env(runtime, "CTO_MEETING_AUDIO_CHUNK_SECONDS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(3u64);
        let stt_model = normalize_meeting_stt_model(
            runtime_setting_or_env(runtime, "CTOX_STT_MODEL").as_deref(),
        );
        let realtime_stt_model = runtime_setting_or_env(runtime, "CTOX_MISTRAL_REALTIME_STT_MODEL")
            .or_else(|| runtime_setting_or_env(runtime, "CTOX_STT_REALTIME_MODEL"))
            .unwrap_or_else(|| "voxtral-mini-transcribe-realtime-2602".to_string());
        let mistral_api_key = runtime_setting_or_env(runtime, "CTOX_MISTRAL_API_KEY")
            .or_else(|| runtime_setting_or_env(runtime, "MISTRAL_API_KEY"))
            .or_else(|| crate::secrets::get_credential(root, "CTOX_MISTRAL_API_KEY"))
            .or_else(|| crate::secrets::get_credential(root, "MISTRAL_API_KEY"));
        Ok(Self {
            root: root.to_path_buf(),
            meeting_url: meeting_url.to_string(),
            provider,
            bot_name,
            max_duration_minutes,
            audio_chunk_seconds,
            stt_model,
            realtime_stt_model,
            mistral_api_key,
        })
    }
}

fn runtime_setting_or_env(runtime: &BTreeMap<String, String>, key: &str) -> Option<String> {
    runtime
        .get(key)
        .cloned()
        .or_else(|| std::env::var(key).ok())
}

fn normalize_meeting_stt_model(configured: Option<&str>) -> String {
    let configured = configured.map(str::trim).unwrap_or("");
    if configured.is_empty() || is_disabled_selector(configured) {
        return DEFAULT_MEETING_STT_MODEL.to_string();
    }
    let selected = engine::auxiliary_model_selection(engine::AuxiliaryRole::Stt, Some(configured));
    if selected.request_model == DEFAULT_MEETING_STT_MODEL {
        selected.request_model.to_string()
    } else {
        DEFAULT_MEETING_STT_MODEL.to_string()
    }
}

/// Persistent state for one meeting session, written to disk as JSON.
#[derive(Debug, Clone)]
pub(crate) struct MeetingSession {
    pub session_id: String,
    pub provider: String,
    pub meeting_url: String,
    pub bot_name: String,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub transcript_chunks: Vec<String>,
    pub transcript_segments: Vec<TranscriptSegment>,
    pub speaker_signals: Vec<SpeakerSignal>,
    pub chat_messages: Vec<ChatMessage>,
    pub outbound_chat_texts: Vec<String>,
    pub pid: Option<u32>,
    pub stdin_pipe: Option<String>,
    /// Paths of audio chunk files whose STT failed (engine offline or error).
    /// Retried at finalize time if the engine becomes reachable.
    pub pending_audio_chunks: Vec<String>,
    /// Whether the STT engine was reachable when the session started.
    pub engine_was_reachable_at_start: bool,
    /// Whether local STT was proven suitable for live meeting transcripts.
    pub live_transcription_ready_at_start: bool,
    pub live_transcription_status_at_start: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatMessage {
    pub sender: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TranscriptSegment {
    pub timestamp: String,
    pub speaker_display: String,
    pub speaker_id: Option<String>,
    pub source: String,
    pub confidence: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SpeakerSignal {
    pub timestamp: String,
    pub speaker_display: String,
    pub speaker_id: Option<String>,
    pub source: String,
    pub confidence: f32,
}

impl TranscriptSegment {
    fn from_stt_text(text: String, speaker: Option<&SpeakerSignal>) -> Self {
        let (speaker_display, speaker_id, source, confidence) = speaker
            .map(|signal| {
                (
                    signal.speaker_display.clone(),
                    signal.speaker_id.clone(),
                    "stt_with_active_speaker".to_string(),
                    signal.confidence.min(0.65),
                )
            })
            .unwrap_or_else(|| ("unknown".to_string(), None, "stt".to_string(), 0.25));
        Self {
            timestamp: now_iso_string(),
            speaker_display,
            speaker_id,
            source,
            confidence,
            text,
        }
    }

    fn from_platform_event(event: &Value) -> Option<Self> {
        let text = event.get("text").and_then(Value::as_str)?.trim();
        if text.is_empty() {
            return None;
        }
        let speaker_display = sanitize_speaker_display(
            event
                .get("speaker")
                .or_else(|| event.get("speaker_display"))
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        );
        Some(Self {
            timestamp: event
                .get("ts")
                .or_else(|| event.get("timestamp"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(now_iso_string),
            speaker_display,
            speaker_id: event
                .get("speaker_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned),
            source: event
                .get("source")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("platform_caption")
                .to_string(),
            confidence: event
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|value| value.clamp(0.0, 1.0) as f32)
                .unwrap_or(0.85),
            text: text.to_string(),
        })
    }

    fn render_line(&self) -> String {
        format!(
            "[{}] {}: {} [source={} confidence={:.2}]",
            self.timestamp, self.speaker_display, self.text, self.source, self.confidence
        )
    }
}

impl SpeakerSignal {
    fn from_event(event: &Value) -> Option<Self> {
        let speaker_display = sanitize_speaker_display(
            event
                .get("speaker")
                .or_else(|| event.get("speaker_display"))
                .and_then(Value::as_str)?,
        );
        if speaker_display.eq_ignore_ascii_case("unknown") {
            return None;
        }
        Some(Self {
            timestamp: event
                .get("ts")
                .or_else(|| event.get("timestamp"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(now_iso_string),
            speaker_display,
            speaker_id: event
                .get("speaker_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned),
            source: event
                .get("source")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("platform_active_speaker")
                .to_string(),
            confidence: event
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|value| value.clamp(0.0, 1.0) as f32)
                .unwrap_or(0.55),
        })
    }
}

fn sanitize_speaker_display(value: &str) -> String {
    let cleaned = value
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned
        .trim_matches(|ch: char| ch == ':' || ch == '-' || ch == '|' || ch.is_whitespace())
        .trim();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned.chars().take(96).collect()
    }
}

impl MeetingSession {
    pub(crate) fn new(config: &MeetingSessionConfig) -> Self {
        let session_id = format!(
            "meeting-{}-{}",
            config.provider.as_str(),
            now_epoch_millis()
        );
        Self {
            session_id,
            provider: config.provider.as_str().to_string(),
            meeting_url: config.meeting_url.clone(),
            bot_name: config.bot_name.clone(),
            status: "joining".to_string(),
            started_at: now_iso_string(),
            ended_at: None,
            transcript_chunks: Vec::new(),
            transcript_segments: Vec::new(),
            speaker_signals: Vec::new(),
            chat_messages: Vec::new(),
            outbound_chat_texts: Vec::new(),
            pid: None,
            stdin_pipe: None,
            pending_audio_chunks: Vec::new(),
            engine_was_reachable_at_start: false,
            live_transcription_ready_at_start: false,
            live_transcription_status_at_start: None,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "session_id": self.session_id,
            "provider": self.provider,
            "meeting_url": self.meeting_url,
            "bot_name": self.bot_name,
            "status": self.status,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
            "transcript_chunk_count": self.transcript_chunks.len(),
            "transcript_segment_count": self.transcript_segments.len(),
            "speaker_signal_count": self.speaker_signals.len(),
            "chat_message_count": self.chat_messages.len(),
            "transcript_chunks": self.transcript_chunks,
            "transcript_segments": self.transcript_segments,
            "speaker_signals": self.speaker_signals,
            "chat_messages": self.chat_messages.iter().map(|m| json!({
                "sender": m.sender,
                "text": m.text,
                "timestamp": m.timestamp,
            })).collect::<Vec<_>>(),
            "outbound_chat_texts": &self.outbound_chat_texts,
            "pid": self.pid,
            "stdin_pipe": self.stdin_pipe,
            "pending_audio_chunks": self.pending_audio_chunks,
            "engine_was_reachable_at_start": self.engine_was_reachable_at_start,
            "live_transcription_ready_at_start": self.live_transcription_ready_at_start,
            "live_transcription_status_at_start": self.live_transcription_status_at_start,
        })
    }

    pub(crate) fn save(&self, root: &Path) -> Result<()> {
        let dir = meeting_sessions_dir(root);
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", self.session_id));
        fs::write(&path, serde_json::to_string_pretty(&self.to_json())?)?;
        Ok(())
    }

    /// Build the full transcript from all chunks.
    pub(crate) fn full_transcript(&self) -> String {
        if !self.transcript_segments.is_empty() {
            return self
                .transcript_segments
                .iter()
                .map(TranscriptSegment::render_line)
                .collect::<Vec<_>>()
                .join("\n");
        }
        self.transcript_chunks.join("\n")
    }

    pub(crate) fn push_stt_transcript(&mut self, text: String, speaker: Option<&SpeakerSignal>) {
        if text.trim().is_empty() {
            return;
        }
        self.transcript_chunks.push(text.clone());
        self.transcript_segments
            .push(TranscriptSegment::from_stt_text(text, speaker));
    }

    pub(crate) fn push_platform_transcript(&mut self, segment: TranscriptSegment) {
        if segment.text.trim().is_empty() {
            return;
        }
        self.transcript_chunks.push(segment.text.clone());
        self.transcript_segments.push(segment);
    }

    /// Build the full chat log.
    pub(crate) fn full_chat_log(&self) -> String {
        self.chat_messages
            .iter()
            .map(|msg| format!("[{}] {}: {}", msg.timestamp, msg.sender, msg.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if a chat message mentions the meeting bot.
    /// Returns true if a known bot mention appears with a word boundary on both sides
    /// (so "@ctoxbar" doesn't match, but "@INF Yoda Notetaker" or "@ctox!" do).
    pub(crate) fn is_mention(text: &str) -> bool {
        let lower = normalize_chat_text(text).to_lowercase();
        for prefix in ["inf yoda notetaker", "inf yoda", "ctox"] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let rest = rest.trim_start();
                if rest.starts_with(':') || rest.starts_with('-') || rest.starts_with(',') {
                    return true;
                }
            }
        }
        for needle in ["@ctox", "@inf yoda", "@inf yoda notetaker"] {
            let mut search_from = 0;
            while let Some(pos) = lower[search_from..].find(needle) {
                let abs_pos = search_from + pos;
                let after = abs_pos + needle.len();
                // Word boundary check: char after must be non-alphanumeric (or end of string)
                let bounded = lower[after..]
                    .chars()
                    .next()
                    .map(|c| !c.is_ascii_alphanumeric())
                    .unwrap_or(true);
                if bounded {
                    return true;
                }
                search_from = after;
            }
        }
        false
    }

    /// Check if a chat message likely originated from this bot itself.
    /// Used to prevent self-loop when the bot's own replies appear in the chat.
    pub(crate) fn is_own_message(&self, sender: &str, text: &str) -> bool {
        is_own_message_text(&self.bot_name, &self.outbound_chat_texts, sender, text)
    }
}

fn is_own_message_text(
    bot_name: &str,
    outbound_chat_texts: &[String],
    sender: &str,
    text: &str,
) -> bool {
    let bot_name_lower = normalize_chat_text(bot_name).to_lowercase();
    let bot_name_lower = bot_name_lower.trim();
    if bot_name_lower.is_empty() {
        return false;
    }
    let sender_lower = normalize_chat_text(sender).to_lowercase();
    // Match if sender contains the bot name (sender field may include
    // role suffixes like "(Host)" or be wrapped in other text)
    if sender_lower.contains(bot_name_lower) {
        return true;
    }
    if matches!(
        sender_lower.trim(),
        "you" | "me" | "ich" | "du" | "ctox" | "ctox notetaker"
    ) {
        return true;
    }
    // Some chat scrapers misattribute and put the sender in the text;
    // match if text starts with the bot name + colon/dash separator
    let text_lower = normalize_chat_text(text).to_lowercase();
    if outbound_chat_texts.iter().any(|sent| {
        let sent = normalize_chat_text(sent).trim().to_lowercase();
        !sent.is_empty() && text_lower.contains(&sent)
    }) {
        return true;
    }
    false
}

fn normalize_chat_text(value: &str) -> String {
    value
        .replace('\u{00a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Playwright meeting runner script generation
// ---------------------------------------------------------------------------
//
// The templates below are transplanted from the ScreenApp meeting-bot reference
// implementation (~/Downloads/meeting-bot).  Key architectural decisions kept:
//
//   * Google Meet + Zoom → getDisplayMedia + MediaRecorder (in-browser capture)
//   * Microsoft Teams    → ffmpeg + X11grab + PulseAudio (out-of-process capture)
//   * Participant detection per provider uses the exact DOM queries from the
//     reference (data-avatar-count, badge-div .egzc7c, #wc-footer, etc.)
//   * Silence detection via AudioContext+Analyser (Google/Zoom) or parec (Teams)
//   * Each provider's join-flow includes retry logic, device-notification
//     dismissal, and lobby-mode detection with the same text constants.
//
// Placeholders: __MEETING_URL__, __BOT_NAME__, __PROVIDER__, __CHUNK_SECONDS__,
// __MAX_DURATION_MS__, __JOIN_SCRIPT__, __CHAT_SCRAPE_SCRIPT__,
// __SEND_CHAT_SCRIPT__, __RECORDING_SCRIPT__

/// The runner template uses placeholder tokens (__MEETING_URL__ etc.) instead of
/// Rust format placeholders to avoid brace-escaping conflicts with JavaScript.
const MEETING_RUNNER_TEMPLATE: &str = r#"import process from "node:process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import readline from "node:readline";

const meetingUrl = __MEETING_URL__;
const botName = __BOT_NAME__;
const provider = "__PROVIDER__";
const chunkSeconds = __CHUNK_SECONDS__;
const maxDurationMs = __MAX_DURATION_MS__;
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-meeting-"));
const commandFile = process.env.CTOX_MEETING_COMMAND_FILE || "";
let commandFileOffset = 0;
let stdoutClosed = false;

process.stdout.on("error", (err) => {
  if (err && err.code === "EPIPE") {
    stdoutClosed = true;
    return;
  }
  console.error("[CTOX_MEETING_STDOUT_ERROR]", err?.stack || err);
});

const emit = (event) => {
  if (stdoutClosed) return;
  try {
    process.stdout.write(JSON.stringify(event) + "\n");
  } catch (err) {
    if (err && err.code === "EPIPE") {
      stdoutClosed = true;
      return;
    }
    console.error("[CTOX_MEETING_EMIT_ERROR]", err?.stack || err);
  }
};

const visibleMeetingText = async () => {
  try {
    return await page.evaluate(() => document.body?.innerText || "");
  } catch { return ""; }
};

const buildZoomWebClientUrl = (url) => {
  try {
    const parsed = new URL(url);
    if (parsed.hostname === "events.zoom.us") return url;
    if (parsed.pathname.includes("/wc/")) return url;
    const meetingId = parsed.pathname.match(/\/j\/(\d+)/)?.[1];
    if (!meetingId) return url;
    const webClientUrl = new URL(`https://app.zoom.us/wc/${meetingId}/join`);
    const pwd = parsed.searchParams.get("pwd");
    if (pwd) webClientUrl.searchParams.set("pwd", pwd);
    return webClientUrl.toString();
  } catch {
    return url;
  }
};

const verifyJoinedUi = async (timeoutMs = 30000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const state = await page.evaluate((providerName) => {
        const text = document.body?.innerText || "";
        const lower = text.toLowerCase();
        const buttons = Array.from(document.querySelectorAll("button"));
        const attrs = buttons.map((button) => [
          button.innerText || "",
          button.textContent || "",
          button.getAttribute("aria-label") || "",
          button.getAttribute("title") || "",
        ].join(" ")).join("\n").toLowerCase();

        if (providerName === "zoom") {
          const removedHints = [
            "you have been removed",
            "you were removed",
            "host removed you",
            "meeting has ended",
            "this meeting has been ended",
            "no one responded to your request",
          ];
          if (removedHints.some((hint) => lower.includes(hint))) {
            return { joined: false, reason: "removed_or_ended" };
          }
          const strongLeave = document.querySelector(
            'button[aria-label="Leave"], button[aria-label*="Leave" i], button[title*="Leave" i], button[aria-label*="Verlassen" i]'
          );
          if (strongLeave) return { joined: true, reason: "zoom_leave_control_visible" };
          const blockingHints = [
            "please wait",
            "waiting room",
            "host has not joined",
            "the host will let you in soon",
            "we've let them know you're here",
            "we have let them know you're here",
            "meeting host will let you in soon",
            "bitte warten",
            "warteraum",
            "host hat das meeting noch nicht gestartet",
            "meeting passcode",
            "meeting password",
            "sign in to join",
            "authenticating",
            "not authorized",
          ];
          if (blockingHints.some((hint) => lower.includes(hint))) {
            return { joined: false, reason: "waiting_lobby" };
          }
        }

        const lobbyHints = [
          "someone will let you in",
          "jemand wird sie",
          "wird sie in kuerze einlassen",
          "wird sie in kürze einlassen",
          "bitte warten",
          "please wait",
          "waiting room",
          "warteraum",
          "host has not joined",
          "asking to join",
          "request to join",
        ];
        if (lobbyHints.some((hint) => lower.includes(hint))) {
          return { joined: false, reason: "waiting_lobby" };
        }

        const leaveHints = ["leave", "leave call", "verlassen", "anruf verlassen"];
        if (leaveHints.some((hint) => attrs.includes(hint) || lower.includes(hint))) {
          return { joined: true, reason: "leave_control_visible" };
        }

        if (providerName === "zoom") {
          const footer = document.querySelector('#wc-footer');
          if (footer && /participants?|teilnehmer/i.test(footer.textContent || "")) {
            return { joined: true, reason: "zoom_footer_visible" };
          }
        }

        const meetingChromeHints = ["participants", "teilnehmer", "people", "personen", "chat"];
        if (meetingChromeHints.some((hint) => attrs.includes(hint))) {
          return { joined: true, reason: "meeting_controls_visible" };
        }
        return { joined: false, reason: "meeting_controls_not_visible" };
      }, provider);
      if (state?.joined) return state;
      if (state?.reason === "waiting_lobby") {
        emit({ type: "status", status: "waiting_lobby", provider });
      }
    } catch {}
    await page.waitForTimeout(2000);
  }
  const text = await visibleMeetingText();
  return { joined: false, reason: "join_verification_timeout", bodyText: text.substring(0, 500) };
};

const { chromium } = await import("playwright");

// Browser args differ per provider (transplanted from ScreenApp reference chromium.ts)
const baseBrowserArgs = [
  "--enable-usermedia-screen-capturing",
  "--allow-http-screen-capture",
  "--no-sandbox",
  "--disable-setuid-sandbox",
  "--disable-web-security",
  "--use-gl=angle",
  "--use-angle=swiftshader",
  "--in-process-gpu",
  "--window-size=1280,720",
  "--auto-accept-this-tab-capture",
  "--enable-features=MediaRecorder",
  "--enable-audio-service-out-of-process",
  "--autoplay-policy=no-user-gesture-required",
];
// Teams needs fake devices for pre-join toggle interaction + kiosk for ffmpeg capture
// Google/Zoom use getDisplayMedia and don't need fake devices
const fakeDeviceArgs = ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"];
const displayArgs = provider === "microsoft" ? ["--kiosk", "--start-maximized"] : [];
const browserArgs = provider === "microsoft"
  ? [...baseBrowserArgs, ...fakeDeviceArgs, ...displayArgs]
  : baseBrowserArgs;

const launchOptions = {
  headless: false,
  args: browserArgs,
  ignoreDefaultArgs: ["--mute-audio"],
};

// Try to find chromium executable from Playwright cache (best-effort —
// if not found, Playwright will fall back to its built-in resolution).
// Cache location: Linux=~/.cache/ms-playwright, macOS=~/Library/Caches/ms-playwright
let execPath = null;
try {
  const homeDir = os.homedir();
  const cacheDirs = [
    path.join(homeDir, "Library", "Caches", "ms-playwright"), // macOS
    path.join(homeDir, ".cache", "ms-playwright"),            // Linux
  ];
  for (const cacheDir of cacheDirs) {
    if (!fs.existsSync(cacheDir)) continue;
    // Prefer "chromium-NNNN" over "chromium-headless-shell-NNNN"
    const entries = fs.readdirSync(cacheDir)
      .filter(e => e.startsWith("chromium-") && !e.includes("headless-shell"));
    if (entries.length === 0) continue;
    const chromiumDir = path.join(cacheDir, entries[entries.length - 1]);
    // macOS variants: chrome-mac-arm64 / chrome-mac / chrome-mac-x64
    const candidates = [
      path.join(chromiumDir, "chrome-mac-arm64", "Google Chrome for Testing.app", "Contents", "MacOS", "Google Chrome for Testing"),
      path.join(chromiumDir, "chrome-mac", "Google Chrome for Testing.app", "Contents", "MacOS", "Google Chrome for Testing"),
      path.join(chromiumDir, "chrome-mac", "Chromium.app", "Contents", "MacOS", "Chromium"),
      path.join(chromiumDir, "chrome-linux", "chrome"),
      path.join(chromiumDir, "chrome-win", "chrome.exe"),
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) { execPath = candidate; break; }
    }
    if (execPath) break;
  }
} catch {}
if (execPath) launchOptions.executablePath = execPath;

const browser = await chromium.launch(launchOptions);
const context = await browser.newContext({
  permissions: ["camera", "microphone"],
  viewport: { width: 1280, height: 720 },
  ignoreHTTPSErrors: true,
  userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
});
if (provider === "microsoft") {
  await context.addInitScript(({ botName }) => {
    if (window.__ctoxTranscriptCameraInstalled) return;
    window.__ctoxTranscriptCameraInstalled = true;
    const state = {
      botName: botName || "INF Yoda Notetaker",
      entries: [],
      status: "Realtime-Transcript wird verbunden",
      updatedAt: Date.now(),
      sequence: 0,
    };
    const compact = (value) => String(value || "").replace(/\s+/g, " ").trim();
    const wrapLine = (ctx, text, maxWidth) => {
      const words = compact(text).split(" ").filter(Boolean);
      const lines = [];
      let current = "";
      for (const word of words) {
        const next = current ? `${current} ${word}` : word;
        if (ctx.measureText(next).width > maxWidth && current) {
          lines.push(current);
          current = word;
        } else {
          current = next;
        }
      }
      if (current) lines.push(current);
      return lines;
    };
    const ensureCanvas = () => {
      if (window.__ctoxTranscriptCanvas) return window.__ctoxTranscriptCanvas;
      const canvas = document.createElement("canvas");
      canvas.width = 1280;
      canvas.height = 720;
      canvas.style.position = "fixed";
      canvas.style.left = "-10000px";
      canvas.style.top = "0";
      document.documentElement.appendChild(canvas);
      const ctx = canvas.getContext("2d");
      const draw = () => {
        const w = canvas.width;
        const h = canvas.height;
        ctx.fillStyle = "rgb(17,24,39)";
        ctx.fillRect(0, 0, w, h);
        const grd = ctx.createLinearGradient(0, 0, w, h);
        grd.addColorStop(0, "rgba(37, 99, 235, 0.28)");
        grd.addColorStop(1, "rgba(20, 184, 166, 0.18)");
        ctx.fillStyle = grd;
        ctx.fillRect(0, 0, w, h);
        ctx.fillStyle = "rgba(255,255,255,0.08)";
        ctx.fillRect(48, 46, w - 96, h - 92);
        ctx.fillStyle = "rgb(248,250,252)";
        ctx.font = "700 48px Arial, sans-serif";
        ctx.fillText(state.botName, 84, 118);
        ctx.font = "500 26px Arial, sans-serif";
        ctx.fillStyle = "rgb(203,213,225)";
        const age = Math.max(0, Math.round((Date.now() - state.updatedAt) / 1000));
        ctx.fillText(`${state.status} - aktualisiert vor ${age}s`, 86, 160);
        ctx.strokeStyle = "rgba(148,163,184,0.55)";
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(84, 190);
        ctx.lineTo(w - 84, 190);
        ctx.stroke();
        const entries = state.entries.slice(-4);
        let y = 250;
        if (entries.length === 0) {
          ctx.font = "600 40px Arial, sans-serif";
          ctx.fillStyle = "rgb(248,250,252)";
          ctx.fillText("Warte auf Realtime-Transcript...", 86, y);
        }
        for (const entry of entries) {
          const speaker = entry.speaker && entry.speaker !== "unknown" ? entry.speaker : "Sprecher unbekannt";
          ctx.font = "700 22px Arial, sans-serif";
          ctx.fillStyle = entry.speaker && entry.speaker !== "unknown" ? "rgb(147,197,253)" : "rgb(203,213,225)";
          const sourceLabel = entry.source === "platform_caption" ? "Teams" : "Realtime";
          ctx.fillText(`${speaker} · ${sourceLabel}`, 86, y);
          y += 32;
          ctx.font = "500 28px Arial, sans-serif";
          ctx.fillStyle = "rgb(248,250,252)";
          for (const line of wrapLine(ctx, entry.text, w - 190).slice(0, 2)) {
            if (y > h - 145) break;
            ctx.fillText(line, 86, y);
            y += 34;
          }
          y += 14;
          if (y > h - 145) break;
        }
        ctx.fillStyle = "rgba(17,24,39,0.72)";
        ctx.fillRect(48, h - 116, w - 96, 70);
        ctx.font = "400 22px Arial, sans-serif";
        ctx.fillStyle = "rgb(148,163,184)";
        ctx.fillText("CTOX Meeting Bot - Chat-Mentions und Audio werden protokolliert", 84, h - 70);
      };
      draw();
      window.__ctoxTranscriptDrawTimer = window.setInterval(draw, 1000);
      window.__ctoxTranscriptCanvas = canvas;
      return canvas;
    };
    const mergeText = (previous, next) => {
      previous = compact(previous);
      next = compact(next);
      if (!previous) return next;
      if (!next) return previous;
      if (next === previous || previous.endsWith(next)) return previous;
      if (next.startsWith(previous)) return next;
      const prevWords = previous.split(" ");
      const nextWords = next.split(" ");
      const maxOverlap = Math.min(prevWords.length, nextWords.length, 14);
      for (let size = maxOverlap; size >= 2; size--) {
        if (prevWords.slice(-size).join(" ").toLowerCase() === nextWords.slice(0, size).join(" ").toLowerCase()) {
          return compact(`${previous} ${nextWords.slice(size).join(" ")}`);
        }
      }
      return compact(`${previous} ${next}`);
    };
    window.__ctoxTranscriptOverlayPush = (text, speaker, source = "realtime_stt") => {
      const clean = compact(text);
      if (!clean || /^(sending|message sent)$/i.test(clean)) return;
      if (source === "chat") return;
      if (source === "platform_caption") return;
      const now = Date.now();
      if (source === "realtime_stt") {
        state.primarySource = "realtime_stt";
        state.primarySourceAt = now;
      }
      if (source === "platform_caption" && state.primarySource === "realtime_stt" && now - (state.primarySourceAt || 0) < 30000) {
        return;
      }
      const normalizedSpeaker = compact(speaker || "unknown");
      const compacted = clean.length > 700 ? `${clean.slice(0, 700).trim()} ...` : clean;
      const last = state.entries[state.entries.length - 1];
      const recentSameLine = last
        && last.source === source
        && last.speaker === normalizedSpeaker
        && now - last.ts < (source === "realtime_stt" ? 12000 : 5000);
      if (recentSameLine) {
        last.text = mergeText(last.text, compacted);
        last.ts = now;
      } else if (!last || last.text !== compacted || last.speaker !== normalizedSpeaker || last.source !== source) {
        state.sequence += 1;
        state.entries.push({ speaker: normalizedSpeaker, text: compacted, source, seq: state.sequence, ts: now });
      }
      state.entries = state.entries.slice(-10);
      state.status = "Realtime-STT aktiv";
      state.updatedAt = now;
    };
    window.__ctoxTranscriptOverlaySetStatus = (status) => {
      const clean = compact(status);
      if (!clean) return;
      state.status = clean;
      state.updatedAt = Date.now();
    };
    const originalGetUserMedia = navigator.mediaDevices?.getUserMedia?.bind(navigator.mediaDevices);
    if (!originalGetUserMedia) return;
    const silentAudioTracks = () => {
      try {
        const AudioCtx = window.AudioContext || window.webkitAudioContext;
        if (!AudioCtx) return [];
        if (!window.__ctoxSilentAudioContext) window.__ctoxSilentAudioContext = new AudioCtx();
        const dest = window.__ctoxSilentAudioContext.createMediaStreamDestination();
        const track = dest.stream.getAudioTracks()[0];
        if (track) track.enabled = false;
        return track ? [track] : [];
      } catch {
        return [];
      }
    };
    navigator.mediaDevices.getUserMedia = async (constraints = {}) => {
      const wantsVideo = !!constraints.video;
      const wantsAudio = !!constraints.audio;
      if (!wantsVideo && !wantsAudio) return originalGetUserMedia(constraints);
      if (!wantsVideo && wantsAudio) return new MediaStream(silentAudioTracks());
      let audioTracks = wantsAudio ? silentAudioTracks() : [];
      if (wantsAudio) {
        console.log("[CTOX_AUDIO] outgoing microphone replaced with silent local track");
      }
      const canvas = ensureCanvas();
      const videoStream = canvas.captureStream(12);
      const tracks = [...audioTracks, ...videoStream.getVideoTracks()];
      return new MediaStream(tracks);
    };
  }, { botName: "INF Yoda Notetaker" }).catch(() => {});
}
for (const origin of new Set([meetingUrl, provider === "zoom" ? buildZoomWebClientUrl(meetingUrl) : meetingUrl].map((url) => {
  try { return new URL(url).origin; } catch { return null; }
}).filter(Boolean))) {
  await context.grantPermissions(["microphone", "camera"], { origin }).catch(() => {});
}
let page = await context.newPage();

const pageText = async (candidate) => {
  try { return await candidate.evaluate(() => document.body?.innerText || ""); }
  catch { return ""; }
};

const isLikelyMeetingPage = async (candidate) => {
  const url = candidate.url();
  if (provider === "google") {
    if (url.includes("workspace.google.com/products/meet")) return false;
    if (/https:\/\/meet\.google\.com\/[a-z0-9-]+/i.test(url)) return true;
    const text = await pageText(candidate);
    return /Leave call|Verlassen|Anruf verlassen|Ask to join|Join now|Teilnahme anfragen|People|Participants|Teilnehmer|Chat/i.test(text)
      && !/KI-gestuetzte Videoanrufe|KI-gestützte Videoanrufe|Meet fuer Unternehmen testen|Meet für Unternehmen testen/i.test(text);
  }
  if (provider === "zoom" && /\/wc\/(join|[0-9]+)/.test(url)) return true;
  if (provider === "microsoft" && /teams\.microsoft\.com/.test(url)) return true;
  const text = await pageText(candidate);
  return /Leave call|Leave|Verlassen|Anruf verlassen|Ask to join|Join now|Teilnehmen|Teilnahme anfragen|People|Participants|Teilnehmer|Chat/i.test(text);
};

const selectActiveMeetingPage = async () => {
  for (let attempt = 0; attempt < 5; attempt++) {
    const pages = context.pages();
    for (let i = pages.length - 1; i >= 0; i--) {
      const candidate = pages[i];
      if (await isLikelyMeetingPage(candidate)) {
        page = candidate;
        await page.bringToFront().catch(() => {});
        return;
      }
    }
    await page.waitForTimeout(1000);
  }
};

const dismissZoomPopups = async (targetPage, timeoutMs = 15000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    let clicked = false;
    const scopes = [targetPage, ...targetPage.frames().filter((frame) => frame !== targetPage.mainFrame())];
    for (const scope of scopes) {
      const selectors = [
        'button[aria-label="close" i]',
        'button[title="Close" i]',
        'button:has-text("OK")',
        'button:has-text("Got it")',
        'button:has-text("Continue")',
        'button:has-text("Join Audio by Computer")',
      ];
      for (const selector of selectors) {
        try {
          const button = scope.locator(selector).first();
          if (await button.isVisible({ timeout: 500 }).catch(() => false)) {
            await button.click({ force: true, timeout: 1000 }).catch(() => {});
            clicked = true;
          }
        } catch {}
      }
    }
    if (!clicked) break;
    await targetPage.waitForTimeout(700);
  }
};

const countLiveAudioElements = async (targetPage) => {
  try {
    return await targetPage.evaluate(() => {
      return Array.from(document.querySelectorAll("audio, video")).filter((el) => {
        try {
          return !el.paused && el.readyState >= 2 && el.currentTime >= 0;
        } catch { return false; }
      }).length;
    });
  } catch {
    return 0;
  }
};

const prepareZoomAudio = async (targetPage) => {
  await dismissZoomPopups(targetPage, 5000);
  const scopes = () => [targetPage, ...targetPage.frames().filter((frame) => frame !== targetPage.mainFrame())];
  for (let attempt = 0; attempt < 3; attempt++) {
    let clicked = false;
    for (const scope of scopes()) {
      const audioSelectors = [
        'button[aria-label*="Join Audio" i]',
        'button:has-text("Join Audio")',
        'button:has-text("Join Audio by Computer")',
        'button:has-text("Computer Audio")',
        'button:has-text("Mit Computeraudio teilnehmen")',
      ];
      for (const selector of audioSelectors) {
        try {
          const button = scope.locator(selector).first();
          if (await button.isVisible({ timeout: 1000 }).catch(() => false)) {
            await button.click({ force: true, timeout: 2000 });
            clicked = true;
            break;
          }
        } catch {}
      }
      if (clicked) break;
    }
    await targetPage.waitForTimeout(clicked ? 2500 : 1000);
    if (await countLiveAudioElements(targetPage) > 0) break;
  }

  for (const scope of scopes()) {
    try {
      const stopVideo = scope.locator('button[aria-label*="Stop Video" i], button[title*="Stop Video" i]').first();
      if (await stopVideo.isVisible({ timeout: 1000 }).catch(() => false)) {
        await stopVideo.click({ force: true }).catch(() => {});
      }
    } catch {}
  }
};

const startZoomRemovalMonitor = (targetPage) => {
  let consecutiveMisses = 0;
  const interval = setInterval(async () => {
    try {
      const state = await targetPage.evaluate(() => {
        const text = (document.body?.innerText || "").toLowerCase();
        const removed = [
          "you have been removed",
          "you were removed",
          "host removed you",
          "meeting has ended",
          "this meeting has been ended",
          "no one responded to your request",
        ].some((hint) => text.includes(hint));
        const waiting = [
          "please wait",
          "waiting room",
          "the host will let you in soon",
          "we've let them know you're here",
        ].some((hint) => text.includes(hint));
        const leave = document.querySelector('button[aria-label*="Leave" i], button[title*="Leave" i]');
        return { removed, waiting, leaveVisible: Boolean(leave) };
      });
      if (state.removed) {
        clearInterval(interval);
        await targetPage.evaluate((reason) => window.ctoxMeetingEnd?.(reason), "zoom_removed_or_ended").catch(() => {});
        return;
      }
      if (state.waiting) return;
      consecutiveMisses = state.leaveVisible ? 0 : consecutiveMisses + 1;
      if (consecutiveMisses >= 6) {
        clearInterval(interval);
        await targetPage.evaluate((reason) => window.ctoxMeetingEnd?.(reason), "zoom_left_meeting").catch(() => {});
      }
    } catch {}
  }, 5000);
  return () => clearInterval(interval);
};

const enableTeamsLiveCaptions = async (targetPage) => {
  const scopes = () => [targetPage, ...targetPage.frames().filter((frame) => frame !== targetPage.mainFrame())];
  const tryClick = async (matchers) => {
    for (const scope of scopes()) {
      for (const matcher of matchers) {
        try {
          const roleTargets = [
            scope.getByRole("button", { name: matcher }).first(),
            scope.getByRole("menuitem", { name: matcher }).first(),
          ];
          for (const target of roleTargets) {
            if (await target.isVisible({ timeout: 600 }).catch(() => false)) {
              await target.click({ force: true });
              await targetPage.waitForTimeout(700);
              return true;
            }
          }
          const clicked = await scope.evaluate((matcherSource) => {
            const re = new RegExp(matcherSource.source, matcherSource.flags);
            const visible = (el) => {
              const rect = el.getBoundingClientRect();
              const style = window.getComputedStyle(el);
              return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
            };
            const nodes = Array.from(document.querySelectorAll('button, [role="button"], [role="menuitem"], [role="option"], [data-tid], span, div'));
            for (const node of nodes) {
              if (!visible(node)) continue;
              const text = `${node.getAttribute("aria-label") || ""} ${node.getAttribute("title") || ""} ${node.innerText || node.textContent || ""}`;
              if (!re.test(text)) continue;
              const clickable = node.closest('button, [role="button"], [role="menuitem"], [role="option"]') || node;
              clickable.click();
              return true;
            }
            return false;
          }, { source: matcher.source, flags: matcher.flags }).catch(() => false);
          if (clicked) {
            await targetPage.waitForTimeout(700);
            return true;
          }
        } catch {}
      }
    }
    return false;
  };

  if (await tryClick([/More/i, /Weitere/i, /Mehr/i])) {
    if (!(await tryClick([/^Captions$/i, /^Live captions$/i, /^Untertitel$/i, /^Liveuntertitel$/i, /Turn on live captions/i, /Untertitel aktivieren/i]))) {
      await tryClick([/Language and speech/i, /Sprache und Spracherkennung/i, /Speech/i]);
      await tryClick([/Turn on live captions/i, /^Live captions$/i, /^Captions$/i, /Untertitel aktivieren/i, /^Liveuntertitel$/i, /^Untertitel$/i]);
    }
  }
  await targetPage.keyboard.press(process.platform === "darwin" ? "Meta+Shift+C" : "Control+Shift+C").catch(() => {});
};

const muteTeamsMicrophone = async (targetPage) => {
  for (let attempt = 0; attempt < 4; attempt++) {
    const clicked = await targetPage.evaluate(() => {
      const visible = (el) => {
        const rect = el.getBoundingClientRect();
        const style = window.getComputedStyle(el);
        return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      };
      const buttons = Array.from(document.querySelectorAll("button, [role='button']"));
      for (const button of buttons) {
        if (!visible(button)) continue;
        const label = `${button.getAttribute("aria-label") || ""} ${button.getAttribute("title") || ""} ${button.innerText || button.textContent || ""}`;
        if (!/(mute mic|mute microphone|mikrofon stummschalten|stumm schalten)/i.test(label)) continue;
        if (/(unmute|nicht mehr stumm|stummschaltung aufheben)/i.test(label)) continue;
        button.click();
        return true;
      }
      return false;
    }).catch(() => false);
    if (clicked) {
      await targetPage.waitForTimeout(700);
      return true;
    }
    await targetPage.waitForTimeout(700);
  }
  return false;
};

// --- Join the meeting ---
emit({ type: "status", status: "joining", provider });
let navigationUrl = meetingUrl;
if (provider === "zoom") {
  navigationUrl = buildZoomWebClientUrl(meetingUrl);
}
try {
  await page.goto(navigationUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
} catch (err) {
  emit({ type: "warning", message: "Initial meeting navigation did not fully settle: " + err.message });
}
await page.waitForTimeout(5000);
await selectActiveMeetingPage();

__JOIN_SCRIPT__
await selectActiveMeetingPage();

const joinedState = await verifyJoinedUi(Math.min(30000, Math.max(5000, maxDurationMs)));
if (joinedState.joined) {
  emit({ type: "joined", provider, reason: joinedState.reason });
} else {
  emit({
    type: "join_failed",
    provider,
    reason: joinedState.reason || "unknown",
    bodyText: joinedState.bodyText || "",
  });
  emit({ type: "finalized", temp_dir: tempDir, provider });
  await browser.close();
  process.exit(2);
}

// --- Recording setup (transplanted from ScreenApp reference) ---
// Google Meet + Zoom: getDisplayMedia + MediaRecorder (in-browser tab capture)
// Microsoft Teams: ffmpeg + X11grab + PulseAudio (out-of-process)
let chunkIndex = 0;
await page.exposeFunction("ctoxAudioChunk", async (payload) => {
  const base64Data = typeof payload === "string" ? payload : payload.base64;
  const extension = typeof payload === "object" && payload.extension ? payload.extension : "webm";
  const filePath = path.join(tempDir, `chunk_${String(chunkIndex).padStart(4, "0")}.${extension}`);
  fs.writeFileSync(filePath, Buffer.from(base64Data, "base64"));
  emit({ type: "audio_chunk", path: filePath, index: chunkIndex });
  chunkIndex++;
});

let meetingEnded = false;
await page.exposeFunction("ctoxMeetingEnd", (reason) => {
  if (meetingEnded) return;
  emit({ type: "ended", reason: reason || "meeting_end" });
  meetingEnded = true;
});

// Capture browser console logs (errors, warnings, and CTOX audio diagnostics)
page.on("console", async (msg) => {
  const text = msg.text();
  const level = msg.type();
  if (text.includes("[CTOX_AUDIO]") || text.includes("error") || text.includes("Error") || level === "warning") {
    emit({ type: "browser_log", level, text });
  }
});

let stopZoomRemovalMonitor = null;
if (provider === "zoom") {
  await prepareZoomAudio(page).catch((err) => emit({ type: "warning", message: "Zoom audio preparation failed: " + err.message }));
  stopZoomRemovalMonitor = startZoomRemovalMonitor(page);
}
if (provider === "microsoft") {
  await muteTeamsMicrophone(page).catch((err) => emit({ type: "warning", message: "Teams microphone mute failed: " + err.message }));
  // Do not enable or consume Teams captions for Microsoft meetings. They are
  // client-side captions with Teams-controlled language settings and produced
  // unusable English hallucinations in German meetings. The Microsoft path must
  // use direct audio -> Mistral realtime STT, and fail visibly if that path is
  // unavailable.
  await muteTeamsMicrophone(page).catch(() => {});
}

// --- Live meeting observers: chat, captions, active speaker, participants ---
// These start before recording so Teams also gets real-time chat/speaker events
// while its ffmpeg branch blocks the main runner loop until the meeting ends.
const visibleNode = (el) => {
  try {
    const rect = el.getBoundingClientRect();
    const style = window.getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
  } catch { return false; }
};

const compactText = (value) => String(value || "").replace(/\s+/g, " ").trim();
const cleanSpeakerName = (value) => {
  let name = compactText(value)
    .replace(/\b(is speaking|speaking|active speaker|current speaker|spricht|aktueller sprecher)\b/ig, "")
    .replace(/[:|,-]+$/g, "")
    .trim();
  if (!name || name.length > 96) return "";
  return name;
};

const parseCaptionNode = (node, providerName) => {
  const raw = compactText(node.innerText || node.textContent || "");
  if (!raw || raw.length < 2 || raw.length > 1200) return null;
  if (/^(chat|people|participants|teilnehmer|leave|verlassen)$/i.test(raw)) return null;
  const aria = node.getAttribute?.("aria-label") || "";
  const className = String(node.getAttribute?.("class") || "");
  const dataTid = String(node.getAttribute?.("data-tid") || "");
  const role = String(node.getAttribute?.("role") || "");
  const captionish = /(caption|closed-caption|transcript|subtitle|untertitel)/i.test(`${aria} ${className} ${dataTid}`);
  if (/messages? addressed to|direct messages? are private/i.test(raw)) return null;
  if (/^(new notification|notification)[:：]/i.test(raw)) return null;
  if (/your video stopped working|camera and plugging it back|use another device/i.test(raw)) return null;
  if (providerName === "microsoft" && /(status|alert|log)/i.test(role) && !captionish) return null;
  if (providerName === "microsoft" && !captionish) return null;
  let speaker = "";
  let text = raw;
  const labelled = aria.match(/(?:caption|transcript|live caption).*?(?:from|by)\s+(.+?)[,:-]\s*(.+)$/i);
  if (labelled) {
    speaker = cleanSpeakerName(labelled[1]);
    text = compactText(labelled[2]);
  }
  if (!speaker) {
    const lines = (node.innerText || node.textContent || "").split(/\n+/).map(compactText).filter(Boolean);
    if (lines.length >= 2 && lines[0].length <= 80 && !/[.!?]$/.test(lines[0])) {
      speaker = cleanSpeakerName(lines[0]);
      text = compactText(lines.slice(1).join(" "));
    }
  }
  if (!speaker) {
    const speakerNode = node.querySelector?.('[data-speaker-name], [class*="speaker" i], [class*="name" i]');
    speaker = cleanSpeakerName(speakerNode?.textContent || "");
    if (speaker && raw.startsWith(speaker)) text = compactText(raw.slice(speaker.length));
  }
  if (!text || text === speaker) return null;
  return {
    speaker: speaker || "unknown",
    text,
    source: "platform_caption",
    confidence: speaker ? 0.9 : 0.65,
    provider: providerName,
    ts: new Date().toISOString(),
  };
};

const scrapeTranscriptEntries = (providerName) => {
  const doms = [document];
  try {
    const iframe = document.querySelector("iframe#webclient");
    if (iframe?.contentDocument) doms.push(iframe.contentDocument);
  } catch {}
  const selectorsByProvider = {
    google: [
      '[aria-live="polite"]',
      '[aria-live="assertive"]',
      '[role="status"]',
      '[jsname][data-ved]',
      '[class*="caption" i]',
    ],
    microsoft: [
      '[data-tid*="closed-caption" i]',
      '[data-tid*="caption" i]',
      '[class*="caption" i]',
      '[class*="transcript" i]',
    ],
    zoom: [
      '.live-transcription-subtitle',
      '.closed-caption',
      '[class*="caption" i]',
      '[class*="transcription" i]',
      '[aria-live="polite"]',
      '[aria-live="assertive"]',
    ],
  };
  const selectors = selectorsByProvider[providerName] || selectorsByProvider.google;
  const entries = [];
  for (const dom of doms) {
    for (const selector of selectors) {
      for (const node of Array.from(dom.querySelectorAll(selector))) {
        if (!visibleNode(node)) continue;
        const entry = parseCaptionNode(node, providerName);
        if (entry) entries.push(entry);
      }
    }
  }
  return entries;
};

const scrapeActiveSpeaker = (providerName) => {
  const doms = [document];
  try {
    const iframe = document.querySelector("iframe#webclient");
    if (iframe?.contentDocument) doms.push(iframe.contentDocument);
  } catch {}
  const selectorsByProvider = {
    google: [
      '[data-speaking="true"]',
      '[aria-label*="speaking" i]',
      '[aria-label*="spricht" i]',
      '[class*="speaking" i]',
      '[class*="active-speaker" i]',
    ],
    microsoft: [
      '[data-tid*="active-speaker" i]',
      '[data-tid*="speaking" i]',
      '[aria-label*="speaking" i]',
      '[aria-label*="spricht" i]',
      '[class*="speaking" i]',
    ],
    zoom: [
      '[aria-label*="active speaker" i]',
      '[aria-label*="speaking" i]',
      '[class*="active-speaker" i]',
      '[class*="activeSpeaker" i]',
      '[class*="is-speaking" i]',
    ],
  };
  const selectors = selectorsByProvider[providerName] || selectorsByProvider.google;
  for (const dom of doms) {
    for (const selector of selectors) {
      for (const node of Array.from(dom.querySelectorAll(selector))) {
        if (!visibleNode(node)) continue;
        const aria = node.getAttribute("aria-label") || node.getAttribute("title") || "";
        let speaker = cleanSpeakerName(aria);
        if (!speaker) {
          const nameNode = node.querySelector?.('[data-self-name], [data-participant-name], [class*="name" i], [class*="display" i]');
          speaker = cleanSpeakerName(nameNode?.textContent || "");
        }
        if (!speaker) {
          const lines = (node.innerText || node.textContent || "").split(/\n+/).map(compactText).filter(Boolean);
          speaker = cleanSpeakerName(lines.find(line => line.length <= 80) || "");
        }
        if (!speaker) continue;
        return {
          speaker,
          speaker_id: node.getAttribute("data-participant-id") || node.getAttribute("data-user-id") || "",
          source: "platform_active_speaker",
          confidence: 0.6,
          provider: providerName,
          ts: new Date().toISOString(),
        };
      }
    }
  }
  return null;
};

const knownChatKeys = new Set();
const knownTranscriptKeys = new Set();
let lastSpeakerKey = "";
let lastSpeakerProbeAt = 0;
let currentDirectSpeaker = "";

const installChatObservers = async () => {
  await page.exposeFunction("ctoxObservedChatMessage", (msg) => {
    if (!msg || !msg.text) return;
    const sender = msg.sender || "Participant";
    const key = `${sender}|${msg.text}`;
    if (knownChatKeys.has(key)) return;
    knownChatKeys.add(key);
    emit({ type: "chat", sender, text: msg.text, ts: msg.ts || new Date().toISOString() });
  }).catch(() => {});

  await page.evaluate((providerName) => {
    if (window.__ctoxChatObserverInstalled) return;
    window.__ctoxChatObserverInstalled = true;
    const compact = (value) => String(value || "").replace(/\s+/g, " ").trim();
    const send = (sender, text) => {
      text = compact(text);
      if (!text || /^messages? addressed to|^direct messages? are private/i.test(text)) return;
      window.ctoxObservedChatMessage?.({ sender: compact(sender) || "Participant", text, ts: new Date().toISOString() });
    };
    const scan = () => {
      try {
        const doms = [document];
        try {
          const iframe = document.querySelector("iframe#webclient");
          if (iframe?.contentDocument) doms.push(iframe.contentDocument);
        } catch {}
        if (providerName === "zoom") {
          for (const dom of doms) {
            const roots = Array.from(dom.querySelectorAll('[id^="chat-list-item-"], .new-chat-item__container, .new-chat-message__container, [role="listitem"][class*="chat"]'));
            for (const item of roots) {
              const sender = item.querySelector?.('[id^="chat-msg-author"], .new-chat-item__author, .chat-item__sender, [class*="sender" i]')?.textContent || "";
              const text = item.querySelector?.('[id^="chat-msg-text"], .new-chat-message__container__text, .chat-rtf-box__display, [class*="message__text" i]')?.textContent || "";
              if (text) send(sender, text);
            }
          }
        } else if (providerName === "microsoft") {
          for (const dom of doms) {
            for (const item of Array.from(dom.querySelectorAll('[data-tid="chat-pane-message"], [data-tid*="chat-message" i], [role="listitem"]'))) {
              const sender = item.querySelector?.('[data-tid="message-author-name"], [class*="author" i], [class*="sender" i]')?.textContent || "";
              const text = item.querySelector?.('[data-tid="message-body"], [class*="message-body" i], [class*="content" i]')?.textContent || item.textContent || "";
              send(sender, text);
            }
          }
        } else {
          for (const dom of doms) {
            for (const item of Array.from(dom.querySelectorAll('[data-message-id], [data-is-chat-message="true"], [role="listitem"]'))) {
              const sender = item.querySelector?.('[data-sender-name]')?.getAttribute?.("data-sender-name")
                || item.querySelector?.('[data-sender-name], [class*="sender" i], [class*="name" i]')?.textContent
                || "";
              const text = item.querySelector?.('[data-message-text], [class*="message-text" i]')?.textContent || item.textContent || "";
              send(sender, text);
            }
          }
        }
      } catch {}
    };
    const observer = new MutationObserver(scan);
    observer.observe(document.body, { childList: true, subtree: true, characterData: true });
    const iframe = document.querySelector("iframe#webclient");
    try {
      if (iframe?.contentDocument?.body) observer.observe(iframe.contentDocument.body, { childList: true, subtree: true, characterData: true });
    } catch {}
    scan();
  }, provider).catch(() => {});
};

await installChatObservers();

const chatPollInterval = setInterval(async () => {
  try {
    const messages = await page.evaluate(() => {
      __CHAT_SCRAPE_SCRIPT__
    });
    if (!Array.isArray(messages)) return;
    for (const msg of messages) {
      const key = `${msg.sender}|${msg.text}`;
      if (!knownChatKeys.has(key)) {
        knownChatKeys.add(key);
        emit({ type: "chat", sender: msg.sender, text: msg.text, ts: msg.ts || new Date().toISOString() });
      }
    }
  } catch {}
}, 2000);

const transcriptPollInterval = setInterval(async () => {
  try {
    if (provider === "microsoft") return;
    const entries = await page.evaluate((providerName) => {
      const visibleNode = (el) => {
        try {
          const rect = el.getBoundingClientRect();
          const style = window.getComputedStyle(el);
          return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
        } catch { return false; }
      };
      const queryAllDeep = (root, selector, limit = 700) => {
        const out = [];
        const visit = (scope) => {
          if (!scope || out.length >= limit) return;
          try {
            for (const node of Array.from(scope.querySelectorAll(selector))) {
              out.push(node);
              if (out.length >= limit) return;
            }
            for (const node of Array.from(scope.querySelectorAll("*"))) {
              if (out.length >= limit) return;
              if (node.shadowRoot) visit(node.shadowRoot);
            }
          } catch {}
        };
        visit(root);
        return out;
      };
      const compactText = (value) => String(value || "").replace(/\s+/g, " ").trim();
      const cleanSpeakerName = (value) => {
        let name = compactText(value)
          .replace(/\b(is speaking|speaking|active speaker|current speaker|spricht|aktueller sprecher)\b/ig, "")
          .replace(/[:|,-]+$/g, "")
          .trim();
        if (!name || name.length > 96) return "";
        return name;
      };
const parseCaptionNode = (node) => {
        const raw = compactText(node.innerText || node.textContent || "");
        if (!raw || raw.length < 2 || raw.length > 1200) return null;
        if (/^(chat|people|participants|teilnehmer|leave|verlassen)$/i.test(raw)) return null;
        const aria = node.getAttribute?.("aria-label") || "";
        const className = String(node.getAttribute?.("class") || "");
        const dataTid = String(node.getAttribute?.("data-tid") || "");
        const role = String(node.getAttribute?.("role") || "");
        const captionish = /(caption|closed-caption|transcript|subtitle|untertitel)/i.test(`${aria} ${className} ${dataTid}`);
        if (/messages? addressed to|direct messages? are private/i.test(raw)) return null;
        if (/^(new notification|notification)[:：]/i.test(raw)) return null;
        if (/your video stopped working|camera and plugging it back|use another device/i.test(raw)) return null;
        if (providerName === "microsoft" && /(status|alert|log)/i.test(role) && !captionish) return null;
        if (providerName === "microsoft" && !captionish) return null;
        let speaker = "";
        let text = raw;
        const labelled = aria.match(/(?:caption|transcript|live caption).*?(?:from|by)\s+(.+?)[,:-]\s*(.+)$/i);
        if (labelled) {
          speaker = cleanSpeakerName(labelled[1]);
          text = compactText(labelled[2]);
        }
        if (!speaker) {
          const lines = (node.innerText || node.textContent || "").split(/\n+/).map(compactText).filter(Boolean);
          if (lines.length >= 2 && lines[0].length <= 80 && !/[.!?]$/.test(lines[0])) {
            speaker = cleanSpeakerName(lines[0]);
            text = compactText(lines.slice(1).join(" "));
          }
        }
        if (!speaker) {
          const speakerNode = node.querySelector?.('[data-speaker-name], [class*="speaker" i], [class*="name" i]');
          speaker = cleanSpeakerName(speakerNode?.textContent || "");
          if (speaker && raw.startsWith(speaker)) text = compactText(raw.slice(speaker.length));
        }
        if (!text || text === speaker) return null;
        return {
          speaker: speaker || "unknown",
          text,
          source: "platform_caption",
          confidence: speaker ? 0.9 : 0.65,
          provider: providerName,
          ts: new Date().toISOString(),
        };
      };
      const doms = [document];
      try {
        const iframe = document.querySelector("iframe#webclient");
        if (iframe?.contentDocument) doms.push(iframe.contentDocument);
      } catch {}
      const selectorsByProvider = {
        google: ['[aria-live="polite"]', '[aria-live="assertive"]', '[role="status"]', '[jsname][data-ved]', '[class*="caption" i]'],
        microsoft: ['[data-tid*="closed-caption" i]', '[data-tid*="caption" i]', '[class*="caption" i]', '[class*="transcript" i]'],
        zoom: ['.live-transcription-subtitle', '.closed-caption', '[class*="caption" i]', '[class*="transcription" i]', '[aria-live="polite"]', '[aria-live="assertive"]'],
      };
      const selectors = selectorsByProvider[providerName] || selectorsByProvider.google;
      const entries = [];
      for (const dom of doms) {
        for (const selector of selectors) {
          for (const node of Array.from(dom.querySelectorAll(selector))) {
            if (!visibleNode(node)) continue;
            const entry = parseCaptionNode(node);
            if (entry) entries.push(entry);
          }
        }
      }
      return entries;
    }, provider);
    if (!Array.isArray(entries)) return;
    for (const entry of entries) {
      const key = `${entry.speaker}|${entry.text}`;
      if (knownTranscriptKeys.has(key)) continue;
      knownTranscriptKeys.add(key);
      await page.evaluate(({ text, speaker }) => {
        window.__ctoxTranscriptOverlayPush?.(text, speaker, "platform_caption");
      }, { text: entry.text, speaker: entry.speaker }).catch(() => {});
      emit({ type: "transcript_segment", ...entry });
    }
  } catch {}
}, 1500);

const speakerPollInterval = setInterval(async () => {
  try {
    const signal = await page.evaluate(({ providerName, botNameValue }) => {
      const visibleNode = (el) => {
        try {
          const rect = el.getBoundingClientRect();
          const style = window.getComputedStyle(el);
          return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
        } catch { return false; }
      };
      const compactText = (value) => String(value || "").replace(/\s+/g, " ").trim();
      const botLower = compactText(botNameValue || "").toLowerCase();
      const isBotOrUiName = (value) => {
        const v = compactText(value).toLowerCase();
        if (!v) return true;
        if (botLower && v.includes(botLower)) return true;
        return /^(you|me|ich|du|chat|people|participants|teilnehmer|personen|camera|microphone|leave|verlassen|more|caption|captions|notes)$/i.test(v);
      };
      const cleanSpeakerName = (value) => {
        let name = compactText(value)
          .replace(/\b(is speaking|speaking|active speaker|current speaker|spricht|aktueller sprecher|ist am sprechen|spricht gerade)\b/ig, "")
          .replace(/\b(muted|unmuted|stummgeschaltet|nicht stummgeschaltet|microphone|mikrofon|camera|kamera|pinned|angeheftet)\b/ig, "")
          .replace(/[:|,-]+$/g, "")
          .trim();
        if (!name || name.length > 96 || isBotOrUiName(name)) return "";
        return name;
      };
      const parseAriaSpeaker = (value) => {
        const raw = compactText(value);
        if (!raw) return "";
        const patterns = [
          /^(.+?)(?:,|\s)+(?:is speaking|speaking)$/i,
          /^(.+?)(?:,|\s)+(?:spricht|spricht gerade|ist am sprechen)$/i,
          /(?:active speaker|current speaker)[:,-]?\s*(.+)$/i,
          /(?:aktueller sprecher)[:,-]?\s*(.+)$/i,
        ];
        for (const pattern of patterns) {
          const match = raw.match(pattern);
          if (match) {
            const speaker = cleanSpeakerName(match[1]);
            if (speaker) return speaker;
          }
        }
        return cleanSpeakerName(raw);
      };
      const extractSpeakerFromNode = (node) => {
        const attrs = [
          node.getAttribute?.("aria-label"),
          node.getAttribute?.("title"),
          node.getAttribute?.("data-participant-name"),
          node.getAttribute?.("data-self-name"),
          node.getAttribute?.("data-display-name"),
        ].filter(Boolean);
        for (const attr of attrs) {
          const speaker = parseAriaSpeaker(attr);
          if (speaker) return speaker;
        }
        const nameNode = node.querySelector?.('[data-self-name], [data-participant-name], [data-display-name], [class*="name" i], [class*="display" i], [data-tid*="name" i]');
        const speakerFromName = cleanSpeakerName(nameNode?.textContent || nameNode?.getAttribute?.("aria-label") || "");
        if (speakerFromName) return speakerFromName;
        const lines = (node.innerText || node.textContent || "").split(/\n+/).map(compactText).filter(Boolean);
        for (const line of lines) {
          const speaker = cleanSpeakerName(line);
          if (speaker) return speaker;
        }
        return "";
      };
      const doms = [document];
      try {
        const iframe = document.querySelector("iframe#webclient");
        if (iframe?.contentDocument) doms.push(iframe.contentDocument);
      } catch {}
      const selectorsByProvider = {
        google: ['[data-speaking="true"]', '[aria-label*="speaking" i]', '[aria-label*="spricht" i]', '[class*="speaking" i]', '[class*="active-speaker" i]'],
        microsoft: [
          '[data-tid*="active-speaker" i]',
          '[data-tid*="speaking" i]',
          '[data-is-speaking="true"]',
          '[data-speaking="true"]',
          '[aria-label*="speaking" i]',
          '[aria-label*="spricht" i]',
          '[class*="speaking" i]',
          '[class*="activeSpeaker" i]',
          '[class*="active-speaker" i]',
        ],
        zoom: ['[aria-label*="active speaker" i]', '[aria-label*="speaking" i]', '[class*="active-speaker" i]', '[class*="activeSpeaker" i]', '[class*="is-speaking" i]'],
      };
      const selectors = selectorsByProvider[providerName] || selectorsByProvider.google;
      for (const dom of doms) {
        for (const selector of selectors) {
          for (const node of queryAllDeep(dom, selector, 500)) {
            if (!visibleNode(node)) continue;
            const tile = node.closest?.('[data-tid*="participant" i], [data-tid*="tile" i], [role="group"], [role="listitem"]') || node;
            const speaker = extractSpeakerFromNode(tile) || extractSpeakerFromNode(node);
            if (!speaker) continue;
            return {
              speaker,
              speaker_id: node.getAttribute("data-participant-id") || tile.getAttribute?.("data-participant-id") || node.getAttribute("data-user-id") || "",
              source: "platform_active_speaker",
              confidence: 0.75,
              provider: providerName,
              ts: new Date().toISOString(),
            };
          }
        }
      }
      if (providerName === "microsoft") {
        for (const dom of doms) {
          const candidates = queryAllDeep(dom, '[data-tid*="participant" i], [data-tid*="tile" i], [role="group"], [role="listitem"]', 700);
          for (const node of candidates) {
            if (!visibleNode(node)) continue;
            const text = `${node.getAttribute("data-tid") || ""} ${node.className || ""} ${node.getAttribute("aria-label") || ""}`;
            if (!/(speaking|active-speaker|activeSpeaker|spricht)/i.test(text)) continue;
            const speaker = extractSpeakerFromNode(node);
            if (!speaker) continue;
            return {
              speaker,
              speaker_id: node.getAttribute("data-participant-id") || node.getAttribute("data-user-id") || "",
              source: "platform_active_speaker",
              confidence: 0.65,
              provider: providerName,
              ts: new Date().toISOString(),
            };
          }
        }
        const probeRows = [];
        for (const dom of doms) {
          const bodyText = compactText((dom.body || dom.documentElement || dom).innerText || "");
          if (bodyText) probeRows.push(`body | visible-text | ${bodyText.slice(0, 420)}`);
          const candidates = queryAllDeep(dom, '[data-tid], [aria-label], [role="group"], [role="listitem"], [role="button"], [role="img"], button, video', 1200);
          for (const node of candidates) {
            if (!visibleNode(node)) continue;
            const tid = compactText(node.getAttribute?.("data-tid") || "");
            const aria = compactText(node.getAttribute?.("aria-label") || "");
            const title = compactText(node.getAttribute?.("title") || "");
            const klass = compactText(String(node.className || ""));
            const text = compactText((node.innerText || node.textContent || "").split(/\n+/).slice(0, 5).join(" / "));
            const nameish = [aria, title, text].join(" ");
            const blob = `${tid} ${aria} ${title} ${klass} ${text}`;
            const interesting = /(speaker|speaking|spricht|active|participant|tile|people|person|teilnehmer|microphone|mute|noise|rauschen|camera|kamera|video|name|author)/i.test(blob)
              || /\b[A-ZÄÖÜ][a-zäöüß]+ [A-ZÄÖÜ][a-zäöüß]+\b/.test(nameish);
            if (!interesting) continue;
            probeRows.push(`${tid || "no-tid"} | ${aria || title || "no-label"} | ${text || "no-text"}`.slice(0, 240));
            if (probeRows.length >= 16) break;
          }
          if (probeRows.length >= 16) break;
        }
        if (probeRows.length) {
          return {
            probe: probeRows.join(" || "),
            provider: providerName,
            ts: new Date().toISOString(),
          };
        }
      }
      return null;
    }, { providerName: provider, botNameValue: botName });
    if (signal?.probe) {
      const now = Date.now();
      if (now - lastSpeakerProbeAt > 10000) {
        lastSpeakerProbeAt = now;
        emit({ type: "speaker_probe", text: signal.probe, provider: signal.provider, ts: signal.ts });
      }
      return;
    }
    if (!signal?.speaker) return;
    const key = `${signal.speaker}|${signal.source}`;
    if (key === lastSpeakerKey) return;
    lastSpeakerKey = key;
    if (provider === "microsoft") currentDirectSpeaker = signal.speaker;
    emit({ type: "active_speaker", ...signal });
  } catch {}
}, 1000);

const participantPollInterval = setInterval(async () => {
  try {
    const count = await page.evaluate(() => {
      const buttons = Array.from(document.querySelectorAll("button"));
      for (const btn of buttons) {
        const text = btn.textContent || "";
        const match = text.match(/(\d+)/);
        if (match && (text.toLowerCase().includes("people") ||
            text.toLowerCase().includes("participant") ||
            btn.getAttribute("aria-label")?.toLowerCase().includes("people") ||
            btn.getAttribute("aria-label")?.toLowerCase().includes("participant"))) {
          return parseInt(match[1]);
        }
      }
      return null;
    });
    if (count !== null) {
      emit({ type: "participant_count", count });
      if (count <= 1) {
        await page.waitForTimeout(60000);
        const recheck = await page.evaluate(() => {
          const buttons = Array.from(document.querySelectorAll("button"));
          for (const btn of buttons) {
            const text = btn.textContent || "";
            const match = text.match(/(\d+)/);
            if (match && (text.toLowerCase().includes("people") || text.toLowerCase().includes("participant"))) {
              return parseInt(match[1]);
            }
          }
          return null;
        });
        if (recheck !== null && recheck <= 1) {
          window.ctoxMeetingEnd?.("alone_in_meeting");
        }
      }
    }
  } catch {}
}, 10000);

if (provider === "microsoft" && process.platform !== "darwin") {
  // --- Teams: ffmpeg + PulseAudio recording ---
  // Verify PulseAudio virtual output
  const { execSync } = await import("node:child_process");
  try {
    const sources = execSync("pactl list sources short 2>/dev/null").toString();
    if (!sources.includes("virtual_output.monitor")) {
      emit({ type: "warning", message: "virtual_output.monitor not found, attempting PulseAudio restart" });
      try {
        execSync("pulseaudio --kill 2>/dev/null || true");
        execSync("sleep 1");
        execSync("pulseaudio -D --exit-idle-time=-1 --log-level=info");
        execSync("sleep 2");
        execSync('pactl load-module module-null-sink sink_name=virtual_output sink_properties=device.description="Virtual_Output"');
        execSync("pactl set-default-sink virtual_output");
      } catch (e) { emit({ type: "warning", message: "PulseAudio restart failed: " + e.message }); }
    }
  } catch { /* pactl not available */ }

  // Start ffmpeg process
  const { spawn } = await import("node:child_process");
  const outputPath = path.join(tempDir, "recording.mp4");
  const display = process.env.DISPLAY || ":99";
  const runtimeDir = process.env.XDG_RUNTIME_DIR || (typeof process.getuid === "function" ? `/run/user/${process.getuid()}` : undefined);
  const ffmpegArgs = [
    "-y", "-loglevel", "warning",
    "-f", "x11grab", "-video_size", "1280x720", "-framerate", "8",
    "-draw_mouse", "0", "-i", `${display}+0,80`,
    "-f", "pulse", "-ac", "2", "-ar", "44100", "-i", "virtual_output.monitor",
    "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-pix_fmt", "yuv420p", "-crf", "32",
    "-g", "16", "-threads", "1",
    "-c:a", "aac", "-b:a", "96k", "-ar", "44100", "-ac", "1", "-strict", "experimental",
    "-vsync", "cfr", "-async", "1",
    "-movflags", "+faststart",
    outputPath,
  ];
  const ffmpeg = spawn("ffmpeg", ffmpegArgs, {
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env, ...(runtimeDir ? { XDG_RUNTIME_DIR: runtimeDir } : {}), DISPLAY: display },
  });
  ffmpeg.on("error", (err) => {
    emit({ type: "ffmpeg_error", text: err.message || String(err) });
    meetingEnded = true;
  });
  ffmpeg.stderr.on("data", (d) => {
    const s = d.toString();
    if (s.includes("error") || s.includes("Error")) emit({ type: "ffmpeg_error", text: s.substring(0, 200) });
  });
  ffmpeg.on("exit", (code) => {
    if (code !== 0 && code !== null) {
      emit({ type: "ffmpeg_exit", code });
      meetingEnded = true;
    }
  });

  // Teams realtime STT: stream raw 16 kHz PCM into Mistral's realtime
  // transcription API. This deliberately replaces the old file-segment path:
  // completed WAV chunks are batch STT and must not drive a live transcript UI.
  const realtimeScriptPath = path.join(tempDir, "mistral_realtime_stt.py");
  fs.writeFileSync(realtimeScriptPath, String.raw`import asyncio
import json
import os
import sys

try:
    from mistralai.client import Mistral
    from mistralai.client.models import AudioFormat
except Exception as exc:
    print(json.dumps({"type": "error", "message": "missing mistralai realtime SDK: " + str(exc)}), flush=True)
    sys.exit(4)

api_key = os.environ.get("CTOX_MISTRAL_API_KEY") or os.environ.get("MISTRAL_API_KEY")
if not api_key:
    print(json.dumps({"type": "error", "message": "missing CTOX_MISTRAL_API_KEY/MISTRAL_API_KEY"}), flush=True)
    sys.exit(3)

model = os.environ.get("CTOX_MISTRAL_REALTIME_STT_MODEL", "voxtral-mini-transcribe-realtime-2602")
delay_ms = int(os.environ.get("CTOX_MISTRAL_REALTIME_DELAY_MS", "2400"))
chunk_bytes = int(os.environ.get("CTOX_MISTRAL_REALTIME_PCM_CHUNK_BYTES", "15360"))
client = Mistral(api_key=api_key)
audio_eof = False

async def audio_stream():
    global audio_eof
    loop = asyncio.get_running_loop()
    while True:
        data = await loop.run_in_executor(None, sys.stdin.buffer.read, chunk_bytes)
        if not data:
            audio_eof = True
            break
        yield data

def event_text(event):
    for attr in ("text", "delta", "transcript"):
        value = getattr(event, attr, None)
        if isinstance(value, str) and value.strip():
            return value
    data = getattr(event, "data", None)
    if isinstance(data, dict):
        for key in ("text", "delta", "transcript"):
            value = data.get(key)
            if isinstance(value, str) and value.strip():
                return value
    return ""

def event_type(event):
    value = getattr(event, "type", None)
    return value if isinstance(value, str) else type(event).__name__

def event_error_message(event):
    error = getattr(event, "error", None)
    if error is None:
        return ""
    message = getattr(error, "message", None)
    code = getattr(error, "code", None)
    if isinstance(message, str) and message.strip():
        return f"{message} (code={code})" if code is not None else message
    return str(error)

async def main():
    attempt = 0
    while True:
        attempt += 1
        ready = False
        try:
            async for event in client.audio.realtime.transcribe_stream(
                audio_stream=audio_stream(),
                model=model,
                audio_format=AudioFormat(encoding="pcm_s16le", sample_rate=16000),
                target_streaming_delay_ms=delay_ms,
            ):
                kind = event_type(event)
                if kind == "session.created" and not ready:
                    ready = True
                    print(json.dumps({"type": "ready", "model": model, "delay_ms": delay_ms, "attempt": attempt}), flush=True)
                    continue
                if kind == "error" or type(event).__name__ == "RealtimeTranscriptionError":
                    print(json.dumps({"type": "error", "message": event_error_message(event) or repr(event), "attempt": attempt}), flush=True)
                    break
                text = event_text(event)
                if text:
                    print(json.dumps({"type": "delta", "text": text}, ensure_ascii=False), flush=True)
            if audio_eof:
                break
            await asyncio.sleep(min(2 * attempt, 10))
        except Exception as exc:
            print(json.dumps({"type": "error", "message": str(exc), "attempt": attempt}), flush=True)
            if audio_eof:
                break
            await asyncio.sleep(min(2 * attempt, 10))

asyncio.run(main())
`);
  const realtimePcm = spawn("ffmpeg", [
    "-y", "-loglevel", "warning",
    "-f", "pulse", "-ac", "1", "-ar", "16000", "-i", "virtual_output.monitor",
    "-vn", "-f", "s16le", "-acodec", "pcm_s16le", "-ac", "1", "-ar", "16000", "-"
  ], {
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, ...(runtimeDir ? { XDG_RUNTIME_DIR: runtimeDir } : {}), DISPLAY: display },
  });
  const realtimeStt = spawn("python3", [realtimeScriptPath], {
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env, PYTHONUNBUFFERED: "1" },
  });
  realtimePcm.stdout.pipe(realtimeStt.stdin);
  realtimePcm.stderr.on("data", (d) => {
    const s = d.toString();
    if (s.includes("error") || s.includes("Error")) emit({ type: "warning", message: "Teams realtime PCM ffmpeg: " + s.substring(0, 180) });
  });
  realtimePcm.on("error", (err) => emit({ type: "warning", message: "Teams realtime PCM ffmpeg failed: " + (err.message || String(err)) }));
  realtimeStt.on("error", (err) => emit({ type: "warning", message: "Mistral realtime STT failed: " + (err.message || String(err)) }));
  realtimeStt.stderr.on("data", (d) => {
    const s = d.toString();
    if (s.trim()) emit({ type: "browser_log", level: "warning", text: "[MISTRAL_REALTIME_STT] " + s.substring(0, 500) });
  });
  const realtimeRl = readline.createInterface({ input: realtimeStt.stdout });
  let realtimeBuffer = "";
  let realtimeFlushTimer = null;
  let realtimeReady = false;
  let realtimeDeltaSeen = false;
  let realtimeNoTextTimer = null;
  const flushRealtimeBuffer = () => {
    const text = realtimeBuffer.replace(/\s+/g, " ").trim();
    realtimeBuffer = "";
    realtimeFlushTimer = null;
    if (!text) return;
    const speaker = currentDirectSpeaker || "unknown";
    page.evaluate(({ text, speaker }) => {
      window.__ctoxTranscriptOverlayPush?.(text, speaker, "realtime_stt");
    }, { text, speaker }).catch(() => {});
    emit({
      type: "transcript_segment",
      speaker,
      source: "realtime_stt",
      confidence: currentDirectSpeaker ? 0.68 : 0.5,
      provider,
      text,
      ts: new Date().toISOString(),
    });
  };
  realtimeRl.on("line", (line) => {
    let msg = null;
    try { msg = JSON.parse(line); } catch { return; }
    if (msg.type === "ready") {
      realtimeReady = true;
      emit({ type: "status", status: "mistral_realtime_stt_ready", model: msg.model, delay_ms: msg.delay_ms });
      page.evaluate(() => {
        window.__ctoxTranscriptOverlaySetStatus?.("Realtime-STT verbunden - warte auf Sprache");
      }).catch(() => {});
      realtimeNoTextTimer = setTimeout(() => {
        if (realtimeReady && !realtimeDeltaSeen) {
          page.evaluate(() => {
            window.__ctoxTranscriptOverlaySetStatus?.("Realtime-STT verbunden, aber noch kein Transkript");
          }).catch(() => {});
          emit({ type: "warning", message: "Mistral realtime STT connected but produced no text yet; Teams captions are disabled" });
        }
      }, 12000);
      return;
    }
    if (msg.type === "error") {
      emit({ type: "warning", message: "Mistral realtime STT: " + msg.message });
      page.evaluate(({ message }) => {
        window.__ctoxTranscriptOverlaySetStatus?.(`Realtime-STT reconnect - ${message}`);
      }, { message: String(msg.message || "unknown").slice(0, 240) }).catch(() => {});
      return;
    }
    if (msg.type !== "delta" || !msg.text) return;
    realtimeDeltaSeen = true;
    if (realtimeNoTextTimer) {
      clearTimeout(realtimeNoTextTimer);
      realtimeNoTextTimer = null;
    }
    realtimeBuffer = `${realtimeBuffer}${msg.text}`;
    if (/[.!?。！？]\s*$/.test(realtimeBuffer.trim()) || realtimeBuffer.length >= 320) {
      if (realtimeFlushTimer) clearTimeout(realtimeFlushTimer);
      flushRealtimeBuffer();
    } else if (!realtimeFlushTimer) {
      realtimeFlushTimer = setTimeout(flushRealtimeBuffer, 2200);
    }
  });
  const terminateTeamsMediaChildren = () => {
    for (const child of [realtimeStt, realtimePcm, ffmpeg]) {
      try {
        if (child && !child.killed) child.kill("SIGTERM");
      } catch {}
    }
  };
  process.once("SIGTERM", () => {
    terminateTeamsMediaChildren();
    process.exit(143);
  });
  process.once("SIGINT", () => {
    terminateTeamsMediaChildren();
    process.exit(130);
  });
  const sendTeamsChatFromBranch = async (text) => {
    try { await page.keyboard.press("Escape"); } catch {}
    const scopes = () => [page, ...page.frames().filter((frame) => frame !== page.mainFrame())];
    const chatButtonMatchers = [/chat/i, /unterhaltung/i, /conversation/i, /messages/i, /nachrichten/i];
    for (const scope of scopes()) {
      for (const matcher of chatButtonMatchers) {
        try {
          const button = scope.getByRole("button", { name: matcher }).first();
          if (await button.isVisible({ timeout: 1000 }).catch(() => false)) {
            await button.click({ force: true });
            await page.waitForTimeout(1000);
            break;
          }
        } catch {}
      }
    }
    const inputSelectors = [
      '.chat-rtf-box__editor-outer [contenteditable="true"]',
      '.chat-rtf-box__display',
      '.tiptap.ProseMirror',
      '[contenteditable="true"][aria-label*="message" i]',
      '[contenteditable="true"][data-tid*="message" i]',
      '[data-tid="meeting-chat-input"] [contenteditable="true"]',
      'textarea[placeholder*="message" i]',
      'textarea[placeholder*="Type" i]',
      'textarea[aria-label*="message" i]',
      'textarea[aria-label*="Send" i]',
      'input[placeholder*="message" i]',
      'input[placeholder*="Type" i]',
      'input[aria-label*="message" i]',
      '[contenteditable="true"]',
      '[role="textbox"]',
    ];
    for (const scope of scopes()) {
      for (const selector of inputSelectors) {
        try {
          const input = scope.locator(selector).last();
          if (!(await input.isVisible({ timeout: 1000 }).catch(() => false))) continue;
          await input.click({ force: true });
          const editable = await input.evaluate((el) => el.isContentEditable || el.getAttribute("role") === "textbox").catch(() => false);
          if (editable) {
            await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A").catch(() => {});
            await page.keyboard.type(text, { delay: 10 });
          } else {
            try { await input.fill(text); }
            catch {
              await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A").catch(() => {});
              await page.keyboard.type(text, { delay: 10 });
            }
          }
          await page.keyboard.press("Enter");
          return true;
        } catch {}
      }
    }
    return false;
  };

  const handleTeamsCommandLine = async (line) => {
    try {
      const cmd = JSON.parse(line);
      if (cmd.action === "send_chat") {
        emit({ type: "command_received", action: "send_chat" });
        const sent = await sendTeamsChatFromBranch(cmd.text);
        emit(sent ? { type: "chat_sent", text: cmd.text } : { type: "chat_send_failed", text: cmd.text });
      } else if (cmd.action === "overlay_text") {
        emit({ type: "command_received", action: "overlay_text" });
        await page.evaluate(({ text, speaker }) => {
          window.__ctoxTranscriptOverlayPush?.(text, speaker);
        }, { text: cmd.text || "", speaker: cmd.speaker || "unknown" }).catch(() => {});
      }
    } catch (err) {
      emit({ type: "error", message: err.message });
    }
  };

  let teamsCommandFileOffset = 0;
  const teamsCommandFilePollInterval = setInterval(async () => {
    if (!commandFile) return;
    try {
      if (!fs.existsSync(commandFile)) return;
      const stat = fs.statSync(commandFile);
      if (stat.size < teamsCommandFileOffset) teamsCommandFileOffset = 0;
      if (stat.size === teamsCommandFileOffset) return;
      const fd = fs.openSync(commandFile, "r");
      try {
        const buffer = Buffer.alloc(stat.size - teamsCommandFileOffset);
        fs.readSync(fd, buffer, 0, buffer.length, teamsCommandFileOffset);
        teamsCommandFileOffset = stat.size;
        const lines = buffer.toString("utf8").split(/\r?\n/).filter(Boolean);
        for (const commandLine of lines) await handleTeamsCommandLine(commandLine);
      } finally {
        fs.closeSync(fd);
      }
    } catch (err) {
      emit({ type: "warning", message: "teams command file poll failed: " + err.message });
    }
  }, 500);

  // Teams participant detection (from reference) + audio silence via parec
  await page.evaluate(({ maxMs, inactivityMinutes }) => {
    // Max duration safety
    setTimeout(() => { console.log("Max duration reached"); window.ctoxMeetingEnd("max_duration"); }, maxMs);

    // Participant detection (after delay)
    setTimeout(() => {
      const interval = setInterval(() => {
        try {
          const regex = /\d+/;
          const contributors = Array.from(document.querySelectorAll('button[aria-label=People]') || [])
            .filter(x => regex.test(x?.textContent ?? ""))[0]?.textContent;
          const match = (!contributors) ? null : contributors.match(regex);
          if (match && Number(match[0]) >= 2) return;
          console.log("Bot is alone, ending meeting");
          clearInterval(interval);
          window.ctoxMeetingEnd("alone_in_meeting");
        } catch {}
      }, 5000);
    }, inactivityMinutes * 60 * 1000);
  }, { maxMs: maxDurationMs, inactivityMinutes: 1 });

  // Teams also monitors audio silence via parec (Node-side). Silence is useful
  // telemetry, but it must not end the meeting while participants are present:
  // real meetings often have quiet stretches.
  const monitorTeamsSilence = () => {
    let consecutiveSilent = 0;
    const checksNeeded = Math.ceil((2 * 60 * 1000) / 1000 / 5); // 2min inactivity
    const iv = setInterval(async () => {
      try {
        const out = execSync(
          "timeout 1 parec --device=virtual_output.monitor --format=s16le --rate=16000 --channels=1 2>/dev/null | " +
          "od -An -td2 -v | awk 'BEGIN{max=0} {for(i=1;i<=NF;i++) {val=($i<0)?-$i:$i; if(val>max) max=val}} END{print max}'"
        ).toString();
        const peak = parseInt(out.trim()) || 0;
        if (peak < 200) {
          consecutiveSilent++;
          if (consecutiveSilent >= checksNeeded) {
            emit({ type: "status", status: "audio_silence_detected" });
            consecutiveSilent = 0;
          }
        }
        else consecutiveSilent = 0;
      } catch {}
    }, 5000);
  };
  setTimeout(monitorTeamsSilence, 60000);

  // Wait for meeting end then stop ffmpeg
  const startTime = Date.now();
  while (!meetingEnded && (Date.now() - startTime) < maxDurationMs) {
    await new Promise(r => setTimeout(r, 1000));
  }

  // Graceful realtime STT + ffmpeg stop.
  clearInterval(teamsCommandFilePollInterval);
  if (realtimeFlushTimer) {
    clearTimeout(realtimeFlushTimer);
    flushRealtimeBuffer();
  }
  try { realtimeRl.close(); } catch {}
  try { realtimePcm.kill("SIGTERM"); } catch {}
  try { realtimeStt.stdin.end(); } catch {}
  try { realtimeStt.kill("SIGTERM"); } catch {}
  await Promise.all([
    new Promise(r => { realtimePcm.on("exit", r); setTimeout(() => { try { realtimePcm.kill("SIGKILL"); } catch {} r(); }, 5000); }),
    new Promise(r => { realtimeStt.on("exit", r); setTimeout(() => { try { realtimeStt.kill("SIGKILL"); } catch {} r(); }, 5000); }),
  ]);
  try { ffmpeg.stdin.write("q\n"); ffmpeg.stdin.end(); } catch { ffmpeg.kill("SIGTERM"); }
  await new Promise(r => { ffmpeg.on("exit", r); setTimeout(() => { try { ffmpeg.kill("SIGKILL"); } catch {} r(); }, 20000); });

  // Persist the screen recording artifact. Audio chunks are emitted by the
  // segmenter above; do not call browser-exposed functions from Node here.
  if (fs.existsSync(outputPath)) {
    emit({ type: "recording_artifact", path: outputPath, name: "screen-recording", extension: "mp4" });
    fs.unlinkSync(outputPath);
  }

} else {
  // --- Google Meet / Zoom: getDisplayMedia + MediaRecorder ---
  const primaryMimeType = "video/webm;codecs=\"h264,opus\"";
  const fallbackMimeType = "video/webm;codecs=\"vp9,opus\"";

  await page.evaluate(async ({ chunkMs, maxMs, primaryMimeType, fallbackMimeType, inactivityMinutes }) => {
    let inactivityParticipantTimeout;
    let inactivitySilenceTimeout;

    const sendChunk = async (chunk) => {
      let binary = "";
      const bytes = new Uint8Array(chunk);
      for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i]);
      await window.ctoxAudioChunk(btoa(binary));
    };

    // MediaDevices check
    if (!navigator.mediaDevices || !navigator.mediaDevices.getDisplayMedia) {
      console.error("[CTOX_AUDIO] getDisplayMedia not supported in this browser");
      return;
    }

    let stream;
    try {
      stream = await navigator.mediaDevices.getDisplayMedia({
        video: true,  // Required by spec — without it, Chrome refuses the request
        audio: {
          autoGainControl: false,
          channels: 2,
          channelCount: 2,
          echoCancellation: false,
          noiseSuppression: false,
        },
        preferCurrentTab: true,
        selfBrowserSurface: "include",
        systemAudio: "include",
      });
    } catch (err) {
      console.error("[CTOX_AUDIO] getDisplayMedia rejected:", err.name, err.message);
      console.error("[CTOX_AUDIO] On macOS this usually means the tab-capture dialog was dismissed or system audio capture is not permitted. Audio capture will be unavailable for this meeting.");
      return;
    }

    const audioTracks = stream.getAudioTracks();
    const videoTracks = stream.getVideoTracks();
    const hasAudio = audioTracks.length > 0;
    console.log("[CTOX_AUDIO] stream tracks: audio=" + audioTracks.length + " video=" + videoTracks.length);
    if (hasAudio) {
      const settings = audioTracks[0].getSettings();
      console.log("[CTOX_AUDIO] audio settings:", JSON.stringify(settings));
    }
    if (!hasAudio) {
      console.warn("[CTOX_AUDIO] No audio tracks captured — only video will be recorded. STT will receive video chunks (which are likely useless).");
    }
    // Keep video tracks so screen sharing/current-tab content is retained as
    // a reviewable meeting artifact. STT reads the same WebM container and
    // extracts usable audio where the backend supports it.

    let options = {};
    if (MediaRecorder.isTypeSupported(primaryMimeType)) {
      options = { mimeType: primaryMimeType };
    } else {
      console.warn("Using fallback codec:", fallbackMimeType);
      options = { mimeType: fallbackMimeType };
    }

    const recorder = new MediaRecorder(stream, { ...options });
    let chunkCount = 0;
    recorder.ondataavailable = async (event) => {
      if (!event.data.size) {
        console.warn("[CTOX_AUDIO] empty chunk received (count=" + chunkCount + ")");
        return;
      }
      chunkCount++;
      console.log("[CTOX_AUDIO] chunk #" + chunkCount + " size=" + event.data.size + " bytes");
      try { await sendChunk(await event.data.arrayBuffer()); }
      catch (e) { console.error("[CTOX_AUDIO] chunk send error:", e.message); }
    };
    recorder.onerror = (e) => { console.error("[CTOX_AUDIO] recorder error:", e); };
    recorder.start(chunkMs);
    console.log("[CTOX_AUDIO] MediaRecorder started, chunkMs=" + chunkMs);

    const stopRecording = () => {
      recorder.stop();
      stream.getTracks().forEach(t => t.stop());
      clearTimeout(maxTimeout);
      if (inactivityParticipantTimeout) clearTimeout(inactivityParticipantTimeout);
      if (inactivitySilenceTimeout) clearTimeout(inactivitySilenceTimeout);
      if (dismissInterval) clearInterval(dismissInterval);
      if (pageCheckInterval) clearInterval(pageCheckInterval);
      if (loneTestTimeout) clearTimeout(loneTestTimeout);
      window.ctoxMeetingEnd("recording_stopped");
    };

    // Max duration timeout
    const maxTimeout = setTimeout(stopRecording, maxMs);

    // --- Participant detection (Google Meet: 6-method from reference) ---
    let loneTestTimeout;
    let detectionFailures = 0;
    const maxFailures = 10;
    let loneActive = true;

    const detectLoneParticipant = () => {
      const re = /^[0-9]+$/;

      const getCount = () => {
        try {
          const btn = document.querySelector('button[aria-label^="People"]')
            || document.querySelector('button[aria-label*="People"]');
          if (btn) {
            const roots = [btn, btn.parentElement, btn.parentElement?.parentElement].filter(Boolean);
            for (const root of roots) {
              // Method 1: data-avatar-count
              const avatar = root.querySelector("[data-avatar-count]");
              if (avatar) { const c = Number(avatar.getAttribute("data-avatar-count")); if (!isNaN(c) && c > 0) return c; }
              // Method 2: badge div.egzc7c
              const badge = root.querySelector("div.egzc7c");
              if (badge) { const t = (badge.innerText || badge.textContent || "").trim(); if (t.length <= 3 && re.test(t)) { const c = Number(t); if (c > 0) return c; } }
            }
            // Method 3: search all divs near People button
            const mainRoot = btn.parentElement?.parentElement || btn;
            for (const div of Array.from(mainRoot.querySelectorAll("div"))) {
              const t = (div.innerText || div.textContent || "").trim();
              if (t.length > 0 && t.length <= 3 && re.test(t) && div.offsetParent !== null) {
                const c = Number(t); if (c > 0) return c;
              }
            }
          }
          return undefined;
        } catch { return undefined; }
      };

      const check = () => {
        if (!loneActive) return;
        let count;
        try {
          count = getCount();
          if (count === undefined) {
            detectionFailures++;
            if (detectionFailures >= maxFailures) { loneActive = false; return; }
            loneTestTimeout = setTimeout(check, 5000); return;
          }
          detectionFailures = 0;
          if (count < 2) { console.log("Bot is alone"); loneActive = false; stopRecording(); return; }
        } catch { detectionFailures++; }
        loneTestTimeout = setTimeout(check, 5000);
      };
      loneTestTimeout = setTimeout(check, 5000);
    };

    inactivityParticipantTimeout = setTimeout(detectLoneParticipant, inactivityMinutes * 60 * 1000);

    // --- Silence detection via AudioContext (from reference) ---
    const detectSilence = () => {
      if (!hasAudio) return;
      try {
        const ctx = new AudioContext();
        const source = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 256;
        source.connect(analyser);
        const data = new Uint8Array(analyser.frequencyBinCount);
        let silenceDuration = 0;
        const threshold = 10;
        const inactivityLimitMs = 2 * 60 * 1000; // 2 minutes silence = end
        let active = true;
        const monitor = () => {
          if (!active) return;
          analyser.getByteFrequencyData(data);
          const avg = data.reduce((a, b) => a + b) / data.length;
          if (avg < threshold) {
            silenceDuration += 100;
            if (silenceDuration >= inactivityLimitMs) { active = false; stopRecording(); return; }
          } else { silenceDuration = 0; }
          setTimeout(monitor, 100);
        };
        monitor();
      } catch (e) { console.error("Silence detection init failed:", e); }
    };

    inactivitySilenceTimeout = setTimeout(detectSilence, inactivityMinutes * 60 * 1000);

    // --- Dismiss modals perpetually (Google Meet "Got it", device notifications) ---
    let dismissInterval;
    dismissInterval = setInterval(() => {
      try {
        const buttons = document.querySelectorAll("button");
        Array.from(buttons).filter(b => b.offsetParent !== null && b.innerText?.includes("Got it"))
          .forEach(b => b.click());
        // Device notifications
        const bodyText = document.body.innerText;
        if (bodyText.includes("Microphone not found") || bodyText.includes("Camera not found")) {
          Array.from(document.querySelectorAll("button")).filter(btn => {
            const label = btn.getAttribute("aria-label");
            return label?.toLowerCase().includes("close") || label?.toLowerCase().includes("dismiss");
          }).forEach(btn => { if (btn.offsetParent !== null) btn.click(); });
        }
      } catch {}
    }, 2000);

    // --- Detect page navigation away from meeting ---
    let pageCheckInterval;
    pageCheckInterval = setInterval(() => {
      try {
        const url = window.location.href;
        if (!url.includes("meet.google.com") && !url.includes("zoom.us")) {
          console.warn("Page navigated away"); stopRecording();
        }
        const bt = document.body.innerText || "";
        if (bt.includes("You've been removed from the meeting") ||
            bt.includes("No one responded to your request")) {
          stopRecording();
        }
      } catch {}
    }, 10000);

  }, { chunkMs: chunkSeconds * 1000, maxMs: maxDurationMs, primaryMimeType, fallbackMimeType, inactivityMinutes: 1 });
}

const handleCommandLine = async (line) => {
  try {
    const cmd = JSON.parse(line);
    if (cmd.action === "send_chat") {
      emit({ type: "command_received", action: "send_chat" });
      let sent = false;
      try {
        sent = await page.evaluate(async (text) => {
        __SEND_CHAT_SCRIPT__
        }, cmd.text);
      } catch (err) {
        emit({ type: "warning", message: "browser-context chat send failed: " + err.message });
      }
      if (!sent) {
        sent = await sendChatViaPlaywrightFallback(cmd.text);
      }
      if (sent) {
        emit({ type: "chat_sent", text: cmd.text });
      } else {
        emit({ type: "chat_send_failed", text: cmd.text });
      }
    } else if (cmd.action === "overlay_text") {
      emit({ type: "command_received", action: "overlay_text" });
      await page.evaluate(({ text, speaker }) => {
        window.__ctoxTranscriptOverlayPush?.(text, speaker);
      }, { text: cmd.text || "", speaker: cmd.speaker || "unknown" }).catch(() => {});
    }
  } catch (err) {
    emit({ type: "error", message: err.message });
  }
};

const sendChatViaPlaywrightFallback = async (text) => {
  const scopes = () => [page, ...page.frames().filter((frame) => frame !== page.mainFrame())];
  const chatButtonMatchers = [
    /chat/i,
    /unterhaltung/i,
    /conversation/i,
    /messages/i,
    /nachrichten/i,
  ];
  for (const scope of scopes()) {
    for (const matcher of chatButtonMatchers) {
      try {
        const button = scope.getByRole("button", { name: matcher }).first();
        if (await button.isVisible({ timeout: 1000 }).catch(() => false)) {
          await button.click({ force: true });
          await page.waitForTimeout(1000);
          break;
        }
      } catch {}
    }
  }

  const inputSelectors = [
    '.chat-rtf-box__editor-outer [contenteditable="true"]',
    '.chat-rtf-box__display',
    '.tiptap.ProseMirror',
    '[contenteditable="true"][aria-label*="message" i]',
    '[contenteditable="true"][data-tid*="message" i]',
    '[data-tid="meeting-chat-input"] [contenteditable="true"]',
    'textarea[placeholder*="message" i]',
    'textarea[placeholder*="Type" i]',
    'textarea[aria-label*="message" i]',
    'textarea[aria-label*="Send" i]',
    'input[placeholder*="message" i]',
    'input[placeholder*="Type" i]',
    'input[aria-label*="message" i]',
    '[contenteditable="true"]',
    '[role="textbox"]',
  ];
  for (const scope of scopes()) {
    for (const selector of inputSelectors) {
      try {
        const input = scope.locator(selector).last();
        if (!(await input.isVisible({ timeout: 1000 }).catch(() => false))) continue;
        await input.click({ force: true });
        const editable = await input.evaluate((el) => el.isContentEditable || el.getAttribute("role") === "textbox").catch(() => false);
        if (editable) {
          await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A").catch(() => {});
          await page.keyboard.type(text, { delay: 10 });
        } else {
          try { await input.fill(text); }
          catch {
            await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A").catch(() => {});
            await page.keyboard.type(text, { delay: 10 });
          }
        }
        await page.keyboard.press("Enter");
        return true;
      } catch {}
    }
  }
  return false;
};

// --- Command handling ---
// stdin is useful for the CLI process that spawned the runner. commandFile is
// the durable cross-process bridge used by meeting_send_chat and @CTOX acks.
const rl = readline.createInterface({ input: process.stdin });
rl.on("line", handleCommandLine);

const commandFilePollInterval = setInterval(async () => {
  if (!commandFile) return;
  try {
    if (!fs.existsSync(commandFile)) return;
    const stat = fs.statSync(commandFile);
    if (stat.size < commandFileOffset) commandFileOffset = 0;
    if (stat.size === commandFileOffset) return;
    const fd = fs.openSync(commandFile, "r");
    try {
      const buffer = Buffer.alloc(stat.size - commandFileOffset);
      fs.readSync(fd, buffer, 0, buffer.length, commandFileOffset);
      commandFileOffset = stat.size;
      const lines = buffer.toString("utf8").split(/\r?\n/).filter(Boolean);
      for (const commandLine of lines) await handleCommandLine(commandLine);
    } finally {
      fs.closeSync(fd);
    }
  } catch (err) {
    emit({ type: "warning", message: "command file poll failed: " + err.message });
  }
}, 500);

// --- Wait for meeting to end ---
const startTime = Date.now();
while (!meetingEnded && (Date.now() - startTime) < maxDurationMs) {
  await new Promise(r => setTimeout(r, 1000));
}

clearInterval(chatPollInterval);
clearInterval(transcriptPollInterval);
clearInterval(speakerPollInterval);
clearInterval(participantPollInterval);
clearInterval(commandFilePollInterval);
if (stopZoomRemovalMonitor) stopZoomRemovalMonitor();
rl.close();

emit({ type: "finalized", temp_dir: tempDir, provider });
await browser.close();
process.exit(0);
"#;

/// Build a long-running Node.js Playwright script that:
/// 1. Joins the meeting as a guest
/// 2. Captures audio via getDisplayMedia + MediaRecorder
/// 3. Polls the meeting chat for new messages
/// 4. Emits JSON-lines events on stdout
/// 5. Accepts JSON commands on stdin (e.g., send_chat)
pub(crate) fn build_meeting_runner_script(config: &MeetingSessionConfig) -> Result<String> {
    let url = serde_json::to_string(&config.meeting_url)?;
    let bot_name = serde_json::to_string(&config.bot_name)?;
    let provider = config.provider.as_str();
    let chunk_seconds = config.audio_chunk_seconds;
    let max_duration_ms = config.max_duration_minutes * 60 * 1000;

    let join_script = match config.provider {
        MeetingProvider::GoogleMeet => build_google_meet_join_script(),
        MeetingProvider::MicrosoftTeams => build_teams_join_script(),
        MeetingProvider::Zoom => build_zoom_join_script(),
    };

    let chat_scrape_script = match config.provider {
        MeetingProvider::GoogleMeet => build_google_meet_chat_scraper(),
        MeetingProvider::MicrosoftTeams => build_teams_chat_scraper(),
        MeetingProvider::Zoom => build_zoom_chat_scraper(),
    };

    let send_chat_script = match config.provider {
        MeetingProvider::GoogleMeet => build_google_meet_chat_sender(),
        MeetingProvider::MicrosoftTeams => build_teams_chat_sender(),
        MeetingProvider::Zoom => build_zoom_chat_sender(),
    };

    // Use string replacement instead of format! to avoid brace escaping issues
    // with JavaScript code that heavily uses { and }.
    Ok(MEETING_RUNNER_TEMPLATE
        .replace("__MEETING_URL__", &url)
        .replace("__BOT_NAME__", &bot_name)
        .replace("__PROVIDER__", provider)
        .replace("__CHUNK_SECONDS__", &chunk_seconds.to_string())
        .replace("__MAX_DURATION_MS__", &max_duration_ms.to_string())
        .replace("__JOIN_SCRIPT__", join_script)
        .replace("__CHAT_SCRAPE_SCRIPT__", chat_scrape_script)
        .replace("__SEND_CHAT_SCRIPT__", send_chat_script))
}

// ---------------------------------------------------------------------------
// Provider-specific join scripts (injected into the Playwright runner)
// ---------------------------------------------------------------------------

fn build_google_meet_join_script() -> &'static str {
    r#"
// Google Meet join flow — transplanted from ScreenApp meeting-bot reference
try {
  const detectPage = async () => {
    const currentUrl = page.url();
    if (currentUrl.startsWith("https://accounts.google.com/")) {
      return "SIGN_IN_PAGE";
    }
    if (currentUrl.includes("workspace.google.com/products/meet")) {
      return "UNSUPPORTED_PAGE";
    }
    if (!currentUrl.includes("meet.google.com")) {
      return "UNSUPPORTED_PAGE";
    }
    return "GOOGLE_MEET_PAGE";
  };

  const initialPageStatus = await detectPage();
  if (initialPageStatus === "SIGN_IN_PAGE") {
    throw new Error("Meeting requires sign in");
  }
  if (initialPageStatus === "UNSUPPORTED_PAGE") {
    throw new Error("Google Meet redirected to unsupported page: " + page.url());
  }

  // 1. Dismiss "Continue without microphone and camera" (with retry)
  try {
    const retryClick = async (desc, fn, retries = 1, wait = 15000) => {
      for (let i = 0; i <= retries; i++) {
        try { await fn(); return; } catch (e) {
          if (i === retries) throw e;
          await page.waitForTimeout(wait);
        }
      }
    };
    await retryClick(
      "Continue without microphone and camera",
      async () => {
        const button = page.getByRole("button", {
          name: /Continue without microphone and camera|Ohne Mikrofon und Kamera fortfahren|Mikrofon und Kamera nicht verwenden/i
        }).first();
        await button.waitFor({ timeout: 30000 });
        await button.click();
      }
    );
  } catch { /* may not appear */ }

  // 2. Verify we are on a Google Meet page (not redirected to sign-in)
  const pageStatus = await detectPage();
  if (pageStatus === "SIGN_IN_PAGE") {
    throw new Error("Meeting requires sign in");
  }
  if (pageStatus === "UNSUPPORTED_PAGE") {
    throw new Error("Google Meet redirected to unsupported page: " + page.url());
  }

  // 3. Wait for name input and fill it (with retry)
  const nameInputSelectors = [
    'input[type="text"][aria-label="Your name"]',
    'input[type="text"][aria-label*="name" i]',
    'input[type="text"][aria-label*="Name" i]',
    'input[type="text"][placeholder*="name" i]',
    'input[type="text"][placeholder*="Name" i]',
    'input[type="text"]',
  ];
  let filledName = false;
  try {
    const retryWait = async (desc, fn, retries = 3, wait = 15000, onError) => {
      for (let i = 0; i <= retries; i++) {
        try { await fn(); return; } catch (e) {
          if (onError) try { await onError(); } catch {}
          if (i === retries) throw e;
          await page.waitForTimeout(wait);
        }
      }
    };
    await retryWait(
      "Name input field",
      async () => {
        for (const selector of nameInputSelectors) {
          const input = page.locator(selector).first();
          if (await input.isVisible({ timeout: 1000 }).catch(() => false)) return;
        }
        throw new Error("name input not visible");
      },
      3,
      15000
    );
  } catch (err) {
    emit({ type: "warning", message: "Name input not found: " + err.message });
  }

  for (const selector of nameInputSelectors) {
    try {
      const input = page.locator(selector).first();
      if (await input.isVisible({ timeout: 1000 }).catch(() => false)) {
        await input.fill(botName);
        filledName = true;
        break;
      }
    } catch {}
  }
  if (filledName) {
    await page.waitForTimeout(2000);
  }

  // 4. Click join button (Ask to join / Join now / Join anyway) — with retry
  {
    const possibleTexts = [
      "Ask to join",
      "Join now",
      "Join anyway",
      "Teilnahme anfragen",
      "Jetzt teilnehmen",
      "Teilnehmen",
      "Trotzdem teilnehmen",
    ];
    let buttonClicked = false;
    for (let attempt = 0; attempt <= 3 && !buttonClicked; attempt++) {
      for (const text of possibleTexts) {
        try {
          const btn = page.locator("button", { hasText: new RegExp(text, "i") }).first();
          if (await btn.isVisible({ timeout: 3000 }).catch(() => false)) {
            await btn.click({ timeout: 5000 });
            buttonClicked = true;
            break;
          }
        } catch { /* try next text */ }
      }
      if (!buttonClicked) await page.waitForTimeout(15000);
    }
    if (!buttonClicked) {
      emit({ type: "warning", message: "Could not find join button" });
    }
  }

  // 5. Wait at lobby — detect admission via People button + participant count
  //    Transplanted from reference: 6-method participant detection
  {
    const LOBBY_HOST_TEXT = "Please wait until a meeting host brings you";
    const REQUEST_DENIED = "Someone in the call denied your request to join";
    const REQUEST_TIMEOUT = "No one responded to your request to join the call";
    const wanderingTime = Math.min(10 * 60 * 1000, maxDurationMs);
    const lobbyResult = await new Promise((resolve) => {
      const timeout = setTimeout(() => { clearInterval(interval); resolve(false); }, wanderingTime);
      const interval = setInterval(async () => {
        try {
          // Check for denied/timeout
          const bodyText = await page.evaluate(() => document.body.innerText);
          if (bodyText.includes(REQUEST_DENIED)) {
            clearInterval(interval); clearTimeout(timeout); resolve(false); return;
          }
          if (bodyText.includes(REQUEST_TIMEOUT)) {
            clearInterval(interval); clearTimeout(timeout); resolve(false); return;
          }
          if (
            bodyText.includes(LOBBY_HOST_TEXT)
            || bodyText.includes("Jemand wird dich")
            || bodyText.includes("Jemand wird Sie")
            || bodyText.includes("Teilnahme anfragen")
            || bodyText.includes("Bitte warten")
          ) return; // still waiting

          // Check for People button or Leave call button
          const detected = await page.evaluate(() => {
            try {
              const peopleBtn = document.querySelector('button[aria-label^="People"]')
                || document.querySelector('button[aria-label*="People"]')
                || document.querySelector('button[aria-label*="Teilnehmer"]');
              const leaveBtn = document.querySelector('button[aria-label="Leave call"]')
                || document.querySelector('button[aria-label*="Verlassen"]')
                || document.querySelector('button[aria-label*="Anruf verlassen"]');

              if (!peopleBtn && !leaveBtn) return false;

              // Check participant count via data-avatar-count
              if (peopleBtn) {
                const roots = [peopleBtn, peopleBtn.parentElement, peopleBtn.parentElement?.parentElement].filter(Boolean);
                for (const root of roots) {
                  const avatar = root.querySelector("[data-avatar-count]");
                  if (avatar) {
                    const count = Number(avatar.getAttribute("data-avatar-count"));
                    if (!isNaN(count) && count >= 1) return true;
                  }
                  // Fallback: badge div with class egzc7c
                  const badge = root.querySelector("div.egzc7c");
                  if (badge) {
                    const text = (badge.innerText || badge.textContent || "").trim();
                    if (/^\d+$/.test(text) && Number(text) >= 1) return true;
                  }
                }
              }

              // Fallback: Leave call button present + no lobby text
              if (leaveBtn) {
                const bt = document.body.innerText || "";
                if (!bt.includes("Asking to join") && !bt.includes("You're the only one here")) {
                  return true;
                }
              }
              return false;
            } catch { return false; }
          });

          if (detected) {
            clearInterval(interval); clearTimeout(timeout); resolve(true);
          }
        } catch { /* retry next tick */ }
      }, 20000);
    });

    if (!lobbyResult) {
      const bodyText = await page.evaluate(() => document.body.innerText);
      emit({ type: "error", message: "Lobby admission failed", bodyText: (bodyText || "").substring(0, 500) });
    }
  }

  // 6. Dismiss "Got it" modals (loop until all gone)
  try {
    await page.waitForSelector('button:has-text("Got it")', { timeout: 15000 });
    let consecutiveNoChange = 0;
    let prevCount = -1;
    while (true) {
      const btns = await page.locator('button:visible', { hasText: "Got it" }).all();
      if (btns.length === 0) break;
      if (btns.length === prevCount) { consecutiveNoChange++; if (consecutiveNoChange >= 2) break; }
      else consecutiveNoChange = 0;
      prevCount = btns.length;
      for (const btn of btns) { try { await btn.click({ timeout: 5000 }); await page.waitForTimeout(2000); } catch {} }
      await page.waitForTimeout(2000);
    }
  } catch { /* modals may be missing */ }

  // 7. Dismiss device notifications (Microphone/Camera not found)
  try {
    const hasNotif = await page.evaluate(() =>
      document.body.innerText.includes("Microphone not found") ||
      document.body.innerText.includes("Camera not found") ||
      document.body.innerText.includes("Make sure your microphone is plugged in")
    );
    if (hasNotif) {
      await page.evaluate(() => {
        const allButtons = Array.from(document.querySelectorAll("button"));
        allButtons.filter(btn => {
          const label = btn.getAttribute("aria-label");
          const hasIcon = btn.querySelector("svg") !== null;
          return (label?.toLowerCase().includes("close") ||
                  label?.toLowerCase().includes("dismiss") ||
                  (hasIcon && btn.offsetParent !== null && btn.innerText === ""));
        }).forEach(btn => { if (btn.offsetParent !== null) btn.click(); });
      });
    }
  } catch {}
} catch (err) {
  emit({ type: "error", message: "Google Meet join error: " + err.message });
}
"#
}

fn build_teams_join_script() -> &'static str {
    r#"
// Microsoft Teams join flow — transplanted from ScreenApp meeting-bot reference
// Note: Teams uses ffmpeg+PulseAudio for recording, not getDisplayMedia.
// The browser is launched with --use-fake-ui-for-media-stream, --kiosk.
try {
  const teamsScopes = () => [page, ...page.frames().filter((frame) => frame !== page.mainFrame())];
  const warmUpTeamsMediaDevices = async () => {
    try {
      await page.evaluate(async () => {
        if (!navigator.mediaDevices?.getUserMedia) return false;
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
        stream.getTracks().forEach((track) => track.stop());
        return true;
      });
    } catch {}
  };
  const waitForTeamsPreJoinReadiness = async (timeoutMs = 45000) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      for (const scope of teamsScopes()) {
        const ready = await scope.evaluate(() => {
          const visible = (el) => {
            try {
              const rect = el.getBoundingClientRect();
              const style = window.getComputedStyle(el);
              return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
            } catch { return false; }
          };
          const hasName = Array.from(document.querySelectorAll("input, textarea, [contenteditable='true'], [role='textbox']"))
            .some((el) => visible(el) && (el.value || el.textContent || el.getAttribute("aria-label") || "").length >= 0);
          const hasJoin = Array.from(document.querySelectorAll("button"))
            .some((btn) => visible(btn) && /join|teilnehmen|beitreten|ask to join/i.test(btn.innerText || btn.textContent || btn.getAttribute("aria-label") || ""));
          return hasName || hasJoin;
        }).catch(() => false);
        if (ready) return true;
      }
      await page.waitForTimeout(1000);
    }
    return false;
  };

  await warmUpTeamsMediaDevices();

  // 1. Click "Join from browser" / "Continue on this browser"
  const joinButtonSelectors = [
    'button[aria-label="Join meeting from this browser"]',
    'button[aria-label="Continue on this browser"]',
    'button[aria-label="Join on this browser"]',
    'button:has-text("Continue on this browser")',
    'button:has-text("Join from browser")',
    'button:has-text("In diesem Browser fortfahren")',
    'button:has-text("In diesem Browser teilnehmen")',
  ];
  let browserBtnClicked = false;
  const visibleTextInputInScope = async (scope) => {
    try {
      return await scope.evaluate(() => {
        return Array.from(document.querySelectorAll("input")).some((el) => {
          const rect = el.getBoundingClientRect();
          const style = window.getComputedStyle(el);
          return rect.width > 80 && rect.height > 20 && style.visibility !== "hidden" && style.display !== "none";
        });
      });
    } catch { return false; }
  };
  let alreadyOnPrejoin = false;
  for (const scope of teamsScopes()) {
    if (await visibleTextInputInScope(scope)) { alreadyOnPrejoin = true; break; }
  }
  if (!alreadyOnPrejoin) {
    for (const sel of joinButtonSelectors) {
      try {
        const button = page.locator(sel).first();
        if (await button.isVisible({ timeout: 3000 }).catch(() => false)) {
          await button.click({ force: true });
          browserBtnClicked = true;
          break;
        }
      } catch { continue; }
    }
  }
  if (!browserBtnClicked && !alreadyOnPrejoin) {
    emit({ type: "warning", message: "Join from browser button not found, proceeding" });
  }
  await waitForTeamsPreJoinReadiness();

  // 2. Fill name input (Teams light meetings/localized variants)
  try {
    const nameInputSelectors = [
      'input[data-tid="prejoin-display-name-input"]',
      'input[placeholder*="name" i]',
      'input[placeholder*="Namen" i]',
      'input[type="text"]',
    ];
    let filledName = false;
    for (const scope of teamsScopes()) {
      for (const sel of nameInputSelectors) {
        const nameInput = scope.locator(sel).first();
        if (await nameInput.isVisible({ timeout: 2000 }).catch(() => false)) {
          await nameInput.fill(botName);
          filledName = true;
          break;
        }
      }
      if (filledName) break;
    }
    if (!filledName) {
      for (const scope of teamsScopes()) {
        const filled = await scope.evaluate((name) => {
          const candidates = Array.from(document.querySelectorAll("input, textarea, [contenteditable='true'], [role='textbox']"));
          for (const el of candidates) {
            const rect = el.getBoundingClientRect();
            const style = window.getComputedStyle(el);
            if (rect.width <= 80 || rect.height <= 20 || style.visibility === "hidden" || style.display === "none") continue;
            el.focus();
            if ("value" in el) el.value = name;
            else el.textContent = name;
            el.dispatchEvent(new Event("input", { bubbles: true }));
            el.dispatchEvent(new Event("change", { bubbles: true }));
            return true;
          }
          return false;
        }, botName).catch(() => false);
        if (filled) { filledName = true; break; }
      }
    }
    if (!filledName) {
      // Teams light-meetings sometimes renders the guest name control through
      // a localized/translated surface that Playwright cannot see as a normal
      // input. The prejoin layout is stable enough for this fallback.
      await page.mouse.click(640, 200);
      await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
      await page.keyboard.type(botName);
      filledName = true;
    }
    if (!filledName) throw new Error("no visible Teams display-name input");
    await page.waitForTimeout(1000);
  } catch (err) {
    emit({ type: "warning", message: "Teams name input not found: " + err.message });
  }

  // 2b. Select computer audio so the "Join now" button becomes enabled.
  try {
    const audioLabels = [
      /Computer audio/i,
      /Computeraudio/i,
      /Use computer audio/i,
    ];
    let selectedAudio = false;
    for (const scope of teamsScopes()) {
      for (const label of audioLabels) {
        const option = scope.getByText(label).first();
        if (await option.isVisible({ timeout: 2000 }).catch(() => false)) {
          await option.click({ force: true });
          selectedAudio = true;
          break;
        }
      }
      if (selectedAudio) break;
    }
    if (!selectedAudio) {
      for (const scope of teamsScopes()) {
        const clicked = await scope.evaluate(() => {
          const cards = Array.from(document.querySelectorAll("button, label, div"));
          for (const el of cards) {
            const text = (el.innerText || el.textContent || "").trim();
            if (!/Computer audio|Computeraudio|Use computer audio/i.test(text)) continue;
            const rect = el.getBoundingClientRect();
            const style = window.getComputedStyle(el);
            if (rect.width <= 50 || rect.height <= 20 || style.visibility === "hidden" || style.display === "none") continue;
            el.click();
            return true;
          }
          return false;
        }).catch(() => false);
        if (clicked) {
          selectedAudio = true;
          break;
        }
      }
    }
    if (selectedAudio) await page.waitForTimeout(1000);
  } catch (err) {
    emit({ type: "warning", message: "Teams audio option not selected: " + err.message });
  }

  // 3. Keep the injected transcript camera on, mute microphone
  try {
    await page.waitForTimeout(2000);
    // Microphone mute
    const micSelectors = [
      'input[data-tid="toggle-mute"]:not([checked])',
      'input[type="checkbox"][title*="Mute mic" i]',
      'input[role="switch"][data-tid="toggle-mute"]',
      'button[aria-label*="Mute microphone" i]',
      'button[aria-label*="Mute mic" i]',
    ];
    for (const sel of micSelectors) {
      const el = page.locator(sel).first();
      if (await el.isVisible({ timeout: 2000 }).catch(() => false)) {
        await el.click(); await page.waitForTimeout(500); break;
      }
    }
  } catch { /* device toggles best-effort */ }

  // 4. Click Join button (with retry)
  {
    const possibleTexts = ["Join now", "Join", "Ask to join", "Join meeting", "Jetzt teilnehmen", "Teilnehmen"];
    let joinClicked = false;
    for (let attempt = 0; attempt <= 3 && !joinClicked; attempt++) {
      for (const scope of teamsScopes()) {
        for (const text of possibleTexts) {
          try {
            const btn = scope.getByRole("button", { name: new RegExp(text, "i") });
            if (await btn.isVisible({ timeout: 2000 }).catch(() => false)) {
              await btn.click(); joinClicked = true; break;
            }
          } catch {}
        }
        if (joinClicked) break;
      }
      if (!joinClicked) {
        for (const scope of teamsScopes()) {
          const clicked = await scope.evaluate((labels) => {
            const buttons = Array.from(document.querySelectorAll("button"));
            for (const button of buttons) {
              const text = (button.innerText || button.getAttribute("aria-label") || "").trim();
              if (!labels.some((label) => text.toLowerCase().includes(label.toLowerCase()))) continue;
              button.click();
              return true;
            }
            return false;
          }, possibleTexts).catch(() => false);
          if (clicked) {
            joinClicked = true;
            break;
          }
        }
      }
      if (!joinClicked && attempt === 1) {
        await page.mouse.click(1060, 600);
        joinClicked = true;
      }
      if (!joinClicked) await page.waitForTimeout(15000);
    }
    if (!joinClicked) emit({ type: "warning", message: "Could not find Teams join button" });
    await page.keyboard.press(process.platform === "darwin" ? "Meta+Shift+M" : "Control+Shift+M").catch(() => {});
  }

  // 5. Wait for lobby admission (Leave button appears)
  {
    const DENIED_TEXT = "Sorry, but you were denied access to the meeting";
    const wanderingTime = Math.min(10 * 60 * 1000, maxDurationMs);
    try {
      const leaveBtn = page.getByRole("button", { name: /Leave|Verlassen/i });
      await leaveBtn.waitFor({ timeout: wanderingTime });
    } catch {
      const bodyText = await page.evaluate(() => document.body.innerText);
      const denied = (bodyText || "").includes(DENIED_TEXT);
      emit({ type: "error", message: "Teams lobby failed", denied, bodyText: (bodyText || "").substring(0, 500) });
    }
  }

  // 6. Dismiss Close buttons (notifications/device checks) — with loop
  try {
    await page.waitForSelector('button[aria-label=Close]', { timeout: 5000 });
    await page.click('button[aria-label=Close]', { timeout: 2000 });
  } catch {}
  try {
    let prevCount = -1;
    let noChange = 0;
    while (true) {
      const btns = await page.locator('button[title="Close"]:visible').all();
      if (btns.length === 0) break;
      if (btns.length === prevCount) { noChange++; if (noChange >= 2) break; }
      else noChange = 0;
      prevCount = btns.length;
      for (const btn of btns) { try { await btn.click({ timeout: 5000 }); await page.waitForTimeout(2000); } catch {} }
      await page.waitForTimeout(2000);
    }
  } catch {}

  // 7. Wait for audio to stabilize before recording
  await page.waitForTimeout(5000);
} catch (err) {
  emit({ type: "error", message: "Teams join error: " + err.message });
}
"#
}

fn build_zoom_join_script() -> &'static str {
    r##"
// Zoom join flow — direct web client flow with Vexa-style readiness checks.
try {
  // Block .exe downloads
  await page.route("**/*.exe", (route) => {
    emit({ type: "status", status: "blocked_exe_download", url: route.request().url() });
  });

  // 1. Accept cookies
  try {
    await page.waitForTimeout(3000);
    const acceptCookies = page.locator("button", { hasText: /Accept Cookies|Cookies akzeptieren|Alle Cookies akzeptieren/i }).first();
    await acceptCookies.waitFor({ timeout: 5000 });
    await acceptCookies.click({ force: true });
  } catch { /* may not appear */ }

  if (!page.url().includes("/wc/")) {
    await page.goto(buildZoomWebClientUrl(meetingUrl), { waitUntil: "domcontentloaded", timeout: 60000 });
  }
  await page.waitForTimeout(5000);

  const text = (await page.evaluate(() => document.body?.innerText || "").catch(() => "")).toLowerCase();
  if (/sign in to join|only authenticated users|not authorized|meeting authentication/i.test(text)) {
    emit({ type: "join_failed", provider, reason: "zoom_requires_authentication", bodyText: text.substring(0, 500) });
  }

  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const allow = page.getByRole("button", { name: /Allow/i }).first();
      if (await allow.isVisible({ timeout: 1500 }).catch(() => false)) await allow.click({ force: true });
    } catch {}
  }

  const zoomScopes = () => [page, ...page.frames().filter((frame) => frame !== page.mainFrame())];
  const findVisibleLocator = async (selectors, timeout = 1500) => {
    for (const scope of zoomScopes()) {
      for (const selector of selectors) {
        try {
          const locator = scope.locator(selector).first();
          if (await locator.isVisible({ timeout }).catch(() => false)) return locator;
        } catch {}
      }
    }
    return null;
  };

  const nameInput = await findVisibleLocator([
    "#input-for-name",
    'input[aria-label*="name" i]',
    'input[placeholder*="name" i]',
    'input[type="text"]',
  ], 30000);
  if (nameInput) {
    await nameInput.click({ force: true });
    await nameInput.fill("").catch(() => {});
    await page.keyboard.type(botName, { delay: 30 });
  } else {
    emit({ type: "warning", message: "Zoom name input not found" });
  }

  const passcodeInput = await findVisibleLocator([
    "#input-for-pwd",
    'input[type="password"]',
    'input[placeholder*="passcode" i]',
    'input[aria-label*="passcode" i]',
  ], 1000);
  if (passcodeInput && !new URL(buildZoomWebClientUrl(meetingUrl)).searchParams.get("pwd")) {
    emit({ type: "warning", message: "Zoom passcode field visible but no passcode was present in the meeting URL" });
  }

  const joinSelectors = [
    "button.preview-join-button",
    'button[type="submit"]',
    'button:has-text("Join")',
    'button:has-text("Beitreten")',
  ];
  let joinedClicked = false;
  for (let attempt = 0; attempt < 5 && !joinedClicked; attempt++) {
    const joinButton = await findVisibleLocator(joinSelectors, 2000);
    if (joinButton) {
      try {
        await joinButton.waitFor({ state: "visible", timeout: 5000 });
        const enabled = await joinButton.evaluate((btn) => !btn.disabled && !btn.classList.contains("disabled")).catch(() => true);
        if (enabled) {
          joinedClicked = await joinButton.evaluate((btn) => { btn.click(); return true; }).catch(() => false);
          if (!joinedClicked) {
            await joinButton.click({ force: true, timeout: 3000 });
            joinedClicked = true;
          }
        }
      } catch {}
    }
    if (!joinedClicked) await page.waitForTimeout(2000);
  }
  if (!joinedClicked) emit({ type: "error", message: "Zoom join button not found or disabled" });

  await page.waitForTimeout(5000);

  const previewStopVideo = await findVisibleLocator([
    'button[aria-label*="Stop Video" i]',
    'button[title*="Stop Video" i]',
  ], 1000);
  if (previewStopVideo) await previewStopVideo.click({ force: true }).catch(() => {});

  const wanderingTime = Math.min(10 * 60 * 1000, maxDurationMs);
  const deadline = Date.now() + wanderingTime;
  while (Date.now() < deadline) {
    const state = await page.evaluate(() => {
      const body = (document.body?.innerText || "").toLowerCase();
      const leave = document.querySelector('button[aria-label*="Leave" i], button[title*="Leave" i]');
      const footer = document.querySelector("#wc-footer");
      return {
        admitted: Boolean(leave) || /participants?|teilnehmer/i.test(footer?.textContent || ""),
        waiting: /please wait|waiting room|let you in soon|we've let them know|host has not joined/i.test(body),
        denied: /removed|denied|no one responded|meeting has ended/i.test(body),
        bodyText: body.substring(0, 500),
      };
    });
    if (state.admitted) break;
    if (state.denied) {
      emit({ type: "error", message: "Zoom lobby failed", bodyText: state.bodyText });
      break;
    }
    if (state.waiting) emit({ type: "status", status: "waiting_lobby", provider });
    await page.waitForTimeout(3000);
  }

  await dismissZoomPopups(page, 30000);
} catch (err) {
  emit({ type: "error", message: "Zoom join error: " + err.message });
}
"##
}

// ---------------------------------------------------------------------------
// Provider-specific chat scraping (runs inside page.evaluate)
// ---------------------------------------------------------------------------

fn build_google_meet_chat_scraper() -> &'static str {
    r#"
      const messages = [];
      // Google Meet chat messages
      const chatMsgs = document.querySelectorAll('[data-message-id]');
      for (const el of chatMsgs) {
        const senderEl = el.querySelector('[data-sender-name]');
        const textEl = el.querySelector('[data-message-text]');
        const sender = senderEl?.getAttribute('data-sender-name') || senderEl?.textContent?.trim() || 'Unknown';
        const text = textEl?.textContent?.trim() || el.textContent?.trim() || '';
        if (text) messages.push({ sender, text, ts: new Date().toISOString() });
      }
      // Fallback: try aria-label based selectors
      if (messages.length === 0) {
        const items = document.querySelectorAll('[data-is-chat-message="true"], [jsname] [role="listitem"]');
        for (const item of items) {
          const clone = item.cloneNode(true);
          clone.querySelectorAll('[aria-label*="Pinned" i], [aria-label*="pin" i], button, svg').forEach((node) => node.remove());
          const lines = (clone.innerText || clone.textContent || '').split(/\n+/).map((line) => line.trim()).filter(Boolean);
          const sender = lines.length > 1 && lines[0].length <= 80 ? lines[0] : 'Participant';
          const text = lines.length > 1 ? lines.slice(1).join(' ') : lines.join(' ');
          if (text) messages.push({ sender, text, ts: new Date().toISOString() });
        }
      }
      return messages;
    "#
}

fn build_teams_chat_scraper() -> &'static str {
    r#"
      const messages = [];
      const chatItems = document.querySelectorAll('[data-tid="chat-pane-message"], [role="listitem"]');
      for (const el of chatItems) {
        const senderEl = el.querySelector('[data-tid="message-author-name"]') || el.querySelector('.ui-chat__message__author') || el.querySelector('[class*="author" i], [class*="sender" i]');
        const textEl = el.querySelector('[data-tid="message-body"]') || el.querySelector('.ui-chat__message__content') || el.querySelector('[class*="message-body" i], [class*="content" i]');
        const sender = senderEl?.textContent?.trim() || 'Unknown';
        const text = textEl?.textContent?.trim() || el.textContent?.trim() || '';
        if (text) messages.push({ sender, text, ts: new Date().toISOString() });
      }
      return messages;
    "#
}

fn build_zoom_chat_scraper() -> &'static str {
    r#"
      const messages = [];
      let dom = document;
      const iframe = document.querySelector('iframe#webclient');
      if (iframe && iframe.contentDocument) dom = iframe.contentDocument;

      // Zoom Web Client 2025 chat structure:
      // - List items have id starting with "chat-list-item-" or class "new-chat-item__container"
      // - Each item contains author name (in [id^="chat-msg-author"] or .new-chat-item__author)
      // - And message text (in [id^="chat-msg-text"] or .new-chat-message__container__text)
      //
      // Strategy: find each top-level chat item, then extract sender + text
      // from its known sub-structure rather than relying on text scraping.

      // 1. Find all chat list items (top-level only, not nested duplicates)
      const itemSelectors = [
        '[id^="chat-list-item-"]',                    // primary: stable id
        '.new-chat-item__container',                  // primary: known class
        '.new-chat-message__container',               // current web client message node
        '[class*="chat-item-container"]',             // fallback
        '[role="listitem"][class*="chat"]',           // generic fallback
      ];

      let items = [];
      for (const sel of itemSelectors) {
        const found = Array.from(dom.querySelectorAll(sel));
        if (found.length > 0) { items = found; break; }
      }

      // Filter to top-level only: drop items that contain another item
      items = items.filter(el => !items.some(other => other !== el && el.contains(other)));

      for (const item of items) {
        // Skip system messages (UI hints like "Messages addressed to...")
        const itemId = item.id || '';
        if (itemId.includes('system') || itemId.includes('hint')) continue;
        const cls = item.className || '';
        if (typeof cls === 'string' && (cls.includes('system') || cls.includes('hint'))) continue;

        // Extract sender — try several strategies
        let sender = '';
        const authorSelectors = [
          '[id^="chat-msg-author"]',
          '.new-chat-item__author',
          '.chat-item__sender',
          '[class*="chat-message__author"]',
          '[class*="sender-name"]',
        ];
        for (const sel of authorSelectors) {
          const el = item.querySelector(sel);
          if (el && el.textContent?.trim()) { sender = el.textContent.trim(); break; }
        }
        // Fallback: aria-label like "Chat message from John Doe to everyone"
        if (!sender) {
          const aria = item.getAttribute('aria-label') || '';
          const m = aria.match(/from\s+(.+?)(?:\s+to\s+|\s+at\s+|$)/i);
          if (m) sender = m[1].trim();
        }
        if (!sender) sender = 'Participant';

        // Strip role suffix like "John Doe (Host)" → "John Doe"
        sender = sender.replace(/\s*\((Host|Co-host|Me)\)\s*$/i, '').trim();

        // Extract text — try known containers
        let text = '';
        const textSelectors = [
          '[id^="chat-msg-text"]',
          '.new-chat-message__container__text',
          '.chat-rtf-box__display',
          '.new-chat-message__content',
          '.chat-message__text-content',
          '[class*="chat-message__body"]',
          '[class*="chat-msg-text"]',
        ];
        for (const sel of textSelectors) {
          const el = item.querySelector(sel);
          if (el && el.textContent?.trim()) { text = el.textContent.trim(); break; }
        }
        // Fallback: full item text minus the sender prefix
        if (!text && item.textContent) {
          text = item.textContent.trim();
          // Remove leading "SENDER To EVERYONE: " or similar
          if (sender) {
            const prefix = new RegExp("^" + sender.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\\\$&") + "\\s*(?:To\\s+\\S+\\s*)?[:\\s-]+", "i");
            text = text.replace(prefix, '').trim();
          }
        }

        // Skip empty or pure-UI-hint messages
        if (!text) continue;
        if (/^Messages? addressed to/i.test(text)) continue;
        if (/^Direct messages? are private/i.test(text)) continue;

        messages.push({ sender, text, ts: new Date().toISOString() });
      }
      return messages;
    "#
}

// ---------------------------------------------------------------------------
// Provider-specific chat sending (runs inside page.evaluate)
// ---------------------------------------------------------------------------

fn build_google_meet_chat_sender() -> &'static str {
    r#"
      // Open chat panel if not visible
      try {
        const chatBtn = document.querySelector('button[aria-label*="Chat" i], button[aria-label*="chat" i]');
        if (chatBtn) chatBtn.click();
        await new Promise(r => setTimeout(r, 1000));
      } catch {}
      // The actual send uses the Playwright keyboard fallback so Meet receives trusted key events.
      return false;
    "#
}

fn build_teams_chat_sender() -> &'static str {
    r#"
      // Open chat panel if not visible
      try {
        const chatBtn = document.querySelector('button[aria-label*="Chat" i]');
        if (chatBtn) chatBtn.click();
        await new Promise(r => setTimeout(r, 1000));
      } catch {}
      // The actual send uses the Playwright keyboard fallback so Teams receives trusted key events.
      return false;
    "#
}

fn build_zoom_chat_sender() -> &'static str {
    r#"
      let dom = document;
      const iframe = document.querySelector('iframe#webclient');
      if (iframe && iframe.contentDocument) dom = iframe.contentDocument;
      // Open chat panel if needed
      try {
        const chatBtn = Array.from(dom.querySelectorAll('button')).find(b =>
          b.textContent?.toLowerCase().includes('chat') || b.getAttribute('aria-label')?.toLowerCase().includes('chat'));
        if (chatBtn) chatBtn.click();
        await new Promise(r => setTimeout(r, 1000));
      } catch {}
      // The actual send uses the Playwright keyboard fallback so Zoom receives trusted key events.
      return false;
    "#
}

// ---------------------------------------------------------------------------
// STT transcription (reuses Jami pattern)
// ---------------------------------------------------------------------------

pub(crate) fn transcribe_audio_chunk(
    root: &Path,
    audio_path: &Path,
    stt_model: &str,
) -> Result<String> {
    crate::communication::gateway::transcribe_audio_file(root, audio_path, stt_model)
}

// ---------------------------------------------------------------------------
// Meeting invitation detection & scheduling
// ---------------------------------------------------------------------------

/// Known meeting URL patterns.
const MEETING_URL_PATTERNS: &[&str] = &[
    "meet.google.com/",
    "teams.microsoft.com/l/meetup-join/",
    "teams.microsoft.com/meet/",
    "teams.live.com/meet/",
    "zoom.us/j/",
    "zoom.us/my/",
    "zoom.com/j/",
];

/// Extract meeting URLs from a text body (email body, chat message, etc.).
pub(crate) fn extract_meeting_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    // Simple URL extraction: find https:// followed by known meeting domains
    for word in text.split_whitespace() {
        // Strip common surrounding punctuation/markup
        let candidate = word.trim_matches(|c: char| {
            c == '<'
                || c == '>'
                || c == '"'
                || c == '\''
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == ','
                || c == ';'
                || c == '.'
        });
        if !candidate.starts_with("https://") && !candidate.starts_with("http://") {
            continue;
        }
        let candidate = candidate.replace("&amp;", "&");
        let lower = candidate.to_lowercase();
        if MEETING_URL_PATTERNS.iter().any(|pat| lower.contains(pat)) {
            // Normalize: strip trailing fragments and tracking params
            let clean = candidate
                .split('#')
                .next()
                .unwrap_or(candidate.as_str())
                .to_string();
            if !urls.contains(&clean) {
                urls.push(clean);
            }
        }
    }
    urls
}

/// Parse a meeting time from ICS-style DTSTART or common date patterns.
/// Returns ISO 8601 timestamp if found.
pub(crate) fn extract_meeting_time_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "DTSTART").and_then(|value| parse_ics_datetime(&value))
}

fn extract_meeting_end_time_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "DTEND").and_then(|value| parse_ics_datetime(&value))
}

fn extract_meeting_uid_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "UID")
}

fn extract_meeting_sequence_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "SEQUENCE")
}

fn extract_meeting_summary_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "SUMMARY")
}

fn extract_meeting_method_from_text(text: &str) -> Option<String> {
    extract_ics_value(text, "METHOD").map(|value| value.to_ascii_uppercase())
}

fn extract_ics_value(text: &str, field: &str) -> Option<String> {
    let needle = field.to_ascii_uppercase();
    for line in unfold_ics_lines(text) {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper == needle
            || upper.starts_with(&(needle.clone() + ":"))
            || upper.starts_with(&(needle.clone() + ";"))
        {
            let value = trimmed.rsplit_once(':')?.1.trim();
            if !value.is_empty() {
                return Some(unescape_ics_text(value));
            }
        }
    }
    None
}

fn unfold_ics_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        if raw.starts_with(' ') || raw.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(raw.trim_start());
            }
        } else {
            lines.push(raw.trim_end_matches('\r').to_string());
        }
    }
    lines
}

fn unescape_ics_text(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn parse_ics_datetime(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 15 {
        return None;
    }
    let year = value.get(0..4)?;
    let month = value.get(4..6)?;
    let day = value.get(6..8)?;
    let hour = value.get(9..11)?;
    let min = value.get(11..13)?;
    let sec = value.get(13..15)?;
    let tz = if value.ends_with('Z') { "Z" } else { "" };
    Some(format!("{year}-{month}-{day}T{hour}:{min}:{sec}{tz}"))
}

/// Detect whether an email body indicates a meeting cancellation.
pub(crate) fn is_meeting_cancellation(subject: &str, body: &str) -> bool {
    let lower_subject = subject.to_lowercase();
    let lower_body = body.to_lowercase();
    // Common cancellation indicators
    lower_subject.contains("canceled")
        || lower_subject.contains("cancelled")
        || lower_subject.contains("abgesagt")
        || lower_body.contains("has been canceled")
        || lower_body.contains("has been cancelled")
        || lower_body.contains("meeting wurde abgesagt")
        || lower_body.contains("method:cancel")
        || extract_meeting_method_from_text(body).as_deref() == Some("CANCEL")
}

/// Detect whether an email body indicates a meeting time change.
pub(crate) fn is_meeting_update(subject: &str, body: &str) -> bool {
    let lower_subject = subject.to_lowercase();
    let lower_body = body.to_lowercase();
    lower_subject.contains("updated")
        || lower_subject.contains("rescheduled")
        || lower_subject.contains("aktualisiert")
        || lower_subject.contains("verschoben")
        || lower_body.contains("has been updated")
        || lower_body.contains("has been rescheduled")
        || lower_body.contains("new time:")
        || lower_body.contains("neue zeit:")
}

/// Build a cron expression for a one-shot meeting at a specific ISO timestamp.
/// Cron format: minute hour day month *
/// The schedule module fires when `next_run_at <= now`, so we set it directly.
pub(crate) fn cron_for_meeting_time(iso_time: &str) -> Option<String> {
    // Parse "2026-04-15T14:00:00Z" → minute=0 hour=14 day=15 month=4
    if iso_time.len() < 16 {
        return None;
    }
    let month: u32 = iso_time[5..7].parse().ok()?;
    let day: u32 = iso_time[8..10].parse().ok()?;
    let hour: u32 = iso_time[11..13].parse().ok()?;
    let min: u32 = iso_time[14..16].parse().ok()?;
    Some(format!("{min} {hour} {day} {month} *"))
}

/// Unique schedule name for a meeting URL (stable across updates).
pub(crate) fn meeting_schedule_name(meeting_url: &str) -> String {
    format!("meeting-join:{}", stable_digest(meeting_url))
}

fn meeting_schedule_name_for_invitation(meeting_url: &str, uid: Option<&str>) -> String {
    if let Some(stable_key) = uid.map(str::trim).filter(|value| !value.is_empty()) {
        return format!("meeting-join:{}", stable_digest(stable_key));
    }
    meeting_schedule_name(meeting_url)
}

fn meeting_join_time(meeting_time_iso: &str) -> String {
    DateTime::parse_from_rfc3339(meeting_time_iso)
        .map(|dt| (dt.with_timezone(&Utc) - Duration::minutes(1)).to_rfc3339())
        .unwrap_or_else(|_| meeting_time_iso.to_string())
}

/// Schedule a meeting join via the CTOX schedule system.
/// Creates or updates a scheduled task that will fire at the meeting start time.
pub(crate) fn schedule_meeting_join(
    root: &Path,
    meeting_url: &str,
    meeting_time_iso: &str,
    bot_name: &str,
) -> Result<Value> {
    schedule_meeting_join_with_metadata(
        root,
        meeting_url,
        meeting_time_iso,
        bot_name,
        None,
        None,
        None,
    )
}

fn schedule_meeting_join_with_metadata(
    root: &Path,
    meeting_url: &str,
    meeting_time_iso: &str,
    bot_name: &str,
    uid: Option<&str>,
    sequence: Option<&str>,
    summary: Option<&str>,
) -> Result<Value> {
    let provider =
        MeetingProvider::detect(meeting_url).context("cannot detect meeting provider from URL")?;
    let join_time_iso = meeting_join_time(meeting_time_iso);
    let cron_expr = cron_for_meeting_time(&join_time_iso)
        .context("cannot parse meeting time into cron expression")?;
    let schedule_name = meeting_schedule_name_for_invitation(meeting_url, uid);
    let thread_key = format!("meeting:{}", provider.as_str());

    let payload = json!({
        "url": meeting_url,
        "bot_name": bot_name,
        "provider": provider.as_str(),
        "meeting_time": meeting_time_iso,
        "join_time": join_time_iso,
        "uid": uid,
        "sequence": sequence,
        "summary": summary,
    });
    let prompt = format!(
        "CTOX_MEETING_JOIN: {payload}\n\
         Join the {provider} meeting at {url} as \"{bot_name}\". \
         Capture audio transcript and monitor chat. \
         If no other participants join within 15 minutes, leave the meeting. \
         After the meeting ends, summarize the transcript and create tickets. \
         Create durable knowledge only when the meeting produced reusable operational procedure; \
         durable knowledge must be a Skillbook/Runbook/Runbook-Item, not a ticket_knowledge_entries note.",
        provider = provider.as_str(),
        url = meeting_url,
        bot_name = bot_name,
    );

    let request = crate::mission::schedule::ScheduleEnsureRequest {
        name: schedule_name.clone(),
        cron_expr,
        prompt,
        thread_key,
        skill: Some("system-onboarding".to_string()),
    };
    let task = crate::mission::schedule::ensure_task(root, request)?;

    // Also persist the meeting details for the join logic
    let sessions_dir = meeting_sessions_dir(root);
    fs::create_dir_all(&sessions_dir)?;
    let session_file = sessions_dir.join(format!("{}.json", schedule_name));
    let session_meta = json!({
        "schedule_name": schedule_name,
        "meeting_url": meeting_url,
        "meeting_time": meeting_time_iso,
        "join_time": join_time_iso,
        "provider": provider.as_str(),
        "bot_name": bot_name,
        "uid": uid,
        "sequence": sequence,
        "summary": summary,
        "status": "scheduled",
        "created_at": now_iso_string(),
    });
    fs::write(&session_file, serde_json::to_string_pretty(&session_meta)?)?;

    Ok(json!({
        "ok": true,
        "action": "scheduled",
        "schedule_name": schedule_name,
        "task_id": task.task_id,
        "meeting_url": meeting_url,
        "meeting_time": meeting_time_iso,
        "join_time": join_time_iso,
        "provider": provider.as_str(),
        "cron_expr": task.cron_expr,
        "next_run_at": task.next_run_at,
    }))
}

/// Cancel a scheduled meeting join.
pub(crate) fn cancel_meeting_join(root: &Path, meeting_url: &str) -> Result<Value> {
    cancel_meeting_join_with_uid(root, meeting_url, None)
}

fn cancel_meeting_join_with_uid(
    root: &Path,
    meeting_url: &str,
    uid: Option<&str>,
) -> Result<Value> {
    let schedule_name = meeting_schedule_name_for_invitation(meeting_url, uid);
    let session_file = meeting_sessions_dir(root).join(format!("{schedule_name}.json"));
    let provider_thread_key = MeetingProvider::detect(meeting_url)
        .map(|provider| format!("meeting:{}", provider.as_str()));

    // Remove matching scheduled tasks by persisted metadata instead of relying
    // on reconstructing the schedule module's task-id derivation.
    if let Ok(tasks) = crate::mission::schedule::list_tasks(root) {
        for task in tasks {
            let provider_matches = provider_thread_key
                .as_deref()
                .map(|thread_key| task.thread_key == thread_key)
                .unwrap_or(true);
            if task.name == schedule_name && provider_matches {
                if let Err(err) = crate::mission::schedule::remove_task(root, &task.task_id) {
                    eprintln!(
                        "note: could not remove scheduled task {}: {err}",
                        task.task_id
                    );
                }
            }
        }
    }

    // Update session file
    if session_file.exists() {
        let _ = fs::write(
            &session_file,
            serde_json::to_string_pretty(&json!({
                "schedule_name": schedule_name,
                "meeting_url": meeting_url,
                "status": "cancelled",
                "cancelled_at": now_iso_string(),
            }))?,
        );
    }

    Ok(json!({
        "ok": true,
        "action": "cancelled",
        "schedule_name": schedule_name,
        "meeting_url": meeting_url,
    }))
}

/// Process an inbound email to detect meeting invitations, updates, or cancellations.
/// Returns a summary of actions taken.
pub(crate) fn process_email_for_meetings(
    root: &Path,
    subject: &str,
    body: &str,
    bot_name: &str,
) -> Result<Value> {
    let urls = extract_meeting_urls(body);
    if urls.is_empty() {
        return Ok(json!({"ok": true, "action": "none", "reason": "no meeting URLs found"}));
    }

    let mut results = Vec::new();
    let uid = extract_meeting_uid_from_text(body);
    let sequence = extract_meeting_sequence_from_text(body);
    let summary = extract_meeting_summary_from_text(body);
    let meeting_time = extract_meeting_time_from_text(body);
    let meeting_end_time = extract_meeting_end_time_from_text(body);

    for url in &urls {
        if is_meeting_cancellation(subject, body) {
            let result = cancel_meeting_join_with_uid(root, url, uid.as_deref())?;
            results.push(result);
            continue;
        }

        if let Some(ref time) = meeting_time {
            if is_meeting_update(subject, body) {
                // Update = cancel old + schedule new
                let _ = cancel_meeting_join_with_uid(root, url, uid.as_deref());
            }
            let mut result = schedule_meeting_join_with_metadata(
                root,
                url,
                time,
                bot_name,
                uid.as_deref(),
                sequence.as_deref(),
                summary.as_deref(),
            )?;
            if let Some(object) = result.as_object_mut() {
                object.insert("uid".to_string(), json!(uid));
                object.insert("sequence".to_string(), json!(sequence));
                object.insert("summary".to_string(), json!(summary));
                object.insert("meeting_end_time".to_string(), json!(meeting_end_time));
            }
            results.push(result);
        } else {
            results.push(json!({
                "ok": false,
                "meeting_url": url,
                "uid": uid,
                "reason": "meeting URL found but no start time detected",
            }));
        }
    }

    let successful_results = results
        .iter()
        .filter(|result| {
            result
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let action = if results.is_empty() {
        "none"
    } else if successful_results > 0 {
        "processed"
    } else {
        "needs_review"
    };

    Ok(json!({
        "ok": true,
        "action": action,
        "results": results,
    }))
}

// ---------------------------------------------------------------------------
// Meeting join timeout & lifecycle
// ---------------------------------------------------------------------------

// Meeting join behavior constants are now embedded directly in the
// Playwright runner templates (participant detection + silence detection).

/// Update the Playwright runner template's participant monitoring to include
/// the empty-meeting timeout. This is already embedded in the runner script
/// via the participant count monitoring interval — when count <= 1 for 60s
/// the meeting ends. The 15-minute initial empty timeout is handled by
/// injecting it into the runner script.
///
/// Build the runner script with integrated timeout/inactivity detection.
/// Since the transplanted recording template already includes participant-detection,
/// silence-detection, and max-duration timeouts from the reference implementation,
/// this is now a thin wrapper around `build_meeting_runner_script`.
pub(crate) fn build_meeting_runner_script_with_timeout(
    config: &MeetingSessionConfig,
) -> Result<String> {
    // The recording template now natively handles:
    //  - Google/Zoom: participant count detection (6 methods), AudioContext silence detection
    //  - Teams: `parec` audio silence detection, participant count via aria-label
    //  - All providers: max duration timeout
    build_meeting_runner_script(config)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn meeting_sessions_dir(root: &Path) -> PathBuf {
    communication_runtime::channel_dir(root, "meeting").join("sessions")
}

fn legacy_meeting_sessions_dir(root: &Path) -> PathBuf {
    root.join("runtime").join("meeting_sessions")
}

fn existing_meeting_session_dirs(root: &Path) -> Vec<PathBuf> {
    let canonical = meeting_sessions_dir(root);
    let legacy = legacy_meeting_sessions_dir(root);
    let mut dirs = Vec::new();
    if canonical.exists() {
        dirs.push(canonical.clone());
    }
    if legacy.exists() && legacy != canonical {
        dirs.push(legacy);
    }
    dirs
}

fn meeting_session_file(root: &Path, session_id: &str) -> PathBuf {
    let canonical = meeting_sessions_dir(root).join(format!("{session_id}.json"));
    if canonical.exists() {
        return canonical;
    }
    let legacy = legacy_meeting_sessions_dir(root).join(format!("{session_id}.json"));
    if legacy.exists() {
        return legacy;
    }
    canonical
}

fn meeting_session_artifact_file(root: &Path, session_id: &str, suffix: &str) -> PathBuf {
    let canonical = meeting_sessions_dir(root).join(format!("{session_id}{suffix}"));
    if canonical.exists() {
        return canonical;
    }
    let legacy = legacy_meeting_sessions_dir(root).join(format!("{session_id}{suffix}"));
    if legacy.exists() {
        return legacy;
    }
    canonical
}

/// Load the final artifacts of a meeting session: the session metadata
/// JSON (speakers, duration, provider, etc.), the full STT transcript,
/// and the captured chat log. Returns a structured JSON suitable for
/// direct emission by the `ctox meeting transcript` CLI and consumption
/// by the agent-runtime `meeting_get_transcript` tool.
///
/// Missing transcript/chatlog files are returned as empty strings rather
/// than errors — an active session may have metadata persisted before
/// finalize_meeting_session has written the text files.
pub(crate) fn load_meeting_transcript(root: &Path, session_id: &str) -> Result<Value> {
    let session_path = meeting_session_file(root, session_id);
    let session: Value = if session_path.exists() {
        let contents = fs::read_to_string(&session_path)
            .with_context(|| format!("read meeting session {}", session_path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("parse meeting session JSON at {}", session_path.display()))?
    } else {
        anyhow::bail!("no meeting session found with id {session_id}");
    };

    let transcript_path = meeting_session_artifact_file(root, session_id, "-transcript.txt");
    let chatlog_path = meeting_session_artifact_file(root, session_id, "-chatlog.txt");

    let transcript = fs::read_to_string(&transcript_path).unwrap_or_default();
    let chatlog = fs::read_to_string(&chatlog_path).unwrap_or_default();

    Ok(json!({
        "ok": true,
        "session_id": session_id,
        "session": session,
        "transcript": transcript,
        "chatlog": chatlog,
        "transcript_path": transcript_path.display().to_string(),
        "chatlog_path": chatlog_path.display().to_string(),
    }))
}

fn now_iso_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    // Simple ISO-8601 without external deps
    let (year, month, day, hour, min, sec) = epoch_to_datetime(secs);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z",
        millis = nanos / 1_000_000
    )
}

fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn epoch_to_datetime(epoch_secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = epoch_secs / 86400;
    let time = epoch_secs % 86400;
    let hour = time / 3600;
    let min = (time % 3600) / 60;
    let sec = time % 60;

    // Simplified date calculation (accurate for 1970-2099)
    let mut y = 1970u64;
    let mut remaining_days = days;
    loop {
        let days_in_year =
            if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
                366
            } else {
                365
            };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    while m < 12 && remaining_days >= month_days[m] {
        remaining_days -= month_days[m];
        m += 1;
    }
    (y, (m + 1) as u64, remaining_days + 1, hour, min, sec)
}

fn stable_digest(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-meeting-{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp root");
        root
    }

    #[test]
    fn detect_meeting_provider_from_url() {
        assert_eq!(
            MeetingProvider::detect("https://meet.google.com/abc-defg-hij"),
            Some(MeetingProvider::GoogleMeet)
        );
        assert_eq!(
            MeetingProvider::detect("https://teams.microsoft.com/l/meetup-join/abc"),
            Some(MeetingProvider::MicrosoftTeams)
        );
        assert_eq!(
            MeetingProvider::detect("https://us04web.zoom.us/j/123456"),
            Some(MeetingProvider::Zoom)
        );
        assert_eq!(MeetingProvider::detect("https://example.com"), None);
    }

    #[test]
    fn mention_detection_is_case_insensitive() {
        assert!(MeetingSession::is_mention("Hey @CTOX what do you think?"));
        assert!(MeetingSession::is_mention("@ctox please summarize"));
        assert!(MeetingSession::is_mention("Hello @Ctox!"));
        assert!(!MeetingSession::is_mention("This is a normal message"));
    }

    #[test]
    fn mention_detection_respects_word_boundary() {
        // Bot's full display name in chat headers should still match
        assert!(MeetingSession::is_mention(
            "@INF Yoda Notetaker hat geantwortet"
        ));
        assert!(MeetingSession::is_mention("@ctox notetaker"));
        // But unrelated tokens that contain "ctox" as a substring should not
        assert!(!MeetingSession::is_mention("@ctoxbar"));
        assert!(!MeetingSession::is_mention("@ctoxology is a fake word"));
        // Embedded inside a longer URL or token also shouldn't match
        assert!(!MeetingSession::is_mention(
            "https://example.com/@ctoxapi/foo"
        ));
    }

    #[test]
    fn engine_reachable_check_returns_false_for_closed_port() {
        assert!(!check_engine_reachable(Path::new(
            "/definitely/not/a/ctox/root"
        )));
    }

    #[test]
    fn linux_meeting_runner_uses_xvfb_when_display_is_missing() {
        assert_eq!(
            should_wrap_browser_runner_with_xvfb(None),
            cfg!(target_os = "linux")
        );
        assert!(!should_wrap_browser_runner_with_xvfb(Some(OsStr::new(
            ":99"
        ))));
        assert_eq!(
            should_wrap_browser_runner_with_xvfb(Some(OsStr::new(""))),
            cfg!(target_os = "linux")
        );
        assert!(MEETING_XVFB_SERVER_ARGS.contains("1920x1080x24"));
    }

    #[test]
    fn stale_running_session_is_marked_ended() {
        let root = temp_root("stale-session");
        let session_path = root.join("session.json");
        let session = json!({
            "session_id": "meeting-test",
            "status": "active",
            "pid": 999999999u64,
            "ended_at": null,
        });
        fs::write(
            &session_path,
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        let reconciled = reconcile_stale_running_session(&session_path, session);

        assert_eq!(reconciled["status"], "ended");
        assert_eq!(reconciled["end_reason"], "process_not_running");
        assert!(reconciled["ended_at"].as_str().is_some());
        let persisted: Value =
            serde_json::from_str(&fs::read_to_string(&session_path).unwrap()).unwrap();
        assert_eq!(persisted["status"], "ended");
    }

    #[test]
    fn self_loop_protection_filters_bot_messages() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp"),
            meeting_url: "https://zoom.us/j/123".to_string(),
            provider: MeetingProvider::Zoom,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            stt_model: String::new(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);

        // Sender contains bot name → own message
        assert!(session.is_own_message("INF Yoda Notetaker", "hello"));
        assert!(session.is_own_message("INF Yoda Notetaker (Host)", "hello"));
        assert!(session.is_own_message("ctox notetaker", "hello"));

        // Real participants are not filtered
        assert!(!session.is_own_message("Michael Welsch", "@CTOX hello"));
        assert!(!session.is_own_message("Participant", "regular message"));

        // Addressing the bot by name is an inbound mention, not self-loop.
        assert!(!session.is_own_message("Participant", "INF Yoda Notetaker: I heard you"));
        assert!(MeetingSession::is_mention(
            "INF Yoda Notetaker: I heard you"
        ));
        assert!(MeetingSession::is_mention(
            "INF\u{00a0}Yoda\u{00a0}Notetaker: bitte pruefen"
        ));
        session
            .outbound_chat_texts
            .push("CTOX Test: Chat-Bridge aktiv.".to_string());
        assert!(session.is_own_message("You", "You20:35CNCTOX Test: Chat-Bridge aktiv."));
        assert!(session.is_own_message("Participant", "You20:35CNCTOX Test: Chat-Bridge aktiv."));

        // Text mentions bot name but doesn't start with it (real user message)
        assert!(!session.is_own_message("Michael", "Hey @INF Yoda Notetaker what do you think?"));
    }

    #[test]
    fn session_roundtrip_json() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://meet.google.com/abc".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            stt_model: String::new(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let session = MeetingSession::new(&config);
        let json = session.to_json();
        assert_eq!(json["provider"], "google");
        assert_eq!(json["status"], "joining");
        assert_eq!(json["bot_name"], "INF Yoda Notetaker");
        assert_eq!(json["transcript_segment_count"], 0);
        assert_eq!(json["speaker_signal_count"], 0);
    }

    #[test]
    fn meeting_runtime_defaults_to_voxtral_4b_stt() {
        let config = MeetingSessionConfig::from_runtime(
            Path::new("/tmp"),
            "https://meet.google.com/abc-defg-hij",
            &BTreeMap::new(),
        )
        .expect("meeting config");
        assert_eq!(config.stt_model, DEFAULT_MEETING_STT_MODEL);
    }

    #[test]
    fn meeting_runtime_replaces_legacy_stt_models_with_voxtral_4b() {
        let mut runtime = BTreeMap::new();
        runtime.insert("CTOX_STT_MODEL".to_string(), "legacy-stt-model".to_string());
        let config = MeetingSessionConfig::from_runtime(
            Path::new("/tmp"),
            "https://zoom.us/j/123456789",
            &runtime,
        )
        .expect("meeting config");
        assert_eq!(config.stt_model, DEFAULT_MEETING_STT_MODEL);
    }

    #[test]
    fn transcript_segments_render_speaker_source_and_confidence() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://meet.google.com/abc".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            stt_model: DEFAULT_MEETING_STT_MODEL.to_string(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);
        session.push_platform_transcript(TranscriptSegment {
            timestamp: "2026-04-28T12:00:00Z".to_string(),
            speaker_display: "Alice".to_string(),
            speaker_id: Some("alice-platform-id".to_string()),
            source: "platform_caption".to_string(),
            confidence: 0.9,
            text: "The rollout is blocked by permissions.".to_string(),
        });

        let transcript = session.full_transcript();
        assert!(transcript.contains("Alice: The rollout is blocked"));
        assert!(transcript.contains("source=platform_caption"));
        assert!(transcript.contains("confidence=0.90"));

        let snapshot = session_transcript_snapshot(&session.to_json(), 12);
        assert!(snapshot.contains("Alice: The rollout is blocked"));
    }

    #[test]
    fn stt_segments_use_active_speaker_when_available() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://zoom.us/j/123456".to_string(),
            provider: MeetingProvider::Zoom,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            stt_model: DEFAULT_MEETING_STT_MODEL.to_string(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);
        let signal = SpeakerSignal {
            timestamp: "2026-04-28T12:00:00Z".to_string(),
            speaker_display: "Bob".to_string(),
            speaker_id: None,
            source: "platform_active_speaker".to_string(),
            confidence: 0.6,
        };
        session.push_stt_transcript(
            "I can take the deployment ticket.".to_string(),
            Some(&signal),
        );
        assert_eq!(
            session.transcript_segments[0].source,
            "stt_with_active_speaker"
        );
        assert_eq!(session.transcript_segments[0].speaker_display, "Bob");
        assert!(session.full_transcript().contains("Bob: I can take"));
    }

    #[test]
    fn full_transcript_keeps_stt_when_platform_captions_exist() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://teams.microsoft.com/meet/demo".to_string(),
            provider: MeetingProvider::MicrosoftTeams,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            stt_model: DEFAULT_MEETING_STT_MODEL.to_string(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);
        session.push_platform_transcript(TranscriptSegment {
            timestamp: "2026-04-28T12:00:00Z".to_string(),
            speaker_display: "Teams".to_string(),
            speaker_id: None,
            source: "platform_caption".to_string(),
            confidence: 0.8,
            text: "Screen shared.".to_string(),
        });
        session.push_stt_transcript(
            "A participant described the Salesforce assignment workflow.".to_string(),
            None,
        );

        let transcript = session.full_transcript();
        assert!(transcript.contains("Teams: Screen shared."));
        assert!(transcript.contains("Salesforce assignment workflow"));
        assert!(transcript.contains("source=stt"));
    }

    #[test]
    fn recording_fallback_is_needed_only_without_usable_stt() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://teams.microsoft.com/meet/demo".to_string(),
            provider: MeetingProvider::MicrosoftTeams,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            stt_model: DEFAULT_MEETING_STT_MODEL.to_string(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);
        session.push_platform_transcript(TranscriptSegment {
            timestamp: "2026-04-28T12:00:00Z".to_string(),
            speaker_display: "Teams".to_string(),
            speaker_id: None,
            source: "platform_caption".to_string(),
            confidence: 0.8,
            text: "You are screen sharing.".to_string(),
        });
        assert!(meeting_transcript_needs_recording_fallback(&session));

        session.push_stt_transcript("A participant gave a real update.".to_string(), None);
        assert!(!meeting_transcript_needs_recording_fallback(&session));
    }

    #[test]
    fn full_recording_candidates_ignore_audio_chunks_and_prefer_largest() {
        let root = temp_root("recording-candidates");
        let session_id = "meeting-microsoft-recording-test";
        let sessions_dir = meeting_sessions_dir(&root);
        std::fs::create_dir_all(&sessions_dir).expect("sessions dir");
        std::fs::write(
            sessions_dir.join(format!("{session_id}-manual-recording.mp4")),
            vec![0; 16],
        )
        .expect("manual recording");
        std::fs::write(
            sessions_dir.join(format!("{session_id}-recording.mp4")),
            vec![0; 32],
        )
        .expect("recording");
        let audio_dir = sessions_dir.join(format!("{session_id}-audio"));
        std::fs::create_dir_all(&audio_dir).expect("audio dir");
        std::fs::write(audio_dir.join("chunk-001.webm"), vec![0; 64]).expect("chunk");

        let candidates = full_meeting_recording_candidates(&root, session_id);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0]
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap()
            .ends_with("-recording.mp4"));
        assert!(candidates.iter().all(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains("recording"))
                .unwrap_or(false)
        }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_runner_script_compiles_for_all_providers() {
        for provider in [
            MeetingProvider::GoogleMeet,
            MeetingProvider::MicrosoftTeams,
            MeetingProvider::Zoom,
        ] {
            let config = MeetingSessionConfig {
                root: PathBuf::from("/tmp"),
                meeting_url: "https://example.com".to_string(),
                provider,
                bot_name: "Test Bot".to_string(),
                max_duration_minutes: 60,
                audio_chunk_seconds: 30,
                stt_model: String::new(),
                realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
                mistral_api_key: None,
            };
            let script = build_meeting_runner_script(&config).unwrap();
            assert!(script.contains("chromium"));
            assert!(script.contains("emit"));
        }
    }

    #[test]
    fn extract_meeting_urls_from_email_body() {
        let body =
            "Hi team,\n\nJoin the meeting here: https://meet.google.com/abc-defg-hij\n\nThanks";
        let urls = extract_meeting_urls(body);
        assert_eq!(urls, vec!["https://meet.google.com/abc-defg-hij"]);

        let body2 = "Teams meeting: https://teams.microsoft.com/l/meetup-join/abc123 and also https://zoom.us/j/999";
        let urls2 = extract_meeting_urls(body2);
        assert_eq!(urls2.len(), 2);
        assert!(urls2[0].contains("teams.microsoft.com"));
        assert!(urls2[1].contains("zoom.us"));

        let body3 = "No meeting links here, just regular text.";
        assert!(extract_meeting_urls(body3).is_empty());
    }

    #[test]
    fn extract_meeting_time_from_ics() {
        let ics =
            "BEGIN:VCALENDAR\nDTSTART:20260415T140000Z\nDTEND:20260415T150000Z\nEND:VCALENDAR";
        assert_eq!(
            extract_meeting_time_from_text(ics),
            Some("2026-04-15T14:00:00Z".to_string())
        );

        let no_time = "Just a regular email body without ICS data.";
        assert_eq!(extract_meeting_time_from_text(no_time), None);
    }

    #[test]
    fn extracts_uid_sequence_summary_and_folded_ics_values() {
        let ics = "BEGIN:VCALENDAR\nUID:meeting-123@example.com\nSEQUENCE:4\nSUMMARY:Weekly\\, Platform Review\nDTSTART;TZID=Europe/Berlin:20260415T140000\nEND:VCALENDAR\n";
        assert_eq!(
            extract_meeting_uid_from_text(ics).as_deref(),
            Some("meeting-123@example.com")
        );
        assert_eq!(
            extract_meeting_sequence_from_text(ics).as_deref(),
            Some("4")
        );
        assert_eq!(
            extract_meeting_summary_from_text(ics).as_deref(),
            Some("Weekly, Platform Review")
        );
        assert_eq!(
            extract_meeting_time_from_text(ics).as_deref(),
            Some("2026-04-15T14:00:00")
        );
    }

    #[test]
    fn meeting_schedule_name_prefers_calendar_uid() {
        let url_a = "https://meet.google.com/aaa-bbbb-ccc";
        let url_b = "https://meet.google.com/xxx-yyyy-zzz";
        assert_eq!(
            meeting_schedule_name_for_invitation(url_a, Some("uid-1")),
            meeting_schedule_name_for_invitation(url_b, Some("uid-1"))
        );
        assert_ne!(
            meeting_schedule_name_for_invitation(url_a, None),
            meeting_schedule_name_for_invitation(url_b, None)
        );
    }

    #[test]
    fn process_email_with_link_but_no_time_needs_review() {
        let root = temp_root("no-time");
        let _ = std::fs::remove_dir_all(&root);
        let result = process_email_for_meetings(
            &root,
            "Meeting invitation",
            "Join here: https://meet.google.com/abc-defg-hij",
            "INF Yoda Notetaker",
        )
        .expect("meeting parse result");
        assert_eq!(result["action"], "needs_review");
        assert_eq!(result["results"][0]["ok"], false);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn process_email_request_schedules_join_task_with_marker_and_uid() {
        let root = temp_root("schedule-request");
        let body = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nUID:uid-request-1@example.com\nSEQUENCE:2\nSUMMARY:Platform Review\nDTSTART:20260428T130000Z\nDTEND:20260428T133000Z\nDESCRIPTION:Join https://meet.google.com/abc-defg-hij\nEND:VCALENDAR";
        let result = process_email_for_meetings(
            &root,
            "Invitation: Platform Review",
            body,
            "INF Yoda Notetaker",
        )
        .expect("schedule result");
        assert_eq!(result["action"], "processed");
        assert_eq!(result["results"][0]["action"], "scheduled");
        assert_eq!(result["results"][0]["uid"], "uid-request-1@example.com");

        let tasks = crate::mission::schedule::list_tasks(&root).expect("scheduled tasks");
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].prompt.starts_with("CTOX_MEETING_JOIN:"));
        assert!(tasks[0]
            .prompt
            .contains("https://meet.google.com/abc-defg-hij"));
        assert_eq!(tasks[0].cron_expr, "59 12 28 4 *");
        assert_eq!(
            result["results"][0]["join_time"],
            "2026-04-28T12:59:00+00:00"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn process_email_cancel_removes_uid_based_schedule() {
        let root = temp_root("schedule-cancel");
        let request_body = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nUID:uid-cancel-1@example.com\nDTSTART:20260428T130000Z\nDESCRIPTION:Join https://zoom.us/j/123456789\nEND:VCALENDAR";
        process_email_for_meetings(
            &root,
            "Invitation: Standup",
            request_body,
            "INF Yoda Notetaker",
        )
        .expect("schedule result");
        assert_eq!(
            crate::mission::schedule::list_tasks(&root)
                .expect("scheduled tasks")
                .len(),
            1
        );

        let cancel_body = "BEGIN:VCALENDAR\nMETHOD:CANCEL\nUID:uid-cancel-1@example.com\nDESCRIPTION:Join https://zoom.us/j/123456789\nEND:VCALENDAR";
        let cancel = process_email_for_meetings(
            &root,
            "Meeting cancelled: Standup",
            cancel_body,
            "INF Yoda Notetaker",
        )
        .expect("cancel result");
        assert_eq!(cancel["action"], "processed");
        assert_eq!(cancel["results"][0]["action"], "cancelled");
        assert!(crate::mission::schedule::list_tasks(&root)
            .expect("scheduled tasks")
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_sends_first_mention_ack_once_and_marks_priority() {
        let root = temp_root("mention-ack");
        let sessions_dir = meeting_sessions_dir(&root);
        std::fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let stdin_path = sessions_dir.join("session-1.stdin");
        std::fs::write(&stdin_path, "").expect("stdin file");
        let session_path = sessions_dir.join("session-1.json");
        std::fs::write(
            &session_path,
            serde_json::to_string_pretty(&json!({
                "session_id": "session-1",
                "provider": "google",
                "status": "active",
                "stdin_pipe": stdin_path.display().to_string(),
                "transcript_chunks": [
                    "Alice said the rollout is blocked by permissions.",
                    "Bob offered to prepare the deployment ticket."
                ],
                "chat_messages": [{
                    "sender": "Alice",
                    "text": "@CTOX wie ist der Status?",
                    "timestamp": "2026-04-28T12:00:00Z"
                }]
            }))
            .unwrap(),
        )
        .expect("session json");

        let request = AdapterSyncCommandRequest {
            db_path: &db_path,
            passthrough_args: &[],
            skip_flags: &[],
        };
        let first = sync(&root, &BTreeMap::new(), &request).expect("first sync");
        let second = sync(&root, &BTreeMap::new(), &request).expect("second sync");
        assert_eq!(first["active_sessions"], 1);
        assert_eq!(second["active_sessions"], 1);

        let stdin_contents = std::fs::read_to_string(&stdin_path).expect("stdin contents");
        let commands = stdin_contents.lines().collect::<Vec<_>>();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("send_chat"));
        assert!(commands[0].contains("Echtzeit-Antworten"));

        let updated_session: Value =
            serde_json::from_str(&std::fs::read_to_string(&session_path).unwrap()).unwrap();
        assert_eq!(updated_session["mention_ack_sent"], true);

        let conn = open_channel_db(&db_path).expect("channel db");
        let mention_metadata: String = conn
            .query_row(
                "SELECT metadata_json FROM communication_messages WHERE channel='meeting' AND direction='inbound' AND body_text LIKE '%@CTOX%'",
                [],
                |row| row.get(0),
            )
            .expect("mention metadata");
        let mention_metadata: Value = serde_json::from_str(&mention_metadata).unwrap();
        assert_eq!(mention_metadata["is_mention"], true);
        assert_eq!(mention_metadata["priority"], "urgent");
        assert_eq!(mention_metadata["transcript_chunk_count"], 2);
        assert!(mention_metadata["transcript_snapshot"]
            .as_str()
            .unwrap_or_default()
            .contains("rollout is blocked"));

        let mention_body: String = conn
            .query_row(
                "SELECT body_text FROM communication_messages WHERE channel='meeting' AND direction='inbound' AND body_text LIKE '%@CTOX%'",
                [],
                |row| row.get(0),
            )
            .expect("mention body");
        assert!(mention_body.contains("Live-Transcript bisher"));
        assert!(mention_body.contains("deployment ticket"));

        let ack_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE metadata_json LIKE '%ctox_first_mention_ack%'",
                [],
                |row| row.get(0),
            )
            .expect("ack count");
        assert_eq!(ack_count, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn service_sync_ingests_active_meeting_chat() {
        let root = temp_root("service-sync");
        let sessions_dir = meeting_sessions_dir(&root);
        std::fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let stdin_path = sessions_dir.join("session-service.commands.jsonl");
        std::fs::write(&stdin_path, "").expect("stdin file");
        std::fs::write(
            sessions_dir.join("session-service.json"),
            serde_json::to_string_pretty(&json!({
                "session_id": "session-service",
                "provider": "zoom",
                "bot_name": "INF Yoda Notetaker",
                "status": "active",
                "stdin_pipe": stdin_path.display().to_string(),
                "chat_messages": [{
                    "sender": "Alice",
                    "text": "@CTOX bitte pruefen",
                    "timestamp": "2026-04-28T12:00:00Z"
                }]
            }))
            .unwrap(),
        )
        .expect("session json");

        let result = service_sync(&root, &BTreeMap::new())
            .expect("service sync")
            .expect("meeting sync result");
        assert_eq!(result["ingested"], 1);

        let conn = open_channel_db(&root.join("runtime/ctox.sqlite3")).expect("channel db");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE channel='meeting' AND thread_key='session-service'",
                [],
                |row| row.get(0),
            )
            .expect("message count");
        assert_eq!(count, 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_filters_bot_echoes_from_session_json() {
        let root = temp_root("sync-own-filter");
        let sessions_dir = meeting_sessions_dir(&root);
        std::fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        std::fs::write(
            sessions_dir.join("session-own.json"),
            serde_json::to_string_pretty(&json!({
                "session_id": "session-own",
                "provider": "zoom",
                "bot_name": "INF Yoda Notetaker",
                "status": "active",
                "outbound_chat_texts": ["Ich pruefe das."],
                "chat_messages": [
                    {"sender": "INF Yoda Notetaker", "text": "Ich pruefe das.", "timestamp": "2026-04-28T12:00:00Z"},
                    {"sender": "Participant", "text": "You20:35CNCIch pruefe das.", "timestamp": "2026-04-28T12:00:01Z"}
                ]
            }))
            .unwrap(),
        )
        .expect("session json");

        let request = AdapterSyncCommandRequest {
            db_path: &db_path,
            passthrough_args: &[],
            skip_flags: &[],
        };
        let result = sync(&root, &BTreeMap::new(), &request).expect("sync");
        assert_eq!(result["active_sessions"], 1);
        assert_eq!(result["ingested"], 0);
        let conn = open_channel_db(&db_path).expect("channel db");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE channel='meeting'",
                [],
                |row| row.get(0),
            )
            .expect("message count");
        assert_eq!(count, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn send_writes_chat_command_to_session_pipe_and_records_outbound() {
        let root = temp_root("send-chat");
        let sessions_dir = meeting_sessions_dir(&root);
        std::fs::create_dir_all(&sessions_dir).expect("sessions dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let stdin_path = sessions_dir.join("session-send.stdin");
        std::fs::write(&stdin_path, "").expect("stdin file");
        std::fs::write(
            sessions_dir.join("session-send.json"),
            serde_json::to_string_pretty(&json!({
                "session_id": "session-send",
                "provider": "zoom",
                "status": "active",
                "stdin_pipe": stdin_path.display().to_string(),
                "chat_messages": []
            }))
            .unwrap(),
        )
        .expect("session json");

        let request = MeetingSendCommandRequest {
            db_path: &db_path,
            session_id: "session-send",
            body: "Ich pruefe das und melde mich hier.",
        };
        let result = send(&root, &BTreeMap::new(), &request).expect("send result");
        assert_eq!(result["status"], "sent");

        let stdin_contents = std::fs::read_to_string(&stdin_path).expect("stdin contents");
        let command: Value = serde_json::from_str(stdin_contents.trim()).expect("stdin json");
        assert_eq!(command["action"], "send_chat");
        assert_eq!(command["text"], "Ich pruefe das und melde mich hier.");

        let conn = open_channel_db(&db_path).expect("channel db");
        let outbound_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE channel='meeting' AND direction='outbound' AND metadata_json LIKE '%ctox_reply%'",
                [],
                |row| row.get(0),
            )
            .expect("outbound count");
        assert_eq!(outbound_count, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn simulate_meeting_runs_offline_post_meeting_pipeline() {
        let root = temp_root("simulate");
        let fixture = root.join("fixture.wav");
        std::fs::write(&fixture, vec![0_u8; 4096]).expect("fixture audio");
        let args = vec![
            "--provider".to_string(),
            "zoom".to_string(),
            "--audio".to_string(),
            fixture.display().to_string(),
            "--transcript".to_string(),
            "A participant agreed to create a rollout ticket.".to_string(),
            "--chat".to_string(),
            "Alice:@CTOX bitte als Ticket aufnehmen".to_string(),
        ];

        let result = simulate_meeting_session(&root, &args).expect("simulate");
        assert_eq!(result["ok"], true);
        assert_eq!(result["provider"], "zoom");
        assert_eq!(result["transcript_chunks"], 1);
        assert_eq!(result["chat_messages"], 1);
        assert_eq!(
            result["recording_artifacts"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(result["finalization"]["action"], "ingested");
        let session_id = result["session_id"].as_str().expect("session id");
        let transcript = load_meeting_transcript(&root, session_id).expect("transcript");
        assert!(transcript["transcript"]
            .as_str()
            .unwrap_or_default()
            .contains("rollout ticket"));
        assert!(transcript["chatlog"]
            .as_str()
            .unwrap_or_default()
            .contains("@CTOX"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn finalize_meeting_writes_artifact_manifest_and_default_ticket() {
        let root = temp_root("finalize");
        let config = MeetingSessionConfig {
            root: root.clone(),
            meeting_url: "https://meet.google.com/abc-defg-hij".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            stt_model: String::new(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let mut session = MeetingSession::new(&config);
        session.session_id = "meeting-google-finalize-test".to_string();
        session.status = "ended".to_string();
        session.ended_at = Some("2026-04-28T12:30:00Z".to_string());
        session
            .transcript_chunks
            .push("A participant agreed to create a deployment ticket.".to_string());
        session.chat_messages.push(ChatMessage {
            sender: "Alice".to_string(),
            text: "Bitte als Ticket aufnehmen.".to_string(),
            timestamp: "2026-04-28T12:10:00Z".to_string(),
        });
        let artifact_dir =
            meeting_sessions_dir(&root).join(format!("{}-audio", session.session_id));
        std::fs::create_dir_all(&artifact_dir).expect("artifact dir");
        std::fs::write(artifact_dir.join("chunk-001.webm"), b"audio").expect("audio artifact");
        std::fs::write(artifact_dir.join("screen-001.mp4"), b"screen").expect("screen artifact");

        let result = finalize_meeting(&root, &session, &config).expect("finalize");
        assert_eq!(result["action"], "ingested");
        assert_eq!(result["recording_artifact_count"], 2);
        assert!(result["post_meeting_ticket_id"].as_str().is_some());

        let transcript_path =
            meeting_sessions_dir(&root).join(format!("{}-transcript.txt", session.session_id));
        let chatlog_path =
            meeting_sessions_dir(&root).join(format!("{}-chatlog.txt", session.session_id));
        let manifest_path =
            meeting_sessions_dir(&root).join(format!("{}-artifacts.json", session.session_id));
        assert!(transcript_path.exists());
        assert!(chatlog_path.exists());
        assert!(manifest_path.exists());
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
        assert_eq!(
            manifest["recording_artifacts"].as_array().map(Vec::len),
            Some(2)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detect_cancellation_and_update() {
        assert!(is_meeting_cancellation(
            "Meeting Canceled: Sprint Review",
            ""
        ));
        assert!(is_meeting_cancellation(
            "",
            "BEGIN:VCALENDAR\nMETHOD:CANCEL\nEND:VCALENDAR"
        ));
        assert!(is_meeting_cancellation("Abgesagt: Weekly Standup", ""));
        assert!(!is_meeting_cancellation("Meeting: Sprint Review", ""));

        assert!(is_meeting_update("Updated: Sprint Review", ""));
        assert!(is_meeting_update("Verschoben: Weekly Standup", ""));
        assert!(!is_meeting_update("Meeting: Sprint Review", ""));
    }

    #[test]
    fn cron_for_meeting_time_produces_valid_expression() {
        assert_eq!(
            cron_for_meeting_time("2026-04-15T14:30:00Z"),
            Some("30 14 15 4 *".to_string())
        );
        assert_eq!(
            cron_for_meeting_time("2026-12-01T09:00:00Z"),
            Some("0 9 1 12 *".to_string())
        );
        assert_eq!(cron_for_meeting_time("bad"), None);
    }

    #[test]
    fn timeout_and_inactivity_detection_present_in_script() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp"),
            meeting_url: "https://meet.google.com/abc".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            stt_model: String::new(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let script = build_meeting_runner_script_with_timeout(&config).unwrap();
        // Verify transplanted reference detection logic is present
        assert!(
            script.contains("detectLoneParticipant"),
            "missing participant detection"
        );
        assert!(
            script.contains("data-avatar-count"),
            "missing Google Meet badge detection"
        );
        assert!(
            script.contains("detectSilence"),
            "missing silence detection"
        );
        assert!(
            script.contains("AudioContext"),
            "missing AudioContext for silence"
        );
        assert!(
            script.contains("ctoxMeetingEnd"),
            "missing meeting end callback"
        );
        assert!(
            script.contains("verifyJoinedUi"),
            "missing join verification gate"
        );
        assert!(script.contains("join_failed"), "missing join failure event");
        assert!(
            script.contains("CTOX_MEETING_COMMAND_FILE"),
            "missing command file bridge"
        );
        assert!(
            script.contains("commandFilePollInterval"),
            "missing command file poller"
        );
        assert!(
            script.contains("--in-process-gpu"),
            "missing chromium in-process GPU flag"
        );
        assert!(
            script.contains("installChatObservers"),
            "missing chat mutation observer"
        );
        assert!(
            script.contains("transcriptPollInterval"),
            "missing live caption transcript observer"
        );
        assert!(
            script.contains("speakerPollInterval"),
            "missing active speaker observer"
        );
        assert!(
            script.contains("platform_active_speaker"),
            "missing platform active speaker source"
        );
        assert!(
            script.contains("platform_caption"),
            "missing platform caption source"
        );

        // Verify Teams uses ffmpeg path
        let teams_config = MeetingSessionConfig {
            provider: MeetingProvider::MicrosoftTeams,
            ..config.clone()
        };
        let teams_script = build_meeting_runner_script_with_timeout(&teams_config).unwrap();
        assert!(
            teams_script.contains("ffmpeg"),
            "Teams should use ffmpeg recording"
        );
        assert!(
            teams_script.contains("virtual_output.monitor"),
            "Teams should use PulseAudio"
        );
        assert!(
            teams_script.contains("--kiosk"),
            "Teams should use kiosk mode"
        );
        assert!(
            teams_script.contains("warmUpTeamsMediaDevices"),
            "Teams should warm up media devices"
        );
        assert!(
            teams_script.contains("enableTeamsLiveCaptions"),
            "Teams should enable live captions"
        );
        assert!(
            teams_script.contains("stdoutClosed"),
            "meeting runner should tolerate stdout EPIPE after host shutdown"
        );
        assert!(
            teams_script.contains(r#"type: "recording_artifact""#),
            "Teams should preserve the full ffmpeg recording as an artifact"
        );
        assert!(
            teams_script.contains("mistral_realtime_stt.py"),
            "Teams should write the realtime STT helper"
        );
        assert!(
            teams_script.contains("client.audio.realtime.transcribe_stream"),
            "Teams live transcript must use Mistral realtime streaming"
        );
        assert!(
            teams_script.contains("voxtral-mini-transcribe-realtime-2602"),
            "Teams should default to the Voxtral realtime model"
        );
        assert!(
            teams_script.contains("AudioFormat(encoding=\"pcm_s16le\", sample_rate=16000)"),
            "Teams should stream raw 16 kHz PCM into realtime STT"
        );
        assert!(
            !teams_script.contains("teams-audio-chunks"),
            "Teams live transcript must not use file chunk directories"
        );
        assert!(
            !teams_script.contains("audioSegmenter"),
            "Teams live transcript must not use the old batch segmenter"
        );
        assert!(
            !teams_script.contains("Live-ish Teams STT"),
            "Teams should not present delayed batch STT as live transcript"
        );
    }

    #[test]
    fn runner_scripts_keep_provider_recording_paths() {
        let google_config = MeetingSessionConfig {
            root: PathBuf::from("/tmp"),
            meeting_url: "https://meet.google.com/abc".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "INF Yoda Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            stt_model: String::new(),
            realtime_stt_model: "voxtral-mini-transcribe-realtime-2602".to_string(),
            mistral_api_key: None,
        };
        let google_script = build_meeting_runner_script(&google_config).unwrap();
        assert!(google_script.contains("getDisplayMedia"));
        assert!(google_script.contains("ctoxAudioChunk"));
        assert!(google_script.contains("video: true"));

        let zoom_script = build_meeting_runner_script(&MeetingSessionConfig {
            provider: MeetingProvider::Zoom,
            meeting_url: "https://zoom.us/j/123456".to_string(),
            ..google_config.clone()
        })
        .unwrap();
        assert!(zoom_script.contains("getDisplayMedia"));
        assert!(zoom_script.contains("ctoxAudioChunk"));
        assert!(zoom_script.contains("buildZoomWebClientUrl"));
        assert!(zoom_script.contains("button.preview-join-button"));
        assert!(zoom_script.contains("prepareZoomAudio"));
        assert!(zoom_script.contains("startZoomRemovalMonitor"));

        let teams_script = build_meeting_runner_script(&MeetingSessionConfig {
            provider: MeetingProvider::MicrosoftTeams,
            meeting_url: "https://teams.microsoft.com/l/meetup-join/abc".to_string(),
            ..google_config
        })
        .unwrap();
        assert!(teams_script.contains("ffmpeg"));
        assert!(teams_script.contains("recording_artifact"));
        assert!(teams_script.contains(r#"extension: "mp4""#));
        assert!(teams_script.contains("x11grab"));
    }
}
