// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
async fn fixture() -> anyhow::Result<(
    tempfile::TempDir,
    Arc<Registry<u64>>,
    Arc<Session>,
    String,
    tokio::task::JoinHandle<()>,
)> {
    let root = super::super::project_chats::tests::speech_live_fixture()?;
    let instance = store::stable_instance_id(root.path())?;
    let token = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner",
        "Owner",
        "admin",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0;
    let binding = jour_fixe_speech::open_binding(root.path(), &token, "project", "meeting-1", 1)?;
    let (endpoint, server) = crate::execution::speech::tests::fixture("normal").await;
    let stream = crate::execution::speech::tests::start(&endpoint).await;
    let bound = BoundTranscription::bind(root.path(), &token, binding.clone(), stream)?;
    let id = bound.stream_id().to_owned();
    let authority = Arc::new(Authority {
        root: root.path().to_owned(),
        token,
        scope: Scope {
            instance_id: instance,
            project_id: "project".into(),
            meeting_id: "meeting-1".into(),
            deck_revision: 1,
        },
        binding,
        current: Arc::new(|| true),
    });
    let (commands, rx) = mpsc::channel(8);
    let s = Arc::new(Session {
        authority: Arc::clone(&authority),
        commands,
        cancel: Arc::new(Notify::new()),
        canceled: Arc::new(AtomicBool::new(false)),
        snapshot: Arc::new(Mutex::new(Snapshot::default())),
        commit_gate: Arc::new(Mutex::new(())),
        request_id: Uuid::new_v4().to_string(),
        opened: Instant::now(),
        task: Mutex::new(None),
    });
    let registry = Arc::new(Registry {
        sessions: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        opening: tokio::sync::Mutex::new(()),
    });
    let permit = Arc::clone(&registry.slots).try_acquire_owned()?;
    *s.task.lock().unwrap() = Some(tokio::spawn(run(
        bound,
        rx,
        authority,
        Arc::clone(&s.cancel),
        Arc::clone(&s.canceled),
        Arc::clone(&s.snapshot),
        Arc::clone(&s.commit_gate),
        permit,
    )));
    registry
        .sessions
        .lock()
        .unwrap()
        .insert(id.clone(), (7, Arc::clone(&s)));
    Ok((root, registry, s, id, server))
}
fn req(s: &Session, id: &str, op: &str) -> Request {
    Request {
        op: op.into(),
        scope: s.authority.scope.clone(),
        stream_id: Some(id.into()),
        request_id: None,
        sequence: None,
        pcm_base64: None,
        after_sequence: None,
    }
}
async fn call(registry: &Arc<Registry<u64>>, s: &Session, r: Request) -> anyhow::Result<Value> {
    Ok(handle(
        Arc::clone(registry),
        7,
        s.authority.root.clone(),
        s.authority.token.clone(),
        Arc::clone(&s.authority.current),
        r,
    )
    .await?
    .value)
}
#[tokio::test]
async fn real_final_has_private_handle_only_after_domain_commit() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture().await?;
    let mut w = req(&s, &id, "write");
    w.sequence = Some(1);
    w.pcm_base64 = Some(STANDARD.encode([0; 640]));
    call(&registry, &s, w).await?;
    call(&registry, &s, req(&s, &id, "finish")).await?;
    let receipt = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut r = req(&s, &id, "read");
            r.after_sequence = Some(0);
            let v = call(&registry, &s, r).await?;
            if v["state"] == "committed" {
                break Ok::<_, anyhow::Error>(v);
            }
            ensure!(v["state"] != "failed", "{v}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert!(receipt["receipt"]["handle"]
        .as_str()
        .is_some_and(|h| Uuid::parse_str(h).is_ok()));
    assert_eq!(receipt["receipt"]["meetingRevision"], 1);
    assert!(receipt.get("text").is_none());
    assert_eq!(
        jour_fixe_speech::committed_revision(
            root.path(),
            &s.authority.token,
            &s.authority.binding,
            &id
        )?,
        1
    );
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn cancel_before_finish_does_not_persist_partial() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture().await?;
    let mut w = req(&s, &id, "write");
    w.sequence = Some(1);
    w.pcm_base64 = Some(STANDARD.encode([0; 640]));
    call(&registry, &s, w).await?;
    assert_eq!(
        call(&registry, &s, req(&s, &id, "cancel")).await?["state"],
        "canceled"
    );
    assert!(call(&registry, &s, req(&s, &id, "finish")).await.is_err());
    let conn = rusqlite::Connection::open(store::business_os_store_path(root.path()))?;
    let n:i64=conn.query_row("SELECT count(*) FROM workjet_jour_fixe_speech_receipts WHERE final_json IS NOT NULL OR consumed_command IS NOT NULL",[],|r|r.get(0))?;
    assert_eq!(n, 0);
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn wrong_peer_scope_and_revoked_actor_cannot_reuse_stream() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture().await?;
    let r = req(&s, &id, "finish");
    assert!(registry.lookup(&8, &s.authority.token, &r).is_err());
    let mut changed = req(&s, &id, "finish");
    changed.scope.deck_revision = 2;
    assert!(registry.lookup(&7, &s.authority.token, &changed).is_err());
    assert!(registry.lookup(&7, "other-token", &r).is_err());
    let conn = rusqlite::Connection::open(store::business_os_store_path(root.path()))?;
    conn.execute(
        "UPDATE business_users SET active=0 WHERE user_id='owner'",
        [],
    )?;
    assert!(call(&registry, &s, r).await.is_err());
    assert!(s.authority.with_current(&mut || Ok(())).is_err());
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn duplicate_pcm_is_idempotent_and_conflicting_sequence_is_terminal() -> anyhow::Result<()> {
    let (_root, registry, s, id, server) = fixture().await?;
    for _ in 0..2 {
        let mut w = req(&s, &id, "write");
        w.sequence = Some(1);
        w.pcm_base64 = Some(STANDARD.encode([0; 640]));
        call(&registry, &s, w).await?;
    }
    let mut w = req(&s, &id, "write");
    w.sequence = Some(1);
    w.pcm_base64 = Some(STANDARD.encode([1; 640]));
    assert!(call(&registry, &s, w).await.is_err());
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
