use anyhow::Context;
use ctox_web_stack::sources::{Country, FieldKey, ResearchMode};
use ctox_web_stack::{KnownPersonRecord, PersonResearchRequest};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use super::store::{self, BusinessCommand};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ActiveResearchCommandKey {
    root: PathBuf,
    record_id: String,
}

static ACTIVE_RESEARCH_COMMANDS: OnceLock<Mutex<HashSet<ActiveResearchCommandKey>>> =
    OnceLock::new();

#[derive(Debug, Deserialize)]
struct PersonResearchCommandRequest {
    company: String,
    country: String,
    mode: String,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    include_private: Vec<String>,
    #[serde(default)]
    auto_browser_capture: bool,
    #[serde(default)]
    person_priorities: Vec<String>,
    #[serde(default)]
    known_person_records: Vec<serde_json::Value>,
    #[serde(default)]
    research_instructions: String,
    #[serde(default)]
    source_policy: RuntimeResearchSourcePolicy,
}

#[derive(Debug, Default, Deserialize)]
struct RuntimeResearchSourcePolicy {
    #[serde(default)]
    sources: Vec<RuntimeResearchSource>,
}

#[derive(Debug, Deserialize)]
struct RuntimeResearchSource {
    id: String,
    url: String,
    target_key: String,
    #[serde(default)]
    field_keys: Vec<String>,
}

pub(super) fn start(root: &Path, command: BusinessCommand) -> anyhow::Result<Value> {
    command
        .id
        .as_deref()
        .context("person-research command id is required")?;
    let mut running = store::write_rxdb_control_command_progress(
        root,
        &command,
        "running",
        serde_json::json!({
            "ok": true,
            "status": "running",
            "summary": "Recherche wurde gestartet."
        }),
    )?;
    match spawn_worker(root.to_path_buf(), command.clone()) {
        Ok(worker_started) => {
            if let Some(object) = running.as_object_mut() {
                object.insert("worker_started".to_string(), Value::Bool(worker_started));
            }
        }
        Err(error) => {
            let message = error.to_string();
            let failed = store::write_rxdb_failed_control_command_outcome(
                root,
                &command,
                "person_research_start",
                error,
            );
            if failed.is_ok() {
                log_lead_projection_error(
                    command.id.as_deref().unwrap_or_default(),
                    project_outbound_lead_generation_lead_state(
                        root,
                        &command,
                        "failed",
                        Some(message.as_str()),
                        None,
                    ),
                );
            }
            return failed;
        }
    }
    Ok(running)
}

pub(crate) fn recover_once(root: &Path) -> anyhow::Result<usize> {
    let terminal_candidates = store::terminal_person_research_projection_candidates(root)?;
    let mut recovered = 0;
    for candidate in terminal_candidates {
        let command_id = candidate
            .command
            .id
            .as_deref()
            .context("terminal person-research candidate is missing command id")?;
        super::command_plane::complete_and_project_business_control_command(
            root,
            command_id,
            &candidate.terminal_status,
            &candidate.result,
            candidate.error_message.as_deref(),
            &mut store::RxdbProjectionWriterCache::new(root),
        )?;
        let mut recovered_result = candidate.result.clone();
        let (lead_status, lead_error) = if candidate.terminal_status == "completed" {
            let gap_task = super::person_research_gap_closure::enqueue_gap_closure_if_needed(
                root,
                &candidate.command,
                &mut recovered_result,
            )?;
            (
                if gap_task.is_some() {
                    "running"
                } else {
                    "completed"
                },
                None,
            )
        } else {
            ("failed", candidate.error_message.as_deref())
        };
        log_lead_projection_error(
            command_id,
            project_outbound_lead_generation_lead_state(
                root,
                &candidate.command,
                lead_status,
                lead_error,
                (candidate.terminal_status == "completed").then_some(&recovered_result),
            ),
        );
        recovered += 1;
    }
    recovered += recover_completed_phase_a_gap_closures(root)?;
    let commands = store::recoverable_person_research_commands(root)?;
    let mut started = recovered;
    for recoverable in commands {
        let worker_started = if recoverable.status == "accepted" {
            match store::authorize_recoverable_person_research_command(root, &recoverable.command) {
                Ok(command) => start(root, command)?
                    .get("worker_started")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                Err(error) => {
                    store::write_rxdb_failed_control_command_outcome(
                        root,
                        &recoverable.command,
                        "person_research_authorization",
                        error,
                    )?;
                    false
                }
            }
        } else {
            // `running` is only persisted after the normal intake path completed
            // authentication and policy checks, so a crash-safe replay can resume
            // the worker without treating browser-controlled actor data as local.
            spawn_worker(root.to_path_buf(), recoverable.command)?
        };
        if worker_started {
            started += 1;
        }
    }
    Ok(started)
}

fn recover_completed_phase_a_gap_closures(root: &Path) -> anyhow::Result<usize> {
    let conn = store::open_store(root)?;
    let mut statement = conn.prepare(
        "SELECT command_id, module, record_id, payload_json, client_context_json
         FROM business_commands
         WHERE command_type = 'web_stack.person_research'
           AND module = 'outbound-lead-generation'
           AND status = 'completed'
           AND record_id <> ''
         ORDER BY observed_at_ms ASC",
    )?;
    let rows = statement.query_map([], |row| {
        let command_id: String = row.get(0)?;
        let payload_json: String = row.get(3)?;
        let client_context_json: String = row.get(4)?;
        Ok(BusinessCommand {
            id: Some(command_id),
            module: row.get(1)?,
            command_type: "web_stack.person_research".to_string(),
            record_id: Some(row.get(2)?),
            payload: serde_json::from_str(&payload_json).unwrap_or(Value::Null),
            client_context: serde_json::from_str(&client_context_json).unwrap_or(Value::Null),
            origin: store::CommandOrigin::TrustedLocal,
        })
    })?;
    let commands = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    drop(conn);

    let mut recovered = 0;
    for command in commands {
        let command_id = command.id.as_deref().unwrap_or_default();
        let record_id = command.record_id.as_deref().unwrap_or_default();
        let Some(_guard) = ActiveResearchCommandGuard::claim(root, record_id) else {
            continue;
        };
        let Some(lead) =
            store::load_rxdb_collection_record(root, "outbound_lead_generation_leads", record_id)?
        else {
            continue;
        };
        if lead
            .get("gap_task_id")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
            || lead
                .pointer("/payload/last_research_command_id")
                .and_then(Value::as_str)
                != Some(command_id)
        {
            continue;
        }
        let Some(command_projection) =
            store::load_rxdb_collection_record(root, "business_commands", command_id)?
        else {
            continue;
        };
        let mut result = command_projection
            .get("result")
            .cloned()
            .unwrap_or(Value::Null);
        if !result.is_object() {
            continue;
        }
        let gap_task = super::person_research_gap_closure::enqueue_gap_closure_if_needed(
            root,
            &command,
            &mut result,
        )?;
        if gap_task.is_none() {
            continue;
        }
        project_outbound_lead_generation_lead_state(
            root,
            &command,
            "running",
            None,
            Some(&result),
        )?;
        recovered += 1;
    }
    Ok(recovered)
}

fn spawn_worker(root: PathBuf, command: BusinessCommand) -> anyhow::Result<bool> {
    let command_id = command
        .id
        .as_deref()
        .context("person-research command id is required")?
        .to_string();
    let guard_record_id = command
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(command_id.as_str())
        .to_string();
    let Some(active_guard) = ActiveResearchCommandGuard::claim(&root, &guard_record_id) else {
        return Ok(false);
    };
    super::person_research_gap_closure::cancel_open_gap_task_for_new_research(&root, &command)?;
    project_outbound_lead_generation_lead_state(&root, &command, "running", None, None)?;
    let worker_command = command;
    let worker_command_id = command_id.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!(
            "ctox-person-research-{}",
            safe_workspace_segment(&command_id)
        ))
        .spawn(move || {
            let _active_guard = active_guard;
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                execute(
                    &root,
                    &worker_command_id,
                    &worker_command.payload,
                    &worker_command.client_context,
                )
            }));
            let (persisted, lead_status, lead_error, lead_result) = match result {
                Ok(Ok(mut outcome)) => {
                    let gap_task =
                        super::person_research_gap_closure::enqueue_gap_closure_if_needed(
                            &root,
                            &worker_command,
                            &mut outcome,
                        );
                    match gap_task {
                        Ok(gap_task) => {
                            let lead_status = if gap_task.is_some() {
                                "running"
                            } else {
                                "completed"
                            };
                            let lead_result = outcome.clone();
                            (
                                store::write_rxdb_control_command_outcome(
                                    &root,
                                    &worker_command,
                                    "completed",
                                    gap_task.as_ref().map(|task| task.message_key.as_str()),
                                    Some("completed"),
                                    outcome,
                                ),
                                lead_status,
                                None,
                                Some(lead_result),
                            )
                        }
                        Err(error) => {
                            let message = error.to_string();
                            (
                                store::write_rxdb_failed_control_command_outcome(
                                    &root,
                                    &worker_command,
                                    "person_research_gap_closure",
                                    error,
                                ),
                                "failed",
                                Some(message),
                                None,
                            )
                        }
                    }
                }
                Ok(Err(error)) => {
                    let message = error.to_string();
                    (
                        store::write_rxdb_failed_control_command_outcome(
                            &root,
                            &worker_command,
                            "person_research",
                            error,
                        ),
                        "failed",
                        Some(message),
                        None,
                    )
                }
                Err(_) => {
                    let message = "person-research worker panicked".to_string();
                    (
                        store::write_rxdb_failed_control_command_outcome(
                            &root,
                            &worker_command,
                            "person_research",
                            anyhow::anyhow!(message.clone()),
                        ),
                        "failed",
                        Some(message),
                        None,
                    )
                }
            };
            if let Err(error) = &persisted {
                eprintln!(
                    "[business-os] person research `{worker_command_id}` outcome failed: {error:#}"
                );
            } else {
                log_lead_projection_error(
                    &worker_command_id,
                    project_outbound_lead_generation_lead_state(
                        &root,
                        &worker_command,
                        lead_status,
                        lead_error.as_deref(),
                        lead_result.as_ref(),
                    ),
                );
            }
        });
    if let Err(error) = spawn_result {
        return Err(error.into());
    }
    Ok(true)
}

fn project_outbound_lead_generation_lead_state(
    root: &Path,
    command: &BusinessCommand,
    research_status: &str,
    error: Option<&str>,
    result: Option<&Value>,
) -> anyhow::Result<()> {
    let Some(record_id) = outbound_lead_generation_writeback_record_id(command) else {
        return Ok(());
    };
    let now = now_ms();
    let command_id = command.id.as_deref().unwrap_or_default();
    let mut lead_document = outbound_lead_generation_lead_state_document(
        record_id,
        command_id,
        research_status,
        error,
        now,
    );
    if let Some(result) = result {
        let existing =
            store::load_rxdb_collection_record(root, "outbound_lead_generation_leads", record_id)?
                .unwrap_or_else(|| serde_json::json!({ "id": record_id }));
        let outcome_patch = outbound_lead_generation_research_outcome_patch(&existing, result, now);
        merge_json_object_values(&mut lead_document, &outcome_patch);
        if let Some(gap_task_id) = result
            .pointer("/gap_closure/task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lead_document["research_status"] = Value::String("running".to_string());
            lead_document["research_phase"] = Value::String("gap_closure".to_string());
            lead_document["gap_task_id"] = Value::String(gap_task_id.to_string());
            lead_document["payload"]["gap_requested_fields"] = result
                .pointer("/gap_closure/requested_fields")
                .cloned()
                .unwrap_or(Value::Null);
            lead_document["payload"]["gap_open_fields"] = result
                .pointer("/gap_closure/open_fields")
                .cloned()
                .unwrap_or(Value::Null);
            lead_document["payload"]["gap_terminal_fields"] = result
                .pointer("/gap_closure/terminal_fields")
                .cloned()
                .unwrap_or(Value::Null);
        } else if research_status == "completed" {
            lead_document["research_status"] = Value::String("completed".to_string());
        }
    }
    store::upsert_rxdb_collection_record(
        root,
        "outbound_lead_generation_leads",
        record_id,
        now,
        lead_document,
    )
}

