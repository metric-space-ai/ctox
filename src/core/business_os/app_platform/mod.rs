// Origin: CTOX
// License: AGPL-3.0-only
//! App publication is a transaction on the canonical store, never a second DB.
//!
//! The store owner injects its existing Database and native stream signer. The
//! migration runner prepares pages outside that writer and calls `complete`.
//! Client activation remains a separate checkpoint-gated protocol.

mod commands;
#[cfg(test)]
mod model_check_tests;
/// Native owner hooks over this same canonical transaction; never a wire API.
pub(crate) mod platform_routes;
pub(crate) mod runtime_host;
#[cfg(test)]
mod tests;

use super::super::policy::BusinessOsActor;
pub(crate) use commands::{
    can_read_draft, can_write_draft, handles, ipc_declarations_allow, CommandDocument,
};
use ctox_store::{
    types::{JobOptions, StoreError, WriteJob},
    writer::WriterHandle,
};
use serde_json::Value;
use std::sync::Arc;

/// Implemented by the native signing-key owner. No key material enters commands.
/// The owner loads its key before attaching: this callback performs bounded,
/// in-memory signing only and must never do I/O on the writer thread.
pub(crate) type Signer = Arc<dyn ctox_store::change_log::subscription::protocol::InstanceSigner>;
/// Native runner resolution, never a boolean supplied by an app. Resolution
/// must be in-memory and bounded: migration execution happens outside writer.
pub(crate) type MigrationResolver = Arc<
    dyn Fn(
            &ctox_store::schema::CollectionDefinition,
            &ctox_store::schema::CollectionDefinition,
            &Value,
        ) -> bool
        + Send
        + Sync,
>;

fn store_failure(error: StoreError) -> Value {
    // Store errors contain validation/SQLite/I/O diagnostics, never secret
    // values: credential deletion binds only native constant selectors.
    let raw_detail = match &error {
        StoreError::Invalid(message) | StoreError::Sqlite(message) | StoreError::Io(message) => {
            message.clone()
        }
        _ => error.to_string(),
    };
    let (code, reason) = match error {
        StoreError::Invalid(_) => ("invalid", "native_validation_failed"),
        StoreError::Busy => ("unavailable", "store_busy"),
        StoreError::Cancelled => ("unavailable", "store_cancelled"),
        StoreError::DeadlineExceeded => ("unavailable", "store_deadline"),
        StoreError::Closed => ("unavailable", "store_closed"),
        StoreError::AlreadyOwned => ("conflict", "store_already_owned"),
        StoreError::Sqlite(_) => ("unavailable", "store_sqlite_failed"),
        StoreError::Io(_) => ("unavailable", "store_io_failed"),
        StoreError::Panicked => ("unavailable", "store_panicked"),
    };
    commands::failure("failed", code, reason, "", &raw_detail)
}

pub(crate) type ModelEffectFence =
    Arc<dyn Fn(&mut dyn FnMut() -> anyhow::Result<Value>) -> anyhow::Result<Value> + Send + Sync>;
