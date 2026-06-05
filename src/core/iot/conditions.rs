// Origin: CTOX  /  License: AGPL-3.0-only
//
// Thin attribute-condition evaluator. Ports ONLY attribute-predicate evaluation
// and duration windowing from OpenRemote JsonRulesBuilder + AssetQueryPredicate.
// Contains NO firing loop, NO recurrence timer, NO jeasy-rules, NO trigger cap —
// firing/scheduling/dedup/recurrence/loop-bounding are delegated to CTOX's mission
// brain (schedule.rs / queue.rs / mission_governor.rs / core_transition_guard.rs).
//
// There is exactly ONE automation brain, and it is CTOX's. This module CONSUMES
// attribute events + a loaded ruleset and PRODUCES matches/unmatched sets. It
// does NOT schedule, fire, or write alarms by itself in `evaluate_event`; the
// emitter (`emit_matches`) routes matches into durable, budget-bounded work
// through the existing guard/alarm/schedule surfaces.
//
// State lives ONLY in runtime/ctox.sqlite3 via `crate::paths::core_db(root)`
// (opened through `store::open_iot_store` / `alarms::open`). No `std::env` is read
// for runtime state; there is no HTTP data bridge.
//
// ref: JsonRulesBuilder.java:86-1519  (predicate build + duration windowing)
// ref: AssetQueryPredicate.java:188-302  (asAttributeMatcher / NameValuePredicate)

use crate::iot::model::{AttributeEvent, AttributeValue, MetaMap};
use crate::iot::{now_iso, store, Result};
use crate::service::core_transition_guard as guard;
use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;

// ===========================================================================
// a.2 Ruleset JSON model (the `data` column of `iot_rulesets`, ported when-then)
// ===========================================================================

/// ref: JsonRule.java
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct JsonRule {
    pub name: String,
    /// root condition (AND/OR + items + nested groups).
    #[serde(default)]
    pub when: RuleConditionGroup,
    /// §2A.18 / §2A.23 — gate for the `otherwise` (unmatched) branch.
    #[serde(default)]
    pub track_unmatched: bool,
    /// §2A.17 — recurrence scope + re-fire window; ENFORCED at emit time by
    /// `recurrence_admits` against the durable `iot_recurrence_block` timer.
    #[serde(default)]
    pub recurrence: Recurrence,
    /// CTOX-native emitter config (alarm / queue task / message).
    #[serde(default)]
    pub on_match: OnMatch,
}

/// ref: LogicGroup<AttributePredicate>
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct RuleConditionGroup {
    #[serde(default)]
    pub operator: LogicOperator,
    #[serde(default)]
    pub items: Vec<RuleCondition>,
    /// nested, recursive (§2A.16 step 5).
    #[serde(default)]
    pub groups: Vec<RuleConditionGroup>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum LogicOperator {
    #[default]
    And,
    Or,
}

/// ref: RuleCondition + AssetQuery
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RuleCondition {
    /// asset query: type filter.
    #[serde(default)]
    pub asset_types: Vec<String>,
    /// asset query: realm filter.
    #[serde(default)]
    pub realm: Option<String>,
    /// asset query: explicit ids.
    #[serde(default)]
    pub asset_ids: Vec<String>,
    /// the attribute condition.
    pub attribute: AttributePredicate,
    /// §2A.16 duration windowing ("PT5M"-style; parsed to millis once at load).
    #[serde(default)]
    pub duration: Option<DurationSpec>,
    /// §2A.22 RULE_RESET_IMMEDIATE meta.
    #[serde(default)]
    pub reset_immediate: bool,
}

/// ref: AssetQueryPredicate.java:188-302
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AttributePredicate {
    pub name: String,
    /// §2A.23 value op.
    #[serde(default)]
    pub value: Option<ValuePredicate>,
    /// §2A.23 meta-item match (AND, anyMatch each).
    #[serde(default)]
    pub meta: Vec<MetaPredicate>,
    /// §2A.23 previousValue (vs event.old_value).
    #[serde(default)]
    pub previous_value: Option<ValuePredicate>,
    /// §2A.16 timestampOlderThan.
    #[serde(default)]
    pub timestamp_older_than: Option<DurationSpec>,
}

/// ref: ValuePredicate variants
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum ValuePredicate {
    Eq { value: serde_json::Value },
    Neq { value: serde_json::Value },
    GreaterThan { value: f64 },
    GreaterEqual { value: f64 },
    LessThan { value: f64 },
    LessEqual { value: f64 },
    Between { min: f64, max: f64 },
    Contains { value: String },
    Regex { pattern: String },
    IsNull,
    NotNull,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MetaPredicate {
    pub name: String,
    #[serde(default)]
    pub value: Option<ValuePredicate>,
}

/// §2A.17
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum RecurrenceScope {
    PerAsset,
    Global,
}

/// §2A.17 — recurrence scope + re-fire window. ENFORCED at emit time by
/// `recurrence_admits` (CTOX-native durable timer in `iot_recurrence_block`); no
/// cron timer and no firing loop are involved.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Recurrence {
    /// null => PER_ASSET default.
    #[serde(default)]
    pub scope: Option<RecurrenceScope>,
    /// null=never recur (default previously_matched cycle), 0=always,
    /// >0=block re-fire for N minutes per scope.
    #[serde(default)]
    pub mins: Option<i64>,
}

/// Parsed-to-millis ONCE at parse time (§2A.16 edge: parse at build, not runtime).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct DurationSpec(pub i64);

/// CTOX-native emitter config (NOT upstream).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct OnMatch {
    #[serde(default = "default_true")]
    pub raise_alarm: bool,
    /// LOW|MEDIUM|HIGH.
    #[serde(default)]
    pub alarm_severity: Option<String>,
    /// flagged => durable queue task (§6 exit).
    #[serde(default)]
    pub enqueue_task: bool,
    #[serde(default)]
    pub task_prompt: Option<String>,
    /// default "iot-operations".
    #[serde(default)]
    pub suggested_skill: Option<String>,
    /// iot-event-message contract family.
    #[serde(default)]
    pub emit_message: bool,
}

impl Default for OnMatch {
    fn default() -> Self {
        OnMatch {
            raise_alarm: true,
            alarm_severity: None,
            enqueue_task: false,
            task_prompt: None,
            suggested_skill: None,
            emit_message: false,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Parse a ruleset `data` JSON value into a validated `JsonRule`. Duration specs
/// must already be in millis (the wire form is an integer-millis transparent
/// newtype); parse failure here means malformed rules are rejected at SAVE time,
/// not at fire time.
/// ref: JsonRulesBuilder.java:1442-1445 (TimeUtil.parseTimeDuration once at build)
pub(crate) fn parse_rule(data: &serde_json::Value) -> Result<JsonRule> {
    serde_json::from_value::<JsonRule>(data.clone())
        .context("malformed IoT ruleset: failed to parse JsonRule")
}

// ===========================================================================
// a.3 Per-condition evaluation state (durable, keyed per (asset, predicateIndex))
//
// This is condition-evaluation MEMORY, not a timer and not a firing loop. It is
// persisted on the core db so duration windows survive a daemon restart.
// ===========================================================================

/// Ensure the ruleset table exists before evaluation. The canonical owner of
/// this schema is `commands::ensure_stub_schema`; this is the same idempotent
/// `CREATE TABLE IF NOT EXISTS` so the evaluator is safe on a store that has not
/// yet had a ruleset saved (a live attribute event must not error just because
/// no rules exist yet — it simply finds no enabled rulesets and emits nothing).
fn ensure_ruleset_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_rulesets (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            name        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            data        TEXT NOT NULL,
            last_fired_ms INTEGER,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_rulesets_realm ON iot_rulesets(realm);",
    )
    .context("failed to ensure iot_rulesets schema")?;
    Ok(())
}

/// ref: JsonRulesBuilder.java:1432-1519 (durationMatchTimes / previouslyMatched)
fn ensure_condition_state_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_condition_state (
            ruleset_id          TEXT NOT NULL,
            condition_index     INTEGER NOT NULL,
            asset_id            TEXT NOT NULL,
            first_match_ms      INTEGER,
            previously_matched  INTEGER NOT NULL DEFAULT 0,
            previously_unmatched INTEGER NOT NULL DEFAULT 0,
            match_value_ms      INTEGER,
            updated_at          TEXT NOT NULL,
            PRIMARY KEY (ruleset_id, condition_index, asset_id)
        );",
    )
    .context("failed to create iot_condition_state schema")?;
    Ok(())
}

