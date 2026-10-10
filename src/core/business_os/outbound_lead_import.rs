//! Native "import leads and start their research" for Outbound Lead Generation.
//!
//! The Outbound app imports spreadsheets and starts each lead's research from
//! the browser. A CTOX agent answering a mail ("bitte die Firmen aus der Excel
//! recherchieren", 10.10.2026) had no way to do either: it read the Excel,
//! promised the research, and the completion review rightly rejected the
//! unbacked promise. This action gives the agent the same two steps the app
//! uses: leads in a named campaign, and per lead the research chat task the
//! app's "Recherchieren" button submits. The research policy (instructions,
//! field set, sources, writeback contract) is taken from the newest research
//! task the app started, so the app stays the single place where it is
//! maintained.

use anyhow::Context;
use serde_json::{json, Value};
use std::path::Path;

use super::store;

pub(super) const MODULE_ID: &str = "outbound-lead-generation";
pub(super) const IMPORT_AND_RESEARCH_ACTION: &str = "outbound.leads.import_and_research";
const LEADS: &str = "outbound_lead_generation_leads";
const IMPORTS: &str = "outbound_lead_generation_imports";
const MAX_ROWS: usize = 200;
const MAX_FIELD_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportRow {
    pub name: String,
    pub website: String,
    pub city: String,
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportRequest {
    pub campaign: String,
    pub rows: Vec<ImportRow>,
    pub start_research: bool,
    pub source_note: String,
}

fn clean(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|text| text.chars().take(MAX_FIELD_CHARS).collect::<String>())
        .unwrap_or_default()
}

fn normalized_country(raw: &str) -> String {
    match raw.trim().to_ascii_uppercase().as_str() {
        "AT" | "AUT" | "ÖSTERREICH" | "OESTERREICH" | "AUSTRIA" => "AT".to_string(),
        "CH" | "CHE" | "SCHWEIZ" | "SWITZERLAND" => "CH".to_string(),
        _ => "DE".to_string(),
    }
}

fn domain_of(website: &str) -> String {
    let lowered = website.trim().to_ascii_lowercase();
    let without_scheme = lowered
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    without_scheme
        .trim_start_matches("www.")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `payload: { campaign, rows: [{ name, website?, city?, country? }], start_research?, source_note? }`
pub(super) fn parse_request(arguments: &Value) -> anyhow::Result<ImportRequest> {
    let payload = arguments
        .get("payload")
        .filter(|value| value.is_object())
        .context("payload object is required")?;
    let campaign = clean(payload.get("campaign"));
    anyhow::ensure!(!campaign.is_empty(), "payload.campaign is required");
    let rows = payload
        .get("rows")
        .and_then(Value::as_array)
        .context("payload.rows must be a list of companies")?;
    anyhow::ensure!(!rows.is_empty(), "payload.rows is empty");
    anyhow::ensure!(
        rows.len() <= MAX_ROWS,
        "payload.rows has {} entries; at most {MAX_ROWS} per call",
        rows.len()
    );
    let mut parsed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        let name = clean(row.get("name").or_else(|| row.get("company")));
        anyhow::ensure!(!name.is_empty(), "payload.rows[{index}].name is required");
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        parsed.push(ImportRow {
            name,
            website: clean(row.get("website").or_else(|| row.get("domain"))),
            city: clean(row.get("city").or_else(|| row.get("ort"))),
            country: normalized_country(&clean(row.get("country").or_else(|| row.get("land")))),
        });
    }
    Ok(ImportRequest {
        campaign,
        rows: parsed,
        start_research: payload
            .get("start_research")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        source_note: clean(payload.get("source_note")),
    })
}

fn short_hash(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .map(|part| part.trim().to_lowercase())
        .collect::<Vec<_>>()
        .join("\u{0}");
    super::hashing::hex_sha256(joined.as_bytes())[..16].to_string()
}

/// Same campaign and company name give the same lead: a retried mail turn
/// must not import or research a company twice.
pub(super) fn lead_id_for(campaign: &str, name: &str) -> String {
    format!("lead_mail_{}", short_hash(&[campaign, name]))
}

fn import_id_for(campaign: &str) -> String {
    format!("import_mail_{}", short_hash(&[campaign]))
}

pub(super) fn lead_document(
    request: &ImportRequest,
    row: &ImportRow,
    index: usize,
    now_ms: i64,
) -> Value {
    let herkunft = if request.source_note.is_empty() {
        format!("Übernahme durch CTOX in „{}“", request.campaign)
    } else {
        format!(
            "Übernahme durch CTOX in „{}“ ({})",
            request.campaign, request.source_note
        )
    };
    json!({
        "id": lead_id_for(&request.campaign, &row.name),
        "import_id": import_id_for(&request.campaign),
        "campaign": request.campaign,
        "name": row.name,
        "domain": domain_of(&row.website),
        "website": row.website,
        "city": row.city,
        "country": row.country,
        "research_status": "new",
        "validation_status": "pending",
        "sellify_status": "not_started",
        "task_id": "",
        "command_id": "",
        "selected_contact_ids": [],
        "data": {
            "herkunft_import": herkunft,
            "statistische_kampagne": request.campaign,
        },
        "payload": {
            "imported_row": {
                "__rowIndex": index + 1,
                "unternehmen": row.name,
                "website": row.website,
                "ort": row.city,
                "land": row.country,
            },
            "import_source": "ctox_action",
            "min_independent_sources": 1,
        },
        "created_at_ms": now_ms,
        "updated_at_ms": now_ms,
    })
}

