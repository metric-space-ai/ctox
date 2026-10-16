# Shell v4 – Alt-Tests (Strang OLDTEST)

Stand: Do 15.10.2026 18:4x (Berlin), Klon `~/Documents/ctox-shell-v4`. Regel: Löschen statt stilllegen; ein roter Test ist nie
„bekannt rot“ (E13). Jede Testdatei einzeln gelaufen:

```
cd src/apps/business-os
CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/7706796 \
  node --import ./v4/tools/node/ctox-shared.mjs --test <datei>
```

ohne `NODE_PATH`, Zeitlimit 300 s je Datei. „vorher/nachher“ = Exit-Code und pass/fail des Laufs.

**Lader-Muster** (für alle Lader-Fälle gleich, keine Installation): `esbuild`/`playwright` per
`createRequire(import.meta.url)` aus dem Klon, sonst `ESBUILD_MODULE`/`PLAYWRIGHT_MODULE`, sonst aus
`~/Documents/ctox/src/apps/business-os/node_modules` – wie die vorhandenen v4-api-Tests mit `PLAYWRIGHT_MODULE`.
Ursache aller Lader-Fälle: der Klon hat kein `node_modules`, ESM-Bare-Imports (`import … from 'esbuild'`) ignorieren
`NODE_PATH`, und `playwright` ist CommonJS (benannter ESM-Import `{ chromium }` scheitert).

**Mutationsbeleg**: App-Datei kurzzeitig verändert, Test gelaufen, Datei per Byte-Kopie zurückgesetzt und per sha256
gegengeprüft (App-Code bleibt unverändert). KILLED = Test wird rot.

## Runde 1 – Apps außerhalb der Erstausnahme

Geprüft: 137 Testdateien in 25 Apps (app-store, browser, buchhaltung, calendar, coding-agents, consent, conversations,
creator, credentials, ctox, customers, desktop, esign, explorer, file-viewer, importer, intake, interviews, invoices,
mail, nachweise, notes, placements, shiftflow, submissions) plus `desktop-apps/code-editor`. Vorher 124 grün, 13 rot
(davon 1 keine Testdatei, s. Befunde).

| Datei | vorher | Entscheidung | Begründung | nachher |
|---|---|---|---|---|
| modules/calendar/calendar.test.mjs | rot (ERR_MODULE_NOT_FOUND esbuild) | Lader | Testet geltende Fachregeln (Validierung Termine/Buchungsseiten, Recurrence-IDs) gegen v4-`index.js`, bereits v4-angepasst (ctox:shared-Plugin) | grün 16/16 |
| modules/creator/creator.test.mjs | rot (esbuild) | Lader | v4-angepasst (api.query, keine Wartebefehle); nur esbuild fehlte | grün 23/23 |
| modules/credentials/credentials.test.mjs | rot (esbuild) | Lader | Fachregeln Schlüsselformat, nie Geheimwert in Export/Detail; v4-angepasst | grün 17/17 |
| modules/ctox/test.js | rot (esbuild) | Lader | Task-/Flow-Fachregeln gegen v4-`index.js` (eigener `test()`-Wrapper, ~40 Fälle) | grün (6 node:test + Wrapper ok) |
| modules/ctox/tests/ctox.test.mjs | rot (esbuild über `../test.js`) | Lader (in `test.js`) | Datei ist nur `import '../test.js'`; grün durch Lader in test.js | grün 6/6 |
| modules/ctox/tests/terminal-endpoint.test.mjs | rot (esbuild über `bundle.mjs`) | Lader (in `tests/bundle.mjs`) | Fachregeln Terminal-Endpunkt; Lader im gemeinsamen Bündel-Helfer | grün 3/3 |
| modules/customers/customers.test.mjs | rot (esbuild) | Lader | Schema aus collections.schema.json (schema.js schon entfernt), v4-Manifest, Hooks | grün 1/1 (165 asserts) |
| modules/importer/importer.test.mjs | rot (esbuild) | Lader | Fachregeln GitHub-URL, Modul-IDs, Harness-Befehl; v4-angepasst | grün 7/7 |
| modules/mail/tests/mail.test.mjs | rot (esbuild) | Lader | Mail-Fachregeln; prüft weiter die exakte esbuild-Version gegen package.json (0.28.1, Haupt-Checkout erfüllt sie) | grün 26/26 |
| modules/mail/editor/tests/logic-editor.browser.test.mjs | rot (playwright nicht auflösbar / CJS) | Lader | Browser-DOM-Test des Logik-Editors, Code unverändert gültig | grün 1/1 |
| modules/coding-agents/tests/coding-agents.test.mjs | rot 13/6 | umgeschrieben (3 Fälle), 3 Fälle gelöscht | s. unten | grün 16/16 |
| modules/consent/handlers/tests/consent.test.mjs | rot 3/2 | umgeschrieben (2 Fälle) | s. unten | grün 5/5 |

