// Origin: CTOX
// License: AGPL-3.0-only
//! Machine join with one click (Sync v4 §16.41): core side.
//!
//! The core owns the enrollment state (`business_machine_enrollments`, never
//! synced) and projects the UI document into the platform collection
//! `machine_enrollments`. The join token is single-use, expires after 15
//! minutes, is bound to this instance and is persisted only as a SHA-256 hash.
//! SSH/Tailscale/local process start is bootstrap transport only; the joining
//! machine reaches the instance over the existing native WebRTC pairing
//! (`mobile_invites` + `transfers pair` + `native_data_device`).
use anyhow::Context;
use base64::Engine;
use ring::rand::SecureRandom;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub(crate) const COLLECTION: &str = "machine_enrollments";
pub(crate) const TOKEN_TTL_MS: i64 = 15 * 60 * 1000;
pub(crate) const TOKEN_PREFIX: &str = "ctoxjoin1.";
pub(crate) const BEGIN: &str = "ctox.machine.enroll.begin";
pub(crate) const CANCEL: &str = "ctox.machine.enroll.cancel";
pub(crate) const STEP: &str = "ctox.machine.enroll.step";
pub(crate) const HEARTBEAT: &str = "ctox.machine.heartbeat";

/// Fixed step order of §16.41. Every document always carries all eight.
pub(crate) const STEP_KEYS: [&str; 8] = [
    "token_issued",
    "transport_connected",
    "package_copied",
    "installed",
    "service_started",
    "device_bound",
    "peer_connected",
    "green",
];
/// Written by the desktop that runs the bootstrap transport (creator only).
const CLIENT_STEPS: [&str; 3] = ["transport_connected", "package_copied", "installed"];
/// Written by the joining machine itself (token first, then bound device).
const MACHINE_STEPS: [&str; 3] = ["service_started", "device_bound", "peer_connected"];

/// Deterministic, machine-readable failure classes. `Display` is the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinError {
    TokenExpired,
    TokenUsed,
    TokenInvalid,
    InstanceUnreachable,
    BindRejected,
    ServiceStartFailed,
    EnrollmentNotFound,
    EnrollmentClosed,
    StepForbidden,
    InvalidRequest,
}
impl JoinError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::TokenExpired => "token_expired",
            Self::TokenUsed => "token_used",
            Self::TokenInvalid => "token_invalid",
            Self::InstanceUnreachable => "instance_unreachable",
            Self::BindRejected => "bind_rejected",
            Self::ServiceStartFailed => "service_start_failed",
            Self::EnrollmentNotFound => "enrollment_not_found",
            Self::EnrollmentClosed => "enrollment_closed",
            Self::StepForbidden => "step_forbidden",
            Self::InvalidRequest => "invalid_request",
        }
    }
}
impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for JoinError {}

pub(crate) fn ensure_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_machine_enrollments (
            enrollment_id TEXT PRIMARY KEY,
            instance_id TEXT NOT NULL,
            label TEXT NOT NULL,
            transport TEXT NOT NULL CHECK (transport IN ('local','ssh','tailscale')),
            created_by TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            expires_at_ms INTEGER NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            invite_user_id TEXT,
            redeemed_at_ms INTEGER,
            bound_user_id TEXT,
            heartbeat_at_ms INTEGER,
            status TEXT NOT NULL CHECK (status IN ('pending','running','green','failed','cancelled')),
            steps_json TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn sha256_hex(value: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, value.as_bytes())
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn random_b64(bytes: usize) -> anyhow::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    ring::rand::SystemRandom::new()
        .fill(&mut buffer)
        .map_err(|_| anyhow::anyhow!("machine enrollment randomness unavailable"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer))
}

/// One-time redemption proof derived from the token secret. The machine sends
/// only this (never the token, which also carries the WebRTC invite); the
/// core persists only `sha256(proof)`, so neither table nor command document
/// holds the token, and the consumed proof is worthless (single use).
pub(crate) fn join_proof(secret: &str) -> String {
    sha256_hex(&format!("ctox-machine-join-proof-v1:{secret}"))
}

