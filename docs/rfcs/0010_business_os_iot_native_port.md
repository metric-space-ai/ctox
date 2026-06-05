# RFC 0010: Business OS Native IoT (OpenRemote Domain Port)

**Status:** Phase 0 decision record — scope and naming locked
**Date:** 2026-06-04
**Affects:** `src/core/iot/` (new subsystem),
`src/apps/business-os/modules/iot/` (new module),
`src/core/business_os/rxdb_peer.rs`,
`src/core/business_os/store.rs`,
`src/core/main.rs`,
`src/core/capabilities/`,
`src/core/service/core_transition_guard.rs`,
`src/core/business_os/business_os_schema_contract.json`,
`src/apps/business-os/modules/registry.json`,
`docs/legal/NOTICE`

**Implementation Plan:** `docs/business-os-openremote-iot-port-plan.md`

**Porting source:** `openremote/openremote`
(cloned for analysis into `archive/openremote`, AGPL-3.0, HEAD `22a42a7`)

## 1. Decision

CTOX Business OS gets a first-party **IoT** capability for asset, telemetry,
alarm, and attribute-condition work. The capability is delivered by porting
OpenRemote's IoT **domain** into a native CTOX Rust subsystem at `src/core/iot/`.
There is **no external runtime**: no OpenRemote Java backend, no JVM, no Docker,
no Keycloak, no PostgreSQL/TimescaleDB, and no MQTT broker dependency.

This is the same discipline CTOX already applies to the Codex harness
(`src/core/harness/`), the native RxDB peer (`src/core/rxdb/`), and the WhatsApp
client (`whatsapp_rust/`): hard-fork the *concepts and algorithms*, reimplement
them as native, self-contained Rust, with authoritative state in
`runtime/ctox.sqlite3`.

OpenRemote is used as the **porting source** for IoT domain semantics:

- the asset / attribute / value model and descriptor registry
- the attribute-processing flow (timestamp, outdated-event, coercion semantics)
- datapoint query semantics and LTTB downsampling behavior
- the alarm lifecycle
- attribute-condition predicate evaluation (the *condition* layer only)
- the protocol-agent abstraction and the MQTT/HTTP/WebSocket client agents

OpenRemote is **not** claimed as a brand, and its Java framework, Keycloak,
Postgres/Timescale, MQTT broker service, and 16 of its 19 protocol-agent
subdirectories are explicitly out of scope (see §6, §7).

This RFC is the Phase 0 deliverable from the implementation plan §6. Its job is
to **lock scope and naming** so later phases cannot quietly redraw the boundary.

### What this RFC LOCKS

1. Native-Rust port of the OpenRemote IoT domain into `src/core/iot/`, no
   external runtime (plan §1 NOT-doing list — §2 below).
2. One engine, three surfaces — app `business_commands`, agent `ctox iot` CLI,
   and an IoT skill — all over the same `iot::` code (plan §4A — §3 below).
3. A **thin condition layer, not a second automation engine** — firing,
   scheduling, dedup, recurrence, and loop-bounding are reused from CTOX's
   mission core (plan §2.2 — §4 below).
4. Naming and trademark stance: subsystem `iot`, collections `iot_*`, commands
   `ctox.iot.*`; OpenRemote credited as porting source only (plan naming — §5
   below).
5. Dependency policy: native Rust, vendored/forked protocol clients with pinned
   upstream commits, no heavy inference/runtime framework (§6 below).
6. The DEFERRED list (plan §2.4) and the DROP list (plan §2.3) so deferred is
   visibly *not* broken (§7 below).
7. The §2A edge-case inventory and §12 test spec are **binding acceptance
   gates** (§8 below).

## 2. Native Port, No External Runtime

The decision is to port the domain, not to wrap or bridge a foreign runtime.
The following are **NOT** part of the architecture, by design (plan §1):

- No OpenRemote Java `manager`, no Gradle, no JVM.
- No Docker / docker-compose / external process to bridge to.
- No Keycloak — CTOX identity is `business_users` + `business_module_acl`.
- No PostgreSQL / TimescaleDB — authoritative state lives in
  `runtime/ctox.sqlite3` via `paths::core_db(root)`.
- No ActiveMQ broker dependency — MQTT is a native Rust *client* agent
  (connect out to devices), never a broker we host.
- No `@openremote/*` LitElement components, no bundler in the browser module.
- No HTTP data bridge browser↔CTOX — data crosses the boundary only via RxDB
  `iot_*` projections (read) and `ctox.iot.*` `business_commands` (write), per
  the CTOX data-boundary rule (`README.md`; memory `feedback_no_http_sync_path`).

The honest framing of the port (plan §2): OpenRemote is 157,633 Java LOC; only
~30k LOC is in scope. Of that, the domain *semantics/algorithms* port faithfully
(that is where the edge cases live), while the *persistence and transport
framework* is necessarily reimplemented on CTOX-native primitives (SQLite,
native event dispatch). The remaining ~125k LOC is out of scope by design, not
"skipped" (§7).

