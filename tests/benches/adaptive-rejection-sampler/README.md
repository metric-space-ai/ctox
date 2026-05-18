# Bench: `adaptive-rejection-sampler`

Terminal-Bench-2 task, difficulty `medium`, category `scientific-computing`.
Requires implementing an Adaptive Rejection Sampler in R with log-concavity
checking, modular design, and formal tests.

- Image: `alexgshaw/adaptive-rejection-sampler:20251031`
- Task timeout: `900s` (Harbor `[agent] timeout_sec`)
- Verifier: 9 tests (function exists, generates samples, formal tests,
  modularity, input validation, log-concavity, etc.)

## Observations across runs

### Run: `ars-gpt-5-4-mini-1776243943`
- **Status**: FAIL — integration error
- Tokens: `in=666  out=0`  → model never produced output
- Error (from `trajectory.notes`):
  ```
  mission failed on turn 1: stream disconnected before completion:
  error sending request for url (https://api.openai.com/v1/responses)
  ```
- Same error seen on earlier runs (`v3-q`, `v4-q`, `v3-m`, `v4-m`, `g-q`,
  `g-m`). Reproducible across gpt-5.4, gpt-5.4-mini, gpt-5.4-nano.
- The 666-token input is the exact same request body each time — this is
  NOT a random network glitch.

### Run: `ars-gpt-5-4-nano-1776243943`
- **Status**: FAIL — identical to mini: `in=666 out=0`, stream-disconnect
  against `https://api.openai.com/v1/responses`.

### Run: `ars-minimax-m2-7-1776243943`
- **Status**: FAIL — Harbor-side timeout
- `AgentTimeoutError: Agent execution timed out after 900.0 seconds`
- Ran for ~15 min inside the mid-work continuation loop (new in commit
  `f062a63`) — mission was still going when Harbor killed the container.
- No `trajectory.json` was written because ctox was killed mid-execution
  before ATIF export.

## Classification (per user's bench acceptance criteria)

Three failure modes observed, all need treatment:

1. **OpenAI path stream-disconnect** *(gpt-5.4 / mini / nano)* —
   reproducible, task-content-specific, 0 output. This is a transport-
   layer failure where CTOX-to-OpenAI HTTP send fails consistently for
   this one brief. Since it's deterministic, it's either:
   - (a) CTOX sends a request OpenAI refuses at TLS/HTTP layer
   - (b) Something specific to the rendered request body that codex-exec
     sends upstream
   Needs investigation before calling it a CTOX-vs-OpenAI question.

2. **M2.7 hit 900s Harbor agent_timeout** — CTOX orchestration worked
   (mid-work continuation kept the mission alive through multiple turns),
   but 900s was insufficient. Harbor has `agent_timeout_multiplier` to
   raise this; bench runs should supply `--agent-timeout-multiplier 3`
   or similar for reasoning-first models.

3. **No trajectory written when Harbor kills CTOX** — because ATIF export
   is after the turn loop, a Harbor-side timeout kill drops all diagnostic
   output. Should flush a partial trajectory on signal handler. (Future
   hardening.)

## CTOX fixes applied on this path (in upstream commits, not task-local)

- **`f062a63` `run-once: don't close mission on is_open=false alone —
  check reply completion`** — historical commit on the removed legacy
  `run-once` CLI path; added the mid-work heuristic
  (`reply_looks_mid_work`) + auto-continuation (`enqueue_midwork_continuation`),
  plus source-accurate rewrite of that legacy single-mission termination
  logic. Validated in
  `src/context/lcm.rs::mission_is_open` (line 4178): `is_open` defaults to
  `false` for fresh missions because the focus-continuity template is
  empty. Using `is_open == false` as mission-completion signal was a
  CTOX orchestration bug; now only explicit closure signals
  (status=done, mode=closed/dormant) or an explicitly-empty queue are
  used to terminate, and intent-only replies auto-queue a continuation.
- M2.7 mid-work symptoms (ended at `</think>` with "I'll do X", "Let me
  Y:") confirmed this was the cause of previous 0-score runs.

## Open investigations for next iteration

- **Stream-disconnect on OpenAI** needs one of:
  - A tcpdump / request-body dump of what codex-exec sends for this task
  - Retry with a curl from the VPS using the same body to confirm it's
    deterministic
  - Check if the brief contains characters that trip the HTTP client
- **Raise Harbor agent timeout for M2.7 runs** to >20 min so the
  mid-work loop has room.
- **Signal-safe partial trajectory dump** so Harbor-killed runs leave
  diagnostic output.

## Status

| Model | Reward | Error type | Root cause |
|---|---|---|---|
| gpt-5.4 | 0.0 | OpenAI stream-disconnect | task-specific, not yet understood |
| gpt-5.4-mini | 0.0 | OpenAI stream-disconnect | same |
| gpt-5.4-nano | 0.0 | OpenAI stream-disconnect | same |
| MiniMax-M2.7 | 0.0 | Harbor 900s timeout | mid-work loop running, out of wall-time |

No CTOX crash or hang. No premature mission closure (fixed). One
orchestration gap remains (Harbor-side timeout vs CTOX's multi-turn
loop) and one upstream-HTTP mystery (stream-disconnect on OpenAI for
this specific brief).
