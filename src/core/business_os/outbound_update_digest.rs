// Origin: CTOX
// License: AGPL-3.0-only
//
// Outbound "Update-Verteiler": an admin stores one or more recipients, the
// weekdays, a local time and a sender mailbox in the Outbound app. At that time
// CTOX mails what happened in the Outbound research since the previous update:
// finished research, Sellify handovers, and what is blocked.
//
// The report is built from the app's own records (the RxDB store the browser
// shows), never from a model's recollection, so every number in the mail can
// be traced to a lead record. The admin's configuration is the approval: the
// mail goes through the policy-report send path (exact body digest, core send
// transition, durable send artifact), at most once per local day.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::store;
use crate::mission::channels;

pub(crate) const DIGEST_CONFIG_ID: &str = "outbound_update_digest_v1";
pub(crate) const DIGEST_STATUS_ID: &str = "outbound_update_digest_status_v1";
const POLICY_COLLECTION: &str = "outbound_lead_generation_research_policies";
const LEAD_COLLECTION: &str = "outbound_lead_generation_leads";
const ADAPTER_COLLECTION: &str = "outbound_lead_generation_adapters";
const SOURCE_COLLECTION: &str = "outbound_lead_generation_sources";
const STATE_FILE: &str = "outbound-update-digest-state.json";
/// A daemon that was down at 07:00 still sends until 13:00; later the update
/// waits for the next configured day instead of arriving in the evening.
const SEND_WINDOW_MINUTES: u32 = 6 * 60;
const MAX_ATTEMPTS_PER_SLOT: u32 = 3;
const RETRY_AFTER_MS: i64 = 10 * 60 * 1000;
const TICK_EVERY: Duration = Duration::from_secs(60);
const STUCK_RESEARCH_MS: i64 = 2 * 60 * 60 * 1000;
const FIRST_REPORT_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const LIST_LIMIT: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DigestConfig {
    pub enabled: bool,
    pub recipients: Vec<String>,
    /// ISO weekdays, 1 = Monday … 7 = Sunday.
    pub weekdays: Vec<u32>,
    pub minute_of_day: u32,
    pub timezone: Tz,
    pub sender_email: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct DigestState {
    #[serde(default)]
    last_sent_slot: String,
    #[serde(default)]
    last_sent_at_ms: i64,
    #[serde(default)]
    attempt_slot: String,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    last_attempt_at_ms: i64,
    #[serde(default)]
    last_error: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DigestReport {
    pub subject: String,
    pub body: String,
    pub html: String,
    pub stats: Value,
}

pub(crate) fn parse_config(doc: &Value) -> Option<DigestConfig> {
    let config = doc.get("update_digest").unwrap_or(doc);
    let mut weekdays = config
        .get("weekdays")
        .and_then(Value::as_array)
        .map(|days| {
            days.iter()
                .filter_map(Value::as_u64)
                .map(|day| day as u32)
                .filter(|day| (1..=7).contains(day))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    weekdays.sort_unstable();
    weekdays.dedup();
    let minute_of_day = parse_hhmm(config.get("time").and_then(Value::as_str).unwrap_or(""))?;
    let timezone = config
        .get("timezone")
        .and_then(Value::as_str)
        .and_then(|name| name.trim().parse::<Tz>().ok())
        .unwrap_or(chrono_tz::Europe::Berlin);
    Some(DigestConfig {
        enabled: config
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recipients: parse_recipients(config.get("recipients")),
        weekdays,
        minute_of_day,
        timezone,
        sender_email: config
            .get("sender_email")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase(),
    })
}

fn parse_hhmm(value: &str) -> Option<u32> {
    let (hours, minutes) = value.trim().split_once(':')?;
    let hours = hours.trim().parse::<u32>().ok()?;
    let minutes = minutes.trim().parse::<u32>().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

pub(crate) fn is_plausible_email(value: &str) -> bool {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !value.contains(char::is_whitespace)
        && !domain.contains('@')
}

fn parse_recipients(value: Option<&Value>) -> Vec<String> {
    let raw: Vec<String> = match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(text)) => text.split([',', ';', '\n']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let mut recipients = Vec::new();
    for address in raw {
        let address = address.trim().to_ascii_lowercase();
        if is_plausible_email(&address) && !recipients.contains(&address) {
            recipients.push(address);
        }
    }
    recipients
}

/// The local day this update belongs to, when it is due now.
fn due_slot(config: &DigestConfig, now: DateTime<Utc>, state: &DigestState) -> Option<String> {
    if !config.enabled || config.recipients.is_empty() || config.weekdays.is_empty() {
        return None;
    }
    let local = now.with_timezone(&config.timezone);
    if !config
        .weekdays
        .contains(&local.weekday().number_from_monday())
    {
        return None;
    }
    let minute = local.hour() * 60 + local.minute();
    if minute < config.minute_of_day || minute >= config.minute_of_day + SEND_WINDOW_MINUTES {
        return None;
    }
    let slot = local.date_naive().to_string();
    if state.last_sent_slot == slot {
        return None;
    }
    if state.attempt_slot == slot
        && (state.attempts >= MAX_ATTEMPTS_PER_SLOT
            || now.timestamp_millis() - state.last_attempt_at_ms < RETRY_AFTER_MS)
    {
        return None;
    }
    Some(slot)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

fn millis(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(|raw| raw.as_i64().or_else(|| raw.as_f64().map(|v| v as i64)))
        .unwrap_or(0)
}

fn lead_name(lead: &Value) -> String {
    let name = text(lead, "name");
    if !name.is_empty() {
        return name.to_string();
    }
    let firm = lead
        .get("data")
        .map(|data| text(data, "firma_name"))
        .unwrap_or("");
    if !firm.is_empty() {
        return firm.to_string();
    }
    text(lead, "id").to_string()
}

fn is_test_lead(lead: &Value) -> bool {
    let marker = |value: &str| value.to_ascii_uppercase().starts_with("UITEST-");
    marker(text(lead, "name")) || marker(text(lead, "campaign"))
}

fn research_finished_ms(lead: &Value) -> i64 {
    let finished = lead
        .get("payload")
        .map(|payload| millis(payload, "research_finished_at_ms"))
        .unwrap_or(0);
    if finished > 0 {
        finished
    } else {
        millis(lead, "research_updated_at_ms")
    }
}

fn field_counts(lead: &Value) -> (usize, usize, usize) {
    let Some(fields) = lead.get("field_status").and_then(Value::as_object) else {
        return (0, 0, 0);
    };
    let with_status = |wanted: &str| {
        fields
            .values()
            .filter(|value| value.get("status").and_then(Value::as_str) == Some(wanted))
            .count()
    };
    let verified = with_status("verified");
    let open = with_status("action_required");
    (verified, open, fields.len())
}

fn contact_count(lead: &Value) -> usize {
    lead.get("contacts")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// Sentence-sized, mail-safe reason text. Internal error dumps stay in the app.
fn short_reason(raw: &str) -> Option<String> {
    let first = raw
        .split(['\n', '|'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.');
    if first.is_empty() {
        return None;
    }
    let mut short = first.chars().take(110).collect::<String>();
    if first.chars().count() > 110 {
        short.push('…');
    }
    if channels::ensure_founder_outbound_body_text_clean(&short).is_err()
        || short.contains('`')
        || short.contains("::")
    {
        return Some("technischer Fehler, Details in der App".to_string());
    }
    Some(short)
}

fn is_reconciliation_noise(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    [
        "reconcil",
        "adapter-abgleich",
        "command writeback failed",
        "unsupported status",
        "queue lease",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn source_problem(source: &Value, adapter: Option<&Value>) -> Option<&'static str> {
    let status = adapter
        .map(|adapter| text(adapter, "status"))
        .filter(|status| !status.is_empty())
        .unwrap_or_else(|| text(source, "adapter_status"))
        .to_ascii_lowercase();
    let scrape = adapter
        .map(|adapter| text(adapter, "scrape_status"))
        .unwrap_or("")
        .to_ascii_lowercase();
    let auth = adapter
        .map(|adapter| text(adapter, "auth_status"))
        .filter(|auth| !auth.is_empty())
        .unwrap_or_else(|| text(source, "auth_status"))
        .to_ascii_lowercase();
    let error = adapter
        .map(|adapter| text(adapter, "last_error"))
        .unwrap_or("");
    if is_reconciliation_noise(error) {
        return None;
    }
    if auth == "credential_missing" {
        return Some("Zugangsdaten fehlen");
    }
    if scrape == "script_required" {
        return Some("noch kein Abruf-Skript hinterlegt");
    }
    if status.contains("blocked") || scrape == "blocked" {
        return Some("Zugriff blockiert");
    }
    if status.contains("portal_drift") {
        return Some("Seitenaufbau geändert, Abruf muss angepasst werden");
    }
    if status.contains("temporary_unreachable") {
        return Some("bei der letzten Prüfung nicht erreichbar");
    }
    if status.contains("fields_missing") {
        return Some("liefert nicht alle erwarteten Felder");
    }
    if status.contains("failed") || scrape.contains("failed") || scrape.contains("error") {
        return Some("letzte Prüfung fehlgeschlagen");
    }
    None
}

fn local_label(tz: Tz, ms: i64, with_time: bool) -> String {
    let Some(stamp) = tz.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    const DAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
    let day = DAYS[stamp.weekday().num_days_from_monday() as usize];
    if with_time {
        format!("{day} {}", stamp.format("%d.%m. %H:%M"))
    } else {
        stamp.format("%d.%m.").to_string()
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A source test older than this says nothing about today; such sources are
/// named once as "not re-checked" instead of being counted as blockers.
const SOURCE_TEST_FRESH_MS: i64 = 3 * 24 * 60 * 60 * 1000;
const REVIEW_LIST_LIMIT: usize = 8;

#[derive(Debug, Clone)]
struct ResearchedRow {
    name: String,
    verified: usize,
    fields: usize,
    contacts: usize,
    open: usize,
}

#[derive(Debug, Clone)]
struct AttentionRow {
    kind: &'static str,
    subject: String,
    detail: String,
}

#[derive(Debug, Clone, Default)]
struct DigestData {
    period: String,
    date_label: String,
    researched: Vec<ResearchedRow>,
    verified: usize,
    contacts: usize,
    sellify_done: Vec<String>,
    attention: Vec<AttentionRow>,
    stale_sources: Vec<String>,
    review: Vec<(String, usize)>,
    review_leads: usize,
    review_fields: usize,
    total: usize,
    total_completed: usize,
    total_new: usize,
    running: usize,
    total_sellify: usize,
}

fn collect_digest(
    leads: &[Value],
    sources: &[Value],
    adapters: &[Value],
    since_ms: i64,
    now_ms: i64,
    tz: Tz,
) -> DigestData {
    let in_window = |ms: i64| ms > since_ms && ms <= now_ms;
    let leads = leads
        .iter()
        .filter(|lead| !is_test_lead(lead))
        .collect::<Vec<_>>();
    let mut data = DigestData {
        period: format!(
            "{} bis {}",
            local_label(tz, since_ms, true),
            local_label(tz, now_ms, true)
        ),
        date_label: tz
            .timestamp_millis_opt(now_ms)
            .single()
            .map(|stamp| {
                const DAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
                format!(
                    "{} {}",
                    DAYS[stamp.weekday().num_days_from_monday() as usize],
                    stamp.format("%d.%m.%Y")
                )
            })
            .unwrap_or_default(),
        total: leads.len(),
        ..DigestData::default()
    };
    let mut failed = Vec::new();
    let mut stuck = Vec::new();
    let mut sellify_failed = Vec::new();
    for lead in &leads {
        let status = text(lead, "research_status");
        let (verified, open, fields) = field_counts(lead);
        match status {
            "completed" => data.total_completed += 1,
            "needs_review" | "partially_completed" => {
                data.review_leads += 1;
                data.review_fields += open;
                if open > 0 {
                    data.review.push((lead_name(lead), open));
                }
            }
            "" | "new" => data.total_new += 1,
            "running" | "queued" | "requested" => {
                data.running += 1;
                let updated = millis(lead, "research_updated_at_ms");
                if updated > 0 && now_ms - updated > STUCK_RESEARCH_MS {
                    stuck.push(AttentionRow {
                        kind: "Recherche hängt",
                        subject: lead_name(lead),
                        detail: format!("kein Fortschritt seit {}", local_label(tz, updated, true)),
                    });
                }
            }
            _ => {}
        }
        if matches!(status, "completed" | "needs_review" | "partially_completed")
            && in_window(research_finished_ms(lead))
        {
            let contacts = contact_count(lead);
            data.verified += verified;
            data.contacts += contacts;
            data.researched.push(ResearchedRow {
                name: lead_name(lead),
                verified,
                fields,
                contacts,
                open,
            });
        }
        if status == "failed" && in_window(millis(lead, "research_updated_at_ms")) {
            failed.push(AttentionRow {
                kind: "Recherche fehlgeschlagen",
                subject: lead_name(lead),
                detail: lead
                    .get("research_error")
                    .and_then(Value::as_str)
                    .and_then(short_reason)
                    .unwrap_or_else(|| "Grund siehe App".to_string()),
            });
        }
        match text(lead, "sellify_status") {
            "completed" => {
                data.total_sellify += 1;
                let finished = lead
                    .get("payload")
                    .map(|payload| millis(payload, "sellify_finished_at_ms"))
                    .unwrap_or(0);
                if in_window(finished) {
                    data.sellify_done.push(lead_name(lead));
                }
            }
            "failed" => sellify_failed.push(AttentionRow {
                kind: "Sellify-Übergabe fehlgeschlagen",
                subject: lead_name(lead),
                detail: "erneut übergeben oder in der App prüfen".to_string(),
            }),
            _ => {}
        }
    }
    let mut source_rows = Vec::new();
    for source in sources {
        if source.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let source_id = text(source, "id");
        if source_id.is_empty() || source_id == "sellify" {
            continue;
        }
        let adapter = adapters
            .iter()
            .filter(|adapter| text(adapter, "source_id") == source_id)
            .max_by_key(|adapter| millis(adapter, "updated_at_ms"));
        let Some(problem) = source_problem(source, adapter) else {
            continue;
        };
        let label = match text(source, "label") {
            "" => source_id.to_string(),
            label => label.to_string(),
        };
        let checked = adapter
            .map(|adapter| millis(adapter, "updated_at_ms"))
            .unwrap_or(0);
        if checked <= 0 || now_ms - checked > SOURCE_TEST_FRESH_MS {
            data.stale_sources.push(label);
            continue;
        }
        source_rows.push(AttentionRow {
            kind: "Quelle gestört",
            subject: label,
            detail: format!("{problem} (geprüft {})", local_label(tz, checked, false)),
        });
    }
    source_rows.sort_by(|a, b| a.subject.cmp(&b.subject));
    data.stale_sources.sort();
    data.attention.extend(failed);
    data.attention.extend(stuck);
    data.attention.extend(sellify_failed);
    data.attention.extend(source_rows);
    data.researched
        .sort_by(|a, b| b.verified.cmp(&a.verified).then(a.name.cmp(&b.name)));
    data.review
        .sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    data
}

fn digest_subject(data: &DigestData) -> String {
    let mut subject = format!(
        "Outbound-Update {}: {} abgeschlossen, {} an Sellify",
        data.date_label,
        plural(data.researched.len(), "Recherche", "Recherchen"),
        data.sellify_done.len()
    );
    if !data.attention.is_empty() {
        subject.push_str(&format!(
            ", {}",
            plural(data.attention.len(), "Blocker", "Blocker")
        ));
    }
    subject
}

const FOOTER_TEXT: &str = "Automatisch erstellt aus der Outbound-App. Empfänger und Zeitplan ändern: Outbound-App › Recherche-Einstellungen › Update-Verteiler.";

fn stale_sources_text(data: &DigestData) -> Option<String> {
    (!data.stale_sources.is_empty()).then(|| {
        format!(
            "{} seit über drei Tagen nicht neu geprüft (letzter Stand fehlerhaft): {}. Neu prüfen unter Recherche-Einstellungen › Quellen & Zugänge.",
            plural(data.stale_sources.len(), "Quelle", "Quellen"),
            data.stale_sources.join(", ")
        )
    })
}

fn render_text(data: &DigestData) -> String {
    let mut out = vec![
        format!("OUTBOUND-UPDATE · {}", data.date_label),
        format!("Zeitraum: {}", data.period),
        String::new(),
        "AUF EINEN BLICK".to_string(),
        format!("Recherchen abgeschlossen: {}", data.researched.len()),
        format!("Felder belegt: {}", data.verified),
        format!("Ansprechpartner gefunden: {}", data.contacts),
        format!("An Sellify übergeben: {}", data.sellify_done.len()),
        format!("Blocker: {}", data.attention.len()),
        String::new(),
        "ABGESCHLOSSENE RECHERCHEN".to_string(),
    ];
    if data.researched.is_empty() {
        out.push("Keine Recherche in diesem Zeitraum abgeschlossen.".to_string());
    }
    for row in data.researched.iter().take(LIST_LIMIT) {
        out.push(format!(
            "- {}: {} von {} Feldern belegt, {}{}",
            row.name,
            row.verified,
            row.fields,
            plural(row.contacts, "Ansprechpartner", "Ansprechpartner"),
            if row.open > 0 {
                format!(", {} zu prüfen", plural(row.open, "Feld", "Felder"))
            } else {
                String::new()
            }
        ));
    }
    if data.researched.len() > LIST_LIMIT {
        out.push(format!(
            "- … und {} weitere",
            data.researched.len() - LIST_LIMIT
        ));
    }
    if !data.sellify_done.is_empty() {
        out.push(String::new());
        out.push("AN SELLIFY ÜBERGEBEN".to_string());
        for name in data.sellify_done.iter().take(LIST_LIMIT) {
            out.push(format!("- {name}"));
        }
    }
    out.push(String::new());
    out.push("BLOCKER".to_string());
    if data.attention.is_empty() {
        out.push("Keine.".to_string());
    }
    for row in data.attention.iter().take(LIST_LIMIT) {
        out.push(format!("- {}: {} – {}", row.kind, row.subject, row.detail));
    }
    if data.attention.len() > LIST_LIMIT {
        out.push(format!(
            "- … und {} weitere",
            data.attention.len() - LIST_LIMIT
        ));
    }
    if let Some(note) = stale_sources_text(data) {
        out.push(format!("Hinweis: {note}"));
    }
    if data.review_leads > 0 {
        out.push(String::new());
        out.push(format!(
            "PRÜFBEDARF: {} warten auf Prüfung, {} offen",
            plural(data.review_leads, "Lead", "Leads"),
            plural(data.review_fields, "Feld", "Felder")
        ));
        for (name, open) in data.review.iter().take(REVIEW_LIST_LIMIT) {
            out.push(format!("- {name}: {}", plural(*open, "Feld", "Felder")));
        }
        if data.review.len() > REVIEW_LIST_LIMIT {
            out.push(format!(
                "- … und {} weitere",
                data.review.len() - REVIEW_LIST_LIMIT
            ));
        }
    }
    out.push(String::new());
    out.push("GESAMTSTAND".to_string());
    out.push(format!(
        "{} gesamt: {} abgeschlossen, {} mit Prüfbedarf, {} nicht begonnen, {} in Arbeit, {} an Sellify übergeben.",
        plural(data.total, "Lead", "Leads"),
        data.total_completed,
        data.review_leads,
        data.total_new,
        data.running,
        data.total_sellify
    ));
    out.push(String::new());
    out.push(FOOTER_TEXT.to_string());
    out.join("\n")
}

fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// Mail-client safe HTML: tables and inline styles only (Outlook ignores
// <style> blocks and flexbox), explicit light colours so dark-mode clients
// do not invert text onto the wrong ground.
const INK: &str = "#1f2328";
const MUTED: &str = "#656d76";
const LINE: &str = "#d8dee4";
const PANEL: &str = "#f6f8fa";
const ACCENT: &str = "#c2410c";
const DANGER: &str = "#b42318";
const OK: &str = "#1a7f37";
const FONT: &str = "-apple-system,'Segoe UI',Helvetica,Arial,sans-serif";

fn html_section_title(title: &str, note: &str) -> String {
    format!(
        r#"<tr><td style="padding:22px 24px 8px;"><div style="font:600 13px/1.3 {FONT};color:{INK};text-transform:uppercase;letter-spacing:.06em;">{}</div>{}</td></tr>"#,
        esc(title),
        if note.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div style="font:13px/1.4 {FONT};color:{MUTED};padding-top:2px;">{}</div>"#,
                esc(note)
            )
        }
    )
}

fn html_table(headers: &[(&str, &str)], rows: &[Vec<String>]) -> String {
    let head = headers
        .iter()
        .map(|(label, align)| {
            format!(
                r#"<th align="{align}" style="font:600 12px/1.3 {FONT};color:{MUTED};padding:8px 10px;border-bottom:1px solid {LINE};text-align:{align};">{}</th>"#,
                esc(label)
            )
        })
        .collect::<String>();
    let body = rows
        .iter()
        .map(|cells| {
            let tds = cells
                .iter()
                .zip(headers.iter())
                .map(|(cell, (_, align))| {
                    format!(
                        r#"<td align="{align}" style="font:14px/1.4 {FONT};color:{INK};padding:9px 10px;border-bottom:1px solid {LINE};text-align:{align};vertical-align:top;">{cell}</td>"#
                    )
                })
                .collect::<String>();
            format!("<tr>{tds}</tr>")
        })
        .collect::<String>();
    format!(
        r#"<tr><td style="padding:0 14px;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="border-collapse:collapse;"><tr>{head}</tr>{body}</table></td></tr>"#
    )
}

fn html_paragraph(text: &str, color: &str) -> String {
    format!(
        r#"<tr><td style="padding:2px 24px 4px;font:14px/1.5 {FONT};color:{color};">{}</td></tr>"#,
        esc(text)
    )
}

fn more_row(total: usize, shown: usize, columns: usize) -> Option<Vec<String>> {
    (total > shown).then(|| {
        let mut row = vec![format!(
            r#"<span style="color:{MUTED};">… und {} weitere in der App</span>"#,
            total - shown
        )];
        row.extend(std::iter::repeat_n(String::new(), columns - 1));
        row
    })
}

fn render_html(data: &DigestData) -> String {
    let kpi = |value: usize, label: &str, color: &str| {
        format!(
            r#"<td width="25%" align="center" style="padding:14px 6px;border-right:1px solid {LINE};"><div style="font:700 26px/1.1 {FONT};color:{color};">{value}</div><div style="font:12px/1.3 {FONT};color:{MUTED};padding-top:4px;">{}</div></td>"#,
            esc(label)
        )
    };
    let blocker_color = if data.attention.is_empty() {
        OK
    } else {
        DANGER
    };
    let mut rows = vec![
        format!(
            r#"<tr><td style="padding:22px 24px 16px;border-bottom:3px solid {ACCENT};"><div style="font:600 12px/1.3 {FONT};color:{ACCENT};text-transform:uppercase;letter-spacing:.08em;">Outbound-Update</div><div style="font:700 22px/1.25 {FONT};color:{INK};padding-top:4px;">{}</div><div style="font:13px/1.4 {FONT};color:{MUTED};padding-top:4px;">Zeitraum {}</div></td></tr>"#,
            esc(&data.date_label),
            esc(&data.period)
        ),
        format!(
            r#"<tr><td style="padding:16px 14px 0;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="border-collapse:collapse;background:{PANEL};border:1px solid {LINE};"><tr>{}{}{}{}</tr></table></td></tr>"#,
            kpi(data.researched.len(), "Recherchen abgeschlossen", INK),
            kpi(data.verified, "Felder belegt", INK),
            kpi(data.sellify_done.len(), "an Sellify übergeben", INK),
            kpi(data.attention.len(), "Blocker", blocker_color)
                .replace(&format!("border-right:1px solid {LINE};"), ""),
        ),
    ];

    rows.push(html_section_title(
        "Abgeschlossene Recherchen",
        if data.researched.is_empty() {
            ""
        } else {
            "seit dem letzten Update"
        },
    ));
    if data.researched.is_empty() {
        rows.push(html_paragraph(
            "Keine Recherche in diesem Zeitraum abgeschlossen.",
            MUTED,
        ));
    } else {
        let mut table_rows = data
            .researched
            .iter()
            .take(LIST_LIMIT)
            .map(|row| {
                vec![
                    format!("<strong>{}</strong>", esc(&row.name)),
                    format!("{} / {}", row.verified, row.fields),
                    row.contacts.to_string(),
                    if row.open > 0 {
                        format!(r#"<span style="color:{DANGER};">{}</span>"#, row.open)
                    } else {
                        "–".to_string()
                    },
                ]
            })
            .collect::<Vec<_>>();
        table_rows.extend(more_row(data.researched.len(), LIST_LIMIT, 4));
        rows.push(html_table(
            &[
                ("Firma", "left"),
                ("Felder belegt", "right"),
                ("Ansprechpartner", "right"),
                ("zu prüfen", "right"),
            ],
            &table_rows,
        ));
    }

    if !data.sellify_done.is_empty() {
        rows.push(html_section_title("An Sellify übergeben", ""));
        let table_rows = data
            .sellify_done
            .iter()
            .take(LIST_LIMIT)
            .map(|name| vec![esc(name)])
            .collect::<Vec<_>>();
        rows.push(html_table(&[("Firma", "left")], &table_rows));
    }

    rows.push(html_section_title(
        "Blocker",
        if data.attention.is_empty() {
            ""
        } else {
            "braucht eine Entscheidung oder einen Handgriff"
        },
    ));
    if data.attention.is_empty() {
        rows.push(html_paragraph("Keine Blocker.", OK));
    } else {
        let mut table_rows = data
            .attention
            .iter()
            .take(LIST_LIMIT)
            .map(|row| {
                vec![
                    format!(
                        r#"<strong>{}</strong><div style="font:12px/1.3 {FONT};color:{DANGER};padding-top:2px;">{}</div>"#,
                        esc(&row.subject),
                        esc(row.kind)
                    ),
                    esc(&row.detail),
                ]
            })
            .collect::<Vec<_>>();
        table_rows.extend(more_row(data.attention.len(), LIST_LIMIT, 2));
        rows.push(html_table(
            &[("Betrifft", "left"), ("Was ist los", "left")],
            &table_rows,
        ));
    }
    if let Some(note) = stale_sources_text(data) {
        rows.push(html_paragraph(&note, MUTED));
    }

    if data.review_leads > 0 {
        rows.push(html_section_title(
            "Prüfbedarf",
            &format!(
                "{} warten auf Prüfung und Freigabe, {} offen",
                plural(data.review_leads, "Lead", "Leads"),
                plural(data.review_fields, "Feld", "Felder")
            ),
        ));
        if !data.review.is_empty() {
            let mut table_rows = data
                .review
                .iter()
                .take(REVIEW_LIST_LIMIT)
                .map(|(name, open)| vec![esc(name), open.to_string()])
                .collect::<Vec<_>>();
            table_rows.extend(more_row(data.review.len(), REVIEW_LIST_LIMIT, 2));
            rows.push(html_table(
                &[("Firma", "left"), ("offene Felder", "right")],
                &table_rows,
            ));
        }
    }

    rows.push(html_section_title("Gesamtstand", ""));
    rows.push(html_table(
        &[
            ("Leads", "right"),
            ("abgeschlossen", "right"),
            ("Prüfbedarf", "right"),
            ("nicht begonnen", "right"),
            ("in Arbeit", "right"),
            ("an Sellify", "right"),
        ],
        &[vec![
            format!("<strong>{}</strong>", data.total),
            data.total_completed.to_string(),
            data.review_leads.to_string(),
            data.total_new.to_string(),
            data.running.to_string(),
            data.total_sellify.to_string(),
        ]],
    ));
    rows.push(format!(
        r#"<tr><td style="padding:24px 24px 22px;font:12px/1.5 {FONT};color:{MUTED};">{}</td></tr>"#,
        esc(FOOTER_TEXT)
    ));
    format!(
        r#"<!DOCTYPE html><html lang="de"><head><meta charset="utf-8"><meta name="color-scheme" content="light"><meta name="supported-color-schemes" content="light"><title>Outbound-Update</title></head><body style="margin:0;padding:0;background:#eef1f4;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#eef1f4;"><tr><td align="center" style="padding:20px 10px;"><table role="presentation" width="640" cellpadding="0" cellspacing="0" style="width:100%;max-width:640px;background:#ffffff;border:1px solid {LINE};border-collapse:collapse;">{}</table></td></tr></table></body></html>"#,
        rows.join("")
    )
}

pub(crate) fn build_report(
    leads: &[Value],
    sources: &[Value],
    adapters: &[Value],
    since_ms: i64,
    now_ms: i64,
    tz: Tz,
) -> DigestReport {
    let data = collect_digest(leads, sources, adapters, since_ms, now_ms, tz);
    DigestReport {
        subject: digest_subject(&data),
        body: render_text(&data),
        html: render_html(&data),
        stats: json!({
            "since_ms": since_ms,
            "until_ms": now_ms,
            "researched": data.researched.len(),
            "verified_fields": data.verified,
            "contacts": data.contacts,
            "sellify_handovers": data.sellify_done.len(),
            "blockers": data.attention.len(),
            "stale_sources": data.stale_sources.len(),
            "needs_review": data.review_leads,
            "leads_total": data.total,
        }),
    }
}

fn state_path(root: &Path) -> PathBuf {
    crate::paths::runtime_dir(root).join(STATE_FILE)
}

fn load_state(root: &Path) -> DigestState {
    std::fs::read(state_path(root))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn save_state(root: &Path, state: &DigestState) -> Result<()> {
    let path = state_path(root);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn load_config(root: &Path) -> Result<Option<DigestConfig>> {
    let Some(doc) = store::load_rxdb_collection_record(root, POLICY_COLLECTION, DIGEST_CONFIG_ID)?
    else {
        return Ok(None);
    };
    if doc.get("_deleted").and_then(Value::as_bool) == Some(true) {
        return Ok(None);
    }
    Ok(parse_config(&doc))
}

fn resolve_sender(root: &Path, config: &DigestConfig) -> Option<String> {
    if is_plausible_email(&config.sender_email) {
        return Some(config.sender_email.clone());
    }
    crate::inference::runtime_env::effective_operator_env_map(root)
        .ok()?
        .get("CTO_EMAIL_ADDRESS")
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| is_plausible_email(value))
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn publish_status(root: &Path, state: &DigestState, extra: Value) {
    let now = now_millis();
    let mut doc = json!({
        "id": DIGEST_STATUS_ID,
        "title": "Update-Verteiler Status",
        "status": "digest_status",
        // Required by the collection schema (shared with research policies).
        "version_number": 1,
        "skill_name": "",
        "skill_version": "",
        "min_independent_sources": 0,
        "rules": [],
        "last_sent_slot": state.last_sent_slot,
        "last_sent_at_ms": state.last_sent_at_ms,
        "last_attempt_at_ms": state.last_attempt_at_ms,
        "attempts": state.attempts,
        "last_error": state.last_error,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    if let (Some(target), Some(source)) = (doc.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    if let Err(error) =
        store::upsert_rxdb_collection_record(root, POLICY_COLLECTION, DIGEST_STATUS_ID, now, doc)
    {
        eprintln!("[outbound-digest] status projection failed: {error}");
    }
}

fn build_current_report(
    root: &Path,
    config: &DigestConfig,
    since_ms: i64,
    now_ms: i64,
) -> Result<DigestReport> {
    let leads = store::load_rxdb_collection_records(root, LEAD_COLLECTION)?;
    let sources = store::load_rxdb_collection_records(root, SOURCE_COLLECTION)?;
    let adapters = store::load_rxdb_collection_records(root, ADAPTER_COLLECTION)?;
    Ok(build_report(
        &leads,
        &sources,
        &adapters,
        since_ms,
        now_ms,
        config.timezone,
    ))
}

fn send_report(
    root: &Path,
    config: &DigestConfig,
    report: &DigestReport,
    slot_key: &str,
    recipients: &[String],
) -> Result<Value> {
    let sender = resolve_sender(root, config)
        .context("kein Absender-Postfach konfiguriert und keine Instanz-Adresse vorhanden")?;
    channels::record_and_send_policy_report_email(
        root,
        &channels::PolicyReportEmail {
            sender_email: &sender,
            to: recipients,
            subject: &report.subject,
            body: &report.body,
            body_html: Some(&report.html),
            report_key: slot_key,
            policy_summary: "Outbound Update-Verteiler: Empfänger, Zeitplan und Absender vom Admin in der Outbound-App konfiguriert; Inhalt deterministisch aus den App-Datensätzen.",
        },
    )
}

static LAST_TICK: Mutex<Option<Instant>> = Mutex::new(None);

/// Maintenance-loop entry: sends the configured update when its slot is due.
/// Returns a one-line event when something was sent or failed.
pub fn tick(root: &Path) -> Option<String> {
    {
        let mut last = LAST_TICK.lock().ok()?;
        if last.is_some_and(|at| at.elapsed() < TICK_EVERY) {
            return None;
        }
        *last = Some(Instant::now());
    }
    let config = match load_config(root) {
        Ok(Some(config)) => config,
        Ok(None) => return None,
        Err(error) => {
            return Some(format!(
                "Outbound-Update: Konfiguration nicht lesbar: {error}"
            ))
        }
    };
    let now_ms = now_millis();
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms)?;
    let mut state = load_state(root);
    let slot = due_slot(&config, now, &state)?;
    if state.attempt_slot != slot {
        state.attempt_slot = slot.clone();
        state.attempts = 0;
    }
    state.attempts += 1;
    state.last_attempt_at_ms = now_ms;
    let since_ms = if state.last_sent_at_ms > 0 {
        state.last_sent_at_ms
    } else {
        now_ms - FIRST_REPORT_WINDOW_MS
    };
    let slot_key = format!("outbound-update:{slot}");
    let outcome = build_current_report(root, &config, since_ms, now_ms).and_then(|report| {
        send_report(root, &config, &report, &slot_key, &config.recipients).map(|_| report)
    });
    let event = match outcome {
        Ok(report) => {
            state.last_sent_slot = slot;
            state.last_sent_at_ms = now_ms;
            state.last_error.clear();
            let _ = save_state(root, &state);
            publish_status(
                root,
                &state,
                json!({
                    "last_subject": report.subject,
                    "last_recipients": config.recipients,
                    "last_stats": report.stats,
                    "last_kind": "scheduled",
                }),
            );
            format!(
                "Outbound-Update gesendet an {} Empfänger: {}",
                config.recipients.len(),
                report.subject
            )
        }
        Err(error) => {
            state.last_error = error.to_string().chars().take(400).collect();
            let _ = save_state(root, &state);
            publish_status(root, &state, json!({ "last_kind": "scheduled" }));
            format!(
                "Outbound-Update fehlgeschlagen (Versuch {}/{}): {}",
                state.attempts, MAX_ATTEMPTS_PER_SLOT, state.last_error
            )
        }
    };
    Some(event)
}

/// `outbound.update_digest.send_now`: sends the current update immediately to
/// the configured (or explicitly given) recipients. It does not move the
/// schedule's "since" marker, so the next scheduled update still covers the
/// full period.
pub(super) fn send_now(root: &Path, payload: &Value) -> Result<Value> {
    // The drawer may send its unsaved form so "Vorschau" works before saving.
    let config = match payload
        .get("update_digest")
        .and_then(|_| parse_config(payload))
    {
        Some(config) => config,
        None => load_config(root)?.context(
            "Update-Verteiler ist noch nicht gespeichert. Bitte Empfänger und Zeitplan speichern.",
        )?,
    };
    let dry_run = payload.get("dry_run").and_then(Value::as_bool) == Some(true);
    let recipients = {
        let explicit = parse_recipients(payload.get("recipients"));
        if explicit.is_empty() {
            config.recipients.clone()
        } else {
            explicit
        }
    };
    anyhow::ensure!(
        dry_run || !recipients.is_empty(),
        "Keine gültige Empfängeradresse hinterlegt."
    );
    let now_ms = now_millis();
    let mut state = load_state(root);
    let since_ms = if state.last_sent_at_ms > 0 {
        state.last_sent_at_ms
    } else {
        now_ms - FIRST_REPORT_WINDOW_MS
    };
    let report = build_current_report(root, &config, since_ms, now_ms)?;
    // "Vorschau": the exact text the next update would carry, nothing is sent.
    if dry_run {
        return Ok(json!({
            "ok": true,
            "dry_run": true,
            "subject": report.subject,
            "body": report.body,
            "html": report.html,
            "recipients": recipients,
            "sender": resolve_sender(root, &config),
            "stats": report.stats,
        }));
    }
    let slot_key = format!("outbound-update-test:{now_ms}");
    let result = send_report(root, &config, &report, &slot_key, &recipients);
    state.last_attempt_at_ms = now_ms;
    match &result {
        Ok(_) => state.last_error.clear(),
        Err(error) => state.last_error = error.to_string().chars().take(400).collect(),
    }
    let _ = save_state(root, &state);
    publish_status(
        root,
        &state,
        json!({
            "last_test_at_ms": now_ms,
            "last_test_ok": result.is_ok(),
            "last_test_recipients": recipients,
            "last_test_subject": report.subject,
            "last_kind": "test",
        }),
    );
    let delivery = result?;
    Ok(json!({
        "ok": true,
        "subject": report.subject,
        "recipients": recipients,
        "stats": report.stats,
        "delivery_status": delivery.get("status").cloned().unwrap_or(Value::Null),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DigestConfig {
        parse_config(&json!({
            "id": DIGEST_CONFIG_ID,
            "update_digest": {
                "enabled": true,
                "recipients": "Lena.Ogiermann@thesen-ag.com; kaputt, vertrieb@thesen-ag.com",
                "weekdays": [1, 2, 3, 4, 5, 9],
                "time": "07:00",
                "timezone": "Europe/Berlin",
                "sender_email": "Lena.Ogiermann@thesen-ag.com"
            }
        }))
        .expect("config")
    }

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn config_keeps_only_valid_recipients_and_weekdays() {
        let config = config();
        assert_eq!(
            config.recipients,
            vec![
                "lena.ogiermann@thesen-ag.com".to_string(),
                "vertrieb@thesen-ag.com".to_string()
            ]
        );
        assert_eq!(config.weekdays, vec![1, 2, 3, 4, 5]);
        assert_eq!(config.minute_of_day, 7 * 60);
        assert_eq!(config.sender_email, "lena.ogiermann@thesen-ag.com");
        assert!(parse_config(&json!({"update_digest": {"time": "25:00"}})).is_none());
    }

    #[test]
    fn slot_is_due_once_per_local_weekday_after_the_configured_time() {
        let config = config();
        let mut state = DigestState::default();
        // Wednesday 23.09.2026, 06:59 Berlin (CEST, UTC+2) -> not yet.
        assert_eq!(due_slot(&config, utc("2026-09-23T04:59:00Z"), &state), None);
        // 07:00 Berlin -> due.
        assert_eq!(
            due_slot(&config, utc("2026-09-23T05:00:00Z"), &state).as_deref(),
            Some("2026-09-23")
        );
        state.last_sent_slot = "2026-09-23".to_string();
        assert_eq!(due_slot(&config, utc("2026-09-23T06:00:00Z"), &state), None);
        // Saturday is not configured.
        assert_eq!(due_slot(&config, utc("2026-09-26T05:30:00Z"), &state), None);
        // Winter time: Monday 07.12.2026 07:00 Berlin is 06:00 UTC.
        assert_eq!(due_slot(&config, utc("2026-12-07T05:30:00Z"), &state), None);
        assert!(due_slot(&config, utc("2026-12-07T06:00:00Z"), &state).is_some());
        // After the send window the day is skipped instead of mailing at night.
        assert_eq!(due_slot(&config, utc("2026-12-07T19:00:00Z"), &state), None);
    }

    #[test]
    fn failed_slot_retries_with_backoff_and_gives_up() {
        let config = config();
        let now = utc("2026-09-23T05:30:00Z");
        let mut state = DigestState {
            attempt_slot: "2026-09-23".to_string(),
            attempts: 1,
            last_attempt_at_ms: now.timestamp_millis() - 60_000,
            ..DigestState::default()
        };
        assert_eq!(due_slot(&config, now, &state), None);
        state.last_attempt_at_ms = now.timestamp_millis() - RETRY_AFTER_MS;
        assert!(due_slot(&config, now, &state).is_some());
        state.attempts = MAX_ATTEMPTS_PER_SLOT;
        assert_eq!(due_slot(&config, now, &state), None);
    }

    #[test]
    fn disabled_or_recipientless_config_never_sends() {
        let mut config = config();
        let now = utc("2026-09-23T05:30:00Z");
        config.enabled = false;
        assert_eq!(due_slot(&config, now, &DigestState::default()), None);
        config.enabled = true;
        config.recipients.clear();
        assert_eq!(due_slot(&config, now, &DigestState::default()), None);
    }

    #[test]
    fn report_counts_only_measured_changes_in_the_window() {
        let since = utc("2026-09-22T05:00:00Z").timestamp_millis();
        let now = utc("2026-09-23T05:00:00Z").timestamp_millis();
        let inside = since + 3_600_000;
        let before = since - 3_600_000;
        let leads = vec![
            json!({
                "id": "lead_a", "name": "Carbosulf Chemische Werke GmbH",
                "research_status": "completed", "sellify_status": "completed",
                "payload": {"research_finished_at_ms": inside, "sellify_finished_at_ms": inside},
                "field_status": {
                    "firma_name": {"status": "verified"},
                    "firma_ort": {"status": "verified"},
                    "umsatz": {"status": "no_match"}
                },
                "contacts": [{"id": "c1"}, {"id": "c2"}]
            }),
            json!({
                "id": "lead_b", "name": "Alte Recherche AG",
                "research_status": "needs_review",
                "payload": {"research_finished_at_ms": before},
                "field_status": {"umsatz": {"status": "action_required"}}
            }),
            json!({
                "id": "lead_c", "name": "Kaputt GmbH", "research_status": "failed",
                "research_updated_at_ms": inside,
                "research_error": "Quelle northdata.de nicht erreichbar\nstack trace"
            }),
            json!({
                "id": "lead_d", "name": "Haengt KG", "research_status": "running",
                "research_updated_at_ms": now - STUCK_RESEARCH_MS - 1
            }),
            json!({
                "id": "lead_t", "name": "UITEST-Probe", "research_status": "completed",
                "payload": {"research_finished_at_ms": inside}
            }),
        ];
        let sources = vec![
            json!({"id": "xing.com", "label": "XING", "enabled": true}),
            json!({"id": "northdata.de", "label": "North Data", "enabled": true}),
            json!({"id": "moneyhouse.ch", "label": "Moneyhouse", "enabled": true}),
            json!({"id": "sellify", "label": "Sellify", "enabled": true}),
        ];
        let adapters = vec![
            json!({"source_id": "xing.com", "status": "test_temporary_unreachable", "updated_at_ms": inside}),
            json!({"source_id": "northdata.de", "status": "failed",
                   "last_error": "business command `cmd_outbound_adapter_reconcile_x` failed", "updated_at_ms": inside}),
            json!({"source_id": "moneyhouse.ch", "status": "test_temporary_unreachable",
                   "updated_at_ms": now - 5 * 24 * 3_600_000}),
        ];
        let report = build_report(
            &leads,
            &sources,
            &adapters,
            since,
            now,
            chrono_tz::Europe::Berlin,
        );
        assert_eq!(report.stats["researched"], 1);
        assert_eq!(report.stats["verified_fields"], 2);
        assert_eq!(report.stats["contacts"], 2);
        assert_eq!(report.stats["sellify_handovers"], 1);
        // failed research + stuck research + XING; reconcile noise and the
        // five-day-old Moneyhouse test are not blockers.
        assert_eq!(report.stats["blockers"], 3);
        assert_eq!(report.stats["stale_sources"], 1);
        assert_eq!(
            report.stats["leads_total"], 4,
            "UITEST leads stay out of the report"
        );
        assert!(report.body.contains(
            "- Carbosulf Chemische Werke GmbH: 2 von 3 Feldern belegt, 2 Ansprechpartner"
        ));
        assert!(report.body.contains(
            "- Recherche fehlgeschlagen: Kaputt GmbH – Quelle northdata.de nicht erreichbar"
        ));
        assert!(report.body.contains(
            "- Quelle gestört: XING – bei der letzten Prüfung nicht erreichbar (geprüft 22.09.)"
        ));
        assert!(report
            .body
            .contains("1 Quelle seit über drei Tagen nicht neu geprüft"));
        assert!(!report.body.contains("UITEST"));
        assert_eq!(
            report.subject,
            "Outbound-Update Mi 23.09.2026: 1 Recherche abgeschlossen, 1 an Sellify, 3 Blocker"
        );
        assert!(report
            .html
            .contains("<strong>Carbosulf Chemische Werke GmbH</strong>"));
        assert!(report.html.contains(">Blocker<"));
        assert!(!report.html.contains("UITEST"));
        channels::ensure_founder_outbound_body_text_clean(&report.body)
            .expect("report body passes the outbound body gate");
        channels::ensure_founder_outbound_body_text_clean(&report.html)
            .expect("report html passes the outbound body gate");
    }

    #[test]
    fn html_escapes_record_text() {
        let now = utc("2026-09-23T05:00:00Z").timestamp_millis();
        let leads = vec![json!({
            "id": "x", "name": "A&B <script>", "research_status": "completed",
            "payload": {"research_finished_at_ms": now - 1000}
        })];
        let report = build_report(
            &leads,
            &[],
            &[],
            now - 3_600_000,
            now,
            chrono_tz::Europe::Berlin,
        );
        assert!(report.html.contains("A&amp;B &lt;script&gt;"));
        assert!(!report.html.contains("<script>"));
    }

    #[test]
    fn internal_error_text_never_reaches_the_mail() {
        assert_eq!(
            short_reason("failed to open runtime/ctox.sqlite3: locked").as_deref(),
            Some("technischer Fehler, Details in der App")
        );
        assert_eq!(short_reason("   "), None);
    }
}
