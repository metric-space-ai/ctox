# WJB-A – Projekt, KPI, Exit, Präsentation (Bericht Strang A)

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, Basis ee0ac39fb + Arbeitsbaum, nichts
committet). V2-Quelle: `/Volumes/tmp/app-main.js` (= app.js @ e35a116ae), Orakel per `git show e35a116ae:…`.
Workjet-Verträge: `/Volumes/tmp/wj-measure/packages/contracts/src` (eb65947d).
Orchestrator-Entscheide berücksichtigt: E26 (K-WJ-2 kommt als `files.get({id, range:{start, end}})`, noch nicht im
Stub → presentation.* bleibt unsupported), K-WJ-1 freigegeben (Manifest-Export `workjet_working_copies` macht der
Orchestrator).

## Aktionen

| Aktion | Stand | Orakel / Grund |
|---|---|---|
| project.list | unverändert (WJB-0) | 26 V2-Proben workjet-project-control |
| project.worker.add | unverändert (WJB-0) | 16 V2-Proben workjet-project-control |
| project.create | **freigeschaltet** | workjet-project-control „create/list is idempotent …“; workjet-projection-readiness (4 Proben project/working copy × awaitInSync/awaitInitialReplication); v4-Probe gegen Stub |
| project.configure | **freigeschaltet** | workjet-project-control: 11 Proben (Policy-CAS, Luma, Metadaten, null vs. fehlend, Alias, Fälschung, Quittungen) + 3 Korpus-Proben (Policy-, Luma-, Konfigurations-Fixture des Kerns); v4-Probe |
| project.chat.create | **freigeschaltet** (PREPARED → Tabelle) | „project.chat.create forwards native mutation …“, „chat creation rejects a receipt for another title“, „validates chat request fields before dispatch“ |
| project.kpis.read | **freigeschaltet** | „native KPI control reads, configures and clears …“, „native project detail control rejects forged …“, „native KPI receipts reject …“; v4-Probe |
| project.kpis.configure | **freigeschaltet** | dieselben drei Proben; v4-Probe |
| project.kpis.refresh | **freigeschaltet** | „native KPI refresh maps per-KPI status …“; v4-Probe |
| project.exit_model.read | **freigeschaltet** | workjet-exit-model-control (4 Proben); v4-Probe (Hülle); voller Vertrag: todo K-WJ-6 |
| project.exit_model.refresh | **freigeschaltet** | dto. |
| project.exit_model.submit | **freigeschaltet** | dto. |
| project.init | **freigeschaltet** (Orchestrator-Entscheid, Nachtrag) | V2 hat nur eine Quelltext-Regex-Probe (Testdatei Z. 2039); daher 3 eigene v4-Proben gegen den Stub, Erwartung wörtlich aus app.js:14647-14669, 15002-15008, 13906-13915 (Abschnitt „Nachtrag project.init“) |
| project.presentation.read | **unsupported, Kern-Lücke K-WJ-2** (Rumpf in PREPARED) | E26 |
| project.presentation.content.read | **unsupported, Kern-Lücke K-WJ-2** (Rumpf in PREPARED) | E26 |
| project.presentation.canvas.save | **unsupported, Kern-Lücke K-WJ-2** (Rumpf in PREPARED) | E26 (Orchestrator: presentation.* gemeinsam) |

Hinweis KPI: „native project detail control rejects forged …“ prüft am Ende auch `project.jour_fixe.meeting.read`
(Strang C). Bis C freischaltet, besteht dieser Teil über „nicht unterstützt“ (0 Befehle) – inhaltlich gilt die Probe
dann für C mit.

## Nachtrag project.init (Orchestrator-Entscheid: freischalten)

`project.init` steht jetzt in `PROJECT_ACTIONS` und hat einen Prüfer in CHECKERS (ref ctox.ts:1085-1092 Schritt,
:1182-1188 Antwort: action, commandId, projectId, operationId ≤ 128, steps ≤ 12 × {id ≤ 64, state ≤ 32, detail ≤ 256}).
Eigene v4-Proben in `tests/oracle.test.mjs`, Block „Strang A (v4-Proben)“, gegen den Kern-Stub; die Erwartung steht
wörtlich im V2-Rumpf:

1. *Befehl, Payload, Antwortform, Schrittkappung* (app.js:14647-14669): Befehlstyp `ctox.workjet.project.init`,
   `record_id` = projectId (im gespeicherten business_commands-Dokument geprüft), Payload genau
   `{project_id, operation_id}`, Antwortschlüssel genau `action, commandId, projectId, operationId, steps`, 13 Schritte
   → 12, `detail` auf 256 gekürzt, `String()` auf id/state (Zahl 7 → '7', fehlendes detail → ''), Workjet-Vertrag.
