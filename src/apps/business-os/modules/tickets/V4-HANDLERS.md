# tickets – v4-Handler (Strang H8, Shell v4)

Stand 15.10.2026. Quelle der Einstufung: `~/.local/state/shell-v4/analysis/E-befehlstypen.md` (§2 Tabelle, §3 Capabilities),
Trennregel Architektur §16.13, Handler-Vertrag §16.16. Rust nur gelesen: `~/Documents/ctox-shell-v4/src/core/` `mission/tickets/cases.rs` (Router `business_os/command_plane.rs:2325`).
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`
(`invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable`). Tests: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (aus handlers/)
(In-Memory-ctx `handlers/tests/_ctx.mjs`: eigene Collections = v4-Manifest `collections`, Fremdzugriff = Manifest
`foreign_collections` + hier beantragte Ergänzungen; Capabilities protokolliert und je Test beantwortbar).

**Stand (TICKAPP, 15.10.; SKL-A1: Skill `tickets/zammad-writeback` + Templates, siehe Ende): 12 von 12 Befehlstypen als JS-Handler mit eigener Fachlogik (Fall-Zustandsautomat in `handlers/_tickets.js`), dazu 2 Abschlussbefehle des agent.task-Rückschriebs (Kern §16.30), 0 über `ctx.cap.legacy.<command_type>`, 0 Aufrufe von `ctox.ticket.run`. Tests: 16 grün (`node --test handlers/tests/tickets.test.mjs`).**

## Befehlstypen

| # | Befehlstyp | Einstufung | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|---|
| 1 | `ctox.ticket.local.create` | FACHLOGIK | `mission/tickets/cases.rs:1230-1248` | `handlers/ctox.ticket.local.create.js` (App-Logik) | portiert |
| 2 | `ctox.ticket.approve` | FACHLOGIK | `mission/tickets/cases.rs:1277-1290` | `handlers/ctox.ticket.approve.js` (App-Logik) | portiert |
| 3 | `ctox.ticket.execute` | FACHLOGIK | `mission/tickets/cases.rs:1291-1296` | `handlers/ctox.ticket.execute.js` (App-Logik) | portiert |
| 4 | `ctox.ticket.verify` | FACHLOGIK | `mission/tickets/cases.rs:1297-1307` | `handlers/ctox.ticket.verify.js` (App-Logik) | portiert |
| 5 | `ctox.ticket.request_clarification` | FACHLOGIK | `mission/tickets/cases.rs:1308-1359` | `handlers/ctox.ticket.request_clarification.js` (App-Logik) | portiert |
| 6 | `ctox.ticket.publish_clarification` | FACHLOGIK | `mission/tickets/cases.rs:1360-1380` | `handlers/ctox.ticket.publish_clarification.js` (App-Logik) | portiert |
| 7 | `ctox.ticket.resolve_clarification` | FACHLOGIK | `mission/tickets/cases.rs:1381-1401` | `handlers/ctox.ticket.resolve_clarification.js` (App-Logik) | portiert |
| 8 | `ctox.ticket.writeback_comment` | FACHLOGIK | `mission/tickets/cases.rs:1402-1412` | `handlers/ctox.ticket.writeback_comment.js` (App-Logik) | portiert |
| 9 | `ctox.ticket.close` | FACHLOGIK | `mission/tickets/cases.rs:1425-1433` | `handlers/ctox.ticket.close.js` (App-Logik) | portiert |
| 10 | `ctox.ticket.local.comment` | FACHLOGIK | `mission/tickets/cases.rs:1249-1261` | `handlers/ctox.ticket.local.comment.js` (App-Logik) | portiert (kein UI-Sender, nicht in E) |
| 11 | `ctox.ticket.local.transition` | FACHLOGIK | `mission/tickets/cases.rs:1262-1276` | `handlers/ctox.ticket.local.transition.js` (App-Logik) | portiert (kein UI-Sender, nicht in E) |
| 12 | `ctox.ticket.writeback_transition` | FACHLOGIK | `mission/tickets/cases.rs:1413-1424` | `handlers/ctox.ticket.writeback_transition.js` (App-Logik) | portiert (kein UI-Sender, nicht in E) |
| 13 | `ctox.ticket.writeback.complete` | FACHLOGIK | `cases.rs:1016-1059`, `:1137-1192`, `:700-773` (Erfolgszweig nach Adapteraufruf) | `handlers/ctox.ticket.writeback.complete.js` → `_writeback.js` | neu (on_complete-Befehl, Kern §16.30, kein UI-Sender) |
| 14 | `ctox.ticket.writeback.failed` | FACHLOGIK | `cases.rs:1906-1955`, `:671-691` (Fehlerzweig) | `handlers/ctox.ticket.writeback.failed.js` → `_writeback.js` | neu (on_failed-Befehl, Kern §16.30, kein UI-Sender) |

Die neun Typen aus E sind die UI-Sender; die drei weiteren Arme desselben Routers (`local.comment`, `local.transition`,
`writeback_transition`) sind mitportiert, damit der Präfix-Router `ctox.ticket.*` vollständig abgelöst werden kann.

## Fall-Zustandsautomat in der App (Strang TICKAPP)

Owner-Regel: App-Logik darf nicht im Core liegen. `ctox.ticket.run` (MAPPING (b)) ist deshalb entfallen; die Handler
setzen `run_business_os_ticket_command` (cases.rs:1224-1436) selbst um, über die eigenen `ctox_ticket_*`-Collections:

- **Zustände/Übergänge** (`_tickets.js`): Vokabular + Aliase aus `case_state.rs:96-131`, Ticket-Übergangskatalog aus
  `core_state/mod.rs:1172-1198`, gleicher Kernzustand = noop (`guard.rs:150`), Rückmeldetext wörtlich aus
  `guard.rs:1193-1245`. Abschluss (`close`, `writeback_transition`) verlangt Kernzustand Verified, eine bestandene
  Prüfung (`closure_requires_verification`) und einen Review-Beleg (letzte Freigabe/Prüfung; cases.rs:1457-1602).
  Fehlercodes: `not_found` (Fall/Ticket/Rückfrage fehlt), `conflict` (Zustand/Katalog), `invalid` (Payload/Status).
- **Freigaben/Prüfungen/Ausführung**: `ctox_ticket_approvals`, `ctox_ticket_verifications`; die Ausführungsnotiz
  (Rust `ticket_execution_actions`, nur geschrieben) steht am Fall als `last_execution_action`.
- **Rückfragen**: `ctox_ticket_clarification_requests` (draft → waiting_for_response → resolved, send_failed),
  Fall → `blocked_needs_clarification` und beim Auflösen zurück in `resume_state`.
- **Lokale Tickets**: direkt als `ctox_ticket_items` (`local:LT-…`) und `ctox_ticket_events` (`local:LE-…`, eigene
  Rückschriebe `direction:'outbound'`, `metadata.origin:'ctox-writeback'`); `ticket_key` im Ergebnis = `LT-…` wie Rust.
  `local.create` akzeptiert zusätzlich `ticket_id` (`LT-…`), damit support die ID über `ctx.command` vorgeben kann.
- **Rückschrieb an Fremdsysteme** (zammad): statt Kern-Adapter `agent.task.enqueue` (flache Form, `idempotency_key`
  `ticket-writeback:<writeback_id>`, `on_complete:'ctox.ticket.writeback.complete'`,
  `on_failed:'ctox.ticket.writeback.failed'`). Writeback-Zeile `queued` mit `task_id` und der Anfrage in `payload`.
  Nach Kern §16.30 schreibt der Core nach Taskende genau einen Befehl `{type:<on_complete|on_failed>,
  payload:{task_id, status, result, error?, workspace?}}`; die Handler finden die Zeile über `task_id`
  (fehlt sie: `not_found`), schließen ab (ok + Fall closed bzw. `failed`/`send_failed`) und sind idempotent: eine
  Zeile, die nicht mehr `queued` ist, liefert `{status:'ignored'}` ohne Schreibzugriff und ohne Audit
  (Test „Doppelzustellung“). Restrisiko: `task_id` wird direkt nach dem Enqueue an die Zeile geschrieben; ein
  Abschlussbefehl, der vor diesem Schreiben verarbeitet würde, endet mit `not_found`.
- **Audit**: jeder Rust-`record_audit` als `audit.event({collection, record_id, event_type:'ticket.<action_type>',
  payload:{ticket_key, case_id, actor_type, action_type, label, bundle_label, bundle_version, details}, observed_at_ms})`
  nach dem Commit (Capabilities nicht in `ctx.transaction`).
- **Bewusste Abweichungen**: (1) Freigabe/Prüfung werden atomar mit dem Übergang geschrieben – Rust schrieb die
  Zeile vor der Übergangsprüfung ohne Transaktion und ließ sie bei abgelehntem Übergang stehen. (2) IDs aus
  `stable_digest(<Befehls-ID>)` statt `stable_digest(now)` (Wiederholung desselben Befehls trifft dieselben Daten).
  (3) `validate_closure_assurance_claims`/`validate_outcome_artifact_state` (Kern-DB-Prüfungen) entfallen; für
  Ticket-Fälle trugen sie keine Belege. (4) Fehlgeschlagener lokaler Rückschrieb schreibt keine `failed`-Zeile
  (lokal kann nur die Transaktion scheitern, sie rollt komplett zurück).

## Capabilities

| Capability | Signatur | Beispielaufruf | Nutzer |
|---|---|---|---|
| `ctx.cap.audit.event` | `({collection, record_id, event_type, payload, observed_at_ms})` (CATALOG Nr. 13) | `await callCap(ctx, 'audit.event', {collection:'ctox_ticket_cases', record_id:'case-1', event_type:'ticket.approval_decision', payload:{…}, observed_at_ms})` | alle Fall-/Rückfrage-Handler |
| `ctx.cap.agent.task.enqueue` | `({title, instruction, skill?, thread_key?, payload, idempotency_key?, on_complete?, on_failed?}) → {task_id}` (CATALOG Nr. 7, flache Form beim Kern angefordert) | `await callCap(ctx, 'agent.task.enqueue', {title, instruction, thread_key:'ticket/zammad/4711', payload:{writeback_id,…}, idempotency_key:'ticket-writeback:<id>', on_complete:'ctox.ticket.writeback.complete', on_failed:'ctox.ticket.writeback.failed'})` | `writeback_comment`, `writeback_transition`, `publish_clarification` (nur Fremdsysteme) |

Entfallen: die Kern-Capability ctox.ticket.run. Aufrufer aus anderen Apps senden die Befehle `ctox.ticket.*` an diese App
(`ctx.command(type, payload, {module:'tickets'})`), heute `support.ticket.create_from_conversation`.

## Kern-Import

Einmaliger Import, den der KERN liefert (Quelle `runtime/ctox.sqlite3`, Ticket-DB `open_ticket_db`); Ziel sind die
eigenen Collections, Felder wie die heutige Projektion `business_os_ticket_projection_documents` (mod.rs:3500-3660),
also ohne Umbenennung, plus `id`, `updated_at_ms` (ISO → ms), `is_deleted:false`:

| Quelle (Tabelle) | Ziel-Collection | id | Feldabbildung |
|---|---|---|---|
| `ticket_items` (inkl. gespiegelter `local_tickets`) | `ctox_ticket_items` | `ticket_key` | TicketItemView 1:1 (`ticket_key, source_system, remote_ticket_id, title, body_text, remote_status, priority, requester, created_at, updated_at, last_synced_at, metadata`) |
| `ticket_events` | `ctox_ticket_events` | `event_key` | TicketEventView 1:1 |
| `ticket_cases` | `ctox_ticket_cases` | `case_id` | TicketCaseView 1:1 (`state` unverändert, auch Aliase) |
| `ticket_approvals` | `ctox_ticket_approvals` | `approval_id` | `approval_id, case_id, status, decided_by, rationale, created_at` |
| `ticket_verifications` | `ctox_ticket_verifications` | `verification_id` | `verification_id, case_id, status, summary, created_at` |
| `ticket_writebacks` | `ctox_ticket_writebacks` | `writeback_id` | `writeback_id, case_id, ticket_key, operation, payload(json), status, created_at` |
| `ticket_clarification_requests` | `ctox_ticket_clarification_requests` | `clarification_id` | wie `list_ticket_clarification_requests_for_business_os` (`missing_inputs`, `metadata` als JSON) |
| `ticket_execution_actions` (neueste je Fall) | `ctox_ticket_cases.last_execution_action` | `case_id` | `{action_id, summary, created_at}` |
| `local_ticket_events` ohne Spiegel in `ticket_events` | `ctox_ticket_events` | `local:<event_id>` | `remote_event_id=event_id`, `direction` = `outbound` wenn `metadata.origin='ctox-writeback'`, sonst `inbound` |

Nach dem Import ist die App Wahrheit für diese Collections; die Kern-Projektion in sie entfällt. Offen beim Kern
(kein Teil von `ctox.ticket.run`): Anlage neuer Fälle aus der Ticket-Routing-Schiene (`create_dry_run`,
Label-/Bundle-Zuordnung) und der Sync von Fremdsystemen (`sync_ticket_system`) schreiben heute in die Kern-DB; sie
müssen künftig über App-Befehle bzw. agent.task in diese Collections schreiben.

## App-Skill und Prompt-Templates (Strang SKL-A1, 15.10.2026)

Der Rückschrieb an Fremdsysteme war schon im Zielformat (§16.30), aber ohne Skill und mit Anweisungstext im Handler.
Jetzt:

- Skill `skills/zammad-writeback/SKILL.md` (+ `references/api-access.md`): aus `src/skills/packs/vendor/zammad-rest`
  kopiert und auf den Rückschrieb zugeschnitten (Endpunkte und Felder wie `ticket_zammad_native.rs`
  `create_article`/`update_ticket_state`/`writeback_transition`), ohne Rückschreibweg über CTOX-CLI/MCP; Ergebnisformat
  `{"remote_event_ids":[…]}` bzw. `{"error":"…"}`.
- Templates `prompts/writeback-comment.md`, `writeback-transition.md`, `writeback-transition-note.md`: wörtlich aus der
  früheren `remoteInstruction` (`_tickets.js`), nur Platzhalter (`source_system`, `remote_ticket_id`, `visibility`,
  `body`, `state`, `note_body`); gefüllt mit `renderPrompt` (`ctox:shared/prompt-template.js`, liest über `ctx.asset`).
  `clarification_request` nutzt das Kommentar-Template (wie vorher).
- `enqueueRemoteWriteback` setzt `skill:'tickets/zammad-writeback'`.
- Antwortauswertung in `_writeback.js` (`parseWritebackResult`): Ergebnis als Objekt oder letzter JSON-Block im
  Antworttext (`text`/`output`/`final_text` oder String); `{error}` ohne IDs → Fehlerzweig; numerische IDs → Text.
  Fehlertext aus `{code, message}` → `message` (vorher JSON-Text). Abbruch (`on_failed` mit `status:'cancelled'`) →
  Writeback-Zeile `cancelled`, Audit `writeback_failed` mit `details.cancelled:true`, Rückfrage `send_failed`
  (erneut veröffentlichbar).
- Tests: `handlers/tests/tickets-agent.test.mjs` (6, agent-task-Simulation `v4/tools/agent-sim`: Skill-Auflösung
  gegen das Paket, Template wörtlich gegen die alte Funktion, complete/fail/cancel/{error}, Doppelzustellung);
  `handlers/tests/_ctx.mjs` stellt `ctx.asset` (Paketleser der Simulation). Aufruf mit der gemeinsamen Auflösung:
  `cd handlers && node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (22/22).
  Mutationsbeleg: 7 Mutationen (Skill, Template-Wert, Template-Text, {error}, JSON im Text, Abbruch, Doppelzustellung) rot.
- Kern-Abbau (tot für tickets): `ticket_zammad_native.rs` `writeback_comment`/`writeback_transition` als
  Rückschreibweg der Business-OS-Befehle; `src/skills/packs/vendor/zammad-rest` bleibt nur, solange andere Nutzer
  (Sync/Provisionierung) es brauchen.
