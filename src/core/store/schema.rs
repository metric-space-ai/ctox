//! Sync v4 schema registry, generation migration and durable publication reservations.
//! All effects use the caller's transaction. Migration JS runs outside the writer.
use super::change_log::{
    self, atomic, identifier, integer, invalid, literal, ChangePage, Operation, Result,
    TableBinding,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MIGRATION_BATCH: usize = 500;
pub const GENERATION_RESERVE_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionDefinition {
    pub name: String,
    pub app_id: String,
    pub schema_ver: u32,
    pub schema_json: Value,
    pub class: String,
    pub client_limit: u64,
    pub byte_budget: u64,
    pub rights_ver: u64,
    pub app_ver: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub definition: CollectionDefinition,
    pub gen: u64,
    pub table: String,
    pub state: String,
    #[serde(with = "change_log::optional_decimal")]
    pub migrated_at_seq: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaChange {
    Compatible,
    Incompatible,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishLock {
    pub app_id: String,
    pub base_app_ver: String,
    pub target_app_ver: String,
    pub operation_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Migration {
    pub publication_id: String,
    pub collection: String,
    pub source_gen: u64,
    pub gen: u64,
    #[serde(with = "change_log::decimal")]
    pub cursor: u64,
    pub state: String,
    pub epoch: String,
    #[serde(with = "change_log::optional_decimal")]
    pub migrated_at_seq: Option<u64>,
}
#[derive(Debug, Clone)]
pub enum MigrationOutput {
    Upsert { rev: String, body: Value },
    Delete,
    Archive { reason: String },
}
#[derive(Debug, Clone)]
pub struct SourceWrite {
    pub id: String,
    pub base_rev: Option<String>,
    pub rev: String,
    pub body: Value,
    pub deleted: bool,
}

fn name(value: &str) -> Result<()> {
    identifier(value)?;
    if value.len() > 96 || value.contains("__g") {
        return Err(invalid("reserved/oversized collection name"));
    }
    Ok(())
}
pub fn physical_table(collection: &str, gen: u64) -> Result<String> {
    name(collection)?;
    if gen == 0 {
        return Err(invalid("generation must be positive"));
    }
    integer(gen)?;
    Ok(if gen == 1 {
        format!("c_{collection}")
    } else {
        format!("c_{collection}__g{gen}")
    })
}
fn validate(def: &CollectionDefinition) -> Result<()> {
    name(&def.name)?;
    if def.app_id.is_empty()
        || def.app_ver.is_empty()
        || !def.schema_json.is_object()
        || !matches!(
            def.class.as_str(),
            "replicated"
                | "demand"
                | "control"
                | "stream"
                | "window"
                | "full"
                | "blob"
                | "projection"
        )
    {
        return Err(invalid("invalid collection definition"));
    }
    integer(def.client_limit)?;
    integer(def.byte_budget)?;
    integer(def.rights_ver)?;
    indexes(&def.schema_json)?;
    Ok(())
}

pub fn initialize(tx: &Transaction<'_>) -> Result<()> {
    change_log::initialize(tx)?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS _collections(
          name TEXT NOT NULL,gen INTEGER NOT NULL,app_id TEXT NOT NULL,schema_ver INTEGER NOT NULL,
          schema_json TEXT NOT NULL CHECK(json_valid(schema_json)),class TEXT NOT NULL,
          client_limit INTEGER NOT NULL,byte_budget INTEGER NOT NULL,rights_ver INTEGER NOT NULL,
          state TEXT NOT NULL CHECK(state IN ('preparing','active','archived')),
          app_ver TEXT NOT NULL,table_name TEXT NOT NULL UNIQUE,migrated_at_seq INTEGER,
          archived_at INTEGER,retire_after INTEGER,PRIMARY KEY(name,gen));
         CREATE UNIQUE INDEX IF NOT EXISTS _collections_active ON _collections(name) WHERE state='active';
         CREATE TABLE IF NOT EXISTS _schema_apps(
          app_id TEXT PRIMARY KEY,app_ver TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS _schema_updates(
          publication_id TEXT NOT NULL,collection TEXT NOT NULL,gen INTEGER NOT NULL,
          definition_json TEXT NOT NULL CHECK(json_valid(definition_json)),
          PRIMARY KEY(publication_id,collection));
         CREATE TABLE IF NOT EXISTS _schema_removals(
          publication_id TEXT NOT NULL,collection TEXT NOT NULL,gen INTEGER NOT NULL,
          PRIMARY KEY(publication_id,collection));
         CREATE TABLE IF NOT EXISTS publish_lock(
          app_id TEXT PRIMARY KEY,base_app_ver TEXT NOT NULL,target_app_ver TEXT NOT NULL,
          operation_id TEXT NOT NULL UNIQUE,created_at INTEGER NOT NULL DEFAULT(unixepoch()));
         CREATE TABLE IF NOT EXISTS _migrations(
          publication_id TEXT NOT NULL,app_ver TEXT NOT NULL,collection TEXT NOT NULL,
          source_gen INTEGER NOT NULL,gen INTEGER NOT NULL,cursor TEXT NOT NULL DEFAULT '0',
          state TEXT NOT NULL CHECK(state IN ('preparing','backfilling','ready','committed','aborted')),
          epoch TEXT NOT NULL,migrated_at_seq INTEGER,
          PRIMARY KEY(publication_id,collection));
         CREATE TABLE IF NOT EXISTS _archive(
          archive_id INTEGER PRIMARY KEY,app_id TEXT NOT NULL,collection TEXT NOT NULL,
          gen INTEGER NOT NULL,id TEXT NOT NULL,rev TEXT NOT NULL,app_ver TEXT NOT NULL,
          data TEXT NOT NULL CHECK(json_valid(data)),reason TEXT NOT NULL,
          archived_at INTEGER NOT NULL,expires_at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS _archive_expiry ON _archive(expires_at);"
    )?;
    Ok(())
}

pub fn collection(tx: &Transaction<'_>, collection: &str, gen: u64) -> Result<Collection> {
    tx.query_row(
        "SELECT app_id,schema_ver,schema_json,class,client_limit,byte_budget,rights_ver,
        state,app_ver,table_name,migrated_at_seq FROM _collections WHERE name=?1 AND gen=?2",
        params![collection, integer(gen)?],
        |r| {
            let schema: String = r.get(2)?;
            let schema_json = serde_json::from_str(&schema).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(Collection {
                definition: CollectionDefinition {
                    name: collection.into(),
                    app_id: r.get(0)?,
                    schema_ver: r.get(1)?,
                    schema_json,
                    class: r.get(3)?,
                    client_limit: r.get(4)?,
                    byte_budget: r.get(5)?,
                    rights_ver: r.get(6)?,
                    app_ver: r.get(8)?,
                },
                gen,
                table: r.get(9)?,
                state: r.get(7)?,
                migrated_at_seq: r.get(10)?,
            })
        },
    )
}
pub fn active_collection(tx: &Transaction<'_>, name: &str) -> Result<Option<Collection>> {
    let gen: Option<u64> = tx
        .query_row(
            "SELECT gen FROM _collections WHERE name=?1 AND state='active'",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    gen.map(|g| collection(tx, name, g)).transpose()
}

fn indexes(schema: &Value) -> Result<Vec<Vec<String>>> {
    let Some(raw) = schema.get("indexes") else {
        return Ok(vec![]);
    };
    let raw = raw
        .as_array()
        .ok_or_else(|| invalid("schema indexes must be an array"))?;
    if raw.len() > 32 {
        return Err(invalid("too many schema indexes"));
    }
    let mut result = Vec::new();
    for entry in raw {
        let fields = if let Some(field) = entry.as_str() {
            vec![field.to_owned()]
        } else {
            entry
                .as_array()
                .ok_or_else(|| invalid("invalid index definition"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("index field must be text"))
                })
                .collect::<Result<Vec<_>>>()?
        };
        if fields.is_empty() || fields.len() > 4 {
            return Err(invalid("index must contain 1..=4 fields"));
        }
        for field in &fields {
            if field.len() > 64 || field.is_empty() {
                return Err(invalid("invalid indexed field length"));
            }
            for segment in field.split('.') {
                identifier(segment)?;
            }
        }
        if result.contains(&fields) {
            return Err(invalid("duplicate index"));
        }
        result.push(fields);
    }
    Ok(result)
}
/// Generated column name for a declared dotted JSON index field.
pub fn indexed_column(field: &str) -> Result<String> {
    indexes(&serde_json::json!({"indexes":[field]}))?;
    Ok(format!("j_{}", encoded(field)))
}

fn encoded(field: &str) -> String {
    field.bytes().map(|b| format!("{b:02x}")).collect()
}
fn create_indexes(tx: &Transaction<'_>, table: &str, schema: &Value) -> Result<()> {
    let table_sql = identifier(table)?;
    for fields in indexes(schema)? {
        let index_name = format!(
            "i_{table}_{}",
            fields
                .iter()
                .map(|f| encoded(f))
                .collect::<Vec<_>>()
                .join("_")
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name=?1)",
            [&index_name],
            |r| r.get(0),
        )?;
        if exists {
            continue;
        }
        let populated: bool = tx.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table_sql} LIMIT 1)"),
            [],
            |r| r.get(0),
        )?;
        if populated {
            return Err(invalid("online_index_build_required: populated generation must use the bounded index runner"));
        }
        let mut columns = Vec::new();
        for field in &fields {
            let column = format!("j_{}", encoded(field));
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_xinfo(?1) WHERE name=?2)",
                params![table, column],
                |r| r.get(0),
            )?;
            if !exists {
                let path = format!(
                    "$.{}",
                    field
                        .split('.')
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(".")
                );
                tx.execute_batch(&format!(
                    "ALTER TABLE {table_sql} ADD COLUMN {}
                    GENERATED ALWAYS AS (json_extract(data,{})) VIRTUAL",
                    identifier(&column)?,
                    literal(&path)
                ))?;
            }
            columns.push(identifier(&column)?);
        }
        tx.execute_batch(&format!(
            "CREATE INDEX {} ON {table_sql}({})",
            identifier(&index_name)?,
            columns.join(",")
        ))?;
    }
    Ok(())
}

