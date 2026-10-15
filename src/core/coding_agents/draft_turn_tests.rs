// Origin: CTOX
// License: AGPL-3.0-only
//! §16.44 contract invariants on the real canonical store (StoreFixture: real
//! WriterHandle, real SQLite, real change-log triggers). Each test names the
//! mutation that turns it red (E13); see befund-coding-panel.md.
use super::*;
use ctox_store::{schema::CollectionDefinition, testkit::StoreFixture, DbId};
use std::sync::mpsc;

struct MemoryContent(Mutex<HashMap<String, Vec<u8>>>);
impl MemoryContent {
    fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(HashMap::new())))
    }
    fn text(&self, sha: &str) -> String {
        String::from_utf8(self.0.lock().unwrap()[sha].clone()).unwrap()
    }
}
impl DraftContent for MemoryContent {
    fn get(&self, sha256: &str) -> Result<Vec<u8>, String> {
        self.0
            .lock()
            .unwrap()
            .get(sha256)
            .cloned()
            .ok_or_else(|| format!("missing {sha256}"))
    }
    fn put(&self, bytes: &[u8]) -> Result<String, String> {
        let sha = format!("{:x}", Sha256::digest(bytes));
        self.0.lock().unwrap().insert(sha.clone(), bytes.to_vec());
        Ok(sha)
    }
}

/// Sidecar stand-in: returns the agent's snapshot; `during` runs while the turn
/// is in flight (e.g. the user saving the draft). Counts invocations.
struct ScriptedRunner {
    snapshot: Vec<(String, String)>,
    during: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    calls: Mutex<usize>,
}
impl ScriptedRunner {
    fn new(snapshot: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            snapshot: snapshot
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            during: Mutex::new(None),
            calls: Mutex::new(0),
        })
    }
}
impl SidecarRunner for ScriptedRunner {
    fn run(
        &self,
        _input: &SidecarTurnInput,
        _control: &TurnControl,
        progress: &dyn Fn(),
    ) -> Result<Value, TurnFailure> {
        *self.calls.lock().unwrap() += 1;
        progress();
        if let Some(during) = self.during.lock().unwrap().take() {
            during();
        }
        Ok(json!({
            "ok": true,
            "messages": [{"role":"assistant","content":[
                {"type":"text","text":"edited"},
                {"type":"toolCall","name":"write","arguments":{"path":"/workspace/a.js"}}]}],
            "snapshot": self.snapshot.iter().map(|(p, c)| json!({
                "kind":"file","path":format!("/workspace/{p}"),"content":c})).collect::<Vec<_>>(),
        }))
    }
}

