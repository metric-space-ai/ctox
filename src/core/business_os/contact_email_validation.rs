//! Native e-mail validation for outbound leads.
//!
//! The release gate of the outbound app demands a contact whose e-mail address
//! has been checked. A contact address without a validation verdict does not
//! make a lead ready for handoff to the CRM.
//!
//! The check itself works. The registered scrape target `experte-de` answers a
//! question about ONE address, driven like this:
//!
//! ```text
//! ctox scrape execute --target-key experte-de --trigger-kind manual \
//!   --input-json '{"email":"info@weicon.de"}'
//! -> person_email_validation = "valid"
//!    note: "EXPERTE.de verdict: info@weicon.de | Gültig"
//! ```
//!
//! Nobody ever gave it an address. The compile-time research path calls the
//! target with company and country only (27 `portal_drift` runs, all of them
//! `CTOX_SCRAPE_INPUT_JSON.email missing`), and the research worker cannot make
//! the call itself because its sandbox denies the ctox CLI.
//!
//! So the daemon checks every contact address itself, after a research result
//! or a writeback has been stored, and attaches the verdict to exactly the
//! contact that owns the address. It reads the verdict from the run's own
//! output: the target keeps one shared record per (field, source_url) and
//! overwrites it on every run, so the shared store cannot say which address a
//! verdict belongs to.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::store;

const LEAD_COLLECTION: &str = "outbound_lead_generation_leads";
const VALIDATION_TARGET_KEY: &str = "experte-de";
const VALIDATION_SOURCE_ID: &str = "experte.de";
const MAX_ADDRESSES_PER_PASS: usize = 4;

/// A checked address and what the validator said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EmailVerdict {
    pub email: String,
    pub valid: bool,
    pub note: String,
    pub source_url: String,
    pub run_id: String,
}

fn normalize_email(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_ascii_lowercase();
    let (local, host) = value.split_once('@')?;
    if local.is_empty() || !host.contains('.') || value.contains(char::is_whitespace) {
        return None;
    }
    // A documentation host proves nothing; a worker probing the target used
    // `max.muster@example.de` three times on 10.09.2026.
    let reserved = [
        "example.com",
        "example.org",
        "example.net",
        "example.de",
        "example.edu",
    ];
    if reserved.contains(&host) || host.ends_with(".example") || host.ends_with(".invalid") {
        return None;
    }
    Some(value)
}

fn contact_email(contact: &Value) -> Option<String> {
    ["person_email", "email"]
        .iter()
        .filter_map(|key| contact.get(*key).and_then(Value::as_str))
        .find_map(normalize_email)
}

/// Only a real verdict counts. A worker writing `no_match` into
/// `person_email_validation` could not run the check. Treating any non-empty
/// value as a verdict would have skipped exactly that address forever.
pub(super) fn is_email_verdict(value: &str) -> bool {
    let value = value.trim().to_lowercase();
    [
        "valid",
        "invalid",
        "gültig",
        "gueltig",
        "ungültig",
        "ungueltig",
        "zustellbar",
        "unzustellbar",
        "bestätigt",
        "bestaetigt",
    ]
    .iter()
    .any(|word| value.contains(word))
}

fn contact_has_verdict(contact: &Value) -> bool {
    ["person_email_validation", "email_validation"]
        .iter()
        .filter_map(|key| contact.get(*key).and_then(Value::as_str))
        .any(is_email_verdict)
}

