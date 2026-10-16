# Shell v4 – Wächter-Baseline (Löschliste §11)

Lauf: 15.10.2026, 12:52, `node src/apps/business-os/v4/tools/guards/guard-v4.mjs --root ~/.local/state/shell-v4-src/4794af03d/src/apps/business-os --json` – unveränderte Quellkopie von origin/main 4794af03d (dieselbe Quelle wie analysis/A1–C2). Grund: im Klon `~/Documents/ctox-shell-v4` portieren andere Stränge gerade (modules/knowledge, v4/), dort schwanken die Zahlen minütlich. Zählmodus, `ported.json` ohne Einträge → Exit 0.

Gegenprobe im Klon zur selben Zeit (Zweig shell-v4/prototype, Arbeitsstand der parallelen Stränge): 3149 Treffer; abweichend nur `modules/knowledge` 33 → 2, `v4` 0 → 8.

Ziel Mo 05:00: jede Zeile unten auf null. Bis dahin ist diese Tabelle die Löschliste je App; eine portierte App wird in `ported.json` eingetragen, ab dann bricht jeder Treffer in ihr den Lauf (Exit 1).

## Summe

- **3172 Treffer** in 291 Dateien (982 Dateien gelesen, 1278 durchlaufen; ohne vendor/, rxdb/, v4/tools/, node_modules).
- Testdateien getrennt gezählt: 754 Treffer (nicht in der Summe; `--include-tests` rechnet sie ein).
- Nach Art: Apps 1754 (40 modules, 5 desktop-apps, customer-modules), Shell-Dateien 701, shared 465, Katalog 6, Werkzeuge (scripts/qa/output) 142, Sonstiges 104.
- Je Kategorie: vocab 1512 · sync-api 731 · command-bus 461 · reload 297 · catalog 120 · data-guard 34 · loop 15 · shell-heuristic 2.

## Treffer je Regel

