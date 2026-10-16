# research – Handler-Port Shell v4 (Strang H6)

Stand 15.10.2026. Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/store.rs`
(research hat keine eigene Rust-Datei; die Logik steckt verstreut in `store.rs`).

Tests: `node --test src/apps/business-os/modules/research/handlers/tests/*.test.mjs` → 13/13 grün; alle Handler laden
zusätzlich in einem nackten `vm`-Kontext (`vm.SourceTextModule`, keine Node-/Web-Globals)
(u. a. SHA-256 gegen `node:crypto`, CSV-Parser, Quittungs-Gate, Ablehnung fremder Lauf-Bindung,
Fehlerpersistenz, `measured_load_points`-Prüfung).

## Grundbefund

Die research-App sendet drei Befehlstypen. Keiner hat einen eigenen nativen Arm im Dispatcher:
`research.systematic.run` und `research.knowledge.refresh` gehen über `record_command` als Agenten-Aufgabe in die
Queue; `research.systematic.report.create` wird mit `module:'documents'` gesendet und gehört dem documents-Pfad.
Die eigentliche research-Fachlogik (~1 300 Zeilen) läuft **nach** dem Agenten-Turn im nativen Antwortpfad
`complete_business_command_from_queue_reply` (`store.rs:10312`) → `promote_systematic_research_workspace_outputs`
und `process_systematic_research_command`: Manifest- und Quittungsprüfung, CSV-Validierung je Ausgabetabelle,
Quellen-Eignungsgate, Evidenz-Schnappschuss mit Inhalts-Hash, Fortschreiben von `research_tasks`/`research_runs`.
Diese Logik ist jetzt der neue Handler `research.systematic.run.complete`.

## Tabelle (Reihenfolge = UI-Häufigkeit; Basis: Aufrufstellen in modules/ + shared/)

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Status |
|---|---|---|---|---|---|
| 1 | `research.systematic.report.create` | GEMISCHT (E) | `store.rs:21471` `process_documents_report_command` (+ Lineage `:23395ff.`), `module:'documents'` | `handlers/research.systematic.report.create.js` → `ctx.cap.legacy['research.systematic.report.create']` | **Legacy** (Eigentümer documents-Port; Rendering = `cap.office.render`) |
| 2 | `research.systematic.run` | Queue (Aufnahme) | `store.rs:6753` `record_command` + `suggested_skill_for_command :27747` | `handlers/research.systematic.run.js` + `cap.agent.task.enqueue({skill:'systematic-research', on_complete})` | portiert, getestet |
| 2a | `research.systematic.run.complete` (**neu**) | FACHLOGIK + Fähigkeiten | `store.rs:10335-10348`, `:21570-21799`, `:21977-22818`, `:22820-22943`, `:23180`, `:23289-23417` | `handlers/research.systematic.run.complete.js`, `_lib/systematic.js`, `_lib/records.js`, `_lib/util.js` | portiert, getestet |
| 3 | `research.knowledge.refresh` | Queue (Aufnahme) | nur Inventar `browser_literal_types`; `suggested_skill_for_command` → `knowledge` | `handlers/research.knowledge.refresh.js` + `cap.agent.task.enqueue` | portiert, getestet |

**Legacy-Rest (`ctx.cap.legacy.<type>`): 1 von 3** (`research.systematic.report.create`, offen gezählt; gehört fachlich
zu documents, `process_documents_report_command` ist GEMISCHT mit Office-Rendering).

## Capability-Wünsche

| Capability | Signatur | Beispielaufruf |
|---|---|---|
| `ctx.cap.agent.task.enqueue` (E §3) | `({command_id, skill, title?, instruction, priority?, thread_key?, payload, on_complete?}) → {task_id}`; **neu: `on_complete`** = Befehlstyp, den der Core nach dem Turn mit `{command_id, reply_text, task_id}` auslöst (ersetzt den nativen Antwortpfad) | `await ctx.cap.agent.task.enqueue({command_id:cmd.id, skill:'systematic-research', instruction, on_complete:'research.systematic.run.complete'})` |
| `ctx.cap.agent.workspace.read` | `({task_id, path}) → {text, sha256}\|null` – liest eine Datei im geleasten Task-Workspace, kanonisiert, verweigert Pfade außerhalb; `sha256` über die exakten Bytes | `await ctx.cap.agent.workspace.read({task_id, path:'validation/evidence-manifest.json'})` |
| `ctx.cap.agent.queue.task.get` / `.for_command` | wie ctox (`load_queue_task`, `find_queue_task_for_command`) | `await ctx.cap.agent.queue.task.for_command({command_id})` |
| `ctx.cap.agent.queue.projection.refresh` | `({command_id}) → {}` (`refresh_queue_task_projection`) | `await ctx.cap.agent.queue.projection.refresh({command_id})` |
| `ctx.cap.agent.command.settle` | `({command_id, status, task_id, task_status, result?, error?, inbound_channel?, updated_at_ms}) → {}` – schließt den **ursprünglichen** Befehl in `business_commands` ab (Plattform-Collection, nicht research) | `await ctx.cap.agent.command.settle({command_id, status:'accepted', result})` |
| `ctx.cap.knowledge.table.describe` / `.create` / `.import` | E §3 `knowledge.*`: `describe({domain,key})`, `create({domain,key,source_system,title,description})`, `import({domain,key,from_workspace:{task_id,path},mode:'replace'}) → {ok,row_count}` (bisher `knowledge::dispatch_capturing data …`) | `await ctx.cap.knowledge.table.import({domain, key:'source_catalog', from_workspace:{task_id, path}, mode:'replace'})` |
| `ctx.cap.knowledge.project` | `() → {count}` (`sync_knowledge_tables`) | `await ctx.cap.knowledge.project()` |
| `ctx.cap.knowledge.tables.active` | `({domain}) → [{catalog_id, domain, table_key}]` geordnet nach table_id | `await ctx.cap.knowledge.tables.active({domain})` |
| `ctx.cap.knowledge.rows` (E §3) | `({catalog_id}) → {rows[]}` (alle Parquet-Zeilen, `knowledge_table_all_rows`) | `await ctx.cap.knowledge.rows({catalog_id})` |
| `ctx.cap.knowledge.snapshot.sha256` | `({path}) → {sha256}\|null` – Snapshot-Datei unter dem Instanz-Root hashen (`document_source_receipt_snapshot_matches`) | `await ctx.cap.knowledge.snapshot.sha256({path:receipt.snapshot_path})` |
| `ctx.cap.legacy['research.systematic.report.create']` | Übergang §16.14 | – |

Manifest-Deklarationen: `foreign_collections: { business_commands: { owner_app: 'ctox', ops: ['read'] } }` (der
Abschluss-Handler liest den Ursprungsbefehl); eigene Schreibzugriffe nur `research_tasks`, `research_runs`.

## Abweichungen / Unklarheiten (markiert)

1. **Neuer Befehlstyp `research.systematic.run.complete`**: Vertragswunsch an „ctox sync v4“ – der Core muss nach dem
   Agenten-Turn statt des nativen Antwortpfads diesen Handler auslösen (`on_complete`). Ohne diese Brücke läuft der
   Abschluss nicht. Laufzeitgrenze 30 s CPU: bei sehr großen Knowledge-Tabellen ggf. knapp (UNKLAR, nicht gemessen).
2. **Hash der Knowledge-Version**: `serde_json::to_vec` ohne `preserve_order` sortiert Schlüssel (BTreeMap) – so
   nachgebaut. UNKLAR: ob `preserve_order` transitiv aktiv ist (Feature-Vereinigung) und dass Rust Gleitkommazahlen mit
   ganzem Wert als `1.0` schreibt, JS als `1`. Der `version_id` kann daher vom Rust-Wert abweichen; innerhalb von v4 ist
   er stabil.
3. **CSV**: eigener Parser nach `csv`-Crate-Voreinstellungen (Komma, `"`-Escape, nicht flexibel, Leerzeilen übersprungen);
   Fehlermeldungstexte des Crates nicht 1:1.
4. **`Url::parse`**: Im nackten vm-Kontext gibt es weder `URL` noch `TextEncoder` noch `crypto` (geprüft). Deshalb
   eigene UTF-8-Kodierung, eigenes SHA-256 und ein Schema/Host-Ersatz `parseUrlParts` (WHATWG-`URL`, falls vorhanden).
   Ohne diesen Ersatz wäre jede Quittung still als „nicht geeignet“ verworfen worden. Abweichung bei exotischen URLs
   (z. B. `https:///host`) möglich.
5. **Ablauf bei Fehlern unverändert**: Fehler in der Promotion brechen ohne Fehlerpersistenz ab (Rust `?`); Fehler im
   Evidenz-Schnappschuss schreiben `failed` auf Task, Lauf und Befehl.
6. **Status von `business_commands`**: Rust schrieb die Zeile direkt; jetzt `cap.agent.command.settle`, weil
   `business_commands` nicht research gehört.


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `agent.task.enqueue` | flach, `payload: {...payload, business_command_id}`, `idempotency_key`, `on_complete:'research.systematic.run.complete'` |
| `agent.task.get` | Rückruf `{task_id, status, result}` → Befehls-ID |
| `agent.task.workspace.read` | `({task_id, path})` (vormals `agent.workspace.read`) |
| `knowledge.query` | `({argv, workspace_task_id?})` = `ctox knowledge data describe|create|import --from-file … --mode replace` (store.rs:22282) |
| `dataframe.window` | `({table, skip, limit})` – alle Zeilen einer Knowledge-Tabelle seitenweise |

- `knowledge.tables.active` → Katalog aus `knowledge_tables` (App knowledge, Export read; foreign_collections).
- `knowledge.rows` → `dataframe.window`; `knowledge.snapshot.sha256` → files-Datensatz mit ref = Snapshot-Hash (§16.32).
- `knowledge.project` und `agent.queue.projection.refresh` entfallen (§16.30: Projektion ist live).
- Querverweis knowledge: nur über den Export `knowledge_tables` und das Kernprimitiv `knowledge.query`; keine Code-Importe,
  keine neuen knowledge-Befehle nötig (der Knowledge-Datenspeicher ist nativ, die App knowledge projiziert nur Fenster).

### Kern-Import
- Snapshot-Dateien (`snapshot_path`/`archive_path`) einmalig nach files (ref = sha256).

## App-Skills und Automatisierung (Strang SKL-B2, 15.10.2026, §16.30/§16.34)

| Auslöser (UI) | Vorher | Nachher |
|---|---|---|
| „Research starten“ (`index.js` runResearch) | `research.systematic.run`, Anweisung als Literal in index.js, Skill `systematic-research` per `suggestedSkill()`, nur `on_complete` | Handler füllt `prompts/systematic-run.md` aus `payload.research_brief` + `web_stack_plan`, Core-Skill `systematic-research`, `on_complete:'research.systematic.run.complete'`, `on_failed:'research.systematic.run.failed'` |
| Graph-Nachrecherche (`dispatchTargetedGraphResearch`) | wie oben, Literal `index.js:2586-2598` | `research_mode:'targeted_graph_gap'` → `prompts/graph-gap.md`, sonst wie oben |
| „Knowledge aufbauen“ (`buildKnowledgeFromResearch`) | `research.knowledge.refresh`, Literal in `knowledgeRefreshPayload`, Phantom-Skill `knowledge`, kein Callback | `prompts/knowledge-refresh.md`, Paket-Skill `research/knowledge-refresh`, `on_complete:'research.knowledge.refresh.complete'`, `on_failed:'research.knowledge.refresh.failed'` |

Die Templates enthalten die bisherigen Texte wörtlich (Tests vergleichen gegen den früheren Literal-Aufbau inkl.
`.filter(Boolean)`: optionale Zeilen kommen als `*_block`-Platzhalter mit eigenem Zeilenende). Die UI sendet kein
`instruction`/`prompt`/`required_skills` mehr, sondern `research_brief` (Titel, Aufgabentext, Kriterien, Scoring-Modell mit
aufgelösten Gewichten, Achsen, Zahl der evidence-fähigen Quellen).

**`systematic-research` bleibt Core-System-Skill** (Entscheidung nach Inhalt): generische Recherche-Methodik (Literatur-
graph, Evidence-Gate, Sättigung, CTOX-Web-Stack) mit ~20 Referenzen/Skripten, genutzt auch von documents
(`research.systematic.report.create`) und als allgemeiner Einstieg „single entry point for durable research in CTOX“;
nichts darin ist research-App-spezifisch. Aufruf ohne Namensraum.

**Neuer Paket-Skill `skills/knowledge-refresh/SKILL.md`**: Knowledge Book (Skillbook, Skill, Runbooks, Ressourcen) aus der
Lineage des Laufs, Ergebnis als `knowledge-refresh/result.json` im Workspace, kein Schreiben in Stores.

Rückrufe (`handlers/_lib/callback.js`, idempotent, nur eigene Collections):
- `research.systematic.run.failed` – Lauf `failed`/`cancelled` + `payload.error`, Aufgabe `failed` (Abbruch: `ready`) +
  `payload.last_error` (ref: store.rs:22820-22943).
- `research.systematic.run.complete` – unverändert, plus Doppelzustellungs-Sperre (abgeschlossener Lauf mit derselben
  `task_queue_id`).
- `research.knowledge.refresh.complete` – prüft `result.json` fail-closed (Schema, Kennungen = Auftrag, Elemente mit
  id/kind/title, Zitate nur aus `source_lineage`) und übergibt das Book an die App knowledge (seit KNOW2, siehe unten);
  `research_tasks.payload.knowledge_refresh` {status, task_id, summary, element_ids, gaps, book_ref, knowledge_command_id}.
- `research.knowledge.refresh.failed` – `knowledge_refresh.status` `failed`/`cancelled` mit `error`.

Tests: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (aus `handlers/`) → 21/21
(`tests/skills.test.mjs` 9 gegen `v4/tools/agent-sim`). Mutationsbeleg 9 Mutationen research rot.

OFFEN:
1. ~~Knowledge Book → Knowledge-App~~ **erledigt durch Strang KNOW2**, siehe Abschnitt „Knowledge Book an die App
   knowledge (KNOW2)“.
2. `research.systematic.run.complete` nutzt weiter `agent.queue.task.*` und `agent.command.settle` (Bestand H6).
3. `research.systematic.report.create` (documents-Pfad) und Kontext-Chat/`app.modify` unverändert.

Kern-Abbau (jetzt tot für research): `suggested_skill_for_command` Zweige `starts_with("research.systematic.")` und
`contains("knowledge")` (store.rs:27747-27755), `command_prompt` für diese Typen (:27009), `complete_business_command_
from_queue_reply` Zweig `is_systematic_research_command` (:10336-10348) sobald on_complete im Kern zugestellt wird,
`browser_literal_types`-Eintrag `research.knowledge.refresh`.

## Knowledge Book an die App knowledge (Strang KNOW2, 15.10.2026)

Entscheidung (Orchestrator): Wissensdaten gehören der App knowledge; Apps greifen untereinander nur über deklarierte
Befehle (`ctx.command`) oder `foreign_collections` + `exports` zu.

Vorher: `research.knowledge.refresh.complete` schrieb je Book-Element ein `research_notes`-Dokument
(`kn:<task>:<element>`, `kind: knowledge_<kind>`, Markdown im `body`) – die Elemente erschienen nie in `knowledge_items`.

Nachher (`handlers/_lib/callback.js` `completeKnowledgeRefresh`):
1. `result.json` prüfen wie bisher (fail-closed, unverändert).
2. Book in das Format der App knowledge `ctox.knowledge.book.v1` bringen (`knowledgeBook`: domain, version, title,
   summary, elements {id, kind, title, markdown, summary, citations}, gaps, provenance {app:'research', research_task_id,
   research_run_id, research_command_id, agent_task_id}) und als files-Datensatz anlegen:
   `ctx.files.put(bytes, {name:'knowledge-book-<task>.json', mime:'application/json'})` → `ref` (Kern §16.32).
3. `ctx.command('knowledge.book.ingest', {book:{ref}, title, knowledge_domain, knowledge_version_id, record_id, source},
   {id:'<agent_task_id>:knowledge-book', module:'knowledge', record_id:<research_task_id>})` – feste ID: eine erneute
   Zustellung nach einem Abbruch trifft denselben Befehl (knowledge ist je Befehls-ID idempotent). Capability- und
   Befehlsaufrufe außerhalb der Transaktion.
4. In **einer** Transaktion: `research_notes` genau ein Verweis-Dokument je Research-Aufgabe `kb:<research_task_id>`
   {kind:'knowledge_book', status:'handed_over', payload:{owner_app:'knowledge', book_ref, knowledge_command_id,
   knowledge_element_ids, knowledge_domain, knowledge_version_id, research_run_id, research_command_id, agent_task_id}},
   kein Inhalt; `research_tasks.payload.knowledge_refresh` {status:'completed', note_id, book_ref, knowledge_command_id,
   element_ids, summary, gaps}.
5. Lehnt knowledge den Befehl ab (`status` rejected/failed) → `knowledge_refresh` failed, Code `knowledge_rejected`,
   kein Verweis. Wirft `ctx.command`/`files.put` (unavailable) → Fehler an die Laufzeit, nichts geschrieben.

Das Ergebnis der Übernahme (Elemente angelegt oder Book ungültig) steht in der App knowledge (`knowledge_tasks`, id =
Befehls-ID) und erscheint dort in der Zustandszone; research sieht nur die Annahme (§16.21).

**Manifest-Wunsch research** (für den Manifest-Strang, hier nicht geschrieben):
```json
{
  "commands_out": [
    { "app": "knowledge", "type": "knowledge.book.ingest", "from_handlers": ["handlers/research.knowledge.refresh.complete.js"] }
  ]
}
```
(Feldname Vorschlag; Inhalt: research darf an knowledge genau den Befehl `knowledge.book.ingest` senden.) Die
Gegenseite (knowledge nimmt ihn von research an) steht in `modules/knowledge/V4-PORT.md`, Abschnitt KNOW2.
Benutzte Handler-Primitive: `ctx.files.put` (§16.32), `ctx.command` (§16.21/§16.25) – keine neue Core-Capability.

Tests (aus `handlers/`): `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` → 23/23; neu bzw.
geändert in `tests/skills.test.mjs`: complete → Befehl mit BlobRef, ein Verweis in `research_notes`, Doppelzustellung
ohne zweiten Befehl; Ende-zu-Ende research → echter knowledge-Handler `knowledge.book.ingest` (Elemente in
`knowledge_items`/`knowledge_runbooks`); Ablehnung → `knowledge_rejected`; ungültiges Ergebnis → kein Befehl.
`tests/_ctx.mjs`: `ctx.command` protokolliert jetzt `opts` und nimmt `commandResult`. Mutationsbeleg: 5 Mutationen
research rot (Befehl fehlt, falsche App, Elemente wieder in research_notes, Doppelzustellung, Ablehnung ignoriert).

Kern-Abbau: keiner neu (research hatte für das Knowledge Book keinen Core-Pfad).


## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

„App ändern“ im Kontextmenü (`dispatchResearchContextChat`): vorher `ctox.business_os.app.modify`; jetzt `sendContextChat` (Modus app) über `dispatchResearchCommand`. Der Datenmodus bleibt `business_os.data.modify` (nicht Teil von SKL-D).

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
