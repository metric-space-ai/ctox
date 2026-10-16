# threads – Port auf App-API 4.0 (Strang UD2, Welle D)

Auftrag: `~/.local/state/shell-v4/docs/ui-port-brief.md`; Vorlage `v4/tools/port/PORTING.md`; Pilot `modules/knowledge`.
Stub-Pin: `~/.local/state/shell-v4/stub-pins/67229d2` (Port), Nacharbeit UD2b gegen `cff5ba5` (abwärtskompatibel).
Fachlogik: `handlers/` (Strang H2, `V4-HANDLERS.md`) – nicht angefasst.

Fortschritt (gemessen, `TZ=Europe/Berlin date`): 13:40 Brief/PORTING/Stub/Pilot gelesen, Vorher-Aufnahme ·
13:47 `naming.js` · 14:14 `index.js` portiert, CSS gescoped, Markup/Locales ohne Zustandstexte, `schema.js` gelöscht ·
14:18 v4-Test 12/12 grün, Nachher-Aufnahmen gesichtet, Kopf-Geometrie nachgebessert · 14:20 alle Prüfungen grün.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app threads` | vorher 53 Stellen + 169 CSS-Selektoren ungescoped → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs threads modules/threads/index.css --check` | gescoped (11 `.shell-window[data-shell-contract="v2"]`-Vorfahren ersetzt) |
| Wächter | `node v4/tools/guards/guard-v4.mjs --json` | 0 Treffer in `modules/threads/` außerhalb `handlers/` (3 Treffer in `handlers/` gehören Strang H2) |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=…/67229d2 node --test src/apps/business-os/modules/threads/tests/*.test.mjs` | **18/18 grün** (v4-api 12, search-rescan 3, detail-generation 2, threads.test 1) |
| Handler (Strang H2, Gegenprobe) | `cd modules/threads && node --test handlers/tests/*.test.mjs` | 46/46 grün (unverändert) |
| Screenshots | `tests/shots/threads-{before,after}-*.png` | selbst angesehen, siehe workjet#594 |

Abgedeckt von `tests/v4-api.test.mjs` (Chromium headless, `createStubKernel` storage memory, Ops-Fixture
`tests/fixtures/ops-fixture.mjs`): mount zeigt Live-Daten, je Collection genau ein Abo, Titel über `api.shell.setTitle`,
exakte Zähler ohne „≥“ · workjet#594 Name/Kopf/Kennung · Status ohne Abschneiden bei 1280 und 420 (scrollWidth,
`text-overflow`) · `sim.push` erscheint live, Tombstone verschwindet · leere Collections: kein Text, keine Zone ·
Befehl `threads.message.create` mit `record_id`, `sim.failCommand` → genau ein Zoneneintrag, nächster Erfolg räumt ihn ·
Snooze/Ablehnen über `api.shell.dialog` · `api.put` → `accepted` und live sichtbar · snapshot → structured clone →
Remount mit `restored` (Antwortentwurf, Notiz-Entwurf, Empfänger-Auswahl, offenes Aktionsfeld, Auswahl) · unmount löst
alle 11 Abo-Arten und leert root · keine `pageerror`.
**Nicht** abgedeckt: Locale-Wechsel live, `onRoute`-Deep-Link mit unbekannter Kennung, Export-Download, Geometrie-Labor
in drei Breiten, Hot-Plug-Skript.

## Was sich geändert hat (Regeln R1–R17)

- **R1/R2 Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; `ctx.host/left/right` → `root`.
  Modulzustand je mount neu, alle Abos je Montage lokal (`disposers`, Detail- und Nachschlage-Abos mit eigener Generation).
- **R3 Daten**: Live-Abos statt `ctx.db`-Lesen: `user_threads` (alle, sortiert), `user_thread_states` (eigene mit
  Aufmerksamkeit > 0 + Präferenzen – gleiche Auswahl wie vorher), `ctox_task_approval_requests`, `user_notifications`
  (eigene), `business_users` (Personen), `business_apps` (App-Titel und erlaubte Sprungziele statt `ctx.getModules`),
  `workjet_projects` (Projektnamen, OFFEN-2). Nachrichten/Verknüpfungen nur für den ausgewählten Thread (wie vorher);
  `business_commands`/`ctox_queue_tasks` gezielt per `{id:{$in:[…]}}` für Thread-Namen und den ausgewählten Thread.
- **R4/R12 Sync und Schleifen entfernt**: 30-s-Poll (`REALTIME_POLL_MS`), 5-s-Nachlauf, `observe`-Entprellung mit
  10-s-Abkühlung, Readiness-Abos (`collectionReadiness`, `subscribeCollectionReadiness`), Such-Scan in Seiten
  (`scheduleSearchScan`, Generationen, Sichtbarkeitsprüfung) und die 20×500-ms-Nachladeschleife nach einer
  Freigabe-Entscheidung – ersatzlos; die Live-Abos liefern jeden Stand. Einziger Timer: 16-ms-Bündelung des Renderns.
- **R5 Befehle**: `api.command(type, payload, {record_id, client_context})`, nie warten. Typen und payload-Felder exakt
  nach `V4-HANDLERS.md` (23 Typen; die UI sendet 16 davon). `record_id` steht in den Befehlsoptionen (§16.19 Nr. 8),
  nicht im payload – `_lib.js` würde `payload.record_id` als Quellkontext-Rückfall lesen (U-3). `failed`/`local`-Ablehnung →
  `api.status.report({code:'threads.command.<type>'})`, `done` → `clear`. Eingaben werden sofort geleert; das
  „Übermittelt · cmd_…“-Band und `setBusy` entfallen.
- **R6 Shell**: `ctx.presence` (wer sieht den Thread gerade) entfällt – 4.0 hat keine Präsenz-API (OFFEN-3).
  Web-/System-Benachrichtigungen (`new Notification`, `ctx.notifications.showSystem`) entfallen – „nichts außerhalb des
  App-Fensters“ (OFFEN-4).
- **R7 Dialoge**: `window.confirm/prompt` → `api.shell.dialog({kind:'confirm'|'prompt', title, text, default})`; riskante
  Freigabe mit `default:false`.
- **R8 Sprache**: `loadModuleMessages` (HTTP) → eingebettete Texte + `api.shell.t('threads.<key>')`, `locale.subscribe`.
- **R9 Akteur**: `ctx.session.user` → `api.actor.{id, display_name, role}`.
- **R10/R13 Navigation**: `location.hash = …` → `api.shell.navigate(app, params)` (Quell-Objekte, Befehle, Aufgaben);
  eingehend `api.shell.onRoute(ev => ev.params.thread_id)` statt `hashchange`/`ctox-business-os-app-launch`. Unbekannte
  Kennung → Zone `threads.route.unknown_thread` statt Text „Thread nicht verfügbar.“ im Inhalt.
  `storageScope`/`sessionStorage` (Entwurf, Navigation, Benachrichtigungs-Dedupe) → `snapshot()`/`restored`.
- **R11 Zustandstexte entfernt**: „Threads werden synchronisiert.“, „Keine lokalen Treffer · weitere Daten möglich.“,
  „Offline · lokaler Datenstand …“, „Keine Arbeit für dich.“, „Keine Threads in dieser Ansicht.“, „Verlauf wird
  vollständig geladen…“, „Noch keine Nachrichten.“, „Kein verknüpfter App-Kontext.“, „Kein Thread ausgewählt.“,
  „bereit“/„sendet“, Footer-Zusätze „Mindestens …“/„weitere Daten möglich“, `data-sync-state` „Lokaler Datenstand bereit“,
  `role="status"` im Kopf, Locale-Schlüssel `syncingThreads`, `noThreads`, `noSelection`, `refresh`, `presenceViewing`.
- **R14 Importe**: `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(root)` im mount);
  `shared/i18n.js` entfällt.
- **R10/R16 Markup/CSS**: `fetch(new URL('./index.html', import.meta.url))` ohne Cache-Buster, `ensureStyles` entfernt;
  `index.css` unter `[data-app="threads"]`; das App-root ist selbst Container `business-app-window` (v4-Shell setzt keinen,
  Muster intake) – sonst greifen die 1024/768-Umbrüche nicht.
- **R17**: `schema.js` gelöscht. Abgleich mit `collections.schema.json`: 8 Collections, Versionen (2/3/1/0/1/0/0/0),
  `required` und Feldlisten gleich. **Abweichung**: `schema.js` hatte für `ctox_queue_tasks` zusätzlich Migration 3
  (Identität), `collections.schema.json` nur 1 und 2 – fachlich ohne Wirkung, Manifest-Strang möge 3 ergänzen (OFFEN-5).
- **Wächter-Vokabular**: ~~Reiter „Wartet“ → „Bei mir“, Ansichten „Wartet auf mich/andere“ → „Liegt bei mir/anderen“~~
  **KORREKTUR (UD2b)**: Umbenennen war keine Port-Aufgabe; Originalwortlaut wiederhergestellt und mit
  `<!-- v4-allow: label -->` gekennzeichnet, siehe Nachtrag UD2b. Fachvorgangs-Status ausgeschrieben:
  Fehlgeschlagen / In Arbeit / Eingereiht / Abgeschlossen / Blockiert (statt Rohwert `failed`).

## workjet#594 „Threads heißen wie Kennungen“ (Owner-Issue, UI-Audit 13.10., Folie 40)

**Befund vorher** (heutiger Code `git show HEAD:…/index.js`, gegen den Stub-Pin mit der Ops-Fixture, Shell-V2-Geometrie):
Ursache in den Daten: Workjet-Befehle tragen `payload.title = <Befehls-ID>` (vgl. `handlers/tests/projection.test.mjs:52`);
die Projektion übernimmt das als `user_threads.title` **und** als `source_label`. Die UI zeigte `title || id` als Namen,
`source_label` in der Metazeile und `source_module · source_label` als Kicker – daher die Kennung zweimal je Zeile und
zweimal im Kopf. Die Metazeile war einzeilig mit Ellipse („Blockiert · Fe…“), der Kopfstatus absolut positioniert mit
Ellipse; bei 420 war der Name im Kopf gar nicht sichtbar (Aktionsleiste nahm die ganze Breite).

| Vorher 1280 | Nachher 1280 |
|---|---|
| ![](tests/shots/threads-before-1280.png) | ![](tests/shots/threads-after-1280.png) |

| Vorher 420 Liste | Nachher 420 Liste | Vorher 420 Detail | Nachher 420 Detail |
|---|---|---|---|
| ![](tests/shots/threads-before-420.png) | ![](tests/shots/threads-after-420.png) | ![](tests/shots/threads-before-420-detail.png) | ![](tests/shots/threads-after-420-detail.png) |

Kennung in den Details: `tests/shots/threads-after-1280-details.png` (rechte Spalte, Zeile „Kennung“ unter Quelle/Record/Deep Link).

Gemessene Texte (Ops-Fixture, Ansicht Handeln):

| Thread | Vorher (Zeilentext) | Nachher |
|---|---|---|
| Supervisor-Zug | `a0796d32-2ca9-4f1e-9b7a-3c5d8e2f1a46` · Blockiert · Fe… · Blockiert · a0796d32-… | **CTOX Shell v4 · Login-Seite auf App-API 4.0 umstellen und Abnahme fahren** · Blockiert · Fehlgeschlagen · Blockiert |
| Steuerbefehl ohne Prompt | `e41b7c05-…` (dreimal: Titel, Meta, Vorschau) | **ctox.workjet.project.list** · Blockiert · Fehlgeschlagen |
| Supervisor-Thread | `7b2e4d19-3a6c-…` · Workjet Desktop (zweimal) | **Workjet Desktop · Supervisor** · Erwähnung · Offen |
| Fachthread | Angebot Müller GmbH prüfen · Müller GmbH (zweimal) | **Angebot Müller GmbH prüfen** · Freigabe nötig · Offen · Michael Welsch |
| Kopf | Kicker `CTOX · A0796D32-…`, Titel `a0796d32-…`, „Fachvorgang: failed“ | Kicker `CTOX SHELL V4`, Titel `Login-Seite auf App-API 4.0 umstellen und Abnahme fahren`, „Abstimmung: Blockiert · Fachvorgang: Fehlgeschlagen“ |

**Namensregel** (`naming.js`, nur vorhandene Felder; jede Stufe wird übersprungen, wenn sie eine Kennung ist –
UUID-Teil, `cmd_/thread_/msg_/approval_/task_…`, Hex ≥ 16, eine der IDs des Threads/Befehls/der Aufgabe, oder der
Projektions-Rückfall `CTOX Arbeit <id>` – oder dem Projekt gleicht):

- **Projekt**: 1. `workjet_projects[project_id].name`, `project_id` aus `business_commands.payload.project_id`
  (verknüpfter Befehl; Befehl = `source_record_id` bei `source_record_type=command`, sonst `ctox_queue_tasks.command_id`)
  → `metadata.workjet_supervisor.project_id` → `source_record_id` bei `source_record_type=workjet_project`;
  2. `user_threads.source_label`; 3. `business_apps[source_module].title`, sonst `source_module`.
- **Aufgabe**: 1. `user_threads.title` 2. `ctox_queue_tasks.title` (Aufgabe = `source_record_id` bei
  `queue_task|ctox_task|task`, sonst `business_commands.task_id`) 3. `business_commands.payload.title`
  4. `user_threads.source_label` 5. `metadata.source_context.label` 6. `business_commands.payload.prompt` (erste Zeile,
  ≤ 120 Zeichen wie `ctoxThreadTitle` der Projektion) 7. `user_threads.next_step` 8. `business_commands.command_type`
  9. „Supervisor“ (wenn `metadata.workjet_supervisor`) bzw. „CTOX Arbeit“ (`kind=ctox_task`, Wortlaut der Projektion).
- Enthält die Aufgabe das Projekt wörtlich, entfällt das Projekt. Anzeige: Liste `Projekt · Aufgabe` (Projekt gedämpft),
  Kopf Kicker = Projekt, Titel = Aufgabe (Tooltip = ganzer Name). Suche trifft zusätzlich den angezeigten Namen.
- **Status**: Aufmerksamkeitsgrund und Metazeile brechen um (`white-space: normal`, `flex-wrap`), keine Ellipse; der
  Kopfstatus steht in Band-Zeile 2 des Timeline-Kopfs im Fluss (Ruhezustand weiter 2 × 37 px, nur ein umbrechender
  Status streckt ihn). Im Einspalter (≤ 768) stehen Projekt/Aufgabe/Status unterhalb der 74-px-Ikonenzone in voller
  Breite und umbrechend.
- **Kennung nur in den Details**: Zeile „Kennung“ (Thread-ID, Monospace) in der Kontextspalte; Quelle/Record/Deep Link
  dort unverändert. Keine Kennung mehr in Zeile, Vorschau, Metazeile, Kopf oder `data-context-label`; Quell-Links in
  Nachrichten zeigen statt einer Kennung das Quellmodul.

## Befehlstypen – Einstufung (§16.13)

Alle 23 `threads.*`-Typen sind **Fachlogik** und als JS-Handler portiert (Strang H2, `V4-HANDLERS.md` §1, mit
Rust-Ort und Tests). Die UI sendet 16 davon: message.create, note.create, notification.mark_read/dismiss/
preferences.update, ctox_approval.request/approve/reject/edit, ai.request, handoff.create, thread.claim/watch/unwatch/
snooze/archive. Nicht von der UI gesendet: thread.assign (intern), ctox_approval.cancel, note.update/delete,
thread.state.update, link.create/remove. Rust-Bezüge und Fähigkeitswünsche (W-1…W-5, `cap.directory/policy/audit/
workjet`) stehen dort; dieser Strang fügt keine hinzu.

## OFFEN

1. **Befehlsoptionen**: `api.command(type, payload, {record_id, client_context})` – `app-api.d.ts` kennt nur zwei
   Parameter; der Stub übernimmt `record_id` (§16.19 Nr. 8), `client_context` geht verloren (V4-HANDLERS W-1b).
2. **Fremd-Collection `workjet_projects`** (Projektnamen, Eigentum ctox): braucht `foreign_collections: {workjet_projects:
   read}` im threads-Manifest (Manifest-Strang). Ohne Freigabe fängt die App `forbidden` ab und benennt über die
   Stufen 2/3 (Quelllabel, App-Titel); gemessen nur gegen den Stub, der Fremd-Collections nicht sperrt.
3. **Präsenz** („👁 Anna sieht sich diesen Thread an“) entfällt mangels 4.0-API.
4. **Benachrichtigungen außerhalb des Fensters** (Web-Notification, Decision-Hub-Systemhinweis) entfallen; braucht einen
   Shell-Kanal, falls gewünscht (Owner).
5. `ctox_queue_tasks`-Migration 3 (Identität) fehlt in `collections.schema.json`; `v4/manifests/reports.manifest.json`
   verweist noch auf `modules/threads/schema.js` (gelöscht) – Manifest-Strang.
6. **Zähler „AI arbeitet“/„Blockiert“** folgen wie vor dem Port nur den Befehlen/Aufgaben des ausgewählten Threads
   (Detaildaten nur für die Auswahl). Mit Live-Abos wäre ein vollständiger Zähler billig – Fachentscheidung, nicht Port.
7. **Mehrfenster**: Modulzustand ist modulweit (`multi_instance: false` im Manifest); zwei gleichzeitige Montagen teilen
   ihn (PORTING §4 Falle 1, wie knowledge OFFEN-4).
8. `paging.js` (`collectUniquePages`) hat keinen Nutzer mehr in der App; nur noch durch `threads.test.mjs` geprüft.
9. ~~Stufe-8-Name `ctox.workjet.project.list` roh~~ – erledigt in UD2b über die Übersetzungstabelle (Owner-Auftrag).
   Ein sprechender `payload.title` beim Absender (Workjet) bliebe trotzdem die bessere Quelle (Stufe 3 vor Stufe 8).

## Dateien

Geändert: `index.js` (Port), `index.css` (Scope, Container, workjet#594), `index.html` (Zustandstexte/Status-Band),
`locales/de.json`, `locales/en.json`, `tests/threads.test.mjs`, `tests/search-rescan.test.mjs`,
`tests/detail-generation.test.mjs` (auf v4-Vertrag umgestellt). Neu: `naming.js`, `V4-PORT.md`, `tests/v4-api.test.mjs`,
`tests/_v4-harness.mjs`, `tests/fixtures/ops-fixture.mjs`, `tests/shots/threads-{before,after}-*.png` (7).
Gelöscht: `schema.js`. Vorher-Aufnahme: Wegwerfskript außerhalb des Repos (heutiger Code aus `HEAD` über eine reine
Test-Hülle ctx → api, nur lesend).

## Nachtrag UD2b – Nacharbeit (15.10., Stub-Pin `cff5ba5`)

Fortschritt (gemessen, `TZ=Europe/Berlin date`): Wiederaufnahme nach Sitzungsabbruch; Halb-Edits aus dem
abgebrochenen Lauf geprüft und übernommen · 15:06 Tests grün, Aufnahmen neu · 15:10 angesehen.

1. **Beschriftungen im Originalwortlaut** (Port ändert keine Beschriftung): Reiter „Wartet“ (`data-pg-band="waiting"`),
   Sekundäransicht „Wartet auf andere“ (`delegated`), beide mit `<!-- v4-allow: label -->` in der Vorzeile. Alle
   Reiter-/Ansichtsbeschriftungen in `index.html` sind byte-gleich zu `69af11dae` (Vergleich der Band-/Option-Texte).
   „Wartet auf mich“ stand vor dem Port nur in `FILTER_LABELS` für die Fußzeile – mit der Fußzeile entfällt es (Punkt 2).
2. **Keine Zähl-/Zustandszeile** (Owner-Regel): Fußzeile „4 Threads · Jetzt handeln · als Michael Welsch“ entfernt
   (`<footer class="threads-footer">`, `data-pg-footer`, `setFooter`, `FILTER_LABELS`, CSS `.threads-footer`, vierte
   Grid-Zeile von `.threads-left`). Die Tab-Zähler im Band bleiben (Navigation). Weitere Zähl-/Zustandszeilen außerhalb
   der Zone: keine gefunden (Zustandstexte waren schon im Port entfernt, R11).
3. **Kein technischer Rückfall-Name**: Stufe 8 (`business_commands.command_type`) erscheint nie roh. `naming.js`
   (`COMMAND_TYPE_NAMES`, `commandTypeName`) leitet Quelle + sprechende Aktion aus einer festen Tabelle ab; Treffer nur an
   Segmentgrenzen, das längste Präfix gewinnt. Die Quelle ersetzt nur ein Projekt aus Stufe 3 (App-Titel/Modul) –
   ein echter Projektname oder ein `source_label` bleibt. Ohne Tabellentreffer: keine Ableitung, weiter mit Stufe 9
   bzw. leer (nichts erfunden). Alle Präfixe sind Befehlstypen, die im Repo vorkommen (`grep -rhoE "ctox\.workjet\.[a-z_.]+" src`).

| Präfix | Quelle | Ableitung (de) | Ableitung (en) |
|---|---|---|---|
| `ctox.workjet.project.supervisor.turn` | Workjet | Supervisor-Zug | Supervisor turn |
| `ctox.workjet.project.supervisor` | Workjet | Supervisor | Supervisor |
| `ctox.workjet.project.list` | Workjet | Projektliste | Project list |
| `ctox.workjet.project.kpis` | Workjet | Projektkennzahlen | Project KPIs |
| `ctox.workjet.project.chat` | Workjet | Projektchat | Project chat |
| `ctox.workjet.project.worker` | Workjet | Projekt-Worker | Project workers |
| `ctox.workjet.project` | Workjet | Projekt | Project |
| `ctox.workjet.session.transfer` | Workjet | Sitzungsübergabe | Session transfer |
| `ctox.workjet.session` | Workjet | Sitzung | Session |
| `ctox.workjet.computer` | Workjet | Rechner | Computer |
| `ctox.workjet.providers` | Workjet | Anbieter | Providers |
| `ctox.workjet.jour_fixe` | Workjet | Jour fixe | Jour fixe |
| `ctox.workjet` | Workjet | – (nur Quelle) | – |

Gemessen (Ops-Fixture): Thread t2 vorher `ctox.workjet.project.list`, jetzt **Workjet · Projektliste**.

| Prüfung | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test modules/threads/tests/v4-api.test.mjs` | 12/12 grün (prüft Reiter „Wartet“, keine Fußzeile/Zählzeile, „Workjet · Projektliste“, kein `ctox.workjet.` in Zeilen) |
| dto. `modules/threads/tests/*.test.mjs` | 18/18 grün (`threads.test.mjs`: Fußzeile/`FILTER_LABELS` weg, `v4-allow`-Kennzeichnung, Tabelle inkl. Segmentgrenze und unbekanntem Typ) |
| `node --test handlers/tests/*.test.mjs` (Gegenprobe, unverändert) | 46/46 grün |
| `node v4/tools/guards/guard-v4.mjs --json` | 0 Treffer in `modules/threads/` (ohne Tests, inkl. `handlers/`) |
| Aufnahmen `tests/shots/threads-after-{1280,1280-details,420,420-detail}.png` | neu, 1280 und 420 selbst angesehen: Reiter „Wartet (1)“, keine Fußzeile, „Workjet · Projektliste“ |

Geändert in UD2b: `index.html`, `index.js`, `index.css`, `naming.js`, `tests/threads.test.mjs`, `tests/v4-api.test.mjs`,
`tests/shots/threads-after-*.png` (4), `V4-PORT.md`.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Nachrichtenvorschau in der Liste bleibt einzeilig, jetzt mit `data-v4-allow-ellipsis` (Vorschau, fachlich gewollt).

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app threads --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/threads/tests/v4-api.test.mjs` grün; `scan.mjs --app threads` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
