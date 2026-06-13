# CTOX Harness Hardening Constraints

Lies dieses Dokument vor jedem Vorschlag oder Code-Change, der den CTOX
Harness, Queue/Ticket-Ausfuehrung, Long-Running-Tasks, Skill-Automation,
Review/Rework oder Spawn-/Workflow-Logik haerten soll.

CTOX-Haertung bedeutet: die vorhandene serielle Pipeline enger, pruefbarer und
auditierbarer machen. Sie bedeutet nicht: einen zweiten Scheduler, Runner,
Batch-Controller oder Workflow-Harness danebenstellen.

## Pruefsatz

Ein Vorschlag zur CTOX-Haertung ist erst reif, wenn er diese Fragen konkret
beantwortet:

1. Welche bestehende Funktion, Tabelle oder Guard-Regel wird gehaertet?
2. Bleibt `src/core/service/service.rs::route_external_messages` der
   Dispatch-Hot-Path?
3. Laeuft neue Arbeit weiter durch Queue, Ticket oder Internal Work?
4. Laeuft jede Statusaenderung weiter durch die Core State Machine?
5. Laeuft jeder neue Spawn weiter durch den Core Spawn Guard?
6. Bleibt Review-Rework auf dasselbe Main Work Item begrenzt?
7. Wird semantische Gleichheit nur bei expliziten harten Keys angenommen?
8. Bleiben Skills die Quelle fuer wiederholbare Prozeduren?
9. Gibt es Audit-, Review-, Verification- oder Process-Mining-Evidence?
10. Gibt es einen Test, der die neue Invariante beweist?

Wenn eine Antwort diese Fragen nicht code-nah beantworten kann, ist sie fuer
CTOX-Haertung nicht reif.

## Code-Anker

`HARNESS.md` ist Orientierung, aber nicht Ersatz fuer Code-Sichtung.

| Thema | Code-Anker |
| --- | --- |
| Serielle Router-Grenze | `src/core/service/service.rs::active_agent_loop_in_progress`, `src/core/service/service.rs::route_external_messages`, `src/core/service/service.rs::CHANNEL_ROUTER_SERIAL_LEASE_LIMIT` |
| Bounded Worker Slice | `src/core/service/service.rs::start_prompt_worker` |
| Durable Queue-Erzeugung | `src/core/mission/channels.rs::create_queue_task_with_metadata`, `src/core/mission/channels.rs::enforce_queue_task_spawn` |
| Queue-Status-Gate | `src/core/mission/channels.rs::enforce_queue_route_status_transition` |
| Core Transition Guard | `src/core/service/core_transition_guard.rs::enforce_core_transition`, `src/core/service/core_state_machine.rs::validate_transition` |
| Review/Rework/Closure | `src/core/service/core_state_machine.rs::validate_review_checkpoint`, `src/core/service/core_state_machine.rs::validate_rework_required_gate`, `src/core/service/core_state_machine.rs::validate_ticket_closure`, `src/core/service/core_state_machine.rs::analyze_graph` |
| Spawn-Budget und Liveness | `src/core/service/core_transition_guard.rs::enforce_core_spawn`, `src/core/service/core_transition_guard.rs::validate_core_spawn`, `src/core/service/core_transition_guard.rs::core_spawner_contracts` |
| Internal Work | `src/core/mission/tickets.rs::put_ticket_self_work_item`, `src/core/mission/tickets.rs::enforce_ticket_self_work_spawn`, `src/core/mission/tickets.rs::ticket_self_work_spawn_budget`, `src/core/service/service.rs::render_ticket_self_work_prompt` |
| Ticket-Workflows | `src/core/mission/tickets.rs::start_ticket_workflow`, `src/core/mission/tickets.rs::put_ticket_workflow_step`, `src/core/mission/tickets.rs::apply_ticket_workflow_delta`, `src/core/mission/tickets.rs::workflow_step_ready_internal`, `src/core/mission/tickets.rs::materialize_ready_workflow_steps_for_workflow` |
| Queue Repair, Spill, Restore | `src/core/mission/queue.rs::repair_queue_state`, `src/core/mission/queue.rs::spill_queue_task_to_ticket`, `src/core/mission/queue.rs::restore_spilled_queue_task` |

Wichtige durable Belege:

- `communication_routing_state`: Queue-/Message-Routingstatus.
- `ticket_self_work_items`: Internal Work und Ticket-Workflow-Schritte.
- `ctox_core_transition_proofs`: Core-State-Machine-Evidence.
- `ctox_core_spawn_edges`: akzeptierte und abgelehnte Spawn-Kanten.
- `queue_ticket_spills`: Queue-Spillover in Internal Work.
- Harte Identitaetsbelege: `message_key`, `ticket_self_work_id`,
  `dedupe_key`, Workflow-ID, Workflow-Step-ID, explizite Idempotency-Keys.

