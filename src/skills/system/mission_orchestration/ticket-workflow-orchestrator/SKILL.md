---
name: ticket-workflow-orchestrator
description: Use when a heavy or long-running CTOX task should be modeled as dynamic ticket workflow phases while preserving the existing serial queue, review gate, and responsiveness.
metadata:
  short-description: Orchestrate long tasks as dynamic ticket workflows
cluster: mission_orchestration
---

# Ticket Workflow Orchestrator

Use this skill when CTOX must handle a hard or long-running task as a durable, interruptible workflow without bypassing the existing ticket, queue, review, and serial harness controls.

## Operating Model

- A workflow is a graph of CTOX internal work items, not a separate scheduler.
- The parent ticket has kind `workflow-case`.
- Executable or planning units have kind `workflow-step`.
- Only ready `workflow-step` tickets are materialized into the normal serial queue.
- Founder or owner communication, urgent tickets, and already pending queue work can run between workflow steps.
- Never create a large hidden todo list inside one step. Persist the next bounded step or phase as tickets.

## Roles

- `reducer`: reads prior phase output, review notes, and evidence; then creates the next dynamic steps with `ctox ticket workflow-apply-delta`.
- `leaf`: executes exactly one bounded task and reports evidence. A leaf must not create sibling workflow tickets directly.

## Commands

Start a workflow:

```bash
ctox ticket workflow-start --title "<title>" --goal "<goal>" --thread-key "<scope>" --priority normal --queue-now
```

Create one manual step:

```bash
ctox ticket workflow-step-put --workflow-id "<workflow-id>" --phase "<phase>" --title "<title>" --body "<prompt>" --step-id "<stable-step-id>"
```

Apply a reducer delta:

```bash
ctox ticket workflow-apply-delta --workflow-id "<workflow-id>" --delta-json '<json>' --queue-now
```

Inspect:

```bash
ctox ticket workflow-show --workflow-id "<workflow-id>"
```

## Delta Schema

Reducers produce small deltas:

```json
{
  "phase_decision": "advance",
  "update_steps": [
    {
      "step_id": "phase-0-reducer",
      "workflow_step_status": "verified",
      "evidence": {
        "summary": "Planning phase produced executable implementation and verification steps."
      }
    }
  ],
  "create_steps": [
    {
      "step_id": "implement-queue-hook",
      "phase": "implementation",
      "role": "leaf",
      "title": "Implement queue hook",
      "prompt": "Make one scoped code change and run the focused check.",
      "predecessor_steps": ["phase-0-reducer"],
      "exit_gate": "Patch compiles or the failure is recorded with exact blocker output.",
      "priority": "normal"
    }
  ],
  "queue_now": ["implement-queue-hook"]
}
```

Keep deltas bounded:

- Create at most 16 steps in one reducer pass.
- Prefer 1 to 5 steps unless the phase has naturally independent bounded leaves.
- Use predecessor step ids for phase gates and predecessor work ids for exact item dependencies.
- Mark predecessor steps `verified` only when review, tests, or explicit evidence support it.
- If the next phase is unclear, create one reducer or clarification step instead of guessing.

## Prompt Discipline

For reducers:

- Summarize what the previous phase established.
- Identify only the next executable phase.
- Create tickets whose prompts include acceptance criteria and exit gates.
- Do not queue work that depends on missing evidence.

For leaves:

- Execute the assigned work only.
- Record concrete evidence in the final response or as a workflow delta candidate.
- If the phase needs to change, return `workflow_delta_candidate` JSON instead of spawning tickets directly.
- Yield after the bounded step so CTOX can reprioritize the queue.
