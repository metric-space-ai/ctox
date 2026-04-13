# Execution Flow

This example main skill is intentionally thin.

## Resolution

1. Normalize the user email into one likely issue.
2. Use `scripts/resolve_runbook_item.py` against `runbook_items.jsonl`.
3. Take the highest-confidence label as the candidate item.
4. Read `skillbook.json` to apply:
   - non-negotiable rules
   - reply mode policy
   - answer contract
   - escalation boundaries
5. Use `scripts/compose_support_reply.py` to build the actual email output.
6. Use `scripts/handle_support_email.py` when the whole flow should run as one deterministic step.

## Execution

The concrete execution in this example is:

- write a reply suggestion
- or write a reply draft
- or return `needs_review`

## Important Boundary

The main skill is an orchestrator.
The runbook item is the concrete problem unit.
The skillbook is the behavior and policy frame.
