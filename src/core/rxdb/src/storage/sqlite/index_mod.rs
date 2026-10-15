//! Factory entry for SQLite storage.

use super::instance::RxStorageInstanceSqlite;
use super::sql::{
    ensure_collection_schema_indexes, ensure_collection_table, ensure_owned_collection_table,
    table_name,
};
use super::types::{owner_result, RxStorageSqlite, RxStorageSqliteSettings, SqliteOwnerJobs};
use crate::rx_error::RxResult;
use crate::rx_storage_helper::ensure_rx_storage_instance_params_are_correct;
use crate::types::{RxStorage, RxStorageInstance, RxStorageInstanceCreationParams};
use async_trait::async_trait;
use std::sync::Arc;

pub const RX_STORAGE_NAME_SQLITE: &str = "sqlite";

pub fn get_rx_storage_sqlite(settings: RxStorageSqliteSettings) -> Arc<RxStorageSqlite> {
    RxStorageSqlite::new(settings)
}

/// Owner registration, connection configuration and lifetime belong to the daemon.
/// The storage adapter accepts jobs only and cannot open a parallel writer.
async fn create_owned(
    owner: Arc<dyn SqliteOwnerJobs>,
    settings: &RxStorageSqliteSettings,
    params: RxStorageInstanceCreationParams,
) -> RxResult<Arc<RxStorageInstanceSqlite>> {
    let table = table_name(
        &params.database_name,
        &params.collection_name,
        params.schema.version,
    );
    let ddl_table = table.clone();
    let schema = params.schema.clone();
    let collection = params.collection_name.clone();
    let log = owner.clone();
    let ticket = owner.write(Box::new(move |conn| {
        ensure_owned_collection_table(conn, &ddl_table)?;
        ensure_collection_schema_indexes(
            conn,
            &ddl_table,
            &schema,
            &crate::rx_schema_helper::get_primary_field_of_primary_key(&schema.primary_key),
        )?;
        log.install_change_log(conn, &ddl_table, &collection, schema.version)?;
        Ok(Box::new(()) as super::types::SqliteJobValue)
    }))?;
    owner_result::<()>(ticket).await?;
    let mut after: Option<String> = None;
    loop {
        let table = table.clone();
        let previous = after.clone();
        let log = owner.clone();
        let ticket = owner.write(Box::new(move |tx| {
            log.backfill_change_log(tx, &table, previous.as_deref())
                .map(|next| Box::new(next) as super::types::SqliteJobValue)
        }))?;
        let next = owner_result::<Option<String>>(ticket).await?;
        match next {
            None => break,
            Some(next) if after.as_ref().is_none_or(|previous| next > *previous) => {
                after = Some(next)
            }
            _ => {
                return Err(super::types::owner_job_error(
                    "change-log backfill cursor did not advance",
                ))
            }
        }
    }
    Ok(Arc::new(RxStorageInstanceSqlite::new_owned(
        owner,
        params,
        table,
        settings.database_path.clone(),
    )))
}

pub async fn create_storage_instance(
    storage: &Arc<RxStorageSqlite>,
    params: RxStorageInstanceCreationParams,
) -> RxResult<Arc<RxStorageInstanceSqlite>> {
    ensure_rx_storage_instance_params_are_correct(&params)?;
    if let Some(owner) = &storage.owner {
        return create_owned(owner.clone(), &storage.settings, params).await;
    }
    // Transitional V3 constructor: even its open/DDL/initial checkpoint must
    // never block an async worker. It is not an alternative owner backend.
    let storage = storage.clone();
    tokio::task::spawn_blocking(move || create_legacy(&storage, params))
        .await
        .map_err(|error| super::types::owner_job_error(&error.to_string()))?
}

fn create_legacy(
    storage: &RxStorageSqlite,
    params: RxStorageInstanceCreationParams,
) -> RxResult<Arc<RxStorageInstanceSqlite>> {
    let table = table_name(
        &params.database_name,
        &params.collection_name,
        params.schema.version,
    );
    let connection = storage.connection()?;
    {
        let conn = connection.lock();
        ensure_collection_table(&conn, &table)?;
        ensure_collection_schema_indexes(
            &conn,
            &table,
            &params.schema,
            &crate::rx_schema_helper::get_primary_field_of_primary_key(&params.schema.primary_key),
        )?;
    }
    let (point, changes) = storage.collection_readers();
    Ok(Arc::new(RxStorageInstanceSqlite::new(
        connection,
        params,
        table,
        storage.settings.database_path.clone(),
        point,
        changes,
    )))
}

#[async_trait]
impl RxStorage for RxStorageSqlite {
    fn name(&self) -> &str {
        &self.name
    }

    async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
    ) -> RxResult<Arc<dyn RxStorageInstance>> {
        ensure_rx_storage_instance_params_are_correct(&params)?;
        if let Some(owner) = &self.owner {
            return create_owned(owner.clone(), &self.settings, params)
                .await
                .map(|instance| instance as Arc<dyn RxStorageInstance>);
        }
        let storage = self.clone();
        tokio::task::spawn_blocking(move || create_legacy(&storage, params))
            .await
            .map_err(|error| super::types::owner_job_error(&error.to_string()))?
            .map(|instance| instance as Arc<dyn RxStorageInstance>)
    }
}
