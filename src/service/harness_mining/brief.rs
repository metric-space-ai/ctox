// Origin: CTOX
// License: Apache-2.0
//
// harness-mining brief — the agent-shaped synthesis layer over Tier 1+2.
//
// Runs all eight algorithms once, ranks the signals, and emits a compact
// JSON briefing the agent can read directly into a plan step:
//
//   status          : healthy | attention_required | drift_detected
//   top_signal      : one-sentence narrative of the most pressing finding
//   recommended_next_step : a concrete CLI invocation the agent should make next
//   metrics         : flat numeric block (counts, ratios, alarms)
//   findings        : structured list of the strongest evidence items
//
// Brief is the natural pre-skill call for incident-response, queue-cleanup,
// and follow-up-orchestrator. It is also the input shape for the audit-tick.

use crate::service::harness_mining::{conformance, drift, sojourn, stuck_cases, variants};
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub stuck_min_attempts: i64,
    pub conformance_threshold: f64,
    pub drift_threshold: f64,
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        Self {
            stuck_min_attempts: super::parse_i64_flag(args, "--stuck-min-attempts", 5)
                .clamp(1, 1000),
            conformance_threshold: super::parse_f64_flag(args, "--conformance-threshold", 0.95)
                .clamp(0.0, 1.0),
            drift_threshold: super::parse_f64_flag(args, "--drift-threshold", 5.0).max(0.1),
        }
    }
}

pub fn synthesize(conn: &Connection, opts: &Options) -> Result<Value> {
    // Run all the underlying algorithms with conservative defaults; if one
    // fails we record the failure but continue, so a corrupt sub-table
    // never silences the whole brief.
    let stuck = run_or_error(|| {
        stuck_cases::detect(
            conn,
            &stuck_cases::Options {
                min_attempts: opts.stuck_min_attempts,
                idle_seconds: 0,
                limit: 50,
            },
        )
    });
    let conf = run_or_error(|| {
        conformance::replay(
            conn,
            &conformance::Options {
                lane: None,
                window: 1000,
                fitness_threshold: opts.conformance_threshold,
            },
        )
    });
    let drift_report = run_or_error(|| {
        drift::detect(
            conn,
            &drift::Options {
                window: 1000,
                threshold: opts.drift_threshold,
            },
        )
    });
    let soj = run_or_error(|| sojourn::analyze(conn, &sojourn::Options::default()));
    let var = run_or_error(|| variants::analyze(conn, &variants::Options::default()));

    let stuck_count = stuck
        .as_ref()
        .ok()
        .and_then(|v| v["case_count"].as_i64())
        .unwrap_or(0);
    let stuck_top = stuck
        .as_ref()
        .ok()
        .and_then(|v| v["cases"].as_array().and_then(|a| a.first()).cloned());

    let conformance_ok = conf
        .as_ref()
        .ok()
        .and_then(|v| v["fitness_ok"].as_bool())
        .unwrap_or(true);
    let preventive_fitness = conf
        .as_ref()
        .ok()
        .and_then(|v| v["preventive"]["fitness"].as_f64())
        .unwrap_or(1.0);
    let trigger_fitness = conf
        .as_ref()
        .ok()
        .and_then(|v| v["trigger"]["fitness"].as_f64())
        .unwrap_or(1.0);
    let trigger_top_breach = conf.as_ref().ok().and_then(|v| {
        v["trigger"]["failing_buckets"]
            .as_array()
            .and_then(|a| a.first())
            .cloned()
    });

    let drift_detected = drift_report
        .as_ref()
        .ok()
        .and_then(|v| v["drift_detected"].as_bool())
        .unwrap_or(false);
    let drift_top = drift_report.as_ref().ok().and_then(|v| {
        v["chi_squared_activity"]["top_drift_activities"]
            .as_array()
            .and_then(|a| a.first())
            .cloned()
    });

    let worst_p95 = soj
        .as_ref()
        .ok()
        .and_then(|v| v["states"].as_array())
        .and_then(|states| states.first())
        .and_then(|s| {
            s["p95_seconds"].as_f64().map(|p95| {
                json!({
                    "entity_type": s["entity_type"],
                    "state": s["state"],
                    "p95_seconds": p95,
                    "observations": s["observations"],
                })
            })
        });

    let dominant_variant_share = var
        .as_ref()
        .ok()
        .and_then(|v| v["variants"].as_array().and_then(|a| a.first()))
        .and_then(|v| v["share"].as_f64())
        .unwrap_or(0.0);

    // Rank the signals — order matters because top_signal is what the
    // agent reads first. A confirmed conformance breach always outranks
    // drift, which outranks stuck cases, which outranks bottlenecks.
    let mut findings: Vec<Value> = Vec::new();
    if !conformance_ok {
        findings.push(json!({
            "kind": "conformance_breach",
            "severity": "critical",
            "preventive_fitness": round4(preventive_fitness),
            "trigger_fitness": round4(trigger_fitness),
            "top_breach": trigger_top_breach,
        }));
    }
    if drift_detected {
        findings.push(json!({
            "kind": "drift",
            "severity": "warning",
            "top_driver": drift_top,
        }));
    }
    if stuck_count > 0 {
        findings.push(json!({
            "kind": "stuck_cases",
            "severity": if stuck_count >= 3 { "critical" } else { "warning" },
            "case_count": stuck_count,
            "worst_case": stuck_top,
        }));
    }
    if let Some(slow) = &worst_p95 {
        if slow["p95_seconds"].as_f64().unwrap_or(0.0) > 600.0 {
            findings.push(json!({
                "kind": "bottleneck",
                "severity": "info",
                "state": slow,
            }));
        }
    }

    let (status, top_signal, next_step) = top_signal_and_next(
        &findings,
        conformance_ok,
        drift_detected,
        stuck_count,
        worst_p95.as_ref(),
    );

    let metrics = json!({
        "stuck_case_count": stuck_count,
        "preventive_fitness": round4(preventive_fitness),
        "trigger_fitness": round4(trigger_fitness),
        "conformance_ok": conformance_ok,
        "drift_detected": drift_detected,
        "dominant_variant_share": round4(dominant_variant_share),
        "worst_state_p95_seconds": worst_p95
            .as_ref()
            .and_then(|v| v["p95_seconds"].as_f64())
            .map(round2),
    });

    let errors = collect_errors(&[
        ("stuck_cases", &stuck),
        ("conformance", &conf),
        ("drift", &drift_report),
        ("sojourn", &soj),
        ("variants", &var),
    ]);

    Ok(json!({
        "ok": true,
        "tier": "synthesis",
        "algorithm": "harness-mining-brief",
        "status": status,
        "top_signal": top_signal,
        "recommended_next_step": next_step,
        "metrics": metrics,
        "findings": findings,
        "errors": errors,
    }))
}

