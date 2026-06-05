# Business OS — Native IoT App: Production-Readiness Plan

Status: draft / proposal
Predecessor: `docs/business-os-openremote-iot-port-plan.md` (the build plan — done)
Scope: take the committed native IoT engine + module from "compiles, tested, bench-verified" to **production ready**.

---

## 0. Baseline — what is already verified (this is not a rebuild)

Confirmed in-session against the committed code (`b8cdf4adc "Add native IoT engine integration"`):

- **Engine** `src/core/iot/` — 16,423 LOC, `cargo check` clean, **139/139 tests pass**, §2A edge cases really tested (LTTB-vs-reference, reconnect/backoff with injected clock, outdated/coercion/duration/dedup).
- **Integration wired** — `ctox iot` CLI (`main.rs:596`), 9 `iot_*` collections + schema hashes, `ctox.iot.*` routing (`rxdb_peer.rs:2382`), `iot-event-queue-task`/`iot-event-message` spawn contracts; skill at `src/skills/system/host_ops/iot-operations/SKILL.md`.
- **E2E core chain proven with REAL data** — open-meteo 20.3 °C → `attribute write` → `iot_datapoints` → condition fires → **HIGH alarm** (`iot_alarms`) → durable **QueueTask spawn** (`ctox_core_spawn_edges`: `IotAlarm→QueueTask`, `accepted=1`, `max_attempts=64`, `violation_codes=[]`) → alarm `ack` (Open→Acknowledged). Hard DB evidence, not prose.

So the engine is solid. Production-readiness is about **validating the layers the bench test could not** and **operational hardening** — not new domain code.

---

## 1. The five gaps that block "production ready"

From the in-session assessment. Each is a *validation/hardening* gap, mapped to a workstream:

| # | Gap | Why it blocks prod | Workstream |
| --- | --- | --- | --- |
| 1 | **Agent autonomy unverified** — the engine was driven via CLI by hand; no chat model runs here (`Qwen/Qwen3.5-27B` CUDA on a Mac, no API key), so the agent never drove it itself | This is the headline feature ("CTOX agent gets real IoT capability"). Unproven in practice. | **WS1** |
| 2 | **Live device ingestion unverified** — the value was written manually; real HTTP-poll / real MQTT broker ingestion not run (only the `ci_soak.rs` loopback fixture) | The whole point of protocol agents is device connectivity. | **WS2** |
| 3 | **Business OS live integration unverified** — module only rendered in a throwaway harness with injected data; module→daemon RxDB→browser (WebRTC) round-trip not run live | App-Store users get the browser surface; it must work against the real daemon. | **WS3** |
| 4 | **Repo hygiene** — 9 IoT files copied into `src/apps/business-os-desktop/release/mac-arm64/.../modules/iot/` (build artifacts under `src/`, CLAUDE.md violation) | Release/source hygiene; CI cleanliness. | **WS4** |
| 5 | **No formal acceptance/review** — §13 user stories not run as scripted acceptance; CI soak green in Actions unknown; ~16k generated LOC not independently reviewed for correctness/security | Release gate + trust in generated code. | **WS5** |

Plus **WS6** (operational hardening) and an explicit **deferred** list.

---

## 2. Workstreams

### WS1 — Agent autonomy (headline feature) · *needs a chat model*
Goal: prove the **agent**, given a normal chat task, autonomously uses `ctox iot` (via the skill) to do what I did by hand.

- **W1.1** Provision a working chat model — one of:
  - point CTOX at the CUDA box `gpu1-a6000` (Qwen3.5-27B), or
  - `ctox secret put --scope credentials --name OPENAI_API_KEY …` + chat source `api`.
- **W1.2** Run the 4 natural prompts via `ctox chat` (or browser chat):
  1. *überwach mir die temperatur in berlin über open-meteo und gib alarm wenn's über 19 grad geht*
  2. *wie warm isses grad?*  3. *zeig mir den verlauf*  4. *es ist ein alarm offen oder? kümmer dich drum*
- **W1.3** Forensics (same as the manual run): did the **agent** create the asset, configure the agent, save the rule, and did the chain fire? Evidence in `iot_assets`/`iot_rulesets`/`iot_alarms`/`ctox_core_spawn_edges` + `messages.agent_outcome`.
- **W1.4** Confirm the **skill is actually used** (agent references `iot-operations` / chooses `ctox iot`), and that completion goes through CTOX's **review + outcome gates** (not just "assistant said done").
- **Acceptance:** §13 **U8/U9/U10** pass autonomously; `ctox process-mining spawn-liveness` green; the safety rule (no device write without task/approval) holds.

### WS2 — Live device ingestion · *no model needed — can start now*
Goal: prove real ingestion, not a hand-written value.

- **W2.1 HTTP agent (open-meteo):** `ctox iot agent configure --kind http --data '<host/path/poll/json-pointer→temperature>'`; confirm the **runtime supervisor auto-starts the poll loop** and `iot_attributes`/`iot_datapoints` fill **without** a manual write; `iot_agent_status = connected`; poll interval + value mapping correct.
- **W2.2 MQTT agent (real broker):** configure against `test.mosquitto.org` (or local mosquitto); subscribe; publish a value externally; verify ingest; **force a disconnect and verify reconnect + resubscribe against a real broker** (not just the loopback fixture).
- **W2.3 WebSocket agent:** equivalent smoke against a real WS endpoint.
- **W2.4** Real-endpoint edge cases: network flap → reconnect/backfill cursor; bounded event coalescing under a fast publisher.
- **Acceptance:** live values land from a real source; `iot_agent_status` connected; reconnect/resubscribe proven live; truncation/coalescing logged (no silent caps).

