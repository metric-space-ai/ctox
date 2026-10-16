//! E34: identity repair from the native verified alias registry only.
use super::*;
use crate::{read_pool::ReadPool, ReadJob};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const COLLECTIONS: [&str; 2] = ["workjet_computers", "workjet_sessions"];
const MARKER: &str = "e34.workjet-owner-alias.v1";
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Aliases(BTreeMap<String, String>);
impl Aliases {
    /// This table is populated by remember_managed_identity, never profile_json.
    pub fn load(conn: &Connection) -> Result<Self> {
        let exists: bool = sql(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_user_identity_aliases')", [], |r| r.get(0)))?;
        if !exists {
            return Ok(Self::default());
        }
        let mut statement = sql(conn.prepare("SELECT a.alias,a.canonical_user_id FROM business_user_identity_aliases a JOIN business_users u ON u.user_id=a.canonical_user_id WHERE u.active=1 AND a.verified_at_ms>0 ORDER BY a.alias"))?;
        let rows =
            sql(statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (alias, owner) = sql(row)?;
            let alias = alias.to_ascii_lowercase();
            if alias.len() > 320 || !alias.contains('@') || uuid::Uuid::parse_str(&owner).is_err() {
                return Err(invalid("owner_alias_invalid_native_identity"));
            }
            if out
                .insert(alias, owner.clone())
                .is_some_and(|old| old != owner)
            {
                return Err(invalid("owner_alias_ambiguous"));
            }
        }
        Ok(Self(out))
    }
    pub fn extend(&mut self, other: Self) -> Result<()> {
        for (alias, owner) in other.0 {
            if self
                .0
                .insert(alias, owner.clone())
                .is_some_and(|old| old != owner)
            {
                return Err(invalid("owner_alias_ambiguous"));
            }
        }
        Ok(())
    }
    pub fn migrate(&self, collection: &str, body: &mut Value) -> bool {
        if !COLLECTIONS.contains(&collection)
            || body["is_deleted"] == true
            || body["_deleted"] == true
        {
            return false;
        }
        let Some(owner) = body["owner_user_id"].as_str() else {
            return false;
        };
        let Some(canonical) = self.0.get(&owner.to_ascii_lowercase()) else {
            return false;
        };
        if owner == canonical {
            return false;
        }
        body["owner_user_id"] = canonical.clone().into();
        true
    }
}
/// Runs before ingress. A crash between the Sync commit and Core marker replays
/// safely; it cannot publish a second change or certify an incomplete migration.
pub async fn startup(
    core: &WriterHandle,
    sync: &WriterHandle,
    identities: &ReadPool,
) -> Result<()> {
    core.submit(WriteJob::new(JobOptions::default(), |tx| {
        sql(tx.execute_batch("CREATE TABLE IF NOT EXISTS _workjet_owner_migrations(id TEXT PRIMARY KEY,receipt_json TEXT NOT NULL);"))?;
        Ok(())
    }))?.await?;
    let done = core
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            sql(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM _workjet_owner_migrations WHERE id=?1)",
                [MARKER],
                |r| r.get::<_, bool>(0),
            ))
        }))?
        .await?;
    if done {
        return Ok(());
    }
    let aliases = identities
        .read_job(ReadJob::new(JobOptions::default(), Aliases::load))
        .await?;
    let mut counts = BTreeMap::new();
    let mut all_active = true;
    for collection in COLLECTIONS {
        let mut cursor = String::new();
        let mut changed = 0usize;
        loop {
            let after = cursor.clone();
            let aliases = aliases.clone();
            let (next,count,active)=sync.submit(WriteJob::new(JobOptions::default(), move |tx| {
                let Some(active) = sql(crate::schema::active_collection(tx,collection))? else { return Ok((None,0,false)); };
                let table = sql(crate::change_log::identifier(&active.table))?;
                let mut statement=sql(tx.prepare(&format!("SELECT id,data FROM {table} WHERE deleted=0 AND id>?1 ORDER BY id LIMIT 128")))?;
                let rows=sql(statement.query_map([after],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))))?.collect::<rusqlite::Result<Vec<_>>>().map_err(StoreError::from)?;
                drop(statement);
                let next=rows.last().map(|r|r.0.clone());let mut changed=0;
                for (id,raw) in rows {
                    let mut body:Value=serde_json::from_str(&raw).map_err(|_|invalid("owner_alias_invalid_document"))?;
                    if aliases.migrate(collection,&mut body) {
                        let next=body.to_string();let revision=format!("e34-{:x}",Sha256::digest(next.as_bytes()));
                        changed+=sql(tx.execute(&format!("UPDATE {table} SET data=?2,rev=?3 WHERE id=?1 AND data=?4"),rusqlite::params![id,next,revision,raw]))?;
                    }
                }
                Ok((next,changed,true))
            }))?.await?;
            changed += count;
            all_active &= active;
            match next {
                Some(next) => cursor = next,
                None => break,
            }
        }
        counts.insert(collection, changed);
    }
    // Do not seal an empty or not-yet-activated base release as migrated.
    if !all_active {
        return Ok(());
    }
    let receipt = serde_json::to_string(&counts).map_err(|_| invalid("owner_alias_receipt"))?;
    core.submit(WriteJob::new(JobOptions::default(), move |tx| {
        sql(tx.execute(
            "INSERT OR IGNORE INTO _workjet_owner_migrations VALUES(?1,?2)",
            rusqlite::params![MARKER, receipt],
        ))?;
        Ok(())
    }))?
    .await
}
