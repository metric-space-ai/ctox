// Origin: CTOX
// License: Apache-2.0

use super::rxdb_peer::{
    incremental_upsert_projection_if_changed, now_ms, projection_sleep_secs,
    upsert_business_record_projection, BUSINESS_COMMAND_ACTIVE_POLL_SECS,
    BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS, BUSINESS_COMMAND_IDLE_POLL_SECS,
    NATIVE_RXDB_WRITE_LOCK,
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
    let mut idle_rounds = 0u32;
    loop {
        let result: anyhow::Result<usize> = async {
            if idle_rounds > 0 && idle_rounds % 60 == 0 {
                let reconcile_root = root.clone();
                tokio::task::spawn_blocking(move || {
                    crate::mission::channels::audit_and_migrate_business_command_storage(
                        &reconcile_root,
                        true,
                    )
                })
                .await
                .context("join business command invariant reconciliation")??;
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
                tokio::time::sleep(Duration::from_secs(projection_sleep_secs(
                    BUSINESS_COMMAND_ACTIVE_POLL_SECS,
                    BUSINESS_COMMAND_IDLE_POLL_SECS,
                    BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS,
                    idle_rounds,
                )))
                .await;
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
mod tests {}
