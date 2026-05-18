# CTOX Prompt Hardening Proposal

Date: 2026-04-09

## Goal

Make CTOX prompts easier for weaker models to execute correctly without reducing CTOX features.

The target is not "simpler product behavior".
The target is "clearer operational contracts".

The prompt layer should no longer require the model to infer source-code semantics, internal CTOX jargon, or hidden runtime rules before it can solve the task.

## Why This Proposal Exists

Recent benchmark forensics show a pattern:

- `gpt-5.4-nano` often solves the domain problem reasonably.
- It then fails on CTOX operating semantics:
  - durable continuation not persisted in runtime state
  - open work described in prose but not actually left open
  - workspace-relative artifact grounding slips
  - plausible closure chosen where CTOX expected explicit bounded continuation

This is not primarily a loop-collapse pattern.
So far the visible failures are mostly:

- `failure_attribution = model`
- `loop_validity = true`

That means CTOX is still running, but the prompt contract is leaving too much room for plausible misreadings.

## Audit Scope

This proposal covers all active prompt surfaces found in the repo:

1. System prompt
   Source: `assets/prompts/ctox_chat_system_prompt.md`
2. Runtime prompt assembly
   Source: `src/context/live_context.rs`
3. Governance prompt block
   Source: `src/service/governance.rs`
4. Context-health prompt block
   Source: `src/context/context_health.rs`
5. Timeout continuation prompt
   Source: `src/service/service.rs`
6. Mission-idle continuation prompt
   Source: `src/service/service.rs`
7. Loop-repair prompt
   Source: `src/service/mission_governor.rs`
8. Follow-up prompt
   Source: `src/mission/follow_up.rs`
9. Plan step prompt
   Source: `src/mission/plan.rs`
10. Scheduled task prompt
    Source: `src/mission/schedule.rs`
11. Bench-generated task prompts
    Source: `scripts/benchmarks/ctox_bench_run.py`

## Core Diagnosis

The main prompt weakness is not lack of information.

It is the combination of:

- internal terminology
- multiple partially overlapping state blocks
- closure rules that are phrased semantically instead of operationally
- several plausible interpretations where only one is counted as correct

Weak models pay attention to decoding the prompt instead of executing the task.

## Prompt Design Rules Going Forward

These should become hard rules for CTOX prompts:

1. Do not use language that assumes source-code knowledge.
   If the model must know the code to understand the instruction, the prompt is under-specified.

2. Do not use internal CTOX jargon unless immediately translated into a visible action.
   Bad: `durable continuation`
   Good: `leave one open follow-up item in CTOX plan or queue before ending the turn`

3. Do not offer two similar plausible actions if only one counts as success.
   If prose mention does not count as open work, say so directly.

4. Every required runtime mutation must be stated as a concrete success condition.
   Example: `This task is not finished until an open plan or queue item exists in CTOX runtime state.`

5. Separate state description from action contract.
   Blocks like `Focus`, `Workflow state`, and `Context health` should not require inference to figure out what to do next.

6. Prefer simple operational language over taxonomic language.
   Use `current task`, `blocked by`, `do next`, `task is complete only when`.
   Avoid making weaker models decode labels like `turn_class`, `read_scope`, `slice_id`, `done_gate`, `closure_confidence`.

7. When a workspace path matters, say exactly what counts.
   Example: `Only files under this workspace count. Similar files elsewhere do not count.`

## Prompt Surface Review

### 1. System Prompt

Source: `assets/prompts/ctox_chat_system_prompt.md`

Strengths:

- strong emphasis on honest state and durable continuation
- good precedence model for context blocks
- good distinction between evidence and continuity

Risks for weaker models:

- too many internal terms early:
  - `slice`
  - `done gate`
  - `sidequest`
  - `compaction`
  - `read_scope`
  - `workflow state`
- uses conceptual explanations where weaker models need simple rules
- several rules are semantically correct but not operationally concrete

Proposal:

- keep the high-level structure, but rewrite the operating section in simpler language
- add one short explicit success contract:
  - finish the current task, or
  - leave exactly what remains open in CTOX runtime state
- add a direct line:
  - `A sentence in your reply does not count as open work. Open work counts only if it exists in CTOX plan, queue, follow-up, or schedule state.`

### 2. Runtime Prompt Assembly

Source: `src/context/live_context.rs`

This is the most important hardening surface.

#### 2a. `Focus`

Current risks:

- `turn_class`, `read_scope`, `slice_id`, and `status` are internal runtime labels, not plain work instructions
- `done_gate` is often readable only if the model already understands CTOX conventions
- `next_slice` can sound descriptive rather than mandatory

Proposal:

