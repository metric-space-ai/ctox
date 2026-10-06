// Origin: CTOX
// License: AGPL-3.0-only

//! Owner-bound native endpoint authority shared by build and TransferEngine.
//! No connection credentials or endpoint records are projected into browser DBs.

use super::computer_capabilities::{
    validate_capabilities, ComputerCapability, StorageProtocol, StoragePurpose,
};
use super::store::{
    open_store, outbound_load_record, outbound_load_records_by_string_field,
    upsert_business_record, BusinessCommand,
};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const COMPUTER_ENDPOINT_CONTRACT: &str = "ctox.computer-endpoints.v1";
const ENDPOINTS: &str = "workjet_computer_endpoints";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecretReference {
    pub scope: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "protocol", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComputerEndpoint {
    Ssh {
        host: String,
        port: u16,
        username: String,
        root: String,
        host_key_sha256: String,
        private_key: SecretReference,
        #[serde(default)]
        passphrase: Option<SecretReference>,
    },
    Smb {
        host: String,
        port: u16,
        username: String,
        share: String,
        root: String,
        password: SecretReference,
    },
}

impl ComputerEndpoint {
    pub fn protocol(&self) -> StorageProtocol {
        match self {
            Self::Ssh { .. } => StorageProtocol::Ssh,
            Self::Smb { .. } => StorageProtocol::Smb,
        }
    }

    pub fn root(&self) -> &str {
        match self {
            Self::Ssh { root, .. } | Self::Smb { root, .. } => root,
        }
    }

    /// Credential order for the bounded callback: SSH key, optional passphrase;
    /// or SMB password. Values are borrowed only while native authority is held.
    pub fn credential_references(&self) -> Vec<&SecretReference> {
        match self {
            Self::Ssh {
                private_key,
                passphrase,
                ..
            } => {
                let mut refs = vec![private_key];
                refs.extend(passphrase.iter());
                refs
            }
            Self::Smb { password, .. } => vec![password],
        }
    }

    fn validate(&self) -> Result<()> {
        let (host, port, username) = match self {
            Self::Ssh {
                host,
                port,
                username,
                host_key_sha256,
                ..
            } => {
                let pin = host_key_sha256
                    .strip_prefix("SHA256:")
                    .context("SSH requires a SHA256 host-key pin")?;
                let decoded = STANDARD_NO_PAD
                    .decode(pin)
                    .context("invalid SSH host-key pin")?;
                anyhow::ensure!(
                    decoded.len() == 32 && STANDARD_NO_PAD.encode(decoded) == pin,
                    "invalid SSH host-key pin"
                );
                (host, port, username)
            }
            Self::Smb {
                host,
                port,
                username,
                share,
                ..
            } => {
                label(share, 128)?;
                anyhow::ensure!(
                    !share.contains(['/', '\\', ':']) && share != "." && share != "..",
                    "invalid SMB share"
                );
                (host, port, username)
            }
        };
        label(host, 253)?;
        let ip = host.parse::<std::net::IpAddr>().is_ok();
        let dns = host.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
        anyhow::ensure!(ip || dns, "host must be an IP address or DNS name");
        anyhow::ensure!(*port > 0, "endpoint port must be positive");
        label(username, 128)?;
        anyhow::ensure!(
            !username.starts_with('-')
                && !username.chars().any(char::is_whitespace)
                && !username.contains(['/', ':', '@']),
            "invalid endpoint username"
        );
        absolute_root(self.root())?;
        for reference in self.credential_references() {
            label(&reference.scope, 256)?;
            label(&reference.name, 256)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EndpointUse {
    Build,
    Storage { purpose: StoragePurpose },
}

/// Identity fields must originate in a verified native session/job owner.
#[derive(Debug, Clone)]
pub struct ComputerEndpointRequest {
    pub owner_user_id: String,
    pub computer_id: String,
    pub endpoint_ref: String,
    pub usage: EndpointUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedComputerEndpoint {
    pub contract: String,
    pub owner_user_id: String,
    pub computer_id: String,
    pub endpoint_ref: String,
    pub endpoint_revision: String,
    pub capability_epoch: u64,
    pub connection: ComputerEndpoint,
    pub grant: ComputerCapability,
    pub usage: EndpointUse,
    /// Non-secret digest of endpoint, grant, owner and encrypted credential
    /// revisions. Persist with each job; a changed digest requires a new job.
    pub fingerprint: String,
}

/// Resolves a current snapshot without exposing plaintext credentials. NFS is
/// deliberately unsupported in this increment; declaring it is not IO support.
pub fn resolve_computer_endpoint(
    root: &Path,
    request: &ComputerEndpointRequest,
) -> Result<ResolvedComputerEndpoint> {
    with_endpoint_authority(root, request, None, |endpoint, _| Ok(endpoint.clone()))
}

/// Enter before worker/Core/controller locks. Perform at most one bounded,
/// synchronous protocol operation; never await, retain credentials, or reenter
/// APIs. Both native endpoint/grant mutations and secret rotation are fenced
/// until this callback returns. Adapters enforce a deadline (currently 10s) and
/// at most 1 MiB per data operation. Call again before every operation and resume.
pub fn with_current_computer_endpoint<T>(
    root: &Path,
    request: &ComputerEndpointRequest,
    expected_fingerprint: &str,
    apply: impl FnOnce(&ResolvedComputerEndpoint, &[&[u8]]) -> Result<T>,
) -> Result<T> {
    anyhow::ensure!(
        !expected_fingerprint.is_empty(),
        "job endpoint fingerprint is missing"
    );
    with_endpoint_authority(root, request, Some(expected_fingerprint), apply)
}

fn with_endpoint_authority<T>(
    root: &Path,
    request: &ComputerEndpointRequest,
    expected: Option<&str>,
    apply: impl FnOnce(&ResolvedComputerEndpoint, &[&[u8]]) -> Result<T>,
) -> Result<T> {
    // Schema initialization happens before taking the secret authority fence.
    let conn = open_store(root)?;
    conn.busy_timeout(std::time::Duration::ZERO)?;
    let initial = resolve_record(&conn, request)?;
    let refs = initial.connection.credential_references();
    let keys = refs
        .iter()
        .map(|r| (r.scope.as_str(), r.name.as_str()))
        .collect::<Vec<_>>();
    crate::secrets::with_current_secret_values_and_fingerprint(
        root,
        &keys,
        |values, credential_revision| {
            let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
            let mut current = resolve_record(&tx, request)?;
            anyhow::ensure!(
                current.connection.credential_references() == refs,
                "endpoint credential references changed; resolve again"
            );
            current.fingerprint = format!(
                "sha256:{:x}",
                Sha256::digest(serde_json::to_vec(&json!({
                    "endpoint": current,
                    "credential_revision": credential_revision,
                }))?)
            );
            if let Some(expected) = expected {
                anyhow::ensure!(
                    current.fingerprint == expected,
                    "job endpoint authority changed; explicit new job required"
                );
            }
            // tx and the secret-store fence stay alive through the bounded operation.
            apply(&current, values)
        },
    )
}

fn resolve_record(
    conn: &Connection,
    request: &ComputerEndpointRequest,
) -> Result<ResolvedComputerEndpoint> {
    label(&request.owner_user_id, 256)?;
    label(&request.computer_id, 256)?;
    opaque_ref(&request.endpoint_ref)?;
    let computer = assigned_computer(conn, &request.owner_user_id, &request.computer_id)?;
    let record = outbound_load_record(conn, ENDPOINTS, &request.endpoint_ref)?
        .context("computer endpoint is not registered")?;
    ensure_binding(&record, &request.owner_user_id, &request.computer_id)?;
    anyhow::ensure!(
        record["enabled"] == true && record["is_deleted"] != true && record["_deleted"] != true,
        "computer endpoint is disabled"
    );
    let connection: ComputerEndpoint = serde_json::from_value(record["connection"].clone())
        .context("invalid persisted computer endpoint")?;
    connection.validate()?;
    let mut capabilities: Vec<ComputerCapability> =
        serde_json::from_value(computer["capability_config"].clone())
            .context("computer has no valid capability configuration")?;
    let agentless = computer["agentless"].as_bool().unwrap_or(false);
    validate_capabilities(&mut capabilities, agentless)?;
    let grant = capabilities
        .into_iter()
        .find(|capability| match (&request.usage, capability) {
            (EndpointUse::Build, ComputerCapability::Build(build)) => {
                !agentless
                    && build.ssh_endpoint_ref == request.endpoint_ref
                    && connection.protocol() == StorageProtocol::Ssh
                    && contained_root(connection.root(), &build.lane_root)
            }
            (EndpointUse::Storage { purpose }, ComputerCapability::Storage(storage)) => {
                storage.endpoint_ref == request.endpoint_ref
                    && storage.protocol == connection.protocol()
                    && contained_root(connection.root(), &storage.root)
                    && storage.purposes.contains(purpose)
            }
            _ => false,
        })
        .context("current computer capability does not authorize this endpoint, root or purpose")?;
    Ok(ResolvedComputerEndpoint {
        contract: COMPUTER_ENDPOINT_CONTRACT.to_owned(),
        owner_user_id: request.owner_user_id.clone(),
        computer_id: request.computer_id.clone(),
        endpoint_ref: request.endpoint_ref.clone(),
        endpoint_revision: record["_rev"]
            .as_str()
            .context("endpoint revision missing")?
            .to_owned(),
        capability_epoch: computer["capability_epoch"].as_u64().unwrap_or(0),
        connection,
        grant,
        usage: request.usage.clone(),
        fingerprint: String::new(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpsertPayload {
    endpoint_ref: String,
    computer_id: String,
    connection: ComputerEndpoint,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisablePayload {
    endpoint_ref: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListPayload {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

pub(super) fn is_endpoint_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.workjet.computer.endpoint.upsert"
            | "ctox.workjet.computer.endpoint.disable"
            | "ctox.workjet.computer.endpoint.list"
    )
}

/// Only reached after verified Owner/Admin and IntegrationsManage policy checks.
pub(super) fn handle_command(root: &Path, command: &BusinessCommand, owner: &str) -> Result<Value> {
    label(owner, 256)?;
    let mut conn = open_store(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcome = match command.command_type.as_str() {
        "ctox.workjet.computer.endpoint.upsert" => {
            let payload: UpsertPayload = serde_json::from_value(command.payload.clone())?;
            opaque_ref(&payload.endpoint_ref)?;
            label(&payload.computer_id, 256)?;
            payload.connection.validate()?;
            assigned_computer(&tx, owner, &payload.computer_id)?;
            let existing = outbound_load_record(&tx, ENDPOINTS, &payload.endpoint_ref)?;
            if let Some(existing) = &existing {
                ensure_binding(existing, owner, &payload.computer_id)?;
            }
            let desired = json!({
                "owner_user_id": owner, "computer_id": payload.computer_id,
                "connection": payload.connection, "enabled": true, "is_deleted": false,
            });
            let unchanged = existing.as_ref().is_some_and(|record| {
                [
                    "owner_user_id",
                    "computer_id",
                    "connection",
                    "enabled",
                    "is_deleted",
                ]
                .iter()
                .all(|field| record[*field] == desired[*field])
            });
            let record = if unchanged {
                existing.unwrap()
            } else {
                upsert_business_record(
                    &tx,
                    ENDPOINTS,
                    &payload.endpoint_ref,
                    super::store::now_ms() as i64,
                    desired,
                )?;
                outbound_load_record(&tx, ENDPOINTS, &payload.endpoint_ref)?
                    .context("endpoint reload failed")?
            };
            json!({"ok": true, "endpoint": record})
        }
        "ctox.workjet.computer.endpoint.disable" => {
            let payload: DisablePayload = serde_json::from_value(command.payload.clone())?;
            opaque_ref(&payload.endpoint_ref)?;
            let mut record = outbound_load_record(&tx, ENDPOINTS, &payload.endpoint_ref)?
                .context("computer endpoint is not registered")?;
            anyhow::ensure!(
                record["owner_user_id"] == owner,
                "endpoint belongs to a different owner"
            );
            if record["enabled"] != false {
                record["enabled"] = json!(false);
                upsert_business_record(
                    &tx,
                    ENDPOINTS,
                    &payload.endpoint_ref,
                    super::store::now_ms() as i64,
                    record,
                )?;
                record = outbound_load_record(&tx, ENDPOINTS, &payload.endpoint_ref)?
                    .context("endpoint reload failed")?;
            }
            json!({"ok": true, "endpoint": record})
        }
        "ctox.workjet.computer.endpoint.list" => {
            let payload: ListPayload = serde_json::from_value(command.payload.clone())?;
            let mut endpoints =
                outbound_load_records_by_string_field(&tx, ENDPOINTS, "owner_user_id", owner)?;
            endpoints.retain(|record| record["is_deleted"] != true && record["_deleted"] != true);
            endpoints.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
            endpoints.truncate(payload.limit.unwrap_or(100).clamp(1, 100));
            json!({"ok": true, "endpoints": endpoints})
        }
        _ => anyhow::bail!("unsupported computer endpoint command"),
    };
    tx.commit()?;
    Ok(outcome)
}

fn assigned_computer(conn: &Connection, owner: &str, computer_id: &str) -> Result<Value> {
    let record = outbound_load_record(
        conn,
        super::store_workjet_computers::COMPUTERS_COLLECTION,
        computer_id,
    )?
    .context("computer is not registered")?;
    anyhow::ensure!(
        record["owner_user_id"] == owner,
        "computer belongs to a different owner"
    );
    anyhow::ensure!(
        record["status"] == "assigned"
            && record["is_deleted"] != true
            && record["_deleted"] != true
            && matches!(
                record["hosting_mode"].as_str(),
                Some("workstation" | "self_hosted")
            ),
        "computer is not an assigned self-hosted/workstation target"
    );
    Ok(record)
}

fn ensure_binding(record: &Value, owner: &str, computer_id: &str) -> Result<()> {
    anyhow::ensure!(
        record["owner_user_id"] == owner && record["computer_id"] == computer_id,
        "endpoint owner/computer binding is immutable"
    );
    Ok(())
}

fn label(value: &str, limit: usize) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value == value.trim()
            && value.chars().count() <= limit
            && !value.chars().any(char::is_control),
        "invalid endpoint field"
    );
    Ok(())
}

fn opaque_ref(value: &str) -> Result<()> {
    label(value, 256)?;
    anyhow::ensure!(
        value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)),
        "endpoint_ref must be opaque"
    );
    Ok(())
}

fn absolute_root(value: &str) -> Result<()> {
    label(value, 4096)?;
    anyhow::ensure!(
        value.starts_with('/')
            && !value.contains('\\')
            && (value == "/" || !value.ends_with('/'))
            && !value.contains("//")
            && !value.split('/').any(|part| part == "." || part == ".."),
        "endpoint root must be an absolute normalized path"
    );
    Ok(())
}

fn contained_root(parent: &str, child: &str) -> bool {
    absolute_root(child).is_ok()
        && (parent == "/"
            || parent == child
            || child
                .strip_prefix(parent)
                .is_some_and(|suffix| suffix.starts_with('/')))
}

#[cfg(test)]
#[path = "computer_endpoints_tests.rs"]
mod tests;
