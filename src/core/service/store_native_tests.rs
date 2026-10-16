//! Real foreground owners, encrypted key provider, WebRTC and Raft; no authority mock.
use super::*;
use crate::service::store_startup::Foreground;
use ctox_store::WriteJob;
use ctox_sync::{authority::Peer, host_config::HostMember};
use futures_util::{SinkExt, StreamExt};
use std::{collections::BTreeMap, sync::Mutex, time::Duration};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// Isolated signaling relay only. Membership, keys, transport, quorum and
/// authority remain the actual native production implementation.
struct Signal(tokio::task::JoinHandle<()>, String);
impl Drop for Signal {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl Signal {
    async fn start() -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("ws://{}?role=ctox_instance", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let members: Arc<Mutex<BTreeMap<String, mpsc::Sender<Message>>>> = Arc::default();
            let mut children = tokio::task::JoinSet::new();
            let mut next = 0;
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                next += 1;
                let id = format!("native{next:06}");
                let members = members.clone();
                children.spawn(async move {
                    let (mut writer, mut reader) = tokio_tungstenite::accept_async(stream).await.unwrap().split();
                    let (tx, mut rx) = mpsc::channel(128);
                    tx.try_send(Message::text(serde_json::json!({"type":"init","yourPeerId":id}).to_string())).unwrap();
                    loop {
                        tokio::select! {
                            next = rx.recv() => {
                                let Some(next) = next else { break; };
                                if writer.send(next).await.is_err() { break; }
                            }
                            next = reader.next() => {
                                let Some(Ok(Message::Text(text))) = next else { break; };
                                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                                match value["type"].as_str() {
                                    Some("join") => {
                                        let mut all = members.lock().unwrap();
                                        all.insert(id.clone(), tx.clone());
                                        let ids: Vec<_> = all.keys().cloned().collect();
                                        let peers: Vec<_> = ids.iter().map(|id|serde_json::json!({"peerId":id,"role":"ctox_instance"})).collect();
                                        let packet = Message::text(serde_json::json!({"type":"joined","otherPeerIds":ids,"peers":peers}).to_string());
                                        for target in all.values() { target.try_send(packet.clone()).unwrap(); }
                                    }
                                    Some("signal") => {
                                        assert_eq!(value["senderPeerId"], id);
                                        let target = members.lock().unwrap().get(value["receiverPeerId"].as_str().unwrap()).cloned();
                                        if let Some(target) = target { target.send(Message::text(value.to_string())).await.unwrap(); }
                                    }
                                    Some("ping") => {}
                                    other => panic!("unexpected native signaling {other:?}"),
                                }
                            }
                        }
                    }
                    members.lock().unwrap().remove(&id);
                });
            }
        });
        Ok(Self(task, url))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_retains_real_native_authority_after_sixty_seconds_idle() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "fixture refuses configured state root"
    );
    let signal = Signal::start().await?;
    let mut roots = Vec::new();
    let mut bootstrap = Vec::new();
    let mut peers = BTreeMap::new();
    for node in 1..=3 {
        let root = tempfile::Builder::new().prefix("c4-").tempdir_in("/tmp")?;
        let owner =
            Foreground::start(root.path().to_owned(), tokio::runtime::Handle::current()).await?;
        let identity = owner.inputs.instance_signer()?.key_id().to_owned();
        peers.insert(
            node,
            Peer {
                identity,
                executor: true,
                data_replica: true,
            },
        );
        roots.push(root);
        bootstrap.push(owner);
    }
    for (index, owner) in bootstrap.iter_mut().enumerate() {
        let config = ctox_sync::host_config::HostConfiguration {
            version: 1,
            scope_id: "cutover4-native".into(),
            local: HostMember::Voter {
                node_id: index as u64 + 1,
            },
            voters: peers.clone(),
            timing: Default::default(),
        };
        config.validate()?;
        let config = serde_json::to_string(&config)?;
        owner.inputs.databases["runtime"].writer.submit(WriteJob::new(Default::default(),move |tx| {
            tx.execute_batch("CREATE TABLE ctox_sync_host(singleton INTEGER PRIMARY KEY CHECK(singleton=1),configuration TEXT NOT NULL)")?;
            tx.execute("INSERT INTO ctox_sync_host VALUES(1,?1)",[config])?;
            Ok(())
        }))?.await?;
        let root = owner.inputs.root.clone();
        let transport = serde_json::json!({"signalingUrls":[signal.1],"iceServers":[]}).to_string();
        crate::persistence::run_core_sync(move || {
            crate::secrets::write_secret_record(
                &root,
                crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0,
                "transport:cutover4-native",
                &transport,
                None,
                serde_json::json!({"source":"isolated-real-native-fixture"}),
            )
        })
        .await?;
        owner.shutdown().await?;
    }
    let mut owners = Vec::new();
    for root in &roots {
        owners.push(
            Foreground::start(root.path().to_owned(), tokio::runtime::Handle::current()).await?,
        );
    }
    let mut authorities = Vec::new();
    let mut signers = Vec::new();
    for (index, owner) in owners.iter().enumerate() {
        let native = owner
            .inputs
            .native_session()
            .map_err(|_| anyhow::anyhow!("native session is not retained after attach"))?;
        assert_eq!(
            native.instance_signer.key_id(),
            peers[&(index as u64 + 1)].identity
        );
        assert!(Arc::ptr_eq(
            &native.instance_signer,
            &owner.inputs.instance_signer()?
        ));
        authorities.push(native.execution_authority.clone());
        signers.push(native.instance_signer.clone());
    }
    let idle = std::time::Instant::now();
    tokio::time::sleep(Duration::from_secs(60)).await;
    // This is a real quorum-confirmed operation, not node_id()/a cached flag.
    for authority in &authorities {
        assert!(
            tokio::time::timeout(Duration::from_secs(10), authority.worker_membership(9001))
                .await??
                .is_none()
        );
    }
    println!(
        "CUTOVER4_NATIVE_IDLE {{\"idle_ms\":{},\"quorum_read_after_idle\":true}}",
        idle.elapsed().as_millis()
    );
    for owner in &mut owners {
        owner.shutdown().await?;
    }
    for authority in &authorities {
        assert!(
            tokio::time::timeout(Duration::from_secs(5), authority.worker_membership(9001))
                .await?
                .is_err()
        );
    }
    for signer in &signers {
        assert!(signer.sign(b"retired native host").is_err());
    }
    Ok(())
}