const TEMPLATE_DROP_FIELDS: &[&str] = &[
    "known_person_records",
    "sellify_company",
    "crm_knowledge",
    "campaign_run_id",
    "workflow_id",
    "record_snapshot",
];

/// The research task for one imported lead, built from the newest research
/// task the app started (its policy, sources, fields and writeback contract)
/// with every lead-specific value replaced. Sellify knowledge of the template
/// lead is dropped: the research skill checks Sellify itself first.
pub(super) fn research_command(template_payload: &Value, lead: &Value, command_id: &str) -> Value {
    let mut payload = template_payload.clone();
    let lead_id = lead["id"].as_str().unwrap_or_default();
    let name = lead["name"].as_str().unwrap_or_default();
    if let Some(object) = payload.as_object_mut() {
        for field in TEMPLATE_DROP_FIELDS {
            object.remove(*field);
        }
    }
    let adapter_note = template_payload["prompt"]
        .as_str()
        .and_then(|prompt| prompt.split_once("). "))
        .map(|(_, tail)| tail.trim())
        .filter(|tail| tail.contains("Adapter"))
        .map(|tail| format!(" {tail}"))
        .unwrap_or_default();
    let prompt = format!(
        "Starte eine Outbound Neurecherche für {name} [{lead_id}] (Auftrag {command_id}).{adapter_note}"
    );
    payload["lead_id"] = json!(lead_id);
    payload["lead_snapshot"] = lead.clone();
    payload["company"] = json!(name);
    payload["country"] = lead["country"].clone();
    payload["mode"] = json!("new_record");
    payload["title"] = json!(format!("Neurecherche: {name}"));
    payload["instruction"] = json!(prompt);
    payload["prompt"] = json!(prompt);
    payload["user_message"] = json!(prompt);
    payload["thread_key"] = json!(format!(
        "business-os/outbound-lead-generation/lead/{lead_id}"
    ));
    if payload["writeback_contract"].is_object() {
        payload["writeback_contract"]["record_ids"] = json!([lead_id]);
    }
    payload
}

pub(super) fn research_command_id(lead_id: &str) -> String {
    format!(
        "leadgen-lead-research-{}",
        short_hash(&[lead_id, "research"])
    )
}

