# support – Fachlogik als JS-Handler (Strang H3, Shell v4)

Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/support.rs` (5 150 Z., davon
Produktion 1–4354, Tests 4355–5150), Routing `command_plane.rs:2338`, Rechte `command_plane.rs:1417` →
`support.rs:392 command_permission`, Rollenregel `policy.rs:478–490`, Intake-Aufruf `rxdb_peer.rs:5605`.
Vertrag: Architektur §9.4, §16.9, §16.13; Analyse E (`E-befehlstypen.md` Z. 355–388), C2 § modules/support.

Fortschritt (Berlin-Zeit):
- 12:46 Start, Analyse E/C2/D, Architektur §9.4/§16.9–16.13, support.rs vollständig gelesen.
- 12:55 Inventar angelegt.
- 12:58 `handlers/_shared.js` (gemeinsame Fachlogik) und `tests/_ctx.mjs` (ctx-Attrappe).
- 13:00 alle 34 Handler + `support.intake.project` geschrieben.
- 13:04 58/58 Tests grün (mit `--experimental-vm-modules`), bestehender `modules/support/tests/support.test.mjs` unverändert 14/14.
- 13:08 Abschluss dieses Dokuments.

**Ergebnis: 34 / 34 Befehlstypen als Handler portiert (seit SKL-A1 zusätzlich `support.agent.task` + 2 Abschlussbefehle, §6), 0 über `ctx.cap.legacy.*`, 0 Handler braucht eine
Fach-Capability im Core (Ticketanlage seit Strang TICKAPP per `ctx.command` an die Tickets-App). Zusätzlich die
Intake-Projektionsschleife als Handler.**

## 1. Inventar der 34 Befehlstypen

„UI“ = Sender in `modules/support/index.js` (Zeile), „–“ = kein Browser-Sender (MCP/Agent/Admin).
Reihenfolge = Port-Reihenfolge nach Nutzen. Tests: Datei unter `handlers/tests/`.

| # | Befehlstyp | E-Einstufung | Rust-Ort | UI | Ziel | Stand / Tests |
|---|---|---|---|---|---|---|
| 1 | `support.note.create` | FACHLOGIK | `support.rs:733` | 962 | Handler | ✔ conversation |
| 2 | `support.conversation.assign` | FACHLOGIK | `:516` | 1041 | Handler | ✔ conversation |
| 3 | `support.conversation.snooze` | FACHLOGIK | `:625` | 1060 | Handler | ✔ conversation |
| 4 | `support.conversation.claim` | FACHLOGIK | `:478`, `:2045` | 1091 | Handler | ✔ conversation |
| 5 | `support.conversation.resolve` | FACHLOGIK | `:658`, `:2076`, `:2762` | 1094 | Handler | ✔ conversation, sla-reporting |
| 6 | `support.conversation.reopen` | FACHLOGIK | `:674` | 1093 | Handler | ✔ conversation |
| 7 | `support.conversation.status` | FACHLOGIK | `:572`, `:2960` | 1127 | Handler | ✔ conversation (U8) |
| 8 | `support.conversation.priority` | FACHLOGIK | `:591` | 1128 | Handler | ✔ conversation |
| 9 | `support.agent.apply_suggestion` | FACHLOGIK | `:1279` | 1154 | Handler | ✔ agent-reply-ticket (U9) |
| 10 | `support.agent.reject_suggestion` | FACHLOGIK | `:1279` | 1153 | Handler | ✔ agent-reply-ticket |
| 11 | `support.agent.writeback` | FACHLOGIK | `:1229` | 1522 (Vertrag) | Handler | ✔ agent-reply-ticket (U6) |
| 12 | `support.conversation.open_from_thread` | GEMISCHT laut E → **FACHLOGIK** | `:442`, `:1508` | 1252 | Handler | ✔ conversation (U1) |
| 13 | `support.ticket.create_from_conversation` | GEMISCHT | `:817` | 1078 | Handler + `ctx.command('ctox.ticket.local.create')` an tickets (W1) | ✔ agent-reply-ticket |
| 14 | `support.ticket.link` | FACHLOGIK | `:783` | – | Handler | ✔ conversation |
| 15 | `support.identity.link` | FACHLOGIK | `:688`, `:3167` | – | Handler | ✔ conversation (U6) |
| 16 | `support.reply.draft` | FACHLOGIK | `:881` | – | Handler | ✔ agent-reply-ticket |
| 17 | `support.reply.send` | GEMISCHT laut E → **FACHLOGIK** | `:917` | – | Handler | ✔ agent-reply-ticket (U2) |
| 18 | `support.inbox.upsert` | FACHLOGIK | `:435`, `:1640` | – | Handler | ✔ config |
| 19 | `support.view.upsert` | FACHLOGIK | `:961`, `:1880` | – | Handler | ✔ config |
| 20 | `support.view_filter.upsert` | FACHLOGIK | `:968`, `:1925` | – | Handler | ✔ config |
| 21 | `support.bulk.assign` | FACHLOGIK | `:975` | – | Handler | ✔ bulk |
| 22 | `support.bulk.status` | FACHLOGIK | `:1017` | – | Handler | ✔ bulk |
| 23 | `support.bulk.priority` | FACHLOGIK | `:1044` | – | Handler | ✔ bulk |
| 24 | `support.bulk.snooze` | FACHLOGIK | `:1068` | – | Handler | ✔ bulk |
| 25 | `support.bulk.resolve` | FACHLOGIK | `:1091` | – | Handler | ✔ bulk |
| 26 | `support.assignment_policy.upsert` | FACHLOGIK | `:1109`, `:1698` | – | Handler | ✔ config (U10) |
| 27 | `support.macro.upsert` | FACHLOGIK | `:1116`, `:1741` | – | Handler | ✔ config |
| 28 | `support.macro.run` | FACHLOGIK | `:1123`, `:2159` | – | Handler | ✔ macro-automation |
| 29 | `support.automation_rule.upsert` | FACHLOGIK | `:1156`, `:1785` | – | Handler | ✔ config |
| 30 | `support.automation.evaluate` | FACHLOGIK | `:1163`, `:2421` | – | Handler | ✔ macro-automation |
| 31 | `support.sla_policy.upsert` | FACHLOGIK | `:1182`, `:1836` | – | Handler | ✔ config (U10) |
| 32 | `support.sla.apply` | FACHLOGIK | `:1189`, `:2595` | – | Handler | ✔ sla-reporting (U7) |
| 33 | `support.sla.recalculate` | FACHLOGIK | `:1218`, `:2698` | – | Handler | ✔ sla-reporting |
| 34 | `support.reporting.rebuild_rollups` | FACHLOGIK | `:1225`, `:1977` | – | Handler | ✔ sla-reporting, agent-reply-ticket |
| + | `support.intake.project` (neu, kein Rust-Befehlstyp) | FACHLOGIK | `:1340` (`rxdb_peer.rs:5605`) | – | Handler + Auslöser (W3) | ✔ intake |
| 35 | `support.agent.task` (neu, SKL-A1) | FACHLOGIK | ersetzt `business_os.chat.task` aus der UI | `askCtox` | Handler → `agent.task.enqueue` (Skill `support/support-workflow`, Template `prompts/agent-task.md`) | ✔ agent-task |
| + | `support.agent.task.complete` (on_complete, SKL-A1) | FACHLOGIK | Rückschreibvertrag `support.rs:1229-1278` | – (Core) | Handler → `support_agent_suggestions` + Ereignis | ✔ agent-task |
| + | `support.agent.task.failed` (on_failed, SKL-A1) | FACHLOGIK | – | – (Core) | Handler → `support_agent_requests` failed/cancelled | ✔ agent-task |

Zählung: Handler 34 + 1 · Capability-gestützt 0 (`ticket.create_from_conversation` schickt einen Befehl an die Tickets-App) · **legacy 0**.

Weitere app-spezifische Rust-Stellen (nicht Fachlogik dieses Strangs, nur eingestuft):
- `rxdb_peer_projections.rs:359–381 support_projection_collection` – Allowlist der 21 `support_*`-Collections für
  die Projektion: PLATTFORM-Verdrahtung, entfällt, sobald Collections aus `collections.schema.json` kommen (§16.13).
- `store_policy_audit.rs:50–113 support_*` – gehört zum Plattform-Diagnoseexport („support diagnostics“), nicht zur
  App support; Namensgleichheit.
- `store.rs:15553` – `is_support_command` in der Command-Gate-Liste: entfällt mit dem Handler-Routing.

## 2. Capability-Wünsche an „ctox sync v4“

| # | Name | Signatur | Beispielaufruf | Nutzer |
|---|---|---|---|---|
| W1 | entfallen (TICKAPP): kein Core-Aufruf mehr | `ctx.command('ctox.ticket.local.create', {title, body, status:'open', priority, ticket_id}, {id:'<command.id>:ticket', module:'tickets', record_id:<conversation_id>}) → {id, status}`; der Fall-Zustandsautomat gehört der Tickets-App (Eigentümerin der `ctox_ticket_*`-Collections, siehe modules/tickets/V4-HANDLERS.md). `ticket_id = 'LT-' + uuidv5(URL, 'ctox.support.ticket:<command.id>')` ohne Bindestriche, 12 Zeichen – support kennt den `ticket_key` so ohne Rückgabewert (Rust speicherte ohne `case_id` ohnehin `ticket_key = LT-…`). Abgelehnter/fehlgeschlagener Befehl → `unavailable`, keine Verknüpfung | `await ctx.command('ctox.ticket.local.create', {title, body, status:'open', priority, ticket_id}, {id, module:'tickets', record_id})` | `support.ticket.create_from_conversation` |
| W2 | `ctx.cap.mail.send` | wie E §3 | – | **heute kein Nutzer in support**: `support.reply.send` versendet nicht (U2). Erst nötig, wenn ein Freigabe→Versand-Befehl gebaut wird |
| W3 | Auslöser für `support.intake.project` | entweder `ctx.cap.schedule.upsert({id:'support-intake', every_ms, command:{type:'support.intake.project', payload:{since_ms}}})` oder Manifest-Auslöser `triggers:[{on:'communication_messages', command:'support.intake.project'}]`; plus Ort für den Cursor `max_updated_at_ms` (heute `since_by_collection` im Peer) | `await ctx.cap.schedule.upsert({id:'support-intake', every_ms:5000, command:{type:'support.intake.project', payload:{}}})` | `support.intake.project` |
| W4 | Manifest `command_rights` | `{<command_type>: <permission>}`; Core prüft vor dem Handleraufruf mit Rolle **und** Modulzuweisung (`policy.rs:483–490`: Chef/Admin **oder** `scope.assigned_to_actor`). Tabelle: `SUPPORT_COMMAND_PERMISSIONS` in `handlers/_shared.js` | – | alle 34 |
| W5 | `foreign_collections` | `communication_threads`, `communication_messages`: `read` (Eigentümer: Kommunikations-/mail-App) | – | `support.intake.project` |

Nicht gebraucht, obwohl in E genannt: `ctx.cap.mail.read` für `open_from_thread` (U1).

## 3. Unklarheiten (U) und bewusste Abweichungen (D)

**U1 `open_from_thread` ist reine Fachlogik und im UI-Import kaputt.** Rust liest keinen Thread über einen Kanal;
alle Felder kommen aus dem Payload (`support.rs:442–477`). Der JSON-Import (`index.js:1252`) schickt nur
`{thread_key, inbox_id}` ohne `conversation_id`/`record_id` → Rust **und** Handler antworten
`invalid: conversation_id is required`. Treu belassen; Korrektur gehört in den UI-Port (z. B. `conversation_id =
support_conv_<uuidv5(URL, thread_key)>` wie der Intake) oder ist eine Owner-Entscheidung.
**U2 `reply.send` versendet nicht.** Er legt nur ein Ereignis `support.reply.pending_approval` + Reporting-Ereignis an;
`send_mode:"direct"` wird abgewiesen. Ein Verbraucher dieser Freigabe existiert im Schnappschuss nirgends
(`grep` über `src/` findet nur `support.rs`). Die Antwort erreicht den Kunden heute also nie.
**U3 `record_id` fehlt im Handler-Vertrag.** Rust fällt bei fehlender `conversation_id` auf `command.record_id`
zurück; die UI setzt beides. Die Handler lesen `command.record_id`, falls der Core ihn durchreicht
(`recordIdOf`). Entscheidung nötig: Feld in den Vertrag aufnehmen oder entfernen (dann muss jeder Sender
`conversation_id` in den Payload schreiben – die UI tut das bereits überall außer U1).
**U4 Transaktionssemantik.** Angenommen: `ctx.transaction` liefert read-your-writes, ist atomar und serialisiert
gegen parallele Befehle. `claim` verlässt sich darauf (Rust: bedingtes `UPDATE … WHERE assignee_id = ''`).
**U5 Rechte** setzt der Handler nicht durch (er kennt nur `actor.role`, nicht die Modulzuweisung) → W4.
**U6 `number_field` nimmt nur ganze Zahlen** (`as_i64`). Folge, treu übernommen: `agent.writeback` mit
`confidence: 0.82` speichert 0; `identity.link` mit `confidence: 0.9` speichert 1; Zielwerte/Zeitstempel als Fließkomma
gelten als fehlend. Empfehlung nach der Paritätsabnahme: Fließkomma annehmen.
**U7 SLA-Neuanwendung**: Existiert die Zeile unverletzt (`breached_at_ms = 0`) und ist die Frist inzwischen gerissen,
bleibt `breached_at_ms = 0` und es entsteht kein `support.sla.breached`-Ereignis (`Option`-Logik
`support.rs:2616–2630, 2679`). Erst `sla.recalculate` holt es nach. Treu, mit Test.
**U8 `conversation.status` mit `resolved`** prüft keine Pflichtfelder und schließt keine SLAs, `resolve`/`bulk.status`
schon. Treu.
**U9 `agent.apply_suggestion`** setzt nur `status = applied`; der Vorschlag (z. B. Antwortentwurf) wird nicht
ausgeführt. Treu.
**U10** `assignment_policy.strategy`/`fair_distribution_*` und `sla_policy.business_hours` werden gespeichert, aber von
keinem Pfad gelesen (keine automatische Zuweisung, Fristen ohne Geschäftszeiten).
**U11** Vergleichsdetails ohne praktische Wirkung: serde formatiert `1.0` als `"1.0"`, JS als `"1"`; serde-Objekt-
Schlüsselreihenfolge in `json_scalar_string`. Gleichstände bei Sortierung nach `updated_at_ms` (SQLite unbestimmt) → nach `id`.
**U12 Hilfsmodule.** Handler importieren `./_shared.js` bzw. `./_suggestion-decision.js` (nur ECMAScript, geprüft im
leeren `vm`-Kontext, `tests/vm-isolation.test.mjs`). Die App-Laufzeit muss relative Importe innerhalb `handlers/`
auflösen; sonst Bündelschritt. Im Manifest stehen unter `handlers` nur die 35 Einstiegsdateien.
**U13** `project_macro` / `project_automation_rule` nach Ausführung sind in Rust unveränderte Neuprojektionen; die
Handler schreiben nichts, nennen die Datensätze aber in `projections` (Ergebnisfeld wie Rust).

**D1 Ein Befehl = eine Transaktion.** Rust schreibt im Autocommit je Anweisung: bei Fehlern bleiben Teilergebnisse
(z. B. `note.create` ohne `body` legt die Konversation trotzdem an; `bulk.*` bricht nach k Konversationen ab). Handler
sind alles-oder-nichts. Ausnahme `ticket.create_from_conversation`: Konversation sichern → Befehl an tickets → verknüpfen
(zwei Transaktionen, damit der Befehl nicht im Schreib-Thread liegt; entspricht der Rust-Reihenfolge).
**D2 IDs.** Rust `Uuid::new_v4()`; Handler `uuidv5(URL, "ctox.support:<command.id>:<n>")` mit gleichem Präfix
(`support_event_…`, `support_note_…`). Wiederholung desselben Befehls überschreibt statt zu verdoppeln. Abgeleitete IDs
(`support_thread_link_`, `support_applied_sla_`, `support_rollup_`, `support_conv_`) sind byteidentisch zu Rust
(UUIDv5, gegen Python-Referenzwerte getestet).
**D3** Dokumente mit `is_deleted: true` gelten als nicht vorhanden (Rust-Tabellen kennen kein Löschen; der Zustand ist in
Rust unerreichbar).
**D4 Fehlercodes** (Texte wörtlich aus Rust): „… is required“, „must be an array“, unbekannte Aktion → `invalid`;
„does not exist“ → `not_found`; „already assigned“, „missing required fields“, „SLA policy … is inactive“ → `conflict`;
`send_mode direct`, fehlende Ticket-Capability/`case_id` → `unavailable`.
**D5** `ON CONFLICT`-Erhalt nachgebaut: `created_at_ms` bleibt bei allen Upserts; Notizen behalten `author_id` und
`conversation_id`; Konfig-Upserts ersetzen alle übrigen Felder (kein Merge), wie Rusts `upsert_business_record`.

## 4. `support.intake.project` (Projektionsschleife)

Rust zieht im Peer-Takt `communication_threads` und `communication_messages` (`updated_at_ms > since`, aufsteigend,
`limit` ≤ 2000) aus der Kanal-DB, ermittelt je Thread die letzte **eingehende** Nachricht, überspringt interne Threads
`business-os/support/…` und legt Konversation (`support_conv_<uuidv5(thread_key)>`), Primärlink und einmalig das
Ereignis `support.intake.communication_thread` (Akteur `ctox`) an; unveränderte Threads werden übersprungen. Der
Handler tut dasselbe über die Fremd-Collections und liefert `{changed_count, max_updated_at_ms}`. Offen: Auslöser und
Cursorablage (W3), Leserecht (W5). Rust-Tests `support.rs:4520`, `:4599` sind als `tests/intake.test.mjs` nachgebaut.

## 5. Dateien und Prüfung

- `handlers/_shared.js` – gemeinsame Fachlogik, je Funktion `// ref: support.rs:<zeilen>`.
- `handlers/_suggestion-decision.js` – gemeinsamer Arm apply/reject.
- `handlers/<command_type>.js` × 34, `handlers/support.intake.project.js`.
- `handlers/tests/_ctx.mjs` – ctx-Attrappe (Mango-Teilmenge `$eq $ne $gt $gte $lt $lte $in $nin $exists $and $or`,
  Sortierung mit `id`-Gleichstand, Transaktion mit Rücknahme, Capability-Protokoll, 1-MiB-Ergebnisprüfung).
