# ctox-rxdb-js — Claude guardrails

Identical rules to `AGENTS.md` in this directory — read that file and
`docs/ctox-rxdb.md` before changing anything here.

Summary of the hard rules: WebRTC-only data plane (no HTTP fallback, ever);
never patch `dist/` directly — rebuild from src with the pinned esbuild
command and bump all three `?v=` cache-busters; no npm/bare/`node:` imports;
never hand-edit generated contract files (fixture → generator pipeline);
keep `node src/apps/business-os/rxdb/tests/run-all.mjs` green and never
delete or weaken a failing test to make a change pass.
