# CTOX Pi Coding Sidecar — Porting Contract

This package is the CTOX port of the Pi coding-agent core for use as a
**managed sidecar** driven by the native Rust owner. It must stay mechanically
traceable to Pi `v0.80.2` (`0201806adfa825ab3d7957a4267d46e5030fd357`) and to
the reference port in `MRP-learn-buddy/packages/agent-runtime`.

## Turn contract

The sidecar exposes a single primitive, modelled on the reference
`runVercelPiCodingAgentTurn`:

- Input: `{ prompt, env, streamFn, systemPrompt?, messages?, tools?,
  maxAssistantTurns?, model? }`
- Result: `{ messages: PiAgentMessage[], events: PiAgentEvent[], snapshot }`

It runs Pi's real `runAgentLoop` with the upstream coding-agent tool factories
(`read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`). It is a turn/session
primitive, **not** a terminal UI. `pi-tui` is not used.

## CTOX deltas from the Vercel reference

1. **ExecutionEnv = Business OS app source (not virtualfs).** The `env` is a
   projection of a module's `business_module_source_files` records into a
   POSIX-like virtual filesystem. Reads come from the synced source records;
   writes accumulate in the env and, on acceptance, become new **P0 commits**
   (`business_module_commits`) via the existing `ctox.source.save` /
   `ctox.source.commit` path. No host filesystem. (Decision 2026-07-18:
   app-source projection, not a git-worktree sandbox.)
2. **Provider = CTOX model gateway.** `streamFn` routes to the internal
   Responses-shaped contract in `src/core/execution/`, never Pi's own provider
   fan-out and never a direct outbound call from the sidecar.
3. **Transport = LocalTransport.** The turn API is served over a Unix socket
   (Windows named pipe); the native Rust owner in `src/core/coding_agents`
   spawns and supervises the sidecar, owns session state, policy, and outcome
   evidence. The sidecar is a bounded leaf executor — it proposes, CTOX decides.
4. **Review-patch model.** Productive writes are diff/commit artifacts first
   (P0 history); applying them to a live module still goes through the
   server-side policy-gated `ctox.source.*` commands.

## Boundaries

- No `pi-tui`, no host shell/process/filesystem, no generic network tools.
- Agent privileges are a strict subset of the daemon: no CTOX state root, no
  secret store / `runtime_env`, no WebRTC mesh. (See the sandbox model in the
  coding-harness delegation notes.)
- All source and commit data flows over RxDB/WebRTC and the typed command
  surface — never an HTTP data bridge (AGENTS.md rule 1).

## Packaging

`npm install` (pinned via `package-lock.json`), then `npm run build` bundles
`src/index.mjs` + the pinned Pi packages into a single self-contained
`dist/ctox-pi-sidecar.mjs` (esbuild@0.28.0, `--bundle`, no `--external`), so the
managed sidecar is one artifact with no runtime package manager. `node_modules`
and `dist/` are build/runtime output and are not committed.

## Status

Vendoring verified (pi packages install cleanly, 239 deps). Remaining:
`src/execution-env.mjs` (app-source projection), `src/turn.mjs` (the turn
adapter over the pinned Pi loop), `src/index.mjs` (LocalTransport server), the
esbuild bundle, and the native Rust owner wiring.
