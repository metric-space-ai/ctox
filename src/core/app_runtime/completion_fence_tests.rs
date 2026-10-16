//! Real Node IPC and canonical Store leases; only the fixture's grants are static.
use super::*;
use ctox_store::{
    change_log::subscription::{
        protocol::{OrderedWrite, Seq, Write, WriteOp, WriteStatus},
        write_contract::{self, CommandLease, OrderedWriter},
        WriteAuthority, WritePermission,
    },
    schema::{Collection, CollectionDefinition},
    testkit::StoreFixture,
    writer::WriterHandle,
    DbId, JobOptions, WriteJob,
};
use rusqlite::Transaction;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};

const NOW: u64 = 1_800_000_000_000;
struct Grants;
impl WriteAuthority for Grants {
    fn authorize_command_intent(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if participant == "client" {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
    fn authorize_command_executor(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(if matches!(participant, "executor-a" | "executor-b") {
            WritePermission::Permit
        } else {
            WritePermission::Reject("forbidden".into())
        })
    }
    fn authorize(
        &self,
        _: &Transaction<'_>,
        participant: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(
            if matches!(participant, "client" | "executor-a" | "executor-b") {
                WritePermission::Permit
            } else {
                WritePermission::Reject("forbidden".into())
            },
        )
    }
    fn validate_document(
        &self,
        _: &Transaction<'_>,
        _: &str,
        _: &Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Permit)
    }
}
struct LeaseHost {
    writer: WriterHandle,
    now: AtomicU64,
    handoff: Mutex<Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>>,
}
impl LeaseHost {
    fn check(&self, binding: &Value) -> Result<()> {
        let participant = string(&binding["fence"], "participant")?.to_owned();
        let command = string(&binding["command"], "id")?.to_owned();
        let fence: Seq = serde_json::from_value(binding["fence"]["fence"].clone())?;
        let now = self.now.load(Ordering::SeqCst);
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                Ok(write_contract::check_executor_fence(
                    tx,
                    &participant,
                    &command,
                    fence,
                    now,
                )?)
            }))?
            .blocking_wait()?
            .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.reason))
    }
}
impl RuntimeHost for LeaseHost {
    fn authorize(&self, _: &str, actor: &Value, _: &Value) -> Result<()> {
        ensure!(actor["id"] == "owner", "forbidden: actor");
        Ok(())
    }
    fn exported(&self, _: &str, _: &str, _: &str) -> Result<bool> {
        Ok(false)
    }
    fn capability_available(&self, _: &str) -> bool {
        false
    }
    fn admit(&self, _: &Value, binding: &Value) -> Result<()> {
        self.check(binding)
    }
    fn start_bound(&self, binding: &Value, effect: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.check(binding)?;
        // No process/IPC work runs in the Store writer thread.
        effect()
    }
    fn invoke(&self, _: &str, _: &Value, method: &str, _: &Value) -> Result<Value> {
        ensure!(method == "log", "unexpected context callback");
        let handoff = self.handoff.lock().unwrap().take();
        if let Some((entered, release)) = handoff {
            entered.send(())?;
            release.recv_timeout(Duration::from_secs(10))?;
        }
        Ok(Value::Null)
    }
}
fn envelope(fail: bool) -> Value {
    json!({"id":"command-1","app_id":"fixture","type":"task","payload":{"fail":fail},
        "actor_id":"owner","created_at":"2027-01-15T08:00:00Z","status":"queued"})
}
fn command(fail: bool) -> Value {
    let mut command = envelope(fail);
    command["app_ver"] = json!("1");
    command["actor"] = json!({"id":"owner"});
    command
}
fn check_owner_outcome(result: Result<Value>, fail: bool) {
    if fail {
        let error = result.expect_err("owner's handler error must be preserved");
        assert!(error.downcast_ref::<HandlerError>().is_some(), "{error:#}");
        assert!(error.to_string().contains("expected-handler-error"));
    } else {
        assert_eq!(result.unwrap(), json!({"ok":true}));
    }
}
#[test]
fn terminal_result_requires_current_store_executor_after_inflight_takeover() {
    terminal_frame_after_takeover(false);
}
#[test]
fn terminal_error_requires_current_store_executor_after_inflight_takeover() {
    terminal_frame_after_takeover(true);
}
#[cfg(feature = "legacy-store")]
#[test]
fn legacy_adapter_terminal_result_requires_current_store_executor() {
    terminal_frame_with_adapter(false, true);
}
#[cfg(feature = "legacy-store")]
#[test]
fn legacy_adapter_terminal_error_requires_current_store_executor() {
    terminal_frame_with_adapter(true, true);
}
fn terminal_frame_after_takeover(fail: bool) {
    terminal_frame_with_adapter(fail, false);
}
#[cfg(feature = "legacy-store")]
impl crate::legacy_store::LegacyStorePolicy for LeaseHost {
    fn project_read(&self, _: &str, _: &Value, _: &str, _: &Value) -> Result<Option<Value>> {
        anyhow::bail!("unexpected: this lease test only logs")
    }
    fn prepare_write(
        &self,
        _: &str,
        _: &Value,
        _: &str,
        _: Option<&Value>,
        _: Option<Value>,
    ) -> Result<Option<Value>> {
        anyhow::bail!("unexpected: this lease test only logs")
    }
}
#[cfg(feature = "legacy-store")]
struct LeaseHash;
#[cfg(feature = "legacy-store")]
impl rxdb::types::HashFunction for LeaseHash {
    fn hash<'a>(&'a self, input: String) -> rxdb::types::HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}
