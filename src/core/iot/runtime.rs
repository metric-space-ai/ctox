// Origin: CTOX
// License: AGPL-3.0-only
//
// Phase 3 — IoT protocol-agent runtime: spawn/supervise the per-agent device
// loops and wire them to the engine write path. This is the IoT analogue of
// `communication::runtime`: it owns the moving parts (the connect/retry pump,
// the inbound drain, the outbound device write) that sit BETWEEN a concrete
// `IotAgent` (gateway-dispatched) and CTOX's SQLite engine.
//
// The two directions it wires:
//   INBOUND   device PUBLISH/poll/frame
//             -> agent.read() -> raw AttributeReading
//             -> adapters::do_inbound_value_processing (filters/converters/coerce, §2A.28)
//             -> store::process_attribute_event(conn, event, now)  [datapoint recorded here]
//             -> projector reprojection (project_attribute/project_asset/...)
//   OUTBOUND  device-backed attribute.write
//             -> adapters::do_outbound_value_processing (%VALUE%/%TIME%, §2A.28)
//             -> agent.write(link, processed)
//             -> §2A.30 update_on_write: re-write locally via process_attribute_event + reproject.
//
// HARD RULES honored here:
//   * native Rust only; the agent loop is driven by the crate's existing tokio
//     (the spawned supervisor) but the per-step logic is a plain sync fn so it is
//     DIRECTLY testable without a runtime — `run_agent_step` / `write_attribute`.
//   * the clock is INJECTED into every time-dependent path (reconnect poll,
//     timestamp normalization) — production passes `crate::iot::now_ms()`.
//   * ALL state lands in runtime/ctox.sqlite3 via `store::process_attribute_event`
//     against `crate::paths::core_db(root)` (opened by `store::open_iot_store`);
//     this layer never writes the DB directly and never reads `std::env`.
//   * agents talk to DEVICES; the reprojected rows are handed to the integrator
//     (the existing RxDB sync path), NOT to any HTTP browser bridge.

use crate::iot::adapters::{
    do_inbound_value_processing, do_outbound_value_processing, AgentLink, ConnectionStatus,
    IotAgent,
};
use crate::iot::model::{coerce_value, AttributeEvent, AttributeValue, ValueBaseType};
use crate::iot::projector::{self, ProjectionRow};
use crate::iot::store::{self, EventOutcome};
use crate::iot::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// One supervised agent: the boxed protocol agent plus the link table keyed by
/// `(asset_id, attribute_name)`. The links carry the §2A.28 filter/converter
/// chains the base layer applies, so the runtime resolves the right link for an
/// inbound reading / outbound write without re-parsing protocol bindings.
pub(crate) struct AgentRuntime {
    agent: Box<dyn IotAgent>,
    /// the agent's realm, stamped onto reprojected rows / status.
    realm: String,
    /// linked attributes keyed by (asset_id, attribute_name).
    links: HashMap<(String, String), AgentLink>,
    /// §2A.21 startup suppression (the in-process mission-start guard): the
    /// FIRST inbound step replays pre-existing device state into the engine and
    /// must NOT fire alarms/work. It flips warm after that first step, so only
    /// changes observed while the engine is warm are routed into the mission
    /// loop. Condition-evaluation MEMORY itself is durable (iot_condition_state),
    /// so a restart does not re-fire matches that already fired before the crash.
    warmed: bool,
}

/// The result of one inbound step: how many device readings were applied and the
/// reprojected rows the integrator should push to RxDB. Returned (not pushed)
/// so the step stays pure/testable and the sync wiring stays in the integrator.
#[derive(Debug, Default)]
pub(crate) struct StepOutcome {
    /// device readings that produced an engine write this step.
    pub applied: usize,
    /// device readings dropped by the base layer (§2A.28 @IGNORE / coercion fail).
    pub dropped: usize,
    /// reprojected projection rows (attribute + asset summaries) for the writes.
    pub projections: Vec<ProjectionRow>,
}

impl AgentRuntime {
    /// Wrap a dispatched agent (see `gateway::build_agent`) with its realm.
    pub(crate) fn new(agent: Box<dyn IotAgent>, realm: impl Into<String>) -> Self {
        AgentRuntime {
            agent,
            realm: realm.into(),
            links: HashMap::new(),
            warmed: false,
        }
    }