fn run_or_error<F>(f: F) -> Result<Value, String>
where
    F: FnOnce() -> Result<Value>,
{
    f().map_err(|e| e.to_string())
}

fn collect_errors(named: &[(&str, &Result<Value, String>)]) -> Vec<Value> {
    named
        .iter()
        .filter_map(|(name, r)| match r {
            Err(e) => Some(json!({ "stage": name, "error": e })),
            Ok(_) => None,
        })
        .collect()
}

fn top_signal_and_next(
    findings: &[Value],
    conformance_ok: bool,
    drift_detected: bool,
    stuck_count: i64,
    worst_p95: Option<&Value>,
) -> (&'static str, String, String) {
    if !conformance_ok {
        let breach = findings
            .iter()
            .find(|f| f["kind"] == "conformance_breach")
            .cloned()
            .unwrap_or(json!({}));
        let preventive = breach["preventive_fitness"].as_f64().unwrap_or(0.0);
        let trigger = breach["trigger_fitness"].as_f64().unwrap_or(0.0);
        return (
            "attention_required",
            format!(
                "Conformance breach: preventive={:.0}%, trigger={:.0}% — declared spec is being violated.",
                preventive * 100.0,
                trigger * 100.0
            ),
            "ctox harness-mining conformance --window 2000 --fitness-threshold 0.95"
                .to_string(),
        );
    }
    if stuck_count > 0 {
        let worst = findings
            .iter()
            .find(|f| f["kind"] == "stuck_cases")
            .and_then(|f| f["worst_case"].as_object().cloned());
        let entity_type = worst
            .as_ref()
            .and_then(|m| m.get("entity_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let attempts = worst
            .as_ref()
            .and_then(|m| m.get("rejected_attempts"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        return (
            "attention_required",
            format!(
                "{stuck_count} stuck case(s); worst is a {entity_type} with {attempts} rejected attempts."
            ),
            "ctox harness-mining stuck-cases --min-attempts 5 --limit 20".to_string(),
        );
    }
    if drift_detected {
        let driver = findings
            .iter()
            .find(|f| f["kind"] == "drift")
            .and_then(|f| f["top_driver"]["activity"].as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        return (
            "drift_detected",
            format!("Concept drift detected; top driver activity is {driver}."),
            "ctox harness-mining drift --window 1000".to_string(),
        );
    }
    if let Some(slow) = worst_p95 {
        if slow["p95_seconds"].as_f64().unwrap_or(0.0) > 600.0 {
            let state = slow["state"].as_str().unwrap_or("?");
            let p95 = slow["p95_seconds"].as_f64().unwrap_or(0.0);
            return (
                "healthy",
                format!("Bottleneck candidate: {state} dwell p95={p95:.0}s."),
                "ctox harness-mining sojourn --limit 30".to_string(),
            );
        }
    }
    (
        "healthy",
        "Harness conformant, no drift, no stuck cases.".to_string(),
        "no action required".to_string(),
    )
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_core_transition_proofs (
                proof_id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                lane TEXT NOT NULL,
                from_state TEXT NOT NULL,
                to_state TEXT NOT NULL,
                core_event TEXT NOT NULL,
                actor TEXT NOT NULL,
                accepted INTEGER NOT NULL,
                violation_codes_json TEXT NOT NULL DEFAULT '[]',
                request_json TEXT NOT NULL DEFAULT '{}',
                report_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE ctox_process_events (
                event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                observed_at TEXT NOT NULL,
                case_id TEXT NOT NULL,
                activity TEXT NOT NULL,
                lifecycle_transition TEXT NOT NULL DEFAULT 'complete',
                entity_type TEXT NOT NULL DEFAULT 'x',
                entity_id TEXT NOT NULL DEFAULT 'x',
                table_name TEXT NOT NULL DEFAULT 'x',
                operation TEXT NOT NULL DEFAULT 'INSERT',
                from_state TEXT,
                to_state TEXT,
                primary_key_json TEXT NOT NULL DEFAULT '{}',
                row_before_json TEXT NOT NULL DEFAULT '{}',
                row_after_json TEXT NOT NULL DEFAULT '{}',
                changed_columns_json TEXT NOT NULL DEFAULT '[]',
                turn_id TEXT,
                command_id TEXT,
                actor_key TEXT,
                source TEXT,
                command_name TEXT,
                db_path TEXT NOT NULL DEFAULT '',
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE ctox_pm_state_violations (
                violation_id TEXT PRIMARY KEY,
                event_id TEXT,
                case_id TEXT NOT NULL,
                violation_code TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'critical',
                message TEXT NOT NULL DEFAULT '',
                detected_at TEXT NOT NULL,
                evidence_json TEXT NOT NULL DEFAULT '{}'
            );
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn brief_returns_healthy_on_empty_db() {
        let conn = setup_conn();
        let report = synthesize(&conn, &Options::default()).unwrap();
        assert_eq!(report["status"], "healthy");
        assert_eq!(report["metrics"]["stuck_case_count"], 0);
        assert!(report["recommended_next_step"]
            .as_str()
            .unwrap()
            .contains("no action"));
    }

    #[test]
    fn brief_surfaces_stuck_cases_when_proofs_pile_up() {
        let conn = setup_conn();
        for i in 0..7 {
            conn.execute(
                r#"INSERT INTO ctox_core_transition_proofs
                    (proof_id, entity_type, entity_id, lane, from_state, to_state,
                     core_event, actor, accepted, violation_codes_json, created_at, updated_at)
                   VALUES (?1, 'FounderCommunication', 'e1', 'P0FounderCommunication',
                           'Approved', 'Sending', 'Send', 'upgrade', 0,
                           '["founder_send_body_hash_mismatch"]',
                           ?2, ?2)"#,
                params![format!("p{i}"), format!("2026-04-25T22:55:{:02}.000Z", i)],
            )
            .unwrap();
        }
        let report = synthesize(&conn, &Options::default()).unwrap();
        assert_eq!(report["status"], "attention_required");
        assert!(report["top_signal"]
            .as_str()
            .unwrap()
            .contains("stuck case"));
        assert_eq!(report["metrics"]["stuck_case_count"], 1);
    }

    #[test]
    fn brief_recommends_concrete_next_step() {
        let conn = setup_conn();
        for i in 0..7 {
            conn.execute(
                r#"INSERT INTO ctox_core_transition_proofs
                    (proof_id, entity_type, entity_id, lane, from_state, to_state,
                     core_event, actor, accepted, violation_codes_json, created_at, updated_at)
                   VALUES (?1, 'FounderCommunication', 'e1', 'P0FounderCommunication',
                           'Approved', 'Sending', 'Send', 'upgrade', 0,
                           '[]',
                           ?2, ?2)"#,
                params![format!("p{i}"), format!("2026-04-25T22:55:{:02}.000Z", i)],
            )
            .unwrap();
        }
        let report = synthesize(&conn, &Options::default()).unwrap();
        let next_step = report["recommended_next_step"].as_str().unwrap();
        assert!(next_step.starts_with("ctox harness-mining"));
    }
}
