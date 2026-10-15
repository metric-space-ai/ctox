//! FULL-durability native owner factory. Connections never leave the actor.
use rusqlite::Connection;
use std::{io, path::PathBuf};
#[path = "harness_adapter.rs"]
mod file_owner;
pub use file_owner::BoundedOwner;

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
