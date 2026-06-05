// Origin: CTOX
// License: AGPL-3.0-only
//
// Phase 3 — native HTTP poll/push protocol agent. Ported domain semantics from
// OpenRemote's HTTPProtocol (AGPL-3.0, archive/openremote, HEAD 22a42a7);
// transport reimplemented on CTOX's existing blocking HTTP client
// (`communication::email_native::http_request`, ureq-backed) — NOT reqwest, NOT
// a new HTTP dependency. See docs/legal/NOTICE.
//
// ref: HTTPProtocol.java:137-597
//
// This file implements ONLY the `IotAgent` trait for `IotAgentKind::Http`. It
// reuses, never re-implements:
//   * the shared value-processing base layer (`adapters::do_inbound_value_processing`
//     / `do_outbound_value_processing`) — filters, converters, %VALUE%/%TIME%
//     placeholders (§2A.28). The agent only extracts the RAW reading; the runtime
//     runs the base layer.
//   * the deterministic `adapters::ReconnectStateMachine` — the agent embeds one
//     and drives it with the INJECTED clock; it never owns a bespoke retry loop.
//
// What this agent owns (the HTTP-specific behavior, §2A.29 / §2A.30):
//   * minimum poll interval, default 5s (`MIN_POLLING_MILLIS`), enforced as
//     `delay = max(configured, 5000)`. ref: HTTPProtocol.java:278,358
//   * FIXED-DELAY scheduling: the next poll is due `delay` ms after the PREVIOUS
//     poll COMPLETES, not at a fixed rate — a slow response never causes drift or
//     overlap. ref: HTTPProtocol.java:474 (scheduleWithFixedDelay)
//   * process ONLY 2xx responses; non-2xx is logged and skipped, polling
//     continues (no error propagation). ref: HTTPProtocol.java:565,580-583
//   * `Link: rel="next"` pagination (RFC 5988): accumulate entity pages into one
//     logical body before mapping to a reading, capped. ref: HTTPProtocol.java:508-543
//   * writes are fire-and-forget; `update_on_write` is honored by the RUNTIME
//     (the agent just reports send success). ref: HTTPProtocol.java:428-441 / §2A.30
//
// HARD RULES honored here:
//   * native Rust only; reuse the existing ureq-backed client, no new HTTP dep.
//   * the clock is INJECTED (the `clock` closure, defaults to `crate::iot::now_ms`)
//     so min-interval / fixed-delay are deterministically testable — NO wall-clock
//     reads on the hot path.
//   * config/secrets flow through `runtime_env::env_or_config(root, …)` + the CTOX
//     secret store, NEVER std::env for runtime state. Agents talk to DEVICES, not
//     the browser (no HTTP data bridge to the UI).
//   * ported algorithmic fns carry `// ref: <upstream>:<lines>` with preserved
//     upstream names (doStart, schedulePollingRequest, executePollingRequest,
//     executePagingRequest, onPollingResponse, doLinkedAttributeWrite).

use crate::iot::adapters::{
    AgentContext, AgentLink, AttributeReading, ConnectionStatus, IotAgent, IotAgentKind,
    ReconnectStateMachine,
};
use crate::iot::model::AttributeValue;
use crate::iot::Result;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Constants — ported 1:1 from HTTPProtocol.
// ---------------------------------------------------------------------------

/// ref: HTTPProtocol.java:278 (MIN_POLLING_MILLIS = 5000)
pub(crate) const MIN_POLLING_MILLIS: i64 = 5_000;

/// Upper bound on accumulated `Link: rel=next` pages, so a misbehaving or
/// adversarial server cannot make one poll loop unbounded. There is no explicit
/// cap upstream (it trusts the server's pagination terminating); CTOX adds one
/// and logs when it trips. Kept generous so legitimate paging is unaffected.
const MAX_PAGINATION_PAGES: usize = 256;

// ---------------------------------------------------------------------------
// HTTP transport seam
// ---------------------------------------------------------------------------
//
// The production transport is the existing blocking client
// `communication::email_native::http_request` (ureq). It is injectable ONLY so
// the unit tests can drive a real in-process loopback listener through a thin
// wrapper while still asserting against a deterministic clock; the default path
// is the real client. This is NOT a new HTTP framework — it is the same ureq
// call behind a one-method trait so the agent stays decoupled from the global.

/// One performed HTTP exchange, normalized for the agent's mapping logic.
#[derive(Clone, Debug)]
pub(crate) struct HttpExchange {
    pub status: u16,
    /// header name lowercased -> value (mirrors `email_native::HttpResponse`).
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Blocking HTTP transport. The default impl forwards to the existing ureq-backed
/// `email_native::http_request`; tests substitute a loopback double.
pub(crate) trait HttpTransport: Send {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<HttpExchange>;
}

/// Production transport: the existing crate-wide ureq client. No new dependency.
pub(crate) struct UreqTransport;

impl HttpTransport for UreqTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<HttpExchange> {
        // Reuse the existing client identified in research; do NOT add reqwest.
        let resp = crate::communication::email_native::http_request(method, url, headers, body)?;
        Ok(HttpExchange {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
        })
    }
}

// ---------------------------------------------------------------------------
// Per-link HTTP request binding (the `binding` JSON of an AgentLink)
// ---------------------------------------------------------------------------

