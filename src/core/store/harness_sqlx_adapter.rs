//! SQLx native pool factory, called only under the file owner's lease.
use log::LevelFilter;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, SqlitePool, migrate::Migrator};
use std::{sync::Arc, time::Duration};

pub(crate) async fn open_pool(
    lease: Arc<super::owner_adapter::FileOwner>,
    migrator: &'static Migrator,
) -> anyhow::Result<SqlitePool> {
    lease.require_exclusive()?;
    let path = lease.path();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::ZERO)
        .log_statements(LevelFilter::Off);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .max_lifetime(None)
        .idle_timeout(None)
        .acquire_timeout(Duration::from_millis(250))
        .connect_with(options)
        .await?;
    if let Err(error) = migrator.run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    Ok(pool)
}
