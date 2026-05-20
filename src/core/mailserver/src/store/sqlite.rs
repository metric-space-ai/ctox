// ref: stalwart/src/store/sqlite/mod.rs:1-120
// ref: ctox-mailserver new code for campaign & collaboration SQLite store

use crate::util::errors::StalwartResult;
use crate::store::sqlite_schema::SQLITE_SCHEMA;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
pub struct SqliteStore {
    db_path: String,
}

impl SqliteStore {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
        }
    }

    fn connect(&self) -> StalwartResult<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(conn)
    }

    pub fn init(&self) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SQLITE_SCHEMA)?;
        Ok(())
    }

    // --- Domain and DKIM Management ---

    pub fn add_domain(
        &self,
        domain_name: &str,
        dkim_selector: &str,
        dkim_private_key: &str,
    ) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO stalwart_domains (domain_name, dkim_selector, dkim_private_key)
             VALUES (?1, ?2, ?3)",
            params![domain_name, dkim_selector, dkim_private_key],
        )?;
        Ok(())
    }

    pub fn get_domain_dkim(&self, domain_name: &str) -> StalwartResult<Option<(String, String)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT dkim_selector, dkim_private_key FROM stalwart_domains WHERE domain_name = ?1",
        )?;
        let mut rows = stmt.query(params![domain_name])?;
        if let Some(row) = rows.next()? {
            let selector: String = row.get(0)?;
            let priv_key: String = row.get(1)?;
            Ok(Some((selector, priv_key)))
        } else {
            Ok(None)
        }
    }

    // --- SMTP Outbound Queue ---

    pub fn queue_email(&self, from: &str, to: &str, body: &str) -> StalwartResult<String> {
        let id = crate::util::generate_unique_id();
        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        conn.execute(
            "INSERT INTO stalwart_smtp_queue (id, from_addr, to_addr, msg_body, retry_count, next_attempt_at, status)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, 'pending')",
            params![id, from, to, body, now],
        )?;
        Ok(id)
    }

    pub fn get_pending_emails(&self) -> StalwartResult<Vec<(String, String, String, String, usize)>> {
        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        let mut stmt = conn.prepare(
            "SELECT id, from_addr, to_addr, msg_body, retry_count FROM stalwart_smtp_queue
             WHERE status = 'pending' AND next_attempt_at <= ?1",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, usize>(4)?,
            ))
        })?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn update_email_status(
        &self,
        id: &str,
        status: &str,
        next_attempt: u64,
        retry_count: usize,
    ) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE stalwart_smtp_queue SET status = ?2, next_attempt_at = ?3, retry_count = ?4
             WHERE id = ?1",
            params![id, status, next_attempt, retry_count],
        )?;
        Ok(())
    }

    pub fn delete_email(&self, id: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM stalwart_smtp_queue WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- CalDAV Operations ---

    pub fn create_calendar(&self, id: &str, owner: &str, display_name: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR IGNORE INTO stalwart_caldav_calendars (id, owner, display_name, description)
             VALUES (?1, ?2, ?3, '')",
            params![id, owner, display_name],
        )?;
        Ok(())
    }

    pub fn get_calendars(&self, owner: &str) -> StalwartResult<Vec<(String, String, String)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, display_name, COALESCE(description, '') FROM stalwart_caldav_calendars WHERE owner = ?1",
        )?;
        let rows = stmt.query_map(params![owner], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn put_event(&self, calendar_id: &str, uid: &str, ical_data: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        let id = format!("{}:{}", calendar_id, uid);
        conn.execute(
            "INSERT OR REPLACE INTO stalwart_caldav_events (id, calendar_id, uid, ical_data, last_modified)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, calendar_id, uid, ical_data, now],
        )?;
        Ok(())
    }

    pub fn get_events(&self, calendar_id: &str) -> StalwartResult<Vec<(String, String, String, u64)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, uid, ical_data, last_modified FROM stalwart_caldav_events WHERE calendar_id = ?1",
        )?;
        let rows = stmt.query_map(params![calendar_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn get_event(&self, calendar_id: &str, uid: &str) -> StalwartResult<Option<(String, u64)>> {
        let conn = self.connect()?;
        let id = format!("{}:{}", calendar_id, uid);
        let mut stmt = conn.prepare(
            "SELECT ical_data, last_modified FROM stalwart_caldav_events WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    pub fn delete_event(&self, calendar_id: &str, uid: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        let id = format!("{}:{}", calendar_id, uid);
        conn.execute("DELETE FROM stalwart_caldav_events WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- CardDAV Operations ---

    pub fn create_addressbook(&self, id: &str, owner: &str, display_name: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR IGNORE INTO stalwart_carddav_addressbooks (id, owner, display_name, description)
             VALUES (?1, ?2, ?3, '')",
            params![id, owner, display_name],
        )?;
        Ok(())
    }

    pub fn get_addressbooks(&self, owner: &str) -> StalwartResult<Vec<(String, String, String)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, display_name, COALESCE(description, '') FROM stalwart_carddav_addressbooks WHERE owner = ?1",
        )?;
        let rows = stmt.query_map(params![owner], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn put_contact(&self, addressbook_id: &str, uid: &str, vcard_data: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        let id = format!("{}:{}", addressbook_id, uid);
        conn.execute(
            "INSERT OR REPLACE INTO stalwart_carddav_contacts (id, addressbook_id, uid, vcard_data, last_modified)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, addressbook_id, uid, vcard_data, now],
        )?;
        Ok(())
    }

    pub fn get_contacts(&self, addressbook_id: &str) -> StalwartResult<Vec<(String, String, String, u64)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, uid, vcard_data, last_modified FROM stalwart_carddav_contacts WHERE addressbook_id = ?1",
        )?;
        let rows = stmt.query_map(params![addressbook_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;

        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn delete_contact(&self, addressbook_id: &str, uid: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        let id = format!("{}:{}", addressbook_id, uid);
        conn.execute("DELETE FROM stalwart_carddav_contacts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- User & Mailbox Operations ---

    pub fn add_user(&self, username: &str, password_hash: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        conn.execute(
            "INSERT OR REPLACE INTO stalwart_users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
            params![username, password_hash, now],
        )?;
        // Auto-create standard mailboxes for the user
        let inbox_id = format!("{}_inbox", username.replace("@", "_"));
        let sent_id = format!("{}_sent", username.replace("@", "_"));
        let trash_id = format!("{}_trash", username.replace("@", "_"));
        conn.execute(
            "INSERT OR IGNORE INTO stalwart_mailboxes (id, owner, name) VALUES (?1, ?2, 'INBOX')",
            params![inbox_id, username],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO stalwart_mailboxes (id, owner, name) VALUES (?1, ?2, 'Sent')",
            params![sent_id, username],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO stalwart_mailboxes (id, owner, name) VALUES (?1, ?2, 'Trash')",
            params![trash_id, username],
        )?;
        Ok(())
    }

    pub fn authenticate_user(&self, username: &str, password_hash: &str) -> StalwartResult<bool> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT password_hash FROM stalwart_users WHERE username = ?1")?;
        let mut rows = stmt.query(params![username])?;
        if let Some(row) = rows.next()? {
            let db_pass: String = row.get(0)?;
            Ok(db_pass == password_hash)
        } else {
            Ok(false)
        }
    }

    pub fn user_exists(&self, username: &str) -> StalwartResult<bool> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT 1 FROM stalwart_users WHERE username = ?1")?;
        let mut rows = stmt.query(params![username])?;
        Ok(rows.next()?.is_some())
    }

    pub fn get_mailboxes(&self, owner: &str) -> StalwartResult<Vec<(String, String)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id, name FROM stalwart_mailboxes WHERE owner = ?1")?;
        let rows = stmt.query_map(params![owner], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn get_mailbox_id(&self, owner: &str, name: &str) -> StalwartResult<Option<String>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id FROM stalwart_mailboxes WHERE owner = ?1 AND name = ?2")?;
        let mut rows = stmt.query(params![owner, name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    // --- Message Operations ---

    pub fn put_message(
        &self,
        mailbox_id: &str,
        from_addr: &str,
        to_addr: &str,
        subject: Option<&str>,
        body: &str,
        headers: Option<&str>,
    ) -> StalwartResult<String> {
        let conn = self.connect()?;
        let id = crate::util::generate_unique_id();
        let now = crate::util::now_utc_secs();
        conn.execute(
            "INSERT INTO stalwart_messages (id, mailbox_id, from_addr, to_addr, subject, body, headers, is_read, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![id, mailbox_id, from_addr, to_addr, subject, body, headers, now],
        )?;
        Ok(id)
    }

    pub fn get_messages(&self, mailbox_id: &str) -> StalwartResult<Vec<(String, String, String, Option<String>, String, Option<String>, bool, u64)>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, from_addr, to_addr, subject, body, headers, is_read, received_at 
             FROM stalwart_messages WHERE mailbox_id = ?1 ORDER BY received_at DESC",
        )?;
        let rows = stmt.query_map(params![mailbox_id], |row| {
            let is_read_int: i32 = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                is_read_int != 0,
                row.get::<_, u64>(7)?,
            ))
        })?;
        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn update_message_flags(&self, id: &str, is_read: bool) -> StalwartResult<()> {
        let conn = self.connect()?;
        let is_read_int = if is_read { 1 } else { 0 };
        conn.execute("UPDATE stalwart_messages SET is_read = ?2 WHERE id = ?1", params![id, is_read_int])?;
        Ok(())
    }

    pub fn delete_message(&self, id: &str) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM stalwart_messages WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn check_greylist(&self, ip: &str, sender: &str, recipient: &str) -> StalwartResult<bool> {
        if ip == "127.0.0.1" || ip == "::1" || ip.starts_with("127.") || ip == "localhost" {
            return Ok(true);
        }

        let conn = self.connect()?;
        let now = crate::util::now_utc_secs();
        let mut stmt = conn.prepare(
            "SELECT first_seen_at FROM stalwart_greylist WHERE ip = ?1 AND sender = ?2 AND recipient = ?3",
        )?;
        let mut rows = stmt.query(params![ip, sender, recipient])?;
        if let Some(row) = rows.next()? {
            let first_seen_at: u64 = row.get(0)?;
            if now >= first_seen_at + 300 {
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            conn.execute(
                "INSERT INTO stalwart_greylist (ip, sender, recipient, first_seen_at) VALUES (?1, ?2, ?3, ?4)",
                params![ip, sender, recipient, now],
            )?;
            Ok(false)
        }
    }
}

