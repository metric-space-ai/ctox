---
name: ticket-operating-model-bootstrap
description: Learn how a concrete helpdesk actually handles tickets by deriving recurring ticket families, handling playbooks, state norms, note-style references, and fast retrieval artifacts from a historical ticket dataset.
metadata:
  short-description: Learn a desk's real ticket-handling model from history
cluster: knowledge_bootstrap
---

# Ticket Operating Model Bootstrap

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when CTOX should learn how a specific helpdesk works from a historical ticket export.

The operating model is only durable when the learned behavior is reflected into CTOX knowledge, source-skill bindings, or other runtime state. Standalone output files do not count as completed knowledge by themselves.

This skill is not about generic clustering.
It is about deriving an operating model that CTOX can reuse while working future tickets in the same desk.

The goal is to produce:

- recurring ticket families
- family playbooks
- per-family decision support for live ticket turns
- state and closure norms
- note-style references
- fast retrieval artifacts for similar historical cases

Read these first:

- [references/method.md](references/method.md)
- [references/tool-contracts.md](references/tool-contracts.md)

## Output Contract

The run is only acceptable if it produces:

- `operating_families.json`
- `family_playbooks.json`
- `state_transition_norms.json`
- `note_style_refs.json`
- `retrieval_index.jsonl`
- `operating_model.md`

If embeddings are enabled, also produce:

- `retrieval_vectors.npy`

Every promoted family playbook must contain a `decision_support` block with:

- `operator_summary`
- `triage_focus`
- `handling_steps`
- `close_when`
- `caution_signals`
- `note_guidance`

## What Counts As Success

The output must answer, for repeated ticket families:

- what kinds of tickets exist in this desk
- how operators in this desk usually handle them
- which channels, states, and closures are normal
- what CTOX should check first on a new matching ticket
- what good handling looks like before closing
- what good historical examples look like
- what internal note style or action wording is common

## Commands

Bootstrap or refresh the ticket operating model through CTOX:

```bash
ctox ticket knowledge-bootstrap --system <system>
```

Import a prepared source-skill bundle when the dataset analysis has already produced one:

```bash
ctox ticket source-skill-import-bundle --system <system> --bundle-dir <dir>
```

Query the resulting operating model for a new ticket:

```bash
ctox ticket source-skill-query --system <system> --query "<new ticket text>" --top-k 8
```

Resolve matching skill context for a concrete ticket:

```bash
ctox ticket source-skill-resolve --ticket-key <key> --top-k 8
```

Do not execute embedded Python helpers from `skills/system`. If a needed import path is missing from `ctox ticket`, stop and add the missing CTOX CLI/API command rather than materializing this system skill as files.

## Guardrails

- Do not mistake categories for operating playbooks.
- Do not promote a family without historical examples.
- Do not claim a handling norm without evidence from repeated historical cases.
- Do not put SQLite, tool, or parser internals into any ticket-facing text.
- Do not stop at topic clustering. The point of this skill is reusable helpdesk handling behavior.
