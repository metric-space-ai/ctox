// Origin: CTOX
// License: AGPL-3.0-only
//
// Phase 3 — native IoT protocol-agent base layer. Ported domain semantics from
// OpenRemote (AGPL-3.0, archive/openremote, HEAD 22a42a7); persistence/transport
// reimplemented on CTOX-native SQLite + tokio. See docs/legal/NOTICE.
//
// This file owns the *protocol-agnostic* surface every native agent shares:
//   1. the `IotAgent` trait (the connect/subscribe/read/write surface),
//   2. the shared inbound/outbound value-processing base layer (§2A.28:
//      filters -> converters -> built-in coercion, %VALUE%/%TIME% placeholders),
//   3. the DETERMINISTIC reconnect state machine (§2A.24: exponential backoff
//      1s->5min with 25% jitter, infinite retries, atomic CAS transitions).
//
// HARD RULES honored here:
//   * native Rust only; no new MQTT/HTTP framework deps.
//   * the clock is INJECTED into every time-dependent fn (backoff, %TIME%,
//     poll-ready) — NO wall-clock reads — so the whole file is deterministically
//     testable. Production callers pass `crate::iot::now_ms()`.
//   * runtime state belongs in runtime/ctox.sqlite3 via the engine write path
//     (`store::process_attribute_event`); config/secrets flow through
//     `runtime_env::env_or_config(root, …)` + the CTOX secret store, NEVER
//     `std::env`. Agents talk to DEVICES, not the browser.
//   * ported algorithmic fns carry `// ref: <upstream-file>:<line-range>` with
//     preserved upstream names (doInboundValueProcessing, applyValueConverter,
//     checkSetConnectionStatus, scheduleDoConnect, …).

use crate::iot::model::{coerce_value, AttributeValue, ValueBaseType};
use crate::iot::Result;
use std::collections::BTreeMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// 3.0 Protocol-agent kind (closed set)
// ---------------------------------------------------------------------------
//
// The dispatch table (`gateway::build_agent`) and the per-protocol agents land
// in later Phase-3 work; the kind is declared here so the trait + base layer
// compile standalone. Mirrors communication::CommunicationAdapterKind: a closed
// enum where adding a member is a deliberate core edit.

/// One native protocol agent flavor. ref: AgentLink subtypes (mqtt/http/websocket)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum IotAgentKind {
    Mqtt,
    Http,
    WebSocket,
}

impl IotAgentKind {
    /// Parse the `iot_agents.kind` string persisted by the Phase-2
    /// `agent_configure` op (`"mqtt" | "http" | "websocket"`).
    /// ref: communication external_adapter_for_channel
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "mqtt" => Some(Self::Mqtt),
            "http" => Some(Self::Http),
            "websocket" | "ws" => Some(Self::WebSocket),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Http => "http",
            Self::WebSocket => "websocket",
        }
    }
}

// ---------------------------------------------------------------------------
// 3.1 Shared agent types
// ---------------------------------------------------------------------------

/// ref: ConnectionStatus (org.openremote.model.asset.agent.ConnectionStatus)
///
/// Atomic-CAS transition target for the reconnect state machine. The legal
/// transition graph is enforced by `ReconnectStateMachine::check_set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ConnectionStatus {
    /// Initial; the ONLY state from which `begin_connect` may enter Connecting.
    Disconnected,
    Connecting,
    /// Backoff delay before the next Connecting attempt (§2A.24).
    Waiting,
    Connected,
    /// Graceful shutdown in flight; blocks reconnect (stale-execution guard).
    Disconnecting,
}

/// One inbound device reading the agent extracts from a protocol message BEFORE
/// base-layer processing. `raw` is whatever the device sent.
#[derive(Clone, Debug)]
pub(crate) struct AttributeReading {
    pub asset_id: String,
    pub attribute_name: String,
    pub raw: AttributeValue,
    /// device-supplied epoch-ms, or 0 ("no explicit timestamp", §2A.1) — the
    /// runtime hands this to `process_attribute_event`, which normalizes it
    /// against system time.
    pub device_timestamp_ms: i64,
}

/// A single inbound value filter. The full OpenRemote `ValueFilter` hierarchy
/// (regex/substring/json-path/math) is reproduced incrementally; the
/// load-bearing chaining contract (apply in order, any `None` short-circuits to
/// drop) is captured here. ref: ValueUtil.applyValueFilters
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ValueFilter {
    /// Keep only substrings between `begin`..`end` (1:1 with SubStringValueFilter).
    #[serde(rename = "substring")]
    Substring { begin: usize, end: Option<usize> },
    /// Replace `pattern` occurrences with `replacement` (RegexValueFilter shape,
    /// literal match here — the device firehose path stays allocation-cheap).
    #[serde(rename = "replace")]
    Replace {
        pattern: String,
        replacement: String,
    },
}

/// Converter map: device-value (uppercased string key) -> replacement, with the
/// `@IGNORE` / `@NULL` sentinels and `"*"` wildcard NEGATE honored upstream-style.
/// ref: ProtocolUtil.java:163-200 (applyValueConverter)
pub(crate) type ConverterMap = BTreeMap<String, serde_json::Value>;

