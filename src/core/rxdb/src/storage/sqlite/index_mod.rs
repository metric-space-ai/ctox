//! Factory entry for SQLite storage.

use std::sync::Arc;

use async_trait::async_trait;

use crate::rx_error::RxResult;
use crate::rx_storage_helper::ensure_rx_storage_instance_params_are_correct;
use crate::types::{RxStorage, RxStorageInstance, RxStorageInstanceCreationParams};

use super::instance::RxStorageInstanceSqlite;
use super::sql::{ensure_collection_table, table_name};
use super::types::{RxStorageSqlite, RxStorageSqliteSettings};

pub const RX_STORAGE_NAME_SQLITE: &str = "sqlite";

pub fn get_rx_storage_sqlite(settings: RxStorageSqliteSettings) -> Arc<RxStorageSqlite> {
    RxStorageSqlite::new(settings)
}

pub async fn create_storage_instance(
    storage: &Arc<RxStorageSqlite>,
    params: RxStorageInstanceCreationParams,
) -> RxResult<Arc<RxStorageInstanceSqlite>> {
    ensure_rx_storage_instance_params_are_correct(&params)?;
    let table_name = table_name(
        &params.database_name,
        &params.collection_name,
        params.schema.version,
    );
    let connection = storage.connection()?;
    {
        let conn = connection.lock();
        ensure_collection_table(&conn, &table_name)?;
    }
    let database_path = storage.settings.database_path.clone();
    Ok(Arc::new(RxStorageInstanceSqlite::new(
        connection,
        params,
        table_name,
        database_path,
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
        let table_name = table_name(
            &params.database_name,
            &params.collection_name,
            params.schema.version,
        );
        let connection = self.connection()?;
        {
            let conn = connection.lock();
            ensure_collection_table(&conn, &table_name)?;
        }
        let database_path = self.settings.database_path.clone();
        Ok(Arc::new(RxStorageInstanceSqlite::new(
            connection,
            params,
            table_name,
            database_path,
        )))
    }
}
