# CAPSIG – Handler auf Kernform (Strang CAPSIG, Shell v4)

Auftrag: ABWEICHUNG der Aufrufprüfung `node v4/os-primitives/check-caps.mjs --calls` in documents, spreadsheets,
matching, cv-print-builder, interviews, knowledge, kundenpipeline, outbound, research, news auf 0 bringen; die Handler
verwenden die Kernform. Kernfestlegungen: §16.32 (BlobRef = `{ref: files.ref}`; erzeugte Blobs erscheinen als
files-Datensatz), §16.30 (agent.task/agent.queue.task/schedule echte Primitive, flache Argumente, projection.refresh
entfällt). Ausgang 15.10. 16:33: 76 ABWEICHUNG gesamt.

## Zwischenstände

### 17:3x – Vertrag + documents + spreadsheets
- `core-contract.mjs`: §16.32 (BLOB_REF_*), §16.30 (agent.task.*, agent.queue.task.{get,for_command,claim},
  schedule.{upsert,list,remove,set_enabled}, knowledge.query flach, REMOVED projection.refresh, FLAT_ARGS_SOURCE).
  `scan-calls.mjs`: BlobRef-Literal muss genau `{ref}` sein; Kind `flat`; cap1 flach = OK (§16.30 Nr. 5).
- documents: prepare/commit/export/freeze_email_content auf `office.edit.prepare|apply` + `office.render({session,to})`
  (Session immer aus prepare bzw. aus der vorbereiteten Version); blob_id der Versionen bleibt files-id, *_sha256 = ref;
  pdf → `unavailable` vor jedem Aufruf. `doc.extract.verify_source_receipts` → Handler-Logik (`_lib/report.js`
  verifySourceReceipts: Belegzeile über `dataframe.window`, Eignung wörtlich aus store.rs:23289, Snapshot-Bytes =
  files-Datensatz mit ref). Erzeugte DOCX = files-ref, validiert über `office.edit.prepare` (kein Re-Render).
  Tests 23/23 grün (vorher 16).
- spreadsheets: gleiche Umstellung (prepare/commit/export). Tests 7/7 grün (vorher 6).

### 17:27 – matching, cv-print-builder, interviews, knowledge
- matching: Base64/Inline-HTML/Fragmente über `ctx.files.put` als files-Datensatz (`_lib/files.js`), dann
  `doc.extract({blob_id:{ref}, kind, ocr:true})`, `web.fetch({url})`, `doc.html({blob_id:{ref}, selector, attribute?, limit?})`
  je Abfrage (Modi all_text/text/attr/micro im Handler, `_lib/job.js queryJobDom`). Befund: web.fetch liefert
  Seitentext statt HTML → JSON-LD/Meta abgerufener Seiten nicht auswertbar (Titel-Rückfall = vom Kern gemeldeter
  title). Tests 13/13 grün (vorher 12).
- cv-print-builder: `doc.extract({blob_id:{ref}, kind:'pdf'})`; Altpfad desktop_files entfernt → Kern-Import
  desktop_files→files (gleiche id). agent.task.enqueue flach (§16.30) mit on_complete/on_failed =
  `ctox.cv_print.apply_parse.done` (neuer Rückruf-Handler, liest den Ausgangsbefehl über `agent.task.get`).
  Tests 10/10 grün (vorher 8).
- interviews: `speech.stt.transcribe({blob_id:{ref}, model})`, Quelle files (source_file_ref oder Kern-Import-id).
  Tests 3/3 grün (vorher 2).
- knowledge: `dataframe.window({table, skip, limit})`; Filter/Sortierung im Handler über seitenweise gelesene
  Tabelle (readWindow), Simulation verhält sich wie der Kern (nicht-leere filter/sort → invalid). Tests 9/9 grün.

### 17:30 – news (+#778), kundenpipeline
- news: `schedule.ensure` → `schedule.upsert` (§16.30); agent.task.enqueue flach mit Rückruf-Befehl
  `news.task.done` (neu; Ausgangsbefehl über `agent.task.get`, dann onReply/onFailed des Typs). #778
  (e5f057ead, Halte-Projektion) nachgezogen: Rückruf mit Status failed|blocked|cancelled markiert die Ausgabe
  `failed`, Grund = status_note getrimmt, sonst „CTOX hat die Ausgabe nicht abgeschlossen.“ – je Regel ein Test.
  Tests 25/25 grün (vorher 21).
- kundenpipeline: `mail.send({account_id: decisions@<hostname>, to, subject, text, idempotency_key})`;
  delegate über flaches agent.task.enqueue (workspace_root/vorgang_id im payload, idempotency_key eigenes Feld).
  Tests 25/25 grün.

### 17:3x – outbound (+#772), research, Abschluss
- outbound: `knowledge.query({argv})`, `mail.delivery_log.query({message_ids})` (jüngste Zustellung je id im
  Handler), `mail.channel.mailbox.link`, `mail.send` ohne from/tracking. `mail.communication.upsert` /
  `mail.message.annotate` → direkte Schreibzugriffe auf `communication_messages`/`communication_threads` der App
  conversations (foreign_collections read+write, conversations-Exporte um write ergänzt); Begründung: Datenpflege
  fremder Collections, conversations hat weder Handler noch Befehle. #772 (1c269d63a) nachgezogen: Zwei-Anbieter-
  Regel für Personen + Pflichtfelder person_geschlecht/person_titel (`tests/person-rules.test.mjs`, je Regel ein Test).
  Tests 115/115 grün (vorher 112).
- research: agent.task.enqueue flach (idempotency_key, business_command_id), Rückruf über `agent.task.get`,
  `agent.task.workspace.read`, Knowledge-Schreiben über `knowledge.query` (argv wie store.rs:22282), Katalog aus
  `knowledge_tables` (Export der App knowledge), Zeilen über `dataframe.window`, Snapshot = files-ref;
  `knowledge.project` und `projection.refresh` entfernt. Tests 14/14 grün (vorher 12).
- Manifeste (`v4/manifests`): capabilities_required der zehn Apps auf Kernnamen; foreign_collections ergänzt
  (files, knowledge_tables, communication_*), desktop_files/desktop_file_chunks bei cv-print-builder/interviews
  gestrichen. `catalog.mjs`: knowledge.query als OS-Primitiv (dataframe).

## Abnahme (17:3x)
- `node v4/os-primitives/check-caps.mjs --calls`: 176 Aufrufe · ABWEICHUNG 14 · HUELLE 0 · OK 83 – davon **0** in den
  zehn Apps dieses Strangs (Rest: customer-modules/outbound-lead-generation 11, modules/iot 3 – fremde Stränge).
  Generische Kernlücken bleiben keine: §16.30 macht agent.task.*, agent.queue.task.*, schedule.* zu Primitiven.
- Handler-Tests grün: documents 23, spreadsheets 7, matching 13, cv-print-builder 10, interviews 3, knowledge 9
  (+ UI 10), kundenpipeline 25, outbound 115, research 14, news 25.
- `node v4/schemas/check-schemas.mjs`: 4585 grün, 0 rot. `node v4/debt/ratchet.mjs`: OK (35 Zähler gesunken).
  `node v4/os-primitives/check-caps.mjs` (Manifest-Modus): OK.
