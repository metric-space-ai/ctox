use super::store::{upsert_business_record, BusinessCommand};
use crate::mission::channels;
use anyhow::{anyhow, Context};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ctox_web_stack::sources::{Country, FieldKey, ResearchMode};
use ctox_web_stack::PersonResearchRequest;
use regex::Regex;
use rusqlite::Connection;
use scraper::{Html, Selector};
use serde_json::Map;
use serde_json::Value;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(super) struct ImportOutcome {
    pub collection: String,
    pub definition_id: String,
    pub record_ids: Vec<String>,
    pub records_count: usize,
}

pub(super) fn handle_source_parse(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    if command.module == "outbound" && command.command_type == "outbound.source.import" {
        return import_outbound_companies(root, conn, command_id, command, queue_task);
    }
    anyhow::ensure!(
        command.module == "matching",
        "source.parse is only implemented for matching"
    );
    if command.command_type == "matching.source.parse_requirement" {
        return import_matching_requirement(root, conn, command_id, command, queue_task);
    }
    if command.command_type == "matching.source.parse_object" {
        return import_matching_objects(root, conn, command_id, command, queue_task);
    }
    let source_type = str_path(&command.payload, &["source_type"]);
    let column = str_path(&command.payload, &["column"]);
    match (column.as_str(), source_type.as_str()) {
        ("requirements", "url") => {
            import_requirement_url(root, conn, command_id, command, queue_task)
        }
        ("objects", "document") | ("candidates", "document") => {
            import_candidate_documents(root, conn, command_id, command, queue_task)
        }
        _ => Err(anyhow!(
            "unsupported matching import source: column={column}, source_type={source_type}"
        )),
    }
}

pub(super) fn handle_match_compute(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    anyhow::ensure!(
        command.module == "matching",
        "match.compute is only implemented for matching"
    );
    if command.command_type == "matching.match" {
        return compute_matching_result(root, conn, command_id, command, queue_task);
    }
    let company_id = first_nonempty(&[
        str_path(&command.client_context, &["companyId"]),
        str_path(
            &command.payload,
            &["options", "businessContext", "companyId"],
        ),
    ]);
    let job_id = first_nonempty(&[
        str_path(&command.client_context, &["jobId"]),
        str_path(&command.payload, &["options", "businessContext", "jobId"]),
    ]);
    let candidate_id = first_nonempty(&[
        str_path(&command.client_context, &["candidateId"]),
        str_path(
            &command.payload,
            &["options", "businessContext", "candidateId"],
        ),
    ]);
    let match_id = command
        .record_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{company_id}|{job_id}|{candidate_id}"));
    anyhow::ensure!(!company_id.is_empty(), "match.compute requires companyId");
    anyhow::ensure!(!job_id.is_empty(), "match.compute requires jobId");
    anyhow::ensure!(
        !candidate_id.is_empty(),
        "match.compute requires candidateId"
    );

    let job = load_collection_payload(conn, "jobs", &job_id)
        .with_context(|| format!("job not found: {job_id}"))?;
    let candidate = load_collection_payload(conn, "candidates", &candidate_id)
        .with_context(|| format!("candidate not found: {candidate_id}"))?;
    let now_iso = now_iso();
    let now_ms = now_ms() as i64;
    let job_text = job_text(&job);
    let cv_text = candidate_text(&candidate);
    let items = build_match_items(
        &company_id,
        &job_id,
        &candidate_id,
        &job_text,
        &cv_text,
        &now_iso,
    );
    anyhow::ensure!(!items.is_empty(), "match.compute produced zero items");
    let score = total_match_score(&items);
    let match_doc = serde_json::json!({
        "id": match_id,
        "companyId": company_id,
        "jobId": job_id,
        "candidateId": candidate_id,
        "active": true,
        "removed": false,
        "progress": 10,
        "status": "prematch",
        "statuses": [],
        "score": score,
        "notes": "",
        "interview": {
            "attendees": [],
            "reminders": []
        },
        "events": [{
            "type": "match.ctox_created",
            "payload": {
                "companyId": company_id,
                "jobId": job_id,
                "candidateId": candidate_id,
                "itemsCount": items.len(),
                "command_id": command_id,
                "queue_task_id": queue_task.map(|task| task.message_key.clone())
            },
            "at": now_iso
        }],
        "items": items,
        "createdAt": now_iso,
        "updatedAt": now_iso,
        "activeKey": 1,
        "scoreKey": score
    });
    let canonical = serde_json::json!({
        "module_id": "matching",
        "definition_id": "matching.matches.v1",
        "entity_type": "match",
        "record_key": match_id,
        "schema_version": "match.v1",
        "data": {
            "match": match_doc,
            "job_text_preview": truncate_chars(&job_text, 1200),
            "cv_text_preview": truncate_chars(&cv_text, 1200)
        },
        "source_refs": [{
            "type": "ctox-command",
            "command_id": command_id,
            "queue_task_id": queue_task.map(|task| task.message_key.clone())
        }],
        "links": {
            "company_id": company_id,
            "job_id": job_id,
            "candidate_id": candidate_id,
            "object_id": candidate_id
        },
        "display_cache": {
            "title": format!("{}%", score),
            "subtitle": candidate.get("name").and_then(Value::as_str).unwrap_or(""),
            "primary": job.get("title").and_then(Value::as_str).unwrap_or("")
        },
        "index_text": format!("{job_text}\n\n{cv_text}"),
        "sort_key": score,
        "status_key": "prematch",
        "score_key": score,
        "deleted": false,
        "created_at": now_ms,
        "updated_at": now_ms
    });
    upsert_business_record(conn, "matches", &match_id, now_ms, match_doc)?;
    upsert_business_record(conn, "business_records", &match_id, now_ms, canonical)?;
    write_import_artifact(
        root,
        command_id,
        "match_result.json",
        &serde_json::json!({
            "command_id": command_id,
            "match_id": match_id,
            "company_id": company_id,
            "job_id": job_id,
            "candidate_id": candidate_id,
            "score": score
        }),
    )?;

    Ok(ImportOutcome {
        collection: "matches".to_string(),
        definition_id: "matching.matches.v1".to_string(),
        record_ids: vec![match_id],
        records_count: 1,
    })
}

pub(super) fn handle_outbound_research(
    root: &Path,
    _conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    _queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    anyhow::ensure!(
        command.module == "outbound",
        "outbound research is only implemented for outbound"
    );
    match command.command_type.as_str() {
        "outbound.company.research" => outbound_company_research(root, command_id, command),
        "outbound.pipeline.contact_research" => {
            outbound_contact_research(root, command_id, command)
        }
        "outbound.pipeline.lead_qualification" => {
            outbound_lead_qualification(root, command_id, command)
        }
        other => Err(anyhow!("unsupported outbound research command: {other}")),
    }
}

#[derive(Debug, Clone)]
struct OutboundKnowledgeRefs {
    domain: String,
    companies_key: String,
    contacts_key: String,
    runs_key: String,
    runbook_id: String,
    campaign_id: String,
    campaign_name: String,
}

fn outbound_refs(command: &BusinessCommand) -> OutboundKnowledgeRefs {
    let campaign_id = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "campaign_id"]),
        str_path(&command.payload, &["campaign", "id"]),
        str_path(&command.client_context, &["campaign_id"]),
    ]);
    let campaign_name = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "campaign_name"]),
        str_path(&command.payload, &["campaign", "name"]),
        str_path(&command.payload, &["title"]),
    ]);
    let domain = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "domain"]),
        str_path(&command.payload, &["knowledge", "domain"]),
        str_path(&command.client_context, &["knowledge_domain"]),
        "outbound".to_string(),
    ]);
    let companies_key = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "companies_table_key"]),
        if !campaign_id.is_empty() {
            format!("campaign_{}_companies", slug_for_key(&campaign_id))
        } else {
            String::new()
        },
    ]);
    let contacts_key = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "contacts_table_key"]),
        str_path(&command.payload, &["writeback_contract", "table_key"]),
        if !campaign_id.is_empty() {
            format!("campaign_{}_contacts", slug_for_key(&campaign_id))
        } else {
            String::new()
        },
    ]);
    let runs_key = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "runs_table_key"]),
        if !campaign_id.is_empty() {
            format!("campaign_{}_research_runs", slug_for_key(&campaign_id))
        } else {
            String::new()
        },
    ]);
    let runbook_id = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "runbook_id"]),
        str_path(&command.payload, &["knowledge", "runbook_id"]),
        if !campaign_id.is_empty() {
            format!(
                "business-os.outbound.{}.runbook.v1",
                slug_for_key(&campaign_id)
            )
        } else {
            String::new()
        },
    ]);
    OutboundKnowledgeRefs {
        domain,
        companies_key,
        contacts_key,
        runs_key,
        runbook_id,
        campaign_id,
        campaign_name,
    }
}

fn outbound_company_research(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<ImportOutcome> {
    let refs = outbound_refs(command);
    anyhow::ensure!(
        !refs.domain.is_empty() && !refs.companies_key.is_empty(),
        "outbound company research requires Knowledge companies table"
    );
    ensure_outbound_knowledge_contract(
        root,
        command,
        &refs.domain,
        &refs.companies_key,
        &refs.campaign_id,
        &refs.campaign_name,
    )?;
    let company = command.payload.get("company").unwrap_or(&Value::Null);
    let company_id = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "stable_id_value"]),
        str_path(company, &["id"]),
        str_path(&command.client_context, &["company_id"]),
        command.record_id.clone().unwrap_or_default(),
    ]);
    let title_name = str_path(&command.payload, &["title"])
        .trim_start_matches("Unternehmensdaten recherchieren:")
        .trim()
        .to_string();
    let company_name = first_nonempty(&[
        str_path(company, &["name"]),
        str_path(&command.payload, &["research_request", "company"]),
        title_name,
    ]);
    anyhow::ensure!(
        !company_id.is_empty(),
        "outbound company research requires company_id"
    );
    anyhow::ensure!(
        !company_name.is_empty(),
        "outbound company research requires company name"
    );
    let country = first_nonempty(&[
        str_path(company, &["country"]),
        str_path(&command.payload, &["research_request", "country"]),
        "DE".to_string(),
    ]);
    let website = str_path(company, &["website"]);
    let existing_domain =
        first_nonempty(&[str_path(company, &["domain"]), domain_from_url(&website)]);
    let requested_fields = requested_research_fields(command);
    let research = run_person_research(
        root,
        &company_name,
        &country,
        ResearchMode::NewRecord,
        company_field_keys(&requested_fields),
    )?;
    let mut company_data = merge_json_objects(
        command
            .payload
            .get("company")
            .and_then(|value| value.get("company_data")),
        Some(&research),
    );
    apply_company_research_fields(&mut company_data, &research);
    let city = first_nonempty(&[
        string_from_map(&company_data, "city"),
        string_from_map(&company_data, "firma_ort"),
        str_path(company, &["city"]),
    ]);
    let domain = first_nonempty(&[
        string_from_map(&company_data, "domain"),
        string_from_map(&company_data, "firma_domain"),
        existing_domain,
    ]);
    let fit_score = if country_is_germany(&country) {
        80
    } else if !domain.is_empty() {
        65
    } else {
        50
    };
    let now = now_ms() as i64;
    let evidence = evidence_from_research(&research);
    let row = serde_json::json!({
        "company_id": company_id,
        "campaign_id": refs.campaign_id,
        "campaign_name": refs.campaign_name,
        "company_name": company_name,
        "website": website,
        "domain": domain,
        "city": city,
        "country": country,
        "qualification_status": "qualified",
        "research_status": "researched",
        "pipeline_status": first_nonempty(&[str_path(company, &["pipeline_status"]), "not_started".to_string()]),
        "fit_score": fit_score,
        "fit_status": "fit",
        "requested_fields_json": serde_json::to_string(command.payload.pointer("/research_request/fields").unwrap_or(&Value::Array(Vec::new()))).unwrap_or_else(|_| "[]".to_string()),
        "custom_instruction": str_path(&command.payload, &["research_request", "custom_instruction"]),
        "company_data_json": serde_json::to_string(&Value::Object(company_data))?,
        "evidence_json": serde_json::to_string(&evidence)?,
        "updated_at_ms": now,
    });
    append_knowledge_rows(root, &refs.domain, &refs.companies_key, &[row])?;
    append_run_status(
        root,
        &refs,
        command_id,
        command,
        "company_research",
        "completed",
        now,
    )?;
    Ok(ImportOutcome {
        collection: "knowledge_data_rows".to_string(),
        definition_id: "outbound.company.research.v1".to_string(),
        record_ids: vec![company_id],
        records_count: 1,
    })
}

/// GDPR provenance for persisted outbound contact rows. Outbound B2B contact
/// research relies on the legitimate-interest basis; the operator must maintain
/// a documented Legitimate Interest Assessment and honor erasure requests. We
/// stamp every persisted person row with a lawful basis, purpose, and retention
/// horizon so the data is accountable (Art. 5/6) and erasable by `subject_key`.
/// Field names mirror the CONSENT-1 ledger shape in `ats_gates.rs`.
const OUTBOUND_CONTACT_LEGAL_BASIS: &str = "legitimate_interest";
const OUTBOUND_CONTACT_PURPOSE: &str = "outbound_b2b_contact_research";
const OUTBOUND_CONTACT_BASIS_EVIDENCE: &str =
    "Operator-asserted B2B prospecting; operator must keep a documented Legitimate \
     Interest Assessment (LIA) and honor Art. 17 erasure requests.";
/// Default retention horizon: 180 days. Personal data persisted past this is
/// over-retained and should be purged by the retention sweep.
const OUTBOUND_CONTACT_RETENTION_MS: i64 = 180 * 24 * 60 * 60 * 1000;

