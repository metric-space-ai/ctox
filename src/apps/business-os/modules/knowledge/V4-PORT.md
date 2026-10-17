# knowledge – Port auf App-API 4.0 (Strang K, Pilot)

Fortschritt (inkrementell, Berlin-Zeit):
- 12:38 Start, Doku gelesen (Arch §10.2/§10.2a/§16.10/§16.13/§16.15, Entwurf §5–§7/§11/§12, C2 knowledge, B-shared §5).
- KORREKTUR: die folgenden drei Zeitstempel (12:53/12:58/13:18) waren geschätzt, nicht gemessen; gemessen
  (`TZ=Europe/Berlin date`) lagen Start 12:37, Testlauf 1 12:52, Testlauf grün 13:01.
- (12:4x) Gerüst angelegt.
- (12:4x) app-api.d.ts erste Fassung (Abgleich mit Stub makeAppApi).
- (12:5x) index.js portiert (mount/unmount/snapshot, api.query-Abos, api.command, Drawer im Fenster), index.css gescoped (41 .shell-window-Vorfahren ersetzt), Locales ohne Zustandstexte.
- 12:52 Koordinator: Test gegen createStubKernel statt eigener Attrappe, app-api.d.ts nach Stub index.d.ts.
- 12:55 Befund: `ctox:shared/` per Import-Map in Chromium nicht auflösbar (CORS-Block) → Testserver schreibt um wie der SW (§16.15).
- 13:00 Befund: zweite Montage derselben Modulinstanz verlor die Abos der ersten (3 verwaiste Abos gemessen) → Abos je Montage lokal; OFFEN-4.
- 13:01 v4-api.test.mjs gegen createStubKernel: 8/8 grün.
- 13:03 app-api.d.ts = wörtlich Teil 1 von stub/index.d.ts + OFFEN-Liste.
- 13:10 Legacy-Suite test.mjs auf v4-Vertrag umgestellt: 30/30 grün; Scanner knowledge 98 → 0 Stellen; SCAN-BASELINE erzeugt.
- Strang K2 (E11, gemessene Zeiten): 13:29 Start nach Lesen (E11 in entscheidungen.md, Arch §16.13/§16.16/§16.20,
  rxdb_peer_knowledge_rows.rs); Baseline v4 8/8, Legacy 30/30, Scanner 0. 13:33 Handler + Attrappe 9/9 grün.
  13:36 v4-Test 10/10 (zwei neue E11-Tests), dreimal wiederholt grün. 13:38 Legacy 30/30, Scanner 0, CSS gescoped.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/knowledge/tests/v4-api.test.mjs` | 10/10 grün (K2; vorher 8/8; Chromium headless, createStubKernel storage:memory) |
| Trigger-Handler E11 | `node --test 'src/apps/business-os/modules/knowledge/handlers/tests/*.test.mjs'` | 9/9 grün (In-Memory-ctx, simulierte `cap.dataframe.window`) |
| Bestehende Suite | `node modules/knowledge/test.mjs` (aus Kopie mit node_modules-Link, Klon hat keine node_modules) | 30/30 grün |
| Port-Scanner | `node v4/tools/port/scan.mjs --app knowledge` | 0 Stellen, Exit 0 (vor dem Port: 98 Stellen, CSS 303 Selektoren ungescoped) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs knowledge modules/knowledge/index.css --check` | gescoped |

