# documents — Fachlogik als JS-Handler (Strang H7, Shell v4)

Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/store_office_commands.rs`
(1571 Z.), `store.rs:15857` (`ctox.office.settings.save`), `store.rs:21471` (`process_documents_report_command`);
Analyse E (`E-befehlstypen.md` Z. 214–218, 330), C1 §documents, Hash `hashing.rs:24`.
Vertrag: Architektur §9.4, §16.13, §16.16.

Trennung (E/§16.13): Die Office-Befehle sind durchgehend **GEMISCHT**. Fachlogik (Datensatzauflösung,
Versions-/Konfliktprüfung, Idempotenz, Pflege der `document_versions`/`documents`-Dokumente) ist Handler;
die Office-Engine (öffnen, rendern, Änderungen anwenden, Blob-Chunks lesen/schreiben, Integritätsprüfung)
ist generische Capability `ctx.cap.office.*`.

## 1. Inventar der Befehlstypen

| # | Befehlstyp | E-Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `office.document.prepare` | GEMISCHT | `store_office_commands.rs:69` | Handler + `cap.office.prepare` | portiert, 2 Tests |
| 2 | `office.document.commit` | GEMISCHT | `:235` → `:718` | Handler + `cap.office.apply` | portiert, 4 Tests |
| 3 | `office.document.freeze_email_content` | GEMISCHT | `:324` | Handler + `cap.office.render` | portiert, 1 Test |
| 4 | `office.document.export` | GEMISCHT | `:431` | Handler + `cap.office.render` | portiert, 1 Test |
| 5 | `ctox.office.settings.save` | FAEHIGKEIT (geteilt documents/spreadsheets) | `store.rs:15857` | Handler + `cap.office.settings` | portiert, 1 Test |
| 6 | `research.systematic.report.create` | GEMISCHT (`module:'documents'`, fachlich documents) | `store.rs:21471` `process_documents_report_command`, `:23433` `document_report_lineage`, `:23498` `writeback_generated_docx` | Handler + `cap.office.render` + `cap.doc.extract.verify_source_receipts` | portiert, 5 Tests |

`#6` wird mit `module:'documents'` gesendet und gehört fachlich documents (Koordinator-Hinweis H7). Die
`business_commands`-Statuspflege von `process_documents_report_command` ist Plattform/Queue (nicht App-Logik)
und entfällt im Handler-Vertrag. Portiert ist die documents-FACHLOGIK: Dateiname/Dokument-ID/Titel/Tags,
Lineage (knowledge_version + source_receipt_snapshot_hashes + linked_records), Versionsnummerierung und die
`documents`/`document_versions`-Datensätze. Die DOCX-Aufnahme (Lokalisieren/Validieren/Index-Text/Hash/Chunks)
und die Receipt-Snapshot-Verifikation sind Capabilities. Der Legacy-Durchgriff in `modules/research/handlers/
research.systematic.report.create.js` (Strang H6) bleibt stehen; der Koordinator entfernt ihn danach. **0 offen.**

## 2. Treue-Notizen
- `prepare` stempelt genau die Rust-Felder auf die Version (`editor_blob_id`, `editor_protocol(_version)`,
  `source_sha256`, `editor_sha256`, `conversion_state='prepared'`, `implemented_features`, `office_manifest`,
  `editor_manifest`, `updated_at_ms`) und übernimmt die delimited→xlsx-Kanonisierung, wenn die Capability
  `canonical` meldet (`blob_id`, `original_blob_id`, `canonical_source_kind`, `canonical_mime_type`).
- `commit`: Konfliktkette wie Rust — `base_version_id == geladene version_id == record.current_version_id`,
  neue `version_id = ${id}_office_v${n}_${shortHash(command.id)}`, Idempotenz-Replay über vorhandenes
  Versionsdokument (Crash-Recovery), Version + Datensatz in **einer** `ctx.transaction`.
- `shortHash` = erste 10 Zeichen base64url(SHA-256) — identisch zu `hashing.rs:24` (`crypto.digest('base64url')`).
- `index_text`/Modelltext auf 20 000 Unicode-Skalarwerte begrenzt.
- Feature-Liste (11 Dokument-Features) wird vom Handler als Argument an die Capability übergeben (App-Wissen).

