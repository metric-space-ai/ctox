// Origin: CTOX
// License: AGPL-3.0-only
//
// RFC 0011 — automation-widget watcher scheduler & fire path. This is the
// store-coupled half of a widget: it builds the read-only `SignalContext` for a
// widget from the engine's datapoints, runs the CTOX-generated Rhai watcher
// (`crate::iot::watcher`, which stays pure — no DB/clock/model), persists the
// watcher's `state`, and on `fire()` drives the SAME proven, budget-bounded
// chain `conditions.rs` uses:
//
//     widget fire()
//       -> raise iot_alarm (the "CTOX is acting" surface)
//       -> mission::channels::ingest_iot_event_message
//       -> budget-bounded `iot-event-queue-task` spawn (guard) + ONE durable
//          communication_messages row seeded with the widget's `action_prompt`
//          + signal references (the "Auftrags-Prompt -> Chat-Spawn" of the spec)
//       -> Agent leases & acts under the existing review/outcome/spawn gates.
//
// There is NO LLM in this loop: the intelligence is compiled once into the Rhai
// watcher; here we only execute it per datapoint and route a fire to durable
// work. Compile/runtime errors flip the widget to `needs_attention` so CTOX can
// rewrite the program (self-repair).
//
// HARD RULES: native Rust; the clock is INJECTED (`now_ms` passed in); all state
// lands in runtime/ctox.sqlite3 via the iot store; no `std::env`; no HTTP bridge.

use crate::iot::watcher::{self, SignalContext, SignalSeries};
use crate::iot::{alarms, commands, datapoints, now_iso, store, Result};
use anyhow::{anyhow, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

/// History fed to the watcher each tick — enough for the windowed aggregates a
/// generated program is likely to request (`.window("…")`, `.avg`, `.rate`).
const WATCHER_LOOKBACK_MS: i64 = 24 * 60 * 60 * 1000;

/// Bounded re-firing per widget (mirrors the ruleset path's max_attempts).
const WIDGET_SPAWN_MAX_ATTEMPTS: i64 = 64;

/// Persisted `trigger_status` values (projected as `status_key`).
const STATUS_ARMED: &str = "armed";
const STATUS_FIRED: &str = "fired";
const STATUS_NEEDS_ATTENTION: &str = "needs_attention";
const STATUS_PAUSED: &str = "paused";

/// Outcome of ticking ONE widget.
#[derive(Clone, Debug, Default)]
pub(crate) struct WidgetTickOutcome {
    /// Reasons the watcher reported via `fire(grund)` this tick.
    pub fired: Vec<String>,
    /// A durable queue task was (re)written for this fire.
    pub spawned: bool,
    /// The fire was suppressed because the per-widget spawn budget is exhausted.
    pub budget_exhausted: bool,
    /// The watcher failed to compile or threw — widget is `needs_attention`.
    pub error: Option<String>,
    /// No watcher to run (no trigger_code yet, or paused).
    pub skipped: bool,
}

struct WidgetRow {
    realm: String,
    signal_ref: String,
    action_prompt: Option<String>,
    trigger_code: Option<String>,
    trigger_state: Option<String>,
    trigger_status: Option<String>,
}

/// Canonical signal binding form: `"<asset_id>::<attribute_name>"`.
fn parse_signal_ref(signal_ref: &str) -> Result<(&str, &str)> {
    signal_ref
        .split_once("::")
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .ok_or_else(|| {
            anyhow!("signal_ref must be '<asset_id>::<attribute_name>', got '{signal_ref}'")
        })
}

fn load_widget(conn: &Connection, widget_id: &str) -> Result<Option<WidgetRow>> {
    conn.query_row(
        "SELECT realm, signal_ref, action_prompt, trigger_code, trigger_state, trigger_status
         FROM iot_widgets WHERE id = ?1",
        params![widget_id],
        |r| {
            Ok(WidgetRow {
                realm: r.get(0)?,
                signal_ref: r.get(1)?,
                action_prompt: r.get(2)?,
                trigger_code: r.get(3)?,
                trigger_state: r.get(4)?,
                trigger_status: r.get(5)?,
            })
        },
    )
    .optional()
    .context("failed to load widget for tick")
}

fn build_series(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
) -> Result<SignalSeries> {
    let points = datapoints::all(conn, asset_id, attribute_name, from_ms, to_ms)?
        .into_iter()
        .filter_map(|dp| dp.value.as_numeric().map(|v| (dp.timestamp_ms, v)))
        .collect::<Vec<_>>();
    Ok(SignalSeries::new(points))
}

/// Persist the watcher's state + status and reproject the widget so the UI sees
/// the new status immediately.
fn persist(
    conn: &Connection,
    widget_id: &str,
    state: &Value,
    status: &str,
    now_iso_str: &str,
) -> Result<()> {
    let state_json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "UPDATE iot_widgets SET trigger_state = ?2, trigger_status = ?3, updated_at = ?4 WHERE id = ?1",
        params![widget_id, state_json, status, now_iso_str],
    )
    .context("failed to persist widget watcher state")?;
    commands::project_widget(conn, widget_id)?;
    Ok(())
}

