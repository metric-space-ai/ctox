// Origin: CTOX
// License: Apache-2.0
//
// Meeting bot adapter — joins video meetings (Google Meet, Microsoft Teams, Zoom)
// as a silent participant via Playwright, captures audio for transcription, monitors
// the meeting chat, and responds when @CTOX is mentioned.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::channels::{
    ensure_routing_rows_for_inbound, open_channel_db, refresh_thread, upsert_communication_message,
    UpsertMessage,
};
use super::communication_adapters::{AdapterSyncCommandRequest, MeetingSendCommandRequest};

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
    let sessions_dir = meeting_sessions_dir(root);
    if !sessions_dir.exists() {
        return Ok(json!({"ok": true, "active_sessions": 0, "ingested": 0}));
    }
    let db_path = request.db_path;
    let mut conn = open_channel_db(db_path)?;
    let mut active = 0u64;
    let mut ingested = 0u64;
    let account_key = "meeting:system";

    for entry in fs::read_dir(&sessions_dir).unwrap_or_else(|_| fs::read_dir(".").unwrap()) {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(session) = serde_json::from_str::<Value>(&contents) else {
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
            .unwrap_or("");
        let provider = session
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

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
            let metadata = json!({
                "provider": provider,
                "session_id": session_id,
                "source": "meeting_chat",
                "is_mention": is_mention,
                "skill": if is_mention { "meeting-participant" } else { "" },
            });

            upsert_communication_message(
                &mut conn,
                UpsertMessage {
                    message_key: &message_key,
                    channel: "meeting",
                    account_key,
                    thread_key: session_id,
                    remote_id: &message_key,
                    direction: "inbound",
                    folder_hint: "chat",
                    sender_display: sender,
                    sender_address: sender,
                    recipient_addresses_json: "[]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject: &format!("{} meeting chat", provider),
                    preview: &text[..text.len().min(120)],
                    body_text: text,
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
            let _ = refresh_thread(&mut conn, session_id);
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
            sender_display: "CTOX Notetaker",
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
    let stdin_path = session.get("stdin_pipe").and_then(Value::as_str);
    if let Some(stdin_path) = stdin_path {
        let command = json!({"action": "send_chat", "text": request.body});
        match fs::OpenOptions::new().append(true).open(stdin_path) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", command);
            }
            Err(err) => {
                eprintln!("[meeting] warning: could not write to stdin pipe: {err}");
            }
        }
    }

    Ok(
        json!({"ok": true, "status": "sent", "session_id": request.session_id, "message_key": message_key}),
    )
}

/// Service sync — delegates to sync() with proper db_path.
pub(crate) fn service_sync(
    _root: &Path,
    _settings: &BTreeMap<String, String>,
) -> Result<Option<Value>> {
    // Service sync is handled via the normal sync_configured_channels path
    // which calls adapter.service_sync(). For meeting we need the db_path
    // which is already resolved in channels.rs sync_channel().
    Ok(None)
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
            let bot_name = find_flag_value(args, "--name").unwrap_or("CTOX Notetaker");
            let runtime = super::communication_gateway::runtime_settings_from_root(
                root,
                super::communication_gateway::CommunicationAdapterKind::Meeting,
            );
            let mut config = MeetingSessionConfig::from_runtime(root, url, &runtime)?;
            if bot_name != "CTOX Notetaker" {
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
            let bot_name = find_flag_value(args, "--name").unwrap_or("CTOX Notetaker");
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
            let runtime = super::communication_gateway::runtime_settings_from_root(
                root,
                super::communication_gateway::CommunicationAdapterKind::Meeting,
            );
            let config = MeetingSessionConfig::from_runtime(root, url, &runtime)?;
            let script = build_meeting_runner_script(&config)?;
            print!("{script}");
            Ok(())
        }
        "status" => {
            let sessions_dir = meeting_sessions_dir(root);
            let mut sessions = Vec::new();
            if sessions_dir.exists() {
                for entry in fs::read_dir(&sessions_dir)? {
                    let entry = entry?;
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(contents) = fs::read_to_string(entry.path()) {
                            if let Ok(session) = serde_json::from_str::<Value>(&contents) {
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
        _ => {
            println!("usage: ctox meeting <join|schedule|cancel|status|transcript> [args]");
            println!();
            println!("  join <url> [--name <bot-name>]       Join a meeting now");
            println!("  schedule <url> --time <ISO-8601>     Schedule a future join");
            println!("  cancel <url>                         Cancel a scheduled join");
            println!("  status                               Show active/scheduled sessions");
            println!("  transcript <session_id>              Print transcript + chatlog as JSON");
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

    // Find Node.js executable
    let node = find_node_executable()?;

    eprintln!(
        "[meeting] Starting {} session: {}",
        config.provider.as_str(),
        config.meeting_url
    );
    eprintln!("[meeting] Script: {}", script_path.display());

    // Pre-flight: check if the STT engine is reachable. We don't abort if it
    // isn't — audio chunks are still captured and persisted to disk, and we
    // retry transcription at finalize time. But we warn clearly so the user
    // knows to run `ctox start` if they want live STT.
    let engine_reachable = check_engine_reachable(&config.proxy_host, config.proxy_port);
    session.engine_was_reachable_at_start = engine_reachable;
    if engine_reachable {
        eprintln!(
            "[meeting] STT engine reachable at http://{}:{}",
            config.proxy_host, config.proxy_port
        );
    } else {
        eprintln!(
            "[meeting] WARNING: STT engine not reachable at http://{}:{}",
            config.proxy_host, config.proxy_port
        );
        eprintln!("[meeting] Audio chunks will still be captured and saved to disk.");
        eprintln!("[meeting] To enable live transcription, run `ctox start` in another terminal.");
        eprintln!("[meeting] Unsent chunks will be retried at meeting end if the engine becomes available.");
    }

    // Spawn the Node.js process
    let mut child = Command::new(&node)
        .current_dir(&reference_dir)
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn node at {node}"))?;

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
                eprintln!("[meeting] Joined meeting successfully");
                session.status = "active".to_string();
                session.save(root)?;
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
                    &config.proxy_host,
                    config.proxy_port,
                    Path::new(chunk_for_stt),
                    &config.stt_model,
                ) {
                    Ok(text) if !text.is_empty() => {
                        eprintln!("[meeting] transcript: {}...", &text[..text.len().min(80)]);
                        session.transcript_chunks.push(text);
                        // Chunk successfully transcribed — remove the persisted
                        // copy to save disk space.
                        if let Some(p) = persisted_path.as_ref() {
                            let _ = fs::remove_file(p);
                        }
                        session.save(root)?;
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
                // Persist session so sync() can pick up new chat messages
                session.save(root)?;

                // @CTOX mentions are now ingested as normal inbound messages
                // via sync() → upsert_communication_message(). The service
                // loop's route_external_messages() will pick them up and
                // route them to the agent with the meeting-participant skill.
                // No extra queue task needed — the standard pipeline handles it.
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

    // Finalize
    session.status = "ended".to_string();
    if session.ended_at.is_none() {
        session.ended_at = Some(now_iso_string());
    }
    session.save(root)?;

    // Lazy re-transcription: if the engine is now reachable and we have
    // pending chunks from failed STT attempts, retry them now.
    let retry_result = retry_pending_audio_chunks(root, &mut session, config);

    let finalization = finalize_meeting(root, &session, config)?;

    drop(stdin); // close stdin pipe

    Ok(json!({
        "ok": true,
        "session_id": session.session_id,
        "provider": session.provider,
        "status": "finalized",
        "transcript_chunks": session.transcript_chunks.len(),
        "chat_messages": session.chat_messages.len(),
        "pending_audio_chunks": session.pending_audio_chunks.len(),
        "stt_retry": retry_result,
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
         4. **Knowledge** -- Technical facts, status updates, numbers that CTOX should remember.\n\
         \n\
         ### What to create\n\
         \n\
         - For each **action item**: Create a ticket with clear title, assignee, and deadline.\n\
         - For each **decision/fact**: Create a knowledge context entry.\n\
         - For **open questions**: Create a follow-up queue task.\n\
         - **Always**: Send a meeting summary to the relevant communication channel.\n\
         \n\
         ### Quality checks\n\
         \n\
         - STT does not attribute speakers reliably. Say \"a participant\" not \"Max\".\n\
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
        chat_count = session.chat_messages.len(),
        transcript_path = transcript_path.display(),
        transcript = if transcript.is_empty() { "(empty)" } else { &transcript },
        chat_log = if chat_log.is_empty() { "(no chat)" } else { &chat_log },
    );

    // Ingest the summary as a normal inbound message in the "meeting" channel.
    // The service loop's route_external_messages() will pick it up and route
    // it to the agent with the meeting-participant skill via metadata.
    let db_path = root.join("runtime/cto_agent.db");
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
        "skill": "meeting-participant",
    }))
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
    let engine_now_reachable = check_engine_reachable(&config.proxy_host, config.proxy_port);
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
        match transcribe_audio_chunk(
            &config.proxy_host,
            config.proxy_port,
            Path::new(&chunk_path),
            &config.stt_model,
        ) {
            Ok(text) if !text.is_empty() => {
                session.transcript_chunks.push(text);
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

/// Quick TCP-level reachability check for the STT engine.
/// Returns true if we can open a socket to host:port within 500ms.
pub(crate) fn check_engine_reachable(host: &str, port: u16) -> bool {
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let addr_str = format!("{host}:{port}");
    let addrs: Vec<SocketAddr> = match addr_str.to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(_) => return false,
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return true;
        }
    }
    false
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
    pub proxy_host: String,
    pub proxy_port: u16,
    pub stt_model: String,
}

impl MeetingSessionConfig {
    pub(crate) fn from_runtime(
        root: &Path,
        meeting_url: &str,
        runtime: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let provider = MeetingProvider::detect(meeting_url)
            .context("cannot detect meeting provider from URL")?;
        let bot_name = runtime
            .get("CTO_MEETING_BOT_NAME")
            .cloned()
            .unwrap_or_else(|| "CTOX Notetaker".to_string());
        let max_duration_minutes = runtime
            .get("CTO_MEETING_MAX_DURATION_MINUTES")
            .and_then(|v| v.parse().ok())
            .unwrap_or(180u64);
        let audio_chunk_seconds = runtime
            .get("CTO_MEETING_AUDIO_CHUNK_SECONDS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30u64);
        let proxy_host = runtime
            .get("CTOX_PROXY_HOST")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let proxy_port = runtime
            .get("CTOX_PROXY_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080u16);
        let stt_model = runtime.get("CTOX_STT_MODEL").cloned().unwrap_or_default();
        Ok(Self {
            root: root.to_path_buf(),
            meeting_url: meeting_url.to_string(),
            provider,
            bot_name,
            max_duration_minutes,
            audio_chunk_seconds,
            proxy_host,
            proxy_port,
            stt_model,
        })
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
    pub chat_messages: Vec<ChatMessage>,
    pub pid: Option<u32>,
    pub stdin_pipe: Option<String>,
    /// Paths of audio chunk files whose STT failed (engine offline or error).
    /// Retried at finalize time if the engine becomes reachable.
    pub pending_audio_chunks: Vec<String>,
    /// Whether the STT engine was reachable when the session started.
    pub engine_was_reachable_at_start: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatMessage {
    pub sender: String,
    pub text: String,
    pub timestamp: String,
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
            chat_messages: Vec::new(),
            pid: None,
            stdin_pipe: None,
            pending_audio_chunks: Vec::new(),
            engine_was_reachable_at_start: false,
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
            "chat_message_count": self.chat_messages.len(),
            "transcript_chunks": self.transcript_chunks,
            "chat_messages": self.chat_messages.iter().map(|m| json!({
                "sender": m.sender,
                "text": m.text,
                "timestamp": m.timestamp,
            })).collect::<Vec<_>>(),
            "pid": self.pid,
            "stdin_pipe": self.stdin_pipe,
            "pending_audio_chunks": self.pending_audio_chunks,
            "engine_was_reachable_at_start": self.engine_was_reachable_at_start,
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
        self.transcript_chunks.join("\n")
    }

    /// Build the full chat log.
    pub(crate) fn full_chat_log(&self) -> String {
        self.chat_messages
            .iter()
            .map(|msg| format!("[{}] {}: {}", msg.timestamp, msg.sender, msg.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if a chat message mentions @CTOX.
    /// Returns true if "@ctox" appears with a word boundary on both sides
    /// (so "@ctoxbar" doesn't match, but "@CTOX Notetaker" or "@ctox!" do).
    pub(crate) fn is_mention(text: &str) -> bool {
        let lower = text.to_lowercase();
        let needle = "@ctox";
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
        false
    }

    /// Check if a chat message likely originated from this bot itself.
    /// Used to prevent self-loop when the bot's own replies appear in the chat.
    pub(crate) fn is_own_message(&self, sender: &str, text: &str) -> bool {
        let bot_name_lower = self.bot_name.to_lowercase();
        let bot_name_lower = bot_name_lower.trim();
        if bot_name_lower.is_empty() {
            return false;
        }
        let sender_lower = sender.to_lowercase();
        // Match if sender contains the bot name (sender field may include
        // role suffixes like "(Host)" or be wrapped in other text)
        if sender_lower.contains(bot_name_lower) {
            return true;
        }
        // Some chat scrapers misattribute and put the sender in the text;
        // match if text starts with the bot name + colon/dash separator
        let text_lower = text.to_lowercase();
        let text_trimmed = text_lower.trim_start();
        if text_trimmed.starts_with(bot_name_lower) {
            let after_name = &text_trimmed[bot_name_lower.len()..];
            if after_name.starts_with(':')
                || after_name.starts_with(" -")
                || after_name.starts_with(" to ")
            {
                return true;
            }
        }
        false
    }
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

const emit = (event) => {
  process.stdout.write(JSON.stringify(event) + "\n");
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
await context.grantPermissions(["microphone", "camera"], { origin: meetingUrl });
const page = await context.newPage();

// --- Join the meeting ---
emit({ type: "status", status: "joining", provider });
await page.goto(meetingUrl, { waitUntil: "networkidle" });
await page.waitForTimeout(5000);

__JOIN_SCRIPT__

emit({ type: "joined", provider });

// --- Recording setup (transplanted from ScreenApp reference) ---
// Google Meet + Zoom: getDisplayMedia + MediaRecorder (in-browser tab capture)
// Microsoft Teams: ffmpeg + X11grab + PulseAudio (out-of-process)
let chunkIndex = 0;
await page.exposeFunction("ctoxAudioChunk", async (base64Data) => {
  const filePath = path.join(tempDir, `chunk_${String(chunkIndex).padStart(4, "0")}.webm`);
  fs.writeFileSync(filePath, Buffer.from(base64Data, "base64"));
  emit({ type: "audio_chunk", path: filePath, index: chunkIndex });
  chunkIndex++;
});

let meetingEnded = false;
await page.exposeFunction("ctoxMeetingEnd", (reason) => {
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

if (provider === "microsoft") {
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
  const ffmpegArgs = [
    "-y", "-loglevel", "info",
    "-f", "x11grab", "-video_size", "1280x720", "-framerate", "25",
    "-draw_mouse", "0", "-i", `${display}+0,80`,
    "-f", "pulse", "-ac", "2", "-ar", "44100", "-i", "virtual_output.monitor",
    "-c:v", "libx264", "-preset", "faster", "-pix_fmt", "yuv420p", "-crf", "23",
    "-g", "50", "-threads", "0",
    "-c:a", "aac", "-b:a", "128k", "-ar", "44100", "-ac", "2", "-strict", "experimental",
    "-vsync", "cfr", "-async", "1",
    "-movflags", "+faststart",
    outputPath,
  ];
  const ffmpeg = spawn("ffmpeg", ffmpegArgs, {
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env, XDG_RUNTIME_DIR: process.env.XDG_RUNTIME_DIR || "/run/user/1001", DISPLAY: display },
  });
  ffmpeg.stderr.on("data", (d) => {
    const s = d.toString();
    if (s.includes("error") || s.includes("Error")) emit({ type: "ffmpeg_error", text: s.substring(0, 200) });
  });
  ffmpeg.on("exit", (code) => {
    if (code !== 0 && code !== null) emit({ type: "ffmpeg_exit", code });
  });

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

  // Teams also monitors audio silence via parec (Node-side)
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
        if (peak < 200) { consecutiveSilent++; if (consecutiveSilent >= checksNeeded) { clearInterval(iv); meetingEnded = true; } }
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

  // Graceful ffmpeg stop
  try { ffmpeg.stdin.write("q\n"); ffmpeg.stdin.end(); } catch { ffmpeg.kill("SIGTERM"); }
  await new Promise(r => { ffmpeg.on("exit", r); setTimeout(() => { try { ffmpeg.kill("SIGKILL"); } catch {} r(); }, 20000); });

  // Read the recording and emit as chunks
  if (fs.existsSync(outputPath)) {
    const buffer = fs.readFileSync(outputPath);
    let binary = "";
    const bytes = new Uint8Array(buffer);
    for (let i = 0; i < bytes.byteLength; i++) binary += String.fromCharCode(bytes[i]);
    await window.ctoxAudioChunk(btoa(binary));
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
    // Stop video tracks immediately — we only want audio for STT, video bloats chunks
    videoTracks.forEach(t => { try { t.stop(); } catch {} });

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

// --- Chat monitoring ---
const knownChatKeys = new Set();
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

// --- Participant count monitoring ---
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

// --- Stdin command handling ---
const rl = readline.createInterface({ input: process.stdin });
rl.on("line", async (line) => {
  try {
    const cmd = JSON.parse(line);
    if (cmd.action === "send_chat") {
      await page.evaluate(async (text) => {
        __SEND_CHAT_SCRIPT__
      }, cmd.text);
      emit({ type: "chat_sent", text: cmd.text });
    }
  } catch (err) {
    emit({ type: "error", message: err.message });
  }
});

// --- Wait for meeting to end ---
const startTime = Date.now();
while (!meetingEnded && (Date.now() - startTime) < maxDurationMs) {
  await new Promise(r => setTimeout(r, 1000));
}

clearInterval(chatPollInterval);
clearInterval(participantPollInterval);
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
        await page.getByRole("button", { name: "Continue without microphone and camera" }).waitFor({ timeout: 30000 });
        await page.getByRole("button", { name: "Continue without microphone and camera" }).click();
      }
    );
  } catch { /* may not appear */ }

  // 2. Verify we are on a Google Meet page (not redirected to sign-in)
  const detectPage = async () => {
    const currentUrl = page.url();
    if (currentUrl.startsWith("https://accounts.google.com/")) {
      return "SIGN_IN_PAGE";
    }
    if (!currentUrl.includes("meet.google.com")) {
      return "UNSUPPORTED_PAGE";
    }
    return "GOOGLE_MEET_PAGE";
  };

  const pageStatus = await detectPage();
  if (pageStatus === "SIGN_IN_PAGE") {
    emit({ type: "error", message: "Meeting requires sign in" });
  }

  // 3. Wait for name input and fill it (with retry)
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
      async () => await page.waitForSelector('input[type="text"][aria-label="Your name"]', { timeout: 10000 }),
      3,
      15000
    );
  } catch (err) {
    emit({ type: "warning", message: "Name input not found: " + err.message });
  }

  await page.waitForTimeout(10000);
  await page.fill('input[type="text"][aria-label="Your name"]', botName);
  await page.waitForTimeout(10000);

  // 4. Click join button (Ask to join / Join now / Join anyway) — with retry
  {
    const possibleTexts = ["Ask to join", "Join now", "Join anyway"];
    let buttonClicked = false;
    for (let attempt = 0; attempt <= 3 && !buttonClicked; attempt++) {
      for (const text of possibleTexts) {
        try {
          const btn = page.locator("button", { hasText: new RegExp(text.toLowerCase(), "i") }).first();
          if (await btn.count() > 0) {
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
    const wanderingTime = 10 * 60 * 1000;
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
          if (bodyText.includes(LOBBY_HOST_TEXT)) return; // still waiting

          // Check for People button or Leave call button
          const detected = await page.evaluate(() => {
            try {
              const peopleBtn = document.querySelector('button[aria-label^="People"]')
                || document.querySelector('button[aria-label*="People"]');
              const leaveBtn = document.querySelector('button[aria-label="Leave call"]');

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
  // 1. Click "Join from browser" / "Continue on this browser"
  const joinButtonSelectors = [
    'button[aria-label="Join meeting from this browser"]',
    'button[aria-label="Continue on this browser"]',
    'button[aria-label="Join on this browser"]',
    'button:has-text("Continue on this browser")',
    'button:has-text("Join from browser")',
  ];
  let browserBtnClicked = false;
  for (const sel of joinButtonSelectors) {
    try {
      await page.waitForSelector(sel, { timeout: 60000 });
      await page.click(sel, { force: true });
      browserBtnClicked = true;
      break;
    } catch { continue; }
  }
  if (!browserBtnClicked) {
    emit({ type: "warning", message: "Join from browser button not found, proceeding" });
  }

  // 2. Fill name input (Teams-specific data-tid selector)
  try {
    const nameInput = page.locator('input[data-tid="prejoin-display-name-input"]');
    await nameInput.waitFor({ state: "visible", timeout: 120000 });
    await nameInput.fill(botName);
    await page.waitForTimeout(1000);
  } catch (err) {
    emit({ type: "warning", message: "Teams name input not found after 120s: " + err.message });
  }

  // 3. Toggle off camera and mute microphone
  try {
    await page.waitForTimeout(2000);
    // Camera off
    const cameraSelectors = [
      'input[data-tid="toggle-video"][checked]',
      'input[type="checkbox"][title*="Turn camera off" i]',
      'input[role="switch"][data-tid="toggle-video"]',
      'button[aria-label*="Turn camera off" i]',
      'button[aria-label*="Camera off" i]',
    ];
    for (const sel of cameraSelectors) {
      const el = page.locator(sel).first();
      if (await el.isVisible({ timeout: 2000 }).catch(() => false)) {
        await el.click(); await page.waitForTimeout(500); break;
      }
    }
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
    const possibleTexts = ["Join now", "Join", "Ask to join", "Join meeting"];
    let joinClicked = false;
    for (let attempt = 0; attempt <= 3 && !joinClicked; attempt++) {
      for (const text of possibleTexts) {
        try {
          const btn = page.getByRole("button", { name: new RegExp(text, "i") });
          if (await btn.isVisible({ timeout: 3000 }).catch(() => false)) {
            await btn.click(); joinClicked = true; break;
          }
        } catch {}
      }
      if (!joinClicked) await page.waitForTimeout(15000);
    }
    if (!joinClicked) emit({ type: "warning", message: "Could not find Teams join button" });
  }

  // 5. Wait for lobby admission (Leave button appears)
  {
    const DENIED_TEXT = "Sorry, but you were denied access to the meeting";
    const wanderingTime = 10 * 60 * 1000;
    try {
      const leaveBtn = page.getByRole("button", { name: /Leave/i });
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
// Zoom join flow — transplanted from ScreenApp meeting-bot reference
try {
  // Block .exe downloads
  await page.route("**/*.exe", (route) => {
    emit({ type: "status", status: "blocked_exe_download", url: route.request().url() });
  });

  // 1. Accept cookies
  try {
    await page.waitForTimeout(3000);
    const acceptCookies = page.locator("button", { hasText: "Accept Cookies" });
    await acceptCookies.waitFor({ timeout: 5000 });
    await acceptCookies.click({ force: true });
  } catch { /* may not appear */ }

  // 2. Click "Download Now" then "Join from your browser" (with retry)
  let usingDirectWebClient = false;
  const findJoinFromBrowser = async (retry) => {
    if (retry >= 3) return false;
    try {
      await page.waitForTimeout(5000);
      const downloadBtn = page.getByRole("button", { name: /Download Now/i }).first();
      if (await downloadBtn.isVisible()) await downloadBtn.click({ force: true });
      const joinLink = page.locator("a", { hasText: "Join from your browser" }).first();
      await joinLink.waitFor({ timeout: 5000 });
      if (await joinLink.count() > 0) { await joinLink.click({ force: true }); return true; }
      return await findJoinFromBrowser(retry + 1);
    } catch { return retry < 3 ? await findJoinFromBrowser(retry + 1) : false; }
  };

  let foundBrowserJoin = await findJoinFromBrowser(0);

  // Wait for nav to complete after clicking Join from browser
  if (foundBrowserJoin) {
    for (let i = 0; i < 3; i++) {
      try {
        const link = page.locator("a", { hasText: "Join from your browser" }).first();
        await link.waitFor({ timeout: 4000 });
        // Still on the same page — wait more
      } catch { break; } // link gone = navigated
    }
  }

  // 3. Fallback: direct /wc/join/ URL
  if (!foundBrowserJoin) {
    usingDirectWebClient = true;
    try {
      const wcUrl = new URL(meetingUrl);
      wcUrl.pathname = wcUrl.pathname.replace("/j/", "/wc/join/");
      await page.goto(wcUrl.toString(), { waitUntil: "networkidle" });
    } catch {
      emit({ type: "error", message: "Cannot access Zoom web client" });
    }
  }

  await page.waitForTimeout(10000);

  // 4. Detect iframe vs app container (with bidirectional retry from reference)
  let iframe = page;
  {
    const tried = [];
    const detect = async (startWith) => {
      if (tried.includes("app") && tried.includes("iframe")) return false;
      tried.push(startWith);
      try {
        if (startWith === "app") {
          const input = await page.waitForSelector('input[type="text"]', { timeout: 30000 });
          const join = page.locator("button", { hasText: /Join/i });
          await join.waitFor({ timeout: 15000 });
          if (input && join) { iframe = page; }
          else return await detect("iframe");
        }
        if (startWith === "iframe") {
          const handle = await page.waitForSelector("iframe#webclient", { timeout: 30000, state: "attached" });
          const frame = await handle.contentFrame();
          if (frame) { iframe = frame; }
          else return await detect("app");
        }
        return true;
      } catch {
        return await detect(startWith === "app" ? "iframe" : "app");
      }
    };
    const found = await detect(usingDirectWebClient ? "app" : "iframe");
    if (!found) emit({ type: "error", message: "Failed to detect Zoom web client container" });
  }

  // 5. Enter name
  try {
    await iframe.waitForSelector('input[type="text"]', { timeout: 60000 });
    await page.waitForTimeout(5000);
    await iframe.fill('input[type="text"]', botName);
    await page.waitForTimeout(3000);
  } catch (err) {
    emit({ type: "warning", message: "Zoom name input not found: " + err.message });
  }

  // 6. Click Join
  try {
    const joinBtn = iframe.locator("button", { hasText: "Join" });
    await joinBtn.click();
  } catch (err) {
    emit({ type: "error", message: "Zoom join button not found: " + err.message });
  }

  // 7. Wait in waiting room — footer-based participant detection from reference
  {
    const DENIED = "You have been removed";
    const wanderingTime = 10 * 60 * 1000;
    const lobbyResult = await new Promise((resolve) => {
      const timeout = setTimeout(() => { clearInterval(interval); resolve(false); }, wanderingTime);
      const interval = setInterval(async () => {
        try {
          const footerInfo = iframe.locator("#wc-footer");
          await footerInfo.waitFor({ state: "attached" });
          const footerText = await footerInfo.innerText();
          // Parse "N participants" from footer
          const tokens1 = footerText.split("\n");
          const tokens2 = footerText.split(" ");
          const tokens = tokens1.length > tokens2.length ? tokens1 : tokens2;
          const filtered = [];
          for (const tok of tokens) {
            if (!tok) continue;
            if (!Number.isNaN(Number(tok.trim()))) filtered.push(tok);
            else if (tok.trim().toLowerCase() === "participants") { filtered.push("participants"); break; }
          }
          const joined = filtered.join("");
          if (joined === "participants") return;
          const isValid = joined.match(/\d+(.*)participants/i);
          if (!isValid) return;
          const num = joined.match(/\d+/);
          if (num && Number(num[0]) > 0) {
            clearInterval(interval); clearTimeout(timeout); resolve(true);
          }
        } catch {}
      }, 2000);
    });
    if (!lobbyResult) {
      const bodyText = await page.evaluate(() => document.body.innerText);
      emit({ type: "error", message: "Zoom lobby failed", bodyText: (bodyText || "").substring(0, 500) });
    }
  }

  // 8. Dismiss device notifications (camera/mic not found)
  try {
    const stopWaiting = 30000;
    const cameraFound = [];
    const micFound = [];
    await new Promise((res) => {
      const t = setTimeout(() => { clearInterval(iv); res(false); }, stopWaiting);
      const iv = setInterval(async () => {
        try {
          const camDiv = iframe.locator("div", { hasText: /^Cannot detect your camera/i }).first();
          const micDiv = iframe.locator("div", { hasText: /^Cannot detect your microphone/i }).first();
          if (await camDiv.isVisible()) { if (!cameraFound.includes("found")) cameraFound.push("found"); }
          else { if (cameraFound.includes("found")) cameraFound.push("dismissed"); }
          if (await micDiv.isVisible()) { if (!micFound.includes("found")) micFound.push("found"); }
          else { if (micFound.includes("found")) micFound.push("dismissed"); }
          if (micFound.length >= 2 && cameraFound.length >= 2) {
            clearInterval(iv); clearTimeout(t); res(true); return;
          }
          const closeButtons = await iframe.getByLabel("close").all();
          for (const btn of closeButtons) {
            if (await btn.isVisible()) await btn.click({ timeout: 5000 });
          }
        } catch { clearInterval(iv); clearTimeout(t); res(false); }
      }, 2000);
    });
  } catch {}

  // 9. Dismiss OK button
  try {
    const okBtn = iframe.locator("button", { hasText: "OK" }).first();
    if (await okBtn.isVisible()) await okBtn.click({ timeout: 5000 });
  } catch {}
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
          const text = item.textContent?.trim() || '';
          if (text) messages.push({ sender: 'Participant', text, ts: new Date().toISOString() });
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
        const senderEl = el.querySelector('[data-tid="message-author-name"]') || el.querySelector('.ui-chat__message__author');
        const textEl = el.querySelector('[data-tid="message-body"]') || el.querySelector('.ui-chat__message__content');
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
      // Find chat input and type
      const input = document.querySelector('textarea[aria-label*="Send a message" i], input[aria-label*="Send a message" i]');
      if (input) {
        input.focus();
        input.value = text;
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise(r => setTimeout(r, 500));
        // Press Enter
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true }));
      }
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
      const input = document.querySelector('[data-tid="meeting-chat-input"] [contenteditable="true"], textarea[placeholder*="Type" i]');
      if (input) {
        input.focus();
        input.textContent = text;
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise(r => setTimeout(r, 500));
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true }));
      }
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
      const input = dom.querySelector('textarea[placeholder*="Type" i], input[placeholder*="Type" i]');
      if (input) {
        input.focus();
        input.value = text;
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise(r => setTimeout(r, 500));
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true }));
      }
    "#
}

// ---------------------------------------------------------------------------
// STT transcription (reuses Jami pattern)
// ---------------------------------------------------------------------------

pub(crate) fn transcribe_audio_chunk(
    proxy_host: &str,
    proxy_port: u16,
    audio_path: &Path,
    stt_model: &str,
) -> Result<String> {
    let boundary = format!("----ctox-meeting-{}", now_epoch_millis());
    let file_bytes = fs::read(audio_path)
        .with_context(|| format!("failed to read audio chunk {}", audio_path.display()))?;
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            audio_path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("audio.webm")
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(&file_bytes);
    body.extend_from_slice(b"\r\n");
    if !stt_model.trim().is_empty() {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(stt_model.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type".to_string(),
        format!("multipart/form-data; boundary={boundary}"),
    );
    let response = super::communication_email_native::http_request(
        "POST",
        &format!(
            "http://{}:{}/v1/audio/transcriptions",
            proxy_host, proxy_port
        ),
        &headers,
        Some(&body),
    )?;
    if !(200..300).contains(&response.status) {
        bail!(
            "audio transcription returned HTTP {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        );
    }
    let parsed = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    Ok(parsed
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| parsed.get("transcript").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_string())
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
        });
        if !candidate.starts_with("https://") && !candidate.starts_with("http://") {
            continue;
        }
        let lower = candidate.to_lowercase();
        if MEETING_URL_PATTERNS.iter().any(|pat| lower.contains(pat)) {
            // Normalize: strip trailing fragments and tracking params
            let clean = candidate.split('#').next().unwrap_or(candidate).to_string();
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
    // Look for ICS DTSTART pattern: DTSTART:20260415T140000Z or DTSTART;TZID=...:20260415T140000
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("DTSTART") {
            // Extract the value after the last ':'
            if let Some(value) = trimmed.rsplit(':').next() {
                let value = value.trim();
                if value.len() >= 15 {
                    // Parse: 20260415T140000Z → 2026-04-15T14:00:00Z
                    let year = &value[0..4];
                    let month = &value[4..6];
                    let day = &value[6..8];
                    let hour = &value[9..11];
                    let min = &value[11..13];
                    let sec = &value[13..15];
                    let tz = if value.ends_with('Z') { "Z" } else { "" };
                    return Some(format!("{year}-{month}-{day}T{hour}:{min}:{sec}{tz}"));
                }
            }
        }
    }
    None
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

/// Schedule a meeting join via the CTOX schedule system.
/// Creates or updates a scheduled task that will fire at the meeting start time.
pub(crate) fn schedule_meeting_join(
    root: &Path,
    meeting_url: &str,
    meeting_time_iso: &str,
    bot_name: &str,
) -> Result<Value> {
    let provider =
        MeetingProvider::detect(meeting_url).context("cannot detect meeting provider from URL")?;
    let cron_expr = cron_for_meeting_time(meeting_time_iso)
        .context("cannot parse meeting time into cron expression")?;
    let schedule_name = meeting_schedule_name(meeting_url);
    let thread_key = format!("meeting:{}", provider.as_str());

    let prompt = format!(
        "Join the {provider} meeting at {url} as \"{bot_name}\". \
         Capture audio transcript and monitor chat. \
         If no other participants join within 15 minutes, leave the meeting. \
         After the meeting ends, summarize the transcript and create knowledge entries and tickets.",
        provider = provider.as_str(),
        url = meeting_url,
        bot_name = bot_name,
    );

    let request = super::schedule::ScheduleEnsureRequest {
        name: schedule_name.clone(),
        cron_expr,
        prompt,
        thread_key,
        skill: Some("system-onboarding".to_string()),
    };
    let task = super::schedule::ensure_task(root, request)?;

    // Also persist the meeting details for the join logic
    let sessions_dir = meeting_sessions_dir(root);
    fs::create_dir_all(&sessions_dir)?;
    let session_file = sessions_dir.join(format!("{}.json", schedule_name));
    let session_meta = json!({
        "schedule_name": schedule_name,
        "meeting_url": meeting_url,
        "meeting_time": meeting_time_iso,
        "provider": provider.as_str(),
        "bot_name": bot_name,
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
        "provider": provider.as_str(),
        "cron_expr": task.cron_expr,
        "next_run_at": task.next_run_at,
    }))
}

/// Cancel a scheduled meeting join.
pub(crate) fn cancel_meeting_join(root: &Path, meeting_url: &str) -> Result<Value> {
    let schedule_name = meeting_schedule_name(meeting_url);
    let session_file = meeting_sessions_dir(root).join(format!("{schedule_name}.json"));

    // Remove the scheduled task
    if let Err(err) = super::schedule::remove_task(
        root,
        &format!(
            "sched_{}",
            stable_digest(&format!(
                "{schedule_name}:meeting:{}",
                MeetingProvider::detect(meeting_url)
                    .map(|p| p.as_str())
                    .unwrap_or("unknown")
            ))
        ),
    ) {
        // Task may not exist — not fatal
        eprintln!("note: could not remove scheduled task: {err}");
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

    for url in &urls {
        if is_meeting_cancellation(subject, body) {
            let result = cancel_meeting_join(root, url)?;
            results.push(result);
            continue;
        }

        let meeting_time = extract_meeting_time_from_text(body);
        if let Some(ref time) = meeting_time {
            if is_meeting_update(subject, body) {
                // Update = cancel old + schedule new
                let _ = cancel_meeting_join(root, url);
            }
            let result = schedule_meeting_join(root, url, time, bot_name)?;
            results.push(result);
        } else {
            results.push(json!({
                "ok": false,
                "meeting_url": url,
                "reason": "meeting URL found but no start time detected",
            }));
        }
    }

    Ok(json!({
        "ok": true,
        "action": if results.is_empty() { "none" } else { "processed" },
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
    root.join("runtime").join("meeting_sessions")
}

fn meeting_session_file(root: &Path, session_id: &str) -> PathBuf {
    meeting_sessions_dir(root).join(format!("{session_id}.json"))
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
        serde_json::from_str(&contents).with_context(|| {
            format!("parse meeting session JSON at {}", session_path.display())
        })?
    } else {
        anyhow::bail!("no meeting session found with id {session_id}");
    };

    let transcript_path =
        meeting_sessions_dir(root).join(format!("{session_id}-transcript.txt"));
    let chatlog_path = meeting_sessions_dir(root).join(format!("{session_id}-chatlog.txt"));

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
            "@CTOX Notetaker hat geantwortet"
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
        // Port 1 is always unused for TCP in normal systems
        assert!(!check_engine_reachable("127.0.0.1", 1));
        // Invalid host
        assert!(!check_engine_reachable(
            "this-host-does-not-exist.invalid",
            80
        ));
    }

    #[test]
    fn self_loop_protection_filters_bot_messages() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp"),
            meeting_url: "https://zoom.us/j/123".to_string(),
            provider: MeetingProvider::Zoom,
            bot_name: "CTOX Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 8080,
            stt_model: String::new(),
        };
        let session = MeetingSession::new(&config);

        // Sender contains bot name → own message
        assert!(session.is_own_message("CTOX Notetaker", "hello"));
        assert!(session.is_own_message("CTOX Notetaker (Host)", "hello"));
        assert!(session.is_own_message("ctox notetaker", "hello"));

        // Real participants are not filtered
        assert!(!session.is_own_message("Michael Welsch", "@CTOX hello"));
        assert!(!session.is_own_message("Participant", "regular message"));

        // Sender misattributed to "Participant" but text starts with bot name + colon
        assert!(session.is_own_message("Participant", "CTOX Notetaker: I heard you"));
        assert!(session.is_own_message("Participant", "CTOX Notetaker To everyone: hi"));

        // Text mentions bot name but doesn't start with it (real user message)
        assert!(!session.is_own_message("Michael", "Hey @CTOX Notetaker what do you think?"));
    }

    #[test]
    fn session_roundtrip_json() {
        let config = MeetingSessionConfig {
            root: PathBuf::from("/tmp/test"),
            meeting_url: "https://meet.google.com/abc".to_string(),
            provider: MeetingProvider::GoogleMeet,
            bot_name: "CTOX Notetaker".to_string(),
            max_duration_minutes: 180,
            audio_chunk_seconds: 30,
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 8080,
            stt_model: String::new(),
        };
        let session = MeetingSession::new(&config);
        let json = session.to_json();
        assert_eq!(json["provider"], "google");
        assert_eq!(json["status"], "joining");
        assert_eq!(json["bot_name"], "CTOX Notetaker");
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
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 8080,
                stt_model: String::new(),
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
    fn detect_cancellation_and_update() {
        assert!(is_meeting_cancellation(
            "Meeting Canceled: Sprint Review",
            ""
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
            bot_name: "CTOX Notetaker".to_string(),
            max_duration_minutes: 60,
            audio_chunk_seconds: 30,
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 8080,
            stt_model: String::new(),
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
    }
}
