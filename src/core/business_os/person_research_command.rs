use anyhow::Context;
use ctox_web_stack::sources::{Country, FieldKey, ResearchMode};
use ctox_web_stack::PersonResearchRequest;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::store::{self, BusinessCommand};

static ACTIVE_RESEARCH_COMMANDS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct PersonResearchCommandRequest {
    company: String,
    country: String,
    mode: String,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    include_private: Vec<String>,
}

pub(super) fn start(root: &Path, command: BusinessCommand) -> anyhow::Result<Value> {
    command
        .id
        .as_deref()
        .context("person-research command id is required")?;
    let running = store::write_rxdb_control_command_progress(
        root,
        &command,
        "running",
        serde_json::json!({
            "ok": true,
            "status": "running",
            "summary": "Recherche wurde gestartet."
        }),
    )?;
    if let Err(error) = spawn_worker(root.to_path_buf(), command.clone()) {
        return store::write_rxdb_failed_control_command_outcome(
            root,
            &command,
            "person_research_start",
            error,
        );
    }
    Ok(running)
}

pub(super) fn resume(root: &Path) -> anyhow::Result<usize> {
    let commands = store::running_person_research_commands(root)?;
    let mut started = 0;
    for command in commands {
        if spawn_worker(root.to_path_buf(), command)? {
            started += 1;
        }
    }
    Ok(started)
}

fn spawn_worker(root: PathBuf, command: BusinessCommand) -> anyhow::Result<bool> {
    let command_id = command
        .id
        .as_deref()
        .context("person-research command id is required")?
        .to_string();
    {
        let mut active = active_commands();
        if !active.insert(command_id.clone()) {
            return Ok(false);
        }
    }
    let worker_command = command;
    let worker_command_id = command_id.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!(
            "ctox-person-research-{}",
            safe_workspace_segment(&command_id)
        ))
        .spawn(move || {
            let result =
                panic::catch_unwind(AssertUnwindSafe(|| execute(&root, &worker_command.payload)));
            let persisted = match result {
                Ok(Ok(outcome)) => store::write_rxdb_control_command_outcome(
                    &root,
                    &worker_command,
                    "completed",
                    None,
                    Some("completed"),
                    outcome,
                ),
                Ok(Err(error)) => store::write_rxdb_failed_control_command_outcome(
                    &root,
                    &worker_command,
                    "person_research",
                    error,
                ),
                Err(_) => store::write_rxdb_failed_control_command_outcome(
                    &root,
                    &worker_command,
                    "person_research",
                    anyhow::anyhow!("person-research worker panicked"),
                ),
            };
            if let Err(error) = persisted {
                eprintln!(
                    "[business-os] person research `{worker_command_id}` outcome failed: {error:#}"
                );
            }
            active_commands().remove(&worker_command_id);
        });
    if let Err(error) = spawn_result {
        active_commands().remove(&command_id);
        return Err(error.into());
    }
    Ok(true)
}

fn active_commands() -> std::sync::MutexGuard<'static, HashSet<String>> {
    ACTIVE_RESEARCH_COMMANDS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn execute(root: &Path, payload: &Value) -> anyhow::Result<Value> {
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
    let fields = request
        .fields
        .iter()
        .map(|field| {
            FieldKey::from_str(field)
                .with_context(|| format!("unsupported person-research field `{field}`"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let workspace =
        root.join("runtime")
            .join("research")
            .join("person")
            .join(safe_workspace_segment(
                payload
                    .get("command_id")
                    .and_then(Value::as_str)
                    .unwrap_or(company),
            ));
    let mut result = ctox_web_stack::run_ctox_person_research_tool(
        root,
        &PersonResearchRequest {
            company: company.to_string(),
            country,
            mode,
            fields,
            include_private: request.include_private,
            workspace: Some(workspace),
            persist_workspace: true,
        },
    )?;
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
    Ok(result)
}

fn safe_workspace_segment(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_segment_is_bounded_and_safe() {
        let segment = safe_workspace_segment("cmd /WITTENSTEIN:2026");
        assert_eq!(segment, "cmd--WITTENSTEIN-2026");
        assert!(segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')));
    }
}
