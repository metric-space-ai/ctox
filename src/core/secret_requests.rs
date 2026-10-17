//! Secret requests and reference-only secret use.
//!
//! An agent asks the user for a credential with `ctox secret request`. The
//! request row carries only metadata (scope, name, label, reason). A UI
//! fulfils it with `ctox secret fulfill --value-stdin`, which writes the value
//! straight into the encrypted secret store and answers with the reference
//! only. Agents then use the value through `ctox secret exec`, which injects it
//! into a child process environment and scrubs every exact occurrence of the
//! value from the child's output before it can reach a transcript or log.

use anyhow::{Context, Result};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use zeroize::Zeroizing;

use crate::secrets::{
    find_flag_value, get_secret_value, print_json, put_secret, resolve_secret_value_arg,
    secret_reference_text,
};

const REQUEST_USAGE: &str = "usage: ctox secret request --scope <scope> --name <name> --label <text> [--reason <text>] [--thread <key>]";
const FULFILL_USAGE: &str = "usage: ctox secret fulfill --request-id <id> --value-stdin";
const EXEC_USAGE: &str =
    "usage: ctox secret exec --env <VAR>=<scope>/<name> [--env ...] -- <command> [args...]";
/// Values shorter than this are refused by `exec`: scrubbing them would
/// mangle ordinary output and they are not credentials worth protecting.
const MIN_SCRUB_LEN: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SecretRequestView {
    pub request_id: String,
    pub scope: String,
    pub name: String,
    pub label: String,
    pub reason: Option<String>,
    pub thread_key: Option<String>,
    pub status: String,
    pub secret_ref: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn handle(root: &Path, command: &str, args: &[String]) -> Result<()> {
    match command {
        "request" => {
            let scope = find_flag_value(args, "--scope").context(REQUEST_USAGE)?;
            let name = find_flag_value(args, "--name").context(REQUEST_USAGE)?;
            let label = find_flag_value(args, "--label").context(REQUEST_USAGE)?;
            let request = create_request(
                root,
                scope,
                name,
                label,
                find_flag_value(args, "--reason"),
                find_flag_value(args, "--thread"),
            )?;
            print_json(&json!({
                "ok": true,
                "event": {"type": "secret_request", "request": request},
                "hint": "Wait until `ctox secret request-status` reports stored, then use the value only via `ctox secret exec --env VAR=<scope>/<name> -- <cmd>`."
            }))
        }
        "requests" => {
            let list = list_requests(
                root,
                find_flag_value(args, "--status"),
                find_flag_value(args, "--thread"),
            )?;
            print_json(&json!({"ok": true, "count": list.len(), "requests": list}))
        }
        "request-status" => {
            let id = find_flag_value(args, "--request-id")
                .context("usage: ctox secret request-status --request-id <id>")?;
            let request = request_by_id(root, id)?.context("secret request not found")?;
            print_json(&json!({"ok": true, "request": request}))
        }
        "fulfill" => {
            let id = find_flag_value(args, "--request-id").context(FULFILL_USAGE)?;
            let value = resolve_secret_value_arg(args, FULFILL_USAGE)?;
            let request = fulfill_request(root, id, value.as_str())?;
            // Only the reference leaves this command. Never the value.
            print_json(
                &json!({"ok": true, "request_id": request.request_id, "status": request.status, "ref": request.secret_ref}),
            )
        }
        "decline" => {
            let id = find_flag_value(args, "--request-id")
                .context("usage: ctox secret decline --request-id <id>")?;
            let request = decline_request(root, id)?;
            print_json(
                &json!({"ok": true, "request_id": request.request_id, "status": request.status}),
            )
        }
        "exec" => {
            let code = exec_with_secrets(root, args)?;
            std::process::exit(code);
        }
        _ => anyhow::bail!("unknown secret request command {command}"),
    }
}

// Installed once by the Secrets writer; reads never perform schema writes.
pub(crate) fn initialize_owned_request_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_secret_requests (
            request_id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            label TEXT NOT NULL,
            reason TEXT,
            thread_key TEXT,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_ctox_secret_requests_status
            ON ctox_secret_requests(status, created_at DESC);
        "#,
    )?;
    Ok(())
}
fn owner(root: &Path) -> Result<crate::lcm::owner_adapter::OwnerAdapter> {
    crate::persistence::require_sync_owner_context()?;
    Ok(crate::lcm::owner_adapter::OwnerAdapter::for_path(
        &crate::paths::secrets_db(root),
    )?)
}
fn request_by_id(root: &Path, id: &str) -> Result<Option<SecretRequestView>> {
    let id = id.to_owned();
    Ok(owner(root)?.read_blocking(move |conn| Ok(load_request(conn, &id)?))?)
}