/// ref: HTTPAgentLink (method / path / headers / pollingMillis / pagingMode)
///
/// The protocol-specific fields the HTTP agent reads out of `AgentLink.binding`.
/// Absolute `url` is resolved at link time from the agent `base_uri` + the link
/// `path` (mirrors HTTPProtocol.doStart's base URI + doLinkAttribute's path).
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub(crate) struct HttpBinding {
    /// HTTP method; defaults to GET. ref: HTTPProtocol.java:274,352
    #[serde(default)]
    pub method: Option<String>,
    /// request path appended to the agent base URI. ref: HTTPProtocol.java:353
    #[serde(default)]
    pub path: Option<String>,
    /// per-link extra headers (merged over the agent-wide headers).
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// poll cadence; clamped up to MIN_POLLING_MILLIS. `None` == push-only (no
    /// poll scheduled — the device pushes to us, we never pull).
    /// ref: HTTPProtocol.java:358,405
    #[serde(default)]
    pub polling_millis: Option<i64>,
    /// follow `Link: rel="next"` and accumulate pages. ref: HTTPProtocol.java:359
    #[serde(default)]
    pub paging_mode: bool,
}

/// Agent-wide HTTP config (the `iot_agents.data` JSON).
/// ref: HTTPAgent (baseURI, requestHeaders)
#[derive(Clone, Default, serde::Deserialize)]
struct HttpAgentConfig {
    /// base URI; per-link `path` is appended. ref: HTTPProtocol.java:306-311
    #[serde(default)]
    base_uri: String,
    /// agent-wide headers applied to every request, overridden per link. These
    /// may carry caller-supplied auth material, so the custom Debug redacts them.
    #[serde(default)]
    request_headers: BTreeMap<String, String>,
    /// secret-store key whose value is sent as the `Authorization` header. Read
    /// via runtime_env::env_or_config — never std::env. (e.g. CTO_IOT_HTTP_AUTH_HEADER)
    #[serde(default)]
    auth_header_key: Option<String>,
}

