// Origin: CTOX
// License: AGPL-3.0-only

//! Compact, complete read-back of a stored outbound lead.
//!
//! A researched lead grows with every writeback: quotes, attempts, revision
//! stamps, the evidence list and the per-contact copies of every person field.
//! On a customer tenant (14.10.2026) one lead reached ~254 KB after several
//! runs. The research worker could no longer read its own record back (the
//! MCP response cap is 256 KiB and the harness keeps only ~10-12 KB of any
//! tool output), wrote its closing report from memory, and the completion
//! review, which judged against a different table (lead-level `field_status`
//! only, no contacts), rejected the report five times as contradicting the
//! stored lead.
//!
//! This view is the single shape both sides read: the worker receives it from
//! `business_os.get_record` (bounded mode) and with every
//! `business_os.execute_writeback` receipt, and the review gate renders the
//! same view as the authoritative stored lead result. It drops quotes,
//! attempts, revision stamps, HTML and history, keeps every company field and
//! every contact with value, status and source ids, and states the counts the
//! server itself uses (`verified` and `no_match` answer a field, everything
//! else is open). It is data only; it carries no research instructions.

use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

use super::store;

pub(crate) const COMPACT_LEAD_READBACK_VIEW: &str = "compact_lead_readback_v1";
const LEAD_COLLECTION: &str = "outbound_lead_generation_leads";

/// Target size of the serialized view. Below the harness tool-output window
/// (10 000 bytes plus its 20 % serialization allowance) so a model reads the
/// whole view, not a head/tail cut. Exceeding it degrades detail (reasons,
/// then long values, then contact source ids), never drops a field or a
/// contact.
pub(crate) const COMPACT_LEAD_READBACK_TARGET_BYTES: usize = 10_000;

/// The full record is returned inline by `get_record` only up to this size;
/// a larger outbound lead is answered with the compact view.
pub(crate) const FULL_LEAD_RECORD_INLINE_MAX_BYTES: usize = 24 * 1024;

const HEADLINE_FIELDS: &[(&str, &[&str], &str)] = &[
    // (output key, contact members in priority order, person status field)
    ("function", &["person_funktion", "role"], "person_funktion"),
    (
        "position",
        &["person_position", "position"],
        "person_position",
    ),
    ("email", &["email", "person_email"], "person_email"),
    ("phone", &["phone", "person_telefon"], "person_telefon"),
    (
        "linkedin",
        &["person_linkedin", "linkedin"],
        "person_linkedin",
    ),
    ("xing", &["person_xing", "xing"], "person_xing"),
];

#[derive(Clone, Copy)]
struct Detail {
    value_chars: usize,
    reason_chars: usize,
    contact_sources: bool,
}

const DETAIL_LEVELS: &[Detail] = &[
    Detail {
        value_chars: 240,
        reason_chars: 90,
        contact_sources: true,
    },
    Detail {
        value_chars: 160,
        reason_chars: 0,
        contact_sources: true,
    },
    Detail {
        value_chars: 80,
        reason_chars: 0,
        contact_sources: true,
    },
    Detail {
        value_chars: 80,
        reason_chars: 0,
        contact_sources: false,
    },
];

/// Loads the lead with the native review witnesses applied (the same view the
/// research writeback uses to compute its open fields) and compacts it.
pub(crate) fn load_compact_lead_readback(
    root: &Path,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    let Some(lead) = store::load_rxdb_collection_record(root, LEAD_COLLECTION, record_id)? else {
        return Ok(None);
    };
    Ok(Some(compact_lead_readback_for_store(
        root, record_id, &lead,
    )?))
}

pub(crate) fn compact_lead_readback_for_store(
    root: &Path,
    record_id: &str,
    lead: &Value,
) -> anyhow::Result<Value> {
    let reviewed = super::outbound_field_review::native_review_view(root, record_id, lead)?;
    Ok(compact_lead_readback(&reviewed))
}

pub(crate) fn compact_lead_readback(lead: &Value) -> Value {
    let mut last = Value::Null;
    for (level, detail) in DETAIL_LEVELS.iter().enumerate() {
        let view = render(lead, *detail, level);
        let size = serde_json::to_vec(&view)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        if size <= COMPACT_LEAD_READBACK_TARGET_BYTES {
            return view;
        }
        last = view;
    }
    last
}