## Aufgabe

Die Aufgabe war nicht, CTOX zum besten Coding Agent umzubauen. Die Aufgabe war,
CTOX als stabilen Harness fuer lange, wiederkehrende und gemischte Arbeit zu
haerten.

CTOX muss drei Arbeitsklassen mit einer Logik tragen:

1. Standard-Tasks: einmalige, experimentelle oder zufaellige Arbeit.
2. Production-Tasks: laenger laufende Arbeit mit hoeherem Anspruch an Review,
   Evidence und Abschlussqualitaet.
3. Viele aehnliche Skill-Tasks: wiederholte, klar definierte Aufgaben mit
   variierenden Inputs, etwa PDF-Strukturierung, Extraktion, Klassifikation
   oder Transformation.

Der Unterschied zwischen diesen Klassen gehoert in Metadata, Skill-Vertrag,
Prioritaet, Budgets, Review-Tiefe, Verification-Pflicht und
Process-Mining-Auswertung. Er gehoert nicht in getrennte Runtime-Pfade.

## Nicht verhandelbare Code-Fakten

CTOX hat bereits eine serielle Ausfuehrungspipeline:

- `route_external_messages` prueft vor jedem Leasing neuer externer Arbeit, ob
  ein Agent-Loop aktiv ist. Ticket-Reconciliation und Business-OS-Writeback-
  Sweeps laufen davor weiter.
- Solange ein Worker aktiv ist, wird keine neue externe Arbeit geleast.
- Queue-Leasing ist durch `CHANNEL_ROUTER_SERIAL_LEASE_LIMIT` auf einen
  externen Lease begrenzt.
- Statuswechsel laufen ueber `enforce_queue_route_status_transition` und
  `enforce_core_transition`.
- Review, Rework und Closure werden in `core_state_machine.rs` begrenzt.
- Neue interne Arbeit laeuft durch `enforce_core_spawn` und registrierte
  Spawn-Contracts.

Diese Serialitaet ist kein Performance-Versehen. Sie ist die zentrale
Stabilitaetsannahme, weil CTOX lokale Modelle, durable State, Review, Audit und
Recovery in einem Harness verbinden muss.

## Claude-Code-Workflow-Idee in CTOX

Aus Claude-Code-Workflows ist nicht die Parallelitaet uebernehmbar, sondern das
Phasenmodell:

- Arbeit wird in begrenzte Phasen zerlegt.
- Jede Phase hat Ziel, Exit Gate und Evidence.
- Nach einer Phase wird dynamisch entschieden, welche naechsten Schritte
  entstehen.
- Prompts unterscheiden Rollen: Reducer/Planner, Leaf Worker, Reviewer.

In CTOX ist das aktuell als Ticket/Internal-Work-Scaffolding verdrahtet:

- Workflow Case: durable Root-Internal-Work-Item mit `workflow_role=case`.
- Reducer-Schritt: Internal Work Item mit `workflow_role=reducer`.
- Leaf-Schritt: Internal Work Item mit konkretem Skill, Input und Evidence.
- Vorgaengerbedingungen: `workflow_predecessor_work_ids` und
  `workflow_predecessor_step_ids`.
- Readiness: `workflow_step_ready_internal`.
- Materialisierung: `materialize_ready_workflow_steps_for_workflow`, danach
  `workflow_mark_step_queue_ready` mit `assigned_to=self` und `queued`.
- Prompting: `render_ticket_self_work_prompt`.
- Dynamische Anpassung: `apply_ticket_workflow_delta`.

Damit bleibt ein langer Task unterbrechbar. Hoeher priorisierte Arbeit, etwa
Founder-Kommunikation, kann im seriellen Router dazwischenkommen, weil der
Long-Running-Task nur als durable Ticket-/Internal-Work-Graph weiterlebt.

Die Code-Semantik belegt dabei Predecessor-Gating, Queue-Materialisierung und
Prompt-Rollen im Internal-Work-Prompt. Die fachliche Entscheidung, ob eine
Phase abgeschlossen ist oder welche naechste Phase sinnvoll ist, bleibt
Modell-/Skill-Urteil und muss durch Evidence, Review und Audit abgesichert
werden.

## Aehnliche Massentasks

"Aehnlich" ist nicht "gleich". Viele Automatisierungsaufgaben folgen demselben
Skill, haben aber unterschiedliche Inputs. Ein PDF, eine Rechnung oder ein
Supportfall kann Randfaelle enthalten, die ein anderer Input nicht enthaelt.

Deshalb darf der Harness semantische Gleichheit nicht mechanisch erraten.

