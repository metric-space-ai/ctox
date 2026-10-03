use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use crate::channels::record_channel_db_open_for_tests;
use crate::channels::{ensure_open_routing_rows_once, ensure_schema_once};

pub(crate) fn open_channel_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create db parent {}", parent.display()))?;
    }
    #[cfg(test)]
    record_channel_db_open_for_tests(path);
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open channel db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for channels")?;
    ensure_schema_once(path, &conn)?;
    ensure_open_routing_rows_once(path, &conn)?;
    Ok(conn)
}

pub(crate) struct UpsertMessage<'a> {
    pub message_key: &'a str,
    pub channel: &'a str,
    pub account_key: &'a str,
    pub thread_key: &'a str,
    pub remote_id: &'a str,
    pub direction: &'a str,
    pub folder_hint: &'a str,
    pub sender_display: &'a str,
    pub sender_address: &'a str,
    pub recipient_addresses_json: &'a str,
    pub cc_addresses_json: &'a str,
    pub bcc_addresses_json: &'a str,
    pub subject: &'a str,
    pub preview: &'a str,
    pub body_text: &'a str,
    pub body_html: &'a str,
    pub raw_payload_ref: &'a str,
    pub trust_level: &'a str,
    pub status: &'a str,
    pub seen: bool,
    pub has_attachments: bool,
    pub external_created_at: &'a str,
    pub observed_at: &'a str,
    pub metadata_json: &'a str,
}

pub(crate) fn upsert_communication_message(
    conn: &mut Connection,
    message: UpsertMessage<'_>,
) -> Result<()> {
    // IMMEDIATE: read-then-write must not start as a WAL reader (see
    // channels::ensure_account).
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    upsert_communication_message_tx(&tx, message)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn upsert_communication_message_tx(
    tx: &Transaction<'_>,
    message: UpsertMessage<'_>,
) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO communication_messages (
            message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
            sender_display, sender_address, recipient_addresses_json, cc_addresses_json, bcc_addresses_json,
            subject, preview, body_text, body_html, raw_payload_ref, trust_level, status, seen,
            has_attachments, external_created_at, observed_at, metadata_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24
        )
        ON CONFLICT(message_key) DO UPDATE SET
            channel=excluded.channel,
            account_key=excluded.account_key,
            thread_key=excluded.thread_key,
            remote_id=excluded.remote_id,
            direction=excluded.direction,
            folder_hint=excluded.folder_hint,
            sender_display=excluded.sender_display,
            sender_address=excluded.sender_address,
            recipient_addresses_json=excluded.recipient_addresses_json,
            cc_addresses_json=excluded.cc_addresses_json,
            bcc_addresses_json=excluded.bcc_addresses_json,
            subject=excluded.subject,
            preview=excluded.preview,
            body_text=excluded.body_text,
            body_html=excluded.body_html,
            raw_payload_ref=excluded.raw_payload_ref,
            trust_level=excluded.trust_level,
            status=excluded.status,
            seen=excluded.seen,
            has_attachments=excluded.has_attachments,
            external_created_at=excluded.external_created_at,
            observed_at=excluded.observed_at,
            metadata_json=excluded.metadata_json
        "#,
        params![
            message.message_key,
            message.channel,
            message.account_key,
            message.thread_key,
            message.remote_id,
            message.direction,
            message.folder_hint,
            message.sender_display,
            message.sender_address,
            message.recipient_addresses_json,
            message.cc_addresses_json,
            message.bcc_addresses_json,
            message.subject,
            message.preview,
            message.body_text,
            message.body_html,
            message.raw_payload_ref,
            message.trust_level,
            message.status,
            if message.seen { 1 } else { 0 },
            if message.has_attachments { 1 } else { 0 },
            message.external_created_at,
            message.observed_at,
            message.metadata_json,
        ],
    )?;
    Ok(())
}

pub(crate) fn refresh_thread(conn: &mut Connection, thread_key: &str) -> Result<()> {
    // IMMEDIATE: read-then-write must not start as a WAL reader (see
    // channels::ensure_account).
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    refresh_thread_tx(&tx, thread_key)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn refresh_thread_tx(tx: &Transaction<'_>, thread_key: &str) -> Result<()> {
    let summary = tx
        .query_row(
            r#"
            SELECT
                channel,
                account_key,
                subject,
                message_key,
                external_created_at
            FROM communication_messages
            WHERE thread_key = ?1
            ORDER BY external_created_at DESC, observed_at DESC
            LIMIT 1
            "#,
            params![thread_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((channel, account_key, subject, last_message_key, last_message_at)) = summary else {
        return Ok(());
    };

    let message_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE thread_key = ?1",
        params![thread_key],
        |row| row.get(0),
    )?;
    let unread_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE thread_key = ?1 AND direction = 'inbound' AND seen = 0",
        params![thread_key],
        |row| row.get(0),
    )?;
    let mut participants = BTreeSet::new();
    let mut participant_stmt = tx.prepare(
        r#"
        SELECT sender_address, recipient_addresses_json, cc_addresses_json
        FROM communication_messages
        WHERE thread_key = ?1
        "#,
    )?;
    let participant_rows = participant_stmt.query_map(params![thread_key], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in participant_rows {
        let (sender, recipients_json, cc_json) = row?;
        if !sender.trim().is_empty() {
            participants.insert(sender);
        }
        for value in parse_string_json_array(&recipients_json) {
            participants.insert(value);
        }
        for value in parse_string_json_array(&cc_json) {
            participants.insert(value);
        }
    }

    tx.execute(
        r#"
        INSERT INTO communication_threads (
            thread_key, channel, account_key, subject, participant_keys_json, last_message_key,
            last_message_at, message_count, unread_count, metadata_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(thread_key) DO UPDATE SET
            channel=excluded.channel,
            account_key=excluded.account_key,
            subject=excluded.subject,
            participant_keys_json=excluded.participant_keys_json,
            last_message_key=excluded.last_message_key,
            last_message_at=excluded.last_message_at,
            message_count=excluded.message_count,
            unread_count=excluded.unread_count,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            thread_key,
            channel,
            account_key,
            subject,
            serde_json::to_string(&participants.into_iter().collect::<Vec<_>>())?,
            last_message_key,
            last_message_at,
            message_count,
            unread_count,
            r#"{"refreshed_by":"ctox-channel-router"}"#,
            now_iso_string(),
        ],
    )?;
    Ok(())
}

pub(crate) fn preview_text(body: &str, subject: &str) -> String {
    let source = if body.trim().is_empty() {
        subject
    } else {
        body
    };
    let collapsed = source.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(280).collect()
}

pub(crate) fn parse_string_json_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub(crate) fn now_iso_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono_like_iso(now)
}

fn chrono_like_iso(epoch_seconds: u64) -> String {
    // Minimal UTC ISO-8601 formatter without adding a new dependency to the top-level crate.
    use std::fmt::Write as _;

    let seconds_per_day = 86_400u64;
    let days = epoch_seconds / seconds_per_day;
    let seconds_of_day = epoch_seconds % seconds_per_day;

    let (year, month, day) = civil_from_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let mut output = String::with_capacity(20);
    let _ = write!(
        output,
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    );
    output
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}
