#[path = "store_browser_process_fixture.rs"]
mod process_fixture;
use super::*;
use crate::business_os::launch_context_v4::{build, tests::fixture};
use crate::service::store_startup::isolate_test_roots;
use rxdb::plugins::replication_webrtc::index_mod::hello::HelloFrame;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_ingress_reads_the_real_owner_and_fences_revoked_pages() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut foreground, user) = fixture().await?;
    foreground.inputs.sync.writer.submit(ctox_store::WriteJob::new(Default::default(),|tx|{
        let table:String=tx.query_row("SELECT table_name FROM _collections WHERE name='notes'",[],|r|r.get(0))?;
        tx.execute(&format!("INSERT INTO \"{}\"(id,rev,schema_ver,data,deleted) VALUES('real-owner-note','1',1,?1,0)",table),[r#"{"id":"real-owner-note","title":"held writer"}"#])?;Ok(())
    }))?.await?;
    let context = build(&foreground.inputs, &user).await?;
    let HelloFrame::Hello(hello) = context.v4.hello.offer.clone() else {
        panic!("hello required")
    };
    let peer = format!("{}@1", context.v4.signaling.peer_id);
    let admission = foreground.inputs.browser_admission()?;
    (admission.authenticator())(peer.clone(), hello)
        .await
        .map_err(|_| anyhow::anyhow!("ticket rejected"))?;
    let source = Source {
        inputs: Arc::downgrade(&foreground.inputs),
        admission: admission.clone(),
    };
    let binding = admission.peer(&peer)?;
    let request = serde_json::json!({"type":"subscribe","collection":"notes","scope_hash":binding.scopes["notes"],"gen":1,"checkpoint":null,"priority":"P1"});
    let mut stream = source
        .subscribe(&peer, request)
        .await
        .map_err(|_| anyhow::anyhow!("subscription rejected"))?;
    let page = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("subscription ended"))?
        .map_err(|_| anyhow::anyhow!("owner read failed"))?;
    let packet: ctox_store::change_log::subscription::protocol::StreamPacket =
        serde_json::from_value(page.clone())?;
    packet
        .verify(&*foreground.inputs.instance_verifier()?)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(packet.participant, "alice");
    assert!(
        packet.docs.iter().any(|d| d.id == "real-owner-note"
            && d.body.as_ref().is_some_and(|d| d["title"] == "held writer")),
        "real writer document missing from signed cursor"
    );
    let guard = source
        .publication(&peer, &page)
        .map_err(|_| anyhow::anyhow!("publication rejected"))?;
    guard
        .with_current(&mut || Ok(()))
        .map_err(|_| anyhow::anyhow!("current guard rejected"))?;
    admission
        .authority()
        .replace(vec![], vec![])
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        guard.with_current(&mut || Ok(())).is_err(),
        "prepared page crossed rights revocation"
    );
    drop(stream);
    foreground.shutdown().await?;
    Ok(())
}