/// Parsed join token. No Debug/Serialize: it contains the one-time secrets.
pub(crate) struct JoinToken {
    pub enrollment_id: String,
    pub instance_id: String,
    pub secret: String,
    pub source_pin: Option<String>,
    pub invite: Value,
}
impl JoinToken {
    pub(crate) fn encode(&self) -> anyhow::Result<String> {
        let body = json!({"v":1,"e":self.enrollment_id,"i":self.instance_id,"s":self.secret,
            "p":self.source_pin,"inv":self.invite});
        Ok(format!(
            "{TOKEN_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&body)?)
        ))
    }
    pub(crate) fn decode(token: &str) -> Result<Self, JoinError> {
        let body = token
            .trim()
            .strip_prefix(TOKEN_PREFIX)
            .filter(|body| body.len() <= 16 * 1024)
            .ok_or(JoinError::TokenInvalid)?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| JoinError::TokenInvalid)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| JoinError::TokenInvalid)?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty() && v.len() <= 256)
                .map(str::to_owned)
                .ok_or(JoinError::TokenInvalid)
        };
        if value.get("v").and_then(Value::as_u64) != Some(1) {
            return Err(JoinError::TokenInvalid);
        }
        Ok(Self {
            enrollment_id: text("e")?,
            instance_id: text("i")?,
            secret: text("s")?,
            source_pin: value.get("p").and_then(Value::as_str).map(str::to_owned),
            invite: value.get("inv").cloned().unwrap_or(Value::Null),
        })
    }
}

struct Row {
    id: String,
    instance_id: String,
    label: String,
    transport: String,
    created_by: String,
    created_at_ms: i64,
    expires_at_ms: i64,
    token_hash: String,
    invite_user_id: Option<String>,
    redeemed_at_ms: Option<i64>,
    bound_user_id: Option<String>,
    heartbeat_at_ms: Option<i64>,
    status: String,
    steps: Vec<Value>,
}