## 3. Capability-Vertragsvorschlag

### `ctx.cap.office.prepare(req) → prepared`
- Deckt heute: `prepare`-Zweig `store_office_commands.rs:69-234` (office_engine `prepare`, `delimited_text_to_xlsx`,
  `persist_office_editor_blob`, `persist_office_canonical_spreadsheet_blob`).
- `req`: `{ kind:'document'|'spreadsheet', source_blob_id, chunks_collection, record_id, version_id,
  implemented_features:string[], source_hints:{source_kind, mime_type, filename} }`
- `prepared`: `{ editor_blob_id, editor_protocol, editor_protocol_version, source_sha256, editor_sha256,
  implemented_features:string[], manifest, editor_manifest, diagnostics:any[],
  canonical?:{ blob_id } }` (canonical nur bei delimited-Konvertierung).
- Fehler: `invalid` (Blob fehlt/kaputt), `unavailable` (Engine nicht bereit).
- Beispiel: `const p = await ctx.cap.office.prepare({ kind:'document', source_blob_id, chunks_collection:'document_blob_chunks', record_id, version_id, implemented_features: DOCUMENT_FEATURES, source_hints })`.

### `ctx.cap.office.apply(req) → applied`
- Deckt heute: `commit`-Zweig `:235-323` + `commit_office_document_version :718-999` (ohne Versions-/Datensatzpflege:
  `apply_changes`, `export`, Blob-Persist unter `target_blob_id`, Integritätsprüfung).
- `req`: `{ kind, base_canonical_blob_id, base_editor_blob_id, changed_blob_id, expected_changed_sha256|null,
  expected_base_sha256|null, implemented_features:string[], chunks_collection, record_id,
  target_version_id, target_blob_id }`
- `applied`: `{ blob_id, sha256, editor_sha256, bytes, chunks, editor_protocol, editor_protocol_version,
  implemented_features:string[], manifest, diagnostics:any[] }`
- Fehler: `conflict` (Hash-Mismatch der Basis), `invalid`, `unavailable`.

### `ctx.cap.office.render(req) → out`
- Deckt heute: `export`-Zweig `:431-452` **und** `freeze_email_content :324-430` (office_engine `export`,
  `compile_document_to_email`, `persist_document_email_blob`).
- `req` (Export): `{ to:'docx'|'xlsx'|'pdf', kind, source_blob_id, chunks_collection, record_id, version_id }`
  → `{ mime_type, extension, sha256, bytes, manifest, diagnostics }`.
- `req` (E-Mail): `{ to:'email_html', source_blob_id, chunks_collection, record_id, version_id,
  expected_source_sha256|null }` → `{ schema_version, compiler_id, source_sha256, html_blob_id, html_sha256,
  html_bytes, text_blob_id, text_sha256, text_bytes, assets:[{content_id,filename,mime_type,blob_id,sha256,bytes}],
  diagnostics }`.
- Fehler: `invalid` (Quell-Hash-Mismatch), `unavailable`.

### `ctx.cap.office.settings.save(settings) → outcome`
- Deckt heute: `save_office_runtime_settings` (`store.rs:15857`), Policy workspace `RuntimeManage`.
- `settings`: `OfficeRuntimeSettingsRequest`; `outcome`: gespeicherte Laufzeiteinstellungen. Fehler: `invalid`, `forbidden`.

### `ctx.cap.office.render(req) → rendered` (generierte DOCX-Aufnahme — zusätzlicher `source:'generated'`-Zweig)
- Deckt heute: `writeback_generated_docx :23498` ohne Datensatzpflege — `resolve_generated_docx_path` (DOCX aus
  dem Agenten-Workspace lokalisieren), `validate_and_extract_docx_text` (ZIP/`[Content_Types].xml`/`word/document.xml`),
  `hex_sha256`, Blob-Chunking unter `target_blob_id`.
- `req`: `{ to:'docx', source:'generated', command_ref:{id,type,payload,client_context}, filename, document_id,
  version_id, target_blob_id, chunks_collection }`
