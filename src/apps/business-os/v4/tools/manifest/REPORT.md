# Manifest v4 – Bericht (Strang M, 15.10.2026; vom Orchestrator aus dem Strangbericht übernommen)

Prüfung: `node --test gen-manifest.test.mjs` 7/7 grün (Determinismus, genau ein Eigentümer je Collection,
foreign_collection → exports mit passenden ops, Pflichtfelder, base_hash = Dateien, Mini-Repo). `--check` weicht
derzeit bei customers/outbound/support ab, weil die Handler-Stränge dort Dateien anlegen: nach den Ports neu erzeugen.

## Ableitung
- files: alle Dateien außer Tests, sha256; base_hash = sha256 über sortierte Zeilen `pfad\tsha`. schema.js noch gelistet
  (_todo, entfällt beim Port). entry: index.js (alle 40 exportieren mount).
- Eigentum: App, die die Collection selbst in schema.js definiert; bei mehreren per Präfixregel (als Vorschlag markiert).
  4 Collections gehören der Shell/Plattform: business_commands, business_module_catalog, ctox_queue_tasks,
  ctox_runtime_settings (app.js:211-216).
- schema aus collections.schema.json des Eigentümers; class full, Vorschläge: blob für *_chunks, window für die
  Demand-Liste in shared/sync.js:3686, projection 2×. foreign_collections aus module.json, schema.js, JSON und Zeilenscan
  (write nur bei Schreibaufruf in derselben Zeile; nur-Scan-Funde als _proposal). exports = Vereinigung der Nutzung.
  migrations aus migration_strategies. layout aus module.json. _suggested_capabilities aus C1/C2.

## Zahlen
| Merkmal | Wert |
|---|---|
| Apps | 40 (customer-modules/outbound-lead-generation noch nicht erfasst) |
| Collections | 199 (= Schema-Vertrag) |
| App-eigen / Plattform | 195 / 4 |
| Eigentum nur Vorschlag | 8 |
| foreign-Deklarationen | 174 (39 nur aus Scan, 3 mit write) |
| exportierte Collections | 66 |
| ohne Eigentümer / nur im Vertrag | 0 / 0 |
| mehrere Definierer | 6 (business_commands 5 Apps, business_module_catalog, knowledge_* 3× documents+knowledge, outbound_research_adapters browser+outbound) |
| Feld-Drift schema.js vs collections.schema.json | 0 in 300 Kopien |
| Migrationsschritt-Drift | 9 (browser 5, mail outbound_messages, reports workjet_projects, reports+threads ctox_queue_tasks) |
| abweichende Kopien | 2 (business_commands ohne 4 Vertrags-Indizes in 6 Apps, business_module_catalog) |
| Eigentümer ≠ Präfix | 2 (ctox_task_approval_requests → threads, outbound_lead_generation_adapters → browser) |
| in schema.js, nicht in module.json | 31 (reports 23, desktop 5, ctox 2, browser 1) |
| Klassenvorschläge ≠ full | 16 (11 window, 4 blob, 1 projection) |

Folge: collections.schema.json kann einzige Quelle werden, sobald die 9 Migrationsschritt-Sätze übertragen sind.

## Offene Entscheidungen (mit Vorschlag)
1. rights überall null → aus policy.rs ableiten. 2. client_limit/byte_budget null → zuerst die 11 window-Collections
(Werte aus sync.js:3691-3735). 3. Shell-Manifest übernimmt die 4 Plattform-Collections (out/ownership.json
platform.shell); business_commands mit Vertragsschema, App-Kopien löschen. 4. write-ops unterzählt → je App-Port
bestätigen. 5. 39 _proposal-Nutzungen bestätigen/löschen (app-store → documents/notes/spreadsheets vermutlich Fehlfund).
6. *_chunks entfallen zugunsten files. 7. research (research.systematic.run) und tickets (mission::tickets) beim Port prüfen.
Je App: browser gibt Adapter-Collections an outbound ab und überträgt 5 JS-Migrationen; ctox gibt business_module_*/
business_users an Shell/Kernel; knowledge besitzt knowledge_*, documents liest fremd; reports auf tatsächlich Gelesenes
kürzen; cv-print-builder schreibt über api.files; desktop behält channel_pairing_state/desktop_windows;
credentials → ctx.cap.secrets mit Metadaten als Collection.
