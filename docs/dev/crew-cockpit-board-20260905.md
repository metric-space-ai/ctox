# CREW-COCKPIT · Board (CTOX-App + Crew-Leiste + Tickets + Crew-Identität)

Stand: 2026-09-05 12:40 · Orchestrator: Fable · Implementierer: Codex-Thread `01a07107-d27c-7e80-a1dc-2311f60ad0bb` (Arbeit landet in PRs gegen `metric-space-ai/ctox` main)

**Headline:** Kritischer Pfad ist PR-1 (Harness-Projektionen + Steuerbefehle): ohne sie kann keine Oberfläche zeigen, was der Harness tut. Danach PR-2 (Crew-Identität im Harness), dann erst die drei Oberflächen.

## Kanban

### Done

- **D1 · Ist-Analyse Code (4 Audits, 05.09. 12:05–12:25)** — CTOX-App (`src/apps/business-os/modules/ctox/`), Crew-Leiste (`shared/business-chat.js`), Tickets (`modules/tickets/` + `src/core/mission/tickets/`), Harness-Observability (`src/core/service/service.rs`, `store_projections.rs`, `harness_flow.rs`). Ergebnisse unten unter „Befunde“. Stichproben per grep verifiziert: `ctox_runs` hat keinen Rust-Writer; `commandBus.cancel` wird in CTOX-App und Chat 0× benutzt; `loadLocalCollection` = `find().limit(200)` → sort → `slice(0,20)` (`modules/ctox/index.js:3944-3954`); `lease_owner`/`status_note` 0× gerendert; `hold_reason`/`retry_not_before` 0× in `src/core/business_os/`; `QUEUE_PRESSURE_GUARD_THRESHOLD = 20` (`service.rs:159`).
- **D2 · Ist-Analyse live (welsch.ctox.dev, 05.09. 12:12–12:25, Rolle Admin)** — Testaufgabe über die Crew-Leiste gesendet: Command `cmd_b964f8bc-2130-42df-80f8-53e7fe9b2961`, Task `queue:system::41c33261fb7b96906277e159`. Beobachtet: Übergabe in Queue nach ~3 s; während der Arbeit nur leerer Balken, Fortschritt ausschließlich im Tooltip („0 % · 20/23 Turns · Denkblöcke 17 · Tools 6“, Prozent bleibt 0 während Turns steigen); Composer verschwindet; nach Abschluss (`execution_phase=retry_wait`, `result.user_message` vorhanden, `status=succeeded`, `attempt=1`) zeigt der Chat **keine Antwort**, die CTOX-App „Wartet“, kein Grund sichtbar. Wesen wechselte nach dem ersten Senden den Namen (Tavi → Milo), weil Identität = Hash(commandId || chatId) (`business-chat.js:1952-1978`).
- **D3 · Ist-Analyse lokal (127.0.0.1:8765)** — Instanz hing seit 04.09. 17:13 in `status=stale` eines Upgrade-Lease (`/api/business-os/ctox/maintenance`, `retry_action: ctox upgrade --dev`); Browser-Peer nie authentifiziert; CTOX-App zeigte dauerhaft „Tasks werden synchronisiert“, kein Hinweis auf die Ursache, Banner nicht schließbar.
- **D4 · Lokale Instanz repariert (05.09. 12:48–12:52)** — Ursache: Upgrade-Prozess pid 2666 starb am 04.09. nach dem Build; Release `business-os-shell-v0.1.44` war fertig, Dienst lief seit 07:12 bereits daraus (launchd `com.metric-space.ctox.service`, `releases/…/runtime` ist Symlink auf `~/.local/state/ctox`), aber `current` zeigte auf `workjet-sync-efe5ef1a7`, Manifest alt, `update_state.json` in `building`, toter `update.lock`. Kein Rebuild nötig: Lock → `update.lock.stale-20260905T-dead-pid-2666`, `current` → 0.1.44, Manifest `current_release=business-os-shell-v0.1.44` (Sicherung `backups/install_manifest.json.pre-repair-20260905`), Update-State `completed` (Sicherung `backups/update_state.json.pre-repair-20260905`). Maintenance ging sofort von `stale` auf `waiting_replication` (92 %, `service_active=true`). Der letzte Schritt (`waiting_collections` → `completed`) braucht einen Browser, der die Replikation aufbaut und `ctox.maintenance.client_ready` sendet. Aus dem Claude-In-App-Browser kommt lokal kein WebRTC-Datenkanal zustande (Signaling-Log: Offer/Answer beidseitig, Browser liefert nur einen mDNS-Host-Kandidaten, kein srflx/relay; Browser meldet `peer_connect_timeout` nach 30 s; nativer Peer `peerAuthenticated=false`). Beim Dienststart 07:12 war die Replikation mit einem normalen Browser oben („replication up for 220 collections“), das Problem ist also der Pane-Browser, nicht die Instanz. Abschluss erfolgt beim nächsten Laden in Chrome/Safari; dort dann prüfen, dass das Banner „Upgrade abgeschlossen“ zeigt.
- **D5 · welsch-Upgrade geklärt (05.09. 12:50)** — Nicht meine Baustelle, sondern eine andere Sitzung (`ctox-dev/output/welsch-office-upgrade-core-20260905.sh`, systemd-Unit `ctox-office-core-upgrade-20260905`): `ctox upgrade --dev` lief 12:16–12:44 lokal Zeit, Ergebnis `applied release branch-main-20260905T101623Z`, Dienst `active/running`, Shell-Slot 0.1.44 `healthy` (Phase `rollback` von 0.1.45, 12:05). Browser-Prüfung 12:53: Maintenance `completed`, Banner ausgeblendet, `app.js?v=20260903-app-import-fidelity-v337`.
- **D6 · Workjet-Slider/Stundenzettel geprüft** — `claude-workjet` ist auf dem Stand von `origin/main` (0 Commits dahinter, HEAD 8e5b0f4); weder Quelltext noch installierte `Workjet.app` kennen Slider oder Stundenzettel. Beide Konzepte sind im Brief eigenständig definiert (Soul-Achsen 0–100, Stundenzettel = `ctox_runs` je Mitglied + Rückblick). Keine offene Frage mehr.

