//! E29.8 portable SQLite-API content measurement (no database-file hashes).
use super::*;
use rusqlite::{types::ValueRef, Connection};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub row_count: usize,
    pub schema_columns: Vec<String>,
    pub source_table_hash: String,
}
/// Caller supplies an already verified frozen read transaction (or an owner
/// snapshot). Cell types/bytes and duplicate rows are preserved; row order is not.
pub fn measure(conn: &Connection, table: &str) -> Result<Measurement> {
    let table = sql(crate::change_log::identifier(table))?;
    let mut statement = sql(conn.prepare(&format!("SELECT * FROM {table}")))?;
    let columns: Vec<_> = statement
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = sql(statement.query([]))?;
    let mut hashes = Vec::new();
    while let Some(row) = sql(cursor.next())? {
        let mut h = Sha256::new();
        for index in 0..columns.len() {
            let (tag, bytes): (u8, Vec<u8>) = match sql(row.get_ref(index))? {
                ValueRef::Null => (b'n', vec![]),
                ValueRef::Integer(n) => (b'i', n.to_string().into_bytes()),
                ValueRef::Real(n) => (b'r', n.to_bits().to_be_bytes().to_vec()),
                ValueRef::Text(b) => (b't', b.to_vec()),
                ValueRef::Blob(b) => (b'b', b.to_vec()),
            };
            h.update([tag]);
            h.update((bytes.len() as u64).to_be_bytes());
            h.update(bytes);
        }
        hashes.push(h.finalize().to_vec());
    }
    hashes.sort();
    let row_count = hashes.len();
    let mut h = Sha256::new();
    h.update(b"ctox.sqlite.table.v1\0");
    for column in &columns {
        h.update((column.len() as u64).to_be_bytes());
        h.update(column.as_bytes());
    }
    h.update([0]);
    for row in hashes {
        h.update(row);
    }
    Ok(Measurement {
        row_count,
        schema_columns: columns,
        source_table_hash: format!("{:x}", h.finalize()),
    })
}
