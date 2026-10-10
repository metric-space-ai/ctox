// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

fn value(op: &str) -> Value {
    json!({"op":op, "commandId":Uuid::new_v4().to_string(), "scope":{"instanceId":"instance"}})
}
#[test]
fn wire_is_draft_only_strict_and_bounded() {
    let good = value("open");
    assert!(Request::parse(vec![good.clone()]).is_ok());
    for field in [
        "action",
        "text",
        "meetingId",
        "projectId",
        "deckRevision",
        "receipt",
        "speaker",
        "model",
        "provider",
        "capabilityToken",
    ] {
        let mut v = good.clone();
        v[field] = json!("forged");
        assert!(Request::parse(vec![v]).is_err(), "{field}");
    }
    let mut v = good.clone();
    v["scope"]["meetingId"] = json!("meeting");
    assert!(Request::parse(vec![v]).is_err());
    let mut v = good;
    v["commandId"] = json!("not-uuid");
    assert!(Request::parse(vec![v]).is_err());
    for bytes in [vec![], vec![0], vec![0; 3202]] {
        let mut v = value("write");
        v["streamId"] = json!(Uuid::new_v4().to_string());
        v["sequence"] = json!(1);
        v["pcmBase64"] = json!(STANDARD.encode(bytes));
        assert!(Request::parse(vec![v]).and_then(|r| r.pcm()).is_err());
    }
    let mut v = value("write");
    v["streamId"] = json!(Uuid::new_v4().to_string());
    v["sequence"] = json!(1);
    v["pcmBase64"] = json!(STANDARD.encode([0; 3200]));
    assert_eq!(Request::parse(vec![v]).unwrap().pcm().unwrap().len(), 3200);
}
type Fixture = (
    tempfile::TempDir,
    Arc<Registry<u64>>,
    Arc<Session>,
    String,
    tokio::task::JoinHandle<()>,
);
async fn fixture(mode: &'static str) -> anyhow::Result<Fixture> {
    // No project, meeting, deck, transcript or meeting speech binding exists.
    let root = tempfile::tempdir()?;
    let instance = store::stable_instance_id(root.path())?;
    let token = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "dictator",
        "Dictator",
        "chef",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0;
    let (endpoint, server) = crate::execution::speech::tests::fixture(mode).await;
    let stream = crate::execution::speech::tests::start(&endpoint).await;
    let id = stream.stream_id().to_owned();
    let authority = Arc::new(Authority {
        root: root.path().to_owned(),
        token,
        scope: Scope {
            instance_id: instance,
        },
        config_binding: config_binding(root.path())?,
        current: Arc::new(|| true),
    });
    let (commands, rx) = mpsc::channel(8);
    let session = Arc::new(Session {
        authority: Arc::clone(&authority),
        commands,
        cancel: Arc::new(Notify::new()),
        canceled: Arc::new(AtomicBool::new(false)),
        snapshot: Arc::new(Mutex::new(Snapshot::default())),
        final_gate: Arc::new(Mutex::new(())),
        open_command_id: Uuid::new_v4().to_string(),
        opened: Instant::now(),
        task: Mutex::new(None),
    });
    let registry = Arc::new(Registry {
        sessions: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        opening: tokio::sync::Mutex::new(()),
    });
    let permit = Arc::clone(&registry.slots).try_acquire_owned()?;
    *session.task.lock().unwrap() = Some(tokio::spawn(run(
        stream,
        rx,
        authority,
        Arc::clone(&session.cancel),
        Arc::clone(&session.canceled),
        Arc::clone(&session.snapshot),
        Arc::clone(&session.final_gate),
        permit,
    )));
    registry
        .sessions
        .lock()
        .unwrap()
        .insert(id.clone(), (7, Arc::clone(&session)));
    Ok((root, registry, session, id, server))
}
fn request(s: &Session, id: &str, op: &str) -> Request {
    Request {
        op: op.into(),
        command_id: Uuid::new_v4().to_string(),
        scope: s.authority.scope.clone(),
        stream_id: Some(id.into()),
        sequence: None,
        pcm_base64: None,
        after_sequence: None,
    }
}
async fn call(registry: &Arc<Registry<u64>>, s: &Session, r: Request) -> anyhow::Result<Value> {
    let response = handle(
        Arc::clone(registry),
        7,
        s.authority.root.clone(),
        s.authority.token.clone(),
        Arc::clone(&s.authority.current),
        r,
    )
    .await?;
    response.publication.with_current(&mut || Ok(()))?;
    Ok(response.result)
}
async fn write(
    registry: &Arc<Registry<u64>>,
    s: &Session,
    id: &str,
    seq: u64,
    byte: u8,
) -> anyhow::Result<Value> {
    let mut w = request(s, id, "write");
    w.sequence = Some(seq);
    w.pcm_base64 = Some(STANDARD.encode([byte; 640]));
    call(registry, s, w).await
}
async fn terminal(registry: &Arc<Registry<u64>>, s: &Session, id: &str) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let mut r = request(s, id, "read");
            r.after_sequence = Some(0);
            let v = call(registry, s, r).await?;
            if v["state"] == "finished" || v["state"] == "failed" {
                return Ok::<_, anyhow::Error>(v);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}
