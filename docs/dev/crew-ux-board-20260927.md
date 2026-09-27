# CREW-UX Board (ab 2026-09-27)

Kritischer Pfad (Owner 27.09.: „setze alles um … test in browser, merge zu main, teile Ergebnisse Codex mit“ + „merge auf main und deploye auf thesen.ctox.dev und welsch.ctox.dev“): Rust-Test (Signaling + Lumi-Farbe) läuft unter eigener Lease → push → welsch `ctox upgrade --dev` → Slot beta.63 (bf94a9f3a) sobald signiert → Browser-Abnahme welsch → Bericht Codex-Thread.

Owner-Auftrag 27.09.: "setze den plan um und sorge auch dafür, dass welsch.ctox.dev wieder funktioniert, so dass wir es hier erproben können" + "die ganze UI/UX-Implementierung der Crew … muss überall optimiert werden" + Vorgehen im Codex-Thread 01a0879f-bdaa-77e3-b877-76bc040eacf9 teilen.

Arbeitsklon: `~/.local/state/workjet-launchpads/ctox-crew-genome` (Basis origin/main `ccde06f4d`, remote GitHub). NICHT `~/Documents/ctox` (1109 Commits hinter main).

## Done

- **S0 · welsch-Sync repariert (27.09. ~07:35 UTC, verifiziert)** — Befund: welsch lieferte `signaling_urls=["ws://127.0.0.1:18894"]` seit 26.09. 19:15:37 UTC (nativer Peer: "sync config changed" 19:15:42). Quelle: `/home/ctox/.local/state/ctox/business-os-signaling-urls.json` (runtime-Symlink zeigt auf state), geschrieben über `CTOX_BUSINESS_OS_SIGNALING_URLS` aus einer Release-Prüfung (Port 18894 = `~/.cache/ctox/release-checks/crew-outbound-conflict-539-20260926/*`). `store.rs::signaling_urls_config` persistiert jede Env-Übersteuerung dauerhaft. Fix: Datei nach `~/.local/state/ctox/backups/signaling-urls-loopback-20260926T191537.json` verschoben, kein Neustart. Beleg: Peer-Journal "multiplexed WebRTC replication up for 205 collections"; Browser (interner Pane) `signalingUrls=["wss://signaling.ctox.dev/v2"]`, 28 connected / 1 pending / 2 reused, Crew-Collections connected, 32 Wesen gerendert. Angekündigt im Codex-Thread (Queue-Nachricht `01a0e1c6-4299-7600-8151-f1f6ef3f3b25`).
- **Analyse Ausgangslage (27.09., Code origin/main + Messung welsch v375)** — 4 feste Körperpfade × 6 Farben (`crew-renderer.js:31-42`, DB-CHECK `crew/mod.rs:109`); Lumi `#7d7f84` = Neutralfarbe; arbeitende Wesen 0/263 Frames bewegt, Ereignis = 1400 ms Zucken bei 30 fps; 6 Keyframes + 12 Variablen tot (`animation:none`); Mitglieder ohne Telemetrie → nie bewegt; `will-change` auf jedem Wesen; CSS in 5 Kopien; Tickets importiert `business-chat.js?v=…v339` (eigene Modulinstanz); Karte SVG→foreignObject→HTML→SVG.

