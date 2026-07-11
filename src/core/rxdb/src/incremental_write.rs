//! Port of `src/incremental-write.ts`.
//!
//! T2 deviations (RxJS Promise → tokio):
//! - `resolve/reject` callbacks become `tokio::sync::oneshot::Sender`.
//! - The shared mutable `queueByDocId` + `isRunning` become a `tokio::sync::Mutex<State>`.
//! - `Promise.all(map(...))` in trigger_run becomes a sequential `for` loop
//!   over docIds. Per-docId modifier loops were sequential in upstream
//!   (`for (const item of items) await ...`), so the outer parallelism was
//!   just an idiomatic JS quirk; collapsing it to sequential preserves
//!   semantics for the single-storage write call.
//! - Modifier callbacks are reusable `Arc<dyn Fn(Value) -> ...>` values so a
//!   409 retry reapplies the original mutation to the latest document state.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::{oneshot, Mutex};

use crate::plugins::utils::utils_object::clone_deep;
use crate::plugins::utils::utils_revision::get_height_of_revision;
use crate::rx_error::{rx_storage_write_error_to_rx_error, RxResult};
use crate::rx_storage_helper::get_written_documents_from_bulk_write_response;
use crate::types::{BulkWriteRow, RxStorageBulkWriteResponse, RxStorageInstance};

/// Modifier callback: takes a cloned doc, returns a future of a (possibly
/// modified) doc.
pub type IncrementalWriteModifier =
    Arc<dyn Fn(Value) -> BoxFuture<'static, RxResult<Value>> + Send + Sync>;

/// Pre-write hook: called once per docId with `(new_data, old_data)`.
/// It returns the possibly mutated document data, mirroring JS hooks that
/// mutate the passed object before the storage write.
pub type PreWriteFn =
    Arc<dyn Fn(Value, Value) -> BoxFuture<'static, RxResult<Value>> + Send + Sync>;

/// Post-write hook: called once per successfully written doc.
pub type PostWriteFn = Arc<dyn Fn(Value) -> BoxFuture<'static, RxResult<()>> + Send + Sync>;

struct QueueItem {
    last_known_document_state: Value,
    modifier: IncrementalWriteModifier,
    /// `None` once the sender has been consumed (either resolved or rejected).
    sender: Option<oneshot::Sender<RxResult<Value>>>,
}

struct State {
    queue_by_doc_id: HashMap<String, Vec<QueueItem>>,
    is_running: bool,
}

// ref: rxdb/src/incremental-write.ts:48-186
pub struct IncrementalWriteQueue {
    storage_instance: Arc<dyn RxStorageInstance>,
    primary_path: String,
    pre_write: PreWriteFn,
    post_write: PostWriteFn,
    state: Mutex<State>,
}

impl IncrementalWriteQueue {
    // ref: rxdb/src/incremental-write.ts:52-59
    pub fn new(
        storage_instance: Arc<dyn RxStorageInstance>,
        primary_path: String,
        pre_write: PreWriteFn,
        post_write: PostWriteFn,
    ) -> Arc<Self> {
        Arc::new(Self {
            storage_instance,
            primary_path,
            pre_write,
            post_write,
            state: Mutex::new(State {
                queue_by_doc_id: HashMap::new(),
                is_running: false,
            }),
        })
    }

    // ref: rxdb/src/incremental-write.ts:61-78
    pub async fn add_write(
        self: &Arc<Self>,
        last_known_document_state: Value,
        modifier: IncrementalWriteModifier,
    ) -> RxResult<Value> {
        let (tx, rx) = oneshot::channel::<RxResult<Value>>();
        let doc_id = last_known_document_state
            .get(&self.primary_path)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        {
            let mut state = self.state.lock().await;
            let ar = state.queue_by_doc_id.entry(doc_id).or_default();
            ar.push(QueueItem {
                last_known_document_state,
                modifier,
                sender: Some(tx),
            });
        }
        // Run on a separate task so that add_write returns to the caller
        // immediately. Multiple concurrent add_write calls all hit the
        // is_running guard inside trigger_run.
        let me = Arc::clone(self);
        tokio::spawn(async move {
            me.trigger_run().await;
        });
        match rx.await {
            Ok(r) => r,
            Err(_) => Err(crate::rx_error::new_rx_error(
                "STO15",
                Some(serde_json::json!({
                    "message": "incremental write was dropped before resolving",
                })),
            )),
        }
    }

