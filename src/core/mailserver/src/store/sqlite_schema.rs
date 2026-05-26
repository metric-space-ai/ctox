// ref: stalwart/src/store/sqlite/schema.rs:1-50
// ref: ctox-mailserver new code for unified campaign/collaboration sqlite schema

pub const SQLITE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS stalwart_domains (
    domain_name TEXT PRIMARY KEY,
    dkim_selector TEXT NOT NULL,
    dkim_private_key TEXT NOT NULL,
    spf_record TEXT,
    dmarc_record TEXT
);

CREATE TABLE IF NOT EXISTS stalwart_smtp_queue (
    id TEXT PRIMARY KEY,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    msg_body TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL,
    status TEXT NOT NULL
);

-- Append-only delivery log so the outbound module can reconcile real send
-- outcomes back into outbound_messages.send_status after the SMTP queue row is
-- gone. One row per terminal delivery attempt (success or permanent failure).
CREATE TABLE IF NOT EXISTS stalwart_smtp_delivery_log (
    id TEXT NOT NULL,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    outcome TEXT NOT NULL,
    error_text TEXT,
    completed_at INTEGER NOT NULL,
    PRIMARY KEY (id, completed_at)
);
CREATE INDEX IF NOT EXISTS stalwart_smtp_delivery_log_id_idx
    ON stalwart_smtp_delivery_log (id);

CREATE TABLE IF NOT EXISTS stalwart_caldav_calendars (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS stalwart_caldav_events (
    id TEXT PRIMARY KEY,
    calendar_id TEXT NOT NULL,
    uid TEXT NOT NULL,
    ical_data TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    FOREIGN KEY(calendar_id) REFERENCES stalwart_caldav_calendars(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS stalwart_carddav_addressbooks (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS stalwart_carddav_contacts (
    id TEXT PRIMARY KEY,
    addressbook_id TEXT NOT NULL,
    uid TEXT NOT NULL,
    vcard_data TEXT NOT NULL,
    last_modified INTEGER NOT NULL,
    FOREIGN KEY(addressbook_id) REFERENCES stalwart_carddav_addressbooks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS stalwart_users (
    username TEXT PRIMARY KEY,
    password_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS stalwart_mailboxes (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    UNIQUE(owner, name)
);

CREATE TABLE IF NOT EXISTS stalwart_messages (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    subject TEXT,
    body TEXT NOT NULL,
    headers TEXT,
    is_read INTEGER NOT NULL DEFAULT 0,
    received_at INTEGER NOT NULL,
    FOREIGN KEY(mailbox_id) REFERENCES stalwart_mailboxes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS stalwart_greylist (
    ip TEXT,
    sender TEXT,
    recipient TEXT,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY(ip, sender, recipient)
);
"#;
