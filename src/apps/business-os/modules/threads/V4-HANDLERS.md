# threads – Fachlogik als JS-Handler (Shell v4, Strang H2)

Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/threads.rs` (8 146 Z.; 5 248 Prod.),
Router `handle_business_command` `threads.rs:1090-1132`, Zulassung `command_plane.rs:1224,1428,1513,2355`,
Workjet-Vorspann `project_chats/privacy.rs:473`, Supervisor-Bindung `project_chats/supervisor_binding.rs:133` (beide seit Strang WJAPP
App-Logik, §6),
Speicher `store_projections.rs:1397` (`upsert_business_record`), `store.rs:11392` (`pull_collection_record`).

Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`.
Prüfen: `cd src/apps/business-os/modules/threads/handlers && node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` → **64/64 grün** (Strang SKL-A2, 15.10.; vorher 57/57 WJAPP, 46/46 um 13:24). `--import` ist Pflicht, weil `_agent.js` `ctox:shared/prompt-template.js` lädt.

Fortschritt (Berlin): 12:46 Start · 13:01 Gerüst + Tabelle · 13:04 Nachrichten/Notizen/Benachrichtigungen (4) ·
13:06 Freigaben (5) · 13:10 übrige 14 · 13:12 Vertrags- und vm-Test · 13:15 Dokument · 13:18 Projektion CTOX-Relevanz ·
13:20 Projektion App-Relevanz · 13:22 Sichtbarkeitsregel · 13:24 Abschluss.

## 1. Befehlstypen (23) – Stand

Reihenfolge = Nutzen: UI-Sender in `modules/threads/*.js` (Aufrufstellen) + Fremdsender (`app.js`, `m/reports`), dann
Abhängigkeit. **Portiert 23/23 · Legacy 0 · Fähigkeitsteile über ctx.command/ctx.cap (siehe §3).**

| # | Befehlstyp | Einstufung (E) | Rust-Ort | UI-Sender | Ziel | Handler | Tests |
|---|---|---|---|---|---|---|---|
| 1 | `threads.message.create` | FACHLOGIK | `threads.rs:1330-1417` | m/threads ×2 | Handler | ✅ `threads.message.create.js` | messages ×3 |
| 2 | `threads.note.create` | FACHLOGIK | `:1134-1235` | m/threads ×1 | Handler | ✅ | messages ×6 |
| 3 | `threads.notification.mark_read` | FACHLOGIK | `:2395-2439` | m/threads ×1 | Handler | ✅ | messages ×2 |
| 4 | `threads.notification.dismiss` | FACHLOGIK | `:1124 → :2395` | m/threads ×1 | Handler | ✅ | messages ×1 |
| 5 | `threads.ctox_approval.request` | FACHLOGIK | `:1419-1537`, `:3394` | m/threads, app.js, m/reports | Handler | ✅ | approvals ×2 |
| 6 | `threads.ctox_approval.approve` | GEMISCHT | `:1644-1752`, `:3246-3392`, `:3448` | m/threads, app.js | Handler + `ctx.command` + `cap.audit` | ✅ | approvals ×2 |
| 7 | `threads.ctox_approval.reject` | FACHLOGIK | `:1754`, `:1779-1888` | m/threads, app.js | Handler + `cap.audit` | ✅ | approvals ×1 |
| 8 | `threads.ai.request` | GEMISCHT | `:2212`, `:2239-2313` | m/threads ×2 | Handler + `ctx.command` | ✅ | threadops ×1 |
| 9 | `threads.handoff.create` | FACHLOGIK | `:2143-2210` | m/threads ×1 | Handler | ✅ | threadops ×2 |
| 10 | `threads.thread.claim` | FACHLOGIK | `:1111 → :2078` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 11 | `threads.thread.watch` | FACHLOGIK | `:1890-1936` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 12 | `threads.thread.unwatch` | FACHLOGIK | `:1106 → :1890` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 13 | `threads.thread.snooze` | FACHLOGIK | `:1973-2009` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 14 | `threads.thread.archive` | FACHLOGIK | `:1938-1971` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 15 | `threads.ctox_approval.edit` | FACHLOGIK | `:1539-1642` | m/threads ×1 | Handler | ✅ | approvals ×1 |
| 16 | `threads.notification.preferences.update` | FACHLOGIK | `:2441-2506` | m/threads ×1 | Handler | ✅ | threadops ×1 |
| 17 | `threads.thread.assign` | FACHLOGIK | `:2078-2141` | – (intern von handoff) | Handler | ✅ | threadops ×1 |
| 18 | `threads.ctox_approval.cancel` | FACHLOGIK | `:1762-1777` | – | Handler + `cap.audit` | ✅ | approvals ×1 |
| 19 | `threads.note.update` | FACHLOGIK | `:1237-1299` | – | Handler | ✅ | threadops ×1 |
| 20 | `threads.note.delete` | FACHLOGIK | `:1301-1328` | – | Handler | ✅ | threadops ×1 |
| 21 | `threads.thread.state.update` | FACHLOGIK | `:2011-2076` | – | Handler | ✅ | threadops ×1 |
| 22 | `threads.link.create` | FACHLOGIK | `:2315-2367` | – | Handler | ✅ | threadops ×1 |
| 23 | `threads.link.remove` | FACHLOGIK | `:2369-2393` | – | Handler | ✅ | threadops ×1 |