2. *Ablehnungen*: fremde Felder (`ownerUserId`, `steps`) → „Unsupported Workjet project payload field: …“ und leere
   operationId → „Invalid Workjet project operationId.“, jeweils ohne Befehl; fremde project_id, result.ok false,
   fehlende oder Nicht-Array-steps und Kernfehler (`forbidden: policy`) → „Project init returned an uncorrelated or
   unsuccessful response.“; `unknown_command` → „nicht unterstützt“ im Workjet-Merkmal; ohne commandBus
   (app.js:15002-15008) → „Workjet supervisor control is not ready.“ ohne Befehl.
3. *Sitzungszaun* (app.js:14660): Sitzungswechsel während der Quittung → „uncorrelated …“.

Gegenprobe (je einzeln eingebaut, geprüft, zurückgesetzt, Datei danach per `cmp` identisch):
M1 `slice(0, 13)` → Probe 1 rot; M2 Payload `operationId` statt `operation_id` → Probe 1 rot; M3 Sitzungszaun entfernt
→ Probe 3 rot; M4 Datenebene-Fehlertext geändert → Probe 2 rot.

## Einsatzstelle Bereichslesen (E26 / K-WJ-2)

`actions/project.js`, Funktion `projectPresentation(action)` → `readJourFixePresentationContent(request, { … readRange })`:

```js
readRange: (fileId, range) => {
  const loader = bridge?.state?.demandFileLoader;
  if (!loader?.fetchFile) throw unsupportedError('project', action, 'K-WJ-2: files.get ohne range');
  return awaitWorkjetProjectListStep(loader.fetchFile(fileId, { range }), deadline, 'presentation bytes', now);
},
```

`range` ist dort `{ offset, length }` (aus `shared/jour-fixe-presentation.mjs`, length ≤ 128 KiB). Der Rumpf bleibt
wörtlich V2; die v4-Abbildung gehört in **`v2-compat.js` → `startCollection` → `peer`** (Orchestrator-Datei):
`peer.demandFileLoader = { fetchFile(fileId, { range }) → api.files.get({ id: fileId, range: { start: range.offset,
end: range.offset + range.length } }) }`, Rückgabe als V2-Chunkliste `[{ sequence: 0, bytesBase64, hash: sha256Hex(bytes) }]`
(`readStoredFileFromDemandChunks` in `shared/file-integrity.js:40-77` prüft je Chunk sha256 über die dekodierten
Bytes und verlangt lückenlose `sequence` ab 0).
Achtung: `peer.collection.demandLoader` ist schon belegt (Generationsprüfung); `demandFileLoader` ist ein eigenes
Feld. Danach wandern die drei Aktionen nur von `PREPARED` nach `PROJECT_ACTIONS`, plus Orakel
`jour-fixe-presentation.test.mjs` (das Modul ist byte-gleich zum Pin, Probe unten) und Prüfer in CHECKERS.
Zusätzlich braucht `readMetadata` den Export von `desktop_files` an die Shell (wie K-WJ-1).

## Orakel-Anzahl

- vorher (für Strang-A-Aktionen außer list/worker.add): 0
- nachher: 30 V2-Proben gegen v4 und 3 eigene v4-Proben für project.init (Nachtrag) (19 workjet-project-control über die gemeinsame Hülle, 3 Korpus-Proben und
  4 Readiness-Proben über die Strang-A-Hülle, 4 workjet-exit-model-control) + 1 Byte-Gleichheitsprobe
  (jour-fixe-presentation.mjs, file-integrity.js = Pin) + 3 v4-Proben gegen den Kern-Stub + 1 todo (K-WJ-6).
- Gegenprobe: drei absichtliche Fehler in project.js (Exit-Fehlertext, working_copy-Befehlstyp, Ressourcen-Feldname)
  → 4 Orakel rot; danach zurückgesetzt.

Readiness-Hülle: `tests/oracle.test.mjs`, Block „Strang A“ am Dateiende. app.js enthält den **Quelltext** der
v4-Warter aus `actions/project.js` (`PROJECTION_WAIT_SOURCES`), einzige Umformung: Parameter `env` entfällt, eine
Zeile bindet env an den vm-Kontext. Die Rechner-/Sitzungs-Warter (Strang D) stehen als V2-Text daneben, weil der Test
beim Laden alle fünf verlangt; ihre Proben sind nicht freigegeben. Die drei Warter in project.js deshalb nicht
umformatieren (Signatur auf einer Zeile, `}` in Spalte 0).

## Abnahme (selbst gelaufen, aus src/apps/business-os)

```
Letzter Lauf (nach dem Nachtrag project.init):
node --test 'v4/shell/workjet/tests/*.test.mjs'   → tests 254 · pass 251 · fail 0 · todo 3 (EXIT 0)
    todo: K-WJ-6 (Strang A), K-WJ-B1 und K-WJ-B2 (Strang B)
node v4/os-primitives/check-caps.mjs --calls      → calls 202 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 3 · OK 195 · OHNE_VERTRAG 4
node v4/debt/ratchet.mjs                          → ratchet: guard 1838 · scan 3 · ratchet: OK
    guard-v4 in v4/shell/workjet: 0 Treffer (Strang A: 0; „Secure Workjet command IDs are unavailable.“ trägt
    `// v4-allow: code`). Ein früherer Lauf zeigte FEHLER durch 4 Treffer in meeting.js (Strang C), inzwischen behoben.