- **S1+S2 · Genom-Renderer + Bewegungs-Engine (main `90c9fa1ac`, verifiziert)** — `shared/crew-renderer.js` (Genom aus id/Name + Archetyp + Farbe; Dreieck als abgerundetes Polygon; neutraler Geist `is-neutral`), neu `shared/crew-motion.js` (seitenweit, MutationObserver, Grundpose je Zustand, Impulse nur aus dauerhaften Turns, Übergänge, Blinzeln/Blicke, IO-Pause, reduced-motion). Belege: Headless-Sonde (Galerie) 16,6 ms Median-Update, Impulse nur working/review, `wake` bei Moduswechsel, reduced → 0 Transforms; Tests shared 145/145, ctox 6/6, tickets 14/14, Crew-Karte 7 Szenarien, Chat-Verhalten, Layout 7/7 (`PLAYWRIGHT_CHANNEL=chrome`), Shell-Vertrag 37/37. Wächter-Vertrag geändert (Owner 27.09.): Golden-Bytes → Genom-Tests; "wartend still" → "keine Geste ohne Turn".
- **S3 · Einbindung (main `9c9cf4df9`, verifiziert)** — CSS-Kopien in app.css/tickets/ctox auf Größen reduziert; Engine installiert `CREW_CREATURE_CSS` (id `ctox-crew-creature-css`); Tickets rendert über Renderer (kein zweites business-chat v339); gemeinsamer Buster `?v=20260927-crew-genome-v1`; kein `will-change` in foreignObject (WebKit). foreignObject selbst bleibt vorerst (S5).
- **Stempel v404 (main `40a6b4616`, gepusht 07:51 UTC)** — `20260927-shell-v2-crew-genome-v404` in 6 Dateien/38 Stellen; Wächter shell-generation+thesen-contract 8/8, data-plane, registry, allowlists, rxdb-only, Branding, Chrome, Content-Audit, shell-artifact 16/16 grün.