/// Spawns a REAL OS process as the "sidecar", hands it to the TurnControl and
/// blocks like the IPC read does, until the process is gone or 20 s pass.
struct ProcessRunner {
    started: Mutex<Option<mpsc::Sender<u32>>>,
    observed_exit: Arc<AtomicBool>,
}
impl SidecarRunner for ProcessRunner {
    fn run(
        &self,
        _input: &SidecarTurnInput,
        control: &TurnControl,
        _progress: &dyn Fn(),
    ) -> Result<Value, TurnFailure> {
        let child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn stand-in sidecar");
        let pid = child.id();
        let _guard = control.attach(child)?;
        if let Some(started) = self.started.lock().unwrap().take() {
            let _ = started.send(pid);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                self.observed_exit.store(true, Ordering::SeqCst);
                return Err(TurnFailure::new("unavailable", "sidecar_transport"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(json!({"ok":true,"messages":[],
            "snapshot":[{"kind":"file","path":"/workspace/a.js","content":"agent\n"}]}))
    }
}

struct Env {
    _rt: tokio::runtime::Runtime,
    _dir: tempfile::TempDir,
    fixture: StoreFixture,
    content: Arc<MemoryContent>,
}

fn definition(name: &str) -> CollectionDefinition {
    CollectionDefinition {
        name: name.into(),
        app_id: "ctox-platform".into(),
        schema_ver: 1,
        schema_json: json!({"primaryKey":"id","type":"object",
            "properties":{"id":{"type":"string"}}}),
        class: if name == COLLECTION_TURNS {
            "window"
        } else {
            "full"
        }
        .into(),
        client_limit: 0,
        byte_budget: 1 << 20,
        rights_ver: 1,
        app_ver: "test".into(),
    }
}

fn env() -> Env {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let fixture = rt
        .block_on(StoreFixture::open(
            DbId::new(dir.path().join("ctox.sqlite3")).unwrap(),
        ))
        .unwrap();
    let content = MemoryContent::new();
    let original = content.put(b"original\n").unwrap();
    let keep = content.put(b"keep\n").unwrap();
    rt.block_on(fixture.write(JobOptions::default(), move |tx| {
        for name in [
            COLLECTION_DRAFTS,
            COLLECTION_TURNS,
            "files",
            "business_commands",
            "business_apps",
            "business_app_versions",
        ] {
            schema::create_collection(tx, &definition(name))?;
        }
        for user in ["alice", "bob"] {
            put(
                tx,
                COLLECTION_DRAFTS,
                &format!("{user}/notes"),
                &json!({
                    "id": format!("{user}/notes"), "app_id":"notes", "user_id": user,
                    "base_app_ver":"1.0.0", "files":{"a.js":original,"b.js":keep,"gone.js":null},
                    "updated_at_ms": 1,
                }),
            )?;
        }
        put(
            tx,
            "business_apps",
            "notes",
            &json!({"id":"notes","active_ver":"1.0.0"}),
        )?;
        put(
            tx,
            "business_app_versions",
            "notes@1.0.0",
            &json!({
            "id":"notes@1.0.0","app_id":"notes","app_ver":"1.0.0",
            "manifest":{"files":{"a.js":original}}}),
        )?;
        Ok(())
    }))
    .unwrap();
    Env {
        _rt: rt,
        _dir: dir,
        fixture,
        content,
    }
}

impl Env {
    fn panel(&self, runner: Arc<dyn SidecarRunner>) -> CodingPanel {
        CodingPanel::new(self.fixture.writer(), self.content.clone(), runner)
    }
    fn row(&self, collection: &'static str, id: &str) -> Option<(String, Value)> {
        let id = id.to_owned();
        self._rt
            .block_on(self.fixture.write(JobOptions::default(), move |tx| {
                get_with_rev(tx, collection, &id)
            }))
            .unwrap()
    }
    fn count(&self, collection: &'static str) -> i64 {
        self._rt
            .block_on(self.fixture.write(JobOptions::default(), move |tx| {
                let (table, _) = table(tx, collection)?;
                Ok(tx.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE deleted=0"),
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            }))
            .unwrap()
    }
    fn draft_rev(&self, user: &str) -> String {
        self.row(COLLECTION_DRAFTS, &format!("{user}/notes"))
            .unwrap()
            .0
    }
}

fn actor(id: &str, role: &str) -> BusinessOsActor {
    BusinessOsActor::new(Some(id.into()), role)
}
fn turn(id: &str, payload: Value) -> CommandDocument {
    CommandDocument {
        id: id.into(),
        module: "reports".into(),
        command_type: COMMAND_TURN.into(),
        payload,
        on_complete: Some("reports.coding.done".into()),
        on_failed: Some("reports.coding.failed".into()),
    }
}
fn turn_payload(env: &Env, user: &str, turn_id: &str) -> Value {
    json!({"app_id":"notes","draft_id":format!("{user}/notes"),
        "base_draft_rev":env.draft_rev(user),"prompt":"rename","turn_id":turn_id})
}
fn cancel(id: &str, turn_id: &str) -> CommandDocument {
    CommandDocument {
        id: id.into(),
        module: "reports".into(),
        command_type: COMMAND_CANCEL.into(),
        payload: json!({"turn_id":turn_id}),
        on_complete: None,
        on_failed: None,
    }
}

/// Mutation: `put_if_rev` without `AND rev=?5`, or the `rev == base_rev` arm in
/// `complete` replaced by `Some((_, mut draft))` → the user's edit is overwritten.
#[test]
fn cas_conflict_keeps_user_edit_and_returns_patch() {
    let env = env();
    let runner = ScriptedRunner::new(&[("a.js", "agent\n"), ("b.js", "keep\n")]);
    let user_sha = env.content.put(b"user edit\n").unwrap();
    let writer = env.fixture.writer();
    let user_sha_in_edit = user_sha.clone();
    *runner.during.lock().unwrap() = Some(Box::new(move || {
        // The user saves the draft through the normal write path mid-turn.
        writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                let (_, mut draft) = get_with_rev(tx, COLLECTION_DRAFTS, "alice/notes")?.unwrap();
                draft["files"]["a.js"] = json!(user_sha_in_edit);
                put(tx, COLLECTION_DRAFTS, "alice/notes", &draft)
            }))
            .unwrap()
            .blocking_wait()
            .unwrap();
    }));
    let panel = env.panel(runner.clone());
    let admitted = panel
        .execute(
            actor("alice", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            10,
        )
        .unwrap();
    assert_eq!(admitted.result["status"], "running");
    let final_turn = admitted.worker.unwrap().join().unwrap();
    assert_eq!(final_turn["state"], "conflict", "{final_turn}");
    let (_, draft) = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    assert_eq!(
        draft["files"]["a.js"],
        json!(user_sha),
        "user edit must survive"
    );
    assert!(final_turn["draft_rev_after"].is_null());
    let reference = final_turn["patch"]["ref"].as_str().unwrap();
    let patch = env.content.text(reference);
    assert!(
        patch.contains("+agent") && patch.contains("-original"),
        "{patch}"
    );
    let (_, file) = env.row("files", "coding-patch:t1").unwrap();
    assert_eq!(file["ref"], json!(reference));
    assert_eq!(file["owner_app"], "notes");
}

/// Positive path + "active version untouched". Mutation: re-adding the legacy
/// write of the turn result into the active app source (an extra `put` of the
/// changed files into `business_app_versions` in `complete`, as the V3
/// `apply_changed_turn_snapshot` did) → red; dropping the files update in
/// `complete` → red on the draft assertion.
#[test]
fn done_creates_new_draft_revision_and_leaves_active_version_unchanged() {
    let env = env();
    let before_version = env.row("business_app_versions", "notes@1.0.0").unwrap();
    let before_app = env.row("business_apps", "notes").unwrap();
    let base = env.draft_rev("alice");
    let panel = env.panel(ScriptedRunner::new(&[
        ("a.js", "agent\n"),
        ("new.js", "n\n"),
    ]));
    let admitted = panel
        .execute(
            actor("alice", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            10,
        )
        .unwrap();
    let final_turn = admitted.worker.unwrap().join().unwrap();
    assert_eq!(final_turn["state"], "done", "{final_turn}");
    let (rev, draft) = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    assert_ne!(rev, base);
    assert_eq!(final_turn["draft_rev_after"], json!(rev));
    assert_eq!(
        env.content.text(draft["files"]["a.js"].as_str().unwrap()),
        "agent\n"
    );
    assert_eq!(
        env.content.text(draft["files"]["new.js"].as_str().unwrap()),
        "n\n"
    );
    // b.js was not in the snapshot → deleted in the draft only.
    assert!(draft["files"]["b.js"].is_null());
    assert_eq!(
        env.row("business_app_versions", "notes@1.0.0").unwrap(),
        before_version
    );
    assert_eq!(env.row("business_apps", "notes").unwrap(), before_app);
    assert_eq!(final_turn["messages"][1]["text"], "edited");
    assert_eq!(final_turn["tool_events"][0]["path"], "a.js");
}

/// Mutation: drop the `user_id`/`app_id` comparison in `admit_turn` → bob's
/// turn is admitted on alice's draft → red. Mutation: `may_edit_source`
/// returning true → the role "user" is admitted → red.
#[test]
fn foreign_draft_and_missing_right_are_refused_before_any_effect() {
    let env = env();
    let runner = ScriptedRunner::new(&[("a.js", "agent\n")]);
    let panel = env.panel(runner.clone());
    let before = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    // Even an admin may not run a turn on another user's draft.
    let foreign = panel
        .execute(
            actor("bob", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            10,
        )
        .unwrap();
    assert_eq!(foreign.result["status"], "rejected");
    assert_eq!(foreign.result["code"], "forbidden");
    assert_eq!(foreign.result["reason"], "draft_not_owned");
    assert!(foreign.worker.is_none());
    // Own draft, but no apps.source.edit.
    let no_right = panel
        .execute(
            actor("bob", "user"),
            turn("c2", turn_payload(&env, "bob", "t2")),
            10,
        )
        .unwrap();
    assert_eq!(no_right.result["code"], "forbidden");
    assert_eq!(no_right.result["reason"], "apps.source.edit");
    assert_eq!(env.count(COLLECTION_TURNS), 0);
    assert_eq!(*runner.calls.lock().unwrap(), 0);
    assert_eq!(env.row(COLLECTION_DRAFTS, "alice/notes").unwrap(), before);
    let (_, command) = env.row("business_commands", "c1").unwrap();
    assert_eq!(command["status"], "failed");
}

/// Mutation: `text("draft_id")`/`text("base_draft_rev")` in `parse_turn`
/// made optional → red.
#[test]
fn missing_draft_or_base_rev_is_invalid() {
    let env = env();
    let panel = env.panel(ScriptedRunner::new(&[]));
    for missing in ["draft_id", "base_draft_rev"] {
        let mut payload = turn_payload(&env, "alice", "t1");
        payload.as_object_mut().unwrap().remove(missing);
        let admitted = panel
            .execute(actor("alice", "admin"), turn("c1", payload), 10)
            .unwrap();
        assert_eq!(admitted.result["code"], "invalid");
        assert_eq!(
            admitted.result["reason"],
            format!("payload_invalid:{missing}")
        );
    }
    assert_eq!(env.count(COLLECTION_TURNS), 0);
}

/// Mutation: remove the `running_turn_for_draft` check → the second turn is
/// admitted → red. Mutation: `cancel` without `control.cancel()` → the stand-in
/// sidecar process survives until the 20 s deadline → `observed_exit` false → red.
#[cfg(target_os = "linux")]
#[test]
fn second_turn_is_refused_and_cancel_kills_the_sidecar() {
    let env = env();
    let (started_tx, started_rx) = mpsc::channel();
    let observed_exit = Arc::new(AtomicBool::new(false));
    let panel = env.panel(Arc::new(ProcessRunner {
        started: Mutex::new(Some(started_tx)),
        observed_exit: observed_exit.clone(),
    }));
    let before = env.row(COLLECTION_DRAFTS, "alice/notes").unwrap();
    let first = panel
        .execute(
            actor("alice", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            10,
        )
        .unwrap();
    let pid = started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    let second = panel
        .execute(
            actor("alice", "admin"),
            turn("c2", turn_payload(&env, "alice", "t2")),
            11,
        )
        .unwrap();
    assert_eq!(second.result["code"], "conflict");
    assert_eq!(second.result["reason"], "turn_running");
    assert!(second.worker.is_none());

    let started = Instant::now();
    let cancelled = panel
        .execute(actor("alice", "admin"), cancel("c3", "t1"), 12)
        .unwrap();
    assert_eq!(
        cancelled.result["state"], "cancelled",
        "{}",
        cancelled.result
    );
    let final_turn = first.worker.unwrap().join().unwrap();
    assert!(
        observed_exit.load(Ordering::SeqCst),
        "sidecar process survived cancel"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(final_turn["state"], "cancelled");
    assert_eq!(env.row(COLLECTION_DRAFTS, "alice/notes").unwrap(), before);
    // A second cancel finds no running turn.
    let third = panel
        .execute(actor("alice", "admin"), cancel("c4", "t1"), 13)
        .unwrap();
    assert_eq!(third.result["reason"], "turn_not_running");
}

/// Terminal exactly once + callback at most once (§16.39 Nr. 3). Mutation:
/// drop the `queued|running` guard in `finish_turn` → the late cancel
/// regresses `done` to `cancelled` → red. Mutation: `insert_new` → `put` and
/// no `callback_command_id` guard → the callback document is rewritten as
/// `reports.coding.failed` → red.
#[test]
fn terminal_state_and_callback_happen_exactly_once() {
    let env = env();
    let panel = env.panel(ScriptedRunner::new(&[
        ("a.js", "agent\n"),
        ("b.js", "keep\n"),
    ]));
    let admitted = panel
        .execute(
            actor("alice", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            10,
        )
        .unwrap();
    let final_turn = admitted.worker.unwrap().join().unwrap();
    assert_eq!(final_turn["state"], "done");
    let late = panel
        .execute(actor("alice", "admin"), cancel("c2", "t1"), 20)
        .unwrap();
    assert_eq!(late.result["reason"], "turn_not_running");
    assert_eq!(panel.recover_interrupted(30).unwrap(), 0);
    let (_, turn_doc) = env.row(COLLECTION_TURNS, "t1").unwrap();
    assert_eq!(turn_doc["state"], "done");
    let (_, callback) = env.row("business_commands", "coding-callback:c1").unwrap();
    assert_eq!(callback["command_type"], "reports.coding.done");
    assert_eq!(callback["module"], "reports");
    assert_eq!(callback["payload"]["turn_id"], "t1");
    assert_eq!(callback["payload"]["status"], "done");
    // Exactly: original turn command, cancel command, one callback.
    assert_eq!(env.count("business_commands"), 3);
    // Redelivering the same command is a replay, not a second turn.
    let replay = panel
        .execute(
            actor("alice", "admin"),
            turn("c1", turn_payload(&env, "alice", "t1")),
            40,
        )
        .unwrap();
    assert!(replay.worker.is_none());
    assert_eq!(replay.result["state"], "done");
    assert_eq!(env.count(COLLECTION_TURNS), 1);
}
