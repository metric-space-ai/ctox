use super::*;
use crate::config::ConfigBuilder;
use crate::features::Feature;
use crate::find_thread_path_by_id_str;
use chrono::TimeZone;
use ctox_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use ctox_protocol::protocol::AgentMessageEvent;
use ctox_protocol::protocol::AskForApproval;
use ctox_protocol::protocol::EventMsg;
use ctox_protocol::protocol::InitialHistory;
use ctox_protocol::protocol::SandboxPolicy;
use ctox_protocol::protocol::TurnContextItem;
use ctox_protocol::protocol::UserMessageEvent;
use pretty_assertions::assert_eq;
use std::fs::File;
use std::fs::{self};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

fn write_session_file(root: &Path, ts: &str, uuid: Uuid) -> std::io::Result<PathBuf> {
    let day_dir = root.join("sessions/2025/01/03");
    fs::create_dir_all(&day_dir)?;
    let path = day_dir.join(format!("rollout-{ts}-{uuid}.jsonl"));
    let mut file = File::create(&path)?;
    let meta = serde_json::json!({
        "timestamp": ts,
        "type": "session_meta",
        "payload": {
            "id": uuid,
            "timestamp": ts,
            "cwd": ".",
            "originator": "test_originator",
            "cli_version": "test_version",
            "source": "cli",
            "model_provider": "test-provider",
        },
    });
    writeln!(file, "{meta}")?;
    let user_event = serde_json::json!({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {
            "type": "user_message",
            "message": "Hello from user",
            "kind": "plain",
        },
    });
    writeln!(file, "{user_event}")?;
    Ok(path)
}

#[tokio::test]
async fn recorder_flush_propagates_writer_failure() -> std::io::Result<()> {
    let home = TempDir::new()?;
    let path = home.path().join("read-only-rollout.jsonl");
    fs::write(&path, b"original\n")?;
    // Tokio buffers the write; flushing this read-only handle must report the
    // actual OS write failure, including when the test runs as an administrator.
    let mut file = tokio::fs::File::from_std(File::open(&path)?);
    tokio::io::AsyncWriteExt::write_all(&mut file, b"not writable\n").await?;
    let (tx, rx) = mpsc::channel(1);
    let recorder = RolloutRecorder {
        tx,
        rollout_path: path.clone(),
        materialization_pending: Arc::new(AtomicBool::new(false)),
        materialization_staging_path: None,
        state_db: None,
        event_persistence_mode: EventPersistenceMode::Limited,
    };
    let writer = rollout_writer(
        Some(file),
        None,
        rx,
        None,
        home.path().to_path_buf(),
        path.clone(),
        None,
        None,
        "test-provider".to_string(),
        false,
        Arc::new(AtomicBool::new(false)),
    );
    let (flush, write) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(recorder.flush(), writer)
    })
    .await
    .expect("flush failure must acknowledge without hanging");
    let write_error = write.expect_err("read-only writer must fail");
    let flush_error = flush.expect_err("a failed flush must never acknowledge success");
    assert_eq!(flush_error.kind(), write_error.kind());
    assert_eq!(flush_error.to_string(), write_error.to_string());
    assert_eq!(fs::read(path)?, b"original\n");
    assert!(
        recorder.flush().await.is_err(),
        "failed writer stays closed"
    );
    Ok(())
}
fn staging_paths(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    let entries = fs::read_dir(parent).expect("read session directory");
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".rollout-materializing-"))
        {
            paths.push(entry_path);
        }
    }
    paths.sort();
    paths
}

struct MaterializationBarrierGuard(PathBuf);

impl Drop for MaterializationBarrierGuard {
    fn drop(&mut self) {
        release_materialization_barrier(&self.0);
    }
}

fn arm_materialization_barrier_for(
    recorder: &RolloutRecorder,
) -> (Arc<tokio::sync::Notify>, MaterializationBarrierGuard) {
    let reached = arm_materialization_barrier(recorder.rollout_path());
    let guard = MaterializationBarrierGuard(recorder.rollout_path().to_path_buf());
    (reached, guard)
}

