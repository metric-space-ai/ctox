use anyhow::Context;
use anyhow::Result;
use std::path::Path;

use crate::inference::turn_loop;
use crate::lcm;

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
                .context("usage: ctox strategy set --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]")?;
            let title = required_flag_value(args, "--title")
                .context("usage: ctox strategy set --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]")?;
            let body = required_body_value(args)?;
            let author = find_flag_value(args, "--author").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
            let record = engine.create_strategic_directive(
                conversation_id,
                thread_key.as_deref(),
                kind,
                title,
                &body,
                "active",
                author,
                reason,
            )?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            Ok(())
        }
        "propose" => {
            let (conversation_id, thread_key) = resolve_scope(args)?;
            let kind = required_flag_value(args, "--kind")
                .context("usage: ctox strategy propose --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]")?;
            let title = required_flag_value(args, "--title")
                .context("usage: ctox strategy propose --kind <vision|mission|...> --title <text> --body <text>|--body-file <path> [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]")?;
            let body = required_body_value(args)?;
            let author = find_flag_value(args, "--author").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
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
                .context("usage: ctox strategy activate --directive-id <id> [--decided-by <name>] [--reason <text>]")?;
            let decided_by = find_flag_value(args, "--decided-by").unwrap_or("ctox");
            let reason = find_flag_value(args, "--reason");
            let record = engine.activate_strategic_directive(directive_id, decided_by, reason)?;
            println!("{}", serde_json::to_string_pretty(&record)?);
            Ok(())
        }
        _ => anyhow::bail!(
            "usage:\n  ctox strategy show [--conversation-id <id>|--thread-key <key>]\n  ctox strategy history [--conversation-id <id>|--thread-key <key>] [--kind <kind>] [--limit <n>]\n  ctox strategy set --kind <vision|mission|...> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]\n  ctox strategy propose --kind <vision|mission|...> --title <text> (--body <text>|--body-file <path>) [--conversation-id <id>|--thread-key <key>] [--author <name>] [--reason <text>]\n  ctox strategy activate --directive-id <id> [--decided-by <name>] [--reason <text>]"
        ),
    }
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
