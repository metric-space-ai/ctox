# CHATATT – Anhänge im V2-Business-Chat unter Shell v4 (Statusdatei)

Ziel: Büroklammer im Chat-Eingabefeld funktioniert im v4-Rahmen; die Crew-Aufgabe kann die Datei lesen.
`shared/business-chat.js` bleibt byte-gleich. Stub-Pin b6624eb (E30).

## 1. Inventar

### 1a. Schreibweg in shared/business-chat.js

1. Auswahl (Z. 2295–2310): Büroklammer `[data-chat-clip]` → `input[type=file][data-chat-file-input]` (multiple,
   accept `image/*,application/pdf`); ebenso Drop (Z. 2333) und Einfügen (Z. 2363). Je Datei
   `addAttachmentToChatState` (Z. 8881): Grenze 8 MiB, `prepareChatAttachment` (Z. 8860) → `{attachmentId 'chatatt_…',
   fileId 'chatfile_…', generationId 'gen_<ms>_<hash12>', name (bereinigt), mimeType, size, extension, base64Data
   (data:-URL), contentHash = sha256 der Bytes (hex), contentHashScheme 'sha256-bytes-v1', createdAt}`.
   Danach `persistChatState` → der vorgemerkte Anhang steht samt base64Data in `business_chats.<chat>.attachments`
   (V2-Eigenheit, unverändert) und wird als Vorschau (`.ctox-attachment-item`) gezeigt.
2. Senden (`submitChatMessage`, Z. 4196): VOR dem Befehl `stageChatAttachments({db, sync, chat, commandId, messageId,
   attachments})` (Z. 8899):
   - `prepareAttachmentSync`: `sync.startCollection('desktop_files')` (Fehler geschluckt) und
     `sync.leaseCollection('desktop_file_chunks','business-chat-attachment')` – fehlt `leaseCollection`, wirft es
     „desktop_file_chunks requires sync.leaseCollection().“ (heutige Meldung in v4).
   - Collections: `db.collection?.('desktop_files') || db.raw.desktop_files`, ebenso `desktop_file_chunks`; fehlen sie:
     „Business-OS Dateiablage ist nicht verfügbar.“
   - `ensureChatAttachmentRoot`: upsert Ordnerzeile `desktop_files/fs_business_os_chat_attachments`
     (`kind 'folder'`, Pfad `/Business OS Chat`).
   - je Anhang `stageChatAttachment` (Z. 9016): base64 in 16-KiB-Stücke; je Stück upsert `desktop_file_chunks`
     `{id '<fileId>_<generationId>_<idx>', file_id, generation_id, content_hash, content_hash_scheme, idx, total,
     encoding 'base64', data, chunk_hash = sha256(base64-Text), size_bytes}`; dann upsert `desktop_files`
     `{id fileId, parent_id root, path/virtual_path '/Business OS Chat/<chat>/<name>', name, kind 'file', mime_type,
     extension, size_bytes, owner_id, source 'business-os-chat', linked_collection 'business_chats',
     linked_record_id chat.id, content_ref fileId, content_state 'available', content_hash, content_hash_scheme,
     content_generation_id, …}`. Schreibform `upsertRxDocument`: `upsert`, bei Konflikt `findOne().exec()` →
     `incrementalPatch` | `insert`.
   - `flushAttachmentSync` wartet auf die Brücken (`state.awaitInSync`), danach `lease.release()`.
   - Rückgabe je Anhang (von business-chat.js selbst gebaut, nicht aus der DB gelesen): `{kind 'desktop_file',
     storage_collection 'desktop_files', chunk_collection 'desktop_file_chunks', attachment_id, file_id, generation_id,
     name, mime_type, size_bytes, content_hash, content_hash_scheme, virtual_path, chat_id, message_id, command_id,
     content_state 'available'}`.
3. Befehl `business_os.chat.task` (Z. 4206): `payload.attachments` = `payload.attachment_refs` = diese Referenzen,
   `client_context.attachment_count`, `client_context.attachment_storage 'desktop_files'`. Geplante Chats
   (Z. 9362) gleich, nur zeitversetzt.
4. Verlauf: die Nutzernachricht in `business_chats.messages` trägt nur `chatMessageAttachmentSummary` =
   `{name, mime_type, size_bytes}` (Z. 8852) – keine Datei-ID, keine Prüfsumme. `payload.conversation` ebenso.
   Nach erfolgreichem Senden entfernt business-chat.js die vorgemerkten Anhänge aus `chat.attachments`.

### 1b. Empfänger in V2 (Rust, src/core/business_os/store.rs)

