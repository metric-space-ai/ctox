# CREW-UX Board (ab 2026-09-27)

Kritischer Pfad: welsch-Upgrade (Binary → main 40a6b4616) + Release-Lauf beta.58 → Slot beta.58 aktivieren → Browser-Abnahme; parallel S4 Shell-Tiefe und S5 Crew-App für beta.59.

Owner-Auftrag 27.09.: "setze den plan um und sorge auch dafür, dass welsch.ctox.dev wieder funktioniert, so dass wir es hier erproben können" + "die ganze UI/UX-Implementierung der Crew … muss überall optimiert werden" + Vorgehen im Codex-Thread 01a0879f-bdaa-77e3-b877-76bc040eacf9 teilen.

Arbeitsklon: `~/.local/state/workjet-launchpads/ctox-crew-genome` (Basis origin/main `ccde06f4d`, remote GitHub). NICHT `~/Documents/ctox` (1109 Commits hinter main).

## Done

- **S0 · welsch-Sync repariert (27.09. ~07:35 UTC, verifiziert)** — Befund: welsch lieferte `signaling_urls=["ws://127.0.0.1:18894"]` seit 26.09. 19:15:37 UTC (nativer Peer: "sync config changed" 19:15:42). Quelle: `/home/ctox/.local/state/ctox/business-os-signaling-urls.json` (runtime-Symlink zeigt auf state), geschrieben über `CTOX_BUSINESS_OS_SIGNALING_URLS` aus einer Release-Prüfung (Port 18894 = `~/.cache/ctox/release-checks/crew-outbound-conflict-539-20260926/*`). `store.rs::signaling_urls_config` persistiert jede Env-Übersteuerung dauerhaft. Fix: Datei nach `~/.local/state/ctox/backups/signaling-urls-loopback-20260926T191537.json` verschoben, kein Neustart. Beleg: Peer-Journal "multiplexed WebRTC replication up for 205 collections"; Browser (interner Pane) `signalingUrls=["wss://signaling.ctox.dev/v2"]`, 28 connected / 1 pending / 2 reused, Crew-Collections connected, 32 Wesen gerendert. Angekündigt im Codex-Thread (Queue-Nachricht `01a0e1c6-4299-7600-8151-f1f6ef3f3b25`).
- **Analyse Ausgangslage (27.09., Code origin/main + Messung welsch v375)** — 4 feste Körperpfade × 6 Farben (`crew-renderer.js:31-42`, DB-CHECK `crew/mod.rs:109`); Lumi `#7d7f84` = Neutralfarbe; arbeitende Wesen 0/263 Frames bewegt, Ereignis = 1400 ms Zucken bei 30 fps; 6 Keyframes + 12 Variablen tot (`animation:none`); Mitglieder ohne Telemetrie → nie bewegt; `will-change` auf jedem Wesen; CSS in 5 Kopien; Tickets importiert `business-chat.js?v=…v339` (eigene Modulinstanz); Karte SVG→foreignObject→HTML→SVG.

- **S1+S2 · Genom-Renderer + Bewegungs-Engine (main `90c9fa1ac`, verifiziert)** — `shared/crew-renderer.js` (Genom aus id/Name + Archetyp + Farbe; Dreieck als abgerundetes Polygon; neutraler Geist `is-neutral`), neu `shared/crew-motion.js` (seitenweit, MutationObserver, Grundpose je Zustand, Impulse nur aus dauerhaften Turns, Übergänge, Blinzeln/Blicke, IO-Pause, reduced-motion). Belege: Headless-Sonde (Galerie) 16,6 ms Median-Update, Impulse nur working/review, `wake` bei Moduswechsel, reduced → 0 Transforms; Tests shared 145/145, ctox 6/6, tickets 14/14, Crew-Karte 7 Szenarien, Chat-Verhalten, Layout 7/7 (`PLAYWRIGHT_CHANNEL=chrome`), Shell-Vertrag 37/37. Wächter-Vertrag geändert (Owner 27.09.): Golden-Bytes → Genom-Tests; "wartend still" → "keine Geste ohne Turn".
- **S3 · Einbindung (main `9c9cf4df9`, verifiziert)** — CSS-Kopien in app.css/tickets/ctox auf Größen reduziert; Engine installiert `CREW_CREATURE_CSS` (id `ctox-crew-creature-css`); Tickets rendert über Renderer (kein zweites business-chat v339); gemeinsamer Buster `?v=20260927-crew-genome-v1`; kein `will-change` in foreignObject (WebKit). foreignObject selbst bleibt vorerst (S5).
- **Stempel v404 (main `40a6b4616`, gepusht 07:51 UTC)** — `20260927-shell-v2-crew-genome-v404` in 6 Dateien/38 Stellen; Wächter shell-generation+thesen-contract 8/8, data-plane, registry, allowlists, rxdb-only, Branding, Chrome, Content-Audit, shell-artifact 16/16 grün.