    /// Register a linked attribute: subscribe the agent to the device source AND
    /// remember the link so the base layer can resolve its filters/converters.
    /// Safe to call while a reconnect is in flight (§2A.27) — the agents mutate a
    /// synchronized table.
    pub(crate) fn link(&mut self, link: AgentLink) -> Result<()> {
        self.agent.subscribe(&link)?;
        self.links
            .insert((link.asset_id.clone(), link.attribute_name.clone()), link);
        Ok(())
    }

    /// Remove a linked attribute (§2A.27).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn unlink(&mut self, asset_id: &str, attribute_name: &str) -> Result<()> {
        if let Some(link) = self
            .links
            .remove(&(asset_id.to_string(), attribute_name.to_string()))
        {
            self.agent.unlink(&link)?;
        }
        Ok(())
    }

    pub(crate) fn status(&self) -> ConnectionStatus {
        self.agent.status()
    }

    /// Resolve the declared base type for a linked attribute from the engine
    /// (stored attribute row else the asset-type descriptor). The base layer
    /// coerces inbound readings to this type; `None` lets the raw value through.
    fn declared_type(
        conn: &Connection,
        asset_id: &str,
        attribute_name: &str,
    ) -> Option<ValueBaseType> {
        let asset = store::get_asset(conn, asset_id).ok()??;
        if let Some(attr) = asset.attributes.get(attribute_name) {
            if let Some(t) = attr.value_type {
                return Some(t);
            }
        }
        let info = store::get_asset_type(conn, &asset.asset_type).ok()??;
        info.descriptor(attribute_name)
            .map(|d| d.value_descriptor.base_type)
    }

    /// ONE inbound step against the engine, driven by the INJECTED clock:
    ///   1. drive the agent's connect/reconnect one transition (idempotent if
    ///      already CONNECTED; the agent owns the §2A.24 backoff math),
    ///   2. drain raw device readings,
    ///   3. for each, resolve its link + declared type, run the §2A.28 inbound
    ///      base layer; a dropped reading (ignore) is counted and skipped,
    ///   4. write the surviving event via `process_attribute_event` (which records
    ///      the datapoint), then
    ///   5. reproject the affected attribute + asset rows for the sync path.
    /// ref: communication/runtime.rs (per-channel pump step) + AbstractProtocol
    /// onMessageReceived (the inbound mapping the runtime owns, not the agent).
    pub(crate) fn run_agent_step(
        &mut self,
        conn: &Connection,
        ctx: &crate::iot::adapters::AgentContext,
        now_ms: i64,
    ) -> Result<StepOutcome> {
        // (1) keep the link alive. connect() is idempotent when CONNECTED and is a
        // single SM transition otherwise; the supervisor loop calls this on a
        // cadence so WAITING -> CONNECTING happens once the injected clock is due.
        let _status = self.agent.connect(ctx)?;

        // (2) drain raw readings.
        let readings = self.agent.read()?;
        let mut outcome = StepOutcome::default();

        for reading in &readings {
            let key = (reading.asset_id.clone(), reading.attribute_name.clone());
            let link = match self.links.get(&key) {
                Some(l) => l.clone(),
                None => {
                    // No link for this reading (a topic mapped to an unlinked attr):
                    // skip it as a drop rather than fabricating an event.
                    outcome.dropped += 1;
                    continue;
                }
            };

            // (3) §2A.28 inbound base layer (filters -> converter -> coerce).
            let declared = Self::declared_type(conn, &reading.asset_id, &reading.attribute_name);
            let (ignore, value) = do_inbound_value_processing(reading, &link, declared);
            if ignore {
                outcome.dropped += 1;
                continue;
            }

            // Capture the prior value BEFORE the write so condition predicates
            // testing `previousValue` (§2A.23) see what this event replaced.
            let prior_value = store::get_asset(conn, &reading.asset_id)
                .ok()
                .flatten()
                .and_then(|a| {
                    a.attributes
                        .get(&reading.attribute_name)
                        .and_then(|attr| attr.value.clone())
                });

            // (4) engine write path — records the datapoint internally (§2A.3-8).
            let event = AttributeEvent {
                asset_id: reading.asset_id.clone(),
                attribute_name: reading.attribute_name.clone(),
                value,
                // §2A.1/2A.2 — a 0 device timestamp is normalized against `now_ms`
                // inside process_attribute_event; a real device ts is honored.
                timestamp: reading.device_timestamp_ms,
                old_value: None,
                old_value_timestamp: 0,
            };
            store::process_attribute_event(conn, &event, now_ms)?;
            outcome.applied += 1;

            // Route the just-applied change through the thin condition layer.
            // §2A.21: the first step over pre-existing state is cold (warmed ==
            // false) and emits nothing; subsequent live changes are warm. CTOX's
            // mission brain (channels/queue/schedule/spawn-budget) does the
            // firing — no second automation engine lives here.
            let eval_event = AttributeEvent {
                asset_id: reading.asset_id.clone(),
                attribute_name: reading.attribute_name.clone(),
                value: event.value.clone(),
                timestamp: event.timestamp,
                old_value: prior_value,
                old_value_timestamp: 0,
            };
            crate::iot::conditions::evaluate_and_emit(
                ctx.root,
                &self.realm,
                &eval_event,
                self.warmed,
                now_ms,
            )?;

            // (5) reproject the written attribute + its owning asset summary.
            outcome.projections.extend(reproject_attribute(
                conn,
                &reading.asset_id,
                &reading.attribute_name,
            )?);
        }

        // §2A.21 mission-start guard: after the first step's pre-existing-state
        // replay, the engine is warm for all subsequent steps.
        self.warmed = true;

        Ok(outcome)
    }

    /// Outbound device-backed `attribute.write`: run the §2A.28 outbound base
    /// layer against the link, send the processed value to the DEVICE via the
    /// agent, then honor §2A.30 `update_on_write` by re-writing the attribute
    /// locally through the engine and reprojecting. Returns the reprojected rows
    /// (empty when the link is push-only / drop / not update_on_write).
    /// ref: AbstractIOClientProtocol doLinkedAttributeWrite + §2A.30.
    pub(crate) fn write_attribute(
        &mut self,
        conn: &Connection,
        asset_id: &str,
        attribute_name: &str,
        value: &AttributeValue,
        now_ms: i64,
    ) -> Result<Vec<ProjectionRow>> {
        let link = self
            .links
            .get(&(asset_id.to_string(), attribute_name.to_string()))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("write to unlinked attribute {asset_id}/{attribute_name}")
            })?;

        // §2A.28 outbound processing (write filters -> converter -> %VALUE%/%TIME%).
        let processed = match do_outbound_value_processing(value, &link, now_ms)? {
            Some(v) => v,
            None => return Ok(Vec::new()), // @IGNORE on write -> no device send.
        };

        // Send to the DEVICE (fire-and-forget per protocol; agent owns transport).
        self.agent.write(&link, &processed)?;

        if !link.update_on_write {
            // §2A.30 — without update_on_write the local state waits for a device
            // echo (which would arrive as an inbound reading next step).
            return Ok(Vec::new());
        }

        // §2A.30 — re-write the attribute LOCALLY immediately with the REQUESTED
        // value, NOT the device-wire `processed` form. The `%VALUE%`/`%TIME%`
        // template (and any write converter) shapes only the bytes sent to the
        // device; the locally-stored attribute reflects the value the caller
        // wrote, coerced to the declared type. ref: AbstractProtocol
        // updateLinkedAttribute(state) — the state value, not the wire payload.
        let declared = Self::declared_type(conn, asset_id, attribute_name);
        let local_value = match declared {
            Some(t) => coerce_value(value, t).unwrap_or_else(|_| value.clone()),
            None => value.clone(),
        };
        let event = AttributeEvent {
            asset_id: asset_id.to_string(),
            attribute_name: attribute_name.to_string(),
            value: local_value,
            timestamp: 0, // normalized to `now_ms` by the engine.
            old_value: None,
            old_value_timestamp: 0,
        };
        match store::process_attribute_event(conn, &event, now_ms)? {
            EventOutcome::Updated | EventOutcome::OutdatedRecordedOnly => {}
        }
        reproject_attribute(conn, asset_id, attribute_name)
    }
}