### Working

- **W1 · PR-1 „Harness-Cockpit Foundation“ (Server) — in Review-Schleife** — PR #58 (Draft) https://github.com/metric-space-ai/ctox/pull/58, Head `523a8d3ba`, Basis `2d88f2267`, 62 Dateien +5107/−343, merged sauber auf `origin/main 0b4b14a25`. Worker: Codex-Thread; Review-Worktree `/Volumes/tmp/worktrees/ctox/review-pr1` (Branch `review/pr1`). **Mein Review 05.09. 17:50–18:03 (zwei Opus-Reviewer, stichprobenartig gegengelesen): mergefähig mit Fixes.** Vertrag 2a–2g erfüllt; Policy dreifach verifiziert (Browser-Read/Write, Grants, MCP); kein Guard geschwächt; Serialität/Gates unverändert; fremde Schema-Diffs (browser, kundenpipeline, reports) sind bewiesene Generator-Regeneration (Basis-Commit war „stale“). 16 Fix-Forderungen an den Worker gesendet (18:03, Datei `scratchpad/review-pr1-fixes.md`): Blocker = blockierende Reads im Turn-Pfad (`service.rs:5712`), Ereignisverlust bei transientem Fehler (`harness_flow.rs:264`), stumme Pause bei Parse-Fehler (`harness_cockpit.rs:37`), `hold_reason`-Überschreiben (`mod.rs:3559`), fehlender `?v=`-Buster auf `rxdb-runtime.js` (Edge-Cache bis 24 h), gecachte Import-Rejection. Kimi-Adversarial-Review (Workjet-Run `local-2026-09-05T160332Z-5e4f884d-…`, Launchpad `~/.local/state/workjet-launchpads/ctox-crew-cockpit`, Brief `briefs/kimi-review-pr1.md`) fertig 18:28: **mergefähig mit Fixes**, 0 kritisch/hoch, 5 niedrig, 2 info; kein bestätigter Weg für Rolle user oder MCP auf Cockpit-Collections oder Control-Commands. Drei Punkte als Nachtrag 17–19 an den Worker: Legacy-Grant-Materialisierung für Cockpit-Collections (`store.rs:15590`), rohes Payload im Audit (`harness_cockpit.rs:63-77`), Terminal-Guard ohne ORDER BY bei Multi-Link-Tasks (`mod.rs:3485`). CI: drei rote Jobs sind Altbefunde von main (Plattform-Freeze-Check auf Basis-Commit reproduziert rot; `npm audit` fast-uri/xmldom, Lockfile nicht im Diff). Worker-Abschlussbericht 18:29 (PR-Text §4): gezielte Rust-Tests grün (8/9/17/130/…), RxDB 399, JS 114/114 mit Wire-Daemon, Spawn-Liveness ok, Geometrie 37/37. **Offen: 6 von 146 Peer-Tests rot** im seriellen Einzellauf (`business_command_idle_wait_rechecks_a_change_seen_before_notifier_arm` Timing 750 ms; `native_peer_consumes_pending_knowledge_command`, `…module_governance_commands`, `…marks_invalid_ticket_commands_failed` = Intake-Verhalten `pending_sync` statt `failed`; `native_peer_sync_config_change_detects_room_rotation`; `…materializes_procedural_knowledge` = `/payload/markdown` vs. `markdown`). Worker behauptet: betroffene Pfade gegenüber Basis unverändert, aber **keine Baseline-Messung**. **Baseline gemessen (19:01–19:24, Worktrees `base-pr1` = `2d88f2267` und `review-pr1` = `fde5d0702`, Target `~/.cache/ctox-review-pr1-target`, `--exact --test-threads=1`, Log `/Volumes/tmp/dev-artifacts/ctox/crew-cockpit/baseline/summary.txt`):** Basis/Head identisch für alle sechs — `idle_wait…notifier_arm` grün/grün (223 s / 181 s; das Rot des Workers war Host-Last), `…room_rotation` grün/grün, `…pending_knowledge_command` rot/rot, `…module_governance_commands` rot/rot, `…invalid_ticket_commands_failed` rot/rot, `…procedural_knowledge` rot/rot. **Keine Regression durch PR-1; vier Altfehler von main** (Backlog B4). **Fix-Runde 1 (Worker-Push 20:06, Commits 3e05b7a66, 4f4443270, edc101047):** unabhängig verifiziert — 14 von 17 Code-Punkten substanziell behoben mit Tests, keine Assertion entfernt; **nicht angefasst: 17 (Legacy-Grants), 18 (Audit-Whitelist), 19 (Terminal-Guard über mehrere Links)**; **neu durch Fix 12: `millis()`→`Option` kann `null` in required/Index-Zeitfelder schreiben (Punkt 20)**. Runde 2 mit 17–20 an den Worker 20:16. Pause-fail-open (Punkt 3) als Entscheidung akzeptiert. **Eigener Testlauf auf `edc101047` (20:07–20:14, Target `~/.cache/ctox-review-pr1-target`, TMPDIR Systemplatte): harness_cockpit 13/13, command_plane 18/18, store_projections 9/9, store_policy 4/4, cockpit_mcp_policy 1/1, local_transport 11/11, harness_flow 5/5, queue_capacity 1/1, worker_attempt 2/2 — alle grün** (`baseline/head-tests-summary.txt`). Für den Bau war das Pi-Sidecar-Bundle nötig (`include_bytes!` in `pi_sidecar.rs:42`); selbst gebaut unter `/Volumes/tmp/dev-artifacts/ctox/crew-cockpit/sidecar-build/dist` (npm ci + npm run build, 12,5 MB) und in beide Worktrees gesymlinkt. KORREKTUR: Der Worker hat sein Cargo-Target (31 GiB) trotz meiner Anweisung gelöscht — die Nachricht kam erst nach seinem Turn-Ende an (Codex-Queue liefert zwischen Schritten). Kaltbau nötig. CI zusätzlich rot: aarch64-CLI `E0282` in `office-engine/src/ops.rs:293` (nicht im Diff, main-seitig). Worker-Cargo-Target `~/.cache/ctox-crew-cockpit-target` (31 GiB) bleibt für meinen eigenen Testlauf stehen. Fertig heißt: Fixes im PR, Worker-Abschlussbericht, Kimi-Befunde bewertet, eigener `cargo test`-Lauf der Cockpit-Module grün, dann Draft → Ready und Merge.