fn load(conn: &Connection, id: &str) -> anyhow::Result<Option<Row>> {
    ensure_table(conn)?;
    conn.query_row(
        "SELECT enrollment_id, instance_id, label, transport, created_by, created_at_ms,
                expires_at_ms, token_hash, invite_user_id, redeemed_at_ms, bound_user_id,
                heartbeat_at_ms, status, steps_json
         FROM business_machine_enrollments WHERE enrollment_id=?1",
        params![id],
        |r| {
            let steps: String = r.get(13)?;
            Ok(Row {
                id: r.get(0)?,
                instance_id: r.get(1)?,
                label: r.get(2)?,
                transport: r.get(3)?,
                created_by: r.get(4)?,
                created_at_ms: r.get(5)?,
                expires_at_ms: r.get(6)?,
                token_hash: r.get(7)?,
                invite_user_id: r.get(8)?,
                redeemed_at_ms: r.get(9)?,
                bound_user_id: r.get(10)?,
                heartbeat_at_ms: r.get(11)?,
                status: r.get(12)?,
                steps: serde_json::from_str(&steps).unwrap_or_default(),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn store(conn: &Connection, row: &Row) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE business_machine_enrollments SET redeemed_at_ms=?2, bound_user_id=?3,
            heartbeat_at_ms=?4, status=?5, steps_json=?6 WHERE enrollment_id=?1",
        params![
            row.id,
            row.redeemed_at_ms,
            row.bound_user_id,
            row.heartbeat_at_ms,
            row.status,
            serde_json::to_string(&row.steps)?
        ],
    )?;
    Ok(())
}

impl Row {
    /// The §16.41 document. Token hash, invite user and binding stay core-only.
    fn document(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "transport": self.transport,
            "created_by": self.created_by,
            "created_at_ms": self.created_at_ms,
            "expires_at_ms": self.expires_at_ms,
            "status": self.status,
            "steps": self.steps,
        })
    }
    fn open(&self) -> bool {
        matches!(self.status.as_str(), "pending" | "running")
    }
    fn step_mut(&mut self, key: &str) -> &mut Value {
        let index = STEP_KEYS.iter().position(|k| *k == key).unwrap_or(0);
        &mut self.steps[index]
    }
    fn step_state(&self, key: &str) -> &str {
        STEP_KEYS
            .iter()
            .position(|k| *k == key)
            .and_then(|index| self.steps.get(index))
            .and_then(|step| step.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("pending")
    }
    /// Monotonic: done/failed are final per step; a failed step fails the run.
    fn apply(&mut self, update: &StepUpdate, now: i64) -> Result<(), JoinError> {
        if matches!(self.step_state(&update.key), "done" | "failed") {
            return Err(JoinError::StepForbidden);
        }
        let mut step = json!({"key": update.key, "state": update.state, "at_ms": now});
        if let Some(detail) = &update.detail {
            step["detail"] = json!(detail);
        }
        if let Some(code) = &update.error_code {
            step["error_code"] = json!(code);
        }
        *self.step_mut(&update.key) = step;
        self.status = if update.state == "failed" {
            "failed".into()
        } else {
            "running".into()
        };
        Ok(())
    }
    /// green = peer connected (bound machine reported it) + first life sign
    /// observed by the core. Only this function ever writes `green`.
    fn settle_green(&mut self, now: i64) {
        if self.open()
            && self.step_state("peer_connected") == "done"
            && self.heartbeat_at_ms.is_some()
        {
            *self.step_mut("green") = json!({"key":"green","state":"done","at_ms":now});
            self.status = "green".into();
        }
    }
}

pub(crate) struct StepUpdate {
    pub key: String,
    pub state: String,
    pub detail: Option<String>,
    pub error_code: Option<String>,
}
impl StepUpdate {
    pub(crate) fn from_payload(payload: &Value) -> Result<Self, JoinError> {
        let text = |key: &str, max: usize| {
            payload
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty() && v.len() <= max && !v.chars().any(char::is_control))
                .map(str::to_owned)
        };
        let key = text("key", 32).ok_or(JoinError::InvalidRequest)?;
        let state = text("state", 16).ok_or(JoinError::InvalidRequest)?;
        if !STEP_KEYS.contains(&key.as_str())
            || !matches!(state.as_str(), "running" | "done" | "failed")
        {
            return Err(JoinError::InvalidRequest);
        }
        let error_code = text("error_code", 64);
        if error_code
            .as_deref()
            .is_some_and(|code| !code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        {
            return Err(JoinError::InvalidRequest);
        }
        Ok(Self {
            key,
            state,
            detail: text("detail", 512),
            error_code,
        })
    }
}

pub(crate) struct Begun {
    pub document: Value,
    pub join_token: String,
}

/// `ctox.machine.enroll.begin`: issues the single-use token (hash only).
#[allow(clippy::too_many_arguments)]
pub(crate) fn begin(
    conn: &Connection,
    instance_id: &str,
    actor_id: &str,
    label: &str,
    transport: &str,
    invite: Value,
    invite_user_id: Option<&str>,
    source_pin: Option<&str>,
    now: i64,
) -> anyhow::Result<Begun> {
    ensure_table(conn)?;
    let label = label.trim();
    anyhow::ensure!(
        !label.is_empty() && label.len() <= 120 && !label.chars().any(char::is_control),
        JoinError::InvalidRequest
    );
    anyhow::ensure!(
        matches!(transport, "local" | "ssh" | "tailscale"),
        JoinError::InvalidRequest
    );
    anyhow::ensure!(!actor_id.is_empty(), JoinError::InvalidRequest);
    let token = JoinToken {
        enrollment_id: format!("menr_{}", random_b64(12)?),
        instance_id: instance_id.into(),
        secret: random_b64(32)?,
        source_pin: source_pin.map(str::to_owned),
        invite,
    };
    let mut steps: Vec<Value> = STEP_KEYS
        .iter()
        .map(|key| json!({"key": key, "state": "pending", "at_ms": now}))
        .collect();
    steps[0] = json!({"key":"token_issued","state":"done","at_ms":now});
    let row = Row {
        id: token.enrollment_id.clone(),
        instance_id: instance_id.into(),
        label: label.into(),
        transport: transport.into(),
        created_by: actor_id.into(),
        created_at_ms: now,
        expires_at_ms: now + TOKEN_TTL_MS,
        token_hash: sha256_hex(&join_proof(&token.secret)),
        invite_user_id: invite_user_id.map(str::to_owned),
        redeemed_at_ms: None,
        bound_user_id: None,
        heartbeat_at_ms: None,
        status: "pending".into(),
        steps,
    };
    conn.execute(
        "INSERT INTO business_machine_enrollments (enrollment_id, instance_id, label, transport,
            created_by, created_at_ms, expires_at_ms, token_hash, invite_user_id, status, steps_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            row.id,
            row.instance_id,
            row.label,
            row.transport,
            row.created_by,
            row.created_at_ms,
            row.expires_at_ms,
            row.token_hash,
            row.invite_user_id,
            row.status,
            serde_json::to_string(&row.steps)?
        ],
    )?;
    Ok(Begun {
        document: row.document(),
        join_token: token.encode()?,
    })
}

fn open_row(conn: &Connection, id: &str) -> anyhow::Result<Row> {
    let row = load(conn, id)?.ok_or(JoinError::EnrollmentNotFound)?;
    anyhow::ensure!(row.open(), JoinError::EnrollmentClosed);
    Ok(row)
}

/// `ctox.machine.enroll.cancel`: creator only; closes every further write.
pub(crate) fn cancel(conn: &Connection, actor_id: &str, id: &str) -> anyhow::Result<Value> {
    let mut row = open_row(conn, id)?;
    anyhow::ensure!(row.created_by == actor_id, JoinError::StepForbidden);
    row.status = "cancelled".into();
    store(conn, &row)?;
    Ok(row.document())
}

/// Desktop transport steps: creator only, only the three transport keys.
pub(crate) fn client_step(
    conn: &Connection,
    actor_id: &str,
    id: &str,
    update: &StepUpdate,
    now: i64,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        CLIENT_STEPS.contains(&update.key.as_str()),
        JoinError::StepForbidden
    );
    let mut row = open_row(conn, id)?;
    anyhow::ensure!(row.created_by == actor_id, JoinError::StepForbidden);
    if row.redeemed_at_ms.is_none() && now >= row.expires_at_ms {
        anyhow::bail!(JoinError::TokenExpired);
    }
    row.apply(update, now)?;
    store(conn, &row)?;
    Ok(row.document())
}

