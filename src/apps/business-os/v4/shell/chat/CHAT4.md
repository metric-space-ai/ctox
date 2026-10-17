# CHAT4 – V2-Business-Chat in Shell v4 (Statusdatei)

Ziel: `shared/business-chat.js` (`initBusinessChat`) UNVERÄNDERT im v4-Rahmen montieren, über `createV2Compat`
(`v4/shell/workjet/v2-compat.js`) für App `shell`. Stub-Pin b6624eb. `shared/business-chat.js` byte-gleich zu HEAD
(`git diff --quiet HEAD -- shared/business-chat.js`).

## 1. Inventar (shared/business-chat.js, Stand dieses Checkouts)

Aufruf `initBusinessChat({session, commandBus, db, sync, getActiveModule})` (Z. 708). Abbruch ohne
`session.authenticated` (Z. 715). Eigentümer der Chats = `session.user.id` (`ownerUserId`, Z. 5539).

### Collections (alle über `db.raw.<name>`, nie über `db.collection`, außer Anhängen)

| Collection | Zugriff | Zeilen | Form |
|---|---|---|---|
| `business_chats` | lesen + schreiben | 44, 1193, 5163, 5267, 5350 | `find({selector:{},limit:200}).exec()` → Docs mit `toJSON()`; `find(..).$.subscribe(docs[])`; `findOne(id).exec()` → `toJSON/incrementalPatch/remove`; `insert(doc)` (auch Grabstein `_deleted:true`) |
| `business_commands` | lesen | 1292, 4327, 4347 | `$.subscribe(fn,{invalidateOnly})`; `find({selector:{id:{$in}}})` |
| `ctox_queue_tasks` | lesen | 371, 535, 1293, 4328, 4348 | `$.subscribe`; `find({selector:{id/command_id:{$in}, status:{$in}, updated_at_ms:{$gt}}})` |
| `ctox_crew_members` | lesen | 74, 545, 912, 4383 | `find({selector:{archived:false},limit:64})`, `$.subscribe` |
| `ctox_harness_status` | lesen | 359, 536 | `find({selector:{id:'harness'},limit:1})`, `$.subscribe` |
| `ctox_harness_events` | lesen | 4412 | `crewSelectionReason` (find) |
| `desktop_files`, `desktop_file_chunks` | schreiben | 8904–8953, 9107–9118 | nur Anhänge: `upsert/insert/incrementalPatch` + `sync.leaseCollection('desktop_file_chunks')` |

Live-Start: `sync.startCollection(name)` für business_chats, business_commands, ctox_queue_tasks, ctox_crew_members,
ctox_harness_status (Z. 60, 5222) und vor jedem Senden business_commands/ctox_queue_tasks (Z. 4667).
Optional (try/optional chaining): `sync.subscribeCollectionReadiness`, `sync.subscribeCollectionFreshness`.
Globale Hooks (optional, in v4 nicht vorhanden): `window.CTOX_BUSINESS_OS_APP.eventBus/openCrewContextMenu`.

### Befehle

| Typ | Zeile | Weg |
|---|---|---|
| `business_os.chat.task` (Standard, `meta.command_type` überschreibt) | 4127, 4269 | `commandBus.dispatch({id, module, type, record_id: chat.id, inbound_channel, payload, client_context})` |
| geplante Chats (Scheduler) | 9374 | `commandBus.dispatch(command)` |

Kein `requestNative`. Empfänger im v4-Bestand: `modules/ctox/handlers/business_os.chat.task.js` (Übergang L-35,
identisch zu `ctox.chat.task`), Manifest ctox `commands_in from:"*"`. GEMESSEN (chat.test.mjs): genau ein Befehl
`business_os.chat.task`, App `shell`, `record_id` = Chat-ID.

### Abbildung über v2-compat (Befund)

- `commandBus.dispatch` → `compat.state.commandBus.dispatch` (liest `command.type`, `command.id` → idempotency_key,
  `record_id`). Lücke: die V2-Quittung trägt `task_id` oben (shared/command-bus.js commandReceipt), v2-compat legt die
  Antwort des Empfängers unter `result`. Ohne `task_id` wartet business-chat 12 s und meldet „Die Annahme der Aufgabe
  wurde noch nicht bestätigt.“ Adapter hebt `result.task_id`/`result.status` in die Quittung (kein neuer Wert).
- `sync.startCollection` → `compat.state.sync.startCollection` passt (`waitForSyncBridgeReady` nutzt `state.awaitInSync`).
- `session` → v2-compat liefert `{kernel_instance_id, user_id}` ohne `authenticated`/`user`: Adapter ergänzt aus `api.actor`.
- `db.raw` FEHLT in v2-compat (nur `db.collection(name)`, nur lesen, Zeilen ohne `toJSON`, `$` je Zeile, kein
  `find().$`). Adapter `raw-db.js`: Lesen über `compat.state.db.collection` (exec) bzw. `compat.api.query` (Live),
  Schreiben über `compat.api.put/remove`, nur `business_chats`.