fn outbound_lead_generation_lead_state_document(
    record_id: &str,
    command_id: &str,
    research_status: &str,
    error: Option<&str>,
    now: i64,
) -> Value {
    let mut document = serde_json::json!({
        "id": record_id,
        "research_status": research_status,
        "command_id": command_id,
        "task_id": "",
        "payload": outbound_lead_generation_lead_state_patch(command_id, research_status, error, now),
    });
    if research_status == "running" {
        document["research_phase"] = Value::Null;
        document["gap_task_id"] = Value::Null;
    } else {
        document["research_error"] = error
            .filter(|value| !value.trim().is_empty())
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null);
        document["research_updated_at_ms"] = Value::Number(now.into());
    }
    document
}

pub(super) fn outbound_lead_generation_research_outcome_patch(
    existing: &Value,
    command_result: &Value,
    now: i64,
) -> Value {
    // Phase A reads the tool result directly, and that result carries the same
    // XML-shaped list carriers the writeback does: on the Aeroxon lead
    // 09.09.2026 `person_records` and `evidence` arrived as {"item": [...]}
    // and were read as an empty list, so the lead lost every contact.
    let command_result =
        &super::person_research_gap_closure::unwrap_item_carriers(command_result.clone());
    let outcome = command_result
        .get("result")
        .filter(|value| value.is_object())
        .unwrap_or(command_result);
    let mut data = existing
        .get("data")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut contacts = existing
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let lead_locations = lead_location_values(existing);
    set_known_person_contacts(
        existing.get("id").and_then(Value::as_str).unwrap_or("lead"),
        &mut contacts,
        outcome
            .get("known_person_records")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        &lead_locations,
    );
    let mut contact = contacts
        .first()
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let person_records = outcome
        .get("person_records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_person_records = !person_records.is_empty();
    let mut evidence = existing
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    evidence.extend(
        outcome
            .get("evidence")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    );
    let mut researched_field_keys = Vec::new();

    for (field_key, field) in outcome
        .get("fields")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let Some(value) = field
            .get("value")
            .filter(|value| research_scalar_is_populated(value))
        else {
            continue;
        };
        researched_field_keys.push(field_key.clone());
        let value = &restore_german_spelling(value, field);
        if field_key.starts_with("person_") && !has_person_records {
            contact[field_key] = value.clone();
        } else if !field_key.starts_with("person_") {
            data[field_key] = match field_key.as_str() {
                "firma_land" => normalize_country_to_iso2(value),
                "firma_domain" => normalize_domain(value),
                _ => value.clone(),
            };
        }
        for candidate in field
            .get("candidates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let source_id = candidate
                .get("source_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source_url = candidate
                .get("source_url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if source_id.trim().is_empty() && source_url.trim().is_empty() {
                continue;
            }
            evidence.push(serde_json::json!({
                "field_key": field_key,
                "value": candidate.get("value").cloned().unwrap_or(Value::Null),
                "confidence": candidate.get("confidence").cloned().unwrap_or(Value::Null),
                "source_id": source_id,
                "source_url": source_url,
                "person_key": candidate.get("person_key").cloned().unwrap_or(Value::Null),
                "quote": candidate.get("quote").or_else(|| candidate.get("note")).cloned().unwrap_or_else(|| candidate.get("value").cloned().unwrap_or(Value::Null)),
                "tier": candidate.get("tier").cloned().unwrap_or(Value::Null),
                "via": candidate.get("via").cloned().unwrap_or(Value::Null),
                "label": if source_id.trim().is_empty() { source_url } else { source_id },
            }));
        }
    }

    let person_records = person_records
        .into_iter()
        .map(restore_german_spelling_in_person_record)
        .collect::<Vec<_>>();

    for record in &person_records {
        for candidate in record
            .get("evidence")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let source_id = candidate
                .get("source_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source_url = candidate
                .get("source_url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            evidence.push(serde_json::json!({
                "field_key": candidate.get("field").cloned().unwrap_or(Value::Null),
                "value": candidate.get("value").cloned().unwrap_or(Value::Null),
                "confidence": candidate.get("confidence").cloned().unwrap_or(Value::Null),
                "source_id": source_id,
                "source_url": source_url,
                "person_key": candidate.get("person_key").cloned().unwrap_or_else(|| record.get("person_key").cloned().unwrap_or(Value::Null)),
                "quote": candidate.get("quote").or_else(|| candidate.get("note")).cloned().unwrap_or_else(|| candidate.get("value").cloned().unwrap_or(Value::Null)),
                "tier": candidate.get("tier").cloned().unwrap_or(Value::Null),
                "via": candidate.get("via").cloned().unwrap_or(Value::Null),
                "label": if source_id.trim().is_empty() { source_url } else { source_id },
            }));
        }
    }
    evidence = deduplicate_research_evidence(evidence);
    if has_person_records {
        merge_researched_person_records_with_context(
            existing.get("id").and_then(Value::as_str).unwrap_or("lead"),
            &mut contacts,
            person_records,
            &lead_locations,
        );
    } else if let Some(normalized) =
        normalize_researched_contact_with_context(contact, &lead_locations)
    {
        let normalized = with_stable_contact_id(
            existing.get("id").and_then(Value::as_str).unwrap_or("lead"),
            normalized,
            0,
        );
        if contacts.is_empty() {
            contacts.push(normalized);
        } else if let Some(existing_index) = contacts
            .iter()
            .position(|existing| contacts_match(existing, &normalized))
        {
            if contacts[existing_index]
                .get("crm_known")
                .and_then(Value::as_bool)
                == Some(true)
            {
                merge_research_into_known_contact(&mut contacts[existing_index], &normalized);
            } else {
                contacts[existing_index] = normalized;
            }
        } else {
            contacts.push(normalized);
        }
    }
    deduplicate_contacts(&mut contacts);
    let contact_ids = contacts
        .iter()
        .filter_map(|contact| contact.get("id").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    let selected_contact_ids = existing
        .get("selected_contact_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| contact_ids.contains(id))
        .map(|id| Value::String(id.to_string()))
        .collect::<Vec<_>>();
    let unverified_field_keys = researched_field_keys
        .iter()
        .filter(|field_key| {
            independent_research_evidence_count(&evidence, field_key)
                < required_independent_sources(field_key)
        })
        .cloned()
        .collect::<Vec<_>>();
    let verified_field_keys = researched_field_keys
        .iter()
        .filter(|field_key| !unverified_field_keys.contains(field_key))
        .cloned()
        .collect::<Vec<_>>();

    serde_json::json!({
        "data": data,
        "contacts": contacts,
        "selected_contact_ids": selected_contact_ids,
        "evidence": evidence,
        "research_status": if !researched_field_keys.is_empty() && unverified_field_keys.is_empty() { "completed" } else { "needs_review" },
        "research_error": Value::Null,
        "research_updated_at_ms": now,
        "payload": {
            "researched_field_keys": researched_field_keys,
            "verified_field_keys": verified_field_keys,
            "unverified_field_keys": unverified_field_keys,
            "research_finished_at_ms": now,
            "research_tool": outcome.get("tool").cloned().unwrap_or(Value::Null),
            "research_instructions_len": outcome.get("research_instructions_len").cloned().unwrap_or(Value::Null),
            "browser_assist_tasks": outcome.get("browser_assist_tasks").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
            "authenticated_source_capture_runs": outcome.get("authenticated_source_capture_runs").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        }
    })
}

/// Collapse contact rows that are the same record. Measured on the AKEMI lead
/// 09.09.2026: "Gunter-Torsten Hamann" stood six times in the contact list,
/// five of them byte-identical down to the same `id` and `person_key`. Whoever
/// appended them, a list that carries one person five times is wrong at the
/// point it is stored, so the guard sits here rather than at each writer.
fn deduplicate_contacts(contacts: &mut Vec<Value>) {
    let identity = |contact: &Value, key: &str| {
        contact
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
    };
    let mut kept: Vec<Value> = Vec::with_capacity(contacts.len());
    for contact in std::mem::take(contacts) {
        // Incoming placeholders are already refused; these are the ones that
        // were stored before that rule existed. Carbosulf still carried
        // "test test" and "n/a n/a" on 09.09.2026, and they stood in the
        // release list like real people.
        if contact_name_is_placeholder(&contact) {
            continue;
        }
        let id = identity(&contact, "id");
        let person_key = identity(&contact, "person_key");
        let duplicate_of = kept.iter().position(|existing| {
            (id.is_some() && identity(existing, "id") == id)
                || (person_key.is_some() && identity(existing, "person_key") == person_key)
        });
        if let Some(index) = duplicate_of {
            // The later row may carry a field the first one lacked.
            merge_json_object_values(&mut kept[index], &contact);
            continue;
        }
        kept.push(contact);
    }
    *contacts = kept;
}

fn merge_researched_person_records(
    lead_id: &str,
    contacts: &mut Vec<Value>,
    person_records: Vec<Value>,
) {
    merge_researched_person_records_with_context(lead_id, contacts, person_records, &[]);
}

fn merge_researched_person_records_with_context(
    lead_id: &str,
    contacts: &mut Vec<Value>,
    person_records: Vec<Value>,
    locations: &[String],
) {
    for (index, record) in person_records.into_iter().enumerate() {
        let Some(mut normalized) = normalize_researched_contact_with_context(record, locations)
        else {
            continue;
        };
        // Ein Platzhaltername ist kein Ansprechpartner. thesen 09.09.2026:
        // Carbosulf trug neun Kontakte "test test" und "n/a n/a" aus einem
        // Probelauf; sie standen in der Freigabeliste wie echte Personen.
        if contact_name_is_placeholder(&normalized) {
            continue;
        }
        normalized = with_stable_contact_id(lead_id, normalized, index);
        let existing_index = contacts.iter().position(|existing| {
            existing.get("id").and_then(Value::as_str)
                == normalized.get("id").and_then(Value::as_str)
                || profiles_match(existing, &normalized)
                || same_person_by_name(existing, &normalized)
                || (existing.get("crm_known").and_then(Value::as_bool) == Some(true)
                    && contacts_match(existing, &normalized))
        });
        if let Some(existing_index) = existing_index {
            if contacts[existing_index]
                .get("crm_known")
                .and_then(Value::as_bool)
                == Some(true)
            {
                merge_research_into_known_contact(&mut contacts[existing_index], &normalized);
            } else {
                merge_json_object_values(&mut contacts[existing_index], &normalized);
            }
        } else {
            contacts.push(normalized);
        }
    }
}

fn set_known_person_contacts(
    lead_id: &str,
    contacts: &mut Vec<Value>,
    known_person_records: Vec<Value>,
    locations: &[String],
) {
    for known in known_person_records {
        let Some(mut contact) = known_person_contact(lead_id, known, locations) else {
            continue;
        };
        let existing_index = contacts
            .iter()
            .position(|existing| contacts_match(existing, &contact));
        if let Some(existing_index) = existing_index {
            merge_known_contact_values(&mut contact, &contacts[existing_index]);
            contacts[existing_index] = contact;
        } else {
            contacts.push(contact);
        }
    }
}

fn known_person_contact(lead_id: &str, known: Value, locations: &[String]) -> Option<Value> {
    let first_name = contact_string(&known, &["vorname", "person_vorname", "first_name"]);
    let last_name = contact_string(&known, &["nachname", "person_nachname", "last_name"]);
    let function = contact_string(&known, &["funktion", "person_funktion", "role"]);
    let email = contact_string(&known, &["email", "person_email"]);
    let phone = contact_string(&known, &["telefon", "person_telefon", "phone"]);
    let sellify_contact_id = contact_string(&known, &["sellify_contact_id"]);
    let sellify_contact_id = sellify_contact_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        .then_some(sellify_contact_id)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .unwrap_or_default();
    let known_source_url = contact_string(&known, &["source"]);
    let known_source_url = url::Url::parse(&known_source_url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| url.to_string())
        .unwrap_or_default();
    let name = format!("{first_name} {last_name}").trim().to_string();
    if name.is_empty() && email.is_empty() && phone.is_empty() {
        return None;
    }
    let id_suffix = if sellify_contact_id.is_empty() {
        javascript_fingerprint(&format!("{lead_id}|{name}|{email}|{phone}"))
    } else {
        sellify_contact_id.clone()
    };
    let mut contact = serde_json::json!({
        "id": format!("contact_sellify_{id_suffix}"),
        "person_vorname": first_name,
        "person_nachname": last_name,
        "person_funktion": function,
        "person_email": email,
        "person_telefon": phone,
        "sellify_contact_id": sellify_contact_id,
        "source_url": known_source_url,
        "source": "sellify",
        "crm_known": true,
    });
    contact = normalize_researched_contact_with_context(contact, locations)?;
    contact["source"] = Value::String("sellify".to_string());
    contact["crm_known"] = Value::Bool(true);
    Some(contact)
}

fn merge_known_contact_values(known: &mut Value, existing: &Value) {
    let (Some(known), Some(existing)) = (known.as_object_mut(), existing.as_object()) else {
        return;
    };
    for (key, value) in existing {
        let known_missing = known.get(key).map(json_value_is_empty).unwrap_or(true);
        if known_missing {
            known.insert(key.clone(), value.clone());
        }
    }
    known.insert("source".to_string(), Value::String("sellify".to_string()));
    known.insert("crm_known".to_string(), Value::Bool(true));
}

fn merge_research_into_known_contact(known: &mut Value, researched: &Value) {
    let source_url = contact_string(researched, &["source_url"]);
    let (Some(known_object), Some(researched_object)) =
        (known.as_object_mut(), researched.as_object())
    else {
        return;
    };
    let mut conflicts = known_object
        .remove("conflicts")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    for (field, value) in researched_object {
        if matches!(field.as_str(), "id" | "conflicts" | "crm_known" | "source")
            || researched_alias_has_canonical_value(field, researched_object)
        {
            continue;
        }
        let known_value = known_object.get(field);
        if known_value.is_none_or(json_value_is_empty) {
            known_object.insert(field.clone(), value.clone());
        } else if !json_value_is_empty(value) && known_value != Some(value) {
            let conflict = serde_json::json!({
                "field": field,
                "value": value,
                "source_url": source_url,
            });
            if !conflicts.iter().any(|existing| existing == &conflict) {
                conflicts.push(conflict);
            }
        }
    }
    known_object.insert("conflicts".to_string(), Value::Array(conflicts));
    known_object.insert("source".to_string(), Value::String("sellify".to_string()));
    known_object.insert("crm_known".to_string(), Value::Bool(true));
}

fn researched_alias_has_canonical_value(
    field: &str,
    researched: &serde_json::Map<String, Value>,
) -> bool {
    match field {
        "person_funktion" => researched
            .get("role")
            .is_some_and(|value| !json_value_is_empty(value)),
        "person_position" => researched
            .get("position")
            .is_some_and(|value| !json_value_is_empty(value)),
        "person_email" => researched
            .get("email")
            .is_some_and(|value| !json_value_is_empty(value)),
        "person_telefon" => researched
            .get("phone")
            .is_some_and(|value| !json_value_is_empty(value)),
        "person_vorname" | "person_nachname" => researched
            .get("name")
            .is_some_and(|value| !json_value_is_empty(value)),
        _ => false,
    }
}

fn contacts_match(left: &Value, right: &Value) -> bool {
    let left_email = normalized_email(&contact_string(left, &["person_email", "email"]));
    let right_email = normalized_email(&contact_string(right, &["person_email", "email"]));
    if !left_email.is_empty() && left_email == right_email {
        return true;
    }
    if profiles_match(left, right) {
        return true;
    }
    let left_name = normalized_contact_name(left);
    let right_name = normalized_contact_name(right);
    if left_name.is_empty() || left_name != right_name {
        return false;
    }
    let left_phone = normalized_phone(&contact_string(
        left,
        &["person_telefon", "telefon", "phone"],
    ));
    let right_phone = normalized_phone(&contact_string(
        right,
        &["person_telefon", "telefon", "phone"],
    ));
    !left_phone.is_empty() && left_phone == right_phone
}

fn profiles_match(left: &Value, right: &Value) -> bool {
    let profile = |contact: &Value| {
        normalized_profile(&contact_string(
            contact,
            &[
                "person_linkedin",
                "person_xing",
                "linkedin",
                "xing",
                "source_url",
            ],
        ))
    };
    let left = profile(left);
    let right = profile(right);
    !left.is_empty() && left == right
}

fn normalized_email(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalized_phone(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect()
}

fn normalized_profile(value: &str) -> String {
    value.trim().trim_end_matches('/').to_lowercase()
}

/// Ein Kontakt, dessen Name nur aus Platzhaltern besteht, ist ein Artefakt und
/// kein Ansprechpartner — er darf nicht in die Freigabeliste geraten. Traegt der
/// Datensatz eine echte E-Mail, Telefonnummer oder ein Profil, bleibt er: dann
/// ist nur der Name unbrauchbar, nicht der Kontakt.
fn contact_name_is_placeholder(contact: &Value) -> bool {
    const PLACEHOLDERS: &[&str] = &[
        "test",
        "testtest",
        "na",
        "n/a",
        "nn",
        "xxx",
        "xx",
        "unbekannt",
        "unknown",
        "dummy",
        "muster",
        "beispiel",
        "example",
        "todo",
        "tbd",
        "keine",
        "none",
        "null",
    ];
    let first = contact_string(contact, &["person_vorname", "first_name"]);
    let last = contact_string(contact, &["person_nachname", "last_name"]);
    let combined = if first.is_empty() && last.is_empty() {
        contact_string(contact, &["name"])
    } else {
        format!("{first} {last}")
    };
    let tokens = combined
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '/')
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return false;
    }
    if !tokens
        .iter()
        .all(|token| PLACEHOLDERS.contains(&token.as_str()))
    {
        return false;
    }
    // Ohne jede weitere Kennung ist nichts zu retten.
    contact_string(contact, &["person_email", "email"]).is_empty()
        && contact_string(contact, &["person_telefon", "phone"]).is_empty()
        && contact_string(
            contact,
            &["person_linkedin", "person_xing", "linkedin", "xing"],
        )
        .is_empty()
}