Gemeinsame Teile: `handlers/_lib.js` (Port von `threads.rs:3500-4062` Upserts + `refresh_thread_states`, `:4510-4670`
Freigabe-Prüfungen, `:4749-5245` Werkzeuge). Mehrfach genutzte Kerne werden aus Handler-Dateien benannt exportiert:
`createMessage` (message.create → handoff, ai.request), `assignThread` (assign → claim, handoff),
`updateNotificationStatus` (mark_read → dismiss), `updateThreadWatch` (watch → unwatch),
`decideWithoutQueue` (reject → cancel), `approvalRecord` (request), `enqueueApprovedCommand` (approve).

Übertragung der Rust-Testfälle (gleiche Eingaben, gleiche Erwartungen):
`mark_read_recomputes_unread_count_and_attention_score` (:6048), `addressed_note_preserves_thread_assignment_and_source_state`
(:6126), `handoff_rejects_invalid_target_and_stale_thread_before_writing_message` (:5730),
`approval_request_is_not_a_queue_task_until_reviewed` (:5850), `pending_approval_attention_is_scoped_to_reviewer` (:5915),
`unauthorized_actor_cannot_approve_and_request_stays_pending` (:5979), `approval_request_requires_active_reviewer_and_blocks_self_review`
(:6222), `rejected_approval_creates_no_queue_task` (:6291), `approved_request_creates_command_task_and_audit_linkage` (:6364),
`stale_approval_decision_is_rejected` (:6527), `approval_edit_cannot_change_target` (:6591). Nicht übertragbar (Core-Sache):
`approval_execution_uses_central_command_policy` (:6667 – zentrale Policy des eingereihten Befehls, im Test nur als
`ok:false/status:failed` des `ctx.command` nachgestellt), alle Replikations-/Peer-Write-Tests (:5256-5629, :6763-7560).

### Fehlercodes (Rust kennt nur anyhow-Texte; Texte 1:1 übernommen)

| Code | Fälle |
|---|---|
| `invalid` | Pflichtfeld fehlt (`required text field is missing`, `thread_id is required`, `… is required`), `invalid thread message kind`, `invalid priority`, `future snoozed_until_ms is required`, Ruhezeiten/Schwelle, Reviewer nicht aktiv/unbekannt, Selbstfreigabe, Ziel nach Antrag geändert, `non-internal AI actions …`, `threads commands require module=threads` |
| `forbidden` | Nicht-Teilnehmer, Nicht-Autor, Nicht-Besitzer der Benachrichtigung, Nicht-Reviewer, Nicht-Antragsteller (cancel), Policy-Verweigerung (Quelle/Ziel), zentrale Ablehnung des freigegebenen Befehls, Workjet-Privatchat (`Workjet chat is unavailable to this user or instance`, `… cannot add another human participant`, `… cannot assign another human participant`, §6) |
| `not_found` | Thread/Nachricht/Benachrichtigung/Verknüpfung/Freigabe fehlt |
| `conflict` | `approval request changed; reload before deciding`, `approval request is not pending`, `thread changed; refresh …`, `thread was already claimed`, Supervisor-Bindungskonflikt |
| `unavailable` | Capability fehlt, `approved CTOX command did not return command id` |

## 2. Unklarheiten und bewusste Abweichungen (markiert im Code als U-n)

- **U-1 Akteur**: v4-`command.actor` = `{id, role}`. Rust nutzt zusätzlich `display_name` (für `author_display_name`,
  `requester_display_name`, `actor_payload`) und `is_admin`. Handler lesen `actor.display_name`/`actor.is_admin`, falls
  vorhanden; sonst Rust-Rückfall (`display_name = id`, Admin nur bei Rolle `chef|admin`). → Vertragswunsch W-2.
- **U-2 IDs**: Rust `Uuid::new_v4` (`msg_…`, `approval_…`, `cmd_…`, `thread_…`). Vertrag hat keinen ID-Erzeuger;
  `newUuid` nimmt `ctx.uuid()` (Wunsch W-3), sonst `crypto.randomUUID`, sonst deterministisch `command.id`+Zähler.
- **U-3 Befehlsfelder**: Rust nutzt `command.record_id` (Rückfall für thread/message/notification/link/approval-ID; die UI
  sendet `recordId`), `command.module` (muss `threads` sein) und `command.client_context` (Quellkontext-Rückfall,
  Freigabe-Datensatz). Der feste Vertrag kennt sie nicht → Handler lesen sie optional; `module` fällt auf `app_id`
  zurück, `client_context` fehlt → `null` im Freigabe-Datensatz. → Vertragswunsch W-1. Leeres `record_id` bleibt wie
  in Rust „vorhanden“ (führt zu `not_found` statt `invalid`).
- **U-4 Atomarität**: Rust schreibt jeden Datensatz einzeln (keine Transaktion); ein Fehler mitten im Ablauf (z. B.
  `refresh_thread_states` findet den Thread nicht) hinterlässt Teilschreibungen. Handler bündeln die Schreibschritte
  je Ablauf in `ctx.transaction` → Fehler rollen zurück. Bewusste Abweichung zugunsten Konsistenz; Prüfungen laufen in
  derselben Reihenfolge vor dem ersten Schreiben wie in Rust.
- **U-5 Audit-Reihenfolge**: Rust schreibt `business_os.external_approval.decided` direkt nach dem Freigabe-Datensatz;
  der Handler ruft `ctx.cap.audit.event` nach dem Commit (Capabilities nicht innerhalb der Transaktion).