#[derive(Clone)]
pub(crate) struct ModelCheckFence {
    pub effect: ModelEffectFence,
    pub commit: Arc<dyn Fn(&rusqlite::Transaction<'_>) -> Result<(), StoreError> + Send + Sync>,
}
type ModelChecker = Arc<
    dyn Fn(
            String,
            commands::ModelCheckRequest,
            Option<ModelEffectFence>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::execution::cliproxyapi_host::model_probe::Probe,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

pub(super) type RightsPublicationFence = Arc<dyn Fn() -> Result<(), StoreError> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Platform {
    writer: WriterHandle,
    signer: Signer,
    migrations: MigrationResolver,
    secret_writer: Option<WriterHandle>,
    model_checker: Option<ModelChecker>,
    rights_publication: Option<RightsPublicationFence>,
    worker_reader: Option<ctox_store::read_pool::ReadPool>,
}

impl Platform {
    /// Called by the authenticated native hello owner before serializing its
    /// selected capabilities. Never accepts a role asserted by the peer.
    pub(crate) fn filter_session_capabilities(
        &self,
        authenticated_user_id: String,
        capabilities: Vec<String>,
    ) -> Result<ctox_store::types::JobHandle<Vec<String>>, StoreError> {
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                platform_routes::filter_session_capabilities(
                    tx,
                    &authenticated_user_id,
                    capabilities,
                )
            }))
    }

    /// The daemon composition root passes its already registered sync database.
    /// This adapter clones its writer; it never opens a file or registers a
    /// second owner. Key loading is complete before the bounded signer arrives.
    pub(crate) fn from_database(
        database: &crate::service::store_runtime::Database,
        signer: Signer,
    ) -> Result<Self, StoreError> {
        if database.readers.generation() != database.writer.generation().0 {
            return Err(StoreError::Invalid(
                "app platform owner generation differs".into(),
            ));
        }
        if signer.instance_id().is_empty() || signer.key_id().is_empty() {
            return Err(StoreError::Invalid(
                "instance signing identity is required".into(),
            ));
        }
        Ok(Self::new(database.writer.clone(), signer))
    }

    fn new(writer: WriterHandle, signer: Signer) -> Self {
        Self {
            writer,
            signer,
            migrations: Arc::new(|_, _, _| false),
            secret_writer: None,
            model_checker: None,
            rights_publication: None,
            worker_reader: None,
        }
    }

    pub(super) fn with_rights_publication(mut self, fence: RightsPublicationFence) -> Self {
        self.rights_publication = Some(fence);
        self
    }

    /// The daemon lends its own root; command JSON cannot choose it. Network
    /// work uses the existing native gateway, outside the Store writer.
    pub(crate) fn with_native_model_checks(mut self, root: std::path::PathBuf) -> Self {
        // This is the existing native-root attachment point. Borrow its BOS
        // reader as well; do not alter the concurrently-owned feed or open a DB.
        self.worker_reader = crate::service::store_inputs::store_inputs(&root)
            .ok()
            .and_then(|inputs| {
                inputs
                    .databases
                    .get("bos")
                    .filter(|db| db.readers.generation() == db.writer.generation().0)
                    .map(|db| db.readers.clone())
            });
        self.model_checker = Some(Arc::new(move |actor, request, fence| {
            Box::pin(crate::execution::cliproxyapi_host::model_probe::check(
                root.clone(),
                actor,
                request.account_id,
                request.expected_account_revision,
                request.model_id,
                fence,
            ))
        }));
        self
    }

    pub(crate) fn with_native_worker_database(
        mut self,
        database: &crate::service::store_runtime::Database,
    ) -> Result<Self, StoreError> {
        if database.readers.generation() != database.writer.generation().0 {
            return Err(StoreError::Invalid(
                "worker identity owner generation differs".into(),
            ));
        }
        self.worker_reader = Some(database.readers.clone());
        Ok(self)
    }

    async fn worker_binding(
        &self,
        device: String,
    ) -> Result<Option<crate::business_os::machine_enrollment::ActiveDeviceEnrollment>, StoreError>
    {
        let reader = self.worker_reader.as_ref().ok_or(StoreError::Closed)?;
        reader
            .read(move |tx| {
                Ok(
                    crate::business_os::devices::active_machine_enrollment(tx, &device).map_err(
                        |_| StoreError::Invalid("native machine binding lookup failed".into()),
                    ),
                )
            })
            .await?
    }

    /// Native execution consumer: resolve the live source again, then inspect
    /// this grant's current canonical rights, actor, scope and expiry. No DB
    /// owner is nested in another writer and no claimed JSON binding is used.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn worker_grant(
        &self,
        id: String,
        actor: BusinessOsActor,
        app: String,
        device: String,
        now: u64,
    ) -> Result<Option<Value>, StoreError> {
        let binding = self.worker_binding(device.clone()).await?;
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                platform_routes::worker_grant(tx, &id, &actor, &app, &device, now, binding.as_ref())
            }))?
            .await
    }

    /// CUTOVER composition supplies an already registered secrets DB owner.
    /// This is a second *handle*, never a second writer for that database.
    pub(crate) fn with_secret_database(
        mut self,
        database: &crate::service::store_runtime::Database,
    ) -> Result<Self, StoreError> {
        if database.readers.generation() != database.writer.generation().0 {
            return Err(StoreError::Invalid(
                "secret owner generation differs".into(),
            ));
        }
        self.secret_writer = Some(database.writer.clone());
        Ok(self)
    }

    /// Async dispatcher interface: retains typed failures all the way to the
    /// command result. Separate owned databases are never synchronously nested.
    /// A durable pending command survives a crash between secret deletion and
    /// its public projection; retry repeats only the idempotent delete.
    pub(crate) async fn dispatch(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now_ms: u64,
    ) -> Value {
        self.dispatch_fenced(actor, command, now_ms, None).await
    }

    /// Only the native feed supplies its retained execution scope. Model IO
    /// and result publication both recheck it; no fence is accepted from JSON.
    pub(crate) async fn dispatch_fenced(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now_ms: u64,
        fence: Option<ModelCheckFence>,
    ) -> Value {
        if command.command_type == "ctox.workjet.remote_worker.admit" {
            let device = command.payload["device_id"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            use crate::business_os::machine_enrollment::DeviceEnrollmentResolution;
            let binding = match self.worker_binding(device).await {
                Ok(binding) => DeviceEnrollmentResolution::Resolved(binding),
                Err(_) => DeviceEnrollmentResolution::Unavailable,
            };
            let signer = self.signer.clone();
            let migrations = self.migrations.clone();
            let secret_owner_available = self.secret_writer.is_some();
            return match self
                .writer
                .submit(WriteJob::new(JobOptions::default(), move |tx| {
                    if let Some(fence) = fence {
                        (fence.commit)(tx)?;
                    }
                    commands::execute_with_worker_binding(
                        tx,
                        &actor,
                        &command,
                        now_ms,
                        secret_owner_available,
                        signer.as_ref(),
                        migrations.as_ref(),
                        &binding,
                    )
                })) {
                Ok(job) => job.await.unwrap_or_else(store_failure),
                Err(e) => store_failure(e),
            };
        }
        let prepared = match self.execute(actor.clone(), command.clone(), now_ms) {
            Ok(job) => match job.await {
                Ok(value) => value,
                Err(error) => return store_failure(error),
            },
            Err(error) => return store_failure(error),
        };
        if command.command_type == "instance.providers.models.check"
            && prepared["status"] == "preparing"
        {
            let request: commands::ModelCheckRequest =
                match serde_json::from_value(command.payload.clone()) {
                    Ok(value) => value,
                    Err(_) => {
                        return store_failure(StoreError::Invalid(
                            "invalid admitted model check".into(),
                        ))
                    }
                };
            let probe = match &self.model_checker {
                Some(check) => {
                    check(
                        actor.id.clone().unwrap_or_default(),
                        request,
                        fence.as_ref().map(|f| f.effect.clone()),
                    )
                    .await
                }
                None => crate::execution::cliproxyapi_host::model_probe::Probe::unavailable(
                    &request.model_id,
                    "gateway_state_unavailable",
                ),
            };
            return match self
                .writer
                .submit(WriteJob::new(JobOptions::default(), move |tx| {
                    if let Some(fence) = fence {
                        (fence.commit)(tx)?;
                    }
                    commands::finish_model_check(tx, &actor, &command, probe, now_ms)
                })) {
                Ok(job) => match job.await {
                    Ok(value) => value,
                    Err(e) => store_failure(e),
                },
                Err(e) => store_failure(e),
            };
        }
        if command.command_type != "speech.tts.key.delete" || prepared["status"] != "preparing" {
            return prepared;
        }
        let Some(secrets) = &self.secret_writer else {
            return commands::failure(
                "failed",
                "unavailable",
                "secret_owner_missing",
                "payload.provider",
                "The daemon secret-store writer is not attached.",
            );
        };
        let current_actor = actor.clone();
        let secret_command = command.clone();
        let deleted = match secrets.submit(WriteJob::new(JobOptions::default(), move |tx| {
            commands::delete_tts_key(tx, &current_actor, &secret_command)
        })) {
            Ok(job) => match job.await {
                Ok(value) => value,
                Err(error) => store_failure(error),
            },
            Err(error) => store_failure(error),
        };
        match self
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::finish_key_delete(tx, &actor, &command, deleted, now_ms)
            })) {
            Ok(job) => match job.await {
                Ok(value) => value,
                Err(error) => store_failure(error),
            },
            Err(error) => store_failure(error),
        }
    }

    /// Called by the native credential owner after an authorized key update.
    /// The closure captures only the hint; the full key stays in the caller.
    pub(crate) fn project_speech_key(
        &self,
        provider: &str,
        key: &str,
        now_ms: u64,
    ) -> Result<ctox_store::types::JobHandle<()>, StoreError> {
        let hint = commands::key_hint(key);
        let provider = provider.to_owned();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::project_key_hint(tx, &provider, hint.as_deref(), now_ms)
            }))
    }

    pub(crate) fn with_migrations(mut self, migrations: MigrationResolver) -> Self {
        self.migrations = migrations;
        self
    }

    pub(crate) fn execute(
        &self,
        actor: BusinessOsActor,
        command: CommandDocument,
        now_ms: u64,
    ) -> Result<ctox_store::types::JobHandle<Value>, StoreError> {
        let signer = self.signer.clone();
        let migrations = self.migrations.clone();
        let secret_owner_available = self.secret_writer.is_some();
        let publication = self.rights_publication.clone();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::execute_with_publication(
                    tx,
                    &actor,
                    &command,
                    now_ms,
                    secret_owner_available,
                    signer.as_ref(),
                    migrations.as_ref(),
                    publication.as_deref(),
                )
            }))
    }

    /// Native migration/index runner completion; the saved publication's CAS is
    /// checked again. Never invoked by an untrusted command payload.
    pub(crate) fn complete(
        &self,
        actor: BusinessOsActor,
        command_id: String,
        now_ms: u64,
    ) -> Result<ctox_store::types::JobHandle<Value>, StoreError> {
        let publication = self.rights_publication.clone();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::complete_with_publication(
                    tx,
                    &actor,
                    &command_id,
                    now_ms,
                    publication.as_deref(),
                )
            }))
    }
}