/// Addresses on this lead that carry no verdict yet, deduplicated, capped.
pub(super) fn emails_needing_validation(lead: &Value, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    lead.get("contacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|contact| !contact_has_verdict(contact))
        .filter_map(contact_email)
        .filter(|email| seen.insert(email.clone()))
        .take(limit)
        .collect()
}

/// Reads "EXPERTE.de verdict: <address> | <label>" into the address and
/// whether the label says the address is deliverable. "Ungültig" contains
/// "gültig" and "invalid" contains "valid", so the negative words are checked
/// first.
pub(super) fn parse_verdict_note(note: &str) -> Option<(String, bool)> {
    let (_, rest) = note.split_once("verdict:")?;
    let (address, label) = rest.split_once('|')?;
    let email = normalize_email(address)?;
    let label = label.trim().to_lowercase();
    let negative = [
        "ungültig",
        "ungueltig",
        "invalid",
        "unzustellbar",
        "nicht",
        "unbekannt",
    ];
    let positive = ["gültig", "gueltig", "valid", "zustellbar"];
    let valid = if negative.iter().any(|word| label.contains(word)) {
        false
    } else if positive.iter().any(|word| label.contains(word)) {
        true
    } else {
        return None;
    };
    Some((email, valid))
}

/// The verdict for `requested` from one run's `result.json` records. A verdict
/// for any other address is ignored.
pub(super) fn verdict_from_records(
    records: &[Value],
    requested: &str,
    run_id: &str,
) -> Option<EmailVerdict> {
    let requested = normalize_email(requested)?;
    records.iter().find_map(|record| {
        if record.get("field").and_then(Value::as_str) != Some("person_email_validation") {
            return None;
        }
        let note = record
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (email, valid) = parse_verdict_note(note)?;
        if email != requested {
            return None;
        }
        Some(EmailVerdict {
            email,
            valid,
            note: note.trim().to_string(),
            source_url: record
                .get("source_url")
                .and_then(Value::as_str)
                .unwrap_or("https://www.experte.de/email-pruefen")
                .to_string(),
            run_id: run_id.to_string(),
        })
    })
}

/// Attaches each verdict to every contact that owns the checked address and
/// records it as evidence. Returns how many contacts changed.
pub(super) fn apply_email_verdicts(lead: &mut Value, verdicts: &[EmailVerdict]) -> usize {
    if verdicts.is_empty() {
        return 0;
    }
    let mut changed = 0;
    let mut new_evidence = Vec::new();
    if let Some(contacts) = lead.get_mut("contacts").and_then(Value::as_array_mut) {
        for contact in contacts.iter_mut() {
            let Some(email) = contact_email(contact) else {
                continue;
            };
            let Some(verdict) = verdicts.iter().find(|verdict| verdict.email == email) else {
                continue;
            };
            let label = if verdict.valid { "valid" } else { "invalid" };
            contact["person_email_validation"] = Value::String(label.to_string());
            changed += 1;
            new_evidence.push(json!({
                "field_key": "person_email_validation",
                "value": label,
                "source_id": VALIDATION_SOURCE_ID,
                "source_url": verdict.source_url,
                "quote": verdict.note,
                "person_key": contact.get("person_key").cloned().unwrap_or(Value::Null),
                "validated_email": verdict.email,
                "run_id": verdict.run_id,
                "via": "native_email_validation",
                "label": VALIDATION_SOURCE_ID,
                "evidence_gate": {"verification_status": "verified", "evidence_eligible": true},
            }));
        }
    }
    if changed == 0 {
        return 0;
    }
    if !lead.get("evidence").is_some_and(Value::is_array) {
        lead["evidence"] = Value::Array(Vec::new());
    }
    if let Some(evidence) = lead.get_mut("evidence").and_then(Value::as_array_mut) {
        for entry in new_evidence.iter() {
            let duplicate = evidence.iter().any(|existing| {
                existing.get("field_key") == entry.get("field_key")
                    && existing.get("validated_email") == entry.get("validated_email")
                    && existing.get("person_key") == entry.get("person_key")
                    && existing.get("value") == entry.get("value")
            });
            if !duplicate {
                evidence.push(entry.clone());
            }
        }
    }
    // The lead-level field is answered as soon as one contact is checked. A
    // deliverable address wins over an undeliverable one.
    let best = verdicts
        .iter()
        .find(|verdict| verdict.valid)
        .or_else(|| verdicts.first())
        .expect("verdicts is not empty");
    let already_valid = lead
        .pointer("/field_status/person_email_validation/value")
        .and_then(Value::as_str)
        == Some("valid");
    if !already_valid {
        if !lead.get("field_status").is_some_and(Value::is_object) {
            lead["field_status"] = json!({});
        }
        lead["field_status"]["person_email_validation"] = json!({
            "status": "verified",
            "value": if best.valid { "valid" } else { "invalid" },
            "sources": [{
                "source_id": VALIDATION_SOURCE_ID,
                "url": best.source_url,
                "quote": best.note,
            }],
            "attempts": [],
            "reason": format!("Vom Daemon ueber {VALIDATION_SOURCE_ID} geprueft: {}", best.email),
        });
    }
    changed
}

fn run_dir_from_envelope(envelope: &Value) -> Option<PathBuf> {
    let manifest = envelope
        .get("run_manifest_path")
        .or_else(|| envelope.pointer("/result/run_manifest_path"))
        .and_then(Value::as_str)?;
    Path::new(manifest).parent().map(Path::to_path_buf)
}

/// Runs the validation target for one address and returns its verdict.
///
/// The run happens in-process against the daemon's own root. The first
/// version shelled out to `ctox scrape execute --runtime-root <root>`; the CLI
/// relays that call to the daemon, the relay refuses `--runtime-root`, and
/// every check failed before it started.
fn run_validation(root: &Path, email: &str) -> anyhow::Result<Option<EmailVerdict>> {
    // The target's run lock names its holder by pid, and every in-process run
    // carries the daemon's pid: two checks at once would refuse each other.
    static RUNS: Mutex<()> = Mutex::new(());
    let _serial = RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut attempt = 0;
    loop {
        match run_validation_once(root, email) {
            Err(error)
                if attempt < 5 && format!("{error:#}").contains("already has an active run") =>
            {
                attempt += 1;
                std::thread::sleep(Duration::from_secs(15));
            }
            other => return other,
        }
    }
}

fn run_validation_once(root: &Path, email: &str) -> anyhow::Result<Option<EmailVerdict>> {
    let args = [
        "execute",
        "--target-key",
        VALIDATION_TARGET_KEY,
        "--trigger-kind",
        "manual",
        "--timeout-seconds",
        "180",
        "--input-json",
        &json!({ "email": email }).to_string(),
    ]
    .map(str::to_string);
    let outcome = crate::capabilities::scrape::execute_scrape_with_outcome(root, &args)?;
    let envelope = serde_json::to_value(&outcome)?;
    let Some(run_dir) = run_dir_from_envelope(&envelope) else {
        anyhow::bail!("validation run for {email} reported no run manifest");
    };
    let run_id = run_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let result: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("outputs/result.json"))?)?;
    let records = result
        .get("records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(verdict_from_records(&records, email, &run_id))
}