- **U-6 mark_read/dismiss** ohne Thread (`thread_id` leer): Rust scheitert nach dem Schreiben, Handler rollt zurück.
- **U-7 handoff**: wie Rust drei getrennte Schritte (Nachricht, Zuweisung, Neuberechnung), keine gemeinsame Transaktion
  (verschachtelte Transaktionen sind im Vertrag nicht definiert). Alle Prüfungen vor dem ersten Schreiben.
- **U-8 ai.request**: Rust prüft `risk_class` **nach** dem Schreiben der Nachricht → nicht-interne Anfrage hinterlässt
  die Nachricht und scheitert. Beibehalten (Test belegt es); Kandidat für fachliche Korrektur durch den Owner.
- **U-9 state.update** übernimmt `pinned/unread_count/last_seen_at_ms/snoozed_until_ms/follow_up_at_ms` ungeprüft (auch
  `null`/falscher Typ), anschließend überschreibt `refresh_thread_states` Zähler und Aufmerksamkeit. Wie Rust.
- **U-10 link.create**: `source_context` füllt `module` immer (Rückfall `threads`), die Pflichtprüfung „module oder
  record_id“ greift deshalb praktisch nie. Wie Rust.
- **U-11 Lesen**: `refresh_thread_states` braucht alle Nachrichten/Benachrichtigungen/Freigaben eines Threads. Handler lesen
  seitenweise (`sort:[{id:'asc'}]`, `skip`, `limit:200`), damit eine 200er-Kappung von `ctx.read` (heutige Browser-API)
  nichts abschneidet (Test mit 450 Benachrichtigungen). Braucht `skip` in `ctx.read`.
- **U-12 Projektionsliste**: Ergebnis enthält wie Rust `projections:[{collection, record_id}]` (dedupliziert). In v4
  redundant (Schreiben ist direkt), aber unverändert für Aufrufer.
- **U-13 Soft-Delete**: `note.delete`/`link.remove` setzen `is_deleted:true` (kein `ctx.remove`), wie Rust; `ctx.get`
  muss solche Dokumente weiter liefern (Rust `load_record` liefert sie). `_deleted:true` (RxDB) gilt als nicht vorhanden.
- **U-14 Rust-Feinheiten treu übernommen**: `serde`-„vorhanden“ zählt auch `null` (`firstPresent`/`has`), `as_i64` nur
  Ganzzahlen, `u8::parse` akzeptiert führendes `+` (Ruhezeit `+7:00` gültig), Sortierung nach Codepunkten, `slug_part`/
  `truncate` zeichenweise; `message.create` setzt `kind:'thread'`, `status:'open'` und `assigned_user_id` = erster
  Adressat (oder leer!), Thread-Titel ohne Trim; `note.create` erhält Titel/Art/Status/Zuständigen.
- **U-15 Redaktion**: Rust entfernt `capability_token` u. a. aus `client_context` beim Speichern
  (`redact_document_client_context_secrets`, `store.rs:27329`) – das bleibt Schreibpfad des Core, nicht Handler.

## 3. Capability- und Vertragswünsche (an „ctox sync v4“)

| Wunsch | Signatur | Beispielaufruf | Nutzer (Handler) | Rust-Herkunft |
|---|---|---|---|---|
| **W-1 `ctx.command` mit Optionen + Annahme-Ergebnis** | `command(type, payload, {id?, module?, record_id?, client_context?}) → {ok?, id\|command_id, status, task_id?, task_status?}`; Core prüft zentrale Befehlspolicy des Zielbefehls; `status:'failed'` statt throw bei Policy-Ablehnung | `await ctx.command('business_os.chat.task', payload, {id, module:'support', record_id, client_context})` | approve, ai.request | `store::accept_rxdb_business_command_with_origin(…, TrustedLocal)` (`threads.rs:3341`, `:2302`) |
| **W-1b Befehlsfelder** | `command.record_id?`, `command.module?`, `command.client_context?` (ohne Geheimnisse) im Handler-Befehl | – | alle 23 | `BusinessCommand` (`store.rs:575`) |
| **W-2 Akteur vollständig** | `command.actor = {id, role, display_name, is_admin}` | `command.actor.display_name` | alle schreibenden | `BusinessOsSession.user` (`threads.rs:5154-5201`) |
| **W-3 `ctx.uuid()`** | `uuid() → string` (v4) | ``const id = `msg_${ctx.uuid()}` `` | 7 Handler-Dateien (msg_/approval_/cmd_/thread_-IDs) | `Uuid::new_v4` |
| **`ctx.cap.directory.user`** | `directory.user({user_id}) → {user_id, display_name, active} \| null` | `await ctx.cap.directory.user({user_id:'lead'})` | approval.request, thread.assign, thread.claim, handoff.create | `active_business_user_display_name` → `business_users` (`threads.rs:4516`) |
| **`ctx.cap.policy.module_decision`** | `policy.module_decision({permission:'data.read'\|'data.write'\|'apps.modify', module}) → {allowed, display_reason}` (Akteur = Befehlsakteur) | `await ctx.cap.policy.module_decision({permission:'data.read', module:'support'})` | note.create, message.create, approval.request, link.create, handoff/ai.request (über message), approve | `store::module_policy_decision` (`threads.rs:4612`, `:4651`) |
| **`ctx.cap.audit.event`** | `audit.event({collection, record_id, event_type, payload, observed_at_ms}) → {ok}` | `await ctx.cap.audit.event({collection:'ctox_task_approval_requests', record_id, event_type:'business_os.external_approval.decided', payload, observed_at_ms})` | approve, reject, cancel | `store::insert_business_event` (`threads.rs:3448`) |
| ~~workjet.command_access_check~~ | **entfallen** (Strang WJAPP) | – | – | jetzt threads-Fachlogik `handlers/_chat_access.js` über foreign_collections, §6 |
| ~~workjet.supervisor_binding~~ | **entfallen** (Strang WJAPP) | – | – | jetzt Lesezugriff auf foreign_collection `workjet_supervisor_bindings` in `_lib.supervisorBindingForThread`, §6 |
| **`ctx.read` mit `skip`** | Mango `{selector, sort, skip, limit}` | siehe U-11 | `_lib.threadCollectionRecords` | `thread_collection_records` (`threads.rs:3963`) |

