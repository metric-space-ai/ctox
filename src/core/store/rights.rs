//! Versioned rights data in the Sync Store; all writes use its owning transaction.
use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[path = "scope.rs"]
pub mod scope;
use scope::{FieldMask, Projection, ResolvedScope, RowFilter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Read,
    Write,
    Delete,
    Artifact,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Right {
    pub role: String,
    pub collection: String,
    pub row_filter: RowFilter,
    pub field_mask: FieldMask,
    pub ops: BTreeSet<Operation>,
}
#[derive(Debug, Clone)]
pub struct RightsSnapshot {
    pub rights_ver: u64,
    pub rules: Vec<Right>,
}

fn invalid(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}
fn encode(value: &impl Serialize) -> rusqlite::Result<String> {
    let value = serde_json::to_value(value).map_err(|_| invalid("rights encoding failed"))?;
    String::from_utf8(scope::canonical_json(&value).map_err(invalid)?)
        .map_err(|_| invalid("invalid rights UTF-8"))
}

/// P3 initializes _seq/_changes first. Reuse its exact sequence and compacted log;
/// no second clock, connection or authority cache is introduced.
pub fn initialize(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS _rights_state (
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           rights_ver INTEGER NOT NULL CHECK(rights_ver>0));
         INSERT OR IGNORE INTO _rights_state VALUES(1,1);
         CREATE TABLE IF NOT EXISTS _rights (
           rights_ver INTEGER NOT NULL, role TEXT NOT NULL, collection TEXT NOT NULL,
           row_filter_json TEXT NOT NULL, field_mask_json TEXT NOT NULL, ops TEXT NOT NULL,
           seq INTEGER NOT NULL UNIQUE, id TEXT NOT NULL UNIQUE, data TEXT NOT NULL,
           rev TEXT NOT NULL DEFAULT '0', schema_ver INTEGER NOT NULL DEFAULT 1, deleted INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY(role,collection));
         INSERT OR IGNORE INTO _change_sources
           (collection,gen,table_name,id_col,rev_col,data_col,deleted_col,schema_col,schema_ver,app_ver)
           VALUES('_rights',1,'_rights','id','rev','data','deleted',NULL,1,'4.0');"
    )
}

