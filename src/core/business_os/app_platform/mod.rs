// Origin: CTOX
// License: AGPL-3.0-only
//! App publication is a transaction on the canonical store, never a second DB.
//!
//! The store owner injects its existing Database and native stream signer. The
//! migration runner prepares pages outside that writer and calls `complete`.
//! Client activation remains a separate checkpoint-gated protocol.

mod commands;
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

#[derive(Clone)]
pub(crate) struct Platform {
    writer: WriterHandle,
    signer: Signer,
    migrations: MigrationResolver,
}

impl Platform {
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
        }
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
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::execute(
                    tx,
                    &actor,
                    &command,
                    now_ms,
                    signer.as_ref(),
                    migrations.as_ref(),
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
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                commands::complete(tx, &actor, &command_id, now_ms)
            }))
    }
}
