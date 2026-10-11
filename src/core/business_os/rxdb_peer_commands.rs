// Origin: CTOX
// License: Apache-2.0

use super::rxdb_peer::{
    incremental_upsert_projection_if_changed, now_ms, upsert_business_record_projection,
    BUSINESS_COMMAND_ACTIVE_POLL_SECS, BUSINESS_COMMAND_IDLE_POLL_SECS, NATIVE_RXDB_WRITE_LOCK,
};
use super::rxdb_peer_projections::{
    appsec_projection_collection, support_projection_collection, threads_projection_collection,
};
use super::store;
use anyhow::Context;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn deliver_business_command_outbox_background_loop(root: PathBuf) {
    deliver_business_command_outbox_with_idle_delay(
        root,
        Duration::from_secs(BUSINESS_COMMAND_ACTIVE_POLL_SECS),
    )
    .await;
}

async fn deliver_business_command_outbox_with_idle_delay(root: PathBuf, idle_delay: Duration) {
    let mut idle_rounds = 0u32;
    let mut last_invariant_sweep = std::time::Instant::now();
    let invariant_sweep_interval = Duration::from_secs(60 * BUSINESS_COMMAND_IDLE_POLL_SECS);
    loop {
        let result: anyhow::Result<usize> = async {
            // Keep the former roughly thirty-minute reconciliation cadence;
            // faster receipt polling must not multiply expensive sweeps.
            if idle_rounds > 0 && last_invariant_sweep.elapsed() >= invariant_sweep_interval {
                let reconcile_root = root.clone();
                tokio::task::spawn_blocking(move || {
                    crate::mission::channels::audit_and_migrate_business_command_storage(
                        &reconcile_root,
                        true,
                    )
                })
                .await
                .context("join business command invariant reconciliation")??;
                last_invariant_sweep = std::time::Instant::now();
            }
            let outbox_root = root.clone();
            let outbox = tokio::task::spawn_blocking(move || {
                store::deliver_business_command_outbox(&outbox_root, 10)
            })
            .await
            .context("join business command outbox delivery")??;
            Ok(outbox
                .get("processed")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize)
        }
        .await;
        match result {
            Ok(0) => {
                idle_rounds = idle_rounds.saturating_add(1);
                // Unlike command intake, canonical outbox append has no table
                // notifier here. A thirty-second idle sleep delays newly
                // terminal receipts and their local RxDB mirrors by that much.
                tokio::time::sleep(idle_delay).await;
            }
            Ok(_) => {
                idle_rounds = 0;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(err) => {
                idle_rounds = 0;
                eprintln!("[business-os] native business command outbox delivery failed: {err:#}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub(super) async fn enqueue_business_command_document_with_database(
    database: &Arc<RxDatabase>,
    mut document: Value,
) -> anyhow::Result<Value> {
    let Some(object) = document.as_object_mut() else {
        anyhow::bail!("business command document must be an object");
    };
    let command_id = object
        .get("command_id")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("business_command_{}", Uuid::new_v4().simple()));
    let now = now_ms() as u64;
    object.insert("id".to_string(), Value::String(command_id.clone()));
    object.insert("command_id".to_string(), Value::String(command_id.clone()));
    object
        .entry("status".to_string())
        .or_insert_with(|| Value::String("pending_sync".to_string()));
    object
        .entry("created_at_ms".to_string())
        .or_insert_with(|| Value::from(now));
    object.insert("updated_at_ms".to_string(), Value::from(now));

    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    incremental_upsert_document_with_envelope(
        &commands,
        document.clone(),
        &format!("enqueued business_command {command_id}"),
    )
    .await
    .map_err(|err| anyhow::anyhow!("enqueue business command {command_id}: {err}"))?;
    Ok(document)
}

/// Schreibt nur, wenn sich der Inhalt wirklich unterscheidet. Ohne diesen
/// Umweg stempelt jeder Replay denselben Envelope neu und hebt allein
/// `_rev`/`updated_at_ms` — auf einer Kundeninstanz waren das 93 Revisionen je
/// Minute auf sechs unveraenderten Dokumenten und eine 2,29-GB-Store-Datei.
pub(super) async fn incremental_upsert_document_with_envelope(
    collection: &Arc<RxCollection>,
    document: Value,
    label: &str,
) -> anyhow::Result<()> {
    incremental_upsert_projection_if_changed(collection, document, label)
        .await
        .map(|_| ())
}

pub(super) fn command_id_from_document(document: &Value) -> anyhow::Result<String> {
    document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("business command id is required")
}

pub(super) fn typed_app_action_error_code(message: &str) -> Option<&'static str> {
    [
        "app_action_not_registered",
        "app_action_input_invalid",
        "app_action_permission_denied",
        "app_action_definition_changed",
        "app_runtime_reconfiguring",
        "app_action_compensation_failed",
    ]
    .into_iter()
    .find(|code| message.contains(code))
}

pub(super) async fn project_support_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .get("result")
        .and_then(|result| result.get("projections"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(support_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}

pub(super) async fn project_threads_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .get("result")
        .and_then(|result| result.get("projections"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(threads_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}

pub(super) async fn project_appsec_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .pointer("/result/ctox_durable_projection/business_os_projection/projected_records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(appsec_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::channels;
    use rusqlite::{params, Connection, OpenFlags};
    use serde_json::json;
    use std::time::Instant;

    fn seed_terminal(root: &std::path::Path, id: &str) -> anyhow::Result<()> {
        channels::claim_business_control_command(
            root,
            channels::BusinessCommandClaimRequest {
                command_id: id.to_owned(),
                idempotency_key: id.to_owned(),
                payload_hash: format!("sha256:{id}"),
                module: "ctox".to_owned(),
                command_type: "ctox.workjet.project.list".to_owned(),
                record_id: String::new(),
                intent: json!({"command_id": id, "command_type": "ctox.workjet.project.list"}),
                created_at_ms: now_ms() as i64,
            },
        )?;
        channels::complete_business_control_command(
            root,
            id,
            "completed",
            &json!({"ok": true, "count": 0}),
            None,
        )?;
        Ok(())
    }

    fn delivered_count(root: &std::path::Path, id: &str) -> anyhow::Result<i64> {
        let path = crate::paths::core_db(root);
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM business_command_outbox
             WHERE command_id = ?1 AND status = 'delivered'",
            params![id],
            |row| row.get(0),
        )?)
    }

    async fn measure_idle_delivery(original_cadence: bool) -> anyhow::Result<f64> {
        let root = tempfile::tempdir()?;
        // Initialize both actual native mirror destinations, then drain them.
        seed_terminal(root.path(), "outbox-warmup")?;
        while store::deliver_business_command_outbox(root.path(), 10)?["processed"]
            .as_u64()
            .unwrap_or(0)
            > 0
        {}
        assert_eq!(delivered_count(root.path(), "outbox-warmup")?, 4);

        let fixture_root = root.path().to_path_buf();
        let consumer = tokio::spawn(async move {
            if original_cadence {
                deliver_business_command_outbox_with_idle_delay(
                    fixture_root,
                    Duration::from_secs(BUSINESS_COMMAND_IDLE_POLL_SECS),
                )
                .await;
            } else {
                deliver_business_command_outbox_background_loop(fixture_root).await;
            }
        });
        // This fixture is idle and initialized before the new receipt arrives.
        tokio::time::sleep(Duration::from_millis(250)).await;
        let started = Instant::now();
        seed_terminal(root.path(), "outbox-after-idle")?;
        let result = tokio::time::timeout(Duration::from_secs(35), async {
            while delivered_count(root.path(), "outbox-after-idle")? < 4 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            anyhow::Ok(())
        })
        .await;
        consumer.abort();
        let _ = consumer.await;
        result??;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "outbox_idle_delivery_timing={}",
            json!({
                "original_idle_cadence": original_cadence,
                "elapsed_ms": elapsed_ms, "delivered_events": 4,
                "destinations": ["business-os", "rxdb"],
            })
        );
        assert_eq!(
            channels::inspect_business_command(root.path(), "outbox-after-idle")?
                .expect("retained native command")["command"]["terminal_status"],
            "completed"
        );
        Ok(elapsed_ms)
    }

    #[tokio::test]
    async fn idle_outbox_delivers_new_terminal_without_a_thirty_second_wait() -> anyhow::Result<()>
    {
        // Same actual SQLite destinations and consumer, with the old idle
        // cadence for the baseline and the production entry point for the fix.
        let before_ms = measure_idle_delivery(true).await?;
        let after_ms = measure_idle_delivery(false).await?;
        assert!(
            before_ms > 25_000.0,
            "baseline must reach the original idle sleep"
        );
        assert!(
            after_ms < 2000.0,
            "a newly terminal command waited {after_ms:.1} ms behind the idle outbox poll"
        );
        Ok(())
    }
}
