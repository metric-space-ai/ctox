# Benchmark Protocol — qwen36_35b_a3b_q4km_metal

This is the project-local benchmark protocol for the Qwen3.6-35B-A3B
Q4_K_M Metal port. It is a thin specialization of the canonical
protocol in
[skills/.../qwen35-benchmark-protocol.md](../../../../../skills/system/model_optimization/local-llm-inference-optimization/references/qwen35-benchmark-protocol.md);
read that one first if you have not.

## What's project-specific here

- **Reference baseline = the existing `qwen36_35b_a3b_ggml/` shim.**
  Same M5, same Q4_K_M GGUF, same Responses-IPC contract, same
  prompt/decode workload. Numbers are captured by issuing the
  benchmark prompt-suite over the shim's Unix socket and timing
  end-to-end response latency per phase. This is the "ggml is only a
  transition; never a runtime dependency" rule applied in measurement
  form — the shim exists *as the baseline*, not as a target.

  Concretely:

  ```text
  baseline_t_prefill[N]  = median latency for the shim to produce the
                           first token of an N-token prompt
  baseline_t_decode[K]   = median latency for the shim to extend by K
                           tokens once prefill is done
  ours_t_prefill[N]      = same metric, this crate's server
  ours_t_decode[K]       = same metric, this crate's server
  speedup[phase, size]   = baseline_t / ours_t
  ```

  The optimization is "working" only when `speedup ≥ 1.0` everywhere
  on a stable thermal pack, with correctness gates green.

- **Optional cross-check, not required.** If a discrepancy with the
  shim is suspicious, a separate `llama-bench` run against the same
  GGUF can be used to triangulate, but it is not part of the
  acceptance pack — the shim already wraps `llama-server` end-to-end,
  so its numbers are equivalent up to harness IPC overhead, which is
  exactly the overhead this crate also pays.

- **Stage gating.** Because end-to-end inference doesn't run yet, the
  acceptance pack (see canonical §"Measurement Packs") cannot be used
  in stages 1–3. In those stages, only:

  - **smoke**: per-op verifier passes against the f32 CPU reference.
    Acceptance: never.
  - **candidate**: isolated kernel benchmark on a single op, fixed
    shape derived from the frozen ABI in [MODEL_SHAPE.md](MODEL_SHAPE.md).
    Acceptance: only with the matching forensics record.

  Once stage 4 lands the linear-attention port, the **acceptance** pack
  with prompt sizes `512, 4096, 16384` becomes available for
  promotion.

- **Hardware roofline location.** Roofline numbers for this engine go
  into `tools/roofline_baseline_<date>.env` written by the (stage-2)
  Metal probe extension; the canonical protocol's
  `tools/capture_roofline_baseline.sh` path doesn't apply since this
  crate owns its own probe binary
  ([src/bin/probe_m5_metal.rs](../../src/bin/probe_m5_metal.rs)).

- **Modelpack vs. metalpack naming.** This crate uses **modelpack**
  for the deterministic packed-weight artifact (file extension
  `.modelpack`) — the metalpack name in the qwen35-0.8B work referred
  to a Metal-specific layout encoding which collides with the broader
  notion of "the packed model file". Stage 2 will add the packer.

## Hardware/runtime conditions to capture every run

- chip / GPU core count / unified memory size from
  `qwen36-35b-a3b-q4km-metal-probe` (commit the JSON next to the
  numbers)
- `xcrun --show-sdk-version`
- `system_profiler SPDisplaysDataType | grep "Metal Support"`
- macOS build (`sw_vers -buildVersion`)
- Power Adapter status (Power Source = AC vs Battery — thermal
  variance is large)
- ambient room temperature (rough; ±2 °C is fine)
- whether any GUI app is using the GPU during the run (if yes, redo)

## Anti-patterns the qwen35 work documented; do not repeat

(quoted from qwen35-lessons.md "Dead Ends" — the same traps apply at
35B-A3B, scaled up):

- four-token decode runs are smoke only — never promote on them
- "no cache misses" is not a real performance claim without either
  hardware counters or a labeled byte model
- block32 vs block64 scan variants too close to call → look for a
  structural change, not a tile knob
- SIMD is not magic — clean reductions that lose are a real signal
- scalar int8 dequant: lost to unpack overhead at 0.8B, will lose
  louder at 35B; if a quantized matmul candidate doesn't beat block-Q
  Metal kernels in *isolated* bench *and* full-path bench, reject

## Stage-1 status

No project benchmark numbers exist yet. The first numbers will be
upstream `llama-bench` against `Qwen3.6-35B-A3B-Q4_K_M.gguf` once that
artifact is in hand and `vendor/llama-cpp.version` is pinned. Until
then, every claim about "performance" in this crate is a hypothesis,
not an evidence-backed result.