fn domain_counts(root: &Path) -> anyhow::Result<Vec<(String, i64)>> {
    let conn = rusqlite::Connection::open(store::business_os_store_path(root))?;
    let mut query = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'
        AND (name LIKE 'workjet_jour_fixe%' OR name LIKE 'workjet_project_chat%' OR name='business_commands')
        ORDER BY name")?;
    let tables: Vec<String> = query
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    tables
        .into_iter()
        .map(|name| {
            let count = conn.query_row(
                &format!("SELECT count(*) FROM \"{}\"", name.replace('"', "\"\"")),
                [],
                |r| r.get(0),
            )?;
            Ok((name, count))
        })
        .collect()
}
#[tokio::test]
async fn verified_final_is_correlated_draft_text_without_domain_mutations() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture("normal").await?;
    let before = domain_counts(root.path())?;
    write(&registry, &s, &id, 1, 0).await?;
    let r = request(&s, &id, "finish");
    let command = r.command_id.clone();
    let first = call(&registry, &s, r).await?;
    assert_eq!(first["commandId"], command);
    assert_eq!(first["streamId"], id);
    let final_value = terminal(&registry, &s, &id).await?;
    assert_eq!(final_value["state"], "finished", "{final_value}");
    assert_eq!(final_value["text"], "Hallo Welt.");
    assert_eq!(final_value["error"], Value::Null);
    assert!(final_value.get("receipt").is_none());
    assert_eq!(domain_counts(root.path())?, before);
    assert_eq!(
        call(&registry, &s, request(&s, &id, "finish")).await?["text"],
        "Hallo Welt."
    );
    let mut open = request(&s, &id, "open");
    open.stream_id = None;
    open.command_id = s.open_command_id.clone();
    assert_eq!(call(&registry, &s, open).await?["streamId"], id);
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn cancel_erases_even_an_already_finished_draft() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture("normal").await?;
    let before = domain_counts(root.path())?;
    write(&registry, &s, &id, 1, 0).await?;
    call(&registry, &s, request(&s, &id, "finish")).await?;
    assert_eq!(terminal(&registry, &s, &id).await?["state"], "finished");
    let canceled = call(&registry, &s, request(&s, &id, "cancel")).await?;
    assert_eq!(canceled["state"], "canceled");
    assert_eq!(canceled["text"], Value::Null);
    assert_eq!(canceled["events"], json!([]));
    assert_eq!(
        call(&registry, &s, request(&s, &id, "finish")).await?["text"],
        Value::Null
    );
    assert_eq!(domain_counts(root.path())?, before);
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn cancellation_before_finish_cannot_publish_a_final() -> anyhow::Result<()> {
    let (_root, registry, s, id, server) = fixture("normal").await?;
    write(&registry, &s, &id, 1, 0).await?;
    call(&registry, &s, request(&s, &id, "cancel")).await?;
    assert_eq!(
        call(&registry, &s, request(&s, &id, "finish")).await?["state"],
        "canceled"
    );
    assert_eq!(s.snapshot.lock().unwrap().text, None);
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn duplicate_audio_is_idempotent_but_conflicting_sequence_fails() -> anyhow::Result<()> {
    let (_root, registry, s, id, server) = fixture("normal").await?;
    write(&registry, &s, &id, 1, 0).await?;
    write(&registry, &s, &id, 1, 0).await?;
    let conflict = write(&registry, &s, &id, 1, 1).await;
    assert!(conflict.is_err(), "{conflict:?}");
    assert_eq!(
        terminal(&registry, &s, &id).await?["error"],
        "invalid_sequence_or_audio"
    );
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn replay_from_another_peer_token_or_instance_and_revocation_are_denied() -> anyhow::Result<()>
{
    let (root, registry, s, id, server) = fixture("normal").await?;
    let r = request(&s, &id, "read");
    assert!(registry.lookup(&8, &s.authority.token, &r).is_err());
    assert!(registry.lookup(&7, "different", &r).is_err());
    let mut r = request(&s, &id, "read");
    r.scope.instance_id = "other".into();
    assert!(registry.lookup(&7, &s.authority.token, &r).is_err());
    let conn = rusqlite::Connection::open(store::business_os_store_path(root.path()))?;
    conn.execute(
        "UPDATE business_users SET active=0 WHERE user_id='dictator'",
        [],
    )?;
    assert!(call(&registry, &s, request(&s, &id, "finish"))
        .await
        .is_err());
    assert!(s.authority.with_current(&mut || Ok(())).is_err());
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn saved_backend_change_retires_the_live_stream() -> anyhow::Result<()> {
    let (root, registry, s, id, server) = fixture("normal").await?;
    let mut config = SpeechRuntimeConfig::load(root.path())?;
    config.transcription = SpeechBackend::Mistral;
    config.save(root.path())?;
    assert!(call(&registry, &s, request(&s, &id, "finish"))
        .await
        .is_err());
    assert!(s.authority.with_current(&mut || Ok(())).is_err());
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[tokio::test]
async fn upstream_failure_is_sanitized_and_never_becomes_final_text() -> anyhow::Result<()> {
    let (_root, registry, s, id, server) = fixture("error").await?;
    write(&registry, &s, &id, 1, 0).await?;
    let v = terminal(&registry, &s, &id).await?;
    assert_eq!(v["state"], "failed");
    assert_eq!(v["text"], Value::Null);
    assert!(!v.to_string().contains("secret-private-transcript"));
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[test]
fn slow_reader_receives_coalesced_full_partial_snapshot() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let authority = Arc::new(Authority {
        root: root.path().into(),
        token: "unused".into(),
        scope: Scope {
            instance_id: "instance".into(),
        },
        config_binding: "unused".into(),
        current: Arc::new(|| true),
    });
    let (commands, _) = mpsc::channel(1);
    let s = Session {
        authority,
        commands,
        cancel: Arc::new(Notify::new()),
        canceled: Arc::new(AtomicBool::new(false)),
        snapshot: Arc::new(Mutex::new(Snapshot {
            event_sequence: 100,
            latest_event: Some(json!({"sequence":100,"text":"full partial"})),
            ..Snapshot::default()
        })),
        final_gate: Arc::new(Mutex::new(())),
        open_command_id: Uuid::new_v4().to_string(),
        opened: Instant::now(),
        task: Mutex::new(None),
    };
    let id = Uuid::new_v4().to_string();
    let mut r = request(&s, &id, "read");
    r.after_sequence = Some(0);
    assert_eq!(s.snapshot(&r, &id)?["events"][0]["text"], "full partial");
    r.after_sequence = Some(101);
    assert!(s.snapshot(&r, &id).is_err());
    Ok(())
}
#[tokio::test]
async fn configured_mistral_missing_key_is_a_bounded_typed_failure() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let instance = store::stable_instance_id(root.path())?;
    let token = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "dictator",
        "Dictator",
        "chef",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0;
    let mut config = SpeechRuntimeConfig::default();
    config.transcription = SpeechBackend::Mistral;
    config.save(root.path())?;
    let registry = Arc::new(Registry {
        sessions: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        opening: tokio::sync::Mutex::new(()),
    });
    let mut v = value("open");
    v["scope"]["instanceId"] = json!(instance);
    let r = Request::parse(vec![v])?;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        handle(
            registry,
            7u64,
            root.path().into(),
            token,
            Arc::new(|| true),
            r,
        ),
    )
    .await??;
    result.publication.with_current(&mut || Ok(()))?;
    assert_eq!(result.result["state"], "failed");
    assert_eq!(result.result["error"], "missing_credential");
    assert_eq!(result.result["text"], Value::Null);
    Ok(())
}
#[tokio::test]
async fn idle_stream_terminates_without_unbounded_microphone_work() -> anyhow::Result<()> {
    let (_root, registry, s, id, server) = fixture("normal").await?;
    tokio::time::sleep(Duration::from_millis(5300)).await;
    assert_eq!(terminal(&registry, &s, &id).await?["error"], "timeout");
    assert_eq!(registry.slots.available_permits(), 2);
    drop(registry);
    drop(s);
    server.abort();
    Ok(())
}
#[test]
fn preparation_snapshot_does_not_compete_with_or_replace_publication_fence() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let instance = store::stable_instance_id(root.path())?;
    let token = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "dictator",
        "Dictator",
        "chef",
        chrono::Utc::now().timestamp_millis(),
    )?
    .0;
    let authority = Authority {
        root: root.path().into(),
        token,
        scope: Scope {
            instance_id: instance,
        },
        config_binding: config_binding(root.path())?,
        current: Arc::new(|| true),
    };
    store::with_current_webrtc_capability_signer(root.path(), |_| {
        // A preparatory read is safe under another current publication reservation.
        authority.check()?;
        let mut published = false;
        assert!(authority
            .with_current(&mut || {
                published = true;
                Ok(())
            })
            .is_err());
        assert!(!published);
        Ok(())
    })?;
    authority.with_current(&mut || Ok(()))?;
    Ok(())
}
