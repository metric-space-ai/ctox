---
name: dataset-skill-creator
description: Create a reusable custom skill from a concrete dataset or from dataset-derived analysis artifacts. Use when CTOX should turn a data source into an operational Codex skill with SKILL.md, references, optional scripts/assets, and validation instead of leaving the result as raw knowledge bundles, RAG files, or one-off summaries.
cluster: skill_meta
---

# Dataset Skill Creator

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Generated datasets, reports, and skill files are build artifacts. For CTOX mission work, the resulting knowledge only counts as durable when it is reflected into CTOX runtime store, bindings, or knowledge records.

Use this skill when the real output should be a new skill, not just extracted knowledge.

This is a meta-skill.
It helps CTOX turn a concrete data source and its recurring patterns into a reusable Codex skill for later work.

The generated skill must be shaped around the operating goal of the dataset, not around the extraction process.

Read these first:

- [references/archetypes.md](references/archetypes.md)
- [references/method.md](references/method.md)
- [references/tool-contracts.md](references/tool-contracts.md)
- [references/validation.md](references/validation.md)

## Output Contract

The run is only acceptable if it produces a new skill folder with:

- `SKILL.md`
- `agents/openai.yaml`
- `references/`
- optional `scripts/`
- optional `assets/`

The generated skill must tell CTOX:

- when to use the skill
- what operating goal the skill serves
- which reference artifacts matter
- which tool/script entrypoints to use
- what counts as success

## Workflow

1. Identify the dataset goal.
   Decide whether the resulting skill should be:
   - `operating-model`
   - `lookup-reference`
   - `workflow`
   - `policy-gate`

2. Ensure evidence exists.
   If the dataset still needs analysis, first produce a durable analysis bundle.
   Do not generate a skill from vague summaries alone.

3. Promote only the right artifacts.
   Move stable, reusable artifacts into the generated skill:
   - references for context
   - scripts for deterministic operations
   - assets for templates or seed material

4. Write the generated skill around the operator problem.
   Do not write the generated skill around parsing, SQLite, extraction, or tooling internals.

5. Validate the generated skill.
   Run the validator and inspect whether the generated skill would actually help another CTOX instance perform the job.

## Commands

Create or update the file-backed user skill through CTOX:

```bash
ctox skills user create --name <skill-name> --description "<short purpose>" --body "<skill instructions>"
ctox skills user update --name <skill-name> --description "<short purpose>" --body "<skill instructions>"
```

For ticket-history operating models, build and import the durable knowledge through the ticket CLI:

```bash
ctox ticket history-export --system <system> --output <path>
ctox ticket knowledge-bootstrap --system <system>
ctox ticket source-skill-import-bundle --system <system> --bundle-dir <dir>
```

Do not execute embedded dataset-skill helper scripts from this system skill. If the dataset-to-skill transform needs automation beyond these commands, add a CTOX CLI/API command first.

## Guardrails

- Do not stop at dataset profiling or clustering; the output must be a usable skill.
- Do not copy extraction prose into the generated skill.
- Do not leak SQLite, parser, CLI, or internal kernel details into user-facing skill instructions.
- Do not generate a skill without a clear operating goal.
- Do not promote weak evidence as a handling norm.
