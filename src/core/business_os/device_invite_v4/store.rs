//! Private pairing state on the existing Sync writer. Never a replicated collection.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ctox_store::{writer::WriterHandle, StoreError, WriteJob};
use ring::{
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_TTL_MS: i64 = 86_400_000;
const KEY_DOMAIN: &[u8] = b"ctox.device.invite.key.v1\0";
const PROOF_DOMAIN: &[u8] = b"ctox.device.invite.redeem.v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reject {
    InviteUnknown,
    InviteExpired,
    InviteUsed,
    ProofInvalid,
    DeviceRejected,
    RateLimited,
}
impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InviteUnknown => "invite_unknown",
            Self::InviteExpired => "invite_expired",
            Self::InviteUsed => "invite_used",
            Self::ProofInvalid => "proof_invalid",
            Self::DeviceRejected => "device_rejected",
            Self::RateLimited => "rate_limited",
        })
    }
}
impl std::error::Error for Reject {}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Redeem {
    pub invite_id: String,
    pub proof: String,
    pub device_pubkey: String,
    pub device_label: String,
}
// Deliberately no Debug: creation includes the one-time secret.
#[derive(Serialize)]
pub struct Invitation {
    pub invite_id: String,
    pub one_time_secret: String,
    pub expires_at_ms: i64,
}
#[derive(Clone)]
pub struct Principal {
    pub id: String,
    pub role: String,
    pub display_name: String,
}
pub struct Prepared {
    request: Redeem,
    pub principal: Principal,
}
impl Prepared {
    pub fn device_pubkey(&self) -> &str {
        &self.request.device_pubkey
    }
}
#[derive(Clone)]
pub struct Invites {
    writer: WriterHandle,
}
type Checked<T> = Result<Result<T, Reject>, StoreError>;
fn rejected<T>(reason: Reject) -> Checked<T> {
    Ok(Err(reason))
}
fn clock() -> Result<i64, Reject> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .ok_or(Reject::DeviceRejected)
}
fn key(secret: &[u8]) -> [u8; 32] {
    Sha256::digest([KEY_DOMAIN, secret].concat()).into()
}
fn message(id: &str, public: &[u8]) -> Vec<u8> {
    [
        PROOF_DOMAIN,
        &(id.len() as u32).to_be_bytes(),
        id.as_bytes(),
        public,
    ]
    .concat()
}
fn decode(value: &str) -> Option<[u8; 32]> {
    if value.len() != 43 {
        return None;
    }
    let bytes: [u8; 32] = URL_SAFE_NO_PAD.decode(value).ok()?.try_into().ok()?;
    (URL_SAFE_NO_PAD.encode(bytes) == value).then_some(bytes)
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn principal(
    tx: &Transaction<'_>,
    user: &str,
    role: &str,
) -> Result<Option<Principal>, StoreError> {
    if !matches!(role, "chef" | "admin" | "founder" | "user") {
        return Ok(None);
    }
    let Some(collection) = ctox_store::schema::active_collection(tx, "business_users")? else {
        return Ok(None);
    };
    let table = collection.table.replace('"', "\"\"");
    let raw: Option<String> = tx
        .query_row(
            &format!("SELECT data FROM \"{table}\" WHERE id=?1 AND deleted=0"),
            [user],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| StoreError::Invalid("invalid pairing principal".into()))?;
    if value["id"] != user || value["active"] != true || value["role"] != role {
        return Ok(None);
    }
    Ok(Some(Principal {
        id: user.into(),
        role: role.into(),
        display_name: value["display_name"].as_str().unwrap_or(user).into(),
    }))
}
impl Invites {
    /// Attach only to the daemon's existing writer; never opens a database.
    pub async fn attach(writer: WriterHandle) -> Result<Self, Reject> {
        writer.submit(WriteJob::new(Default::default(),|tx| {
            tx.execute_batch("CREATE TABLE IF NOT EXISTS _workjet_invites_v4 (
                invite_id TEXT PRIMARY KEY, proof_key BLOB NOT NULL CHECK(length(proof_key)=32),
                user_id TEXT NOT NULL, role TEXT NOT NULL, created_ms INTEGER NOT NULL,
                expires_ms INTEGER NOT NULL, used_ms INTEGER, revoked INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE IF NOT EXISTS _workjet_pairing_budget_v4 (
                singleton INTEGER PRIMARY KEY CHECK(singleton=1), window_ms INTEGER NOT NULL, attempts INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS _workjet_devices_v4 (
                device_key TEXT PRIMARY KEY, user_id TEXT NOT NULL, role TEXT NOT NULL,
                label TEXT NOT NULL, paired_ms INTEGER NOT NULL, active INTEGER NOT NULL DEFAULT 1);")?;
            Ok(())
        })).map_err(|_|Reject::DeviceRejected)?.await.map_err(|_|Reject::DeviceRejected)?;
        Ok(Self { writer })
    }
    /// Identity comes from the authenticated create command, never its JSON payload.
    pub async fn create(
        &self,
        user: String,
        role: String,
        ttl_ms: i64,
    ) -> Result<Invitation, Reject> {
        if !(1..=MAX_TTL_MS).contains(&ttl_ms) {
            return Err(Reject::DeviceRejected);
        }
        let now = clock()?;
        let expires = now.checked_add(ttl_ms).ok_or(Reject::DeviceRejected)?;
        let mut secret = [0; 32];
        SystemRandom::new()
            .fill(&mut secret)
            .map_err(|_| Reject::DeviceRejected)?;
        let proof_key = key(&secret);
        let id = uuid::Uuid::new_v4().to_string();
        let stored = id.clone();
        self.writer.submit(WriteJob::new(Default::default(),move|tx| {
            if principal(tx,&user,&role)?.is_none() { return rejected(Reject::DeviceRejected); }
            tx.execute("DELETE FROM _workjet_invites_v4 WHERE expires_ms<?1",[now-MAX_TTL_MS])?;
            let count:i64=tx.query_row("SELECT COUNT(*) FROM _workjet_invites_v4",[],|r|r.get(0))?;
            if count>=4096 { return rejected(Reject::RateLimited); }
            tx.execute("INSERT INTO _workjet_invites_v4(invite_id,proof_key,user_id,role,created_ms,expires_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![stored,proof_key.as_slice(),user,role,now,expires])?;
            Ok(Ok(()))
        })).map_err(|_|Reject::DeviceRejected)?.await.map_err(|_|Reject::DeviceRejected)??;
        Ok(Invitation {
            invite_id: id,
            one_time_secret: URL_SAFE_NO_PAD.encode(secret),
            expires_at_ms: expires,
        })
    }
    /// Preparation never spends an invite. Mint E41/E50 only after this succeeds.
    pub async fn prepare(&self, request: Redeem) -> Result<Prepared, Reject> {
        let now = clock()?;
        self.writer.submit(WriteJob::new(Default::default(),move|tx| {
            let prior:Option<(i64,i64)>=tx.query_row("SELECT window_ms,attempts FROM _workjet_pairing_budget_v4 WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (window,attempts)=prior.filter(|(start,_)|now>=*start && now-*start<60_000).unwrap_or((now,0));
            if attempts>=120 { return rejected(Reject::RateLimited); }
            tx.execute("INSERT INTO _workjet_pairing_budget_v4 VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET window_ms=excluded.window_ms,attempts=excluded.attempts",params![window,attempts+1])?;
            inspect(tx,request,now)
        })).map_err(|_|Reject::DeviceRejected)?.await.map_err(|_|Reject::DeviceRejected)?
    }
    /// Recheck and consume in the same writer transaction. Only its winner may
    /// publish the already minted launch context to the exact pairing connection.
    pub async fn consume(&self, prepared: Prepared) -> Result<(), Reject> {
        let now = clock()?;
        self.writer.submit(WriteJob::new(Default::default(),move|tx| {
            let current=match inspect(tx,prepared.request,now)? {Ok(v)=>v,Err(e)=>return rejected(e)};
            if current.principal.id!=prepared.principal.id || current.principal.role!=prepared.principal.role {return rejected(Reject::DeviceRejected);}
            tx.execute("INSERT INTO _workjet_devices_v4(device_key,user_id,role,label,paired_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(device_key) DO UPDATE SET role=excluded.role,label=excluded.label,paired_ms=excluded.paired_ms",params![current.request.device_pubkey,current.principal.id,current.principal.role,current.request.device_label,now])?;
            tx.execute("UPDATE _workjet_invites_v4 SET used_ms=?2 WHERE invite_id=?1",params![current.request.invite_id,now])?;
            Ok(Ok(()))
        })).map_err(|_|Reject::DeviceRejected)?.await.map_err(|_|Reject::DeviceRejected)?
    }
}
fn inspect(tx: &Transaction<'_>, request: Redeem, now: i64) -> Checked<Prepared> {
    if request.invite_id.len() != 36
        || uuid::Uuid::parse_str(&request.invite_id)
            .ok()
            .is_none_or(|id| id.to_string() != request.invite_id)
    {
        return rejected(Reject::InviteUnknown);
    }
    let Some(public) = decode(&request.device_pubkey) else {
        return rejected(Reject::DeviceRejected);
    };
    if !label(&request.device_label) {
        return rejected(Reject::DeviceRejected);
    }
    let row:Option<(Vec<u8>,String,String,i64,i64,Option<i64>,bool)>=tx.query_row("SELECT proof_key,user_id,role,created_ms,expires_ms,used_ms,revoked FROM _workjet_invites_v4 WHERE invite_id=?1",[&request.invite_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
    let Some((stored, user, role, created, expires, used, revoked)) = row else {
        return rejected(Reject::InviteUnknown);
    };
    if used.is_some() {
        return rejected(Reject::InviteUsed);
    }
    if expires <= now {
        return rejected(Reject::InviteExpired);
    }
    if revoked
        || created > now
        || expires <= created
        || expires - created > MAX_TTL_MS
        || stored.len() != 32
    {
        return rejected(Reject::DeviceRejected);
    }
    let Some(proof) = decode(&request.proof) else {
        return rejected(Reject::ProofInvalid);
    };
    if hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, &stored),
        &message(&request.invite_id, &public),
        &proof,
    )
    .is_err()
    {
        return rejected(Reject::ProofInvalid);
    }
    let Some(principal) = principal(tx, &user, &role)? else {
        return rejected(Reject::DeviceRejected);
    };
    let bound: Option<(String, bool)> = tx
        .query_row(
            "SELECT user_id,active FROM _workjet_devices_v4 WHERE device_key=?1",
            [&request.device_pubkey],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if bound.is_some_and(|(owner, active)| owner != user || !active) {
        return rejected(Reject::DeviceRejected);
    }
    Ok(Ok(Prepared { request, principal }))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