/// Stable, opaque per-subject key so persisted person rows can be found and
/// erased on request: the lower-cased email when present, else name + company.
fn outbound_contact_subject_key(contact: &Value, company_name: &str) -> String {
    let email = contact
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let seed = if !email.is_empty() {
        email
    } else {
        format!(
            "{}|{}",
            contact
                .get("contact_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_lowercase(),
            company_name.trim().to_lowercase()
        )
    };
    format!("subj_{}", &hex_sha256(seed.as_bytes())[..16])
}

fn outbound_contact_research(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<ImportOutcome> {
    let refs = outbound_refs(command);
    anyhow::ensure!(
        !refs.domain.is_empty() && !refs.contacts_key.is_empty(),
        "outbound contact research requires Knowledge contacts table"
    );
    ensure_outbound_knowledge_contract(
        root,
        command,
        &refs.domain,
        &refs.companies_key,
        &refs.campaign_id,
        &refs.campaign_name,
    )?;
    let pipeline_id = first_nonempty(&[
        str_path(&command.payload, &["pipeline_id"]),
        str_path(&command.client_context, &["pipeline_id"]),
        str_path(&command.payload, &["writeback_contract", "stable_id_value"]),
        command.record_id.clone().unwrap_or_default(),
    ]);
    let company_id = first_nonempty(&[
        str_path(&command.payload, &["company_id"]),
        str_path(&command.client_context, &["company_id"]),
        str_path(&command.payload, &["writeback_contract", "company_id"]),
    ]);
    let title_name = str_path(&command.payload, &["title"])
        .trim_start_matches("Ansprechpartner recherchieren:")
        .trim()
        .to_string();
    let company_name = first_nonempty(&[str_path(&command.payload, &["company_name"]), title_name]);
    anyhow::ensure!(
        !pipeline_id.is_empty(),
        "outbound contact research requires pipeline_id"
    );
    anyhow::ensure!(
        !company_name.is_empty(),
        "outbound contact research requires company name"
    );
    let contact_fields = requested_contact_fields(command);
    let research = run_person_research(
        root,
        &company_name,
        "DE",
        ResearchMode::UpdatePerson,
        contact_field_keys(&contact_fields),
    )?;
    let contacts = contacts_from_research(&pipeline_id, &company_id, &company_name, &research);
    anyhow::ensure!(
        !contacts.is_empty(),
        "ctox contact research found no public contacts for {company_name}"
    );
    let now = now_ms() as i64;
    let rows = contacts
        .iter()
        .map(|contact| {
            serde_json::json!({
                "pipeline_id": pipeline_id,
                "company_id": company_id,
                "campaign_id": refs.campaign_id,
                "campaign_name": refs.campaign_name,
                "company_name": company_name,
                "contact_id": contact.get("contact_id").and_then(Value::as_str).unwrap_or_default(),
                "contact_name": contact.get("contact_name").and_then(Value::as_str).unwrap_or_default(),
                "role": contact.get("role").and_then(Value::as_str).unwrap_or_default(),
                "email": contact.get("email").and_then(Value::as_str).unwrap_or_default(),
                "linkedin_url": contact.get("linkedin_url").and_then(Value::as_str).unwrap_or_default(),
                "contact_research_status": "qualified",
                "lead_status": "open",
                "stage": "contact_qualified",
                "contact_fields_json": serde_json::to_string(command.payload.get("contact_fields").unwrap_or(&Value::Array(Vec::new()))).unwrap_or_else(|_| "[]".to_string()),
                "custom_instruction": str_path(&command.payload, &["custom_instruction"]),
                "evidence_json": serde_json::to_string(contact.get("evidence").unwrap_or(&Value::Array(Vec::new()))).unwrap_or_else(|_| "[]".to_string()),
                "updated_at_ms": now,
                // GDPR provenance (WS2-01 / H2): every persisted person row
                // carries a lawful basis, purpose, and retention horizon and is
                // erasable by subject_key.
                "legal_basis": OUTBOUND_CONTACT_LEGAL_BASIS,
                "basis_evidence": OUTBOUND_CONTACT_BASIS_EVIDENCE,
                "purpose": OUTBOUND_CONTACT_PURPOSE,
                "granted_at_ms": now,
                "expires_at_ms": now + OUTBOUND_CONTACT_RETENTION_MS,
                "subject_key": outbound_contact_subject_key(contact, &company_name),
            })
        })
        .collect::<Vec<_>>();
    append_knowledge_rows(root, &refs.domain, &refs.contacts_key, &rows)?;
    append_run_status(
        root,
        &refs,
        command_id,
        command,
        "contact_research",
        "completed",
        now,
    )?;
    let record_ids = rows
        .iter()
        .filter_map(|row| str_or_none(row, &["contact_id"]))
        .collect::<Vec<_>>();
    Ok(ImportOutcome {
        collection: "knowledge_data_rows".to_string(),
        definition_id: "outbound.pipeline.contact_research.v1".to_string(),
        records_count: rows.len(),
        record_ids,
    })
}

fn outbound_lead_qualification(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<ImportOutcome> {
    let refs = outbound_refs(command);
    anyhow::ensure!(
        !refs.domain.is_empty() && !refs.contacts_key.is_empty(),
        "outbound lead qualification requires Knowledge contacts table"
    );
    ensure_outbound_knowledge_contract(
        root,
        command,
        &refs.domain,
        &refs.companies_key,
        &refs.campaign_id,
        &refs.campaign_name,
    )?;
    let pipeline_id = first_nonempty(&[
        str_path(&command.payload, &["pipeline_id"]),
        str_path(&command.client_context, &["pipeline_id"]),
        str_path(&command.payload, &["writeback_contract", "stable_id_value"]),
        command.record_id.clone().unwrap_or_default(),
    ]);
    anyhow::ensure!(
        !pipeline_id.is_empty(),
        "outbound lead qualification requires pipeline_id"
    );
    let contact_rows = load_contact_rows_for_pipeline(root, &refs, &pipeline_id)?;
    anyhow::ensure!(
        !contact_rows.is_empty(),
        "lead qualification requires contact rows in Knowledge for pipeline_id={pipeline_id}"
    );
    let now = now_ms() as i64;
    let rows = contact_rows
        .iter()
        .filter(|row| contact_row_is_qualifiable(row))
        .map(|row| {
            let mut next = row.as_object().cloned().unwrap_or_default();
            next.insert("lead_status".to_string(), Value::String("qualified".to_string()));
            next.insert("outreach_status".to_string(), Value::String("qualified".to_string()));
            next.insert("stage".to_string(), Value::String("lead_qualified".to_string()));
            next.insert(
                "lead_score".to_string(),
                Value::Number(serde_json::Number::from(80)),
            );
            next.insert(
                "lead_reason".to_string(),
                Value::String(
                    "Öffentlich belegbarer Ansprechpartner mit Rolle/Profil für den Campaign Scope vorhanden."
                        .to_string(),
                ),
            );
            next.insert(
                "updated_at_ms".to_string(),
                Value::Number(serde_json::Number::from(now)),
            );
            Value::Object(next)
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !rows.is_empty(),
        "lead qualification found contacts, but none were qualifiable"
    );
    append_knowledge_rows(root, &refs.domain, &refs.contacts_key, &rows)?;
    append_run_status(
        root,
        &refs,
        command_id,
        command,
        "lead_qualification",
        "completed",
        now,
    )?;
    let record_ids = rows
        .iter()
        .filter_map(|row| str_or_none(row, &["contact_id"]))
        .collect::<Vec<_>>();
    Ok(ImportOutcome {
        collection: "knowledge_data_rows".to_string(),
        definition_id: "outbound.pipeline.lead_qualification.v1".to_string(),
        records_count: rows.len(),
        record_ids,
    })
}

fn import_matching_requirement(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let url = first_nonempty(&[
        str_path(&command.payload, &["input", "url"]),
        str_path(&command.payload, &["source", "url"]),
    ]);
    let mut html = str_path(&command.payload, &["input", "html"]);
    if html.trim().is_empty() {
        html = first_nonempty(&[
            str_path(&command.payload, &["input", "text"]),
            str_path(&command.payload, &["source", "text"]),
        ]);
    }
    anyhow::ensure!(
        !url.trim().is_empty() || !html.trim().is_empty(),
        "matching requirement parse requires input.url, input.html, or input.text"
    );

    let mut http_status = 0;
    if html.trim().is_empty() {
        let response = fetch_url_guarded(&url)?;
        http_status = response.status();
        html = response
            .into_string()
            .with_context(|| format!("failed to read response body from {url}"))?;
    }

    let html_sha256 = hex_sha256(html.as_bytes());
    let parsed = parse_job_html(&url, &html);
    let now_iso = now_iso();
    let now_ms = now_ms() as i64;
    let source_id = format!(
        "source_{}",
        short_hash(&format!("{}:{}", parsed.company_name, parsed.location))
    );
    let requirement_id = command
        .record_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(command_id)
        .to_string();
    let requirement_source_id = format!("{requirement_id}::source");
    let location_id = format!("loc_{}", short_hash(&parsed.location));
    let source_doc = serde_json::json!({
        "id": source_id,
        "kind": "source",
        "name": parsed.company_name,
        "legalName": parsed.company_name,
        "logoUrl": parsed.company_logo,
        "sourceUrl": url,
        "active": true,
        "locations": [{
            "id": location_id,
            "city": if parsed.city.trim().is_empty() { parsed.location.clone() } else { parsed.city.clone() },
            "country": parsed.country_name,
            "postalCode": parsed.zip,
            "address": parsed.street,
            "countryKey": parsed.country,
            "postalCodeKey": parsed.zip
        }],
        "createdAt": now_iso,
        "updatedAt": now_iso
    });
    let requirement_doc = serde_json::json!({
        "id": requirement_id,
        "kind": "requirement",
        "sourceId": source_id,
        "sourceName": parsed.company_name,
        "title": parsed.title,
        "internalReferenceId": parsed.external_ref,
        "status": "open",
        "location": parsed.location,
        "locationIds": [location_id],
        "fachlevelClass": parsed.fachlevel_class,
        "workModel": parsed.work_model,
        "type": parsed.work_model.clone().unwrap_or_else(|| "Vollzeit".to_string()),
        "remote": parsed.remote,
        "aboutSource": parsed.about_company,
        "aboutRole": parsed.about_role,
        "responsibilities": parsed.responsibilities,
        "objectRequirements": parsed.candidate_requirements,
        "requirements": parsed.requirements,
        "benefits": parsed.benefits,
        "closingNotes": parsed.closing_notes,
        "language": "de",
        "sourceUrl": url,
        "rawText": parsed.raw_text,
        "active": true,
        "createdAt": now_iso,
        "updatedAt": now_iso
    });
    let requirement_source_doc = serde_json::json!({
        "id": requirement_source_id,
        "kind": "requirementSource",
        "requirementId": requirement_id,
        "sourceId": source_id,
        "source": parsed.source_name,
        "sourceUrl": url,
        "externalRef": parsed.external_ref,
        "rawText": parsed.raw_text,
        "rawHtmlSha256": html_sha256,
        "parsed": {
            "aboutSource": parsed.about_company,
            "aboutRole": parsed.about_role,
            "responsibilities": parsed.responsibilities,
            "objectRequirements": parsed.candidate_requirements,
            "benefits": parsed.benefits,
            "closingNotes": parsed.closing_notes,
            "workModel": parsed.work_model,
            "remote": parsed.remote,
            "fachlevelClass": parsed.fachlevel_class
        },
        "parsingMeta": {
            "schemaVersion": "matching.requirement.v1",
            "confidence": parsed.confidence,
            "httpStatus": http_status,
            "command_id": command_id,
            "queue_task_id": queue_task.map(|task| task.message_key.clone())
        },
        "createdAt": now_iso,
        "updatedAt": now_iso
    });

    upsert_business_record(
        conn,
        "matching_requirements",
        &source_id,
        now_ms,
        source_doc,
    )?;
    upsert_business_record(
        conn,
        "matching_requirements",
        &requirement_id,
        now_ms,
        requirement_doc,
    )?;
    upsert_business_record(
        conn,
        "matching_requirements",
        &requirement_source_id,
        now_ms,
        requirement_source_doc,
    )?;
    write_import_artifact(
        root,
        command_id,
        "matching_requirement.json",
        &serde_json::json!({
            "command_id": command_id,
            "source_id": source_id,
            "requirement_id": requirement_id,
            "source_url": url,
            "title": parsed.title,
            "records_count": 3
        }),
    )?;

    Ok(ImportOutcome {
        collection: "matching_requirements".to_string(),
        definition_id: "matching.requirements.v1".to_string(),
        record_ids: vec![source_id, requirement_id, requirement_source_id],
        records_count: 3,
    })
}

fn import_matching_objects(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let files = command
        .payload
        .get("input")
        .and_then(|input| input.get("files"))
        .and_then(Value::as_array)
        .or_else(|| {
            command
                .payload
                .get("source")
                .and_then(|source| source.get("files"))
                .and_then(Value::as_array)
        })
        .cloned()
        .unwrap_or_default();
    anyhow::ensure!(
        !files.is_empty(),
        "matching object parse requires input.files"
    );

    let base_record = command
        .record_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(command_id);
    let cleanup_prefix = format!("{base_record}__%");
    conn.execute(
        "DELETE FROM business_records
         WHERE collection = 'matching_objects'
           AND record_id LIKE ?1",
        rusqlite::params![cleanup_prefix],
    )?;

    let mut record_ids = Vec::new();
    let mut summaries = Vec::new();
    for file in files {
        let name = file
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("document.pdf");
        let mime = file
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("application/pdf");
        let bytes =
            decode_file_payload(&file).with_context(|| format!("failed to decode {name}"))?;
        let mut raw_text = parse_pdf_text(&bytes).unwrap_or_else(|err| {
            eprintln!("[business-os-import] PDF parse failed for {name}: {err:#}");
            String::new()
        });
        if raw_text.trim().is_empty() {
            // The native parser extracted no text — almost always a scanned /
            // image-only PDF. Try a vision-model OCR pass; it degrades to None
            // when vision is unavailable, in which case we import with empty text
            // (the record still carries raw_text_length: 0 as a signal).
            match ocr_pdf_via_vision(root, &bytes) {
                Some(text) => {
                    eprintln!(
                        "[business-os-import] recovered {} chars from {name} via vision OCR",
                        text.len()
                    );
                    raw_text = text;
                }
                None => eprintln!(
                    "[business-os-import] no extractable text from {name} (scanned/image PDF; vision OCR unavailable)"
                ),
            }
        }
        let candidate = parse_candidate_text(name, &raw_text);
        let now_iso = now_iso();
        let now_ms = now_ms() as i64;
        let object_id = format!("{base_record}__{}", slug(&candidate.name));
        let document_sha256 = hex_sha256(&bytes);
        let methoden_kompetenz = candidate
            .skills
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let executive_info = serde_json::json!({
            "fachlicheQualifikation": candidate.current_role.clone(),
            "methodenKompetenz": methoden_kompetenz,
            "leadershipFaehigkeit": candidate.leadership.clone(),
            "gehaltswunschUndOrt": candidate.region.clone()
        });
        let object_doc = serde_json::json!({
            "id": object_id,
            "kind": "object",
            "name": candidate.name,
            "firstName": candidate.first_name,
            "lastName": candidate.last_name,
            "email": candidate.email,
            "phone": candidate.phone,
            "currentRole": candidate.current_role,
            "desiredPosition": candidate.desired_position,
            "taxonomy": "importiertes Objekt",
            "region": candidate.region,
            "driverLicense": candidate.driver_license,
            "highestDegree": candidate.highest_degree,
            "languages": candidate.languages,
            "skills": candidate.skills,
            "executiveInfo": executive_info,
            "active": true,
            "activeKey": 1,
            "documents": [{
                "id": format!("{object_id}::document"),
                "kind": "document",
                "filename": name,
                "mimeType": mime,
                "size": bytes.len(),
                "uploadedAt": now_iso,
                "parsed": true,
                "meta": {
                    "rawText": raw_text,
                    "sha256": document_sha256,
                    "command_id": command_id,
                    "queue_task_id": queue_task.map(|task| task.message_key.clone())
                }
            }],
            "additional": [{
                "key": "system.import",
                "label": "Import",
                "type": "json",
                "value": {
                    "state": "done",
                    "source": "drawer",
                    "command_id": command_id
                },
                "source": "ctox-native-importer",
                "confidence": candidate.confidence,
                "required": false
            }],
            "rawText": raw_text,
            "createdAt": now_iso,
            "updatedAt": now_iso
        });

        upsert_business_record(conn, "matching_objects", &object_id, now_ms, object_doc)?;
        record_ids.push(object_id.clone());
        summaries.push(serde_json::json!({
            "record_id": object_id,
            "source_name": name,
            "raw_text_length": raw_text.len()
        }));
    }

    write_import_artifact(
        root,
        command_id,
        "matching_objects.json",
        &serde_json::json!({
            "command_id": command_id,
            "records_count": record_ids.len(),
            "records": summaries
        }),
    )?;

    let records_count = record_ids.len();
    Ok(ImportOutcome {
        collection: "matching_objects".to_string(),
        definition_id: "matching.objects.v1".to_string(),
        record_ids,
        records_count,
    })
}

fn compute_matching_result(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let requirement_id = first_nonempty(&[
        str_path(&command.client_context, &["requirementId"]),
        str_path(
            &command.payload,
            &["options", "businessContext", "requirementId"],
        ),
        str_path(&command.payload, &["request", "requirement_id"]),
    ]);
    let object_id = first_nonempty(&[
        str_path(&command.client_context, &["objectId"]),
        str_path(
            &command.payload,
            &["options", "businessContext", "objectId"],
        ),
        str_path(&command.payload, &["request", "object_id"]),
    ]);
    let source_id = first_nonempty(&[
        str_path(&command.client_context, &["sourceId"]),
        str_path(
            &command.payload,
            &["options", "businessContext", "sourceId"],
        ),
    ]);
    anyhow::ensure!(
        !requirement_id.is_empty(),
        "matching.match requires requirementId"
    );
    anyhow::ensure!(!object_id.is_empty(), "matching.match requires objectId");

    let requirement = load_collection_payload(conn, "matching_requirements", &requirement_id)
        .with_context(|| format!("requirement not found: {requirement_id}"))?;
    let object = load_collection_payload(conn, "matching_objects", &object_id)
        .with_context(|| format!("object not found: {object_id}"))?;
    let requirement_text = matching_requirement_text(&requirement);
    let object_text = matching_object_text(&object);
    anyhow::ensure!(
        !requirement_text.trim().is_empty(),
        "requirement text is empty"
    );
    anyhow::ensure!(!object_text.trim().is_empty(), "object text is empty");

    let now_iso = now_iso();
    let now_ms = now_ms() as i64;
    let match_id = command
        .record_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{source_id}|{requirement_id}|{object_id}"));
    let items = build_match_items(
        &source_id,
        &requirement_id,
        &object_id,
        &requirement_text,
        &object_text,
        &now_iso,
    );
    let score = total_match_score(&items);
    let match_doc = serde_json::json!({
        "id": match_id,
        "kind": "match",
        "sourceId": source_id,
        "requirementId": requirement_id,
        "objectId": object_id,
        "requirementTitle": requirement.get("title").and_then(Value::as_str).unwrap_or(""),
        "objectName": object.get("name").and_then(Value::as_str).unwrap_or(""),
        "active": true,
        "removed": false,
        "progress": 10,
        "status": "prematch",
        "score": score,
        "scoreKey": score,
        "notes": "",
        "items": items,
        "evidence": items,
        "createdAt": now_iso,
        "updatedAt": now_iso,
        "command_id": command_id,
        "queue_task_id": queue_task.map(|task| task.message_key.clone())
    });

    upsert_business_record(conn, "matching_results", &match_id, now_ms, match_doc)?;
    write_import_artifact(
        root,
        command_id,
        "matching_result.json",
        &serde_json::json!({
            "command_id": command_id,
            "match_id": match_id,
            "requirement_id": requirement_id,
            "object_id": object_id,
            "score": score
        }),
    )?;

    Ok(ImportOutcome {
        collection: "matching_results".to_string(),
        definition_id: "matching.matches.v1".to_string(),
        record_ids: vec![match_id],
        records_count: 1,
    })
}

fn import_requirement_url(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let url = str_path(&command.payload, &["source", "url"]);
    anyhow::ensure!(!url.trim().is_empty(), "URL import requires source.url");

    let response = fetch_url_guarded(&url)?;
    let http_status = response.status();
    let html = response
        .into_string()
        .with_context(|| format!("failed to read response body from {url}"))?;
    let html_sha256 = hex_sha256(html.as_bytes());
    let parsed = parse_job_html(&url, &html);
    let now_iso = now_iso();
    let now_ms = now_ms() as i64;
    let record_id = command
        .record_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(command_id);

    let company_id = slug(&parsed.company_name);
    let job_id = record_id.to_string();
    let posting_id = format!("{job_id}::posting");
    let definition_id = "matching.requirements.v1";

    let company = serde_json::json!({
        "id": company_id,
        "name": parsed.company_name,
        "legalName": parsed.company_name,
        "website": parsed.company_website,
        "websiteUrl": parsed.company_website,
        "logoUrl": parsed.company_logo,
        "tags": [],
        "taxonomyCode": null,
        "active": true,
        "activeKey": 1,
        "locations": [{
            "id": format!("loc_{}", short_hash(&parsed.location)),
            "city": parsed.city,
            "country": parsed.country_name,
            "postalCode": parsed.zip,
            "address": parsed.street,
            "countryKey": parsed.country,
            "postalCodeKey": parsed.zip
        }],
        "createdAt": now_iso,
        "updatedAt": now_iso
    });

    let job = serde_json::json!({
        "id": job_id,
        "companyId": company_id,
        "title": parsed.title,
        "internalReferenceId": parsed.external_ref,
        "status": "open",
        "kldbCode": null,
        "fachlevelClass": parsed.fachlevel_class,
        "workModel": parsed.work_model,
        "remote": parsed.remote,
        "remotePercent": null,
        "salaryMin": null,
        "salaryMax": null,
        "salaryPeriod": null,
        "salaryCurrency": null,
        "relocation": null,
        "visaSponsorship": null,
        "locationIds": [],
        "aboutCompany": parsed.about_company,
        "aboutRole": parsed.about_role,
        "candidateRequirements": parsed.candidate_requirements,
        "benefits": parsed.benefits,
        "closingNotes": parsed.closing_notes,
        "agencyTypeValue": 0,
        "incentivesValue": parsed.incentives_value,
        "urgencyValue": 1,
        "relaxValue": 1,
        "vacancyAgeClass": 0,
        "language": "de",
        "ownerUserId": null,
        "createdAt": now_iso,
        "updatedAt": now_iso,
        "kldbKey": "",
        "remoteKey": if parsed.remote { 1 } else { 0 },
        "remotePercentKey": -1,
        "salaryMinKey": -1,
        "salaryMaxKey": -1
    });

    let posting = serde_json::json!({
        "id": posting_id,
        "jobId": job_id,
        "source": "Other",
        "sourceUrl": url,
        "externalRef": parsed.external_ref,
        "publishAt": parsed.date_posted.as_ref().map(|date| format!("{date}T00:00:00Z")),
        "expireAt": null,
        "language": "de",
        "rawText": parsed.raw_text,
        "parsed": {
            "aboutCompany": parsed.about_company,
            "aboutRole": parsed.about_role,
            "candidateRequirements": parsed.candidate_requirements,
            "benefits": parsed.benefits,
            "closingNotes": parsed.closing_notes,
            "agencyTypeValue": 0,
            "agencyTypeEvidence": [parsed.source_name],
            "incentivesValue": parsed.incentives_value,
            "incentivesEvidence": parsed.benefits,
            "urgencyValue": 1,
            "urgencyEvidence": [],
            "relaxValue": 1,
            "relaxEvidence": [],
            "vacancyAgeClass": 0,
            "vacancyAgeEvidence": parsed.date_posted.iter().cloned().collect::<Vec<_>>(),
            "fachlevelClass": parsed.fachlevel_class,
            "fachlevelEvidence": [],
            "kldbCode": "",
            "kldbEvidence": [],
            "workModel": parsed.work_model,
            "remote": parsed.remote,
            "remotePercent": null,
            "salaryMin": null,
            "salaryMax": null,
            "salaryPeriod": null,
            "salaryCurrency": null,
            "relocation": null,
            "visaSponsorship": null,
            "companyCar": parsed.raw_text.to_lowercase().contains("firmenfahrzeug"),
            "bonus": parsed.raw_text.to_lowercase().contains("sonderzahlung")
        },
        "ba": null,
        "parsingMeta": {
            "genModelId": "ctox-native-importer",
            "embedModelId": null,
            "schemaVersion": "matching.requirement.v1",
            "confidence": parsed.confidence
        },
        "createdAt": now_iso,
        "updatedAt": now_iso,
        "publishAtKey": parsed.date_posted.clone().unwrap_or_default(),
        "externalRefKey": parsed.external_ref.clone().unwrap_or_default(),
        "parsedKldbKey": "",
        "parsedAgencyTypeKey": 0
    });

    let canonical = serde_json::json!({
        "module_id": "matching",
        "definition_id": definition_id,
        "entity_type": "requirement",
        "record_key": job_id,
        "schema_version": "requirement.v1",
        "data": {
            "company": {
                "id": company_id,
                "name": parsed.company_name,
                "legalName": parsed.company_name,
                "website": parsed.company_website,
                "location": parsed.location,
                "city": parsed.city,
                "country": parsed.country,
                "zip": parsed.zip
            },
            "job": {
                "id": job_id,
                "externalRef": parsed.external_ref,
                "title": parsed.title,
                "name": parsed.title,
                "companyId": company_id,
                "companyName": parsed.company_name,
                "status": "open",
                "workModel": parsed.work_model,
                "employmentType": parsed.work_model,
                "contractType": parsed.contract_type,
                "remote": parsed.remote,
                "remotePercent": null,
                "location": parsed.location,
                "city": parsed.city,
                "country": parsed.country,
                "zip": parsed.zip,
                "aboutCompany": parsed.about_company,
                "aboutRole": parsed.about_role,
                "responsibilities": parsed.responsibilities,
                "candidateRequirements": parsed.candidate_requirements,
                "requirements": parsed.requirements,
                "benefits": parsed.benefits,
                "closingNotes": parsed.closing_notes,
                "language": "de",
                "postedAt": parsed.date_posted,
                "updatedAt": parsed.date_posted,
                "fachlevelClass": parsed.fachlevel_class
            },
            "posting": {
                "source": "Other",
                "sourceName": parsed.source_name,
                "sourceUrl": url,
                "externalRef": parsed.external_ref,
                "rawText": parsed.raw_text,
                "rawHtmlSha256": html_sha256,
                "parsed": {
                    "aboutCompany": parsed.about_company,
                    "aboutRole": parsed.about_role,
                    "responsibilities": parsed.responsibilities,
                    "candidateRequirements": parsed.candidate_requirements,
                    "requirementBullets": parsed.requirements,
                    "benefits": parsed.benefits,
                    "closingNotes": parsed.closing_notes,
                    "agencyTypeValue": 0,
                    "incentivesValue": parsed.incentives_value,
                    "urgencyValue": 1,
                    "relaxValue": 1,
                    "vacancyAgeClass": 0,
                    "fachlevelClass": parsed.fachlevel_class,
                    "workModel": parsed.work_model,
                    "remote": parsed.remote
                },
                "parsingMeta": {
                    "schemaVersion": "matching.requirement.v1",
                    "confidence": parsed.confidence,
                    "httpStatus": http_status
                }
            }
        },
        "source_refs": [{
            "type": "url",
            "url": url,
            "http_status": http_status,
            "html_sha256": html_sha256,
            "command_id": command_id,
            "queue_task_id": queue_task.map(|task| task.message_key.clone())
        }],
        "links": {},
        "display_cache": {
            "title": parsed.title,
            "subtitle": parsed.location,
            "primary": parsed.company_name
        },
        "index_text": parsed.raw_text,
        "sort_key": parsed.date_posted.unwrap_or_else(|| parsed.title.clone()),
        "status_key": "open",
        "score_key": 0,
        "deleted": false,
        "created_at": now_ms,
        "updated_at": now_ms
    });

    upsert_business_record(conn, "business_records", &job_id, now_ms, canonical)?;
    upsert_business_record(conn, "companies", &company_id, now_ms, company)?;
    upsert_business_record(conn, "jobs", &job_id, now_ms, job)?;
    upsert_business_record(conn, "postings", &posting_id, now_ms, posting)?;
    write_import_artifact(
        root,
        command_id,
        "parsed_record.json",
        &serde_json::json!({
            "command_id": command_id,
            "record_id": job_id,
            "definition_id": definition_id,
            "source_url": url,
            "http_status": http_status,
            "html_sha256": html_sha256,
            "title": parsed.title,
            "location": parsed.location
        }),
    )?;

    Ok(ImportOutcome {
        collection: "business_records".to_string(),
        definition_id: definition_id.to_string(),
        record_ids: vec![job_id],
        records_count: 1,
    })
}

fn import_candidate_documents(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let files = command
        .payload
        .get("source")
        .and_then(|source| source.get("files"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    anyhow::ensure!(!files.is_empty(), "document import requires source.files");

    let base_record = command
        .record_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(command_id);
    let cleanup_prefix = format!("{base_record}__%");
    conn.execute(
        "DELETE FROM business_records
         WHERE collection IN ('business_records', 'candidates')
           AND record_id LIKE ?1",
        rusqlite::params![cleanup_prefix],
    )?;

    let mut record_ids = Vec::new();
    let mut summaries = Vec::new();
    for file in files {
        let name = file
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("resume.pdf");
        let mime = file
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("application/pdf");
        let base64 = file.get("base64").and_then(Value::as_str).unwrap_or("");
        let compact_base64 = base64
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        let bytes = BASE64_STANDARD
            .decode(compact_base64)
            .with_context(|| format!("failed to decode {name}"))?;
        let mut raw_text = parse_pdf_text(&bytes).unwrap_or_else(|err| {
            eprintln!("[business-os-import] PDF parse failed for {name}: {err:#}");
            String::new()
        });
        if raw_text.trim().is_empty() {
            // The native parser extracted no text — almost always a scanned /
            // image-only PDF. Try a vision-model OCR pass; it degrades to None
            // when vision is unavailable, in which case we import with empty text
            // (the record still carries raw_text_length: 0 as a signal).
            match ocr_pdf_via_vision(root, &bytes) {
                Some(text) => {
                    eprintln!(
                        "[business-os-import] recovered {} chars from {name} via vision OCR",
                        text.len()
                    );
                    raw_text = text;
                }
                None => eprintln!(
                    "[business-os-import] no extractable text from {name} (scanned/image PDF; vision OCR unavailable)"
                ),
            }
        }
        let candidate = parse_candidate_text(name, &raw_text);
        let now_iso = now_iso();
        let now_ms = now_ms() as i64;
        let candidate_id = format!("{base_record}__{}", slug(&candidate.name));
        let definition_id = "matching.objects.v1";
        let document_sha256 = hex_sha256(&bytes);
        let methoden_kompetenz = candidate
            .skills
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let executive_info = serde_json::json!({
            "fachlicheQualifikation": candidate.current_role.clone(),
            "methodenKompetenz": methoden_kompetenz,
            "leadershipFaehigkeit": candidate.leadership.clone(),
            "gehaltswunschUndOrt": candidate.region.clone()
        });
        let documents = serde_json::json!([{
            "id": format!("{candidate_id}::cv"),
            "kind": "CV",
            "filename": name,
            "mimeType": mime,
            "size": bytes.len(),
            "uploadedAt": now_iso,
            "parsed": true,
            "meta": {
                "rawText": raw_text,
                "sha256": document_sha256,
                "command_id": command_id,
                "queue_task_id": queue_task.map(|task| task.message_key.clone())
            }
        }]);
        let additional = serde_json::json!([{
            "key": "system.import",
            "label": "Import",
            "type": "json",
            "value": {
                "state": "done",
                "source": "drawer",
                "command_id": command_id
            },
            "source": "ctox-native-importer",
            "confidence": candidate.confidence,
            "required": false
        }]);

        let projection = serde_json::json!({
            "id": candidate_id,
            "name": candidate.name.clone(),
            "firstName": candidate.first_name.clone(),
            "lastName": candidate.last_name.clone(),
            "birthDate": null,
            "nationality": null,
            "gender": null,
            "email": candidate.email.clone(),
            "phone": candidate.phone.clone(),
            "address": {
                "street": null,
                "postalCode": null,
                "city": null,
                "country": "DE"
            },
            "preferredChannel": null,
            "currentRole": candidate.current_role.clone(),
            "desiredPosition": candidate.desired_position.clone(),
            "taxonomy": "importierter CV",
            "industry": null,
            "availabilityFrom": null,
            "region": candidate.region.clone(),
            "travelOk": null,
            "workModelWish": null,
            "driverLicense": candidate.driver_license.clone(),
            "hasCar": null,
            "highestDegree": candidate.highest_degree.clone(),
            "degree": null,
            "languages": candidate.languages.clone(),
            "skills": candidate.skills.clone(),
            "softSkills": [],
            "executiveInfo": executive_info,
            "idealJob": null,
            "idealJobUpdatedAt": null,
            "consent": {
                "processing": true,
                "shareWithClients": false,
                "source": "import"
            },
            "profilePhotoBase64": "",
            "documents": documents,
            "hasRelation": false,
            "tags": [],
            "candidateStatus": "neu",
            "active": true,
            "activeKey": 1,
            "additional": additional,
            "proposals": [],
            "versions": [],
            "createdAt": now_iso,
            "updatedAt": now_iso,
            "taxonomyKey": "importierter cv",
            "hasRelationKey": 0
        });

        let canonical = serde_json::json!({
            "module_id": "matching",
            "definition_id": definition_id,
            "entity_type": "object",
            "record_key": candidate_id,
            "schema_version": "object.v1",
            "data": {
                "object": projection,
                "candidate": projection,
                "source_document": {
                    "filename": name,
                    "mimeType": mime,
                    "size": bytes.len(),
                    "sha256": hex_sha256(&bytes)
                }
            },
            "source_refs": [{
                "type": "document",
                "filename": name,
                "command_id": command_id,
                "queue_task_id": queue_task.map(|task| task.message_key.clone())
            }],
            "links": {},
            "display_cache": {
                "title": candidate.name,
                "subtitle": candidate.current_role,
                "primary": candidate.region
            },
            "index_text": raw_text,
            "sort_key": candidate.name,
            "status_key": "neu",
            "score_key": 0,
            "deleted": false,
            "created_at": now_ms,
            "updated_at": now_ms
        });

        upsert_business_record(conn, "business_records", &candidate_id, now_ms, canonical)?;
        upsert_business_record(conn, "candidates", &candidate_id, now_ms, projection)?;
        record_ids.push(candidate_id.clone());
        summaries.push(serde_json::json!({
            "record_id": candidate_id,
            "source_name": name,
            "name": candidate.name,
            "raw_text_length": raw_text.len()
        }));
    }

    write_import_artifact(
        root,
        command_id,
        "parsed_records.json",
        &serde_json::json!({
            "command_id": command_id,
            "record_id": command.record_id,
            "definition_id": "matching.objects.v1",
            "source_type": "document",
            "scope": "each_file",
            "records_count": record_ids.len(),
            "records": summaries
        }),
    )?;

    Ok(ImportOutcome {
        collection: "business_records".to_string(),
        definition_id: "matching.objects.v1".to_string(),
        records_count: record_ids.len(),
        record_ids,
    })
}

fn import_outbound_companies(
    root: &Path,
    _conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
) -> anyhow::Result<ImportOutcome> {
    let url = str_path(&command.payload, &["source", "url"]);
    anyhow::ensure!(
        !url.trim().is_empty(),
        "outbound import requires source.url"
    );
    let source_id = first_nonempty(&[
        str_path(&command.payload, &["source_id"]),
        str_path(&command.client_context, &["source_id"]),
        command
            .record_id
            .clone()
            .unwrap_or_else(|| command_id.to_string()),
    ]);
    let campaign_id = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "campaign_id"]),
        str_path(&command.client_context, &["campaign_id"]),
    ]);
    let campaign_name = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "campaign_name"]),
        str_path(&command.payload, &["title"]),
    ]);
    let domain = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "domain"]),
        str_path(&command.payload, &["knowledge", "domain"]),
        str_path(&command.client_context, &["knowledge_domain"]),
    ]);
    let table_key = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "table_key"]),
        str_path(&command.payload, &["knowledge", "companies_table_key"]),
        str_path(&command.client_context, &["knowledge_table_key"]),
    ]);
    anyhow::ensure!(
        !domain.is_empty() && !table_key.is_empty(),
        "outbound import requires a knowledge data writeback_contract"
    );
    ensure_outbound_knowledge_contract(
        root,
        command,
        &domain,
        &table_key,
        &campaign_id,
        &campaign_name,
    )?;

    let germany_only = outbound_import_germany_only(command);
    let mut source = OUTBOUND_COMPANY_BROWSER_SCRIPT
        .replace("__IMPORT_URL__", &serde_json::to_string(&url)?)
        .replace(
            "__GERMANY_ONLY__",
            if germany_only { "true" } else { "false" },
        );
    source = source.replace("__MAX_ROWS__", "100");
    let automation = crate::web_stack::run_browser_automation(
        root,
        &crate::web_stack::BrowserAutomationRequest {
            dir: None,
            timeout_ms: Some(180_000),
            source,
        },
    )?;
    anyhow::ensure!(
        automation.get("ok").and_then(Value::as_bool) == Some(true),
        "ctox browser automation failed: {}",
        automation
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown browser automation error")
    );
    let rows_json = str_path(&automation, &["result", "rows_json"]);
    let parsed_rows: Vec<Value> = serde_json::from_str(&rows_json)
        .with_context(|| "browser automation returned invalid outbound rows_json")?;
    anyhow::ensure!(
        !parsed_rows.is_empty(),
        "outbound importer did not extract any companies from {url}"
    );

    let now_ms = now_ms() as i64;
    let mut knowledge_rows = Vec::new();
    let mut record_ids = Vec::new();
    for item in parsed_rows.into_iter().take(100) {
        let name = first_nonempty(&[
            str_path(&item, &["name"]),
            str_path(&item, &["company_name"]),
        ]);
        if name.is_empty() {
            continue;
        }
        let website = first_nonempty(&[str_path(&item, &["website"]), str_path(&item, &["href"])]);
        let detail_url = str_path(&item, &["href"]);
        let company_id = format!(
            "co_{}",
            &hex_sha256(format!("{campaign_id}:{name}:{website}:{detail_url}").as_bytes())[..16]
        );
        let country = first_nonempty(&[
            str_path(&item, &["country"]),
            if germany_only {
                "Deutschland".to_string()
            } else {
                String::new()
            },
        ]);
        let row = serde_json::json!({
            "company_id": company_id,
            "campaign_id": campaign_id,
            "campaign_name": campaign_name,
            "source_id": source_id,
            "source_url": url,
            "company_name": name,
            "website": website,
            "domain": domain_from_url(&website),
            "detail_url": detail_url,
            "booth": str_path(&item, &["booth"]),
            "event": str_path(&item, &["event"]),
            "city": str_path(&item, &["city"]),
            "country": country,
            "description": str_path(&item, &["description"]),
            "qualification_status": "input",
            "research_status": "",
            "pipeline_status": "",
            "imported_at_ms": now_ms,
            "updated_at_ms": now_ms,
            "raw_json": serde_json::to_string(&item).unwrap_or_else(|_| "{}".to_string()),
        });
        record_ids.push(company_id);
        knowledge_rows.push(row);
    }
    anyhow::ensure!(
        !knowledge_rows.is_empty(),
        "outbound importer extracted rows but none had a company name"
    );

    let append_args = vec![
        "data".to_string(),
        "append".to_string(),
        "--domain".to_string(),
        domain.clone(),
        "--key".to_string(),
        table_key.clone(),
        "--rows".to_string(),
        serde_json::to_string(&knowledge_rows)?,
    ];
    let append_result = crate::knowledge::dispatch_capturing(root, &append_args)?;
    anyhow::ensure!(
        append_result.get("ok").and_then(Value::as_bool) == Some(true),
        "knowledge data append failed: {}",
        append_result
    );

    write_import_artifact(
        root,
        command_id,
        "outbound_companies.json",
        &serde_json::json!({
            "command_id": command_id,
            "queue_task_id": queue_task.map(|task| task.message_key.clone()),
            "source_id": source_id,
            "source_url": url,
            "campaign_id": campaign_id,
            "knowledge_domain": domain,
            "knowledge_table_key": table_key,
            "records_count": knowledge_rows.len(),
            "records": knowledge_rows,
            "automation": automation,
        }),
    )?;

    Ok(ImportOutcome {
        collection: "knowledge_data_rows".to_string(),
        definition_id: "outbound.companies.v1".to_string(),
        records_count: record_ids.len(),
        record_ids,
    })
}

