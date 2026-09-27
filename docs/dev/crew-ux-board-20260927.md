# CREW-UX Board (ab 2026-09-27)

Kritischer Pfad: S1 Genom-Renderer → S2 Bewegungs-Engine → S3 Einbindung → S6 Shell-Release auf welsch; S4 Shell-Tiefe und S5 Crew-App laufen danach in derselben Release-Kette.

Owner-Auftrag 27.09.: "setze den plan um und sorge auch dafür, dass welsch.ctox.dev wieder funktioniert, so dass wir es hier erproben können" + "die ganze UI/UX-Implementierung der Crew … muss überall optimiert werden" + Vorgehen im Codex-Thread 01a0879f-bdaa-77e3-b877-76bc040eacf9 teilen.

Arbeitsklon: `~/.local/state/workjet-launchpads/ctox-crew-genome` (Basis origin/main `ccde06f4d`, remote GitHub). NICHT `~/Documents/ctox` (1109 Commits hinter main).

## Done

- **S0 · welsch-Sync repariert (27.09. ~07:35 UTC, verifiziert)** — Befund: welsch lieferte `signaling_urls=["ws://127.0.0.1:18894"]` seit 26.09. 19:15:37 UTC (nativer Peer: "sync config changed" 19:15:42). Quelle: `/home/ctox/.local/state/ctox/business-os-signaling-urls.json` (runtime-Symlink zeigt auf state), geschrieben über `CTOX_BUSINESS_OS_SIGNALING_URLS` aus einer Release-Prüfung (Port 18894 = `~/.cache/ctox/release-checks/crew-outbound-conflict-539-20260926/*`). `store.rs::signaling_urls_config` persistiert jede Env-Übersteuerung dauerhaft. Fix: Datei nach `~/.local/state/ctox/backups/signaling-urls-loopback-20260926T191537.json` verschoben, kein Neustart. Beleg: Peer-Journal "multiplexed WebRTC replication up for 205 collections"; Browser (interner Pane) `signalingUrls=["wss://signaling.ctox.dev/v2"]`, 28 connected / 1 pending / 2 reused, Crew-Collections connected, 32 Wesen gerendert. Angekündigt im Codex-Thread (Queue-Nachricht `01a0e1c6-4299-7600-8151-f1f6ef3f3b25`).
- **Analyse Ausgangslage (27.09., Code origin/main + Messung welsch v375)** — 4 feste Körperpfade × 6 Farben (`crew-renderer.js:31-42`, DB-CHECK `crew/mod.rs:109`); Lumi `#7d7f84` = Neutralfarbe; arbeitende Wesen 0/263 Frames bewegt, Ereignis = 1400 ms Zucken bei 30 fps; 6 Keyframes + 12 Variablen tot (`animation:none`); Mitglieder ohne Telemetrie → nie bewegt; `will-change` auf jedem Wesen; CSS in 5 Kopien; Tickets importiert `business-chat.js?v=…v339` (eigene Modulinstanz); Karte SVG→foreignObject→HTML→SVG.

## Working

- **S1 · Genom-Renderer** — Worker: ich (direkt). Fertig heißt: `renderCrewCreature` erzeugt Körper/Augen/Farbton aus Genom (id + Archetyp + Farbe), neutrales Wesen = Geist, Galerie-Screenshot 4 Archetypen × 6 Individuen, Tests grün, Commit auf main.

## To-Do

- **S2 · Bewegungs-Engine `shared/crew-motion.js`** — Trigger: S1 committed. Seitenweit (MutationObserver), Grundbewegung je Zustand + Impulse + Übergänge, IntersectionObserver-Pause, reduced-motion.
- **S3 · Einbindung konsolidieren** — Trigger: S2 committed. Ein Renderer + eine CSS-Quelle; Tickets/CTOX importieren nur Renderer; CSS-Kopien (app.css ~9737, tickets ~493, ctox index.css) raus; Karte ohne foreignObject.
- **S4 · Shell-Tiefe** — Trigger: S3 committed. Fenster/Chatfenster/Crew-Leiste mit Haarlinie, Ebenenschatten, angehobener Fläche; Geometrie-Labor + Vertrag grün.
- **S5 · CTOX/Crew-App** — Trigger: S4 committed. Harness-Flow kompakt, Crew lebt im Flow.
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