- `rendered`: `{ source_sha256, index_text, bytes, chunks, source_path }`. Fehler: `not_found` (DOCX fehlt),
  `invalid` (keine gültige DOCX), `unavailable`.

### `ctx.cap.doc.extract.verify_source_receipts(req) → { snapshot_hashes }`
- Deckt heute: die Receipt-Auflösung + Eignungs-/Snapshot-Byte-Prüfung aus `document_report_lineage :23433`
  (`document_source_receipt_candidate`, `document_source_receipt_is_eligible`,
  `document_source_receipt_snapshot_matches`). Reine Leseprüfung persistierter Receipt-Snapshots.
- `req`: `{ links: Value[] }` (vom Handler vorselektierte Source-Receipt-Links).
- Ergebnis: `{ snapshot_hashes: string[] }` (dedupliziert). Fehler: `conflict` (Snapshot-Hash geändert),
  `invalid` (nicht eignungsfähig / kein verifizierter Snapshot), `not_found` (Receipt nicht auflösbar).
- Gehört zur `cap.doc.extract`-Familie (E §3); der documents-Handler ruft sie nur im Research-Modus.

## 4. Dateien
- `handlers/_lib/office.js`
- `handlers/office.document.prepare.js`, `office.document.commit.js`, `office.document.freeze_email_content.js`,
  `office.document.export.js`, `ctox.office.settings.save.js`, `research.systematic.report.create.js`
- `handlers/tests/_ctx.mjs`, `handlers/tests/documents.test.mjs`


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

BlobRef = files.ref (sha256) als `{ref}` (§16.32); Versionsdokumente behalten `blob_id` = files-id und `*_sha256` = ref.

| Capability | Aufruf (Kernform) | Wo |
|---|---|---|
| `office.edit.prepare` | `({kind, blob_id:{ref}, implemented_features})` → `{session, model, manifest, protocol, protocol_version, sha256}` | prepare; vor export/freeze; Validierung der erzeugten DOCX (report.create) |
| `office.edit.apply` | `({session, changes:{ref}, base_version: session.editor.ref, implemented_features})` → `{session, blob_id:{ref}, sha256, manifest}` | commit (`_lib/office.js applyStagedEditor`) |
| `office.render` | `({session, to: docx|email_html})` – Session immer aus prepare | export, freeze_email_content |
| `dataframe.window` | `({table, skip, limit})` | Belegzeilen für report.create |

- `pdf`: liefert der Kern nicht → `export` lehnt `format:'pdf'` vor jedem Aufruf mit `unavailable` ab (Befehlsstatus).
- `doc.extract.verify_source_receipts` entfällt: Handler-Logik `_lib/report.js verifySourceReceipts` (Belegzeile aus
  Knowledge-Tabelle über `dataframe.window`, Eignung wörtlich store.rs:23289, Snapshot-Bytes = files-Datensatz mit
  ref = Hash, angefragter Hash muss gleich sein). Fremdzugriff `knowledge_tables` (knowledge, read), `files` (read).
- research.systematic.report.create: die erzeugte DOCX ist ein files-Datensatz (Payload `output_file_ref|file_ref|blob_ref`,
  sonst jüngste Datei mit dem erwarteten Namen); kein Re-Render, das Original bleibt Versionsblob.

### Kern-Import
- Snapshot-Dateien der Quellbelege (`snapshot_path`/`archive_path` auf der Platte) einmalig nach files (ref = sha256).
- `document_blob_chunks` → files (je Blob ein Datensatz, id = bisherige blob_id); danach entfällt die Chunk-Collection.

## Automatisierung (Strang SKL-B3, 15.10.2026, Kern §16.30/§16.34)

Knopf → `api.command` → Handler füllt `prompts/*.md` (`ctox:shared/prompt-template.js`, `ctx.asset`) →
`ctx.cap.agent.task.enqueue({…, on_complete, on_failed})` → Rückruf als eigener Befehl (höchstens 1× je task_id).
Kein Skill im Paket: die Aufgaben nutzen die Core-System-Skills `systematic-research` (Methode, geteilt mit research)
und `doc` (Office-Pack). Die Phantom-Angabe `required_skills: [..., 'knowledge', ...]` ist aus index.js entfernt.