fn ensure_outbound_knowledge_contract(
    root: &Path,
    command: &BusinessCommand,
    domain: &str,
    companies_key: &str,
    campaign_id: &str,
    campaign_name: &str,
) -> anyhow::Result<()> {
    let contacts_key = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "contacts_table_key"]),
        format!("campaign_{}_contacts", slug_for_key(campaign_id)),
    ]);
    let runs_key = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "runs_table_key"]),
        format!("campaign_{}_research_runs", slug_for_key(campaign_id)),
    ]);
    let runbook_id = first_nonempty(&[
        str_path(&command.payload, &["writeback_contract", "runbook_id"]),
        str_path(&command.payload, &["knowledge", "runbook_id"]),
        format!(
            "business-os.outbound.{}.runbook.v1",
            slug_for_key(campaign_id)
        ),
    ]);
    let skillbook_id = first_nonempty(&[
        str_path(&command.payload, &["knowledge", "skillbook_id"]),
        "business-os.outbound.skillbook.v1".to_string(),
    ]);

    ensure_knowledge_data_table(
        root,
        domain,
        companies_key,
        &format!("{campaign_name} · Unternehmen"),
        "Outbound Campaign Firmen, Importjobs, Qualifikation und Unternehmens-Research.",
    )?;
    ensure_knowledge_data_table(
        root,
        domain,
        &contacts_key,
        &format!("{campaign_name} · Ansprechpartner"),
        "Outbound Campaign Ansprechpartner- und Lead-Qualifikation.",
    )?;
    ensure_knowledge_data_table(
        root,
        domain,
        &runs_key,
        &format!("{campaign_name} · Research Runs"),
        "Outbound Campaign Research-Auftraege, Status und CTOX Command-Referenzen.",
    )?;

    let _ = crate::knowledge::dispatch_capturing(
        root,
        &[
            "skill".to_string(),
            "add-skillbook".to_string(),
            "--id".to_string(),
            skillbook_id.clone(),
            "--title".to_string(),
            "Business OS Outbound Campaigns".to_string(),
            "--version".to_string(),
            "v1".to_string(),
            "--mission".to_string(),
            "Outbound Campaigns fuehren Firmenquellen ueber Unternehmensqualifikation, Ansprechpartner-Recherche und Lead-Qualifikation.".to_string(),
            "--runtime-policy".to_string(),
            "Nutze Knowledge DataFrames als einzige record-shaped Wissensquelle. Outbound speichert nur Workflow-State und Referenzen.".to_string(),
            "--workflow-backbone".to_string(),
            "source-import,company-research,pipeline-contact-research,lead-qualification".to_string(),
            "--linked-runbooks".to_string(),
            runbook_id.clone(),
        ],
    );
    let _ = crate::knowledge::dispatch_capturing(
        root,
        &[
            "skill".to_string(),
            "add-runbook".to_string(),
            "--id".to_string(),
            runbook_id.clone(),
            "--skillbook".to_string(),
            skillbook_id.clone(),
            "--title".to_string(),
            format!("{campaign_name} Campaign Runbook"),
            "--version".to_string(),
            "v1".to_string(),
            "--problem-domain".to_string(),
            "outbound-campaign".to_string(),
            "--status".to_string(),
            "active".to_string(),
            "--item-labels".to_string(),
            "CAMPAIGN-SCOPE,DATAFRAMES,FUNNEL-RUNS".to_string(),
        ],
    );
    let runbook_chunk = format!(
        "Campaign: {campaign_name}\nCampaign ID: {campaign_id}\n\nRecord-shaped Knowledge:\n- Companies DataFrame: ctox knowledge data describe --domain {domain} --key {companies_key}\n- Contacts DataFrame: ctox knowledge data describe --domain {domain} --key {contacts_key}\n- Research Runs DataFrame: ctox knowledge data describe --domain {domain} --key {runs_key}\n\nFunnel:\n1. Importjobs anlegen, daraus Unternehmen extrahieren und nur Unternehmen in den Companies DataFrame schreiben.\n2. Unternehmensdaten recherchieren, belegen und Firmen qualifizieren.\n3. Erst nach Unternehmensqualifikation Ansprechpartner im Contacts DataFrame recherchieren.\n4. Ansprechpartner gegen Scope/ICP qualifizieren und erst dann als Lead markieren.\n\nGrenze: Vor Pipeline-Stufe keine Personen recherchieren und keine Outreach-Nachrichten erzeugen."
    );
    let _ = crate::knowledge::dispatch_capturing(
        root,
        &[
            "skill".to_string(),
            "add-item".to_string(),
            "--id".to_string(),
            format!("{runbook_id}.scope"),
            "--runbook".to_string(),
            runbook_id,
            "--skillbook".to_string(),
            skillbook_id,
            "--label".to_string(),
            "CAMPAIGN-SCOPE".to_string(),
            "--title".to_string(),
            "Campaign Scope und Datenvertrag".to_string(),
            "--problem-class".to_string(),
            "outbound-campaign-scope".to_string(),
            "--chunk-text".to_string(),
            runbook_chunk,
            "--version".to_string(),
            "v1".to_string(),
            "--status".to_string(),
            "active".to_string(),
            "--skip-embedding".to_string(),
        ],
    );
    Ok(())
}

