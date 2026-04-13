---
name: office365-identity-support-main
description: Use when CTOX should handle simulated Office365 identity tickets by retrieving one labeled runbook item, executing the bounded identity action against the local simulation state, and writing the result back into the ticket flow.
metadata:
  short-description: Main skill for simulated Office365 identity support
---

# Office365 Identity Support Main

This is a thin executable main skill for the simulated Office365 identity desk.

It is meant to prove the full loop:

1. retrieve one runbook item
2. execute one bounded action against a simulated target system
3. verify the result
4. write a ticket-safe update

## Load Order

1. Load [references/generated/main_skill.json](references/generated/main_skill.json).
2. Retrieve the best matching item from [references/generated/runbook_items.jsonl](references/generated/runbook_items.jsonl).
3. Load the shared behavior contract from [references/generated/skillbook.json](references/generated/skillbook.json).
4. Load the runbook metadata from [references/generated/runbook.json](references/generated/runbook.json).
5. Use [scripts/resolve_runbook_item.py](scripts/resolve_runbook_item.py) for deterministic item selection.
6. Use [scripts/simulate_office365_identity_action.py](scripts/simulate_office365_identity_action.py) for bounded execution.
7. Use [scripts/compose_ticket_update.py](scripts/compose_ticket_update.py) to turn execution output into a ticket-safe internal update.
8. Use [scripts/seed_local_office365_tickets.py](scripts/seed_local_office365_tickets.py) to create the corresponding local tickets.

## Execution Rule

The retrieval unit is one labeled runbook item.

Do not infer actions outside the matched label.
Do not mutate the simulation state without running the execution script.

## Working Pattern

1. Read the inbound ticket body.
2. Resolve the best matching runbook item.
3. Extract the target user or mailbox from the ticket text.
4. Run the matching bounded Office365 action.
5. Verify the returned execution result.
6. Write the internal ticket update.

## Output

For this testbed the execution target is:

- one deterministic state change in the Office365 simulation
- one internal ticket update with the action and result

The simulated system state lives in [references/generated/office365_directory.json](references/generated/office365_directory.json).