Kein `ctx.model`, kein `ctx.http`, keine Datei-/Prozess-Capability nötig. `ctx.cap.agent.task` aus E §3 wird für
threads **nicht** gebraucht: `ai.request` und `approve` reihen wie Rust einen `business_commands`-Befehl ein
(`business_os.chat.task` bzw. den freigegebenen Zieltyp); erst der Core macht daraus den Agentenauftrag. Das hält die
Projektion `business_commands → user_threads` (§4) intakt, die `payload.thread_id`/`thread_key` des Befehls liest.

Zulassung vor dem Handler (Core, Manifest-Deklaration statt Handler-Code): `approve`/`reject` verlangen
`external.approve` im Bereich `approval:<id>` mit `assigned_to_actor = (reviewer == Akteur)`
(`command_plane.rs:1428-1546`); alle übrigen `threads.*` brauchen nur eine authentifizierte Sitzung (`:1224`).

## 4. Nicht-Befehls-Rust in threads.rs

| Bereich | Rust-Ort | Zeilen | Einstufung | Stand |
|---|---|---|---|---|
| Projektion `business_commands` + `ctox_queue_tasks` → Threads („CTOX-Relevanz“) | `:966-1042`, `:2508-2688`, `:4064-4508` | ~700 | FACHLOGIK (Projektion) | ✅ portiert `handlers/_projection.ctox_relevance.js` (Rust-Tests :7560, :7614, :7763 übertragen, inkl. „Wiederholung ändert nichts“); Aufruf braucht **W-4** |
| Projektion App-Datensätze → Threads (14 Collections: kundenpipeline, tickets ×4, support ×3, outbound ×2, research ×2, documents, notes) | `:44-150`, `:1044-1088`, `:2689-3245` | ~720 | FACHLOGIK (Projektion) | ✅ portiert `handlers/_projection.app_relevance.js` (Rust-Tests :7870, :7939, :7957, :8006, :8085 übertragen); braucht **W-4** + `foreign_collections` read für 14 Collections und `exports` der Eigentümer-Apps (§16.13) |
| Sichtbarkeit der 6 threads-eigenen Collections je Nutzer | `:575-620`, `:4781-4812` | ~70 | FACHLOGIK der App, durchgesetzt vom Core | ✅ Kandidat `handlers/_visibility.js` (`visible(collection, doc, user, read)`); braucht **W-5** |
| Übrige Replikationssichtbarkeit in threads.rs (guest_frames, Browser, communication_*, business_commands/ctox_queue_tasks über Eltern-Befehle, Workjet-Privatsphäre, Rollen/Grants) | `:285-574`, `:621-688`, `:4671-4748` | ~480 | PLATTFORM / andere Apps (ctox, mail, browser, workjet) | bleibt Core – gehört nicht zu threads, liegt nur in threads.rs |
| Peer-Schreibschutz (threads-Collections nur über Befehle; generisch server-autoritative Collections; persönliche Collections an Token-Nutzer) | `:188-238`, `:239-284`, `:689-965` | ~330 | PLATTFORM | bleibt Core; für threads im Manifest: „Collections nur über Befehle schreibbar“ (`write: 'commands'`) |
| `create_supervisor_ai_request` (Workjet-Supervisor-Turn) | `:2220-2238`, Zweig in `:2286-2299` | ~35 | GEMISCHT, Aufrufer `project_chats` | bleibt nativ (kein threads-Befehlstyp; Herkunftsnachweis nur nativ) |

**W-4 Projektions-Handler** (Vorschlag): Manifest `projections: [{handler:'_projection.ctox_relevance.js', sources:['business_commands','ctox_queue_tasks']}, {handler:'_projection.app_relevance.js', sources:[…14…]}]`; Core ruft `project(input, ctx)` entweder im Cursor-Betrieb `{command_since_ms, task_since_ms, limit}` bzw. `{source_cursors:[{collection, since_ms}], limit}` (heutiges Verhalten, Ergebnis `{changed_count, source_cursors, projections}` liefert den neuen Cursor) oder im Änderungs-Betrieb `{commands, tasks}` bzw. `{collection, docs}`. Beides ist implementiert und getestet. Idempotent: unveränderte Quellen schreiben nichts (`upsertIfChanged`, Zeitstempel = Quellzeit).

**W-5 Sichtbarkeitsregel** (Vorschlag): Manifest `visibility: '_visibility.js'`; Core ruft `visible(collection, doc, user_id, read)` nach Rollen-/Grant-Prüfung für die sechs threads-Collections; Ergebnis entscheidet Replikation und UI-Lesen. Alternativ deklarativ (`any_of` Felder + `join` über `thread_id`) – die JS-Regel ist exakt dieser Ausdruck.

