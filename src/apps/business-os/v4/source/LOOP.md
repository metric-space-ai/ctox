# Strang LOOP – Meldeschleife (Owner-Mangel M-06, Bedienkonzept Fassung 4 §3a)

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (shell-v4/prototype), Stub-Pin `8489b4e`. Kein git außer Lesen.

## Ablauf (fünf Schritte, wie umgesetzt)

| Schritt | Wo | Was passiert |
|---|---|---|
| 1 Melden | App-Menü am Icon-Header / Strg+Alt+B (bestand, `v4/shell/report.js` reportProblem) | Zeilen in `business_module_reports` + `ctox_bug_reports`, `module_id` = App |
| 2 Sichtbar bei der App | Code-Ansicht, Reiter **„Meldungen“** neben „Dateien“ oben im Baum (`v4/source/reports-tab.js`) | offene Meldungen (open/running) genau dieser App, älteste zuerst, `#N`, Titel, Art (Problem/Wunsch), Melder („Du“ bzw. Name/ID), Datum, Status; Zahl am Reiter; live |
| 3 Agent beauftragen | Knopf je Meldung | Prompt = Meldung #N + Text + Erwartung + App/Version (+ Datensatz) + Zonenfehler (Meldung und jetzt); `ctox.coding.turn` in §16.44-Form über die Agent-Seitenleiste (Entwurf anlegen/nutzen, `base_draft_rev` aus `draft.write().result.rev` bzw. `draft.info().rev`, Retry bei `stale_draft_rev`), Befehl mit `record_id` = Meldung; Meldung → `status: running` („In Arbeit“), `coding_turn_id`, `coding_draft_id`, `ctox_command_id`, `client_context.report_delivery {status:'delegated', turn_id}`; „Lauf ansehen“ öffnet die Seitenleiste und hebt den Turn hervor |
| 4 Veröffentlichen | „Veröffentlichen“ im Editor-Kopf | mit verknüpften offenen Meldungen (running + coding_turn_id auf diesem Entwurf) fragt das Fensterblatt die **Versionsnotiz** ab, Vorschlag „behebt Meldung #N“ (mehrere: „behebt Meldungen #1, #3“), änderbar; Abbrechen veröffentlicht nicht. Nach `accepted`: jede in der Notiz genannte Meldung → `status: completed`, `fixed_in_ver` = neue Version, `fixed_note`, `fixed_at_ms`, `fixed_by` (ctox_bug_reports: `payload.change_summary/fixed_in_ver`). Ohne verknüpfte Meldung: unverändert ohne Blatt |
| 5 Rückmeldung | Meldungs-Badge jedes Fensters der App (`watchFixedReports`, angehängt in `frame/index.js` openApp neben attachSyncState) | für die meldende Person (reporter_id = actor) genau einmal info-Eintrag `shell.report.fixed:<id>` „Deine Meldung #N ist behoben“ mit Aktion „Ansehen“ (Fensterblatt: Titel, Version, Notiz). Ansehen oder stilles Auslaufen (info-Frist der Zone) setzt `reporter_seen_at_ms` → kommt nie wieder |

Nummer `#N`: `report_no`, falls vorhanden, sonst Rang der Meldung in Meldereihenfolge innerhalb der App (stabil, solange nichts gelöscht wird).
Status-Vokabular wie „Bugs & Features“ (open | running | completed | blocked); neue Felder additiv (Schema `additionalProperties: true`).

## Geänderte Dateien