/// Per-attribute device binding resolved from the agent's `data` JSON config
/// (iot_agents.data) plus the attribute's agentLink meta. Protocol-specific
/// fields live in the opaque `binding` value the agent interprets.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub(crate) struct AgentLink {
    #[serde(alias = "assetId")]
    pub asset_id: String,
    #[serde(alias = "attributeName")]
    pub attribute_name: String,
    /// inbound filters (applied first) then the converter map (applied second). §2A.28
    #[serde(default, alias = "valueFilters")]
    pub value_filters: Vec<ValueFilter>,
    #[serde(default, alias = "valueConverter")]
    pub value_converter: Option<ConverterMap>,
    /// outbound write filters/converter (separate chain from inbound). §2A.28
    #[serde(default, alias = "writeValueFilters")]
    pub write_value_filters: Vec<ValueFilter>,
    #[serde(default, alias = "writeValueConverter")]
    pub write_value_converter: Option<ConverterMap>,
    /// literal write template; may contain %VALUE% / %TIME% placeholders. §2A.28
    #[serde(default, alias = "writeValue")]
    pub write_value: Option<String>,
    /// §2A.30 — re-write the attribute locally immediately after the outbound write.
    #[serde(default, alias = "updateOnWrite")]
    pub update_on_write: bool,
    /// protocol-specific binding (mqtt topic+qos, http url+interval, ws path).
    #[serde(default)]
    pub binding: serde_json::Value,
}

/// Agent runtime context: the resolved config + secret-store handle root.
/// Secrets are read via `runtime_env::env_or_config(root, …)` — never std::env.
pub(crate) struct AgentContext<'a> {
    pub root: &'a Path,
    pub agent_id: String,
    pub realm: String,
    /// opaque agent config (iot_agents.data); host/port/clientId/credential-keys.
    pub config: serde_json::Value,
}

// ---------------------------------------------------------------------------
// 3.2 The IotAgent trait
// ---------------------------------------------------------------------------

/// One native protocol agent. Mirrors `communication::CommunicationTransportAdapter`:
/// a closed kind with a connect/subscribe/read/write surface.
///
/// Methods are SYNC and internally drive an isolated tokio runtime (the
/// `run_async` idiom from `communication::whatsapp_native`): no `async fn` in
/// the public trait, no shared global runtime, no new `async-trait` dependency.
/// The base layer below (NOT the agent) owns filters/converters/placeholders.
pub(crate) trait IotAgent: Send {
    fn kind(&self) -> IotAgentKind;

    /// Establish the device link. Drives the reconnect state machine toward
    /// CONNECTED (or returns the WAITING/backoff state on transient failure —
    /// infinite retries are the runtime loop's job, not connect()'s). Idempotent
    /// if already connected.
    /// ref: AbstractIOClientProtocol.java:152-162 (doStart must not throw)
    fn connect(&mut self, ctx: &AgentContext) -> Result<ConnectionStatus>;

    /// Register interest in a device source for one linked attribute. Safe to
    /// call while a reconnect is in flight (§2A.27): mutates the synchronized
    /// link table only.
    /// ref: AbstractProtocol.java:104-133 (linkAttribute)
    fn subscribe(&mut self, link: &AgentLink) -> Result<()>;

    /// Drain any inbound device messages that have arrived since the last read,
    /// mapped to raw `AttributeReading`s. The base layer (NOT this method)
    /// applies filters -> converters -> placeholders.
    /// ref: AbstractIOClientProtocol.java:195-200 (onMessageReceived)
    fn read(&mut self) -> Result<Vec<AttributeReading>>;

    /// Send one outbound write to the device. `processed` is the post-base-layer
    /// value (filters/converters/placeholders already applied by the runtime).
    /// Fire-and-forget unless `link.update_on_write` (§2A.30 — the runtime owns
    /// the local re-write).
    /// ref: AbstractIOClientProtocol.java:164-179 (doLinkedAttributeWrite)
    fn write(&mut self, link: &AgentLink, processed: &AttributeValue) -> Result<()>;

    /// Remove a link. Safe during reconnect (§2A.27).
    /// ref: AbstractProtocol.java:104-133 (unlinkAttribute)
    fn unlink(&mut self, link: &AgentLink) -> Result<()>;

    fn status(&self) -> ConnectionStatus;
}

// ---------------------------------------------------------------------------
// 3.3 Shared value-processing base layer (§2A.28)
// ---------------------------------------------------------------------------
//
// This is the load-bearing ported code; it lives ONCE here and every agent
// calls it through the runtime. Upstream fn names preserved.

/// Inbound: filters (silently null on no-match) -> converter (map/negate/drop)
/// -> built-in type coercion (AFTER the converter). Returns `(ignore, value)`;
/// `ignore == true` drops the update. Coercion failure -> `ignore == true`
/// (§2A.5 drop, not a hard error — the device firehose must not wedge the agent
/// loop). §2A.28
///
/// ref: ProtocolUtil.java:113-160 (doInboundValueProcessing)
pub(crate) fn do_inbound_value_processing(
    reading: &AttributeReading,
    link: &AgentLink,
    declared_type: Option<ValueBaseType>,
) -> (bool /*ignore*/, AttributeValue) {
    // value filtering — ref: ProtocolUtil.java:118-126
    // applyValueFilters may produce null (no match); we carry that as JSON null,
    // mirroring the upstream `valRef.set(null)` then null-guard below.
    let mut val = match apply_value_filters(&reading.raw, &link.value_filters) {
        Some(v) => v,
        None => AttributeValue(serde_json::Value::Null),
    };

    // value conversion — ref: ProtocolUtil.java:128-138
    if let Some(converter) = &link.value_converter {
        let (ignore, converted) = apply_value_converter(&val, converter);
        if ignore {
            // ref: ProtocolUtil.java:134-136 (early return on @IGNORE)
            return (true, converted);
        }
        val = converted;
    }

    // ref: ProtocolUtil.java:140-142 — a null after conversion is NOT ignored,
    // it is forwarded (a clear). No further coercion is attempted on null.
    if val.is_null() {
        return (false, val);
    }

    // built-in value conversion — ref: ProtocolUtil.java:144-157
    // Coerce to the declared base type; failure -> ignore (drop), matching the
    // upstream "cannot send linked attribute update" warning + Pair<true,null>.
    if let Some(ty) = declared_type {
        match coerce_value(&val, ty) {
            Ok(coerced) => (false, coerced),
            Err(_) => (true, AttributeValue(serde_json::Value::Null)),
        }
    } else {
        (false, val)
    }
}