async fn wait_materialization_reached(reached: &tokio::sync::Notify) {
    tokio::time::timeout(Duration::from_secs(5), reached.notified())
        .await
        .expect("writer must reach its per-recorder materialization barrier");
}

#[tokio::test]
async fn concurrent_reader_never_sees_partially_materialized_rollout() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "first-user-message".to_string(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await?;

    let (reached, barrier_guard) = arm_materialization_barrier_for(&recorder);
    let persist_recorder = recorder.clone();
    let persist = tokio::spawn(async move { persist_recorder.persist().await });
    wait_materialization_reached(&reached).await;

    let (observed_held_tx, observed_held_rx) = tokio::sync::oneshot::channel::<()>();
    let reader = tokio::spawn({
        let codex_home = config.codex_home.clone();
        let rollout_path = recorder.rollout_path().to_path_buf();
        async move {
            let mut observed_held_tx = Some(observed_held_tx);
            loop {
                let staging = staging_paths(&rollout_path);
                if !staging.is_empty() && !rollout_path.exists() {
                    let discovered =
                        find_thread_path_by_id_str(&codex_home, &thread_id.to_string())
                            .await
                            .expect("discovery search");
                    assert_eq!(
                        discovered, None,
                        "discovery must not find writer-owned preparation state"
                    );
                    if let Some(observed_held_tx) = observed_held_tx.take() {
                        observed_held_tx
                            .send(())
                            .expect("reader observation receiver stays waiting");
                    }
                }

                if rollout_path.exists() {
                    assert!(
                        observed_held_tx.is_none(),
                        "reader must observe the held window"
                    );
                    let discovered =
                        find_thread_path_by_id_str(&codex_home, &thread_id.to_string())
                            .await
                            .expect("discovery search");
                    assert_eq!(discovered.as_ref(), Some(&rollout_path));
                    let history = RolloutRecorder::get_rollout_history(&rollout_path).await?;
                    let InitialHistory::Resumed(resumed) = history else {
                        panic!("published rollout must contain history");
                    };
                    assert!(
                        resumed.history.iter().any(|item| matches!(
                            item,
                            RolloutItem::EventMsg(EventMsg::UserMessage(event))
                                if event.message == "first-user-message"
                        )),
                        "published rollout must contain complete history"
                    );
                    return Ok::<(), std::io::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    });

    tokio::time::timeout(Duration::from_secs(5), observed_held_rx)
        .await
        .expect("reader must observe held staging before release")
        .expect("reader observation channel");
    drop(barrier_guard);

    let (persist_result, reader_result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(persist, reader)
    })
    .await
    .expect("materialization/read race must complete");
    persist_result.map_err(std::io::Error::other)??;
    reader_result.map_err(std::io::Error::other)??;

    assert!(
        staging_paths(recorder.rollout_path()).is_empty(),
        "writer must remove owned staging after completed publication"
    );
    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn owned_preparation_files_are_unique_and_leave_stale_files_alone() {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await
        .expect("config");
    let thread_id = ThreadId::new();
    let stale_path = config
        .codex_home
        .join("sessions/.rollout-materializing-stale.jsonl");
    fs::create_dir_all(stale_path.parent().expect("stale file parent"))
        .expect("create stale file directory");
    fs::write(&stale_path, b"unrelated").expect("write stale file");

    let first = precompute_log_file_info(&config, thread_id).expect("first info");
    let second = precompute_log_file_info(&config, thread_id).expect("second info");
    assert_ne!(
        first.materializing_path, second.materializing_path,
        "each preparation must be writer-owned"
    );

    let (first_file, second_file) = tokio::join!(
        tokio::spawn({
            let path = first.materializing_path.clone();
            async move { open_materializing_log_file(&path).expect("first file") }
        }),
        tokio::spawn({
            let path = second.materializing_path.clone();
            async move { open_materializing_log_file(&path).expect("second file") }
        }),
    );
    drop(first_file.expect("first preparation task"));
    drop(second_file.expect("second preparation task"));

    assert!(first.materializing_path.exists());
    assert!(second.materializing_path.exists());
    assert_eq!(fs::read(&stale_path).expect("stale file"), b"unrelated");

    fs::remove_file(&first.materializing_path).expect("remove first");
    fs::remove_file(&second.materializing_path).expect("remove second");
    assert_eq!(fs::read(&stale_path).expect("stale file"), b"unrelated");
}

#[tokio::test]
async fn state_visibility_waits_until_public_rollout_publication() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .enable(Feature::Sqlite)
        .expect("test config should allow sqlite");
    let state_db = StateRuntime::init(config.codex_home.clone(), config.model_provider_id.clone())
        .await
        .expect("state db should initialize");
    state_db
        .mark_backfill_complete(None)
        .await
        .expect("backfill should be complete");

    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        Some(state_db.clone()),
        None,
    )
    .await?;
    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "state-visibility".to_string(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await?;

    let (reached, barrier_guard) = arm_materialization_barrier_for(&recorder);
    let persist_recorder = recorder.clone();
    let persist = tokio::spawn(async move { persist_recorder.persist().await });
    wait_materialization_reached(&reached).await;

    let staging_path = recorder
        .materialization_staging_path()
        .expect("deferred recorder keeps its staging path")
        .to_path_buf();
    assert!(staging_path.exists());
    assert!(!recorder.rollout_path().exists());
    let discovered = find_thread_path_by_id_str(&config.codex_home, &thread_id.to_string())
        .await
        .expect("discovery search");
    assert_eq!(discovered, None, "staging must not be discoverable");
    assert!(
        state_db
            .get_thread(thread_id)
            .await
            .expect("state db query")
            .is_none(),
        "staging metadata must not publish state"
    );

    drop(barrier_guard);
    tokio::time::timeout(Duration::from_secs(5), persist)
        .await
        .expect("publication must complete")
        .expect("persist task must not panic")?;

    assert!(!staging_path.exists());
    assert!(recorder.rollout_path().exists());
    let discovered = find_thread_path_by_id_str(&config.codex_home, &thread_id.to_string())
        .await
        .expect("discovery search");
    assert_eq!(discovered.as_deref(), Some(recorder.rollout_path()));
    assert!(
        state_db
            .get_thread(thread_id)
            .await
            .expect("state db query")
            .is_some(),
        "published rollout must make state visible"
    );
    let history = RolloutRecorder::get_rollout_history(recorder.rollout_path()).await?;
    let InitialHistory::Resumed(resumed) = history else {
        panic!("published rollout must contain history");
    };
    assert!(
        resumed.history.iter().any(|item| matches!(
            item,
            RolloutItem::EventMsg(EventMsg::UserMessage(event))
                if event.message == "state-visibility"
        )),
        "published rollout must contain complete history"
    );
    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn cancelled_persist_caller_does_not_leave_recorder_pending() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;
    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "cancelled-caller".to_string(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await?;

    let (reached, barrier_guard) = arm_materialization_barrier_for(&recorder);
    let persist_recorder = recorder.clone();
    let persist = tokio::spawn(async move { persist_recorder.persist().await });
    wait_materialization_reached(&reached).await;
    assert!(recorder.materialization_pending());
    persist.abort();
    persist
        .await
        .expect_err("caller cancellation should be visible to test");
    drop(barrier_guard);
    tokio::time::timeout(Duration::from_secs(5), recorder.flush())
        .await
        .expect("writer completion after cancellation must be bounded")?;
    assert!(!recorder.materialization_pending());
    assert!(recorder.rollout_path().exists());
    assert!(staging_paths(recorder.rollout_path()).is_empty());
    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn failed_creation_preserves_exact_staging_path() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;

    let rollout_path = recorder.rollout_path().to_path_buf();
    let staging_path = recorder
        .materialization_staging_path()
        .expect("deferred recorder keeps its staging path")
        .to_path_buf();
    fs::create_dir_all(staging_path.parent().expect("session parent"))?;
    fs::write(&staging_path, b"unowned-content")?;

    let error = recorder
        .persist()
        .await
        .expect_err("create-new collision must reach the caller");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(
        !recorder.materialization_pending(),
        "terminal writer failure must not remain deferred"
    );
    assert_eq!(
        fs::read(&staging_path)?,
        b"unowned-content",
        "failed creation must not claim or remove an unowned staging path"
    );
    assert!(!rollout_path.exists());
    Ok(())
}

#[tokio::test]
async fn final_path_collision_preserves_owner_and_cleans_own_staging() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;
    let rollout_path = recorder.rollout_path().to_path_buf();
    fs::create_dir_all(rollout_path.parent().expect("session parent"))?;
    fs::write(&rollout_path, b"existing-owner")?;

    let (reached, barrier_guard) = arm_materialization_barrier_for(&recorder);
    let persist_recorder = recorder.clone();
    let persist = tokio::spawn(async move { persist_recorder.persist().await });
    wait_materialization_reached(&reached).await;
    drop(barrier_guard);

    let error = tokio::time::timeout(Duration::from_secs(5), persist)
        .await
        .expect("collision publication must finish")
        .expect("persist task must not panic")
        .expect_err("collision must reach the caller");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(!recorder.materialization_pending());
    assert!(staging_paths(&rollout_path).is_empty());
    assert_eq!(fs::read(&rollout_path)?, b"existing-owner");
    Ok(())
}

#[tokio::test]
async fn recorder_materializes_only_after_explicit_persist() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;

    let rollout_path = recorder.rollout_path().to_path_buf();
    assert!(
        !rollout_path.exists(),
        "rollout file should not exist before first user message"
    );

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::AgentMessage(
            AgentMessageEvent {
                message: "buffered-event".to_string(),
                phase: None,
            },
        ))])
        .await?;
    recorder.flush().await?;
    assert!(
        !rollout_path.exists(),
        "rollout file should remain deferred before first user message"
    );

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "first-user-message".to_string(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await?;
    recorder.flush().await?;
    assert!(
        !rollout_path.exists(),
        "user-message-like items should not materialize without explicit persist"
    );

    recorder.persist().await?;
    // Second call verifies `persist()` is idempotent after materialization.
    recorder.persist().await?;
    assert!(rollout_path.exists(), "rollout file should be materialized");

    let text = std::fs::read_to_string(&rollout_path)?;
    assert!(
        text.contains("\"type\":\"session_meta\""),
        "expected session metadata in rollout"
    );
    let buffered_idx = text
        .find("buffered-event")
        .expect("buffered event in rollout");
    let user_idx = text
        .find("first-user-message")
        .expect("first user message in rollout");
    assert!(
        buffered_idx < user_idx,
        "buffered items should preserve ordering"
    );
    let text_after_second_persist = std::fs::read_to_string(&rollout_path)?;
    assert_eq!(text_after_second_persist, text);

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::AgentMessage(
            AgentMessageEvent {
                message: "event-after-materialization".to_string(),
                phase: None,
            },
        ))])
        .await?;
    tokio::time::timeout(Duration::from_secs(5), recorder.flush())
        .await
        .expect("materialized writer must acknowledge successful flush")?;
    let flushed_text = std::fs::read_to_string(&rollout_path)?;
    assert!(flushed_text.starts_with(&text));
    assert!(
        flushed_text.contains("event-after-materialization"),
        "successful flush must make the subsequent event readable"
    );

    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn metadata_irrelevant_events_touch_state_db_updated_at() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .enable(Feature::Sqlite)
        .expect("test config should allow sqlite");

    let state_db = StateRuntime::init(home.path().to_path_buf(), config.model_provider_id.clone())
        .await
        .expect("state db should initialize");
    state_db
        .mark_backfill_complete(None)
        .await
        .expect("backfill should be complete");

    let thread_id = ThreadId::new();
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Cli,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        Some(state_db.clone()),
        None,
    )
    .await?;

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "first-user-message".to_string(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await?;
    recorder.persist().await?;
    recorder.flush().await?;
    let initial_thread = state_db
        .get_thread(thread_id)
        .await
        .expect("thread should load")
        .expect("thread should exist");
    let initial_updated_at = initial_thread.updated_at;
    let initial_title = initial_thread.title.clone();
    let initial_first_user_message = initial_thread.first_user_message.clone();

    tokio::time::sleep(Duration::from_secs(1)).await;

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::AgentMessage(
            AgentMessageEvent {
                message: "assistant text".to_string(),
                phase: None,
            },
        ))])
        .await?;
    recorder.flush().await?;

    let updated_thread = state_db
        .get_thread(thread_id)
        .await
        .expect("thread should load after agent message")
        .expect("thread should still exist");

    assert!(updated_thread.updated_at > initial_updated_at);
    assert_eq!(updated_thread.title, initial_title);
    assert_eq!(
        updated_thread.first_user_message,
        initial_first_user_message
    );

    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn metadata_irrelevant_events_fall_back_to_upsert_when_thread_missing() -> std::io::Result<()>
{
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .enable(Feature::Sqlite)
        .expect("test config should allow sqlite");

    let state_db = StateRuntime::init(home.path().to_path_buf(), config.model_provider_id.clone())
        .await
        .expect("state db should initialize");
    let thread_id = ThreadId::new();
    let rollout_path = home.path().join("rollout.jsonl");
    let builder = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path.clone(),
        Utc::now(),
        SessionSource::Cli,
    );
    let items = vec![RolloutItem::EventMsg(EventMsg::AgentMessage(
        AgentMessageEvent {
            message: "assistant text".to_string(),
            phase: None,
        },
    ))];

    sync_thread_state_after_write(
        Some(state_db.as_ref()),
        rollout_path.as_path(),
        Some(&builder),
        items.as_slice(),
        config.model_provider_id.as_str(),
        None,
    )
    .await;

    let thread = state_db
        .get_thread(thread_id)
        .await
        .expect("thread should load after fallback")
        .expect("thread should be inserted after fallback");
    assert_eq!(thread.id, thread_id);

    Ok(())
}