fn status_name(status: &Value) -> String {
    let raw = status
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("missing");
    if raw == "no_match" && super::outbound_field_review::is_refuted_no_match(status) {
        "no_match_refuted".to_string()
    } else {
        raw.to_string()
    }
}

#[derive(Default, Clone, Copy)]
struct Counts {
    verified: usize,
    not_found: usize,
    open: usize,
}

impl Counts {
    fn add(&mut self, status: &str) {
        match status {
            "verified" => self.verified += 1,
            "no_match" => self.not_found += 1,
            _ => self.open += 1,
        }
    }
    fn plus(self, other: Counts) -> Counts {
        Counts {
            verified: self.verified + other.verified,
            not_found: self.not_found + other.not_found,
            open: self.open + other.open,
        }
    }
    fn json(self) -> Value {
        json!({"verified": self.verified, "not_found": self.not_found, "open": self.open})
    }
}

fn append_csv(map: &mut Map<String, Value>, key: &str, item: &str) {
    match map.get_mut(key) {
        Some(Value::String(text)) => {
            text.push(',');
            text.push_str(item);
        }
        _ => {
            map.insert(key.to_string(), Value::String(item.to_string()));
        }
    }
}

fn answered(status: &str) -> bool {
    matches!(status, "verified" | "no_match")
}

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!(
            "{}…",
            flat.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

fn plain_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| plain_text(Some(item)))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("; "),
        Some(other) => other.to_string(),
    }
}

/// Source ids of one status entry, in stored order, without duplicates. A
/// source without an id is named by its URL.
fn source_ids(status: &Value) -> String {
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for source in status
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = ["source_id", "provider", "url"]
            .iter()
            .map(|key| plain_text(source.get(*key)))
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        if !id.is_empty() && seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    ids.join(",")
}

fn requested_fields(lead: &Value) -> Vec<String> {
    let requested = lead
        .pointer("/payload/native_research_requested_fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut fields = requested;
    let mut seen = fields.iter().cloned().collect::<BTreeSet<_>>();
    for key in lead
        .get("field_status")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|object| object.keys())
    {
        if seen.insert(key.clone()) {
            fields.push(key.clone());
        }
    }
    fields
}

fn contact_key(contact: &Value) -> Option<String> {
    ["person_key", "sellify_person_id", "person_id"]
        .iter()
        .map(|key| plain_text(contact.get(*key)))
        .find(|text| !text.is_empty())
}

