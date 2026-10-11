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
    /// Inbound mail the import answers. Mail turns read back exactly the
    /// records carrying their own mail's key (`records_for_source_message`).
    pub source_message_key: String,
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
        source_message_key: {
            let key = clean(payload.get("source_message_key"));
            anyhow::ensure!(
                key.is_empty() || key.starts_with("email:"),
                "payload.source_message_key must be an inbound mail key"
            );
            key
        },
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
        "contacts": [],
        "selected_contact_ids": [],
        "evidence": [],
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
            "source_message_key": request.source_message_key,
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
                "source_message_key": request.source_message_key,
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

/// Leads an import created for exactly this inbound mail. A mail turn may
/// state work and results only from these records; it never searches other
/// records (thesen 10.10.2026: the review held a reply whose "research runs"
/// and "verified" facts had no record the mail turn could read).
pub(crate) fn records_for_source_message(
    root: &std::path::Path,
    message_key: &str,
) -> anyhow::Result<Vec<Value>> {
    let message_key = message_key.trim();
    if !message_key.starts_with("email:") {
        return Ok(Vec::new());
    }
    store::outbound_leads_for_source_message(root, message_key)
}

const MAIL_RECORD_TEXT_LIMIT: usize = 6000;

/// Whether the record's research run has ended with its results written
/// back. The app parks such a lead in `needs_review` until a person checks
/// it; read raw, that status made the mail agent answer "die Recherche laeuft
/// noch" while both leads held their results (thesen, 11.10.2026). Ended is
/// not complete: on thesen every such lead also has fields without a match or
/// still open, which `field_summary_for_mail` names.
pub(crate) fn research_finished(record: &Value) -> bool {
    matches!(
        record["research_status"].as_str().unwrap_or(""),
        "needs_review" | "done" | "completed" | "researched" | "validated"
    )
}

fn research_state_for_mail(record: &Value) -> String {
    let status = record["research_status"].as_str().unwrap_or("-");
    if research_finished(record) {
        return format!(
            "Lauf beendet, Ergebnisse unten; {}",
            field_summary_for_mail(record)
        );
    }
    match status {
        "new" => "nicht gestartet".to_string(),
        "queued" | "running" | "researching" | "in_progress" => "laeuft noch".to_string(),
        "failed" => "fehlgeschlagen".to_string(),
        other => format!("Status {other}"),
    }
}

/// Counts of the research's field states and the fields still open, from the
/// lead's `field_status` (verified / no_match / action_required).
fn field_summary_for_mail(record: &Value) -> String {
    let Some(fields) = record["field_status"].as_object() else {
        return "Feldstatus unbekannt".to_string();
    };
    let (mut verified, mut no_match, mut open) = (0, 0, Vec::new());
    for (key, state) in fields {
        match state["status"].as_str().unwrap_or("") {
            "verified" | "ok" => verified += 1,
            "no_match" | "unsupported" => no_match += 1,
            _ => open.push(key.as_str()),
        }
    }
    let open_text = if open.is_empty() {
        String::new()
    } else {
        format!(" ({})", open.join(", "))
    };
    format!(
        "{verified} Felder belegt, {no_match} ohne Treffer, {} offen{open_text}",
        open.len()
    )
}

fn validation_state_for_mail(record: &Value) -> String {
    match record["validation_status"].as_str().unwrap_or("-") {
        "pending" => "Sichtpruefung durch das Team offen".to_string(),
        "validated" => "vom Team geprueft".to_string(),
        other => format!("Status {other}"),
    }
}

/// Order of the order-record fields in the mail block: what a reply names
/// first. Unknown fields keep their research order after these.
fn mail_field_rank(field: &str) -> usize {
    const ORDER: [&str; 16] = [
        "firma_name",
        "firma_geschaeftsfuehrung",
        "umsatz",
        "mitarbeiter",
        "firma_besucheranschrift",
        "firma_anschrift",
        "firma_plz",
        "firma_ort",
        "firma_telefon",
        "firma_email",
        "firma_domain",
        "firma_geschaeftstaetigkeit",
        "firma_prokura",
        "person_vorname",
        "person_nachname",
        "person_funktion",
    ];
    match field {
        "firma_homepage_fact_sheet" => ORDER.len() + 2,
        _ => ORDER
            .iter()
            .position(|known| *known == field)
            .unwrap_or(ORDER.len() + 1),
    }
}

/// A source as a short readable address: no query or map viewport, at most
/// 90 characters. Map links and search URLs ran to 300 characters and used up
/// the block's budget.
fn short_source(source: &str) -> String {
    let source = source.trim();
    let end = source
        .find(['?', '#'])
        .into_iter()
        .chain(source.find("/@"))
        .min()
        .unwrap_or(source.len());
    let source = &source[..end];
    if source.chars().count() <= 90 {
        source.to_string()
    } else {
        format!("{}...", source.chars().take(87).collect::<String>())
    }
}