fn terminal_frame_with_adapter(fail: bool, legacy: bool) {
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let root = tempfile::tempdir_in(
            std::env::var("CTOX_APP_RUNTIME_TEST_ROOT").expect("lane scratch root"),
        )
        .unwrap();
        let store = runtime
            .block_on(StoreFixture::open(
                DbId::new(root.path().join("store.sqlite3")).unwrap(),
            ))
            .unwrap();
        runtime
            .block_on(store.write(JobOptions::default(), |tx| {
                ctox_store::schema::create_collection(
                    tx,
                    &CollectionDefinition {
                        name: "business_commands".into(),
                        app_id: "fixture".into(),
                        schema_ver: 1,
                        schema_json: write_contract::command_document_schema(),
                        class: "full".into(),
                        client_limit: 100,
                        byte_budget: 1_000_000,
                        rights_ver: 1,
                        app_ver: "1".into(),
                    },
                )?;
                Ok(())
            }))
            .unwrap();
        let ingress = OrderedWriter::new(store.writer(), Arc::new(Grants));
        let mut bytes = [0u8; 16];
        bytes[..6].copy_from_slice(&NOW.to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[15] = 1;
        let submitted = runtime
            .block_on(ingress.submit(
                "client".into(),
                OrderedWrite {
                    client_seq: Seq(1),
                    write: Write {
                        op_id: Uuid::from_bytes(bytes).to_string(),
                        prev_op_id: None,
                        collection: "business_commands".into(),
                        id: "command-1".into(),
                        base_rev: None,
                        expected_revision: None,
                        base_seq: None,
                        gen: 1,
                        schema_ver: 1,
                        app_ver: "1".into(),
                        op: WriteOp::Put,
                        body: Some(envelope(fail)),
                    },
                },
                NOW,
            ))
            .unwrap();
        assert_eq!(submitted.results[0].result.status, WriteStatus::Accepted);
        let first = runtime
            .block_on(ingress.claim_command("executor-a".into(), "command-1".into(), None, NOW))
            .unwrap()
            .unwrap();
        let host = Arc::new(LeaseHost {
            writer: store.writer(),
            now: AtomicU64::new(NOW),
            handoff: Mutex::new(None),
        });
        let runtime_host: Arc<dyn RuntimeHost> = host.clone();
        #[cfg(feature = "legacy-store")]
        let legacy_db = if legacy {
            use rxdb::rx_database::{create_rx_database, RxDatabaseCreator};
            use rxdb::storage::sqlite::{get_rx_storage_sqlite, RxStorageSqliteSettings};
            Some(
                runtime
                    .block_on(create_rx_database(RxDatabaseCreator {
                        name: format!("lease-adapter-{}", Uuid::new_v4()),
                        storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
                            database_path: root.path().join("legacy.sqlite"),
                        }),
                        multi_instance: false,
                        password: None,
                        hash_function: Arc::new(LeaseHash),
                        options: Default::default(),
                        ignore_duplicate: false,
                        close_duplicates: false,
                        event_reduce: false,
                        allow_slow_count: false,
                    }))
                    .unwrap(),
            )
        } else {
            None
        };
        #[cfg(feature = "legacy-store")]
        let runtime_host: Arc<dyn RuntimeHost> = match &legacy_db {
            Some(db) => Arc::new(
                crate::legacy_store::LegacyStoreAdapter::new(
                    db.clone(),
                    runtime.handle().clone(),
                    host.clone(),
                )
                .unwrap(),
            ),
            None => runtime_host,
        };
        #[cfg(not(feature = "legacy-store"))]
        assert!(!legacy);
        let node =
            PathBuf::from(std::env::var("CTOX_APP_RUNTIME_TEST_NODE").expect("lane Node runtime"));
        let pool = Arc::new(
            RuntimePool::with_limits(
                node,
                root.path().join("ipc"),
                runtime_host,
                CALL_LIMIT,
                IDLE_LIMIT,
            )
            .unwrap(),
        );
        let source = "export default async(command,ctx)=>{await ctx.log('info','ready');if(command.payload.fail)throw Error('expected-handler-error');return {ok:true}};";
        let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
        pool.register(AppBundle::new(json!({"app_id":"fixture","app_ver":"1","collections":{},
            "handlers":{"task":"handlers/run.mjs"},"files":{"handlers/run.mjs":hash},"capabilities_required":[]}),
            BTreeMap::from([("handlers/run.mjs".into(),source.into())]),BTreeSet::new()).unwrap()).unwrap();
        check_owner_outcome(
            pool.dispatch(command(fail), serde_json::to_value(&first).unwrap()),
            fail,
        );
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        *host.handoff.lock().unwrap() = Some((entered_tx, release_rx));
        let owned = pool.clone();
        let old: CommandLease = first.clone();
        let turn = thread::spawn(move || {
            owned.dispatch(command(fail), serde_json::to_value(old).unwrap())
        });
        entered_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("real Node handler reached the handoff barrier");
        host.now.store(NOW + 30_001, Ordering::SeqCst);
        let next = runtime
            .block_on(ingress.claim_command(
                "executor-b".into(),
                "command-1".into(),
                Some(first.fence),
                NOW + 30_001,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(next.participant, "executor-b");
        assert!(next.fence.0 > first.fence.0);
        release_tx.send(()).unwrap();
        let late = turn.join().unwrap();
        assert!(
            late.is_err(),
            "foreign executor completion accepted: {late:?}"
        );
        assert!(
            late.unwrap_err().to_string().contains("stale_fence"),
            "late handler outcome escaped its executor fence"
        );
        check_owner_outcome(
            pool.dispatch(command(fail), serde_json::to_value(&next).unwrap()),
            fail,
        );
        drop(pool);
        drop(host);
        #[cfg(feature = "legacy-store")]
        if let Some(db) = legacy_db {
            runtime.block_on(db.close()).unwrap();
        }
        runtime.block_on(store.shutdown()).unwrap();
    }
}