- `v4/shell/report.js` – Schritt 2–5: `appReports`, `reportNo`, `isOpenReport`, `codingPromptFor`, `fixNoteFor`, `notedNos`, `linkReportToTurn`, `markReportsFixed`, `watchFixedReports`, `LOOP_TEXT`, `FIXED_CODE` (FR6b unverändert).
- `v4/source/reports-tab.js` (neu) – Reiter „Meldungen“.
- `v4/source/source-mode.js` – Reiter neben dem Baum, `assignReport`, Versionsnotiz beim Veröffentlichen, Option `sheet`.
- `v4/source/agent-panel.js` – `send(prompt, {record_id})` (Antwort trägt `draft_id`), `focusTurn(turnId)`.
- `v4/source/coding-port.js` – `command(type, payload, opts?)` reicht `record_id` durch.
- `v4/source/source-mode.css` – Reiter + Liste (nur Shell-Tokens).
- `v4/frame/index.js` – nur zwei Stellen: `watchFixedReports` am Badge (openApp), `sheet` an openSourceMode (showCode).
- `v4/frame/lab/frame.test.mjs` – Block `loop()` (LOOP-1…5, `LOOP_ONLY`, `LOOP_MUTATE`); AG-Mutation `stale` an die neue Zeile (`cmdOpts`) angepasst.
- `v4/manifests/shell.manifest.json` – neu erzeugt (Lesen/Schreiben auf beide Melde-Collections war schon deklariert, MAN2; neu nur Dateiliste/Hashes inkl. `reports-tab.js`).

## Abnahme (gemessen)