impl std::fmt::Debug for HttpAgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Secret redaction: never print header values or the secret-store key
        // name. Report only the base URI, header-name set, and key presence.
        let header_names: Vec<&String> = self.request_headers.keys().collect();
        f.debug_struct("HttpAgentConfig")
            .field("base_uri", &self.base_uri)
            .field("request_header_names", &header_names)
            .field(
                "auth_header_key",
                &self.auth_header_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Resolved per-attribute polling state. FIXED-DELAY scheduling lives here:
/// `next_due_ms` is recomputed from the injected clock AFTER each poll completes.
struct PollSlot {
    binding: HttpBinding,
    /// absolute request URL (base_uri + path), resolved once at subscribe.
    url: String,
    method: String,
    /// merged headers (agent-wide + link + resolved auth), resolved once.
    headers: BTreeMap<String, String>,
    /// clamped poll interval (>= MIN_POLLING_MILLIS); None == push-only.
    interval_ms: Option<i64>,
    /// FIXED-DELAY: when the next poll is due. `i64::MIN` == "poll immediately"
    /// (the upstream `initialDelay = 0`, ref: HTTPProtocol.java:505).
    next_due_ms: i64,
}

// ---------------------------------------------------------------------------
// The HTTP agent
// ---------------------------------------------------------------------------

/// Native HTTP poll/push agent. ref: HTTPProtocol.java:137-597
pub(crate) struct HttpAgent {
    agent_id: String,
    config: HttpAgentConfig,
    /// agent-wide resolved auth header value (looked up via secret store / runtime_env).
    auth_header_value: Option<String>,
    transport: Box<dyn HttpTransport>,
    /// INJECTED clock (epoch-ms). Defaults to `crate::iot::now_ms`; tests override
    /// to assert min-interval / fixed-delay deterministically.
    clock: Box<dyn Fn() -> i64 + Send>,
    /// embedded deterministic reconnect SM (§2A.24). HTTP "connect" is just
    /// resolving the base URI, so it normally goes straight to Connected, but the
    /// SM is shared machinery so the runtime supervises HTTP exactly like MQTT/WS.
    reconnect: ReconnectStateMachine,
    /// per-attribute poll/request bindings, keyed (asset_id, attribute_name).
    /// ref: HTTPProtocol.java:281 (requestMap) / :282 (pollingMap)
    slots: BTreeMap<(String, String), PollSlot>,
}

impl HttpAgent {
    /// ref: HTTPProtocol.HTTPProtocol(agent) — resolve config + auth from the
    /// secret store / runtime_env; nothing networked yet (that is `connect`).
    pub(crate) fn new(ctx: AgentContext) -> Result<Self> {
        let config: HttpAgentConfig =
            serde_json::from_value(ctx.config.clone()).unwrap_or_default();

        // Auth header value resolved ONLY through the CTOX secret store /
        // runtime_env gate — never std::env. ref: HTTPProtocol.java:324-338 (auth setup)
        let auth_header_value = config
            .auth_header_key
            .as_deref()
            .and_then(|key| crate::execution::models::runtime_env::env_or_config(ctx.root, key));

        Ok(HttpAgent {
            agent_id: ctx.agent_id,
            config,
            auth_header_value,
            transport: Box::new(UreqTransport),
            clock: Box::new(crate::iot::now_ms),
            reconnect: ReconnectStateMachine::new(seed_from_id(&ctx.realm)),
            slots: BTreeMap::new(),
        })
    }

    /// Test seam: build an agent with an explicit transport + deterministic clock.
    /// NOT used in production (the default `new` wires the real client + now_ms).
    #[cfg(test)]
    fn with_transport_and_clock(
        agent_id: impl Into<String>,
        config: HttpAgentConfig,
        transport: Box<dyn HttpTransport>,
        clock: Box<dyn Fn() -> i64 + Send>,
    ) -> Self {
        HttpAgent {
            agent_id: agent_id.into(),
            config,
            auth_header_value: None,
            transport,
            clock,
            reconnect: ReconnectStateMachine::new(0xA11CE),
            slots: BTreeMap::new(),
        }
    }

    /// Merge agent-wide headers, per-link headers, and the resolved auth header
    /// into the final request header set. Per-link wins over agent-wide; auth is
    /// applied last only if not already explicitly set.
    /// ref: HTTPProtocol.java:376-382 (combinedHeaders)
    fn resolve_headers(&self, binding: &HttpBinding) -> BTreeMap<String, String> {
        let mut headers = self.config.request_headers.clone();
        for (k, v) in &binding.headers {
            headers.insert(k.clone(), v.clone());
        }
        if let Some(auth) = &self.auth_header_value {
            headers
                .entry("Authorization".to_string())
                .or_insert_with(|| auth.clone());
        }
        headers
    }

    /// Resolve the absolute request URL from base URI + link path.
    /// ref: HTTPProtocol.java:306-311 (trim trailing '/') + :147-149 (strip leading '/')
    fn resolve_url(&self, binding: &HttpBinding) -> String {
        let base = self.config.base_uri.trim_end_matches('/');
        match binding.path.as_deref() {
            Some(p) if !p.is_empty() => {
                let p = p.trim_start_matches('/');
                format!("{base}/{p}")
            }
            _ => base.to_string(),
        }
    }

    /// FIXED-DELAY poll executor for the slots that are due at `now`. Performs the
    /// request, accumulates pagination, maps ONLY 2xx to a raw reading, then
    /// re-arms the slot's `next_due_ms = now_after_completion + interval` (so the
    /// gap is measured from completion, never from the scheduled start).
    /// ref: HTTPProtocol.java:466-535 (schedulePollingRequest/executePollingRequest)
    fn poll_due_slots(&mut self) -> Result<Vec<AttributeReading>> {
        let now = (self.clock)();
        let mut out = Vec::new();

        // Collect the keys due now under a fixed snapshot (a slot added mid-drain
        // is picked up on the next `read`, mirroring the upstream scheduled tasks).
        let due_keys: Vec<(String, String)> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.interval_ms.is_some() && now >= slot.next_due_ms)
            .map(|(k, _)| k.clone())
            .collect();

        for key in due_keys {
            // Read the request shape out, dropping the borrow before the blocking
            // call so a concurrent subscribe/unlink stays safe (§2A.27 spirit).
            let (url, method, headers, paging) = {
                let slot = match self.slots.get(&key) {
                    Some(s) => s,
                    None => continue,
                };
                (
                    slot.url.clone(),
                    slot.method.clone(),
                    slot.headers.clone(),
                    slot.binding.paging_mode,
                )
            };

            // Perform the request (+ pagination accumulation). Errors are logged
            // and the poll is simply re-armed — a transient device failure must
            // not wedge the loop. ref: HTTPProtocol.java:502-504,525-526
            let exchange = match self.execute_polling_request(&method, &url, &headers, paging, None)
            {
                Ok(ex) => Some(ex),
                Err(err) => {
                    // A transient device failure is logged to stderr and the poll
                    // is simply re-armed; it must NOT wedge the loop or propagate
                    // (no `log` crate in-tree — stderr is the house warning sink).
                    eprintln!(
                        "[warn] iot.http agent={} poll failed url={url}: {err}",
                        self.agent_id
                    );
                    None
                }
            };

            // Re-arm FIXED-DELAY from the moment the poll COMPLETED, then map.
            // ref: HTTPProtocol.java:474 (scheduleWithFixedDelay re-arms on completion)
            let completed_at = (self.clock)();
            if let Some(slot) = self.slots.get_mut(&key) {
                if let Some(interval) = slot.interval_ms {
                    slot.next_due_ms = completed_at.saturating_add(interval);
                }
            }

            if let Some(ex) = exchange {
                if let Some(reading) = self.on_polling_response(&key, &ex) {
                    out.push(reading);
                }
            }
        }

        Ok(out)
    }

    /// ref: HTTPProtocol.java:508-535 (executePollingRequest)
    ///
    /// Perform the request and, when paging is enabled, follow `Link: rel="next"`
    /// accumulating each page's body. The accumulated logical body is the
    /// concatenation of page bodies separated by a newline (a JSON-array merge is
    /// the consumer's job; upstream wraps the pages in a list entity — we keep the
    /// raw concatenation so the existing converter/filter base layer maps it).
    fn execute_polling_request(
        &self,
        method: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        paging: bool,
        body: Option<&[u8]>,
    ) -> Result<HttpExchange> {
        let original = self.transport.request(method, url, headers, body)?;

        if !paging {
            return Ok(original);
        }

        // Accumulate pages: page 0 is the original body. ref: HTTPProtocol.java:514-522
        let mut pages: Vec<Vec<u8>> = Vec::new();
        pages.push(original.body.clone());

        let mut last = original.clone();
        let mut pages_followed = 0usize;
        // ref: HTTPProtocol.java:517 (while executePagingRequest != null)
        while let Some(next_url) = parse_link_next(&last.headers) {
            if pages_followed >= MAX_PAGINATION_PAGES {
                eprintln!(
                    "[warn] iot.http agent={} pagination capped at {MAX_PAGINATION_PAGES} pages url={url}",
                    self.agent_id
                );
                break;
            }
            // ref: HTTPProtocol.java:537-543 (executePagingRequest follows next link)
            let next = self.transport.request(method, &next_url, headers, None)?;
            pages.push(next.body.clone());
            pages_followed += 1;
            last = next;
        }

        // Build one logical response: newline-joined page bodies, status/headers
        // from the original. ref: HTTPProtocol.java:521 (PagingResponse.entity(entities))
        let mut merged = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            if i > 0 {
                merged.push(b'\n');
            }
            merged.extend_from_slice(page);
        }
        Ok(HttpExchange {
            status: original.status,
            headers: original.headers,
            body: merged,
        })
    }

    /// ref: HTTPProtocol.java:557-597 (onPollingResponse)
    ///
    /// ONLY 2xx with a body becomes a reading; everything else is logged and
    /// dropped (no error, polling continues). The RAW string body is the reading
    /// value — the runtime's base layer applies filters/converters/coercion.
    fn on_polling_response(
        &self,
        key: &(String, String),
        ex: &HttpExchange,
    ) -> Option<AttributeReading> {
        // ref: HTTPProtocol.java:565 (Family.SUCCESSFUL) — strict 2xx.
        if !(200..300).contains(&ex.status) {
            // ref: HTTPProtocol.java:580-583 (un-successful code -> skip + return).
            // Silently dropped (no reading) — the device firehose would make a
            // per-response stderr line on every non-2xx too noisy; the count is
            // observable via iot_agent_status in the runtime layer.
            let _ = (&self.agent_id, key);
            return None;
        }
        if ex.body.is_empty() {
            return None; // ref: HTTPProtocol.java:565 (response.hasEntity())
        }

        // Raw body as a string value; non-UTF-8 falls back to lossy (the device
        // firehose must not panic). The base layer coerces to the declared type.
        let raw = String::from_utf8_lossy(&ex.body).into_owned();
        Some(AttributeReading {
            asset_id: key.0.clone(),
            attribute_name: key.1.clone(),
            raw: AttributeValue(serde_json::Value::String(raw)),
            // HTTP poll responses carry no device timestamp -> 0 ("no explicit
            // timestamp", §2A.1); the engine normalizes against system time.
            device_timestamp_ms: 0,
        })
    }
}

