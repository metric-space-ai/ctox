use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use rusqlite::params;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::communication::adapters as communication_adapters;
use crate::communication::adapters::CommunicationTransportAdapter;
use crate::communication::gateway as communication_gateway;
use crate::secrets;
use crate::service::core_state_machine::{
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
};
use crate::service::core_transition_guard::{
    enforce_core_spawn, enforce_core_transition, CoreSpawnRequest,
};
use crate::service::harness_flow::{
    record_harness_flow_event_lossy, RecordHarnessFlowEventRequest,
};

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_TAKE_LIMIT: usize = 10;
const QUEUE_CHANNEL_NAME: &str = "queue";
const QUEUE_ACCOUNT_KEY: &str = "queue:system";
const QUEUE_ACCOUNT_ADDRESS: &str = "ctox queue";
const QUEUE_PROVIDER: &str = "system";
const QUEUE_SENDER_DISPLAY: &str = "CTOX queue";
const QUEUE_SENDER_ADDRESS: &str = "queue:system";
static REVIEWED_FOUNDER_SEND_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct QueueTaskView {
    pub message_key: String,
    pub thread_key: String,
    pub title: String,
    pub prompt: String,
    pub workspace_root: Option<String>,
    pub ticket_self_work_id: Option<String>,
    pub priority: String,
    pub suggested_skill: Option<String>,
    pub parent_message_key: Option<String>,
    pub route_status: String,
    pub status_note: Option<String>,
    pub lease_owner: Option<String>,
    pub leased_at: Option<String>,
    pub acked_at: Option<String>,
    pub created_at: String,
    pub sort_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct QueueTaskCreateRequest {
    pub title: String,
    pub prompt: String,
    pub thread_key: String,
    pub workspace_root: Option<String>,
    pub priority: String,
    pub suggested_skill: Option<String>,
    pub parent_message_key: Option<String>,
    pub extra_metadata: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct QueueTaskUpdateRequest {
    pub message_key: String,
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub thread_key: Option<String>,
    pub workspace_root: Option<String>,
    pub clear_workspace_root: bool,
    pub priority: Option<String>,
    pub suggested_skill: Option<String>,
    pub clear_skill: bool,
    pub route_status: Option<String>,
    pub status_note: Option<String>,
    pub clear_note: bool,
}

pub struct OwnerPromptContext {
    pub owner_name: String,
    pub owner_email_address: Option<String>,
    pub founder_email_addresses: Vec<String>,
    pub founder_email_roles: Vec<String>,
    pub allowed_email_domain: Option<String>,
    pub admin_email_policies: Vec<String>,
    pub channels: Vec<String>,
    pub preferred_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EmailSenderPolicy {
    pub normalized_email: String,
    pub role: String,
    pub allowed: bool,
    pub allow_admin_actions: bool,
    pub allow_sudo_actions: bool,
    pub secrets_via_email_allowed: bool,
    pub allowed_email_domain: Option<String>,
    pub block_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdminEmailPolicy {
    email: String,
    can_sudo: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunicationFeedItem {
    pub message_key: String,
    pub channel: String,
    pub direction: String,
    pub sender_display: String,
    pub sender_address: String,
    pub subject: String,
    pub preview: String,
    pub thread_key: String,
    pub route_status: String,
    pub external_created_at: String,
}

pub fn sync_prompt_identity(root: &Path, settings: &BTreeMap<String, String>) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    if let Some(owner_name) = settings
        .get("CTOX_OWNER_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        upsert_owner_profile(&mut conn, owner_name)?;
    }
    sync_identity_profiles(&mut conn, settings)?;

    if let Some(email_address) = settings
        .get("CTO_EMAIL_ADDRESS")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let provider = settings
            .get("CTO_EMAIL_PROVIDER")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("imap");
        let profile_json = json!({
            "imapHost": settings.get("CTO_EMAIL_IMAP_HOST").map(|value| value.trim()).unwrap_or(""),
            "imapPort": settings.get("CTO_EMAIL_IMAP_PORT").map(|value| value.trim()).unwrap_or(""),
            "smtpHost": settings.get("CTO_EMAIL_SMTP_HOST").map(|value| value.trim()).unwrap_or(""),
            "smtpPort": settings.get("CTO_EMAIL_SMTP_PORT").map(|value| value.trim()).unwrap_or(""),
            "graphUser": settings.get("CTO_EMAIL_GRAPH_USER").map(|value| value.trim()).unwrap_or(""),
            "ewsUrl": settings.get("CTO_EMAIL_EWS_URL").map(|value| value.trim()).unwrap_or(""),
            "ewsAuthType": settings.get("CTO_EMAIL_EWS_AUTH_TYPE").map(|value| value.trim()).unwrap_or(""),
            "ewsUsername": settings.get("CTO_EMAIL_EWS_USERNAME").map(|value| value.trim()).unwrap_or(""),
        });
        ensure_account(
            &mut conn,
            &format!("email:{email_address}"),
            "email",
            email_address,
            provider,
            profile_json,
        )?;
    }

    if let Some(jami_account_id) = settings
        .get("CTO_JAMI_ACCOUNT_ID")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let profile_name = settings
            .get("CTO_JAMI_PROFILE_NAME")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(jami_account_id);
        let profile_json = json!({
            "accountId": jami_account_id,
            "profileName": profile_name,
            "inboxDir": settings.get("CTO_JAMI_INBOX_DIR").map(|value| value.trim()).unwrap_or(""),
            "outboxDir": settings.get("CTO_JAMI_OUTBOX_DIR").map(|value| value.trim()).unwrap_or(""),
            "archiveDir": settings.get("CTO_JAMI_ARCHIVE_DIR").map(|value| value.trim()).unwrap_or(""),
            "dbusEnvFile": settings.get("CTO_JAMI_DBUS_ENV_FILE").map(|value| value.trim()).unwrap_or(""),
        });
        ensure_account(
            &mut conn,
            &format!("jami:{jami_account_id}"),
            "jami",
            profile_name,
            "jami",
            profile_json,
        )?;
    }

    Ok(())
}

pub fn merge_owner_profile_settings(
    root: &Path,
    settings: &mut BTreeMap<String, String>,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT owner_key, metadata_json
        FROM owner_profiles
        ORDER BY owner_key ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut founder_emails = parse_founder_email_addresses(settings)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut founder_roles = parse_founder_email_roles(settings);
    let mut admin_policies = parse_admin_email_policies(settings)
        .into_iter()
        .map(|entry| (entry.email, entry.can_sudo))
        .collect::<BTreeMap<_, _>>();

    for row in rows {
        let (owner_key, metadata_json) = row?;
        let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
        let email = metadata
            .get("email")
            .and_then(Value::as_str)
            .map(normalize_email_address)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let normalized = normalize_email_address(&owner_key);
                normalized.contains('@').then_some(normalized)
            });
        let Some(email) = email else {
            continue;
        };
        let role = metadata
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match role.as_str() {
            "owner" => {
                settings
                    .entry("CTOX_OWNER_EMAIL_ADDRESS".to_string())
                    .or_insert(email);
            }
            "founder" => {
                founder_emails.insert(email.clone());
                let role_title = metadata
                    .get("role_title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Founder");
                founder_roles
                    .entry(email)
                    .or_insert_with(|| role_title.to_string());
            }
            "admin" => {
                let can_sudo = metadata
                    .get("allow_sudo_actions")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                admin_policies.entry(email).or_insert(can_sudo);
            }
            _ => {}
        }
    }

    if !founder_emails.is_empty() {
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            founder_emails.into_iter().collect::<Vec<_>>().join(","),
        );
    }
    if !founder_roles.is_empty() {
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ROLES".to_string(),
            founder_roles
                .into_iter()
                .map(|(email, role)| format!("{email}={role}"))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if !admin_policies.is_empty() {
        settings.insert(
            "CTOX_EMAIL_ADMIN_POLICIES".to_string(),
            admin_policies
                .into_iter()
                .map(|(email, can_sudo)| {
                    if can_sudo {
                        format!("{email}=sudo")
                    } else {
                        email
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    Ok(())
}

fn runtime_settings_with_owner_profiles(
    root: &Path,
    kind: communication_gateway::CommunicationAdapterKind,
) -> BTreeMap<String, String> {
    let mut settings = communication_gateway::runtime_settings_from_root(root, kind);
    let _ = merge_owner_profile_settings(root, &mut settings);
    settings
}

pub fn ensure_store(root: &Path) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let _conn = open_channel_db(&db_path)?;
    Ok(())
}

pub fn load_prompt_identity(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<OwnerPromptContext> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let owner_name = load_owner_name(&conn)?
        .or_else(|| {
            settings
                .get("CTOX_OWNER_NAME")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "the owner".to_string());

    let mut channels = BTreeSet::new();
    channels.insert("- tui: direct local CTOX session".to_string());

    let mut stmt = conn.prepare(
        r#"
        SELECT channel, address, provider, profile_json
        FROM communication_accounts
        ORDER BY channel ASC, account_key ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (channel, address, provider, profile_json) = row?;
        match channel.as_str() {
            "email" => {
                if !address.trim().is_empty() {
                    channels.insert(format!(
                        "- email: {} (provider: {})",
                        address.trim(),
                        provider.trim()
                    ));
                }
            }
            "jami" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let profile_name = parsed
                    .get("profileName")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !profile_name.is_empty() {
                    channels.insert(format!("- jami: {}", profile_name));
                }
            }
            "teams" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let bot_id = parsed
                    .get("botId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !bot_id.is_empty() {
                    channels.insert(format!("- teams: {}", bot_id));
                }
            }
            "whatsapp" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let jid = parsed
                    .get("jid")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !jid.is_empty() {
                    channels.insert(format!("- whatsapp: {}", jid));
                }
            }
            "cron" | "plan" | "queue" => {}
            other => {
                if !address.trim().is_empty() {
                    channels.insert(format!("- {}: {}", other, address.trim()));
                } else {
                    channels.insert(format!("- {}", other));
                }
            }
        }
    }

    Ok(OwnerPromptContext {
        owner_name,
        owner_email_address: settings
            .get("CTOX_OWNER_EMAIL_ADDRESS")
            .map(|value| normalize_email_address(value))
            .filter(|value| !value.is_empty()),
        founder_email_addresses: parse_founder_email_addresses(settings),
        founder_email_roles: founder_email_role_summaries(settings),
        allowed_email_domain: normalized_allowed_email_domain(settings),
        admin_email_policies: admin_email_policy_summaries(settings),
        channels: channels.into_iter().collect(),
        preferred_channel: settings
            .get("CTOX_OWNER_PREFERRED_CHANNEL")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    })
}

/// Whether the inbound message metadata carries the structured
/// "auto-submitted" marker we extract from RFC 3834 / Outlook headers
/// at IMAP/Graph ingestion time.
///
/// We deliberately do NOT inspect subject lines or body text here:
/// language- and template-specific scraping belongs in skills, not in
/// the core. This check looks only at JSON fields written by the
/// inbound parser.
pub fn metadata_marks_auto_submitted(metadata: &Value) -> bool {
    let direct = metadata
        .get("autoSubmitted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if direct {
        return true;
    }
    let suppress = metadata
        .get("autoResponseSuppress")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if suppress {
        return true;
    }
    // Defense-in-depth: when the inbound parser captured the raw
    // header value but failed to populate the boolean (older row
    // shape), still honour an `auto-replied`/`auto-generated`/
    // `auto-notified` token. We compare structured tokens, not
    // free-form strings.
    if let Some(value) = metadata.get("autoSubmittedValue").and_then(Value::as_str) {
        let token = value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !token.is_empty() && token != "no" {
            return true;
        }
    }
    false
}

pub fn reclassify_historical_auto_submitted_inbounds(root: &Path) -> Result<usize> {
    #[derive(Debug)]
    struct Candidate {
        message_key: String,
        subject: String,
        sender_address: String,
        body_text: String,
        metadata: Value,
    }

    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.subject,
            m.sender_address,
            m.body_text,
            m.metadata_json
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.status = 'received'
          AND m.channel = 'email'
          AND COALESCE(r.route_status, 'pending') IN ('pending','leased','failed','review_rework','handled')
          AND NOT EXISTS (
              SELECT 1
              FROM communication_founder_reply_reviews review
              WHERE review.inbound_message_key = m.message_key
                AND review.terminal_no_send = 1
          )
        "#,
    )?;
    let candidates = stmt
        .query_map([], |row| {
            let metadata_raw: String = row.get(4)?;
            Ok(Candidate {
                message_key: row.get(0)?,
                subject: row.get(1)?,
                sender_address: row.get(2)?,
                body_text: row.get(3)?,
                metadata: serde_json::from_str(&metadata_raw).unwrap_or_else(|_| json!({})),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut reclassified = 0usize;
    for mut candidate in candidates {
        if metadata_marks_auto_submitted(&candidate.metadata) {
            continue;
        }
        let Some(reason) = historical_auto_submitted_reason(
            &candidate.subject,
            &candidate.sender_address,
            &candidate.body_text,
        ) else {
            continue;
        };
        let now = now_iso_string();
        if let Some(object) = candidate.metadata.as_object_mut() {
            object.insert("autoSubmitted".to_string(), Value::Bool(true));
            object.insert(
                "autoSubmittedValue".to_string(),
                Value::String("historical-reclassifier".to_string()),
            );
            object.insert("terminalNoSend".to_string(), Value::Bool(true));
            object.insert(
                "terminalNoSendReason".to_string(),
                Value::String(reason.clone()),
            );
            object.insert("reclassifiedAt".to_string(), Value::String(now.clone()));
        }
        conn.execute(
            r#"
            UPDATE communication_messages
            SET metadata_json = ?2
            WHERE message_key = ?1
            "#,
            params![
                candidate.message_key,
                serde_json::to_string(&candidate.metadata)?
            ],
        )?;
        record_terminal_no_send_verdict(
            root,
            &candidate.message_key,
            "boot-reclassifier",
            &reason,
        )?;
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            VALUES (?1, 'handled', NULL, NULL, ?2, NULL, ?2)
            ON CONFLICT(message_key) DO UPDATE SET
                route_status='handled',
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=?2,
                last_error=NULL,
                updated_at=?2
            "#,
            params![candidate.message_key, now],
        )?;
        reclassified += 1;
    }
    Ok(reclassified)
}

fn historical_auto_submitted_reason(
    subject: &str,
    sender_address: &str,
    body_text: &str,
) -> Option<String> {
    let subject = subject.trim().to_ascii_lowercase();
    if [
        "automatische antwort:",
        "auto-reply:",
        "out of office:",
        "automatic reply:",
    ]
    .iter()
    .any(|prefix| subject.starts_with(prefix))
    {
        return Some("historical auto-reply subject: terminal NO-SEND".to_string());
    }

    let sender = normalize_email_address(sender_address);
    let local_part = sender.split('@').next().unwrap_or("");
    if matches!(
        local_part,
        "noreply" | "no-reply" | "donotreply" | "do-not-reply" | "notification" | "notifications"
    ) {
        return Some("historical notification sender: terminal NO-SEND".to_string());
    }

    if body_is_only_teams_meeting_link(body_text) {
        return Some(
            "historical Teams meeting-link notification without human content: terminal NO-SEND"
                .to_string(),
        );
    }
    None
}

fn body_is_only_teams_meeting_link(body_text: &str) -> bool {
    let lowered = body_text.to_ascii_lowercase();
    if !(lowered.contains("teams.microsoft.com/l/meetup-join")
        || lowered.contains("teams.live.com/meet")
        || lowered.contains("join.microsoft.com/meet"))
    {
        return false;
    }
    let mut remainder = lowered.as_str();
    let mut cleaned = String::new();
    while let Some(start) = remainder.find("http") {
        cleaned.push_str(&remainder[..start]);
        let after_start = &remainder[start..];
        let end = after_start
            .find(char::is_whitespace)
            .unwrap_or(after_start.len());
        remainder = &after_start[end..];
    }
    cleaned.push_str(remainder);
    for phrase in [
        "microsoft teams",
        "join the meeting",
        "meeting id",
        "passcode",
        "dial in",
        "privacy and security",
        "learn more",
        "need help",
        "besprechungs-id",
        "kenncode",
        "an besprechung teilnehmen",
        "teilnehmen",
    ] {
        cleaned = cleaned.replace(phrase, " ");
    }
    let meaningful = cleaned
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect::<String>();
    meaningful.len() <= 40
}

/// Terminal route states that are sticky against further re-routing.
/// Once an inbound message is acked into one of these, the service
/// loop must NOT pull it back into `review_rework` or any other
/// non-terminal state. New work for the same thread must arrive via a
/// fresh inbound message (with its own message_key).
pub fn route_status_is_terminal(route_status: &str) -> bool {
    matches!(
        route_status,
        "handled" | "cancelled" | "failed" | "completed"
    )
}

pub fn classify_email_sender(
    settings: &BTreeMap<String, String>,
    sender_address: &str,
) -> EmailSenderPolicy {
    let normalized_email = normalize_email_address(sender_address);
    let owner_email = settings
        .get("CTOX_OWNER_EMAIL_ADDRESS")
        .map(|value| normalize_email_address(value))
        .filter(|value| !value.is_empty());
    let founder_emails = parse_founder_email_addresses(settings);
    let allowed_email_domain = normalized_allowed_email_domain(settings);
    let admin_policies = parse_admin_email_policies(settings);

    if normalized_email.is_empty() {
        return EmailSenderPolicy {
            normalized_email,
            role: "external".to_string(),
            allowed: false,
            allow_admin_actions: false,
            allow_sudo_actions: false,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: Some("sender email address is empty".to_string()),
        };
    }

    if owner_email.as_deref() == Some(normalized_email.as_str()) {
        return EmailSenderPolicy {
            normalized_email,
            role: "owner".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: true,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if founder_emails
        .iter()
        .any(|email| email == &normalized_email)
    {
        return EmailSenderPolicy {
            normalized_email,
            role: "founder".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: false,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if let Some(admin) = admin_policies
        .iter()
        .find(|entry| entry.email == normalized_email)
    {
        return EmailSenderPolicy {
            normalized_email,
            role: "admin".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: admin.can_sudo,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if let Some(domain) = allowed_email_domain.clone() {
        if email_matches_domain(&normalized_email, &domain) {
            return EmailSenderPolicy {
                normalized_email,
                role: "domain_user".to_string(),
                allowed: true,
                allow_admin_actions: false,
                allow_sudo_actions: false,
                secrets_via_email_allowed: false,
                allowed_email_domain: Some(domain),
                block_reason: None,
            };
        }
    }

    EmailSenderPolicy {
        normalized_email,
        role: "external".to_string(),
        allowed: false,
        allow_admin_actions: false,
        allow_sudo_actions: false,
        secrets_via_email_allowed: false,
        allowed_email_domain,
        block_reason: Some(
            "sender is outside the configured founder/owner/admin list and allowed employee email domain"
                .to_string(),
        ),
    }
}

/// F4: snapshot of the founder/owner outbound pipeline for a single thread.
/// Joins:
/// - `mission_states`        — current mission status, agent_failure_count
/// - `messages`              — agent attempts and their structured outcomes
/// - `communication_founder_reply_reviews` — review and approval records
/// - `communication_messages` (outbound rows, plus their routing state) —
///   actual send attempts and their delivery state
///
/// Output is flat JSON shaped for operator consumption, intentionally
/// avoiding internal-only field names where they would leak past CTOX.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineStatusReport {
    pub thread_key: Option<String>,
    pub founder_outbound_intent: bool,
    pub agent_attempts: Vec<PipelineAgentAttempt>,
    pub review_runs: Vec<PipelineReviewRun>,
    pub approval_records: Vec<PipelineApprovalRecord>,
    pub send_attempts: Vec<PipelineSendAttempt>,
    pub current_mission_status: String,
    pub agent_failure_count: i64,
    /// Iteration counter for the lightweight rewrite-only review path
    /// (per-mission, reset on approval). Surfaced so operators can see
    /// when a thread is bouncing in the body-fix loop versus the heavy
    /// rework path.
    pub rewrite_iteration_count: i64,
    /// Iteration counter for the heavy rework path. Derived from the
    /// stored `agent_failure_count` because rework continuations inherit
    /// the agent-failure backoff machinery; this duplication keeps the
    /// pipeline-status surface self-describing without changing the
    /// underlying schema.
    pub rework_iteration_count: i64,
    /// Most recent disposition the dispatcher chose. One of `Approved`,
    /// `RewriteOnly`, `RequeueSelfWork`, `None`. Computed from the latest
    /// review run / mission status, so it stays accurate without an
    /// extra column.
    pub current_disposition: String,
    pub last_error: Option<String>,
    /// Recent governance events from the strategic-directive owner-authority
    /// gate that touched this thread. Surfaces both permitted and blocked
    /// inbound-mail-driven mutations so operators can see whether the
    /// authority gate fired (and how) for the conversation. Filtered by
    /// `details.thread_key` or `details.conversation_id` so unrelated
    /// global authority events do not leak into the per-thread surface.
    pub strategic_directive_authority_events: Vec<StrategicDirectiveAuthorityEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategicDirectiveAuthorityEvent {
    pub event_id: String,
    pub mechanism_id: String,
    pub severity: String,
    pub created_at: String,
    pub sender_role: Option<String>,
    pub sender_address: Option<String>,
    pub directive_kind: Option<String>,
    pub attempted_status: Option<String>,
    pub action: Option<String>,
    pub triggered_by_message_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineAgentAttempt {
    pub turn_id: String,
    pub outcome: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineReviewRun {
    pub approval_key: String,
    pub inbound_message_key: String,
    pub reviewer: String,
    pub review_summary: String,
    pub approved_at: String,
    pub sent_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineApprovalRecord {
    pub approval_key: String,
    pub action_digest: String,
    pub body_sha256: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineSendAttempt {
    pub message_key: String,
    pub direction: String,
    pub subject: String,
    pub external_created_at: String,
    pub route_status: Option<String>,
    pub last_error: Option<String>,
}

pub(crate) fn pipeline_status(
    root: &Path,
    thread_key: Option<&str>,
    limit: usize,
) -> Result<PipelineStatusReport> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;

    // Agent attempts and reviews are scoped per-conversation_id derived
    // from the thread_key. If the operator did not supply one, we report
    // global state without per-thread review/send rows.
    let conversation_id = thread_key
        .map(|key| crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(key)));

    // Mission state for the conversation that owns this thread.
    let mission_state = if let Some(conv_id) = conversation_id {
        crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())
            .ok()
            .and_then(|engine| engine.stored_mission_state(conv_id).ok().flatten())
    } else {
        None
    };
    let (
        current_mission_status,
        agent_failure_count,
        rewrite_iteration_count,
        rework_iteration_count,
        last_error,
    ) = match &mission_state {
        Some(record) => (
            record.mission_status.clone(),
            record.agent_failure_count,
            record.rewrite_failure_count,
            record.agent_failure_count,
            record.deferred_reason.clone(),
        ),
        None => ("unknown".to_string(), 0, 0, 0, None),
    };

    // Agent attempts: most recent assistant rows for the conversation, in
    // reverse-chronological order, along with their structured outcome.
    let agent_attempts = if let Some(conv_id) = conversation_id {
        let mut stmt = conn.prepare(
            "SELECT message_id, agent_outcome, created_at
             FROM messages
             WHERE conversation_id = ?1 AND role = 'assistant'
             ORDER BY seq DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![conv_id, limit as i64], |row| {
            let id: i64 = row.get(0)?;
            let outcome: Option<String> = row.get(1)?;
            let ended: Option<String> = row.get(2)?;
            Ok(PipelineAgentAttempt {
                turn_id: format!("msg:{id}"),
                outcome,
                started_at: None,
                ended_at: ended,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    // Review and approval records keyed on the inbound message belonging
    // to this thread. If thread_key is None, return all recent reviews.
    let mut review_runs = Vec::new();
    let mut approval_records = Vec::new();
    if let Some(thread) = thread_key {
        let mut stmt = conn.prepare(
            "SELECT r.approval_key, r.inbound_message_key, r.action_digest, r.body_sha256,
                    r.reviewer, r.review_summary, r.approved_at, r.sent_at
             FROM communication_founder_reply_reviews r
             JOIN communication_messages m ON m.message_key = r.inbound_message_key
             WHERE m.thread_key = ?1
             ORDER BY r.approved_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread, limit as i64], |row| {
            let approval_key: String = row.get(0)?;
            let inbound_message_key: String = row.get(1)?;
            let action_digest: String = row.get(2)?;
            let body_sha256: String = row.get(3)?;
            let reviewer: String = row.get(4)?;
            let review_summary: String = row.get(5)?;
            let approved_at: String = row.get(6)?;
            let sent_at: Option<String> = row.get(7)?;
            Ok((
                PipelineReviewRun {
                    approval_key: approval_key.clone(),
                    inbound_message_key,
                    reviewer,
                    review_summary,
                    approved_at: approved_at.clone(),
                    sent_at,
                },
                PipelineApprovalRecord {
                    approval_key,
                    action_digest,
                    body_sha256,
                    approved_at,
                },
            ))
        })?;
        for row in rows {
            let (review, approval) = row?;
            review_runs.push(review);
            approval_records.push(approval);
        }
    }

    // Send attempts: outbound communication_messages rows for this thread.
    let send_attempts = if let Some(thread) = thread_key {
        let mut stmt = conn.prepare(
            "SELECT m.message_key, m.direction, m.subject, m.external_created_at,
                    r.route_status, r.last_error
             FROM communication_messages m
             LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
             WHERE m.thread_key = ?1 AND m.direction = 'outbound'
             ORDER BY m.external_created_at DESC, m.observed_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread, limit as i64], |row| {
            Ok(PipelineSendAttempt {
                message_key: row.get(0)?,
                direction: row.get(1)?,
                subject: row.get(2)?,
                external_created_at: row.get(3)?,
                route_status: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    // founder_outbound_intent is true if there's at least one approval
    // record for this thread (a reviewed founder send was prepared).
    let founder_outbound_intent = !approval_records.is_empty();

    // The dispatcher disposition is structural (no string scraping). We
    // derive it from the persisted state: an approval row implies the most
    // recent disposition was `Approved`; a non-zero rewrite_failure_count
    // implies the loop is in the lightweight rewrite path; a non-zero
    // agent_failure_count implies the heavy rework path. Otherwise the
    // pipeline has never produced a reviewed slice — `None`.
    let current_disposition = if !approval_records.is_empty() {
        "Approved".to_string()
    } else if rewrite_iteration_count > 0 {
        "RewriteOnly".to_string()
    } else if rework_iteration_count > 0 {
        "RequeueSelfWork".to_string()
    } else {
        "None".to_string()
    };

    // E (PR): per-thread strategic-directive authority audit trail. We
    // pull both the `_owner_authorised` and `_blocked_non_owner_sender`
    // events the strategy-mutation gate emits, and filter to those whose
    // structured details reference this thread (`thread_key` or
    // `conversation_id`). The default surface is the last `limit` such
    // events; if no thread was supplied we leave the list empty rather
    // than reporting global state, which matches how the surrounding
    // pipeline fields treat an absent thread_key.
    let strategic_directive_authority_events =
        load_strategic_directive_authority_events(&db_path, thread_key, conversation_id, limit)?;

    Ok(PipelineStatusReport {
        thread_key: thread_key.map(ToOwned::to_owned),
        founder_outbound_intent,
        agent_attempts,
        review_runs,
        approval_records,
        send_attempts,
        current_mission_status,
        agent_failure_count,
        rewrite_iteration_count,
        rework_iteration_count,
        current_disposition,
        last_error,
        strategic_directive_authority_events,
    })
}

fn load_strategic_directive_authority_events(
    db_path: &Path,
    thread_key: Option<&str>,
    conversation_id: Option<i64>,
    limit: usize,
) -> Result<Vec<StrategicDirectiveAuthorityEvent>> {
    if thread_key.is_none() && conversation_id.is_none() {
        return Ok(Vec::new());
    }
    // The governance schema is created lazily by the governance module; if
    // it does not exist yet, return an empty vec rather than erroring.
    let conn = Connection::open(db_path).with_context(|| {
        format!(
            "failed to open db {} for strategic-directive authority events",
            db_path.display()
        )
    })?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='governance_events'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT event_id, mechanism_id, severity, details_json, created_at
         FROM governance_events
         WHERE mechanism_id IN (
             'strategic_directive_mutation_owner_authorised',
             'strategic_directive_mutation_blocked_non_owner_sender'
         )
         ORDER BY CAST(created_at AS INTEGER) DESC
         LIMIT ?1",
    )?;
    // We pull a generous slice and filter in Rust because the structured
    // thread/conversation match lives inside `details_json`. Clamp to a
    // sane upper bound so this stays cheap even if the audit trail is busy.
    let scan_limit = (limit.max(1) * 8).min(512) as i64;
    let rows = stmt.query_map(params![scan_limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out: Vec<StrategicDirectiveAuthorityEvent> = Vec::new();
    for row in rows {
        let (event_id, mechanism_id, severity, details_json, created_at) = row?;
        let details: serde_json::Value =
            serde_json::from_str(&details_json).unwrap_or(serde_json::Value::Null);
        let detail_thread = details
            .get("thread_key")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let detail_conversation = details
            .get("conversation_id")
            .and_then(|value| value.as_i64());
        let matches_thread = match thread_key {
            Some(key) => detail_thread.as_deref() == Some(key),
            None => false,
        };
        let matches_conversation = match (conversation_id, detail_conversation) {
            (Some(want), Some(got)) => want == got,
            _ => false,
        };
        if !matches_thread && !matches_conversation {
            continue;
        }
        out.push(StrategicDirectiveAuthorityEvent {
            event_id,
            mechanism_id,
            severity,
            created_at,
            sender_role: details
                .get("sender_role")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            sender_address: details
                .get("sender_address")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            directive_kind: details
                .get("directive_kind")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            attempted_status: details
                .get("attempted_status")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            action: details
                .get("action")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            triggered_by_message_key: details
                .get("triggered_by_message_key")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

pub fn handle_channel_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let conn = open_channel_db(&db_path)?;
            let result = json!({
                "ok": true,
                "db_path": db_path,
                "initialized": schema_state(&conn)?,
            });
            print_json(&result)
        }
        "sync" => {
            let channel = required_flag_value(args, "--channel")?;
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let result = sync_channel(root, &db_path, channel, args)?;
            print_json(&result)
        }
        "take" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let lease_owner = find_flag_value(args, "--lease-owner")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "codex".to_string());
            let channel = find_flag_value(args, "--channel").map(ToOwned::to_owned);
            let mut conn = open_channel_db(&db_path)?;
            let taken = take_messages(&mut conn, channel.as_deref(), limit, &lease_owner)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "lease_owner": lease_owner,
                "count": taken.len(),
                "messages": taken,
            }))
        }
        "ack" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let status = find_flag_value(args, "--status").unwrap_or("handled");
            let message_keys = positional_after_flags(&args[1..]);
            if message_keys.is_empty() {
                anyhow::bail!(
                    "usage: ctox channel ack [--db <path>] [--status <status>] <message-key>..."
                );
            }
            let mut conn = open_channel_db(&db_path)?;
            let updated = ack_messages(&mut conn, &message_keys, status)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "updated": updated,
                "status": status,
                "message_keys": message_keys,
            }))
        }
        "send" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let request = parse_send_request(args)?;
            let result = send_message(root, &db_path, request)?;
            print_json(&result)
        }
        "founder-reply" => {
            anyhow::bail!(
                "direct founder-reply is disabled; founder/owner outbound email must be sent only through the reviewed service path"
            )
        }
        "test" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let channel = required_flag_value(args, "--channel")?;
            let account_key = find_flag_value(args, "--account-key").map(ToOwned::to_owned);
            let result = test_channel(root, &db_path, channel, account_key.as_deref())?;
            print_json(&result)
        }
        "ingest-tui" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let request = parse_tui_ingest_request(args)?;
            let mut conn = open_channel_db(&db_path)?;
            let stored = ingest_tui_message(root, &mut conn, request)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "stored": stored,
            }))
        }
        "list" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let channel = find_flag_value(args, "--channel");
            let conn = open_channel_db(&db_path)?;
            let messages = list_messages(&conn, channel, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "history" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = required_flag_value(args, "--thread-key")?;
            let conn = open_channel_db(&db_path)?;
            let messages = list_thread_messages(&conn, thread_key, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "thread_key": thread_key,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "search" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let query = required_flag_value(args, "--query")?;
            let channel = find_flag_value(args, "--channel");
            let sender = find_flag_value(args, "--sender");
            let conn = open_channel_db(&db_path)?;
            let messages = search_messages(&conn, query, channel, sender, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "query": query,
                "channel": channel,
                "sender": sender,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "context" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = required_flag_value(args, "--thread-key")?;
            let query = find_flag_value(args, "--query");
            let sender = find_flag_value(args, "--sender");
            let conn = open_channel_db(&db_path)?;
            let context = build_communication_context(&conn, thread_key, query, sender, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "context": context,
            }))
        }
        "pipeline-status" => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = find_flag_value(args, "--thread-key");
            let report = pipeline_status(root, thread_key, limit)?;
            print_json(&json!({
                "ok": true,
                "report": report,
            }))
        }
        _ => {
            anyhow::bail!(
                "usage:\n  ctox channel init [--db <path>]\n  ctox channel sync --channel <email|jami|teams|meeting|whatsapp> [--db <path>] [adapter flags]\n  ctox channel take [--db <path>] [--channel <name>] [--limit <n>] [--lease-owner <owner>]\n  ctox channel ack [--db <path>] [--status <status>] <message-key>...\n  ctox channel send --channel <tui|email|jami|teams|meeting|whatsapp> --account-key <key> --thread-key <key> --body <text> [--subject <text>] [--to <addr>]... [--cc <addr>]... [--attach-file <path>]... [--send-voice] [--reviewed-founder-send]\n  ctox channel founder-reply --message-key <inbound-email-key> --body <text>\n  ctox channel test --channel <tui|email|jami|teams|whatsapp> [--db <path>] [--account-key <key>]\n  ctox channel ingest-tui --account-key <key> --thread-key <key> --body <text> [--sender-display <name>] [--sender-address <addr>] [--subject <text>]\n  ctox channel list [--db <path>] [--channel <name>] [--limit <n>]\n  ctox channel history --thread-key <key> [--db <path>] [--limit <n>]\n  ctox channel search --query <text> [--db <path>] [--channel <name>] [--sender <addr>] [--limit <n>]\n  ctox channel context --thread-key <key> [--db <path>] [--query <text>] [--sender <addr>] [--limit <n>]\n  ctox channel pipeline-status [--thread-key <key>] [--limit <n>]"
            )
        }
    }
}

pub fn load_recent_communication_feed(
    root: &Path,
    limit: usize,
) -> Result<Vec<CommunicationFeedItem>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.channel,
            m.direction,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.thread_key,
            COALESCE(r.route_status, 'pending'),
            m.external_created_at
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(CommunicationFeedItem {
            message_key: String::new(),
            channel: row.get(0)?,
            direction: row.get(1)?,
            sender_display: row.get(2)?,
            sender_address: row.get(3)?,
            subject: row.get(4)?,
            preview: row.get(5)?,
            thread_key: row.get(6)?,
            route_status: row.get(7)?,
            external_created_at: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub fn load_thread_communication_feed(
    root: &Path,
    thread_key: &str,
    limit: usize,
) -> Result<Vec<CommunicationFeedItem>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.direction,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.thread_key,
            COALESCE(r.route_status, 'pending'),
            m.external_created_at
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.thread_key = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![thread_key, limit as i64], |row| {
        Ok(CommunicationFeedItem {
            message_key: row.get(0)?,
            channel: row.get(1)?,
            direction: row.get(2)?,
            sender_display: row.get(3)?,
            sender_address: row.get(4)?,
            subject: row.get(5)?,
            preview: row.get(6)?,
            thread_key: row.get(7)?,
            route_status: row.get(8)?,
            external_created_at: row.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub fn lease_pending_inbound_messages(
    root: &Path,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<RoutedInboundMessage>> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let leased = take_messages(&mut conn, None, limit, lease_owner)?;
    Ok(leased
        .into_iter()
        .map(|item| {
            let preferred_reply_modality = item
                .metadata
                .get("preferredReplyModality")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let workspace_root =
                workspace_root_from_queue_metadata_or_prompt(&item.metadata, &item.body_text);
            RoutedInboundMessage {
                message_key: item.message_key,
                channel: item.channel,
                account_key: item.account_key,
                thread_key: item.thread_key,
                sender_display: item.sender_display,
                sender_address: item.sender_address,
                subject: item.subject,
                preview: item.preview,
                body_text: item.body_text,
                external_created_at: item.external_created_at,
                workspace_root,
                metadata: item.metadata,
                preferred_reply_modality,
            }
        })
        .collect())
}

pub fn list_stalled_inbound_messages(
    root: &Path,
    limit: usize,
) -> Result<Vec<RoutedInboundMessage>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.channel IN ('email', 'jami')
          AND r.route_status IN ('failed', 'review_rework')
          AND (
                r.acked_at IS NULL
             OR r.route_status IN ('failed', 'review_rework')
          )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .map(|items| {
            items
                .into_iter()
                .map(routed_inbound_message_from_view)
                .collect()
        })
}

pub fn list_unreviewed_handled_inbound_messages(
    root: &Path,
    limit: usize,
) -> Result<Vec<RoutedInboundMessage>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.channel IN ('email', 'jami')
          AND r.route_status = 'handled'
          AND NOT EXISTS (
              SELECT 1
              FROM communication_founder_reply_reviews review
              WHERE review.inbound_message_key = m.message_key
                AND review.sent_at IS NOT NULL
          )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .map(|items| {
            items
                .into_iter()
                .map(routed_inbound_message_from_view)
                .collect()
        })
}

pub fn founder_reply_sent_after_review_for_message(
    root: &Path,
    inbound_message_key: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    founder_reply_sent_after_review(&conn, inbound_message_key)
}

/// Whether any inbound communication message is still pending or leased
/// (i.e. not acked as handled/blocked). Used by the mission watchdog to
/// avoid queuing redundant continuation tasks when real work is already
/// waiting in the channel queue.
pub fn has_runnable_inbound_message(root: &Path) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_routing_state
        WHERE route_status IN ('pending', 'leased')
        "#,
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn ack_leased_messages(root: &Path, message_keys: &[String], status: &str) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status)?;
    ack_messages(&mut conn, message_keys, status)
}

pub fn set_queue_task_route_status(
    root: &Path,
    message_key: &str,
    route_status: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    if load_queue_message_from_conn(&conn, message_key)?.is_none() {
        return Ok(false);
    }
    update_queue_task(
        root,
        QueueTaskUpdateRequest {
            message_key: message_key.to_string(),
            route_status: Some(route_status.to_string()),
            ..Default::default()
        },
    )?;
    Ok(true)
}

pub fn create_queue_task(root: &Path, request: QueueTaskCreateRequest) -> Result<QueueTaskView> {
    create_queue_task_with_metadata(root, request)
}

pub fn create_queue_task_with_metadata(
    root: &Path,
    request: QueueTaskCreateRequest,
) -> Result<QueueTaskView> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let title = request.title.trim();
    let prompt = request.prompt.trim();
    if title.is_empty() {
        anyhow::bail!("queue task title must not be empty");
    }
    if prompt.is_empty() {
        anyhow::bail!("queue task prompt must not be empty");
    }
    let priority = canonical_queue_priority(&request.priority)?;
    let now = now_iso_string();
    let sort_at = queue_sort_at(&priority, &now)?;
    let digest = stable_digest(&format!(
        "{}:{}:{}:{}",
        title,
        prompt,
        request.thread_key.trim(),
        now
    ));
    let message_key = format!("{QUEUE_ACCOUNT_KEY}::{digest}");
    let remote_id = format!("queue-{digest}");
    let mut metadata = json!({
        "source": "ctox-queue",
        "priority": priority,
        "skill": request.suggested_skill.as_deref(),
        "parent_message_key": request.parent_message_key.as_deref(),
        "workspace_root": normalize_workspace_root(request.workspace_root.as_deref())
            .or_else(|| legacy_workspace_root_from_prompt(prompt)),
        "created_at": now,
        "sort_at": sort_at,
    });
    if let Some(extra) = request.extra_metadata {
        merge_object_metadata(&mut metadata, extra);
    }
    enforce_queue_task_spawn(
        &conn,
        &metadata,
        request.parent_message_key.as_deref(),
        request.thread_key.trim(),
        &message_key,
        title,
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: QUEUE_CHANNEL_NAME,
            account_key: QUEUE_ACCOUNT_KEY,
            thread_key: request.thread_key.trim(),
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "queue",
            sender_display: QUEUE_SENDER_DISPLAY,
            sender_address: QUEUE_SENDER_ADDRESS,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: title,
            preview: &preview_text(prompt, title),
            body_text: prompt,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &sort_at,
            observed_at: &now,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, request.thread_key.trim())?;
    ensure_routing_rows_for_inbound(&conn)?;
    load_queue_task_from_conn(&conn, &message_key)?.context("failed to load created queue task")
}

fn enforce_queue_task_spawn(
    conn: &Connection,
    metadata: &Value,
    parent_message_key: Option<&str>,
    thread_key: &str,
    message_key: &str,
    title: &str,
) -> Result<()> {
    let ticket_self_work_id = metadata_string_value(metadata, "ticket_self_work_id");
    let ticket_self_work_kind = metadata_string_value(metadata, "ticket_self_work_kind");
    let (parent_entity_type, parent_entity_id, spawn_kind, spawn_reason, budget_key, max_attempts) =
        if let Some(work_id) = ticket_self_work_id.clone() {
            (
                "WorkItem".to_string(),
                work_id.clone(),
                "self-work-queue-task".to_string(),
                "publish_self_work_for_execution".to_string(),
                format!("self-work-queue:{work_id}"),
                64,
            )
        } else if let Some(parent_message_key) = parent_message_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            (
                "Message".to_string(),
                parent_message_key.to_string(),
                "queue-task".to_string(),
                "create_queue_task".to_string(),
                format!("queue-task:message:{parent_message_key}"),
                64,
            )
        } else {
            (
                "Thread".to_string(),
                thread_key.to_string(),
                "queue-task".to_string(),
                "create_queue_task".to_string(),
                format!("queue-task:thread:{thread_key}"),
                64,
            )
        };
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("queue_title".to_string(), title.to_string());
    if let Some(kind) = ticket_self_work_kind {
        edge_metadata.insert("self_work_kind".to_string(), kind);
    }

    enforce_core_spawn(
        conn,
        &CoreSpawnRequest {
            parent_entity_type,
            parent_entity_id,
            child_entity_type: "QueueTask".to_string(),
            child_entity_id: message_key.to_string(),
            spawn_kind,
            spawn_reason,
            actor: "ctox-queue".to_string(),
            checkpoint_key: Some(message_key.to_string()),
            budget_key: Some(budget_key),
            max_attempts: Some(max_attempts),
            metadata: edge_metadata,
        },
    )?;
    Ok(())
}

fn metadata_string_value(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn merge_object_metadata(target: &mut Value, extra: Value) {
    let Some(target_map) = target.as_object_mut() else {
        return;
    };
    let Some(extra_map) = extra.as_object() else {
        return;
    };
    for (key, value) in extra_map {
        target_map.insert(key.clone(), value.clone());
    }
}

pub fn list_queue_tasks(
    root: &Path,
    statuses: &[String],
    limit: usize,
) -> Result<Vec<QueueTaskView>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    if statuses.is_empty() {
        return list_queue_tasks_from_conn(&conn, limit);
    }
    let allowed = statuses
        .iter()
        .map(|status| status.trim().to_lowercase())
        .filter(|status| !status.is_empty())
        .collect::<Vec<_>>();
    if allowed.is_empty() {
        return Ok(Vec::new());
    }
    list_queue_tasks_from_conn_with_statuses(&conn, &allowed, limit)
}

pub fn load_queue_task(root: &Path, message_key: &str) -> Result<Option<QueueTaskView>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    load_queue_task_from_conn(&conn, message_key)
}

pub fn update_queue_task(root: &Path, request: QueueTaskUpdateRequest) -> Result<QueueTaskView> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let current = load_queue_message_from_conn(&conn, &request.message_key)?
        .context("queue task not found")?;
    let current_metadata = queue_metadata_object(&current.metadata);
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.subject.trim())
        .to_string();
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.body_text.trim())
        .to_string();
    let thread_key = request
        .thread_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.thread_key.trim())
        .to_string();
    let priority = if let Some(priority) = request.priority.as_deref() {
        canonical_queue_priority(priority)?
    } else {
        current_queue_priority(&current)
    };
    let now = now_iso_string();
    let sort_at = queue_sort_at(&priority, &now)?;
    let mut metadata = current_metadata;
    metadata.insert(
        "source".to_string(),
        Value::String("ctox-queue".to_string()),
    );
    metadata.insert("priority".to_string(), Value::String(priority.clone()));
    metadata.insert("sort_at".to_string(), Value::String(sort_at.clone()));
    if metadata.get("created_at").is_none() {
        metadata.insert(
            "created_at".to_string(),
            Value::String(current.observed_at.clone()),
        );
    }
    if let Some(skill) = request
        .suggested_skill
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("skill".to_string(), Value::String(skill.to_string()));
    } else if request.clear_skill {
        metadata.remove("skill");
    }
    if let Some(workspace_root) = normalize_workspace_root(request.workspace_root.as_deref()) {
        metadata.insert("workspace_root".to_string(), Value::String(workspace_root));
    } else if request.clear_workspace_root {
        metadata.remove("workspace_root");
    } else if metadata
        .get("workspace_root")
        .and_then(Value::as_str)
        .and_then(|value| normalize_workspace_root(Some(value)))
        .is_none()
    {
        if let Some(workspace_root) = legacy_workspace_root_from_prompt(&prompt) {
            metadata.insert("workspace_root".to_string(), Value::String(workspace_root));
        }
    }
    if let Some(note) = request
        .status_note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("status_note".to_string(), Value::String(note.to_string()));
    } else if request.clear_note {
        metadata.remove("status_note");
    }
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &current.message_key,
            channel: QUEUE_CHANNEL_NAME,
            account_key: QUEUE_ACCOUNT_KEY,
            thread_key: &thread_key,
            remote_id: &current.remote_id,
            direction: "inbound",
            folder_hint: "queue",
            sender_display: QUEUE_SENDER_DISPLAY,
            sender_address: QUEUE_SENDER_ADDRESS,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &title,
            preview: &preview_text(&prompt, &title),
            body_text: &prompt,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: current.seen,
            has_attachments: false,
            external_created_at: &sort_at,
            observed_at: &now,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    if let Some(route_status) = request.route_status.as_deref() {
        set_routing_status(&mut conn, &current.message_key, route_status, &now)?;
    }
    refresh_thread(&mut conn, &thread_key)?;
    load_queue_task_from_conn(&conn, &current.message_key)?
        .context("failed to load updated queue task")
}

