# Benchmark Protocol

This protocol is for performance runs. Use it whenever a result may influence a
kernel default.

## Rules

```text
run benchmarks serially
do not run subagent benchmarks
do not compare against old numbers if thermal/runtime conditions changed
report median and p95
record env flags
record modelpack path
record token/context length
record warmup and iteration count
keep correctness gates next to performance numbers
```

## Preflight

```text
cargo build --release --bins
cargo test cache_model
```

Capture or refresh the local roofline before making performance claims if the
hardware, OS/runtime, accepted profile, modelpack, thermal condition, or major
layout family changed:

```text
tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline_<date>
```

Use the generated `roofline.env` values in later reports:

```text
sustained_stream_GB_s
operational_prefill_matmul_GB_s
operational_matvec_GB_s
```

Every hot operator report must compare against those limits. A benchmark that
only reports `median_s` without `bandwidth_utilization` and `time_vs_floor`
cannot justify a kernel promotion.

Verify real pack:

```text
/tmp/ctox_qwen35_08b_real_fp16.metalpack
```

If the pack changes, old benchmark results are not comparable until rerun.

## Measurement Packs

Use named packs instead of ad hoc one-off benchmark shapes.

```text
smoke:
  purpose: verify tool/kernel route only
  tokens: 64 or 128
  iterations: 1
  warmup: 0
  acceptance: never

candidate:
  purpose: decide whether a candidate deserves a full sweep
  tokens: 4096
  iterations: 3
  warmup: 1
  acceptance: no, unless repeated with acceptance pack

acceptance:
  purpose: promote a default or reject a serious candidate
  tokens: 512,4096,16384
  iterations: 3 minimum, 7 for close calls
  warmup: 1
  acceptance: yes, if correctness and forensics pass

long-context:
  purpose: validate attention/KV/cache behavior
  tokens: 32768,65536,131072
  iterations: 3 minimum
  warmup: 1
  acceptance: required for long-context claims
```

Use:

```text
tools/run_measurement_pack.sh --dry-run <pack> <metalpack-dir>
tools/run_measurement_pack.sh <pack> <metalpack-dir>
tools/run_measurement_pack.sh --capture <pack> <metalpack-dir>
```

to avoid hand-written command drift.

Capture ad hoc real runs through:

```text
tools/capture_measurement_output.sh --accepted-profile --label <slug> -- \
  tools/run_measurement_pack.sh <pack> <metalpack-dir>
```

The capture wrapper holds an exclusive local lock and writes raw stdout, stderr,
normalized evidence fields, command line, exit code, git state, and accepted
profile hash into one run directory.

## Baseline Capture

Use the conservative accepted profile unless the experiment is explicitly
against another baseline.

```text
docs/kernel-dev/accepted_profile.env
tools/run_accepted_profile.sh <command> [args...]
```

## Minimum Acceptance Runs

DeltaNet+FFN prefill stack:

```text
tools/run_measurement_pack.sh acceptance /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

Long-context candidate:

```text
tools/run_measurement_pack.sh long-context /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

Use 7 iterations for close calls:

```text
tools/run_measurement_pack.sh candidate-7 /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

## Reporting Format

```text
command:
env:
modelpack:
roofline_env:
tokens/context:
iterations:
warmup:

baseline:
  median_s:
  p95_s:
  tok_s:
  checksum:

candidate:
  median_s:
  p95_s:
  tok_s:
  checksum:

roofline_gap:
  sustained_stream_GB_s:
  operational_prefill_matmul_GB_s:
  modeled_bytes:
  effective_GB/s:
  bandwidth_utilization:
  time_vs_floor:
  traffic_vs_model:
  classification:
  next_probe:

correctness:
  gate:
  mean_abs_error:
  rms_error:
  max_abs_error:
  checksum_delta:

decision:
```

For rejected or invalid runs, add:

```text
negative_learning:
  hypothesis:
  actual_result:
  failure_mode:
  root_cause:
  do_not_repeat:
  retry_only_if:
```

## Invalid Benchmark Conditions

Discard or mark non-comparable if:

```text
another benchmark was running
subagent ran GPU work
modelpack changed
env flags were not recorded
candidate skipped correctness
only one token length was measured for acceptance
only decode smoke length was used as performance proof
roofline baseline missing for a performance claim
bandwidth/compute utilization not reported
layout candidate was not compared across tile/chunk sizes
```
