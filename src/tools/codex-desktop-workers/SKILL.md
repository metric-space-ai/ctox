---
name: proxy-model-workers
description: Delegate implementation packages to parallel Codex Desktop workers using Grok, GLM, Kimi or OpenAI, with per-task providers and a PR lifecycle.
---

# Codex Desktop workers

The main task analyzes and dispatches independent packages in parallel.
A short assignment states the problem, outcome, boundaries and success criteria.
The worker chooses the implementation. The main task consolidates corrections,
reviews the work and archives the worker after merging its PR.
Shared rules are in GLOBAL-INSTRUCTIONS.md; do not attach them to every assignment.

For technical operations, read only the relevant section of [PROTOCOL.md](PROTOCOL.md):

- Model selection and authentication: **Model routing**. Grok, GLM and Kimi use
  `cli_proxy` with `high` reasoning; OpenAI retains its direct connection.
  Experience: `~/.codex/proxy-workers/MODEL-EXPERIENCE.md`.
  Availability: `scripts/worker.py availability`; defer quota blocks separately.
- Creating a worker: **Prepare and dispatch**. The helper sets provider, project,
  title and 256k context without an initialization turn. Then send the real assignment.
- Corrections or compaction: **Context continuity and publication**.
- Binding a PR, recording review and archiving: **PR and merge lifecycle**.

Worktrees and temporary data belong on `/Volumes/tmp`; the shared resource gate
remains mandatory. Technical references and private coordination do not belong
in public issues or PRs.

[ACCEPTANCE.md](ACCEPTANCE.md) tests the worker integration; it is not a mandatory
checklist for every implementation assignment.
