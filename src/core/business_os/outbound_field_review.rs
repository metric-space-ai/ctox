// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::store;
use crate::lcm;
use crate::mission::channels;

const COLLECTION: &str = "outbound_lead_generation_leads";
const SCHEMA: &str = "ctox.outbound.field_review.v1";

/// Replication permission is not authority to issue native writeback receipts
/// or reviews. Compare full stamped status records with the actual master, so
/// retaining IDs while changing the claim or moving it to another person also
/// fails. Legacy unmarked fields and unrelated authorized lead edits remain
/// writable. This runs inside masterWrite against its CAS previous-state,
/// never against a second, independently read store snapshot.
type FieldStatusKey = (String, String, String);

fn stamped(status: &Value) -> bool {
    ["revision", "review"]
        .iter()
        .any(|key| status.get(*key).is_some_and(|value| !value.is_null()))
}
fn field_status_snapshot(lead: &Value) -> Option<BTreeMap<FieldStatusKey, Value>> {
    let mut protected = BTreeMap::new();
    let mut fields = |scope: &str, person: &str, statuses: &Value| {
        for (field, status) in statuses.as_object().into_iter().flatten() {
            if stamped(status) {
                protected.insert(
                    (scope.to_owned(), person.to_owned(), field.clone()),
                    status.clone(),
                );
            }
        }
    };
    fields("lead", "", &lead["field_status"]);
    for (person, statuses) in lead["person_field_status"]
        .as_object()
        .into_iter()
        .flatten()
    {
        if statuses
            .as_object()
            .is_some_and(|values| values.values().any(stamped))
            && !valid_id(person)
        {
            return None;
        }
        fields("person", person, statuses);
    }
    let contacts = lead["contacts"].as_array();
    let mut people = BTreeMap::<&str, usize>::new();
    for contact in contacts.into_iter().flatten() {
        if let Some(key) = contact["person_key"].as_str() {
            *people.entry(key).or_default() += 1;
        }
    }
    for contact in contacts.into_iter().flatten() {
        let statuses = &contact["field_status"];
        if !statuses
            .as_object()
            .is_some_and(|values| values.values().any(stamped))
        {
            continue;
        }
        let person = contact["person_key"].as_str().filter(|key| valid_id(key))?;
        // An unmarked duplicate must not borrow the stamped one's identity.
        if people.get(person) != Some(&1) {
            return None;
        }
        fields("contact", person, statuses);
    }
    Some(protected)
}

pub(super) fn peer_preserves_native_field_status(
    collection: &str,
    incoming: &Value,
    master: Option<&Value>,
) -> bool {
    if collection != COLLECTION {
        return true;
    }
    let Some(incoming) = field_status_snapshot(incoming) else {
        return false;
    };
    match master {
        Some(master) => field_status_snapshot(master).is_some_and(|stored| incoming == stored),
        None => incoming.is_empty(),
    }
}

// Private native-store evidence. It is neither replicated metadata nor a
// command lifecycle/claim store. One current witness per exact field location
// is committed with the actual normalized document, not with its proposed JSON.
#[derive(Default)]
pub(super) struct NativeFieldStatusWitnesses(BTreeMap<FieldStatusKey, Value>);

pub(super) enum NativeFieldStatusIssuance<'a> {
    Writeback {
        revision: &'a Value,
        keys: &'a BTreeSet<FieldStatusKey>,
    },
    Review(&'a str),
    Preserve,
}

