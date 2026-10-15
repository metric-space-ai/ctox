//! P9 queue readers. Pre-cutover callers retain their Domain schema; a V4
//! command authority never reads the legacy ledger after cutover, even on a projection miss.
use super::read_command_projection;
use rusqlite::{Connection, OptionalExtension};

pub(super) fn projected_command_module(
    conn: &Connection,
    command_id: &str,
) -> rusqlite::Result<Option<String>> {
    match read_command_projection(conn, command_id)? {
        Some(projection) => Ok(projection.map(|p| p.module)),
        None => conn
            .query_row(
                "SELECT module FROM business_commands WHERE command_id=?1",
                [command_id],
                |r| r.get(0),
            )
            .optional(),
    }
}
pub(super) fn projected_command_status(
    conn: &Connection,
    command_id: &str,
) -> rusqlite::Result<Option<String>> {
    match read_command_projection(conn, command_id)? {
        Some(projection) => Ok(projection.map(|p| p.status)),
        None => conn
            .query_row(
                "SELECT status FROM business_commands WHERE command_id=?1",
                [command_id],
                |r| r.get(0),
            )
            .optional(),
    }
}
