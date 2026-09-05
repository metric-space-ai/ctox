# CREW-COCKPIT · Board (CTOX-App + Crew-Leiste + Tickets + Crew-Identität)

Stand: 2026-09-05 12:40 · Orchestrator: Fable · Implementierer: Codex-Thread `01a07107-d27c-7e80-a1dc-2311f60ad0bb` (Arbeit landet in PRs gegen `metric-space-ai/ctox` main)

**Headline:** Kritischer Pfad ist PR-1 (Harness-Projektionen + Steuerbefehle): ohne sie kann keine Oberfläche zeigen, was der Harness tut. Danach PR-2 (Crew-Identität im Harness), dann erst die drei Oberflächen.

## Kanban

### Done

- **D1 · Ist-Analyse Code (4 Audits, 05.09. 12:05–12:25)** — CTOX-App (`src/apps/business-os/modules/ctox/`), Crew-Leiste (`shared/business-chat.js`), Tickets (`modules/tickets/` + `src/core/mission/tickets/`), Harness-Observability (`src/core/service/service.rs`, `store_projections.rs`, `harness_flow.rs`). Ergebnisse unten unter „Befunde“. Stichproben per grep verifiziert: `ctox_runs` hat keinen Rust-Writer; `commandBus.cancel` wird in CTOX-App und Chat 0× benutzt; `loadLocalCollection` = `find().limit(200)` → sort → `slice(0,20)` (`modules/ctox/index.js:3944-3954`); `lease_owner`/`status_note` 0× gerendert; `hold_reason`/`retry_not_before` 0× in `src/core/business_os/`; `QUEUE_PRESSURE_GUARD_THRESHOLD = 20` (`service.rs:159`).
- **D2 · Ist-Analyse live (welsch.ctox.dev, 05.09. 12:12–12:25, Rolle Admin)** — Testaufgabe über die Crew-Leiste gesendet: Command `cmd_b964f8bc-2130-42df-80f8-53e7fe9b2961`, Task `queue:system::41c33261fb7b96906277e159`. Beobachtet: Übergabe in Queue nach ~3 s; während der Arbeit nur leerer Balken, Fortschritt ausschließlich im Tooltip („0 % · 20/23 Turns · Denkblöcke 17 · Tools 6“, Prozent bleibt 0 während Turns steigen); Composer verschwindet; nach Abschluss (`execution_phase=retry_wait`, `result.user_message` vorhanden, `status=succeeded`, `attempt=1`) zeigt der Chat **keine Antwort**, die CTOX-App „Wartet“, kein Grund sichtbar. Wesen wechselte nach dem ersten Senden den Namen (Tavi → Milo), weil Identität = Hash(commandId || chatId) (`business-chat.js:1952-1978`).
- **D3 · Ist-Analyse lokal (127.0.0.1:8765)** — Instanz hängt seit 04.09. 17:13 in `status=stale` eines Upgrade-Lease (`/api/business-os/ctox/maintenance`, `retry_action: ctox upgrade --dev`); Browser-Peer nie authentifiziert (`peerAuthenticated=false`); CTOX-App zeigt dauerhaft „Tasks werden synchronisiert“, kein Hinweis auf die Ursache, Banner nicht schließbar. Nicht behoben (Systemzustand des Nutzers, kein UI-Thema; siehe Umgebungsfallen).

### Working

- **W1 · PR-1 „Harness-Cockpit Foundation“ (Server)** — Worker: Codex-Thread oben. Brief: `docs/dev/crew-cockpit-brief-pr1-pr2.md` (Abschnitt PR-1). Fertig heißt: PR offen gegen main, `cargo test` der genannten Module grün, RxDB-Fixtures beidseitig regeneriert, `node src/apps/business-os/rxdb/tests/run-all.mjs` grün, Doku `docs/ctox-rxdb.md` ergänzt. Ich prüfe: Diff, Tests selbst laufen lassen, Projektion auf lokaler Instanz messen.

### To-Do

