//! `lock` / `fence` — named advisory lease (CATALOG Nr. 20). Exactly one
//! holder per (app, key); TTL on the Core clock (§16.8); every successful
//! claim increments a per-key fence that is part of the token. A release with
//! a token whose fence is no longer current is rejected deterministically as
//! `conflict: stale_fence` — the previous holder can never release a lease a
//! later claimant took over after expiry. The key is opaque app data.
use super::*;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Deserialize;

pub const DEFAULT_TTL_MS: u64 = 60_000;
pub const MIN_TTL_MS: u64 = 1_000;
pub const MAX_TTL_MS: u64 = 15 * 60_000;
pub const STALE_FENCE: &str = "stale_fence";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    key: String,
    ttl_ms: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    token: String,
}

fn open(root: &Path) -> Result<rusqlite::Connection, CtoxError> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(native_error)?;
    }
    let conn = rusqlite::Connection::open(path).map_err(native_error)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(native_error)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_capability_locks (
            app_id TEXT NOT NULL,
            lock_key TEXT NOT NULL,
            fence INTEGER NOT NULL,
            token TEXT,
            holder_command_id TEXT,
            holder_call_id TEXT,
            expires_at_ms INTEGER NOT NULL,
            PRIMARY KEY (app_id, lock_key)
        );",
    )
    .map_err(native_error)?;
    Ok(conn)
}
fn token_fence(token: &str) -> Option<i64> {
    let (fence, rest) = token.split_once('.')?;
    (rest.len() == 32 && rest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| fence.parse::<i64>().ok())
        .flatten()
        .filter(|f| *f > 0)
}

pub(super) fn execute(
    root: &Path,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
    now_ms: u64,
) -> Result<Value, CtoxError> {
    let now = now_ms.min(i64::MAX as u64) as i64;
    match function {
        "claim" => {
            let request: Claim = decode(args)?;
            text(&request.key, 256, false, "$.key")?;
            let ttl = request.ttl_ms.unwrap_or(DEFAULT_TTL_MS);
            if !(MIN_TTL_MS..=MAX_TTL_MS).contains(&ttl) {
                return Err(invalid("$.ttl_ms"));
            }
            let mut conn = open(root)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(native_error)?;
            let row: Option<(i64, Option<String>, Option<String>, Option<String>, i64)> = tx
                .query_row(
                    "SELECT fence, token, holder_command_id, holder_call_id, expires_at_ms
                       FROM app_capability_locks WHERE app_id=?1 AND lock_key=?2",
                    params![ctx.app_id, request.key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()
                .map_err(native_error)?;
            let previous_fence = row.as_ref().map_or(0, |r| r.0);
            if let Some((fence, Some(token), holder_command, holder_call, expires)) = &row {
                let live = *expires > now;
                if live {
                    // A retry of the same call returns its own lease once.
                    if holder_command.as_deref() == Some(ctx.command_id.as_str())
                        && holder_call.as_deref() == Some(ctx.call_id.as_str())
                    {
                        return Ok(json!({"token":token,"fence":fence,"expires_at_ms":expires}));
                    }
                    return Ok(
                        json!({"token":Value::Null,"fence":Value::Null,"expires_at_ms":Value::Null}),
                    );
                }
            }
            let fence = previous_fence + 1;
            let identity = serde_json::to_vec(&(
                &ctx.app_id,
                &request.key,
                fence,
                &ctx.command_id,
                &ctx.call_id,
            ))
            .map_err(native_error)?;
            let token = format!("{fence}.{}", &sha256_hex(&identity)[..32]);
            let expires = now.saturating_add(ttl as i64);
            tx.execute(
                "INSERT INTO app_capability_locks
                    (app_id, lock_key, fence, token, holder_command_id, holder_call_id, expires_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(app_id, lock_key) DO UPDATE SET fence=excluded.fence,
                    token=excluded.token, holder_command_id=excluded.holder_command_id,
                    holder_call_id=excluded.holder_call_id, expires_at_ms=excluded.expires_at_ms",
                params![ctx.app_id, request.key, fence, token, ctx.command_id, ctx.call_id, expires],
            )
            .map_err(native_error)?;
            tx.commit().map_err(native_error)?;
            Ok(json!({"token":token,"fence":fence,"expires_at_ms":expires}))
        }
        "release" => {
            let request: Release = decode(args)?;
            let presented = token_fence(&request.token).ok_or_else(|| invalid("$.token"))?;
            let mut conn = open(root)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(native_error)?;
            let row: Option<(String, i64, Option<String>)> = tx
                .query_row(
                    "SELECT lock_key, fence, token FROM app_capability_locks
                      WHERE app_id=?1 AND fence=?2 AND token=?3",
                    params![ctx.app_id, presented, request.token],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(native_error)?;
            let Some((key, current_fence, _)) = row else {
                // Either a later claimant raised the fence, or the lease was
                // already released: the token no longer names the holder.
                return Err(CtoxError::new(ErrorCode::Conflict, STALE_FENCE));
            };
            tx.execute(
                "UPDATE app_capability_locks SET token=NULL, holder_command_id=NULL,
                    holder_call_id=NULL, expires_at_ms=0 WHERE app_id=?1 AND lock_key=?2 AND fence=?3",
                params![ctx.app_id, key, current_fence],
            )
            .map_err(native_error)?;
            tx.commit().map_err(native_error)?;
            Ok(json!({"released":true}))
        }
        _ => Err(CtoxError::new(ErrorCode::NotFound, "unknown lock function")),
    }
}