### WS3 — Business OS live integration · *no model needed — can start now*
Goal: the **real** module in the **real** shell over the real data path.

- **W3.1** Install the `iot` module into the running Business OS (App Store / `ctox.module.install`); confirm it loads in the actual shell (not the harness).
- **W3.2** Confirm the `iot_*` collections **sync browser↔daemon over WebRTC** and the module renders **live daemon data** (the data created in WS1/WS2).
- **W3.3** Command write-back round-trip: a UI action (alarm ack / attribute write) → `ctox.iot.*` `business_command` → daemon executes → projection echo in the browser (the loop never run live yet).
- **W3.4** Honest "sync not ready" state when the peer is down (no fallback over HTTP).
- **Acceptance:** §13 **U1–U7** pass against the live app; a screenshot from the **real** app (not the harness).

### WS4 — Hygiene & cleanliness · *quick*
- **W4.1** Remove the 9 IoT files under `src/apps/business-os-desktop/release/…` (and ensure release bundles are gitignored, never tracked under `src/`).
- **W4.2** `cargo fmt --check` + `cargo clippy` clean for `src/core/iot/`.
- **W4.3** Decide on the throwaway preview harness (`/tmp/iotprev`, `.claude/launch.json`) — keep as a dev aid or remove.
- **Acceptance:** no build artifacts under `src/`; fmt/clippy clean.

### WS5 — Acceptance, review & CI · *no model needed except U8–U10*
- **W5.1 Independent code review** of the ~16k LOC (fresh adversarial pass — `/code-review` or a review workflow). Focus: ported algorithmic fns (LTTB, condition eval, MQTT reconnect/resubscribe), the command/ACL path, spawn-budget bounding, and **guardrail compliance** (no `std::env`, no HTTP data bridge, no `rxdb` import in the module).
- **W5.2 Security review** (`/security-review`): agent-credential handling (MQTT user/pass, TLS) via secret store; ACL on every `ctox.iot.*` command; token redaction in support bundles.
- **W5.3 CI:** ensure `.github/workflows/iot-soak.yml` runs **green in Actions** (not only locally) and `ci.yml` runs the IoT unit tests.
- **W5.4** Run §13 **U1–U11** as scripted acceptance; record evidence.
- **Acceptance:** review findings resolved; CI soak green; `spawn-liveness` release gate green.

### WS6 — Operational hardening & release
- **W6.1** Secrets/config: all agent credentials + endpoints via `runtime_env::env_or_config` + secret store; rotation; **no process env vars**.
- **W6.2** Multi-realm/tenant isolation via `business_module_acl`.
- **W6.3** Backpressure: event coalescing + datapoint window caps with **logged** truncation; never mirror the full series.
- **W6.4** Observability: `iot_agent_status` panel; emit harness-flow / process-mining events for IoT actions.
- **W6.5** Docs: operator guide (point CTOX at devices/brokers, scope realms), App-Store listing, finalize `docs/iot-engine-phase7-release-notes.md`; mark deferred scope (§3) so nobody mistakes it for broken.
- **W6.6** Release: tag `vX.Y.Z` on `main` → GitHub Actions release.

---

## 3. Deferred — explicitly NOT part of this production milestone
(Per port plan §2.4 — documented, not "missing".)
- Groovy / JavaScript / Flow rules.
- 18 of 20 protocol agents (KNX, Z-Wave, Modbus, LoRaWAN, SNMP, BACnet, …) — ship MQTT/HTTP/WS first.
- Predicted datapoints / forecasting, gateway/edge federation, provisioning auto-enrollment, dashboard builder, map tiling server.
- The long tail of the 45 asset classes.

These must be labeled "deferred" in the App-Store listing.

---

## 4. Sequencing

- **Phase A — no model needed (start now):** WS2 (live ingestion) ∥ WS3 (Business OS live) ∥ WS4 (hygiene) → WS5.1/5.2/5.3 (review + CI). This closes 3 of 5 gaps and most of the review without any model.
- **Phase B — needs a model:** WS1 (agent autonomy) on `gpu1-a6000` or with an API key; then WS5.4 U8–U10.
- **Phase C — release:** WS6 ops hardening → tag.

Rough effort: Phase A ~1–1.5 weeks, Phase B ~2–4 days (once a model is available), Phase C ~2–4 days. A `/code-review` and an acceptance-fan-out workflow can compress WS5.

---

## 5. Definition of Done (production-ready release gate)
- [ ] WS1: agent autonomously drives the full IoT chain (U8–U10) with model evidence + review/outcome gates.
- [ ] WS2: live HTTP **and** real-broker MQTT ingestion proven; reconnect/resubscribe live; `iot_agent_status` connected.
- [ ] WS3: module installs + renders live daemon data over WebRTC; command write-back round-trips; U1–U7 pass; real-app screenshot.
- [ ] WS4: no build artifacts under `src/`; fmt + clippy clean.
- [ ] WS5: independent code + security review resolved; CI soak green in Actions; `spawn-liveness` green.
- [ ] WS6: secrets/ACL/backpressure/observability in place; docs + listing done; deferred scope labeled.
- [ ] `cargo fmt --check`, `cargo test`, module `test.mjs` all green on `main`.

When all boxes hold, tag the release.