### To-Do

- **T1 · PR-2 „Crew-Identität im Harness“ (Server)** — Trigger: PR-1 gemerged oder zumindest reviewt (Schema stabil). Gleicher Brief, Abschnitt PR-2. Serieller Harness und Review-Gates bleiben unverändert.
- **T2 · PR-3 „Cockpit-App“ (Neubau `modules/ctox`)** — Trigger: PR-1 + PR-2 gemerged. Brief folgt (`docs/dev/crew-cockpit-brief-pr3.md`). Enthält Design-Vorgaben (Hierarchie, keine Leerflächen, Wesen-Semantik).
- **T3 · PR-4 „Crew-Leiste“ (Rework `shared/business-chat.js`)** — Trigger: PR-3 in Review (gemeinsame Wesen-Komponente steht). Brief folgt.
- **T4 · PR-5 „Tickets“** — Trigger: PR-1 gemerged (Routing-/Lease-Felder projiziert). Brief folgt.
- **T5 · Unabhängiges Review (Kimi · Cyber & Review) je PR** — Trigger: PR offen. Fokus: Policy-Gates serverseitig, keine HTTP-Datenbrücke, Retention, unbegrenzte `find()`.
- **T6 · Abnahme live** — Trigger: PR-3/4 auf einer Tenant-Instanz (thesen: `currentSlot: null`, src/ ist live; welsch: Slot 0.1.44 aktiv → Slot-Schnitt aus main nötig). Browser-Beweis: eine echte Aufgabe, Fortschritt sichtbar in Text und Wesen, Antwort erscheint, Abbruch funktioniert.