fn render(lead: &Value, detail: Detail, level: usize) -> Value {
    let fields = requested_fields(lead);
    let (person_fields, company_fields): (Vec<&String>, Vec<&String>) = fields
        .iter()
        .partition(|field| field.starts_with("person_"));
    let lead_status = lead.get("field_status").cloned().unwrap_or(Value::Null);
    let empty = Value::Null;

    let mut company = Map::new();
    let mut company_counts = Counts::default();
    for field in &company_fields {
        let status = lead_status.get(field.as_str()).unwrap_or(&empty);
        let name = status_name(status);
        company_counts.add(&name);
        let mut entry = vec![
            Value::String(name.clone()),
            Value::String(clip(&plain_text(status.get("value")), detail.value_chars)),
            Value::String(source_ids(status)),
        ];
        if detail.reason_chars > 0 && name != "verified" {
            let reason = clip(&plain_text(status.get("reason")), detail.reason_chars);
            if !reason.is_empty() {
                entry.push(Value::String(reason));
            }
        }
        company.insert((*field).clone(), Value::Array(entry));
    }

    let person_statuses = lead
        .get("person_field_status")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut contacts = Vec::new();
    let mut person_counts = Counts::default();
    let mut covered = BTreeSet::new();
    let mut open_by_field = std::collections::BTreeMap::<String, usize>::new();
    let contact_list = lead
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // (person key, contact, counted). Like the server's open-field rule, only
    // contacts with a person key are counted per person.
    let mut people: Vec<(Option<String>, Value, bool)> = contact_list
        .into_iter()
        .map(|contact| {
            let key = contact_key(&contact);
            let counted = key.is_some();
            (key, contact, counted)
        })
        .collect();
    for (key, _, _) in &people {
        if let Some(key) = key {
            covered.insert(key.clone());
        }
    }
    // Person statuses stored for a key no contact carries are still stored
    // facts; list them so nothing saved is invisible.
    for key in person_statuses.keys() {
        if !covered.contains(key) {
            people.push((Some(key.clone()), json!({"person_key": key}), false));
        }
    }
    for (key, contact, counted) in &people {
        let counted = *counted;
        let statuses = contact
            .get("field_status")
            .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
            .or_else(|| key.as_ref().and_then(|key| person_statuses.get(key)))
            .cloned()
            .unwrap_or(Value::Null);
        let mut out = Map::new();
        if let Some(key) = key {
            out.insert("key".into(), Value::String(key.clone()));
        }
        if let Some(id) = contact.get("id").and_then(Value::as_str) {
            out.insert("contact_id".into(), Value::String(id.to_string()));
        }
        let mut name = plain_text(contact.get("name"));
        if name.is_empty() {
            name = [
                plain_text(contact.get("person_vorname")),
                plain_text(contact.get("person_nachname")),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        }
        if name.is_empty() {
            name = [
                plain_text(statuses.pointer("/person_vorname/value")),
                plain_text(statuses.pointer("/person_nachname/value")),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        }
        if !name.is_empty() {
            out.insert("name".into(), Value::String(name));
        }
        for (label, members, status_field) in HEADLINE_FIELDS {
            let mut value = members
                .iter()
                .map(|member| plain_text(contact.get(*member)))
                .find(|text| !text.is_empty())
                .unwrap_or_default();
            if value.is_empty() {
                value = plain_text(
                    statuses
                        .get(*status_field)
                        .filter(|status| status_name(status) == "verified")
                        .and_then(|status| status.get("value")),
                );
            }
            if !value.is_empty() {
                out.insert(
                    (*label).to_string(),
                    Value::String(clip(&value, detail.value_chars)),
                );
            }
        }
        if let Some(check) = statuses.get("person_email_validation") {
            let verdict = plain_text(check.get("verdict"));
            let checked = plain_text(check.get("validated_email"));
            let mut text = if verdict.is_empty() {
                status_name(check)
            } else {
                verdict
            };
            let email = out.get("email").and_then(Value::as_str).unwrap_or_default();
            if !checked.is_empty() && !checked.eq_ignore_ascii_case(email) {
                text = format!("{text} ({checked})");
            }
            out.insert("email_check".into(), Value::String(text));
        }
        let mut names = person_fields
            .iter()
            .map(|field| (*field).clone())
            .collect::<Vec<_>>();
        for field in statuses.as_object().into_iter().flat_map(|o| o.keys()) {
            if field.starts_with("person_") && !names.contains(field) {
                names.push(field.clone());
            }
        }
        // Grouped per status and per source id: a contact with eleven fields
        // costs one short line instead of eleven status objects.
        let mut by_status = Map::new();
        let mut by_source = Map::new();
        let mut reasons = Map::new();
        for field in names {
            let status = statuses.get(&field).unwrap_or(&empty);
            let name = status_name(status);
            let short = field.trim_start_matches("person_").to_string();
            let requested = person_fields.iter().any(|f| **f == field);
            if requested && counted {
                person_counts.add(&name);
                if !answered(&name) {
                    *open_by_field.entry(field.clone()).or_default() += 1;
                }
            }
            append_csv(&mut by_status, &name, &short);
            if detail.contact_sources {
                for id in source_ids(status).split(',').filter(|id| !id.is_empty()) {
                    append_csv(&mut by_source, id, &short);
                }
            }
            if detail.reason_chars > 0 && name != "verified" {
                let reason = clip(&plain_text(status.get("reason")), detail.reason_chars);
                if !reason.is_empty() {
                    reasons.insert(short, Value::String(reason));
                }
            }
        }
        out.insert("status".into(), Value::Object(by_status));
        if !by_source.is_empty() {
            out.insert("sources".into(), Value::Object(by_source));
        }
        if !reasons.is_empty() {
            out.insert("reasons".into(), Value::Object(reasons));
        }
        if !counted {
            out.insert("stored_without_contact".into(), Value::Bool(true));
        }
        contacts.push(Value::Object(out));
    }

    let keyed_contacts = people.iter().filter(|(_, _, counted)| *counted).count();
    // Field-level answer, as the writeback receipt reports it: a company field
    // by its own status; a person field is answered only when every contact
    // answered it (without contacts, by the lead-level entry).
    let mut field_answered = Vec::new();
    let mut field_open = Vec::new();
    for field in &fields {
        let is_answered = if field.starts_with("person_") && keyed_contacts > 0 {
            open_by_field.get(field.as_str()).copied().unwrap_or(0) == 0
        } else {
            answered(&status_name(
                lead_status.get(field.as_str()).unwrap_or(&empty),
            ))
        };
        if is_answered {
            field_answered.push(field.clone());
        } else {
            field_open.push(field.clone());
        }
    }

    let mut view = json!({
        "view": COMPACT_LEAD_READBACK_VIEW,
        "record_id": plain_text(lead.get("id")),
        "research_status": plain_text(lead.get("research_status")),
        "updated_at_ms": lead.get("updated_at_ms").cloned().unwrap_or(Value::Null),
        "complete": true,
        "detail_level": level,
        "legend": {
            "company": "field: [status, value, source_ids, reason?]",
            "contacts": "status/sources: person fields without person_ prefix, grouped",
            "counts": "not_found = no_match; open = any other status or missing",
            "omitted": if detail.contact_sources { "quotes, attempts, revisions, evidence, empty values" } else { "quotes, attempts, revisions, evidence, empty values, contact source ids" },
        },
        "counts": {
            "company": company_counts.json(),
            "contacts": person_counts.json(),
            "total": company_counts.plus(person_counts).json(),
            "contacts_count": keyed_contacts,
            "fields_answered": field_answered.len(),
            "fields_open": field_open.len(),
            "open_fields": field_open,
        },
        "company": Value::Object(company),
        "contacts": contacts,
    });
    // Lead-level person entries describe the lead's own contact; without any
    // contact they are the only person facts and are shown as such.
    if keyed_contacts == 0 && !person_fields.is_empty() {
        let mut lead_person = Map::new();
        for field in &person_fields {
            let status = lead_status.get(field.as_str()).unwrap_or(&empty);
            lead_person.insert(
                field.trim_start_matches("person_").to_string(),
                json!([
                    status_name(status),
                    clip(&plain_text(status.get("value")), detail.value_chars),
                    source_ids(status)
                ]),
            );
        }
        view["lead_person"] = Value::Object(lead_person);
    } else if let Some(key) = lead_status
        .pointer("/person_email/person_key")
        .and_then(Value::as_str)
        .filter(|key| !key.trim().is_empty())
    {
        view["lead_contact_key"] = Value::String(key.to_string());
    }
    view
}

/// E-mail addresses a report names that the stored lead does not carry
/// anywhere in its read-back (contact e-mails, checked addresses, company
/// values). Evidence for the review gate; the gate still judges the report.
pub(crate) fn report_emails_not_stored(report: &str, view: &Value) -> Vec<String> {
    let stored = collect_emails(&view.to_string());
    let mut missing = Vec::new();
    for email in collect_emails(report) {
        if !stored.contains(&email) && !missing.contains(&email) {
            missing.push(email);
        }
    }
    missing
}

fn collect_emails(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let is_local = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-');
    let is_domain = |c: char| c.is_alphanumeric() || matches!(c, '.' | '-');
    let chars: Vec<char> = text.chars().collect();
    for (index, c) in chars.iter().enumerate() {
        if *c != '@' {
            continue;
        }
        let mut start = index;
        while start > 0 && is_local(chars[start - 1]) {
            start -= 1;
        }
        let mut end = index + 1;
        while end < chars.len() && is_domain(chars[end]) {
            end += 1;
        }
        let local: String = chars[start..index].iter().collect();
        let domain: String = chars[index + 1..end].iter().collect();
        let local = local.trim_matches('.');
        let domain = domain.trim_matches(|c| c == '.' || c == '-');
        if !local.is_empty() && domain.contains('.') {
            out.insert(format!("{local}@{domain}").to_lowercase());
        }
    }
    out
}

#[cfg(test)]
#[path = "outbound_lead_readback_tests.rs"]
pub(crate) mod tests;
