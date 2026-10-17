# Strang AGENT – Coding Agent als Seitenleiste der Code-Ansicht (Stand 15.10.2026, ca. 23:20)

Owner-Mängel M-03 („Im Coding Agent öffnen führt zu nichts“), M-04 (Icon im Kopf von Coding Agents), M-05 (Agent
nicht direkt in der App). Bedienkonzept Fassung 4 §3/§3a. Kernvertrag §16.44 (Kern-Draft-PR #848, dda16886e).
Getestet gegen Kern-Stub-Pin **51b1e33** (bildet §16.44 ab). Klon `~/Documents/ctox-shell-v4`, Zweig
`shell-v4/prototype`, kein git außer Lesen; alle Änderungen liegen uncommittet im Klon.

## Was gebaut ist

- **Seitenleiste** `v4/source/agent-panel.js` in der Code-Ansicht (FR6a, „Code ansehen“ im App-Menü), rechts neben dem
  Editor, einklappbar (› in der Leiste, Lasche „Agent“ am rechten Rand, „…“-Menü „Agent aus-/einblenden“). Inhalt:
  Verlauf der Turns dieses Entwurfs (Auftrag, dezenter Punkt-Indikator solange queued/running, Antwort am Turn-Ende,
  geänderte Dateien als Knöpfe → Diff), Eingabe „Was soll der Agent an <App> ändern?“ (Enter sendet, Umschalt+Enter
  neue Zeile), „Abbrechen“ statt „Senden“, solange ein Turn läuft. Keine Statussätze in der Leiste; Ablehnungen und
  Fehler nur in die Zone (`agent.turn`, `agent.conflict`, `agent.patch`).
- **Vertrag** `v4/source/coding-port.js` über die App-API der Shell: `api.command('ctox.coding.turn' | 'ctox.coding.cancel')`
  (Ablehnung kommt als WriteResult `{status:'rejected', code, reason}`), Live-Collection `coding_turns`,
  `draft.info().rev` = `base_draft_rev`, `draft_id = '<actor.id>/<app_id>'`, Patch über `files.get(turn.patch.ref)`.
  Der Rahmen reicht die App-API als `shellAppApi` an die Code-Ansicht (`v4/frame/index.js`, eine Zeile).
- **Abläufe:**
  - Kein Entwurf → leerer Entwurf per `draft.rebase(aktiv)`; Revision erst nach Core-Bestätigung verwenden.
  - `stale_draft_rev` → still neu laden, auf die bestätigte Revision warten (≤ 2 s), genau einmal erneut senden;
    kommt es wieder: Prompt bleibt im Feld (erneut angeboten), kein Zoneneintrag. Häufigster Fall: eigene Eingabe
    lag noch im selben Ausgang vor dem Befehl.
  - `turn_running` → Zone (warn) „Der Agent arbeitet noch …“, Prompt bleibt. Sonstige Ablehnung → Zone (error).
  - `done` → Code-Ansicht lädt neu, erste geänderte Datei öffnet als **Diff**.
  - `conflict` → Zone (warn) mit Aktion „Änderungen übernehmen“ und derselbe Knopf an der Turn-Karte: Patch lesen,
    Null-Kontext-Hunks auf den aktuellen Entwurf anwenden (`text-diff.js applyPatch`; entfernte Zeilen werden geprüft,
    verschobene Stellen eindeutig gesucht, sonst Zone `agent.patch`, Entwurf unverändert), dann Diff.
  - `failed` → Zone (error) mit code/reason/detail.
- **Editor-Kopf** (Konzept-Tabelle §3): Hauptaktion „Veröffentlichen“, daneben „Verwerfen“, „Diff“ als Umschalter
  (nur lesende Zeilen-Gegenüberstellung Basis ↔ Entwurf mit 3 Kontextzeilen, `text-diff.js renderDiff`), alles Weitere
  im „…“-Menü („Ordner in den Entwurf laden…“, „Agent ausblenden“). Hinweis: Eine Text-Diff-Ansicht gab es aus Q3
  nicht (Q3 hat nur den Schema-Diff der Collections); sie ist hier neu.
- **Einstiege (M-03):** `api.shell.openSource(appId)` öffnet die Code-Ansicht der Ziel-App im App-Fenster, die
  Seitenleiste ist offen (Standard). In v4 gibt es keinen weiteren Einstieg „Im Coding Agent öffnen“: der Menüpunkt
  steht nur noch in Shell-V2 `app.js` (Versionsmenü, `openDesktopApp('coding-agents', {moduleId})`) – Shell v4 ersetzt
  `app.js`, deshalb nicht angefasst. „Bugs & Features“ (`modules/reports`, „Im Coding-Agents-Log öffnen“) navigiert zu
  Coding Agents und landet jetzt in der Übersicht der Läufe. News hat keinen eigenen Einstieg (die Owner-Meldung lief
  über Coding Agents, das jetzt umgebaut ist).
- **Coding Agents = Übersicht aller Agent-Läufe (M-04/M-05):** `modules/coding-agents/index.{html,js,css}` neu
  (vorher Chat-Werkbank mit eigenem Turn). Eine Spalte im Standard-Pane-Kopf (kein eigenes Kopf-Icon, kein
  Abstandshalter, das App-Icon oben links ist der Icon-Header der Shell), Suche, Läufe aus `coding_turns` (laufende
  zuerst, dann neueste; App-Titel, erste Zeile des Auftrags, Dateizahl, Uhrzeit, Zustand nur als Punkt mit
  aria-label). Klick → `api.shell.openSource(app_id)`. Deep-Link `{app}` (Altform `{moduleId}`) filtert. Texte in
  `locales/{de,en}.json`. Handler (`handlers/ctox.coding.*`) unverändert. `v4-manifest.json`: `coding_turns: read`.
- **Capability:** `v4/tools/manifest/gen-manifest.mjs` `SHELL_CAPABILITIES` → `shell.manifest.json`
  `capabilities_required: ["agent.coding.cancel","agent.coding.turn"]` mit `_capability_evidence`;
  `check-caps`: OK (Präfix `agent.coding.` unter `agent.turn`).

## Abnahme (selbst gefahren)

| Prüfung | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=…/51b1e33 AG_ONLY=1 node v4/frame/lab/frame.test.mjs stub` | 9/9 (AG-1…AG-7, Einklappen, Seitenfehler), zweimal hintereinander grün |
| dieselbe Abnahme gegen Pin 68b83a3 (ohne §16.44) | AG-6 + AG-0 (Senden → ein Zoneneintrag „noch nicht verfügbar“, kein Befehl) |
| Rahmen-Labor gesamt mock / stub (Pin 68b83a3), 23:13 | 144/144 / 162/162 grün (mit allen AGENT-Änderungen bis auf die Umbenennung `shellAppApi`) |
| dieselben Läufe 23:20 (nach Umbenennung, während FR6c an Leiste/Kopf arbeitet) | mock 154/157, stub 154/158: rot nur „Pin erscheint in der Taskleiste“, FR6b-O1 (FR6c-Überlauf), TOP-1 und Klick auf `[data-v4-settings-button]` (Abbruch vor AG) – Leiste/Kopf von FR6c/TOPBAR (taskbar.js 23:15, in Arbeit), kein AG-Fall; AG-Block allein danach grün (51b1e33 9/9, 68b83a3 3/3) |
| Z45-Block gegen 51b1e33 allein | 10/10 (ein früherer Gesamtlauf gegen 51b1e33 brach in Z45 mit Timeout ab; allein nicht reproduzierbar) |
| `node v4/source/lab/accept.mjs --editor textarea` / ohne Option | 13/13 / 13/13 (Monaco lädt im Labor nicht, s. Befunde) |
| `node --test v4/source/schema-diff.test.mjs` | 13/13 |
| `node --test modules/coding-agents/tests/*.test.mjs` (Pins 68b83a3, 74137b1, 51b1e33) | 8/8 |
| `node --test modules/coding-agents/handlers/tests/*.mjs` | 4/4 |
| `node v4/os-primitives/check-caps.mjs` | OK |
| `node v4/tools/guards/guard-v4.mjs` | Exit 0, keine Treffer in `v4/source/` und `modules/coding-agents/` |

AG-Fälle (alle gegen den echten Stub, keine Attrappe):
AG-6 Einstieg aus „Aufgaben“ per `openSource('notes')`; AG-1 Senden (Umschalt+Enter, Vertrag, Indikator, Abbrechen
sichtbar, Antwort, Dateiliste, Entwurf per CAS geändert, Diff offen); AG-2 `turn_running`; AG-5 Abbrechen
(`ctox.coding.cancel`, danach schreibt der Turn nichts, Entwurf und rev gleich); AG-3 `stale_draft_rev` (offline
getippt und gesendet → Ablehnung, stilles Neuladen, ein zweiter Versuch läuft, kein Zoneneintrag); AG-4 Konflikt
(eigene Änderung in Zeile 1 während des Turns → `conflict`, Patch `files/coding-patch:<turn_id>`, Übernehmen setzt die
Agent-Zeile ans Ende, Diff, Zone leer); AG-7 Coding Agents listet die 4 Läufe, Titel rechts neben dem Shell-Icon,
Klick öffnet die Code-Ansicht mit Seitenleiste.

Mutationsbeleg (`AG_MUTATE=<name> AG_ONLY=1 … stub`, mutierte Datei per Route ausgeliefert, Pin 51b1e33; Basis davor
zweimal 9/9). Jede Mutation macht ihren Fall rot:

| Mutation | Eingriff | Ergebnis |
|---|---|---|
| `enter` | Enter-Taste sendet nicht | AG-1, AG-2 rot (+ Folgeabbruch), 1/4 |
| `turnrunning` | Zonentext bei turn_running leer | AG-2 rot, 8/9 |
| `stale` | kein Wiederholversuch nach stale_draft_rev | AG-3 rot (+ AG-7 Folgezählung), 7/9 |
| `patch` | „Änderungen übernehmen“ schreibt nicht | AG-4 rot, 8/9 |
| `cancel` | Abbrechen sendet kein ctox.coding.cancel | AG-5 rot (+ AG-7), 7/9 |
| `entry` | Rahmen reicht keine Shell-App-API an die Code-Ansicht | AG-6 rot (keine Seitenleiste), 0/2 |

Screenshots selbst angesehen (`v4/frame/lab/shots/integrated-stub-ag-*.png`): `panel-1280`, `diff-1280` (Diff mit
Kontext, „Diff“ gedrückt), `running-1280` (Punkte-Indikator, „Abbrechen“), `conflict-1280` (Badge 1 am Icon,
„Änderungen übernehmen“), `coding-agents-1280` (Icon-Header wie die anderen Fenster, Titel daneben), `collapsed-1280`,
`panel-900`. Behoben nach Ansicht: doppelter Laufindikator (Kreisel am Knopf entfernt), Diff zeigte „+“ vor „−“,
leere Basis im Labor (Labor veröffentlicht jetzt den echten notes-Code als Basis).

## Runde 2 (15.10.2026, ca. 23:45) – Wächter-Pflicht wandert mit der Funktion

- Senden/Abbrechen der Seitenleiste ist jetzt **ein** Knopf mit der Signatur-Komponente `ctox-run-control`
  (`shared/base.css`, genau einmal in `agent-panel.js`, wie research/outbound/iot/creator): „Senden“ im Leerlauf,
  „Abbrechen“ mit `aria-busy`/`is-running`, solange ein Turn läuft (`data-agent-run="send|cancel"`).
- `scripts/assert-design-system.mjs`: Eintrag `['coding-agents', 'index.html']` ersetzt durch dieselbe Regel („genau
  eine ctox-run-control“) gegen `v4/source/agent-panel.js`; die Liste liest jetzt Pfade relativ zu
  `src/apps/business-os`. Wächter grün („Business OS design-system contract OK“).
- README, `V4-PORT.md`, `V4-HANDLERS.md` der Coding-Agents-App auf die neue Rolle (Übersicht der Läufe) umgeschrieben.
- AG gegen 51b1e33: 9/9; Mutation `cancel` mit dem zusammengelegten Knopf erneut rot (AG-5), Rest unverändert.
  Screenshots `integrated-stub-ag-running-1280.png` („Abbrechen“ im Laufzustand) und `…-panel-900.png` („Senden“)
  angesehen.

## Runde 3 (16.10.2026, ca. 00:30) – Kern-Pin 8489b4e (K-1h: vier Lücken geschlossen)

- **Senden wartet auf die Bestätigung der eigenen Eingaben:** `source-mode.js` sammelt die `result`-Promises der
  `draft.write`-Handles; `flushConfirmed()` schreibt Ausstehendes, wartet darauf und liefert `{rev}` der letzten
  bestätigten Schreibung (`{rejected}` → Zone, kein Befehl). Die Seitenleiste setzt `base_draft_rev = result.rev`
  (Protokoll `revSource: 'write'`). Ohne `result` (alte Kerne) gilt `draft.info().rev`; die stale-Wiederholung bleibt
  nur als Rückfall (`revSource: 'retry'`).
- **Patch übernehmen mit Kontext:** `text-diff.js` liest unified diffs mit Kontextzeilen (je Hunk alte = Kontext +
  entfernte, neue = Kontext + hinzugefügte Zeilen) und wendet sie mit **Kontextprüfung** an: exakt an der erwarteten
  Stelle oder an genau einer anderen (eigene Zeilen davor eingefügt); sonst `PatchConflict` → Zone `agent.patch`
  („Agent-Änderungen passen nicht mehr zu deinem Entwurf …“), Entwurf unverändert, Knopf bleibt. Null-Kontext-Patches
  alter Kerne funktionieren weiter. Einheitstest `v4/source/text-diff.test.mjs` (8 Fälle, u. a. „Nutzer fügt oben Zeilen
  ein“, „Kontext passt nicht“, „mehrdeutig“).
- **AG-Fälle** (Pin-abhängig, Fähigkeit wird im Labor gemessen): 8489b4e **10/10** (AG-3 ohne stale-Ablehnung mit
  `revSource: 'write'`; AG-4 mit zwei oben eingefügten Zeilen, Agent-Zeile landet trotzdem am Ende; neu AG-4c
  Kontextbruch → Zone, Entwurf unverändert; AG-1: Agent ändert jetzt `index.js` statt `NOTES.md`). Alte Pins siehe
  Tabelle unten.

  | Lauf (`AG_ONLY=1 … stub`) | Ergebnis |
  |---|---|
  | Pin 8489b4e | 10/10 |
  | Pin 51b1e33 (Rückfall: info().rev + stale-Wiederholung, Null-Kontext-Patch) | 9/9 |
  | Pin 68b83a3 (ohne §16.44) | 3/3 (AG-6, AG-0, Seitenfehler) |
  | `AG_MUTATE=writerev`, 8489b4e | AG-3 rot, 9/10 |
  | `AG_MUTATE=context`, 8489b4e | AG-4 und AG-4c rot, 8/10 |
  | `AG_MUTATE=stale`, 51b1e33 | AG-3 rot (+ AG-7 Folgezählung), 7/9; gegen 8489b4e 10/10 (Wiederholung dort ungenutzt, wie gewollt) |
- **Mutationen Runde 3:** `writerev` (result.rev nicht verwenden) → AG-3 rot; `context` (Kontextprüfung aus) → AG-4c
  rot (Patch landet stumm); `stale` gegen 51b1e33 weiterhin rot. Ergebnisse siehe unten.
- Quelltext-Labor gegen 68b83a3 und 8489b4e: 13/13. Einheitstests `v4/source/*.test.mjs`: 21/21. Design-Wächter und
  guard-v4 grün.

## Runde 4 (16.10.2026) – Handler der App coding-agents auf CAP-3d (§16.44-Form)

`modules/coding-agents/handlers/ctox.coding.turn.js` (neu gefasst), `ctox.coding.cancel.js` (neu), `_coding_turn.js`
(Prüfung + unavailable-Abbildung), Tests `handlers/tests/coding.test.mjs` 6/6, `v4/os-primitives/core-contract.mjs`
(agent.coding.turn/cancel nach CAP-3d). `check-caps --calls`: ABWEICHUNG 0. Mutationen: 4 von 4 rot (Details und
offene Punkte in `modules/coding-agents/V4-HANDLERS.md`).

## Befunde / Lücken an den Kern bzw. andere Stränge

1. *(Pin 8489b4e: geschlossen – Punkte 1–4.)* **Agent sieht nur die Entwurfs-Überlagerung (Stub 51b1e33).** Der simulierte Agent arbeitet auf `app_drafts.files`
   (nur geänderte Dateien), nicht auf Basis + Entwurf. In einem frischen Entwurf legt er deshalb `NOTES.md` an, auch
   wenn der Auftrag `index.js` nennt (AG-1 im Labor). `draft_turn.rs` übergibt ebenfalls nur `draft.files` an den
   Sidecar. Frage an den Kern: Soll der Sidecar den vollständigen Baum (Basis-Version + Entwurf) bekommen? Sonst
   kann der Agent bestehende Dateien erst ändern, wenn der Nutzer sie im Entwurf berührt hat.
2. **Revision eines frisch angelegten Entwurfs:** `draft.info().rev` liefert nach `rebase` zunächst die Revision der
   früher gelöschten Zeile (veröffentlicht → gelöscht). Die Shell wartet deshalb nach dem Anlegen auf eine neue,
   bestätigte Revision. Vorschlag: `info().rev = null`, solange die lokale Zeile nicht bestätigt ist.
3. **Eigene Eingaben vor dem Befehl:** `draft.write` liefert nur das lokale Promise; die Shell kann nicht auf die
   Bestätigung warten und bekommt beim Senden direkt nach dem Tippen `stale_draft_rev` (ein Wiederholversuch fängt das
   ab). Vorschlag: `draft.write` gibt ein WriteHandle (`result`) zurück.
4. **Null-Kontext-Patch:** reine Einfügungen ohne Kontext lassen sich nach eigenen Zeilen-Einfügungen davor nicht
   eindeutig verorten (wird dann an der alten Zeilennummer eingefügt). Vorschlag: Patch mit 3 Kontextzeilen.
5. ~~Wächter `scripts/assert-design-system.mjs` rot~~ – erledigt in Runde 2 (Pflicht prüft jetzt die Seitenleiste).
6. **`coding-agents.manifest.json` neu erzeugen** (Strang MAN2): Deklaration `coding_turns: read` ist eingetragen,
   `gen-manifest --check` meldet nur Abweichung (keine Fehler); den vollen Schreiblauf (berührt alle Manifeste und
   Berichte) habe ich nicht gefahren. `shell.manifest.json` ist per `--shell-libs` neu erzeugt.
7. **Namensfalle `kernel.shellApi`:** Der Stub-Kernel hat eine Funktion `kernel.shellApi()`. Die erste Fassung reichte
   die App-API als `shellApi` durch; im Quelltext-Labor (Kernel = Stub) brach die Code-Ansicht dann mit
   „api.query is not a function“ (accept.mjs Schritt 2). Jetzt `shellAppApi` + Typprüfung (`query`/`command`);
   Quelltext-Labor wieder 13/13.
8. **Monaco lädt in beiden Laboren nicht** („Fetch API cannot load ctox:lib/monaco@0/vs/loader.js“) – betrifft
   `editor.js`/Bibliotheks-Auflösung (anderer Strang); Rückfall textarea greift, Abnahmen laufen damit.
9. ~~README/Port-Notizen der Coding-Agents-App~~ – erledigt in Runde 2.

## Geänderte / neue Dateien

Neu: `v4/source/agent-panel.js`, `v4/source/coding-port.js`, `v4/source/text-diff.js`, `v4/source/AGENT.md`,
`modules/coding-agents/locales/{de,en}.json` (neu befüllt).
Geändert: `v4/source/source-mode.js` (Editor-Kopf, Diff-Umschalter, „…“-Menü, Seitenleiste, `openChanges`),
`v4/source/source-mode.css` (Kopf, Diff, Seitenleiste, schmal: Leiste unter dem Editor), `v4/frame/index.js` (eine
Zeile: `shellAppApi` an die Code-Ansicht), `v4/frame/lab/frame.test.mjs` (Block `ag()`, `AG_ONLY`, `AG_MUTATE`),
`v4/frame/lab/stub-lab.html` (`__lab.kernelOpts` für `codingDelayMs`), `modules/coding-agents/index.{html,js,css}`,
`modules/coding-agents/tests/{v4-api,coding-agents}.test.mjs`, `modules/coding-agents/v4-manifest.json`,
`v4/tools/manifest/gen-manifest.mjs` (`SHELL_CAPABILITIES`), `v4/manifests/shell.manifest.json` (+ lib-Manifeste
durch `--shell-libs` unverändert neu geschrieben).