| Auslöser (UI) | Befehl | vorher | nachher | Rückruf |
|---|---|---|---|---|
| „Neues Dokument“, „Runbook starten“ (research.report.*), Import mit Runbook | `research.systematic.report.create` | Anweisung im UI gebaut (`dispatchNewDocumentReport`), Durchfall `record_command` (Q); Auswertung nativ R1 `process_documents_report_command` (store.rs:10325/21471) bzw. bis SKL-B3 der Rückschrieb im create-Handler | Handler rendert `prompts/report.md` + `research-report-<typ>.md` + `report-type-*.md` + `knowledge-*.md`, enqueue mit Skill `systematic-research` | `research.systematic.report.complete` (DOCX-Rückschrieb `_lib/report-writeback.js`, Lineage aus der JSON-Antwort, DOCX aus Antwort-Ref → Workspace → files nach Namen; idempotent über `document_versions.business_command_id`), `research.systematic.report.failed` (ohne Schreiben, Zustandszone) |
| Seed-Runbooks „Zusammenfassen/Requirements extrahieren/Risiken prüfen“ (`dispatchDocumentRunbook`) | `document.summarize`, `document.extract-requirements`, `document.review-risks` | kein Handler, Durchfall Q, Ergebnis nur in der Chat-Projektion (R7) | Handler rendert `prompts/document-runbook.md` + `document-<runbook>.md`, enqueue mit Skill `doc`, Lauf `documents.runbook_runs[<runbook>] = {task_id, status:'queued', …}` | `document.runbook.complete` (`result_text`), `document.runbook.failed` (`failed`/`cancelled` + `error`); nur der laufende Eintrag (gleiche task_id, queued) wird abgeschlossen |

Prompts im Paket (wörtlich aus index.js, nur Platzhalter): `report.md`, `report-type-auto.md`, `report-type-fixed.md`,
`knowledge-auto.md`, `knowledge-skill.md`, `knowledge-table.md`, `research-report-*.md` (10, = `prompt_template` der
SYSTEMATIC_REPORT_RUNBOOKS), `document-runbook.md`, `document-summarize.md`, `document-extract-requirements.md`,
`document-review-risks.md`. Neu (nicht im Bestand): der Abschlussabsatz in `report.md` (JSON-Antwort mit
`knowledge_version_id`/`source_receipt_links`) ersetzt den früheren Core-Rückschrieb; `document-runbook.md` nennt Dokument,
Version, files-Ref und einen Textauszug (≤ 6000 Zeichen), die früher nur in der JSON-Vorschau von `command_prompt` standen.
`prompt_template` der Runbook-Datensätze verweist jetzt auf `prompts/<id>.md`.

UI: `watchAgentFailures` (index.js) meldet eintreffende Rückrufe `*.failed` (auch Abbruch) als je ein Zoneneintrag
`documents.agent.<task_id>`; kein Zustandstext im Inhalt, kein Polling.

Kern-Abbau (jetzt tot für documents): `store.rs` `is_documents_report_command` :23741 + `process_documents_report_command`
:21471 + Zweig in `complete_business_command_from_queue_reply` :10325, `resolve_generated_docx_path`/`docx_paths_from_text`
:24776-24840, `suggested_skill_for_command` Zweig „research.systematic.*“ für module documents, `command_prompt` für
`document.*`; der Legacy-Durchgriff `modules/research/handlers/research.systematic.report.create.js` (H6).

Tests: `handlers/tests/automation.test.mjs` (18, agent-task-Simulation), `tests/v4-api.test.mjs` (Zone-Test).
Lauf: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` aus `handlers/`.

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

Kontextmenü „Mit CTOX arbeiten / Frage / App ändern“ (`index.js` `renderDocumentsContextMenu`, `dispatchDocumentsContextChat`): vorher `business_os.chat.task` / `ctox.business_os.app.modify` ohne App-Handler mit Anweisungstext im UI; jetzt `sendContextChat` (Modi-Markup, Payload `{source_app, record_ref, selection, text, mode, title, thread_key, context}`), Versand über `sendDocumentsCommand` (Zustandszone), Sprung in den ctox-Chat.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }, { "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