/// Zwei Datensaetze beschreiben dieselbe Person, wenn der Nachname gleich ist
/// und die Vornamen sich nicht widersprechen: einer fehlt, oder der erste
/// Vorname stimmt ueberein. thesen 09.09.2026: "Bail" neben "Alena Bail",
/// "Wild" neben "Jana Wild", "Moritz Schleyer" neben "Moritz Fabian Schleyer".
/// Bewusst NICHT zusammengefuehrt werden verschiedene Vornamen zum selben
/// Nachnamen ("Olivier" und "Philippe Blondeau" sind zwei Menschen).
fn same_person_by_name(left: &Value, right: &Value) -> bool {
    // Zwei verschiedene Profile oder E-Mails sind zwei Datensaetze, auch bei
    // gleichem Namen: die Beiersdorf-Fixture haelt zwei "Frederic Heilmann"
    // mit unterschiedlichen XING-Profilen bewusst getrennt.
    let profile = |contact: &Value| {
        normalized_profile(&contact_string(
            contact,
            &[
                "person_linkedin",
                "person_xing",
                "linkedin",
                "xing",
                "source_url",
            ],
        ))
    };
    let (left_profile, right_profile) = (profile(left), profile(right));
    if !left_profile.is_empty() && !right_profile.is_empty() && left_profile != right_profile {
        return false;
    }
    let email =
        |contact: &Value| normalized_email(&contact_string(contact, &["person_email", "email"]));
    let (left_email, right_email) = (email(left), email(right));
    if !left_email.is_empty() && !right_email.is_empty() && left_email != right_email {
        return false;
    }
    let last = |contact: &Value| {
        normalize_person_name_for_match(&contact_string(contact, &["person_nachname", "last_name"]))
    };
    let first_token = |contact: &Value| {
        normalize_person_name_for_match(&contact_string(contact, &["person_vorname", "first_name"]))
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let (left_last, right_last) = (last(left), last(right));
    if left_last.is_empty() || left_last != right_last {
        return false;
    }
    let (left_first, right_first) = (first_token(left), first_token(right));
    left_first.is_empty() || right_first.is_empty() || left_first == right_first
}

/// Return a candidate personal e-mail address already found by research for
/// targets that validate a single address. Without this input, those targets
/// report `CTOX_SCRAPE_INPUT_JSON.email missing`, leaving
/// `person_email_validation` at `no_match`. Supplying the candidate allows
/// validation to run; downstream release still requires a validated address.
fn candidate_person_email(result: &Value) -> Option<String> {
    let looks_like_address = |value: &str| {
        let value = value.trim();
        value.contains('@') && value.contains('.') && !value.contains(char::is_whitespace)
    };
    if let Some(address) = result
        .pointer("/fields/person_email/value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| looks_like_address(value))
    {
        return Some(address.to_string());
    }
    result
        .get("person_records")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            record
                .get("person_email")
                .or_else(|| record.get("email"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .find(|value| looks_like_address(value))
        .map(str::to_string)
}

/// A domain is a host, not a URL. Measured 09.09.2026: Aeroxon was stored as
/// "www.aeroxon.de" while Beiersdorf was "beiersdorf.de", and Sellify would
/// have received two spellings of the same kind of value. Anything that is not
/// recognisably a host is left untouched.
fn normalize_domain(value: &Value) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return value.clone();
    }
    let host = url::Url::parse(trimmed)
        .ok()
        .or_else(|| url::Url::parse(&format!("https://{trimmed}")).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| trimmed.to_string());
    let host = host
        .trim()
        .trim_end_matches('.')
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if host.is_empty() || !host.contains('.') {
        return value.clone();
    }
    Value::String(host)
}

/// Der Importer verlangt einen zweistelligen ISO-Code, die Recherche lieferte
/// "Deutschland" (Kreussler, 09.09.2026). Zwei Schreibweisen fuer dasselbe Land
/// machen jede Auswertung nach Land unbrauchbar.
/// Umlauts and eszett folded to their ASCII spelling, so "Nürnberg" and
/// "Nuernberg" compare equal.
fn german_fold(value: &str) -> String {
    let mut folded = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            'ä' | 'Ä' => folded.push_str("ae"),
            'ö' | 'Ö' => folded.push_str("oe"),
            'ü' | 'Ü' => folded.push_str("ue"),
            'ß' => folded.push_str("ss"),
            _ => folded.extend(character.to_lowercase()),
        }
    }
    folded
}

fn has_german_diacritics(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, 'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß'))
}