fn create_generation(
    tx: &Transaction<'_>,
    def: &CollectionDefinition,
    gen: u64,
    state: &str,
) -> Result<Collection> {
    validate(def)?;
    let foreign: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1 AND app_id<>?2)",
        params![def.name, def.app_id],
        |r| r.get(0),
    )?;
    if foreign {
        return Err(invalid("collection name belongs to another app"));
    }
    let table = physical_table(&def.name, gen)?;
    tx.execute_batch(&format!(
        "CREATE TABLE {}(
        id TEXT PRIMARY KEY NOT NULL,rev TEXT NOT NULL,schema_ver INTEGER NOT NULL,
        data TEXT,
        deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)),
        CHECK((deleted=1 AND data IS NULL) OR
              (deleted=0 AND data IS NOT NULL AND json_valid(data) AND json_type(data)='object')))",
        identifier(&table)?
    ))?;
    create_indexes(tx, &table, &def.schema_json)?;
    change_log::install_triggers(
        tx,
        &TableBinding::generic(&def.name, gen, &table, def.schema_ver, &def.app_ver),
    )?;
    tx.execute("INSERT INTO _collections(name,gen,app_id,schema_ver,schema_json,class,client_limit,
        byte_budget,rights_ver,state,app_ver,table_name) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![def.name,integer(gen)?,def.app_id,def.schema_ver,def.schema_json.to_string(),def.class,
            integer(def.client_limit)?,integer(def.byte_budget)?,integer(def.rights_ver)?,state,def.app_ver,table])?;
    collection(tx, &def.name, gen)
}
/// Atomic table + generated index columns/indexes + three triggers + registry row.
pub fn create_collection(tx: &Transaction<'_>, def: &CollectionDefinition) -> Result<Collection> {
    atomic(tx, || {
        initialize(tx)?;
        let held: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM publish_lock WHERE app_id=?1)",
            [&def.app_id],
            |r| r.get(0),
        )?;
        if held {
            return Err(invalid("busy: collection creation overlaps publication"));
        }

        tx.execute(
            "INSERT INTO _schema_apps VALUES(?1,?2) ON CONFLICT(app_id) DO NOTHING",
            params![def.app_id, def.app_ver],
        )?;
        let current: String = tx.query_row(
            "SELECT app_ver FROM _schema_apps WHERE app_id=?1",
            [&def.app_id],
            |r| r.get(0),
        )?;
        if current != def.app_ver {
            return Err(invalid("collection app version differs"));
        }
        create_generation(tx, def, 1, "active")
    })
}

