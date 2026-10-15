// Origin: CTOX
// License: Apache-2.0

use super::session::{session_role, session_user_id, BusinessOsSession, BusinessOsSessionUser};
use super::sqlite_file_digest::MaintenanceOwner;
use super::store::{
    hex_sha256, now_ms, open_store, outbound_load_record, resolve_business_os_installed_app_root,
    rxdb_collection_table_name, rxdb_store_path, short_hash, source_sanitize_slug,
    sqlite_table_exists, sqlite_table_row_count, support_artifact_forbidden_paths,
    validate_business_audit_retention_days, BusinessAuditRetentionPolicyPayload,
    BusinessBackupRestoreDrillRequest, BusinessCommand, CommandOrigin, BUSINESS_AUDIT_EXPORT_DIR,
    BUSINESS_OS_AUDIT_RETENTION_POLICY_PAYLOAD_KEY, BUSINESS_OS_MCP_POLICY_PAYLOAD_KEY,
    BUSINESS_OS_SECRET_SCOPE, DEFAULT_BUSINESS_AUDIT_RETENTION_DAYS, RXDB_STORE_FILE, STORE_FILE,
};
use anyhow::Context;
use base64::Engine;
use ring::aead;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

pub(super) const BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME: &str =
    "backup_manifest_signing_key_v1";
pub(super) const BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME: &str =
    "portable_backup_encryption_key_v1";
const BUSINESS_OS_BACKUP_KEY_STORE_DIR: &str = ".ctox-backup-key-stores";
pub(super) const BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION: i64 = 1;
pub(super) const BUSINESS_OS_BACKUP_RAW_RETENTION_DAYS: i64 = 14;
const BUSINESS_OS_BACKUP_PORTABLE_CHUNK_SIZE: usize = 4 * 1024 * 1024;
const BUSINESS_OS_BACKUP_PORTABLE_MAGIC: &[u8] = b"CTOX-BOS-PORTABLE-BACKUP-V1\n";
pub(super) const BUSINESS_RESTORE_DRILL_EXPORT_DIR: &str = "runtime/business-os/restore-drills";

pub(super) fn business_os_backup_restore_drill_export(
    root: &Path,
    session: &BusinessOsSession,
    request: &BusinessBackupRestoreDrillRequest,
) -> anyhow::Result<Value> {
    let now = now_ms() as i64;
    let module_id = source_sanitize_slug(&request.module_id);
    let module_scope = if module_id.is_empty() {
        None
    } else {
        Some(module_id.as_str())
    };
    let conn = open_store(root)?;
    let native_tables = business_os_restore_drill_native_tables(&conn)?;
    let releases = business_os_restore_drill_release_summary(&conn, module_scope)?;
    drop(conn);
    let service_state = business_os_restore_drill_service_state(root)?;
    let installed_modules = business_os_restore_drill_installed_modules(root, module_scope)?;
    let source_snapshots = business_os_restore_drill_source_snapshots(root, module_scope)?;
    let audit_exports = business_os_restore_drill_json_dir_summary(
        root,
        Path::new(BUSINESS_AUDIT_EXPORT_DIR),
        "audit_retention_exports",
    )?;
    let rxdb = business_os_restore_drill_rxdb_summary(root)?;
    let required_tables_present = native_tables
        .get("required_tables_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let release_count = releases
        .get("release_count")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let release_record_count = releases
        .get("business_record_projection_count")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let module_manifest_count = installed_modules
        .get("manifest_count")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let rxdb_catalog_present = rxdb
        .get("module_catalog_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let typed_mcp_policy_present = service_state
        .get("typed_mcp_policy_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let typed_audit_retention_policy_present = service_state
        .get("typed_audit_retention_policy_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let typed_audit_retention_policy_valid = service_state
        .get("typed_audit_retention_policy_valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let native_audit_retention_days = service_state
        .get("native_audit_retention_days")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_BUSINESS_AUDIT_RETENTION_DAYS);
    let source_snapshot_count = source_snapshots
        .get("file_count")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let audit_export_count = audit_exports
        .get("file_count")
        .and_then(Value::as_i64)
        .unwrap_or_default();

    let checks = vec![
        business_os_restore_drill_check(
            "native_store_tables",
            "Native Business-OS Tabellen sind vorhanden",
            required_tables_present,
            "blocking",
            "Restore kann Rollen, Grants, Commands, Releases und Audit nur pruefen, wenn die Kern-Tabellen existieren.",
        ),
        business_os_restore_drill_check(
            "release_business_record_projection",
            "Release-Projektionen in business_records sind vollstaendig",
            release_record_count >= release_count,
            "blocking",
            "Release-Rows muessen nach Restore auch in business_records fuer Sync/Support sichtbar sein.",
        ),
        business_os_restore_drill_check(
            "rxdb_module_catalog_projection",
            "RxDB Modul-Katalog-Projektion ist vorhanden",
            rxdb_catalog_present || release_count == 0,
            "blocking",
            "Die Shell liest App-Sichtbarkeit und Lifecycle ueber die RxDB-Katalogprojektion.",
        ),
        business_os_restore_drill_check(
            "installed_module_manifests",
            "Installierte App-Manifeste sind sichtbar",
            module_manifest_count > 0,
            if module_scope.is_some() {
                "blocking"
            } else {
                "warning"
            },
            "Dynamische Apps brauchen ihre runtime/business-os/installed-modules Manifeste und Assets.",
        ),
        business_os_restore_drill_check(
            "typed_mcp_policy_state",
            "Typisierte MCP-Policy ist im Service-State vorhanden",
            typed_mcp_policy_present,
            "warning",
            "MCP-Retention und Scope-Policy sind produktiv als business_os.mcp_policy.v1 gedacht; Legacy-Env ist nur Migration.",
        ),
        business_os_restore_drill_check(
            "typed_audit_retention_policy_state",
            "Typisierte native Audit-Retention-Policy ist im Service-State vorhanden",
            typed_audit_retention_policy_present && typed_audit_retention_policy_valid,
            "warning",
            "Native Audit-Retention darf nach Restore nicht nur von einzelnen Export-Commands oder Defaults abhaengen.",
        ),
        business_os_restore_drill_check(
            "source_snapshot_files",
            "Source-Snapshot-Dateien sind in der Backup-Oberflaeche sichtbar",
            source_snapshot_count > 0,
            "warning",
            "Source-Snapshots sind fuer manuelle App-Quellcode-Wiederherstellung relevant.",
        ),
        business_os_restore_drill_check(
            "audit_export_files",
            "Audit-Export-Artefakte sind in der Backup-Oberflaeche sichtbar",
            audit_export_count > 0,
            "warning",
            "Retention-Pruning schreibt support-safe Audit-Exports, die mitgesichert werden sollen.",
        ),
    ];
    let blocking_failures = checks
        .iter()
        .filter(|check| {
            check.get("severity").and_then(Value::as_str) == Some("blocking")
                && check.get("passed").and_then(Value::as_bool) != Some(true)
        })
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| {
            check.get("severity").and_then(Value::as_str) == Some("warning")
                && check.get("passed").and_then(Value::as_bool) != Some(true)
        })
        .count();
    let artifact = serde_json::json!({
        "ok": blocking_failures == 0,
        "kind": "business_os_backup_restore_drill",
        "schema_version": 1,
        "artifact_schema": "ctox.business_os.backup_restore_drill.v1",
        "generated_at_ms": now,
        "product": "ctox-business-os",
        "mode": "restore-readiness-dry-run",
        "redaction": {
            "profile": "support-safe-v1",
            "raw_payloads_included": false,
            "prompt_bodies_included": false,
            "message_bodies_included": false,
            "record_payloads_included": false,
            "secrets_included": false,
            "excluded_fields": [
                "prompt",
                "selected_text",
                "message_body",
                "body",
                "record_payload",
                "payload_json",
                "raw",
                "token",
                "secret"
            ]
        },
        "actor": {
            "id": session_user_id(session).unwrap_or("rxdb-command"),
            "display_name": session
                .user
                .as_ref()
                .map(|user| user.display_name.as_str())
                .unwrap_or("rxdb-command"),
            "role": session_role(session)
        },
        "scope": {
            "module_id": module_scope,
        },
        "checks": {
            "blocking_failures": blocking_failures,
            "warnings": warning_count,
            "items": checks
        },
        "surfaces": {
            "native_store": {
                "path": business_os_restore_drill_relative_path(root, &root.join("runtime").join(STORE_FILE)),
                "tables": native_tables,
                "releases": releases
            },
            "rxdb_store": rxdb,
            "service_state_store": service_state,
            "installed_modules": installed_modules,
            "source_snapshots": source_snapshots,
            "audit_exports": audit_exports,
            "retention": {
                "business_events_default_days": DEFAULT_BUSINESS_AUDIT_RETENTION_DAYS,
                "business_events_effective_days": native_audit_retention_days,
                "native_policy_storage": BUSINESS_OS_AUDIT_RETENTION_POLICY_PAYLOAD_KEY,
                "typed_native_policy_present": typed_audit_retention_policy_present,
                "typed_native_policy_valid": typed_audit_retention_policy_valid,
                "mcp_policy_storage": BUSINESS_OS_MCP_POLICY_PAYLOAD_KEY,
                "typed_mcp_policy_present": typed_mcp_policy_present
            }
        },
        "restore_validation": {
            "validates_app_visibility_inputs": true,
            "validates_data_grant_inputs": true,
            "validates_release_state_inputs": true,
            "validates_rollback_target_inputs": true,
            "destructive_restore_performed": false,
            "active_root_restore_runbook": business_os_active_root_restore_runbook(
                root,
                None,
                None,
                None,
                module_scope,
                None
            )
        }
    });
    let forbidden = support_artifact_forbidden_paths(&artifact);
    anyhow::ensure!(
        forbidden.is_empty(),
        "backup/restore drill artifact contains forbidden fields: {}",
        forbidden.join(", ")
    );
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    let artifact_sha256 = hex_sha256(&artifact_bytes);
    let file_name = format!(
        "business-os-restore-drill-{}-{}.json",
        now,
        &artifact_sha256[..12]
    );
    let relative_path = format!("{BUSINESS_RESTORE_DRILL_EXPORT_DIR}/{file_name}");
    let artifact_path = root.join(&relative_path);
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&artifact_path, artifact_bytes)?;
    Ok(serde_json::json!({
        "ok": blocking_failures == 0,
        "artifact_schema": "ctox.business_os.backup_restore_drill.v1",
        "mode": "restore-readiness-dry-run",
        "module_id": module_scope,
        "blocking_failures": blocking_failures,
        "warnings": warning_count,
        "artifact_path": relative_path,
        "artifact_sha256": artifact_sha256,
        "artifact": artifact
    }))
}

pub(super) fn business_os_backup_restore_drill_safe_command(
    command: &BusinessCommand,
    request: &BusinessBackupRestoreDrillRequest,
    session: &BusinessOsSession,
) -> BusinessCommand {
    BusinessCommand {
        origin: CommandOrigin::TrustedLocal,
        id: command.id.clone(),
        module: command.module.clone(),
        command_type: command.command_type.clone(),
        record_id: command.record_id.clone(),
        payload: serde_json::json!({
            "module_id": source_sanitize_slug(&request.module_id),
        }),
        client_context: serde_json::json!({
            "actor": {
                "id": session_user_id(session).unwrap_or("rxdb-command"),
                "display_name": session
                    .user
                    .as_ref()
                    .map(|user| user.display_name.as_str())
                    .unwrap_or("rxdb-command"),
                "role": session_role(session),
            }
        }),
    }
}

struct BusinessOsBackupManifestSigningKey {
    bytes: Vec<u8>,
    key_id: String,
    source: &'static str,
}

struct BusinessOsBackupPortableEncryptionKey {
    bytes: Vec<u8>,
    key_id: String,
    source: &'static str,
}

struct BusinessOsPortableZipSummary {
    size_bytes: u64,
    sha256: String,
    entry_count: usize,
    file_count: usize,
}

struct BusinessOsPortableEncryptedSummary {
    size_bytes: u64,
    sha256: String,
    chunk_count: u64,
    nonce_base_b64: String,
}

pub(super) fn business_os_backup_key_store_root(root: &Path) -> anyhow::Result<PathBuf> {
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("failed to resolve Business OS root {}", root.display()))?;
    let parent = canonical_root.parent().with_context(|| {
        format!(
            "Business OS root {} has no parent for an external backup key store",
            canonical_root.display()
        )
    })?;
    let root_fingerprint = hex_sha256(canonical_root.to_string_lossy().as_bytes());
    let key_store_root = parent
        .join(BUSINESS_OS_BACKUP_KEY_STORE_DIR)
        .join(&root_fingerprint[..24]);
    anyhow::ensure!(
        !key_store_root.starts_with(&canonical_root),
        "backup key store must be outside the backed-up Business OS root"
    );
    Ok(key_store_root)
}

fn business_os_read_or_migrate_backup_secret(
    root: &Path,
    name: &str,
    description: &str,
    metadata: Value,
) -> anyhow::Result<Option<(String, &'static str)>> {
    let key_store_root = business_os_backup_key_store_root(root)?;
    if crate::secrets::secret_exists(&key_store_root, BUSINESS_OS_SECRET_SCOPE, name)? {
        let raw =
            crate::secrets::read_secret_value(&key_store_root, BUSINESS_OS_SECRET_SCOPE, name)?;
        if crate::secrets::secret_exists(root, BUSINESS_OS_SECRET_SCOPE, name)? {
            let legacy = crate::secrets::read_secret_value(root, BUSINESS_OS_SECRET_SCOPE, name)?;
            anyhow::ensure!(
                legacy == raw,
                "backup key exists with different values in legacy and dedicated stores for {name}"
            );
            crate::secrets::delete_secret_record(root, BUSINESS_OS_SECRET_SCOPE, name)?;
        }
        return Ok(Some((raw, "dedicated_backup_key_store")));
    }
    if !crate::secrets::secret_exists(root, BUSINESS_OS_SECRET_SCOPE, name)? {
        return Ok(None);
    }

    let legacy = crate::secrets::read_secret_value(root, BUSINESS_OS_SECRET_SCOPE, name)?;
    crate::secrets::write_secret_record(
        &key_store_root,
        BUSINESS_OS_SECRET_SCOPE,
        name,
        &legacy,
        Some(description.to_owned()),
        metadata,
    )?;
    let migrated =
        crate::secrets::read_secret_value(&key_store_root, BUSINESS_OS_SECRET_SCOPE, name)?;
    anyhow::ensure!(
        migrated == legacy,
        "backup key migration verification failed for {name}"
    );
    crate::secrets::delete_secret_record(root, BUSINESS_OS_SECRET_SCOPE, name)?;
    Ok(Some((migrated, "migrated_dedicated_backup_key_store")))
}

fn business_os_write_backup_secret(
    root: &Path,
    name: &str,
    value: &str,
    description: &str,
    metadata: Value,
) -> anyhow::Result<()> {
    let key_store_root = business_os_backup_key_store_root(root)?;
    crate::secrets::write_secret_record(
        &key_store_root,
        BUSINESS_OS_SECRET_SCOPE,
        name,
        value,
        Some(description.to_owned()),
        metadata,
    )?;
    Ok(())
}

fn business_os_backup_manifest_signing_key(
    root: &Path,
) -> anyhow::Result<BusinessOsBackupManifestSigningKey> {
    if let Some(key) = business_os_read_backup_manifest_signing_key(root)? {
        return Ok(key);
    }

    let mut bytes = vec![0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate backup manifest signing key"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    business_os_write_backup_secret(
        root,
        BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
        &encoded,
        "Business OS backup manifest signing key",
        serde_json::json!({
            "source": "business_os_backup_restore_drill",
            "algorithm": "HMAC-SHA256",
            "secret_value_in_manifest": false,
            "storage": "dedicated_backup_key_store_outside_backup_root"
        }),
    )?;
    Ok(BusinessOsBackupManifestSigningKey {
        bytes,
        key_id: short_hash(&encoded),
        source: "generated_dedicated_backup_key_store",
    })
}

fn business_os_read_backup_manifest_signing_key(
    root: &Path,
) -> anyhow::Result<Option<BusinessOsBackupManifestSigningKey>> {
    let Some((raw, source)) = business_os_read_or_migrate_backup_secret(
        root,
        BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
        "Business OS backup manifest signing key",
        serde_json::json!({
            "source": "business_os_backup_key_migration",
            "algorithm": "HMAC-SHA256",
            "secret_value_in_manifest": false,
            "storage": "dedicated_backup_key_store_outside_backup_root"
        }),
    )?
    else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .context("failed to decode Business OS backup manifest signing key")?;
    anyhow::ensure!(
        bytes.len() == 32,
        "Business OS backup manifest signing key must be 32 bytes"
    );
    Ok(Some(BusinessOsBackupManifestSigningKey {
        bytes,
        key_id: short_hash(trimmed),
        source,
    }))
}

fn business_os_backup_portable_encryption_key(
    root: &Path,
) -> anyhow::Result<BusinessOsBackupPortableEncryptionKey> {
    if let Some(key) = business_os_read_backup_portable_encryption_key(root)? {
        return Ok(key);
    }

    let mut bytes = vec![0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate portable backup encryption key"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    business_os_write_backup_secret(
        root,
        BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME,
        &encoded,
        "Business OS portable backup encryption key",
        serde_json::json!({
            "source": "business_os_backup_restore_drill",
            "algorithm": "AES-256-GCM",
            "secret_value_in_manifest": false,
            "escrow_required_for_disaster_restore": true,
            "storage": "dedicated_backup_key_store_outside_backup_root"
        }),
    )?;
    Ok(BusinessOsBackupPortableEncryptionKey {
        bytes,
        key_id: short_hash(&encoded),
        source: "generated_dedicated_backup_key_store",
    })
}

fn business_os_read_backup_portable_encryption_key(
    root: &Path,
) -> anyhow::Result<Option<BusinessOsBackupPortableEncryptionKey>> {
    let Some((raw, source)) = business_os_read_or_migrate_backup_secret(
        root,
        BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME,
        "Business OS portable backup encryption key",
        serde_json::json!({
            "source": "business_os_backup_key_migration",
            "algorithm": "AES-256-GCM",
            "secret_value_in_manifest": false,
            "escrow_required_for_disaster_restore": true,
            "storage": "dedicated_backup_key_store_outside_backup_root"
        }),
    )?
    else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .context("failed to decode Business OS portable backup encryption key")?;
    anyhow::ensure!(
        bytes.len() == 32,
        "Business OS portable backup encryption key must be 32 bytes"
    );
    Ok(Some(BusinessOsBackupPortableEncryptionKey {
        bytes,
        key_id: short_hash(trimmed),
        source,
    }))
}

fn business_os_backup_manifest_integrity(
    signing_key: &BusinessOsBackupManifestSigningKey,
    payload_bytes: &[u8],
) -> Value {
    let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &signing_key.bytes);
    let signature = hmac::sign(&hmac_key, payload_bytes);
    let verified = hmac::verify(&hmac_key, payload_bytes, signature.as_ref()).is_ok();
    serde_json::json!({
        "status": "signed",
        "algorithm": "HMAC-SHA256",
        "signed_payload_sha256": hex_sha256(payload_bytes),
        "signature_b64": base64::engine::general_purpose::STANDARD.encode(signature.as_ref()),
        "signature_verified_before_write": verified,
        "signing_key": {
            "scope": BUSINESS_OS_SECRET_SCOPE,
            "name": BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
            "key_id": signing_key.key_id,
            "source": signing_key.source,
            "storage": "dedicated_backup_key_store_outside_backup_root",
            "secret_value_in_manifest": false
        }
    })
}

