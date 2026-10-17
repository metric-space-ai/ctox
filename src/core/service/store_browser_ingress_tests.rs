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

#[test]
fn startup_failure_retains_typed_cause_without_logging_parameters() {
    let original = new_rx_error(
        "SQLITE_OWNER",
        Some(
            serde_json::json!({"token":"do-not-log-this","url":"ws://private.invalid?credential=hidden"}),
        ),
    );
    let failure = StartupFailure::at("database", original.clone());
    assert_eq!(
        failure
            .cause
            .downcast_ref::<rxdb::rx_error::RxError>()
            .unwrap()
            .code(),
        "SQLITE_OWNER"
    );
    for failure in [
        failure,
        StartupFailure::at("signaling", std::io::Error::other(original)),
    ] {
        let error = std::io::Error::other(failure);
        let public = format!("{error} {error:?}");
        assert!(
            public.contains("RxError(SQLITE_OWNER)"),
            "native cause code lost"
        );
        assert!(
            !public.contains("do-not-log-this")
                && !public.contains("private.invalid")
                && !public.contains("hidden"),
            "startup diagnostic leaked credential-bearing parameters"
        );
    }
}

/// Real foreground owners, the production responder and HTTP route. The local
/// server relays only signaling; it never supplies Core data or admission.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_signaling_precedes_http_and_launch_context_meets_budget() -> anyhow::Result<()> {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::{
        handshake::server::{Request, Response},
        Message,
    };
    let _scope = isolate_test_roots().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let Ok((tcp, _)) = accepted else { break; };
                    connections.spawn(async move {
                        let id = Arc::new(std::sync::Mutex::new(None::<String>));
                        let capture = id.clone();
                        let ws = tokio_tungstenite::accept_hdr_async(tcp, move |request: &Request, response: Response| {
                            if let Ok(url) = url::Url::parse(&format!("ws://127.0.0.1{}", request.uri())) {
                                *capture.lock().unwrap() = url.query_pairs().find(|(k,_)| k == "client").map(|(_,v)| v.into_owned());
                            }
                            Ok(response)
                        }).await;
                        let Ok(mut ws) = ws else { return; };
                        let Some(id) = id.lock().unwrap().clone() else { return; };
                        if ws.send(Message::text(serde_json::json!({"type":"init","yourPeerId":id}).to_string())).await.is_err() { return; }
                        while let Some(Ok(frame)) = ws.next().await {
                            match frame {
                                Message::Text(text) => {
                                    let Ok(packet) = serde_json::from_str::<Value>(&text) else { continue; };
                                    if packet["type"] == "join" && ws.send(Message::text(serde_json::json!({"type":"joined","room":packet["room"],"otherPeerIds":[id],"peers":[{"peerId":id,"role":"ctox_instance"}]}).to_string())).await.is_err() { break; }
                                }
                                Message::Ping(bytes) => { if ws.send(Message::Pong(bytes)).await.is_err() { break; } }
                                Message::Close(_) => break,
                                _ => (),
                            }
                        }
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => (),
            }
        }
    });
    // Abort-on-drop also owns all accepted connections through the JoinSet.
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _server = Server(server);
    let (root, mut initial, _, _) = write_fixture().await?;
    let config_root = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_ICE_SERVERS",
            r#"[{"urls":"stun:127.0.0.1:9"}]"#,
        )?;
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_WEB_AUTOSTART",
            "0",
        )
    })
    .await?;
    std::fs::write(
        root.path().join("runtime/business-os-signaling-urls.json"),
        serde_json::to_vec(&vec![format!("ws://{address}/signal")])?,
    )?;
    std::os::unix::fs::symlink(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/business-os"),
        root.path().join("business-os"),
    )?;
    initial.shutdown().await?;
    let mut owner = crate::service::store_startup::Foreground::start(
        root.path().to_owned(),
        tokio::runtime::Handle::current(),
    )
    .await?;
    let ingress = owner.inputs.browser_ingress.get().unwrap();
    assert!(
        ingress.owner_started(),
        "HTTP was published before owner signaling start"
    );
    let ready = tokio::time::timeout(Duration::from_secs(55), async {
        loop {
            let changed = ingress.changed.notified();
            if ingress.ready().is_ok() {
                return Ok::<_, std::io::Error>(());
            }
            if ingress.state.load(std::sync::atomic::Ordering::Acquire) == 3 {
                return ingress.wait_failed().await;
            }
            changed.await;
        }
    })
    .await?;
    if let Err(error) = ready {
        owner.shutdown().await?;
        return Err(error.into());
    }
    let request = ctox_store::ipc::Request {
        request_id: 19,
        kind: ctox_store::ipc::JobKind::Read,
        method: "http.listen".into(),
        payload: serde_json::to_vec(&serde_json::json!({"addr":"127.0.0.1:0"}))?,
    };
    let bytes = ctox_store::ipc::request(
        crate::service::queue_store::endpoint(root.path()),
        request,
        Default::default(),
    )?
    .await?;
    let address = serde_json::from_slice::<Value>(&bytes)?["addr"]
        .as_str()
        .unwrap()
        .to_owned();
    let measured = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<f64>> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(3))
            .build();
        let mut samples = Vec::new();
        for _ in 0..20 {
            let at = Instant::now();
            let response = agent
                .get(&format!("http://{address}/api/business-os/launch-context"))
                .call()?;
            anyhow::ensure!(
                response.status() == 200,
                "launch-context not ready after owner startup"
            );
            // The credential-bearing body is consumed only in memory, never logged.
            let _ = response.into_string()?;
            samples.push(at.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(samples)
    })
    .await?;
    owner.shutdown().await?;
    let samples = measured?;
    let p99 = samples.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "C11_LAUNCH_METRICS {}",
        serde_json::json!({"samples_ms":samples,"p99_ms":p99,"http":200})
    );
    assert!(p99 < 50.0, "launch-context p99 {p99:.3}ms exceeds 50ms");
    Ok(())
}
