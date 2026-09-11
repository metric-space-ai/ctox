// Origin: CTOX
// License: AGPL-3.0-only

use anyhow::Context;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::person_research_command::{
    independent_research_evidence_count, merge_json_object_values,
    outbound_lead_generation_research_outcome_patch, parse_requested_fields,
    ActiveResearchCommandGuard,
};
use super::store::{self, BusinessCommand};
use crate::mission::channels;

/// Skill the harness worker loads for the gap-closure turn: it carries the
/// Outbound research procedure (field set, Sellify precedence, adapters as
/// tools, typed writeback) so the task prompt only names the lead and the gaps.
const GAP_CLOSURE_SKILL: &str = "outbound-lead-generation-research";
const LEAD_COLLECTION: &str = "outbound_lead_generation_leads";
const GAP_METADATA_KEY: &str = "person_research_gap_closure";
const TERMINAL_FIELD_STATUSES: &[&str] =
    &["verified", "no_match", "unsupported", "action_required"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchWritebackRequest {
    record_id: String,
    module: String,
    research_command_id: String,
    /// Set only when the daemon handed the research over as a queue task
    /// ("Lückenschluss: …"). Empty for a chat assignment researched directly
    /// by the harness worker.
    #[serde(default)]
    gap_task_id: String,
    #[serde(deserialize_with = "lenient_field_status")]
    field_status: BTreeMap<String, FieldStatus>,
    #[serde(default)]
    result: ResearchWritebackResult,
}

/// Workers keep putting field entries next to `field_status` instead of
/// inside it ("unknown field `firma_fruehere_namen`", 07.09.2026, several
/// times). A top-level key that is not part of the envelope and carries an
/// object with a `status` member is unambiguously a field entry, so it is
/// moved into `field_status` instead of failing the run.
fn hoist_top_level_field_entries(mut payload: Value) -> Value {
    const ENVELOPE: [&str; 6] = [
        "record_id",
        "module",
        "research_command_id",
        "gap_task_id",
        "field_status",
        "result",
    ];
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };
    let stray: Vec<String> = object
        .keys()
        .filter(|key| !ENVELOPE.contains(&key.as_str()))
        .filter(|key| {
            object
                .get(*key)
                .and_then(Value::as_object)
                .is_some_and(|entry| entry.contains_key("status"))
        })
        .cloned()
        .collect();
    // `result` members sent at the top level ("unknown field `person_records`",
    // 07.09.2026) are moved into `result` the same way.
    let mut result = match object.remove("result") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let mut hoisted_result = false;
    for key in ["fields", "person_records", "evidence"] {
        if let Some(value) = object.remove(key) {
            result.entry(key.to_string()).or_insert(value);
            hoisted_result = true;
        }
    }
    // Field values placed directly in `result` ("unknown field `person_vorname`,
    // expected one of `fields`, ...", 07.09.2026) belong into `result.fields`.
    let stray_result_keys: Vec<String> = result
        .keys()
        .filter(|key| !["fields", "person_records", "evidence"].contains(&key.as_str()))
        .cloned()
        .collect();
    if !stray_result_keys.is_empty() {
        let mut fields = match result.remove("fields") {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        for key in stray_result_keys {
            if let Some(value) = result.remove(&key) {
                let entry = if value.is_object() {
                    value
                } else {
                    serde_json::json!({ "value": value })
                };
                fields.entry(key).or_insert(entry);
            }
        }
        result.insert("fields".to_string(), Value::Object(fields));
        hoisted_result = true;
    }
    if hoisted_result || !result.is_empty() {
        object.insert("result".to_string(), Value::Object(result));
    }
    if stray.is_empty() {
        return payload;
    }
    let mut field_status = match object.remove("field_status") {
        Some(Value::Object(map)) => map,
        Some(Value::String(text)) => serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default(),
        _ => Map::new(),
    };
    for key in stray {
        if let Some(entry) = object.remove(&key) {
            field_status.entry(key).or_insert(entry);
        }
    }
    object.insert("field_status".to_string(), Value::Object(field_status));
    payload
}

/// `field_status` must be an object keyed by field. Live runs also sent an
/// empty string or a list where a field entry belongs (07.09.2026, three
/// rejections "expected struct FieldStatus"), and once a list of entries
/// carrying their own `field` key. Entries that are not objects are dropped
/// (the field then shows up as open); a list of objects is keyed by its
/// `field`/`key`/`name` member; a JSON string is parsed first.
fn lenient_field_status<'de, D>(deserializer: D) -> Result<BTreeMap<String, FieldStatus>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let mut raw = Value::deserialize(deserializer)?;
    if let Value::String(text) = &raw {
        raw = serde_json::from_str(text).map_err(D::Error::custom)?;
    }
    let mut entries = BTreeMap::new();
    match raw {
        Value::Object(map) => {
            for (field, status) in map {
                if !status.is_object() {
                    continue;
                }
                let parsed = serde_json::from_value::<FieldStatus>(status)
                    .map_err(|error| D::Error::custom(format!("field_status.{field}: {error}")))?;
                entries.insert(field, parsed);
            }
        }
        Value::Array(list) => {
            for item in list {
                let Some(object) = item.as_object() else {
                    continue;
                };
                let Some(field) = ["field", "key", "name", "field_key"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(Value::as_str))
                    .map(str::to_string)
                else {
                    continue;
                };
                let mut status = object.clone();
                for key in ["field", "key", "name", "field_key"] {
                    status.remove(key);
                }
                let parsed = serde_json::from_value::<FieldStatus>(Value::Object(status))
                    .map_err(|error| D::Error::custom(format!("field_status.{field}: {error}")))?;
                entries.insert(field, parsed);
            }
        }
        Value::Null => {}
        other => {
            return Err(D::Error::custom(format!(
                "field_status must be an object keyed by field, got {other}"
            )));
        }
    }
    Ok(entries)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FieldStatus {
    #[serde(deserialize_with = "lenient_string")]
    status: String,
    #[serde(default)]
    value: Value,
    #[serde(default, deserialize_with = "lenient_sources")]
    sources: Vec<FieldSource>,
    #[serde(default, deserialize_with = "lenient_attempts")]
    attempts: Vec<FieldAttempt>,
    #[serde(default, deserialize_with = "lenient_string")]
    reason: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

/// Workers hand `sources` over as a list, but live runs also sent one source as
/// a bare object and, on 07.09.2026, the whole list as a JSON string (three
/// rejections in a row for one lead, "expected a sequence"). The meaning is
/// unambiguous in all three shapes, so accept them instead of losing the run.
/// The same XML-shaped carrier as [`unwrap_single_item_container`], but applied
/// to the whole payload before anything is parsed. On the Aeroxon lead
/// 09.09.2026 the carrier wrapped not only every field's `sources` but also
/// `result.person_records` and `result.evidence`, so the contacts and the
/// evidence list were dropped as well.
///
/// Only the names a real payload never uses as a field are treated as
/// carriers, so a field object such as `{"value": "x"}` keeps its shape.
pub(super) fn unwrap_item_carriers(value: Value) -> Value {
    fn carrier_key(key: &str) -> bool {
        matches!(
            key.trim().to_ascii_lowercase().as_str(),
            "item" | "items" | "entry" | "entries" | "element" | "elements" | "list"
        )
    }
    match value {
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some((key, inner)) = map.iter().next() {
                    if carrier_key(key) && (inner.is_array() || inner.is_object()) {
                        let inner = inner.clone();
                        return match unwrap_item_carriers(inner) {
                            Value::Array(items) => Value::Array(items),
                            single => Value::Array(vec![single]),
                        };
                    }
                }
            }
            Value::Object(
                map.into_iter()
                    .map(|(key, entry)| (key, unwrap_item_carriers(entry)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(unwrap_item_carriers).collect()),
        other => other,
    }
}

/// Workers that serialise a list through an XML-shaped intermediate wrap it in
/// a single carrier key: `"sources": {"item": [ ... ]}` instead of
/// `"sources": [ ... ]`. Measured on the Aeroxon lead 09.09.2026: a writeback
/// delivered 16 verified fields, every one of them wrapped this way, and every
/// one lost its evidence and fell back to `no_match`. The wrapper carries no
/// meaning, so it is unwrapped rather than rejected.
fn unwrap_single_item_container(value: &Value) -> Option<Value> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let (key, inner) = map.iter().next()?;
    let carrier = matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "item"
            | "items"
            | "entry"
            | "entries"
            | "element"
            | "elements"
            | "list"
            | "source"
            | "sources"
            | "value"
            | "values"
    );
    if !carrier || !(inner.is_array() || inner.is_object()) {
        return None;
    }
    Some(inner.clone())
}

fn lenient_sources<'de, D>(deserializer: D) -> Result<Vec<FieldSource>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    fn from_value<E: serde::de::Error>(value: Value) -> Result<Vec<FieldSource>, E> {
        match value {
            Value::Null => Ok(Vec::new()),
            Value::Array(items) => items
                .into_iter()
                .map(|item| serde_json::from_value::<FieldSource>(item).map_err(E::custom))
                .collect(),
            Value::Object(_) => {
                if let Some(inner) = unwrap_single_item_container(&value) {
                    return from_value(inner);
                }
                Ok(vec![
                    serde_json::from_value::<FieldSource>(value).map_err(E::custom)?
                ])
            }
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(Vec::new());
                }
                let parsed: Value = serde_json::from_str(trimmed).map_err(|error| {
                    E::custom(format!(
                        "sources must be a list of source objects, not a string: {error}"
                    ))
                })?;
                if parsed.is_string() {
                    return Err(E::custom(
                        "sources must be a list of source objects, not a string",
                    ));
                }
                from_value(parsed)
            }
            other => Err(E::custom(format!(
                "sources must be a list of source objects, got {other}"
            ))),
        }
    }
    from_value(Value::deserialize(deserializer)?)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(from = "RawFieldSource")]
struct FieldSource {
    source_id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    person_key: Option<String>,
    #[serde(default)]
    requires_credential: bool,
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    command_id: String,
}

/// What workers actually send for a source: `source_id` is often missing (only
/// `url` or `host`/`domain` given), numbers appear where strings are expected
/// (a quote that is a year, a numeric source id). All of that is unambiguous,
/// so it is normalized here instead of rejecting the whole writeback
/// (07.09.2026: 16 rejections "missing field `source_id`", 8 "expected a
/// string" within one hour).
#[derive(Debug, Clone, Deserialize)]
struct RawFieldSource {
    #[serde(default, deserialize_with = "lenient_string")]
    source_id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    host: String,
    #[serde(default, deserialize_with = "lenient_string")]
    domain: String,
    #[serde(default, deserialize_with = "lenient_string")]
    url: String,
    #[serde(default, deserialize_with = "lenient_string")]
    quote: String,
    #[serde(default)]
    person_key: Option<String>,
    #[serde(default)]
    requires_credential: bool,
    #[serde(default, deserialize_with = "lenient_string")]
    task_id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    command_id: String,
}

impl From<RawFieldSource> for FieldSource {
    fn from(raw: RawFieldSource) -> Self {
        let mut source_id = raw.source_id.trim().to_string();
        if source_id.is_empty() {
            source_id = raw.host.trim().to_string();
        }
        if source_id.is_empty() {
            source_id = raw.domain.trim().to_string();
        }
        if source_id.is_empty() {
            source_id = host_of_url(&raw.url);
        }
        FieldSource {
            source_id,
            url: raw.url,
            quote: raw.quote,
            person_key: raw.person_key,
            requires_credential: raw.requires_credential,
            task_id: raw.task_id,
            command_id: raw.command_id,
        }
    }
}

fn host_of_url(url: &str) -> String {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches("www.");
    host.to_ascii_lowercase()
}

/// Strings that arrive as numbers or booleans are rendered; null becomes "".
fn lenient_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    Ok(match Value::deserialize(deserializer)? {
        Value::Null => String::new(),
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        other => return Err(D::Error::custom(format!("expected a string, got {other}"))),
    })
}

/// Attempts arrive with missing or oddly typed members ("missing field
/// `kind`" rejected six writebacks for one lead on 07.09.2026 although every
/// attempt carried a URL and a result). An attempt is documentation, not a
/// gate, so every member is optional.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FieldAttempt {
    #[serde(default, deserialize_with = "lenient_string")]
    kind: String,
    #[serde(default, deserialize_with = "lenient_string")]
    query_or_url: String,
    #[serde(default)]
    result: Value,
    #[serde(default, deserialize_with = "lenient_string")]
    artifact_path: String,
    #[serde(default)]
    at: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchWritebackResult {
    #[serde(default, deserialize_with = "lenient_object")]
    fields: Value,
    #[serde(default, deserialize_with = "lenient_values")]
    person_records: Vec<Value>,
    #[serde(default, deserialize_with = "lenient_values")]
    evidence: Vec<Value>,
}

/// Live workers send `evidence`/`person_records` as `""` or as one object;
/// both are unambiguous (empty / one entry). A JSON string holding a list is
/// parsed. A bare word is still an error.
fn lenient_values<'de, D>(deserializer: D) -> Result<Vec<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    fn from_value<E: serde::de::Error>(value: Value) -> Result<Vec<Value>, E> {
        match value {
            Value::Null => Ok(Vec::new()),
            Value::Array(items) => Ok(items),
            Value::Object(_) => {
                if let Some(inner) = unwrap_single_item_container(&value) {
                    return from_value(inner);
                }
                Ok(vec![value])
            }
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(Vec::new());
                }
                let parsed: Value = serde_json::from_str(trimmed).map_err(|error| {
                    E::custom(format!("expected a list of objects, not a string: {error}"))
                })?;
                if parsed.is_string() {
                    return Err(E::custom("expected a list of objects, not a string"));
                }
                from_value(parsed)
            }
            other => Err(E::custom(format!(
                "expected a list of objects, got {other}"
            ))),
        }
    }
    from_value(Value::deserialize(deserializer)?)
}

/// `result.fields` as `""`/null means "no fields"; a JSON string holding an
/// object is parsed.
fn lenient_object<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    Ok(match Value::deserialize(deserializer)? {
        Value::Null => Value::Object(Map::new()),
        Value::String(text) if text.trim().is_empty() => Value::Object(Map::new()),
        Value::String(text) => {
            let parsed: Value = serde_json::from_str(text.trim()).map_err(|error| {
                D::Error::custom(format!("expected an object, not a string: {error}"))
            })?;
            if !parsed.is_object() {
                return Err(D::Error::custom("expected an object of field entries"));
            }
            parsed
        }
        other => other,
    })
}

fn lenient_attempts<'de, D>(deserializer: D) -> Result<Vec<FieldAttempt>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let values = lenient_values(deserializer)?;
    values
        .into_iter()
        .map(|value| serde_json::from_value::<FieldAttempt>(value).map_err(D::Error::custom))
        .collect()
}

pub(super) fn build_gap_closure_prompt(contract: &Value) -> anyhow::Result<String> {
    let company = required_string(contract, "company")?;
    let record_id = required_string(contract, "record_id")?;
    let research_command_id = required_string(contract, "research_command_id")?;
    let module = required_string(contract, "module")?;
    let open_fields = contract
        .get("open_fields")
        .and_then(Value::as_array)
        .context("gap closure contract open_fields array is required")?;
    let instructions = contract
        .get("research_instructions")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let priorities = serde_json::to_string_pretty(
        contract
            .get("person_priorities")
            .unwrap_or(&Value::Array(Vec::new())),
    )?;
    let known_people = serde_json::to_string_pretty(
        contract
            .get("known_person_records")
            .unwrap_or(&Value::Array(Vec::new())),
    )?;
    let attempted = serde_json::to_string_pretty(
        contract
            .get("attempted_sources")
            .unwrap_or(&Value::Array(Vec::new())),
    )?;
    let terminal_fields = serde_json::to_string_pretty(
        contract
            .get("terminal_fields")
            .unwrap_or(&Value::Object(Map::new())),
    )?;
    let writeback_contract = serde_json::to_string_pretty(
        contract
            .get("writeback_contract")
            .context("gap closure writeback_contract is required")?,
    )?;
    let mut field_lines = Vec::new();
    for field in open_fields {
        let key = field
            .as_str()
            .context("gap closure open field names must be strings")?;
        field_lines.push(format!("- `{key}`: {}", field_definition(key)));
    }

    Ok(format!(
        r#"# Lückenschluss Personen-/Firmenrecherche

Firma: {company}
Lead/Record: {record_id}
Modul: {module}
Phase-A-Kommando: {research_command_id}

## Offene Felder mit Definition
{fields}

## Bereits versuchte Quellen aus Phase A
{attempted}

## Bereits terminale Phase-A-Felder (unverändert mitführen)
{terminal_fields}

## Owner-research_instructions (wörtlich, unverändert)
--- BEGIN OWNER INSTRUCTIONS ---
{instructions}
--- END OWNER INSTRUCTIONS ---

## Personen-Prioritäten
{priorities}

## Bereits bekannte Sellify-Personen
{known_people}

## Verbindlicher Feldvertrag für Phase B
- Bearbeite jedes offene Feld einzeln mit `ctox web search` und `ctox web read`; `ctox web browser-capture` ist optional.
- Vor `no_match` sind mindestens 1 dokumentierte Websuche und mindestens 2 dokumentierte Seitenlektüren (`web_read` oder `browser_capture`) Pflicht.
- Schreibe JEDEN Versuch als JSON-Datei unter `gap_closure/attempts/<feld>/<n>.json` in diesem Workspace. Jeder Versuch enthält kind, query_or_url, result, artifact_path und at.
- Halte den fortlaufenden Sammelstand nach jedem Versuch in `gap_closure/field_status.json` fest. Diese Datei ist der Checkpoint für einen Folgeturn.
- Terminale Feldstatus sind ausschließlich `verified`, `no_match`, `unsupported` und `action_required`.
- `verified` verlangt einen Wert und Belege mit source_id, URL und wörtlichem Belegtext.
- Zwei unabhängige Hosts sind Pflicht für Angaben, die Dritte prüfen können: Firmenname, Anschrift, PLZ, Ort, Land, Aktivitätsstatus, frühere Namen, Geschäftstätigkeit, Geschäftsführung, Prokura, WZ-Code, Umsatz, Mitarbeiter.
- EIN Beleg genügt bei Selbstauskünften, für die es keine zweite unabhängige Quelle geben kann: firma_domain, firma_email, firma_telefon, firma_fax, firma_postfach, firma_besucheranschrift, firma_postanschrift, firma_homepage_fact_sheet sowie alle person_-Felder. Belege sie von der Unternehmensseite bzw. dem Profil selbst und trage den Wert ein, statt ihn als no_match zu verwerfen.
- Listen sind reine JSON-Listen: `"sources": [ {{...}}, {{...}} ]`. NIEMALS ein Traegerobjekt wie `{{"item": [...]}}`, weder bei `sources` und `attempts` noch bei `result.person_records` und `result.evidence`.
- E-Mail-Pruefung (`person_email_validation`) uebernimmt der Daemon selbst: nach jedem Rueckschreiben prueft er jede gelieferte Kontaktadresse ueber experte.de und haengt das Ergebnis dem Kontakt an. Liefere die Adresse als `person_email` mit Beleg und dem `person_key` der Person. Setze `person_email_validation` NICHT auf `no_match`, nur weil du die Pruefung nicht selbst ausfuehren kannst.
- Personenbezogene Ergebnisse und Belege tragen einen stabilen `person_key`.
- Schreibe in `result.fields` nur strukturierte Feldobjekte, keine freien Texte.
- `action_required` ist ausschließlich für Login/Freigabe zulässig und verweist auf einen Auth-Assist (source_id plus Task-/Command-ID) oder eine Quelle mit `requires_credential=true`.
- Abschluss erfolgt AUSSCHLIESSLICH mit `ctox business-os commands dispatch` und dem typisierten Befehl `outbound.lead.research_writeback`.
- Der Task darf sich nicht als fertig melden und darf nicht beendet werden, bevor dieser Dispatch vom Daemon angenommen wurde.

## Writeback-Vertrag
{writeback_contract}

Sende beim Abschluss exakt die Payload-Felder `record_id`, `module`, `research_command_id`, `gap_task_id`, `field_status` und `result`. `field_status` muss ALLE angeforderten Felder abdecken; bereits terminale Phase-A-Felder werden unverändert mitgeführt."#,
        fields = field_lines.join("\n")
    ))
}

