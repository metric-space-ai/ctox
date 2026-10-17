# ctox – Handler-Port Shell v4 (Strang H6)

Stand 15.10.2026. Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`
(`crew_commands.rs`, `harness_cockpit.rs`, `store.rs`, `store_projections.rs`, `store_policy.rs`) und die
Kern-Helfer `src/core/crew/mod.rs`, `src/core/mission/channels/mod.rs` aus dem Klon `ctox-sync-v4/clones/app-laufzeit`
(Basis 4794af03d), weil `crew::safe_prose`/`Soul::validate`/`route_status_is_terminal` nicht im Schnappschuss liegen.

Tests: `node --test src/apps/business-os/modules/ctox/handlers/tests/*.test.mjs` → 30/30 grün (Stand Strang CREWAPP); alle Handler laden
zusätzlich in einem nackten `vm`-Kontext (keine Node-/Web-Globals).

## Grundbefund

E stufte alle 15 Befehlstypen als FAEHIGKEIT ein. **Seit Strang CREWAPP (15.10.) gilt die Owner-Regel „keine
App-Logik im Core“ (CATALOG.md „Kein crew-Primitiv“): Crew-Register, Lernsätze, Wesen-Wissen (Continuity je Wesen)
und Crew-Zuordnung sind ctox-App-Collections** (`ctox_crew_members`, `ctox_crew_learnings`, `ctox_crew_continuity`,
`ctox_crew_continuity_commits`, `ctox_crew_assignments`); die sieben `ctox.crew.*`-Handler lesen und schreiben sie
selbst und rufen keine `agent.crew.*`-Capability mehr auf. Core bleiben nur die generische Warteschlange
(`communication_routing_state`, Runtime-Store `queue.pause`, Worker-Kapazität) und das Audit (`agent.flow.record`). **§16.25 (Strang CQ):** `ctox_queue_tasks` ist nur eine
Projektion der Core-Queue (Wahrheit in `ctox.sqlite3`) und für alle Handler **read-only**; geschrieben wird
ausschließlich über `ctx.cap.agent.queue.task.{update, delete}` (Statuswechsel = `task.update` mit `patch.route_status`,
Kernvertrag caps/queue.rs). Die **Fachlogik** der Arme (Pflichtfelder, Grenzen, Prosa-Filter
`safe_prose`, Seelen-/Spezialitäten-Prüfung, Kapazitätsgrenze, Statusübergangsregeln release/block/retry, Audit-
Begrenzung, ID-Auflösung `queue-…`/Befehls-ID, „nur nicht-terminale Aufgaben vor dem Löschen stornieren“) ist jetzt
JS. Die Löschmarken in `ctox_queue_tasks`/`business_commands` setzt seit Strang CQ der Core (`task.delete`).
Damit ist jede Regel ohne Rebuild änderbar; die Capability macht nur noch den Schreibvorgang.

Die „~5k Zeilen“ aus C1 sind überwiegend **Projektionen des Kernzustands** (`harness_cockpit_projections.rs` 1908,
`harness_cockpit_chat.rs` 422, `harness_cockpit_schedule.rs` 130, Queue-Projektion in `store_projections.rs`), keine
Befehlstypen. Einstufung: Spiegel eines Core-eigenen Zustands (Warteschlange, Harness-Ereignisse, Läufe, Crew-Status)
→ bleibt Teil der Fähigkeit `agent.queue` (der Core spiegelt seinen eigenen Zustand in
`ctox_queue_tasks`, `ctox_harness_events`, `ctox_harness_status`, `ctox_runs`, `business_chats`). Crew ist kein
Spiegel mehr (siehe „Crew als App-Collections“). **UNKLAR/Owner-Entscheid**: Wenn §16.13 „Projektionen“ auch diese Spiegel meint, ist ein
zweiter Schritt nötig (Core liefert Rohereignisse, App-Handler projiziert); nicht in diesem Strang begonnen.

## Tabelle (Reihenfolge = UI-Häufigkeit; Basis: Aufrufstellen in modules/ + shared/, keine Laufzeitmessung)

| # | Befehlstyp | Einstufung (E) | Rust-Ort | Ziel | Status |
|---|---|---|---|---|---|
| 1 | `ctox.crew.assign` | APP-LOGIK (CREWAPP) | `crew_commands.rs:194` + `crew/mod.rs:356` | `handlers/ctox.crew.assign.js`: `ctox_crew_members` (aktiv?) + `agent.queue.task.claim` + `ctox_crew_assignments` | portiert, getestet |
| 2 | `ctox.crew.member.update` | APP-LOGIK (CREWAPP) | `crew_commands.rs:111` | `handlers/ctox.crew.member.update.js` → `ctox_crew_members` | portiert, getestet |
| 3 | `ctox.crew.member.create` | APP-LOGIK (CREWAPP) | `crew_commands.rs:83` | `handlers/ctox.crew.member.create.js` → `ctox_crew_members` (Kapazität in `ctx.transaction`) | portiert, getestet |
| 4 | `ctox.crew.memory.update` | APP-LOGIK (CREWAPP) | `crew_commands.rs:136` + `lcm/mod.rs:2611-2900` | `handlers/ctox.crew.memory.update.js` + `_lib/continuity.js` → `ctox_crew_continuity(_commits)`, Sicht `ctox_crew_members.memory` | portiert, getestet |
| 5 | `ctox.crew.learning.confirm` | APP-LOGIK (CREWAPP) | `crew_commands.rs:202` (×3) | `handlers/ctox.crew.learning.confirm.js` + `_lib/learning.js` → `ctox_crew_learnings` | portiert, getestet |
| 6 | `ctox.crew.learning.update` | APP-LOGIK (CREWAPP) | `crew_commands.rs:202` (×3) | `handlers/ctox.crew.learning.update.js` → `ctox_crew_learnings` | portiert, getestet |
| 7 | `ctox.crew.learning.delete` | APP-LOGIK (CREWAPP) | `crew_commands.rs:202` (×3) | `handlers/ctox.crew.learning.delete.js` → `ctox_crew_learnings` | portiert, getestet |
| 8 | `ctox.queue.pause` | FAEHIGKEIT | `harness_cockpit.rs:118` | `handlers/ctox.queue.pause.js` + `cap.agent.queue.control({action:'pause'})` | portiert, getestet |
| 9 | `ctox.queue.release` | FAEHIGKEIT | `harness_cockpit.rs:152` (×3) | `handlers/ctox.queue.release.js` + `_lib/cockpit.js` | portiert, getestet |
| 10 | `ctox.queue.retry` | FAEHIGKEIT | `harness_cockpit.rs:152` (×3) | `handlers/ctox.queue.retry.js` | portiert, getestet |
| 11 | `ctox.queue.block` | FAEHIGKEIT | `harness_cockpit.rs:152` (×3) | `handlers/ctox.queue.block.js` | portiert, getestet |
| 12 | `ctox.queue.capacity` | FAEHIGKEIT | `harness_cockpit.rs:136` | `handlers/ctox.queue.capacity.js` + `cap.agent.queue.control({action:'capacity'})` | portiert, getestet |
| 13 | `ctox.task.update` | FAEHIGKEIT | `store.rs:15676` + `:21801` | `handlers/ctox.task.update.js` + `cap.agent.queue.task.update` | portiert, getestet |
| 14 | `ctox.task.delete` | FAEHIGKEIT | `store.rs:15709` + `:21846` | `handlers/ctox.task.delete.js` + `cap.agent.queue.task.update` mit `patch.route_status='cancelled'` (nur nicht-terminal) + `cap.agent.queue.task.delete` | portiert, getestet |
| 15 | `ctox.queue.abort_turn` | FAEHIGKEIT | `harness_cockpit.rs:148` | `handlers/ctox.queue.abort_turn.js` (fest „unsupported“, keine Cap) | portiert, getestet |

**Legacy-Rest (`ctx.cap.legacy.<type>`): 0 von 15.**

Nicht zu dieser App gehörig (C1 listet sie als Sender in `modules/ctox`, E ordnet sie anders zu – hier nicht portiert):
`ctox.runtime_settings.save` (E: settings/ctox, PLATTFORM), `ctox.workjet.project_execution_policy.v1` (workjet),
`web_stack.auth_assist.request` (browser/Fähigkeit), `business_os.chat.task` (reine Queue, `record_command`).

## Capabilities (Bedarf, nur OS-Primitive)

Stand Strang CREWAPP. Die App braucht nur noch das generische Warteschlangen-Primitiv `agent.queue` (CATALOG.md Nr. 9)
und die Plattform-Evidenz `agent.flow.record` (MAPPING.md (c)). Keine `agent.crew.*`-Capability mehr (CATALOG.md
„Kein crew-Primitiv“); `agent.queue.projection.refresh` entfällt (v4: `ctox_queue_tasks` ist Live-Projektion).
`task.get`, `task.for_command`, `task.claim` sind beim Kern als generische Primitive angefordert (Orchestrator,
15.10.) und stehen noch nicht in CAP-1/2 (`check-caps --calls` meldet sie deshalb als ABWEICHUNG); die Aufrufe
verwenden genau die angeforderten flachen Formen.

| Capability | Signatur | Beispielaufruf |
|---|---|---|
| `ctx.cap.agent.flow.record` | `({event_kind, title, message_key?, metadata}) → {}` – `harness_flow::record_harness_flow_event` (dauerhafte Prozess-Evidenz; Plattform) | `await ctx.cap.agent.flow.record({event_kind:'crew.control', title:command.type, metadata})` |
| `ctx.cap.agent.queue.control` | E §3: `({action:'pause'\|'capacity'\|'release'\|'block'\|'retry', task_id?, value})` → pause: `{}`; capacity: Kapazitätsobjekt; Task-Aktionen: `QueueTaskView` (`channels::control_queue_task`, `reset_attempts` bei retry) | `await ctx.cap.agent.queue.control({action:'retry', task_id, value:{route_status:'pending', clear_note:true, reset_attempts:true}})` |
| `ctx.cap.agent.queue.task.get` | `({task_id}) → QueueTaskView\|null` (`channels::load_queue_task`) – angefordert | `await ctx.cap.agent.queue.task.get({task_id})` |
| `ctx.cap.agent.queue.task.for_command` | `({command_id}) → {task_id}\|null` (`find_queue_task_for_command`) – angefordert | `await ctx.cap.agent.queue.task.for_command({command_id})` |
| `ctx.cap.agent.queue.task.update` | `({task_id, patch}) → QueueTaskDoc` – auch Statuswechsel über `patch.route_status`; siehe „Queue-Task-Schreib-Capabilities“ | `await ctx.cap.agent.queue.task.update({task_id, patch:{route_status:'cancelled', status_note}})` |
| `ctx.cap.agent.queue.task.delete` | `({task_id, command_id?}) → {task_id, command_id, deleted}` | `await ctx.cap.agent.queue.task.delete({task_id, command_id})` |
| `ctx.cap.agent.queue.task.claim` | `({task_id, holder, route_status_in}) → {changed:0\|1}` – angefordert; siehe unten | `await ctx.cap.agent.queue.task.claim({task_id, holder:'crew:…', route_status_in:['pending','blocked']})` |

#### `ctx.cap.agent.queue.task.claim({task_id, holder, route_status_in}) → {changed}`

Generischer atomarer Vor-Lease-Claim einer Warteschlangen-Aufgabe (Verallgemeinerung von
`crew::assign_member_before_lease`, `crew/mod.rs:356-380`). Der Core setzt in **einer** bedingten Anweisung den
opaken Halter der Aufgabe (`holder`, App-Zeichenkette; heute Spalte `crew_assigned_member_id`, Ziel: generische
Spalte `claim_holder`), **nur wenn** die Aufgabe nicht verleast ist (`lease_owner IS NULL`, Kern-Invariante) **und**
`route_status ∈ route_status_in` (Fachregel, kommt vom Handler: `['pending','blocked']`). Rückgabe `changed` = Zahl
geänderter Zeilen (0 oder 1); 0 ist kein Fehler – den Text `assignment requires an unleased pending or blocked task`
und den Code `conflict` erzeugt der Handler. Danach erneuert der Core die Live-Projektion der Aufgabe
(`refresh_queue_assignment_projection`). Ein bestehender Halter wird überschrieben (wie Rust). Der Router des Core
bevorzugt beim Leasen den Halter (bisher `crew::select` „assigned: Manuelle Zuordnung vor dem Lease“). Fehler:
`invalid` (Form: `task_id`/`holder` leer, `route_status_in` keine nicht-leere Liste aus Statusnamen), `unavailable`.

### Queue-Task-Schreib-Capabilities (Vertragsvorschlag, §16.25)

Gemeinsam für `task.update` und `task.delete` (Statuswechsel laufen seit CREWAPP über `task.update`, Kernvertrag
caps/queue.rs; das frühere `task.set_status` entfällt):

- **Read-only-Projektion.** `ctox_queue_tasks` wird nur vom Core geschrieben. Ein Handler-`put`/`patch`/`remove` darauf
  (auch in `ctx.transaction`) endet mit `{code:'forbidden'}`; der Core muss das im Einheits-Store genauso erzwingen.
  Lesen (`ctx.get`/`ctx.read`) bleibt erlaubt.
- **Atomar im Core.** Jede Funktion ändert in **einer** Transaktion gegen `ctox.sqlite3` den Queue-Eintrag
  (`communication_routing_state`/Queue-Zeile), das zugehörige `business_commands`-Dokument und die Projektion
  `ctox_queue_tasks/<task_id>` (Live-Projektion).
  Schlägt etwas fehl, bleibt alles unverändert.
- **Keine Fachregeln im Core.** Statusübergangsregeln, Pflichtfelder, Text-Normalisierung, Audit-Notizen
  (`business-os: edited by …`) prüft bzw. baut der Handler **vor** dem Aufruf. Der Core prüft nur Form und eigene
  Invarianten (Lease, Existenz). Rechte (`CtoxTaskManage`, Task-Scope) prüft der Core vor dem Handler.
- **`task_id`** ist die bereits aufgelöste Kern-ID (ohne `queue-`-Präfix, keine Befehls-ID); Auflösung macht der
  Handler (`_lib/tasks.js::resolveTaskId`). Leer/kein String → `invalid`.
- **`QueueTaskDoc`** = das neu projizierte `ctox_queue_tasks`-Dokument (wie `write_queue_task_projection`: `id`,
  `command_id`, `title`, `prompt`, `priority`, `route_status`, `status_note`, `updated_at_ms`, …).
- **Fehler** in der Handler-Fehlerform `{code, message}`: `invalid` (Form), `not_found` (Task unbekannt),
  `conflict` (Kern-Invariante verletzt, z. B. Lease), `unavailable` (Kern-DB nicht erreichbar).

#### `ctx.cap.agent.queue.task.update({task_id, patch}) → QueueTaskDoc`

Port von `channels::update_queue_task` + `write_queue_task_projection` (`store.rs::update_ctox_task`).

| Parameter | Typ | Bedeutung |
|---|---|---|
| `task_id` | string | Kern-Task-ID |
| `patch` | object | erlaubte Schlüssel: `title`, `prompt`, `priority`, `status_note`, `route_status` (je `string\|null`); `null`/fehlend = unverändert. Andere Schlüssel → `invalid` |

Fehler: `invalid` (kein Objekt, unbekannter Schlüssel, Nicht-String, kein einziger Wert gesetzt), `not_found`,
`conflict` (Statuswechsel einer `leased` Aufgabe auf etwas anderes als `cancelled`).
Hat die Aufgabe ein `business_commands`-Dokument, aktualisiert der Core dessen Titel/Prompt-Spiegel in derselben
Transaktion.

```js
const task = await ctx.cap.agent.queue.task.update({
  task_id: 'task-1',
  patch: { title: 'Neu', prompt: null, priority: null, status_note: 'business-os: edited by Chefin (chef)' },
});
// → { id: 'task-1', command_id: 'cmd-9', title: 'Neu', route_status: 'pending', status_note: '…', updated_at_ms: … }
```

##### Statuswechsel über `task.update` (`patch.route_status`)

Port von `transition_business_command_for_task(root, task_id, status, …, reason)` mit Rückfall
`update_queue_task(route_status=status, status_note=reason)`. Ob ein Übergang fachlich zulässig ist (z. B. „nur
nicht-terminale Aufgaben stornieren“), entscheidet der Handler vorher. Zulässige Ziel-`route_status`: `pending`,
`blocked`, `cancelled`, `failed`, `completed`, `handled`, `superseded`; `leased` → `invalid` (Leases vergibt nur der
Worker). `status_note` wird beim Statuswechsel getrimmt übernommen (leer = Notiz bleibt). Wirkung: Queue-Eintrag
`route_status`; zugehöriges `business_commands`-Dokument bekommt denselben Status und `observed_at_ms=now`.

```js
await ctx.cap.agent.queue.task.update({
  task_id: 'task-1', patch: { route_status: 'cancelled', status_note: 'business-os: deleted by Chefin (chef)' },
});
// → { id: 'task-1', command_id: 'cmd-9', route_status: 'cancelled', status_note: 'business-os: deleted by …', … }
```

#### `ctx.cap.agent.queue.task.delete({task_id, command_id?}) → {task_id, command_id, deleted}`

Port des Schreibteils von `store.rs::delete_ctox_task`: Löschmarke auf `ctox_queue_tasks/<task_id>`;
`business_commands/<command_id>` erst `status='cancelled'`, `observed_at_ms=now`, dann Löschmarke – alles in einer
Transaktion. Der Queue-Eintrag selbst bleibt als terminaler Routing-Zustand erhalten (wie bisher; der Handler hat ihn
vorher per `task.update` (`route_status='cancelled'`) storniert, falls er nicht terminal war). Den Status ändert `delete` **nicht**.

| Parameter | Typ | Bedeutung |
|---|---|---|
| `task_id` | string | Kern-Task-ID |
| `command_id` | string? | getrimmt; fehlt er, nimmt der Core `command_id` der Projektion (`queue_projection_command_id`) |

Rückgabe: `command_id` = tatsächlich markierte Befehls-ID oder `null`; `deleted` = ob eine Projektion existierte.
Idempotent: unbekannte `task_id` → `{deleted:false}`, kein Fehler (Rust setzt die Löschmarke ebenfalls ungeprüft).
Fehler: `invalid` (Form).

```js
await ctx.cap.agent.queue.task.delete({ task_id: 'task-1', command_id: 'cmd-9' });
// → { task_id: 'task-1', command_id: 'cmd-9', deleted: true }
```

Handler-Zuordnung: `ctox.task.update` → 1× `task.update`; `ctox.task.delete` → 0–1× `task.update` mit `route_status` (nur
nicht-terminal) + 1× `task.delete`. `ctox.queue.release/block/retry` laufen weiter über `queue.control` (Lease-Reset,
Priorität, `reset_attempts` sind dort Teil des Kern-Übergangs) und schreiben die Projektion ebenfalls nicht.
Testattrappe: `handlers/tests/_ctx.mjs` simuliert `task.update` (inkl. `route_status`) und `task.delete` und wirft `forbidden` bei jedem Handler-Write
auf `ctox_queue_tasks`; `tests/task.test.mjs` prüft das plus einen Quelltext-Wächter über alle Handler.

Manifest-Rechte, die der Core **vor** dem Handler prüft (bisher `CentralCommandPolicyRequirement::for_command` bzw.
`enforce_command_policy`): `ctox.task.*` → `CtoxTaskManage` im Task-Scope; Crew → `CrewManage`; Queue → Cockpit-Recht.
Der Handler wiederholt `session_can_manage_task` nicht (identische Prüfung, nur Core kennt Grants).

## Crew als App-Collections (Strang CREWAPP)

Schema: `v4/schemas/ctox.collections.schema.json` (von Hand gepflegt, `check-schemas.mjs` grün). Alle Schreibwege
laufen über `ctx.put/patch/remove` bzw. `ctx.transaction`; jede Collection-Konstante steht in `_lib/crew.js` bzw.
`_lib/continuity.js`.

| Collection | Inhalt | Schreiber | Rust-Referenz |
|---|---|---|---|
| `ctox_crew_members` | Register: `name, shape, color, archived, soul, specialties, stats, created_at, updated_at(_ms)`; Sicht `memory {anchors, narrative, anchor_count, experience_count, updated_at}`; Laufzeitfelder `state, active_task_id, domain, last_*_at_ms` | create/update/memory-Handler; Laufzeitfelder nur Core (siehe Kern-Folgen) | `crew/mod.rs:105-113`, `crew_commands.rs:83-135`, Projektion `harness_cockpit_projections.rs:1524-1608` |
| `ctox_crew_learnings` | Lernsätze; neu Pflichtfeld `normalized_text` (UNIQUE je Mitglied, Handler prüft → `conflict`) | learning-Handler | `crew/mod.rs:114-121`, `crew_commands.rs:200-241` |
| `ctox_crew_continuity` | Wesen-Wissen je Wesen: ein Kopf je `(member_id, kind ∈ anchors\|narrative)`, `id = contdoc_crew:<member_id>_<kind>`, `head_commit_id`, `content` (gerenderter Text) | memory-Handler | `lcm/runtime_support.rs:849-874, 1090-1106` |
| `ctox_crew_continuity_commits` | Commit-Kette: Basis `contbase_crew:<member_id>_<kind>` (Vorlage), danach `contc_<sha256[0..8]>` mit `parent_commit_id`, `diff_text` (normalisierter Diff bzw. `<tool:full_replace len=…>`/`<tool:string_replace find_len=… replace_len=…>`), `rendered_text` | memory-Handler | `lcm/mod.rs:2674-2900, 5249-5479` |
| `ctox_crew_assignments` | Zuordnung Mitglied → Task (`id = task_id`, `member_id`, `assigned_at`) | assign-Handler (nach erfolgreichem `agent.queue.task.claim`) | `crew/mod.rs:356-380` |

Migration: `ctox_crew_learnings` v0 → v1 (`normalized_text` Pflicht) über
`v4/schemas/migrations/ctox/ctox_crew_learnings.js` (leitet den Wert wie `crew::normalized` aus `text` ab). Die neuen
Felder an `ctox_crew_members` sind optional mit Default (kompatibel, gleiche Generation).

## Kern-Import (einmalig, liefert der KERN)

Daten, die heute nur im Core liegen; der Kern überträgt sie einmal beim Umstieg (danach schreibt nur noch die App).

| Quelle im Core (`ctox.sqlite3`) | Ziel-Collection | Feldabbildung |
|---|---|---|
| `crew_members` | `ctox_crew_members` | `id, name, shape, color` 1:1; `archived` INTEGER → bool; `soul_json/specialties_json/stats_json` → `soul/specialties/stats` (JSON geparst; nicht parsebare Zeilen auslassen und melden wie `members_with_errors`); `created_at/updated_at` (RFC 3339) 1:1 + `created_at_ms/updated_at_ms`; `state/active_task_id/domain/last_memory_read_at_ms/last_learning_at_ms/memory` wie bisher die Projektion `project_crew` |
| `crew_member_learnings` | `ctox_crew_learnings` | `id, member_id, text, normalized_text, kind` 1:1; `scope_json` → `scope`; `evidence_run_id` → `projected_worker_run_id(...)` wie Projektion; `created_at` → `created_at_ms` (= `updated_at_ms`); `confirmed_by_owner/archived` INTEGER → bool |
| `continuity_documents` + `continuity_commits` mit `conversation_id = member_conversation_id(member_id)` und `kind ∈ {anchors, narrative}` | `ctox_crew_continuity` | `id = contdoc_crew:<member_id>_<kind>`, `member_id`, `kind`, `head_commit_id` 1:1, `content` = `rendered_text` des Kopf-Commits, `created_at/updated_at` 1:1 (Millisekunden-Text, `iso_now`), `updated_at_ms` = Zahl davon |
| `continuity_commits` derselben Dokumente (ganze Kette ab Kopf über `parent_commit_id`) | `ctox_crew_continuity_commits` | `id = commit_id`, `document_id` = neue Dokument-ID, `member_id`, `kind`, `parent_commit_id`, `diff_text`, `rendered_text`, `created_at` 1:1, `created_at_ms = updated_at_ms` = Zahl davon; Basis-Commit `contbase_<conv>_<kind>` → `contbase_crew:<member_id>_<kind>` (Elternverweis des Folge-Commits mit umschreiben) |
| `communication_routing_state.crew_assigned_member_id IS NOT NULL` | `ctox_crew_assignments` | `id = task_id = message_key`, `member_id = crew_assigned_member_id`, `assigned_at = updated_at`, `updated_at_ms` = Zahl davon; die Spalte bleibt als Halter des generischen `task.claim` |

`crew_attempts` (Auswahl-/Erfolgsprotokoll des Routers) bleibt Harness-Evidenz im Core und wird nicht importiert.

## Kern-Folgen (nicht in diesem Strang, an den Kern)

1. `agent.queue.task.claim` (oben) bereitstellen; die Spalte `crew_assigned_member_id` generisch als Halter führen.
2. Harness/Router (`crew::select`, `render_persona`, `render_memory_block`, `run_learning_tick`, Finalisierung) lesen
   Mitglieder, Lernsätze und Continuity künftig aus den App-Collections (Sync-Store) statt aus den Kern-Tabellen und
   schreiben Laufzeitfelder (`state, active_task_id, stats, domain, last_*_at_ms`) nur feldweise per Patch in
   `ctox_crew_members` bzw. neue Lernsätze/Erfahrung in `ctox_crew_learnings`/`ctox_crew_continuity(_commits)` – gleiche
   Übergangsschuld wie die IoT-Engine (CATALOG.md). Die Projektion `project_crew` entfällt danach.
3. Leserechte: `ctox_crew_continuity_commits` (Wissenshistorie) sollte wie `ctox_crew_learnings` nur
   admin/chef/founder lesen (`DOC_COLLECTION_RIGHTS` in `gen-manifest.mjs`, nicht in diesem Strang geändert).

## Abweichungen / Unklarheiten (markiert)

1. **ID neuer Mitglieder**: Rust `uuid::new_v4`. Handler nimmt `crypto.randomUUID()` (§16.21-Global), sonst
   `ctx.uuid()`, zuletzt die Befehls-ID (`crew:<command.id>`).
2. **Atomarität**: Seit CREWAPP laufen Kapazitätszählung + Anlage, Lernsatz-Existenz + Schreiben und
   Continuity-Commit + Kopf + Mitglieds-Touch je in einer `ctx.transaction` (wie Rust). Nur `ctox.crew.assign` hat
   zwei Schritte: atomarer Claim im Core, danach `ctox_crew_assignments`-Dokument; scheitert das Schreiben, ist die
   Aufgabe gebunden, der Eintrag fehlt – erneutes Zuordnen ist idempotent.
3. **Regex `safe_prose`**: Rusts Unicode-`\w`/`\b` per `\p{…}`-Klassen und Lookarounds nachgebaut; Rust-`(?i)` mit
   Unicode-Faltung (z. B. Kelvin-Zeichen) nicht 1:1.
4. **Zeitstempel**: Rust `chrono::Utc::now().to_rfc3339()` (`+00:00`, Nanosekunden); Handler `toISOString()` (`Z`, ms).
5. **Nutzer-Label** `"{display_name} ({role})"`: Handler-`actor` trägt nur `{id, role}`; Name aus `business_users`,
   Rückfall `actor.id`. Besser: Core reicht `actor.display_name` mit (Vertragswunsch).
6. **Fehlercodes**: Rust kennt nur `anyhow`-Fehler. Zuordnung: Pflicht/Format → `invalid`; „not found“ → `not_found`;
   Lease-/Statusregeln → `conflict`; Kapazität → `quota`.
7. **`ctox.task.delete`**: Rust setzt `business_commands.status='cancelled'` in der Tabelle und schreibt danach eine
   Löschmarke. Seit Strang CQ (§16.25) macht das der Core atomar in `task.delete`; der Handler schreibt weder
   `ctox_queue_tasks` noch `business_commands`. Stornieren (`task.update`) und Löschen (`delete`) sind zwei
   Core-Aufrufe wie in Rust (dort ebenfalls getrennte Schritte); scheitert `delete` nach dem Stornieren, bleibt die
   Aufgabe storniert, aber sichtbar – erneutes Löschen ist idempotent. Rückgabe trägt wie Rust `status:'cancelled'`.
8. **`ctox.queue.capacity`**: Rust gibt das Kapazitätsobjekt mit `ok:true` zurück – übernommen, Form kommt aus der Cap.
9. **Continuity (CREWAPP)**: Rust adressiert die Dokumente über die gehashte i64-Konversation von `crew:<member_id>`;
   die App verwendet den Schlüssel selbst (`contdoc_crew:<member_id>_<kind>`). Die Commit-ID hasht deshalb
   `crew:<member_id>` statt der Zahl (Form `contc_` + 16 Hex gleich; ohne WebCrypto FNV-Rückfall gleicher Form).
   `continuity_init_documents` legt für Crew nur anchors/narrative an (die Focus-/Mission-State-Spur nutzt Crew nicht).
   Rust legte die Dokumente schon vor der Modus-Prüfung im LCM an (eigene Verbindung); der Handler prüft erst alle
   Eingaben und schreibt dann alles in einer Transaktion – ungültige Befehle hinterlassen nichts.
10. **Fehlercodes Continuity**: Diff-Formfehler (`unknown continuity section in diff: …`, `unsupported continuity diff
    line: …`, `continuity diff requires a section header before +/- lines`) → `invalid`; Ersetzen gegen den
    Dokumentstand (`find string not present …`, `… matches N times …; refusing ambiguous replace`, `replace produced no
    change`) → `conflict`. Texte wörtlich aus `lcm/mod.rs`.
11. **Lernsatz-Eindeutigkeit**: Rust scheiterte an `UNIQUE(member_id, normalized_text)` mit einem SQLite-Fehler; der
    Handler meldet `conflict` „learning text already exists for this member“ (ganze Änderung verworfen wie die
    Rust-Transaktion). `updated_at_ms` wird bei jeder Änderung auf jetzt gesetzt (die Projektion setzte `created_at_ms`).

## Rechte (Collection-Lesen, Manifest `collections.<name>.rights`)

Strang SHARED (R4, 15.10.2026): Die Lese-Sperren je Collection standen bisher in `shared/permissions.js`
(`ADMIN_ONLY_COLLECTIONS`, `USER_PRIVATE_COCKPIT_COLLECTIONS`, Spiegel von `policy.rs`
`ADMIN_ONLY_COLLECTIONS`/`role_may_read_collection`/`user_readable_cockpit_projection`). Sie sind App-Daten der
Eigentümer-App und stehen jetzt im Manifest; der Generator überträgt diese Tabelle (`DOC_COLLECTION_RIGHTS` in
`v4/tools/manifest/gen-manifest.mjs`). Collections ohne Eintrag: alle Rollen lesen (policy.rs-Standard).

| Collection | `rights` | Basis |
|---|---|---|
| `business_users` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
| `business_module_acl` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
| `business_module_source_files` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
| `business_module_commits` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
| `business_module_source_blob_chunks` | `{read:{roles:['admin','chef']}}` | policy.rs `ADMIN_ONLY_COLLECTIONS` |
| `ctox_harness_events` | `{read:{roles:['admin','chef','founder']}}` | policy.rs `is_cockpit_projection` ohne `user_readable_cockpit_projection` (Owner 08.09.2026) |
| `ctox_runs` | `{read:{roles:['admin','chef','founder']}}` | wie oben |
| `ctox_crew_learnings` | `{read:{roles:['admin','chef','founder']}}` | wie oben |

`ctox_runtime_settings` (ebenfalls admin-only) gehört keiner App (Plattform) und steht deshalb nicht hier.

## SKL-D: Kontext-Chat als Standardbefehle der ctox-App (15.10.2026)

Entscheidung (Orchestrator): Der CTOX-Chat gehört der App `ctox`. Sie bietet allen Apps zwei Standardbefehle an; der
gemeinsame UI-Baustein `ctox:shared/context-chat.js` (app-neutral: Menüeinträge, Kontext-Payload, Absenden über
`api.command`) ersetzt die acht Kopien des Rechtsklick-Menüs „Mit CTOX arbeiten / App ändern“.

| Befehl | Handler | Template | Skill | Rückruf |
|---|---|---|---|---|
| `ctox.chat.task` (Modus `data`\|`ask`) | `handlers/ctox.chat.task.js` | `prompts/chat-data.md` / `prompts/chat-ask.md` | keiner (Bestand: keiner) | `ctox.chat.task.complete` / `.failed` |
| `ctox.app.modify.request` (Modus `app`) | `handlers/ctox.app.modify.request.js` | `prompts/app-modify.md` | System-Skill `business-os-app-module-development` (`src/skills/system/product_engineering/`, generisch, ohne Präfix; in der Core-Skill-Liste von agent-sim vorhanden, belegt durch `sim.enqueued[0].skill = {scope:'core'}`) | `ctox.chat.task.complete` / `.failed` |
| `ctox.chat.task.complete` | `handlers/ctox.chat.task.complete.js` | – | – | on_complete: Antwort als Nachricht `{role:'ctox', replyFor: task_id}` in `business_chats` |
| `ctox.chat.task.failed` | `handlers/ctox.chat.task.failed.js` | – | – | on_failed (auch `status:'cancelled'`): `{role:'ctox', failureFor: task_id, status}` |

Gemeinsame Logik `handlers/_lib/context-chat.js`. Payload des Bausteins: `{source_app, record_ref, selection, text, mode,
title?, thread_key?, context?, chat_id?}`. Prüfung: `source_app` App-ID, `text` 1–8000 Zeichen (ref store.rs:206), Modus je
Befehl (`app` nur über `ctox.app.modify.request`), sonst `invalid` ohne Auftrag. `ctox.app.modify.request` prüft im
Handler Rolle owner/admin/chef oder Recht `apps.source.edit` (sonst `forbidden`; UI-Spiegel nur Darstellung).
Prompt-Texte wörtlich aus dem Bestand: Befehlsblock/Payload-/Kontext-Vorschau aus `store.rs:27009-27100 command_prompt`
(sensible Schlüssel raus, `store.rs:27126`), Frage-Anweisung aus den App-Kopien („Beantworte die folgende Frage
ausschließlich lesend …“), App-Ziel-Block aus `store.rs:27354-27560` (module_id = `source_app`, Standard
`runtime-installed-module`). Chat: `business_chats/<chat_id>` (Standard `chat_id` = Befehls-ID; Folge-Turns mit `chat_id`),
Nutzernachricht `user_<command_id>`, `tracking_*` auf den laufenden Turn; Rückruf findet den Chat über
`agent.task.get().payload.chat_id`, sonst `tracking_task_id`. Idempotent: je task_id höchstens eine
`replyFor`/`failureFor`-Nachricht; eine zweite Zustellung (auch complete nach failed) schreibt nichts.

Tests: `node --import ./v4/tools/node/ctox-shared.mjs --test modules/ctox/handlers/tests/context-chat.test.mjs` (8) und
`shared/context-chat.test.mjs` (4), gegen `v4/tools/agent-sim/agent-task-sim.mjs`. Mutationsbeleg (Temp-Spiegel, Arbeitsbaum
unverändert): 15/15 rot – Handler: on_complete-Typ, Skill bei App ändern, Rechteprüfung, Geheimnisfilter,
Doppelzustellung, Abbruch-Status, app über chat.task, Textlimit, ask-Template, Tracking-Ende, Fallback
tracking_task_id; Baustein: app ohne Recht, Typ je Modus, Sprung ohne command_id, leerer Text.

Sender umgestellt: documents, spreadsheets, buchhaltung, matching (alle Modi), knowledge und research (nur „App ändern“;
Daten bleiben `ctox.knowledge.chat` bzw. `business_os.data.modify`), desktop (Symbol- und Flächenmenü: Text per
`api.shell.dialog`, dann Baustein; vorher Sprung `{chat}`, den diese App nie las), mail „An CTOX übergeben“
(`buildMailRouteCommands`), ctox „Aufgaben aus Datei“ und „Aufgabe fortsetzen“. app-store: toter Test-Haken
`appStoreContextChatDetail` entfernt.

### Manifest-Wünsche (Kern §16.40, für den Generator)

```json
{
  "handlers": {
    "ctox.chat.task": "handlers/ctox.chat.task.js",
    "ctox.app.modify.request": "handlers/ctox.app.modify.request.js",
    "ctox.chat.task.complete": "handlers/ctox.chat.task.complete.js",
    "ctox.chat.task.failed": "handlers/ctox.chat.task.failed.js"
  },
  "commands_in": [
    { "type": "ctox.chat.task", "from": "*" },
    { "type": "ctox.app.modify.request", "from": "*" }
  ],
  "command_rights": {
    "ctox.chat.task": { "_permission": "jeder angemeldete Nutzer (Kontext-Auftrag an CTOX)" },
    "ctox.app.modify.request": { "_permission": "apps.source.edit", "_roles": ["owner", "admin", "chef"] }
  },
  "prompts": ["prompts/chat-data.md", "prompts/chat-ask.md", "prompts/app-modify.md"]
}
```
`from:'*'` nur für diese zwei ausdrücklich allen Apps angebotenen Befehle. `ctox.chat.task.complete`/`.failed` sind
reine Rückrufe (nicht in `commands_in`). Das Befehlsergebnis `{ok, task_id, chat_id, status:'queued'}` geht nach §16.40
per Callback an die auslösende App; die Agenten-Antwort selbst landet im Chat der ctox-App.

### Kern-Abbau (jetzt tot, sobald alle Sender umgestellt sind)
- `store.rs:26700-26790 create_ctox_queue_task` für `business_os.chat.task` und `ctox.business_os.app.modify` aus diesen
  Sendern; `store.rs:27418-27484 required_skill_names`/`is_business_os_app_module_command` (Skill-Wahl per Typ/Modus);
  `business_os_app_command_prompt` (`:27354`) für den Kontext-Chat-Weg; die Chat-Projektion R7 (`store.rs:6900-6960`)
  für diese Aufträge.
- Noch nicht umgestellt (außerhalb SKL-D): outbound `index.js:6532` (Chat-Detail `business_os.chat.task`, Strang OUT3),
  creator `index.js:204` (liest nur), appsec-pentest Bezeichnungstabelle `:2615`.