### coding-agents/tests/coding-agents.test.mjs (V4-PORT.md OFFEN 5 nennt die Datei als nicht umgestellten Alt-Test)

- **umgeschrieben** „presentation layer stays compact and shell-native“: Gestaltungsregeln bleiben. Ersetzt wurden die
  v2-Shell-Annahmen `getActionIcon?.('export')` (App-API 4.0 hat keine Icon-API, OFFEN-D → app-lokales Inline-Glyph)
  und das DOM-Ereignis `ctox-business-os-app-launch` (→ `api.shell.onRoute` mit `params.moduleId` → `focusRequestedApp`).
  Mutation M1 (Export-Glyph wieder über `getActionIcon`) KILLED, M2 (onRoute fokussiert nicht) KILLED.
- **umgeschrieben** „app catalog normalization …“: Regel sichtbar + editierbar + dedupliziert + nach Titel sortiert
  bleibt; v4-Zeile trägt zusätzlich `iconSvg`/`iconAsset` aus der Plattform-Zeile (geprüft). M3 (Icon fällt weg)
  KILLED, M6 (`editable:false` sichtbar) KILLED.
- **umgeschrieben** „model capability discovery …“ (vorher: wartet auf `until: 'terminal'`, 120-s-Frist – v2-Befehlsbus,
  in v4 verboten): jetzt `api.command(CODING_MODELS_COMMAND, …)` ohne `until`, Modellliste aus `CommandState.result`
  bei `done` ersetzt die Presets, `failed` → Zone. M4 (Serverliste ignoriert) KILLED, M5 (`until: 'terminal'`) KILLED.
- **gelöscht** „projection errors surface policy denials …“: Hook `turnErrorFromProjection` existiert in v4 nicht
  mehr; Fehleranzeige eines Turns deckt `tests/v4-api.test.mjs` („failed → genau ein Zoneneintrag“) ab.
- **gelöscht** „ungranted transcript projections stay optional …“: `filterReadableCollectionNames`/`ctx.permissions`
  entfernt; Lesegrenze ist in v4 das Manifest (Kern), keine App-Regel mehr.
- **gelöscht** „data-driven empty center views become sync states …“: prüfte `sync.collectionReadiness`/`.ctox-syncing`
  – in v4 das Gegenteil der Regel (leere Ansichten rendern nichts, Zustand nur in der Zone; v4-api-Test „leere
  Collections rendern keinen Zustandstext“).
- Der Testname des neuen Modell-Falls wurde so gewählt, dass guard-v4 (`--include-tests`) keinen Vokabeltreffer zählt.

### consent/handlers/tests/consent.test.mjs

Ursache: Manifest-Generator M2 (`dcb9d007f`) hat `foreign_collections` in `v4/manifests/consent.manifest.json`
eingetragen (alle ATS-Collections plus `accounting_invoices` lesend, `_proposal` aus Code-Literal). Die Tests nahmen
„nicht deklariert“ an. Fachregel unverändert: Zugriff nur über Manifest-Deklaration, sonst `forbidden` statt still leer.

- **umgeschrieben** „retention.due liest nur; purge nur ATS-Collections …“: `retention.due` auf eine nicht deklarierte
  Collection (`customer_accounts` statt des jetzt deklarierten `accounting_invoices`) → `forbidden`. C1 (`due` schluckt
  `forbidden` und liefert leer) KILLED, C2 (`purge` ohne ATS-Allowlist) KILLED.
