---
name: ticket-knowledge
description: Use when Codex is handling ticket work and must load or inspect the CTOX ticket knowledge plane before any operational action.
metadata:
  short-description: Load and inspect ticket knowledge before ticket handling
cluster: ticket_integration
---

# Ticket Knowledge

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill before ticket classification, dry run, execution, or writeback whenever ticket understanding depends on the CTOX knowledge plane.

## Core Rule

No ticket should be handled operationally without loading the relevant ticket knowledge first.

The ticket system is only the communication surface. CTOX runtime knowledge is the source of truth.

Only CTOX ticket knowledge counts as durable knowledge. Workspace markdown files, copied ticket notes, or ad hoc analysis documents do not count as knowledge unless the same facts are present in the runtime store.

## Commands

### Refresh observed knowledge from mirrored ticket data

```sh
ctox ticket knowledge-bootstrap --system "<system>"
```

### Inspect ticket knowledge references

```sh
ctox ticket knowledge-list [--system "<system>"] [--domain "<domain>"] [--status "<status>"] [--limit "<n>"]
ctox ticket knowledge-show --system "<system>" --domain "<domain>" --key "<key>"
```

### Load the reference context for a concrete ticket

```sh
ctox ticket knowledge-load --ticket-key "<ticket_key>" [--domains "source_profile,label_catalog,glossary,service_catalog,infrastructure_assets,team_model,access_model,monitoring_landscape"]
```

### Inspect CTOX self-work around source understanding

```sh
ctox ticket self-work-list [--system "<system>"] [--state "<state>"] [--limit "<n>"]
```

## Operating Pattern

1. Refresh or inspect the source-specific knowledge plane.
2. Load the ticket-specific knowledge context.
3. If access or secret context is missing, stop operational handling and inspect the secret store or create an explicit access request through the onboarding or access skill.
4. If monitoring context is missing for infra/process questions, ingest or request monitoring evidence instead of guessing.
5. If domains are missing, stop operational handling, inspect existing self-work, and if needed create or continue a justified onboarding or maintenance work item instead of proceeding blindly.
6. If you continue an existing self-work item, assign it to CTOX if needed and leave a plain internal note about what knowledge gap you are resolving.
7. Only continue into dry run or execution once the knowledge load is ready.
8. Verify that the source is actually operationalized when that matters:
   - `ctox ticket sources`
   - `ctox ticket source-skills`
   - `ctox ticket list --system "<system>"`
   - `ctox ticket knowledge-list --system "<system>" --limit 20`
   If the source is only running in local fallback or partial monitoring mode, say so explicitly and handle the work as knowledge/onboarding correction, not mature ticket execution.

## Important Boundaries

- Do not treat remote ticket fields as durable truth when CTOX knowledge already contradicts them.
- Do not hide knowledge gaps in prose; surface them explicitly through the ticket knowledge commands or self-work items.
- Do not skip knowledge load just because the ticket looks familiar.
- Do not leak raw secrets into ticket knowledge or ticket self-work metadata.
- Do not write internal storage or tool mechanics into remote ticket notes.
- Do not call a markdown file or workspace artifact "knowledge". If it is not in CTOX runtime knowledge, it is not durable ticket knowledge.