`create_ctox_queue_task` (Z. 26701) → `materialize_business_chat_attachments` (Z. 25455): Referenzen aus
`payload|client_context` `.attachment_refs|.attachments` (Objekte mit `file_id`); je Referenz
`materialize_business_chat_attachment` (Z. 25510): liest die `desktop_files`-Zeile per `file_id` aus der
Kern-SQLite (`ctox_business_os__desktop_files__v0`), prüft kind 'file', content_state 'available', Generation,
content_hash, Schema `sha256-bytes-v1`, Größe; setzt die Bytes aus `desktop_file_chunks` derselben Generation
zusammen und prüft sie; schreibt sie nach `runtime/business-os/chat-attachments/<command_id>/<datei>`.
`business_chat_attachment_prompt_manifest` (Z. 27640) hängt an den Queue-Prompt je Anhang
`name, mime_type, size_bytes, local_path, sha256, source desktop_files/<file_id> generation <g>`. Der Agent liest die
Datei über `local_path`. Fehlt die Zeile oder passt die Prüfsumme nicht, scheitert die ganze Aufgabe.

### 1c. Empfänger in v4 (modules/ctox/handlers)

Manifest ctox `handlers['business_os.chat.task']` → `handlers/business_os.chat.task.js` = Re-Export von
`ctox.chat.task.js` → `_lib/context-chat.js enqueueContextTask`. Befund:
- Anhänge werden dort gar nicht gelesen (weder `desktop_files`-ID noch Blob); keine Stelle erwartet eine Desktop-ID.
  → Laut Auftrag ist kein Handler-Eingriff vorgesehen (Bedingung „erwartet heute eine desktop_files-ID“ nicht erfüllt).
- Unabhängig von Anhängen nimmt dieser Handler den V2-Befehl nicht an: `enqueueContextTask` wirft für
  `command.type === 'business_os.chat.task'` „unknown context chat command“ (nur `ctox.chat.task`), und
  `contextRequest` verlangt `payload.source_app` + `payload.text`, die business-chat.js nicht sendet
  (`source_module`, `prompt`, `instruction`). CHAT4-Tests ersetzen den Empfänger per `kernel.sim.onCommand`.
  Offene Frage an den ctox-Strang (siehe §5).

### 1d. v4-Mittel

- `api.files.put(bytes, {name, mime})` → `{id, ref}`, `ref` = sha256 der Bytes (hex) = V2 `content_hash`
  (gleiches Schema `sha256-bytes-v1`: Hash über die Rohbytes).
- E30: `files.get` ist für eine fremde App erlaubt, wenn ein BlobRef (`{ref: <sha>}` bzw. `{blob_ref: <sha>}` irgendwo
  im Dokument) in einem für sie lesbaren, kanonischen (nicht nur optimistischen) Dokument steht. `business_chats`
  gehört ctox → ein BlobRef im Chat-Dokument macht die Datei für ctox lesbar.
- Die Befehlsnutzlast liegt in `business_commands`; ctox liest dort nur eigene Befehle → die Nutzlast allein erteilt
  ctox kein Leserecht. Der BlobRef muss deshalb im Chat-Dokument stehen.

## 2. Entwurf

Kleinste treue Abbildung, nur in `v4/shell/chat/`:

- `raw-db.js` bekommt zwei Schreibattrappen `desktop_files` und `desktop_file_chunks` (nur `upsert/insert/findOne`,
  nur Speicher, kein Lesen/Schreiben fremder Kern-Collections):
  - Stücke sammeln je `file_id`+`generation_id`;
  - Ordnerzeile: angenommen, nichts geschrieben;
  - Dateizeile (`kind 'file'`): Stücke vollständig (`total`), base64 → Bytes, `sha256(bytes) === content_hash` und
    `size_bytes` prüfen, `api.files.put(bytes, {name, mime})`, `ref === content_hash` prüfen, dann BlobRef
    `{ref, id, name, mime, size, client_file_id}` in `business_chats/<linked_record_id>.attachment_blobs`
    (lokal committet, vor dem Befehl). Jeder spätere `put` von business-chat.js auf diesen Chat behält die Liste
    (Vereinigung aus Speicher + Dokument).
- `index.js`: `sync` um `leaseCollection(name)` ergänzt (nur `desktop_file_chunks`; Griff ohne Netz, `release()`
  leer); `commandBus.dispatch` ersetzt in `payload.attachments|attachment_refs` jede `desktop_file`-Referenz durch die
  v4-Form `{kind 'blob', ref, blob_id, attachment_id, name, mime_type, size_bytes, content_hash, content_hash_scheme,
  chat_id, message_id, command_id, content_state, blob_doc 'business_chats/<chat>'}` (ohne `file_id`, damit kein
  V2-Leser nach einer nicht existierenden desktop_files-Zeile sucht) und setzt `client_context.attachment_storage`
  auf `'files'`. Fehlt zu einer Referenz der Blob, scheitert das Senden (kein stiller Verlust).
- Kein v2-compat-Eingriff, kein Handler-Eingriff.

## 3. Umsetzung

