```
╔════════════════════════════════════════════════════════════════════════════╗
║                    CTOX — Harness explained                                ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  ctox start → systemd Service → Persistent Loop                            ║
║                                                                            ║
║  ┌─────────────────────────────────────────────────────────────────────┐   ║
║  │  Mission Queue (SQLite: runtime/ctox.sqlite3)                       │   ║
║  │                                                                     │   ║
║  │  Produzenten (schreiben via mission/channels):                      │   ║
║  │   • TUI Chat-Eingabe       "Deploy the new API version"             │   ║
║  │   • Email-Adapter          "Server disk full - please fix"          │   ║
║  │   • Cron-Schedule          "Run nightly backup verification"        │   ║
║  │   • Plan-Steps             "Step 3: Run integration tests"          │   ║
║  │   • Ticket-Sync (Zammad)   "JIRA-4521: Fix login timeout"           │   ║
║  └────────────────────────┬────────────────────────────────────────────┘   ║
║                           │ create_queue_task() / lease_pending_*()        ║
║                           ▼                                                ║
║  ┌─────────────────────────────────────────────────────────────────────┐   ║
║  │  CTOX Mission Loop (while service running)                          │   ║
║  │                                                                     │   ║
║  │  FOR EACH task from queue:                                          │   ║
║  │                                                                     │   ║
║  │   ┌────────────────────────────────────────────────────────────┐    │   ║
║  │   │ A. Context aufbauen                                        │    │   ║
║  │   │                                                            │    │   ║
║  │   │ ┌─ System Prompt ──────────────────────────────────────┐   │    │   ║
║  │   │ │  CTOX Runtime Contract                               │   │    │   ║
║  │   │ │  "You are CTOX, an autonomous agent on this host..." │   │    │   ║
║  │   │ │  Tool-Beschreibungen, Governance-Regeln              │   │    │   ║
║  │   │ └──────────────────────────────────────────────────────┘   │    │   ║
║  │   │                                                            │    │   ║
║  │   │ ┌─ Continuity: Focus (aktualisiert nach jedem Turn) ───┐   │    │   ║
║  │   │ │  "Active task: Deploy API v2.3 to production         │   │    │   ║
║  │   │ │   Status: in-progress                                │   │    │   ║
║  │   │ │   Next: Run smoke tests on staging                   │   │    │   ║
║  │   │ │   Blocker: None                                      │   │    │   ║
║  │   │ │   Done gate: curl /health returns 200"               │   │    │   ║
║  │   │ └──────────────────────────────────────────────────────┘   │    │   ║
║  │   │                                                            │    │   ║
║  │   │ ┌─ Continuity: Anchors (aktualisiert bei Erkenntnissen) ┐  │    │   ║
║  │   │ │  "• Repo: /opt/api at branch release/2.3              │  │    │   ║
║  │   │ │   • Config: /etc/api/config.yaml (port 8443)          │  │    │   ║
║  │   │ │   • DB migration applied: 0047_add_rate_limits.sql    │  │    │   ║
║  │   │ │   • Decision: use blue-green deploy via nginx"        │  │    │   ║
║  │   │ └───────────────────────────────────────────────────────┘  │    │   ║
║  │   │                                                            │    │   ║
║  │   │ ┌─ Continuity: Narrative (komprimierte Historie) ──────┐   │    │   ║
║  │   │ │  "Turn 1: Pulled latest, ran tests — 2 failures.     │   │    │   ║
║  │   │ │   Turn 2: Fixed auth test, DB migration applied.     │   │    │   ║
║  │   │ │   Turn 3: Built Docker image, pushed to registry.    │   │    │   ║
║  │   │ │   Turn 4: Deployed to staging, smoke test passed."   │   │    │   ║
║  │   │ └──────────────────────────────────────────────────────┘   │    │   ║
║  │   │                                                            │    │   ║
║  │   │ ┌─ Task Prompt ───────────────────────────────────────┐    │    │   ║
║  │   │ │  "Deploy the new API version"                       │    │    │   ║
║  │   │ │  (oder re-enqueued: "Continue working on the task") │    │    │   ║
║  │   │ └─────────────────────────────────────────────────────┘    │    │   ║
║  │   └────────────────────────────────────────────────────────────┘    │   ║
║  │                           │                                         │   ║
║  │                           ▼                                         │   ║
║  │   ┌────────────────────────────────────────────────────────────┐    │   ║
║  │   │ B. ctox-core Inner Loop (ein Client pro Turn-Slice)        │    │   ║
║  │   │                                                            │    │   ║
║  │   │    Think → ToolCall → ToolResult → Think → ToolCall → ...  │    │   ║
║  │   │                                                            │    │   ║
║  │   │    Beispiel:                                               │    │   ║
║  │   │    🧠 "I need to check the current deployment status"      │    │   ║
║  │   │    🔧 shell: kubectl get pods -n api                       │    │   ║
║  │   │    📋 "pod/api-v2.2-abc running since 3d"                  │    │   ║
║  │   │    🧠 "Old version running. I'll deploy v2.3..."           │    │   ║
║  │   │    🔧 shell: docker build -t api:2.3 .                     │    │   ║
║  │   │    📋 "Successfully built abc123"                          │    │   ║
║  │   │    🔧 shell: kubectl apply -f deploy-v2.3.yaml             │    │   ║
║  │   │    📋 "deployment.apps/api configured"                     │    │   ║
║  │   │    ...                                                     │    │   ║
║  │   │                                                            │    │   ║
║  │   │    ┌─ Compact Policy (beobachtet jeden Event) ─────────┐   │    │   ║
║  │   │    │                                                   │   │    │   ║
║  │   │    │  Schicht 1: EMERGENCY                             │   │    │   ║
║  │   │    │    call_input >= 75% × 128K = 98304 Tokens        │   │    │   ║
║  │   │    │    (DEFAULT_CONTEXT_THRESHOLD = 0.75)             │   │    │   ║
║  │   │    │    → ThreadCompactStart (Context wird komprimiert)│   │    │   ║
║  │   │    │    → Context schrumpft z.B. 30K → 5K (83%↓)       │   │    │   ║
║  │   │    │    → Inner Loop läuft auf kompaktem Context weiter│   │    │   ║
║  │   │    │                                                   │   │    │   ║
║  │   │    │  Schicht 2: ADAPTIVE (default 15%)                │   │    │   ║
║  │   │    │    Modell-Output >= 15% des per-call Input        │   │    │   ║
║  │   │    │    (output_budget_pct = 15)                       │   │    │   ║
║  │   │    │    (Drift-Signal: Modell wiederholt sich)         │   │    │   ║
║  │   │    │    → ThreadCompactStart                           │   │    │   ║
║  │   │    │    → Max 1 Compact pro Turn (suppress danach)     │   │    │   ║
║  │   │    └───────────────────────────────────────────────────┘   │    │   ║
║  │   │                                                            │    │   ║
║  │   │    TurnComplete → reply text                               │    │   ║
║  │   └────────────────────────────────────────────────────────────┘    │   ║
║  │                           │                                         │   ║
║  │                           ▼                                         │   ║
║  │   ┌────────────────────────────────────────────────────────────┐    │   ║
║  │   │ C. Optionaler Continuity Refresh (0-3 Turns, gleicher      │    │   ║
║  │   │    Client)                                                 │    │   ║
║  │   │                                                            │    │   ║
║  │   │  Narrative: "Turn 5: Deployed to prod, smoke test passed"  │    │   ║
║  │   │  Anchors:   + "Production: api-v2.3 running on port 8443"  │    │   ║
║  │   │  Focus:     "Status: done. Done gate: ✓ curl returns 200"  │    │   ║
║  │   │                                                            │    │   ║
║  │   │  → Modell ruft `ctox continuity-update` CLI Tool auf       │    │   ║
║  │   │  → Änderungen als Commits in ctox.sqlite3 gespeichert      │    │   ║
║  │   └────────────────────────────────────────────────────────────┘    │   ║
║  │                           │                                         │   ║
║  │                           ▼                                         │   ║
║  │   ┌────────────────────────────────────────────────────────────┐    │   ║
║  │   │ D. Mission-Status entscheiden                              │    │   ║
║  │   │                                                            │    │   ║
║  │   │  Focus/Mission-State sagt done → Mission schließen         │    │   ║
║  │   │  Mission offen + idle → Mission-Watchdog queued Slice      │    │   ║
║  │   │  Turn timeout → Timeout-Continuation als Queue-Task        │    │   ║
║  │   │  Inbound/Ticket blocker → vor Active Loop als blocked ack  │    │   ║
║  │   │  Queue/Worker hat nächsten Task → nächste Iteration        │    │   ║
║  │   └────────────────────────────────────────────────────────────┘    │   ║
║  │                                                                     │   ║
║  │  → LOOP zurück zu A. mit nächstem Task                              │   ║
║  └─────────────────────────────────────────────────────────────────────┘   ║
║                                                                            ║
║  ┌─────────────────────────────────────────────────────────────────────┐   ║
║  │  Persistenz (runtime/)                                              │   ║
║  │   ctox.sqlite3    Focus / Anchors / Narrative (Commit-Historie)     │   ║
║  │   ctox.sqlite3    Queue, Tickets, Plan, Mission-State               │   ║
║  │   ctox.sqlite3    Settings (CTOX_CHAT_MODEL_MAX_CONTEXT=131072)     │   ║
║  │   context-log.jsonl  Token-/Turn-Forensik aus DirectSession         │   ║
║  └─────────────────────────────────────────────────────────────────────┘   ║
║                                                                            ║
║  Context Window: CTOX_CHAT_MODEL_MAX_CONTEXT (default 131072 = 128K)       ║
║  Alle Schwellenwerte relativ — funktioniert bei 128K, 256K, jeder Größe    ║
╚════════════════════════════════════════════════════════════════════════════╝
```