| Prüfung | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=…/8489b4e LOOP_ONLY=1 node v4/frame/lab/frame.test.mjs stub` | 6/6 (LOOP-1…5 + ohne Seitenfehler) |
| Mutationen `LOOP_MUTATE=melden\|filter\|link\|note\|seen` | alle 5 rot (Tabelle unten) |
| Rahmen-Labor mock | 167/167 |
| Rahmen-Labor stub (8489b4e) | 204/205; einziger roter Fall AG-M (Monaco über `ctox:lib/monaco`, `"monaco":false`) – Bibliotheksauflösung/Labor-Server, `v4/frame/lab/serve.mjs` ist während des Laufs von Strang MONACO geändert; LOOP berührt editor.js/sw.js/serve.mjs nicht |
| Alter Pin 51b1e33 | LOOP-Block entfällt sauber (kein draft.write.result), AG 10/10 |
| Quelltext-Labor `CTOX_APP_API_STUB=…/8489b4e accept.mjs --editor textarea` | ALLE BESTANDEN |
| `node v4/os-primitives/check-caps.mjs --calls` | ABWEICHUNG 0 (202 Aufrufe, OK 195) |
| `node v4/tools/manifest/gen-manifest.mjs --check` | Exit 0 nach Neuerzeugung |
| Screenshots 1280 angesehen | `frame/lab/shots/integrated-stub-loop-{meldungen,auftrag,versionsnotiz,rueckmeldung}-1280.png`: Reiter im Baum in Shell-Optik, Auftrag in der Seitenleiste, Zeile „In Arbeit · Lauf ansehen“, Badge-Liste mit „Deine Meldung #1 ist behoben · Ansehen“ |

Mutationsbelege (je Schritt muss genau der zugehörige Fall rot werden):

| Mutation | Stelle | Ergebnis |
|---|---|---|
| melden | `module_id: win.appId` → `''` (report.js) | LOOP-1 rot (+ LOOP-2, Abbruch) |
| filter | Abfrage-Selektor `module_id` entfernt (reports-tab.js; einziger App-Filter des Reiters) | LOOP-1, LOOP-2 rot (+ LOOP-4) |
| link | `linkReportToTurn(...)` entfernt (source-mode.js) | LOOP-3 rot (+ LOOP-4/5) |
| note | `fixNoteFor` liefert `''` | LOOP-4 rot (+ LOOP-5) |
| seen | `markSeen` schreibt nicht | nur LOOP-5 rot |

## Punkt 5 (Delegation aus „Bugs & Features“) – nicht im Rahmen dieser Dateien lösbar

Befund (Code gelesen, nicht vermutet):
1. Der Dialog sendet `threads.ctox_approval.request` mit `target_payload: { module_id, prompt }`
   (`modules/reports/index.js` openDelegateDialog). Das sitzt in index.js, nicht in `handlers/` – die Handler
   (`_delegation.js`, `reports.delegation.complete/failed`) sind nur der Rückweg und senden nichts.
2. Auch mit korrektem `target_payload` kann der threads-Weg §16.44 nicht erfüllen:
   `threads.ctox_approval.approve.js` enqueueApprovedCommand ergänzt per `orInsert` `instruction`, `user_message`,
   `title`, `thread_key`, `context`, `approval` → Kern lehnt mit `invalid payload_invalid:instruction` ab;
   der Befehl läuft unter dem Akteur der Freigabe, der Entwurf `<user>/<app>` gehört aber dem Anfragenden →
   `forbidden draft_not_owned`; `base_draft_rev` vom Anfragezeitpunkt ist bei Freigabe meist veraltet → `stale_draft_rev`.
3. Apps haben keine Quelltext-API (Entwürfe nur über `kernel.source`, Shell). Der §16.44-Weg für „Bugs & Features“
   ist deshalb: Knopf „An Coding Agent übergeben“ → `api.shell.openSource(report.moduleId)` → Code-Ansicht der Ziel-App,
   Reiter „Meldungen“, „Agent beauftragen“ (= Schritt 3 oben, eigener Entwurf, §16.44).

Übergabe:
- **DARK** (`modules/reports/index.js`): im Delegationsknopf statt `openDelegateDialog` `state.api.shell.openSource(report.moduleId)`
  aufrufen; Anzeige „in Arbeit“/Lauf aus `report.coding_turn_id` statt aus `business_commands … payload.context.record_id`
  (der §16.44-Befehl trägt den Bezug als `record_id` am Befehl, nicht im payload).
- **Rahmen/app-host** (`v4/shell/app-host.js`, nicht LOOP): `api.shell.openSource(appId, {report_id})` durchreichen,
  dann kann die Code-Ansicht direkt im Reiter „Meldungen“ auf der Meldung öffnen (`view.showSide('reports')` existiert).
- **threads**: Ziel `ctox.coding.turn` in `APPROVAL_TARGETS` ist unter §16.44 nicht mehr erfüllbar (siehe 2.) – entfernen
  oder auf „Freigabe → Anfragender beauftragt selbst“ umbauen.

## Offen / Grenzen

- Schmale Breite (< 560 px): der Baum (und damit der Reiter) ist ausgeblendet wie bisher; Meldungen nur ab 560 px.
- Fensterbild wird weiterhin nicht angehängt (FR6b.md).
- „Melder“ zeigt bei fremden Meldungen `reporter_display_name` bzw. die ID: die Shell liest `business_users` nicht.
- Versionsnotiz: der Kern-Vertrag `draft.publish` hat kein Notizfeld; die Notiz steht an den Meldungen (`fixed_note`).

## Runde 2 (16.10.2026) – Meldeschleife verzahnt

Kopf 2f006b168, Stub-Pin `8489b4e`. Kein git außer Lesen.

### Was jetzt gilt

| Punkt | Umsetzung |
|---|---|
| 1 „Bugs & Features“ übergibt an die Code-Ansicht | `modules/reports/index.js`: Kopf-Knopf und Aktionsspalte „An Coding Agent übergeben“ rufen `api.shell.openSource(report.moduleId, {tab:'reports', reportId})` (`codeViewTargetFor`, `openInCodeView`). Delegationsdialog, Reviewer-Auswahl, `threads.ctox_approval.request` und der Snapshot-Teil `delegate` sind entfernt. „In Arbeit“ kommt aus `report.coding_turn_id` (`normalizeReportItems`: offener Status mit `coding_turn_id` → running, `isCodingRunning`). Die Aktionsspalte zeigt dann „Coding Agent · In Arbeit“ und „In der Code-Ansicht öffnen“. Gelesen werden `ctox_task_approval_requests` und `business_users` nicht mehr; die Deklaration `v4-manifest.json` ist entsprechend kleiner. Locales: `delegateCodingRunning` neu, dazu `delegateCodingTitle` und `openCodeView`. |
| 2 Option durchreichen | `v4/shell/app-host.js` `openSource(target, options)`: `sourceFocusOf` prüft `{tab?: 'files'|'reports', reportId?: string}`. Nur `reportId` → `tab:'reports'`. Unbekannte Felder, andere Werte oder ein Nicht-Objekt → `CtoxError invalid`. `v4/source/source-mode.js` nimmt `focus` an (`openSourceMode({…, focus})`, später `handle.focus(f)`): Reiter wählen, Meldung markieren. `reports-tab.js` `mark(id)`: `is-marked` + `aria-current`, einmal in Sicht, wirkt auch bei erst später eintreffenden Zeilen. Typen: `OpenSourceOptions` in `v4/tools/port/app-api.d.ts`. |
| 3 threads ohne Coding-Route | `_approval_targets.js`: `APPROVAL_TARGETS`, `approvalTarget`, `dispatchApprovedTarget` und die coding-agents-Konstanten sind entfernt. Einziges Ziel einer Freigabe ist `business_os.chat.task`. Für `ctox.coding.turn` lehnen `request` und `approve` (Altbestand) mit `invalid` ab, Begründung: Coding-Turns nur aus der Code-Ansicht mit eigenem Entwurf, §16.44. `approve.js` hat nur noch den Agenten-Task-Pfad; die tote Abwicklung synchroner Fremdbefehle (`terminal`/`settleApproval`) ist entfernt. Rückrufe per `command_id` bleiben, damit noch laufende Altbestände zugeordnet werden. Deklaration: `commands_in from:['threads'] ctox.coding.turn` aus `modules/coding-agents/v4-manifest.json` entfernt; die abgeleiteten `commands_out` von threads enthalten nur noch die reports-Rückrufe. |
| 4 Melder-Name | Die Kern-Fähigkeiten `identity.users.list` und `directory.user` (`core-contract.mjs`) sind Handler-Fähigkeiten (`ctx.cap`). Die Shell hat keine Handler und kann sie im Browser nicht aufrufen. Quelle ist deshalb die Nutzerliste der Instanz `business_users` (Eigentümer ctox, Felder `id`/`user_id`/`display_name`), nur lesend. Neu in der Shell-Deklaration `SHELL_FOREIGN` (`gen-manifest.mjs`), Manifeste neu erzeugt. `reports-tab.js` `reporterLabel`: eigene Meldung „Du“, sonst Name aus der Nutzerliste, sonst ein gespeicherter Name, der keine ID ist, sonst „Teammitglied“. Nie eine ID. |

### Geänderte Dateien (Runde 2)

- `modules/reports/index.js`, `test.mjs`, `tests/v4-api.test.mjs`, `handlers/tests/delegation.test.mjs`, `locales/{de,en}.json`, `v4-manifest.json`
- `modules/threads/handlers/_approval_targets.js`, `threads.ctox_approval.approve.js`, `handlers/tests/{targets,approvals}.test.mjs`, `v4-manifest.json` (nur `_basis`-Text)
- `modules/coding-agents/v4-manifest.json`: nur der `commands_in`-Eintrag samt `_basis`. Das ist das Gegenstück der entfernten Route.
- `v4/shell/app-host.js` (`sourceFocusOf`, `openSource(target, options)`)
- `v4/source/source-mode.js` (`focus`, `handle.focus`), `reports-tab.js` (`mark`, `reporterLabel`, `userNames`, `USERS_COLLECTION`), `source-mode.css` (eine Regel `.v4-src__report.is-marked`)
- `v4/tools/port/app-api.d.ts` (`OpenSourceOptions`)
- `v4/tools/manifest/gen-manifest.mjs`: `SHELL_FOREIGN.business_users` (read). Manifeste neu erzeugt.
- **Außerhalb der Liste, unvermeidbar:** `v4/frame/index.js`. Der einzige Weg von `app-host.js` zur Code-Ansicht führt über den Rahmen. Geändert sind fünf Stellen, die nur `focus` durchreichen: `windowShellApi.openSource`, `showCode(rec, focus)` (offene Ansicht → `view.focus`, beim Öffnen → `openSourceMode({focus})`), `openSourceView({focus})` und der Labor-Griff `openSource(appId, focus)`. Eingespielt per exaktem Einzelersatz, nichts von KIT2 berührt.
- `v4/frame/lab/frame.test.mjs`: LOOP-6/7/8. Die echte App `modules/reports` ist im Labor unter `/~a/reports@` eingebunden. Neue Mutationen, auch für Dateien unter `modules/reports/`.

### Abnahme (gemessen)

| Prüfung | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=…/8489b4e LOOP_ONLY=1 node v4/frame/lab/frame.test.mjs stub` | 9/9 (LOOP-1…8 + ohne Seitenfehler) |
| Mutationen Runde 2 (je der zugehörige Fall rot) | siehe Tabelle unten |
| `node test.mjs` (reports) | 10/10 (neu: Übergabeziel, „In Arbeit“ aus coding_turn_id) |
| `node --test tests/v4-api.test.mjs` (reports, Pin 8489b4e und Standard-Pin 68b83a3) | 9/9 und 9/9. Neuer Fall „Übergabe“: openSource-Aufruf mit `['notes',{tab:'reports',reportId:'rep_1'}]`, kein Dialog, kein Befehl, „In Arbeit“ live aus coding_turn_id |
| reports-Handler `node --import ../../v4/tools/node/ctox-shared.mjs --test handlers/tests/*.test.mjs` | 4/4 |
| threads-Handler `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` | 73/73 |
| threads `tests/v4-api.test.mjs` (8489b4e), `threads/detail-generation/search-rescan` | 15/15, 1/1, 2/2, 3/3 |
| `node v4/os-primitives/check-caps.mjs --calls` | ABWEICHUNG 0 (202 Aufrufe, OK 195); `check-caps` OK |
| `node v4/debt/ratchet.mjs` | aus LOOP nichts gestiegen. Einziger gestiegener Zähler: `scan.app.modules/research.R16-css` 0 → 2, fremder Strang, research nicht angefasst. Ein eigener Anstieg (`en-pending-waiting` durch „In progress“) ist behoben: Text jetzt „Working …“ |
| `node v4/tools/manifest/gen-manifest.mjs --check` | Exit 0 nach Neuerzeugung; `gen-manifest.test.mjs` 43/43 |
| Quelltext-Labor `accept.mjs --editor textarea` (8489b4e) | ALLE BESTANDEN |
| Rahmen-Labor komplett: `node v4/frame/lab/frame.test.mjs mock` und `CTOX_APP_API_STUB=…/8489b4e … stub` | mock 167/167, stub 208/208 (Runde 1 hatte 204/205; AG-M ist jetzt auch grün) |
| Screenshot 1280 angesehen | `frame/lab/shots/integrated-stub-loop-uebergabe-1280.png`: „Bugs & Features“ hinten, davor Notizen in der Code-Ansicht mit Reiter „Meldungen 2“. #2 „Spalte ‚Fällig‘ sortierbar“ ist markiert (Balken links, getönter Grund), #1 steht auf „In Arbeit · Lauf ansehen“. `…loop-melder-1280.png`: Melder „Kim Berger“ bzw. „Teammitglied“ |