Abgedeckt vom v4-Test: mount zeigt Daten · Live-Änderung per `sim.push` erscheint, Löschung verschwindet · leere
Collections: kein Text, keine Zone · `api.put` → `accepted`, erscheint live · Befehl `ctox.knowledge.document.modify`
legt `business_commands`-Dokument an, `sim.failCommand` → genau ein Zoneneintrag · Löschen fragt über
`api.shell.dialog` (ohne Ja kein Befehl, mit Ja `knowledge.entry.delete` → done) · snapshot → structured clone →
Remount mit `restored` erhält Editor-Entwurf und Auswahl · unmount löst alle Abos und leert root.
Seit K2 zusätzlich (E11): Katalog-Tabelle gewählt → `knowledge_views`-Dokument per `api.put` (Pflichtfelder laut
Schema) · leeres Fenster: Kopf ohne Zeilen, kein Lade-/Zustandstext, keine `knowledge.table.*`-Zone · simulierte
App-Laufzeit ruft den echten Trigger-Handler, eingespielte Zeilen erscheinen live (120 von 300) · Blättern und Filtern
aktualisieren **dasselbe** View-Dokument, Zeilen werden ersetzt (121–240, Filter 43 Treffer, leerer Treffer ohne Text)
· `knowledge_table_rows` genau ein Abo, nach `unmount` null.
**Nicht** abgedeckt: Runbook-Ausführung, Kontextmenü/Chat,
Deep-Link über `onRoute`, Locale-Wechsel, Geometrie-Labor, Hot-Plug-Skript.

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`. Modulzustand wird je mount zurückgesetzt; Abos
  je Montage lokal (Befund 13:00).
- **Daten**: drei `api.query(<coll>, {sort:[{updated_at_ms:'desc'}]}).subscribe` statt `ctx.db`-Lesen,
  `$.subscribe`, Readiness-Abos, `startCollection`-Demand-Loader und Retry-Kette 750…8000 ms. Entprellt 80 ms.
- **Zustandstexte** entfernt (index.js, index.html, locales): „Knowledge wird geladen“, „Daten werden synchronisiert.“,
  „Zeilen werden geladen“, „Command angelegt …“, „Sende…“, Leer- und Hinweistexte. Über `api.status.report` gehen:
  `knowledge.command.<type>` (failed/lokal ungültig), `knowledge.table.incomplete`, `knowledge.table.render`,
  `knowledge.render`, `knowledge.runbook.not_executable`, `knowledge.route.unknown_domain`.
- **Befehle**: `api.command(type, {...payload, record_id, module, client_context})`, nie warten; UI schaltet lokal weiter.
- **Shell**: `setTitle('Knowledge')`; Drawer im eigenen root (`openDrawer`); Bestätigung über `api.shell.dialog`;
  Sprung in die CTOX-App über `api.shell.navigate('ctox', {command_id, …})` statt `location.hash` + `sessionStorage`;
  Deep-Link-Eingang über `onRoute` (`params.id`, `params.domain`) statt `sessionStorage`; Sprache über
  `api.shell.locale` + `t('knowledge.<key>')` mit eingebetteten Rückfalltexten; Rechte über `api.actor`.
- **Imports**: `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js`; `autoWirePaneGrammar(root)` ruft die App selbst.
  `shared/i18n.js` (HTTP-Locales) entfällt.
- **Markup/CSS**: `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`; `ensureStyles` entfernt (Shell lädt CSS);
  index.css unter `[data-app="knowledge"]` gescoped, 41 `.shell-window[data-shell-contract="v2"]`-Vorfahren ersetzt.
- **schema.js gelöscht**: inhaltlich gleich zu `collections.schema.json` (4 Collections, Versionen 2/0/0/0, gleiche
  `required`, Migration business_commands 1 = set_from_first_truthy inbound_channel/module, 2 = leer).

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen mit `grep -rn '"<type>"' src/core` (nur .rs) im Klon:

| Befehlstyp | Rust-Treffer | Einstufung | Ziel |
|---|---:|---|---|
| `knowledge.entry.delete` | 0 | Fachlogik | Handler `knowledge.entry.delete`: Eintrag in `knowledge_items`/`knowledge_runbooks` als gelöscht markieren; heute ohne eigenen Rust-Code (vorher fire-and-forget, Ergebnis unbekannt) |
| `ctox.knowledge.document.modify` | 0 | Fachlogik + Fähigkeit | Handler baut Queue-Auftrag (Titel/Anweisung/Markdown) → generische Fähigkeit `ctx.cap.queue.enqueue({title, instruction, thread_key, context})` (Agent-Lauf) |
| `ctox.knowledge.runbook.modify` | 0 | Fachlogik + Fähigkeit | wie oben |
| `ctox.knowledge.runbook.execute` | 0 | Fachlogik + Fähigkeit | wie oben (`required_output`, `priority`, `thread_key: business-os/knowledge`) |
| `ctox.knowledge.book.create` | 0 | Fachlogik + Fähigkeit | wie oben |
| `ctox.knowledge.book.import` | 0 | Fachlogik + Fähigkeit | wie oben; Quelle URL/Ordner lesen = Fähigkeit des Agenten, nicht der App |
| `ctox.knowledge.book.export` | 0 | Fachlogik + Fähigkeit | wie oben |
| `ctox.knowledge.chat` | 0 | Fachlogik + Fähigkeit | wie oben (Kontext-Chat, Modus data) |
| `ctox.business_os.app.modify` (Kontext-Chat Modus app, nur mit `apps.source.edit`) | 26 | Core-generisch | bleibt app-unabhängig im Core (App-Authoring), kein knowledge-Handler |

Die 7 `ctox.knowledge.*`/`knowledge.*`-Typen haben heute keinen app-spezifischen Rust-Code; sie laufen über den
generischen Weg Befehl → Queue-Task. Die Fähigkeit `ctx.cap.queue.enqueue` ist ein Wunsch an „ctox sync v4“ (Name und
Signatur Vorschlag, nicht im Vertrag).

**Rust `src/core/business_os/rxdb_peer_knowledge_rows.rs` (113 Zeilen, gemessen `wc -l`)**: registriert für
`knowledge_tables` eine Parquet-Zeilenquelle für `rxdb.rows.fetch` (`knowledge_rows_window(root, table_id, offset,
limit)` → `crate::knowledge::knowledge_table_row_window`). Einstufung: **Fähigkeit** (Dateiformat Parquet lesen),
app-unabhängig zu benennen, z. B. `ctx.cap.dataframe.window({ref, offset, limit}) → {rows, row_count, content_hash,
schema_hash}`. Die Registrierung unter dem Collection-Namen `knowledge_tables` ist app-spezifisch und verstößt gegen
die Trennregel → entfällt. Weitere knowledge-Bezüge im Core laut C2 (~70 in rxdb_peer.rs, 11 in importer.rs) hier
nicht neu gezählt.

## OFFEN (nummeriert, für Kern bzw. Board)

1. ~~Tabellenzeilen~~ **erledigt durch E11 (Strang K2)**, siehe Abschnitt „Katalog-Tabellenzeilen (E11)“. Rest:
   1a. Laufzeit muss `ctx.cursor` je Trigger und `change.seq` liefern (Handler toleriert beides fehlend).
   1b. Die `knowledge_tables`-Projektion soll `dataframe_ref` tragen (app-unabhängiger Verweis für
       `cap.dataframe.window`); bis dahin baut der Handler `knowledge-table:<table_id>` – die Fähigkeit muss diesen
       Rückfall auflösen (heute `crate::knowledge::knowledge_table_row_window`).
   1c. Filter/Sortierung kann die Rust-Vorlage nicht (nur offset/limit): CAP-2 muss `filter.text` und `sort` auf
       Parquet umsetzen; `row_count` = Zeilen nach Filter.
   1d. Eine View je Nutzer und Tabelle (`kv:<actor>:<table>`): zwei Fenster desselben Nutzers auf dieselbe Tabelle
       teilen das Fenster (deckt sich mit OFFEN-4).
2. **Manifest**: `module.json` nicht geändert (Strang v4/manifests): `business_commands` steht noch in `collections`;
   `knowledge_table_rows`, `handlers`, `capabilities_required`, `styles` (index.css lädt die Shell) fehlen.
3. **Shell-Lücken 4.0**: keine Icon- und Drawer-API (OFFEN-D in app-api.d.ts); `dialog`-Form offen (OFFEN-A);
   `t()`-Schlüsselraum offen (OFFEN-B) – knowledge nutzt `knowledge.<key>` mit Rückfall.
4. **Mehrere Fenster**: Modulzustand ist modulweit; zwei gleichzeitige Montagen derselben Modulversion teilen
   `state`/`els` (die zweite übernimmt die Darstellung). Leck ist behoben (Abos je Montage), echte Mehrfenster-Fähigkeit
   braucht Zustand je Instanz (index.js hat 3.639 Zeilen mit modulweitem `state`; nicht in diesem Strang).
5. **pane-grammar**: `autoWirePaneGrammar` ist idempotent und wird jetzt von der App gerufen; ob die v4-Shell es
   zusätzlich ruft, ist Sache des Shell-Strangs.
6. Kein git benutzt bis auf einen **lesenden** `git archive HEAD` (Vorher-Zählung des Scanners in den Scratchpad).
   K2: nur lesend `git branch --show-current` und einmal `git diff --stat` auf collections.schema.json.

## Katalog-Tabellenzeilen (E11, Strang K2)

Entscheidung E11 (Kern): `knowledge_table_rows` ist die Projektion eines Sichtfensters. Rohdaten bleiben in Parquet.

**Ablauf**: Tabellenwahl, Blättern (`prev-rows`/`next-rows`), Filter (Suchfeld `[data-table-filter]` in der
Tabellenleiste, 250 ms entprellt) und Sortierung (Klick auf Spaltenkopf: aus → asc → desc) schreiben
`knowledge_views {id:'kv:<actor>:<table>', table, filter:{text?}, sort:[{<spalte>:'asc'|'desc'}], skip, limit:120,
actor_id, updated_at_ms}` per `api.put` – nie warten; `local`-Ablehnung bzw. `rejected` → Zone
`knowledge.table.view`. Geschrieben wird nur bei geändertem Fenster. Die App liest ausschließlich
`api.query('knowledge_table_rows', {selector:{view_id}, sort:[{row_index:'asc'}]})` und zeigt den lokalen Stand:
nach einem Fensterwechsel bis zum Umlauf das vorige Fenster (Kopfzeile „Zeilen a–b“ aus den lokalen `row_index`),
leeres Fenster = Kopf ohne Zeilen bzw. ohne Spalten gar nichts – nie ein Lade-Text (dazu entfiel „Keine Spalten“).
Wechsel auf eine andere Katalog-Tabelle setzt Filter/Sortierung/Offset zurück. `tableFilter`/`tableSort` gehen in
`snapshot()`. Nebenbefund behoben: das Live-Neurendern der Zeilen prüfte `activeTab === 'table'`, der Reiter heißt
aber `data` – eingespielte Zeilen erschienen vorher nie ohne weitere Datenänderung.

**Handler** `handlers/knowledge.view.window.js` (Vertrag §16.16, Trigger §16.20): nur `type:'trigger'` auf
`knowledge_views` insert|update. Überspringt erneute Zustellung (`ctx.cursor` `{seq}`), überholte Stände (aktuelles
View-Dokument per `ctx.get`) und unveränderte Fenster (`window_key` + `content_hash`). Katalog aus `knowledge_tables`
(`table:<id>`/`<id>`/`logical_table_id`/`table_id`); fehlt er oder meldet die Fähigkeit `not_found` → Fenster leeren
(Rust: `ROWS_TABLE_NOT_FOUND`). Andere Fehler gehen an die Laufzeit (wiederholbar, lokaler Stand bleibt, Cursor rückt
nicht vor). Fähigkeitsaufruf außerhalb, Ersetzen der Zeilen (remove alt + put neu) in **einer** `ctx.transaction`.
Zeilendokument: `{id:'<view_id>#<row_index>', view_id, table_id, actor_id, row_index (absolut = skip+i), values,
row_count, window_key, content_hash, schema_hash, updated_at_ms}`.

**Fähigkeit (Wunsch CAP-2, app-unabhängig)**, abgeleitet aus `rxdb_peer_knowledge_rows.rs` (`knowledge_rows_window(root,
table_id, offset, limit) → RowsWindow{rows,row_count,content_hash,schema_hash}`):
```js
const r = await ctx.cap.dataframe.window({ ref: 'knowledge-table:kdt-measured', filter: { text: 'peak' },
  sort: [{ rpm: 'desc' }], offset: 0, limit: 120 });