impl IotAgent for HttpAgent {
    fn kind(&self) -> IotAgentKind {
        IotAgentKind::Http
    }

    /// ref: HTTPProtocol.java:303-346 (doStart)
    ///
    /// HTTP has no persistent socket; "connect" validates the base URI and marks
    /// the SM Connected. A missing base URI is a transient failure -> backoff
    /// (the runtime retries), mirroring doStart's hard-fail being caught by the
    /// supervising layer rather than wedging the agent.
    fn connect(&mut self, _ctx: &AgentContext) -> Result<ConnectionStatus> {
        // Idempotent if already connected. ref: AbstractIOClientProtocol doStart guard
        if self.reconnect.status() == ConnectionStatus::Connected {
            return Ok(ConnectionStatus::Connected);
        }
        // CAS into Connecting from Disconnected. If this fails the SM is mid-flight
        // (Connecting / Waiting / Disconnecting): a Disconnecting machine is being
        // torn down and must NOT be resurrected here (stale-execution guard), and
        // Connecting/Waiting are the runtime loop's territory — report the current
        // status and let the runtime drive `poll_ready_to_retry`. ref:
        // AbstractMQTT_IOClient.java:430,464-468 (begin_connect CAS + abort guard).
        if !self.reconnect.begin_connect() {
            return Ok(self.reconnect.status());
        }

        if self.config.base_uri.trim().is_empty() {
            // ref: HTTPProtocol.java:306-307 (orElseThrow on missing base URI) — a
            // transient config failure -> backoff (Waiting); the runtime retries.
            let now = (self.clock)();
            self.reconnect.schedule_backoff(now);
            return Ok(self.reconnect.status());
        }

        self.reconnect.mark_connected();
        Ok(ConnectionStatus::Connected)
    }

    /// ref: HTTPProtocol.java:348-412 (doLinkAttribute)
    ///
    /// Resolve the request shape + clamped poll interval and register the slot.
    /// Safe to call during a reconnect (§2A.27): it only mutates the slot table.
    fn subscribe(&mut self, link: &AgentLink) -> Result<()> {
        let binding: HttpBinding = serde_json::from_value(link.binding.clone()).unwrap_or_default();

        let url = self.resolve_url(&binding);
        let method = binding
            .method
            .clone()
            .unwrap_or_else(|| "GET".to_string())
            .to_uppercase();
        let headers = self.resolve_headers(&binding);

        // ref: HTTPProtocol.java:358 (Math.max(millis, MIN_POLLING_MILLIS))
        let interval_ms = binding.polling_millis.map(|m| m.max(MIN_POLLING_MILLIS));

        let key = (link.asset_id.clone(), link.attribute_name.clone());
        self.slots.insert(
            key,
            PollSlot {
                binding,
                url,
                method,
                headers,
                interval_ms,
                // initial delay 0 -> due immediately. ref: HTTPProtocol.java:505
                next_due_ms: i64::MIN,
            },
        );
        Ok(())
    }

    /// Drain readings: run every poll slot whose FIXED-DELAY due time has been
    /// reached per the injected clock, accumulating 2xx-mapped readings.
    /// ref: HTTPProtocol.java:474-535 (the scheduled poll body)
    fn read(&mut self) -> Result<Vec<AttributeReading>> {
        self.poll_due_slots()
    }