- `v4/shell/chat/raw-db.js`: Schreibattrappen `desktop_files`/`desktop_file_chunks` (nur wenn business_chats
  beschreibbar und `api.files.put` da), Prüfung sha256 + Größe + vollständige Stücke, `api.files.put`, BlobRef in
  `business_chats.<chat>.attachment_blobs` (vorhandenes Dokument erweitert, sonst Mindestdokument); jeder `put` auf
  business_chats vereinigt die Liste; `blobForClientFile(fileId)` für die Nutzlast.
- `v4/shell/chat/index.js`: `sync.leaseCollection('desktop_file_chunks')` (Griff ohne Netz, alles andere `forbidden`),
  `commandBus.dispatch` schreibt `payload.attachments|attachment_refs` auf die BlobRef-Form um,
  `client_context.attachment_storage = 'files'`. Wächter-Marker `// v4-allow: debt L-42` an den V2-Namen.
- `v4/shell/chat/tests/attachment.test.mjs` (neu), Modi `counter` (HEAD-Stand von index.js/raw-db.js) und `noref`
  (Adapter ohne BlobRef im Chat-Dokument).
- Nicht angefasst: `shared/**` (business-chat.js byte-gleich), v2-compat, `modules/ctox/handlers/**`, Manifeste.

## 4. Messungen (Stub b6624eb)

| Lauf | Ergebnis |
|---|---|
| `node v4/shell/chat/tests/attachment.test.mjs` | 10/10 |
| `… attachment.test.mjs counter` (ohne Adapter) | 5/10: rot „keine Fehlermeldung“ (Chat zeigt „Die Aufgabe konnte nicht an die Crew übergeben werden: desktop_file_chunks requires sync.leaseCollection().“), „ein Befehl“ (0), „BlobRef in der Nutzlast“, „BlobRef im Chat“, „files.get als ctox“ (forbidden) |
| `… attachment.test.mjs noref` (Adapter, kein BlobRef im Chat) | 8/10: genau „BlobRef im Chat“ und „files.get als ctox“ rot (forbidden) – die Nutzlast allein erteilt kein Leserecht, die Lesbarkeit kommt aus business_chats (E30) |
| `node v4/shell/chat/tests/chat.test.mjs` | 14/14 |
| `node v4/shell/chat/tests/research-handoff.test.mjs` | 16/16 |
| `node v4/debt/ratchet.mjs` | OK (guard 1838 · scan 3) |
| `node v4/tools/guards/guard-v4.mjs` | 1838, `v4/shell/chat/` 0 Treffer |
| `git diff --quiet HEAD -- shared/business-chat.js` | 0 |

Bilder: `tests/shots/att-1280-staged.png` (Vorschau mit Bild-Miniatur über dem Eingabefeld),
`att-1280-sent.png` (nach dem Senden; Verlauf zeigt die Nachricht, V2 zeigt Anhänge in gesendeten Blasen nicht an;
die verbreiterte Statuszeile ist die bekannte V2-Eigenheit aus CHAT4.md).

## 5. Offen / benötigte Deklarationen

- Deklarationen: keine neuen. Shell schreibt nur die eigene Dateiablage (`api.files`) und business_chats (bereits
  SHELL_FOREIGN read+write). Falls der echte Kern `files.put` hinter eine Fähigkeit stellt, braucht die Shell sie im
  generierten Manifest (im Stub nicht nötig).
- Kern-Frage (Reihenfolge): der BlobRef im Chat ist vor `api.command` lokal committet. E30 verlangt ein kanonisches
  Dokument; im Stub ist es sofort kanonisch. Garantiert der echte Kern, dass ein vor dem Befehl geschriebener
  business_chats-Stand kanonisch ist, bevor der ctox-Handler bzw. der Agent liest?
- ctox-Strang (nicht Teil dieses Auftrags, vorbestehend): `handlers/business_os.chat.task.js` →
  `enqueueContextTask` nimmt den V2-Befehl nicht an (Typprüfung nur `ctox.chat.task`; verlangt `source_app`/`text`).
  Anhänge reicht der Handler ohnehin nicht weiter. Damit die Crew die Datei liest, muss der Handler
  `payload.attachments` (BlobRefs: `ref`, `name`, `mime_type`, `size_bytes`, `blob_doc`) in Task-Payload/Prompt geben
  und die Agentenseite die Bytes per `files.get({ref})` (E30) holen – Ersatz für V2 `local_path` aus
  `materialize_business_chat_attachment`.
- V2-Eigenheit unverändert: der vorgemerkte Anhang steht bis zum Senden samt base64Data (bis 8 MiB) in
  `business_chats.<chat>.attachments`. Dokumentgrößen-Grenze des Kerns prüfen.
- `attachment_blobs` wächst je Chat; Chat löschen entzieht ctox das Leserecht (E30), der Blob bleibt in der
  Shell-Dateiablage (kein Aufräumen). Bricht das Senden nach `files.put` ab, bleibt ein Blob ohne Befehl.

## Stand

- [x] Inventar
- [x] Adapter
- [x] Tests (a)–(b) + Gegenproben counter/noref + Bestandstests
- [x] Bild angesehen