pub(super) fn enqueue_gap_closure_if_needed(
    root: &Path,
    command: &BusinessCommand,
    phase_a_result: &mut Value,
) -> anyhow::Result<Option<channels::QueueTaskView>> {
    let command_id = command
        .id
        .as_deref()
        .context("person-research command id is required for gap closure")?;
    let Some(record_id) =
        super::person_research_command::outbound_lead_generation_writeback_record_id(command)
    else {
        return Ok(None);
    };
    let requested_fields = canonical_requested_fields(command, phase_a_result)?;
    let phase_a_evidence = phase_a_projection_evidence(record_id, phase_a_result);
    let mut terminal_fields = Map::new();
    let mut open_fields = Vec::new();
    for field in &requested_fields {
        let field_result = phase_a_result.pointer(&format!("/fields/{field}"));
        let populated = field_result
            .and_then(|entry| entry.get("value"))
            .is_some_and(research_value_is_populated);
        let sources = phase_a_verified_sources(&phase_a_evidence, field);
        let independent = independent_research_evidence_count(&sources, field);
        let person_keys = sources
            .iter()
            .filter_map(|source| source.get("person_key").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<BTreeSet<_>>();
        let person_evidence_complete = !field.starts_with("person_")
            || (person_keys.len() == 1
                && sources.iter().all(|source| {
                    source
                        .get("person_key")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty())
                }));
        let required = super::person_research_command::required_independent_sources(field);
        if populated && independent >= required && person_evidence_complete {
            terminal_fields.insert(
                field.clone(),
                serde_json::json!({
                    "status": "verified",
                    "value": field_result.and_then(|entry| entry.get("value")).cloned().unwrap_or(Value::Null),
                    "sources": sources,
                    "attempts": [],
                    "independent_sources": independent,
                }),
            );
        } else {
            open_fields.push(Value::String(field.clone()));
        }
    }
    if open_fields.is_empty() {
        phase_a_result["gap_closure"] = serde_json::json!({
            "required": false,
            "requested_fields": requested_fields,
            "terminal_fields": terminal_fields,
        });
        return Ok(None);
    }

    let company = command
        .payload
        .get("company")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("gap closure requires company")?;
    let workspace = phase_a_workspace(root, command)?;
    std::fs::create_dir_all(&workspace)
        .with_context(|| format!("create gap closure workspace {}", workspace.display()))?;
    let attempted_sources = phase_a_attempted_sources(phase_a_result);
    let writeback_contract = serde_json::json!({
        "command_type": "outbound.lead.research_writeback",
        "record_id": record_id,
        "module": "outbound-lead-generation",
        "research_command_id": command_id,
        "gap_task_id": "<current queue task id>",
        "field_status": {
            "statuses": TERMINAL_FIELD_STATUSES,
            "verified": "value plus at least two sources on different hosts",
            "no_match": "at least one web_search plus two web_read/browser_capture artifacts",
            "unsupported": "field cannot be researched by the available contract",
            "action_required": "login or approval with auth-assist reference"
        },
        "result": {"fields": {}, "person_records": [], "evidence": []}
    });
    let contract = serde_json::json!({
        "lead_id": record_id,
        "record_id": record_id,
        "company": company,
        "module": command.module,
        "research_command_id": command_id,
        "requested_fields": requested_fields,
        "open_fields": open_fields,
        "terminal_fields": terminal_fields,
        "attempted_sources": attempted_sources,
        "research_instructions": command.payload.get("research_instructions").cloned().unwrap_or_else(|| Value::String(String::new())),
        "known_person_records": command.payload.get("known_person_records").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        "person_priorities": command.payload.get("person_priorities").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        "writeback_contract": writeback_contract,
    });
    let prompt = build_gap_closure_prompt(&contract)?;
    let idempotency_key = format!("gap:{command_id}");
    let mut task = if let Some(existing) = load_gap_task_by_idempotency_key(root, &idempotency_key)?
    {
        existing
    } else {
        channels::create_queue_task(
            root,
            channels::QueueTaskCreateRequest {
                title: format!("Lückenschluss: {company}"),
                prompt,
                thread_key: format!("person-research-gap/{record_id}"),
                workspace_root: Some(workspace.to_string_lossy().into_owned()),
                priority: "high".to_string(),
                suggested_skill: Some(GAP_CLOSURE_SKILL.to_string()),
                parent_message_key: None,
                extra_metadata: Some(serde_json::json!({
                    "idempotency_key": idempotency_key,
                    GAP_METADATA_KEY: contract,
                })),
            },
        )?
    };
    // The gap task inherits the research command's human owner through the
    // `business_os_command_id` metadatum: owner resolution for
    // `--task-id <gap task>` and the harness command session follow it to the
    // command's verified actor. It is deliberately NOT a
    // `business_command_task_links` row — that link marks a task as the
    // command's own execution, and for an already-completed research command
    // the queue would settle the gap task to `handled` on lease as an
    // "orphaned lease of a terminal command" without running it.
    channels::set_queue_task_metadata_value(
        root,
        &task.message_key,
        "business_os_command_id",
        Value::String(command_id.to_string()),
    )?;
    let mut final_contract = contract;
    final_contract["gap_task_id"] = Value::String(task.message_key.clone());
    final_contract["writeback_contract"]["gap_task_id"] = Value::String(task.message_key.clone());
    channels::set_queue_task_metadata_value(
        root,
        &task.message_key,
        GAP_METADATA_KEY,
        final_contract.clone(),
    )?;
    task = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key.clone(),
            title: Some(format!("Lückenschluss: {company}")),
            prompt: Some(build_gap_closure_prompt(&final_contract)?),
            thread_key: Some(format!("person-research-gap/{record_id}")),
            workspace_root: Some(workspace.to_string_lossy().into_owned()),
            priority: Some("high".to_string()),
            ..Default::default()
        },
    )?;
    // Solange eine Nachrecherche eingereiht ist, LAEUFT die Recherche. Ohne
    // diese Zeile blieb der Lead auf `needs_review` stehen, waehrend der
    // Lueckenschluss-Worker arbeitete, und die Liste zeigte "Pruefung noetig"
    // fuer einen laufenden Vorgang (thesen 09.09.2026, Hoffmann und AKEMI).
    if let Some(mut lead) = store::load_rxdb_collection_record(root, LEAD_COLLECTION, record_id)? {
        lead["research_status"] = Value::String("running".to_string());
        lead["research_phase"] = Value::String("gap_closure".to_string());
        lead["gap_task_id"] = Value::String(task.message_key.clone());
        store::upsert_rxdb_collection_record(
            root,
            LEAD_COLLECTION,
            record_id,
            super::person_research_command::now_ms(),
            lead,
        )?;
    }
    phase_a_result["gap_closure"] = serde_json::json!({
        "required": true,
        "owner_command_id": command_id,
        "task_id": task.message_key.clone(),
        "workspace_root": task.workspace_root.clone(),
        "requested_fields": task.metadata.pointer(&format!("/{GAP_METADATA_KEY}/requested_fields")).cloned().unwrap_or(Value::Null),
        "open_fields": task.metadata.pointer(&format!("/{GAP_METADATA_KEY}/open_fields")).cloned().unwrap_or(Value::Null),
        "terminal_fields": task.metadata.pointer(&format!("/{GAP_METADATA_KEY}/terminal_fields")).cloned().unwrap_or(Value::Null),
    });
    Ok(Some(task))
}

