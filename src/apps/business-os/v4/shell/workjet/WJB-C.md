# WJB-C – Jour fixe, Kalender, Sprache (Bericht Strang C)

Stand 16.10.2026 05:30, Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, HEAD ee0ac39fb + Arbeitsbaum).
V2-Quelle: `/Volumes/tmp/app-main.js` (= e35a116ae app.js) und `git show e35a116ae:src/apps/business-os/shared/…`.
Kein git außer Lesen, nichts committet.

## Aktionen (19 im INVENTORY.project.C)

| Aktion | Stand | Orakel (V2-Testdatei → Proben) |
|---|---|---|
| project.jour_fixe.meeting.read | freigeschaltet | workjet-project-control: „meeting read returns null or …“, „meeting read rejects foreign …“ (Kern-Fixture, Strang-C-Hülle) |
| project.jour_fixe.meeting.start | freigeschaltet | workjet-project-control: „meeting Owner controls preserve …“, „… reject caller authority …“, v4-Probe gegen Stub |
| project.jour_fixe.meeting.end | freigeschaltet | „meeting Owner controls preserve …“, „meeting mutation rejects …“ (end-Zweig) |
| project.jour_fixe.todos.revise | freigeschaltet | „meeting Owner controls preserve …“, „meeting mutation rejects …“ (revise-Zweig) |
| project.jour_fixe.todos.confirm | freigeschaltet | „Owner confirmation preserves …“, „… rejects caller authority …“, „… rejects stale or substituted …“ |
| project.jour_fixe.comment.add | freigeschaltet | „meeting Owner controls preserve …“, „slide comment rejects unsafe pins …“, „slide comment receipt must confirm …“ |
| project.jour_fixe.transcript.append | freigeschaltet | „Owner text cannot manufacture …“, „meeting mutation rejects …“, „meeting mutation snapshots nested intent …“ |
| project.jour_fixe.speech | freigeschaltet (E26) | jour-fixe-speech.test.mjs (2/2) + v4-Probe E26 gegen Stub |
| project.jour_fixe.narration.read | **unsupported** – K-WJ-2 | Bereichslesen `files.get({id, range})` ist entschieden, aber nicht im Stub/Vertrag 8489b4e. Orakel jour-fixe-narration.test.mjs daher nicht aufgenommen. v4-Probe: Antwort ist „nicht unterstützt“ in Workjet-Form (Nachricht trifft die Workjet-Regex). |
| project.calendar.accounts.read | freigeschaltet | workjet-project-control: „calendar reads use the authenticated guest …“; workjet-calendar-native.test.mjs (5/5); v4-Probe |
| project.calendar.events.read | freigeschaltet | workjet-calendar-native.test.mjs („events preserve range …“, Fristprobe …) |
| speech.dictation | freigeschaltet (E26) | speech-dictation.test.mjs (7/7) + v4-Probe E26 gegen Stub |
| speech.settings.read | freigeschaltet | speech-settings-control.test.mjs (5/5) |
| speech.settings.configure | freigeschaltet | speech-settings-control.test.mjs (gemeinsamer Rumpf) |
| speech.settings.key | freigeschaltet | „secret input goes directly …“, „missing transport … cannot leak …“ |
| speech.settings.playback | freigeschaltet | speech-settings-control.test.mjs (gemeinsamer Rumpf) + v4-Probe (Vertrag Playback) |
| speech.settings.voices | freigeschaltet | speech-settings-control.test.mjs (gemeinsamer Rumpf) |
| speech.settings.check | freigeschaltet | speech-settings-control.test.mjs (gemeinsamer Rumpf) |
| speech.settings.check.transcription | freigeschaltet | „transcription probes keep scope/correlation …“ (30-s-Frist) |

18 freigeschaltet, 1 unsupported (narration.read, K-WJ-2). Jede freigeschaltete Aktion hat einen Prüfer in
`CHECKERS` (eigene v4-Probe „[Strang C] Vertragsabgleich …“ belegt das und dass nur narration.read offen ist).

Portiert, nicht freigeschaltet (`PREPARED` in actions/meeting.js): `project.jour_fixe.narration.local_publish` und
`project.jour_fixe.transcript.local_candidate`. Beide hängen am selben V2-Rumpf (Besprechungsänderungen), stehen aber
nicht im gemessenen INVENTORY (Workjet ruft sie heute nicht über die Brücke). Ihre Orakel („local final …“ 4,
„local narration …“ 2) sind deshalb nicht eingetragen; Freischalten ist eine Zeile, falls das INVENTORY sie aufnimmt.