/// Outbound: write filters -> write converter -> if the link has dynamic
/// placeholders, do `%VALUE%` / `%TIME%` replacement against the `write_value`
/// template; else use the converted value. Returns the processed value/string
/// to hand to `agent.write()`. `None` == `@IGNORE` / drop.
///
/// `now_ms` is INJECTED — `%TIME%` uses it, NOT the wall clock (§2A.28).
///
/// ref: ProtocolUtil.java:63-103 (doOutboundValueProcessing)
pub(crate) fn do_outbound_value_processing(
    value: &AttributeValue,
    link: &AgentLink,
    now_ms: i64,
) -> Result<Option<AttributeValue>> {
    // value filters — ref: ProtocolUtil.java:69-74
    let filtered = apply_value_filters(value, &link.write_value_filters)
        .unwrap_or_else(|| AttributeValue(serde_json::Value::Null));

    // value conversion — ref: ProtocolUtil.java:76-87
    let converted = if let Some(converter) = &link.write_value_converter {
        let (ignore, conv) = apply_value_converter(&filtered, converter);
        if ignore {
            // ref: ProtocolUtil.java:83-85 (return ignore Pair)
            return Ok(None);
        }
        conv
    } else {
        filtered
    };

    // dynamic placeholder insertion — ref: ProtocolUtil.java:89-100
    let write_value = link.write_value.as_deref().unwrap_or("");
    let has_write_value = !write_value.is_empty();
    if has_write_value {
        // `containsDynamicPlaceholder` is recorded per-link at link-time upstream
        // (perf note §2A.28); here it is cheap to recompute and the result is
        // identical. ref: ProtocolUtil.java:94 (containsDynamicPlaceholder flag)
        let rendered = if has_dynamic_placeholders(write_value) {
            let r = do_dynamic_value_replace(write_value, &converted);
            do_dynamic_time_replace(&r, now_ms)
        } else {
            write_value.to_string()
        };
        return Ok(Some(AttributeValue(serde_json::Value::String(rendered))));
    }

    Ok(Some(converted))
}

/// ref: ProtocolUtil.java:162-200 (applyValueConverter)
///
/// `@IGNORE` -> `(true, null)` (drop the update); `@NULL` -> `(false, null)`
/// (forward a clear); `"*"` wildcard with `NEGATE` flips numbers/booleans;
/// an unmatched key with no `"*"` fallback -> `(true, value)` (ignore).
fn apply_value_converter(
    value: &AttributeValue,
    converter: &ConverterMap,
) -> (bool, AttributeValue) {
    // converterKey = uppercased string-coercion of the value, else NULL literal.
    // ref: ProtocolUtil.java:169 (getValueCoerced(value, String).toUpperCase, NULL_LITERAL)
    let converter_key = value_to_string_key(value)
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| "NULL".to_string());

    if let Some(mapped) = converter.get(&converter_key) {
        // ref: ProtocolUtil.java:172-184
        if let serde_json::Value::String(s) = mapped {
            if s.eq_ignore_ascii_case("@IGNORE") {
                return (true, AttributeValue(serde_json::Value::Null));
            }
            if s.eq_ignore_ascii_case("@NULL") {
                return (false, AttributeValue(serde_json::Value::Null));
            }
        }
        return (false, AttributeValue(mapped.clone()));
    }

    // wildcard fallback — ref: ProtocolUtil.java:185-198
    if let Some(wildcard) = converter.get("*") {
        if let serde_json::Value::String(s) = wildcard {
            // AttributeLink.ConverterType.NEGATE.getValue() == "@NEGATE"
            // Type dispatch matches upstream isNumber()/isBoolean() — a Boolean is
            // NOT a number, so the boolean branch must be checked on the JSON shape
            // directly, NOT via as_numeric() (which coerces bool -> 1.0).
            if s == "@NEGATE" {
                match &value.0 {
                    // NEGATE on a number — ref: ProtocolUtil.java:189-191
                    serde_json::Value::Number(n) => {
                        let neg = n
                            .as_f64()
                            .and_then(|f| serde_json::Number::from_f64(f * -1.0))
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null);
                        return (false, AttributeValue(neg));
                    }
                    // NEGATE on a boolean — ref: ProtocolUtil.java:192-194
                    serde_json::Value::Bool(b) => {
                        return (false, AttributeValue(serde_json::Value::Bool(!b)));
                    }
                    _ => {}
                }
            }
        }
        return (false, AttributeValue(wildcard.clone()));
    }

    // no match, no wildcard -> ignore. ref: ProtocolUtil.java:199 (Pair<true,value>)
    (true, value.clone())
}