## 3. One Engine, Three Surfaces

IoT is a real CTOX capability the agent harness can use, not just an app
feature. The architecture is **one native engine with three consumer surfaces**,
all calling the same `iot::` functions; none re-implements domain logic (plan
§4A).

1. **Engine (backend)** — the native Rust subsystem `src/core/iot/`, modeled on
   the existing `src/core/communication/` connector framework: asset model,
   datapoint store, native protocol agents (MQTT/HTTP/WebSocket), alarm store,
   and a thin attribute-condition layer. Authoritative state in
   `runtime/ctox.sqlite3`.
2. **App surface** — the no-build Business OS module `modules/iot/` (reference
   apps: `customers`, `shiftflow`, `matching`). Reads the engine via RxDB
   `iot_*` projections; writes only via `ctox.iot.*` `business_commands`; never
   imports `rxdb`; CSS uses only shell tokens.
3. **Harness / agent surface** — a `ctox iot <subcmd>` CLI (like `ctox queue` /
   `ctox ticket`) routed to `iot::handle_iot_command`, plus an IoT skill at
   `src/skills/system/host_ops/iot-operations/SKILL.md` that teaches the agent
   *when and how* to use the capability and the safety rules (never write a
   device attribute without an explicit task/approval).

The `business_commands` handler and the CLI are **thin adapters over the same
`iot::` functions** — one code path, two entry points, identical validation and
ACL — so app and agent can never diverge.

## 4. Thin Condition Layer, Not a Second Automation Engine

This is the load-bearing design decision. CTOX **already owns** firing,
scheduling, dedup, recurrence, and loop-bounding; IoT must reuse that machinery
rather than port a second automation brain (plan §2.2, §4A surface 3).

What is ported:

- attribute-condition **predicate evaluation** (from `JsonRulesBuilder`
  conditions + duration windowing + `AssetQueryPredicate`) — the
  `conditions.rs` evaluator emits *matches*.

What is **NOT** ported, and is delegated to CTOX's mission core:

- OpenRemote's `RulesEngine` firing loop, `RulesFacts`, jeasy-rules, the ~3s/50s
  timers, recurrence timers, and the 100-trigger loop cap are not reimplemented.
- Firing / scheduling → `schedule.rs` (time-based emission).
- Durable work + dedup → `queue.rs`.
- Loop governance → `mission_governor.rs`.
- Re-firing bound → the durable-spawn **budget** in
  `core_transition_guard.rs` (recorded in `ctox_core_spawn_edges`), which
  provably bounds re-firing and replaces OpenRemote's 100-trigger cap.

The IoT→agent trigger registers a spawn-contract family (e.g.
`iot-event-queue-task`: parent `IotAlarm` → child `QueueTask`, finite budget;
`iot-event-message`). There is exactly **one** automation brain. The
*observable* condition behavior must still match upstream; only the
implementation is CTOX-native.

## 5. Naming and Trademark

- **Subsystem:** `iot` (not "OpenRemote").
- **Collections:** `iot_*` (canonical CTOX prefix, no registry collision).
- **Commands:** `ctox.iot.*` (matches the existing command-family hierarchy).
- **Store listing (optional label):** "CTOX IoT — IoT asset & automation
  workspace" with an "inspired by / ported from OpenRemote" credit line.

OpenRemote is AGPL-3.0 and its name/logo are trademarked. We credit OpenRemote
as the porting source in `docs/legal/NOTICE` and in `// ref:` anchors, but the
CTOX product surface uses the CTOX-native `iot` name. This matches the existing
CTOX trademark stance already carried in `src/apps/business-os/README.md` and
`docs/legal/NOTICE`: the license grants rights to the source code, not the right
to present modified versions as official CTOX products, and OpenRemote's brand
is not claimed.

## 6. Dependency Policy

- **Native Rust only.** No JVM, no Docker, no external broker/DB/Keycloak, no
  heavy inference/runtime framework.
- **Prefer std + minimal vendored protocol code** over pulling a framework. If a
  protocol client is non-trivial (MQTT), **vendor/fork it as a self-contained
  crate** the way `whatsapp_rust/` is done, with the **upstream commit pinned**.
- **Closed kind enum** `IotAgentKind { Mqtt, Http, WebSocket }` — adding an
  agent is a deliberate core edit, exactly like `CommunicationAdapterKind` and
  the fixed collection list.
- **No process-env reads for runtime state.** All config/secrets flow through
  `runtime_env::env_or_config(root, …)` and the CTOX secret store. `std::env` is
  only acceptable for OS-level path expansion, per `CLAUDE.md` guardrails.
- **Porting discipline (CLAUDE.md):** ported algorithmic functions (LTTB
  downsampling, attribute-event semantics, condition evaluation, descriptor
  resolution) carry a `// ref: <upstream-file>:<line-range>` anchor, preserve
  upstream names, and translate comments verbatim where they describe the
  algorithm. Pure CRUD/storage glue is idiomatic CTOX Rust, not a line port.

## 7. Scope Boundary — DROP and DEFER (deferred ≠ broken)

