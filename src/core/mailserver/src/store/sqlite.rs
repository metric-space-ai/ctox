// ref: stalwart/src/store/sqlite/mod.rs:1-120
// ref: ctox-mailserver new code for campaign & collaboration SQLite store

use crate::config::MailserverRuntimeSettings;
use crate::store::sqlite_schema::SQLITE_SCHEMA;
use crate::util::errors::StalwartResult;
use ring::hmac;
use rusqlite::{params, Connection};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

thread_local! {
    static SQLITE_STORE_CONNECTIONS: RefCell<HashMap<String, Connection>> =
        RefCell::new(HashMap::new());
}

#[cfg(test)]
static SQLITE_STORE_OPEN_COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SqliteStoreChangeStamp {
    main: SqliteFileChangeStamp,
    wal: SqliteFileChangeStamp,
    shm: SqliteFileChangeStamp,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct SqliteFileChangeStamp {
    exists: bool,
    len: u64,
    modified_ns: u128,
}

#[derive(Clone, Debug)]
pub struct SqliteStore {
    db_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageSummary {
    pub id: String,
    pub from_addr: String,
    pub to_addr: String,
    pub subject: Option<String>,
    pub is_read: bool,
    pub received_at: u64,
    pub uid: i64,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageContent {
    pub body: String,
    pub headers: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailTrackingToken {
    pub message_id: String,
    pub campaign_id: Option<String>,
    pub event_type: String,
    pub target_url: Option<String>,
}

impl SqliteStore {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
        }
    }

    fn open_connection(db_path: &str) -> StalwartResult<Connection> {
        record_open_connection_for_test(db_path);
        let conn = Connection::open(db_path)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(conn)
    }

    fn connect(&self) -> StalwartResult<Connection> {
        Self::open_connection(&self.db_path)
    }

    fn with_connection<T>(
        &self,
        f: impl FnOnce(&Connection) -> StalwartResult<T>,
    ) -> StalwartResult<T> {
        SQLITE_STORE_CONNECTIONS.with(|connections| {
            let mut connections = connections.borrow_mut();
            if !connections.contains_key(&self.db_path) {
                let conn = Self::open_connection(&self.db_path)?;
                connections.insert(self.db_path.clone(), conn);
            }
            let conn = connections
                .get(&self.db_path)
                .expect("sqlite store connection cache entry exists");
            f(conn)
        })
    }

    pub fn change_stamp(&self) -> SqliteStoreChangeStamp {
        sqlite_store_change_stamp(Path::new(&self.db_path))
    }

    pub fn init(&self) -> StalwartResult<()> {
        let conn = self.connect()?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SQLITE_SCHEMA)?;
        migrate_message_uids(&conn)?;
        Ok(())
    }

    pub fn load_runtime_settings(&self) -> StalwartResult<MailserverRuntimeSettings> {
        self.with_connection(|conn| {
            let raw = conn.query_row(
                "SELECT config_json FROM stalwart_runtime_config WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            );
            match raw {
                Ok(raw) => serde_json::from_str(&raw)
                    .map_err(|err| crate::util::errors::StalwartError::General(err.to_string())),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    Ok(MailserverRuntimeSettings::default())
                }
                Err(err) => Err(err.into()),
            }
        })
    }

    pub fn save_runtime_settings(
        &self,
        settings: &MailserverRuntimeSettings,
    ) -> StalwartResult<()> {
        let raw = serde_json::to_string(settings)
            .map_err(|err| crate::util::errors::StalwartError::General(err.to_string()))?;
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO stalwart_runtime_config (id, enabled, config_json, updated_at)
                 VALUES (1, ?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled,
                     config_json = excluded.config_json, updated_at = excluded.updated_at",
                params![
                    settings.enabled as i64,
                    raw,
                    crate::util::now_utc_secs() as i64
                ],
            )?;
            Ok(())
        })
    }

    pub fn save_tracking_token(
        &self,
        token: &str,
        message_id: &str,
        campaign_id: Option<&str>,
        event_type: &str,
        target_url: Option<&str>,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO stalwart_mail_tracking_tokens
                    (token, message_id, campaign_id, event_type, target_url, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    token,
                    message_id,
                    campaign_id,
                    event_type,
                    target_url,
                    crate::util::now_utc_secs() as i64
                ],
            )?;
            Ok(())
        })
    }

    pub fn tracking_token(&self, token: &str) -> StalwartResult<Option<MailTrackingToken>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT message_id, campaign_id, event_type, target_url
                 FROM stalwart_mail_tracking_tokens WHERE token = ?1",
            )?;
            let mut rows = stmt.query(params![token])?;
            if let Some(row) = rows.next()? {
                Ok(Some(MailTrackingToken {
                    message_id: row.get(0)?,
                    campaign_id: row.get(1)?,
                    event_type: row.get(2)?,
                    target_url: row.get(3)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    pub fn record_tracking_event(
        &self,
        token: &str,
        message_id: &str,
        event_type: &str,
        occurred_at: i64,
        user_agent: Option<&str>,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO stalwart_mail_tracking_events
                    (id, token, message_id, event_type, occurred_at, user_agent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    crate::util::generate_unique_id(),
                    token,
                    message_id,
                    event_type,
                    occurred_at,
                    user_agent
                ],
            )?;
            Ok(())
        })
    }

    // --- Domain and DKIM Management ---

    pub fn add_domain(
        &self,
        domain_name: &str,
        dkim_selector: &str,
        dkim_private_key: &str,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO stalwart_domains (domain_name, dkim_selector, dkim_private_key)
                 VALUES (?1, ?2, ?3)",
                params![domain_name, dkim_selector, dkim_private_key],
            )?;
            Ok(())
        })
    }

    pub fn get_domain_dkim(&self, domain_name: &str) -> StalwartResult<Option<(String, String)>> {
        self.with_connection(|conn| {
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
        })
    }

    // --- SMTP Outbound Queue ---

    pub fn queue_email(&self, from: &str, to: &str, body: &str) -> StalwartResult<String> {
        let id = crate::util::generate_unique_id();
        let now = crate::util::now_utc_secs();
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO stalwart_smtp_queue (id, from_addr, to_addr, msg_body, retry_count, next_attempt_at, status)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, 'pending')",
                params![id.as_str(), from, to, body, now],
            )?;
            Ok(id)
        })
    }

    pub fn get_pending_emails(
        &self,
    ) -> StalwartResult<Vec<(String, String, String, String, usize)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn next_pending_email_attempt_at(&self) -> StalwartResult<Option<u64>> {
        self.with_connection(|conn| {
            let next_attempt = conn.query_row(
                "SELECT MIN(next_attempt_at) FROM stalwart_smtp_queue WHERE status = 'pending'",
                [],
                |row| row.get::<_, Option<u64>>(0),
            )?;
            Ok(next_attempt)
        })
    }

    pub fn update_email_status(
        &self,
        id: &str,
        status: &str,
        next_attempt: u64,
        retry_count: usize,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE stalwart_smtp_queue SET status = ?2, next_attempt_at = ?3, retry_count = ?4
                 WHERE id = ?1",
                params![id, status, next_attempt, retry_count],
            )?;
            Ok(())
        })
    }

    pub fn delete_email(&self, id: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute("DELETE FROM stalwart_smtp_queue WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// Record a terminal SMTP delivery outcome (success or permanent failure) so the
    /// outbound module can reconcile send_status without polling the queue table that
    /// has already been deleted from.
    pub fn record_delivery_outcome(
        &self,
        id: &str,
        from_addr: &str,
        to_addr: &str,
        outcome: &str,
        error_text: Option<&str>,
        completed_at: i64,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO stalwart_smtp_delivery_log
                    (id, from_addr, to_addr, outcome, error_text, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, from_addr, to_addr, outcome, error_text, completed_at],
            )?;
            Ok(())
        })
    }

    // --- CalDAV Operations ---

    pub fn create_calendar(&self, id: &str, owner: &str, display_name: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO stalwart_caldav_calendars (id, owner, display_name, description)
                 VALUES (?1, ?2, ?3, '')",
                params![id, owner, display_name],
            )?;
            Ok(())
        })
    }

    pub fn get_calendars(&self, owner: &str) -> StalwartResult<Vec<(String, String, String)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn put_event(&self, calendar_id: &str, uid: &str, ical_data: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            let now = crate::util::now_utc_secs();
            let id = format!("{}:{}", calendar_id, uid);
            conn.execute(
                "INSERT OR REPLACE INTO stalwart_caldav_events (id, calendar_id, uid, ical_data, last_modified)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, calendar_id, uid, ical_data, now],
            )?;
            Ok(())
        })
    }

    pub fn get_events(
        &self,
        calendar_id: &str,
    ) -> StalwartResult<Vec<(String, String, String, u64)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn get_event(&self, calendar_id: &str, uid: &str) -> StalwartResult<Option<(String, u64)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn delete_event(&self, calendar_id: &str, uid: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            let id = format!("{}:{}", calendar_id, uid);
            conn.execute(
                "DELETE FROM stalwart_caldav_events WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }

    // --- CardDAV Operations ---

    pub fn create_addressbook(
        &self,
        id: &str,
        owner: &str,
        display_name: &str,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO stalwart_carddav_addressbooks (id, owner, display_name, description)
                 VALUES (?1, ?2, ?3, '')",
                params![id, owner, display_name],
            )?;
            Ok(())
        })
    }

    pub fn get_addressbooks(&self, owner: &str) -> StalwartResult<Vec<(String, String, String)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn put_contact(
        &self,
        addressbook_id: &str,
        uid: &str,
        vcard_data: &str,
    ) -> StalwartResult<()> {
        self.with_connection(|conn| {
            let now = crate::util::now_utc_secs();
            let id = format!("{}:{}", addressbook_id, uid);
            conn.execute(
                "INSERT OR REPLACE INTO stalwart_carddav_contacts (id, addressbook_id, uid, vcard_data, last_modified)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, addressbook_id, uid, vcard_data, now],
            )?;
            Ok(())
        })
    }

    pub fn get_contacts(
        &self,
        addressbook_id: &str,
    ) -> StalwartResult<Vec<(String, String, String, u64)>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn delete_contact(&self, addressbook_id: &str, uid: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            let id = format!("{}:{}", addressbook_id, uid);
            conn.execute(
                "DELETE FROM stalwart_carddav_contacts WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }

    // --- User & Mailbox Operations ---

    pub fn add_user(&self, username: &str, password: &str) -> StalwartResult<()> {
        let password_hash = hash_password(password);
        self.with_connection(|conn| {
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
        })
    }

    pub fn authenticate_user(&self, username: &str, password: &str) -> StalwartResult<bool> {
        let stored: Option<String> = self.with_connection(|conn| {
            let mut stmt =
                conn.prepare("SELECT password_hash FROM stalwart_users WHERE username = ?1")?;
            let mut rows = stmt.query(params![username])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get(0)?)),
                None => Ok(None),
            }
        })?;
        let Some(stored) = stored else {
            return Ok(false);
        };
        if let Some(parsed) = parse_password_hash(&stored) {
            return Ok(verify_password(password, &parsed));
        }
        // Legacy row from before hashing: the column held the raw password.
        // Verify in constant time, then rewrite the row hashed so the
        // plaintext disappears on first successful login.
        let matches = constant_time_eq(stored.as_bytes(), password.as_bytes());
        if matches {
            let upgraded = hash_password(password);
            self.with_connection(|conn| {
                conn.execute(
                    "UPDATE stalwart_users SET password_hash = ?1 WHERE username = ?2",
                    params![upgraded, username],
                )?;
                Ok(())
            })?;
        }
        Ok(matches)
    }

    pub fn user_exists(&self, username: &str) -> StalwartResult<bool> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT 1 FROM stalwart_users WHERE username = ?1")?;
            let mut rows = stmt.query(params![username])?;
            Ok(rows.next()?.is_some())
        })
    }

    pub fn get_mailboxes(&self, owner: &str) -> StalwartResult<Vec<(String, String)>> {
        self.with_connection(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, name FROM stalwart_mailboxes WHERE owner = ?1")?;
            let rows = stmt.query_map(params![owner], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut res = Vec::new();
            for r in rows {
                res.push(r?);
            }
            Ok(res)
        })
    }

    pub fn get_mailbox_id(&self, owner: &str, name: &str) -> StalwartResult<Option<String>> {
        self.with_connection(|conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM stalwart_mailboxes WHERE owner = ?1 AND name = ?2")?;
            let mut rows = stmt.query(params![owner, name])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
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
        let id = crate::util::generate_unique_id();
        let now = crate::util::now_utc_secs();
        self.with_connection(|conn| {
            // Allocate the next persistent IMAP UID for this mailbox in one
            // atomic statement — clients key their local cache on (uidvalidity,
            // uid), so UIDs must never be re-derived from list positions.
            let uid: i64 = conn.query_row(
                "UPDATE stalwart_mailboxes SET uid_next = uid_next + 1
                 WHERE id = ?1 RETURNING uid_next - 1",
                params![mailbox_id],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO stalwart_messages (id, mailbox_id, from_addr, to_addr, subject, body, headers, is_read, received_at, uid, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, 0)",
                params![id.as_str(), mailbox_id, from_addr, to_addr, subject, body, headers, now, uid],
            )?;
            Ok(id)
        })
    }

    pub fn get_messages(
        &self,
        mailbox_id: &str,
    ) -> StalwartResult<
        Vec<(
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            bool,
            u64,
        )>,
    > {
        self.with_connection(|conn| {
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
        })
    }

    pub fn count_messages(&self, mailbox_id: &str) -> StalwartResult<usize> {
        self.with_connection(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM stalwart_messages WHERE mailbox_id = ?1",
                params![mailbox_id],
                |row| row.get(0),
            )?;
            Ok(count.max(0) as usize)
        })
    }

    pub fn get_message_summaries(&self, mailbox_id: &str) -> StalwartResult<Vec<MessageSummary>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, from_addr, to_addr, subject, is_read, received_at, uid, is_deleted
                 FROM stalwart_messages
                 WHERE mailbox_id = ?1
                 ORDER BY received_at DESC, id ASC",
            )?;
            let rows = stmt.query_map(params![mailbox_id], |row| {
                let is_read_int: i32 = row.get(4)?;
                let is_deleted_int: i32 = row.get(7)?;
                Ok(MessageSummary {
                    id: row.get(0)?,
                    from_addr: row.get(1)?,
                    to_addr: row.get(2)?,
                    subject: row.get(3)?,
                    is_read: is_read_int != 0,
                    received_at: row.get(5)?,
                    uid: row.get(6)?,
                    is_deleted: is_deleted_int != 0,
                })
            })?;
            let mut res = Vec::new();
            for row in rows {
                res.push(row?);
            }
            Ok(res)
        })
    }

    pub fn get_message_content(&self, id: &str) -> StalwartResult<Option<MessageContent>> {
        self.with_connection(|conn| {
            let mut stmt =
                conn.prepare("SELECT body, headers FROM stalwart_messages WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(MessageContent {
                    body: row.get(0)?,
                    headers: row.get(1)?,
                }))
            } else {
                Ok(None)
            }
        })
    }

    pub fn update_message_flags(&self, id: &str, is_read: bool) -> StalwartResult<()> {
        self.with_connection(|conn| {
            let is_read_int = if is_read { 1 } else { 0 };
            conn.execute(
                "UPDATE stalwart_messages SET is_read = ?2 WHERE id = ?1",
                params![id, is_read_int],
            )?;
            Ok(())
        })
    }

    pub fn delete_message(&self, id: &str) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute("DELETE FROM stalwart_messages WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    /// IMAP `\Deleted` semantics: flag only — the message stays visible (with
    /// the flag) until an explicit EXPUNGE removes it.
    pub fn mark_message_deleted(&self, id: &str, deleted: bool) -> StalwartResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                "UPDATE stalwart_messages SET is_deleted = ?2 WHERE id = ?1",
                params![id, if deleted { 1 } else { 0 }],
            )?;
            Ok(())
        })
    }

    /// (uid_validity, uid_next) for the SELECT response.
    pub fn mailbox_uid_state(&self, mailbox_id: &str) -> StalwartResult<(i64, i64)> {
        self.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT uid_validity, uid_next FROM stalwart_mailboxes WHERE id = ?1",
                params![mailbox_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
    }

    pub fn check_greylist(&self, ip: &str, sender: &str, recipient: &str) -> StalwartResult<bool> {
        if ip == "127.0.0.1" || ip == "::1" || ip.starts_with("127.") || ip == "localhost" {
            return Ok(true);
        }

        self.with_connection(|conn| {
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
        })
    }
}