pub fn lease_queue_task(
    root: &Path,
    message_key: &str,
    lease_owner: &str,
) -> Result<QueueTaskView> {
    let normalized_owner = lease_owner.trim();
    anyhow::ensure!(
        !normalized_owner.is_empty(),
        "lease owner must not be empty"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let current =
        load_queue_message_from_conn(&conn, message_key)?.context("queue task not found")?;
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        VALUES (?1, 'leased', ?2, ?3, NULL, NULL, ?3)
        ON CONFLICT(message_key) DO UPDATE SET
            route_status='leased',
            lease_owner=excluded.lease_owner,
            leased_at=excluded.leased_at,
            acked_at=NULL,
            updated_at=excluded.updated_at
        "#,
        params![message_key, normalized_owner, now],
    )?;
    refresh_thread(&mut conn, &current.thread_key)?;
    load_queue_task_from_conn(&conn, message_key)?.context("failed to load leased queue task")
}

pub fn release_stale_queue_task_leases(
    root: &Path,
    lease_owner: &str,
    active_message_keys: &HashSet<String>,
) -> Result<Vec<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND r.route_status = 'leased'
          AND r.lease_owner = ?1
        ORDER BY r.leased_at ASC, r.updated_at ASC
        LIMIT 128
        "#,
    )?;
    let rows = statement.query_map(params![lease_owner], |row| row.get::<_, String>(0))?;
    let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let now = now_iso_string();
    let mut released = Vec::new();
    for message_key in candidates {
        if active_message_keys.contains(&message_key) {
            continue;
        }
        conn.execute(
            r#"
            UPDATE communication_routing_state
            SET route_status='pending',
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=NULL,
                last_error=NULL,
                updated_at=?2
            WHERE message_key = ?1
              AND route_status = 'leased'
            "#,
            params![message_key, now],
        )?;
        released.push(message_key);
    }
    Ok(released)
}