// → { rows: [{row_id:'MLP-0001', rpm:1000, …}], row_count: 43, content_hash: '…', schema_hash: '…' }
// unbekannt/archiviert → throw {code:'not_found'}; sonst {code:'unavailable'} (wiederholbar)
```

**Manifest-Ergänzung** (Strang v4/manifests übernimmt; `module.json` hier nicht geändert):
```json
{
  "collections": {
    "knowledge_views":      { "class": "window", "client_limit": 200 },
    "knowledge_table_rows": { "class": "window", "client_limit": 5000 }
  },
  "row_filters": {
    "knowledge_views":      { "op": "eq", "path": "/actor_id", "value": { "kind": "actor_id" } },
    "knowledge_table_rows": { "op": "eq", "path": "/actor_id", "value": { "kind": "actor_id" } }
  },
  "triggers": [
    { "handler": "handlers/knowledge.view.window.js", "on": { "collection": "knowledge_views", "op": "insert" } },
    { "handler": "handlers/knowledge.view.window.js", "on": { "collection": "knowledge_views", "op": "update" } }
  ],
  "capabilities_required": ["dataframe.window"]
}
```
Schemas beider Collections stehen in `collections.schema.json` (Version 0). Klasse `window` + Zeilenfilter eigener
Nutzer: beide Collections tragen `actor_id`; die Zeilen erbt der Handler vom View (`actor_id` des Views). Die
`client_limit`-Werte sind Vorschlag (120 Zeilen × Tabellen je Nutzer); Byte-Budget legt der Kern fest.
`row_filters` verwendet die native typisierte `RowFilter`-Grammatik. `ActorId` bindet der Kern an den authentifizierten
Nutzer; `{ "actor_id": "$self" }` ist keine gültige Laufzeitregel und wird bei Veröffentlichung abgewiesen.

**Tests**: `handlers/tests/knowledge.view.window.test.mjs` (9) gegen `handlers/tests/_ctx.mjs` (In-Memory-ctx mit
Schema-Prüfung, `cursor`, `transaction`) und `handlers/tests/_dataframe-sim.mjs` (simulierte Fähigkeit, auch im
Browser-Test genutzt); `tests/v4-api.test.mjs` +2 Tests mit simulierter App-Laufzeit (Handler echt, Schreiben per
`sim.push`). Visuell nur im Testbild geprüft (ohne Shell-CSS): Suchfeld sitzt links der Pager-Knöpfe.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- Senkrechte Linie durch den Inhalt: `.ctox-column-resizer::before` (left: 50%) hatte keinen positionierten Bezug und lief bei 50 % des Moduls; Resizer jetzt `position: relative`.
- Unlesbare Reiter: fest verdrahtete Dunkelpalette (#10151c/#eef3f7 …) im Pilot-Block entfernt; das Modul nutzt die Theme-Token-Aliasse, Reiter aus base.css sind wieder lesbar.
- Gruppentitel „Budgetplanung Vertrieb S D“: Titel kam aus dem Slug; `domainTitleFor` nimmt den Originaltext (Umlaute bleiben).
- G4: Listentitel und Metazeilen brechen um.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app knowledge --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/knowledge/tests/v4-api.test.mjs` grün; `scan.mjs --app knowledge` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