fn ensure_knowledge_data_table(
    root: &Path,
    domain: &str,
    key: &str,
    title: &str,
    description: &str,
) -> anyhow::Result<()> {
    let describe = crate::knowledge::dispatch_capturing(
        root,
        &[
            "data".to_string(),
            "describe".to_string(),
            "--domain".to_string(),
            domain.to_string(),
            "--key".to_string(),
            key.to_string(),
        ],
    );
    if describe
        .as_ref()
        .ok()
        .and_then(|payload| payload.get("ok"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Ok(());
    }
    let create = crate::knowledge::dispatch_capturing(
        root,
        &[
            "data".to_string(),
            "create".to_string(),
            "--domain".to_string(),
            domain.to_string(),
            "--key".to_string(),
            key.to_string(),
            "--source-system".to_string(),
            "business-os.outbound".to_string(),
            "--title".to_string(),
            title.to_string(),
            "--description".to_string(),
            description.to_string(),
        ],
    )?;
    anyhow::ensure!(
        create.get("ok").and_then(Value::as_bool) == Some(true),
        "knowledge data table create failed: {}",
        create
    );
    Ok(())
}

fn slug_for_key(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
        if out.len() >= 80 {
            break;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "campaign".to_string()
    } else {
        trimmed
    }
}

fn outbound_import_germany_only(command: &BusinessCommand) -> bool {
    let haystack = serde_json::to_string(&serde_json::json!({
        "filter_prompt": command.payload.get("filter_prompt"),
        "source_filter_prompt": command.payload.get("source").and_then(|source| source.get("filter_prompt")),
        "definition": command.payload.get("definition"),
    }))
    .unwrap_or_default()
    .to_ascii_lowercase();
    haystack.contains("deutschland")
        || haystack.contains("germany")
        || haystack.contains("\"de\"")
        || haystack.contains("sitz in deutschland")
        || haystack.contains("deutsche unternehmen")
}

fn domain_from_url(url: &str) -> String {
    let value = url.trim();
    let without_scheme = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    let host = without_scheme.split('/').next().unwrap_or("").trim();
    host.trim_start_matches("www.").to_ascii_lowercase()
}

const OUTBOUND_COMPANY_BROWSER_SCRIPT: &str = r#"
const importUrl = __IMPORT_URL__;
const germanyOnly = __GERMANY_ONLY__;
const maxRows = __MAX_ROWS__;

await page.goto(importUrl, { waitUntil: "networkidle", timeout: 45000 });
await page.waitForTimeout(1800);

async function clickGermanyFilter() {
  if (!germanyOnly) return;
  await page.evaluate(() => {
    const link = Array.from(document.querySelectorAll(".search-filters-countries a[data-value]"))
      .find((item) => /Deutschland|Germany/i.test(item.textContent || ""));
    if (link) link.click();
  });
  await page.waitForTimeout(2600);
}

function normalize(text) {
  return String(text || "").replace(/\s+/g, " ").trim();
}

async function collectVisible() {
  return await page.evaluate((onlyGermany) => {
    const normalize = (text) => String(text || "").replace(/\s+/g, " ").trim();
    return Array.from(document.querySelectorAll("a.teaser")).map((el) => {
      const lines = String(el.innerText || "")
        .split(/\n+/)
        .map((line) => normalize(line))
        .filter(Boolean);
      const countryIndex = lines.findIndex((line) => /^(Deutschland|Germany)$/i.test(line));
      const event = countryIndex >= 2 ? lines[countryIndex - 1] : "";
      const name = countryIndex >= 2 ? lines[countryIndex - 2] : (lines[1] || lines[0] || "");
      const booth = countryIndex >= 2 ? lines.slice(0, countryIndex - 2).join(", ") : "";
      const country = countryIndex >= 0 ? lines[countryIndex] : (onlyGermany ? "Deutschland" : "");
      const description = countryIndex >= 0 ? lines.slice(countryIndex + 1).join(" ") : lines.slice(2).join(" ");
      const href = el.href || el.getAttribute("href") || "";
      return { name, company_name: name, booth, event, country, description, href, website: href };
    }).filter((row) => row.name && (!onlyGermany || /^(Deutschland|Germany)$/i.test(row.country)));
  }, germanyOnly);
}

async function clickSortCharacter(letter) {
  return await page.evaluate((value) => {
    const link = Array.from(document.querySelectorAll(".search-filters-sorting-characters a[data-value]"))
      .find((item) => String(item.dataset.value || "") === value);
    if (!link) return false;
    link.click();
    return true;
  }, letter);
}

await clickGermanyFilter();

const seen = new Map();
async function addVisibleRows() {
  const rows = await collectVisible();
  for (const row of rows) {
    const key = `${row.name}|${row.href}`;
    if (!seen.has(key)) seen.set(key, row);
    if (seen.size >= maxRows) break;
  }
}

await addVisibleRows();
const letters = ["B","C","D","E","F","G","H","I","J","K","L","M","N","O","P","Q","R","S","T","U","V","W","X","Y","Z"];
for (const letter of letters) {
  if (seen.size >= maxRows) break;
  const clicked = await clickSortCharacter(letter);
  if (!clicked) continue;
  await page.waitForTimeout(1800);
  await addVisibleRows();
}

const rows = Array.from(seen.values()).slice(0, maxRows);
return { count: rows.length, rows_json: JSON.stringify(rows) };
"#;

#[derive(Default)]
struct ParsedJob {
    source_name: String,
    title: String,
    company_name: String,
    company_website: Option<String>,
    company_logo: Option<String>,
    external_ref: Option<String>,
    date_posted: Option<String>,
    location: String,
    street: Option<String>,
    city: String,
    zip: String,
    country: String,
    country_name: String,
    work_model: Option<String>,
    contract_type: Option<String>,
    remote: bool,
    about_company: String,
    about_role: String,
    responsibilities: Vec<String>,
    candidate_requirements: String,
    requirements: Vec<String>,
    benefits: Vec<String>,
    closing_notes: String,
    raw_text: String,
    fachlevel_class: i64,
    incentives_value: i64,
    confidence: f64,
}

fn parse_job_html(url: &str, html: &str) -> ParsedJob {
    let document = Html::parse_document(html);
    let json_ld = extract_jobposting_json(&document);
    let title = json_ld
        .as_ref()
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .map(clean_text)
        .or_else(|| meta_content(&document, "meta[property=\"og:title\"]"))
        .or_else(|| select_text(&document, "title"))
        .unwrap_or_else(|| "Unbenannte Stelle".to_string())
        .replace(" - Bosch Stellenportal", "")
        .replace(" Stellendetails | Alfred Kärcher SE & Co. KG", "");
    let description_html = json_ld
        .as_ref()
        .and_then(|value| value.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut raw_text = if !description_html.trim().is_empty() {
        html_fragment_text(description_html)
    } else {
        select_text(&document, ".jobdescription")
            .or_else(|| select_text(&document, "body"))
            .unwrap_or_default()
    };
    raw_text = normalize_lines(&raw_text);

    let company_name = json_ld
        .as_ref()
        .and_then(|value| value.get("hiringOrganization"))
        .and_then(|org| org.get("name").or_else(|| org.get("@value")))
        .and_then(Value::as_str)
        .map(clean_text)
        .or_else(|| micro_meta(&document, "hiringOrganization"))
        .unwrap_or_else(|| infer_company_from_url(url));
    let location = json_ld
        .as_ref()
        .and_then(|value| value.get("jobLocation"))
        .and_then(first_json)
        .and_then(|place| place.get("address"))
        .map(address_text)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| select_text(&document, ".jobGeoLocation"))
        .unwrap_or_default();
    let city = json_ld
        .as_ref()
        .and_then(|value| value.get("jobLocation"))
        .and_then(first_json)
        .and_then(|place| place.get("address"))
        .and_then(|address| address.get("addressLocality"))
        .and_then(Value::as_str)
        .map(clean_text)
        .or_else(|| select_text(&document, "[data-careersite-propertyid=\"city\"]"))
        .unwrap_or_else(|| location.split(',').next().unwrap_or("").trim().to_string());
    let zip = json_ld
        .as_ref()
        .and_then(|value| value.get("jobLocation"))
        .and_then(first_json)
        .and_then(|place| place.get("address"))
        .and_then(|address| address.get("postalCode"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let street = json_ld
        .as_ref()
        .and_then(|value| value.get("jobLocation"))
        .and_then(first_json)
        .and_then(|place| place.get("address"))
        .and_then(|address| address.get("streetAddress"))
        .and_then(Value::as_str)
        .map(clean_text);
    let external_ref = json_ld
        .as_ref()
        .and_then(|value| value.get("identifier"))
        .and_then(|id| id.get("value"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| regex_capture(html, r#""idFS"\s*:\s*"([^"]+)""#))
        .or_else(|| regex_capture(url, r"[?&]id=([^&]+)"));
    let date_posted = json_ld
        .as_ref()
        .and_then(|value| value.get("datePosted"))
        .and_then(Value::as_str)
        .map(|value| value.chars().take(10).collect::<String>())
        .or_else(|| {
            micro_meta(&document, "datePosted").map(|value| value.chars().take(10).collect())
        });
    let company_website = json_ld
        .as_ref()
        .and_then(|value| value.get("hiringOrganization"))
        .and_then(|org| org.get("sameAs"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| origin_url(url));
    let company_logo = json_ld
        .as_ref()
        .and_then(|value| value.get("hiringOrganization"))
        .and_then(|org| org.get("logo"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let (about_company, about_role, requirements, benefits, closing_notes) =
        split_job_sections(&raw_text);
    let responsibilities = bulletize(&about_role);
    let requirement_bullets = bulletize(&requirements);
    let benefits_vec = bulletize(&benefits);
    let lower = raw_text.to_lowercase();

    ParsedJob {
        source_name: if url.contains("bosch.") {
            "Bosch Stellenportal".to_string()
        } else if url.contains("kaercher.") || url.contains("karcher.") {
            "Kärcher Careers".to_string()
        } else {
            origin_url(url).unwrap_or_else(|| "Other".to_string())
        },
        title,
        company_name,
        company_website,
        company_logo,
        external_ref,
        date_posted,
        location: if location.is_empty() {
            city.clone()
        } else {
            location
        },
        street,
        city,
        zip,
        country: "DE".to_string(),
        country_name: "Deutschland".to_string(),
        work_model: Some("Vollzeit".to_string()),
        contract_type: if lower.contains("unbefristet") {
            Some("unbefristet".to_string())
        } else {
            None
        },
        remote: lower.contains("home-office") || lower.contains("mobiles arbeiten"),
        about_company,
        about_role: about_role.clone(),
        responsibilities,
        candidate_requirements: requirements.clone(),
        requirements: requirement_bullets,
        benefits: benefits_vec.clone(),
        closing_notes,
        raw_text,
        fachlevel_class: infer_fachlevel(&about_role, &requirements),
        incentives_value: if benefits_vec.len() >= 4 { 2 } else { 1 },
        confidence: 0.86,
    }
}

#[derive(Default)]
struct CandidateParse {
    name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    current_role: Option<String>,
    desired_position: Option<String>,
    highest_degree: Option<String>,
    region: Option<String>,
    driver_license: Vec<String>,
    languages: Vec<Value>,
    skills: Vec<String>,
    leadership: Option<String>,
    confidence: f64,
}

fn parse_candidate_text(filename: &str, text: &str) -> CandidateParse {
    let normalized = normalize_lines(text);
    let name = extract_candidate_name(filename, &normalized);
    let mut parts = name.split_whitespace().collect::<Vec<_>>();
    let first_name = parts.first().map(|value| (*value).to_string());
    let last_name = parts.pop().map(|value| value.to_string()).filter(|value| {
        first_name.as_deref() != Some(value.as_str()) || name.split_whitespace().count() == 1
    });
    let email = regex_capture(
        &normalized,
        r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b",
    );
    let phone = regex_capture(&normalized, r"(?x)(?:\+49|0)\s*(?:\d[\s()/.-]*){6,}\d");
    let current_role = extract_current_role(&normalized);
    let desired_position = regex_capture(
        &normalized,
        r"(?i)(?:Angestrebte Position|gewünschte Position)\s*:?\s*([^\n]+)",
    );
    let highest_degree = if normalized.contains("Master") {
        Some("Master".to_string())
    } else if normalized.contains("Bachelor") {
        Some("Bachelor".to_string())
    } else if normalized.contains("Techniker") {
        Some("Staatlich geprüfter Techniker".to_string())
    } else {
        None
    };
    let region = regex_capture(&normalized, r"(?i)(?:Ort|Adresse|Wohnort)\s*:?\s*([^\n]+)")
        .or_else(|| regex_capture(&normalized, r"\b\d{5}\s+([A-ZÄÖÜ][^\n,]+)"));
    let lower = normalized.to_lowercase();
    let driver_license = if lower.contains("führerschein") || lower.contains("fuehrerschein") {
        vec!["B".to_string()]
    } else {
        Vec::new()
    };
    let languages = language_values(&normalized);
    let skills = extract_skills(&normalized);
    let leadership = if lower.contains("leitung")
        || lower.contains("projektleitung")
        || lower.contains("koordination")
        || lower.contains("koordiniert")
        || lower.contains("teamarbeit")
        || lower.contains("fachliche führung")
    {
        Some("Hinweise auf Koordination, Teamarbeit oder fachliche Leitung im CV.".to_string())
    } else {
        None
    };

    CandidateParse {
        name,
        first_name,
        last_name,
        email,
        phone,
        current_role,
        desired_position,
        highest_degree,
        region,
        driver_license,
        languages,
        skills,
        leadership,
        confidence: if text.trim().is_empty() { 0.25 } else { 0.78 },
    }
}

/// SSRF-guarded HTTP GET for caller-supplied job-posting URLs. Rejects
/// non-http(s) schemes and installs a resolver that filters every DNS result —
/// including redirect re-resolutions — down to publicly routable addresses, so
/// a supplied URL cannot reach loopback / RFC1918 / link-local / CGNAT / cloud-
/// metadata endpoints.
///
/// NOTE: the IP-range logic duplicates `ctox-web-stack`'s `egress` module
/// (currently `pub(crate)`); fold onto that shared guard once it is exposed.
pub(crate) fn fetch_url_guarded(url: &str) -> anyhow::Result<ureq::Response> {
    let parsed =
        url::Url::parse(url).map_err(|err| anyhow::anyhow!("invalid URL '{url}': {err}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => anyhow::bail!("refusing to fetch URL with non-http(s) scheme '{other}': {url}"),
    }
    let agent = ureq::AgentBuilder::new()
        .resolver(PublicOnlyResolver)
        .timeout(Duration::from_secs(30))
        .build();
    agent
        .get(url)
        .set("User-Agent", "Mozilla/5.0 CTOX Business OS Importer")
        .call()
        .with_context(|| format!("failed to fetch {url}"))
}

struct PublicOnlyResolver;

impl ureq::Resolver for PublicOnlyResolver {
    fn resolve(&self, netloc: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
        use std::net::ToSocketAddrs;
        let public: Vec<std::net::SocketAddr> = netloc
            .to_socket_addrs()?
            .filter(|addr| is_public_ip(addr.ip()))
            .collect();
        if public.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("SSRF guard: '{netloc}' resolves only to non-public addresses"),
            ));
        }
        Ok(public)
    }
}

/// True for publicly routable addresses; false for loopback, RFC1918 private,
/// link-local (incl. 169.254.169.254 metadata), CGNAT (100.64/10), broadcast,
/// documentation, unspecified, and the IPv6 equivalents. Uses only stable
/// `std::net` predicates.
fn is_public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || o[0] == 0
                || (o[0] == 100 && (64..=127).contains(&o[1])))
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return false;
            }
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(mapped));
            }
            let seg = v6.segments();
            // fe80::/10 link-local, fc00::/7 unique-local.
            !((seg[0] & 0xffc0) == 0xfe80 || (seg[0] & 0xfe00) == 0xfc00)
        }
    }
}

