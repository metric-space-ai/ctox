use anyhow::Context;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;
use zeroize::Zeroizing;

use crate::lcm;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/cto_agent.db";
const MASTER_KEY_ENV: &str = "CTOX_SECRET_MASTER_KEY";
const MASTER_KEY_RELATIVE_PATH: &str = "runtime/ctox_secret_master.key";

type SecretMaterial = Zeroizing<Vec<u8>>;

#[derive(Debug, Clone, Serialize)]
pub struct SecretRecordView {
    pub secret_id: String,
    pub scope: String,
    pub secret_name: String,
    pub description: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct SecretIntakeView {
    pub secret: SecretRecordView,
    pub rewrite: Option<lcm::SecretRewriteResult>,
}

pub fn handle_secret_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_secret_db(root)?;
            ensure_secret_schema(&conn)?;
            let key_source = ensure_secret_master_key(root)?.1;
            print_json(&json!({"ok": true, "db_path": resolve_db_path(root), "key_source": key_source}))
        }
        "put" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let value = required_flag_value(args, "--value")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let description = find_flag_value(args, "--description").map(str::to_string);
            let metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let record = put_secret(root, scope, name, value, description, metadata)?;
            print_json(&json!({"ok": true, "secret": record}))
        }
        "intake" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let value = required_flag_value(args, "--value")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let description = find_flag_value(args, "--description").map(str::to_string);
            let metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let rewrite = parse_intake_rewrite_request(args)?;
            let intake = intake_secret(root, scope, name, value, description, metadata, rewrite)?;
            print_json(&json!({"ok": true, "intake": intake}))
        }
        "list" => {
            let scope = find_flag_value(args, "--scope");
            let records = list_secrets(root, scope)?;
            print_json(&json!({"ok": true, "count": records.len(), "secrets": records}))
        }
        "show" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret show --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret show --scope <scope> --name <name>")?;
            let record = load_secret_record(root, scope, name)?.context("secret not found")?;
            print_json(&json!({"ok": true, "secret": record}))
        }
        "get" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret get --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret get --scope <scope> --name <name>")?;
            let value = get_secret_value(root, scope, name)?;
            print_json(&json!({"ok": true, "scope": scope, "name": name, "value": value}))
        }
        "delete" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret delete --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret delete --scope <scope> --name <name>")?;
            delete_secret(root, scope, name)?;
            print_json(&json!({"ok": true, "scope": scope, "name": name, "deleted": true}))
        }
        "memory-rewrite" => {
            let db_path = required_flag_value(args, "--db")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let conversation_id = required_flag_value(args, "--conversation-id")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?
                .parse::<i64>()
                .context("failed to parse conversation id")?;
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let match_text = required_flag_value(args, "--match-text")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            anyhow::ensure!(
                secret_exists(root, scope, name)?,
                "secret {scope}/{name} does not exist in the local secret store"
            );
            let replacement = secret_reference_text(
                scope,
                name,
                find_flag_value(args, "--label"),
            );
            let result = lcm::run_secret_rewrite(
                Path::new(db_path),
                conversation_id,
                scope,
                name,
                match_text,
                &replacement,
            )?;
            print_json(&json!({"ok": true, "rewrite": result}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox secret init\n  ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]\n  ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]\n  ctox secret list [--scope <scope>]\n  ctox secret show --scope <scope> --name <name>\n  ctox secret get --scope <scope> --name <name>\n  ctox secret delete --scope <scope> --name <name>\n  ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]"
        ),
    }
}

#[derive(Debug, Clone)]
struct IntakeRewriteRequest {
    db_path: PathBuf,
    conversation_id: i64,
    match_text: String,
    label: Option<String>,
}

fn parse_intake_rewrite_request(args: &[String]) -> Result<Option<IntakeRewriteRequest>> {
    let db_path = find_flag_value(args, "--db");
    let conversation_id = find_flag_value(args, "--conversation-id");
    let match_text = find_flag_value(args, "--match-text");
    let label = find_flag_value(args, "--label").map(str::to_string);

    if db_path.is_none() && conversation_id.is_none() && match_text.is_none() {
        return Ok(None);
    }

    let db_path = db_path.context(
        "ctox secret intake requires --db together with --conversation-id and --match-text when memory rewrite is requested",
    )?;
    let conversation_id = conversation_id
        .context(
            "ctox secret intake requires --conversation-id together with --db and --match-text when memory rewrite is requested",
        )?
        .parse::<i64>()
        .context("failed to parse conversation id")?;
    let match_text = match_text
        .context(
            "ctox secret intake requires --match-text together with --db and --conversation-id when memory rewrite is requested",
        )?
        .to_string();

    Ok(Some(IntakeRewriteRequest {
        db_path: PathBuf::from(db_path),
        conversation_id,
        match_text,
        label,
    }))
}

