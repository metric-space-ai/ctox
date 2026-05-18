---
name: privilege-escalation
description: Use when CTOX must perform a local privileged action through a visible sudo path backed by a local secret reference. This skill exists so root-level work is explicit, inspectable, and bounded instead of silently failing on interactive sudo prompts.
cluster: security_access
---

# Privilege Escalation

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


For CTOX mission work, only CTOX secret and access state, ticket state, communication records, and verification records count as durable access knowledge. Workspace notes or copied commands do not count as durable knowledge by themselves.

Use this skill when a local task requires `sudo` or another privileged host action.

Do not use it for the whole deployment by itself. Pair it with `service-deployment`, `change_lifecycle`, or another concrete execution skill.

## Operating Model

Treat this skill as:

1. privilege requirement check
2. local sudo secret reference lookup
3. visible privileged execution through an inspectable helper

Use explicit CTOX CLI/API paths for privileged execution. Do not execute embedded `scripts/` helpers from this system skill; if a privileged operation lacks an audited CTOX command, leave the task blocked and add that command first.

## Workflow

1. Confirm the task truly needs privilege.
2. Confirm the current requester is actually allowed to authorize sudo for this turn.
   - `owner` may authorize sudo.
   - configured `admin` profiles may authorize sudo only when their mail profile says so.
   - plain domain users may not authorize sudo work by email.
   - secret-bearing approval or credential entry must move to TUI.
3. Prefer non-privileged execution paths first.
4. If privilege is required, check the local secret reference:
   - `runtime/secrets/ctox-sudo.env`
5. Use an audited CTOX command instead of ad-hoc hidden `sudo -S` calls.
6. Persist the blocker if no valid non-interactive sudo path exists or the requester lacks sudo authority.

## Guardrails

- Do not assume `sudo` works just because the user account exists.
- Do not print the sudo password in operator-facing output.
- Do not ask the owner for a sudo password repeatedly if a local secret reference already exists.
- Do not treat a mail sender without sudo authority as sufficient approval for privileged work.
- If the audited command path or secret reference is missing, say that exactly and leave the task `blocked`.

## Resources

- [references/sudo-rules.md](references/sudo-rules.md)