`dataframe.window({table, skip, limit})` – `table` = Katalog-`dataframe_ref` bzw. Tabellen-id ohne `table:`. Der Kern
nimmt nur leere filter/sort: ohne Filter/Sortierung liest der Handler genau sein Fenster; sonst lädt er die Tabelle
seitenweise (≤ 1000 je Aufruf, Deckel 100 000 Zeilen → quota), filtert (Volltext, case-insensitiv) und sortiert
(mehrstufig, stabil) selbst und schneidet das Fenster (`readWindow`). Simulation `_dataframe-sim.mjs` verhält sich wie der Kern.


## App-Skills und Automatisierung (Strang SKL-B2, 15.10.2026, §16.30/§16.34)

Vorher: 7 Typen `ctox.knowledge.*` ohne Handler, Anweisungs-Literale in index.js (:2250, :2299, :2427, :2463, :2504;
:2778 im toten `dispatchContextAction`), Phantom-Skill `knowledge` aus der Core-Heuristik, kein Abschluss (Agent schrieb
selbst in den Core-Knowledge-Store, R6).

Nachher:

| Befehl (Knopf) | Template | Handler |
|---|---|---|
| `ctox.knowledge.document.modify` (Markdown speichern) | `prompts/document-modify.md` | `handlers/ctox.knowledge.document.modify.js` |
| `ctox.knowledge.runbook.modify` (Runbook speichern) | `prompts/runbook-modify.md` | `…runbook.modify.js` |
| `ctox.knowledge.runbook.execute` (Runbook ausführen) | `prompts/runbook-execute.md` (`{{runbook_instruction}}` = Runbook-Text) | `…runbook.execute.js` |
| `ctox.knowledge.book.create` / `.import` / `.export` (Drawer) | `prompts/book-create.md` / `book-import.md` / `book-export.md` | je eine Datei |
| `ctox.knowledge.chat` (Kontext-Chat, Modus data) | `prompts/chat.md` (`{{user_message}}`) | `…chat.js` |