## Review-Gate, Spawner und Subagent-Liveness

Der Harness wird als kontrollierte State Machine betrieben. Prompts duerfen die
Arbeit beschreiben, aber Review, Rework, Task-Spawns, Subagents und Abschluss
muessen durch durable State- und Prozesskanten erklaerbar bleiben.

### Rollen

- **Main Agent**: besitzt die user-sichtbare Aufgabe, das Ergebnis, die
  Schlussfolgerung und den Abschluss. Review-Feedback ist Input fuer denselben
  Parent-Task.
- **Reviewer**: ist ein Quality Gate. Er klassifiziert den aktuellen Stand, darf
  aber keine eigenstaendige Self-Work-Kaskade besitzen.
- **Spawner**: erzeugt durable Child-Arbeit und muss als Parent-Child-Kante im
  Core-Prozessgraphen registriert sein.
- **Subagent**: ist ein paralleler Leaf-Worker. Subagents duerfen nicht zu
  eigenen Review-/Rework-Orchestratoren werden.

### Review als Kontrollpunkt

Der Reviewer darf drei Arten von Findings zurueckgeben:

- `wording`: inhaltlich richtig, aber sprachliche Ueberarbeitung noetig.
- `substantive`: Inhalt, Evidenz, Implementierung oder Begruendung reicht nicht.
- `stale`: Weltzustand oder Queue hat sich geaendert; Refresh, Obsolete oder
  Consolidate ist noetig.

