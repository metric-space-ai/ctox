---
name: skill-updater
description: Update or modify already installed Codex skills under $CODEX_HOME/skills or $CODEX_HOME/skills/.system. Use when a user wants to change an installed skill, refresh its metadata, validate it, or make a safety backup before editing.
metadata:
  short-description: Modify installed skills and refresh their metadata
cluster: skill_meta
---

# Skill Updater

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Updated skill files are not durable mission knowledge by themselves. The lasting operational state must still be reflected in the CTOX runtime store, bindings, knowledge records, or verification records.

Use this skill when the task is to change an already installed skill rather than creating a new one.

## Scope

- Installed user skills usually live in `$CODEX_HOME/skills/<skill-name>`.
- CTOX system skills are managed in the core SQLite skill store and must not be edited through `$CODEX_HOME/skills/.system`.
- Prefer editing user skills as files and system skills through CTOX CLI/store migration paths.

## Workflow

1. Resolve the installed skill directory first.
2. Create a timestamped backup before making non-trivial changes.
3. For user skills, edit `SKILL.md` and only the resource folders that are actually needed.
4. For system skills, use `ctox skills system show`, `ctox skills system diff`, and `ctox skills system migrate`; do not patch a materialized `.system` copy.
5. Validate the skill structure after the edit.
6. Regenerate `agents/openai.yaml` for file-backed user skills if the UI metadata is stale or should change.

## Commands

- `ctox skills user path`
- `ctox skills user list`
- `ctox skills user create --name <name> --description <text> --body <text>`
- `ctox skills user update --name <name> --description <text> --body <text>`
- `ctox skills system list`
- `ctox skills system show <name> --body`
- `ctox skills system diff`
- `ctox skills system migrate`

## Notes

- Keep installed skill names stable unless the user explicitly wants a rename.
- Preserve bundled resources that are still referenced by the skill.
- If the user wants the source-of-truth in a repo updated too, patch both the installed copy and the source copy or explain the divergence clearly.
