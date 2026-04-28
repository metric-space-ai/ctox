use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde_json::json;
use std::path::Path;

use crate::execution::agent::turn_loop;
use crate::governance;
use crate::lcm;
use crate::mission::channels;
use crate::mission::communication_gateway;

const DEFAULT_HISTORY_LIMIT: usize = 20;

pub fn handle_strategy_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    let db_path = root.join("runtime/ctox.sqlite3");
    let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
    match command {
        "show" => {
            let (conversation_id, thread_key) = resolve_scope(args)?;
            let snapshot = engine.active_strategy_snapshot(conversation_id, thread_key.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            Ok(())
        }
        "history" => {
            let (conversation_id, thread_key) = resolve_scope(args)?;
            let kind = find_flag_value(args, "--kind");
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(DEFAULT_HISTORY_LIMIT);
            let directives = engine.list_strategic_directives(
                conversation_id,
                thread_key.as_deref(),
                kind,
                limit,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "conversation_id": conversation_id,
                    "thread_key": thread_key,
                    "count": directives.len(),
                    "directives": directives,
                }))?
            );
            Ok(())
        }
        "set" => {
            let (conversation_id, thread_key) = resolve_scope(args)?;
            let kind = required_flag_value(args, "--kind")
                .context("usage: ctox strategy set --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--status <active|proposed>] [--triggered-by-inbound <message_key>]")?;
            let title = required_flag_value(args, "--title")
                .context("usage: ctox strategy set --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--status <active|proposed>] [--triggered-by-inbound <message_key>]")?;
            let body = required_body_value(args)?;
            let author = find_flag_value(args, "--author").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
            // Status is normally implicit `active` for the `set` subcommand,
            // but the inbound-driven authority gate may need to permit only
            // `proposed`. The flag is opt-in so existing operator-direct
            // invocations remain unchanged.
            let status = find_flag_value(args, "--status").unwrap_or("active");
            if !matches!(status, "active" | "proposed") {
                anyhow::bail!(
                    "--status must be `active` or `proposed`; got `{status}`"
                );
            }
            let triggered_by_inbound = find_flag_value(args, "--triggered-by-inbound");
            if let Some(message_key) = triggered_by_inbound {
                check_inbound_strategy_authority(
                    root,
                    &db_path,
                    InboundStrategyAuthorityCheck {
                        message_key,
                        directive_kind: kind,
                        attempted_status: status,
                        action: "set",
                        conversation_id: Some(conversation_id),
                        thread_key: thread_key.as_deref(),
                    },
                )?;
            }
            let record = engine.create_strategic_directive(
                conversation_id,
                thread_key.as_deref(),
                kind,
                title,
                &body,
                status,
                author,
                reason,
            )?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            Ok(())
        }
        "propose" => {
            let (conversation_id, thread_key) = resolve_scope(args)?;
            let kind = required_flag_value(args, "--kind")
                .context("usage: ctox strategy propose --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--triggered-by-inbound <message_key>]")?;
            let title = required_flag_value(args, "--title")
                .context("usage: ctox strategy propose --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--triggered-by-inbound <message_key>]")?;
            let body = required_body_value(args)?;
            let author = find_flag_value(args, "--author").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
            let triggered_by_inbound = find_flag_value(args, "--triggered-by-inbound");
            if let Some(message_key) = triggered_by_inbound {
                check_inbound_strategy_authority(
                    root,
                    &db_path,
                    InboundStrategyAuthorityCheck {
                        message_key,
                        directive_kind: kind,
                        attempted_status: "proposed",
                        action: "propose",
                        conversation_id: Some(conversation_id),
                        thread_key: thread_key.as_deref(),
                    },
                )?;
            }
            let record = engine.create_strategic_directive(
                conversation_id,
                thread_key.as_deref(),
                kind,
                title,
                &body,
                "proposed",
                author,
                reason,
            )?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            Ok(())
        }
        "activate" => {
            let directive_id = required_flag_value(args, "--directive-id")
                .context("usage: ctox strategy activate --directive-id <id> [--decided-by <name>] [--reason <text>] [--triggered-by-inbound <message_key>]")?;
            let decided_by = find_flag_value(args, "--decided-by").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
            let triggered_by_inbound = find_flag_value(args, "--triggered-by-inbound");
            if let Some(message_key) = triggered_by_inbound {
                // Pull the directive's kind for richer audit details. If the
                // directive is unknown the engine call below will surface a
                // clearer error; here we tolerate the missing record so the
                // gate still records the attempt.
                let directive_kind = engine
                    .list_strategic_directives(
                        turn_loop::conversation_id_for_thread_key(None),
                        None,
                        None,
                        256,
                    )
                    .ok()
                    .and_then(|directives| {
                        directives
                            .into_iter()
                            .find(|item| item.directive_id == directive_id)
                            .map(|item| item.directive_kind)
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                check_inbound_strategy_authority(
                    root,
                    &db_path,
                    InboundStrategyAuthorityCheck {
                        message_key,
                        directive_kind: &directive_kind,
                        attempted_status: "active",
                        action: "activate",
                        conversation_id: None,
                        thread_key: None,
                    },
                )?;
            }
            let record = engine.activate_strategic_directive(directive_id, decided_by, reason)?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            Ok(())
        }
        _ => anyhow::bail!(
            "usage:\n  ctox strategy show [--conversation-id <id>|--thread-key <key>]\n  ctox strategy history [--conversation-id <id>|--thread-key <key>] [--kind <kind>] [--limit <n>]\n  ctox strategy set --kind <vision|mission|...> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--status <active|proposed>] [--triggered-by-inbound <message_key>]\n  ctox strategy propose --kind <vision|mission|...> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>] [--triggered-by-inbound <message_key>]\n  ctox strategy activate --directive-id <id> [--decided-by <name>] [--reason <text>] [--triggered-by-inbound <message_key>]"
        ),
    }
}

