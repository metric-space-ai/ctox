//! Backlog OS-A1: the measurable form of the strategy rule "idle must stay
//! idle" (docs/ctox-os-framework-strategy.md). With no external writes and no
//! table-change notifications, the SQLite external write poll must go quiet:
//! the 1s active window backs off to the 30-minute standby interval after 3
//! idle reads, so once settled, an idle observation window must add ZERO
//! wakeups (and therefore zero SQLite statements) for this database.
//!
//! This is a dedicated integration test binary ON PURPOSE: it keeps a live
//! poll thread running for several seconds, and inside the shared lib-test
//! process its connection opens inflate the exact global-counter deltas other
//! unit tests assert. A separate binary gets its own counter statics, and the
//! per-database wakeup counter keys the measurement to THIS database only.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rxdb::storage::sqlite::index_mod::get_rx_storage_sqlite;
use rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot;
use rxdb::storage::sqlite::types::RxStorageSqliteSettings;
use rxdb::types::{RxJsonSchema, RxStorage, RxStorageInstanceCreationParams};

fn idle_schema() -> RxJsonSchema {
    serde_json::from_value(serde_json::json!({
        "version": 0,
        "primaryKey": "id",
        "type": "object",
        "properties": {
            "id": { "type": "string", "maxLength": 64 },
            "value": { "type": "number" }
        },
        "required": ["id"]
    }))
    .expect("idle-budget test schema must deserialize")
}

fn wakeups_for_database(map_key_fragment: &str) -> u64 {
    let snapshot = sqlite_runtime_counters_snapshot();
    let Some(map) = snapshot
        .get("external_poll_wakeups_by_database")
        .and_then(|value| value.as_object())
    else {
        return 0;
    };
    map.iter()
        .filter(|(key, _)| key.contains(map_key_fragment))
        .map(|(_, value)| value.as_u64().unwrap_or(0))
        .sum()
}

#[tokio::test]
async fn external_write_poll_idle_budget_is_zero_after_backoff() {
    let dir = tempfile::tempdir().unwrap();
    let database_path = dir.path().join("ctox-idle-budget.sqlite3");
    let storage: Arc<dyn RxStorage> = get_rx_storage_sqlite(RxStorageSqliteSettings {
        database_path: database_path.clone(),
    });
    let instance = storage
        .create_storage_instance(RxStorageInstanceCreationParams {
            database_instance_token: "token-idle-budget".to_string(),
            database_name: "idle-budget".to_string(),
            collection_name: "idle_budget_records".to_string(),
            schema: idle_schema(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: true,
            password: None,
        })
        .await
        .expect("sqlite storage instance");
    // Keep a change stream alive so the external poll thread runs.
    let _stream = instance.change_stream();
    // The counter map is keyed by the canonical database key; matching on the
    // unique file name fragment avoids depending on the key derivation.
    let key_fragment = "ctox-idle-budget";

    // Settle: 3 idle reads at the 1s active interval enter standby. Standby
    // is detected as the wakeup counter staying flat for 2s of consecutive
    // samples — strictly longer than the 1s active interval, so a flat
    // window can only mean the poll left the active phase. Generous deadline
    // so a loaded test machine still gets there.
    let settle_deadline = Instant::now() + Duration::from_secs(30);
    let mut settled = false;
    let mut last_count = 0u64;
    let mut flat_samples = 0u32;
    while Instant::now() < settle_deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let now = wakeups_for_database(key_fragment);
        if now > 0 && now == last_count {
            flat_samples += 1;
            if flat_samples >= 4 {
                settled = true;
                break;
            }
        } else {
            flat_samples = 0;
        }
        last_count = now;
    }
    assert!(
        settled,
        "external poll never settled into standby within 30s — the idle \
         backoff (3 idle reads -> 30min standby) regressed"
    );

    // Idle budget: a 3s window in standby must add ZERO wakeups (the standby
    // interval is 30 minutes; a notify-driven wake needs a real table change,
    // and none happens here).
    let before = wakeups_for_database(key_fragment);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = wakeups_for_database(key_fragment);
    assert_eq!(
        after - before,
        0,
        "idle must stay idle: the external poll woke {}x during a 3s idle \
         window after entering standby",
        after - before
    );
}
