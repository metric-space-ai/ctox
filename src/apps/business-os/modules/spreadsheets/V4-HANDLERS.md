# spreadsheets — Fachlogik als JS-Handler (Strang H7, Shell v4)

Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/store_office_commands.rs`
(geteilte Office-Engine mit documents), `commit_office_spreadsheet_version :1002-1220`; Analyse E
(`E-befehlstypen.md` Z. 351–353), C2 §spreadsheets. Vertrag: §9.4, §16.13, §16.16.

Trennung wie documents: GEMISCHT. Fachlogik = Versions-/Datensatzpflege, Konflikt/Idempotenz,
**Ingestion-/Knowledge-Lineage-Übernahme**; Engine = `ctx.cap.office.*` (identischer Vertrag, siehe
documents/V4-HANDLERS.md §3). Die Capability-Signaturen werden nicht dupliziert — dieselbe `cap.office`.

## 1. Inventar der Befehlstypen

| # | Befehlstyp | E-Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `office.spreadsheet.prepare` | GEMISCHT | `store_office_commands.rs:69` | Handler + `cap.office.prepare` | portiert, 1 Test |
| 2 | `office.spreadsheet.commit` | GEMISCHT | `:235` → `:1002` | Handler + `cap.office.apply` | portiert, 3 Tests |
| 3 | `office.spreadsheet.export` | GEMISCHT | `:431` | Handler + `cap.office.render` | portiert, 1 Test |

`ctox.office.settings.save` ist der geteilte Office-Einstellungsbefehl; er liegt im documents-Strang
(dort als Handler + `cap.office.settings`), da beide Apps ihn senden und die Einstellung engine-global ist.

## 2. Treue-Notizen
- Kanonisierung: prepare fordert bei CSV/TSV-Quellen über die Capability die delimited→xlsx-Umschreibung an
  (Hinweise `source_kind`/`mime_type`/`filename` → `source_hints`); nur **ein** Feature
  (`spreadsheet.open-render-sheets`).
- commit: `source_kind='office_edited_xlsx'`, `model_json.type='xlsx'`, `editor_blob_id = staged editor blob`
  (nicht der neue Canonical-Blob, anders als beim Dokument — treu zu `:1216`).
- Lineage-Übernahme aus Basisversion (sonst Datensatz): `ingestion_kind`, `linked_records`,
  `source_receipt_snapshot_hashes`, `knowledge_version`, `knowledge_lineage` (`:1083-1096`).
- Konflikt-/Idempotenz-/`shortHash`-Regeln identisch zu documents.

## 3. Capability-Vertragsvorschlag
Keine eigenen Capabilities: nutzt `ctx.cap.office.{prepare,apply,render,settings}` (Vertrag in
documents/V4-HANDLERS.md §3). `kind:'spreadsheet'` wählt den xlsx-Pfad; `apply` persistiert unter
`target_blob_id = ${version_id}_blob` und verifiziert die Integrität der `spreadsheet_blob_chunks`.

## 4. Dateien
- `handlers/_lib/office.js` (treue Kopie der documents-Office-Logik; getrennte App, kein Cross-Import)
- `handlers/office.spreadsheet.prepare.js`, `office.spreadsheet.commit.js`, `office.spreadsheet.export.js`
- `handlers/tests/_ctx.mjs`, `handlers/tests/spreadsheets.test.mjs`


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) | Wo |
|---|---|---|
| `office.edit.prepare` | `({kind:'spreadsheet', blob_id:{ref}, implemented_features})` (CSV → kanonisches XLSX: `session.source.ref` ≠ Quelle) | prepare, vor export |
| `office.edit.apply` | `({session, changes:{ref}, base_version, implemented_features})` | commit (`_lib/office.js applyStagedEditor`) |
| `office.render` | `({session, to:'xlsx'})` | export |

`pdf` → `unavailable` vor jedem Aufruf. Commit liefert `editor_blob_id` = Editor der neuen Session, die gestagte Datei
steht in `staged_editor_blob_id`.

### Kern-Import
- `spreadsheet_blob_chunks` → files (je Blob ein Datensatz, id = bisherige blob_id).

## Automatisierung (Strang SKL-B3, 15.10.2026, Kern §16.30/§16.34)

| Befehl | vorher | nachher | Rückruf |
|---|---|---|---|
| `spreadsheet.summarize`, `spreadsheet.audit-formulas`, `spreadsheet.risk-review` (index.js `dispatchSpreadsheetRunbook`, heute ohne Aufrufer, V4-PORT.md) | kein Handler, Durchfall `record_command` (Q), Ergebnis nur Chat-Projektion (R7) | Handler (`_lib/automation.js`) rendert `prompts/spreadsheet-runbook.md` + `spreadsheet-<runbook>.md`, enqueue mit Core-Skill `spreadsheet` (Office-Pack), Lauf `spreadsheets.runbook_runs[<runbook>] = {task_id, status:'queued', …}` | `spreadsheet.runbook.complete` (`result_text`), `spreadsheet.runbook.failed` (`failed`/`cancelled` + `error`); idempotent je task_id, überholte Läufe ohne Wirkung |

Prompts (wörtlich aus `SYSTEMATIC_SPREADSHEET_RUNBOOKS.prompt_template`): `spreadsheet-summarize.md`,
`spreadsheet-audit-formulas.md`, `spreadsheet-risk-review.md`; Rahmen `spreadsheet-runbook.md` (Tabelle, Version,
files-Ref, Auszug ≤ 6000 Zeichen – früher nur JSON-Vorschau von `command_prompt`). `prompt_template` der Konstanten
verweist jetzt auf `prompts/<id>.md`. Kern-Abbau: `command_prompt`-Durchfall für `spreadsheet.*`.
Tests: `handlers/tests/automation.test.mjs` (7). Lauf: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` aus `handlers/`.

## SKL-D: Kontext-Chat über den Baustein `ctox:shared/context-chat.js` (15.10.2026)

Kontextmenü (`renderSpreadsheetsContextMenu`, `dispatchSpreadsheetsContextChat`): vorher `business_os.chat.task` / `ctox.business_os.app.modify`; jetzt `sendContextChat` über `sendSpreadsheetsCommand`, Sprung in den ctox-Chat.

### Manifest-Wünsche (Kern §16.40, Sender-Seite)

```json
{ "commands_out": [{ "app": "ctox", "type": "ctox.chat.task" }, { "app": "ctox", "type": "ctox.app.modify.request" }] }
```
Gegenstück in `modules/ctox/V4-HANDLERS.md` (`commands_in` mit `from:'*'`). Zusätzlich braucht der Nutzer das
Befehlsrecht der Ziel-App ctox (`ctox.app.modify.request`: `apps.source.edit` bzw. owner/admin/chef, im Handler geprüft).