#[tokio::test]
async fn list_threads_db_disabled_does_not_skip_paginated_items() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .disable(Feature::Sqlite)
        .expect("test config should allow sqlite to be disabled");

    let newest = write_session_file(home.path(), "2025-01-03T12-00-00", Uuid::from_u128(9001))?;
    let middle = write_session_file(home.path(), "2025-01-02T12-00-00", Uuid::from_u128(9002))?;
    let _oldest = write_session_file(home.path(), "2025-01-01T12-00-00", Uuid::from_u128(9003))?;

    let default_provider = config.model_provider_id.clone();
    let page1 = RolloutRecorder::list_threads(
        &config,
        1,
        None,
        ThreadSortKey::CreatedAt,
        &[],
        None,
        default_provider.as_str(),
        None,
    )
    .await?;
    assert_eq!(page1.items.len(), 1);
    assert_eq!(page1.items[0].path, newest);
    let cursor = page1.next_cursor.clone().expect("cursor should be present");

    let page2 = RolloutRecorder::list_threads(
        &config,
        1,
        Some(&cursor),
        ThreadSortKey::CreatedAt,
        &[],
        None,
        default_provider.as_str(),
        None,
    )
    .await?;
    assert_eq!(page2.items.len(), 1);
    assert_eq!(page2.items[0].path, middle);
    Ok(())
}