- rename the rendered block from `Focus:` to a more operational form while preserving schema internally
- render human-readable lines first:
  - `Current task:`
  - `Do now:`
  - `Blocked by:`
  - `Task is complete only when:`
  - `If unfinished, leave open work as:`
- keep internal IDs only in a smaller machine-like tail section if needed

Example direction:

```text
Current task:
- Main task: triage the real incident and keep noise separate
- Do now: write the incident note and progress note
- Task is complete only when:
  - both files exist in this workspace
  - one open follow-up item exists in CTOX runtime state for the deferred noise work
- If you mention follow-up only in text, the task is still unfinished
```

#### 2b. `Anchors`

Current risks:

- anchor constraints are rendered as compact codes like `workspace_only` or `main_mission_primary`
- these are readable, but they still require CTOX-specific interpretation

Proposal:

- keep compact codes only as a secondary list
- render plain-language constraints first:
  - `Only files inside the current workspace count`
  - `Do not let side work replace the main task`
  - `Update the canonical progress file first`

#### 2c. `Workflow state`

Current risks:

- empty state can be misread as "nothing needs to stay open"
- `leased`, `pending`, `blocked`, `failed` are runtime-specific meanings
- the model has to infer that it may need to create state right now
- filtered state is shown, but the prompt does not say what empty state means operationally

This likely explains several `nano` failures.

Proposal:

- prepend a plain-language contract above the rendered items:
  - `Open work required for this task: yes/no`
  - `If yes and no open item is listed below, create one before ending the turn`
  - `A reply, progress note, or artifact does not count as open work by itself`
  - `An item counts only if it appears in CTOX queue or plan state`
- when title constraints exist, render them in direct language:
  - `The open item title must include one of: customer follow-up, checkout incident, checkout recovery`

#### 2d. `Verified evidence`

Current risks:

- generally strong
- but the block is still sparse in some tasks compared with the importance of closure rules

Proposal:

- keep this surface mostly as-is
- optionally add one `verified success blocker` line when open work is required but unresolved

#### 2e. `Narrative`

Current risks:

- currently small and safe
- low priority

Proposal:

- keep compact
- do not let it compete with `Current task` and `Workflow state`

### 3. Governance Prompt Block

Source: `src/service/governance.rs`

Current risks:

- mechanism IDs are runtime-meaningful but not self-explanatory
- advisory/autonomous distinction is useful, but still abstract for smaller models

Proposal:

- keep mechanism IDs for traceability
- render a plain-language summary first:
  - `Runtime may automatically continue after timeout`
  - `Runtime may block secret handling`
  - `Runtime does not allow hidden side work`
- move raw mechanism IDs into a secondary section

### 4. Context Health Prompt Block

Source: `src/context/context_health.rs`

Current risks:

- warnings such as `mission_switch_pending`, `focus_document_thin`, `mission_contract_thin` are cryptic
- `critical` can distract weaker models even when the current slice should still proceed
- action labels like `rebuild_focus_anchors_narrative` are source-internal

This is too much decoding work for weaker models.

Proposal:

- replace warning codes in the main prompt with plain text
- show machine codes only secondarily if needed
- render only warnings that change current action
- rewrite actions into normal language:
  - `The current task may be mixing two different missions. Reconfirm the main task before continuing.`
  - `The current task contract is too thin. Restate what must be done now and what makes it complete.`

Priority rule:

- if `preempt_current_slice = no`, do not render the block in a way that sounds like a new main task

### 5. Timeout Continuation Prompt

Source: `src/service/service.rs`

Strengths:

- short
- operational

Risks:

- still uses `durable state` and `bounded continuation`
- does not explicitly define what counts as persisted continuation

Proposal:

- rewrite key line:
  - from `queue exactly one bounded continuation`
  - to `leave exactly one open follow-up item in CTOX plan or queue if the task still needs another turn`

### 6. Mission-Idle Continuation Prompt

Source: `src/service/service.rs`

Current risks:

- `Mode`, `Intensity`, `Done gate`, `Closure confidence` are useful but too taxonomic
- weaker models can overread the metadata and underread the concrete requirement

Proposal:

- keep only a short mission summary and the specific next step
- rewrite completion rule plainly:
  - `Do not end this turn with the mission still open unless one open follow-up item exists in CTOX runtime state.`

### 7. Loop-Repair Prompt

Source: `src/service/mission_governor.rs`

Current risks:

- comparatively good
- but still contains phrases like `minimum mission contract` and `bounded slice`

Proposal:

- simplify into direct language:
  - `State the real task`
  - `State why the last attempt failed`
  - `State one different next step`
  - `State what evidence would justify retrying the old approach`

### 8. Follow-Up Prompt

Source: `src/mission/follow_up.rs`

Current risks:

- too generic
- does not distinguish between narrative follow-up and runtime-persisted next work

