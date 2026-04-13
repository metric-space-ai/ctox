---
name: ticket-knowledge
description: Use when Codex is handling ticket work and must load or inspect the SQLite-backed ticket knowledge plane before any operational action.
metadata:
  short-description: Load and inspect ticket knowledge before ticket handling
---

# Ticket Knowledge

Use this skill before ticket classification, dry run, execution, or writeback whenever ticket understanding depends on the CTOX knowledge plane.

## Core Rule

No ticket should be handled operationally without loading the relevant ticket knowledge first.

The ticket system is only the communication surface. SQLite in CTOX is the source of truth.

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

### Load the reference slice for a concrete ticket

```sh
ctox ticket knowledge-load --ticket-key "<ticket_key>" [--domains "source_profile,label_catalog,glossary,service_catalog,infrastructure_assets,team_model,access_model,monitoring_landscape"]
```

### Inspect CTOX self-work around source understanding

```sh
ctox ticket self-work-list [--system "<system>"] [--state "<state>"] [--limit "<n>"]
```

## Operating Pattern

1. Refresh or inspect the source-specific knowledge plane.
2. Load the ticket-specific knowledge slice.
3. If access or secret context is missing, stop operational handling and inspect the secret store or create an explicit access request through the onboarding or access skill.
4. If monitoring context is missing for infra/process questions, ingest or request monitoring evidence instead of guessing.
5. If domains are missing, stop operational handling, inspect existing self-work, and if needed create or continue a justified onboarding or maintenance work item instead of proceeding blindly.
6. If you continue an existing self-work item, assign it to CTOX if needed and leave a plain internal note about what knowledge gap you are resolving.
7. Only continue into dry run or execution once the knowledge load is ready.

## Important Boundaries

- Do not treat remote ticket fields as durable truth when SQLite knowledge already contradicts them.
- Do not hide knowledge gaps in prose; surface them explicitly through the ticket knowledge commands or self-work items.
- Do not skip knowledge load just because the ticket looks familiar.
- Do not leak raw secrets into ticket knowledge or ticket self-work metadata.
- Do not write internal storage or tool mechanics into remote ticket notes.