## Orakel vorher / nachher

- Vorher (WJB-0-Stand, mein erster Lauf 05:05): ganzes Glob 51/51; Strang C: 0 Orakel.
- Nachher Strang C: **32 V2-Proben** gegen v4 (11 aus workjet-project-control über `ORACLE_TESTS`, 2 meeting-read
  + 7 Diktat + 5 Spracheinstellungen + 2 Jour-fixe-Sprache + 5 Kalender in der Strang-C-Hülle), dazu 5 Zähl-Proben
  (Anzahl je Datei) und 5 v4-Proben gegen den Kern-Stub (Vertragsabgleich, E26 Diktat, E26 Jour-fixe-Sprache,
  fehlende Kernroute → unsupported, Kalender/Einstellungen/Besprechung über kernel.command + Vertrag).

## E26 (K-WJ-3) – Umsetzung und Annahmen

Umsetzung in `actions/meeting.js` `sessionSync()` / `openNativeSession()`, nur wenn die Umgebung eine Kernel-App-API
hat (`env.api`, Produktion über v2-compat). Ohne App-API (V2-Orakel) sieht der wörtliche V2-Rumpf genau die
V2-Aufrufe; dadurch prüfen die V2-Orakel unverändert die Parameter und Fristen.

- `open`: `api.command(method, {…params})`; Antwort = erster `running`-Zwischenstand mit `result` (oder der Endstand,
  falls der Befehl sofort endet, z. B. `state:'failed', error:'missing_credential'`). Die V2-Frist (20 s) gilt bis zu
  diesem ersten Zwischenstand, nicht für die Sitzung. Die command_id des open-Befehls ist die Sitzung.
- `write`, `finish`, `cancel`: eigener Befehl über `env.state.sync.requestNative` (v2-compat, wartet terminal,
  V2-Frist 5 s) mit `{…params, session_command_id}`. Nach finish/cancel wird die Sitzung aus dem Register entfernt.
- `read`: kein Befehl. Antwort aus dem letzten Zwischenstand des open-Befehls (bzw. dessen Endstand).
- Annahmen (bitte vom Kern bestätigen):
  - A1: Der erste `running`-Zwischenstand des open-Befehls hat die Form der V2-Nativantwort auf `open` und trägt
    `streamId`. Ohne `streamId` antwortet die Brücke trotzdem, legt aber keine Sitzung an (folgende Operationen →
    `SPEECH_STREAM_CLOSED`).
  - A2: `write`/`finish`/`cancel` antworten als normale Befehlsvollendung (`done` + result in V2-Form); das finale
    Diktat (`state:'finished'`, `text`) kommt im Ergebnis des finish-Befehls.
  - A3: Jeder Zwischenstand hat die Form der V2-Nativantwort derselben Methode, `events` aufsteigend nach `sequence`.
    `read` setzt `commandId`/`op:'read'`/`streamId` aus der Anfrage (Diktat: zusätzlich `action`), gibt nur Ereignisse
    mit `sequence > afterSequence` zurück (Diktat höchstens 1 = neuester Schnappschuss, Jour fixe höchstens 32) und
    lässt bei Jour fixe `requestId` weg (Workjet verlangt es nur bei open).
  - A4: Sitzungsregister je App-API, höchstens 16 offene Sitzungen (älteste wird verdrängt). Nach Shell-Tausch ist
    das Register leer: `write/read/finish/cancel` auf eine unbekannte Sitzung → Fehler `SPEECH_STREAM_CLOSED`
    (Workjet: „fehlgeschlagen“, die Aufnahme muss neu geöffnet werden).
  - A5: Ein vom Kern abgelehnter oder unbekannter open-Befehl (`unknown_command`) → „nicht unterstützt“.
- Die Orakel lassen sich so erfüllen; keine V2-Probe musste ausgelassen werden.

## Abweichungen vom wörtlichen V2-Rumpf (mit Grund)

- `requestSpeechSettings`: im catch `if (isUnsupported(error)) throw error;` vor `speechSettingsFailure`. Sonst würde
  eine fehlende Kernroute zu `SPEECH_SETTINGS_FAILED` („fehlgeschlagen“) statt „nicht unterstützt“. Die Nachricht des
  unsupported-Fehlers enthält nur den Methodennamen, nie Eingaben (v4-Probe prüft, dass das Geheimnis nicht durchsickert).
- meeting.read: aus dem gemeinsamen V2-Block (KPI + meeting.read, app.js:14242-14338) nur der meetingRead-Zweig mit
  ausgewerteten Konstanten (KPI = Strang A).
