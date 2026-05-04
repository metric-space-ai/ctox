---
name: skillbook-runbook-bootstrap
description: Use when CTOX should turn a bounded knowledge source such as a PDF, table, manual, or exported document set into a main skill, one or more skillbooks, and standardized runbook items that can be embedded and retrieved reliably.
metadata:
  short-description: Build skillbooks and runbook items from source material
cluster: knowledge_bootstrap
---

# Skillbook Runbook Bootstrap

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when the source material is good enough to produce explicit operational knowledge instead of vague summaries.

The resulting knowledge is only durable when it is imported or referenced through SQLite-backed runtime state. Generated files on disk are build artifacts, not completed knowledge on their own.

The target structure is:

- one main skill that orchestrates work
- one or more skillbooks for channel or desk behavior
- one or more runbooks for concrete problem families
- standardized runbook items as the embedding and retrieval unit

## Core Rule

The embedding unit is the labeled runbook item, not the whole runbook and not the whole skillbook.

Labels such as `REG-03` are canonical chunk boundaries and must remain stable.

## Output Contract

Always produce:

1. `main_skill.json`
2. `skillbook.json`
3. `runbook.json`
4. `runbook_items.jsonl`
5. `build_report.json`

Read the exact field contract in [references/output-contract.md](references/output-contract.md).
Read the builder contract in [references/builder-contract.md](references/builder-contract.md).
Read the supplement contract in [references/execution-supplement-contract.md](references/execution-supplement-contract.md) when history-derived desk candidates need explicit execution enrichment.

## Workflow

1. Normalize the source material into `evidence_records`.
2. Separate the evidence into `skillbook_knowledge` and `runbook_knowledge`.
3. Propose candidate runbook items from the problem-specific layer.
4. Validate labels, chunk boundaries, tool actions, verification, and writeback semantics.
5. Write the skillbook from the shared behavior layer.
6. Write the runbook from the validated problem-family layer.
7. Split the runbook into labeled canonical items.
8. Build a deterministic `chunk_text` for every item.
9. Emit a `build_report.json` with created artifacts, rejected candidates, and open gaps.
10. Reject any item that does not have a stable label and bounded scope.

## Commands

Build a bundle from one skillbook-like file plus one runbook-like file:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/build_skillbook_runbook_bundle.py \
  --skillbook "<path-to-skillbook.md>" \
  --runbook "<path-to-runbook.md>" \
  --main-skill-id "<main-skill-id>" \
  --skillbook-id "<skillbook-id>" \
  --runbook-id "<runbook-id>" \
  --output-dir "<output-dir>"
```

Extract builder-oriented evidence and candidate gaps from ticket history:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/extract_ticket_history_builder_inputs.py \
  --input "<ticket-history.jsonl>" \
  --system "<source-system>" \
  --output-dir "<output-dir>"
```

Build a desk-only candidate bundle from extracted ticket history:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/build_ticket_history_desk_bundle.py \
  --system "<source-system>" \
  --history-report "<history_build_report.json>" \
  --history-gaps "<history_build_gaps.json>" \
  --main-skill-id "<main-skill-id>" \
  --skillbook-id "<skillbook-id>" \
  --runbook-id "<runbook-id>" \
  --output-dir "<output-dir>"
```

Render a builder result for internal review only:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/report_builder_self_work.py \
  --system "<source-system>" \
  --history-report "<history_build_report.json>" \
  --history-gaps "<history_build_gaps.json>" \
  --bundle-report "<bundle-build_report.json>" \
  --runbook-items "<runbook_items.jsonl>"
```

Publish a real knowledge event after a runbook or skillbook state change:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/publish_knowledge_event.py \
  --ctox-bin "<path-to-ctox>" \
  --system "<source-system>" \
  --events-json "<knowledge_events.json>" \
  --env-file "<runtime.env>" \
  --publish
```

Enrich a history-derived candidate bundle with explicit execution supplements:

```sh
python3 skills/system/knowledge_bootstrap/skillbook-runbook-bootstrap/scripts/enrich_ticket_history_bundle.py \
  --bundle-dir "<candidate-bundle-dir>" \
  --supplements "<execution-supplements.jsonl>" \
  --output-dir "<output-dir>"
```

## Important Boundaries

- Do not let free-form chunking decide retrieval boundaries.
- Do not embed entire manuals when the labeled item is the real unit of work.
- Do not mix desk behavior and execution detail into a single artifact.
- Do not publish a main skill without linked runbook items.
- Do not generate labels dynamically if the source already defines them.
- Do not promote a candidate item when the tool part, verification, or writeback policy is still implicit.
- Do not hide builder uncertainty in prose. Emit an explicit gap or reject the candidate.
- Do not publish builder reports, candidate counts, gap counts, artifact paths, or other build telemetry into a ticket system.
- Publish only explicit knowledge events such as `runbook_confirmed`, `runbook_corrected`, `runbook_split`, `runbook_created`, or `skillbook_updated`.
