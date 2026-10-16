//! FULL-durability native owner factory. Connections never leave the actor.
pub use crate::harness_adapter::BoundedOwner;
use rusqlite::Connection;
use std::{io, path::PathBuf};

pub fn open_full(
    path: PathBuf,
    initialize: impl FnOnce(&Connection) -> io::Result<()> + Send + 'static,
) -> io::Result<BoundedOwner<Connection>> {
    BoundedOwner::start(path, move |path| {
        let connection = Connection::open(path).map_err(io::Error::other)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(io::Error::other)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA journal_size_limit=67108864; PRAGMA wal_autocheckpoint=1000;").map_err(io::Error::other)?;
        initialize(&connection)?;
        Ok(connection)
    })
}
