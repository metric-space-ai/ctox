---
name: skill-installer
description: Install Codex skills into $CODEX_HOME/skills from a curated list or a GitHub repo path. Use when a user asks to list installable skills, install a curated skill, or install a skill from another repo (including private repos).
metadata:
  short-description: Install curated skills from openai/skills or other repos
cluster: skill_meta
---

# Skill Installer

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Installed skill files are not durable mission knowledge by themselves. For CTOX mission work, the relevant operational understanding must still live in the CTOX runtime store and linked knowledge/binding records.

Helps install skills. By default these are from https://github.com/openai/skills/tree/main/skills/.curated, but users can also provide other locations.

Use CTOX CLI commands for packaged and local skills:

- List built-in packs with `ctox skills packs list`.
- Install a built-in pack with `ctox skills packs install <name>`.
- Inspect user skill roots with `ctox skills user path`.
- Create or update local user skills with `ctox skills user create` and `ctox skills user update`.

Do not install CTOX system skills by copying files. System skills are managed by the CTOX core SQLite store and migration path.

## Communication

When listing skills, output approximately as follows, depending on the context of the user's request. If they ask about experimental skills, list from `.experimental` instead of `.curated` and label the source accordingly:
"""
Skills from {repo}:
1. skill-1
2. skill-2 (already installed)
3. ...
Which ones would you like installed?
"""

After installing a skill, tell the user: "Restart Codex to pick up new skills."

## Commands

- `ctox skills packs list`
- `ctox skills packs install <name>`
- `ctox skills user path`
- `ctox skills user list`
- `ctox skills user create --name <name> --description <text> --body <text>`
- `ctox skills user update --name <name> --description <text> --body <text>`
- `ctox skills system list`
- `ctox skills system diff`
- `ctox skills system migrate`

## Behavior and Options

- Aborts if the destination skill directory already exists.
- Installs into `$CODEX_HOME/skills/<skill-name>` (defaults to `~/.codex/skills`).

## Notes

- Source packs are the file-backed starter skills shipped under CTOX `skills/packs`.
- GitHub skill installation is not a CTOX system-skill update path. Use normal user-skill file workflows for externally sourced skills.
- CTOX system skills are present through `ctox skills system list`; update them with `ctox skills system migrate`.
- Installed annotations come from `$CODEX_HOME/skills`.
