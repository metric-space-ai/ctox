//! Transaction-only global change log for Sync v4 (§2.2, §16.1).
//! The Store owner supplies the transaction and authorization; no connection is opened.
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type Result<T> = rusqlite::Result<T>;
pub const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MAX_PAGE: usize = 10_000;

pub(crate) fn invalid(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}
pub(crate) fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| invalid("sequence/generation exceeds SQLite INTEGER"))
}
pub(crate) fn identifier(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > 1024
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || value.as_bytes()[0].is_ascii_digit()
    {
        return Err(invalid("invalid SQLite identifier"));
    }
    Ok(format!("\"{value}\""))
}
pub(crate) fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
/// Nested savepoint makes multi-statement helpers atomic even when a caller catches an error.
pub(crate) fn atomic<T>(tx: &Transaction<'_>, f: impl FnOnce() -> Result<T>) -> Result<T> {
    tx.execute_batch("SAVEPOINT v4_store_operation")?;
    match f() {
        Ok(value) => {
            tx.execute_batch("RELEASE v4_store_operation")?;
            Ok(value)
        }
        Err(error) => {
            tx.execute_batch("ROLLBACK TO v4_store_operation; RELEASE v4_store_operation")?;
            Err(error)
        }
    }
}

/// Optional sequence watermarks preserve the same lossless wire encoding.
pub mod optional_decimal {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Serialize, Deserialize)]
    struct Sequence(#[serde(with = "super::decimal")] u64);
    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        value.map(Sequence).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        Option::<Sequence>::deserialize(deserializer).map(|value| value.map(|seq| seq.0))
    }
}

/// Public sequence fields serialize as decimal strings on the wire.
pub mod decimal {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.is_empty()
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(D::Error::custom("canonical decimal sequence required"));
        }
        text.parse().map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub epoch: String,
    #[serde(with = "decimal")]
    pub head_seq: u64,
    #[serde(with = "decimal")]
    pub horizon_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeDocument {
    pub id: String,
    #[serde(with = "decimal")]
    pub seq: u64,
    pub op: Operation,
    pub rev: String,
    pub gen: u64,
    pub schema_ver: u32,
    pub app_ver: String,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangePage {
    pub docs: Vec<ChangeDocument>,
    pub epoch: String,
    #[serde(with = "decimal")]
    pub cursor_to: u64,
    #[serde(with = "decimal")]
    pub head_seq: u64,
    pub backlog: u64,
}

/// Column binding also permits backfilling pre-v4 physical tables without copying them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBinding {
    pub collection: String,
    pub gen: u64,
    pub table: String,
    pub id_column: String,
    pub rev_column: String,
    pub data_column: String,
    pub deleted_column: String,
    pub schema_column: Option<String>,
    pub schema_ver: u32,
    pub app_ver: String,
}
impl TableBinding {
    pub fn generic(
        collection: &str,
        gen: u64,
        table: &str,
        schema_ver: u32,
        app_ver: &str,
    ) -> Self {
        Self {
            collection: collection.into(),
            gen,
            table: table.into(),
            id_column: "id".into(),
            rev_column: "rev".into(),
            data_column: "data".into(),
            deleted_column: "deleted".into(),
            schema_column: Some("schema_ver".into()),
            schema_ver,
            app_ver: app_ver.into(),
        }
    }
    fn validate(&self) -> Result<()> {
        if self.collection.is_empty()
            || self.collection.len() > 256
            || self.table.starts_with('_')
            || self.gen == 0
            || self.app_ver.is_empty()
        {
            return Err(invalid("invalid change-log table binding"));
        }
        integer(self.gen)?;
        for name in [
            &self.table,
            &self.id_column,
            &self.rev_column,
            &self.data_column,
            &self.deleted_column,
        ] {
            identifier(name)?;
        }
        if let Some(name) = &self.schema_column {
            identifier(name)?;
        }
        Ok(())
    }
}