Rest-Rust nach diesem Strang, der **threads-Fachlogik** ist: **0 Zeilen** (Befehle, beide Projektionen, Sichtbarkeit portiert).
Nativ bleibend in `threads.rs`, aber nicht threads-spezifisch: ~810 Zeilen Plattform (Replikation/Peer-Schreibschutz für
andere Collections) + ~35 Zeilen Workjet-Supervisor. Ohne W-4/W-5 laufen die portierten Projektionen/Sichtbarkeit noch
nicht produktiv – sie sind ausführbar und getestet, aber nicht angeschlossen.

## 5. Dateien

`handlers/_lib.js` (gemeinsame Helfer) · `handlers/threads.*.js` (23 Befehls-Handler) ·
`handlers/_projection.ctox_relevance.js` · `handlers/_projection.app_relevance.js` · `handlers/_visibility.js` ·
`handlers/_chat_access.js` (Workjet-Privatchat-Schutz, §6) · `handlers/_approval_targets.js` (feste Freigabe-Ziele, §7 THR2) · `handlers/tests/workjet-privacy.test.mjs` (§6) ·
`handlers/tests/_ctx.mjs` (ctx-Attrappe: Mango-Teilmenge, 200er-Lesekappung, Transaktion mit Rücksetzen,
Capability-Stubs, Aufrufprotokoll) · `handlers/tests/{messages,approvals,threadops,projection,projection-app,visibility,contract,vm,targets}.test.mjs` ·
`handlers/tests/vm-smoke.mjs` (lädt alle Handler und Projektionen als ES-Module in einen leeren `vm`-Kontext: keine
Host-Globals, nur relative Importe `./*.js`). Annahme U-16: Die App-Laufzeit lädt Handler als ES-Module mit relativen
Importen innerhalb von `handlers/`; sonst müssen `_lib.js` und die geteilten Kerne je Handler gebündelt werden.

## Capabilities

Bedarf der threads-Handler nach Strang WJAPP – nur OS-Primitive aus `v4/os-primitives/CATALOG.md`:

| Capability | Primitiv (CATALOG) | Nutzer |
|---|---|---|
| `cap.audit.event` | 13 `audit` | approve, reject, cancel |
| `cap.directory.user` | 12 `directory` | approval.request, thread.assign, thread.claim, handoff.create |
| `cap.policy.module_decision` | 19 `policy` | note.create, message.create, approval.request, link.create, approve |

Entfallen: workjet.command_access_check, workjet.supervisor_binding (keine Workjet-Capability mehr, Beleg §6).

## 6. Workjet-Privatchat-Schutz und Supervisor-Bindung als App-Logik (Strang WJAPP, 15.10.)

Rust-Referenz: `project_chats/privacy.rs:11-522` (`command_access_check`, `visible_in_store`, Verweis-/Kettenlogik),
`project_chats/supervisor_binding.rs:10-154` (`for_thread`), `project_chats.rs:344-413` (`owned_project`, `required`),
Aufrufer `threads.rs:1095` (Vorspann) und `threads.rs:3546` (`upsert_thread`).

### 6.1 Entscheidung Zugriffsschutz: Handler statt row_filters / command_rights

`row_filters` (P6: `all/none/eq/and/or`, JSON-Pointer, Literal oder `actor_id`, je Rolle) beschreiben die **Lesesicht
einer Collection auf ihre eigenen Felder**. Der Workjet-Schutz braucht dreierlei, was diese Form nicht ausdrücken kann:
(1) einen **Verweis in eine fremde Collection** auflösen (Chat-ID im Befehl → `workjet_project_chats` → Eigentümer →
`workjet_projects` → Eigentümer, Abbruch-Quittung → Zielbefehl in `business_commands`), (2) er prüft **Befehlsnutzlast**,
keine Zeile, (3) Allquantor über Arrays („jeder Eintrag in `target_user_ids` = Akteur“). `command_rights` kennt nur
Rolle bzw. Core-bestätigte Zuordnung. Ein generisches `policy.command_access` würde die Workjet-Präfixe
(`workjet_private_`/`workjet_group_`/`workjet_profile_`) und Collection-Namen im Core halten – genau die verbotene
App-Logik. Darum: Port als threads-Bibliothek `handlers/_chat_access.js`, aufgerufen im gemeinsamen Vorspann
`_lib.preflight` vor jedem der 23 Befehlstypen, nur mit `ctx.get` auf foreign_collections. Lesefehler verweigern
(fail-closed wie Rust `let Ok(..) else Some(false)`).

### 6.2 Entscheidung Supervisor-Bindung: foreign read statt threads-Collection

Die native Tabelle `workjet_supervisor_bindings` (project_id PK, owner_user_id, thread_id UNIQUE, created_at_ms) ist
Herkunftsnachweis der Workjet-Steuerung und wird außer von threads auch nativ gelesen (`mcp_workjet_worker_dispatch.rs:202,
:315`, `workjet_exit_model.rs:317`, `project_chats/project_init.rs:105`, `mcp_workjet_confirmed_plan.rs:217`); geschrieben
nur von `ctox.workjet.project.supervisor.bind` (Kern Phase 11). Eine threads-eigene Kopie hätte zwei Wahrheiten und
müsste vom Kern beschrieben werden (Kern schreibt in App-Collection = Verstoß gegen §16.13). Darum liest threads die
Bindung **lesend** als foreign_collection; `supervisorBindingForThread(store, owner, thread_id)` liefert unverändert
`{contract, project_id, thread_id, thread_key}` bzw. `null` (Selektor `{owner_user_id, thread_id}`, gelöschte Zeilen
zählen nicht, `thread_key = business-os/threads/<thread_id>`). Kein threads-Schema-/Migrationsbedarf.

### 6.3 foreign_collections (threads) und nötige exports

