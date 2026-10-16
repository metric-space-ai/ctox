# JF – `project.jour_fixe.prepare` als Handler der besitzenden App (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf 4f4502564, Basis `src/apps/business-os/`, 16.10.2026.
Rust unter `src/core` und origin/main nur gelesen (`git fetch git@github.com:metric-space-ai/ctox.git main` →
FETCH_HEAD `e35a116ae`, Dateien per `git show FETCH_HEAD:<pfad>`). Kein git außer Lesen.

Übergabevertrag (gelesen, nicht geändert): `~/.local/state/ctox-sync-v4/docs/evidence/harness-10-20261016/interfaces-open.json`
Abschnitt `jour_fixe`; `~/.local/state/ctox-sync-v4/docs/befund-abbau-s9.md:56,65,95,118`.
Owner-Regel E17: Fachlogik gehört in den App-Handler. Der Rust-Emitter `jour_fixe_preparation::emit` (KPI, Brief,
eingebetteter Skill) ist **nicht** nachgebaut, und es wird **kein** Agentenauftrag erzeugt. Der Handler ruft keine einzige
Capability auf.

## Ergebnis

| Teil | Datei | Stand |
|---|---|---|
| Collection `workjet_jour_fixe_meetings` (neu, v0) | `v4/schemas/ctox.collections.schema.json` (`collections.workjet_jour_fixe_meetings`, `_sources`) | `check-schemas.mjs` grün |
| Handler | `modules/ctox/handlers/project.jour_fixe.prepare.js` + `handlers/_lib/jour-fixe.js` | 7/7 Tests grün, 11/11 Mutationen rot |
| Deklaration | `modules/ctox/v4-manifest.json` `command_rights["project.jour_fixe.prepare"]` (Zeile 94), `_basis` (Zeilen 4–5) | `gen-manifest.mjs` erzeugt, `--check` Exit 0 |
| Tests | `modules/ctox/handlers/tests/jour-fixe.test.mjs` | ganze ctox-Handler-Suite 45/45 |

## 1. Befunde (Datei:Zeile)

### 1a. Besitzer von Projekten und Meetings: App `ctox`
- `v4/schemas/ctox.collections.schema.json:2` `"_app_id": "ctox"`; `workjet_projects` ab `:1927` (v5, Pflicht
  `owner_user_id`, `status ∈ {active, archived}`, Terminfeld `jour_fixe {weekday, time, timezone}` ab `:2013`).
- `v4/tools/manifest/out/ownership.json:2345` `workjet_projects` → Basis-Zuordnung ctox (Strang M).
- Damit besitzt ctox auch die Meetings: Ein Meeting gehört genau einem `workjet_projects`-Datensatz und dessen Owner
  (`docs/workjet-jour-fixe-contract.md` „Identity and persistence“, origin/main).

### 1b. Meeting-Collection fehlte in v4
- Vor JF kein `workjet_jour_fixe_meetings` in irgendeiner `v4/schemas/*.collections.schema.json` (Collection-Liste ctox:
  27 Einträge, keiner mit `jour`).
- Im Kern lag die Tabelle nur in SQLite: `src/core/business_os/project_chats/jour_fixe_preparation.rs:16-21`
  (`meeting_id, project_id, owner_user_id, scheduled_at_ms, metadata_json, preparation_task_id`), `metadata_json` = das
  ganze Wire-`Meeting` (z. B. `mcp_workjet_jour_fixe.rs:628`).
- Wire-Vertrag `src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json`: `MeetingState` `:6`, `TodoState` `:9`,
  `Priority` `:10`, `Todo` `:17`, `TodoList` `:18`, `Meeting` `:19`, `PrepareRequest` `:20`, Befehl
  `ctox.workjet.jour_fixe.prepare` mit `authorization: bound_supervisor` `:41`. Generierter Browser-Verbraucher
  `src/apps/business-os/shared/workjet-jour-fixe-contract.generated.mjs` (1085 Zeilen, „Do not edit“) trägt dieselben Typen.
- Der Browser nennt die Collection schon so: `shared/jour-fixe-narration.mjs:28` (`linked_collection !==
  'workjet_jour_fixe_meetings'`); threads verweist darauf `modules/threads/handlers/_chat_access.js:120`.
