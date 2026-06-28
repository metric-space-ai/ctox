# CTOX IoT — Umsetzungsplan

Status: Umsetzungsplan
Grundlage: `docs/business-os-iot-app-spec.md` (Konzept) · `archive/iot-mockup.html` (Interaktion/Anatomie, im Mock geklärt)
Vorbedingung: die native IoT-Engine `src/core/iot/` (kompiliert, 139 Tests, E2E belegt) — **wird nicht neu gebaut**, sondern erweitert.

> **Was es ist:** Keine Grafana. Eine **Delegations-App**: der Mensch beauftragt CTOX in Klartext (Wenn → Dann),
> CTOX **programmiert jedes Widget** und überwacht/handelt. Visualisierung dient dem Auftrag, ist nie der Zweck.

---

## 1. Kernmodell — ein Widget = eine Automatisierung, von CTOX in **drei** Teilen programmiert
Pro Widget schreibt CTOX (Coding-Agent) drei Artefakte; der Mensch sieht/editiert sie:

| Teil | Was | Läuft wo | Erzeugt aus |
| --- | --- | --- | --- |
| **① Trigger-Logik** | Wächter-Programm über den Signalstrom | IoT-Backend (Rust), stateful, pro Messwert — **kein LLM in der Schleife** | der **Wenn**-Bedingung (Freitext) |
| **② Widget-Code** | HTML/CSS/JS, das die **Visualisierung** rendert | Browser-Modul (sandboxed Render-API) | Signal + Absicht |
| **③ Auftrags-Prompt** | der **Dann**-Prompt | bei Auslösung → **Chat-Spawn** | dem **Dann**-Freitext |

Ablauf: Wenn-Bedingung → CTOX schreibt Trigger-Code → Trigger feuert → **Chat-Spawn mit dem Dann-Prompt + Referenzen** (Signal-Verlauf, Asset, Auslösegrund) → Agent leased & handelt (verifizierte `iot-event-queue-task`-Kette). **Keine** feste Schwelle/Heuristik im UI, **keine** vorab-deklarierten Aktions-Ausgänge, **kein** Fake-Fortschritt — die Arbeit liegt im gespawnten Chat (durables Outcome).

---

## 2. Was schon steht (verifiziert — nicht anfassen außer erweitern)
- Engine `src/core/iot/{model,store,datapoints,alarms,conditions,adapters,gateway,runtime,projector,commands}.rs` + `agents/{mqtt,http,ws}_native`.
- Collections `iot_*` (assets, attributes, datapoints, alarms, rulesets, agents, agent_status, realms, asset_types) in `rxdb_peer.rs::collection_creators` + `business_os_schema_contract.json` + Hashes.
- Commands `ctox.iot.*` (Routing `rxdb_peer.rs:2382`, Ausführung `store.rs`, CLI `commands.rs`).
- Spawn-Contracts `iot-event-queue-task`/`iot-event-message` (`core_transition_guard.rs`) — Match → Alarm → Queue-Task → Agent **belegt**.
- Skill `src/skills/system/host_ops/iot-operations/SKILL.md`.

## 3. Backend-Deltas (das Neue)
1. **Wächter-Runtime (Trigger-Logik).** Embed **Rhai** (Rust-nativ, sandboxed: kein FS/Netz, Zeit-/Speicherlimit) in der IoT-Engine. Signal-API für den Wächter: `signal.last()/.window("15m")/.rate()`, `signals["..."]`, persistenter `state`, `fire(grund)`. Scheduler ruft den Wächter **stateful pro neuem Datenpunkt** des gebundenen Signals (bzw. Tick für Zeitbedingungen). **Ersetzt** die deterministische `conditions.rs`-Schablone für neue Aufträge.
2. **Widget-Render-Code.** CTOX generiert pro Widget HTML/CSS/JS. Ausführung im Modul über eine **gesandboxte Render-API** (`render(el, signal)` mit Whitelist: nur das eigene Element + die gebundenen Signaldaten + Draw-Helfer; **kein** beliebiges DOM/Netz/`eval`). Entscheidung Phase 0: `new Function` mit gekapseltem Scope vs. sandboxed `<iframe>` vs. kleiner Render-Interpreter.
3. **Neue Collections** (nach dem `iot_*`-Muster: `collection_creators` + Schema-Contract + Hashes + Parity-Guard):
   - `iot_dashboards` `{id, realm, name, scope, view_mode, sort_index}`
   - `iot_widgets` `{id, dashboard_id, signal_ref, cond_text, action_prompt, trigger_code, render_code, trigger_status, x, y, w, h}`