| Collection | Eigentümer | ops | exports heute | Nutzer |
|---|---|---|---|---|
| `workjet_project_chats` | ctox | read | **fehlt** → ctox muss `read` exportieren | `_chat_access.js` |
| `workjet_worker_profile_bindings` | ctox | read | **fehlt** → ctox muss `read` exportieren | `_chat_access.js` |
| `workjet_projects` | ctox | read | vorhanden (`read`) | `_chat_access.js` (`owned_project`) |
| `business_commands` | ctox-platform | read | vorhanden (schon deklariert) | `_chat_access.js` (Abbruch-Quittung → Zielbefehl) |
| `workjet_supervisor_bindings` | ctox (neu, siehe Kern-Import) | read | **fehlt** – Collection existiert noch in keinem Schema | `_lib.supervisorBindingForThread` (note/message/approval.request, beide Projektionen) |

Annahme: Handler-Lesezugriffe auf diese Collections laufen mit App-Befugnis, nicht durch die `row_filters` des Akteurs
gefiltert. Für den Zugriffsschutz ist das egal (gefiltert = nicht gefunden = verweigert, Ergebnis identisch); für die
Bindung zählt es nur, wenn ein Nicht-Eigentümer (Admin) in einen Supervisor-Thread schreibt – dann fiele die Metadaten-
Bindung sonst weg.

### 6.4 Kern-Import

| Quelle im Core | Ziel-Collection | Feldabbildung | Laufend danach |
|---|---|---|---|
| Store-Tabelle `workjet_supervisor_bindings` (`supervisor_binding.rs:31-36`) | `workjet_supervisor_bindings` (Eigentümer **ctox**, exports `{workjet_supervisor_bindings: ['read']}`) | `id ← project_id`, `project_id ← project_id`, `owner_user_id ← owner_user_id`, `thread_id ← thread_id`, `created_at_ms ← created_at_ms`, `updated_at_ms ← created_at_ms`, `is_deleted ← false` | `ctox.workjet.project.supervisor.bind` (Kern Phase 11) schreibt die Zeile in diese Collection statt (oder zusätzlich zur) Tabelle; die nativen Leser (6.2) lesen dieselbe Quelle |
| – (Daten liegen schon in RxDB) | `workjet_project_chats`, `workjet_worker_profile_bindings` | unverändert | nur `exports: ['read']` im ctox-Manifest ergänzen |

Offen für Kern Phase 11 (nicht threads): `supervisor_binding::apply` legt den Supervisor-Thread direkt in `user_threads`
an (`supervisor_binding.rs:110-127`) – ein Kern-Schreibzugriff in eine threads-Collection. Bei der Portierung von
`ctox.workjet.project.supervisor.bind` muss das über einen threads-Befehl laufen (Vorschlag: eigener Typ
`threads.supervisor_thread.ensure`, Prüfungen aus `supervisor_binding.rs:69-95`); bis dahin bleibt der Pfad nativ.

### 6.5 Bewusste Abweichungen (U-W)

- **U-W1 Identitäts-Alias**: Rust löst den Akteur über `business_user_identity_aliases` zum kanonischen Eigentümer auf
  und verlangt beide aktiv (`workjet_identity.rs:48-75`). Das ist Authentifizierung (Kern): `command.actor.id` gilt als
  kanonisch. Die Teilnehmerprüfung vergleicht wie Rust mit dem authentifizierten Akteur.
- **U-W2 Projektbefehl-Typliste**: `project_chats::is_command` (native Typliste) ist auf die Präfixe
  `ctox.workjet.project.`/`ctox.workjet.jour_fixe.` verkürzt – strenger, nie lockerer. Nur über Abbruch-Ketten erreichbar.
- **U-W3 Besprechungsbefehle** (`jour_fixe`) lösen ihr Projekt über die native Tabelle `workjet_jour_fixe_meetings` auf
  (`privacy.rs:419-436`); ohne diese Quelle verweigert der Handler (fail-closed). Nur über Abbruch-Ketten erreichbar.
- **U-W4 Kettenlänge**: Rust folgt Abbruch-Ketten unbegrenzt (Zyklus = Stapelüberlauf); hier Abbruch nach 8 Gliedern = verweigert.

### 6.6 Tests und Mutationsbelege

`handlers/tests/workjet-privacy.test.mjs` (11 Tests) überträgt `private_threads_reject_foreign_human_mentions_and_admin_mutation`
(`project_chats/tests.rs:846`) 1:1 (fremde Erwähnung verweigert, eigene erlaubt, fremder Mensch und fremder Admin
`unavailable`) und deckt alle acht Nutzlastfelder, Verweisfelder inkl. `record_id`/verschachteltem `client_context`,
gelöschte/fehlende Chats, fremde/gelöschte/archivierte Projekte, `required` (Bytes, Trim, Steuerzeichen),
Abbruch-Quittungsketten sowie Bindung (Eigentümer, gelöscht, Quellkontext-Konflikt `threads.rs:3548`) ab.
`messages.test.mjs` „preflight“ nutzt jetzt echte Chat-Daten statt einer Capability-Attrappe; `_ctx.mjs` bietet keine
Workjet-Capability mehr an. Mutationen (jede einzeln eingesetzt, Suite rot, danach sha256-gleich zurückgesetzt):
M1 Listenfelder, M2 Einzelfelder, M3 Chat-Eigentümer, M4 Chat-Projekt, M5 Chat gelöscht, M6 Projekt gelöscht,
M7 `required` Bytes statt Zeichen, M8 `required` Steuerzeichen, M9 fehlender Zielbefehl, M10 fail-closed bei Lesefehler,
M11 Vorspann entfernt, M12 Bindung ohne Eigentümer, M13 gelöschte Bindung, M14 Quellkontext-Konflikt, M15 leerer Nutzer
→ **15/15 rot**.