/// The chat seed: the human's `action_prompt` (the "Dann") plus the concrete
/// references the agent needs — which signal, why it fired, the last value and a
/// short tail of the series, and the asset. This is what the spawned chat opens
/// with; the durable work lives there, not in the widget.
fn build_seed_prompt(
    action_prompt: Option<&str>,
    signal_ref: &str,
    asset_id: &str,
    reasons: &[String],
    series: &SignalSeries,
    now_ms: i64,
) -> String {
    let prompt = action_prompt
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Prüfe das ausgelöste Signal und handle angemessen.");
    let last = series
        .points
        .last()
        .map(|(t, v)| format!("{v} (vor {}s)", (now_ms.saturating_sub(*t)) / 1000))
        .unwrap_or_else(|| "—".to_string());
    let tail: Vec<String> = series
        .points
        .iter()
        .rev()
        .take(8)
        .map(|(_, v)| format!("{v}"))
        .collect();
    let tail = {
        let mut t = tail;
        t.reverse();
        t.join(", ")
    };
    format!(
        "{prompt}\n\n— Auslöser —\n\
         Signal: {signal_ref}\n\
         Asset: {asset_id}\n\
         Grund: {reasons}\n\
         Letztwert: {last}\n\
         Letzte Werte: {tail}\n\n\
         Handle gemäß Skill `iot-operations`. Die Überwachung läuft weiter.",
        reasons = reasons.join("; "),
    )
}

/// Tick ONE widget: build its signal context, run the watcher, persist state,
/// and route a fire to the durable queue-task chain. Pure w.r.t. the clock —
/// `now_ms` is injected (production passes `crate::iot::now_ms()`).
pub(crate) fn tick_widget(root: &std::path::Path, widget_id: &str, now_ms: i64) -> Result<WidgetTickOutcome> {
    let conn = store::open_iot_store(root)?;
    commands::ensure_stub_schema(&conn)?;

    let Some(row) = load_widget(&conn, widget_id)? else {
        return Ok(WidgetTickOutcome {
            skipped: true,
            ..Default::default()
        });
    };

    // Nothing to run: no program yet (compile_trigger pending) or paused.
    let code = row.trigger_code.clone().unwrap_or_default();
    if code.trim().is_empty() || row.trigger_status.as_deref() == Some(STATUS_PAUSED) {
        return Ok(WidgetTickOutcome {
            skipped: true,
            ..Default::default()
        });
    }

    let (asset_id, attribute_name) = parse_signal_ref(&row.signal_ref)?;
    let primary = build_series(
        &conn,
        asset_id,
        attribute_name,
        now_ms.saturating_sub(WATCHER_LOOKBACK_MS),
        now_ms,
    )?;

    let state_in: Value = row
        .trigger_state
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| json!({}));

    let ctx = SignalContext {
        primary: primary.clone(),
        named: Vec::new(),
        now_ms,
    };
    let outcome = watcher::evaluate(&code, &ctx, &state_in);
    let now_iso_str = now_iso();

    // Watcher failed → self-repair: persist the (unchanged) state, flip status,
    // and surface the error for CTOX to rewrite the program.
    if let Some(err) = outcome.error {
        persist(&conn, widget_id, &outcome.state, STATUS_NEEDS_ATTENTION, &now_iso_str)?;
        return Ok(WidgetTickOutcome {
            error: Some(err),
            ..Default::default()
        });
    }

    // No fire: persist updated state (counters/hysteresis), stay armed.
    if outcome.fired.is_empty() {
        persist(&conn, widget_id, &outcome.state, STATUS_ARMED, &now_iso_str)?;
        return Ok(WidgetTickOutcome {
            fired: Vec::new(),
            ..Default::default()
        });
    }

    // Fire → durable work. Reuse the proven, budget-bounded chain.
    persist(&conn, widget_id, &outcome.state, STATUS_FIRED, &now_iso_str)?;

    // Ensure the iot_alarms schema exists on the shared core db (same pattern as
    // conditions::emit_matches). ensure_stub_schema does not create it.
    alarms::open(root)?;

    let source = if row.realm.trim().is_empty() {
        alarms::Source::GlobalRuleset
    } else {
        alarms::Source::RealmRuleset
    };
    let alarm = alarms::create(
        &conn,
        alarms::NewAlarm {
            realm: row.realm.clone(),
            title: format!("IoT-Auftrag ausgelöst: {}", row.signal_ref),
            content: Some(outcome.fired.join("; ")),
            severity: alarms::Severity::Medium,
            assignee_id: None,
            source,
            source_id: widget_id.to_string(),
        },
        vec![asset_id.to_string()],
    )?;

    let body = build_seed_prompt(
        row.action_prompt.as_deref(),
        &row.signal_ref,
        asset_id,
        &outcome.fired,
        &primary,
        now_ms,
    );
    let dedup_key = format!("widget:{widget_id}");
    let budget_key = format!("iot-widget:{widget_id}");
    let emit = crate::mission::channels::ingest_iot_event_message(
        root,
        &alarm.id,
        widget_id, // ruleset_id slot — used only as a key/label here
        asset_id,
        &dedup_key,
        &budget_key,
        WIDGET_SPAWN_MAX_ATTEMPTS,
        &row.signal_ref,
        &body,
        Some("iot-operations"),
        &now_iso_str,
    )?;

    if emit.message_key.is_some() {
        Ok(WidgetTickOutcome {
            fired: outcome.fired,
            spawned: true,
            ..Default::default()
        })
    } else if emit
        .spawn
        .violation_codes
        .iter()
        .any(|c| c == "spawn_budget_exhausted")
    {
        // Bounded re-firing: budget exhausted, work suppressed (not an error).
        Ok(WidgetTickOutcome {
            fired: outcome.fired,
            budget_exhausted: true,
            ..Default::default()
        })
    } else {
        Err(anyhow!(
            "widget queue-task spawn rejected: {}",
            emit.spawn.message
        ))
    }
}