### Backlog + Owner

- **OWNER: Sichtbarkeit `ctox_harness_status` für Rolle „User“?** Vorschlag: Admin + Founder sehen alles; User sieht nur eigene Tasks und Crew-Namen, keine Kosten. Bis Entscheidung wird Vorschlag umgesetzt.
- ~~OWNER: Lokale Instanz reparieren~~ → erledigt, siehe D4 (Owner hat mir die drei Punkte übertragen, 05.09. 12:46).
- ~~OWNER: welsch-Upgrade~~ → geklärt, siehe D5.
- ~~B3 Workjet-Slider/Stundenzettel~~ → geklärt, siehe D6.
- B1 · Abbruch eines laufenden Turns (Slice-Kill) existiert serverseitig nicht; nur `ctox stop --force`. In PR-1 als bounded Stretch (`ctox.queue.abort_turn`), sonst Folge-PR.
- B2 · Tickets zählen nicht zum Queue-Druck (`pending_queue_task_count_uncached` zählt nur `channel='queue'`). Entscheidung in PR-5: zählen oder explizit „zählen nicht“ dokumentieren.
- B4 · **main ist rot, unabhängig von PR-1:** (a) vier `rxdb_peer`-Tests scheitern auf `2d88f2267` (Baseline oben); (b) CI `assert-app-platform-freeze.mjs` meldet `modules/explorer/index.js` (auf Basis reproduziert); (c) CI Desktop-E2E scheitert an `npm audit` (fast-uri high, xmldom moderate); (d) **alle fünf CLI-Checks** am PR-Head rot wegen `E0282` in `office-engine/src/ops.rs:293` (`text.as_ref().is_empty()`), eingeführt mit main-Commit `0b4b14a25` (fix(office), 05.09. 14:47), auf `origin/main f9024b4b8` (20:54) weiterhin vorhanden; main-CI-Run `33984218175` (c93191408) = failure. Der PR fasst office-engine nicht an; CI läuft auf dem Merge mit main. (e) android-Job: Workflow führt ein Python-Heredoc unter `/usr/bin/sh` aus (`import: not found`), Workflow-Bug auf main. **Aktion (21:05):** Worktree `main-fix` auf `origin/main`, `cargo check -p ctox` zur Reproduktion läuft (Log `baseline/main-check.log`); danach minimaler Fix-PR für E0282, damit vier von fünf CLI-Checks wieder grün werden. Freeze-Check und npm audit bleiben Backlog.
- B5 · `/Volumes/tmp` zu 100 % voll (05.09. 19:16, 1,3 GiB frei): Cargo-Targets fremder Sitzungen `sync-core-offensive` 22 GiB, `thesen-queue-20260905` 16 GiB, dazu Modellverzeichnisse. Nicht meins, nicht gelöscht; Worker auf Systemplatte umgeleitet. OWNER: aufräumen oder freigeben.
- B3 · Workjet-„Slider“ zur Worker-Personalisierung und „Stundenzettel“: im lokalen Checkout `claude-workjet` (WorkerEditorView.swift, Models.swift) gibt es nur Name/Rolle, Modell, Reasoning-Stufe (Choice-Buttons), Aufgabe, Skills-Toggles; weder Slider noch Stundenzettel (`rg -i stundenzettel|timesheet` → 0 Treffer). Beide Konzepte werden hier eigenständig definiert (Soul-Achsen als Slider, Stundenzettel = Run-Einträge + Rückblick je Mitglied).