- **umgeschrieben** „ohne Fremd-Deklaration verweigert v4 den Export“: fehlender Lesezugriff auf eine ATS-Collection
  (`submissions` mit `ops: []`) → `forbidden`; zusätzlich: mit Manifest-Deklaration allein läuft der Export. C3
  (`loadByField` überspringt still) KILLED.

### Befunde Runde 1 (nicht behoben, App-Code)

- `modules/buchhaltung/core/ui_e2e_tests.js` ist **keine Testdatei**, sondern App-Code (von `index.js:14` importierte
  In-App-E2E-Suite); unter Node rot (`document is not defined`), nicht im Testumfang. Befund: die Suite greift auf
  `state.ctx.db.collection()` und `state.ctx.permissions.canReadCollection` zu (v2-Shell-API); unter App-API 4.0 liefert
  `e2eDb()` damit immer `null` – die In-App-E2E-Suite ist faktisch tot. Owner: buchhaltung-Port.
- `modules/ctox/index.js:105/177` und `:376/448`: doppelter Schlüssel `retryAt` im Label-Objekt (esbuild-Warnung
  `duplicate-object-key`); harmlos, aber Doppelcode.
- Opt-in-Screenshot-Fälle (`{ skip: !…SHOTS }`) in ctox/mail/notes/shiftflow `tests/v4-api.test.mjs` sind
  Artefakt-Erzeuger, keine Invarianten; nicht geändert.
- Nebenwirkung: v4-api-Tests schreiben bei jedem Lauf `tests/shots/*.png` neu (z. B. conversations, importer,
  interviews) – Lauf erzeugt Diffs an Binärdateien.

## Runde 2 – nach Neustart freigegebene Apps

Freigegeben (Koordinator, nach Neustart): news, appsec-pentest, tickets, support, threads, reports, documents,
spreadsheets, cv-print-builder, kundenpipeline, matching. Weiter ausgenommen: outbound, outbound-lead-generation, iot,
browser, research, knowledge (browser war in Runde 1 grün und wurde nicht angefasst). 84 Testdateien, vorher 74 grün,
10 rot.

| Datei | vorher | Entscheidung | Begründung | nachher |
|---|---|---|---|---|
| modules/appsec-pentest/appsec-pentest.test.mjs | rot (`./schema.js` fehlt) | umgeschrieben | Vertragstest auf Modulebene; 22 von ~330 Zusicherungen gegen v4 rot (einzeln geprüft). s. unten | grün |
| modules/appsec-pentest/appsec-pentest.browser.test.mjs | rot (`mount(ctx)`, `api.shell` undefined) | umgeschrieben | Browser-Lebenszyklus mit vielen Fachregeln (Simple-Audit, Exploit-Sortierung, Verify-Pillen, Crafting, Freigaben, Rework-Dialog); Attrappe von `ctx` auf App-API 4.0 umgestellt. s. unten | grün |
| modules/documents/documents.test.mjs | rot (esbuild; danach 24 rot) | Lader + umgeschrieben, 9 Fälle gelöscht | s. unten | grün 44/44 |
| modules/documents/mail-merge-recipient-switch.test.mjs | rot 5/2 | umgeschrieben (2 Fälle) | `ctx.host` → `state.root`, `api.status`; Blob-Cache über `api.get('files')`/`api.files.get` statt `document_blob_chunks` | grün 7/7 |
| modules/documents/documents-layout.browser.test.mjs | rot (playwright CJS; danach 1 !== 3 Spalten) | Lader | playwright-Lader; Fixture setzt `data-app="documents"` am Host wie der v4-Rahmen (index.css ist darauf gescoped) – Test selbst unverändert | grün 1/1 |
| modules/spreadsheets/spreadsheets.test.mjs | rot (esbuild; danach 36 rot) | Lader + umgeschrieben, 19 Fälle gelöscht | s. unten | grün 29/29 |
| modules/spreadsheets/spreadsheets-version.browser.test.mjs | rot (esbuild/playwright; danach Timeout) | Lader + umgeschrieben | v2-Wiederholen-Knopf bei Transportfehler existiert in v4 nicht (R4); Regel jetzt: fehlende Version → Zone `spreadsheets.version.missing`, kein Text/Abzeichen im Inhalt, keine rechte Spalte. SV1 (keine Meldung) KILLED, SV2 (Absenztext im Inhalt) KILLED | grün 1/1 |
| modules/reports/test.mjs | rot (esbuild; danach 1 CSS-Regex) | Lader + umgeschrieben (1 Zusicherung) | Regel „gestapelter Rail-Kopf hält die Fensterzone frei“ gilt; Wert kommt jetzt als `var(--shell-v2-control-inset, 70px)` (DEBT D-004). R1 (14px) KILLED | grün |
| modules/reports/tests/reports.test.mjs | rot (über `../test.mjs`) | Lader (in test.mjs) | nur `import '../test.mjs'` | grün |
| modules/support/tests/support.test.mjs | rot 13/1 | umgeschrieben (1 Zusicherung) | Commit VIS3 `8086cc80f` (nach Rundenbeginn) gibt der Queue-Reiterleiste bewusst `flex-wrap` (Abschneiden behoben, Labor 24/24); Regel „keine eigene Chrome-CSS“ bleibt, mit genau dieser Ausnahme. SU1 (Umbruch weg) KILLED, SU2 (weitere Reiterleisten-Regel) KILLED | grün 14/14 |