/// §2A.17 recurrence-block memory: the durable "last fired at" per recurrence
/// scope, used to suppress re-fire inside the N-minute block window. This is the
/// CTOX-native enforcement of the `mins>0` timer (PER_ASSET vs GLOBAL); like the
/// condition state it is evaluation memory on the core db, NOT a firing loop or a
/// cron timer. ref: JsonRulesBuilder.java:333-335 (recurrence-coupled removal)
fn ensure_recurrence_block_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_recurrence_block (
            scope_key       TEXT PRIMARY KEY,
            last_fired_ms   INTEGER NOT NULL,
            updated_at      TEXT NOT NULL
        );",
    )
    .context("failed to create iot_recurrence_block schema")?;
    Ok(())
}

/// The durable recurrence-block key for one match under the rule's scope.
/// PER_ASSET (the default when scope is null) blocks per (ruleset, asset);
/// GLOBAL blocks per ruleset across all assets. ref: §2A.17 (scope timer)
fn recurrence_scope_key(
    ruleset_id: &str,
    asset_id: &str,
    scope: Option<RecurrenceScope>,
) -> String {
    match scope.unwrap_or(RecurrenceScope::PerAsset) {
        RecurrenceScope::PerAsset => format!("{ruleset_id}:{asset_id}"),
        RecurrenceScope::Global => ruleset_id.to_string(),
    }
}

/// §2A.17 recurrence gate, enforced at emit time (CTOX-native, no cron timer):
///   * `mins = None`  → "never recurs": NO recurrence-specific time block is
///     imposed; re-fire is governed entirely by the upstream default — the
///     evaluator's `previously_matched` re-arm cycle (and the spawn budget). This
///     is the default for every rule that does not configure recurrence, so it
///     must not add a second suppression on top of `previously_matched`.
///   * `mins = Some(0)` → "always": no time block, re-fire admitted every match.
///   * `mins = Some(n>0)` → block re-fire for `n` minutes per scope (PER_ASSET or
///     GLOBAL), enforced via the durable `iot_recurrence_block` timer.
/// Returns `true` if the match may fire (and stamps the block for `mins>0`),
/// `false` if it is currently suppressed by the recurrence window.
fn recurrence_admits(
    conn: &Connection,
    ruleset_id: &str,
    asset_id: &str,
    recurrence: &Recurrence,
    now_ms: i64,
) -> Result<bool> {
    // Only mins>0 imposes a durable time window. None ("never recurs", default)
    // and 0 ("always") defer entirely to previously_matched + budget.
    let window = match recurrence.mins {
        Some(n) if n > 0 => n * 60_000,
        _ => return Ok(true),
    };
    ensure_recurrence_block_schema(conn)?;
    let scope_key = recurrence_scope_key(ruleset_id, asset_id, recurrence.scope);
    let last: Option<i64> = conn
        .query_row(
            "SELECT last_fired_ms FROM iot_recurrence_block WHERE scope_key = ?1",
            params![scope_key],
            |r| r.get(0),
        )
        .optional()
        .context("failed to load recurrence block")?;
    if let Some(prev) = last {
        // still inside the N-minute window → suppress.
        if now_ms < prev + window {
            return Ok(false);
        }
    }
    conn.execute(
        "INSERT INTO iot_recurrence_block (scope_key, last_fired_ms, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(scope_key) DO UPDATE SET
            last_fired_ms = excluded.last_fired_ms,
            updated_at = excluded.updated_at",
        params![scope_key, now_ms, now_iso()],
    )
    .context("failed to stamp recurrence block")?;
    Ok(true)
}

#[derive(Clone, Copy, Debug, Default)]
struct CondState {
    first_match_ms: Option<i64>,
    previously_matched: bool,
    previously_unmatched: bool,
    match_value_ms: Option<i64>,
}

fn load_state(
    conn: &Connection,
    ruleset_id: &str,
    condition_index: i64,
    asset_id: &str,
) -> Result<CondState> {
    let row = conn
        .query_row(
            "SELECT first_match_ms, previously_matched, previously_unmatched, match_value_ms
             FROM iot_condition_state
             WHERE ruleset_id = ?1 AND condition_index = ?2 AND asset_id = ?3",
            params![ruleset_id, condition_index, asset_id],
            |r| {
                Ok(CondState {
                    first_match_ms: r.get::<_, Option<i64>>(0)?,
                    previously_matched: r.get::<_, i64>(1)? != 0,
                    previously_unmatched: r.get::<_, i64>(2)? != 0,
                    match_value_ms: r.get::<_, Option<i64>>(3)?,
                })
            },
        )
        .optional()
        .context("failed to load condition state")?;
    Ok(row.unwrap_or_default())
}

fn save_state(
    conn: &Connection,
    ruleset_id: &str,
    condition_index: i64,
    asset_id: &str,
    state: &CondState,
) -> Result<()> {
    conn.execute(
        "INSERT INTO iot_condition_state
            (ruleset_id, condition_index, asset_id, first_match_ms,
             previously_matched, previously_unmatched, match_value_ms, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(ruleset_id, condition_index, asset_id) DO UPDATE SET
            first_match_ms = excluded.first_match_ms,
            previously_matched = excluded.previously_matched,
            previously_unmatched = excluded.previously_unmatched,
            match_value_ms = excluded.match_value_ms,
            updated_at = excluded.updated_at",
        params![
            ruleset_id,
            condition_index,
            asset_id,
            state.first_match_ms,
            if state.previously_matched { 1 } else { 0 },
            if state.previously_unmatched { 1 } else { 0 },
            state.match_value_ms,
            now_iso(),
        ],
    )
    .context("failed to save condition state")?;
    Ok(())
}

// ===========================================================================
// a.4 Public results
// ===========================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MatchKind {
    /// §2A.18 — the rule's `when` matched.
    Matched,
    /// §2A.18 — `track_unmatched` + asset matches the query but fails the predicate.
    OtherwiseUnmatched,
}

#[derive(Clone, Debug)]
pub(crate) struct ConditionMatch {
    pub ruleset_id: String,
    pub ruleset_name: String,
    pub rule_name: String,
    /// de-duped, post order/limit (§2A.22).
    pub asset_id: String,
    pub matched_attribute: String,
    pub on_match: OnMatch,
    /// §2A.17 recurrence scope/window; enforced at emit time by `recurrence_admits`.
    pub recurrence: Recurrence,
    pub kind: MatchKind,
}

// ===========================================================================
// a.4 Value-predicate evaluation (§2A.23)
// ===========================================================================

/// Evaluate one value predicate against one attribute value. Numeric ops use
/// `AttributeValue::as_numeric()` (bool→1/0). Regex is compiled on demand.
/// ref: ValuePredicate.asPredicate (value eq/range/contains/regex/null)
fn eval_value_predicate(p: &ValuePredicate, v: &AttributeValue) -> bool {
    match p {
        ValuePredicate::IsNull => v.is_null(),
        ValuePredicate::NotNull => !v.is_null(),
        ValuePredicate::Eq { value } => &v.0 == value,
        ValuePredicate::Neq { value } => &v.0 != value,
        ValuePredicate::GreaterThan { value } => {
            v.as_numeric().map(|n| n > *value).unwrap_or(false)
        }
        ValuePredicate::GreaterEqual { value } => {
            v.as_numeric().map(|n| n >= *value).unwrap_or(false)
        }
        ValuePredicate::LessThan { value } => v.as_numeric().map(|n| n < *value).unwrap_or(false),
        ValuePredicate::LessEqual { value } => v.as_numeric().map(|n| n <= *value).unwrap_or(false),
        ValuePredicate::Between { min, max } => v
            .as_numeric()
            .map(|n| n >= *min && n <= *max)
            .unwrap_or(false),
        ValuePredicate::Contains { value } => match &v.0 {
            serde_json::Value::String(s) => s.contains(value.as_str()),
            _ => false,
        },
        ValuePredicate::Regex { pattern } => match &v.0 {
            serde_json::Value::String(s) => regex::Regex::new(pattern)
                .map(|re| re.is_match(s))
                .unwrap_or(false),
            _ => false,
        },
    }
}

/// Evaluate one `AttributePredicate` against a live attribute value/timestamp/meta
/// plus the event's previous value. Mirrors the wrapped predicate chain.
/// ref: AssetQueryPredicate.java:189-302 (asAttributeMatcher inner predicate)
#[allow(clippy::too_many_arguments)]
fn as_attribute_matcher(
    pred: &AttributePredicate,
    attr_name: &str,
    attr_value: Option<&AttributeValue>,
    attr_timestamp: i64,
    attr_meta: &MetaMap,
    old_value: Option<&AttributeValue>,
    now_ms: i64,
) -> bool {
    // name match
    if pred.name != attr_name {
        return false;
    }

    // timestampOlderThan: returns false unless attr.timestamp <= now - dur
    // (edge: upstream tests `>` and inverts). ref: AssetQueryPredicate.java:205-211
    if let Some(DurationSpec(dur)) = pred.timestamp_older_than {
        if attr_timestamp > now_ms - dur {
            return false;
        }
    }

    // value op (§2A.23)
    if let Some(p) = &pred.value {
        let null_value = AttributeValue(serde_json::Value::Null);
        let v = attr_value.unwrap_or(&null_value);
        if !eval_value_predicate(p, v) {
            return false;
        }
    }

    // meta: AND of meta predicates, each anyMatch over the attribute's MetaMap.
    // ref: AssetQueryPredicate.java:221-233
    for mp in &pred.meta {
        let any = attr_meta.iter().any(|(k, raw)| {
            if k != &mp.name {
                return false;
            }
            match &mp.value {
                None => true,
                Some(vp) => eval_value_predicate(vp, &AttributeValue(raw.clone())),
            }
        });
        if !any {
            return false;
        }
    }

    // previousValue: tested against event.old_value, NOT the new value.
    // ref: AssetQueryPredicate.java:235-239
    if let Some(p) = &pred.previous_value {
        let null_value = AttributeValue(serde_json::Value::Null);
        let ov = old_value.unwrap_or(&null_value);
        if !eval_value_predicate(p, ov) {
            return false;
        }
    }

    true
}