- Nativer Arm heute: `src/core/business_os/command_plane.rs:1801-1806` meldet „Jour fixe action is not implemented on
  this native release“.

### 1c. Was „offene Themen“ sind (nur zum Verständnis gelesen, Logik nicht kopiert)
- Thema = `Todo {id, title, acceptance, priority P0|P1|P2, evidence_ids, due_at_ms?, owner?}` in der `TodoList` eines
  Meetings (`workjet-jour-fixe-v1.json:17-18`).
- Die vom Owner bestätigte Liste (`TodoList.status = 'confirmed'`) wird das Projektziel; ein Projekt hat genau ein
  laufendes Ziel (`jour_fixe_confirmed_goal.rs:17-20` Tabelle `workjet_project_goal_definitions` mit `project_id PRIMARY
  KEY`; `current_goal` `:118-156`). Die nächste Vorbereitung liest dieses Ziel
  (`jour_fixe_brief.rs:272-282` `current_goal_for_supervisor`; Vertrag „The next preparation reads the confirmed goal
  revision“). Eine spätere Bestätigung ersetzt die frühere („A stale confirmation cannot replace a newer goal“).
- Vorschläge (`proposed`) und abgelöste Listen (`superseded`) ändern kein Ziel (Vertrag „A proposed list never changes
  goals“) → keine offenen Themen.
- Nicht übernommen: KPI-/Exit-Auswertung `evaluation_status.open_items` (`jour_fixe_brief.rs:372-425`), GitHub-Quelle,
  Brief-Snapshot, Skill – das ist die vom Kern ausdrücklich nicht wiederbelebte Orchestrierung.

## 2. Collection `workjet_jour_fixe_meetings` (ctox, v0)

Kopf des Wire-`Meeting` (Feldnamen, Grenzen, Enums 1:1): `id` (= Meeting-ID, primaryKey), `project_id`,
`owner_user_id`, `supervisor {workjet_thread_id, ctox_thread_key}`, `scheduled_at_ms`, `prepare_at_ms`, `timezone`,
`state`, `revision`, `deck_revision`, `previous_goal?`, `todos?` (TodoList, ≤100 Punkte), `error?`.
Neu aus diesem Strang: `agenda {schema:'ctox.workjet.jour_fixe.agenda.v1', operation_id, prepared_at_ms,
source_meeting_id|null, source_goal?, items[≤100] {position, topic_id, source_meeting_id, title, priority, due_at_ms?}}`,
`prepared_at_ms`, `prepared_operation_id`; dazu `created_at_ms`, `updated_at_ms`, `is_deleted`.
Indizes `updated_at_ms`, `[project_id, scheduled_at_ms]`.

Bewusst **nicht** im Dokument: `slides`, `comments`, `transcript`. Der Vertrag verlangt dafür eigene Zeilen
(„The nested fixture is not permission to store an unbounded transcript in one RxDB document“); das gehört zum
Deck-/Transkript-Strang.

Migration: keine. Es ist eine neue Collection ohne Basisfassung (`_sources.source = "(neu, Strang JF)"`, wie die
CREWAPP-Collections), daher keine Generation (`check-schemas.mjs` §3 vergleicht nur gegen eine Basis).

## 3. Handler `project.jour_fixe.prepare`

Eingang `{id, record_id, payload:{project_id, meeting_id}}`; zusätzlich nur `payload.trigger` zulässig (Zeitplan-Feuer
§16.21), jedes andere Feld → `invalid payload_invalid:<feld>` (Vertrag: Aufrufer liefert keine Identität).

| Schritt | Ort | Fehler |
|---|---|---|
| `id`, `record_id`, `project_id`, `meeting_id` 1–128 Zeichen; `meeting_id == record_id` | `project.jour_fixe.prepare.js:21-37` | `invalid` |
| Meeting über `record_id` lesen (in `ctx.transaction`) | `:40-43` | fehlt/gelöscht `not_found`; `project_id` ≠ Payload `invalid` |
| gleiche Operations-ID → gespeicherter Beleg, kein Schreiben | `:45-47` | – |
| Projekt lesen, Owner muss zum Meeting passen, `status = active` | `:48-51` | `not_found` / `invalid` / `conflict project_not_active` |
| Termin vorbereitbar: `planned, preparing, ready, failed` | `:52`, `_lib/jour-fixe.js:11` | `conflict meeting_state:<state>` |
| Quelle: jüngster früherer, nicht gelöschter Termin desselben Projekts und Owners mit `todos.status='confirmed'` | `:54-55`, `_lib/jour-fixe.js:51-64` | – |
| Tagesordnung: Priorität P0→P2, dann Fälligkeit (ohne Datum zuletzt), dann Listenposition (Alter), dann ID; Bezug `topic_id` + `source_meeting_id` | `_lib/jour-fixe.js:19-46` | keine Themen → leere Liste |
| Patch auf `workjet_jour_fixe_meetings/<record_id>`: `agenda, prepared_at_ms, prepared_operation_id, revision+1, updated_at_ms` | `:69-75` | – |