fn parse_pdf_text(bytes: &[u8]) -> anyhow::Result<String> {
    let result = ctox_pdf_parse::parse_pdf_bytes(
        bytes,
        ctox_pdf_parse::LiteParseConfigOverrides {
            ocr_enabled: Some(false),
            max_pages: Some(20),
            precise_bounding_box: Some(false),
            output_format: Some(ctox_pdf_parse::OutputFormat::Text),
            ..Default::default()
        },
    )
    .context("native PDF parser failed")?;
    Ok(result.text)
}

/// Maximum number of PDF pages sent to the vision model for OCR (bounds token /
/// latency cost on long scans).
const OCR_MAX_PAGES: usize = 8;
/// Render DPI for OCR page images — legible glyphs without oversized payloads.
const OCR_RENDER_DPI: u16 = 150;
const OCR_INSTRUCTIONS: &str = "You are an OCR engine. Transcribe ALL visible text from the \
    provided PDF page images verbatim, preserving reading order and line breaks. Output only the \
    transcribed text with no commentary. Separate pages with a blank line.";

/// OCR a (likely scanned / image-only) PDF by rendering its pages and asking a
/// vision-capable model to transcribe them through the internal Responses
/// gateway. Returns `Some(text)` on success and `None` whenever vision OCR is
/// unavailable — no vision-capable model configured, managed local inference
/// (private-IPC only; no loopback HTTP and no local vision backend wired), or
/// the gateway is unreachable. It never panics and never fails the import.
fn ocr_pdf_via_vision(root: &Path, pdf_bytes: &[u8]) -> Option<String> {
    use base64::Engine as _;

    let resolved =
        crate::execution::models::runtime_kernel::InferenceRuntimeKernel::resolve(root).ok()?;
    let model = resolved.active_model()?.to_string();
    if !crate::execution::models::engine::model_supports_vision(&model) {
        return None;
    }
    if resolved.state.source.is_local() {
        return None;
    }

    let pages =
        ctox_pdf_parse::render_pdf_pages_png(pdf_bytes, OCR_MAX_PAGES, OCR_RENDER_DPI, None)
            .ok()?;
    if pages.is_empty() {
        return None;
    }

    let mut content: Vec<serde_json::Value> = Vec::with_capacity(pages.len() + 1);
    content.push(serde_json::json!({"type": "input_text", "text": "Transcribe these PDF pages:"}));
    for png in &pages {
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        content.push(serde_json::json!({
            "type": "input_image",
            "mime_type": "image/png",
            "image_data": encoded,
        }));
    }

    let request = serde_json::json!({
        "model": model,
        "instructions": OCR_INSTRUCTIONS,
        "input": [{"type": "message", "role": "user", "content": content}],
    });

    let base_url = resolved.internal_responses_base_url();
    let response = ureq::post(&format!("{}/v1/responses", base_url.trim_end_matches('/')))
        .set("content-type", "application/json")
        .timeout(Duration::from_secs(120))
        .send_string(&serde_json::to_string(&request).ok()?)
        .ok()?;
    let body = response.into_string().ok()?;
    let payload: serde_json::Value = serde_json::from_str(&body).ok()?;
    extract_responses_output_text(&payload).filter(|text| !text.trim().is_empty())
}