Alle über `handlers/_lib/agent.js`: Template = bisheriger Text wörtlich + Befehlsblock/Payload-/Kontext-JSON wie
`store.rs::command_prompt` (:27009, sensible Schlüssel entfernt :27126, JSON-Vorschau 32 000 Zeichen :205/:27708),
Skill `knowledge/knowledge-maintenance` (`skills/knowledge-maintenance/SKILL.md`, neu aus dem heutigen Verhalten),
`thread_key` Standard `business-os/knowledge`, `on_complete:'knowledge.task.complete'`, `on_failed:'knowledge.task.failed'`.
Der Phantom-Name `knowledge` kommt in der App nicht mehr vor. Die UI sendet keine Anweisungs-Literale mehr
(`runbook.execute` sendet den Runbook-Text als `runbook_instruction`); `dispatchContextAction` war ohne Aufrufer und ist
entfernt. Kontext-Chat Modus app (`ctox.business_os.app.modify`) bleibt Core-generisch.

Neue eigene Collection **`knowledge_tasks`** (Schema seit KNOW2 nur in `v4/schemas/knowledge.collections.schema.json`,
Version 0): ein Dokument je Agenten-Auftrag {id=task_id, command_id, command_type, record_id, title, status
queued|completed|failed|cancelled, summary, result, error, acknowledged_at?, acknowledged_by?}. Zustandszone: siehe KNOW2.

