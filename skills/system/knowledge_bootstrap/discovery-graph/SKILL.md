---
name: discovery-graph
description: Build or refresh a concrete infrastructure inventory from real host, network, service, storage, runtime, journal, and repo discovery commands. Use when CTOX needs to inspect a machine or repo, gather raw discovery evidence, translate it into the shared SQLite discovery model, and leave behind explicit entities, relations, evidence, and rerunnable discovery runs instead of ad hoc shell output.
cluster: knowledge_bootstrap
---

# Discovery Graph

Use this skill to inspect a host or repo, gather raw discovery evidence, and persist a normalized discovery model in SQLite.

SQLite-backed discovery state is the durable knowledge plane. Discovery captures, entities, relations, evidence, continuity, ticket knowledge, and other runtime DB state count as knowledge. Workspace notes or exported summaries do not count as durable knowledge by themselves.

The important rule is:

- helper scripts are available and should be reused when they fit
- helper scripts are not the authority
- the raw evidence is the authority

Use this skill when the primary question is:

- what exists here?
- what is the technical scope?
- which hosts, services, units, timers, ports, files, and dependencies are involved?

Do not use this skill as the first choice for an already-scoped health or saturation question. In that case prefer `reliability_ops`.

You are expected to read the raw output, inspect the helper scripts when needed, and decide whether to use, patch, bypass, or replace them.

## Operating Model

Treat this skill as three layers:

1. canonical capabilities
2. inspectable helper resources
3. agent-authored interpretation

The canonical capabilities are:

- capture raw discovery evidence
- persist raw captures into SQLite
- translate evidence into `graph.json`
- persist normalized entities, relations, and evidence
- query or export the stored graph

The Python files in `scripts/` are open helper resources that implement these capabilities locally. They are preferred helpers, not opaque black-box tools.

This skill uses the shared SQLite kernel with `skill_key=discovery_graph`.

If a helper script behaves incorrectly or is too rigid for the current case:

- open it
- inspect it
- patch it if needed
- or bypass it and work from raw command output directly

Do not force yourself through a helper script that is clearly wrong for the case.

## Preferred Helpers

These are the preferred local helper scripts. They are all inspectable under `scripts/`.

### Raw Capture Helpers

- `scripts/linux_collect.py`
  - runs real discovery commands
  - returns structured raw capture JSON
- `scripts/capture_run.py`
  - convenience wrapper for a full raw sweep
  - runs collectors and persists captures immediately

### Persistence Helpers

- `scripts/discovery_store.py`
  - initializes the SQLite schema
  - stores raw captures
  - stores normalized graph facts

### Query Helpers

- `scripts/discovery_query.py`
  - summarizes the stored graph
  - exports it for visualization

### Bootstrap Normalizer

- `scripts/normalize_minimum.py`
  - conservative bootstrap normalizer
  - useful for a first pass or regression-safe fallback
  - not the final authority for difficult cases

If a difficult case exceeds the bootstrap normalizer:

- build `graph.json` yourself from the raw captures plus the schema resources
- then persist it with `discovery_store.py store-graph`

## Invocation Patterns

Raw capture helpers:

```sh
python3 <skill-path>/scripts/linux_collect.py list-collectors
python3 <skill-path>/scripts/linux_collect.py run --collector host_identity
python3 <skill-path>/scripts/linux_collect.py run --collector network
python3 <skill-path>/scripts/linux_collect.py run --collector listeners
python3 <skill-path>/scripts/linux_collect.py run --collector services
python3 <skill-path>/scripts/linux_collect.py run --collector journals
python3 <skill-path>/scripts/linux_collect.py run --collector processes
python3 <skill-path>/scripts/linux_collect.py run --collector storage
python3 <skill-path>/scripts/linux_collect.py run --collector containers
python3 <skill-path>/scripts/linux_collect.py run --collector kubernetes
python3 <skill-path>/scripts/linux_collect.py run --collector repo_inventory --repo-root <repo>
python3 <skill-path>/scripts/linux_collect.py run-all --repo-root <repo>
python3 <skill-path>/scripts/capture_run.py --db <db-path> --repo-root <repo>
```

Store and query helpers:

```sh
python3 <skill-path>/scripts/discovery_store.py init --db <db-path>
python3 <skill-path>/scripts/discovery_store.py store-capture --db <db-path> --input <capture.json>
python3 <skill-path>/scripts/discovery_store.py store-graph --db <db-path> --input <graph.json>
python3 <skill-path>/scripts/discovery_query.py summary --db <db-path>
python3 <skill-path>/scripts/discovery_query.py export-cytoscape --db <db-path>
```

Bootstrap normalizer:

```sh
python3 <skill-path>/scripts/normalize_minimum.py --db <db-path> --run-id <run-id>
```

Raw capture JSON contains:

- collector name
- executed commands
- raw `stdout`
- raw `stderr`
- exit code
- timestamps