fn sqlite_store_change_stamp(path: &Path) -> SqliteStoreChangeStamp {
    SqliteStoreChangeStamp {
        main: sqlite_file_change_stamp(path),
        wal: sqlite_file_change_stamp(&sqlite_sidecar_path(path, "-wal")),
        shm: sqlite_file_change_stamp(&sqlite_sidecar_path(path, "-shm")),
    }
}

fn sqlite_file_change_stamp(path: &Path) -> SqliteFileChangeStamp {
    match std::fs::metadata(path) {
        Ok(metadata) => SqliteFileChangeStamp {
            exists: true,
            len: metadata.len(),
            modified_ns: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0),
        },
        Err(_) => SqliteFileChangeStamp {
            exists: false,
            len: 0,
            modified_ns: 0,
        },
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
fn record_open_connection_for_test(db_path: &str) {
    let counts = SQLITE_STORE_OPEN_COUNTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut counts = counts.lock().unwrap_or_else(|err| err.into_inner());
    *counts.entry(db_path.to_string()).or_insert(0) += 1;
}

#[cfg(not(test))]
fn record_open_connection_for_test(_db_path: &str) {}

/// One-time schema upgrade for databases created before persistent IMAP UIDs:
/// adds the uid/is_deleted/uid_validity/uid_next columns and backfills UIDs in
/// chronological order per mailbox so existing clients resync onto stable IDs.
fn migrate_message_uids(conn: &Connection) -> StalwartResult<()> {
    let has_column = |table: &str, column: &str| -> StalwartResult<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for name in names {
            if name? == column {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if !has_column("stalwart_mailboxes", "uid_validity")? {
        conn.execute_batch(
            "ALTER TABLE stalwart_mailboxes ADD COLUMN uid_validity INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE stalwart_mailboxes ADD COLUMN uid_next INTEGER NOT NULL DEFAULT 1;",
        )?;
    }
    if !has_column("stalwart_messages", "uid")? {
        conn.execute_batch(
            "ALTER TABLE stalwart_messages ADD COLUMN uid INTEGER;
             ALTER TABLE stalwart_messages ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    conn.execute(
        "UPDATE stalwart_messages SET uid = (
             SELECT COUNT(*) FROM stalwart_messages older
             WHERE older.mailbox_id = stalwart_messages.mailbox_id
               AND (older.received_at < stalwart_messages.received_at
                    OR (older.received_at = stalwart_messages.received_at
                        AND older.id <= stalwart_messages.id))
         )
         WHERE uid IS NULL",
        [],
    )?;
    conn.execute(
        "UPDATE stalwart_mailboxes SET uid_next = COALESCE(
             (SELECT MAX(uid) + 1 FROM stalwart_messages
              WHERE stalwart_messages.mailbox_id = stalwart_mailboxes.id),
             uid_next
         )
         WHERE uid_next < COALESCE(
             (SELECT MAX(uid) + 1 FROM stalwart_messages
              WHERE stalwart_messages.mailbox_id = stalwart_mailboxes.id),
             1
         )",
        [],
    )?;
    Ok(())
}

// --- Password hashing ---
//
// Stored format: `pbkdf2-sha256$<iterations>$<salt-b64>$<derived-key-b64>`.
// Rows without this prefix are legacy plaintext and get rewritten hashed on
// their first successful authentication.

const PASSWORD_HASH_PREFIX: &str = "pbkdf2-sha256";
const PASSWORD_PBKDF2_ITERATIONS: u32 = 100_000;
const PASSWORD_SALT_LEN: usize = 16;
const PASSWORD_KEY_LEN: usize = 32;

struct ParsedPasswordHash {
    iterations: std::num::NonZeroU32,
    salt: Vec<u8>,
    derived_key: Vec<u8>,
}

fn constant_time_eq(expected: &[u8], presented: &[u8]) -> bool {
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"ctox-constant-time-equality-v1");
    let expected_tag = hmac::sign(&key, expected);
    hmac::verify(&key, presented, expected_tag.as_ref()).is_ok()
}

fn hash_password(password: &str) -> String {
    use base64::Engine as _;
    use ring::rand::SecureRandom as _;
    let mut salt = [0u8; PASSWORD_SALT_LEN];
    ring::rand::SystemRandom::new()
        .fill(&mut salt)
        .expect("system randomness for password salt");
    let mut derived_key = [0u8; PASSWORD_KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(PASSWORD_PBKDF2_ITERATIONS).expect("non-zero iterations"),
        &salt,
        password.as_bytes(),
        &mut derived_key,
    );
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD;
    format!(
        "{PASSWORD_HASH_PREFIX}${PASSWORD_PBKDF2_ITERATIONS}${}${}",
        b64.encode(salt),
        b64.encode(derived_key)
    )
}

fn parse_password_hash(stored: &str) -> Option<ParsedPasswordHash> {
    use base64::Engine as _;
    let mut parts = stored.split('$');
    if parts.next()? != PASSWORD_HASH_PREFIX {
        return None;
    }
    let iterations = std::num::NonZeroU32::new(parts.next()?.parse().ok()?)?;
    let b64 = base64::engine::general_purpose::STANDARD_NO_PAD;
    let salt = b64.decode(parts.next()?).ok()?;
    let derived_key = b64.decode(parts.next()?).ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(ParsedPasswordHash {
        iterations,
        salt,
        derived_key,
    })
}

fn verify_password(password: &str, parsed: &ParsedPasswordHash) -> bool {
    ring::pbkdf2::verify(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        parsed.iterations,
        &parsed.salt,
        password.as_bytes(),
        &parsed.derived_key,
    )
    .is_ok()
}

#[cfg(test)]
fn clear_connection_cache_for_test(db_path: &str) {
    SQLITE_STORE_CONNECTIONS.with(|connections| {
        connections.borrow_mut().remove(db_path);
    });
}

#[cfg(test)]
fn reset_open_count_for_test(db_path: &str) {
    let counts = SQLITE_STORE_OPEN_COUNTS.get_or_init(|| Mutex::new(HashMap::new()));
    counts
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .remove(db_path);
}

#[cfg(test)]
fn open_count_for_test(db_path: &str) -> usize {
    SQLITE_STORE_OPEN_COUNTS
        .get()
        .and_then(|counts| {
            counts
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .get(db_path)
                .copied()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> StalwartResult<(tempfile::TempDir, SqliteStore, String)> {
        let temp = tempdir()?;
        let db_path = temp.path().join("mail.sqlite3");
        let store = SqliteStore::new(db_path.to_str().expect("utf8 temp db path"));
        store.init()?;
        store.add_user("alice@example.test", "hash")?;
        let inbox = store
            .get_mailbox_id("alice@example.test", "INBOX")?
            .expect("inbox mailbox");
        Ok((temp, store, inbox))
    }

    #[test]
    fn message_summaries_and_counts_do_not_load_message_content() -> StalwartResult<()> {
        let (_temp, store, inbox) = test_store()?;
        let message_id = store.put_message(
            &inbox,
            "sender@example.test",
            "alice@example.test",
            Some("large body"),
            &"x".repeat(128 * 1024),
            Some("From: sender@example.test\r\nTo: alice@example.test\r\nSubject: large body"),
        )?;

        assert_eq!(store.count_messages(&inbox)?, 1);
        let summaries = store.get_message_summaries(&inbox)?;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, message_id);
        assert_eq!(summaries[0].subject.as_deref(), Some("large body"));
        assert!(!summaries[0].is_read);

        let content = store
            .get_message_content(&message_id)?
            .expect("message content");
        assert_eq!(content.body.len(), 128 * 1024);
        assert!(content.headers.as_deref().unwrap_or("").contains("Subject"));
        Ok(())
    }

    #[test]
    fn message_uids_are_persistent_across_deletions() -> StalwartResult<()> {
        let (_temp, store, inbox) = test_store()?;
        let first =
            store.put_message(&inbox, "a@x", "alice@example.test", Some("one"), "1", None)?;
        store.put_message(&inbox, "a@x", "alice@example.test", Some("two"), "2", None)?;
        store.put_message(
            &inbox,
            "a@x",
            "alice@example.test",
            Some("three"),
            "3",
            None,
        )?;
        store.delete_message(&first)?;
        store.put_message(&inbox, "a@x", "alice@example.test", Some("four"), "4", None)?;
        let mut uids: Vec<i64> = store
            .get_message_summaries(&inbox)?
            .iter()
            .map(|m| m.uid)
            .collect();
        uids.sort_unstable();
        // UIDs 2 and 3 survive the deletion unchanged; the new message gets 4,
        // and UID 1 is never reused.
        assert_eq!(uids, vec![2, 3, 4]);
        let (_validity, uid_next) = store.mailbox_uid_state(&inbox)?;
        assert_eq!(uid_next, 5);

        store.mark_message_deleted(&store.get_message_summaries(&inbox)?[0].id.clone(), true)?;
        assert_eq!(
            store
                .get_message_summaries(&inbox)?
                .iter()
                .filter(|m| m.is_deleted)
                .count(),
            1,
            "\\Deleted flags instead of removing"
        );
        Ok(())
    }

    #[test]
    fn legacy_databases_get_uids_backfilled_chronologically() -> StalwartResult<()> {
        let temp = tempdir()?;
        let db_path = temp.path().join("legacy.sqlite3");
        {
            let conn = Connection::open(&db_path)?;
            conn.execute_batch(
                "CREATE TABLE stalwart_mailboxes (
                     id TEXT PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL,
                     UNIQUE(owner, name));
                 CREATE TABLE stalwart_messages (
                     id TEXT PRIMARY KEY, mailbox_id TEXT NOT NULL,
                     from_addr TEXT NOT NULL, to_addr TEXT NOT NULL, subject TEXT,
                     body TEXT NOT NULL, headers TEXT,
                     is_read INTEGER NOT NULL DEFAULT 0,
                     received_at INTEGER NOT NULL);
                 INSERT INTO stalwart_mailboxes VALUES ('inbox1', 'u@x', 'INBOX');
                 INSERT INTO stalwart_messages
                     (id, mailbox_id, from_addr, to_addr, subject, body, received_at)
                     VALUES ('m_new', 'inbox1', 'a', 'b', 'newer', '.', 200),
                            ('m_old', 'inbox1', 'a', 'b', 'older', '.', 100);",
            )?;
        }
        let store = SqliteStore::new(db_path.to_str().expect("utf8 path"));
        store.init()?;
        let summaries = store.get_message_summaries("inbox1")?;
        let by_subject: std::collections::HashMap<_, _> = summaries
            .iter()
            .map(|m| (m.subject.clone().unwrap_or_default(), m.uid))
            .collect();
        assert_eq!(by_subject["older"], 1, "oldest message gets UID 1");
        assert_eq!(by_subject["newer"], 2);
        assert_eq!(store.mailbox_uid_state("inbox1")?, (1, 3));
        Ok(())
    }

    #[test]
    fn passwords_are_stored_hashed_and_verify_roundtrip() -> StalwartResult<()> {
        let (_temp, store, _inbox) = test_store()?;
        store.add_user("carol@example.test", "s3cret pa55word")?;
        let stored: String = store.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT password_hash FROM stalwart_users WHERE username = 'carol@example.test'",
                [],
                |row| row.get(0),
            )?)
        })?;
        assert!(
            !stored.contains("s3cret pa55word"),
            "raw password must not appear in the stored credential: {stored}"
        );
        assert!(store.authenticate_user("carol@example.test", "s3cret pa55word")?);
        assert!(!store.authenticate_user("carol@example.test", "wrong")?);
        assert!(!store.authenticate_user("nobody@example.test", "s3cret pa55word")?);
        Ok(())
    }

    #[test]
    fn legacy_plaintext_rows_authenticate_once_and_upgrade_to_hash() -> StalwartResult<()> {
        let (_temp, store, _inbox) = test_store()?;
        store.with_connection(|conn| {
            conn.execute(
                "INSERT INTO stalwart_users (username, password_hash, created_at)
                 VALUES ('legacy@example.test', 'oldplain', 0)",
                [],
            )?;
            Ok(())
        })?;
        assert!(!store.authenticate_user("legacy@example.test", "wrong")?);
        assert!(store.authenticate_user("legacy@example.test", "oldplain")?);
        let stored: String = store.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT password_hash FROM stalwart_users WHERE username = 'legacy@example.test'",
                [],
                |row| row.get(0),
            )?)
        })?;
        assert_ne!(
            stored, "oldplain",
            "plaintext must be gone after first login"
        );
        assert!(store.authenticate_user("legacy@example.test", "oldplain")?);
        Ok(())
    }

    #[test]
    fn runtime_settings_roundtrip_without_environment_state() -> StalwartResult<()> {
        let (_temp, store, _inbox) = test_store()?;
        let settings = MailserverRuntimeSettings {
            enabled: false,
            hostname: "mail.example.test".to_string(),
            bind_host: "127.0.0.2".to_string(),
            smtp_port: 2626,
            imap_port: 1243,
            outbound_throttle_per_min: 42,
            max_connections: 7,
            tracking_base_url: "https://mail.example.test".to_string(),
        };
        store.save_runtime_settings(&settings)?;
        assert_eq!(store.load_runtime_settings()?, settings);
        Ok(())
    }

    #[test]
    fn tracking_tokens_resolve_to_append_only_events() -> StalwartResult<()> {
        let (_temp, store, _inbox) = test_store()?;
        store.save_tracking_token(
            "click-token",
            "message-1",
            Some("campaign-1"),
            "clicked",
            Some("https://example.test/offer"),
        )?;
        let token = store
            .tracking_token("click-token")?
            .expect("tracking token");
        assert_eq!(token.message_id, "message-1");
        assert_eq!(token.event_type, "clicked");
        assert_eq!(
            token.target_url.as_deref(),
            Some("https://example.test/offer")
        );
        store.record_tracking_event(
            "click-token",
            "message-1",
            "clicked",
            1234,
            Some("test-agent"),
        )?;
        let count: i64 = store.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM stalwart_mail_tracking_events WHERE message_id = 'message-1'",
                [],
                |row| row.get(0),
            )?)
        })?;
        assert_eq!(count, 1);
        Ok(())
    }
}
