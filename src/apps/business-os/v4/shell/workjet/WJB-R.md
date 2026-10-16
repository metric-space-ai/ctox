# WJB-R – Bereichslesen, Präsentation/Narration freigeschaltet, idempotency_key (Bericht Strang R)

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, HEAD 81a0f9c8a + Arbeitsbaum, nichts
committet, kein git außer Lesen). V2-Quelle: `/Volumes/tmp/app-main.js` (= app.js @ e35a116ae) und
`git show e35a116ae:…`. Workjet-Verträge: `/Volumes/tmp/wj-measure/packages/contracts/src` (eb65947d).
Kern: E26 (`files.get({id|ref, range:{start, end}})`, `command(…, {idempotency_key})`), Stub-Pins 637caf6 (Standard)
und 53402aa (Nachtrag Orchestrator, enthält 637caf6 + E26-Fixture-Befehle).

## Aktionen

| Aktion | Stand | Orakel / Proben |
|---|---|---|
| project.presentation.read | **freigeschaltet** (PREPARED → PROJECT_ACTIONS) | V2: jour-fixe-presentation.test.mjs „read and save payloads follow the shared wire contract“ (+ 4 Modulproben); v4-Probe „presentation.read über kernel.command“ (Manifest und null, Vertrag + Korrelation); Prüfer `presentationReadResponseR` |
| project.presentation.content.read | **freigeschaltet** | V2: jour-fixe-presentation.test.mjs „returns one bounded range …“, „a newer revision or a different file …“, „ranges above the Workjet response budget …“; v4-Proben „content.read: echte Datei …“ und „Gegenprobe: Metadaten größer als die Kerndatei“; Prüfer `presentationContentResponseR` |
| project.presentation.canvas.save | **freigeschaltet** | V2: jour-fixe-presentation.test.mjs „a save receipt must answer exactly …“, „read and save payloads …“; v4-Probe „canvas.save: V2-Befehl ctox.workjet.presentation.canvas.save …“ (Payload, record_id, falsche Revision, fehlende Route); Prüfer `presentationCanvasSaveResponseR` |
| project.jour_fixe.narration.read | **freigeschaltet** (meeting.js, Rumpf wörtlich app.js:13952-14004) | V2: jour-fixe-narration.test.mjs, alle 12 Proben, davon „Shell narration read uses native meeting receipts, fresh file metadata and demand bytes“ als Hüllenprobe gegen v4 `narrationRead`; v4-Proben „narration.read: Besprechung über kernel.command …“ und „Gegenproben: end > Größe; fehlende Kernroute“; Prüfer `narrationReadResponseR` |

`canvas.save` sendet wie V2 `ctox.workjet.presentation.canvas.save` (app.js:15541-15542) mit
`record_id = projectId` und Payload `{operation_id, presentation_id, expected_revision, slide_id, scene_json}`.
In der Probe ist der Befehl per `k.sim.onCommand` registriert, die gespeicherte Befehlszeile ist geprüft
(`type`, `record_id`).

Gemeinsame Probe ohne `files.get` (Kernel-Attrappe ohne files): content.read und narration.read melden
„nicht unterstützt“ im Workjet-Merkmal, nicht „fehlgeschlagen“.

## Umsetzung

**Bereichslesen (v2-compat.js).** `startCollection(name)` gibt dem Peer zusätzlich `demandFileLoader`
mit. Das ist ein eigenes Feld, `collection.demandLoader` bleibt die Generationsmarke.
`fetchFile(fileId, {range})` ruft `api.files.get({id, range:{start: offset, end: offset + length}})` auf (`end` exklusiv) und liefert die
V2-Chunkliste `[{sequence: 0, bytesBase64, hash: sha256Hex(bytes)}]`. Diese Form verlangen
`readStoredFileFromDemandChunks` (shared/file-integrity.js:40-77) und die Bereichsprüfungen in
jour-fixe-presentation.mjs:151-155 bzw. jour-fixe-narration.mjs:74-78. Ohne `range` liest es die ganze Datei,
Text wird als UTF-8 kodiert.
- Unbekannte Datei (`found:false`): leere Liste. Der V2-Leser meldet dann „Dateiinhalt fehlt.“ (`ctox_file_chunk_missing`).
- Kernfehler wie `invalid`/`path range` oder `forbidden` gehen unverändert durch.
- Hat der Kernel kein `files.get`, gibt es keinen Loader (`gaps` bekommt `files`). `readRange` meldet dann
  „nicht unterstützt“. Das ist dieselbe Abweichung, die Strang A in project.js schon hatte; in meeting.js habe ich
  sie übernommen. Der V2-Text „Narration file transport is …“ entfällt dadurch. Das hält auch den Wächter sauber
  (vocab en-unavailable).