fn business_os_backup_restore_compatibility() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "ctox_version": env!("CARGO_PKG_VERSION"),
        "manifest_schema": "ctox.business_os.backup_restore_snapshot_manifest.v1",
        "supported_manifest_schema_versions": {
            "min": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION,
            "max": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION
        },
        "same_version_restore_supported": true,
        "automatic_cross_version_restore_supported": false,
        "downgrade_restore_supported": false,
        "downgrade_restore_policy": "blocked_without_explicit_release_level_evidence",
        "cross_version_restore_policy": "run_restore_drill_with_target_version_before_active_root_restore"
    })
}

fn business_os_backup_key_needles(
    signing_key: &BusinessOsBackupManifestSigningKey,
    portable_key: &BusinessOsBackupPortableEncryptionKey,
) -> Vec<Vec<u8>> {
    vec![
        signing_key.bytes.clone(),
        portable_key.bytes.clone(),
        base64::engine::general_purpose::STANDARD
            .encode(&signing_key.bytes)
            .into_bytes(),
        base64::engine::general_purpose::STANDARD
            .encode(&portable_key.bytes)
            .into_bytes(),
    ]
}

fn reader_contains_any_sequence<R: Read>(
    mut reader: R,
    needles: &[Vec<u8>],
) -> anyhow::Result<bool> {
    let overlap = needles
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .saturating_sub(1);
    let mut carry = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(false);
        }
        let mut window = Vec::with_capacity(carry.len() + read);
        window.extend_from_slice(&carry);
        window.extend_from_slice(&buffer[..read]);
        if needles.iter().any(|needle| {
            !needle.is_empty()
                && window
                    .windows(needle.len())
                    .any(|candidate| candidate == needle.as_slice())
        }) {
            return Ok(true);
        }
        let keep = overlap.min(window.len());
        carry.clear();
        carry.extend_from_slice(&window[window.len() - keep..]);
    }
}

pub(super) fn sqlite_backup_key_records(path: &Path) -> anyhow::Result<Vec<String>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to inspect backup sqlite content {}", path.display()))?;
    let has_secret_table = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'ctox_secret_records')",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if !has_secret_table {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT secret_name FROM ctox_secret_records WHERE scope = ?1 AND secret_name IN (?2, ?3) ORDER BY secret_name",
    )?;
    let records = statement
        .query_map(
            params![
                BUSINESS_OS_SECRET_SCOPE,
                BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
                BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(records)
}

fn file_has_sqlite_header(path: &Path) -> anyhow::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 16];
    Ok(file.read(&mut header)? == header.len() && &header == b"SQLite format 3\0")
}