/// ref: ValueUtil.applyValueFilters
///
/// Apply each filter in order; any filter that yields no value short-circuits
/// the whole chain to `None` (drop), exactly like the upstream null propagation.
fn apply_value_filters(value: &AttributeValue, filters: &[ValueFilter]) -> Option<AttributeValue> {
    if filters.is_empty() {
        return Some(value.clone());
    }
    // Filters operate on the string projection of the value (upstream coerces to
    // String for substring/regex filters). Non-stringable values pass through.
    let mut current = value.clone();
    for filter in filters {
        let s = match value_to_string_key(&current) {
            Some(s) => s,
            None => return Some(current), // not string-projectable; leave as-is
        };
        match filter {
            ValueFilter::Substring { begin, end } => {
                // ref: SubStringValueFilter (begin inclusive, end exclusive)
                let chars: Vec<char> = s.chars().collect();
                if *begin > chars.len() {
                    return None; // out of range -> null -> drop
                }
                let stop = end.unwrap_or(chars.len()).min(chars.len());
                if stop < *begin {
                    return None;
                }
                let sub: String = chars[*begin..stop].iter().collect();
                current = AttributeValue(serde_json::Value::String(sub));
            }
            ValueFilter::Replace {
                pattern,
                replacement,
            } => {
                // ref: RegexValueFilter (literal replace form)
                let replaced = s.replace(pattern, replacement);
                current = AttributeValue(serde_json::Value::String(replaced));
            }
        }
    }
    Some(current)
}

/// ref: Constants.containsDynamicValuePlaceholder / containsDynamicTimePlaceholder
/// (Constants.java:155-161 — plain substring checks, not the full regex)
fn has_dynamic_placeholders(template: &str) -> bool {
    template.contains("%VALUE") || template.contains("%TIME")
}

/// ref: ValueUtil.java:1439-1465 (doDynamicValueReplace)
///
/// Replace each `%VALUE%` occurrence with the string projection of `value`
/// (NULL literal when null). The optional `%VALUE:fmt%` format spec is parsed
/// but only the no-format case is rendered here (the device write path uses bare
/// `%VALUE%`); a present format falls back to the plain projection.
fn do_dynamic_value_replace(template: &str, value: &AttributeValue) -> String {
    let projection = value_to_string_key(value).unwrap_or_else(|| "NULL".to_string());
    replace_placeholder(template, "%VALUE", &projection)
}

/// ref: ValueUtil.java:1398-1437 (doDynamicTimeReplace)
///
/// Replace each `%TIME%` occurrence with the ISO-8601 instant for the INJECTED
/// `now_ms`, and the `:EPOCH_SECONDS` / `:EPOCH_MILLIS` format specs with their
/// numeric forms. Wall-clock is never read — the caller supplies `now_ms`.
fn do_dynamic_time_replace(template: &str, now_ms: i64) -> String {
    if !template.contains("%TIME") {
        return template.to_string();
    }
    // EPOCH_MILLIS / EPOCH_SECONDS format specs first (longest-match first).
    let with_millis = replace_placeholder(template, "%TIME:EPOCH_MILLIS%", &now_ms.to_string());
    let with_secs = replace_placeholder(
        &with_millis,
        "%TIME:EPOCH_SECONDS%",
        &(now_ms / 1000).to_string(),
    );
    // bare %TIME% -> ISO-8601 instant. ref: ValueUtil.java:1422 (instant.toString())
    let iso = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default();
    replace_placeholder(&with_secs, "%TIME%", &iso)
}

/// Helper for the bare/specced placeholder forms: replace `marker` token then,
/// for the bare-value case, also the un-suffixed `%MARKER%` wrapper.
fn replace_placeholder(template: &str, marker: &str, replacement: &str) -> String {
    if marker.ends_with('%') {
        // fully-formed token (e.g. "%TIME:EPOCH_MILLIS%")
        template.replace(marker, replacement)
    } else {
        // bare value/time marker without trailing format -> wrap with `%`
        template.replace(&format!("{marker}%"), replacement)
    }
}