/// Give a transliterated value its German spelling back, but only when a cited
/// quote carries it. Measured on the AKEMI lead 09.09.2026: the quote read
/// "D-90451 Nürnberg" while the stored city was "Nuernberg", and Sellify would
/// have received the transliteration. The correction is never invented — it is
/// literally the spelling of the source the worker cited.
pub(super) fn restore_german_spelling_from_quotes(value: &str, quotes: &[&str]) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || has_german_diacritics(trimmed) {
        return None;
    }
    let folded = german_fold(trimmed);
    if folded.is_empty() {
        return None;
    }
    for quote in quotes {
        if quote.len() > 2_000 || !has_german_diacritics(quote) {
            continue;
        }
        let characters = quote.chars().collect::<Vec<_>>();
        for start in 0..characters.len() {
            for end in (start + 1)..=characters.len() {
                if end - start > folded.chars().count() {
                    break;
                }
                let candidate = characters[start..end].iter().collect::<String>();
                if has_german_diacritics(&candidate) && german_fold(&candidate) == folded {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// A person record carries its own quotes, so a surname stored as "Mueller"
/// can take back the "Müller" its evidence spells out. Same rule as for company
/// fields: without a quote that carries the German form, nothing changes.
fn restore_german_spelling_in_person_record(mut record: Value) -> Value {
    let quotes = record
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("quote")
                .or_else(|| entry.get("note"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    if quotes.is_empty() {
        return record;
    }
    let quotes = quotes.iter().map(String::as_str).collect::<Vec<_>>();
    let Some(fields) = record.as_object_mut() else {
        return record;
    };
    for (key, value) in fields.iter_mut() {
        // Machine-readable values are never respelled.
        if key == "evidence"
            || key == "person_key"
            || key.ends_with("_url")
            || key.contains("email")
            || key.contains("linkedin")
            || key.contains("xing")
            || key.contains("telefon")
            || key.contains("phone")
        {
            continue;
        }
        let Some(text) = value.as_str() else { continue };
        if let Some(restored) = restore_german_spelling_from_quotes(text, &quotes) {
            *value = Value::String(restored);
        }
    }
    record
}

/// Applies [`restore_german_spelling_from_quotes`] to a researched field value,
/// using the quotes the worker cited for that very field.
fn restore_german_spelling(value: &Value, field: &Value) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    let quotes = field
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|candidate| {
            candidate
                .get("quote")
                .or_else(|| candidate.get("snippet"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>();
    match restore_german_spelling_from_quotes(text, &quotes) {
        Some(restored) => Value::String(restored),
        None => value.clone(),
    }
}

fn normalize_country_to_iso2(value: &Value) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    let key = text.trim().to_lowercase();
    let code = match key.as_str() {
        "de" | "deutschland" | "germany" | "bundesrepublik deutschland" => "DE",
        "at" | "österreich" | "oesterreich" | "austria" => "AT",
        "ch" | "schweiz" | "switzerland" | "suisse" | "svizzera" => "CH",
        other => {
            let trimmed = other.trim();
            if trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
                return Value::String(trimmed.to_uppercase());
            }
            return value.clone();
        }
    };
    Value::String(code.to_string())
}

fn normalized_contact_name(contact: &Value) -> String {
    let first = contact_string(contact, &["person_vorname", "first_name"]);
    let last = contact_string(contact, &["person_nachname", "last_name"]);
    let combined = if first.is_empty() && last.is_empty() {
        contact_string(contact, &["name"])
    } else {
        format!("{first} {last}")
    };
    normalize_person_name_for_match(&combined)
}

fn normalize_person_name_for_match(value: &str) -> String {
    const PARTICLES: &[&str] = &[
        "da", "de", "del", "den", "der", "di", "du", "la", "le", "van", "von", "zu", "zum", "zur",
    ];
    value
        .split(|ch: char| !ch.is_alphabetic())
        .map(transliterate_name_token)
        .filter(|token| !token.is_empty() && !PARTICLES.contains(&token.as_str()))
        .collect::<String>()
}

fn transliterate_name_token(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().flat_map(char::to_lowercase) {
        match ch {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            ch if ch.is_alphabetic() => out.push(ch),
            _ => {}
        }
    }
    out
}

fn json_value_is_empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|value| value.trim().is_empty())
        || value.as_array().is_some_and(Vec::is_empty)
}

fn lead_location_values(existing: &Value) -> Vec<String> {
    let mut locations = Vec::new();
    for pointer in ["/city", "/firma_ort", "/data/city", "/data/firma_ort"] {
        if let Some(value) = existing
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            locations.push(value.to_string());
        }
    }
    locations
}

fn research_scalar_is_populated(value: &Value) -> bool {
    match value {
        Value::String(value) => !value.trim().is_empty(),
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

fn deduplicate_research_evidence(entries: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter(|entry| {
            let key = [
                entry
                    .get("field_key")
                    .or_else(|| entry.get("field"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                entry
                    .get("source_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                entry
                    .get("source_url")
                    .or_else(|| entry.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                entry.get("value").map(Value::to_string).unwrap_or_default(),
                entry
                    .get("person_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ]
            .join("|")
            .to_ascii_lowercase();
            !key.replace('|', "").is_empty() && seen.insert(key)
        })
        .collect()
}

/// Facts a company states about itself: its switchboard number, its own mail
/// address, the profile links of its own staff. The company's own site is the
/// record of truth for these, and no second independent host can be expected to
/// repeat them. Demanding two hosts here does not raise quality, it only pushes
/// a correct value into `no_match`; the field still needs a source with a URL
/// and a verbatim quote, and the review UI marks a single source as weak.
pub(super) fn field_accepts_single_authoritative_source(field_key: &str) -> bool {
    matches!(
        field_key.trim(),
        "firma_domain"
            | "firma_email"
            | "firma_telefon"
            | "firma_fax"
            | "firma_postfach"
            | "firma_besucheranschrift"
            | "firma_postanschrift"
            | "firma_homepage_fact_sheet"
            | "person_geschlecht"
            | "person_titel"
            | "person_vorname"
            | "person_nachname"
            | "person_funktion"
            | "person_position"
            | "person_email"
            | "person_telefon"
            | "person_linkedin"
            | "person_xing"
    )
}

/// How many independent source hosts a field needs before it counts as verified.
pub(super) fn required_independent_sources(field_key: &str) -> usize {
    if field_accepts_single_authoritative_source(field_key) {
        1
    } else {
        2
    }
}

pub(super) fn independent_research_evidence_count(evidence: &[Value], field_key: &str) -> usize {
    evidence
        .iter()
        .filter(|entry| {
            entry
                .get("field_key")
                .or_else(|| entry.get("field"))
                .and_then(Value::as_str)
                == Some(field_key)
        })
        .filter_map(research_evidence_source_key)
        .collect::<HashSet<_>>()
        .len()
}

/// Suffixes whose second-to-last label is not the provider.
const COMPOUND_TLDS: &[&str] = &[
    "co.uk", "org.uk", "ac.uk", "gov.uk", "co.at", "or.at", "ac.at", "com.au", "net.au", "org.au",
    "co.jp", "com.br", "com.tr", "co.nz", "com.pl",
];

/// The provider behind a host, not the host itself. `northdata.de` and
/// `northdata.com` are one company, and so are `linkedin.com` and
/// `de.linkedin.com`. Counting hosts turns two pages of one provider into two
/// independent sources, which is exactly what the evidence rule forbids.
pub(super) fn research_evidence_provider(host: &str) -> String {
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    let labels = host
        .split('.')
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>();
    if labels.len() < 2 {
        return labels.join(".");
    }
    let last_two = labels[labels.len() - 2..].join(".");
    if COMPOUND_TLDS.contains(&last_two.as_str()) {
        return labels
            .get(labels.len().wrapping_sub(3))
            .map(|label| (*label).to_string())
            .unwrap_or(last_two);
    }
    labels[labels.len() - 2].to_string()
}

fn research_evidence_source_key(entry: &Value) -> Option<String> {
    let source_url = entry
        .get("source_url")
        .or_else(|| entry.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(host) = source_url
        .and_then(|source_url| {
            url::Url::parse(source_url)
                .ok()
                // Evidence without a scheme is common; without this the same
                // provider counts twice, once with and once without a path.
                .or_else(|| url::Url::parse(&format!("https://{source_url}")).ok())
        })
        .and_then(|url| url.host_str().map(research_evidence_provider))
        .filter(|host| !host.is_empty())
    {
        return Some(host);
    }
    entry
        .get("source_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let provider = research_evidence_provider(value);
            if provider.is_empty() {
                value.to_ascii_lowercase()
            } else {
                provider
            }
        })
}

fn normalize_researched_contact(contact: Value) -> Option<Value> {
    normalize_researched_contact_with_context(contact, &[])
}

fn normalize_researched_contact_with_context(
    mut contact: Value,
    locations: &[String],
) -> Option<Value> {
    let first_name = contact
        .get("person_vorname")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let last_name = contact
        .get("person_nachname")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let name = format!("{first_name} {last_name}").trim().to_string();
    let email = contact_string(&contact, &["person_email", "email"]);
    let phone = contact_string(&contact, &["person_telefon", "phone"]);
    let position = contact_string(&contact, &["person_position", "position"]);
    let profile = contact_string(
        &contact,
        &["person_linkedin", "person_xing", "linkedin", "xing"],
    );
    if name.is_empty()
        && email.is_empty()
        && phone.is_empty()
        && position.is_empty()
        && profile.is_empty()
    {
        return None;
    }
    let function_candidate = contact_string(&contact, &["person_funktion"]);
    let role_candidates = [
        function_candidate.clone(),
        contact_string(&contact, &["person_position"]),
        contact_string(&contact, &["role"]),
    ];
    let role = role_candidates
        .iter()
        .find(|candidate| {
            ctox_web_stack::person_ranking::role_is_valid(candidate, &name, locations)
        })
        .cloned()
        .unwrap_or_default();
    if !function_candidate.is_empty()
        && !ctox_web_stack::person_ranking::role_is_valid(&function_candidate, &name, locations)
    {
        contact["person_funktion_candidate"] = Value::String(function_candidate);
        contact["person_funktion"] = Value::String(String::new());
    } else if role.is_empty() {
        if let Some(candidate) = role_candidates
            .iter()
            .find(|candidate| !candidate.is_empty())
        {
            contact["person_funktion_candidate"] = Value::String(candidate.clone());
        }
    }
    if let Some(object) = contact.as_object_mut() {
        object.insert("name".to_string(), Value::String(name));
        object.insert("role".to_string(), Value::String(role));
        object.insert("position".to_string(), Value::String(position));
        object.insert("email".to_string(), Value::String(email));
        object.insert("phone".to_string(), Value::String(phone));
    }
    Some(contact)
}

fn contact_string(contact: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| {
            contact
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default()
        .to_string()
}

fn with_stable_contact_id(lead_id: &str, mut contact: Value, index: usize) -> Value {
    if contact
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty())
    {
        return contact;
    }
    let identity = [
        "name",
        "first_name",
        "last_name",
        "person_vorname",
        "person_nachname",
        "email",
        "person_email",
        "phone",
        "person_telefon",
        "linkedin",
        "xing",
        "person_linkedin",
        "person_xing",
    ]
    .iter()
    .map(|key| {
        contact
            .get(*key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_lowercase()
    })
    .collect::<Vec<_>>()
    .join("|");
    let identity = if identity.replace('|', "").is_empty() {
        format!("position-{index}")
    } else {
        identity
    };
    let id = format!(
        "contact_{}",
        javascript_fingerprint(&format!("{lead_id}|{identity}"))
    );
    contact["id"] = Value::String(id);
    contact
}

fn javascript_fingerprint(value: &str) -> String {
    let mut hash = 2_166_136_261_u32;
    for unit in value.to_lowercase().encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(16_777_619);
    }
    base36(hash)
}

fn base36(mut value: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        output.push(DIGITS[(value % 36) as usize] as char);
        value /= 36;
    }
    output.iter().rev().collect()
}

pub(super) fn merge_json_object_values(target: &mut Value, patch: &Value) {
    let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) else {
        *target = patch.clone();
        return;
    };
    for (key, value) in patch {
        match (target.get_mut(key), value) {
            (Some(existing), Value::Object(_)) if existing.is_object() => {
                merge_json_object_values(existing, value);
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn outbound_lead_generation_lead_state_patch(
    command_id: &str,
    research_status: &str,
    error: Option<&str>,
    now: i64,
) -> Value {
    let mut lead_payload = serde_json::json!({
        "last_research_command_id": command_id,
        "native_research_terminal_status": if research_status == "running" { Value::Null } else { Value::String(research_status.to_string()) },
    });
    if research_status == "running" {
        lead_payload["research_started_at_ms"] = Value::Number(now.into());
    } else {
        lead_payload["research_finished_at_ms"] = Value::Number(now.into());
    }
    if let Some(error) = error.filter(|value| !value.trim().is_empty()) {
        lead_payload["research_error"] = Value::String(error.to_string());
    } else if research_status != "running" {
        // Lifecycle patches are merged into the durable lead. A successful
        // retry must explicitly clear an error left by an older failed or
        // abandoned attempt instead of displaying both success and failure.
        lead_payload["research_error"] = Value::Null;
    }
    lead_payload
}

pub(super) fn outbound_lead_generation_writeback_record_id(
    command: &BusinessCommand,
) -> Option<&str> {
    if command.module.trim() != "outbound-lead-generation" {
        return None;
    }
    let record_id = command.record_id.as_deref()?.trim();
    if record_id.is_empty() {
        return None;
    }
    let contract = command.payload.get("writeback_contract")?;
    let collection_allowed = contract
        .get("collection")
        .and_then(Value::as_str)
        .is_some_and(|collection| collection == "outbound_lead_generation_leads")
        || contract
            .get("allowed_collections")
            .and_then(Value::as_array)
            .is_some_and(|collections| {
                collections
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|collection| collection == "outbound_lead_generation_leads")
            });
    if !collection_allowed {
        return None;
    }
    contract
        .get("record_ids")
        .and_then(Value::as_array)
        .is_some_and(|record_ids| {
            record_ids
                .iter()
                .filter_map(Value::as_str)
                .any(|candidate| candidate == record_id)
        })
        .then_some(record_id)
}

fn log_lead_projection_error(command_id: &str, result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!(
            "[business-os] person research `{command_id}` lead lifecycle projection failed: {error:#}"
        );
    }
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub(super) struct ActiveResearchCommandGuard {
    key: ActiveResearchCommandKey,
}

impl ActiveResearchCommandGuard {
    pub(super) fn claim(root: &Path, record_id: &str) -> Option<Self> {
        let key = ActiveResearchCommandKey {
            root: std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
            record_id: record_id.to_string(),
        };
        let mut active = active_commands();
        if active.insert(key.clone()) {
            Some(Self { key })
        } else {
            None
        }
    }
}

impl Drop for ActiveResearchCommandGuard {
    fn drop(&mut self) {
        active_commands().remove(&self.key);
    }
}

fn active_commands() -> std::sync::MutexGuard<'static, HashSet<ActiveResearchCommandKey>> {
    ACTIVE_RESEARCH_COMMANDS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn parse_requested_fields(fields: &[String]) -> anyhow::Result<Vec<FieldKey>> {
    fields
        .iter()
        .map(|field| {
            FieldKey::from_str(field)
                .with_context(|| format!("unsupported person-research field `{field}`"))
        })
        .collect()
}

fn safe_runtime_source_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn verified_research_owner_user_id(client_context: &Value) -> Option<&str> {
    [
        "/owner_user_id",
        "/actor/id",
        "/user_id",
        "/native_authorization/actor/id",
    ]
    .into_iter()
    .find_map(|pointer| {
        client_context
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn authenticated_capture_args(
    source_id: &str,
    company: &str,
    country: Country,
    command_id: &str,
    owner_user_id: &str,
) -> Vec<String> {
    vec![
        "source-capture".to_string(),
        "--source-id".to_string(),
        source_id.to_string(),
        "--company".to_string(),
        company.to_string(),
        "--country".to_string(),
        country.as_iso().to_string(),
        "--task-id".to_string(),
        command_id.to_string(),
        "--owner-user-id".to_string(),
        owner_user_id.to_string(),
        "--timeout-ms".to_string(),
        "180000".to_string(),
    ]
}

fn execute(
    root: &Path,
    command_id: &str,
    payload: &Value,
    client_context: &Value,
) -> anyhow::Result<Value> {
    let parsed_client_context = match client_context {
        Value::String(value) => serde_json::from_str(value).unwrap_or(Value::Null),
        _ => client_context.clone(),
    };
    let owner_user_id = verified_research_owner_user_id(&parsed_client_context);
    let request: PersonResearchCommandRequest = serde_json::from_value(payload.clone())
        .context("invalid web_stack.person_research payload")?;
    let company = request.company.trim();
    anyhow::ensure!(!company.is_empty(), "company is required");
    let country = Country::from_iso(&request.country).with_context(|| {
        format!(
            "unsupported country `{}`; expected DE, AT or CH",
            request.country
        )
    })?;
    let mode = ResearchMode::from_str(&request.mode).with_context(|| {
        format!(
            "unsupported research mode `{}`; expected new_record, update_firm, update_person, update_inventory_general or have_data",
            request.mode
        )
    })?;
    // The app and the native planner share one strict field vocabulary. A
    // misspelled or not-yet-implemented field must fail visibly instead of
    // making a partial result look like a complete research run.
    let fields = parse_requested_fields(&request.fields)?;
    let workspace = root
        .join("runtime")
        .join("research")
        .join("person")
        .join(safe_workspace_segment(command_id));
    let auto_browser_capture = request.auto_browser_capture;
    // Fail before any adapter work: authenticated capture must run as the human
    // who asked for the research, never as the harness worker. Without a
    // verified owner on the command there is nothing an auth session could be
    // attributed to, so the command is rejected up front instead of after the
    // Phase-A adapters have already spent their budget.
    if auto_browser_capture {
        owner_user_id
            .context("authenticated person-research capture requires a verified command owner")?;
    }
    let research_instructions_len = request.research_instructions.len();
    let known_person_records = request
        .known_person_records
        .into_iter()
        .map(serde_json::from_value::<KnownPersonRecord>)
        .collect::<Result<Vec<_>, _>>()
        .context("invalid known_person_records entry")?;
    let research_request = PersonResearchRequest {
        company: company.to_string(),
        country,
        mode,
        fields,
        include_private: request.include_private,
        person_priorities: request.person_priorities,
        known_person_records,
        workspace: Some(workspace),
        persist_workspace: true,
    };
    let mut result = ctox_web_stack::run_ctox_person_research_tool(root, &research_request)?;
    result["research_instructions_len"] = serde_json::json!(research_instructions_len);
    result["workspace_root"] = Value::String(
        research_request
            .workspace
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    // Sellify is the contractual baseline of every lead research: the CRM
    // record is consulted first and its values must appear as visible
    // evidence candidates, not as invisible pre-knowledge. A lookup failure
    // must not fail the research run.
    match merge_sellify_baseline_evidence(root, &mut result, company) {
        Ok(added) if added > 0 => {}
        Ok(_) => {}
        Err(error) => {
            eprintln!(
                "[person-research] sellify baseline evidence merge failed for `{company}`: \
                 {error:#}"
            );
        }
    }
    let ctox_bin = ctox_web_stack::sources::scrape_bridge::default_ctox_bin();
    for runtime_source in request.source_policy.sources {
        let source_id = runtime_source.id.trim();
        let target_key = runtime_source.target_key.trim();
        let source_url = runtime_source.url.trim();
        anyhow::ensure!(
            safe_runtime_source_identifier(source_id, 160)
                && safe_runtime_source_identifier(target_key, 128),
            "invalid runtime research source identifier"
        );
        // Compile-time modules already ran through the normal research plan.
        // This path exists for truly dynamic/manual/discovered adapters.
        // The skip must happen BEFORE URL validation: builtin policy entries
        // carry symbolic ids instead of URLs (`impressum`), and validating
        // them first aborted the whole research run over a source that was
        // never going to be fetched through this path.
        if ctox_web_stack::sources::find(source_id).is_some() {
            continue;
        }
        let parsed_url = url::Url::parse(source_url)
            .with_context(|| format!("invalid runtime source URL for `{source_id}`"))?;
        anyhow::ensure!(
            matches!(parsed_url.scheme(), "http" | "https") && parsed_url.host_str().is_some(),
            "runtime source URL for `{source_id}` must be HTTP(S)"
        );
        let mut target_fields = parse_requested_fields(&runtime_source.field_keys)?;
        target_fields.retain(|field| {
            research_request.fields.is_empty() || research_request.fields.contains(field)
        });
        if target_fields.is_empty() {
            continue;
        }
        let runtime_result = ctox_web_stack::sources::scrape_bridge::run_via_runtime_target(
            root,
            &ctox_bin,
            source_id,
            source_url,
            target_key,
            &target_fields,
            company,
            country,
            owner_user_id,
            candidate_person_email(&result).as_deref(),
        );
        ctox_web_stack::person_research::merge_runtime_scrape_result(
            &mut result,
            source_id,
            source_url,
            &target_fields,
            &runtime_result,
        )?;
    }
    if auto_browser_capture {
        let owner_user_id = owner_user_id
            .context("authenticated person-research capture requires a verified command owner")?;
        let capture_tasks =
            crate::service::business_os::authenticated_person_research_capture_tasks(&result);
        let mut capture_runs = Vec::new();
        let mut remaining_tasks = Vec::new();
        for task in capture_tasks {
            let Some(source_id) = task
                .get("source_id")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                remaining_tasks.push(task);
                continue;
            };
            let capture_args =
                authenticated_capture_args(&source_id, company, country, command_id, owner_user_id);
            match crate::service::business_os::run_business_os_web_stack_source_capture_trusted(
                root,
                &capture_args,
            ) {
                Ok(capture) => {
                    let summary =
                        crate::service::business_os::merge_authenticated_person_research_capture(
                            &mut result,
                            &capture,
                        )?;
                    if !summary.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                        remaining_tasks.push(task);
                    }
                    capture_runs.push(summary);
                }
                Err(error) => {
                    remaining_tasks.push(task);
                    capture_runs.push(serde_json::json!({
                        "ok": false,
                        "source_id": source_id,
                        "status": "failed",
                        "error": error.to_string(),
                        "stream": "rxdb",
                        "secret_value_in_payload": false,
                    }));
                }
            }
        }
        result["browser_assist_tasks"] = Value::Array(remaining_tasks);
        result["authenticated_source_capture_runs"] = Value::Array(capture_runs);
        crate::service::business_os::repersist_augmented_person_research(
            &research_request,
            &mut result,
        );
    }
    let populated = result
        .get("fields")
        .and_then(Value::as_object)
        .map(|fields| {
            fields
                .values()
                .filter(|field| field.get("value").is_some_and(|value| !value.is_null()))
                .count()
        })
        .unwrap_or(0);
    let requested = result
        .get("requested_fields")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let browser_assists = result
        .get("browser_assist_tasks")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "summary".to_string(),
            Value::String(if browser_assists > 0 {
                format!(
                    "Recherche für {company} abgeschlossen: {populated} von {requested} Feldern gefunden. {browser_assists} Quelle(n) benötigen eine Browser-Autorisierung."
                )
            } else {
                format!(
                    "Recherche für {company} abgeschlossen: {populated} von {requested} Feldern gefunden."
                )
            }),
        );
    }
    crate::service::business_os::repersist_augmented_person_research(
        &research_request,
        &mut result,
    );
    Ok(result)
}

pub(super) fn safe_workspace_segment(value: &str) -> String {
    let segment = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = segment.trim_matches('-');
    if trimmed.is_empty() {
        "research".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn merge_sellify_baseline_evidence(
    root: &Path,
    payload: &mut Value,
    company: &str,
) -> anyhow::Result<usize> {
    let company = company.trim();
    if company.is_empty() {
        return Ok(0);
    }
    let mut lookup_payload = serde_json::json!({
        "entity": "company",
        "selectors": [
            { "field": "name", "value": company },
            { "field": "company_name", "value": company }
        ],
        "limit": 3
    });
    // Company names drift between research input and CRM ("BNT Chemicals"
    // vs "BNT Chemicals GmbH"); probe by the legal-form-free core name too.
    if let Some(probe) = sellify_fuzzy_company_probe(company) {
        lookup_payload["fuzzy_selectors"] =
            serde_json::json!([{ "field": "name", "value": probe }]);
    }
    let lookup = super::store_outbound_commands::outbound_sellify_lookup(root, &lookup_payload)?;
    let Some(record) = lookup
        .get("records")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .cloned()
    else {
        return Ok(0);
    };
    Ok(inject_sellify_candidates(payload, &record))
}

/// Legal-form-free core of a company name, used as a containment probe for
/// CRM matching. Returns None when nothing distinctive remains.
fn sellify_fuzzy_company_probe(company: &str) -> Option<String> {
    const LEGAL_TOKENS: &[&str] = &[
        "gmbh", "mbh", "ag", "se", "kg", "kgaa", "ohg", "ug", "co", "cokg", "ev", "eg", "inc",
        "ltd", "llc", "sa", "srl", "bv", "nv",
    ];
    let core = company
        .split_whitespace()
        .filter(|token| {
            let normalized: String = token
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            !normalized.is_empty() && !LEGAL_TOKENS.contains(&normalized.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ");
    let core = core.trim();
    (core.len() >= 3 && core != company.trim()).then(|| core.to_string())
}

/// Pushes the CRM record's values as `sellify` evidence candidates and fills
/// fields that no other source answered. Existing web evidence keeps its top
/// spot; the CRM value stays visible as a candidate either way.
fn inject_sellify_candidates(payload: &mut Value, record: &Value) -> usize {
    let record_data = record.get("data").filter(|value| value.is_object());
    let record_field = |keys: &[&str]| -> Option<String> {
        keys.iter().find_map(|key| {
            record
                .get(key)
                .or_else(|| record_data.and_then(|data| data.get(key)))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
    };
    let mappings: [(&str, Option<String>); 5] = [
        ("firma_name", record_field(&["name", "company_name"])),
        ("firma_domain", record_field(&["website"])),
        ("firma_email", record_field(&["email"])),
        ("firma_telefon", record_field(&["phone"])),
        ("crm_record_number", record_field(&["contact_id", "id"])),
    ];
    let Some(fields) = payload.get_mut("fields").and_then(Value::as_object_mut) else {
        return 0;
    };
    let mut added = 0_usize;
    for (field_key, value) in mappings {
        let Some(value) = value else {
            continue;
        };
        let Some(field_result) = fields.get_mut(field_key).and_then(Value::as_object_mut) else {
            continue;
        };
        let candidate = serde_json::json!({
            "value": value,
            "confidence": "high",
            "source_id": "sellify",
            "source_url": Value::Null,
            "tier": "baseline",
            "via": "sellify_crm",
            "note": "Sellify CRM Stammdatensatz",
        });
        let candidates = match field_result
            .entry("candidates")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
        {
            Some(candidates) => candidates,
            None => continue,
        };
        if candidates.iter().any(|existing| {
            existing.get("value") == candidate.get("value")
                && existing.get("source_id") == candidate.get("source_id")
        }) {
            continue;
        }
        candidates.push(candidate.clone());
        let current_missing = field_result
            .get("value")
            .map(|value| value.is_null())
            .unwrap_or(true)
            || field_result.get("confidence").and_then(Value::as_str) == Some("missing");
        if current_missing {
            for key in [
                "value",
                "confidence",
                "source_id",
                "source_url",
                "tier",
                "note",
            ] {
                field_result.insert(
                    key.to_string(),
                    candidate.get(key).cloned().unwrap_or(Value::Null),
                );
            }
        }
        added += 1;
    }
    if added > 0 {
        if let Some(plan) = payload.get_mut("plan").and_then(Value::as_array_mut) {
            if !plan
                .iter()
                .any(|entry| entry.get("source_id").and_then(Value::as_str) == Some("sellify"))
            {
                plan.push(serde_json::json!({
                    "source_id": "sellify",
                    "tier": "baseline",
                    "api_path": false,
                    "target_key": "sellify_companies",
                    "target_fields": ["firma_name", "firma_domain", "firma_email", "firma_telefon", "crm_record_number"],
                }));
            }
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sellify_candidates_fill_missing_fields_and_stay_visible_on_filled_ones() {
        let mut payload = serde_json::json!({
            "fields": {
                "firma_name": {
                    "value": "Example AG",
                    "confidence": "high",
                    "source_id": "web",
                    "candidates": [
                        {"value": "Example AG", "confidence": "high", "source_id": "web"}
                    ]
                },
                "firma_domain": { "value": null, "confidence": "missing", "candidates": [] },
                "crm_record_number": { "value": null, "confidence": "missing", "candidates": [] }
            },
            "plan": []
        });
        let record = serde_json::json!({
            "id": "crm-42",
            "name": "Example AG",
            "website": "https://example.test",
            "contact_id": "42"
        });

        let added = inject_sellify_candidates(&mut payload, &record);

        assert_eq!(added, 3);
        // Missing fields are answered by the CRM baseline...
        assert_eq!(
            payload["fields"]["firma_domain"]["value"],
            "https://example.test"
        );
        assert_eq!(payload["fields"]["firma_domain"]["source_id"], "sellify");
        assert_eq!(payload["fields"]["crm_record_number"]["value"], "42");
        // ...while web evidence keeps its top spot but the CRM candidate is
        // still listed as visible evidence.
        assert_eq!(payload["fields"]["firma_name"]["source_id"], "web");
        assert!(payload["fields"]["firma_name"]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["source_id"] == "sellify"));
        assert!(payload["plan"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["source_id"] == "sellify"));

        // Re-injection stays idempotent.
        assert_eq!(inject_sellify_candidates(&mut payload, &record), 0);
    }

    #[test]
    fn active_worker_deduplication_is_scoped_to_the_ctox_root() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let command_id = format!("cmd-root-scope-{}", std::process::id());

        let first_guard = ActiveResearchCommandGuard::claim(first.path(), &command_id).unwrap();
        assert!(ActiveResearchCommandGuard::claim(first.path(), &command_id).is_none());
        let second_guard = ActiveResearchCommandGuard::claim(second.path(), &command_id).unwrap();

        drop(first_guard);
        assert!(ActiveResearchCommandGuard::claim(first.path(), &command_id).is_some());
        drop(second_guard);
    }

    #[test]
    fn active_worker_claim_is_released_during_unwind() {
        let root = tempfile::tempdir().unwrap();
        let command_id = format!("cmd-unwind-{}", std::process::id());
        let guard = ActiveResearchCommandGuard::claim(root.path(), &command_id).unwrap();

        let _ = panic::catch_unwind(AssertUnwindSafe(move || {
            let _guard = guard;
            panic!("worker panic");
        }));

        assert!(ActiveResearchCommandGuard::claim(root.path(), &command_id).is_some());
    }

    #[test]
    fn recovery_enqueues_missing_gap_task_for_completed_phase_a_command() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let command_id = "completed-phase-a-without-gap";
        let record_id = "lead-completed-phase-a";
        let payload = serde_json::json!({
            "company": "Example GmbH",
            "country": "DE",
            "mode": "new_record",
            "fields": ["firma_domain"],
            "writeback_contract": {
                "collection": "outbound_lead_generation_leads",
                "allowed_collections": ["outbound_lead_generation_leads"],
                "record_ids": [record_id]
            }
        });
        let conn = store::open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'outbound-lead-generation', 'web_stack.person_research', ?2, 'completed', ?3, '{}', 1)",
            rusqlite::params![command_id, record_id, serde_json::to_string(&payload)?],
        )?;
        drop(conn);
        for collection in ["business_commands", "outbound_lead_generation_leads"] {
            crate::business_os::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
                root, collection,
            )?;
        }
        store::upsert_rxdb_collection_record(
            root,
            "business_commands",
            command_id,
            1,
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "status": "completed",
                "result": {
                    "requested_fields": ["firma_domain"],
                    "fields": {"firma_domain": {"value": null, "candidates": []}},
                    "plan": []
                }
            }),
        )?;
        store::upsert_rxdb_collection_record(
            root,
            "outbound_lead_generation_leads",
            record_id,
            1,
            serde_json::json!({
                "id": record_id,
                "research_status": "needs_review",
                "payload": {"last_research_command_id": command_id}
            }),
        )?;

        assert_eq!(recover_completed_phase_a_gap_closures(root)?, 1);
        assert_eq!(recover_completed_phase_a_gap_closures(root)?, 0);
        let lead =
            store::load_rxdb_collection_record(root, "outbound_lead_generation_leads", record_id)?
                .context("recovered lead missing")?;
        assert_eq!(lead["research_status"], "running");
        assert_eq!(lead["research_phase"], "gap_closure");
        assert!(lead["gap_task_id"]
            .as_str()
            .is_some_and(|task_id| !task_id.trim().is_empty()));
        assert_eq!(
            crate::mission::channels::list_queue_tasks(root, &[], 10)?.len(),
            1
        );
        Ok(())
    }

    #[test]
    fn recovery_selects_only_nonterminal_person_research_commands() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let conn = store::open_store(root)?;
        for (id, status) in [
            ("accepted-research", "accepted"),
            ("running-research", "running"),
            ("completed-research", "completed"),
        ] {
            conn.execute(
                "INSERT INTO business_commands
                    (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
                 VALUES (?1, 'research', 'web_stack.person_research', 'company', ?2, ?3, '{}', 1)",
                rusqlite::params![
                    id,
                    status,
                    serde_json::json!({
                        "company": "Example GmbH",
                        "country": "DE",
                        "mode": "have_data"
                    })
                    .to_string(),
                ],
            )?;
        }
        drop(conn);

        let commands = store::recoverable_person_research_commands(root)?;
        let ids = commands
            .iter()
            .filter_map(|recoverable| recoverable.command.id.as_deref())
            .collect::<HashSet<_>>();
        assert_eq!(
            ids,
            HashSet::from(["accepted-research", "running-research"])
        );
        assert_eq!(commands[0].status, "accepted");
        assert_eq!(commands[1].status, "running");
        Ok(())
    }

    #[test]
    fn authenticated_capture_args_bind_command_id_and_native_authorization_owner() {
        let context = serde_json::json!({
            "native_authorization": {
                "actor": { "id": "michael.welsch@metric-space.ai" }
            }
        });
        let owner = verified_research_owner_user_id(&context).expect("verified owner");
        let args = authenticated_capture_args(
            "leadfeeder.com",
            "KUKA Deutschland GmbH",
            Country::De,
            "leadgen-lead-research-fd2769a3",
            owner,
        );

        assert!(args
            .windows(2)
            .any(|pair| { pair == ["--task-id", "leadgen-lead-research-fd2769a3"] }));
        assert!(args
            .windows(2)
            .any(|pair| { pair == ["--owner-user-id", "michael.welsch@metric-space.ai"] }));
        assert!(!args
            .windows(2)
            .any(|pair| { pair == ["--task-id", "KUKA Deutschland GmbH"] }));
    }

    #[test]
    fn verified_research_owner_uses_required_priority_order() {
        let context = serde_json::json!({
            "owner_user_id": "owner",
            "actor": { "id": "actor" },
            "user_id": "user",
            "native_authorization": { "actor": { "id": "native" } }
        });
        assert_eq!(verified_research_owner_user_id(&context), Some("owner"));
        assert_eq!(
            verified_research_owner_user_id(&serde_json::json!({
                "native_authorization": { "actor": { "id": "native" } }
            })),
            Some("native")
        );
    }

    #[test]
    fn browser_capture_requires_explicit_typed_request_flag() {
        let disabled: PersonResearchCommandRequest = serde_json::from_value(serde_json::json!({
            "company": "Example AG",
            "country": "DE",
            "mode": "new_record"
        }))
        .unwrap();
        assert!(!disabled.auto_browser_capture);

        let enabled: PersonResearchCommandRequest = serde_json::from_value(serde_json::json!({
            "company": "Example AG",
            "country": "DE",
            "mode": "new_record",
            "auto_browser_capture": true
        }))
        .unwrap();
        assert!(enabled.auto_browser_capture);
    }

    #[test]
    fn person_research_contract_accepts_priorities_known_people_and_instructions() {
        let request: PersonResearchCommandRequest = serde_json::from_value(serde_json::json!({
            "company": "Example AG",
            "country": "DE",
            "mode": "new_record",
            "person_priorities": ["Geschäftsführung/Gesamtverantwortung", "Prokura"],
            "known_person_records": [{
                "vorname": "Eric",
                "nachname": "Hahn",
                "funktion": "Geschäftsführer",
                "sellify_contact_id": "42"
            }],
            "research_instructions": "Only current leadership"
        }))
        .unwrap();

        assert_eq!(request.person_priorities.len(), 2);
        assert_eq!(request.known_person_records.len(), 1);
        assert_eq!(request.research_instructions.len(), 23);
    }

    #[test]
    fn outbound_research_contract_accepts_all_32_canonical_fields() -> anyhow::Result<()> {
        let requested = ctox_web_stack::sources::OUTBOUND_RESEARCH_FIELDS
            .iter()
            .map(|field| field.as_str().to_string())
            .collect::<Vec<_>>();

        let parsed = parse_requested_fields(&requested)?;

        assert_eq!(parsed, ctox_web_stack::sources::OUTBOUND_RESEARCH_FIELDS);
        assert_eq!(parsed.len(), 32);
        assert!(parse_requested_fields(&["firma_unbekannt".to_string()]).is_err());
        Ok(())
    }

    #[test]
    fn workspace_segment_is_bounded_and_safe() {
        let segment = safe_workspace_segment("cmd /Example Industrial:2026");
        assert_eq!(segment, "cmd--Example-Industrial-2026");
        assert!(segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')));
    }

    #[test]
    fn outbound_lead_generation_lifecycle_projection_requires_bounded_writeback_contract() {
        let command = BusinessCommand {
            id: Some("cmd-research".to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "web_stack.person_research".to_string(),
            record_id: Some("lead-1".to_string()),
            payload: serde_json::json!({
                "writeback_contract": {
                    "collection": "outbound_lead_generation_leads",
                    "allowed_collections": ["outbound_lead_generation_leads"],
                    "record_ids": ["lead-1"]
                }
            }),
            client_context: Value::Null,
            origin: store::CommandOrigin::TrustedLocal,
        };
        assert_eq!(
            outbound_lead_generation_writeback_record_id(&command),
            Some("lead-1")
        );

        let mut wrong_module = command.clone();
        wrong_module.module = "research".to_string();
        assert_eq!(
            outbound_lead_generation_writeback_record_id(&wrong_module),
            None
        );

        let mut wrong_record = command.clone();
        wrong_record.record_id = Some("lead-2".to_string());
        assert_eq!(
            outbound_lead_generation_writeback_record_id(&wrong_record),
            None
        );

        let mut wrong_collection = command;
        wrong_collection.payload["writeback_contract"]["collection"] =
            Value::String("sellify_companies".to_string());
        wrong_collection.payload["writeback_contract"]["allowed_collections"] =
            serde_json::json!(["sellify_companies"]);
        assert_eq!(
            outbound_lead_generation_writeback_record_id(&wrong_collection),
            None
        );
    }

    #[test]
    fn successful_outbound_lead_generation_retry_clears_stale_research_error() {
        let failed = outbound_lead_generation_lead_state_patch(
            "cmd-failed",
            "failed",
            Some("prior attempt failed"),
            1_000,
        );
        assert_eq!(failed["research_error"], "prior attempt failed");

        let completed =
            outbound_lead_generation_lead_state_patch("cmd-completed", "needs_review", None, 2_000);
        assert_eq!(completed["research_error"], Value::Null);
        assert_eq!(completed["native_research_terminal_status"], "needs_review");
        assert_eq!(completed["research_finished_at_ms"], 2_000);

        let document = outbound_lead_generation_lead_state_document(
            "lead-1",
            "cmd-completed",
            "needs_review",
            None,
            2_000,
        );
        assert_eq!(document["research_error"], Value::Null);
        assert_eq!(document["research_updated_at_ms"], 2_000);
        assert_eq!(document["payload"]["research_error"], Value::Null);
    }

    #[test]
    fn terminal_research_outcome_projects_contact_data_and_evidence_without_browser() {
        let existing = serde_json::json!({
            "id": "lead-1",
            "data": {"legacy": "kept"},
            "contacts": [],
            "selected_contact_ids": [],
            "evidence": [],
            "research_error": "old failure"
        });
        let outcome = serde_json::json!({
            "tool": "ctox_person_research",
            "fields": {
                "firma_domain": {
                    "value": "example.test",
                    "candidates": [
                        {"value": "example.test", "source_id": "source-a", "source_url": "https://a.test/company"},
                        {"value": "example.test", "source_id": "source-b", "source_url": "https://b.test/company"}
                    ]
                },
                "person_vorname": {
                    "value": "Ada",
                    "candidates": [{"value": "Ada", "source_id": "xing.com", "source_url": "https://www.xing.com/profile/Ada_Lovelace"}]
                },
                "person_nachname": {
                    "value": "Lovelace",
                    "candidates": [{"value": "Lovelace", "source_id": "xing.com", "source_url": "https://www.xing.com/profile/Ada_Lovelace"}]
                },
                "person_xing": {
                    "value": "https://www.xing.com/profile/Ada_Lovelace",
                    "candidates": [{"value": "https://www.xing.com/profile/Ada_Lovelace", "source_id": "xing.com", "source_url": "https://www.xing.com/profile/Ada_Lovelace"}]
                }
            },
            "browser_assist_tasks": []
        });

        let patch = outbound_lead_generation_research_outcome_patch(&existing, &outcome, 2_000);

        assert_eq!(patch["data"]["legacy"], "kept");
        assert_eq!(patch["data"]["firma_domain"], "example.test");
        assert_eq!(patch["contacts"][0]["name"], "Ada Lovelace");
        assert_eq!(
            patch["contacts"][0]["person_xing"],
            "https://www.xing.com/profile/Ada_Lovelace"
        );
        assert!(patch["contacts"][0]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("contact_")));
        // The person fields carry one source each, the XING profile itself.
        // That is the only host that can carry them, so they count as verified
        // and the lead has nothing left open.
        assert_eq!(patch["research_status"], "completed");
        assert_eq!(patch["research_error"], Value::Null);
        assert_eq!(patch["payload"]["research_tool"], "ctox_person_research");
        assert_eq!(
            patch["payload"]["verified_field_keys"],
            serde_json::json!([
                "firma_domain",
                "person_vorname",
                "person_nachname",
                "person_xing"
            ])
        );
    }

    #[test]
    fn a_domain_is_stored_as_a_bare_host() {
        let d = |value: &str| normalize_domain(&serde_json::json!(value));
        assert_eq!(d("www.aeroxon.de"), serde_json::json!("aeroxon.de"));
        assert_eq!(
            d("https://www.aeroxon.de/"),
            serde_json::json!("aeroxon.de")
        );
        assert_eq!(
            d("https://beiersdorf.de/impressum"),
            serde_json::json!("beiersdorf.de")
        );
        assert_eq!(d("Beiersdorf.DE"), serde_json::json!("beiersdorf.de"));
        // Nothing that fails to look like a host is rewritten.
        assert_eq!(
            d("keine Domain bekannt"),
            serde_json::json!("keine Domain bekannt")
        );
        assert_eq!(d("intranet"), serde_json::json!("intranet"));
    }

    #[test]
    fn a_person_record_takes_the_spelling_of_its_own_quotes() {
        let record = serde_json::json!({
            "person_key": "p1",
            "person_vorname": "Juergen",
            "person_nachname": "Mueller",
            "person_funktion": "Geschaeftsfuehrer",
            "person_email": "j.mueller@example.test",
            "evidence": [
                {"field": "person_nachname", "quote": "Geschäftsführer Jürgen Müller"}
            ]
        });
        let restored = restore_german_spelling_in_person_record(record);
        assert_eq!(restored["person_vorname"], "Jürgen");
        assert_eq!(restored["person_nachname"], "Müller");
        assert_eq!(restored["person_funktion"], "Geschäftsführer");
        // An address is not a word to be respelled.
        assert_eq!(restored["person_email"], "j.mueller@example.test");
        assert_eq!(restored["person_key"], "p1");
    }

    #[test]
    fn the_same_person_is_stored_once() {
        // Measured on the AKEMI lead 09.09.2026: five byte-identical rows for
        // one managing director, plus the CRM row for the same person_key.
        let repeated = serde_json::json!({
            "id": "contact_2ksd",
            "person_key": "person_hamann_torsten",
            "name": "Gunter-Torsten Hamann",
            "person_nachname": "Hamann"
        });
        let mut contacts = vec![
            serde_json::json!({"id": "contact_ka6iyb", "person_key": "sellify-person-59812", "name": "Gunter-Torsten Hamann", "person_email": "t.hamann@akemi.de"}),
            repeated.clone(),
            repeated.clone(),
            repeated.clone(),
            serde_json::json!({"id": "contact_cj1rmz", "person_key": "sellify-person-59813", "name": "Dirk C. Hamann"}),
        ];
        deduplicate_contacts(&mut contacts);
        assert_eq!(contacts.len(), 3);
        assert_eq!(contacts[1]["id"], "contact_2ksd");
        // A later row still contributes what the first one was missing.
        let mut with_extra = vec![
            serde_json::json!({"id": "c1", "name": "Ada Lovelace"}),
            serde_json::json!({"id": "c1", "name": "Ada Lovelace", "person_email": "ada@example.test"}),
        ];
        deduplicate_contacts(&mut with_extra);
        assert_eq!(with_extra.len(), 1);
        assert_eq!(with_extra[0]["person_email"], "ada@example.test");
        // A stored placeholder from an old probe is dropped on the next write.
        let mut with_placeholder = vec![
            serde_json::json!({"id": "c1", "name": "test test", "person_key": "test"}),
            serde_json::json!({"id": "c2", "name": "n/a n/a", "person_key": "placeholder"}),
            serde_json::json!({"id": "c3", "name": "Joppe Smit", "person_nachname": "Smit"}),
        ];
        deduplicate_contacts(&mut with_placeholder);
        assert_eq!(with_placeholder.len(), 1);
        assert_eq!(with_placeholder[0]["name"], "Joppe Smit");
    }

    #[test]
    fn a_transliterated_value_takes_the_spelling_of_its_own_quote() {
        // Measured on the AKEMI lead 09.09.2026.
        assert_eq!(
            restore_german_spelling_from_quotes("Nuernberg", &["D-90451 Nürnberg"]),
            Some("Nürnberg".to_string())
        );
        assert_eq!(
            restore_german_spelling_from_quotes(
                "Lechstrasse 28, 90451 Nuernberg",
                &["Anschrift: Lechstraße 28, 90451 Nürnberg"]
            ),
            Some("Lechstraße 28, 90451 Nürnberg".to_string())
        );
        // Without a quote carrying the spelling nothing is invented.
        assert_eq!(
            restore_german_spelling_from_quotes("Nuernberg", &["Seat: Nuremberg"]),
            None
        );
        // A value that already reads German is left alone.
        assert_eq!(
            restore_german_spelling_from_quotes("Nürnberg", &["Nürnberg"]),
            None
        );
        // A Swiss address keeps its "ss" when no quote says otherwise.
        assert_eq!(
            restore_german_spelling_from_quotes("Bahnhofstrasse 1", &["Bahnhofstrasse 1, Zürich"]),
            None
        );
    }

    #[test]
    fn two_pages_of_one_provider_are_one_source() {
        let evidence = |url: &str| serde_json::json!({"field_key": "firma_ort", "source_id": "northdata.de", "source_url": url});
        // Measured on the AKEMI lead 09.09.2026: `Ort` counted three sources
        // although two of them were northdata under different top-level domains.
        let same_house = vec![
            evidence("https://www.northdata.de/Akemi"),
            evidence("https://www.northdata.com/AKEMI/HRB%201834"),
        ];
        assert_eq!(
            independent_research_evidence_count(&same_house, "firma_ort"),
            1
        );
        let two_houses = vec![
            evidence("https://www.northdata.de/Akemi"),
            evidence("https://akemi.de/impressum"),
        ];
        assert_eq!(
            independent_research_evidence_count(&two_houses, "firma_ort"),
            2
        );
        // A subdomain is not a second source either.
        let one_platform = vec![
            evidence("https://de.linkedin.com/company/akemi"),
            evidence("https://linkedin.com/company/akemi"),
        ];
        assert_eq!(
            independent_research_evidence_count(&one_platform, "firma_ort"),
            1
        );
        // Evidence without a scheme still resolves to its provider.
        let no_scheme = vec![
            evidence("northdata.de/Akemi"),
            evidence("https://www.northdata.de/Akemi/2"),
        ];
        assert_eq!(
            independent_research_evidence_count(&no_scheme, "firma_ort"),
            1
        );
    }

    #[test]
    fn beiersdorf_fixture_keeps_eight_profiles_and_rejects_false_roles() {
        let existing = serde_json::json!({
            "id": "lead_1awj3nw",
            "data": {"firma_ort": "Leipzig"},
            "contacts": [],
            "selected_contact_ids": [],
            "evidence": []
        });
        let people = [
            ("Jana", "Laufer", Some("Geschäftsführerin"), "Jana_Laufer"),
            ("Falk", "Herbst", None, "Falk_Herbst"),
            ("Frederic", "Heilmann", Some("Leipzig"), "Frederic_Heilmann"),
            ("Frederic", "Heilmann", None, "Frederic_Heilmann_2"),
            ("Heiko", "Fischer", None, "Heiko_Fischer"),
            ("Ringo", "Bergelt", None, "Ringo_Bergelt"),
            ("Swantje", "Trinitz", None, "Swantje_Trinitz"),
            (
                "TimNils",
                "Berner",
                Some("Tim Nils Berner"),
                "TimNils_Berner",
            ),
        ];
        let records = people
            .into_iter()
            .map(|(first, last, function, slug)| {
                let mut record = serde_json::json!({
                    "person_vorname": first,
                    "person_nachname": last,
                    "person_xing": format!("https://www.xing.com/profile/{slug}"),
                    "source_id": "xing.com",
                    "source_url": format!("https://www.xing.com/profile/{slug}"),
                });
                if let Some(function) = function {
                    record["person_funktion"] = Value::String(function.to_string());
                }
                record
            })
            .collect::<Vec<_>>();
        let outcome = serde_json::json!({
            "fields": {
                "person_vorname": {"value": "Jana", "candidates": []},
                "person_nachname": {"value": "Laufer", "candidates": []}
            },
            "person_records": records
        });

        let patch = outbound_lead_generation_research_outcome_patch(&existing, &outcome, 3_500);
        let contacts = patch["contacts"].as_array().unwrap();
        let heilmann = contacts
            .iter()
            .find(|contact| {
                contact["source_url"] == "https://www.xing.com/profile/Frederic_Heilmann"
            })
            .unwrap();
        let berner = contacts
            .iter()
            .find(|contact| contact["name"] == "TimNils Berner")
            .unwrap();

        assert_eq!(contacts.len(), 8);
        assert!(!contacts.iter().any(|contact| contact["role"] == "Leipzig"));
        assert_eq!(heilmann["person_funktion"], "");
        assert_eq!(heilmann["person_funktion_candidate"], "Leipzig");
        assert_eq!(berner["role"], "");
        assert_eq!(berner["person_funktion_candidate"], "Tim Nils Berner");
    }

    #[test]
    fn known_sellify_person_wins_role_conflict_and_research_merges_into_it() {
        let existing = serde_json::json!({
            "id": "lead-sellify",
            "data": {},
            "contacts": [],
            "selected_contact_ids": [],
            "evidence": []
        });
        let outcome = serde_json::json!({
            "fields": {},
            "known_person_records": [
                {
                    "vorname": "Eric",
                    "nachname": "Hahn",
                    "funktion": "Geschäftsführer",
                    "email": "eric.hahn@example.com",
                    "sellify_contact_id": "eric-1",
                    "source": "sellify"
                },
                {
                    "vorname": "Heinz-Tristan",
                    "nachname": "Gund",
                    "funktion": "Prokurist",
                    "sellify_contact_id": "heinz-2",
                    "source": "sellify"
                }
            ],
            "person_records": [{
                "person_vorname": "Eric",
                "person_nachname": "Hahn",
                "person_funktion": "CEO",
                "person_email": "eric.hahn@example.com",
                "person_xing": "https://www.xing.com/profile/Eric_Hahn",
                "source_id": "xing.com",
                "source_url": "https://www.xing.com/profile/Eric_Hahn"
            }]
        });

        let patch = outbound_lead_generation_research_outcome_patch(&existing, &outcome, 4_000);
        let contacts = patch["contacts"].as_array().unwrap();
        let hahn = contacts
            .iter()
            .filter(|contact| contact["name"] == "Eric Hahn")
            .collect::<Vec<_>>();

        assert_eq!(hahn.len(), 1);
        assert_eq!(hahn[0]["id"], "contact_sellify_eric-1");
        assert_eq!(hahn[0]["crm_known"], true);
        assert_eq!(hahn[0]["role"], "Geschäftsführer");
        assert!(hahn[0]["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|conflict| conflict["field"] == "role" && conflict["value"] == "CEO"));
        assert!(
            contacts
                .iter()
                .any(|contact| contact["name"] == "Heinz-Tristan Gund"
                    && contact["crm_known"] == true)
        );
    }

    #[test]
    fn placeholder_contacts_are_dropped_and_partial_names_merge() {
        let mut contacts = Vec::new();
        merge_researched_person_records(
            "lead-x",
            &mut contacts,
            vec![
                serde_json::json!({"person_vorname": "test", "person_nachname": "test", "person_funktion": "test"}),
                serde_json::json!({"person_vorname": "n/a", "person_nachname": "n/a"}),
                serde_json::json!({"person_nachname": "Bail", "person_funktion": "Prokura"}),
                serde_json::json!({"person_vorname": "Alena", "person_nachname": "Bail", "person_funktion": "Prokura"}),
                serde_json::json!({"person_vorname": "Olivier", "person_nachname": "Blondeau"}),
                serde_json::json!({"person_vorname": "Philippe", "person_nachname": "Blondeau"}),
            ],
        );
        let namen = contacts
            .iter()
            .map(|c| {
                format!(
                    "{} {}",
                    c.get("person_vorname")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    c.get("person_nachname")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
                .trim()
                .to_string()
            })
            .collect::<Vec<_>>();
        assert!(
            !namen
                .iter()
                .any(|n| n.to_lowercase().contains("test") || n.contains("n/a")),
            "Platzhalternamen duerfen keinen Kontakt erzeugen: {namen:?}"
        );
        assert_eq!(
            namen.iter().filter(|n| n.contains("Bail")).count(),
            1,
            "`Bail` und `Alena Bail` sind eine Person: {namen:?}"
        );
        assert_eq!(
            namen.iter().filter(|n| n.contains("Blondeau")).count(),
            2,
            "Olivier und Philippe Blondeau sind zwei Menschen: {namen:?}"
        );
    }

    #[test]
    fn country_is_stored_as_a_two_letter_code() {
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("Deutschland")),
            serde_json::json!("DE")
        );
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("germany")),
            serde_json::json!("DE")
        );
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("Österreich")),
            serde_json::json!("AT")
        );
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("ch")),
            serde_json::json!("CH")
        );
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("DE")),
            serde_json::json!("DE")
        );
        // Unbekanntes bleibt unveraendert statt falsch geraten zu werden.
        assert_eq!(
            normalize_country_to_iso2(&serde_json::json!("Vereinigtes Koenigreich")),
            serde_json::json!("Vereinigtes Koenigreich")
        );
    }

    #[test]
    fn contacts_match_requires_more_than_a_shared_name() {
        let first = serde_json::json!({
            "person_vorname": "Michael",
            "person_nachname": "Schmidt",
            "person_xing": "https://www.xing.com/profile/Michael_Schmidt_One",
            "person_email": "one@example.com"
        });
        let namesake = serde_json::json!({
            "person_vorname": "Michael",
            "person_nachname": "Schmidt",
            "person_xing": "https://www.xing.com/profile/Michael_Schmidt_Two",
            "person_email": "two@example.com"
        });
        let same_email = serde_json::json!({
            "person_vorname": "Michael",
            "person_nachname": "Schmidt",
            "person_email": "ONE@example.com"
        });

        assert!(!contacts_match(&first, &namesake));
        assert!(contacts_match(&first, &same_email));
    }

    #[test]
    fn known_person_fallback_ids_include_lead_identity_and_validate_sellify_ids() {
        let known = serde_json::json!({
            "vorname": "Thomas",
            "nachname": "Müller",
            "email": "thomas@example.com",
            "telefon": "+49 30 1234"
        });
        let first = known_person_contact("lead-one", known.clone(), &[]).unwrap();
        let second = known_person_contact("lead-two", known.clone(), &[]).unwrap();
        assert_ne!(first["id"], second["id"]);

        let invalid = known_person_contact(
            "lead-one",
            serde_json::json!({
                "vorname": "Thomas",
                "nachname": "Müller",
                "sellify_contact_id": "../../escape"
            }),
            &[],
        )
        .unwrap();
        assert_ne!(invalid["id"], "contact_sellify_../../escape");
        assert_eq!(invalid["sellify_contact_id"], "");
    }

    #[test]
    fn normalize_researched_contact_validates_roles_and_preserves_rejected_candidate() {
        for (value, expected) in [
            ("Leipzig", false),
            ("Tim Nils Berner", false),
            ("Geschäftsführer", true),
            ("Head of Supply Chain", true),
            ("Leiterin Einkauf", true),
            ("Dr.", false),
        ] {
            let contact = normalize_researched_contact(serde_json::json!({
                "person_vorname": "Tim",
                "person_nachname": "Berner",
                "person_funktion": value,
            }))
            .unwrap();
            assert_eq!(
                !contact["role"].as_str().unwrap().is_empty(),
                expected,
                "{value}"
            );
            if expected {
                assert!(contact.get("person_funktion_candidate").is_none());
            } else {
                assert_eq!(contact["person_funktion"], "");
                assert_eq!(contact["person_funktion_candidate"], value);
            }
        }
    }

    #[test]
    fn terminal_research_outcome_projects_every_profile_bound_person_record() {
        let existing = serde_json::json!({
            "id": "lead-multi",
            "data": {},
            "contacts": [{"id": "manual-contact", "name": "Manual Contact", "email": "manual@example.test"}],
            "selected_contact_ids": ["manual-contact"],
            "evidence": []
        });
        let outcome = serde_json::json!({
            "tool": "ctox_person_research",
            "fields": {
                "person_vorname": {"value": "Ada", "candidates": []},
                "person_nachname": {"value": "Lovelace", "candidates": []},
                "person_xing": {"value": "https://www.xing.com/profile/Ada_Lovelace", "candidates": []}
            },
            "person_records": [
                {
                    "person_vorname": "Ada",
                    "person_nachname": "Lovelace",
                    "person_funktion": "Geschäftsführung",
                    "person_xing": "https://www.xing.com/profile/Ada_Lovelace",
                    "source_id": "xing.com",
                    "source_url": "https://www.xing.com/profile/Ada_Lovelace"
                },
                {
                    "person_vorname": "Grace",
                    "person_nachname": "Hopper",
                    "person_funktion": "Leitung Vertrieb",
                    "person_xing": "https://www.xing.com/profile/Grace_Hopper",
                    "source_id": "xing.com",
                    "source_url": "https://www.xing.com/profile/Grace_Hopper"
                }
            ]
        });

        let patch = outbound_lead_generation_research_outcome_patch(&existing, &outcome, 3_000);

        assert_eq!(patch["contacts"].as_array().map(Vec::len), Some(3));
        assert_eq!(patch["contacts"][0]["id"], "manual-contact");
        assert_eq!(patch["contacts"][1]["name"], "Ada Lovelace");
        assert_eq!(patch["contacts"][2]["name"], "Grace Hopper");
        assert_eq!(patch["contacts"][2]["role"], "Leitung Vertrieb");
        assert_eq!(
            patch["selected_contact_ids"],
            serde_json::json!(["manual-contact"])
        );
    }
}