    // ref: rxdb/src/incremental-write.ts:80-185
    pub fn trigger_run<'a>(
        self: &'a Arc<Self>,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // ref: rxdb/src/incremental-write.ts:82-89
            let mut state_guard = self.state.lock().await;
            if state_guard.is_running || state_guard.queue_by_doc_id.is_empty() {
                return;
            }
            state_guard.is_running = true;
            // ref: rxdb/src/incremental-write.ts:95-96
            // 'take over' so that while the async function runs,
            // new incremental updates could be added from the outside.
            let items_by_id = std::mem::take(&mut state_guard.queue_by_doc_id);
            drop(state_guard);

            let mut write_rows: Vec<BulkWriteRow> = Vec::new();
            // Collect items that should receive results post-bulk-write.
            // For docIds where pre_write failed we already rejected; they are
            // not added to write_rows nor kept in items_by_id_for_result.
            let mut items_by_id_for_result: HashMap<String, Vec<QueueItem>> = HashMap::new();

            // ref: rxdb/src/incremental-write.ts:97-137
            for (doc_id, mut items) in items_by_id.into_iter() {
                let old_data = find_newest_of_document_states(
                    items
                        .iter()
                        .map(|i| i.last_known_document_state.clone())
                        .collect(),
                );
                let mut new_data = old_data.clone();
                for item in items.iter_mut() {
                    let cloned = clone_deep(&new_data);
                    match (item.modifier)(cloned).await {
                        Ok(updated) => {
                            new_data = updated;
                        }
                        Err(err) => {
                            // Reject this item and mark it as resolved so the
                            // success/error distribution below skips it.
                            if let Some(tx) = item.sender.take() {
                                let _ = tx.send(Err(err));
                            }
                        }
                    }
                }
                // ref: rxdb/src/incremental-write.ts:121-131
                let pre_write_result = (self.pre_write)(new_data.clone(), old_data.clone()).await;
                let new_data = match pre_write_result {
                    Ok(new_data) => new_data,
                    Err(err) => {
                        // Reject all items for this docId.
                        for item in items.iter_mut() {
                            if let Some(tx) = item.sender.take() {
                                let _ = tx.send(Err(err.clone()));
                            }
                        }
                        continue;
                    }
                };
                write_rows.push(BulkWriteRow {
                    previous: Some(old_data),
                    document: new_data,
                });
                items_by_id_for_result.insert(doc_id, items);
            }

            // ref: rxdb/src/incremental-write.ts:138-141
            let write_result: RxStorageBulkWriteResponse = if !write_rows.is_empty() {
                match self
                    .storage_instance
                    .bulk_write(write_rows.clone(), "incremental-write")
                    .await
                {
                    Ok(r) => r,
                    Err(err) => {
                        // The storage call itself failed: reject all remaining items.
                        for (_doc_id, items) in items_by_id_for_result.iter_mut() {
                            for item in items.iter_mut() {
                                if let Some(tx) = item.sender.take() {
                                    let _ = tx.send(Err(err.clone()));
                                }
                            }
                        }
                        self.set_running(false).await;
                        return;
                    }
                }
            } else {
                RxStorageBulkWriteResponse::default()
            };

            // ref: rxdb/src/incremental-write.ts:143-150
            // process success
            let success = get_written_documents_from_bulk_write_response(
                &self.primary_path,
                &write_rows,
                &write_result,
                None,
            );
            for doc in success.iter() {
                let id = doc
                    .get(&self.primary_path)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(items) = items_by_id_for_result.get_mut(id) {
                    let post_write_result = (self.post_write)(doc.clone()).await;
                    for item in items.iter_mut() {
                        if let Some(tx) = item.sender.take() {
                            let _ = tx.send(post_write_result.clone().map(|_| doc.clone()));
                        }
                    }
                }
            }

