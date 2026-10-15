//! Domain persistence seam. V4 mutations publish through the owning writer;
//! the legacy SQL builder remains isolated until its synchronous callers move.
use ctox_store::{
    publication::{self, CurrentAuthority, Delivery, Publication, Receiver, RetryPolicy, Stage},
    writer::WriterHandle,
    JobOptions, StoreError,
};
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection, Transaction};
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

/// A persisted authority reference, never a captured policy decision. The
/// receiver resolves it again on the destination writer at effect time.
#[derive(Clone)]
pub struct Projection {
    pub operation_id: String,
    pub subject: String,
    pub collection: String,
    pub id: String,
    pub document: Value,
}

/// Caller retains the already-owned handles. This adapter never opens a file,
/// never acquires a second owner and never waits from inside a transaction.
pub struct BusinessStoreAdapter {
    source: WriterHandle,
    destination: WriterHandle,
    destination_name: String,
    receiver: Receiver,
}
impl BusinessStoreAdapter {
    pub fn new(
        source: WriterHandle,
        destination: WriterHandle,
        destination_name: String,
        authority: Arc<dyn CurrentAuthority>,
        apply: impl Fn(&Transaction<'_>, &str, &str, Value) -> Result<(), StoreError>
            + Send
            + Sync
            + 'static,
    ) -> Result<Self, StoreError> {
        if source.db_id() == destination.db_id() || destination_name.is_empty() {
            return Err(StoreError::Invalid(
                "projection requires distinct owned stores".into(),
            ));
        }
        let receiver = Receiver {
            authority,
            apply: Arc::new(move |tx, publication| {
                if publication.kind != "business.projection.v4" {
                    return Err(StoreError::Invalid("foreign publication kind".into()));
                }
                let body: Value = serde_json::from_slice(&publication.payload)
                    .map_err(|_| StoreError::Invalid("invalid projection envelope".into()))?;
                let text = |field| {
                    body.get(field)
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| StoreError::Invalid("incomplete projection envelope".into()))
                };
                let collection = text("collection")?;
                let id = text("id")?;
                let document = body
                    .get("document")
                    .filter(|d| d.is_object())
                    .ok_or_else(|| StoreError::Invalid("projection is not a document".into()))?;
                if document.get("id").and_then(Value::as_str) != Some(id) {
                    return Err(StoreError::Invalid("projection identity mismatch".into()));
                }
                // Schema, generation and native revision belong to the target
                // sink. No caller timestamp is used as a replication cursor.
                apply(tx, collection, id, document.clone())
            }),
        };
        Ok(Self {
            source,
            destination,
            destination_name,
            receiver,
        })
    }