// ===========================================================================
// Asset-query filter (type / realm / explicit-id)
// ===========================================================================

/// ref: AssetQueryPredicate asset filter (type/realm/ids)
fn asset_query_matches(
    rc: &RuleCondition,
    asset_type: &str,
    asset_realm: &str,
    asset_id: &str,
) -> bool {
    if !rc.asset_types.is_empty() && !rc.asset_types.iter().any(|t| t == asset_type) {
        return false;
    }
    if let Some(realm) = &rc.realm {
        if realm != asset_realm {
            return false;
        }
    }
    if !rc.asset_ids.is_empty() && !rc.asset_ids.iter().any(|a| a == asset_id) {
        return false;
    }
    true
}

// ===========================================================================
// a.4 Single-condition evaluation with duration windowing (§2A.16)
// ===========================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CondResult {
    /// asset matches query AND predicate holds for the full duration window.
    Matched,
    /// asset matches query but predicate currently fails (unmatched candidate).
    Unmatched,
    /// duration window is open but not yet elapsed (predicate true, timer running).
    Pending,
    /// asset does not match the asset query at all.
    NotApplicable,
}

/// Evaluate one condition against the event's asset, applying duration
/// windowing keyed (ruleset_id, condition_index, asset_id). The predicate is
/// tested against the asset's CURRENT attribute collection — upstream runs each
/// predicate over the asset's whole AttributeInfo set, so a multi-condition AND
/// referencing several attributes resolves each from live state (not just the
/// one attribute the event changed). `previous_value` only applies to the
/// attribute the event actually changed (the only one carrying old_value).
/// ref: JsonRulesBuilder.java:1432-1519 + AssetQueryPredicate.java:189-302
#[allow(clippy::too_many_arguments)]
fn evaluate_condition(
    conn: &Connection,
    rc: &RuleCondition,
    condition_index: i64,
    ruleset_id: &str,
    asset: &crate::iot::model::Asset,
    event: &AttributeEvent,
    now_ms: i64,
) -> Result<CondResult> {
    if !asset_query_matches(rc, &asset.asset_type, &asset.realm, &event.asset_id) {
        return Ok(CondResult::NotApplicable);
    }

    // Resolve the predicate's target attribute from the asset's live state.
    let target = asset.attributes.get(&rc.attribute.name);
    let attr_value = target.and_then(|a| a.value.as_ref());
    let attr_timestamp = target.map(|a| a.timestamp).unwrap_or(0);
    let empty_meta = MetaMap::new();
    let attr_meta = target.map(|a| &a.meta).unwrap_or(&empty_meta);
    // old_value is only meaningful for the attribute the event changed.
    let old_value = if rc.attribute.name == event.attribute_name {
        event.old_value.as_ref()
    } else {
        None
    };

    let predicate_true = as_attribute_matcher(
        &rc.attribute,
        &rc.attribute.name,
        attr_value,
        attr_timestamp,
        attr_meta,
        old_value,
        now_ms,
    );

    // duration == None → match immediately. ref: JsonRulesBuilder.java:1478-1483
    let Some(DurationSpec(dur)) = rc.duration else {
        return Ok(if predicate_true {
            CondResult::Matched
        } else {
            CondResult::Unmatched
        });
    };

    let mut state = load_state(conn, ruleset_id, condition_index, &event.asset_id)?;
    if predicate_true {
        match state.first_match_ms {
            // duration_matches = first_match_ms + dur <= now. ref: :1497
            Some(first) => {
                let duration_matches = first + dur <= now_ms;
                Ok(if duration_matches {
                    CondResult::Matched
                } else {
                    CondResult::Pending
                })
            }
            // start the window this eval. ref: :1500
            None => {
                state.first_match_ms = Some(now_ms);
                save_state(conn, ruleset_id, condition_index, &event.asset_id, &state)?;
                Ok(CondResult::Pending)
            }
        }
    } else {
        // predicate false → DELETE first_match_ms (reset; timer restarts on next
        // true; flip false→true resets to now, not resumed). ref: :1509
        if state.first_match_ms.is_some() {
            state.first_match_ms = None;
            save_state(conn, ruleset_id, condition_index, &event.asset_id, &state)?;
        }
        Ok(CondResult::Unmatched)
    }
}

// ===========================================================================
// a.4 De-dup THEN order/limit (§2A.22)
// ===========================================================================

/// De-dupe matched asset ids FIRST (preserving first-seen order), THEN apply an
/// optional limit. Single-event eval yields at most one asset, but the
/// dedupe-before-order discipline must exist and be unit-testable on a synthetic
/// multi-asset collection. ref: JsonRulesBuilder.java:713-724
fn dedup_then_limit(asset_ids: &[String], limit: Option<usize>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for id in asset_ids {
        if seen.insert(id.clone()) {
            out.push(id.clone());
        }
    }
    if let Some(n) = limit {
        out.truncate(n);
    }
    out
}

// ===========================================================================
// a.4 The public evaluate entry — one event, all enabled rulesets in a realm
// ===========================================================================