pub fn initialize(tx: &Transaction<'_>) -> Result<Head> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS _seq (
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           v INTEGER NOT NULL CHECK(typeof(v)='integer' AND v>=0));
         INSERT OR IGNORE INTO _seq VALUES(1,0);
         CREATE TABLE IF NOT EXISTS _meta (
           singleton INTEGER PRIMARY KEY CHECK(singleton=1),
           epoch TEXT NOT NULL, horizon_seq INTEGER NOT NULL DEFAULT 0 CHECK(horizon_seq>=0));
         CREATE TABLE IF NOT EXISTS _changes (
           collection TEXT NOT NULL, id TEXT NOT NULL, seq INTEGER NOT NULL UNIQUE,
           op INTEGER NOT NULL CHECK(op IN (0,1)), rev TEXT NOT NULL,
           gen INTEGER NOT NULL CHECK(gen>0), schema_ver INTEGER NOT NULL,
           app_ver TEXT NOT NULL, deleted_at INTEGER,
           PRIMARY KEY(collection,gen,id));
         CREATE INDEX IF NOT EXISTS _changes_page ON _changes(collection,gen,seq);
         CREATE INDEX IF NOT EXISTS _changes_retention ON _changes(deleted_at,seq) WHERE op=1;
         CREATE TABLE IF NOT EXISTS _change_sources (
           collection TEXT NOT NULL, gen INTEGER NOT NULL, table_name TEXT NOT NULL UNIQUE,
           id_col TEXT NOT NULL, rev_col TEXT NOT NULL, data_col TEXT NOT NULL,
           deleted_col TEXT NOT NULL, schema_col TEXT, schema_ver INTEGER NOT NULL,
           app_ver TEXT NOT NULL, PRIMARY KEY(collection,gen));",
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO _meta(singleton,epoch) VALUES(1,?1)",
        [Uuid::new_v4().to_string()],
    )?;
    head(tx)
}

pub fn head(tx: &Transaction<'_>) -> Result<Head> {
    tx.query_row("SELECT epoch,v,horizon_seq FROM _meta CROSS JOIN _seq WHERE _meta.singleton=1 AND _seq.singleton=1",
        [], |row| Ok(Head { epoch: row.get(0)?, head_seq: row.get(1)?, horizon_seq: row.get(2)? }))
}

/// Install three constant-cost triggers. All identifiers are validated and all values quoted.
/// Triggers and their source binding become visible in the caller's one commit.
pub fn install_triggers(tx: &Transaction<'_>, binding: &TableBinding) -> Result<()> {
    binding.validate()?;
    atomic(tx, || {
        initialize(tx)?;
        let gen = integer(binding.gen)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT table_name FROM _change_sources WHERE collection=?1 AND gen=?2",
                params![binding.collection, gen],
                |r| r.get(0),
            )
            .optional()?;
        if existing
            .as_deref()
            .is_some_and(|name| name != binding.table)
        {
            return Err(invalid(
                "generation already bound to another physical table",
            ));
        }
        tx.execute(
            "INSERT INTO _change_sources VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(collection,gen) DO UPDATE SET id_col=excluded.id_col,rev_col=excluded.rev_col,
             data_col=excluded.data_col,deleted_col=excluded.deleted_col,schema_col=excluded.schema_col,
             schema_ver=excluded.schema_ver,app_ver=excluded.app_ver",
            params![binding.collection,gen,binding.table,binding.id_column,binding.rev_column,
                    binding.data_column,binding.deleted_column,binding.schema_column,binding.schema_ver,binding.app_ver])?;
        let table = identifier(&binding.table)?;
        let collection = literal(&binding.collection);
        let app = literal(&binding.app_ver);
        let id = identifier(&binding.id_column)?;
        let rev = identifier(&binding.rev_column)?;
        let deleted = identifier(&binding.deleted_column)?;
        for (event, suffix, row) in [
            ("INSERT", "insert", "NEW"),
            ("UPDATE", "update", "NEW"),
            ("DELETE", "delete", "OLD"),
        ] {
            let trigger = identifier(&format!("v4_{}_{}", binding.table, suffix))?;
            let op = if event == "DELETE" {
                "1".into()
            } else {
                format!("CASE WHEN {row}.{deleted}<>0 THEN 1 ELSE 0 END")
            };
            let schema = match &binding.schema_column {
                Some(name) => format!("{row}.{}", identifier(name)?),
                None => binding.schema_ver.to_string(),
            };
            let write = format!(
                "UPDATE _seq SET v=v+1 WHERE singleton=1;
                 INSERT INTO _changes(collection,id,seq,op,rev,gen,schema_ver,app_ver,deleted_at)
                 VALUES({collection},{row}.{id},(SELECT v FROM _seq WHERE singleton=1),{op},
                        {row}.{rev},{gen},{schema},{app},CASE WHEN ({op})=1 THEN unixepoch() ELSE NULL END)
                 ON CONFLICT(collection,gen,id) DO UPDATE SET seq=excluded.seq,op=excluded.op,
                    rev=excluded.rev,schema_ver=excluded.schema_ver,app_ver=excluded.app_ver,deleted_at=excluded.deleted_at;"
            );
            // A primary-key rename must also withdraw the old id.
            let rename = if event == "UPDATE" {
                format!("UPDATE _seq SET v=v+1 WHERE singleton=1 AND OLD.{id}<>NEW.{id};
                 INSERT INTO _changes(collection,id,seq,op,rev,gen,schema_ver,app_ver,deleted_at)
                 SELECT {collection},OLD.{id},v,1,OLD.{rev},{gen},{schema},{app},unixepoch()
                 FROM _seq WHERE singleton=1 AND OLD.{id}<>NEW.{id}
                 ON CONFLICT(collection,gen,id) DO UPDATE SET seq=excluded.seq,op=1,
                    rev=excluded.rev,schema_ver=excluded.schema_ver,app_ver=excluded.app_ver,deleted_at=excluded.deleted_at;")
            } else {
                String::new()
            };
            tx.execute_batch(&format!(
                "DROP TRIGGER IF EXISTS {trigger};
                CREATE TRIGGER {trigger} AFTER {event} ON {table} BEGIN {rename}{write} END;"
            ))?;
        }
        Ok(())
    })
}

