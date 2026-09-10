//! Native e-mail validation for outbound leads.
//!
//! The release gate of the outbound app demands a contact whose e-mail address
//! has been checked. Measured on the THESEN tenant 09./10.09.2026: of 25 leads
//! eleven carried a contact address and not one carried a checked one, so no
//! lead could ever be handed to the CRM.
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
use std::process::Command;
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

fn contact_has_verdict(contact: &Value) -> bool {
    ["person_email_validation", "email_validation"]
        .iter()
        .filter_map(|key| contact.get(*key).and_then(Value::as_str))
        .any(|value| !value.trim().is_empty())
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

fn run_dir_from_envelope(stdout: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(stdout);
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let envelope: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let manifest = envelope
        .get("run_manifest_path")
        .or_else(|| envelope.pointer("/result/run_manifest_path"))
        .and_then(Value::as_str)?;
    Path::new(manifest).parent().map(Path::to_path_buf)
}

/// Runs the validation target for one address and returns its verdict.
fn run_validation(
    root: &Path,
    ctox_bin: &Path,
    email: &str,
) -> anyhow::Result<Option<EmailVerdict>> {
    let output = Command::new(ctox_bin)
        .arg("scrape")
        .arg("execute")
        .arg("--target-key")
        .arg(VALIDATION_TARGET_KEY)
        .arg("--trigger-kind")
        .arg("manual")
        .arg("--timeout-seconds")
        .arg("180")
        .arg("--input-json")
        .arg(json!({ "email": email }).to_string())
        .arg("--runtime-root")
        .arg(root)
        .output()?;
    let Some(run_dir) = run_dir_from_envelope(&output.stdout) else {
        anyhow::bail!(
            "validation run for {email} printed no run manifest (exit {:?})",
            output.status.code()
        );
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
    let ctox_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ctox"));
    let mut verdicts = Vec::new();
    for email in &emails {
        match run_validation(root, &ctox_bin, email) {
            Ok(Some(verdict)) => verdicts.push(verdict),
            Ok(None) => eprintln!("[email-validation] {record_id}: no verdict for {email}"),
            Err(error) => eprintln!("[email-validation] {record_id}: {email}: {error:#}"),
        }
    }
    if verdicts.is_empty() {
        return Ok(0);
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
    if changed > 0 {
        let now = super::person_research_command::now_ms();
        lead["research_updated_at_ms"] = Value::from(now);
        store::upsert_rxdb_collection_record(root, LEAD_COLLECTION, record_id, now, lead)?;
    }
    Ok(changed)
}

/// Checks the contact addresses of one lead in the background. At most one
/// pass per lead runs at a time; a lead without unchecked addresses costs a
/// single record read.
pub(super) fn spawn_contact_email_validation(root: &Path, record_id: &str) {
    if cfg!(test) {
        return;
    }
    let key = (
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
        record_id.to_string(),
    );
    if !in_flight().insert(key.clone()) {
        return;
    }
    let root = root.to_path_buf();
    let record_id = record_id.to_string();
    let spawned = std::thread::Builder::new()
        .name("ctox-email-validation".to_string())
        .spawn(move || {
            match validate_lead_contacts(&root, &record_id) {
                Ok(0) => {}
                Ok(changed) => {
                    eprintln!("[email-validation] {record_id}: {changed} contact(s) checked")
                }
                Err(error) => eprintln!("[email-validation] {record_id}: {error:#}"),
            }
            in_flight().remove(&key);
        });
    if spawned.is_err() {
        eprintln!("[email-validation] could not start the validation thread");
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
            {"name": "Doppelt", "person_email": "R.Weidling@weicon.de"},
            {"name": "Probe", "person_email": "max.muster@example.de"},
            {"name": "Ohne Adresse"}
        ]});
        assert_eq!(
            emails_needing_validation(&lead, 4),
            vec!["r.weidling@weicon.de".to_string()]
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
    fn the_run_directory_is_found_in_the_cli_envelope() {
        let stdout = br#"log line
{"ok": true, "run_manifest_path": "/srv/ctox/runtime/scraping/targets/experte-de/runs/scrape_run-a9873c8554ce056c/run.json"}"#;
        assert_eq!(
            run_dir_from_envelope(stdout),
            Some(PathBuf::from(
                "/srv/ctox/runtime/scraping/targets/experte-de/runs/scrape_run-a9873c8554ce056c"
            ))
        );
    }
}
