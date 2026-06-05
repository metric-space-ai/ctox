// Origin: CTOX
// License: AGPL-3.0-only
//
// RFC 0011 — IoT automation-widget trigger runtime (the "Trigger-Logik" part of
// a widget). A widget's watcher is a small CTOX-GENERATED Rhai program that
// observes a signal stream and calls `fire(grund)` when the human's free-text
// "Wenn" condition holds. CTOX writes the program once (via `compile_trigger`);
// this runtime executes it STATEFULLY per datapoint — there is NO LLM in the
// loop. The Rust backend exposes a small read-only signal API; the watcher can
// do nothing else, which is what makes it a real sandbox.
//
// HARD RULES honored here:
//   * Rust-native, sandboxed: a fresh `rhai::Engine` registers ONLY the signal
//     API below. Rhai has no FS/net by default; `eval` is disabled; hard
//     operation / call-depth / string / array / map limits bound a generated
//     (untrusted) program so it cannot hang or exhaust memory.
//   * Pure & injectable: `evaluate` is a synchronous fn over an in-memory
//     `SignalContext` + a JSON `state`. No DB, no clock read, no `std::env` —
//     the scheduler builds the context from the store and injects `now_ms`.
//   * Self-repair friendly: compile/runtime errors are CAPTURED into
//     `WatcherOutcome.error` (never panic), so the caller can flip the widget to
//     `trigger_status = "needs_attention"` and have CTOX rewrite the program.

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, Position, Scope};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// Sandbox limits — generous enough for real watcher logic (windows, hysteresis,
// counters) but a hard backstop against a runaway generated program.
const MAX_OPERATIONS: u64 = 500_000;
const MAX_CALL_LEVELS: usize = 32;
const MAX_STRING_SIZE: usize = 16 * 1024;
const MAX_ARRAY_SIZE: usize = 50_000;
const MAX_MAP_SIZE: usize = 10_000;
const MAX_SCRIPT_BYTES: usize = 64 * 1024;

/// A time-ordered numeric series for one signal, ascending by `ts_ms`.
#[derive(Clone, Debug, Default)]
pub(crate) struct SignalSeries {
    /// `(epoch_ms, value)`, sorted ascending by time.
    pub points: Vec<(i64, f64)>,
}

impl SignalSeries {
    pub(crate) fn new(mut points: Vec<(i64, f64)>) -> Self {
        points.sort_by_key(|p| p.0);
        Self { points }
    }

    fn last_value(&self) -> Option<f64> {
        self.points.last().map(|p| p.1)
    }

    fn last_ts(&self) -> Option<i64> {
        self.points.last().map(|p| p.0)
    }

    /// Values within the trailing `dur` ending at `now_ms`.
    fn window(&self, dur: Duration, now_ms: i64) -> Vec<f64> {
        let cutoff = now_ms.saturating_sub(dur.as_millis() as i64);
        self.points
            .iter()
            .filter(|(t, _)| *t >= cutoff)
            .map(|(_, v)| *v)
            .collect()
    }

    /// Linear rate over the window: (last - first) / dt_seconds. 0 if < 2 points
    /// or a non-positive time span.
    fn rate(&self, dur: Duration, now_ms: i64) -> f64 {
        let cutoff = now_ms.saturating_sub(dur.as_millis() as i64);
        let w: Vec<(i64, f64)> = self
            .points
            .iter()
            .copied()
            .filter(|(t, _)| *t >= cutoff)
            .collect();
        if w.len() < 2 {
            return 0.0;
        }
        let (t0, v0) = w[0];
        let (t1, v1) = *w.last().unwrap();
        let dt = (t1 - t0) as f64 / 1000.0;
        if dt <= 0.0 {
            0.0
        } else {
            (v1 - v0) / dt
        }
    }
}

/// What the scheduler hands the watcher for one evaluation: the bound signal,
/// any extra named signals the program may reference, and the injected clock.
pub(crate) struct SignalContext {
    pub primary: SignalSeries,
    pub named: Vec<(String, SignalSeries)>,
    pub now_ms: i64,
}

/// The result of one evaluation. `error.is_some()` means the program failed to
/// compile or threw at runtime (including hitting a sandbox limit) — the caller
/// should mark the widget as needing attention and NOT treat `fired` as valid.
#[derive(Clone, Debug)]
pub(crate) struct WatcherOutcome {
    pub fired: Vec<String>,
    pub state: Value,
    pub error: Option<String>,
}

/// A signal as seen from inside the script (Rhai custom type "Signal").
#[derive(Clone)]
struct SignalHandle {
    series: SignalSeries,
    now_ms: i64,
}