### appsec-pentest.test.mjs (Einzelprüfung jeder Zusicherung gegen v4)

- **gelöscht** (gelöschte Dateien/Altwege ohne v4-Fachregel): Registry-Eintrag/-Titel/-Layout und `system-apps.json`
  (5, beide Dateien auf der Löschliste), Drift-Vergleich `schema.js` ↔ `collections.schema.json` (ersetzt durch „schema.js
  ist entfernt“), `until: 'terminal'/'accepted'` bei create/archive/run/audit.run (4, ersetzt durch eine Invariante „kein
  Befehl wartet“), `resumeTracking`, `refreshAppsecData`, `sync.restartCollections`, Locale-Schlüssel
  `{0} in progress...` (Fortschrittstext existiert in v4 nicht mehr).
- **umgeschrieben**: `ctx.storageScope` → `snapshot()`/`restored` (A1, A2 KILLED); i18n über `../../shared/i18n.js` →
  relativ geladenes `locales/de.json` ohne `?v=` (A3 KILLED); Remount-Hygiene → Abos je Montage, `unmount` gibt alle
  frei (A4 KILLED); Dialog-CSS auf gescopte Selektoren `[data-app="appsec-pentest"]` (A5 KILLED); Locale-Parität →
  en.json bildet jeden Schlüssel auf sich ab und de.json übersetzt alle (A6 KILLED); kein `until` (A7 KILLED);
  Befund-Knopf → `ctox.appsec.finding.remediate` statt `business_os.chat.task` (A9 KILLED; die alte Zusicherung war
  „grün“ nur, weil `business_os.chat.task` noch als Label in einer Tabelle steht).

### appsec-pentest.browser.test.mjs

Attrappe neu: `api.query` (Abo mit Selektor), `api.command` (Handle mit `status`-Abo; der Core projiziert die Wirkung
zurück, ersetzt `restartCollections`), `api.status`, `api.shell` (`t`, `locale`, `dialog`), App-Katalog über
`business_apps`; Montage `mount(host, api, {restored})`, Remount über `snapshot()`. Entfernt: `options.until`-Zusicherungen
(9), `restartCalls`. Umgestellt: Remediation-Befehl `ctox.appsec.finding.remediate` (Payload `category`,
`evidence_artifacts`), Validierungsfehler aus der Zone (`appsec.ui`) statt aus `[data-appsec-simple-status]`.
Mutationen: AB1 Auswahl nicht im Snapshot, AB2 Remediation ohne Nachweispfade, AB3 Validierungsfehler nicht in die
Zone, AB4 Katalog nicht aus `business_apps` – alle KILLED.

### documents/documents.test.mjs