/// The previous `researched_field_keys` / `verified_field_keys` /
/// `unverified_field_keys` of a lead, captured before a writeback patch.
fn previous_research_keys(lead: &Value) -> BTreeMap<String, Vec<String>> {
    [
        "researched_field_keys",
        "verified_field_keys",
        "unverified_field_keys",
    ]
    .iter()
    .map(|name| {
        let keys = lead
            .pointer(&format!("/payload/{name}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        ((*name).to_string(), keys)
    })
    .collect()
}

/// A follow-up writeback (gap closure, continuation, a worker reporting the
/// one field it worked) must not shrink what the lead already carries. On
/// 07.09.2026 a one-field gap writeback replaced the lead's field status and
/// its researched keys (6 verified fields → 1) while the data itself stayed.
/// Statuses are merged per field (new entry wins for its own field), key lists
/// are unioned, and a key that is now verified leaves the unverified list.
fn union_research_keys(lead: &mut Value, previous: &BTreeMap<String, Vec<String>>) {
    let Some(payload) = lead.get_mut("payload").and_then(Value::as_object_mut) else {
        return;
    };
    let mut union = |name: &str| -> Vec<String> {
        let mut keys = previous.get(name).cloned().unwrap_or_default();
        for key in payload
            .get(name)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !keys.iter().any(|existing| existing == key) {
                keys.push(key.to_string());
            }
        }
        keys
    };
    let researched = union("researched_field_keys");
    let verified = union("verified_field_keys");
    let unverified = union("unverified_field_keys")
        .into_iter()
        .filter(|key| !verified.contains(key))
        .collect::<Vec<_>>();
    payload.insert(
        "researched_field_keys".to_string(),
        Value::Array(researched.into_iter().map(Value::String).collect()),
    );
    payload.insert(
        "verified_field_keys".to_string(),
        Value::Array(verified.into_iter().map(Value::String).collect()),
    );
    payload.insert(
        "unverified_field_keys".to_string(),
        Value::Array(unverified.into_iter().map(Value::String).collect()),
    );
}

/// A follow-up writeback carries the sanitizer's filler entries
/// (`unsupported`, "Vom Rueckschreiben nicht geliefert") for every requested
/// field the worker did not deliver. Those fillers must never downgrade a
/// status an earlier writeback established (07.09.2026: Aeroxon lost all
/// fifteen `verified` entries to fillers, the gap closure then re-opened every
/// field). An incoming `unsupported` without sources or attempts is only
/// accepted for a field that has no status yet or is itself `unsupported`.
fn merge_field_status(existing: Option<&Value>, incoming: Value) -> Value {
    let mut merged = existing
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(incoming) = incoming.as_object() {
        for (field, status) in incoming {
            if is_filler_field_status(status) {
                let previous_is_informative = merged.get(field).is_some_and(|previous| {
                    !previous
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unsupported")
                        .trim()
                        .eq_ignore_ascii_case("unsupported")
                });
                if previous_is_informative {
                    continue;
                }
            }
            merged.insert(field.clone(), status.clone());
        }
    }
    Value::Object(merged)
}

/// A research field is answered when the lead carries a verified value for it
/// or a documented `no_match`. `unsupported` and `action_required` mean the
/// question is still open, whether the worker never delivered the field or the
/// evidence gate rejected what it delivered.
fn research_field_is_answered(status: &Value) -> bool {
    matches!(
        status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim(),
        "verified" | "no_match"
    )
}

fn is_filler_field_status(status: &Value) -> bool {
    let unsupported = status
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("unsupported");
    let no_sources = status
        .get("sources")
        .and_then(Value::as_array)
        .map(|sources| sources.is_empty())
        .unwrap_or(true);
    let no_attempts = status
        .get("attempts")
        .and_then(Value::as_array)
        .map(|attempts| attempts.is_empty())
        .unwrap_or(true);
    let placeholder_reason = status
        .get("reason")
        .and_then(Value::as_str)
        .map(|reason| reason.trim().to_ascii_lowercase())
        .is_some_and(|reason| {
            matches!(
                reason.as_str(),
                "test" | "testing" | "probe" | "dummy" | "n/a" | "na" | "-" | "tbd" | "todo"
            )
        });
    (unsupported && no_sources && no_attempts) || placeholder_reason
}

/// Workers regularly send `field_status` alone and forget `result` (or send
/// `result.fields` empty). The verified values and their sources are already
/// in `field_status`, so `result.fields` is derived from them instead of
/// rejecting the run (07.09.2026: eighteen rejections in seven minutes for one
/// lead, most of them "missing field `result`").
fn derive_result_fields_from_field_status(request: &mut ResearchWritebackRequest) {
    let empty = request
        .result
        .fields
        .as_object()
        .map(|fields| fields.is_empty())
        .unwrap_or(true);
    if !empty {
        return;
    }
    let mut fields = Map::new();
    for (key, status) in &request.field_status {
        if !status.status.trim().eq_ignore_ascii_case("verified") || status.value.is_null() {
            continue;
        }
        let mut entry = Map::new();
        entry.insert("value".to_string(), status.value.clone());
        entry.insert(
            "sources".to_string(),
            serde_json::to_value(&status.sources).unwrap_or(Value::Array(Vec::new())),
        );
        if let Some(person_key) = status.extra.get("person_key") {
            entry.insert("person_key".to_string(), person_key.clone());
        }
        fields.insert(key.clone(), Value::Object(entry));
    }
    request.result.fields = Value::Object(fields);
}

/// A writeback in which nothing is verified and no field carries a single
/// source or documented attempt is not a research result. It happened on
/// 07.09.2026 when a worker attempt resumed after a service restart without
/// its context and closed two leads with 32x `no_match` and zero evidence; the
/// skill demands a documented search and two page reads before `no_match`.
fn reject_evidence_free_writeback(request: &ResearchWritebackRequest) -> anyhow::Result<()> {
    let any_verified = request
        .field_status
        .values()
        .any(|status| status.status.trim().eq_ignore_ascii_case("verified"));
    let any_evidence = request
        .field_status
        .values()
        .any(|status| !status.sources.is_empty() || !status.attempts.is_empty())
        || !request.result.evidence.is_empty();
    anyhow::ensure!(
        any_verified || any_evidence,
        "evidence-free research writeback rejected: no field is verified and no field carries a source or a documented attempt; a `no_match` needs the documented search and page reads that led to it"
    );
    Ok(())
}

pub(super) fn handle_research_writeback(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    // The worker only ever sees this message. Without the serde detail it
    // resent the same malformed payload four times on 07.09.2026 (a stray
    // `firma_land` key nested inside another field's status object).
    let request: ResearchWritebackRequest = serde_json::from_value(hoist_top_level_field_entries(
        unwrap_item_carriers(command.payload.clone()),
    ))
    .map_err(|error| {
        anyhow::anyhow!("invalid outbound.lead.research_writeback payload: {error}")
    })?;
    anyhow::ensure!(
        request.module == "outbound-lead-generation" && command.module == request.module,
        "research writeback module must be outbound-lead-generation"
    );
    anyhow::ensure!(
        command.record_id.as_deref() == Some(request.record_id.as_str()),
        "research writeback record_id does not match command record_id"
    );
    let _guard = ActiveResearchCommandGuard::claim(root, &request.record_id)
        .context("another person research or research writeback is active for this record_id")?;

    validate_original_research_command(root, &request)?;
    let mut lead = store::load_rxdb_collection_record(root, LEAD_COLLECTION, &request.record_id)?
        .context("research writeback lead record does not exist")?;
    let gap_task = if request.gap_task_id.trim().is_empty() {
        // Chat assignment: the harness worker researched the lead directly
        // (skill `outbound-lead-generation-research`). No queue task, no
        // phase precondition; the field set is what the worker reports, and
        // every reported field must be a canonical research field.
        None
    } else {
        anyhow::ensure!(
            lead.get("research_phase").and_then(Value::as_str) == Some("gap_closure"),
            "lead is not in research_phase gap_closure"
        );
        anyhow::ensure!(
            lead.get("gap_task_id").and_then(Value::as_str) == Some(request.gap_task_id.as_str()),
            "research writeback gap_task_id does not match lead"
        );
        let task = channels::load_queue_task(root, &request.gap_task_id)?
            .context("research writeback gap task does not exist")?;
        let contract = task
            .metadata
            .get(GAP_METADATA_KEY)
            .cloned()
            .context("gap task is missing person_research_gap_closure metadata")?;
        validate_task_correlation(&contract, &request)?;
        Some((task, contract))
    };
    let requested_fields = match &gap_task {
        Some((_, contract)) => contract
            .get("requested_fields")
            .and_then(Value::as_array)
            .context("gap task requested_fields array is missing")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .context("gap task requested_fields must contain strings")
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
        // Chat assignment: the field set is what the app asked for
        // (`payload.fields` of the research command), not what the worker
        // chose to report. Otherwise a worker could silently close a lead with
        // a handful of fields (observed on thesen: 21 of 32).
        None => research_command_requested_fields(root, &request.research_command_id)?
            .unwrap_or_else(|| request.field_status.keys().cloned().collect::<Vec<_>>()),
    };
    anyhow::ensure!(
        !requested_fields.is_empty(),
        "research writeback field_status must not be empty"
    );
    // Erst retten, dann pruefen: Einzelverstoesse duerfen die Arbeit einer
    // ganzen Firma nicht mehr vernichten (Befund 03.09.2026, 14 von 19).
    let mut request = request;
    derive_result_fields_from_field_status(&mut request);
    reject_evidence_free_writeback(&request)?;
    let delivered_fields = request
        .field_status
        .keys()
        .cloned()
        .collect::<BTreeSet<String>>();
    // "nicht geliefert" is not a defect: a follow-up writeback legitimately
    // carries a subset of the requested fields. Workers read a list of 31
    // rejections as a failed call and resend the same field over and over
    // (07.09.2026: Aeroxon, seven identical one-field writebacks in five
    // minutes), so undelivered fields are reported separately as `open_fields`.
    let research_payload = research_command_payload(root, &request.research_command_id)?;
    let crm = CrmBaseline::from_research_payload(&research_payload);
    let rejections = sanitize_research_writeback(&mut request, &requested_fields, &crm)?
        .into_iter()
        .filter(|entry| !entry.ends_with(": nicht geliefert"))
        .collect::<Vec<String>>();
    validate_field_status_keys(&requested_fields, &request.field_status)?;
    validate_result_shape(&request.result, &requested_fields, &crm)?;
    validate_result_field_status_consistency(&request.result, &request.field_status)?;
    if let Some((task, contract)) = &gap_task {
        let workspace = task_workspace(root, task, contract)?;
        for (field, status) in &request.field_status {
            validate_terminal_field(field, status, &workspace, contract)?;
        }
    }
    // The chat assignment carries the Sellify persons on the research command;
    // only the gap task copied them into its contract. Without this the nine
    // Sasol persons Sellify holds never reached the lead (11.09.2026).
    let known_person_records = gap_task
        .as_ref()
        .and_then(|(_, contract)| contract.get("known_person_records").cloned())
        .or_else(|| {
            research_payload
                .get("known_person_records")
                .filter(|records| records.is_array())
                .cloned()
        })
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let mut projection_result = serde_json::json!({
        "fields": request.result.fields,
        "person_records": request.result.person_records,
        "evidence": request.result.evidence,
        "known_person_records": known_person_records,
    });
    add_field_status_evidence(&mut projection_result, &request.field_status)?;
    let now = super::person_research_command::now_ms();
    let previous_keys = previous_research_keys(&lead);
    let patch = outbound_lead_generation_research_outcome_patch(&lead, &projection_result, now);
    merge_json_object_values(&mut lead, &patch);
    union_research_keys(&mut lead, &previous_keys);
    let merged_field_status = merge_field_status(
        lead.get("field_status"),
        serde_json::to_value(&request.field_status)?,
    );
    lead["field_status"] = merged_field_status;
    // A field is answered only when the lead now carries a verified value or a
    // documented `no_match`. Everything else stays open — including a field the
    // evidence gate downgraded from `verified` because it carried a single
    // source host. Reporting those as answered told the worker there was
    // nothing left to do and ended the run with 3 of 32 fields stored while 10
    // verified answers had been dropped (thesen, Sasol Germany, 09.09.2026).
    let open_fields = requested_fields
        .iter()
        .filter(|field| {
            lead["field_status"]
                .get(field.as_str())
                .map(|status| !research_field_is_answered(status))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<String>>();
    let accepted_fields = requested_fields
        .iter()
        .filter(|field| {
            lead["field_status"]
                .get(field.as_str())
                .map(research_field_is_answered)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<String>>();
    if gap_task.is_some() {
        lead["gap_task_id"] = Value::String(request.gap_task_id.clone());
    }
    lead["research_phase"] = Value::Null;
    // Ein verworfenes oder fehlendes Feld ist ein Grund zur Pruefung durch einen
    // Menschen - sonst haette die Rettung "abgeschlossen" gemeldet, obwohl
    // Felder fehlen.
    let accepted_count = accepted_fields.len();
    let open_count = open_fields.len();
    // Judged on the lead's merged state, not on this call's subset: a closing
    // writeback that only resolved two conflicts turned a lead with 21
    // `no_match` fields into `completed` (Sasol, 11.09.2026).
    let needs_review = !rejections.is_empty()
        || !open_fields.is_empty()
        || lead["field_status"]
            .as_object()
            .into_iter()
            .flatten()
            .any(|(field, status)| {
                requested_fields.contains(field)
                    && matches!(
                        status.get("status").and_then(Value::as_str),
                        Some("no_match" | "action_required")
                    )
            });
    let terminal_lead_status = if needs_review {
        "needs_review"
    } else {
        "completed"
    };
    lead["research_status"] = Value::String(terminal_lead_status.to_string());
    lead["payload"]["native_research_terminal_status"] =
        Value::String(terminal_lead_status.to_string());
    lead["payload"]["research_finished_at_ms"] = Value::Number(now.into());
    lead["research_error"] = Value::Null;
    lead["research_updated_at_ms"] = Value::Number(now.into());
    store::upsert_rxdb_collection_record(root, LEAD_COLLECTION, &request.record_id, now, lead)?;
    super::contact_email_validation::spawn_contact_email_validation(root, &request.record_id);

    Ok(serde_json::json!({
        "ok": true,
        "record_id": request.record_id,
        "research_command_id": request.research_command_id,
        "gap_task_id": request.gap_task_id,
        "research_status": if needs_review { "needs_review" } else { "completed" },
        "field_status": request
            .field_status
            .iter()
            .filter(|(key, _)| delivered_fields.contains(key.as_str()))
            .collect::<BTreeMap<_, _>>(),
        "accepted_fields": accepted_fields,
        "delivered_fields": delivered_fields.iter().cloned().collect::<Vec<String>>(),
        "open_fields": open_fields,
        // Der Agent erfaehrt genau, was verworfen wurde, und kann im selben
        // Auftrag nachliefern statt die ganze Firma neu zu recherchieren.
        "rejections": rejections,
        "summary": format!(
            "{} Feld(er) gespeichert, {} Beleg(e) verworfen, {} Feld(er) noch offen. Ein Feld gilt erst als beantwortet, wenn es verifiziert ist (zwei unabhaengige Quell-Hosts; bei Selbstauskuenften wie Telefon, E-Mail, Domain und allen person_-Feldern genuegt ein Beleg von der Unternehmensseite bzw. dem Profil) oder als no_match belegt wurde. Offene Felder sind keine Ablehnung: hole die fehlende Zweitquelle bzw. den Wert und sende sie gesammelt in einem weiteren Aufruf; gespeicherte Felder nicht erneut senden.",
            accepted_count,
            rejections.len(),
            open_count
        ),
    }))
}

pub(super) fn cancel_open_gap_task_for_new_research(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<bool> {
    let Some(record_id) = command.record_id.as_deref() else {
        return Ok(false);
    };
    let Some(lead) = store::load_rxdb_collection_record(root, LEAD_COLLECTION, record_id)? else {
        return Ok(false);
    };
    if lead.get("research_phase").and_then(Value::as_str) != Some("gap_closure") {
        return Ok(false);
    }
    let Some(task_id) = lead
        .get("gap_task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(false);
    };
    let Some(task) = channels::load_queue_task(root, task_id)? else {
        return Ok(false);
    };
    if channels::route_status_is_terminal(&task.route_status) {
        return Ok(false);
    }
    channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task.message_key,
            route_status: Some("cancelled".to_string()),
            status_note: Some(format!(
                "Cancelled because a new person research command `{}` superseded this gap closure.",
                command.id.as_deref().unwrap_or_default()
            )),
            ..Default::default()
        },
    )?;
    Ok(true)
}

fn load_gap_task_by_idempotency_key(
    root: &Path,
    idempotency_key: &str,
) -> anyhow::Result<Option<channels::QueueTaskView>> {
    let task_count = channels::count_queue_tasks(root, &[])?;
    if task_count == 0 {
        return Ok(None);
    }
    Ok(channels::list_queue_tasks(root, &[], task_count)?
        .into_iter()
        .find(|task| {
            task.metadata.get("idempotency_key").and_then(Value::as_str) == Some(idempotency_key)
        }))
}

/// The payload of the research command a writeback answers (`Null` when the
/// command is unknown or unreadable).
fn research_command_payload(root: &Path, research_command_id: &str) -> anyhow::Result<Value> {
    let conn = store::open_store(root)?;
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_commands WHERE command_id = ?1",
            params![research_command_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(payload_json
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .unwrap_or(Value::Null))
}

/// The canonical field set of a chat assignment: the app puts the requested
/// field keys into `payload.fields` when it dispatches the research command.
/// Returns `None` when the command carries no explicit field list, so the
/// caller can fall back to what the worker reported.
fn research_command_requested_fields(
    root: &Path,
    research_command_id: &str,
) -> anyhow::Result<Option<Vec<String>>> {
    let conn = store::open_store(root)?;
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_commands WHERE command_id = ?1",
            params![research_command_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(payload_json) = payload_json else {
        return Ok(None);
    };
    let payload: Value = serde_json::from_str(&payload_json)?;
    let fields = payload
        .get("fields")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|fields| !fields.is_empty());
    Ok(fields)
}

fn validate_original_research_command(
    root: &Path,
    request: &ResearchWritebackRequest,
) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    let original = conn
        .query_row(
            "SELECT command_type, module, record_id FROM business_commands WHERE command_id = ?1",
            params![request.research_command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let (command_type, module, record_id) = original.with_context(|| {
        format!(
            "research_command_id `{}` does not exist",
            request.research_command_id
        )
    })?;
    anyhow::ensure!(
        matches!(
            command_type.as_str(),
            "web_stack.person_research" | "business_os.chat.task"
        ),
        "research_command_id must reference web_stack.person_research or the chat assignment (business_os.chat.task)"
    );
    anyhow::ensure!(
        module == request.module && record_id == request.record_id,
        "research_command_id does not belong to the same module and record_id"
    );
    Ok(())
}

fn validate_task_correlation(
    contract: &Value,
    request: &ResearchWritebackRequest,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        contract.get("record_id").and_then(Value::as_str) == Some(request.record_id.as_str()),
        "gap task record_id does not match writeback"
    );
    anyhow::ensure!(
        contract.get("research_command_id").and_then(Value::as_str)
            == Some(request.research_command_id.as_str()),
        "gap task research_command_id does not match writeback"
    );
    anyhow::ensure!(
        contract.get("module").and_then(Value::as_str) == Some(request.module.as_str()),
        "gap task module does not match writeback"
    );
    anyhow::ensure!(
        contract.get("gap_task_id").and_then(Value::as_str) == Some(request.gap_task_id.as_str()),
        "gap task id does not match writeback"
    );
    Ok(())
}

/// Rettet die Arbeit einer Recherche, deren Rueckschreiben in Details gegen den
/// Vertrag verstoesst.
///
/// Gemessen am 03.09.2026 auf einem Kundenmandanten: von 19 Chemie-Firmen endeten 14 als
/// "failed", obwohl die Recherche gelaufen war - eine Firma hatte 19 Felder und
/// 32 Feldstatus und verlor trotzdem alles, weil EIN Beleg keine http-URL trug.
/// Die haeufigsten Gruende (12 x person_key ungleich, 7 x person_key fehlt,
/// 4 x Wert weicht ab, 3 x Wert auf nicht-verifiziertem Feld, 1 x Beleg-URL)
/// betreffen immer nur EINZELNE Felder oder Belege.
///
/// Diese Bereinigung wirft deshalb genau das Betroffene weg statt des Ganzen.
/// Die Schutzrichtung bleibt: nichts Unbelegtes wird "verified". Ein verworfenes
/// Feld faellt auf `unsupported` mit Begruendung, der Lead landet zwingend in
/// `needs_review`, und die Ablehnungen gehen als Liste an den Aufrufer zurueck,
/// damit der Agent im naechsten Zug genau die Luecken schliessen kann.
fn sanitize_research_writeback(
    request: &mut ResearchWritebackRequest,
    requested_fields: &[String],
    crm: &CrmBaseline,
) -> anyhow::Result<Vec<String>> {
    let mut rejections: Vec<String> = Vec::new();
    let requested: BTreeSet<&String> = requested_fields.iter().collect();
    let mut verworfene_felder: BTreeSet<String> = BTreeSet::new();

    let fields = request
        .result
        .fields
        .as_object_mut()
        .context("research writeback result.fields must be an object")?;
    let mut entfernen: Vec<String> = Vec::new();
    for (field, value) in fields.iter() {
        let grund = if !requested.contains(field) {
            Some("nicht angefordert".to_string())
        } else if parse_requested_fields(&[field.clone()]).is_err() {
            Some("kein bekanntes Recherchefeld".to_string())
        } else if !value.is_object() {
            Some("kein strukturiertes Objekt".to_string())
        } else if value
            .get("value")
            .is_some_and(|inner| !inner.is_null() && !research_value_is_populated(inner))
        {
            Some("Wert ist weder leer noch ein befuellter Skalar".to_string())
        } else if field.starts_with("person_")
            && value.get("value").is_some_and(research_value_is_populated)
            && !value
                .get("person_key")
                .and_then(Value::as_str)
                .is_some_and(|key| !key.trim().is_empty())
        {
            Some("person_key fehlt".to_string())
        } else {
            None
        };
        if let Some(grund) = grund {
            rejections.push(format!("result.fields.{field}: {grund}"));
            entfernen.push(field.clone());
        }
    }
    for field in entfernen {
        fields.remove(&field);
        verworfene_felder.insert(field);
    }

    let vorher = request.result.person_records.len();
    request.result.person_records.retain(|person| {
        person
            .get("person_key")
            .and_then(Value::as_str)
            .is_some_and(|key| !key.trim().is_empty())
    });
    if request.result.person_records.len() != vorher {
        rejections.push(format!(
            "result.person_records: {} Eintraege ohne person_key verworfen",
            vorher - request.result.person_records.len()
        ));
    }

    let mut belege: Vec<Value> = Vec::new();
    for (index, evidence) in request.result.evidence.iter().enumerate() {
        let field = evidence
            .get("field_key")
            .or_else(|| evidence.get("field"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let raw_url = evidence
            .get("url")
            .or_else(|| evidence.get("source_url"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let crm_citation = match (field.as_deref(), raw_url) {
            (Some(field), Some(url)) => crm.check(
                field,
                url,
                evidence.get("quote").and_then(Value::as_str).unwrap_or(""),
            ),
            _ => None,
        };
        let url_ok = crm_citation == Some(true)
            || raw_url
                .and_then(|value| url::Url::parse(value).ok())
                .is_some_and(|url| {
                    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
                });
        let grund = match field.as_deref() {
            None => Some("field_key fehlt".to_string()),
            Some(field) if !requested.contains(&field.to_string()) => {
                Some(format!("Feld {field} war nicht angefordert"))
            }
            Some(field) if verworfene_felder.contains(field) => {
                Some(format!("Feld {field} wurde bereits verworfen"))
            }
            Some(field) if parse_requested_fields(&[field.to_string()]).is_err() => {
                Some(format!("Feld {field} ist kein bekanntes Recherchefeld"))
            }
            Some(_) if crm_citation == Some(false) => Some(format!(
                "Sellify-Beleg passt zu keinem Wert, den der Auftrag aus Sellify mitgebracht hat ({})",
                crm.hint()
            )),
            Some(_) if !url_ok => Some("Beleg-URL ist keine http(s)-Adresse".to_string()),
            Some(_)
                if !evidence
                    .get("source_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()) =>
            {
                Some("source_id fehlt".to_string())
            }
            Some(_)
                if !evidence
                    .get("quote")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()) =>
            {
                Some("Zitat fehlt".to_string())
            }
            Some(field)
                if field.starts_with("person_")
                    && !evidence
                        .get("person_key")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty()) =>
            {
                Some("person_key fehlt".to_string())
            }
            Some(_) => None,
        };
        match grund {
            Some(grund) => rejections.push(format!("result.evidence[{index}]: {grund}")),
            None => belege.push(evidence.clone()),
        }
    }
    request.result.evidence = belege;

    // An unchecked Sellify citation is dropped, not merely left uncounted: it
    // would otherwise reach the lead's evidence and read as a CRM confirmation
    // that does not exist (Sasol, 11.09.2026: `sellify://person/<import key>`).
    for (field, status) in request.field_status.iter_mut() {
        let before = status.sources.len();
        status
            .sources
            .retain(|source| crm.check(field, &source.url, &source.quote) != Some(false));
        let dropped = before - status.sources.len();
        if dropped > 0 {
            rejections.push(format!(
                "field_status.{field}: {dropped} Sellify-Beleg(e) verworfen ({})",
                crm.hint()
            ));
        }
    }
    for person in request.result.person_records.iter_mut() {
        let Some(person) = person.as_object_mut() else {
            continue;
        };
        for list in ["evidence", "sources"] {
            let Some(entries) = person.get_mut(list).and_then(Value::as_array_mut) else {
                continue;
            };
            let before = entries.len();
            entries.retain(|entry| {
                let url = entry
                    .get("url")
                    .or_else(|| entry.get("source_url"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let field = entry
                    .get("field_key")
                    .or_else(|| entry.get("field"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let quote = entry.get("quote").and_then(Value::as_str).unwrap_or("");
                crm.check(field, url, quote) != Some(false)
            });
            if entries.len() != before {
                rejections.push(format!(
                    "result.person_records: {} Sellify-Beleg(e) verworfen ({})",
                    before - entries.len(),
                    crm.hint()
                ));
            }
        }
    }

    // Sellify agrees -> Sellify is a source, whether or not the worker wrote
    // the citation. Six live runs on 11.09.2026 produced no usable
    // `sellify://` citation (none, or the lead id, an import key, a
    // paraphrase); the server holds the CRM record and compares the value
    // itself. Sellify still never counts alone (see crm_aware_provider_count).
    let field_person_keys: BTreeMap<String, String> = request
        .result
        .fields
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(field, entry)| {
            entry
                .get("person_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(|key| (field.clone(), key.to_string()))
        })
        .collect();
    for (field, status) in request.field_status.iter_mut() {
        if status.status != "verified"
            || status
                .sources
                .iter()
                .any(|source| crm.check(field, &source.url, &source.quote) == Some(true))
        {
            continue;
        }
        let value = match &status.value {
            Value::String(text) => text.trim().to_string(),
            Value::Number(number) => number.to_string(),
            _ => continue,
        };
        let person_key = field_person_keys.get(field).cloned().or_else(|| {
            status
                .sources
                .iter()
                .find_map(|source| source.person_key.clone())
        });
        if let Some(source) = crm.agreeing_source(field, &value, person_key.as_deref()) {
            status.sources.push(source);
        }
    }

    // Feldstatus gegen das Ergebnis abgleichen. Ein Widerspruch verwirft NUR
    // dieses Feld, nie die ganze Firma.
    let fields_snapshot = request.result.fields.clone();
    let leer = serde_json::Map::new();
    let fields_ro = fields_snapshot.as_object().unwrap_or(&leer);
    let mut demotieren: Vec<(String, String)> = Vec::new();
    for (field, status) in request.field_status.iter() {
        if !requested.contains(field) {
            demotieren.push((field.clone(), "nicht angefordert".to_string()));
            continue;
        }
        let eintrag = fields_ro.get(field);
        let ergebniswert = eintrag.and_then(|entry| entry.get("value"));
        if status.status == "verified" {
            if verworfene_felder.contains(field) {
                demotieren.push((field.clone(), "Ergebnisfeld wurde verworfen".to_string()));
                continue;
            }
            if let Some(wert) = ergebniswert.filter(|value| !value.is_null()) {
                if wert != &status.value {
                    demotieren.push((
                        field.clone(),
                        "Wert in result und field_status stimmen nicht ueberein".to_string(),
                    ));
                    continue;
                }
            }
            // Am 03.09.2026 auf einem Kundenmandanten nachgezaehlt: von 265 als "verified"
            // gemeldeten Feldern trugen 56 nur EINE Quelle und 100 weniger als
            // zwei verschiedene Quell-Hosts. Der Vertrag verlangt zwei
            // unabhaengige Hosts - geprueft wurde das aber nur auf dem
            // Warteschlangenweg (validate_terminal_field). Die Chemie-Kampagne
            // lief ueber den Chatweg, wo die Regel schlicht nicht existierte.
            // "verified" bedeutete damit nicht, was es behauptet.
            // A checked Sellify record is one host of its own; an unchecked
            // Sellify citation is none (it would read as host `company`).
            let hosts = status
                .sources
                .iter()
                .filter_map(
                    |source| match crm.check(field, &source.url, &source.quote) {
                        Some(true) => Some(CRM_SOURCE_SCHEME.to_string()),
                        Some(false) => None,
                        None => url::Url::parse(source.url.trim()).ok().and_then(|url| {
                            url.host_str()
                                .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
                        }),
                    },
                )
                .collect::<BTreeSet<_>>();
            // A probe against the writeback contract is not evidence. One bad
            // field is demoted; the other thirty-one survive.
            if let Some(host) = hosts
                .iter()
                .find(|host| host_is_reserved_for_documentation(host))
            {
                demotieren.push((
                    field.clone(),
                    format!("Beleg vom Dokumentations-Host `{host}` beweist nichts"),
                ));
                continue;
            }
            let benoetigt = super::person_research_command::required_independent_sources(field);
            let unabhaengig = super::person_research_command::crm_aware_provider_count(
                hosts.len(),
                hosts.iter().map(String::as_str),
            );
            if unabhaengig == 0 && !hosts.is_empty() {
                demotieren.push((
                    field.clone(),
                    "Sellify allein belegt nichts: eine externe Quelle muss den Wert bestaetigen"
                        .to_string(),
                ));
                continue;
            }
            if unabhaengig < benoetigt {
                demotieren.push((
                    field.clone(),
                    format!(
                        "verified verlangt {benoetigt} unabhaengige Quell-Hosts, gefunden: {}",
                        hosts.len()
                    ),
                ));
                continue;
            }
            if field.starts_with("person_") {
                let person_key = eintrag
                    .and_then(|entry| entry.get("person_key"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                match person_key {
                    None => {
                        demotieren
                            .push((field.clone(), "person_key im Ergebnis fehlt".to_string()));
                        continue;
                    }
                    Some(person_key) => {
                        if !status.sources.iter().all(|source| {
                            source.person_key.as_deref().map(str::trim) == Some(person_key)
                        }) {
                            demotieren.push((
                                field.clone(),
                                "person_key in Beleg und Ergebnis nicht identisch".to_string(),
                            ));
                            continue;
                        }
                    }
                }
            }
        }
    }
    for (field, grund) in demotieren {
        rejections.push(format!("field_status.{field}: {grund}"));
        verworfene_felder.insert(field.clone());
        if let Some(map) = request.result.fields.as_object_mut() {
            map.remove(&field);
        }
        if requested.contains(&field) {
            let status = request.field_status.entry(field).or_insert(FieldStatus {
                status: String::new(),
                value: Value::Null,
                sources: Vec::new(),
                attempts: Vec::new(),
                reason: String::new(),
                extra: BTreeMap::new(),
            });
            status.status = "unsupported".to_string();
            status.value = Value::Null;
            status.sources.clear();
            status.reason = "Vom Rueckschreiben verworfen, siehe rejections".to_string();
        } else {
            request.field_status.remove(&field);
        }
    }

    // Nicht-verifizierte Felder duerfen keinen Wert tragen: den Wert entfernen,
    // nicht das Feld verlieren.
    for (field, status) in request.field_status.iter_mut() {
        if status.status == "verified" {
            continue;
        }
        if research_value_is_populated(&status.value) {
            rejections.push(format!(
                "field_status.{field}: Wert auf nicht-verifiziertem Feld entfernt"
            ));
            status.value = Value::Null;
        }
        if let Some(map) = request.result.fields.as_object_mut() {
            if let Some(entry) = map.get_mut(field) {
                if entry.get("value").is_some_and(research_value_is_populated) {
                    rejections.push(format!(
                        "result.fields.{field}: Wert auf nicht-verifiziertem Feld entfernt"
                    ));
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert("value".to_string(), Value::Null);
                    }
                }
            }
        }
    }

    // Nicht gelieferte Felder ergaenzen, damit der Feldsatz vollstaendig bleibt.
    for field in requested_fields {
        if request.field_status.contains_key(field) {
            continue;
        }
        rejections.push(format!("field_status.{field}: nicht geliefert"));
        request.field_status.insert(
            field.clone(),
            FieldStatus {
                status: "unsupported".to_string(),
                value: Value::Null,
                sources: Vec::new(),
                attempts: Vec::new(),
                reason: "Vom Rueckschreiben nicht geliefert".to_string(),
                extra: BTreeMap::new(),
            },
        );
    }

    Ok(rejections)
}

fn validate_field_status_keys(
    requested_fields: &[String],
    field_status: &BTreeMap<String, FieldStatus>,
) -> anyhow::Result<()> {
    parse_requested_fields(requested_fields)?;
    let submitted = field_status.keys().cloned().collect::<Vec<_>>();
    parse_requested_fields(&submitted)?;
    let requested = requested_fields.iter().cloned().collect::<BTreeSet<_>>();
    let submitted = submitted.into_iter().collect::<BTreeSet<_>>();
    let missing = requested
        .difference(&submitted)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = submitted
        .difference(&requested)
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        missing.is_empty() && unexpected.is_empty(),
        "field_status must cover all requested_fields exactly; missing={missing:?}, unexpected={unexpected:?}"
    );
    Ok(())
}

fn validate_terminal_field(
    field: &str,
    status: &FieldStatus,
    workspace: &Path,
    contract: &Value,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        TERMINAL_FIELD_STATUSES.contains(&status.status.as_str()),
        "field `{field}` has non-terminal or unsupported status `{}`",
        status.status
    );
    for attempt in &status.attempts {
        validate_attempt(field, attempt, workspace)?;
    }
    match status.status.as_str() {
        "verified" => {
            anyhow::ensure!(
                research_value_is_populated(&status.value),
                "verified field `{field}` requires a populated scalar value"
            );
            let evidence = status
                .sources
                .iter()
                .map(|source| {
                    serde_json::json!({
                        "field_key": field,
                        "source_id": source.source_id,
                        "source_url": source.url,
                        "quote": source.quote,
                        "person_key": source.person_key,
                    })
                })
                .collect::<Vec<_>>();
            for source in &status.sources {
                validate_source(field, source)?;
                if field.starts_with("person_") {
                    anyhow::ensure!(
                        source
                            .person_key
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty()),
                        "verified person field `{field}` evidence requires person_key"
                    );
                }
            }
            let required = super::person_research_command::required_independent_sources(field);
            anyhow::ensure!(
                independent_research_evidence_count(&evidence, field) >= required,
                "verified field `{field}` requires at least {required} independent sources on different hosts"
            );
        }
        "no_match" => validate_no_match(field, status)?,
        "unsupported" => {},
        "action_required" => anyhow::ensure!(
            action_required_has_auth_reference(status, contract),
            "action_required field `{field}` requires an auth-assist reference or requires_credential source"
        ),
        _ => unreachable!(),
    }
    Ok(())
}

/// Sellify, the customer's CRM, is the starting value of every field and one
/// source (THESEN procedure, step 0; skill: "Sellify alone proves nothing, but
/// counts as one source"). A CRM record has no web address, and every evidence
/// check demanded HTTP(S), so no worker could cite it: a value held in Sellify
/// and confirmed by Northdata ended as `no_match` (thesen, Sasol Germany,
/// 11.09.2026: address, postcode, phone, WZ code, revenue, person e-mails).
///
/// A Sellify record is cited as `sellify://company/<contact_id>` or
/// `sellify://person/<sellify_person_id>`. The citation counts only for a
/// record the research command carried (`sellify_company`,
/// `known_person_records`), and only when its quote is that record's value for
/// the cited field, so a worker cannot manufacture a CRM source.
pub(super) const CRM_SOURCE_SCHEME: &str = "sellify";

#[derive(Debug, Default)]
struct CrmBaseline {
    /// `company/<contact_id>` or `person/<sellify_person_id>` → field → value.
    records: BTreeMap<String, BTreeMap<String, String>>,
}

impl CrmBaseline {
    fn from_research_payload(payload: &Value) -> Self {
        let scalar_fields = |record: &Value| -> BTreeMap<String, String> {
            record
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(key, value)| {
                    let text = match value {
                        Value::String(text) => text.trim().to_string(),
                        Value::Number(number) => number.to_string(),
                        _ => return None,
                    };
                    (!text.is_empty()).then(|| (key.clone(), text))
                })
                .collect()
        };
        let mut records = BTreeMap::new();
        if let Some(company) = payload.get("sellify_company").filter(|v| v.is_object()) {
            let contact_id = company
                .get("contact_id")
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| value.as_i64().map(|id| id.to_string()))
                })
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty());
            if let Some(contact_id) = contact_id {
                let fields = scalar_fields(company);
                // Workers cite the company by the lead they research as often
                // as by its CRM number (Sasol, 11.09.2026). The quote is still
                // checked against this very record.
                if let Some(lead_id) = payload
                    .get("lead_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    records.insert(format!("company/{lead_id}"), fields.clone());
                }
                records.insert(format!("company/{contact_id}"), fields);
            }
        }
        for person in payload
            .get("known_person_records")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let person_id = person
                .get("sellify_person_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty());
            if let Some(person_id) = person_id {
                records.insert(format!("person/{person_id}"), scalar_fields(person));
            }
        }
        Self { records }
    }

    /// `None`: not a Sellify citation. `Some(true)`: the command carried this
    /// record and the quote is its value for `field`. `Some(false)`: a Sellify
    /// citation that proves nothing.
    fn check(&self, field: &str, url: &str, quote: &str) -> Option<bool> {
        let url = url.trim();
        let prefix = format!("{CRM_SOURCE_SCHEME}://");
        if !url.to_ascii_lowercase().starts_with(&prefix) {
            return None;
        }
        let path = url[prefix.len()..].trim_end_matches('/');
        let Some(record) = self.records.get(path) else {
            return Some(false);
        };
        let quote = crm_comparable(quote);
        if quote.chars().count() < 2 {
            return Some(false);
        }
        // The quote carries the stored value, or is at least half of it: a
        // fragment such as "+49 40" sits in every Hamburg number and proves
        // nothing about this one.
        let matches = crm_record_keys(field).iter().any(|key| {
            record.get(*key).is_some_and(|value| {
                let value = crm_comparable(value);
                let value_len = value.chars().count();
                value_len >= 2
                    && (quote.contains(&value)
                        || (value.contains(&quote) && quote.chars().count() * 2 >= value_len))
            })
        });
        Some(matches)
    }
}

impl CrmBaseline {
    /// The Sellify source for `value` when the carried record holds the same
    /// value for `field`: the company record for company fields, the person
    /// named by `person_key` (a `sellify-person-…` id) for person fields.
    fn agreeing_source(
        &self,
        field: &str,
        value: &str,
        person_key: Option<&str>,
    ) -> Option<FieldSource> {
        let record_key = if field.starts_with("person_") {
            let key = format!("person/{}", person_key?.trim());
            self.records.contains_key(&key).then_some(key)?
        } else {
            self.records
                .keys()
                .find(|key| {
                    key.strip_prefix("company/")
                        .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
                })?
                .clone()
        };
        let url = format!("{CRM_SOURCE_SCHEME}://{record_key}");
        if self.check(field, &url, value) != Some(true) {
            return None;
        }
        let record = self.records.get(&record_key)?;
        let quote = crm_record_keys(field)
            .iter()
            .filter_map(|key| record.get(*key))
            .find(|stored| {
                let stored = crm_comparable(stored);
                let value = crm_comparable(value);
                value.contains(&stored)
                    || (stored.contains(&value)
                        && value.chars().count() * 2 >= stored.chars().count())
            })?
            .clone();
        Some(FieldSource {
            source_id: CRM_SOURCE_SCHEME.to_string(),
            url,
            quote,
            person_key: person_key.map(str::to_string),
            requires_credential: false,
            task_id: String::new(),
            command_id: String::new(),
        })
    }

    /// The citations this assignment allows, for a rejection the worker can act on.
    fn hint(&self) -> String {
        if self.records.is_empty() {
            return "der Auftrag bringt keinen Sellify-Datensatz mit".to_string();
        }
        let mut urls = self
            .records
            .keys()
            .filter(|key| {
                key.starts_with("company/") && key[8..].chars().all(|c| c.is_ascii_digit())
            })
            .chain(
                self.records
                    .keys()
                    .filter(|key| key.starts_with("person/"))
                    .take(3),
            )
            .map(|key| format!("{CRM_SOURCE_SCHEME}://{key}"))
            .collect::<Vec<_>>();
        if self
            .records
            .keys()
            .filter(|key| key.starts_with("person/"))
            .count()
            > 3
        {
            urls.push("…".to_string());
        }
        format!(
            "gueltig sind nur {}; das Zitat muss den dort gespeicherten Wert des Feldes enthalten",
            urls.join(", ")
        )
    }
}

/// The Sellify keys that can support a research field. The address fields
/// share the one Sellify address; person fields carry their own name.
fn crm_record_keys(field: &str) -> &'static [&'static str] {
    match field {
        "firma_name" => &["name"],
        "firma_anschrift" | "firma_besucheranschrift" | "firma_postanschrift" => {
            &["anschrift", "plz", "ort"]
        }
        "firma_plz" => &["plz"],
        "firma_ort" => &["ort"],
        "firma_land" => &["land"],
        "firma_email" => &["email"],
        "firma_domain" => &["domain"],
        "firma_telefon" => &["telefon"],
        "firma_fax" => &["fax"],
        "wz_code" => &["wz_code"],
        "mitarbeiter" => &["mitarbeiter"],
        "umsatz" => &["umsatz"],
        "person_vorname" => &["person_vorname"],
        "person_nachname" => &["person_nachname"],
        "person_funktion" => &["person_funktion"],
        "person_position" => &["person_position", "person_funktion"],
        "person_email" => &["person_email"],
        "person_telefon" => &["person_telefon"],
        _ => &[],
    }
}

/// Letters and digits only, lower case: "+49 40 63684-1000" and the stored
/// "+4940636841000" are the same number.
fn crm_comparable(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn validate_source(field: &str, source: &FieldSource) -> anyhow::Result<()> {
    anyhow::ensure!(
        !source.source_id.trim().is_empty(),
        "verified field `{field}` source_id must not be empty"
    );
    anyhow::ensure!(
        !source.quote.trim().is_empty(),
        "verified field `{field}` quote must not be empty"
    );
    let url = url::Url::parse(source.url.trim())
        .with_context(|| format!("verified field `{field}` has invalid source URL"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "verified field `{field}` source URL must be HTTP(S)"
    );
    Ok(())
}

/// Hosts RFC 2606 and RFC 6761 reserve for documentation and testing. A worker
/// probing the writeback contract sends them; on the Aeroxon lead 09.09.2026
/// one such probe claimed `firma_name` as verified from
/// `https://example.com` with the quote "test", and its `no_match` siblings
/// carried the reason "TEST" and overwrote a real result.
fn host_is_reserved_for_documentation(host: &str) -> bool {
    // `.test` stays allowed on purpose: the fixtures in this file use it as
    // their stand-in domain, and a rule that rejects it would only be checking
    // its own test data.
    matches!(
        host,
        "example.com"
            | "example.org"
            | "example.net"
            | "example.edu"
            | "example"
            | "localhost"
            | "invalid"
    ) || host.ends_with(".example")
        || host.ends_with(".invalid")
        || host.ends_with(".localhost")
        || host.ends_with(".example.com")
        || host.ends_with(".example.org")
}

fn validate_no_match(field: &str, status: &FieldStatus) -> anyhow::Result<()> {
    let searches = status
        .attempts
        .iter()
        .filter(|attempt| attempt.kind == "web_search")
        .count();
    let reads = status
        .attempts
        .iter()
        .filter(|attempt| matches!(attempt.kind.as_str(), "web_read" | "browser_capture"))
        .count();
    anyhow::ensure!(
        searches >= 1 && reads >= 2,
        "no_match field `{field}` requires at least 1 web_search and 2 web_read/browser_capture attempts"
    );
    Ok(())
}

fn validate_attempt(field: &str, attempt: &FieldAttempt, workspace: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(
            attempt.kind.as_str(),
            "web_search" | "web_read" | "browser_capture" | "adapter"
        ),
        "field `{field}` has unsupported attempt kind `{}`",
        attempt.kind
    );
    anyhow::ensure!(
        !attempt.query_or_url.trim().is_empty(),
        "field `{field}` attempt query_or_url must not be empty"
    );
    anyhow::ensure!(
        !attempt.at.is_null(),
        "field `{field}` attempt at must not be null"
    );
    validate_artifact_path(field, workspace, &attempt.artifact_path)
}

fn validate_artifact_path(
    field: &str,
    workspace: &Path,
    artifact_path: &str,
) -> anyhow::Result<()> {
    let relative = Path::new(artifact_path.trim());
    let required_prefix = format!("gap_closure/attempts/{field}/");
    anyhow::ensure!(
        !artifact_path.trim().is_empty()
            && !relative.is_absolute()
            && artifact_path.trim().starts_with(&required_prefix),
        "field `{field}` artifact_path must be under `{required_prefix}`"
    );
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .context("attempt artifact_path must end in a UTF-8 file name")?;
    let sequence = file_name
        .strip_suffix(".json")
        .and_then(|value| value.parse::<usize>().ok());
    anyhow::ensure!(
        sequence.is_some_and(|value| value > 0),
        "field `{field}` artifact_path must end in a positive numeric `<n>.json`"
    );
    let workspace = std::fs::canonicalize(workspace)
        .with_context(|| format!("canonicalize gap task workspace {}", workspace.display()))?;
    let field_attempts =
        std::fs::canonicalize(workspace.join("gap_closure").join("attempts").join(field))
            .with_context(|| format!("canonicalize attempt directory for field `{field}`"))?;
    anyhow::ensure!(
        field_attempts.starts_with(&workspace),
        "field `{field}` attempt directory escapes the gap task workspace"
    );
    let artifact = std::fs::canonicalize(workspace.join(relative)).with_context(|| {
        format!("field `{field}` artifact_path `{artifact_path}` does not exist")
    })?;
    anyhow::ensure!(
        artifact.parent() == Some(field_attempts.as_path()),
        "field `{field}` artifact_path must remain directly under `{required_prefix}`"
    );
    let metadata = std::fs::metadata(&artifact)?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() > 0,
        "field `{field}` artifact_path must be an existing non-empty file"
    );
    Ok(())
}

fn action_required_has_auth_reference(status: &FieldStatus, contract: &Value) -> bool {
    if status.sources.iter().any(|source| {
        !source.source_id.trim().is_empty()
            && (source.requires_credential
                || !source.task_id.trim().is_empty()
                || !source.command_id.trim().is_empty())
    }) {
        return true;
    }
    let attempted_sources = contract
        .get("attempted_sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    if status.sources.iter().any(|source| {
        attempted_sources.clone().any(|attempted| {
            attempted.get("source_id").and_then(Value::as_str) == Some(source.source_id.as_str())
                && attempted
                    .get("requires_credential")
                    .and_then(Value::as_bool)
                    == Some(true)
        })
    }) {
        return true;
    }
    status.attempts.iter().any(|attempt| {
        let source_id = attempt
            .result
            .get("source_id")
            .or_else(|| attempt.result.pointer("/auth_assist/source_id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let task_or_command = [
            "/task_id",
            "/command_id",
            "/auth_assist/task_id",
            "/auth_assist/command_id",
        ]
        .into_iter()
        .any(|pointer| {
            attempt
                .result
                .pointer(pointer)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        });
        !source_id.trim().is_empty() && task_or_command
    })
}

fn validate_result_shape(
    result: &ResearchWritebackResult,
    requested_fields: &[String],
    crm: &CrmBaseline,
) -> anyhow::Result<()> {
    let fields = result
        .fields
        .as_object()
        .context("research writeback result.fields must be an object")?;
    let keys = fields.keys().cloned().collect::<Vec<_>>();
    parse_requested_fields(&keys)?;
    let requested = requested_fields.iter().collect::<BTreeSet<_>>();
    anyhow::ensure!(
        keys.iter().all(|key| requested.contains(key)),
        "research writeback result.fields contains a field that was not requested"
    );
    for (field, value) in fields {
        anyhow::ensure!(
            value.is_object(),
            "research writeback result.fields.{field} must be a structured object, not free text"
        );
        if let Some(field_value) = value.get("value") {
            anyhow::ensure!(
                field_value.is_null() || research_value_is_populated(field_value),
                "research writeback result.fields.{field}.value must be a populated scalar or null"
            );
            if field.starts_with("person_") && research_value_is_populated(field_value) {
                anyhow::ensure!(
                    value
                        .get("person_key")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty()),
                    "research writeback result.fields.{field} requires person_key"
                );
            }
        }
    }
    for (index, person) in result.person_records.iter().enumerate() {
        anyhow::ensure!(
            person
                .get("person_key")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty()),
            "research writeback result.person_records[{index}] requires person_key"
        );
    }
    for (index, evidence) in result.evidence.iter().enumerate() {
        let field = evidence
            .get("field_key")
            .or_else(|| evidence.get("field"))
            .and_then(Value::as_str)
            .with_context(|| {
                format!("research writeback result.evidence[{index}] requires field_key")
            })?;
        parse_requested_fields(&[field.to_string()])?;
        anyhow::ensure!(
            requested_fields.iter().any(|requested| requested == field),
            "research writeback result.evidence[{index}] field was not requested"
        );
        anyhow::ensure!(
            evidence
                .get("source_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty()),
            "research writeback result.evidence[{index}] requires source_id"
        );
        let source_url = evidence
            .get("url")
            .or_else(|| evidence.get("source_url"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .with_context(|| format!("research writeback result.evidence[{index}] requires URL"))?;
        let quote = evidence.get("quote").and_then(Value::as_str).unwrap_or("");
        if crm.check(field, source_url, quote) != Some(true) {
            let source_url = url::Url::parse(source_url).with_context(|| {
                format!("research writeback result.evidence[{index}] has invalid URL")
            })?;
            anyhow::ensure!(
                matches!(source_url.scheme(), "http" | "https") && source_url.host_str().is_some(),
                "research writeback result.evidence[{index}] URL must be HTTP(S)"
            );
        }
        anyhow::ensure!(
            evidence
                .get("quote")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty()),
            "research writeback result.evidence[{index}] requires quote"
        );
        if field.starts_with("person_") {
            anyhow::ensure!(
                evidence
                    .get("person_key")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()),
                "research writeback person evidence[{index}] requires person_key"
            );
        }
    }
    Ok(())
}

fn validate_result_field_status_consistency(
    result: &ResearchWritebackResult,
    field_status: &BTreeMap<String, FieldStatus>,
) -> anyhow::Result<()> {
    let fields = result
        .fields
        .as_object()
        .context("research writeback result.fields must be an object")?;
    for (field, status) in field_status {
        let result_entry = fields.get(field);
        let result_value = result_entry.and_then(|entry| entry.get("value"));
        if status.status == "verified" {
            if let Some(result_value) = result_value.filter(|value| !value.is_null()) {
                anyhow::ensure!(
                    result_value == &status.value,
                    "verified result field `{field}` value does not match field_status value"
                );
            }
            if field.starts_with("person_") {
                let person_key = result_entry
                    .and_then(|entry| entry.get("person_key"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .with_context(|| {
                        format!("verified person result field `{field}` requires person_key")
                    })?;
                anyhow::ensure!(
                    status
                        .sources
                        .iter()
                        .all(|source| source.person_key.as_deref().map(str::trim)
                            == Some(person_key)),
                    "verified person field `{field}` evidence person_key must match result.fields"
                );
            }
        } else {
            anyhow::ensure!(
                !research_value_is_populated(&status.value),
                "non-verified field `{field}` must not carry a populated field_status value"
            );
            anyhow::ensure!(
                !result_value.is_some_and(research_value_is_populated),
                "non-verified result field `{field}` must not carry a populated value"
            );
        }
    }
    Ok(())
}

fn add_field_status_evidence(
    result: &mut Value,
    field_status: &BTreeMap<String, FieldStatus>,
) -> anyhow::Result<()> {
    let fields = result
        .get_mut("fields")
        .and_then(Value::as_object_mut)
        .context("projection result fields must be an object")?;
    for (field, status) in field_status {
        if status.status != "verified" {
            continue;
        }
        let entry = fields
            .entry(field.clone())
            .or_insert_with(|| serde_json::json!({}));
        anyhow::ensure!(
            entry.is_object(),
            "verified result field `{field}` must be an object"
        );
        if entry.get("value").is_none_or(Value::is_null) {
            entry["value"] = status.value.clone();
        }
        let candidates = status
            .sources
            .iter()
            .map(|source| {
                serde_json::json!({
                    "value": status.value,
                    "source_id": source.source_id,
                    "source_url": source.url,
                    "quote": source.quote,
                    "person_key": source
                        .person_key
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| Value::String(value.to_string()))
                        .unwrap_or_else(|| entry.get("person_key").cloned().unwrap_or(Value::Null)),
                    "via": "gap_closure",
                })
            })
            .collect::<Vec<_>>();
        entry["candidates"] = Value::Array(candidates);
    }
    Ok(())
}

fn canonical_requested_fields(
    command: &BusinessCommand,
    result: &Value,
) -> anyhow::Result<Vec<String>> {
    let fields = result
        .get("requested_fields")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            command
                .payload
                .get("fields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
    let fields = if fields.is_empty() {
        ctox_web_stack::sources::OUTBOUND_RESEARCH_FIELDS
            .iter()
            .map(|field| field.as_str().to_string())
            .collect()
    } else {
        fields
    };
    parse_requested_fields(&fields)?;
    Ok(fields)
}

fn phase_a_verified_sources(evidence: &[Value], field: &str) -> Vec<Value> {
    evidence
        .iter()
        .filter(|entry| {
            entry
                .get("field_key")
                .or_else(|| entry.get("field"))
                .and_then(Value::as_str)
                == Some(field)
        })
        .filter_map(|entry| {
            let url = entry
                .get("source_url")
                .or_else(|| entry.get("url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let parsed = url::Url::parse(url).ok()?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return None;
            }
            let source_id = entry
                .get("source_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let quote = entry
                .get("quote")
                .or_else(|| entry.get("note"))
                .or_else(|| entry.get("value"))
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| (!value.is_null()).then(|| value.to_string()))
                })
                .filter(|value| !value.trim().is_empty())?;
            Some(serde_json::json!({
                "field_key": field,
                "source_id": source_id,
                "source_url": url,
                "url": url,
                "quote": quote,
                "person_key": entry.get("person_key").cloned().unwrap_or(Value::Null),
            }))
        })
        .collect()
}

fn phase_a_attempted_sources(result: &Value) -> Value {
    let mut attempted = Vec::new();
    for (key, attempt_kind) in [
        ("plan", "adapter"),
        ("search_runs", "web_search"),
        ("read_runs", "web_read"),
        ("scrape_runs", "web_read"),
        ("browser_extract_runs", "browser_capture"),
        ("authenticated_source_capture_runs", "browser_capture"),
    ] {
        for entry in result
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut entry = entry.clone();
            if let Some(object) = entry.as_object_mut() {
                object
                    .entry("attempt_kind".to_string())
                    .or_insert_with(|| Value::String(attempt_kind.to_string()));
            }
            attempted.push(entry);
        }
    }
    Value::Array(attempted)
}

fn phase_a_projection_evidence(record_id: &str, result: &Value) -> Vec<Value> {
    let patch = outbound_lead_generation_research_outcome_patch(
        &serde_json::json!({"id": record_id, "data": {}, "contacts": [], "evidence": []}),
        result,
        0,
    );
    patch
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn phase_a_workspace(root: &Path, command: &BusinessCommand) -> anyhow::Result<PathBuf> {
    let command_id = command
        .id
        .as_deref()
        .context("person-research command id is required for the Phase-A workspace")?;
    Ok(root.join("runtime").join("research").join("person").join(
        super::person_research_command::safe_workspace_segment(command_id),
    ))
}

fn task_workspace(
    root: &Path,
    task: &channels::QueueTaskView,
    contract: &Value,
) -> anyhow::Result<PathBuf> {
    let research_command_id = required_string(contract, "research_command_id")?;
    let expected = root.join("runtime").join("research").join("person").join(
        super::person_research_command::safe_workspace_segment(research_command_id),
    );
    let actual = task
        .workspace_root
        .as_deref()
        .map(PathBuf::from)
        .context("gap task workspace_root is missing")?;
    anyhow::ensure!(
        actual == expected,
        "gap task workspace_root does not match the Phase-A workspace"
    );
    Ok(expected)
}

fn research_value_is_populated(value: &Value) -> bool {
    match value {
        Value::String(value) => !value.trim().is_empty(),
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("gap closure contract {key} is required"))
}

fn field_definition(field: &str) -> &'static str {
    match field {
        "firma_name" => "aktuelle rechtliche Firmierung",
        "firma_fruehere_namen" => "belegte frühere Firmierungen",
        "firma_aktivitaetsstatus" => "aktueller Register-/Geschäftsstatus",
        "firma_anschrift" => "offizielle Hauptanschrift",
        "firma_besucheranschrift" => "öffentlich ausgewiesene Besucheranschrift",
        "firma_postanschrift" => "offizielle Postanschrift",
        "firma_postfach" => "Postfachangabe",
        "firma_plz" => "Postleitzahl der maßgeblichen Firmenanschrift",
        "firma_ort" => "Ort der maßgeblichen Firmenanschrift",
        "firma_land" => "Land der maßgeblichen Firmenanschrift",
        "firma_email" => "veröffentlichte zentrale Firmen-E-Mail-Adresse",
        "firma_domain" => "kanonische Unternehmensdomain",
        "firma_telefon" => "veröffentlichte zentrale Firmentelefonnummer",
        "firma_fax" => "veröffentlichte zentrale Faxnummer",
        "firma_geschaeftstaetigkeit" => "sachliche Beschreibung der Geschäftstätigkeit",
        "firma_homepage_fact_sheet" => "strukturierte Kernaussagen der offiziellen Homepage",
        "firma_geschaeftsfuehrung" => "aktuell belegte Geschäftsführung",
        "firma_prokura" => "aktuell belegte Prokuristinnen und Prokuristen",
        "wz_code" => "belegter Wirtschaftszweig-/WZ-Code",
        "umsatz" => "aktuellster belegbarer Umsatz mit Zeitraum und Einheit",
        "mitarbeiter" => "aktuellste belegbare Mitarbeiterzahl mit Zeitraum",
        "crm_record_number" => "Sellify-/CRM-Datensatznummer",
        "person_geschlecht" => "belegbare Anrede-/Geschlechtsangabe der Person",
        "person_titel" => "belegter akademischer oder beruflicher Titel",
        "person_vorname" => "belegter Vorname der priorisierten Person",
        "person_nachname" => "belegter Nachname der priorisierten Person",
        "person_funktion" => "belegte organisatorische Funktion der Person",
        "person_position" => "belegte konkrete Stellen-/Positionsbezeichnung",
        "person_email" => "belegte geschäftliche E-Mail-Adresse der Person",
        "person_email_validation" => "belegter Validierungsstatus der Personen-E-Mail",
        "person_telefon" => "belegte geschäftliche Telefonnummer der Person",
        "person_linkedin" => "kanonische LinkedIn-Profil-URL der Person",
        "person_xing" => "kanonische XING-Profil-URL der Person",
        _ => "Wert gemäß dem kanonischen CTOX-Personenrecherche-Feldvokabular",
    }
}

/// Test-only: materialize an RxDB collection table in the tenant RxDB store so
/// `store::upsert_rxdb_collection_record` / `load_rxdb_collection_record` see
/// a real table. In production the browser peer creates these tables during
/// replication; without one, the writer silently skips the upsert.
#[cfg(test)]
pub(super) fn seed_rxdb_collection_table_for_tests(
    root: &Path,
    collection: &str,
) -> anyhow::Result<()> {
    let path = store::rxdb_store_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = rusqlite::Connection::open(&path)?;
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v0 (
            id TEXT PRIMARY KEY NOT NULL,
            revision TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL,
            data TEXT NOT NULL
        );"
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_gap_fixture(
        root: &Path,
        research_command_id: &str,
        record_id: &str,
        field: &str,
    ) -> anyhow::Result<(BusinessCommand, channels::QueueTaskView)> {
        let research_command = BusinessCommand {
            id: Some(research_command_id.to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "web_stack.person_research".to_string(),
            record_id: Some(record_id.to_string()),
            payload: serde_json::json!({
                "company": "Example AG",
                "country": "DE",
                "mode": "new_record",
                "fields": [field],
                "research_instructions": "Nur aktuelle Quellen.",
                "known_person_records": [],
                "person_priorities": [],
                "writeback_contract": {
                    "collection": "outbound_lead_generation_leads",
                    "allowed_collections": ["outbound_lead_generation_leads"],
                    "record_ids": [record_id]
                }
            }),
            client_context: Value::Null,
            origin: store::CommandOrigin::TrustedLocal,
        };
        let mut fields = Map::new();
        fields.insert(
            field.to_string(),
            serde_json::json!({"value": null, "candidates": []}),
        );
        let mut phase_a_result = serde_json::json!({
            "requested_fields": [field],
            "fields": fields,
            "plan": []
        });
        // Production always has the Business OS store (with its RxDB collection
        // tables) before a research command runs; open it first so the lead
        // upsert below lands in a real collection table instead of a no-op.
        drop(store::open_store(root)?);
        seed_rxdb_collection_table_for_tests(root, LEAD_COLLECTION)?;
        let task = enqueue_gap_closure_if_needed(root, &research_command, &mut phase_a_result)?
            .context("expected gap task")?;
        let conn = store::open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, ?2, ?3, ?4, 'completed', ?5, '{}', 1)",
            rusqlite::params![
                research_command_id,
                &research_command.module,
                &research_command.command_type,
                record_id,
                serde_json::to_string(&research_command.payload)?,
            ],
        )?;
        drop(conn);
        store::upsert_rxdb_collection_record(
            root,
            LEAD_COLLECTION,
            record_id,
            1,
            serde_json::json!({
                "id": record_id,
                "data": {},
                "contacts": [],
                "evidence": [],
                "research_status": "running",
                "research_phase": "gap_closure",
                "gap_task_id": task.message_key,
                "payload": {"last_research_command_id": research_command_id}
            }),
        )?;
        Ok((research_command, task))
    }

    fn writeback_command(record_id: &str, payload: Value) -> BusinessCommand {
        BusinessCommand {
            id: Some(format!("writeback-{record_id}")),
            module: "outbound-lead-generation".to_string(),
            command_type: "outbound.lead.research_writeback".to_string(),
            record_id: Some(record_id.to_string()),
            payload,
            client_context: Value::Null,
            origin: store::CommandOrigin::TrustedLocal,
        }
    }

    #[test]
    fn phase_a_with_eight_of_thirty_two_fields_queues_exactly_one_complete_gap_task(
    ) -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let requested = ctox_web_stack::sources::OUTBOUND_RESEARCH_FIELDS
            .iter()
            .map(|field| field.as_str().to_string())
            .collect::<Vec<_>>();
        let mut fields = Map::new();
        for field in requested.iter().take(8) {
            fields.insert(
                field.clone(),
                serde_json::json!({
                    "value": format!("value-{field}"),
                    "candidates": [
                        {"value": format!("value-{field}"), "source_id": "source-a", "source_url": format!("https://a.test/{field}"), "quote": "Beleg A"},
                        {"value": format!("value-{field}"), "source_id": "source-b", "source_url": format!("https://b.test/{field}"), "quote": "Beleg B"}
                    ]
                }),
            );
        }
        let command = BusinessCommand {
            id: Some("research-8-of-32".to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "web_stack.person_research".to_string(),
            record_id: Some("lead-8-of-32".to_string()),
            payload: serde_json::json!({
                "company": "Example AG",
                "country": "DE",
                "mode": "new_record",
                "fields": requested,
                "research_instructions": "Aktuelle Quellen bevorzugen.",
                "known_person_records": [{"person_key": "sellify-1"}],
                "person_priorities": ["Geschäftsführung"],
                "writeback_contract": {
                    "collection": "outbound_lead_generation_leads",
                    "allowed_collections": ["outbound_lead_generation_leads"],
                    "record_ids": ["lead-8-of-32"]
                }
            }),
            client_context: Value::Null,
            origin: store::CommandOrigin::TrustedLocal,
        };
        let mut first_result = serde_json::json!({
            "requested_fields": requested,
            "fields": fields,
            "plan": [{"source_id": "phase-a-source"}]
        });
        let first = enqueue_gap_closure_if_needed(temp.path(), &command, &mut first_result)?
            .context("expected gap task")?;
        let mut replay_result = first_result.clone();
        replay_result["search_runs"] = serde_json::json!([{
            "query": "a changed retry result that would otherwise change the task digest"
        }]);
        let replay = enqueue_gap_closure_if_needed(temp.path(), &command, &mut replay_result)?
            .context("expected replayed gap task")?;

        assert_eq!(first.message_key, replay.message_key);
        assert_eq!(channels::list_queue_tasks(temp.path(), &[], 10)?.len(), 1);
        assert_eq!(first.title, "Lückenschluss: Example AG");
        assert_eq!(first.thread_key, "person-research-gap/lead-8-of-32");
        assert_eq!(first.priority, "high");
        let expected_workspace = temp
            .path()
            .join("runtime/research/person/research-8-of-32")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            first.workspace_root.as_deref(),
            Some(expected_workspace.as_str())
        );
        assert_eq!(first.metadata["idempotency_key"], "gap:research-8-of-32");
        let metadata = &first.metadata[GAP_METADATA_KEY];
        assert_eq!(metadata["gap_task_id"], first.message_key);
        assert_eq!(
            metadata["writeback_contract"]["gap_task_id"],
            first.message_key
        );
        for key in [
            "lead_id",
            "record_id",
            "module",
            "research_command_id",
            "requested_fields",
            "open_fields",
            "terminal_fields",
            "attempted_sources",
            "research_instructions",
            "known_person_records",
            "person_priorities",
            "writeback_contract",
        ] {
            assert!(metadata.get(key).is_some(), "missing metadata key {key}");
        }
        assert_eq!(
            metadata["requested_fields"].as_array().map(Vec::len),
            Some(32)
        );
        assert_eq!(metadata["open_fields"].as_array().map(Vec::len), Some(24));
        assert_eq!(
            metadata["terminal_fields"].as_object().map(Map::len),
            Some(8)
        );
        assert_eq!(first_result["gap_closure"]["required"], true);
        Ok(())
    }

    #[test]
    fn gap_closure_prompt_contains_owner_fields_and_contract_sentences() {
        let prompt = build_gap_closure_prompt(&serde_json::json!({
            "company": "Example AG",
            "record_id": "lead-1",
            "module": "outbound-lead-generation",
            "research_command_id": "research-1",
            "open_fields": ["firma_domain", "person_email"],
            "attempted_sources": [{"source_id": "handelsregister"}],
            "research_instructions": "Nur aktuell bestellte Personen recherchieren.",
            "person_priorities": ["Geschäftsführung"],
            "known_person_records": [{"person_key": "sellify-1", "nachname": "Muster"}],
            "writeback_contract": {"command_type": "outbound.lead.research_writeback"}
        }))
        .unwrap();
        assert!(prompt.contains("Nur aktuell bestellte Personen recherchieren."));
        assert!(prompt.contains("`firma_domain`"));
        assert!(prompt.contains("`person_email`"));
        assert!(prompt.contains("`ctox web search`"));
        assert!(prompt.contains("`ctox web read`"));
        assert!(prompt.contains("`ctox web browser-capture`"));
        assert!(prompt.contains("mindestens 1 dokumentierte Websuche"));
        assert!(prompt.contains("mindestens 2 dokumentierte Seitenlektüren"));
        assert!(prompt.contains("gap_closure/field_status.json"));
        assert!(prompt.contains("outbound.lead.research_writeback"));
        assert!(prompt.contains("bevor dieser Dispatch vom Daemon angenommen wurde"));
    }

    #[test]
    fn no_match_requires_search_two_reads_and_nonempty_workspace_artifacts() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::create_dir_all(temp.path().join("gap_closure/attempts/firma_domain"))?;
        for number in 1..=3 {
            std::fs::write(
                temp.path()
                    .join(format!("gap_closure/attempts/firma_domain/{number}.json")),
                b"{}",
            )?;
        }
        let attempt = |kind: &str, number: usize| FieldAttempt {
            kind: kind.to_string(),
            query_or_url: "query-or-url".to_string(),
            result: Value::Null,
            artifact_path: format!("gap_closure/attempts/firma_domain/{number}.json"),
            at: serde_json::json!(1),
        };
        let mut status = FieldStatus {
            status: "no_match".to_string(),
            value: Value::Null,
            sources: Vec::new(),
            attempts: vec![attempt("web_search", 1), attempt("web_read", 2)],
            reason: "not found".to_string(),
            extra: BTreeMap::new(),
        };
        assert!(validate_terminal_field(
            "firma_domain",
            &status,
            temp.path(),
            &serde_json::json!({})
        )
        .is_err());
        status.attempts.push(attempt("browser_capture", 3));
        validate_terminal_field("firma_domain", &status, temp.path(), &serde_json::json!({}))?;
        std::fs::write(
            temp.path().join("gap_closure/attempts/firma_domain/3.json"),
            b"",
        )?;
        assert!(validate_terminal_field(
            "firma_domain",
            &status,
            temp.path(),
            &serde_json::json!({})
        )
        .unwrap_err()
        .to_string()
        .contains("non-empty"));
        Ok(())
    }

    #[test]
    fn writeback_with_thirty_one_of_thirty_two_terminal_fields_is_rejected() {
        let requested = ctox_web_stack::sources::OUTBOUND_RESEARCH_FIELDS
            .iter()
            .map(|field| field.as_str().to_string())
            .collect::<Vec<_>>();
        let status = FieldStatus {
            status: "unsupported".to_string(),
            value: Value::Null,
            sources: Vec::new(),
            attempts: Vec::new(),
            reason: "unsupported".to_string(),
            extra: BTreeMap::new(),
        };
        let submitted = requested
            .iter()
            .take(31)
            .map(|field| (field.clone(), status.clone()))
            .collect::<BTreeMap<_, _>>();

        let error = validate_field_status_keys(&requested, &submitted).unwrap_err();
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn field_status_rejects_missing_and_unknown_vocabulary_fields() {
        let mut submitted = BTreeMap::new();
        submitted.insert(
            "firma_domain".to_string(),
            FieldStatus {
                status: "unsupported".to_string(),
                value: Value::Null,
                sources: Vec::new(),
                attempts: Vec::new(),
                reason: "unsupported".to_string(),
                extra: BTreeMap::new(),
            },
        );
        assert!(validate_field_status_keys(
            &["firma_domain".to_string(), "firma_email".to_string()],
            &submitted
        )
        .unwrap_err()
        .to_string()
        .contains("missing"));
        let unknown = submitted["firma_domain"].clone();
        submitted.insert("firma_unbekannt".to_string(), unknown);
        assert!(
            validate_field_status_keys(&["firma_domain".to_string()], &submitted)
                .unwrap_err()
                .to_string()
                .contains("unsupported person-research field")
        );
    }

    #[test]
    fn writeback_rejects_wrong_record_and_research_command_correlation() {
        let request: ResearchWritebackRequest = serde_json::from_value(serde_json::json!({
            "record_id": "lead-wrong",
            "module": "outbound-lead-generation",
            "research_command_id": "research-wrong",
            "gap_task_id": "gap-wrong",
            "field_status": {},
            "result": {"fields": {}, "person_records": [], "evidence": []}
        }))
        .unwrap();
        let error = validate_task_correlation(
            &serde_json::json!({
                "record_id": "lead-right",
                "module": "outbound-lead-generation",
                "research_command_id": "research-right",
                "gap_task_id": "gap-right"
            }),
            &request,
        )
        .unwrap_err();
        assert!(error.to_string().contains("record_id"));
        let error = validate_task_correlation(
            &serde_json::json!({
                "record_id": "lead-wrong",
                "module": "outbound-lead-generation",
                "research_command_id": "research-right",
                "gap_task_id": "gap-wrong"
            }),
            &request,
        )
        .unwrap_err();
        assert!(error.to_string().contains("research_command_id"));
        let error = validate_task_correlation(
            &serde_json::json!({
                "record_id": "lead-wrong",
                "module": "outbound-lead-generation",
                "research_command_id": "research-wrong",
                "gap_task_id": "gap-right"
            }),
            &request,
        )
        .unwrap_err();
        assert!(error.to_string().contains("task id"));
    }

    #[test]
    fn verified_writeback_completes_lead_and_retains_gap_audit_id() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-verified";
        let research_command_id = "research-verified";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_domain")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_domain": {
                        "status": "verified",
                        "value": "example.test",
                        "sources": [
                            {"source_id": "official", "url": "https://example.test/imprint", "quote": "Example AG"},
                            {"source_id": "register", "url": "https://register.test/example", "quote": "example.test"}
                        ],
                        "attempts": []
                    }
                },
                "result": {
                    "fields": {"firma_domain": {"value": "example.test"}},
                    "person_records": [],
                    "evidence": []
                }
            }),
        );

        let result = handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        assert_eq!(result["research_status"], "completed");
        assert_eq!(lead["research_status"], "completed");
        assert!(lead["research_phase"].is_null());
        assert_eq!(lead["gap_task_id"], task.message_key);
        assert_eq!(lead["data"]["firma_domain"], "example.test");
        assert_eq!(lead["field_status"]["firma_domain"]["status"], "verified");
        Ok(())
    }

    /// Der Befund vom 03.09.2026: EIN fehlerhafter Beleg hat die Recherche einer
    /// ganzen Firma vernichtet. Jetzt bleibt das gute Feld erhalten, das
    /// schlechte faellt begruendet heraus, und der Lead geht in die Pruefung.
    fn evidence_free_request(sources: usize, verified: bool) -> ResearchWritebackRequest {
        let mut field_status = BTreeMap::new();
        field_status.insert(
            "firma_name".to_string(),
            FieldStatus {
                status: if verified { "verified" } else { "no_match" }.to_string(),
                value: if verified {
                    Value::String("Beispiel GmbH".to_string())
                } else {
                    Value::Null
                },
                sources: (0..sources)
                    .map(|index| FieldSource {
                        source_id: format!("quelle-{index}"),
                        url: format!("https://quelle-{index}.example/impressum"),
                        quote: "Beispiel GmbH".to_string(),
                        person_key: None,
                        requires_credential: false,
                        task_id: String::new(),
                        command_id: String::new(),
                    })
                    .collect(),
                attempts: Vec::new(),
                reason: "Keine unabhängige Quelle gefunden.".to_string(),
                extra: BTreeMap::new(),
            },
        );
        field_status.insert(
            "firma_domain".to_string(),
            FieldStatus {
                status: "no_match".to_string(),
                value: Value::Null,
                sources: Vec::new(),
                attempts: Vec::new(),
                reason: "Website nicht lesbar.".to_string(),
                extra: BTreeMap::new(),
            },
        );
        ResearchWritebackRequest {
            record_id: "lead_test".to_string(),
            module: "outbound-lead-generation".to_string(),
            research_command_id: "leadgen-lead-research-test".to_string(),
            gap_task_id: String::new(),
            field_status,
            result: ResearchWritebackResult {
                fields: serde_json::json!({}),
                person_records: Vec::new(),
                evidence: Vec::new(),
            },
        }
    }

    #[test]
    fn field_status_sources_accept_list_object_and_json_string() {
        let list: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified", "value": "x",
            "sources": [{"source_id": "a.de", "url": "https://a.de/", "quote": "x"}]
        }))
        .unwrap();
        assert_eq!(list.sources.len(), 1);
        let object: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified", "value": "x",
            "sources": {"source_id": "a.de", "url": "https://a.de/", "quote": "x"}
        }))
        .unwrap();
        assert_eq!(object.sources.len(), 1);
        let text: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified", "value": "x",
            "sources": "[{\"source_id\":\"a.de\",\"url\":\"https://a.de/\",\"quote\":\"x\"},{\"source_id\":\"b.de\",\"url\":\"https://b.de/\",\"quote\":\"y\"}]"
        }))
        .unwrap();
        assert_eq!(text.sources.len(), 2);
        assert_eq!(text.sources[1].source_id, "b.de");
        let bad = serde_json::from_value::<FieldStatus>(serde_json::json!({
            "status": "verified", "value": "x", "sources": "northdata"
        }));
        assert!(bad.is_err());
    }

    #[test]
    fn writeback_without_result_derives_result_fields_from_verified_status() {
        let mut request: ResearchWritebackRequest = serde_json::from_value(serde_json::json!({
            "record_id": "lead_x", "module": "outbound-lead-generation",
            "research_command_id": "leadgen-lead-research-x",
            "field_status": {
                "firma_name": {"status": "verified", "value": "Beispiel GmbH",
                    "sources": [{"source_id": "a.de", "url": "https://a.de/", "quote": "Beispiel GmbH"},
                                {"source_id": "b.de", "url": "https://b.de/", "quote": "Beispiel GmbH"}]},
                "firma_domain": {"status": "no_match", "reason": "nicht gefunden", "sources": "", "attempts": ""}
            }
        }))
        .expect("missing result and empty-string lists are tolerated");
        derive_result_fields_from_field_status(&mut request);
        let fields = request.result.fields.as_object().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields["firma_name"]["value"], "Beispiel GmbH");
        assert_eq!(fields["firma_name"]["sources"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn result_lists_accept_empty_string_and_single_object() {
        let result: ResearchWritebackResult = serde_json::from_value(serde_json::json!({
            "fields": "", "person_records": {"person_key": "p1", "person_nachname": "Muster"}, "evidence": ""
        }))
        .unwrap();
        assert!(result.fields.as_object().unwrap().is_empty());
        assert_eq!(result.person_records.len(), 1);
        assert!(result.evidence.is_empty());
        assert!(serde_json::from_value::<ResearchWritebackResult>(
            serde_json::json!({"fields": {}, "evidence": "northdata"})
        )
        .is_err());
    }

    #[test]
    fn field_sources_derive_source_id_and_render_numbers() {
        let status: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified", "value": "x",
            "sources": [
                {"url": "https://www.northdata.de/Firma", "quote": 2024},
                {"host": "impressum.example", "url": "https://impressum.example/i", "quote": "x"},
                {"source_id": 42, "url": "https://a.de/", "quote": "x"}
            ],
            "reason": 7
        }))
        .unwrap();
        assert_eq!(status.sources[0].source_id, "northdata.de");
        assert_eq!(status.sources[0].quote, "2024");
        assert_eq!(status.sources[1].source_id, "impressum.example");
        assert_eq!(status.sources[2].source_id, "42");
        assert_eq!(status.reason, "7");
    }

    #[test]
    fn follow_up_writeback_keeps_previous_field_status_and_unions_keys() {
        let mut lead = serde_json::json!({
            "field_status": {
                "firma_name": {"status": "verified", "value": "Beispiel GmbH"},
                "firma_domain": {"status": "verified", "value": "beispiel.de"},
                "firma_email": {"status": "no_match", "reason": "keine Quelle"}
            },
            "payload": {
                "researched_field_keys": ["firma_name", "firma_domain"],
                "verified_field_keys": ["firma_name", "firma_domain"],
                "unverified_field_keys": []
            }
        });
        let previous = previous_research_keys(&lead);
        // The patch of a one-field follow-up writeback names only that field.
        lead["payload"]["researched_field_keys"] = serde_json::json!(["firma_email"]);
        lead["payload"]["verified_field_keys"] = serde_json::json!([]);
        lead["payload"]["unverified_field_keys"] = serde_json::json!(["firma_email"]);
        union_research_keys(&mut lead, &previous);
        assert_eq!(
            lead["payload"]["researched_field_keys"],
            serde_json::json!(["firma_name", "firma_domain", "firma_email"])
        );
        assert_eq!(
            lead["payload"]["verified_field_keys"],
            serde_json::json!(["firma_name", "firma_domain"])
        );
        assert_eq!(
            lead["payload"]["unverified_field_keys"],
            serde_json::json!(["firma_email"])
        );

        let merged = merge_field_status(
            lead.get("field_status"),
            serde_json::json!({"firma_email": {"status": "verified", "value": "info@beispiel.de"}}),
        );
        assert_eq!(merged["firma_name"]["status"], "verified");
        assert_eq!(merged["firma_domain"]["value"], "beispiel.de");
        assert_eq!(merged["firma_email"]["status"], "verified");
        assert_eq!(merged.as_object().unwrap().len(), 3);
    }

    #[test]
    fn filler_statuses_of_a_follow_up_do_not_downgrade_earlier_verified_fields() {
        let previous = serde_json::json!({
            "firma_name": {"status": "verified", "value": "Beispiel GmbH", "sources": [{"source_id": "official"}]},
            "firma_domain": {"status": "no_match", "reason": "keine Quelle", "attempts": [{"source_id": "register"}]},
            "firma_fax": {"status": "unsupported", "reason": "Vom Rueckschreiben nicht geliefert"}
        });
        // The sanitizer fills every requested-but-undelivered field with an
        // evidence-free `unsupported` entry; only firma_email was delivered.
        let incoming = serde_json::json!({
            "firma_name": {"status": "unsupported", "value": null, "sources": [], "attempts": [], "reason": "Vom Rueckschreiben nicht geliefert"},
            "firma_domain": {"status": "unsupported", "value": null, "sources": [], "attempts": [], "reason": "Vom Rueckschreiben nicht geliefert"},
            "firma_fax": {"status": "unsupported", "value": null, "sources": [], "attempts": [], "reason": "Vom Rueckschreiben nicht geliefert"},
            "firma_email": {"status": "verified", "value": "info@beispiel.de", "sources": [{"source_id": "official"}], "attempts": []},
            "firma_telefon": {"status": "unsupported", "value": null, "sources": [], "attempts": [], "reason": "Vom Rueckschreiben nicht geliefert"}
        });
        let merged = merge_field_status(Some(&previous), incoming);
        assert_eq!(merged["firma_name"]["status"], "verified");
        assert_eq!(merged["firma_name"]["value"], "Beispiel GmbH");
        assert_eq!(merged["firma_domain"]["status"], "no_match");
        assert_eq!(merged["firma_email"]["status"], "verified");
        // A field without an informative earlier status takes the filler.
        assert_eq!(
            merged["firma_fax"]["reason"],
            "Vom Rueckschreiben nicht geliefert"
        );
        assert_eq!(merged["firma_telefon"]["status"], "unsupported");
        // A documented `unsupported` (with attempts) is not a filler and wins.
        let documented = serde_json::json!({
            "firma_name": {"status": "unsupported", "attempts": [{"source_id": "official", "note": "Seite offline"}], "sources": []}
        });
        let merged = merge_field_status(Some(&merged), documented);
        assert_eq!(merged["firma_name"]["status"], "unsupported");
    }

    #[test]
    fn a_field_rejected_for_a_single_source_stays_open() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-einzelquelle";
        let research_command_id = "research-einzelquelle";
        let (_, task) = create_gap_fixture(temp.path(), research_command_id, record_id, "umsatz")?;
        // The worker claims `verified` but documents a single source host —
        // exactly what the evidence gate rejects.
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "umsatz": {
                        "status": "verified",
                        "value": "12,5 Mio. EUR",
                        "sources": [
                            {"source_id": "northdata.de", "url": "https://www.northdata.de/a", "quote": "12,5 Mio. EUR"},
                            {"source_id": "northdata.de", "url": "https://www.northdata.de/b", "quote": "12,5 Mio. EUR"}
                        ],
                        "attempts": []
                    }
                },
                "result": {"fields": {"umsatz": {"value": "12,5 Mio. EUR"}}, "person_records": [], "evidence": []}
            }),
        );
        let result = handle_research_writeback(temp.path(), &command)?;
        assert_eq!(result["ok"], true);
        let rejections = result["rejections"]
            .as_array()
            .context("rejections fehlen")?;
        assert!(
            rejections.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("unabhaengige Quell-Hosts"))),
            "the single-host claim must be rejected: {rejections:?}"
        );
        assert_eq!(
            result["accepted_fields"],
            serde_json::json!([]),
            "a rejected field was never stored, so it is not accepted"
        );
        assert_eq!(
            result["open_fields"],
            serde_json::json!(["umsatz"]),
            "a rejected field stays open so the worker fetches a second source"
        );
        assert!(result["summary"]
            .as_str()
            .is_some_and(|text| text.contains("0 Feld(er) gespeichert")
                && text.contains("1 Feld(er) noch offen")));
        assert_eq!(result["research_status"], "needs_review");
        Ok(())
    }

    #[test]
    fn writeback_response_separates_open_fields_from_rejections() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-offene-felder";
        let research_command_id = "research-offene-felder";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_domain")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_domain": {
                        "status": "verified",
                        "value": "example.test",
                        "sources": [
                            {"source_id": "official", "url": "https://example.test/imprint", "quote": "example.test"},
                            {"source_id": "register", "url": "https://register.test/example", "quote": "example.test"}
                        ],
                        "attempts": []
                    }
                },
                "result": {"fields": {"firma_domain": {"value": "example.test"}}, "person_records": [], "evidence": []}
            }),
        );
        let result = handle_research_writeback(temp.path(), &command)?;
        assert_eq!(result["ok"], true);
        assert_eq!(
            result["accepted_fields"],
            serde_json::json!(["firma_domain"])
        );
        assert_eq!(result["open_fields"], serde_json::json!([]));
        let rejections = result["rejections"]
            .as_array()
            .context("rejections fehlen")?;
        assert!(
            !rejections.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("nicht geliefert"))),
            "undelivered fields must not be reported as rejections: {rejections:?}"
        );
        assert!(result["summary"]
            .as_str()
            .is_some_and(|text| text.contains("1 Feld(er) gespeichert")));
        assert_eq!(
            result["field_status"].as_object().map(|map| map.len()),
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn field_status_drops_non_object_entries_and_accepts_attempts_without_kind() {
        let request: ResearchWritebackRequest = serde_json::from_value(serde_json::json!({
            "record_id": "lead-x",
            "module": "outbound-lead-generation",
            "research_command_id": "research-x",
            "field_status": {
                "firma_name": {
                    "status": "verified",
                    "value": "Beispiel GmbH",
                    "sources": {"item": [{"source_id": "official", "url": "https://beispiel.de/impressum"}]},
                    "attempts": {"item": [{"query_or_url": "https://beispiel.de/impressum", "result": "Found"}]}
                },
                "firma_fax": "",
                "firma_email": ["nicht", "objekt"],
                "firma_telefon": null
            }
        }))
        .expect("non-object entries are dropped, attempts without kind accepted");
        assert_eq!(request.field_status.len(), 1);
        assert_eq!(request.field_status["firma_name"].attempts.len(), 1);
        assert_eq!(request.field_status["firma_name"].attempts[0].kind, "");

        let listed: ResearchWritebackRequest = serde_json::from_value(serde_json::json!({
            "record_id": "lead-x",
            "module": "outbound-lead-generation",
            "research_command_id": "research-x",
            "field_status": [
                {"field": "firma_domain", "status": "verified", "value": "beispiel.de", "sources": []},
                {"status": "verified", "value": "ohne Schluessel"}
            ]
        }))
        .expect("a list of entries keyed by `field` is accepted");
        assert_eq!(listed.field_status.len(), 1);
        assert_eq!(listed.field_status["firma_domain"].status, "verified");
    }

    #[test]
    fn top_level_field_entries_are_hoisted_into_field_status() {
        let payload = serde_json::json!({
            "record_id": "lead-x",
            "module": "outbound-lead-generation",
            "research_command_id": "research-x",
            "field_status": {"firma_name": {"status": "verified", "value": "Beispiel GmbH"}},
            "firma_fruehere_namen": {"status": "no_match", "reason": "keine", "attempts": []},
            "person_records": [{"person_key": "p1", "name": "Erika Muster"}]
        });
        let request: ResearchWritebackRequest =
            serde_json::from_value(hoist_top_level_field_entries(payload.clone()))
                .expect("stray field entry is hoisted, not rejected");
        assert_eq!(request.field_status.len(), 2);
        assert_eq!(
            request.field_status["firma_fruehere_namen"].status,
            "no_match"
        );
        assert_eq!(request.result.person_records.len(), 1);

        let stray_in_result: ResearchWritebackRequest = serde_json::from_value(
            hoist_top_level_field_entries(serde_json::json!({
                "record_id": "lead-x",
                "module": "outbound-lead-generation",
                "research_command_id": "research-x",
                "field_status": {"person_vorname": {"status": "verified", "value": "Erika"}},
                "result": {"person_vorname": "Erika", "fields": {"firma_name": {"value": "Beispiel GmbH"}}}
            })),
        )
        .expect("a field value placed directly in result moves into result.fields");
        assert_eq!(
            stray_in_result.result.fields["person_vorname"]["value"],
            "Erika"
        );
        assert_eq!(
            stray_in_result.result.fields["firma_name"]["value"],
            "Beispiel GmbH"
        );
        // A stray non-field key still fails the envelope check (deny_unknown_fields).
        let mut with_note = payload;
        with_note["note"] = serde_json::json!("kein Feld");
        assert!(
            serde_json::from_value::<ResearchWritebackRequest>(hoist_top_level_field_entries(
                with_note
            ))
            .is_err()
        );
    }

    #[test]
    fn a_writeback_without_any_evidence_is_rejected() {
        let error = reject_evidence_free_writeback(&evidence_free_request(0, false))
            .expect_err("32x no_match without a single source must not close a lead");
        assert!(error
            .to_string()
            .contains("evidence-free research writeback rejected"));
    }

    #[test]
    fn a_documented_no_match_or_a_verified_field_passes_the_evidence_gate() {
        reject_evidence_free_writeback(&evidence_free_request(1, false))
            .expect("one documented source keeps the writeback");
        reject_evidence_free_writeback(&evidence_free_request(2, true))
            .expect("a verified field keeps the writeback");
    }

    #[test]
    fn writeback_rettet_gute_felder_und_verwirft_nur_den_fehlerhaften_beleg() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-teilrettung";
        let research_command_id = "research-teilrettung";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_domain")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_domain": {
                        "status": "verified",
                        "value": "example.test",
                        "sources": [
                            {"source_id": "official", "url": "https://example.test/imprint", "quote": "Example AG"},
                            {"source_id": "register", "url": "https://register.test/example", "quote": "example.test"}
                        ],
                        "attempts": []
                    }
                },
                "result": {
                    "fields": {"firma_domain": {"value": "example.test"}},
                    "person_records": [{"name": "Ohne Schluessel"}],
                    "evidence": [
                        {"field_key": "firma_domain", "source_id": "official", "url": "ftp://example.test/datei", "quote": "Example AG"}
                    ]
                }
            }),
        );

        let result = handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        // Das belegte Feld ueberlebt.
        assert_eq!(lead["data"]["firma_domain"], "example.test");
        assert_eq!(lead["field_status"]["firma_domain"]["status"], "verified");
        // Der fehlerhafte Beleg und der Personendatensatz ohne Schluessel sind weg.
        let ablehnungen = result["rejections"]
            .as_array()
            .context("rejections fehlen in der Antwort")?;
        assert!(
            ablehnungen
                .iter()
                .any(|entry| entry.as_str().is_some_and(|text| text.contains("evidence"))),
            "der nicht-http-Beleg muss als Ablehnung erscheinen: {ablehnungen:?}"
        );
        assert!(
            ablehnungen.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("person_records"))),
            "der Personendatensatz ohne person_key muss als Ablehnung erscheinen: {ablehnungen:?}"
        );
        // Verworfenes heisst: ein Mensch schaut drauf.
        assert_eq!(result["research_status"], "needs_review");
        assert_eq!(lead["research_status"], "needs_review");
        Ok(())
    }

    /// Gemessen am 03.09.2026: 100 von 265 "verified" Feldern hatten weniger
    /// als zwei verschiedene Quell-Hosts. Auf dem Chatweg pruefte das niemand.
    #[test]
    fn verified_mit_nur_einem_quell_host_wird_nicht_als_belegt_uebernommen() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-eine-quelle";
        let research_command_id = "research-eine-quelle";
        let (_, task) = create_gap_fixture(temp.path(), research_command_id, record_id, "umsatz")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "umsatz": {
                        "status": "verified",
                        "value": "12,5 Mio. EUR",
                        // Zwei Belege, aber derselbe Host - das ist EINE Quelle.
                        "sources": [
                            {"source_id": "seite-1", "url": "https://example.test/bilanz", "quote": "12,5 Mio. EUR"},
                            {"source_id": "seite-2", "url": "https://example.test/kennzahlen", "quote": "12,5 Mio. EUR"}
                        ],
                        "attempts": []
                    }
                },
                "result": {
                    "fields": {"umsatz": {"value": "12,5 Mio. EUR"}},
                    "person_records": [],
                    "evidence": []
                }
            }),
        );

        let result = handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        assert_eq!(
            lead["field_status"]["umsatz"]["status"], "unsupported",
            "ein einziger Quell-Host darf nicht als belegt durchgehen"
        );
        assert_eq!(result["research_status"], "needs_review");
        let ablehnungen = result["rejections"]
            .as_array()
            .context("rejections fehlen")?;
        assert!(
            ablehnungen.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("unabhaengige Quell-Hosts"))),
            "der Grund muss benannt sein: {ablehnungen:?}"
        );
        Ok(())
    }

    fn create_chat_fixture_with_sellify(
        root: &Path,
        research_command_id: &str,
        record_id: &str,
        fields: &[&str],
    ) -> anyhow::Result<()> {
        drop(store::open_store(root)?);
        seed_rxdb_collection_table_for_tests(root, LEAD_COLLECTION)?;
        let payload = serde_json::json!({
            "company": "Sasol Germany GmbH",
            "lead_id": record_id,
            "fields": fields,
            "sellify_company": {
                "contact_id": "2559",
                "name": "Sasol Germany GmbH",
                "anschrift": "Anckelmannsplatz 1",
                "plz": "20537",
                "telefon": "+4940636841000",
                "wz_code": "20590",
                "umsatz": "2100 Mio. €"
            },
            "known_person_records": [{
                "sellify_person_id": "sellify-person-8096",
                "person_vorname": "Holger",
                "person_email": "holger.hess@de.sasol.com"
            }]
        });
        let conn = store::open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'outbound-lead-generation', 'business_os.chat.task', ?2, 'running', ?3, '{}', 1)",
            rusqlite::params![research_command_id, record_id, serde_json::to_string(&payload)?],
        )?;
        drop(conn);
        store::upsert_rxdb_collection_record(
            root,
            LEAD_COLLECTION,
            record_id,
            1,
            serde_json::json!({
                "id": record_id,
                "data": {},
                "contacts": [],
                "evidence": [],
                "research_status": "running",
                "payload": {"last_research_command_id": research_command_id}
            }),
        )?;
        Ok(())
    }

    #[test]
    fn a_checked_sellify_value_counts_as_one_source_and_a_forged_one_as_none() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-sellify-quelle";
        let research_command_id = "research-sellify-quelle";
        create_chat_fixture_with_sellify(
            temp.path(),
            research_command_id,
            record_id,
            &[
                "firma_plz",
                "umsatz",
                "wz_code",
                "firma_telefon",
                "person_vorname",
            ],
        )?;
        let northdata = "https://www.northdata.de/Sasol+Germany+GmbH,+Hamburg/HRB+78475";
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": "",
                "field_status": {
                    // Sellify plus Northdata: two independent sources.
                    "firma_plz": {"status": "verified", "value": "20537", "sources": [
                        {"source_id": "sellify", "url": "sellify://company/2559", "quote": "20537"},
                        {"source_id": "northdata.de", "url": northdata, "quote": "Anckelmannsplatz 1, 20537 Hamburg"}
                    ]},
                    // Sellify alone proves nothing.
                    "umsatz": {"status": "verified", "value": "2.100 Mio. €", "sources": [
                        {"source_id": "sellify", "url": "sellify://company/2559", "quote": "2.100 Mio. €"}
                    ]},
                    // An import key is no Sellify record: the citation is dropped,
                    // the field stands on its external source alone.
                    "person_vorname": {"status": "verified", "value": "Holger", "sources": [
                        {"source_id": "sellify", "url": "sellify://person/person_hess_holger", "quote": "Holger", "person_key": "sellify-person-8096"},
                        {"source_id": "sasol.com", "url": "https://www.sasol.com/de/kontakt", "quote": "Holger Heß", "person_key": "sellify-person-8096"}
                    ]},
                    // A self-reported field needs one source, but Sellify is not it.
                    "firma_telefon": {"status": "verified", "value": "+49 40 63684-1000", "sources": [
                        {"source_id": "sellify", "url": "sellify://company/2559", "quote": "+49 40 63684-1000"}
                    ]},
                    // A quote Sellify does not hold is no Sellify source.
                    "wz_code": {"status": "verified", "value": "20599", "sources": [
                        {"source_id": "sellify", "url": "sellify://company/2559", "quote": "20599"},
                        {"source_id": "northdata.de", "url": northdata, "quote": "WZ 20599"}
                    ]}
                },
                "result": {
                    "fields": {
                        "firma_plz": {"value": "20537"},
                        "person_vorname": {"value": "Holger", "person_key": "sellify-person-8096"}
                    },
                    "person_records": [],
                    "evidence": [
                        {"field_key": "firma_plz", "source_id": "sellify", "url": "sellify://company/2559", "quote": "20537"},
                        {"field_key": "firma_plz", "source_id": "sellify", "url": "sellify://company/9999", "quote": "20537"}
                    ]
                }
            }),
        );

        let result = handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        assert_eq!(
            lead["field_status"]["firma_plz"]["status"], "verified",
            "{result}"
        );
        assert_eq!(
            lead["field_status"]["umsatz"]["status"], "unsupported",
            "{result}"
        );
        assert_eq!(
            lead["field_status"]["wz_code"]["status"], "unsupported",
            "{result}"
        );
        assert_eq!(
            lead["field_status"]["firma_telefon"]["status"], "unsupported",
            "{result}"
        );
        assert_eq!(
            lead["field_status"]["person_vorname"]["status"], "verified",
            "{result}"
        );
        assert!(
            !lead
                .to_string()
                .contains("sellify://person/person_hess_holger"),
            "an unchecked Sellify citation must not reach the lead"
        );
        assert!(
            result["rejections"]
                .to_string()
                .contains("sellify://company/2559"),
            "the rejection names the valid citation: {result}"
        );
        assert!(
            result["rejections"]
                .to_string()
                .contains("Sellify allein belegt nichts"),
            "{result}"
        );
        let rejections = result["rejections"].to_string();
        assert!(
            rejections.contains("Sellify-Beleg passt zu keinem Wert"),
            "the record the command did not carry must be named: {rejections}"
        );
        let evidence = lead["evidence"].as_array().context("evidence missing")?;
        assert!(
            evidence.iter().any(|entry| {
                entry["source_url"] == "sellify://company/2559"
                    || entry["url"] == "sellify://company/2559"
            }),
            "the checked Sellify source must reach the lead: {evidence:?}"
        );
        assert!(!evidence
            .iter()
            .any(|entry| entry.to_string().contains("sellify://company/9999")));
        // The Sellify persons of the assignment reach the lead with their id,
        // so the handover updates them instead of creating them again.
        let contacts = lead["contacts"].as_array().context("contacts missing")?;
        assert!(
            contacts.iter().any(
                |contact| contact["sellify_person_id"] == "sellify-person-8096"
                    && contact["crm_known"] == true
            ),
            "{contacts:?}"
        );
        // 21 documented gaps are a reason to look, whatever the last call held.
        assert_eq!(lead["research_status"], "needs_review");
        Ok(())
    }

    #[test]
    fn a_value_sellify_holds_gets_sellify_as_its_second_source_without_a_citation(
    ) -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-sellify-auto";
        let research_command_id = "research-sellify-auto";
        create_chat_fixture_with_sellify(
            temp.path(),
            research_command_id,
            record_id,
            &[
                "firma_plz",
                "umsatz",
                "wz_code",
                "firma_telefon",
                "person_email",
            ],
        )?;
        let northdata = "https://www.northdata.de/Sasol+Germany+GmbH,+Hamburg/HRB+78475";
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": "",
                "field_status": {
                    // One external source, Sellify agrees: verified.
                    "firma_plz": {"status": "verified", "value": "20537", "sources": [
                        {"source_id": "northdata.de", "url": northdata, "quote": "20537 Hamburg"}
                    ]},
                    "umsatz": {"status": "verified", "value": "2.100 Mio. €", "sources": [
                        {"source_id": "bundesanzeiger.de", "url": "https://www.bundesanzeiger.de/x", "quote": "Umsatzerloese 2.100 Mio. EUR"}
                    ]},
                    // One external source, Sellify holds another code: stays open.
                    "wz_code": {"status": "verified", "value": "20599", "sources": [
                        {"source_id": "northdata.de", "url": northdata, "quote": "WZ 20599"}
                    ]},
                    // Sellify agrees, but nothing external: Sellify alone proves nothing.
                    "firma_telefon": {"status": "verified", "value": "+49 40 63684-1000", "sources": []},
                    "person_email": {"status": "verified", "value": "holger.hess@de.sasol.com", "sources": [
                        {"source_id": "sasol.com", "url": "https://www.sasol.com/de/kontakt", "quote": "holger.hess@de.sasol.com", "person_key": "sellify-person-8096"}
                    ]}
                },
                "result": {
                    "fields": {
                        "firma_plz": {"value": "20537"},
                        "umsatz": {"value": "2.100 Mio. €"},
                        "person_email": {"value": "holger.hess@de.sasol.com", "person_key": "sellify-person-8096"}
                    },
                    "person_records": [],
                    "evidence": []
                }
            }),
        );

        let result = handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        let status = |field: &str| lead["field_status"][field]["status"].clone();
        assert_eq!(status("firma_plz"), "verified", "{result}");
        assert_eq!(status("umsatz"), "verified", "{result}");
        assert_eq!(status("wz_code"), "unsupported", "{result}");
        assert_eq!(status("firma_telefon"), "unsupported", "{result}");
        assert_eq!(status("person_email"), "verified", "{result}");
        let sellify_urls = |field: &str| {
            lead["field_status"][field]["sources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|source| source["url"].as_str())
                .filter(|url| url.starts_with("sellify://"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(sellify_urls("firma_plz"), vec!["sellify://company/2559"]);
        assert_eq!(sellify_urls("umsatz"), vec!["sellify://company/2559"]);
        assert_eq!(
            sellify_urls("person_email"),
            vec!["sellify://person/sellify-person-8096"]
        );
        let evidence = lead["evidence"].to_string();
        assert!(evidence.contains("sellify://company/2559"), "{evidence}");
        Ok(())
    }

    #[test]
    fn a_sellify_citation_is_checked_against_the_record_the_command_carried() {
        let crm = CrmBaseline::from_research_payload(&serde_json::json!({
            "sellify_company": {"contact_id": 2559, "telefon": "+4940636841000", "anschrift": "Anckelmannsplatz 1"},
            "known_person_records": [{"sellify_person_id": "sellify-person-8096", "person_email": "holger.hess@de.sasol.com"}]
        }));
        assert_eq!(crm.check("firma_telefon", "https://sasol.com", "x"), None);
        let with_lead = CrmBaseline::from_research_payload(&serde_json::json!({
            "lead_id": "lead_13nyxua",
            "sellify_company": {"contact_id": "2559", "telefon": "+4940636841000"}
        }));
        assert_eq!(
            with_lead.check(
                "firma_telefon",
                "sellify://company/lead_13nyxua",
                "+49 40 63684-1000"
            ),
            Some(true),
            "the lead id names the company the command carried"
        );
        assert!(
            !with_lead.hint().contains("lead_13nyxua"),
            "{}",
            with_lead.hint()
        );
        assert_eq!(
            crm.check(
                "firma_telefon",
                "sellify://company/2559",
                "+49 40 63684-1000"
            ),
            Some(true)
        );
        assert_eq!(
            crm.check(
                "firma_anschrift",
                "sellify://company/2559/",
                "Anckelmannsplatz 1, 20537 Hamburg"
            ),
            Some(true)
        );
        assert_eq!(
            crm.check(
                "person_email",
                "sellify://person/sellify-person-8096",
                "holger.hess@de.sasol.com"
            ),
            Some(true)
        );
        // Wrong field, wrong record, wrong value, uncheckable field.
        assert_eq!(
            crm.check("firma_fax", "sellify://company/2559", "+4940636841000"),
            Some(false)
        );
        assert_eq!(
            crm.check("firma_telefon", "sellify://company/1", "+4940636841000"),
            Some(false)
        );
        assert_eq!(
            crm.check("firma_telefon", "sellify://company/2559", "+49 40 1"),
            Some(false)
        );
        assert_eq!(
            crm.check("firma_telefon", "sellify://company/2559", "+49 40"),
            Some(false)
        );
        assert_eq!(
            crm.check(
                "firma_prokura",
                "sellify://company/2559",
                "Anckelmannsplatz"
            ),
            Some(false)
        );
        assert_eq!(
            super::super::person_research_command::independent_research_evidence_count(
                &[
                    serde_json::json!({"field_key": "firma_ort", "url": "sellify://company/2559"}),
                    serde_json::json!({"field_key": "firma_ort", "url": "sellify://person/sellify-person-8096"}),
                    serde_json::json!({"field_key": "firma_ort", "url": "https://www.northdata.de/x"}),
                ],
                "firma_ort"
            ),
            2,
            "company and person records are one provider: Sellify"
        );
        assert_eq!(
            super::super::person_research_command::independent_research_evidence_count(
                &[
                    serde_json::json!({"field_key": "firma_telefon", "url": "sellify://company/2559"})
                ],
                "firma_telefon"
            ),
            0,
            "Sellify alone proves nothing, even where one source is enough"
        );
    }

    #[test]
    fn no_match_writeback_finishes_lead_as_needs_review() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-no-match";
        let research_command_id = "research-no-match";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_domain")?;
        let attempts_root = temp
            .path()
            .join("runtime/research/person/research-no-match/gap_closure/attempts/firma_domain");
        std::fs::create_dir_all(&attempts_root)?;
        for number in 1..=3 {
            std::fs::write(attempts_root.join(format!("{number}.json")), b"{}")?;
        }
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_domain": {
                        "status": "no_match",
                        "value": null,
                        "reason": "Keine belastbare Domain gefunden.",
                        "sources": [],
                        "attempts": [
                            {"kind": "web_search", "query_or_url": "Example AG", "result": {}, "artifact_path": "gap_closure/attempts/firma_domain/1.json", "at": 1},
                            {"kind": "web_read", "query_or_url": "https://one.test", "result": {}, "artifact_path": "gap_closure/attempts/firma_domain/2.json", "at": 2},
                            {"kind": "browser_capture", "query_or_url": "https://two.test", "result": {}, "artifact_path": "gap_closure/attempts/firma_domain/3.json", "at": 3}
                        ]
                    }
                },
                "result": {
                    "fields": {"firma_domain": {"value": null}},
                    "person_records": [],
                    "evidence": []
                }
            }),
        );

        handle_research_writeback(temp.path(), &command)?;
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        assert_eq!(lead["research_status"], "needs_review");
        assert!(lead["research_phase"].is_null());
        assert_eq!(lead["gap_task_id"], task.message_key);
        assert_eq!(lead["field_status"]["firma_domain"]["status"], "no_match");
        Ok(())
    }

    #[test]
    fn writeback_rejects_wrong_gap_task_id_and_keeps_lead_running() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-wrong-gap";
        let research_command_id = "research-wrong-gap";
        create_gap_fixture(temp.path(), research_command_id, record_id, "firma_domain")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": "queue::wrong",
                "field_status": {"firma_domain": {"status": "unsupported"}},
                "result": {"fields": {"firma_domain": {"value": null}}}
            }),
        );

        let error = handle_research_writeback(temp.path(), &command).unwrap_err();
        assert!(error
            .to_string()
            .contains("gap_task_id does not match lead"));
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after rejected writeback")?;
        assert_eq!(lead["research_status"], "running");
        assert_eq!(lead["research_phase"], "gap_closure");
        Ok(())
    }

    #[test]
    fn manual_rerun_cancels_the_open_gap_task() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-rerun";
        let (_, task) = create_gap_fixture(
            temp.path(),
            "research-before-rerun",
            record_id,
            "firma_domain",
        )?;
        let rerun = BusinessCommand {
            id: Some("research-after-rerun".to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "web_stack.person_research".to_string(),
            record_id: Some(record_id.to_string()),
            payload: serde_json::json!({"company": "Example AG"}),
            client_context: Value::Null,
            origin: store::CommandOrigin::TrustedLocal,
        };

        assert!(cancel_open_gap_task_for_new_research(temp.path(), &rerun)?);
        let cancelled = channels::load_queue_task(temp.path(), &task.message_key)?
            .context("cancelled gap task missing")?;
        assert_eq!(cancelled.route_status, "cancelled");
        assert!(cancelled
            .status_note
            .as_deref()
            .is_some_and(|note| note.contains("research-after-rerun")));
        Ok(())
    }

    #[test]
    fn result_fields_reject_free_text_and_nonverified_values() {
        let free_text: ResearchWritebackResult = serde_json::from_value(serde_json::json!({
            "fields": {"firma_domain": "example.test"}
        }))
        .unwrap();
        assert!(validate_result_shape(
            &free_text,
            &["firma_domain".to_string()],
            &CrmBaseline::default()
        )
        .unwrap_err()
        .to_string()
        .contains("structured object"));

        let result: ResearchWritebackResult = serde_json::from_value(serde_json::json!({
            "fields": {"firma_domain": {"value": "example.test"}}
        }))
        .unwrap();
        let statuses = BTreeMap::from([(
            "firma_domain".to_string(),
            FieldStatus {
                status: "unsupported".to_string(),
                value: Value::Null,
                sources: Vec::new(),
                attempts: Vec::new(),
                reason: "unsupported".to_string(),
                extra: BTreeMap::new(),
            },
        )]);
        assert!(validate_result_field_status_consistency(&result, &statuses)
            .unwrap_err()
            .to_string()
            .contains("non-verified"));
    }

    #[test]
    fn action_required_needs_auth_assist_or_credential_source() {
        let mut status = FieldStatus {
            status: "action_required".to_string(),
            value: Value::Null,
            sources: Vec::new(),
            attempts: Vec::new(),
            reason: "Login erforderlich".to_string(),
            extra: BTreeMap::new(),
        };
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_terminal_field(
            "firma_domain",
            &status,
            temp.path(),
            &serde_json::json!({"attempted_sources": []})
        )
        .is_err());
        status.sources.push(FieldSource {
            source_id: "credential-source".to_string(),
            url: String::new(),
            quote: String::new(),
            person_key: None,
            requires_credential: true,
            task_id: String::new(),
            command_id: String::new(),
        });
        assert!(validate_terminal_field(
            "firma_domain",
            &status,
            temp.path(),
            &serde_json::json!({"attempted_sources": []})
        )
        .is_ok());
        status.sources[0].requires_credential = false;
        status.sources[0].task_id = "auth-assist-task".to_string();
        assert!(validate_terminal_field(
            "firma_domain",
            &status,
            temp.path(),
            &serde_json::json!({"attempted_sources": []})
        )
        .is_ok());
    }

    #[test]
    fn verified_person_evidence_requires_matching_person_key() {
        let mut status = FieldStatus {
            status: "verified".to_string(),
            value: serde_json::json!("Ada"),
            sources: vec![
                FieldSource {
                    source_id: "official".to_string(),
                    url: "https://example.test/ada".to_string(),
                    quote: "Ada Example".to_string(),
                    person_key: None,
                    requires_credential: false,
                    task_id: String::new(),
                    command_id: String::new(),
                },
                FieldSource {
                    source_id: "register".to_string(),
                    url: "https://register.test/ada".to_string(),
                    quote: "Ada".to_string(),
                    person_key: None,
                    requires_credential: false,
                    task_id: String::new(),
                    command_id: String::new(),
                },
            ],
            attempts: Vec::new(),
            reason: String::new(),
            extra: BTreeMap::new(),
        };
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_terminal_field(
            "person_vorname",
            &status,
            temp.path(),
            &serde_json::json!({})
        )
        .unwrap_err()
        .to_string()
        .contains("person_key"));
        for source in &mut status.sources {
            source.person_key = Some("person-ada".to_string());
        }
        assert!(validate_terminal_field(
            "person_vorname",
            &status,
            temp.path(),
            &serde_json::json!({})
        )
        .is_ok());
    }

    #[test]
    fn verified_sources_must_use_different_hosts() {
        let status = |second_url: &str| FieldStatus {
            status: "verified".to_string(),
            value: serde_json::json!("example.test"),
            sources: vec![
                FieldSource {
                    source_id: "a".to_string(),
                    url: "https://www.example.test/a".to_string(),
                    quote: "A".to_string(),
                    person_key: None,
                    requires_credential: false,
                    task_id: String::new(),
                    command_id: String::new(),
                },
                FieldSource {
                    source_id: "b".to_string(),
                    url: second_url.to_string(),
                    quote: "B".to_string(),
                    person_key: None,
                    requires_credential: false,
                    task_id: String::new(),
                    command_id: String::new(),
                },
            ],
            attempts: Vec::new(),
            reason: String::new(),
            extra: BTreeMap::new(),
        };
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_terminal_field(
            "umsatz",
            &status("https://example.test/b"),
            temp.path(),
            &serde_json::json!({})
        )
        .is_err());
        assert!(validate_terminal_field(
            "umsatz",
            &status("https://other.test/b"),
            temp.path(),
            &serde_json::json!({})
        )
        .is_ok());
    }

    #[test]
    fn a_probe_from_example_com_is_not_evidence() -> anyhow::Result<()> {
        // Measured on the Aeroxon lead 09.09.2026: a worker probing the
        // contract claimed firma_name as verified from https://example.com
        // with the quote "test", and its no_match siblings carried the reason
        // "TEST" and overwrote a real result.
        let temp = tempfile::tempdir()?;
        let record_id = "lead-probe";
        let research_command_id = "research-probe";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_name")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_name": {
                        "status": "verified",
                        "value": "Aeroxon Insect Control GmbH",
                        "sources": [
                            {"source_id": "test", "url": "https://example.com", "quote": "test"},
                            {"source_id": "test2", "url": "https://example.org", "quote": "test"}
                        ],
                        "attempts": []
                    }
                },
                "result": {"fields": {}, "person_records": [], "evidence": []}
            }),
        );
        let result = handle_research_writeback(temp.path(), &command)?;
        let rejections = result["rejections"].as_array().context("rejections")?;
        assert!(
            rejections.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("example.com"))),
            "the documentation host must be named: {rejections:?}"
        );
        assert_eq!(result["accepted_fields"], serde_json::json!([]));
        Ok(())
    }

    #[test]
    fn a_no_match_reason_of_test_does_not_overwrite_a_real_result() {
        let earlier = serde_json::json!({
            "firma_domain": {"status": "verified", "value": "aeroxon.de", "sources": [{"source_id": "a"}]}
        });
        let probe = serde_json::json!({
            "firma_domain": {"status": "no_match", "reason": "TEST", "attempts": []}
        });
        let merged = merge_field_status(Some(&earlier), probe);
        assert_eq!(merged["firma_domain"]["status"], "verified");
        assert_eq!(merged["firma_domain"]["value"], "aeroxon.de");
    }

    #[test]
    fn item_carriers_are_unwrapped_across_the_whole_payload() {
        // Measured on the Aeroxon lead 09.09.2026: not only every field's
        // sources, but also result.person_records and result.evidence arrived
        // inside an {"item": [...]} carrier.
        let payload = serde_json::json!({
            "record_id": "lead-a",
            "result": {
                "person_records": {"item": [{"person_key": "p1", "person_nachname": "Updike"}]},
                "evidence": {"item": [{"field_key": "firma_name", "source_id": "aeroxon.de"}]},
                "fields": {"firma_name": {"value": "Aeroxon", "sources": {"item": [{"source_id": "a"}]}}}
            }
        });
        let unwrapped = unwrap_item_carriers(payload);
        assert_eq!(unwrapped["result"]["person_records"][0]["person_key"], "p1");
        assert_eq!(
            unwrapped["result"]["evidence"][0]["field_key"],
            "firma_name"
        );
        assert_eq!(
            unwrapped["result"]["fields"]["firma_name"]["sources"][0]["source_id"],
            "a"
        );
        // A single wrapped object becomes a one-element list, not a lost value.
        let single =
            unwrap_item_carriers(serde_json::json!({"sources": {"item": {"source_id": "a"}}}));
        assert_eq!(single["sources"][0]["source_id"], "a");
        // A field object that merely has one key keeps its shape.
        let field =
            unwrap_item_carriers(serde_json::json!({"fields": {"firma_name": {"value": "x"}}}));
        assert_eq!(field["fields"]["firma_name"]["value"], "x");
    }

    #[test]
    fn sources_wrapped_in_an_item_carrier_are_still_sources() -> anyhow::Result<()> {
        // Measured on the Aeroxon lead 09.09.2026: a writeback delivered 16
        // verified fields whose evidence was wrapped as {"item": [...]}, and
        // every one of them lost its sources and fell back to no_match.
        let single: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified",
            "value": "www.aeroxon.de",
            "sources": {"item": {"source_id": "aeroxon.de", "url": "https://www.aeroxon.de/impressum/", "quote": "Aeroxon Insect Control GmbH"}}
        }))?;
        assert_eq!(single.sources.len(), 1);
        assert_eq!(single.sources[0].source_id, "aeroxon.de");
        let many: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified",
            "value": "Aeroxon Insect Control GmbH",
            "sources": {"item": [
                {"source_id": "aeroxon.de", "url": "https://www.aeroxon.de/impressum/", "quote": "Aeroxon Insect Control GmbH"},
                {"source_id": "northdata.com", "url": "https://www.northdata.com/AEROXON", "quote": "AEROXON INSECT CONTROL GmbH"}
            ]}
        }))?;
        assert_eq!(many.sources.len(), 2);
        // A real single source object is still a single source, not a carrier.
        let plain: FieldStatus = serde_json::from_value(serde_json::json!({
            "status": "verified",
            "value": "x",
            "sources": {"source_id": "a.test", "url": "https://a.test/", "quote": "x"}
        }))?;
        assert_eq!(plain.sources.len(), 1);
        assert_eq!(plain.sources[0].source_id, "a.test");
        Ok(())
    }

    #[test]
    fn a_self_reported_field_is_verified_from_its_own_single_source() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let record_id = "lead-selbstauskunft";
        let research_command_id = "research-selbstauskunft";
        let (_, task) =
            create_gap_fixture(temp.path(), research_command_id, record_id, "firma_telefon")?;
        let command = writeback_command(
            record_id,
            serde_json::json!({
                "record_id": record_id,
                "module": "outbound-lead-generation",
                "research_command_id": research_command_id,
                "gap_task_id": task.message_key,
                "field_status": {
                    "firma_telefon": {
                        "status": "verified",
                        "value": "+49 2621 12-0",
                        // Only the company's own site carries its switchboard number.
                        "sources": [{
                            "source_id": "unternehmensseite",
                            "url": "https://beispiel.test/kontakt",
                            "quote": "Telefon: +49 2621 12-0"
                        }],
                        "attempts": []
                    }
                },
                "result": {
                    "fields": {"firma_telefon": {"value": "+49 2621 12-0"}},
                    "person_records": [],
                    "evidence": []
                }
            }),
        );
        let result = handle_research_writeback(temp.path(), &command)?;
        assert_eq!(result["ok"], true);
        assert_eq!(
            result["rejections"],
            serde_json::json!([]),
            "eine Selbstauskunft mit einem Beleg darf nicht abgelehnt werden"
        );
        let lead = store::load_rxdb_collection_record(temp.path(), LEAD_COLLECTION, record_id)?
            .context("lead missing after writeback")?;
        assert_eq!(lead["field_status"]["firma_telefon"]["status"], "verified");
        assert_eq!(lead["data"]["firma_telefon"], "+49 2621 12-0");
        Ok(())
    }
}