Rückrufe (`handlers/_lib/callback.js`, idempotent): `knowledge.task.complete` liest `knowledge/writeback.json`
(Schema `ctox.knowledge.writeback.v1`), prüft fail-closed und schreibt in **einer** Transaktion `knowledge_items` /
`knowledge_runbooks` (Upsert, bestehende Payload bleibt, `last_task_id`) und `knowledge_tasks` completed; ungültig →
nichts angewendet, Auftrag `failed` (`invalid_result`). `knowledge.task.failed` → `failed` bzw. `cancelled`.
Rust-Vorbild der Auswertung: keines (store.rs:10312-10411 hatte keinen Zweig für ctox.knowledge.*), Aufnahme nach
store.rs:26700-26790.

Tests (aus `handlers/`): `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` → 15/15
(`tests/skills.test.mjs` 6). Mutationsbeleg 8 Mutationen knowledge rot. v4-api 10/10, Legacy 30/30.

OFFEN: Manifest (Strang v4/manifests): `knowledge_tasks` in `collections`; `skills`/`prompts` erzeugt der Generator.
Kern-Abbau: `suggested_skill_for_command` Zweig `contains("knowledge"|"runbook"|"skillbook")` (store.rs:27751-27755)
und der generische Queue-Weg für die 7 Typen.

## KNOW2: Knowledge Books anderer Apps, Schemaquelle, Fehler in der Zone (15.10.2026)

Entscheidung (Orchestrator): Wissensdaten gehören der App knowledge; Apps greifen untereinander nur über deklarierte
Befehle (`ctx.command`) oder `foreign_collections` + `exports` zu.

### 1. Deklarierter Eingang `knowledge.book.ingest`

`handlers/knowledge.book.ingest.js` + `handlers/_lib/book.js`. Kein UI-Sender; heutiger Sender
`research.knowledge.refresh.complete` (siehe `modules/research/V4-HANDLERS.md`, Abschnitt KNOW2).

- Payload `{book:{ref}, knowledge_domain, knowledge_version_id?, title?, record_id?, source:{app, …}}`; `book.ref` =
  files-ref (BlobRef, Kern §16.32). Ohne `book.ref` → `invalid`.
- Datei über `ctx.files.get(ref)` (≤ 4 MiB, UTF-8), Format **`ctox.knowledge.book.v1`** (eigenes Format der App):
  `{schema_version, knowledge_domain, knowledge_version_id?, title?, summary?, elements:[{id ≤180, kind
  skillbook|skill|resource|runbook, title, markdown?, summary?, citations?[]}], gaps?, provenance?}`. Prüfung fail-closed
  (Schema, Domäne = Befehl, Kinds, eindeutige ids).
- Gültig → in **einer** Transaktion Upsert nach `knowledge_items` (skillbook/skill/resource) bzw. `knowledge_runbooks`
  (runbook) über dieselbe `elementDoc`-Abbildung wie `knowledge.task.complete` (bestehende Payload bleibt;
  `payload` + {citations, book_ref, import_command_id, knowledge_domain, knowledge_version_id, source, last_task_id}) und
  `knowledge_tasks` {id = Befehls-ID, command_type `knowledge.book.ingest`, record_id = Ziel, status completed,
  result {item_ids, runbook_ids, book_ref, gaps}}.
- Ungültig/fehlend (`not_found`/`revoked`) → nichts angewendet, `knowledge_tasks` failed (`invalid_book`) – erscheint
  einmal in der Zustandszone (siehe 3). `files.get` unavailable → Fehler an die Laufzeit, nichts geschrieben.