/// Conservative compatibility: old properties retain their complete definition.
/// New fields require an optional default; destructive/unknown structural edits use a new generation.
pub fn classify_change(
    old: &CollectionDefinition,
    new: &CollectionDefinition,
) -> Result<SchemaChange> {
    validate(old)?;
    validate(new)?;
    if old.name != new.name || old.app_id != new.app_id || new.schema_ver < old.schema_ver {
        return Err(invalid(
            "collection identity changed or schema version regressed",
        ));
    }
    if old.schema_json == new.schema_json {
        return Ok(SchemaChange::Compatible);
    }
    if new.schema_ver <= old.schema_ver {
        return Err(invalid("schema content change requires a newer version"));
    }
    let mut before = old.schema_json.clone();
    let mut after = new.schema_json.clone();
    let old_props = before
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let new_props = after
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let old_required = before
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let new_required = after
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (field, definition) in &old_props {
        if new_props.get(field) != Some(definition) {
            return Ok(SchemaChange::Incompatible);
        }
    }
    for (field, definition) in &new_props {
        if !old_props.contains_key(field)
            && (new_required.contains(&Value::String(field.clone()))
                || definition.get("default").is_none())
        {
            return Ok(SchemaChange::Incompatible);
        }
    }
    if new_required.iter().any(|v| !old_required.contains(v)) {
        return Ok(SchemaChange::Incompatible);
    }
    for value in [&mut before, &mut after] {
        let object = value.as_object_mut().expect("validated object");
        for key in ["properties", "required", "indexes", "version"] {
            object.remove(key);
        }
    }
    Ok(if before == after {
        SchemaChange::Compatible
    } else {
        SchemaChange::Incompatible
    })
}

/// Stage a compatible definition under its durable reservation; active metadata
/// and documents remain untouched until activate_publication's atomic cutover.
pub fn update_compatible(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    def: &CollectionDefinition,
) -> Result<Collection> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        if def.app_id != lock.app_id || def.app_ver != lock.target_app_ver {
            return Err(invalid("schema does not belong to publication"));
        }
        let old = active_collection(tx, &def.name)?
            .ok_or_else(|| invalid("active collection missing"))?;
        if classify_change(&old.definition, def)? != SchemaChange::Compatible {
            return Err(invalid("incompatible change requires a new generation"));
        }
        let migrating: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations
            WHERE publication_id=?1 AND collection=?2)",
            params![lock.operation_id, def.name],
            |r| r.get(0),
        )?;
        if migrating {
            return Err(invalid("collection already has a generation migration"));
        }
        reject_removed(tx, lock, &def.name)?;
        // A native SQLite index on a populated table cannot be built incrementally
        // through &Transaction. The dedicated online-index runner must prepare it.
        ensure_index_build_safe(tx, &old.table, &def.schema_json)?;
        tx.execute(
            "INSERT INTO _schema_updates(publication_id,collection,gen,definition_json)
            VALUES(?1,?2,?3,?4) ON CONFLICT(publication_id,collection) DO NOTHING",
            params![
                lock.operation_id,
                def.name,
                integer(old.gen)?,
                serde_json::to_string(def).map_err(|e| invalid(e.to_string()))?
            ],
        )?;
        let saved: String = tx.query_row(
            "SELECT definition_json FROM _schema_updates
            WHERE publication_id=?1 AND collection=?2",
            params![lock.operation_id, def.name],
            |r| r.get(0),
        )?;
        let saved: CollectionDefinition =
            serde_json::from_str(&saved).map_err(|e| invalid(e.to_string()))?;
        if saved != *def {
            return Err(invalid("compatible preparation binding differs"));
        }
        Ok(old)
    })
}

fn ensure_index_build_safe(tx: &Transaction<'_>, table: &str, schema: &Value) -> Result<()> {
    let populated: bool = tx.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {} LIMIT 1)",
            identifier(table)?
        ),
        [],
        |r| r.get(0),
    )?;
    if !populated {
        return Ok(());
    }
    for fields in indexes(schema)? {
        let index_name = format!(
            "i_{table}_{}",
            fields
                .iter()
                .map(|f| encoded(f))
                .collect::<Vec<_>>()
                .join("_")
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name=?1)",
            [&index_name],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(invalid("online_index_build_required: populated generation must use the bounded index runner"));
        }
    }
    Ok(())
}