/// How the joining machine proves itself for a machine step.
pub(crate) enum MachineProof<'a> {
    /// First write: the one-time join proof (redeemed exactly once).
    Token(&'a str),
    /// Later writes: the device already bound by the redemption.
    BoundDevice,
}

/// Machine steps. `actor_id` is the authenticated native peer user and
/// `device_active` says whether its device-proof binding is currently active.
#[allow(clippy::too_many_arguments)]
pub(crate) fn machine_step(
    conn: &Connection,
    instance_id: &str,
    actor_id: &str,
    device_active: bool,
    id: &str,
    proof: MachineProof<'_>,
    update: &StepUpdate,
    now: i64,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        MACHINE_STEPS.contains(&update.key.as_str()),
        JoinError::StepForbidden
    );
    let mut row = open_row(conn, id)?;
    match proof {
        MachineProof::Token(proof) => {
            // Bound to this instance and this enrollment's hash only.
            anyhow::ensure!(
                row.instance_id == instance_id && sha256_hex(proof) == row.token_hash,
                JoinError::TokenInvalid
            );
            anyhow::ensure!(row.redeemed_at_ms.is_none(), JoinError::TokenUsed);
            anyhow::ensure!(now < row.expires_at_ms, JoinError::TokenExpired);
            // Bind only the device that redeemed this enrollment's own invite.
            anyhow::ensure!(
                device_active
                    && row
                        .invite_user_id
                        .as_deref()
                        .is_none_or(|user| user == actor_id),
                JoinError::BindRejected
            );
            row.redeemed_at_ms = Some(now);
            row.bound_user_id = Some(actor_id.into());
        }
        MachineProof::BoundDevice => {
            anyhow::ensure!(
                device_active && row.bound_user_id.as_deref() == Some(actor_id),
                JoinError::BindRejected
            );
        }
    }
    row.apply(update, now)?;
    row.settle_green(now);
    store(conn, &row)?;
    Ok(row.document())
}