- **Lader**: esbuild wie oben; Bündel bildet `ctox:shared/` wie der SW ab und lässt `ctox:lib/<name>@<major>/`
  (Shell-Standardbibliothek, dynamischer Import) extern.
- **gelöscht** (Hooks existieren nicht mehr, v2-Replikations-/Readiness-/Recovery-Pfad, in v4 entfallen):
  „readiness event recovers …“ (`wireDocumentReadiness`), „missing documents collection is never reported …“,
  „Knowledge refresh reads only changed collections …“ (`refreshKnowledge`), „document versions open from local state
  …“, „missing or transient local document versions wait for replication once“, „document metadata deadlines …“,
  „document metadata timeout does not relabel …“, „document recovery upgrades a follower …“, „document recovery
  propagates a refused channel …“ (`resolveLocalFirst`/`awaitDocumentVersionReplication`).
- **umgeschrieben**: Word-Versionsladen (3 Szenarien; `head-after-recovery` entfällt mit dem Recovery-Pfad) gegen
  `api.get`/`api.query` – DM1 (Kopf-Rückfall nicht auf das Dokument begrenzt) KILLED; Refresh liest bei fremden
  Änderungen nichts – DM2 KILLED; Hintergrund-Refresh rendert nach Schließen nicht – DM3 KILLED; Listenzustand nur
  rows/filtered/empty (R11); Startfehler: Replikationsabbrüche sind keine wiederholbaren Startfehler mehr – DM4 KILLED;
  Rechte: Kommentar/Review folgen `write`, kein `ctx.permissions` – DM5 KILLED; Serienbrief-Navigator (2) über
  `api.query` – DM9 KILLED; UI-Quelltextregeln auf gescopte CSS, `state.root`, `index.html` ohne `?v=`; Leer-Knopf
  `data-documents-empty-new` entfällt (R11) – DM6 (Menü an `document.body`) KILLED; Bytes als EIN Blob über
  `api.files.put`, Referenz erst nach `local` – DM7 KILLED.

### spreadsheets/spreadsheets.test.mjs

- **Lader** wie documents.
- **gelöscht** (19 Fälle, entfernte Hooks des v2-Lade-/Replikationspfads): version reads locally, metadata deadline,
  metadata recovery relabel, peer-reopen bounded ×3, absence after recovery, terminal codes, supersession ×5, pending
  direct bridge, refused direct channel, reconnect budget, cancelled direct bridge (`resolveSpreadsheetVersionLocalFirst`,
  `awaitSpreadsheetVersionReplication`, `isTransientSpreadsheetVersionReadError`), runtime waits for initial
  replication (`ensureSpreadsheetRuntimeReady`), explorer shows syncing (`shouldRenderSpreadsheetsSyncing`).
- **umgeschrieben**: Editor-Fehler (csv/unsupported) → Zone statt Fehlerkasten (S7 KILLED); Refresh liest bei fremden
  Änderungen nichts (S1 KILLED); kein Rendern nach Schließen während des Lesens (S2 KILLED); Lader: missing/ready,
  Editor arbeitet auf Kopie der eingefrorenen Live-Zeile (S3 KILLED); kein Übernehmen nach Schließen/Entwurf (S4 KILLED;
  Variante „concurrent selection“ entfällt, v4 liest synchron ohne Wartefenster); keine rechte Spalte (`renderRight`
  leer); Bericht-Snapshot-Fixture (6 Fälle) auf `api.put`/`api.files.put`, Abo-Push über `rawSpreadsheets` – Prüfung
  des Snapshot-Digests S6b KILLED; Bytes über `api.files.put`, Referenz erst nach `local` (S5 KILLED); Kontextmenü am
  Fenster-Root (S8 KILLED).

### Befunde Runde 2 (nicht behoben)

- appsec-pentest: `locales/en.json` fehlt der Schlüssel `No evidence yet` (de.json hat ihn); v4 lädt en.json nie
  (`loadMessages` nur für de) – en.json ist faktisch toter Bestand. Mutation A8 (de-Schlüssel umbenannt) überlebt
  deshalb; Owner appsec.