fn binding(tx: &Transaction<'_>, collection: &str, gen: u64) -> Result<TableBinding> {
    tx.query_row(
        "SELECT table_name,id_col,rev_col,data_col,deleted_col,schema_col,schema_ver,app_ver
         FROM _change_sources WHERE collection=?1 AND gen=?2",
        params![collection, integer(gen)?],
        |r| {
            Ok(TableBinding {
                collection: collection.into(),
                gen,
                table: r.get(0)?,
                id_column: r.get(1)?,
                rev_column: r.get(2)?,
                data_column: r.get(3)?,
                deleted_column: r.get(4)?,
                schema_column: r.get(5)?,
                schema_ver: r.get(6)?,
                app_ver: r.get(7)?,
            })
        },
    )
}

/// Read one collection/generation from exactly the transaction's WAL snapshot.
/// A short (including empty) page advances to that snapshot's global head.
pub fn read_page(
    tx: &Transaction<'_>,
    collection: &str,
    gen: u64,
    cursor: u64,
    limit: usize,
) -> Result<ChangePage> {
    if limit == 0 || limit > MAX_PAGE {
        return Err(invalid("page limit must be 1..=10000"));
    }
    let snapshot = head(tx)?; // Establish the read snapshot before consulting schema or rows.
    if cursor > snapshot.head_seq {
        return Err(invalid("cursor is ahead of snapshot"));
    }
    if cursor != 0 && cursor < snapshot.horizon_seq {
        return Err(invalid("cursor expired: epoch resnapshot required"));
    }
    let source = binding(tx, collection, gen)?;
    source.validate()?;
    let table = identifier(&source.table)?;
    let id = identifier(&source.id_column)?;
    let data = identifier(&source.data_column)?;
    let sql = format!(
        "SELECT c.id,c.seq,c.op,c.rev,c.schema_ver,c.app_ver,d.{data}
         FROM _changes c LEFT JOIN {table} d ON d.{id}=c.id
         WHERE c.collection=?1 AND c.gen=?2 AND c.seq>?3 ORDER BY c.seq LIMIT ?4"
    );
    let mut statement = tx.prepare(&sql)?;
    let rows = statement.query_map(
        params![collection, integer(gen)?, integer(cursor)?, limit as i64],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, u32>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    let mut docs = Vec::new();
    for row in rows {
        let (id, seq, op, rev, schema_ver, app_ver, data) = row?;
        let body = if op == 1 {
            None
        } else {
            let data =
                data.ok_or_else(|| invalid("change log references missing live document"))?;
            Some(
                serde_json::from_str(&data)
                    .map_err(|e| invalid(format!("invalid document JSON: {e}")))?,
            )
        };
        docs.push(ChangeDocument {
            id,
            seq,
            op: if op == 1 {
                Operation::Delete
            } else {
                Operation::Upsert
            },
            rev,
            gen,
            schema_ver,
            app_ver,
            body,
        });
    }
    let cursor_to = if docs.len() < limit {
        snapshot.head_seq
    } else {
        docs.last().expect("nonempty page").seq
    };
    let backlog = tx.query_row(
        "SELECT COUNT(*) FROM _changes WHERE collection=?1 AND gen=?2 AND seq>?3",
        params![collection, integer(gen)?, integer(cursor_to)?],
        |r| r.get(0),
    )?;
    Ok(ChangePage {
        docs,
        epoch: snapshot.epoch,
        cursor_to,
        head_seq: snapshot.head_seq,
        backlog,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillPage {
    pub cursor: Option<String>,
    pub processed: usize,
    pub complete: bool,
}

/// Bounded id-ordered bootstrap. Already triggered writes are never overwritten.
pub fn backfill_page(
    tx: &Transaction<'_>,
    collection: &str,
    gen: u64,
    after: Option<&str>,
    limit: usize,
) -> Result<BackfillPage> {
    if limit == 0 || limit > 500 {
        return Err(invalid("backfill limit must be 1..=500"));
    }
    let source = binding(tx, collection, gen)?;
    source.validate()?;
    atomic(tx, || {
        let table = identifier(&source.table)?;
        let id = identifier(&source.id_column)?;
        let rev = identifier(&source.rev_column)?;
        let deleted = identifier(&source.deleted_column)?;
        let schema = match &source.schema_column {
            Some(c) => identifier(c)?,
            None => source.schema_ver.to_string(),
        };
        let mut stmt = tx.prepare(&format!(
            "SELECT {id},{rev},{deleted},{schema} FROM {table}
            WHERE (?1 IS NULL OR {id}>?1) ORDER BY {id} LIMIT ?2"
        ))?;
        let rows = stmt
            .query_map(params![after, limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, u32>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>>>()?;
        for (id, rev, deleted, schema) in &rows {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3)",
                params![collection, integer(gen)?, id],
                |r| r.get(0),
            )?;
            if exists {
                continue;
            }
            tx.execute("UPDATE _seq SET v=v+1 WHERE singleton=1", [])?;
            tx.execute("INSERT INTO _changes(collection,id,seq,op,rev,gen,schema_ver,app_ver,deleted_at)
                VALUES(?1,?2,(SELECT v FROM _seq WHERE singleton=1),?3,?4,?5,?6,?7,CASE WHEN ?3=1 THEN unixepoch() ELSE NULL END)",
                params![collection,id,deleted,rev,integer(gen)?,schema,source.app_ver])?;
        }
        Ok(BackfillPage {
            cursor: rows
                .last()
                .map(|r| r.0.clone())
                .or_else(|| after.map(str::to_owned)),
            processed: rows.len(),
            complete: rows.len() < limit,
        })
    })
}

/// Rotate only for an owner-authorized restore/rebuild or retention compaction.
pub fn rotate_epoch(tx: &Transaction<'_>) -> Result<Head> {
    tx.execute(
        "UPDATE _meta SET epoch=?1 WHERE singleton=1",
        [Uuid::new_v4().to_string()],
    )?;
    head(tx)
}

/// Drop tombstones only after 30 days; invalidate old epoch/cursor proofs atomically.
pub fn purge_tombstones(tx: &Transaction<'_>, now: i64) -> Result<usize> {
    let cutoff = now
        .checked_sub(RETENTION_SECONDS)
        .ok_or_else(|| invalid("retention clock overflow"))?;
    atomic(tx, || {
        let horizon: Option<i64> = tx.query_row(
            "SELECT MAX(seq) FROM (SELECT seq FROM _changes WHERE op=1 AND deleted_at<?1
             ORDER BY deleted_at,seq LIMIT 500)",
            [cutoff],
            |r| r.get(0),
        )?;
        let removed = tx.execute("DELETE FROM _changes WHERE seq IN
            (SELECT seq FROM _changes WHERE op=1 AND deleted_at<?1 ORDER BY deleted_at,seq LIMIT 500)",[cutoff])?;
        if let Some(seq) = horizon {
            tx.execute(
                "UPDATE _meta SET horizon_seq=MAX(horizon_seq,?1) WHERE singleton=1",
                [seq],
            )?;
            rotate_epoch(tx)?;
        }
        Ok(removed)
    })
}

#[cfg(test)]
#[path = "change_log_tests.rs"]
mod tests;