pub fn ingest_cron_message(
    root: &Path,
    run_id: &str,
    thread_key: &str,
    task_name: &str,
    body: &str,
    skill: Option<&str>,
    scheduled_for: &str,
) -> Result<String> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_account(
        &mut conn,
        "cron:system",
        "cron",
        "ctox scheduler",
        "system",
        json!({"source": "cron"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!("cron-{run_id}");
    let message_key = format!("cron:system::{remote_id}");
    let metadata = json!({
        "source": "ctox-schedule",
        "task_name": task_name,
        "skill": skill,
        "scheduled_for": scheduled_for,
        "run_id": run_id,
    });
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("task_name".to_string(), task_name.to_string());
    edge_metadata.insert("scheduled_for".to_string(), scheduled_for.to_string());
    enforce_core_spawn(
        &conn,
        &CoreSpawnRequest {
            parent_entity_type: "ScheduleTask".to_string(),
            parent_entity_id: task_name.to_string(),
            child_entity_type: "Message".to_string(),
            child_entity_id: message_key.clone(),
            spawn_kind: "schedule-run-message".to_string(),
            spawn_reason: "emit_due_schedule".to_string(),
            actor: "ctox-schedule".to_string(),
            checkpoint_key: Some(run_id.to_string()),
            budget_key: Some(format!("schedule-run:{task_name}:{scheduled_for}")),
            max_attempts: Some(64),
            metadata: edge_metadata,
        },
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "cron",
            account_key: "cron:system",
            thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "schedule",
            sender_display: "CTOX scheduler",
            sender_address: "cron:system",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: task_name,
            preview: &preview_text(body, task_name),
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: scheduled_for,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, thread_key)?;
    ensure_routing_rows_for_inbound(&conn)?;
    Ok(message_key)
}

pub fn ingest_plan_message(
    root: &Path,
    goal_id: &str,
    step_id: &str,
    thread_key: &str,
    goal_title: &str,
    step_title: &str,
    body: &str,
    skill: Option<&str>,
    step_order: i64,
    total_steps: i64,
) -> Result<String> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_account(
        &mut conn,
        "plan:system",
        "plan",
        "ctox planner",
        "system",
        json!({"source": "plan"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!("plan-{goal_id}-{step_id}");
    let message_key = format!("plan:system::{goal_id}::{step_id}");
    let metadata = json!({
        "source": "ctox-plan",
        "goal_id": goal_id,
        "step_id": step_id,
        "goal_title": goal_title,
        "step_title": step_title,
        "skill": skill,
        "step_order": step_order,
        "total_steps": total_steps,
    });
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("goal_id".to_string(), goal_id.to_string());
    edge_metadata.insert("goal_title".to_string(), goal_title.to_string());
    edge_metadata.insert("step_title".to_string(), step_title.to_string());
    enforce_core_spawn(
        &conn,
        &CoreSpawnRequest {
            parent_entity_type: "PlanStep".to_string(),
            parent_entity_id: step_id.to_string(),
            child_entity_type: "Message".to_string(),
            child_entity_id: message_key.clone(),
            spawn_kind: "plan-step-message".to_string(),
            spawn_reason: "emit_plan_step".to_string(),
            actor: "ctox-plan".to_string(),
            checkpoint_key: Some(step_id.to_string()),
            budget_key: Some(format!("plan-step:{step_id}")),
            max_attempts: Some(8),
            metadata: edge_metadata,
        },
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "plan",
            account_key: "plan:system",
            thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "plan",
            sender_display: "CTOX planner",
            sender_address: "plan:system",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: step_title,
            preview: &preview_text(body, step_title),
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, thread_key)?;
    ensure_routing_rows_for_inbound(&conn)?;
    Ok(message_key)
}

#[derive(Debug, Serialize)]
struct ChannelMessageView {
    message_key: String,
    channel: String,
    account_key: String,
    thread_key: String,
    remote_id: String,
    direction: String,
    folder_hint: String,
    sender_display: String,
    sender_address: String,
    subject: String,
    preview: String,
    body_text: String,
    status: String,
    seen: bool,
    external_created_at: String,
    observed_at: String,
    metadata: Value,
    routing: RoutingView,
}

#[derive(Debug)]
struct MessageAddressing {
    recipient_addresses: Vec<String>,
    cc_addresses: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CommunicationStateCandidate {
    kind: String,
    message_key: String,
    channel: String,
    thread_key: String,
    created_at: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct CommunicationContextView {
    thread_key: String,
    latest_subject: Option<String>,
    latest_inbound: Option<CommunicationStateCandidate>,
    latest_outbound: Option<CommunicationStateCandidate>,
    thread_messages: Vec<ChannelMessageView>,
    related_messages: Vec<ChannelMessageView>,
    candidate_blockers: Vec<CommunicationStateCandidate>,
    candidate_promises: Vec<CommunicationStateCandidate>,
    open_owner_questions: Vec<CommunicationStateCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutedInboundMessage {
    pub message_key: String,
    pub channel: String,
    pub account_key: String,
    pub thread_key: String,
    pub sender_display: String,
    pub sender_address: String,
    pub subject: String,
    pub preview: String,
    pub body_text: String,
    pub external_created_at: String,
    pub workspace_root: Option<String>,
    pub metadata: Value,
    pub preferred_reply_modality: Option<String>,
}

#[derive(Debug, Serialize)]
struct RoutingView {
    route_status: String,
    lease_owner: Option<String>,
    leased_at: Option<String>,
    acked_at: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct TuiIngestRequest {
    account_key: String,
    thread_key: String,
    body: String,
    subject: String,
    sender_display: String,
    sender_address: String,
    metadata: Value,
}

#[derive(Debug)]
struct ChannelSendRequest {
    channel: String,
    account_key: String,
    thread_key: String,
    body: String,
    subject: String,
    to: Vec<String>,
    cc: Vec<String>,
    attachments: Vec<String>,
    sender_display: Option<String>,
    sender_address: Option<String>,
    send_voice: bool,
    reviewed_founder_send: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FounderReplyAction {
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FounderOutboundAction {
    pub account_key: String,
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

fn sync_channel(root: &Path, db_path: &Path, channel: &str, args: &[String]) -> Result<Value> {
    let conn = open_channel_db(db_path)?;
    match communication_adapters::external_adapter_for_channel(channel) {
        Some(communication_adapters::ExternalCommunicationAdapter::Email(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Jami(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Meeting(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Teams(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Whatsapp(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        None => anyhow::bail!("unsupported channel sync target: {channel}"),
    }
}

fn send_message(root: &Path, db_path: &Path, request: ChannelSendRequest) -> Result<Value> {
    let mut conn = open_channel_db(db_path)?;
    let request = resolve_outbound_subject(&conn, request)?;
    enforce_external_work_ack_has_pipeline_backing(&conn, &request)?;
    enforce_channel_attachment_support(&request)?;
    match request.channel.as_str() {
        "tui" => {
            let message_key = store_tui_outbound_message(&mut conn, &request)?;
            Ok(json!({
                "ok": true,
                "channel": "tui",
                "db_path": db_path,
                "message_key": message_key,
                "status": "sent",
            }))
        }
        "email" => {
            let settings = runtime_settings_with_owner_profiles(
                root,
                communication_gateway::CommunicationAdapterKind::Email,
            );
            let protected = protected_recipient_policies(&settings, &request);
            if request.reviewed_founder_send && !protected.is_empty() {
                return send_reviewed_founder_outbound_request(root, &conn, db_path, &request);
            }
            validate_founder_outbound_email(&settings, &request)?;
            send_email_message(root, &conn, db_path, &request, None)
        }
        "jami" => {
            let adapter = communication_adapters::jami();
            let sender = request
                .sender_address
                .clone()
                .unwrap_or_else(|| jami_address_from_account_key(&request.account_key));
            let send_voice =
                request.send_voice || thread_prefers_voice_reply(&conn, &request.thread_key)?;
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::JamiSendCommandRequest {
                    db_path,
                    account_id: &sender,
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    subject: &request.subject,
                    body: &request.body,
                    send_voice,
                    attachments: &request.attachments,
                },
            )?;
            Ok(json!({
                "ok": true,
                "channel": "jami",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("queued"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "teams" => {
            let adapter = communication_adapters::teams();
            let account_config = load_account_config(&conn, &request.account_key)?;
            let tenant_id = teams_tenant_from_account_config(account_config.as_ref());
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::TeamsSendCommandRequest {
                    db_path,
                    tenant_id: tenant_id.as_deref().unwrap_or_default(),
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    subject: &request.subject,
                    body: &request.body,
                    attachments: &request.attachments,
                },
            )?;
            Ok(json!({
                "ok": true,
                "channel": "teams",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "whatsapp" => {
            let adapter = communication_adapters::whatsapp();
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::WhatsappSendCommandRequest {
                    db_path,
                    account_key: &request.account_key,
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    body: &request.body,
                    attachments: &request.attachments,
                },
            )?;
            Ok(json!({
                "ok": true,
                "channel": "whatsapp",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "meeting" => {
            let adapter = communication_adapters::meeting();
            let session_id = &request.thread_key;
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::MeetingSendCommandRequest {
                    db_path,
                    session_id,
                    body: &request.body,
                },
            )?;
            Ok(json!({
                "ok": true,
                "channel": "meeting",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "adapter_result": adapter_json,
            }))
        }
        other => anyhow::bail!("unsupported channel send target: {other}"),
    }
}

fn enforce_channel_attachment_support(request: &ChannelSendRequest) -> Result<()> {
    let _ = request;
    Ok(())
}

fn enforce_external_work_ack_has_pipeline_backing(
    conn: &Connection,
    request: &ChannelSendRequest,
) -> Result<()> {
    if !matches!(
        request.channel.as_str(),
        "teams" | "jami" | "whatsapp" | "meeting"
    ) {
        return Ok(());
    }
    if !body_promises_follow_up_work(&request.body) {
        return Ok(());
    }
    if thread_has_open_work_backing(conn, &request.thread_key)? {
        return Ok(());
    }
    anyhow::bail!(
        "outbound {} acknowledgement promises follow-up work but no durable queue, plan, or self-work item exists for thread `{}`. Create the pipeline item first, then send the acknowledgement.",
        request.channel,
        request.thread_key
    )
}

fn body_promises_follow_up_work(body: &str) -> bool {
    let normalized = format!(
        "{} {}",
        body.to_lowercase(),
        normalize_deliverable_text(body)
    );
    text_mentions_any(
        &normalized,
        &[
            "ich scrolle",
            "ich uebertrage",
            "ich übertrage",
            "ich erstelle",
            "ich bearbeite",
            "ich kuemmere",
            "ich kümmere",
            "ich pruefe",
            "ich prüfe",
            "ich recherchiere",
            "ich lese",
            "ich extrahiere",
            "ich sende",
            "ich melde",
            "ich mache",
            "ich werde",
            "werde ich",
            "i will",
            "i ll",
            "i am going to",
            "i will check",
            "i will create",
            "i will send",
            "working on it",
        ],
    )
}

fn thread_has_open_work_backing(conn: &Connection, thread_key: &str) -> Result<bool> {
    if open_queue_backing_exists(conn, thread_key)? {
        return Ok(true);
    }
    if table_exists(conn, "planned_goals")? && open_plan_backing_exists(conn, thread_key)? {
        return Ok(true);
    }
    if table_exists(conn, "ticket_self_work_items")?
        && open_self_work_backing_exists(conn, thread_key)?
    {
        return Ok(true);
    }
    Ok(false)
}

fn open_queue_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND m.thread_key = ?1
          AND COALESCE(r.route_status, 'pending') NOT IN ('handled', 'cancelled', 'failed', 'superseded')
        "#,
        params![thread_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn open_plan_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM planned_goals
        WHERE thread_key = ?1
          AND status NOT IN ('completed', 'closed', 'cancelled', 'failed', 'superseded')
        "#,
        params![thread_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn open_self_work_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ticket_self_work_items
        WHERE state NOT IN ('closed', 'cancelled', 'failed', 'superseded', 'blocked')
          AND (
            json_extract(metadata_json, '$.thread_key') = ?1
            OR json_extract(metadata_json, '$.parent_thread_key') = ?1
            OR body_text LIKE '%' || ?1 || '%'
          )
        "#,
        params![thread_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        params![table_name],
        |_| Ok(true),
    )
    .optional()
    .map(|value| value.unwrap_or(false))
    .map_err(anyhow::Error::from)
}

fn load_message_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<ChannelMessageView>> {
    conn.query_row(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.message_key = ?1
        LIMIT 1
        "#,
        params![message_key],
        map_channel_message_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_message_addressing_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<MessageAddressing>> {
    conn.query_row(
        r#"
        SELECT recipient_addresses_json, cc_addresses_json
        FROM communication_messages
        WHERE message_key = ?1
        LIMIT 1
        "#,
        params![message_key],
        |row| {
            Ok(MessageAddressing {
                recipient_addresses: parse_string_json_array(&row.get::<_, String>(0)?),
                cc_addresses: parse_string_json_array(&row.get::<_, String>(1)?),
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn normalize_email_list(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_email_address(trimmed);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        ordered.push(trimmed.to_string());
    }
    ordered
}

fn normalize_deliverable_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn text_mentions_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn detect_required_founder_deliverables(subject: &str, body: &str) -> Vec<String> {
    let normalized = format!(
        "{} {}",
        normalize_deliverable_text(subject),
        normalize_deliverable_text(body)
    );
    let mut required = Vec::new();
    if text_mentions_any(&normalized, &["qr code", "qrcode", "jami qr", "qr zugang"]) {
        required.push("qr_code".to_string());
    }
    if text_mentions_any(
        &normalized,
        &[
            "5 mockups",
            "fuenf mockups",
            "fuenf verschiedenen design vorlagen",
            "5 verschiedenen design vorlagen",
            "mockups",
            "entwuerfe",
            "entwurfe",
            "standalone html mockup",
        ],
    ) {
        required.push("mockup_links_or_files".to_string());
    }
    if text_mentions_any(
        &normalized,
        &[
            "link set",
            "linkset",
            "links schicken",
            "schick links",
            "verlinkten zwischenstand",
            "oeffentlichen links",
            "offentlichen links",
        ],
    ) {
        required.push("link_set".to_string());
    }
    normalize_email_list(required)
}

fn attachments_satisfy_deliverable(attachments: &[String], deliverable: &str) -> bool {
    let lowered = attachments
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match deliverable {
        "qr_code" => lowered.iter().any(|value| {
            (value.contains("jami") || value.contains("qr")) && value.ends_with(".pdf")
        }),
        "mockup_links_or_files" => lowered.iter().any(|value| {
            value.ends_with(".html") || value.ends_with(".pdf") || value.ends_with(".png")
        }),
        "link_set" => false,
        _ => false,
    }
}

fn founder_reply_satisfies_deliverable(
    body: &str,
    attachments: &[String],
    deliverable: &str,
) -> bool {
    if attachments_satisfy_deliverable(attachments, deliverable) {
        return true;
    }
    let normalized = normalize_deliverable_text(body);
    match deliverable {
        "qr_code" => text_mentions_any(&normalized, &["qr code", "qrcode", "jami qr", "qr zugang"]),
        "mockup_links_or_files" => text_mentions_any(
            &normalized,
            &[
                "mockup",
                "entwurf",
                "design vorlage",
                "html",
                "http",
                "https",
                "link",
            ],
        ),
        "link_set" => text_mentions_any(&normalized, &["http", "https", "link", "links"]),
        _ => true,
    }
}

fn prepare_founder_reply_attachments(
    root: &Path,
    subject: &str,
    body: &str,
) -> Result<Vec<String>> {
    let required = detect_required_founder_deliverables(subject, body);
    let mut attachments = Vec::new();
    if required.iter().any(|value| value == "qr_code")
        && normalize_deliverable_text(&format!("{subject} {body}")).contains("jami")
    {
        attachments.push(generate_jami_setup_pdf_artifact(root)?);
    }
    Ok(attachments)
}

fn generate_jami_setup_pdf_artifact(root: &Path) -> Result<String> {
    let settings = communication_gateway::runtime_settings_from_root(
        root,
        communication_gateway::CommunicationAdapterKind::Jami,
    );
    let account_id = settings
        .get("CTO_JAMI_ACCOUNT_ID")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .context("missing CTO_JAMI_ACCOUNT_ID for Jami QR artifact generation")?;
    let profile_name = settings
        .get("CTO_JAMI_PROFILE_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("CTO1");
    let share_uri = format!("jami:{account_id}");
    let artifact_dir = root.join("runtime/communication/artifacts/jami");
    fs::create_dir_all(&artifact_dir).with_context(|| {
        format!(
            "failed to create Jami artifact dir {}",
            artifact_dir.display()
        )
    })?;
    let file_name = format!(
        "ctox-jami-setup-{}.pdf",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let path = artifact_dir.join(file_name);
    let bytes = build_simple_jami_setup_pdf(profile_name, &share_uri)?;
    fs::write(&path, bytes)
        .with_context(|| format!("failed to write Jami setup PDF {}", path.display()))?;
    Ok(path.display().to_string())
}

fn build_simple_jami_setup_pdf(profile_name: &str, share_uri: &str) -> Result<Vec<u8>> {
    let qr = QrCode::new(share_uri.as_bytes()).context("failed to build Jami QR code")?;
    let width = qr.width();
    let colors = qr.to_colors();
    let mut content = String::new();
    content.push_str("BT /F1 20 Tf 72 760 Td ");
    content.push_str(&pdf_text(profile_name));
    content.push_str(" Tj ET\n");
    content.push_str("BT /F1 12 Tf 72 738 Td ");
    content.push_str(&pdf_text("Scan this QR code in Jami or use the URI below."));
    content.push_str(" Tj ET\n");
    content.push_str("BT /F1 11 Tf 72 718 Td ");
    content.push_str(&pdf_text(share_uri));
    content.push_str(" Tj ET\n");
    content.push_str("0 0 0 rg\n");
    let module = 5.0f32;
    let origin_x = 72.0f32;
    let origin_y = 420.0f32;
    for y in 0..width {
        for x in 0..width {
            let idx = y * width + x;
            if matches!(colors.get(idx), Some(QrColor::Dark)) {
                let px = origin_x + (x as f32 * module);
                let py = origin_y + ((width - 1 - y) as f32 * module);
                content.push_str(&format!("{px:.2} {py:.2} {module:.2} {module:.2} re f\n"));
            }
        }
    }
    content.push_str("BT /F1 10 Tf 72 396 Td ");
    content.push_str(&pdf_text("Account name:"));
    content.push_str(" Tj ET\n");
    content.push_str("BT /F1 10 Tf 140 396 Td ");
    content.push_str(&pdf_text(profile_name));
    content.push_str(" Tj ET\n");
    content.push_str("BT /F1 10 Tf 72 380 Td ");
    content.push_str(&pdf_text("Fallback URI:"));
    content.push_str(" Tj ET\n");
    content.push_str("BT /F1 10 Tf 140 380 Td ");
    content.push_str(&pdf_text(share_uri));
    content.push_str(" Tj ET\n");

    let mut objects = Vec::new();
    objects.push("1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string());
    objects.push("2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string());
    objects.push("3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >> endobj\n".to_string());
    objects.push(
        "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
    );
    objects.push(format!(
        "5 0 obj << /Length {} >> stream\n{}endstream\nendobj\n",
        content.as_bytes().len(),
        content
    ));

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for object in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(object.as_bytes());
    }
    let xref_start = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_start
        )
        .as_bytes(),
    );
    Ok(pdf)
}

fn pdf_text(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    format!("({escaped})")
}

fn derives_targets_from_forward(subject: &str, body: &str) -> bool {
    let lowered_subject = subject.to_ascii_lowercase();
    if lowered_subject.starts_with("fwd:") || lowered_subject.starts_with("fw:") {
        return true;
    }
    let lowered_body = body.to_ascii_lowercase();
    lowered_body.contains("weitergeleiteten nachricht")
        || lowered_body.contains("begin forwarded message")
        || lowered_body.contains("forwarded message")
}

fn derive_founder_reply_recipients(
    inbound: &ChannelMessageView,
    addressing: &MessageAddressing,
) -> (Vec<String>, Vec<String>) {
    let account_email =
        normalize_email_address(&email_address_from_account_key(&inbound.account_key));
    let sender_email = normalize_email_address(&inbound.sender_address);

    let filter_external = |values: &[String]| {
        values
            .iter()
            .filter(|value| {
                let normalized = normalize_email_address(value);
                !normalized.is_empty() && normalized != account_email && normalized != sender_email
            })
            .cloned()
            .collect::<Vec<_>>()
    };

    let external_to = normalize_email_list(filter_external(&addressing.recipient_addresses));
    let external_cc = normalize_email_list(filter_external(&addressing.cc_addresses));

    if derives_targets_from_forward(&inbound.subject, &inbound.body_text) && !external_to.is_empty()
    {
        let mut cc = vec![inbound.sender_address.clone()];
        cc.extend(external_cc);
        return (external_to, normalize_email_list(cc));
    }

    let mut cc = external_to;
    cc.extend(external_cc);
    (
        vec![inbound.sender_address.clone()],
        normalize_email_list(cc),
    )
}

fn protected_recipient_policies(
    settings: &BTreeMap<String, String>,
    request: &ChannelSendRequest,
) -> Vec<EmailSenderPolicy> {
    request
        .to
        .iter()
        .chain(request.cc.iter())
        .map(|email| classify_email_sender(settings, email))
        .filter(|policy| matches!(policy.role.as_str(), "owner" | "founder" | "admin"))
        .collect::<Vec<_>>()
}

fn ensure_founder_outbound_body_clean(request: &ChannelSendRequest) -> Result<()> {
    let lowered = request.body.to_ascii_lowercase();
    let first_lines = request
        .body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(5)
        .collect::<Vec<_>>();
    let header_preamble_hits = first_lines
        .iter()
        .filter(|line| {
            let lowered = line.to_ascii_lowercase();
            lowered.starts_with("an:")
                || lowered.starts_with("to:")
                || lowered.starts_with("cc:")
                || lowered.starts_with("bcc:")
                || lowered.starts_with("betreff:")
                || lowered.starts_with("subject:")
        })
        .copied()
        .collect::<Vec<_>>();
    if !header_preamble_hits.is_empty() {
        anyhow::bail!(
            "founder/owner outbound email failed communication review because addressing or subject headers were placed in the message body: {}",
            header_preamble_hits.join(", ")
        );
    }
    let forbidden_markers = [
        "/home/",
        "queue:",
        "runtime/ctox.sqlite3",
        "strategic direction setup",
        "review rework",
        "review-rework",
        "self-work",
        "thread_key",
        "message_key",
        "conversation_id",
        "lease_owner",
        "route_status",
        "routing-state",
        "review-approval",
        "review approval",
        "send-proof",
        "send proof",
        "outbound-message-row",
        "outbound message row",
        "review/send proof",
        "inbound `email:",
        "steht jetzt auf `handled`",
        "status `handled`",
        "sqlite",
        "host-pfad",
        "host-pfade",
        "vps-pfad",
        "api.qrserver.com",
        "qrserver.com",
        "public server",
        "public link",
        "oeffentlicher server",
        "oeffentlicher link",
        "offentlicher server",
        "offentlicher link",
    ];
    let hits = forbidden_markers
        .iter()
        .filter(|marker| lowered.contains(**marker))
        .copied()
        .collect::<Vec<_>>();
    if !hits.is_empty() {
        anyhow::bail!(
            "founder/owner outbound email failed communication review due to internal-language leakage: {}",
            hits.join(", ")
        );
    }
    Ok(())
}

fn send_email_message(
    root: &Path,
    conn: &Connection,
    db_path: &Path,
    request: &ChannelSendRequest,
    reviewed_context: Option<ReviewedFounderSendContext<'_>>,
) -> Result<Value> {
    let adapter = communication_adapters::email();
    let sender_email = request
        .sender_address
        .clone()
        .unwrap_or_else(|| email_address_from_account_key(&request.account_key));
    let account_config = load_account_config(conn, &request.account_key)?;
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let approval_key = reviewed_context
        .map(|context| context.approval_key)
        .unwrap_or("");
    let pending_message_key =
        record_outbound_pending_send(conn, request, approval_key, &body_sha256)?;
    let adapter_json = match adapter.send_cli(
        root,
        &communication_adapters::EmailSendCommandRequest {
            db_path,
            sender_email: &sender_email,
            provider: account_config
                .as_ref()
                .map(|config| config.provider.as_str()),
            profile_json: account_config.as_ref().map(|config| &config.profile_json),
            thread_key: &request.thread_key,
            to: &request.to,
            cc: &request.cc,
            sender_display: request.sender_display.as_deref(),
            subject: &request.subject,
            body: &request.body,
            attachments: &request.attachments,
        },
    ) {
        Ok(value) => value,
        Err(err) => {
            let _ = mark_outbound_send_failed(conn, &pending_message_key, &err.to_string());
            if let Some(context) = reviewed_context {
                let _ = enforce_reviewed_founder_send_failed_core_transition(
                    conn,
                    context.entity_id,
                    context.approval_key,
                    request,
                    &pending_message_key,
                    &err.to_string(),
                );
            }
            return Err(err);
        }
    };
    let status = adapter_json
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    mark_outbound_send_accepted(conn, &pending_message_key, status, &adapter_json)?;
    Ok(json!({
        "ok": true,
        "channel": "email",
        "db_path": db_path,
        "message_key": pending_message_key,
        "status": status,
        "delivery_confirmed": adapter_json
            .get("delivery")
            .and_then(|value| value.get("confirmed"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "adapter_result": adapter_json,
    }))
}

#[derive(Debug, Clone, Copy)]
struct ReviewedFounderSendContext<'a> {
    entity_id: &'a str,
    approval_key: &'a str,
}

fn record_outbound_pending_send(
    conn: &Connection,
    request: &ChannelSendRequest,
    approval_key: &str,
    body_sha256: &str,
) -> Result<String> {
    let observed_at = now_iso_string();
    let message_key = pending_send_message_key(request, body_sha256);
    let remote_id = format!("pending-send-{}", stable_digest(&message_key));
    let recipient_set_sha256 = founder_send_recipient_set_sha256(request);
    let sender_email = request
        .sender_address
        .clone()
        .unwrap_or_else(|| email_address_from_account_key(&request.account_key));
    let metadata_json = serde_json::to_string(&json!({
        "source": "ctox-send-durability",
        "pendingSend": true,
        "pending_send": true,
        "reviewedFounderSend": request.reviewed_founder_send,
        "attachments": request.attachments,
        "approval_key": approval_key,
        "body_sha256": body_sha256,
        "recipient_set_sha256": recipient_set_sha256,
        "phase": "phase1_body_durability",
    }))?;
    conn.execute(
        r#"
        INSERT INTO communication_messages (
            message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
            sender_display, sender_address, recipient_addresses_json, cc_addresses_json, bcc_addresses_json,
            subject, preview, body_text, body_html, raw_payload_ref, trust_level, status, seen,
            has_attachments, external_created_at, observed_at, metadata_json
        ) VALUES (
            ?1, 'email', ?2, ?3, ?4, 'outbound', 'outbox',
            ?5, ?6, ?7, ?8, '[]',
            ?9, ?10, ?11, '', ?12, 'high', 'draft_pending_send', 1,
            ?13, ?14, ?14, ?15
        )
        ON CONFLICT(message_key) DO UPDATE SET
            folder_hint='outbox',
            status='draft_pending_send',
            body_text=excluded.body_text,
            metadata_json=excluded.metadata_json,
            observed_at=excluded.observed_at
        "#,
        params![
            message_key,
            request.account_key,
            request.thread_key,
            remote_id,
            request.sender_display.as_deref().unwrap_or(""),
            sender_email,
            serde_json::to_string(&request.to)?,
            serde_json::to_string(&request.cc)?,
            request.subject,
            preview_text(&request.body, &request.subject),
            request.body,
            request.attachments.join("\n"),
            if request.attachments.is_empty() { 0 } else { 1 },
            observed_at,
            metadata_json,
        ],
    )?;
    Ok(message_key)
}

fn mark_outbound_send_accepted(
    conn: &Connection,
    message_key: &str,
    status: &str,
    adapter_json: &Value,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE communication_messages
        SET status = ?2,
            folder_hint = 'sent',
            metadata_json = json_set(
                json_set(metadata_json, '$.pendingSend', false),
                '$.adapterResult',
                json(?3)
            ),
            observed_at = ?4
        WHERE message_key = ?1
        "#,
        params![
            message_key,
            status,
            serde_json::to_string(adapter_json)?,
            now_iso_string()
        ],
    )?;
    Ok(())
}

fn mark_outbound_send_failed(conn: &Connection, message_key: &str, error: &str) -> Result<()> {
    conn.execute(
        r#"
        UPDATE communication_messages
        SET status = 'send_failed',
            metadata_json = json_set(
                json_set(metadata_json, '$.pendingSend', false),
                '$.sendError',
                ?2
            ),
            observed_at = ?3
        WHERE message_key = ?1
        "#,
        params![message_key, error, now_iso_string()],
    )?;
    Ok(())
}

pub(crate) fn prepare_reviewed_founder_reply(
    root: &Path,
    inbound_message_key: &str,
) -> Result<FounderReplyAction> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let inbound = load_message_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing inbound communication message {inbound_message_key}"))?;
    anyhow::ensure!(
        inbound.channel == "email" && inbound.direction == "inbound",
        "reviewed founder reply requires an inbound email message"
    );
    let addressing = load_message_addressing_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing communication addressing for {inbound_message_key}"))?;
    let (to, cc) = derive_founder_reply_recipients(&inbound, &addressing);
    let attachments =
        prepare_founder_reply_attachments(root, &inbound.subject, &inbound.body_text)?;
    let request = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: inbound.account_key.clone(),
            thread_key: inbound.thread_key.clone(),
            body: String::new(),
            subject: format!("Re: {}", inbound.subject.trim()),
            to,
            cc,
            attachments,
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        },
    )?;
    Ok(FounderReplyAction {
        thread_key: request.thread_key,
        subject: request.subject,
        to: request.to,
        cc: request.cc,
        attachments: request.attachments,
    })
}

pub(crate) fn required_founder_reply_deliverables(
    root: &Path,
    inbound_message_key: &str,
) -> Result<Vec<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let inbound = load_message_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing inbound communication message {inbound_message_key}"))?;
    Ok(detect_required_founder_deliverables(
        &inbound.subject,
        &inbound.body_text,
    ))
}

pub(crate) fn ensure_founder_reply_deliverables_present(
    root: &Path,
    inbound_message_key: &str,
    body: &str,
    attachments: &[String],
) -> Result<()> {
    let required = required_founder_reply_deliverables(root, inbound_message_key)?;
    let missing = required
        .into_iter()
        .filter(|deliverable| !founder_reply_satisfies_deliverable(body, attachments, deliverable))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!(
            "founder reply is missing required deliverable(s): {}",
            missing.join(", ")
        );
    }
    Ok(())
}

pub(crate) fn record_founder_reply_review_approval(
    root: &Path,
    inbound_message_key: &str,
    body: &str,
    review_summary: &str,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let action = prepare_reviewed_founder_reply(root, inbound_message_key)?;
    let (action_digest, action_json, body_sha256) = founder_reply_review_digest(&action, body);
    let approval_key = format!("founder-review:{inbound_message_key}:{action_digest}");
    conn.execute(
        r#"
        INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json,
            body_sha256, reviewer, review_summary, approved_at, sent_at, send_result_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'external-review', ?6, ?7, NULL, '{}')
        ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET
            approval_key=excluded.approval_key,
            action_json=excluded.action_json,
            body_sha256=excluded.body_sha256,
            reviewer=excluded.reviewer,
            review_summary=excluded.review_summary,
            approved_at=excluded.approved_at,
            sent_at=NULL,
            send_result_json='{}'
        "#,
        params![
            approval_key,
            inbound_message_key,
            action_digest,
            action_json,
            body_sha256,
            review_summary,
            now_iso_string()
        ],
    )
    .context("failed to record founder reply review approval")?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "review.approved",
            title: "Review approved",
            body_text: review_summary,
            message_key: Some(inbound_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "body_sha256": body_sha256,
                "action_digest": action_digest,
            }),
        },
    );
    Ok(())
}

fn founder_reply_review_digest(
    action: &FounderReplyAction,
    body: &str,
) -> (String, String, String) {
    let action_json = json!({
        "thread_key": &action.thread_key,
        "subject": &action.subject,
        "to": &action.to,
        "cc": &action.cc,
        "attachments": &action.attachments,
    })
    .to_string();
    let body_sha256 = format!("{:x}", Sha256::digest(body.trim().as_bytes()));
    let mut hasher = Sha256::new();
    hasher.update(action_json.as_bytes());
    hasher.update(b"\0");
    hasher.update(body_sha256.as_bytes());
    let action_digest = format!("{:x}", hasher.finalize());
    (action_digest, action_json, body_sha256)
}

fn founder_outbound_review_digest(
    action: &FounderOutboundAction,
    body: &str,
) -> (String, String, String) {
    let action_json = json!({
        "account_key": &action.account_key,
        "thread_key": &action.thread_key,
        "subject": &action.subject,
        "to": &action.to,
        "cc": &action.cc,
        "attachments": &action.attachments,
    })
    .to_string();
    let body_sha256 = format!("{:x}", Sha256::digest(body.trim().as_bytes()));
    let mut hasher = Sha256::new();
    hasher.update(action_json.as_bytes());
    hasher.update(b"\0");
    hasher.update(body_sha256.as_bytes());
    let action_digest = format!("{:x}", hasher.finalize());
    (action_digest, action_json, body_sha256)
}

pub(crate) fn default_email_account_key(root: &Path) -> Result<String> {
    let db_path = resolve_db_path(root, None);
    bootstrap_channel_account(root, "email")?;
    let conn = open_channel_db(&db_path)?;
    resolve_account_key(&conn, "email", None)
}

pub(crate) fn terminal_founder_outbound_artifact_count(
    root: &Path,
    action: &FounderOutboundAction,
) -> Result<i64> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let to_json = serde_json::to_string(&action.to)?;
    let cc_json = serde_json::to_string(&action.cc)?;
    let attachments = action.attachments.join("\n");
    conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages
        WHERE channel = 'email'
          AND direction = 'outbound'
          AND status IN ('accepted', 'sent')
          AND lower(account_key) = lower(?1)
          AND thread_key = ?2
          AND subject = ?3
          AND recipient_addresses_json = ?4
          AND cc_addresses_json = ?5
          AND raw_payload_ref = ?6
        "#,
        params![
            action.account_key,
            action.thread_key,
            action.subject,
            to_json,
            cc_json,
            attachments
        ],
        |row| row.get(0),
    )
    .context("failed to count terminal founder outbound artifacts")
}

pub(crate) fn record_founder_outbound_review_approval(
    root: &Path,
    anchor_message_key: &str,
    action: &FounderOutboundAction,
    body: &str,
    review_summary: &str,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let (action_digest, action_json, body_sha256) = founder_outbound_review_digest(action, body);
    let approval_key = format!("founder-outbound-review:{anchor_message_key}:{action_digest}");
    conn.execute(
        r#"
        INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json,
            body_sha256, reviewer, review_summary, approved_at, sent_at, send_result_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'external-review', ?6, ?7, NULL, '{}')
        ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET
            approval_key=excluded.approval_key,
            action_json=excluded.action_json,
            body_sha256=excluded.body_sha256,
            reviewer=excluded.reviewer,
            review_summary=excluded.review_summary,
            approved_at=excluded.approved_at,
            sent_at=NULL,
            send_result_json='{}'
        "#,
        params![
            approval_key,
            anchor_message_key,
            action_digest,
            action_json,
            body_sha256,
            review_summary,
            now_iso_string()
        ],
    )
    .context("failed to record founder outbound review approval")?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "review.approved",
            title: "Review approved",
            body_text: review_summary,
            message_key: Some(anchor_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "body_sha256": body_sha256,
                "action_digest": action_digest,
                "outbound": true,
            }),
        },
    );
    Ok(())
}

/// Persist a structured "no-send" verdict for an inbound message. The
/// terminal NO-SEND disposition is identified by a synthetic
/// `terminal-no-send:<inbound>` digest; it does not reference any
/// outbound action because the whole point of the verdict is that no
/// reply is going to be drafted. Re-recording is idempotent: the
/// underlying UNIQUE(inbound_message_key, action_digest) constraint
/// upserts on conflict.
pub fn record_terminal_no_send_verdict(
    root: &Path,
    inbound_message_key: &str,
    reviewer: &str,
    review_summary: &str,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let action_digest = format!(
        "{:x}",
        Sha256::digest(format!("terminal-no-send:{inbound_message_key}").as_bytes())
    );
    let approval_key = format!("founder-no-send:{inbound_message_key}:{action_digest}");
    let action_json = json!({
        "kind": "terminal_no_send",
        "inbound_message_key": inbound_message_key,
    })
    .to_string();
    let body_sha256 = format!("{:x}", Sha256::digest(b""));
    conn.execute(
        r#"
        INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json,
            body_sha256, reviewer, review_summary, approved_at, sent_at,
            send_result_json, terminal_no_send
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, '{}', 1)
        ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET
            approval_key=excluded.approval_key,
            action_json=excluded.action_json,
            reviewer=excluded.reviewer,
            review_summary=excluded.review_summary,
            approved_at=excluded.approved_at,
            terminal_no_send=1
        "#,
        params![
            approval_key,
            inbound_message_key,
            action_digest,
            action_json,
            body_sha256,
            reviewer,
            review_summary,
            now_iso_string()
        ],
    )
    .context("failed to record terminal NO-SEND verdict")?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "review.no_send",
            title: "Review verdict: no-send",
            body_text: review_summary,
            message_key: Some(inbound_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "terminal_no_send": true,
            }),
        },
    );
    Ok(())
}

/// Whether a structured terminal NO-SEND verdict has been recorded for
/// the inbound message. Callers (notably the rework-spawn gate) must
/// query this BEFORE creating new founder-communication rework, so a
/// later auto-classifier cannot overwrite the original NO-SEND review.
pub fn inbound_message_has_terminal_no_send(
    root: &Path,
    inbound_message_key: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let exists: i64 = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM communication_founder_reply_reviews
            WHERE inbound_message_key = ?1
              AND terminal_no_send = 1
            LIMIT 1
        )
        "#,
        params![inbound_message_key],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

/// Whether an inbound message is structurally non-actionable (i.e. an
/// auto-submitted/out-of-office reply per RFC 3834). The check looks
/// only at the metadata JSON written by the inbound parser; subject
/// and body text are not inspected here.
pub fn inbound_message_is_auto_submitted(root: &Path, inbound_message_key: &str) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let row: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            params![inbound_message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to load inbound metadata for auto-submitted check")?;
    let Some(raw) = row else {
        return Ok(false);
    };
    let metadata: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    Ok(metadata_marks_auto_submitted(&metadata))
}

fn require_unconsumed_founder_reply_review(
    conn: &Connection,
    inbound_message_key: &str,
    action: &FounderReplyAction,
    body: &str,
) -> Result<String> {
    let (action_digest, _, _) = founder_reply_review_digest(action, body);
    let approval_key = conn
        .query_row(
            r#"
            SELECT approval_key
            FROM communication_founder_reply_reviews
            WHERE inbound_message_key = ?1
              AND action_digest = ?2
              AND sent_at IS NULL
            LIMIT 1
            "#,
            params![inbound_message_key, action_digest],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to load founder reply review approval")?;
    approval_key.with_context(|| {
        "reviewed founder reply has no matching unconsumed review approval for the exact body, recipients, cc, subject, and attachments"
            .to_string()
    })
}

fn require_any_unconsumed_founder_outbound_review(
    conn: &Connection,
    action: &FounderOutboundAction,
    body: &str,
) -> Result<(String, String)> {
    let (action_digest, _, _) = founder_outbound_review_digest(action, body);
    let approval = conn
        .query_row(
            r#"
            SELECT approval_key, inbound_message_key
            FROM communication_founder_reply_reviews
            WHERE action_digest = ?1
              AND sent_at IS NULL
              AND terminal_no_send = 0
            ORDER BY approved_at DESC
            LIMIT 1
            "#,
            params![action_digest],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("failed to load founder outbound review approval")?;
    approval.with_context(|| {
        "reviewed founder outbound has no matching unconsumed review approval for the exact body, recipients, cc, subject, and attachments. Run completion review first, then send exactly the approved body with the same recipients and subject."
            .to_string()
    })
}

fn mark_founder_reply_review_sent(
    conn: &Connection,
    approval_key: &str,
    send_result: &Value,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE communication_founder_reply_reviews
        SET sent_at = ?2,
            send_result_json = ?3
        WHERE approval_key = ?1
          AND sent_at IS NULL
        "#,
        params![approval_key, now_iso_string(), send_result.to_string()],
    )
    .context("failed to mark founder reply review as sent")?;
    Ok(())
}

fn founder_reply_sent_after_review(conn: &Connection, inbound_message_key: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_founder_reply_reviews
        WHERE inbound_message_key = ?1
          AND sent_at IS NOT NULL
          AND COALESCE(json_extract(send_result_json, '$.synthetic'), 0) != 1
          AND COALESCE(json_extract(send_result_json, '$.status'), '') != 'no-send-recorded'
        "#,
        params![inbound_message_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn protected_founder_inbound_message(
    root: &Path,
    conn: &Connection,
    message_key: &str,
) -> Result<bool> {
    let Some((channel, direction, sender_address)) = conn
        .query_row(
            r#"
            SELECT channel, direction, sender_address
            FROM communication_messages
            WHERE message_key = ?1
            LIMIT 1
            "#,
            params![message_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(false);
    };
    if channel != "email" || direction != "inbound" {
        return Ok(false);
    }
    let settings = runtime_settings_with_owner_profiles(
        root,
        communication_gateway::CommunicationAdapterKind::Email,
    );
    let policy = classify_email_sender(&settings, &sender_address);
    Ok(matches!(
        policy.role.as_str(),
        "owner" | "founder" | "admin"
    ))
}

fn message_metadata_marks_auto_submitted(conn: &Connection, message_key: &str) -> Result<bool> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(false);
    };
    let metadata: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    Ok(metadata_marks_auto_submitted(&metadata))
}

fn message_has_terminal_no_send_in_conn(conn: &Connection, message_key: &str) -> Result<bool> {
    let exists: i64 = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM communication_founder_reply_reviews
            WHERE inbound_message_key = ?1
              AND terminal_no_send = 1
            LIMIT 1
        )
        "#,
        params![message_key],
        |row| row.get(0),
    )?;
    Ok(exists != 0)
}

fn guard_founder_handled_ack(
    root: &Path,
    conn: &Connection,
    message_keys: &[String],
    status: &str,
) -> Result<()> {
    if status != "handled" {
        return Ok(());
    }
    for message_key in message_keys {
        if !protected_founder_inbound_message(root, conn, message_key)? {
            continue;
        }
        if founder_reply_sent_after_review(conn, message_key)? {
            continue;
        }
        // Bug #1: an auto-submitted (RFC 3834) founder/owner/admin
        // mail does not require a reviewed reply. The structured
        // header marker is checked at ingestion time and persisted
        // into metadata_json; we only consult the structured field
        // here, never subject/body strings.
        if message_metadata_marks_auto_submitted(conn, message_key)? {
            continue;
        }
        // Bug #3: an explicit terminal NO-SEND verdict closes the
        // inbound without a reply.
        if message_has_terminal_no_send_in_conn(conn, message_key)? {
            continue;
        }
        anyhow::bail!(
            "cannot mark founder/owner/admin inbound mail as handled before an exact reviewed reply was accepted by the email adapter: {}",
            message_key
        );
    }
    Ok(())
}

pub fn send_reviewed_founder_reply(
    root: &Path,
    inbound_message_key: &str,
    body: &str,
) -> Result<Value> {
    let _send_guard = acquire_reviewed_founder_send_lock()?;
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let inbound = load_message_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing inbound communication message {inbound_message_key}"))?;
    let action = prepare_reviewed_founder_reply(root, inbound_message_key)?;
    let request = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: inbound.account_key.clone(),
            thread_key: action.thread_key.clone(),
            body: body.trim().to_string(),
            subject: action.subject.clone(),
            to: action.to.clone(),
            cc: action.cc.clone(),
            attachments: action.attachments.clone(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        },
    )?;
    let settings = runtime_settings_with_owner_profiles(
        root,
        communication_gateway::CommunicationAdapterKind::Email,
    );
    let protected = protected_recipient_policies(&settings, &request);
    anyhow::ensure!(
        !protected.is_empty(),
        "reviewed founder reply requires founder/owner/admin recipient"
    );
    let approval_key = require_unconsumed_founder_reply_review(
        &conn,
        inbound_message_key,
        &action,
        &request.body,
    )?;
    ensure_founder_outbound_body_clean(&request)?;
    ensure_founder_reply_deliverables_present(
        root,
        inbound_message_key,
        &request.body,
        &request.attachments,
    )?;
    let entity_id = format!("founder-reply:{inbound_message_key}");
    enforce_reviewed_founder_send_core_transition(&conn, &entity_id, &approval_key, &request)?;
    let send_result = send_email_message(
        root,
        &conn,
        &db_path,
        &request,
        Some(ReviewedFounderSendContext {
            entity_id: &entity_id,
            approval_key: &approval_key,
        }),
    )?;
    mark_founder_reply_review_sent(&conn, &approval_key, &send_result)?;
    Ok(send_result)
}

fn send_reviewed_founder_outbound_request(
    root: &Path,
    conn: &Connection,
    db_path: &Path,
    request: &ChannelSendRequest,
) -> Result<Value> {
    let _send_guard = acquire_reviewed_founder_send_lock()?;
    let settings = runtime_settings_with_owner_profiles(
        root,
        communication_gateway::CommunicationAdapterKind::Email,
    );
    let protected = protected_recipient_policies(&settings, request);
    anyhow::ensure!(
        !protected.is_empty(),
        "reviewed founder outbound requires founder/owner/admin recipient"
    );
    let action = FounderOutboundAction {
        account_key: request.account_key.clone(),
        thread_key: request.thread_key.clone(),
        subject: request.subject.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        attachments: request.attachments.clone(),
    };
    let (approval_key, anchor_message_key) =
        require_any_unconsumed_founder_outbound_review(conn, &action, &request.body)?;
    ensure_founder_outbound_body_clean(request)?;
    let entity_id = format!("founder-outbound:{anchor_message_key}");
    enforce_reviewed_founder_send_core_transition(conn, &entity_id, &approval_key, request)?;
    let send_result = send_email_message(
        root,
        conn,
        db_path,
        request,
        Some(ReviewedFounderSendContext {
            entity_id: &entity_id,
            approval_key: &approval_key,
        }),
    )?;
    mark_founder_reply_review_sent(conn, &approval_key, &send_result)?;
    Ok(send_result)
}

fn enforce_reviewed_founder_send_core_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
) -> Result<()> {
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let recipient_set_sha256 = founder_send_recipient_set_sha256(request);
    let mut metadata = BTreeMap::new();
    metadata.insert("protected_party".to_string(), "founder".to_string());
    metadata.insert("thread_key".to_string(), request.thread_key.clone());
    metadata.insert("subject".to_string(), request.subject.clone());
    metadata.insert("account_key".to_string(), request.account_key.clone());

    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: entity_id.to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Approved,
            to_state: CoreState::Sending,
            event: CoreEvent::Send,
            actor: "ctox-reviewed-founder-send".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some(approval_key.to_string()),
                approved_body_sha256: Some(body_sha256.clone()),
                outgoing_body_sha256: Some(body_sha256),
                approved_recipient_set_sha256: Some(recipient_set_sha256.clone()),
                outgoing_recipient_set_sha256: Some(recipient_set_sha256),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(())
}

fn acquire_reviewed_founder_send_lock() -> Result<MutexGuard<'static, ()>> {
    REVIEWED_FOUNDER_SEND_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|err| anyhow::anyhow!("reviewed founder send lock poisoned: {err}"))
}

/// Compute the deterministic `message_key` for a pending-send durable
/// outbound row. Stable for identical (account_key, thread_key, subject,
/// recipient set, body) tuples. This is the retry-binding key the
/// operator uses to resume after a provider failure (RFC 0001 §5.1).
fn pending_send_message_key(request: &ChannelSendRequest, body_sha256: &str) -> String {
    let recipient_set_sha256 = founder_send_recipient_set_sha256(request);
    let payload = format!(
        "{}|{}|{}|{}",
        request.account_key.trim(),
        request.thread_key.trim(),
        recipient_set_sha256,
        body_sha256
    );
    let digest = sha256_hex(payload.as_bytes());
    format!("{}::pending_send::{}", request.account_key.trim(), digest)
}

/// Flip a `draft_pending_send` row to `accepted` after a successful
/// provider call. The CAS on `status` is defensive: a concurrent failure-
/// path update would cause this to be a noop, which is safer than
/// silently overwriting.
fn update_pending_send_to_accepted(
    conn: &Connection,
    pending_message_key: &str,
    adapter_result: &Value,
) -> Result<()> {
    let prior_metadata = load_metadata_for_message(conn, pending_message_key)?;
    let mut metadata = prior_metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    metadata.insert("pending_send".to_string(), Value::Bool(false));
    metadata.insert(
        "transitioned_to".to_string(),
        Value::String("accepted".to_string()),
    );
    metadata.insert("adapter_result".to_string(), adapter_result.clone());
    let metadata_json = Value::Object(metadata).to_string();
    let now = now_iso_string();
    let updated = conn
        .execute(
            r#"
            UPDATE communication_messages
            SET status = 'accepted',
                metadata_json = ?2,
                observed_at = ?3
            WHERE message_key = ?1
              AND status = 'draft_pending_send'
            "#,
            params![pending_message_key, metadata_json, now],
        )
        .context("failed to mark outbound body as accepted")?;
    if updated == 0 {
        anyhow::bail!(
            "outbound durability row {} was not in draft_pending_send when accepted-update was attempted",
            pending_message_key
        );
    }
    Ok(())
}

/// Flip a `draft_pending_send` row to `send_failed` after a provider
/// failure. Body and recipients stay; the provider error is recorded in
/// `metadata_json` so the operator/retry path can read it.
fn update_pending_send_to_failed(
    conn: &Connection,
    pending_message_key: &str,
    error_text: &str,
) -> Result<()> {
    let prior_metadata = load_metadata_for_message(conn, pending_message_key)?;
    let mut metadata = prior_metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    metadata.insert("pending_send".to_string(), Value::Bool(false));
    metadata.insert(
        "transitioned_to".to_string(),
        Value::String("send_failed".to_string()),
    );
    metadata.insert(
        "provider_error".to_string(),
        Value::String(clip_error_text(error_text, 2000)),
    );
    let metadata_json = Value::Object(metadata).to_string();
    let now = now_iso_string();
    let updated = conn
        .execute(
            r#"
            UPDATE communication_messages
            SET status = 'send_failed',
                metadata_json = ?2,
                observed_at = ?3
            WHERE message_key = ?1
              AND status = 'draft_pending_send'
            "#,
            params![pending_message_key, metadata_json, now],
        )
        .context("failed to mark outbound body as send_failed")?;
    if updated == 0 {
        anyhow::bail!(
            "outbound durability row {} was not in draft_pending_send when send_failed-update was attempted",
            pending_message_key
        );
    }
    Ok(())
}

fn load_metadata_for_message(conn: &Connection, message_key: &str) -> Result<Value> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            params![message_key],
            |row| row.get(0),
        )
        .optional()
        .context("failed to load metadata_json for outbound durability row")?;
    match raw {
        Some(json) => Ok(serde_json::from_str::<Value>(&json).unwrap_or(Value::Null)),
        None => Ok(Value::Null),
    }
}

fn enforce_reviewed_founder_send_failed_core_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
    pending_message_key: &str,
    provider_error: &str,
) -> Result<()> {
    emit_reviewed_founder_send_failed_transition(
        conn,
        entity_id,
        approval_key,
        request,
        pending_message_key,
        provider_error,
    )
}

/// Emit the `Sending -> SendFailed` core transition after a provider
/// failure. RFC 0001 Phase 1: the kernel must witness every founder-send
/// failure, and the durable pending body row is bound into metadata.
fn emit_reviewed_founder_send_failed_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
    pending_message_key: &str,
    provider_error: &str,
) -> Result<()> {
    let body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let recipient_set_sha256 = founder_send_recipient_set_sha256(request);
    let mut metadata = BTreeMap::new();
    metadata.insert("protected_party".to_string(), "founder".to_string());
    metadata.insert("thread_key".to_string(), request.thread_key.clone());
    metadata.insert("subject".to_string(), request.subject.clone());
    metadata.insert("account_key".to_string(), request.account_key.clone());
    metadata.insert(
        "pending_message_key".to_string(),
        pending_message_key.to_string(),
    );
    metadata.insert(
        "provider_error".to_string(),
        clip_error_text(provider_error, 500),
    );

    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: entity_id.to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Sending,
            to_state: CoreState::SendFailed,
            event: CoreEvent::Fail,
            actor: "ctox-reviewed-founder-send".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some(approval_key.to_string()),
                approved_body_sha256: Some(body_sha256.clone()),
                outgoing_body_sha256: Some(body_sha256),
                approved_recipient_set_sha256: Some(recipient_set_sha256.clone()),
                outgoing_recipient_set_sha256: Some(recipient_set_sha256),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(())
}

fn clip_error_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut clipped: String = text.chars().take(max).collect();
        clipped.push_str("...");
        clipped
    }
}

fn founder_send_recipient_set_sha256(request: &ChannelSendRequest) -> String {
    let mut to = request
        .to
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut cc = request
        .cc
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut attachments = request
        .attachments
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    to.sort();
    cc.sort();
    attachments.sort();
    let payload = json!({
        "to": to,
        "cc": cc,
        "subject": request.subject.trim(),
        "attachments": attachments,
    })
    .to_string();
    sha256_hex(payload.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn test_channel(
    root: &Path,
    db_path: &Path,
    channel: &str,
    account_key: Option<&str>,
) -> Result<Value> {
    match channel {
        "tui" => Ok(json!({
            "ok": true,
            "channel": "tui",
            "status": "ready",
            "detail": "local TUI channel does not require external transport setup",
            "db_path": db_path,
        })),
        "email" => {
            bootstrap_channel_account(root, "email")?;
            let conn = open_channel_db(db_path)?;
            let resolved_account_key = resolve_account_key(&conn, "email", account_key)?;
            let account_config =
                load_account_config(&conn, &resolved_account_key)?.ok_or_else(|| {
                    anyhow::anyhow!("missing email account config for {}", resolved_account_key)
                })?;
            let adapter = communication_adapters::email();
            let resolved_email = email_address_from_account_key(&resolved_account_key);
            let adapter_json = adapter.test_cli(
                root,
                &communication_adapters::EmailTestCommandRequest {
                    db_path,
                    email_address: &resolved_email,
                    provider: &account_config.provider,
                    profile_json: &account_config.profile_json,
                },
            )?;
            Ok(json!({
                "ok": adapter_json.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "channel": "email",
                "account_key": resolved_account_key,
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        "jami" => {
            bootstrap_channel_account(root, "jami")?;
            let conn = open_channel_db(db_path)?;
            let resolved_account_key = resolve_account_key(&conn, "jami", account_key)?;
            let account_config =
                load_account_config(&conn, &resolved_account_key)?.ok_or_else(|| {
                    anyhow::anyhow!("missing jami account config for {}", resolved_account_key)
                })?;
            let adapter = communication_adapters::jami();
            let resolved_account_id = jami_address_from_account_key(&resolved_account_key);
            let adapter_json = adapter.test_cli(
                root,
                &communication_adapters::JamiTestCommandRequest {
                    db_path,
                    account_id: &resolved_account_id,
                    provider: &account_config.provider,
                    profile_json: &account_config.profile_json,
                },
            )?;
            Ok(json!({
                "ok": adapter_json.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "channel": "jami",
                "account_key": resolved_account_key,
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        "teams" => {
            bootstrap_channel_account(root, "teams")?;
            let conn = open_channel_db(db_path)?;
            let adapter = communication_adapters::teams();
            let resolved_account_key = resolve_account_key(&conn, "teams", account_key).ok();
            let account_config = resolved_account_key
                .as_deref()
                .and_then(|key| load_account_config(&conn, key).ok().flatten());
            let empty_profile = json!({});
            let resolved_tenant_id = account_config
                .as_ref()
                .and_then(|config| config.profile_json.get("tenantId"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let adapter_json = adapter.test_cli(
                root,
                &communication_adapters::TeamsTestCommandRequest {
                    db_path,
                    tenant_id: &resolved_tenant_id,
                    profile_json: account_config
                        .as_ref()
                        .map(|config| &config.profile_json)
                        .unwrap_or(&empty_profile),
                },
            )?;
            Ok(json!({
                "ok": adapter_json.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "channel": "teams",
                "account_key": resolved_account_key,
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        "whatsapp" => {
            let conn = open_channel_db(db_path)?;
            let resolved_account_key = resolve_account_key(&conn, "whatsapp", account_key).ok();
            let adapter = communication_adapters::whatsapp();
            let adapter_json = adapter.test_cli(
                root,
                &communication_adapters::WhatsappTestCommandRequest {
                    db_path,
                    account_key: resolved_account_key.as_deref().or(account_key),
                },
            )?;
            Ok(json!({
                "ok": adapter_json.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "channel": "whatsapp",
                "account_key": resolved_account_key,
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        other => anyhow::bail!("unsupported channel test target: {other}"),
    }
}

fn bootstrap_channel_account(root: &Path, channel: &str) -> Result<()> {
    match channel {
        "email" => {
            let settings = communication_gateway::runtime_settings_from_root(
                root,
                communication_gateway::CommunicationAdapterKind::Email,
            );
            if settings
                .get("CTO_EMAIL_ADDRESS")
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            {
                sync_prompt_identity(root, &settings)?;
            }
        }
        "jami" => {
            let mut settings = communication_gateway::runtime_settings_from_root(
                root,
                communication_gateway::CommunicationAdapterKind::Jami,
            );
            let configured_account_id = settings
                .get("CTO_JAMI_ACCOUNT_ID")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let configured_profile_name = settings
                .get("CTO_JAMI_PROFILE_NAME")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            if configured_account_id.is_some() || configured_profile_name.is_some() {
                let resolved = communication_adapters::jami().resolve_account(
                    root,
                    &communication_adapters::JamiResolveAccountCommandRequest {
                        account_id: configured_account_id.as_deref(),
                        profile_name: configured_profile_name.as_deref(),
                    },
                )?;
                if resolved.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    if let Some(account) =
                        resolved.get("resolvedAccount").and_then(Value::as_object)
                    {
                        if let Some(account_id) = account
                            .get("accountId")
                            .and_then(Value::as_str)
                            .filter(|v| !v.trim().is_empty())
                        {
                            settings
                                .insert("CTO_JAMI_ACCOUNT_ID".to_string(), account_id.to_string());
                        }
                        if let Some(profile_name) = account
                            .get("displayName")
                            .and_then(Value::as_str)
                            .filter(|v| !v.trim().is_empty())
                        {
                            settings.insert(
                                "CTO_JAMI_PROFILE_NAME".to_string(),
                                profile_name.to_string(),
                            );
                        }
                    }
                }
                sync_prompt_identity(root, &settings)?;
            }
        }
        "teams" => {}
        _ => {}
    }
    Ok(())
}

fn parse_send_request(args: &[String]) -> Result<ChannelSendRequest> {
    let channel = required_flag_value(args, "--channel")?.to_string();
    let account_key = required_flag_value(args, "--account-key")?.to_string();
    let thread_key = required_flag_value(args, "--thread-key")?.to_string();
    let body = required_flag_value(args, "--body")?.to_string();
    let subject = find_flag_value(args, "--subject")
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let to = collect_flag_values(args, "--to");
    // "tui", "teams", "meeting", and WhatsApp thread replies don't require ad hoc recipients here:
    // tui is local, teams targets the configured Graph chat/channel or the
    // stored Teams thread key, meeting broadcasts through the active
    // Playwright session, and WhatsApp replies target the chat encoded in
    // thread_key. Email and Jami still need explicit remote targets.
    let whatsapp_thread_reply = channel == "whatsapp" && thread_key.contains("::chat::");
    if !matches!(channel.as_str(), "tui" | "teams" | "meeting")
        && !whatsapp_thread_reply
        && to.is_empty()
    {
        anyhow::bail!("channel send for {channel} requires at least one --to value");
    }
    Ok(ChannelSendRequest {
        channel,
        account_key,
        thread_key,
        body,
        subject,
        to,
        cc: collect_flag_values(args, "--cc"),
        attachments: collect_flag_values(args, "--attach-file"),
        sender_display: find_flag_value(args, "--sender-display").map(ToOwned::to_owned),
        sender_address: find_flag_value(args, "--sender-address").map(ToOwned::to_owned),
        send_voice: has_flag(args, "--send-voice"),
        reviewed_founder_send: has_flag(args, "--reviewed-founder-send"),
    })
}

fn validate_founder_outbound_email(
    settings: &BTreeMap<String, String>,
    request: &ChannelSendRequest,
) -> Result<()> {
    if request.channel != "email" {
        return Ok(());
    }
    let protected_recipients = request
        .to
        .iter()
        .chain(request.cc.iter())
        .map(|email| classify_email_sender(settings, email))
        .filter(|policy| matches!(policy.role.as_str(), "owner" | "founder" | "admin"))
        .collect::<Vec<_>>();
    if protected_recipients.is_empty() {
        return Ok(());
    }
    let recipient_summary = protected_recipients
        .iter()
        .map(|policy| format!("{} ({})", policy.normalized_email, policy.role))
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::ensure!(
        request.reviewed_founder_send,
        "direct outbound email to founder/owner/admin recipients is blocked without review: {}. Use a reviewed founder-send path.",
        recipient_summary
    );
    // Body-content guidance for mandantengerechte mail lives in
    // `owner-communication/SKILL.md`. CTOX core does not scrape the body for
    // internal vocabulary — the agent owns the wording, not the harness.
    anyhow::bail!(
        "generic channel send is disabled for founder/owner/admin outbound email: {}. Use the dedicated reviewed founder communication path instead.",
        recipient_summary
    );
}

fn resolve_outbound_subject(
    conn: &Connection,
    mut request: ChannelSendRequest,
) -> Result<ChannelSendRequest> {
    let subject = request.subject.trim();
    if !subject_is_placeholder(subject) {
        return Ok(request);
    }
    if let Some(existing) = load_thread_subject(conn, &request.thread_key)? {
        request.subject = existing;
    }
    if request.channel == "email" && subject_is_placeholder(request.subject.trim()) {
        anyhow::bail!(
            "email send requires a real subject or an existing thread subject for {}",
            request.thread_key
        );
    }
    Ok(request)
}

fn thread_prefers_voice_reply(conn: &Connection, thread_key: &str) -> Result<bool> {
    let metadata_json = conn
        .query_row(
            r#"
            SELECT metadata_json
            FROM communication_messages
            WHERE thread_key = ?1
              AND direction = 'inbound'
            ORDER BY external_created_at DESC, observed_at DESC
            LIMIT 1
            "#,
            params![thread_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(metadata_json) = metadata_json else {
        return Ok(false);
    };
    let parsed = serde_json::from_str::<Value>(&metadata_json).unwrap_or_else(|_| Value::Null);
    Ok(parsed
        .get("preferredReplyModality")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("voice")))
}

fn load_thread_subject(conn: &Connection, thread_key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT subject FROM communication_threads WHERE thread_key = ?1 LIMIT 1",
            params![thread_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to load existing thread subject")?
        .filter(|subject| !subject_is_placeholder(subject.trim())))
}

fn subject_is_placeholder(subject: &str) -> bool {
    let normalized = subject.trim().to_ascii_lowercase();
    normalized.is_empty() || normalized == "(no subject)" || normalized == "(ohne betreff)"
}

fn parse_tui_ingest_request(args: &[String]) -> Result<TuiIngestRequest> {
    Ok(TuiIngestRequest {
        account_key: required_flag_value(args, "--account-key")?.to_string(),
        thread_key: required_flag_value(args, "--thread-key")?.to_string(),
        body: required_flag_value(args, "--body")?.to_string(),
        subject: find_flag_value(args, "--subject")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "TUI input".to_string()),
        sender_display: find_flag_value(args, "--sender-display")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "Local TUI".to_string()),
        sender_address: find_flag_value(args, "--sender-address")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "tui:local".to_string()),
        metadata: json!({
            "source": "ctox-channel-ingest-tui",
        }),
    })
}

pub(crate) fn open_channel_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create db parent {}", parent.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open channel db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for channels")?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout={busy_timeout_ms};

        CREATE TABLE IF NOT EXISTS communication_accounts (
            account_key TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            address TEXT NOT NULL,
            provider TEXT NOT NULL,
            profile_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_inbound_ok_at TEXT,
            last_outbound_ok_at TEXT
        );

        CREATE TABLE IF NOT EXISTS communication_threads (
            thread_key TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            account_key TEXT NOT NULL,
            subject TEXT NOT NULL,
            participant_keys_json TEXT NOT NULL,
            last_message_key TEXT NOT NULL,
            last_message_at TEXT NOT NULL,
            message_count INTEGER NOT NULL,
            unread_count INTEGER NOT NULL,
            metadata_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS communication_messages (
            message_key TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            account_key TEXT NOT NULL,
            thread_key TEXT NOT NULL,
            remote_id TEXT NOT NULL,
            direction TEXT NOT NULL,
            folder_hint TEXT NOT NULL,
            sender_display TEXT NOT NULL,
            sender_address TEXT NOT NULL,
            recipient_addresses_json TEXT NOT NULL,
            cc_addresses_json TEXT NOT NULL,
            bcc_addresses_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            preview TEXT NOT NULL,
            body_text TEXT NOT NULL,
            body_html TEXT NOT NULL,
            raw_payload_ref TEXT NOT NULL,
            trust_level TEXT NOT NULL,
            status TEXT NOT NULL,
            seen INTEGER NOT NULL,
            has_attachments INTEGER NOT NULL,
            external_created_at TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            metadata_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_communication_messages_account_time
            ON communication_messages(account_key, external_created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_communication_messages_thread
            ON communication_messages(thread_key, external_created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_communication_messages_channel_remote
            ON communication_messages(channel, account_key, remote_id);

        CREATE TABLE IF NOT EXISTS communication_sync_runs (
            run_key TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            account_key TEXT NOT NULL,
            folder_hint TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT NOT NULL,
            ok INTEGER NOT NULL,
            fetched_count INTEGER NOT NULL,
            stored_count INTEGER NOT NULL,
            error_text TEXT NOT NULL,
            metadata_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS communication_routing_state (
            message_key TEXT PRIMARY KEY,
            route_status TEXT NOT NULL,
            lease_owner TEXT,
            leased_at TEXT,
            acked_at TEXT,
            last_error TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_communication_routing_status_owner
            ON communication_routing_state(route_status, lease_owner, leased_at, updated_at);

        CREATE TABLE IF NOT EXISTS communication_founder_reply_reviews (
            approval_key TEXT PRIMARY KEY,
            inbound_message_key TEXT NOT NULL,
            action_digest TEXT NOT NULL,
            action_json TEXT NOT NULL,
            body_sha256 TEXT NOT NULL,
            reviewer TEXT NOT NULL,
            review_summary TEXT NOT NULL,
            approved_at TEXT NOT NULL,
            sent_at TEXT,
            send_result_json TEXT NOT NULL DEFAULT '{{}}',
            terminal_no_send INTEGER NOT NULL DEFAULT 0,
            UNIQUE(inbound_message_key, action_digest)
        );

        CREATE INDEX IF NOT EXISTS idx_founder_reply_reviews_inbound
            ON communication_founder_reply_reviews(inbound_message_key, sent_at);

        CREATE TABLE IF NOT EXISTS owner_profiles (
            owner_key TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    ))
    .context("failed to ensure channel schema")?;
    ensure_terminal_no_send_column(conn)?;
    ensure_routing_rows_for_inbound(conn)?;
    Ok(())
}

/// Add the `terminal_no_send` column to
/// `communication_founder_reply_reviews` on existing databases that
/// were created before the NO-SEND verdict was a structured field.
/// New databases pick the column up from the CREATE TABLE statement
/// in this same migration block. This is idempotent: we probe via
/// `pragma_table_info` and only ALTER when the column is missing.
fn ensure_terminal_no_send_column(conn: &Connection) -> Result<()> {
    let column_exists: bool = conn
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM pragma_table_info('communication_founder_reply_reviews')
                WHERE name = 'terminal_no_send'
            )
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .unwrap_or(false);
    if !column_exists {
        conn.execute(
            "ALTER TABLE communication_founder_reply_reviews ADD COLUMN terminal_no_send INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .context("failed to add terminal_no_send column to communication_founder_reply_reviews")?;
    }
    Ok(())
}

pub(crate) fn ensure_routing_rows_for_inbound(conn: &Connection) -> Result<()> {
    // Historical auto-handle rule: inbound messages whose external timestamp
    // predates the communication account's creation are marked as already
    // handled so we don't re-process mailbox history at first boot. The
    // synthetic `queue` and `tui` channels are programmatic — work items are
    // created after the account exists and must stay `pending` until leased —
    // so they are excluded from the pre-account auto-handle.
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        SELECT
            m.message_key,
            CASE
                WHEN m.direction = 'outbound' THEN 'handled'
                WHEN m.trust_level = 'system_probe' THEN 'handled'
                WHEN m.channel IN ('queue', 'tui') THEN 'pending'
                WHEN m.channel = 'teams'
                     AND m.direction = 'inbound'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at
                     AND datetime(m.external_created_at) < datetime('now', '-24 hours') THEN 'handled'
                WHEN m.direction = 'inbound'
                     AND m.channel <> 'teams'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at THEN 'handled'
                ELSE 'pending'
            END,
            NULL,
            NULL,
            CASE
                WHEN m.direction = 'outbound' OR m.trust_level = 'system_probe' THEN m.observed_at
                WHEN m.channel IN ('queue', 'tui') THEN NULL
                WHEN m.channel = 'teams'
                     AND m.direction = 'inbound'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at
                     AND datetime(m.external_created_at) < datetime('now', '-24 hours') THEN m.observed_at
                WHEN m.direction = 'inbound'
                     AND m.channel <> 'teams'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at THEN m.observed_at
                ELSE NULL
            END,
            NULL,
            m.observed_at
        FROM communication_messages m
        LEFT JOIN communication_accounts a ON a.account_key = m.account_key
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE r.message_key IS NULL
        "#,
        [],
    )
    .context("failed to backfill communication routing state")?;
    conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status = 'handled',
            lease_owner = NULL,
            leased_at = NULL,
            acked_at = COALESCE(acked_at, (
                SELECT observed_at
                FROM communication_messages m
                WHERE m.message_key = communication_routing_state.message_key
            )),
            last_error = NULL,
            updated_at = COALESCE((
                SELECT observed_at
                FROM communication_messages m
                WHERE m.message_key = communication_routing_state.message_key
            ), updated_at)
        WHERE message_key IN (
            SELECT message_key
            FROM communication_messages
            WHERE direction = 'inbound'
              AND trust_level = 'system_probe'
        )
          AND route_status <> 'handled'
        "#,
        [],
    )
    .context("failed to normalize routing for system probe messages")?;
    Ok(())
}

fn schema_state(conn: &Connection) -> Result<Value> {
    let inbound_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE direction = 'inbound'",
        [],
        |row| row.get(0),
    )?;
    let thread_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM communication_threads", [], |row| {
            row.get(0)
        })?;
    Ok(json!({
        "inbound_messages": inbound_count,
        "threads": thread_count,
    }))
}

fn ingest_tui_message(
    root: &Path,
    conn: &mut Connection,
    mut request: TuiIngestRequest,
) -> Result<Value> {
    let sanitized = secrets::auto_intake_prompt_secrets(root, &request.body)
        .context("failed to sanitize TUI secret-bearing input")?;
    if sanitized.auto_ingested_secrets > 0 {
        request.body = sanitized.sanitized_prompt;
        request.metadata = json!({
            "source": "ctox-channel-ingest-tui",
            "secret_sanitized": true,
            "auto_ingested_secrets": sanitized.auto_ingested_secrets,
            "suggested_skill": "secret-hygiene",
        });
    }
    ensure_account(
        conn,
        &request.account_key,
        "tui",
        &request.sender_address,
        "local",
        json!({"source": "tui"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!(
        "tui-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.thread_key, request.sender_address, request.body
        ))
    );
    let message_key = format!("{}::{remote_id}", request.account_key);
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "tui",
            account_key: &request.account_key,
            thread_key: &request.thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "tui",
            sender_display: &request.sender_display,
            sender_address: &request.sender_address,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &request.subject,
            preview: &preview_text(&request.body, &request.subject),
            body_text: &request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "medium",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&request.metadata)?,
        },
    )?;
    refresh_thread(conn, &request.thread_key)?;
    ensure_routing_rows_for_inbound(conn)?;
    Ok(json!({
        "message_key": message_key,
        "thread_key": request.thread_key,
        "channel": "tui",
    }))
}

fn store_tui_outbound_message(
    conn: &mut Connection,
    request: &ChannelSendRequest,
) -> Result<String> {
    ensure_account(
        conn,
        &request.account_key,
        "tui",
        request.sender_address.as_deref().unwrap_or("tui:local"),
        "local",
        json!({"source": "tui"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!(
        "tui-out-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.thread_key, request.account_key, observed_at
        ))
    );
    let message_key = format!("{}::{remote_id}", request.account_key);
    let sender_display = request
        .sender_display
        .clone()
        .unwrap_or_else(|| "Local TUI".to_string());
    let sender_address = request
        .sender_address
        .clone()
        .unwrap_or_else(|| "tui:local".to_string());
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "tui",
            account_key: &request.account_key,
            thread_key: &request.thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "tui",
            sender_display: &sender_display,
            sender_address: &sender_address,
            recipient_addresses_json: &serde_json::to_string(&request.to)?,
            cc_addresses_json: &serde_json::to_string(&request.cc)?,
            bcc_addresses_json: "[]",
            subject: &request.subject,
            preview: &preview_text(&request.body, &request.subject),
            body_text: &request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "sent",
            seen: true,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: r#"{"source":"ctox-tui-send"}"#,
        },
    )?;
    refresh_thread(conn, &request.thread_key)?;
    ensure_routing_rows_for_inbound(conn)?;
    Ok(message_key)
}

fn take_messages(
    conn: &mut Connection,
    channel: Option<&str>,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<ChannelMessageView>> {
    let sql = if channel.is_some() {
        r#"
        WITH eligible AS (
            SELECT
                m.message_key,
                m.channel,
                m.account_key,
                m.thread_key,
                m.remote_id,
                m.direction,
                m.folder_hint,
                m.sender_display,
                m.sender_address,
                m.subject,
                m.preview,
                m.body_text,
                m.status,
                m.seen,
                m.external_created_at,
                m.observed_at,
                m.metadata_json,
                r.route_status,
                r.lease_owner,
                r.leased_at,
                r.acked_at,
                r.updated_at,
                ROW_NUMBER() OVER (
                    PARTITION BY m.thread_key
                    ORDER BY
                        CASE
                            WHEN r.route_status = 'pending' THEN 0
                            WHEN r.route_status = 'leased' THEN 1
                            ELSE 2
                        END ASC,
                        m.external_created_at DESC,
                        m.observed_at DESC,
                        m.message_key DESC
                ) AS thread_rank
            FROM communication_messages m
            JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.direction = 'inbound'
              AND m.channel = ?1
              AND r.route_status IN ('pending', 'leased')
              AND (
                    json_extract(m.metadata_json, '$.not_before') IS NULL
                 OR json_extract(m.metadata_json, '$.not_before') = ''
                 OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
              )
              AND (
                    r.route_status = 'pending'
                    OR r.lease_owner IS NULL
                    OR r.lease_owner = ''
                    OR r.lease_owner = ?2
              )
        )
        SELECT
            message_key,
            channel,
            account_key,
            thread_key,
            remote_id,
            direction,
            folder_hint,
            sender_display,
            sender_address,
            subject,
            preview,
            body_text,
            status,
            seen,
            external_created_at,
            observed_at,
            metadata_json,
            route_status,
            lease_owner,
            leased_at,
            acked_at,
            updated_at
        FROM eligible
        WHERE thread_rank = 1
        ORDER BY external_created_at DESC, observed_at DESC, message_key DESC
        LIMIT ?3
        "#
    } else {
        r#"
        WITH eligible AS (
            SELECT
                m.message_key,
                m.channel,
                m.account_key,
                m.thread_key,
                m.remote_id,
                m.direction,
                m.folder_hint,
                m.sender_display,
                m.sender_address,
                m.subject,
                m.preview,
                m.body_text,
                m.status,
                m.seen,
                m.external_created_at,
                m.observed_at,
                m.metadata_json,
                r.route_status,
                r.lease_owner,
                r.leased_at,
                r.acked_at,
                r.updated_at,
                ROW_NUMBER() OVER (
                    PARTITION BY m.thread_key
                    ORDER BY
                        CASE
                            WHEN r.route_status = 'pending' THEN 0
                            WHEN r.route_status = 'leased' THEN 1
                            ELSE 2
                        END ASC,
                        m.external_created_at DESC,
                        m.observed_at DESC,
                        m.message_key DESC
                ) AS thread_rank
            FROM communication_messages m
            JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.direction = 'inbound'
              AND r.route_status IN ('pending', 'leased')
              AND (
                    json_extract(m.metadata_json, '$.not_before') IS NULL
                 OR json_extract(m.metadata_json, '$.not_before') = ''
                 OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
              )
              AND (
                    r.route_status = 'pending'
                    OR r.lease_owner IS NULL
                    OR r.lease_owner = ''
                    OR r.lease_owner = ?1
              )
        )
        SELECT
            message_key,
            channel,
            account_key,
            thread_key,
            remote_id,
            direction,
            folder_hint,
            sender_display,
            sender_address,
            subject,
            preview,
            body_text,
            status,
            seen,
            external_created_at,
            observed_at,
            metadata_json,
            route_status,
            lease_owner,
            leased_at,
            acked_at,
            updated_at
        FROM eligible
        WHERE thread_rank = 1
        ORDER BY external_created_at DESC, observed_at DESC, message_key DESC
        LIMIT ?2
        "#
    };

    let mut statement = conn.prepare(sql)?;
    let mapped = if let Some(channel) = channel {
        statement.query_map(
            params![channel, lease_owner, limit as i64],
            map_channel_message_row,
        )?
    } else {
        statement.query_map(params![lease_owner, limit as i64], map_channel_message_row)?
    };
    let rows = mapped.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let tx = conn.unchecked_transaction()?;
    let leased_at = now_iso_string();
    let mut taken = Vec::new();
    for mut item in rows {
        tx.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            VALUES (?1, 'leased', ?2, ?3, NULL, NULL, ?3)
            ON CONFLICT(message_key) DO UPDATE SET
                route_status='leased',
                lease_owner=excluded.lease_owner,
                leased_at=excluded.leased_at,
                acked_at=NULL,
                updated_at=excluded.updated_at
            "#,
            params![item.message_key, lease_owner, leased_at],
        )?;
        item.routing.route_status = "leased".to_string();
        item.routing.lease_owner = Some(lease_owner.to_string());
        item.routing.leased_at = Some(leased_at.clone());
        item.routing.updated_at = leased_at.clone();
        taken.push(item);
    }
    tx.commit()?;
    Ok(taken)
}

pub fn defer_messages_until(
    root: &Path,
    message_keys: &[String],
    not_before: &str,
    reason: &str,
) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut updated = 0usize;
    for message_key in message_keys {
        updated += conn.execute(
            r#"
            UPDATE communication_messages
            SET metadata_json = json_set(
                json_set(metadata_json, '$.not_before', ?2),
                '$.defer_reason',
                ?3
            )
            WHERE message_key = ?1
            "#,
            params![message_key, not_before, reason],
        )?;
    }
    Ok(updated)
}

fn ack_messages(conn: &mut Connection, message_keys: &[String], status: &str) -> Result<usize> {
    let now = now_iso_string();
    let acked_at = if matches!(status, "handled" | "cancelled") {
        Some(now.as_str())
    } else {
        None
    };
    let tx = conn.unchecked_transaction()?;
    let mut updated = 0usize;
    for message_key in message_keys {
        let routing_updates = tx.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            SELECT ?1, ?2, NULL, NULL, ?3, NULL, ?4
            FROM communication_messages
            WHERE message_key = ?1
            ON CONFLICT(message_key) DO UPDATE SET
                route_status=excluded.route_status,
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=excluded.acked_at,
                updated_at=excluded.updated_at
            "#,
            params![message_key, status, acked_at, now],
        )?;
        if routing_updates == 0 {
            continue;
        }
        updated += routing_updates;
        tx.execute(
            "UPDATE communication_messages SET seen = 1 WHERE message_key = ?1",
            params![message_key],
        )?;
    }
    tx.commit()?;
    Ok(updated)
}

fn list_messages(
    conn: &Connection,
    channel: Option<&str>,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let sql = if channel.is_some() {
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#
    } else {
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#
    };
    let mut statement = conn.prepare(sql)?;
    let rows = if let Some(channel) = channel {
        statement.query_map(params![channel, limit as i64], map_channel_message_row)?
    } else {
        statement.query_map(params![limit as i64], map_channel_message_row)?
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_thread_messages(
    conn: &Connection,
    thread_key: &str,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.thread_key = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![thread_key, limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_messages(
    conn: &Connection,
    query: &str,
    channel: Option<&str>,
    sender: Option<&str>,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let normalized_query = format!("%{}%", search_query_seed(query));
    let normalized_sender = sender
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE
            (?1 IS NULL OR m.channel = ?1)
            AND (?2 IS NULL OR LOWER(m.sender_address) = ?2)
            AND (
                LOWER(m.subject) LIKE ?3
                OR LOWER(m.preview) LIKE ?3
                OR LOWER(m.body_text) LIKE ?3
                OR LOWER(m.sender_display) LIKE ?3
                OR LOWER(m.sender_address) LIKE ?3
                OR LOWER(m.thread_key) LIKE ?3
            )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?4
        "#,
    )?;
    let rows = statement.query_map(
        params![channel, normalized_sender, normalized_query, limit as i64],
        map_channel_message_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_query_seed(query: &str) -> String {
    let compact = query.trim().to_ascii_lowercase();
    compact
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .find(|token| token.len() >= 3)
        .unwrap_or(compact.as_str())
        .to_string()
}

fn build_communication_context(
    conn: &Connection,
    thread_key: &str,
    query: Option<&str>,
    sender: Option<&str>,
    limit: usize,
) -> Result<CommunicationContextView> {
    let thread_messages = list_thread_messages(conn, thread_key, limit)?;
    let latest_subject = thread_messages
        .iter()
        .find(|item| !item.subject.trim().is_empty())
        .map(|item| item.subject.clone());
    let mut related_messages = Vec::new();
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let mut seen = std::collections::BTreeSet::new();
        for message in search_messages(conn, query, None, None, limit)?
            .into_iter()
            .chain(
                sender
                    .map(|value| search_messages(conn, query, None, Some(value), limit))
                    .transpose()?
                    .unwrap_or_default()
                    .into_iter(),
            )
        {
            if seen.insert(message.message_key.clone()) {
                related_messages.push(message);
            }
        }
    }
    related_messages.retain(|item| item.thread_key != thread_key);
    let latest_inbound = thread_messages
        .iter()
        .find(|item| item.direction == "inbound")
        .map(|item| candidate_from_message("latest_inbound", item));
    let latest_outbound = thread_messages
        .iter()
        .find(|item| item.direction == "outbound")
        .map(|item| candidate_from_message("latest_outbound", item));
    let mut candidate_blockers = collect_candidates(&thread_messages, &related_messages, "blocker");
    let mut candidate_promises = collect_candidates(&thread_messages, &related_messages, "promise");
    let open_owner_questions = collect_open_owner_questions(&thread_messages);
    candidate_blockers.truncate(limit.min(8));
    candidate_promises.truncate(limit.min(8));
    Ok(CommunicationContextView {
        thread_key: thread_key.to_string(),
        latest_subject,
        latest_inbound,
        latest_outbound,
        thread_messages,
        related_messages,
        candidate_blockers,
        candidate_promises,
        open_owner_questions,
    })
}

fn collect_candidates(
    thread_messages: &[ChannelMessageView],
    related_messages: &[ChannelMessageView],
    candidate_kind: &str,
) -> Vec<CommunicationStateCandidate> {
    let mut out = Vec::new();
    for message in thread_messages.iter().chain(related_messages.iter()) {
        if message.direction != "outbound" {
            continue;
        }
        let body = format!(
            "{}\n{}\n{}",
            message.subject.to_ascii_lowercase(),
            message.preview.to_ascii_lowercase(),
            message.body_text.to_ascii_lowercase()
        );
        let is_match = match candidate_kind {
            "blocker" => {
                body.contains("blocked")
                    || body.contains("blocker")
                    || body.contains("need ")
                    || body.contains("missing ")
                    || body.contains("requires ")
                    || body.contains("cannot ")
            }
            "promise" => {
                body.contains("next step")
                    || body.contains("i will")
                    || body.contains("i'll")
                    || body.contains("follow-up")
                    || body.contains("queued")
                    || body.contains("review")
                    || body.contains("continue")
            }
            _ => false,
        };
        if is_match {
            out.push(candidate_from_message(candidate_kind, message));
        }
    }
    out
}

fn collect_open_owner_questions(
    thread_messages: &[ChannelMessageView],
) -> Vec<CommunicationStateCandidate> {
    let latest_outbound_at = thread_messages
        .iter()
        .find(|item| item.direction == "outbound")
        .map(|item| item.external_created_at.clone());
    thread_messages
        .iter()
        .filter(|item| item.direction == "inbound")
        .filter(|item| {
            let text = format!("{}\n{}", item.subject, item.body_text);
            text.contains('?')
                || text.to_ascii_lowercase().contains("please")
                || text.to_ascii_lowercase().contains("can you")
        })
        .filter(|item| {
            latest_outbound_at
                .as_ref()
                .map(|outbound| item.external_created_at >= *outbound)
                .unwrap_or(true)
        })
        .map(|item| candidate_from_message("open_question", item))
        .collect()
}

fn candidate_from_message(kind: &str, message: &ChannelMessageView) -> CommunicationStateCandidate {
    CommunicationStateCandidate {
        kind: kind.to_string(),
        message_key: message.message_key.clone(),
        channel: message.channel.clone(),
        thread_key: message.thread_key.clone(),
        created_at: message.external_created_at.clone(),
        summary: preview_text(&message.body_text, &message.subject),
    }
}

fn map_channel_message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelMessageView> {
    let metadata_json: String = row.get(16)?;
    let metadata = serde_json::from_str(&metadata_json)
        .unwrap_or_else(|_| json!({"raw_metadata": metadata_json}));
    Ok(ChannelMessageView {
        message_key: row.get(0)?,
        channel: row.get(1)?,
        account_key: row.get(2)?,
        thread_key: row.get(3)?,
        remote_id: row.get(4)?,
        direction: row.get(5)?,
        folder_hint: row.get(6)?,
        sender_display: row.get(7)?,
        sender_address: row.get(8)?,
        subject: row.get(9)?,
        preview: row.get(10)?,
        body_text: row.get(11)?,
        status: row.get(12)?,
        seen: row.get::<_, i64>(13)? != 0,
        external_created_at: row.get(14)?,
        observed_at: row.get(15)?,
        metadata,
        routing: RoutingView {
            route_status: row.get(17)?,
            lease_owner: row.get(18)?,
            leased_at: row.get(19)?,
            acked_at: row.get(20)?,
            updated_at: row.get(21)?,
        },
    })
}

fn routed_inbound_message_from_view(item: ChannelMessageView) -> RoutedInboundMessage {
    let preferred_reply_modality = item
        .metadata
        .get("preferredReplyModality")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let workspace_root =
        workspace_root_from_queue_metadata_or_prompt(&item.metadata, &item.body_text);
    RoutedInboundMessage {
        message_key: item.message_key,
        channel: item.channel,
        account_key: item.account_key,
        thread_key: item.thread_key,
        sender_display: item.sender_display,
        sender_address: item.sender_address,
        subject: item.subject,
        preview: item.preview,
        body_text: item.body_text,
        external_created_at: item.external_created_at,
        workspace_root,
        metadata: item.metadata,
        preferred_reply_modality,
    }
}

fn list_queue_tasks_from_conn(conn: &Connection, limit: usize) -> Result<Vec<QueueTaskView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'blocked' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                WHEN 'cancelled' THEN 5
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![QUEUE_CHANNEL_NAME, limit as i64],
        map_channel_message_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?
        .into_iter()
        .map(queue_task_from_message)
        .collect()
}

fn list_queue_tasks_from_conn_with_statuses(
    conn: &Connection,
    statuses: &[String],
    limit: usize,
) -> Result<Vec<QueueTaskView>> {
    let placeholders = (0..statuses.len())
        .map(|index| format!("?{}", index + 3))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND lower(COALESCE(r.route_status, 'pending')) IN ({placeholders})
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'blocked' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                WHEN 'cancelled' THEN 5
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT ?2
        "#
    );
    let mut values = Vec::with_capacity(statuses.len() + 2);
    values.push(SqlValue::Text(QUEUE_CHANNEL_NAME.to_string()));
    values.push(SqlValue::Integer(limit as i64));
    values.extend(statuses.iter().cloned().map(SqlValue::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?
        .into_iter()
        .map(queue_task_from_message)
        .collect()
}

fn load_queue_message_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<ChannelMessageView>> {
    conn.query_row(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND m.message_key = ?2
        LIMIT 1
        "#,
        params![QUEUE_CHANNEL_NAME, message_key],
        map_channel_message_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_queue_task_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<QueueTaskView>> {
    load_queue_message_from_conn(conn, message_key)?
        .map(queue_task_from_message)
        .transpose()
}

fn queue_task_from_message(message: ChannelMessageView) -> Result<QueueTaskView> {
    if message.channel != QUEUE_CHANNEL_NAME || message.direction != "inbound" {
        anyhow::bail!("message is not a queue task");
    }
    let priority = current_queue_priority(&message);
    let created_at = message
        .metadata
        .get("created_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message.observed_at.clone());
    let sort_at = message
        .metadata
        .get("sort_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message.external_created_at.clone());
    let prompt = message.body_text;
    let workspace_root = workspace_root_from_queue_metadata_or_prompt(&message.metadata, &prompt);
    Ok(QueueTaskView {
        message_key: message.message_key,
        thread_key: message.thread_key,
        title: message.subject,
        prompt,
        workspace_root,
        ticket_self_work_id: message
            .metadata
            .get("ticket_self_work_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        priority,
        suggested_skill: message
            .metadata
            .get("skill")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        parent_message_key: message
            .metadata
            .get("parent_message_key")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        route_status: message.routing.route_status,
        status_note: message
            .metadata
            .get("status_note")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        lease_owner: message.routing.lease_owner,
        leased_at: message.routing.leased_at,
        acked_at: message.routing.acked_at,
        created_at,
        sort_at,
        updated_at: message.routing.updated_at,
    })
}

fn ensure_queue_account(conn: &mut Connection) -> Result<()> {
    ensure_account(
        conn,
        QUEUE_ACCOUNT_KEY,
        QUEUE_CHANNEL_NAME,
        QUEUE_ACCOUNT_ADDRESS,
        QUEUE_PROVIDER,
        json!({"source": "ctox-queue"}),
    )
}

fn set_routing_status(
    conn: &mut Connection,
    message_key: &str,
    route_status: &str,
    now: &str,
) -> Result<()> {
    let route_status = canonical_queue_route_status(route_status)?;
    let acked_at = if matches!(route_status.as_str(), "handled" | "cancelled") {
        Some(now)
    } else {
        None
    };
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        VALUES (?1, ?2, NULL, NULL, ?3, NULL, ?4)
        ON CONFLICT(message_key) DO UPDATE SET
            route_status=excluded.route_status,
            lease_owner=NULL,
            leased_at=NULL,
            acked_at=excluded.acked_at,
            updated_at=excluded.updated_at
        "#,
        params![message_key, route_status, acked_at, now],
    )?;
    Ok(())
}

fn canonical_queue_priority(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_lowercase();
    match normalized.as_str() {
        "urgent" | "high" | "normal" | "low" => Ok(normalized),
        _ => anyhow::bail!("unsupported queue priority '{raw}' (expected urgent|high|normal|low)"),
    }
}

fn canonical_queue_route_status(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_lowercase();
    match normalized.as_str() {
        "pending" | "blocked" | "failed" | "handled" | "cancelled" => Ok(normalized),
        _ => anyhow::bail!(
            "unsupported queue route status '{raw}' (expected pending|blocked|failed|handled|cancelled)"
        ),
    }
}

fn current_queue_priority(message: &ChannelMessageView) -> String {
    message
        .metadata
        .get("priority")
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .trim()
        .to_lowercase()
}

fn queue_metadata_object(metadata: &Value) -> serde_json::Map<String, Value> {
    metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new)
}

fn normalize_workspace_root(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn workspace_root_from_queue_metadata_or_prompt(
    metadata: &Value,
    prompt: &str,
) -> Option<String> {
    queue_metadata_object(metadata)
        .get("workspace_root")
        .and_then(Value::as_str)
        .and_then(|value| normalize_workspace_root(Some(value)))
        .or_else(|| legacy_workspace_root_from_prompt(prompt))
}

pub fn legacy_workspace_root_from_prompt(prompt: &str) -> Option<String> {
    for marker in [
        "Work only inside this workspace:",
        "Arbeite ausschließlich im Verzeichnis ",
        "Arbeite im Verzeichnis ",
        "Arbeite ausschließlich im Workspace ",
        "Arbeite im Workspace ",
    ] {
        if let Some(path) = extract_workspace_root_after_marker(prompt, marker) {
            return Some(path);
        }
    }
    None
}

fn extract_workspace_root_after_marker(prompt: &str, marker: &str) -> Option<String> {
    let start = prompt.find(marker)? + marker.len();
    let tail = prompt[start..].trim_start();
    let line = tail.lines().next()?.trim();
    let candidate = if let Some(stripped) = line.strip_prefix('/') {
        format!("/{stripped}")
    } else if let Some(index) = line.find('/') {
        line[index..].to_string()
    } else {
        return None;
    };
    let trimmed = candidate
        .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '"' | '\''));
    normalize_workspace_root(Some(trimmed))
}

fn queue_sort_at(priority: &str, now: &str) -> Result<String> {
    let base = DateTime::parse_from_rfc3339(now)
        .with_context(|| format!("failed to parse queue timestamp '{now}'"))?
        .with_timezone(&Utc);
    let shifted = match priority {
        "urgent" => base - Duration::hours(24),
        "high" => base - Duration::hours(1),
        "normal" => base,
        "low" => base + Duration::hours(1),
        _ => anyhow::bail!("unsupported queue priority '{priority}'"),
    };
    Ok(shifted.to_rfc3339())
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

pub(crate) struct CommunicationSyncRun<'a> {
    pub run_key: &'a str,
    pub channel: &'a str,
    pub account_key: &'a str,
    pub folder_hint: &'a str,
    pub started_at: &'a str,
    pub finished_at: &'a str,
    pub ok: bool,
    pub fetched_count: i64,
    pub stored_count: i64,
    pub error_text: &'a str,
    pub metadata_json: &'a str,
}

pub(crate) fn upsert_communication_message(
    conn: &mut Connection,
    message: UpsertMessage<'_>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    upsert_communication_message_tx(&tx, message)?;
    tx.commit()?;
    Ok(())
}

fn upsert_communication_message_tx(tx: &Transaction<'_>, message: UpsertMessage<'_>) -> Result<()> {
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

pub(crate) fn ensure_account(
    conn: &mut Connection,
    account_key: &str,
    channel: &str,
    address: &str,
    provider: &str,
    profile_json: Value,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    ensure_account_tx(&tx, account_key, channel, address, provider, profile_json)?;
    tx.commit()?;
    Ok(())
}

fn upsert_owner_profile(conn: &mut Connection, display_name: &str) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO owner_profiles (
            owner_key, display_name, metadata_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(owner_key) DO UPDATE SET
            display_name=excluded.display_name,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params!["primary", display_name.trim(), r#"{}"#, now],
    )?;
    Ok(())
}

fn sync_identity_profiles(
    conn: &mut Connection,
    settings: &BTreeMap<String, String>,
) -> Result<()> {
    let owner_name = settings
        .get("CTOX_OWNER_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Owner");
    if let Some(owner_email) = settings
        .get("CTOX_OWNER_EMAIL_ADDRESS")
        .map(|value| normalize_email_address(value))
        .filter(|value| !value.is_empty())
    {
        upsert_identity_profile(
            conn,
            &owner_email,
            owner_name,
            json!({
                "email": owner_email,
                "role": "owner",
                "allow_admin_actions": true,
                "allow_sudo_actions": true,
                "mail_instruction_scope": "full_admin",
            }),
        )?;
    }

    let founder_roles = parse_founder_email_roles(settings);
    for founder_email in parse_founder_email_addresses(settings) {
        let founder_role = founder_roles
            .get(&founder_email)
            .cloned()
            .unwrap_or_else(|| "Founder".to_string());
        upsert_identity_profile(
            conn,
            &founder_email,
            &founder_email,
            json!({
                "email": founder_email,
                "role": "founder",
                "role_title": founder_role,
                "allow_admin_actions": true,
                "allow_sudo_actions": false,
                "mail_instruction_scope": "founder_strategic",
            }),
        )?;
    }

    for admin in parse_admin_email_policies(settings) {
        upsert_identity_profile(
            conn,
            &admin.email,
            &admin.email,
            json!({
                "email": admin.email,
                "role": "admin",
                "allow_admin_actions": true,
                "allow_sudo_actions": admin.can_sudo,
                "mail_instruction_scope": if admin.can_sudo { "admin_with_sudo" } else { "admin_without_sudo" },
            }),
        )?;
    }
    Ok(())
}

fn upsert_identity_profile(
    conn: &mut Connection,
    owner_key: &str,
    display_name: &str,
    metadata: Value,
) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO owner_profiles (
            owner_key, display_name, metadata_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(owner_key) DO UPDATE SET
            display_name=excluded.display_name,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            owner_key,
            display_name.trim(),
            serde_json::to_string(&metadata)?,
            now
        ],
    )?;
    Ok(())
}

fn load_owner_name(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            r#"
        SELECT display_name
        FROM owner_profiles
        WHERE owner_key = 'primary'
        LIMIT 1
        "#,
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .filter(|name| !name.trim().is_empty()))
}

fn admin_email_policy_summaries(settings: &BTreeMap<String, String>) -> Vec<String> {
    let admins = parse_admin_email_policies(settings);
    if admins.is_empty() {
        return vec!["- no additional admin mail profiles configured".to_string()];
    }
    admins
        .into_iter()
        .map(|entry| {
            format!(
                "- {} ({})",
                entry.email,
                if entry.can_sudo {
                    "admin with sudo"
                } else {
                    "admin without sudo"
                }
            )
        })
        .collect()
}

fn parse_founder_email_addresses(settings: &BTreeMap<String, String>) -> Vec<String> {
    let raw = settings
        .get("CTOX_FOUNDER_EMAIL_ADDRESSES")
        .map(String::as_str)
        .unwrap_or("");
    let mut seen = BTreeSet::new();
    raw.split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_email_address)
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn parse_founder_email_roles(settings: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let raw = settings
        .get("CTOX_FOUNDER_EMAIL_ROLES")
        .map(String::as_str)
        .unwrap_or("");
    let mut roles = BTreeMap::new();
    for entry in raw
        .split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let separator_index = entry.find(['|', ':', '=']);
        let Some(index) = separator_index else {
            continue;
        };
        let email = normalize_email_address(entry[..index].trim());
        let role = entry[index + 1..].trim();
        if email.is_empty() || role.is_empty() {
            continue;
        }
        roles.insert(email, role.to_string());
    }
    roles
}

fn founder_email_role_summaries(settings: &BTreeMap<String, String>) -> Vec<String> {
    let roles = parse_founder_email_roles(settings);
    parse_founder_email_addresses(settings)
        .into_iter()
        .map(|email| {
            let role = roles
                .get(&email)
                .cloned()
                .unwrap_or_else(|| "Founder".to_string());
            format!("{email} ({role})")
        })
        .collect()
}

fn parse_admin_email_policies(settings: &BTreeMap<String, String>) -> Vec<AdminEmailPolicy> {
    let raw = settings
        .get("CTOX_EMAIL_ADMIN_POLICIES")
        .map(String::as_str)
        .unwrap_or("");
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for entry in raw
        .split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let separator_index = entry.find(['|', ':', '=']);
        let (email_part, policy_part) = if let Some(index) = separator_index {
            (entry[..index].trim(), entry[index + 1..].trim())
        } else {
            (entry, "")
        };
        let email = normalize_email_address(email_part);
        if email.is_empty() || !seen.insert(email.clone()) {
            continue;
        }
        let policy = policy_part.to_ascii_lowercase().replace(' ', "");
        let can_sudo = policy == "sudo"
            || policy == "admin+sudo"
            || policy == "withsudo"
            || (policy.contains("sudo")
                && !policy.contains("no-sudo")
                && !policy.contains("nosudo")
                && !policy.contains("withoutsudo"));
        out.push(AdminEmailPolicy { email, can_sudo });
    }
    out
}

fn normalize_email_address(value: &str) -> String {
    value
        .trim()
        .trim_matches('<')
        .trim_matches('>')
        .to_lowercase()
}

fn normalized_allowed_email_domain(settings: &BTreeMap<String, String>) -> Option<String> {
    settings
        .get("CTOX_ALLOWED_EMAIL_DOMAIN")
        .map(|value| value.trim().trim_start_matches('@').to_lowercase())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            settings
                .get("CTOX_OWNER_EMAIL_ADDRESS")
                .map(|value| normalize_email_address(value))
                .and_then(|value| value.split_once('@').map(|(_, domain)| domain.to_string()))
                .filter(|value| !value.is_empty())
        })
}

fn email_matches_domain(email: &str, domain: &str) -> bool {
    email
        .rsplit_once('@')
        .map(|(_, candidate_domain)| candidate_domain.eq_ignore_ascii_case(domain))
        .unwrap_or(false)
}

fn ensure_account_tx(
    tx: &Transaction<'_>,
    account_key: &str,
    channel: &str,
    address: &str,
    provider: &str,
    profile_json: Value,
) -> Result<()> {
    let now = now_iso_string();
    tx.execute(
        r#"
        INSERT INTO communication_accounts (
            account_key, channel, address, provider, profile_json, created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL, NULL)
        ON CONFLICT(account_key) DO UPDATE SET
            channel=excluded.channel,
            address=excluded.address,
            provider=excluded.provider,
            profile_json=excluded.profile_json,
            updated_at=excluded.updated_at
        "#,
        params![
            account_key,
            channel,
            address,
            provider,
            serde_json::to_string(&profile_json)?,
            now,
        ],
    )?;
    Ok(())
}

pub(crate) fn refresh_thread(conn: &mut Connection, thread_key: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    refresh_thread_tx(&tx, thread_key)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn record_communication_sync_run(
    conn: &mut Connection,
    run: CommunicationSyncRun<'_>,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO communication_sync_runs (
            run_key, channel, account_key, folder_hint, started_at, finished_at,
            ok, fetched_count, stored_count, error_text, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            run.run_key,
            run.channel,
            run.account_key,
            run.folder_hint,
            run.started_at,
            run.finished_at,
            if run.ok { 1 } else { 0 },
            run.fetched_count,
            run.stored_count,
            run.error_text,
            run.metadata_json,
        ],
    )?;
    Ok(())
}

fn refresh_thread_tx(tx: &Transaction<'_>, thread_key: &str) -> Result<()> {
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

#[allow(dead_code)]
fn ensure_routing_rows_for_inbound_tx(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        SELECT
            m.message_key,
            CASE WHEN m.direction = 'outbound' THEN 'handled' ELSE 'pending' END,
            NULL,
            NULL,
            CASE WHEN m.direction = 'outbound' THEN m.observed_at ELSE NULL END,
            NULL,
            m.observed_at
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE r.message_key IS NULL
        "#,
        [],
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

fn parse_string_json_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub(crate) fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..24].to_string()
}

fn email_address_from_account_key(account_key: &str) -> String {
    account_key
        .strip_prefix("email:")
        .unwrap_or(account_key)
        .to_string()
}

#[derive(Debug)]
struct AccountConfig {
    provider: String,
    profile_json: Value,
}

fn load_account_config(conn: &Connection, account_key: &str) -> Result<Option<AccountConfig>> {
    let row = conn
        .query_row(
            r#"
            SELECT provider, profile_json
            FROM communication_accounts
            WHERE account_key = ?1
            LIMIT 1
            "#,
            params![account_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((provider, profile_json)) = row else {
        return Ok(None);
    };
    let parsed_profile = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| json!({ "raw_profile_json": profile_json }));
    Ok(Some(AccountConfig {
        provider,
        profile_json: parsed_profile,
    }))
}

fn jami_address_from_account_key(account_key: &str) -> String {
    account_key
        .strip_prefix("jami:")
        .unwrap_or(account_key)
        .to_string()
}

fn teams_tenant_from_account_config(account_config: Option<&AccountConfig>) -> Option<String> {
    account_config
        .and_then(|config| config.profile_json.get("tenantId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tenant_id| !tenant_id.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_account_key(conn: &Connection, channel: &str, explicit: Option<&str>) -> Result<String> {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(value.to_string());
    }
    conn.query_row(
        r#"
        SELECT account_key
        FROM communication_accounts
        WHERE channel = ?1
        ORDER BY updated_at DESC, account_key ASC
        LIMIT 1
        "#,
        params![channel],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("no configured account found for channel {channel}"))
}

fn resolve_db_path(root: &Path, explicit: Option<&str>) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(DEFAULT_DB_RELATIVE_PATH))
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Result<&'a str> {
    find_flag_value(args, flag).with_context(|| format!("missing required flag {flag}"))
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            return args.get(index + 1).map(String::as_str);
        }
        index += 1;
    }
    None
}

fn collect_flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    values
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn positional_after_flags(args: &[String]) -> Vec<String> {
    let mut items = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let token = &args[index];
        if token.starts_with("--") {
            index += 1;
            if index < args.len() && !args[index].starts_with("--") {
                index += 1;
            }
            continue;
        }
        items.push(token.clone());
        index += 1;
    }
    items
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_test_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn queue_tasks_round_trip_through_channel_store() {
        let root = std::env::temp_dir().join(format!(
            "ctox-queue-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test root");

        let created = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "queue smoke".to_string(),
                prompt: "Inspect the queue task round-trip.".to_string(),
                thread_key: "queue/test".to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("queue-orchestrator".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        assert_eq!(created.route_status, "pending");
        assert_eq!(created.priority, "high");
        assert_eq!(
            created.suggested_skill.as_deref(),
            Some("queue-orchestrator")
        );
        let conn = open_channel_db(&root.join(DEFAULT_DB_RELATIVE_PATH))
            .expect("failed to open channel db");
        let spawn_edge_count: i64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_spawn_edges
                WHERE child_entity_type = 'QueueTask'
                  AND child_entity_id = ?1
                  AND spawn_kind = 'queue-task'
                  AND parent_entity_type = 'Thread'
                  AND accepted = 1
                "#,
                params![&created.message_key],
                |row| row.get(0),
            )
            .expect("failed to count queue spawn edge");
        assert_eq!(spawn_edge_count, 1);

        let updated = update_queue_task(
            &root,
            QueueTaskUpdateRequest {
                message_key: created.message_key.clone(),
                priority: Some("urgent".to_string()),
                route_status: Some("blocked".to_string()),
                status_note: Some("waiting for owner".to_string()),
                ..Default::default()
            },
        )
        .expect("failed to update queue task");
        assert_eq!(updated.route_status, "blocked");
        assert_eq!(updated.priority, "urgent");
        assert_eq!(updated.status_note.as_deref(), Some("waiting for owner"));

        let loaded = load_queue_task(&root, &created.message_key)
            .expect("failed to load queue task")
            .expect("queue task missing after update");
        assert_eq!(loaded.message_key, created.message_key);
        assert_eq!(loaded.route_status, "blocked");

        let listed = list_queue_tasks(&root, &["blocked".to_string()], 10)
            .expect("failed to list blocked queue tasks");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].message_key, created.message_key);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_queue_task_lease_releases_to_pending() {
        let root = std::env::temp_dir().join(format!(
            "ctox-queue-stale-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test root");

        let created = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "stale lease".to_string(),
                prompt: "Release this stale queue lease.".to_string(),
                thread_key: "queue/stale".to_string(),
                workspace_root: None,
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue task");
        lease_queue_task(&root, &created.message_key, "ctox-service")
            .expect("failed to lease queue task");

        let released = release_stale_queue_task_leases(&root, "ctox-service", &HashSet::new())
            .expect("failed to release stale queue lease");
        assert_eq!(released, vec![created.message_key.clone()]);
        let reloaded = load_queue_task(&root, &created.message_key)
            .expect("failed to load queue task")
            .expect("missing queue task");
        assert_eq!(reloaded.route_status, "pending");
        assert!(reloaded.lease_owner.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tui_ingest_sanitizes_minimax_secret_before_persisting_message() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "ctox-tui-secret-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path)?;
        let fake_key = "sk-api-test_minimax_abcdefghijklmnopqrstuvwxyz0123456789";

        let stored = ingest_tui_message(
            &root,
            &mut conn,
            TuiIngestRequest {
                account_key: "local".to_string(),
                thread_key: "kunstmen-supervisor".to_string(),
                body: format!(
                    "MiniMax API key fuer MiniMax M2.7: {fake_key}. Bitte mit Secret-Skill ablegen."
                ),
                subject: "MiniMax key".to_string(),
                sender_display: "Codex".to_string(),
                sender_address: "tui:codex".to_string(),
                metadata: json!({"source": "test"}),
            },
        )?;
        let message_key = stored
            .get("message_key")
            .and_then(Value::as_str)
            .context("missing stored message key")?;
        let (body_text, preview, metadata_json): (String, String, String) = conn.query_row(
            "SELECT body_text, preview, metadata_json FROM communication_messages WHERE message_key = ?1",
            [message_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let metadata: Value = serde_json::from_str(&metadata_json)?;

        assert!(!body_text.contains(fake_key));
        assert!(!preview.contains(fake_key));
        assert!(body_text.contains("[secret-ref:credentials/MINIMAX_API_KEY"));
        assert_eq!(
            secrets::read_secret_value(&root, "credentials", "MINIMAX_API_KEY")?,
            fake_key
        );
        assert_eq!(
            metadata.get("secret_sanitized").and_then(Value::as_bool),
            Some(true)
        );

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn sync_prompt_identity_persists_founder_role_titles() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-role-sync-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "michael.welsch@metric-space.ai,o.schaefers@gmx.net".to_string(),
        );
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ROLES".to_string(),
            "michael.welsch@metric-space.ai=CEO / Founder,o.schaefers@gmx.net=Sales Officer"
                .to_string(),
        );

        sync_prompt_identity(&root, &settings).expect("failed to sync prompt identity");

        let db_path = resolve_db_path(&root, None);
        let conn = open_channel_db(&db_path).expect("failed to open channel db");
        let metadata_json: String = conn
            .query_row(
                "SELECT metadata_json FROM owner_profiles WHERE owner_key = ?1",
                ["o.schaefers@gmx.net"],
                |row| row.get(0),
            )
            .expect("failed to load founder profile");
        let metadata: Value =
            serde_json::from_str(&metadata_json).expect("failed to parse founder metadata");

        assert_eq!(
            metadata.get("role").and_then(Value::as_str),
            Some("founder")
        );
        assert_eq!(
            metadata.get("role_title").and_then(Value::as_str),
            Some("Sales Officer")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn owner_profile_settings_merge_adds_founder_mailboxes_for_routing() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-profile-settings-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path).expect("failed to open channel db");
        upsert_identity_profile(
            &mut conn,
            "mp@iip-gmbh.de",
            "Marco Pucciarelli",
            json!({
                "email": "mp@iip-gmbh.de",
                "role": "founder",
                "role_title": "CFO / Founder",
                "allow_admin_actions": true,
                "allow_sudo_actions": false,
                "mail_instruction_scope": "founder_strategic",
            }),
        )
        .expect("failed to insert founder profile");

        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "o.schaefers@gmx.net".to_string(),
        );

        merge_owner_profile_settings(&root, &mut settings).expect("failed to merge owner profiles");

        let policy = classify_email_sender(&settings, "mp@iip-gmbh.de");
        assert!(policy.allowed);
        assert_eq!(policy.role, "founder");
        assert!(settings
            .get("CTOX_FOUNDER_EMAIL_ROLES")
            .is_some_and(|roles| roles.contains("mp@iip-gmbh.de=CFO / Founder")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn founder_ack_guard_uses_sqlite_owner_profiles() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-ack-owner-profiles-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create temp test root");

        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path).expect("failed to open channel db");
        upsert_identity_profile(
            &mut conn,
            "mp@iip-gmbh.de",
            "Marco Pucciarelli",
            json!({
                "email": "mp@iip-gmbh.de",
                "role": "founder",
                "role_title": "CFO / Founder",
            }),
        )
        .expect("failed to insert founder profile");
        let message_key = "email:cto1@metric-space.ai::INBOX::101";
        conn.execute(
            r#"INSERT INTO communication_messages (
                message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
                sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
                bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
                trust_level, status, seen, has_attachments, external_created_at, observed_at,
                metadata_json
            ) VALUES (
                ?1, 'email', 'email:cto1@metric-space.ai', 'crm-thread',
                '101', 'inbound', 'INBOX', 'Marco Pucciarelli',
                'mp@iip-gmbh.de', '[]', '[]', '[]',
                'AW: Kunstmen CRM', 'CRM reply', 'Bitte beantworten.',
                '', '', 'normal', 'received', 0, 0,
                '2026-04-29T06:51:46Z', '2026-04-29T08:13:36Z', '{}'
            )"#,
            params![message_key],
        )
        .expect("failed to insert founder inbound");
        conn.execute(
            r#"INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (?1, 'leased', 'ctox-service', '2026-04-29T08:20:00Z', NULL, NULL, '2026-04-29T08:20:00Z')"#,
            params![message_key],
        )
        .expect("failed to insert route");

        let err = ack_leased_messages(&root, &[message_key.to_string()], "handled")
            .expect_err("founder mail must not be handled without reviewed send proof");
        assert!(err
            .to_string()
            .contains("cannot mark founder/owner/admin inbound mail as handled"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn queue_task_workspace_root_round_trips_and_legacy_prompt_falls_back() {
        let root = std::env::temp_dir().join(format!(
            "ctox-queue-workspace-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test root");

        let explicit = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "workspace explicit".to_string(),
                prompt: "Build only in the assigned workspace.".to_string(),
                thread_key: "queue/workspace-explicit".to_string(),
                workspace_root: Some("/tmp/ctox-explicit-workspace".to_string()),
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create explicit workspace task");
        assert_eq!(
            explicit.workspace_root.as_deref(),
            Some("/tmp/ctox-explicit-workspace")
        );

        let legacy = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "workspace legacy".to_string(),
                prompt: "Arbeite ausschließlich im Verzeichnis /tmp/ctox-legacy-workspace.\n\nImplementiere die Aufgabe dort.".to_string(),
                thread_key: "queue/workspace-legacy".to_string(),
                workspace_root: None,
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create legacy workspace task");
        assert_eq!(
            legacy.workspace_root.as_deref(),
            Some("/tmp/ctox-legacy-workspace")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn queue_task_prompt_update_preserves_existing_workspace_root() {
        let root = std::env::temp_dir().join(format!(
            "ctox-queue-workspace-update-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("failed to create temp test root");

        let task = create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "workspace update".to_string(),
                prompt: "Create and verify artifacts.".to_string(),
                thread_key: "queue/workspace-update".to_string(),
                workspace_root: Some("/tmp/ctox-original-workspace".to_string()),
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create workspace task");

        let updated = update_queue_task(
            &root,
            QueueTaskUpdateRequest {
                message_key: task.message_key,
                prompt: Some(
                    "HARNESS FEEDBACK\n\nCURRENT TASK\nWork only inside this workspace: /tmp/wrong-inline-text Execution contract: keep working.\n\nRUNTIME FAILURE\nx".to_string(),
                ),
                ..Default::default()
            },
        )
        .expect("failed to update workspace task");

        assert_eq!(
            updated.workspace_root.as_deref(),
            Some("/tmp/ctox-original-workspace")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn communication_support_paths_use_native_gateway_runtime_merge() {
        let root = PathBuf::from("/tmp/ctox-root");
        let mut email_settings = BTreeMap::new();
        email_settings.insert(
            "CTO_EMAIL_RAW_DIR".to_string(),
            root.join("runtime/communication/email/raw")
                .display()
                .to_string(),
        );
        assert_eq!(
            crate::communication::gateway::runtime_settings_from_settings(
                &root,
                crate::communication::gateway::CommunicationAdapterKind::Email,
                &email_settings,
            )
            .get("CTO_EMAIL_RAW_DIR"),
            Some(
                &root
                    .join("runtime/communication/email/raw")
                    .display()
                    .to_string()
            )
        );
        let mut jami_settings = BTreeMap::new();
        jami_settings.insert(
            "CTO_JAMI_INBOX_DIR".to_string(),
            root.join("runtime/communication/jami/inbox")
                .display()
                .to_string(),
        );
        assert_eq!(
            crate::communication::gateway::runtime_settings_from_settings(
                &root,
                crate::communication::gateway::CommunicationAdapterKind::Jami,
                &jami_settings,
            )
            .get("CTO_JAMI_INBOX_DIR"),
            Some(
                &root
                    .join("runtime/communication/jami/inbox")
                    .display()
                    .to_string()
            )
        );
    }

    #[test]
    fn resolve_outbound_subject_reuses_existing_thread_subject() {
        let db_path = unique_test_db_path("ctox-channel-subject-reuse");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "msg-1",
                channel: "email",
                account_key: "email:test@example.com",
                thread_key: "email/thread-1",
                remote_id: "remote-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Owner",
                sender_address: "owner@example.com",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Existing subject",
                preview: "preview",
                body_text: "body",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "owner_verified",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-03-26T10:00:00Z",
                observed_at: "2026-03-26T10:00:00Z",
                metadata_json: "{}",
            },
        )
        .expect("failed to upsert message");
        refresh_thread(&mut conn, "email/thread-1").expect("failed to refresh thread");

        let resolved = resolve_outbound_subject(
            &conn,
            ChannelSendRequest {
                channel: "email".to_string(),
                account_key: "email:test@example.com".to_string(),
                thread_key: "email/thread-1".to_string(),
                body: "reply".to_string(),
                subject: String::new(),
                to: vec!["owner@example.com".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
                sender_display: None,
                sender_address: None,
                send_voice: false,
                reviewed_founder_send: false,
            },
        )
        .expect("failed to resolve subject");
        assert_eq!(resolved.subject, "Existing subject");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn resolve_outbound_subject_rejects_missing_email_subject() {
        let db_path = unique_test_db_path("ctox-channel-subject-missing");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let error = resolve_outbound_subject(
            &conn,
            ChannelSendRequest {
                channel: "email".to_string(),
                account_key: "email:test@example.com".to_string(),
                thread_key: "email/thread-2".to_string(),
                body: "reply".to_string(),
                subject: "(no subject)".to_string(),
                to: vec!["owner@example.com".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
                sender_display: None,
                sender_address: None,
                send_voice: false,
                reviewed_founder_send: false,
            },
        )
        .expect_err("missing email subject should fail");
        assert!(
            error
                .to_string()
                .contains("email send requires a real subject"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn founder_outbound_email_requires_review_override() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );

        let error = validate_founder_outbound_email(
            &settings,
            &ChannelSendRequest {
                channel: "email".to_string(),
                account_key: "email:cto1@metric-space.ai".to_string(),
                thread_key: "mail-thread".to_string(),
                body: "Short founder update.".to_string(),
                subject: "Re: Test".to_string(),
                to: vec!["michael.welsch@metric-space.ai".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
                sender_display: None,
                sender_address: None,
                send_voice: false,
                reviewed_founder_send: false,
            },
        )
        .expect_err("founder outbound should require review override");

        assert!(
            error.to_string().contains("blocked without review"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parse_send_request_allows_teams_without_to_recipient() {
        let args = vec![
            "send".to_string(),
            "--channel".to_string(),
            "teams".to_string(),
            "--account-key".to_string(),
            "teams:bot".to_string(),
            "--thread-key".to_string(),
            "teams:bot::chat::chat-123".to_string(),
            "--body".to_string(),
            "kurze antwort".to_string(),
        ];

        let request = parse_send_request(&args).expect("teams send should not require --to");
        assert_eq!(request.channel, "teams");
        assert!(request.to.is_empty());
    }

    #[test]
    fn teams_tenant_comes_from_profile_not_account_key() {
        let account = AccountConfig {
            provider: "graph".to_string(),
            profile_json: json!({
                "tenantId": "tenant-123",
            }),
        };

        assert_eq!(
            teams_tenant_from_account_config(Some(&account)).as_deref(),
            Some("tenant-123")
        );
        assert_eq!(teams_tenant_from_account_config(None), None);
    }

    #[test]
    fn parse_send_request_still_requires_email_to_recipient() {
        let args = vec![
            "send".to_string(),
            "--channel".to_string(),
            "email".to_string(),
            "--account-key".to_string(),
            "email:cto@example.com".to_string(),
            "--thread-key".to_string(),
            "email-thread".to_string(),
            "--body".to_string(),
            "kurze antwort".to_string(),
        ];

        let error = parse_send_request(&args).expect_err("email send should require --to");
        assert!(
            error
                .to_string()
                .contains("channel send for email requires at least one --to value"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn founder_outbound_email_does_not_string_scrape_body_content() {
        // Core no longer scrapes outbound bodies for "internal vocabulary"
        // substrings. That guidance lives in `owner-communication/SKILL.md`.
        // Whatever the body contains, the generic `channel send` path is
        // still blocked for founder/owner/admin recipients — the operator
        // must use the reviewed founder-outbound pipeline.
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "founder@example.com".to_string(),
        );

        let error = validate_founder_outbound_email(
            &settings,
            &ChannelSendRequest {
                channel: "email".to_string(),
                account_key: "email:cto1@metric-space.ai".to_string(),
                thread_key: "mail-thread".to_string(),
                body: "Die Dateien liegen unter /home/ubuntu/workspace/kunstmen/public/mockups/."
                    .to_string(),
                subject: "Re: Test".to_string(),
                to: vec!["founder@example.com".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
                sender_display: None,
                sender_address: None,
                send_voice: false,
                reviewed_founder_send: true,
            },
        )
        .expect_err("generic founder send should still be blocked irrespective of body content");

        let message = error.to_string();
        assert!(
            message.contains("generic channel send is disabled"),
            "unexpected error: {message}"
        );
        assert!(
            !message.contains("internal-language leakage"),
            "core must not string-scrape outbound bodies anymore: {message}"
        );
    }

    #[test]
    fn founder_outbound_body_rejects_address_headers_in_body() {
        let error = ensure_founder_outbound_body_clean(&ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "An: founder@example.com\nCc: owner@example.com\nBetreff: Re: Test\n\nHallo zusammen,\n\nsauberer Text.".to_string(),
            subject: "Re: Test".to_string(),
            to: vec!["founder@example.com".to_string()],
            cc: vec!["owner@example.com".to_string()],
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        })
        .expect_err("body header preamble should be blocked");

        assert!(
            error
                .to_string()
                .contains("headers were placed in the message body"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn founder_outbound_body_rejects_internal_send_status_report() {
        let error = ensure_founder_outbound_body_clean(&ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Die Founder-Mail ist als Reply raus. Review-Approval, Send-Proof, Outbound-Message-Row und Routing-State sind persistiert; Michaels Inbound `email:cto1@metric-space.ai::INBOX::105` steht jetzt auf `handled`."
                .to_string(),
            subject: "Kunstmen CRM: ehrlicher Zwischenstand".to_string(),
            to: vec!["founder@example.com".to_string()],
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        })
        .expect_err("internal send status reports must never reach founders");

        let message = error.to_string();
        assert!(
            message.contains("internal-language leakage"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("review-approval") || message.contains("routing-state"),
            "unexpected markers: {message}"
        );
    }

    #[test]
    fn founder_outbound_email_still_blocks_generic_send_after_review_override() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            "founder@example.com".to_string(),
        );

        let error = validate_founder_outbound_email(
            &settings,
            &ChannelSendRequest {
                channel: "email".to_string(),
                account_key: "email:cto1@metric-space.ai".to_string(),
                thread_key: "mail-thread".to_string(),
                body: "Kurzes sauberes Update ohne internen Systemmuell.".to_string(),
                subject: "Re: Test".to_string(),
                to: vec!["founder@example.com".to_string()],
                cc: Vec::new(),
                attachments: Vec::new(),
                sender_display: None,
                sender_address: None,
                send_voice: false,
                reviewed_founder_send: true,
            },
        )
        .expect_err("generic founder send should still be blocked");

        assert!(
            error
                .to_string()
                .contains("generic channel send is disabled"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn reviewed_founder_reply_for_forward_targets_original_recipient_and_ccs_sender() {
        let db_path = unique_test_db_path("ctox-founder-forward-reply");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "email:cto1@metric-space.ai::INBOX::forward-1",
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<forward-thread@example.com>",
                remote_id: "remote-forward-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Michael Welsch",
                sender_address: "michael.welsch@metric-space.ai",
                recipient_addresses_json: "[\"o.schaefers@gmx.net\"]",
                cc_addresses_json: "[\"cto1@metric-space.ai\"]",
                bcc_addresses_json: "[]",
                subject: "Fwd: Visuelle Homepage",
                preview: "Hi Olaf",
                body_text: "Hi Olaf,\n\nAnfang der weitergeleiteten Nachricht:\n...",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T12:04:04Z",
                observed_at: "2026-04-24T12:04:05Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");

        let inbound = load_message_from_conn(&conn, "email:cto1@metric-space.ai::INBOX::forward-1")
            .expect("load inbound")
            .expect("inbound missing");
        let addressing = load_message_addressing_from_conn(
            &conn,
            "email:cto1@metric-space.ai::INBOX::forward-1",
        )
        .expect("load addressing")
        .expect("addressing missing");

        let (to, cc) = derive_founder_reply_recipients(&inbound, &addressing);
        assert_eq!(to, vec!["o.schaefers@gmx.net".to_string()]);
        assert_eq!(cc, vec!["michael.welsch@metric-space.ai".to_string()]);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn founder_reply_detects_qr_code_as_required_deliverable() {
        let required = detect_required_founder_deliverables(
            "Jami zugang schicken.",
            "Schick mir bitte den Jami QR code Zugang für den Chat mir dir.",
        );
        assert_eq!(required, vec!["qr_code".to_string()]);
    }

    #[test]
    fn founder_reply_blocks_send_when_qr_code_is_missing() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-qr-deliverable-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "email:cto1@metric-space.ai::INBOX::qr-1",
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<qr-thread@example.com>",
                remote_id: "remote-qr-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Michael Welsch",
                sender_address: "michael.welsch@metric-space.ai",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Jami zugang schicken.",
                preview: "QR code needed",
                body_text: "Schick mir bitte den Jami QR code Zugang für den Chat mir dir.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T14:28:56Z",
                observed_at: "2026-04-24T14:28:56Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");

        let error = ensure_founder_reply_deliverables_present(
            &root,
            "email:cto1@metric-space.ai::INBOX::qr-1",
            "Hi Michael,\n\nhier ist der direkte Jami-Zugang:\n\njami:abc123",
            &[],
        )
        .expect_err("missing qr code should block founder reply");
        assert!(error
            .to_string()
            .contains("missing required deliverable(s): qr_code"));

        let _ = std::fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn teams_work_ack_is_blocked_without_pipeline_backing() {
        let root = std::env::temp_dir().join(format!(
            "ctox-teams-ack-guard-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = root.join(DEFAULT_DB_RELATIVE_PATH);
        let conn = open_channel_db(&db_path).expect("failed to open channel db");
        let request = ChannelSendRequest {
            channel: "teams".to_string(),
            account_key: "teams:inf.yoda@example.test".to_string(),
            thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
            body: "Danke für den Hinweis — verstanden. Ich scrolle die Seite vollständig durch und übertrage die Aussteller aus Deutschland in eine Excel.".to_string(),
            subject: "(Teams)".to_string(),
            to: Vec::new(),
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        };

        let err = enforce_external_work_ack_has_pipeline_backing(&conn, &request)
            .expect_err("work acknowledgement must require durable backing");
        assert!(err.to_string().contains("promises follow-up work"));

        let _ = std::fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn teams_work_ack_is_allowed_with_queue_backing() {
        let root = std::env::temp_dir().join(format!(
            "ctox-teams-ack-backed-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("failed to create temp root");
        let thread_key = "teams:inf.yoda@example.test::chat::jill";
        create_queue_task(
            &root,
            QueueTaskCreateRequest {
                title: "Intersolar-Aussteller Deutschland in Excel".to_string(),
                prompt: "Scrape Intersolar and create the verified Excel artifact.".to_string(),
                thread_key: thread_key.to_string(),
                workspace_root: None,
                priority: "high".to_string(),
                suggested_skill: Some("universal-scraping".to_string()),
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("failed to create queue backing");
        let conn =
            open_channel_db(&root.join(DEFAULT_DB_RELATIVE_PATH)).expect("failed to reopen db");
        let request = ChannelSendRequest {
            channel: "teams".to_string(),
            account_key: "teams:inf.yoda@example.test".to_string(),
            thread_key: thread_key.to_string(),
            body: "Danke, ich prüfe das und erstelle die Excel.".to_string(),
            subject: "(Teams)".to_string(),
            to: Vec::new(),
            cc: Vec::new(),
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        };

        enforce_external_work_ack_has_pipeline_backing(&conn, &request)
            .expect("queue-backed acknowledgement should be allowed");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn teams_send_allows_attachments_for_adapter_delivery() {
        let request = ChannelSendRequest {
            channel: "teams".to_string(),
            account_key: "teams:inf.yoda@example.test".to_string(),
            thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
            body: "Hier ist die Excel.".to_string(),
            subject: "(Teams)".to_string(),
            to: Vec::new(),
            cc: Vec::new(),
            attachments: vec!["/tmp/result.xlsx".to_string()],
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: false,
        };

        enforce_channel_attachment_support(&request)
            .expect("Teams attachments are handed to the adapter for Graph delivery");
    }

    #[test]
    fn reviewed_founder_reply_requires_exact_approval_before_send() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-exact-review-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::exact-review-1";
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: inbound_key,
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<exact-review-thread@example.com>",
                remote_id: "remote-exact-review-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Michael Welsch",
                sender_address: "michael.welsch@metric-space.ai",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Status",
                preview: "Status bitte",
                body_text: "Bitte antworte mit dem aktuellen Stand.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T18:00:00Z",
                observed_at: "2026-04-24T18:00:00Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");

        let action = prepare_reviewed_founder_reply(&root, inbound_key).expect("prepare reply");
        let approved_body = "Hi Michael,\n\nDer Status ist jetzt konkret.";
        let before_approval =
            require_unconsumed_founder_reply_review(&conn, inbound_key, &action, approved_body)
                .expect_err("send must be blocked before review approval");
        assert!(before_approval
            .to_string()
            .contains("no matching unconsumed review approval"));

        record_founder_reply_review_approval(&root, inbound_key, approved_body, "PASS")
            .expect("record approval");
        let conn = open_channel_db(&db_path).expect("reopen db");
        require_unconsumed_founder_reply_review(&conn, inbound_key, &action, approved_body)
            .expect("exact reviewed body should be approved");
        let changed_body =
            require_unconsumed_founder_reply_review(&conn, inbound_key, &action, "Changed body")
                .expect_err("changed body must not inherit the approval");
        assert!(changed_body
            .to_string()
            .contains("no matching unconsumed review approval"));

        let _ = std::fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reviewed_founder_send_writes_core_transition_proof() {
        let db_path = unique_test_db_path("ctox-founder-core-proof");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "mail-thread".to_string(),
            body: "Hi Michael,\n\nDer Status ist belegt.".to_string(),
            subject: "Re: Status".to_string(),
            to: vec!["michael.welsch@metric-space.ai".to_string()],
            cc: vec!["o.schaefers@gmx.net".to_string()],
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        };

        enforce_reviewed_founder_send_core_transition(
            &conn,
            "founder-reply:email:cto1@metric-space.ai::INBOX::proof",
            "founder-review:proof",
            &request,
        )
        .expect("reviewed founder send should write an accepted proof");

        let accepted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_core_transition_proofs
                 WHERE entity_type = 'FounderCommunication'
                   AND lane = 'P0FounderCommunication'
                   AND from_state = 'Approved'
                   AND to_state = 'Sending'
                   AND accepted = 1",
                [],
                |row| row.get(0),
            )
            .expect("failed to count proofs");
        assert_eq!(accepted, 1);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn founder_inbound_cannot_be_handled_without_reviewed_send() {
        let root = std::env::temp_dir().join(format!(
            "ctox-founder-handled-guard-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let mut runtime_settings = BTreeMap::new();
        runtime_settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "michael.welsch@metric-space.ai".to_string(),
        );
        crate::inference::runtime_env::save_runtime_env_map(&root, &runtime_settings)
            .expect("failed to persist owner setting");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        let inbound_key = "email:cto1@metric-space.ai::INBOX::handled-guard-1";
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: inbound_key,
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<handled-guard-thread@example.com>",
                remote_id: "remote-handled-guard-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Michael Welsch",
                sender_address: "michael.welsch@metric-space.ai",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Bitte antworten",
                preview: "Bitte antworten",
                body_text: "Bitte antworte.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T18:05:00Z",
                observed_at: "2026-04-24T18:05:00Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");

        let err = ack_leased_messages(&root, &[inbound_key.to_string()], "handled")
            .expect_err("founder inbound should not be handleable before reviewed send");
        assert!(err
            .to_string()
            .contains("cannot mark founder/owner/admin inbound mail as handled"));

        let _ = std::fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn take_messages_allows_pending_rows_with_stale_lease_owner() {
        let db_path = unique_test_db_path("ctox-channel-take-pending-stale-owner");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "pending-stale-owner-1",
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<pending-stale-owner@example.com>",
                remote_id: "remote-pending-stale-owner-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Michael Welsch",
                sender_address: "michael.welsch@metric-space.ai",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Re: Visuelle Homepage",
                preview: "latest founder reply",
                body_text: "Please answer the latest founder feedback.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T18:41:06Z",
                observed_at: "2026-04-24T18:41:06Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
        ensure_routing_rows_for_inbound(&conn).expect("routing rows");
        conn.execute(
            "UPDATE communication_routing_state SET route_status='pending', lease_owner='ctox', leased_at='2026-04-24T18:41:07Z' WHERE message_key=?1",
            params!["pending-stale-owner-1"],
        )
        .expect("failed to seed stale lease owner");

        let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
            .expect("take messages should succeed");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].message_key, "pending-stale-owner-1");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn releasing_leased_message_to_pending_does_not_ack_it() {
        let root = std::env::temp_dir().join(format!(
            "ctox-channel-pending-release-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "pending-release-1",
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<pending-release@example.com>",
                remote_id: "remote-pending-release-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Customer",
                sender_address: "customer@example.com",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Needs rework",
                preview: "needs rework",
                body_text: "Please rework before replying.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-24T18:42:06Z",
                observed_at: "2026-04-24T18:42:06Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
        ensure_routing_rows_for_inbound(&conn).expect("routing rows");
        let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
            .expect("take messages should succeed");
        assert_eq!(taken.len(), 1);

        ack_leased_messages(&root, &["pending-release-1".to_string()], "pending")
            .expect("pending release should succeed");
        let (route_status, acked_at): (String, Option<String>) = conn
            .query_row(
                "SELECT route_status, acked_at FROM communication_routing_state WHERE message_key = ?1",
                params!["pending-release-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("missing routing row");
        assert_eq!(route_status, "pending");
        assert!(acked_at.is_none());

        let taken_again = take_messages(&mut conn, Some("email"), 10, "ctox-service")
            .expect("released pending message should be leaseable again");
        assert_eq!(taken_again.len(), 1);
        let acked_after_retake: Option<String> = conn
            .query_row(
                "SELECT acked_at FROM communication_routing_state WHERE message_key = ?1",
                params!["pending-release-1"],
                |row| row.get(0),
            )
            .expect("missing routing row after retake");
        assert!(acked_after_retake.is_none());

        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stalled_inbound_includes_acked_failed_messages_for_repair() {
        let root = std::env::temp_dir().join(format!(
            "ctox-channel-acked-failed-stalled-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = root.join("runtime/ctox.sqlite3");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "acked-failed-founder-1",
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "<acked-failed-founder@example.com>",
                remote_id: "remote-acked-failed-founder-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Founder",
                sender_address: "founder@example.com",
                recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Affiliate follow-up",
                preview: "Please answer this founder follow-up.",
                body_text: "Please answer this founder follow-up.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-27T09:01:02Z",
                observed_at: "2026-04-27T09:01:02Z",
                metadata_json: "{}",
            },
        )
        .expect("message upsert");
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            ) VALUES (
                'acked-failed-founder-1', 'failed', NULL, NULL,
                '2026-04-27T11:55:27Z', NULL, '2026-04-27T11:55:27Z'
            )
            "#,
            [],
        )
        .expect("failed to seed failed acked route");

        let stalled =
            list_stalled_inbound_messages(&root, 10).expect("failed to list stalled messages");
        assert_eq!(stalled.len(), 1);
        assert_eq!(stalled[0].message_key, "acked-failed-founder-1");

        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn take_messages_prefers_latest_pending_message_in_thread() {
        let db_path = unique_test_db_path("ctox-channel-take-latest-per-thread");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        for (message_key, external_created_at, preview) in [
            (
                "thread-msg-old",
                "2026-04-24T18:10:00Z",
                "old founder reply",
            ),
            (
                "thread-msg-new",
                "2026-04-24T18:41:06Z",
                "latest founder reply",
            ),
        ] {
            upsert_communication_message(
                &mut conn,
                UpsertMessage {
                    message_key,
                    channel: "email",
                    account_key: "email:cto1@metric-space.ai",
                    thread_key: "<latest-thread@example.com>",
                    remote_id: message_key,
                    direction: "inbound",
                    folder_hint: "INBOX",
                    sender_display: "Michael Welsch",
                    sender_address: "michael.welsch@metric-space.ai",
                    recipient_addresses_json: "[\"cto1@metric-space.ai\"]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject: "Re: Visuelle Homepage",
                    preview,
                    body_text: preview,
                    body_html: "",
                    raw_payload_ref: "",
                    trust_level: "trusted",
                    status: "received",
                    seen: false,
                    has_attachments: false,
                    external_created_at,
                    observed_at: external_created_at,
                    metadata_json: "{}",
                },
            )
            .expect("message upsert");
        }
        ensure_routing_rows_for_inbound(&conn).expect("routing rows");

        let taken = take_messages(&mut conn, Some("email"), 10, "ctox-service")
            .expect("take messages should succeed");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].message_key, "thread-msg-new");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn thread_prefers_voice_reply_for_voice_jami_inbound() {
        let db_path = unique_test_db_path("ctox-channel-jami-voice");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "msg-jami-voice-1",
                channel: "jami",
                account_key: "jami:test-account",
                thread_key: "jami/thread-voice-1",
                remote_id: "remote-jami-voice-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Owner",
                sender_address: "jami:owner",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Voice subject",
                preview: "voice preview",
                body_text: "voice body",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "owner_verified",
                status: "received",
                seen: false,
                has_attachments: true,
                external_created_at: "2026-03-29T08:00:00Z",
                observed_at: "2026-03-29T08:00:00Z",
                metadata_json: r#"{"preferredReplyModality":"voice"}"#,
            },
        )
        .expect("failed to upsert jami voice message");

        assert!(thread_prefers_voice_reply(&conn, "jami/thread-voice-1")
            .expect("failed to resolve jami voice preference"));

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn system_probe_inbound_messages_are_marked_handled() {
        let db_path = unique_test_db_path("ctox-channel-system-probe");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "probe-1",
                channel: "email",
                account_key: "email:test@example.com",
                thread_key: "email/thread-self-test",
                remote_id: "remote-probe-1",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "CTOX",
                sender_address: "test@example.com",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "[CTOX mail self-test] 2026-03-26T10:00:00Z",
                preview: "CTOX self-test",
                body_text: "CTOX self-test <abc@example.com>",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "system_probe",
                status: "self_test_received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-03-26T10:00:00Z",
                observed_at: "2026-03-26T10:00:00Z",
                metadata_json: "{\"technicalSelfTest\":true}",
            },
        )
        .expect("failed to insert system probe");
        ensure_routing_rows_for_inbound(&conn).expect("failed to backfill routing rows");
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                params!["probe-1"],
                |row| row.get(0),
            )
            .expect("missing routing row");
        assert_eq!(route_status, "handled");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn system_probe_routing_heals_existing_non_handled_rows() {
        let db_path = unique_test_db_path("ctox-channel-system-probe-heal");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "probe-2",
                channel: "email",
                account_key: "email:test@example.com",
                thread_key: "email/thread-self-test-2",
                remote_id: "remote-probe-2",
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "CTOX",
                sender_address: "test@example.com",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "[CTOX mail self-test] 2026-03-26T10:10:00Z",
                preview: "CTOX self-test",
                body_text: "CTOX self-test <def@example.com>",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "system_probe",
                status: "self_test_received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-03-26T10:10:00Z",
                observed_at: "2026-03-26T10:10:00Z",
                metadata_json: "{\"technicalSelfTest\":true}",
            },
        )
        .expect("failed to insert system probe");
        conn.execute(
            "INSERT INTO communication_routing_state (message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at) VALUES (?1, 'blocked_sender', NULL, NULL, NULL, 'legacy', ?2)",
            params!["probe-2", "2026-03-26T10:10:01Z"],
        )
        .expect("failed to seed legacy routing row");
        ensure_routing_rows_for_inbound(&conn).expect("failed to normalize routing rows");
        let route_status: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key = ?1",
                params!["probe-2"],
                |row| row.get(0),
            )
            .expect("missing routing row");
        assert_eq!(route_status, "handled");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn channel_history_and_search_can_reconstruct_related_messages() {
        let db_path = unique_test_db_path("ctox-channel-search");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        for (message_key, thread_key, sender_address, subject, preview, body_text, created_at) in [
            (
                "msg-1",
                "email/thread-a",
                "owner@example.com",
                "Nextcloud blocked",
                "Need endpoint",
                "Nextcloud is blocked on missing endpoint and credentials.",
                "2026-03-26T10:00:00Z",
            ),
            (
                "msg-2",
                "email/thread-a",
                "ctox@example.com",
                "Nextcloud blocked",
                "Asked for endpoint",
                "I asked for NEXTCLOUD_URL and credentials.",
                "2026-03-26T10:05:00Z",
            ),
            (
                "msg-3",
                "email/thread-b",
                "owner@example.com",
                "Redis recovered",
                "Rotated secret",
                "The Redis password was rotated and the service is healthy now.",
                "2026-03-26T11:00:00Z",
            ),
        ] {
            upsert_communication_message(
                &mut conn,
                UpsertMessage {
                    message_key,
                    channel: "email",
                    account_key: "email:cto1@example.com",
                    thread_key,
                    remote_id: message_key,
                    direction: "inbound",
                    folder_hint: "INBOX",
                    sender_display: "Owner",
                    sender_address,
                    recipient_addresses_json: "[]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject,
                    preview,
                    body_text,
                    body_html: "",
                    raw_payload_ref: "",
                    trust_level: "owner_verified",
                    status: "received",
                    seen: false,
                    has_attachments: false,
                    external_created_at: created_at,
                    observed_at: created_at,
                    metadata_json: "{}",
                },
            )
            .expect("failed to insert communication message");
        }

        let thread_history =
            list_thread_messages(&conn, "email/thread-a", 10).expect("failed to load history");
        assert_eq!(thread_history.len(), 2);
        assert_eq!(thread_history[0].message_key, "msg-2");

        let search = search_messages(&conn, "nextcloud endpoint", Some("email"), None, 10)
            .expect("failed to search");
        assert_eq!(search.len(), 2);
        assert!(search
            .iter()
            .all(|item| item.thread_key == "email/thread-a"));

        let sender_search =
            search_messages(&conn, "redis", Some("email"), Some("owner@example.com"), 10)
                .expect("failed to search sender-scoped messages");
        assert_eq!(sender_search.len(), 1);
        assert_eq!(sender_search[0].message_key, "msg-3");

        let _ = fs::remove_file(&db_path);
    }

    #[test]
    fn channel_context_groups_thread_state_blockers_and_open_questions() {
        let db_path = unique_test_db_path("ctox-channel-context");
        let mut conn = open_channel_db(&db_path).expect("failed to open db");
        for (
            message_key,
            thread_key,
            direction,
            sender_display,
            sender_address,
            subject,
            preview,
            body_text,
            created_at,
        ) in [
            (
                "ctx-1",
                "email/thread-zammad",
                "inbound",
                "Michael",
                "michael@example.com",
                "Zammad status",
                "Please finish it",
                "Can you finish Zammad and report the blocker?",
                "2026-03-26T10:00:00Z",
            ),
            (
                "ctx-2",
                "email/thread-zammad",
                "outbound",
                "CTOX",
                "cto1@example.com",
                "Zammad status",
                "Still blocked",
                "Blocked: the admin API still returns 401 and I need to repair auth.",
                "2026-03-26T10:05:00Z",
            ),
            (
                "ctx-3",
                "email/thread-zammad",
                "inbound",
                "Michael",
                "michael@example.com",
                "Zammad status",
                "Please continue",
                "Please continue and tell me if you need anything else?",
                "2026-03-26T10:10:00Z",
            ),
            (
                "ctx-4",
                "tui/main",
                "inbound",
                "Michael",
                "tui:local",
                "TUI",
                "Freigabe",
                "Die Freigabe fuer die Zammad-Reparatur ist erteilt.",
                "2026-03-26T10:12:00Z",
            ),
            (
                "ctx-5",
                "email/thread-redis",
                "outbound",
                "CTOX",
                "cto1@example.com",
                "Redis repaired",
                "Follow-up queued",
                "I queued a follow-up review for Redis and will continue after verification.",
                "2026-03-26T09:00:00Z",
            ),
        ] {
            upsert_communication_message(
                &mut conn,
                UpsertMessage {
                    message_key,
                    channel: if thread_key.starts_with("tui/") {
                        "tui"
                    } else {
                        "email"
                    },
                    account_key: "email:cto1@example.com",
                    thread_key,
                    remote_id: message_key,
                    direction,
                    folder_hint: if direction == "inbound" {
                        "INBOX"
                    } else {
                        "Sent"
                    },
                    sender_display,
                    sender_address,
                    recipient_addresses_json: "[]",
                    cc_addresses_json: "[]",
                    bcc_addresses_json: "[]",
                    subject,
                    preview,
                    body_text,
                    body_html: "",
                    raw_payload_ref: "",
                    trust_level: "owner_verified",
                    status: "received",
                    seen: false,
                    has_attachments: false,
                    external_created_at: created_at,
                    observed_at: created_at,
                    metadata_json: "{}",
                },
            )
            .expect("failed to insert context message");
        }

        let context = build_communication_context(
            &conn,
            "email/thread-zammad",
            Some("zammad blocker repair"),
            Some("michael@example.com"),
            10,
        )
        .expect("failed to build communication context");

        assert_eq!(context.thread_messages.len(), 3);
        assert_eq!(context.latest_subject.as_deref(), Some("Zammad status"));
        assert_eq!(
            context
                .latest_inbound
                .as_ref()
                .map(|item| item.message_key.as_str()),
            Some("ctx-3")
        );
        assert_eq!(
            context
                .latest_outbound
                .as_ref()
                .map(|item| item.message_key.as_str()),
            Some("ctx-2")
        );
        assert!(!context.candidate_blockers.is_empty());
        assert!(context
            .candidate_blockers
            .iter()
            .any(|item| item.message_key == "ctx-2"));
        assert!(!context.open_owner_questions.is_empty());
        assert!(context
            .related_messages
            .iter()
            .any(|item| item.message_key == "ctx-4"));

        let _ = fs::remove_file(&db_path);
    }

    // F4: pipeline_status returns a structured snapshot joining mission
    // state, agent attempts, review/approval rows, and outbound sends for
    // a given thread_key. The test seeds rows into the runtime db that
    // `pipeline_status` will resolve via `resolve_db_path(root, None)`.
    #[test]
    fn pipeline_status_reports_thread_state() {
        let root = std::env::temp_dir().join(format!(
            "ctox-pipeline-status-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");

        let thread_key = "pipeline-status-thread";

        // Seed the channel db with one outbound message and one routing row.
        let db_path = root.join(DEFAULT_DB_RELATIVE_PATH);
        let mut conn = open_channel_db(&db_path).expect("open channel db");
        upsert_communication_message(
            &mut conn,
            UpsertMessage {
                message_key: "outbound-1",
                channel: "email",
                account_key: "email:cto1@example.com",
                thread_key,
                remote_id: "remote-1",
                direction: "outbound",
                folder_hint: "Sent",
                sender_display: "CTOX",
                sender_address: "cto1@example.com",
                recipient_addresses_json: "[\"founder@example.com\"]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Founder update",
                preview: "Update for founder.",
                body_text: "Update for founder.",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "trusted",
                status: "sent",
                seen: true,
                has_attachments: false,
                external_created_at: "2026-04-27T10:00:00Z",
                observed_at: "2026-04-27T10:00:00Z",
                metadata_json: "{}",
            },
        )
        .expect("failed to upsert outbound");

        // Seed an agent assistant row with structured outcome on the matching conversation_id.
        let conversation_id =
            crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(thread_key));
        let engine = crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())
            .expect("open lcm engine");
        let _ = engine
            .add_message_with_outcome(
                conversation_id,
                "assistant",
                "(agent turn did not complete)",
                Some(crate::lcm::AgentOutcome::TurnTimeout),
            )
            .expect("seed assistant row");
        // Bump the failure counter to simulate one failed turn.
        let _ = engine
            .increment_mission_agent_failure_count(conversation_id)
            .expect("bump failure count");

        let report = pipeline_status(&root, Some(thread_key), 10).expect("pipeline status");
        assert_eq!(report.thread_key.as_deref(), Some(thread_key));
        assert_eq!(report.send_attempts.len(), 1);
        assert_eq!(report.send_attempts[0].message_key, "outbound-1");
        assert_eq!(report.agent_attempts.len(), 1);
        assert_eq!(
            report.agent_attempts[0].outcome.as_deref(),
            Some("TurnTimeout")
        );
        assert_eq!(report.agent_failure_count, 1);
        // No review row was seeded → no founder_outbound_intent.
        assert!(!report.founder_outbound_intent);
        assert_eq!(report.rewrite_iteration_count, 0);
        assert_eq!(report.rework_iteration_count, 1);
        assert_eq!(report.current_disposition, "RequeueSelfWork");
        assert!(
            report.strategic_directive_authority_events.is_empty(),
            "no strategic-directive authority events seeded → field must be empty"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // E (PR): pipeline_status surfaces strategic-directive owner-authority
    // governance events that match the thread, but skips events whose
    // details reference a different thread.
    #[test]
    fn pipeline_status_surfaces_strategic_directive_authority_events() {
        let root = std::env::temp_dir().join(format!(
            "ctox-pipeline-strategy-auth-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");

        let thread_key = "pipeline-strategy-auth-thread";
        let conversation_id =
            crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(thread_key));

        // Seed two strategic-directive authority events: one matching this
        // thread, one matching an unrelated thread. Only the first should
        // surface in the report.
        let _ = crate::governance::record_event(
            &root,
            crate::governance::GovernanceEventRequest {
                mechanism_id: "strategic_directive_mutation_owner_authorised",
                conversation_id: Some(conversation_id),
                severity: "info",
                reason: "test-permitted",
                action_taken: "permitted_strategic_directive_mutation",
                details: serde_json::json!({
                    "triggered_by_message_key": "owner-msg-A",
                    "sender_address": "owner@example.com",
                    "sender_role": "owner",
                    "directive_kind": "mission",
                    "attempted_status": "active",
                    "action": "set",
                    "thread_key": thread_key,
                    "conversation_id": conversation_id,
                }),
                idempotence_key: Some("test-permitted-A"),
            },
        )
        .expect("record permitted event");
        let _ = crate::governance::record_event(
            &root,
            crate::governance::GovernanceEventRequest {
                mechanism_id: "strategic_directive_mutation_blocked_non_owner_sender",
                conversation_id: None,
                severity: "critical",
                reason: "test-blocked-other-thread",
                action_taken: "blocked_strategic_directive_mutation",
                details: serde_json::json!({
                    "triggered_by_message_key": "founder-msg-B",
                    "sender_address": "founder@example.com",
                    "sender_role": "founder",
                    "directive_kind": "vision",
                    "attempted_status": "active",
                    "action": "set",
                    "thread_key": "some-other-thread",
                }),
                idempotence_key: Some("test-blocked-B"),
            },
        )
        .expect("record blocked event");

        let report = pipeline_status(&root, Some(thread_key), 10).expect("pipeline status");
        let events = &report.strategic_directive_authority_events;
        assert_eq!(
            events.len(),
            1,
            "expected exactly the matching authority event in the per-thread report, got {events:#?}"
        );
        assert_eq!(
            events[0].mechanism_id,
            "strategic_directive_mutation_owner_authorised"
        );
        assert_eq!(events[0].sender_role.as_deref(), Some("owner"));
        assert_eq!(events[0].directive_kind.as_deref(), Some("mission"));
        assert_eq!(events[0].action.as_deref(), Some("set"));
        assert_eq!(
            events[0].triggered_by_message_key.as_deref(),
            Some("owner-msg-A")
        );

        let _ = fs::remove_dir_all(&root);
    }

    fn unique_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn upsert_test_inbound(
        conn: &mut Connection,
        message_key: &str,
        metadata: Value,
    ) -> Result<()> {
        upsert_communication_message(
            conn,
            UpsertMessage {
                message_key,
                channel: "email",
                account_key: "email:cto1@metric-space.ai",
                thread_key: "email/test-thread",
                remote_id: message_key,
                direction: "inbound",
                folder_hint: "INBOX",
                sender_display: "Jill",
                sender_address: "j.cakmak@remcapital.de",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Re: any subject (irrelevant for structural test)",
                preview: "preview",
                body_text: "body",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "high",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: "2026-04-27T09:00:00Z",
                observed_at: "2026-04-27T09:00:00Z",
                metadata_json: &metadata.to_string(),
            },
        )?;
        Ok(())
    }

    #[test]
    fn metadata_marks_auto_submitted_consults_only_structured_fields() {
        // Subject and body content must NOT influence the decision —
        // only the structured fields written by the inbound parser.
        let positive = json!({"autoSubmitted": true});
        assert!(metadata_marks_auto_submitted(&positive));
        let suppress = json!({"autoResponseSuppress": true});
        assert!(metadata_marks_auto_submitted(&suppress));
        let raw_value = json!({"autoSubmittedValue": "auto-replied; foo=bar"});
        assert!(metadata_marks_auto_submitted(&raw_value));
        let neg = json!({
            "subject": "Automatische Antwort: ich bin im Urlaub",
            "body_text": "Out of office until 2026-05-12.",
        });
        assert!(
            !metadata_marks_auto_submitted(&neg),
            "subject/body strings must never trigger the marker"
        );
        let explicit_no = json!({"autoSubmitted": false, "autoSubmittedValue": "no"});
        assert!(!metadata_marks_auto_submitted(&explicit_no));
    }

    #[test]
    fn route_status_is_terminal_covers_documented_terminal_states() {
        for sticky in ["handled", "cancelled", "failed", "completed"] {
            assert!(
                route_status_is_terminal(sticky),
                "{sticky} must be terminal"
            );
        }
        for non_sticky in ["pending", "leased", "review_rework", "blocked", ""] {
            assert!(
                !route_status_is_terminal(non_sticky),
                "{non_sticky} must NOT be terminal"
            );
        }
    }

    #[test]
    fn record_terminal_no_send_verdict_is_persistent_and_idempotent() -> Result<()> {
        let root = unique_root("ctox-no-send-verdict");
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path)?;
        let key = "email:cto1@metric-space.ai::ooo-1";
        upsert_test_inbound(
            &mut conn,
            key,
            json!({"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}),
        )?;
        drop(conn);

        record_terminal_no_send_verdict(&root, key, "test", "first NO-SEND: auto-reply")?;
        assert!(inbound_message_has_terminal_no_send(&root, key)?);

        // Re-recording must be idempotent and must not flip the flag.
        record_terminal_no_send_verdict(&root, key, "test", "second NO-SEND record (idempotent)")?;
        assert!(inbound_message_has_terminal_no_send(&root, key)?);

        // A different inbound key has no verdict.
        assert!(!inbound_message_has_terminal_no_send(
            &root,
            "email:cto1@metric-space.ai::other"
        )?);

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn inbound_message_is_auto_submitted_reads_persisted_metadata() -> Result<()> {
        let root = unique_root("ctox-auto-submitted-metadata");
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path)?;
        let auto_key = "email:cto1@metric-space.ai::ooo-2";
        let human_key = "email:cto1@metric-space.ai::human-1";
        upsert_test_inbound(
            &mut conn,
            auto_key,
            json!({"autoSubmitted": true, "autoSubmittedValue": "auto-replied"}),
        )?;
        upsert_test_inbound(&mut conn, human_key, json!({"autoSubmitted": false}))?;
        drop(conn);
        assert!(inbound_message_is_auto_submitted(&root, auto_key)?);
        assert!(!inbound_message_is_auto_submitted(&root, human_key)?);
        assert!(!inbound_message_is_auto_submitted(
            &root,
            "email:does-not-exist"
        )?);
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn message_metadata_marks_auto_submitted_low_level_helper_works_off_db() -> Result<()> {
        // Tests the low-level conn-bound helper used by the
        // founder-handled-ack guard. It must consult only the
        // structured metadata field, never subject/body strings.
        let root = unique_root("ctox-meta-marks-auto-submitted");
        fs::create_dir_all(root.join("runtime"))?;
        let db_path = resolve_db_path(&root, None);
        let mut conn = open_channel_db(&db_path)?;
        let auto_key = "email:cto1@metric-space.ai::ooo-low";
        let human_key = "email:cto1@metric-space.ai::human-low";
        upsert_test_inbound(&mut conn, auto_key, json!({"autoSubmitted": true}))?;
        upsert_test_inbound(&mut conn, human_key, json!({}))?;
        assert!(message_metadata_marks_auto_submitted(&conn, auto_key)?);
        assert!(!message_metadata_marks_auto_submitted(&conn, human_key)?);

        // And the no-send-flag helper reads only the
        // terminal_no_send column, not the review_summary string.
        record_terminal_no_send_verdict(&root, auto_key, "test", "auto-replied / NO-SEND")?;
        let conn = open_channel_db(&db_path)?;
        assert!(message_has_terminal_no_send_in_conn(&conn, auto_key)?);
        assert!(!message_has_terminal_no_send_in_conn(&conn, human_key)?);

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // RFC 0001 Phase 1 — body durability before Sending + SendFailed transition.
    //
    // These tests cover the helper functions added to harden the
    // `send_reviewed_founder_*` paths against provider failure. Helper-level
    // tests are used because the full `send_reviewed_founder_*` paths require
    // account configs, identity profiles, settings, and a live email adapter
    // — wiring all of that for an injected mock would be a larger refactor
    // than Phase 1 is scoped for. The helpers are the load-bearing surface:
    // if they round-trip correctly, the wiring in `send_reviewed_founder_*`
    // (a `match` over `send_email_message` with the same helper calls) is a
    // small, locally-auditable change.
    // -----------------------------------------------------------------------

    fn phase1_test_request(body: &str) -> ChannelSendRequest {
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "<phase1-thread@example.com>".to_string(),
            body: body.to_string(),
            subject: "Vorschlag Tag-System fuer Lead-Funnel".to_string(),
            to: vec!["j.kienzler@remcapital.de".to_string()],
            cc: vec![
                "j.cakmak@remcapital.de".to_string(),
                "d.lottes@remcapital.de".to_string(),
            ],
            attachments: Vec::new(),
            sender_display: None,
            sender_address: None,
            send_voice: false,
            reviewed_founder_send: true,
        }
    }

    #[test]
    fn phase1_record_outbound_pending_send_persists_body_with_draft_pending_send_status() {
        let db_path = unique_test_db_path("ctox-phase1-pending-send");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request =
            phase1_test_request("Hallo Jill,\n\nVorschlag fuer Tag-System: ...\n\nGruesse, Yoda");
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());

        let message_key =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("pending send must persist");

        let (status, body_text, direction, subject, recipients_json, metadata_json): (
            String,
            String,
            String,
            String,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT status, body_text, direction, subject, recipient_addresses_json, metadata_json
                 FROM communication_messages WHERE message_key = ?1",
                params![message_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .expect("row must exist");
        assert_eq!(status, "draft_pending_send");
        assert_eq!(direction, "outbound");
        assert_eq!(body_text, request.body);
        assert_eq!(subject, request.subject);
        assert!(recipients_json.contains("j.kienzler@remcapital.de"));
        let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
        assert_eq!(
            metadata.get("approval_key").and_then(Value::as_str),
            Some("founder-review:phase1")
        );
        assert_eq!(
            metadata.get("body_sha256").and_then(Value::as_str),
            Some(body_sha256.as_str())
        );
        assert_eq!(
            metadata.get("pending_send").and_then(Value::as_bool),
            Some(true)
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn reviewed_founder_send_cli_path_requires_exact_unconsumed_review() {
        let root = unique_root("ctox-reviewed-send-cli-approval");
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let body = "Hallo Julia,\n\nhier ist der freigegebene Vorschlag.\n\nViele Gruesse";
        let action = FounderOutboundAction {
            account_key: "email:cto1@metric-space.ai".to_string(),
            thread_key: "salesforce-tags".to_string(),
            subject: "Vorschlag Tag-System fuer Lead-Funnel".to_string(),
            to: vec!["j.kienzler@remcapital.de".to_string()],
            cc: vec!["j.cakmak@remcapital.de".to_string()],
            attachments: Vec::new(),
        };

        record_founder_outbound_review_approval(
            &root,
            "tui-outbound:test",
            &action,
            body,
            "PASS: send-ready",
        )
        .expect("approval should persist");
        let conn = open_channel_db(&resolve_db_path(&root, None)).expect("failed to open db");

        let (approval_key, anchor) =
            require_any_unconsumed_founder_outbound_review(&conn, &action, body)
                .expect("exact reviewed send should find approval");
        assert!(approval_key.starts_with("founder-outbound-review:tui-outbound:test:"));
        assert_eq!(anchor, "tui-outbound:test");

        let err = require_any_unconsumed_founder_outbound_review(
            &conn,
            &action,
            "Hallo Julia,\n\nleicht geaenderter Text.",
        )
        .expect_err("changed body must not match review approval");
        assert!(
            err.to_string()
                .contains("no matching unconsumed review approval"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn phase1_pending_send_message_key_is_stable_for_same_request() {
        let request = phase1_test_request("Konsistente Anfrage");
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());
        let key_a = pending_send_message_key(&request, &body_sha256);
        let key_b = pending_send_message_key(&request, &body_sha256);
        assert_eq!(
            key_a, key_b,
            "same request inputs must yield the same message_key — retry binding"
        );

        let mut request_changed = phase1_test_request("Konsistente Anfrage");
        request_changed.body = "Andere Nachricht".to_string();
        let other_sha = sha256_hex(request_changed.body.trim().as_bytes());
        let key_c = pending_send_message_key(&request_changed, &other_sha);
        assert_ne!(
            key_a, key_c,
            "different body must yield a different durable message_key"
        );
    }

    #[test]
    fn phase1_update_pending_send_to_accepted_flips_status_and_records_adapter_result() {
        let db_path = unique_test_db_path("ctox-phase1-accepted");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = phase1_test_request("Body fuer Erfolg");
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());
        let message_key =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("pending send must persist");

        update_pending_send_to_accepted(
            &conn,
            &message_key,
            &json!({
                "ok": true,
                "channel": "email",
                "status": "accepted",
                "remote_id": "smtp-msg-123",
            }),
        )
        .expect("accepted update must succeed");

        let (status, metadata_json): (String, String) = conn
            .query_row(
                "SELECT status, metadata_json FROM communication_messages WHERE message_key = ?1",
                params![message_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("row must exist");
        assert_eq!(status, "accepted");
        let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
        assert_eq!(
            metadata.get("pending_send").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            metadata.get("transitioned_to").and_then(Value::as_str),
            Some("accepted")
        );
        assert_eq!(
            metadata
                .get("adapter_result")
                .and_then(|value| value.get("remote_id"))
                .and_then(Value::as_str),
            Some("smtp-msg-123")
        );

        // Idempotence guard: a second accepted-update on a non-pending row
        // must error rather than silently overwrite.
        let err = update_pending_send_to_accepted(&conn, &message_key, &json!({}))
            .expect_err("second accepted-update must fail because row is no longer pending");
        assert!(err.to_string().contains("not in draft_pending_send"));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn terminal_founder_outbound_artifact_count_requires_terminal_send_row() {
        let root = unique_root("ctox-terminal-outbound-artifact-count");
        fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        let db_path = resolve_db_path(&root, None);
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = phase1_test_request("Body fuer Outcome-Gate");
        let action = FounderOutboundAction {
            account_key: request.account_key.to_ascii_uppercase(),
            thread_key: request.thread_key.clone(),
            subject: request.subject.clone(),
            to: request.to.clone(),
            cc: request.cc.clone(),
            attachments: request.attachments.clone(),
        };
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());
        let message_key =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("pending send must persist");

        assert_eq!(
            terminal_founder_outbound_artifact_count(&root, &action)
                .expect("count pending artifact"),
            0,
            "draft_pending_send is not a delivered outcome"
        );

        update_pending_send_to_accepted(
            &conn,
            &message_key,
            &json!({
                "ok": true,
                "channel": "email",
                "status": "accepted",
            }),
        )
        .expect("accepted update must succeed");

        assert_eq!(
            terminal_founder_outbound_artifact_count(&root, &action)
                .expect("count accepted artifact"),
            1
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn phase1_update_pending_send_to_failed_preserves_body_and_records_provider_error() {
        let db_path = unique_test_db_path("ctox-phase1-failed");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = phase1_test_request(
            "Body fuer Provider-Fehler-Pfad: muss nach dem Failure noch da sein.",
        );
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());
        let message_key =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("pending send must persist");

        update_pending_send_to_failed(
            &conn,
            &message_key,
            "smtp authentication failed: 535 5.7.0 outdated endpoint",
        )
        .expect("send-failed update must succeed");

        let (status, body_text, metadata_json): (String, String, String) = conn
            .query_row(
                "SELECT status, body_text, metadata_json FROM communication_messages WHERE message_key = ?1",
                params![message_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("row must exist");
        assert_eq!(status, "send_failed");
        assert_eq!(
            body_text, request.body,
            "body must survive provider failure for retry"
        );
        let metadata: Value = serde_json::from_str(&metadata_json).expect("valid json");
        assert_eq!(
            metadata.get("transitioned_to").and_then(Value::as_str),
            Some("send_failed")
        );
        assert!(metadata
            .get("provider_error")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("smtp authentication failed"));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn phase1_emit_send_failed_transition_records_kernel_proof() {
        let db_path = unique_test_db_path("ctox-phase1-sendfailed-kernel");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = phase1_test_request("Body fuer Kernel-Transition");
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());
        let pending_message_key =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("pending send must persist");

        // First the Approved → Sending proof (precondition for SendFailed):
        enforce_reviewed_founder_send_core_transition(
            &conn,
            "founder-outbound:phase1-anchor",
            "founder-review:phase1",
            &request,
        )
        .expect("Approved->Sending must be accepted");

        // Now the failure path:
        emit_reviewed_founder_send_failed_transition(
            &conn,
            "founder-outbound:phase1-anchor",
            "founder-review:phase1",
            &request,
            &pending_message_key,
            "smtp 535 outdated endpoint",
        )
        .expect("Sending->SendFailed must be accepted by kernel");

        let send_failed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_core_transition_proofs
                 WHERE entity_type = 'FounderCommunication'
                   AND lane = 'P0FounderCommunication'
                   AND from_state = 'Sending'
                   AND to_state = 'SendFailed'
                   AND core_event = 'Fail'
                   AND accepted = 1",
                [],
                |row| row.get(0),
            )
            .expect("kernel proof query must run");
        assert_eq!(
            send_failed, 1,
            "the Sending->SendFailed transition must be witnessed by the kernel"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn phase1_record_outbound_pending_send_is_idempotent_for_retry() {
        let db_path = unique_test_db_path("ctox-phase1-retry-idempotent");
        let conn = open_channel_db(&db_path).expect("failed to open db");
        let request = phase1_test_request("Wiederholung");
        let body_sha256 = sha256_hex(request.body.trim().as_bytes());

        let key_first =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("first persist");
        let key_second =
            record_outbound_pending_send(&conn, &request, "founder-review:phase1", &body_sha256)
                .expect("second persist (retry-style) must not crash");
        assert_eq!(
            key_first, key_second,
            "retrying record_outbound_pending_send must yield the same key (idempotent upsert)"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1",
                params![key_first],
                |row| row.get(0),
            )
            .expect("count query");
        assert_eq!(
            count, 1,
            "exactly one durable row must exist for the retry-bound key"
        );

        let _ = std::fs::remove_file(&db_path);
    }
}