            // ref: rxdb/src/incremental-write.ts:152-176
            // process errors
            let mut requeue: HashMap<String, Vec<QueueItem>> = HashMap::new();
            for error in write_result.error.iter() {
                let id = error.document_id.clone();
                let Some(mut items) = items_by_id_for_result.remove(&id) else {
                    continue;
                };
                if error.status == 409 {
                    // had conflict -> retry afterwards. Reverse to maintain
                    // original order when unshifted onto a fresh queue.
                    items.reverse();
                    let in_db = error.document_in_db.clone().unwrap_or(Value::Null);
                    let entry = requeue.entry(id).or_default();
                    for mut item in items.into_iter() {
                        item.last_known_document_state = in_db.clone();
                        entry.insert(0, item);
                    }
                } else {
                    let rx_err = rx_storage_write_error_to_rx_error(
                        &serde_json::to_value(error).unwrap_or(Value::Null),
                    );
                    for item in items.iter_mut() {
                        if let Some(tx) = item.sender.take() {
                            let _ = tx.send(Err(rx_err.clone()));
                        }
                    }
                }
            }

            // Merge requeues back into the live state.
            {
                let mut state = self.state.lock().await;
                for (id, items) in requeue.into_iter() {
                    let entry = state.queue_by_doc_id.entry(id).or_default();
                    // Items in `items` are already in the desired front-order;
                    // we splice them in at index 0.
                    let mut combined = items;
                    combined.append(entry);
                    *entry = combined;
                }
                state.is_running = false;
            }

            // ref: rxdb/src/incremental-write.ts:178-184
            // Always trigger another run because in between there might be
            // new items added to the queue.
            self.trigger_run().await;
        })
    }

    async fn set_running(&self, value: bool) {
        let mut s = self.state.lock().await;
        s.is_running = value;
    }
}

// ref: rxdb/src/incremental-write.ts:189-210
/// Convert a public modifier (operates on the document without RxDB meta) to
/// an internal modifier (re-attaches `_meta`, `_attachments`, `_rev`, `_deleted`).
pub fn modifier_from_public_to_internal(
    public_modifier: crate::rx_document::ModifyFunction,
) -> IncrementalWriteModifier {
    let public_modifier: Arc<dyn Fn(Value) -> BoxFuture<'static, RxResult<Value>> + Send + Sync> =
        Arc::from(public_modifier);
    Arc::new(move |doc_data: Value| {
        let public_modifier = Arc::clone(&public_modifier);
        Box::pin(async move {
            // Strip meta fields.
            let mut without_meta = doc_data.clone();
            if let Some(obj) = without_meta.as_object_mut() {
                obj.remove("_meta");
                obj.remove("_rev");
                obj.remove("_attachments");
                // Keep _deleted (upstream re-adds it via `withoutMeta._deleted = docData._deleted`).
            }
            let modified = public_modifier(without_meta).await?;

            // Re-attach meta from the original doc.
            let mut out = modified;
            if let Some(obj) = out.as_object_mut() {
                if let Some(orig) = doc_data.as_object() {
                    if let Some(m) = orig.get("_meta").cloned() {
                        obj.insert("_meta".to_string(), m);
                    }
                    if let Some(a) = orig.get("_attachments").cloned() {
                        obj.insert("_attachments".to_string(), a);
                    }
                    if let Some(r) = orig.get("_rev").cloned() {
                        obj.insert("_rev".to_string(), r);
                    }
                    // _deleted: prefer modified, fall back to original, default false.
                    if !obj.contains_key("_deleted") {
                        let d = orig.get("_deleted").cloned().unwrap_or(Value::Bool(false));
                        obj.insert("_deleted".to_string(), d);
                    }
                }
                if obj.get("_deleted").is_none() {
                    obj.insert("_deleted".to_string(), Value::Bool(false));
                }
            }
            Ok(out)
        })
    })
}

// ref: rxdb/src/incremental-write.ts:213-227
/// Of all known document states, return the one with the highest revision height.
pub fn find_newest_of_document_states(docs: Vec<Value>) -> Value {
    if docs.is_empty() {
        return Value::Null;
    }
    let mut newest = docs[0].clone();
    let mut newest_height = docs[0]
        .get("_rev")
        .and_then(|v| v.as_str())
        .and_then(|s| get_height_of_revision(s).ok())
        .unwrap_or(0);
    for doc in docs.into_iter().skip(1) {
        let h = doc
            .get("_rev")
            .and_then(|v| v.as_str())
            .and_then(|s| get_height_of_revision(s).ok())
            .unwrap_or(0);
        if h > newest_height {
            newest = doc;
            newest_height = h;
        }
    }
    newest
}

// `RxError` already derives `Clone`; no extra trait needed.
