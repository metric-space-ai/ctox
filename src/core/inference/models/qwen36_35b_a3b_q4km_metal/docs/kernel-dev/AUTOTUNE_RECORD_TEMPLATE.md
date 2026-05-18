# Autotune Record Template

Use this for any layout, tile, chunk, dispatch, or env-flag search whose result
may become a candidate default.

## Metadata

```text
date:
experiment:
parameter_family:
model:
metalpack:
binary_path:
output_csv:
tokens/context:
iterations:
warmup:
```

## Search Space

```text
search_space:
candidate_count:
baseline_selection:
best_selection:
chosen_env:
rejected_candidates_path:
```

## Metrics

```text
selection_metric:
baseline_median_s:
baseline_p95_s:
baseline_tok_s:
best_median_s:
best_p95_s:
best_tok_s:
median_delta_percent:
p95_delta_percent:
```

## Correctness

```text
correctness_gate:
hidden_mean_abs_error:
hidden_rms_error:
hidden_max_abs_error:
checksum_delta:
token_sweep_gate:
```

## Interpretation

```text
why_best_won:
why_others_lost:
cache_forensics_record:
risk:
decision:
  accepted | rejected | opt-in | needs-more-data
next_action:
```
