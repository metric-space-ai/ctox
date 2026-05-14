//! Bridge between source modules and the CTOX scrape registry
//! (`src/capabilities/scrape.rs` + the `universal-scraping` skill).
//!
//! When a [`SourceModule`] sets [`SourceModule::scrape_target_key`] to
//! `Some(<key>)`, this bridge dispatches extraction to the registered
//! script revision under `runtime/scraping/targets/<key>/scripts/current.*`
//! instead of running the module's in-tree Rust `extract_fields`. The
//! shell-out invokes:
//!
//! ```text
//! ctox scrape execute --target-key <key> \
//!     --trigger-kind <person_research|repair|manual> \
//!     --input-json '{"company":"<name>","country":"<DE|AT|CH>"}' \
//!     --allow-heal --runtime-root <root>
//! ```
//!
//! and parses the resulting envelope into typed `(FieldKey, FieldEvidence)`
//! pairs that the [`person_research`](crate::person_research) orchestrator
//! aggregates exactly like Rust-native results.
//!
//! The bridge fails soft: if the target is not registered yet (e.g. early
//! in the migration of a source from Rust to scrape-target), it returns an
//! empty record list with a `target_not_registered` reason so the caller
//! can fall back to the Rust path.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde_json::json;
use serde_json::Value;

use super::Confidence;
use super::Country;
use super::FieldEvidence;
use super::FieldKey;
use super::SourceModule;

/// What the bridge produced for one delegated scrape call.
#[derive(Debug, Clone)]
pub struct ScrapeBridgeResult {
    pub target_key: &'static str,
    /// Typed records the scrape script emitted, mapped onto our vocab.
    pub fields: Vec<(FieldKey, FieldEvidence)>,
    /// `ctox scrape execute` classification:
    /// `succeeded | temporary_unreachable | portal_drift | blocked | partial_output`,
    /// or `target_not_registered` when the bridge skipped the call.
    pub classification: String,
    /// Compact reason string from the executor.
    pub reason: Option<String>,
    /// Whether a CTOX repair queue task was enqueued by `--allow-heal`.
    pub repair_queued: bool,
    /// Run id assigned by the scrape executor, if the call ran.
    pub run_id: Option<String>,
}

/// Drive a scrape-target extraction for the given source module.
///
/// `root` is the CTOX state root (the dir containing `runtime/ctox.sqlite3`).
/// `ctox_bin` is the path of the `ctox` binary to invoke; pass
/// [`default_ctox_bin`] in the orchestrator.
pub fn run_via_scrape_target(
    module: &dyn SourceModule,
    company: &str,
    country: Country,
    root: &Path,
    ctox_bin: &Path,
) -> ScrapeBridgeResult {
    let Some(target_key) = module.scrape_target_key() else {
        // Not opted in — caller should use the Rust path.
        return ScrapeBridgeResult {
            target_key: "",
            fields: Vec::new(),
            classification: "no_scrape_target".to_string(),
            reason: Some("module.scrape_target_key() == None".to_string()),
            repair_queued: false,
            run_id: None,
        };
    };

    let input = json!({
        "company": company,
        "country": country.as_iso(),
        "source_id": module.id(),
    });

    let output = Command::new(ctox_bin)
        .arg("scrape")
        .arg("execute")
        .arg("--target-key")
        .arg(target_key)
        .arg("--trigger-kind")
        .arg("manual")
        .arg("--allow-heal")
        .arg("--input-json")
        .arg(input.to_string())
        .arg("--runtime-root")
        .arg(root)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(err) => {
            return ScrapeBridgeResult {
                target_key,
                fields: Vec::new(),
                classification: "subprocess_failed".to_string(),
                reason: Some(format!("failed to spawn ctox: {err}")),
                repair_queued: false,
                run_id: None,
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Distinguish "target not registered" from other failures so the
    // caller can fall back to Rust extraction cleanly.
    if !output.status.success() {
        let classification = if stderr.contains("target_key not found")
            || stdout.contains("target_key not found")
        {
            "target_not_registered"
        } else {
            "executor_error"
        };
        return ScrapeBridgeResult {
            target_key,
            fields: Vec::new(),
            classification: classification.to_string(),
            reason: Some(stderr.into_owned()),
            repair_queued: false,
            run_id: None,
        };
    }

    let envelope: Value = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(err) => {
            return ScrapeBridgeResult {
                target_key,
                fields: Vec::new(),
                classification: "parse_error".to_string(),
                reason: Some(format!("invalid JSON from scrape execute: {err}")),
                repair_queued: false,
                run_id: None,
            };
        }
    };

    parse_scrape_envelope(target_key, &envelope)
}

/// Best-effort default for the `ctox` binary path: the currently running
/// executable. Falls back to plain `"ctox"` if `current_exe` is unavailable
/// (test contexts, sandboxes).
pub fn default_ctox_bin() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ctox"))
}