Nach einem Review arbeitet der Main Agent weiter. Der Review-Loop darf nicht so
modelliert werden, dass `review -> self-work -> review -> self-work` unbegrenzt
neue Arbeit erzeugt.

Jeder erneute Review-Durchlauf braucht einen Witness of Progress:

- wording rework: neuer `body_hash`
- substantive rework: neuer Substance-/Evidence-/Implementation-Pointer
- stale rework: neuer World-Pointer, Queue-Konsolidierung oder terminale
  No-Send/No-Action-Entscheidung

Ohne Witness ist der Pfad kein Fortschritt, sondern ein Loop-Kandidat.

### Spawner-Vertrag

Jeder durable Spawn braucht einen Core-Spawner-Contract:

- stabiler `spawn_kind`
- erlaubte Parent-Entity-Typen
- erlaubter Child-Entity-Typ
- finite Budget-Pflicht
- maximales Budget
- Intervention-Skill
- finite, nicht-spawnende Interventionseffekte

Akzeptierte und abgelehnte Spawns werden in `ctox_core_spawn_edges`
persistiert. Der Kernel lehnt unregistrierte, instabile, zyklische,
budgetlose, ueberbudgetierte oder budget-erschoepfte Spawns ab.

Aktuelle Contract-Familien:

| Pattern | Parent | Child | Bound |
|---|---|---|---|
| `self-work:*` | ControlPlane/Message/QueueTask/Thread/WorkItem | WorkItem | <= 64 |
| `self-work-queue-task` | WorkItem | QueueTask | <= 64 |
| `queue-task` | ControlPlane/Message/Thread/WorkItem | QueueTask | <= 64 |
| `plan-step-message` | PlanStep | Message | <= 8 |
| `schedule-run-message` | ScheduleTask | Message | <= 64 |