/// Inbound-driven strategic-directive authority gate.
///
/// Looks up the inbound `communication_messages` row by `message_key`, reads
/// its `sender_address`, classifies the sender via
/// `channels::classify_email_sender`, and decides whether the requested
/// directive mutation is permitted. Operator-direct invocations (no
/// `--triggered-by-inbound`) bypass this gate entirely and keep their
/// existing authority.
struct InboundStrategyAuthorityCheck<'a> {
    message_key: &'a str,
    directive_kind: &'a str,
    attempted_status: &'a str,
    action: &'a str,
    conversation_id: Option<i64>,
    thread_key: Option<&'a str>,
}

fn check_inbound_strategy_authority(
    root: &Path,
    db_path: &Path,
    check: InboundStrategyAuthorityCheck<'_>,
) -> Result<()> {
    let lookup = lookup_inbound_sender(db_path, check.message_key)?;
    let settings = communication_gateway::runtime_settings_from_root(
        root,
        communication_gateway::CommunicationAdapterKind::Email,
    );
    let (sender_address, policy_role) = match lookup {
        Some(sender) => {
            let policy = channels::classify_email_sender(&settings, &sender);
            (sender, policy.role)
        }
        None => {
            // No matching message_key — treat as an external/unknown sender so
            // the gate cannot be bypassed by spoofing a non-existent key.
            (String::new(), "unknown_message_key".to_string())
        }
    };
    let allowed = match (policy_role.as_str(), check.attempted_status) {
        ("owner", _) => true,
        ("founder", "proposed") => true,
        ("admin", "proposed") => true,
        // founder/admin attempting `active` (set --status active or activate)
        // is blocked.
        // any other role attempting any mutation is blocked.
        _ => false,
    };
    let details = json!({
        "triggered_by_message_key": check.message_key,
        "sender_address": sender_address,
        "sender_role": policy_role,
        "directive_kind": check.directive_kind,
        "attempted_status": check.attempted_status,
        "action": check.action,
        "conversation_id": check.conversation_id,
        "thread_key": check.thread_key,
    });
    if allowed {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "strategic_directive_mutation_owner_authorised",
                conversation_id: check.conversation_id,
                severity: "info",
                reason: "inbound_sender_authority_permits_strategic_directive_mutation",
                action_taken: "permitted_strategic_directive_mutation",
                details,
                idempotence_key: Some(&format!(
                    "strategic_directive_authority::{}::{}::{}",
                    check.message_key, check.action, check.attempted_status
                )),
            },
        );
        Ok(())
    } else {
        let _ = governance::record_event(
            root,
            governance::GovernanceEventRequest {
                mechanism_id: "strategic_directive_mutation_blocked_non_owner_sender",
                conversation_id: check.conversation_id,
                severity: "critical",
                reason: "inbound_sender_authority_does_not_permit_strategic_directive_mutation",
                action_taken: "blocked_strategic_directive_mutation",
                details,
                idempotence_key: Some(&format!(
                    "strategic_directive_authority::{}::{}::{}",
                    check.message_key, check.action, check.attempted_status
                )),
            },
        );
        anyhow::bail!(
            "strategic directive mutation blocked: triggered-by-inbound sender role `{role}` may not perform `{verb}`; record as `propose` instead and request owner activation",
            role = policy_role,
            verb = if check.action == "activate" {
                "activate".to_string()
            } else {
                format!("set --status {}", check.attempted_status)
            },
        );
    }
}