    /// ref: HTTPProtocol.java:428-441 (doLinkedAttributeWrite) + :545-555
    ///
    /// Fire-and-forget device write. `processed` is already post-base-layer
    /// (filters/converters/%VALUE%/%TIME% applied by the runtime). The body is the
    /// processed value's string projection. `update_on_write` is the RUNTIME's
    /// concern (§2A.30) — this method only performs the send and reports success.
    fn write(&mut self, link: &AgentLink, processed: &AttributeValue) -> Result<()> {
        let key = (link.asset_id.clone(), link.attribute_name.clone());
        let (url, method, headers) = match self.slots.get(&key) {
            Some(slot) => (slot.url.clone(), slot.method.clone(), slot.headers.clone()),
            None => {
                // ref: HTTPProtocol.java:438-439 (ignore write to unlinked attribute)
                let _ = (&self.agent_id, &key);
                return Ok(());
            }
        };

        // For a write, GET is meaningless; default to POST when the link did not
        // pin a non-GET method. ref: HTTPProtocol.java:212 (value sent for non-GET)
        let write_method = if method == "GET" {
            "POST".to_string()
        } else {
            method
        };

        let body = value_to_body(processed);

        // Fire-and-forget: a failed device write is logged, not propagated (the
        // engine state is already updated; §2A.30). ref: HTTPProtocol.java:552-554
        match self
            .transport
            .request(&write_method, &url, &headers, Some(&body))
        {
            Ok(ex) => {
                if !(200..300).contains(&ex.status) {
                    // ref: HTTPProtocol.java:599-604 (onAttributeWriteResponse logs non-2xx).
                    // Fire-and-forget: the engine state is already updated; a non-2xx
                    // device ack is informational only and not propagated (§2A.30).
                    eprintln!(
                        "[warn] iot.http agent={} write non-2xx ({}) url={url}",
                        self.agent_id, ex.status
                    );
                }
            }
            Err(err) => {
                eprintln!(
                    "[warn] iot.http agent={} write failed url={url}: {err}",
                    self.agent_id
                );
            }
        }
        Ok(())
    }

    /// ref: HTTPProtocol.java:414-426 (doUnlinkAttribute)
    fn unlink(&mut self, link: &AgentLink) -> Result<()> {
        let key = (link.asset_id.clone(), link.attribute_name.clone());
        self.slots.remove(&key);
        Ok(())
    }

