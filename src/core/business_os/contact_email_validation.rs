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

use std::collections::{BTreeMap, HashMap, HashSet};
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

/// A research writeback may match a person by key while replacing their
/// address. The old scalar verdict and projected statuses must be removed
/// before open-field calculation, or the new address is never checked.
pub(super) fn invalidate_changed_email_addresses(
    lead: &mut Value,
    previous_contacts: &Value,
) -> usize {
    let mut old = HashMap::new();
    let mut duplicate_keys = HashSet::new();
    for contact in previous_contacts.as_array().into_iter().flatten() {
        let Some(key) = contact
            .get("person_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        if old
            .insert(key.to_string(), contact_email(contact))
            .is_some()
        {
            duplicate_keys.insert(key.to_string());
        }
    }
    let Some(contacts) = lead.get_mut("contacts").and_then(Value::as_array_mut) else {
        return 0;
    };
    let mut changed = Vec::new();
    for contact in contacts.iter_mut() {
        let Some(key) = contact
            .get("person_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        if duplicate_keys.contains(&key) {
            continue;
        }
        let Some(Some(old_email)) = old.get(&key) else {
            continue;
        };
        if contact_email(contact).as_deref() == Some(old_email.as_str()) {
            continue;
        }
        if let Some(object) = contact.as_object_mut() {
            object.remove("person_email_validation");
            object.remove("email_validation");
            if let Some(statuses) = object
                .get_mut("field_status")
                .and_then(Value::as_object_mut)
            {
                statuses.remove("person_email_validation");
            }
        }
        changed.push(key);
    }
    if changed.is_empty() {
        return 0;
    }
    for key in &changed {
        if let Some(fields) = lead["person_field_status"]
            .get_mut(key)
            .and_then(Value::as_object_mut)
        {
            fields.remove("person_email_validation");
        }
    }
    if lead["field_status"]
        .get("person_email_validation")
        .is_some()
    {
        lead["field_status"]["person_email_validation"] = json!({
            "status": "action_required",
            "value": null,
            "sources": [],
            "attempts": [],
            "reason": "E-Mail-Adresse geaendert; erneute native Pruefung erforderlich",
        });
    }
    changed.len()
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
    let mut matched = Vec::new();
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
            let person_key = contact
                .get("person_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_string);
            let status = native_email_field_status(verdict, person_key.as_deref());
            if !contact.get("field_status").is_some_and(Value::is_object) {
                contact["field_status"] = json!({});
            }
            contact["field_status"]["person_email_validation"] = status;
            matched.push((person_key.clone(), verdict.clone()));
            changed += 1;
            new_evidence.push(json!({
                "field_key": "person_email_validation",
                "value": label,
                "source_id": VALIDATION_SOURCE_ID,
                "source_url": verdict.source_url,
                "quote": verdict.note,
                "person_key": person_key,
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
    // This is the canonical per-person status. `contacts[].field_status` is
    // its projection and may be rebuilt by a later research writeback.
    for (person_key, verdict) in &matched {
        let Some(person_key) = person_key else {
            continue;
        };
        if !lead
            .get("person_field_status")
            .is_some_and(Value::is_object)
        {
            lead["person_field_status"] = json!({});
        }
        if !lead["person_field_status"]
            .get(person_key)
            .is_some_and(Value::is_object)
        {
            lead["person_field_status"][person_key] = json!({});
        }
        lead["person_field_status"][person_key]["person_email_validation"] =
            native_email_field_status(verdict, Some(person_key));
    }
    for (person_key, verdict) in &matched {
        promote_pattern_email(lead, person_key.as_deref(), verdict);
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
    let best = matched
        .iter()
        .find(|(_, verdict)| verdict.valid)
        .or_else(|| matched.first())
        .map(|(_, verdict)| verdict)
        .expect("changed contacts have matched verdicts");
    let already_valid = lead
        .pointer("/field_status/person_email_validation/value")
        .and_then(Value::as_str)
        == Some("valid");
    if !already_valid {
        if !lead.get("field_status").is_some_and(Value::is_object) {
            lead["field_status"] = json!({});
        }
        lead["field_status"]["person_email_validation"] = native_email_field_status(best, None);
    }
    changed
}

/// A personal address is often not published; the worker builds it from the
/// pattern of other addresses at the same company and leaves it open, because
/// no quote names it (owner 09.10.2026: "aus dem Schema rekonstruieren und
/// dann mit dem E-Mail-Tester testen"). Once experte.de confirms exactly that
/// address as deliverable, its verdict is the quote that names it, and the
/// address counts as verified for that person. An undeliverable verdict, an
/// address the worker did not claim for this person, or an already verified
/// value stays as it is.
fn promote_pattern_email(lead: &mut Value, person_key: Option<&str>, verdict: &EmailVerdict) {
    if !verdict.valid {
        return;
    }
    let promote = |status: &mut Value| {
        let claimed = status
            .get("value")
            .and_then(Value::as_str)
            .and_then(normalize_email);
        if claimed.as_deref() != Some(verdict.email.as_str())
            || status.get("status").and_then(Value::as_str) == Some("verified")
        {
            return;
        }
        let source = json!({
            "source_id": VALIDATION_SOURCE_ID,
            "url": verdict.source_url,
            "quote": verdict.note,
        });
        let mut sources = status
            .get("sources")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        sources.push(source);
        status["status"] = Value::String("verified".to_string());
        status["sources"] = Value::Array(sources);
        status["reason"] = Value::String(format!(
            "Aus dem Adressmuster gebildet; {VALIDATION_SOURCE_ID} bestaetigt die Zustellbarkeit ({})",
            verdict.note
        ));
        status["validated_email"] = Value::String(verdict.email.clone());
        status["run_id"] = Value::String(verdict.run_id.clone());
    };
    if let Some(person_key) = person_key {
        if let Some(status) = lead
            .get_mut("person_field_status")
            .and_then(|people| people.get_mut(person_key))
            .and_then(|fields| fields.get_mut("person_email"))
        {
            promote(status);
        }
    }
    if let Some(contacts) = lead.get_mut("contacts").and_then(Value::as_array_mut) {
        for contact in contacts.iter_mut() {
            let owner = contact
                .get("person_key")
                .and_then(Value::as_str)
                .map(str::trim);
            if owner != person_key || contact_email(contact).as_deref() != Some(&verdict.email) {
                continue;
            }
            if let Some(status) = contact
                .get_mut("field_status")
                .and_then(|fields| fields.get_mut("person_email"))
            {
                promote(status);
            }
        }
    }
}

fn native_email_field_status(verdict: &EmailVerdict, person_key: Option<&str>) -> Value {
    let mut status = json!({
        "status": "verified",
        "value": if verdict.valid { "valid" } else { "invalid" },
        "sources": [{
            "source_id": VALIDATION_SOURCE_ID,
            "url": verdict.source_url,
            "quote": verdict.note,
        }],
        "attempts": [],
        "reason": format!("Vom Daemon ueber {VALIDATION_SOURCE_ID} geprueft: {}", verdict.email),
        "validated_email": verdict.email,
        "run_id": verdict.run_id,
        "via": "native_email_validation",
    });
    if let Some(person_key) = person_key {
        status["person_key"] = Value::String(person_key.to_string());
    }
    status
}

fn legacy_validation_placeholder(lead: &Value) -> bool {
    let status = &lead["field_status"]["person_email_validation"];
    status.get("status").and_then(Value::as_str) == Some("action_required")
        && status
            .get("reason")
            .and_then(Value::as_str)
            .is_some_and(|reason| {
                let reason = reason.to_ascii_lowercase();
                reason.contains("daemon") && reason.contains("experte.de")
            })
        && !email_receipt_entries(lead).is_empty()
}

fn email_receipt_entries(lead: &Value) -> Vec<Value> {
    let history = lead
        .pointer("/payload/email_validation_receipts")
        .and_then(Value::as_object);
    if let Some(history) = history.filter(|history| !history.is_empty()) {
        return history.values().cloned().collect();
    }
    lead.pointer("/payload/email_validation_pass/checked")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The last pass still describes only that pass. This address-keyed receipt
/// map retains older currently attached addresses across batches, so a later
/// writeback can verify every native per-person status against its own run.
fn merged_email_receipts(lead: &Value, verdicts: &[EmailVerdict]) -> Value {
    let current_emails = lead
        .get("contacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(contact_email)
        .collect::<HashSet<_>>();
    let mut receipts = BTreeMap::new();
    for entry in email_receipt_entries(lead).into_iter().chain(
        lead.pointer("/payload/email_validation_pass/checked")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .cloned(),
    ) {
        let Some(email) = entry
            .get("email")
            .and_then(Value::as_str)
            .and_then(normalize_email)
        else {
            continue;
        };
        if current_emails.contains(&email)
            && entry.get("valid").and_then(Value::as_bool).is_some()
            && entry
                .get("run_id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
        {
            receipts.insert(
                email.clone(),
                json!({
                    "email": email,
                    "valid": entry["valid"],
                    "run_id": entry["run_id"],
                }),
            );
        }
    }
    for verdict in verdicts {
        if current_emails.contains(&verdict.email) {
            receipts.insert(
                verdict.email.clone(),
                json!({
                    "email": verdict.email,
                    "valid": verdict.valid,
                    "run_id": verdict.run_id,
                }),
            );
        }
    }
    json!(receipts)
}

/// Reconstructs only the field status of contacts whose saved native receipt
/// agrees with their current stable person key, address, and scalar verdict.
/// No scrape is repeated, no external quote is invented, and the lead's
/// `research_status` remains unchanged. Called only for an already-checked
/// lead that enters the existing per-record validation path again.
fn reconcile_receipted_email_status(lead: &mut Value) -> usize {
    if !legacy_validation_placeholder(lead) {
        return 0;
    }
    let checked = email_receipt_entries(lead);
    let Some(contacts) = lead.get("contacts").and_then(Value::as_array) else {
        return 0;
    };
    let mut seen_keys = HashSet::new();
    let mut seen_emails = HashSet::new();
    let mut address_count = 0;
    let mut matches = Vec::new();
    for (index, contact) in contacts.iter().enumerate() {
        let Some(email) = contact_email(contact) else {
            continue;
        };
        address_count += 1;
        let Some(person_key) = contact
            .get("person_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        if !seen_keys.insert(person_key.to_string()) || !seen_emails.insert(email.clone()) {
            return 0;
        }
        let valid = match contact
            .get("person_email_validation")
            .and_then(Value::as_str)
        {
            Some("valid") => true,
            Some("invalid") => false,
            _ => continue,
        };
        let receipts = checked
            .iter()
            .filter(|entry| {
                entry
                    .get("email")
                    .and_then(Value::as_str)
                    .and_then(normalize_email)
                    .as_deref()
                    == Some(email.as_str())
            })
            .collect::<Vec<_>>();
        if receipts.len() != 1 || receipts[0].get("valid").and_then(Value::as_bool) != Some(valid) {
            continue;
        }
        let Some(run_id) = receipts[0]
            .get("run_id")
            .and_then(Value::as_str)
            .filter(|id| {
                id.starts_with("scrape_run-")
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            })
        else {
            continue;
        };
        let old_canonical = &lead["person_field_status"][person_key]["person_email_validation"];
        let old_projected = &contact["field_status"]["person_email_validation"];
        if [old_canonical, old_projected].into_iter().any(|status| {
            status
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "action_required")
        }) {
            continue;
        }
        matches.push((
            index,
            person_key.to_string(),
            email,
            valid,
            run_id.to_string(),
        ));
    }
    if matches.is_empty() {
        return 0;
    }
    for (index, person_key, email, valid, run_id) in &matches {
        let status = json!({
            "status": "verified",
            "value": if *valid { "valid" } else { "invalid" },
            "sources": [{
                "source_id": VALIDATION_SOURCE_ID,
                "url": "https://www.experte.de/email-pruefen",
                "run_id": run_id,
            }],
            "attempts": [],
            "reason": format!("Aus gespeichertem nativen experte.de-Prueflauf {run_id} rekonstruiert; Originalzitat nicht erneut gelesen"),
            "validated_email": email,
            "run_id": run_id,
            "via": "native_email_validation_receipt",
            "person_key": person_key,
        });
        if !lead["contacts"][*index]
            .get("field_status")
            .is_some_and(Value::is_object)
        {
            lead["contacts"][*index]["field_status"] = json!({});
        }
        lead["contacts"][*index]["field_status"]["person_email_validation"] = status.clone();
        if !lead
            .get("person_field_status")
            .is_some_and(Value::is_object)
        {
            lead["person_field_status"] = json!({});
        }
        if !lead["person_field_status"]
            .get(person_key)
            .is_some_and(Value::is_object)
        {
            lead["person_field_status"][person_key] = json!({});
        }
        lead["person_field_status"][person_key]["person_email_validation"] = status;
    }
    // Count every current address, including an already-verified contact that
    // did not need reconstruction in this pass. A scalar verdict by itself is
    // insufficient: both statuses must still agree with its native receipt.
    let bound = lead["contacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|contact| {
            let email = contact_email(contact)?;
            let person_key = contact.get("person_key")?.as_str()?;
            let valid = match contact.get("person_email_validation")?.as_str()? {
                "valid" => true,
                "invalid" => false,
                _ => return None,
            };
            let receipts = checked
                .iter()
                .filter(|entry| entry.get("email").and_then(Value::as_str) == Some(email.as_str()))
                .collect::<Vec<_>>();
            if receipts.len() != 1
                || receipts[0].get("valid").and_then(Value::as_bool) != Some(valid)
            {
                return None;
            }
            let run_id = receipts[0].get("run_id").and_then(Value::as_str)?;
            let canonical = lead
                .get("person_field_status")?
                .get(person_key)?
                .get("person_email_validation")?;
            let projected = contact
                .get("field_status")?
                .get("person_email_validation")?;
            if [canonical, projected].into_iter().any(|status| {
                status.get("status").and_then(Value::as_str) != Some("verified")
                    || !matches!(
                        status.get("via").and_then(Value::as_str),
                        Some("native_email_validation" | "native_email_validation_receipt")
                    )
                    || status.get("person_key").and_then(Value::as_str) != Some(person_key)
                    || status.get("run_id").and_then(Value::as_str) != Some(run_id)
                    || status.get("value").and_then(Value::as_str)
                        != Some(if valid { "valid" } else { "invalid" })
                    || status
                        .get("validated_email")
                        .and_then(Value::as_str)
                        .and_then(normalize_email)
                        .as_deref()
                        != Some(email.as_str())
            }) {
                return None;
            }
            Some((person_key.to_string(), valid))
        })
        .collect::<Vec<_>>();
    if bound.len() == address_count {
        let best = bound.iter().find(|(_, valid)| *valid).unwrap_or(&bound[0]);
        lead["field_status"]["person_email_validation"] =
            lead["person_field_status"][&best.0]["person_email_validation"].clone();
    }
    matches.len()
}

/// A later research writeback may carry the worker's old `action_required`
/// placeholder for step 4a. Keep a native verdict only for the same person
/// and the same address; a changed address must be checked again.
pub(super) fn restore_native_email_verdicts(lead: &mut Value, previous: &Value) -> usize {
    let Some(previous) = previous.as_object() else {
        return 0;
    };
    let checked = email_receipt_entries(lead);
    let mut restored = Vec::new();
    let Some(contacts) = lead.get_mut("contacts").and_then(Value::as_array_mut) else {
        return 0;
    };
    for contact in contacts.iter_mut() {
        let Some(person_key) = contact
            .get("person_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let Some(email) = contact_email(contact) else {
            continue;
        };
        let Some(status) = previous
            .get(&person_key)
            .and_then(|fields| fields.get("person_email_validation"))
        else {
            continue;
        };
        if !matches!(
            status.get("via").and_then(Value::as_str),
            Some("native_email_validation" | "native_email_validation_receipt")
        ) || status.get("status").and_then(Value::as_str) != Some("verified")
            || !matches!(
                status.get("value").and_then(Value::as_str),
                Some("valid" | "invalid")
            )
            || status
                .get("run_id")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            || status.get("person_key").and_then(Value::as_str) != Some(person_key.as_str())
            || status
                .get("validated_email")
                .and_then(Value::as_str)
                .and_then(normalize_email)
                .as_deref()
                != Some(email.as_str())
        {
            continue;
        }
        // Worker-supplied statuses can contain arbitrary extra JSON keys. The
        // daemon's own last-pass receipt must agree on address, run and result.
        let run_id = status
            .get("run_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let valid = status.get("value").and_then(Value::as_str) == Some("valid");
        if !checked.iter().any(|entry| {
            entry.get("email").and_then(Value::as_str) == Some(email.as_str())
                && entry.get("run_id").and_then(Value::as_str) == Some(run_id)
                && entry.get("valid").and_then(Value::as_bool) == Some(valid)
        }) {
            continue;
        }
        if !contact.get("field_status").is_some_and(Value::is_object) {
            contact["field_status"] = json!({});
        }
        contact["field_status"]["person_email_validation"] = status.clone();
        contact["person_email_validation"] = status["value"].clone();
        restored.push((person_key, status.clone()));
    }
    if restored.is_empty() {
        return 0;
    }
    let unbound_valid_address = contacts.iter().any(|contact| {
        contact
            .get("person_email_validation")
            .and_then(Value::as_str)
            == Some("valid")
            && !restored.iter().any(|(key, status)| {
                contact.get("person_key").and_then(Value::as_str) == Some(key.as_str())
                    && contact_email(contact).as_deref()
                        == status.get("validated_email").and_then(Value::as_str)
            })
    });
    if !lead
        .get("person_field_status")
        .is_some_and(Value::is_object)
    {
        lead["person_field_status"] = json!({});
    }
    for (person_key, status) in &restored {
        if !lead["person_field_status"]
            .get(person_key)
            .is_some_and(Value::is_object)
        {
            lead["person_field_status"][person_key] = json!({});
        }
        lead["person_field_status"][person_key]["person_email_validation"] = status.clone();
    }
    let best = restored
        .iter()
        .find(|(_, status)| status.get("value").and_then(Value::as_str) == Some("valid"))
        .or_else(|| restored.first())
        .map(|(_, status)| status)
        .expect("restored statuses are not empty");
    if !lead.get("field_status").is_some_and(Value::is_object) {
        lead["field_status"] = json!({});
    }
    if best.get("value").and_then(Value::as_str) == Some("invalid") && unbound_valid_address {
        lead["field_status"]["person_email_validation"] = json!({
            "status": "action_required",
            "value": null,
            "sources": [],
            "attempts": [],
            "reason": "Ein weiteres gueltiges Kontakturteil hat keinen gebundenen nativen Prueflauf",
        });
    } else {
        lead["field_status"]["person_email_validation"] = best.clone();
    }
    restored.len()
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
    if emails.is_empty() && !legacy_validation_placeholder(&lead) {
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
    let expected_master = lead.clone();
    let repaired = if emails.is_empty() {
        reconcile_receipted_email_status(&mut lead)
    } else {
        0
    };
    let changed = repaired + apply_email_verdicts(&mut lead, &verdicts);
    if emails.is_empty() && changed == 0 {
        return Ok(0);
    }
    if changed > 0 {
        super::person_research_gap_closure::complete_after_native_email_validation_with_native_reviews(root, record_id, &mut lead)?;
    }
    let now = super::person_research_command::now_ms();
    // The daemon's stderr goes nowhere on a managed tenant, so every pass
    // leaves its account on the lead itself.
    if !lead.get("payload").is_some_and(Value::is_object) {
        lead["payload"] = json!({});
    }
    let receipts = merged_email_receipts(&lead, &verdicts);
    lead["payload"]["email_validation_receipts"] = receipts;
    if !emails.is_empty() {
        lead["payload"]["email_validation_pass"] = json!({
            "at_ms": now,
            "checked": verdicts
                .iter()
                .map(|verdict| json!({"email": verdict.email, "valid": verdict.valid, "run_id": verdict.run_id}))
                .collect::<Vec<_>>(),
            "failed": failures,
        });
    }
    lead["research_updated_at_ms"] = Value::from(now);
    store::upsert_native_email_validation_record(root, record_id, now, lead, &expected_master)?;
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
        assert_eq!(
            lead["person_field_status"]["p-ralph"]["person_email_validation"]["status"],
            "verified"
        );
        assert_eq!(
            lead["contacts"][0]["field_status"]["person_email_validation"],
            lead["person_field_status"]["p-ralph"]["person_email_validation"]
        );
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
    fn a_deliverable_pattern_address_becomes_the_persons_verified_email() {
        let open = json!({"status": "action_required", "value": "r.weidling@weicon.de",
            "sources": [{"source_id": "weicon.de", "url": "https://www.weicon.de/impressum",
                "quote": "info@weicon.de, s.beilmann@weicon.de"}],
            "reason": "aus dem Muster v.nachname@weicon.de gebildet"});
        let mut lead = json!({
            "contacts": [
                {"person_key": "p-ralph", "person_email": "r.weidling@weicon.de",
                 "field_status": {"person_email": open.clone()}},
                {"person_key": "p-anna", "person_email": "a.muster@weicon.de",
                 "field_status": {"person_email": open.clone()}}
            ],
            "person_field_status": {"p-ralph": {"person_email": open.clone()},
                                    "p-anna": {"person_email": open.clone()}},
            "evidence": []
        });
        apply_email_verdicts(
            &mut lead,
            &[
                verdict("r.weidling@weicon.de", true),
                verdict("a.muster@weicon.de", false),
            ],
        );
        let ralph = &lead["person_field_status"]["p-ralph"]["person_email"];
        assert_eq!(ralph["status"], "verified");
        assert_eq!(ralph["value"], "r.weidling@weicon.de");
        let sources = ralph["sources"].as_array().expect("sources");
        assert_eq!(
            sources.len(),
            2,
            "the pattern source stays, the verdict joins it"
        );
        assert_eq!(sources[1]["source_id"], "experte.de");
        assert_eq!(lead["contacts"][0]["field_status"]["person_email"], *ralph);
        // Undeliverable: the address stays open.
        assert_eq!(
            lead["person_field_status"]["p-anna"]["person_email"]["status"],
            "action_required"
        );
        assert_eq!(
            lead["contacts"][1]["field_status"]["person_email"]["status"],
            "action_required"
        );
        // A verdict for another address never verifies this person's claim.
        let mut other = json!({
            "contacts": [{"person_key": "p1", "person_email": "x@weicon.de"}],
            "person_field_status": {"p1": {"person_email": {"status": "action_required", "value": "y@weicon.de"}}}
        });
        apply_email_verdicts(&mut other, &[verdict("x@weicon.de", true)]);
        assert_eq!(
            other["person_field_status"]["p1"]["person_email"]["status"],
            "action_required"
        );
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
    fn later_worker_placeholder_cannot_undo_a_native_verdict_for_the_same_person_and_email() {
        let mut checked = json!({
            "contacts": [{"person_key": "p1", "person_email": "a@weicon.de"}],
        });
        apply_email_verdicts(&mut checked, &[verdict("a@weicon.de", true)]);
        checked["payload"]["email_validation_pass"]["checked"] = json!([{
            "email": "a@weicon.de", "valid": true, "run_id": "scrape_run-test"
        }]);
        let previous = checked["person_field_status"].clone();
        checked["field_status"]["person_email_validation"] =
            json!({"status": "action_required", "reason": "Daemon prueft spaeter"});
        checked["person_field_status"]["p1"]["person_email_validation"] =
            json!({"status": "action_required", "person_key": "p1"});
        checked["contacts"][0]["field_status"]["person_email_validation"] =
            json!({"status": "action_required"});

        assert_eq!(restore_native_email_verdicts(&mut checked, &previous), 1);
        assert_eq!(
            checked["field_status"]["person_email_validation"]["status"],
            "verified"
        );
        assert_eq!(
            checked["person_field_status"]["p1"]["person_email_validation"]["value"],
            "valid"
        );
        assert_eq!(
            checked["contacts"][0]["field_status"]["person_email_validation"]["value"],
            "valid"
        );

        let mut different_email = checked.clone();
        different_email["contacts"][0]["person_email"] = json!("new@weicon.de");
        different_email["field_status"]["person_email_validation"] =
            json!({"status": "action_required"});
        assert_eq!(
            restore_native_email_verdicts(&mut different_email, &previous),
            0
        );
        assert_eq!(
            different_email["field_status"]["person_email_validation"]["status"],
            "action_required"
        );

        let mut different_person = checked.clone();
        different_person["contacts"][0]["person_key"] = json!("p2");
        different_person["field_status"]["person_email_validation"] =
            json!({"status": "action_required"});
        assert_eq!(
            restore_native_email_verdicts(&mut different_person, &previous),
            0
        );

        let mut forged_receipt = checked.clone();
        forged_receipt["payload"]["email_validation_pass"]["checked"] = json!([]);
        forged_receipt["field_status"]["person_email_validation"] =
            json!({"status": "action_required"});
        assert_eq!(
            restore_native_email_verdicts(&mut forged_receipt, &previous),
            0
        );
    }

    #[test]
    fn native_verdict_completes_only_a_receipted_final_email_gap() {
        let mut lead = json!({
            "contacts": [{"person_key": "p1", "person_email": "a@weicon.de"}],
            "research_status": "needs_review",
            "payload": {
                "native_research_terminal_status": "needs_review",
                "native_research_requested_fields": ["firma_name", "person_email_validation"],
                "native_research_rejections_count": 0
            },
            "field_status": {
                "firma_name": {"status": "verified", "value": "Weicon"},
                "person_email_validation": {"status": "action_required"}
            }
        });
        apply_email_verdicts(&mut lead, &[verdict("a@weicon.de", true)]);
        assert!(
            super::super::person_research_gap_closure::complete_after_native_email_validation(
                &mut lead
            )
        );
        assert_eq!(lead["research_status"], "completed");

        let mut another_address = lead.clone();
        another_address["research_status"] = json!("needs_review");
        another_address["payload"]["native_research_terminal_status"] = json!("needs_review");
        another_address["contacts"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "person_email": "b@weicon.de"
            }));
        assert!(
            !super::super::person_research_gap_closure::complete_after_native_email_validation(
                &mut another_address
            )
        );

        let mut another_open_field = lead.clone();
        another_open_field["research_status"] = json!("needs_review");
        another_open_field["payload"]["native_research_terminal_status"] = json!("needs_review");
        another_open_field["field_status"]["firma_name"] = json!({"status": "action_required"});
        assert!(
            !super::super::person_research_gap_closure::complete_after_native_email_validation(
                &mut another_open_field
            )
        );

        let mut older_lead = lead.clone();
        older_lead["research_status"] = json!("needs_review");
        older_lead["payload"]["native_research_terminal_status"] = json!("needs_review");
        older_lead["payload"]
            .as_object_mut()
            .unwrap()
            .remove("native_research_rejections_count");
        assert!(
            !super::super::person_research_gap_closure::complete_after_native_email_validation(
                &mut older_lead
            )
        );
    }

    #[test]
    fn old_two_person_receipt_repairs_only_the_email_field() {
        // Shape of the saved lead_x6q05l pair, with synthetic addresses.
        let legacy = json!({
            "contacts": [
                {"person_key": "sellify-person-41504", "person_email": "a@biogen.com", "person_email_validation": "valid"},
                {"person_key": "sellify-person-41503", "person_email": "b@biogen.com", "person_email_validation": "valid"}
            ],
            "person_field_status": null,
            "field_status": {"person_email_validation": {
                "status": "action_required", "person_key": "sellify-person-41503",
                "reason": "CTOX-Daemon validiert automatisch via experte.de"
            }},
            "payload": {"email_validation_pass": {"checked": [
                {"email": "a@biogen.com", "valid": true, "run_id": "scrape_run-aaa"},
                {"email": "b@biogen.com", "valid": true, "run_id": "scrape_run-bbb"}
            ]}},
            "research_status": "needs_review"
        });
        let mut repaired = legacy.clone();
        assert_eq!(reconcile_receipted_email_status(&mut repaired), 2);
        assert_eq!(
            repaired["person_field_status"]["sellify-person-41504"]["person_email_validation"]
                ["run_id"],
            "scrape_run-aaa"
        );
        assert_eq!(
            repaired["person_field_status"]["sellify-person-41503"]["person_email_validation"]
                ["run_id"],
            "scrape_run-bbb"
        );
        assert_eq!(
            repaired["field_status"]["person_email_validation"]["status"],
            "verified"
        );
        assert_eq!(repaired["research_status"], "needs_review");
        assert_eq!(repaired["payload"], legacy["payload"]);
        assert_eq!(reconcile_receipted_email_status(&mut repaired), 0);

        let mut one_already_bound = legacy.clone();
        let first_status = repaired["person_field_status"]["sellify-person-41504"]
            ["person_email_validation"]
            .clone();
        one_already_bound["person_field_status"] = json!({
            "sellify-person-41504": {"person_email_validation": first_status}
        });
        one_already_bound["contacts"][0]["field_status"]["person_email_validation"] =
            first_status.clone();
        assert_eq!(reconcile_receipted_email_status(&mut one_already_bound), 1);
        assert_eq!(
            one_already_bound["field_status"]["person_email_validation"]["status"],
            "verified"
        );
        assert_eq!(one_already_bound["research_status"], "needs_review");

        let mut unbound_existing = legacy.clone();
        let mut forged_status = first_status;
        forged_status["run_id"] = json!("scrape_run-other");
        unbound_existing["person_field_status"] = json!({
            "sellify-person-41504": {"person_email_validation": forged_status}
        });
        unbound_existing["contacts"][0]["field_status"]["person_email_validation"] = forged_status;
        assert_eq!(reconcile_receipted_email_status(&mut unbound_existing), 1);
        assert_eq!(
            unbound_existing["field_status"]["person_email_validation"]["status"],
            "action_required"
        );

        let mut changed_email = legacy.clone();
        changed_email["contacts"][1]["person_email"] = json!("new@biogen.com");
        assert_eq!(reconcile_receipted_email_status(&mut changed_email), 1);
        assert_eq!(
            changed_email["field_status"]["person_email_validation"]["status"],
            "action_required"
        );
        assert!(changed_email["person_field_status"]["sellify-person-41503"].is_null());

        let mut duplicate_key = legacy.clone();
        duplicate_key["contacts"][1]["person_key"] = json!("sellify-person-41504");
        assert_eq!(reconcile_receipted_email_status(&mut duplicate_key), 0);

        let mut mismatched_verdict = legacy.clone();
        mismatched_verdict["payload"]["email_validation_pass"]["checked"][1]["valid"] =
            json!(false);
        assert_eq!(reconcile_receipted_email_status(&mut mismatched_verdict), 1);
        assert_eq!(
            mismatched_verdict["field_status"]["person_email_validation"]["status"],
            "action_required"
        );

        let mut unrelated_review = legacy.clone();
        unrelated_review["field_status"]["person_email_validation"]["reason"] =
            json!("Widerspruch in externen Quellen");
        assert_eq!(reconcile_receipted_email_status(&mut unrelated_review), 0);
    }

    #[test]
    fn changed_person_address_invalidates_old_verdict_before_completion() {
        let mut lead = json!({
            "contacts": [{"person_key": "p1", "person_email": "old@weicon.de"}],
            "research_status": "needs_review",
            "payload": {
                "native_research_terminal_status": "needs_review",
                "native_research_requested_fields": ["person_email_validation"],
                "native_research_rejections_count": 0
            }
        });
        apply_email_verdicts(&mut lead, &[verdict("old@weicon.de", true)]);
        let previous_contacts = lead["contacts"].clone();
        // The research merge matched p1 and retained the old scalar verdict.
        lead["contacts"][0]["person_email"] = json!("new@weicon.de");
        assert_eq!(
            invalidate_changed_email_addresses(&mut lead, &previous_contacts),
            1
        );
        assert!(lead["contacts"][0].get("person_email_validation").is_none());
        assert!(lead["person_field_status"]["p1"]
            .get("person_email_validation")
            .is_none());
        assert_eq!(
            lead["field_status"]["person_email_validation"]["status"],
            "action_required"
        );
        assert_eq!(emails_needing_validation(&lead, 4), vec!["new@weicon.de"]);
        assert!(
            !super::super::person_research_gap_closure::complete_after_native_email_validation(
                &mut lead
            )
        );
    }

    #[test]
    fn five_address_batches_keep_the_first_valid_receipt_and_lead_verdict() {
        let mut lead = json!({"contacts": (0..5).map(|i| json!({
            "person_key": format!("p{i}"),
            "person_email": format!("p{i}@weicon.de")
        })).collect::<Vec<_>>()});
        let first = (0..4)
            .map(|i| verdict(&format!("p{i}@weicon.de"), i == 0))
            .collect::<Vec<_>>();
        apply_email_verdicts(&mut lead, &first);
        lead["payload"]["email_validation_receipts"] = merged_email_receipts(&lead, &first);
        lead["payload"]["email_validation_pass"]["checked"] = json!(first
            .iter()
            .map(|v| json!({
                "email": v.email, "valid": v.valid, "run_id": v.run_id
            }))
            .collect::<Vec<_>>());

        let last = verdict("p4@weicon.de", false);
        apply_email_verdicts(&mut lead, &[last.clone()]);
        let receipts = merged_email_receipts(&lead, &[last.clone()]);
        lead["payload"]["email_validation_receipts"] = receipts;
        lead["payload"]["email_validation_pass"]["checked"] = json!([{
            "email": last.email, "valid": last.valid, "run_id": last.run_id
        }]);
        let previous = lead["person_field_status"].clone();
        for i in 0..5 {
            let key = format!("p{i}");
            lead["contacts"][i]["field_status"]["person_email_validation"] =
                json!({"status": "action_required"});
            lead["person_field_status"][key.as_str()]["person_email_validation"] =
                json!({"status": "action_required"});
        }
        lead["field_status"]["person_email_validation"] = json!({"status": "action_required"});
        assert_eq!(restore_native_email_verdicts(&mut lead, &previous), 5);
        assert_eq!(
            lead["field_status"]["person_email_validation"]["value"],
            "valid"
        );
        assert_eq!(
            lead["person_field_status"]["p0"]["person_email_validation"]["value"],
            "valid"
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