/// Extract assistant text from a Responses-API payload, supporting both the flat
/// `output_text` field and the structured `output[].content[]` array.
fn extract_responses_output_text(payload: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    if let Some(text) = payload.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return Some(text.to_string());
        }
    }
    payload
        .get("output")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .and_then(|content| {
                        content.iter().find_map(|part| {
                            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                                part.get("text")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            } else {
                                None
                            }
                        })
                    })
            })
        })
}

fn extract_candidate_name(filename: &str, text: &str) -> String {
    if let Some(value) = regex_capture(text, r"(?i)Name und Vorname:\s*([^\n]+)") {
        let parts = value.split_whitespace().collect::<Vec<_>>();
        if parts.len() == 2 {
            return format!("{} {}", parts[1], parts[0]);
        }
        return clean_name(&value);
    }
    for raw in text.lines().take(8) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Lebenslauf") {
            let candidate = clean_name(rest);
            if looks_like_name(&candidate) {
                return candidate;
            }
        }
        if looks_like_name(line) {
            return clean_name(line);
        }
    }
    filename_name(filename)
}

fn filename_name(filename: &str) -> String {
    let lower_filename = filename.to_lowercase();
    let levenslauf_suffix = lower_filename.strip_prefix("lebenslauf_").map(|_| {
        filename
            .trim_start_matches("Lebenslauf_")
            .trim_start_matches("lebenslauf_")
            .to_string()
    });
    let mut name = filename.replace(".pdf", "").replace(".PDF", "");
    for token in [
        "Lebenslauf",
        "lebenslauf",
        "CV",
        "cv",
        "JAN2026",
        "KEIN Upload",
        "(1)",
    ] {
        name = name.replace(token, " ");
    }
    let name = name.replace(['_', '-'], " ");
    let mut parts = name
        .split_whitespace()
        .filter(|part| !part.chars().all(|ch| ch.is_ascii_digit()))
        .collect::<Vec<_>>();
    let looks_reversed = levenslauf_suffix.as_deref().is_some_and(|suffix| {
        suffix.contains('_') && !suffix.split('_').next().unwrap_or("").contains(' ')
    });
    if parts.len() == 2 && looks_reversed {
        parts.swap(0, 1);
    }
    clean_name(&parts.join(" "))
}

fn clean_name(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|part| !part.chars().all(|ch| ch.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| !ch.is_alphabetic())
        .to_string()
}

fn looks_like_name(value: &str) -> bool {
    let cleaned = clean_name(value);
    let lower = cleaned.to_lowercase();
    let parts = cleaned.split_whitespace().collect::<Vec<_>>();
    parts.len() >= 2
        && parts.len() <= 4
        && !cleaned.contains(':')
        && !lower.contains("lebenslauf")
        && !lower.contains("zur person")
        && !lower.contains("persönliche daten")
        && !lower.contains("persoenliche daten")
        && !lower.contains("berufliche tätigkeiten")
        && !lower.contains("berufliche taetigkeiten")
        && !lower.contains("ausbildungsweg")
        && !lower.contains("geburt")
        && !lower.contains("adresse")
        && !lower.contains("telefon")
        && !lower.contains("email")
        && !lower.contains("staatsangehörigkeit")
        && !lower.contains("staatsangehoerigkeit")
        && !lower.contains("deutsch")
        && parts
            .iter()
            .all(|part| part.chars().next().is_some_and(|ch| ch.is_uppercase()))
}

fn extract_current_role(text: &str) -> Option<String> {
    for pattern in [
        r"(?i)(?:aktuelle\s+(?:position|tätigkeit)|derzeitige\s+(?:position|tätigkeit)|current\s+role)\s*:?\s*([^\n]+)",
        r"(?i)(?:angestrebte\s+position|gewünschte\s+position)\s*:?\s*([^\n]+)",
    ] {
        if let Some(value) = regex_capture(text, pattern) {
            let cleaned = clean_role_line(&value);
            if is_plausible_role(&cleaned) {
                return Some(cleaned);
            }
        }
    }

    let lines = text.lines().map(clean_text).collect::<Vec<_>>();
    for idx in 0..lines.len() {
        let lower = lines[idx].to_lowercase();
        let is_recent_marker = lower.contains("heute")
            || lower.contains("aktuell")
            || lower.contains("present")
            || lower.contains("seit ");
        if !is_recent_marker {
            continue;
        }
        for offset in 0..=2 {
            let role = clean_role_line(&joined_cv_line(&lines, idx + offset, 3));
            if is_plausible_role(&role) {
                return Some(role);
            }
        }
    }

    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("ingenieur")
            || lower.contains("techniker")
            || lower.contains("entwickler")
            || lower.contains("mechaniker")
            || lower.contains("elektriker")
            || lower.contains("meister")
            || lower.contains("tester")
            || lower.contains("manager")
        {
            let cleaned = clean_role_line(line);
            if is_plausible_role(&cleaned) {
                return Some(cleaned);
            }
        }
    }
    None
}

fn clean_role_line(value: &str) -> String {
    let without_dates = Regex::new(
        r"(?ix)
        \b(?:\d{1,2}[./])?\d{2,4}\s*(?:-|–|—|bis|/)\s*(?:heute|aktuell|present|\d{1,2}[./]?\d{2,4})\b
        |\b(?:januar|februar|märz|maerz|april|mai|juni|juli|august|september|oktober|november|dezember)\s+\d{4}\s*(?:-|–|—|bis|/)\s*(?:heute|aktuell|present|\d{1,2}[./]?\d{2,4})\b
        |\b(?:seit|ab)\s+\d{1,2}[./]?\d{2,4}\b
        ",
    )
    .ok()
    .map(|regex| regex.replace_all(value, " ").to_string())
    .unwrap_or_else(|| value.to_string());
    clean_text(&without_dates)
        .trim_matches(['-', '–', '—', ':', '|', '·', ','])
        .trim()
        .to_string()
}

fn joined_cv_line(lines: &[String], start: usize, max_lines: usize) -> String {
    let mut out = Vec::new();
    for line in lines.iter().skip(start).take(max_lines) {
        if line.trim().is_empty() {
            break;
        }
        out.push(line.trim().to_string());
        let joined = out.join(" ");
        if !joined.ends_with(" und")
            && !joined.ends_with(" /")
            && !joined.ends_with(',')
            && joined.len() >= 32
        {
            break;
        }
    }
    out.join(" ")
}

fn is_plausible_role(value: &str) -> bool {
    let cleaned = clean_text(value);
    let lower = cleaned.to_lowercase();
    cleaned.len() >= 8
        && cleaned.len() <= 140
        && !lower.contains('@')
        && !lower.contains("telefon")
        && !lower.contains("email")
        && !lower.contains("adresse")
        && !lower.contains("geburt")
        && !lower.starts_with("ich ")
        && !lower.starts_with("ich habe")
        && !lower.contains("ich habe langjährige")
        && !lower.contains("berufsausbildung")
        && !lower.contains("ausbildung:")
        && !lower.contains("lehre zum")
        && !lower.contains("führerschein")
        && !lower.contains("fuehrerschein")
        && !lower.contains("skills")
        && !lower.contains("kenntnisse")
        && !lower.contains("lebenslauf")
}

fn extract_skills(text: &str) -> Vec<String> {
    let known = [
        "Python",
        "SQL",
        "Databricks",
        "Grafana",
        "Tableau",
        "Altium",
        "C",
        "C++",
        "CAD",
        "CATIA",
        "SAP",
        "MSR",
        "Gebäudeautomation",
        "Automatisierung",
        "HIL",
        "SIL",
        "ISO 26262",
        "ASPICE",
        "Polarion",
        "dSPACE",
        "Messtechnik",
        "Validierung",
        "Projektleitung",
        "Maschinenbau",
        "Elektrotechnik",
        "Sonderanlagenbau",
        "Koordination",
        "Teamarbeit",
        "Fahrerassistenzsysteme",
        "Typisierung",
        "Testmanagement",
    ];
    let mut out = Vec::new();
    for skill in known {
        if contains_skill(text, skill) {
            out.push(skill.to_string());
        }
    }
    out
}

fn contains_skill(text: &str, skill: &str) -> bool {
    if skill == "C" {
        return Regex::new(r"(?i)(^|[^A-Z0-9+#])C([^A-Z0-9+#]|$)")
            .ok()
            .is_some_and(|regex| regex.is_match(text));
    }
    if skill == "C++" {
        return Regex::new(r"(?i)(^|[^A-Z0-9+#])C\+\+([^A-Z0-9+#]|$)")
            .ok()
            .is_some_and(|regex| regex.is_match(text));
    }
    text.to_lowercase().contains(&skill.to_lowercase())
}

fn language_values(text: &str) -> Vec<Value> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    if lower.contains("deutsch") {
        out.push(serde_json::json!({ "code": "de", "level": "" }));
    }
    if lower.contains("englisch") || lower.contains("english") {
        out.push(serde_json::json!({ "code": "en", "level": "" }));
    }
    out
}

fn split_job_sections(raw_text: &str) -> (String, String, String, String, String) {
    let about_company = before_any(
        raw_text,
        &[
            "Was dich bei uns erwartet:",
            "Stellenbeschreibung:",
            "Das werden Ihre WOW-Momente:",
        ],
    );
    let about_role = between_any(
        raw_text,
        &[
            "Was dich bei uns erwartet:",
            "Stellenbeschreibung:",
            "Das werden Ihre WOW-Momente:",
        ],
        &[
            "Ausbildung:",
            "Erfahrungen & Know-How:",
            "Arbeitsweise & Persönlichkeit:",
            "Qualifikationen:",
            "Es wäre WOW, wenn Sie das hier mitbringen:",
        ],
    );
    let explicit_requirements = collect_sections(
        raw_text,
        &[
            "Ausbildung:",
            "Erfahrungen & Know-How:",
            "Arbeitsweise & Persönlichkeit:",
            "Qualifikationen:",
            "Es wäre WOW, wenn Sie das hier mitbringen:",
        ],
        &["Zusätzliche Informationen:", "Unser Kärcher WOW-Paket:"],
    );
    let requirements = if explicit_requirements.trim().is_empty() {
        between_any(
            raw_text,
            &[
                "Ausbildung:",
                "Erfahrungen & Know-How:",
                "Arbeitsweise & Persönlichkeit:",
                "Qualifikationen:",
                "Es wäre WOW, wenn Sie das hier mitbringen:",
            ],
            &["Zusätzliche Informationen:", "Unser Kärcher WOW-Paket:"],
        )
    } else {
        explicit_requirements
    };
    let benefits = between_any(
        raw_text,
        &["Unser Kärcher WOW-Paket:", "Wir bieten Ihnen:"],
        &["Also: Wanna WOW with us?", "Vielfalt und Inklusion"],
    );
    let closing = after_any(
        raw_text,
        &["Zusätzliche Informationen:", "Also: Wanna WOW with us?"],
    );
    (
        normalize_lines(&about_company),
        normalize_lines(&about_role),
        normalize_lines(&requirements),
        normalize_lines(&benefits),
        normalize_lines(&closing),
    )
}

fn collect_sections(text: &str, starts: &[&str], terminal_markers: &[&str]) -> String {
    let mut out = Vec::new();
    let mut active = false;
    for raw in text.lines() {
        let line = clean_text(raw);
        if line.is_empty() {
            continue;
        }
        if terminal_markers
            .iter()
            .any(|marker| line.eq_ignore_ascii_case(marker.trim_end_matches(':')))
            || terminal_markers
                .iter()
                .any(|marker| line.starts_with(marker))
        {
            if active {
                break;
            }
        }
        if starts.iter().any(|marker| line.starts_with(marker)) {
            active = true;
            out.push(line);
            continue;
        }
        if active {
            out.push(line);
        }
    }
    out.join("\n")
}

fn before_any(text: &str, markers: &[&str]) -> String {
    let idx = markers
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .unwrap_or(text.len());
    text[..idx].to_string()
}

fn after_any(text: &str, markers: &[&str]) -> String {
    for marker in markers {
        if let Some(idx) = text.find(marker) {
            return text[idx + marker.len()..].to_string();
        }
    }
    String::new()
}

fn between_any(text: &str, starts: &[&str], ends: &[&str]) -> String {
    for start in starts {
        if let Some(start_idx) = text.find(start) {
            let rest = &text[start_idx + start.len()..];
            let end_idx = ends
                .iter()
                .filter_map(|marker| rest.find(marker))
                .min()
                .unwrap_or(rest.len());
            return rest[..end_idx].to_string();
        }
    }
    String::new()
}

fn bulletize(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['-', '•', '*'])
                .trim()
                .to_string()
        })
        .filter(|line| line.len() > 8)
        .take(24)
        .collect()
}

fn infer_fachlevel(role: &str, requirements: &str) -> i64 {
    let text = format!("{role}\n{requirements}").to_lowercase();
    if text.contains("studium") || text.contains("ingenieur") || text.contains("projekt") {
        3
    } else if text.contains("techniker") || text.contains("meister") || text.contains("mehrjährige")
    {
        2
    } else {
        1
    }
}

fn extract_jobposting_json(document: &Html) -> Option<Value> {
    let selector = Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;
    for script in document.select(&selector) {
        let text = script.text().collect::<Vec<_>>().join("");
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(found) = find_jobposting(&value) {
                return Some(found.clone());
            }
        }
    }
    None
}

fn find_jobposting(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => {
            let is_job = map
                .get("@type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("JobPosting"));
            if is_job {
                return Some(value);
            }
            map.values().find_map(find_jobposting)
        }
        Value::Array(items) => items.iter().find_map(find_jobposting),
        _ => None,
    }
}