fn in_flight() -> std::sync::MutexGuard<'static, HashSet<(PathBuf, String)>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<(PathBuf, String)>>> = OnceLock::new();
    IN_FLIGHT
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn validate_lead_contacts(root: &Path, record_id: &str) -> anyhow::Result<usize> {
    let Some(lead) = store::load_rxdb_collection_record(root, LEAD_COLLECTION, record_id)? else {
        return Ok(0);
    };
    let emails = emails_needing_validation(&lead, MAX_ADDRESSES_PER_PASS);
    if emails.is_empty() {
        return Ok(0);
    }
    let mut verdicts = Vec::new();
    let mut failures = Vec::new();
    for email in &emails {
        match run_validation(root, email) {
            Ok(Some(verdict)) => verdicts.push(verdict),
            Ok(None) => failures.push(json!({"email": email, "error": "kein Urteil im Lauf"})),
            Err(error) => failures.push(json!({"email": email, "error": format!("{error:#}")})),
        }
    }
    // The slow runs happen outside the per-record guard; the patch waits for it
    // so it never interleaves with a research writeback on the same lead.
    let deadline = Instant::now() + Duration::from_secs(900);
    let _guard = loop {
        if let Some(guard) =
            super::person_research_command::ActiveResearchCommandGuard::claim(root, record_id)
        {
            break guard;
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "lead {record_id} stayed busy; {} verdicts not applied",
                verdicts.len()
            );
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let Some(mut lead) = store::load_rxdb_collection_record(root, LEAD_COLLECTION, record_id)?
    else {
        return Ok(0);
    };
    let changed = apply_email_verdicts(&mut lead, &verdicts);
    let now = super::person_research_command::now_ms();
    // The daemon's stderr goes nowhere on a managed tenant, so every pass
    // leaves its account on the lead itself.
    if !lead.get("payload").is_some_and(Value::is_object) {
        lead["payload"] = json!({});
    }
    lead["payload"]["email_validation_pass"] = json!({
        "at_ms": now,
        "checked": verdicts
            .iter()
            .map(|verdict| json!({"email": verdict.email, "valid": verdict.valid, "run_id": verdict.run_id}))
            .collect::<Vec<_>>(),
        "failed": failures,
    });
    lead["research_updated_at_ms"] = Value::from(now);
    store::upsert_rxdb_collection_record(root, LEAD_COLLECTION, record_id, now, lead)?;
    Ok(changed)
}

fn in_flight_key(root: &Path, record_id: &str) -> (PathBuf, String) {
    (
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
        record_id.to_string(),
    )
}

fn validate_registered(root: &Path, record_id: &str, key: &(PathBuf, String)) {
    match validate_lead_contacts(root, record_id) {
        Ok(0) => {}
        Ok(changed) => eprintln!("[email-validation] {record_id}: {changed} contact(s) checked"),
        Err(error) => eprintln!("[email-validation] {record_id}: {error:#}"),
    }
    in_flight().remove(key);
}

/// Checks the contact addresses of one lead in the background. At most one
/// pass per lead runs at a time; a lead without unchecked addresses costs a
/// single record read.
pub(super) fn spawn_contact_email_validation(root: &Path, record_id: &str) {
    if cfg!(test) {
        return;
    }
    let key = in_flight_key(root, record_id);
    if !in_flight().insert(key.clone()) {
        return;
    }
    let root = root.to_path_buf();
    let record_id = record_id.to_string();
    let spawned = std::thread::Builder::new()
        .name("ctox-email-validation".to_string())
        .spawn(move || validate_registered(&root, &record_id, &key));
    if spawned.is_err() {
        eprintln!("[email-validation] could not start the validation thread");
    }
}

const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
const RETRY_AFTER_MS: i64 = 6 * 3600 * 1000;

/// Whether the recovery sweep should check this lead now: it carries an
/// unchecked address, no research is writing to it, and its last pass is not
/// recent.
pub(super) fn lead_due_for_sweep(lead: &Value, now_ms: i64) -> bool {
    let status = lead
        .get("research_status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(status, "queued" | "running") {
        return false;
    }
    if emails_needing_validation(lead, 1).is_empty() {
        return false;
    }
    let last_pass = lead
        .pointer("/payload/email_validation_pass/at_ms")
        .and_then(Value::as_i64);
    last_pass.is_none_or(|at| now_ms - at >= RETRY_AFTER_MS)
}

/// Picks up addresses stored before this check existed and passes that
/// failed. Runs from the recovery loop, at most once an hour, one lead after
/// the other in a single background thread.
pub(super) fn sweep_unchecked_leads(root: &Path) {
    if cfg!(test) {
        return;
    }
    static LAST_SWEEP: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST_SWEEP
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.is_some_and(|at| at.elapsed() < SWEEP_INTERVAL) {
            return;
        }
        *last = Some(Instant::now());
    }
    let Ok(leads) = store::load_rxdb_collection_records(root, LEAD_COLLECTION) else {
        return;
    };
    let now = super::person_research_command::now_ms();
    let due: Vec<String> = leads
        .iter()
        .filter(|lead| lead_due_for_sweep(lead, now))
        .filter_map(|lead| lead.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if due.is_empty() {
        return;
    }
    let root = root.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("ctox-email-validation-sweep".to_string())
        .spawn(move || {
            for record_id in due {
                let key = in_flight_key(&root, &record_id);
                if in_flight().insert(key.clone()) {
                    validate_registered(&root, &record_id, &key);
                }
            }
        });
    if spawned.is_err() {
        eprintln!("[email-validation] could not start the validation sweep");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(email: &str, valid: bool) -> EmailVerdict {
        EmailVerdict {
            email: email.to_string(),
            valid,
            note: format!(
                "EXPERTE.de verdict: {email} | {}",
                if valid { "Gültig" } else { "Ungültig" }
            ),
            source_url: "https://www.experte.de/email-pruefen".to_string(),
            run_id: "scrape_run-test".to_string(),
        }
    }

    #[test]
    fn the_verdict_note_is_read_without_mistaking_ungueltig_for_gueltig() {
        // Measured on the VM 09.09.2026.
        assert_eq!(
            parse_verdict_note("EXPERTE.de verdict: info@weicon.de | Gültig"),
            Some(("info@weicon.de".to_string(), true))
        );
        // "Ungültig" contains "gültig", "invalid" contains "valid".
        assert_eq!(
            parse_verdict_note("EXPERTE.de verdict: R.Weidling@Weicon.de | Ungültig"),
            Some(("r.weidling@weicon.de".to_string(), false))
        );
        assert_eq!(
            parse_verdict_note("EXPERTE.de verdict: x@firma.de | invalid"),
            Some(("x@firma.de".to_string(), false))
        );
        assert_eq!(parse_verdict_note("keine Aussage"), None);
        // The probe address a repair worker used overnight proves nothing.
        assert_eq!(
            parse_verdict_note("EXPERTE.de verdict: max.muster@example.de | Ungültig"),
            None
        );
    }

    #[test]
    fn a_verdict_for_another_address_is_ignored() {
        let records = vec![json!({
            "field": "person_email_validation",
            "value": "valid",
            "source_url": "https://www.experte.de/email-pruefen",
            "note": "EXPERTE.de verdict: info@beiersdorf.com | Gültig"
        })];
        assert_eq!(verdict_from_records(&records, "info@weicon.de", "r1"), None);
        let found = verdict_from_records(&records, "INFO@beiersdorf.com", "r1").expect("verdict");
        assert!(found.valid);
        assert_eq!(found.run_id, "r1");
    }

    #[test]
    fn only_unchecked_real_addresses_are_queued() {
        let lead = json!({"contacts": [
            {"name": "Ralph Weidling", "person_email": "r.weidling@weicon.de"},
            {"name": "Ann-Katrin Weidling", "email": "a.weidling@weicon.de", "person_email_validation": "valid"},
            {"name": "Nicht geprueft", "email": "s.beilmann@weicon.de", "person_email_validation": "no_match"},
            {"name": "Doppelt", "person_email": "R.Weidling@weicon.de"},
            {"name": "Probe", "person_email": "max.muster@example.de"},
            {"name": "Ohne Adresse"}
        ]});
        // `no_match` means the worker could not check; it is not a verdict.
        assert_eq!(
            emails_needing_validation(&lead, 4),
            vec![
                "r.weidling@weicon.de".to_string(),
                "s.beilmann@weicon.de".to_string()
            ]
        );
    }

    #[test]
    fn a_verdict_lands_on_the_contact_that_owns_the_address() {
        let mut lead = json!({
            "contacts": [
                {"person_key": "p-ralph", "name": "Ralph Weidling", "person_email": "r.weidling@weicon.de"},
                {"person_key": "p-sascha", "name": "Sascha Beilmann"}
            ],
            "evidence": [],
            "field_status": {"person_email_validation": {"status": "no_match", "reason": "Sandbox"}}
        });
        let changed = apply_email_verdicts(&mut lead, &[verdict("r.weidling@weicon.de", true)]);
        assert_eq!(changed, 1);
        assert_eq!(lead["contacts"][0]["person_email_validation"], "valid");
        assert!(lead["contacts"][1].get("person_email_validation").is_none());
        let evidence = lead["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["person_key"], "p-ralph");
        assert_eq!(evidence[0]["source_id"], "experte.de");
        assert_eq!(
            lead["field_status"]["person_email_validation"]["status"],
            "verified"
        );
        assert_eq!(
            lead["field_status"]["person_email_validation"]["value"],
            "valid"
        );
        // Applying the same verdict again adds no second evidence entry.
        apply_email_verdicts(&mut lead, &[verdict("r.weidling@weicon.de", true)]);
        assert_eq!(lead["evidence"].as_array().expect("evidence").len(), 1);
    }

    #[test]
    fn an_undeliverable_address_is_answered_but_never_marked_valid() {
        let mut lead = json!({"contacts": [
            {"person_key": "p1", "person_email": "falsch@weicon.de"}
        ]});
        apply_email_verdicts(&mut lead, &[verdict("falsch@weicon.de", false)]);
        assert_eq!(lead["contacts"][0]["person_email_validation"], "invalid");
        assert_eq!(
            lead["field_status"]["person_email_validation"]["value"],
            "invalid"
        );
    }

    #[test]
    fn the_sweep_checks_idle_leads_with_unchecked_addresses_once_per_window() {
        let now = 100 * 3600 * 1000;
        let lead = |status: &str, pass_at: Option<i64>| {
            let mut lead = json!({
                "research_status": status,
                "contacts": [{"person_email": "a.weidling@weicon.de", "person_email_validation": "no_match"}],
                "payload": {},
            });
            if let Some(at) = pass_at {
                lead["payload"]["email_validation_pass"] = json!({"at_ms": at});
            }
            lead
        };
        assert!(lead_due_for_sweep(&lead("needs_review", None), now));
        assert!(!lead_due_for_sweep(&lead("running", None), now));
        assert!(!lead_due_for_sweep(&lead("queued", None), now));
        assert!(!lead_due_for_sweep(
            &lead("completed", Some(now - 3600 * 1000)),
            now
        ));
        assert!(lead_due_for_sweep(
            &lead("completed", Some(now - RETRY_AFTER_MS)),
            now
        ));
        let checked = json!({
            "research_status": "completed",
            "contacts": [{"person_email": "a.weidling@weicon.de", "person_email_validation": "valid"}],
        });
        assert!(!lead_due_for_sweep(&checked, now));
    }

    #[test]
    fn the_run_directory_is_found_in_the_outcome() {
        let envelope = json!({"ok": true, "run_manifest_path": "/srv/ctox/runtime/scraping/targets/experte-de/runs/scrape_run-a9873c8554ce056c/run.json"});
        assert_eq!(
            run_dir_from_envelope(&envelope),
            Some(PathBuf::from(
                "/srv/ctox/runtime/scraping/targets/experte-de/runs/scrape_run-a9873c8554ce056c"
            ))
        );
    }
}