fn parse_scrape_envelope(target_key: &'static str, envelope: &Value) -> ScrapeBridgeResult {
    let classification = envelope
        .get("classification")
        .and_then(|v| v.get("status"))
        .or_else(|| envelope.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let reason = envelope
        .get("classification")
        .and_then(|v| v.get("reason"))
        .and_then(Value::as_str)
        .or_else(|| envelope.get("reason").and_then(Value::as_str))
        .map(|s| s.to_string());
    let repair_queued = envelope
        .get("repair_queue_task")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    let run_id = envelope
        .get("run_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    // Records: ctox scrape execute persists them to
    // `<run_dir>/outputs/records.json` and reports the run_manifest_path
    // in the envelope. The script's raw stdout is also tail-truncated
    // into `result.stdout_excerpt`; for short runs the records may be
    // recoverable from there, but we prefer the on-disk records.json
    // because it is the durable contract documented in
    // skills/.../universal-scraping/references/storage-layout.md.
    let records: Vec<Value> = if let Some(records_path) = locate_records_file(envelope) {
        load_records_file(&records_path).unwrap_or_default()
    } else {
        envelope
            .get("records")
            .or_else(|| envelope.get("result").and_then(|v| v.get("records")))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };

    let fields = records
        .into_iter()
        .filter_map(record_to_field_evidence)
        .collect();

    ScrapeBridgeResult {
        target_key,
        fields,
        classification,
        reason,
        repair_queued,
        run_id,
    }
}

/// Try to find the on-disk records.json that the scrape executor wrote
/// for this run. Derived from `run_manifest_path` when present.
fn locate_records_file(envelope: &Value) -> Option<PathBuf> {
    let manifest = envelope
        .get("run_manifest_path")
        .and_then(Value::as_str)?;
    let manifest_path = PathBuf::from(manifest);
    let run_dir = manifest_path.parent()?;
    let candidate = run_dir.join("outputs").join("records.json");
    Some(candidate)
}

fn load_records_file(path: &Path) -> Option<Vec<Value>> {
    let raw = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    match value {
        Value::Array(items) => Some(items),
        Value::Object(map) => {
            let recs = map.get("records")?;
            recs.as_array().cloned()
        }
        _ => None,
    }
}

fn record_to_field_evidence(record: Value) -> Option<(FieldKey, FieldEvidence)> {
    let field_str = record.get("field").and_then(Value::as_str)?;
    let field = FieldKey::from_str(field_str)?;
    let value = record.get("value").and_then(Value::as_str)?.to_string();
    if value.trim().is_empty() {
        return None;
    }
    let confidence = match record.get("confidence").and_then(Value::as_str) {
        Some("high") => Confidence::High,
        Some("low") => Confidence::Low,
        Some("user_provided") => Confidence::UserProvided,
        _ => Confidence::Medium,
    };
    let source_url = record
        .get("source_url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let note = record
        .get("note")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    Some((
        field,
        FieldEvidence {
            value,
            confidence,
            source_url,
            note,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_envelope_decodes_typed_records() {
        let envelope = json!({
            "ok": true,
            "run_id": "scrape_run-abc",
            "classification": { "status": "succeeded", "reason": "" },
            "records": [
                {
                    "field": "firma_name",
                    "value": "Roche Holding AG",
                    "confidence": "high",
                    "source_url": "https://example/profile",
                    "note": "h1 selector"
                },
                {
                    "field": "person_funktion",
                    "value": "Mitglied des Verwaltungsrates",
                    "confidence": "medium",
                    "source_url": "https://example/profile"
                },
                {
                    "field": "umsatz",
                    "value": "",
                    "confidence": "high"
                }
            ]
        });
        let result = parse_scrape_envelope("zefix.ch", &envelope);
        assert_eq!(result.classification, "succeeded");
        assert_eq!(result.run_id.as_deref(), Some("scrape_run-abc"));
        assert!(!result.repair_queued);
        // Empty-value records are dropped.
        assert_eq!(result.fields.len(), 2);
        let (firma_key, firma_ev) = &result.fields[0];
        assert_eq!(*firma_key, FieldKey::FirmaName);
        assert_eq!(firma_ev.value, "Roche Holding AG");
        assert_eq!(firma_ev.confidence, Confidence::High);
        assert_eq!(firma_ev.note.as_deref(), Some("h1 selector"));
    }

    #[test]
    fn parse_envelope_marks_drift_with_repair_queue() {
        let envelope = json!({
            "ok": true,
            "run_id": "scrape_run-drift",
            "classification": { "status": "portal_drift", "reason": "reachable_empty_output" },
            "records": [],
            "repair_queue_task": {
                "queue_item_id": "queue_abc",
                "thread_key": "scrape/northdata.de"
            }
        });
        let result = parse_scrape_envelope("northdata.de", &envelope);
        assert_eq!(result.classification, "portal_drift");
        assert!(result.repair_queued);
        assert!(result.fields.is_empty());
        assert_eq!(
            result.reason.as_deref(),
            Some("reachable_empty_output")
        );
    }

    #[test]
    fn parse_envelope_handles_alt_record_shapes() {
        // Some scripts emit under result.records (legacy shape from
        // task-contracts.md).
        let envelope = json!({
            "ok": true,
            "classification": { "status": "succeeded" },
            "result": {
                "records": [
                    { "field": "firma_ort", "value": "Basel", "confidence": "high" }
                ]
            }
        });
        let result = parse_scrape_envelope("zefix.ch", &envelope);
        assert_eq!(result.fields.len(), 1);
        assert_eq!(result.fields[0].0, FieldKey::FirmaOrt);
    }
}
