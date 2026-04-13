---
name: eventus-email-support-main
description: Use when CTOX should handle inbound E.VENT.US supplier support emails by retrieving one standardized runbook item, then loading the linked skillbook to draft or suggest the actual email response.
metadata:
  short-description: Main skill for Eventus support email handling
---

# Eventus Email Support Main

This is the main skill for the Eventus email support example.

It stays thin, but it is executable.
It resolves one runbook item and turns it into a concrete email suggestion or draft.

## Load Order

1. Load [references/generated/main_skill.json](references/generated/main_skill.json).
2. Retrieve the best matching item from [references/generated/runbook_items.jsonl](references/generated/runbook_items.jsonl).
3. Load the shared behavior contract from [references/generated/skillbook.json](references/generated/skillbook.json).
4. Load the runbook metadata from [references/generated/runbook.json](references/generated/runbook.json).
5. Use [scripts/resolve_runbook_item.py](scripts/resolve_runbook_item.py) for deterministic item selection.
6. Use [scripts/compose_support_reply.py](scripts/compose_support_reply.py) for deterministic reply assembly.
7. Use [scripts/handle_support_email.py](scripts/handle_support_email.py) as the end-to-end entrypoint when you already have the generated bundle.

## Execution Rule

The retrieval unit is one labeled runbook item.

Do not answer from the whole skillbook alone.
Do not answer from the whole runbook alone.

## Working Pattern

1. Read the inbound email.
2. Rewrite the problem as a likely runbook-item query.
3. Match the best labeled item.
4. Use the linked skillbook to enforce style, runtime policy, and escalation.
5. Produce an email suggestion or draft.
6. Escalate instead of inventing steps when the item does not cleanly cover the case.

## Scripts

- `scripts/resolve_runbook_item.py`
  - turns an inbound email into a best-match labeled runbook item
- `scripts/compose_support_reply.py`
  - turns `main_skill.json + skillbook.json + runbook item + inbound email` into a structured email output
- `scripts/handle_support_email.py`
  - runs resolve plus compose as one end-to-end execution path

## Output

For this example the execution target is an email reply.

Preferred outcome order:

1. suggestion
2. draft
3. send only when the linked skillbook policy allows it