## Befunde (verifiziert; Zeilenangaben aus den Audits, Stichproben gegengelesen)

### A · Der Harness ist für den Browser fast unsichtbar (Ursache aller drei Oberflächen)

1. `ctox_runs` ist eine leere Hülle: Schema, Registry, MCP-Reader existieren, **kein Writer** (`grep -rl ctox_runs src/core` → nur `mcp_channel.rs`).
2. Per-Tool-/Token-Ereignisse sind durable (`ctox_harness_flow_events`, Writer `service.rs:5292-5305`), erreichen den Browser aber nur als 12-Ereignis-Blob **einer** Kette in `ctox_runtime_settings.harness_flow` (`harness_flow.rs:403-412`), stamp-gated 3 s → 1800 s idle (`rxdb_peer.rs:877-891`), **admin-only** (`policy.rs:317-324`).
3. `ctox_queue_tasks` projiziert weder `lease_expires_at`, `lease_worker_id`, `failure_class`, `failure_attempt_count`, `retry_not_before`, `hold_reason` noch `wait_entity_*` (`QueueTaskView` `channels/mod.rs:281-299`). „Blockiert“ ist deshalb nie „wartet auf X bis T“.
4. Service-Liveness im Browser = PID-Probe (`store_sync_turn_auth.rs:292-317`): `busy`, `worker_active_count`, `worker_phase`, Kapazität, Arbeitszeitfenster, Druckzustand — alle nicht projiziert.
5. Kosten/Modell je Turn nur in `api_model_cost_events` (CLI `ctox cost`), keine Projektion.
6. Steuerung aus dem Browser: create/update/delete/`ctox.command.cancel`. Nicht erreichbar: release, block, capacity, pause, spill/restore, abort (`queue.rs:33-49`, `service_queue_capacity.rs:23-33`).
7. Keine Retention: `ctox_queue_tasks` wächst unbegrenzt (946 Zeilen lokal), repliziert an jeden Browser; nur Tombstones werden nach 7 Tagen gefegt.

### B · CTOX-App (`modules/ctox`, 4874 Zeilen JS)