fn activate_compatible(tx: &Transaction<'_>, lock: &PublishLock) -> Result<()> {
    let mut statement = tx.prepare(
        "SELECT collection,gen,definition_json FROM _schema_updates
        WHERE publication_id=?1 ORDER BY collection",
    )?;
    let rows = statement
        .query_map([&lock.operation_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>>>()?;
    for (name, gen, json) in rows {
        let def: CollectionDefinition =
            serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?;
        let old = active_collection(tx, &name)?
            .ok_or_else(|| invalid("prepared collection no longer active"))?;
        if old.gen != gen
            || def.app_id != lock.app_id
            || def.app_ver != lock.target_app_ver
            || classify_change(&old.definition, &def)? != SchemaChange::Compatible
        {
            return Err(invalid("prepared schema base changed"));
        }
        create_indexes(tx, &old.table, &def.schema_json)?;
        tx.execute(
            "UPDATE _collections SET schema_ver=?3,schema_json=?4,class=?5,
            client_limit=?6,byte_budget=?7,rights_ver=?8 WHERE name=?1 AND gen=?2",
            params![
                name,
                integer(gen)?,
                def.schema_ver,
                def.schema_json.to_string(),
                def.class,
                integer(def.client_limit)?,
                integer(def.byte_budget)?,
                integer(def.rights_ver)?
            ],
        )?;
    }
    tx.execute(
        "DELETE FROM _schema_updates WHERE publication_id=?1",
        [&lock.operation_id],
    )?;
    Ok(())
}

fn check_base(tx: &Transaction<'_>, lock: &PublishLock) -> Result<()> {
    let current: Option<String> = tx
        .query_row(
            "SELECT app_ver FROM _schema_apps WHERE app_id=?1",
            [&lock.app_id],
            |r| r.get(0),
        )
        .optional()?;
    if current.as_deref().unwrap_or("") != lock.base_app_ver {
        return Err(invalid("publication base version changed"));
    }
    let mismatch:bool=tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM _collections WHERE app_id=?1 AND state='active' AND app_ver<>?2)",
        params![lock.app_id,lock.base_app_ver],|r|r.get(0))?;
    if mismatch {
        return Err(invalid("publication base version changed"));
    }
    Ok(())
}
pub fn reserve_publish(tx: &Transaction<'_>, lock: &PublishLock) -> Result<()> {
    if lock.app_id.is_empty()
        || lock.operation_id.is_empty()
        || lock.target_app_ver.is_empty()
        || lock.target_app_ver == lock.base_app_ver
    {
        return Err(invalid("invalid publication lock"));
    }
    atomic(tx, || {
        initialize(tx)?;
        check_base(tx, lock)?;
        tx.execute(
            "INSERT INTO publish_lock(app_id,base_app_ver,target_app_ver,operation_id)
            VALUES(?1,?2,?3,?4) ON CONFLICT(app_id) DO NOTHING",
            params![
                lock.app_id,
                lock.base_app_ver,
                lock.target_app_ver,
                lock.operation_id
            ],
        )?;
        verify_publish(tx, lock)
    })
}
fn held_publish(tx: &Transaction<'_>, lock: &PublishLock) -> Result<()> {
    let held: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM publish_lock WHERE app_id=?1
        AND base_app_ver=?2 AND target_app_ver=?3 AND operation_id=?4)",
        params![
            lock.app_id,
            lock.base_app_ver,
            lock.target_app_ver,
            lock.operation_id
        ],
        |r| r.get(0),
    )?;
    if !held {
        return Err(invalid("busy: publication reservation is not held"));
    }
    Ok(())
}
pub fn verify_publish(tx: &Transaction<'_>, lock: &PublishLock) -> Result<()> {
    held_publish(tx, lock)?;
    check_base(tx, lock)
}
pub fn migration(tx: &Transaction<'_>, publication: &str, name: &str) -> Result<Migration> {
    tx.query_row(
        "SELECT source_gen,gen,cursor,state,epoch,migrated_at_seq FROM _migrations
        WHERE publication_id=?1 AND collection=?2",
        params![publication, name],
        |r| {
            let cursor: String = r.get(2)?;
            let cursor = cursor
                .parse()
                .map_err(|_| invalid("invalid persisted migration cursor"))?;
            Ok(Migration {
                publication_id: publication.into(),
                collection: name.into(),
                source_gen: r.get(0)?,
                gen: r.get(1)?,
                cursor,
                state: r.get(3)?,
                epoch: r.get(4)?,
                migrated_at_seq: r.get(5)?,
            })
        },
    )
}
pub fn prepare_migration(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    def: &CollectionDefinition,
) -> Result<Migration> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        validate(def)?;
        if def.app_id != lock.app_id || def.app_ver != lock.target_app_ver {
            return Err(invalid("migration does not belong to publication"));
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE publication_id=?1 AND collection=?2)",
            params![lock.operation_id, def.name],
            |r| r.get(0),
        )?;
        if exists {
            let saved = migration(tx, &lock.operation_id, &def.name)?;
            let target = collection(tx, &def.name, saved.gen)?;
            if target.definition != *def || saved.state == "aborted" || saved.state == "committed" {
                return Err(invalid("migration binding differs"));
            }
            return Ok(saved);
        }
        reject_removed(tx, lock, &def.name)?;
        let staged:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _schema_updates WHERE publication_id=?1 AND collection=?2)",
            params![lock.operation_id,def.name],|r|r.get(0))?;
        if staged {
            return Err(invalid("collection already has a compatible preparation"));
        }
        let old = active_collection(tx, &def.name)?;
        if old
            .as_ref()
            .is_some_and(|old| old.definition.app_id != lock.app_id)
        {
            return Err(invalid("foreign collection"));
        }
        if let Some(old) = &old {
            if classify_change(&old.definition, def)? == SchemaChange::Compatible {
                return Err(invalid("compatible change must retain generation"));
            }
        }
        let previous: u64 = tx.query_row(
            "SELECT COALESCE(MAX(gen),0) FROM _collections WHERE name=?1",
            [&def.name],
            |r| r.get(0),
        )?;
        let gen = previous
            .checked_add(1)
            .ok_or_else(|| invalid("generation overflow"))?;
        create_generation(tx, def, gen, "preparing")?;
        let epoch = change_log::head(tx)?.epoch;
        let source_gen = old.map(|v| v.gen).unwrap_or(0);
        tx.execute(
            "INSERT INTO _migrations(publication_id,app_ver,collection,source_gen,gen,state,epoch)
            VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                lock.operation_id,
                def.app_ver,
                def.name,
                integer(source_gen)?,
                integer(gen)?,
                if source_gen == 0 {
                    "ready"
                } else {
                    "backfilling"
                },
                epoch
            ],
        )?;
        migration(tx, &lock.operation_id, &def.name)
    })
}

fn archive(
    tx: &Transaction<'_>,
    target: &Collection,
    doc: &change_log::ChangeDocument,
    reason: &str,
    now: i64,
) -> Result<()> {
    let Some(body) = &doc.body else { return Ok(()) };
    let expires = now
        .checked_add(change_log::RETENTION_SECONDS)
        .ok_or_else(|| invalid("archive clock overflow"))?;
    tx.execute("INSERT INTO _archive(app_id,collection,gen,id,rev,app_ver,data,reason,archived_at,expires_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![target.definition.app_id,target.definition.name,integer(doc.gen)?,doc.id,doc.rev,
            doc.app_ver,body.to_string(),reason,now,expires])?;
    Ok(())
}
fn write_output(
    tx: &Transaction<'_>,
    target: &Collection,
    source: &change_log::ChangeDocument,
    output: &MigrationOutput,
    now: i64,
) -> Result<()> {
    if source.op == Operation::Delete && !matches!(output, MigrationOutput::Delete) {
        return Err(invalid("source tombstone must remain a delete"));
    }
    let table = identifier(&target.table)?;
    match output {
        MigrationOutput::Upsert { rev, body } => {
            if rev.is_empty() || !body.is_object() {
                return Err(invalid("invalid migrated document"));
            }
            archive(tx, target, source, "generation migration", now)?;
            tx.execute(
                &format!(
                    "INSERT INTO {table}(id,rev,schema_ver,data,deleted)
                VALUES(?1,?2,?3,?4,0) ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,
                schema_ver=excluded.schema_ver,data=excluded.data,deleted=0"
                ),
                params![
                    source.id,
                    rev,
                    target.definition.schema_ver,
                    body.to_string()
                ],
            )?;
        }
        MigrationOutput::Delete | MigrationOutput::Archive { .. } => {
            if let MigrationOutput::Archive { reason } = output {
                if reason.is_empty() {
                    return Err(invalid("archive reason required"));
                }
            }
            tx.execute(
                &format!(
                    "INSERT INTO {table}(id,rev,schema_ver,data,deleted)
                VALUES(?1,?2,?3,NULL,1) ON CONFLICT(id) DO UPDATE SET
                rev=excluded.rev,schema_ver=excluded.schema_ver,data=NULL,deleted=1"
                ),
                params![source.id, source.rev, target.definition.schema_ver],
            )?;
            // This is schema-value archival, not an explicit document remove.
            // Preserve E4's old value after creating the target tombstone.
            if let MigrationOutput::Archive { reason } = output {
                archive(tx, target, source, reason, now)?;
            }
        }
    }
    Ok(())
}