fn intake_secret(
    root: &Path,
    scope: &str,
    name: &str,
    value: &str,
    description: Option<String>,
    metadata: Value,
    rewrite: Option<IntakeRewriteRequest>,
) -> Result<SecretIntakeView> {
    let record = put_secret(root, scope, name, value, description, metadata)?;
    let rewrite_result = match rewrite {
        Some(rewrite) => {
            let replacement = secret_reference_text(scope, name, rewrite.label.as_deref());
            Some(lcm::run_secret_rewrite(
                &rewrite.db_path,
                rewrite.conversation_id,
                scope,
                name,
                &rewrite.match_text,
                &replacement,
            )?)
        }
        None => None,
    };
    Ok(SecretIntakeView {
        secret: record,
        rewrite: rewrite_result,
    })
}

pub fn secret_exists(root: &Path, scope: &str, name: &str) -> Result<bool> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ctox_secret_records WHERE scope = ?1 AND secret_name = ?2)",
        params![scope, name],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn put_secret(
    root: &Path,
    scope: &str,
    name: &str,
    value: &str,
    description: Option<String>,
    metadata: Value,
) -> Result<SecretRecordView> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let (key_bytes, _) = ensure_secret_master_key(root)?;
    let encrypted = encrypt_secret_value(&key_bytes, value.as_bytes())?;
    let now = now_iso_string();
    let secret_id = format!("secret:{}:{}", scope, stable_digest(name));
    conn.execute(
        r#"
        INSERT INTO ctox_secret_records (
            secret_id, scope, secret_name, description, metadata_json,
            nonce_b64, ciphertext_b64, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
        ON CONFLICT(scope, secret_name) DO UPDATE SET
            description=excluded.description,
            metadata_json=excluded.metadata_json,
            nonce_b64=excluded.nonce_b64,
            ciphertext_b64=excluded.ciphertext_b64,
            updated_at=excluded.updated_at
        "#,
        params![
            secret_id,
            scope,
            name,
            description,
            serde_json::to_string(&metadata)?,
            encrypted.nonce_b64,
            encrypted.ciphertext_b64,
            now,
        ],
    )?;
    load_secret_record(root, scope, name)?.context("secret metadata vanished after write")
}

fn list_secrets(root: &Path, scope: Option<&str>) -> Result<Vec<SecretRecordView>> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let mut statement = conn.prepare(
        r#"
        SELECT secret_id, scope, secret_name, description, metadata_json, created_at, updated_at
        FROM ctox_secret_records
        WHERE (?1 IS NULL OR scope = ?1)
        ORDER BY updated_at DESC
        "#,
    )?;
    let rows = statement.query_map(params![scope], map_secret_record_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_secret_record(root: &Path, scope: &str, name: &str) -> Result<Option<SecretRecordView>> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let record = conn
        .query_row(
            r#"
            SELECT secret_id, scope, secret_name, description, metadata_json, created_at, updated_at
            FROM ctox_secret_records
            WHERE scope = ?1 AND secret_name = ?2
            LIMIT 1
            "#,
            params![scope, name],
            map_secret_record_row,
        )
        .optional()?;
    Ok(record)
}

fn get_secret_value(root: &Path, scope: &str, name: &str) -> Result<String> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let (nonce_b64, ciphertext_b64): (String, String) = conn
        .query_row(
            r#"
            SELECT nonce_b64, ciphertext_b64
            FROM ctox_secret_records
            WHERE scope = ?1 AND secret_name = ?2
            LIMIT 1
            "#,
            params![scope, name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("secret not found")?;
    let (key_bytes, _) = ensure_secret_master_key(root)?;
    let value = decrypt_secret_value(&key_bytes, &nonce_b64, &ciphertext_b64)?;
    std::str::from_utf8(&value)
        .map(str::to_owned)
        .context("secret value is not valid UTF-8")
}

fn delete_secret(root: &Path, scope: &str, name: &str) -> Result<()> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    conn.execute(
        "DELETE FROM ctox_secret_records WHERE scope = ?1 AND secret_name = ?2",
        params![scope, name],
    )?;
    Ok(())
}

fn resolve_db_path(root: &Path) -> PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn open_secret_db(root: &Path) -> Result<Connection> {
    let db_path = resolve_db_path(root);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create secret DB directory {}", parent.display())
        })?;
    }
    Connection::open(&db_path).with_context(|| format!("failed to open {}", db_path.display()))
}

fn ensure_secret_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_secret_records (
            secret_id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            description TEXT,
            metadata_json TEXT NOT NULL,
            nonce_b64 TEXT NOT NULL,
            ciphertext_b64 TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, secret_name)
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_secret_scope
            ON ctox_secret_records(scope, updated_at DESC);
        "#,
    )?;
    Ok(())
}