fn business_os_snapshot_backup_key_separation(
    snapshot_root: &Path,
    files: &[Value],
    signing_key: &BusinessOsBackupManifestSigningKey,
    portable_key: &BusinessOsBackupPortableEncryptionKey,
) -> anyhow::Result<Value> {
    let needles = business_os_backup_key_needles(signing_key, portable_key);
    let mut plaintext_key_material_paths = Vec::new();
    let mut forbidden_secret_records = Vec::new();
    let mut sqlite_files_inspected = 0usize;
    for relative in backup_file_manifest_paths(files) {
        let path = snapshot_root.join(&relative);
        if reader_contains_any_sequence(fs::File::open(&path)?, &needles)? {
            plaintext_key_material_paths.push(relative.clone());
        }
        if file_has_sqlite_header(&path)? {
            sqlite_files_inspected += 1;
            for secret_name in sqlite_backup_key_records(&path)? {
                forbidden_secret_records.push(serde_json::json!({
                    "path": relative,
                    "scope": BUSINESS_OS_SECRET_SCOPE,
                    "name": secret_name
                }));
            }
        }
    }
    let passed = plaintext_key_material_paths.is_empty() && forbidden_secret_records.is_empty();
    Ok(serde_json::json!({
        "passed": passed,
        "verification": "snapshot_file_content_and_sqlite_secret_records",
        "files_inspected": backup_file_manifest_paths(files).len(),
        "sqlite_files_inspected": sqlite_files_inspected,
        "plaintext_key_material_paths": plaintext_key_material_paths,
        "forbidden_secret_records": forbidden_secret_records,
        "signing_key_absent": passed,
        "portable_encryption_key_absent": passed
    }))
}

fn business_os_portable_zip_backup_key_separation(
    verify_zip_path: &Path,
    signing_key: &BusinessOsBackupManifestSigningKey,
    portable_key: &BusinessOsBackupPortableEncryptionKey,
) -> anyhow::Result<Value> {
    let needles = business_os_backup_key_needles(signing_key, portable_key);
    let mut archive = zip::ZipArchive::new(fs::File::open(verify_zip_path)?)
        .context("failed to inspect decrypted portable backup zip for key material")?;
    let mut plaintext_key_material_entries = Vec::new();
    let mut forbidden_secret_records = Vec::new();
    let mut sqlite_entries_inspected = 0usize;
    let mut files_inspected = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if !entry.is_file() {
            continue;
        }
        files_inspected += 1;
        let entry_name = entry.name().replace('\\', "/");
        let mut header = [0u8; 16];
        let header_len = entry.read(&mut header)?;
        let is_sqlite = header_len == header.len() && &header == b"SQLite format 3\0";
        if is_sqlite {
            sqlite_entries_inspected += 1;
            let temp_path = verify_zip_path.with_extension(format!("entry-{index}.sqlite3.tmp"));
            let inspection = (|| -> anyhow::Result<(bool, Vec<String>)> {
                let mut temp = fs::File::create(&temp_path)?;
                temp.write_all(&header)?;
                io::copy(&mut entry, &mut temp)?;
                temp.flush()?;
                let contains_key =
                    reader_contains_any_sequence(fs::File::open(&temp_path)?, &needles)?;
                let records = sqlite_backup_key_records(&temp_path)?;
                Ok((contains_key, records))
            })();
            let _ = remove_file_if_present(&temp_path);
            let (contains_key, records) = inspection?;
            if contains_key {
                plaintext_key_material_entries.push(entry_name.clone());
            }
            for secret_name in records {
                forbidden_secret_records.push(serde_json::json!({
                    "entry": entry_name,
                    "scope": BUSINESS_OS_SECRET_SCOPE,
                    "name": secret_name
                }));
            }
        } else {
            let reader = io::Cursor::new(header[..header_len].to_vec()).chain(entry);
            if reader_contains_any_sequence(reader, &needles)? {
                plaintext_key_material_entries.push(entry_name);
            }
        }
    }
    let passed = plaintext_key_material_entries.is_empty() && forbidden_secret_records.is_empty();
    Ok(serde_json::json!({
        "passed": passed,
        "verification": "decrypted_portable_zip_entry_content_and_sqlite_secret_records",
        "files_inspected": files_inspected,
        "sqlite_entries_inspected": sqlite_entries_inspected,
        "plaintext_key_material_entries": plaintext_key_material_entries,
        "forbidden_secret_records": forbidden_secret_records,
        "signing_key_absent": passed,
        "portable_encryption_key_absent": passed
    }))
}

fn business_os_raw_backup_security(
    now_ms: i64,
    portable_export: Option<&Value>,
    key_separation: Option<&Value>,
) -> Value {
    let retention_ms = BUSINESS_OS_BACKUP_RAW_RETENTION_DAYS * 24 * 60 * 60 * 1_000;
    let portable_export_created = portable_export
        .and_then(|export| export.get("encrypted"))
        .and_then(Value::as_bool)
        == Some(true);
    let portable_export_path = portable_export
        .and_then(|export| export.pointer("/ciphertext/path"))
        .and_then(Value::as_str);
    let portable_export_sha256 = portable_export
        .and_then(|export| export.pointer("/ciphertext/sha256"))
        .and_then(Value::as_str);
    let key_separation_passed = key_separation
        .and_then(|evidence| evidence.get("passed"))
        .and_then(Value::as_bool)
        == Some(true);
    let encryption = if portable_export_created {
        serde_json::json!({
            "status": "portable_encrypted_export_created",
            "required_before_off_machine_transfer": true,
            "portable_export_encrypted": true,
            "portable_export_required_for_off_machine_transfer": true,
            "portable_export_path": portable_export_path,
            "portable_export_sha256": portable_export_sha256,
            "algorithm": "AES-256-GCM",
            "key_escrow_required_for_disaster_restore": true
        })
    } else {
        serde_json::json!({
            "status": "not_encrypted_by_restore_drill",
            "required_before_off_machine_transfer": true,
            "portable_export_encrypted": false,
            "reason": "The local drill validates backup completeness and restore readiness; portable/off-machine backup encryption is an infrastructure gate."
        })
    };
    serde_json::json!({
        "schema_version": 1,
        "classification": "sensitive-local-raw-backup",
        "raw_payloads_included": true,
        "contains_sensitive_data": true,
        "support_attachment_allowed": false,
        "plaintext_restore_workspace": true,
        "backup_key_separation": {
            "verified": key_separation.is_some(),
            "passed": key_separation_passed,
            "portable_key_must_not_travel_with_artifact": key_separation_passed,
            "manifest_signing_key_must_not_travel_with_artifact": key_separation_passed,
            "evidence": key_separation.cloned().unwrap_or(Value::Null)
        },
        "encryption": encryption,
        "retention": {
            "policy": "ctox.business_os.local_raw_backup_retention.v1",
            "retention_days": BUSINESS_OS_BACKUP_RAW_RETENTION_DAYS,
            "created_at_ms": now_ms,
            "expires_at_ms": now_ms + retention_ms,
            "operator_delete_after_drill": true,
            "applies_to": [
                "runtime/backup/business-os-drill-*"
            ]
        }
    })
}

fn business_os_create_portable_backup_export(
    root: &Path,
    backup_root: &Path,
    snapshot_root: &Path,
    drill_id: &str,
    files: &[Value],
    signing_key: &BusinessOsBackupManifestSigningKey,
    encryption_key: &BusinessOsBackupPortableEncryptionKey,
    snapshot_key_separation: &Value,
) -> anyhow::Result<Value> {
    let portable_dir = backup_root.join("portable");
    fs::create_dir_all(&portable_dir)?;
    let plaintext_zip_path = portable_dir.join(format!("{drill_id}.snapshot.zip.tmp"));
    let verify_zip_path = portable_dir.join(format!("{drill_id}.verify.zip.tmp"));
    let ciphertext_path = portable_dir.join(format!("{drill_id}.snapshot.zip.aes256gcm"));
    let result = (|| -> anyhow::Result<Value> {
        let zip_summary =
            business_os_write_snapshot_zip(snapshot_root, &plaintext_zip_path, files)?;
        let expected_entries = backup_file_manifest_paths(files);
        let mut nonce_base = [0u8; 12];
        SystemRandom::new()
            .fill(&mut nonce_base)
            .map_err(|_| anyhow::anyhow!("failed to generate portable backup nonce"))?;
        let encrypted_summary = business_os_encrypt_portable_backup_zip(
            &plaintext_zip_path,
            &ciphertext_path,
            drill_id,
            encryption_key,
            nonce_base,
        )?;
        let verification = business_os_verify_portable_backup_export(
            &ciphertext_path,
            &verify_zip_path,
            drill_id,
            encryption_key,
            nonce_base,
            encrypted_summary.chunk_count,
            zip_summary.size_bytes,
            &zip_summary.sha256,
            &expected_entries,
            signing_key,
        )?;
        let snapshot_key_separation_passed = snapshot_key_separation
            .get("passed")
            .and_then(Value::as_bool)
            == Some(true);
        let portable_key_separation_passed = verification
            .pointer("/backup_key_separation/passed")
            .and_then(Value::as_bool)
            == Some(true);
        anyhow::ensure!(
            snapshot_key_separation_passed && portable_key_separation_passed,
            "backup key material was detected in the snapshot or decrypted portable artifact"
        );
        let key_separation = serde_json::json!({
            "passed": true,
            "key_store_outside_backup_root": true,
            "snapshot": snapshot_key_separation,
            "decrypted_portable_artifact": verification
                .get("backup_key_separation")
                .cloned()
                .unwrap_or(Value::Null)
        });
        Ok(serde_json::json!({
            "ok": true,
            "schema_version": 1,
            "artifact_schema": "ctox.business_os.portable_backup_export.v1",
            "status": "encrypted",
            "encrypted": true,
            "created_at_ms": now_ms() as i64,
            "format": "snapshot-zip-chunked-aes-gcm",
            "algorithm": "AES-256-GCM",
            "framing": "ctox.business_os.portable_backup_frame.v1",
            "aad_prefix": format!("ctox.business_os.portable_backup_export.v1:{drill_id}:"),
            "chunk_size_bytes": BUSINESS_OS_BACKUP_PORTABLE_CHUNK_SIZE as i64,
            "key": {
                "scope": BUSINESS_OS_SECRET_SCOPE,
                "name": BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME,
                "key_id": encryption_key.key_id,
                "source": encryption_key.source,
                "storage": "dedicated_backup_key_store_outside_backup_root",
                "secret_value_in_manifest": false,
                "escrow_required_for_disaster_restore": true
            },
            "nonce_base_b64": encrypted_summary.nonce_base_b64,
            "plaintext": {
                "format": "zip",
                "zip64_large_file_enabled": true,
                "source": "snapshot",
                "size_bytes": zip_summary.size_bytes,
                "sha256": zip_summary.sha256,
                "file_count": zip_summary.file_count,
                "zip_entry_count": zip_summary.entry_count,
                "temp_plaintext_path": business_os_restore_drill_relative_path(root, &plaintext_zip_path)
            },
            "ciphertext": {
                "path": business_os_restore_drill_relative_path(root, &ciphertext_path),
                "size_bytes": encrypted_summary.size_bytes,
                "sha256": encrypted_summary.sha256,
                "chunk_count": encrypted_summary.chunk_count
            },
            "verification": verification,
            "backup_key_separation": key_separation,
            "off_machine_transfer": {
                "allowed_when_encrypted": true,
                "raw_snapshot_transfer_allowed": false,
                "key_must_not_travel_with_artifact": portable_key_separation_passed,
                "requires_incident_owner_approval": true,
                "requires_separate_key_escrow": true
            }
        }))
    })();
    let plaintext_deleted = remove_file_if_present(&plaintext_zip_path)?;
    let verify_deleted = remove_file_if_present(&verify_zip_path)?;
    let mut export = result?;
    if let Some(plaintext) = export.get_mut("plaintext").and_then(Value::as_object_mut) {
        plaintext.insert(
            "temp_plaintext_deleted".to_owned(),
            Value::Bool(plaintext_deleted),
        );
    }
    if let Some(verification) = export
        .get_mut("verification")
        .and_then(Value::as_object_mut)
    {
        verification.insert(
            "temp_verify_zip_deleted".to_owned(),
            Value::Bool(verify_deleted),
        );
    }
    Ok(export)
}