/// First (and later) life sign of the bound machine as observed by the core.
pub(crate) fn heartbeat(
    conn: &Connection,
    actor_id: &str,
    device_active: bool,
    id: &str,
    now: i64,
) -> anyhow::Result<Value> {
    let mut row = load(conn, id)?.ok_or(JoinError::EnrollmentNotFound)?;
    anyhow::ensure!(
        device_active && row.bound_user_id.as_deref() == Some(actor_id),
        JoinError::BindRejected
    );
    if row.open() {
        row.heartbeat_at_ms.get_or_insert(now);
        row.settle_green(now);
        store(conn, &row)?;
    }
    Ok(row.document())
}

/// Platform command dispatch for `ctox.machine.*` (command_plane arm).
pub(super) fn handle_command(
    root: &std::path::Path,
    command: &super::store::BusinessCommand,
) -> anyhow::Result<Value> {
    use super::policy::BusinessOsPermission;
    use super::store_policy::{enforce_command_policy, CommandPolicyRequirement};
    let manage =
        |_: &super::session::BusinessOsSession| -> anyhow::Result<CommandPolicyRequirement> {
            Ok(CommandPolicyRequirement::workspace(
                BusinessOsPermission::MachinesManage,
            ))
        };
    let payload = &command.payload;
    let now = super::store::now_ms() as i64;
    let machine_key = payload
        .get("key")
        .and_then(Value::as_str)
        .is_some_and(|key| MACHINE_STEPS.contains(&key));
    match command.command_type.as_str() {
        BEGIN => enforce_command_policy(root, command, manage, |session| {
            settle(
                root,
                command,
                (|| -> anyhow::Result<Value> {
                    let actor = super::session::session_user_id(session)
                        .context(JoinError::InvalidRequest)?;
                    let label = payload.get("label").and_then(Value::as_str).unwrap_or("");
                    anyhow::ensure!(
                        !label.trim().is_empty()
                            && label.len() <= 120
                            && !label.chars().any(char::is_control),
                        JoinError::InvalidRequest
                    );
                    let transport = payload
                        .get("transport")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    anyhow::ensure!(
                        matches!(transport, "local" | "ssh" | "tailscale"),
                        JoinError::InvalidRequest
                    );
                    let config = super::store::sync_config(root)?;
                    // Existing native WebRTC bootstrap: one-time invite (hash-only)
                    // whose device-proof binding happens on the machine's first use.
                    let invite = super::mobile_invites::create(
                        root,
                        TOKEN_TTL_MS / 1000,
                        Some(label.trim()),
                        None,
                    )?;
                    let invite_user = invite
                        .pointer("/invite/session/user/id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let pin = crate::sync_host::signing_identity(root)
                        .map(|identity| identity.public_identity())
                        .ok();
                    let conn = super::store::open_store(root)?;
                    let begun = begin(
                        &conn,
                        &config.instance_id,
                        actor,
                        label,
                        transport,
                        invite.get("invite").cloned().unwrap_or(Value::Null),
                        invite_user.as_deref(),
                        pin.as_deref(),
                        now,
                    )?;
                    project(root, &begun.document, now)?;
                    Ok(json!({
                        "enrollment_id": begun.document["id"],
                        "join_token": begun.join_token,
                        "installer_ref": installer_ref(transport),
                        "enrollment": begun.document,
                    }))
                })(),
            )
        })?
        .into_outcome(),
        CANCEL => enforce_command_policy(root, command, manage, |session| {
            settle(
                root,
                command,
                (|| -> anyhow::Result<Value> {
                    let actor = super::session::session_user_id(session).unwrap_or_default();
                    let conn = super::store::open_store(root)?;
                    let doc = cancel(&conn, actor, &enrollment_id(payload)?)?;
                    project(root, &doc, now)?;
                    Ok(json!({"enrollment": doc}))
                })(),
            )
        })?
        .into_outcome(),
        STEP if !machine_key => enforce_command_policy(root, command, manage, |session| {
            settle(
                root,
                command,
                (|| -> anyhow::Result<Value> {
                    let actor = super::session::session_user_id(session).unwrap_or_default();
                    let update = StepUpdate::from_payload(payload)?;
                    let conn = super::store::open_store(root)?;
                    let doc = client_step(&conn, actor, &enrollment_id(payload)?, &update, now)?;
                    project(root, &doc, now)?;
                    Ok(json!({"enrollment": doc}))
                })(),
            )
        })?
        .into_outcome(),
        STEP | HEARTBEAT => {
            // The joining machine: authenticated native peer identity, never
            // a role. First write proves the token, later ones the bound key.
            let session = super::store::rxdb_authenticated_session(root, command)?;
            let result = (|| -> anyhow::Result<Value> {
                let actor = super::session::session_user_id(&session)
                    .context(JoinError::BindRejected)?
                    .to_owned();
                let active = super::mobile_invites::is_active_paired_device_user(root, &actor);
                let id = enrollment_id(payload)?;
                let conn = super::store::open_store(root)?;
                let doc = if command.command_type == HEARTBEAT {
                    let doc = heartbeat(&conn, &actor, active, &id, now)?;
                    // §16.43: a machine heartbeat is presence of its device.
                    if let Ok(Some(device)) = super::devices::device_id_for_user(&conn, &actor) {
                        super::devices::presence(root, &device, true, now).ok();
                    }
                    doc
                } else {
                    let instance = super::store::sync_config(root)?.instance_id;
                    let token = payload.get("join_proof").and_then(Value::as_str);
                    let proof = token.map_or(MachineProof::BoundDevice, MachineProof::Token);
                    let update = StepUpdate::from_payload(payload)?;
                    machine_step(&conn, &instance, &actor, active, &id, proof, &update, now)?
                };
                project(root, &doc, now)?;
                Ok(json!({"enrollment": doc}))
            })();
            settle(root, command, result)
        }
        other => anyhow::bail!("unsupported machine command type: {other}"),
    }
}

/// Terminalize the command: deterministic `error_code` on contract failures.
fn settle(
    root: &std::path::Path,
    command: &super::store::BusinessCommand,
    result: anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    use super::command_plane::write_rxdb_control_command_outcome as outcome;
    match result {
        Ok(mut value) => {
            value["ok"] = json!(true);
            outcome(root, command, "completed", None, Some("completed"), value)
        }
        Err(error) => match error_code(&error) {
            Some(code) => outcome(
                root,
                command,
                "failed",
                None,
                Some("failed"),
                json!({"ok": false, "error_code": code, "error": code}),
            ),
            None => Err(error),
        },
    }
}

fn project(root: &std::path::Path, doc: &Value, now: i64) -> anyhow::Result<()> {
    let id = doc["id"].as_str().context(JoinError::InvalidRequest)?;
    super::store::upsert_rxdb_collection_record(root, COLLECTION, id, now, doc.clone())
}

/// What the desktop copies/starts per transport. No env toggles: role and
/// token are CLI flags; the installer itself is the existing public script.
pub(crate) fn installer_ref(transport: &str) -> Value {
    json!({
        "kind": "install_sh",
        "transport": transport,
        "script_url": "https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh",
        "package": "install.sh",
        "install_args": ["--no-business-os-autostart"],
        // `--token -` reads the token from stdin: never in argv, ps or logs.
        "join_argv": ["ctox", "machine", "join", "--token", "-", "--label", "<label>"],
        "join_binary": "~/.local/bin/ctox",
        "token_via": "stdin",
    })
}

/// Read the error code out of an anyhow chain (deterministic wire value).
pub(crate) fn error_code(error: &anyhow::Error) -> Option<&'static str> {
    error.downcast_ref::<JoinError>().map(|e| e.code())
}

pub(crate) fn enrollment_id(payload: &Value) -> anyhow::Result<String> {
    payload
        .get("enrollment_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| id.starts_with("menr_") && id.len() <= 64)
        .map(str::to_owned)
        .context(JoinError::InvalidRequest)
}

#[cfg(test)]
#[path = "machine_enrollment_tests.rs"]
mod tests;