## 7. Agentenläufe mit Rückruf statt `business_os.chat.task` (Strang SKL-A2, 15.10., Kern §16.30)

Auslöser vorher → nachher:

| Auslöser | vorher | nachher |
|---|---|---|
| „KI fragen“ (`index.js` → `threads.ai.request`) | `ctx.command('business_os.chat.task')`, Core baute den Prompt (`store.rs:27009 command_prompt`) und meldete nur Status über die Projektion (R7, `_projection.ctox_relevance.js`) | `ctx.cap.agent.task.enqueue({title, instruction, thread_key, payload, idempotency_key:'threads.ai.request:<cmd>', on_complete:'threads.ai.complete', on_failed:'threads.ai.failed'})`; Verknüpfung `user_thread_links` (`link_role:'ai_task'`, `task_id`, `status`) als Schlüssel für den Rückruf |
| Freigabe `threads.ctox_approval.approve`, Ziel `business_os.chat.task` (Vorgabe) | `ctx.command(Zieltyp)` → Kern-Alias `agent.task{action:'approve'}` | `enqueueApprovedCommand` reiht selbst ein (`idempotency_key:'threads.ctox_approval:<id>'`, `on_complete:'threads.ctox_approval.complete'`, `on_failed:'threads.ctox_approval.failed'`), `approved_task_id` = Task |
| Freigabe, anderes Ziel (nur `ctox.coding.turn` aus reports, feste Liste, THR2) | `ctx.command` an dynamisches Ziel, Ergebnis nur als Befehlsstatus | `ctx.command` an die feste Ziel-App `coding-agents` mit `on_complete/on_failed` = `threads.ctox_approval.complete/.failed` (§16.39 Nr. 3); `status:'completed'` (synchron, R8) wird sofort abgeschlossen (`settleApproval`), der spätere Rückruf ist dann wirkungslos; `status:'failed'` bei Annahme bleibt die zentrale Ablehnung (`forbidden`) |

Neue Befehlstypen (keine UI-Sender): `threads.ai.complete`, `threads.ai.failed`, `threads.ctox_approval.complete`,
`threads.ctox_approval.failed` (je 1 Zeile, Logik in `handlers/_agent.js`).
- complete: Antwort (`result` als Text oder `{text|reply_text|reply|answer|message|summary|output}`) als Nachricht Art
  `ai_result`, Autor `ctox` (UI zeigt „CTOX“), Benachrichtigung `ctox_completed` an den Anfragenden, letzte Nachricht
  am Thread, Zähler neu. failed/cancelled: Ereigniszeile `ctox_status` mit den Bestandstexten aus `ctox_status_body`
  (threads.rs:4288: „CTOX Arbeit blockiert oder fehlgeschlagen.“ / „CTOX Arbeit wurde beendet.“) + `ctox_failed` bzw.
  `ctox_finished`. Idempotent: Verknüpfung bzw. `approval.execution_status` terminal → `{status:'ignored'}` ohne Schreiben.
- Freigabe-Rückruf der anfragenden App: `threads.ctox_approval.request` nimmt optional
  `callback:{on_complete, on_failed}` an; Befehlstypen müssen im Namensraum von `source_context.module` liegen (≠ threads),
  sonst `invalid` vor dem ersten Schreiben. Nach Abschluss leitet threads genau einmal per `ctx.command(<typ>,
  {approval_request_id, status, result_text, error, task_id, command_id, target_module, target_record_id,
  target_command_type, source_context}, {module:<app>, record_id})` weiter (Nutzer: reports).
- Freigabe-Datensatz bekommt `callback`, `execution_status`, `execution_finished_at_ms`, `result_message_id`,
  `result_text` (≤ 4000 Zeichen), `execution_error`; `status` bleibt `approved` (UI/Zähler unverändert).

Prompt im Paket: `prompts/chat-task.md` = wörtliche Übernahme des Kern-Formats `command_prompt` (allgemeiner Zweig,
`store.rs:27097`), Platzhalter `instruction, command_id, module, type, record_id, payload_json, context_json`;
Geheimnis-Schlüssel entfernt (`:27126`), JSON-Vorschau ≤ 32 000, Gesamt ≤ 96 000, Anweisung > 8 000 → `invalid`
(`:203-206`, `:26999`). Abweichung: `dedupe_repeated_long_strings` nicht portiert. Skills: keine (F §5).

Tests: `tests/agent.test.mjs` (7, mit `v4/tools/agent-sim`: enqueue/Template/Idempotenz, complete, Doppelzustellung,
failed, cancelled, zu lange Anweisung, Freigabe-Task + Weiterleitung genau einmal, Rückruf-Namensraum); angepasst:
`approvals` (Ziel `support.case.reply` für den ctx.command-Pfad + sofortiger Abschluss), `threadops` (ai.request),
`contract` (27 Typen, Import `ctox:shared/prompt-template.js`), `workjet-privacy` (cap `agent`), `vm-smoke`
(ctox:shared im vm, Freigabe-Rückruf). Mutationsbeleg (8 Mutationen, alle rot, Dateien byte-gleich zurück):
Doppelzustellung, fehlendes on_failed, Rückruf-Namensraum, Freigabe-Idempotenz, Agentenziel über ctx.command,
Abbruch als failed (+ reports: Statussperre, Idempotenz).