**Rumpf narration.read (meeting.js).** Der Rumpf ist wörtlich V2 mit `// ref: app.js:13952-14004`, mit diesen
Ersetzungen: `state` → `env.state`, `Date.now()` → `env.now()`, `crypto.randomUUID()` → `env.newId()`.
Den rekursiven V2-Aufruf `workjetProjectControl({action:'project.jour_fixe.meeting.read', …})` habe ich durch
`meetingRead(env, …)` ersetzt. Das ist derselbe Handler, der Router davor prüft nur die Aktion.
Die Module `shared/jour-fixe-narration.mjs`, `jour-fixe-presentation.mjs` und `file-integrity.js` sind
unverändert. Ihre Byte-Gleichheit zum Pin prüft eine Probe.

**idempotency_key (v2-compat.js dispatch).** `command.command_id ?? command.id` wird als `opts.idempotency_key`
mitgegeben, aber nur als Text mit 1–128 Zeichen.
- Begründung: In V2 war die commandId die Dokument-ID in `business_commands`. Dieselbe ID bedeutete schon dort
  dieselbe Operation.
- `requestNative` bekommt **keinen** Schlüssel. Eine Probe belegt das: zwei gleiche Nativaufrufe ergeben zwei Befehle.

**Sitzungsbefehle, Entscheidung: kein Schlüssel.** `openNativeSession` in meeting.js ruft `api.command(method, params, {})`
direkt auf (Sprachsitzung `open`). `write`/`finish`/`cancel` laufen über `sync.requestNative` mit
`session_command_id`, also ebenfalls ohne Schlüssel. Ich habe nichts geändert, aus diesen Gründen:
1. Der V2-Rumpf (shared/speech-dictation.mjs, jour-fixe-speech.mjs) geht über `requestNative`. Das war in V2 je
   Aufruf und entdoppelte nie.
2. `project.jour_fixe.speech` hat keine ID je Operation, nur eine `requestId` beim `open`.
3. `speech.dictation` trägt zwar eine `commandId` je Operation. Workjet vergibt sie aber für jede Operation neu
   (`apps/web/src/lib/composerDictation.ts:86/103/130/151/171`, `newCommandId()`). Ein Schlüssel würde also nie
   entdoppeln, sondern nur ein Risiko einführen: eine wiederverwendete ID würde Audio-Schreibvorgänge verschlucken.
4. Die Sprachaktionen gehören nicht zu den Aktionen dieses Strangs.

## Orakel-Anzahl

- Vorher für die vier Aktionen: 0 V2-Proben, 3 Proben, die „nicht unterstützt“ erwarteten.
- Nachher: 17 V2-Proben (presentation 5, narration 12), 2 Zählproben, 1 Byte-Gleichheitsprobe und 10 v4-Proben gegen
  den Stub, darunter die idempotency- und requestNative-Proben.
- Gesamtlauf: 263 → 293 Tests.

## Gegenproben (Fehler absichtlich eingebaut, Lauf, Rücksetzen, `cmp` rc=0 je Datei)

| Mutation | Datei | rot |
|---|---|---|
| M1 `end: offset + length + 1` | v2-compat.js | 3 (fetchFile, content.read, narration.read) |
| M2 presentation.read antwortet `meetingId = projectId` | actions/project.js | 1 (presentation.read, Korrelation) |
| M3 canvas.save `record_id = meetingId` | actions/project.js | 1 (canvas.save) |
| M4 narration meeting.read ohne `meetingId` | actions/meeting.js | 1 (narration.read) |
| M5 `idempotency_key` entfernt | v2-compat.js | 1 (idempotency: Handler 2×, zwei kernel_command_id) |
| M6 Chunk-hash über falsche Bytes | v2-compat.js | 3 (fetchFile, content.read, narration.read) |
| M7 narration `requireRevision` konstant | actions/meeting.js | 1 (V2-Orakel „Shell narration read …“, frische Tokens) |

Nach jeder Mutation habe ich die Datei aus der Sicherung zurückgesetzt. `cmp` gegen die Sicherung ergab jedes Mal
rc=0 (Skript `scratchpad/wjb-r/mut.py`, `mut7.py`).

## Abnahme (selbst gelaufen, aus src/apps/business-os)