/// Evaluate every ENABLED ruleset in `realm` against ONE just-applied attribute
/// event and return the matches that newly fire. Pure decision: it raises NO
/// alarm and emits NO work — the caller (`emit_matches`) does that.
///
/// Delegation note: upstream lines 333-335 (recurrence-coupled removal) and
/// §2A.17/20 are NOT decided here. This returns a match every time a condition
/// NEWLY holds (subject to the §2A.15 previously_matched re-arm cycle, which IS
/// evaluated here); whether that match is then suppressed (still inside its
/// recurrence window, budget-exhausted) is decided by `recurrence_admits` +
/// queue.rs dedup + the spawn budget at emit time. One brain.
///
/// ref: JsonRulesBuilder.java:268-366 (evaluateRuleConditions) + 1432-1519 (duration)
pub(crate) fn evaluate_event(
    conn: &Connection,
    realm: &str,
    event: &AttributeEvent,
    now_ms: i64,
) -> Result<Vec<ConditionMatch>> {
    ensure_condition_state_schema(conn)?;
    ensure_ruleset_schema(conn)?;

    // Resolve the live asset state for this event's asset.
    let asset = match store::get_asset(conn, &event.asset_id)? {
        Some(a) => a,
        None => return Ok(Vec::new()),
    };
    // Load enabled rulesets for the realm.
    let mut stmt = conn
        .prepare("SELECT id, name, data FROM iot_rulesets WHERE realm = ?1 AND enabled = 1")
        .context("failed to prepare ruleset load")?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![realm], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .context("failed to query rulesets")?
        .collect::<rusqlite::Result<_>>()?;

    let mut out: Vec<ConditionMatch> = Vec::new();
    for (ruleset_id, ruleset_name, data) in rows {
        let value: serde_json::Value =
            serde_json::from_str(&data).context("failed to parse ruleset data")?;
        let rule = match parse_rule(&value) {
            Ok(r) => r,
            // A malformed persisted rule is skipped (it should have been rejected
            // at save time). It does not poison evaluation of sibling rulesets.
            Err(_) => continue,
        };

        // Empty condition group → EMPTY_SET (NOT error). ref: AssetQueryPredicate.java:190-191
        if rule.when.items.is_empty() && rule.when.groups.is_empty() {
            continue;
        }

        // Evaluate each root-level item as an indexed condition for this asset.
        let mut per_condition: Vec<(i64, CondResult)> = Vec::new();
        for (idx, rc) in rule.when.items.iter().enumerate() {
            let res = evaluate_condition(conn, rc, idx as i64, &ruleset_id, &asset, event, now_ms)?;
            per_condition.push((idx as i64, res));
        }

        let applicable: Vec<&(i64, CondResult)> = per_condition
            .iter()
            .filter(|(_, r)| *r != CondResult::NotApplicable)
            .collect();
        if applicable.is_empty() {
            continue;
        }

        let matched = match rule.when.operator {
            // AND: every applicable condition must be Matched. ref: :305-322 (allMatch)
            LogicOperator::And => applicable.iter().all(|(_, r)| *r == CondResult::Matched),
            // OR: any applicable condition Matched. ref: :324-345 (anyMatch)
            LogicOperator::Or => applicable.iter().any(|(_, r)| *r == CondResult::Matched),
        };

        // §2A.15 per-condition re-arm (operator-agnostic): upstream clears
        // `previouslyMatched` for ANY condition whose asset stops matching that
        // condition, INDEPENDENT of the group AND/OR operator — the operator only
        // governs how matched SETS are intersected (AND) vs unioned (OR), not
        // whether re-arm occurs. So in BOTH modes, any applicable condition that
        // is now Unmatched re-arms (clears its own previously_matched), allowing a
        // clean matched→unmatched→matched cycle to re-fire. Without this, an
        // OR-mode rule (or a non-stale-reset path) is permanently suppressed after
        // its first match. ref: JsonRulesBuilder.java:268-293 (previouslyMatchedAssetStates)
        for (idx, r) in &applicable {
            if *r == CondResult::Unmatched {
                let mut st = load_state(conn, &ruleset_id, *idx, &event.asset_id)?;
                if st.previously_matched {
                    st.previously_matched = false;
                    save_state(conn, &ruleset_id, *idx, &event.asset_id, &st)?;
                }
            }
        }

        // §2A.19 stale-AND reset: in AND mode, if ANY condition currently has no
        // match for the asset, clear previously_matched for ALL of this rule's
        // conditions for the asset so it can re-match. (The per-condition re-arm
        // above only clears the condition(s) that went Unmatched; a multi-condition
        // AND must clear the SIBLING conditions too, so the whole group can
        // re-fire on the next all-true pass.) ref: JsonRulesBuilder.java:268-293
        if rule.when.operator == LogicOperator::And
            && applicable.iter().any(|(_, r)| *r != CondResult::Matched)
        {
            for (idx, _) in &per_condition {
                let mut st = load_state(conn, &ruleset_id, *idx, &event.asset_id)?;
                if st.previously_matched {
                    st.previously_matched = false;
                    save_state(conn, &ruleset_id, *idx, &event.asset_id, &st)?;
                }
            }
        }

        if matched {
            // §2A.22 RULE_RESET_IMMEDIATE: a strictly-newer attr.timestamp than the
            // stored match_value_ms clears previously_matched (forces re-trigger).
            // ref: JsonRulesBuilder.java:320-331
            let primary_idx = applicable
                .iter()
                .find(|(_, r)| *r == CondResult::Matched)
                .map(|(i, _)| *i)
                .unwrap_or(0);
            // Resolve the primary matched condition's target attribute + its live
            // timestamp (the value the RULE_RESET_IMMEDIATE compare uses).
            let primary_attr = rule
                .when
                .items
                .get(primary_idx as usize)
                .map(|c| c.attribute.name.clone())
                .unwrap_or_else(|| event.attribute_name.clone());
            let attr_timestamp = asset
                .attributes
                .get(&primary_attr)
                .map(|a| a.timestamp)
                .unwrap_or(event.timestamp);
            let mut st = load_state(conn, &ruleset_id, primary_idx, &event.asset_id)?;

            if rule
                .when
                .items
                .get(primary_idx as usize)
                .map(|c| c.reset_immediate)
                == Some(true)
            {
                if let Some(stored) = st.match_value_ms {
                    if attr_timestamp > stored {
                        st.previously_matched = false;
                    }
                }
            }

            // §2A.19 previously-matched filtering: drop if already matched, unless
            // a reset cleared it above. ref: JsonRulesBuilder.java:333-338
            if !st.previously_matched {
                st.previously_matched = true;
                st.match_value_ms = Some(attr_timestamp); // ref: :730
                save_state(conn, &ruleset_id, primary_idx, &event.asset_id, &st)?;

                out.push(ConditionMatch {
                    ruleset_id: ruleset_id.clone(),
                    ruleset_name: ruleset_name.clone(),
                    rule_name: rule.name.clone(),
                    asset_id: event.asset_id.clone(),
                    matched_attribute: primary_attr,
                    on_match: rule.on_match.clone(),
                    recurrence: rule.recurrence.clone(),
                    kind: MatchKind::Matched,
                });
            }
        } else if rule.track_unmatched && rule.when.operator == LogicOperator::And {
            // §2A.18 trackUnmatched / otherwise: only when track_unmatched AND
            // AND-mode (edge: OR mode does NOT collect unmatched). Unmatched =
            // asset matches asset-query but fails the attribute predicate; only
            // NEWLY-unmatched survives the previously_unmatched filter.
            // ref: JsonRulesBuilder.java:299-306, 516-523
            let unmatched_idx = applicable
                .iter()
                .find(|(_, r)| *r == CondResult::Unmatched)
                .map(|(i, _)| *i);
            if let Some(idx) = unmatched_idx {
                let mut st = load_state(conn, &ruleset_id, idx, &event.asset_id)?;
                if !st.previously_unmatched {
                    st.previously_unmatched = true;
                    // an unmatched asset is no longer matched
                    st.previously_matched = false;
                    save_state(conn, &ruleset_id, idx, &event.asset_id, &st)?;
                    out.push(ConditionMatch {
                        ruleset_id: ruleset_id.clone(),
                        ruleset_name: ruleset_name.clone(),
                        rule_name: rule.name.clone(),
                        asset_id: event.asset_id.clone(),
                        matched_attribute: event.attribute_name.clone(),
                        on_match: rule.on_match.clone(),
                        recurrence: rule.recurrence.clone(),
                        kind: MatchKind::OtherwiseUnmatched,
                    });
                }
            }
        }

        // When a rule re-matches, clear the previously_unmatched flag so the
        // otherwise branch can fire again on the next clear.
        if matched {
            for (idx, _) in &per_condition {
                let mut st = load_state(conn, &ruleset_id, *idx, &event.asset_id)?;
                if st.previously_unmatched {
                    st.previously_unmatched = false;
                    save_state(conn, &ruleset_id, *idx, &event.asset_id, &st)?;
                }
            }
        }
    }

    Ok(out)
}

// ===========================================================================
// (c) The emitter — match → alarm + budget-bounded durable spawn
// ===========================================================================

#[derive(Clone, Debug, Default)]
pub(crate) struct EmitReport {
    /// alarm ids raised (the spawn parents).
    pub alarm_ids: Vec<String>,
    /// accepted spawn edge ids (queue tasks / messages).
    pub spawn_edge_ids: Vec<String>,
    /// matches suppressed because their spawn budget was exhausted (§2A.20).
    pub budget_exhausted: Vec<String>,
    /// ruleset ids suppressed by the §2A.17 recurrence block window (PER_ASSET /
    /// GLOBAL N-minute re-fire block, or `mins=None` not-yet-re-armed).
    pub recurrence_blocked: Vec<String>,
    /// durable queue-task message keys created (idempotent per dedup key,
    /// §2A.15). The notification path renders these like other CTOX activity.
    pub queue_task_keys: Vec<String>,
}

fn severity_from(label: Option<&str>) -> crate::iot::alarms::Severity {
    use crate::iot::alarms::Severity;
    match label.map(|s| s.to_ascii_uppercase()).as_deref() {
        Some("LOW") => Severity::Low,
        Some("HIGH") => Severity::High,
        _ => Severity::Medium,
    }
}