fn business_os_write_snapshot_zip(
    snapshot_root: &Path,
    zip_path: &Path,
    files: &[Value],
) -> anyhow::Result<BusinessOsPortableZipSummary> {
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if zip_path.exists() {
        fs::remove_file(zip_path)?;
    }
    let file = fs::File::create(zip_path).with_context(|| {
        format!(
            "failed to create portable backup zip {}",
            zip_path.display()
        )
    })?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut entry_count = 0usize;
    for relative in backup_file_manifest_paths(files) {
        let source = snapshot_root.join(&relative);
        if !source.is_file() {
            continue;
        }
        zip.start_file(&relative, options)
            .with_context(|| format!("failed to start portable backup zip entry {relative}"))?;
        let mut input = fs::File::open(&source).with_context(|| {
            format!("failed to open portable backup source {}", source.display())
        })?;
        io::copy(&mut input, &mut zip)
            .with_context(|| format!("failed to write portable backup zip entry {relative}"))?;
        entry_count += 1;
    }
    zip.finish()?;
    let (size_bytes, sha256) = file_sha256(zip_path)?;
    Ok(BusinessOsPortableZipSummary {
        size_bytes,
        sha256,
        entry_count,
        file_count: backup_file_manifest_paths(files).len(),
    })
}

fn backup_file_manifest_paths(files: &[Value]) -> BTreeSet<String> {
    files
        .iter()
        .filter_map(|file| file.get("path").and_then(Value::as_str))
        .map(|path| path.replace('\\', "/"))
        .filter(|path| !path.is_empty() && !path.starts_with('/') && !path.contains(".."))
        .collect()
}

fn business_os_encrypt_portable_backup_zip(
    plaintext_zip_path: &Path,
    ciphertext_path: &Path,
    drill_id: &str,
    encryption_key: &BusinessOsBackupPortableEncryptionKey,
    nonce_base: [u8; 12],
) -> anyhow::Result<BusinessOsPortableEncryptedSummary> {
    if let Some(parent) = ciphertext_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if ciphertext_path.exists() {
        fs::remove_file(ciphertext_path)?;
    }
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &encryption_key.bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct portable backup encryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut input = fs::File::open(plaintext_zip_path).with_context(|| {
        format!(
            "failed to open portable backup plaintext zip {}",
            plaintext_zip_path.display()
        )
    })?;
    let mut output = fs::File::create(ciphertext_path).with_context(|| {
        format!(
            "failed to create portable backup ciphertext {}",
            ciphertext_path.display()
        )
    })?;
    let mut ciphertext_hasher = Sha256::new();
    output.write_all(BUSINESS_OS_BACKUP_PORTABLE_MAGIC)?;
    ciphertext_hasher.update(BUSINESS_OS_BACKUP_PORTABLE_MAGIC);
    let mut ciphertext_size = BUSINESS_OS_BACKUP_PORTABLE_MAGIC.len() as u64;
    let mut chunk_count = 0u64;
    let mut buffer = vec![0u8; BUSINESS_OS_BACKUP_PORTABLE_CHUNK_SIZE];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let mut chunk = buffer[..read].to_vec();
        let nonce = business_os_portable_backup_nonce(nonce_base, chunk_count)?;
        let aad = business_os_portable_backup_aad(drill_id, chunk_count);
        key.seal_in_place_append_tag(nonce, aead::Aad::from(aad.as_bytes()), &mut chunk)
            .map_err(|_| anyhow::anyhow!("failed to encrypt portable backup chunk"))?;
        let len_bytes = (chunk.len() as u64).to_be_bytes();
        output.write_all(&len_bytes)?;
        output.write_all(&chunk)?;
        ciphertext_hasher.update(len_bytes);
        ciphertext_hasher.update(&chunk);
        ciphertext_size += len_bytes.len() as u64 + chunk.len() as u64;
        chunk_count += 1;
    }
    output.flush()?;
    let digest = ciphertext_hasher.finalize();
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(BusinessOsPortableEncryptedSummary {
        size_bytes: ciphertext_size,
        sha256,
        chunk_count,
        nonce_base_b64: base64::engine::general_purpose::STANDARD.encode(nonce_base),
    })
}

fn business_os_verify_portable_backup_export(
    ciphertext_path: &Path,
    verify_zip_path: &Path,
    drill_id: &str,
    encryption_key: &BusinessOsBackupPortableEncryptionKey,
    nonce_base: [u8; 12],
    chunk_count: u64,
    expected_plaintext_size_bytes: u64,
    expected_plaintext_sha256: &str,
    expected_entries: &BTreeSet<String>,
    signing_key: &BusinessOsBackupManifestSigningKey,
) -> anyhow::Result<Value> {
    if let Some(parent) = verify_zip_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if verify_zip_path.exists() {
        fs::remove_file(verify_zip_path)?;
    }
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &encryption_key.bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct portable backup decryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut input = fs::File::open(ciphertext_path).with_context(|| {
        format!(
            "failed to open portable backup ciphertext {}",
            ciphertext_path.display()
        )
    })?;
    let mut magic = vec![0u8; BUSINESS_OS_BACKUP_PORTABLE_MAGIC.len()];
    input.read_exact(&mut magic)?;
    anyhow::ensure!(
        magic == BUSINESS_OS_BACKUP_PORTABLE_MAGIC,
        "portable backup ciphertext has invalid frame magic"
    );
    let mut output = fs::File::create(verify_zip_path).with_context(|| {
        format!(
            "failed to create portable backup verify zip {}",
            verify_zip_path.display()
        )
    })?;
    let mut plaintext_hasher = Sha256::new();
    let mut plaintext_size = 0u64;
    for chunk_index in 0..chunk_count {
        let mut len_bytes = [0u8; 8];
        input.read_exact(&mut len_bytes)?;
        let ciphertext_len = u64::from_be_bytes(len_bytes);
        anyhow::ensure!(
            ciphertext_len
                <= (BUSINESS_OS_BACKUP_PORTABLE_CHUNK_SIZE + aead::AES_256_GCM.tag_len()) as u64,
            "portable backup encrypted chunk is larger than the declared chunk frame"
        );
        let mut chunk = vec![0u8; ciphertext_len as usize];
        input.read_exact(&mut chunk)?;
        let nonce = business_os_portable_backup_nonce(nonce_base, chunk_index)?;
        let aad = business_os_portable_backup_aad(drill_id, chunk_index);
        let plaintext = key
            .open_in_place(nonce, aead::Aad::from(aad.as_bytes()), &mut chunk)
            .map_err(|_| anyhow::anyhow!("failed to decrypt portable backup chunk"))?;
        output.write_all(plaintext)?;
        plaintext_hasher.update(&*plaintext);
        plaintext_size += plaintext.len() as u64;
    }
    let mut trailing = [0u8; 1];
    let trailing_read = input.read(&mut trailing)?;
    anyhow::ensure!(
        trailing_read == 0,
        "portable backup ciphertext has trailing bytes after declared chunks"
    );
    output.flush()?;
    let digest = plaintext_hasher.finalize();
    let plaintext_sha256 = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut archive = zip::ZipArchive::new(fs::File::open(verify_zip_path)?)
        .context("failed to open verified portable backup zip")?;
    let mut zip_entries = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_file() {
            zip_entries.insert(entry.name().replace('\\', "/"));
        }
    }
    drop(archive);
    let backup_key_separation = business_os_portable_zip_backup_key_separation(
        verify_zip_path,
        signing_key,
        encryption_key,
    )?;
    let missing_entries = expected_entries
        .difference(&zip_entries)
        .cloned()
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "status": "decrypted-and-validated",
        "plaintext_sha256": plaintext_sha256,
        "plaintext_sha256_matches": plaintext_sha256 == expected_plaintext_sha256,
        "plaintext_size_bytes": plaintext_size,
        "plaintext_size_matches": plaintext_size == expected_plaintext_size_bytes,
        "zip_opened": true,
        "zip_entry_count": zip_entries.len(),
        "expected_zip_entry_count": expected_entries.len(),
        "zip_entries_match_manifest": missing_entries.is_empty() && zip_entries.len() == expected_entries.len(),
        "missing_entries": missing_entries,
        "backup_key_separation": backup_key_separation
    }))
}

fn business_os_portable_backup_nonce(
    nonce_base: [u8; 12],
    chunk_index: u64,
) -> anyhow::Result<aead::Nonce> {
    let mut nonce = nonce_base;
    let mut suffix = [0u8; 8];
    suffix.copy_from_slice(&nonce[4..12]);
    let next = u64::from_be_bytes(suffix)
        .checked_add(chunk_index)
        .context("portable backup nonce counter overflow")?;
    nonce[4..12].copy_from_slice(&next.to_be_bytes());
    Ok(aead::Nonce::assume_unique_for_key(nonce))
}

fn business_os_portable_backup_aad(drill_id: &str, chunk_index: u64) -> String {
    format!("ctox.business_os.portable_backup_export.v1:{drill_id}:{chunk_index}")
}

fn remove_file_if_present(path: &Path) -> anyhow::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err)
            .with_context(|| format!("failed to remove temporary backup file {}", path.display())),
    }
}