- Tests: `shared`, `coverage` (34 Dateien, Rechte-Tabelle, keine Node-/Netz-/Dateisystem-Importe), `conversation`,
  `agent-reply-ticket`, `config`, `bulk`, `macro-automation`, `sla-reporting`, `intake`, `vm-isolation`.
  Alle 8 Rust-Tests aus `support.rs:4402–5150` sind fachlich nachgebaut.

```
cd src/apps/business-os/modules/support/handlers
node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs                               # vm-isolation ohne Flag übersprungen
node --experimental-vm-modules --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs     # 65/65 grün (SKL-A1)
```

Nicht geprüft: Lauf gegen den echten Handler-Stub/die App-Laufzeit von „ctox sync v4“ (liegt noch nicht vor; nur
`makeCtx` austauschen), Gleichlauf gegen das Rust-Binary auf identischen Daten (Differenztest wäre der nächste Schritt).

## Capabilities

Keine Core-Capability (Stand TICKAPP 15.10.). Die Ticketanlage läuft über `ctx.command` an die Tickets-App; der
frühere Wunsch ctox.ticket.run (W1) ist entfallen.

## 6. App-Automatisierung „CTOX fragen“ (Strang SKL-A1, 15.10.2026)

Vorher: Knopf „CTOX fragen“ (`index.js` `askCtox`) → `buildSupportAgentTaskCommand` schickte den **Kernbefehl**
`business_os.chat.task` mit `required_skills:['business-os-support-workflow']` (**Phantom**, kein SKILL.md) und
einem `writeback_contract`; der Agent sollte selbst `support.agent.writeback` aufrufen. Anweisung aus
`buildAgentInstruction(item)` im UI.

