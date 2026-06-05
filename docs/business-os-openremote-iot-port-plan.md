# Business OS — IoT App (Native OpenRemote Port) Production Plan

Status: **v1 — finalized, ready for the Phase 0 RFC**
Owner: TBD
Last updated: 2026-06-04
Porting source: `openremote/openremote` (cloned for analysis into `archive/openremote`, AGPL-3.0)
Backend target: native CTOX Rust IoT subsystem `src/core/iot/`
Frontend target: no-build Business OS app `src/apps/business-os/modules/iot/`, App-Store-installable

**Design decisions (locked):**
1. Native Rust engine — no JVM, Docker, Keycloak, Postgres, or broker dependency (§1).
2. One engine, three surfaces — app (`business_commands`), agent (`ctox iot` CLI), and an IoT skill — all over the same `iot::` code (§4A).
3. **Thin condition layer, not a second automation engine** — attribute predicates are ported; firing, scheduling, recurrence, dedup and loop-bounding are reused from CTOX's existing mission/queue/schedule/spawn-budget machinery (§2.2, §4A).
4. The §2A edge-case inventory, §12 unit-test spec, and §13 user stories are **binding acceptance gates**, not aspirations.

---

## 0. Read-before-acting note

Written after reading the root architecture docs (`README.md`, `HARNESS.md`,
`CLAUDE.md`, `docs/architecture.md`), the Business OS contracts
(`src/apps/business-os/README.md`, `ARCHITECTURE.md`, `RXDB_SYNC_CONTRACT.md`),
the live module manifests and mount contract
(`modules/registry.json`, `modules/matching/index.js`), the command and
collection path on the Rust side (`src/core/business_os/rxdb_peer.rs`,
`business_os_schema_contract.json`), the existing **native connector framework**
(`src/core/communication/` — `adapters.rs`, `gateway.rs`, `runtime.rs`,
`email_native.rs`, `whatsapp_native.rs`, plus the forked `whatsapp_rust/`
client), and the OpenRemote tree under `archive/openremote`.

---

## 1. Decision (final)

**Port OpenRemote's IoT domain into native CTOX Rust. No external runtime.**

This is the same discipline CTOX already applies to the Codex harness, the RxDB
peer (`src/core/rxdb/`), and the WhatsApp client (`whatsapp_rust/`): hard-fork
the *concepts*, reimplement them as native, self-contained Rust, with
authoritative state in `runtime/ctox.sqlite3`.

Explicitly **NOT** doing:

- ❌ No OpenRemote Java `manager`, no Gradle, no JVM.
- ❌ No Docker / docker-compose / external process to bridge to.
- ❌ No Keycloak (CTOX identity is `business_users` + `business_module_acl`).
- ❌ No PostgreSQL / TimescaleDB (state lives in `runtime/ctox.sqlite3`).
- ❌ No ActiveMQ broker dependency — MQTT is implemented as a native Rust client/agent.
- ❌ No `@openremote/*` LitElement components, no rspack build in the browser.
- ❌ No HTTP data bridge browser↔CTOX (data-boundary rule, `README.md`, memory `feedback_no_http_sync_path`).

The architecture is **one native engine with three consumer surfaces** — so IoT
is a real CTOX capability the agent harness can use, not just an app feature:

1. **Engine (backend)** — a native Rust IoT subsystem at `src/core/iot/`, modeled
   on the existing `src/core/communication/` connector framework. Single source
   of truth: asset model, datapoint store, native protocol agents
   (MQTT/HTTP/WebSocket), alarm store, and a **thin attribute-condition layer**
   (firing/scheduling reused from CTOX's mission core — not a second engine).
   Authoritative state in `runtime/ctox.sqlite3`.
2. **App surface** — a no-build Business OS module `modules/iot/` that looks and
   behaves like the existing apps (`customers`, `shiftflow`, `matching` are the
   reference points): same `mount(ctx)` contract, shared helpers, shell tokens,
   3-pane + drawers. Reaches the engine via RxDB `iot_*` projections +
   `ctox.iot.*` `business_commands`.
3. **Harness / agent surface** — a `ctox iot` CLI subcommand (like `ctox queue` /
   `ctox ticket`) that workers and the agent call as auditable commands, **plus**
   IoT events (alarms, rule actions) that feed the mission queue so the agent can
   *autonomously act on* physical-world conditions. See §4A.

All three call the same `iot::` functions; none re-implements domain logic.

### Naming / trademark

Subsystem and surface are named **`iot`** (collections `iot_*`, commands
`ctox.iot.*`), not "OpenRemote". OpenRemote is AGPL-3.0 and its name/logo are
trademarked; `src/apps/business-os/README.md` already carries the matching CTOX
trademark stance. We credit OpenRemote as the porting source in
`docs/legal/NOTICE` and in `// ref:` anchors, but the CTOX product surface uses
the CTOX-native `iot` name. (If a recognizable label is wanted in the store
listing, "CTOX IoT — IoT asset & automation workspace" with an "inspired by /
ported from OpenRemote" credit line.)

---

## 2. Port ledger (grounded in reading both codebases)

This section is built from actually reading OpenRemote's implementation, not its
file names. **OpenRemote is 157,633 Java LOC total.** The honest framing of the
"port vs. reimplement" balance is: **only ~30k LOC is even in scope; of that, the
domain *semantics/algorithms* port faithfully (that is where the edge cases
live), while the *persistence and transport framework* is necessarily
reimplemented on CTOX-native primitives** (SQLite, native event dispatch). The
remaining ~125k LOC (18 other protocol agents, the MQTT broker, the Java
container framework, Hibernate/Jackson/Camel, the LitElement UI, setup) is **out
of scope by design**, not "skipped".

So the answer to "won't this be 15% ported and 85% naive?" is: no — but only if
the edge-case lists in **§2A** are treated as binding acceptance criteria for the
port, which they now are. The risk is real and the mitigation is the edge-case
inventory + the unit-test spec (§12).

### 2.1 The ledger — what each subsystem becomes

| Subsystem | Upstream (in-scope LOC) | Verdict | Native target | Key upstream refs |
| --- | --- | --- | --- | --- |
| Asset/attribute model | `model/asset` 5,550 + `model/attribute` 1,903 + `model/value` 3,228 | **PORT semantics** (drop Hibernate/Jackson/JSR-380) | ~1,500 LOC | `Attribute.java:58-474`, `MetaItem.java`, `ValueDescriptor.java` |
| Attribute processing pipeline | `manager/asset` 5,322 | **PORT the flow, REIMPLEMENT storage** (drop Camel/JPA) | ~1,400 LOC | `AssetProcessingService.java:264-445`, `AssetStorageService.java:1404-1439` |
| Datapoints + queries | `model/datapoint` 1,136 + `manager/datapoint` 1,824 | **PORT query semantics; REIMPLEMENT LTTB** (Java delegates LTTB to TimescaleDB `timescaledb_toolkit.lttb()` — there is *no* Java LTTB to copy) | ~1,500 LOC | `AssetDatapointLTTBQuery.java:27-36`, `AbstractDatapointService.java:97-290` |
| Rules — **thin condition layer only** | `model/rules` json types + `JsonRulesBuilder` *condition* eval 1,520 + predicates 308 (NOT the `RulesEngine` firing loop / `RulesFacts` / jeasy-rules) | **PORT conditions; ROUTE the rest into CTOX's mission loop** (see §2.2) | ~1,500 LOC | `JsonRulesBuilder.java:86-1519` (predicates + duration), `AssetQueryPredicate.java` |
| Protocol agent abstraction | `AbstractProtocol.java` 258 + `AbstractIOClientProtocol` 223 | **PORT** (this is the `IotAgent` trait) | ~400 LOC | `AbstractProtocol.java:1-258` |
| MQTT *client* agent | `agent/protocol/mqtt` 1,824 | **PORT** (native Rust client; vendor/fork like `whatsapp_rust`) | ~1,500 LOC | `AbstractMQTT_IOClient.java:428-669` (reconnect/backoff/resubscribe) |
| HTTP agent | `agent/protocol/http` 1,184 | **PORT** | ~800 LOC | `HTTPProtocol.java:137-597` |
| WebSocket agent | `agent/protocol/websocket` | **PORT** | ~600 LOC | `agent/protocol/websocket` |
| Alarms | `AlarmResource` + alarm model | **PORT lifecycle** | ~500 LOC | `AlarmResource.java`, alarm model |
| Realms/tenancy | `RealmResource` | **REIMPLEMENT** onto CTOX session + `business_module_acl` (no Keycloak) | ~300 LOC | — |

**Estimated native engine: ~9,800 Rust LOC** (the rules decision in §2.2 cut ~3,200
LOC by not porting a second automation engine), plus CTOX integration
(collections, commands, CLI, skill, projector ~2,500) plus the frontend module
(~4,000) plus **tests (~1:1 on the edge-case-heavy subsystems, see §12) ≈ 11–16k
more**. **Realistic total: 27,000–35,000 LOC.** This is a multi-month, multi-phase effort
— the §6 phases and §11 workflow decomposition exist precisely to keep it
shippable and verifiable in slices rather than as one naive dump.

### 2.2 REIMPLEMENT-not-port (no upstream to copy — it is framework/DB)

- SQLite storage layer (OpenRemote uses PostgreSQL JSONB + Hibernate).
- **LTTB downsampling algorithm** — Java pushes it to TimescaleDB; we implement the classic Largest-Triangle-Three-Buckets in Rust (~200–400 LOC) and the interval/nearest/all queries on SQLite window functions.
- In-process event dispatch (replaces Apache Camel routes).
- JSON value handling (replaces Jackson custom (de)serializers).
- Per-asset write locking (replaces Hibernate `@Version` + `withAssetLock`).
- **Rule firing/scheduling/dedup/loop-bounding — NOT ported, delegated to CTOX.** OpenRemote's `RulesEngine` firing loop (~1,800 LOC), `RulesFacts`, jeasy-rules, the ~3s/50s timers, recurrence timers, and the 100-trigger loop cap are **not** reimplemented. CTOX already owns this: `schedule.rs` (time-based emission), `queue.rs` (durable work + dedup), `mission_governor.rs` (loop governance), and the durable-spawn **budget** (`core_transition_guard.rs`) which provably bounds re-firing. We port only the *condition evaluation*; CTOX's existing brain does the *deciding and scheduling*. This is the key "seamless, not a second automation engine" decision.

### 2.3 DROP (out of scope, with reason)

- **MQTT broker service** `manager/mqtt` 2,669 — that is OpenRemote exposing *its own* API over MQTT for external clients; CTOX uses RxDB/WebRTC for that. We keep the MQTT *client* agent (connect out to devices), not the broker.
- Hibernate/JPA, Jackson, Apache Camel, JSR-380 validation, GraalVM, Keycloak, the Java `container` framework (8,271 LOC), the LitElement UI, `setup` (2,899 LOC).

### 2.4 DEFER (explicit, not silent — revisit after the core is solid)

- **Groovy + JavaScript + Flow rules.** Groovy (~350 LOC) needs a Groovy runtime; JS is *not even implemented upstream* (legacy); Flow is a separate visual model. Only when-then/JSON ships first.
- **18 of 20 protocol agents** — `agent/protocol/` is 62,138 LOC across artnet, bluetooth, knx, lorawan, modbus, snmp, zwave, velbus, tradfri, openweathermap, serial, tcp, udp, mail, simulator, io, …. We ship MQTT + HTTP + WebSocket first; the rest are added one crate-file at a time on demand.
- Predicted datapoints / forecasting, gateway/edge federation, provisioning auto-enrollment, dashboard builder, map tiling server.
- The long tail of the 45 asset classes — start with a generic typed-attribute model + a few high-value types (sensor, plug/switch, thermostat, building/room grouping).

### Porting discipline (CLAUDE.md)

When porting domain logic with real algorithmic content (LTTB downsampling,
when-then rule evaluation, attribute-event semantics, asset model descriptor
resolution), follow the same rule as the Qwen graph and RxDB ports: a
`// ref: <upstream-file>:<line-range>` anchor on each ported function, preserve
upstream names, translate comments verbatim where they describe algorithm. Pure
CRUD/storage glue is idiomatic CTOX Rust, not a line port.

## 2A. Edge-case inventory (binding acceptance criteria for the port)

These are the non-obvious semantics found by reading the upstream code. **Each
line is a required behavior with a required test (§12).** This list is what
prevents "ported the happy path, missed every edge case." It is not exhaustive
forever, but nothing here may be silently dropped.

**Asset/attribute model & processing** (`AssetProcessingService`, `Attribute`):
1. Attribute `timestamp == 0` means "no explicit timestamp" — distinct from epoch; `hasExplicitTimestamp()` is `> 0`, not `>= 0`.
2. Event timestamp `<= 0` → use system time; event timestamp **in the future** → clamp to system time (clock-drift guard).
3. Outdated rule: event is outdated iff `oldValueTimestamp > eventTimestamp` (strictly). Outdated events are **not** persisted to current state but **are** still recorded as a datapoint.
4. Equality uses `(name, type, timestamp)` only (for change detection); deep-equality additionally compares value. Both are needed.
5. Type coercion happens at the event boundary **before** validation; coercion failure rejects the whole event.
6. Attribute-descriptor meta is merged into an attribute **once at creation**, never re-merged on later updates.
7. Per-asset write serialization (`withAssetLock`) around read-old-value → write so concurrent writers can't interleave.
8. Lazy value hydration: a stored attribute may carry an unparsed JSON string until its type/descriptor is known.

**Datapoints / LTTB** (`AssetDatapointLTTBQuery`, `AbstractDatapointService`):
9. LTTB only valid for numeric/boolean attributes; boolean coerced `true→1 / false→0`; non-numeric → `IllegalStateException` upstream (we reject explicitly).
10. Empty series → empty result; single point → that point; first and last points always retained by LTTB.
11. Interval queries align to bucket boundaries (not data timestamps); gap-fill + last-observation-carry-forward semantics for `time_bucket_gapfill`.
12. Nearest = closest datapoint **at or before** target ts (`<=`, order desc, limit 1).
13. Epoch/timezone: store normalized to UTC ms; upstream converts via system zone — we must not inherit a local-zone bug.
14. Query result hard limit (upstream default 100,000) — bounded, and truncation is logged (no silent cap).

**Attribute conditions** (thin layer from `JsonRulesBuilder`; *firing cadence,
recurrence and loop-bounding are delegated to CTOX's mission loop / `schedule.rs`
/ `queue.rs` / spawn-budget — not a ported engine, see §2.2*). The condition
*behavior* below must still hold exactly, regardless of who schedules it:
15. Re-trigger suppression: a matched asset won't re-fire until it goes unmatched (`previouslyMatchedAssetStates`), unless `RULE_RESET_IMMEDIATE` meta and a newer timestamp.
16. Duration windowing tracked per `(asset, predicateIndex)`; timer resets when the predicate goes false; all predicates must hold for the duration.
17. Recurrence scope `PER_ASSET` vs `GLOBAL`; `mins=null` never recurs, `0` always, `>0` blocks for N minutes (per-asset or global timer).
18. `otherwise` branch only with `trackUnmatched`; unmatched = assets matching the asset query but failing the attribute predicate.
19. Multi-condition AND: if any condition currently has no matches, clear matched-state so it can re-match (stale-AND reset).
20. Loop detection: hard cap (upstream 100) rule triggers per execution; action→attribute→rule chains count.
21. Startup suppression: actions do **not** fire on initial engine start over pre-existing asset state.
22. Multi-asset matching de-dupes by asset id, then applies order/limit (order after de-dup).
23. Predicate operators: value (eq/range/contains/regex), meta-item match, `previousValue`, user-asset-link.

> Delegation note: §2A.15 (re-trigger suppression), 17 (recurrence) and 20 (loop
> cap) are **satisfied by CTOX mechanisms** — durable-queue dedup, `schedule.rs`
> recurrence, and the spawn **budget** that bounds re-firing — not by a ported
> firing loop. The *observable behavior* must still match; the implementation is
> CTOX-native. 16/18/19/22/23 live in the ported condition layer; 21 (startup
> suppression) maps to CTOX's mission-start guard.

**Protocol agents** (`AbstractProtocol`, `AbstractMQTT_IOClient`, `HTTPProtocol`):
24. Reconnect: exponential backoff 1s→5min with 25% jitter, infinite retries; state machine `CONNECTED/CONNECTING/WAITING/DISCONNECTING/DISCONNECTED` with atomic transitions; stale-execution guards.
25. Resubscribe on session loss: if CONNACK `sessionPresent=false` resubscribe all; else optionally only previously-failed topics.
26. QoS preserved per subscription on resubscribe; Last-Will published on ungraceful disconnect; retain flag honored.
27. Link/unlink while a reconnect is in flight must be safe (consumer maps synchronized; resubscribe uses the topic set present at CONNACK).
28. Inbound/outbound value processing (filters → converters; `%VALUE%`/`%TIME%` placeholders) applied in the base layer, not per-protocol.
29. HTTP polling: minimum interval enforced (upstream 5s), fixed-delay (not fixed-rate), only 2xx processed, `Link: rel=next` pagination accumulates.
30. Writes are fire-and-forget unless `updateOnWrite` — then the attribute is updated locally to avoid round-trip lag.

---

## 3. Backend architecture — `src/core/iot/` (mirrors `src/core/communication/`)

The communication module is the template: a shared connector framework +
one native module per platform + a forked client crate for a hard protocol.
IoT is a **parallel domain** (assets/telemetry, not messages/channels), so it
gets its own subsystem rather than being forced through `mission/channels.rs`.

```
src/core/iot/
  mod.rs            subsystem entry, exports, wiring into service boot
  model.rs          asset / attribute / value types + descriptor registry   // ref: model/.../asset
  store.rs          authoritative SQLite state (assets, attributes, alarms, rulesets)
  datapoints.rs     time-series store + LTTB downsampling                     // ref: AssetDatapointLTTBQuery
  conditions.rs     thin attribute-predicate evaluator (NOT a firing engine;  // ref: JsonRulesBuilder predicates
                    emits matches → CTOX schedule.rs/queue.rs own the rest)
  alarms.rs         alarm lifecycle
  adapters.rs       trait IotAgent { kind(); connect(); subscribe(); read(); write(); }   (parallels CommunicationTransportAdapter)
  gateway.rs        agent registry + dispatch by IotAgentKind                 (parallels communication/gateway.rs)
  runtime.rs        lifecycle: spawned agent loops + projector into RxDB      (parallels communication/runtime.rs)
  projector.rs      writes iot_* RxDB collections; consumes ctox.iot.* commands
  agents/
    mqtt_native.rs      native MQTT client agent       (forked/vendored like whatsapp_rust if from-scratch)
    http_native.rs      HTTP poll/push agent
    ws_native.rs        WebSocket agent
```

- **Closed kind enum** `IotAgentKind { Mqtt, Http, WebSocket }` — adding an agent
  is a deliberate core edit, exactly like `CommunicationAdapterKind` and the
  fixed collection list. This is intentional, not a limitation.
- **Authoritative state in `runtime/ctox.sqlite3`** via `paths::core_db(root)`;
  no separate DB, no env vars — all config/secrets through
  `runtime_env::env_or_config(root, …)` and the CTOX secret store
  (`CLAUDE.md` guardrails).
- **Dependency policy:** native Rust only. Prefer std + minimal vendored
  protocol code over pulling a heavy framework; if a protocol client is
  non-trivial (MQTT), vendor/fork it as a self-contained crate the way
  `whatsapp_rust/` is, with the upstream commit pinned.
- **Service wiring:** the agent loops + projector are spawned tasks started from
  service boot, siblings to the existing `consume_business_commands_loop`
  (`rxdb_peer.rs:1057`). Idle-cheap when no agents are configured.

### How it reaches the browser (unchanged boundary)

- **Read:** `projector.rs` writes the `iot_*` collections (§5.1). The browser
  reads them over RxDB/WebRTC. No HTTP.
- **Write:** browser dispatches `ctox.iot.*` `business_commands`; the consumer
  (`accept_pending_business_command`, `rxdb_peer.rs:1897`, routed in the
  `command_type` block at `:2102`) calls into `iot::` — write attribute, ack
  alarm, toggle ruleset, configure agent — then projects the result back.

### Required core changes (confirmed against the code)

1. **Register `iot_*` collections** in `business_os_collections()` /
   `collection_creators()` (`rxdb_peer.rs:5159`), add their schemas, update
   `business_os_schema_contract.json`, regenerate `business_os_schema_hashes.json`,
   and extend the parity/`assert-rxdb-only.mjs` guard. Collections are a fixed,
   hash-guarded contract — this is mandatory core work.
2. **Add the `ctox.iot.*` command family** to the executor (`store::accept_rxdb_business_command`) and a projection branch in `rxdb_peer.rs`.
3. **The `src/core/iot/` subsystem itself** — the bulk of the work.

---

## 4. Frontend architecture — `modules/iot/` (must match existing apps)

The module is a no-build, vendored-ESM Business OS app. It must be
indistinguishable in look and behavior from `customers` / `shiftflow` —
those are the reference data-heavy apps.

### Module layout (same as every module)

```
src/apps/business-os/modules/iot/
  module.json     install_scope:"store", default_installed:false, shell:"full-workspace", collections:[iot_*]
  schema.js       client-side collection schema declarations (mirror customers/schema.js)
  index.html      static 3-pane markup (no <script>/<link> inline; shell strips them)
  index.js        export async function mount(ctx){...} returning teardown
  index.css       uses ONLY shell tokens; no custom theme vars
  icon.svg        gradient SVG icon in the registry style
  locales/        i18n (en + de, like matching/shiftflow)
  test.mjs        module smoke test (mirror app-store.test.mjs / matching/test.mjs)
```

### Contract the module must honor (from `matching/index.js`)

- Export `async function mount(ctx)` → returns a teardown function.
- Use the provided context only: `ctx.host` (center), `ctx.left`, `ctx.right`
  pane containers; `ctx.db` (`.raw` RxDB handles / `.collection()`);
  `ctx.commandBus.dispatch({ id, module:'iot', command_type:'ctox.iot.*',
  record_id, inbound_channel:'business_os.iot', payload, client_context })`.
- Load CSS via `<link ...?v=BUILD>`; load markup from `index.html` with
  scripts/links stripped, inject into `ctx.host`.
- **Read projected `iot_*` collections only; never import `rxdb` directly; all
  writes go through `business_commands`.** Show an honest "sync not ready" state
  when the peer/projector is down — no fallback data (`ARCHITECTURE.md` sync
  priority).

### Reuse the shell, don't reinvent (shared helpers)

The shell already provides, under `src/apps/business-os/shared/`: `command-bus.js`,
`db.js`, `sync.js`, `notifications.js`, `event-bus.js`, `context-menu.js`,
`dialogs.js`, `i18n.js`, `icons.js`, `window-manager.js`, `taskbar.js`,
`resizer.js`, `universal-importer.js`, `business-chat.js`. The module consumes
these; it does not ship its own resizer, toast, or sync logic (standardization
contract: shell owns loading/sync-toast/resizing, module uses pane-mode — memory
`project_app_standardization`). The loading skeleton is auto-derived from the
module's own `index.html`/`index.css` (memory `feedback_loading_shells_auto_derived`).

### Styling (must fit the others)

All color/dimension via shell tokens in `app.css`: `--bg`, `--surface`,
`--surface-2`, `--line`, `--text`, `--muted`, `--accent`, `--accent-soft`,
`--danger`, `--panel-radius`, `--control-radius`, `--panel-shadow`, `--shadow`,
`--shell-*`. No module-local theme variables. Follows
`[data-shell-style="windows"|"macos"]` at the shell level.

### Layout (the standard 3-pane + drawers, IoT-specific content)

- **left pane:** realm/scope selector + asset tree (hierarchy) + search/filter.
- **center pane:** asset/attribute workbench — selected asset's typed attributes
  with live values; a native (canvas/SVG) attribute chart over `iot_datapoints`;
  optional native map for geo-located assets. No `@openremote/*`.
- **right pane:** alarms, ruleset status, agent/connection context (CTOX "is
  working" surface).
- **drawers:** left = realm/agent setup & asset-type config; right = selected
  asset / ruleset inspector; bottom = selected assets + queued `ctox.iot.*`
  commands + agent diagnostics.

Where Documents/Spreadsheets-style heavy editing is needed (e.g. ruleset
editing), embed React only for that dense form (the contract allows React for
menus/settings/forms); working views stay direct ESM.

---

## 4A. IoT as a first-class CTOX capability (apps **and** harness)

The reason for a native engine (instead of an app-only feature) is that the
**agent harness can use IoT too**. CTOX already exposes its domain capabilities
to workers as auditable CLI subcommands — `ctox queue`, `ctox ticket`,
`ctox verification`, `ctox knowledge`, `ctox process-mining` (`src/core/main.rs`;
HARNESS.md: "workers can call CTOX commands themselves … through an auditable
command surface"). IoT joins that surface. Symmetric read/act for both apps and
the agent, plus an event→work trigger:

**1. Agent → IoT (act).** New `ctox iot <subcmd>` in `src/core/main.rs`, routed to
`iot::handle_iot_command(&root, &args)` exactly like `queue::handle_queue_command`:

```text
ctox iot asset    list | show | upsert | delete
ctox iot attribute read | write
ctox iot datapoints query
ctox iot alarm    list | ack | assign | resolve
ctox iot rules    list | save | toggle
ctox iot agent    list | configure | status
```

The agent invokes these through the harness **shell tool** — no new hard-coded
Responses tool required (the same way it already runs `ctox ticket`). Optionally
register an `iot` entry in `src/core/capabilities/` alongside
`web`/`scrape`/`doc`/`browser` for capability gating and discovery.

**1b. Agent skill (the "skill" surface).** A CTOX skill teaches the agent *when and
how* to use the IoT capability — the CLI alone is necessary but not sufficient.
Add `src/skills/system/host_ops/iot-operations/SKILL.md` (same shape as the
existing `host_ops` skills like `acceptance-verification`, `incident-response`;
skills are embedded at compile-time and imported into SQLite at service start via
`src/core/skill_store.rs`). The skill documents: the `ctox iot` command map, when
to read vs. write attributes, how to interpret alarms/rulesets, the safety rules
(never write a device attribute without an explicit task/approval), and the
read→reason→act loop. This is what makes IoT a *usable* agent capability, not just
an available one — and it composes with the IoT→agent trigger (surface 3) so a
worker spun up from an alarm already knows the playbook.

**2. App → IoT (act).** The Business OS module dispatches `ctox.iot.*`
`business_commands` (§5.2). The command handler and the CLI **both delegate to
the same `iot::` functions** — one code path, two entry points, identical
validation and ACL.

**3. IoT → Agent (trigger) — through CTOX's existing brain, not a parallel engine.**
The thin condition layer (§2.1/§2.2) only *evaluates attribute predicates* and
emits **durable work**: an alarm and/or a queue task / message. From there CTOX's
**own** machinery owns scheduling, dedup, recurrence, and loop-bounding —
`schedule.rs`, `queue.rs`, `mission_governor.rs`, and the durable-spawn **budget**
(`core_transition_guard.rs`, recorded in `ctox_core_spawn_edges`). The registered
contract family is e.g. `iot-event-queue-task` (parent `IotAlarm` → child
`QueueTask`, finite budget) / `iot-event-message`; the budget is what bounds
re-firing (replacing OpenRemote's 100-trigger cap). So there is exactly **one**
automation brain. The loop: temperature breach → condition matches → alarm +
queue task → mission loop leases it → agent diagnoses (guided by the IoT skill) →
writes an attribute back through the same engine — all under CTOX's review,
verification, and spawn-budget gates.

Net effect: a worker can read live sensor state, reason over it, and command a
device within one bounded turn; the same actions and state are live in the
Business OS app; and IoT conditions can originate autonomous CTOX work. The CLI
and the `business_commands` handler are thin adapters over `iot::`, so app and
agent never diverge.

## 5. Data contract

### 5.1 RxDB collections (engine → browser, read-only in UI)

| Collection | Holds |
| --- | --- |
| `iot_realms` | scopes/tenants (mapped to CTOX ACL) |
| `iot_asset_types` | descriptor registry: type → attribute schema/meta |
| `iot_assets` | asset records: id, parentId, type, name, realm, location, attribute summary |
| `iot_attributes` | latest value + timestamp per (assetId, attributeName) |
| `iot_datapoints` | **windowed/downsampled** history projections (LTTB), pulled on demand |
| `iot_alarms` | alarm records + lifecycle state |
| `iot_rulesets` | when-then rulesets + enabled state + last-fire evidence |
| `iot_agents` | configured protocol agents + connection/link status |
| `iot_agent_status` | engine-internal: link health, last-event ts, error surface |

Derivation discipline per `ARCHITECTURE.md` JSON-native records: canonical JSON
in `data`, plus light `index_text`/`sort_key`/`status_key` for search/sort/filter.
Full time-series stays in the authoritative SQLite store; only bounded windows
are projected (no silent caps — truncation is logged).

### 5.2 Command families (browser → engine, via `business_commands`)

| `command_type` | Engine action |
| --- | --- |
| `ctox.iot.attribute.write` | write/command an attribute value (→ agent if device-backed) |
| `ctox.iot.asset.upsert` / `.delete` | create/update/delete asset |
| `ctox.iot.alarm.update` | ack / assign / resolve alarm |
| `ctox.iot.ruleset.save` / `.toggle` | create/update / enable/disable when-then ruleset |
| `ctox.iot.agent.configure` | add/configure/remove a protocol agent (MQTT/HTTP/WS) |
| `ctox.iot.datapoints.query` | request a windowed datapoint backfill into `iot_datapoints` |

Standard lifecycle: `pending_sync` → engine executes → writes
`completed`/`failed` + `payload.outcome` back on the command doc; UI awaits via
the existing projection-poll pattern (`waitForBusinessCommandProjection`). ACL
checked before execution.

---

## 6. Phased delivery

Each phase ends committable to `main`, `cargo fmt --check`/`check`/`test` clean,
and (where UI is touched) carries a `test.mjs`. The release gate
`ctox process-mining spawn-liveness` must stay green throughout.

> **Execution:** each phase below is meant to be run as **one Claude Code
> Workflow** (fan-out → verify → integrate), staying in the loop between phases.
> The decomposition, isolation, and verification model — plus runnable workflow
> scripts — are in **§11**.

### Phase 0 — Decision record & licensing (1–2 days)
- One-page RFC in `docs/rfcs/`: confirm native-port scope, the deferred list (§2), naming/trademark (`iot`, OpenRemote credited), dependency policy (native Rust, vendor protocol clients like `whatsapp_rust`).
- `docs/legal/NOTICE`: OpenRemote AGPL-3.0 attribution for ported domain logic.
- **Exit:** scope and naming locked.

### Phase 1 — Native IoT domain core (2–3 weeks)
- `src/core/iot/{mod,model,store,datapoints,alarms}.rs`: asset/attribute model + descriptor registry, authoritative SQLite schema in `ctox.sqlite3`, datapoint store + LTTB downsampler (`// ref:` ported), alarm store.
- Unit/integration tests for model round-trip, datapoint windowing, alarm lifecycle.
- **Exit:** engine can hold assets/attributes/datapoints/alarms with no UI and no devices yet; tests green.

### Phase 2 — Collections + command path + CLI (harness-usable) (1–2 weeks)
- Register `iot_*` collections (schemas + contract + hashes + parity guard).
- `projector.rs`: project engine state into the collections.
- `ctox.iot.*` command family wired into the executor + projection branch; ACL-gated; idempotent with a retry budget.
- **`ctox iot <subcmd>` CLI** in `main.rs` → `iot::handle_iot_command` (§4A surface 1); the `business_commands` handler and the CLI share the same `iot::` functions. Optional `capabilities/iot.rs` registration.
- Integration test: dispatch `ctox.iot.asset.upsert` / `ctox.iot.attribute.write`, observe projection echo; CLI test: `ctox iot attribute write …` then `ctox iot attribute read` round-trips.
- **Exit:** full read+write loop through both RxDB/`business_commands` **and** the CLI; the harness can already read/act on IoT state, still headless UI.

### Phase 3 — Native protocol agents (2–3 weeks)
- `src/core/iot/adapters.rs` (`IotAgent` trait) + `gateway.rs` + `runtime.rs` (spawned agent loops).
- `agents/mqtt_native.rs` first (vendored/forked native MQTT client à la `whatsapp_rust`), then `http_native.rs`, then `ws_native.rs`.
- Agents map inbound device data → attribute updates → datapoints; outbound `attribute.write` → device.
- Reconnect/backfill on agent restart or WebRTC flap (parallels `src/core/rxdb/revisions/` flap work).
- **Exit:** a real MQTT device round-trips through the engine into `iot_attributes`/`iot_datapoints`.

### Phase 4 — Thin condition layer + route into CTOX's mission loop (1.5 weeks)
- `conditions.rs`: a **condition evaluator only** (`// ref:` `JsonRulesBuilder` predicates + duration windowing). It evaluates attribute predicates over events and produces *matches* — it does NOT contain a firing loop, recurrence timers, or jeasy-rules. Covers §2A.16/18/19/22/23.
- Emit on match: raise an `iot_alarms` entry and/or emit durable work. Bridge alarms into the existing notification path (`shared/notifications.js`, `mission/channels.rs`) so IoT surfaces like other CTOX activity.
- **Route into CTOX's existing brain (§4A surface 3):** scheduling/recurrence via `schedule.rs`, dedup via `queue.rs`, loop-bounding via the durable-spawn **budget**. Register the `iot-event-queue-task` / `iot-event-message` contract family in the spawn-guard registry. Re-firing is bounded by budget, not a ported 100-trigger cap.
- Verify `ctox process-mining spawn-liveness` proves the IoT spawn family bounded.
- **Exit:** a live attribute event matching a condition → alarm/notification **and**, for flagged conditions, a durable queue task the agent picks up via `ctox iot` — with no second automation engine running (one mission brain).

### Phase 5 — Business OS module, read-only (2 weeks)
- `modules/iot/` per §4: `module.json`, `schema.js`, `index.html`, `index.js` (`mount(ctx)`), `index.css` (shell tokens only), `icon.svg`, `locales/`, `test.mjs`.
- 3-pane + drawers; asset tree, attribute workbench, native chart over `iot_datapoints`, alarms/agents context. Reads `iot_*` only; honest not-ready state.
- App Store catalog entry; install/uninstall flow.
- Visual parity pass against `customers`/`shiftflow` (spacing, tokens, density).
- **Exit:** installs from the store, renders live engine data, `test.mjs` green.

### Phase 6 — Interactive control (1–2 weeks)
- Wire UI actions to §5.2 commands; optimistic UI + command-projection await; ACL-aware affordances; surface failures from command outcomes.
- Ruleset editor (React-embedded dense form is acceptable here).
- **Exit:** operator drives assets/attributes/alarms/rules/agents end-to-end from Business OS.

### Phase 7 — Production hardening & release (1–2 weeks)
- Backpressure/bounds on event fan-out and datapoint windows (logged truncation).
- Multi-realm isolation via session + `business_module_acl`.
- Secrets handling for agent credentials; redact from any support bundle.
- Observability: `iot_agent_status` panel; emit harness-flow/process-mining events where useful.
- CI soak job modeled on `.github/workflows/rxdb-soak.yml`: spin a local MQTT publisher, assert live projection + command round-trips + reconnect.
- Operator docs, store listing, release notes; tag `vX.Y.Z` on `main` for the Actions release.
- **Exit:** §8 checklist passes.

Rough total: ~10–15 weeks for one engineer. The native protocol agents (Phase 3)
and the domain core (Phase 1) are the load-bearing risk; the module (Phases 5–6)
is well-trodden given the existing apps.

---

## 7. Files this touches

New:
- `src/core/iot/{mod,model,store,datapoints,alarms,conditions,adapters,gateway,runtime,projector}.rs` (`conditions.rs` = thin predicate layer, NOT a rules engine — firing/scheduling reused from CTOX mission core)
- `src/core/iot/agents/{mqtt_native,http_native,ws_native}.rs` (+ a vendored MQTT client crate if from-scratch)
- `src/apps/business-os/modules/iot/*` (full module)
- `.github/workflows/` — IoT soak/integration job
- `docs/rfcs/` decision record; `docs/legal/NOTICE` OpenRemote attribution

Changed:
- `src/core/business_os/rxdb_peer.rs` — register `iot_*` collections (`:5159`); add `ctox.iot.*` routing (`:2102`)
- `src/core/business_os/store.rs` — `ctox.iot.*` execution in `accept_rxdb_business_command`
- `src/core/main.rs` — `ctox iot <subcmd>` dispatch → `iot::handle_iot_command` (harness surface)
- `src/core/capabilities/{mod,iot}.rs` — optional `iot` capability registration alongside `web`/`scrape`/`doc`/`browser`
- `src/core/service/core_transition_guard.rs` (+ spawn-contract registry) — `iot-event-*` durable-spawn contract family
- `business_os_schema_contract.json` + `business_os_schema_hashes.json` — new schemas
- `src/apps/business-os/modules/registry.json` — register `iot` module (if not generated)
- service boot — spawn IoT agent loops + projector

Analysis-only: `archive/openremote` (ignored). No Java/Docker enters the repo.

---

## 8. Production-readiness checklist

- [ ] Backend is native Rust only — no JVM, no Docker, no external broker/DB/Keycloak.
- [ ] No new process env vars; config/secrets via `runtime_env::env_or_config` + secret store.
- [ ] Authoritative state in `runtime/ctox.sqlite3`; no separate database.
- [ ] Collections registered + schema contract + hashes updated; parity guard green.
- [ ] `ctox.iot.*` commands ACL-gated, idempotent, bounded retries.
- [ ] No HTTP data bridge browser↔CTOX; module reads `iot_*`, writes only `business_commands`; never imports `rxdb`.
- [ ] IoT is harness-usable: `ctox iot` CLI works and shares the exact `iot::` code path as `ctox.iot.*`; IoT→agent durable-spawn contract registered and provably bounded.
- [ ] Module uses only shell tokens + shared helpers; visually matches `customers`/`shiftflow`; no `@openremote/*`, no bundler.
- [ ] Honest "sync not ready" state; no fallback data over HTTP.
- [ ] Event/datapoint volume bounded; truncation logged, not silent.
- [ ] Agent reconnect/backfill after restart / WebRTC flap.
- [ ] Ported algorithmic logic carries `// ref:` anchors; OpenRemote AGPL attribution recorded; trademark not implied.
- [ ] `cargo fmt --check`, `cargo check`, `cargo test` clean; `test.mjs` green; `ctox process-mining spawn-liveness` green.
- [ ] CI soak job asserts live projection + command round-trip + reconnect.
- [ ] Every §2A edge case has a passing §12 test before its phase is marked done (port-fidelity gate — not happy-path only).
- [ ] All P0 user stories U1–U11 pass as scripted end-to-end acceptance (§13) before Freigabe; deferred scope (§2.4) labeled as deferred in the store listing.

---

## 9. Risks & scope honesty

1. **Scope.** OpenRemote is 157k LOC; only ~30k is in scope (§2). This plan ships
   the IoT *core* (assets, telemetry, MQTT/HTTP/WS agents, alarms, attribute
   conditions) and defers Groovy/JS/Flow rules, 18 protocol agents, gateway
   federation, provisioning, dashboard builder, and the asset-class long tail
   (§2.4). The deferred list is explicit, not silent.
2. **The two real fidelity risks** are (a) the **asset-processing edge cases**
   (§2A.1–8: timestamp/outdated/coercion/locking semantics — easy to get subtly
   wrong) and (b) the **native protocol agents** (§2A.24–30: reconnect/resubscribe
   state machine). MQTT follows the `whatsapp_rust` precedent (self-contained
   vendored client). Both are guarded by the §12 edge-case tests.
3. **Rules-engine decision reduces risk.** By porting only the condition layer and
   routing into CTOX's mission loop (§2.2), we avoid reimplementing a firing loop /
   recurrence / loop-detection — the historically bug-prone part — and keep one
   automation brain. Trade-off: condition expressiveness at v1 = attribute
   predicates + duration; richer logic lives in CTOX plans/agent, not a DSL.
4. **Dynamic asset model** — the UI must render generic typed attributes without
   hardcoding asset classes.
5. **Event firehose** — high-frequency telemetry must be coalesced engine-side;
   only last-value + windowed history reach RxDB.
6. **Charts/maps without `@openremote/*`** — reuse whatever the existing modules
   already vendor; do not add a framework.
7. **AGPL** — ported domain logic is a derivative of OpenRemote; attribution in
   `NOTICE`, `// ref:` anchors on ported functions.

---

## 10. Non-goals

- Running or bridging to OpenRemote's Java backend, broker, Keycloak, or Postgres.
- Any Docker/external-process dependency.
- Re-hosting OpenRemote's LitElement UI or using `@openremote/*` components.
- Mirroring a full time-series database into RxDB.
- Any HTTP pull/push command bridge for module data.
- Claiming the OpenRemote name/brand for the CTOX product surface.

---

## 11. Workflow execution model (Claude Code)

This plan is built to be executed with the **Workflow** tool: **one workflow per
phase**, run in sequence, with the operator reading each result before launching
the next. Each phase workflow is a fan-out → verify → integrate graph.

### 11.1 Three kinds of work unit

Every phase is decomposed into exactly these, which determines what may run in
parallel:

| Kind | Examples | Parallel? | Isolation |
| --- | --- | --- | --- |
| **A. Independent new files** | the 3 protocol agents, `iot_*` JSON schemas, locales, icon, per-asset-type descriptors | yes — fan out | `isolation:'worktree'` (they mutate disk concurrently) |
| **B. Shared-registration edits** | `rxdb_peer.rs` (collections + routing), `store.rs` (executor), `main.rs` (CLI), `business_os_schema_contract.json` + hashes, `registry.json`, `core_transition_guard.rs` | **no — single serial integrator** | one agent, no worktree |
| **C. Verification** | adversarial multi-lens review, build/test gate, schema-parity, spawn-liveness | yes — fan out | read-only reviewers |

The rule that prevents merge hell: **fan-out (A) agents create only their own
file and must not touch any (B) file.** A single **integrator** agent at the end
of each phase adds the `mod`/`use` declarations, registrations, and contract
regen, then runs `cargo check`/`test`. This is why (B) is never parallelized.

### 11.2 Per-phase parallelism map

| Phase | Fan-out (A, parallel/worktree) | Serial integrator (B) | Verify (C) | Barrier? |
| --- | --- | --- | --- | --- |
| 0 RFC | 3 read-only: license/NOTICE, naming, scope | write RFC | — | synth |
| 1 engine core | research ×4 (Explore) → 1 coherent impl (coupled code, single author) | reconcile `mod.rs` | 4 lenses: LTTB-correctness, guardrails, house-style, tests | design synth; verify |
| 2 collections+cmd+CLI | 9 `iot_*` schemas + `projector.rs` (new) | `rxdb_peer.rs`, `store.rs`, `main.rs` (all serial) | command round-trip + CLI round-trip + schema-parity | contract regen |
| 3 protocol agents | **mqtt/http/ws ×3** | `gateway.rs` + `IotAgentKind` + `agents/mod.rs` | per-agent: protocol, reconnect, secrets | none (pipeline) |
| 4 conditions + trigger | `conditions.rs` (new, thin) | notif bridge + `core_transition_guard.rs` spawn contract + `schedule.rs`/`queue.rs` routing | condition-match test + spawn-budget bound + **`spawn-liveness`** | none |
| 5 module read-only | locales(en/de), icon, `schema.js`, `test.mjs` (independent) + 1 module author | `registry.json` | token-only CSS, no-`rxdb`-import, mount-contract, visual parity vs `customers`/`shiftflow` | module author before reviewers |
| 6 interactive | ruleset editor (React form) | module `index.js` action wiring | each `ctox.iot.*` action smoke | none |
| 7 hardening+release | audits ×4: backpressure, ACL/multi-realm, secret-redaction, soak-job | apply fixes; release notes | CI soak green; `spawn-liveness` green | audits before fixes |

Honest note: Phase 1's engine core is **tightly coupled** (`store` ↔ `model` ↔
`datapoints`/`alarms` share types and `mod.rs`), so the win there is parallel
*research + verification*, not parallel writing — one author implements the
coherent unit. The real write-time parallelism is Phase 3 (3 agents), Phase 5
(independent assets), and Phase 7 (independent audits).

### 11.3 Guardrails injected into every agent prompt

Every implementing agent prompt must carry the CLAUDE.md hard rules so a
subagent can't drift: native Rust only (no JVM/Docker/wrapper); state in
`runtime/ctox.sqlite3`; config/secrets via `runtime_env::env_or_config` + secret
store (no `std::env`); no HTTP data bridge; module reads `iot_*` + writes only
`business_commands`, never imports `rxdb`; CSS uses only shell tokens. Reviewers
default to `pass=false` when uncertain.

### 11.4 Runnable script — Phase 3 (the clean fan-out shape)

```js
export const meta = {
  name: 'iot-phase3-protocol-agents',
  description: 'Implement + verify the 3 native IoT protocol agents in parallel, then integrate',
  phases: [{ title: 'Implement' }, { title: 'Verify' }, { title: 'Integrate' }],
}
const GUARD = 'Native Rust only; no env vars (secrets via CTOX secret store); no HTTP-to-browser; create ONLY your file (+ vendored crate dir if needed); do NOT touch gateway.rs/mod.rs/main.rs/schema — the Integrate stage owns shared files.'
const AGENTS = [
  { id: 'mqtt', file: 'src/core/iot/agents/mqtt_native.rs', note: 'native MQTT client; vendor/fork a self-contained client like whatsapp_rust if from-scratch; pin upstream commit' },
  { id: 'http', file: 'src/core/iot/agents/http_native.rs', note: 'HTTP poll/push agent' },
  { id: 'ws',   file: 'src/core/iot/agents/ws_native.rs',   note: 'WebSocket agent' },
]
const UNIT = { type:'object', required:['file','buildPassed','notes'], properties:{ file:{type:'string'}, buildPassed:{type:'boolean'}, notes:{type:'string'} } }
const VERDICT = { type:'object', required:['pass','findings'], properties:{ pass:{type:'boolean'}, findings:{type:'array', items:{type:'string'}} } }

// each agent: implement (own worktree) -> adversarial review. Independent -> pipeline, no barrier.
const reviewed = await pipeline(
  AGENTS,
  a => agent(`Implement the ${a.id} IoT protocol agent at ${a.file} against the IotAgent trait in src/core/iot/adapters.rs. ${a.note}. ${GUARD} Run \`cargo check\` on your file; report pass/fail.`,
    { label: `impl:${a.id}`, phase: 'Implement', schema: UNIT, isolation: 'worktree' }),
  (unit, a) => agent(`Adversarially review the ${a.id} agent (${a.file}): protocol correctness, reconnect/backfill, secrets handling, no env reads. Find a real defect or pass=true; default pass=false if uncertain.`,
    { label: `verify:${a.id}`, phase: 'Verify', schema: VERDICT })
    .then(v => ({ ...a, unit, verdict: v })),
)

// one SERIAL integrator owns all shared files
phase('Integrate')
const integ = await agent(
  `Integrate the 3 IoT protocol agents: add IotAgentKind variants, register them in src/core/iot/gateway.rs, declare modules in agents/mod.rs + src/core/iot/mod.rs, fix any review findings, then run cargo check + cargo test. Reviewed: ${JSON.stringify(reviewed)}`,
  { label: 'integrate:agents', phase: 'Integrate' })
return { reviewed, integ }
```

### 11.5 Runnable script — Phase 1 (coupled core: research/verify fan out, writing doesn't)

```js
export const meta = {
  name: 'iot-phase1-engine-core',
  description: 'Implement + verify the native IoT engine core (model, store, datapoints, alarms)',
  phases: [{ title: 'Research' }, { title: 'Design' }, { title: 'Implement' }, { title: 'Verify' }, { title: 'Integrate' }],
}
const MAP = { type:'object', required:['summary','keyFiles','algorithm'], properties:{ summary:{type:'string'}, keyFiles:{type:'array',items:{type:'string'}}, algorithm:{type:'string'} } }
const DIFF = { type:'object', required:['filesWritten','buildPassed','testPassed','notes'], properties:{ filesWritten:{type:'array',items:{type:'string'}}, buildPassed:{type:'boolean'}, testPassed:{type:'boolean'}, notes:{type:'string'} } }
const VERDICT = { type:'object', required:['pass','findings'], properties:{ pass:{type:'boolean'}, findings:{type:'array',items:{type:'string'}} } }

phase('Research')   // read-only fan-out over upstream + CTOX house style
const maps = (await parallel([
  () => agent('Read archive/openremote model/.../asset/*Asset.java + AssetModelResource. Map the asset/attribute/value model + dynamic descriptor resolution.', { label:'or:model', phase:'Research', schema:MAP, agentType:'Explore' }),
  () => agent('Read archive/openremote AssetDatapointResource + AssetDatapointLTTBQuery. Describe datapoint storage + LTTB downsampling precisely enough to port line-for-line.', { label:'or:datapoints', phase:'Research', schema:MAP, agentType:'Explore' }),
  () => agent('Read archive/openremote AlarmResource + alarm model. Describe the alarm lifecycle (open/ack/assign/resolve).', { label:'or:alarms', phase:'Research', schema:MAP, agentType:'Explore' }),
  () => agent('Read src/core/business_os/store.rs + src/core/communication/{adapters,gateway,runtime}.rs. Map how CTOX structures a native subsystem + SQLite store so src/core/iot matches house style.', { label:'ctox:patterns', phase:'Research', schema:MAP, agentType:'Explore' }),
])).filter(Boolean)

phase('Design')
const spec = await agent(`From these maps, write a concrete impl spec for src/core/iot/{mod,model,store,datapoints,alarms}.rs: Rust types, SQLite tables, fn signatures, and // ref: anchors to OpenRemote sources. Honor CLAUDE.md guardrails. Maps:\n${JSON.stringify(maps)}`, { label:'design:core', phase:'Design' })

phase('Implement')  // coupled code -> single author, isolated worktree
const built = await agent(`Implement src/core/iot/{mod,model,store,datapoints,alarms}.rs per the spec. Create ONLY these files; do NOT edit rxdb_peer.rs/main.rs/schema/registry (Integrate owns shared files). Add unit tests (model round-trip, datapoint windowing, alarm lifecycle). Run cargo check + cargo test; report. Spec:\n${spec}`, { label:'impl:core', phase:'Implement', schema:DIFF, isolation:'worktree' })

phase('Verify')     // multi-lens adversarial fan-out
const lenses = ['LTTB-correctness vs OpenRemote', 'guardrails: no std::env, no HTTP, SQLite-only state', 'house-style vs src/core/communication', 'test coverage actually exercises model/datapoints/alarms']
const verdicts = (await parallel(lenses.map(l => () =>
  agent(`Adversarially review the IoT engine core. Lens: ${l}. Find a real defect or pass=true; default pass=false if uncertain.`, { label:`verify:${l.split(':')[0].slice(0,12)}`, phase:'Verify', schema:VERDICT })))).filter(Boolean)
const findings = verdicts.filter(v => !v.pass).flatMap(v => v.findings)

phase('Integrate')
const result = await agent(findings.length
  ? `Fix these findings in the IoT engine core, reconcile mod.rs, re-run cargo check/test: ${JSON.stringify(findings)}`
  : `No findings. Confirm cargo check/test green and summarize the engine core.`, { label:'integrate:core', phase:'Integrate', schema:DIFF })
return { maps, built, verdicts, result }
```

The other phases follow the same two templates: **§11.4 (independent fan-out +
serial integrator)** for Phases 3/5/7, **§11.5 (research/verify fan out, single
author writes the coupled unit)** for Phases 1/2/4/6.

### 11.6 Scale, cost, cadence

- **Opt-in & cost.** Workflows spawn many agents and spend real tokens; run them only on explicit request, one phase at a time. Rough per-phase agent counts: Phase 0 ~4, Phase 1 ~10, Phase 2 ~8, Phase 3 ~7, Phase 4 ~6, Phase 5 ~9, Phase 6 ~6, Phase 7 ~8.
- **Wall-clock.** Parallelism compresses Phases 3/5/7 and all verification; it does **not** compress the coupled Phase 1 write. So the §6 calendar estimate is roughly unchanged for the core and shorter for the fan-out phases.
- **Resume.** If a phase workflow is edited or interrupted, relaunch with `resumeFromRunId` so the unchanged agent prefix returns from cache.
- **Stay in the loop.** Read each phase's returned `{ verdicts, result }` before launching the next phase; do not chain all phases into one mega-workflow.

---

## 12. Unit-test specification (per subsystem)

Tests are not an afterthought — for a port, the test suite **is** the proof of
fidelity. Every numbered edge case in §2A maps to at least one test. Targets are
Rust `#[cfg(test)]` unit/integration tests plus the module `test.mjs`, and a CI
soak job. Coverage gate: the edge-case tests below must exist and pass before a
phase is "done".

**Engine — asset/attribute (Phase 1):**
- model round-trip (asset + typed attributes + meta + descriptors) through the SQLite store.
- §2A.1–2: timestamp `0` vs explicit; missing/future timestamp clamping.
- §2A.3: outdated event → current state unchanged **and** datapoint recorded.
- §2A.4: shallow vs deep equality.
- §2A.5: coercion failure rejects event.
- §2A.6: descriptor meta merged once, not re-merged.
- §2A.7: concurrent writers to one asset serialize (no lost update) — property/loom-style or threaded test.
- §2A.8: lazy hydration of an unparsed value.

**Engine — datapoints/LTTB (Phase 1):**
- LTTB golden tests vs. a reference implementation: empty, single point, two points, n≫buckets; assert first/last retained and bucket selection matches reference (§2A.9–10).
- interval bucketing + gap-fill + LOCF (§2A.11); nearest `<=` semantics (§2A.12).
- UTC/epoch normalization (§2A.13); query-limit truncation logged (§2A.14).

**Engine — attribute conditions + mission routing (Phase 4):**
- condition layer (ported): duration windowing reset on predicate-false + all-predicates-hold (§2A.16); `otherwise`/unmatched (§2A.18); stale-AND reset (§2A.19); de-dup-then-order/limit (§2A.22); predicate operators incl. `previousValue` (§2A.23).
- delegated behavior (CTOX-native, not a ported loop — assert the *observable* behavior): re-trigger suppression via queue dedup (§2A.15); recurrence PER_ASSET/GLOBAL/`null/0/>0` via `schedule.rs` (§2A.17); re-firing bounded by spawn-budget, proven by `spawn-liveness` (§2A.20); startup suppression via the mission-start guard (§2A.21).
- routing: a matched condition emits exactly one durable queue task per dedup key; the `iot-event-*` spawn family is provably bounded.

**Engine — protocol agents (Phase 3):**
- reconnect backoff curve + jitter bounds + state-machine transitions (§2A.24) — deterministic via injected clock (no `Date.now`; pass time in).
- resubscribe-on-session-loss all vs failed-only (§2A.25); QoS preserved, Last-Will, retain (§2A.26).
- link/unlink during reconnect is safe (§2A.27).
- value filters/converters + `%VALUE%`/`%TIME%` placeholders (§2A.28).
- HTTP min-interval, fixed-delay, 2xx-only, pagination (§2A.29); fire-and-forget vs `updateOnWrite` (§2A.30).
- Agents tested against a **local broker/HTTP fixture** (loopback test double), not a real device.

**Integration / boundary (Phases 2,5,6):**
- `ctox.iot.*` command round-trips through `business_commands` → engine → projection echo.
- `ctox iot` CLI round-trips share the exact engine path as the command (assert identical result for the same op via both surfaces).
- schema-parity guard green; module reads only `iot_*`, never imports `rxdb`; CSS resolves only shell tokens.
- IoT→agent durable-spawn contract proven bounded by `ctox process-mining spawn-liveness`.

**CI soak (Phase 7):** boot the engine + a local MQTT publisher, drive N attribute
events, assert live projection, command round-trip, rule fire, alarm→queue-task,
and reconnect after a forced disconnect.

---

## 13. User stories & end-to-end acceptance (release gate)

End-to-end **Freigabe** requires these user stories to pass as scripted
acceptance tests (operator-level, through the real surfaces). Each maps to the
unit tests above; a release is blocked if any P0 story fails.

**Operator (Business OS app):**
- **U1 (P0)** As an operator I add an MQTT agent, point it at a broker, and see its connection go green and devices appear as assets. *Accept:* `iot_agents.status=connected`, assets visible in the tree within the reconnect window.
- **U2 (P0)** I open an asset and see its live attribute values update in real time. *Accept:* a published MQTT value appears in the center pane ≤ projection latency budget.
- **U3 (P0)** I view an attribute's history chart over a time window. *Accept:* LTTB-downsampled series renders; matches engine query for the same window.
- **U4 (P0)** I write a value to a writable attribute and the device receives it. *Accept:* `ctox.iot.attribute.write` → broker sees the publish; optimistic UI reconciles with the echo.
- **U5 (P1)** I create a when-then rule ("if temperature > X for 5 min, raise an alarm") and it fires. *Accept:* alarm appears; duration windowing respected; no re-fire until reset.
- **U6 (P1)** I acknowledge/resolve an alarm. *Accept:* `iot_alarms` lifecycle transitions; ACL enforced.
- **U7 (P1)** When sync/engine is down, the app says so honestly — no stale fallback data. *Accept:* not-ready state shown; no HTTP fetch.

**Agent / harness:**
- **U8 (P0)** A worker runs `ctox iot asset list` / `attribute read` and reasons over live state within one bounded turn. *Accept:* CLI returns engine truth; same result as the app surface.
- **U9 (P0)** A worker writes a device attribute via `ctox iot attribute write` **only** under an explicit task/approval (per the skill). *Accept:* write reaches the device; unauthorized write is refused.
- **U10 (P1)** A rule alarm spawns a durable queue task; the agent leases it, diagnoses, and acts (e.g. writes a setpoint), guided by the IoT skill. *Accept:* spawn edge recorded in `ctox_core_spawn_edges`, bounded; task closes via the review/outcome gates.
- **U11 (P2)** The agent discovers IoT capability via the skill and the `ctox iot` help without prior context. *Accept:* skill present in catalog; help is self-describing.

**Cross-cutting acceptance (all P0):** no `std::env` reads for runtime state; no
HTTP data bridge browser↔CTOX; `cargo fmt/check/test` + `test.mjs` +
`spawn-liveness` green; AGPL attribution + trademark notice present.

> **Scope honesty for Freigabe:** "production ready" means the **in-scope core**
> (assets, attributes, datapoints+LTTB, MQTT/HTTP/WS agents, alarms, when-then
> rules) passes U1–U11 with the §2A edge cases covered by §12 tests. The §2.4
> deferred items (Groovy/JS/Flow rules, the other 18 protocols, forecasting,
> gateway) are **not** part of this release and are documented as such in the
> store listing — so nobody mistakes deferred scope for broken scope.

```