pub fn run_business_os_backup_restore_drill(
    root: &Path,
    module_id: Option<&str>,
) -> anyhow::Result<Value> {
    let now = now_ms() as i64;
    let signing_key = business_os_backup_manifest_signing_key(root)?;
    let portable_encryption_key = business_os_backup_portable_encryption_key(root)?;
    let drill_id = format!("business-os-drill-{}-{}", now, Uuid::new_v4());
    let backup_root = crate::paths::backup_dir(root).join(&drill_id);
    let snapshot_root = backup_root.join("snapshot");
    let restore_root = backup_root.join("restore-root");
    fs::create_dir_all(&snapshot_root)?;
    fs::create_dir_all(&restore_root)?;

    let sqlite_sources = [
        (
            "core_service_state",
            crate::persistence::sqlite_path(root),
            PathBuf::from("runtime/ctox.sqlite3"),
        ),
        (
            "ctox_secret_store",
            crate::secrets::secret_store_path(root),
            PathBuf::from("runtime/ctox-secrets.sqlite3"),
        ),
        (
            "business_os_native_store",
            root.join("runtime").join(STORE_FILE),
            PathBuf::from(format!("runtime/{STORE_FILE}")),
        ),
        (
            "business_os_rxdb_store",
            rxdb_store_path(root),
            PathBuf::from(format!("runtime/{RXDB_STORE_FILE}")),
        ),
    ];
    let mut sqlite_snapshots = Vec::new();
    let mut sqlite_integrity = Vec::new();
    for (kind, source, relative) in sqlite_sources {
        let backup_target = snapshot_root.join(&relative);
        let copied = sqlite_vacuum_into(&source, &backup_target)?;
        if copied {
            let restore_target = restore_root.join(&relative);
            if let Some(parent) = restore_target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&backup_target, &restore_target)?;
            sqlite_integrity.push(sqlite_integrity_summary(&restore_target, kind, &relative)?);
        }
        sqlite_snapshots.push(serde_json::json!({
            "kind": kind,
            "source_path": business_os_restore_drill_relative_path(root, &source),
            "backup_path": business_os_restore_drill_relative_path(root, &backup_target),
            "restore_path": business_os_restore_drill_relative_path(root, &restore_root.join(&relative)),
            "copied": copied
        }));
    }

    let directory_sources = [
        (
            "installed_modules",
            resolve_business_os_installed_app_root(root).join("installed-modules"),
            PathBuf::from("runtime/business-os/installed-modules"),
        ),
        (
            "source_snapshots",
            root.join("runtime").join("business-os-source-snapshots"),
            PathBuf::from("runtime/business-os-source-snapshots"),
        ),
        (
            "audit_exports",
            root.join(BUSINESS_AUDIT_EXPORT_DIR),
            PathBuf::from(BUSINESS_AUDIT_EXPORT_DIR),
        ),
    ];
    let mut copied_directories = Vec::new();
    for (kind, source, relative) in directory_sources {
        let backup_target = snapshot_root.join(&relative);
        let copied = copy_directory_if_exists(&source, &backup_target)?;
        if copied {
            copy_directory_if_exists(&backup_target, &restore_root.join(&relative))?;
        }
        copied_directories.push(serde_json::json!({
            "kind": kind,
            "source_path": business_os_restore_drill_relative_path(root, &source),
            "backup_path": business_os_restore_drill_relative_path(root, &backup_target),
            "restore_path": business_os_restore_drill_relative_path(root, &restore_root.join(&relative)),
            "copied": copied,
            "file_count": count_files_recursive(&backup_target)? as i64
        }));
    }

    let files = backup_file_manifest(&snapshot_root)?;
    let snapshot_key_separation = business_os_snapshot_backup_key_separation(
        &snapshot_root,
        &files,
        &signing_key,
        &portable_encryption_key,
    )?;
    if snapshot_key_separation
        .get("passed")
        .and_then(Value::as_bool)
        != Some(true)
    {
        let _ = fs::remove_dir_all(&backup_root);
        anyhow::bail!(
            "backup snapshot contains backup signing or portable encryption key material"
        );
    }
    let portable_encrypted_export = match business_os_create_portable_backup_export(
        root,
        &backup_root,
        &snapshot_root,
        &drill_id,
        &files,
        &signing_key,
        &portable_encryption_key,
        &snapshot_key_separation,
    ) {
        Ok(export) => export,
        Err(err) => {
            let _ = fs::remove_dir_all(&backup_root);
            return Err(err);
        }
    };
    let key_separation = portable_encrypted_export
        .get("backup_key_separation")
        .cloned()
        .unwrap_or(Value::Null);
    let raw_backup_security = business_os_raw_backup_security(
        now,
        Some(&portable_encrypted_export),
        Some(&key_separation),
    );
    let restore_compatibility = business_os_backup_restore_compatibility();
    let manifest_payload = serde_json::json!({
        "schema_version": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION,
        "artifact_schema": "ctox.business_os.backup_restore_snapshot_manifest.v1",
        "generated_at_ms": now,
        "drill_id": drill_id,
        "product": "ctox-business-os",
        "raw_backup_contains_sensitive_data": true,
        "raw_backup_security": raw_backup_security,
        "portable_encrypted_export": portable_encrypted_export,
        "restore_compatibility": restore_compatibility,
        "sqlite_snapshots": sqlite_snapshots,
        "copied_directories": copied_directories,
        "files": files
    });
    let manifest_payload_bytes = serde_json::to_vec_pretty(&manifest_payload)?;
    let manifest_integrity =
        business_os_backup_manifest_integrity(&signing_key, &manifest_payload_bytes);
    let mut manifest = manifest_payload;
    if let Some(object) = manifest.as_object_mut() {
        object.insert("manifest_integrity".to_owned(), manifest_integrity);
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_sha256 = hex_sha256(&manifest_bytes);
    let manifest_path = backup_root.join("manifest.json");
    fs::write(&manifest_path, manifest_bytes)?;

    let request = BusinessBackupRestoreDrillRequest {
        module_id: module_id.unwrap_or_default().to_owned(),
    };
    let session = BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(BusinessOsSessionUser {
            id: "backup-restore-drill".to_owned(),
            display_name: "Backup Restore Drill".to_owned(),
            role: "admin".to_owned(),
            is_admin: true,
        }),
        login_url: None,
        reason: None,
    };
    let restore_readiness =
        business_os_backup_restore_drill_export(&restore_root, &session, &request)?;
    let integrity_ok = sqlite_integrity
        .iter()
        .all(|item| item.get("ok").and_then(Value::as_bool) == Some(true));
    let restore_ok = restore_readiness
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let key_separation_ok = key_separation.get("passed").and_then(Value::as_bool) == Some(true);
    let ok = integrity_ok && restore_ok && key_separation_ok;
    Ok(serde_json::json!({
        "ok": ok,
        "schema_version": 1,
        "artifact_schema": "ctox.business_os.backup_restore_drill_run.v1",
        "mode": "backup-copy-isolated-restore-validate",
        "drill_id": drill_id,
        "backup_path": business_os_restore_drill_relative_path(root, &backup_root),
        "restore_root": business_os_restore_drill_relative_path(root, &restore_root),
        "manifest_path": business_os_restore_drill_relative_path(root, &manifest_path),
        "manifest_sha256": manifest_sha256,
        "raw_backup_contains_sensitive_data": true,
        "raw_backup_security": manifest
            .get("raw_backup_security")
            .cloned()
            .unwrap_or(Value::Null),
        "portable_encrypted_export": manifest
            .get("portable_encrypted_export")
            .cloned()
            .unwrap_or(Value::Null),
        "restore_compatibility": manifest
            .get("restore_compatibility")
            .cloned()
            .unwrap_or(Value::Null),
        "manifest_integrity": manifest
            .get("manifest_integrity")
            .cloned()
            .unwrap_or(Value::Null),
        "backup_key_separation": key_separation,
        "sqlite_snapshots": sqlite_snapshots,
        "copied_directories": copied_directories,
        "restore_validation": {
            "isolated_root": true,
            "sqlite_integrity": sqlite_integrity,
            "readiness": restore_readiness,
            "active_root_restore_runbook": business_os_active_root_restore_runbook(
                root,
                Some(&restore_root),
                Some(&manifest_path),
                Some(&manifest_sha256),
                module_id,
                Some(key_separation_ok)
            )
        },
        "remaining_boundaries": [
            "browser IndexedDB unsynced local state is not backed up by this native drill",
            "hosted WebRTC resync after restore still needs browser-level proof",
            "cross-version downgrade/upgrade restore compatibility still needs release-level evidence",
            "portable backup restore requires the encryption key to be escrowed outside the encrypted artifact"
        ]
    }))
}