```
Stub 637caf6 (Standard):
node --test 'v4/shell/workjet/tests/*.test.mjs'          → tests 293 · pass 293 · fail 0 · todo 0 (Exit 0, ~2,7 s, Prozess endet)
node v4/shell/workjet/tests/measure-command-types.mjs    → Exit 0; Aktionen: 76 (erwartet 76), gemessen 76, statisch 0;
                                                           Unterschiedliche Typen: 58; grep-Gegenprobe: 49 Literale, unbegründet 0
Stub 53402aa (CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/53402aa):
node --test 'v4/shell/workjet/tests/*.test.mjs'          → tests 293 · pass 293 · fail 0 · todo 0 (Exit 0)
node v4/shell/workjet/tests/measure-command-types.mjs    → Exit 0; 76/76 gemessen; Unterschiedliche Typen: 58
Unabhängig vom Stub:
node v4/os-primitives/check-caps.mjs --calls             → calls 202 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 3 · OK 195 · OHNE_VERTRAG 4
node v4/debt/ratchet.mjs                                 → ratchet: guard 1838 · scan 3 · ratchet: OK
node --test v4/tools/guards/guard-v4.test.mjs            → tests 74 · pass 74 · fail 0
```
Die Ausgabe `~/.local/state/shell-v4/analysis/wjb-command-types.{json,txt}` stammt aus dem letzten Lauf mit dem
Standard-Pin 637caf6.
- **58 unterschiedliche Befehlstypen.** Neu sind `ctox.workjet.presentation.read` und
  `ctox.workjet.presentation.canvas.save`. narration.read nutzt das schon gezählte
  `ctox.workjet.jour_fixe.meeting.read`.
- Die Bytes laufen über `files.get`, das ist kein Befehl.
- Vorher (72 Aktionen) waren es damit 56 Typen. Diesen Wert habe ich nur abgeleitet (58 minus die zwei neuen), den
  alten Lauf habe ich nicht wiederholt.

Erste Zwischenläufe dauerten 300 s, weil rote Proben Stub-Kernel offen hielten. Ich habe deshalb mit
`--test-force-exit` diagnostiziert. Die Abnahmeläufe oben sind ohne dieses Flag gelaufen und enden sauber.

## Geänderte Dateien

- `v4/shell/workjet/v2-compat.js`: Import `sha256Hex` aus shared/file-integrity.js, `demandFileLoader` im
  `startCollection`-Peer, `idempotency_key` in `dispatch`, Kommentar an `requestNative`.
- `v4/shell/workjet/actions/project.js`: presentation.* in `PROJECT_ACTIONS`. `PROJECT_GAPS` und `PREPARED` sind
  jetzt leer. Nur Kommentare geändert, der Rumpf ist unverändert.
- `v4/shell/workjet/actions/meeting.js`: `narrationRead` portiert und freigeschaltet, aus `PREPARED` entfernt, zwei Importe ergänzt.
- `v4/shell/workjet/tests/contract-checkers.mjs`: Block „Strang R“ mit 4 Prüfern in CHECKERS, Prüferfunktionen und
  `correlateR` (Anfrage↔Antwort wie `isWorkjet…ReceiptForRequest`) am Dateiende.
- `v4/shell/workjet/tests/oracle.test.mjs`: Block „Strang R“ am Dateiende.
- `v4/shell/workjet/tests/measure-command-types.mjs`: erwartete Zahl 72 → 76, SITES/REQUESTS/Antworten für die vier
  Aktionen, die zwei EXCLUDED-Einträge für presentation-Literale entfernt.
- `v4/shell/workjet/tests/bridge.test.mjs`: siehe Abweichungen.
- `v4/shell/workjet/WJB-R.md` (dieser Bericht).

**Abweichungen: bestehende Proben anderer Stränge angepasst, jede Zeile mit `WJB-R` markiert.** Grund ist die
verlangte Semantik: dieselbe commandId ist jetzt dieselbe Operation, und die vier Aktionen sind freigeschaltet. Jede
Änderung ist minimal, die Absicht der Probe bleibt erhalten.
- bridge.test.mjs, „unbekannte und nicht portierte Aktionen …“: Als Beispiel steht jetzt
  `project.jour_fixe.narration.local_publish` statt `project.presentation.read`.
