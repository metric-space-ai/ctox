---
name: skillbook-runbook-bootstrap
description: Use when CTOX should turn a bounded knowledge source such as a PDF, table, manual, or exported document set into a main skill, one or more skillbooks, and standardized runbook items that can be embedded and retrieved reliably.
metadata:
  short-description: Build skillbooks and runbook items from source material
cluster: knowledge_bootstrap
---

# Skillbook Runbook Bootstrap

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when the source material is good enough to produce explicit operational knowledge instead of vague summaries.

The resulting knowledge is only durable when it is imported or referenced through the CTOX runtime store. Generated files on disk are build artifacts, not completed knowledge on their own.

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

Two equally valid write paths, depending on whether the source material is
ready to land as a full bundle or whether procedural knowledge will grow
turn-by-turn:

**Bundle import** — use when the source has been normalized into the four
JSON artifacts (`main_skill.json` / `skillbook.json` / `runbook.json` /
`runbook_items.jsonl`) on disk first. One call imports everything and
indexes embeddings:

```sh
ctox ticket history-export --system "<source-system>" --output "<ticket-history.jsonl>"
ctox ticket knowledge-bootstrap --system "<source-system>"
ctox knowledge skill import-bundle --system "<source-system>" --bundle-dir "<bundle-dir>"
ctox knowledge skill set --system "<source-system>" --skill "<main-skill-id>" --status active
```

**Incremental writes** — use when you just learned one new procedural
fact and want to make it durable without preparing a bundle directory.
Each call writes one row (the embedding for `add-item` is refreshed
automatically through the standard auxiliary backend):

```sh
ctox knowledge skill new --id "<main_skill_id>" --title "<text>" --primary-channel "<email|chat|...>" --entry-action "<text>"
ctox knowledge skill add-skillbook --id "<skillbook_id>" --title "<text>" --version "v1" --mission "<text>" --runtime-policy "<text>" --answer-contract "<text>"
ctox knowledge skill add-runbook --id "<runbook_id>" --skillbook "<skillbook_id>" --title "<text>" --version "v1" --problem-domain "<text>"
ctox knowledge skill add-item --id "<item_id>" --runbook "<runbook_id>" --skillbook "<skillbook_id>" --label "<REG-XX>" --title "<text>" --problem-class "<text>" --chunk-text "<labeled chunk>"
ctox knowledge skill refresh-item-embedding --id "<item_id>"   # only if the original add-item couldn't reach the embedding backend
```

When a runbook item should reference a record-shape data table (or vice
versa), record the edge so later turns find both sides:

```sh
ctox knowledge link --from runbook_item:"<item_id>" --to data_table:"<domain>/<table_key>" --relation consult --note "<one-line reason>"
ctox knowledge references --of runbook_item:"<item_id>"
```

Discovery and review:

```sh
ctox knowledge search --query "<topic>" --with-references
ctox knowledge skill query --system "<source-system>" --query "<ticket text>" --top-k 8
ctox ticket self-work-put --system "<source-system>" --kind "knowledge-review" --title "<title>" --body "<review body>" --publish
```

The legacy `ctox ticket source-skill-*` and `ctox ticket knowledge-*` verbs
still work — they write to the same SQLite tables — but `ctox knowledge …`
is the canonical surface; prefer it in new work. Do not execute embedded
Python helpers from this system skill. If a builder operation lacks a CTOX
CLI/API command, add that command first.

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
