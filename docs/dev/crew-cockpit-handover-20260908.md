# CREW-COCKPIT · Handover 08.09.2026 (Fable → Codex-Thread 01a07107)

Stand: main `60c80d753`, welsch läuft Shell-Slot `0.1.46-beta.38` (Revision
`20260908-shell-v2-crew-home-v352`), Daemon `branch-main-20260908T065708Z`.
Board (Wahrheit, mit Evidenzkarte und Umgebungsfallen):
`docs/dev/crew-cockpit-board-20260905.md`, Karten D19–D22 beschreiben die letzten
zwei Tage. Vision/Konzept: `docs/dev/crew-cockpit-vision.md` §7 (Crew = Experten im
MoE-Sinn in EINEM seriellen Harness, Gedächtnis pro Wesen im LCM, Router-Prompt über
Erfahrung, Persona in den Basis-Instruktionen — keine Parallelspeicher, keine Wesen-Sessions).

## 1. Was heute gelandet ist (alles auf main)

| Commit | Inhalt |
|---|---|
| `16781693b` | Gestaltungsdurchgang nach Owner-Kritik: Chat-Leiste (Pool-Zeile raus, ID-Fragmente raus, `humanizeHarnessLine`), CTOX-App (Crew-Zuhause nur ohne gewählten Task → Flow-Karte zurück; Karten mit Wesen-Portrait; Grund/Quelle im Tooltip; Drawer-Signatur + Scroll-Erhalt; Gedächtnis-Editor = Aussagen je Zeile, `anchorsDocumentFromLines`; Fehlerkarte `flex-wrap`), `alignChatWindows` misst `offsetWidth` statt transformierter Rects |
| `cdb7cec4d` | Wesen im App-Logo: `wireCrewAppPresence`/`crewAppPresenceFromTasks`/`applyCrewAppPresence` in `shared/business-chat.js`; Quelle `ctox_queue_tasks.module` + `crew_member_id`, Status running/leased/review/drafting; Ziel `.shell-window[data-owner-id] .shell-window-v2-icon` und `.desktop-icon[data-target] .desktop-icon-glyph`; folgt Queue-`$`, Crew-`$`, `window:opened` (eventBus über `window.CTOX_BUSINESS_OS_APP`), MutationObserver auf `[data-desktop-icons]` |
| `7b87b51fa` | Gestoppte Tasks (blocked/failed/cancelled) behalten EINE Ursachenzeile `.ctox-task-reason.is-problem` (Release-Wächter verlangt das); Meta-Trenner `\00B7` escaped |
| `cff65c121` | Karussell-Fächer bleibt in der Bühne (Schrittweite je Seite auf verfügbaren Raum begrenzt, aktives Fenster bleibt unter seinem Chip) |
| `60c80d753` | Board D21/D22 |

Tags: `business-os-shell-v0.1.46-beta.33/35/36/38` (33/35 scheiterten am Release-
Wächter, 36 und 38 wurden auf welsch aktiviert). beta.31/32/34/37 gehören der
parallel arbeitenden Office-Sitzung — Tags IMMER frisch abfragen (`git ls-remote --tags`).

## 2. Gates (alle grün auf `60c80d753`, aus `src/apps/business-os`)

```
node modules/ctox/test.js                       # 3/3
node modules/ctox/tests/layout.browser.mjs      # 5 Breiten — RELEASE-WÄCHTER, gehört in jede Gate-Liste
node modules/tickets/tickets.test.mjs           # 14/14
node --test shared/business-chat.test.mjs       # 73/73
node scripts/assert-business-chat-behavior.mjs  # 95 Szenarien, misst Geometrie (Überlappung, Streifen, Fächer, Präsenz-Badges)
node scripts/assert-business-chat-layout.mjs
node scripts/assert-ctox-data-state.mjs --output-dir <dir>
node scripts/assert-ctox-crew-map.mjs
node scripts/shell-v2-geometry-lab.mjs --apps ctox,tickets --widths 640,1180,1440
node scripts/assert-shell-v2-contract.mjs; node scripts/assert-rxdb-only.mjs
node scripts/assert-module-collection-allowlists.mjs; node scripts/generate-module-registry.mjs --check
node rxdb/tests/data-plane-guard-smoke.mjs      # Cache-Revision konsistent
```
Plus die Schritte aus `.github/workflows/business-os-shell-release.yml` (Office-Tests,
`qa:shell-office-height`, `qa:office-notifications`, `test:shell-artifact`).
`assert-shell-chat-composition.mjs` ist auf main rot (Timeout `.shell-window-control--minimize`),
in CI auskommentiert — nicht meins. Rust: `cargo test --release -p ctox --bins --
business_os::crew_cockpit_command business_os::policy` 6/6; Vollsuite/Clippy NICHT
gelaufen (Maschine Last 90–130, Systemplatte 14 GB).

## 3. Landung auf welsch (Checkliste, jede Schicht zählt — AGENTS.md)