fn rt_err(msg: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        msg.into().into(),
        Position::NONE,
    ))
}

/// Parse a human duration like `"15m"`, `"30s"`, `"2h"`, `"1d"`, `"500ms"`.
fn parse_duration(s: &str) -> std::result::Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".to_string());
    }
    let idx = s
        .find(|c: char| c.is_alphabetic())
        .ok_or_else(|| format!("duration '{s}' has no unit (use ms/s/m/h/d)"))?;
    let (num, unit) = s.split_at(idx);
    let n: f64 = num
        .trim()
        .parse()
        .map_err(|_| format!("duration '{s}' has a bad number"))?;
    if !n.is_finite() || n < 0.0 {
        return Err(format!("duration '{s}' must be finite and non-negative"));
    }
    let ms = match unit.trim() {
        "ms" => n,
        "s" => n * 1_000.0,
        "m" => n * 60_000.0,
        "h" => n * 3_600_000.0,
        "d" => n * 86_400_000.0,
        other => return Err(format!("unknown duration unit '{other}' (use ms/s/m/h/d)")),
    };
    Ok(Duration::from_millis(ms as u64))
}

impl SignalHandle {
    fn last(&mut self) -> f64 {
        self.series.last_value().unwrap_or(f64::NAN)
    }

    fn has_data(&mut self) -> bool {
        !self.series.points.is_empty()
    }

    /// Milliseconds since the most recent sample (`i64::MAX` if no data).
    fn age_ms(&mut self) -> i64 {
        self.series
            .last_ts()
            .map(|t| self.now_ms.saturating_sub(t))
            .unwrap_or(i64::MAX)
    }

    fn window(&mut self, dur: &str) -> std::result::Result<Array, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        Ok(self
            .series
            .window(d, self.now_ms)
            .into_iter()
            .map(Dynamic::from_float)
            .collect())
    }

    fn rate(&mut self, dur: &str) -> std::result::Result<f64, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        Ok(self.series.rate(d, self.now_ms))
    }

    fn count(&mut self, dur: &str) -> std::result::Result<i64, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        Ok(self.series.window(d, self.now_ms).len() as i64)
    }

    fn avg(&mut self, dur: &str) -> std::result::Result<f64, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        let w = self.series.window(d, self.now_ms);
        if w.is_empty() {
            Ok(f64::NAN)
        } else {
            Ok(w.iter().sum::<f64>() / w.len() as f64)
        }
    }

    fn min(&mut self, dur: &str) -> std::result::Result<f64, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        Ok(self
            .series
            .window(d, self.now_ms)
            .into_iter()
            .fold(f64::NAN, f64::min))
    }

    fn max(&mut self, dur: &str) -> std::result::Result<f64, Box<EvalAltResult>> {
        let d = parse_duration(dur).map_err(rt_err)?;
        Ok(self
            .series
            .window(d, self.now_ms)
            .into_iter()
            .fold(f64::NAN, f64::max))
    }
}

/// Build the sandboxed engine + register the read-only signal API and `fire`.
/// `fired` is shared so `fire(grund)` can record reasons during evaluation.
fn build_engine(named: Vec<(String, SignalHandle)>, fired: Rc<RefCell<Vec<String>>>) -> Engine {
    let mut engine = Engine::new();

    // Sandbox: bound work + sizes, kill `eval`. No FS/net exists in Rhai to begin
    // with, and we register no custom modules/IO.
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ARRAY_SIZE);
    engine.set_max_map_size(MAX_MAP_SIZE);
    engine.set_max_expr_depths(128, 64);
    engine.disable_symbol("eval");
    // Generated code must not spam the host; discard print/debug.
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});

    engine
        .register_type_with_name::<SignalHandle>("Signal")
        .register_fn("last", SignalHandle::last)
        .register_fn("has_data", SignalHandle::has_data)
        .register_fn("age_ms", SignalHandle::age_ms)
        .register_fn("window", SignalHandle::window)
        .register_fn("rate", SignalHandle::rate)
        .register_fn("count", SignalHandle::count)
        .register_fn("avg", SignalHandle::avg)
        .register_fn("min", SignalHandle::min)
        .register_fn("max", SignalHandle::max);

    // `signals("name")` → the named Signal (read-only lookup by binding name).
    engine.register_fn(
        "signals",
        move |name: &str| -> std::result::Result<SignalHandle, Box<EvalAltResult>> {
            named
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, h)| h.clone())
                .ok_or_else(|| rt_err(format!("unknown signal '{name}'")))
        },
    );

    // `fire(grund)` records a trigger reason; `fire()` uses a default reason.
    let fired_one = Rc::clone(&fired);
    engine.register_fn("fire", move |reason: &str| {
        fired_one.borrow_mut().push(reason.to_string());
    });
    let fired_zero = Rc::clone(&fired);
    engine.register_fn("fire", move || {
        fired_zero.borrow_mut().push("trigger".to_string());
    });

    engine
}