#[tokio::test]
async fn list_threads_db_enabled_drops_missing_rollout_paths() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .enable(Feature::Sqlite)
        .expect("test config should allow sqlite");

    let uuid = Uuid::from_u128(9010);
    let thread_id = ThreadId::from_string(&uuid.to_string()).expect("valid thread id");
    let stale_path = home.path().join(format!(
        "sessions/2099/01/01/rollout-2099-01-01T00-00-00-{uuid}.jsonl"
    ));

    let runtime =
        ctox_state::StateRuntime::init(home.path().to_path_buf(), config.model_provider_id.clone())
            .await
            .expect("state db should initialize");
    runtime
        .mark_backfill_complete(None)
        .await
        .expect("backfill should be complete");
    let created_at = chrono::Utc
        .with_ymd_and_hms(2025, 1, 3, 13, 0, 0)
        .single()
        .expect("valid datetime");
    let mut builder = ctox_state::ThreadMetadataBuilder::new(
        thread_id,
        stale_path,
        created_at,
        SessionSource::Cli,
    );
    builder.model_provider = Some(config.model_provider_id.clone());
    builder.cwd = home.path().to_path_buf();
    let mut metadata = builder.build(config.model_provider_id.as_str());
    metadata.first_user_message = Some("Hello from user".to_string());
    runtime
        .upsert_thread(&metadata)
        .await
        .expect("state db upsert should succeed");

    let default_provider = config.model_provider_id.clone();
    let page = RolloutRecorder::list_threads(
        &config,
        10,
        None,
        ThreadSortKey::CreatedAt,
        &[],
        None,
        default_provider.as_str(),
        None,
    )
    .await?;
    assert_eq!(page.items.len(), 0);
    let stored_path = runtime
        .find_rollout_path_by_id(thread_id, Some(false))
        .await
        .expect("state db lookup should succeed");
    assert_eq!(stored_path, None);
    Ok(())
}

