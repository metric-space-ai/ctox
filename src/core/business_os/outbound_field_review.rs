// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::store;
use crate::lcm;
use crate::mission::channels;

const COLLECTION: &str = "outbound_lead_generation_leads";
const SCHEMA: &str = "ctox.outbound.field_review.v1";

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

/// Only a typed verdict on this exact current writeback reopens a negative.
/// Honest no_match and stale reviews keep their original meaning.
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
        store::update_rxdb_record_conditionally(root, COLLECTION, &record_id, now, |lead| {
            let mut changed = false;
            for claim in scoped {
                if apply_refutation(lead, claim, &binding, &run.run_id, reviewed_at_ms) {
                    applied += 1;
                    changed = true;
                }
            }
            Ok(changed)
        })?;
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
