// Reviewed outbound sending: founder deliverables, the durable pending-
// send state machine, review approvals/digests, core transitions on
// send success/failure, and the Jami setup PDF artifact. The P8a map
// lists outbound_send and review_approvals separately; they are moved
// together because the code interleaves them — the semantic wave may
// split further.

use super::{
    classify_email_sender, collect_flag_values, communication_adapters, communication_gateway,
    email_address_from_account_key, enforce_core_transition, ensure_routing_rows_for_inbound,
    ensure_routing_state_hardening_columns, ensure_terminal_no_send_column, find_flag_value,
    has_flag, jami_address_from_account_key, load_account_config, map_channel_message_row,
    metadata_marks_auto_submitted, non_negative_i64_to_usize, normalize_email_address,
    now_iso_string, open_channel_db, parse_string_json_array, preview_text,
    record_harness_flow_event_lossy, required_flag_value, resolve_account_key, resolve_db_path,
    runtime_settings_with_owner_profiles, send_message, stable_digest, sync_prompt_identity,
    ChannelFileChangeStamp, ChannelMessageView, ChannelRoutingCacheStamp, ChannelSchemaCacheKey,
    ChannelSendRequest, CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState,
    CoreTransitionRequest, EmailSenderPolicy, ExternalChatAction, FounderOutboundAction,
    FounderReplyAction, MessageAddressing, QrColor, QueueTaskCountCacheEntry,
    QueueTaskCountCacheKey, QueueTaskListCacheEntry, QueueTaskListCacheKey,
    QueueTaskListCacheStamp, QueueTaskView, RecordHarnessFlowEventRequest, RuntimeLane,
    TuiIngestRequest, CHANNEL_OPEN_ROUTING_READY, CHANNEL_SCHEMA_READY, QUEUE_TASK_COUNT_CACHE,
    QUEUE_TASK_COUNT_CACHE_MAX_ENTRIES, QUEUE_TASK_LIST_CACHE, QUEUE_TASK_LIST_CACHE_MAX_ENTRIES,
    REVIEWED_FOUNDER_SEND_LOCK,
};
#[cfg(test)]
use super::{
    CHANNEL_DB_OPEN_CALL_COUNTS, CHANNEL_OPEN_ROUTING_ENSURE_COUNTS, CHANNEL_SCHEMA_ENSURE_COUNTS,
    QUEUE_TASK_COUNT_CACHE_MISS_COUNTS, QUEUE_TASK_LIST_CACHE_MISS_COUNTS,
};
use anyhow::{anyhow, bail, Context, Result};
use qrcode::QrCode;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

thread_local! {
    static CHANNEL_DB_READ_ONLY: RefCell<BTreeMap<ChannelSchemaCacheKey, Connection>> = const { RefCell::new(BTreeMap::new()) };
    #[cfg(test)]
    static CHANNEL_DB_READ_ONLY_OPEN_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

const CHANNEL_DB_READ_ONLY_MAX_ENTRIES: usize = 8;

pub(super) fn is_review_required_outbound_channel(channel: &str) -> bool {
    matches!(
        channel,
        "email"
            | "teams"
            | "jami"
            | "whatsapp"
            | "meeting"
            | "slack"
            | "discord"
            | "telegram"
            | "matrix"
            | "mattermost"
            | "zulip"
            | "google_chat"
    )
}

pub(super) fn enforce_external_chat_send_is_reviewed(request: &ChannelSendRequest) -> Result<()> {
    if is_review_required_outbound_channel(&request.channel) && !request.reviewed_founder_send {
        anyhow::bail!(
            "outbound {} communication must pass communication review before sending. Draft the response for completion review first; after approval the Harness sends the exact approved body through the reviewed send path.",
            request.channel
        );
    }
    Ok(())
}

pub(super) fn enforce_external_work_ack_has_pipeline_backing(
    conn: &Connection,
    request: &ChannelSendRequest,
) -> Result<()> {
    if !matches!(
        request.channel.as_str(),
        "teams"
            | "jami"
            | "whatsapp"
            | "meeting"
            | "slack"
            | "discord"
            | "telegram"
            | "matrix"
            | "mattermost"
            | "zulip"
            | "google_chat"
    ) {
        return Ok(());
    }
    if !body_promises_follow_up_work(&request.body) {
        return Ok(());
    }
    if thread_has_open_work_backing(conn, &request.thread_key)? {
        if request.reviewed_founder_send {
            return Ok(());
        }
        anyhow::bail!(
            "outbound {} acknowledgement promises follow-up work and has pipeline backing, but it has not passed communication review. Draft the quick response for review first; after approval the Harness sends the exact approved body.",
            request.channel
        );
    }
    anyhow::bail!(
        "outbound {} acknowledgement promises follow-up work but no durable queue item, plan, or internal work item exists for thread `{}`. Create the pipeline item first, then send the acknowledgement.",
        request.channel,
        request.thread_key
    )
}

pub(super) fn body_promises_follow_up_work(body: &str) -> bool {
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

pub(super) fn thread_has_open_work_backing(conn: &Connection, thread_key: &str) -> Result<bool> {
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

pub(super) fn open_queue_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
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

pub(super) fn open_plan_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
    let terminal = crate::mission::plan_status::PlanGoalStatus::terminal_read_values();
    let placeholders = (2..terminal.len() + 2)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT COUNT(*)
        FROM planned_goals
        WHERE thread_key = ?1
          AND status NOT IN ({placeholders})
        "#
    );
    let query_params = std::iter::once(thread_key).chain(terminal.iter().copied());
    let count: i64 = conn.query_row(&sql, rusqlite::params_from_iter(query_params), |row| {
        row.get(0)
    })?;
    Ok(count > 0)
}

pub(super) fn open_self_work_backing_exists(conn: &Connection, thread_key: &str) -> Result<bool> {
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

pub(super) fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        params![table_name],
        |_| Ok(true),
    )
    .optional()
    .map(|value| value.unwrap_or(false))
    .map_err(anyhow::Error::from)
}