/// Commit an externally transformed page. Revalidate its exact source rows/revisions
/// and complete cursor interval; stale or omitted input rolls the whole page back.
pub fn apply_migration_page(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    name: &str,
    expected_cursor: u64,
    page: &ChangePage,
    outputs: &[MigrationOutput],
    now: i64,
) -> Result<Migration> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        let saved = migration(tx, &lock.operation_id, name)?;
        if !matches!(saved.state.as_str(), "backfilling" | "ready")
            || saved.source_gen == 0
            || saved.cursor != expected_cursor
            || page.docs.len() > MIGRATION_BATCH
            || page.docs.len() != outputs.len()
            || page.cursor_to < expected_cursor
        {
            return Err(invalid("invalid migration page/cursor"));
        }
        let fresh = change_log::read_page(
            tx,
            name,
            saved.source_gen,
            expected_cursor,
            MIGRATION_BATCH + 1,
        )?;
        if fresh.epoch != saved.epoch
            || page.epoch != saved.epoch
            || page.cursor_to > fresh.head_seq
        {
            return Err(invalid("migration epoch/cursor is stale"));
        }
        let interval: Vec<_> = fresh
            .docs
            .into_iter()
            .filter(|d| d.seq <= page.cursor_to)
            .collect();
        if interval != page.docs || interval.len() > MIGRATION_BATCH {
            return Err(invalid("migration source revision/coverage changed"));
        }
        let target = collection(tx, name, saved.gen)?;
        for (source, output) in page.docs.iter().zip(outputs) {
            write_output(tx, &target, source, output, now)?;
        }
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _changes WHERE collection=?1 AND gen=?2 AND seq>?3)",
            params![name, integer(saved.source_gen)?, integer(page.cursor_to)?],
            |r| r.get(0),
        )?;
        let head = change_log::head(tx)?.head_seq;
        tx.execute(
            "UPDATE _migrations SET cursor=?3,state=?4,migrated_at_seq=?5
            WHERE publication_id=?1 AND collection=?2",
            params![
                lock.operation_id,
                name,
                page.cursor_to.to_string(),
                if pending { "backfilling" } else { "ready" },
                if pending { None } else { Some(integer(head)?) }
            ],
        )?;
        migration(tx, &lock.operation_id, name)
    })
}

/// Old-generation writes during migration must use this atomic dual-write seam.
/// The caller computes the target transform outside the writer from this exact source input.
pub fn dual_write(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    name: &str,
    source: &SourceWrite,
    output: &MigrationOutput,
    now: i64,
) -> Result<()> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        let saved = migration(tx, &lock.operation_id, name)?;
        if saved.source_gen == 0
            || !matches!(saved.state.as_str(), "backfilling" | "ready")
            || source.id.is_empty()
            || source.rev.is_empty()
            || !source.body.is_object()
        {
            return Err(invalid("invalid dual write"));
        }
        let old = collection(tx, name, saved.source_gen)?;
        let table = identifier(&old.table)?;
        let current: Option<String> = tx
            .query_row(
                &format!("SELECT rev FROM {table} WHERE id=?1"),
                [&source.id],
                |r| r.get(0),
            )
            .optional()?;
        if current != source.base_rev {
            return Err(invalid("source revision conflict"));
        }
        tx.execute(
            &format!(
                "INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,
            data=excluded.data,deleted=excluded.deleted"
            ),
            params![
                source.id,
                source.rev,
                old.definition.schema_ver,
                source.body.to_string(),
                source.deleted
            ],
        )?;
        let seq: u64 = tx.query_row(
            "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
            params![name, integer(saved.source_gen)?, source.id],
            |r| r.get(0),
        )?;
        let doc = change_log::ChangeDocument {
            id: source.id.clone(),
            seq,
            op: if source.deleted {
                Operation::Delete
            } else {
                Operation::Upsert
            },
            rev: source.rev.clone(),
            gen: saved.source_gen,
            schema_ver: old.definition.schema_ver,
            app_ver: old.definition.app_ver,
            body: if source.deleted {
                None
            } else {
                Some(source.body.clone())
            },
        };
        write_output(tx, &collection(tx, name, saved.gen)?, &doc, output, now)?;
        tx.execute("UPDATE _migrations SET state='backfilling',migrated_at_seq=NULL WHERE publication_id=?1 AND collection=?2",
            params![lock.operation_id,name])?;
        Ok(())
    })
}

