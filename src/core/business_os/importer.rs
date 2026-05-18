use super::store::{upsert_business_record, BusinessCommand};
use crate::mission::channels;
use anyhow::{anyhow, Context};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use regex::Regex;
use rusqlite::Connection;
use scraper::{Html, Selector};
use serde_json::Value;
use sha2::Digest;
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
        let response = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0 CTOX Business OS Importer")
            .timeout(Duration::from_secs(30))
            .call()
            .with_context(|| format!("failed to fetch {url}"))?;
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
        let raw_text = parse_pdf_text(&bytes).unwrap_or_else(|err| {
            eprintln!("[business-os-import] PDF parse failed for {name}: {err:#}");
            String::new()
        });
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

    let response = ureq::get(&url)
        .set("User-Agent", "Mozilla/5.0 CTOX Business OS Importer")
        .call()
        .with_context(|| format!("failed to fetch {url}"))?;
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
        let raw_text = parse_pdf_text(&bytes).unwrap_or_else(|err| {
            eprintln!("[business-os-import] PDF parse failed for {name}: {err:#}");
            String::new()
        });
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
        .enumerate()
        .map(|(idx, (req_id, title, dimension, priority))| {
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
                "matchScoreKey": match_score_key + idx as i64 - idx as i64
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
    let basis = if required.is_empty() {
        keywords.to_vec()
    } else {
        required
    };
    let hits = basis
        .iter()
        .filter(|keyword| cv_lower.contains(&keyword.to_lowercase()))
        .count();
    let ratio = if basis.is_empty() {
        0.0
    } else {
        hits as f64 / basis.len() as f64
    };
    (0.28 + ratio * 0.68).clamp(0.0, 0.96)
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