pub fn prune_business_os_backup_restore_drills(
    root: &Path,
    dry_run: bool,
) -> anyhow::Result<Value> {
    let now = now_ms() as i64;
    let backup_dir = crate::paths::backup_dir(root);
    let mut items = Vec::new();
    let mut scanned = 0usize;
    let mut expired = 0usize;
    let mut deleted = 0usize;
    if backup_dir.is_dir() {
        for entry in fs::read_dir(&backup_dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("business-os-drill-") {
                continue;
            }
            scanned += 1;
            let path = entry.path();
            let manifest_path = path.join("manifest.json");
            let manifest = fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
            let expires_at_ms = manifest.as_ref().and_then(|value| {
                value
                    .pointer("/raw_backup_security/retention/expires_at_ms")
                    .and_then(Value::as_i64)
            });
            let retention_policy_present = expires_at_ms.is_some();
            let is_expired = expires_at_ms.is_some_and(|expires_at| expires_at <= now);
            let mut item_deleted = false;
            if is_expired {
                expired += 1;
                if !dry_run {
                    fs::remove_dir_all(&path)?;
                    deleted += 1;
                    item_deleted = true;
                }
            }
            items.push(serde_json::json!({
                "drill_id": name,
                "path": business_os_restore_drill_relative_path(root, &path),
                "manifest_path": business_os_restore_drill_relative_path(root, &manifest_path),
                "retention_policy_present": retention_policy_present,
                "expires_at_ms": expires_at_ms,
                "expired": is_expired,
                "deleted": item_deleted
            }));
        }
    }
    items.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(b.get("path").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(serde_json::json!({
        "ok": true,
        "schema_version": 1,
        "artifact_schema": "ctox.business_os.backup_restore_drill_prune.v1",
        "mode": if dry_run { "dry-run" } else { "prune-expired" },
        "backup_path": business_os_restore_drill_relative_path(root, &backup_dir),
        "now_ms": now,
        "dry_run": dry_run,
        "scanned_drill_count": scanned,
        "expired_drill_count": expired,
        "deleted_drill_count": deleted,
        "items": items
    }))
}

pub fn inspect_business_os_backup_manifest(
    root: &Path,
    manifest_path: &Path,
) -> anyhow::Result<Value> {
    let raw = fs::read(manifest_path)
        .with_context(|| format!("failed to read backup manifest {}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "failed to parse backup manifest {}",
            manifest_path.display()
        )
    })?;
    let artifact_schema_ok = manifest.get("artifact_schema").and_then(Value::as_str)
        == Some("ctox.business_os.backup_restore_snapshot_manifest.v1");
    let integrity = business_os_inspect_backup_manifest_integrity(root, &manifest)?;
    let compatibility = business_os_backup_restore_manifest_compatibility(&manifest);
    let portable_export =
        business_os_inspect_portable_backup_export(root, manifest_path, &manifest)?;
    let ok = artifact_schema_ok
        && integrity
            .get("signature_valid")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && compatibility
            .get("allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && portable_export
            .get("ciphertext_sha256_matches")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(serde_json::json!({
        "ok": ok,
        "schema_version": 1,
        "artifact_schema": "ctox.business_os.backup_manifest_preflight.v1",
        "mode": "backup-manifest-restore-preflight",
        "manifest_path": business_os_restore_drill_relative_path(root, manifest_path),
        "artifact_schema_ok": artifact_schema_ok,
        "integrity": integrity,
        "compatibility": compatibility,
        "portable_encrypted_export": portable_export,
        "requires_operator_confirmation": true,
        "destructive_restore_performed": false
    }))
}

pub fn inspect_business_os_backup_key_escrow(root: &Path) -> anyhow::Result<Value> {
    let key = business_os_read_backup_portable_encryption_key(root)?;
    let key_exists = key.is_some();
    let key_id = key.as_ref().map(|key| key.key_id.as_str());
    Ok(serde_json::json!({
        "schema_version": 1,
        "artifact_schema": "ctox.business_os.backup_key_escrow_status.v1",
        "ok": key_exists,
        "status": if key_exists {
            "ready_for_external_escrow_confirmation"
        } else {
            "portable_backup_key_missing"
        },
        "generated_at_ms": now_ms() as i64,
        "key": {
            "scope": BUSINESS_OS_SECRET_SCOPE,
            "name": BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME,
            "algorithm": "AES-256-GCM",
            "key_id": key_id,
            "source": key.as_ref().map(|key| key.source),
            "storage": "dedicated_backup_key_store_outside_backup_root",
            "exists_in_secret_store": key_exists,
            "secret_value_revealed": false,
            "secret_value_in_report": false
        },
        "external_escrow": {
            "required_for_disaster_restore": true,
            "status": if key_exists {
                "operator_confirmation_required"
            } else {
                "blocked_until_restore_drill_or_key_generation"
            },
            "key_must_not_travel_with_backup_artifact": true,
            "artifact_channel_must_be_separate_from_key_channel": true,
            "acceptable_targets": [
                "organisation secret manager",
                "hardware security module",
                "approved break-glass credential vault"
            ],
            "non_goal": "CTOX does not export the raw key in this status command."
        },
        "operator_actions": [
            "Run a restore-drill so the portable backup key exists before release evidence is captured.",
            "Escrow the key material from the dedicated CTOX backup key store into an approved organisation secret manager through the operator's secret-handling process.",
            "Record reviewer, date and evidence revision in docs/business-os-security-privacy-signoff.json after confirming escrow."
        ]
    }))
}

pub(super) fn business_os_inspect_backup_manifest_integrity(
    root: &Path,
    manifest: &Value,
) -> anyhow::Result<Value> {
    let integrity = manifest.get("manifest_integrity").unwrap_or(&Value::Null);
    let mut signed_payload = manifest.clone();
    if let Some(object) = signed_payload.as_object_mut() {
        object.remove("manifest_integrity");
    }
    let signed_payload_bytes = serde_json::to_vec_pretty(&signed_payload)?;
    let signed_payload_sha256 = hex_sha256(&signed_payload_bytes);
    let expected_payload_sha256 = integrity
        .get("signed_payload_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload_sha256_matches =
        !expected_payload_sha256.is_empty() && expected_payload_sha256 == signed_payload_sha256;
    let signature_b64 = integrity
        .get("signature_b64")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let signing_key = business_os_read_backup_manifest_signing_key(root)?;
    let signature_valid = if let (Some(signing_key), Ok(signature)) = (
        signing_key.as_ref(),
        base64::engine::general_purpose::STANDARD.decode(signature_b64),
    ) {
        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &signing_key.bytes);
        hmac::verify(&hmac_key, &signed_payload_bytes, &signature).is_ok()
    } else {
        false
    };
    Ok(serde_json::json!({
        "status": if signature_valid { "valid" } else { "invalid" },
        "algorithm": integrity.get("algorithm").cloned().unwrap_or(Value::Null),
        "payload_sha256_matches": payload_sha256_matches,
        "signed_payload_sha256": signed_payload_sha256,
        "expected_signed_payload_sha256": expected_payload_sha256,
        "signature_present": !signature_b64.is_empty(),
        "signing_key_present": signing_key.is_some(),
        "signature_valid": signature_valid,
        "signing_secret_scope": BUSINESS_OS_SECRET_SCOPE,
        "signing_secret_name": BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
        "signing_key_storage": "dedicated_backup_key_store_outside_backup_root",
        "secret_value_in_manifest": false
    }))
}

fn business_os_backup_restore_manifest_compatibility(manifest: &Value) -> Value {
    let schema_version = manifest.get("schema_version").and_then(Value::as_i64);
    let backup_ctox_version = manifest
        .pointer("/restore_compatibility/ctox_version")
        .and_then(Value::as_str);
    business_os_backup_restore_version_compatibility(schema_version, backup_ctox_version)
}

fn business_os_backup_restore_version_compatibility(
    schema_version: Option<i64>,
    backup_ctox_version: Option<&str>,
) -> Value {
    let current_version = env!("CARGO_PKG_VERSION");
    let schema_supported = schema_version == Some(BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION);
    let backup_ctox_version = backup_ctox_version.unwrap_or_default();
    let version_relation = compare_semverish_versions(backup_ctox_version, current_version)
        .unwrap_or_else(|| "unknown".to_owned());
    let same_version = backup_ctox_version == current_version;
    let downgrade_restore = version_relation == "newer_than_runtime";
    let cross_version_restore = !same_version && !backup_ctox_version.is_empty();
    let allowed = schema_supported && same_version;
    let reason = if !schema_supported {
        "unsupported_manifest_schema"
    } else if backup_ctox_version.is_empty() {
        "missing_backup_ctox_version"
    } else if downgrade_restore {
        "backup_created_by_newer_runtime_downgrade_blocked"
    } else if cross_version_restore {
        "cross_version_restore_requires_release_level_evidence"
    } else {
        "same_version_restore_supported"
    };
    serde_json::json!({
        "allowed": allowed,
        "reason": reason,
        "manifest_schema_version": schema_version,
        "supported_manifest_schema_versions": {
            "min": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION,
            "max": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION
        },
        "backup_ctox_version": backup_ctox_version,
        "current_ctox_version": current_version,
        "version_relation": version_relation,
        "same_version_restore_supported": same_version && schema_supported,
        "automatic_cross_version_restore_supported": false,
        "cross_version_restore_requested": cross_version_restore,
        "downgrade_restore_supported": false,
        "downgrade_restore_requested": downgrade_restore
    })
}

fn compare_semverish_versions(left: &str, right: &str) -> Option<String> {
    let left = parse_semverish_triplet(left)?;
    let right = parse_semverish_triplet(right)?;
    Some(
        match left.cmp(&right) {
            std::cmp::Ordering::Less => "older_than_runtime",
            std::cmp::Ordering::Equal => "same_as_runtime",
            std::cmp::Ordering::Greater => "newer_than_runtime",
        }
        .to_owned(),
    )
}

fn parse_semverish_triplet(value: &str) -> Option<(u64, u64, u64)> {
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn business_os_inspect_portable_backup_export(
    root: &Path,
    manifest_path: &Path,
    manifest: &Value,
) -> anyhow::Result<Value> {
    let export = manifest
        .get("portable_encrypted_export")
        .unwrap_or(&Value::Null);
    let ciphertext_path = export
        .pointer("/ciphertext/path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_sha256 = export
        .pointer("/ciphertext/sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let resolved_path = resolve_backup_manifest_artifact_path(root, manifest_path, ciphertext_path);
    let (exists, size_bytes, actual_sha256) = if resolved_path.is_file() {
        let (size, sha256) = file_sha256(&resolved_path)?;
        (true, Some(size), Some(sha256))
    } else {
        (false, None, None)
    };
    let ciphertext_sha256_matches =
        exists && !expected_sha256.is_empty() && actual_sha256.as_deref() == Some(expected_sha256);
    Ok(serde_json::json!({
        "present": export.is_object(),
        "encrypted": export.get("encrypted").and_then(Value::as_bool).unwrap_or(false),
        "algorithm": export.get("algorithm").cloned().unwrap_or(Value::Null),
        "ciphertext_path": ciphertext_path,
        "resolved_ciphertext_path": business_os_restore_drill_relative_path(root, &resolved_path),
        "ciphertext_exists": exists,
        "ciphertext_size_bytes": size_bytes,
        "ciphertext_sha256": actual_sha256,
        "expected_ciphertext_sha256": expected_sha256,
        "ciphertext_sha256_matches": ciphertext_sha256_matches,
        "key_secret_value_in_manifest": export
            .pointer("/key/secret_value_in_manifest")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "key_escrow_required_for_disaster_restore": export
            .pointer("/key/escrow_required_for_disaster_restore")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "verification_status": export
            .pointer("/verification/status")
            .cloned()
            .unwrap_or(Value::Null)
    }))
}

fn resolve_backup_manifest_artifact_path(
    root: &Path,
    manifest_path: &Path,
    artifact_path: &str,
) -> PathBuf {
    let path = Path::new(artifact_path);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let root_relative = root.join(path);
    if root_relative.exists() {
        return root_relative;
    }
    manifest_path.parent().unwrap_or(root).join(path)
}

fn business_os_active_root_restore_runbook(
    root: &Path,
    restore_root: Option<&Path>,
    manifest_path: Option<&Path>,
    manifest_sha256: Option<&str>,
    module_scope: Option<&str>,
    backup_key_separation_verified: Option<bool>,
) -> Value {
    let restore_root_path =
        restore_root.map(|path| business_os_restore_drill_relative_path(root, path));
    let manifest_path =
        manifest_path.map(|path| business_os_restore_drill_relative_path(root, path));
    serde_json::json!({
        "schema_version": 1,
        "status": "manual_operator_runbook",
        "destructive_restore_performed": false,
        "requires_operator_confirmation": true,
        "quiesce_required": true,
        "restart_required": true,
        "module_scope": module_scope,
        "staged_restore_root": restore_root_path,
        "snapshot_manifest_path": manifest_path,
        "snapshot_manifest_sha256": manifest_sha256,
        "scope_note": "This runbook documents the active-root restore gates. The drill validates an isolated restore and does not overwrite the running installation.",
        "operator_gates": [
            {
                "id": "confirm-incident-owner",
                "required": true,
                "label": "Incident-Verantwortlichen und Restore-Ziel bestaetigen"
            },
            {
                "id": "quiesce-business-os",
                "required": true,
                "label": "Business-OS Desktop, native RxDB Peer, MCP Gateway und Schreibautomationen stoppen"
            },
            {
                "id": "capture-current-active-root",
                "required": true,
                "label": "Aktuelles Active Root vor dem Restore erneut sichern"
            },
            {
                "id": "verify-snapshot-manifest",
                "required": true,
                "label": "Snapshot-Manifest und SHA-256 gegen das Drill-Ergebnis pruefen"
            },
            {
                "id": "verify-snapshot-signature",
                "required": true,
                "label": "Snapshot-Manifest-Signatur mit dem Backup-Signing-Key aus dem getrennten Backup-Key-Store pruefen"
            },
            {
                "id": "verify-restore-compatibility",
                "required": true,
                "label": "Manifest-Schema, CTOX-Version und Downgrade-Policy vor Active-Root-Restore pruefen"
            },
            {
                "id": "verify-portable-encrypted-export",
                "required": true,
                "label": "Verschluesselten Portable-Export per Decrypt/ZIP-Check und Manifest-Hash pruefen"
            },
            {
                "id": "confirm-portable-key-escrow",
                "required": true,
                "label": "Portable-Backup-Schluessel ausserhalb des verschluesselten Artefakts in einem Organisations-Secret-Store bestaetigen"
            },
            {
                "id": "restore-runtime-surfaces",
                "required": true,
                "label": "SQLite Stores, installierte App-Roots, Source-Snapshots und Audit-Exports aus dem Restore-Root zurueckspielen"
            },
            {
                "id": "restart-and-validate",
                "required": true,
                "label": "Business-OS neu starten und Restore-Readiness plus App-Sichtbarkeit erneut pruefen"
            }
        ],
        "restore_targets": [
            "runtime/ctox.sqlite3",
            "runtime/ctox-secrets.sqlite3",
            format!("runtime/{STORE_FILE}"),
            format!("runtime/{RXDB_STORE_FILE}"),
            "runtime/business-os/installed-modules",
            "runtime/business-os-source-snapshots",
            BUSINESS_AUDIT_EXPORT_DIR
        ],
        "manifest_requirements": {
            "artifact_schema": "ctox.business_os.backup_restore_snapshot_manifest.v1",
            "supported_schema_versions": {
                "min": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION,
                "max": BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION
            },
            "signature_required": true,
            "signature_algorithm": "HMAC-SHA256",
            "signing_secret_scope": BUSINESS_OS_SECRET_SCOPE,
            "signing_secret_name": BUSINESS_OS_BACKUP_MANIFEST_SIGNING_SECRET_NAME,
            "portable_export_required": true,
            "portable_export_algorithm": "AES-256-GCM",
            "portable_export_secret_scope": BUSINESS_OS_SECRET_SCOPE,
            "portable_export_secret_name": BUSINESS_OS_BACKUP_PORTABLE_ENCRYPTION_SECRET_NAME,
            "downgrade_restore_supported": false,
            "cross_version_restore_policy": "run_restore_drill_with_target_version_before_active_root_restore"
        },
        "raw_backup_handling": {
            "classification": "sensitive-local-raw-backup",
            "support_attachment_allowed": false,
            "retention_days": BUSINESS_OS_BACKUP_RAW_RETENTION_DAYS,
            "encryption_required_before_off_machine_transfer": true,
            "portable_encrypted_export_required_for_off_machine_transfer": true,
            "portable_key_must_not_travel_with_artifact": backup_key_separation_verified,
            "manifest_signing_key_must_not_travel_with_artifact": backup_key_separation_verified,
            "backup_key_separation_verification_status": if backup_key_separation_verified == Some(true) {
                "verified_by_snapshot_and_decrypted_portable_artifact_content_scan"
            } else {
                "not_verified_for_this_generic_runbook"
            },
            "portable_key_escrow_required_for_disaster_restore": true,
            "operator_delete_after_drill": true
        },
        "post_restore_checks": [
            {
                "id": "sqlite-integrity",
                "required": true,
                "label": "PRAGMA integrity_check fuer alle wiederhergestellten SQLite Stores ist ok"
            },
            {
                "id": "restore-readiness-preflight",
                "required": true,
                "label": "ctox.business_os.backup.restore_drill oder CLI restore-drill liefert keine Blocking-Failures"
            },
            {
                "id": "app-visibility-smoke",
                "required": true,
                "label": "Admin- und Teammitglied-Sichtbarkeit fuer betroffene Apps im Browser neu laden und pruefen"
            },
            {
                "id": "audit-retention-policy",
                "required": true,
                "label": "Typisierte MCP- und native Audit-Retention-Policy nach Restore sichtbar"
            },
            {
                "id": "backup-signing-key",
                "required": true,
                "label": "Backup-Signing-Key ist im getrennten Backup-Key-Store unabhaengig vom Backup vorhanden"
            },
            {
                "id": "portable-backup-key",
                "required": true,
                "label": "Portable-Backup-Schluessel ist verfuegbar, aber nicht im Artefakt oder Manifest offengelegt"
            }
        ],
        "explicit_non_goals": [
            "No active installation is overwritten by this drill.",
            "Browser IndexedDB state that has not replicated is not restored by this runbook.",
            "Hosted WebRTC resync after restore still needs separate browser-level evidence.",
            "Key escrow into an external organisation secret manager is an operator process, not a local filesystem mutation."
        ]
    })
}

fn business_os_restore_drill_check(
    id: &str,
    label: &str,
    passed: bool,
    severity: &str,
    detail: &str,
) -> Value {
    serde_json::json!({
        "id": id,
        "label": label,
        "passed": passed,
        "severity": severity,
        "detail": detail
    })
}

fn business_os_restore_drill_native_tables(conn: &Connection) -> anyhow::Result<Value> {
    const REQUIRED_TABLES: &[&str] = &[
        "business_users",
        "business_module_acl",
        "business_permission_grants",
        "business_records",
        "business_commands",
        "business_module_releases",
        "business_module_versions",
        "business_events",
    ];
    let mut tables = Vec::new();
    let mut required_tables_present = true;
    for table in REQUIRED_TABLES {
        let exists = sqlite_table_exists(conn, table)?;
        required_tables_present &= exists;
        let row_count = if exists {
            sqlite_table_row_count(conn, table)?
        } else {
            0
        };
        tables.push(serde_json::json!({
            "name": table,
            "exists": exists,
            "row_count": row_count
        }));
    }
    Ok(serde_json::json!({
        "required_tables_present": required_tables_present,
        "tables": tables
    }))
}

fn business_os_restore_drill_release_summary(
    conn: &Connection,
    module_scope: Option<&str>,
) -> anyhow::Result<Value> {
    let mut sql = String::from(
        "SELECT version_id, module_id, status, snapshot_json
         FROM business_module_releases",
    );
    if module_scope.is_some() {
        sql.push_str(" WHERE module_id = ?1");
    }
    sql.push_str(" ORDER BY module_id ASC, version DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(module_id) = module_scope {
        stmt.query_map(params![module_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    drop(stmt);
    let mut release_record_count = 0i64;
    let mut released_count = 0i64;
    let mut rollback_target_count = 0i64;
    let mut modules = BTreeSet::new();
    let mut sample = Vec::new();
    for (version_id, module_id, status, snapshot_json) in &rows {
        if outbound_load_record(conn, "business_module_releases", version_id)?.is_some() {
            release_record_count += 1;
        }
        if status == "released" {
            released_count += 1;
        }
        let snapshot = serde_json::from_str::<Value>(snapshot_json).unwrap_or(Value::Null);
        let rollback_version_id = snapshot
            .get("rollback_version_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !rollback_version_id.is_empty() {
            rollback_target_count += 1;
        }
        modules.insert(module_id.clone());
        if sample.len() < 20 {
            sample.push(serde_json::json!({
                "version_id": version_id,
                "module_id": module_id,
                "status": status,
                "has_rollback_target": !rollback_version_id.is_empty()
            }));
        }
    }
    Ok(serde_json::json!({
        "release_count": rows.len() as i64,
        "released_count": released_count,
        "business_record_projection_count": release_record_count,
        "rollback_target_count": rollback_target_count,
        "module_count": modules.len() as i64,
        "sample": sample
    }))
}

fn business_os_restore_drill_service_state(root: &Path) -> anyhow::Result<Value> {
    let path = crate::persistence::sqlite_path(root);
    let path_summary = business_os_restore_drill_path_summary(root, &path)?;
    let mut table_present = false;
    let mut typed_mcp_policy_present = false;
    let mut typed_audit_retention_policy_present = false;
    let mut typed_audit_retention_policy_valid = false;
    let mut native_audit_retention_days: Option<i64> = None;
    if path.is_file() {
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("failed to open service state store {}", path.display()))?;
        table_present = sqlite_table_exists(&conn, "ctox_payload_store")?;
        if table_present {
            typed_mcp_policy_present = conn
                .query_row(
                    "SELECT 1 FROM ctox_payload_store WHERE payload_key = ?1 LIMIT 1",
                    params![BUSINESS_OS_MCP_POLICY_PAYLOAD_KEY],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            let audit_policy_json: Option<String> = conn
                .query_row(
                    "SELECT payload_json FROM ctox_payload_store WHERE payload_key = ?1 LIMIT 1",
                    params![BUSINESS_OS_AUDIT_RETENTION_POLICY_PAYLOAD_KEY],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(audit_policy_json) = audit_policy_json {
                typed_audit_retention_policy_present = true;
                if let Ok(payload) =
                    serde_json::from_str::<BusinessAuditRetentionPolicyPayload>(&audit_policy_json)
                {
                    if validate_business_audit_retention_days(payload.retention_days).is_ok() {
                        typed_audit_retention_policy_valid = true;
                        native_audit_retention_days = Some(payload.retention_days);
                    }
                }
            }
        }
    }
    Ok(serde_json::json!({
        "path": path_summary,
        "payload_table_present": table_present,
        "typed_mcp_policy_present": typed_mcp_policy_present,
        "mcp_policy_storage": BUSINESS_OS_MCP_POLICY_PAYLOAD_KEY,
        "typed_audit_retention_policy_present": typed_audit_retention_policy_present,
        "typed_audit_retention_policy_valid": typed_audit_retention_policy_valid,
        "audit_retention_policy_storage": BUSINESS_OS_AUDIT_RETENTION_POLICY_PAYLOAD_KEY,
        "native_audit_retention_days": native_audit_retention_days
    }))
}

fn business_os_restore_drill_installed_modules(
    root: &Path,
    module_scope: Option<&str>,
) -> anyhow::Result<Value> {
    let installed_modules_root =
        resolve_business_os_installed_app_root(root).join("installed-modules");
    let path = business_os_restore_drill_path_summary(root, &installed_modules_root)?;
    let mut module_ids = Vec::new();
    if installed_modules_root.is_dir() {
        for entry in fs::read_dir(&installed_modules_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let module_id = entry.file_name().to_string_lossy().to_string();
            if module_scope.is_some_and(|scope| scope != module_id) {
                continue;
            }
            if entry.path().join("module.json").is_file() {
                module_ids.push(module_id);
            }
        }
    }
    module_ids.sort();
    module_ids.truncate(100);
    Ok(serde_json::json!({
        "path": path,
        "manifest_count": module_ids.len() as i64,
        "module_ids": module_ids
    }))
}

fn business_os_restore_drill_source_snapshots(
    root: &Path,
    module_scope: Option<&str>,
) -> anyhow::Result<Value> {
    let snapshot_root = root.join("runtime").join("business-os-source-snapshots");
    let path = business_os_restore_drill_path_summary(root, &snapshot_root)?;
    let file_count = if let Some(module_id) = module_scope {
        count_files_recursive(&snapshot_root.join(module_id))?
    } else {
        count_files_recursive(&snapshot_root)?
    };
    Ok(serde_json::json!({
        "path": path,
        "file_count": file_count as i64
    }))
}

fn business_os_restore_drill_json_dir_summary(
    root: &Path,
    relative: &Path,
    kind: &str,
) -> anyhow::Result<Value> {
    let dir = root.join(relative);
    let path = business_os_restore_drill_path_summary(root, &dir)?;
    let mut file_count = 0i64;
    let mut latest_modified_at_ms = 0i64;
    if dir.is_dir() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            file_count += 1;
            let modified = fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            latest_modified_at_ms = latest_modified_at_ms.max(modified);
        }
    }
    Ok(serde_json::json!({
        "kind": kind,
        "path": path,
        "file_count": file_count,
        "latest_modified_at_ms": latest_modified_at_ms
    }))
}

fn business_os_restore_drill_rxdb_summary(root: &Path) -> anyhow::Result<Value> {
    let path = rxdb_store_path(root);
    let path_summary = business_os_restore_drill_path_summary(root, &path)?;
    if !path.is_file() {
        return Ok(serde_json::json!({
            "path": path_summary,
            "module_catalog_present": false,
            "business_module_releases_projection_count": 0,
            "reason": "native RxDB SQLite store is missing"
        }));
    }
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open native RxDB SQLite store {}", path.display()))?;
    let rxdb_path = rxdb_store_path(root);
    let catalog_table = rxdb_collection_table_name(&rxdb_path, &conn, "business_module_catalog");
    let module_catalog_present = if let Some(table) = catalog_table.as_deref() {
        conn.query_row(
            &format!("SELECT 1 FROM {table} WHERE id = 'module-catalog' LIMIT 1"),
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    } else {
        false
    };
    let release_table = rxdb_collection_table_name(&rxdb_path, &conn, "business_module_releases");
    let release_projection_count = if let Some(table) = release_table.as_deref() {
        sqlite_table_row_count(&conn, table)?
    } else {
        0
    };
    Ok(serde_json::json!({
        "path": path_summary,
        "module_catalog_table": catalog_table,
        "module_catalog_present": module_catalog_present,
        "business_module_releases_table": release_table,
        "business_module_releases_projection_count": release_projection_count
    }))
}

fn business_os_restore_drill_path_summary(root: &Path, path: &Path) -> anyhow::Result<Value> {
    let relative_path = business_os_restore_drill_relative_path(root, path);
    let metadata = fs::metadata(path).ok();
    let kind = metadata
        .as_ref()
        .map(|metadata| {
            if metadata.is_dir() {
                "directory"
            } else if metadata.is_file() {
                "file"
            } else {
                "other"
            }
        })
        .unwrap_or("missing");
    let size_bytes = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(fs::Metadata::len)
        .unwrap_or_default();
    let modified_at_ms = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    Ok(serde_json::json!({
        "path": relative_path,
        "exists": kind != "missing",
        "kind": kind,
        "size_bytes": size_bytes,
        "modified_at_ms": modified_at_ms
    }))
}

fn business_os_restore_drill_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn count_files_recursive(root: &Path) -> anyhow::Result<usize> {
    if !root.is_dir() {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            count += count_files_recursive(&entry.path())?;
        } else if file_type.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn sqlite_vacuum_into(source: &Path, target: &Path) -> anyhow::Result<bool> {
    if !source.is_file() {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // Offline operation: never create a second live connection beside a v4
    // owner. Target ownership is acquired before removing an earlier snapshot.
    let _source_owner = MaintenanceOwner::acquire(source)?;
    let _target_owner = MaintenanceOwner::acquire(target)?;
    if target.exists() {
        fs::remove_file(target)?;
    }
    let conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open sqlite snapshot source {}", source.display()))?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let target_text = target.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", params![target_text])
        .with_context(|| format!("failed to snapshot sqlite database to {}", target.display()))?;
    Ok(true)
}

fn sqlite_integrity_summary(path: &Path, kind: &str, relative: &Path) -> anyhow::Result<Value> {
    if !path.is_file() {
        return Ok(serde_json::json!({
            "kind": kind,
            "path": relative.to_string_lossy().replace('\\', "/"),
            "ok": false,
            "result": "missing"
        }));
    }
    let _owner = MaintenanceOwner::acquire(path)?;
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open restored sqlite {}", path.display()))?;
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    Ok(serde_json::json!({
        "kind": kind,
        "path": relative.to_string_lossy().replace('\\', "/"),
        "ok": result == "ok",
        "result": result
    }))
}

fn copy_directory_if_exists(source: &Path, target: &Path) -> anyhow::Result<bool> {
    if !source.is_dir() {
        return Ok(false);
    }
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    copy_directory_recursive(source, target)?;
    Ok(true)
}

fn copy_directory_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_recursive(&entry.path(), &target_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target_path)?;
        }
    }
    Ok(())
}

fn backup_file_manifest(root: &Path) -> anyhow::Result<Vec<Value>> {
    let mut files = Vec::new();
    collect_backup_file_manifest(root, root, &mut files)?;
    files.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(b.get("path").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(files)
}

fn collect_backup_file_manifest(
    root: &Path,
    dir: &Path,
    files: &mut Vec<Value>,
) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_backup_file_manifest(root, &path, files)?;
        } else if file_type.is_file() {
            let (size_bytes, sha256) = file_sha256(&path)?;
            files.push(serde_json::json!({
                "path": path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                "size_bytes": size_bytes,
                "sha256": sha256
            }));
        }
    }
    Ok(())
}

pub(super) fn file_sha256(path: &Path) -> anyhow::Result<(u64, String)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok((size, sha256))
}

#[cfg(test)]
mod tests {
    use super::{
        business_os_backup_restore_version_compatibility,
        business_os_snapshot_backup_key_separation, now_ms,
        prune_business_os_backup_restore_drills, BusinessOsBackupManifestSigningKey,
        BusinessOsBackupPortableEncryptionKey, BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION,
    };
    use anyhow::Context;
    use base64::Engine;
    use serde_json::Value;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    // The separation check is the only thing standing between a backup and a
    // key that travels with it, so it needs its own proof that it fires. The
    // drill test cannot supply that: reverting the key location there is
    // healed by the migration path before the snapshot is taken. Plant the
    // material directly instead.
    #[test]
    fn backup_key_separation_detects_planted_key_material() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let snapshot_root = temp.path();
        std::fs::create_dir_all(snapshot_root.join("runtime"))?;

        let signing_key = BusinessOsBackupManifestSigningKey {
            bytes: vec![7u8; 32],
            key_id: "signing-test".to_owned(),
            source: "test",
        };
        let portable_key = BusinessOsBackupPortableEncryptionKey {
            bytes: vec![9u8; 32],
            key_id: "portable-test".to_owned(),
            source: "test",
        };
        let files = vec![serde_json::json!({"path": "runtime/planted.bin"})];

        std::fs::write(snapshot_root.join("runtime/planted.bin"), b"nothing to see")?;
        let clean = business_os_snapshot_backup_key_separation(
            snapshot_root,
            &files,
            &signing_key,
            &portable_key,
        )?;
        assert_eq!(
            clean.get("passed").and_then(Value::as_bool),
            Some(true),
            "a snapshot without key material must pass"
        );

        // Raw bytes.
        std::fs::write(
            snapshot_root.join("runtime/planted.bin"),
            &portable_key.bytes,
        )?;
        let leaked_raw = business_os_snapshot_backup_key_separation(
            snapshot_root,
            &files,
            &signing_key,
            &portable_key,
        )?;
        assert_eq!(
            leaked_raw.get("passed").and_then(Value::as_bool),
            Some(false),
            "the portable key's raw bytes inside a backed-up file must fail the check"
        );

        // And the base64 form, which is how it is actually persisted.
        let encoded = base64::engine::general_purpose::STANDARD.encode(&signing_key.bytes);
        std::fs::write(
            snapshot_root.join("runtime/planted.bin"),
            format!("key={encoded}").as_bytes(),
        )?;
        let leaked_encoded = business_os_snapshot_backup_key_separation(
            snapshot_root,
            &files,
            &signing_key,
            &portable_key,
        )?;
        assert_eq!(
            leaked_encoded.get("passed").and_then(Value::as_bool),
            Some(false),
            "the signing key in base64 form must fail the check too"
        );
        Ok(())
    }

    #[test]
    fn backup_manifest_version_compatibility_blocks_cross_version_and_downgrade(
    ) -> anyhow::Result<()> {
        let current = env!("CARGO_PKG_VERSION");
        let same = business_os_backup_restore_version_compatibility(
            Some(BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION),
            Some(current),
        );
        assert_eq!(same.get("allowed").and_then(Value::as_bool), Some(true));
        assert_eq!(
            same.get("reason").and_then(Value::as_str),
            Some("same_version_restore_supported")
        );

        let older = business_os_backup_restore_version_compatibility(
            Some(BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION),
            Some("0.0.1"),
        );
        assert_eq!(older.get("allowed").and_then(Value::as_bool), Some(false));
        assert_eq!(
            older.get("reason").and_then(Value::as_str),
            Some("cross_version_restore_requires_release_level_evidence")
        );

        let newer = business_os_backup_restore_version_compatibility(
            Some(BUSINESS_OS_BACKUP_MANIFEST_SCHEMA_VERSION),
            Some("999.0.0"),
        );
        assert_eq!(newer.get("allowed").and_then(Value::as_bool), Some(false));
        assert_eq!(
            newer.get("reason").and_then(Value::as_str),
            Some("backup_created_by_newer_runtime_downgrade_blocked")
        );
        assert_eq!(
            newer
                .get("downgrade_restore_requested")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn backup_restore_drill_prune_deletes_expired_drill_dirs() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let backup_root = crate::paths::backup_dir(root);
        fs::create_dir_all(&backup_root)?;

        let expired = backup_root.join("business-os-drill-expired");
        let fresh = backup_root.join("business-os-drill-fresh");
        let missing_policy = backup_root.join("business-os-drill-missing-policy");
        for dir in [&expired, &fresh, &missing_policy] {
            fs::create_dir_all(dir)?;
            fs::write(dir.join("payload.sqlite3"), "raw-drill-payload")?;
        }
        fs::write(
            expired.join("manifest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "raw_backup_security": {
                    "retention": {
                        "expires_at_ms": 1
                    }
                }
            }))?,
        )?;
        fs::write(
            fresh.join("manifest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "raw_backup_security": {
                    "retention": {
                        "expires_at_ms": now_ms() as i64 + 86_400_000
                    }
                }
            }))?,
        )?;
        fs::write(missing_policy.join("manifest.json"), "{}")?;

        let dry_run = prune_business_os_backup_restore_drills(root, true)?;
        assert_eq!(
            dry_run.get("expired_drill_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            dry_run.get("deleted_drill_count").and_then(Value::as_u64),
            Some(0)
        );
        assert!(expired.is_dir());
        assert!(fresh.is_dir());
        assert!(missing_policy.is_dir());

        let applied = prune_business_os_backup_restore_drills(root, false)?;
        assert_eq!(
            applied.get("expired_drill_count").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            applied.get("deleted_drill_count").and_then(Value::as_u64),
            Some(1)
        );
        assert!(!expired.exists());
        assert!(fresh.is_dir());
        assert!(missing_policy.is_dir());
        let items = applied
            .get("items")
            .and_then(Value::as_array)
            .context("prune items")?;
        assert!(
            items.iter().any(|item| {
                item.get("drill_id").and_then(Value::as_str)
                    == Some("business-os-drill-missing-policy")
                    && item
                        .get("retention_policy_present")
                        .and_then(Value::as_bool)
                        == Some(false)
                    && item.get("deleted").and_then(Value::as_bool) == Some(false)
            }),
            "drills without retention policy must be reported but not deleted"
        );
        Ok(())
    }
}