- Idempotent je Befehls-ID (terminaler `knowledge_tasks`-Eintrag → `duplicate`, keine Schreibvorgänge).

**Manifest-Wunsch knowledge** (für den Manifest-Strang, hier nicht geschrieben):
```json
{
  "handlers": { "knowledge.book.ingest": "handlers/knowledge.book.ingest.js" },
  "commands_in": { "knowledge.book.ingest": { "from_apps": ["research"], "ui": false } },
  "collections": { "knowledge_tasks": {} }
}
```
(Feldname `commands_in` Vorschlag; Inhalt: knowledge nimmt `knowledge.book.ingest` nur von research an, kein
UI-Knopf.) Gegenstück research (`commands_out`) in `modules/research/V4-HANDLERS.md`.

### 2. Schemaquelle `knowledge_tasks`

Geprüft: `modules/knowledge/collections.schema.json` lesen nur noch die alte Shell, der Generator
`v4/schemas/build-schemas.mjs` (einmalige Basis), der Browser-Test `tests/v4-api.test.mjs` (nur `knowledge_views`) und bis
KNOW2 die Handler-Attrappe. Der Manifest-Generator nimmt `v4/schemas/knowledge.collections.schema.json` mit Vorrang
(gen-manifest.mjs:833-838); dieselbe Lage hatte CREWAPP (`ctox_crew_*` nur in v4/schemas). Deshalb: `knowledge_tasks`
steht **nur** in `v4/schemas/knowledge.collections.schema.json` (A9, Version 0, `_sources` „(neu, Strang SKL-B2/KNOW2)“,
neue Collection ohne Migration, daher kein `migrations/knowledge/`), die SKL-B2-Ergänzung in
`modules/knowledge/collections.schema.json` ist zurückgenommen (Datei = Stand vor 69c757748), und
`handlers/tests/_ctx.mjs` prüft gegen das v4-Schema (wie iot). `node v4/schemas/check-schemas.mjs` grün.

### 3. Fehler in der Zustandszone: einmal, bis quittiert oder überholt

Vorher meldete die App bei jeder Montage **jeden** `failed`/`cancelled`-Eintrag aus `knowledge_tasks` (Code je
task_id) – ein Dauer-Zustandstext, der nie verschwand.

Jetzt (`index.js` `knowledgeTaskAlerts` / `reportKnowledgeTasks` / `acknowledgeKnowledgeTask`):
- Ziel eines Auftrags = `command_type` + `record_id` (ohne `record_id` ist jeder Auftrag sein eigenes Ziel).
- Je Ziel zählt nur der jüngste Fehlschlag/Abbruch (nach `created_at_ms`); ältere sind überholt.
- Er erscheint genau einmal (Code `knowledge.task.<ziel>`, ersetzt sich selbst), solange er weder quittiert ist noch
  ein gleich alter oder neuerer Lauf desselben Ziels `completed` ist.
- Zonen-Aktion „Ausblenden“ (`action` am Zonen-Eintrag, v4/shell/app-host.js) → `api.patch('knowledge_tasks', id,
  {acknowledged_at: ISO, acknowledged_by: actor.id, updated_at_ms})`, nie warten; die Quittung gilt für alle Clients und
  jede spätere Montage. Nicht mehr gültige Codes räumt die App selbst.
- Locales: `dismiss` (Ausblenden/Dismiss).

### Prüfungen (gemessen 15.10.2026)

| Prüfung | Ergebnis |
|---|---|
| Handler (aus `handlers/`): `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` | 19/19 (neu `tests/book.ingest.test.mjs` 4) |
| v4-api: `CTOX_APP_API_STUB=…/stub-pins/7706796 node --test modules/knowledge/tests/v4-api.test.mjs` | 11/11 (neu: Zone je Ziel einmal, Ausblenden quittiert am Datensatz, Remount ohne Eintrag, neuer Erfolg räumt), 3× wiederholt grün |
| Mutationsbeleg | 3 Mutationen Eingang (Doppelzustellung, ungültiges Book angewandt, Domänenprüfung) + 5 Mutationen Zone (Quittung, Erfolg, je Ziel, Ausblenden ohne Schreiben, veraltete Codes) rot |

Hinweis Test: Der neue v4-api-Test baut das Hauptfenster vor der eigenen Montage ab (OFFEN-4: Modulzustand
modulweit; eine zweite Montage über einem offenen Tabellenfenster hinterlässt sonst ein `knowledge_table_rows`-Abo).