- `requireWorkjetSupervisorDataPlane` (app.js:15002-15008) als lokale Funktion.

## Befunde für den Kern

- **K-WJ-6 (Stub, gleich K-WJ-B1 von Strang B)**: der Stub legt das Handler-Ergebnis per JSON-Merge-Patch ab
  (`mergePatch`, null = Feld löschen). Explizite null-Felder in Befehlsergebnissen verschwinden (`text:null`,
  `error:null`, `meeting:null`, `receipt:null`). Die V2-Verträge verlangen diese null-Felder; mit einem Kern, der so
  speichert, scheitern meeting.read (meeting null), Diktat (text/error null) und Jour-fixe-Sprache als
  „fehlgeschlagen“. Meine Stub-Proben umgehen das nur im Test (Ergebnis vorab per `sim.push` schreiben, das ersetzt
  das Dokument). Bitte im Kern: `result` als Wert ersetzen, nicht als Merge-Patch mischen.
- **K-WJ-2**: narration.read bleibt unsupported, bis `files.get({id, range:{start,end}})` im Stub/Vertrag steht.
  Der V2-Rumpf (app.js:13952-14004 + shared/jour-fixe-narration.mjs) ist noch nicht portiert; er liest zusätzlich
  `desktop_files` über startCollection (Export an die Shell nötig, K-WJ-1).
- **E26 Annahmen A1–A5** (oben) bestätigen.
- `requiredCapability` (V2: ctox-workjet-speech-dictation-v1 usw.) reicht die Brücke nicht weiter (wie v2-compat).
  Prüft der Kern die Fähigkeit selbst, ist das gleichwertig.

## Gehört nach v2-helpers / v2-compat

- `requireWorkjetSupervisorDataPlane(env)` (Strang B braucht dieselbe Hilfe).
- `openNativeSession` / `sessionSync` (E26-Sitzungen über einem Kernbefehl) gehören in v2-compat neben
  `requestNative`; dort müsste `requestNative` die Handle-ID des Befehls preisgeben (heute nicht), dann entfiele der
  direkte `api.command`-Aufruf in meeting.js.
- `unsupportedError`/`isUnsupported` werden aus v2-compat importiert (keine Kopie).

## Brücken/Globals

Alle C-Aktionen laufen in V2 über `globalThis.workjetProjectControl` (app.js:13918-14045); es gibt keine eigene
Sprach-Global. Daher nur additiv in `index.js`: Import `MEETING_ACTIONS` und `...MEETING_ACTIONS` in
`ACTIONS.project` (Block „Strang C“). Kein neues Global.

## Geänderte / neue Dateien

- neu: `v4/shell/workjet/actions/meeting.js`, `v4/shell/workjet/WJB-C.md`
- additiv (Block „Strang C“): `v4/shell/workjet/index.js` (2 Zeilen), `v4/shell/workjet/tests/oracle.test.mjs`
  (Namensliste vor der Schleife + Strang-C-Hülle mit v4-Proben am Ende), `v4/shell/workjet/tests/contract-checkers.mjs`
  (Einträge in `CHECKERS` + Prüferfunktionen am Dateiende).

## Abnahme (05:30, aus `src/apps/business-os/`)

```
node --test 'v4/shell/workjet/tests/*.test.mjs'   → tests 251 · pass 248 · fail 0 · todo 3 (todo = Strang B, K-WJ-B1), 2,1 s, Exit 0
node v4/os-primitives/check-caps.mjs --calls      → calls: 202 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 3 · OK 195 · OHNE_VERTRAG 4
node v4/debt/ratchet.mjs                          → ratchet: guard 1838 · scan 3 · ratchet: OK
node --test v4/tools/guards/guard-v4.test.mjs     → tests 74 · pass 74 · fail 0
```
guard-v4 auf actions/meeting.js: 0 Treffer (V2-Namen mit `// v4-allow: debt L-42`, V2-Fehlertexte mit
`// v4-allow: code`). Zwischenstand: ein früherer Ratchet-Lauf (05:27) zeigte en-unavailable +1. Ursache waren die
damals noch unmarkierten Fehlertexte in meeting.js; danach lag der Zuwachs in `host-bridge.js:313` (Strang D). Der
Schlusslauf oben ist OK.
Hinweis: zwischendurch hingen ganze Läufe von oracle.test.mjs (offene Stub-Kernel in damals roten Strang-B-Proben).
Der Schlusslauf endet sauber in 2,1 s.
