---
name: knowledge-refresh
description: Build or update the Knowledge Book (skillbook, skills, runbooks, resources) of a finished Web Research task from its verified sources and evidence. Use when the research app hands over a research.knowledge.refresh task. Every factual statement cites the original source lineage; the result is delivered as one JSON file in the task workspace and written back by the research app.
class: app
app: research
state: active
cluster: research
---

# Knowledge Refresh (research app)

You turn the verified result of one systematic research run into a reusable
Knowledge Book: a domain skillbook with its skill, runbooks for recurring
analyses and document types, and resources. The research app gives you the
complete lineage in the task payload. You do **not** write into any database
or store yourself; you deliver one result file, the research app validates it
and writes it into its own collections when the task completes.

## Inputs (task payload)

- `research_task_id`, `research_run_id`, `research_command_id`,
  `knowledge_domain` – identity of the research; repeat them verbatim in the
  result.
- `knowledge_version_id`, `knowledge_version`, `immutable_knowledge_version` –
  the knowledge version you build on. Do not invent another version.
- `source_tables` – table key → table id of the research Knowledge tables
  (`source_catalog`, `evidence_points`, …).
- `source_lineage`, `snapshot_lineage`, `evidence_lineage`, `claim_lineage`,
  `requested_snapshot_hashes` – the only admissible evidence.
- `lineage_status` – `complete` or `incomplete`. With `incomplete`, build only
  what the available lineage supports and say what is missing in `gaps`.
- `knowledge_contract` – `create_or_update` (skillbook, skills, runbooks,
  resources), `stable_identity`, `provenance_required`,
  `source_of_truth: original_sources`, `citations` fields, `refresh_policy`.
- `update_mode: upsert` and `existing_elements` (if present): elements of an
  earlier refresh. Reuse their ids.

## Rules

1. **Upsert, never fork.** Use the existing stable ids; update elements in
   place. Never create a parallel copy of the same Knowledge Book. A new
   element gets a stable id derived from domain and purpose
   (`<knowledge_domain>/<kind>/<slug>`), not a random id.
2. **The skill is a hub, not a source.** Every factual statement points back to
   the original `source_id`/`canonical_url` from `source_catalog` and
   `evidence_points` – with `claim_id`, `evidence_id`, `snapshot_id` where
   known. A statement without such a citation is left out.
3. **Keep layers apart.** Sources, evidence, tables and derivations stay
   separately traceable. Derived conclusions say what they are derived from.
4. **Runbooks re-read.** Runbooks describe recurring analyses and document
   types that read this Knowledge and, when needed, the original sources
   again. A runbook has a title, a purpose, inputs, steps and the expected
   output.
5. **Preserve the link** to research task, research run and Knowledge tables
   so later research runs update the same elements.
6. Read sources only through the given lineage (snapshots, canonical URLs).
   Do not start new discovery; that is the job of a research run.
7. Do not start child agents.

## Result (required)

Write exactly one file `knowledge-refresh/result.json` (UTF-8 JSON) into the
task workspace:

```json
{
  "schema_version": "ctox.research.knowledge-refresh.v1",
  "research_task_id": "…",
  "research_run_id": "…",
  "research_command_id": "…",
  "knowledge_domain": "…",
  "knowledge_version_id": "…",
  "summary": "two or three sentences: what was created or updated",
  "elements": [
    {
      "id": "<stable id>",
      "kind": "skillbook | skill | runbook | resource",
      "title": "…",
      "summary": "…",
      "markdown": "full content of the element",
      "citations": [
        { "source_id": "…", "canonical_url": "…", "evidence_id": "…", "claim_id": "…", "snapshot_id": "…" }
      ]
    }
  ],
  "gaps": ["evidence that is missing for a complete Knowledge Book"]
}
```

The research app rejects the result (task ends as failed) when

- an identity field differs from the task payload,
- an element has no `id`, `kind` or `title`, or an unknown `kind`,
- an element of kind `skill`, `skillbook` or `resource` has no citation,
- a citation names a `source_id` that is not in `source_lineage`.

Finish your turn with a short plain-text summary (same content as `summary`).
