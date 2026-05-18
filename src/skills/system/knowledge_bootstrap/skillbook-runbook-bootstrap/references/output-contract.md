# Output Contract

This bootstrap path emits five mandatory artifacts and may emit generated helper artifacts for the execution layer.

## `main_skill.json`

The orchestration layer.

Required fields:

- `main_skill_id`
- `title`
- `primary_channel`
- `entry_action`
- `resolver_contract`
- `execution_contract`
- `resolve_flow`
- `writeback_flow`
- `linked_skillbooks`
- `linked_runbooks`

`resolver_contract` must describe how the agent resolves an inbound case to one runbook item.

`execution_contract` must describe the concrete output target. For the email example this is a suggested or drafted reply, not a generic prose answer.

## `skillbook.json`

The shared behavior contract.

Required fields:

- `skillbook_id`
- `title`
- `version`
- `mission`
- `non_negotiable_rules`
- `runtime_policy`
- `answer_contract`
- `workflow_backbone`
- `routing_taxonomy`
- `linked_runbooks`

## `runbook.json`

The problem-family layer.

Required fields:

- `runbook_id`
- `skillbook_id`
- `title`
- `version`
- `status`
- `problem_domain`
- `item_labels`

## `runbook_items.jsonl`

One canonical retrieval item per line.

Required fields:

- `item_id`
- `runbook_id`
- `skillbook_id`
- `label`
- `title`
- `problem_class`
- `trigger_phrases`
- `entry_conditions`
- `earliest_blocker`
- `expected_guidance`
- `tool_actions`
- `verification`
- `writeback_policy`
- `escalate_when`
- `sources`
- `pages`
- `chunk_text`

## `chunk_text`

`chunk_text` is deterministic and must always be rendered from the same field order:

1. label
2. title
3. problem_class
4. trigger_phrases
5. earliest_blocker
6. expected_guidance
7. escalate_when
8. sources/pages

This text is the embedding input.

## `build_report.json`

The build report is mandatory.

Required fields:

- `builder_version`
- `sources`
- `evidence_record_count`
- `skillbook_knowledge_count`
- `runbook_knowledge_count`
- `items_created`
- `items_rejected`
- `gaps_open`
- `embedding_ready_items`
- `sqlite_upserts`

This report is not a retrieval artifact.
It is the operator and builder audit artifact for the bundle build.

## Optional generated helper artifacts

When the target system needs a concrete execution layer, the bootstrap path may also emit helper artifacts such as:

- `sample_queries.json`
- `execution_examples.jsonl`

These are not the retrieval unit. They are only execution helpers for the main skill layer.