- **S4 · Tiefenstaffel (main `e9e1cf800`, verifiziert lokal)** — `--elev-*` in app.css (hell/dunkel): App-Fenster mit heller Außenkante + dunkler Kontur + tiefem Schatten; Chatfenster und Crew-Leiste eine Stufe höher (hellere Fläche #1c1f25/#252a32, Innenkante, Außenkontur, Leiste mit Blur); innere Flächen über gescopte --surface/--line. Prototyp auf welsch per temporärem Stylesheet verglichen und entfernt. Layout-Wächter `assert-business-chat-layout.mjs` auf Engine umgestellt.
- **S5 · Crew-App (main `14b2ca826`, verifiziert lokal)** — Karte als kompaktes U 1180×530 (vorher 1760×740), Einpassen-Zoom (76 % bei 1280, ganze Karte ohne Scrollen), Wesen stehen auf der Station, laufen per Bogen + `travel`-Geste zur nächsten, Blase „nutzt ein Werkzeug · 2/4 Quellen sammeln“ nur aus Telemetrie, Turn-Zahl erreicht das Karten-Wesen (normierter Fortschritt → Wire-Form). Tests: ctox 46/46 inkl. neuem Layout-Test, Layout 7/7, Crew-Karte, Geometrie-Labor ctox+tickets 6/6.
- **Stempel v405 (main `9da8daca2`)** — `20260927-shell-v2-crew-live-v405`, Crew-Buster `?v=20260927-crew-genome-v2`.
- **beta.58 abgebrochen (überholt)** — Run `36304466632` cancelled; Tag bleibt als Historie.

- **welsch auf main (27.09. 08:32–08:40 UTC, verifiziert im Browser)** — Upgrade 2. Versuch fertig (`current` → `releases/branch-main-20260927T075827Z`, Unit success). Datei-Deploy 13 Dateien (v405, Backup `~/.local/state/ctox/backups/files-20260927T083227Z`) + 7 Dateien (v406, Backup `files-20260927T083801Z`), alle sha256 = main. Slot aus: `business_os_shell_update_state.state_json.currentSlot` beta.54 → null (Zeilen-Backup `backups/shell-update-state-20260927T083255Z.json`), Restart. Wartung `waiting_replication` → per `[data-maintenance-retry]` bestätigt → completed. Browser: `app.js?v=…crew-depth-v406`, crew-renderer/motion v2, Engine aktiv, Signaling wss://signaling.ctox.dev/v2, 25 connected/5 reused; App-Fenster-Schatten = Tiefenleiter; Chatfenster #1c1f25 + Kante + Kontur; Leiste #1c1f25/92 % + Blur; Crew-Karte kompakt 80 %, Wesen auf Station.
- **Fix v406 (main `2e4e36858`)** — Regel für Fenster mit Workjet-Kategorie setzte den alten schwarzen Schatten per !important erneut; jetzt Token. (Beim Live-Test gefunden.)

- **Actions-Stau entschärft (27.09. 08:47 UTC)** — nur ~2 gleichzeitige Runner; 36 überholte, noch nicht gestartete Läufe abgebrochen (je Workflow+Branch bleibt der neueste; laufende/geplante/Shell-Release unberührt; per `gh run rerun` wiederherstellbar), angekündigt im Codex-Thread (`01a0e20a-…`). Queue 56 → 20.
- **Leisten-Fix (main `99f91c68d`, Stempel v407 `a3b3ed341`)** — In-Place-Pfad verlangt jetzt, dass Streifen-Chips/Navigation/Überlauf dem Soll entsprechen; Browser-Szenario `dock-strip-follows-a-chat-that-leaves` reproduziert ohne Fix exakt den welsch-Zustand (1 Chip bei has-no-chats, 92 px, 2 Reihen), mit Fix grün. welsch: v407 per Datei-Deploy (Backup `files-20260927T090210Z`, 10 Hashes = main), geladen `app.js?v=…crew-dock-v407`, Leiste 56 px.

- **Crew zu Hause steht auf einem Boden (main `2e89a09ab`, v408)** — 96-px-Figuren auf gemeinsamer flacher Bodenlinie auf Fußhöhe (Modulvertrag „keine Verläufe“ eingehalten, erster Entwurf mit Verlauf vom Test „Presentation layer stays compact“ zurückgewiesen). ctox 46/46, Layout 7/7, Geometrie 3/3. welsch per Datei-Deploy (Backup `files-20260927T090555Z`, Hashes = main).

- **Namen + neutraler Geist (main `4e6c7efcd`, v409 `bf94a9f3a`)** — Owner: „für viele Crews fehlen Namen und verschiedene Farben“. Messung welsch: 33 Tasks, 23 mit Mitglied (Lumi 11, Nori 8, Pico 3, Milo 1), 10 ohne; Chats 13/14 mit Mitglied, 7× Lumi. Ursachen: (a) Lumi-Seed = Neutralgrau, (b) Aufgabenliste zeigte keinen Namen, (c) Bug: Chat-Adapter gab normalisierte Identität {name:'Crew'} an den Renderer → jede unzugeordnete Aufgabe wurde graues Mitglied „Crew“ statt Geist. Fix: Name in Mitgliedsfarbe in der Meta-Zeile, „ohne Crew“ + Geist für Unzugeordnete, Namensschild für das ausgewählte Wesen auf der Karte, Rohidentität an Renderer, Palette ohne Grau (Bernstein). Tests: Renderer+Chat 101, ctox 47, Tickets 14, Chat-Harness 146 Szenarien, Layout 7/7. welsch v409 (Backup `files-20260927T09xx`, 15 Hashes = main); Browser: „Lumi · Erledigt · 23.09.“, Karte „ohne Crew · wartet“.

- **thesen auf main-Shell (27.09. 10:07 UTC, verifiziert)** — thesen: Release `branch-main-20260927T005313Z` (Quelle `ccde06f4d`), currentSlot null → src/ gilt; Hash-Stichprobe = ccde06f4d. Seit ccde06f4d nur CREW-UX-Commits in business-os. Deploy 23 Dateien von `bf94a9f3a` (Backup `~/.local/state/ctox/backups/files-20260927T100731Z`, alle Hashes = main); zwei Anläufe scheiterten vorher an Neon-`fetch failed` (Netz), nichts verändert. Browser: `app.js?v=…crew-names-v409`, admin, Signaling wss://signaling.ctox.dev/v2, Engine aktiv, 4 Mitglieder, Leiste 56 px. Angekündigt im Codex-Thread.
- **Compiler-Grenze eingehalten (Codex-Befund 09:5x)** — btls-sys-cmake (make -j2) + rustc = 3 effektive Compiler; Drossel `signaling-throttle.py` hält eigene Nachfahren von Cargo 95709 per SIGSTOP/SIGCONT auf ≤2 (bis 10:02: 8 Pausen/8 Fortsetzungen). Harte Frist 10:25Z, Cleanup 10:30Z, genau EIN Callback per Wächter v2 (`signaling-run-guard-v2.sh`). Danach kein lokaler Heavy-Job vor dem Greppy-Slot (Thread 01a07f6a-83e8-7901-851c-36521e4916b4).

## Working

- **(erledigt) welsch-Upgrade Binary → main** — 1. Versuch `ctox-crew-genome-upgrade-20260927` scheiterte am Platzgate (20 GiB verlangt, 19,5 frei; nichts verändert, Wartung nicht aktiv). Freigemacht: `~/.cache/ctox/build-office-20260906` (7,1 GB, reines Cargo-Target vom 06.09., kein Prozess). 2. Versuch Unit `ctox-crew-genome-upgrade-20260927b`, target `branch-main-20260927T075827Z` (Quelle = main mit S1–S3+v404), Phase building. Fertig heißt: phase=completed, current_release neu, Wartung completed, Dienst aktiv; Symlink-Falle prüfen.
- **Shell-Release beta.63** — Tag → `bf94a9f3a` (v409); beta.60/61/62 als überholt abgebrochen. Fertig heißt: Run success → welsch `shell-update stage --version 0.1.46-beta.63` → activate → restart; „Recovery“ verschwindet.
- **Rust: Signaling-Env + Lumi-Farbe** — im Klon `ctox-crew-genome` (rebased auf main): Signaling-Commit (Env nur prozesslokal, 2 Tests, Doku) + `fix(crew): Lumi gets a real colour` (Seed #e97255, idempotente Migration nur für unverändertes Seed-Grau, Palette ohne Grau, Test). `cargo test --bin ctox -- signaling_ crew::` unter eigener Lease (seit ~09:19 UTC, kalt, -j2). Fertig heißt: grün → push → welsch `ctox upgrade --dev` (Lumi wird Koralle, Env-Falle geschlossen).

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
2. Baseline-Vergleich per `git checkout <rev> -- <dir>` überschreibt UNCOMMITTETE Arbeit im selben Baum (1×, 27.09., S4 verloren und neu eingespielt; stellte außerdem gelöschte Dateien als staged wieder her). Regel: vor jedem Baseline-Vergleich committen oder `git show <rev>:<pfad>` in eine Scratch-Datei / eigener `git worktree`.
3. Eigene Wächterliste unvollständig: `assert-business-chat-layout.mjs` (ci.yml) nicht vor dem Push gefahren → Keyframe-Assertion auf main kurz rot (1×, 27.09., mit e9e1cf800 behoben). Regel: vor Push alle Business-OS-Schritte aus ci.yml + crew-ui-acceptance.yml + business-os-shell-release.yml lokal.

## Altbefunde (nicht von dieser Kampagne)

- (behoben 99f91c68d) Crew-Leiste: veralteter Chip bei `has-no-chats` → „+“ in zweiter Zeile.
- Kopf zeigt „Recovery“ statt Version, solange welsch `src/` statt signiertem Slot serviert (erwartet bis beta.60 aktiv).

- `assert-shell-chat-composition.mjs` rot auch auf main `ccde06f4d`: "Shell-V2 windows expose exactly one title-bar control: [layout, close]" + drei Snap-Kanten null (Layout-Menü-Arbeit auf Branch `codex/shell-v2-layout-menu`).

## Evidence map

- Diagnoseskripte: Session-Scratchpad `welsch-diag-signal*.sh`, `welsch-fix-signal.sh` (flüchtig).
- welsch-Backup: `/home/ctox/.local/state/ctox/backups/signaling-urls-loopback-20260926T191537.json`.