1. Rendert die **Spezifikation** des Harness (statisches 16-Knoten-Poster mit Enum-Namen `AwaitingReview`, `ReviewUnavailable`, hart kodierte x/y `index.js:411-440`) statt seinen **Zustand**.
2. Nutzt `commandBus.cancel` nie; kein Stop-Knopf. Vier Writes gesamt (`index.js:1155, 2977, 3025, 3068`).
3. `lease_owner`, `status_note`, `error`, `workspace_root`, `ctox_runs` nie gerendert; Fehler zeigen generisches „Aktion fehlgeschlagen“.
4. `find().limit(200)` ohne Selektor/Sort, dann `slice(0,20)`: bei >200 Commands fehlen die neuesten (`index.js:3944-3954`). Live: `business_commands` (190 Docs) wird ~1×/s vollständig geholt (Konsole welsch).
5. `main.innerHTML = …` alle 4 s (`index.js:1888`): Fokus und Drag gehen verloren; alle Element-Refs veralten (live: Klick-Refs nach 2 s ungültig).
6. Redaktion per Regex (`hasSensitiveUiLeak` `index.js:4559-4620`) blendet praktisch jeden Coding-Prompt aus und deaktiviert die Textarea (`:2884`).
7. Fokus-Task aus dem Chat (`sessionStorage['ctox.businessOs.focusTask']`) wird nie gelöscht → Auswahl springt alle 4 s zurück (`index.js:632, 689, 2698-2705`).
8. Alle vier Loads `.catch(() => [])` (`index.js:614-619`): DB kaputt = „Keine Arbeit hier“. Live lokal: „Tasks werden synchronisiert“ ohne Ende.
9. Web-Stack-Panel (Sales-Browser-Secrets) in der Harness-Ansicht (`index.js:3888-3963`).
10. Zähler-Widerspruch live: „Arbeitet (4)“ bei vier Tasks mit Chip „Fehler“; „Zeit 3178 m“ für einen fehlgeschlagenen Task.
11. i18n doppelt (Inline-Tabelle 184 Keys + `locales/*.json`), 50 tote Keys; englische Lane-Labels im deutschen UI, deutsche `aria-label` im englischen.
12. `module.json:34` verspricht „runtime scopes“ links — existiert nicht.
13. Tests (`test.js`, 28): überwiegend Markup-String-Regressionen; kein Write-Pfad getestet.

### C · Crew-Leiste (`shared/business-chat.js`, 8332 Zeilen, davon ~3000 CSS im Template-String)

1. Antwort kommt **einmal, terminal**, über `business_commands.outbound_text` (`store_projections.rs:32-79`); Zwischenstände, `retry_wait`, Review-Urteile erreichen den Chat nie (live bestätigt, F-D2).
2. Fortschritt nur als `title`-Tooltip (`executionProgressTooltip` `:2320`); `executionProgressHeaderHtml` gibt `''` zurück (`:2310`) und wird trotzdem aufgerufen.
3. Composer wird bei `queued|running|blocked` entfernt (`:2515`): kein Nachsteuern, kein Abbruch (`commandBus.cancel` 0×).
4. Wesen-Identität = Hash der Command-ID → Name/Form/Farbe zufällig, wechselt mitten im Gespräch (`:1952-1978`, live Tavi → Milo). Es gibt keine Crew-Mitglieder als Entität.
5. Deep-Link in die CTOX-App per `window.dispatchEvent` auf dem Shell-Window (`:3947`); Modul hört im eigenen iframe (`modules/ctox/index.js:4030`) → wirkt nur bei Remount.
6. Drei konkurrierende Timeouts (30 s `app.js:7514`, 12 s `:3535`, 3,5 s `:3818`), zwei Autoren für `business_chats.messages` (Browser `:4224`, Server `store_projections.rs:268`), Merge per `mergeChatMessages` (`:4480`).
7. `hydrateChatsFromRxDb` = `find().exec()` über alle Chats ohne Selektor/Limit (`:4336`).
8. Keine i18n-Anbindung, 64 nur-deutsche Literale (`:2461-2545`, `:3398-3481`, `:3908-3929`); Server-Literal deutsch (`store_projections.rs:253`).
9. Dialog-Host = globaler `window.__ctoxBusinessDialogHost` des zuletzt gemounteten Moduls (`dialogs.js:8-16`, `app.js:6203-6207`).
10. Zweite parallele Sende-UI in `app.js:14353-14680` mit eigenen Labels und eigenem Dispatch.

### D · Tickets (`modules/tickets`, 1415 Zeilen; Server `src/core/mission/tickets/`)