The plan's scope is itemized so nothing is silently dropped.

### DROP — out of scope, with reason (plan §2.3)

- **MQTT broker service** (`manager/mqtt`, ~2,669 LOC) — that is OpenRemote
  exposing its own API over MQTT to external clients; CTOX uses RxDB/WebRTC for
  that. We keep the MQTT *client* agent, not the broker.
- Hibernate/JPA, Jackson, Apache Camel, JSR-380 validation, GraalVM, Keycloak,
  the Java `container` framework (~8,271 LOC), the LitElement UI, and `setup`
  (~2,899 LOC).

### DEFER — explicit, revisit after the core is solid (plan §2.4)

- **Groovy + JavaScript + Flow rules.** Only when-then/JSON conditions ship
  first. (JS is not even implemented upstream; Groovy needs a Groovy runtime;
  Flow is a separate visual model.)
- **16 of the 19 protocol-agent subdirectories** under `agent/protocol/`. We
  ship **MQTT + HTTP + WebSocket** first; the 16 deferred subdirectories are
  artnet, bluetooth, knx, lorawan, modbus, snmp, zwave, velbus, tradfri,
  openweathermap, serial, tcp, udp, mail, simulator, and `io` — where `io` is
  the shared abstract base (`IOAgent`) for the serial/tcp/udp client protocols,
  not itself an instantiable agent, leaving 15 deferrable concrete protocols.
  The rest are added one crate-file at a time on demand.
- Predicted/forecast datapoints, gateway/edge federation, provisioning
  auto-enrollment, dashboard builder, map tiling server.
- The long tail of the ~45 asset classes — start with a generic typed-attribute
  model plus a few high-value types.

These are deliberate boundary decisions, not quality reductions. The store
listing must label deferred scope as deferred so nobody mistakes it for broken.

## 8. Binding Acceptance Gates

The following plan sections are binding acceptance criteria for the port, not
aspirations (plan §1, §8, §12):

- **§2A edge-case inventory** — the non-obvious semantics found by reading the
  upstream code (timestamp/outdated/coercion/locking, LTTB and bucketing
  semantics, condition re-trigger/duration/recurrence/loop-bounding,
  reconnect/resubscribe state machine). Each numbered edge case must have a
  passing test before its phase is marked done. Nothing in §2A may be silently
  dropped.
- **§12 unit-test specification** — for a port, the test suite *is* the proof of
  fidelity. Every §2A edge case maps to at least one Rust `#[cfg(test)]` test,
  module `test.mjs`, or the CI soak job. Delegated behaviors (re-trigger
  suppression, recurrence, loop-bounding, startup suppression) are asserted as
  *observable* behavior over CTOX's native mechanisms, not over a ported loop.
- **§13 user stories** — P0 stories U1–U11 must pass as scripted end-to-end
  acceptance before Freigabe.
- **Spawn-liveness** — `ctox process-mining spawn-liveness` must prove the
  `iot-event-*` spawn family bounded and stay green throughout.

## 9. Phase Plan

The implementation plan §6 defines an eight-phase delivery (Phase 0 through
Phase 7), each ending committable to `main` with `cargo fmt --check` / `check` /
`test` clean and (where UI is touched) a `test.mjs`.

This RFC is **Phase 0 — Decision record & licensing**. Its exit criterion is:

> **scope and naming locked.**

The Phase 0 deliverables are this RFC (native-port scope, deferred list,
naming/trademark, dependency policy) and the `docs/legal/NOTICE` OpenRemote
AGPL-3.0 attribution entry. Phases 1–7 (engine core, collections + command path
+ CLI, native protocol agents, thin condition layer + mission routing, read-only
module, interactive control, hardening & release) are specified in the plan and
are out of scope for this docs-only phase.

## 10. Legal Constraint

OpenRemote is AGPL-3.0. The ported domain logic is a derivative work. AGPL-3.0
compliance for this port is achieved through:

1. An attribution entry in `docs/legal/NOTICE` with the upstream URL, the pinned
   commit, the license, the integration type, and the ported-vs-reimplemented
   scope.
2. Source pointers — the upstream URL plus `// ref:` anchors on each ported
   function.
3. The license copy carried by the root `LICENSE` (AGPLv3).
4. Modification honesty — the NOTICE entry distinguishes ported domain semantics
   from CTOX-reimplemented persistence/transport and out-of-scope subsystems.

The trademark stance in §5 keeps the CTOX product surface named `iot` and does
not claim the OpenRemote brand.

## 11. Open Questions

- Which CTOX user/member collection owns alarm `assignee` and agent ownership
  fields under `business_module_acl`?
- Should `iot_realms` map one-to-one to existing CTOX ACL scopes, or carry a
  separate realm record that references them?
- What is the projection-latency budget for live attribute values (U2) and the
  default windowed-datapoint cap before logged truncation (§2A.14)?
- For the MQTT client agent, do we fork an existing Rust MQTT crate or implement
  from scratch as a vendored self-contained crate (the `whatsapp_rust`
  precedent)?
