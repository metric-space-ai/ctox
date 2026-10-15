use super::*;
use crate::service::store_runtime::DaemonStores;
use ctox_store::{DbId, WriteJob};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_queue_ack_under_load_commits_intent_and_uses_current_target_authority(
) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "fixture refuses a configured state root"
    );
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().to_path_buf();
    let mut stores = DaemonStores::start(endpoint(&root), Handle::current())?;
    let core=stores.register("core",DbId::new(crate::paths::core_db(&root))?,false,Arc::new(|tx| {
        crate::channels::ensure_owned_schema(tx).map_err(|_|StoreError::Invalid("fixture Core DDL".into()))?;
        assert_eq!(tx.pragma_query_value(None,"busy_timeout",|r|r.get::<_,i64>(0))?,0,"domain DDL replaced owner pragmas");
        for n in 0..130 {
            let id=format!("queue-owner-fixture-{n:03}");
            crate::communication_store::upsert_communication_message_tx(tx,crate::communication_store::UpsertMessage {
                message_key:&id,channel:"queue",account_key:"synthetic",thread_key:"synthetic",remote_id:&id,direction:"inbound",folder_hint:"queue",
                sender_display:"fixture",sender_address:"fixture",recipient_addresses_json:"[]",cc_addresses_json:"[]",bcc_addresses_json:"[]",subject:"synthetic owner job",preview:"fixture",body_text:"synthetic load",body_html:"",raw_payload_ref:"",trust_level:"high",status:"received",seen:false,has_attachments:false,
                external_created_at:"2026-10-15T00:00:00Z",observed_at:"2026-10-15T00:00:00Z",metadata_json:r#"{"source":"ctox-queue","priority":"normal"}"#
            }).map_err(|_|StoreError::Invalid("fixture message seed".into()))?;
            tx.execute("INSERT INTO communication_routing_state(message_key,route_status,lease_owner,updated_at) VALUES(?1,'leased','fixture','2026-10-15T00:00:00Z')",[id])?;
        }
        Ok(())
    })).await?;
    let sync=stores.register("sync",DbId::new(crate::paths::sync_db(&root))?,true,Arc::new(|tx| {
        ctox_store::schema::create_collection(tx,&ctox_store::schema::CollectionDefinition {
            name:"ctox_queue_tasks".into(),app_id:"ctox-platform".into(),schema_ver:0,
            schema_json:json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"},"status":{"type":"string"}}}),
            class:"projection".into(),client_limit:1000,byte_budget:1048576,rights_ver:1,app_ver:"fixture-1".into()
        })?;Ok(())
    })).await?;
    stores
        .install_queue(root.clone(), core.clone(), sync.clone())
        .await?;
    let mut changes = sync.writer.subscribe_commits();
    let mut workers = Vec::new();
    for worker in 0..8 {
        let root = root.clone();
        workers.push(tokio::spawn(async move {
            let mut elapsed = Vec::new();
            for offset in 0..16 {
                let n = worker * 16 + offset;
                let request = AckRequest {
                    operation_id: format!("queue-load-{n}"),
                    keys: vec![format!("queue-owner-fixture-{n:03}")],
                    status: "failed".into(),
                    failure: Some("synthetic bounded failure".into()),
                    reason: None,
                    attempt: None,
                };
                let start = Instant::now();
                let result = ack_async(root.clone(), request, None).await?;
                anyhow::ensure!(result == 1, "queue acknowledgement missing");
                elapsed.push(start.elapsed().as_micros() as u64);
            }
            Ok::<_, anyhow::Error>(elapsed)
        }));
    }
    let mut elapsed = Vec::new();
    for worker in workers {
        elapsed.extend(worker.await??);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let present = sync
            .readers
            .read(|tx| {
                tx.query_row("SELECT COUNT(*) FROM c_ctox_queue_tasks", [], |r| {
                    r.get::<_, u64>(0)
                })
            })
            .await?;
        if present == 128 {
            break;
        }
        tokio::time::timeout_at(deadline, changes.changed()).await??;
    }
    let receipt = core
        .readers
        .read(|tx| {
            let acked = tx.query_row(
                "SELECT COUNT(*) FROM communication_routing_state WHERE route_status='failed'",
                [],
                |r| r.get::<_, u64>(0),
            )?;
            let intents = tx.query_row(
                "SELECT COUNT(*) FROM _publication_outbox WHERE target='sync.queue'",
                [],
                |r| r.get::<_, u64>(0),
            )?;
            let receipts = tx.query_row("SELECT COUNT(*) FROM _queue_ack_ops", [], |r| {
                r.get::<_, u64>(0)
            })?;
            Ok((acked, intents, receipts))
        })
        .await?;
    assert_eq!(receipt, (128, 128, 128), "ACK, intent and receipt diverged");
    let replay = AckRequest {
        operation_id: "queue-load-0".into(),
        keys: vec!["queue-owner-fixture-000".into()],
        status: "failed".into(),
        failure: Some("synthetic bounded failure".into()),
        reason: None,
        attempt: None,
    };
    assert_eq!(ack_async(root.clone(), replay.clone(), None).await?, 1);
    let mut conflict = replay;
    conflict.failure = Some("different operation".into());
    assert!(ack_async(root.clone(), conflict, None).await.is_err());
    // The destination owner changes after source admission was configured.
    // Resolving it again in B's transaction must reject the pending effect.
    sync.writer
        .submit(WriteJob::new(options(), |tx| {
            tx.execute(
                "UPDATE _collections SET app_id='other-app' WHERE name='ctox_queue_tasks'",
                [],
            )?;
            Ok(())
        }))?
        .await?;
    let mut source_commits = core.writer.subscribe_commits();
    let denied = AckRequest {
        operation_id: "queue-current-owner".into(),
        keys: vec!["queue-owner-fixture-128".into()],
        status: "failed".into(),
        failure: Some("synthetic authority change".into()),
        reason: None,
        attempt: None,
    };
    ack_async(root.clone(), denied, None).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rejected=core.readers.read(|tx|tx.query_row("SELECT EXISTS(SELECT 1 FROM _publication_outbox WHERE op_id='queue-current-owner:queue-owner-fixture-128' AND state='dead_letter' AND error_code='forbidden')",[],|r|r.get::<_,bool>(0))).await?;
        if rejected {
            break;
        }
        tokio::time::timeout_at(deadline, source_commits.changed()).await??;
    }
    assert!(!sync.readers.read(|tx|tx.query_row("SELECT EXISTS(SELECT 1 FROM c_ctox_queue_tasks WHERE id='queue-owner-fixture-128')",[],|r|r.get::<_,bool>(0))).await?);
    elapsed.sort_unstable();
    eprintln!(
        "QUEUE_OWNER_MEASURED {}",
        json!({"parallel_producers":8,"ack_p99_us":elapsed[(elapsed.len()*99/100).min(elapsed.len()-1)],"ack_errors":[],"lock_busy":0,"durable_source_intents":receipt.1})
    );
    stores.shutdown().await?;
    Ok(())
}