fn lookup_inbound_sender(db_path: &Path, message_key: &str) -> Result<Option<String>> {
    let conn = Connection::open(db_path).with_context(|| {
        format!(
            "failed to open communication db {} for triggered-by-inbound lookup",
            db_path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .ok();
    // The communication_messages table is created lazily by the channel
    // module; if it does not exist yet, treat the lookup as a miss rather
    // than erroring (the gate then classifies the role as unknown and blocks).
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='communication_messages'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !table_exists {
        return Ok(None);
    }
    let sender = conn
        .query_row(
            "SELECT sender_address FROM communication_messages
             WHERE message_key = ?1 AND direction = 'inbound'
             LIMIT 1",
            params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(sender)
}

fn resolve_scope(args: &[String]) -> Result<(i64, Option<String>)> {
    let thread_key = find_flag_value(args, "--thread-key").map(ToOwned::to_owned);
    let conversation_id = find_flag_value(args, "--conversation-id")
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("failed to parse --conversation-id")?
        .unwrap_or_else(|| turn_loop::conversation_id_for_thread_key(thread_key.as_deref()));
    Ok((conversation_id, thread_key))
}

fn required_body_value(args: &[String]) -> Result<String> {
    if let Some(body) = find_flag_value(args, "--body") {
        return Ok(body.to_string());
    }
    if let Some(path) = find_flag_value(args, "--body-file") {
        return std::fs::read_to_string(path)
            .with_context(|| format!("failed to read strategy body from {}", path));
    }
    anyhow::bail!("missing --body or --body-file")
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("ctox-strategy-auth-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        root
    }

    fn write_env_settings(root: &std::path::Path, settings: &[(&str, &str)]) {
        // Seed the operator env directly into the `runtime_env_kv` table the
        // gate's `runtime_settings_from_root` reads from. We write the table
        // by hand rather than calling `save_runtime_env_map` so we do not
        // also have to provide a model configuration; only the addressing
        // settings (`CTOX_OWNER_EMAIL_ADDRESS`, `CTOX_FOUNDER_EMAIL_ADDRESSES`)
        // are relevant for these tests.
        let db_path = root.join("runtime/ctox.sqlite3");
        let conn = rusqlite::Connection::open(&db_path).expect("open settings db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runtime_env_kv (
                env_key TEXT PRIMARY KEY,
                env_value TEXT NOT NULL
            );",
        )
        .expect("create runtime_env_kv");
        for (key, value) in settings {
            conn.execute(
                "INSERT INTO runtime_env_kv (env_key, env_value)
                 VALUES (?1, ?2)
                 ON CONFLICT(env_key) DO UPDATE SET env_value = excluded.env_value",
                params![key, value],
            )
            .expect("insert kv");
        }
    }

    fn seed_inbound_message(root: &std::path::Path, message_key: &str, sender_address: &str) {
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn =
            crate::mission::channels::open_channel_db(&db_path).expect("open channel db");
        let upsert = crate::mission::channels::UpsertMessage {
            message_key,
            channel: "email",
            account_key: "email:cto@example.com",
            thread_key: "strategy-thread",
            remote_id: message_key,
            direction: "inbound",
            folder_hint: "Inbox",
            sender_display: sender_address,
            sender_address,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: "strategy authority test",
            preview: "",
            body_text: "",
            body_html: "",
            raw_payload_ref: "",
            trust_level: "trusted",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: "2026-04-27T10:00:00Z",
            observed_at: "2026-04-27T10:00:00Z",
            metadata_json: "{}",
        };
        crate::mission::channels::upsert_communication_message(&mut conn, upsert)
            .expect("upsert inbound message");
    }

    fn governance_events_with_mechanism(root: &std::path::Path, mechanism_id: &str) -> Vec<String> {
        // Read the raw `governance_events` table directly so we are not
        // bound to the per-conversation-id filter that
        // `governance::list_recent_events` applies. The governance schema
        // is created lazily by `record_event`; if no event was ever
        // recorded the table will be absent — treat that as "no events".
        let db_path = root.join("runtime/ctox.sqlite3");
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='governance_events'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .expect("sqlite_master query")
            .is_some();
        if !exists {
            return Vec::new();
        }
        let mut stmt = conn
            .prepare(
                "SELECT event_id FROM governance_events WHERE mechanism_id = ?1
                 ORDER BY CAST(created_at AS INTEGER) DESC",
            )
            .expect("prepare");
        let rows = stmt
            .query_map(params![mechanism_id], |row| row.get::<_, String>(0))
            .expect("query");
        rows.filter_map(Result::ok).collect()
    }

    fn run_set_active(root: &std::path::Path, message_key: Option<&str>, kind: &str) -> Result<()> {
        let mut args = vec![
            "set".to_string(),
            "--thread-key".to_string(),
            "strategy-thread".to_string(),
            "--kind".to_string(),
            kind.to_string(),
            "--title".to_string(),
            "test directive".to_string(),
            "--body".to_string(),
            "body text".to_string(),
            "--status".to_string(),
            "active".to_string(),
        ];
        if let Some(key) = message_key {
            args.push("--triggered-by-inbound".to_string());
            args.push(key.to_string());
        }
        handle_strategy_command(root, &args)
    }

    fn run_propose(root: &std::path::Path, message_key: &str, kind: &str) -> Result<()> {
        let args = vec![
            "propose".to_string(),
            "--thread-key".to_string(),
            "strategy-thread".to_string(),
            "--kind".to_string(),
            kind.to_string(),
            "--title".to_string(),
            "proposed directive".to_string(),
            "--body".to_string(),
            "body text".to_string(),
            "--triggered-by-inbound".to_string(),
            message_key.to_string(),
        ];
        handle_strategy_command(root, &args)
    }

    fn run_activate(
        root: &std::path::Path,
        directive_id: &str,
        message_key: Option<&str>,
    ) -> Result<()> {
        let mut args = vec![
            "activate".to_string(),
            "--directive-id".to_string(),
            directive_id.to_string(),
        ];
        if let Some(key) = message_key {
            args.push("--triggered-by-inbound".to_string());
            args.push(key.to_string());
        }
        handle_strategy_command(root, &args)
    }

    #[test]
    fn set_active_with_triggered_by_inbound_owner_succeeds() {
        let root = temp_root("owner-set-active");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        seed_inbound_message(&root, "owner-msg-1", "owner@example.com");
        run_set_active(&root, Some("owner-msg-1"), "mission").expect("owner set active");

        // Directive must exist and be active.
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).expect("engine");
        let conv = crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(
            "strategy-thread",
        ));
        let active = engine
            .active_strategic_directive(conv, Some("strategy-thread"), "mission")
            .expect("active mission");
        assert!(active.is_some(), "expected an active mission directive");

        let events = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_owner_authorised",
        );
        assert!(
            !events.is_empty(),
            "expected a strategic_directive_mutation_owner_authorised event"
        );

        let blocks = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            blocks.is_empty(),
            "did not expect a block event for owner sender"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_active_with_triggered_by_inbound_founder_blocks() {
        let root = temp_root("founder-set-active");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        seed_inbound_message(&root, "founder-msg-1", "founder@example.com");

        let err = run_set_active(&root, Some("founder-msg-1"), "mission")
            .expect_err("expected founder set active to be blocked");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("strategic directive mutation blocked"),
            "unexpected error: {msg}"
        );
        assert!(msg.contains("founder"), "error should name the role: {msg}");

        let events = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            !events.is_empty(),
            "expected a strategic_directive_mutation_blocked_non_owner_sender event"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_proposed_with_triggered_by_inbound_founder_succeeds() {
        let root = temp_root("founder-propose");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        seed_inbound_message(&root, "founder-msg-2", "founder@example.com");

        run_propose(&root, "founder-msg-2", "mission").expect("founder propose should succeed");

        let blocks = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            blocks.is_empty(),
            "did not expect a block event for founder propose"
        );

        // Directive should exist with status=proposed.
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).expect("engine");
        let conv = crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(
            "strategy-thread",
        ));
        let directives = engine
            .list_strategic_directives(conv, Some("strategy-thread"), Some("mission"), 16)
            .expect("list");
        assert!(
            directives.iter().any(|d| d.status == "proposed"),
            "expected a proposed directive"
        );

        let success = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_owner_authorised",
        );
        assert!(
            !success.is_empty(),
            "expected a positive audit event for permitted founder propose"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn activate_with_triggered_by_inbound_founder_blocks() {
        let root = temp_root("founder-activate");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        seed_inbound_message(&root, "founder-msg-3", "founder@example.com");

        // Seed an existing proposed directive directly via the engine so we
        // can target it for activation.
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).expect("engine");
        let conv = crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(
            "strategy-thread",
        ));
        let proposed = engine
            .create_strategic_directive(
                conv,
                Some("strategy-thread"),
                "mission",
                "proposed mission",
                "body",
                "proposed",
                "ctox",
                None,
            )
            .expect("seed proposed directive");
        drop(engine);

        let err = run_activate(&root, &proposed.directive_id, Some("founder-msg-3"))
            .expect_err("expected founder activate to be blocked");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("strategic directive mutation blocked"),
            "unexpected error: {msg}"
        );

        let events = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            !events.is_empty(),
            "expected a block event for founder activate"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_active_without_triggered_by_inbound_succeeds_as_operator_direct() {
        let root = temp_root("operator-direct");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        // Seed nothing in communication_messages — operator-direct path
        // never consults that table.

        run_set_active(&root, None, "mission").expect("operator-direct set active should succeed");

        // No new authority gate event should fire (neither block nor audit).
        let blocks = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            blocks.is_empty(),
            "operator-direct should not emit a block event"
        );
        let success = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_owner_authorised",
        );
        assert!(
            success.is_empty(),
            "operator-direct should not emit an authority audit event"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_active_with_triggered_by_inbound_external_sender_blocks() {
        let root = temp_root("external-set-active");
        write_env_settings(
            &root,
            &[
                ("CTOX_OWNER_EMAIL_ADDRESS", "owner@example.com"),
                ("CTOX_FOUNDER_EMAIL_ADDRESSES", "founder@example.com"),
            ],
        );
        seed_inbound_message(&root, "external-msg-1", "stranger@external.net");

        let err = run_set_active(&root, Some("external-msg-1"), "mission")
            .expect_err("expected external set active to be blocked");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("strategic directive mutation blocked"),
            "unexpected error: {msg}"
        );

        let blocks = governance_events_with_mechanism(
            &root,
            "strategic_directive_mutation_blocked_non_owner_sender",
        );
        assert!(
            !blocks.is_empty(),
            "expected a block event for external sender"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
