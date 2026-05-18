//! Write operations for CTOX databases (approval, etc.)

use crate::db_reader::agent_db_path;
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;

fn open_readwrite(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

/// Insert an approval record for a case.
pub fn approve_case(root: &Path, case_id: &str, rationale: &str) -> Result<(), String> {
    let path = agent_db_path(root).ok_or("Agent DB not found")?;
    let conn = open_readwrite(&path).ok_or("Cannot open agent DB for writing")?;

    let approval_id = format!("approval:{}:{}", case_id, &uuid_v4_short());
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO ticket_approvals (approval_id, case_id, status, decided_by, rationale, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![approval_id, case_id, "approved", "owner", rationale, now],
    )
    .map_err(|e| format!("Failed to insert approval: {}", e))?;

    Ok(())
}

/// Insert a denial record for a case.
pub fn deny_case(root: &Path, case_id: &str, rationale: &str) -> Result<(), String> {
    let path = agent_db_path(root).ok_or("Agent DB not found")?;
    let conn = open_readwrite(&path).ok_or("Cannot open agent DB for writing")?;

    let approval_id = format!("approval:{}:{}", case_id, &uuid_v4_short());
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO ticket_approvals (approval_id, case_id, status, decided_by, rationale, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![approval_id, case_id, "denied", "owner", rationale, now],
    )
    .map_err(|e| format!("Failed to insert denial: {}", e))?;

    Ok(())
}

fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:012x}", t & 0xFFFF_FFFF_FFFF)
}
