# CTOX Web Research Benchmark Handoff

Updated: 2026-06-08T19:22:29Z

This handoff captures the current state so another agent can continue without redoing the same forensic work or introducing harness regressions.

## Objective

Benchmark CTOX Web Stack with MiniMax-M3 against the Antigravity/Gemini baseline on the DeepSearchBenchmark task set.

Primary requirement: score actual research quality, not process shape. JSON formatting, queue completion, handled/failed status, and elapsed time are diagnostics only. The benchmark score must come from content review: required-hop correctness, final answer correctness, evidence/source support, completeness, and honesty about partial failures.

## Important User Constraints

- Use CTOX through the official CTOX path, especially `ctox chat` for benchmark tasks.
- Use the official managed update/install path (`ctox upgrade --dev` / `ctox update apply`) rather than ad hoc binaries.
- Do not create multiple competing active CTOX installations. The active CLI should resolve through `/Users/michaelwelsch/.local/lib/ctox/current`.
- MiniMax-M3 is not a local model. It must be treated as a cloud/API runtime.
- Do not patch the CTOX harness state machine, queue, inbound/outbound, or review flow unless a real architectural bug is proven.
- Do not add prompt-ish deterministic shortcuts or heuristic hacks to the harness. Skills should only describe tools clearly.
- Do not treat model/tool-use failures as harness failures unless the code path proves that the harness is at fault.
- Do not put secrets into files, logs, commits, or benchmark artifacts.

## Current Running Operation

At the timestamp above, the corrected managed CTOX install was still running.

Command in progress:

```bash
ctox update apply \
  --source /tmp/ctox-branch-main-minimax-history-release \
  --release local-main-minimax-m3-full-history-20260608T1850Z
```

Observed processes at 2026-06-08T19:22:29Z:

```text
85592  ctox-real update apply --source /tmp/ctox-branch-main-minimax-history-release --release local-main-minimax-m3-full-history-20260608T1850Z
85781  install.sh --rebuild /Users/michaelwelsch/.local/lib/ctox/releases/local-main-minimax-m3-full-history-20260608T1850Z
```

The build is compiling Rust release dependencies under:

```text
/Users/michaelwelsch/.local/lib/ctox/releases/local-main-minimax-m3-full-history-20260608T1850Z/runtime/build/cargo-target
```

Poll with:

```bash
ps -ax -o pid,ppid,etime,%cpu,%mem,command | rg 'ctox-real update apply|cargo build --release --bin ctox|rustc .*ctox|local-main-minimax-m3-full-history'
```

If the original tool session is still available in Codex, it was session `74080`.

## Active CTOX Symlink

At 2026-06-08T19:22:29Z, the active symlink still pointed to the earlier wrong release:

```text
/Users/michaelwelsch/.local/lib/ctox/current -> /Users/michaelwelsch/.local/lib/ctox/releases/local-minimax-full-history-20260608T000000Z
```

That earlier release is wrong for the benchmark because it was built from an older local `HEAD` and lost the newer MiniMax-M3 model registry/runtime support. It caused:

```text
unsupported runtime planner model: MiniMax-M3
```

Do not use that old active release for the benchmark.

The corrected release being built is:

```text
/Users/michaelwelsch/.local/lib/ctox/releases/local-main-minimax-m3-full-history-20260608T1850Z
```

It was created from the previous functional main release `branch-main-20260608T172856Z` plus only the MiniMax full-history harness-client patch.

After the update finishes, verify the symlink points to the corrected release:

```bash
readlink /Users/michaelwelsch/.local/lib/ctox/current
```

## Root Cause Found

Direct MiniMax Responses API testing showed:

- MiniMax `/v1/responses` accepts full-history requests.
- MiniMax fails when CTOX uses OpenAI-style incremental `previous_response_id` plus only `function_call_output`.
- The failure was:

```text
400 invalid params, tool result's tool id(call_function_...) not found (2013)
```

Therefore the CTOX compatibility bug was in the Responses API request preparation for MiniMax: CTOX was sending an incremental delta using `previous_response_id`, but MiniMax requires the full tool-call history to resolve tool outputs.

This is not a chat-completions issue. Do not introduce Chat Completions here.

## CTOX Patch State

Local CTOX repo:

```text
/Users/michaelwelsch/Documents/ctox
```

Dirty files currently observed:

```text
 M docs/site/index.html
 M src/core/execution/models/runtime_state.rs
 M src/core/harness/core/src/client.rs
 M src/core/harness/core/src/client_tests.rs
 M src/core/iot/widget_runtime.rs
 M src/core/knowledge/data.rs
```

Only these two files are the MiniMax harness-client fix from this work:

```text
src/core/harness/core/src/client.rs
src/core/harness/core/src/client_tests.rs
```

Other dirty files appear to be unrelated/pre-existing/other-agent work. Do not revert them unless the user explicitly asks.

Patch behavior:

- `prepare_http_request` returns the full `ResponsesApiRequest` for MiniMax Responses providers.
- `prepare_websocket_request` returns the full response-create payload for MiniMax Responses providers.
- The provider check is intentionally narrow:
  - `wire_api == WireApi::Responses`
  - `base_url` contains `api.minimax.io` or `api.minimaxi.com`

No queue, state-machine, inbound, outbound, review, or skill flow was changed for this fix.

Local tests already passed:

```bash
cd /Users/michaelwelsch/Documents/ctox/src/core/harness/core
cargo test minimax_responses
cargo test http_request_uses_previous_response_id_for_incremental_delta
```

Expected result:

```text
2 passed for minimax_responses
1 passed for incremental OpenAI-style previous_response_id behavior
```

The second test matters: the patch should not regress providers that support incremental `previous_response_id`.

## Correct Source Used For Managed Update

Do not rebuild from local `HEAD` for this benchmark fix. That was already tried and produced a release without MiniMax-M3 support.

Correct temp source:

```text
/tmp/ctox-branch-main-minimax-history-release
```

It is based on:

```text
/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260608T172856Z
```

Then only the harness client/test patch was applied.

Before starting the current update, this source was verified to include both:

```text
MiniMax-M3 runtime/model registry support
provider_requires_full_responses_history in src/core/harness/core/src/client.rs
```

## After The Corrected CTOX Update Finishes

Run:

```bash
ctox start
ctox runtime switch MiniMax-M3 quality --context 256k --timeout 1800
ctox version
ctox status
```

Expected runtime properties:

- Active model: `MiniMax-M3`
- Provider/upstream: MiniMax API, e.g. `https://api.minimax.io`
- Preset: `quality`
- Context: `256k` / `262144`
- Max run: `1800`

If `ctox runtime switch MiniMax-M3 ...` still fails, inspect the active release:

```bash
rg -n 'MiniMax-M3|provider_requires_full_responses_history|api.minimax' \
  /Users/michaelwelsch/.local/lib/ctox/current/src/core
```

If the active release does not contain both MiniMax-M3 support and the full-history patch, the managed update did not switch to the corrected release.

## Benchmark Project

Dashboard/repo:

```text
/Users/michaelwelsch/Documents/deepSearchBenchmark
```

Current git status:

```text
 M README.md
 M data/current-benchmark.json
 M docs/methodology.md
 M index.html
?? data/reviews.example.json
?? scripts/
```

Existing commits:

```text
42a2e53 Initial DeepSearchBenchmark static site
14e7708 Generalize benchmark dashboard for multiple harnesses
```

Important scoring files:

```text
/Users/michaelwelsch/Documents/deepSearchBenchmark/scripts/apply_review_scores.py
/Users/michaelwelsch/Documents/deepSearchBenchmark/data/current-benchmark.json
/Users/michaelwelsch/Documents/deepSearchBenchmark/docs/methodology.md
/Users/michaelwelsch/Documents/deepSearchBenchmark/scripts/build_site.py
```

Current score model in `scripts/apply_review_scores.py`:

```text
hop_score             0.40
final_answer_score    0.30
evidence_score        0.20
completeness_score    0.10
hallucinated_completion penalty: x0.65
```

This is the intended direction: score research content, not JSON transport.

Important: existing dataset still has no complete task-level review records. Therefore the current page must not be represented as final research-quality leaderboard data.

Current dashboard semantics:

- `research_quality_score`: primary benchmark score, only after review records exist.
- `artifact_contract_score` / `output_contract_rate`: JSON/format diagnostic only.
- `completion_rate`: operational diagnostic only.
- `run_valid=false`: invalid runtime/config run, unranked.
- `unscored`: valid/complete enough to inspect but no content review yet.

## Existing Benchmark Artifacts

Existing AGY baseline artifact:

```text
/tmp/web-research-bench/antigravity-gemini-baseline-50-clean-20260608T044450Z/combined_agy_baseline_summary.json
```

Telemetry from that baseline:

- 50/50 commands completed.
- 23/50 strict JSON.
- 25/50 recoverable.
- 2/50 invalid.

This is output-contract telemetry only. It is not the research-quality score.

Old CTOX runs are not final benchmark results:

```text
/tmp/web-research-bench/rerun-ctox-chat-20260608T174555Z
/tmp/web-research-bench/rerun-ctox-chat-harness-20260608T174950Z
/tmp/web-research-bench/rerun-ctox-chat-harness-fixed-*
```

Known invalidation causes:

- non-harness thread key routed into strategic direction setup
- invalid `service_tier=default` config
- MiniMax Responses incremental tool-output failure (`tool id not found (2013)`)

## CTOX Config Fix Already Applied

The bad config was:

```toml
service_tier = "default"
```

It was changed to:

```toml
service_tier = "flex"
```

Config file:

```text
/Users/michaelwelsch/.codex/config.toml
```