/// Turn each `ConditionMatch` into durable work through CTOX's brain. Raises an
/// alarm (Source::*Ruleset) — projected to the `iot_alarms` collection by the
/// caller's projection path — then, for flagged matches, records a
/// budget-bounded spawn edge via the registered `iot-event-*` contract family.
///
/// The N-minute recurrence block (§2A.17) is enforced here by `recurrence_admits`
/// against the durable `iot_recurrence_block` timer BEFORE the alarm/spawn: a
/// match still inside its scope's window is suppressed. This is a durable
/// last-fired comparison, NOT a cron timer or a firing loop.
///
/// `engine_warm` gates §2A.21 startup suppression: a cold first pass over
/// pre-existing state emits nothing.
pub(crate) fn emit_matches(
    root: &Path,
    realm: &str,
    matches: &[ConditionMatch],
    engine_warm: bool,
    now_ms: i64,
) -> Result<EmitReport> {
    let mut report = EmitReport::default();
    if !engine_warm || matches.is_empty() {
        // §2A.21 startup suppression — pre-existing matches at cold start raise
        // no alarm/work.
        return Ok(report);
    }

    let conn = store::open_iot_store(root)?;
    crate::iot::alarms::open(root)?; // ensure iot_alarms schema exists on the core db.
    guard::ensure_core_transition_guard_schema(&conn)?;

    for m in matches {
        if m.kind != MatchKind::Matched {
            // OtherwiseUnmatched may raise a distinct cleared alarm if configured;
            // for Phase 4 we only emit durable work for positive matches.
            continue;
        }
        if !m.on_match.raise_alarm {
            continue;
        }

        // §2A.17 recurrence block (CTOX-native enforcement, no cron timer): a
        // match that is still inside its scope's N-minute re-fire window — or a
        // `mins=None` scope that already fired and has not re-armed — is suppressed
        // here BEFORE any alarm/work is raised. PER_ASSET vs GLOBAL scope and the
        // null/0/>0 semantics are decided by `recurrence_admits`.
        if !recurrence_admits(&conn, &m.ruleset_id, &m.asset_id, &m.recurrence, now_ms)? {
            report.recurrence_blocked.push(m.ruleset_id.clone());
            continue;
        }

        // 1. Raise alarm (parent entity). ref: AlarmService.sendAlarm
        let source = if realm.trim().is_empty() {
            crate::iot::alarms::Source::GlobalRuleset
        } else {
            crate::iot::alarms::Source::RealmRuleset
        };
        let alarm = crate::iot::alarms::create(
            &conn,
            crate::iot::alarms::NewAlarm {
                realm: realm.to_string(),
                title: format!("IoT: {} on {}", m.rule_name, m.asset_id),
                content: Some(format!(
                    "ruleset={} attribute={} matched",
                    m.ruleset_name, m.matched_attribute
                )),
                severity: severity_from(m.on_match.alarm_severity.as_deref()),
                assignee_id: None,
                source,
                source_id: m.ruleset_id.clone(),
            },
            vec![m.asset_id.clone()],
        )?;
        // Link the alarm to the matched asset (set semantics).
        crate::iot::alarms::link_assets(&conn, &alarm.id, &[m.asset_id.clone()])?;
        // Stamp last_fired_ms on the ruleset.
        conn.execute(
            "UPDATE iot_rulesets SET last_fired_ms = ?2 WHERE id = ?1",
            params![m.ruleset_id, now_ms],
        )
        .context("failed to stamp ruleset last_fired_ms")?;
        report.alarm_ids.push(alarm.id.clone());

        // 3. Queue task (flagged) — durable work through CTOX's mission brain.
        //
        // The firing/dedup/loop-bounding is DELEGATED to channels.rs +
        // queue.rs + the spawn budget (one automation brain — no firing loop
        // lives here). `ingest_iot_event_message` performs the budget-bounded
        // `iot-event-queue-task` spawn AND upserts ONE durable
        // `communication_messages` row keyed by the dedup key, so:
        //   * exactly one durable queue task exists per dedup key (§2A.15), and
        //   * the number of re-fires is bounded by the spawn budget (§2A.20).
        // The durable inbound message is also the bridge into the notification
        // path: it surfaces IoT activity like every other CTOX inbound item.
        if m.on_match.enqueue_task {
            let dedup_key = format!("{}:{}", m.ruleset_id, m.asset_id);
            let budget_key = format!("iot-event:{}:{}", m.ruleset_id, m.asset_id);
            let body = format!(
                "IoT condition matched: ruleset `{}` rule `{}` on asset `{}` (attribute `{}`). Diagnose and act per the iot-operations skill.",
                m.ruleset_name, m.rule_name, m.asset_id, m.matched_attribute
            );
            let skill = m.on_match.suggested_skill.as_deref();
            let observed_at = now_iso();
            let outcome = crate::mission::channels::ingest_iot_event_message(
                root,
                &alarm.id,
                &m.ruleset_id,
                &m.asset_id,
                &dedup_key,
                &budget_key,
                64,
                &m.rule_name,
                &body,
                skill,
                &observed_at,
            )?;
            match outcome.message_key {
                Some(key) => {
                    report.spawn_edge_ids.push(outcome.spawn.edge_id);
                    report.queue_task_keys.push(key);
                }
                None => {
                    if outcome
                        .spawn
                        .violation_codes
                        .iter()
                        .any(|c| c == "spawn_budget_exhausted")
                    {
                        // §2A.20 — bounded re-firing: budget exhausted, suppressed.
                        report.budget_exhausted.push(m.ruleset_id.clone());
                    } else {
                        bail!("iot queue-task spawn rejected: {}", outcome.spawn.message);
                    }
                }
            }
        }

        // Message (flagged) — budget-bounded spawn through the guard.
        if m.on_match.emit_message {
            let budget_key = format!("iot-event-msg:{}:{}", m.ruleset_id, m.asset_id);
            let child_id = format!("iot-msg:{}:{}", alarm.id, now_ms);
            let mut metadata = BTreeMap::new();
            metadata.insert("iot_alarm_id".to_string(), alarm.id.clone());
            let proof = guard::evaluate_core_spawn(
                &conn,
                &guard::CoreSpawnRequest {
                    parent_entity_type: "IotAlarm".to_string(),
                    parent_entity_id: alarm.id.clone(),
                    child_entity_type: "Message".to_string(),
                    child_entity_id: child_id,
                    spawn_kind: "iot-event-message".to_string(),
                    spawn_reason: "iot_condition_match".to_string(),
                    actor: "iot-conditions".to_string(),
                    checkpoint_key: None,
                    budget_key: Some(budget_key),
                    max_attempts: Some(8),
                    metadata,
                },
            )?;
            if proof.accepted {
                report.spawn_edge_ids.push(proof.edge_id);
            } else if !proof
                .violation_codes
                .iter()
                .any(|c| c == "spawn_budget_exhausted")
            {
                bail!("iot message spawn rejected: {}", proof.message);
            }
        }
    }

    Ok(report)
}

/// Evaluate one just-applied attribute event against the realm's enabled
/// rulesets and route the matches into durable, budget-bounded work. This is
/// the single production entry point the IoT write path (`commands::attribute_write`
/// and `runtime::run_agent_step`) calls AFTER `store::process_attribute_event`.
///
/// `engine_warm` gates §2A.21 startup suppression: the engine's FIRST pass over
/// pre-existing device state is cold and emits nothing; only attribute changes
/// observed after the mission-start guard has warmed produce alarms/work.
///
/// One brain: this does not run a firing loop. It evaluates predicates and hands
/// matches to `emit_matches`, which raises an alarm and emits one durable queue
/// task per dedup key through `channels::ingest_iot_event_message`.
pub(crate) fn evaluate_and_emit(
    root: &Path,
    realm: &str,
    event: &AttributeEvent,
    engine_warm: bool,
    now_ms: i64,
) -> Result<EmitReport> {
    let matches = {
        let conn = store::open_iot_store(root)?;
        evaluate_event(&conn, realm, event, now_ms)?
    };
    emit_matches(root, realm, &matches, engine_warm, now_ms)
}

