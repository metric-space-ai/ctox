# Builder Contract

This skill does not turn source material directly into final books.
It uses a fixed build pipeline with explicit intermediate artifacts.

## Build Goal

For one bounded knowledge source, produce:

- one `main_skill.json`
- one or more `skillbook.json` / `runbook.json` pairs
- one canonical `runbook_items.jsonl`
- one `build_report.json`

The builder must keep behavior and execution detail separate.

## Inputs

The builder accepts one or more source inputs.

Supported V1 source kinds:

- `pdf_manual`
- `table_faq`
- `markdown_manual`
- `existing_skillbook`
- `existing_runbook`

Every source must be normalized into a source descriptor:

- `source_id`
- `source_type`
- `title`
- `uri`
- `version`
- `ingested_at`

## Intermediate Artifacts

### `evidence_records.jsonl`

The first durable intermediate artifact.

Each record:

- `record_id`
- `source_id`
- `source_type`
- `section_ref`
- `page_ref`
- `raw_text`
- `normalized_fact`
- `domain_hint`
- `confidence`

This layer contains observations only.
No runbook items and no prose generation yet.

### `knowledge_separation.json`

Separates the normalized evidence into two buckets:

- `skillbook_knowledge`
- `runbook_knowledge`

`skillbook_knowledge` is shared work behavior:

- mission
- channel policy
- runtime policy
- answer contract
- workflow backbone
- routing taxonomy
- writeback norms

`runbook_knowledge` is bounded problem knowledge:

- problem family
- trigger
- entry condition
- blocker
- expected guidance
- tool action
- verification
- escalation

### `candidate_runbook_items.jsonl`

One proposed retrieval unit per line.

Each candidate must already have:

- stable `label`
- bounded `problem_class`
- bounded `expected_guidance`
- explicit `tool_actions`
- explicit `verification`
- explicit `writeback_policy`

Candidates that fail these conditions must not move forward unchanged.

### `build_report.json`

The mandatory build trace.

Required sections:

- `sources`
- `artifacts_written`
- `items_created`
- `items_rejected`
- `gaps_open`
- `embedding_ready_items`
- `sqlite_upserts`

## Final Artifacts

Mandatory final artifacts:

- `main_skill.json`
- `skillbook.json`
- `runbook.json`
- `runbook_items.jsonl`
- `build_report.json`

The exact runtime field contracts remain defined in [output-contract.md](output-contract.md).

## Validation Rules

The builder must reject or gap a candidate if any of these fail:

### Label integrity

- label missing
- label unstable
- label duplicates another live item in the same runbook/version

### Chunk integrity

- chunk boundary depends on neighboring text outside the labeled unit
- candidate is only a paragraph fragment without a bounded task shape
- `chunk_text` cannot be rendered deterministically from the canonical field order

### Execution integrity

- `tool_actions` are still implicit
- `verification` is still implicit
- `writeback_policy` is still implicit
- escalation boundary is vague

### Knowledge separation integrity

- behavior text leaks into runbook item logic
- execution detail leaks into skillbook policy

## Gap Types

Open gaps are first-class outputs.

Allowed V1 gap types:

- `missing_label`
- `ambiguous_boundary`
- `missing_tool_actions`
- `missing_verification`
- `missing_writeback_policy`
- `mixed_behavior_and_execution`
- `insufficient_source_evidence`
- `needs_item_split`
- `needs_item_merge`

Each gap entry must carry:

- `gap_id`
- `gap_type`
- `summary`
- `affected_source_refs`
- `proposed_resolution`
- `status`

## SQLite Persistence

Runtime books are persisted into:

- `knowledge_main_skills`
- `knowledge_main_skill_links`
- `knowledge_skillbooks`
- `knowledge_runbooks`
- `knowledge_runbook_items`
- `knowledge_sources`
- `knowledge_item_sources`
- `knowledge_embeddings`

Builder history is persisted into:

- `knowledge_build_runs`
- `knowledge_build_run_sources`
- `knowledge_build_gaps`

### Persistence order

1. upsert `knowledge_sources`
2. insert one `knowledge_build_runs` row
3. insert `knowledge_build_run_sources`
4. upsert `knowledge_main_skills`
5. upsert `knowledge_skillbooks`
6. upsert `knowledge_runbooks`
7. upsert `knowledge_runbook_items`
8. upsert `knowledge_item_sources`
9. upsert `knowledge_embeddings`
10. insert unresolved `knowledge_build_gaps`

## Promotion Rule

Only validated runbook items are retrieval-eligible.

Items may be written as candidates into the build report, but they must not be promoted into active retrieval unless:

- label is stable
- scope is bounded
- tool part is explicit
- verification is explicit
- writeback policy is explicit
- source references are present

## Runtime Intention

The runtime does not discover whole books.
It discovers one `runbook_item`, then loads its parent `runbook`, then its parent `skillbook`, then executes through the `main skill`.

That runtime order is why the builder must be strict on labeled item quality.