Nachher (Kern §16.30/§16.34):

```
Knopf → api.command('support.agent.task', {conversation_id, request_kind, title, status, priority, record_snapshot})
     └▶ handlers/support.agent.task.js (_agent-task.js enqueueAgentTask)
          1. support_agent_requests-Zeile queued (id support_agent_request_<uuidv5(command.id)>)
          2. renderPrompt('agent-task', {conversation_id, status, priority, snapshot}) – Schnappschuss ≤ 6 000 Zeichen
          3. agent.task.enqueue({title, instruction, skill:'support/support-workflow', thread_key:'business-os/support/<id>',
             payload:{request_id, conversation_id, request_kind, source_command_id, record_snapshot, allowed_suggestion_kinds},
             idempotency_key:'support-agent:<command.id>', on_complete:'support.agent.task.complete',
             on_failed:'support.agent.task.failed'}) → task_id an die Zeile
     ── Harness mit skills/support-workflow/SKILL.md ──
     └▶ support.agent.task.complete: Ergebnis {suggestions:[…]} (Objekt oder letzter JSON-Block im Text) →
          je gültigem Vorschlag support_agent_suggestions (proposed) + Ereignis support.agent.writeback (ref support.rs:1229-1278);
          Auftrag completed (suggestion_ids, dropped_suggestions). {error} → wie failed. Freitext ohne JSON → ein summary-Vorschlag.
     └▶ support.agent.task.failed: Auftrag failed bzw. cancelled (status 'cancelled'), payload.error.
```