`store-graph` expects agent-authored JSON that matches the schema in [references/sqlite-model.md](references/sqlite-model.md).

## Tool Contracts

Think in these capability contracts, regardless of which helper script you use:

- `discovery.capture_raw`
  - read-only
  - returns raw command output plus metadata
- `discovery.store_capture`
  - persist-only
  - writes raw capture JSON into SQLite
- `discovery.store_graph`
  - persist-only
  - writes agent-authored `graph.json` into SQLite
- `discovery.query`
  - read-only
  - summarizes or exports the stored graph
- `discovery.normalize_bootstrap`
  - evaluate-only
  - optional helper that proposes a conservative first graph from stored captures

The helper scripts under `scripts/` are current local implementations of these capabilities. They are not a replacement for your judgment.

## Workflow

1. Define scope.
   State which host, repo, runtime, or narrow environment slice is being inspected.
2. Inspect the available helpers.
   Read the helper script that best matches the task before relying on it in a nontrivial case.
3. Choose the capture path.
   Usually use `capture_run.py` or `linux_collect.py`. If they do not fit, run the underlying commands directly.
4. Read raw output.
   Use capture JSON as transport, but inspect the real command output before translating it.
5. Translate into the shared model.
   Convert findings into entities, relations, and evidence according to [references/sqlite-model.md](references/sqlite-model.md) and [references/normalization-rules.md](references/normalization-rules.md).
6. Persist captures first, facts second.
   Raw captures go in first under one logical `run_id`. Normalized facts go in only after you have enough evidence.
7. Reuse the current run, do not spray synthetic runs.
   A full sweep should share one `run_id` across all captures and the later `store-graph` write.
8. Mark ambiguity explicitly.
   If a service match, dependency edge, or ownership guess is uncertain, keep it out of the normalized graph or mark it clearly in `attrs_json`.
9. Use `full_sweep` only when the sweep is truly broad enough.
   If you are refreshing the whole host/repo inventory, set `full_sweep=true` so missing facts can be marked inactive. Do not use it for narrow partial probes.
10. Always finish with one `store-graph` write.
   A successful sweep is not complete after `store-capture`. You must persist at least a partial normalized graph for the same `run_id`.
11. Use the bootstrap normalizer only when it helps.
   `normalize_minimum.py` is a conservative helper. Use it when it accelerates a safe first graph. For difficult cases, derive `graph.json` yourself.
12. Treat missing captures as a hard failure.
   If `normalize_minimum.py` says there are no captures for the run, the sweep failed. Fix the capture path first; do not reply with success.
13. Patch helpers when needed.
   If a helper script is close but wrong, patch it instead of working around the same defect repeatedly.

## Minimum Completion Gate

If the relevant collectors succeeded, the normalized graph should contain at least:

- one `host`
- `listener` entities for proven `ss` bindings
- `process` entities for proven PIDs from `ps` or `systemctl show`
- `systemd_unit` entities for proven services or timers from `systemctl`
- `repo` and `repo_file` entities when `repo_inventory` succeeded

And these relations whenever the evidence directly supports them:

- `listener -> managed_by -> process`
- `process -> managed_by -> systemd_unit`
- `process -> runs_on -> host`
- `systemd_unit -> runs_on -> host`
- `systemd_unit -> defined_in -> repo_file`
- `systemd_unit -> scheduled_by -> timer`
- `journal_finding -> about -> systemd_unit`

If a relation is not provable, omit it. But do not skip `store-graph` entirely just because some relations are missing.

## Operator Feedback Contract

Answer for the operator first.

Use these exact headings:

- `**Status**`
- `**State**`
- `**Scope**`
- `**Autonomous Actions**`
- `**Escalation**`
- `**Current Findings**`
- `**Next Step**`

`State` must be one of:

- `proposed`
- `prepared`
- `executed`
- `blocked`

For discovery work, a successful sweep is usually `prepared` unless it also activated or changed live behavior.

If the scope remains unclear or the sweep is partial, say that explicitly and queue the next discovery slice instead of implying the inventory is finished.

## Guardrails

- Stay read-only unless the user explicitly asks for mutation.
- Do not treat helper output as already-semantic truth; it is still just capture or a conservative proposal.
- Do not invent relations from names alone.
- Distinguish raw evidence, interpreted fact, and unresolved hypothesis.
- Prefer live host/runtime evidence over repo hints when they conflict. Keep repo-only findings as weaker hints until a live command confirms them.
- If a command is unavailable, let the store persist it as a `coverage_gap` fact and continue with the remaining collectors.
- Do not let a helper script hide logic from you. Read it when the case is unclear.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md): roles, limits, and expected use of bundled scripts
- [references/discovery-commands.md](references/discovery-commands.md): collector command palette
- [references/sqlite-model.md](references/sqlite-model.md): normalized storage model and `graph.json` shape
- [references/normalization-rules.md](references/normalization-rules.md): conservative translation rules for listeners, processes, units, repo files, timers, and journal findings