fn request(seq: u64, id: &str, body: Value) -> Value {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&now().to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[14..].copy_from_slice(&(seq as u16).to_be_bytes());
    serde_json::json!({"type":"write","client_seq":seq.to_string(),"op_id":uuid::Uuid::from_bytes(bytes).to_string(),"collection":"notes","id":id,"base_rev":null,"base_seq":null,"gen":1,"schema_ver":1,"app_ver":"1","op":"put","body":body})
}
async fn write_fixture() -> anyhow::Result<(
    tempfile::TempDir,
    crate::service::store_startup::Foreground,
    Source,
    String,
)> {
    let (root, fg, user) = fixture().await?;
    let authority = fg.inputs.browser_admission()?.authority();
    let collections = [
        "business_users",
        "business_apps",
        "business_app_versions",
        "business_commands",
        "notes",
        "foreign_rows",
    ]
    .map(str::to_owned);
    let manifest = sync_rights::manifest_rules::ManifestRules::from_manifest(
        "notes-app",
        "1",
        ["notes".to_owned()].into(),
        &serde_json::json!({
            "row_filters":{"notes":{"op":"eq","path":"/owner","value":{"kind":"actor_id"}}},
            "write_guard":{"notes":{"command_only_fields":["locked"]}},
            "foreign_collections":{"business_commands":{"owner_app":"ctox-platform","ops":["read","write"]}},
            "command_rights":{"notes.echo":{"roles":["founder"]}}
        }),
    )
    .map_err(anyhow::Error::msg)?;
    let platform = sync_rights::manifest_rules::ManifestRules::from_manifest(
        "ctox-platform",
        "1",
        ["business_commands".to_owned()].into(),
        &serde_json::json!({"exports":{"business_commands":["read","write"]}}),
    )
    .map_err(anyhow::Error::msg)?;
    // The canonical fixture user is founder, whose imported defaults are
    // read-only. Grant this synthetic actor's role only these test mutations
    // through the real Store authority; never widen production defaults.
    let mut grants = crate::business_os::policy::base_release_rights(&collections);
    for grant in &mut grants {
        if grant.role == "founder"
            && matches!(grant.collection.as_str(), "notes" | "business_commands")
        {
            grant.ops.extend([
                sync_rights::Operation::Write,
                sync_rights::Operation::Delete,
            ]);
        }
    }
    authority
        .replace(grants, vec![manifest, platform])
        .await
        .map_err(anyhow::Error::msg)?;
    let context = build(&fg.inputs, &user).await?;
    let HelloFrame::Hello(hello) = context.v4.hello.offer else {
        panic!("hello required")
    };
    let peer = format!("{}@1", context.v4.signaling.peer_id);
    let admission = fg.inputs.browser_admission()?;
    (admission.authenticator())(peer.clone(), hello)
        .await
        .map_err(|_| anyhow::anyhow!("ticket rejected"))?;
    let source = Source {
        inputs: Arc::downgrade(&fg.inputs),
        admission,
    };
    Ok((root, fg, source, peer))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_write_receipt_requires_real_commit_and_replay_uses_same_revision(
) -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, source, peer) = write_fixture().await?;
    let mut results = source
        .write_results(&peer)
        .map_err(|_| anyhow::anyhow!("results"))?;
    let write = request(
        1,
        "committed",
        serde_json::json!({"id":"committed","owner":"alice","title":"durable"}),
    );
    source
        .write(&peer, write.clone())
        .await
        .map_err(|_| anyhow::anyhow!("write failed"))?;
    let receipt = tokio::time::timeout(Duration::from_secs(2), results.next())
        .await?
        .unwrap()
        .map_err(|_| anyhow::anyhow!("receipt failed"))?;
    assert_eq!(
        receipt["status"], "accepted",
        "real browser write rejected: {receipt}"
    );
    let (document, ops): (String, u64) = fg
        .inputs
        .sync
        .readers
        .read(|tx| {
            let table: String = tx.query_row(
                "SELECT table_name FROM _collections WHERE name='notes' AND state='active'",
                [],
                |r| r.get(0),
            )?;
            Ok((
                tx.query_row(
                    &format!("SELECT data FROM \"{table}\" WHERE id='committed'"),
                    [],
                    |r| r.get(0),
                )?,
                tx.query_row("SELECT COUNT(*) FROM _ops WHERE id='committed'", [], |r| {
                    r.get(0)
                })?,
            ))
        })
        .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&document)?["title"],
        "durable"
    );
    assert_eq!(ops, 1, "receipt preceded committed dedup ledger");
    source
        .write(&peer, write)
        .await
        .map_err(|_| anyhow::anyhow!("replay failed"))?;
    let replay = tokio::time::timeout(Duration::from_secs(2), results.next())
        .await?
        .unwrap()
        .map_err(|_| anyhow::anyhow!("replay receipt"))?;
    assert_eq!(replay, receipt, "replay minted another result");
    let mut command = request(
        2,
        "queued-command",
        serde_json::json!({"id":"queued-command","app_id":"notes-app","type":"notes.echo","actor_id":"alice","created_at":chrono::Utc::now().to_rfc3339(),"status":"queued","payload":{}}),
    );
    command["collection"] = serde_json::json!("business_commands");
    let command_op = command["op_id"].clone();
    source
        .write(&peer, command)
        .await
        .map_err(|_| anyhow::anyhow!("command intent failed"))?;
    let command_result = tokio::time::timeout(Duration::from_secs(2), results.next())
        .await?
        .unwrap()
        .map_err(|_| anyhow::anyhow!("command receipt"))?;
    assert_eq!(command_result["op_id"], command_op);
    assert_eq!(
        command_result["status"], "accepted",
        "command did not pass current dispatch policy: {command_result}"
    );
    let projection: String = fg
        .inputs
        .sync
        .readers
        .read(|tx| {
            tx.query_row(
                "SELECT status FROM _command_projections WHERE command_id='queued-command'",
                [],
                |r| r.get(0),
            )
        })
        .await?;
    assert_eq!(
        projection, "queued",
        "receipt preceded committed command intent"
    );
    fg.inputs.sync.writer.submit(ctox_store::WriteJob::new(Default::default(),|tx| {
        let table:String=tx.query_row("SELECT table_name FROM _collections WHERE name='notes'",[],|r|r.get(0))?;
        // Deferred FK fails at COMMIT, after P8 has constructed its real result.
        tx.execute_batch(&format!("CREATE TABLE c_c10_parent(id INTEGER PRIMARY KEY); CREATE TABLE c_c10_child(id INTEGER REFERENCES c_c10_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER c10_abort AFTER INSERT ON \"{table}\" BEGIN INSERT INTO c_c10_child VALUES(1); END"))?;Ok(())
    }))?.await?;
    let failed = request(
        3,
        "rolled-back",
        serde_json::json!({"id":"rolled-back","owner":"alice","title":"never committed"}),
    );
    assert!(
        source.write(&peer, failed).await.is_err(),
        "failed transaction was accepted"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), results.next())
            .await
            .is_err(),
        "receipt escaped before commit"
    );
    fg.shutdown().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_write_checks_actual_before_after_inside_the_owner_job() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    let (_root, mut fg, source, peer) = write_fixture().await?;
    let mut results = source
        .write_results(&peer)
        .map_err(|_| anyhow::anyhow!("results"))?;
    let seed = request(
        1,
        "ownership",
        serde_json::json!({"id":"ownership","owner":"alice","title":"original"}),
    );
    source
        .write(&peer, seed)
        .await
        .map_err(|_| anyhow::anyhow!("seed"))?;
    let receipt = tokio::time::timeout(Duration::from_secs(2), results.next())
        .await?
        .unwrap()
        .map_err(|_| anyhow::anyhow!("seed result"))?;
    assert_eq!(receipt["status"], "accepted");
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let blocker=fg.inputs.sync.writer.submit(ctox_store::WriteJob::new(Default::default(),move|tx| {
        let _=entered.send(()); wait.recv_timeout(Duration::from_secs(5)).map_err(|_|StoreError::DeadlineExceeded)?;
        let table:String=tx.query_row("SELECT table_name FROM _collections WHERE name='notes'",[],|r|r.get(0))?;
        tx.execute(&format!("UPDATE \"{table}\" SET data=json_set(data,'$.owner','bob'),rev='changed-by-core' WHERE id='ownership'"),[])?;Ok(())
    }))?;
    started.await?;
    let mut write = request(
        2,
        "ownership",
        serde_json::json!({"id":"ownership","owner":"alice","title":"must reject"}),
    );
    write["base_rev"] = receipt["rev"].clone();
    let pending = source.write(&peer, write);
    let mut pending = tokio::spawn(pending);
    // Keep the writer held while a misplaced read-pool preflight could finish.
    // This is a controlled concurrency fault, not a latency acceptance budget.
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut pending)
            .await
            .is_err()
    );
    release.send(())?;
    blocker.await?;
    pending
        .await?
        .map_err(|_| anyhow::anyhow!("write admission failed"))?;
    let denied = tokio::time::timeout(Duration::from_secs(2), results.next())
        .await?
        .unwrap()
        .map_err(|_| anyhow::anyhow!("denial"))?;
    assert_eq!(
        denied["status"], "rejected",
        "queued write reused pre-job authority"
    );
    assert!(
        matches!(denied["reason"].as_str(), Some("forbidden" | "revoked")),
        "test did not reach final transaction authority: {denied}"
    );
    fg.shutdown().await?;
    Ok(())
}