fn ensure_secret_master_key(root: &Path) -> Result<(SecretMaterial, &'static str)> {
    if let Ok(value) = std::env::var(MASTER_KEY_ENV) {
        let bytes = BASE64_STANDARD
            .decode(value.trim())
            .context("failed to decode CTOX_SECRET_MASTER_KEY base64")?;
        if bytes.len() != 32 {
            anyhow::bail!("CTOX_SECRET_MASTER_KEY must decode to exactly 32 bytes");
        }
        return Ok((Zeroizing::new(bytes), "env"));
    }

    let path = root.join(MASTER_KEY_RELATIVE_PATH);
    if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let bytes = BASE64_STANDARD
            .decode(raw.trim())
            .with_context(|| format!("failed to decode {}", path.display()))?;
        if bytes.len() != 32 {
            anyhow::bail!("{} does not contain a 32-byte base64 key", path.display());
        }
        return Ok((Zeroizing::new(bytes), "local_file"));
    }

    let mut key = Zeroizing::new(vec![0u8; 32]);
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| anyhow::anyhow!("failed to generate secret master key"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, format!("{}\n", BASE64_STANDARD.encode(&key)))
        .with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&path, permissions)?;
    }
    Ok((key, "generated_local_file"))
}

struct EncryptedSecretValue {
    nonce_b64: String,
    ciphertext_b64: String,
}

fn encrypt_secret_value(key_bytes: &[u8], plaintext: &[u8]) -> Result<EncryptedSecretValue> {
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct secret encryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce_bytes = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption nonce"))?;
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
    let mut buffer = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buffer)
        .map_err(|_| anyhow::anyhow!("failed to encrypt secret value"))?;
    let ciphertext_b64 = BASE64_STANDARD.encode(buffer.as_slice());
    buffer.zeroize();
    Ok(EncryptedSecretValue {
        nonce_b64: BASE64_STANDARD.encode(nonce_bytes),
        ciphertext_b64,
    })
}

fn decrypt_secret_value(
    key_bytes: &[u8],
    nonce_b64: &str,
    ciphertext_b64: &str,
) -> Result<SecretMaterial> {
    let nonce_bytes = BASE64_STANDARD
        .decode(nonce_b64)
        .context("failed to decode secret nonce")?;
    let nonce_array: [u8; 12] = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("decoded secret nonce must be 12 bytes"))?;
    let mut ciphertext = BASE64_STANDARD
        .decode(ciphertext_b64)
        .context("failed to decode secret ciphertext")?;
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct secret decryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce_array),
            aead::Aad::empty(),
            &mut ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("failed to decrypt secret value"))?;
    let plaintext_copy = Zeroizing::new(plaintext.to_vec());
    ciphertext.zeroize();
    Ok(plaintext_copy)
}

fn map_secret_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SecretRecordView> {
    let metadata_raw: String = row.get(4)?;
    Ok(SecretRecordView {
        secret_id: row.get(0)?,
        scope: row.get(1)?,
        secret_name: row.get(2)?,
        description: row.get(3)?,
        metadata: serde_json::from_str(&metadata_raw).unwrap_or_else(|_| json!({})),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2).find_map(|window| {
        if window[0] == flag {
            Some(window[1].as_str())
        } else {
            None
        }
    })
}

fn parse_json_value(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse JSON value: {raw}"))
}

fn stable_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest[..12])
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn now_iso_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn secret_reference_text(scope: &str, name: &str, label: Option<&str>) -> String {
    let handle = format!("{scope}/{name}");
    match label.map(str::trim).filter(|value| !value.is_empty()) {
        Some(label) => format!("[secret-ref:{handle} label={label}]"),
        None => format!("[secret-ref:{handle}]"),
    }
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

// ── Credential helpers for runtime_env integration ──────────────────────
//
// These functions provide a thin API for storing and retrieving API keys
// and other credentials in the encrypted SQLite secret store instead of
// plaintext engine.env.  Scope is always "credentials".

const CREDENTIAL_SCOPE: &str = "credentials";

/// Keys that must be stored encrypted (never in plaintext engine.env).
const SECRET_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "OPENROUTER_API_KEY",
    "MINIMAX_API_KEY",
    "CTO_EMAIL_PASSWORD",
    "CTOX_WEBRTC_PASSWORD",
    "HF_TOKEN",
    "HUGGINGFACE_HUB_TOKEN",
];