- documents: `ctoxDocumentsPermissions()` liefert konstant `write/comment/review: true`; Leserechte-Nutzer sehen
  Kommentar/Review, die Durchsetzung liegt nur noch im Kern. Owner documents/Kern.
- spreadsheets: `assertSpreadsheetSnapshotBytes` steht doppelt (`index.js:251` und `:554`); Entfernen einer Stelle
  bleibt unentdeckt (S6 überlebt), die Funktion selbst ist abgesichert (S6b).
- `modules/documents/index.js` wurde während des Laufs von einem Parallelstrang geändert (Kontext-Chat, SKL-D); alle
  documents-Tests sind gegen den Stand 18:28+ grün.
- Mein Lader-Edit in `modules/mail/tests/mail.test.mjs` ist im fremden Commit SKL-D `0a544610f` mitgelandet.
- Mutationsläufe S1–S8 liefen zuerst parallel auf dem Live-Baum; `git diff` belegt `spreadsheets/index.js` danach
  unverändert, alle Belege wurden sequenziell in einer Kopie (Scratchpad) wiederholt.

## Ausgenommene Apps – nur Befund (Stand nach Runde 2)

Gelaufen in einer Kopie des Arbeitsbaums (keine Screenshot-Nebenwirkung im Klon), 101 Testdateien in outbound,
outbound-lead-generation, iot, browser, research, knowledge: 98 grün, 3 rot – alle drei reine Lader-Fälle (mit
esbuild-Auflösung aus dem Haupt-Checkout grün):

| Datei | Befund | mit Lader |
|---|---|---|
| modules/iot/iot.test.mjs | ERR_MODULE_NOT_FOUND esbuild | grün 11/11 |
| modules/knowledge/test.mjs | ERR_MODULE_NOT_FOUND esbuild | grün |
| modules/knowledge/tests/knowledge.test.mjs | rot über `../test.mjs` (esbuild) | grün |

Erstbefund 18:10 (Erstausnahme, inzwischen in Runde 2 erledigt oder grün geworden): appsec-pentest ×2, documents ×3,
reports ×1, spreadsheets ×2, iot ×1, knowledge ×1 (Lader), outbound `handlers/tests/legacy-forwarders.test.mjs` 8/1
(inzwischen grün durch den outbound-Strang).

## Abnahme (gemessen nach Runde 2)

- Alle Testdateien der bearbeiteten Apps einzeln: **218 Dateien, 218 grün, 1489 pass, 0 fail** (37 Apps inkl.
  desktop-apps/code-editor; ohne outbound, outbound-lead-generation, iot, browser, research, knowledge).
- `node v4/tools/guards/guard-v4.mjs --summary`: Exit 0, 1483 Treffer (unverändert zu 18:01; Testdateien getrennt).
- `node v4/debt/ratchet.mjs`: Exit 0 „ratchet: OK“, guard 1483 · scan 3, nichts gestiegen.
- Keine neuen Vokabeltreffer in geänderten Testdateien (`guard-v4 --include-tests`, geprüft für die Lader-Blöcke und
  die neuen Testnamen).

Zählung gesamt: **gelöscht** 0 Dateien, 34 Testfälle (coding-agents 3, documents 9, spreadsheets 19 inkl.
Schleifenfälle, appsec-browser 0, dazu ~15 Einzelzusicherungen in appsec-pentest.test.mjs); **umgeschrieben** 10
Dateien (coding-agents, consent-handlers, appsec ×2, documents.test, mail-merge-recipient-switch, spreadsheets ×2,
reports/test.mjs, support); **Lader** 14 Dateien (calendar, creator, credentials, ctox/test.js, ctox/tests/bundle.mjs,
customers, importer, mail/tests/mail.test.mjs, mail logic-editor.browser, reports/test.mjs, documents.test.mjs,
documents-layout.browser, spreadsheets.test.mjs, spreadsheets-version.browser – davon 4 zugleich umgeschrieben).
Vorher rot und jetzt grün: 22 Dateien (Runde 1: 12, Runde 2: 10); `buchhaltung/core/ui_e2e_tests.js` ist keine
Testdatei (Befund Runde 1).