/// Coerce a value to the string key used by the converter/filter/placeholder
/// layers. Numbers/booleans/strings project directly; objects/arrays/null do not.
/// ref: ValueUtil.getValueCoerced(value, String.class)
fn value_to_string_key(value: &AttributeValue) -> Option<String> {
    match &value.0 {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 3.4 Deterministic reconnect state machine (§2A.24)
// ---------------------------------------------------------------------------
//
// Port of AbstractMQTT_IOClient's Failsafe RetryPolicy (exponential backoff
// 1s..5min, withJitter(0.25), withMaxRetries(Integer.MAX_VALUE)) + the atomic
// compareAndSet status transitions. Protocol-agnostic: all three agents embed
// one. The clock is INJECTED (`now_ms: i64`), never wall-clock, and jitter comes
// from a seedable PRNG (NOT rand::thread_rng) so the curve is reproducible.

/// ref: AbstractNettyIOClient.java:80 / AbstractMQTT_IOClient.java:118
const RECONNECT_DELAY_INITIAL_MILLIS: i64 = 1_000;
/// ref: AbstractNettyIOClient.java:81 / AbstractMQTT_IOClient.java:119
const RECONNECT_DELAY_MAX_MILLIS: i64 = 5 * 60_000;
/// ref: AbstractMQTT_IOClient.java:633 (withJitter(0.25))
const RECONNECT_JITTER: f64 = 0.25;
// withMaxRetries(Integer.MAX_VALUE) -> infinite, modeled as "never give up".
// ref: AbstractMQTT_IOClient.java:644

/// ref: AbstractMQTT_IOClient.java:122,628-669
pub(crate) struct ReconnectStateMachine {
    status: ConnectionStatus,
    /// retry count -> exponential delay exponent. 0 == "next attempt is the first".
    attempt: u32,
    /// when the WAITING delay expires (computed with the injected `now_ms`).
    next_attempt_at_ms: i64,
    /// deterministic jitter source (xorshift64*), seeded per agent so a fixed
    /// seed yields a fixed backoff curve. NOT a wall-clock-seeded RNG.
    jitter_state: u64,
}

impl ReconnectStateMachine {
    /// ref: AbstractMQTT_IOClient.java:122 (initial DISCONNECTED)
    pub(crate) fn new(jitter_seed: u64) -> Self {
        // Avoid a 0 state (xorshift fixed point); fold in a constant.
        let seeded = jitter_seed ^ 0x9E37_79B9_7F4A_7C15;
        ReconnectStateMachine {
            status: ConnectionStatus::Disconnected,
            attempt: 0,
            next_attempt_at_ms: 0,
            jitter_state: if seeded == 0 { 1 } else { seeded },
        }
    }

    /// CAS-style guarded transition: only transitions if `status == expected`.
    /// ref: AbstractMQTT_IOClient.java:550-557 (checkSetConnectionStatus)
    fn check_set(&mut self, expected: ConnectionStatus, next: ConnectionStatus) -> bool {
        if self.status == expected {
            self.status = next;
            true
        } else {
            false
        }
    }

    /// connect() entry: Disconnected -> Connecting only.
    /// ref: AbstractMQTT_IOClient.java:430
    pub(crate) fn begin_connect(&mut self) -> bool {
        self.check_set(ConnectionStatus::Disconnected, ConnectionStatus::Connecting)
    }

    /// On a successful link: Connecting -> Connected, reset the attempt counter.
    /// ref: AbstractMQTT_IOClient.java:664 (checkSet CONNECTING->CONNECTED)
    pub(crate) fn mark_connected(&mut self) -> bool {
        let ok = self.check_set(ConnectionStatus::Connecting, ConnectionStatus::Connected);
        if ok {
            self.attempt = 0;
        }
        ok
    }

    /// On an unexpected drop while Connected: Connected -> Connecting (triggers
    /// reconnect). ref: AbstractMQTT_IOClient.java:161 (disconnectedListener)
    pub(crate) fn on_disconnected(&mut self) -> bool {
        self.check_set(ConnectionStatus::Connected, ConnectionStatus::Connecting)
    }

    /// On a failed connect attempt: schedule backoff -> Waiting, computing the
    /// next attempt time from the INJECTED `now_ms`.
    /// ref: AbstractMQTT_IOClient.java:628-645 (scheduleDoConnect), :638 (WAITING)
    pub(crate) fn schedule_backoff(&mut self, now_ms: i64) {
        self.attempt = self.attempt.saturating_add(1);
        let delay = self.backoff_delay_ms();
        self.next_attempt_at_ms = now_ms.saturating_add(delay);
        self.status = ConnectionStatus::Waiting;
    }

    /// Exponential 1s -> 5min (factor 2, Failsafe default), capped, then ±25%
    /// deterministic jitter. Returns the delay for the CURRENT `attempt`.
    /// ref: AbstractMQTT_IOClient.java:628-645 (withBackoff(delay,maxDelay)+withJitter(0.25))
    pub(crate) fn backoff_delay_ms(&mut self) -> i64 {
        // Failsafe withBackoff uses a delay factor of 2 by default; attempt 1 ->
        // base, attempt 2 -> base*2, … capped at maxDelay.
        // ref: dev.failsafe Delay.with(...).withBackoff default factor == 2
        let exp = self.attempt.saturating_sub(1).min(40); // guard u64 shift overflow
        let scaled = (RECONNECT_DELAY_INITIAL_MILLIS as i128) << exp;
        let base = scaled.min(RECONNECT_DELAY_MAX_MILLIS as i128) as i64;

        // ±RECONNECT_JITTER fraction of `base`, deterministic from the seed.
        // ref: AbstractMQTT_IOClient.java:633 (withJitter(0.25))
        let r = self.next_unit_f64(); // [0,1)
        let jitter_span = base as f64 * RECONNECT_JITTER; // ±25% of base
        let offset = (r * 2.0 - 1.0) * jitter_span; // [-span, +span)
        let jittered = (base as f64 + offset).round() as i64;
        // Clamp into the legal [1s .. 5min] envelope after jitter.
        jittered
            .max(RECONNECT_DELAY_INITIAL_MILLIS)
            .min(RECONNECT_DELAY_MAX_MILLIS)
    }

    /// In WAITING, has the backoff expired given the injected `now_ms`? If so,
    /// restore Waiting -> Connecting and report it's time to retry.
    /// ref: AbstractMQTT_IOClient.java:475 (checkSet WAITING->CONNECTING)
    pub(crate) fn poll_ready_to_retry(&mut self, now_ms: i64) -> bool {
        if self.status == ConnectionStatus::Waiting && now_ms >= self.next_attempt_at_ms {
            self.check_set(ConnectionStatus::Waiting, ConnectionStatus::Connecting)
        } else {
            false
        }
    }

    /// Graceful disconnect: any non-terminal state -> Disconnecting (CAS loop).
    /// Blocks reconnect (the stale-execution guard reads this).
    /// ref: AbstractMQTT_IOClient.java:573-606, :464-468
    pub(crate) fn begin_disconnect(&mut self) {
        loop {
            let current = self.status;
            if matches!(
                current,
                ConnectionStatus::Disconnected | ConnectionStatus::Disconnecting
            ) {
                return; // already (dis)connecting/disconnected
            }
            if self.check_set(current, ConnectionStatus::Disconnecting) {
                return;
            }
            // Single-threaded here; the loop mirrors the upstream CAS retry shape.
        }
    }

    /// ref: AbstractMQTT_IOClient.java:599 (setConnectionStatus(DISCONNECTED))
    pub(crate) fn mark_disconnected(&mut self) {
        self.status = ConnectionStatus::Disconnected;
    }

    /// Stale-task guard: true if Disconnecting/Disconnected (abort an in-flight
    /// connect issued before a begin_disconnect).
    /// ref: AbstractMQTT_IOClient.java:464-468, :451
    pub(crate) fn is_aborting(&self) -> bool {
        matches!(
            self.status,
            ConnectionStatus::Disconnecting | ConnectionStatus::Disconnected
        )
    }

    pub(crate) fn status(&self) -> ConnectionStatus {
        self.status
    }

    #[cfg(test)]
    pub(crate) fn next_attempt_at_ms(&self) -> i64 {
        self.next_attempt_at_ms
    }

    /// xorshift64* — deterministic, seedable, no external dep. Advances the
    /// internal state and returns a value in `[0, 1)`.
    fn next_unit_f64(&mut self) -> f64 {
        let mut x = self.jitter_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.jitter_state = x;
        let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // top 53 bits -> [0,1) double.
        (v >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------------------
// 3.5 Tests — deterministic, no wall clock
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn link_with(converter: Option<ConverterMap>, filters: Vec<ValueFilter>) -> AgentLink {
        AgentLink {
            asset_id: "a1".into(),
            attribute_name: "temp".into(),
            value_filters: filters,
            value_converter: converter,
            ..AgentLink::default()
        }
    }

    fn reading(raw: serde_json::Value) -> AttributeReading {
        AttributeReading {
            asset_id: "a1".into(),
            attribute_name: "temp".into(),
            raw: AttributeValue(raw),
            device_timestamp_ms: 0,
        }
    }

    // ---- kind round-trip ----------------------------------------------------

    #[test]
    fn agent_kind_from_str_round_trip() {
        for k in [
            IotAgentKind::Mqtt,
            IotAgentKind::Http,
            IotAgentKind::WebSocket,
        ] {
            assert_eq!(IotAgentKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(IotAgentKind::from_str("MQTT"), Some(IotAgentKind::Mqtt));
        assert_eq!(IotAgentKind::from_str("ws"), Some(IotAgentKind::WebSocket));
        assert_eq!(IotAgentKind::from_str("coap"), None);
    }

    // ---- §2A.24 backoff curve: 1s..5min envelope, ±25% jitter --------------

    #[test]
    fn backoff_curve_hits_1s_to_5min_envelope() {
        // Deterministic seed -> reproducible curve.
        let mut sm = ReconnectStateMachine::new(0xDEAD_BEEF);
        let mut now = 0i64;

        // Walk many failed attempts; the un-jittered base is 1s,2s,4s,... capped
        // at 5min. Every realized delay must stay inside the [1s, 5min] envelope,
        // and within ±25% of the (capped) exponential base.
        let mut last_base = 0i64;
        for attempt in 1..=20u32 {
            sm.schedule_backoff(now);
            assert_eq!(sm.status(), ConnectionStatus::Waiting);
            let delay = sm.next_attempt_at_ms() - now;

            // hard envelope
            assert!(
                (RECONNECT_DELAY_INITIAL_MILLIS..=RECONNECT_DELAY_MAX_MILLIS).contains(&delay),
                "attempt {attempt}: delay {delay} out of [1000,300000]"
            );

            // ±25% of the capped base
            let exp = (attempt - 1).min(40);
            let base = ((RECONNECT_DELAY_INITIAL_MILLIS as i128) << exp)
                .min(RECONNECT_DELAY_MAX_MILLIS as i128) as i64;
            let lo = ((base as f64) * 0.75).floor() as i64 - 1;
            let hi = ((base as f64) * 1.25).ceil() as i64 + 1;
            // also respect the post-jitter envelope clamp
            let lo = lo.max(RECONNECT_DELAY_INITIAL_MILLIS);
            let hi = hi.min(RECONNECT_DELAY_MAX_MILLIS);
            assert!(
                delay >= lo && delay <= hi,
                "attempt {attempt}: delay {delay} not within ±25% of base {base} ([{lo},{hi}])"
            );

            // base is monotonically non-decreasing and caps at 5min
            assert!(base >= last_base);
            last_base = base;
            now += delay + 10;
        }
        // By attempt 20 the base must be pinned at the 5-minute cap.
        assert_eq!(last_base, RECONNECT_DELAY_MAX_MILLIS);
    }

    #[test]
    fn backoff_first_attempt_centers_on_one_second() {
        let mut sm = ReconnectStateMachine::new(7);
        sm.schedule_backoff(0);
        let delay = sm.next_attempt_at_ms();
        // first base is 1000ms; ±25% -> [750, 1250].
        assert!((750..=1250).contains(&delay), "first delay {delay}");
    }

    #[test]
    fn backoff_is_deterministic_for_a_fixed_seed() {
        let curve = |seed: u64| {
            let mut sm = ReconnectStateMachine::new(seed);
            let mut now = 0i64;
            let mut out = vec![];
            for _ in 0..8 {
                sm.schedule_backoff(now);
                let d = sm.next_attempt_at_ms() - now;
                out.push(d);
                now += d + 1;
            }
            out
        };
        assert_eq!(curve(42), curve(42), "same seed -> same curve");
        assert_ne!(
            curve(42),
            curve(43),
            "different seed -> different jitter draw"
        );
    }

    // ---- §2A.24 state-transition legality ----------------------------------

    #[test]
    fn state_machine_legal_transitions() {
        let mut sm = ReconnectStateMachine::new(1);
        assert_eq!(sm.status(), ConnectionStatus::Disconnected);

        // Disconnected -> Connecting
        assert!(sm.begin_connect());
        assert_eq!(sm.status(), ConnectionStatus::Connecting);
        // begin_connect again is illegal (not Disconnected) -> no-op
        assert!(!sm.begin_connect());

        // Connecting -> Connected
        assert!(sm.mark_connected());
        assert_eq!(sm.status(), ConnectionStatus::Connected);
        // mark_connected from Connected is illegal -> no-op
        assert!(!sm.mark_connected());

        // Connected -> Connecting on drop
        assert!(sm.on_disconnected());
        assert_eq!(sm.status(), ConnectionStatus::Connecting);
        // on_disconnected from Connecting is illegal -> no-op
        assert!(!sm.on_disconnected());

        // Connecting -> Waiting on failed attempt
        sm.schedule_backoff(1_000);
        assert_eq!(sm.status(), ConnectionStatus::Waiting);

        // Waiting -> Connecting only after now >= next_attempt_at_ms
        let due = sm.next_attempt_at_ms();
        assert!(!sm.poll_ready_to_retry(due - 1), "not yet due");
        assert_eq!(sm.status(), ConnectionStatus::Waiting);
        assert!(sm.poll_ready_to_retry(due), "due -> retry");
        assert_eq!(sm.status(), ConnectionStatus::Connecting);
    }

    #[test]
    fn begin_disconnect_blocks_retry_from_any_state() {
        // From Connected
        let mut sm = ReconnectStateMachine::new(1);
        sm.begin_connect();
        sm.mark_connected();
        sm.begin_disconnect();
        assert_eq!(sm.status(), ConnectionStatus::Disconnecting);
        assert!(sm.is_aborting());
        // poll_ready_to_retry must not resurrect a disconnecting machine
        assert!(!sm.poll_ready_to_retry(i64::MAX));
        assert_eq!(sm.status(), ConnectionStatus::Disconnecting);
        sm.mark_disconnected();
        assert_eq!(sm.status(), ConnectionStatus::Disconnected);
        assert!(sm.is_aborting());

        // From Waiting (mid-backoff): disconnect still wins.
        let mut sm2 = ReconnectStateMachine::new(2);
        sm2.begin_connect();
        sm2.schedule_backoff(0);
        assert_eq!(sm2.status(), ConnectionStatus::Waiting);
        sm2.begin_disconnect();
        assert_eq!(sm2.status(), ConnectionStatus::Disconnecting);
        assert!(!sm2.poll_ready_to_retry(i64::MAX));
    }

    #[test]
    fn mark_connected_resets_attempt_so_next_backoff_restarts_at_base() {
        let mut sm = ReconnectStateMachine::new(99);
        sm.begin_connect();
        // climb a few backoffs
        for _ in 0..5 {
            sm.schedule_backoff(0);
            sm.poll_ready_to_retry(i64::MAX); // Waiting -> Connecting
        }
        // recover
        assert!(sm.mark_connected());
        // drop again; the very next backoff base must be 1s again (attempt reset)
        assert!(sm.on_disconnected());
        sm.schedule_backoff(0);
        let d = sm.next_attempt_at_ms();
        assert!((750..=1250).contains(&d), "post-recovery base not 1s: {d}");
    }

    // ---- §2A.28 converter: @IGNORE / @NULL / *-negate ----------------------

    #[test]
    fn converter_ignore_null_and_wildcard_negate() {
        // @IGNORE drops the update
        let mut c = ConverterMap::new();
        c.insert("OFF".into(), json!("@IGNORE"));
        let link = link_with(Some(c), vec![]);
        let (ignore, _) = do_inbound_value_processing(&reading(json!("off")), &link, None);
        assert!(ignore, "@IGNORE must drop");

        // @NULL forwards a clear (not ignored)
        let mut c = ConverterMap::new();
        c.insert("CLEAR".into(), json!("@NULL"));
        let link = link_with(Some(c), vec![]);
        let (ignore, v) = do_inbound_value_processing(&reading(json!("clear")), &link, None);
        assert!(!ignore);
        assert!(v.is_null(), "@NULL forwards null");

        // direct mapping
        let mut c = ConverterMap::new();
        c.insert("ON".into(), json!(true));
        let link = link_with(Some(c), vec![]);
        let (ignore, v) =
            do_inbound_value_processing(&reading(json!("on")), &link, Some(ValueBaseType::Boolean));
        assert!(!ignore);
        assert_eq!(v.0, json!(true));

        // wildcard NEGATE on a number
        let mut c = ConverterMap::new();
        c.insert("*".into(), json!("@NEGATE"));
        let link = link_with(Some(c), vec![]);
        let (ignore, v) =
            do_inbound_value_processing(&reading(json!(5)), &link, Some(ValueBaseType::Number));
        assert!(!ignore);
        assert_eq!(v.as_numeric(), Some(-5.0));

        // wildcard NEGATE on a boolean
        let mut c = ConverterMap::new();
        c.insert("*".into(), json!("@NEGATE"));
        let link = link_with(Some(c), vec![]);
        let (ignore, v) =
            do_inbound_value_processing(&reading(json!(true)), &link, Some(ValueBaseType::Boolean));
        assert!(!ignore);
        assert_eq!(v.0, json!(false));

        // unmatched key, no wildcard -> ignore
        let mut c = ConverterMap::new();
        c.insert("ON".into(), json!(true));
        let link = link_with(Some(c), vec![]);
        let (ignore, _) = do_inbound_value_processing(&reading(json!("zzz")), &link, None);
        assert!(ignore, "unmatched + no wildcard -> ignore");
    }

    #[test]
    fn inbound_coercion_failure_drops_not_errors() {
        // No converter; declared Number but device sent non-numeric text.
        let link = link_with(None, vec![]);
        let (ignore, _) = do_inbound_value_processing(
            &reading(json!("not-a-number")),
            &link,
            Some(ValueBaseType::Number),
        );
        assert!(
            ignore,
            "coercion failure must DROP (§2A.5), not panic/error"
        );

        // Coercible string -> Number succeeds.
        let (ignore, v) = do_inbound_value_processing(
            &reading(json!("23.5")),
            &link,
            Some(ValueBaseType::Number),
        );
        assert!(!ignore);
        assert_eq!(v.as_numeric(), Some(23.5));
    }

    // ---- §2A.28 inbound filter chain ---------------------------------------

    #[test]
    fn inbound_filter_chain_substring_then_replace() {
        let filters = vec![
            ValueFilter::Substring {
                begin: 5,
                end: Some(9),
            },
            ValueFilter::Replace {
                pattern: "C".into(),
                replacement: "".into(),
            },
        ];
        let link = link_with(None, filters);
        // "temp=23.5C" -> substring[5..9] = "23.5" -> replace "C" -> "23.5"
        let (ignore, v) = do_inbound_value_processing(
            &reading(json!("temp=23.5C")),
            &link,
            Some(ValueBaseType::Number),
        );
        assert!(!ignore);
        assert_eq!(v.as_numeric(), Some(23.5));
    }

    // ---- §2A.28 %VALUE% / %TIME% placeholder substitution ------------------

    #[test]
    fn outbound_value_placeholder_substitution() {
        let link = AgentLink {
            asset_id: "a1".into(),
            attribute_name: "setpoint".into(),
            write_value: Some("set:%VALUE%".into()),
            ..AgentLink::default()
        };
        let out = do_outbound_value_processing(&AttributeValue(json!(21.5)), &link, 0)
            .unwrap()
            .unwrap();
        assert_eq!(out.0, json!("set:21.5"));
    }

    #[test]
    fn outbound_time_placeholder_uses_injected_clock() {
        // Fixed injected clock -> exact rendered string (no wall clock).
        let now = 1_700_000_000_000i64; // 2023-11-14T22:13:20.000Z
        let link = AgentLink {
            asset_id: "a1".into(),
            attribute_name: "x".into(),
            write_value: Some("t=%TIME% s=%TIME:EPOCH_SECONDS% m=%TIME:EPOCH_MILLIS%".into()),
            ..AgentLink::default()
        };
        let out = do_outbound_value_processing(&AttributeValue(json!(1)), &link, now)
            .unwrap()
            .unwrap();
        assert_eq!(
            out.0,
            json!("t=2023-11-14T22:13:20.000Z s=1700000000 m=1700000000000")
        );
    }

    #[test]
    fn outbound_value_and_time_combined() {
        let now = 1_700_000_000_000i64;
        let link = AgentLink {
            asset_id: "a1".into(),
            attribute_name: "x".into(),
            write_value: Some("v=%VALUE%@%TIME:EPOCH_MILLIS%".into()),
            ..AgentLink::default()
        };
        let out = do_outbound_value_processing(&AttributeValue(json!("ON")), &link, now)
            .unwrap()
            .unwrap();
        assert_eq!(out.0, json!("v=ON@1700000000000"));
    }

    #[test]
    fn outbound_no_template_passes_converted_value_through() {
        let link = AgentLink {
            asset_id: "a1".into(),
            attribute_name: "x".into(),
            write_value: None,
            ..AgentLink::default()
        };
        let out = do_outbound_value_processing(&AttributeValue(json!(42)), &link, 0)
            .unwrap()
            .unwrap();
        assert_eq!(out.0, json!(42));
    }

    #[test]
    fn outbound_write_converter_ignore_drops_send() {
        let mut c = ConverterMap::new();
        c.insert("STOP".into(), json!("@IGNORE"));
        let link = AgentLink {
            asset_id: "a1".into(),
            attribute_name: "x".into(),
            write_value_converter: Some(c),
            ..AgentLink::default()
        };
        let out = do_outbound_value_processing(&AttributeValue(json!("stop")), &link, 0).unwrap();
        assert!(out.is_none(), "@IGNORE on write -> no device send (§2A.28)");
    }

    #[test]
    fn has_dynamic_placeholders_detects_markers() {
        assert!(has_dynamic_placeholders("x %VALUE% y"));
        assert!(has_dynamic_placeholders("t=%TIME:EPOCH_MILLIS%"));
        assert!(!has_dynamic_placeholders("static payload"));
    }
}
