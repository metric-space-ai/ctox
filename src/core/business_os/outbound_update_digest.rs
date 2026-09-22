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

fn push_limited(lines: &mut Vec<String>, items: &[String], indent: &str) {
    for item in items.iter().take(LIST_LIMIT) {
        lines.push(format!("{indent}· {item}"));
    }
    if items.len() > LIST_LIMIT {
        lines.push(format!(
            "{indent}· … und {} weitere",
            items.len() - LIST_LIMIT
        ));
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub(crate) fn build_report(
    leads: &[Value],
    sources: &[Value],
    adapters: &[Value],
    since_ms: i64,
    now_ms: i64,
    tz: Tz,
) -> DigestReport {
    let in_window = |ms: i64| ms > since_ms && ms <= now_ms;
    let leads = leads
        .iter()
        .filter(|lead| !is_test_lead(lead))
        .collect::<Vec<_>>();

    let mut researched = Vec::new();
    let mut failed = Vec::new();
    let mut stuck = Vec::new();
    let mut sellify_done = Vec::new();
    let mut sellify_failed = Vec::new();
    let mut review_leads = 0usize;
    let mut review_fields = 0usize;
    let mut running = 0usize;
    let (mut total_completed, mut total_new, mut total_sellify) = (0usize, 0usize, 0usize);
    let mut verified_in_window = 0usize;
    let mut contacts_in_window = 0usize;

    for lead in &leads {
        let status = text(lead, "research_status");
        let (verified, open, fields) = field_counts(lead);
        match status {
            "completed" => total_completed += 1,
            "needs_review" | "partially_completed" => {
                review_leads += 1;
                review_fields += open;
            }
            "" | "new" => total_new += 1,
            "running" | "queued" | "requested" => {
                running += 1;
                let updated = millis(lead, "research_updated_at_ms");
                if updated > 0 && now_ms - updated > STUCK_RESEARCH_MS {
                    stuck.push(format!(
                        "{} (seit {})",
                        lead_name(lead),
                        local_label(tz, updated, true)
                    ));
                }
            }
            _ => {}
        }
        if matches!(status, "completed" | "needs_review" | "partially_completed")
            && in_window(research_finished_ms(lead))
        {
            verified_in_window += verified;
            let contacts = contact_count(lead);
            contacts_in_window += contacts;
            let mut line = format!(
                "{}: {verified} von {fields} Feldern belegt",
                lead_name(lead)
            );
            if contacts > 0 {
                line.push_str(&format!(
                    ", {}",
                    plural(contacts, "Ansprechpartner", "Ansprechpartner")
                ));
            }
            if open > 0 {
                line.push_str(&format!(", {} zu prüfen", plural(open, "Feld", "Felder")));
            }
            researched.push(line);
        }
        if status == "failed" && in_window(millis(lead, "research_updated_at_ms")) {
            let reason = lead
                .get("research_error")
                .and_then(Value::as_str)
                .and_then(short_reason);
            failed.push(match reason {
                Some(reason) => format!("{} ({reason})", lead_name(lead)),
                None => lead_name(lead),
            });
        }
        let sellify_status = text(lead, "sellify_status");
        if sellify_status == "completed" {
            total_sellify += 1;
            let finished = lead
                .get("payload")
                .map(|payload| millis(payload, "sellify_finished_at_ms"))
                .unwrap_or(0);
            if in_window(finished) {
                sellify_done.push(lead_name(lead));
            }
        } else if sellify_status == "failed" {
            sellify_failed.push(lead_name(lead));
        }
    }

    let mut source_problems = Vec::new();
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
        if let Some(problem) = source_problem(source, adapter) {
            let label = match text(source, "label") {
                "" => source_id,
                label => label,
            };
            let checked = adapter
                .map(|adapter| millis(adapter, "updated_at_ms"))
                .filter(|ms| *ms > 0)
                .map(|ms| format!(" (Stand {})", local_label(tz, ms, false)))
                .unwrap_or_default();
            source_problems.push(format!("{label}: {problem}{checked}"));
        }
    }
    source_problems.sort();

    let now_local = tz.timestamp_millis_opt(now_ms).single();
    let greeting = match now_local.map(|stamp| stamp.hour()).unwrap_or(8) {
        0..=10 => "Guten Morgen,",
        11..=17 => "Guten Tag,",
        _ => "Guten Abend,",
    };
    let mut lines = vec![
        greeting.to_string(),
        String::new(),
        format!(
            "hier das Update aus der Outbound-Recherche für den Zeitraum {} bis {}.",
            local_label(tz, since_ms, true),
            local_label(tz, now_ms, true)
        ),
        String::new(),
        "ERREICHT".to_string(),
    ];
    if researched.is_empty() {
        lines.push("- Keine Recherche abgeschlossen.".to_string());
    } else {
        lines.push(format!(
            "- {} abgeschlossen, {} belegt, {} gefunden:",
            plural(researched.len(), "Recherche", "Recherchen"),
            plural(verified_in_window, "Feld", "Felder"),
            plural(contacts_in_window, "Ansprechpartner", "Ansprechpartner")
        ));
        push_limited(&mut lines, &researched, "  ");
    }
    if sellify_done.is_empty() {
        lines.push("- Keine Übergabe an Sellify.".to_string());
    } else {
        lines.push(format!(
            "- {} an Sellify übergeben:",
            plural(sellify_done.len(), "Lead", "Leads")
        ));
        push_limited(&mut lines, &sellify_done, "  ");
    }

    lines.push(String::new());
    lines.push("OFFEN UND BLOCKER".to_string());
    let blocker_count = failed.len() + stuck.len() + sellify_failed.len() + source_problems.len();
    if !failed.is_empty() {
        lines.push(format!(
            "- {} fehlgeschlagen:",
            plural(failed.len(), "Recherche", "Recherchen")
        ));
        push_limited(&mut lines, &failed, "  ");
    }
    if !stuck.is_empty() {
        lines.push(format!(
            "- {} ohne Fortschritt seit über zwei Stunden:",
            plural(stuck.len(), "Recherche", "Recherchen")
        ));
        push_limited(&mut lines, &stuck, "  ");
    }
    if !sellify_failed.is_empty() {
        lines.push("- Sellify-Übergabe fehlgeschlagen:".to_string());
        push_limited(&mut lines, &sellify_failed, "  ");
    }
    if !source_problems.is_empty() {
        lines.push(format!(
            "- {} mit Störung:",
            plural(source_problems.len(), "Quelle", "Quellen")
        ));
        push_limited(&mut lines, &source_problems, "  ");
    }
    if review_leads > 0 {
        lines.push(format!(
            "- {} mit Prüfbedarf ({} offen).",
            plural(review_leads, "Lead", "Leads"),
            plural(review_fields, "Feld", "Felder")
        ));
    }
    if blocker_count == 0 && review_leads == 0 {
        lines.push("- Nichts offen.".to_string());
    }

    lines.push(String::new());
    lines.push("GESAMTSTAND".to_string());
    lines.push(format!(
        "- {} in der Recherche: {total_completed} abgeschlossen, {review_leads} mit Prüfbedarf, {total_new} noch nicht begonnen, {running} in Arbeit.",
        plural(leads.len(), "Lead", "Leads")
    ));
    lines.push(format!(
        "- {} an Sellify übergeben.",
        plural(total_sellify, "Lead", "Leads")
    ));
    lines.push(String::new());
    lines.push(
        "Automatisch erstellt aus der Outbound-App. Empfänger und Zeitplan: Outbound-App, Recherche-Einstellungen, Update-Verteiler."
            .to_string(),
    );

    let date = now_local
        .map(|stamp| stamp.format("%d.%m.%Y").to_string())
        .unwrap_or_default();
    let mut subject = format!(
        "Outbound-Update {date}: {} abgeschlossen, {} an Sellify",
        plural(researched.len(), "Recherche", "Recherchen"),
        sellify_done.len()
    );
    if blocker_count > 0 {
        subject.push_str(&format!(
            ", {}",
            plural(blocker_count, "Blocker", "Blocker")
        ));
    }
    DigestReport {
        subject,
        body: lines.join("\n"),
        stats: json!({
            "since_ms": since_ms,
            "until_ms": now_ms,
            "researched": researched.len(),
            "verified_fields": verified_in_window,
            "contacts": contacts_in_window,
            "sellify_handovers": sellify_done.len(),
            "failed": failed.len(),
            "stuck": stuck.len(),
            "source_problems": source_problems.len(),
            "needs_review": review_leads,
            "blockers": blocker_count,
            "leads_total": leads.len(),
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
            json!({"id": "sellify", "label": "Sellify", "enabled": true}),
        ];
        let adapters = vec![
            json!({"source_id": "xing.com", "status": "test_temporary_unreachable", "updated_at_ms": inside}),
            json!({"source_id": "northdata.de", "status": "failed",
                   "last_error": "business command `cmd_outbound_adapter_reconcile_x` failed", "updated_at_ms": inside}),
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
        assert_eq!(report.stats["failed"], 1);
        assert_eq!(report.stats["stuck"], 1);
        assert_eq!(
            report.stats["source_problems"], 1,
            "reconcile noise is not an outage"
        );
        assert_eq!(
            report.stats["leads_total"], 4,
            "UITEST leads stay out of the report"
        );
        assert!(report
            .body
            .contains("Carbosulf Chemische Werke GmbH: 2 von 3 Feldern belegt, 2 Ansprechpartner"));
        assert!(report
            .body
            .contains("Kaputt GmbH (Quelle northdata.de nicht erreichbar)"));
        assert!(report
            .body
            .contains("XING: bei der letzten Prüfung nicht erreichbar"));
        assert!(!report.body.contains("UITEST"));
        assert!(report
            .subject
            .starts_with("Outbound-Update 23.09.2026: 1 Recherche abgeschlossen, 1 an Sellify"));
        channels::ensure_founder_outbound_body_text_clean(&report.body)
            .expect("report body passes the outbound body gate");
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