pub(super) fn load_message_from_conn(
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
            r.last_error,
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

pub(super) fn load_message_addressing_from_conn(
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

pub(super) fn normalize_email_list(values: impl IntoIterator<Item = String>) -> Vec<String> {
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

pub(super) fn normalize_deliverable_text(value: &str) -> String {
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

pub(super) fn text_mentions_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

pub(super) fn detect_required_founder_deliverables(subject: &str, body: &str) -> Vec<String> {
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

pub(super) fn attachments_satisfy_deliverable(attachments: &[String], deliverable: &str) -> bool {
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

pub(super) fn founder_reply_satisfies_deliverable(
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

pub(super) fn prepare_founder_reply_attachments(
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

pub(super) fn generate_jami_setup_pdf_artifact(root: &Path) -> Result<String> {
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

pub(super) fn build_simple_jami_setup_pdf(profile_name: &str, share_uri: &str) -> Result<Vec<u8>> {
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

pub(super) fn pdf_text(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    format!("({escaped})")
}

pub(super) fn derives_targets_from_forward(subject: &str, body: &str) -> bool {
    let lowered_subject = subject.to_ascii_lowercase();
    if lowered_subject.starts_with("fwd:") || lowered_subject.starts_with("fw:") {
        return true;
    }
    let lowered_body = body.to_ascii_lowercase();
    lowered_body.contains("weitergeleiteten nachricht")
        || lowered_body.contains("begin forwarded message")
        || lowered_body.contains("forwarded message")
}

pub(super) fn derive_founder_reply_recipients(
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

pub(super) fn protected_recipient_policies(
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

pub(super) fn ensure_founder_outbound_body_clean(request: &ChannelSendRequest) -> Result<()> {
    ensure_founder_outbound_body_text_clean(&request.body)
}

pub(crate) fn ensure_founder_outbound_body_text_clean(body: &str) -> Result<()> {
    let lowered = body.to_ascii_lowercase();
    let first_lines = body
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

pub(super) fn send_email_message(
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
    // EGRESS-2: a crash between the provider send (adapter.send_cli) and the
    // accepted-mark strands a draft_pending_send row carrying a
    // send_attempt_started_at marker. Refuse to blind-resend that founder email
    // — the provider may already have delivered it — and require operator
    // verification instead of silently duplicating it. This runs BEFORE
    // record_outbound_pending_send, whose ON CONFLICT would overwrite the
    // marker on the stranded row.
    let stranded_message_key = pending_send_message_key(request, &body_sha256);
    if let Some(attempt_started_at) = stranded_outbound_send_attempt(conn, &stranded_message_key)? {
        anyhow::bail!(
            "refusing to re-send founder email {stranded_message_key}: a provider send was \
             initiated at {attempt_started_at} but never confirmed accepted (possible crash \
             between send and acknowledgement); verify delivery before resending"
        );
    }
    let pending_send = record_outbound_pending_send(conn, request, approval_key, &body_sha256)?;
    let pending_message_key = pending_send.message_key;
    if let Some(existing) = pending_send.existing_result {
        return Ok(json!({
            "ok": true,
            "channel": "email",
            "db_path": db_path,
            "message_key": pending_message_key,
            "status": existing
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("accepted"),
            "delivery_confirmed": existing
                .get("adapter_result")
                .or_else(|| existing.get("adapterResult"))
                .and_then(|value| value.get("delivery"))
                .and_then(|value| value.get("confirmed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "adapter_result": existing
                .get("adapter_result")
                .or_else(|| existing.get("adapterResult"))
                .cloned()
                .unwrap_or_else(|| json!({ "deduplicated": true })),
            "deduplicated": true,
        }));
    }
    // EGRESS-2: record that the provider call is about to happen BEFORE the
    // physical send, so a crash after send_cli but before the accepted-mark is
    // recoverable as "maybe sent" (stranded_outbound_send_attempt) rather than
    // an unconditional resend on the next attempt.
    mark_outbound_send_attempt_started(conn, &pending_message_key)?;
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
    if let Some(context) = reviewed_context {
        // The kernel must witness send SUCCESS too, symmetric to the failure
        // twin above, so a reviewed founder send reaches terminal Sent instead
        // of being stranded in non-terminal Sending. Best-effort: an
        // already-delivered mail must not be failed by a witness hiccup.
        let _ = enforce_reviewed_founder_send_succeeded_core_transition(
            conn,
            context.entity_id,
            context.approval_key,
            request,
            &pending_message_key,
        );
    }
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
pub(super) struct ReviewedFounderSendContext<'a> {
    entity_id: &'a str,
    approval_key: &'a str,
}

pub(super) fn record_outbound_pending_send(
    conn: &Connection,
    request: &ChannelSendRequest,
    approval_key: &str,
    body_sha256: &str,
) -> Result<PendingSendReservation> {
    let observed_at = now_iso_string();
    let message_key = pending_send_message_key(request, body_sha256);
    if let Some(existing) = existing_durable_outbound_send_result(conn, &message_key)? {
        return Ok(PendingSendReservation {
            message_key,
            existing_result: Some(existing),
        });
    }
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
        WHERE communication_messages.status IN ('draft_pending_send', 'send_failed')
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
    if let Some(existing) = existing_durable_outbound_send_result(conn, &message_key)? {
        return Ok(PendingSendReservation {
            message_key,
            existing_result: Some(existing),
        });
    }
    Ok(PendingSendReservation {
        message_key,
        existing_result: None,
    })
}

#[derive(Debug)]
pub(super) struct PendingSendReservation {
    pub(super) message_key: String,
    pub(super) existing_result: Option<Value>,
}

pub(super) fn existing_durable_outbound_send_result(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<Value>> {
    let existing = conn
        .query_row(
            r#"
            SELECT status, folder_hint, metadata_json
            FROM communication_messages
            WHERE message_key = ?1
              AND channel = 'email'
              AND direction = 'outbound'
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
        .optional()?;
    let Some((status, folder_hint, metadata_json)) = existing else {
        return Ok(None);
    };
    let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
    if !is_durable_outbound_send_state(&status, &folder_hint, &metadata) {
        return Ok(None);
    }
    Ok(Some(json!({
        "status": status,
        "folder_hint": folder_hint,
        "adapter_result": metadata
            .get("adapterResult")
            .or_else(|| metadata.get("adapter_result"))
            .cloned()
            .unwrap_or_else(|| json!({})),
    })))
}

pub(super) fn is_durable_outbound_send_state(
    status: &str,
    folder_hint: &str,
    metadata: &Value,
) -> bool {
    if !folder_hint.eq_ignore_ascii_case("sent") {
        return false;
    }
    if matches!(
        status,
        "draft_pending_send" | "send_failed" | "failed" | "cancelled"
    ) {
        return false;
    }
    !metadata
        .get("pendingSend")
        .or_else(|| metadata.get("pending_send"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether an outbound row's provider send was already initiated but never
/// confirmed accepted — i.e. a not-yet-durable `draft_pending_send` row
/// carrying a `send_attempt_started_at` marker. Such a row is "maybe sent": a
/// process can crash after `adapter.send_cli` returns Ok but before
/// `mark_outbound_send_accepted` commits, and a blind resend would duplicate a
/// founder email. Returns the recorded attempt timestamp when stranded so the
/// caller can refuse the resend and require operator verification (EGRESS-2).
pub(super) fn stranded_outbound_send_attempt(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<String>> {
    let existing = conn
        .query_row(
            r#"
            SELECT status, folder_hint, metadata_json
            FROM communication_messages
            WHERE message_key = ?1
              AND channel = 'email'
              AND direction = 'outbound'
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
        .optional()?;
    let Some((status, folder_hint, metadata_json)) = existing else {
        return Ok(None);
    };
    let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
    // A durable (accepted) row is handled by existing_durable_outbound_send_result.
    if is_durable_outbound_send_state(&status, &folder_hint, &metadata) {
        return Ok(None);
    }
    // Only a still-pending row is "maybe sent". A send_failed row means
    // adapter.send_cli returned Err (the provider rejected it = NOT delivered),
    // so it is safe to retry and must never be treated as stranded — and
    // mark_outbound_send_failed already clears the marker, so this is also
    // defense-in-depth against a lingering marker on a non-pending row.
    if status != "draft_pending_send" {
        return Ok(None);
    }
    Ok(metadata
        .get("send_attempt_started_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
}

/// Stamp `send_attempt_started_at` into the row metadata immediately before
/// `adapter.send_cli`, without disturbing the rest of the metadata, so a crash
/// in the send→accept window leaves a recoverable "maybe sent" marker that
/// `stranded_outbound_send_attempt` detects (EGRESS-2).
pub(super) fn mark_outbound_send_attempt_started(
    conn: &Connection,
    message_key: &str,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE communication_messages
        SET metadata_json = json_set(metadata_json, '$.send_attempt_started_at', ?2)
        WHERE message_key = ?1
        "#,
        params![message_key, now_iso_string()],
    )?;
    Ok(())
}

pub(super) fn mark_outbound_send_accepted(
    conn: &Connection,
    message_key: &str,
    status: &str,
    adapter_json: &Value,
) -> Result<()> {
    // Only transition a row that is still pending (or retrying after a prior
    // failure); never clobber a row that has already reached a terminal state.
    // A 0-row result means the send was already resolved, so this is an
    // idempotent no-op — NOT an error: the caller uses `?` on the success path,
    // and erroring there could trigger a re-send of an already-accepted message.
    let changed = conn.execute(
        r#"
        UPDATE communication_messages
        SET status = ?2,
            folder_hint = 'sent',
            metadata_json = json_set(
                json_remove(
                    json_set(metadata_json, '$.pendingSend', false),
                    '$.send_attempt_started_at'
                ),
                '$.adapterResult',
                json(?3)
            ),
            observed_at = ?4
        WHERE message_key = ?1
          AND status IN ('draft_pending_send', 'send_failed')
        "#,
        params![
            message_key,
            status,
            serde_json::to_string(adapter_json)?,
            now_iso_string()
        ],
    )?;
    if changed == 0 {
        eprintln!(
            "[ctox channels] mark_outbound_send_accepted: {message_key} was not in a pending state (already resolved); skipping idempotently"
        );
    }
    Ok(())
}

pub(super) fn mark_outbound_send_failed(
    conn: &Connection,
    message_key: &str,
    error: &str,
) -> Result<()> {
    // Only mark a still-pending row as failed; a late or duplicate failure must
    // never clobber a row that has already been accepted (or cancelled). A
    // 0-row result is an idempotent no-op so a stray failure callback cannot
    // override a successful send.
    let changed = conn.execute(
        r#"
        UPDATE communication_messages
        SET status = 'send_failed',
            metadata_json = json_set(
                json_remove(
                    json_set(metadata_json, '$.pendingSend', false),
                    '$.send_attempt_started_at'
                ),
                '$.sendError',
                ?2
            ),
            observed_at = ?3
        WHERE message_key = ?1
          AND status IN ('draft_pending_send', 'send_failed')
        "#,
        params![message_key, error, now_iso_string()],
    )?;
    if changed == 0 {
        eprintln!(
            "[ctox channels] mark_outbound_send_failed: {message_key} was not in a pending state (already resolved); skipping idempotently"
        );
    }
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
        account_key: request.account_key,
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

pub(super) fn founder_reply_review_digest(
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

pub(super) fn founder_outbound_review_digest(
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

pub(super) fn external_chat_review_digest(
    action: &ExternalChatAction,
    body: &str,
) -> (String, String, String) {
    let review_kind = if action.channel.eq_ignore_ascii_case("email") {
        "reviewed_outbound_email"
    } else {
        "external_chat_quick_response"
    };
    let action_json = json!({
        "kind": review_kind,
        "channel": &action.channel,
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

pub(crate) fn is_reviewed_external_chat_channel(channel: &str) -> bool {
    matches!(
        channel,
        "teams"
            | "jami"
            | "whatsapp"
            | "meeting"
            | "slack"
            | "discord"
            | "telegram"
            | "matrix"
            | "mattermost"
            | "zulip"
            | "google_chat"
    )
}

pub(super) fn external_chat_action_from_send_request(
    request: &ChannelSendRequest,
) -> ExternalChatAction {
    ExternalChatAction {
        channel: request.channel.clone(),
        account_key: request.account_key.clone(),
        thread_key: request.thread_key.clone(),
        subject: request.subject.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        attachments: request.attachments.clone(),
    }
}

pub(crate) fn prepare_reviewed_external_chat_reply(
    root: &Path,
    inbound_message_key: &str,
) -> Result<Option<ExternalChatAction>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let Some(inbound) = load_message_from_conn(&conn, inbound_message_key)? else {
        return Ok(None);
    };
    if !is_reviewed_external_chat_channel(&inbound.channel) || inbound.direction != "inbound" {
        return Ok(None);
    }
    let to = if inbound.channel == "jami" && !inbound.sender_address.trim().is_empty() {
        vec![inbound.sender_address.trim().to_string()]
    } else {
        Vec::new()
    };
    Ok(Some(ExternalChatAction {
        channel: inbound.channel,
        account_key: inbound.account_key,
        thread_key: inbound.thread_key,
        subject: inbound.subject,
        to,
        cc: Vec::new(),
        attachments: Vec::new(),
    }))
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
          AND status IN ('accepted', 'sent', 'queued', 'queued_in_mailserver', 'queued_for_provider')
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

pub(crate) fn reviewed_send_result_has_durable_outbound_artifact(
    root: &Path,
    send_result: &Value,
) -> Result<bool> {
    let Some(message_key) = send_result
        .get("message_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(false);
    };
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages
        WHERE message_key = ?1
          AND channel = 'email'
          AND direction = 'outbound'
          AND lower(COALESCE(folder_hint, '')) = 'sent'
          AND status NOT IN ('draft_pending_send', 'send_failed', 'failed', 'cancelled')
          AND COALESCE(
                json_extract(metadata_json, '$.pendingSend'),
                json_extract(metadata_json, '$.pending_send'),
                0
              ) = 0
        "#,
        params![message_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
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

pub(crate) fn record_external_chat_review_approval(
    root: &Path,
    anchor_message_key: &str,
    action: &ExternalChatAction,
    body: &str,
    review_summary: &str,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let (action_digest, action_json, body_sha256) = external_chat_review_digest(action, body);
    let approval_prefix = if action.channel.eq_ignore_ascii_case("email") {
        "communication-email-review"
    } else {
        "external-chat-review"
    };
    let approval_key = format!("{approval_prefix}:{anchor_message_key}:{action_digest}");
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
    .context("failed to record communication review approval")?;
    let email_review = action.channel.eq_ignore_ascii_case("email");
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "review.approved",
            title: if email_review {
                "Email communication review approved"
            } else {
                "External chat review approved"
            },
            body_text: review_summary,
            message_key: Some(anchor_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "body_sha256": body_sha256,
                "action_digest": action_digest,
                "channel": &action.channel,
                "communication_review": true,
                "email": email_review,
                "external_chat": !email_review,
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

pub(super) fn require_unconsumed_founder_reply_review(
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

pub(super) fn require_any_unconsumed_founder_outbound_review(
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

pub(super) fn require_any_unconsumed_external_chat_review(
    conn: &Connection,
    action: &ExternalChatAction,
    body: &str,
) -> Result<(String, String)> {
    let (action_digest, _, _) = external_chat_review_digest(action, body);
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
        .context("failed to load communication review approval")?;
    approval.with_context(|| {
        "reviewed outbound communication has no matching unconsumed review approval for the exact body, channel, thread, recipients, subject, and attachments. Run completion review first; after approval the Harness sends exactly the approved body."
            .to_string()
    })
}

pub(super) fn mark_founder_reply_review_sent(
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

pub(super) fn founder_reply_sent_after_review(
    conn: &Connection,
    inbound_message_key: &str,
) -> Result<bool> {
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

pub(super) fn protected_founder_inbound_message(
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

pub(super) fn message_metadata_marks_auto_submitted(
    conn: &Connection,
    message_key: &str,
) -> Result<bool> {
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

pub(super) fn message_has_terminal_no_send_in_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<bool> {
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

pub(super) fn guard_founder_handled_ack(
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

pub(super) fn send_reviewed_founder_outbound_request(
    root: &Path,
    conn: &Connection,
    db_path: &Path,
    request: &ChannelSendRequest,
) -> Result<Value> {
    let _send_guard = acquire_reviewed_founder_send_lock()?;
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

pub(crate) fn send_reviewed_founder_outbound_action(
    root: &Path,
    action: &FounderOutboundAction,
    body: &str,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let request = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: action.account_key.clone(),
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
    send_reviewed_founder_outbound_request(root, &conn, &db_path, &request)
}

pub(crate) fn send_reviewed_external_chat_action(
    root: &Path,
    action: &ExternalChatAction,
    body: &str,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    send_message(
        root,
        &db_path,
        ChannelSendRequest {
            channel: action.channel.clone(),
            account_key: action.account_key.clone(),
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
    )
}

/// Deterministic policy escalation for a founder/owner inbound email whose
/// finite completion-review budget is exhausted: record a policy-authored
/// approval for the exact escalation body and send it through the same gated
/// reviewed-send sequence as `send_reviewed_founder_reply` (send lock,
/// protected-recipient check, exact-digest approval match, body-clean gate,
/// core transition, durable send artifact). The deliverables-presence gate is
/// intentionally not applied: the escalation exists precisely because the
/// requested deliverable could not be produced, and it must still reach the
/// founder instead of the thread ending silently.
pub(crate) fn record_and_send_founder_escalation_reply(
    root: &Path,
    inbound_message_key: &str,
    body: &str,
    review_summary: &str,
) -> Result<Value> {
    let _send_guard = acquire_reviewed_founder_send_lock()?;
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let inbound = load_message_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing inbound communication message {inbound_message_key}"))?;
    anyhow::ensure!(
        inbound.channel == "email" && inbound.direction == "inbound",
        "founder escalation reply requires an inbound email message"
    );
    let addressing = load_message_addressing_from_conn(&conn, inbound_message_key)?
        .with_context(|| format!("missing communication addressing for {inbound_message_key}"))?;
    let (to, cc) = derive_founder_reply_recipients(&inbound, &addressing);
    let request = resolve_outbound_subject(
        &conn,
        ChannelSendRequest {
            channel: "email".to_string(),
            account_key: inbound.account_key.clone(),
            thread_key: inbound.thread_key.clone(),
            body: body.trim().to_string(),
            subject: format!("Re: {}", inbound.subject.trim()),
            to,
            cc,
            attachments: Vec::new(),
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
        "founder escalation reply requires founder/owner/admin recipient"
    );
    let action = FounderReplyAction {
        account_key: request.account_key.clone(),
        thread_key: request.thread_key.clone(),
        subject: request.subject.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        attachments: Vec::new(),
    };
    let (action_digest, action_json, body_sha256) =
        founder_reply_review_digest(&action, &request.body);
    let approval_key = format!("founder-escalation:{inbound_message_key}:{action_digest}");
    // A consumed escalation approval (sent_at set) must stay consumed: the
    // conflict arm deliberately does not reset sent_at, so a retry after a
    // successful send fails the unconsumed-approval lookup below instead of
    // double-sending the notice.
    conn.execute(
        r#"
        INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json,
            body_sha256, reviewer, review_summary, approved_at, sent_at, send_result_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'policy-escalation', ?6, ?7, NULL, '{}')
        ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET
            review_summary=excluded.review_summary
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
    .context("failed to record founder escalation approval")?;
    let approval_key = require_unconsumed_founder_reply_review(
        &conn,
        inbound_message_key,
        &action,
        &request.body,
    )?;
    ensure_founder_outbound_body_clean(&request)?;
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
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "communication.escalated",
            title: "Founder communication escalated after exhausted rework budget",
            body_text: review_summary,
            message_key: Some(inbound_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "body_sha256": body_sha256,
                "action_digest": action_digest,
                "escalation": true,
            }),
        },
    );
    Ok(send_result)
}

/// Chat-channel counterpart of `record_and_send_founder_escalation_reply`:
/// record a policy-authored approval for the exact escalation body against
/// the stalled inbound chat message and deliver it through the reviewed
/// external-chat send path (exact-digest approval match plus core send
/// transition inside `send_message`).
pub(crate) fn record_and_send_external_chat_escalation_reply(
    root: &Path,
    inbound_message_key: &str,
    body: &str,
    review_summary: &str,
) -> Result<Value> {
    let action =
        prepare_reviewed_external_chat_reply(root, inbound_message_key)?.with_context(|| {
            format!("inbound {inbound_message_key} is not a reviewed external chat message")
        })?;
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let trimmed_body = body.trim();
    let (action_digest, action_json, body_sha256) =
        external_chat_review_digest(&action, trimmed_body);
    let approval_key = format!("external-chat-escalation:{inbound_message_key}:{action_digest}");
    conn.execute(
        r#"
        INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json,
            body_sha256, reviewer, review_summary, approved_at, sent_at, send_result_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'policy-escalation', ?6, ?7, NULL, '{}')
        ON CONFLICT(inbound_message_key, action_digest) DO UPDATE SET
            review_summary=excluded.review_summary
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
    .context("failed to record external chat escalation approval")?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "communication.escalated",
            title: "External chat communication escalated after exhausted rework budget",
            body_text: review_summary,
            message_key: Some(inbound_message_key),
            work_id: None,
            ticket_key: None,
            attempt_index: Some(1),
            metadata: json!({
                "approval_key": approval_key,
                "body_sha256": body_sha256,
                "action_digest": action_digest,
                "channel": &action.channel,
                "escalation": true,
            }),
        },
    );
    drop(conn);
    send_reviewed_external_chat_action(root, &action, trimmed_body)
}

pub(super) fn send_reviewed_email_communication_request(
    root: &Path,
    conn: &Connection,
    db_path: &Path,
    request: &ChannelSendRequest,
    approval_key: &str,
) -> Result<Value> {
    ensure_founder_outbound_body_clean(request)?;
    let entity_id = format!(
        "reviewed-email:{}:{}",
        request.thread_key,
        stable_digest(approval_key)
    );
    enforce_reviewed_founder_send_core_transition(conn, &entity_id, approval_key, request)?;
    let send_result = send_email_message(
        root,
        conn,
        db_path,
        request,
        Some(ReviewedFounderSendContext {
            entity_id: &entity_id,
            approval_key,
        }),
    )?;
    mark_founder_reply_review_sent(conn, approval_key, &send_result)?;
    Ok(send_result)
}

pub(super) fn enforce_reviewed_communication_send_core_transition_if_approved(
    conn: &Connection,
    request: &ChannelSendRequest,
    approval: Option<&(String, String)>,
) -> Result<Option<String>> {
    let Some((approval_key, anchor_message_key)) = approval else {
        return Ok(None);
    };
    let entity_id = format!(
        "reviewed-communication:{}:{}",
        request.channel,
        stable_digest(&format!("{anchor_message_key}:{approval_key}"))
    );
    enforce_reviewed_founder_send_core_transition(conn, &entity_id, approval_key, request)?;
    Ok(Some(entity_id))
}

pub(super) fn enforce_reviewed_founder_send_core_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
) -> Result<()> {
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
            // EGRESS-3: approved hashes from the durable review record, outgoing
            // from the live request — the kernel require_reviewed_outbound gate is
            // now load-bearing instead of comparing the request against itself.
            evidence: reviewed_outbound_evidence(conn, approval_key, request),
            metadata,
        },
    )?;
    Ok(())
}

pub(super) fn acquire_reviewed_founder_send_lock() -> Result<MutexGuard<'static, ()>> {
    REVIEWED_FOUNDER_SEND_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|err| anyhow::anyhow!("reviewed founder send lock poisoned: {err}"))
}

/// Compute the deterministic `message_key` for a pending-send durable
/// outbound row. Stable for identical (account_key, thread_key, subject,
/// recipient set, body) tuples. This is the retry-binding key the
/// operator uses to resume after a provider failure (RFC 0001 §5.1).
pub(super) fn pending_send_message_key(request: &ChannelSendRequest, body_sha256: &str) -> String {
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
pub(super) fn update_pending_send_to_accepted(
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
pub(super) fn update_pending_send_to_failed(
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

pub(super) fn load_metadata_for_message(conn: &Connection, message_key: &str) -> Result<Value> {
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

pub(super) fn enforce_reviewed_founder_send_failed_core_transition(
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
pub(super) fn emit_reviewed_founder_send_failed_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
    pending_message_key: &str,
    provider_error: &str,
) -> Result<()> {
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
            // EGRESS-3: approved hashes from the durable review record, outgoing
            // from the live request — the ->Sent confirmation and the symmetric
            // failure record carry the same load-bearing evidence as the Send gate.
            evidence: reviewed_outbound_evidence(conn, approval_key, request),
            metadata,
        },
    )?;
    Ok(())
}

pub(super) fn enforce_reviewed_founder_send_succeeded_core_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
    pending_message_key: &str,
) -> Result<()> {
    emit_reviewed_founder_send_succeeded_transition(
        conn,
        entity_id,
        approval_key,
        request,
        pending_message_key,
    )
}

/// Emit the `Sending -> Sent` core transition after a successful provider
/// send. RFC 0001 Phase 1: the kernel must witness every founder-send outcome,
/// success symmetric to the failure twin, so the entity reaches a terminal
/// Sent state instead of being stranded in non-terminal Sending.
pub(super) fn emit_reviewed_founder_send_succeeded_transition(
    conn: &Connection,
    entity_id: &str,
    approval_key: &str,
    request: &ChannelSendRequest,
    pending_message_key: &str,
) -> Result<()> {
    let mut metadata = BTreeMap::new();
    metadata.insert("protected_party".to_string(), "founder".to_string());
    metadata.insert("thread_key".to_string(), request.thread_key.clone());
    metadata.insert("subject".to_string(), request.subject.clone());
    metadata.insert("account_key".to_string(), request.account_key.clone());
    metadata.insert(
        "pending_message_key".to_string(),
        pending_message_key.to_string(),
    );

    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: entity_id.to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Sending,
            to_state: CoreState::Sent,
            event: CoreEvent::ConfirmDelivery,
            actor: "ctox-reviewed-founder-send".to_string(),
            // EGRESS-3: approved hashes from the durable review record, outgoing
            // from the live request — the ->Sent confirmation and the symmetric
            // failure record carry the same load-bearing evidence as the Send gate.
            evidence: reviewed_outbound_evidence(conn, approval_key, request),
            metadata,
        },
    )?;
    Ok(())
}

pub(super) fn clip_error_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut clipped: String = text.chars().take(max).collect();
        clipped.push_str("...");
        clipped
    }
}

pub(super) fn founder_send_recipient_set_sha256(request: &ChannelSendRequest) -> String {
    recipient_set_sha256(
        &request.to,
        &request.cc,
        &request.subject,
        &request.attachments,
    )
}

/// EGRESS-3: the canonical recipient-set hash over (to, cc, subject,
/// attachments) with the exact normalization the founder-send gate uses — to/cc
/// trimmed + lowercased, attachments trimmed, all sorted, subject trimmed. Shared
/// by the live-request path (`founder_send_recipient_set_sha256`) and the
/// stored-approval path (`approved_outbound_evidence_hashes`) so the kernel
/// `require_reviewed_outbound` comparison is between two genuinely independent
/// values computed by IDENTICAL code — no normalization drift can false-reject a
/// legitimate send.
pub(super) fn recipient_set_sha256(
    to: &[String],
    cc: &[String],
    subject: &str,
    attachments: &[String],
) -> String {
    let mut to = to
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut cc = cc
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut attachments = attachments
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    to.sort();
    cc.sort();
    attachments.sort();
    let payload = json!({
        "to": to,
        "cc": cc,
        "subject": subject.trim(),
        "attachments": attachments,
    })
    .to_string();
    sha256_hex(payload.as_bytes())
}

/// EGRESS-3: load the APPROVED body + recipient-set hashes from the durable
/// review record by `approval_key`, so the kernel gate compares the stored
/// approval against the live request rather than the request against itself. The
/// recipient hash is derived from the stored `action_json` (to/cc/subject/
/// attachments — present in every review action shape) via the same
/// `recipient_set_sha256`. Returns `None` when no review row matches the key (or
/// its `action_json` cannot be parsed), so the caller can fall back to the
/// request-derived values and never NEWLY reject a previously-valid send.
pub(super) fn approved_outbound_evidence_hashes(
    conn: &Connection,
    approval_key: &str,
) -> Option<(String, String)> {
    let (body_sha256, action_json): (String, String) = conn
        .query_row(
            "SELECT body_sha256, action_json FROM communication_founder_reply_reviews \
             WHERE approval_key = ?1 LIMIT 1",
            params![approval_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .ok()
        .flatten()?;
    let action: Value = serde_json::from_str(&action_json).ok()?;
    let string_list = |key: &str| -> Vec<String> {
        action
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let to = string_list("to");
    let cc = string_list("cc");
    let attachments = string_list("attachments");
    let subject = action.get("subject").and_then(Value::as_str).unwrap_or("");
    let approved_recipient = recipient_set_sha256(&to, &cc, subject, &attachments);
    Some((body_sha256, approved_recipient))
}

/// EGRESS-3: build the `CoreEvidenceRefs` for a reviewed founder/owner-send
/// transition with the APPROVED hashes sourced from the durable review record
/// (independent of the live request) and the OUTGOING hashes from the request, so
/// every `require_reviewed_outbound`-gated transition (Approved->Sending,
/// Sending->Sent, plus the symmetric failure record) carries a genuinely
/// load-bearing comparison instead of a value-against-itself tautology. Falls
/// back to the request-derived values when the approval is not record-backed, so
/// a previously-valid send is never newly rejected.
pub(super) fn reviewed_outbound_evidence(
    conn: &Connection,
    approval_key: &str,
    request: &ChannelSendRequest,
) -> CoreEvidenceRefs {
    let outgoing_body_sha256 = sha256_hex(request.body.trim().as_bytes());
    let outgoing_recipient_set_sha256 = founder_send_recipient_set_sha256(request);
    let (approved_body_sha256, approved_recipient_set_sha256) =
        approved_outbound_evidence_hashes(conn, approval_key).unwrap_or_else(|| {
            (
                outgoing_body_sha256.clone(),
                outgoing_recipient_set_sha256.clone(),
            )
        });
    CoreEvidenceRefs {
        review_audit_key: Some(approval_key.to_string()),
        approved_body_sha256: Some(approved_body_sha256),
        outgoing_body_sha256: Some(outgoing_body_sha256),
        approved_recipient_set_sha256: Some(approved_recipient_set_sha256),
        outgoing_recipient_set_sha256: Some(outgoing_recipient_set_sha256),
        ..CoreEvidenceRefs::default()
    }
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn test_channel(
    root: &Path,
    db_path: &Path,
    channel: &str,
    account_key: Option<&str>,
) -> Result<Value> {
    macro_rules! test_chat_adapter {
        ($factory:ident, $channel:literal) => {{
            bootstrap_channel_account(root, $channel)?;
            let conn = open_channel_db(db_path)?;
            let adapter = communication_adapters::$factory();
            let resolved_account_key = resolve_account_key(&conn, $channel, account_key).ok();
            let account_config = resolved_account_key
                .as_deref()
                .and_then(|key| load_account_config(&conn, key).ok().flatten());
            let empty_profile = json!({});
            let adapter_json = adapter.test_cli(
                root,
                &communication_adapters::ChatTestCommandRequest {
                    db_path,
                    profile_json: account_config
                        .as_ref()
                        .map(|config| &config.profile_json)
                        .unwrap_or(&empty_profile),
                },
            )?;
            Ok(json!({
                "ok": adapter_json.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "channel": $channel,
                "account_key": resolved_account_key,
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }};
    }
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
        "discord" => test_chat_adapter!(discord, "discord"),
        "google_chat" => test_chat_adapter!(google_chat, "google_chat"),
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
        "matrix" => test_chat_adapter!(matrix, "matrix"),
        "mattermost" => test_chat_adapter!(mattermost, "mattermost"),
        "slack" => test_chat_adapter!(slack, "slack"),
        "telegram" => test_chat_adapter!(telegram, "telegram"),
        "zulip" => test_chat_adapter!(zulip, "zulip"),
        other => anyhow::bail!("unsupported channel test target: {other}"),
    }
}

pub(super) fn bootstrap_channel_account(root: &Path, channel: &str) -> Result<()> {
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

pub(super) fn parse_send_request(args: &[String]) -> Result<ChannelSendRequest> {
    let channel = required_flag_value(args, "--channel")?.to_string();
    let account_key = required_flag_value(args, "--account-key")?.to_string();
    let thread_key = required_flag_value(args, "--thread-key")?.to_string();
    let body = required_flag_value(args, "--body")?.to_string();
    let subject = find_flag_value(args, "--subject")
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let to = collect_flag_values(args, "--to");
    // Local/configured chat transports do not require ad hoc recipients here:
    // tui is local, Teams and the bot-platform adapters can target configured
    // default destinations or destination markers in thread_key, meeting
    // broadcasts through the active Playwright session, and WhatsApp replies
    // target the chat encoded in thread_key. Email and Jami still need
    // explicit remote targets.
    let whatsapp_thread_reply = channel == "whatsapp" && thread_key.contains("::chat::");
    if !matches!(
        channel.as_str(),
        "tui"
            | "teams"
            | "meeting"
            | "slack"
            | "discord"
            | "telegram"
            | "matrix"
            | "mattermost"
            | "zulip"
            | "google_chat"
    ) && !whatsapp_thread_reply
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
        reviewed_founder_send: has_flag(args, "--reviewed-founder-send")
            || has_flag(args, "--reviewed-communication-send"),
    })
}

pub(super) fn validate_founder_outbound_email(
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
        anyhow::bail!(
            "direct outbound email is blocked without communication review. Draft the email for completion review first; after approval the Harness sends the exact approved body."
        );
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

pub(super) fn resolve_outbound_subject(
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

pub(super) fn thread_prefers_voice_reply(conn: &Connection, thread_key: &str) -> Result<bool> {
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

pub(super) fn load_thread_subject(conn: &Connection, thread_key: &str) -> Result<Option<String>> {
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

pub(super) fn subject_is_placeholder(subject: &str) -> bool {
    let normalized = subject.trim().to_ascii_lowercase();
    normalized.is_empty() || normalized == "(no subject)" || normalized == "(ohne betreff)"
}

pub(super) fn parse_tui_ingest_request(args: &[String]) -> Result<TuiIngestRequest> {
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

pub(crate) fn ensure_schema_once(path: &Path, conn: &Connection) -> Result<()> {
    let key = channel_schema_cache_key(path);
    let ready = CHANNEL_SCHEMA_READY.get_or_init(|| Mutex::new(HashSet::new()));
    let mut ready = ready
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if ready.contains(&key) {
        return Ok(());
    }
    ensure_schema(conn)?;
    #[cfg(test)]
    record_channel_schema_ensure_for_tests(&key);
    ready.insert(key);
    Ok(())
}

pub(crate) fn ensure_open_routing_rows_once(path: &Path, conn: &Connection) -> Result<()> {
    let key = channel_schema_cache_key(path);
    let ready = CHANNEL_OPEN_ROUTING_READY.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut ready = ready
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stamp = channel_routing_cache_stamp(path);
    if ready.get(&key) == Some(&stamp) {
        return Ok(());
    }
    ensure_routing_rows_for_inbound(conn)?;
    #[cfg(test)]
    record_channel_open_routing_ensure_for_tests(&key);
    ready.insert(key, channel_routing_cache_stamp(path));
    Ok(())
}

#[cfg(unix)]
pub(super) fn channel_schema_cache_key(path: &Path) -> ChannelSchemaCacheKey {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| absolute_channel_db_path(path));
    let metadata = fs::metadata(&canonical)
        .or_else(|_| fs::metadata(path))
        .ok();
    let (device, inode) = metadata
        .map(|metadata| (metadata.dev(), metadata.ino()))
        .unwrap_or((0, 0));
    (canonical, device, inode)
}

#[cfg(not(unix))]
pub(super) fn channel_schema_cache_key(path: &Path) -> ChannelSchemaCacheKey {
    fs::canonicalize(path).unwrap_or_else(|_| absolute_channel_db_path(path))
}

pub(super) fn queue_task_list_cache_key(
    path: &Path,
    statuses: &[String],
    limit: usize,
) -> QueueTaskListCacheKey {
    QueueTaskListCacheKey {
        database: channel_schema_cache_key(path),
        statuses: statuses.to_vec(),
        limit,
    }
}

pub(super) fn cached_queue_task_list(
    key: &QueueTaskListCacheKey,
    stamp: &QueueTaskListCacheStamp,
) -> Option<Vec<QueueTaskView>> {
    let cache = QUEUE_TASK_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .get(key)
        .filter(|entry| &entry.stamp == stamp)
        .map(|entry| entry.tasks.clone())
}

pub(super) fn store_queue_task_list_cache(
    key: QueueTaskListCacheKey,
    stamp: QueueTaskListCacheStamp,
    tasks: Vec<QueueTaskView>,
) {
    let cache = QUEUE_TASK_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= QUEUE_TASK_LIST_CACHE_MAX_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(key, QueueTaskListCacheEntry { stamp, tasks });
}

pub(super) fn queue_task_count_cache_key(
    path: &Path,
    statuses: &[String],
) -> QueueTaskCountCacheKey {
    QueueTaskCountCacheKey {
        database: channel_schema_cache_key(path),
        statuses: statuses.to_vec(),
    }
}

pub(super) fn cached_queue_task_count(
    key: &QueueTaskCountCacheKey,
    stamp: &QueueTaskListCacheStamp,
) -> Option<usize> {
    let cache = QUEUE_TASK_COUNT_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .get(key)
        .filter(|entry| &entry.stamp == stamp)
        .map(|entry| entry.count)
}

pub(super) fn store_queue_task_count_cache(
    key: QueueTaskCountCacheKey,
    stamp: QueueTaskListCacheStamp,
    count: usize,
) {
    let cache = QUEUE_TASK_COUNT_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= QUEUE_TASK_COUNT_CACHE_MAX_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(key, QueueTaskCountCacheEntry { stamp, count });
}

pub(super) fn absolute_channel_db_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn channel_routing_cache_stamp(path: &Path) -> ChannelRoutingCacheStamp {
    (
        channel_file_size_stamp(path),
        channel_file_size_stamp(&sqlite_sidecar_path(path, "-wal")),
        channel_file_size_stamp(&sqlite_sidecar_path(path, "-journal")),
    )
}

pub(super) fn queue_task_list_cache_stamp(path: &Path) -> QueueTaskListCacheStamp {
    match queue_task_projection_clock_stamp(path) {
        Ok(stamp) => stamp,
        Err(_) => QueueTaskListCacheStamp::File {
            main: channel_file_change_stamp(path),
            wal: channel_file_change_stamp(&sqlite_sidecar_path(path, "-wal")),
            journal: channel_file_change_stamp(&sqlite_sidecar_path(path, "-journal")),
        },
    }
}

pub(super) fn queue_task_projection_clock_stamp(path: &Path) -> Result<QueueTaskListCacheStamp> {
    with_cached_channel_db_read_only(path, |conn| {
        let Some(conn) = conn else {
            return Ok(QueueTaskListCacheStamp::ProjectionClock {
                database_exists: false,
                clock_exists: false,
                version: 0,
                message_count: 0,
                routing_count: 0,
                updated_at: String::new(),
            });
        };
        let clock_exists =
            channel_projection_tables_exist(conn, &["communication_projection_clock"])?;
        if !clock_exists {
            return Ok(QueueTaskListCacheStamp::ProjectionClock {
                database_exists: true,
                clock_exists: false,
                version: 0,
                message_count: 0,
                routing_count: 0,
                updated_at: String::new(),
            });
        }
        let (version, message_count, routing_count, updated_at) = conn.query_row(
            r#"
            SELECT version, message_count, routing_count, updated_at
            FROM communication_projection_clock
            WHERE id = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        Ok(QueueTaskListCacheStamp::ProjectionClock {
            database_exists: true,
            clock_exists: true,
            version,
            message_count: non_negative_i64_to_usize(message_count),
            routing_count: non_negative_i64_to_usize(routing_count),
            updated_at,
        })
    })
}

pub(super) fn with_cached_channel_db_read_only<T>(
    path: &Path,
    f: impl FnOnce(Option<&Connection>) -> Result<T>,
) -> Result<T> {
    CHANNEL_DB_READ_ONLY.with(|cell| {
        let mut cached = cell.borrow_mut();
        if !path.exists() {
            cached.clear();
            return f(None);
        }
        let key = channel_schema_cache_key(path);
        if !cached.contains_key(&key) {
            if cached.len() >= CHANNEL_DB_READ_ONLY_MAX_ENTRIES {
                cached.clear();
            }
            let Some(conn) = open_channel_db_read_only(path)? else {
                return f(None);
            };
            #[cfg(test)]
            CHANNEL_DB_READ_ONLY_OPEN_COUNT.with(|count| count.set(count.get() + 1));
            cached.insert(key.clone(), conn);
        }
        let result = f(cached.get(&key));
        if result.is_err() {
            cached.remove(&key);
        }
        result
    })
}

#[cfg(test)]
pub(super) fn reset_channel_db_read_only_cache_for_tests() {
    CHANNEL_DB_READ_ONLY.with(|cell| cell.borrow_mut().clear());
    CHANNEL_DB_READ_ONLY_OPEN_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(super) fn channel_db_read_only_open_count_for_tests() -> usize {
    CHANNEL_DB_READ_ONLY_OPEN_COUNT.with(std::cell::Cell::get)
}

pub(super) fn channel_file_size_stamp(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

pub(super) fn channel_file_change_stamp(path: &Path) -> ChannelFileChangeStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return (0, 0);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (metadata.len(), modified_at)
}

pub(super) fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
pub(super) fn record_channel_schema_ensure_for_tests(key: &ChannelSchemaCacheKey) {
    let counts = CHANNEL_SCHEMA_ENSURE_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
pub(super) fn channel_schema_ensure_count_for_tests(path: &Path) -> usize {
    let key = channel_schema_cache_key(path);
    let Some(counts) = CHANNEL_SCHEMA_ENSURE_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

#[cfg(test)]
pub(super) fn record_channel_open_routing_ensure_for_tests(key: &ChannelSchemaCacheKey) {
    let counts = CHANNEL_OPEN_ROUTING_ENSURE_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
pub(super) fn channel_open_routing_ensure_count_for_tests(path: &Path) -> usize {
    let key = channel_schema_cache_key(path);
    let Some(counts) = CHANNEL_OPEN_ROUTING_ENSURE_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn record_channel_db_open_for_tests(path: &Path) {
    let counts = CHANNEL_DB_OPEN_CALL_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(path.to_path_buf()).or_insert(0) += 1;
}

#[cfg(test)]
pub(crate) fn reset_channel_db_open_count_for_tests(path: &Path) {
    if let Some(counts) = CHANNEL_DB_OPEN_CALL_COUNTS.get() {
        counts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(path);
    }
}

#[cfg(test)]
pub(crate) fn channel_db_open_count_for_tests(path: &Path) -> usize {
    let Some(counts) = CHANNEL_DB_OPEN_CALL_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(path).copied().unwrap_or(0)
}

#[cfg(test)]
pub(super) fn record_queue_task_list_cache_miss_for_tests(key: &QueueTaskListCacheKey) {
    let counts = QUEUE_TASK_LIST_CACHE_MISS_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
pub(super) fn queue_task_list_cache_miss_count_for_tests(
    path: &Path,
    statuses: &[String],
    limit: usize,
) -> usize {
    let key = queue_task_list_cache_key(path, statuses, limit);
    let Some(counts) = QUEUE_TASK_LIST_CACHE_MISS_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

#[cfg(test)]
pub(super) fn record_queue_task_count_cache_miss_for_tests(key: &QueueTaskCountCacheKey) {
    let counts = QUEUE_TASK_COUNT_CACHE_MISS_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
pub(super) fn queue_task_count_cache_miss_count_for_tests(
    path: &Path,
    statuses: &[String],
) -> usize {
    let key = queue_task_count_cache_key(path, statuses);
    let Some(counts) = QUEUE_TASK_COUNT_CACHE_MISS_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

pub(super) fn open_channel_db_read_only(path: &Path) -> Result<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open channel db read-only {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for read-only channels")?;
    conn.execute_batch("PRAGMA query_only = ON;")
        .context("failed to configure read-only channel projection")?;
    Ok(Some(conn))
}

pub(super) fn channel_projection_tables_exist(conn: &Connection, tables: &[&str]) -> Result<bool> {
    for table in tables {
        let exists = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                params![table],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .with_context(|| format!("failed to inspect channel projection table {table}"))?
            .is_some();
        if !exists {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn empty_business_os_projection(collection: &str) -> Value {
    json!({
        "ok": true,
        "collection": collection,
        "documents": [],
        "count": 0,
        "since_ms": 0,
    })
}

pub(super) fn ensure_schema(conn: &Connection) -> Result<()> {
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

        CREATE INDEX IF NOT EXISTS idx_communication_messages_email_folder_remote
            ON communication_messages(channel, account_key, folder_hint, remote_id);

        CREATE INDEX IF NOT EXISTS idx_communication_messages_queue_business_command_valid
            ON communication_messages(json_extract(metadata_json, '$.business_os_command_id'), observed_at DESC)
            WHERE channel = 'queue' AND direction = 'inbound' AND json_valid(metadata_json);

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
            first_pending_at TEXT,
            lease_expires_at TEXT,
            lease_worker_id TEXT,
            failure_class TEXT,
            failure_attempt_count INTEGER NOT NULL DEFAULT 0,
            retry_not_before TEXT,
            priority_time_credit_hours INTEGER NOT NULL DEFAULT 0,
            hold_reason TEXT,
            wait_entity_type TEXT,
            wait_entity_id TEXT,
            acked_at TEXT,
            last_error TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_communication_routing_status_owner
            ON communication_routing_state(route_status, lease_owner, leased_at, updated_at);

        CREATE TABLE IF NOT EXISTS communication_projection_clock (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL,
            account_count INTEGER NOT NULL,
            thread_count INTEGER NOT NULL,
            message_count INTEGER NOT NULL,
            routing_count INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );

        INSERT INTO communication_projection_clock (
            id, version, account_count, thread_count, message_count, routing_count, updated_at
        )
        SELECT
            1,
            0,
            (SELECT COUNT(*) FROM communication_accounts),
            (SELECT COUNT(*) FROM communication_threads),
            (SELECT COUNT(*) FROM communication_messages),
            (SELECT COUNT(*) FROM communication_routing_state),
            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE NOT EXISTS (
            SELECT 1 FROM communication_projection_clock WHERE id = 1
        );

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_accounts_insert
        AFTER INSERT ON communication_accounts
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                account_count = account_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_accounts_update
        AFTER UPDATE ON communication_accounts
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_accounts_delete
        AFTER DELETE ON communication_accounts
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                account_count = CASE
                    WHEN account_count > 0 THEN account_count - 1
                    ELSE 0
                END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_threads_insert
        AFTER INSERT ON communication_threads
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                thread_count = thread_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_threads_update
        AFTER UPDATE ON communication_threads
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_threads_delete
        AFTER DELETE ON communication_threads
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                thread_count = CASE
                    WHEN thread_count > 0 THEN thread_count - 1
                    ELSE 0
                END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_messages_insert
        AFTER INSERT ON communication_messages
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                message_count = message_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_messages_update
        AFTER UPDATE ON communication_messages
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_messages_delete
        AFTER DELETE ON communication_messages
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                message_count = CASE
                    WHEN message_count > 0 THEN message_count - 1
                    ELSE 0
                END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_routing_insert
        AFTER INSERT ON communication_routing_state
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                routing_count = routing_count + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_routing_update
        AFTER UPDATE ON communication_routing_state
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_communication_projection_clock_routing_delete
        AFTER DELETE ON communication_routing_state
        BEGIN
            UPDATE communication_projection_clock
            SET version = version + 1,
                routing_count = CASE
                    WHEN routing_count > 0 THEN routing_count - 1
                    ELSE 0
                END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1;
        END;

        CREATE TABLE IF NOT EXISTS business_command_aggregates (
            command_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            payload_hash TEXT NOT NULL,
            module TEXT NOT NULL,
            command_type TEXT NOT NULL,
            record_id TEXT NOT NULL DEFAULT '',
            execution_mode TEXT NOT NULL CHECK(execution_mode IN ('control', 'queue')),
            execution_phase TEXT NOT NULL,
            terminal_status TEXT NOT NULL DEFAULT 'none',
            attempt INTEGER NOT NULL DEFAULT 0,
            projection_version INTEGER NOT NULL DEFAULT 1,
            intent_json TEXT NOT NULL,
            result_json TEXT,
            error_code TEXT,
            error_message TEXT,
            retryable INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_command_aggregates_state
            ON business_command_aggregates(execution_phase, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_business_command_open_type_id
            ON business_command_aggregates(command_type, command_id)
            WHERE execution_phase != 'terminal';
        CREATE INDEX IF NOT EXISTS idx_active_adapter_reconciliation
            ON business_command_aggregates(module, record_id,
                json_extract(intent_json,'$.payload.configuration_digest'), created_at_ms, command_id)
            WHERE command_type='outbound.research.adapters.reconcile' AND execution_phase!='terminal';

        CREATE TABLE IF NOT EXISTS business_command_task_links (
            command_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL UNIQUE,
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );

        CREATE TABLE IF NOT EXISTS business_command_transitions (
            transition_id INTEGER PRIMARY KEY AUTOINCREMENT,
            command_id TEXT NOT NULL,
            projection_version INTEGER NOT NULL,
            from_phase TEXT NOT NULL,
            to_phase TEXT NOT NULL,
            terminal_status TEXT NOT NULL DEFAULT 'none',
            reason TEXT NOT NULL DEFAULT '',
            evidence_json TEXT NOT NULL DEFAULT '{{}}',
            created_at_ms INTEGER NOT NULL,
            UNIQUE(command_id, projection_version),
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );

        CREATE TABLE IF NOT EXISTS business_command_effects (
            command_id TEXT NOT NULL,
            effect_key TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('claimed', 'completed', 'failed', 'uncertain')),
            result_json TEXT,
            error_message TEXT,
            claimed_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(command_id, effect_key),
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );

        CREATE TABLE IF NOT EXISTS business_command_sagas (
            saga_id TEXT PRIMARY KEY,
            command_id TEXT NOT NULL UNIQUE,
            saga_kind TEXT NOT NULL,
            phase TEXT NOT NULL CHECK(phase IN ('forward', 'compensating', 'completed', 'compensated', 'manual_intervention')),
            current_step INTEGER NOT NULL DEFAULT 0,
            total_steps INTEGER NOT NULL,
            compensation_status TEXT NOT NULL DEFAULT 'not_started',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );

        CREATE TABLE IF NOT EXISTS business_command_saga_steps (
            saga_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            step_name TEXT NOT NULL,
            forward_effect_key TEXT NOT NULL,
            compensation_effect_key TEXT NOT NULL,
            forward_status TEXT NOT NULL DEFAULT 'pending' CHECK(forward_status IN ('pending', 'claimed', 'completed', 'failed')),
            compensation_status TEXT NOT NULL DEFAULT 'not_required' CHECK(compensation_status IN ('not_required', 'pending', 'claimed', 'completed', 'failed')),
            forward_attempts INTEGER NOT NULL DEFAULT 0,
            compensation_attempts INTEGER NOT NULL DEFAULT 0,
            evidence_json TEXT NOT NULL DEFAULT '{{}}',
            error_message TEXT,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(saga_id, step_index),
            UNIQUE(saga_id, forward_effect_key),
            UNIQUE(saga_id, compensation_effect_key),
            FOREIGN KEY(saga_id) REFERENCES business_command_sagas(saga_id)
        );

        CREATE TABLE IF NOT EXISTS business_app_action_snapshots (
            command_id TEXT PRIMARY KEY,
            module_id TEXT NOT NULL,
            action_name TEXT NOT NULL,
            definition_hash TEXT NOT NULL,
            definition_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );
        CREATE INDEX IF NOT EXISTS idx_business_app_action_snapshots_definition
            ON business_app_action_snapshots(module_id, action_name, definition_hash);

        CREATE TABLE IF NOT EXISTS business_command_results (
            command_id TEXT NOT NULL,
            attempt INTEGER NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('succeeded', 'failed', 'cancelled')),
            user_reply TEXT NOT NULL DEFAULT '',
            artifacts_json TEXT NOT NULL DEFAULT '[]',
            writebacks_json TEXT NOT NULL DEFAULT '[]',
            claims_json TEXT NOT NULL DEFAULT '[]',
            error_json TEXT,
            review_status TEXT NOT NULL DEFAULT 'pending',
            validation_status TEXT NOT NULL DEFAULT 'pending',
            review_evidence_json TEXT NOT NULL DEFAULT '{{}}',
            created_at_ms INTEGER NOT NULL,
            reviewed_at_ms INTEGER,
            PRIMARY KEY(command_id, attempt),
            FOREIGN KEY(command_id) REFERENCES business_command_aggregates(command_id)
        );

        CREATE TABLE IF NOT EXISTS business_command_outbox (
            event_id TEXT PRIMARY KEY,
            command_id TEXT NOT NULL,
            projection_version INTEGER NOT NULL,
            destination TEXT NOT NULL CHECK(destination IN ('business-os', 'rxdb')),
            event_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'delivered', 'failed', 'dead_letter')),
            attempts INTEGER NOT NULL DEFAULT 0,
            next_attempt_at_ms INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at_ms INTEGER NOT NULL,
            delivered_at_ms INTEGER,
            UNIQUE(command_id, projection_version, destination)
        );
        CREATE INDEX IF NOT EXISTS idx_business_command_outbox_delivery
            ON business_command_outbox(status, next_attempt_at_ms, created_at_ms);

        CREATE TABLE IF NOT EXISTS business_command_intake_failures (
            failure_id INTEGER PRIMARY KEY AUTOINCREMENT,
            command_id TEXT NOT NULL,
            attempt INTEGER NOT NULL,
            error_message TEXT NOT NULL,
            exhausted INTEGER NOT NULL DEFAULT 0,
            observed_at_ms INTEGER NOT NULL,
            resolved_at_ms INTEGER,
            UNIQUE(command_id, attempt)
        );
        CREATE INDEX IF NOT EXISTS idx_business_command_intake_failures_open
            ON business_command_intake_failures(command_id, resolved_at_ms, attempt);

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
    ensure_routing_state_hardening_columns(conn)?;
    crate::crew::ensure_schema(conn)?;
    Ok(())
}

#[cfg(test)]
mod queue_task_projection_clock_tests {
    use super::*;

    #[test]
    fn projection_clock_reuses_read_only_connection_and_observes_commits() {
        let root = std::env::temp_dir().join(format!(
            "ctox-channel-clock-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join("ctox.sqlite3");
        {
            let conn = Connection::open(&path).expect("open writable channel db");
            conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                CREATE TABLE communication_projection_clock (
                    id INTEGER PRIMARY KEY,
                    version INTEGER NOT NULL,
                    message_count INTEGER NOT NULL,
                    routing_count INTEGER NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO communication_projection_clock
                    (id, version, message_count, routing_count, updated_at)
                VALUES (1, 1, 2, 3, '2026-08-16T00:00:00Z');
                "#,
            )
            .expect("seed projection clock");
        }

        reset_channel_db_read_only_cache_for_tests();
        let first = queue_task_projection_clock_stamp(&path).expect("first clock stamp");
        let second = queue_task_projection_clock_stamp(&path).expect("second clock stamp");
        assert_eq!(first, second);
        assert_eq!(channel_db_read_only_open_count_for_tests(), 1);

        {
            let conn = Connection::open(&path).expect("reopen writable channel db");
            conn.execute(
                "UPDATE communication_projection_clock SET version = 2, updated_at = ?1 WHERE id = 1",
                params!["2026-08-16T00:01:00Z"],
            )
            .expect("advance projection clock");
        }
        let advanced = queue_task_projection_clock_stamp(&path).expect("advanced clock stamp");
        assert_ne!(advanced, first);
        assert_eq!(channel_db_read_only_open_count_for_tests(), 1);

        reset_channel_db_read_only_cache_for_tests();
        fs::remove_dir_all(root).expect("remove temp root");
    }
}