Alle Spawn-Arten, deren Name `review` enthaelt, brauchen ebenfalls ein finites
Budget.

### Subagents

Subagents sind leaf-only:

- alle `SessionSource::SubAgent(_)` verlieren rekursive Collaboration-/Spawn
  Tools
- Subagents erhalten kein `spawn_agents_on_csv`
- Agent-Job-Worker erhalten nur `report_agent_job_result`
- der Parent-Agent besitzt Review, Rework, Completion und owner-visible Claims
- die Review-State-Machine sieht ein Parent-Ergebnis, nicht ein Review-Gate pro
  Subagent

Thread-Spawn-Subagents sind durch `agents.max_depth` und `agents.max_threads`
begrenzt. Der Rank ist:

```text
depth_remaining = agents.max_depth - child_depth
```

Agent-Job-Worker sind durch eine finite persistierte Item-Tabelle und die
Concurrency-Grenze begrenzt. Ihr Rank ist:

```text
pending_agent_job_items
```

### Mathematischer Proof

Der ausfuehrbare Proof ist:

```bash
ctox process-mining spawn-liveness
```

Er prueft zwei Ebenen:

- `core_spawn_liveness`: registrierte durable Spawner, Budgets,
  Intervention-Skills und Graph-Zyklen
- `harness_subagent_liveness`: Depth-/Count-Bounds und leaf-only Tool-Surfaces

Das Kommando liefert nur dann `ok: true`, wenn beide Ebenen beweisbar bounded
sind.

Der Proof laeuft bewusst nicht bei jedem `rustc`-Compile via `build.rs`. Er ist
ein Repository-Konformitaets- und Release-Safety-Check, kein Typcheck. Die
richtigen Gates sind:

- Unit-Test fuer normale Testlaeufe
- CI-Gate fuer Pull-/Main-Aenderungen
- Release-Gate mit gebauter Binary vor dem Packaging

Wenn `ctox process-mining spawn-liveness` fehlschlaegt, darf das nicht mit
Prompt-Text ueberdeckt werden. Dann muss der Prozessgraph, der Transition Guard
oder die Tool-Surface korrigiert werden.

## Quellen im Code

