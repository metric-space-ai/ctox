# RFC 0011: IoT Automation Widgets (CTOX-programmierte Widgets)

**Status:** P0 — Architektur-Entscheidungen vor dem Bau
**Datum:** 2026-06-05
**Grundlage:** `docs/business-os-iot-umsetzungsplan.md`, `docs/business-os-iot-app-spec.md`, `archive/iot-mockup.html`
**Erweitert:** die native IoT-Engine `src/core/iot/` (wird nicht neu gebaut)

## Kontext
Ein Widget = eine Automatisierung, von CTOX in drei Teilen programmiert: ① Trigger-Logik (Backend),
② Widget-Code (HTML/CSS/JS, Visualisierung), ③ Auftrags-Prompt (Aktion). Dieses RFC legt die drei
risikobehafteten/neuen Entscheidungen fest, die der Umsetzungsplan §3/§6 als P0 markiert.

## Entscheidung 1 — Wächter-Runtime (Trigger-Logik)
- **Rhai** (Rust-nativ, eingebettet) als Runtime für den von CTOX generierten Wächter. Kein neues llama/JS-Framework.
- **Sandbox** über Rhai-`Engine`-Limits: `set_max_operations`, `set_max_call_levels`, `set_max_string_size`, `set_max_array_size`; **keine** Module/FS/Netz registriert; harte Zeit-/Speichergrenze pro Aufruf.
- **Signal-API** (als Rhai-Funktionen registriert, read-only): `signal.last()`, `signal.window("15m")`, `signal.rate("15m")`, `signals["raum.temp"]`, persistenter `state` (pro Widget, serialisiert), `fire(grund)`.
- **Scheduler** in `src/core/iot/runtime.rs`: ruft den Wächter **stateful pro neuem Datenpunkt** des gebundenen Signals; zusätzlich Tick (z.B. 30 s) für Zeitbedingungen. `state` wird in `iot_widgets.trigger_state` persistiert.
- **Verhältnis zu `conditions.rs`:** der deterministische Prädikat-Layer bleibt für Alt-Rulesets; **neue** Aufträge nutzen den generierten Rhai-Wächter (keine feste Schwelle).
- **Crate:** `rhai` (default-features, optional `sync`) als Workspace-Dependency.

## Entscheidung 2 — Render-Sandbox (Widget-Code im Browser)
CTOX generiert pro Widget HTML/CSS/JS. Ausführung im Modul **gesandboxt**, nie als beliebiges Skript:
- Kontrakt: `render(host, api)` — `host` = das eigene Kachel-Sub-Element; `api` = `{ signal:{last,window,rate}, draw:{line,value,gauge,grid}, fmt }`. **Kein** Zugriff auf `window/document/parent/fetch/eval`.
- **v1:** `new Function('host','api', code)`, aufgerufen mit eingefrorenem `api`; statischer Lint bei `generate_render` (verbotene Tokens: `import`, `fetch`, `document`, `window`, `eval`, `Function`, `parent`); CSP `script-src 'self'`. Der Render-Code ist CTOX-authored, wird aber als **untrusted** behandelt.
- **Fallback** falls Isolation nicht ausreicht: sandboxed `<iframe srcdoc>` mit `postMessage`-Daten-Bridge.
- `render_code` in `iot_widgets` persistiert; läuft erneut bei Daten-Update (RxDB-Subscription).

## Entscheidung 3 — Collections (nach dem `iot_*`-Muster)
Registrierung in `rxdb_peer.rs::business_os_collections()` (:5960) + `business_os_schema_contract.json` + Hashes + Parity-Guard:
- **`iot_dashboards`**: `{ id, realm, name, scope, scope_ref, view_mode, sort_index, updated_at_ms }`
- **`iot_widgets`**: `{ id, dashboard_id, realm, signal_ref, cond_text, action_prompt, trigger_code, trigger_state, trigger_status, render_code, x, y, w, h, sort_index, updated_at_ms }`
- `iot_triggers` wird **nicht** separat angelegt (in `iot_widgets` gefaltet) — weniger Oberfläche.

## Entscheidung 4 — Commands
Geroutet/ausgeführt wie bestehende `ctox.iot.*` (`rxdb_peer.rs:2382` + `store.rs` + CLI `commands.rs`):
- `ctox.iot.dashboard.{upsert,delete}`
- `ctox.iot.widget.{upsert,delete,arrange}` (arrange = x/y/w/h)
- `ctox.iot.widget.compile_trigger` — `cond_text` → `trigger_code` (Agent-Turn)
- `ctox.iot.widget.generate_render` — `signal_ref`+Absicht → `render_code` (Agent-Turn)
ACL-gated, idempotent, mit Outcome-Echo.

## Entscheidung 5 — Trigger → Chat-Spawn
`fire(grund)` → bestehende `iot-event-queue-task`-Kette → Queue-Task **seedet einen `business_chat`** mit
`action_prompt` + Referenzen (Signal-Serie, Asset, Auslösegrund). Agent leased & handelt unter
Review-/Outcome-/Spawn-Budget-Gates. **Self-Repair:** Compile-/Laufzeitfehler des Wächters → `trigger_status="needs_attention"` → CTOX schreibt eine neue Version.

## Entscheidung 6 — Sicherheit
Rhai-Sandbox (Limits, kein I/O) · Render-Sandbox (Whitelist-API, Lint, CSP) · ACL je Command · Webhook-Secrets im Secret-Store · keine env-Vars · keine HTTP-Datenbrücke Browser↔CTOX.

## Offene Punkte
- Reicht `new Function`+Whitelist, oder iframe-Isolation nötig? (Phase-3-Spike, Default: new Function, iframe als Fallback.)
- Rhai-Eval-Kosten pro Datenpunkt bei hoher Frequenz → engine-seitiges Coalescing (Last-Value + Fenster).
- Versionierung/Rollback von `trigger_code`/`render_code` (v1: letzte gute Version in `iot_widgets`).

## Umsetzung
Phasen P1–P5 siehe `docs/business-os-iot-umsetzungsplan.md`. Dieses RFC = P0 (entschieden).