/// Reproject the canonical envelope for one written attribute plus its owning
/// asset summary, so the integrator's RxDB push carries the richer projector
/// rows. ref: projector::reproject_business_command_outcome (same reproject_one
/// surface, driven here directly from the engine write rather than a result doc).
fn reproject_attribute(
    conn: &Connection,
    asset_id: &str,
    attribute_name: &str,
) -> Result<Vec<ProjectionRow>> {
    let mut rows = projector::project_attribute(conn, asset_id, attribute_name)?;
    rows.extend(projector::project_asset(conn, asset_id)?);
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Supervisor (tokio) — the always-on pump. The Business OS native RxDB peer
// owns boot-time integration because it already owns the long-lived runtime and
// the business_records -> RxDB/WebRTC projection loop.
// ---------------------------------------------------------------------------

/// Cadence between inbound pump steps. The agents' own poll/read are non-blocking
/// drains; this is the supervisor tick, deliberately short so reconnect WAITING
/// windows (driven by the injected clock inside the agent) are observed promptly.
const PUMP_TICK: Duration = Duration::from_millis(100);

/// Spawn a supervised inbound pump for one agent on the crate's tokio runtime.
/// Returns a stop flag; setting it ends the loop after the current tick. The loop
/// opens its OWN connection to `core_db(root)` (SQLite handles are not Send-shared)
/// and steps the agent until stopped. The caller owns the `AgentRuntime`, root,
/// and projection callback; tests can still exercise `run_agent_step` directly.
pub(crate) fn spawn_supervisor<F>(
    mut runtime: AgentRuntime,
    root: std::path::PathBuf,
    agent_id: String,
    config: serde_json::Value,
    on_progress: F,
) -> Arc<AtomicBool>
where
    F: Fn(Vec<ProjectionRow>, ConnectionStatus) -> Result<()> + Send + Sync + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop_task = Arc::clone(&stop);
    let realm = runtime.realm.clone();
    let on_progress = Arc::new(on_progress);
    tokio::spawn(async move {
        // One owned connection for the life of the pump. ref: open_iot_store ->
        // crate::paths::core_db(root). A failure here means the engine store is
        // unavailable; the supervisor simply exits (nothing to pump into).
        let conn = match store::open_iot_store(&root) {
            Ok(c) => c,
            Err(_) => return,
        };
        let ctx = crate::iot::adapters::AgentContext {
            root: &root,
            agent_id,
            realm,
            config,
        };
        let mut last_status: Option<ConnectionStatus> = None;
        while !stop_task.load(Ordering::SeqCst) {
            let now = crate::iot::now_ms();
            // A transient step error must not kill the pump (the device firehose
            // must not wedge supervision); log to stderr and continue.
            match runtime.run_agent_step(&conn, &ctx, now) {
                Ok(outcome) => {
                    let status = runtime.status();
                    let status_changed = last_status != Some(status);
                    if status_changed || !outcome.projections.is_empty() {
                        let on_progress = Arc::clone(&on_progress);
                        if let Err(err) = on_progress(outcome.projections, status) {
                            eprintln!("ctox::iot::runtime: projection error: {err}");
                        }
                        last_status = Some(status);
                    }
                }
                Err(err) => {
                    eprintln!("ctox::iot::runtime: agent step error: {err}");
                }
            }
            tokio::time::sleep(PUMP_TICK).await;
        }
    });
    stop
}