| Regel | Kategorie | Treffer | Prüft |
|---|---|---:|---|
| `commandBus` | command-bus | 393 | Bezeichner commandBus |
| `cache-buster` | reload | 294 | ?v= Cache-Buster (JS, HTML, CSS) |
| `collectionReadiness` | sync-api | 162 | Bezeichner collectionReadiness |
| `vocab:de-synchronisier` | vocab | 162 | Vokabel: „Daten werden synchronisiert.“, „Inhalte werden synchronisiert“ |
| `vocab:en-unavailable` | vocab | 155 | Vokabel: „CTOX service is unavailable right now.“, „CTOX device control is unavailable.“ |
| `startCollection` | sync-api | 145 | Bezeichner startCollection |
| `vocab:en-pending-waiting` | vocab | 138 | Vokabel: „{n} pending write(s)“, „app collection readiness timed out after 30 seconds“ |
| `advancedStatus` | sync-api | 120 | Bezeichner advancedStatus |
| `vocab:de-nicht-verfuegbar` | vocab | 119 | Vokabel: „CTOX Status ist gerade nicht verfügbar.“, „CTOX Direktkanal ist nicht verfügbar.“ |
| `vocab:en-sync` | vocab | 104 | Vokabel: „Syncing content“, „Content synced“ |
| `vocab:de-fortschritt-wird` | vocab | 99 | Vokabel: „CTOX wird aktualisiert.“, „System wird gestartet...“ |
| `vocab:de-laeuft` | vocab | 97 | Vokabel: „Update läuft“, „Wiederherstellung läuft…“ |
| `vocab:de-geladen` | vocab | 95 | Vokabel: „Workspace wird geladen“, „Modul-Workspace wird geladen.“ |
| `vocab:de-warten` | vocab | 86 | Vokabel: „⏳ Warte auf Scan…“, „Wartet — blockiert, …“ |
| `ref-legacy-catalog-file` | catalog | 74 | Verweis auf registry.json, mobile-apps.json, system-apps.json oder schema.js |
| `until-accepted-terminal` | command-bus | 68 | until: 'accepted' \| 'terminal' |
| `vocab:en-loading` | vocab | 57 | Vokabel: „Loading workspace“, „Loading module workspace.“ |
| `vocab:both-replikation-recovery` | vocab | 50 | Vokabel: „CTOX-Dienst aktiv · Replikation wird verbunden“, „… im Reconnect“ |
| `vocab:de-veraltet-offline` | vocab | 50 | Vokabel: „Offline: angezeigte Daten können veraltet sein“, „Offline: displayed data may be out of date“ |
| `syncDiagnostics` | sync-api | 47 | Bezeichner syncDiagnostics |
| `requestNative` | sync-api | 46 | Bezeichner requestNative, requestNativeViaLeader, requestNativeDirectly |
| `file-schema-js` | catalog | 41 | Datei schema.js existiert (Collections nur aus collections.schema.json) |
| `vocab:de-verbindung` | vocab | 41 | Vokabel: „Datenverbindung wird neu verbunden“, „Verbindung verloren“ |
| `dataPlaneReady` | sync-api | 40 | Bezeichner dataPlaneReady |
| `vocab:en-progress-verb` | vocab | 38 | Vokabel: „Preparing workspace...“, „Starting realtime sync...“ |
| `leaseCollection` | sync-api | 35 | Bezeichner leaseCollection |
| `data-guard` | data-guard | 34 | Bezeichner createDynamicAppDataGuard, hasReviewedCollectionDataGrant, createGuardedRawDbProxy, createGuardedCollectionsProxy, createGuardedCollectionProxy, guardedCollectionFor, moduleUsesGuardedDataFacade, GUARDED_PACKAGED_DATA_MODULE_IDS, assertGuardedCollectionPermission, guardAllowsCollectionPermission |
| `restartCollection` | sync-api | 34 | Bezeichner restartCollection |
| `requireRevision` | sync-api | 31 | Bezeichner requireRevision |
| `vocab:en-not-ready` | vocab | 31 | Vokabel: „Business OS command bus is not ready.“, „… did not become ready before the command deadline.“ |
| `recoveryWarning` | sync-api | 28 | Bezeichner recoveryWarning |
| `vocab:de-nicht-bereit` | vocab | 28 | Vokabel: „Business OS Datenspeicher ist noch nicht bereit.“, „Die Crew ist noch nicht bereit.“ |
| `vocab:de-schreibgeschuetzt` | vocab | 26 | Vokabel: „… bleibt vorübergehend schreibgeschützt.“, „Apps bleiben bis zum Abschluss schreibgeschützt.“ |
| `vocab:de-vorbereitet` | vocab | 26 | Vokabel: „Business OS Datenspeicher wird noch vorbereitet.“, „Workspace wird vorbereitet...“ |
| `vocab:de-ausstehend` | vocab | 21 | Vokabel: „Die Rückmeldung steht noch aus.“ |
| `vocab:de-verzoegert` | vocab | 19 | Vokabel: „Ausführung verzögert“, „Das Speichern dauert länger als sonst. Neuer Versuch läuft...“ |
| `vocab:de-unbestaetigt` | vocab | 18 | Vokabel: „Stand unbestätigt“, „angezeigter Stand noch nicht bestätigt“ |
| `set-interval-undeclared` | loop | 15 | setInterval in Apps ohne Deklaration '// v4-allow: ui-animation' (gleiche oder vorige Zeile) (nur Apps) |
| `vocab:en-connecting` | vocab | 13 | Vokabel: „Connecting sync peers“, „Connecting“ |
| `vocab:de-abgleich` | vocab | 12 | Vokabel: „Daten werden abgeglichen: …“, „Kein Datenabgleich für diese Ansicht“ |
| `stopCollection` | sync-api | 10 | Bezeichner stopCollection |
| `vocab:both-update-status` | vocab | 9 | Vokabel: „Update verfügbar“, „Upgrade unterbrochen“ |
| `vocab:en-unconfirmed` | vocab | 9 | Vokabel: „Unconfirmed“, „Syncing data: displayed state is not yet confirmed“ |
| `vocab:de-nicht-rechtzeitig` | vocab | 8 | Vokabel: „… nicht rechtzeitig geladen“ |
| `readCollectionNativeDocument` | sync-api | 7 | Bezeichner readCollectionNativeDocument |
| `activeModuleSyncLease` | sync-api | 6 | Bezeichner activeModuleSyncLease |
| `ctoxHealthTimer` | sync-api | 6 | Bezeichner ctoxHealthTimer |
| `readNativeCollectionDocument` | sync-api | 5 | Bezeichner readNativeCollectionDocument |
| `leaseModule` | sync-api | 3 | Bezeichner leaseModule |
| `location-reload` | reload | 3 | location.reload |
| `waitForDataPlaneReady` | sync-api | 3 | Bezeichner waitForDataPlaneReady |
| `OFFLINE_FALLBACK_CATALOG` | catalog | 2 | Bezeichner OFFLINE_FALLBACK_CATALOG |
| `startMaintenanceMonitor` | sync-api | 2 | Bezeichner startMaintenanceMonitor |
| `wireShellV2ModuleTitle` | shell-heuristic | 2 | Bezeichner wireShellV2ModuleTitle |
| `file-mobile-apps-json` | catalog | 1 | Datei mobile-apps.json existiert |
| `file-registry-json` | catalog | 1 | Datei registry.json existiert |
| `file-system-apps-json` | catalog | 1 | Datei system-apps.json existiert |
| `localReplicaComplete` | sync-api | 1 | Bezeichner localReplicaComplete |
| `vocab:de-laedt` | vocab | 1 | Vokabel: „lädt…“, „Lädt“ |
| `fetch-absolute-or-api` | http | 0 | fetch( auf absolute (/, //, http) oder /api-URL in Apps (nur Apps) |
| **Summe** | | **3172** | |

## Treffer je App (40 modules)

| App/Bereich | Gesamt | sync-api | command-bus | reload | catalog | data-guard | loop | http | shell-heuristic | vocab | Tests (getrennt) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| modules/ctox | **126** | 14 | 14 | 7 | 2 | 0 | 1 | 0 | 0 | 88 | 20 |
| modules/research | **97** | 19 | 4 | 2 | 4 | 0 | 0 | 0 | 0 | 68 | 13 |
| modules/matching | **88** | 17 | 21 | 7 | 1 | 0 | 1 | 0 | 0 | 41 | 8 |
| modules/documents | **81** | 9 | 3 | 5 | 1 | 0 | 0 | 0 | 0 | 63 | 29 |
| modules/conversations | **76** | 22 | 3 | 5 | 2 | 0 | 0 | 0 | 0 | 44 | 7 |
| modules/outbound | **76** | 9 | 19 | 6 | 1 | 0 | 2 | 0 | 0 | 39 | 3 |
| modules/mail | **72** | 8 | 3 | 4 | 5 | 0 | 1 | 0 | 0 | 51 | 26 |
| modules/browser | **70** | 25 | 12 | 1 | 1 | 0 | 1 | 0 | 0 | 30 | 12 |
| modules/cv-print-builder | **64** | 13 | 2 | 7 | 4 | 0 | 0 | 0 | 0 | 38 | 5 |
| modules/appsec-pentest | **59** | 8 | 17 | 2 | 1 | 0 | 0 | 0 | 0 | 31 | 10 |
| modules/desktop | **54** | 5 | 3 | 6 | 3 | 0 | 2 | 0 | 0 | 35 | 21 |
| modules/reports | **53** | 10 | 3 | 4 | 3 | 0 | 0 | 0 | 0 | 33 | 2 |
| modules/tickets | **49** | 4 | 3 | 8 | 1 | 0 | 0 | 0 | 0 | 33 | 6 |
| modules/threads | **39** | 7 | 2 | 4 | 2 | 0 | 1 | 0 | 0 | 23 | 3 |
| modules/knowledge | **33** | 10 | 4 | 4 | 1 | 0 | 0 | 0 | 0 | 14 | 5 |
| modules/app-store | **32** | 7 | 2 | 8 | 1 | 0 | 0 | 0 | 0 | 14 | 15 |
| modules/spreadsheets | **31** | 5 | 1 | 4 | 1 | 0 | 0 | 0 | 0 | 20 | 26 |
| modules/support | **30** | 7 | 2 | 6 | 6 | 0 | 0 | 0 | 0 | 9 | 0 |
| modules/customers | **28** | 5 | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 18 | 2 |
| modules/importer | **28** | 2 | 5 | 2 | 1 | 0 | 0 | 0 | 0 | 18 | 6 |
| modules/nachweise | **28** | 3 | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 20 | 6 |
| modules/coding-agents | **27** | 5 | 6 | 4 | 1 | 0 | 0 | 0 | 0 | 11 | 0 |
| modules/file-viewer | **25** | 13 | 2 | 5 | 3 | 0 | 0 | 0 | 0 | 2 | 2 |
| modules/consent | **23** | 2 | 4 | 4 | 1 | 0 | 0 | 0 | 0 | 12 | 5 |
| modules/explorer | **23** | 5 | 0 | 5 | 7 | 0 | 0 | 0 | 0 | 6 | 2 |
| modules/creator | **21** | 6 | 2 | 5 | 1 | 0 | 0 | 0 | 0 | 7 | 2 |
| modules/notes | **20** | 2 | 0 | 4 | 1 | 0 | 0 | 0 | 0 | 13 | 2 |
| modules/esign | **17** | 2 | 4 | 2 | 1 | 0 | 0 | 0 | 0 | 8 | 1 |
| modules/intake | **17** | 3 | 1 | 2 | 1 | 0 | 0 | 0 | 0 | 10 | 6 |
| modules/placements | **17** | 3 | 3 | 2 | 1 | 0 | 0 | 0 | 0 | 8 | 1 |
| modules/buchhaltung | **16** | 2 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 11 | 4 |
| modules/submissions | **16** | 3 | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 8 | 1 |
| modules/iot | **15** | 2 | 1 | 6 | 1 | 0 | 0 | 0 | 0 | 5 | 4 |
| modules/credentials | **14** | 2 | 1 | 5 | 1 | 0 | 0 | 0 | 0 | 5 | 7 |
| modules/kundenpipeline | **14** | 2 | 2 | 1 | 2 | 0 | 1 | 0 | 0 | 6 | 2 |
| modules/calendar | **13** | 3 | 0 | 4 | 1 | 0 | 0 | 0 | 0 | 5 | 3 |
| modules/invoices | **13** | 1 | 1 | 5 | 4 | 0 | 0 | 0 | 0 | 2 | 5 |
| modules/interviews | **12** | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 6 | 1 |
| modules/news | **12** | 0 | 2 | 4 | 1 | 0 | 0 | 0 | 0 | 5 | 1 |
| modules/shiftflow | **10** | 1 | 0 | 4 | 1 | 0 | 0 | 0 | 0 | 4 | 4 |
| **Summe modules** | **1539** | 269 | 159 | 163 | 74 | 0 | 10 | 0 | 0 | 864 | 278 |

## desktop-apps und customer-modules

| App/Bereich | Gesamt | sync-api | command-bus | reload | catalog | data-guard | loop | http | shell-heuristic | vocab | Tests (getrennt) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| customer-modules/outbound-lead-generation | **168** | 21 | 17 | 2 | 1 | 0 | 5 | 0 | 0 | 122 | 0 |
| desktop-apps/file-viewer | **18** | 13 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 2 |
| desktop-apps/code-editor | **16** | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 7 | 1 |
| desktop-apps/explorer | **12** | 5 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 6 | 2 |
| desktop-apps/creator | **1** | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| desktop-apps/browser | **0** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **Summe** | **215** | 42 | 22 | 6 | 3 | 0 | 5 | 0 | 0 | 137 | 5 |

## Shell-Dateien, shared, Katalog

| App/Bereich | Gesamt | sync-api | command-bus | reload | catalog | data-guard | loop | http | shell-heuristic | vocab | Tests (getrennt) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| shell/app.js | **674** | 282 | 110 | 75 | 8 | 34 | 0 | 0 | 2 | 163 | 0 |
| shell/index.html | **20** | 0 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 |
| shell/design-lab.js | **2** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| shell/mobile-host.js | **2** | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 |
| shell/design-lab.html | **1** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |
| shell/mobile-apps.json | **1** | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| shell/system-apps.json | **1** | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| shared | **465** | 105 | 143 | 28 | 1 | 0 | 0 | 0 | 0 | 188 | 373 |
| modules/registry.json | **6** | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 5 | 0 |

shared – die 15 Dateien mit den meisten Treffern: `business-chat.js` 126 · `react-settings.js` 115 · `sync.js` 68 · `command-bus.js` 21 · `collection-freshness.js` 20 · `db.js` 14 · `app-actions.js` 10 · `shell-release-status.js` 10 · `documents.js` 9 · `maintenance-state.js` 8 · `universal-importer.js` 7 · `workjet-project-control.browser.mjs` 7 · `business-reporter.js` 6 · `sync-contract.js` 6 · `workjet-calendar-native.mjs` 5.

## Werkzeuge und Sonstiges (nicht Laufzeit der Apps, aber in src/apps/business-os)

| App/Bereich | Gesamt | sync-api | command-bus | reload | catalog | data-guard | loop | http | shell-heuristic | vocab | Tests (getrennt) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| scripts | **133** | 13 | 12 | 11 | 30 | 0 | 0 | 0 | 0 | 67 | 31 |
| output | **6** | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| qa | **3** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| office-engine | **88** | 14 | 15 | 4 | 0 | 0 | 0 | 0 | 0 | 55 | 65 |
| app-starter | **7** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7 | 0 |
| public | **7** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7 | 0 |
| ui-contract | **2** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |

## Messregeln (wie der Wächter zählt)

- Bezeichner-Regeln gelten für Bezeichner außerhalb von Kommentaren und Strings; eine Familie (z. B. `subscribeCollectionReadiness`, `renderAdvancedStatus…`) zählt beim enthaltenen Namen an einer camelCase-Grenze, je Token genau eine Regel (längster Name gewinnt). `requestNative`, `data-guard`, `OFFLINE_FALLBACK_CATALOG`, `wireShellV2ModuleTitle` zählen nur exakt.
- `?v=` zählt in JS, HTML und CSS ohne Kommentare; `until: 'accepted'|'terminal'` in Code; `location.reload` in Code.
- `setInterval` und `fetch(` auf `/…`, `//…`, `http(s)://…` oder `…/api…` zählen nur in modules/, desktop-apps/, customer-modules/. Deklaration einer reinen UI-Animation: `// v4-allow: ui-animation` in derselben oder der vorigen Zeile.
- Existenz: jede Datei `registry.json`, `mobile-apps.json`, `system-apps.json`, `schema.js` ist ein Treffer; Verweise darauf in Strings zählen zusätzlich als `ref-legacy-catalog-file`.
- Zustandsvokabular (`vocabulary.json`, 27 Einträge DE/EN) wird nur in String-Literalen, Template-Texten, HTML-Text/Attributwerten und JSON-Werten gesucht, ein Treffer je Zeile und Eintrag. Ausgenommen: Kommentare, Argumente von `status.report/clear(…)` und `zone.report(…)`, `console.*(…)`, Literale in Schlüsselform ohne Leerzeichen (`'syncing'`, `'sync.loading'`, `'loadingModule'`), JSON-Schlüssel, JSON-Werte unter Schlüsseln `status.*`/`status_*`, Dateien in `ported.json:statusZoneFiles` (die Zone selbst), CSS, `*.schema.json`, `module.json`, `package*.json`. Geworfene Fehlertexte zählen mit (sie schlagen heute bis ins UI durch, B §4c).
- Testdateien: Name enthält `test`/`smoke` oder Ordner `tests/`/`fixtures/`.
- Die Zahlen sind vom Wächter gemessen, nicht übernommen; Abweichungen zu analysis/C1/C2 (dort heuristisch, eindeutige Literale) sind erwartbar.

## Aufruf

```
node src/apps/business-os/v4/tools/guards/guard-v4.mjs              # Treffer je Datei:Zeile + Summen
node src/apps/business-os/v4/tools/guards/guard-v4.mjs --summary    # nur Summen
node src/apps/business-os/v4/tools/guards/guard-v4.mjs --json       # maschinenlesbar
node src/apps/business-os/v4/tools/guards/guard-v4.mjs --app modules/news --rule vocab   # Löschliste einer App
node --test src/apps/business-os/v4/tools/guards/guard-v4.test.mjs
```