/// Atomically switch every prepared collection only after source coverage and base revalidation.
/// The app-catalog owner must publish its version + watermark in this same transaction.
pub fn activate_publication(tx: &Transaction<'_>, lock: &PublishLock, now: i64) -> Result<u64> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        let mut statement = tx.prepare(
            "SELECT collection FROM _migrations WHERE publication_id=?1 ORDER BY collection",
        )?;
        let names = statement
            .query_map([&lock.operation_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>>>()?;
        for name in &names {
            let saved = migration(tx, &lock.operation_id, name)?;
            if saved.state != "ready" || saved.epoch != change_log::head(tx)?.epoch {
                return Err(invalid("migration is not ready"));
            }
            if saved.source_gen != 0 {
                let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _changes WHERE collection=?1 AND gen=?2 AND seq>?3)",
                    params![name,integer(saved.source_gen)?,integer(saved.cursor)?],|r|r.get(0))?;
                if pending {
                    return Err(invalid("source changed after migration readiness"));
                }
            }
        }
        activate_compatible(tx, lock)?;
        let removals = {
            let mut statement = tx.prepare(
                "SELECT collection,gen FROM _schema_removals WHERE publication_id=?1 ORDER BY collection")?;
            let rows = statement
                .query_map([&lock.operation_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
                })?
                .collect::<Result<Vec<_>>>()?;
            rows
        };
        for (name, gen) in removals {
            let active = active_collection(tx, &name)?
                .ok_or_else(|| invalid("removed collection no longer active"))?;
            if active.gen != gen {
                return Err(invalid("removal generation changed"));
            }
            commit_archive_collection(tx, lock, &name, now)?;
        }
        tx.execute(
            "DELETE FROM _schema_removals WHERE publication_id=?1",
            [&lock.operation_id],
        )?;
        let watermark = change_log::head(tx)?.head_seq;
        let retire = now
            .checked_add(GENERATION_RESERVE_SECONDS)
            .ok_or_else(|| invalid("generation reserve overflow"))?;
        for name in &names {
            let saved = migration(tx, &lock.operation_id, name)?;
            tx.execute(
                "UPDATE _collections SET state='archived',archived_at=?2,retire_after=?3
                WHERE name=?1 AND state='active'",
                params![name, now, retire],
            )?;
            tx.execute(
                "UPDATE _collections SET state='active',migrated_at_seq=?3
                WHERE name=?1 AND gen=?2 AND state='preparing'",
                params![name, integer(saved.gen)?, integer(watermark)?],
            )?;
            tx.execute(
                "UPDATE _migrations SET state='committed',migrated_at_seq=?3
                WHERE publication_id=?1 AND collection=?2",
                params![lock.operation_id, name, integer(watermark)?],
            )?;
        }
        tx.execute(
            "UPDATE _collections SET app_ver=?2 WHERE app_id=?1 AND state='active'",
            params![lock.app_id, lock.target_app_ver],
        )?;
        tx.execute("INSERT INTO _schema_apps VALUES(?1,?2) ON CONFLICT(app_id) DO UPDATE SET app_ver=excluded.app_ver",
            params![lock.app_id,lock.target_app_ver])?;
        // Unchanged collections' future change rows also identify the new active app version.
        let mut statement =
            tx.prepare("SELECT name,gen FROM _collections WHERE app_id=?1 AND state='active'")?;
        let active = statement
            .query_map([&lock.app_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })?
            .collect::<Result<Vec<_>>>()?;
        for (name, gen) in active {
            let value = collection(tx, &name, gen)?;
            change_log::install_triggers(
                tx,
                &TableBinding::generic(
                    &name,
                    gen,
                    &value.table,
                    value.definition.schema_ver,
                    &value.definition.app_ver,
                ),
            )?;
        }
        tx.execute(
            "DELETE FROM publish_lock WHERE app_id=?1 AND operation_id=?2",
            params![lock.app_id, lock.operation_id],
        )?;
        Ok(watermark)
    })
}

pub fn abort_publication(tx: &Transaction<'_>, lock: &PublishLock, now: i64) -> Result<()> {
    atomic(tx, || {
        held_publish(tx, lock)?;
        tx.execute(
            "DELETE FROM _schema_removals WHERE publication_id=?1",
            [&lock.operation_id],
        )?;
        let mut statement = tx.prepare(
            "SELECT collection,gen FROM _migrations WHERE publication_id=?1 AND state<>'committed'",
        )?;
        let rows = statement
            .query_map([&lock.operation_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })?
            .collect::<Result<Vec<_>>>()?;
        for (name, gen) in rows {
            let value = collection(tx, &name, gen)?;
            if value.state != "preparing" {
                return Err(invalid("only unpublished generation can be aborted"));
            }
            // Revoke its read binding now; retire potentially large tables through
            // purge_generation_page rather than blocking the writer on DROP.

            tx.execute(
                "DELETE FROM _change_sources WHERE collection=?1 AND gen=?2",
                params![name, integer(gen)?],
            )?;
            // Retain the registry generation number: never reuse an abandoned generation.
            tx.execute(
                "UPDATE _collections SET state='archived',archived_at=?3,retire_after=?3
                WHERE name=?1 AND gen=?2",
                params![name, integer(gen)?, now],
            )?;
        }
        tx.execute(
            "UPDATE _migrations SET state='aborted' WHERE publication_id=?1",
            [&lock.operation_id],
        )?;
        tx.execute(
            "DELETE FROM _schema_updates WHERE publication_id=?1",
            [&lock.operation_id],
        )?;
        tx.execute(
            "DELETE FROM publish_lock WHERE app_id=?1 AND operation_id=?2",
            params![lock.app_id, lock.operation_id],
        )?;
        Ok(())
    })
}

fn reject_removed(tx: &Transaction<'_>, lock: &PublishLock, name: &str) -> Result<()> {
    let pending: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM _schema_removals
        WHERE publication_id=?1 AND collection=?2)",
        params![lock.operation_id, name],
        |r| r.get(0),
    )?;
    if pending {
        return Err(invalid("collection is prepared for removal"));
    }
    Ok(())
}

/// Stage collection removal; active data remains visible until final publication.
/// The retention clock starts at activation, never at preparation.
pub fn archive_collection(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    name: &str,
    _now: i64,
) -> Result<()> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        let old =
            active_collection(tx, name)?.ok_or_else(|| invalid("active collection missing"))?;
        if old.definition.app_id != lock.app_id {
            return Err(invalid("foreign collection"));
        }
        let pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM _migrations WHERE collection=?1
            AND state IN ('preparing','backfilling','ready')) OR EXISTS(SELECT 1 FROM _schema_updates
            WHERE publication_id=?2 AND collection=?1)",params![name,lock.operation_id],|r|r.get(0))?;
        if pending {
            return Err(invalid("collection has pending schema preparation"));
        }
        tx.execute(
            "INSERT INTO _schema_removals VALUES(?1,?2,?3)
            ON CONFLICT(publication_id,collection) DO NOTHING",
            params![lock.operation_id, name, integer(old.gen)?],
        )?;
        let saved: u64 = tx.query_row(
            "SELECT gen FROM _schema_removals WHERE publication_id=?1 AND collection=?2",
            params![lock.operation_id, name],
            |r| r.get(0),
        )?;
        if saved != old.gen {
            return Err(invalid("removal generation changed"));
        }
        Ok(())
    })
}