    /// Domain mutation + intent commit in A. Return means locally durable,
    /// not applied in B. Schema installation is the daemon's startup job.
    pub async fn publish(
        &self,
        projection: Projection,
        options: JobOptions,
        now_ms: i64,
        mutate: impl FnOnce(&Transaction<'_>) -> Result<(), StoreError> + Send + 'static,
    ) -> Result<Stage, StoreError> {
        if projection.collection.is_empty()
            || projection.id.is_empty()
            || !projection.document.is_object()
            || projection.document.get("id").and_then(Value::as_str) != Some(projection.id.as_str())
        {
            return Err(StoreError::Invalid("invalid projection identity".into()));
        }
        let payload = serde_json::to_vec(&serde_json::json!({
            "collection": projection.collection, "id": projection.id,
            "document": projection.document,
        }))
        .map_err(|_| StoreError::Invalid("invalid projection encoding".into()))?;
        publication::stage(
            &self.source,
            options,
            Publication {
                op_id: projection.operation_id,
                target: self.destination_name.clone(),
                kind: "business.projection.v4".into(),
                subject: projection.subject,
                payload,
            },
            now_ms,
            mutate,
        )
        .await
    }

    /// Drive after A commits, including startup recovery and persisted retries.
    /// The P3-07 pair releases A before waiting for B, and B before finishing A.
    pub async fn deliver(&self, options: JobOptions, now_ms: i64) -> Result<Delivery, StoreError> {
        publication::deliver_one(
            &self.source,
            &self.destination,
            self.destination_name.clone(),
            options,
            Receiver {
                authority: self.receiver.authority.clone(),
                apply: self.receiver.apply.clone(),
            },
            now_ms,
            RetryPolicy::default(),
        )
        .await
    }
}

/// Compatibility boundary for callers not yet on the daemon writer. All legacy
/// attached writes use this builder; V4 sinks use their target transaction and
/// schema/change-log triggers instead. `write_time` is assigned by the host,
/// never taken from the document's updated_at_ms.
pub(super) fn write_legacy_projection(
    conn: &Connection,
    qualified_table: &str,
    table_columns: &HashSet<String>,
    id: &str,
    payload: &Value,
    revision: String,
    write_time: f64,
    guard: Option<String>,
) -> anyhow::Result<()> {
    let mut columns = vec!["id".to_string(), "data".to_string()];
    let mut values = vec![
        SqlValue::Text(id.into()),
        SqlValue::Text(serde_json::to_string(payload)?),
    ];
    let mut updates = vec!["data = excluded.data".to_string()];
    for column in ["deleted", "_deleted"] {
        if table_columns.contains(column) {
            columns.push(column.into());
            values.push(SqlValue::Integer(0));
            updates.push(format!("{column} = excluded.{column}"));
            break;
        }
    }
    for column in ["revision", "_rev"] {
        if table_columns.contains(column) {
            columns.push(column.into());
            values.push(SqlValue::Text(revision));
            updates.push(format!("{column} = excluded.{column}"));
            break;
        }
    }
    if table_columns.contains("lastWriteTime") {
        columns.push("lastWriteTime".into());
        values.push(SqlValue::Real(write_time));
        updates.push(format!(
            "lastWriteTime = MAX(lastWriteTime + 1, excluded.lastWriteTime)"
        ));
    }
    let placeholders = (1..=columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    conn.execute(
        &format!(
            "INSERT INTO {qualified_table} ({}) VALUES ({placeholders})
        ON CONFLICT(id) DO UPDATE SET {}{}",
            columns.join(","),
            updates.join(","),
            guard.map(|s| format!(" WHERE {s}")).unwrap_or_default()
        ),
        params_from_iter(values),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_store::publication::{AuthorityError, AuthorityPermit, Rejection};
    use ctox_store::{registry::Registry, writer::WriterOptions, DbId, WriteJob};
    struct Policy;
    impl CurrentAuthority for Policy {
        fn acquire(
            &self,
            tx: &Transaction<'_>,
            _: &Publication,
        ) -> Result<AuthorityPermit, AuthorityError> {
            if tx
                .query_row("SELECT allowed FROM policy", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
            {
                Ok(AuthorityPermit::holding(()))
            } else {
                Err(AuthorityError::Rejected(Rejection::Revoked))
            }
        }
    }
    async fn owner(path: &std::path::Path) -> WriterHandle {
        let w = Registry
            .open(DbId::new(path).unwrap(), WriterOptions::default())
            .unwrap();
        w.ready().await.unwrap();
        w.submit(WriteJob::new(JobOptions::default(), |tx| {
            publication::install(tx)?;
            tx.execute_batch("CREATE TABLE domain(id TEXT PRIMARY KEY); CREATE TABLE policy(allowed INTEGER); INSERT INTO policy VALUES(1)")?;
            Ok(())
        })).unwrap().await.unwrap();
        w
    }
    #[tokio::test]
    async fn projection_source_commit_and_effect_time_revocation() {
        let dir = tempfile::tempdir().unwrap();
        let a = owner(&dir.path().join("core")).await;
        let b = owner(&dir.path().join("sync")).await;
        let adapter = BusinessStoreAdapter::new(
            a.clone(),
            b.clone(),
            "sync".into(),
            Arc::new(Policy),
            |tx, _, id, _| {
                tx.execute("INSERT INTO domain VALUES(?)", [id])?;
                Ok(())
            },
        )
        .unwrap();
        adapter
            .publish(
                Projection {
                    operation_id: "queue-lease-1".into(),
                    subject: "native-queue".into(),
                    collection: "ctox_queue_tasks".into(),
                    id: "task".into(),
                    document: serde_json::json!({"id":"task"}),
                },
                JobOptions::default(),
                100,
                |tx| {
                    tx.execute("INSERT INTO domain VALUES('task')", [])?;
                    Ok(())
                },
            )
            .await
            .unwrap();
        let exists = a
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                Ok(tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM domain WHERE id='task')",
                    [],
                    |r| r.get::<_, bool>(0),
                )?)
            }))
            .unwrap()
            .await
            .unwrap();
        assert!(
            exists,
            "the source mutation must commit with the publication"
        );
        b.submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute("UPDATE policy SET allowed=0", [])?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
        assert_eq!(
            adapter.deliver(JobOptions::default(), 101).await.unwrap(),
            Delivery::DeadLetter(Rejection::Revoked)
        );
        let empty = b
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                Ok(
                    !tx.query_row("SELECT EXISTS(SELECT 1 FROM domain)", [], |r| {
                        r.get::<_, bool>(0)
                    })?,
                )
            }))
            .unwrap()
            .await
            .unwrap();
        assert!(empty, "revoked projection must not produce an effect");
        a.shutdown().await.unwrap();
        b.shutdown().await.unwrap();
    }
    #[test]
    fn legacy_projection_write_time_cannot_go_backwards() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projection(id TEXT PRIMARY KEY,data TEXT,lastWriteTime REAL)",
        )
        .unwrap();
        let columns = HashSet::from(["lastWriteTime".into()]);
        for time in [1000.0, 1.0] {
            write_legacy_projection(
                &conn,
                "projection",
                &columns,
                "task",
                &serde_json::json!({"id":"task","updated_at_ms":-999}),
                "1-native".into(),
                time,
                None,
            )
            .unwrap();
        }
        let time: f64 = conn
            .query_row("SELECT lastWriteTime FROM projection", [], |r| r.get(0))
            .unwrap();
        assert_eq!(time, 1001.0);
    }
}