| Element | Datei |
|---|---|
| `ctox start` | [src/main.rs:265](src/main.rs#L265) |
| Mission-Loop / systemd-Service | [src/service/service.rs:307](src/service/service.rs#L307) |
| Queue-Pfad `runtime/ctox.sqlite3` | [src/service/service.rs:328](src/service/service.rs#L328) |
| Queue task creation | [src/mission/channels.rs:685](src/mission/channels.rs#L685) |
| Queue leasing / worker dispatch | [src/service/service.rs:2411](src/service/service.rs#L2411) · [src/service/service.rs:1962](src/service/service.rs#L1962) |
| System Prompt Runtime Contract | [assets/prompts/ctox_chat_system_prompt.md:105](assets/prompts/ctox_chat_system_prompt.md#L105) |
| `PersistentSession` lifecycle | [src/execution/agent/direct_session.rs:43](src/execution/agent/direct_session.rs#L43) |
| Service currently uses per-turn clients | [src/service/service.rs:1995](src/service/service.rs#L1995) |
| Continuity-Kinds Loop (Focus/Anchors/Narrative) | [src/execution/agent/turn_loop.rs:588](src/execution/agent/turn_loop.rs#L588) |
| ADAPTIVE-Refreshbudget 15% (`output_budget_pct`) | [src/execution/agent/turn_loop.rs:490](src/execution/agent/turn_loop.rs#L490) |
| EMERGENCY-Schwelle 75% (`DEFAULT_CONTEXT_THRESHOLD`) | [src/context/lcm.rs:16](src/context/lcm.rs#L16) |
| Context-Window Default 131072 | [src/context/compact.rs:166](src/context/compact.rs#L166) |
| `ctox continuity-update` CLI | [src/main.rs:576](src/main.rs#L576) · Driver [src/execution/agent/turn_loop.rs:673](src/execution/agent/turn_loop.rs#L673) |
| Mission-Watchdog Continuation | [src/service/service.rs:2343](src/service/service.rs#L2343) |
| Timeout-Continuation | [src/service/service.rs:3290](src/service/service.rs#L3290) |
| `runtime/context-log.jsonl` | [src/execution/agent/direct_session.rs:584](src/execution/agent/direct_session.rs#L584) |
| `CTOX_CHAT_MODEL_MAX_CONTEXT=131072` seed | [install.sh:1276](install.sh#L1276) |
| Producer Email / Cron / Tickets | [src/mission/communication_email_native.rs](src/mission/communication_email_native.rs) · [src/mission/schedule.rs](src/mission/schedule.rs) · [src/mission/tickets.rs](src/mission/tickets.rs) · [src/mission/ticket_zammad_native.rs](src/mission/ticket_zammad_native.rs) |

## Persistenz-Policy

- **Core-State** lebt in einer einzigen Datei `runtime/ctox.db` (Mission-Queue, Tickets, Plan, Schedule, Governance, Secrets, Communication, Knowledge/Skillbooks/Runbooks, LCM, Verification). Alle Pfade werden über [src/paths.rs](src/paths.rs) zentral aufgelöst — `paths::core_db(root)` ist die Single-Source-of-Truth.
- **Tool-Stores** behalten ihre eigenen Dateien, weil sie in ihrem Tool gekapselt sind: `runtime/ticket_local.db` (lokaler Ticket-Adapter), `runtime/ctox_scraping.db` (Scrape-Capability), `runtime/documents/ctox_doc.db` (Doc-Stack).
- **System-Skills** liegen unter [skills/system/](skills/system) im Repo und werden via `include_dir!` in `tools/agent-runtime/skills/src/assets/samples` in die Binary eingebettet; bei Service-Start extrahiert der Codex-Skill-Manager sie nach `$CODEX_HOME/skills/.system/`. User-Skills — darunter die initial aus [skills/packs/](skills/packs) ausgerollten Packs — bleiben lose im Ordner `$CODEX_HOME/skills/<name>/`, sichtbar und dynamisch veränderbar.
- **Runtime-erzeugte Desk-Skills** (Output von `dataset-skill-creator` / `system-onboarding`) landen als User-Skills im gleichen Ordner, nicht im Repo. Metadaten, Bindings und Embeddings dieser Skills werden über die `knowledge_*`-Tabellen in `ctox.db` persistiert.
- **Erste Start nach dem ctox.db-Merge:** die historischen Dateien `cto_agent.db` und `ctox_lcm.db` werden einmalig in `runtime/ctox.db` konsolidiert und nach `runtime/backup/<ISO8601>/` verschoben — ausgelöst von [src/service/db_migration.rs](src/service/db_migration.rs), aufgerufen früh in `main()` ([src/main.rs](src/main.rs)).

## Änderungen gegenüber der Ursprungsversion

1. **`context-log.jsonl` wieder aufgenommen** — der aktuelle Code schreibt Turn- und Token-Forensik aktiv nach `runtime/context-log.jsonl`; zusätzlich bleiben Token-Zähler in `ctox.sqlite3`.
2. **Dispatch-Pfad korrigiert** — statt eines nicht vorhandenen `lease_next_for_thread()` zeigt die Grafik jetzt den realen Pfad aus Queue-Erzeugung, Leasing und Worker-Dispatch (`create_queue_task`, `lease_pending_*`, `start_prompt_worker`).
3. **`PersistentSession` präzisiert** — im aktuellen Service-Lauf wird pro Turn-Slice ein Client aufgebaut; innerhalb eines einzelnen `run_chat_turn_with_events_extended`-Aufrufs teilen sich Main-Turn und Continuity-Refresh denselben Client.
4. **Continuity-Refresh als optional markiert** — die drei Refresh-Turns laufen nur bei Output-Budget- oder State-Transition-Triggern, nicht nach jedem Turn blind.
5. **Continuation-Pfade konkretisiert** — offene Arbeit wird derzeit vor allem über Mission-Watchdog- und Timeout-Continuation-Queue-Tasks fortgesetzt, nicht nur über einen simplen `status=active + Queue leer`-Pfad.
6. **Quellverweise aktualisiert** — tote oder verschobene Links wurden auf die aktuell relevanten Stellen im Code angepasst.