- bridge.test.mjs, „project.worker.add …“: Der zweite Aufruf nach `failCommand` bekommt eine neue commandId.
- oracle.test.mjs, Strang B „turn.submit|watch|…“: Jede Aktion bekommt eine eigene commandId (vorher fünfmal `c-1`).
- oracle.test.mjs, Strang C „Vertragsabgleich“: Die Erwartung ist jetzt „keine offene C-Aktion“ statt narration.read.
- oracle.test.mjs, Strang C „fehlende Kernroute …“: Die narration-unsupported-Zeile ist entfernt. Ersatz ist die
  Strang-R-Probe „fehlende Kernroute → nicht unterstützt“.
- oracle.test.mjs, Strang A „fehlende Kernroute …“: presentation.read meldet ohne Kernroute „nicht unterstützt“.
  Die Erwartung „bleiben unsupported“ ist entfernt.
- oracle.test.mjs, Strang A „project.init: fremde Felder …“: Jede Antwortvariante bekommt eine eigene commandId.
- oracle.test.mjs, Strang D „instance.providers.*“: Je Operation eine eigene operationId (`OP_2..OP_4`), weil die
  V2-Befehls-ID dort die operationId ist.

**Harness-Befund:** `oracle-vm-shim.mjs` liest `globalThis.__WJB_ORACLE.factory` erst zur Laufzeit. Die zuletzt
gesetzte Fabrik gilt also für alle Orakel. Meine erste Fassung hat die Fabrik von Strang D überschrieben, dadurch
wurden 12 D-Orakel rot. Der Strang-R-Block übernimmt jetzt die vorige Fabrik und ergänzt nur `project`.
Gehört bei Gelegenheit in v2-oracle.mjs: eine Fabrik je Orakeldatei statt eines Globals.

## Gehört nach v2-helpers

Nichts Neues. narration.read nutzt `awaitWorkjetProjectListStep` und `readWorkjetProjectListRows` aus v2-helpers.
`requireWorkjetSupervisorDataPlane` gibt es weiterhin doppelt (project.js und meeting.js), das hat Strang A
schon gemeldet. `toBase64` in v2-compat.js entspricht `encode` aus den shared-Modulen; es ist lokal, weil die
Module es nicht exportieren.

## Offene Fragen an den Kern

1. **Dateizugriff über App-Grenzen (neu, wichtig).** Im Stub liefert `files.get` nur Dateien der eigenen App
   (CAP-3b, `ownFile` → `forbidden` bei fremder `owner_app`). Die Brücke schreibt und liest als App `shell`.
   Die Präsentations- und Narrationsdateien legt in Produktion der Kern bzw. die Desktop-App an
   (`desktop_files.owner_id`, `linked_collection`). Die Proben funktionieren, weil die Datei mit der Shell-App-API
   angelegt wird.
   - Frage 1: Darf die Shell eine `desktop_files`-Datei per `files.get({id})` lesen?
   - Frage 2: Ist `desktop_files.id` dieselbe ID wie die des Kern-Dateidatensatzes?
   - Wenn nicht, meldet content.read/narration.read im echten Kern `forbidden`, also „fehlgeschlagen“. Abhilfe wäre
     eine Lesefreigabe für verknüpfte Dateien oder eine Abbildung von desktop_files auf die Datei-ID bzw. den ref.
2. **Export an die Shell.** Laut Brief exportiert das desktop-Manifest `desktop_files` und `desktop_file_chunks` mit
   `read`. Den echten Kern habe ich dazu nicht geprüft; der Stub prüft keine exports (wie bei K-WJ-1).
3. **Idempotenz nach Fehlschlag.** Mit `idempotency_key` liefert eine Wiederholung mit derselben commandId nach einem
   `failed` wieder den alten Fehlschlag, ohne neuen Versuch. Das entspricht V2, dort war die Dokument-ID gleich.
   Workjet vergibt für einen echten Neuversuch eine neue commandId (composerDictation, newCommandId). Zu bestätigen
   ist, dass Workjet nirgends „gleiche ID = neuer Versuch“ erwartet. Bekannt sind mir nur die Sprach- und
   Projektpfade.
4. **Kalte Bereichsabrufe im nativen Kern** fehlen laut Orchestrator noch (#885). Gegen den Stub ist das ohne Wirkung;
   die Probe „fetchFile …“ läuft im Stub warm.
5. **Shell-Manifest:** v2-compat.js importiert jetzt `shared/file-integrity.js`, meeting.js importiert
   `shared/jour-fixe-narration.mjs`. Das SW-Artefakt braucht beide in `shell.manifest.json`
   (gen-manifest, nicht in meinem Whitelist).