Kern-Abbau (jetzt tot für threads): Kern-Alias `agent.task{action:'request'|'approve'}`
(`v4/integration:src/core/app_capabilities/caps/task.rs`), `command_prompt` für `business_os.chat.task` aus threads,
`process_business_chat_reply`/`persist_terminal_business_chat_command_projection` für threads-Aufträge
(`store.rs:10404-10443`, `:11088`), Rust `threads.rs:2239-2313` (create_ai_request). `_projection.ctox_relevance.js`
überspringt threads-eigene Befehle ohnehin (`isThreadsInternalCommand`); Agenten-Tasks von threads erzeugen keine
`business_commands` mehr.

> **Stand FIX10 (16.10.2026):** Beide Listen dieses Abschnitts sind abgebaut. `APPROVAL_TARGETS` samt
> `ctox.coding.turn` entfiel in LOOP Runde 2 (§16.44): einziges Ziel ist die Agentenarbeit (`business_os.chat.task`,
> `AGENT_TASK_TARGET`), ein Coding-Turn startet nur aus der Code-Ansicht der App. `APPROVAL_CALLBACKS` ist seit FIX10
> leer: „Bugs & Features“ stellt keine Freigabeanfragen mehr, der Rückruf `reports.delegation.complete/.failed`, die
> reports-Handler und `commands_in from:['reports']` sind entfernt. Eine Anfrage mit `callback` ist `invalid`
> („not a declared approval callback“); Altbestände mit Rückruf werden abgeschlossen, aber nicht weitergeleitet (Log
> `warn`). threads hat damit kein `commands_out`. Der Text darunter beschreibt den Stand von THR2.

**Feste Ziele statt dynamischer Weiterleitung (Strang THR2, 15.10., Kern §16.40 / §16.39 Nr. 3).** Freigaben sind
Fachlogik von threads, kein allgemeines Freigabe-Primitiv. `handlers/_approval_targets.js` führt zwei feste Listen:
`APPROVAL_TARGETS = [{app:'coding-agents', type:'ctox.coding.turn', async:true}]` (Eigentümer laut
`modules/coding-agents/handlers/ctox.coding.turn.js`; einziges fremdes Ziel, Absender reports-Delegation) und
`APPROVAL_CALLBACKS = [{app:'reports', on_complete:'reports.delegation.complete', on_failed:'reports.delegation.failed'}]`.
Agentenarbeit (`business_os.chat.task`, Vorgabe) bleibt der eigene Task-Pfad. Je Eintrag genau ein Aufruf mit Typ-
und App-Konstante (`dispatchApprovedTarget`, `dispatchApprovalCallback`); die Ziel-App kommt nie aus der Anfrage
(`target_module` = bearbeitete App bleibt im Payload/`client_context`).
- `threads.ctox_approval.request`: Zieltyp außerhalb der Liste → `invalid` vor dem ersten Schreiben; ebenso ein
  `callback`, der nicht genau einem Eintrag aus `APPROVAL_CALLBACKS` entspricht. `approve` prüft Altbestände nach
  (`invalid`, nichts gesendet); ein Altbestand-Rückruf außerhalb der Liste wird nicht weitergeleitet (Log `warn`).
- **W-6 erledigt:** asynchron endende Ziele (`async:true`) bekommen `{on_complete:'threads.ctox_approval.complete',
  on_failed:'threads.ctox_approval.failed'}`. Die Rückruf-Handler nehmen neben `task_id` (Agenten-Task, §16.30) auch
  `command_id` (= `approved_command_id`, Befehls-Rückruf §16.39 Nr. 3) an; Abschluss idempotent über
  `execution_status`, danach genau eine Weiterleitung an reports wie im Task-Pfad.
- Manifest: `gen-manifest.mjs` leitet `commands_out` (coding-agents `ctox.coding.turn`, reports
  `reports.delegation.complete/.failed`) aus dem Code ab, `commands_out` in `v4-manifest.json` entfällt;
  Gegenstücke `commands_in from:['threads']` in `modules/coding-agents/v4-manifest.json` und
  `modules/reports/v4-manifest.json`. Neues Ziel = Listeneintrag + `case` + Test + `commands_in` beim Empfänger.
- Tests `tests/targets.test.mjs` (10): nicht gelistetes Ziel/Rückruf abgelehnt ohne Schreiben, Altbestand, asynchroner
  Abschluss und Fehlschlag per Rückruf mit genau einer Weiterleitung, Doppelzustellung wirkungslos, synchron + später
  Rückruf, Versand je Listeneintrag, Ableitung im Generator = Listen. `approvals.test.mjs` nutzt `ctox.coding.turn`.
  Mutationsbeleg (7 Mutationen, alle rot, Dateien byte-gleich zurück): Zielprüfung bei request, Rückruf an async-Ziel,
  command_id-Zuordnung, Idempotenz, Ziel-App aus Optionen, Rückruf-Liste, Weiterleitung an beliebigen Rückruf.
- Nicht übernommen: Altziele der v2-Shell (`app.js`: `ctox.business_os.app.modify`, `business_os.data.modify`) haben in
  v4 keinen Eigentümer-Handler und stehen nicht auf der Liste.
Der Kanal „Fehler in der Zustandszone“ für einen nach dem Befehl fehlschlagenden Task läuft über den Task-Status des
Kerns; in der App erscheint er als Ereigniszeile im Thread (Bestandsverhalten der Projektion).
