# Kernel Decision Record Template

Use this after an experiment has measurements. The goal is to make accept/reject
decisions searchable and reproducible.

## Decision

```text
date:
experiment:
decision: <accepted | rejected | opt-in | needs-more-data>
accepted_env:
rejected_env:
```

## Summary

```text
one_sentence:
  <what changed and why the decision was made>
```

## Learning Capture

```text
hypothesis:
  <what we expected>

actual_result:
  <what happened>

failure_mode:
  <slower runtime | p95 instability | correctness drift | compile failure |
   bandwidth underuse | scratch explosion | register pressure | other>

root_cause:
  <measured | inferred | unknown> - <short explanation>

do_not_repeat:
  <specific pattern future work should avoid>

retry_only_if:
  <condition that would make this idea worth trying again>
```

## Evidence

```text
model:
metalpack:
tokens/context:
iterations:
warmup:
baseline_command:
candidate_command:
forensics_command:
forensics_record:
search_based: <yes | no>
autotune_record:
```

## Performance

```text
baseline_median_s:
baseline_p95_s:
baseline_tok_s:

candidate_median_s:
candidate_p95_s:
candidate_tok_s:

median_delta_percent:
p95_delta_percent:
```

## Correctness

```text
checksum:
hidden_mean_abs_error:
hidden_rms_error:
hidden_max_abs_error:
checksum_delta:
greedy_tokens:
logits_check:
```

## Memory / Cache Interpretation

```text
modeled_bytes_baseline:
modeled_bytes_candidate:
weight_stream_delta:
scratch_delta:
tail_underfill:
dram_equivalent_bytes:
cache_miss_claim:
  inferred-only | hardware-counter-backed
```

Do not mark `hardware-counter-backed` unless the counter source is named and
captured.

## Why This Decision Is Safe

```text
correctness_gate:
integrated_path_gate:
token_sweep_gate:
reference_comparison:
```

## Follow-Up

```text
next_experiment:
cleanup:
docs_to_update:
handbook_update_required: <yes | no>
```
