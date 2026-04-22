---
name: privilege-escalation
description: Use when CTOX must perform a local privileged action through a visible sudo path backed by a local secret reference. This skill exists so root-level work is explicit, inspectable, and bounded instead of silently failing on interactive sudo prompts.
cluster: security_access
---

# Privilege Escalation

For CTOX mission work, only SQLite-backed secret/access state, ticket state, communication records, and verification records count as durable access knowledge. Workspace notes or copied commands do not count as durable knowledge by themselves.

Use this skill when a local task requires `sudo` or another privileged host action.

Do not use it for the whole deployment by itself. Pair it with `service-deployment`, `change_lifecycle`, or another concrete execution skill.

## Operating Model

Treat this skill as:

1. privilege requirement check
2. local sudo secret reference lookup
3. visible privileged execution through an inspectable helper

Preferred helper script under `scripts/`:

- `ctox_sudo.py`

The helper is open and inspectable. Read or patch it when the host shape is unusual.

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
5. Use the helper instead of ad-hoc hidden `sudo -S` calls.
6. Persist the blocker if no valid non-interactive sudo path exists or the requester lacks sudo authority.

## Guardrails

- Do not assume `sudo` works just because the user account exists.
- Do not print the sudo password in operator-facing output.
- Do not ask the owner for a sudo password repeatedly if a local secret reference already exists.
- Do not treat a mail sender without sudo authority as sufficient approval for privileged work.
- If the helper or secret reference is missing, say that exactly and leave the task `blocked`.

## Resources

- [references/sudo-rules.md](references/sudo-rules.md)
- [scripts/ctox_sudo.py](scripts/ctox_sudo.py)