1. `app.js`/CSS/Modul geändert → Revision bumpen: `crew-home-vNNN` in `app.js`,
   `index.html`, `shared/db.js`, `shared/sync.js`, `shared/rxdb-runtime.js`,
   `modules/ctox/index.js` (35 Stellen, `sed`), dann `data-plane-guard-smoke.mjs`.
2. Neue Collections eines Moduls → `module.json` + `generate-module-registry.mjs`
   (Katalog in `app.js`) + Allowlisten `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS` /
   `BUSINESS_CHAT_DB_COLLECTIONS` + `docs/business-os-db-isolation-inventory.json`.
3. Push auf main, Tag `business-os-shell-v0.1.46-beta.N` (nächste freie Nummer),
   Workflow ~2 min Bau, GitHub-Queue heute 30–45 min.
4. welsch: `~/Documents/ctox-dev` → `npx tsx output/run-remote-welsch.ts <script.sh>`;
   Vorlagen `output/welsch-crew-beta38-{stage,status,activate}-20260908.sh`
   (stage per `systemd-run --user --unit=…`, dann Status bis `phase: ready`,
   activate + `systemctl --user restart ctox.service`, sonst bleibt Phase `restart`).
5. Beweis im Browser: geladene `app.js?v=` prüfen, dann DOM messen — im versteckten
   Claude-Browser-Pane kommen Klicks nicht an und Screenshots sind teils veraltet.

## 4. Entscheidungen (Owner-bestätigt oder von mir, im Board vermerkt)

- Rolle `user` liest `ctox_crew_members` + `ctox_harness_status`; Runs/Events/Learnings
  bleiben Admin/Founder (`policy.rs::user_readable_cockpit_projection`).
- B7: Crew-Leiste unter den Kennzahlen, solange ein Einsatz läuft; Zuhause-Karten im Leerlauf.
- Kartentext: nur Wesen, Titel, Status, Zeit; Ursache sichtbar NUR bei gestoppten Tasks;
  alles andere Tooltip/Drawer. Keine Erklärsätze, keine Rohwerte (Enums, Epochs, Vorlagen).
- Gedächtnis: nur Anchors editierbar (Aussagen je Zeile), Narrative read-only.

## 5. Offen (Priorität absteigend)

1. **OWNER: Modell-Gateway auf welsch** (CLIProxy 127.0.0.1:12435, gpt-5.6-sol): `/v1/models`
   antwortet, Responses laufen in 45–60-s-Timeouts → Router „Punktzahl entschied“, alle 12
   Tasks gescheitert. Ohne das kein Router-Urteil, kein Lernen mit Inhalt, kein Live-Beweis
   „Wesen im App-Icon“. Neu anmelden: Workjet → Einstellungen → Anbieter.
2. Live-Abnahme T11/T12 nach (1): Owner-Auftrag im Chat, `crew_selected` → `crew.memory_read`
   → `crew.learning` (Kette lief am 08.09. 06:00 bereits), zweiter ähnlicher Auftrag geht an
   dasselbe Wesen; Präsenz-Badge am Fenster-/Desktop-Icon während der Task läuft.
3. Rust-Vollsuite + `cargo clippy --all-targets -- -D warnings` auf ruhiger Maschine
   (eigenes Target unter `~/.cache/`, danach löschen).
4. Sync-Stall (Replikation stallt trotz PR #65): Feldbefund
   `docs/ctox-sync-feldbefund-20260907-crew-local.md`, Brief
   `~/.local/state/workjet-launchpads/ctox-crew-cockpit/briefs/sol-sync-stall.md`, Klon
   `~/.local/state/workjet-launchpads/ctox-sync-stall` (Branch `sync/native-peer-request-latency`).
   Sol startet nicht (`skill_runtime_unavailable`).
5. Kleinigkeiten: Ursachenzeile enthält noch die Fehlerklasse („technical“) — ggf. weglassen;
   `assert-shell-chat-composition.mjs` auf main rot; Screenshots dunkel je Ansicht für die Doku.

## 6. Umgebungsfallen (Kurzfassung, Langfassung im Board)

- Aktiver Shell-Slot übersteuert `src/` auf welsch; ohne Tag+Slot+Restart ist nichts gelandet.
- Lokaler Checkout `~/Documents/ctox` ist von main abgedriftet; Landungen aus
  `/Volumes/tmp/worktrees/ctox/merge-test` per `git push origin HEAD:main`.
- Parallel landet eine Office-Sitzung auf main und aktiviert eigene Slots auf welsch — wer
  zuletzt aktiviert, gewinnt; vor Aktivierung `shell-update status` lesen.
- Cargo-Targets nur unter `~/.cache/<name>`, nie zwei parallel, nach Gebrauch löschen;
  nach abgebrochenem `cargo test` Waisen `deps/ctox-*` (PPID 1) killen.
- Nach `ctox upgrade` bleibt die Instanz read-only, bis ein sichtbarer Browser ackt.
