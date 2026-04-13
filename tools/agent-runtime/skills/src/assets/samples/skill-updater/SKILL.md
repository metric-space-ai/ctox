---
name: skill-updater
description: Update or modify already installed Codex skills under $CODEX_HOME/skills or $CODEX_HOME/skills/.system. Use when a user wants to change an installed skill, refresh its metadata, validate it, or make a safety backup before editing.
metadata:
  short-description: Modify installed skills and refresh their metadata
---

# Skill Updater

Use this skill when the task is to change an already installed skill rather than creating a new one.

## Scope

- Installed user skills usually live in `$CODEX_HOME/skills/<skill-name>`.
- Installed bundled system skills usually live in `$CODEX_HOME/skills/.system/<skill-name>`.
- Prefer editing the installed copy that Codex actually loads, not a random source checkout.

## Workflow

1. Resolve the installed skill directory first.
2. Create a timestamped backup before making non-trivial changes.
3. Edit `SKILL.md` and only the resource folders that are actually needed.
4. Validate the skill structure after the edit.
5. Regenerate `agents/openai.yaml` if the UI metadata is stale or should change.

## Scripts

- `scripts/backup_skill.py <skill_dir>`
- `scripts/quick_validate.py <skill_dir>`
- `scripts/generate_openai_yaml.py <skill_dir> --interface key=value`
- `scripts/refresh_skill_metadata.py <skill_dir> [--interface key=value]`

## Notes

- Keep installed skill names stable unless the user explicitly wants a rename.
- Preserve bundled resources that are still referenced by the skill.
- If the user wants the source-of-truth in a repo updated too, patch both the installed copy and the source copy or explain the divergence clearly.