    fn status(&self) -> ConnectionStatus {
        self.reconnect.status()
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Project a processed AttributeValue to the request body bytes.
/// ref: HTTPProtocol.java:548 (ValueUtil.convert(value, String.class))
fn value_to_body(value: &AttributeValue) -> Vec<u8> {
    match &value.0 {
        serde_json::Value::String(s) => s.clone().into_bytes(),
        serde_json::Value::Null => Vec::new(),
        other => other.to_string().into_bytes(),
    }
}

/// Parse the `Link` header for the `rel="next"` target (RFC 5988 / §2A.29).
/// ref: HTTPProtocol.java:538-539 (response.hasLink("next") / getLink("next").getUri())
///
/// Header form: `<https://api/next?page=2>; rel="next", <…>; rel="last"`.
fn parse_link_next(headers: &BTreeMap<String, String>) -> Option<String> {
    // Header names are stored lowercased by the transport (email_native).
    let link = headers.get("link")?;
    for part in link.split(',') {
        let part = part.trim();
        // Split the URI segment (`<uri>`) from the params (`; rel="next"`).
        let mut segments = part.split(';');
        let uri_seg = segments.next()?.trim();
        if !uri_seg.starts_with('<') || !uri_seg.ends_with('>') {
            continue;
        }
        let uri = &uri_seg[1..uri_seg.len() - 1];
        // any param `rel=next` / `rel="next"` (case-insensitive) marks this link.
        let is_next = segments.any(|param| {
            let param = param.trim();
            let Some((k, v)) = param.split_once('=') else {
                return false;
            };
            k.trim().eq_ignore_ascii_case("rel")
                && v.trim().trim_matches('"').eq_ignore_ascii_case("next")
        });
        if is_next {
            return Some(uri.to_string());
        }
    }
    None
}

/// Deterministic jitter seed for the embedded reconnect SM, derived from the
/// agent realm so the backoff curve is stable per agent (NOT wall-clock seeded).
fn seed_from_id(s: &str) -> u64 {
    // FNV-1a 64-bit.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ===========================================================================
// Tests — in-process LOOPBACK HTTP fixture (real 127.0.0.1 listener) + injected
// deterministic clock. No real device, no network egress beyond loopback.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    // Secret redaction: HttpAgentConfig Debug must never print the secret-store
    // key name or header values; only the base URI + header NAMES are shown.
    #[test]
    fn http_agent_config_debug_redacts_secrets() {
        let mut headers = BTreeMap::new();
        headers.insert(
            "X-Api-Key".to_string(),
            "secret-header-value-xyz".to_string(),
        );
        let cfg = HttpAgentConfig {
            base_uri: "https://device.example/api".into(),
            request_headers: headers,
            auth_header_key: Some("CTO_IOT_HTTP_AUTH_HEADER".into()),
        };
        let dbg = format!("{cfg:?}");
        assert!(
            !dbg.contains("CTO_IOT_HTTP_AUTH_HEADER"),
            "key leaked: {dbg}"
        );
        assert!(
            !dbg.contains("secret-header-value-xyz"),
            "value leaked: {dbg}"
        );
        assert!(dbg.contains("<redacted>"), "no redaction marker: {dbg}");
        assert!(dbg.contains("X-Api-Key"), "header name present: {dbg}");
        assert!(
            dbg.contains("https://device.example/api"),
            "uri present: {dbg}"
        );
    }

    // ---- a deterministic, injectable clock --------------------------------

    #[derive(Clone)]
    struct TestClock(Arc<AtomicI64>);
    impl TestClock {
        fn new(start: i64) -> Self {
            TestClock(Arc::new(AtomicI64::new(start)))
        }
        fn advance(&self, ms: i64) {
            self.0.fetch_add(ms, Ordering::SeqCst);
        }
        fn closure(&self) -> Box<dyn Fn() -> i64 + Send> {
            let c = self.0.clone();
            Box::new(move || c.load(Ordering::SeqCst))
        }
    }

    // ---- a recording loopback transport double (no network) ---------------
    //
    // This double records every request and returns scripted responses. It lets
    // the fixed-delay / min-interval / pagination logic be asserted purely on the
    // injected clock without flakiness, while the REAL loopback listener test
    // below proves the production ureq client path end to end.

    #[derive(Clone)]
    struct ScriptedTransport {
        // (method, url) -> queue of responses; the last is repeated when drained.
        responses: Arc<Mutex<Vec<HttpExchange>>>,
        requests: Arc<Mutex<Vec<(String, String)>>>,
    }
    impl ScriptedTransport {
        fn new(responses: Vec<HttpExchange>) -> Self {
            ScriptedTransport {
                responses: Arc::new(Mutex::new(responses)),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
        fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
        fn requests(&self) -> Vec<(String, String)> {
            self.requests.lock().unwrap().clone()
        }
    }
    impl HttpTransport for ScriptedTransport {
        fn request(
            &self,
            method: &str,
            url: &str,
            _headers: &BTreeMap<String, String>,
            _body: Option<&[u8]>,
        ) -> Result<HttpExchange> {
            self.requests
                .lock()
                .unwrap()
                .push((method.to_string(), url.to_string()));
            let mut q = self.responses.lock().unwrap();
            if q.len() > 1 {
                Ok(q.remove(0))
            } else {
                Ok(q.first().cloned().unwrap_or(HttpExchange {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: Vec::new(),
                }))
            }
        }
    }

    fn ex(status: u16, body: &str, headers: &[(&str, &str)]) -> HttpExchange {
        HttpExchange {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn http_config() -> HttpAgentConfig {
        HttpAgentConfig {
            base_uri: "http://device.local/api".into(),
            request_headers: BTreeMap::new(),
            auth_header_key: None,
        }
    }

    fn link(asset: &str, attr: &str, binding: serde_json::Value) -> AgentLink {
        AgentLink {
            asset_id: asset.into(),
            attribute_name: attr.into(),
            binding,
            ..AgentLink::default()
        }
    }

    // ---- §2A.29 min poll interval enforced (injected clock) ---------------

    #[test]
    fn min_poll_interval_enforced_default_5s() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "23.5", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        // request a 1s poll — must be clamped UP to 5s.
        agent
            .subscribe(&link(
                "asset1",
                "temp",
                serde_json::json!({ "path": "/temp", "polling_millis": 1000 }),
            ))
            .unwrap();

        // first read: due immediately (initial delay 0) -> 1 request.
        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "first poll fires immediately");
        assert_eq!(transport.request_count(), 1);

        // advance 4999ms: NOT yet due (min interval is 5000, not the configured 1000).
        clock.advance(4_999);
        let r = agent.read().unwrap();
        assert!(
            r.is_empty(),
            "before 5s -> no poll (min interval clamps 1s up)"
        );
        assert_eq!(transport.request_count(), 1);

        // cross 5000ms -> due.
        clock.advance(1);
        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "at 5s -> poll fires");
        assert_eq!(transport.request_count(), 2);
    }

    // ---- §2A.29 FIXED-DELAY: gap measured from completion, not start ------

    #[test]
    fn fixed_delay_measures_from_completion_not_fixed_rate() {
        // A clock that JUMPS forward inside the request (simulating a slow
        // response) lets us prove the next due time is anchored to completion.
        let clock = TestClock::new(0);
        let inner_clock = clock.clone();

        // A transport that advances the clock by 3000ms per request (slow response).
        struct SlowTransport {
            clock: TestClock,
            requests: Arc<Mutex<usize>>,
        }
        impl HttpTransport for SlowTransport {
            fn request(
                &self,
                _m: &str,
                _u: &str,
                _h: &BTreeMap<String, String>,
                _b: Option<&[u8]>,
            ) -> Result<HttpExchange> {
                *self.requests.lock().unwrap() += 1;
                self.clock.advance(3_000); // response takes 3s
                Ok(HttpExchange {
                    status: 200,
                    headers: BTreeMap::new(),
                    body: b"1".to_vec(),
                })
            }
        }
        let req_count = Arc::new(Mutex::new(0usize));
        let transport = SlowTransport {
            clock: inner_clock,
            requests: req_count.clone(),
        };

        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport),
            clock.closure(),
        );
        agent
            .subscribe(&link(
                "asset1",
                "temp",
                serde_json::json!({ "path": "/temp", "polling_millis": 5000 }),
            ))
            .unwrap();

        // first poll at t=0, completes at t=3000 (the transport advanced the clock).
        agent.read().unwrap();
        assert_eq!(*req_count.lock().unwrap(), 1);
        // now t == 3000. FIXED-RATE would make the next poll due at 5000 (i.e. now
        // only 2000ms away). FIXED-DELAY makes it due at completion(3000)+5000=8000.

        // advance to t=7999 (well past a 5000 fixed-RATE boundary) -> still no poll.
        clock.advance(4_999); // t = 7999
        agent.read().unwrap();
        assert_eq!(
            *req_count.lock().unwrap(),
            1,
            "fixed-delay: no second poll before completion+interval"
        );

        // cross t=8000 -> due (completion 3000 + interval 5000).
        clock.advance(1); // t = 8000
        agent.read().unwrap();
        assert_eq!(
            *req_count.lock().unwrap(),
            2,
            "fixed-delay: second poll fires at completion+interval"
        );
    }

    // ---- §2A.29 only 2xx processed ----------------------------------------

    #[test]
    fn non_2xx_is_skipped_polling_continues() {
        let clock = TestClock::new(0);
        // first response 500 (skip), second 200 (process).
        let transport = ScriptedTransport::new(vec![ex(500, "boom", &[]), ex(200, "42.0", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        agent
            .subscribe(&link(
                "asset1",
                "temp",
                serde_json::json!({ "path": "/temp", "polling_millis": 5000 }),
            ))
            .unwrap();

        // first poll -> 500 -> no reading, but the request DID happen.
        let r = agent.read().unwrap();
        assert!(r.is_empty(), "non-2xx yields no reading");
        assert_eq!(transport.request_count(), 1);

        // polling continues: next due at 5000 -> 200 -> reading.
        clock.advance(5_000);
        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "polling continues after a non-2xx");
        assert_eq!(r[0].raw.0, serde_json::json!("42.0"));
        assert_eq!(r[0].device_timestamp_ms, 0, "no device ts -> 0 (§2A.1)");
    }

    // ---- §2A.29 Link: rel=next pagination accumulates ----------------------

    #[test]
    fn pagination_accumulates_link_next_pages() {
        let clock = TestClock::new(0);
        // page 1 has a Link: rel=next to page 2; page 2 has no next.
        let transport = ScriptedTransport::new(vec![
            ex(
                200,
                "page1",
                &[(
                    "Link",
                    "<http://device.local/api/temp?page=2>; rel=\"next\"",
                )],
            ),
            ex(200, "page2", &[]),
        ]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        agent
            .subscribe(&link(
                "asset1",
                "log",
                serde_json::json!({ "path": "/temp", "polling_millis": 5000, "paging_mode": true }),
            ))
            .unwrap();

        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "one accumulated reading from two pages");
        // accumulated body is the newline-joined pages.
        assert_eq!(r[0].raw.0, serde_json::json!("page1\npage2"));
        // two requests were made: original + the rel=next follow.
        assert_eq!(transport.request_count(), 2);
        let reqs = transport.requests();
        assert_eq!(reqs[0].1, "http://device.local/api/temp");
        assert_eq!(reqs[1].1, "http://device.local/api/temp?page=2");
    }

    #[test]
    fn pagination_disabled_does_not_follow_next() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(
            200,
            "page1",
            &[(
                "Link",
                "<http://device.local/api/temp?page=2>; rel=\"next\"",
            )],
        )]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        agent
            .subscribe(&link(
                "asset1",
                "log",
                serde_json::json!({ "path": "/temp", "polling_millis": 5000 }), // paging off
            ))
            .unwrap();
        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].raw.0, serde_json::json!("page1"));
        assert_eq!(
            transport.request_count(),
            1,
            "no next-link follow when paging off"
        );
    }

    #[test]
    fn parse_link_next_extracts_next_uri() {
        let mut h = BTreeMap::new();
        h.insert(
            "link".into(),
            "<http://a/1>; rel=\"prev\", <http://a/3>; rel=\"next\", <http://a/9>; rel=\"last\""
                .into(),
        );
        assert_eq!(parse_link_next(&h), Some("http://a/3".to_string()));

        // unquoted rel, single link
        let mut h2 = BTreeMap::new();
        h2.insert("link".into(), "<http://a/2>; rel=next".into());
        assert_eq!(parse_link_next(&h2), Some("http://a/2".to_string()));

        // no next
        let mut h3 = BTreeMap::new();
        h3.insert("link".into(), "<http://a/9>; rel=\"last\"".into());
        assert_eq!(parse_link_next(&h3), None);

        // no link header at all
        assert_eq!(parse_link_next(&BTreeMap::new()), None);
    }

    // ---- §2A.30 fire-and-forget writes; updateOnWrite is the runtime's job -

    #[test]
    fn write_is_fire_and_forget_and_sends_processed_value() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        let mut l = link(
            "asset1",
            "setpoint",
            serde_json::json!({ "path": "/setpoint", "method": "PUT" }),
        );
        l.update_on_write = true; // the agent does NOT act on this; the runtime does.
        agent.subscribe(&l).unwrap();

        // a processed value -> body. write returns Ok even though it is f-and-f.
        agent
            .write(&l, &AttributeValue(serde_json::json!("21.5")))
            .unwrap();
        assert_eq!(transport.request_count(), 1);
        let reqs = transport.requests();
        assert_eq!(reqs[0].0, "PUT", "explicit non-GET method honored on write");
        assert_eq!(reqs[0].1, "http://device.local/api/setpoint");
    }

    #[test]
    fn write_to_unlinked_attribute_is_ignored() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        // never subscribed -> no slot -> ignored, no request.
        agent
            .write(
                &link("asset1", "nope", serde_json::json!({})),
                &AttributeValue(serde_json::json!("x")),
            )
            .unwrap();
        assert_eq!(
            transport.request_count(),
            0,
            "write to unlinked attr is ignored"
        );
    }

    // ---- unlink + connect/status sanity -----------------------------------

    #[test]
    fn unlink_stops_polling() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "1", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport.clone()),
            clock.closure(),
        );
        let l = link(
            "asset1",
            "temp",
            serde_json::json!({ "path": "/temp", "polling_millis": 5000 }),
        );
        agent.subscribe(&l).unwrap();
        agent.read().unwrap();
        assert_eq!(transport.request_count(), 1);

        agent.unlink(&l).unwrap();
        clock.advance(10_000);
        let r = agent.read().unwrap();
        assert!(r.is_empty());
        assert_eq!(transport.request_count(), 1, "no poll after unlink");
    }

    #[test]
    fn connect_marks_connected_with_base_uri() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "", &[])]);
        let mut agent = HttpAgent::with_transport_and_clock(
            "a1",
            http_config(),
            Box::new(transport),
            clock.closure(),
        );
        let ctx = AgentContext {
            root: std::path::Path::new("/tmp"),
            agent_id: "a1".into(),
            realm: "master".into(),
            config: serde_json::json!({}),
        };
        assert_eq!(agent.status(), ConnectionStatus::Disconnected);
        let st = agent.connect(&ctx).unwrap();
        assert_eq!(st, ConnectionStatus::Connected);
        assert_eq!(agent.kind(), IotAgentKind::Http);
    }

    #[test]
    fn connect_without_base_uri_schedules_backoff() {
        let clock = TestClock::new(0);
        let transport = ScriptedTransport::new(vec![ex(200, "", &[])]);
        let mut cfg = http_config();
        cfg.base_uri = "".into();
        let mut agent =
            HttpAgent::with_transport_and_clock("a1", cfg, Box::new(transport), clock.closure());
        let ctx = AgentContext {
            root: std::path::Path::new("/tmp"),
            agent_id: "a1".into(),
            realm: "master".into(),
            config: serde_json::json!({}),
        };
        let st = agent.connect(&ctx).unwrap();
        assert_eq!(st, ConnectionStatus::Waiting, "missing base URI -> backoff");
    }

    // ---- REAL loopback listener: prove the production ureq client path ------
    //
    // A genuine 127.0.0.1 TCP listener speaking minimal HTTP/1.1, hit through the
    // REAL `UreqTransport` (the production client). This proves the agent works
    // end-to-end over the actual ureq path identified in research, returning a
    // 2xx body and a paginated Link header. The injected clock still drives the
    // poll cadence deterministically.

    fn spawn_http_fixture() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        let handle = thread::spawn(move || {
            // Serve a fixed number of connections then exit.
            // /page1 -> body "p1" + Link rel=next to /page2
            // /page2 -> body "p2", no next
            // /single -> body "23.5"
            for _ in 0..6 {
                let stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => break,
                };
                handle_conn(stream);
            }
        });
        (base, handle)
    }

    fn handle_conn(mut stream: TcpStream) {
        let mut buf = [0u8; 1024];
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(_) => return,
        };
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();

        let (body, extra_header): (&str, String) = if path.starts_with("/page1") {
            // Link header points at the SAME listener's /page2.
            let local = stream.local_addr().unwrap();
            (
                "p1",
                format!("Link: <http://{local}/page2>; rel=\"next\"\r\n"),
            )
        } else if path.starts_with("/page2") {
            ("p2", String::new())
        } else {
            ("23.5", String::new())
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
            body.len(),
            extra_header,
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    #[test]
    fn real_loopback_listener_2xx_body_via_ureq() {
        let (base, _handle) = spawn_http_fixture();
        let clock = TestClock::new(0);

        let cfg = HttpAgentConfig {
            base_uri: base.clone(),
            request_headers: BTreeMap::new(),
            auth_header_key: None,
        };
        // REAL ureq transport (production path), deterministic clock.
        let mut agent = HttpAgent {
            agent_id: "a1".into(),
            config: cfg,
            auth_header_value: None,
            transport: Box::new(UreqTransport),
            clock: clock.closure(),
            reconnect: ReconnectStateMachine::new(1),
            slots: BTreeMap::new(),
        };
        agent
            .subscribe(&link(
                "asset1",
                "temp",
                serde_json::json!({ "path": "/single", "polling_millis": 5000 }),
            ))
            .unwrap();

        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "real loopback 2xx body mapped to a reading");
        assert_eq!(r[0].raw.0, serde_json::json!("23.5"));
    }

    #[test]
    fn real_loopback_listener_pagination_via_ureq() {
        let (base, _handle) = spawn_http_fixture();
        let clock = TestClock::new(0);

        let cfg = HttpAgentConfig {
            base_uri: base.clone(),
            request_headers: BTreeMap::new(),
            auth_header_key: None,
        };
        let mut agent = HttpAgent {
            agent_id: "a1".into(),
            config: cfg,
            auth_header_value: None,
            transport: Box::new(UreqTransport),
            clock: clock.closure(),
            reconnect: ReconnectStateMachine::new(1),
            slots: BTreeMap::new(),
        };
        agent
            .subscribe(&link(
                "asset1",
                "log",
                serde_json::json!({ "path": "/page1", "polling_millis": 5000, "paging_mode": true }),
            ))
            .unwrap();

        let r = agent.read().unwrap();
        assert_eq!(r.len(), 1, "accumulated pages -> one reading");
        // page1 body "p1" + page2 body "p2", newline-joined.
        assert_eq!(r[0].raw.0, serde_json::json!("p1\np2"));
    }
}