## Working

- **welsch-Upgrade Binary → main** — Unit `ctox-crew-genome-upgrade-20260927` (gestartet ~07:52 UTC, RuntimeMaxSec 5400). Fertig heißt: `update_state.json` phase=completed, current_release neu, Wartung completed, Dienst aktiv. Achtung Symlink-Falle (Wartungssperre-Memory). Grund: Shell von main erwartet workjet_computers-Hash + rowsFetch.
- **Shell-Release beta.58** — Tag `business-os-shell-v0.1.46-beta.58` → `40a6b4616`, GitHub-Run `36304466632` (letzter Lauf 55 min). Fertig heißt: Run success, Release-Assets signiert.

## To-Do

- **welsch Slot beta.58 aktivieren** — Trigger: Upgrade completed UND Run success. `shell-update stage --version 0.1.46-beta.58` (systemd-run) → activate → `systemctl --user restart ctox.service`; dann Browser-Abnahme (Wesen, Engine, Sync).
- **S4 · Shell-Tiefe** — Trigger: jetzt (parallel). Fenster/Chatfenster/Crew-Leiste mit Haarlinie, Ebenenschatten, angehobener Fläche; Geometrie-Labor + Vertrag grün.
- **S5 · CTOX/Crew-App** — Trigger: S4 committed. Owner: "im Harness-Flow zu viel scrollen, zu viele freie Flächen, Crew schwebt nur komisch rum". Harness-Flow kompakt, Crew lebt im Flow.
- **S6 · Shell-Release welsch** — Trigger: jede grüne Scheibe ab S3. Stempel-Bump, Tag `business-os-shell-v0.1.46-beta.58+`, welsch `shell-update stage/activate`, Browser-Abnahme. thesen NICHT.

## Backlog / Owner

- **Code-Fix Env-Persistenz** — `store.rs::signaling_urls_config` schreibt Env-Übersteuerung dauerhaft in den State-Root; Release-Checks mit gesetzter Variable vergiften so Produktion. Rust → gebündeltes Upgrade später.
- **OWNER: Seelen-Achsen → Temperament?** — Seele ist nicht in der öffentlichen Projektion (`public_fields` ohne soul); Temperament kommt vorerst aus dem Genom-Seed.

## Environment traps

- `~/Documents/ctox` ist abgedriftet (305 vor / 1109 hinter origin/main) — nur im Klon arbeiten.
- Shell-Slots übersteuern `src/` auf welsch (aktiv `0.1.46-beta.54`, desired `beta.57`); sichtbar nur über Release-Tag + stage/activate + Restart.
- Stempel-Vertrag: APP_BUILD = `app.js?v=` in index.html = `MULTI_TAB_COORDINATOR_EPOCH`, Kennzeichen `-shell-v2-`; genau EIN `-shell-v2-`-Token.
- Unversionierte `shared/*.js`-Importe (z. B. `./crew-renderer.js`) cached der Edge bis 4 h.
- Codex-CLI: `/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex queue --thread <id> --message …` (kein `codex` im PATH).
- welsch-SSH nur über Control Plane: `cd ~/Documents/ctox-dev && npx tsx output/run-remote-welsch.ts <skript.sh>`.

## Error patterns

1. Env-Übersteuerung wird dauerhaft persistiert → Testläufe vergiften Produktion (1×, 26.09.).

## Evidence map

- Diagnoseskripte: Session-Scratchpad `welsch-diag-signal*.sh`, `welsch-fix-signal.sh` (flüchtig).
- welsch-Backup: `/home/ctox/.local/state/ctox/backups/signaling-urls-loopback-20260926T191537.json`.