fn validate_handle_part(kind: &str, value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
        "{kind} must be 1-128 chars of [A-Za-z0-9_.-]"
    );
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn new_request_id() -> Result<String> {
    let mut bytes = [0u8; 12];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("random source unavailable"))?;
    Ok(format!(
        "sreq_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

pub fn create_request(
    root: &Path,
    scope: &str,
    name: &str,
    label: &str,
    reason: Option<&str>,
    thread_key: Option<&str>,
) -> Result<SecretRequestView> {
    validate_handle_part("scope", scope)?;
    validate_handle_part("name", name)?;
    anyhow::ensure!(
        !label.trim().is_empty() && label.len() <= 200,
        "label must be 1-200 chars"
    );
    let owner = owner(root)?;
    let (scope, name, label, reason, thread_key) = (
        scope.to_owned(),
        name.to_owned(),
        label.trim().to_owned(),
        reason.map(str::to_owned),
        thread_key.map(str::to_owned),
    );
    let id = new_request_id()?;
    let ts = now();
    owner.write_blocking(move |conn| {
        conn.execute(
            "INSERT INTO ctox_secret_requests (request_id, scope, secret_name, label, reason, thread_key, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7)",
            params![id, scope, name, label.trim(), reason, thread_key, ts],
        )?;
        Ok(load_request(conn, &id)?)
    })?.context("secret request vanished after insert")
}

fn row_to_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<SecretRequestView> {
    let scope: String = row.get(1)?;
    let name: String = row.get(2)?;
    Ok(SecretRequestView {
        request_id: row.get(0)?,
        secret_ref: secret_reference_text(&scope, &name, None),
        scope,
        name,
        label: row.get(3)?,
        reason: row.get(4)?,
        thread_key: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

const SELECT: &str = "SELECT request_id, scope, secret_name, label, reason, thread_key, status, created_at, updated_at FROM ctox_secret_requests";

fn load_request(conn: &Connection, id: &str) -> rusqlite::Result<Option<SecretRequestView>> {
    Ok(conn
        .query_row(
            &format!("{SELECT} WHERE request_id = ?1"),
            params![id],
            row_to_view,
        )
        .optional()?)
}

pub fn list_requests(
    root: &Path,
    status: Option<&str>,
    thread_key: Option<&str>,
) -> Result<Vec<SecretRequestView>> {
    let (status, thread_key) = (status.map(str::to_owned), thread_key.map(str::to_owned));
    Ok(owner(root)?.read_blocking(move |conn| {
        let mut stmt = conn.prepare(&format!(
            "{SELECT} WHERE (?1 IS NULL OR status = ?1) AND (?2 IS NULL OR thread_key = ?2) ORDER BY created_at DESC LIMIT 200"
        ))?;
        let rows = stmt.query_map(params![status, thread_key], row_to_view)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })?)
}

pub fn fulfill_request(root: &Path, id: &str, value: &str) -> Result<SecretRequestView> {
    anyhow::ensure!(!value.is_empty(), "secret value must not be empty");
    let request = request_by_id(root, id)?.context("secret request not found")?;
    anyhow::ensure!(request.status != "declined", "secret request was declined");
    put_secret(
        root,
        &request.scope,
        &request.name,
        value,
        Some(request.label.clone()),
        json!({"source": "secret_request", "request_id": request.request_id}),
    )?;
    set_status(root, id, "stored")
}

pub fn decline_request(root: &Path, id: &str) -> Result<SecretRequestView> {
    let request = request_by_id(root, id)?.context("secret request not found")?;
    anyhow::ensure!(
        request.status == "pending",
        "secret request is already {}",
        request.status
    );
    set_status(root, id, "declined")
}

fn set_status(root: &Path, id: &str, status: &str) -> Result<SecretRequestView> {
    let (id, status) = (id.to_owned(), status.to_owned());
    owner(root)?.write_blocking(move |conn| {
        let changed = conn.execute("UPDATE ctox_secret_requests SET status = ?2, updated_at = ?3 WHERE request_id = ?1", params![id, status, now()])?;
        if changed != 1 { return Err(ctox_store::StoreError::Invalid("secret request not found".into())); }
        Ok(load_request(conn, &id)?)
    })?.context("secret request vanished after update")
}

/// Parses `--env VAR=scope/name` pairs and the command after `--`.
fn parse_exec_args(args: &[String]) -> Result<(Vec<(String, String, String)>, Vec<String>)> {
    let split = args.iter().position(|a| a == "--").context(EXEC_USAGE)?;
    let (head, tail) = (&args[..split], &args[split + 1..]);
    anyhow::ensure!(!tail.is_empty(), "{EXEC_USAGE}");
    let mut envs = Vec::new();
    let mut i = 0;
    while i < head.len() {
        anyhow::ensure!(head[i] == "--env" && i + 1 < head.len(), "{EXEC_USAGE}");
        let spec = &head[i + 1];
        let (var, handle) = spec.split_once('=').context(EXEC_USAGE)?;
        let (scope, name) = handle.split_once('/').context(EXEC_USAGE)?;
        anyhow::ensure!(
            !var.is_empty() && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "invalid env var name {var}"
        );
        validate_handle_part("scope", scope)?;
        validate_handle_part("name", name)?;
        envs.push((var.to_string(), scope.to_string(), name.to_string()));
        i += 2;
    }
    anyhow::ensure!(!envs.is_empty(), "{EXEC_USAGE}");
    Ok((envs, tail.to_vec()))
}

/// Replaces every exact occurrence of each secret value with its reference.
pub fn scrub(output: &[u8], secrets: &[(Zeroizing<String>, String)]) -> Vec<u8> {
    let mut sorted: Vec<&(Zeroizing<String>, String)> = secrets.iter().collect();
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut out = output.to_vec();
    for (value, reference) in sorted {
        let needle = value.as_bytes();
        if needle.is_empty() {
            continue;
        }
        let mut next = Vec::with_capacity(out.len());
        let mut i = 0;
        while i < out.len() {
            if out[i..].starts_with(needle) {
                next.extend_from_slice(reference.as_bytes());
                i += needle.len();
            } else {
                next.push(out[i]);
                i += 1;
            }
        }
        out = next;
    }
    out
}

fn exec_with_secrets(root: &Path, args: &[String]) -> Result<i32> {
    let (envs, command) = parse_exec_args(args)?;
    let mut secrets = Vec::new();
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    for (var, scope, name) in &envs {
        let value = Zeroizing::new(
            get_secret_value(root, scope, name)
                .with_context(|| format!("secret {scope}/{name} is not available"))?,
        );
        anyhow::ensure!(
            value.len() >= MIN_SCRUB_LEN,
            "secret {scope}/{name} is too short to scrub safely"
        );
        child.env(var, value.as_str());
        secrets.push((value, secret_reference_text(scope, name, None)));
    }
    let output = child
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {}", command[0]))?;
    std::io::stdout().write_all(&scrub(&output.stdout, &secrets))?;
    std::io::stderr().write_all(&scrub(&output.stderr, &secrets))?;
    std::io::stdout().flush()?;
    Ok(output.status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(_label: &str) -> std::path::PathBuf {
        // Keep the Unix owner socket below SUN_LEN even under lane TMPDIR.
        tempfile::Builder::new()
            .prefix("sr-")
            .tempdir()
            .unwrap()
            .keep()
    }

    fn grep_tree(dir: &Path, needle: &[u8]) -> Vec<String> {
        let mut hits = Vec::new();
        for entry in walk(dir) {
            if let Ok(bytes) = std::fs::read(&entry) {
                if bytes.windows(needle.len()).any(|w| w == needle) {
                    hits.push(entry.display().to_string());
                }
            }
        }
        hits
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }

    #[test]
    fn request_fulfill_roundtrip_returns_only_reference_and_never_persists_plaintext() -> Result<()>
    {
        let root = temp_root("roundtrip");
        let work = root.clone();
        let result = crate::persistence::run_cli_owned(&root, move || {
            let root = work;
            let canary = "CANARY-pw-7f3a9c1e5b";
            let request = create_request(
                &root,
                "mail",
                "cto1_password",
                "Passwort cto1",
                Some("IMAP"),
                Some("t1"),
            )?;
            assert_eq!(request.status, "pending");
            assert_eq!(list_requests(&root, Some("pending"), Some("t1"))?.len(), 1);
            let stored = fulfill_request(&root, &request.request_id, canary)?;
            assert_eq!(stored.status, "stored");
            assert_eq!(stored.secret_ref, "[secret-ref:mail/cto1_password]");
            let serialized = serde_json::to_string(&stored)?;
            assert!(!serialized.contains(canary));
            assert_eq!(get_secret_value(&root, "mail", "cto1_password")?, canary);
            // Grep canary: the value must not appear anywhere on disk in plaintext.
            let hits = grep_tree(&root, canary.as_bytes());
            assert!(hits.is_empty(), "plaintext secret found in {hits:?}");
            Ok(())
        });
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn declined_request_cannot_be_fulfilled() -> Result<()> {
        let root = temp_root("decline");
        let work = root.clone();
        let result = crate::persistence::run_cli_owned(&root, move || {
            let root = work;
            let request = create_request(&root, "s", "n", "Label", None, None)?;
            set_status(&root, &request.request_id, "declined")?;
            assert!(fulfill_request(&root, &request.request_id, "value-1234").is_err());
            Ok(())
        });
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn requests_use_injected_owner_and_preserve_admission() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().to_owned();
        let error = create_request(&path, "fixture", "owner", "Owner", None, None).unwrap_err();
        assert_eq!(
            error.downcast_ref::<crate::persistence::CoreSyncError>(),
            Some(&crate::persistence::CoreSyncError::Required)
        );
        let work = path.clone();
        crate::persistence::run_cli_owned(&path, move || {
            let owner = owner(&work)?;
            // TEMP objects exist only on the injected writer. A second opener
            // can insert a row but cannot fire this connection's trigger.
            owner.write_blocking(|conn| {
                conn.execute_batch("CREATE TEMP TABLE request_owner_audit(value INTEGER); CREATE TEMP TRIGGER request_owner_insert AFTER INSERT ON main.ctox_secret_requests BEGIN INSERT INTO request_owner_audit VALUES(1); END;")?;
                Ok(())
            })?;
            let request = create_request(&work, "fixture", "owner", "Owner", None, None)?;
            let hits: i64 = owner.write_blocking(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM request_owner_audit", [], |r| r.get(0))?)
            })?;
            assert_eq!(hits, 1, "request bypassed the injected Secrets writer");
            let stored = fulfill_request(&work, &request.request_id, "synthetic-owner-canary")?;
            assert_eq!(stored.status, "stored");
            assert_eq!(list_requests(&work, Some("stored"), None)?, vec![stored]);
            Ok(())
        })?;
        let error = create_request(&path, "fixture", "after", "After", None, None).unwrap_err();
        assert_eq!(
            error.downcast_ref::<crate::persistence::CoreSyncError>(),
            Some(&crate::persistence::CoreSyncError::Required)
        );
        Ok(())
    }

    #[test]
    fn rejects_bad_handles() {
        let root = temp_root("handles");
        assert!(create_request(&root, "a/b", "n", "L", None, None).is_err());
        assert!(create_request(&root, "s", "", "L", None, None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scrub_replaces_every_exact_occurrence_longest_first() {
        let secrets = vec![
            (
                Zeroizing::new("abcd".to_string()),
                "[secret-ref:s/short]".to_string(),
            ),
            (
                Zeroizing::new("abcdEFGH".to_string()),
                "[secret-ref:s/long]".to_string(),
            ),
        ];
        let out = scrub(b"x abcdEFGH y abcd z abcdabcd", &secrets);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "x [secret-ref:s/long] y [secret-ref:s/short] z [secret-ref:s/short][secret-ref:s/short]"
        );
    }

    #[test]
    fn parse_exec_args_requires_env_and_command() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (envs, cmd) =
            parse_exec_args(&a(&["--env", "PW=mail/cto1", "--", "sh", "-c", "x"])).unwrap();
        assert_eq!(envs, vec![("PW".into(), "mail".into(), "cto1".into())]);
        assert_eq!(cmd, a(&["sh", "-c", "x"]));
        assert!(parse_exec_args(&a(&["--", "sh"])).is_err());
        assert!(parse_exec_args(&a(&["--env", "PW=mail/cto1"])).is_err());
        assert!(parse_exec_args(&a(&["--env", "P-W=mail/cto1", "--", "sh"])).is_err());
    }
}
