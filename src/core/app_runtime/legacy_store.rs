//! Transitional adapter to the Core's existing RxCollection writer.
//! The v4 Store can replace this without changing the Node ctx protocol.
use crate::{error_code, RuntimeHost};
use anyhow::{bail, ensure, Context, Result};
use rxdb::incremental_write::IncrementalWriteModifier;
use rxdb::rx_database::RxDatabase;
use rxdb::rx_error::{new_rx_error, RxError};
use rxdb::rx_query::QueryExecutionResult;
use rxdb::types::MangoQuery;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::{Handle, RuntimeFlavor};

/// Implemented by Core authority, never by an app. No permissive defaults:
/// reads must apply row visibility/field masks; writes must preserve protected
/// fields and enforce schema/role policy against the current stored document.
pub trait LegacyStorePolicy: RuntimeHost {
    fn project_read(
        &self,
        app: &str,
        actor: &Value,
        collection: &str,
        document: &Value,
    ) -> Result<Option<Value>>;
    fn prepare_write(
        &self,
        app: &str,
        actor: &Value,
        collection: &str,
        before: Option<&Value>,
        proposed: Option<Value>,
    ) -> Result<Option<Value>>;
}
pub struct LegacyStoreAdapter {
    database: Arc<RxDatabase>,
    handle: Handle,
    authority: Arc<dyn LegacyStorePolicy>,
}
impl LegacyStoreAdapter {
    /// Reuses the running Core database and Tokio runtime; creates no database,
    /// writer service, replica, network connection or executor of its own.
    pub fn new(
        database: Arc<RxDatabase>,
        handle: Handle,
        authority: Arc<dyn LegacyStorePolicy>,
    ) -> Result<Self> {
        ensure!(
            matches!(handle.runtime_flavor(), RuntimeFlavor::MultiThread),
            "unavailable: a multi-thread Core runtime is required"
        );
        Ok(Self {
            database,
            handle,
            authority,
        })
    }
    fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        if let Ok(current) = Handle::try_current() {
            ensure!(
                matches!(current.runtime_flavor(), RuntimeFlavor::MultiThread),
                "unavailable: invoke app handlers outside a single-thread async executor"
            );
            tokio::task::block_in_place(|| self.handle.block_on(future))
        } else {
            self.handle.block_on(future)
        }
    }
    async fn read(&self, app: &str, actor: &Value, args: &Value) -> Result<Value> {
        let name = args[0].as_str().context("invalid: collection name")?;
        let collection = self
            .database
            .collection(name)
            .context("unavailable: collection is not registered")?;
        ensure!(
            collection.primary_path().as_deref() == Some("id"),
            "unavailable: the App API requires primary key id"
        );
        // No implicit limit. Only authorized/masked rows cross the IPC boundary;
        // JS Mango evaluation then preserves the exact JS regex/sort semantics.
        let records = collection
            .find(Some(MangoQuery::default()))
            .map_err(store_error)?
            .exec_over_database()
            .await
            .map_err(store_error)?;
        let QueryExecutionResult::Documents(records) = records else {
            bail!("unavailable: native query did not return rows");
        };
        let mut projected = Vec::new();
        let mut bytes = 0usize;
        for original in &records {
            if let Some(mut row) = self.authority.project_read(app, actor, name, original)? {
                let object = row
                    .as_object_mut()
                    .context("invalid: Core field mask must return a document")?;
                object.insert("id".into(), original["id"].clone());
                for key in ["_rev", "_meta", "_attachments", "_deleted"] {
                    object.remove(key);
                }
                if let Some(rev) = original["_rev"].as_str() {
                    object.insert("rev".into(), json!(rev));
                }
                bytes += serde_json::to_vec(&row)?.len();
                ensure!(bytes <= 6 * 1024 * 1024, "quota: legacy authorized snapshot exceeds IPC budget; use the v4 Store query adapter");
                projected.push(row);
            }
        }
        self.authority
            .authorize(app, actor, &json!({"operation":"read","args":args}))?;
        Ok(json!({"kind":"ctox.authorized_snapshot.v1","documents":projected}))
    }
    async fn write(&self, app: &str, actor: &Value, args: &Value) -> Result<Value> {
        let name = args[0].as_str().context("invalid: collection name")?;
        let op = args[1].as_object().context("invalid: write operation")?;
        let kind = op.get("type").and_then(Value::as_str).unwrap_or("");
        ensure!(
            matches!(kind, "put" | "patch" | "remove"),
            "invalid: write type"
        );
        let id = op
            .get("id")
            .or_else(|| op.get("doc").and_then(|doc| doc.get("id")))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .context("invalid: document id")?;
        if let Some(base) = op.get("base_rev") {
            ensure!(
                base.is_null() || base.is_string(),
                "invalid: base_rev must be string or null"
            );
        }
        let collection = self
            .database
            .collection(name)
            .context("unavailable: collection is not registered")?;
        ensure!(
            collection.primary_path().as_deref() == Some("id"),
            "unavailable: the App API requires primary key id"
        );
        // Read committed state; reactive query events may lag the preceding call.
        let current = collection
            .storage_instance
            .find_documents_by_id(&[id.to_owned()], false)
            .await
            .map_err(store_error)?
            .pop()
            .unwrap_or(Value::Null);
        if current.is_null() {
            ensure!(kind == "put", "not_found: write target is unavailable");
            ensure!(
                op.get("base_rev").is_none_or(Value::is_null),
                "conflict: source revision differs"
            );
            let proposed =
                clean_input(op.get("doc").cloned().context("invalid: put document")?, id)?;
            let value = self
                .authority
                .prepare_write(app, actor, name, None, Some(proposed))?
                .context("forbidden: Core policy denied create")?;
            ensure!(
                value["id"].as_str() == Some(id),
                "invalid: Core write policy changed identity"
            );
            self.authority
                .authorize(app, actor, &json!({"operation":"write","args":args}))?;
            let created = collection.insert(value).await.map_err(store_error)?;
            return Ok(json!({"status":"accepted","id":id,"rev":created.revision()}));
        }
        // add_write runs through the SAME conflict/revision queue as
        // RxDocument::incremental_modify; the precondition is rechecked on
        // every native 409 retry, against the latest stored revision.
        let id = id.to_owned();
        let app = app.to_owned();
        let actor = actor.clone();
        let name = name.to_owned();
        let args = args.clone();
        let operation = Value::Object(op.clone());
        let authority = self.authority.clone();
        let database_token = self.database.token.clone();
        let modifier: IncrementalWriteModifier = Arc::new(move |before| {
            let (id, app, actor, name, args, operation, authority) = (
                id.clone(),
                app.clone(),
                actor.clone(),
                name.clone(),
                args.clone(),
                operation.clone(),
                authority.clone(),
            );
            let database_token = database_token.clone();
            Box::pin(async move {
                let apply = || -> Result<Value> {
                    if let Some(expected) = operation.get("base_rev") {
                        ensure!(
                            expected == before.get("_rev").unwrap_or(&Value::Null),
                            "conflict: source revision differs"
                        );
                    }
                    ensure!(
                        before["_deleted"] != true,
                        "not_found: document was deleted"
                    );
                    let kind = operation["type"].as_str().unwrap_or("");
                    let proposed = match kind {
                        "remove" => None,
                        "put" => Some(clean_input(operation["doc"].clone(), &id)?),
                        "patch" => {
                            let patch = operation["patch"]
                                .as_object()
                                .context("invalid: patch must be an object")?;
                            let mut next = before.clone();
                            next.as_object_mut()
                                .context("invalid: stored document")?
                                .extend(patch.clone());
                            Some(clean_input(next, &id)?)
                        }
                        _ => bail!("invalid: write type"),
                    };
                    let desired =
                        authority.prepare_write(&app, &actor, &name, Some(&before), proposed)?;
                    let mut next = match desired {
                        Some(doc) => {
                            ensure!(
                                kind != "remove",
                                "forbidden: Core policy changed remove intent"
                            );
                            clean_input(doc, &id)?
                        }
                        None => {
                            ensure!(kind == "remove", "forbidden: Core policy denied write");
                            before.clone()
                        }
                    };
                    for key in ["_rev", "_meta", "_attachments"] {
                        if let Some(value) = before.get(key) {
                            next[key] = value.clone();
                        }
                    }
                    // Match the native storage wrapper's revision before queue callbacks
                    // publish the successful write (the wrapper does not rewrite that ack).
                    next["_rev"] = json!(rxdb::plugins::utils::utils_revision::create_revision(
                        &database_token,
                        before["_rev"].as_str()
                    )
                    .map_err(store_error)?);
                    next["_deleted"] = json!(kind == "remove");
                    authority.authorize(&app, &actor, &json!({"operation":"write","args":args}))?;
                    Ok(next)
                };
                apply().map_err(|error| {
                    new_rx_error(
                        "APP_RUNTIME",
                        Some(json!({"code":error_code(&error.to_string())})),
                    )
                })
            })
        });
        let written = collection
            .incremental_write_queue()
            .map_err(store_error)?
            .add_write(current, modifier)
            .await
            .map_err(store_error)?;
        Ok(json!({"status":"accepted","id":written["id"],"rev":written["_rev"]}))
    }
}
fn clean_input(mut value: Value, id: &str) -> Result<Value> {
    ensure!(
        value.is_object() && value["id"].as_str() == Some(id),
        "invalid: write cannot change identity"
    );
    let object = value.as_object_mut().unwrap();
    for key in ["rev", "_rev", "_meta", "_attachments", "_deleted"] {
        object.remove(key);
    }
    Ok(value)
}
fn store_error(error: RxError) -> anyhow::Error {
    let code = match error.code() {
        "APP_RUNTIME" => error.parameters()["code"].as_str().unwrap_or("unavailable"),
        "CONFLICT" | "409" => "conflict",
        value if value.starts_with("VD") || value.starts_with("SC") || value.starts_with("DOC") => {
            "invalid"
        }
        _ => "unavailable",
    };
    // Never include native error parameters: they can contain unmasked records.
    anyhow::anyhow!("{code}: native Store operation rejected ({})", error.code())
}
impl RuntimeHost for LegacyStoreAdapter {
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()> {
        self.authority.admit(manifest, binding)
    }
    fn start_bound(&self, binding: &Value, effect: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.authority.start_bound(binding, effect)
    }
    fn complete_bound(&self, binding: &Value) -> Result<()> {
        self.authority.complete_bound(binding)
    }
    fn finish(&self, binding: &Value) {
        self.authority.finish(binding);
    }
    fn authorize(&self, app: &str, actor: &Value, operation: &Value) -> Result<()> {
        self.authority.authorize(app, actor, operation)
    }
    fn exported(&self, owner: &str, collection: &str, operation: &str) -> Result<bool> {
        self.authority.exported(owner, collection, operation)
    }
    fn capability_available(&self, name: &str) -> bool {
        self.authority.capability_available(name)
    }
    fn invoke(&self, app: &str, actor: &Value, method: &str, args: &Value) -> Result<Value> {
        match method {
            "read" => self.run(self.read(app, actor, args)),
            "write" => self.run(self.write(app, actor, args)),
            _ => self.authority.invoke(app, actor, method, args),
        }
    }
}

include!("legacy_store_tests.rs");