#[tokio::test]
async fn list_threads_db_enabled_repairs_stale_rollout_paths() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let mut config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    config
        .features
        .enable(Feature::Sqlite)
        .expect("test config should allow sqlite");

    let uuid = Uuid::from_u128(9011);
    let thread_id = ThreadId::from_string(&uuid.to_string()).expect("valid thread id");
    let real_path = write_session_file(home.path(), "2025-01-03T13-00-00", uuid)?;
    let stale_path = home.path().join(format!(
        "sessions/2099/01/01/rollout-2099-01-01T00-00-00-{uuid}.jsonl"
    ));

    let runtime =
        ctox_state::StateRuntime::init(home.path().to_path_buf(), config.model_provider_id.clone())
            .await
            .expect("state db should initialize");
    runtime
        .mark_backfill_complete(None)
        .await
        .expect("backfill should be complete");
    let created_at = chrono::Utc
        .with_ymd_and_hms(2025, 1, 3, 13, 0, 0)
        .single()
        .expect("valid datetime");
    let mut builder = ctox_state::ThreadMetadataBuilder::new(
        thread_id,
        stale_path,
        created_at,
        SessionSource::Cli,
    );
    builder.model_provider = Some(config.model_provider_id.clone());
    builder.cwd = home.path().to_path_buf();
    let mut metadata = builder.build(config.model_provider_id.as_str());
    metadata.first_user_message = Some("Hello from user".to_string());
    runtime
        .upsert_thread(&metadata)
        .await
        .expect("state db upsert should succeed");

    let default_provider = config.model_provider_id.clone();
    let page = RolloutRecorder::list_threads(
        &config,
        1,
        None,
        ThreadSortKey::CreatedAt,
        &[],
        None,
        default_provider.as_str(),
        None,
    )
    .await?;
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].path, real_path);

    let repaired_path = runtime
        .find_rollout_path_by_id(thread_id, Some(false))
        .await
        .expect("state db lookup should succeed");
    assert_eq!(repaired_path, Some(real_path));
    Ok(())
}