4. **Commands** `ctox.iot.widget.{upsert,delete,arrange,compile_trigger,generate_render}` und `ctox.iot.dashboard.{upsert,delete}` — `compile_trigger` (Wenn-Freitext → Rhai-Wächter via Agent-Turn) und `generate_render` (Signal/Absicht → Widget-Code via Agent-Turn). CLI + `business_commands` teilen `iot::`.
5. **Trigger → Chat-Spawn.** Bei `fire` spawnt die Engine einen **Chat** (`business_chats`/Mission-Queue) mit Seed = Auftrags-Prompt + Referenzen (Signal, Verlauf, Asset, Grund). Reuse `iot-event-queue-task`. Self-Repair: Wächter-Compile-/Laufzeitfehler → Widget-Status „braucht Aufmerksamkeit" → CTOX repariert.
6. **Webhook-Connector** in/out (inbound-Endpunkt → Signal; outbound-POST als Agenten-Aktion).

## 4. Frontend — `modules/iot/`, gegen die **echten** BOS-Komponenten (nicht nachmalen)
- **Shell:** `ctox-pane`-Chrome, Tokens (oklch hell + dark), **`CtoxResizer`** aus `shared/resizer.js`, 2-Pane (**keine rechte Spalte**), `mount(ctx)`-Contract, Daten nur über `iot_*`-Projektionen + `business_commands` (nie `rxdb` importieren).
- **Links — Assets & Signale:** Baum bis aufs **Signal** (Gerät → Attribute), Live-Wert + Status; Connectoren (MQTT/HTTP/WS/Webhook). **Rechtsklick** über `createContextMenu` aus `shared/context-menu.js`: „Auftrag von diesem Signal" / „Verlauf öffnen" / „Mit CTOX chatten".
- **Mitte — Dashboard aus Automatisierungs-Kacheln**, umschaltbar **Karten ⇄ Liste** (Segmented wie `customers`), Dashboard-Selector, persistentes Grid (`desktop`-Muster: Drag → `incrementalPatch({x,y})`).
  - **Kachel (entschlackt):** Status + Wert + Sparkline (aus `render_code`) · **Wenn** (Akzent-Linie) · **Dann** · dezenter Footer (Chat öffnen / `</>` Code). Keine Boxen-/Label-Flut, keine Erklär-Bars.
  - **Rechtsklick/⋯** → „Bearbeiten / Code öffnen / Pausieren / Mit CTOX chatten / Löschen".
- **Setup (Auftrag anlegen):** **`openBusinessDialog`-Stil** (kein Fake-Chat): Kurzbeschreibung → „CTOX vorschlagen" füllt **Signal · Wenn · Dann**; „Auftrag anlegen" → `ctox.iot.widget.upsert` + `compile_trigger` + `generate_render`.
- **Editor:** Modal, 3 Tabs — **Auftrag** (Wenn/Dann) · **Trigger-Logik** (Rhai-Code) · **Widget-Code** (HTML/CSS/JS) — alle editierbar, je „↻ Neu generieren". Der Widget-Code-Tab ist die sichtbare **visuelle Programmierung durch CTOX**.

## 5. CTOX-Vertrag (warum es eine CTOX-App ist)
Der Mensch schreibt nur Prompts (Wenn/Dann). CTOX **programmiert** Trigger-Code + Render-Code, **führt** die Aktion als Agenten-Chat aus, und kann **ganze Dashboards/Widgets aus einem Prompt bauen** (`ctox iot` + Skill). Alles sind native Records → Agent **und** Mensch editieren dieselben Artefakte. Aktion läuft unter Review-/Outcome-/Spawn-Budget-Gates.

## 6. Sicherheit & Härtung
- **Generierter Code sandboxed:** Trigger (Rhai, kein I/O, Limits) und Render (Whitelist-API, kein DOM/Netz/eval). Nie ungeprüft ausführen.
- ACL auf jedem `ctox.iot.*`; Webhook-Secrets im Secret-Store; keine env-Vars; keine HTTP-Datenbrücke Browser↔CTOX.
- Backpressure: Signal-Coalescing engine-seitig; nur Last-Value + gefensterte Historie in RxDB.

