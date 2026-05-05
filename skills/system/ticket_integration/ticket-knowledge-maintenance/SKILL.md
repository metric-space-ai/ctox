---
name: ticket-knowledge-maintenance
description: Use when CTOX must keep the ticket knowledge plane current from a live desk and turn observed gaps into visible, operator-meaningful internal work rather than hidden background assumptions.
metadata:
  short-description: Maintain ticket knowledge through visible self-work
cluster: ticket_integration
---

# Ticket Knowledge Maintenance

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when CTOX already has a ticket system attached and needs to continue building or repairing the knowledge plane from live evidence.

This is not the first attach skill. It is the steady-state maintenance skill for glossary growth, service mapping, infrastructure discovery, team model refinement, access model updates, and monitoring understanding.

If a new source of truth arrives as a structured export, worksheet, record list, or query result, first normalize it through [`tabular-knowledge-bootstrap`](../tabular-knowledge-bootstrap/SKILL.md) and then refresh the ticket knowledge projection from that shared discovery state.

If the maintained knowledge materially changes how the desk is handled, refresh the desk-specific operating skill through [`dataset-skill-creator`](../dataset-skill-creator/SKILL.md) and update the source binding with `ctox ticket source-skill-set`.

## Core Rules

The knowledge plane stays local in CTOX.

The remote ticket system is only the visible collaboration surface.

Only CTOX ticket fact/context entries, source bindings, ticket state, verifications, and related runtime records count as durable ticket state. Workspace markdown files or copied summaries do not count as completed knowledge on their own. When maintenance changes how future cases should be handled, promote the learning into the source-skill, Skillbook, Runbook, and Runbook-Item hierarchy.

When work needs to be visible to operators, create durable internal CTOX work, assign it to CTOX, and maintain a human note trail until the work is done or blocked.

Do not bypass the generic ticket primitives with direct remote API calls. If work should exist in the ticket system, create it through `ctox ticket self-work-put`, `self-work-publish`, `self-work-assign`, `self-work-note`, and `self-work-transition`.

## Commands

Refresh observed ticket knowledge:

```sh
ctox ticket sync --system "<system>"
ctox ticket knowledge-list --system "<system>" --limit 30
```

Inspect a specific knowledge domain:

```sh
ctox ticket knowledge-show --system "<system>" --domain "<domain>" --key "<key>"
```

Create visible maintenance work:

```sh
ctox ticket self-work-put --system "<system>" --kind "<kind>" --title "<title>" --body "<plain human task description>" --skill "ticket-knowledge-maintenance" --publish
ctox ticket self-work-assign --work-id "<work_id>" --assignee "self" --assigned-by "ctox"
ctox ticket self-work-note --work-id "<work_id>" --body "<plain human progress note>" --authored-by "ctox" --visibility internal
ctox ticket self-work-transition --work-id "<work_id>" --state "<open|blocked|closed>" --transitioned-by "ctox" --note "<plain human note>" --visibility internal
```

Escalate missing access or secrets:

```sh
ctox ticket access-request-put --system "<system>" --title "<title>" --body "<plain human request>" --required-scopes "<csv>" --secret-refs "<csv>" --channels "mail,jami" --publish
```

Ingest monitoring evidence:

```sh
ctox ticket monitoring-ingest --system "<system>" --snapshot-json '<json>'
```

Inspect ticket-lifecycle variants:

```sh
ctox harness-mining variants --entity-type ticket --cluster --limit 20
```

What to read:

- `pareto.variants_for_80pct`: how many distinct ticket-lifecycle paths
  account for 80% of cases. A small number (1–3) means the desk is
  well-shaped; a large number means the closure procedure is unstable —
  often a glossary or service-catalog gap that creates inconsistent
  triage.
- `variants[].activities`: the actual sequence of states a ticket walks
  through. Surprising transitions (e.g. a ticket reopened many times) hint
  at missing knowledge — phrase the maintenance work item against the
  variant, not against an individual ticket.
- `clusters[]`: near-variants merged by edit distance. A cluster with many
  member-variants but few cases each indicates *bespoke* handling — the
  desk lacks a stable shape there. Promote the dominant variant to the
  source skill via `dataset-skill-creator` and tighten the binding.

## Operating Pattern

1. Refresh the mirrored desk and inspect the current knowledge domains.
2. Decide which gaps are worth visible maintenance work.
3. Prefer a small number of durable work items over many tiny ones.
4. Publish and assign the work when operator collaboration is useful.
5. Add an initial internal note describing what you are checking.
6. Add further notes only when something changed: progress, blocker, decision, completion.
7. When the desk behavior changed enough to matter, rebuild the source skill and keep the source binding current.
8. Close the work when the knowledge gap is resolved, or block it explicitly when access or policy is missing.

## Remote Writing Style

Remote notes must read like short teammate updates.

Useful note content:

- what area you are reviewing
- what evidence you found
- what is still ambiguous
- what you need from the team
- what changed since the previous note

Forbidden remote content:

- database/storage language
- internal control metadata
- structured dumps pasted into the note body
- command instructions unless the operator explicitly asked for them

## Recommended Work Areas

- glossary-candidate-review
- service-catalog-seeding
- infrastructure-map-review
- team-model-review
- access-model-review
- monitoring-landscape-review
- adoption-gap-review

Use only the work types justified by current evidence.