1. Domäne ist überbaut, aber kohärent: 17 Case-States (`case_state.rs:23-39`) und 21 Work-Item-States (`work_item_status.rs:3-23`) ohne Mapping; Fälle entstehen **nur** über `create_dry_run` (`cases.rs:29`), das kein Business-Command erreicht → Approval/Verification/Writeback-Hälfte ist aus dem Browser unerreichbar (lokal: `ticket_cases` 0, `ticket_approvals` 0, `ticket_self_work_items` 6).
2. Self-Work-Join kaputt: `remote_ticket_id === ticketKey` vergleicht `LT-…` mit `local:LT-…` (`index.js:1252-1254`) → immer „Kein Self-work verknüpft“.
3. Filter auf `remote_status` (Fremdsystem) statt CTOX-Zustand (`index.js:226-234`); Zustände unübersetzt snake_case → Title Case (`:1362`).
4. 12 unbegrenzte `find()` je Refresh (`index.js:573`, alle 80 ms debounced).
5. `module.json:63` „Read-only“ ist falsch: 9 Write-Commands (`index.js:1001-1081`); Permission-Scope `support` statt `tickets` (`command_plane.rs:819-822`).
6. Tickets zählen **nicht** zum Queue-Druck; Ticket-Arbeit ist vom Parallel-Pool ausgeschlossen (`service_queue_capacity.rs:42-55`); Spill-Scorer nur per CLI (`queue.rs:418, 432`).
7. Routing-Felder (`failure_class`, `retry_not_before`, `hold_reason`, `lease_owner`) geladen und verworfen (`index.js:852`).

### E · Gestaltung (Nutzerbefund, live bestätigt)

Große Leerflächen, Text-Labels statt Hierarchie („nicht erfasst“ ×5, „keine Live-Tokenmetriken“ ×16), Zustände nur über Farbe/rote Chips; die Wesen tragen keine Bedeutung (Zustand ≠ Ausdruck; X-Augen existieren im Code `crewEyesMarkupForMode('failed')`, hängen aber am zufälligen Chat-Hash statt am Task-Zustand eines echten Mitglieds).

## Zielbild (Entscheidungen)

1. **Harness bleibt seriell, alle Review-/Validierungs-Gates bleiben.** Neu ist ausschließlich: Sichtbarkeit (Projektionen), Steuerbarkeit (Control-Commands) und Identität (Crew-Mitglieder als durable Entität mit Seele, Lebenslauf, Learnings; Auswahl nach Passung beim Lease).
2. **Server-autoritativ, kein HTTP-Datenpfad.** Alles Neue sind RxDB-Projektionen mit Retention + Indizes und `EXACT_CONTROL_TYPES`-Commands hinter `enforce_command_policy`.
3. **Eine Wesen-Komponente** für Chat, Cockpit und Tickets; Ausdruck = Harness-Zustand (wartet · aufgewacht · denkt · arbeitet mit Werkzeug · prüft · wartet auf X · gescheitert (X-Augen) · fertig), nicht Farbe.
4. **Die CTOX-App wird das Zuhause der Crew:** Die Mitglieder leben dort; wer den aktuellen Task hält, ist „im Einsatz“ (Arbeitsplatz-Ansicht mit Plan-Schritten, Live-Aktivität, Runs: Modell, Tokens, Kosten, Dauer, Urteil, Grund, Steuerung), alle anderen sind zu Hause (ruhen, warten, lesen ihre Learnings). Jedes Mitglied führt einen **Stundenzettel** (je Run: Beginn, Ende, Task, Ergebnis, Aufwand, eigener Rückblick), sichtbar im Profil; Statuskopf = Harness läuft/pausiert, Kapazität, Druck, Warteschlange. Kein Poster, keine Leerflächen.
5. **Chat = Steuerkanal:** Composer bleibt offen, Zwischenstände/Fragen/Urteile erscheinen als Nachrichten des Mitglieds, Abbruch/Retry/Priorität inline, ein Klick öffnet den Task im Cockpit (postMessage).

## Umgebungsfallen