node --test v4/tools/guards/guard-v4.test.mjs     → tests 74 · pass 74 · fail 0
Hinweis Orchestrator (v2-compat dispatch `ok` wie V2 commandReceipt): Strang A hatte keine lokale Angleichung;
alle Strang-A-Aktionen warten mit until:'terminal'.
```
Während der Läufe editierten B und C dieselben Testdateien; zwei frühere Läufe hingen nach dem letzten Test
(offene Stub-Kernel in damals roten fremden Proben) und wurden per PID beendet; der Wiederholungslauf endete sauber
(1,5 s für oracle.test.mjs). Rahmen-Labor nicht gelaufen (Brief: läuft in anderem Strang).

## Geänderte Dateien

- `v4/shell/workjet/actions/project.js` (Strang A): Handler create, configure, chat.create (Tabelle),
  kpis.*, exit_model.*, init (seit Nachtrag in PROJECT_ACTIONS), presentation.* (PREPARED); lokale V2-Hilfen; `PROJECT_GAPS`, `PROJECTION_WAIT_SOURCES`.
  project.list und project.worker.add unverändert.
- `v4/shell/workjet/tests/oracle.test.mjs`: Block „Strang A“ vor der Schleife (Testnamen), Blöcke „Strang A“ und
  „Strang A (v4-Proben)“ am Dateiende.
- `v4/shell/workjet/tests/contract-checkers.mjs`: 10 Prüfer (inkl. project.init) im CHECKERS-Block „Strang A“, Prüferfunktionen im Block
  „Strang A“ am Dateiende (create, configure, kpis.read|configure|refresh, exit_model.read|refresh|submit).
- **Abweichung:** `v4/shell/workjet/tests/bridge.test.mjs` Zeile 55 (nicht im Whitelist): die Probe „unbekannte und
  nicht portierte Aktionen …“ nutzte `project.create` als Beispiel für „nicht unterstützt“; mit der Freischaltung
  wäre sie rot geworden (und hätte den Testprozess offen gehalten). Ersetzt durch `project.presentation.read`
  (bleibt unsupported bis E26), Zeile mit `// Strang A` markiert. Dieselbe Zeile nennt `speech.dictation` und
  `instance.grok.read` – Strang C/D trifft dasselbe.
- `v4/shell/workjet/WJB-A.md` (dieser Bericht). `index.js` unverändert (Aktionen laufen über `PROJECT_ACTIONS`).

## Gehört nach v2-helpers

Lokal in `actions/project.js`, wörtlich V2: `requireWorkjetSupervisorDataPlane` (ref app.js:15002-15008; Strang B
braucht dieselbe), `waitForSyncBridgeReady` (15458-15476), `waitForProjectedWorkjetProject` (15120-15154),
`waitForProjectedWorkjetWorkingCopy` (15171-15204), `boundedWorkjetWorkingCopyRequest` (15206-15216),
`workjetProjectChildCommandId` (15242-15251). Beim Umzug die Readiness-Hülle mitziehen (liest den Quelltext aus
project.js).

## Offene Fragen an den Kern / Orchestrator

1. **K-WJ-6 (= K-WJ-B1 von Strang B): null in Befehlsergebnissen.** Der Stub wendet Merge-Patch auf `command.result`
   an; Schlüssel mit `null` verschwinden (gemessen: `{a:null, b:{c:null,d:1}}` → `{b:{d:1}}`). Das Exit-Assessment hat
   Pflichtfelder „null erlaubt“ (run_id, as_of, exit_date, refresh_due, plan_summary, result) → Workjet läse die
   Antwort als Vertragsbruch. Frage: erhält der echte Kern null in `result`? Bis dahin todo-Probe in oracle.test.mjs.
2. ~~project.init freischalten?~~ Entschieden (Orchestrator): freigeschaltet mit eigener v4-Probe, siehe Nachtrag.
3. **K-WJ-4 (Idempotenz) bei project.create:** V2-Wiederholungen nutzen dieselbe commandId; in v4 erzeugt jede
   Wiederholung neue Befehlsdokumente (upsert und working_copy.upsert). Fachlich idempotent nur, wenn der Kern
   upsert über project_id bzw. (project_id, computer_id) entdoppelt.
4. **Export `desktop_files`** an die Shell für presentation.content.read (readMetadata), zusätzlich zu E26.
5. project.create prüft die upsert-Quittung wie V2 nicht; scheitert der Kern anders als mit unknown_command, wartet
   die Aktion bis zur Frist (30 s) und meldet `workjet_project_projection_timeout` (V2-Verhalten).
