# LLM-generated realistic ATS synthetic data (Antigravity / Gemini)

The LLM AUTHORS rich, realistic German recruiting records; a deterministic
injector lands them gate-consistently into the live ATS. This is the realism path
(real CVs, varied names, job descriptions, interview scorecards) — distinct from
the templated `ats_synthetic_generate.sh`.

## Pipeline
1. `genprompt.txt` — the per-batch generation prompt (schema + realism + gate rules).
2. `agy --model "Gemini 3.5 Flash (Medium)" --print "<prompt>" < /dev/null` →
   one rich JSON batch (~15 candidates, ~33 KB). **The `< /dev/null` is load-bearing:**
   `agy --print` reads stdin and HANGS on missing EOF; feeding /dev/null fixes it.
3. `ats_inject.py batch.json` (run on the instance) — seeds stammdaten + dispatches
   the gated flow (submission/placement/signoff), namespacing all ids by batch.
4. `scale.py <n_batches> <concurrency>` — orchestrates parallel generation + injection.

## Verified
End-to-end on the ninja test instance: rich varied data (99/105 distinct names,
full CVs/scorecards/job posts), both gates firing, 0 consistency violations,
invoices produced.

## Known limit
Antigravity rate-limits after ~15–20 generations in a window: agy then returns
empty (exit 0, 0 bytes, all models) and an app restart does NOT reset it — it is a
server-side quota. Reaching thousands needs the quota to allow it (wait for reset
and resume, or a higher Antigravity tier). `scale.py` should be re-run to resume;
make it retry empty batches and throttle once recovered.