/// Caller authorizes the policy mutation before submitting this to the Store
/// writer. expected_ver is a CAS; no partial update survives a caught error.
pub fn replace(tx: &Transaction<'_>, expected_ver: u64, rules: &[Right]) -> rusqlite::Result<u64> {
    tx.execute_batch("SAVEPOINT v4_rights_replace")?;
    let result = (|| {
        initialize(tx)?;
        let current: u64 = tx.query_row(
            "SELECT rights_ver FROM _rights_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if current != expected_ver {
            return Err(invalid("stale rights version"));
        }
        let next = current
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(|| invalid("rights version exhausted"))?;
        if rules.len() > 4096 {
            return Err(invalid("too many rights rules"));
        }
        let mut identities = BTreeSet::new();
        for rule in rules {
            if rule.role.is_empty()
                || rule.role.len() > 128
                || rule.collection.is_empty()
                || rule.collection.len() > 256
                || !identities.insert((&rule.role, &rule.collection))
            {
                return Err(invalid("invalid or duplicate rights rule"));
            }
            rule.row_filter.resolve("validate-actor").map_err(invalid)?;
            rule.field_mask.normalize().map_err(invalid)?;
        }
        let old = {
            let mut stmt = tx.prepare(
                "SELECT role,collection FROM _rights WHERE id!='0:' ORDER BY role,collection",
            )?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute("DELETE FROM _rights", [])?;
        for (role, collection) in old {
            if !rules
                .iter()
                .any(|r| r.role == role && r.collection == collection)
            {
                record_change(tx, &role, &collection, next, true)?;
            }
        }
        for rule in rules {
            let seq = record_change(tx, &rule.role, &rule.collection, next, false)?;
            let id = format!("{}:{}{}", rule.role.len(), rule.role, rule.collection);
            let data = serde_json::json!({ "id": id, "rights_ver": next.to_string(),
                "role": rule.role, "collection": rule.collection, "row_filter": rule.row_filter,
                "field_mask": rule.field_mask.normalize().map_err(invalid)?, "ops": rule.ops });
            tx.execute("INSERT INTO _rights(rights_ver,role,collection,row_filter_json,field_mask_json,ops,seq,id,data) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![next, rule.role, rule.collection, encode(&rule.row_filter)?,
                    encode(&rule.field_mask.normalize().map_err(invalid)?)?, encode(&rule.ops)?, seq, id, encode(&data)?])?;
        }
        // A version-only change must also have a seq, including a complete revoke.
        let seq = record_change(tx, "", "", next, false)?;
        tx.execute("INSERT INTO _rights(rights_ver,role,collection,row_filter_json,field_mask_json,ops,seq,id,data) VALUES(?1,'','',?2,?3,'[]',?4,'0:',?5)",
            params![next, encode(&RowFilter::None)?, encode(&FieldMask::Fields { paths: BTreeSet::new() })?,
                seq, encode(&serde_json::json!({"id":"0:","rights_ver":next.to_string()}))?])?;
        tx.execute(
            "UPDATE _rights_state SET rights_ver=?1 WHERE singleton=1",
            [next],
        )?;
        Ok(next)
    })();
    match result {
        Ok(version) => {
            tx.execute_batch("RELEASE v4_rights_replace")?;
            Ok(version)
        }
        Err(error) => {
            tx.execute_batch("ROLLBACK TO v4_rights_replace; RELEASE v4_rights_replace")?;
            Err(error)
        }
    }
}
fn record_change(
    tx: &Transaction<'_>,
    role: &str,
    collection: &str,
    version: u64,
    deleted: bool,
) -> rusqlite::Result<u64> {
    // The length prefix makes role/collection identity injective.
    let id = format!("{}:{role}{collection}", role.len());
    let seq: u64 = tx.query_row(
        "UPDATE _seq SET v=v+1 WHERE singleton=1 RETURNING v",
        [],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO _changes(collection,id,seq,op,rev,gen,schema_ver,app_ver,deleted_at)
         VALUES('_rights',?1,?2,?3,?4,1,1,'4.0',CASE WHEN ?3=1 THEN unixepoch() ELSE NULL END)
         ON CONFLICT(collection,gen,id) DO UPDATE SET seq=excluded.seq,op=excluded.op,
         rev=excluded.rev,deleted_at=excluded.deleted_at",
        params![id, seq, u8::from(deleted), version.to_string()],
    )?;
    Ok(seq)
}