Rückgabe nach `MeetingMutationReceipt` (`workjet-jour-fixe-v1.json:29`): `{operation_id, meeting_id, project_id, revision,
state, source_meeting_id, agenda_items, replayed}`. Der Terminstatus bleibt unverändert (Deck und Vertonung setzen
`ready`, nicht die Tagesordnung).

## 4. Deklaration: wer sendet, und warum kein `commands_in`

- Sender ist **keine App**. `interfaces-open.json` `jour_fixe`: `owning_app_request.app_id = "resolve authoritative
  active app, never infer from user payload"`, Zustellung über den generischen `AppRuntimeIngress.command` (Kern
  APP-PLATFORM-4c, #868). Fachlich auslösend ist der gebundene Projekt-Supervisor bzw. der Vorbereitungs-Zeitplan
  (`workjet-jour-fixe-v1.json:41` `bound_supervisor`), beide über den Kern.
- §16.40 (`ctox-sync-v4-architektur.md:1079-1083`) regelt nur App-zu-App-IPC. Der Generator lässt in `commands_in.from`
  nur `'*'` oder fremde App-IDs zu (`gen-manifest.mjs:959-963`). `'*'` würde den Befehl allen Apps öffnen; einen
  App-Sender gibt es nicht.
- Muster für Kern-zugestellte Befehle im Generator: net.ingest-Zustelltypen bekommen `command_rights {roles: [],
  assigned_users: false}` = nur Core-Zustellung, kein UI-Aufruf (`gen-manifest.mjs:1520-1525`, Regel 8 `:2269`).
- **Wahl:** kein `commands_in`-Eintrag; `command_rights["project.jour_fixe.prepare"] = {roles: [], assigned_users: false,
  _permission: "nur Core-Zustellung …"}` in `modules/ctox/v4-manifest.json:94`. Begründung steht in `_basis.command_rights`
  und `_basis.commands_in` (Zeilen 4–5). Ergebnis im erzeugten `v4/manifests/ctox.manifest.json`: Handler registriert,
  `command_rights` 53 → 54, `commands_in_without_handler: 0`, `commands_unmatched: 0`.

## 5. Abnahme

```
node v4/schemas/check-schemas.mjs                       → 4700 grün, 0 rot (230 Collections), Exit 0
node v4/tools/manifest/gen-manifest.mjs                 → erzeugt; danach --check Exit 0
node v4/os-primitives/check-caps.mjs --calls            → calls 202 · ABWEICHUNG 0 · HUELLE 0, Exit 0 (Handler ruft keine cap)
node --test modules/ctox/handlers/tests/jour-fixe.test.mjs                         → 7/7
node --import ./v4/tools/node/ctox-shared.mjs --test modules/ctox/handlers/tests/*.test.mjs → 45/45
node --test v4/tools/manifest/gen-manifest.test.mjs     → 43/43
node --test modules/ctox/tests/v4-api.test.mjs          → 12 pass, 0 fail
```

Tests (`tests/jour-fixe.test.mjs`): richtiges Meeting gepatcht (genau ein Schreibvorgang `patch` auf `record_id`, alle
anderen Meetings bytegleich, Anzahl unverändert, ID und `project_id` erhalten, Dokument erfüllt das v4-Schema), Reihenfolge
P0 → P1 (fällig früh, spät, ohne Datum) → P2; gleiche Operations-ID idempotent (kein zweiter Schreibvorgang, Revision
bleibt 3; neue Operation → Revision 4); keine offenen Themen → leere Tagesordnung ohne Fehler; falsches Projekt,
`meeting_id ≠ record_id`, fehlende `record_id`/`id`, Fremdfeld `owner_user_id`, Projekt eines anderen Owners → `invalid`
ohne Schreiben; unbekanntes Meeting/Projekt bzw. gelöschtes Meeting → `not_found`; Termin `live`, Projekt `archived`
→ `conflict`; nur die jüngste frühere Bestätigung zählt (spätere, gelöschte, Vorschläge und fremde Projekte nicht);
Themenquelle überschreitet Projekt- und Owner-Grenze auch dann nicht, wenn das Lesen mehr liefert.

Mutationsbeleg (Temp-Spiegel unter dem Session-Scratchpad, Arbeitsbaum unverändert), 11/11 rot: M1 Quelle statt
`record_id` gepatcht; M2 neuer Datensatz statt Patch; M3 Projektprüfung entfernt; M4 `meeting_id == record_id` entfernt;
M5 Idempotenz entfernt; M6 keine Themen → Fehler; M7 Priorität nicht sortiert; M8 `confirmed` nicht gefiltert;
M9 Projektgrenze entfernt; M10 Fälligkeit nicht sortiert; M11 Owner-Grenze entfernt.

## 6. Offen / an andere Stränge

1. **Erledigt-Status je Thema fehlt in v4.** Ob ein bestätigter Punkt schon abgearbeitet ist, steht nur im Kern
   (`planned_steps.status`, `jour_fixe_confirmed_goal.rs:190-205`), ohne RxDB-Projektion und ohne Capability. Der Handler
   nimmt deshalb alle Punkte des laufenden Ziels als offen. Sobald der Kern den Schrittstatus projiziert, ist das ein
   Filter in `buildAgendaItems` (`_lib/jour-fixe.js:19`). → Kern/CUTOVER.
2. **Wer schreibt die Meeting-Zeilen?** Die neue Collection ist leer, bis der Kern beim Umstieg die SQLite-Zeilen
   (`metadata_json` → Dokument, ohne slides/comments/transcript) einmal importiert und der Zeitplan neue Termine als
   ctox-Dokument anlegt. → Kern-Import, gleiche Form wie CREWAPP (`modules/ctox/V4-HANDLERS.md` „Kern-Import“).
3. **Kein `expected_revision`.** Der Wire-`PrepareRequest` (`workjet-jour-fixe-v1.json:20`) hat `expected_revision`, der
   Übergabevertrag nicht. Der Handler schützt über `ctx.transaction` und Idempotenz, nicht über CAS. Soll der Supervisor
   CAS bekommen, ist das ein optionales `payload.expected_revision` (`conflict stale_base`).
4. **threads bekommt automatisch Leserecht.** Der Generator erkennt in `modules/threads/handlers/_chat_access.js:120`
   den Namen `workjet_jour_fixe_meetings` (nur in einem Kommentar zu U-W3) und schlägt `foreign_collections … read` vor;
   ctox exportiert die Collection dadurch `read` (`_proposal: "nur per Code-Literal erkannt … Nutzung bestätigen"`).
   Das ist nicht mein Strang: threads bestätigt es (U-W3 löst Besprechungsbefehle über diese Tabelle auf) oder trägt
   `foreign_drop` ein.
5. `DRIFT.md:97`: `workjet_jour_fixe_meetings` steht nur im v4-Schema, nicht in `modules/ctox/collections.schema.json`.
   Das gilt für alle v4-only-Collections und wird über den Paket-Build aufgelöst; hier unverändert.

## Runde 2 (16.10.2026, nach Abnahme 41c4eef3b, Kern-Entscheidung E23)

Basis 41c4eef3b, kein git außer Lesen. Geändert: `handlers/project.jour_fixe.prepare.js`, `handlers/_lib/jour-fixe.js`
(`PAYLOAD_KEYS` + `expected_revision`), `v4/schemas/ctox.collections.schema.json` (Feld `created_operation_id`),
`modules/ctox/v4-manifest.json`, `tests/jour-fixe.test.mjs`. Neu: `handlers/project.jour_fixe.create.js`,
`tests/jour-fixe-create.test.mjs`. Neu erzeugt: `v4/manifests/ctox.manifest.json`, `v4/tools/manifest/REPORT-M2.md`,
`out/DRIFT.md`, `out/stats.json`.

### R2.1 `expected_revision` in `project.jour_fixe.prepare`
- Name = Wire-Name `PrepareRequest.expected_revision` (`workjet-jour-fixe-v1.json:20`); es gibt nur diesen einen Namen.
  Optional, ganzzahlig ≥ 0, sonst `invalid payload_invalid:expected_revision` (`prepare.js:39-42`).
- Reihenfolge im Handler (`prepare.js:44-59`): Meeting lesen → ID/Projekt prüfen → **zuerst** gleiche Operations-ID =
  gespeicherter Beleg (`replayed:true`, kein Schreiben), auch wenn die Revision inzwischen gestiegen ist → **dann**
  `expected_revision ≠ meeting.revision` → `stale_base` (§16.42-Code), vor jeder Wirkung → danach wie Runde 1.
- Tests: passende Revision bereitet vor (Revision 2 → 3); veraltete 1 und zukünftige 3 → `stale_base` ohne Schreiben;
  −1, 1.5, `'2'`, `null` → `invalid`; Wiederholung derselben Operation nach fremder Revisionsänderung (2 → 7) liefert
  den gespeicherten Beleg ohne Schreiben, eine NEUE Operation mit der alten Revision ist `stale_base`.

### R2.2 Neuer Handler `project.jour_fixe.create`
- Vertrag: `{id: Operations-ID, record_id: neue meeting_id (vom Absender vergeben), payload: Meeting-Kopf}`; Kopf nach
  `types.Meeting` (`workjet-jour-fixe-v1.json:19`) mit denselben Grenzen: `project_id` (≤128), `owner_user_id` (≤256),
  `supervisor {workjet_thread_id ≤128, ctox_thread_key ≤512}`, `scheduled_at_ms`, `prepare_at_ms` (≥ 0,
  `prepare_at_ms ≤ scheduled_at_ms`), `timezone` (≤128), `previous_goal? {goal_id, revision}`, optional `id`/`meeting_id`
  (müssen `record_id` sein), `trigger` (Zeitplan). Jedes andere Feld (z. B. `state`, `revision`, `slides`) → `invalid`:
  Zustand und Revisionen setzt nie der Absender.
- Anlage wie der Kern-Emitter `jour_fixe_preparation.rs:95-119`: `state 'planned'`, `revision 0`, `deck_revision 0`,
  dazu `created_operation_id`, `created_at_ms`, `updated_at_ms`, `is_deleted:false`; ein `put` unter `record_id`.
- Prüfungen (in `ctx.transaction`): Projekt fehlt/gelöscht → `not_found`; anderer Owner → `invalid`;
  `status ≠ active` (archiviert) → `conflict project_not_active`.
- Idempotenz wie `jour_fixe_preparation.rs:124-143` (`ON CONFLICT DO NOTHING` + Kopfvergleich): existiert die
  meeting_id mit gleichem Kopf, kommt der gespeicherte Termin zurück (`replayed:true`), auch wenn er inzwischen
  vorbereitet wurde; er wird nie zurückgesetzt. Anderer Kopf (Zeitpunkt, Vorlauf, Zeitzone, Supervisor, Vorgängerziel)
  oder gelöschter Datensatz unter derselben meeting_id → `conflict meeting_id_conflict`. Dieselbe Operations-ID für eine
  andere meeting_id → `conflict operation_id_conflict` (Suche über `created_operation_id`).
- Kein Agentenauftrag, kein `ctx.command`, keine Capability (Test prüft `state.commands` und `state.capCalls` leer).
- Rechte: `command_rights["project.jour_fixe.create"] = {roles: [], assigned_users: false}` (nur Kern-Zustellung), kein
  `commands_in` (Begründung wie §4); `_basis` in `v4-manifest.json` nennt beide Typen.
- Schema: neues optionales Feld `created_operation_id` (1–128) in `workjet_jour_fixe_meetings`; die Collection hat
  weiterhin keine Basisfassung, daher keine Migration.

### R2-Abnahme
```
node v4/tools/manifest/gen-manifest.mjs                 → Exit 0; --check Exit 0 (ctox: Handler create+prepare, roles [] je Typ, kein _todo)
node v4/schemas/check-schemas.mjs                       → 4701 grün, 0 rot, Exit 0
node v4/os-primitives/check-caps.mjs --calls            → calls 202 · ABWEICHUNG 0 · HUELLE 0, Exit 0
node --test …/jour-fixe.test.mjs …/jour-fixe-create.test.mjs                        → 14/14 (9 prepare, 5 create)
node --import ./v4/tools/node/ctox-shared.mjs --test modules/ctox/handlers/tests/*.test.mjs → 52/52
node --test v4/tools/manifest/gen-manifest.test.mjs     → 43/43
node --test modules/ctox/tests/v4-api.test.mjs          → 12 pass, 0 fail
```
Mutationsbeleg (Temp-Spiegel im Session-Scratchpad, Arbeitsbaum unverändert), 13/13 rot: R1 Revision nicht geprüft;
R2 Revisionsprüfung vor der Idempotenz; R3 `conflict` statt `stale_base`; R4 Format von `expected_revision` nicht
geprüft; C1 Projekt nicht geprüft; C2 Archiv nicht geprüft; C3 anderer Inhalt wird angenommen; C4 Wiederholung schreibt
neu; C5 Operations-ID für zweiten Termin erlaubt; C6 Owner nicht geprüft; C7 Fremdfelder erlaubt; C8 Kopfvergleich
ohne Zeitpunkt; C9 Agentenauftrag nach der Anlage. Die 11 Mutationen aus Runde 1 sind gegen den neuen Stand erneut
11/11 rot.

Damit ist Offen-Punkt 2 (wer schreibt die Meeting-Zeilen) für neue Termine gelöst; der einmalige Import alter
SQLite-Zeilen bleibt Kern-Sache. Offen-Punkt 3 (`expected_revision`) ist erledigt.

### R2.3 Vermerk für später (nichts gebaut): Felder, die `prepare` für „erledigt“ braucht

Der Kern projiziert `planned_steps` read-only in eine ctox-Collection (Schemaform wird abgestimmt). Damit
`buildAgendaItems` (`_lib/jour-fixe.js:19`) erledigte Punkte weglassen kann, braucht der Handler je Schritt:

| Feld | Zweck | Kern-Quelle |
|---|---|---|
| `id` (Dokument) | stabiler Schlüssel, z. B. `<goal_id>:<step_id>` | – |
| `goal_id` | Zuordnung zur bestätigten Liste: muss `todos.goal.goal_id` des Quell-Termins sein (= `agenda.source_goal.goal_id`) | `planned_steps.goal_id` |
| `goal_revision` | nur Schritte genau dieser Zielrevision zählen (`todos.goal.revision`), nie die eines abgelösten Ziels | `workjet_project_goal_definitions.revision` |
| `step_id` | **= Todo-`id`** (der Kern legt die Schritte mit `ConfirmedStep.id = todo.id` an, `jour_fixe_confirmed_goal.rs:388-389`) → `topic_id` | `planned_steps.step_id` |
| `status` | `pending, queued, completed, blocked, failed` (`src/core/mission/plan_status.rs:23-27`); „erledigt“ = `completed`, alles andere bleibt auf der Tagesordnung | `planned_steps.status` |
| `completed_at_ms` | optional für eine spätere „seit dem letzten Termin erledigt“-Liste | `planned_steps.completed_at` |
| `updated_at_ms` | Sync-Cursor/Frische | `planned_steps.updated_at` |
| Ziel-Status je `goal_id` (`active, completed, blocked, failed, superseded`, `plan_status.rs:104-108`) und `failed_step_id` | Ziel `superseded` → keine offenen Themen aus diesem Ziel; `failed_step_id` → der gescheiterte Punkt zuerst bzw. markiert | `planned_goals.status` + Kern-Projektion |
| `project_id`, `owner_user_id` | Leseschranke wie bei den Meetings (Themenquelle überschreitet nie Projekt/Owner) | über `workjet_project_goal_definitions` |

Regel für den späteren Filter: Punkt offen ⇔ es gibt keinen Schritt mit `goal_id = source_goal.goal_id`,
`goal_revision = source_goal.revision`, `step_id = topic_id` und `status = 'completed'`. Fehlt die Projektion
(Collection leer oder Ziel unbekannt), bleibt das heutige Verhalten (alle Punkte offen) – kein Raten. Nicht gebraucht:
`last_result_excerpt`, `attempt_count`, `last_message_key`, Anweisungstext.