Erlaubt ist Dedupe nur bei harten, expliziten Keys:

- identischer `message_key`,
- identisches `ticket_self_work_id`,
- expliziter `dedupe_key`,
- expliziter Idempotency-Key,
- explizite Workflow-/Step-ID.

Nicht erlaubt ist:

- Prompts nach Aehnlichkeit automatisch zusammenlegen,
- Varianten still in einem Task verstecken,
- Work Items unterdruecken, weil ein anderer Task "nah genug" aussieht.

Die richtige Haertung fuer Massentasks ist stabile Skill-Ausfuehrung:
expliziter `skill`, Input-Identitaet, erwartetes Output-Format, required
Evidence, erlaubte Failure-Klassen, Review gegen denselben Skill-Vertrag und
Process-Mining-Aggregation nach Skill/Task-Family.

## Determinismus-Split

Deterministisch sein muessen harte Harness-Fakten:

- Lease-Exklusivitaet,
- Status-Transitionen,
- Spawn-Budgets,
- Terminal-Gates,
- Audit-Writes,
- harte Idempotency Keys,
- Workflow-Predecessor-Erfuellung,
- Review- und Validation-Pflicht.

Nicht deterministisch entschieden werden darf fachliche Semantik:

- ob zwei aehnliche Aufgaben semantisch gleich sind,
- ob eine unklare Aufgabe production-ready behandelt werden muss,
- ob eine Workflow-Phase fachlich abgeschlossen ist,
- wie ein komplexer Task sinnvoll zerlegt wird,
- ob ein variierender Input ein Spezialfall ist.

Der richtige Split ist: Modell-/Skill-Urteil fuer Semantik, deterministischer
Apply-Pfad fuer erlaubte Queue-/Ticket-/State-Actions, Review/Verification fuer
Ergebniswahrheit, Audit/Process Mining fuer Nachvollziehbarkeit.

## Anti-Patterns

Die bisherigen Vorschlaege wurden gefaehrlich, wenn sie diese Fehler gemacht
haben:

- Greenfield statt Hot Path: neue Workflow Engines, Batch Controller,
  Scheduler-Schichten oder Runner-Logik, bevor der bestehende Router-/Queue-/
  Ticket-/Review-/Spawn-Pfad nachweislich nicht reicht.
- Aehnlich gleich gesetzt: viele Skill-Tasks als Dedupe-Problem behandelt,
  statt Skill-Vertrag, Input-Identitaet, Evidence, Review und Aggregation zu
  haerten.
- Determinismus falsch angesetzt: harte Harness-Fakten deterministisch
  erzwingen wollen ist richtig; fachliche Semantik mechanisch zu entscheiden
  ist falsch.
- Refactor statt Brandmauer: sichtbare Begriffe wie `internal work item`
  bereinigen, aber Legacy-Persistenz wie Tabellen, Spawn-Kinds, Metadata-Keys
  und Backcompat-Aliasse nicht ohne Migrationsplan umbenennen.
- Ebenen vermischt: Skill ist kein Runner, Prompt ist kein State, Review ist
  kein frei spawnbarer Nebenauftrag, Workflow ist kein zweiter Scheduler.

## Harte Leitlinie

Sinnvolle CTOX-Haertung darf nur vorhandene Pfade enger machen:

- Router-Reihenfolge unter Mischlast pruefen.
- Lease- und Repriorisierungsentscheidungen auditierbar machen.
- Internal-Work-Prompts skill- und evidence-stabiler rendern.
- Workflow-Delta-Erzeugung an Phase Evidence binden.
- Mechanische Repair-Faelle von agentischen Repair-Faellen trennen.
- Process-Mining-Findings in vorhandene Cleanup-/Repair-Pfade einspeisen.
- Review gegen denselben Auftrag/Skill/Evidence-Kontext stabilisieren.
- Queue-Spill/Restore als bestehendes Druckventil haerten.

Ausdruecklich nicht akzeptabel:

- neuer Scheduler neben dem Channel Router,
- neuer Batch Runner,
- neue Terminalpfade,
- direkte SQLite-Writes an Guard und State Machine vorbei,
- Auto-Dedupe aehnlicher Aufgaben,
- Review-Rework als review-owned Subtask,
- parallele Agenten als Grundannahme,
- globale Env-Var-Toggles fuer Runtime-Verhalten,
- Legacy-Persistenz-Rename ohne Migrations- und Backcompat-Plan.

Die zentrale Lehre: CTOX ist ein durable-state-first Harness, kein
Gruene-Wiese-Agentenentwurf. Jede Haertung muss von Code-Ankern,
State-Machine-Grenzen und Audit-Evidence ausgehen.