- Codex-Worker-cwd ist `~/Documents/ctox` (137 hinter / 233 vor origin/main). PR-Arbeit nur in Worktrees unter `/Volumes/tmp/worktrees/ctox/<branch>` von `origin/main`; nie den Checkout selbst editieren.
- `/Volumes/tmp` ist klein (ENOSPC am 02.09.); Cargo-Target vor Builds mit `df -h` prüfen, sonst `~/.cache/ctox-crew-cockpit-target`. Target nach Gebrauch löschen (≈13 GiB je Testbau).
- `cargo fmt` nur je Datei (`rustfmt <datei>`), nie paketweit.
- RxDB-Wire-Contracts: Fixtures in `src/core/rxdb/tests/fixtures/*.json` ändern, beide Seiten regenerieren, `dist/ctox-rxdb-js.mjs` nie von Hand; `?v=`-Buster in `shared/rxdb-runtime.js` bumpen.
- Shell-Stempel: genau EIN `-shell-v2-`-Token in `index.html`/`app.js` (`grep -o '?v=[^"]*' index.html app.js | grep -- -shell-v2-`), sonst 409 beim Boot.
- welsch: Slot 0.1.44 aktiv → Datei-Deploy nach src/ unsichtbar; thesen: `currentSlot: null` → src/ live.
- In-App-Browser (Claude): Koordinaten-Klicks im 800×500-Frame treffen die Business-OS-Shell nicht zuverlässig; Refs nach Re-Render sofort veraltet → Interaktionen per `find`-Ref direkt danach oder per JS auslösen. Gegen die LOKALE Instanz baut er keinen WebRTC-Datenkanal auf (nur mDNS-Host-Kandidat); gegen welsch funktioniert es über TURN. Lokale Sync-Beweise also nur in Chrome/Safari führen.
- Lokale Instanz (nach D4): `current` → `business-os-shell-v0.1.44`, Wrapper `~/.local/bin/ctox` exportiert `CTOX_STATE_ROOT=releases/…/runtime` (Symlink auf `~/.local/state/ctox`, also derselbe State-Root); das Release-eigene `bin/ctox` verweist auf ein nicht existierendes `bin/ctox-real` und ist damit unbrauchbar, der echte Binary liegt in `~/.local/bin/ctox-real` (04.09. 19:12).
- welsch-Administration nur über `~/Documents/ctox-dev`: `npx tsx output/run-remote-welsch.ts <lokales-skript.sh>` (Skript wird per SFTP hochgeladen und mit bash ausgeführt). Kein `timeout` auf macOS; das Bash-Tool-Timeout nutzen.

## Fehlermuster (eigene)

1. Klick-Beweis ohne DOM-Prüfung: zwei Klicks „wirkten“ nicht, weil der Frame falsch war (2×). Immer DOM-Zustand nach Aktion lesen.
2. Auf `find()`-Refs vertrauen, während das Modul alle 4 s neu rendert (1×).
3. `mv symlink.new current` auf macOS, wenn `current` ein Symlink auf ein Verzeichnis ist: `mv` legt den neuen Link IN das Zielverzeichnis. Richtig: `ln -sfn <ziel> current` (1×; Streuner in `releases/workjet-sync-efe5ef1a7/current.new` entfernt).

## Evidenzkarte

- Audits (Subagenten-Berichte, im Chat-Transkript dieser Sitzung; Kernaussagen oben übernommen und stichprobenartig geprüft).
- Live-Command welsch: `business_commands/cmd_b964f8bc-2130-42df-80f8-53e7fe9b2961` (RxDB im Browser), Task `queue:system::41c33261fb7b96906277e159`.
- Lokale Maintenance: `http://127.0.0.1:8765/api/business-os/ctox/maintenance`; Peer-Status `~/.local/lib/ctox/releases/business-os-shell-v0.1.44/runtime/business-os-rxdb-peer.status.json`.
- Vision/Onboarding für alle Mitbauenden: `docs/dev/crew-cockpit-vision.md` (Commit e96434448; nach Owner-Hinweis 13:20 ergänzt: der Worker bekam vorher nur den Vertrag, nicht das Wofür).
- Briefs: `docs/dev/crew-cockpit-brief-pr1-pr2.md`, weitere folgen im selben Verzeichnis. Jeder Brief verweist zuerst auf die Vision.
- Board-Artefakt: https://claude.ai/code/artifact/9b10debc-a89e-443d-a93e-8a69d8c86d0b (stabile URL, bei jedem Update dieselbe Datei neu veröffentlichen).