/// Removing a collection is archival, never DROP. Existing data remains exportable for 30 days.
fn commit_archive_collection(
    tx: &Transaction<'_>,
    lock: &PublishLock,
    name: &str,
    now: i64,
) -> Result<()> {
    atomic(tx, || {
        verify_publish(tx, lock)?;
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _migrations WHERE collection=?1
            AND state IN ('preparing','backfilling','ready')) OR EXISTS(SELECT 1 FROM _schema_updates
            WHERE publication_id=?2 AND collection=?1)",params![name,lock.operation_id],|r|r.get(0))?;
        if pending {
            return Err(invalid("collection has pending schema preparation"));
        }
        let foreign: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1
            AND state='active' AND app_id<>?2)",
            params![name, lock.app_id],
            |r| r.get(0),
        )?;
        if foreign {
            return Err(invalid("foreign collection"));
        }

        let old =
            active_collection(tx, name)?.ok_or_else(|| invalid("active collection missing"))?;
        // Archive the generation itself, rather than copying an unbounded collection
        // on the writer. Export reads the retained physical table in bounded pages.
        let metadata = serde_json::json!({"table":old.table,"schema":old.definition.schema_json,
            "kind":"retained_collection_generation"});
        let expires = now
            .checked_add(change_log::RETENTION_SECONDS)
            .ok_or_else(|| invalid("archive clock overflow"))?;
        tx.execute("INSERT INTO _archive(app_id,collection,gen,id,rev,app_ver,data,reason,archived_at,expires_at)
            VALUES(?1,?2,?3,'','',?4,?5,'collection removed',?6,?7)",
            params![old.definition.app_id,name,integer(old.gen)?,old.definition.app_ver,metadata.to_string(),now,expires])?;
        tx.execute(
            "UPDATE _collections SET state='archived',archived_at=?3,retire_after=?4
            WHERE name=?1 AND gen=?2",
            params![
                name,
                integer(old.gen)?,
                now,
                now.checked_add(change_log::RETENTION_SECONDS)
                    .ok_or_else(|| invalid("archive clock overflow"))?
            ],
        )?;
        Ok(())
    })
}
pub fn purge_archive(tx: &Transaction<'_>, now: i64) -> Result<usize> {
    tx.execute("DELETE FROM _archive WHERE archive_id IN
        (SELECT archive_id FROM _archive WHERE expires_at<?1 ORDER BY expires_at,archive_id LIMIT 500)",[now])
}

/// Bounded retirement after the seven-day rollback or thirty-day archive reserve.
/// Keep the archived registry row so abandoned/retired generation ids are never reused.
pub fn purge_generation_page(
    tx: &Transaction<'_>,
    name: &str,
    gen: u64,
    now: i64,
    limit: usize,
) -> Result<bool> {
    if limit == 0 || limit > MIGRATION_BATCH {
        return Err(invalid("invalid retirement batch limit"));
    }
    atomic(tx, || {
        let value = collection(tx, name, gen)?;
        let retire: Option<i64> = tx.query_row(
            "SELECT retire_after FROM _collections WHERE name=?1 AND gen=?2",
            params![name, integer(gen)?],
            |r| r.get(0),
        )?;
        if value.state != "archived" || retire.is_none_or(|time| now <= time) {
            return Err(invalid("generation retention has not elapsed"));
        }
        let busy: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE collection=?1
            AND source_gen=?2 AND state IN ('preparing','backfilling','ready'))",
            params![name, integer(gen)?],
            |r| r.get(0),
        )?;
        if busy {
            return Err(invalid("generation still used by a migration"));
        }
        let table = identifier(&value.table)?;
        for suffix in ["insert", "update", "delete"] {
            tx.execute_batch(&format!(
                "DROP TRIGGER IF EXISTS {}",
                identifier(&format!("v4_{}_{suffix}", value.table))?
            ))?;
        }
        let removed = tx.execute(
            "DELETE FROM _change_sources WHERE collection=?1 AND gen=?2",
            params![name, integer(gen)?],
        )?;
        if removed != 0 {
            change_log::rotate_epoch(tx)?;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [&value.table],
            |r| r.get(0),
        )?;
        if exists {
            tx.execute(
                &format!(
                    "DELETE FROM {table} WHERE id IN (SELECT id FROM {table} ORDER BY id LIMIT ?1)"
                ),
                [limit as i64],
            )?;
        }
        tx.execute(
            "DELETE FROM _changes WHERE seq IN
            (SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 ORDER BY seq LIMIT ?3)",
            params![name, integer(gen)?, limit as i64],
        )?;
        let docs_left = if exists {
            tx.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} LIMIT 1)"),
                [],
                |r| r.get::<_, bool>(0),
            )?
        } else {
            false
        };
        let changes_left: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _changes WHERE collection=?1 AND gen=?2)",
            params![name, integer(gen)?],
            |r| r.get(0),
        )?;
        if !docs_left && !changes_left {
            if exists {
                tx.execute_batch(&format!("DROP TABLE {table}"))?;
            }
            return Ok(true);
        }
        Ok(false)
    })
}

/// Remove with revision CAS; scrub all retained generations and revision/archive payloads atomically.
/// The Store/app adapter supplies authorization and the new server revision.
pub fn remove_document(
    tx: &Transaction<'_>,
    name: &str,
    gen: u64,
    id: &str,
    base_rev: &str,
    new_rev: &str,
) -> Result<u64> {
    if new_rev.is_empty() || new_rev == base_rev {
        return Err(invalid("new delete revision required"));
    }
    atomic(tx, || {
        let current =
            active_collection(tx, name)?.ok_or_else(|| invalid("active collection missing"))?;
        if current.gen != gen {
            return Err(invalid("conflict: inactive generation"));
        }
        let table = identifier(&current.table)?;
        let changed = tx.execute(
            &format!(
                "UPDATE {table} SET rev=?3,deleted=1,data=NULL
            WHERE id=?1 AND rev=?2 AND deleted=0"
            ),
            params![id, base_rev, new_rev],
        )?;
        if changed != 1 {
            return Err(invalid("conflict: removed or changed revision"));
        }
        let mut statement =
            tx.prepare("SELECT table_name FROM _collections WHERE name=?1 AND gen<>?2")?;
        let tables = statement
            .query_map(params![name, integer(gen)?], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>>>()?;
        for old in tables {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
                [&old],
                |r| r.get(0),
            )?;
            if exists {
                tx.execute(
                    &format!(
                        "UPDATE {} SET data=NULL,deleted=1 WHERE id=?1",
                        identifier(&old)?
                    ),
                    [id],
                )?;
            }
        }
        tx.execute(
            "UPDATE _revs SET body=NULL WHERE collection=?1 AND id=?2",
            params![name, id],
        )?;
        tx.execute(
            "DELETE FROM _archive WHERE collection=?1 AND id=?2",
            params![name, id],
        )?;
        Ok(change_log::head(tx)?.head_seq)
    })
}

