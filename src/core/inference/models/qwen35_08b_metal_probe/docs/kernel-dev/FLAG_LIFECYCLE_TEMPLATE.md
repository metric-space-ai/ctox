# Env Flag Lifecycle Template

Use this for every new `CTOX_QWEN35_*` flag. The goal is to avoid hidden
defaults, stale experimental paths, and incompatible flag combinations.

## Flag

```text
name:
introduced_date:
owner:
status:
  experimental | accepted-default | deprecated | removed
default:
  off | on
```

## Purpose

```text
what_it_selects:
why_it_exists:
expected_win:
```

## Compatibility

```text
requires:
  -

incompatible_with:
  -

fallback_path:
  -
```

## Correctness Requirements

```text
operator_gate:
integrated_gate:
hidden_dump_gate:
logits_gate:
greedy_gate:
long_context_gate:
```

## Benchmark Requirements

```text
smoke_pack:
candidate_pack:
acceptance_pack:
long_context_pack:
```

## Promotion Criteria

```text
can_become_default_when:
  -

must_stay_opt_in_when:
  -

must_be_removed_when:
  -
```

## Current Evidence

```text
best_result:
worst_result:
known_regressions:
research_log_refs:
```

## Deprecation Plan

```text
if_replaced_by:
cleanup_files:
cleanup_tests:
removal_condition:
```