impl NativeFieldStatusWitnesses {
    pub(super) fn load(conn: &Connection, record_id: &str) -> anyhow::Result<Self> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='outbound_native_field_status_witnesses')",
            [], |row| row.get(0),
        )?;
        if !exists {
            return Ok(Self::default());
        }
        let mut stmt = conn.prepare(
            "SELECT scope, person_key, field, status_json FROM outbound_native_field_status_witnesses WHERE record_id=?1",
        )?;
        let rows = stmt.query_map([record_id], |row| {
            Ok((
                (
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ),
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut witnesses = BTreeMap::new();
        for row in rows {
            let (key, raw) = row?;
            witnesses.insert(key, serde_json::from_str(&raw)?);
        }
        Ok(Self(witnesses))
    }

    pub(super) fn retain_current(&mut self, lead: &Value) {
        match field_status_snapshot(lead) {
            Some(current) => self
                .0
                .retain(|key, status| current.get(key) == Some(status)),
            None => self.0.clear(),
        }
    }

    fn contains(&self, scope: &str, person: &str, field: &str, status: &Value) -> bool {
        self.0.get(&(scope.into(), person.into(), field.into())) == Some(status)
    }

    fn permits_claim(&self, lead: &Value, claim: &Refutation) -> bool {
        match &claim.person_key {
            None => self.contains(
                "lead",
                "",
                &claim.field,
                &lead["field_status"][&claim.field],
            ),
            Some(person) => {
                let canonical = &lead["person_field_status"][person][&claim.field];
                if !self.contains("person", person, &claim.field, canonical) {
                    return false;
                }
                // Every contact projection that apply_refutation will touch
                // needs its own exact native issuance, not borrowed IDs.
                lead["contacts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["person_key"] == person.as_str())
                    .all(|c| {
                        let status = &c["field_status"][&claim.field];
                        status["status"] != "no_match"
                            || status.pointer("/revision/writeback_id")
                                != canonical.pointer("/revision/writeback_id")
                            || status.pointer("/revision/command_id")
                                != canonical.pointer("/revision/command_id")
                            || self.contains("contact", person, &claim.field, status)
                    })
            }
        }
    }

    pub(super) fn persist(
        &self,
        conn: &Connection,
        record_id: &str,
        lead: &Value,
        issuance: NativeFieldStatusIssuance<'_>,
    ) -> anyhow::Result<()> {
        let current =
            field_status_snapshot(lead).context("ambiguous native field-status identity")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS outbound_native_field_status_witnesses (
            record_id TEXT NOT NULL, scope TEXT NOT NULL, person_key TEXT NOT NULL,
            field TEXT NOT NULL, status_json TEXT NOT NULL,
            PRIMARY KEY(record_id, scope, person_key, field))",
        )?;
        conn.execute(
            "DELETE FROM outbound_native_field_status_witnesses WHERE record_id=?1",
            [record_id],
        )?;
        let mut stmt = conn.prepare(
            "INSERT INTO outbound_native_field_status_witnesses
            (record_id,scope,person_key,field,status_json) VALUES (?1,?2,?3,?4,?5)",
        )?;
        for (key, status) in current {
            let newly_issued = match &issuance {
                NativeFieldStatusIssuance::Preserve => false,
                NativeFieldStatusIssuance::Writeback { revision, keys } => {
                    keys.contains(&key)
                        && status.get("revision") == Some(*revision)
                        && status.get("review").is_none_or(Value::is_null)
                }
                NativeFieldStatusIssuance::Review(run_id) => {
                    let mut previous = self.0.get(&key).cloned().unwrap_or(Value::Null);
                    let mut reviewed = status.clone();
                    if let Some(obj) = previous.as_object_mut() {
                        obj.remove("review");
                    }
                    if let Some(obj) = reviewed.as_object_mut() {
                        obj.remove("review");
                    }
                    previous.is_object()
                        && previous == reviewed
                        && status
                            .pointer("/review/review_attempt_id")
                            .and_then(Value::as_str)
                            == Some(*run_id)
                        && is_refuted_no_match(&status)
                }
            };
            if self.0.get(&key) == Some(&status) || newly_issued {
                stmt.execute(rusqlite::params![
                    record_id,
                    key.0,
                    key.1,
                    key.2,
                    serde_json::to_string(&status)?
                ])?;
            }
        }
        Ok(())
    }
}

/// A computation-only view: never rewrite a legacy record or manufacture its
/// historical receipt. Shape-valid review JSON without exact native issuance
/// has no authority to reopen a field. Stored evidence remains untouched.
pub(super) fn native_review_view(
    root: &Path,
    record_id: &str,
    lead: &Value,
) -> anyhow::Result<Value> {
    let witnesses = store::load_current_native_field_status_witnesses(root, record_id)?;
    let mut view = lead.clone();
    let retain = |scope: &str, person: &str, fields: &mut Value| {
        for (field, status) in fields.as_object_mut().into_iter().flatten() {
            if status.get("review").is_some_and(|review| !review.is_null())
                && !witnesses.contains(scope, person, field, status)
            {
                status["review"] = Value::Null;
            }
        }
    };
    retain("lead", "", &mut view["field_status"]);
    if let Some(people) = view["person_field_status"].as_object_mut() {
        for (person, fields) in people {
            retain("person", person, fields);
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for contact in lead["contacts"].as_array().into_iter().flatten() {
        if let Some(person) = contact["person_key"].as_str() {
            *counts.entry(person.into()).or_default() += 1;
        }
    }
    if let Some(contacts) = view["contacts"].as_array_mut() {
        for contact in contacts {
            let person = contact["person_key"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if counts.get(&person) == Some(&1) {
                retain("contact", &person, &mut contact["field_status"]);
            } else if let Some(fields) = contact["field_status"].as_object_mut() {
                for status in fields.values_mut() {
                    if status.is_object() {
                        status["review"] = Value::Null;
                    }
                }
            }
        }
    }
    Ok(view)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionRef {
    writeback_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refutation {
    record_id: String,
    field: String,
    #[serde(default)]
    person_key: Option<String>,
    verdict: String,
    claim_status: String,
    revision_ref: RevisionRef,
    reason_code: String,
    #[serde(default)]
    evidence_ref: Option<String>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn parse_refutations(report: &str) -> anyhow::Result<Vec<Refutation>> {
    let mut block = None;
    let mut fence: Option<(u8, usize)> = None;
    for line in report.lines() {
        let line = line.trim();
        let marker = line.as_bytes().first().copied().unwrap_or_default();
        let count = line.bytes().take_while(|byte| *byte == marker).count();
        if let Some((opened, length)) = fence {
            if marker == opened && count >= length && line[count..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if count >= 3 && (marker == b'`' || marker == b'~') {
            fence = Some((marker, count));
            continue;
        }
        if let Some(candidate) = line.strip_prefix("FIELD_REVIEWS:") {
            anyhow::ensure!(block.is_none(), "duplicate FIELD_REVIEWS block");
            block = Some(candidate);
        }
    }
    let Some(block) = block else {
        return Ok(Vec::new());
    };
    anyhow::ensure!(
        block.len() <= 32 * 1024,
        "FIELD_REVIEWS exceeds byte budget"
    );
    let claims: Vec<Refutation> =
        serde_json::from_str(block.trim()).context("FIELD_REVIEWS must be one typed JSON array")?;
    anyhow::ensure!(claims.len() <= 64, "FIELD_REVIEWS exceeds field budget");
    let mut identities = BTreeSet::new();
    for claim in &claims {
        anyhow::ensure!(
            valid_id(&claim.record_id)
                && valid_id(&claim.field)
                && valid_id(&claim.revision_ref.writeback_id),
            "invalid field-review identity"
        );
        anyhow::ensure!(
            claim.verdict == "refuted"
                && claim.claim_status == "no_match"
                && claim.reason_code == "contradicted_by_saved_source",
            "unsupported field-review verdict"
        );
        anyhow::ensure!(
            claim
                .field
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'),
            "invalid field-review field"
        );
        match &claim.person_key {
            Some(key) => anyhow::ensure!(
                valid_id(key) && claim.field.starts_with("person_"),
                "person review requires its exact person key"
            ),
            None => anyhow::ensure!(
                !claim.field.starts_with("person_"),
                "person fields require person_key"
            ),
        }
        if let Some(reference) = &claim.evidence_ref {
            anyhow::ensure!(
                !reference.is_empty()
                    && reference.len() <= 2048
                    && !reference.chars().any(char::is_control),
                "invalid review evidence reference"
            );
        }
        anyhow::ensure!(
            identities.insert((
                claim.record_id.clone(),
                claim.field.clone(),
                claim.person_key.clone()
            )),
            "duplicate field-review identity"
        );
    }
    Ok(claims)
}

#[derive(Debug, Clone)]
struct Binding {
    command_id: String,
    attempt: i64,
}

fn record_binding(root: &Path, message_key: &str) -> anyhow::Result<Vec<(String, Binding)>> {
    let task = channels::load_queue_task(root, message_key)?
        .context("reviewed queue task no longer exists")?;
    let linked = channels::inspect_business_command_for_task(root, message_key)?;
    let gap = task.metadata.get("person_research_gap_closure");
    let command_id = linked
        .as_ref()
        .and_then(|v| v.pointer("/command/command_id"))
        .or_else(|| task.metadata.get("business_os_command_id"))
        .or_else(|| gap.and_then(|v| v.get("research_command_id")))
        .and_then(Value::as_str)
        .context("field review has no native research command binding")?;
    anyhow::ensure!(valid_id(command_id), "invalid review parent command");
    let parent = channels::business_command_projection(root, command_id)?;
    anyhow::ensure!(
        parent["module"] == "outbound-lead-generation",
        "field review parent is not an Outbound command"
    );
    let kind = parent["command_type"].as_str().unwrap_or_default();
    anyhow::ensure!(
        matches!(kind, "business_os.chat.task" | "web_stack.person_research"),
        "field review parent is not research"
    );
    // Recheck the same native execution authority that owns the writeback.
    store::revalidate_business_command_execution_authorization(root, command_id)?;
    let mut records = BTreeSet::new();
    if kind == "business_os.chat.task" {
        let contract = parent
            .pointer("/payload/writeback_contract")
            .context("review parent writeback contract missing")?;
        anyhow::ensure!(
            contract["command_type"] == "outbound.lead.research_writeback",
            "review parent has another writeback mechanism"
        );
        for record in contract["record_ids"]
            .as_array()
            .context("review parent record scope missing")?
        {
            let record = record
                .as_str()
                .context("invalid review parent record scope")?;
            anyhow::ensure!(valid_id(record), "invalid scoped record");
            records.insert(record.to_owned());
        }
    } else {
        let record = parent["record_id"]
            .as_str()
            .context("research record scope missing")?;
        anyhow::ensure!(valid_id(record), "invalid scoped record");
        records.insert(record.to_owned());
    }
    if let Some(gap) = gap {
        anyhow::ensure!(
            gap["research_command_id"] == command_id,
            "gap review parent differs from native command"
        );
        let record = gap["record_id"]
            .as_str()
            .context("gap review record scope missing")?;
        anyhow::ensure!(
            records.contains(record),
            "gap review crosses research record scope"
        );
        records.retain(|id| id == record);
    }
    Ok(records
        .into_iter()
        .map(|record| {
            (
                record,
                Binding {
                    command_id: command_id.to_owned(),
                    attempt: task.attempt,
                },
            )
        })
        .collect())
}

/// JSON shape check only; native consumers must first use native_review_view.
/// This predicate alone authenticates neither issuer nor historical metadata.
pub(super) fn is_refuted_no_match(status: &Value) -> bool {
    let Some(writeback) = status
        .pointer("/revision/writeback_id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
    else {
        return false;
    };
    let Some(command) = status
        .pointer("/revision/command_id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
    else {
        return false;
    };
    let Some(review) = status.get("review") else {
        return false;
    };
    if status["status"] != "no_match"
        || review["schema"] != SCHEMA
        || review["verdict"] != "refuted"
        || review["claim_status"] != "no_match"
        || review
            .pointer("/revision_ref/writeback_id")
            .and_then(Value::as_str)
            != Some(writeback)
        || review["command_id"] != command
        || review["reason_code"] != "contradicted_by_saved_source"
        || !review
            .get("review_attempt_id")
            .and_then(Value::as_str)
            .is_some_and(valid_id)
        || !review
            .get("attempt")
            .and_then(Value::as_i64)
            .is_some_and(|attempt| attempt >= 0)
        || !review
            .get("reviewed_at_ms")
            .and_then(Value::as_i64)
            .is_some_and(|time| time > 0)
    {
        return false;
    }
    match status.get("person_key") {
        Some(Value::String(key)) => valid_id(key) && review["person_key"] == key.as_str(),
        None | Some(Value::Null) => review.get("person_key").is_none_or(Value::is_null),
        _ => false,
    }
}

fn apply_refutation(
    lead: &mut Value,
    claim: &Refutation,
    binding: &Binding,
    review_id: &str,
    reviewed_at_ms: i64,
) -> bool {
    let status = match &claim.person_key {
        Some(person) => {
            let matches = lead["contacts"]
                .as_array()
                .map(|contacts| {
                    contacts
                        .iter()
                        .filter(|contact| contact["person_key"] == person.as_str())
                        .count()
                })
                .unwrap_or(0);
            if matches != 1 {
                return false;
            }
            lead.get_mut("person_field_status")
                .and_then(|people| people.get_mut(person))
                .and_then(|fields| fields.get_mut(&claim.field))
        }
        None => lead
            .get_mut("field_status")
            .and_then(|fields| fields.get_mut(&claim.field)),
    };
    let Some(status) = status else { return false };
    if status["status"] != "no_match"
        || status
            .pointer("/revision/writeback_id")
            .and_then(Value::as_str)
            != Some(claim.revision_ref.writeback_id.as_str())
        || status
            .pointer("/revision/command_id")
            .and_then(Value::as_str)
            != Some(binding.command_id.as_str())
    {
        return false;
    }
    if let Some(person) = &claim.person_key {
        if status["person_key"] != person.as_str() {
            return false;
        }
    }
    let mut review = json!({
        "schema": SCHEMA, "verdict": "refuted", "claim_status": "no_match",
        "review_attempt_id": review_id, "command_id": binding.command_id,
        "attempt": binding.attempt, "reviewed_at_ms": reviewed_at_ms,
        "revision_ref": {"writeback_id": claim.revision_ref.writeback_id},
        "reason_code": "contradicted_by_saved_source",
    });
    if let Some(person) = &claim.person_key {
        review["person_key"] = json!(person);
    }
    if let Some(reference) = &claim.evidence_ref {
        review["evidence_ref"] = json!(reference);
    }
    if status.get("review") == Some(&review) {
        return false;
    }
    status["review"] = review.clone();
    if let Some(person) = &claim.person_key {
        if let Some(contacts) = lead["contacts"].as_array_mut() {
            for contact in contacts
                .iter_mut()
                .filter(|c| c["person_key"] == person.as_str())
            {
                if let Some(projected) = contact
                    .get_mut("field_status")
                    .and_then(|fields| fields.get_mut(&claim.field))
                {
                    if projected
                        .pointer("/revision/writeback_id")
                        .and_then(Value::as_str)
                        == Some(claim.revision_ref.writeback_id.as_str())
                        && projected
                            .pointer("/revision/command_id")
                            .and_then(Value::as_str)
                            == Some(binding.command_id.as_str())
                        && projected["status"] == "no_match"
                    {
                        projected["review"] = review.clone();
                    }
                }
            }
        }
    }
    true
}

/// Only the service calls this after persist_verification_run succeeds.
/// IDs, attempt and record scope are native; the reviewer supplies no authority.
pub(crate) fn publish(
    root: &Path,
    message_keys: &[String],
    run: &lcm::VerificationRunRecord,
) -> anyhow::Result<usize> {
    let claims = parse_refutations(&run.raw_report)?;
    if claims.is_empty() {
        return Ok(0);
    }
    let engine = lcm::LcmEngine::open(
        &root.join("runtime/ctox.sqlite3"),
        lcm::LcmConfig::default(),
    )?;
    let stored = engine
        .verification_run_by_id(run.conversation_id, &run.run_id)?
        .context("native field review audit is not persisted")?;
    anyhow::ensure!(
        stored.raw_report == run.raw_report
            && stored.review_verdict == run.review_verdict
            && stored.created_at == run.created_at,
        "field review differs from its persisted native audit"
    );
    let run = &stored;
    anyhow::ensure!(
        run.review_required && matches!(run.review_verdict.as_str(), "fail" | "partial"),
        "refutations require a durable rejecting review"
    );
    anyhow::ensure!(valid_id(&run.run_id), "native review audit ID missing");
    let mut bindings = BTreeMap::<String, Binding>::new();
    for key in message_keys {
        for (record, binding) in record_binding(root, key)? {
            if let Some(previous) = bindings.get(&record) {
                anyhow::ensure!(
                    previous.command_id == binding.command_id
                        && previous.attempt == binding.attempt,
                    "ambiguous native review scope"
                );
            } else {
                bindings.insert(record, binding);
            }
        }
    }
    for claim in &claims {
        anyhow::ensure!(
            bindings.contains_key(&claim.record_id),
            "field review crosses native record scope"
        );
    }
    let reviewed_at_ms: i64 = run
        .created_at
        .parse()
        .context("native review audit timestamp is invalid")?;
    anyhow::ensure!(reviewed_at_ms > 0, "native review timestamp missing");
    let now = super::person_research_command::now_ms();
    let mut applied = 0;
    for (record_id, binding) in bindings {
        let scoped: Vec<_> = claims.iter().filter(|c| c.record_id == record_id).collect();
        if scoped.is_empty() {
            continue;
        }
        store::update_native_field_review_record(
            root,
            &record_id,
            now,
            &run.run_id,
            |lead, issued| {
                let mut changed = false;
                for claim in scoped {
                    if issued.permits_claim(lead, claim)
                        && apply_refutation(lead, claim, &binding, &run.run_id, reviewed_at_ms)
                    {
                        applied += 1;
                        changed = true;
                    }
                }
                Ok(changed)
            },
        )?;
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(person: Option<&str>) -> Refutation {
        Refutation {
            record_id: "lead-a".into(),
            field: if person.is_some() {
                "person_email"
            } else {
                "firma_prokura"
            }
            .into(),
            person_key: person.map(str::to_owned),
            verdict: "refuted".into(),
            claim_status: "no_match".into(),
            revision_ref: RevisionRef {
                writeback_id: "wb-a".into(),
            },
            reason_code: "contradicted_by_saved_source".into(),
            evidence_ref: Some("saved/source.json".into()),
        }
    }

    fn binding() -> Binding {
        Binding {
            command_id: "research-a".into(),
            attempt: 8,
        }
    }

    fn negative() -> Value {
        json!({"status": "no_match", "value": null, "revision": {
            "writeback_id": "wb-a", "command_id": "research-a", "attempt": 8,
            "written_at_ms": 1
        }})
    }

    #[test]
    fn peers_cannot_issue_erase_replay_or_launder_native_field_status() {
        let mut reviewed = json!({"field_status": {"firma_prokura": negative()}});
        assert!(apply_refutation(
            &mut reviewed,
            &claim(None),
            &binding(),
            "review-a",
            2
        ));
        let status = reviewed["field_status"]["firma_prokura"].clone();
        let locations = [
            ("/field_status/firma_prokura", reviewed),
            (
                "/person_field_status/person-a/person_email",
                json!({
                    "person_field_status": {"person-a": {"person_email": status.clone()}}
                }),
            ),
            (
                "/contacts/0/field_status/person_email",
                json!({"contacts": [{
                    "person_key": "person-a", "field_status": {"person_email": status.clone()}
                }]}),
            ),
        ];
        for (path, mut master) in locations {
            master["id"] = json!("lead-a");
            let mut ordinary = master.clone();
            ordinary["campaign_id"] = json!("another-campaign");
            ordinary["updated_at_ms"] = json!(100);
            assert!(peer_preserves_native_field_status(
                COLLECTION,
                &ordinary,
                Some(&master)
            ));
            ordinary["_deleted"] = json!(true);
            assert!(peer_preserves_native_field_status(
                COLLECTION,
                &ordinary,
                Some(&master)
            ));
            assert!(!peer_preserves_native_field_status(
                COLLECTION, &master, None
            ));
            assert!(!peer_preserves_native_field_status(
                COLLECTION,
                &master,
                Some(&json!({"id": "lead-a"}))
            ));
            for (key, value) in [
                ("revision", Value::Null),
                ("review", Value::Null),
                ("revision", json!({"writeback_id": "borrowed"})),
                ("review", json!({"schema": SCHEMA, "verdict": "refuted"})),
                ("status", json!("verified")),
                ("value", json!("invented claim")),
                ("person_key", json!("person-other")),
            ] {
                let mut forged = master.clone();
                forged.pointer_mut(path).unwrap()[key] = value;
                assert!(
                    !peer_preserves_native_field_status(COLLECTION, &forged, Some(&master)),
                    "{path}: {key}"
                );
            }
            for key in ["revision", "review"] {
                let mut forged = master.clone();
                forged
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(!peer_preserves_native_field_status(
                    COLLECTION,
                    &forged,
                    Some(&master)
                ));
            }
            let mut erased = master.clone();
            *erased.pointer_mut(path).unwrap() = Value::Null;
            assert!(!peer_preserves_native_field_status(
                COLLECTION,
                &erased,
                Some(&master)
            ));
            let mut newer = master.clone();
            newer.pointer_mut(path).unwrap()["revision"]["writeback_id"] = json!("wb-new");
            assert!(!peer_preserves_native_field_status(
                COLLECTION,
                &master,
                Some(&newer)
            ));
        }
    }

    #[test]
    fn peer_field_status_guard_preserves_legacy_edits_and_exact_person_identity() {
        let legacy = json!({"id": "lead-a", "field_status": {
            "firma_prokura": {"status": "no_match", "value": null, "review": null}
        }});
        assert!(peer_preserves_native_field_status(
            COLLECTION, &legacy, None
        ));
        let mut edited = legacy.clone();
        edited["field_status"]["firma_prokura"]["status"] = json!("pending");
        assert!(peer_preserves_native_field_status(
            COLLECTION,
            &edited,
            Some(&legacy)
        ));
        assert!(peer_preserves_native_field_status(
            "unrelated",
            &negative(),
            None
        ));
        let master = json!({"contacts": [
            {"person_key": "person-a", "field_status": {"person_email": negative()}},
            {"person_key": "person-b", "name": "Other"}
        ]});
        let mut reordered = master.clone();
        reordered["contacts"].as_array_mut().unwrap().reverse();
        assert!(peer_preserves_native_field_status(
            COLLECTION,
            &reordered,
            Some(&master)
        ));
        for key in ["person-b", "", " person-a "] {
            let mut moved = master.clone();
            moved["contacts"][0]["person_key"] = json!(key);
            assert!(!peer_preserves_native_field_status(
                COLLECTION,
                &moved,
                Some(&master)
            ));
        }
        let mut duplicated = master.clone();
        duplicated["contacts"][1]["person_key"] = json!("person-a");
        assert!(!peer_preserves_native_field_status(
            COLLECTION,
            &duplicated,
            Some(&master)
        ));
        let mut missing_key = master.clone();
        missing_key["contacts"][0]
            .as_object_mut()
            .unwrap()
            .remove("person_key");
        assert!(!peer_preserves_native_field_status(
            COLLECTION,
            &missing_key,
            Some(&master)
        ));
    }

    #[test]
    fn typed_parser_rejects_extra_authority_duplicates_and_freetext() {
        assert!(parse_refutations("FINDINGS: firma_prokura is contradicted")
            .unwrap()
            .is_empty());
        assert!(parse_refutations("FIELD_REVIEWS: []\nFIELD_REVIEWS: []").is_err());
        let mut value = json!({"record_id": "lead-a", "field": "firma_prokura",
            "verdict": "refuted", "claim_status": "no_match",
            "revision_ref": {"writeback_id": "wb-a"},
            "reason_code": "contradicted_by_saved_source", "command_id": "borrowed"});
        assert!(parse_refutations(&format!("FIELD_REVIEWS: [{}]", value)).is_err());
        value.as_object_mut().unwrap().remove("command_id");
        assert_eq!(
            parse_refutations(&format!("FIELD_REVIEWS: [{}]", value))
                .unwrap()
                .len(),
            1
        );
        assert!(parse_refutations(&format!("FIELD_REVIEWS: [{value},{value}]")).is_err());
        let header = format!("FIELD_REVIEWS: [{value}]");
        for fence in ["```", "~~~~", "````"] {
            let quoted = format!("EVIDENCE:\n{fence}json\n{header}\n{fence}\n");
            assert!(parse_refutations(&quoted).unwrap().is_empty());
            let with_verdict = format!("{quoted}{header}");
            assert_eq!(parse_refutations(&with_verdict).unwrap().len(), 1);
        }
        assert!(parse_refutations(&format!("````json\n```\n{header}\n````"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn refutation_reopens_only_the_matching_negative_and_native_parent() {
        let mut lead = json!({"field_status": {"firma_prokura": negative()}});
        assert!(!is_refuted_no_match(&lead["field_status"]["firma_prokura"]));
        let before = lead.clone();
        let mut wrong = binding();
        wrong.command_id = "another-research".into();
        assert!(!apply_refutation(
            &mut lead,
            &claim(None),
            &wrong,
            "review-a",
            2
        ));
        assert_eq!(lead, before);
        assert!(apply_refutation(
            &mut lead,
            &claim(None),
            &binding(),
            "review-a",
            2
        ));
        let status = &lead["field_status"]["firma_prokura"];
        assert_eq!(status["status"], "no_match");
        assert_eq!(status["review"]["review_attempt_id"], "review-a");
        assert_eq!(status["review"]["attempt"], 8);
        assert!(is_refuted_no_match(status));
        let reviewed = lead.clone();
        assert!(!apply_refutation(
            &mut lead,
            &claim(None),
            &binding(),
            "review-a",
            2
        ));
        assert_eq!(lead, reviewed);
        lead["field_status"]["firma_prokura"]["revision"]["writeback_id"] = json!("wb-new");
        let newer = lead.clone();
        assert!(!apply_refutation(
            &mut lead,
            &claim(None),
            &binding(),
            "review-old",
            1
        ));
        assert_eq!(lead, newer);
        assert!(!is_refuted_no_match(&lead["field_status"]["firma_prokura"]));
    }

    #[test]
    fn person_refutation_does_not_cross_people_or_ambiguous_contacts() {
        let mut a = negative();
        a["person_key"] = json!("person-a");
        let mut b = negative();
        b["person_key"] = json!("person-b");
        let mut lead = json!({"person_field_status": {
            "person-a": {"person_email": a}, "person-b": {"person_email": b}
        }, "contacts": [
            {"person_key": "person-a", "field_status": {"person_email": a}},
            {"person_key": "person-b", "field_status": {"person_email": b}}
        ]});
        let before_b = lead["person_field_status"]["person-b"].clone();
        assert!(apply_refutation(
            &mut lead,
            &claim(Some("person-a")),
            &binding(),
            "review-a",
            2
        ));
        assert!(is_refuted_no_match(
            &lead["person_field_status"]["person-a"]["person_email"]
        ));
        assert_eq!(lead["person_field_status"]["person-b"], before_b);
        assert_eq!(
            lead["contacts"][0]["field_status"]["person_email"]["review"],
            lead["person_field_status"]["person-a"]["person_email"]["review"]
        );
        lead["contacts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"person_key": "person-a"}));
        let ambiguous = lead.clone();
        assert!(!apply_refutation(
            &mut lead,
            &claim(Some("person-a")),
            &binding(),
            "review-b",
            3
        ));
        assert_eq!(lead, ambiguous);
    }

    #[test]
    fn person_projection_keeps_a_newer_parent_or_native_validation() {
        for (revision, status) in [
            (
                json!({"writeback_id": "wb-new", "command_id": "research-a"}),
                "no_match",
            ),
            (
                json!({"writeback_id": "wb-a", "command_id": "research-other"}),
                "no_match",
            ),
            (
                json!({"writeback_id": "wb-a", "command_id": "research-a"}),
                "verified",
            ),
        ] {
            let mut canonical = negative();
            canonical["person_key"] = json!("person-a");
            let mut projected = canonical.clone();
            projected["revision"] = revision;
            projected["status"] = json!(status);
            let mut lead = json!({
                "person_field_status": {"person-a": {"person_email": canonical}},
                "contacts": [{"person_key": "person-a", "field_status": {"person_email": projected}}]
            });
            let contact = lead["contacts"][0].clone();
            assert!(apply_refutation(
                &mut lead,
                &claim(Some("person-a")),
                &binding(),
                "review-a",
                2
            ));
            assert!(is_refuted_no_match(
                &lead["person_field_status"]["person-a"]["person_email"]
            ));
            assert_eq!(lead["contacts"][0], contact);
        }
    }

    #[test]
    fn conditional_publication_never_resurrects_a_tombstone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::open_store(root)?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        store::upsert_rxdb_collection_record(
            root,
            COLLECTION,
            "lead-a",
            1,
            json!({"id": "lead-a", "field_status": {"firma_prokura": negative()}}),
        )?;
        let conn = rusqlite::Connection::open(store::rxdb_store_path(root))?;
        conn.execute(
            "UPDATE ctox_business_os__outbound_lead_generation_leads__v0 SET deleted=1 WHERE id='lead-a'", []
        )?;
        let before: String = conn.query_row(
            "SELECT data FROM ctox_business_os__outbound_lead_generation_leads__v0 WHERE id='lead-a'",
            [], |row| row.get(0)
        )?;
        let mut invoked = false;
        assert!(!store::update_rxdb_record_conditionally(
            root,
            COLLECTION,
            "lead-a",
            2,
            |_| {
                invoked = true;
                Ok(true)
            }
        )?);
        assert!(!invoked);
        let after: (String, i64) = conn.query_row(
            "SELECT data, deleted FROM ctox_business_os__outbound_lead_generation_leads__v0 WHERE id='lead-a'",
            [], |row| Ok((row.get(0)?, row.get(1)?))
        )?;
        assert_eq!(after, (before, 1));
        Ok(())
    }

    fn audit(root: &Path) -> anyhow::Result<lcm::VerificationRunRecord> {
        std::fs::create_dir_all(root.join("runtime"))?;
        let request = crate::mission::verification::SliceVerificationRequest {
            conversation_id: 7,
            goal: "Review Outbound research".into(),
            prompt: "Check the saved source".into(),
            preview: "Review lead-a".into(),
            source_label: "queue".into(),
            owner_visible: false,
        };
        let mut run = crate::mission::verification::record_slice_assurance(
            root,
            &request,
            "Research remains open",
            None,
            None,
        )?
        .run;
        run.run_id = "review-field-a".into();
        run.review_required = true;
        run.review_verdict = "fail".into();
        run.raw_report = format!(
            "FIELD_REVIEWS: [{}]",
            json!({
                "record_id": "lead-a", "field": "firma_prokura", "verdict": "refuted",
                "claim_status": "no_match", "revision_ref": {"writeback_id": "wb-a"},
                "reason_code": "contradicted_by_saved_source"
            })
        );
        Ok(run)
    }

    #[test]
    fn publication_requires_the_persisted_audit_and_native_task_scope() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::open_store(root)?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        store::upsert_rxdb_collection_record(
            root,
            COLLECTION,
            "lead-a",
            1,
            json!({"id": "lead-a", "field_status": {"firma_prokura": negative()}}),
        )?;
        let before = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let run = audit(root)?;
        assert!(publish(root, &[], &run)
            .unwrap_err()
            .to_string()
            .contains("not persisted"));
        let engine = lcm::LcmEngine::open(
            &root.join("runtime/ctox.sqlite3"),
            lcm::LcmConfig::default(),
        )?;
        engine.persist_verification_run(&run, &[])?;
        assert!(publish(root, &[], &run)
            .unwrap_err()
            .to_string()
            .contains("record scope"));
        assert!(publish(root, &["missing-task".into()], &run).is_err());
        let mut altered = run.clone();
        altered.raw_report = altered.raw_report.replace("wb-a", "wb-other");
        assert!(publish(root, &[], &altered)
            .unwrap_err()
            .to_string()
            .contains("differs"));
        altered = run.clone();
        altered.conversation_id += 1;
        assert!(publish(root, &[], &altered)
            .unwrap_err()
            .to_string()
            .contains("not persisted"));
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap(),
            before
        );
        Ok(())
    }

    #[test]
    fn publication_follows_admitted_research_scope_and_rechecks_actor_authority(
    ) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::tests::seed_business_user(root, "operator", "admin")?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        let now = super::super::person_research_command::now_ms();
        let (token, _) = store::issue_business_os_capability_token(root, "operator", now)?;
        let accepted = store::accept_rxdb_business_command_with_origin(
            root,
            json!({
                "id": "research-a", "command_id": "research-a",
                "module": "outbound-lead-generation", "command_type": "business_os.chat.task",
                "record_id": "lead-a", "status": "pending_sync",
                "payload": {
                    "title": "Research lead-a", "instruction": "Research the company and write back the saved evidence.",
                    "fields": ["firma_prokura"],
                    "writeback_contract": {"command_type": "outbound.lead.research_writeback",
                        "collection": COLLECTION, "record_ids": ["lead-a"]}
                },
                "client_context": {"actor": {"id": "operator", "role": "admin"},
                    "capability_token": token}
            }),
            store::CommandOrigin::ReplicatedPeer,
        )?;
        assert_eq!(accepted["status"], "accepted");
        let parent = channels::business_command_projection(root, "research-a")?;
        let task_key = parent["execution_task_id"]
            .as_str()
            .context("execution task missing")?
            .to_owned();
        assert!(!task_key.is_empty());
        store::upsert_rxdb_collection_record(
            root,
            COLLECTION,
            "lead-a",
            1,
            json!({"id": "lead-a", "field_status": {"firma_prokura": negative()}}),
        )?;
        let run = audit(root)?;
        let engine = lcm::LcmEngine::open(
            &root.join("runtime/ctox.sqlite3"),
            lcm::LcmConfig::default(),
        )?;
        engine.persist_verification_run(&run, &[])?;
        // A durable audit plus shape-valid IDs do not issue a historical
        // writeback. Only the native producer's atomic witness permits it.
        let legacy = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let mut shaped = legacy.clone();
        assert!(apply_refutation(
            &mut shaped,
            &claim(None),
            &binding(),
            "review-a",
            2
        ));
        assert!(is_refuted_no_match(
            &shaped["field_status"]["firma_prokura"]
        ));
        assert!(
            !is_refuted_no_match(
                &native_review_view(root, "lead-a", &shaped)?["field_status"]["firma_prokura"]
            ),
            "shape-valid audit/writeback IDs alone authenticate nothing"
        );
        assert_eq!(publish(root, &[task_key.clone()], &run)?, 0);
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?,
            Some(legacy.clone())
        );
        store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            1,
            legacy,
            &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap_or(Value::Null),
            &negative()["revision"],
            &BTreeSet::from([("lead".into(), "".into(), "firma_prokura".into())]),
        )?;
        assert_eq!(publish(root, &[task_key.clone()], &run)?, 1);
        let reviewed = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let task = channels::load_queue_task(root, &task_key)?.context("task missing")?;
        assert_eq!(
            reviewed["field_status"]["firma_prokura"]["review"]["attempt"],
            task.attempt
        );
        assert_eq!(
            reviewed["field_status"]["firma_prokura"]["review"]["review_attempt_id"],
            run.run_id
        );
        assert!(is_refuted_no_match(
            &reviewed["field_status"]["firma_prokura"]
        ));
        let native_view = native_review_view(root, "lead-a", &reviewed)?;
        assert_eq!(
            native_view["field_status"]["firma_prokura"],
            reviewed["field_status"]["firma_prokura"]
        );
        assert!(is_refuted_no_match(
            &native_view["field_status"]["firma_prokura"]
        ));
        // Neither another record nor another field can borrow the witness.
        let copied = native_review_view(root, "lead-other", &reviewed)?;
        assert!(!is_refuted_no_match(
            &copied["field_status"]["firma_prokura"]
        ));
        let mut copied = reviewed.clone();
        copied["field_status"]["firma_telefon"] = copied["field_status"]["firma_prokura"].clone();
        let copied = native_review_view(root, "lead-a", &copied)?;
        assert!(!is_refuted_no_match(
            &copied["field_status"]["firma_telefon"]
        ));
        let mut changed = reviewed.clone();
        changed["field_status"]["firma_prokura"]["value"] = json!("changed claim");
        assert!(!is_refuted_no_match(
            &native_review_view(root, "lead-a", &changed)?["field_status"]["firma_prokura"]
        ));
        assert_eq!(publish(root, &[task_key.clone()], &run)?, 0);
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap(),
            reviewed
        );

        let mut outside = run.clone();
        outside.run_id = "review-outside".into();
        outside.raw_report = run.raw_report.replace("lead-a", "lead-b");
        engine.persist_verification_run(&outside, &[])?;
        assert!(publish(root, &[task_key.clone()], &outside)
            .unwrap_err()
            .to_string()
            .contains("record scope"));
        assert!(store::load_rxdb_collection_record(root, COLLECTION, "lead-b")?.is_none());

        let conn = store::open_store(root)?;
        conn.execute(
            "UPDATE business_users SET role = 'user' WHERE user_id = 'operator'",
            [],
        )?;
        drop(conn);
        assert!(publish(root, &[task_key], &run).is_err());
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap(),
            reviewed
        );
        Ok(())
    }

    #[test]
    fn native_producers_cannot_erase_a_concurrently_published_field_review() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::tests::seed_business_user(root, "operator", "admin")?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        let now = super::super::person_research_command::now_ms();
        let (token, _) = store::issue_business_os_capability_token(root, "operator", now)?;
        let accepted = store::accept_rxdb_business_command_with_origin(
            root,
            json!({
                "id":"research-a", "command_id":"research-a", "module":"outbound-lead-generation",
                "command_type":"business_os.chat.task", "record_id":"lead-a", "status":"pending_sync",
                "payload":{"title":"Research lead-a", "instruction":"Research and persist the saved evidence.",
                    "fields":["firma_prokura"], "writeback_contract":{"command_type":"outbound.lead.research_writeback",
                        "collection":COLLECTION, "record_ids":["lead-a"]}},
                "client_context":{"actor":{"id":"operator", "role":"admin"}, "capability_token":token}
            }),
            store::CommandOrigin::ReplicatedPeer,
        )?;
        assert_eq!(accepted["status"], "accepted");
        let parent = channels::business_command_projection(root, "research-a")?;
        let task_key = parent["execution_task_id"]
            .as_str()
            .context("native task missing")?
            .to_owned();
        let lead = json!({"id":"lead-a", "field_status":{"firma_prokura":negative()}});
        store::upsert_rxdb_collection_record(root, COLLECTION, "lead-a", 1, lead)?;
        let initial = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            1,
            initial.clone(),
            &initial,
            &negative()["revision"],
            &BTreeSet::from([("lead".into(), "".into(), "firma_prokura".into())]),
        )?;
        // Both producers retain this old full document while publication wins
        // the next transaction. The field they will deliver is different.
        let stale_master = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let run = audit(root)?;
        let engine = lcm::LcmEngine::open(
            &root.join("runtime/ctox.sqlite3"),
            lcm::LcmConfig::default(),
        )?;
        engine.persist_verification_run(&run, &[])?;
        assert_eq!(publish(root, &[task_key], &run)?, 1);
        let current = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let conn = Connection::open(store::rxdb_store_path(root))?;
        let current_witnesses = NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0;
        assert!(is_refuted_no_match(
            &current["field_status"]["firma_prokura"]
        ));
        assert_eq!(
            current_witnesses[&("lead".into(), "".into(), "firma_prokura".into())],
            current["field_status"]["firma_prokura"]
        );
        let revision = json!({"writeback_id":"wb-phone", "command_id":"research-a", "attempt":8, "written_at_ms":3});
        let phone = json!({"status":"verified", "value":"+4940636841000", "revision":revision, "review":null});
        let keys = BTreeSet::from([("lead".into(), "".into(), "firma_telefon".into())]);
        let mut stale = stale_master.clone();
        stale["field_status"]["firma_telefon"] = phone.clone();
        let error = store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            3,
            stale,
            &stale_master,
            &revision,
            &keys,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("changed before persistence"),
            "{error:#}"
        );
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?,
            Some(current.clone())
        );
        assert_eq!(
            NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0,
            current_witnesses
        );
        let mut stale_email = stale_master.clone();
        stale_email["payload"] = json!({"email_validation_pass":{"at_ms":3, "checked":[]}});
        let error = store::upsert_native_email_validation_record(
            root,
            "lead-a",
            3,
            stale_email,
            &stale_master,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("changed before persistence"),
            "{error:#}"
        );
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?,
            Some(current.clone())
        );
        assert_eq!(
            NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0,
            current_witnesses
        );
        // A fresh producer can still deliver another field and must retain
        // both the newer untouched review and its exact issuance witness.
        let mut fresh = current.clone();
        fresh["field_status"]["firma_telefon"] = phone;
        store::upsert_native_research_writeback_record(
            root, "lead-a", 4, fresh, &current, &revision, &keys,
        )?;
        let fresh = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        assert_eq!(
            fresh["field_status"]["firma_prokura"],
            current["field_status"]["firma_prokura"]
        );
        let fresh_witnesses = NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0;
        assert_eq!(
            fresh_witnesses[&("lead".into(), "".into(), "firma_prokura".into())],
            current_witnesses[&("lead".into(), "".into(), "firma_prokura".into())]
        );
        let mut fresh_email = fresh.clone();
        fresh_email["payload"] = json!({"email_validation_pass":{"at_ms":5, "checked":[]}});
        store::upsert_native_email_validation_record(root, "lead-a", 5, fresh_email, &fresh)?;
        assert_eq!(
            NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0,
            fresh_witnesses,
            "native email may preserve witnesses, never issue another writeback"
        );
        assert!(is_refuted_no_match(
            &native_review_view(
                root,
                "lead-a",
                &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap()
            )?["field_status"]["firma_prokura"]
        ));
        Ok(())
    }

    #[test]
    fn native_field_status_witnesses_bind_people_and_rollback_a_clamped_issuance(
    ) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::open_store(root)?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        let mut status = negative();
        status["person_key"] = json!("person-a");
        let lead = json!({"id":"lead-a", "field_status":{},
            "person_field_status":{"person-a":{"person_email":status}},
            "contacts":[{"person_key":"person-a", "field_status":{"person_email":status}}]});
        let keys = BTreeSet::from([
            ("person".into(), "person-a".into(), "person_email".into()),
            ("contact".into(), "person-a".into(), "person_email".into()),
        ]);
        store::upsert_rxdb_collection_record(root, COLLECTION, "lead-a", 1, lead.clone())?;
        store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            1,
            lead,
            &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap_or(Value::Null),
            &negative()["revision"],
            &keys,
        )?;
        let mut claim = claim(Some("person-a"));
        claim.field = "person_email".into();
        assert!(store::update_native_field_review_record(
            root,
            "lead-a",
            2,
            "review-a",
            |lead, issued| {
                assert!(issued.permits_claim(lead, &claim));
                Ok(apply_refutation(lead, &claim, &binding(), "review-a", 2))
            }
        )?);
        let saved = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        let view = native_review_view(root, "lead-a", &saved)?;
        assert!(is_refuted_no_match(
            &view["person_field_status"]["person-a"]["person_email"]
        ));
        assert!(is_refuted_no_match(
            &view["contacts"][0]["field_status"]["person_email"]
        ));
        let mut changed = saved.clone();
        changed["contacts"][0]["person_key"] = json!("person-b");
        changed["contacts"][0]["field_status"]["person_email"]["person_key"] = json!("person-b");
        changed["contacts"][0]["field_status"]["person_email"]["review"]["person_key"] =
            json!("person-b");
        assert!(!is_refuted_no_match(
            &native_review_view(root, "lead-a", &changed)?["contacts"][0]["field_status"]
                ["person_email"]
        ));
        let mut changed = saved.clone();
        let duplicate = changed["contacts"][0].clone();
        changed["contacts"].as_array_mut().unwrap().push(duplicate);
        assert!(!is_refuted_no_match(
            &native_review_view(root, "lead-a", &changed)?["contacts"][0]["field_status"]
                ["person_email"]
        ));
        let conn = Connection::open(store::rxdb_store_path(root))?;
        let before = NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0;
        let mut too_large = saved.clone();
        let mut oversized = negative();
        oversized["reason"] = json!("native evidence ".repeat(200));
        too_large["field_status"]["firma_prokura"] = oversized;
        for n in 0..310 {
            too_large[format!("ordinary_{n}")] = json!("x".repeat(900));
        }
        assert!(store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            3,
            too_large,
            &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap_or(Value::Null),
            &negative()["revision"],
            &BTreeSet::from([("lead".into(), "".into(), "firma_prokura".into())])
        )
        .is_err());
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?,
            Some(saved.clone())
        );
        assert_eq!(NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0, before);
        // Even a real old witness cannot authenticate a stale caller copy
        // after a canonical change or deletion by another native producer.
        let mut changed = saved.clone();
        changed["person_field_status"]["person-a"]["person_email"]["value"] = json!("new claim");
        store::upsert_rxdb_collection_record(root, COLLECTION, "lead-a", 4, changed)?;
        assert!(!is_refuted_no_match(
            &native_review_view(root, "lead-a", &saved)?["person_field_status"]["person-a"]
                ["person_email"]
        ));
        // Ordinary upserts revive rows; exercise the actual native tombstone
        // path before checking that a saved caller copy has lost authority.
        {
            let mut writer = store::BusinessProjectionWriter::open(root)?;
            writer.tombstone_source_projection(COLLECTION, "lead-a", 5)?;
        }
        let deleted = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        assert_eq!(deleted["_deleted"], true);
        assert_eq!(deleted["is_deleted"], true);
        assert!(!is_refuted_no_match(
            &native_review_view(root, "lead-a", &saved)?["contacts"][0]["field_status"]
                ["person_email"]
        ));
        assert!(!store::update_native_field_review_record(
            root,
            "lead-a",
            6,
            "review-other",
            |_, _| {
                panic!("deleted records never reach publication");
            }
        )?);
        assert!(store::upsert_native_research_writeback_record(
            root,
            "lead-a",
            6,
            saved.clone(),
            &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap_or(Value::Null),
            &negative()["revision"],
            &keys
        )
        .is_err());
        assert!(store::upsert_native_research_writeback_record(
            root,
            "missing",
            6,
            saved,
            &store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap_or(Value::Null),
            &negative()["revision"],
            &keys
        )
        .is_err());
        assert_eq!(NativeFieldStatusWitnesses::load(&conn, "lead-a")?.0, before);
        Ok(())
    }

    #[test]
    fn persisted_update_rejects_a_newer_revision_and_preserves_other_fields() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        store::open_store(root)?;
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root, COLLECTION,
        )?;
        let mut status = negative();
        status["revision"]["writeback_id"] = json!("wb-new");
        store::upsert_rxdb_collection_record(
            root,
            COLLECTION,
            "lead-a",
            1,
            json!({"id": "lead-a", "firma_name": "Unchanged AG",
                "field_status": {"firma_prokura": status}}),
        )?;
        let before = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        assert!(!store::update_rxdb_record_conditionally(
            root,
            COLLECTION,
            "lead-a",
            2,
            |lead| Ok(apply_refutation(
                lead,
                &claim(None),
                &binding(),
                "review-a",
                2
            ))
        )?);
        assert_eq!(
            store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap(),
            before
        );
        let mut current = claim(None);
        current.revision_ref.writeback_id = "wb-new".into();
        assert!(store::update_rxdb_record_conditionally(
            root,
            COLLECTION,
            "lead-a",
            2,
            |lead| Ok(apply_refutation(lead, &current, &binding(), "review-a", 2))
        )?);
        let after = store::load_rxdb_collection_record(root, COLLECTION, "lead-a")?.unwrap();
        assert_eq!(after["firma_name"], "Unchanged AG");
        assert_eq!(
            after["field_status"]["firma_prokura"]["revision"],
            before["field_status"]["firma_prokura"]["revision"]
        );
        assert!(is_refuted_no_match(&after["field_status"]["firma_prokura"]));
        assert!(!store::update_rxdb_record_conditionally(
            root,
            COLLECTION,
            "missing",
            3,
            |_| Ok(true)
        )?);
        assert!(store::load_rxdb_collection_record(root, COLLECTION, "missing")?.is_none());
        Ok(())
    }
}
