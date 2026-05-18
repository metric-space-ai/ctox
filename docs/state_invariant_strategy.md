# CTOX State Invariant Strategy

This is the direct deterministic complement to the benchmark path.

The goal is not to model every theoretical CTOX state. The goal is to pin down the small set of durable state relationships that must remain true even when prompts, models, timing, or restarts vary.

## Why This Exists

Bench-driven work found real problems, but it also has a cost:

- it is slower than direct state checks
- it can bias work toward benchmark symptoms
- it makes it harder to tell whether a failure is a runtime invariant break or just a model-quality miss

The invariant layer is meant to reduce that ambiguity.

## Design Rules

Every invariant must satisfy all of these:

1. It maps to a real CTOX failure class already seen in benchmarks, forensics, or production-like testing.
2. It is evaluated only from durable runtime state.
3. It is read-only. It must not silently repair state.
4. It has at least one deterministic test that reproduces the failure shape.
5. It is explainable in operator language without source-code knowledge.

If an invariant does not satisfy those rules, it should not exist.

## Initial Scope

The first invariant slice is intentionally narrow:

- `closed_mission_with_open_runtime_work`
- `idle_allowed_with_open_runtime_work`
- `mission_focus_head_mismatch`

These correspond directly to failure classes already surfaced by the split-brain and continuity corruption bench tasks.

## Success Criteria

This path is only worth expanding if it produces one of these outcomes:

- catches a known bench failure before an LLM run is needed
- turns a vague bench symptom into a concrete deterministic runtime defect
- reduces future benchmark forensics time
- prevents a real continuity or rehydrate regression with a targeted unit or transition test

## Complexity Guardrail

We should treat the invariant layer as overcomplicated if any of the following happen:

- new invariants are added without a concrete prior failure class
- invariants start depending on model phrasing instead of durable state
- the invariant set grows faster than the set of real regressions it prevents
- operators cannot tell what action an invariant violation implies

## How To Evaluate Whether This Is Working

For each new invariant, track:

- the bench or forensic case that motivated it
- the deterministic test that proves it
- whether it later caught a regression before the bench did

If an invariant never pays for itself, remove it.