Mutationsbelege Runde 2:

| Mutation | Stelle | Ergebnis |
|---|---|---|
| uebergabe | `codeViewTargetFor` ohne `options` (reports/index.js) | nur LOOP-6 rot |
| durchreichen | `frame.openSource?.(id)` ohne `focus` (app-host.js) | LOOP-6 + LOOP-7 rot |
| markieren | Markierung in `reports-tab.js` aus | LOOP-6 + LOOP-7 rot |
| name | Name aus der Nutzerliste → ID (`reporterLabel`) | nur LOOP-8 rot |
| rueckfall | Rückfall „Teammitglied“ → ID | nur LOOP-8 rot |

### Offen / Übergabe

- Die Rückruf-Kette `reports.delegation.complete/.failed` (reports-Handler, `APPROVAL_CALLBACKS`, threads `commands_in from:['reports']`) funktioniert weiter und ist getestet. Seit Runde 2 sendet reports aber keine Freigabeanfragen mehr, die Kette hat also keinen Absender. Kandidat zum Entfernen; nicht Teil dieses Auftrags.
- `modules/reports/module.json` (v2-Manifest) nennt `ctox_task_approval_requests` und `business_users` noch unter den Collections und beschreibt die rechte Spalte als „coding-agent delegation“. Nicht angefasst.
- `modules/threads/V4-HANDLERS.md` (THR2-Abschnitt) beschreibt noch `APPROVAL_TARGETS` mit ctox.coding.turn. Nicht in meiner Liste, Nachtrag nötig.
- `gen-manifest.mjs` hat beim Neuerzeugen auch Manifeste anderer Apps (appsec-pentest, documents, mail, matching, research, shiftflow, spreadsheets) neu geschrieben, und zwar wegen Dateiänderungen paralleler Stränge. Das ist reine Generatorausgabe, `--check` gibt 0.