/// The records as a bounded, quotable block for the mail prompt.
pub(crate) fn render_records_for_mail(records: &[Value]) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "Auftragsdatensaetze dieser Mail (Outbound-Leads, nur lesen; nenne Arbeit, Status und Ergebnisse ausschliesslich aus diesen Feldern). \
         Steht bei einem Datensatz \"Recherche: Lauf beendet\", liegen seine Ergebnisse vor: die Antwort nennt sie mit Quelle, benennt \
         die offenen Felder als offen und behauptet nicht, die Recherche laufe noch oder Ergebnisse kaemen spaeter.\n",
    );
    let share = MAIL_RECORD_TEXT_LIMIT.saturating_sub(text.len()) / records.len().max(1);
    for record in records {
        let mut part = String::new();
        let evidence = record["evidence"].as_array().map(Vec::len).unwrap_or(0);
        part.push_str(&format!(
            "- {} [{}], Kampagne \"{}\", Website {}, Ort {}: Recherche: {}, Pruefung: {}, {} Belege\n",
            record["name"].as_str().unwrap_or("?"),
            record["id"].as_str().unwrap_or("?"),
            record["campaign"].as_str().unwrap_or(""),
            record["website"].as_str().unwrap_or("-"),
            record["city"].as_str().unwrap_or("-"),
            research_state_for_mail(record),
            validation_state_for_mail(record),
            evidence,
        ));
        // One line per (field, value) with up to two of its sources. The
        // research stores one evidence entry per source, with the address in
        // `source_url` (thesen) or in a `sources` list.
        let mut lines: Vec<((String, String), Vec<String>)> = Vec::new();
        for item in record["evidence"].as_array().into_iter().flatten() {
            let key = (
                item["field_key"].as_str().unwrap_or("?").to_string(),
                item["value"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| item["value"].to_string()),
            );
            let sources = item["sources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|source| {
                    source
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| source["url"].as_str().map(str::to_owned))
                })
                .chain(item["source_url"].as_str().map(str::to_owned))
                .filter(|source| !source.trim().is_empty());
            let index = match lines.iter().position(|(existing, _)| *existing == key) {
                Some(index) => index,
                None if lines.len() < 200 => {
                    lines.push((key, Vec::new()));
                    lines.len() - 1
                }
                None => continue,
            };
            let line = &mut lines[index].1;
            for source in sources {
                if line.len() < 2 && !line.contains(&source) {
                    line.push(source);
                }
            }
        }
        // Key facts first, long free text last, so a cut drops the least
        // useful lines (thesen 11.10.2026: revenue sat late and was cut).
        lines.sort_by_key(|((field, _), _)| mail_field_rank(field));
        for ((field, value), sources) in &lines {
            part.push_str(&format!(
                "    {field}: {value}{}\n",
                if sources.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (Quelle: {})",
                        sources
                            .iter()
                            .map(|source| short_source(source))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            ));
        }
        // Every record gets the same share of the budget, cut at a line, so a
        // long first record cannot crowd out the next one (thesen 11.10.2026:
        // the second lead ended before its management and revenue).
        if part.len() > share {
            let mut cut = 0;
            for (index, _) in part.match_indices('\n') {
                if index + 1 > share {
                    break;
                }
                cut = index + 1;
            }
            part.truncate(cut);
            part.push_str("    (weitere Felder gekuerzt)\n");
        }
        text.push_str(&part);
    }
    let mut text: String = text.chars().take(MAIL_RECORD_TEXT_LIMIT).collect();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_message_key_is_an_inbound_mail_key_and_lands_on_the_lead() {
        let mut arguments = json!({
            "payload": {
                "campaign": "Recherche aus Mail",
                "rows": [{ "name": "Hedinger GmbH & Co. KG" }],
                "source_message_key": "email:crew@example.test::inbox::42"
            }
        });
        let request = parse_request(&arguments).expect("mail key");
        let lead = lead_document(&request, &request.rows[0], 0, 1);
        assert_eq!(
            lead["payload"]["source_message_key"],
            "email:crew@example.test::inbox::42"
        );
        arguments["payload"]["source_message_key"] = json!("queue:system::x");
        assert!(parse_request(&arguments).is_err());
        let rendered = render_records_for_mail(&[json!({
            "id": "lead_x", "name": "Hedinger GmbH & Co. KG", "campaign": "C",
            "website": "hedinger.de", "city": "Stuttgart", "research_status": "done",
            "validation_status": "validated",
            "evidence": [{"field_key": "firma_umsatz", "value": "12 Mio",
                "sources": [{"url": "https://register.example/hedinger"}]}]
        })]);
        assert!(
            rendered.contains("firma_umsatz: 12 Mio (Quelle: https://register.example/hedinger)")
        );
        assert!(render_records_for_mail(&[]).is_empty());
        // A finished research waiting for a person's check is reported as
        // finished, with each result once (thesen, 11.10.2026).
        let rendered = render_records_for_mail(&[json!({
            "id": "lead_h", "name": "Hedinger GmbH & Co. KG", "research_status": "needs_review",
            "validation_status": "pending", "evidence": [
                {"field_key": "umsatz", "value": "140,6 Mio. EUR", "source_url": "https://northdata.example/h"},
                {"field_key": "umsatz", "value": "140,6 Mio. EUR", "source_url": "https://register.example/h"},
                {"field_key": "umsatz", "value": "140,6 Mio. EUR", "source_url": "https://third.example/h"}],
            "field_status": {"umsatz": {"status": "verified"}, "wz_code": {"status": "no_match"},
                "mitarbeiter": {"status": "action_required"}}
        })]);
        assert!(rendered.contains(
            "Recherche: Lauf beendet, Ergebnisse unten; 1 Felder belegt, 1 ohne Treffer, 1 offen (mitarbeiter), Pruefung: Sichtpruefung durch das Team offen"
        ));
        assert!(!rendered.contains("needs_review"));
        assert_eq!(rendered.matches("umsatz: 140,6 Mio. EUR").count(), 1);
        assert!(rendered.contains(
            "umsatz: 140,6 Mio. EUR (Quelle: https://northdata.example/h, https://register.example/h)"
        ));
        // A long first record cannot crowd out the second one, and map links
        // are shortened to their place.
        let long_values: Vec<Value> = (0..200)
            .map(|index| json!({"field_key": format!("feld_{index}"), "value": "x".repeat(60),
                "source_url": format!("https://www.google.com/maps/place/A/@48.7,9.2,12z/data={}", "y".repeat(200))}))
            .collect();
        let two = render_records_for_mail(&[
            json!({"name": "Erste GmbH", "research_status": "needs_review", "evidence": long_values}),
            json!({"name": "Zweite GmbH", "research_status": "needs_review", "evidence": [
                {"field_key": "umsatz", "value": "103,1 Mio.", "source_url": "https://register.example/z?x=1"}]}),
        ]);
        assert!(two.contains("umsatz: 103,1 Mio. (Quelle: https://register.example/z)"));
        // Revenue stored after many other fields still survives the cut.
        let mut late = long_values_for_rank();
        late.push(json!({"field_key": "umsatz", "value": "140,6 Mio.", "source_url": "https://n.example/h"}));
        let ranked = render_records_for_mail(&[
            json!({"name": "A", "research_status": "needs_review", "evidence": late.clone()}),
            json!({"name": "B", "research_status": "needs_review", "evidence": late}),
        ]);
        assert_eq!(
            ranked
                .matches("umsatz: 140,6 Mio. (Quelle: https://n.example/h)")
                .count(),
            2
        );
        assert!(two.contains("(weitere Felder gekuerzt)"));
        assert!(two.contains("(Quelle: https://www.google.com/maps/place/A)"));
        assert!(two.len() <= MAIL_RECORD_TEXT_LIMIT + 1);
        let running = render_records_for_mail(&[json!({"name": "X", "research_status": "queued"})]);
        assert!(running.contains("Recherche: laeuft noch"));
    }

    /// A lead the browser cannot save blocks the whole collection: its first
    /// update failed the schema check (422, "required field 'evidence' is
    /// missing"), was retried forever and kept every lead query from becoming
    /// ready (thesen 10.10.2026). Imported leads carry every required field.
    #[test]
    fn imported_lead_has_every_field_the_lead_schema_requires() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../apps/business-os/customer-modules/outbound-lead-generation/collections.schema.json"
        ))
        .expect("lead schema");
        let leads = schema
            .pointer("/collections/outbound_lead_generation_leads/schema")
            .or_else(|| {
                schema
                    .get("collections")
                    .and_then(Value::as_array)
                    .and_then(|list| {
                        list.iter()
                            .find(|entry| entry["name"] == "outbound_lead_generation_leads")
                    })
                    .and_then(|entry| entry.get("schema"))
            })
            .or_else(|| schema.pointer("/outbound_lead_generation_leads/schema"))
            .or_else(|| schema.pointer("/outbound_lead_generation_leads"))
            .expect("lead collection schema");
        let request = request();
        let document = lead_document(&request, &request.rows[0], 0, 1);
        for field in leads["required"].as_array().expect("required list") {
            let field = field.as_str().expect("field name");
            assert!(
                document.get(field).is_some(),
                "imported lead lacks `{field}`"
            );
            if let Some(kind) = leads.pointer(&format!("/properties/{field}/type")) {
                let ok = match kind.as_str() {
                    Some("array") => document[field].is_array(),
                    Some("object") => document[field].is_object(),
                    Some("string") => document[field].is_string(),
                    Some("number") | Some("integer") => document[field].is_number(),
                    _ => true,
                };
                assert!(ok, "imported lead field `{field}` is not of type {kind}");
            }
        }
    }

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

    fn long_values_for_rank() -> Vec<Value> {
        (0..60)
            .map(|index| json!({"field_key": format!("feld_{index}"), "value": "x".repeat(60)}))
            .collect()
    }
}