- Anhänge (`desktop_file_chunks` + `sync.leaseCollection`): NICHT abbildbar (v4 kennt keine Chunk-Collection; Weg wäre
  `api.files.put`). Fluchtweg: nicht umgangen; `leaseCollection` fehlt, Anhang-Senden scheitert mit der V2-Meldung
  „desktop_file_chunks requires sync.leaseCollection().“ Text-Chat ist davon unberührt.
- CSS-Kontext: business-chat schreibt seine Regeln selbst (installChatStyles), braucht aber Tokens/Typografie/Regeln
  aus app.css (V2 :root). Ohne sie: Schrift Times, Fenster dunkel (rgb 28,31,37 statt weiß), 420 px falsch versetzt.
  Lösung wie FR6d: `chat-v2.css` generiert (tests/extract-chat-css.mjs, 51 Regeln, 68 Tokens) aus ui-contract,
  app.css, mobile-host.css, Hülle `[data-v4-chat-v2]`; die Leiste hängt in dieser Hülle.

## 2. Umsetzung

- `v4/shell/chat/index.js` – `mountBusinessChat({kernel, shellVer, getActiveModule})`: createV2Compat('shell'),
  Sitzung aus `api.actor`, Quittung mit `task_id`, Hülle + `chat-v2.css`, eine Leiste je Dokument (neuer Rahmen
  übernimmt, `unmount()` nur durch den aktiven Griff).
- `v4/shell/chat/raw-db.js` – RxDB-förmige Fassade (nur die Formen aus dem Inventar).
- `v4/shell/chat/chat-v2.css` – generiert; `tests/extract-chat-css.mjs` erzeugt es neu.
- `v4/frame/index.js` – `syncBusinessChat()` in `applyApps`: Leiste steht, solange App `ctox` aktiv ist (Eigentümer von
  business_chats, Empfänger von business_os.chat.task); montiert → `crewBar.handOver(true)`; ohne ctox → abbauen,
  Crew-Leiste zurück. `getActiveModule` = fokussiertes Fenster. Abtretender Rahmen baut die Leiste erst bei
  `v4:retire` ab, und nur, wenn kein neuer Rahmen sie übernommen hat.
- `v4/frame/crew-bar.js` – `handOver(active)`, `handedOver`.
- `gen-manifest.mjs SHELL_FOREIGN` – business_chats (ctox, read+write), business_commands + ctox_queue_tasks
  (ctox-platform, read), ctox_crew_members, ctox_harness_status, ctox_harness_events (ctox, read); neu erzeugt.

## 3. Messungen

| Lauf | vorher | nachher |
|---|---|---|
| `node v4/frame/lab/frame.test.mjs stub` | 219/220 (rot: K-1k Monaco-Worker `workerUrl is not a function`, Stub 68b83a3) | 219/220, derselbe eine Rote |
| `node --test v4/shell/workjet/tests/*.test.mjs` | 302/304 (rot: Strang R content.read, narration.read) | 302/304, dieselben 2 |
| `node v4/debt/ratchet.mjs` | OK (guard 1838 · scan 3) | OK (guard 1838 · scan 3) |
| `node v4/tools/guards/guard-v4.mjs` | 1838 Treffer | 1838 Treffer (neue Dateien 0) |
| `node v4/tools/manifest/gen-manifest.mjs --check` | 0 | 0 nach dem Neuerzeugen; später 1 nur durch fremde, laufende Änderungen an modules/mail/* (mail.manifest.json, REPORT-M2.md), nicht aus CHAT4 |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 | 38/38 |
| `node v4/shell/chat/tests/chat.test.mjs` | – | 14/14 (inkl. Shell-Tausch) |
| `node v4/shell/chat/tests/chat.test.mjs counter` (ohne Montage) | – | 3/14 grün (nur Überlauf, Rückgabe, Seitenfehler); alle (a)–(c) rot |
| `node v4/shell/chat/tests/geometry.test.mjs` (gegen V2) | 9/20 vor chat-v2.css | 30/30 |

Bilder: `v4/shell/chat/tests/shots/` (chat-1280-*, chat-420-open, geometry-{1280,420}-{v4,v2}[-sent]).

## Stand

- [x] Inventar
- [x] Adapter + Montage
- [x] SHELL_FOREIGN + Regenerieren
- [x] Tests + Gegenprobe + Geometrie gegen V2
- [x] Rahmenlabor nachher: 219/220 wie vorher

## Offen

- Anhänge: kein v4-Weg (desktop_file_chunks/leaseCollection); Brücke auf api.files.put wäre ein eigener Auftrag.
- Verfolgung: business-chat sucht business_commands über die V2-Befehls-ID (cmd_…), der Kern vergibt eine UUIDv8 als
  Dokument-ID; die Antwort kommt trotzdem über business_chats (ctox.chat.task.complete), die Statussuche über die
  Befehls-ID trifft aber nie.
- V2-Eigenheit (unverändert übernommen, geometry.test belegt Gleichheit): nach dem Senden verbreitert die lange
  Statuszeile den Verlauf über das Chatfenster hinaus (V2 und v4: messages 594 px breit bei 460 px Fenster).
- Fremd-App öffnet Chat mit Entwurf: Vorschlag `api.shell.openChat(spec)` im Fenster-Shell-API des Rahmens (siehe Bericht).
