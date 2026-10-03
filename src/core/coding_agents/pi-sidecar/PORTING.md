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

## Independent coding-plan routes

The inherited `ctox` preset resolves the current main model and provider using
the native runtime kernel and the same API route specification as the main
harness. API routes use a fresh turn-scoped capability
bridge: the native owner injects the selected provider credential, pins the
model, streams the response, and drops the listener at turn completion. The
sidecar receives neither the provider key nor its remote URL. This does not
start or depend on the retired persistent `127.0.0.1:12434` gateway.

OpenAI, CTOX proxy and Azure use Pi's Responses adapter. Direct MiniMax and
OpenRouter use Pi's existing Chat Completions adapter and the configured
upstream's `/chat/completions` endpoint. The bridge preserves the selected
wire protocol and streams bytes; it does not translate SSE or infer the
provider from a model name. Other main protocols fail explicitly before sidecar startup;
they require their own supported adapter or an independent server-authored
preset. An upstream model name alone never selects a credential or provider.

`ctox coding-agent route` reports the inherited provider, model, upstream
origin and wire API through the operator CLI. It does not start Pi, call a
provider, read app records or return credentials, endpoint paths or account IDs.

Provider and model selection are independent dimensions. The browser sends an
opaque server-authored `preset_id`; native CTOX resolves the account, model,
fixed endpoint profile and encrypted-store secret handle immediately before a
turn. A model name must never silently reinterpret MiniMax as `ctox_proxy`.

Direct Anthropic-compatible coding plans use a turn-scoped native loopback
bridge. Pi receives only a public sentinel and calls the loopback URL; the Rust
owner injects `x-api-key` and streams the response. The provider secret is
never copied into the sidecar request or process environment. The shared bridge
mechanism supports MiniMax Coding Plan (`https://api.minimax.io/anthropic`) and
can carry Kimi Coding Plan (`https://api.kimi.com/coding/`) through separate
typed account configurations and allow-lists.

The descriptors are cross-checked against the direct Workjet integrations:
Kimi uses `k3[1m]` with a 1,048,576-token context window; MiniMax uses
`MiniMax-M3`. Those public model facts are fixed by the server-side preset and
remain independent of CTOX's main model.

MiniMax capacity is a separate explicit operation against
`https://www.minimax.io/v1/token_plan/remains` using Bearer authentication.
Only labeled general/MiniMax/coding usage windows are accepted; media quotas,
ambiguous counters and malformed used/limit pairs remain unknown.

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

The bounded TypeScript sidecar, self-contained esbuild bundle, native Rust
owner, app-source projection and server-resolved provider presets are wired.
The LocalTransport request, response and turn limits have offline socket tests;
direct Kimi/MiniMax coding-plan routes use a turn-scoped native capability
bridge.

This status is not a provider-integration completion claim. Subscription routes
still require their own turn-bound gateway capability, live provider accounts
still require redacted E2E receipts, and credential refresh/rollback remains a
separate Track-B gate. The authoritative per-provider state is
`src/core/execution/cliproxyapi_integration/provider-integration.json`.