// ---------------------------------------------------------------------------
// Tests — EXIT round-trip over a TEMP core_db: a loopback-broker MQTT device
// publishes a value that flows through the runtime into BOTH iot_attributes and
// iot_datapoints. Plus an outbound update_on_write round-trip.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::adapters::{AgentContext, IotAgentKind};
    use crate::iot::gateway;
    use crate::iot::model::{Asset, AssetTypeInfo, AttributeDescriptor, MetaMap, ValueDescriptor};
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    // ---- engine fixture ----------------------------------------------------

    fn seed_thermostat(conn: &Connection, id: &str) -> Asset {
        let info = AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![AttributeDescriptor {
                name: "temp".into(),
                value_descriptor: ValueDescriptor {
                    name: "temp_vd".into(),
                    base_type: ValueBaseType::Number,
                    array_dimensions: 0,
                    constraints: vec![],
                    units: None,
                    format: None,
                },
                meta: MetaMap::new(),
            }],
        };
        store::upsert_asset_type(conn, &info).unwrap();
        let asset = Asset::new_with_type(
            id.to_string(),
            "master".into(),
            "Thermostat".into(),
            "Living room".into(),
            &info,
        );
        store::upsert_asset(conn, &asset).unwrap();
        asset
    }

    // ---- minimal loopback MQTT broker (vendored codec, mirrors mqtt_native) -
    //
    // A tiny in-process broker that speaks just enough of the same wire format:
    // CONNECT->CONNACK, SUBSCRIBE->SUBACK(grant), then delivers one queued
    // PUBLISH. It is intentionally hand-rolled here (the mqtt_native broker
    // fixture is a private test module) using the public-on-the-wire framing.

    fn enc_remaining_len(mut len: usize, out: &mut Vec<u8>) {
        loop {
            let mut b = (len % 128) as u8;
            len /= 128;
            if len > 0 {
                b |= 128;
            }
            out.push(b);
            if len == 0 {
                break;
            }
        }
    }

    fn frame(ptype: u8, flags: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![(ptype << 4) | (flags & 0x0F)];
        enc_remaining_len(body.len(), &mut out);
        out.extend_from_slice(body);
        out
    }

    async fn read_packet(s: &mut TcpStream) -> std::io::Result<(u8, u8, Vec<u8>)> {
        let mut h = [0u8; 1];
        s.read_exact(&mut h).await?;
        let ptype = h[0] >> 4;
        let flags = h[0] & 0x0F;
        let mut mult = 1usize;
        let mut len = 0usize;
        loop {
            let mut b = [0u8; 1];
            s.read_exact(&mut b).await?;
            len += (b[0] & 127) as usize * mult;
            mult *= 128;
            if (b[0] & 0x80) == 0 {
                break;
            }
        }
        let mut rem = vec![0u8; len];
        s.read_exact(&mut rem).await?;
        Ok((ptype, flags, rem))
    }

    fn put_str(s: &str, out: &mut Vec<u8>) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    /// Start a one-shot loopback broker that, after the subscribe handshake,
    /// delivers `(topic, payload)` as a QoS-1 PUBLISH. Returns (port, shutdown).
    fn start_broker(
        deliver_topic: String,
        deliver_payload: Vec<u8>,
    ) -> (u16, std::sync::mpsc::Sender<()>) {
        let (port_tx, port_rx) = std::sync::mpsc::channel::<u16>();
        let (sd_tx, sd_rx) = std::sync::mpsc::channel::<()>();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                port_tx.send(listener.local_addr().unwrap().port()).unwrap();
                loop {
                    if sd_rx.try_recv().is_ok() {
                        return;
                    }
                    let accept =
                        tokio::time::timeout(Duration::from_millis(50), listener.accept()).await;
                    let (mut sock, _) = match accept {
                        Ok(Ok(p)) => p,
                        _ => continue,
                    };
                    let topic = deliver_topic.clone();
                    let payload = deliver_payload.clone();
                    tokio::spawn(async move {
                        // CONNECT -> CONNACK (session_present=false, return 0).
                        if read_packet(&mut sock).await.is_err() {
                            return;
                        }
                        let _ = sock.write_all(&frame(2, 0, &[0x00, 0x00])).await;
                        let mut delivered = false;
                        loop {
                            let pkt = match tokio::time::timeout(
                                Duration::from_millis(50),
                                read_packet(&mut sock),
                            )
                            .await
                            {
                                Ok(Ok(p)) => Some(p),
                                Ok(Err(_)) => return,
                                Err(_) => None,
                            };
                            if let Some((ptype, _flags, rem)) = pkt {
                                match ptype {
                                    8 => {
                                        // SUBSCRIBE: pid + (topic,qos)*. Grant each qos.
                                        let pid = u16::from_be_bytes([rem[0], rem[1]]);
                                        let mut pos = 2usize;
                                        let mut granted = Vec::new();
                                        while pos < rem.len() {
                                            let tl = u16::from_be_bytes([rem[pos], rem[pos + 1]])
                                                as usize;
                                            pos += 2 + tl;
                                            granted.push(rem[pos]);
                                            pos += 1;
                                        }
                                        let mut body = pid.to_be_bytes().to_vec();
                                        body.extend_from_slice(&granted);
                                        let _ = sock.write_all(&frame(9, 0, &body)).await;
                                    }
                                    14 => return, // DISCONNECT
                                    _ => {}
                                }
                            }
                            // Deliver the device PUBLISH once, after subscribe.
                            if !delivered {
                                let mut body = Vec::new();
                                put_str(&topic, &mut body);
                                body.extend_from_slice(&1u16.to_be_bytes()); // packet id
                                body.extend_from_slice(&payload);
                                // QoS 1 PUBLISH (flags 0b0010).
                                let _ = sock.write_all(&frame(3, 0b0010, &body)).await;
                                delivered = true;
                            }
                        }
                    });
                }
            });
        });
        let port = port_rx.recv().unwrap();
        (port, sd_tx)
    }

    fn ctx_for(root: &std::path::Path, port: u16) -> AgentContext<'_> {
        AgentContext {
            root,
            agent_id: "agent-rt-mqtt".into(),
            realm: "master".into(),
            config: json!({
                "host": "127.0.0.1",
                "port": port,
                "clientId": "ctox-rt-test",
                "cleanSession": true,
            }),
        }
    }

    // ---- EXIT round-trip: device PUBLISH -> runtime -> engine --------------

    #[test]
    fn exit_round_trip_mqtt_device_through_runtime_into_engine() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-rt-1";
        seed_thermostat(&conn, asset_id);

        let topic = "devices/thermostat/temp";
        let (port, shutdown) = start_broker(topic.to_string(), b"23.5".to_vec());

        // Dispatch the agent through the gateway (proves the dispatch path), then
        // wrap it in the runtime supervisor.
        let agent = gateway::build_agent(IotAgentKind::Mqtt, ctx_for(tmp.path(), port)).unwrap();
        let mut runtime = AgentRuntime::new(agent, "master");
        runtime
            .link(AgentLink {
                asset_id: asset_id.into(),
                attribute_name: "temp".into(),
                binding: json!({ "subscriptionTopic": topic, "publishTopic": topic, "qos": 1 }),
                ..AgentLink::default()
            })
            .unwrap();

        let ctx = ctx_for(tmp.path(), port);
        // Deterministic injected engine clock.
        let now = 1_700_000_000_000i64;

        // Step the runtime until the inbound device value lands (connect handshake
        // + delivery are async; the SM is clock-injected inside the agent).
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut total_applied = 0usize;
        let last_projections;
        loop {
            let outcome = runtime.run_agent_step(&conn, &ctx, now).unwrap();
            total_applied += outcome.applied;
            if outcome.applied > 0 {
                last_projections = outcome.projections;
                break;
            }
            assert!(
                Instant::now() < deadline,
                "device value never flowed through the runtime"
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(total_applied, 1, "exactly one device reading applied");

        // ASSERT 1: the value reached iot_attributes (coerced to Number).
        let asset = store::get_asset(&conn, asset_id).unwrap().unwrap();
        let temp = asset.attributes.get("temp").unwrap();
        assert_eq!(
            temp.value.as_ref().unwrap().as_numeric(),
            Some(23.5),
            "device value round-tripped into iot_attributes via the runtime"
        );

        // ASSERT 2: a datapoint was recorded by the engine write path.
        let samples = crate::iot::datapoints::all(&conn, asset_id, "temp", 0, i64::MAX).unwrap();
        assert_eq!(samples.len(), 1, "datapoint recorded in iot_datapoints");

        // The runtime also reprojected the attribute + asset for the sync path.
        assert!(
            !last_projections.is_empty(),
            "runtime reprojected the written attribute for the integrator"
        );

        let _ = shutdown.send(());
    }

    // ---- §2A.30 outbound update_on_write through the runtime ---------------

    #[test]
    fn outbound_update_on_write_rewrites_local_attribute() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-rt-2";
        seed_thermostat(&conn, asset_id);

        // A WS device endpoint we connect to so write() has a live socket. Use a
        // loopback ws server that just accepts (records nothing needed here).
        let server = WsEcho::start();
        let agent = gateway::build_agent(
            IotAgentKind::WebSocket,
            AgentContext {
                root: tmp.path(),
                agent_id: "agent-rt-ws".into(),
                realm: "master".into(),
                config: json!({ "url": server.url() }),
            },
        )
        .unwrap();
        let mut runtime = AgentRuntime::new(agent, "master");
        runtime
            .link(AgentLink {
                asset_id: asset_id.into(),
                attribute_name: "temp".into(),
                update_on_write: true, // §2A.30
                write_value: Some("set:%VALUE%".into()),
                binding: json!({}),
                ..AgentLink::default()
            })
            .unwrap();

        let ctx = AgentContext {
            root: tmp.path(),
            agent_id: "agent-rt-ws".into(),
            realm: "master".into(),
            config: json!({ "url": server.url() }),
        };
        // Drive connect to CONNECTED.
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let st = runtime.run_agent_step(&conn, &ctx, 1).unwrap();
            let _ = st;
            if runtime.status() == ConnectionStatus::Connected {
                break;
            }
            assert!(Instant::now() < deadline, "ws agent never connected");
            thread::sleep(Duration::from_millis(20));
        }

        let now = 1_700_000_500_000i64;
        // Outbound write of 21.5: the %VALUE% template makes the DEVICE payload
        // "set:21.5", but update_on_write re-writes the LOCAL attribute. The local
        // value is coerced to the declared Number type.
        let rows = runtime
            .write_attribute(&conn, asset_id, "temp", &AttributeValue(json!(21.5)), now)
            .unwrap();
        assert!(
            !rows.is_empty(),
            "update_on_write reprojected the attribute"
        );

        let asset = store::get_asset(&conn, asset_id).unwrap().unwrap();
        assert_eq!(
            asset
                .attributes
                .get("temp")
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .as_numeric(),
            Some(21.5),
            "§2A.30 update_on_write reflected locally without a device echo"
        );

        // The device received the templated frame.
        let mut got = Vec::new();
        for _ in 0..200 {
            got = server.received();
            if !got.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            got,
            vec!["set:21.5".to_string()],
            "device saw %VALUE% frame"
        );
    }

    // ---- a loopback ws server that records inbound frames ------------------

    struct WsEcho {
        addr: std::net::SocketAddr,
        received: Arc<Mutex<Vec<String>>>,
        rt: Arc<tokio::runtime::Runtime>,
        stop: Arc<AtomicBool>,
    }
    impl WsEcho {
        fn start() -> Self {
            use futures_util::StreamExt;
            let rt = Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(2)
                    .build()
                    .unwrap(),
            );
            let received = Arc::new(Mutex::new(Vec::<String>::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let listener = rt
                .block_on(async { TcpListener::bind("127.0.0.1:0").await })
                .unwrap();
            let addr = listener.local_addr().unwrap();
            let rec = Arc::clone(&received);
            let stop_t = Arc::clone(&stop);
            rt.spawn(async move {
                loop {
                    if stop_t.load(Ordering::SeqCst) {
                        return;
                    }
                    let accept =
                        tokio::time::timeout(Duration::from_millis(50), listener.accept()).await;
                    let (stream, _) = match accept {
                        Ok(Ok(p)) => p,
                        _ => continue,
                    };
                    let rec = Arc::clone(&rec);
                    tokio::spawn(async move {
                        let mut ws = match tokio_tungstenite::accept_async(stream).await {
                            Ok(w) => w,
                            Err(_) => return,
                        };
                        while let Some(item) = ws.next().await {
                            match item {
                                Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => {
                                    rec.lock().unwrap().push(t.as_str().to_string());
                                }
                                Ok(_) => {}
                                Err(_) => break,
                            }
                        }
                    });
                }
            });
            WsEcho {
                addr,
                received,
                rt,
                stop,
            }
        }
        fn url(&self) -> String {
            format!("ws://{}", self.addr)
        }
        fn received(&self) -> Vec<String> {
            self.received.lock().unwrap().clone()
        }
    }
    impl Drop for WsEcho {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = &self.rt;
        }
    }
}