- **T1 · PR-2 „Crew-Identität im Harness“ (Server)** — Trigger: PR-1 gemerged oder zumindest reviewt (Schema stabil). Gleicher Brief, Abschnitt PR-2. Serieller Harness und Review-Gates bleiben unverändert.
- **T2 · PR-3 „Cockpit-App“ (Neubau `modules/ctox`)** — Trigger: PR-1 + PR-2 gemerged. Brief folgt (`docs/dev/crew-cockpit-brief-pr3.md`). Enthält Design-Vorgaben (Hierarchie, keine Leerflächen, Wesen-Semantik).
- **T3 · PR-4 „Crew-Leiste“ (Rework `shared/business-chat.js`)** — Trigger: PR-3 in Review (gemeinsame Wesen-Komponente steht). Brief folgt.
- **T4 · PR-5 „Tickets“** — Trigger: PR-1 gemerged (Routing-/Lease-Felder projiziert). Brief folgt.
- **T5 · Unabhängiges Review (Kimi · Cyber & Review) je PR** — Trigger: PR offen. Fokus: Policy-Gates serverseitig, keine HTTP-Datenbrücke, Retention, unbegrenzte `find()`.
- **T6 · Abnahme live** — Trigger: PR-3/4 auf einer Tenant-Instanz (thesen: `currentSlot: null`, src/ ist live; welsch: Slot 0.1.44 aktiv → Slot-Schnitt aus main nötig). Browser-Beweis: eine echte Aufgabe, Fortschritt sichtbar in Text und Wesen, Antwort erscheint, Abbruch funktioniert.

### Backlog + Owner

- **OWNER: Sichtbarkeit `ctox_harness_status` für Rolle „User“?** Vorschlag: Admin + Founder sehen alles; User sieht nur eigene Tasks und Crew-Namen, keine Kosten. Bis Entscheidung wird Vorschlag umgesetzt.
- **OWNER: Lokale Instanz reparieren (`ctox upgrade --dev` laut Maintenance-State)** — Systemeingriff, nicht von mir ausgeführt.
- **OWNER: welsch-Upgrade** — Banner „Neues CTOX-Release wird gebaut“ / „release business-os-shell-v0.1.44 has no asset ctox-linux-x64.tar.gz“ am 05.09. 12:15 sichtbar. Wer baut dort gerade?
- B1 · Abbruch eines laufenden Turns (Slice-Kill) existiert serverseitig nicht; nur `ctox stop --force`. In PR-1 als bounded Stretch (`ctox.queue.abort_turn`), sonst Folge-PR.
- B2 · Tickets zählen nicht zum Queue-Druck (`pending_queue_task_count_uncached` zählt nur `channel='queue'`). Entscheidung in PR-5: zählen oder explizit „zählen nicht“ dokumentieren.
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
- In-App-Browser (Claude): Koordinaten-Klicks im 800×500-Frame treffen die Business-OS-Shell nicht zuverlässig; Refs nach Re-Render sofort veraltet → Interaktionen per `find`-Ref direkt danach oder per JS auslösen.
- Lokale Instanz: `current` → `workjet-sync-efe5ef1a7`, laufender Dienst aus `business-os-shell-v0.1.44`, Upgrade-Lease `stale` seit 04.09.

## Fehlermuster (eigene)

1. Klick-Beweis ohne DOM-Prüfung: zwei Klicks „wirkten“ nicht, weil der Frame falsch war (2×). Immer DOM-Zustand nach Aktion lesen.
2. Auf `find()`-Refs vertrauen, während das Modul alle 4 s neu rendert (1×).

## Evidenzkarte

- Audits (Subagenten-Berichte, im Chat-Transkript dieser Sitzung; Kernaussagen oben übernommen und stichprobenartig geprüft).
- Live-Command welsch: `business_commands/cmd_b964f8bc-2130-42df-80f8-53e7fe9b2961` (RxDB im Browser), Task `queue:system::41c33261fb7b96906277e159`.
- Lokale Maintenance: `http://127.0.0.1:8765/api/business-os/ctox/maintenance`; Peer-Status `~/.local/lib/ctox/releases/business-os-shell-v0.1.44/runtime/business-os-rxdb-peer.status.json`.
- Briefs: `docs/dev/crew-cockpit-brief-pr1-pr2.md` (dieser Commit), weitere folgen im selben Verzeichnis.
- Board-Artefakt: https://claude.ai/code/artifact/9b10debc-a89e-443d-a93e-8a69d8c86d0b (stabile URL, bei jedem Update dieselbe Datei neu veröffentlichen).
