# CTOX — agent instructions

The canonical agent instruction file for this repository is `CLAUDE.md` in the
repo root. Read it fully before working; its operator guardrails (work on
`main`, no env-var runtime toggles, repository hygiene) apply to every agent,
not just Claude.

Subsystem guardrails live next to the code they protect:

- `docs/ctox-rxdb.md` — the Business OS data plane (CTOX DB). **WebRTC-only;
  no HTTP fallbacks; never patch the dist bundle directly; wire contracts are
  generated from fixtures; keep the guard/test suites green.**
- `src/core/rxdb/AGENTS.md`, `src/apps/business-os/rxdb/AGENTS.md`,
  `src/core/business_os/AGENTS.md` — directory-local hard rules for the data
  plane.

If a guard test blocks your change, the guard is right: fix your change, do
not weaken the guard.