// ===========================================================================
// (e) Tests — §2A.16,18,19,22,23 + delegated behavior + spawn-liveness proof.
// Deterministic injected clock (no wall-clock for duration windows).
//
// Release gate (manual/CI, NOT a unit test):
//   `cargo run -- process-mining spawn-liveness` must exit 0 with the two new
//   `iot-event-*` contracts present.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::model::{Asset, AssetTypeInfo};

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn open(root: &Path) -> Connection {
        let conn = store::open_iot_store(root).unwrap();
        crate::iot::alarms::open(root).unwrap();
        // ensure ruleset table exists (created by commands::ensure_stub_schema;
        // recreate the minimal shape here for an isolated evaluator test).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS iot_rulesets (
                id TEXT PRIMARY KEY, realm TEXT NOT NULL, name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1, data TEXT NOT NULL,
                last_fired_ms INTEGER, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn put_asset(conn: &Connection, id: &str, realm: &str, ty: &str) {
        let asset = Asset::new_with_type(
            id.to_string(),
            realm.to_string(),
            ty.to_string(),
            id.to_string(),
            &AssetTypeInfo::default(),
        );
        store::upsert_asset(conn, &asset).unwrap();
    }

    fn write_event(
        conn: &Connection,
        asset_id: &str,
        name: &str,
        value: serde_json::Value,
        ts: i64,
    ) {
        let ev = AttributeEvent {
            asset_id: asset_id.to_string(),
            attribute_name: name.to_string(),
            value: AttributeValue(value),
            timestamp: ts,
            old_value: None,
            old_value_timestamp: 0,
        };
        store::process_attribute_event(conn, &ev, ts).unwrap();
    }

    fn save_ruleset(conn: &Connection, id: &str, realm: &str, rule: &JsonRule) {
        let data = serde_json::to_string(rule).unwrap();
        conn.execute(
            "INSERT INTO iot_rulesets (id, realm, name, enabled, data, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, '2026-01-01', '2026-01-01')
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, enabled = 1",
            params![id, realm, rule.name, data],
        )
        .unwrap();
    }

    fn ev(asset_id: &str, name: &str, value: serde_json::Value, ts: i64) -> AttributeEvent {
        AttributeEvent {
            asset_id: asset_id.to_string(),
            attribute_name: name.to_string(),
            value: AttributeValue(value),
            timestamp: ts,
            old_value: None,
            old_value_timestamp: 0,
        }
    }

    fn gt_rule(name: &str, attr: &str, threshold: f64) -> JsonRule {
        JsonRule {
            name: name.to_string(),
            when: RuleConditionGroup {
                operator: LogicOperator::And,
                items: vec![RuleCondition {
                    asset_types: vec![],
                    realm: None,
                    asset_ids: vec![],
                    attribute: AttributePredicate {
                        name: attr.to_string(),
                        value: Some(ValuePredicate::GreaterThan { value: threshold }),
                        meta: vec![],
                        previous_value: None,
                        timestamp_older_than: None,
                    },
                    duration: None,
                    reset_immediate: false,
                }],
                groups: vec![],
            },
            track_unmatched: false,
            recurrence: Recurrence::default(),
            on_match: OnMatch::default(),
        }
    }

    // ---- §2A.23 operators -------------------------------------------------

    #[test]
    fn value_operators_cover_eq_neq_range_between_contains_regex_null() {
        use serde_json::json;
        let num = AttributeValue(json!(10.0));
        assert!(eval_value_predicate(
            &ValuePredicate::Eq { value: json!(10.0) },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::Neq { value: json!(9.0) },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::GreaterThan { value: 9.0 },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::GreaterEqual { value: 10.0 },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::LessThan { value: 11.0 },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::LessEqual { value: 10.0 },
            &num
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::Between {
                min: 5.0,
                max: 15.0
            },
            &num
        ));
        assert!(!eval_value_predicate(
            &ValuePredicate::Between {
                min: 11.0,
                max: 15.0
            },
            &num
        ));

        let s = AttributeValue(json!("hello world"));
        assert!(eval_value_predicate(
            &ValuePredicate::Contains {
                value: "world".into()
            },
            &s
        ));
        assert!(eval_value_predicate(
            &ValuePredicate::Regex {
                pattern: "^hello".into()
            },
            &s
        ));
        assert!(!eval_value_predicate(
            &ValuePredicate::Regex {
                pattern: "^world".into()
            },
            &s
        ));

        let nul = AttributeValue(json!(null));
        assert!(eval_value_predicate(&ValuePredicate::IsNull, &nul));
        assert!(eval_value_predicate(&ValuePredicate::NotNull, &num));

        // bool coerced via as_numeric (true → 1.0).
        let b = AttributeValue(json!(true));
        assert!(eval_value_predicate(
            &ValuePredicate::GreaterEqual { value: 1.0 },
            &b
        ));
    }

    #[test]
    fn meta_item_anymatch_and_anded() {
        use serde_json::json;
        let mut meta = MetaMap::new();
        meta.insert("severity".into(), json!("critical"));
        meta.insert("rules".into(), json!(true));
        let pred = AttributePredicate {
            name: "temp".into(),
            value: None,
            meta: vec![
                MetaPredicate {
                    name: "severity".into(),
                    value: Some(ValuePredicate::Eq {
                        value: json!("critical"),
                    }),
                },
                MetaPredicate {
                    name: "rules".into(),
                    value: None,
                },
            ],
            previous_value: None,
            timestamp_older_than: None,
        };
        assert!(as_attribute_matcher(
            &pred,
            "temp",
            Some(&AttributeValue(json!(1))),
            0,
            &meta,
            None,
            1000
        ));
        // a missing meta key fails the AND.
        let pred2 = AttributePredicate {
            name: "temp".into(),
            value: None,
            meta: vec![MetaPredicate {
                name: "absent".into(),
                value: None,
            }],
            previous_value: None,
            timestamp_older_than: None,
        };
        assert!(!as_attribute_matcher(
            &pred2,
            "temp",
            Some(&AttributeValue(json!(1))),
            0,
            &meta,
            None,
            1000
        ));
    }

    #[test]
    fn previous_value_operator_tests_against_old_not_new() {
        use serde_json::json;
        let pred = AttributePredicate {
            name: "temp".into(),
            value: Some(ValuePredicate::GreaterThan { value: 50.0 }),
            meta: vec![],
            // old value must have been <= 50 (a fresh breach).
            previous_value: Some(ValuePredicate::LessEqual { value: 50.0 }),
            timestamp_older_than: None,
        };
        // new=60 (>50), old=40 (<=50) → match.
        assert!(as_attribute_matcher(
            &pred,
            "temp",
            Some(&AttributeValue(json!(60.0))),
            0,
            &MetaMap::new(),
            Some(&AttributeValue(json!(40.0))),
            1000
        ));
        // new=60, old=55 (>50) → previous_value fails.
        assert!(!as_attribute_matcher(
            &pred,
            "temp",
            Some(&AttributeValue(json!(60.0))),
            0,
            &MetaMap::new(),
            Some(&AttributeValue(json!(55.0))),
            1000
        ));
    }

    // ---- §2A.16 duration windowing ---------------------------------------

    #[test]
    fn duration_window_reset_on_predicate_false_and_restart() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.when.items[0].duration = Some(DurationSpec(5_000)); // 5s window
        save_ruleset(&conn, "rs-dur", "master", &rule);

        // t0: predicate true → window started, NO match yet.
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m0 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert!(
            m0.is_empty(),
            "duration window started, should not match yet"
        );

        // t0+5s with predicate still true → match.
        write_event(&conn, "asset-1", "temp", json!(61.0), 6_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(61.0), 6_000),
            6_000,
        )
        .unwrap();
        assert_eq!(m1.len(), 1, "window elapsed with predicate held → match");

        // reset previously_matched so we can observe re-windowing cleanly.
        conn.execute(
            "UPDATE iot_condition_state SET previously_matched = 0 WHERE ruleset_id = 'rs-dur'",
            [],
        )
        .unwrap();

        // predicate goes FALSE mid-window → first_match_ms deleted.
        write_event(&conn, "asset-1", "temp", json!(10.0), 7_000);
        let _ = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 7_000),
            7_000,
        )
        .unwrap();
        let cleared: Option<i64> = conn
            .query_row(
                "SELECT first_match_ms FROM iot_condition_state WHERE ruleset_id='rs-dur'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            cleared.is_none(),
            "predicate-false must DELETE first_match_ms"
        );

        // true again at t=8s: timer restarts from now, so NOT immediately matched
        // even though more than 5s passed since the original window.
        write_event(&conn, "asset-1", "temp", json!(70.0), 8_000);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(70.0), 8_000),
            8_000,
        )
        .unwrap();
        assert!(m2.is_empty(), "timer restarts on false→true, not resumed");
        let restarted: Option<i64> = conn
            .query_row(
                "SELECT first_match_ms FROM iot_condition_state WHERE ruleset_id='rs-dur'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(restarted, Some(8_000), "window restarts at the new now");
    }

    #[test]
    fn duration_none_matches_immediately() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        save_ruleset(&conn, "rs-imm", "master", &gt_rule("breach", "temp", 50.0));

        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m.len(), 1, "no duration → immediate match");
    }

    // ---- §2A.18 otherwise / unmatched ------------------------------------

    #[test]
    fn otherwise_only_with_track_unmatched_and_filtered() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.track_unmatched = true;
        save_ruleset(&conn, "rs-otw", "master", &rule);

        // asset matches the query (no type filter) but value 10 fails predicate.
        write_event(&conn, "asset-1", "temp", json!(10.0), 1_000);
        let m = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].kind, MatchKind::OtherwiseUnmatched);

        // second still-unmatched event is filtered (previously_unmatched).
        write_event(&conn, "asset-1", "temp", json!(11.0), 2_000);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(11.0), 2_000),
            2_000,
        )
        .unwrap();
        assert!(
            m2.is_empty(),
            "previously_unmatched filters repeat otherwise"
        );
    }

    #[test]
    fn or_mode_does_not_collect_unmatched() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.track_unmatched = true;
        rule.when.operator = LogicOperator::Or;
        save_ruleset(&conn, "rs-or", "master", &rule);

        write_event(&conn, "asset-1", "temp", json!(10.0), 1_000);
        let m = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 1_000),
            1_000,
        )
        .unwrap();
        assert!(m.is_empty(), "OR mode does not collect unmatched");
    }

    #[test]
    fn or_mode_rearms_on_unmatched_then_refires() {
        // §2A.15 re-arm: an OR-mode rule must re-fire after a clean
        // matched→unmatched→matched cycle (previously_matched cleared when the
        // matched condition goes Unmatched), not be permanently suppressed.
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.when.operator = LogicOperator::Or;
        save_ruleset(&conn, "rs-or-rearm", "master", &rule);

        // 1) condition true → fires.
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m1.len(), 1, "OR-mode first match fires");

        // 2) still-true repeat → suppressed by previously_matched.
        write_event(&conn, "asset-1", "temp", json!(61.0), 1_100);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(61.0), 1_100),
            1_100,
        )
        .unwrap();
        assert!(m2.is_empty(), "still-matched OR repeat is suppressed");

        // 3) condition goes false → re-arms (clears previously_matched).
        write_event(&conn, "asset-1", "temp", json!(10.0), 2_000);
        let m3 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 2_000),
            2_000,
        )
        .unwrap();
        assert!(
            m3.is_empty(),
            "unmatched OR produces no match (track_unmatched off)"
        );
        let pm: i64 = conn
            .query_row(
                "SELECT previously_matched FROM iot_condition_state WHERE ruleset_id='rs-or-rearm'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            pm, 0,
            "OR-mode re-arm cleared previously_matched on unmatched"
        );

        // 4) condition true again → RE-FIRES (the defect: previously suppressed).
        write_event(&conn, "asset-1", "temp", json!(70.0), 3_000);
        let m4 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(70.0), 3_000),
            3_000,
        )
        .unwrap();
        assert_eq!(
            m4.len(),
            1,
            "OR-mode rule re-fires after a clean unmatched→matched cycle"
        );
    }

    // ---- §2A.19 stale-AND reset ------------------------------------------

    #[test]
    fn stale_and_reset_allows_rematch() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        // two-condition AND: temp>50 AND humidity>80
        let rule = JsonRule {
            name: "both".into(),
            when: RuleConditionGroup {
                operator: LogicOperator::And,
                items: vec![
                    gt_rule("x", "temp", 50.0).when.items.remove(0),
                    gt_rule("x", "humidity", 80.0).when.items.remove(0),
                ],
                groups: vec![],
            },
            track_unmatched: false,
            recurrence: Recurrence::default(),
            on_match: OnMatch::default(),
        };
        save_ruleset(&conn, "rs-and", "master", &rule);

        // both true → match.
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        write_event(&conn, "asset-1", "humidity", json!(90.0), 1_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m1.len(), 1, "both conditions hold → match");

        // same still-true event again → previously_matched suppresses.
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(61.0), 1_100),
            1_100,
        )
        .unwrap();
        assert!(m2.is_empty(), "still-matched is suppressed");

        // humidity drops → one condition loses match → stale-AND reset clears
        // previously_matched for ALL conditions.
        write_event(&conn, "asset-1", "humidity", json!(10.0), 2_000);
        let _ = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "humidity", json!(10.0), 2_000),
            2_000,
        )
        .unwrap();
        let pm: i64 = conn
            .query_row(
                "SELECT MAX(previously_matched) FROM iot_condition_state WHERE ruleset_id='rs-and'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            pm, 0,
            "stale-AND reset cleared previously_matched for all conditions"
        );

        // humidity recovers → rule can re-match.
        write_event(&conn, "asset-1", "humidity", json!(95.0), 3_000);
        let m3 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "humidity", json!(95.0), 3_000),
            3_000,
        )
        .unwrap();
        assert_eq!(m3.len(), 1, "after stale reset the rule re-matches");
    }

    // ---- §2A.22 dedup-then-order/limit + RULE_RESET_IMMEDIATE ------------

    #[test]
    fn dedup_happens_before_limit() {
        let input = vec![
            "a".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
            "b".to_string(),
        ];
        // dedup FIRST → [a,b,c]; THEN limit 2 → [a,b].
        let out = dedup_then_limit(&input, Some(2));
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
        // without dedup-first, a naive limit-2 of the raw input would be [a,b]
        // too here, so prove dedup with a limit large enough to expose it:
        let out_all = dedup_then_limit(&input, Some(10));
        assert_eq!(
            out_all,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn rule_reset_immediate_strict_newer_clears_match() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.when.items[0].reset_immediate = true;
        save_ruleset(&conn, "rs-ri", "master", &rule);

        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m1.len(), 1);

        // equal timestamp does NOT clear (no strictly-newer value).
        // Force-write same ts current state then evaluate.
        let m_eq = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert!(m_eq.is_empty(), "equal timestamp does NOT re-trigger");

        // strictly-newer attr.timestamp clears previously_matched → re-trigger.
        write_event(&conn, "asset-1", "temp", json!(70.0), 2_000);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(70.0), 2_000),
            2_000,
        )
        .unwrap();
        assert_eq!(
            m2.len(),
            1,
            "strictly-newer value re-triggers via RULE_RESET_IMMEDIATE"
        );
    }

    // ---- §2A.15 re-trigger suppression (delegated: dedup + budget) --------

    #[test]
    fn re_trigger_suppressed_by_previously_matched() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        save_ruleset(&conn, "rs-sup", "master", &gt_rule("breach", "temp", 50.0));

        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let a = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(a.len(), 1);
        // second still-true match suppressed by previously_matched.
        let b = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(61.0), 1_100),
            1_100,
        )
        .unwrap();
        assert!(b.is_empty());
    }

    // ---- §2A.21 startup suppression + (c) emitter routing -----------------

    #[test]
    fn startup_suppression_then_warm_emits_alarm_and_bounded_spawn() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.on_match.enqueue_task = true;
        save_ruleset(&conn, "rs-emit", "master", &rule);

        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let matches = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(matches.len(), 1);

        // §2A.21: cold pass (engine_warm=false) emits nothing.
        let cold = emit_matches(root.path(), "master", &matches, false, 1_000).unwrap();
        assert!(cold.alarm_ids.is_empty());
        assert!(cold.spawn_edge_ids.is_empty());

        // warm pass: alarm + bounded queue-task spawn + ONE durable queue task.
        let warm = emit_matches(root.path(), "master", &matches, true, 1_000).unwrap();
        assert_eq!(warm.alarm_ids.len(), 1, "warm emits one alarm");
        assert_eq!(
            warm.spawn_edge_ids.len(),
            1,
            "warm spawns one bounded queue task"
        );
        assert_eq!(
            warm.queue_task_keys.len(),
            1,
            "warm creates one durable queue task (the notification-path inbound)"
        );
        // The durable task is a real inbound communication message on the core db.
        let task_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM communication_messages WHERE message_key = ?1",
                params![warm.queue_task_keys[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            task_count, 1,
            "durable queue task persisted for the agent to lease"
        );

        // alarm is projectable: it exists in iot_alarms with HIGH/MEDIUM severity OPEN.
        let alarm = crate::iot::alarms::get(&conn, &warm.alarm_ids[0]).unwrap();
        assert_eq!(alarm.status, crate::iot::alarms::Status::Open);
        assert_eq!(alarm.asset_ids, vec!["asset-1".to_string()]);

        // the spawn edge is bounded: parent=IotAlarm, child=QueueTask, finite budget.
        let (pt, ct, kind, accepted, max_attempts, budget): (String, String, String, i64, i64, String) = conn
            .query_row(
                "SELECT parent_entity_type, child_entity_type, spawn_kind, accepted, max_attempts, budget_key
                 FROM ctox_core_spawn_edges WHERE edge_id = ?1",
                params![warm.spawn_edge_ids[0]],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(pt, "IotAlarm");
        assert_eq!(ct, "QueueTask");
        assert_eq!(kind, "iot-event-queue-task");
        assert_eq!(accepted, 1);
        assert_eq!(max_attempts, 64);
        assert_eq!(budget, "iot-event:rs-emit:asset-1");
    }

    // ---- §2A.20 bounded re-firing via spawn budget ------------------------

    #[test]
    fn bounded_refiring_stops_at_budget() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.on_match.enqueue_task = true;
        save_ruleset(&conn, "rs-bud", "master", &rule);

        // Build a single match and emit it 64 + 1 times under the same budget_key.
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let matches = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(matches.len(), 1);

        let mut accepted = 0;
        let mut exhausted = 0;
        for i in 0..70 {
            // distinct now_ms → distinct child id / alarm, same budget_key.
            let rep = emit_matches(root.path(), "master", &matches, true, 1_000 + i).unwrap();
            accepted += rep.spawn_edge_ids.len();
            exhausted += rep.budget_exhausted.len();
        }
        assert_eq!(
            accepted, 64,
            "budget bounds accepted spawns at max_budget=64"
        );
        assert!(
            exhausted >= 1,
            "further re-fires are budget-exhausted, not a 100-cap"
        );

        // §2A.15 — despite 64 accepted re-fires under the same dedup key, EXACTLY
        // ONE durable queue task exists (the message_key IS the dedup key, so the
        // re-fires UPSERT one row). One brain: budget bounds re-fire count, dedup
        // bounds durable work.
        let conn2 = store::open_iot_store(root.path()).unwrap();
        let durable_tasks: i64 = conn2
            .query_row(
                "SELECT COUNT(*) FROM communication_messages
                 WHERE channel = 'iot' AND message_key = 'iot:system::rs-bud:asset-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            durable_tasks, 1,
            "exactly one durable queue task per dedup key (re-fires upsert the same row)"
        );
    }

    // ---- §4A surface 3 — evaluate_and_emit warm gate (production entry) ----

    #[test]
    fn evaluate_and_emit_suppresses_cold_then_fires_warm_with_dedup() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        let mut rule = gt_rule("breach", "temp", 50.0);
        rule.on_match.enqueue_task = true;
        save_ruleset(&conn, "rs-prod", "master", &rule);

        // Cold first pass (engine_warm=false): §2A.21 startup suppression — the
        // event is recorded but NO alarm/work is emitted.
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let cold = evaluate_and_emit(
            root.path(),
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            false,
            1_000,
        )
        .unwrap();
        assert!(cold.alarm_ids.is_empty(), "cold pass suppresses alarms");
        assert!(
            cold.queue_task_keys.is_empty(),
            "cold pass emits no durable work"
        );

        // previously_matched is now set for this asset; a still-true re-eval is
        // suppressed by the evaluator. Drop a fresh asset to observe a warm fire.
        put_asset(&conn, "asset-2", "master", "Thing");
        write_event(&conn, "asset-2", "temp", json!(70.0), 2_000);
        let warm = evaluate_and_emit(
            root.path(),
            "master",
            &ev("asset-2", "temp", json!(70.0), 2_000),
            true,
            2_000,
        )
        .unwrap();
        assert_eq!(warm.alarm_ids.len(), 1, "warm pass raises the alarm");
        assert_eq!(
            warm.queue_task_keys.len(),
            1,
            "warm pass emits one durable queue task"
        );
        assert_eq!(warm.queue_task_keys[0], "iot:system::rs-prod:asset-2");

        // A second still-true event for asset-2 is suppressed by previously_matched
        // (no new match), so no second durable task and no budget churn.
        write_event(&conn, "asset-2", "temp", json!(71.0), 2_100);
        let again = evaluate_and_emit(
            root.path(),
            "master",
            &ev("asset-2", "temp", json!(71.0), 2_100),
            true,
            2_100,
        )
        .unwrap();
        assert!(
            again.queue_task_keys.is_empty(),
            "still-matched re-eval emits nothing"
        );
    }

    // ---- spawn-liveness in-crate proof (release gate is the CLI) ----------

    #[test]
    fn spawn_model_is_live_with_iot_contracts() {
        let report = guard::analyze_core_spawn_model();
        assert!(
            report.ok,
            "spawn model must stay live: {:?}",
            report.violations
        );
        let names: Vec<&str> = report.spawner_contracts.iter().map(|c| c.pattern).collect();
        assert!(names.contains(&"iot-event-queue-task"));
        assert!(names.contains(&"iot-event-message"));
        for c in &report.spawner_contracts {
            if c.pattern.starts_with("iot-event") {
                assert!(c.max_budget >= 1 && c.max_budget <= 64, "budget in [1..64]");
                assert_eq!(c.intervention_skill, "queue-cleanup");
                assert_eq!(c.parent_entity_types, &["IotAlarm"]);
            }
        }
    }

    // ---- §2A.17 recurrence block enforcement ------------------------------

    fn recurring_gt_rule(
        name: &str,
        attr: &str,
        threshold: f64,
        recurrence: Recurrence,
    ) -> JsonRule {
        let mut rule = gt_rule(name, attr, threshold);
        rule.on_match.enqueue_task = true;
        rule.recurrence = recurrence;
        rule
    }

    #[test]
    fn recurrence_mins_blocks_refire_within_window_per_asset() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");

        // mins=5 (300_000 ms) PER_ASSET (default scope).
        let rule = recurring_gt_rule(
            "breach",
            "temp",
            50.0,
            Recurrence {
                scope: None,
                mins: Some(5),
            },
        );
        save_ruleset(&conn, "rs-rec", "master", &rule);

        // First warm match fires (admitted, stamps the block at t=1_000).
        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        assert_eq!(m1.len(), 1);
        let r1 = emit_matches(root.path(), "master", &m1, true, 1_000).unwrap();
        assert_eq!(r1.alarm_ids.len(), 1, "first match fires");
        assert!(r1.recurrence_blocked.is_empty());

        // Re-arm the rule (asset drops below threshold) so the evaluator would
        // produce a fresh match again — without recurrence this would re-fire.
        write_event(&conn, "asset-1", "temp", json!(10.0), 2_000);
        let _ = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 2_000),
            2_000,
        )
        .unwrap();
        write_event(&conn, "asset-1", "temp", json!(70.0), 3_000);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(70.0), 3_000),
            3_000,
        )
        .unwrap();
        assert_eq!(m2.len(), 1, "evaluator re-armed and produced a fresh match");

        // Still inside the 5-min window (t=3_000 < 1_000 + 300_000) → blocked.
        let r2 = emit_matches(root.path(), "master", &m2, true, 3_000).unwrap();
        assert!(r2.alarm_ids.is_empty(), "within window: no alarm");
        assert_eq!(
            r2.recurrence_blocked,
            vec!["rs-rec".to_string()],
            "within window: recurrence-blocked"
        );

        // After the window elapses (t >= 1_000 + 300_000) a fresh match fires.
        write_event(&conn, "asset-1", "temp", json!(10.0), 301_500);
        let _ = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(10.0), 301_500),
            301_500,
        )
        .unwrap();
        write_event(&conn, "asset-1", "temp", json!(80.0), 302_000);
        let m3 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(80.0), 302_000),
            302_000,
        )
        .unwrap();
        assert_eq!(m3.len(), 1);
        let r3 = emit_matches(root.path(), "master", &m3, true, 302_000).unwrap();
        assert_eq!(
            r3.alarm_ids.len(),
            1,
            "after the window the recurrence block lifts and the match fires"
        );
        assert!(r3.recurrence_blocked.is_empty());
    }

    #[test]
    fn recurrence_mins_zero_always_admits() {
        // mins=0 ("always") imposes no time block: re-fire is governed only by the
        // previously_matched re-arm cycle, so each fresh match fires immediately.
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        let rule = recurring_gt_rule(
            "breach",
            "temp",
            50.0,
            Recurrence {
                scope: None,
                mins: Some(0),
            },
        );
        save_ruleset(&conn, "rs-always", "master", &rule);

        let mut fires = 0;
        // fire / re-arm / fire — within milliseconds — all admitted.
        for (i, v) in [(1_000_i64, 60.0), (1_100, 10.0), (1_200, 70.0)] {
            write_event(&conn, "asset-1", "temp", json!(v), i);
            let m =
                evaluate_event(&conn, "master", &ev("asset-1", "temp", json!(v), i), i).unwrap();
            let r = emit_matches(root.path(), "master", &m, true, i).unwrap();
            fires += r.alarm_ids.len();
            assert!(r.recurrence_blocked.is_empty(), "mins=0 never blocks");
        }
        assert_eq!(
            fires, 2,
            "both true→matches fired (the false re-arm produced none)"
        );
    }

    #[test]
    fn recurrence_global_scope_blocks_across_assets() {
        use serde_json::json;
        let root = temp_root();
        let conn = open(root.path());
        put_asset(&conn, "asset-1", "master", "Thing");
        put_asset(&conn, "asset-2", "master", "Thing");
        // GLOBAL scope: a match on ANY asset blocks re-fire for the whole ruleset.
        let rule = recurring_gt_rule(
            "breach",
            "temp",
            50.0,
            Recurrence {
                scope: Some(RecurrenceScope::Global),
                mins: Some(5),
            },
        );
        save_ruleset(&conn, "rs-glob", "master", &rule);

        write_event(&conn, "asset-1", "temp", json!(60.0), 1_000);
        let m1 = evaluate_event(
            &conn,
            "master",
            &ev("asset-1", "temp", json!(60.0), 1_000),
            1_000,
        )
        .unwrap();
        let r1 = emit_matches(root.path(), "master", &m1, true, 1_000).unwrap();
        assert_eq!(r1.alarm_ids.len(), 1, "first asset fires");

        // A DIFFERENT asset matching inside the window is blocked GLOBALLY.
        write_event(&conn, "asset-2", "temp", json!(70.0), 2_000);
        let m2 = evaluate_event(
            &conn,
            "master",
            &ev("asset-2", "temp", json!(70.0), 2_000),
            2_000,
        )
        .unwrap();
        assert_eq!(m2.len(), 1, "asset-2 is a fresh match for the evaluator");
        let r2 = emit_matches(root.path(), "master", &m2, true, 2_000).unwrap();
        assert!(
            r2.alarm_ids.is_empty(),
            "GLOBAL scope blocks a second asset inside the window"
        );
        assert_eq!(r2.recurrence_blocked, vec!["rs-glob".to_string()]);
    }
}