## 7. Build-Reihenfolge
| Phase | Inhalt | Fertig wenn |
| --- | --- | --- |
| **P0** | Entscheidungen: Wächter-Runtime (Rhai) · Render-Sandbox-Modell · Collections-Schema | RFC `docs/rfcs/` |
| **P1** | Backend: Rhai-Wächter + Signal-API + Scheduler; `iot_dashboards`/`iot_widgets` + Commands + CLI; `compile_trigger`/`generate_render` (Agent-Turn) | `cargo test` grün; CLI-Roundtrip; Wächter feuert → Chat-Spawn |
| **P2** | Modul-Gerüst: 2-Pane + `CtoxResizer` + `ctox-pane`; links Baum/Signale + `createContextMenu`; Karten⇄Liste | rendert Live-`iot_*` über RxDB |
| **P3** | Automatisierungs-Kachel (entschlackt) + Render-Sandbox (`render_code`) + persistentes Grid | Kachel rendert von CTOX generierten Widget-Code |
| **P4** | Setup (`openBusinessDialog`) + Editor (3 Tabs, editierbar, „neu generieren") + Webhook in/out | Auftrag anlegen/editieren end-to-end gegen Daemon |
| **P5** | Agent-gebaute Dashboards; Self-Repair; Zustände (leer/laden/sync/Fehler); Politur; CI-Soak | §8 |

### 7a. Implementierungsstand (P0 ✓ · P1 großteils ✓)
**P0 ✓** — `docs/rfcs/0011_iot_automation_widgets.md` (Wächter-Runtime Rhai · Render-Sandbox-Modell · Collections-Schema · Commands · Trigger→Chat-Spawn · Sicherheit).

**P1 ✓ (Kernfunktionsweise lokal verifiziert, ohne Modell):**
- **Collections** `iot_dashboards` + `iot_widgets`: kanonische Quelle `modules/iot/schema.js` → generierter Contract → Hash-Fixture (Parität grün), Registrierung automatisch via `business_os_collections()`.
- **Commands** (`src/core/iot/commands.rs`): `dashboard_upsert/delete/list`, `widget_upsert/delete/arrange/list` mit Projektionen; geroutet auf **CLI** (`handle_iot_command`) **und** RxDB (`handle_business_command`: `ctox.iot.dashboard.*`/`ctox.iot.widget.*`); Realm session-scoped; Delete = Tombstone.
- **Wächter-Runtime** (`src/core/iot/watcher.rs`): sandboxed **Rhai** (Op-/Call-/Größen-Limits, `eval` aus, kein FS/Netz); Signal-API `signal.last()/.window/.rate/.avg/.min/.max/.count/.age_ms`, `signals("name")`, persistenter `state`, `fire(grund)`. `evaluate()` ist rein/injizierbar (kein DB/Clock/Modell).
- **Scheduler/Fire-Pfad** (`src/core/iot/widget_runtime.rs`): `tick_widget` baut `SignalContext` aus den Datapoints → `evaluate` → `state` + `trigger_status` persistiert + reprojiziert → bei `fire`: Alarm + **bewährte** `mission::channels::ingest_iot_event_message`-Kette (budget-begrenzter `iot-event-queue-task` + durable Message, Seed = `action_prompt` + Referenzen). `tick_widgets_for_signal` dispatcht **pro Datenpunkt** (eingehängt in `attribute_write` neben dem Alt-Ruleset-Pfad). Self-Repair: Compile-/Laufzeitfehler → `trigger_status="needs_attention"`.
- **Tests:** Watcher (Schwellwert/Fenster/Rate/**State-Hysterese**/benannte Signale/**Sandbox-Stopp**/Compile-Fehler/`eval`-Block) + Widget-Runtime (**E2E „Wächter feuert → durable Queue-Task" ohne Modell**, Dispatch-by-Signal, Skip-ohne-Programm, needs_attention) + Command-Roundtrip + Schema-Parität.

**P1 Codegen-Seam ✓ (Generierung modell-gegated):** `compile_trigger`/`generate_render` reihen einen **durable Codegen-Auftrag** ein (`mission::channels::create_queue_task`); ein modell-fähiger Agent least ihn und schreibt den Code per `ctox iot widget upsert --trigger-code/--render-code` zurück. Das Rückschreiben **validiert** (`watcher::validate_program`) → `armed`/`needs_attention` (Self-Repair). Kein synchroner Modell-Call im Command-Pfad; die eigentliche Generierung braucht das Chat-Modell (`Qwen/Qwen3.5-27B`, CUDA-only) → Lauf auf `gpu-host`/API. CLI nimmt von CTOX/Hand geliefertes `trigger_code`/`render_code` direkt entgegen.

**Webhooks ✓ (rein & raus, end-to-end):** `webhook register` mintet eine token-gegatete Inbound-URL, an ein Signal gebunden (Token im **Secret-Store**); `webhook::handle_http` prüft das Bearer-Token konstant-zeitig und ingestet über den vollen Inbound-Pfad (Conditions + Wächter-Tick) — voll getestet ohne HTTP. Die dünne Daemon-Route `POST /ctox/iot/webhook/<id>` (`X-Webhook-Token`) auf dem bestehenden `tiny_http`-Listener delegiert daran (Fehler → 401/400, kein Leak). `webhook send` = Outbound-POST (Auth-Secret aus dem Store). E2E „Webhook → Wächter feuert" + Auth-Tests grün.

**P2–P4 ✓ (gebaut, committet & Playwright-verifiziert gegen die echten BOS-Komponenten + Mock-`ctx`):** 2-Pane + `CtoxResizer` + `ctox-pane` + Tokens; Asset/Signal-Baum + `createContextMenu`; Karten⇄Liste; **3-Teil-Automatisierungs-Kachel**; **Render-Sandbox** (`render_code` im `<iframe sandbox>`, CSP, Lint — browser-verifiziert); **3-Tab-Editor** (Auftrag/Trigger-Logik/Widget-Code, je „neu generieren"); Setup via `showBusinessPrompt`/`-Confirm`; **Webhook-Source-UI** (Token/URL); **Drag-Reorder** des Grids (persistiert via `sort_index`). Inbound-HTTP-Route im Daemon (`#[cfg(not(unix))]`-Front über `webhook::handle_http`).

**P5 großteils ✓:** Self-Repair (`needs_attention` + `validate_program`) · agent-baubar (`ctox iot …` + Skill, **Live-Binary-Roundtrip** verifiziert) · **i18n DE+EN** (browser-verifiziert) · Leer-Zustände · **CI-Soak** (`widget_watcher_soak_stays_bounded` — 80 Fires → bounded durable Work) · `rustfmt` clean · `iot.test.mjs` grün (11 Collections, module.json/registry konsistent, Bundle). Laden/Sync = shell-owned.

**Verifikation:** ~167 Rust-Unit-Tests grün · Schema-Parität grün · `iot.test.mjs` grün · **Playwright** (Mount→Render→Sandbox→Editor→Kontextmenü→Pause→Anlege-Flow→Webhook→Drag→i18n DE/EN; Konsole fehlerfrei) · **Live-Binary** (Wächter `fired`, Webhook-Token, Codegen-Enqueue, isoliert via `CTOX_ROOT`).

**Genuin offen (harte externe Abhängigkeit):** (1) **LLM-Codegen-Generierung** der `compile_trigger`/`generate_render`-Aufträge — braucht das Chat-Modell auf `gpu-host`/API (Enqueue ist verifiziert); (2) **Live-Daemon-Stack-E2E** (echtes RxDB/WebRTC) statt Mock-`ctx` + Live-CLI.

## 8. Qualitäts-Gate
- [ ] Liest sich als **„meine CTOX-Aufträge"**, nicht als Chart-Dashboard; Politur auf `customers`/`shiftflow`-Niveau.
- [ ] Pro Widget **drei editierbare CTOX-Teile** (Trigger-Logik · Widget-Code · Auftrags-Prompt), je „neu generieren".
- [ ] **Echte BOS-Komponenten:** `createContextMenu` (Rechtsklick), `openBusinessDialog` (Setup), `CtoxResizer`, `ctox-pane`, Tokens — **nicht** nachgemalt.
- [ ] Trigger = von CTOX generierter Rhai-Wächter (keine Heuristik-Schwelle); Aktion = Prompt → Chat-Spawn mit Referenzen; **keine** Fake-Live-Narration/Vorab-Ausgänge.
- [ ] Karten ⇄ Liste; Dashboards konfigurierbar **und** persistent; Webhooks rein & raus; MQTT/HTTP/WS rein.
- [ ] Generierter Code (Trigger + Render) **gesandboxt**; ACL; keine env-Vars; keine HTTP-Datenbrücke.
- [ ] Alles **agent-baubar** über `ctox iot` + Skill; `cargo fmt/check/test` + `test.mjs` + `spawn-liveness` grün.

---

### Anhang — was das Mockup geklärt hat (`archive/iot-mockup.html`)
2-Pane ohne rechts · Asset→Signal-Baum · Karten⇄Liste · **entschlackte** Automatisierungs-Kachel (Wenn/Dann + dezenter Footer) · `shell-context-menu` (Rechtsklick auf Signal/Widget/Fläche) · `business-dialog`-Setup statt Fake-Chat · **Editor mit Auftrag/Trigger-Logik/Widget-Code** · Trigger→Chat-Spawn realistisch (Seed-Prompt + Referenzen, kein Fortschritts-Theater).
