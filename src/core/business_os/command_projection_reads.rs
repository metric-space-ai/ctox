//! P9 queue readers. Preparation alone does not switch command authority.
//! After explicit cutover a projection miss never consults the legacy ledger.
use super::read_command_projection;
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone, Copy)]
enum Field {
    Module,
    Status,
}
fn read_field(
    conn: &Connection,
    command_id: &str,
    field: Field,
) -> rusqlite::Result<Option<String>> {
    // A native pre-cutover caller may supply an autocommit connection; reuse an
    // existing writer/reader transaction, otherwise pin one read snapshot here.
    let snapshot = if conn.is_autocommit() {
        Some(conn.unchecked_transaction()?)
    } else {
        None
    };
    let value = match read_command_projection(conn, command_id)? {
        Some(projection) => projection.map(|p| match field {
            Field::Module => p.module,
            Field::Status => p.status,
        }),
        None => conn
            .query_row(
                match field {
                    Field::Module => "SELECT module FROM business_commands WHERE command_id=?1",
                    Field::Status => "SELECT status FROM business_commands WHERE command_id=?1",
                },
                [command_id],
                |r| r.get(0),
            )
            .optional()?,
    };
    if let Some(snapshot) = snapshot {
        snapshot.commit()?;
    }
    Ok(value)
}
pub(super) fn projected_command_module(
    conn: &Connection,
    command_id: &str,
) -> rusqlite::Result<Option<String>> {
    read_field(conn, command_id, Field::Module)
}
pub(super) fn projected_command_status(
    conn: &Connection,
    command_id: &str,
) -> rusqlite::Result<Option<String>> {
    read_field(conn, command_id, Field::Status)
}