fn first_json(value: &Value) -> Option<&Value> {
    value
        .as_array()
        .and_then(|items| items.first())
        .or(Some(value))
}

fn address_text(address: &Value) -> String {
    let city = address
        .get("addressLocality")
        .and_then(Value::as_str)
        .unwrap_or("");
    let region = address
        .get("addressRegion")
        .and_then(Value::as_str)
        .unwrap_or("");
    let zip = address
        .get("postalCode")
        .and_then(Value::as_str)
        .unwrap_or("");
    let country = address
        .get("addressCountry")
        .and_then(Value::as_str)
        .unwrap_or("Deutschland");
    [city, region, zip, country]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn html_fragment_text(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    let root = Selector::parse("body, html").ok();
    if let Some(selector) = root {
        let text = fragment
            .select(&selector)
            .flat_map(|node| node.text())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            return normalize_lines(&text);
        }
    }
    normalize_lines(
        &fragment
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .map(|node| normalize_lines(&node.text().collect::<Vec<_>>().join("\n")))
        .filter(|text| !text.trim().is_empty())
}

fn meta_content(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr("content"))
        .map(clean_text)
}

fn micro_meta(document: &Html, itemprop: &str) -> Option<String> {
    let selector = Selector::parse(&format!(r#"[itemprop="{itemprop}"]"#)).ok()?;
    document.select(&selector).next().and_then(|node| {
        node.value()
            .attr("content")
            .map(str::to_string)
            .or_else(|| {
                let text = clean_text(&node.text().collect::<Vec<_>>().join(" "));
                (!text.is_empty()).then_some(text)
            })
    })
}

fn requested_research_fields(command: &BusinessCommand) -> Vec<String> {
    command
        .payload
        .get("research_request")
        .and_then(|value| value.get("fields"))
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| {
                    field
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| field.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn requested_contact_fields(command: &BusinessCommand) -> Vec<String> {
    command
        .payload
        .get("contact_fields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| {
                    field
                        .get("id")
                        .and_then(Value::as_str)
                        .or_else(|| field.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn company_field_keys(fields: &[String]) -> Vec<FieldKey> {
    let mut out = Vec::new();
    for field in fields {
        let mapped = match field.as_str() {
            "city" => Some(FieldKey::FirmaOrt),
            "postal_code" => Some(FieldKey::FirmaPlz),
            "street" => Some(FieldKey::FirmaAnschrift),
            "email" => Some(FieldKey::FirmaEmail),
            "domain" => Some(FieldKey::FirmaDomain),
            "industry_wz" => Some(FieldKey::WzCode),
            "revenue_eur" => Some(FieldKey::Umsatz),
            "employee_count" => Some(FieldKey::Mitarbeiter),
            _ => None,
        };
        if let Some(key) = mapped {
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    if out.is_empty() {
        out.extend([
            FieldKey::FirmaAnschrift,
            FieldKey::FirmaPlz,
            FieldKey::FirmaOrt,
            FieldKey::FirmaEmail,
            FieldKey::FirmaDomain,
            FieldKey::WzCode,
            FieldKey::Umsatz,
            FieldKey::Mitarbeiter,
        ]);
    }
    out
}

fn contact_field_keys(fields: &[String]) -> Vec<FieldKey> {
    let mut out = Vec::new();
    let mut push = |key: FieldKey| {
        if !out.contains(&key) {
            out.push(key);
        }
    };
    for field in fields {
        match field.as_str() {
            "contact.people" => {
                push(FieldKey::PersonVorname);
                push(FieldKey::PersonNachname);
                push(FieldKey::PersonFunktion);
                push(FieldKey::PersonLinkedin);
                push(FieldKey::PersonXing);
            }
            "contact.role" | "contact.fit" => {
                push(FieldKey::PersonFunktion);
                push(FieldKey::PersonPosition);
            }
            "contact.email" => push(FieldKey::PersonEmail),
            "contact.linkedin" => {
                push(FieldKey::PersonLinkedin);
                push(FieldKey::PersonXing);
            }
            "contact.phone" => push(FieldKey::PersonTelefon),
            value if value.starts_with("contact_custom_") => {
                push(FieldKey::PersonVorname);
                push(FieldKey::PersonNachname);
                push(FieldKey::PersonFunktion);
                push(FieldKey::PersonPosition);
                push(FieldKey::PersonLinkedin);
                push(FieldKey::PersonXing);
            }
            _ => {}
        }
    }
    if out.is_empty() {
        out.extend([
            FieldKey::PersonVorname,
            FieldKey::PersonNachname,
            FieldKey::PersonFunktion,
            FieldKey::PersonPosition,
            FieldKey::PersonEmail,
            FieldKey::PersonTelefon,
            FieldKey::PersonLinkedin,
            FieldKey::PersonXing,
        ]);
    }
    out
}

fn run_person_research(
    root: &Path,
    company: &str,
    country: &str,
    mode: ResearchMode,
    fields: Vec<FieldKey>,
) -> anyhow::Result<Value> {
    let country = Country::from_iso(country).unwrap_or(Country::De);
    ctox_web_stack::run_ctox_person_research_tool(
        root,
        &PersonResearchRequest {
            company: company.to_string(),
            country,
            mode,
            fields,
            include_private: Vec::new(),
            workspace: None,
            persist_workspace: false,
        },
    )
}

fn merge_json_objects(a: Option<&Value>, b: Option<&Value>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(obj) = a.and_then(Value::as_object) {
        for (key, value) in obj {
            out.insert(key.clone(), value.clone());
        }
    }
    if let Some(obj) = b.and_then(Value::as_object) {
        for (key, value) in obj {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

fn apply_company_research_fields(data: &mut Map<String, Value>, research: &Value) {
    for (web_key, outbound_key) in [
        ("firma_anschrift", "street"),
        ("firma_plz", "postal_code"),
        ("firma_ort", "city"),
        ("firma_email", "email"),
        ("firma_domain", "domain"),
        ("wz_code", "industry_wz"),
        ("umsatz", "revenue_eur"),
        ("mitarbeiter", "employee_count"),
    ] {
        if let Some(value) = research_field_value(research, web_key) {
            data.insert(outbound_key.to_string(), Value::String(value.clone()));
            data.insert(web_key.to_string(), Value::String(value));
        }
    }
}

fn research_field_value(research: &Value, field: &str) -> Option<String> {
    research
        .get("fields")
        .and_then(|fields| fields.get(field))
        .and_then(|entry| entry.get("value"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn evidence_from_research(research: &Value) -> Value {
    let mut evidence = Vec::new();
    if let Some(fields) = research.get("fields").and_then(Value::as_object) {
        for (field, entry) in fields {
            if let Some(candidates) = entry.get("candidates").and_then(Value::as_array) {
                for candidate in candidates.iter().take(6) {
                    let mut item = candidate.as_object().cloned().unwrap_or_default();
                    item.insert("field".to_string(), Value::String(field.clone()));
                    evidence.push(Value::Object(item));
                }
            } else if entry.get("value").is_some() {
                let mut item = Map::new();
                item.insert("field".to_string(), Value::String(field.clone()));
                item.insert(
                    "value".to_string(),
                    entry.get("value").cloned().unwrap_or(Value::Null),
                );
                if let Some(url) = entry.get("source_url").cloned() {
                    item.insert("source_url".to_string(), url);
                }
                evidence.push(Value::Object(item));
            }
        }
    }
    Value::Array(evidence)
}

fn contacts_from_research(
    pipeline_id: &str,
    company_id: &str,
    company_name: &str,
    research: &Value,
) -> Vec<Value> {
    let mut grouped: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for (field, target) in [
        ("person_vorname", "first_name"),
        ("person_nachname", "last_name"),
        ("person_funktion", "role"),
        ("person_position", "role"),
        ("person_email", "email"),
        ("person_telefon", "phone"),
        ("person_linkedin", "linkedin_url"),
        ("person_xing", "xing_url"),
    ] {
        let Some(candidates) = research
            .get("fields")
            .and_then(|fields| fields.get(field))
            .and_then(|entry| entry.get("candidates"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for candidate in candidates.iter().take(8) {
            let value = candidate
                .get("value")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if value.is_empty() {
                continue;
            }
            let source_url = candidate
                .get("source_url")
                .and_then(Value::as_str)
                .or_else(|| candidate.get("hit_url").and_then(Value::as_str))
                .unwrap_or(value);
            let key = source_url.to_string();
            let entry = grouped.entry(key).or_default();
            entry
                .entry(target.to_string())
                .or_insert_with(|| Value::String(value.to_string()));
            let evidence = entry
                .entry("evidence".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(items) = evidence {
                let mut ev = candidate.as_object().cloned().unwrap_or_default();
                ev.insert("field".to_string(), Value::String(field.to_string()));
                items.push(Value::Object(ev));
            }
        }
    }
    let mut contacts = Vec::new();
    let mut by_identity: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for (_, mut item) in grouped {
        let first = item
            .get("first_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let last = item
            .get("last_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let inferred_name = infer_name_from_profile_url(
            item.get("linkedin_url")
                .and_then(Value::as_str)
                .or_else(|| item.get("xing_url").and_then(Value::as_str))
                .unwrap_or(""),
        );
        let contact_name = first_nonempty(&[
            [first, last]
                .iter()
                .copied()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            inferred_name,
        ]);
        let role = item
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let linkedin = item
            .get("linkedin_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if contact_name.is_empty() && role.is_empty() && linkedin.is_empty() {
            continue;
        }
        let contact_id = format!(
            "contact_{}",
            &hex_sha256(
                format!("{pipeline_id}:{company_id}:{contact_name}:{role}:{linkedin}").as_bytes()
            )[..16]
        );
        item.insert("contact_id".to_string(), Value::String(contact_id));
        item.insert(
            "company_name".to_string(),
            Value::String(company_name.to_string()),
        );
        item.insert(
            "contact_name".to_string(),
            Value::String(contact_name.clone()),
        );
        let identity = first_nonempty(&[
            linkedin.clone(),
            item.get("xing_url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            normalize_whitespace(&format!("{contact_name} {role}")),
        ]);
        by_identity.entry(identity).or_insert(item);
    }
    for (_, item) in by_identity {
        contacts.push(Value::Object(item));
    }
    contacts
}

fn infer_name_from_profile_url(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return String::new();
    };
    let Some(segment) = parsed
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
    else {
        return String::new();
    };
    segment
        .trim_matches('/')
        .split('-')
        .filter(|part| !part.chars().all(|ch| ch.is_ascii_digit()))
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn load_contact_rows_for_pipeline(
    root: &Path,
    refs: &OutboundKnowledgeRefs,
    pipeline_id: &str,
) -> anyhow::Result<Vec<Value>> {
    let payload = crate::knowledge::dispatch_capturing(
        root,
        &[
            "data".to_string(),
            "select".to_string(),
            "--domain".to_string(),
            refs.domain.clone(),
            "--key".to_string(),
            refs.contacts_key.clone(),
            "--where".to_string(),
            format!("pipeline_id={pipeline_id}"),
            "--limit".to_string(),
            "200".to_string(),
        ],
    )?;
    Ok(payload
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn contact_row_is_qualifiable(row: &Value) -> bool {
    !first_nonempty(&[
        str_path(row, &["contact_name"]),
        str_path(row, &["role"]),
        str_path(row, &["linkedin_url"]),
        str_path(row, &["email"]),
    ])
    .is_empty()
}

fn append_knowledge_rows(
    root: &Path,
    domain: &str,
    key: &str,
    rows: &[Value],
) -> anyhow::Result<()> {
    let payload = crate::knowledge::dispatch_capturing(
        root,
        &[
            "data".to_string(),
            "append".to_string(),
            "--domain".to_string(),
            domain.to_string(),
            "--key".to_string(),
            key.to_string(),
            "--rows".to_string(),
            serde_json::to_string(rows)?,
        ],
    )?;
    anyhow::ensure!(
        payload.get("ok").and_then(Value::as_bool) == Some(true),
        "knowledge data append failed: {}",
        payload
    );
    Ok(())
}

fn append_run_status(
    root: &Path,
    refs: &OutboundKnowledgeRefs,
    command_id: &str,
    command: &BusinessCommand,
    run_type: &str,
    status: &str,
    now_ms: i64,
) -> anyhow::Result<()> {
    if refs.runs_key.is_empty() {
        return Ok(());
    }
    let run_id = first_nonempty(&[
        str_path(&command.payload, &["run_id"]),
        command_id.to_string(),
    ]);
    append_knowledge_rows(
        root,
        &refs.domain,
        &refs.runs_key,
        &[serde_json::json!({
            "run_id": run_id,
            "command_id": command_id,
            "campaign_id": refs.campaign_id,
            "runbook_id": refs.runbook_id,
            "record_id": command.record_id.clone().unwrap_or_default(),
            "company_id": str_path(&command.client_context, &["company_id"]),
            "pipeline_id": str_path(&command.client_context, &["pipeline_id"]),
            "run_type": run_type,
            "status": status,
            "ctox_status": status,
            "updated_at_ms": now_ms,
            "created_at_ms": now_ms,
        })],
    )
}

fn string_from_map(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn str_or_none(value: &Value, path: &[&str]) -> Option<String> {
    let value = str_path(value, path);
    (!value.is_empty()).then_some(value)
}

fn country_is_germany(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "de" | "deu" | "germany" | "deutschland" | "bundesrepublik deutschland"
    )
}

fn str_path(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return String::new();
        };
        current = next;
    }
    current.as_str().unwrap_or("").trim().to_string()
}

fn first_nonempty(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn decode_file_payload(file: &Value) -> anyhow::Result<Vec<u8>> {
    let raw = first_nonempty(&[
        str_path(file, &["base64"]),
        str_path(file, &["data_base64"]),
        str_path(file, &["data"]),
    ]);
    let encoded = if raw.trim().is_empty() {
        let data_url = str_path(file, &["data_url"]);
        data_url
            .split_once(',')
            .map(|(_, value)| value.to_string())
            .unwrap_or(data_url)
    } else {
        raw
    };
    let compact = encoded
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    anyhow::ensure!(!compact.is_empty(), "file payload is empty");
    BASE64_STANDARD
        .decode(compact)
        .context("failed to decode base64 file payload")
}

fn load_collection_payload(conn: &Connection, collection: &str, record_id: &str) -> Option<Value> {
    conn.query_row(
        "SELECT payload_json FROM business_records WHERE collection = ?1 AND record_id = ?2",
        rusqlite::params![collection, record_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|json| serde_json::from_str(&json).ok())
}

fn matching_requirement_text(requirement: &Value) -> String {
    let mut parts = Vec::new();
    push_value(&mut parts, "Titel", requirement.get("title"));
    push_value(&mut parts, "Quelle", requirement.get("sourceName"));
    push_value(&mut parts, "Ort", requirement.get("location"));
    push_value(&mut parts, "Rolle", requirement.get("aboutRole"));
    push_value(
        &mut parts,
        "Anforderungen",
        requirement.get("objectRequirements"),
    );
    push_value(&mut parts, "Aufgaben", requirement.get("responsibilities"));
    push_value(&mut parts, "Benefits", requirement.get("benefits"));
    push_value(&mut parts, "Rohtext", requirement.get("rawText"));
    parts.join("\n")
}

fn matching_object_text(object: &Value) -> String {
    let mut parts = Vec::new();
    push_value(&mut parts, "Name", object.get("name"));
    push_value(&mut parts, "Aktuelle Rolle", object.get("currentRole"));
    push_value(&mut parts, "Ziel", object.get("desiredPosition"));
    push_value(&mut parts, "Abschluss", object.get("highestDegree"));
    push_value(&mut parts, "Region", object.get("region"));
    push_value(&mut parts, "Sprachen", object.get("languages"));
    push_value(&mut parts, "Skills", object.get("skills"));
    push_value(&mut parts, "Zusammenfassung", object.get("executiveInfo"));
    push_value(&mut parts, "Rohtext", object.get("rawText"));
    parts.join("\n")
}

fn job_text(job: &Value) -> String {
    let mut parts = Vec::new();
    push_value(&mut parts, "Titel", job.get("title"));
    push_value(&mut parts, "Unternehmen", job.get("companyName"));
    push_value(&mut parts, "Ort", job.get("location"));
    push_value(&mut parts, "Rolle", job.get("aboutRole"));
    push_value(
        &mut parts,
        "Anforderungen",
        job.get("candidateRequirements"),
    );
    push_value(&mut parts, "Benefits", job.get("benefits"));
    normalize_lines(&parts.join("\n"))
}

fn candidate_text(candidate: &Value) -> String {
    let mut parts = Vec::new();
    push_value(&mut parts, "Name", candidate.get("name"));
    push_value(&mut parts, "Aktuelle Rolle", candidate.get("currentRole"));
    push_value(&mut parts, "Zielposition", candidate.get("desiredPosition"));
    push_value(&mut parts, "Abschluss", candidate.get("highestDegree"));
    push_value(&mut parts, "Region", candidate.get("region"));
    push_value(&mut parts, "Skills", candidate.get("skills"));
    if let Some(raw) = candidate
        .get("documents")
        .and_then(Value::as_array)
        .and_then(|docs| docs.first())
        .and_then(|doc| doc.get("meta"))
        .and_then(|meta| meta.get("rawText"))
    {
        push_value(&mut parts, "CV", Some(raw));
    }
    normalize_lines(&parts.join("\n"))
}

fn push_value(parts: &mut Vec<String>, label: &str, value: Option<&Value>) {
    let Some(value) = value else {
        return;
    };
    let text = match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    };
    let text = clean_text(&text);
    if !text.is_empty() {
        parts.push(format!("{label}: {text}"));
    }
}

fn build_match_items(
    company_id: &str,
    job_id: &str,
    candidate_id: &str,
    job_text: &str,
    cv_text: &str,
    now_iso: &str,
) -> Vec<Value> {
    let dimensions = [
        ("REQ-1", "Fachliche Qualifikation", "skill", "base"),
        ("REQ-2", "Berufserfahrung", "experience", "performance"),
        ("REQ-3", "Ausbildung / Abschluss", "education", "base"),
        (
            "REQ-4",
            "Sprache und Kommunikation",
            "language",
            "performance",
        ),
        (
            "REQ-5",
            "Motivation und Arbeitsweise",
            "other",
            "enthusiasm",
        ),
    ];
    dimensions
        .iter()
        .map(|(req_id, title, dimension, priority)| {
            let score = dimension_score(dimension, job_text, cv_text);
            let match_level = if score >= 0.78 {
                "full"
            } else if score >= 0.42 {
                "partial"
            } else {
                "none"
            };
            let priority_key = match *priority {
                "base" => 2,
                "performance" => 1,
                _ => 0,
            };
            let match_level_key = match match_level {
                "full" => 2,
                "partial" => 1,
                _ => 0,
            };
            let match_score_key = (score * 100.0).round() as i64;
            serde_json::json!({
                "id": format!("{company_id}|{job_id}|{candidate_id}|{req_id}"),
                "companyId": company_id,
                "jobId": job_id,
                "candidateId": candidate_id,
                "requirementId": req_id,
                "title": title,
                "dimension": dimension,
                "priority": priority,
                "matchLevel": match_level,
                "matchScore": score,
                "jobSnippet": snippet_for_dimension(job_text, dimension),
                "cvSnippet": snippet_for_dimension(cv_text, dimension),
                "explanation": explanation_for_score(title, score),
                "createdAt": now_iso,
                "updatedAt": now_iso,
                "priorityKey": priority_key,
                "matchLevelKey": match_level_key,
                "matchScoreKey": match_score_key
            })
        })
        .collect()
}

fn dimension_score(dimension: &str, job_text: &str, cv_text: &str) -> f64 {
    let keywords = match dimension {
        "skill" => &[
            "gebäudeautomation",
            "automatisierung",
            "cad",
            "catia",
            "sap",
            "test",
            "validierung",
            "python",
            "sql",
            "elektro",
            "maschinenbau",
        ][..],
        "experience" => &[
            "erfahrung",
            "projekt",
            "service",
            "sonderanlagenbau",
            "automotive",
            "technik",
            "kunden",
            "leitung",
        ][..],
        "education" => &[
            "studium",
            "bachelor",
            "master",
            "techniker",
            "ausbildung",
            "ingenieur",
            "maschinenbau",
            "elektrotechnik",
        ][..],
        "language" => &["deutsch", "englisch", "kommunikation", "international"][..],
        _ => &[
            "team",
            "selbstständig",
            "flexibel",
            "qualität",
            "kundenorientiert",
            "motivation",
        ][..],
    };
    let job_lower = job_text.to_lowercase();
    let cv_lower = cv_text.to_lowercase();
    let required = keywords
        .iter()
        .filter(|keyword| job_lower.contains(&keyword.to_lowercase()))
        .copied()
        .collect::<Vec<_>>();
    if required.is_empty() {
        // None of this dimension's curated keywords appear in the posting, so the
        // hardcoded vocabulary is irrelevant here (e.g. an off-domain role).
        // Scoring the CV against those irrelevant terms previously pinned every
        // such candidate to a constant ~0.28 floor; fall back to direct
        // requirement<->CV salient-term overlap so off-domain roles get a real
        // signal instead.
        return content_overlap_score(job_text, cv_text);
    }
    let hits = required
        .iter()
        .filter(|keyword| cv_lower.contains(&keyword.to_lowercase()))
        .count();
    let ratio = hits as f64 / required.len() as f64;
    (0.28 + ratio * 0.68).clamp(0.0, 0.96)
}

/// Domain-agnostic salient-term overlap: the fraction of the requirement (job)
/// text's salient terms that also occur in the CV text. Used as the fallback in
/// [`dimension_score`] when a dimension's curated keyword vocabulary does not
/// apply to the posting, so off-domain roles are scored on real content overlap
/// rather than against an irrelevant hardcoded list.
fn content_overlap_score(job_text: &str, cv_text: &str) -> f64 {
    let job_terms = salient_terms(job_text);
    if job_terms.is_empty() {
        return 0.1;
    }
    let cv_terms = salient_terms(cv_text);
    let overlap = job_terms
        .iter()
        .filter(|term| cv_terms.contains(*term))
        .count();
    let ratio = overlap as f64 / job_terms.len() as f64;
    (0.10 + ratio * 0.80).clamp(0.0, 0.96)
}

/// Lower-cased word tokens of length >= 4 that are not common German/English
/// stopwords. Deliberately simple and dependency-free.
fn salient_terms(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|word| {
            let lowered = word.to_lowercase();
            if lowered.chars().count() >= 4 && !is_match_stopword(&lowered) {
                Some(lowered)
            } else {
                None
            }
        })
        .collect()
}

fn is_match_stopword(word: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "und", "oder", "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem",
        "einer", "eines", "mit", "für", "von", "vom", "zur", "zum", "auf", "aus", "bei", "nach",
        "über", "unter", "sowie", "sind", "wird", "werden", "haben", "sich", "auch", "nicht",
        "dass", "ihre", "ihrer", "unsere", "unser", "diese", "dieser", "dieses", "sowohl", "and",
        "the", "for", "with", "from", "that", "this", "your", "you", "our", "are", "will", "have",
        "should", "must", "they", "their", "into", "such", "able", "well",
    ];
    STOPWORDS.contains(&word)
}

fn snippet_for_dimension(text: &str, dimension: &str) -> String {
    let needles = match dimension {
        "skill" => &["Skills", "Anforderungen", "Kompetenz", "Kenntnisse"][..],
        "experience" => &["Erfahrung", "Berufserfahrung", "Rolle", "Projekt"][..],
        "education" => &["Studium", "Bachelor", "Master", "Ausbildung", "Abschluss"][..],
        "language" => &["Deutsch", "Englisch", "Sprache", "Kommunikation"][..],
        _ => &[
            "Team",
            "selbstständig",
            "flexibel",
            "Qualität",
            "Arbeitsweise",
        ][..],
    };
    for line in text.lines() {
        let lower = line.to_lowercase();
        if needles
            .iter()
            .any(|needle| lower.contains(&needle.to_lowercase()))
        {
            return truncate_chars(line, 240);
        }
    }
    truncate_chars(text, 240)
}

fn explanation_for_score(title: &str, score: f64) -> String {
    if score >= 0.78 {
        format!("{title}: starke Übereinstimmung zwischen Anforderung und CV-Evidenz.")
    } else if score >= 0.42 {
        format!("{title}: teilweise passende Evidenz vorhanden; Details sollten geprüft werden.")
    } else {
        format!("{title}: aktuell nur schwache oder indirekte Evidenz im CV.")
    }
}

fn total_match_score(items: &[Value]) -> i64 {
    if items.is_empty() {
        return 0;
    }
    let sum = items
        .iter()
        .filter_map(|item| item.get("matchScore").and_then(Value::as_f64))
        .sum::<f64>();
    ((sum / items.len() as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as i64
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn regex_capture(text: &str, pattern: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|captures| captures.get(1).or_else(|| captures.get(0)))
        .map(|m| clean_text(m.as_str()))
        .filter(|value| !value.is_empty())
}

fn normalize_lines(text: &str) -> String {
    text.lines()
        .map(clean_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean_text(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn slug(value: &str) -> String {
    let ascii = value
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
        .replace('ß', "ss");
    let mut out = String::new();
    let mut last_dash = false;
    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        format!("record-{}", short_hash(value))
    } else {
        out.chars().take(96).collect()
    }
}

fn infer_company_from_url(url: &str) -> String {
    if url.contains("bosch") {
        "Bosch".to_string()
    } else if url.contains("kaercher") || url.contains("karcher") {
        "Alfred Kärcher SE & Co. KG".to_string()
    } else {
        origin_url(url).unwrap_or_else(|| "Unbekanntes Unternehmen".to_string())
    }
}

fn origin_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default()
    ))
}

fn write_import_artifact(
    root: &Path,
    command_id: &str,
    filename: &str,
    value: &Value,
) -> anyhow::Result<()> {
    let dir = root
        .join("runtime")
        .join("business-os-imports")
        .join(command_id);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(filename), serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn short_hash(value: &str) -> String {
    let digest = sha2::Sha256::digest(value.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &digest)[..10]
        .to_string()
}

#[cfg(test)]
mod outbound_provenance_tests {
    use super::outbound_contact_subject_key;
    use serde_json::json;

    #[test]
    fn subject_key_is_stable_and_email_first() {
        let a = json!({"contact_name": "Anna Müller", "email": "Anna.Mueller@ACME.de"});
        let b = json!({"contact_name": "different name", "email": "anna.mueller@acme.de"});
        // Same email (case-insensitive) → same subject key regardless of name.
        assert_eq!(
            outbound_contact_subject_key(&a, "ACME GmbH"),
            outbound_contact_subject_key(&b, "Other GmbH")
        );
        // Falls back to name+company when no email; different company → different key.
        let no_email = json!({"contact_name": "Anna Müller"});
        assert_ne!(
            outbound_contact_subject_key(&no_email, "ACME GmbH"),
            outbound_contact_subject_key(&no_email, "Beta GmbH")
        );
        assert!(outbound_contact_subject_key(&a, "ACME GmbH").starts_with("subj_"));
    }
}

#[cfg(test)]
mod ssrf_guard_tests {
    use super::is_public_ip;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }
    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn blocks_non_public_v4() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
        ] {
            assert!(!is_public_ip(v4(ip)), "{ip} must be blocked");
        }
    }

    #[test]
    fn allows_public_v4() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34"] {
            assert!(is_public_ip(v4(ip)), "{ip} must be allowed");
        }
    }

    #[test]
    fn blocks_non_public_v6_and_mapped() {
        assert!(!is_public_ip(v6("::1")));
        assert!(!is_public_ip(v6("fe80::1")));
        assert!(!is_public_ip(v6("fc00::1")));
        assert!(
            !is_public_ip(v6("::ffff:10.0.0.1")),
            "IPv4-mapped private must be blocked"
        );
        assert!(
            is_public_ip(v6("2606:4700:4700::1111")),
            "public v6 must be allowed"
        );
    }
}

#[cfg(test)]
mod ocr_response_tests {
    use super::extract_responses_output_text;
    use serde_json::json;

    #[test]
    fn extracts_flat_output_text() {
        let payload = json!({ "output_text": "hello world" });
        assert_eq!(
            extract_responses_output_text(&payload).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn extracts_structured_output_text() {
        let payload = json!({
            "output": [{ "content": [{ "type": "output_text", "text": "page text" }] }]
        });
        assert_eq!(
            extract_responses_output_text(&payload).as_deref(),
            Some("page text")
        );
    }

    #[test]
    fn returns_none_when_no_text_present() {
        assert!(extract_responses_output_text(&json!({ "foo": 1 })).is_none());
        assert!(extract_responses_output_text(&json!({ "output_text": "   " })).is_none());
    }
}

#[cfg(test)]
mod matching_score_tests {
    use super::{content_overlap_score, dimension_score};

    #[test]
    fn in_domain_posting_keeps_keyword_path() {
        // The posting mentions curated skill keywords and the CV matches them,
        // so the unchanged keyword path yields a high score.
        let job = "Wir suchen Erfahrung mit Python und SQL sowie CAD.";
        let cv = "Python, SQL und CAD im Projekt eingesetzt.";
        let score = dimension_score("skill", job, cv);
        assert!(score > 0.9, "expected high in-domain score, got {score}");
    }

    #[test]
    fn off_domain_posting_uses_real_content_overlap_not_a_constant_floor() {
        // A nursing posting matches none of the engineering keywords, so the
        // score must reflect actual requirement<->CV overlap rather than the old
        // constant ~0.28 floor.
        let job =
            "Pflegefachkraft für die geriatrische Station, Wundversorgung und Medikamentengabe.";
        let cv_match = "Examinierte Pflegefachkraft, Wundversorgung und Medikamentengabe auf der geriatrischen Station.";
        let cv_unrelated = "Softwareentwickler mit Schwerpunkt verteilte Systeme und Datenbanken.";

        let matched = dimension_score("skill", job, cv_match);
        let unrelated = dimension_score("skill", job, cv_unrelated);

        assert!(
            matched > unrelated,
            "matching CV ({matched}) must outscore unrelated CV ({unrelated})"
        );
        assert!(
            matched > 0.5,
            "strong overlap should score well, got {matched}"
        );
        assert!(
            unrelated < 0.28,
            "unrelated off-domain CV must not collapse onto the old 0.28 floor, got {unrelated}"
        );
    }

    #[test]
    fn content_overlap_score_is_bounded_and_handles_empty_input() {
        assert!((content_overlap_score("", "anything") - 0.1).abs() < 1e-9);
        let full = content_overlap_score("alpha bravo charlie", "alpha bravo charlie delta");
        assert!(
            full > 0.8 && full <= 0.96,
            "full overlap should be high, got {full}"
        );
    }
}