/// Writes the campaign's import record and leads, then submits each new
/// lead's research task with the caller's client context (actor, channel).
pub(super) fn import_and_research(
    root: &Path,
    request: &ImportRequest,
    client_context: &Value,
) -> anyhow::Result<Value> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    let template = if request.start_research {
        Some(store::latest_outbound_lead_research_payload(root)?.context(
            "no Outbound research task has been started in the app yet; its research policy is the template. Start one lead's research in the Outbound app once, then repeat.",
        )?)
    } else {
        None
    };
    let import_id = import_id_for(&request.campaign);
    store::upsert_external_projection_record(
        root,
        IMPORTS,
        &import_id,
        now_ms,
        json!({
            "id": import_id,
            "title": request.campaign,
            "source_type": "ctox_action",
            "status": "imported",
            "lead_count": request.rows.len(),
            "payload": {
                "source_note": request.source_note,
                "file_names": [],
                "invalid_rows": [],
                "duplicate_rows": [],
                "secret_value_in_payload": false,
            },
            "created_at_ms": now_ms,
            "updated_at_ms": now_ms,
        }),
    )?;
    let mut results = Vec::new();
    for (index, row) in request.rows.iter().enumerate() {
        let lead_id = lead_id_for(&request.campaign, &row.name);
        let existing = store::read_rxdb_collection_record(root, LEADS, &lead_id)?;
        let lead = match existing.clone() {
            Some(lead) => lead,
            None => {
                let lead = lead_document(request, row, index, now_ms);
                store::upsert_external_projection_record(
                    root,
                    LEADS,
                    &lead_id,
                    now_ms,
                    lead.clone(),
                )?;
                lead
            }
        };
        let existing_command = lead["command_id"].as_str().unwrap_or_default().to_string();
        let mut entry = json!({
            "lead_id": lead_id,
            "name": row.name,
            "created": existing.is_none(),
        });
        match (&template, existing_command.is_empty()) {
            (Some(template), true) => {
                let command_id = research_command_id(&lead_id);
                let document = json!({
                    "id": command_id,
                    "command_id": command_id,
                    "module": MODULE_ID,
                    "command_type": "business_os.chat.task",
                    "record_id": lead_id,
                    "payload": research_command(template, &lead, &command_id),
                    "client_context": client_context,
                });
                let outcome = store::accept_rxdb_business_command(root, document)?;
                let status = outcome["status"].as_str().unwrap_or("accepted").to_string();
                let task_id = outcome["task_id"].as_str().unwrap_or_default().to_string();
                store::upsert_external_projection_record(
                    root,
                    LEADS,
                    &lead_id,
                    now_ms,
                    json!({
                        "command_id": command_id,
                        "task_id": task_id,
                        "research_status": "queued",
                        "payload": { "research_queued_at_ms": now_ms },
                        "updated_at_ms": now_ms,
                    }),
                )?;
                entry["research"] = json!({ "command_id": command_id, "status": status });
            }
            (Some(_), false) => {
                entry["research"] =
                    json!({ "command_id": existing_command, "status": "already_started" });
            }
            (None, _) => {
                entry["research"] = json!({ "status": "not_requested" });
            }
        }
        results.push(entry);
    }
    Ok(json!({
        "ok": true,
        "module_id": MODULE_ID,
        "action_id": IMPORT_AND_RESEARCH_ACTION,
        "campaign": request.campaign,
        "import_id": import_id,
        "leads": results,
        "note": "Research runs as Outbound research tasks; each finished task writes its result to the lead. Report progress or results only from the lead records and command status.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ImportRequest {
        parse_request(&json!({
            "payload": {
                "campaign": "Recherche aus Mail 10.10.",
                "rows": [
                    { "name": "Hedinger GmbH & Co. KG", "website": "https://www.hedinger.de/", "city": "Stuttgart", "country": "DE" },
                    { "name": "Carl Roth GmbH + Co. KG", "website": "carlroth.com", "ort": "Karlsruhe" },
                    { "name": "hedinger gmbh & co. kg" }
                ],
                "source_note": "Mail vom 09.10."
            }
        }))
        .expect("valid request")
    }

    #[test]
    fn rows_are_cleaned_deduplicated_and_ids_are_stable() {
        let request = request();
        assert_eq!(request.rows.len(), 2, "same company once per campaign");
        assert!(request.start_research);
        assert_eq!(request.rows[1].city, "Karlsruhe");
        assert_eq!(request.rows[1].country, "DE");
        let lead = lead_document(&request, &request.rows[0], 0, 1);
        assert_eq!(lead["domain"], "hedinger.de");
        assert_eq!(lead["campaign"], "Recherche aus Mail 10.10.");
        assert_eq!(
            lead["id"],
            json!(lead_id_for(
                "Recherche aus Mail 10.10.",
                "HEDINGER GmbH & Co. KG"
            ))
        );
        assert!(parse_request(&json!({ "payload": { "campaign": "x", "rows": [] } })).is_err());
        assert!(parse_request(&json!({ "payload": { "rows": [{ "name": "a" }] } })).is_err());
    }

    #[test]
    fn research_task_reuses_the_app_policy_but_names_only_the_imported_lead() {
        let template = json!({
            "lead_id": "lead_old",
            "company": "Syensqo Specialty Polymers Germany GmbH",
            "mode": "update_firm",
            "fields": ["firma_name", "umsatz"],
            "source_policy": { "skill": "outbound-lead-generation-research" },
            "research_instructions": "0. Zuerst Sellify prüfen.",
            "known_person_records": [{ "person_nachname": "Bikard" }],
            "sellify_company": { "contact_id": "17854" },
            "crm_knowledge": "BEKANNT AUS DEM EIGENEN CRM",
            "campaign_run_id": "run-1",
            "workflow_id": "run-1",
            "required_skills": ["outbound-lead-generation-research"],
            "writeback_contract": { "command_type": "outbound.lead.research_writeback", "record_ids": ["lead_old"] },
            "prompt": "Starte eine Outbound Nachrecherche für Syensqo [lead_old] (Auftrag c-old). Die registrierten Adapter aus source_policy laufen zuerst."
        });
        let request = request();
        let lead = lead_document(&request, &request.rows[0], 0, 1);
        let lead_id = lead["id"].as_str().unwrap().to_string();
        let command_id = research_command_id(&lead_id);
        let payload = research_command(&template, &lead, &command_id);
        assert_eq!(payload["lead_id"], json!(lead_id));
        assert_eq!(payload["company"], "Hedinger GmbH & Co. KG");
        assert_eq!(payload["mode"], "new_record");
        assert_eq!(payload["fields"], template["fields"]);
        assert_eq!(payload["source_policy"], template["source_policy"]);
        assert_eq!(
            payload["writeback_contract"]["record_ids"],
            json!([lead_id])
        );
        assert_eq!(
            payload["writeback_contract"]["command_type"],
            "outbound.lead.research_writeback"
        );
        for dropped in TEMPLATE_DROP_FIELDS {
            assert!(payload.get(*dropped).is_none(), "{dropped} must not leak");
        }
        let prompt = payload["prompt"].as_str().unwrap();
        assert!(prompt.starts_with("Starte eine Outbound Neurecherche für Hedinger GmbH & Co. KG"));
        assert!(prompt.contains(&command_id));
        assert!(prompt.contains("registrierten Adapter"));
        assert!(!prompt.contains("Syensqo"));
        assert!(!serde_json::to_string(&payload)
            .unwrap()
            .contains("lead_old"));
    }
}