/// Returns true if `key` is a credential that must be stored encrypted.
pub fn is_secret_key(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Store a credential value in the encrypted secret store.
pub fn set_credential(root: &Path, key: &str, value: &str) -> Result<()> {
    put_secret(
        root,
        CREDENTIAL_SCOPE,
        key,
        value,
        Some(format!("{key} (auto-managed)")),
        json!({"source": "runtime_env"}),
    )?;
    Ok(())
}

/// Retrieve a credential value from the encrypted secret store.
/// Returns None if the key does not exist or on any error (fail-open for
/// migration: caller falls back to engine.env / process env).
pub fn get_credential(root: &Path, key: &str) -> Option<String> {
    get_secret_value(root, CREDENTIAL_SCOPE, key).ok()
}

/// Delete a credential from the encrypted secret store.
pub fn delete_credential(root: &Path, key: &str) -> Result<()> {
    delete_secret(root, CREDENTIAL_SCOPE, key)
}

/// Migrate secrets from a plaintext env map into the encrypted store.
/// Returns the number of keys migrated.
pub fn migrate_secrets_from_env_map(
    root: &Path,
    env_map: &mut std::collections::BTreeMap<String, String>,
) -> usize {
    let mut migrated = 0;
    let secret_entries: Vec<(String, String)> = env_map
        .iter()
        .filter(|(k, v)| is_secret_key(k) && !v.trim().is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (key, value) in &secret_entries {
        // Only migrate if not already in the encrypted store.
        if get_credential(root, key).is_some() {
            // Already encrypted — just remove from plaintext map.
            env_map.remove(key.as_str());
            migrated += 1;
            continue;
        }
        match set_credential(root, key, value) {
            Ok(()) => {
                env_map.remove(key.as_str());
                migrated += 1;
                eprintln!("[secrets] migrated {key} from engine.env to encrypted store");
            }
            Err(e) => {
                eprintln!("[secrets] failed to migrate {key}: {e:#} — keeping in engine.env");
            }
        }
    }
    migrated
}

/// Merge encrypted credentials back into an env map so callers see a
/// unified view.  Existing entries in the map are NOT overwritten (process
/// env or engine.env take precedence when already present).
pub fn merge_credentials_into_env_map(
    root: &Path,
    env_map: &mut std::collections::BTreeMap<String, String>,
) {
    for &key in SECRET_KEYS {
        if env_map.contains_key(key) {
            continue; // already populated (process env or engine.env residual)
        }
        if let Some(value) = get_credential(root, key) {
            if !value.trim().is_empty() {
                env_map.insert(key.to_string(), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ctox-secret-test-{}-{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn secret_store_encrypts_values_at_rest_and_round_trips() -> Result<()> {
        let root = temp_root("roundtrip");
        fs::create_dir_all(&root)?;

        let record = put_secret(
            &root,
            "ticket:zammad",
            "api-token",
            "super-secret-token",
            Some("Zammad API token".to_string()),
            json!({"kind": "token"}),
        )?;
        assert_eq!(record.secret_name, "api-token");
        assert!(secret_exists(&root, "ticket:zammad", "api-token")?);

        let plaintext = get_secret_value(&root, "ticket:zammad", "api-token")?;
        assert_eq!(plaintext, "super-secret-token");

        let db_path = resolve_db_path(&root);
        let raw = fs::read(&db_path)?;
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(!raw_text.contains("super-secret-token"));

        let records = list_secrets(&root, Some("ticket:zammad"))?;
        assert_eq!(records.len(), 1);

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn secret_intake_stores_secret_and_rewrites_memory_reference() -> Result<()> {
        let root = temp_root("intake");
        fs::create_dir_all(&root)?;
        let lcm_db = root.join("runtime").join("ctox_lcm.db");
        if let Some(parent) = lcm_db.parent() {
            fs::create_dir_all(parent)?;
        }
        let engine = lcm::LcmEngine::open(&lcm_db, lcm::LcmConfig::default())?;
        engine.add_message(
            51,
            "user",
            "Please use sk-live-super-secret for the monitoring API",
        )?;
        drop(engine);

        let intake = intake_secret(
            &root,
            "monitoring",
            "api-token",
            "sk-live-super-secret",
            Some("Monitoring API token".to_string()),
            json!({"source": "user_message"}),
            Some(IntakeRewriteRequest {
                db_path: lcm_db.clone(),
                conversation_id: 51,
                match_text: "sk-live-super-secret".to_string(),
                label: Some("monitoring api token".to_string()),
            }),
        )?;

        assert_eq!(intake.secret.scope, "monitoring");
        let rewrite = intake.rewrite.context("expected memory rewrite result")?;
        assert_eq!(rewrite.message_rows_updated, 1);
        assert_eq!(
            get_secret_value(&root, "monitoring", "api-token")?,
            "sk-live-super-secret"
        );

        let snapshot = lcm::run_dump(&lcm_db, 51)?;
        assert!(snapshot.messages[0]
            .content
            .contains("[secret-ref:monitoring/api-token label=monitoring api token]"));
        assert!(!snapshot.messages[0]
            .content
            .contains("sk-live-super-secret"));

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