Do not reintroduce `service_tier = "default"`.

## Correct CTOX Benchmark Submission Plan

Use official `ctox chat`.

Use harness/internal thread keys to avoid owner-strategy routing:

```text
codex/harness/deep-search-benchmark/ctox-minimax-m3/{run_id}/{task_id}
```

Avoid old `bench/...` thread keys. They can be treated as owner-visible inbound work and can route into business setup/strategic direction logic when the business profile is incomplete.

Because previous batch submission created queue pressure and DB-lock symptoms, prefer sequential runs with wait:

```bash
ctox chat \
  --thread-key "codex/harness/deep-search-benchmark/ctox-minimax-m3/{run_id}/{task_id}" \
  --workspace /Users/michaelwelsch/Documents/ctox \
  --wait \
  --timeout-secs 1800 \
  --atif-out "{artifact_path}" \
  "{title}

{prompt}"
```

If the CLI option names differ in the active build, check:

```bash
ctox chat --help
```

Do not replace this with smoke prompts. Use the real benchmark task prompts.

Likely prompt source from the AGY baseline:

```text
/tmp/web-research-bench/antigravity-gemini-baseline-50-clean-20260608T044450Z/prompts
```

## AGY Baseline

AGY binary:

```text
/Users/michaelwelsch/.local/bin/agy
```

The existing AGY baseline exists, but the user also asked to rerun the 50-task baseline. There is a helper script in the benchmark repo:

```text
/Users/michaelwelsch/Documents/deepSearchBenchmark/scripts/run_agy.py
```

Earlier AGY rerun attempt exited immediately and was not fully diagnosed. Do not claim a fresh rerun succeeded unless artifacts prove it.

## What Is Still Open

1. Wait for the corrected `ctox update apply` to finish.
2. Verify `/Users/michaelwelsch/.local/lib/ctox/current` points to `local-main-minimax-m3-full-history-20260608T1850Z`.
3. Start CTOX and switch runtime to MiniMax-M3 quality, 256k context, 1800 seconds.
4. Run one real CTOX benchmark task through `ctox chat --wait` to prove MiniMax tool continuation no longer fails.
5. Run the full 50-task CTOX benchmark through official `ctox chat`, preferably sequentially for quality/stability.
6. Rerun or validate the AGY 50-task baseline.
7. Perform task-level content review for each harness result.
8. Apply review scores using `scripts/apply_review_scores.py`.
9. Rebuild dashboard using `scripts/build_site.py`.
10. Verify the standalone dashboard visually and functionally.

## What Not To Claim Yet

Do not claim final CTOX-vs-AGY research-quality scores yet.

Current facts support only:

- AGY has an existing 50-command operational baseline.
- CTOX old run is invalid/unranked due to runtime/config/API compatibility failures.
- Direct MiniMax API forensic work identified and constrained the CTOX client compatibility bug.
- The corrected CTOX managed release is still building/installing as of this handoff timestamp.
- The dashboard scoring model now separates content quality from JSON/process diagnostics.

## Minimal Next Command Sequence

After confirming the build finished:

```bash
readlink /Users/michaelwelsch/.local/lib/ctox/current
ctox start
ctox runtime switch MiniMax-M3 quality --context 256k --timeout 1800
ctox status
ctox chat --help
```

Then run a single real task:

```bash
RUN_ID="ctox-minimax-m3-$(date -u +%Y%m%dT%H%M%SZ)"
TASK_ID="t01_fritz_kola_register_bundesanzeiger"
PROMPT_FILE="/tmp/web-research-bench/antigravity-gemini-baseline-50-clean-20260608T044450Z/prompts/${TASK_ID}.txt"
OUT_DIR="/tmp/web-research-bench/${RUN_ID}/ctox"
mkdir -p "$OUT_DIR"

ctox chat \
  --thread-key "codex/harness/deep-search-benchmark/ctox-minimax-m3/${RUN_ID}/${TASK_ID}" \
  --workspace /Users/michaelwelsch/Documents/ctox \
  --wait \
  --timeout-secs 1800 \
  --atif-out "${OUT_DIR}/${TASK_ID}.atif.json" \
  "$(cat "$PROMPT_FILE")"
```

If the prompt file naming differs, list the prompt directory first:

```bash
find /tmp/web-research-bench/antigravity-gemini-baseline-50-clean-20260608T044450Z/prompts -maxdepth 1 -type f | sort | head
```

## Scoring Reminder

A formally invalid JSON artifact can still contain strong research and should be content-reviewed.

A perfectly valid JSON artifact can be wrong and should receive a low research-quality score.

A CTOX queue task marked `handled` is not automatically correct.

A CTOX queue task marked `failed` because of runtime/config invalidation should not be counted as bad research quality. It is an invalid run unless the artifact contains enough content to review and the failure is not caused by infrastructure.

The final benchmark must distinguish:

- research quality
- output contract
- runtime health
- latency
- cost
- failure mode