/// Tick every widget bound to `(asset_id, attribute_name)`. Called from the
/// inbound datapoint path so each new sample drives its watchers statefully
/// ("Wächter feuert pro Datenpunkt"). A per-widget failure is captured in that
/// widget's outcome and never aborts the others.
pub(crate) fn tick_widgets_for_signal(
    root: &std::path::Path,
    asset_id: &str,
    attribute_name: &str,
    now_ms: i64,
) -> Result<Vec<(String, WidgetTickOutcome)>> {
    let signal_ref = format!("{asset_id}::{attribute_name}");
    let ids: Vec<String> = {
        let conn = store::open_iot_store(root)?;
        commands::ensure_stub_schema(&conn)?;
        let mut stmt = conn
            .prepare(
                "SELECT id FROM iot_widgets
                 WHERE signal_ref = ?1 AND trigger_code IS NOT NULL AND trigger_code != ''",
            )
            .context("failed to query widgets for signal")?;
        let collected = stmt
            .query_map(params![signal_ref], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect widget ids")?;
        collected
    };

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        match tick_widget(root, &id, now_ms) {
            Ok(o) => out.push((id, o)),
            Err(e) => out.push((
                id,
                WidgetTickOutcome {
                    error: Some(e.to_string()),
                    ..Default::default()
                },
            )),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::commands::WidgetUpsertReq;
    use crate::iot::model::AttributeValue;

    fn record(conn: &Connection, asset: &str, attr: &str, v: f64, ts: i64) {
        datapoints::record_datapoint(conn, asset, attr, &AttributeValue(json!(v)), ts).unwrap();
    }

    fn upsert_widget(root: &std::path::Path, code: &str) {
        let req = WidgetUpsertReq {
            id: Some("w1".to_string()),
            dashboard_id: "d1".to_string(),
            realm: "master".to_string(),
            signal_ref: "asset-1::temperature".to_string(),
            cond_text: Some("wenn es zu heiß wird".to_string()),
            action_prompt: Some("Kühlung hochfahren und melden".to_string()),
            trigger_code: Some(code.to_string()),
            render_code: None,
            x: None,
            y: None,
            w: None,
            h: None,
            sort_index: None,
        };
        commands::widget_upsert(root, req, None).unwrap();
    }

    #[test]
    fn parse_signal_ref_requires_both_halves() {
        assert_eq!(parse_signal_ref("a::b").unwrap(), ("a", "b"));
        assert!(parse_signal_ref("nope").is_err());
        assert!(parse_signal_ref("::b").is_err());
        assert!(parse_signal_ref("a::").is_err());
    }

    // The P1 done-criterion: a CTOX-shaped watcher fires on real datapoints and
    // a durable queue task (the chat-spawn seed) lands — WITHOUT any model.
    #[test]
    fn watcher_fires_and_spawns_durable_queue_task() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let conn = store::open_iot_store(root).unwrap();
        commands::ensure_stub_schema(&conn).unwrap();
        datapoints::init_schema(&conn).unwrap();

        // A hot reading on the bound signal.
        record(&conn, "asset-1", "temperature", 35.0, 1_000);

        upsert_widget(root, r#"if signal.last() > 30.0 { fire("Serverraum zu heiß"); }"#);

        let out = tick_widget(root, "w1", 2_000).unwrap();
        assert_eq!(out.fired, vec!["Serverraum zu heiß".to_string()]);
        assert!(out.spawned, "a durable queue task should be written");
        assert!(out.error.is_none());

        // The widget is now persisted + projected as `fired`.
        let status: String = conn
            .query_row(
                "SELECT trigger_status FROM iot_widgets WHERE id = 'w1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, STATUS_FIRED);

        // A durable IoT communication message exists (the queue-task/chat seed).
        let core = crate::paths::core_db(root);
        let ch = Connection::open(&core).unwrap();
        let count: i64 = ch
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE channel = 'iot'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert!(count >= 1, "expected a durable iot queue-task message");
    }

    // Per-datapoint dispatch: a new sample on a bound signal ticks exactly the
    // widgets watching it (matched by signal_ref), and they fire.
    #[test]
    fn dispatch_by_signal_ref_ticks_bound_widgets() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let conn = store::open_iot_store(root).unwrap();
        commands::ensure_stub_schema(&conn).unwrap();
        datapoints::init_schema(&conn).unwrap();
        record(&conn, "asset-1", "temperature", 35.0, 1_000);

        upsert_widget(root, r#"if signal.last() > 30.0 { fire("heiß"); }"#);

        // A widget on a DIFFERENT signal must not be ticked by this sample.
        commands::widget_upsert(
            root,
            WidgetUpsertReq {
                id: Some("w-other".to_string()),
                dashboard_id: "d1".to_string(),
                realm: "master".to_string(),
                signal_ref: "asset-1::humidity".to_string(),
                cond_text: None,
                action_prompt: None,
                trigger_code: Some(r#"fire("nope");"#.to_string()),
                render_code: None,
                x: None,
                y: None,
                w: None,
                h: None,
                sort_index: None,
            },
            None,
        )
        .unwrap();

        let results = tick_widgets_for_signal(root, "asset-1", "temperature", 2_000).unwrap();
        assert_eq!(results.len(), 1, "only the temperature widget matches");
        assert_eq!(results[0].0, "w1");
        assert_eq!(results[0].1.fired, vec!["heiß".to_string()]);
        assert!(results[0].1.spawned);
    }

    // No trigger_code yet (compile_trigger pending) → tick is a no-op, not an error.
    #[test]
    fn widget_without_program_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let conn = store::open_iot_store(root).unwrap();
        commands::ensure_stub_schema(&conn).unwrap();
        datapoints::init_schema(&conn).unwrap();
        record(&conn, "asset-1", "temperature", 35.0, 1_000);

        // Upsert with no trigger_code.
        commands::widget_upsert(
            root,
            WidgetUpsertReq {
                id: Some("w1".to_string()),
                dashboard_id: "d1".to_string(),
                realm: "master".to_string(),
                signal_ref: "asset-1::temperature".to_string(),
                cond_text: None,
                action_prompt: None,
                trigger_code: None,
                render_code: None,
                x: None,
                y: None,
                w: None,
                h: None,
                sort_index: None,
            },
            None,
        )
        .unwrap();

        let out = tick_widget(root, "w1", 2_000).unwrap();
        assert!(out.skipped);
        assert!(out.fired.is_empty());
    }

    // A broken watcher flips the widget to needs_attention and never panics.
    #[test]
    fn broken_watcher_marks_needs_attention() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let conn = store::open_iot_store(root).unwrap();
        commands::ensure_stub_schema(&conn).unwrap();
        datapoints::init_schema(&conn).unwrap();
        record(&conn, "asset-1", "temperature", 35.0, 1_000);

        upsert_widget(root, "this is @@@ not rhai");
        let out = tick_widget(root, "w1", 2_000).unwrap();
        assert!(out.error.is_some());
        assert!(!out.spawned);

        let status: String = conn
            .query_row(
                "SELECT trigger_status FROM iot_widgets WHERE id = 'w1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, STATUS_NEEDS_ATTENTION);
    }
}