Proposal:

- add a hard continuation contract:
  - `If any work remains open after this turn, keep exactly one open follow-up item and say what it is.`
- add workspace and state reminder when appropriate

### 9. Plan Step Prompt

Source: `src/mission/plan.rs`

Current risks:

- step prompts are simple, which is good
- but they do not explicitly say whether the step should leave open work or close cleanly

Proposal:

- add one line:
  - `If this step cannot fully finish the goal, leave the next step state explicit for the plan.`

### 10. Scheduled Task Prompt

Source: `src/mission/schedule.rs`

Current risks:

- currently extremely thin
- good for strong models
- weak models may miss workspace/runtime constraints if the scheduled prompt text itself is sparse

Proposal:

- prepend a standard operational shell:
  - current workspace
  - state counts only if persisted in CTOX runtime state
  - if work remains open, leave one explicit follow-up

### 11. Bench-Generated Task Prompts

Source: `scripts/benchmarks/ctox_bench_run.py`

Current strengths:

- concrete objectives
- explicit file paths
- explicit runtime queue/plan requirement

Current risks:

- still uses some CTOX-internal phrasing:
  - `durable continuation`
  - `bounded slice`
- success rules are partly distributed across several bullets
- a weaker model can still interpret prose and runtime state as equivalent

Proposal:

- add an explicit `Success conditions:` block
- add an explicit `This does not count:` block

Example:

```text
Success conditions:
- required files exist in this workspace
- required markers are present
- if open work is required, one open CTOX plan or queue item exists

This does not count:
- mentioning follow-up only in the reply
- writing a note about future work without creating open runtime state
- writing files outside this workspace
```

## Prompt Terms To Replace

These terms are not forbidden, but they should not appear without direct translation:

- `durable continuation`
  Replace with: `open follow-up item in CTOX plan or queue`
- `done gate`
  Replace with: `task is complete only when`
- `slice`
  Replace with: `current step` or `next step`
- `mission contract`
  Replace with: `main task and completion rule`
- `closure confidence`
  Replace with: `do not mark complete unless evidence is clear`
- `read_scope`
  Remove from the model-facing prompt, or hide behind simpler language
- `turn_class`
  Remove from the model-facing prompt
- `leased/routed`
  Translate into plain language:
  `this may no longer count as open handoff work`

## Main Hardening Proposal

### P0: Make completion contracts explicit

Files:

- `src/context/live_context.rs`
- `scripts/benchmarks/ctox_bench_run.py`
- `src/service/service.rs`

Changes:

- add a plain-language `Completion contract` section
- explicitly state whether open runtime work is required
- explicitly state that prose does not count as runtime state
- explicitly state that only workspace files count

### P1: Remove CTOX-internal decoding burden

Files:

- `assets/prompts/ctox_chat_system_prompt.md`
- `src/context/live_context.rs`
- `src/context/context_health.rs`
- `src/service/governance.rs`

Changes:

- reduce internal jargon in the primary prompt path
- render human-readable action guidance before internal labels
- demote machine-like codes to secondary lines or omit them

### P2: Align all continuation prompts

Files:

- `src/service/service.rs`
- `src/service/mission_governor.rs`
- `src/mission/follow_up.rs`
- `src/mission/plan.rs`
- `src/mission/schedule.rs`

Changes:

- standardize follow-up wording
- standardize "what counts as open work"
- standardize "only one explicit open continuation if work remains"

## Recommended Implementation Order

1. Harden `Focus` and `Workflow state` in `src/context/live_context.rs`
2. Simplify `Context health` language in `src/context/context_health.rs`
3. Rewrite the system prompt operating contract in `assets/prompts/ctox_chat_system_prompt.md`
4. Normalize continuation wording in service/follow-up/plan/schedule prompts
5. Align bench-generated prompts with the same simple language

## Success Criteria For Prompt Hardening

The prompt hardening work is successful when weaker models stop failing because they chose a plausible but non-counting interpretation.

Concrete expected improvements:

- fewer failures of the form:
  - "I documented the follow-up but did not persist it"
  - "I wrote the right file but outside the counted workspace"
  - "I believed routed/leased work still counted as open"
  - "I reasonably closed the task even though CTOX required explicit open continuation"
- no increase in bench reliance on hidden source-code semantics
- clearer separation between:
  - task solved
  - task solved and runtime state persisted

## Summary

CTOX does not currently look too complex for weaker models because of feature breadth alone.

It looks too inference-heavy at the prompt layer.

The hardening direction should therefore be:

- less internal taxonomy
- less semantic guesswork
- fewer plausible-but-wrong alternatives
- more explicit operational success conditions

That is how CTOX can stay fully featured while becoming more robust for smaller models.