pub fn read(tx: &Transaction<'_>) -> rusqlite::Result<RightsSnapshot> {
    let rights_ver = tx.query_row(
        "SELECT rights_ver FROM _rights_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let mut stmt = tx.prepare("SELECT role,collection,row_filter_json,field_mask_json,ops FROM _rights WHERE id!='0:' ORDER BY role,collection")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut rules = Vec::new();
    for row in rows {
        let (role, collection, filter, mask, ops) = row?;
        rules.push(Right {
            role,
            collection,
            row_filter: serde_json::from_str(&filter)
                .map_err(|_| invalid("invalid stored row filter"))?,
            field_mask: serde_json::from_str(&mask)
                .map_err(|_| invalid("invalid stored field mask"))?,
            ops: serde_json::from_str(&ops).map_err(|_| invalid("invalid stored rights ops"))?,
        });
    }
    Ok(RightsSnapshot { rights_ver, rules })
}

impl RightsSnapshot {
    pub fn resolve(&self, role: &str, actor: &str) -> scope::Result<ResolvedScope> {
        ResolvedScope::new(
            self.rights_ver,
            self.rules
                .iter()
                .filter(|r| r.role == role && r.ops.contains(&Operation::Read))
                .map(|r| Projection {
                    collection: r.collection.clone(),
                    row_filter: r.row_filter.clone(),
                    field_mask: r.field_mask.clone(),
                })
                .collect(),
            actor,
        )
    }

    /// Both before and after documents must satisfy a write filter. Field masks
    /// limit *changed* fields, preventing partial input from clearing hidden data.
    pub fn authorize_write(
        &self,
        role: &str,
        actor: &str,
        collection: &str,
        before: Option<&serde_json::Value>,
        after: Option<&serde_json::Value>,
    ) -> scope::Result<()> {
        let op = if after.is_none() {
            Operation::Delete
        } else {
            Operation::Write
        };
        let rule = self
            .rules
            .iter()
            .find(|r| r.role == role && r.collection == collection && r.ops.contains(&op))
            .ok_or("revoked")?;
        let filter = rule.row_filter.resolve(actor)?;
        if before.is_none() && after.is_none() {
            return Err("revoked".into());
        }
        for doc in [before, after].into_iter().flatten() {
            if !doc.is_object() || !filter.matches(doc) {
                return Err("revoked".into());
            }
        }
        let mask = rule.field_mask.normalize()?;
        if after.is_some() && !matches!(mask, FieldMask::All) {
            fn changed(
                before: Option<&serde_json::Value>,
                after: Option<&serde_json::Value>,
                path: &str,
                mask: &FieldMask,
            ) -> bool {
                if before == after {
                    return true;
                }
                if !path.is_empty() && mask.allows(path) {
                    return true;
                }
                if before.is_some_and(|v| !v.is_object()) || after.is_some_and(|v| !v.is_object()) {
                    return false;
                }
                match (
                    before.and_then(|v| v.as_object()),
                    after.and_then(|v| v.as_object()),
                ) {
                    (a, b) if a.is_some() || b.is_some() => {
                        let keys: BTreeSet<_> = a
                            .into_iter()
                            .flat_map(|m| m.keys())
                            .chain(b.into_iter().flat_map(|m| m.keys()))
                            .collect();
                        keys.into_iter().all(|key| {
                            changed(
                                a.and_then(|m| m.get(key)),
                                b.and_then(|m| m.get(key)),
                                &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                                mask,
                            )
                        })
                    }
                    _ => false,
                }
            }
            if !changed(before, after, "", &mask) {
                return Err("revoked".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "rights_tests.rs"]
mod tests;

/// One Core-side authority for all active subscriptions. A queued change is
/// projected again at the synchronous frame handoff; callers must not enqueue
/// already-authorized plaintext and send it later outside this boundary.
pub struct RightsRouter(std::sync::Mutex<RouterState>);
struct RouterState {
    instance_id: String,
    epoch: String,
    rights: Option<RightsSnapshot>,
    pending: Option<std::sync::Arc<()>>,
    subscriptions: std::collections::BTreeMap<String, Subscription>,
}
#[derive(Debug, Clone)]
pub struct Subscription {
    pub device_id: String,
    pub actor_id: String,
    pub role: String,
    pub collection: String,
    pub generation: u64,
}
#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    pub subscription_id: String,
    pub previous_scope_hash: String,
    pub scope: ResolvedScope,
}

impl RightsRouter {
    pub fn new(instance_id: String, epoch: String, rights: RightsSnapshot) -> scope::Result<Self> {
        if instance_id.is_empty() || epoch.is_empty() || rights.rights_ver == 0 {
            return Err("invalid rights authority identity".into());
        }
        Ok(Self(std::sync::Mutex::new(RouterState {
            instance_id,
            epoch,
            rights: Some(rights),
            pending: None,
            subscriptions: std::collections::BTreeMap::new(),
        })))
    }

    /// Identity and role come from the authenticated Core session, never from
    /// the subscribe packet. The signed token proves only that resolved scope.
    pub fn subscribe(
        &self,
        id: &str,
        subscription: Subscription,
        token: &scope::SignedScope,
        public_key: &[u8],
        now: u64,
    ) -> scope::Result<ResolvedScope> {
        let mut state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        if id.is_empty()
            || id.len() > 256
            || subscription.generation == 0
            || subscription.device_id.is_empty()
            || subscription.actor_id.is_empty()
            || state.subscriptions.contains_key(id)
            || state.subscriptions.len() >= 4096
        {
            return Err("invalid or duplicate subscription".into());
        }
        let rights = state
            .rights
            .as_ref()
            .ok_or("rights authority unavailable")?;
        let scope = rights.resolve(&subscription.role, &subscription.actor_id)?;
        token.verify(
            public_key,
            &state.instance_id,
            &subscription.device_id,
            &scope,
            now,
        )?;
        if scope.projection(&subscription.collection).is_none() {
            return Err("revoked".into());
        }
        state.subscriptions.insert(id.into(), subscription);
        Ok(scope)
    }

    pub fn unsubscribe(&self, id: &str) -> scope::Result<()> {
        self.0
            .lock()
            .map_err(|_| "rights authority poisoned")?
            .subscriptions
            .remove(id);
        Ok(())
    }

    pub fn active_subscriptions(&self, collection: &str) -> scope::Result<Vec<String>> {
        let state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        if state.rights.is_none() {
            return Err("rights authority unavailable".into());
        }
        Ok(state
            .subscriptions
            .iter()
            .filter(|(_, s)| s.collection == collection)
            .map(|(id, _)| id.clone())
            .collect())
    }

    /// Enter before the Store writer commits. The callback returns only after
    /// commit; all later frame handoffs see its new snapshot or fail closed.
    /// Revoke/token renewal notifications are built from the returned resolved
    /// scopes. Do not wait for network cleanup ACKs before replacing authority.
    pub fn begin_rights_update(&self) -> scope::Result<PendingRightsUpdate> {
        let mut state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let previous = state
            .rights
            .take()
            .ok_or("rights update pending or authority unavailable")?;
        let marker = std::sync::Arc::new(());
        state.pending = Some(marker.clone());
        Ok(PendingRightsUpdate { marker, previous })
    }

    /// Call only with the result of the Store job handle, which resolves AFTER
    /// commit. No mutex/guard crosses the writer queue or an async boundary.
    pub fn finish_rights_update(
        &self,
        update: PendingRightsUpdate,
        committed: scope::Result<RightsSnapshot>,
    ) -> scope::Result<Vec<SubscriptionUpdate>> {
        let mut state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        if !state
            .pending
            .as_ref()
            .is_some_and(|p| std::sync::Arc::ptr_eq(p, &update.marker))
        {
            return Err("foreign or retired rights publication".into());
        }
        state.pending = None;
        let new = committed?;
        if new.rights_ver <= update.previous.rights_ver {
            return Err("rights version must increase".into());
        }
        let updates = state
            .subscriptions
            .iter()
            .map(|(id, s)| {
                Ok(SubscriptionUpdate {
                    subscription_id: id.clone(),
                    previous_scope_hash: update.previous.resolve(&s.role, &s.actor_id)?.hash()?,
                    scope: new.resolve(&s.role, &s.actor_id)?,
                })
            })
            .collect::<scope::Result<Vec<_>>>()?;
        state.rights = Some(new);
        Ok(updates)
    }

    pub fn update_committed(
        &self,
        commit: impl FnOnce(&RightsSnapshot) -> scope::Result<RightsSnapshot>,
    ) -> scope::Result<Vec<SubscriptionUpdate>> {
        let update = self.begin_rights_update()?;
        let result = commit(&update.previous);
        self.finish_rights_update(update, result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_change_frame<T>(
        &self,
        subscription_id: &str,
        token: &scope::SignedScope,
        public_key: &[u8],
        now: u64,
        collection: &str,
        generation: u64,
        id: &str,
        seq: u64,
        document: Option<&serde_json::Value>,
        send_one_frame: impl FnOnce(scope::scope_filter::ProjectedChange) -> scope::Result<T>,
    ) -> scope::Result<T> {
        let state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let s = state
            .subscriptions
            .get(subscription_id)
            .ok_or("subscription retired")?;
        if s.collection != collection || s.generation != generation {
            return Err("subscription mismatch".into());
        }
        let scope = state
            .rights
            .as_ref()
            .ok_or("rights authority unavailable")?
            .resolve(&s.role, &s.actor_id)?;
        token.verify(public_key, &state.instance_id, &s.device_id, &scope, now)?;
        // No read grant now means unproject, never a stale cached projection.
        send_one_frame(scope::scope_filter::project(
            scope.projection(collection),
            collection,
            id,
            seq,
            document,
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_peer_frame<T>(
        &self,
        subscription_id: &str,
        public_key: &[u8],
        now: u64,
        sender: (&str, &scope::PeerLease),
        receiver_lease: &scope::PeerLease,
        packet: &scope::PeerPacket,
        signature: &str,
        payload: &[u8],
        send_one_frame: impl FnOnce() -> scope::Result<T>,
    ) -> scope::Result<T> {
        let state = self.0.lock().map_err(|_| "rights authority poisoned")?;
        let s = state
            .subscriptions
            .get(subscription_id)
            .ok_or("subscription retired")?;
        if packet.collection != s.collection {
            return Err("subscription mismatch".into());
        }
        let scope = state
            .rights
            .as_ref()
            .ok_or("rights authority unavailable")?
            .resolve(&s.role, &s.actor_id)?;
        packet.authorize(
            signature,
            payload,
            public_key,
            sender,
            (&s.device_id, receiver_lease),
            &state.instance_id,
            &state.epoch,
            s.generation,
            &scope,
            now,
        )?;
        send_one_frame()
    }

    /// Execute inside a Store writer job with before/after from its transaction.
    /// Never enqueue or wait for another Store job in this callback. The owning
    /// writer commits after it returns; pending rights changes serialize after
    /// that write and suspend frame delivery in the meantime.
    pub fn with_outbox_commit<T>(
        &self,
        subscription_id: &str,
        op_id: &str,
        before: Option<&serde_json::Value>,
        after: Option<&serde_json::Value>,
        commit: impl FnOnce() -> scope::Result<T>,
    ) -> std::result::Result<T, OutboxError> {
        let denied = || OutboxError::Revoked(scope::scope_filter::RevokedWrite::new(op_id));
        let state = self
            .0
            .lock()
            .map_err(|_| OutboxError::AuthorityUnavailable)?;
        let s = state
            .subscriptions
            .get(subscription_id)
            .ok_or_else(denied)?;
        let rights = state
            .rights
            .as_ref()
            .ok_or(OutboxError::AuthorityUnavailable)?;
        rights
            .authorize_write(&s.role, &s.actor_id, &s.collection, before, after)
            .map_err(|_| denied())?;
        commit().map_err(OutboxError::CommitFailed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxError {
    Revoked(scope::scope_filter::RevokedWrite),
    /// Retain the pending operation; unknown authority is not a revocation receipt.
    AuthorityUnavailable,
    /// The writer's transaction failed; do not report it as a committed rejection.
    CommitFailed(String),
}

/// Linear publication ticket. While it exists the router is suspended, not
/// locked: pending Store jobs can drain without a writer/router lock inversion.
pub struct PendingRightsUpdate {
    marker: std::sync::Arc<()>,
    previous: RightsSnapshot,
}
impl PendingRightsUpdate {
    pub fn expected_version(&self) -> u64 {
        self.previous.rights_ver
    }
}