#[tokio::test]
async fn resume_candidate_matches_cwd_reads_latest_turn_context() -> std::io::Result<()> {
    let home = TempDir::new().expect("temp dir");
    let stale_cwd = home.path().join("stale");
    let latest_cwd = home.path().join("latest");
    fs::create_dir_all(&stale_cwd)?;
    fs::create_dir_all(&latest_cwd)?;

    let path = write_session_file(home.path(), "2025-01-03T13-00-00", Uuid::from_u128(9012))?;
    let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
    let turn_context = RolloutLine {
        timestamp: "2025-01-03T13:00:01Z".to_string(),
        item: RolloutItem::TurnContext(TurnContextItem {
            turn_id: Some("turn-1".to_string()),
            trace_id: None,
            cwd: latest_cwd.clone(),
            current_date: None,
            timezone: None,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            network: None,
            model: "test-model".to_string(),
            personality: None,
            collaboration_mode: None,
            realtime_active: None,
            effort: None,
            summary: ReasoningSummaryConfig::Auto,
            user_instructions: None,
            developer_instructions: None,
            final_output_json_schema: None,
            truncation_policy: None,
        }),
    };
    writeln!(file, "{}", serde_json::to_string(&turn_context)?)?;

    assert!(
        resume_candidate_matches_cwd(
            path.as_path(),
            Some(stale_cwd.as_path()),
            latest_cwd.as_path(),
            "test-provider",
        )
        .await
    );
    Ok(())
}