- Idempotenz: Abschluss nur für Aufträge im Status queued/running; sonst `{status:'ignored'}` ohne Schreibzugriff.
  Abschluss vor dem Setzen der task_id findet den Auftrag über `agent.task.get` → `payload.request_id`.
- Zone: die UI meldet Aufträge, die während der Montage auf failed/cancelled gehen, live über die Collection
  `support_agent_requests` (`reportAgentRequests`, Code `support.agent.request.<id>`; completed räumt). Kein Polling.
- Prompt: Zeilen 1–2 und 4–6 wörtlich aus `buildAgentInstruction`; Zeile 3 („Nutze den writeback_contract … für
  support.agent.writeback zurück“) ersetzt, weil das Ergebnis jetzt als Callback kommt; dazu der Schnappschuss, den
  vorher die Payload des Kernbefehls trug (die Anweisung kappt der Queue-Speicher bei 8 000 Zeichen).
- Bewusste Abweichung zu U6: `confidence` aus dem Skill bleibt Fließkomma (0–1); `support.agent.writeback` bleibt treu.
- Rechte: `support.agent.task`, `.complete`, `.failed` → `support.agent_request` (Tabelle `_shared.js`).
- Kern-Abbau (tot für support): der UI-Sender von `business_os.chat.task` mit `source_module:'support'`,
  `writeback_contract`/`response_channel:'business_os_chat'`; der Name `business-os-support-workflow`.
  `support.agent.writeback` bleibt als Befehl (externe Agenten/MCP), hat aber keinen App-internen Sender mehr.

Tests: `handlers/tests/agent-task.test.mjs` (7, agent-task-Simulation `v4/tools/agent-sim`), Bestandssuite
`tests/support.test.mjs` auf den neuen Befehl umgestellt, `tests/v4-api.test.mjs` + Zonentest (12/12).
Aufruf (Handler importieren `ctox:shared/prompt-template.js`):
`cd handlers && node --experimental-vm-modules --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (65/65).
Mutationsbeleg: 12 Mutationen (Skill, on_complete, Template, Doppelzustellung, Abbruch, Vorschlagsart, {error},
Rückweg agent.task.get, Schnappschuss-Kappung, Rechte, UI-Zone, UI-Befehl) rot.