/// Filesystem loading happens before submitting the atomic import to the writer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseReleaseInput {
    pub ownership: Value,
    pub manifests: Vec<Value>,
    pub schemas: std::collections::BTreeMap<String, Value>,
}
#[derive(Debug, Clone)]
pub struct BaseReleaseDefaults {
    pub app_ver: String,
    pub client_limit: u64,
    pub byte_budget: u64,
    pub rights_ver: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct BaseReleaseReport {
    pub created: usize,
    pub existing: usize,
    pub source_ownership_count: usize,
    pub legacy_catalog_replaced: bool,
    pub missing_addenda_definitions: Vec<String>,
}
/// Read immutable release inputs outside the Store writer. No ambient paths/config.
pub fn load_base_release(
    v4_root: &std::path::Path,
    platform_manifest: &std::path::Path,
) -> Result<BaseReleaseInput> {
    fn json(path: &std::path::Path) -> Result<Value> {
        let data = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        serde_json::from_slice(&data).map_err(|e| invalid(format!("{}: {e}", path.display())))
    }
    let ownership = json(&v4_root.join("tools/manifest/out/ownership.json"))?;
    let mut files = std::fs::read_dir(v4_root.join("manifests"))
        .map_err(|e| invalid(e.to_string()))?
        .map(|e| e.map(|x| x.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| invalid(e.to_string()))?;
    files.sort();
    let mut manifests = Vec::new();
    for path in files {
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".manifest.json"))
        {
            manifests.push(json(&path)?);
        }
    }
    manifests.retain(|m| m.get("app_id").and_then(Value::as_str) != Some("ctox-platform"));
    manifests.push(json(platform_manifest)?);
    let mut schemas = std::collections::BTreeMap::new();
    for manifest in &manifests {
        let app = manifest
            .get("app_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("manifest app_id missing"))?;
        if app.contains('/') || app.contains('\\') || app == "." || app == ".." {
            return Err(invalid("invalid manifest app_id path"));
        }
        let path = v4_root
            .join("schemas")
            .join(format!("{app}.collections.schema.json"));
        if path.exists() {
            schemas.insert(app.to_owned(), json(&path)?);
        }
    }
    Ok(BaseReleaseInput {
        ownership,
        manifests,
        schemas,
    })
}
const OLG_COLLECTIONS: [&str; 5] = [
    "outbound_lead_generation_adapters",
    "sellify_companies",
    "sellify_people",
    "sellify_campaigns",
    "outbound_research_policies",
];

/// Atomic registry/table import only. Does not activate/install any app or read a deployed store.
/// Missing numeric manifest budgets require explicit caller policy, never guessed defaults.
pub fn import_base_release(
    tx: &Transaction<'_>,
    input: &BaseReleaseInput,
    defaults: &BaseReleaseDefaults,
) -> Result<BaseReleaseReport> {
    atomic(tx, || {
        initialize(tx)?;
        if defaults.app_ver.is_empty() {
            return Err(invalid("base release version required"));
        }
        let ownership = input
            .ownership
            .get("ownership")
            .unwrap_or(&input.ownership)
            .as_object()
            .ok_or_else(|| invalid("ownership map missing"))?;
        let mut apps = std::collections::BTreeMap::new();
        let mut definitions: std::collections::BTreeMap<String, Vec<(String, Value)>> =
            std::collections::BTreeMap::new();
        for manifest in &input.manifests {
            let app = manifest
                .get("app_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("manifest app_id missing"))?;
            if apps.insert(app, manifest).is_some() {
                return Err(invalid("duplicate app manifest"));
            }
            let collections = manifest
                .get("collections")
                .and_then(Value::as_object)
                .ok_or_else(|| invalid("manifest collections missing"))?;
            for (name, definition) in collections {
                definitions
                    .entry(name.clone())
                    .or_default()
                    .push((app.into(), definition.clone()));
            }
        }
        for name in ownership.keys() {
            if name != "business_module_catalog" && !definitions.contains_key(name) {
                return Err(invalid(format!("collection definition missing: {name}")));
            }
        }
        let missing_addenda_definitions = OLG_COLLECTIONS
            .iter()
            .filter(|n| !definitions.contains_key(**n))
            .map(|n| n.to_string())
            .collect();
        let mut report = BaseReleaseReport {
            created: 0,
            existing: 0,
            source_ownership_count: ownership.len(),
            legacy_catalog_replaced: ownership.contains_key("business_module_catalog"),
            missing_addenda_definitions,
        };
        if report.legacy_catalog_replaced
            && (!definitions.contains_key("business_apps")
                || !definitions.contains_key("business_app_versions"))
        {
            return Err(invalid("legacy catalog replacement schemas missing"));
        }
        for (name, candidates) in definitions {
            if name == "business_module_catalog" {
                continue;
            }
            let platform = candidates.iter().any(|(app, _)| app == "ctox-platform");
            let owner = if OLG_COLLECTIONS.contains(&name.as_str()) {
                "outbound-lead-generation".to_owned()
            } else if platform {
                "ctox-platform".to_owned()
            } else if let Some(owner) = ownership
                .get(&name)
                .and_then(|v| v.get("owner"))
                .and_then(Value::as_str)
            {
                owner.to_owned()
            } else if candidates.len() == 1 {
                candidates[0].0.clone()
            } else {
                return Err(invalid(format!("ambiguous collection owner: {name}")));
            };
            if !apps.contains_key(owner.as_str()) {
                return Err(invalid(format!("owner manifest missing: {owner}")));
            }
            let metadata = candidates
                .iter()
                .find(|(app, _)| app == &owner)
                .unwrap_or(&candidates[0])
                .1
                .clone();
            let corrected = input
                .schemas
                .get(&owner)
                .and_then(|v| v.get("collections"))
                .and_then(|v| v.get(&name));
            let schema = corrected
                .or_else(|| metadata.get("schema"))
                .ok_or_else(|| invalid(format!("schema missing: {name}")))?
                .clone();
            let version = schema.get("version").and_then(Value::as_u64).unwrap_or(0);
            let def = CollectionDefinition {
                name: name.clone(),
                app_id: owner,
                schema_ver: u32::try_from(version)
                    .map_err(|_| invalid("schema version overflow"))?,
                schema_json: schema,
                class: metadata
                    .get("class")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("collection class missing"))?
                    .into(),
                client_limit: metadata
                    .get("client_limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(defaults.client_limit),
                byte_budget: metadata
                    .get("byte_budget")
                    .and_then(Value::as_u64)
                    .unwrap_or(defaults.byte_budget),
                rights_ver: defaults.rights_ver,
                app_ver: defaults.app_ver.clone(),
            };
            if let Some(existing) = active_collection(tx, &name)? {
                if existing.definition != def {
                    return Err(invalid(format!("base registry differs: {name}")));
                }
                report.existing += 1;
            } else {
                create_collection(tx, &def)?;
                report.created += 1;
            }
        }
        Ok(report)
    })
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