/// Convert a JSON object into a Rhai `state` map. Non-objects become an empty map.
fn json_to_state_map(state_in: &Value) -> Map {
    match rhai::serde::to_dynamic(state_in) {
        Ok(d) => d.try_cast::<Map>().unwrap_or_default(),
        Err(_) => Map::new(),
    }
}

/// Read the (possibly mutated) `state` back out as a JSON object.
fn state_map_to_json(map: Map) -> Value {
    serde_json::to_value(map).unwrap_or_else(|_| json!({}))
}

/// Evaluate a CTOX-generated watcher program once, statefully.
///
/// In scope for the program:
///   * `signal`  — the bound Signal (`.last()`, `.window("15m")`, `.rate("5m")`,
///                 `.avg/.min/.max/.count("…")`, `.age_ms()`, `.has_data()`)
///   * `signals("name")` — a read-only lookup of another bound Signal
///   * `state`   — a persistent map carried across calls (hysteresis, counters)
///   * `now_ms`  — the injected evaluation clock (epoch-ms)
///   * `fire(grund)` / `fire()` — report that the condition held
pub(crate) fn evaluate(trigger_code: &str, ctx: &SignalContext, state_in: &Value) -> WatcherOutcome {
    let state_fallback = if state_in.is_object() {
        state_in.clone()
    } else {
        json!({})
    };

    if trigger_code.trim().is_empty() {
        return WatcherOutcome {
            fired: Vec::new(),
            state: state_fallback,
            error: Some("trigger code is empty".to_string()),
        };
    }
    if trigger_code.len() > MAX_SCRIPT_BYTES {
        return WatcherOutcome {
            fired: Vec::new(),
            state: state_fallback,
            error: Some(format!(
                "trigger code is too large ({} bytes, max {MAX_SCRIPT_BYTES})",
                trigger_code.len()
            )),
        };
    }

    let fired = Rc::new(RefCell::new(Vec::new()));
    let named: Vec<(String, SignalHandle)> = ctx
        .named
        .iter()
        .map(|(name, series)| {
            (
                name.clone(),
                SignalHandle {
                    series: series.clone(),
                    now_ms: ctx.now_ms,
                },
            )
        })
        .collect();

    let engine = build_engine(named, Rc::clone(&fired));

    let mut scope = Scope::new();
    scope.push(
        "signal",
        SignalHandle {
            series: ctx.primary.clone(),
            now_ms: ctx.now_ms,
        },
    );
    scope.push("now_ms", ctx.now_ms);
    scope.push("state", json_to_state_map(state_in));

    let run = engine.run_with_scope(&mut scope, trigger_code);

    // Read `state` back regardless of outcome; on error we still report the
    // pre-call state so a failed evaluation never corrupts persisted state.
    match run {
        Ok(()) => {
            let state_out = scope
                .get_value::<Map>("state")
                .map(state_map_to_json)
                .unwrap_or(state_fallback);
            let reasons = fired.borrow().clone();
            WatcherOutcome {
                fired: reasons,
                state: state_out,
                error: None,
            }
        }
        Err(err) => WatcherOutcome {
            fired: Vec::new(),
            state: state_fallback,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(points: &[(i64, f64)]) -> SignalSeries {
        SignalSeries::new(points.to_vec())
    }

    fn ctx(primary: SignalSeries, now_ms: i64) -> SignalContext {
        SignalContext {
            primary,
            named: Vec::new(),
            now_ms,
        }
    }

    // The canonical case: a threshold "Wenn" CTOX would compile to a last()-check.
    #[test]
    fn fires_when_threshold_crossed() {
        let code = r#"if signal.last() > 30.0 { fire("Serverraum zu heiß"); }"#;

        let hot = evaluate(code, &ctx(series(&[(1, 22.0), (2, 35.0)]), 100), &json!({}));
        assert!(hot.error.is_none(), "err: {:?}", hot.error);
        assert_eq!(hot.fired, vec!["Serverraum zu heiß".to_string()]);

        let cool = evaluate(code, &ctx(series(&[(1, 22.0), (2, 24.0)]), 100), &json!({}));
        assert!(cool.error.is_none());
        assert!(cool.fired.is_empty());
    }

    // Windowed aggregate: avg over a window drives the decision.
    #[test]
    fn windowed_average_condition() {
        let code = r#"if signal.avg("1h") >= 50.0 { fire(); }"#;
        let now = 10_000_000;
        let s = series(&[(now - 1000, 40.0), (now - 500, 60.0), (now, 50.0)]);
        let out = evaluate(code, &ctx(s, now), &json!({}));
        assert!(out.error.is_none(), "err: {:?}", out.error);
        assert_eq!(out.fired, vec!["trigger".to_string()]);
    }

    // A rising trend (positive rate) is a condition CTOX can express directly.
    #[test]
    fn rate_of_change_condition() {
        let code = r#"if signal.rate("10s") > 0.5 { fire("steigt schnell"); }"#;
        // +10 over 10s = 1.0/s > 0.5.
        let s = series(&[(0, 20.0), (10_000, 30.0)]);
        let out = evaluate(code, &ctx(s, 10_000), &json!({}));
        assert!(out.error.is_none(), "err: {:?}", out.error);
        assert_eq!(out.fired, vec!["steigt schnell".to_string()]);
    }

    // Persistent state across calls: only fire after the condition holds 3x
    // (the kind of "seit längerer Zeit" hysteresis the spec calls for).
    #[test]
    fn state_persists_across_calls() {
        let code = r#"
            if signal.last() > 30.0 {
                state.streak = (state.streak ?? 0) + 1;
            } else {
                state.streak = 0;
            }
            if (state.streak ?? 0) >= 3 { fire("3x in Folge zu heiß"); }
        "#;
        let mut state = json!({});
        let mut last = WatcherOutcome {
            fired: vec![],
            state: json!({}),
            error: None,
        };
        for i in 0..3 {
            last = evaluate(code, &ctx(series(&[(i, 35.0)]), 100), &state);
            assert!(last.error.is_none(), "err: {:?}", last.error);
            state = last.state.clone();
        }
        assert_eq!(state["streak"], 3);
        assert_eq!(last.fired, vec!["3x in Folge zu heiß".to_string()]);

        // A cool reading resets the streak and stops firing.
        let reset = evaluate(code, &ctx(series(&[(9, 10.0)]), 100), &state);
        assert_eq!(reset.state["streak"], 0);
        assert!(reset.fired.is_empty());
    }

    // A second bound signal is reachable via signals("name").
    #[test]
    fn named_signal_lookup() {
        let code = r#"
            if signal.last() > 30.0 && signals("humid").last() > 60.0 {
                fire("heiß UND feucht");
            }
        "#;
        let c = SignalContext {
            primary: series(&[(1, 33.0)]),
            named: vec![("humid".to_string(), series(&[(1, 70.0)]))],
            now_ms: 100,
        };
        let out = evaluate(code, &c, &json!({}));
        assert!(out.error.is_none(), "err: {:?}", out.error);
        assert_eq!(out.fired, vec!["heiß UND feucht".to_string()]);
    }

    // Sandbox: an infinite loop hits the operation limit and returns an error
    // instead of hanging the backend. Self-repair reads this error.
    #[test]
    fn sandbox_blocks_runaway_loop() {
        let code = r#"loop { } "#;
        let out = evaluate(code, &ctx(series(&[(1, 1.0)]), 100), &json!({}));
        assert!(out.fired.is_empty());
        assert!(
            out.error
                .as_deref()
                .map(|e| e.to_lowercase().contains("operation"))
                .unwrap_or(false),
            "expected an operations-limit error, got: {:?}",
            out.error
        );
    }

    // A compile error is captured, not panicked, and leaves state untouched.
    #[test]
    fn compile_error_is_captured() {
        let out = evaluate(
            "this is not valid rhai @@@",
            &ctx(series(&[(1, 1.0)]), 100),
            &json!({ "keep": 1 }),
        );
        assert!(out.fired.is_empty());
        assert!(out.error.is_some());
        assert_eq!(out.state, json!({ "keep": 1 }), "state must be preserved");
    }

    // The watcher cannot reach `eval` (disabled) — a real sandbox boundary.
    #[test]
    fn eval_is_disabled() {
        let out = evaluate(r#"eval("fire(\"x\")")"#, &ctx(series(&[(1, 1.0)]), 100), &json!({}));
        assert!(out.fired.is_empty());
        assert!(out.error.is_some(), "eval must be rejected");
    }

    #[test]
    fn parse_duration_units() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("15m").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86_400));
        assert!(parse_duration("15x").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("").is_err());
    }
}
