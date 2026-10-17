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

/// An immutable capture from ONE owner reader transaction. Hashing is CPU work
/// and must run after that reader job releases its short execution budget.
pub(crate) struct Capture {
    columns: Vec<String>,
    rows: Vec<Vec<(u8, Vec<u8>)>>,
}
pub(crate) fn capture(
    conn: &Connection,
    table: &str,
    max_rows: usize,
    max_bytes: usize,
) -> Result<Capture> {
    let table = sql(crate::change_log::identifier(table))?;
    let mut statement = sql(conn.prepare(&format!("SELECT * FROM {table}")))?;
    let columns: Vec<_> = statement
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = sql(statement.query([]))?;
    let mut rows = Vec::new();
    let mut total = 0usize;
    while let Some(row) = sql(cursor.next())? {
        let mut cells = Vec::with_capacity(columns.len());
        total = total.saturating_add(std::mem::size_of_val(&cells));
        for index in 0..columns.len() {
            let cell = match sql(row.get_ref(index))? {
                ValueRef::Null => (b'n', vec![]),
                ValueRef::Integer(n) => (b'i', n.to_string().into_bytes()),
                ValueRef::Real(n) => (b'r', n.to_bits().to_be_bytes().to_vec()),
                ValueRef::Text(b) => (b't', b.to_vec()),
                ValueRef::Blob(b) => (b'b', b.to_vec()),
            };
            total = total
                .saturating_add(cell.1.len())
                .saturating_add(std::mem::size_of_val(&cell));
            if total > max_bytes {
                return Err(invalid("cutover_retained_capture_budget"));
            }
            cells.push(cell);
        }
        if rows.len() == max_rows {
            return Err(invalid("cutover_retained_capture_budget"));
        }
        rows.push(cells);
    }
    Ok(Capture { columns, rows })
}
impl Capture {
    pub(crate) fn measure(self) -> Measurement {
        let mut hashes = Vec::with_capacity(self.rows.len());
        for row in self.rows {
            let mut hasher = Sha256::new();
            for (tag, bytes) in row {
                hasher.update([tag]);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
            hashes.push(hasher.finalize().to_vec());
        }
        hashes.sort();
        let row_count = hashes.len();
        let mut hasher = Sha256::new();
        hasher.update(b"ctox.sqlite.table.v1\0");
        for name in &self.columns {
            hasher.update((name.len() as u64).to_be_bytes());
            hasher.update(name.as_bytes());
        }
        hasher.update([0]);
        for row in hashes {
            hasher.update(row);
        }
        Measurement {
            row_count,
            schema_columns: self.columns,
            source_table_hash: format!("{:x}", hasher.finalize()),
        }
    }
}
#[cfg(test)]
mod capture_tests {
    use super::*;
    #[test]
    fn retained_capture_keeps_the_original_snapshot_and_sqlite_cell_types() -> Result<()> {
        let db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE sample(a,b,c,d);INSERT INTO sample VALUES(NULL,42,1.5,x'6162'),('ab',42,1.5,NULL);INSERT INTO sample SELECT * FROM sample;")?;
        let original = measure(&db, "sample")?;
        let captured = capture(&db, "sample", 10, 4096)?;
        db.execute_batch("DELETE FROM sample;INSERT INTO sample VALUES('changed',0,0,NULL);")?;
        let captured = captured.measure();
        assert_eq!(original.source_table_hash, captured.source_table_hash);
        assert_eq!(original.row_count, captured.row_count);
        assert_eq!(original.schema_columns, captured.schema_columns);
        assert_ne!(
            measure(&db, "sample")?.source_table_hash,
            captured.source_table_hash
        );
        assert!(capture(&db, "sample", 0, 4096).is_err());
        assert!(capture(&db, "sample", 10, 1).is_err());
        Ok(())
    }
}
