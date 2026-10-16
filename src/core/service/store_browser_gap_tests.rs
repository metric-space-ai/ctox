//! The production deadline callback against the real P3 writer/P8 ledger.
use super::*;
use ctox_store::{
    change_log::subscription::{
        protocol::{OrderedWrite, Seq, Write, WriteOp},
        WriteAuthority, WritePermission,
    },
    config::ConnectionConfig,
    read_pool::ReadPool,
    registry::Registry,
    schema::{self, CollectionDefinition},
    writer::WriterOptions,
    DbId,
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
struct NoEffects;
impl WriteAuthority for NoEffects {
    fn authorize(
        &self,
        _: &rusqlite::Transaction<'_>,
        participant: &str,
        request: &Write,
    ) -> rusqlite::Result<WritePermission> {
        // P8 now authorizes before buffering too. Admit only this fixture's
        // inbox entry; final document validation still forbids every effect.
        Ok(
            if participant == "device" && request.collection == "notes" && request.id == "gap" {
                WritePermission::Permit
            } else {
                WritePermission::Reject("forbidden".into())
            },
        )
    }
    fn validate_document(
        &self,
        _: &rusqlite::Transaction<'_>,
        _: &str,
        _: &schema::Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Reject("forbidden".into()))
    }
}
fn clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gap_cancellation_cannot_discard_a_committed_receipt() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let db = DbId::new(root.path().join("gap.sqlite"))?;
    let writer=Registry.open(db.clone(),WriterOptions{
        configure:Some(Arc::new(|c|{
            ConnectionConfig::default().apply_writer(c)?;
            let tx=c.unchecked_transaction()?;
            schema::create_collection(&tx,&CollectionDefinition{
                name:"notes".into(),app_id:"notes-app".into(),schema_ver:1,
                schema_json:json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),
                class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"1".into(),
            })?;tx.commit()?;Ok(())
        })),
        head_seq:Some(Arc::new(|c| c.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get(0)).map_err(Into::into))),
    })?;
    writer.ready().await?;
    let readers = ReadPool::for_database(
        &db,
        writer.generation(),
        Default::default(),
        Default::default(),
    )
    .await?;
    let ordered = OrderedWriter::new(writer.clone(), Arc::new(NoEffects));
    let now = clock();
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&now.to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    let batch = ordered
        .submit(
            "device".into(),
            OrderedWrite {
                client_seq: Seq(2),
                write: Write {
                    op_id: uuid::Uuid::from_bytes(bytes).to_string(),
                    prev_op_id: None,
                    collection: "notes".into(),
                    id: "gap".into(),
                    base_rev: None,
                    expected_revision: None,
                    base_seq: None,
                    gen: 1,
                    schema_ver: 1,
                    app_ver: "1".into(),
                    op: WriteOp::Put,
                    body: Some(json!({"id":"gap"})),
                },
            },
            now.saturating_sub(ctox_store::change_log::subscription::write_contract::GAP_MS + 10),
        )
        .await?;
    assert!(batch.results.is_empty());
    let (sender, mut results) = tokio::sync::mpsc::channel(1);
    // A real earlier batch fills the actual delivery channel while expiry commits.
    sender.send(batch.clone()).await?;
    let delivery: Delivery = Arc::new(move |batch| {
        let sender = sender.clone();
        Box::pin(async move {
            let _ = sender.send(batch).await;
        })
    });
    let gap = Arc::new(GapTimer::default());
    let mut commits = writer.subscribe_commits();
    commits.latest();
    let expires = Instant::now() + Duration::from_secs(60);
    gap.arm(
        ordered.clone(),
        "device".into(),
        expires,
        delivery.clone(),
        batch.next_deadline_ms,
        clock,
    )?;
    tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            commits.changed().await?;
            if readers.read(|tx|tx.query_row("SELECT EXISTS(SELECT 1 FROM _write_results WHERE participant='device' AND client_seq=2)",[],|r|r.get::<_,bool>(0))).await? {break;}
        }
        Ok::<_,anyhow::Error>(())
    }).await??;
    gap.arm(ordered, "device".into(), expires, delivery, None, clock)?;
    assert!(results.recv().await.unwrap().results.is_empty());
    let result = tokio::time::timeout(Duration::from_secs(2), results.recv())
        .await
        .ok()
        .flatten()
        .expect("committed gap receipt was discarded");
    assert_eq!(result.results[0].client_seq, Seq(2));
    assert_eq!(result.results[0].result.reason.as_deref(), Some("expired"));
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}
